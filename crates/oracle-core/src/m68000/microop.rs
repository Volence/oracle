//! The micro-op opcode framework — one resumable definition per opcode, two drivers over it.
//!
//! Each 68000 opcode is written **once** as an ordered sequence of [`MicroOp`]s (the ratified
//! single-definition hybrid, `docs/decisions/2026-06-24-cycle-granularity.md`). One shared interpreter
//! ([`MicroState::exec_one`]) performs a single micro-op; the two drivers (run-to-completion fast path /
//! step-one-micro-op quiesce) are just two loops over that same data, so they cannot diverge. The
//! in-flight cursor ([`MicroState`]) is small fixed state deriving bincode `Encode`/`Decode`, so the
//! machine can snapshot/restore *mid-instruction*.
//!
//! This push runs over the word+FC [`Bus68k`]; unifying it with the generic `crate::bus::Bus` is a
//! follow-up (Step 2).

use super::bus68k::Bus68k;
use super::registers::Registers;

/// Maximum micro-ops in one opcode's recipe. Most opcodes need ≤ a handful; unbounded families
/// (MOVEM-class) get a generator variant later. Grown as coverage requires.
const MAX_OPS: usize = 8;

/// Number of scratch slots carrying values between micro-ops within one instruction.
const SCRATCH_SLOTS: usize = 4;

/// Index into the scratch register file.
pub type Slot = u8;

/// Which 68000 function-code class a bus access uses: data or program space (the supervisor/user half is
/// derived from the live SR by [`Registers::fc`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Fc {
    Data,
    Program,
}

/// A value resolved at execution time — an address or an operand. Grows with addressing-mode coverage
/// (data/address registers, immediates); for now a micro-op only references a scratch slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Operand {
    /// A value computed by an earlier micro-op and stored in a scratch slot.
    Scratch(Slot),
}

/// One resumable step. Bus-access steps emit a [`Transaction`](super::bus68k::Transaction) and cost
/// 4 master cycles (one word access); compute/idle steps carry their own cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum MicroOp {
    /// Read a word at `addr` (data/program per `fc`) into scratch slot `dst`.
    ReadWord { addr: Operand, fc: Fc, dst: Slot },
    /// Write the word `value` at `addr` (data/program per `fc`).
    WriteWord { addr: Operand, fc: Fc, value: Operand },
    /// Refill the prefetch queue (read at `pc+4`), advance the queue and `pc` by one word.
    Prefetch,
    /// Consume `cycles` master cycles with no bus access (compute / idle `n` cycles).
    Internal { cycles: u8 },
}

/// The in-flight micro-op cursor for one instruction: the recipe, how far through it we are, and the
/// scratch values flowing between steps. Small, fixed, bincode-serializable — snapshot/restore at any
/// bus-access boundary.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct MicroState {
    ops: [MicroOp; MAX_OPS],
    len: u8,
    step: u8,
    scratch: [u32; SCRATCH_SLOTS],
}

impl MicroState {
    /// Build a cursor from a recipe (its ordered micro-ops). Slots beyond `len` are inert filler.
    pub fn from_ops(ops: &[MicroOp]) -> Self {
        assert!(ops.len() <= MAX_OPS, "recipe exceeds MAX_OPS");
        let mut arr = [MicroOp::Internal { cycles: 0 }; MAX_OPS];
        arr[..ops.len()].copy_from_slice(ops);
        Self {
            ops: arr,
            len: ops.len() as u8,
            step: 0,
            scratch: [0; SCRATCH_SLOTS],
        }
    }

    /// True once every micro-op has executed.
    pub fn is_done(&self) -> bool {
        self.step >= self.len
    }

    /// Resolve an [`Operand`] to its concrete value at execution time.
    fn resolve(&self, op: Operand) -> u32 {
        match op {
            Operand::Scratch(s) => self.scratch[s as usize],
        }
    }

    /// Execute exactly the next micro-op, advancing the cursor; returns the master cycles it cost.
    /// This is the single shared "cook" both drivers call — identical behavior by construction.
    pub fn exec_one(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
        let cycles = match self.ops[self.step as usize] {
            MicroOp::ReadWord { addr, fc, dst } => {
                let address = self.resolve(addr);
                let value = bus.read16(address, regs.fc(matches!(fc, Fc::Program)));
                self.scratch[dst as usize] = value as u32;
                4
            }
            MicroOp::WriteWord { addr, fc, value } => {
                let address = self.resolve(addr);
                let word = self.resolve(value) as u16;
                bus.write16(address, regs.fc(matches!(fc, Fc::Program)), word);
                4
            }
            MicroOp::Prefetch => {
                let refill = bus.read16(regs.pc.wrapping_add(4), regs.fc(true));
                regs.prefetch[0] = regs.prefetch[1];
                regs.prefetch[1] = refill;
                regs.pc = regs.pc.wrapping_add(2);
                4
            }
            MicroOp::Internal { cycles } => cycles as u32,
        };
        self.step += 1;
        cycles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m68000::bus68k::{FlatBus, Transaction, TxKind};
    use crate::m68000::registers::{Registers, SR_SUPERVISOR};

    /// Supervisor-mode registers (so a data access carries FC 5), otherwise zeroed.
    fn regs() -> Registers {
        Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0,
            ssp: 0,
            pc: 0x0C00,
            sr: SR_SUPERVISOR,
            prefetch: [0; 2],
        }
    }

    #[test]
    fn read_word_reads_to_scratch_and_emits_transaction() {
        let mut regs = regs();
        let mut bus = FlatBus::new();
        bus.poke(0x1000, 0xAB);
        bus.poke(0x1001, 0xCD);

        let mut st = MicroState::from_ops(&[MicroOp::ReadWord {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            dst: 1,
        }]);
        st.scratch[0] = 0x1000;

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 4, "a word bus access is 4 master cycles");
        assert_eq!(st.scratch[1], 0xABCD, "operand landed in scratch slot 1");
        assert_eq!(st.step, 1, "cursor advanced one micro-op");
        assert!(st.is_done());
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x1000,
                value: 0xABCD,
            }]
        );
    }

    #[test]
    fn write_word_writes_value_at_address_and_emits_transaction() {
        let mut regs = regs();
        let mut bus = FlatBus::new();

        let mut st = MicroState::from_ops(&[MicroOp::WriteWord {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            value: Operand::Scratch(1),
        }]);
        st.scratch[0] = 0x2000;
        st.scratch[1] = 0x6576;

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(st.step, 1);
        assert!(st.is_done());
        assert_eq!(bus.peek(0x2000), 0x65, "high byte (big-endian)");
        assert_eq!(bus.peek(0x2001), 0x76, "low byte");
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x2000,
                value: 0x6576,
            }]
        );
    }

    #[test]
    fn internal_consumes_cycles_without_bus_access() {
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Internal { cycles: 6 }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 6, "Internal costs exactly its declared cycles");
        assert_eq!(st.step, 1);
        assert!(st.is_done());
        assert!(bus.log.is_empty(), "Internal touches no bus");
    }

    #[test]
    fn prefetch_refills_queue_and_advances_pc() {
        let mut regs = regs();
        regs.pc = 0x0C00;
        regs.prefetch = [0xDB50, 0x6A3C];
        let mut bus = FlatBus::new();
        // The word at pc+4 (= 0x0C04) refills the queue's tail.
        bus.poke(0x0C04, 0x41);
        bus.poke(0x0C05, 0x4E);

        let mut st = MicroState::from_ops(&[MicroOp::Prefetch]);
        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 4, "a prefetch refill is one word access");
        assert_eq!(regs.pc, 0x0C02, "pc advanced by one word");
        assert_eq!(
            regs.prefetch,
            [0x6A3C, 0x414E],
            "queue shifted and refilled from pc+4"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 6,
                addr: 0x0C04,
                value: 0x414E,
            }],
            "prefetch is a supervisor-program (FC 6) word read at pc+4"
        );
    }
}
