//! Opcode → micro-op recipe (decode), and the instruction-level [`Cpu68000`] entry points.
//!
//! `decode` maps the opcode in the prefetch queue to its [`MicroState`] recipe; the two `Cpu68000`
//! methods tie decode to the framework's two drivers (run-to-completion fast path / step-one-micro-op
//! quiesce). Decodes the `ADD.w` / `SUB.w` families so far (the shared `arith_w_*` builders are
//! parameterized by `AluOp`); the full 65536-entry dispatch (one builder per instruction family) lands
//! with full coverage.

use super::bus68k::Bus68k;
use super::ea::{ea_dst, ea_src, RecipeBuf};
use super::microop::{AluOp, Cpu68000, Dest, MicroOp, MicroState, Operand, Size};
use super::registers::Registers;

/// Whether an EA `mode`/`reg` pair is an alterable-memory destination the builder currently covers:
/// `(An)` (010), `(An)+` (011), `-(An)` (100), `d16(An)` (101), `abs.w` (111/000), `abs.l` (111/001). The
/// remaining alterable-memory mode (`d8(An,Xn)`) lands in a later commit. (PC-relative and `#imm` are not
/// alterable, so never destinations.)
#[inline]
fn is_dst_mem_mode(mode: u16, reg: u16) -> bool {
    matches!(mode, 2..=5) || (mode == 7 && (reg == 0 || reg == 1))
}

/// Decode the opcode currently in `regs.prefetch[0]` into its micro-op recipe.
#[inline]
pub fn decode(regs: &Registers) -> MicroState {
    let opcode = regs.prefetch[0];
    // ADD.w and SUB.w share recipe shapes — they differ only in the `AluOp` (operand order is arranged so
    // the destination is the minuend, which matters for the non-commutative SUB).
    // `<op>.w Dn,<ea>` (memory destination, `1xx1 ddd 1 01 mmm rrr`). The destination-EA builder handles
    // the covered alterable-memory modes: `(An)` (010), `(An)+` (011), `-(An)` (100).
    if opcode & 0xF1C0 == 0xD140 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_w_dn_ea(opcode, AluOp::Add); // ADD.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0xD040 {
        return arith_w_ea_dn(opcode, AluOp::Add); // ADD.w <ea>,Dn
    }
    if opcode & 0xF1C0 == 0x9140 && is_dst_mem_mode((opcode >> 3) & 7, opcode & 7) {
        return arith_w_dn_ea(opcode, AluOp::Sub); // SUB.w Dn,<ea>
    }
    if opcode & 0xF1C0 == 0x9040 {
        return arith_w_ea_dn(opcode, AluOp::Sub); // SUB.w <ea>,Dn
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

/// `<op>.w Dn,<ea>` (`1xx1 ddd 1 01 mmm rrr`, memory destination): read the memory operand at the dest
/// EA, refill prefetch, combine it with `Dn`, write the result back to the same address. The **memory
/// operand is the minuend** (`a`) so `SUB` computes `<ea> - Dn`; `ADD` is commutative so the same order is
/// correct. The ALU is an overlapped internal step; the `(An)+`/`-(An)` register adjust is a 0-cycle
/// `AdjustAddr`. Expressed through the shared destination-EA builder ([`ea_dst`]) — the read/refill/ALU/
/// write skeleton (and any auto-(in/de)crement) is the mode's, only the ALU operands are the opcode's.
fn arith_w_dn_ea(opcode: u16, op: AluOp) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    ea_dst(&mut buf, mode, reg, |a| MicroOp::Alu {
        op,
        size: Size::Word,
        a,
        b: Operand::DataRegLow16(dn),
        dst: Dest::Scratch(1),
    });
    buf.finish()
}

/// `<op>.w <ea>,Dn` (`1xx1 ddd 0 01 mmm rrr`, register destination): `Dn = Dn <op> <ea>` — **Dn is the
/// minuend** (`a`). The source-EA builder ([`ea_src`]) covers the register-direct (`Dn`/`An`), indirect
/// (`(An)`/`(An)+`/`-(An)`), displaced (`d16(An)`/`d16(PC)`), absolute (`abs.w`/`abs.l`) and immediate
/// (`#imm`) modes. The indexed `d8(An,Xn)`/`d8(PC,Xn)` modes are out of slice for this push (decode panics,
/// and the harness xfails them).
fn arith_w_ea_dn(opcode: u16, op: AluOp) -> MicroState {
    let dn = ((opcode >> 9) & 7) as u8;
    let mode = (opcode >> 3) & 7;
    let reg = (opcode & 7) as u8;
    let mut buf = RecipeBuf::new();
    // The source-EA builder fetches the operand and places the prefetch(es); the ALU it builds combines
    // `Dn` (the minuend) with that source operand and writes back to `Dn`.
    ea_src(&mut buf, mode, reg, |b| MicroOp::Alu {
        op,
        size: Size::Word,
        a: Operand::DataRegLow16(dn),
        b,
        dst: Dest::DataRegLow16(dn),
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
                value: 0x3FE0,
            },
            Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x0C04,
                value: 0x414E,
            },
            Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x4F_4F46,
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
}
