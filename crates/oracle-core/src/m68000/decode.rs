//! Opcode → micro-op recipe (decode), and the instruction-level [`Cpu68000`] entry points.
//!
//! `decode` maps the opcode in the prefetch queue to its [`MicroState`] recipe; the two `Cpu68000`
//! methods tie decode to the framework's two drivers (run-to-completion fast path / step-one-micro-op
//! quiesce). Decodes the `ADD`/`SUB` families in word, byte and long sizes so far (the shared `arith_ea_dn`
//! / `arith_dn_ea` builders are parameterized by `AluOp` and `Size`); the full 65536-entry dispatch (one
//! builder per instruction family) lands with full coverage.

use super::bus68k::Bus68k;
use super::ea::{ea_dst, ea_src, RecipeBuf};
use super::microop::{AluOp, Cpu68000, Dest, MicroOp, MicroState, Operand, Size};
use super::registers::Registers;

/// Whether an EA `mode`/`reg` pair is an alterable-memory destination the builder currently covers:
/// `(An)` (010), `(An)+` (011), `-(An)` (100), `d16(An)` (101), `d8(An,Xn)` (110), `abs.w` (111/000),
/// `abs.l` (111/001) — all seven alterable-memory modes. (PC-relative and `#imm` are not alterable, so
/// never destinations.)
#[inline]
fn is_dst_mem_mode(mode: u16, reg: u16) -> bool {
    matches!(mode, 2..=6) || (mode == 7 && (reg == 0 || reg == 1))
}

/// Decode the opcode currently in `regs.prefetch[0]` into its micro-op recipe.
#[inline]
pub fn decode(regs: &Registers) -> MicroState {
    let opcode = regs.prefetch[0];
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
}
