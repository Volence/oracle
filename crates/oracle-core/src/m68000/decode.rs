//! Opcode → micro-op recipe (decode), and the instruction-level [`Cpu68000`] entry points.
//!
//! `decode` maps the opcode in the prefetch queue to its [`MicroState`] recipe; the two `Cpu68000`
//! methods tie decode to the framework's two drivers (run-to-completion fast path / step-one-micro-op
//! quiesce). Decodes the `ADD`/`SUB` families in word, byte and long sizes so far (the shared `arith_ea_dn`
//! / `arith_dn_ea` builders are parameterized by `AluOp` and `Size`); the full 65536-entry dispatch (one
//! builder per instruction family) lands with full coverage.

use super::bus68k::Bus68k;
use super::ea::{
    ea_cmpa, ea_dst, ea_move, ea_movea, ea_read_word_operand, ea_src, ea_tas, ea_tst, RecipeBuf,
};
use super::exception::{push_standard_frame, vector_fetch_and_reload};
use super::microop::{
    condition_true, AluOp, Cpu68000, Dest, LogicOp, MicroOp, MicroState, Operand, Size,
};
use super::registers::{Registers, CCR_V};

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

/// Whether an EA `mode`/`reg` pair is a **control** addressing mode — the modes that name a memory location
/// WITHOUT the register-updating auto-(in/de)crement, the set JMP/JSR/LEA/PEA accept: `(An)` (010),
/// `(d16,An)` (101), `(d8,An,Xn)` (110), `abs.w` (111/000), `abs.l` (111/001), `(d16,PC)` (111/010),
/// `(d8,PC,Xn)` (111/011). Excludes `Dn`/`An` direct (000/001), `(An)+`/`-(An)` (011/100), `#imm` (111/100).
#[inline]
fn is_control_mode(mode: u16, reg: u16) -> bool {
    matches!(mode, 2 | 5 | 6) || (mode == 7 && matches!(reg, 0..=3))
}

/// Whether an EA `mode`/`reg` pair is a valid `MOVEM` addressing mode for direction `dir` (0 = reg→mem,
/// 1 = mem→reg). reg→mem = control-alterable + predecrement: `(An)` (2), `-(An)` (4), `d16(An)` (5),
/// `d8(An,Xn)` (6), `abs.w` (7/0), `abs.l` (7/1) — NO PC-relative, NO `(An)+`. mem→reg = control +
/// postincrement + PC-relative: `(An)` (2), `(An)+` (3), `d16(An)` (5), `d8(An,Xn)` (6), `abs.w` (7/0),
/// `abs.l` (7/1), `d16(PC)` (7/2), `d8(PC,Xn)` (7/3) — NO `-(An)`.
#[inline]
fn movem_mode_ok(dir: u16, mode: u16, reg: u16) -> bool {
    if dir == 0 {
        // reg→mem: control-alterable + predecrement (no PC, no (An)+).
        matches!(mode, 2 | 4 | 5 | 6) || (mode == 7 && (reg == 0 || reg == 1))
    } else {
        // mem→reg: control + postincrement + PC-relative (no -(An)).
        matches!(mode, 2 | 3 | 5 | 6) || (mode == 7 && matches!(reg, 0..=3))
    }
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

/// Which member of the `1011`-prefixed compare family an opcode is — the load-bearing classifier shared by
/// [`decode`] and the SST runner's `covered()` filter. **The `CMP.<sz>.json` vendored files are 3-way
/// mixes** (CMP `<ea>,Dn` + CMPM `(Ay)+,(Ax)+` + CMPI `#imm,<ea>`), all mislabeled `"CMP.<sz>"` in the
/// `name` field — so a case MUST be classified by its **opcode**, never by name. Classifying by opcode:
///
/// - **CMPI** `0000 1100 SS mmm rrr` (high byte `0x0C`, the `0000`-prefixed immediate space). Tested FIRST
///   (it is not in the `0xB` nibble at all, but kept here so the one classifier covers every CMP-file class).
/// - **CMPM** `1011 xxx 1SS 001 yyy` — nibble `0xB` with the opmode bit2 set (opmode 4/5/6 → b/w/l) and the
///   EA mode field forced to `001` (`(An)+`).
/// - **CMP** `1011 ddd 0SS mmm rrr` — nibble `0xB`, opmode 0/1/2 (b/w/l), `Dn − <ea>`.
/// - **CMPA** `1011 aaa 0 11/111 mmm rrr` — nibble `0xB`, opmode 3 (`.w`) or 7 (`.l`).
/// - **None** for any non-`0xB`, non-CMPI opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpClass {
    Cmp,
    Cmpm,
    Cmpi,
    Cmpa,
    None,
}

/// Classify an opcode into its [`CmpClass`] — see that type's docs. Shared by [`decode`] (which CMP arm to
/// build) and the SST runner's `covered()` (which CMP-file cases are in scope this commit). Classifying by
/// OPCODE — not the misleading `name` field — is the central correctness pin of the CMP family.
#[inline]
pub fn cmp_class(opcode: u16) -> CmpClass {
    // CMPI: `0000 1100 SS mmm rrr` — high byte 0x0C (the 0000-prefixed immediate space).
    if opcode >> 8 == 0x0C {
        return CmpClass::Cmpi;
    }
    if opcode >> 12 == 0b1011 {
        return match (opcode >> 6) & 7 {
            // opmode bit2 set with EA mode field 001 → CMPM (Ay)+,(Ax)+ (opmode 4/5/6 = b/w/l).
            4..=6 if (opcode >> 3) & 7 == 1 => CmpClass::Cmpm,
            // opmode 0/1/2 → CMP <ea>,Dn (b/w/l).
            0..=2 => CmpClass::Cmp,
            // opmode 3/7 → CMPA <ea>,An (.w/.l).
            3 | 7 => CmpClass::Cmpa,
            _ => CmpClass::None,
        };
    }
    CmpClass::None
}

/// Classify a group-0 immediate-to-EA opcode (`0000 hhhh ss mmm rrr`) into its ALU op + operand size, or
/// `None` if it is not an in-scope `*I` instruction. The shared classifier for the `imm_rmw_recipe` family —
/// `ADDI`/`SUBI`/`ANDI`/`ORI`/`EORI`, each hiding as a contaminant inside an already-vendored register-form
/// file (ADD/SUB/AND/OR/EOR). Classifying by OPCODE keeps [`decode`] and the SST runner's `covered()` in
/// agreement (the `cmp_class` pattern).
///
/// The high byte selects the op: `0x06 → Add` (ADDI), `0x04 → Sub` (SUBI), `0x02 → And` (ANDI), `0x00 → Or`
/// (ORI), `0x0A → Eor` (EORI). All five `*I` forms are now classified. The size field `ss = (opcode >>
/// 6) & 3` gives `0 → Byte`, `1 → Word`, `2 → Long`; `ss == 3` (a `.l`-space CMPI-style code) is NOT an `*I`
/// form and returns `None`. This classifier does NOT filter the destination EA mode — the caller pairs it with
/// a data-alterable guard (`mode == 0 || is_dst_mem_mode`), which excludes the `*toSR`/`*toCCR` mode-7/4 `#imm`
/// single points (0x023C ANDItoCCR / 0x027C ANDItoSR, 0x003C ORItoCCR / 0x007C ORItoSR, and 0x0A3C EORItoCCR /
/// 0x0A7C EORItoSR are mode 7/4 `#imm`, excluded by the guard and living in their own files; 0x06/0x04 have
/// none, but the guard is uniform across the family).
#[inline]
pub fn imm_class(opcode: u16) -> Option<(AluOp, Size)> {
    let ss = (opcode >> 6) & 3;
    if ss == 3 {
        return None;
    }
    let op = match (opcode >> 8) & 0xFF {
        0x06 => AluOp::Add, // ADDI
        0x04 => AluOp::Sub, // SUBI (EA/Dn is the minuend `a`, the immediate the subtrahend `b`)
        0x02 => AluOp::And, // ANDI (logic flags: N/Z set, V = C = 0, X preserved)
        0x00 => AluOp::Or,  // ORI (logic flags: N/Z set, V = C = 0, X preserved)
        0x0A => AluOp::Eor, // EORI (logic flags: N/Z set, V = C = 0, X preserved)
        _ => return None,
    };
    let size = match ss {
        0 => Size::Byte,
        1 => Size::Word,
        _ => Size::Long,
    };
    Some((op, size))
}

/// Decode the opcode currently in `regs.prefetch[0]` into its micro-op recipe, latching the original opcode
/// into the recipe ([`MicroState::set_opcode`]) so the address-error abort (E3) can stack it as the IR /
/// SSW fields after the prefetch shifts have overwritten `regs.prefetch`.
#[inline]
pub fn decode(regs: &Registers) -> MicroState {
    let mut state = decode_dispatch(regs);
    state.set_opcode(regs.prefetch[0]);
    state
}

/// The opcode → recipe dispatch (the `decode` body, wrapped by [`decode`] so every recipe is opcode-latched).
#[inline]
fn decode_dispatch(regs: &Registers) -> MicroState {
    let opcode = regs.prefetch[0];
    // Privilege gate (M68000UM §6.3.7): a privileged instruction attempted in user mode traps to the
    // privilege-violation vector (8) BEFORE any execution — the check is at decode time (registers are
    // available). Every vendored SST case is supervisor, so this never fires on the gate-keeping suite;
    // it is exercised only by the hand-authored A1 vectors. Placed first so no privileged arm below runs.
    if !regs.supervisor() && is_privileged_opcode(opcode) {
        return decode_time_exception_recipe(8); // privilege violation
    }
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
    // ADDX.b/.w/.l (`1101 xxx 1 SS 00 M yyy`, `opcode & 0xF130 == 0xD100`, size = bits 7-6 != 3) — the
    // EXTENDED (multi-precision) add `Dx = Dx + Dy + X` (M = bit3 = 0, register-direct) / `-(Ax) = -(Ax) +
    // -(Ay) + X` (M = 1, two-operand predecrement). The mask fixes bits 15-12 = 1101, bit 8 = 1, bits 5-4 = 00
    // (ea-mode field 000/001) — the ADD `Dn,<ea>` direction's ea-modes 000/001 are illegal as a real dest, so
    // the ISA repurposes this slot as ADDX. Placed BEFORE the ADD.b arm: the ADD.b `Dn,<ea>` arm below is
    // guarded by `is_dst_mem_mode` (alterable memory only, modes 2-6/7-0/7-1), so it can never swallow the
    // ADDX modes 000/001 anyway, but classifying ADDX first keeps the intent explicit. size = bits 7-6 (3 =
    // `ADDA`, excluded). `Rx = (op>>9)&7` (dst), `Ry = op&7` (src), `M = bit3`.
    if opcode & 0xF130 == 0xD100 && (opcode >> 6) & 3 != 3 {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // 2
        };
        return xarith_recipe(opcode, AluOp::Addx, size);
    }
    // SUBX.b/.w/.l (`1001 xxx 1 SS 00 M yyy`, `opcode & 0xF130 == 0x9100`, size = bits 7-6 != 3) — the
    // EXTENDED (multi-precision) subtract `Dx = Dx − Dy − X` (M = bit3 = 0, register-direct) / `-(Ax) =
    // -(Ax) − -(Ay) − X` (M = 1, two-operand predecrement). The `0x9` (SUB) space twin of ADDX with the
    // identical bit layout: the mask fixes bits 15-12 = 1001, bit 8 = 1, bits 5-4 = 00 (ea-mode field
    // 000/001) — the SUB `Dn,<ea>` direction's ea-modes 000/001 are illegal as a real dest, so the ISA
    // repurposes this slot as SUBX. Placed BEFORE the SUB.b arm below (guarded by `is_dst_mem_mode`, which
    // already excludes modes 000/001 — classifying SUBX first keeps the intent explicit). size = bits 7-6
    // (3 = `SUBA`, excluded). Reuses `xarith_recipe` with the dedicated `AluOp::Subx` (X into the value AND
    // the borrow, sticky Z). `Rx = (op>>9)&7` (dst), `Ry = op&7` (src), `M = bit3`.
    if opcode & 0xF130 == 0x9100 && (opcode >> 6) & 3 != 3 {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // 2
        };
        return xarith_recipe(opcode, AluOp::Subx, size);
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
    // ADDA `<ea>,An` (`1101 aaa s11 mmm rrr`, opmode 3 = `.w` = 0xD0C0 / opmode 7 = `.l` = 0xD1C0) — address
    // arithmetic `An = An + src`, NO flags (SR untouched). `.w` sign-extends the source word→long before the
    // long-boundary add (`AluOp::Adda` does this internally, mirroring `AluOp::MoveA`); `.l` adds the full 32.
    // All 12 source modes legal (An-direct included — it is address arithmetic). The opcode space (nibble 0xD,
    // opmode 3/7) is disjoint from the ADD arms above (opmode 0/1/2/4/5/6) and the CMP/CMPA arms (nibble 0xB).
    if opcode & 0xF1C0 == 0xD0C0 {
        return adda_suba_recipe(opcode, AluOp::Adda, Size::Word); // ADDA.w <ea>,An
    }
    if opcode & 0xF1C0 == 0xD1C0 {
        return adda_suba_recipe(opcode, AluOp::Adda, Size::Long); // ADDA.l <ea>,An
    }
    // SUBA `<ea>,An` (`1001 aaa s11 mmm rrr`, opmode 3 = `.w` = 0x90C0 / opmode 7 = `.l` = 0x91C0) — address
    // arithmetic `An = An − src`, NO flags (SR untouched), a near-exact mirror of ADDA. `.w` sign-extends the
    // source word→long before the long-boundary subtract (`AluOp::Suba` does this internally, mirroring
    // `AluOp::MoveA`); `.l` subtracts the full 32. All 12 source modes legal (An-direct included — it is address
    // arithmetic). The opcode space (nibble 0x9, opmode 3/7) is disjoint from the SUB arms above (opmode
    // 0/1/2/4/5/6) and the CMP/CMPA arms (nibble 0xB). Reuses the AluOp-parameterized `adda_suba_recipe`.
    if opcode & 0xF1C0 == 0x90C0 {
        return adda_suba_recipe(opcode, AluOp::Suba, Size::Word); // SUBA.w <ea>,An
    }
    if opcode & 0xF1C0 == 0x91C0 {
        return adda_suba_recipe(opcode, AluOp::Suba, Size::Long); // SUBA.l <ea>,An
    }
    // EXG (`1100 rrr 1 ppppp rrr`, `opcode & 0xF100 == 0xC100`) — exchange two whole 32-bit registers, NO
    // flags. The opmode field `(opcode >> 3) & 0x1F` selects the form: 0x08 = Dx,Dy / 0x09 = Ax,Ay / 0x11 =
    // Dx,Ay. This arm is placed BEFORE the broad 0xCxxx AND/MUL arms below because the 0xCxxx space is SHARED
    // with ABCD (opmode 0x00/0x01) and `AND Dn,<ea>`: the opmode guard {0x08, 0x09, 0x11} is LOAD-BEARING — it
    // classifies STRICTLY by the three EXG opmodes so no ABCD/AND opcode reaches the EXG arm (ABCD is not
    // implemented) and no EXG opcode reaches the AND arm's `todo!()`. `rx = (op>>9)&7`, `ry = op&7`; A7 legs
    // hit the active `addr_reg(7)` (ssp/usp per S). Recipe `[Prefetch, Internal(2), ExgRegs]`, length 6.
    if opcode & 0xF100 == 0xC100 && matches!((opcode >> 3) & 0x1F, 0x08 | 0x09 | 0x11) {
        return exg_recipe(opcode);
    }
    // ABCD (`1100 xxx 1 0000 M yyy`, `opcode & 0xF1F0 == 0xC100`) — the packed-decimal add `Dx = Dx +₁₀ Dy + X`
    // (M = bit3 = 0, register-direct) / `-(Ax) = -(Ax) +₁₀ -(Ay) + X` (M = 1, two-operand predecrement),
    // byte-only. The `0xF1F0` mask fixes bits 15-12 = 1100, bit 8 = 1, bits 7-4 = 0000 (byte size + bits 5-4 =
    // 00, ea-mode field 000/001) — leaving M (3), Rx = (op>>9)&7 (dst), Ry = op&7 (src). DISJOINT from EXG
    // (opmode {0x08,0x09,0x11}, handled above) and from real `AND.b Dn,<ea>` (ea modes 2-7 give bits 5-4 != 00,
    // caught by the `is_dst_mem_mode` guard below). Reuses `xarith_recipe` with the dedicated `AluOp::Abcd` (X
    // into the value AND the carry, sticky Z, the decimal correction). Register-direct = 6 cyc, -(Ay),-(Ax) =
    // 18; byte-only so NO alignment fault path.
    if opcode & 0xF1F0 == 0xC100 {
        return xarith_recipe(opcode, AluOp::Abcd, Size::Byte);
    }
    // AND `<ea>,Dn` (`1100 ddd 0SS mmm rrr`, opmode 0/1/2 = b/w/l = 0xC000/0xC040/0xC080) — bitwise `Dn = Dn &
    // <ea>` (Dn the minuend `a`; AND is commutative so operand order is inert). Source = data modes; An-direct
    // (mode 1) is ILLEGAL/absent (the `arith_ea_dn` arm relies on `covered()` never feeding it mode 1, exactly
    // like the ADD.b precedent). Sets N = msb / Z = (result == 0), clears V/C, PRESERVES X (`AluOp::And` =
    // `move_flags` + X re-injected). Reuses the AluOp-parameterized `arith_ea_dn` VERBATIM — AND <ea>,Dn = ADD
    // <ea>,Dn byte-for-byte (same `ea_src` skeleton, same cycle counts), minus the illegal An source. The
    // **AND.* files MIX** this genuine register form (nibble 0xC) with the dedicated ANDI immediate opcode
    // (`0x02xx`, high nibble 0) — a DIFFERENT instruction NOT decoded this push; `covered()` classifies the
    // ANDI cases OUT by OPCODE (high nibble 0 != 0xC), so decode is only ever reached on the genuine 0xC form.
    // The opcode space (nibble 0xC, opmode 0/1/2) is disjoint from the ADD/SUB arms (nibble 0xD/0x9) and the
    // CMP arms (nibble 0xB). opmode 3/7 (0xC0C0/0xC1C0) is MULU/MULS — not matched by these masks.
    if opcode & 0xF1C0 == 0xC000 {
        return arith_ea_dn(opcode, AluOp::And, Size::Byte); // AND.b <ea>,Dn
    }
    if opcode & 0xF1C0 == 0xC040 {
        return arith_ea_dn(opcode, AluOp::And, Size::Word); // AND.w <ea>,Dn
    }
    if opcode & 0xF1C0 == 0xC080 {
        return arith_ea_dn(opcode, AluOp::And, Size::Long); // AND.l <ea>,Dn
    }
    // AND `Dn,<ea>` (`1100 ddd 1SS mmm rrr`, opmode 4/5/6 = b/w/l = 0xC100/0xC140/0xC180) — bitwise `<ea> =
    // <ea> & Dn` to an alterable-memory destination (modes 2..6, abs.w/abs.l via `is_dst_mem_mode`). Mode
    // 000/001 = ABCD/EXG (a DIFFERENT instruction) is RESERVED — the `is_dst_mem_mode` guard excludes it (it
    // admits only alterable memory), so an ABCD/EXG opcode is never decoded as AND. Reuses `arith_dn_ea`
    // VERBATIM — AND Dn,<ea> = ADD Dn,<ea> byte-for-byte (the same `ea_dst`/`ea_dst_long` RMW skeleton). The
    // memory operand is the minuend (`a`); AND is commutative so the order is correct.
    if opcode & 0xF1C0 == 0xC100 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::And, Size::Byte); // AND.b Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xC140 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::And, Size::Word); // AND.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xC180 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::And, Size::Long); // AND.l Dn,<ea>
    }
    // MULU `<ea>,Dn` (`1100 ddd 011 mmm rrr`, opcode & 0xF1C0 == 0xC0C0, opmode 3 in the 0xC space) — the
    // 16×16→32 UNSIGNED multiply `Dn = (Dn & 0xFFFF) * (src & 0xFFFF)` (the full 32-bit product). opmode 3 is
    // DISJOINT from AND's opmode 0/1/2 (`<ea>,Dn`) and 4/5/6 (`Dn,<ea>`) above, so this arm is reached only on
    // the genuine MULU encoding. Reuses `ea_src(Size::Word)` (the CHK machinery, all 11 source modes incl
    // PC-relative/#imm + the odd-EA address error). The `AluOp::Mulu` exec arm RETURNS its data-dependent cycle
    // cost (`34 + 2*popcount(src16)`) — the count comes from the resolved multiplier (exec-time, not decode).
    if opcode & 0xF1C0 == 0xC0C0 {
        return mul_recipe(opcode, AluOp::Mulu);
    }
    // MULS `<ea>,Dn` (`1100 ddd 111 mmm rrr`, opcode & 0xF1C0 == 0xC1C0, opmode 7 in the 0xC space) — the
    // 16×16→32 SIGNED multiply `Dn = sx16(Dn) * sx16(src)` (the full 32-bit two's-complement product). opmode 7
    // is DISJOINT from AND's opmode 0/1/2/4/5/6 AND from MULU's opmode 3 above, so this arm is reached only on
    // the genuine MULS encoding. REUSES `mul_recipe` VERBATIM (just passing `AluOp::Muls`): the same
    // `ea_src(Size::Word)` machinery CHK uses, all 11 source modes incl PC-relative/#imm + the odd-EA address
    // error. The `AluOp::Muls` exec arm RETURNS its data-dependent cycle cost (`34 + 2*booth(src16)` — the Booth
    // TRANSITION count of `src << 1`, NOT MULU's popcount), resolved from the multiplier at exec.
    if opcode & 0xF1C0 == 0xC1C0 {
        return mul_recipe(opcode, AluOp::Muls);
    }
    // DIVU `<ea>,Dn` (`1000 ddd 011 mmm rrr`, opcode & 0xF1C0 == 0x80C0, opmode 3 in the 0x8 space) — the
    // UNSIGNED 16-bit divide: the FULL 32-bit Dn (the dividend) is divided by the word source (the divisor),
    // writing quotient → low 16 of Dn, remainder → high 16. opmode 3 is DISJOINT from OR's opmode 0/1/2
    // (`<ea>,Dn`) and 4/5/6 (`Dn,<ea>`) below, so this arm is reached only on the genuine DIVU encoding (placed
    // BEFORE the OR arms so OR never swallows it). Reuses `ea_src(Size::Word)` (the CHK/MUL machinery, all 11
    // source modes incl PC-relative/#imm + the odd-EA address error). The `AluOp::Divu` exec arm RETURNS its
    // data-dependent division cost (normal `76 + 2*n_keep + 4*n_restore` / overflow `10`, minus the trailing
    // refill), or on a zero divisor rewrites the in-flight MicroState into the vector-5 divide-by-zero frame.
    if opcode & 0xF1C0 == 0x80C0 {
        return div_recipe(opcode, AluOp::Divu);
    }
    // DIVS `<ea>,Dn` (`1000 ddd 111 mmm rrr`, opcode & 0xF1C0 == 0x81C0, opmode 7 in the 0x8 space) — the SIGNED
    // 16-bit divide, the signed twin of DIVU: the FULL 32-bit Dn (the dividend, `sx32`) is divided by the
    // sign-extended word source (the divisor, `sx16`), truncating toward zero (remainder takes the dividend's
    // sign), writing quotient → low 16 of Dn, remainder → high 16. opmode 7 is DISJOINT from OR's opmode 0/1/2
    // and 4/5/6 AND from DIVU's opmode 3 above, so this arm is reached only on the genuine DIVS encoding (placed
    // BEFORE the OR arms so OR never swallows it). REUSES `div_recipe` VERBATIM (just passing `AluOp::Divs`) —
    // the same `ea_src(Size::Word)` machinery + the `[Alu, Prefetch]` swap + the vector-5 div0 frame. The
    // `AluOp::Divs` exec arm RETURNS its data-dependent division cost (normal `110 + 2*n_restore + sign terms` /
    // overflow `16|18`, minus the trailing refill), or on a zero divisor installs the vector-5 div0 frame.
    if opcode & 0xF1C0 == 0x81C0 {
        return div_recipe(opcode, AluOp::Divs);
    }
    // SBCD (`1000 xxx 1 0000 M yyy`, `opcode & 0xF1F0 == 0x8100`) — the packed-decimal subtract `Dx = Dx −₁₀ Dy
    // − X` (M = bit3 = 0, register-direct) / `-(Ax) = -(Ax) −₁₀ -(Ay) − X` (M = 1, two-operand predecrement),
    // byte-only. The `0x8` (OR) space twin of ABCD with the identical bit layout: the `0xF1F0` mask fixes bits
    // 15-12 = 1000, bit 8 = 1, bits 7-4 = 0000 (byte size + bits 5-4 = 00, ea-mode field 000/001) — leaving M
    // (3), Rx = (op>>9)&7 (dst), Ry = op&7 (src). Placed BEFORE the OR arms below: real `OR.b Dn,<ea>` (opmode
    // 4, 0x8100) is guarded by `is_dst_mem_mode` (alterable memory, modes 2-7 → bits 5-4 != 00), which already
    // excludes SBCD's modes 000/001 — classifying SBCD first keeps the intent explicit and DISJOINT from
    // DIVU/DIVS (opmode 3/7, matched above). Reuses `xarith_recipe` with the dedicated `AluOp::Sbcd` (X into the
    // value AND the borrow, sticky Z, the decimal correction with the carry/result asymmetry). Register-direct =
    // 6 cyc, -(Ay),-(Ax) = 18; byte-only so NO alignment fault path.
    if opcode & 0xF1F0 == 0x8100 {
        return xarith_recipe(opcode, AluOp::Sbcd, Size::Byte);
    }
    // OR `<ea>,Dn` (`1000 ddd 0SS mmm rrr`, opmode 0/1/2 = b/w/l = 0x8000/0x8040/0x8080) — bitwise `Dn = Dn |
    // <ea>` (Dn the minuend `a`; OR is commutative so operand order is inert). Identical to the AND `<ea>,Dn`
    // arms above with `AluOp::Or` and the base nibble 0x8 instead of 0xC — same `arith_ea_dn` builder VERBATIM
    // (OR <ea>,Dn = ADD <ea>,Dn = AND <ea>,Dn byte-for-byte), same illegal An-direct (mode 1) source absent
    // (`covered()` never feeds it mode 1). Sets N = msb / Z = (result == 0), clears V/C, PRESERVES X
    // (`AluOp::Or` = `move_flags` + X re-injected). The **OR.* files MIX** this genuine register form (nibble
    // 0x8) with the dedicated ORI immediate opcode (`0x00xx`, high nibble 0) — a DIFFERENT instruction NOT
    // decoded this push; `covered()` classifies the ORI cases OUT by OPCODE (high nibble 0 != 0x8), so decode
    // is only ever reached on the genuine 0x8 form. The opcode space (nibble 0x8, opmode 0/1/2) is disjoint
    // from the ADD/SUB arms (0xD/0x9), the AND arms (0xC) and the CMP arms (0xB). opmode 3/7 (0x80C0/0x81C0)
    // is DIVU/DIVS — not matched by these masks.
    if opcode & 0xF1C0 == 0x8000 {
        return arith_ea_dn(opcode, AluOp::Or, Size::Byte); // OR.b <ea>,Dn
    }
    if opcode & 0xF1C0 == 0x8040 {
        return arith_ea_dn(opcode, AluOp::Or, Size::Word); // OR.w <ea>,Dn
    }
    if opcode & 0xF1C0 == 0x8080 {
        return arith_ea_dn(opcode, AluOp::Or, Size::Long); // OR.l <ea>,Dn
    }
    // OR `Dn,<ea>` (`1000 ddd 1SS mmm rrr`, opmode 4/5/6 = b/w/l = 0x8100/0x8140/0x8180) — bitwise `<ea> =
    // <ea> | Dn` to an alterable-memory destination (modes 2..6, abs.w/abs.l via `is_dst_mem_mode`). Mode
    // 000/001 = SBCD/PACK (a DIFFERENT instruction) is RESERVED — the `is_dst_mem_mode` guard excludes it (it
    // admits only alterable memory), so an SBCD/PACK opcode is never decoded as OR. Reuses `arith_dn_ea`
    // VERBATIM — OR Dn,<ea> = ADD Dn,<ea> = AND Dn,<ea> byte-for-byte (the same `ea_dst`/`ea_dst_long` RMW
    // skeleton). The memory operand is the minuend (`a`); OR is commutative so the order is correct.
    if opcode & 0xF1C0 == 0x8100 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Or, Size::Byte); // OR.b Dn,<ea>
    }
    if opcode & 0xF1C0 == 0x8140 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Or, Size::Word); // OR.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0x8180 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_dn_ea(opcode, AluOp::Or, Size::Long); // OR.l Dn,<ea>
    }
    // CMP `<ea>,Dn` (`1011 ddd 0SS mmm rrr`, nibble 0xB, opmode 0/1/2 = b/w/l) — the flag-only compare
    // `Dn − <ea>` (Dn the minuend). Classified by OPCODE (the CMP.* files mix CMP/CMPM/CMPI — CMPM/CMPI are
    // N1/N2, and `covered()` admits only the Cmp class this commit, so decode is reached on Cmp cases only).
    // All 12 source modes via `ea_src` (An-direct legal for w/l, illegal/absent for .b). Sets N/Z/V/C as SUB
    // but PRESERVES X and writes nothing (`AluOp::Cmp` + `Dest::None`). The opcode space is disjoint from the
    // ADD/SUB arms above (nibble 0x9/0xD) and the immediate-to-SR / 0x4Exx arms below.
    if matches!(cmp_class(opcode), CmpClass::Cmp) {
        let size = match (opcode >> 6) & 7 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // opmode 2
        };
        return cmp_ea_dn(opcode, size);
    }
    // CMPM `(Ay)+,(Ax)+` (`1011 xxx 1SS 001 yyy`, opmode 4/5/6 = b/w/l, EA mode field forced to 001) — compare
    // memory: two post-increment reads (src @ `(Ay)+` FIRST, then dst @ `(Ax)+`), then the flag-only
    // `(Ax) − (Ay)` (`AluOp::Cmp` + `Dest::None`). The Cmpm class of the 3-way CMP.* mix (classified by OPCODE).
    // `xxx` (bits 11-9) = Ax (dest), `yyy` (bits 2-0) = Ay (src).
    if matches!(cmp_class(opcode), CmpClass::Cmpm) {
        let size = match (opcode >> 6) & 7 {
            4 => Size::Byte,
            5 => Size::Word,
            _ => Size::Long, // opmode 6
        };
        return cmpm_recipe(opcode, size);
    }
    // CMPI `#imm,<ea>` (`0000 1100 SS mmm rrr`, high byte 0x0C, SS bits 7-6 = b/w/l) — compare the
    // data-alterable EA against an immediate: capture the immediate (one ext word for b/w, TWO for `.l`)
    // FIRST, then read the EA (discarded — NO write), then the flag-only `<ea> − #imm` (`AluOp::Cmp` +
    // `Dest::None`, X preserved). The immediate's ext words always precede the EA's ext words in the prefetch
    // stream — pinned to the data via the immediate-then-EA interleave in `cmpi_recipe`. The Cmpi class of the
    // 3-way CMP.* mix (classified by OPCODE). The opcode space 0x0Cxx is disjoint from every arm above (which
    // never matches the 0x0xxx high nibble except the `*toSR` single points 0x007C/0x027C/0x0A7C, all decoded
    // below — and a CMPI never has bits 7-6 == 11, so it cannot alias them).
    if matches!(cmp_class(opcode), CmpClass::Cmpi) {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // SS = 2
        };
        return cmpi_recipe(opcode, size);
    }
    // BTST `<ea>` — test a single bit, setting ONLY Z = NOT(bit); X/N/V/C + the SR system byte preserved.
    // READ-ONLY (no write). The bit width follows the operand: a `Dn` operand is 32-bit (mod 32, `Size::Long`),
    // a memory / `#imm` / PC-relative operand is 8-bit (mod 8, `Size::Byte`). Two forms, classified by OPCODE:
    //   - **DYNAMIC** `0000 ddd 1 00 mmm rrr` (mask `0xF1C0 == 0x0100`, `tt` bits 7-6 == 00) — the bit number is
    //     `D[(opcode>>9)&7]`; the FULL source set: `Dn` (0), `(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)` (2-6),
    //     `abs.w`/`abs.l`/`d16(PC)`/`d8(PC,Xn)`/`#imm` (7/0..7/4). Mode 001 = MOVEP (absent / not decoded), so
    //     the guard excludes `An`-direct.
    //   - **STATIC** `0000 1000 00 mmm rrr` (mask `0xFF00 == 0x0800`, `tt` == 00) — the bit number is the
    //     `prefetch[1]` ext word; the same source set MINUS `#imm` (7/4 absent — `#imm` is not a static operand).
    // The opcode spaces 0x01xx (dynamic, bit 8 set) and 0x08xx (static) are disjoint from CMPI (0x0Cxx, bit 8
    // clear) and the `*toSR` single points (0x007C/0x027C/0x0A7C, bit 8 clear) decoded below.
    let bit_mode = (opcode >> 3) & 7;
    let bit_reg = opcode & 7;
    // MOVEP.w / MOVEP.l (`0000 rrr 1 oo 001 aaa`, `opcode & 0xF138 == 0x0108`) — move a data register ↔ the
    // ALTERNATING (even/odd) memory addresses `EA, EA+2, …` (the historical 8-bit-peripheral mode). One
    // addressing mode only: `d16(An)` (EA = `An + sx16(prefetch[1])`). Opmode bit7 = direction (0 = mem→reg
    // READ / 1 = reg→mem WRITE), bit6 = size (0 = word / 1 = long). NO flags. This fills EXACTLY the "mode 001 =
    // MOVEP (absent)" slot the dynamic BTST/BCHG/BCLR/BSET carve out below — the mask fixes bits5-3 = 001, so it
    // is DISJOINT from every bit-op (which require a non-001 EA mode). Order is not load-bearing (disjoint by the
    // mode field), but placed adjacent to the bit-ops it complements.
    if opcode & 0xF138 == 0x0108 {
        return movep_recipe(opcode, regs);
    }
    if opcode & 0xF1C0 == 0x0100 && btst_dyn_src_in_scope(bit_mode, bit_reg) {
        return btst_recipe(opcode);
    }
    if opcode & 0xFF00 == 0x0800
        && (opcode >> 6) & 3 == 0
        && btst_static_src_in_scope(bit_mode, bit_reg)
    {
        return btst_recipe(opcode);
    }
    // BCHG `<ea>` — test then TOGGLE a single bit (`operand ^= 1<<pos`), setting ONLY Z = NOT(the PRE-modify
    // bit); X/N/V/C + the SR system byte preserved. A read-modify-WRITE to a data-alterable destination only:
    // `Dn` (0) = 32-bit (mod 32, `Size::Long`, FULL-32 write) / memory (2-6, 7/0, 7/1) = 8-bit (mod 8,
    // `Size::Byte`, byte RMW). Two forms, classified by OPCODE (`tt` bits 7-6 == 01):
    //   - **DYNAMIC** `0000 ddd 1 01 mmm rrr` (mask `0xF1C0 == 0x0140`) — the bit number is `D[(opcode>>9)&7]`.
    //   - **STATIC** `0000 1000 01 mmm rrr` (mask `0xFF00 == 0x0800`, `tt` == 01) — the bit number is `prefetch[1]`.
    // The data-alterable guard (`mode == 0 || is_dst_mem_mode`) excludes `An`-direct (mode 1 = MOVEP, absent) /
    // PC-relative / `#imm` (none of which are alterable). The shared `bit_recipe` carries the DECODE-TIME
    // `pos >= 16` register `+2` (the bit number is read here, from the live `Dn` / the captured ext word) — a
    // REGISTER-ONLY variance; memory timing is fixed per mode. Register base idle = `n2` for BCHG (BSET shares
    // it; BCLR uses `n4`).
    if opcode & 0xF1C0 == 0x0140 && (bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg)) {
        return bit_recipe(opcode, AluOp::Bchg, 2, regs);
    }
    if opcode & 0xFF00 == 0x0800
        && (opcode >> 6) & 3 == 1
        && (bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg))
    {
        return bit_recipe(opcode, AluOp::Bchg, 2, regs);
    }
    // BCLR `<ea>` — test then CLEAR a single bit (`operand &= !(1<<pos)`), setting ONLY Z = NOT(the PRE-clear
    // bit); X/N/V/C + the SR system byte preserved. A read-modify-WRITE to a data-alterable destination only:
    // `Dn` (0) = 32-bit (mod 32, `Size::Long`, FULL-32 write) / memory (2-6, 7/0, 7/1) = 8-bit (mod 8,
    // `Size::Byte`, byte RMW). Two forms, classified by OPCODE (`tt` bits 7-6 == 10):
    //   - **DYNAMIC** `0000 ddd 1 10 mmm rrr` (mask `0xF1C0 == 0x0180`) — the bit number is `D[(opcode>>9)&7]`.
    //   - **STATIC** `0000 1000 10 mmm rrr` (mask `0xFF00 == 0x0800`, `tt` == 10) — the bit number is `prefetch[1]`.
    // Reuses the shared `bit_recipe` VERBATIM, identical to BCHG EXCEPT the register base idle is `n4` (BCLR is
    // 8/10 cyc, 2 slower than BCHG/BSET's 6/8) — `reg_base = 4`. The DECODE-TIME `pos >= 16` register `+2` (read
    // here from the live `Dn` / the captured ext word) still applies; memory timing is identical to BCHG (fixed
    // byte RMW per mode). The data-alterable guard excludes `An`-direct (mode 1 = MOVEP, absent) / PC-rel / #imm.
    if opcode & 0xF1C0 == 0x0180 && (bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg)) {
        return bit_recipe(opcode, AluOp::Bclr, 4, regs);
    }
    if opcode & 0xFF00 == 0x0800
        && (opcode >> 6) & 3 == 2
        && (bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg))
    {
        return bit_recipe(opcode, AluOp::Bclr, 4, regs);
    }
    // BSET `<ea>` — test then SET a single bit (`operand |= 1<<pos`), setting ONLY Z = NOT(the PRE-set bit);
    // X/N/V/C + the SR system byte preserved. A read-modify-WRITE to a data-alterable destination only: `Dn` (0)
    // = 32-bit (mod 32, `Size::Long`, FULL-32 write) / memory (2-6, 7/0, 7/1) = 8-bit (mod 8, `Size::Byte`,
    // byte RMW). Two forms, classified by OPCODE (`tt` bits 7-6 == 11):
    //   - **DYNAMIC** `0000 ddd 1 11 mmm rrr` (mask `0xF1C0 == 0x01C0`) — the bit number is `D[(opcode>>9)&7]`.
    //   - **STATIC** `0000 1000 11 mmm rrr` (mask `0xFF00 == 0x0800`, `tt` == 11) — the bit number is `prefetch[1]`.
    // Reuses the shared `bit_recipe` VERBATIM, identical to BCHG — the register base idle is `n2` (BSET is 6/8
    // cyc, the SAME as BCHG, NOT BCLR's `n4`/8-10) — `reg_base = 2`. The DECODE-TIME `pos >= 16` register `+2`
    // (read here from the live `Dn` / the captured ext word) still applies; memory timing is identical to BCHG
    // (fixed byte RMW per mode). The data-alterable guard excludes `An`-direct (mode 1 = MOVEP, absent) / PC-rel
    // / #imm. This is the FINAL bit-op (BTST/BCHG/BCLR/BSET — `tt` 00/01/10/11 — all decoded).
    if opcode & 0xF1C0 == 0x01C0 && (bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg)) {
        return bit_recipe(opcode, AluOp::Bset, 2, regs);
    }
    if opcode & 0xFF00 == 0x0800
        && (opcode >> 6) & 3 == 3
        && (bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg))
    {
        return bit_recipe(opcode, AluOp::Bset, 2, regs);
    }
    // ADDI/SUBI/ANDI/ORI/EORI `#imm,<ea>` (`0000 hhhh ss mmm rrr`, group-0 high byte in {0x06 ADDI, 0x04 SUBI,
    // 0x02 ANDI, 0x00 ORI, 0x0A EORI}, `ss` = 00/01/10) — the immediate-to-EA read-modify-write built by
    // [`imm_rmw_recipe`] (the CMPI immediate-capture idiom + the `ea_dst` RMW writeback). Classified by high
    // byte via [`imm_class`]; ADDI (0x06), SUBI (0x04), ANDI (0x02), ORI (0x00) and EORI (0x0A) are all admitted
    // (the classifier now returns `Some` for every `*I` high byte). The destination is data-alterable only (`Dn` (0) or the seven
    // alterable-memory modes). DISJOINT from CMPI (0x0C, decoded above), the bit-static form (0x08), the
    // dynamic BTST/BCHG/BCLR/BSET (0x01xx, bit 8 set — all decoded above), MOVEP (bit 8 set), and the
    // `*toSR`/`*toCCR` single points (mode 7/4 `#imm`, excluded by the data-alterable dest guard). Odd word/
    // long EAs fault on the RMW read via the E3/E4 address-error abort (in scope).
    if bit_mode == 0 || is_dst_mem_mode(bit_mode, bit_reg) {
        if let Some((op, size)) = imm_class(opcode) {
            return imm_rmw_recipe(opcode, op, size);
        }
    }
    // CMPA `<ea>,An` (`1011 aaa 0 11/111 mmm rrr`, nibble 0xB, opmode 3 = `.w` / opmode 7 = `.l`) — the
    // flag-only address compare `An − <ea>` (An the minuend, full 32 bits). All 12 source modes via the MOVEA
    // source machinery ([`ea_movea`]'s `ea_src`/`ea_movea_long`); the `.w` source word is sign-extended to 32
    // before the long-boundary subtraction (`AluOp::Cmpa` does this internally, mirroring `AluOp::MoveA`). Sets
    // N/Z/V/C, PRESERVES X, writes nothing (`Dest::None`). The CMPA cycle count is the MOVEA bus cost plus a
    // uniform trailing `Internal(2)` idle (CMPA = MOVEA + 2 cyc for every source mode — pinned to the data).
    // The Cmpa class of the 0xB nibble (its own CMPA.w/.l files), classified by OPCODE.
    if matches!(cmp_class(opcode), CmpClass::Cmpa) {
        let size = if (opcode >> 6) & 7 == 3 {
            Size::Word
        } else {
            Size::Long // opmode 7
        };
        return cmpa_recipe(opcode, size);
    }
    // EOR `Dn,<ea>` (`1011 ddd 1SS mmm rrr`, nibble 0xB, opmode 4/5/6 = b/w/l = 0xB100/0xB140/0xB180) — bitwise
    // `<ea> = <ea> ^ Dn`. EOR exists ONLY in this `Dn,<ea>` direction (opmode 0/1/2 of nibble 0xB is CMP, not
    // `EOR <ea>,Dn`). The destination is either a **data register** (mode 000 = `Dn,Dn`) or **alterable memory**
    // (modes 2..6, abs.w/abs.l via `is_dst_mem_mode`). **Mode field 001 = CMPM** (`(Ay)+,(Ax)+`) — a DIFFERENT
    // instruction handled by the `cmp_class` Cmpm arm which runs FIRST in dispatch (above), so by the time we
    // reach this arm the mode is never 001; the `mode == 0 || is_dst_mem_mode` guard also excludes it (mode 1 is
    // neither register-direct nor alterable memory). Sets N = msb / Z = (result == 0), clears V/C, PRESERVES X
    // (`AluOp::Eor` = `move_flags` + X re-injected). The **EOR.* files MIX** this genuine register form (nibble
    // 0xB) with the dedicated EORI immediate opcode (`0x0Axx`, high nibble 0) — a DIFFERENT instruction NOT
    // decoded this push; `covered()` classifies the EORI cases OUT by OPCODE (high nibble 0 != 0xB), so decode
    // is only ever reached on the genuine 0xB form. `eor_recipe` routes the mode-000 register dest through its
    // own no-memory arm (like `clr_recipe`'s mode-0 path) and the alterable-memory dest through `arith_dn_ea`
    // VERBATIM (EOR Dn,<ea> = ADD Dn,<ea> byte-for-byte). opmode 3/7 (0xB0C0/0xB1C0) is CMPA — handled above.
    let eor_mode = (opcode >> 3) & 7;
    let eor_reg = opcode & 7;
    if opcode & 0xF1C0 == 0xB100 && (eor_mode == 0 || is_dst_mem_mode(eor_mode, eor_reg)) {
        return eor_recipe(opcode, Size::Byte); // EOR.b Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xB140 && (eor_mode == 0 || is_dst_mem_mode(eor_mode, eor_reg)) {
        return eor_recipe(opcode, Size::Word); // EOR.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xB180 && (eor_mode == 0 || is_dst_mem_mode(eor_mode, eor_reg)) {
        return eor_recipe(opcode, Size::Long); // EOR.l Dn,<ea>
    }
    // TST `<ea>` (`0100 1010 SS mmm rrr`, 0x4A00/4A40/4A80, SS bits 7-6 = b/w/l) — the flag-only test
    // `<ea> − 0`: read the data-alterable EA, set N = msb(operand) / Z = (operand == 0), clear V/C, PRESERVE
    // X, write NOTHING (`AluOp::Cmp` with `b = Operand::Zero` + `Dest::None`). Same `ea_src` source machinery
    // as CMP, but UNLIKE CMP/ADD there is NO trailing idle for any size (TST.l Dn = 4 not 6; TST.l (An) = 12
    // not 14 — pinned to the vendored TST.* stream), so the long source uses a dedicated idle-free
    // `tst_ea_src_long`. SS == 3 (0x4AC0) is TAS, not TST, and is excluded by the `& 0xC0 != 0xC0` guard. The
    // opcode space 0x4A00..=0x4ABF is disjoint from JMP/JSR/RTS (all 0x4Exx) and the 0x4180 CHK arm below.
    if opcode & 0xFF00 == 0x4A00 && opcode & 0xC0 != 0xC0 {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // SS = 2
        };
        return tst_recipe(opcode, size);
    }
    // TAS `<ea>` (`0100 1010 11 mmm rrr`, opcode & 0xFFC0 == 0x4AC0) — the indivisible test-and-set, BOTH the
    // REGISTER and the data-alterable MEMORY forms. This is the `SS == 3` (`& 0xC0 == 0xC0`) sub-case of the
    // 0x4A00 TST space — the TST arm ABOVE excludes it via `& 0xC0 != 0xC0`, so there is no conflict. Read
    // the byte → set N = bit7 / Z = (byte == 0), clear V/C, PRESERVE X → write `byte | 0x80` (the flags are
    // on the READ byte, the written value is `read | 0x80` — DISTINCT). `Dn` (mode 0) is the 4-cyc Alu form;
    // memory is the ATOMIC indivisible RMW (ONE `'t'` bus cycle, via `ea_tas` — NOT the read-then-write
    // `ea_dst` path). The data-alterable guard (`mode == 0 || is_dst_mem_mode`) excludes `An` / PC-rel /
    // `#imm` (none are alterable). Byte-only → NO odd-EA faults.
    let tas_mode = (opcode >> 3) & 7;
    if opcode & 0xFFC0 == 0x4AC0 && (tas_mode == 0 || is_dst_mem_mode(tas_mode, opcode & 7)) {
        return tas_recipe(opcode);
    }
    // CLR `<ea>` (`0100 0010 SS mmm rrr`, 0x4200/4240/4280, SS bits 7-6 = b/w/l) — clear the data-alterable EA
    // to 0, setting Z=1/N=0/V=0/C=0 and PRESERVING X (= `move_flags(0)`). CLR is a READ-then-WRITE: it READS
    // the EA (the value is DISCARDED), refills, then WRITES 0 — so it reuses the existing `ea_dst`/`ea_dst_long`
    // RMW path with `make_alu = Alu{Move, size, a: Zero, dst: Scratch(1)}` (the Move sets the flags + parks the
    // 0, which the write then stores). The odd-EA case faults on the READ (low5 = 0x15), covered by the E3/E4
    // abort. Dn-direct (mode 0) has NO memory access: `Alu{Move, size, a: Zero, dst: dn_dest(Dn,size)}` +
    // Prefetch (CLR.l Dn = 6 cyc with one trailing idle; CLR.b/.w Dn = 4). SS == 3 (0x42C0) is illegal on the
    // 68000 (not CLR — never decoded). The opcode space 0x4200..=0x42BF is disjoint from TST (0x4A00) and the
    // 0x4Exx / 0x4180 arms.
    if opcode & 0xFF00 == 0x4200 && opcode & 0xC0 != 0xC0 {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // SS = 2
        };
        return clr_recipe(opcode, size);
    }
    // NEG `<ea>` (`0100 0100 SS mmm rrr`, 0x4400/4440/4480, SS bits 7-6 = b/w/l) — negate the data-alterable EA:
    // `res = (0 − d) & mask` with FULL SUBTRACT flags (NEG is literally `0 − d`): N = msb(res), Z = (res == 0),
    // V = (d == sign-min) (the 0-minus-itself overflow), C = X = (d != 0) borrow (`AluOp::Neg` ≡ `Sub(0, d)`).
    // NEG is a READ-then-WRITE for a memory destination — the SAME RMW path as CLR (`ea_dst`/`ea_dst_long`), but
    // the read operand is the UNARY SOURCE (CLR discards it and writes 0; NEG negates it). The odd-EA case faults
    // on the READ (low5 = 0x15), covered by the E3/E4 abort. `Dn`-direct (mode 0) has NO memory access:
    // `Alu{Neg, size, a: <Dn by size>, b: Zero, dst: dn_dest(Dn,size)}` + Prefetch (NEG.l Dn = 6 cyc with one
    // trailing idle; NEG.b/.w Dn = 4). The destination must be data-alterable (mode 0 OR `is_dst_mem_mode`).
    // SS == 3 (0x44C0) is MOVE-to-CCR, NOT NEG, excluded by `& 0xC0 != 0xC0`. The opcode space 0x4400..=0x44BF is
    // disjoint from CLR (0x4200) / TST (0x4A00) and the 0x4Exx / 0x4180 arms.
    let neg_mode = (opcode >> 3) & 7;
    let neg_reg = opcode & 7;
    if opcode & 0xFF00 == 0x4400
        && opcode & 0xC0 != 0xC0
        && (neg_mode == 0 || is_dst_mem_mode(neg_mode, neg_reg))
    {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // SS = 2
        };
        return neg_family_recipe(opcode, AluOp::Neg, size);
    }
    // MOVEtoCCR `<ea>` (`0100 0100 11 mmm rrr`, opcode & 0xFFC0 == 0x44C0) — move a word from a data source EA
    // into the CCR: `CCR = (SR & 0xFF00) | (src.w & 0x1F)` (the SR SYSTEM byte is PRESERVED; CCR bits 5-7 force
    // to 0 — only X/N/Z/V/C load). NOT privileged; S never changes. SS == 3 (bits 7-6 = 0b11) distinguishes it
    // from NEG (0x4400, SS 0/1/2); the NEG arm above excludes `& 0xC0 == 0xC0`, so 0x44C0 reaches THIS arm
    // cleanly. The source is the full data + PC-relative + `#imm` set (modes 0, 2-6, 7/0-7/4) — NOT An-direct
    // (mode 1). Odd word EAs are READ address errors (the operand read faults via the E3/E4 abort). Reuses the
    // shared `move_to_sr_ccr_recipe` (the novel pipe-flush bus shape) with `to_sr = false` → `MicroOp::LoadCcr`.
    // The opcode space 0x44C0..=0x44FF is disjoint from NEG (0x4400..=0x44BF) and the 0x4Exx / 0x4180 arms.
    if opcode & 0xFFC0 == 0x44C0 {
        return move_to_sr_ccr_recipe(opcode, false);
    }
    // MOVEtoSR `<ea>` (`0100 0110 11 mmm rrr`, opcode & 0xFFC0 == 0x46C0) — move a word from a data source EA
    // into the SR: `SR = src.w & 0xA71F` (the FULL status register incl the system byte S/T/I). PRIVILEGED, but
    // UNEXERCISED (every vendored case is supervisor, T=0 at entry) — so NO privilege trap and NO trace trap are
    // gated (correctness-only; do not add untested trap code). SS == 3 (bits 7-6 = 0b11) distinguishes it from
    // NOT (0x4600, SS 0/1/2); the NOT arm below excludes `& 0xC0 == 0xC0`, so 0x46C0 reaches THIS arm cleanly.
    // The source is the full data + PC-relative + `#imm` set (modes 0, 2-6, 7/0-7/4) — NOT An-direct (mode 1).
    // Odd word EAs are READ address errors (the operand read faults via the E3/E4 abort). Reuses the SHARED
    // `move_to_sr_ccr_recipe` VERBATIM with `to_sr = true` → `MicroOp::LoadSr` (mask 0xA71F). The load-bearing
    // difference vs MOVEtoCCR: `LoadSr` runs BEFORE the flush-read + prefetch, so when the new SR CLEARS S the
    // two trailing reads run under the NEW mode's function code (FC2 user-program instead of FC6) — the
    // mid-instruction FC switch (`regs.fc` computes each read's FC at execution time). The opcode space
    // 0x46C0..=0x46FF is disjoint from NOT (0x4600..=0x46BF) and the 0x4Exx / 0x4180 arms.
    if opcode & 0xFFC0 == 0x46C0 {
        return move_to_sr_ccr_recipe(opcode, true);
    }
    // MOVEfromSR `<ea>` (`0100 0000 11 mmm rrr`, opcode & 0xFFC0 == 0x40C0) — move the FULL 16-bit SR (incl the
    // system byte) to a data-alterable EA. SS == 3 (bits 7-6 = 0b11) distinguishes it from NEGX (0x4000, SS
    // 0/1/2); the NEGX arm below excludes `& 0xC0 == 0xC0`, so 0x40C0 reaches THIS arm cleanly. NOT privileged
    // on the 68000, sets **NO flags** (SR byte-identical initial→final). Register (Dn) `Dn.w = SR` (high word
    // preserved), 6 cyc; memory is CLR.w's read-before-write RMW (dummy word READ discarded, then the word SR
    // write) so an odd EA is a READ address error (the E3/E4 abort) — but the WRITTEN value is SR with NO flags
    // (unlike CLR's Move-of-zero, which SETS Z=1). The destination must be data-alterable (mode 0 OR
    // `is_dst_mem_mode`) — the guard excludes An-direct (mode 1), PC-relative (7/2, 7/3), and `#imm` (7/4).
    let movefrom_sr_mode = (opcode >> 3) & 7;
    let movefrom_sr_reg = opcode & 7;
    if opcode & 0xFFC0 == 0x40C0
        && (movefrom_sr_mode == 0 || is_dst_mem_mode(movefrom_sr_mode, movefrom_sr_reg))
    {
        return movefrom_sr_recipe(opcode);
    }
    // NEGX `<ea>` (`0100 0000 SS mmm rrr`, 0x4000/4040/4080, SS bits 7-6 = b/w/l) — negate-with-extend the
    // data-alterable EA: `res = (0 − d − X_in) & mask` with SUBX-style flags: N = msb(res), **Z STICKY**
    // (`Z_final = Z_in AND (res == 0)` — NEGX only ever CLEARS Z), V = `(d & res & signbit) != 0`,
    // C = X = NOT(d == 0 AND X_in == 0) borrow (`AluOp::Negx`). NEGX is a READ-then-WRITE for a memory dest — it
    // REUSES `neg_family_recipe` VERBATIM (the recipe shape is identical to NEG/CLR's `ea_dst`/`ea_dst_long` RMW;
    // ONLY the dedicated `AluOp::Negx` exec differs — sticky Z + X-in). The odd-EA case faults on the READ (low5
    // = 0x15), covered by the E3/E4 abort. `Dn`-direct (mode 0) has NO memory access (NEGX.l Dn = 6 cyc with a
    // trailing idle; NEGX.b/.w Dn = 4). The destination must be data-alterable (mode 0 OR `is_dst_mem_mode`).
    // SS == 3 (0x40C0) is MOVE-from-SR, NOT NEGX, excluded by `& 0xC0 != 0xC0`. The opcode space 0x4000..=0x40BF
    // is disjoint from NEGX's NEG (0x4400) / CLR (0x4200) / TST (0x4A00) siblings and the 0x4Exx / 0x4180 arms.
    let negx_mode = (opcode >> 3) & 7;
    let negx_reg = opcode & 7;
    if opcode & 0xFF00 == 0x4000
        && opcode & 0xC0 != 0xC0
        && (negx_mode == 0 || is_dst_mem_mode(negx_mode, negx_reg))
    {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // SS = 2
        };
        return neg_family_recipe(opcode, AluOp::Negx, size);
    }
    // NOT `<ea>` (`0100 0110 SS mmm rrr`, 0x4600/4640/4680, SS bits 7-6 = b/w/l) — bitwise-complement the
    // data-alterable EA: `res = (~d) & mask` with LOGIC flags (the SAME MOVE flag shape as AND/OR/EOR): N =
    // msb(res), Z = (res == 0), **V = 0, C = 0, X PRESERVED** (re-injected `ccr_nz | (sr & CCR_X)`, never
    // computed) via the new `AluOp::Not`. NOT is a READ-then-WRITE for a memory dest — it REUSES
    // `neg_family_recipe` VERBATIM (the recipe shape is identical to NEG/NEGX/CLR's `ea_dst`/`ea_dst_long` RMW;
    // ONLY the `AluOp::Not` exec differs — `~a` instead of a subtraction). The odd-EA case faults on the READ
    // (low5 = 0x15), covered by the E3/E4 abort. `Dn`-direct (mode 0) has NO memory access (NOT.l Dn = 6 cyc
    // with a trailing idle; NOT.b/.w Dn = 4). The destination must be data-alterable (mode 0 OR
    // `is_dst_mem_mode`). SS == 3 (0x46C0) is MOVE-to-SR (privileged), NOT NOT, excluded by `& 0xC0 != 0xC0`.
    // The opcode space 0x4600..=0x46BF is disjoint from NOT's NEG (0x4400) / NEGX (0x4000) / CLR (0x4200) /
    // TST (0x4A00) siblings and the 0x4Exx / 0x4180 arms.
    let not_mode = (opcode >> 3) & 7;
    let not_reg = opcode & 7;
    if opcode & 0xFF00 == 0x4600
        && opcode & 0xC0 != 0xC0
        && (not_mode == 0 || is_dst_mem_mode(not_mode, not_reg))
    {
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // SS = 2
        };
        return neg_family_recipe(opcode, AluOp::Not, size);
    }
    // NBCD `<ea>` (`0100 1000 00 mmm rrr`, opcode & 0xFFC0 == 0x4800, data-alterable modes) — the packed-DECIMAL
    // negate `<ea> = 0 −₁₀ <ea> − X`, BYTE-ONLY, over the single data-alterable EA. It is the FINAL op of the
    // X-flag arithmetic cluster: `AluOp::Nbcd` is EXACTLY `sbcd_core(dst=0, src=operand, X_in)` (the SBCD core
    // with the operand as the src and 0 as the dst) — the SAME carry/result asymmetry, sticky Z, X_out = C_out.
    // The single-EA read-modify-write shape is IDENTICAL to NEG/NOT/NEGX (via `nbcd_recipe`, itself modelled on
    // `neg_family_recipe`): `<ea>` operand read → Alu → write back, with the NBCD idle profile (Dn = 6 via a
    // trailing Internal(2); `(An)` = 12, `(An)+` = 12, `-(An)` = 14, `d16(An)` = 16, `d8(An,Xn)` = 18, `abs.w` =
    // 16, `abs.l` = 20). The destination must be data-alterable (mode 0 OR `is_dst_mem_mode`) — the mode guard is
    // LOAD-BEARING: it excludes An-direct (mode 1), PC-relative (7/2, 7/3), and `#imm` (7/4). Byte-only → NO
    // alignment fault path (`-(A7).b` steps by 2). The opcode space 0x4800..=0x483F is disjoint from its 0x4x00
    // unary siblings (NEGX 0x4000 / CLR 0x4200 / NEG 0x4400 / NOT 0x4600 / TAS 0x4AC0) and from the 0x4840 SWAP/
    // PEA and 0x4880+ EXT/MOVEM neighbours above/below.
    let nbcd_mode = (opcode >> 3) & 7;
    let nbcd_reg = opcode & 7;
    if opcode & 0xFFC0 == 0x4800 && (nbcd_mode == 0 || is_dst_mem_mode(nbcd_mode, nbcd_reg)) {
        return nbcd_recipe(opcode);
    }
    // EXT.w / EXT.l (`0100 1000 1S 000 rrr`, 0x4880 .w / 0x48C0 .l, mask `opcode & 0xFFF8`) — sign-extend the
    // `Dn`-only source whose result width follows the size: EXT.w sign-extends the low BYTE of Dn to 16 bits
    // (`res = sign_extend8→16(Dn & 0xFF)`) and writes the LOW WORD (the high word of Dn is preserved), flags on
    // bit15 / word-zero; EXT.l sign-extends the low WORD to 32 bits (`res = sign_extend16→32(Dn & 0xFFFF)`) and
    // writes the FULL 32, flags on bit31 / long-zero. LOGIC flags (N = result-msb, Z = (result == 0), V = 0,
    // C = 0, X PRESERVED) via the new unary `AluOp::Ext`. The mode is FIXED 000 = `Dn` (the low 3 bits are the
    // register), so the mask is `0xFFF8` — NOT `0xFFC0`, which would swallow the PEA/MOVEM neighbours in 0x48xx
    // (mode ≥ 2). 4 cyc (one Prefetch, no idle, no memory — `Dn`-only, no fault possible).
    if opcode & 0xFFF8 == 0x4880 {
        return ext_recipe(opcode, Size::Word);
    }
    if opcode & 0xFFF8 == 0x48C0 {
        return ext_recipe(opcode, Size::Long);
    }
    // MOVEM.w / MOVEM.l (`0100 1D00 1S mmm rrr`, opcode & 0xFB80 == 0x4880) — move a register list ↔ memory,
    // NO flags. Size = bit 6 (0x0040): `.w` (0x4880/0x4C80) / `.l` (0x48C0/0x4CC0). Direction = bit 10
    // (0x0400): reg→mem (0x4880/0x48C0) / mem→reg (0x4C80/0x4CC0). The register mask is the extension word
    // `prefetch[1]` (AVAILABLE AT DECODE TIME, like the Bcc/DBcc live reads), so `movem_recipe` expands it into
    // an exact-length linear transfer recipe. The mode guard is LOAD-BEARING: EXT.w (0x4880 mode 000) / EXT.l
    // (0x48C0 mode 000) are decoded ABOVE (mask 0xFFF8), and MOVEM requires a valid MOVEM mode (reg→mem =
    // control-alterable + predecrement; mem→reg = control + postincrement + PC-relative), so EXT/SWAP (mode 0)
    // never collide. Placed before PEA (0x4840) — disjoint opcode blocks. For `.l` each register transfer is a
    // LONG (two word bus accesses; the running address steps by 4; NO sign-extension on load — it is already a
    // full 32 bits); `length = base + 8·n` (per = 8, vs 4 for `.w`).
    if opcode & 0xFB80 == 0x4880 {
        let dir = (opcode >> 10) & 1;
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        if movem_mode_ok(dir, mode, reg) {
            let size = if (opcode >> 6) & 1 == 0 {
                Size::Word
            } else {
                Size::Long
            };
            return movem_recipe(opcode, size, regs);
        }
    }
    // SWAP (`0100 1000 01 000 rrr`, 0x4840, mask `opcode & 0xFFF8`) — swap the two 16-bit halves of `Dn`:
    // `res = (Dn >> 16) | (Dn << 16)` on the FULL 32 bits (size ignored / always Long). LOGIC flags on the
    // 32-bit result (N = bit31, Z = (result == 0), V = 0, C = 0, X PRESERVED) via the new unary `AluOp::Swap`.
    // The mode is FIXED 000 = `Dn`, so the mask is `0xFFF8` (isolating the `Dn` encodings from PEA/MOVEM). 4 cyc
    // (one Prefetch, no idle, no memory — `Dn`-only).
    if opcode & 0xFFF8 == 0x4840 {
        return swap_recipe(opcode);
    }
    // PEA `<control ea>` (`0100 1000 01 mmm rrr`, opcode & 0xFFC0 == 0x4840, mode >= 2) — compute the
    // CONTROL-mode effective address (EXACTLY LEA's EA — the address itself, no operand read) and PUSH it as a
    // LONG to `-(SP)`. The `mode >= 2` guard is LOAD-BEARING: `0x4840` mode 000 (Dn) is SWAP (decoded above,
    // mask `0xFFF8`), so PEA never swallows SWAP and SWAP never swallows PEA; the control-mode guard also
    // excludes the illegal modes 001/011/100 and 111/4. Timing = LEA + 8 (the two word pushes). NO flags.
    if opcode & 0xFFC0 == 0x4840 && is_control_mode((opcode >> 3) & 7, opcode & 7) {
        return pea_recipe(opcode);
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
    // LINK An,#disp (`0100 1110 0101 0 rrr`, opcode & 0xFFF8 == 0x4E50) — allocate a stack frame: `SP -= 4;
    // [SP] = An (long push); An = SP; SP += sign_extend16(disp)`. No flags. Flat 16 cyc, bus `[r@pc+4, w@SP−4,
    // w@SP−2, r@pc+6]` (the JSR reload interleave). The A7 quirk (LINK A7, 0x4E57): the predecrement runs first,
    // so `An ≡ SP` pushes SP−4, not the old A7. The opcode space 0x4E50..=0x4E57 is a single 8-point block
    // disjoint from RTS (0x4E75) / UNLINK (0x4E58) / every 0x4Exx arm above.
    if opcode & 0xFFF8 == 0x4E50 {
        return link_recipe(opcode);
    }
    // UNLINK An (`0100 1110 0101 1 rrr`, opcode & 0xFFF8 == 0x4E58) — deallocate a stack frame: `SP = An;
    // An = [SP] (long pop); SP = An_old + 4`. No flags. Flat 12 cyc, bus `[r@An, r@An+2, r@pc+4]`. For UNLK A7
    // (0x4E5F) the SP=An_old+4 write is clobbered by the pop into A7, leaving A7 = the popped long. The opcode
    // space 0x4E58..=0x4E5F is a single 8-point block disjoint from RTS (0x4E75) / LINK (0x4E50) / every arm above.
    if opcode & 0xFFF8 == 0x4E58 {
        return unlink_recipe(opcode);
    }
    // ADDQ/SUBQ `#data,<ea>` (`0101 qqq d ss mmm rrr`, 0x5xxx, `ss` = 00/01/10 so `(opcode>>6)&3 != 3`) — the
    // quick-immediate add (bit 8 = 0 → `AluOp::Add`/`Adda`) / subtract (bit 8 = 1 → `AluOp::Sub`/`Suba`). Only
    // ADDQ is admitted by `covered()` this commit (SUBQ decodes but is not yet in scope); the recipe is
    // parameterized by direction so both route through `addq_recipe`. Dest in scope: `Dn` (0), `An` word/long
    // (1, not byte), the seven alterable-memory modes (2-6, 7/0, 7/1). The `ss == 3` space (bits 7-6 == 11) is
    // Scc/DBcc, handled by their own arms below and disjoint from ADDQ/SUBQ by size — this arm never sees it.
    {
        let mode = (opcode >> 3) & 7;
        let reg = opcode & 7;
        let ss = (opcode >> 6) & 3;
        if opcode & 0xF000 == 0x5000
            && ss != 3
            && (mode == 0
                || (mode == 1 && ss != 0)
                || (2..=6).contains(&mode)
                || (mode == 7 && reg <= 1))
        {
            let size = match ss {
                0 => Size::Byte,
                1 => Size::Word,
                _ => Size::Long,
            };
            let (op, addr_op) = if (opcode >> 8) & 1 == 0 {
                (AluOp::Add, AluOp::Adda)
            } else {
                (AluOp::Sub, AluOp::Suba)
            };
            return addq_recipe(opcode, op, addr_op, size);
        }
    }
    // DBcc (`0101 cccc 11001 rrr`, opcode & 0xF0F8 == 0x50C8) — decrement-and-branch loop. This is the
    // `An`-direct (mode 001) special case of the `Scc` opcode space (`0101 cccc 11 mmm rrr`): only this exact
    // form is DBcc — every other mode is `Scc` (a conditional byte-set, NOT decoded here). The condition AND
    // the loop counter are both resolved at DECODE time (against the live CCR / `Dn` low word), emitting the
    // fall-through or taken linear recipe directly. The opcode space 0x50C8.. (mode 001, size 11) is disjoint
    // from ADDQ/SUBQ (sizes 00/01/10) and from every arm above.
    if opcode & 0xF0F8 == 0x50C8 {
        return dbcc_recipe(opcode, regs);
    }
    // Scc `<ea>` (`0101 cccc 11 mmm rrr`, opcode & 0xF0C0 == 0x50C0) — the conditional byte set: write `0xFF`
    // if the condition `cc` (bits 11-8) is TRUE else `0x00`, with NO flags. The condition is resolved at
    // DECODE time (like Bcc/DBcc/TRAPV) against the LIVE CCR via `condition_true`. **Placed AFTER the DBcc
    // arm** (`0x50C8`, mode 001) so DBcc is consumed first; the data-alterable guard (`mode == 0 ||
    // is_dst_mem_mode`) then excludes mode 1 (An / DBcc). cc 0 = `T` (always 0xFF) and cc 1 = `F` (always
    // 0x00) are BOTH legal for Scc (unlike Bcc/BSR). Byte-only. The opcode space (bits 7-6 = 11) is disjoint
    // from ADDQ/SUBQ (sizes 00/01/10) and from every arm above.
    let scc_mode = (opcode >> 3) & 7;
    if opcode & 0xF0C0 == 0x50C0 && (scc_mode == 0 || is_dst_mem_mode(scc_mode, opcode & 7)) {
        return scc_recipe(opcode, regs);
    }
    // RTR (`0x4E77`) — return and restore condition codes: POP the saved CCR word (@ SP) and the 32-bit
    // return address (hi @ SP+2, lo @ SP+4) off the stack, load the low 5 CCR bits into the SR, then reload
    // the prefetch queue at the popped target. Like RTS but with a leading CCR pop; the data's read order is
    // `pc_hi @ SP+2`, `ccr @ SP`, `pc_lo @ SP+4` (reproduced exactly). `AdjustAddr(SP, +6)` then the universal
    // `SetPc` + two-`Prefetch` reload. The opcode `0x4E77` is a single point in the 0x4Exx space.
    if opcode == 0x4E77 {
        return rtr_recipe();
    }
    // RTE (`0x4E73`) — return from exception: pop the 6-byte frame (SR + 32-bit PC) off the supervisor stack,
    // restore the FULL SR (masked 0xA71F, which may switch S supervisor→user and T), increment SP by 6 while
    // still supervisor, then reload the queue at the popped PC (the reload FC follows the RESTORED mode). The
    // inverse of the standard frame push. The opcode `0x4E73` is a single point in the 0x4Exx space, disjoint
    // from RTS (0x4E75) / RTR (0x4E77) / JSR (0x4E80) / JMP (0x4EC0) and the TRAP block below.
    if opcode == 0x4E73 {
        return rte_recipe();
    }
    // TRAPV (`0x4E76`) — trap on overflow: a conditional trap resolved at DECODE time on the V flag (a direct
    // `sr & CCR_V` test, like Bcc). V=0 → a single prefetch (no trap, len 4); V=1 → the standard 6-byte
    // exception frame to vector 7, distinguished from TRAP by a LEADING prefetch (its first bus event is an
    // FC=6 queue refill @ pc+4, not the PCL write). The opcode `0x4E76` is a single point in the 0x4Exx space,
    // disjoint from RTS (0x4E75) / RTR (0x4E77) / RTE (0x4E73) and the TRAP block below.
    if opcode == 0x4E76 {
        return trapv_recipe(regs);
    }
    // TRAP #n (`0100 1110 0100 nnnn`, 0x4E40 | n) — the cleanest standard 6-byte exception frame: an
    // UNCONDITIONAL trap to vector `32 + n` (address `(32+n)*4`). NO leading prefetch (the queue is NOT
    // refilled before the push — TRAP's first bus event is the `PCL` write); saved PC = `pc + 2`. The opcode
    // space 0x4E40..=0x4E4F is a 16-point block disjoint from RTS/RTR (0x4E75/0x4E77), JSR (0x4E80) and JMP
    // (0x4EC0).
    if opcode & 0xFFF0 == 0x4E40 {
        return trap_recipe(opcode);
    }
    // CHK `<ea>,Dn` (`0100 ddd 110 mmm rrr`, opcode & 0xF1C0 == 0x4180) — bounds-check trap. Read the word
    // bound from the source EA (all 11 legal source modes — An-direct is illegal for CHK and never appears),
    // then `ChkTrap` signed-compares Dn.w against 0 and the bound, sets the CCR, and on out-of-bounds installs
    // the standard 6-byte frame to vector 6 (the execution-time Shape-B abort). The opcode space (bits 8-6 =
    // 110, high nibble 4, ddd any) is disjoint from JMP/JSR/RTS/RTR/RTE/TRAP/TRAPV (all 0x4Exx) and every arm
    // above. An odd source EA word-faults into the E3 address-error frame (already coverable).
    if opcode & 0xF1C0 == 0x4180 {
        return chk_recipe(opcode);
    }
    // LEA `<control ea>,An` (`0100 aaa 111 mmm rrr`, opcode & 0xF1C0 == 0x41C0) — load the COMPUTED effective
    // address (the address itself, full 32-bit UNMASKED — pinned to the data) into `An = (op>>9)&7`, with NO
    // operand read and NO flags. Control addressing modes ONLY: `(An)` 010, `(d16,An)` 101, `(d8,An,Xn)` 110,
    // `abs.w` 111/0, `abs.l` 111/1, `(d16,PC)` 111/2, `(d8,PC,Xn)` 111/3. Modes 000/001/011/100 and 111/4 are
    // illegal/absent (never covered). The opcode space (bits 8-6 = 111, high nibble 4) is disjoint from CHK
    // (bits 8-6 = 110, the 0x4180 arm above) and from JMP/JSR/RTS/RTR/RTE/TRAP/TRAPV (all 0x4Exx). An odd
    // computed EA is fine — LEA never accesses the EA (no fault possible; the address is the result).
    if opcode & 0xF1C0 == 0x41C0 && is_control_mode((opcode >> 3) & 7, opcode & 7) {
        return lea_recipe(opcode);
    }
    // ANDItoSR / ORItoSR / EORItoSR (`0x027C` / `0x007C` / `0x0A7C`) — the privileged immediate-to-SR logic
    // ops (the whole 16-bit SR, masked to 0xA71F). Supervisor-only (every vendored case is supervisor; the
    // user-mode privilege-violation entry is correctness-only, not gated). A mid-instruction SR change that
    // clears S makes the instruction's own re-prefetch run under the NEW (user) function code — the FC
    // sequence is the load-bearing pin. The three opcodes are single points in the 0x0xxx immediate space,
    // disjoint from every arm above (which never matches the 0x0xxx high nibble).
    if opcode == 0x027C {
        return to_sr_recipe(LogicOp::And);
    }
    if opcode == 0x007C {
        return to_sr_recipe(LogicOp::Or);
    }
    if opcode == 0x0A7C {
        return to_sr_recipe(LogicOp::Eor);
    }
    // ANDItoCCR / ORItoCCR / EORItoCCR (`0x023C` / `0x003C` / `0x0A3C`) — the byte/CCR-form twins of the *toSR
    // trio: byte-identical bus stream + timing (20 cyc), but the write mask preserves the SR system byte (only
    // the low-5 CCR bits change; S/T/I never change → no FC switch). NOT privileged (legal in user mode), all
    // vendored cases supervisor. The three opcodes are single points in the 0x0xxx immediate space (low byte
    // 0x3C = mode 7 / reg 4 = `#imm`-to-CCR), disjoint from the 0x?7C *toSR twins and every arm above.
    if opcode == 0x023C {
        return to_ccr_recipe(LogicOp::And);
    }
    if opcode == 0x003C {
        return to_ccr_recipe(LogicOp::Or);
    }
    if opcode == 0x0A3C {
        return to_ccr_recipe(LogicOp::Eor);
    }
    // NOP (`0x4E71`, exact) — no operation: advance `pc` by 2 and shift the prefetch queue, nothing else (all
    // D/A/SP/SR byte-identical). A lone `[Prefetch]` (length 4). The opcode `0x4E71` is a single point in the
    // 0x4Exx space, disjoint from RESET (0x4E70) / JMP/JSR/RTS/RTR/RTE/TRAP/TRAPV and every arm above.
    if opcode == 0x4E71 {
        return nop_recipe();
    }
    // RESET (`0x4E70`) — assert the external reset line for 124 cycles. Privileged (supervisor-only; the
    // user-mode privilege-violation entry is correctness-only, not gated). No state change beyond the queue
    // refill: `[Internal(4), Internal(124), Prefetch]` (len 132). The opcode `0x4E70` is a single point in the
    // 0x4Exx space, disjoint from JMP/JSR/RTS/RTR/RTE/TRAP/TRAPV (all other 0x4Exx) and every arm above.
    if opcode == 0x4E70 {
        return reset_recipe();
    }
    // STOP #imm (`0x4E72`) — load the immediate word (prefetch[1]) into SR (masked to SR_IMPLEMENTED),
    // then halt fetching until an unmasked interrupt or reset (the wake is the CPU driver's job, Push A/A4).
    // Privileged: a user-mode STOP is caught by the decode-time privilege gate above (vector 8) and never
    // reaches here. Yacht L908: 4(0/0), no bus access. The opcode 0x4E72 is a single point in 0x4Exx,
    // disjoint from RESET (0x4E70) / NOP (0x4E71) / RTE (0x4E73) and every arm above.
    if opcode == 0x4E72 {
        return stop_recipe();
    }
    // MOVEfromUSP (`0100 1110 0110 1 rrr`, 0x4E68 | An) — `An = USP` (full 32 bits), NO flags, SR unchanged.
    // MOVEtoUSP   (`0100 1110 0110 0 rrr`, 0x4E60 | An) — `USP = An` (full 32 bits), NO flags, SR unchanged.
    // Both privileged on the 68000 but every vendored case is supervisor, so the privilege-violation trap is
    // UNEXERCISED and NOT gated (correctness-only, like the T-bit). They share base 0x4E40 (with TRAP) and each
    // other's base 0x4E60 — distinguished by bit 3 (0x4E68 from / 0x4E60 to). The two 8-point blocks
    // 0x4E68..=0x4E6F / 0x4E60..=0x4E67 are disjoint from TRAP (0x4E40..=0x4E4F), RTS/RTE/RTR/TRAPV/NOP/RESET and
    // every 0x4Exx arm above. Reg-direct only (An 0-7) → NO EA, NO exceptions.
    if opcode & 0xFFF8 == 0x4E68 {
        return usp_recipe(false, (opcode & 7) as u8);
    }
    if opcode & 0xFFF8 == 0x4E60 {
        return usp_recipe(true, (opcode & 7) as u8);
    }
    // MOVEQ (`0111 ddd 0 dddddddd`, 0x7000 | dn<<9 | imm8) — load a sign-extended 8-bit immediate into the FULL
    // 32 bits of Dn, setting N = msb / Z = (value == 0), clearing V/C, PRESERVING X (the `MOVE` flag op at the
    // long boundary). Bit 8 MUST be 0 — `0111 ddd 1 ...` is illegal on the 68000 and never appears in the data.
    // The immediate is the opcode's own low byte (`Operand::BranchDisp8`); there is no operand fetch (the value
    // is already in the prefetched opcode), so the recipe is a single flag-ALU + the trailing queue refill (4
    // cyc, one FC-6 read). The opcode space 0x7xxx (bit 8 = 0) is disjoint from every arm above (which never
    // matches the 0x7xxx high nibble).
    if opcode & 0xF100 == 0x7000 {
        return moveq_recipe(opcode);
    }
    // ASL `<ea>` (`0xExxx`, AS/left — the foundational shift) — `1110 ccc d ss ir tt rrr` register / `1110
    // 0TTd 11 mmm rrr` memory. The whole `0xExxx` nibble is a dedicated, otherwise-unused opcode space (no
    // other arm matches it). This commit decodes ONLY ASL (type AS, direction LEFT); ASR/LSL/LSR/ROL/ROR/
    // ROXL/ROXR land in S1-S7. The op identity is `(type, dir)`: REGISTER (bits 7-6 != 11) → bit 8 == 1
    // (left) AND bits 4-3 == 00 (AS); MEMORY (bits 7-6 == 11) → bit 8 == 1 AND bits 10-9 == 00 (AS). Only
    // ASL files are loaded this commit, so a non-ASL `0xE` opcode never reaches decode; the guard keeps the
    // arm precise regardless. The shared `shift_recipe` builds the register `[Prefetch, Alu, Internal{idle}]`
    // (the idle's `2*cnt` is the DECODE-TIME count — imm `ccc!=0?ccc:8` / live `D[ccc]&63`) or the word
    // memory shift-by-1 RMW (CLR.w/NEG.w's `ea_dst` path; an odd EA address-errors on the READ via E3/E4).
    if opcode >> 12 == 0xE {
        let is_asl = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 0 // memory: dir LEFT, type AS (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 0 // register: dir LEFT, type AS (bits 4-3)
        };
        if is_asl {
            return shift_recipe(opcode, AluOp::Asl, regs);
        }
        // ASR (S1) — AS/RIGHT: direction bit 8 == 0, type AS (register bits 4-3 == 0 / memory bits 10-9 == 0).
        // The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only the AluOp +
        // the `(type, dir)` classification differ from ASL. Only ASR files are loaded this commit (alongside
        // ASL's), so no other `0xE` op reaches decode; the guard keeps the arm precise regardless.
        let is_asr = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 0 // memory: dir RIGHT, type AS (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 0 // register: dir RIGHT, type AS (bits 4-3)
        };
        if is_asr {
            return shift_recipe(opcode, AluOp::Asr, regs);
        }
        // LSL (S2) — LS/LEFT: direction bit 8 == 1, type LS (register bits 4-3 == 1 / memory bits 10-9 == 1).
        // The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only the AluOp +
        // the `(type, dir)` classification differ from ASL/ASR. LSL == ASL's value and carry with V forced to 0
        // (logical, not arithmetic). Only ASL/ASR/LSL files are loaded this commit, so no other `0xE` op
        // reaches decode; the guard keeps the arm precise regardless.
        let is_lsl = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 1 // memory: dir LEFT, type LS (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 1 // register: dir LEFT, type LS (bits 4-3)
        };
        if is_lsl {
            return shift_recipe(opcode, AluOp::Lsl, regs);
        }
        // LSR (S3) — LS/RIGHT: direction bit 8 == 0, type LS (register bits 4-3 == 1 / memory bits 10-9 == 1).
        // The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only the AluOp +
        // the `(type, dir)` classification differ from ASL/ASR/LSL. LSR is the ZERO-FILL right shift (contrast
        // ASR's sign-extend). Only ASL/ASR/LSL/LSR files are loaded this commit, so no other `0xE` op reaches
        // decode; the guard keeps the arm precise regardless.
        let is_lsr = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 1 // memory: dir RIGHT, type LS (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 1 // register: dir RIGHT, type LS (bits 4-3)
        };
        if is_lsr {
            return shift_recipe(opcode, AluOp::Lsr, regs);
        }
        // ROL (S4) — RO/LEFT: direction bit 8 == 1, type RO (register bits 4-3 == 3 / memory bits 10-9 == 3).
        // The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only the AluOp +
        // the `(type, dir)` classification differ from ASL/ASR/LSL/LSR. ROL is a plain bit-rotate that does NOT
        // pass through X (contrast ROXL, which threads X — S6). Only ASL/ASR/LSL/LSR/ROL files are loaded this
        // commit, so no other `0xE` op reaches decode; the guard keeps the arm precise regardless.
        let is_rol = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 3 // memory: dir LEFT, type RO (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 3 // register: dir LEFT, type RO (bits 4-3)
        };
        if is_rol {
            return shift_recipe(opcode, AluOp::Rol, regs);
        }
        // ROR (S5) — RO/RIGHT: direction bit 8 == 0, type RO (register bits 4-3 == 3 / memory bits 10-9 == 3).
        // The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only the AluOp +
        // the `(type, dir)` classification differ from ASL/ASR/LSL/LSR/ROL. ROR is ROL's right-direction twin —
        // a plain bit-rotate that does NOT pass through X (contrast ROXR, which threads X — S7). Only
        // ASL/ASR/LSL/LSR/ROL/ROR files are loaded this commit, so no other `0xE` op reaches decode; the guard
        // keeps the arm precise regardless.
        let is_ror = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 3 // memory: dir RIGHT, type RO (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 3 // register: dir RIGHT, type RO (bits 4-3)
        };
        if is_ror {
            return shift_recipe(opcode, AluOp::Ror, regs);
        }
        // ROXL (S6) — ROX/LEFT: direction bit 8 == 1, type ROX (register bits 4-3 == 2 / memory bits 10-9 == 2).
        // The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only the AluOp +
        // the `(type, dir)` classification differ from ASL/ASR/LSL/LSR/ROL/ROR. ROXL is the FIRST X-threading
        // rotate — it threads X through an (n+1)-bit rotate (contrast ROL/ROR, which leave X untouched — and the
        // value shifts AS/LS, which set X = C from the value). Only ASL/ASR/LSL/LSR/ROL/ROR/ROXL files are loaded
        // this commit, so no other `0xE` op reaches decode; the guard keeps the arm precise regardless (ROXR is
        // type ROX with direction RIGHT — bit 8 == 0 — and is S7, not loaded).
        let is_roxl = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 1 && (opcode >> 9) & 3 == 2 // memory: dir LEFT, type ROX (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 1 && (opcode >> 3) & 3 == 2 // register: dir LEFT, type ROX (bits 4-3)
        };
        if is_roxl {
            return shift_recipe(opcode, AluOp::Roxl, regs);
        }
        // ROXR (S7, FINAL) — ROX/RIGHT: direction bit 8 == 0, type ROX (register bits 4-3 == 2 / memory bits
        // 10-9 == 2). The shared `shift_recipe`/`Operand::ShiftCount`/`dn_*` machinery is reused VERBATIM; only
        // the AluOp + the `(type, dir)` classification differ from ASL/ASR/LSL/LSR/ROL/ROR/ROXL. ROXR is ROXL's
        // right-direction twin — it threads X through an (n+1)-bit rotate (contrast ROL/ROR, which leave X
        // untouched — and the value shifts AS/LS, which set X = C from the value). All eight shift/rotate ops are
        // now loaded; the guard keeps the arm precise regardless.
        let is_roxr = if (opcode >> 6) & 3 == 3 {
            (opcode >> 8) & 1 == 0 && (opcode >> 9) & 3 == 2 // memory: dir RIGHT, type ROX (bits 10-9)
        } else {
            (opcode >> 8) & 1 == 0 && (opcode >> 3) & 3 == 2 // register: dir RIGHT, type ROX (bits 4-3)
        };
        if is_roxr {
            return shift_recipe(opcode, AluOp::Roxr, regs);
        }
    }
    // Nothing above matched — the opcode is illegal or unimplemented (M68000UM §6.3.6, Table 6-2). Line-A
    // (`0xAxxx`) and line-F (`0xFxxx`) are exact top-nibble emulator traps with their own vectors; every
    // other unassigned encoding (incl. the official ILLEGAL `0x4AFC`) is the vector-4 illegal-instruction
    // trap. This makes `decode` TOTAL over the fall-through space — no reachable `todo!()` here (the
    // illegal-EA gates for otherwise-legal arms land in A2b).
    if opcode & 0xF000 == 0xA000 {
        return decode_time_exception_recipe(10); // line-A (unimplemented instruction)
    }
    if opcode & 0xF000 == 0xF000 {
        return decode_time_exception_recipe(11); // line-F (unimplemented instruction)
    }
    decode_time_exception_recipe(4) // ILLEGAL — all other unassigned encodings
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

/// `MOVEQ #imm8,Dn` (`0111 ddd 0 dddddddd`, 0x7000 | dn<<9 | imm8): load the sign-extended 8-bit immediate into
/// the FULL 32 bits of `Dn`, setting N = msb / Z = (value == 0), clearing V/C, PRESERVING X — the `MOVE` flag op
/// ([`AluOp::Move`]) at the **long** boundary. The immediate is the opcode's own low byte
/// ([`Operand::BranchDisp8`] = `sign_extend8(prefetch[0] & 0xFF)`), so there is NO operand fetch (the value is
/// already in the prefetched opcode word). The recipe is the single flag-ALU into `Dn` (full 32) followed by the
/// trailing queue refill — `[Alu{Move,Long,BranchDisp8→DataReg(Dn)}, Prefetch]` (4 cyc, one FC-6 read). No new
/// vocabulary (`AluOp::Move` + `Operand::BranchDisp8` + `Dest::DataReg` all exist).
fn moveq_recipe(opcode: u16) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::Alu {
        op: AluOp::Move,
        size: Size::Long,
        a: Operand::BranchDisp8,
        b: Operand::Zero,
        dst: Dest::DataReg(dn),
    });
    buf.push(MicroOp::Prefetch);
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

/// `CMP.{b,w,l} <ea>,Dn` (`1011 ddd 0SS mmm rrr`, opmode 0/1/2): the flag-only compare `Dn − <ea>` (**Dn is
/// the minuend**, `a`). The ALU is [`AluOp::Cmp`] (SUB's N/Z/V/C with **X preserved**) writing to
/// [`Dest::None`] (no register/scratch write-back).
///
/// For **byte/word** the source-EA skeleton is byte-for-byte the `<ea>,Dn` shape ADD/SUB use — the same
/// [`ea_src`] builder, the same cycle counts and bus stream (CMP.b/.w match ADD.b/.w exactly in the vendored
/// data). For **long** CMP differs from ADD.l in ONE place: a **register-direct (Dn/An) or `#imm` long
/// source's trailing idle is `n2`, not ADD's `n4`** (CMP.l Dn/An/#imm = 6/6/14 cyc vs ADD.l's 8/8/16; every
/// long MEMORY mode is identical at n2). So long CMP uses a dedicated [`cmp_ea_src_long`] that mirrors
/// `ea_src_long` but with the n2 register/#imm idle — pinned to the vendored CMP.l stream.
fn cmp_ea_dn(opcode: u16, size: Size) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    let make_alu = |b| MicroOp::Alu {
        op: AluOp::Cmp,
        size,
        a: dn_operand(dn, size),
        b,
        dst: Dest::None,
    };
    if size == Size::Long {
        cmp_ea_src_long(&mut buf, mode, reg, make_alu);
    } else {
        ea_src(&mut buf, mode, reg, size, make_alu);
    }
    buf.finish()
}

/// The long source-EA sub-sequence for `CMP.l <ea>,Dn` — a flag-only twin of [`ea_src_long`](super::ea::ea_src)
/// that differs ONLY in the **register-direct / `#imm`** trailing idle: CMP.l uses `n2` where ADD.l/SUB.l use
/// `n4` (CMP.l Dn/An/#imm = 6/6/14 cyc, pinned to the vendored CMP.l stream). Every long **memory** mode is
/// identical to `ea_src_long` (same n2 memory idle), so those delegate straight to the shared builder via
/// [`ea_src`] — only the three no-read source modes (Dn 0, An 1, `#imm` 7/4) are re-emitted here with the
/// correct n2.
fn cmp_ea_src_long(
    buf: &mut RecipeBuf,
    mode: u16,
    reg: u8,
    make_alu: impl FnOnce(Operand) -> MicroOp,
) {
    match (mode, reg) {
        // Dn / An direct — no operand read: one refill, the flag-ALU on the full 32-bit register, then the
        // CMP register-source long idle (n2, NOT ADD's n4). Bus: [PF].
        (0, _) | (1, _) => {
            let operand = if mode == 0 {
                Operand::DataRegFull(reg)
            } else {
                Operand::AddrReg(reg)
            };
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(operand));
            buf.push(MicroOp::Internal { cycles: 2 });
        }
        // #imm.l — the 32-bit immediate is two extension words: HI = prefetch[1] (captured into a scratch slot
        // before the refill shifts it out), a refill shifts the LO word in, Combine32 assembles them, then two
        // more refills complete the 3-word fetch; the flag-ALU and the CMP immediate long idle (n2, NOT ADD's
        // n4). Bus: [PF, PF, PF]. The HI capture reads slot 0 while still zero (fresh recipe), so
        // `(0 << 16) | prefetch[1]` parks the HI word unmasked — the same idiom `ea_src_long` uses.
        (7, 4) => {
            const CMP_IMM_HI_SLOT: u8 = 3;
            buf.push(MicroOp::Combine32 {
                hi: 0,
                lo: Operand::ImmWord,
                dst: CMP_IMM_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Combine32 {
                hi: CMP_IMM_HI_SLOT,
                lo: Operand::ImmWord,
                dst: 0,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
            buf.push(MicroOp::Internal { cycles: 2 });
        }
        // Every long MEMORY mode (2/3/4/5/6, 7/0..=3) is byte-for-byte ADD.l's `ea_src_long` (same n2 memory
        // idle), so delegate to the shared `ea_src` builder (which routes long sizes to `ea_src_long`).
        _ => ea_src(buf, mode, reg, Size::Long, make_alu),
    }
}

/// `CMPA.{w,l} <ea>,An` (`1011 aaa 0 11/111 mmm rrr`, opmode 3 = `.w` / 7 = `.l`): the flag-only address
/// compare `An − <ea>` (**An is the minuend**, full 32 bits). The destination is `An` (bits 11-9, the SWAPPED
/// reg field); the source is the usual `mode/reg` in bits 5-0 (all 12 modes legal — An-direct included). The
/// ALU is [`AluOp::Cmpa`] (sign-extends a `.w` source word→long, computes at the long boundary, sets N/Z/V/C,
/// PRESERVES X) writing to [`Dest::None`] (no write-back). Delegates to [`ea_cmpa`], whose source bus stream
/// mirrors MOVEA of the same size plus the uniform trailing `Internal(2)` idle (CMPA = MOVEA + 2 cyc).
fn cmpa_recipe(opcode: u16, size: Size) -> MicroState {
    let dst_reg = ((opcode >> 9) & 7) as u8;
    let src_mode = (opcode >> 3) & 7;
    let src_reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_cmpa(&mut buf, dst_reg, src_mode, src_reg, size);
    buf.finish()
}

/// `ADDA.{w,l} <ea>,An` / `SUBA.{w,l} <ea>,An` (`1101/1001 aaa s11 mmm rrr`, opmode 3 = `.w` / 7 = `.l`): the
/// no-flag address arithmetic `An = An ± src` (`AluOp::Adda`/`Suba`). The destination/augend `An` is bits 11-9
/// (full 32 bits, the minuend for SUBA); the source is the usual `mode/reg` in bits 5-0 (all 12 modes legal —
/// An-direct included). The op sign-extends a `.w` source word→long internally (mirroring `AluOp::MoveA`) and
/// computes at the long boundary; it writes `An` via [`Dest::AddrReg`] and touches **NO** flags.
///
/// Shared by ADDA (L0) and SUBA (L1) — parameterized only by `op`. The source bus stream is fetched by
/// [`ea_src`]: for **`.w`** a uniform trailing `Internal(4)` idle is appended (`ADDA.w`/`SUBA.w` = the MOVEA.w
/// source stream + n4 for every source mode, pinned to the vendored data); for **`.l`** NOTHING extra is
/// appended — `ea_src` routes to `ea_src_long`, whose built-in n4 (register-direct / `#imm`) / n2 (memory)
/// trailing idle already equals `ADD.l <ea>,Dn` byte-for-byte (and thus `ADDA.l`/`SUBA.l`). Byte ADDA/SUBA is
/// illegal and never decoded.
fn adda_suba_recipe(opcode: u16, op: AluOp, size: Size) -> MicroState {
    let dst_reg = ((opcode >> 9) & 7) as u8;
    let src_mode = (opcode >> 3) & 7;
    let src_reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // The no-flag An-write ALU: `a` = the destination An (full 32, the augend/minuend), `b` = the source
    // operand the EA builder fetched, written full-width back to the same An.
    let make_alu = |operand: Operand| MicroOp::Alu {
        op,
        size,
        a: Operand::AddrReg(dst_reg),
        b: operand,
        dst: Dest::AddrReg(dst_reg),
    };
    ea_src(&mut buf, src_mode, src_reg, size, make_alu);
    // ADDA.w/SUBA.w carry a uniform trailing n4 idle (the one cycle-count difference from MOVEA.w, which has no
    // trailing idle). ADDA.l/SUBA.l append nothing — `ea_src_long`'s built-in n4/n2 trailing idle already
    // matches ADD.l <ea>,Dn (and thus ADDA.l/SUBA.l) exactly. Non-bus, so it does not alter the bus stream.
    if size == Size::Word {
        buf.push(MicroOp::Internal { cycles: 4 });
    }
    buf.finish()
}

/// `ADDQ #data,<ea>` / `SUBQ #data,<ea>` (`0101 qqq 0/1 ss mmm rrr`, 0x5xxx, bit8 = 0/1, `ss` = 00/01/10):
/// add/subtract a decode-time quick immediate `data = qqq != 0 ? qqq : 8` (1-8) to/from the destination.
/// Parameterized by `op` (the data/`Dn`/memory ALU op — [`AluOp::Add`] for ADDQ, [`AluOp::Sub`] for SUBQ)
/// and `addr_op` (the An-direct no-flag address op — [`AluOp::Adda`] for ADDQ, [`AluOp::Suba`] for SUBQ).
/// Three destination arms by `mode = (opcode>>3)&7`:
///
/// - **Dn (mode 0):** `[Prefetch, Alu{op, size, a = Dn (the minuend for SUBQ), b = Quick(data), dst = Dn}]`,
///   then a trailing `Internal{4}` iff `.l`. Timing b/w = 4, l = 8.
/// - **An (mode 1):** `[Prefetch, Alu{addr_op, size, a = An, b = Quick(data), dst = An}, Internal{idle}]` with
///   idle = 4 for `.w` (n4) / **2 for `.l` (n2)** — THE QUIRK: ADDQ/SUBQ.l→An = 6 cyc is CHEAPER than the
///   word form's 8, and CHEAPER than ADDA.l's 8; a naive [`adda_suba_recipe`] reuse (n4 for `.l`) would give 8
///   and fail the cycle gate. NO flags. Byte→An is illegal and never decoded (absent from the data).
/// - **memory (2-7):** `ea_dst(mode, reg, size, |a| Alu{op, size, a, b = Quick(data), dst = Scratch(1)})` —
///   identical to [`arith_dn_ea`] but the addend is `Quick` instead of `dn_operand`; the memory operand is the
///   minuend (`a`). Byte/word route through `ea_dst`, long through `ea_dst_long` (via `ea_dst`). An odd word/
///   long EA faults on the RMW read via the E3/E4 address-error abort (in scope).
fn addq_recipe(opcode: u16, op: AluOp, addr_op: AluOp, size: Size) -> MicroState {
    let qqq = ((opcode >> 9) & 7) as u8;
    let data = if qqq == 0 { 8 } else { qqq };
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    match mode {
        // Dn: prefetch, size-masked ALU on the register, then the .l trailing idle (b/w = 4, l = 8).
        0 => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Alu {
                op,
                size,
                a: dn_operand(reg, size),
                b: Operand::Quick(data),
                dst: dn_dest(reg, size),
            });
            if size == Size::Long {
                buf.push(MicroOp::Internal { cycles: 4 });
            }
        }
        // An: prefetch, no-flag full-32 An write, then the bespoke idle — n4 for word (= 8) / n2 for long
        // (= 6, the quirk). Byte→An is never decoded.
        1 => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Alu {
                op: addr_op,
                size,
                a: Operand::AddrReg(reg),
                b: Operand::Quick(data),
                dst: Dest::AddrReg(reg),
            });
            buf.push(MicroOp::Internal {
                cycles: if size == Size::Word { 4 } else { 2 },
            });
        }
        // memory: the alterable-memory RMW skeleton (identical to `arith_dn_ea`, addend = Quick).
        _ => {
            ea_dst(&mut buf, mode, reg, size, |a| MicroOp::Alu {
                op,
                size,
                a,
                b: Operand::Quick(data),
                dst: Dest::Scratch(1),
            });
        }
    }
    buf.finish()
}

/// Whether a DYNAMIC `BTST <ea>` (`0000 ddd 1 00 mmm rrr`) operand `mode`/`reg` is in scope — the FULL
/// read-only source set: `Dn` (0), `(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)` (2-6), `abs.w`/`abs.l`/
/// `d16(PC)`/`d8(PC,Xn)`/`#imm` (7/0..7/4). Mode 001 = MOVEP (the dynamic mode-001 sibling, absent / not
/// decoded) and the illegal `7/5..7/7` are excluded.
#[inline]
fn btst_dyn_src_in_scope(mode: u16, reg: u16) -> bool {
    mode == 0 || (2..=6).contains(&mode) || (mode == 7 && reg <= 4)
}

/// Whether a STATIC `BTST #,<ea>` (`0000 1000 00 mmm rrr`) operand `mode`/`reg` is in scope — the same
/// read-only source set as the dynamic form **MINUS `#imm`** (7/4 is not a static operand and is absent from
/// the data): `Dn` (0), `(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)` (2-6), `abs.w`/`abs.l`/`d16(PC)`/
/// `d8(PC,Xn)` (7/0..7/3).
#[inline]
fn btst_static_src_in_scope(mode: u16, reg: u16) -> bool {
    mode == 0 || (2..=6).contains(&mode) || (mode == 7 && reg <= 3)
}

/// `BTST <ea>` (dynamic `0000 ddd 1 00 mmm rrr` = 0x01xx / static `0000 1000 00 mmm rrr` = 0x08xx): test the
/// single bit of the operand selected by the bit number, setting **only Z** (`Z = NOT(bit)`) — X/N/V/C and the
/// SR system byte are all PRESERVED. READ-ONLY (`Dest::None`, no write). The bit width follows the operand: a
/// `Dn` operand is **32-bit** (`pos = b mod 32`, `Size::Long`), a memory / `#imm` / PC-relative operand is
/// **8-bit** (`pos = b mod 8`, `Size::Byte`) — the `Alu` `size` field carries this. The bit number `b`:
/// **dynamic** = `D[(opcode>>9)&7]` ([`Operand::DataRegFull`], always live, no capture); **static** = the
/// `prefetch[1]` ext word, captured into a scratch slot BEFORE the refill shifts it out (the `cmpi_recipe`
/// interleave) and fed as [`Operand::Scratch`].
///
/// Three operand shapes (timing pinned to the vendored BTST stream; static = dynamic + 4 for the extra bitnum
/// ext word):
/// - **`Dn`** (mode 0): `Size::Long`, a trailing `Internal(2)` bit-test idle. Dynamic `[Prefetch, Alu,
///   Internal(2)]` = **6 cyc FIXED** (NO `pos>=16` variance — BTST is read-only); static (after the leading
///   capture + refill) `[Prefetch, Alu, Internal(2)]` = **10 cyc**.
/// - **`#imm`** (7/4, dynamic only): `Size::Byte`, the operand is the queued immediate read BEFORE the refills
///   (placement `First`), then the same trailing `Internal(2)`: `[Alu(a=ImmWord), Prefetch, Prefetch,
///   Internal(2)]` = **10 cyc**.
/// - **memory / PC-relative** (2-6, 7/0..7/3): `Size::Byte`, the proven `ea_src` byte read → `Alu` on the
///   just-read scratch, **NO trailing idle** (the read replaces it). Reuses `ea_src` verbatim (it already
///   covers `d16(PC)`/`d8(PC,Xn)`). Byte → NO odd-EA faults.
///
/// The static form prepends `[EaCalc(ImmWord → BITNUM_SLOT), Prefetch]` (capture the bitnum, then the refill
/// that consumes it) and routes the EA's own ext words AFTER, so the `ea_src` `DispWord`/brief-ext captures
/// see the EA's words (mirrors `cmpi_recipe`'s immediate-then-EA interleave).
fn btst_recipe(opcode: u16) -> MicroState {
    // Scratch slot holding the captured static bit-number ext word (`prefetch[1]`). Disjoint from `ea_src`'s
    // slots (0 read value, 2 EA, 3 abs.l-HI, 4/5 long halves — BTST byte never uses the long slots), matching
    // `cmpi_recipe`'s convention, so every in-flight value is snapshot-visible.
    const BITNUM_SLOT: u8 = 6;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let static_form = opcode & 0xFF00 == 0x0800;
    // The bit number operand: dynamic = the live `Dn` (full 32 bits, modulo applied in the exec); static =
    // the captured ext word in BITNUM_SLOT.
    let bitnum = if static_form {
        Operand::Scratch(BITNUM_SLOT)
    } else {
        Operand::DataRegFull(((opcode >> 9) & 7) as u8)
    };
    let mut buf = RecipeBuf::new();
    if static_form {
        // Capture `prefetch[1]` (the bit number) into BITNUM_SLOT BEFORE the first refill shifts it out, then
        // that refill (consuming the bitnum word, bringing the EA's first ext word — if any — into prefetch[1]).
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: BITNUM_SLOT,
        });
        buf.push(MicroOp::Prefetch);
    }
    if mode == 0 {
        // Dn operand — Long (mod 32), no memory: one refill, the bit-test Alu on the full register, the
        // trailing bit-test idle (n2). Dynamic = 6 cyc; static = 10 (the leading capture + refill above).
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Btst,
            size: Size::Long,
            a: Operand::DataRegFull(reg),
            b: bitnum,
            dst: Dest::None,
        });
        buf.push(MicroOp::Internal { cycles: 2 });
    } else if mode == 7 && reg == 4 {
        // #imm operand (dynamic only) — Byte (mod 8): the Alu reads the immediate from `prefetch[1]` BEFORE
        // the two refills shift it out (placement First), then the refills, then the trailing bit-test idle
        // (n2). Bus [r, r, n2] = 10 cyc.
        buf.push(MicroOp::Alu {
            op: AluOp::Btst,
            size: Size::Byte,
            a: Operand::ImmWord,
            b: bitnum,
            dst: Dest::None,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Internal { cycles: 2 });
    } else {
        // Memory / PC-relative operand — Byte (mod 8), NO trailing idle: the proven `ea_src` byte read feeds
        // the bit-test Alu on the just-read scratch (no write). `ea_src` already covers d16(PC)/d8(PC,Xn).
        ea_src(&mut buf, mode, reg, Size::Byte, |operand| MicroOp::Alu {
            op: AluOp::Btst,
            size: Size::Byte,
            a: operand,
            b: bitnum,
            dst: Dest::None,
        });
    }
    buf.finish()
}

/// The SHARED read-modify-WRITE bit op recipe builder for `BCHG`/`BCLR`/`BSET` (dynamic `0000 ddd 1 tt mmm rrr`
/// = 0x01xx / static `0000 1000 tt mmm rrr` = 0x08xx, `tt` = 01 BCHG / 10 BCLR / 11 BSET): `BTST` (test the
/// bit, set ONLY Z = NOT(the PRE-modify bit), X/N/V/C + the SR system byte PRESERVED) PLUS a write of the
/// modified operand (the `op` exec applies the toggle/clear/set). The Z flag is from the bit BEFORE the modify
/// (the read value), not after. Parameterized by the `AluOp` (`Bchg`/`Bclr`/`Bset`) and `reg_base` (the
/// register-form base idle: `2` = `n2` for BCHG/BSET, `4` = `n4` for BCLR) so the trio share one builder.
///
/// The bit number `b`: **dynamic** = `D[(opcode>>9)&7]` ([`Operand::DataRegFull`], always live, no capture);
/// **static** = the `prefetch[1]` ext word, captured into a scratch slot BEFORE the refill shifts it out (the
/// `cmpi_recipe` interleave) and fed as [`Operand::Scratch`]. The static form prepends `[EaCalc(ImmWord →
/// BITNUM_SLOT), Prefetch]` and routes the EA's own ext words AFTER (static = dynamic + 4 everywhere).
///
/// Two dest shapes:
/// - **`Dn`** (mode 0): `Size::Long` (mod 32), a FULL-32 register write with one bit flipped ([`Dest::DataReg`]).
///   `[Prefetch, Alu, Internal(reg_base), (+Internal(2) iff the DECODE-TIME `pos >= 16`)]`. The `pos >= 16` `+2`
///   is the LOAD-BEARING subtlety: the bit number is read HERE at decode (the live `Dn` for dynamic / the
///   captured `prefetch[1]` for static), so the register recipe length depends on `regs`. Dynamic BCHG = 6 cyc
///   (pos<16) / 8 (pos>=16); static = 10 / 12.
/// - **memory** (2-6, 7/0, 7/1): `Size::Byte` (mod 8), the NEG-family read→modify→write RMW via [`ea_dst`] (read
///   the byte, refill, the bit-modify `Alu` into `Scratch(1)`, write it back). NO register `+2` (byte/mod-8
///   timing is fixed per mode). Byte → NO odd-EA address-error faults. Dynamic cost `(An)`/`(An)+` 12, `−(An)`
///   14, `d16(An)` 16, `d8(An,Xn)` 18, `abs.w` 16, `abs.l` 20; static = +4.
fn bit_recipe(opcode: u16, op: AluOp, reg_base: u16, regs: &Registers) -> MicroState {
    // Scratch slot holding the captured static bit-number ext word (`prefetch[1]`). Disjoint from `ea_dst`'s
    // slots (0 read value, 1 write result, 2 EA, 3 abs.l-HI — byte memory never uses the long slots), matching
    // `btst_recipe`/`cmpi_recipe`'s convention, so every in-flight value is snapshot-visible.
    const BITNUM_SLOT: u8 = 6;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let static_form = opcode & 0xFF00 == 0x0800;
    // The bit number operand + its DECODE-TIME value (the live `Dn` for dynamic / the captured `prefetch[1]`
    // for static). `pos = bitnum % 32` selects the REGISTER-only `pos >= 16` `+2` (memory is fixed per mode).
    let (bitnum, bitnum_val) = if static_form {
        (Operand::Scratch(BITNUM_SLOT), regs.prefetch[1] as u32)
    } else {
        let n = ((opcode >> 9) & 7) as u8;
        (Operand::DataRegFull(n), regs.d[n as usize])
    };
    let mut buf = RecipeBuf::new();
    if static_form {
        // Capture `prefetch[1]` (the bit number) into BITNUM_SLOT BEFORE the first refill shifts it out, then
        // that refill (consuming the bitnum word, bringing the EA's first ext word — if any — into prefetch[1]).
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: BITNUM_SLOT,
        });
        buf.push(MicroOp::Prefetch);
    }
    if mode == 0 {
        // Dn dest — Long (mod 32), the FULL-32 register write with one bit flipped. One refill, the bit-modify
        // Alu, the register base idle, and the DECODE-TIME `pos >= 16` `+2` (register-only). Dynamic BCHG = 6
        // (pos<16) / 8 (pos>=16); static (after the leading capture + refill) = 10 / 12.
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op,
            size: Size::Long,
            a: Operand::DataRegFull(reg),
            b: bitnum,
            dst: Dest::DataReg(reg),
        });
        buf.push(MicroOp::Internal { cycles: reg_base });
        if bitnum_val % 32 >= 16 {
            buf.push(MicroOp::Internal { cycles: 2 });
        }
    } else {
        // Memory dest — Byte (mod 8), the NEG-family read→modify→write RMW (`ea_dst` byte): read the old byte,
        // refill, the bit-modify Alu into `Scratch(1)` (Z from the PRE-modify bit), write it back. NO register
        // `+2` (byte/mod-8 timing is fixed per mode). Byte → NO odd-EA faults.
        ea_dst(&mut buf, mode, reg, Size::Byte, |operand| MicroOp::Alu {
            op,
            size: Size::Byte,
            a: operand,
            b: bitnum,
            dst: Dest::Scratch(1),
        });
    }
    buf.finish()
}

/// The SHARED shift/rotate recipe builder (modelled on [`bit_recipe`]) — used VERBATIM by ASL (this commit)
/// and ASR/LSL/LSR/ROL/ROR/ROXL/ROXR (S1-S7); only the `op` (and the decode `(type, dir)` arm) differ. Two
/// forms, classified by OPCODE (bits 7-6):
///
/// - **Register** (`1110 ccc d ss ir tt rrr`, bits 7-6 != 11): shift `Dn` (bits 2-0) at `size` (bits 7-6 =
///   00/01/10 → b/w/l). The count is **decode-time** (the load-bearing data dependency): the **immediate**
///   form (bit 5 = 0) is `ccc != 0 ? ccc : 8` (1-8); the **`Dn`-count** form (bit 5 = 1) is the LIVE
///   `regs.d[ccc] & 63` (0-63, mod 64 — read HERE at decode, exactly like Scc's `n2` / DBcc's counter /
///   `bit_recipe`'s `pos >= 16`; `ccc == rrr` is legal — count reg == operand reg). The recipe is
///   `[Prefetch, Alu { op, size, a: dn_src(rrr,size), b, dst: dn_dest(rrr,size) }, Internal { (base-4) +
///   2*cnt }]` where `base` = 6 (`.b`/`.w`) / 8 (`.l`) → total `6 + 2*cnt` / `8 + 2*cnt` (the `Prefetch`
///   refill is the leading 4). The count **operand** `b` = [`Operand::ShiftCount`] (imm, the literal 1-8) /
///   [`Operand::DataRegFull`] (`Dn`-count, the exec masks `& 63`).
/// - **Memory shift-by-1** (`1110 0TTd 11 mmm rrr`, bits 7-6 == 11, **`.w` only**, count always 1): the WORD
///   `ea_dst` read-modify-WRITE (read the word → shift-by-1 → write the word), byte-for-byte CLR.w/NEG.w's
///   memory path, with `b = Operand::ShiftCount(1)`. An odd EA address-errors on the READ (SSW low5 = 0x15,
///   the E3/E4 abort), exactly like NEG.w/CLR.w. No register `+2` (memory timing is fixed per EA mode).
fn shift_recipe(opcode: u16, op: AluOp, regs: &Registers) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if (opcode >> 6) & 3 == 3 {
        // Memory shift-by-1 (word only): the CLR.w/NEG.w word RMW — read word → shift1 → write word back.
        // An odd EA faults on the READ (E3/E4 abort), exactly like NEG.w/CLR.w. `b` is the constant 1.
        ea_dst(&mut buf, mode, reg, Size::Word, |operand| MicroOp::Alu {
            op,
            size: Size::Word,
            a: operand,
            b: Operand::ShiftCount(1),
            dst: Dest::Scratch(1),
        });
    } else {
        // Register form: shift `Dn` (bits 2-0) by the DECODE-TIME count. `size` from bits 7-6.
        let size = match (opcode >> 6) & 3 {
            0 => Size::Byte,
            1 => Size::Word,
            _ => Size::Long, // 10
        };
        let ccc = ((opcode >> 9) & 7) as u8;
        // The count + its operand: immediate (bit 5 = 0) is `ccc != 0 ? ccc : 8` baked as a `ShiftCount`;
        // the `Dn`-count (bit 5 = 1) is the LIVE `regs.d[ccc] & 63` read here, passed as `DataRegFull(ccc)`
        // (the exec masks `& 63` at run time). The decoded `cnt` value drives the idle ONLY (it is the same
        // count the exec computes), so the recipe length depends on `regs` for the `Dn` form.
        let (cnt, b): (u16, Operand) = if (opcode >> 5) & 1 == 0 {
            let c = if ccc != 0 { ccc } else { 8 };
            (c as u16, Operand::ShiftCount(c))
        } else {
            (
                (regs.d[ccc as usize] & 63) as u16,
                Operand::DataRegFull(ccc),
            )
        };
        let base: u16 = if size == Size::Long { 8 } else { 6 };
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op,
            size,
            a: dn_operand(reg, size),
            b,
            dst: dn_dest(reg, size),
        });
        buf.push(MicroOp::Internal {
            cycles: (base - 4) + 2 * cnt,
        });
    }
    buf.finish()
}

/// `TST.{b,w,l} <ea>` (`0100 1010 SS mmm rrr`, 0x4A00/4A40/4A80): the flag-only test `<ea> − 0` — set
/// N = msb(operand) / Z = (operand == 0), clear V/C, PRESERVE X, write NOTHING (`AluOp::Cmp` with `b =
/// Operand::Zero` + `Dest::None`). The data-alterable EA (`Dn`/`(An)`/`(An)+`/`-(An)`/`d16(An)`/`d8(An,Xn)`/
/// `abs.w`/`abs.l`) is fetched by [`ea_tst`], which (unlike CMP/ADD) emits NO trailing idle for any size.
fn tst_recipe(opcode: u16, size: Size) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_tst(&mut buf, mode, reg, size);
    buf.finish()
}

/// `CLR.{b,w,l} <ea>` (`0100 0010 SS mmm rrr`, 0x4200/4240/4280): clear the data-alterable EA to 0, setting
/// **Z=1, N=0, V=0, C=0** and **PRESERVING X** (exactly `move_flags(0)`), with NO operand value retained.
///
/// CLR is a **read-then-write**: the 68000 READS the EA (the value is discarded), refills, then WRITES 0. So a
/// memory destination reuses the existing RMW path ([`ea_dst`] for `.b`/`.w`, `ea_dst_long` for `.l` — same
/// read/refill/ALU/write skeleton ADD/SUB use) with `make_alu` building `Alu{Move, size, a: Zero, dst:
/// Scratch(1)}`: the Move sets the CCR from the zero value and parks the 0 the trailing `Write` stores. The
/// read can itself address-error on an odd EA (low5 = 0x15) — the same E3/E4 abort path, never modeled as
/// write-only. The `.l` path inherits the reversed long-store order (write lo @ EA+2, then hi @ EA).
///
/// `Dn`-direct (mode 0) has **no memory access**: a single `Prefetch` then `Alu{Move, size, a: Zero, dst:
/// dn_dest(Dn,size)}` (the size-masked register clear). CLR.b/.w Dn = 4 cyc; **CLR.l Dn = 6 cyc** — a `.l`
/// register clear carries one trailing `Internal(2)` idle (pinned to the vendored `4282` anchor), unlike the
/// byte/word forms.
fn clr_recipe(opcode: u16, size: Size) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // The flag-only Move-of-zero ALU: `a = Zero` (b ignored). For a memory destination it parks the 0 in
    // scratch 1 (the slot `ea_dst`'s write stores); for `Dn`-direct it writes the size-masked register.
    if mode == 0 {
        // Dn-direct — no memory: one refill, then the Move-of-zero into Dn (size-masked). CLR.l Dn adds one
        // trailing idle (n2 → 6 cyc); CLR.b/.w Dn are 4 cyc with no idle.
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Move,
            size,
            a: Operand::Zero,
            b: Operand::Zero,
            dst: dn_dest(reg, size),
        });
        if size == Size::Long {
            buf.push(MicroOp::Internal { cycles: 2 });
        }
    } else {
        // Memory destination — the RMW path: read the EA (discarded), refill, Move-of-zero (sets flags + parks
        // the 0), write the 0 back at the same address. `.l` uses the reversed long-store via `ea_dst_long`.
        ea_dst(&mut buf, mode, reg, size, |_discarded| MicroOp::Alu {
            op: AluOp::Move,
            size,
            a: Operand::Zero,
            b: Operand::Zero,
            dst: Dest::Scratch(1),
        });
    }
    buf.finish()
}

/// `MOVEfromSR <ea>` (`0100 0000 11 mmm rrr`, opcode & 0xFFC0 == 0x40C0): move the FULL 16-bit status register
/// (incl the system byte S/T/I) to a data-alterable EA. NOT privileged on the 68000, sets **NO flags** — the SR
/// is byte-identical before/after (it WRITES the SR value but does not modify it, so a CLR-style `AluOp::Move`,
/// which SETS Z=1/N=0, is WRONG; this uses the no-flag [`MicroOp::SetWord`] with [`Operand::Sr`]).
///
/// `MOVEfromSR` is CLR.w's **twin** with value = SR and no flags. A **memory destination** is byte-for-byte
/// [`clr_recipe`]'s word read-then-write RMW ([`ea_dst`] with `Size::Word`): it READS the EA word (the value is
/// DISCARDED — the 68000 read-before-write quirk), refills, then WRITES SR back at the same address. So an odd EA
/// faults on the dummy READ first (low5 = 0x15, a READ address error the E3/E4 abort covers) — reusing `ea_dst`
/// gives this for free (no write-only path). Per-mode cyc = word RMW `8 + ea`: `(An)`/`(An)+` 12, `-(An)` 14,
/// `d16(An)` 16, `d8(An,Xn)` 18, `abs.w` 16, `abs.l` 20. `(A7)`/`(A7)+`/`-(A7)` are all clean and in scope.
///
/// `Dn`-direct (mode 0) has **no memory access**: a 0-cycle `SetWord { Sr → Dn.w }` (the low word = SR, the
/// **high word preserved**) then a `Prefetch` + trailing `Internal(2)` — ordered so the bus stream is `r` (the
/// FC6 prefetch @ pc+4) then `n2`, giving **6 cyc** (pinned to the vendored `40c6` anchor).
fn movefrom_sr_recipe(opcode: u16) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if mode == 0 {
        // Dn-direct — no memory: write SR into Dn's low word (high word preserved), then one refill + a trailing
        // idle (n2 → 6 cyc, bus stream `r` then `n2`).
        buf.push(MicroOp::SetWord {
            value: Operand::Sr,
            dst: Dest::DataRegLow16(reg),
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Internal { cycles: 2 });
    } else {
        // Memory destination — CLR.w's read-then-write RMW, byte-for-byte EXCEPT the closure writes SR with NO
        // flags (CLR uses `AluOp::Move`, which SETS Z=1). The dummy READ (discarded) faults first on an odd EA.
        ea_dst(&mut buf, mode, reg, Size::Word, |_discarded| {
            MicroOp::SetWord {
                value: Operand::Sr,
                dst: Dest::Scratch(1),
            }
        });
    }
    buf.finish()
}

/// `MOVEtoCCR <ea>` (`0x44C0 | ea`, `to_sr = false`) / `MOVEtoSR <ea>` (`0x46C0 | ea`, `to_sr = true`): move a
/// WORD from a data source EA into the CCR / SR. `to_sr = false` → `CCR = (SR & 0xFF00) | (src.w & 0x1F)` (the
/// SR SYSTEM byte preserved, CCR bits 5-7 forced 0, S never changes) via [`MicroOp::LoadCcr`]; `to_sr = true`
/// → `SR = src.w & 0xA71F` via [`MicroOp::LoadSr`] (a later commit — which CAN clear S, switching the trailing
/// reads' function code). Reuses the immediate [`to_sr_recipe`] shape generalized to the full data-source EA
/// set.
///
/// The ONE novel bus shape (verified 0-mismatch over every mode): the operand read (with its extension-word
/// refills) → `Internal(4)` (the `n4` idle) → `LoadCcr`/`LoadSr` → a DISCARD flush `Read` @ `pc+2` (the
/// pipe-flush re-read of the word already in the queue head, at the final PC) → one `Prefetch` (the
/// instruction-completing refill). The two trailing reads reload the queue at `[finalPC, finalPC+2]`; the
/// flush re-read is the extra bus access vs a normal op's single final prefetch. The `LoadCcr`/`LoadSr` runs
/// BEFORE the flush-read + prefetch — the hinge where MOVEtoSR's FC switch applies to BOTH trailing reads (for
/// CCR, S never changes so both stay FC6). TIMING is uniform: `cycles = ea_word_read_cost + 12` (reg 12,
/// (An)/(An)+ 16, -(An) 18, d16(An)/abs.w/d16(PC) 20, d8(An,Xn)/d8(PC,Xn) 22, abs.l 24, #imm 16).
///
/// `Dn`-direct (mode 0) has NO operand read: `[Internal(4), LoadX(DataRegLow16), Read @ pc+2 (flush),
/// Prefetch]`, bus `n4, r@pc+2, r@pc+4`, 12 cyc. `#imm` (7/4) reduces to the exact [`to_sr_recipe`] shape (the
/// leading discard `Read @ pc+4` + `LoadX(ImmWord)` + two `Prefetch`, but `Internal(4)` not ANDItoSR's
/// `Internal(8)` — there is NO separate operand read, the immediate is `prefetch[1]`). Every memory mode goes
/// through [`ea_read_word_operand`] (the operand → scratch 0, then the trailing legs here).
fn move_to_sr_ccr_recipe(opcode: u16, to_sr: bool) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // The write-back micro-op, given the operand it loads (scratch 0 for a memory read, or the direct
    // register / immediate). LoadSr masks to 0xA71F (full SR); LoadCcr keeps only the low 5 CCR bits and
    // preserves the SR system byte.
    let load = |value: Operand| {
        if to_sr {
            MicroOp::LoadSr { value }
        } else {
            MicroOp::LoadCcr { value }
        }
    };
    if mode == 7 && reg == 4 {
        // #imm — the to_sr_recipe shape: the immediate is prefetch[1] (no separate operand read). The leading
        // discard Read @ pc+4 (FC=6) is re-read by the first re-prefetch; Internal(4) (the n4 idle, NOT
        // ANDItoSR's n8); the write-back from ImmWord; then the two completing refills. Bus: r@pc+4, n4,
        // r@pc+4, r@pc+6, 16 cyc.
        buf.push(MicroOp::Read {
            addr: Operand::PcPlus(4),
            fc: super::microop::Fc::Program,
            size: Size::Word,
            dst: 0,
        });
        buf.push(MicroOp::Internal { cycles: 4 });
        buf.push(load(Operand::ImmWord));
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
        return buf.finish();
    }
    if mode == 0 {
        // Dn-direct — no operand read: n4, the write-back from Dn's low word, the flush re-read @ pc+2, the
        // completing refill @ pc+4. Bus: n4, r@pc+2, r@pc+4, 12 cyc.
        buf.push(MicroOp::Internal { cycles: 4 });
        buf.push(load(Operand::DataRegLow16(reg)));
    } else {
        // Every memory / PC-relative mode: the operand read leg (→ scratch 0, with its (word_count − 1)
        // preceding refills), then n4, then the write-back from the read operand. The flush re-read + refill
        // follow below (uniform for all modes). An odd EA faults on the operand read (the E3/E4 abort).
        ea_read_word_operand(&mut buf, mode, reg);
        buf.push(MicroOp::Internal { cycles: 4 });
        buf.push(load(Operand::Scratch(0)));
    }
    // The pipe-flush re-read of the word already in the queue head (at the final PC = pc+2, a program-space
    // read) THEN the instruction-completing refill. For CCR S never changes so both stay FC6; for MOVEtoSR the
    // preceding LoadSr may have cleared S, and `regs.fc` computes each read's FC at execution time.
    buf.push(MicroOp::Read {
        addr: Operand::PcPlus(2),
        fc: super::microop::Fc::Program,
        size: Size::Word,
        dst: 0,
    });
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `Scc.b <ea>` (`0101 cccc 11 mmm rrr`, opcode & 0xF0C0 == 0x50C0): the conditional byte set — write `0xFF`
/// if the condition `cc` (bits 11-8) is TRUE else `0x00`, with **NO flags** (`final.sr == initial.sr`). The
/// condition is resolved **at decode time** (like Bcc/DBcc/TRAPV) against the LIVE CCR via [`condition_true`],
/// so the recipe stays a flat linear sequence; the resulting constant `v` is baked into the
/// [`MicroOp::SetByte`] (the no-flag byte write — NOT [`AluOp::Move`], which CLR uses because CLR DOES set
/// flags). cc 0 = `T` (always 0xFF) and cc 1 = `F` (always 0x00) are BOTH legal for Scc. Byte-only.
///
/// `Dn`-direct (mode 0) has **no memory access**: a single `Prefetch` then the `SetByte` into `Dn`'s low byte
/// (the upper 24 bits preserved). When the condition is TRUE the 68000 spends one extra `Internal(2)` idle —
/// the ONLY true/false timing difference: **FALSE = 4 cyc, TRUE = 6 cyc** (pinned to the vendored `50c4`/`51c5`
/// anchors).
///
/// A **memory destination** is byte-for-byte [`clr_recipe`]'s read-then-write RMW ([`ea_dst`] with `Size::Byte`)
/// EXCEPT the `make_alu` closure emits a `SetByte { value: v, dst: Scratch(1) }` (the trailing `Write` stores
/// `Scratch(1)`'s low byte) — it READS the EA byte (discarded), refills, then WRITES `v` with NO flags. The
/// memory cost is condition-independent (the byte write happens either way) and byte-IDENTICAL to CLR:
/// `(An)`/`(An)+` 12, `−(An)` 14, `d16(An)` 16, `d8(An,Xn)` 18, `abs.w` 16, `abs.l` 20. Byte-only → NO odd-EA
/// address-error faults.
fn scc_recipe(opcode: u16, regs: &Registers) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let cond = condition_true(((opcode >> 8) & 0xF) as u8, regs.sr);
    let v: u8 = if cond { 0xFF } else { 0x00 };
    let mut buf = RecipeBuf::new();
    if mode == 0 {
        // Dn-direct — no memory: one refill, then the no-flag byte set into Dn's low byte. The TAKEN
        // condition adds one trailing Internal(2) idle (6 cyc vs the 4 of the not-taken form).
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::SetByte {
            value: v,
            dst: Dest::DataRegLow8(reg),
        });
        if cond {
            buf.push(MicroOp::Internal { cycles: 2 });
        }
    } else {
        // Memory destination — the RMW path, byte-for-byte `clr_recipe`'s EXCEPT the closure writes the
        // conditional constant `v` with NO flags (CLR uses `AluOp::Move`, which SETS flags; Scc must not).
        // Byte-only (no `ea_dst_long`).
        ea_dst(&mut buf, mode, reg, Size::Byte, |_discarded| {
            MicroOp::SetByte {
                value: v,
                dst: Dest::Scratch(1),
            }
        });
    }
    buf.finish()
}

/// `NEG.{b,w,l} <ea>` (`0100 0100 SS mmm rrr`, 0x4400/4440/4480): negate the data-alterable EA — `res = (0 − d)
/// & mask` with FULL SUBTRACT flags (NEG is literally `0 − d`): N = msb(res), Z = (res == 0), V = (d == sign-min)
/// (the 0-minus-itself overflow), C = X = (d != 0) borrow (`AluOp::Neg` ≡ `Sub(0, d)`, operand order `lhs = 0,
/// rhs = d`).
///
/// The SHARED single-operand recipe builder (modelled on [`clr_recipe`], parameterized by `AluOp` and `Size` so
/// `NEGX`/`NOT` reuse it): for a **memory destination** it is byte-for-byte [`clr_recipe`]'s read-then-write RMW
/// (`ea_dst`/`ea_dst_long`) EXCEPT the `make_alu` closure USES the read `operand` as the unary source `a` (CLR
/// discards it and writes 0 — NEG transforms it). An odd EA faults on the READ (the E3/E4 abort), exactly like
/// CLR — there is no write-only path. `.l` routes through `ea_dst_long` automatically (the reversed long store,
/// lo @ EA+2 then hi @ EA).
///
/// `Dn`-direct (mode 0) has **no memory access**: a single `Prefetch` then `Alu{op, size, a: <Dn by size>, b:
/// Zero, dst: dn_dest(Dn,size)}` (the size-masked register write-back). NEG.b/.w Dn = 4 cyc; **NEG.l Dn = 6 cyc**
/// — the `.l` register form carries one trailing `Internal(2)` idle (pinned to the vendored `4482` anchor),
/// unlike the byte/word forms.
fn neg_family_recipe(opcode: u16, op: AluOp, size: Size) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if mode == 0 {
        // Dn-direct — no memory: one refill, then the unary op on Dn (size-masked). NEG.l Dn adds one trailing
        // idle (n2 → 6 cyc); NEG.b/.w Dn are 4 cyc with no idle.
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op,
            size,
            a: dn_operand(reg, size),
            b: Operand::Zero,
            dst: dn_dest(reg, size),
        });
        if size == Size::Long {
            buf.push(MicroOp::Internal { cycles: 2 });
        }
    } else {
        // Memory destination — the RMW path, IDENTICAL to `clr_recipe`'s EXCEPT the closure USES the read
        // `operand` as the unary source `a` (CLR discards it and writes 0). `.l` uses the reversed long-store via
        // `ea_dst_long`.
        ea_dst(&mut buf, mode, reg, size, |operand| MicroOp::Alu {
            op,
            size,
            a: operand,
            b: Operand::Zero,
            dst: Dest::Scratch(1),
        });
    }
    buf.finish()
}

/// `NBCD <ea>` (`0100 1000 00 mmm rrr`, opcode & 0xFFC0 == 0x4800, data-alterable modes): the packed-DECIMAL
/// negate `<ea> = 0 −₁₀ <ea> − X`, BYTE-ONLY. Structurally IDENTICAL to [`neg_family_recipe`] (the single
/// data-alterable EA read-modify-write: `Dn`-direct with no memory access; else the [`ea_dst`] RMW at
/// `Size::Byte`) — the ONLY differences are the dedicated `AluOp::Nbcd` (= the SBCD core with `dst = 0`,
/// `src = operand`; sticky Z; X_out = C_out) and the register-form idle: **NBCD `Dn` = 6 cyc** (a trailing
/// `Internal(2)`, unlike NEG.b/.w `Dn` = 4). Byte-only → NO alignment fault (`-(A7).b` steps by 2 inside
/// `ea_dst`). The memory-mode timings fall straight out of the byte `ea_dst` stream: `(An)` = 12, `(An)+` =
/// 12, `-(An)` = 14, `d16(An)` = 16, `d8(An,Xn)` = 18, `abs.w` = 16, `abs.l` = 20.
fn nbcd_recipe(opcode: u16) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if mode == 0 {
        // Dn-direct — no memory: one refill, then the BCD negate on Dn (byte, `a` = Dn = the operand, `b`
        // ignored/Zero), then a trailing `Internal(2)` idle (NBCD Dn = 6 cyc).
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Nbcd,
            size: Size::Byte,
            a: dn_operand(reg, Size::Byte),
            b: Operand::Zero,
            dst: dn_dest(reg, Size::Byte),
        });
        buf.push(MicroOp::Internal { cycles: 2 });
    } else {
        // Memory destination — the byte RMW path, IDENTICAL to NEG/NOT's `ea_dst` (byte-only → no `ea_dst_long`
        // path, so `-(A7)` steps by 2 and no operand ever faults on alignment). The read operand is `a`; `b` is
        // ignored (Zero), the SBCD core hardcodes `dst = 0`.
        ea_dst(&mut buf, mode, reg, Size::Byte, |operand| MicroOp::Alu {
            op: AluOp::Nbcd,
            size: Size::Byte,
            a: operand,
            b: Operand::Zero,
            dst: Dest::Scratch(1),
        });
    }
    buf.finish()
}

/// The `-(An)` byte step for the X-arith predecrement forms: 2 for `A7` (keep the SP even), else the size's
/// step (word 2, long 4, byte 1). Mirrors [`super::ea::step_bytes`] (a private helper there) for the ADDX /
/// SUBX / ABCD / SBCD two-operand predecrement.
#[inline]
fn xarith_step(size: Size, reg: u8) -> i8 {
    match size {
        Size::Word => 2,
        Size::Long => 4,
        Size::Byte => {
            if reg == 7 {
                2
            } else {
                1
            }
        }
    }
}

/// `ADDX`/`SUBX`/`ABCD`/`SBCD` — the X-flag arithmetic cluster's TWO recipe shapes, selected by `M = bit 3`.
///
/// **M = 0 — register-direct** (`<op> Dy,Dx`): read `Dy` (src) and `Dx` (dst) at `size`, run the dedicated
/// `AluOp` (carrying the incoming X into the value and the carry, sticky Z), write `Dx`. Bus: `[Prefetch]`
/// (the queue refill). Length 4 (`.b`/`.w`) / 8 (`.l`, a trailing `Internal(4)` idle) for ADDX/SUBX.
///
/// **M = 1 — two-operand predecrement RMW** (`<op> -(Ay),-(Ax)`): the leading predecrement idle (`n2`), then
/// **predecrement `Ay` and read the src, predecrement `Ax` and read the dst, run the Alu, refill, write the
/// result @ `Ax`**. The predecrement is a running-register decrement (an `AdjustAddr` committed BEFORE the
/// read), so the same-register (`rx == ry`) case decrements the ALREADY-decremented value in sequence: src @
/// `A − step`, dst @ `A − 2·step`, final reg = `A − 2·step`. `-(A7).b` steps by 2 (keep the SP even). For a
/// long operand each access is a REVERSED word pair (low word @ base+2 first, then high word @ base — the
/// MOVE.l `-(An)` low-word-first precedent), modeled as two sequential `−2` decrements so an odd-address
/// fault leaves the register decremented by only 2. An odd predecremented address is a group-0 address error
/// the EXISTING E3/E4 abort covers on the FIRST odd word access: byte NEVER faults; word/long fault iff a
/// predecremented address is odd — src-odd → only `Ay` committed (the abort fires on the src read before
/// `Ax` is touched); dst-odd → both committed. Bus (`.b`/`.w`): `[n2, r src, r dst, PF, w dst]`, length 18.
fn xarith_recipe(opcode: u16, op: AluOp, size: Size) -> MicroState {
    let rx = ((opcode >> 9) & 7) as u8; // destination (Dx / Ax)
    let ry = (opcode & 7) as u8; // source (Dy / Ay)
    let mem = (opcode >> 3) & 1 == 1; // M = bit 3
    let data = super::microop::Fc::Data;
    let mut buf = RecipeBuf::new();
    if !mem {
        // Register-direct: the queue refill, then the dedicated Alu on Dx (dst = `a`) + Dy (src = `b`).
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op,
            size,
            a: dn_operand(rx, size),
            b: dn_operand(ry, size),
            dst: dn_dest(rx, size),
        });
        // The register-direct idle that sets the length: ADDX/SUBX `.l` = 8 (a trailing `Internal(4)`);
        // ABCD/SBCD (byte-only) = 6 (a trailing `Internal(2)` — the decimal ops carry one extra internal cycle
        // vs a plain `ADDX.b Dn,Dn` = 4). ADDX/SUBX `.b`/`.w` = 4 (no idle).
        match op {
            AluOp::Abcd | AluOp::Sbcd => buf.push(MicroOp::Internal { cycles: 2 }),
            _ if size == Size::Long => buf.push(MicroOp::Internal { cycles: 4 }),
            _ => {}
        }
        return buf.finish();
    }
    // Two-operand predecrement RMW. Scratch slots (all distinct so every in-flight value is snapshot-visible):
    const SRC_VAL: u8 = 0; // the assembled source operand
    const DST_VAL: u8 = 1; // the assembled destination operand
    const RES: u8 = 2; // the ALU result (parked for the write-back)
    const LONG_LO: u8 = 3; // the low word of a long operand (before Combine32)
    const DST_LO_ADDR: u8 = 4; // the long dst low-word address (Ax base + 2)

    // The leading predecrement idle (n2).
    buf.push(MicroOp::Internal { cycles: 2 });
    if size == Size::Long {
        // Long: each operand is a REVERSED word pair. Model the predecrement as two sequential −2 steps
        // (low word @ the first −2, high word @ the second −2) so a mid-read odd fault commits −2 only.
        // src @ (Ay): −2, read lo @ Ay; −2, read hi @ Ay; combine (hi << 16 | lo).
        buf.push(MicroOp::AdjustAddr { reg: ry, delta: -2 });
        buf.push(MicroOp::Read {
            addr: Operand::AddrReg(ry),
            fc: data,
            size: Size::Word,
            dst: LONG_LO,
        });
        buf.push(MicroOp::AdjustAddr { reg: ry, delta: -2 });
        buf.push(MicroOp::Read {
            addr: Operand::AddrReg(ry),
            fc: data,
            size: Size::Word,
            dst: SRC_VAL,
        });
        buf.push(MicroOp::Combine32 {
            hi: SRC_VAL,
            lo: Operand::Scratch(LONG_LO),
            dst: SRC_VAL,
        });
        // dst @ (Ax): −2, read lo @ Ax; −2, read hi @ Ax; combine. (Ax read AFTER Ay's decrements, so the
        // same-register case aliases correctly — the running register is already Ay's decremented value.)
        buf.push(MicroOp::AdjustAddr { reg: rx, delta: -2 });
        buf.push(MicroOp::EaCalc {
            base: Operand::AddrReg(rx),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: DST_LO_ADDR,
        });
        buf.push(MicroOp::Read {
            addr: Operand::AddrReg(rx),
            fc: data,
            size: Size::Word,
            dst: LONG_LO,
        });
        buf.push(MicroOp::AdjustAddr { reg: rx, delta: -2 });
        buf.push(MicroOp::Read {
            addr: Operand::AddrReg(rx),
            fc: data,
            size: Size::Word,
            dst: DST_VAL,
        });
        buf.push(MicroOp::Combine32 {
            hi: DST_VAL,
            lo: Operand::Scratch(LONG_LO),
            dst: DST_VAL,
        });
        // Alu, then the reversed long write (low word @ Ax+2 FIRST, then the refill, then high word @ Ax).
        buf.push(MicroOp::Alu {
            op,
            size,
            a: Operand::Scratch(DST_VAL),
            b: Operand::Scratch(SRC_VAL),
            dst: Dest::Scratch(RES),
        });
        buf.push(MicroOp::Write {
            addr: Operand::Scratch(DST_LO_ADDR),
            fc: data,
            size: Size::Word,
            value: Operand::Scratch(RES),
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Write {
            addr: Operand::AddrReg(rx),
            fc: data,
            size: Size::Word,
            value: Operand::ScratchHi16(RES),
        });
        return buf.finish();
    }
    // Byte / word: a single sized access per operand. predec Ay → read src; predec Ax → read dst; Alu;
    // refill; write @ Ax. `-(A7).b` steps by 2.
    buf.push(MicroOp::AdjustAddr {
        reg: ry,
        delta: -xarith_step(size, ry),
    });
    buf.push(MicroOp::Read {
        addr: Operand::AddrReg(ry),
        fc: data,
        size,
        dst: SRC_VAL,
    });
    buf.push(MicroOp::AdjustAddr {
        reg: rx,
        delta: -xarith_step(size, rx),
    });
    buf.push(MicroOp::Read {
        addr: Operand::AddrReg(rx),
        fc: data,
        size,
        dst: DST_VAL,
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Alu {
        op,
        size,
        a: Operand::Scratch(DST_VAL),
        b: Operand::Scratch(SRC_VAL),
        dst: Dest::Scratch(RES),
    });
    buf.push(MicroOp::Write {
        addr: Operand::AddrReg(rx),
        fc: data,
        size,
        value: Operand::Scratch(RES),
    });
    buf.finish()
}

/// `EXT.w` (`0x4880`) / `EXT.l` (`0x48C0`) (`0100 1000 1S 000 rrr`, mask `0xFFF8` — mode FIXED 000 = `Dn`, the
/// low 3 bits the register): sign-extend `Dn` whose result WIDTH follows the size. **EXT.w** (`size == Word`)
/// sign-extends the low BYTE to 16 bits and writes the LOW WORD ([`Dest::DataRegLow16`], the high word of `Dn`
/// preserved), N = bit15 / Z = (word == 0); **EXT.l** (`size == Long`) sign-extends the low WORD to 32 bits
/// and writes the FULL 32 ([`Dest::DataReg`]), N = bit31 / Z = (long == 0). Both: V = 0, C = 0, X PRESERVED
/// (the LOGIC flag shape of [`AluOp::Ext`]). The `a` operand is supplied at the **input** size — the low byte
/// ([`Operand::DataRegLow8`]) for `.w`, the low word ([`Operand::DataRegLow16`]) for `.l`; `b` is ignored
/// ([`Operand::Zero`]). `Dn`-only — NO memory access: a single `Prefetch` then the `Alu` (4 cyc, no idle, no
/// fault possible).
fn ext_recipe(opcode: u16, size: Size) -> MicroState {
    let reg = (opcode & 7) as u8;
    // The input is one size SMALLER than the result: EXT.w reads the low BYTE → low word; EXT.l the low WORD
    // → full 32. `a` follows the input size; `dst` (and the flag width) follow the result size.
    let (a, dst) = match size {
        Size::Word => (Operand::DataRegLow8(reg), Dest::DataRegLow16(reg)),
        Size::Long => (Operand::DataRegLow16(reg), Dest::DataReg(reg)),
        Size::Byte => unreachable!("EXT is .w/.l only"),
    };
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Alu {
        op: AluOp::Ext,
        size,
        a,
        b: Operand::Zero,
        dst,
    });
    buf.finish()
}

/// `SWAP Dn` (`0x4840`, `0100 1000 01 000 rrr`, mask `0xFFF8` — mode FIXED 000 = `Dn`): swap the two 16-bit
/// halves of `Dn` on the FULL 32 bits (`res = (Dn >> 16) | (Dn << 16)`; size always Long). LOGIC flags on the
/// 32-bit result — N = bit31, Z = (result == 0), V = 0, C = 0, X PRESERVED (the [`AluOp::Swap`] shape). `a` is
/// the full register ([`Operand::DataRegFull`]) and the result is written full-width ([`Dest::DataReg`]); `b`
/// is ignored ([`Operand::Zero`]). `Dn`-only — NO memory access: a single `Prefetch` then the `Alu` (4 cyc, no
/// idle).
fn swap_recipe(opcode: u16) -> MicroState {
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Alu {
        op: AluOp::Swap,
        size: Size::Long,
        a: Operand::DataRegFull(reg),
        b: Operand::Zero,
        dst: Dest::DataReg(reg),
    });
    buf.finish()
}

/// `NOP` (`0x4E71`, exact): no operation — advance `pc` by 2 and shift the prefetch queue, nothing else. The
/// whole register file (D/A/SP/SR) is byte-identical afterwards; the ONLY effect is the single queue refill.
/// The recipe is a lone [`MicroOp::Prefetch`] (length 4, one FC-6 program read at `pc+4`), the same trailing
/// refill every instruction ends with.
fn nop_recipe() -> MicroState {
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `EXG` (`1100 rrr 1 ppppp rrr`, `opcode & 0xF100 == 0xC100` with the opmode field `(opcode >> 3) & 0x1F` ∈
/// {`0x08` Dx,Dy / `0x09` Ax,Ay / `0x11` Dx,Ay}): exchange two whole 32-bit registers, affecting **NO flags**.
/// `rx = (opcode >> 9) & 7`, `ry = opcode & 7`. The recipe is `[Prefetch, Internal(2), ExgRegs]` — length **6**
/// for ALL three forms (one FC-6 refill then the 2-cycle internal idle; the [`MicroOp::ExgRegs`] swap is a
/// 0-cycle non-bus step). A7 legs (opmode `0x09`, or `0x11` with `ry == 7`, or `0x09` with `rx == 7`) exchange
/// the ACTIVE `addr_reg(7)` (ssp/usp per the S bit) via the A7-aware register helpers. The opmode guard is
/// LOAD-BEARING: ABCD (opmode `0x00`/`0x01`) and `AND Dn,<ea>` share the 0xCxxx space and are NOT EXG — only
/// the three EXG opmodes reach this arm (see the decode-dispatch placement, BEFORE any broad 0xCxxx arm).
fn exg_recipe(opcode: u16) -> MicroState {
    let opmode = ((opcode >> 3) & 0x1F) as u8;
    let rx = ((opcode >> 9) & 7) as u8;
    let ry = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Internal { cycles: 2 });
    buf.push(MicroOp::ExgRegs { opmode, rx, ry });
    buf.finish()
}

/// `MOVEfromUSP` (`0x4E68 | An`) / `MOVEtoUSP` (`0x4E60 | An`): the trivial register↔USP transfers, affecting
/// **NO flags** (SR unchanged). `to_usp` = `true` for `MOVEtoUSP` (`USP = An`), `false` for `MOVEfromUSP`
/// (`An = USP`); both move the FULL 32 bits. The recipe is `[MoveUsp, Prefetch]` — length **4** (the 0-cycle
/// non-bus [`MicroOp::MoveUsp`] register swap, then the single FC-6 queue refill at `pc+4` that books the whole
/// cost). `an == 7` routes through [`Registers::addr_reg`]/[`Registers::addr_reg_set`] (A7-aware, so the
/// supervisor A7 = `ssp`): `MOVEfromUSP A7` sets `ssp = usp`, `MOVEtoUSP A7` sets `usp = ssp`. Both are
/// privileged on the 68000, but every vendored case runs in supervisor mode, so the privilege-violation trap
/// is UNEXERCISED and NOT implemented (correctness-only, like the T-bit trace).
fn usp_recipe(to_usp: bool, an: u8) -> MicroState {
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::MoveUsp { to_usp, an });
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `TAS <ea>` (`0x4AC0 | ea`, `0100 1010 11 mmm rrr`, opcode & 0xFFC0 == 0x4AC0): the indivisible
/// test-and-set — read the byte, set the LOGIC flags from the READ byte (N = bit7(byte), Z = (byte == 0),
/// V = 0, C = 0, X PRESERVED), then write `byte | 0x80`. The KEY subtlety: the flag input (the read byte)
/// DIFFERS from the written value (`read | 0x80`) — unlike `NOT`, whose flags are on the result.
///
/// `Dn`-direct (mode 0) is the REGISTER form: a single `Prefetch` then the unary [`AluOp::Tas`] writing
/// `(Dn & 0xFF) | 0x80` to `Dn`'s low byte ([`Dest::DataRegLow8`], the upper 24 bits preserved), `b` ignored
/// ([`Operand::Zero`]) — 4 cyc, NO memory, no fault possible.
///
/// A **memory destination** is the ATOMIC indivisible RMW — ONE locked `'t'` bus cycle ([`MicroOp::TasRmw`]
/// via [`ea_tas`], 10 cyc), NOT the read-then-write `ea_dst` path (which would emit a separate `'r'`+`'w'`
/// and fail the per-cycle transaction gate). `ea_tas` emits the EA prep (the same `EaCalc`/`AdjustAddr`/
/// prefetch machinery as `ea_dst`) + the single `TasRmw` + the trailing refill. Memory cost = CLR + 2 cyc
/// everywhere: `(An)`/`(An)+` 14, `−(An)` 16, `d16(An)` 18, `d8(An,Xn)` 20, `abs.w` 18, `abs.l` 22. Byte-only
/// → NO odd-EA address-error faults.
fn tas_recipe(opcode: u16) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if mode == 0 {
        // Dn-direct — no memory: one refill, then the unary Tas Alu (4 cyc, the Alu a 0-cycle compute).
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Tas,
            size: Size::Byte,
            a: Operand::DataRegLow8(reg),
            b: Operand::Zero,
            dst: Dest::DataRegLow8(reg),
        });
    } else {
        // Memory destination — the ATOMIC indivisible RMW (ONE `'t'` transaction), NOT a read+write pair.
        // `ea_tas` emits the EA prep + ONE `MicroOp::TasRmw` (flags from the read byte) + the trailing refill.
        ea_tas(&mut buf, mode, reg);
    }
    buf.finish()
}

/// `EOR.{b,w,l} Dn,<ea>` (`1011 ddd 1SS mmm rrr`, opmode 4/5/6): bitwise `<ea> = <ea> ^ Dn` — set N = msb /
/// Z = (result == 0), clear V/C, PRESERVE X (`AluOp::Eor`). EOR exists ONLY in the `Dn,<ea>` direction; the
/// destination is either a **data register** (mode 000 = `Dn,Dn`) or **alterable memory** (modes 2..6,
/// abs.w/abs.l).
///
/// **Memory dest** reuses [`arith_dn_ea`] VERBATIM — the `ea_dst`/`ea_dst_long` read/refill/ALU/write RMW
/// skeleton ADD/AND/OR use (EOR Dn,<ea> = ADD Dn,<ea> byte-for-byte); the memory operand is the EA value `a`
/// and the source `Dn` is `b` (EOR is commutative, so the order is inert).
///
/// **`Dn`-direct (mode 000)** is the register-dest `EOR Dn,Dn`, which has **no memory access** (so it does NOT
/// route through the memory-only `ea_dst`): a single `Prefetch` then `Alu{Eor, size, a: Dn_dest, b: Dn_src,
/// dst: dn_dest(Dn_dest,size)}`, where `Dn_dest` is the EA register (bits 2-0) and `Dn_src` is bits 11-9.
/// `EOR.b`/`.w Dn,Dn` = 4 cyc (no idle); **`EOR.l Dn,Dn` = 8 cyc** — the long register form carries one
/// trailing `Internal(4)` idle (the register-register long idle, pinned to the vendored `b782` anchor), exactly
/// like `ADD.l`/`AND.l <ea>,Dn`'s `n4`.
fn eor_recipe(opcode: u16, size: Size) -> MicroState {
    let mode = (opcode >> 3) & 7;
    if mode == 0 {
        // Dn,Dn register dest — no memory: one refill, then the Eor into the EA register Dn (size-masked).
        // EOR.l Dn,Dn adds one trailing idle (n4 → 8 cyc); EOR.b/.w Dn,Dn are 4 cyc with no idle.
        let dst = (opcode & 7) as u8; // EA register (bits 2-0) — the destination Dn
        let src = ((opcode >> 9) & 7) as u8; // bits 11-9 — the source Dn
        let mut buf = RecipeBuf::new();
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Eor,
            size,
            a: dn_operand(dst, size),
            b: dn_operand(src, size),
            dst: dn_dest(dst, size),
        });
        if size == Size::Long {
            buf.push(MicroOp::Internal { cycles: 4 });
        }
        buf.finish()
    } else {
        // Alterable-memory dest — the RMW path, VERBATIM `arith_dn_ea` (= ADD Dn,<ea> byte-for-byte).
        arith_dn_ea(opcode, AluOp::Eor, size)
    }
}

/// The `(An)+` auto-increment step (bytes) for `CMPM`: word 2, long 4, byte 1 — except `(A7)+` byte steps by 2
/// to keep the stack pointer even (the in-scope A7 rule, mirroring `ea.rs`'s `step_bytes`).
#[inline]
fn cmpm_step(size: Size, reg: u8) -> i8 {
    match size {
        Size::Word => 2,
        Size::Long => 4,
        Size::Byte => {
            if reg == 7 {
                2
            } else {
                1
            }
        }
    }
}

/// `CMPM (Ay)+,(Ax)+` (`1011 xxx 1SS 001 yyy`, opmode 4/5/6 = b/w/l): compare memory — read the **source** at
/// `(Ay)+` FIRST, then the **destination** at `(Ax)+`, and set N/Z/V/C for `(Ax) − (Ay)` (X preserved, no
/// write — `AluOp::Cmp` + `Dest::None`). `xxx` (bits 11-9) = Ax (dest), `yyy` (bits 2-0) = Ay (src). Each
/// operand is a post-increment access: the pre-increment EA is captured into a scratch slot and the register is
/// bumped BEFORE the read (the 68000 commits the auto-increment as part of EA calculation), so an odd-address
/// read-fault still leaves the register incremented — pinned to the SST data, exactly as `ea_src`'s `(An)+`.
/// Capturing Ax's base AFTER bumping Ay also makes the `Ay == Ax` aliasing case correct (the two reads then hit
/// `A` and `A+step`). A `.l` operand is two word reads (hi @ base, lo @ base+2) assembled by `Combine32`. Bus:
/// `[r src, r dst, PF]` (12 cyc for b/w) or `[r src.hi, r src.lo, r dst.hi, r dst.lo, PF]` (20 cyc for `.l`) —
/// no idle, pinned to the vendored CMPM stream.
fn cmpm_recipe(opcode: u16, size: Size) -> MicroState {
    let ax = ((opcode >> 9) & 7) as u8; // destination (Ax)
    let ay = (opcode & 7) as u8; // source (Ay)
                                 // Scratch slots (all distinct, so every in-flight value is snapshot-visible): the two long read halves +
                                 // assembled value per operand, the captured post-increment bases, and the long low-word address.
    const SRC_HI: u8 = 0;
    const SRC_LO: u8 = 1;
    const SRC_VAL: u8 = 2;
    const DST_HI: u8 = 3;
    const DST_LO: u8 = 4;
    const DST_VAL: u8 = 5;
    const AY_BASE: u8 = 6;
    const AX_BASE: u8 = 7;
    const LO_ADDR: u8 = 8;
    let data = super::microop::Fc::Data;
    let mut buf = RecipeBuf::new();
    if size == Size::Long {
        // src @ (Ay)+ : capture Ay, bump +4, read hi @ base, read lo @ base+2, assemble.
        buf.push(MicroOp::EaCalc {
            base: Operand::AddrReg(ay),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: AY_BASE,
        });
        buf.push(MicroOp::AdjustAddr { reg: ay, delta: 4 });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(AY_BASE),
            fc: data,
            size: Size::Word,
            dst: SRC_HI,
        });
        buf.push(MicroOp::EaCalc {
            base: Operand::Scratch(AY_BASE),
            index: Operand::Zero,
            disp: Operand::WordStep,
            dst: LO_ADDR,
        });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(LO_ADDR),
            fc: data,
            size: Size::Word,
            dst: SRC_LO,
        });
        buf.push(MicroOp::Combine32 {
            hi: SRC_HI,
            lo: Operand::Scratch(SRC_LO),
            dst: SRC_VAL,
        });
        // dst @ (Ax)+ : capture Ax (AFTER Ay's bump, so Ay == Ax aliases correctly), bump +4, read the pair.
        buf.push(MicroOp::EaCalc {
            base: Operand::AddrReg(ax),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: AX_BASE,
        });
        buf.push(MicroOp::AdjustAddr { reg: ax, delta: 4 });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(AX_BASE),
            fc: data,
            size: Size::Word,
            dst: DST_HI,
        });
        buf.push(MicroOp::EaCalc {
            base: Operand::Scratch(AX_BASE),
            index: Operand::Zero,
            disp: Operand::WordStep,
            dst: LO_ADDR,
        });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(LO_ADDR),
            fc: data,
            size: Size::Word,
            dst: DST_LO,
        });
        buf.push(MicroOp::Combine32 {
            hi: DST_HI,
            lo: Operand::Scratch(DST_LO),
            dst: DST_VAL,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Cmp,
            size,
            a: Operand::Scratch(DST_VAL),
            b: Operand::Scratch(SRC_VAL),
            dst: Dest::None,
        });
    } else {
        // src @ (Ay)+ : capture Ay, bump, read.
        buf.push(MicroOp::EaCalc {
            base: Operand::AddrReg(ay),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: AY_BASE,
        });
        buf.push(MicroOp::AdjustAddr {
            reg: ay,
            delta: cmpm_step(size, ay),
        });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(AY_BASE),
            fc: data,
            size,
            dst: SRC_HI,
        });
        // dst @ (Ax)+ : capture Ax (after Ay's bump), bump, read.
        buf.push(MicroOp::EaCalc {
            base: Operand::AddrReg(ax),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: AX_BASE,
        });
        buf.push(MicroOp::AdjustAddr {
            reg: ax,
            delta: cmpm_step(size, ax),
        });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(AX_BASE),
            fc: data,
            size,
            dst: DST_HI,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op: AluOp::Cmp,
            size,
            a: Operand::Scratch(DST_HI),
            b: Operand::Scratch(SRC_HI),
            dst: Dest::None,
        });
    }
    buf.finish()
}

/// `CMPI #imm,<ea>` (`0000 1100 SS mmm rrr`, SS bits 7-6 = b/w/l): compare the data-alterable EA against an
/// immediate — the flag-only `<ea> − #imm` (the **EA value is the minuend**, `a`; the immediate is `b`),
/// setting N/Z/V/C exactly as `SUB` but PRESERVING X and writing **nothing** (`AluOp::Cmp` + `Dest::None`).
/// The EA is read and DISCARDED; CMPI is **not** a read-modify-write — there is no write-back.
///
/// The load-bearing ordering: the immediate's extension word(s) come BEFORE the EA's extension word(s) in the
/// prefetch stream. For **byte/word** (one immediate word) the immediate is parked into a scratch slot, then a
/// single refill consumes it from the queue and shifts the EA's first extension word into `prefetch[1]`, then
/// the proven [`ea_src`] source machinery reads the EA (the source-read bus stream — `[…, READ, PF]` — is
/// exactly what CMPI's discarded read uses, with NO trailing memory idle, matching the vendored stream). For
/// **long** (a two-word immediate) the immediate is assembled via the `Combine32` idiom (HI captured before
/// the refill shifts it out, then the LO word), and the EA is read long ([`cmpi_ea_read_long`]) with the
/// CMPI-specific prefetch counts and idles — pinned to the vendored CMP.l stream (Dn-dest = n2 trailing idle;
/// `−(An)`/`d8(An,Xn)` = an n2 leading idle; every other memory mode = no idle).
fn cmpi_recipe(opcode: u16, size: Size) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    // Scratch slots: 6 holds the captured immediate (byte/word) or its HI half (long); 7 holds the assembled
    // long immediate. Disjoint from `ea_src`'s slots (0 read, 2 EA, 3 abs-HI, 4/5 long lo) so every in-flight
    // value is snapshot-visible.
    const IMM_HI_SLOT: u8 = 6;
    const IMM_SLOT: u8 = 7;
    let mut buf = RecipeBuf::new();
    if size == Size::Long {
        let make_alu = |a| MicroOp::Alu {
            op: AluOp::Cmp,
            size,
            a,
            b: Operand::Scratch(IMM_SLOT),
            dst: Dest::None,
        };
        // Assemble the 32-bit immediate: HI = prefetch[1] captured BEFORE the refill shifts it out
        // (`(0 << 16) | prefetch[1]` while slot is still the fresh-recipe zero), a refill shifts the LO word
        // into prefetch[1], then `(HI << 16) | LO`. The refill here is the instruction's FIRST refill.
        buf.push(MicroOp::Combine32 {
            hi: 0,
            lo: Operand::ImmWord,
            dst: IMM_HI_SLOT,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Combine32 {
            hi: IMM_HI_SLOT,
            lo: Operand::ImmWord,
            dst: IMM_SLOT,
        });
        cmpi_ea_read_long(&mut buf, mode, reg, make_alu);
    } else {
        let make_alu = |a| MicroOp::Alu {
            op: AluOp::Cmp,
            size,
            a,
            b: Operand::Scratch(IMM_SLOT),
            dst: Dest::None,
        };
        // Park the immediate word (its low byte/word is the operand) BEFORE any refill shifts it out, then a
        // single refill consumes it and brings the EA's first extension word into prefetch[1], then the proven
        // source-read machinery for the EA (Dn-direct → no read; memory → `[…, READ, PF]`, no trailing idle).
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: IMM_SLOT,
        });
        buf.push(MicroOp::Prefetch);
        ea_src(&mut buf, mode, reg, size, make_alu);
    }
    buf.finish()
}

/// The long EA-read sub-sequence for `CMPI.l #imm,<ea>` — read the data-alterable EA as a long (two word reads
/// assembled by `Combine32`) and DISCARD it, feeding the flag-only `<ea> − #imm` via `make_alu`. Emitted
/// AFTER the recipe's immediate-capture block (which has already done the instruction's first refill). Every
/// prefetch count and idle is pinned to the vendored CMP.l (CMPI) stream:
/// - **Dn-direct** (mode 0): two trailing refills + the register long idle **n2** (NOT ADD.l's n4).
/// - **`(An)`** (mode 2): one refill, the long read pair, one trailing refill. No idle.
/// - **`(An)+`** (mode 3): capture An, post-increment +4, then like `(An)`. No idle.
/// - **`−(An)`** (mode 4): one refill, pre-decrement −4, the predecrement idle **n2**, the long read pair, one
///   trailing refill.
/// - **`d16(An)`/`abs.w`** (modes 5, 7/0): one refill (brings the disp word in), `EaCalc`, one refill, the long
///   read pair, one trailing refill. No idle.
/// - **`d8(An,Xn)`** (mode 6): one refill (brings the brief word in), `EaCalc`, the indexed idle **n2**, one
///   refill, the long read pair, one trailing refill.
/// - **`abs.l`** (mode 7/1): one refill (brings the HI word in), the two-word address assembly, one refill, the
///   long read pair, one trailing refill. No idle.
///
/// CMPI never reads PC-relative or `#imm` EAs (data-alterable only — those modes are absent from the data) or
/// `An`-direct (illegal); only the eight data-alterable modes above appear. A `.l` read pair is `[READ.hi @
/// EA, READ.lo @ EA+2, Combine32]` — the discarded operand value in scratch 0.
fn cmpi_ea_read_long(
    buf: &mut RecipeBuf,
    mode: u16,
    reg: u8,
    make_alu: impl FnOnce(Operand) -> MicroOp,
) {
    // Scratch slots mirroring the EA machinery's conventions (disjoint from the immediate slots 6/7): 0 holds
    // the read hi word / assembled operand, 2 the computed EA, 3 the abs.l HI word, 4 the long lo word, 5 the
    // long lo-word address.
    const EA_SLOT: u8 = 2;
    const ABS_HI_SLOT: u8 = 3;
    const LONG_LO_SLOT: u8 = 4;
    const LONG_LO_ADDR_SLOT: u8 = 5;
    let data = super::microop::Fc::Data;
    // The long read pair at a materialized base address: hi @ addr → scratch 0, lo @ addr+2 → LONG_LO_SLOT,
    // assembled by Combine32 back into scratch 0 (the discarded operand).
    let read_pair = |buf: &mut RecipeBuf, hi_addr: Operand| {
        buf.push(MicroOp::Read {
            addr: hi_addr,
            fc: data,
            size: Size::Word,
            dst: 0,
        });
        buf.push(MicroOp::EaCalc {
            base: hi_addr,
            index: Operand::Zero,
            disp: Operand::WordStep,
            dst: LONG_LO_ADDR_SLOT,
        });
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(LONG_LO_ADDR_SLOT),
            fc: data,
            size: Size::Word,
            dst: LONG_LO_SLOT,
        });
        buf.push(MicroOp::Combine32 {
            hi: 0,
            lo: Operand::Scratch(LONG_LO_SLOT),
            dst: 0,
        });
    };
    match (mode, reg) {
        // Dn-direct — no read: two trailing refills, the flag-ALU on the full register, the register long idle
        // (n2). Bus: [PF, PF] (plus the immediate block's earlier refill).
        (0, _) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::DataRegFull(reg)));
            buf.push(MicroOp::Internal { cycles: 2 });
        }
        // (An) — one refill, the long read pair at An, one trailing refill. No idle.
        (2, _) => {
            buf.push(MicroOp::Prefetch);
            read_pair(buf, Operand::AddrReg(reg));
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
        }
        // (An)+ — capture the pre-increment EA, post-increment An by 4 (committed before the read, so an
        // odd-address fault still bumps), then like (An). No idle.
        (3, _) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::Zero,
                disp: Operand::Zero,
                dst: EA_SLOT,
            });
            buf.push(MicroOp::AdjustAddr { reg, delta: 4 });
            read_pair(buf, Operand::Scratch(EA_SLOT));
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
        }
        // -(An) — one refill, pre-decrement An by 4, the predecrement idle (n2), the long read pair at An-4,
        // one trailing refill.
        (4, _) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::AdjustAddr { reg, delta: -4 });
            buf.push(MicroOp::Internal { cycles: 2 });
            read_pair(buf, Operand::AddrReg(reg));
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
        }
        // d16(An) / abs.w — one refill (brings the disp word into prefetch[1]), EaCalc the EA, one refill, the
        // long read pair, one trailing refill. No idle.
        (5, _) | (7, 0) => {
            let base = if mode == 5 {
                Operand::AddrReg(reg)
            } else {
                Operand::Zero // abs.w
            };
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: EA_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            read_pair(buf, Operand::Scratch(EA_SLOT));
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
        }
        // d8(An,Xn) — one refill (brings the brief ext word in), EaCalc (An + index + disp8), the indexed idle
        // (n2), one refill, the long read pair, one trailing refill.
        (6, _) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: EA_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: 2 });
            buf.push(MicroOp::Prefetch);
            read_pair(buf, Operand::Scratch(EA_SLOT));
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
        }
        // abs.l — one refill (brings the HI word in), the two-word address assembly (HI captured, refill, LO →
        // EA), one refill, the long read pair, one trailing refill. No idle.
        (7, 1) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::ExtWordHi,
                dst: ABS_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::Scratch(ABS_HI_SLOT),
                index: Operand::Zero,
                disp: Operand::ExtWordRaw,
                dst: EA_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            read_pair(buf, Operand::Scratch(EA_SLOT));
            buf.push(MicroOp::Prefetch);
            buf.push(make_alu(Operand::Scratch(0)));
        }
        _ => todo!("cmpi_ea_read_long mode {mode}/{reg} not yet covered"),
    }
}

/// `ADDI`/`SUBI`/`ANDI`/`ORI`/`EORI #imm,<ea>` (`0000 hhhh ss mmm rrr`) — the shared immediate-to-EA
/// read-modify-write builder for the whole `*I` family. Composes the CMPI immediate-capture idiom
/// ([`cmpi_recipe`]'s capture block, VERBATIM) with the `ea_dst`/`ea_dst_long` RMW writeback — the memory (or
/// `Dn`) value is the minuend `a`, the captured immediate the addend/subtrahend `b`. `op` selects the ALU
/// operation (from [`imm_class`]); flags follow it (Add/Sub = full add/sub with X = C; And/Or/Eor = logic N/Z,
/// V = C = 0, X preserved — all proven, reused, NOT reinvented).
///
/// Immediate encoding (identical to `cmpi_recipe`): **byte/word** = ONE extension word captured via
/// `EaCalc{base = ImmWord}` into `IMM_SLOT` BEFORE the refill shifts it out; **long** = TWO extension words
/// assembled by the `Combine32` HI/LO idiom (HI captured before the refill, LO after). Scratch slots 6 (the
/// byte/word immediate or the long HI half) and 7 (the assembled immediate) mirror `cmpi_recipe`'s convention —
/// DISJOINT from `ea_dst`'s slots (0 read value, 1 result, 2 EA, 3 abs-HI, 4/5 long lo) so every in-flight
/// value stays snapshot-visible.
///
/// Two destination shapes by `mode = (opcode>>3)&7`:
/// - **`Dn` (mode 0):** the register form — no memory access. After the capture: `[Alu{op, size, a =
///   dn_operand(reg, size), b = Scratch(IMM_SLOT), dst = dn_dest(reg, size)}, Prefetch]`. Byte/word = 8 cyc
///   (the capture's one refill + this trailing refill), long = 16 cyc (the capture's two-word fetch + the
///   trailing refills). The `Dn` value is the minuend (correct for the non-commutative SUBI later).
/// - **memory (2-7):** the alterable-memory RMW via `ea_dst`/`ea_dst_long`, `b = Scratch(IMM_SLOT)`, `dst =
///   Scratch(1)` — identical to [`arith_dn_ea`] but the addend is the captured immediate. Odd word/long EAs
///   fault on the RMW read via the E3/E4 address-error abort (in scope).
fn imm_rmw_recipe(opcode: u16, op: AluOp, size: Size) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    // Scratch slots: 6 holds the captured immediate (byte/word) or its HI half (long); 7 holds the assembled
    // long immediate. Disjoint from `ea_dst`'s slots (0 read, 1 result, 2 EA, 3 abs-HI, 4/5 long lo) so every
    // in-flight value is snapshot-visible.
    const IMM_HI_SLOT: u8 = 6;
    const IMM_SLOT: u8 = 7;
    let mut buf = RecipeBuf::new();
    // Capture the immediate FIRST (before the EA's extension words / the refill), exactly as `cmpi_recipe`.
    if size == Size::Long {
        // Assemble the 32-bit immediate: HI = prefetch[1] captured BEFORE the refill shifts it out
        // (`(0 << 16) | prefetch[1]` while the slot is still the fresh-recipe zero), a refill shifts the LO
        // word into prefetch[1], then `(HI << 16) | LO`. This refill is the instruction's FIRST refill.
        buf.push(MicroOp::Combine32 {
            hi: 0,
            lo: Operand::ImmWord,
            dst: IMM_HI_SLOT,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Combine32 {
            hi: IMM_HI_SLOT,
            lo: Operand::ImmWord,
            dst: IMM_SLOT,
        });
    } else {
        // Park the immediate word (its low byte/word is the operand) BEFORE any refill shifts it out, then a
        // single refill consumes it and brings the EA's first extension word into prefetch[1].
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: IMM_SLOT,
        });
        buf.push(MicroOp::Prefetch);
    }
    // Then the RMW writeback: the memory/register value is the minuend `a`, the captured immediate the addend.
    if mode == 0 {
        // Dn (mode 0) register form — no memory access. The register value is the minuend (correct for SUBI's
        // non-commutativity). Byte/word = 8 cyc: the capture's one refill (r@pc+4) + this arm's ONE trailing
        // refill (r@pc+6), no idle. Long = 16 cyc: the capture's two-word fetch (HI in prefetch[1] + LO refill
        // r@pc+4) + TWO trailing refills (r@pc+6, r@pc+8) + the register long idle n4 — pinned to the vendored
        // `[r@pc+4, r@pc+6, r@pc+8, n4]` ADDI.l Dn stream.
        buf.push(MicroOp::Alu {
            op,
            size,
            a: dn_operand(reg, size),
            b: Operand::Scratch(IMM_SLOT),
            dst: dn_dest(reg, size),
        });
        buf.push(MicroOp::Prefetch);
        if size == Size::Long {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Internal { cycles: 4 });
        }
    } else {
        // Memory (2-7): the alterable-memory RMW skeleton (identical to `arith_dn_ea`, addend = the immediate).
        //
        // For **byte/word** the capture's single refill already shifted the EA's first extension word into
        // prefetch[1], so `ea_dst`'s per-mode EaCalc/read stream aligns directly (bus = `[r@pc+4, <ea_dst>]`,
        // e.g. `(An).b` = `[r@pc+4, READ.b, r@pc+6, WRITE.b]` = 16 cyc).
        //
        // For **long** the immediate is TWO words, so after the `Combine32` capture prefetch[1] still holds the
        // LO immediate word — the EA's extension words have NOT been shifted in yet. ONE extra leading refill
        // brings the EA's first ext word into prefetch[1] AND supplies the extra bus cycle, after which
        // `ea_dst_long`'s per-mode stream aligns exactly (this prepended-refill + `ea_dst_long` composition is
        // byte-for-byte the CMPI long read structure `cmpi_ea_read_long` uses, only writing back instead of
        // discarding — e.g. `(An).l` = `[r@pc+4, r@pc+6, READ.hi, READ.lo, r@pc+8, WRITE.lo, WRITE.hi]` = 28
        // cyc). Odd word/long EAs fault on the RMW read via the E3/E4 address-error abort (in scope).
        if size == Size::Long {
            buf.push(MicroOp::Prefetch);
        }
        ea_dst(&mut buf, mode, reg, size, |a| MicroOp::Alu {
            op,
            size,
            a,
            b: Operand::Scratch(IMM_SLOT),
            dst: Dest::Scratch(1),
        });
    }
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

/// Scratch slot holding a `DBcc`'s computed 32-bit branch target (the `SetPc` source). Slot 0 — the same slot
/// a `Bcc`'s `TargetCalc` deposits its target into (DBcc reuses the Bcc word-form branch tail).
const DBCC_TARGET_SLOT: u8 = 0;

/// `DBcc Dn,<label>` (`0101 cccc 11001 rrr`, opcode & 0xF0F8 == 0x50C8): the 68000 decrement-and-branch loop
/// primitive. `cc` (bits 11-8) is a *termination* condition: if it is **true** the loop ends (fall through, NO
/// decrement); if it is **false** the counter `Dn.w` is decremented and, while it has not run out, the branch
/// is taken. The 16-bit signed displacement is always the extension word `prefetch[1]` (DBcc has no byte form —
/// the opcode's low byte is `0xC8 | reg`, never 0). The target is `pc + 2 + sign_extend16(disp)` (relative to
/// the extension-word address `pc + 2`, the `PcOfExt` base), UNMASKED.
///
/// Both the condition and the counter are resolved at **decode time** (against the live CCR / `Dn` low word),
/// so the interpreter stays a flat linear recipe (the variable cycle count emerges from the different-length
/// recipe per path). The three paths, pinned to the vendored `DBcc` SST stream:
///
/// - **cond true** → `[Internal(4), Prefetch, Prefetch]`: fall through, advancing `pc` by TWO words (+4,
///   skipping the displacement word), NO decrement. **12 cyc** (`5dcd`). Identical shape to a `Bcc` word
///   not-taken.
/// - **cond false, counter NOT expired** (`Dn.w != 0`) → `[DecrementDnWord(reg), TargetCalc(PcOfExt, ·,
///   DispWord), Internal(2), SetPc(target), Prefetch, Prefetch]`: decrement `Dn.w`, then take the branch (the
///   universal `SetPc` + two-`Prefetch` reload). **10 cyc** (`59c8`). Even target in scope; an odd taken
///   target is an address error (52 cyc) → xfail.
/// - **cond false, counter EXPIRED** (`Dn.w == 0`, so the decrement makes it `0xFFFF` = −1) → `[DecrementDnWord
///   (reg), Internal(6), Prefetch, Prefetch]`: decrement, then fall through (+4), **14 cyc**. Implemented for
///   correctness but **ABSENT from the SST data** — a random 32-bit `Dn` has its low word `== 0` only ≈1/65536
///   of the time, and the vendored `DBcc` file has only lengths 10/12/52 (no expired bucket), so this path is
///   not gate-exercised (the runner documents the same caveat).
fn dbcc_recipe(opcode: u16, regs: &Registers) -> MicroState {
    let cc = ((opcode >> 8) & 0xF) as u8;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if condition_true(cc, regs.sr) {
        // cond true → the loop terminates: fall through two words, NO decrement.
        buf.push(MicroOp::Internal { cycles: 4 });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
    } else {
        // cond false → decrement the counter. The expiry check reads the ORIGINAL Dn.w (the 68000 terminates
        // when the decrement yields −1, i.e. when the counter WAS 0): resolved here at decode time.
        let expired = regs.d[reg as usize] & 0xFFFF == 0;
        buf.push(MicroOp::DecrementDnWord { reg });
        if expired {
            // Counter ran out → fall through (+4). Correctness-only — ABSENT from the SST data (see the doc
            // comment); the 14-cyc total (DecrementDnWord 0 + Internal(6) + two Prefetch) follows the 68000.
            buf.push(MicroOp::Internal { cycles: 6 });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
        } else {
            // Counter still live → take the branch. Compute the UNMASKED target FIRST (capturing prefetch[1]
            // and the original pc via PcOfExt, before any refill), then the universal SetPc + reload tail.
            buf.push(MicroOp::TargetCalc {
                base: Operand::PcOfExt,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: DBCC_TARGET_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: 2 });
            buf.push(MicroOp::SetPc {
                value: Operand::Scratch(DBCC_TARGET_SLOT),
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
        }
    }
    buf.finish()
}

/// Scratch slot holding the HIGH word of an `RTR` popped return address (read from `SP + 2`). Slot 0.
const RTR_HI_SLOT: u8 = 0;

/// Scratch slot holding the assembled 32-bit `RTR` return address (the `SetPc` source). Slot 1.
const RTR_TARGET_SLOT: u8 = 1;

/// Scratch slot holding a materialized `RTR` pop address (`SP + 2` then, reused, `SP + 4`) — masked to the
/// 24-bit bus (a real even bus address). Slot 2. Transient: each address is consumed by its `Read` before the
/// next `EaCalc` overwrites it.
const RTR_ADDR_SLOT: u8 = 2;

/// Scratch slot holding the popped CCR word (read from `SP`), fed to [`MicroOp::LoadCcr`]. Slot 3.
const RTR_CCR_SLOT: u8 = 3;

/// Scratch slot holding the LOW word of an `RTR` popped return address (read from `SP + 4`). Slot 4.
const RTR_LO_SLOT: u8 = 4;

/// `RTR` (`0x4E77`): return and restore condition codes — POP the saved CCR word and the 32-bit return
/// address, load the low 5 CCR bits into the SR, then reload the prefetch queue at the popped target. Like
/// [`rts_recipe`] but with a leading CCR pop and a `+6` stack adjust. The vendored `RTR` SST stream reads the
/// three popped words in the order `pc_hi @ SP+2`, `ccr @ SP`, `pc_lo @ SP+4` (reproduced exactly), then the
/// universal taken-branch tail ([`MicroOp::SetPc`] + two `Prefetch`s read `target` / `target+2`, FC=6).
///
/// Recipe (pinned to the vendored `RTR` SST stream — `4e77`, **20 cyc**): `[EaCalc(SP+2), Read(pc_hi @ SP+2),
/// Read(ccr @ SP), EaCalc(SP+4), Read(pc_lo @ SP+4), LoadCcr(ccr), AdjustAddr(SP, +6), Combine32(hi,lo →
/// target), SetPc(target), Prefetch, Prefetch]`. The popped return address is the FULL 32-bit pc (UNMASKED —
/// `Combine32` does no mask); only the bus reload address masks. `LoadCcr` keeps only the low 5 CCR bits
/// (X/N/Z/V/C); the SR system byte is preserved. The bus stream is `[r@SP+2, r@SP, r@SP+4, r@target,
/// r@target+2]` (5 word reads = 20 cycles, no idle).
fn rtr_recipe() -> MicroState {
    let mut buf = RecipeBuf::new();
    // HI word of the return address @ SP + 2 (read FIRST, per the data's reordered stream).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: RTR_ADDR_SLOT,
    });
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(RTR_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTR_HI_SLOT,
    });
    // The saved CCR word @ SP (read SECOND).
    buf.push(MicroOp::Read {
        addr: Operand::AddrReg(7),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTR_CCR_SLOT,
    });
    // LO word of the return address @ SP + 4 (read THIRD). SP + 4 = base + WordStep + WordStep.
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::WordStep,
        disp: Operand::WordStep,
        dst: RTR_ADDR_SLOT,
    });
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(RTR_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTR_LO_SLOT,
    });
    // Restore the condition codes (low 5 bits), then pop (SP += 6) and reload at the UNMASKED target.
    buf.push(MicroOp::LoadCcr {
        value: Operand::Scratch(RTR_CCR_SLOT),
    });
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: 6 });
    buf.push(MicroOp::Combine32 {
        hi: RTR_HI_SLOT,
        lo: Operand::Scratch(RTR_LO_SLOT),
        dst: RTR_TARGET_SLOT,
    });
    buf.push(MicroOp::SetPc {
        value: Operand::Scratch(RTR_TARGET_SLOT),
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot holding the HIGH word of an `RTE` popped return PC (read from `SP + 2`). Slot 0.
const RTE_HI_SLOT: u8 = 0;

/// Scratch slot holding the assembled 32-bit `RTE` return PC (the `SetPc` source). Slot 1.
const RTE_TARGET_SLOT: u8 = 1;

/// Scratch slot holding a materialized `RTE` pop address (`SP + 2` then, reused, `SP + 4`) — masked to the
/// 24-bit bus (a real even bus address). Slot 2. Transient: each address is consumed by its `Read` before the
/// next `EaCalc` overwrites it.
const RTE_ADDR_SLOT: u8 = 2;

/// Scratch slot holding the popped SR word (read from `SP`), fed to [`MicroOp::LoadSr`]. Slot 3.
const RTE_SR_SLOT: u8 = 3;

/// Scratch slot holding the LOW word of an `RTE` popped return PC (read from `SP + 4`). Slot 4.
const RTE_LO_SLOT: u8 = 4;

/// `RTE` (`0x4E73`): return from exception — the inverse of the standard frame push ([`push_standard_frame`]).
/// POP the 6-byte frame (the saved SR + the 32-bit return PC) off the supervisor stack, restore the FULL SR
/// (masked to the implemented bits `0xA71F`, which may switch S supervisor→user and T), increment SP by 6
/// **while still supervisor**, then reload the prefetch queue at the popped PC — the reload's function code
/// follows the RESTORED mode (FC2 user-program if S cleared, FC6 supervisor-program otherwise). The vendored
/// `RTE` stream reads the three popped words in the order `PC-hi @ SP+2`, `SR @ SP`, `PC-lo @ SP+4` (FC=5
/// supervisor-data, reproduced exactly) — the same word layout as [`rtr_recipe`]'s CCR+PC frame, only the `SP`
/// word is the full SR not just the CCR (and the restore is [`MicroOp::LoadSr`], not [`MicroOp::LoadCcr`]).
///
/// Recipe (pinned to the vendored `RTE` SST stream — `4e73 [RTE] 1` → user (FC2 reload) / `4e73 [RTE] 6` →
/// supervisor (FC6 reload), both **20 cyc**): `[EaCalc(SP+2), Read(PC-hi @ SP+2), Read(SR @ SP), EaCalc(SP+4),
/// Read(PC-lo @ SP+4), Combine32(hi,lo → target), AdjustAddr(SP, +6), LoadSr(SR), SetPc(target), Prefetch,
/// Prefetch]`. The `AdjustAddr(+6)` runs BEFORE `LoadSr` so the pop hits the supervisor stack (`ssp`) while S
/// is still set; `LoadSr` then restores the mode, and the two `Prefetch`s reload under it (no idle between —
/// the clean reload is 5 back-to-back word reads). The popped return address is the FULL 32-bit pc (UNMASKED —
/// `Combine32` does no mask); only the bus reload address masks. The bus stream is `[r@SP+2, r@SP, r@SP+4,
/// r@target, r@target+2]` (5 word reads = 20 cycles). Only the even-popped-PC path is decoded into scope; an
/// odd popped PC is an execution-time address error (it flips into scope at E4).
fn rte_recipe() -> MicroState {
    let mut buf = RecipeBuf::new();
    // HI word of the return PC @ SP + 2 (read FIRST, per the data's stream).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: RTE_ADDR_SLOT,
    });
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(RTE_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTE_HI_SLOT,
    });
    // The saved SR word @ SP (read SECOND).
    buf.push(MicroOp::Read {
        addr: Operand::AddrReg(7),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTE_SR_SLOT,
    });
    // LO word of the return PC @ SP + 4 (read THIRD). SP + 4 = base + WordStep + WordStep.
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::WordStep,
        disp: Operand::WordStep,
        dst: RTE_ADDR_SLOT,
    });
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(RTE_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: RTE_LO_SLOT,
    });
    // Assemble the UNMASKED 32-bit return PC (the full pc keeps its high bits; only the bus reload masks).
    buf.push(MicroOp::Combine32 {
        hi: RTE_HI_SLOT,
        lo: Operand::Scratch(RTE_LO_SLOT),
        dst: RTE_TARGET_SLOT,
    });
    // Pop the frame (SP += 6) BEFORE restoring the SR, so the +6 hits the supervisor stack while S is set.
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: 6 });
    // Restore the full SR (masked 0xA71F) — may switch S supervisor→user and T; the reload below follows it.
    buf.push(MicroOp::LoadSr {
        value: Operand::Scratch(RTE_SR_SLOT),
    });
    // SetPc primes the queue reload at the popped target; the two Prefetch ops reload under the RESTORED mode.
    buf.push(MicroOp::SetPc {
        value: Operand::Scratch(RTE_TARGET_SLOT),
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// The privileged instructions on the **MC68000** (M68000UM §6.3.7): the immediate-to-SR logic ops,
/// `MOVE to SR`, `MOVE USP` (both directions), `RESET`, `STOP`, `RTE`. **`MOVE from SR` is deliberately
/// absent** — it is unprivileged on the 68000 (privileged only from the 68010). Attempting any of these
/// in user mode is a privilege violation (vector 8); the gate in [`decode_dispatch`] consults this.
fn is_privileged_opcode(opcode: u16) -> bool {
    match opcode {
        0x027C | 0x007C | 0x0A7C => true, // ANDI/ORI/EORI #imm,SR
        0x4E70 => true,                   // RESET
        0x4E72 => true,                   // STOP #imm
        0x4E73 => true,                   // RTE
        _ => {
            opcode & 0xFFC0 == 0x46C0     // MOVE <ea>,SR
                || opcode & 0xFFF0 == 0x4E60 // MOVE USP (0x4E60..=0x4E6F, both to/from)
        }
    }
}

/// Scratch slot holding a decode-time exception frame's saved PC (`pc + 0` — the address of the first word
/// of the offending instruction, M68000UM §6.2.5/§6.3.7). Slot 0, the standard-frame saved-PC convention.
const EXC_SAVED_PC_SLOT: u8 = 0;

/// Scratch slot holding a decode-time exception frame's saved SR (captured by [`MicroOp::EnterException`]
/// before S is set / T cleared). Slot 1 — distinct from the saved-PC slot (the CHK/TRAP convention).
const EXC_SAVE_SR_SLOT: u8 = 1;

/// Build a **decode-time group-1 exception frame** for `vector`. Shared by the exceptions whose stacked PC
/// is the OFFENDING instruction (`pc + 0`, M68000UM §6.2.5): **privilege violation** (v8, §6.3.7),
/// **ILLEGAL** (v4, §6.3.6), **line-A** (v10) and **line-F** (v11) unimplemented-instruction traps. All are
/// byte-identical in shape and timing to [`trap_recipe`] (Yacht L1550/1551: `34(4/3)`, stream
/// `nn ns ns nS nV nv np n np`), differing ONLY in the vector. None are vendored in SST (audit finding 3);
/// pinned to the reference and driven by hand-authored vectors. (Trace, whose stacked PC is the NEXT
/// instruction and whose write order differs, is a separate builder in A3.)
fn decode_time_exception_recipe(vector: u32) -> MicroState {
    let mut buf = RecipeBuf::new();
    // Saved PC = pc + 0 (the offending instruction's first word), UNMASKED, before any prefetch.
    buf.push(MicroOp::TargetCalc {
        base: Operand::PcPlus(0),
        index: Operand::Zero,
        disp: Operand::Zero,
        dst: EXC_SAVED_PC_SLOT,
    });
    // Capture the live SR + enter supervisor (set S, clear T).
    buf.push(MicroOp::EnterException {
        save_sr: EXC_SAVE_SR_SLOT,
    });
    // The leading nn idle (= Internal{4}) — the TRAP-shape entry refills no prefetch before the push.
    buf.push(MicroOp::Internal { cycles: 4 });
    push_standard_frame(&mut buf, EXC_SAVED_PC_SLOT, EXC_SAVE_SR_SLOT);
    vector_fetch_and_reload(&mut buf, vector * 4);
    buf.finish()
}

/// Scratch slot holding `TRAP`'s saved return PC (`pc + 2`), deposited by the leading [`MicroOp::TargetCalc`]
/// and consumed by the standard frame's `PCL`/`PCH` writes. Slot 0.
const TRAP_SAVED_PC_SLOT: u8 = 0;

/// Scratch slot holding the SR captured at entry by [`MicroOp::EnterException`], stacked by the standard
/// frame's `SR` write. Slot 1 — distinct from the saved-PC slot so both survive until the push.
const TRAP_SAVE_SR_SLOT: u8 = 1;

/// `TRAP #n` (`0100 1110 0100 nnnn`, 0x4E40 | n): the cleanest standard 6-byte exception entry — an
/// UNCONDITIONAL trap to vector `32 + n` (address `(32 + n) * 4`). The recipe is the canonical Shape-A
/// planned entry: capture the saved PC + SR, enter supervisor, push the standard frame, fetch the vector,
/// reload the queue at the handler. Pinned to the vendored `TRAP` SST stream (`4e40`/`4e41`/`4e47`/`4e4f`,
/// all **34 cyc**):
///
/// - **Saved PC = `pc + 2`**, captured by a leading [`MicroOp::TargetCalc`] (UNMASKED) BEFORE any prefetch.
/// - **NO leading prefetch** — TRAP's first *bus* event is the `PCL` write; the only leading idle is the
///   `n4` ([`MicroOp::Internal`]) before the push.
/// - The standard frame writes in the on-bus order `PCL @ B+4`, `SR @ B+0`, `PCH @ B+2` (the 68000 microcode
///   order; see [`push_standard_frame`]).
/// - The vector reads are FC=5 (supervisor-data); the handler reload is two FC=6 prefetches with an `n2`
///   idle between (see [`vector_fetch_and_reload`]).
///
/// All vendored cases start in supervisor mode (S=1, T=0), so the S/T transform of [`MicroOp::EnterException`]
/// is structurally exercised but a no-op on the data — the caveat the runner documents.
fn trap_recipe(opcode: u16) -> MicroState {
    let n = (opcode & 0xF) as u32;
    let vector_addr = (32 + n) * 4;
    let mut buf = RecipeBuf::new();
    // The saved return PC (pc + 2), UNMASKED, captured before any prefetch advances pc.
    buf.push(MicroOp::TargetCalc {
        base: Operand::PcPlus(2),
        index: Operand::Zero,
        disp: Operand::Zero,
        dst: TRAP_SAVED_PC_SLOT,
    });
    // Capture the live SR + enter supervisor (set S, clear T).
    buf.push(MicroOp::EnterException {
        save_sr: TRAP_SAVE_SR_SLOT,
    });
    // The leading n4 idle — TRAP refills NO prefetch before the push (its first bus event is the PCL write).
    buf.push(MicroOp::Internal { cycles: 4 });
    push_standard_frame(&mut buf, TRAP_SAVED_PC_SLOT, TRAP_SAVE_SR_SLOT);
    vector_fetch_and_reload(&mut buf, vector_addr);
    buf.finish()
}

/// Scratch slot holding `TRAPV`'s saved return PC (`pc + 2`), captured by the leading [`MicroOp::TargetCalc`]
/// BEFORE the leading prefetch advances `pc`; consumed by the standard frame's `PCL`/`PCH` writes. Slot 0 —
/// the same slot map as `TRAP` (the shared frame builders use slots 2..=7).
const TRAPV_SAVED_PC_SLOT: u8 = 0;

/// Scratch slot holding the SR captured at entry by [`MicroOp::EnterException`], stacked by the standard
/// frame's `SR` write. Slot 1 — distinct from the saved-PC slot so both survive until the push.
const TRAPV_SAVE_SR_SLOT: u8 = 1;

/// `TRAPV` (`0x4E76`): trap on overflow — a CONDITIONAL trap resolved at DECODE time on the V flag (a direct
/// `sr & CCR_V` test, the Bcc decode-time-resolution pattern). Pinned to the vendored `TRAPV` SST stream
/// (`4e76 [TRAPV] 1` V=0, len 4; `4e76 [TRAPV] 3` V=1, len 34):
///
/// - **V clear → NO trap:** a single [`MicroOp::Prefetch`] refills the queue (FC=6 @ pc+4) and advances pc by
///   2 — the ordinary fall-through (len 4), nothing else changes.
/// - **V set → trap to vector 7** (address `7*4 = 28`): the standard 6-byte exception entry, DISTINGUISHED
///   FROM TRAP by a **LEADING prefetch** — TRAPV's first bus event is an FC=6 queue refill @ pc+4 (the
///   `n4` idle that TRAP runs instead). The saved PC = `pc + 2` is captured by a leading
///   [`MicroOp::TargetCalc`] BEFORE that prefetch advances pc; the rest is the shared frame push + vector
///   fetch + handler reload ([`push_standard_frame`] / [`vector_fetch_and_reload`]).
///
/// All vendored cases start in supervisor mode (S=1, T=0), so the S/T transform of [`MicroOp::EnterException`]
/// is structurally exercised but a no-op on the data — the caveat the runner documents.
fn trapv_recipe(regs: &Registers) -> MicroState {
    let mut buf = RecipeBuf::new();
    if regs.sr & CCR_V == 0 {
        // V clear → no trap: just refill the queue (FC=6 @ pc+4) and advance pc by 2. (len 4)
        buf.push(MicroOp::Prefetch);
        return buf.finish();
    }
    // V set → trap to vector 7. Saved PC = pc + 2, captured BEFORE the leading prefetch advances pc.
    buf.push(MicroOp::TargetCalc {
        base: Operand::PcPlus(2),
        index: Operand::Zero,
        disp: Operand::Zero,
        dst: TRAPV_SAVED_PC_SLOT,
    });
    // Capture the live SR + enter supervisor (set S, clear T).
    buf.push(MicroOp::EnterException {
        save_sr: TRAPV_SAVE_SR_SLOT,
    });
    // The LEADING prefetch (FC=6 @ pc+4) — TRAPV's first bus event, distinguishing it from TRAP's `n4` idle.
    buf.push(MicroOp::Prefetch);
    push_standard_frame(&mut buf, TRAPV_SAVED_PC_SLOT, TRAPV_SAVE_SR_SLOT);
    vector_fetch_and_reload(&mut buf, 7 * 4);
    buf.finish()
}

/// Scratch slot holding a MULU `#imm` source word, captured from `prefetch[1]` BEFORE the two refills shift it
/// out, so the count-dependent idle `Alu` can run LAST (after both prefetches — the vendored `#imm` stream is
/// `[Prefetch, Prefetch, idle]`, the idle trailing). Slot 0 — the same slot a memory-source `ea_src` deposits
/// its read into; the `Alu` reads it as `b` before any other op touches it.
const MUL_IMM_SRC_SLOT: u8 = 0;

/// `MUL <ea>,Dn` — the shared recipe for BOTH `MULU` (`opcode & 0xF1C0 == 0xC0C0`, opmode 3, [`AluOp::Mulu`])
/// and `MULS` (`opcode & 0xF1C0 == 0xC1C0`, opmode 7, [`AluOp::Muls`]) in the 0xC space (both DISJOINT from
/// AND's opmode 0/1/2/4/5/6). The two ops share IDENTICAL bus/recipe shape and flag shape (N=bit31 / Z=zero /
/// V=0 / C=0 / X-preserved on the FULL 32-bit product); they differ ONLY in the `op` passed here (which selects
/// unsigned vs signed value AND the cycle-count formula in the exec arm). The 16×16→32 multiply writes the full
/// product to `Dn` (bits 11-9); the source EA is the usual `mode/reg` in bits 5-0 (all 11 legal source modes —
/// An-direct is illegal for MUL and never appears). Reuses `ea_src(Size::Word)` VERBATIM — the same machinery
/// CHK uses (`chk_recipe`): it emits the operand read → prefetch refill → the closure op in the order the MUL
/// stream needs, and covers every source mode incl PC-relative + the odd-EA address error (an odd word EA faults
/// on the `ea_src` read, like CHK — NO parity filter). The `Alu{Mulu/Muls}` exec arm computes the product, sets
/// the flags above, and RETURNS its data-dependent cycle cost — `34 + 2*popcount(src16)` for MULU, `34 +
/// 2*booth(src16)` for MULS (the Booth TRANSITION count of `src<<1`, NOT popcount) — the count coming from the
/// resolved multiplier at exec-time (a memory source is not known at decode). The full length is `38 + 2*count +
/// ea_cost` (the MULS count, hence its length, DIFFERS from MULU even for the same source).
///
/// The ONE deviation from a literal `ea_src` reuse is the `#imm` mode (`7/4`): `ea_src`'s `#imm` path places the
/// `Alu` FIRST (it must sample `prefetch[1]` before the two refills shift the immediate out), which would put
/// the count-dependent idle BEFORE the prefetches — but the vendored `#imm` stream is `[Prefetch, Prefetch,
/// idle]` (idle LAST). So, exactly like `chk_recipe`'s `#imm` arm, the immediate is captured into a scratch slot
/// via `EaCalc{base: ImmWord}` BEFORE the two refills, both refills run, then the `Alu{op, b: Scratch}` runs
/// last (the bus stream / cycles are identical — only the value is captured first).
fn mul_recipe(opcode: u16, op: AluOp) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if (mode, reg) == (7, 4) {
        // #imm: capture the immediate (prefetch[1], zero-extended) into scratch BEFORE the two refills shift it
        // out, run both refills, then the Mulu Alu last (so the count-dependent idle trails the prefetches).
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: MUL_IMM_SRC_SLOT,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op,
            size: Size::Word,
            a: Operand::DataRegLow16(dn),
            b: Operand::Scratch(MUL_IMM_SRC_SLOT),
            dst: Dest::DataReg(dn),
        });
    } else {
        // Memory / Dn-direct: ea_src places the make-op last (after the refill(s)), with the source operand it
        // resolves (Scratch(0) for memory, DataRegLow16 for Dn-direct). The Alu returns the data-dependent idle.
        ea_src(&mut buf, mode, reg, Size::Word, |src| MicroOp::Alu {
            op,
            size: Size::Word,
            a: Operand::DataRegLow16(dn),
            b: src,
            dst: Dest::DataReg(dn),
        });
    }
    buf.finish()
}

/// Scratch slot holding a `DIV`-source `#imm` divisor, captured from `prefetch[1]` BEFORE the two ext-word
/// refills shift it out (the same capture-then-idle ordering `chk_recipe`/`mul_recipe` use for `#imm`). Slot 0
/// — the same slot a memory-source `ea_src` deposits its read divisor into.
const DIV_IMM_SRC_SLOT: u8 = 0;

/// `DIV <ea>,Dn` — the shared recipe for `DIVU` (`opcode & 0xF1C0 == 0x80C0`, opmode 3, [`AluOp::Divu`]) and
/// (in a later commit) `DIVS` (`0x81C0`, opmode 7) in the 0x8 space (both DISJOINT from OR's opmode
/// 0/1/2/4/5/6). The full 32-bit `Dn` (bits 11-9) is the dividend (`a` = [`Operand::DataRegFull`] — NOT MUL's
/// low-16 multiplicand); the source EA (`mode/reg` in bits 5-0, all 11 legal source modes — An-direct is
/// illegal/absent) supplies the word divisor. The `AluOp::Divu` exec arm computes value+flags and RETURNS its
/// data-dependent cycle cost (or, on a zero divisor, rewrites the in-flight `MicroState` into the vector-5
/// div0 frame), exactly the MUL Alu-returns-cycles mechanism.
///
/// The recipe is `mul_recipe`'s shape (reuse `ea_src(Size::Word)` for memory/`Dn`, and `chk_recipe`'s `#imm`
/// capture-then-idle ordering for `(7,4)`) with ONE deviation: [`RecipeBuf::swap_last_two`] swaps the trailing
/// `[Prefetch, Alu]` into `[Alu, Prefetch]`. DIVU pins the `[idle, prefetch]` transaction order (the division
/// idle runs BEFORE the instruction-completing refill) — the OPPOSITE of MUL's `[prefetch, idle]`. The Alu
/// therefore returns the documented division cost minus 4, the trailing `Prefetch` books that 4, and the
/// odd-EA divisor read still faults on the (now-earlier-than-the-final-refill) `Read`, installing the E3/E4
/// 14-byte frame — all 11 modes incl PC-relative/#imm + the odd-EA word-read address error stay in scope.
fn div_recipe(opcode: u16, op: AluOp) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if (mode, reg) == (7, 4) {
        // #imm: capture the immediate (prefetch[1], zero-extended) into scratch BEFORE the two refills shift it
        // out, run both refills, then the Divu Alu (the capture-then-idle ordering chk_recipe/mul_recipe use).
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: DIV_IMM_SRC_SLOT,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Alu {
            op,
            size: Size::Word,
            a: Operand::DataRegFull(dn), // the FULL 32-bit dividend (NOT MUL's DataRegLow16)
            b: Operand::Scratch(DIV_IMM_SRC_SLOT),
            dst: Dest::DataReg(dn),
        });
    } else {
        ea_src(&mut buf, mode, reg, Size::Word, |src| MicroOp::Alu {
            op,
            size: Size::Word,
            a: Operand::DataRegFull(dn),
            b: src,
            dst: Dest::DataReg(dn),
        });
    }
    // DIVU/DIVS order pin: swap the trailing `[Prefetch, Alu]` into `[Alu, Prefetch]` so the division idle runs
    // before the instruction-completing refill (the `[idle, prefetch]` order the data shows).
    buf.swap_last_two();
    buf.finish()
}

/// Scratch slot holding `CHK`'s `#imm` bound, captured from `prefetch[1]` BEFORE the two ext-word refills shift
/// it out, so the `ChkTrap` can run LAST (after both prefetches, with `regs.pc` already at the saved return
/// PC). Slot 0 — the same slot a memory-source `ea_src` deposits its read bound into; the `ChkTrap` reads it
/// before any frame install seeds the saved-PC slot.
const CHK_IMM_BOUND_SLOT: u8 = 0;

/// `CHK <ea>,Dn` (`0100 ddd 110 mmm rrr`, opcode & 0xF1C0 == 0x4180): bounds-check `Dn.w` against `0` and the
/// word operand at the source EA; on out-of-bounds, trap to vector 6 (the standard 6-byte frame). `Dn` is bits
/// 11-9; the source EA is the usual `mode/reg` in bits 5-0 (all 11 legal modes — `An`-direct is illegal for
/// CHK and never appears). Pinned to the vendored `CHK` SST stream (`4190` no-trap len 14; `4d91` Dn<0 trap n6
/// len 44; `4396` Dn>bound trap n4 len 42; `45bc` `#imm` Dn>bound trap n4 len 42):
///
/// The recipe is `<ea_src reads the bound + the refill(s)>, ChkTrap{dn, bound}, Internal(6)`. The trailing
/// `Internal(6)` is the no-trap tail (the `n6` of the no-trap `4190`/`4dbc` anchors). The `ChkTrap` must run
/// **LAST** (after every prefetch) so the saved PC it stacks on a trap equals the live `regs.pc` (= pc + the
/// instruction length). For memory modes (`ea_src` placement `Last`) and `Dn`-direct (`AfterPrefetch`) the
/// shared `ea_src` already places the make-op last, so it routes through `ea_src` with `bound` = `Scratch(0)` /
/// `DataRegLow16` respectively. For `#imm` (placement `First`, where `ea_src` would put the op BEFORE the
/// refills) we emit the sequence directly: capture `prefetch[1]` (the immediate, zero-extended) into a scratch
/// slot via an `EaCalc` BEFORE the two refills shift it out, run both refills, then `ChkTrap` last. (This is
/// the one deviation from a literal `bound: ImmWord` — the data's prefetches-before-frame + `saved PC = regs.pc`
/// require the op to run last, so the immediate is captured first; the bus stream / cycles are identical.)
fn chk_recipe(opcode: u16) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    if (mode, reg) == (7, 4) {
        // #imm: capture the immediate (prefetch[1], zero-extended) into scratch BEFORE the two refills shift it
        // out, run both refills, then ChkTrap last (regs.pc = pc+4 = the saved return PC the frame stacks).
        buf.push(MicroOp::EaCalc {
            base: Operand::ImmWord,
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: CHK_IMM_BOUND_SLOT,
        });
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::Prefetch);
        buf.push(MicroOp::ChkTrap {
            dn,
            bound: Operand::Scratch(CHK_IMM_BOUND_SLOT),
        });
    } else {
        // Memory / Dn-direct: ea_src places the make-op last (after the refill(s)), with the bound operand it
        // resolves (Scratch(0) for memory, DataRegLow16 for Dn-direct).
        ea_src(&mut buf, mode, reg, Size::Word, |bound| MicroOp::ChkTrap {
            dn,
            bound,
        });
    }
    // The no-trap tail (n6) — overwritten in place by the CHK frame recipe if ChkTrap traps.
    buf.push(MicroOp::Internal { cycles: 6 });
    buf.finish()
}

/// Scratch slot holding an `LEA`'s computed effective address (the `EaCalc` destination, consumed by the
/// `MoveA` that writes it to `An`). Slot 0 — fresh recipe, no other live scratch.
const LEA_EA_SLOT: u8 = 0;

/// `LEA <control ea>,An` (`0100 aaa 111 mmm rrr`, opcode & 0xF1C0 == 0x41C0): load the COMPUTED effective
/// address (the address itself — NOT a fetched value; there is NO operand read) into `An = (opcode >> 9) & 7`,
/// affecting **NO flags** (SR untouched). The written value is the FULL 32-bit UNMASKED `base + index + disp`
/// (pinned against the data — every case's An equals the raw sum, never 24-bit-masked), so the EA is built by
/// [`MicroOp::EaCalc`] (unmasked) and copied to `An` by an [`AluOp::MoveA`] (`Size::Long`, the no-flag full-32
/// An write — the SAME early-return op MOVEA.l uses).
///
/// Control addressing modes ONLY (JMP's seven), each with its verified idle profile (all `n` idles are absent
/// from the asserted transaction stream — only the queue refills are `r` bus events, and the total cycle count
/// pins the idles):
/// - `(An)` 010: the EA IS `An` — no `EaCalc`, `MoveA(AddrReg(reg))` then ONE refill. 4 cyc, bus `[PF]`.
/// - `(d16,An)` 101 / `abs.w` 111/0 / `(d16,PC)` 111/2: `EaCalc(base + s16(disp))` first (the disp is in
///   `prefetch[1]` now — the refill would shift it out), then TWO refills. 8 cyc, bus `[PF, PF]`. `(d16,PC)`'s
///   base is `pc+2` (the ext-word address via [`Operand::PcOfExt`]), NOT `pc`.
/// - `(d8,An,Xn)` 110 / `(d8,PC,Xn)` 111/3: `EaCalc(base + index(Xn) + s8(disp8))` first, then the indexed
///   idle (n4) and TWO refills. 12 cyc, bus `[PF, PF]`. Same `pc+2` base for the PC-relative form.
/// - `abs.l` 111/1: assemble the two extension words (HI captured, refill shifts the LO in, `EaCalc` combines)
///   then TWO more refills. 12 cyc, bus `[PF, PF, PF]` (three refills — the LO-word refill is the middle one).
fn lea_recipe(opcode: u16) -> MicroState {
    let an = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // The MoveA that writes the (already-computed) EA to An, full 32-bit, no flags.
    let write_an = |src: Operand| MicroOp::Alu {
        op: AluOp::MoveA,
        size: Size::Long,
        a: src,
        b: Operand::Zero,
        dst: Dest::AddrReg(an),
    };
    match (mode, reg) {
        // (An) — the EA is An itself: copy An → An_dst, then one refill. No EaCalc, no idle.
        (2, _) => {
            buf.push(write_an(Operand::AddrReg(reg)));
            buf.push(MicroOp::Prefetch);
        }
        // d16(An) / abs.w / d16(PC) — EaCalc(base + s16(disp)) into the EA slot FIRST (the disp is in
        // prefetch[1] now; the refill would shift it out), write it to An, then two refills. base = An /
        // Zero (abs.w) / PcOfExt (pc+2 for d16(PC)).
        (5, _) | (7, 0) | (7, 2) => {
            let base = match (mode, reg) {
                (5, _) => Operand::AddrReg(reg),
                (7, 2) => Operand::PcOfExt,
                _ => Operand::Zero, // abs.w
            };
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: LEA_EA_SLOT,
            });
            buf.push(write_an(Operand::Scratch(LEA_EA_SLOT)));
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
        }
        // d8(An,Xn) / d8(PC,Xn) — EaCalc(base + index(Xn) + s8(disp8)) FIRST (the brief ext word — index spec
        // + disp8 — is in prefetch[1] now), write it to An, then the indexed idle (n4, non-bus) and two
        // refills. 12 cyc. base = An / PcOfExt (pc+2).
        (6, _) | (7, 3) => {
            let base = if mode == 6 {
                Operand::AddrReg(reg)
            } else {
                Operand::PcOfExt
            };
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: LEA_EA_SLOT,
            });
            buf.push(write_an(Operand::Scratch(LEA_EA_SLOT)));
            buf.push(MicroOp::Internal { cycles: 4 });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
        }
        // abs.l — assemble the two extension words into the UNMASKED 32-bit EA: the HIGH word (prefetch[1]) is
        // parked into LEA_ABS_L_HI_SLOT via `(0 << 16) | prefetch[1]` (slot 0 is still 0 in a fresh recipe)
        // BEFORE the refill shifts it out; the refill reads the LOW word (at pc+4) into prefetch[1]; the second
        // EaCalc combines `(hi << 16) | lo` into the EA slot. Then write it to An and two more refills. 12 cyc,
        // bus [PF, PF, PF] (the LO-word refill is the first).
        (7, 1) => {
            buf.push(MicroOp::EaCalc {
                base: Operand::Scratch(LEA_EA_SLOT), // still 0 — parks the HI word unmasked
                index: Operand::Zero,
                disp: Operand::ExtWordHi,
                dst: LEA_ABS_L_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::Scratch(LEA_ABS_L_HI_SLOT),
                index: Operand::Zero,
                disp: Operand::ExtWordRaw,
                dst: LEA_EA_SLOT,
            });
            buf.push(write_an(Operand::Scratch(LEA_EA_SLOT)));
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
        }
        _ => unreachable!("lea_recipe: non-control EA mode {mode}/{reg}"),
    }
    buf.finish()
}

/// Scratch slot parking the captured HIGH word of an `LEA abs.l` EA between the two extension-word captures, so
/// the LOW-word refill does not clobber it. Slot 1 — distinct from [`LEA_EA_SLOT`] (slot 0) so both halves are
/// snapshot-visible mid-assembly.
const LEA_ABS_L_HI_SLOT: u8 = 1;

/// Scratch slot holding a `PEA`'s computed effective address (the value pushed as a long) — shared with
/// [`LEA_EA_SLOT`] (slot 0) since PEA reuses LEA's EaCalc machinery to materialize the UNMASKED 32-bit EA.
const PEA_EA_SLOT: u8 = 0;

/// Scratch slot parking the captured HIGH word of a `PEA abs.l` EA between the two extension-word captures (the
/// LEA `abs.l` HI slot, slot 1) — distinct from [`PEA_EA_SLOT`] (slot 0) and [`PEA_LO_ADDR_SLOT`] (slot 2).
const PEA_ABS_L_HI_SLOT: u8 = 1;

/// Scratch slot holding the **address** of a `PEA` push's LOW half (`SP + 2`), materialized once by an
/// [`MicroOp::EaCalc`] (masked to the 24-bit bus — a real even bus address) so the low-word `Write` hits exactly
/// `SP + 2`. Slot 2 — distinct from the EA / abs.l-HI slots (0 / 1) so every value is snapshot-visible mid-push.
const PEA_LO_ADDR_SLOT: u8 = 2;

/// `PEA <control ea>` (`0100 1000 01 mmm rrr`, opcode & 0xFFC0 == 0x4840, mode >= 2): compute the COMPUTED
/// effective address (the address itself — NOT a fetched value; there is NO operand read, exactly like LEA) and
/// PUSH it as a LONG to `-(SP)`: `SP -= 4`, hi word @ SP−4, lo word @ SP−2 (the BSR/JSR long-push order, pinned
/// to the data). Affects **NO flags** (SR untouched). The pushed value is the FULL 32-bit UNMASKED
/// `base + index + disp` (e.g. a `d16(PC)` EA with the upper bits set — pinned against the data), built by
/// [`MicroOp::EaCalc`] (unmasked) into [`PEA_EA_SLOT`].
///
/// Control addressing modes ONLY (the SAME seven as LEA; the `mode >= 2` guard keeps SWAP's mode-000 form —
/// `0x4840` mask `0xFFF8` — from ever colliding). **Timing = LEA + 8** (the two word pushes). The r/w bus ORDER
/// per mode is pinned to the vendored `PEA` stream (idles are non-bus, so they only pin the cycle total):
/// - `(An)` 010: EA = An (an `EaCalc(An)` into the EA slot), ONE refill, then the push. **12 cyc**, bus
///   `[r, w, w]`.
/// - `(d16,An)` 101 / `(d16,PC)` 111/2: `EaCalc(base + s16(disp))` first, TWO refills, then the push. **16
///   cyc**, bus `[r, r, w, w]`. `(d16,PC)`'s base is `pc+2` (via [`Operand::PcOfExt`]).
/// - `abs.w` 111/0: `EaCalc(s16(ext))` first, ONE refill, the push, then a SECOND refill LAST (the load-bearing
///   ordering difference from `d16` — same cycle total, different r/w order). **16 cyc**, bus `[r, w, w, r]`.
/// - `(d8,An,Xn)` 110 / `(d8,PC,Xn)` 111/3: `EaCalc(base + index(Xn) + s8(disp8))` first, the indexed idle
///   (n4), TWO refills, then the push. **20 cyc**, bus `[r, r, w, w]`. Same `pc+2` base for the PC-relative form.
/// - `abs.l` 111/1: assemble the two extension words (HI captured, refill shifts the LO in, `EaCalc` combines),
///   the push splitting the tail (`[r@c04, r@c06, w, w, r@c08]`). **20 cyc**, bus `[r, r, w, w, r]`.
fn pea_recipe(opcode: u16) -> MicroState {
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // The long push of the computed EA (in PEA_EA_SLOT) to -(SP): predecrement SP by 4, materialize the LOW
    // half's address (SP−4 + 2), then write hi @ SP−4 (the new SP) then lo @ SP−2 — the BSR/JSR push order.
    let push_ea = |buf: &mut RecipeBuf| {
        buf.push(MicroOp::AdjustAddr { reg: 7, delta: -4 });
        buf.push(MicroOp::EaCalc {
            base: Operand::AddrReg(7),
            index: Operand::Zero,
            disp: Operand::WordStep,
            dst: PEA_LO_ADDR_SLOT,
        });
        buf.push(MicroOp::Write {
            addr: Operand::AddrReg(7),
            fc: super::microop::Fc::Data,
            size: Size::Word,
            value: Operand::ScratchHi16(PEA_EA_SLOT),
        });
        buf.push(MicroOp::Write {
            addr: Operand::Scratch(PEA_LO_ADDR_SLOT),
            fc: super::microop::Fc::Data,
            size: Size::Word,
            value: Operand::Scratch(PEA_EA_SLOT),
        });
    };
    match (mode, reg) {
        // (An) — the EA is An itself: EaCalc(An) into the EA slot (unmasked value), ONE refill, then the push.
        (2, _) => {
            buf.push(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::Zero,
                disp: Operand::Zero,
                dst: PEA_EA_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            push_ea(&mut buf);
        }
        // d16(An) / d16(PC) — EaCalc(base + s16(disp)) FIRST (the disp is in prefetch[1] now), TWO refills, then
        // the push. base = An / PcOfExt (pc+2 for d16(PC)).
        (5, _) | (7, 2) => {
            let base = if mode == 5 {
                Operand::AddrReg(reg)
            } else {
                Operand::PcOfExt
            };
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: PEA_EA_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
            push_ea(&mut buf);
        }
        // abs.w — EaCalc(s16(ext)) FIRST, ONE refill, the push, then a SECOND refill LAST. The push splits the
        // two refills (the load-bearing r/w order `[r, w, w, r]`, distinct from d16's `[r, r, w, w]`).
        (7, 0) => {
            buf.push(MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: PEA_EA_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            push_ea(&mut buf);
            buf.push(MicroOp::Prefetch);
        }
        // d8(An,Xn) / d8(PC,Xn) — EaCalc(base + index(Xn) + s8(disp8)) FIRST, the indexed idle (n4), TWO
        // refills, then the push. 20 cyc. base = An / PcOfExt (pc+2).
        (6, _) | (7, 3) => {
            let base = if mode == 6 {
                Operand::AddrReg(reg)
            } else {
                Operand::PcOfExt
            };
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: PEA_EA_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: 4 });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::Prefetch);
            push_ea(&mut buf);
        }
        // abs.l — assemble the two extension words into the UNMASKED 32-bit EA (the HI word parked into
        // PEA_ABS_L_HI_SLOT via `(0 << 16) | prefetch[1]` before the refill shifts it out; the refill reads the
        // LO word; the second EaCalc combines `(hi << 16) | lo`). The push SPLITS the tail: after the combine,
        // ONE refill (r@c06), the push (w, w), then a FINAL refill (r@c08). Bus `[r@c04, r@c06, w, w, r@c08]`.
        (7, 1) => {
            buf.push(MicroOp::EaCalc {
                base: Operand::Scratch(PEA_EA_SLOT), // still 0 — parks the HI word unmasked
                index: Operand::Zero,
                disp: Operand::ExtWordHi,
                dst: PEA_ABS_L_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::Scratch(PEA_ABS_L_HI_SLOT),
                index: Operand::Zero,
                disp: Operand::ExtWordRaw,
                dst: PEA_EA_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            push_ea(&mut buf);
            buf.push(MicroOp::Prefetch);
        }
        _ => unreachable!("pea_recipe: non-control EA mode {mode}/{reg}"),
    }
    buf.finish()
}

/// Scratch slot holding a `LINK`'s sign-extended displacement (`sign_extend16(prefetch[1])`), captured by an
/// [`MicroOp::EaCalc`] BEFORE the first `Prefetch` shifts the displacement word out of the queue. Consumed by
/// the trailing `Adda` that computes the final `SP = (SP−4) + disp`. Slot 0.
const LINK_DISP_SLOT: u8 = 0;

/// Scratch slot holding the 32-bit value `LINK` pushes as a long — materialized by an [`MicroOp::EaCalc`] from
/// `AddrReg(an)` AFTER the `SP −= 4` predecrement. For `An != 7` this is the (unchanged) original `An`; for the
/// **A7 quirk** (`LINK A7`, `An ≡ SP`) it is the already-decremented `SP` (= `SP − 4`), which is exactly the
/// value the 68000 stores — the predecrement-first quirk falls out for free. Consumed by the two push `Write`s
/// (`ScratchHi16` @ SP−4, `Scratch` @ SP−2). Slot 1 — distinct from the disp slot so both survive the push.
const LINK_PUSH_SLOT: u8 = 1;

/// Scratch slot holding the **address** of a `LINK` push's LOW half (`SP−4 + 2 = SP−2`), materialized once by an
/// [`MicroOp::EaCalc`] (masked to the 24-bit bus — a real even bus address) so the low-word `Write` hits exactly
/// `SP−2`. Slot 2 — distinct from the disp / push slots so every value is snapshot-visible mid-push.
const LINK_LO_ADDR_SLOT: u8 = 2;

/// `LINK An,#disp` (`0100 1110 0101 0 rrr`, opcode & 0xFFF8 == 0x4E50): allocate a stack frame — `SP −= 4;
/// [SP] = An (long push); An = SP; SP += sign_extend16(disp)`. The displacement is the extension word
/// `prefetch[1]`. Affects **NO flags** (SR untouched). Flat **16 cyc**; the bus stream is `[r@pc+4, w@SP−4 (hi),
/// w@SP−2 (lo), r@pc+6]` — the JSR-style reload interleave (the first queue refill precedes the push, the second
/// follows it), pinned to the vendored `LINK` SST stream.
///
/// **The A7 quirk (`LINK A7`, `An ≡ SP`, `0x4E57`, 1005 cases — LOAD-BEARING):** the `SP −= 4` predecrement runs
/// FIRST, and since `An` IS `SP`, the value pushed is the ALREADY-DECREMENTED `SP` (= `SP − 4`), NOT the original
/// A7. This falls out for free: [`LINK_PUSH_SLOT`] is materialized from `AddrReg(an)` AFTER the `AdjustAddr(7,−4)`,
/// so for `an == 7` it reads `SP − 4`. Then `An = SP` (a no-op for A7 — A7 already holds `SP − 4`), then
/// `SP += disp` gives `A7 = (SP − 4) + disp`. For `An != 7`: pushed = the original `An` (unaffected by the SP
/// decrement), `An = SP − 4`, `SP = (SP − 4) + disp`.
///
/// The `An = SP` write ([`AluOp::MoveA`] copying `AddrReg(7)`) runs BEFORE the `SP += disp` add ([`AluOp::Adda`]
/// on `AddrReg(7)`), so a non-A7 `An` captures `SP − 4` (the decremented SP) before disp is folded in; the disp
/// add always targets `AddrReg(7)` (never `An`) so a negative disp (`SP += negative`, common) lands on the stack
/// pointer regardless of which `An` was linked.
fn link_recipe(opcode: u16) -> MicroState {
    let an = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // Capture the sign-extended displacement into a scratch slot NOW — the first Prefetch below shifts the disp
    // word out of prefetch[1], so it must be read before then. (0 + 0 + sign_extend16(prefetch[1]).)
    buf.push(MicroOp::EaCalc {
        base: Operand::Zero,
        index: Operand::Zero,
        disp: Operand::DispWord,
        dst: LINK_DISP_SLOT,
    });
    // The FIRST queue refill (r@pc+4) precedes the push — the JSR reload interleave.
    buf.push(MicroOp::Prefetch);
    // Pre-decrement the stack pointer by 4 (the long push). A7 now points at SP−4.
    buf.push(MicroOp::AdjustAddr { reg: 7, delta: -4 });
    // Materialize the value to push from An AFTER the decrement: original An (An != 7) or SP−4 (the A7 quirk).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(an),
        index: Operand::Zero,
        disp: Operand::Zero,
        dst: LINK_PUSH_SLOT,
    });
    // Materialize the LOW half's address (SP−4 + 2 = SP−2), masked to the 24-bit bus.
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(7),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: LINK_LO_ADDR_SLOT,
    });
    // Push the long — hi @ SP−4 (AddrReg(7), the new SP) FIRST, lo @ SP−2 (big-endian, the BSR/JSR push order).
    buf.push(MicroOp::Write {
        addr: Operand::AddrReg(7),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        value: Operand::ScratchHi16(LINK_PUSH_SLOT),
    });
    buf.push(MicroOp::Write {
        addr: Operand::Scratch(LINK_LO_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        value: Operand::Scratch(LINK_PUSH_SLOT),
    });
    // An = SP (a no-flag full-32 copy). For An != 7 this captures the DECREMENTED SP (SP−4) before disp is
    // folded in; for An == 7 it is a no-op (A7 already holds SP−4). Runs BEFORE the disp add.
    buf.push(MicroOp::Alu {
        op: AluOp::MoveA,
        size: Size::Long,
        a: Operand::AddrReg(7),
        b: Operand::Zero,
        dst: Dest::AddrReg(an),
    });
    // SP += sign_extend16(disp) — ALWAYS on AddrReg(7) (the stack pointer), never An. The captured disp slot
    // already holds the full 32-bit sign extension, so Adda at the long boundary adds it verbatim.
    buf.push(MicroOp::Alu {
        op: AluOp::Adda,
        size: Size::Long,
        a: Operand::AddrReg(7),
        b: Operand::Scratch(LINK_DISP_SLOT),
        dst: Dest::AddrReg(7),
    });
    // The SECOND queue refill (r@pc+6) follows the push — completing the reload interleave.
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot holding the HIGH word of an `UNLINK` popped long (read @ `An`), assembled with the LOW word by
/// [`MicroOp::Combine32`]. Slot 0 — the conventional read-value slot.
const UNLINK_HI_SLOT: u8 = 0;

/// Scratch slot holding the LOW word of an `UNLINK` popped long (read @ `An + 2`). Slot 4 — distinct from the
/// hi-word / lo-addr / sp / pop slots so every half is snapshot-visible mid-pop.
const UNLINK_LO_SLOT: u8 = 4;

/// Scratch slot holding the **address** of an `UNLINK` pop's LOW half (`An + 2`), materialized once by an
/// [`MicroOp::EaCalc`] (masked to the 24-bit bus — a real even bus address). Slot 2.
const UNLINK_LO_ADDR_SLOT: u8 = 2;

/// Scratch slot holding the pre-computed `An_old + 4` — the stack pointer's post-pop value, materialized by an
/// [`MicroOp::EaCalc`] (`An + 2 + 2`) while `AddrReg(an)` still holds the original `An`. Slot 3.
const UNLINK_SP_SLOT: u8 = 3;

/// Scratch slot holding the assembled 32-bit `UNLINK` popped long (the frame's saved `An`). Slot 5 — distinct
/// from the read/addr/sp slots so the popped value survives until the final `An` write. Slot 5.
const UNLINK_POP_SLOT: u8 = 5;

/// `UNLINK An` / `UNLK An` (`0100 1110 0101 1 rrr`, opcode & 0xFFF8 == 0x4E58): deallocate a stack frame —
/// `SP = An; An = [SP] (long pop); SP = An_old + 4`. Affects **NO flags** (SR untouched). Flat **12 cyc**; the
/// bus stream is `[r@An (hi), r@An+2 (lo), r@pc+4]` — two word reads (the popped long) + one queue refill,
/// pinned to the vendored `UNLINK` SST stream.
///
/// The pop reads at `An` (the `SP = An` step is unobservable except through this read address, so it is folded
/// into addressing `AddrReg(an)` directly — no explicit SP=An write). The final register updates are ordered
/// **`SP = An_old + 4` THEN `An = popped`** so the pop write is LAST: for `An != 7` both stick (distinct regs);
/// for **`UNLK A7`** (`An ≡ SP`, `0x4E5F`) the `SP = An_old + 4` write is CLOBBERED by the pop write, leaving
/// `A7 = the popped long` (the documented net effect — the pop wins). `An_old + 4` is pre-computed into
/// [`UNLINK_SP_SLOT`] via an `EaCalc(An + 2 + 2)` while `AddrReg(an)` still holds the original `An` (it is only
/// rewritten by the final pop `MoveA`).
fn unlink_recipe(opcode: u16) -> MicroState {
    let an = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // Pop the saved An — HI word @ An first (SP=An folded into the read address).
    buf.push(MicroOp::Read {
        addr: Operand::AddrReg(an),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: UNLINK_HI_SLOT,
    });
    // Materialize the LOW half's address (An + 2), masked to the 24-bit bus.
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(an),
        index: Operand::Zero,
        disp: Operand::WordStep,
        dst: UNLINK_LO_ADDR_SLOT,
    });
    // LOW word @ An + 2.
    buf.push(MicroOp::Read {
        addr: Operand::Scratch(UNLINK_LO_ADDR_SLOT),
        fc: super::microop::Fc::Data,
        size: Size::Word,
        dst: UNLINK_LO_SLOT,
    });
    // Pre-compute the post-pop stack pointer An_old + 4 (= An + 2 + 2) while AddrReg(an) still holds the original
    // An (only the final pop MoveA rewrites it).
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(an),
        index: Operand::WordStep,
        disp: Operand::WordStep,
        dst: UNLINK_SP_SLOT,
    });
    // Assemble the UNMASKED 32-bit popped long (the saved An).
    buf.push(MicroOp::Combine32 {
        hi: UNLINK_HI_SLOT,
        lo: Operand::Scratch(UNLINK_LO_SLOT),
        dst: UNLINK_POP_SLOT,
    });
    // SP = An_old + 4 (a no-flag full-32 copy to AddrReg(7)). Runs BEFORE the An write so that for UNLK A7 the
    // pop write below clobbers it (net A7 = the popped long).
    buf.push(MicroOp::Alu {
        op: AluOp::MoveA,
        size: Size::Long,
        a: Operand::Scratch(UNLINK_SP_SLOT),
        b: Operand::Zero,
        dst: Dest::AddrReg(7),
    });
    // An = the popped long (LAST — for An == 7 this wins over the SP write above).
    buf.push(MicroOp::Alu {
        op: AluOp::MoveA,
        size: Size::Long,
        a: Operand::Scratch(UNLINK_POP_SLOT),
        b: Operand::Zero,
        dst: Dest::AddrReg(an),
    });
    // The trailing queue refill (r@pc+4).
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot carrying a `MOVEP` transfer's running / EA pointer (`EA = An + sx16(d16)`, then advanced `+= 2`
/// per byte by [`MicroOp::MovepStore`]/[`MicroOp::MovepLoad`]). Slot 0 — reused as BOTH the initial EA and the
/// running alternating-address pointer. NEVER `An` itself (MOVEP never mutates the address register).
const MOVEP_ADDR_SLOT: u8 = 0;

/// `MOVEP.w`/`MOVEP.l` (`0000 rrr 1 oo 001 aaa`, opcode & 0xF138 == 0x0108) — move a DATA register `Dn` ↔ the
/// ALTERNATING (even/odd) memory addresses `EA, EA+2, …` where `EA = An + sx16(d16)` (`d16` = the extension word
/// = `prefetch[1]`, IDENTICAL to the `d16(An)` addressing mode). Opmode bit7 = DIRECTION (0 = mem→reg READ / 1 =
/// reg→mem WRITE), bit6 = SIZE (0 = word `nbytes = 2` / 1 = long `nbytes = 4`). NO flags (SR byte-identical).
/// Size-parameterized so the LONG twin (opmodes 5/7) reuses this verbatim.
///
/// The bus stream is a byte-scatter framed by two prefetches (pinned 0-mismatch against the vendored stream):
/// `[r pc+4 .w (refill), <N byte accesses at EA, EA+2, …> (fc5 .b), r pc+6 .w (final)]`. The **refill Prefetch
/// runs FIRST** (before the byte accesses); the **final Prefetch LAST** — LOAD-BEARING (the per-cycle
/// transaction gate is order-sensitive). The `EaCalc` MUST precede the refill Prefetch: `d16` lives in
/// `prefetch[1]` and the refill shifts it out (the mode-5 `d16(An)` EA leg already respects this). Each byte
/// access is its OWN [`MicroOp::MovepStore`]/[`MicroOp::MovepLoad`] (a per-byte bus-access quiesce boundary,
/// like MOVEM's one-op-per-register) — NEVER collapsed. Uniform timing: word `4 + 2·4 + 4 = 16`, long
/// `4 + 4·4 + 4 = 24`. Op count: word 5, long 7 (≤ [`MAX_OPS`]).
fn movep_recipe(opcode: u16, _regs: &Registers) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let an = (opcode & 7) as u8;
    let opmode = (opcode >> 6) & 7;
    let mem2reg = (opmode & 0x2) == 0; // bit7 == 0
    let nbytes = if (opmode & 0x1) == 0 { 2u8 } else { 4 }; // bit6: word / long
    let mut buf = RecipeBuf::new();

    // Materialize EA = An + sx16(d16) into the running-pointer slot BEFORE the refill (d16 is in prefetch[1] now;
    // the refill would shift it out). Verbatim the `d16(An)` (mode 5) EaCalc shape.
    buf.push(MicroOp::EaCalc {
        base: Operand::AddrReg(an),
        index: Operand::Zero,
        disp: Operand::DispWord,
        dst: MOVEP_ADDR_SLOT,
    });
    // Refill @ pc+4 (FIRST — precedes the byte accesses).
    buf.push(MicroOp::Prefetch);
    // The per-byte scatter, big-endian over the alternating addresses: word shifts [8, 0]; long [24, 16, 8, 0].
    for k in 0..nbytes {
        let shift = 8 * (nbytes - 1 - k);
        if mem2reg {
            buf.push(MicroOp::MovepLoad {
                dn,
                shift,
                addr_slot: MOVEP_ADDR_SLOT,
            });
        } else {
            buf.push(MicroOp::MovepStore {
                dn,
                shift,
                addr_slot: MOVEP_ADDR_SLOT,
            });
        }
    }
    // Final refill @ pc+6 (LAST).
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot carrying a `MOVEM` transfer's running address (the store/load pointer): the base EA for
/// control modes (advanced per register by the transfer op), or the `-(An)` running address (decremented per
/// register). Slot 0. NEVER `An` itself — so an `An`-in-list `-(An)` store writes the INITIAL An (the recipe's
/// trailing `MoveA` writes the final address to An); a `(An)+` load uses `An` directly (the transfer op's
/// postincrement), so this slot is unused for that one mode.
const MOVEM_ADDR_SLOT: u8 = 0;

/// Scratch slot parking the captured HIGH word of a `MOVEM abs.l` EA between the two extension-word captures
/// (the same "capture before the refill shifts it out" shape as `LEA`/`PEA` `abs.l`). Slot 1 — distinct from
/// [`MOVEM_ADDR_SLOT`] so both halves are snapshot-visible mid-assembly.
const MOVEM_HI_SLOT: u8 = 1;

/// Scratch slot the `MOVEM` mem→reg **phantom read** targets (its value is DISCARDED). A single trailing WORD
/// read one word past the last transferred word — a REAL bus transaction (the +4 in every mem→reg base
/// timing) that does NOT advance the running pointer. Slot 2 — distinct from the address / hi slots.
const MOVEM_PHANTOM_SLOT: u8 = 2;

/// `MOVEM.w`/`MOVEM.l` (`0100 1D00 1S mmm rrr`, opcode & 0xFB80 == 0x4880) — move a register list ↔ memory,
/// NO flags. Direction = bit 10 (0x0400): reg→mem (0x4880/0x48C0) / mem→reg (0x4C80/0x4CC0). The register-list
/// mask is the extension word `prefetch[1]` — AVAILABLE AT DECODE TIME (like the Bcc/DBcc live reads), so this
/// expands it into an exact-length linear transfer recipe (one [`MicroOp::MovemStore`]/[`MicroOp::MovemLoad`]
/// per set register). Because the recipe stays a flat linear list, both drivers + mid-instruction
/// snapshot/restore keep working (exactly like the Bcc/DBcc decode-time expansion).
///
/// **Register ↔ bit mapping:** NORMAL (`bit0=D0 … bit7=D7, bit8=A0 … bit15=A7`) for EVERY case EXCEPT reg→mem
/// `-(An)` predecrement, where the mask is interpreted REVERSED (`bit0=A7 … bit7=A0, bit8=D7 … bit15=D0`).
/// A register index `< 8` is `Dn`; `>= 8` is `A(reg-8)` (`reg == 15` = the active A7).
///
/// **Recipe shape** (per the 0-mismatch verified transaction interleave): `n` leading `Prefetch`s bracketing
/// the EA-extension-word captures (1 for `(An)`/`(An)+`/`-(An)`, 2 for `d16`/`abs.w`/PC-rel, 2 + an
/// `Internal(2)` indexed idle for `d8(An,Xn)`/`d8(PC,Xn)`, 3 for `abs.l`) → the per-register transfers (in
/// ascending bit order, REVERSED for `-(An)`) → the mem→reg PHANTOM word read → one trailing `Prefetch`.
/// `length = base + per·n` (per = 4 `.w` / 8 `.l`); the base falls out of the prefetch/idle/phantom cycles.
/// - **reg→mem control (fwd):** EA computed once into [`MOVEM_ADDR_SLOT`]; store the set registers at
///   increasing addresses (the transfer op advances the slot `+= size` after each store).
/// - **reg→mem `-(An)`:** the running address starts at `An` (into [`MOVEM_ADDR_SLOT`]); each store DECREMENTS
///   it by `size` FIRST (the `predec` op), iterating the REVERSED list; the trailing `MoveA` writes the final
///   decremented address to `An`. An `An`-in-list store writes the INITIAL An (the slot is not `An`).
/// - **mem→reg:** load forward from increasing addresses; a WORD load SIGN-EXTENDS to 32 bits. `(An)+` uses
///   `An` directly (the transfer op's postincrement — `An` ends `base + n·size`); the trailing phantom word
///   read hits the final address and does NOT advance it.
fn movem_recipe(opcode: u16, size: Size, regs: &Registers) -> MicroState {
    let dir = (opcode >> 10) & 1; // 0 = reg→mem, 1 = mem→reg
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mask = regs.prefetch[1];
    let mut buf = RecipeBuf::new();

    let predec = dir == 0 && mode == 4; // reg→mem -(An)
    let postinc = dir == 1 && mode == 3; // mem→reg (An)+

    // The ordered register-list indices (0..=15 = D0..D7, A0..A7). REVERSED bit order for -(An) predecrement,
    // NORMAL (ascending) otherwise.
    let mut list: [u8; 16] = [0; 16];
    let mut n = 0usize;
    if predec {
        // Reversed: bit0 = A7 (index 15), bit1 = A6 … bit8 = D7 … bit15 = D0. So a set bit `b` maps to
        // register index `15 - b`.
        for b in 0..16u8 {
            if mask & (1 << b) != 0 {
                list[n] = 15 - b;
                n += 1;
            }
        }
    } else {
        // Normal: bit0 = D0 … bit7 = D7, bit8 = A0 … bit15 = A7 → register index == bit position.
        for b in 0..16u8 {
            if mask & (1 << b) != 0 {
                list[n] = b;
                n += 1;
            }
        }
    }

    // --- Leading prefetch(es) + EA-base computation into MOVEM_ADDR_SLOT (except (An)+, which uses An). ---
    // The first Prefetch always runs first (it fetches the word after the mask into the queue). For a
    // multi-word EA the extension word(s) are captured from prefetch[1] right after the Prefetch that fetches
    // them, BEFORE the next Prefetch shifts them out (the standard capture-before-refill discipline).
    match (mode, reg) {
        // (An) / (An)+ / -(An) — no extension word: one leading Prefetch, running address = An (into the
        // scratch slot; the transfer op advances/decrements it per register). For (An)+ an extra
        // `AdjustAddr(An, +2)` commits the FIRST-WORD postincrement BEFORE the first load, so a faulting
        // odd-base (An)+ leaves `An = base + 2` — ONE WORD, for BOTH `.w` and `.l` (the long access is
        // word-at-a-time; the pointer advances one word, then the odd-address fault). The abort-commit is
        // ALWAYS a single word; on the clean path the recipe's trailing MoveA overwrites An = base + n·size.
        (2, _) | (3, _) | (4, _) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::AddrReg(reg),
                index: Operand::Zero,
                disp: Operand::Zero,
                dst: MOVEM_ADDR_SLOT,
            });
            if postinc {
                buf.push(MicroOp::AdjustAddr { reg, delta: 2 });
            }
        }
        // d16(An) / abs.w / d16(PC) — one displacement extension word: Prefetch (fetch disp), EaCalc(disp),
        // Prefetch. base = An / Zero (abs.w) / pc+2 (d16(PC), the ext-word address == pc+4 since the first
        // Prefetch advanced pc to pc+2).
        (5, _) | (7, 0) | (7, 2) => {
            let base = match (mode, reg) {
                (5, _) => Operand::AddrReg(reg),
                (7, 2) => Operand::PcOfExt,
                _ => Operand::Zero, // abs.w
            };
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::Zero,
                disp: Operand::DispWord,
                dst: MOVEM_ADDR_SLOT,
            });
            buf.push(MicroOp::Prefetch);
        }
        // d8(An,Xn) / d8(PC,Xn) — one brief extension word (index + disp8): Prefetch (fetch brief),
        // EaCalc(brief), the indexed idle (n2, between the two prefetches), Prefetch. base = An / pc+2.
        (6, _) | (7, 3) => {
            let base = if mode == 6 {
                Operand::AddrReg(reg)
            } else {
                Operand::PcOfExt
            };
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base,
                index: Operand::BriefIndex,
                disp: Operand::BriefDisp8,
                dst: MOVEM_ADDR_SLOT,
            });
            buf.push(MicroOp::Internal { cycles: 2 });
            buf.push(MicroOp::Prefetch);
        }
        // abs.l — two extension words: Prefetch (fetch hi), EaCalc parks (hi << 16) into MOVEM_HI_SLOT,
        // Prefetch (fetch lo), EaCalc combines (hi << 16) | lo into MOVEM_ADDR_SLOT, Prefetch.
        (7, 1) => {
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::Zero,
                index: Operand::Zero,
                disp: Operand::ExtWordHi,
                dst: MOVEM_HI_SLOT,
            });
            buf.push(MicroOp::Prefetch);
            buf.push(MicroOp::EaCalc {
                base: Operand::Scratch(MOVEM_HI_SLOT),
                index: Operand::Zero,
                disp: Operand::ExtWordRaw,
                dst: MOVEM_ADDR_SLOT,
            });
            buf.push(MicroOp::Prefetch);
        }
        _ => unreachable!("movem_recipe: invalid MOVEM mode {mode}/{reg}"),
    }

    // --- The per-register transfers (in list order; reversed for -(An)). ---
    for &r in &list[..n] {
        if dir == 0 {
            buf.push(MicroOp::MovemStore {
                reg: r,
                size,
                addr_slot: MOVEM_ADDR_SLOT,
                predec,
            });
        } else {
            buf.push(MicroOp::MovemLoad {
                reg: r,
                size,
                addr_slot: MOVEM_ADDR_SLOT,
            });
        }
    }

    // --- mem→reg: the trailing PHANTOM word read (a real bus transaction, value discarded, no advance). ---
    // Always at the final running address (base + n·size), which is in the scratch slot for every mem→reg mode
    // (including (An)+ — the scratch pointer, not An, since An may have been overwritten by an An-in-list load).
    if dir == 1 {
        buf.push(MicroOp::Read {
            addr: Operand::Scratch(MOVEM_ADDR_SLOT),
            fc: super::microop::Fc::Data,
            size: Size::Word,
            dst: MOVEM_PHANTOM_SLOT,
        });
    }

    // --- reg→mem -(An): write the final DECREMENTED address to An (a no-flag full-32 MoveA). ---
    // --- mem→reg (An)+: write the final POSTINCREMENTED address to An (base + n·size — the postincrement wins
    //     over any value an An-in-list load wrote into An). Both use the running scratch slot's final value. ---
    if predec || postinc {
        buf.push(MicroOp::Alu {
            op: AluOp::MoveA,
            size: Size::Long,
            a: Operand::Scratch(MOVEM_ADDR_SLOT),
            b: Operand::Zero,
            dst: Dest::AddrReg(reg),
        });
    }

    // --- The trailing queue refill (the last Prefetch of every MOVEM). ---
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// Scratch slot holding the `*toSR` discard read's value (the word @ pc+4, read under the OLD function code
/// before the SR write). Slot 0 — the value is never used (the re-prefetch reads it again from the bus), but
/// a `Read` needs a destination slot.
const TO_SR_DISCARD_SLOT: u8 = 0;

/// `ANDItoSR` (`0x027C`) / `ORItoSR` (`0x007C`) / `EORItoSR` (`0x0A7C`): the privileged immediate-to-SR logic
/// ops — `regs.sr = (regs.sr <op> imm) & SR_IMPLEMENTED` (`0xA71F`). Pinned to the vendored `*toSR` SST stream
/// (`027c`/`007c`/`0a7c`, all **20 cyc**):
///
/// - **A leading discard `Read` @ pc+4** (FC=6 supervisor-program — the OLD mode), then `Internal(8)`, then
///   [`MicroOp::SrLogic`] (which applies the op against `prefetch[1]` and may clear S/T), then **two
///   `Prefetch`s**. The two re-prefetch reads (also @ pc+4 / pc+6) run under the **NEW** mode's function code:
///   FC=6 if S stays set, **FC=2 (user-program)** if the SR write cleared S — the load-bearing FC pin.
/// - The bus stream is `[r@pc+4, r@pc+4, r@pc+6]` (the discard read and the first re-prefetch hit the same
///   address, with `prefetch` unchanged between them), 3 word reads + the n8 idle = 20 cycles. Final pc =
///   pc+4, prefetch = `[word@pc+4, word@pc+6]`.
///
/// All vendored cases start supervisor (S=1, T=0); the user-mode privilege-violation entry is correctness-only
/// (not gated). The `SrLogic` runs BEFORE the two re-prefetches so the FC switch is captured — exactly the
/// inverse-ordered analog of `RTE`'s `LoadSr`-then-reload.
fn to_sr_recipe(op: LogicOp) -> MicroState {
    let mut buf = RecipeBuf::new();
    // The leading discard read @ pc+4 (FC=6, the OLD mode) — its value is re-read by the first re-prefetch.
    buf.push(MicroOp::Read {
        addr: Operand::PcPlus(4),
        fc: super::microop::Fc::Program,
        size: Size::Word,
        dst: TO_SR_DISCARD_SLOT,
    });
    buf.push(MicroOp::Internal { cycles: 8 });
    // The SR write (masked 0xA71F) — may clear S/T, switching the function code of the two re-prefetches below.
    buf.push(MicroOp::SrLogic {
        op,
        value: Operand::ImmWord,
    });
    // The two re-prefetch reads run under the NEW mode's FC (FC2 user-program if S cleared, else FC6).
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `ANDItoCCR` (`0x023C`) / `ORItoCCR` (`0x003C`) / `EORItoCCR` (`0x0A3C`): the immediate-to-CCR logic ops —
/// `regs.sr = (regs.sr & 0xFF00) | (((regs.sr <op> imm) & 0x1F))`. The **byte/CCR-form twins** of the `*toSR`
/// trio: byte-identical in bus stream and timing (all **20 cyc**), the ONLY difference is the write mask — the
/// SR **system byte (bits 8-15: T | S | I) is PRESERVED**, only the low-5 CCR bits change. NOT privileged (the
/// CCR form is legal in user mode), though every vendored case starts supervisor.
///
/// Structurally identical to [`to_sr_recipe`] but pushing [`MicroOp::CcrLogic`] instead of `SrLogic`:
/// - **A leading discard `Read` @ pc+4** (FC=6 — the immediate word re-read by the first re-prefetch), then
///   `Internal(8)`, then `CcrLogic` (which applies the op against `prefetch[1]` on ONLY the CCR bits), then
///   **two `Prefetch`s**. Because `CcrLogic` NEVER changes S, both re-prefetches stay FC=6 (no FC switch — the
///   one way these differ structurally from `*toSR`, where an AND/EOR clearing S flips the re-prefetches to
///   FC2). Reuses [`TO_SR_DISCARD_SLOT`] (the leading discard read is identical to `*toSR`).
/// - Bus stream `[r@pc+4, r@pc+4, r@pc+6]` all FC6 word + the n8 idle = 20 cycles. Final pc = pc+4, prefetch =
///   `[word@pc+4, word@pc+6]`.
fn to_ccr_recipe(op: LogicOp) -> MicroState {
    let mut buf = RecipeBuf::new();
    // The leading discard read @ pc+4 (FC=6) — its value is re-read by the first re-prefetch.
    buf.push(MicroOp::Read {
        addr: Operand::PcPlus(4),
        fc: super::microop::Fc::Program,
        size: Size::Word,
        dst: TO_SR_DISCARD_SLOT,
    });
    buf.push(MicroOp::Internal { cycles: 8 });
    // The CCR write (system byte 0xFF00 PRESERVED, only the low-5 CCR bits change) — S never changes, so the
    // two re-prefetches below stay under the live supervisor mode's FC6 (no FC switch, unlike `*toSR`).
    buf.push(MicroOp::CcrLogic {
        op,
        value: Operand::ImmWord,
    });
    buf.push(MicroOp::Prefetch);
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `RESET` (`0x4E70`): assert the external reset line for 124 cycles. Privileged (supervisor-only; the
/// user-mode privilege-violation entry is correctness-only, not gated). Pinned to the vendored `RESET` SST
/// stream (`4e70`, all **132 cyc**): `[Internal(4), Internal(124), Prefetch]` — an `n4` idle, the `n124`
/// reset-line idle (the widened `u16` `Internal` cycle field), then one `Prefetch` queue refill (FC=6 @ pc+4,
/// advancing pc by 2). No register state changes beyond the queue (4 + 124 + 4 = 132 cycles; bus stream a
/// single FC=6 read @ pc+4).
fn reset_recipe() -> MicroState {
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::Internal { cycles: 4 });
    buf.push(MicroOp::Internal { cycles: 124 });
    buf.push(MicroOp::Prefetch);
    buf.finish()
}

/// `STOP #imm` (`0x4E72`): load the immediate word into the whole SR (masked to `SR_IMPLEMENTED`), then halt
/// fetching until an unmasked interrupt or reset. Yacht L908: `4(0/0)`, no bus access (Yacht notes the second
/// microcycle's spend is genuinely unknown — the one deferred timing item, harmless since no bus event
/// occurs either way). Privileged: a user-mode `STOP` is caught by the decode-time privilege gate (vector 8)
/// and never reaches here. NOT vendored in SST (audit finding 3) — hand-authored vectors only.
///
/// Recipe `[LoadSr(ImmWord), Stop]`: `LoadSr` writes SR (0 cycles, may change S/T/I), then [`MicroOp::Stop`]
/// flags the recipe so the orchestrator ([`Cpu68000::step`]/`step_micro_op`) enters [`CpuState::Stopped`] at
/// completion and books the 4-cycle cost. **No `Prefetch`** — `STOP` does no bus, so `pc`/`prefetch` are left
/// pointing at the `STOP`; advancing past the two-word instruction and refilling the queue is the wake
/// sequence's job (Push A / A4), which must stack PC = the instruction after `STOP` (M68000UM §6.2.5).
fn stop_recipe() -> MicroState {
    let mut buf = RecipeBuf::new();
    buf.push(MicroOp::LoadSr {
        value: Operand::ImmWord,
    });
    buf.push(MicroOp::Stop);
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
    fn step_on_normal_cpu_matches_run_instruction() {
        // The orchestrator's fast path: `step()` on a Normal CPU with nothing in flight is a superset
        // of `run_instruction` — decode prefetch[0], run to completion, return CPU cycles. The public
        // contract is byte-identical to `run_instruction` (the SST harness's untouched entry).
        let (mut cpu, mut bus) = setup_db50();
        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 12, "step returns the instruction's CPU cycles");
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

    // --- Push A / A1: privilege violation (vector 8) ---------------------------------------------
    // Hand-authored (no SST data for this cluster). Pinned to M68000UM §6.3.7 (stacked PC = address
    // of the first word of the offending instruction) + Table 6-2 (vector 8) + Yacht L1550 (34(4/3),
    // stream `nn ns ns nS nV nv np n np`, byte-identical to TRAP). A user-mode ANDItoSR (0x027C, a
    // privileged op per §6.3.7) traps instead of writing SR.
    fn setup_privilege_anditosr_user() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000, // user A7 — untouched (the frame goes to the SUPERVISOR stack)
            ssp: 0x0000_1000, // supervisor A7 — where the frame is pushed
            pc: 0x0C00,       // address of the offending ANDItoSR word
            sr: 0x0004,       // USER mode (S=0), Z set — captured as the stacked SR
            prefetch: [0x027C, 0xF71F], // ANDItoSR #imm; the imm word is never consumed (trap preempts)
        };
        let mut bus = FlatBus::new();
        // Vector 8 @ 0x20 → handler 0x0000_2000.
        for (a, v) in [
            (0x20u32, 0x00u8),
            (0x21, 0x00),
            (0x22, 0x20),
            (0x23, 0x00),
            // Handler code words (two NOPs) so the prefetch reload has something to read.
            (0x2000, 0x4E),
            (0x2001, 0x71),
            (0x2002, 0x4E),
            (0x2003, 0x71),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_privilege_log() -> Vec<Transaction> {
        vec![
            // push_standard_frame on-bus order: PCL @ B+4, SR @ B+0, PCH @ B+2 (B = ssp-6 = 0xFFA).
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x0FFE,
                size: Size::Word,
                value: 0x0C00,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x0FFA,
                size: Size::Word,
                value: 0x0004,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x0FFC,
                size: Size::Word,
                value: 0x0000,
            },
            // Vector fetch (FC=5) then handler reload (FC=6).
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x0020,
                size: Size::Word,
                value: 0x0000,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x0022,
                size: Size::Word,
                value: 0x2000,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x2000,
                size: Size::Word,
                value: 0x4E71,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x2002,
                size: Size::Word,
                value: 0x4E71,
            },
        ]
    }

    fn assert_privilege_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 0x2000, "pc reloaded at the handler");
        assert_eq!(
            cpu.regs.sr, 0x2004,
            "S set, T clear, CCR preserved (0x0004)"
        );
        assert_eq!(cpu.regs.ssp, 0x0FFA, "SSP pushed down by the 6-byte frame");
        assert_eq!(cpu.regs.usp, 0x0000_3000, "USP untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [0x4E71, 0x4E71],
            "queue reloaded at the handler"
        );
        assert_eq!(bus.peek(0x0FFA), 0x00, "stacked SR high");
        assert_eq!(bus.peek(0x0FFB), 0x04, "stacked SR low = 0x0004");
        assert_eq!(bus.peek(0x0FFC), 0x00, "stacked PC high");
        assert_eq!(
            bus.peek(0x0FFE),
            0x0C,
            "stacked PC = 0x0C00 (the offending instruction)"
        );
        assert_eq!(bus.peek(0x0FFF), 0x00);
        assert_eq!(bus.log, expected_privilege_log());
    }

    #[test]
    fn privilege_violation_traps_user_mode_anditosr() {
        let (mut cpu, mut bus) = setup_privilege_anditosr_user();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 34, "privilege violation = 34(4/3), the TRAP shape");
        assert_privilege_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_privilege_violation() {
        let (mut rtc, mut bus_rtc) = setup_privilege_anditosr_user();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_privilege_anditosr_user();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 34);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_privilege_final(&step, &bus_step);
    }

    #[test]
    fn privilege_violation_quiescable_at_every_boundary() {
        // Reference final state from an uninterrupted run.
        let (mut rref, mut bref) = setup_privilege_anditosr_user();
        rref.run_instruction(&mut bref);

        let cfg = bincode::config::standard();
        // 18 micro-ops (TargetCalc, EnterException, Internal, 6× frame push, 9× vector fetch/reload) →
        // in-flight boundaries after 0..=17 of them. Snapshot/restore the whole CPU at each, resume on
        // the same bus, and require an identical final state + transaction stream.
        for pause_after in 0..=17 {
            let (mut cpu, mut bus) = setup_privilege_anditosr_user();
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

    #[test]
    fn illegal_frame_agrees_across_both_drivers() {
        // The illegal/line-A/line-F recipe is the shared decode_time_exception_recipe (same as privilege,
        // only the vector differs), so the frame machinery is snapshot-tested via the privilege triplet.
        // This pins that decode-emitted ILLEGAL also runs bit-identically on both drivers.
        let mk = || {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0x0000_3000,
                ssp: 0x0000_1000,
                pc: 0x0C00,
                sr: 0x2704,
                prefetch: [0x4AFC, 0x0000], // official ILLEGAL
            };
            let mut bus = FlatBus::new();
            bus.poke(0x12, 0x20); // vector 4 @ 0x10 → handler 0x2000
            for (a, v) in [
                (0x2000u32, 0x4Eu8),
                (0x2001, 0x71),
                (0x2002, 0x4E),
                (0x2003, 0x71),
            ] {
                bus.poke(a, v);
            }
            (Cpu68000::new(regs), bus)
        };
        let (mut rtc, mut bus_rtc) = mk();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = mk();
        step.start_instruction();
        while step.step_micro_op(&mut bus_step) == Step::Continue {}
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_exception_frame(&step, &bus_step, 4);
    }

    #[test]
    fn is_privileged_opcode_classifies_the_mc68000_set() {
        // Positives — the full MC68000 privileged set (M68000UM §6.3.7).
        for op in [
            0x027C, // ANDI #imm,SR
            0x007C, // ORI  #imm,SR
            0x0A7C, // EORI #imm,SR
            0x46C0, // MOVE Dn,SR (a representative EA)
            0x46FC, // MOVE #imm,SR
            0x4E60, // MOVE An,USP (bottom of the MOVE-USP block)
            0x4E68, // MOVE USP,An
            0x4E6F, // MOVE USP,A7 (top of the block)
            0x4E70, // RESET
            0x4E72, // STOP
            0x4E73, // RTE
        ] {
            assert!(is_privileged_opcode(op), "{op:#06X} must be privileged");
        }
        // Near-miss negatives — the ones a one-bit mask slip or a 68010-flavored reference would break,
        // and which the SST gate can NEVER catch (every vendored case is supervisor → the gate never fires).
        for op in [
            0x4E71, // NOP — sits directly between RESET (0x4E70) and STOP (0x4E72)
            0x40C0, // MOVE from SR — UNPRIVILEGED on the 68000 (privileged only from the 68010)
            0x40FC, // MOVE from SR, another EA — the same 68000-vs-68010 pin
            0x4E75, // RTS
            0x4E77, // RTR
            0x4E5F, // UNLK A7 — just below the MOVE-USP block (0x4E60)
            0x0000, // ORI #imm,Dn — the non-SR immediate form
        ] {
            assert!(
                !is_privileged_opcode(op),
                "{op:#06X} must NOT be privileged on the 68000"
            );
        }
    }

    // --- Push A / A2: decode totality — ILLEGAL (v4) / line-A (v10) / line-F (v11) -----------------
    // Hand-authored (no SST data). Pinned to M68000UM Table 6-2 (vectors) + §6.3.6 (illegal/unimplemented)
    // + Yacht L1551 (34(4/3), the TRAP-shape frame). Saved PC = pc+0 (the offending instruction, §6.2.5).
    fn run_decode_exception(opcode: u16) -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000,
            ssp: 0x0000_1000,
            pc: 0x0C00,
            sr: 0x2704, // supervisor (S=1, T=0), Z set — the distinctive captured SR
            prefetch: [opcode, 0x0000],
        };
        let mut bus = FlatBus::new();
        // Vector tables for 4 (0x10), 10 (0x28), 11 (0x2C) → all point at handler 0x0000_2000.
        for base in [0x10u32, 0x28, 0x2C] {
            bus.poke(base + 2, 0x20); // low word = 0x2000; high word stays 0
        }
        for (a, v) in [
            (0x2000u32, 0x4Eu8),
            (0x2001, 0x71),
            (0x2002, 0x4E),
            (0x2003, 0x71),
        ] {
            bus.poke(a, v);
        }
        let mut cpu = Cpu68000::new(regs);
        cpu.run_instruction(&mut bus);
        (cpu, bus)
    }

    fn assert_exception_frame(cpu: &Cpu68000, bus: &FlatBus, vector: u32) {
        assert_eq!(cpu.regs.pc, 0x2000, "reloaded at the handler");
        assert_eq!(
            cpu.regs.sr, 0x2704,
            "S set / T clear preserved (supervisor case)"
        );
        assert_eq!(cpu.regs.ssp, 0x0FFA, "6-byte frame pushed");
        assert_eq!(bus.peek(0x0FFA), 0x27, "stacked SR high");
        assert_eq!(bus.peek(0x0FFB), 0x04, "stacked SR low");
        assert_eq!(bus.peek(0x0FFC), 0x00, "stacked PC high");
        assert_eq!(
            bus.peek(0x0FFE),
            0x0C,
            "stacked PC = 0x0C00 (the offending instruction)"
        );
        assert_eq!(bus.peek(0x0FFF), 0x00);
        let vaddr = vector * 4;
        assert!(
            bus.log
                .iter()
                .any(|t| t.kind == TxKind::Read && t.addr == vaddr && t.fc == 5),
            "vector {vector} fetched at {vaddr:#06X}"
        );
    }

    #[test]
    fn illegal_line_a_line_f_decode_to_their_vectors() {
        let (cpu, bus) = run_decode_exception(0x4AFC); // the official ILLEGAL opcode
        assert_exception_frame(&cpu, &bus, 4);
        let (cpu, bus) = run_decode_exception(0xA000); // line-A (unimplemented, vector 10)
        assert_exception_frame(&cpu, &bus, 10);
        let (cpu, bus) = run_decode_exception(0xF000); // line-F (unimplemented, vector 11)
        assert_exception_frame(&cpu, &bus, 11);
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

    /// The clean SST reference case `4e77 [RTR] 1` (20 cycles) — the F6 anchor. SP (ssp 2048, supervisor)
    /// holds the saved CCR word 0x6FF6 @ SP=2048 and the 32-bit return address big-endian: hi 0xE9DB @ SP+2,
    /// lo 0x0CCE @ SP+4. The data's read order is hi @ SP+2, ccr @ SP, lo @ SP+4 (reproduced exactly). The
    /// recipe restores the low 5 CCR bits (0xF6 → 0x16; SR 0x2715 → 0x2716), post-increments SP by 6 (→ 2054),
    /// assembles the UNMASKED target 0xE9DB0CCE (= 3923446990), sets pc, and reloads the queue at the target —
    /// the bus reload masks to 0xDB0CCE (= 14355662): r@14355662, r@14355664. final.pc is the full unmasked
    /// 0xE9DB0CCE.
    fn setup_rtr() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 2_639_588_438,
            ssp: 2048,
            pc: 3072,
            sr: 10005,                // 0x2715 — CCR = X|Z|C; supervisor
            prefetch: [20087, 62665], // prefetch[0] = 0x4E77 (RTR)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // CCR word @ SP=2048: 0x6FF6.
            (2048u32, 111u8),
            (2049, 246),
            // return address hi 0xE9DB @ SP+2=2050.
            (2050, 233),
            (2051, 219),
            // return address lo 0x0CCE @ SP+4=2052.
            (2052, 12),
            (2053, 206),
            // the two target words at 0xDB0CCE (14355662) / +2: 0x7DD5 = 32213, 0x8F02 = 36610.
            (14_355_662, 125),
            (14_355_663, 213),
            (14_355_664, 143),
            (14_355_665, 2),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_rtr_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2050, // hi word of the return address @ SP+2 (read FIRST)
                size: Size::Word,
                value: 59867, // 0xE9DB
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2048, // the saved CCR word @ SP (read SECOND)
                size: Size::Word,
                value: 28662, // 0x6FF6
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2052, // lo word @ SP+4 (read THIRD)
                size: Size::Word,
                value: 3278, // 0x0CCE
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 14_355_662, // target (masked) — read into prefetch[0]
                size: Size::Word,
                value: 32213,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 14_355_664, // target+2 (masked) — read into prefetch[1]
                size: Size::Word,
                value: 36610,
            },
        ]
    }

    fn assert_rtr_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3_923_446_990,
            "pc landed at the FULL unmasked popped target 0xE9DB0CCE"
        );
        assert_eq!(
            cpu.regs.ssp, 2054,
            "SP post-incremented by 6 (CCR + long pop)"
        );
        assert_eq!(
            cpu.regs.sr, 10006,
            "CCR restored from the popped low 5 bits (0xF6 → 0x16); system byte preserved"
        );
        assert_eq!(cpu.regs.usp, 2_639_588_438, "usp untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [32213, 36610],
            "queue reloaded at the (masked) target"
        );
        assert_eq!(bus.log, expected_rtr_log());
    }

    #[test]
    fn run_instruction_matches_rtr() {
        let (mut cpu, mut bus) = setup_rtr();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "RTR = [EaCalc, Read.hi, Read.ccr, EaCalc, Read.lo, LoadCcr, AdjustAddr, Combine32, SetPc, PF, PF] = 5 word reads = 20"
        );
        assert_rtr_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_rtr() {
        let (mut rtc, mut bus_rtc) = setup_rtr();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_rtr();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_rtr_final(&step, &bus_step);
    }

    #[test]
    fn rtr_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the RTR pop shape — the CCR pop + the reordered long pop (three stack
        // reads + Combine32), the LoadCcr, the SP +6, and the SetPc + two-Prefetch reload — the whole CPU
        // (incl. the in-flight cursor and its scratch slots) round-trips at every micro-op boundary.
        let (mut rref, mut bref) = setup_rtr();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 11 micro-ops (EaCalc, Read, Read, EaCalc, Read, LoadCcr, AdjustAddr, Combine32, SetPc, PF, PF) → 0..=10.
        for pause_after in 0..=10 {
            let (mut cpu, mut bus) = setup_rtr();
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

    // --- F5: DBcc — decode-time condition + counter resolution. Two anchors: cond-true fall-through (5dcd,
    // 12 cyc, NO decrement) and cond-false-taken (59c8, 10 cyc, decrement Dn.w + branch). The cond-false
    // counter-expired path is correctness-only (absent from the SST data) — a focused decode test below. ---

    #[test]
    fn dbcc_decode_extracts_cc_and_reg() {
        // DBcc layout: `0101 cccc 11001 rrr` — opcode & 0xF0F8 == 0x50C8 (mode 001, size field 11). cc = bits
        // 11-8, reg = bits 2-0; the displacement is always the extension word (no byte form — the opcode low
        // byte is 0xC8 | reg, never 0). 0x59c8 → cc 9 (VS), reg 0. 0x5dcd → cc 13 (LT), reg 5.
        assert_eq!(0x59c8u16 & 0xF0F8, 0x50C8, "0x59c8 is a DBcc");
        assert_eq!((0x59c8u16 >> 8) & 0xF, 9, "0x59c8 cc = 9 (VS)");
        assert_eq!(0x59c8u16 & 7, 0, "0x59c8 reg = 0 (D0)");
        assert_eq!(0x5dcdu16 & 0xF0F8, 0x50C8, "0x5dcd is a DBcc");
        assert_eq!((0x5dcdu16 >> 8) & 0xF, 13, "0x5dcd cc = 13 (LT)");
        assert_eq!(0x5dcdu16 & 7, 5, "0x5dcd reg = 5 (D5)");
        // A different mode (000, Dn-direct) is Scc, NOT DBcc — its low six bits differ from 11001 rrr.
        assert_ne!(
            0x50C0u16 & 0xF0F8,
            0x50C8,
            "0x50C0 (Scc, mode 000) is NOT a DBcc"
        );
    }

    /// The clean SST reference case `5dcd [DBcc D5, #] 2` (condition true → loop terminates, fall through, 12
    /// cycles, NO decrement) — the F5 cond-true anchor. opcode 0x5dcd → cc 13 (LT = N != V); SR 0x2716 (V|Z
    /// set, N clear) → N != V → LT true → terminate. The recipe is `[Internal(4), Prefetch, Prefetch]`: pc
    /// advances TWO words (+4, skipping the displacement word), D5 is unchanged, the queue reloads at pc+4 /
    /// pc+6. Bus: n4 (not in the stream) + two FC-6 reads at 3076 / 3078.
    fn setup_dbcc_cond_true() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 10006,               // 0x2716 — V|Z set, N clear → LT true (terminate)
            prefetch: [24013, 3000], // prefetch[0] = 0x5DCD (DBcc D5)
        };
        regs.d[5] = 0x8A57_C8BD; // counter; must be UNCHANGED (cond true → no decrement)
        let mut bus = FlatBus::new();
        // The two fall-through words at pc+4 (3076 = 0xCD55 = 52565) / pc+6 (3078 = 0xBA9D = 47773).
        for (a, v) in [(3076u32, 0xCDu8), (3077, 0x55), (3078, 0xBA), (3079, 0x9D)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_dbcc_cond_true_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 52565,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3078,
                size: Size::Word,
                value: 47773,
            },
        ]
    }

    fn assert_dbcc_cond_true_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3076, "pc advanced two words (fall-through)");
        assert_eq!(cpu.regs.sr, 10006, "SR unchanged (DBcc affects no flags)");
        assert_eq!(
            cpu.regs.d[5], 0x8A57_C8BD,
            "D5 unchanged — cond true terminates with NO decrement"
        );
        assert_eq!(cpu.regs.prefetch, [52565, 47773], "queue shifted twice");
        assert_eq!(bus.log, expected_dbcc_cond_true_log());
    }

    #[test]
    fn run_instruction_matches_dbcc_cond_true() {
        let (mut cpu, mut bus) = setup_dbcc_cond_true();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 12, "cond true = [Internal(4), PF, PF] = 12");
        assert_dbcc_cond_true_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_dbcc_cond_true() {
        let (mut rtc, mut bus_rtc) = setup_dbcc_cond_true();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_dbcc_cond_true();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_dbcc_cond_true_final(&step, &bus_step);
    }

    #[test]
    fn dbcc_cond_true_quiescable_and_serializable_at_every_micro_op_boundary() {
        let (mut rref, mut bref) = setup_dbcc_cond_true();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Internal(4), Prefetch, Prefetch) → boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_dbcc_cond_true();
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

    /// The clean SST reference case `59c8 [DBcc D0, #] 1` (condition false, counter live → decrement + branch
    /// taken, 10 cycles) — the F5 cond-false-taken anchor. opcode 0x59c8 → cc 9 (VS = V set); SR 0x271C (V
    /// clear) → VS false → do NOT terminate. D0 0x2602_5C43 (low word 0x5C43 != 0 → counter live): decrement to
    /// 0x2602_5C42, then branch. target = pc+2+sign_extend16(0xF002) = 3074 + (−4094) = 0xFFFF_FC04 (UNMASKED);
    /// the bus reload masks to 0xFFFC04 (= 16776196). Recipe `[DecrementDnWord, TargetCalc(DispWord),
    /// Internal(2), SetPc, Prefetch, Prefetch]`. Bus: n2 (not in the stream) + two FC-6 reads at 16776196 /
    /// 16776198; final.pc is the full unmasked 0xFFFF_FC04 (the high bits survive — DBcc shares the unmasked
    /// taken-branch tail).
    fn setup_dbcc_cond_false_taken() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 10012,                // 0x271C — V clear → VS false (do not terminate)
            prefetch: [22984, 61442], // prefetch[0] = 0x59C8 (DBcc D0); prefetch[1] = 0xF002 (disp)
        };
        regs.d[0] = 0x2602_5C43; // counter; low word 0x5C43 != 0 → live, decremented to 0x5C42
        let mut bus = FlatBus::new();
        // The two target words at the MASKED target 0xFFFC04 (16776196 = 0x9392 = 37778) / +2 (0xE856 = 59478).
        for (a, v) in [
            (16776196u32, 0x93u8),
            (16776197, 0x92),
            (16776198, 0xE8),
            (16776199, 0x56),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_dbcc_cond_false_taken_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16776196, // target (masked) → prefetch[0]
                size: Size::Word,
                value: 37778,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 16776198, // target+2 (masked) → prefetch[1]
                size: Size::Word,
                value: 59478,
            },
        ]
    }

    fn assert_dbcc_cond_false_taken_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 4294966276,
            "pc landed at the FULL unmasked target 0xFFFF_FC04"
        );
        assert_eq!(cpu.regs.sr, 10012, "SR unchanged (DBcc affects no flags)");
        assert_eq!(
            cpu.regs.d[0], 0x2602_5C42,
            "D0 low word decremented (high word preserved)"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [37778, 59478],
            "queue reloaded at the (masked) target"
        );
        assert_eq!(bus.log, expected_dbcc_cond_false_taken_log());
    }

    #[test]
    fn run_instruction_matches_dbcc_cond_false_taken() {
        let (mut cpu, mut bus) = setup_dbcc_cond_false_taken();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 10,
            "cond false taken = [DecrementDnWord, TargetCalc, Internal(2), SetPc, PF, PF] = 10"
        );
        assert_dbcc_cond_false_taken_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_dbcc_cond_false_taken() {
        let (mut rtc, mut bus_rtc) = setup_dbcc_cond_false_taken();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_dbcc_cond_false_taken();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_dbcc_cond_false_taken_final(&step, &bus_step);
    }

    #[test]
    fn dbcc_cond_false_taken_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the DBcc decrement-and-branch shape (the DecrementDnWord counter +
        // the SetPc + queue-reload tail) — the whole CPU round-trips at every micro-op boundary.
        let (mut rref, mut bref) = setup_dbcc_cond_false_taken();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 6 micro-ops (DecrementDnWord, TargetCalc, Internal(2), SetPc, Prefetch, Prefetch) → 0..=5.
        for pause_after in 0..=5 {
            let (mut cpu, mut bus) = setup_dbcc_cond_false_taken();
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

    #[test]
    fn dbcc_cond_false_expired_decrements_then_falls_through() {
        // The cond-false counter-EXPIRED path (Dn.w == 0): decrement to 0xFFFF (the −1 terminator), then fall
        // through (+4) — NO branch. This is correctness-only — it is ABSENT from the SST data (a random Dn.w is
        // 0 only ≈1/65536 of the time, and the vendored DBcc file has no expired bucket), so it is pinned by a
        // focused synthetic case here, not a vendored anchor. cc 9 (VS); SR 0x2700 (V clear) → not terminate.
        let regs = Registers {
            d: [0; 8], // D0 low word == 0 → the decrement expires the loop
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2700,
            prefetch: [0x59C8, 0x1234], // DBcc D0; disp 0x1234 (the branch is NOT taken, so disp is unused)
        };
        let mut bus = FlatBus::new();
        // The two fall-through words at pc+4 (3076 = 0x1111) / pc+6 (3078 = 0x2222).
        for (a, v) in [(3076u32, 0x11u8), (3077, 0x11), (3078, 0x22), (3079, 0x22)] {
            bus.poke(a, v);
        }
        let mut cpu = Cpu68000::new(regs);
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 14,
            "cond false expired = decrement + fall-through (+4) = 14"
        );
        assert_eq!(
            cpu.regs.pc, 3076,
            "pc advanced two words (fall-through, NO branch)"
        );
        assert_eq!(
            cpu.regs.d[0], 0x0000_FFFF,
            "D0 low word 0 → 0xFFFF (the −1 terminator); high word preserved"
        );
        assert_eq!(cpu.regs.prefetch, [0x1111, 0x2222], "queue shifted twice");
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 3076,
                    size: Size::Word,
                    value: 0x1111,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 3078,
                    size: Size::Word,
                    value: 0x2222,
                },
            ],
            "fall-through reads at pc+4 / pc+6 (no branch reload)"
        );
    }

    // --- TRAP (the standard 6-byte exception entry) — the F0 anchor `4e40 [TRAP Q] 7` (vector 32, len 34),
    // plus both-drivers agreement and a snapshot/restore anchor ACROSS the whole entry. ---

    /// The clean SST reference case `4e40 [TRAP Q] 7` (34 cycles): TRAP #0 → vector 32 (address 128). In
    /// supervisor mode (S=1, T=0), ssp 2048, pc 3072, sr 9991. The handler vector longword @128 is
    /// 0x00008800 (= 34816): hi 0x0000 @128, lo 0x8800 @130. The handler code is 0xFBA9 @34816, 0x8AED @34818.
    /// The entry saves PC = pc+2 = 3074 and SR = 9991 to ssp−6 = 2042 (PCL @2046, SR @2042, PCH @2044), reads
    /// the vector (FC=5), and reloads the queue at 34816 (FC=6). Final: ssp 2042, pc 34816, sr 9991 (S already
    /// set / T already clear → the S/T transform is a no-op on the data), prefetch [0xFBA9, 0x8AED].
    fn setup_trap() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [348_553_026, 50_768_304, 0, 0, 0, 0, 0, 0],
            a: [488_340_641, 1_309_528_859, 0, 0, 0, 0, 0],
            usp: 2_525_304_260,
            ssp: 2048,
            pc: 3072,
            sr: 9991,
            prefetch: [20032, 14968], // prefetch[0] = 0x4E40 (TRAP #0)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The exception vector @128: 0x00008800 (hi 0x0000 @128, lo 0x8800 @130).
            (128u32, 0u8),
            (129, 0),
            (130, 136),
            (131, 0),
            // The handler code @34816: 0xFBA9, 0x8AED.
            (34816, 251),
            (34817, 169),
            (34818, 138),
            (34819, 237),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_trap_log() -> Vec<Transaction> {
        vec![
            // The standard frame, on-bus order PCL @ B+4, SR @ B+0, PCH @ B+2 (FC=5).
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046, // PCL @ ssp−6+4
                size: Size::Word,
                value: 3074, // pc + 2
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2042, // SR @ ssp−6
                size: Size::Word,
                value: 9991,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044, // PCH @ ssp−6+2
                size: Size::Word,
                value: 0,
            },
            // The vector fetch (FC=5 supervisor-data).
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 128, // vector address (32*4)
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 130, // vector address + 2
                size: Size::Word,
                value: 34816,
            },
            // The handler reload (FC=6 supervisor-program), with the n2 idle between (not in the stream).
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 34816,
                size: Size::Word,
                value: 64425, // 0xFBA9
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 34818,
                size: Size::Word,
                value: 35565, // 0x8AED
            },
        ]
    }

    fn assert_trap_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.ssp, 2042,
            "SP pushed down by 6 (the standard frame)"
        );
        assert_eq!(
            cpu.regs.pc, 34816,
            "pc landed at the handler (vector target)"
        );
        assert_eq!(
            cpu.regs.sr, 9991,
            "SR unchanged (already S=1/T=0 — the transform is a no-op on the data)"
        );
        assert_eq!(cpu.regs.usp, 2_525_304_260, "usp untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [64425, 35565],
            "queue reloaded at the handler"
        );
        // The pushed frame on the supervisor stack (big-endian words).
        assert_eq!(bus.peek(2042), 0x27, "SR hi @ B+0");
        assert_eq!(bus.peek(2043), 0x07, "SR lo @ B+1");
        assert_eq!(bus.peek(2044), 0x00, "PCH hi @ B+2");
        assert_eq!(bus.peek(2045), 0x00, "PCH lo @ B+3");
        assert_eq!(bus.peek(2046), 0x0C, "PCL hi @ B+4");
        assert_eq!(bus.peek(2047), 0x02, "PCL lo @ B+5");
        assert_eq!(bus.log, expected_trap_log());
    }

    #[test]
    fn run_instruction_matches_trap() {
        let (mut cpu, mut bus) = setup_trap();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 34,
            "TRAP = n4 + 3 frame writes + 2 vector reads + 2 handler reloads + n2 = 4+12+8+8+2 = 34"
        );
        assert_trap_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_trap() {
        let (mut rtc, mut bus_rtc) = setup_trap();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_trap();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 34);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_trap_final(&step, &bus_step);
    }

    #[test]
    fn trap_quiescable_and_serializable_across_the_entry() {
        // The snapshot/restore anchor ACROSS a TRAP entry — the supervisor-entry transform, the standard
        // frame push (AdjustAddr SP−6, the three reordered writes), the FC=5 vector fetch, and the FC=6
        // handler reload — the whole CPU (incl. the in-flight cursor and its scratch slots: the saved PC/SR,
        // the frame addresses, the vector address, the assembled handler) round-trips at every micro-op
        // boundary.
        let (mut rref, mut bref) = setup_trap();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 18 micro-ops (TargetCalc, EnterException, Internal, AdjustAddr, EaCalc, Write, Write, EaCalc, Write,
        // LoadImm, Read, EaCalc, Read, Combine32, SetPc, Prefetch, Internal, Prefetch) → 0..=17.
        for pause_after in 0..=17 {
            let (mut cpu, mut bus) = setup_trap();
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

    // --- RTE (return from exception — the inverse of the standard frame push) — the anchors
    // `4e73 [RTE] 1` (→user, FC2 reload, len 20) and `4e73 [RTE] 6` (→supervisor, FC6 reload, len 20), plus
    // both-drivers agreement and a snapshot/restore anchor ACROSS the whole return. ---

    /// The clean SST reference case `4e73 [RTE] 1` (20 cycles): the restored SR clears S → **user** mode, so
    /// the handler reload runs under FC2 (user-program). Supervisor entry state ssp 2048, pc 3072, sr 9989
    /// (0x2705, S=1/T=0). The 6-byte frame on the stack is SR @ 2048 = 0xD6ED, PC-hi @ 2050 = 0xE694, PC-lo @
    /// 2052 = 0x8C98 → popped PC 0xE6948C98 (= 3868495000), restored SR 0xD6ED & 0xA71F = 0x860D (= 34317,
    /// S cleared). The +6 pop hits ssp (2054) while still supervisor; the queue reloads at 0x948C98 (the
    /// masked handler) under FC2.
    fn setup_rte_user() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [2_541_146_255, 0, 0, 0, 0, 0, 0, 0],
            a: [4_062_594_777, 0, 0, 0, 0, 0, 0],
            usp: 2_828_419_046,
            ssp: 2048,
            pc: 3072,
            sr: 9989,                 // 0x2705, S=1, T=0
            prefetch: [20083, 39590], // prefetch[0] = 0x4E73 (RTE)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The 6-byte frame: SR @ 2048, PC-hi @ 2050, PC-lo @ 2052 (big-endian words).
            (2048u32, 214u8), // SR hi (0xD6)
            (2049, 237),      // SR lo (0xED)
            (2050, 230),      // PC-hi hi (0xE6)
            (2051, 148),      // PC-hi lo (0x94)
            (2052, 140),      // PC-lo hi (0x8C)
            (2053, 152),      // PC-lo lo (0x98)
            // The handler code @ 0x948C98 (= 9735320), reloaded under FC2 (restored SR clears S → user).
            (9_735_320, 66),
            (9_735_321, 227),
            (9_735_322, 28),
            (9_735_323, 16),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_rte_user_log() -> Vec<Transaction> {
        vec![
            // The frame pop — the data's read order: PC-hi @ SP+2, SR @ SP, PC-lo @ SP+4 (FC=5).
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2050,
                size: Size::Word,
                value: 59028, // PC-hi (0xE694)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2048,
                size: Size::Word,
                value: 55021, // SR (0xD6ED)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2052,
                size: Size::Word,
                value: 35992, // PC-lo (0x8C98)
            },
            // The handler reload — FC=2 (user-program) because the restored SR cleared S.
            Transaction {
                kind: TxKind::Read,
                fc: 2,
                addr: 9_735_320,
                size: Size::Word,
                value: 17123,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 2,
                addr: 9_735_322,
                size: Size::Word,
                value: 7184,
            },
        ]
    }

    fn assert_rte_user_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.ssp, 2054,
            "SP popped up by 6 (the frame), while still supervisor"
        );
        assert_eq!(
            cpu.regs.pc, 3_868_495_000,
            "pc landed at the popped return address (0xE6948C98)"
        );
        assert_eq!(
            cpu.regs.sr, 34317,
            "SR restored, masked 0xA71F (0x860D) — S cleared → user mode"
        );
        assert_eq!(
            cpu.regs.usp, 2_828_419_046,
            "usp untouched (the +6 hit ssp while still supervisor)"
        );
        assert_eq!(cpu.regs.d[0], 2_541_146_255, "d0 untouched");
        assert_eq!(cpu.regs.a[0], 4_062_594_777, "a0 untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [17123, 7184],
            "queue reloaded at the handler (FC2 user-program)"
        );
        assert_eq!(bus.log, expected_rte_user_log());
    }

    /// The clean SST reference case `4e73 [RTE] 6` (20 cycles): the restored SR keeps S set → **supervisor**
    /// mode, so the handler reload runs under FC6 (supervisor-program). Entry ssp 2048, pc 3072, sr 9995. The
    /// frame: SR @ 2048 = 0x73DA, PC-hi @ 2050 = 0x17AC, PC-lo @ 2052 = 0x1998 → popped PC 0x17AC1998 (=
    /// 397154712), restored SR 0x73DA & 0xA71F = 0x231A (= 8986, S still set). The queue reloads at 0xAC1998
    /// (the masked handler) under FC6.
    fn setup_rte_super() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [1_854_266_780, 0, 0, 0, 0, 0, 0, 0],
            a: [2_020_886_912, 0, 0, 0, 0, 0, 0],
            usp: 256_682_410,
            ssp: 2048,
            pc: 3072,
            sr: 9995,                 // 0x270B, S=1, T=0
            prefetch: [20083, 46782], // prefetch[0] = 0x4E73 (RTE)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The 6-byte frame: SR @ 2048, PC-hi @ 2050, PC-lo @ 2052.
            (2048u32, 115u8), // SR hi (0x73)
            (2049, 218),      // SR lo (0xDA)
            (2050, 23),       // PC-hi hi (0x17)
            (2051, 172),      // PC-hi lo (0xAC)
            (2052, 25),       // PC-lo hi (0x19)
            (2053, 152),      // PC-lo lo (0x98)
            // The handler code @ 0xAC1998 (= 11278744), reloaded under FC6 (restored SR keeps S → supervisor).
            (11_278_744, 72),
            (11_278_745, 220),
            (11_278_746, 71),
            (11_278_747, 172),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_rte_super_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2050,
                size: Size::Word,
                value: 6060, // PC-hi (0x17AC)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2048,
                size: Size::Word,
                value: 29658, // SR (0x73DA)
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2052,
                size: Size::Word,
                value: 6552, // PC-lo (0x1998)
            },
            // The handler reload — FC=6 (supervisor-program) because the restored SR keeps S.
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 11_278_744,
                size: Size::Word,
                value: 18652,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 11_278_746,
                size: Size::Word,
                value: 18348,
            },
        ]
    }

    fn assert_rte_super_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.ssp, 2054, "SP popped up by 6 (the frame)");
        assert_eq!(
            cpu.regs.pc, 397_154_712,
            "pc landed at the popped return address (0x17AC1998)"
        );
        assert_eq!(
            cpu.regs.sr, 8986,
            "SR restored, masked 0xA71F (0x231A) — S still set → supervisor"
        );
        assert_eq!(cpu.regs.usp, 256_682_410, "usp untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [18652, 18348],
            "queue reloaded at the handler (FC6 supervisor-program)"
        );
        assert_eq!(bus.log, expected_rte_super_log());
    }

    #[test]
    fn run_instruction_matches_rte_user() {
        let (mut cpu, mut bus) = setup_rte_user();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "RTE = 3 frame reads + 2 handler reloads = 5 word reads = 20 cyc (no idle)"
        );
        assert_rte_user_final(&cpu, &bus);
    }

    #[test]
    fn run_instruction_matches_rte_super() {
        let (mut cpu, mut bus) = setup_rte_super();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 20);
        assert_rte_super_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_rte_user() {
        let (mut rtc, mut bus_rtc) = setup_rte_user();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_rte_user();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_rte_user_final(&step, &bus_step);
    }

    #[test]
    fn both_drivers_match_rte_super() {
        let (mut rtc, mut bus_rtc) = setup_rte_super();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_rte_super();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_rte_super_final(&step, &bus_step);
    }

    #[test]
    fn rte_quiescable_and_serializable_across_the_return() {
        // The snapshot/restore anchor ACROSS an RTE return — the frame pop (the three FC=5 reads in the data's
        // order), the +6 stack adjust (while still supervisor), the full-SR restore (LoadSr, here SWITCHING to
        // user mode), and the FC2 handler reload — the whole CPU (incl. the in-flight cursor and its scratch
        // slots: the popped PC halves, the SR, the transient addresses, the assembled target) round-trips at
        // every micro-op boundary. The user-mode case is used because the mode switch is the new behaviour.
        let (mut rref, mut bref) = setup_rte_user();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 11 micro-ops (EaCalc, Read, Read, EaCalc, Read, Combine32, AdjustAddr, LoadSr, SetPc, Prefetch,
        // Prefetch) → boundaries after 0..=10.
        for pause_after in 0..=10 {
            let (mut cpu, mut bus) = setup_rte_user();
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

    // --- TRAPV (conditional trap on the V flag, resolved at decode time) — the anchors `4e76 [TRAPV] 1`
    // (V=0, no trap, len 4) and `4e76 [TRAPV] 3` (V=1, trap to vector 7, len 34), plus both-drivers agreement
    // and a snapshot/restore anchor ACROSS the trap entry. ---

    /// The clean SST reference case `4e76 [TRAPV] 1` (4 cycles): V clear → NO trap. A single prefetch refills
    /// the queue (FC=6 @ pc+4 = 3076, value 6464) and advances pc by 2 (3072 → 3074); nothing else changes.
    fn setup_trapv_no_trap() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                802_263_096,
                614_018_252,
                3_770_375_450,
                2_805_376_503,
                2_026_424_402,
                729_447_178,
                767_710_245,
                2_715_703_449,
            ],
            a: [
                487_787_210,
                1_869_842_941,
                3_804_173_515,
                448_313_786,
                1_367_352_243,
                851_232_805,
                1_911_140_584,
            ],
            usp: 1_536_473_616,
            ssp: 2048,
            pc: 3072,
            sr: 9993,                 // 0x2709 — V clear (no trap), S=1
            prefetch: [20086, 21430], // prefetch[0] = 0x4E76 (TRAPV)
        };
        let mut bus = FlatBus::new();
        // The refill word @ pc+4 = 3076: 0x1940 = 6464 (hi 25 @3076, lo 64 @3077).
        for (a, v) in [(3076u32, 25u8), (3077, 64)] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn assert_trapv_no_trap_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced by 2 (no branch)");
        assert_eq!(cpu.regs.ssp, 2048, "SP untouched (no frame pushed)");
        assert_eq!(cpu.regs.sr, 9993, "SR unchanged (no trap)");
        assert_eq!(
            cpu.regs.prefetch,
            [21430, 6464],
            "queue shifted, refilled @ pc+4"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 6464,
            }],
            "no-trap = a single FC6 refill @ pc+4"
        );
    }

    /// The clean SST reference case `4e76 [TRAPV] 3` (34 cycles): V set → trap to vector 7 (address 0x1C = 28).
    /// In supervisor mode (S=1, T=0), ssp 2048, pc 3072, sr 10014 (0x271E). UNLIKE TRAP, TRAPV's first bus
    /// event is a LEADING prefetch (FC=6 @ pc+4 = 3076, value 45072 — discarded by the handler reload), not the
    /// PCL write. The entry then saves PC = pc+2 = 3074 (captured BEFORE the leading prefetch) and SR = 10014 to
    /// ssp−6 = 2042 (PCL @2046, SR @2042, PCH @2044, FC=5), reads the vector @28/30 (FC=5 → handler 9216), and
    /// reloads the queue at 9216 (FC=6): 0x0BF6 (3062) @9216, 0xC6F4 (50932) @9218.
    fn setup_trapv_trap() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                1_546_806_632,
                3_910_088_832,
                1_331_578_002,
                1_456_303_585,
                3_903_322_788,
                1_705_068_337,
                2_151_853_803,
                414_224_066,
            ],
            a: [
                2_706_381_019,
                3_545_637_721,
                4_046_269_578,
                962_156_790,
                3_948_413_531,
                798_812_306,
                2_612_441_334,
            ],
            usp: 2_171_723_396,
            ssp: 2048,
            pc: 3072,
            sr: 10014,                // 0x271E — V set (trap), S=1/T=0
            prefetch: [20086, 41658], // prefetch[0] = 0x4E76 (TRAPV)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The leading-prefetch word @ pc+4 = 3076: 0xB010 = 45072 (discarded by the handler reload).
            (3076u32, 176u8),
            (3077, 16),
            // The exception vector @28 (vector 7 = 0x1C): 0x00002400 (hi 0x0000 @28, lo 0x2400 = 9216 @30).
            (28, 0),
            (29, 0),
            (30, 36),
            (31, 0),
            // The handler code @9216: 0x0BF6 (3062), 0xC6F4 (50932).
            (9216, 11),
            (9217, 246),
            (9218, 198),
            (9219, 244),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_trapv_trap_log() -> Vec<Transaction> {
        vec![
            // The LEADING prefetch (FC=6 supervisor-program @ pc+4) — distinguishes TRAPV from TRAP.
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 45072,
            },
            // The standard frame, on-bus order PCL @ B+4, SR @ B+0, PCH @ B+2 (FC=5).
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046, // PCL @ ssp−6+4
                size: Size::Word,
                value: 3074, // pc + 2
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2042, // SR @ ssp−6
                size: Size::Word,
                value: 10014,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044, // PCH @ ssp−6+2
                size: Size::Word,
                value: 0,
            },
            // The vector fetch (FC=5 supervisor-data) — vector 7 @ 28/30.
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 28,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 30,
                size: Size::Word,
                value: 9216,
            },
            // The handler reload (FC=6 supervisor-program), with the n2 idle between (not in the stream).
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 9216,
                size: Size::Word,
                value: 3062,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 9218,
                size: Size::Word,
                value: 50932,
            },
        ]
    }

    fn assert_trapv_trap_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.ssp, 2042, "SP pushed down by 6 (standard frame)");
        assert_eq!(
            cpu.regs.pc, 9216,
            "pc landed at the handler (vector 7 target)"
        );
        assert_eq!(
            cpu.regs.sr, 10014,
            "SR unchanged (already S=1/T=0 — the transform is a no-op on the data)"
        );
        assert_eq!(cpu.regs.usp, 2_171_723_396, "usp untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [3062, 50932],
            "queue reloaded at the handler"
        );
        // The pushed frame on the supervisor stack (big-endian words).
        assert_eq!(bus.peek(2042), 0x27, "SR hi @ B+0");
        assert_eq!(bus.peek(2043), 0x1E, "SR lo @ B+1");
        assert_eq!(bus.peek(2044), 0x00, "PCH hi @ B+2");
        assert_eq!(bus.peek(2045), 0x00, "PCH lo @ B+3");
        assert_eq!(bus.peek(2046), 0x0C, "PCL hi @ B+4");
        assert_eq!(bus.peek(2047), 0x02, "PCL lo @ B+5");
        assert_eq!(bus.log, expected_trapv_trap_log());
    }

    #[test]
    fn run_instruction_matches_trapv_no_trap() {
        let (mut cpu, mut bus) = setup_trapv_no_trap();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 4, "TRAPV (V=0) = one prefetch = 4 cyc");
        assert_trapv_no_trap_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_trapv_no_trap() {
        let (mut rtc, mut bus_rtc) = setup_trapv_no_trap();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_trapv_no_trap();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 4);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_trapv_no_trap_final(&step, &bus_step);
    }

    #[test]
    fn run_instruction_matches_trapv_trap() {
        let (mut cpu, mut bus) = setup_trapv_trap();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 34,
            "TRAPV (V=1) = leading prefetch + 3 frame writes + 2 vector reads + 2 handler reloads + n2 = \
             4+12+8+8+2 = 34"
        );
        assert_trapv_trap_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_trapv_trap() {
        let (mut rtc, mut bus_rtc) = setup_trapv_trap();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_trapv_trap();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 34);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_trapv_trap_final(&step, &bus_step);
    }

    #[test]
    fn trapv_quiescable_and_serializable_across_the_entry() {
        // The snapshot/restore anchor ACROSS a TRAPV trap entry — the LEADING prefetch, the supervisor-entry
        // transform, the standard frame push (the three reordered writes), the FC=5 vector fetch, and the FC=6
        // handler reload — the whole CPU (incl. the in-flight cursor and its scratch slots: the saved PC/SR,
        // the frame addresses, the vector address, the assembled handler) round-trips at every micro-op
        // boundary.
        let (mut rref, mut bref) = setup_trapv_trap();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 18 micro-ops (TargetCalc, EnterException, Prefetch, AdjustAddr, EaCalc, Write, Write, EaCalc, Write,
        // LoadImm, Read, EaCalc, Read, Combine32, SetPc, Prefetch, Internal, Prefetch) → 0..=17.
        for pause_after in 0..=17 {
            let (mut cpu, mut bus) = setup_trapv_trap();
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

    // --- E3: the execution-time address-error abort + the group-0 14-byte frame (Shape B). The abort is
    // proven end-to-end on the SST anchors in the runner (`address_error_anchors_match_singlesteptests`);
    // here we pin the snapshot/restore-ACROSS-the-abort anchor on `d850`. ---

    /// The SST anchor `d850 [ADD.w (A0),D4] 32` (len 50): `A0 = 0x162C374D` is ODD, so the operand-read
    /// word-faults and the in-flight `MicroState` is rewritten into the group-0 14-byte frame to vector 3
    /// (@`0x0C`). All supervisor (S=1, T=0): the frame stacks `PC = 3072` (live `regs.pc`, no prefetch ran),
    /// `SR = 0x2717`, `IR = 0xD850`, `SSW = 0xD855`, and the full 32-bit access address `0x162C374D`. The
    /// vector @`0x0C` = `0x00001400`; the handler code @5120 = `0x40CF, 0x74A8`.
    fn setup_addr_error_d850() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                896_942_068,
                985_548_997,
                317_845_901,
                274_804_678,
                523_604_791,
                3_609_650,
                312_302_176,
                2_521_981_339,
            ],
            a: [
                371_996_493,
                3_417_051_206,
                2_743_599_534,
                2_239_672_972,
                3_167_642_783,
                1_494_966_947,
                378_450_206,
            ],
            usp: 2_917_056_052,
            ssp: 2048,
            pc: 3072,
            sr: 10007,
            prefetch: [55376, 55131], // prefetch[0] = 0xD850 (ADD.w (A0),D4)
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The vector-3 longword @0x0C: 0x00001400 (hi 0x0000 @12, lo 0x1400 @14).
            (12u32, 0u8),
            (13, 0),
            (14, 20),
            (15, 0),
            // The handler code @5120: 0x40CF, 0x74A8.
            (5120, 64),
            (5121, 207),
            (5122, 116),
            (5123, 168),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn address_error_quiescable_and_serializable_across_the_abort() {
        // The snapshot/restore anchor ACROSS the execution-time address-error abort: quiesce at the faulting
        // micro-op AND at every boundary of the installed 14-byte frame, snapshot the WHOLE CPU (the
        // in-flight cursor — the original recipe before the fault, then the rewritten frame recipe + seeded
        // scratch after it), restore, resume, and get an identical result. This proves the in-place
        // `MicroState` rewrite is serializable across the abort (the one real interpreter change).
        let (mut rref, mut bref) = setup_addr_error_d850();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // The faulting `Read` (step 0) rewrites the cursor into the 19-op frame in place, so the run is
        // 1 (the abort, returning Continue) + 19 frame ops = 20 `step_micro_op` calls (19 Continue + 1
        // Done). Snapshot after 0..=19 Continue boundaries — spanning the pre-fault boundary, the fault
        // itself, and the whole installed frame.
        for pause_after in 0..=19 {
            let (mut cpu, mut bus) = setup_addr_error_d850();
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
        // Pin the abort's final state to the SST anchor (the frame was produced end-to-end).
        assert_eq!(rref.regs.pc, 5120, "pc landed at the vector-3 handler");
        assert_eq!(
            rref.regs.ssp, 2034,
            "SSP pushed down by 14 (the group-0 frame)"
        );
        assert_eq!(
            rref.regs.sr, 10007,
            "SR unchanged (already S=1/T=0 — the entry transform is a no-op on the data)"
        );
        assert_eq!(
            rref.regs.prefetch,
            [16591, 29864],
            "queue reloaded at the handler"
        );
    }

    // --- CHK <ea>,Dn (bounds-check trap to vector 6) — the anchors `4190` (no-trap, len 14), `4d91` (Dn<0
    // trap, n6, len 44), `4396` (Dn>bound trap, n4, len 42) and `45bc` (#imm trap, decode-time bound, n4, len
    // 42), plus both-drivers agreement and a snapshot/restore anchor ACROSS the memory-source trap. The CHK
    // trap reuses the Shape-B execution-time abort: ChkTrap rewrites the in-flight MicroState into the standard
    // 6-byte frame. ---

    /// The clean SST reference case `4190 [CHK (A0),D0] 210` (14 cycles): `D0.w = 0x02B9` (697) is in `[0,
    /// bound]` (`bound = 0x1F86` = 8070, read from `(A0)`), so NO trap. The recipe is `[Read bound @ (A0) FC5,
    /// Prefetch, ChkTrap (no-trap), Internal(6)]`: the FC5 bound read, the FC6 queue refill @ pc+4, then the
    /// `n6` no-trap tail. N preserved (`D0 ≥ 0` and `≤ bound`), Z/V/C cleared, X kept → SR unchanged (0x2700).
    fn setup_chk_no_trap() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x589e_02b9,
                0x47e8_7091,
                0xbcc1_8103,
                0x474b_45ae,
                0xf8de_da8b,
                0x5893_2dbb,
                0xb6b2_90b7,
                0xd66a_cef7,
            ],
            a: [
                0xd3f2_5326,
                0x21bc_a45e,
                0x6cac_d2ce,
                0xd216_9ae4,
                0x9c4a_47c7,
                0xb09e_7856,
                0x3c68_57cc,
            ],
            usp: 0x74de_ed40,
            ssp: 0x800,
            pc: 0xc00,
            sr: 0x2700,
            prefetch: [0x4190, 0xa880],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The (A0) bound word @ 0xF25326 (A0 masked to 24 bits): 0x1F86 = 8070.
            (15_880_998u32, 31u8),
            (15_880_999, 134),
            // The queue refill word @ pc+4 = 3076: 0x709A = 28826.
            (3076, 112),
            (3077, 154),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_chk_no_trap_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 15_880_998,
                size: Size::Word,
                value: 8070,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 28826,
            },
        ]
    }

    fn assert_chk_no_trap_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 0xc02, "pc advanced one word (no trap)");
        assert_eq!(
            cpu.regs.sr, 0x2700,
            "SR unchanged: N preserved, Z/V/C clear, X kept"
        );
        assert_eq!(cpu.regs.ssp, 0x800, "no frame push (no trap)");
        assert_eq!(cpu.regs.prefetch, [0xa880, 0x709a], "queue advanced");
        assert_eq!(bus.log, expected_chk_no_trap_log());
    }

    /// The clean SST reference case `4d91 [CHK (A1),D6] 39` (44 cycles): `D6.w = 0x9C82` = −25470 < 0, so the
    /// CHK exception is taken with a leading **n6** (Dn<0 path). `bound = 0xC327` (read from `(A1)`). The frame
    /// (standard 6-byte, vector 6 @ 0x18) stacks saved PC = pc+2 = 3074 (= live `regs.pc` after the read +
    /// refill) and SR = 0x2718 (the live SR with CHK's N just set: X kept, N=1, Z/V/C clear) to ssp−6 = 2042
    /// (PCL @2046, SR @2042, PCH @2044, FC5), reads the vector @24/26 (FC5 → handler 0x2000), and reloads the
    /// queue at 0x2000 (FC6): 0x817F @8192, 0xD880 @8194.
    fn setup_chk_trap_low() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x502f_b282,
                0x8018_b802,
                0xe1c8_728e,
                0xd600_ce77,
                0xf788_7051,
                0xe527_51c9,
                0xcd3b_9c82,
                0xba5e_629f,
            ],
            a: [
                0x015b_5e6b,
                0xf5f4_dfd2,
                0x9c3b_01b5,
                0xdfa8_556d,
                0x7242_3f40,
                0xb970_4f68,
                0xb20d_144b,
            ],
            usp: 0x94ab_380a,
            ssp: 0x800,
            pc: 0xc00,
            sr: 0x2711, // X=1, C=1 set — X must be KEPT, C cleared by CHK
            prefetch: [0x4d91, 0x1f7c],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The vector-6 longword @0x18 = 24: 0x00002000 (hi 0x0000 @24, lo 0x2000 = 8192 @26).
            (24u32, 0u8),
            (25, 0),
            (26, 32),
            (27, 0),
            // The queue refill word @ pc+4 = 3076: 0xDBA7 = 56231.
            (3076, 219),
            (3077, 167),
            // The handler code @8192: 0x817F, 0xD880.
            (8192, 129),
            (8193, 127),
            (8194, 216),
            (8195, 128),
            // The (A1) bound word @ 0xF4DFD2 (A1 masked): 0xC327 = 49959.
            (16_048_082, 195),
            (16_048_083, 39),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_chk_trap_low_log() -> Vec<Transaction> {
        vec![
            // The FC5 bound read, then the FC6 queue refill.
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 16_048_082,
                size: Size::Word,
                value: 49959,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 56231,
            },
            // The standard frame, on-bus order PCL @ B+4, SR @ B+0, PCH @ B+2 (FC5).
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046, // PCL @ ssp−6+4
                size: Size::Word,
                value: 3074, // pc + 2 = live regs.pc after the read + refill
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2042, // SR @ ssp−6
                size: Size::Word,
                value: 10008, // 0x2718 — the live SR with CHK's N set (X kept, Z/V/C clear)
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044, // PCH @ ssp−6+2
                size: Size::Word,
                value: 0,
            },
            // The vector fetch (FC5 supervisor-data) — vector 6 @ 24/26.
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 24,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 26,
                size: Size::Word,
                value: 8192,
            },
            // The handler reload (FC6 supervisor-program), with the n2 idle between (not in the stream).
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 8192,
                size: Size::Word,
                value: 33151,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 8194,
                size: Size::Word,
                value: 55424,
            },
        ]
    }

    fn assert_chk_trap_low_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.ssp, 2042, "SP pushed down by 6 (standard frame)");
        assert_eq!(cpu.regs.pc, 8192, "pc landed at the vector-6 handler");
        assert_eq!(
            cpu.regs.sr, 0x2718,
            "SR = live SR with CHK's N set (X kept, N=1, Z/V/C clear); S/T no-op on the data"
        );
        assert_eq!(cpu.regs.usp, 0x94ab_380a, "usp untouched");
        assert_eq!(
            cpu.regs.prefetch,
            [0x817f, 0xd880],
            "queue reloaded at the handler"
        );
        // The pushed frame on the supervisor stack (big-endian words).
        assert_eq!(bus.peek(2042), 0x27, "SR hi @ B+0");
        assert_eq!(bus.peek(2043), 0x18, "SR lo @ B+1 (CCR = X|N)");
        assert_eq!(bus.peek(2046), 0x0C, "PCL hi @ B+4");
        assert_eq!(bus.peek(2047), 0x02, "PCL lo @ B+5 (pc+2)");
        assert_eq!(bus.log, expected_chk_trap_low_log());
    }

    /// The clean SST reference case `4396 [CHK (A6),D1] 75` (42 cycles): `D1.w = 0x0AF4` = 2804 > `bound`
    /// (`0xE293` = −7533, read from `(A6)`), so the CHK exception is taken with a leading **n4** (Dn>bound
    /// path — the two-predicate split: `over` picks n4, and N=0 because `Dn ≥ 0`). SR ends 0x2700 (X was 0).
    fn setup_chk_trap_high() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x8afc_d71e,
                0x5a62_0af4,
                0xfe26_361e,
                0xaaa0_ea31,
                0x9851_329d,
                0x9746_b4b2,
                0xe126_a247,
                0xd872_7324,
            ],
            a: [
                0xb4fb_8f95,
                0x3a48_1065,
                0xd7d4_a0b0,
                0x38ce_3485,
                0x448c_8eb8,
                0x437b_2dbe,
                0x6455_6e9e,
            ],
            usp: 0x08c1_3126,
            ssp: 0x800,
            pc: 0xc00,
            sr: 0x2703, // V=1, C=1 set — both cleared by CHK; X was 0
            prefetch: [0x4396, 0x56d3],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (24u32, 0u8),
            (25, 0),
            (26, 32),
            (27, 0),
            (3076, 190),
            (3077, 253),
            (8192, 72),
            (8193, 34),
            (8194, 153),
            (8195, 159),
            // The (A6) bound word @ 0x556E9E (A6 masked): 0xE293 = 58003.
            (5_598_878, 226),
            (5_598_879, 147),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    /// The clean SST reference case `45bc [CHK #imm,D2] 21` (42 cycles): the bound is the **immediate**
    /// (`prefetch[1] = 0xC45C` = −15268); `D2.w = 0x53D1` = 21457 > bound, so trap with a leading **n4**
    /// (Dn>bound). The `#imm` recipe captures the immediate into scratch BEFORE the two refills, runs both
    /// refills, then `ChkTrap` last — so saved PC = pc+4 = 3076 (= live `regs.pc` after both prefetches), NOT
    /// pc+2: the decode-time bound still stacks the post-prefetch PC, the load-bearing `saved PC = regs.pc`.
    fn setup_chk_imm_trap() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xddc1_536a,
                0x44d7_73e9,
                0x1549_53d1,
                0xb2d6_01fb,
                0xa6f9_79ad,
                0x75fe_970d,
                0xbb77_1e36,
                0x1f10_b876,
            ],
            a: [
                0x037a_1d9b,
                0x448d_ffb3,
                0x00a3_1a4f,
                0x52c0_9625,
                0xf439_8bf5,
                0x1587_6490,
                0xc1fa_829d,
            ],
            usp: 0x6a95_0b76,
            ssp: 0x800,
            pc: 0xc00,
            sr: 0x2701, // C=1 set — cleared by CHK; X was 0
            prefetch: [0x45bc, 0xc45c],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (24u32, 0u8),
            (25, 0),
            (26, 32),
            (27, 0),
            // The two queue refill words @ pc+4 = 3076 and @ pc+6 = 3078.
            (3076, 217),
            (3077, 223),
            (3078, 223),
            (3079, 94),
            (8192, 175),
            (8193, 165),
            (8194, 182),
            (8195, 94),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn run_instruction_matches_chk_no_trap() {
        let (mut cpu, mut bus) = setup_chk_no_trap();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 14, "CHK no-trap = bound read + refill + n6 = 4+4+6");
        assert_chk_no_trap_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_chk_no_trap() {
        let (mut rtc, mut bus_rtc) = setup_chk_no_trap();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_chk_no_trap();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_chk_no_trap_final(&step, &bus_step);
    }

    #[test]
    fn run_instruction_matches_chk_trap_low() {
        let (mut cpu, mut bus) = setup_chk_trap_low();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 44,
            "CHK trap (Dn<0) = read + refill + n6 + 3 frame writes + 2 vector reads + 2 reloads + n2 = \
             4+4+6+12+8+8+2 = 44"
        );
        assert_chk_trap_low_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_chk_trap_low() {
        let (mut rtc, mut bus_rtc) = setup_chk_trap_low();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_chk_trap_low();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 44);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_chk_trap_low_final(&step, &bus_step);
    }

    #[test]
    fn run_instruction_matches_chk_trap_high() {
        let (mut cpu, mut bus) = setup_chk_trap_high();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 42,
            "CHK trap (Dn>bound) = read + refill + n4 + frame = 44 − 2 (n4 not n6) = 42"
        );
        assert_eq!(cpu.regs.ssp, 2042, "SP pushed down by 6");
        assert_eq!(cpu.regs.pc, 8192, "pc landed at the vector-6 handler");
        assert_eq!(
            cpu.regs.sr, 0x2700,
            "SR: N=0 (Dn>bound, Dn≥0), Z/V/C clear, X kept (was 0)"
        );
        assert_eq!(cpu.regs.prefetch, [0x4822, 0x999f], "queue reloaded");
        assert_eq!(bus.peek(2042), 0x27, "SR hi @ B+0");
        assert_eq!(bus.peek(2043), 0x00, "SR lo @ B+1 (CCR cleared)");
        assert_eq!(bus.peek(2046), 0x0C, "PCL hi @ B+4");
        assert_eq!(bus.peek(2047), 0x02, "PCL lo @ B+5 (pc+2)");
    }

    #[test]
    fn run_instruction_matches_chk_imm_trap() {
        let (mut cpu, mut bus) = setup_chk_imm_trap();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 42,
            "CHK #imm trap (Dn>bound) = 2 refills + n4 + frame = 8+4+30 = 42"
        );
        assert_eq!(cpu.regs.ssp, 2042, "SP pushed down by 6");
        assert_eq!(cpu.regs.pc, 8192, "pc landed at the vector-6 handler");
        assert_eq!(cpu.regs.sr, 0x2700, "SR: N=0, Z/V/C clear, X kept (was 0)");
        assert_eq!(cpu.regs.prefetch, [0xafa5, 0xb65e], "queue reloaded");
        // saved PC = pc+4 = 3076 (the live regs.pc AFTER both #imm prefetches — the load-bearing
        // "saved PC = regs.pc" even for the decode-time bound).
        assert_eq!(bus.peek(2046), 0x0C, "PCL hi @ B+4");
        assert_eq!(
            bus.peek(2047),
            0x04,
            "PCL lo @ B+5 = pc+4 (after both prefetches)"
        );
    }

    #[test]
    fn both_drivers_match_chk_imm_trap() {
        let (mut rtc, mut bus_rtc) = setup_chk_imm_trap();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_chk_imm_trap();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 42);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
    }

    #[test]
    fn chk_trap_quiescable_and_serializable_across_the_trap() {
        // The snapshot/restore anchor ACROSS a memory-source CHK trap (`4d91`): quiesce at the faulting
        // ChkTrap AND at every boundary of the installed 6-byte frame, snapshot the WHOLE CPU (the in-flight
        // cursor — the original CHK recipe before the trap, then the rewritten frame recipe + seeded scratch
        // after it), restore, resume, and get an identical result. Proves the in-place MicroState rewrite is
        // serializable across the CHK trap (the Shape-B reuse).
        let (mut rref, mut bref) = setup_chk_trap_low();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // The recipe before the trap is [Read, Prefetch, ChkTrap] (the ChkTrap aborts, returning Continue),
        // then the 17-op frame ([Internal, EnterException, AdjustAddr, EaCalc, Write, Write, EaCalc, Write,
        // LoadImm, Read, EaCalc, Read, Combine32, SetPc, Prefetch, Internal, Prefetch]) → 3 + 17 = 20
        // step_micro_op calls (19 Continue + 1 Done). Snapshot after 0..=19 Continue boundaries — spanning the
        // pre-trap reads, the ChkTrap abort itself, and the whole installed frame.
        for pause_after in 0..=19 {
            let (mut cpu, mut bus) = setup_chk_trap_low();
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

    // --- E6: the privileged `*toSR` ops (the mid-instruction FC switch) + RESET (the n124 idle). Pinned to
    // the vendored anchors `027c [ANDItoSR #] 2` (S→user switch, FC6→FC2) and `4e70 [RESET] 1` (len 132). ---

    /// The SST anchor `027c [ANDItoSR #] 2` (20 cyc): the AND CLEARS S (`sr 0x2717 & imm 0x4CBE = 0x0416`,
    /// `& 0xA71F = 0x0416`), so the two re-prefetch reads switch from FC=6 (supervisor-program) to **FC=2
    /// (user-program)** — the load-bearing mid-instruction FC switch. The leading discard read @ pc+4 runs
    /// under the OLD FC=6; the bus stream is `[r FC6 @3076, r FC2 @3076, r FC2 @3078]` (3 word reads + n8 = 20).
    /// pc 3072 → 3076, prefetch [0x027C, 0x4CBE] → [word@3076=0x5921, word@3078=0x6FAE].
    fn setup_andi_to_sr_switch() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                4_271_004_033,
                939_242_940,
                2_332_356_921,
                1_759_078_758,
                3_632_954_379,
                1_792_387_683,
                3_162_307_581,
                2_565_808_449,
            ],
            a: [
                336_893_371,
                1_804_575_635,
                421_147_092,
                900_207_258,
                3_509_327_830,
                1_274_346_764,
                802_666_739,
            ],
            usp: 1_552_985_130,
            ssp: 2048,
            pc: 3072,
            sr: 10007,              // 0x2717 (S=1, T=0)
            prefetch: [636, 19646], // prefetch[0] = 0x027C (ANDItoSR #), prefetch[1] = imm 0x4CBE
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            (3076u32, 89u8), // word @3076 = 0x5921 (= 22817)
            (3077, 33),
            (3078, 111), // word @3078 = 0x6FAE (= 28590)
            (3079, 174),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_andi_switch_log() -> Vec<Transaction> {
        vec![
            // The discard read @ pc+4 under the OLD function code (FC6 supervisor-program).
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 22817,
            },
            // The two re-prefetch reads under the NEW mode's FC (FC2 user-program — S was cleared).
            Transaction {
                kind: TxKind::Read,
                fc: 2,
                addr: 3076,
                size: Size::Word,
                value: 22817,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 2,
                addr: 3078,
                size: Size::Word,
                value: 28590,
            },
        ]
    }

    #[test]
    fn run_instruction_matches_andi_to_sr_switch() {
        let (mut cpu, mut bus) = setup_andi_to_sr_switch();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "ANDItoSR = discard read + n8 + 2 prefetches = 4 + 8 + 8 = 20"
        );
        assert_eq!(
            cpu.regs.sr, 0x0416,
            "(0x2717 & 0x4CBE) & 0xA71F = 0x0416 (S cleared)"
        );
        assert_eq!(cpu.regs.pc, 3076, "pc advanced by 4 (two prefetches)");
        assert_eq!(
            cpu.regs.prefetch,
            [22817, 28590],
            "queue reloaded at pc+4 / pc+6"
        );
        assert_eq!(
            bus.log,
            expected_andi_switch_log(),
            "the FC6→FC2 switch stream"
        );
    }

    #[test]
    fn both_drivers_match_andi_to_sr_switch() {
        let (mut rtc, mut bus_rtc) = setup_andi_to_sr_switch();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_andi_to_sr_switch();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 20);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
    }

    #[test]
    fn andi_to_sr_quiescable_and_serializable_across_the_switch() {
        // The snapshot/restore anchor ACROSS the `*toSR` mid-instruction FC switch: the whole CPU (incl. the
        // in-flight cursor) round-trips at every micro-op boundary — crucially across the `SrLogic` step that
        // clears S, so the two re-prefetches that resume after a restore still run under the NEW (FC2) mode.
        let (mut rref, mut bref) = setup_andi_to_sr_switch();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 5 micro-ops (Read, Internal, SrLogic, Prefetch, Prefetch) → 0..=4 Continue boundaries.
        for pause_after in 0..=4 {
            let (mut cpu, mut bus) = setup_andi_to_sr_switch();
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

    /// The SST anchor `4e70 [RESET] 1` (132 cyc): assert the reset line for 124 cycles. No register state
    /// changes beyond the queue refill: `[Internal(4), Internal(124), Prefetch]`. pc 3072 → 3074, sr unchanged
    /// (0x271B), prefetch [0x4E70, 0xE695] → [0xE695, word@3076=0x457F]. Bus: one FC=6 read @ pc+4.
    fn setup_reset() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                4_213_175_460,
                2_412_132_266,
                3_680_050_421,
                1_150_690_420,
                3_629_634_968,
                2_043_518_162,
                1_806_498_751,
                1_673_573_331,
            ],
            a: [
                37_052_095,
                1_207_300_143,
                1_911_943_729,
                647_123_814,
                1_566_265_857,
                2_212_948_340,
                1_330_467_793,
            ],
            usp: 3_983_537_698,
            ssp: 2048,
            pc: 3072,
            sr: 10011,                // 0x271B (S=1)
            prefetch: [20080, 59029], // prefetch[0] = 0x4E70 (RESET)
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 69); // word @3076 = 0x457F (= 17791)
        bus.poke(3077, 127);
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn run_instruction_matches_reset() {
        let (mut cpu, mut bus) = setup_reset();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 132,
            "RESET = n4 + n124 + one prefetch = 4 + 124 + 4 = 132"
        );
        assert_eq!(cpu.regs.sr, 0x271B, "RESET does not change the SR");
        assert_eq!(
            cpu.regs.pc, 3074,
            "pc advanced by one word (the queue refill)"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [59029, 17791],
            "queue shifted + refilled from pc+4"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 17791,
            }],
            "RESET's only bus event is the FC=6 queue refill @ pc+4"
        );
    }

    #[test]
    fn both_drivers_match_reset() {
        let (mut rtc, mut bus_rtc) = setup_reset();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_reset();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 132);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
    }

    #[test]
    fn reset_quiescable_and_serializable_across_the_idle() {
        // The snapshot/restore anchor ACROSS RESET's long n124 idle: the whole CPU round-trips at every
        // micro-op boundary, including the gap between the two Internal idles and before the final refill.
        let (mut rref, mut bref) = setup_reset();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Internal(4), Internal(124), Prefetch) → 0..=2 Continue boundaries.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_reset();
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

    // --- N0: CMP <ea>,Dn — the flag-only compare (AluOp::Cmp + Dest::None). The recipe is `ea_src` driving
    // a flag-only Alu (Dn the minuend, Dn − <ea>), no write-back. The critical invariant is X PRESERVED (CMP
    // never sets X like SUB does). Anchors pinned to the vendored CMP.{w,l} SST stream. ---

    /// The clean SST reference case `b685 [CMP.l D5,D3]` (Dn source, 6 cycles). opcode 0xB685: opmode 2
    /// (`.l`), dst reg D3 (bits 11-9), source mode 0 reg 5 (D5). Computes D3 − D5 = 0x7C30354C − 0x6EAFDC53 =
    /// 0x0D805 8F9 (positive, nonzero) → N=0 Z=0 V=0 C=0. The X bit (set in the initial SR 0x2714) must be
    /// PRESERVED (final SR 0x2710 keeps X) — the load-bearing CMP-vs-SUB difference. Bus: one FC-6 refill at
    /// 3076, then an n2 idle (no operand read — Dn direct).
    fn setup_cmp_b685() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2714, // X + Z set
            prefetch: [0xB685, 0x0B11],
        };
        regs.d[5] = 1_856_844_115; // 0x6EAFDC53
        regs.d[3] = 2_083_522_828; // 0x7C30354C
        let mut bus = FlatBus::new();
        bus.poke(3076, 0xB5);
        bus.poke(3077, 0x2C); // 0xB52C = 46380 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_cmp_b685_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 46380,
        }]
    }

    fn assert_cmp_b685_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2710,
            "CMP set N/Z/V/C (all clear: positive nonzero diff) but PRESERVED X"
        );
        assert_eq!(
            cpu.regs.d[3], 2_083_522_828,
            "Dn (minuend) unchanged — no write-back"
        );
        assert_eq!(cpu.regs.d[5], 1_856_844_115, "source D5 unchanged");
        assert_eq!(cpu.regs.prefetch, [0x0B11, 46380], "queue advanced");
        assert_eq!(bus.log, expected_cmp_b685_log());
    }

    #[test]
    fn run_instruction_matches_cmp_b685() {
        let (mut cpu, mut bus) = setup_cmp_b685();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 6,
            "Dn-source CMP.l = [Prefetch, Alu, Internal(4)] = 6"
        );
        assert_cmp_b685_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_cmp_b685() {
        let (mut rtc, mut bus_rtc) = setup_cmp_b685();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_cmp_b685();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 6);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_cmp_b685_final(&step, &bus_step);
    }

    #[test]
    fn cmp_b685_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the flag-only compare shape (Alu with Dest::None, no write-back).
        let (mut rref, mut bref) = setup_cmp_b685();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Prefetch, Alu, Internal(4)) → boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_cmp_b685();
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

    #[test]
    fn cmp_class_classifies_by_opcode_not_name() {
        // The load-bearing classifier: CMP.* files mix CMP + CMPM + CMPI; classify by OPCODE.
        assert_eq!(cmp_class(0xB685), CmpClass::Cmp); // CMP.l D5,D3 (opmode 2)
        assert_eq!(cmp_class(0xB650), CmpClass::Cmp); // CMP.w (A0),D3 (opmode 1)
        assert_eq!(cmp_class(0xBC88), CmpClass::Cmp); // CMP.l A0,D6 (opmode 2)
        assert_eq!(cmp_class(0xB108), CmpClass::Cmpm); // CMPM.b (A0)+,(A0)+ (opmode 4)
        assert_eq!(cmp_class(0x0C40), CmpClass::Cmpi); // CMPI.w #imm,D0
        assert_eq!(cmp_class(0xB0C0), CmpClass::Cmpa); // CMPA.w D0,A0 (opmode 3)
        assert_eq!(cmp_class(0xB1C0), CmpClass::Cmpa); // CMPA.l D0,A0 (opmode 7)
        assert_eq!(cmp_class(0xD040), CmpClass::None); // ADD.w — not a CMP opcode
    }

    /// The clean SST anchor `b38c [CMPM.l (A4)+,(A1)+]` (20 cyc): a long compare-memory — read the source long
    /// at `(A4)+` (hi @ A4, lo @ A4+2) FIRST, then the destination long at `(A1)+`, and compare `(A1) − (A4)`
    /// flag-only. Both registers post-increment by 4. Bus: `[r A4, r A4+2, r A1, r A1+2, PF]`. This exercises
    /// the two-post-increment-long-read + `Combine32` + `AluOp::Cmp`/`Dest::None` composition.
    fn setup_cmpm_l_b38c() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                4_051_099_155,
                3_430_939_128,
                2_024_841_587,
                1_388_351_731,
                653_086_419,
                2_046_873_409,
                3_697_922_867,
                4_130_196_076,
            ],
            a: [
                2_207_835_777,
                2_214_816_844, // A1 (dest)
                798_385_746,
                1_951_553_745,
                3_110_298_684, // A4 (source)
                3_309_464_988,
                4_196_267_377,
            ],
            usp: 3_954_082_546,
            ssp: 2048,
            pc: 3072,
            sr: 9995,                 // 0x270B
            prefetch: [45964, 12117], // prefetch[0] = 0xB38C (CMPM.l)
        };
        let mut bus = FlatBus::new();
        for (addr, val) in [
            (3076u32, 119u8),
            (3077, 65),
            (6_513_724, 161), // (A4) long
            (6_513_725, 222),
            (6_513_726, 150),
            (6_513_727, 145),
            (224_332, 179), // (A1) long
            (224_333, 219),
            (224_334, 180),
            (224_335, 236),
        ] {
            bus.poke(addr, val);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn cmpm_l_quiescable_and_serializable_across_the_two_postinc_reads() {
        // The snapshot/restore anchor for CMPM's new composition (two (An)+ long reads feeding a flag-only Cmp).
        // Reference run via the run-to-completion driver, then snapshot/restore at every micro-op boundary.
        let (mut rref, mut bref) = setup_cmpm_l_b38c();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        for pause_after in 0.. {
            let (mut cpu, mut bus) = setup_cmpm_l_b38c();
            cpu.start_instruction();
            let mut finished = false;
            for _ in 0..pause_after {
                if let Step::Done(_) = cpu.step_micro_op(&mut bus) {
                    finished = true;
                    break;
                }
            }
            if finished {
                break; // stepped past the last boundary — all boundaries covered.
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

    // --- N2: CMPI #imm,<ea> — the flag-only `<ea> − #imm` compare (AluOp::Cmp + Dest::None). The immediate
    // (one ext word for b/w, two for .l) is captured BEFORE the EA's extension words; the data-alterable EA is
    // read and DISCARDED (no write). The Dn-dest form has no memory access. X is PRESERVED. ---

    /// The clean SST anchor `0c82 [CMPI.l #imm,D2]` (14 cyc): a long immediate compare against a data register
    /// — no memory access. The 32-bit immediate is two extension words (captured before the EA is "read"); the
    /// EA is `D2`-direct, so the recipe is `[Combine(imm.hi), PF, Combine(imm), PF, PF, Alu(D2 − imm), n2]`.
    /// `D2 − #imm` is computed flag-only (X preserved, no write). Bus: three FC-6 refills + an n2 idle.
    fn setup_cmpi_l_0c82() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2711,                 // X set (0x10) + C set (0x01)
            prefetch: [0x0C82, 0x7A32], // opcode + imm hi word
        };
        regs.d[2] = 0x1234_5678;
        let mut bus = FlatBus::new();
        // imm lo word @ 3076, then two trailing refill words @ 3078/3080.
        for (addr, val) in [
            (3076u32, 0xF8u8),
            (3077, 0x93),
            (3078, 0xDB),
            (3079, 0x07),
            (3080, 0xDD),
            (3081, 0x4F),
        ] {
            bus.poke(addr, val);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn cmpi_l_dn_match() {
        let (mut cpu, mut bus) = setup_cmpi_l_0c82();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 14, "Dn-dest CMPI.l = 3 refills + n2 = 14");
        assert_eq!(cpu.regs.pc, 3078, "pc advanced three words");
        assert_eq!(
            cpu.regs.d[2], 0x1234_5678,
            "EA (Dn) unchanged — no write-back"
        );
        // D2 − imm = 0x12345678 − 0x7A32F893 = negative → N=1, borrow → C=1; X preserved (was set).
        let imm: u32 = 0x7A32_F893;
        let (_r, want) = {
            let a = 0x1234_5678u32;
            let res = a.wrapping_sub(imm);
            let mut ccr = 0u16;
            if res & 0x8000_0000 != 0 {
                ccr |= crate::m68000::registers::CCR_N;
            }
            if res == 0 {
                ccr |= crate::m68000::registers::CCR_Z;
            }
            let am = a & 0x8000_0000 != 0;
            let bm = imm & 0x8000_0000 != 0;
            let rm = res & 0x8000_0000 != 0;
            if (am != bm) && (rm != am) {
                ccr |= crate::m68000::registers::CCR_V;
            }
            if a < imm {
                ccr |= crate::m68000::registers::CCR_C;
            }
            (res, ccr)
        };
        assert_eq!(
            cpu.regs.sr & 0x1F,
            (want | crate::m68000::registers::CCR_X) & 0x1F,
            "CMPI set N/Z/V/C like SUB but PRESERVED X"
        );
    }

    /// The clean SST anchor `0c93 [CMPI.l #imm,(A3)]` (20 cyc): a long immediate compare against a memory
    /// operand. The 32-bit immediate is captured first; the EA `(A3)` is read as a long (hi @ A3, lo @ A3+2)
    /// and DISCARDED. Bus: `[PF@3076, PF@3078, READ.hi, READ.lo, PF@3080]`. No write, no trailing idle. This
    /// exercises the long immediate-then-EA prefetch interleave + the long EA read feeding the flag-only Cmp.
    fn setup_cmpi_l_0c93() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [
                0x5B0B_9B50,
                0x2EFB_E146,
                0x5DEE_CDF3,
                0x7AF3_F41D,
                0x5CE5_392C,
                0xF662_F434,
                0xDC56_287C,
                0xCCE7_9169,
            ],
            a: [
                0x0EC9_2B3C,
                0x365C_6C77,
                0x7423_973D,
                0x9C0A_8AD8,
                0x8CA5_3AB4,
                0x95D1_1DA4,
                0x2FD9_3C96,
            ],
            usp: 0xD5B5_1E46,
            ssp: 0x800,
            pc: 0xC00,
            sr: 0x271E,
            prefetch: [0x0C93, 0xD618],
        };
        let _ = &mut regs;
        let mut bus = FlatBus::new();
        for (addr, val) in [
            (3081u32, 109u8),
            (3080, 34),
            (690905, 189),
            (690904, 75),
            (3079, 18),
            (3078, 41),
            (690907, 76),
            (3077, 81),
            (690906, 182),
            (3076, 75),
        ] {
            bus.poke(addr, val);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn cmpi_l_mem_match() {
        let (mut cpu, mut bus) = setup_cmpi_l_0c93();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 20,
            "(An)-dest CMPI.l = 3 refills + long read pair = 20"
        );
        assert_eq!(cpu.regs.pc, 0xC06, "pc advanced three words");
        assert_eq!(
            cpu.regs.sr, 0x2711,
            "CMPI set N/Z/V/C but PRESERVED X (final SR)"
        );
        assert_eq!(
            cpu.regs.a[3], 0x9C0A_8AD8,
            "A3 unchanged — plain (An), no auto-increment, no write"
        );
        let expected = vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 19281,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3078,
                size: Size::Word,
                value: 10514,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 690904,
                size: Size::Word,
                value: 19389,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 690906,
                size: Size::Word,
                value: 46668,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3080,
                size: Size::Word,
                value: 8813,
            },
        ];
        assert_eq!(bus.log, expected, "CMPI.l (An) bus stream");
    }

    #[test]
    fn cmpi_l_mem_quiescable_and_serializable_across_imm_and_ea_read() {
        // The snapshot/restore anchor for CMPI's new immediate-then-EA-read composition. Reference run via the
        // run-to-completion driver, then snapshot/restore at every micro-op boundary.
        let (mut rref, mut bref) = setup_cmpi_l_0c93();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        for pause_after in 0.. {
            let (mut cpu, mut bus) = setup_cmpi_l_0c93();
            cpu.start_instruction();
            let mut finished = false;
            for _ in 0..pause_after {
                if let Step::Done(_) = cpu.step_micro_op(&mut bus) {
                    finished = true;
                    break;
                }
            }
            if finished {
                break; // stepped past the last boundary — all boundaries covered.
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

    // --- N3: CMPA <ea>,An — the flag-only address compare (AluOp::Cmpa + Dest::None). An is the minuend
    // (full 32 bits); the `.w` source word is sign-extended to 32 before the long-boundary subtraction. Sets
    // N/Z/V/C, PRESERVES X, no write-back. CMPA = MOVEA's source bus stream + a uniform trailing n2 idle. ---

    /// The clean SST anchor `b8d3 [CMPA.w (A3),A4]` (10 cyc): a memory-source word compare — read the source
    /// word at `(A3)` (masked address `0xCEAEEE` = 13545198), refill, then the flag-only `A4 − sext16(src)` at
    /// the long boundary. A4 = 0xCAAE0A36, source word 0x4532 → sext 0x00004532 → 0xCAAE0A36 − 0x00004532 =
    /// 0xCAADC504 (negative) → N=1, Z=V=C=0; the initial X (set in SR 0x2712) is PRESERVED (final SR 0x2718 =
    /// X | N). A4 is UNCHANGED (no write-back — CMPA only sets flags). Bus: `[r 13545198, PF, n2]`. This
    /// exercises the memory-read + Cmpa/Dest::None + trailing-n2 composition.
    fn setup_cmpa_w_b8d3() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2712, // X + V set
            prefetch: [0xB8D3, 0x1B7C],
        };
        regs.a[3] = 0xA8CE_AEEE; // A3 (source base; bus masks to 0xCEAEEE = 13545198)
        regs.a[4] = 0xCAAE_0A36; // A4 (the minuend)
        let mut bus = FlatBus::new();
        bus.poke(13545198, 0x45);
        bus.poke(13545199, 0x32); // 0x4532 = 17714 — the source word
        bus.poke(3076, 0xC4);
        bus.poke(3077, 0xCC); // 0xC4CC = 50380 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_cmpa_w_b8d3_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 13545198,
                size: Size::Word,
                value: 17714,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 50380,
            },
        ]
    }

    fn assert_cmpa_w_b8d3_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2718,
            "CMPA set N (negative diff), cleared Z/V/C, PRESERVED X"
        );
        assert_eq!(
            cpu.regs.a[4], 0xCAAE_0A36,
            "An (minuend) unchanged — CMPA writes no value"
        );
        assert_eq!(cpu.regs.a[3], 0xA8CE_AEEE, "source base A3 unchanged");
        assert_eq!(cpu.regs.prefetch, [0x1B7C, 50380], "queue advanced");
        assert_eq!(bus.log, expected_cmpa_w_b8d3_log());
    }

    #[test]
    fn run_instruction_matches_cmpa_w_b8d3() {
        let (mut cpu, mut bus) = setup_cmpa_w_b8d3();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 10,
            "CMPA.w (An) = [Read, Prefetch, Alu, Internal(2)] = 4+4+0+2 = 10 (MOVEA.w (An) 8 + n2)"
        );
        assert_cmpa_w_b8d3_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_cmpa_w_b8d3() {
        let (mut rtc, mut bus_rtc) = setup_cmpa_w_b8d3();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_cmpa_w_b8d3();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 10);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_cmpa_w_b8d3_final(&step, &bus_step);
    }

    #[test]
    fn cmpa_w_b8d3_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the CMPA shape (Alu{Cmpa} with Dest::None, the trailing n2 idle).
        let (mut rref, mut bref) = setup_cmpa_w_b8d3();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Internal(2)) → boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_cmpa_w_b8d3();
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

    #[test]
    fn cmpa_decode_classifies_and_sizes() {
        // CMPA is opmode 3 (.w) / 7 (.l) of the 0xB nibble — classified by OPCODE, its own decode arm.
        assert_eq!(cmp_class(0xB0C0), CmpClass::Cmpa); // CMPA.w D0,A0 (opmode 3)
        assert_eq!(cmp_class(0xB1C0), CmpClass::Cmpa); // CMPA.l D0,A0 (opmode 7)
        assert_eq!(cmp_class(0xB8D3), CmpClass::Cmpa); // CMPA.w (A3),A4
        assert_eq!(cmp_class(0xB9FC), CmpClass::Cmpa); // CMPA.l #imm,A4
    }

    // --- N4: TST <ea> — the flag-only test `<ea> − 0` (AluOp::Cmp with b=Zero + Dest::None). Sets N=msb,
    // Z=(operand==0), clears V/C, PRESERVES X, no write-back. UNLIKE CMP/ADD there is NO trailing idle for any
    // size (TST.l Dn = 4 not 6; TST.l (An) = 12 not 14). Anchors pinned to the vendored TST.{b,w,l} stream. ---

    /// The clean SST anchor `4a03 [TST.b D3]` (4 cyc): a Dn-source byte test setting N (the low byte 0xE9 has
    /// its msb set). D3 = 0x12EE3AE9 → low byte 0xE9 → N=1, Z=0, V=C=0; the initial X (in SR 0x2711) is
    /// PRESERVED (final SR 0x2718 = X | N, the initial Z cleared). D3 is UNCHANGED (no write-back). Bus: one
    /// FC-6 refill at 3076 (no operand read — Dn direct, no trailing idle). This is the N-set Dn anchor.
    fn setup_tst_b_4a03() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2711, // X + Z set
            prefetch: [0x4A03, 0x7237],
        };
        regs.d[3] = 0x12EE_3AE9;
        let mut bus = FlatBus::new();
        bus.poke(3076, 0xA2);
        bus.poke(3077, 0x74); // 0xA274 = 41588 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_tst_b_4a03_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 41588,
        }]
    }

    fn assert_tst_b_4a03_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2718,
            "TST.b set N (msb of low byte), cleared Z/V/C, PRESERVED X"
        );
        assert_eq!(
            cpu.regs.d[3], 0x12EE_3AE9,
            "Dn unchanged — TST writes nothing"
        );
        assert_eq!(cpu.regs.prefetch, [0x7237, 41588], "queue advanced");
        assert_eq!(bus.log, expected_tst_b_4a03_log());
    }

    #[test]
    fn run_instruction_matches_tst_b_4a03() {
        let (mut cpu, mut bus) = setup_tst_b_4a03();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 4,
            "Dn-source TST.b = [Prefetch, Alu] = 4 (NO trailing idle)"
        );
        assert_tst_b_4a03_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_tst_b_4a03() {
        let (mut rtc, mut bus_rtc) = setup_tst_b_4a03();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_tst_b_4a03();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 4);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_tst_b_4a03_final(&step, &bus_step);
    }

    /// The clean SST anchor `4a56 [TST.w (A6)]` (8 cyc): a memory-source word test. Read the source word at
    /// `(A6)` (masked address 0x223E86 = 2244230), refill, then the flag-only `src − 0`. The source word
    /// 0x44A2 is positive nonzero → N=0 Z=0 V=0 C=0; A6 is UNCHANGED (TST reads, never writes). Bus: `[r
    /// 2244230, PF]` (no trailing idle). The `[Read, Prefetch]` memory interleave anchor.
    fn setup_tst_w_4a56() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2706,
            prefetch: [0x4A56, 0xD4DC],
        };
        regs.a[6] = 0x1322_3E86; // A6 (source base; bus masks to 0x223E86 = 2244230)
        let mut bus = FlatBus::new();
        bus.poke(2244230, 0x44);
        bus.poke(2244231, 0xA2); // 0x44A2 = 17570 — the source word
        bus.poke(3076, 0x9F);
        bus.poke(3077, 0xBF); // 0x9FBF = 40895 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_tst_w_4a56_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2244230,
                size: Size::Word,
                value: 17570,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 40895,
            },
        ]
    }

    fn assert_tst_w_4a56_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2700,
            "TST.w cleared N/Z/V/C (positive nonzero), X already clear"
        );
        assert_eq!(cpu.regs.a[6], 0x1322_3E86, "source base A6 unchanged");
        assert_eq!(cpu.regs.prefetch, [0xD4DC, 40895], "queue advanced");
        assert_eq!(bus.log, expected_tst_w_4a56_log());
    }

    #[test]
    fn run_instruction_matches_tst_w_4a56() {
        let (mut cpu, mut bus) = setup_tst_w_4a56();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "TST.w (An) = [Read, Prefetch, Alu] = 8 (NO trailing idle)"
        );
        assert_tst_w_4a56_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_tst_w_4a56() {
        let (mut rtc, mut bus_rtc) = setup_tst_w_4a56();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_tst_w_4a56();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_tst_w_4a56_final(&step, &bus_step);
    }

    /// The clean SST anchor `4a97 [TST.l (A7)]` (12 cyc): a long memory-source test through A7 (= SSP 0x800 =
    /// 2048, S set). Read the source long (hi @ 2048, lo @ 2050), refill, then the flag-only `src − 0`. The
    /// long 0x00076777 is positive nonzero → N=0 Z=0 V=0 C=0; the initial X (in SR 0x2712, with V) is PRESERVED
    /// and V cleared (final SR 0x2710). A7 is UNCHANGED. Bus: `[r 2048, r 2050, PF]` (the long read pair, NO
    /// trailing idle — TST.l (An) = 12, unlike CMP/ADD.l's 14). The long `[Read.hi, Read.lo, Prefetch]` anchor.
    fn setup_tst_l_4a97() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0xE452_0D26,
            ssp: 2048, // A7 (S set → SSP is the active A7)
            pc: 3072,
            sr: 0x2712, // X + V set
            prefetch: [0x4A97, 0xB0EC],
        };
        let mut bus = FlatBus::new();
        bus.poke(2048, 0x07);
        bus.poke(2049, 0x8E); // hi word 0x078E = 1934
        bus.poke(2050, 0x67);
        bus.poke(2051, 0xF7); // lo word 0x67F7 = 26615 → long 0x078E67F7
        bus.poke(3076, 0x87);
        bus.poke(3077, 0x29); // 0x8729 = 34601 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_tst_l_4a97_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2048,
                size: Size::Word,
                value: 1934,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 2050,
                size: Size::Word,
                value: 26615,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 34601,
            },
        ]
    }

    fn assert_tst_l_4a97_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2710,
            "TST.l cleared N/Z/V/C (positive nonzero diff), PRESERVED X"
        );
        assert_eq!(
            cpu.regs.ssp, 2048,
            "A7 (SSP) unchanged — TST writes nothing"
        );
        assert_eq!(cpu.regs.prefetch, [0xB0EC, 34601], "queue advanced");
        assert_eq!(bus.log, expected_tst_l_4a97_log());
    }

    #[test]
    fn run_instruction_matches_tst_l_4a97() {
        let (mut cpu, mut bus) = setup_tst_l_4a97();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "TST.l (An) = [Read.hi, Read.lo, Prefetch, Alu] = 12 (NO trailing idle)"
        );
        assert_tst_l_4a97_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_tst_l_4a97() {
        let (mut rtc, mut bus_rtc) = setup_tst_l_4a97();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_tst_l_4a97();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_tst_l_4a97_final(&step, &bus_step);
    }

    #[test]
    fn tst_l_4a97_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the TST shape (Alu{Cmp, b=Zero} with Dest::None, no trailing idle).
        let (mut rref, mut bref) = setup_tst_l_4a97();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 6 micro-ops (Read.hi, EaCalc(lo addr), Read.lo, Combine32, Prefetch, Alu) → boundaries after 0..=5.
        for pause_after in 0..=5 {
            let (mut cpu, mut bus) = setup_tst_l_4a97();
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

    #[test]
    fn tst_decode_recognizes_opcode_and_size() {
        // TST is 0x4A00/4A40/4A80 (SS bits 7-6 = b/w/l); SS == 3 (0x4AC0) is TAS, not TST. Decode must produce
        // a recipe (no panic) for the data-alterable modes and reject TAS via the SS != 3 guard.
        for (op, _sz) in [(0x4A03u16, "b"), (0x4A56, "w"), (0x4A97, "l")] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            // Just exercising the decode arm — it must not panic / hit the todo!() fallthrough.
            let _ = decode(&regs);
        }
    }

    // --- N5: CLR <ea> — clear the data-alterable EA to 0 (Z=1/N=0/V=0/C=0, X PRESERVED = move_flags(0)). CLR
    // is a READ-then-WRITE (reuses the `ea_dst`/`ea_dst_long` RMW path): read the EA (discarded), refill,
    // Move-of-zero (sets flags + parks the 0), write 0. Dn-direct has no memory (CLR.l Dn = 6 cyc, one trailing
    // idle; CLR.b/.w Dn = 4). Anchors pinned to the vendored CLR.{b,w,l} stream. ---

    /// The clean SST anchor `4282 [CLR.l D2]` (6 cyc): a Dn-direct long clear. D2 = 0xBF86_8741 → 0; the flags
    /// become `move_flags(0)` (Z set, N/V/C cleared) with the initial X (in SR 0x2707) PRESERVED → final SR
    /// 0x2704 (X clear here, so just Z). Bus: one FC-6 refill at 3076 (no operand access — Dn direct), then the
    /// CLR.l Dn trailing idle (n2 → 6 cyc, unlike CLR.b/.w Dn = 4). The 6-cyc register-clear anchor.
    fn setup_clr_l_4282() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x2707, // low byte 0x07 = C | V | Z set (X clear at bit4) — Z+V+C cleared, X preserved by CLR
            prefetch: [0x4282, 21847],
        };
        regs.d[2] = 0xBF86_8741;
        let mut bus = FlatBus::new();
        bus.poke(3076, 0xA0);
        bus.poke(3077, 0xF8); // 0xA0F8 = 41208 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_clr_l_4282_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 41208,
        }]
    }

    fn assert_clr_l_4282_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2704,
            "CLR.l set Z, cleared N/V/C, PRESERVED X (= move_flags(0))"
        );
        assert_eq!(cpu.regs.d[2], 0, "D2 cleared to 0 (full 32 bits)");
        assert_eq!(cpu.regs.prefetch, [21847, 41208], "queue advanced");
        assert_eq!(bus.log, expected_clr_l_4282_log());
    }

    #[test]
    fn run_instruction_matches_clr_l_4282() {
        let (mut cpu, mut bus) = setup_clr_l_4282();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 6,
            "CLR.l Dn = [Prefetch, Alu, Internal(2)] = 6 (one trailing idle)"
        );
        assert_clr_l_4282_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_clr_l_4282() {
        let (mut rtc, mut bus_rtc) = setup_clr_l_4282();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_clr_l_4282();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 6);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_clr_l_4282_final(&step, &bus_step);
    }

    /// The clean SST anchor `4216 [CLR.b (A6)]` (12 cyc): the canonical READ-then-WRITE byte clear. READ the old
    /// byte at `(A6)` (masked 0xCD04BB = 13432315), refill, Move-of-zero (sets the flags), WRITE 0 at the same
    /// address. The flags become `move_flags(0)` (Z set, N/V cleared) with X PRESERVED (SR 0x271E → 0x2714). The
    /// load-bearing pin: the read FC-5 PRECEDES the write FC-5 (CLR is not write-only). Bus: `[r @ EA, PF, w 0 @
    /// EA]`. A6 is UNCHANGED (no auto-(in/de)crement).
    fn setup_clr_b_4216() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x271E, // X + N + Z + V set
            prefetch: [0x4216, 37610],
        };
        regs.a[6] = 0xE0CC_F5FB; // A6 (bus masks to 0xCCF5FB = 13432315)
        let mut bus = FlatBus::new();
        bus.poke(13432315, 0x96); // old byte 0x96 = 150 (the read value, discarded)
        bus.poke(3076, 0xDE);
        bus.poke(3077, 0xD4); // 0xDED4 = 57044 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_clr_b_4216_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 13432315,
                size: Size::Byte,
                value: 150,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 57044,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 13432315,
                size: Size::Byte,
                value: 0,
            },
        ]
    }

    fn assert_clr_b_4216_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.sr, 0x2714,
            "CLR.b set Z, cleared N/V/C, PRESERVED X (= move_flags(0))"
        );
        assert_eq!(
            cpu.regs.a[6], 0xE0CC_F5FB,
            "A6 unchanged (no auto-increment)"
        );
        assert_eq!(cpu.regs.prefetch, [37610, 57044], "queue advanced");
        assert_eq!(bus.peek(13432315), 0, "the cleared byte is 0");
        assert_eq!(bus.log, expected_clr_b_4216_log());
    }

    #[test]
    fn run_instruction_matches_clr_b_4216() {
        let (mut cpu, mut bus) = setup_clr_b_4216();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "CLR.b (An) = [Read, Prefetch, Alu, Write] = 12 (read-then-write)"
        );
        assert_clr_b_4216_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_clr_b_4216() {
        let (mut rtc, mut bus_rtc) = setup_clr_b_4216();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_clr_b_4216();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_clr_b_4216_final(&step, &bus_step);
    }

    #[test]
    fn clr_b_4216_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor across CLR (An) — the READ-then-WRITE RMW shape (read the discarded EA,
        // Move-of-zero parks the 0, write it back). Snapshot at every bus-access boundary and resume.
        let (mut rref, mut bref) = setup_clr_b_4216();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Write) → boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_clr_b_4216();
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

    #[test]
    fn clr_decode_recognizes_opcode_and_size() {
        // CLR is 0x4200/4240/4280 (SS bits 7-6 = b/w/l); SS == 3 (0x42C0) is illegal on the 68000, not CLR.
        // Decode must produce a recipe (no panic) for the data-alterable modes.
        for op in [0x4282u16, 0x4216, 0x4261, 0x429B] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            // Just exercising the decode arm — it must not panic / hit the todo!() fallthrough.
            let _ = decode(&regs);
        }
    }

    // --- N6: MOVEQ (`0111 ddd 0 dddddddd`, 0x7000 | dn<<9 | imm8, bit 8 = 0) — load a sign-extended 8-bit
    // immediate into the FULL 32 bits of Dn, setting N = msb / Z = (value == 0), clearing V/C, PRESERVING X.
    // The single-bus-event shape: `Alu{Move, Long, a: BranchDisp8, dst: DataReg(Dn)}` + `Prefetch` (4 cyc, one
    // FC-6 queue refill). The immediate is the opcode's own low byte (`Operand::BranchDisp8`,
    // `sign_extend8(prefetch[0] & 0xFF)`). Anchor pinned to the vendored MOVE.q stream. ---

    /// The clean SST anchor `7cb5 [MOVE.q Q, D6]` (4 cyc): MOVEQ #0xB5,D6. imm8 0xB5 sign-extends to
    /// 0xFFFFFFB5; the full 32 bits land in D6. The msb is set so N=1, the value is nonzero so Z=0, V/C cleared,
    /// X PRESERVED (SR 0x270E → 0x2708, X clear here). Bus: one FC-6 refill at 3076 (no operand access — the
    /// immediate is the opcode's own low byte). The sign-extension anchor.
    fn setup_moveq_7cb5() -> (Cpu68000, FlatBus) {
        let mut regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 2048,
            pc: 3072,
            sr: 0x270E, // low byte 0x0E = N | Z | V set (X clear at bit4) — N kept, Z/V cleared, X preserved
            prefetch: [0x7CB5, 32174],
        };
        regs.d[6] = 0xF075_AFE6; // overwritten in full by MOVEQ
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x4B);
        bus.poke(3077, 0x1D); // 0x4B1D = 19229 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_moveq_7cb5_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 19229,
        }]
    }

    fn assert_moveq_7cb5_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.d[6], 0xFFFF_FFB5,
            "D6 = sign_extend8(0xB5) = 0xFFFFFFB5 (full 32 bits)"
        );
        assert_eq!(
            cpu.regs.sr, 0x2708,
            "MOVEQ set N (msb), cleared Z/V/C, PRESERVED X"
        );
        assert_eq!(cpu.regs.prefetch, [32174, 19229], "queue advanced");
        assert_eq!(bus.log, expected_moveq_7cb5_log());
    }

    #[test]
    fn run_instruction_matches_moveq_7cb5() {
        let (mut cpu, mut bus) = setup_moveq_7cb5();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 4, "MOVEQ = [Alu, Prefetch] = 4 (one FC-6 refill)");
        assert_moveq_7cb5_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_moveq_7cb5() {
        let (mut rtc, mut bus_rtc) = setup_moveq_7cb5();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_moveq_7cb5();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 4);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_moveq_7cb5_final(&step, &bus_step);
    }

    #[test]
    fn moveq_7cb5_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor across MOVEQ — the flag-setting register load (Move-of-sign-extended-imm
        // into Dn, then the queue refill). Snapshot at every bus-access boundary and resume.
        let (mut rref, mut bref) = setup_moveq_7cb5();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 2 micro-ops (Alu, Prefetch) → boundaries after 0..=1.
        for pause_after in 0..=1 {
            let (mut cpu, mut bus) = setup_moveq_7cb5();
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

    #[test]
    fn moveq_decode_recognizes_opcode() {
        // MOVEQ is `0111 ddd 0 dddddddd` (0x7000 | dn<<9 | imm8); bit 8 must be 0. Decode must produce a recipe
        // (no panic) for representative Dn / immediate combinations.
        for op in [0x7CB5u16, 0x7004, 0x7AF3, 0x70DE, 0x701E, 0x7E00] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            // Just exercising the decode arm — it must not panic / hit the todo!() fallthrough.
            let _ = decode(&regs);
        }
    }

    // --- L0: ADDA.w / ADDA.l — the no-flag address arithmetic `An = An + src` (AluOp::Adda + Dest::AddrReg).
    // SR is UNTOUCHED; `.w` sign-extends the source word→long before the long-boundary add (mirroring MOVEA.w),
    // `.l` adds the full 32. An is written full-width. The recipe reuses `ea_src`: `.w` appends a uniform
    // trailing n4 idle (ADDA.w = MOVEA.w + 4 for every source mode), `.l` appends nothing (`ea_src_long`'s
    // built-in n4/n2 idle already equals ADD.l <ea>,Dn). All anchors pinned to the vendored ADDA.w/.l stream. ---

    /// The clean SST anchor `dac2 [ADDA.w D2,A5] 31` (8 cyc): a Dn-source word add where the SOURCE WORD's high
    /// bit is SET — pinning the internal sign-extension of `b`. D2 low word = 0x8269 (bit15 set) → sext16 =
    /// 0xFFFF8269 (negative addend); A5 = 0x78EB075B + 0xFFFF8269 = 0x78EA89C4 (full 32). NO flags touched (SR
    /// stays 0x271B). Bus: one FC-6 refill @3076 (Dn direct, no operand read), then the trailing n4 idle. This
    /// is the sign-extend-correctness-on-An anchor.
    fn setup_adda_w_dac2() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x7235_6A78,
                0x5274_B5D8,
                0x6277_8269,
                0x617C_BE3C,
                0x0954_B4A5,
                0x30B5_0681,
                0xAE3F_71C3,
                0x3128_AF20,
            ],
            a: [
                0x3C84_A650,
                0x9205_C601,
                0x5970_F14C,
                0x5D26_E858,
                0x7AF6_F484,
                0x78EB_075B,
                0x822F_1093,
            ],
            usp: 23_865_738,
            ssp: 2048,
            pc: 3072,
            sr: 10011, // 0x271B
            prefetch: [0xDAC2, 0x228B],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x27);
        bus.poke(3077, 0x60); // 0x2760 = 10080 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_adda_w_dac2_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 10080,
        }]
    }

    fn assert_adda_w_dac2_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 10011, "ADDA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[5], 0x78EA_89C4,
            "A5 = 0x78EB075B + sext16(0x8269) = 0x78EA89C4 (negative addend, full 32)"
        );
        assert_eq!(cpu.regs.d[2], 0x6277_8269, "source Dn unchanged");
        assert_eq!(cpu.regs.prefetch, [0x228B, 10080], "queue advanced");
        assert_eq!(bus.log, expected_adda_w_dac2_log());
    }

    #[test]
    fn run_instruction_matches_adda_w_dac2() {
        let (mut cpu, mut bus) = setup_adda_w_dac2();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "ADDA.w Dn = [Prefetch, Alu, Internal(4)] = 4+0+4 = 8 (MOVEA.w Dn 4 + n4)"
        );
        assert_adda_w_dac2_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_adda_w_dac2() {
        let (mut rtc, mut bus_rtc) = setup_adda_w_dac2();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_adda_w_dac2();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_adda_w_dac2_final(&step, &bus_step);
    }

    /// The clean SST anchor `d4cd [ADDA.w A5,A2] 3` (8 cyc): an An-SOURCE word add (An-direct is legal — ADDA
    /// is address arithmetic). A5 low word = 0x0490 → sext16 = 0x00000490; A2 = 0x906FFB62 + 0x490 = 0x907062F0
    /// (full 32). NO flags. Bus: one FC-6 refill (An direct, no operand read), then the trailing n4 idle.
    fn setup_adda_w_d4cd() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x83E4_F829,
                0x2A53_D0F9,
                0x6439_758C,
                0x7140_98C5,
                0x99A3_4C69,
                0x57DE_0490,
                0xB0A4_A5AF,
                0xC8C6_FA98,
            ],
            a: [
                0x18F0_88E5,
                0x43D7_BF5A,
                0x906F_FB62,
                0xECB3_6364,
                0x6FCC_ABC8,
                0xA729_678E,
                0x7099_AFCD,
            ],
            usp: 3_711_164_204,
            ssp: 2048,
            pc: 3072,
            sr: 10009, // 0x2719
            prefetch: [0xD4CD, 0xE207],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x87);
        bus.poke(3077, 0xA0); // 0x87A0 = 34720 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn assert_adda_w_d4cd_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 10009, "ADDA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[2], 0x9070_62F0,
            "A2 = 0x906FFB62 + sext16(0x0490) = 0x907062F0 (full 32)"
        );
        assert_eq!(cpu.regs.a[5], 0xA729_678E, "source An unchanged");
        assert_eq!(cpu.regs.prefetch, [0xE207, 34720], "queue advanced");
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 34720,
            }]
        );
    }

    #[test]
    fn run_instruction_matches_adda_w_d4cd() {
        let (mut cpu, mut bus) = setup_adda_w_d4cd();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 8, "ADDA.w An = [Prefetch, Alu, Internal(4)] = 8");
        assert_adda_w_d4cd_final(&cpu, &bus);
    }

    /// The clean SST anchor `d6d1 [ADDA.w (A1),A3] 1` (12 cyc): a MEMORY-source word add. The source word is
    /// read at `(A1)` (A1 = 0xFCB6996E, masked to 0xB6996E = 11966830), refilled, then `A3 + sext16(src)`. Src
    /// word = 0x9FB1 → sext = 0xFFFF9FB1; A3 = 0x14511DB3 + 0xFFFF9FB1 = 0x1450BD64 (full 32). NO flags. Bus:
    /// `[r 11966830, PF, n4]`. This exercises the memory-read + Adda/AddrReg + trailing-n4 composition.
    fn setup_adda_w_d6d1() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x08A7_63A9,
                0x4E1C_13FA,
                0xB48C_3357,
                0x59FA_7A6C,
                0x85FF_470D,
                0x7003_FD7B,
                0x9265_DDD3,
                0x334F_21F8,
            ],
            a: [
                0x25BC_72FB,
                0xFCB6_996E,
                0x8B01_9903,
                0x1451_1DB3,
                0x301C_3FBB,
                0x720F_4721,
                0x1FE9_923E,
            ],
            usp: 1_030_979_142,
            ssp: 2048,
            pc: 3072,
            sr: 9998, // 0x270E
            prefetch: [0xD6D1, 0x82EF],
        };
        let mut bus = FlatBus::new();
        bus.poke(11_966_830, 0x9F);
        bus.poke(11_966_831, 0xB1); // 0x9FB1 = 40881 — the source word at (A1)
        bus.poke(3076, 0x68);
        bus.poke(3077, 0xBA); // 0x68BA = 26810 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_adda_w_d6d1_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 11_966_830,
                size: Size::Word,
                value: 40881,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 26810,
            },
        ]
    }

    fn assert_adda_w_d6d1_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 9998, "ADDA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[3], 0x1450_BD64,
            "A3 = 0x14511DB3 + sext16(0x9FB1) = 0x1450BD64 (full 32)"
        );
        assert_eq!(cpu.regs.a[1], 0xFCB6_996E, "source base A1 unchanged");
        assert_eq!(cpu.regs.prefetch, [0x82EF, 26810], "queue advanced");
        assert_eq!(bus.log, expected_adda_w_d6d1_log());
    }

    #[test]
    fn run_instruction_matches_adda_w_d6d1() {
        let (mut cpu, mut bus) = setup_adda_w_d6d1();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "ADDA.w (An) = [Read, Prefetch, Alu, Internal(4)] = 4+4+0+4 = 12"
        );
        assert_adda_w_d6d1_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_adda_w_d6d1() {
        let (mut rtc, mut bus_rtc) = setup_adda_w_d6d1();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_adda_w_d6d1();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_adda_w_d6d1_final(&step, &bus_step);
    }

    #[test]
    fn adda_w_d6d1_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the ADDA shape (Alu{Adda} with Dest::AddrReg, memory source + the
        // trailing n4 idle). Snapshot the whole CPU at every micro-op boundary, restore, resume, and match.
        let (mut rref, mut bref) = setup_adda_w_d6d1();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Internal(4)) → boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_adda_w_d6d1();
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

    /// The clean SST anchor `d7d1 [ADDA.l (A1),A3] 18` (14 cyc): a MEMORY-source LONG add — proves the `.l`
    /// path reuses `ea_src_long`'s n2 memory idle (NOT the .w n4). The long operand is read at `(A1)` (A1 =
    /// 0xF5CEE514, masked to 0xCEE514 = 13559060): hi @13559060 = 0x1FA8, lo @13559062 = 0x0685 → 0x1FA80685;
    /// A3 = 0x021B8735 + 0x1FA80685 = 0x21C38DBA (full 32). NO flags. Bus: `[r.hi, r.lo, PF, n2]`.
    fn setup_adda_l_d7d1() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x3CDC_F03D,
                0x4D87_B7B9,
                0x0619_DB27,
                0xF739_488D,
                0x02EF_FF76,
                0x12F7_3F9E,
                0x5831_42FB,
                0x9A83_6DD6,
            ],
            a: [
                0x4879_A2C8,
                0xF5CE_E514,
                0x035B_70E2,
                0x021B_8735,
                0xF543_8C05,
                0x188E_918A,
                0xAC1D_912A,
            ],
            usp: 402_311_418,
            ssp: 2048,
            pc: 3072,
            sr: 0x2710,
            prefetch: [0xD7D1, 0xA969],
        };
        let mut bus = FlatBus::new();
        bus.poke(13_559_060, 0x1F);
        bus.poke(13_559_061, 0xA8); // hi word 0x1FA8 = 8104
        bus.poke(13_559_062, 0x06);
        bus.poke(13_559_063, 0x85); // lo word 0x0685 = 1669
        bus.poke(3076, 0xCC);
        bus.poke(3077, 0x46); // 0xCC46 = 52294 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_adda_l_d7d1_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 13_559_060,
                size: Size::Word,
                value: 8104,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 13_559_062,
                size: Size::Word,
                value: 1669,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 52294,
            },
        ]
    }

    fn assert_adda_l_d7d1_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 0x2710, "ADDA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[3], 0x21C3_8DBA,
            "A3 = 0x021B8735 + 0x1FA80685 = 0x21C38DBA (full 32, no sign-extend for .l)"
        );
        assert_eq!(cpu.regs.a[1], 0xF5CE_E514, "source base A1 unchanged");
        assert_eq!(cpu.regs.prefetch, [0xA969, 52294], "queue advanced");
        assert_eq!(bus.log, expected_adda_l_d7d1_log());
    }

    #[test]
    fn run_instruction_matches_adda_l_d7d1() {
        let (mut cpu, mut bus) = setup_adda_l_d7d1();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 14,
            "ADDA.l (An) = [Read.hi, Read.lo, Prefetch, Alu, Internal(2)] = 4+4+4+0+2 = 14 (n2 memory idle, \
             NOT the .w n4)"
        );
        assert_adda_l_d7d1_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_adda_l_d7d1() {
        let (mut rtc, mut bus_rtc) = setup_adda_l_d7d1();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_adda_l_d7d1();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_adda_l_d7d1_final(&step, &bus_step);
    }

    /// The clean SST anchor `d5fc [ADDA.l #,A2] 35` (16 cyc): a `#imm.l` LONG add — proves the `.l` `#imm`
    /// path's trailing n4 idle (the register/immediate long idle, NOT the n2 memory idle). The 32-bit immediate
    /// is two extension words: HI = prefetch[1] = 0xF893 (captured before the refill shifts it out), LO = the
    /// first refill word @3076 = 0xCE23 → imm.l = 0xF893CE23. A2 = 0xA75C1A2D + 0xF893CE23 = 0x9FEFE850 (full
    /// 32, computed at the long boundary). NO flags. Bus: `[PF, PF, PF, n4]` (3 refills complete the 3-word
    /// fetch, no operand read).
    fn setup_adda_l_d5fc() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xDB59_BD87,
                0xE626_4771,
                0x65E7_E42A,
                0x2D15_C933,
                0x0E0C_CFC9,
                0x714A_6058,
                0x790D_5601,
                0xCDE6_C181,
            ],
            a: [
                0xA0F6_83E4,
                0x5DF3_49F2,
                0xA75C_1A2D,
                0xD475_C53E,
                0x61CE_9188,
                0xC3B8_4994,
                0x6C71_2DC5,
            ],
            usp: 778_566_730,
            ssp: 2048,
            pc: 3072,
            sr: 0x2700,
            prefetch: [0xD5FC, 0xF893],
        };
        let mut bus = FlatBus::new();
        // imm.l = (prefetch[1]=0xF893 << 16) | (first refill @3076 = 0xCE23). The two further refills complete
        // the 3-word instruction fetch.
        bus.poke(3076, 0xCE);
        bus.poke(3077, 0x23); // 0xCE23 = 52771 — imm LO word (the first refill)
        bus.poke(3078, 0xAC);
        bus.poke(3079, 0x67); // 0xAC67 = 44135 — the second refill word
        bus.poke(3080, 0xE9);
        bus.poke(3081, 0x52); // 0xE952 = 59730 — the third (final) refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_adda_l_d5fc_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 52771,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3078,
                size: Size::Word,
                value: 44135,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3080,
                size: Size::Word,
                value: 59730,
            },
        ]
    }

    fn assert_adda_l_d5fc_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3078,
            "pc advanced THREE words (3-word instruction)"
        );
        assert_eq!(cpu.regs.sr, 0x2700, "ADDA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[2], 0x9FEF_E850,
            "A2 = 0xA75C1A2D + imm.l 0xF893CE23 = 0x9FEFE850 (full 32)"
        );
        assert_eq!(cpu.regs.prefetch, [0xAC67, 59730], "queue advanced");
        assert_eq!(bus.log, expected_adda_l_d5fc_log());
    }

    #[test]
    fn run_instruction_matches_adda_l_d5fc() {
        let (mut cpu, mut bus) = setup_adda_l_d5fc();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 16,
            "ADDA.l #imm.l = [Combine32, PF, Combine32, PF, PF, Alu, Internal(4)] = 12 + n4 = 16"
        );
        assert_adda_l_d5fc_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_adda_l_d5fc() {
        let (mut rtc, mut bus_rtc) = setup_adda_l_d5fc();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_adda_l_d5fc();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 16);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_adda_l_d5fc_final(&step, &bus_step);
    }

    /// The SST anchor `d6d0 [ADDA.w (A0),A3] 30` (50 cyc): an ODD-EA ADDA — A0 = 0xDDBBDB63 is ODD (masked
    /// 0xBBDB63), so the word operand read faults and the in-flight `MicroState` is rewritten into the group-0
    /// 14-byte address-error frame to vector 3 (@0x0C). All supervisor (S=1, T=0, SR 0x271E): the frame stacks
    /// `PC = 3072` (live regs.pc, no prefetch ran), `SR = 0x271E`, `IR = 0xD6D0`, `SSW = 0xD6D5`, the full
    /// 32-bit access address 0xDDBBDB63. Vector @0x0C = 0x00001400; handler @5120. This proves an odd ADDA EA
    /// is IN scope (the E3 abort handles it — no parity filter).
    fn setup_adda_odd_d6d0() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xC6E5_A13E,
                0xD515_EFAC,
                0xF983_3A87,
                0x87CD_ABAD,
                0x88CE_16B9,
                0x7696_28B7,
                0x48E0_7F6B,
                0xBF85_B5A5,
            ],
            a: [
                0xDDBB_DB63, // A0 — ODD masked EA (0xBBDB63)
                0x8893_AA27,
                0x08C6_BD63,
                0x2C6F_68E4,
                0xF8CF_742A,
                0x78F7_33FB,
                0x60A8_773F,
            ],
            usp: 3_379_605_804,
            ssp: 2048,
            pc: 3072,
            sr: 0x271E, // S=1, T=0 (supervisor)
            prefetch: [0xD6D0, 0xFEA9],
        };
        let mut bus = FlatBus::new();
        for (a, v) in [
            // The vector-3 longword @0x0C: 0x00001400 (hi 0x0000 @12, lo 0x1400 = 5120 @14).
            (12u32, 0u8),
            (13, 0),
            (14, 20),
            (15, 0),
            // The handler code @5120: 0x8E44, 0xA9F6.
            (5120, 0x8E),
            (5121, 0x44),
            (5122, 0xA9),
            (5123, 0xF6),
        ] {
            bus.poke(a, v);
        }
        (Cpu68000::new(regs), bus)
    }

    fn expected_adda_odd_d6d0_log() -> Vec<Transaction> {
        // The group-0 14-byte frame writes (PCL @2046, SR @2042, PCH @2044, IR @2040, then aLo/SSW/aHi at
        // 2038/2034/2036 — the on-bus microcode order), the vector-3 fetch @12/14, then the handler reload @5120.
        vec![
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3072,
            }, // PCL
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2042,
                size: Size::Word,
                value: 10014,
            }, // SR
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2044,
                size: Size::Word,
                value: 0,
            }, // PCH
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2040,
                size: Size::Word,
                value: 54992,
            }, // IR (0xD6D0)
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2038,
                size: Size::Word,
                value: 56163,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2034,
                size: Size::Word,
                value: 54997,
            }, // SSW (0xD6D5)
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2036,
                size: Size::Word,
                value: 56763,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 12,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 14,
                size: Size::Word,
                value: 5120,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5120,
                size: Size::Word,
                value: 36420,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5122,
                size: Size::Word,
                value: 43510,
            },
        ]
    }

    #[test]
    fn run_instruction_matches_adda_odd_d6d0() {
        let (mut cpu, mut bus) = setup_adda_odd_d6d0();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 50,
            "odd-EA ADDA → group-0 14-byte address-error frame (50 cyc)"
        );
        assert_eq!(cpu.regs.pc, 5120, "pc landed at the vector-3 handler");
        assert_eq!(
            cpu.regs.ssp, 2034,
            "SSP pushed down by 14 (the group-0 frame)"
        );
        assert_eq!(
            cpu.regs.a[3], 0x2C6F_68E4,
            "An (the dest) unchanged — the add never committed"
        );
        assert_eq!(
            cpu.regs.sr, 0x271E,
            "SR unchanged (already S=1/T=0 — the entry transform is a no-op on the data)"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [0x8E44, 0xA9F6],
            "queue reloaded at the handler"
        );
        assert_eq!(bus.log, expected_adda_odd_d6d0_log());
    }

    #[test]
    fn both_drivers_match_adda_odd_d6d0() {
        let (mut rtc, mut bus_rtc) = setup_adda_odd_d6d0();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_adda_odd_d6d0();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 50);
        assert_eq!(step.regs, rtc.regs, "drivers agree across the abort");
        assert_eq!(
            bus_step.log, bus_rtc.log,
            "drivers agree on the frame transactions"
        );
    }

    #[test]
    fn adda_decode_classifies_and_sizes() {
        // ADDA is opmode 3 (.w = 0xD0C0) / 7 (.l = 0xD1C0) of the 0xD nibble — its own decode arms, disjoint
        // from the ADD arms (opmode 0/1/2/4/5/6). Decode must produce a recipe (no panic / no todo!()).
        for op in [0xDAC2u16, 0xD4CD, 0xD6D1, 0xD7D1, 0xD5FC, 0xD0C0, 0xD1C0] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            let _ = decode(&regs);
        }
    }

    // --- L1: SUBA.w / SUBA.l — the no-flag address arithmetic `An = An - src` (AluOp::Suba + Dest::AddrReg),
    // a near-exact mirror of L0's ADDA. SR is UNTOUCHED; `.w` sign-extends the source word->long before the
    // long-boundary SUBTRACT (mirroring MOVEA.w / CMPA.w), `.l` subtracts the full 32. An is written full-width.
    // The recipe REUSES the AluOp-parameterized `adda_suba_recipe`: `.w` appends a uniform trailing n4 idle
    // (SUBA.w = MOVEA.w + 4 for every source mode), `.l` appends nothing (`ea_src_long`'s built-in n4/n2 idle
    // already equals ADD.l <ea>,Dn). All anchors pinned to the vendored SUBA.w/.l stream. ---

    /// The clean SST anchor `94c4 [SUBA.w D4,A2] 3` (8 cyc): a Dn-source word subtract where the SOURCE WORD's
    /// high bit is SET — pinning the internal sign-extension of `b`. D4 low word = 0xA67B (bit15 set) -> sext16 =
    /// 0xFFFFA67B (negative subtrahend); A2 = 0x67BEFCE0 - 0xFFFFA67B = 0x67BF5665 (full 32). NO flags touched (SR stays
    /// 0x270B). Bus: one FC-6 refill @3076 (Dn direct, no operand read), then the trailing n4 idle. This is the
    /// sign-extend-correctness-on-An anchor.
    fn setup_suba_w_94c4() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x02D5_038F,
                0x90B6_491C,
                0xBBAD_0B0D,
                0x8068_80C1,
                0xE217_A67B,
                0x2E72_743B,
                0x06A0_B6BF,
                0x77B8_CF4D,
            ],
            a: [
                0x62D9_83A3,
                0x97CD_2D03,
                0x67BE_FCE0,
                0xF617_2098,
                0xA43E_2F5C,
                0xE6F5_BC23,
                0xE35D_0847,
            ],
            usp: 939291368,
            ssp: 2048,
            pc: 3072,
            sr: 0x270B,
            prefetch: [0x94C4, 0xFF79],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0xA7);
        bus.poke(3077, 0x06);
        (Cpu68000::new(regs), bus)
    }

    fn expected_suba_w_94c4_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 42758,
        }]
    }

    fn assert_suba_w_94c4_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 0x270B, "SUBA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[2], 0x67BF_5665,
            "A2 = 0x67BEFCE0 - sext16(0xA67B) = 0x67BF5665 (negative subtrahend, full 32)"
        );
        assert_eq!(cpu.regs.d[4], 0xE217_A67B, "source Dn unchanged");
        assert_eq!(cpu.regs.prefetch, [0xFF79, 42758], "queue advanced");
        assert_eq!(bus.log, expected_suba_w_94c4_log());
    }

    #[test]
    fn run_instruction_matches_suba_w_94c4() {
        let (mut cpu, mut bus) = setup_suba_w_94c4();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "SUBA.w Dn = [Prefetch, Alu, Internal(4)] = 4+0+4 = 8 (MOVEA.w Dn 4 + n4)"
        );
        assert_suba_w_94c4_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_suba_w_94c4() {
        let (mut rtc, mut bus_rtc) = setup_suba_w_94c4();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_suba_w_94c4();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_suba_w_94c4_final(&step, &bus_step);
    }

    /// The clean SST anchor `92c8 [SUBA.w A0,A1] 19` (8 cyc): an An-SOURCE word subtract (An-direct is legal —
    /// SUBA is address arithmetic). A0 low word = 0xCCEA -> sext16 = 0xFFFFCCEA; A1 = 0x0A763C42 - 0xFFFFCCEA = 0x0A766F58
    /// (full 32). NO flags. Bus: one FC-6 refill (An direct, no operand read), then the trailing n4 idle.
    fn setup_suba_w_92c8() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xACF8_4322,
                0xD958_E9CC,
                0xE4A0_BC26,
                0x783D_FAC8,
                0xF9B5_8AFD,
                0xC72C_F92F,
                0xEA85_78D3,
                0xAB5E_4BAE,
            ],
            a: [
                0xC250_CCEA,
                0x0A76_3C42,
                0x0E5B_C928,
                0x4C86_18FB,
                0xA8F1_E9C3,
                0x8D9A_5122,
                0xE883_3BDC,
            ],
            usp: 3713776246,
            ssp: 2048,
            pc: 3072,
            sr: 0x2703,
            prefetch: [0x92C8, 0x9B62],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x64);
        bus.poke(3077, 0xDF);
        (Cpu68000::new(regs), bus)
    }

    fn assert_suba_w_92c8_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 0x2703, "SUBA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[1], 0x0A76_6F58,
            "A1 = 0x0A763C42 - sext16(0xCCEA) = 0x0A766F58 (full 32)"
        );
        assert_eq!(cpu.regs.a[0], 0xC250_CCEA, "source An unchanged");
        assert_eq!(cpu.regs.prefetch, [0x9B62, 25823], "queue advanced");
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 25823,
            },]
        );
    }

    #[test]
    fn run_instruction_matches_suba_w_92c8() {
        let (mut cpu, mut bus) = setup_suba_w_92c8();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(cycles, 8, "SUBA.w An = [Prefetch, Alu, Internal(4)] = 8");
        assert_suba_w_92c8_final(&cpu, &bus);
    }

    /// The clean SST anchor `94d6 [SUBA.w (A6),A2] 30` (12 cyc): a MEMORY-source word subtract. The source word
    /// is read at `(A6)` (A6 = 0xFCC9230E, masked to 13181710), refilled, then `A2 - sext16(src)`. Src word = 0xB774 ->
    /// sext = 0xFFFFB774; A2 = 0x857D2A86 - 0xFFFFB774 = 0x857D7312 (full 32). NO flags. Bus: `[r 13181710, PF, n4]`. This exercises
    /// the memory-read + Suba/AddrReg + trailing-n4 composition.
    fn setup_suba_w_94d6() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x37C4_C64C,
                0x8061_628F,
                0x3A93_E2DB,
                0x5F70_2312,
                0x3884_83B7,
                0xE1DD_2EE9,
                0xB7B2_7C3D,
                0xF290_528D,
            ],
            a: [
                0x0E3A_7D42,
                0x073E_81F3,
                0x857D_2A86,
                0x75A5_052E,
                0xA234_276C,
                0x5FAB_4020,
                0xFCC9_230E,
            ],
            usp: 3862403486,
            ssp: 2048,
            pc: 3072,
            sr: 0x2717,
            prefetch: [0x94D6, 0xD5B6],
        };
        let mut bus = FlatBus::new();
        bus.poke(13181710, 0xB7);
        bus.poke(13181711, 0x74);
        bus.poke(3076, 0x86);
        bus.poke(3077, 0xED);
        (Cpu68000::new(regs), bus)
    }

    fn expected_suba_w_94d6_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 13181710,
                size: Size::Word,
                value: 46964,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 34541,
            },
        ]
    }

    fn assert_suba_w_94d6_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 0x2717, "SUBA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[2], 0x857D_7312,
            "A2 = 0x857D2A86 - sext16(0xB774) = 0x857D7312 (full 32)"
        );
        assert_eq!(cpu.regs.a[6], 0xFCC9_230E, "source base A6 unchanged");
        assert_eq!(cpu.regs.prefetch, [0xD5B6, 34541], "queue advanced");
        assert_eq!(bus.log, expected_suba_w_94d6_log());
    }

    #[test]
    fn run_instruction_matches_suba_w_94d6() {
        let (mut cpu, mut bus) = setup_suba_w_94d6();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "SUBA.w (An) = [Read, Prefetch, Alu, Internal(4)] = 4+4+0+4 = 12"
        );
        assert_suba_w_94d6_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_suba_w_94d6() {
        let (mut rtc, mut bus_rtc) = setup_suba_w_94d6();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_suba_w_94d6();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_suba_w_94d6_final(&step, &bus_step);
    }

    #[test]
    fn suba_w_94d6_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the SUBA shape (Alu{Suba} with Dest::AddrReg, memory source + the
        // trailing n4 idle). Snapshot the whole CPU at every micro-op boundary, restore, resume, and match.
        let (mut rref, mut bref) = setup_suba_w_94d6();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Internal(4)) -> boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_suba_w_94d6();
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

    /// The clean SST anchor `9bd2 [SUBA.l (A2),A5] 43` (14 cyc): a MEMORY-source LONG subtract — proves the
    /// `.l` path reuses `ea_src_long`'s n2 memory idle (NOT the .w n4). The long operand is read at `(A2)` (A2 =
    /// 0x501198C6, masked to 1153222): hi @1153222 = 0x9FD0, lo @1153224 = 0xD310 -> 0x9FD0D310; A5 = 0xAD840283 - 0x9FD0D310 = 0x0DB32F73
    /// (full 32). NO flags. Bus: `[r.hi, r.lo, PF, n2]`.
    fn setup_suba_l_9bd2() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x9E28_14A9,
                0x2977_EA08,
                0x18D3_E760,
                0xB931_3918,
                0x6D9B_8CF1,
                0x7676_D59D,
                0x2058_E4C2,
                0x21CA_077B,
            ],
            a: [
                0xBC84_10F1,
                0x222C_701B,
                0x5011_98C6,
                0x3297_C8C2,
                0xBA5D_8EB9,
                0xAD84_0283,
                0x4521_8F73,
            ],
            usp: 1431626598,
            ssp: 2048,
            pc: 3072,
            sr: 0x2703,
            prefetch: [0x9BD2, 0x4EB3],
        };
        let mut bus = FlatBus::new();
        bus.poke(1153222, 0x9F);
        bus.poke(1153223, 0xD0);
        bus.poke(1153224, 0xD3);
        bus.poke(1153225, 0x10);
        bus.poke(3076, 0xF7);
        bus.poke(3077, 0x32);
        (Cpu68000::new(regs), bus)
    }

    fn expected_suba_l_9bd2_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 1153222,
                size: Size::Word,
                value: 40912,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 1153224,
                size: Size::Word,
                value: 54032,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 63282,
            },
        ]
    }

    fn assert_suba_l_9bd2_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.sr, 0x2703, "SUBA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[5], 0x0DB3_2F73,
            "A5 = 0xAD840283 - 0x9FD0D310 = 0x0DB32F73 (full 32, no sign-extend for .l)"
        );
        assert_eq!(cpu.regs.a[2], 0x5011_98C6, "source base A2 unchanged");
        assert_eq!(cpu.regs.prefetch, [0x4EB3, 63282], "queue advanced");
        assert_eq!(bus.log, expected_suba_l_9bd2_log());
    }

    #[test]
    fn run_instruction_matches_suba_l_9bd2() {
        let (mut cpu, mut bus) = setup_suba_l_9bd2();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 14,
            "SUBA.l (An) = [Read.hi, Read.lo, Prefetch, Alu, Internal(2)] = 4+4+4+0+2 = 14 (n2 memory idle, \
             NOT the .w n4)"
        );
        assert_suba_l_9bd2_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_suba_l_9bd2() {
        let (mut rtc, mut bus_rtc) = setup_suba_l_9bd2();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_suba_l_9bd2();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 14);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_suba_l_9bd2_final(&step, &bus_step);
    }

    /// The clean SST anchor `99fc [SUBA.l #,A4] 212` (16 cyc): a `#imm.l` LONG subtract — proves the `.l` `#imm`
    /// path's trailing n4 idle (the register/immediate long idle, NOT the n2 memory idle). The 32-bit immediate
    /// is two extension words: HI = prefetch[1] = 0xD54C (captured before the refill shifts it out), LO = the
    /// first refill word @3076 = 0xCB60 -> imm.l = 0xD54CCB60. A4 = 0x46A731A6 - 0xD54CCB60 = 0x715A6646 (full 32, computed at
    /// the long boundary). NO flags. Bus: `[PF, PF, PF, n4]` (3 refills complete the 3-word fetch, no operand
    /// read).
    fn setup_suba_l_99fc() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xF4B3_8C22,
                0x1DE3_A5AF,
                0xFCA8_5E7F,
                0x82A4_54B6,
                0x9EC3_E429,
                0x3B29_6A14,
                0x1CA5_2473,
                0xB4A5_0B79,
            ],
            a: [
                0x6BE6_3B58,
                0x0A11_57D4,
                0x412E_8109,
                0xC3F5_5157,
                0x46A7_31A6,
                0x3FAF_DD03,
                0xC1C8_326D,
            ],
            usp: 3573321902,
            ssp: 2048,
            pc: 3072,
            sr: 0x271B,
            prefetch: [0x99FC, 0xD54C],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0xCB);
        bus.poke(3077, 0x60);
        bus.poke(3078, 0xED);
        bus.poke(3079, 0xD6);
        bus.poke(3080, 0x3C);
        bus.poke(3081, 0x13);
        (Cpu68000::new(regs), bus)
    }

    fn expected_suba_l_99fc_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 52064,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3078,
                size: Size::Word,
                value: 60886,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3080,
                size: Size::Word,
                value: 15379,
            },
        ]
    }

    fn assert_suba_l_99fc_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(
            cpu.regs.pc, 3078,
            "pc advanced THREE words (3-word instruction)"
        );
        assert_eq!(cpu.regs.sr, 0x271B, "SUBA touches NO flags — SR unchanged");
        assert_eq!(
            cpu.regs.a[4], 0x715A_6646,
            "A4 = 0x46A731A6 - imm.l 0xD54CCB60 = 0x715A6646 (full 32)"
        );
        assert_eq!(cpu.regs.prefetch, [0xEDD6, 15379], "queue advanced");
        assert_eq!(bus.log, expected_suba_l_99fc_log());
    }

    #[test]
    fn run_instruction_matches_suba_l_99fc() {
        let (mut cpu, mut bus) = setup_suba_l_99fc();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 16,
            "SUBA.l #imm.l = [Combine32, PF, Combine32, PF, PF, Alu, Internal(4)] = 12 + n4 = 16"
        );
        assert_suba_l_99fc_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_suba_l_99fc() {
        let (mut rtc, mut bus_rtc) = setup_suba_l_99fc();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_suba_l_99fc();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 16);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_suba_l_99fc_final(&step, &bus_step);
    }

    /// The SST anchor `9cd2 [SUBA.w (A2),A6] 24` (50 cyc): an ODD-EA SUBA — A2 = 0xD40B4BC5 is ODD (masked
    /// 0x0B4BC5), so the word operand read faults and the in-flight `MicroState` is rewritten into the group-0
    /// 14-byte address-error frame to vector 3 (@0x0C). All supervisor (S=1, T=0, SR 0x2719): the frame stacks
    /// `PC = 3072` (live regs.pc, no prefetch ran), `SR = 0x2719`, `IR = 0x9CD2`, `SSW = 0x9CD5`, the full
    /// 32-bit access address 0xD40B4BC5. Vector @0x0C = 0x00001400; handler @5120. This proves an odd SUBA EA is IN scope
    /// (the E3 abort handles it — no parity filter).
    fn setup_suba_odd_9cd2() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xE8B9_6135,
                0x1C28_F4E9,
                0x1EBD_1B1B,
                0xD68A_9EB6,
                0xD192_0083,
                0x3E8E_999C,
                0xDB96_BA1A,
                0xB649_E08D,
            ],
            a: [
                0xA547_B83E,
                0xF05F_4581,
                0xD40B_4BC5,
                0xC03D_7EE8,
                0x8E23_8FF8,
                0x3C0F_26DF,
                0x4000_8E36,
            ],
            usp: 2694348544,
            ssp: 2048,
            pc: 3072,
            sr: 0x2719,
            prefetch: [0x9CD2, 0xB33C],
        };
        let mut bus = FlatBus::new();
        bus.poke(12, 0x00);
        bus.poke(13, 0x00);
        bus.poke(14, 0x14);
        bus.poke(15, 0x00);
        bus.poke(5120, 0x8B);
        bus.poke(5121, 0x71);
        bus.poke(5122, 0x96);
        bus.poke(5123, 0xA3);
        (Cpu68000::new(regs), bus)
    }

    fn expected_suba_odd_9cd2_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2046,
                size: Size::Word,
                value: 3072,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2042,
                size: Size::Word,
                value: 10009,
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
                addr: 2040,
                size: Size::Word,
                value: 40146,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2038,
                size: Size::Word,
                value: 19397,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2034,
                size: Size::Word,
                value: 40149,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 2036,
                size: Size::Word,
                value: 54283,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 12,
                size: Size::Word,
                value: 0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 14,
                size: Size::Word,
                value: 5120,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5120,
                size: Size::Word,
                value: 35697,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 5122,
                size: Size::Word,
                value: 38563,
            },
        ]
    }

    #[test]
    fn run_instruction_matches_suba_odd_9cd2() {
        let (mut cpu, mut bus) = setup_suba_odd_9cd2();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 50,
            "odd-EA SUBA -> group-0 14-byte address-error frame (50 cyc)"
        );
        assert_eq!(cpu.regs.pc, 5120, "pc landed at the vector-3 handler");
        assert_eq!(
            cpu.regs.ssp, 2034,
            "SSP pushed down by 14 (the group-0 frame)"
        );
        assert_eq!(
            cpu.regs.a[6], 0x4000_8E36,
            "An (the dest) unchanged — the subtract never committed"
        );
        assert_eq!(
            cpu.regs.sr, 0x2719,
            "SR unchanged (already S=1/T=0 — the entry transform is a no-op on the data)"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [0x8B71, 0x96A3],
            "queue reloaded at the handler"
        );
        assert_eq!(bus.log, expected_suba_odd_9cd2_log());
    }

    #[test]
    fn both_drivers_match_suba_odd_9cd2() {
        let (mut rtc, mut bus_rtc) = setup_suba_odd_9cd2();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_suba_odd_9cd2();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 50);
        assert_eq!(step.regs, rtc.regs, "drivers agree across the abort");
        assert_eq!(
            bus_step.log, bus_rtc.log,
            "drivers agree on the frame transactions"
        );
    }

    #[test]
    fn suba_decode_classifies_and_sizes() {
        // SUBA is opmode 3 (.w = 0x90C0) / 7 (.l = 0x91C0) of the 0x9 nibble — its own decode arms, disjoint
        // from the SUB arms (opmode 0/1/2/4/5/6). Decode must produce a recipe (no panic / no todo!()).
        for op in [0x94C4u16, 0x92C8, 0x94D6, 0x9BD2, 0x99FC, 0x90C0, 0x91C0] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            let _ = decode(&regs);
        }
    }

    // --- L2: AND.b / AND.w / AND.l, BOTH directions — bitwise `a & b` with the MOVE flag shape (N = msb /
    // Z = (result == 0), V/C cleared, X PRESERVED), the new `AluOp::And`. `<ea>,Dn` reuses `arith_ea_dn`, `Dn,<ea>`
    // reuses `arith_dn_ea`, both VERBATIM (AluOp-parameterized; AND = ADD byte-for-byte minus the illegal An
    // source). Anchors pinned to real vendored AND.l/.w cases. ---

    /// The clean SST anchor `ce85 [AND.l D5,D7] 108` (8 cyc): a Dn-source LONG AND — the **X-PRESERVATION + N**
    /// pin. Initial CCR = X|N|Z (0x271c): D7 = 0xE29237B7 & D5 = 0xDFA6D51D = 0xC2821515 (msb set → N stays 1),
    /// Z cleared (result != 0), V/C cleared, **X PRESERVED** (X=1 in and out) → final CCR = X|N (0x2718). The
    /// AND.l register source uses `ea_src_long`'s Dn-direct path `[Prefetch, Alu, Internal(4)]` (8 cyc, the long
    /// register idle = ADD.l <ea>,Dn). Bus: one FC-6 refill @3076 (no operand read).
    fn setup_and_l_ce85() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x6B33_9A0C,
                0x1A77_2E51,
                0x90D4_B6A2,
                0x4C81_77E9,
                0x2F5A_C3B8,
                0xDFA6_D51D,
                0x71C8_4E0A,
                0xE292_37B7,
            ],
            a: [
                0x4821_5C3D,
                0x39B7_0C42,
                0x5D0E_91A6,
                0x6F22_88B4,
                0x1A4C_77E0,
                0x82E0_65DB,
                0x2783_E0EF,
            ],
            usp: 0x1049_922A,
            ssp: 2048,
            pc: 3072,
            sr: 0x271C, // CCR = X|N|Z
            prefetch: [0xCE85, 0x91D9],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x7F);
        bus.poke(3077, 0xAA); // 0x7FAA = 32682 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_and_l_ce85_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 32682,
        }]
    }

    fn assert_and_l_ce85_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.d[7], 0xC282_1515,
            "D7 = 0xE29237B7 & 0xDFA6D51D = 0xC2821515 (full 32)"
        );
        assert_eq!(cpu.regs.d[5], 0xDFA6_D51D, "source D5 unchanged");
        assert_eq!(
            cpu.regs.sr, 0x2718,
            "N stays set (msb), Z/V/C cleared, X PRESERVED → CCR = X|N (0x18)"
        );
        assert_eq!(cpu.regs.prefetch, [0x91D9, 32682], "queue advanced");
        assert_eq!(bus.log, expected_and_l_ce85_log());
    }

    #[test]
    fn run_instruction_matches_and_l_ce85() {
        let (mut cpu, mut bus) = setup_and_l_ce85();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "AND.l Dn,Dn = [Prefetch, Alu, Internal(4)] = 8 (ADD.l <ea>,Dn register idle)"
        );
        assert_and_l_ce85_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_and_l_ce85() {
        let (mut rtc, mut bus_rtc) = setup_and_l_ce85();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_and_l_ce85();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_and_l_ce85_final(&step, &bus_step);
    }

    #[test]
    fn and_l_ce85_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the AND register-source shape (Alu{And} setting N/Z, clearing V/C,
        // PRESERVING X). Snapshot the whole CPU at every micro-op boundary, restore, resume, and match.
        let (mut rref, mut bref) = setup_and_l_ce85();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Prefetch, Alu, Internal(4)) -> boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_and_l_ce85();
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

    /// The clean SST anchor `cf54 [AND.w D7,(A4)] 65` (12 cyc): a MEMORY-DEST word AND — the `Dn,<ea>` RMW path
    /// (`arith_dn_ea`) + the **V/C-CLEARING** pin. Initial CCR = X|V|C (0x2713): (A4) = 0xE9067256 → masked
    /// 0x067256 = 422486, the word there = 0x29D4; D7 low word = 0x620F → 0x29D4 & 0x620F = 0x2004 written back.
    /// Result msb clear → N=0, non-zero → Z=0, **V and C CLEARED** (were set), **X PRESERVED** → final CCR = X
    /// (0x2710). Bus order is the RMW `[r operand @(A4) FC5, r refill @3076 FC6, w result @(A4) FC5]`.
    fn setup_and_w_cf54() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x490F_1780,
                0xB82F_764F,
                0x1D41_0A1C,
                0xD1C4_5558,
                0x3CB6_0C42,
                0xFB74_9672,
                0xB2A9_FA73,
                0xC820_620F,
            ],
            a: [
                0xCC8D_316A,
                0x3044_2DFD,
                0x3179_3B70,
                0xFE47_092D,
                0xE906_7256,
                0x82E0_65DB,
                0xC96A_9590,
            ],
            usp: 0x1049_922A,
            ssp: 2048,
            pc: 3072,
            sr: 0x2713, // CCR = X|V|C
            prefetch: [0xCF54, 0x49D5],
        };
        let mut bus = FlatBus::new();
        bus.poke(422486, 0x29);
        bus.poke(422487, 0xD4); // (A4) word = 0x29D4 = 10708
        bus.poke(3076, 0x76);
        bus.poke(3077, 0x63); // refill 0x7663 = 30307
        (Cpu68000::new(regs), bus)
    }

    fn expected_and_w_cf54_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 422486,
                size: Size::Word,
                value: 10708,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 30307,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 422486,
                size: Size::Word,
                value: 8196,
            },
        ]
    }

    fn assert_and_w_cf54_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.a[4], 0xE906_7256, "dest base A4 unchanged");
        assert_eq!(cpu.regs.d[7], 0xC820_620F, "source D7 unchanged");
        assert_eq!(
            cpu.regs.sr, 0x2710,
            "N=0/Z=0, V and C CLEARED, X PRESERVED → CCR = X (0x10)"
        );
        assert_eq!(
            bus.peek(422486),
            0x20,
            "(A4) hi byte = 0x20 (0x29D4 & 0x620F = 0x2004)"
        );
        assert_eq!(bus.peek(422487), 0x04, "(A4) lo byte = 0x04");
        assert_eq!(cpu.regs.prefetch, [0x49D5, 30307], "queue advanced");
        assert_eq!(bus.log, expected_and_w_cf54_log());
    }

    #[test]
    fn run_instruction_matches_and_w_cf54() {
        let (mut cpu, mut bus) = setup_and_w_cf54();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "AND.w Dn,(An) = [Read, Prefetch, Alu, Write] RMW = 12 (ADD.w Dn,(An))"
        );
        assert_and_w_cf54_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_and_w_cf54() {
        let (mut rtc, mut bus_rtc) = setup_and_w_cf54();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_and_w_cf54();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_and_w_cf54_final(&step, &bus_step);
    }

    #[test]
    fn and_w_cf54_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the AND memory-dest RMW shape (Alu{And} parked in Scratch, then
        // written back) — the interesting mid-bus-access boundary between the operand Read and the result Write.
        let (mut rref, mut bref) = setup_and_w_cf54();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Write) -> boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_and_w_cf54();
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

    #[test]
    fn and_decode_classifies_and_sizes() {
        // AND `<ea>,Dn` is opmode 0/1/2 (0xC000/0xC040/0xC080) and `Dn,<ea>` is opmode 4/5/6 (0xC100/0xC140/
        // 0xC180) of the 0xC nibble — its own decode arms, disjoint from ADD/SUB (0xD/0x9) and CMP (0xB). The
        // ANDI immediate opcode (0x02xx, high nibble 0) is a DIFFERENT instruction NOT decoded here (it must
        // never reach decode — `covered()` classifies it out by opcode). Decode of the genuine register form
        // must produce a recipe (no panic / no todo!()).
        for op in [
            0xC801u16, // AND.b D1,D0 <ea>,Dn (Dn source)
            0xC614,    // AND.b (A4),D3
            0xC03C,    // AND.b #imm,D0
            0xC440,    // AND.w D0,D2
            0xC880,    // AND.l D0,D4
            0xC6BC,    // AND.l #imm,D3
            0xCF12,    // AND.b D7,(A2)   Dn,<ea>
            0xCF54,    // AND.w D7,(A4)
            0xCB92,    // AND.l D5,(A2)
        ] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            let _ = decode(&regs);
        }
    }

    // --- L3: OR.b / OR.w / OR.l, BOTH directions — bitwise `a | b` with the MOVE flag shape (N = msb /
    // Z = (result == 0), V/C cleared, X PRESERVED), the new `AluOp::Or`. Identical to L2's AND with the bit op
    // `|` and the base nibble 0x8 instead of 0xC; `<ea>,Dn` reuses `arith_ea_dn`, `Dn,<ea>` reuses `arith_dn_ea`,
    // both VERBATIM (AluOp-parameterized; OR = AND = ADD byte-for-byte). Anchors pinned to real vendored
    // OR.l/.w cases (the 0xC->0x8 mirror of L2's ce85/cf54). ---

    /// The clean SST anchor `8e85 [OR.l D5,D7] 1696` (8 cyc): a Dn-source LONG OR — the **X-PRESERVATION + N**
    /// pin. Initial CCR = X|N|C (0x2719): D7 = 0xDF761620 | D5 = 0xA640EDD4 = 0xFF76FFF4 (msb set → N stays 1),
    /// Z cleared (result != 0), V/C cleared (C was set → CLEARED), **X PRESERVED** (X=1 in and out) → final CCR =
    /// X|N (0x2718). The OR.l register source uses `ea_src_long`'s Dn-direct path `[Prefetch, Alu, Internal(4)]`
    /// (8 cyc, the long register idle = ADD.l/AND.l <ea>,Dn). Bus: one FC-6 refill @3076 (no operand read).
    fn setup_or_l_8e85() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x403B_C910,
                0x0000_0064,
                0xEB30_A0C3,
                0x465F_A886,
                0xBFBE_7145,
                0xA640_EDD4,
                0x6D3D_DBD9,
                0xDF76_1620,
            ],
            a: [
                0x6470_A010,
                0x3503_6FD8,
                0xFD8B_A1F5,
                0x1EF1_941E,
                0xB923_7ACA,
                0xFB69_6664,
                0xABF0_8AEA,
            ],
            usp: 0xCF6B_0994,
            ssp: 2048,
            pc: 3072,
            sr: 0x2719, // CCR = X|N|C
            prefetch: [0x8E85, 0x093C],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x31);
        bus.poke(3077, 0x8B); // 0x318B = 12683 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_or_l_8e85_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 12683,
        }]
    }

    fn assert_or_l_8e85_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.d[7], 0xFF76_FFF4,
            "D7 = 0xDF761620 | 0xA640EDD4 = 0xFF76FFF4 (full 32)"
        );
        assert_eq!(cpu.regs.d[5], 0xA640_EDD4, "source D5 unchanged");
        assert_eq!(
            cpu.regs.sr, 0x2718,
            "N stays set (msb), Z/V/C cleared (C was set → cleared), X PRESERVED → CCR = X|N (0x18)"
        );
        assert_eq!(cpu.regs.prefetch, [0x093C, 12683], "queue advanced");
        assert_eq!(bus.log, expected_or_l_8e85_log());
    }

    #[test]
    fn run_instruction_matches_or_l_8e85() {
        let (mut cpu, mut bus) = setup_or_l_8e85();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "OR.l Dn,Dn = [Prefetch, Alu, Internal(4)] = 8 (ADD.l/AND.l <ea>,Dn register idle)"
        );
        assert_or_l_8e85_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_or_l_8e85() {
        let (mut rtc, mut bus_rtc) = setup_or_l_8e85();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_or_l_8e85();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_or_l_8e85_final(&step, &bus_step);
    }

    #[test]
    fn or_l_8e85_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the OR register-source shape (Alu{Or} setting N/Z, clearing V/C,
        // PRESERVING X). Snapshot the whole CPU at every micro-op boundary, restore, resume, and match.
        let (mut rref, mut bref) = setup_or_l_8e85();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Prefetch, Alu, Internal(4)) -> boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_or_l_8e85();
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

    /// The clean SST anchor `8f54 [OR.w D7,(A4)] 617` (12 cyc): a MEMORY-DEST word OR — the `Dn,<ea>` RMW path
    /// (`arith_dn_ea`) + the **V/C-CLEARING** pin. Initial CCR = X|V|C (0x2713): (A4) = 0x8AFA98F0 → masked
    /// 0xFA98F0 = 16423152, the word there = 0xBDFB; D7 low word = 0xDA74 → 0xBDFB | 0xDA74 = 0xFFFF written
    /// back. Result msb set → N=1, non-zero → Z=0, **V and C CLEARED** (were set), **X PRESERVED** → final CCR =
    /// X|N (0x2718). Bus order is the RMW `[r operand @(A4) FC5, r refill @3076 FC6, w result @(A4) FC5]`.
    fn setup_or_w_8f54() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xF866_B18D,
                0x03F2_9606,
                0x5F3B_0646,
                0xEB48_61A6,
                0xCD70_02B2,
                0xBCA0_98DC,
                0x5C24_A364,
                0x46C7_DA74,
            ],
            a: [
                0x3469_AE7C,
                0x333D_1875,
                0xDEF8_BC7E,
                0xDDE5_96E5,
                0x8AFA_98F0,
                0x2A57_0F98,
                0x1380_56BC,
            ],
            usp: 0x164A_DAD8,
            ssp: 2048,
            pc: 3072,
            sr: 0x2713, // CCR = X|V|C
            prefetch: [0x8F54, 0xD061],
        };
        let mut bus = FlatBus::new();
        bus.poke(16423152, 0xBD);
        bus.poke(16423153, 0xFB); // (A4) word = 0xBDFB = 48635
        bus.poke(3076, 0x3D);
        bus.poke(3077, 0x78); // refill 0x3D78 = 15736
        (Cpu68000::new(regs), bus)
    }

    fn expected_or_w_8f54_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 16423152,
                size: Size::Word,
                value: 48635,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 15736,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 16423152,
                size: Size::Word,
                value: 65535,
            },
        ]
    }

    fn assert_or_w_8f54_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.a[4], 0x8AFA_98F0, "dest base A4 unchanged");
        assert_eq!(cpu.regs.d[7], 0x46C7_DA74, "source D7 unchanged");
        assert_eq!(
            cpu.regs.sr, 0x2718,
            "N=1 (msb), Z=0, V and C CLEARED, X PRESERVED → CCR = X|N (0x18)"
        );
        assert_eq!(
            bus.peek(16423152),
            0xFF,
            "(A4) hi byte = 0xFF (0xBDFB | 0xDA74 = 0xFFFF)"
        );
        assert_eq!(bus.peek(16423153), 0xFF, "(A4) lo byte = 0xFF");
        assert_eq!(cpu.regs.prefetch, [0xD061, 15736], "queue advanced");
        assert_eq!(bus.log, expected_or_w_8f54_log());
    }

    #[test]
    fn run_instruction_matches_or_w_8f54() {
        let (mut cpu, mut bus) = setup_or_w_8f54();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "OR.w Dn,(An) = [Read, Prefetch, Alu, Write] RMW = 12 (ADD.w/AND.w Dn,(An))"
        );
        assert_or_w_8f54_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_or_w_8f54() {
        let (mut rtc, mut bus_rtc) = setup_or_w_8f54();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_or_w_8f54();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_or_w_8f54_final(&step, &bus_step);
    }

    #[test]
    fn or_w_8f54_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the OR memory-dest RMW shape (Alu{Or} parked in Scratch, then written
        // back) — the interesting mid-bus-access boundary between the operand Read and the result Write.
        let (mut rref, mut bref) = setup_or_w_8f54();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Write) -> boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_or_w_8f54();
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

    #[test]
    fn or_decode_classifies_and_sizes() {
        // OR `<ea>,Dn` is opmode 0/1/2 (0x8000/0x8040/0x8080) and `Dn,<ea>` is opmode 4/5/6 (0x8100/0x8140/
        // 0x8180) of the 0x8 nibble — its own decode arms, disjoint from ADD/SUB (0xD/0x9), AND (0xC) and CMP
        // (0xB). The ORI immediate opcode (0x00xx, high nibble 0) is a DIFFERENT instruction NOT decoded here
        // (it must never reach decode — `covered()` classifies it out by opcode). Decode of the genuine
        // register form must produce a recipe (no panic / no todo!()).
        for op in [
            0x8801u16, // OR.b D1,D0 <ea>,Dn (Dn source)
            0x8614,    // OR.b (A4),D3
            0x803C,    // OR.b #imm,D0
            0x8440,    // OR.w D0,D2
            0x8880,    // OR.l D0,D4
            0x86BC,    // OR.l #imm,D3
            0x8F12,    // OR.b D7,(A2)   Dn,<ea>
            0x8F54,    // OR.w D7,(A4)
            0x8B92,    // OR.l D5,(A2)
        ] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            let _ = decode(&regs);
        }
    }

    // --- L4: EOR.b / EOR.w / EOR.l, `Dn,<ea>` ONLY — bitwise `a ^ b` with the MOVE flag shape (N = msb /
    // Z = (result == 0), V/C cleared, X PRESERVED), the new `AluOp::Eor`. EOR has NO `<ea>,Dn` form (opmode
    // 0/1/2 in 0xB is CMP); the dest is a data register (mode 000 = `Dn,Dn`, its own no-memory arm) or alterable
    // memory (modes 2..6/abs, via `arith_dn_ea` VERBATIM = ADD Dn,<ea>). Mode field 001 = CMPM, handled by the
    // `cmp_class` arm FIRST. Anchors pinned to real vendored EOR.l/.w cases. ---

    /// The clean SST anchor `b782 [EOR.l D3, D2] 1` (8 cyc): a REGISTER-dest LONG EOR — the `Dn,Dn` no-memory
    /// arm + the **`.l` trailing n4** pin. Initial CCR = X|V (0x12): D2 = 0x62222CB8 ^ D3 = 0x3C1A7F67 =
    /// 0x5E3853DF (msb clear → N=0), Z cleared (non-zero), V/C cleared (V was set → CLEARED), **X PRESERVED**
    /// (X=1 in and out) → final CCR = X (0x10). The recipe is `[Prefetch, Alu{Eor}, Internal(4)]` (8 cyc, the
    /// long register idle = ADD.l/AND.l <ea>,Dn). Bus: one FC-6 refill @3076 (no operand read — register dest).
    fn setup_eor_l_b782() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0xE664_9C6E,
                0x10EB_ACBD,
                0x6222_2CB8,
                0x3C1A_7F67,
                0x1C0F_6D81,
                0x5451_63D6,
                0x90B4_2E62,
                0x0EFA_8DA1,
            ],
            a: [
                0x2521_6622,
                0x3743_EF7F,
                0x8B93_EF4C,
                0x4DE2_59BE,
                0xD146_FDCC,
                0x919F_7ED3,
                0xA29D_3690,
            ],
            usp: 0x63CF_C3F8,
            ssp: 2048,
            pc: 3072,
            sr: 0x2712, // CCR = X|V
            prefetch: [0xB782, 0x5965],
        };
        let mut bus = FlatBus::new();
        bus.poke(3076, 0x8C);
        bus.poke(3077, 0x78); // 0x8C78 = 35960 — the refill word
        (Cpu68000::new(regs), bus)
    }

    fn expected_eor_l_b782_log() -> Vec<Transaction> {
        vec![Transaction {
            kind: TxKind::Read,
            fc: 6,
            addr: 3076,
            size: Size::Word,
            value: 35960,
        }]
    }

    fn assert_eor_l_b782_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(
            cpu.regs.d[2], 0x5E38_53DF,
            "D2 = 0x62222CB8 ^ 0x3C1A7F67 = 0x5E3853DF (full 32, register dest)"
        );
        assert_eq!(cpu.regs.d[3], 0x3C1A_7F67, "source D3 unchanged");
        assert_eq!(
            cpu.regs.sr, 0x2710,
            "N=0 (msb clear), Z=0, V CLEARED (was set), C=0, X PRESERVED → CCR = X (0x10)"
        );
        assert_eq!(cpu.regs.prefetch, [0x5965, 35960], "queue advanced");
        assert_eq!(bus.log, expected_eor_l_b782_log());
    }

    #[test]
    fn run_instruction_matches_eor_l_b782() {
        let (mut cpu, mut bus) = setup_eor_l_b782();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 8,
            "EOR.l Dn,Dn = [Prefetch, Alu, Internal(4)] = 8 (register-register long idle)"
        );
        assert_eor_l_b782_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_eor_l_b782() {
        let (mut rtc, mut bus_rtc) = setup_eor_l_b782();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_eor_l_b782();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 8);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_eor_l_b782_final(&step, &bus_step);
    }

    #[test]
    fn eor_l_b782_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the EOR register-dest shape (Alu{Eor} into Dn setting N/Z, clearing
        // V/C, PRESERVING X, then the `.l` n4 idle). Snapshot the whole CPU at every micro-op boundary, restore,
        // resume, and match.
        let (mut rref, mut bref) = setup_eor_l_b782();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 3 micro-ops (Prefetch, Alu, Internal(4)) -> boundaries after 0..=2.
        for pause_after in 0..=2 {
            let (mut cpu, mut bus) = setup_eor_l_b782();
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

    /// The clean SST anchor `b153 [EOR.w D0, (A3)] 24` (12 cyc): a MEMORY-DEST word EOR — the `Dn,<ea>` RMW path
    /// (`arith_dn_ea`) + the **V/C-CLEARING** pin. Initial CCR = X|N|V|C (0x271B): (A3) = 0x3D4F59B0 → masked
    /// 0x4F59B0 = 5200304, the word there = 0x286B; D0 low word = 0x12E5 → 0x286B ^ 0x12E5 = 0x3A8E written
    /// back. Result msb clear → N=0, non-zero → Z=0, **V and C CLEARED** (were set), **X PRESERVED** → final CCR
    /// = X (0x10). Bus order is the RMW `[r operand @(A3) FC5, r refill @3076 FC6, w result @(A3) FC5]`.
    fn setup_eor_w_b153() -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [
                0x465F_12E5,
                0x39C5_CB4C,
                0x5B55_DF80,
                0x9C3F_AA98,
                0xE9F4_F17A,
                0xEDB8_B925,
                0x977E_AB83,
                0x0513_0EF8,
            ],
            a: [
                0x169F_6C1B,
                0x52B2_F8F8,
                0x6B40_5E4F,
                0x3D4F_59B0,
                0x58CE_60DB,
                0xFCEF_8FEE,
                0xD573_80E2,
            ],
            usp: 0x7C30_1102,
            ssp: 2048,
            pc: 3072,
            sr: 0x271B, // CCR = X|N|V|C
            prefetch: [0xB153, 0x2901],
        };
        let mut bus = FlatBus::new();
        bus.poke(5200304, 0x28);
        bus.poke(5200305, 0x6B); // (A3) word = 0x286B = 10347
        bus.poke(3076, 0x13);
        bus.poke(3077, 0x8D); // refill 0x138D = 5005
        (Cpu68000::new(regs), bus)
    }

    fn expected_eor_w_b153_log() -> Vec<Transaction> {
        vec![
            Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 5200304,
                size: Size::Word,
                value: 10347,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 3076,
                size: Size::Word,
                value: 5005,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 5200304,
                size: Size::Word,
                value: 14990,
            },
        ]
    }

    fn assert_eor_w_b153_final(cpu: &Cpu68000, bus: &FlatBus) {
        assert_eq!(cpu.regs.pc, 3074, "pc advanced one word");
        assert_eq!(cpu.regs.a[3], 0x3D4F_59B0, "dest base A3 unchanged");
        assert_eq!(cpu.regs.d[0], 0x465F_12E5, "source D0 unchanged");
        assert_eq!(
            cpu.regs.sr, 0x2710,
            "N=0 (msb clear), Z=0, V and C CLEARED, X PRESERVED → CCR = X (0x10)"
        );
        assert_eq!(
            bus.peek(5200304),
            0x3A,
            "(A3) hi byte = 0x3A (0x286B ^ 0x12E5 = 0x3A8E)"
        );
        assert_eq!(bus.peek(5200305), 0x8E, "(A3) lo byte = 0x8E");
        assert_eq!(cpu.regs.prefetch, [0x2901, 5005], "queue advanced");
        assert_eq!(bus.log, expected_eor_w_b153_log());
    }

    #[test]
    fn run_instruction_matches_eor_w_b153() {
        let (mut cpu, mut bus) = setup_eor_w_b153();
        let cycles = cpu.run_instruction(&mut bus);
        assert_eq!(
            cycles, 12,
            "EOR.w Dn,(An) = [Read, Prefetch, Alu, Write] RMW = 12 (ADD.w/AND.w Dn,(An))"
        );
        assert_eor_w_b153_final(&cpu, &bus);
    }

    #[test]
    fn both_drivers_match_eor_w_b153() {
        let (mut rtc, mut bus_rtc) = setup_eor_w_b153();
        rtc.run_instruction(&mut bus_rtc);
        let (mut step, mut bus_step) = setup_eor_w_b153();
        step.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = step.step_micro_op(&mut bus_step) {
                break c;
            }
        };
        assert_eq!(cycles, 12);
        assert_eq!(step.regs, rtc.regs, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
        assert_eor_w_b153_final(&step, &bus_step);
    }

    #[test]
    fn eor_w_b153_quiescable_and_serializable_at_every_micro_op_boundary() {
        // The snapshot/restore anchor for the EOR memory-dest RMW shape (Alu{Eor} parked in Scratch, then
        // written back) — the interesting mid-bus-access boundary between the operand Read and the result Write.
        let (mut rref, mut bref) = setup_eor_w_b153();
        rref.run_instruction(&mut bref);
        let cfg = bincode::config::standard();
        // 4 micro-ops (Read, Prefetch, Alu, Write) -> boundaries after 0..=3.
        for pause_after in 0..=3 {
            let (mut cpu, mut bus) = setup_eor_w_b153();
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

    #[test]
    fn eor_decode_classifies_and_sizes() {
        // EOR is `Dn,<ea>` ONLY (opmode 4/5/6 = 0xB100/0xB140/0xB180) of the 0xB nibble — its own decode arms.
        // The dest is a data register (mode 000 = `Dn,Dn`) or alterable memory (2..6/abs). Mode field 001 =
        // CMPM (handled by the `cmp_class` arm FIRST), and opmode 0/1/2 = CMP / 3/7 = CMPA (also handled first).
        // The EORI immediate opcode (0x0Axx, high nibble 0) is a DIFFERENT instruction NOT decoded here (it must
        // never reach decode — `covered()` classifies it out by opcode). Decode of the genuine register form
        // must produce a recipe (no panic / no todo!()).
        for op in [
            0xB504u16, // EOR.b D2,D4   Dn,Dn (register dest)
            0xB744,    // EOR.w D3,D4   register dest
            0xB782,    // EOR.l D3,D2   register dest (.l, n4 idle)
            0xB312,    // EOR.b D1,(A2) memory dest
            0xB153,    // EOR.w D0,(A3)
            0xBB91,    // EOR.l D5,(A1) long memory dest
            0xB59C,    // EOR.l D2,(A4)+
            0xBBA2,    // EOR.l D5,-(A2)
        ] {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 2048,
                pc: 3072,
                sr: 0x2700,
                prefetch: [op, 0],
            };
            let _ = decode(&regs);
        }
    }
}
