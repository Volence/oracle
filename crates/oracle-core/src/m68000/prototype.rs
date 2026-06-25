//! Cycle-granularity prototype: `ADD.w Dn,(An)` implemented **two ways** over the same bus, to settle
//! the one open Phase-0 decision empirically (see `docs/decisions/`).
//!
//! - [`step_instruction`] — the **instruction-stepped** fast path: the whole opcode executes atomically,
//!   emitting its three bus transactions (operand read, prefetch refill, result write) in order.
//! - [`AddWFsm`] — the **cycle-stepped** FSM: one `tick` advances one master cycle; the bus access for a
//!   phase happens on the phase's final cycle, so *every* cycle boundary is a coherent break/snapshot
//!   point (no half-completed transaction). The FSM is bincode-serializable, so the machine can be
//!   snapshot/restored mid-instruction.
//!
//! Both are validated against real SingleStepTests data (`tests/singlestep_m68000.rs`) and against each
//! other (same final state + same transaction stream). The 68000 has a 24-bit address bus, so every
//! access is masked to [`ADDR_MASK`].

use super::registers::{Registers, CCR_C, CCR_N, CCR_V, CCR_X, CCR_Z};
// Re-exported so existing consumers (`tests/singlestep_m68000.rs`, the perf example) keep importing the
// durable bus types from here through the framework transition.
pub use super::bus68k::{Bus68k, FlatBus, Transaction, TxKind};

/// Decode `ADD.w Dn,(An)` (`1101 ddd 1 01 010 rrr`) → `(dn, an)` register indices.
pub fn decode_add_w_dn_an(opcode: u16) -> (u8, u8) {
    (((opcode >> 9) & 7) as u8, (opcode & 7) as u8)
}

/// 16-bit `ADD` (`dest = dest + source`) → `(result, new CCR low byte)`.
pub fn add_w_flags(source: u16, dest: u16) -> (u16, u16) {
    let sum = source as u32 + dest as u32;
    let result = sum as u16;
    let sm = source & 0x8000 != 0;
    let dm = dest & 0x8000 != 0;
    let rm = result & 0x8000 != 0;
    let carry = sum > 0xFFFF;
    let overflow = (sm == dm) && (rm != sm);
    let mut ccr = 0u16;
    if rm {
        ccr |= CCR_N;
    }
    if result == 0 {
        ccr |= CCR_Z;
    }
    if overflow {
        ccr |= CCR_V;
    }
    if carry {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

/// Instruction-stepped `ADD.w Dn,(An)`: execute the whole opcode atomically. Returns the cycle count (12).
pub fn step_instruction(regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
    let (dn, an) = decode_add_w_dn_an(regs.prefetch[0]);
    let addr = regs.addr_reg(an as usize);
    // 1. read the memory operand
    let operand = bus.read16(addr, regs.fc(false));
    // 2. refill the prefetch queue (read the word at pc + 4), then advance the queue + pc
    let refill = bus.read16(regs.pc.wrapping_add(4), regs.fc(true));
    regs.prefetch[0] = regs.prefetch[1];
    regs.prefetch[1] = refill;
    regs.pc = regs.pc.wrapping_add(2);
    // 3. ALU, flags, and the result write-back
    let source = (regs.d[dn as usize] & 0xFFFF) as u16;
    let (result, ccr) = add_w_flags(source, operand);
    regs.sr = (regs.sr & 0xFF00) | ccr;
    bus.write16(addr, regs.fc(false), result);
    12
}

/// The phase of the cycle-stepped FSM. Each phase spans 4 master cycles; its bus access fires on the
/// last cycle so every boundary is a coherent snapshot point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
enum Phase {
    ReadOperand,
    Prefetch,
    WriteResult,
    Done,
}

/// Cycle-stepped `ADD.w Dn,(An)`. Serializable, so the machine can be snapshot/restored mid-instruction.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct AddWFsm {
    phase: Phase,
    cyc: u8,
    dn: u8,
    an: u8,
    addr: u32,
    operand: u16,
}

impl AddWFsm {
    /// Begin executing the opcode currently in `regs.prefetch[0]`.
    pub fn new(regs: &Registers) -> Self {
        let (dn, an) = decode_add_w_dn_an(regs.prefetch[0]);
        Self {
            phase: Phase::ReadOperand,
            cyc: 0,
            dn,
            an,
            addr: regs.addr_reg(an as usize),
            operand: 0,
        }
    }

    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Advance exactly one master cycle. Each phase spans 4 cycles; its single bus access fires on the
    /// 4th, so cycles 0–2 are "access in progress" and every boundary is a coherent break/snapshot point.
    pub fn tick(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) {
        match self.phase {
            Phase::ReadOperand => {
                if self.cyc == 3 {
                    self.operand = bus.read16(self.addr, regs.fc(false));
                    self.cyc = 0;
                    self.phase = Phase::Prefetch;
                } else {
                    self.cyc += 1;
                }
            }
            Phase::Prefetch => {
                if self.cyc == 3 {
                    let refill = bus.read16(regs.pc.wrapping_add(4), regs.fc(true));
                    regs.prefetch[0] = regs.prefetch[1];
                    regs.prefetch[1] = refill;
                    regs.pc = regs.pc.wrapping_add(2);
                    self.cyc = 0;
                    self.phase = Phase::WriteResult;
                } else {
                    self.cyc += 1;
                }
            }
            Phase::WriteResult => {
                if self.cyc == 3 {
                    let source = (regs.d[self.dn as usize] & 0xFFFF) as u16;
                    let (result, ccr) = add_w_flags(source, self.operand);
                    regs.sr = (regs.sr & 0xFF00) | ccr;
                    bus.write16(self.addr, regs.fc(false), result);
                    self.cyc = 0;
                    self.phase = Phase::Done;
                } else {
                    self.cyc += 1;
                }
            }
            Phase::Done => {}
        }
    }

    /// Tick until the instruction completes; returns the number of cycles taken.
    pub fn run_to_completion(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
        let mut cycles = 0;
        while !self.is_done() {
            self.tick(regs, bus);
            cycles += 1;
        }
        cycles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clean SingleStepTests reference case `db50 [ADD.w D5,(A0)]` (even address, 12 cycles).
    fn setup_db50() -> (Registers, FlatBus) {
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
        (regs, bus)
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

    fn assert_db50_final(regs: &Registers, bus: &FlatBus) {
        assert_eq!(regs.pc, 0x0C02, "pc advanced by one word");
        assert_eq!(regs.sr, 0x2700, "CCR cleared, system byte preserved");
        assert_eq!(regs.d[5], 0x020D_2596, "Dn unchanged (dest is memory)");
        assert_eq!(regs.a[0], 0xBB4F_4F46, "An unchanged");
        assert_eq!(
            regs.prefetch,
            [0x6A3C, 0x414E],
            "prefetch queue advanced + refilled"
        );
        assert_eq!(bus.peek(0x4F_4F46), 0x65);
        assert_eq!(bus.peek(0x4F_4F47), 0x76);
        assert_eq!(bus.log, expected_log());
    }

    #[test]
    fn add_w_flags_cases() {
        assert_eq!(add_w_flags(0x2596, 0x3FE0), (0x6576, 0x00));
        assert_eq!(add_w_flags(0xFFFF, 0x0001), (0x0000, CCR_Z | CCR_C | CCR_X));
        assert_eq!(add_w_flags(0x7FFF, 0x0001), (0x8000, CCR_N | CCR_V));
        assert_eq!(
            add_w_flags(0x8000, 0x8000),
            (0x0000, CCR_Z | CCR_V | CCR_C | CCR_X)
        );
    }

    #[test]
    fn instruction_stepped_matches_db50() {
        let (mut regs, mut bus) = setup_db50();
        let cycles = step_instruction(&mut regs, &mut bus);
        assert_eq!(cycles, 12);
        assert_db50_final(&regs, &bus);
    }

    #[test]
    fn fsm_runs_in_twelve_cycles_and_matches_db50() {
        let (mut regs, mut bus) = setup_db50();
        let mut fsm = AddWFsm::new(&regs);
        let cycles = fsm.run_to_completion(&mut regs, &mut bus);
        assert_eq!(cycles, 12);
        assert_db50_final(&regs, &bus);
    }

    #[test]
    fn fsm_equals_instruction_stepped() {
        let (mut r1, mut b1) = setup_db50();
        step_instruction(&mut r1, &mut b1);
        let (mut r2, mut b2) = setup_db50();
        AddWFsm::new(&r2).run_to_completion(&mut r2, &mut b2);
        assert_eq!(r1, r2, "final register state differs");
        assert_eq!(b1.log, b2.log, "transaction stream differs");
    }

    #[test]
    fn fsm_is_quiescable_and_serializable_at_every_cycle() {
        // Reference final state.
        let (mut rref, mut bref) = setup_db50();
        AddWFsm::new(&rref).run_to_completion(&mut rref, &mut bref);

        let cfg = bincode::config::standard();
        for pause_at in 0..=12 {
            let (mut regs, mut bus) = setup_db50();
            let mut fsm = AddWFsm::new(&regs);
            for _ in 0..pause_at {
                if !fsm.is_done() {
                    fsm.tick(&mut regs, &mut bus);
                }
            }
            // Snapshot + restore the CPU state mid-instruction, then resume on the same bus.
            let rb = bincode::encode_to_vec(&regs, cfg).unwrap();
            let fb = bincode::encode_to_vec(&fsm, cfg).unwrap();
            let (mut regs2, _): (Registers, usize) = bincode::decode_from_slice(&rb, cfg).unwrap();
            let (mut fsm2, _): (AddWFsm, usize) = bincode::decode_from_slice(&fb, cfg).unwrap();
            while !fsm2.is_done() {
                fsm2.tick(&mut regs2, &mut bus);
            }
            assert_eq!(regs2, rref, "resume from cycle {pause_at} diverged");
            assert_eq!(
                bus.log, bref.log,
                "transaction stream from cycle {pause_at} diverged"
            );
        }
    }
}
