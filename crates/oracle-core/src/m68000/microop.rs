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
use super::registers::{Registers, CCR_C, CCR_N, CCR_V, CCR_X, CCR_Z};

/// 16-bit `ADD` (`a + b`) → `(result, new CCR low byte)`. Sets X/N/Z/V/C per the 68000.
fn add_w(a: u16, b: u16) -> (u16, u16) {
    let sum = a as u32 + b as u32;
    let result = sum as u16;
    let am = a & 0x8000 != 0;
    let bm = b & 0x8000 != 0;
    let rm = result & 0x8000 != 0;
    let mut ccr = 0u16;
    if rm {
        ccr |= CCR_N;
    }
    if result == 0 {
        ccr |= CCR_Z;
    }
    if (am == bm) && (rm != am) {
        ccr |= CCR_V;
    }
    if sum > 0xFFFF {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

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
/// (immediates, indexed modes); a micro-op references registers symbolically so the recipe stays a
/// `Copy` template independent of live register contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Operand {
    /// A value computed by an earlier micro-op and stored in a scratch slot.
    Scratch(Slot),
    /// The low word of data register `Dn`, zero-extended.
    DataRegLow16(u8),
    /// Address register `An` (the active A7 when `n == 7`) — used as a bus address.
    AddrReg(u8),
    /// The immediate word currently in the prefetch queue (`prefetch[1]`, the word after the opcode).
    ImmWord,
}

/// Where a [`MicroOp::Alu`] result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Dest {
    /// A scratch slot (e.g. an intermediate later written to memory).
    Scratch(Slot),
    /// The low word of data register `Dn` (its high word is preserved — a `.w` write-back).
    DataRegLow16(u8),
}

/// An ALU operation a [`MicroOp::Alu`] performs (computing into scratch and updating the CCR). Grows with
/// arithmetic/logic coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum AluOp {
    /// 16-bit add: `dst = a + b`, setting X/N/Z/V/C.
    AddW,
}

/// One resumable step. Bus-access steps emit a [`Transaction`](super::bus68k::Transaction) and cost
/// 4 master cycles (one word access); compute/idle steps carry their own cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum MicroOp {
    /// Read a word at `addr` (data/program per `fc`) into scratch slot `dst`.
    ReadWord { addr: Operand, fc: Fc, dst: Slot },
    /// Write the word `value` at `addr` (data/program per `fc`).
    WriteWord {
        addr: Operand,
        fc: Fc,
        value: Operand,
    },
    /// Refill the prefetch queue (read at `pc+4`), advance the queue and `pc` by one word.
    Prefetch,
    /// Compute `op(a, b)` into `dst` and update the CCR. An internal (overlapped) step — no bus access,
    /// 0 standalone cycles.
    Alu {
        op: AluOp,
        a: Operand,
        b: Operand,
        dst: Dest,
    },
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
    /// Master cycles consumed by the micro-ops executed so far (the instruction total once done).
    cycles: u32,
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
            cycles: 0,
            scratch: [0; SCRATCH_SLOTS],
        }
    }

    /// True once every micro-op has executed.
    pub fn is_done(&self) -> bool {
        self.step >= self.len
    }

    /// Resolve an [`Operand`] to its concrete value at execution time.
    fn resolve(&self, op: Operand, regs: &Registers) -> u32 {
        match op {
            Operand::Scratch(s) => self.scratch[s as usize],
            Operand::DataRegLow16(n) => regs.d[n as usize] & 0xFFFF,
            Operand::AddrReg(n) => regs.addr_reg(n as usize),
            Operand::ImmWord => regs.prefetch[1] as u32,
        }
    }

    /// **Driver 1 — run-to-completion** (the default fast path): execute every remaining micro-op in
    /// order, returning the total master cycles. Drives the *same* [`Self::exec_one`] the quiesce path
    /// uses, so the two paths cannot diverge.
    pub fn run_to_completion(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
        let mut total = 0;
        while !self.is_done() {
            total += self.exec_one(regs, bus);
        }
        total
    }

    /// Execute exactly the next micro-op, advancing the cursor; returns the master cycles it cost.
    /// This is the single shared "cook" both drivers call — identical behavior by construction.
    pub fn exec_one(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
        let cycles = match self.ops[self.step as usize] {
            MicroOp::ReadWord { addr, fc, dst } => {
                let address = self.resolve(addr, regs);
                let value = bus.read16(address, regs.fc(matches!(fc, Fc::Program)));
                self.scratch[dst as usize] = value as u32;
                4
            }
            MicroOp::WriteWord { addr, fc, value } => {
                let address = self.resolve(addr, regs);
                let word = self.resolve(value, regs) as u16;
                bus.write16(address, regs.fc(matches!(fc, Fc::Program)), word);
                4
            }
            MicroOp::Alu { op, a, b, dst } => {
                let lhs = self.resolve(a, regs) as u16;
                let rhs = self.resolve(b, regs) as u16;
                let (result, ccr) = match op {
                    AluOp::AddW => add_w(lhs, rhs),
                };
                regs.sr = (regs.sr & 0xFF00) | ccr;
                match dst {
                    Dest::Scratch(s) => self.scratch[s as usize] = result as u32,
                    Dest::DataRegLow16(n) => {
                        regs.d[n as usize] = (regs.d[n as usize] & 0xFFFF_0000) | result as u32;
                    }
                }
                0
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
        self.cycles += cycles;
        cycles
    }
}

/// One 68000, driven by the micro-op framework. Between instructions `inflight` is `None`; while quiesced
/// mid-instruction it holds the resumable cursor. The whole CPU is bincode-serializable, so a debugger can
/// stop at a bus-access boundary, snapshot, restore, and resume.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Cpu68000 {
    pub regs: Registers,
    inflight: Option<MicroState>,
}

/// The outcome of one [`Cpu68000::step_micro_op`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// A micro-op executed; the instruction is still in flight (quiesced at a bus-access boundary).
    Continue,
    /// The instruction completed; carries the total master cycles it took.
    Done(u32),
}

impl Cpu68000 {
    /// Power on with the given register file and no instruction in flight.
    pub fn new(regs: Registers) -> Self {
        Self {
            regs,
            inflight: None,
        }
    }

    /// Begin executing a decoded recipe (decode wraps this in a later step). Panics if one is already
    /// in flight.
    pub fn begin(&mut self, state: MicroState) {
        assert!(self.inflight.is_none(), "instruction already in flight");
        self.inflight = Some(state);
    }

    /// **Driver 2 — step-one-micro-op** (the on-demand quiesce path): execute a single micro-op of the
    /// in-flight instruction, leaving the machine coherent at a bus-access boundary. Returns
    /// [`Step::Done`] with the total cycle count when the instruction completes.
    pub fn step_micro_op(&mut self, bus: &mut impl Bus68k) -> Step {
        let Cpu68000 { regs, inflight } = self;
        let state = inflight.as_mut().expect("no instruction in flight");
        state.exec_one(regs, bus);
        if state.is_done() {
            let total = state.cycles;
            *inflight = None;
            Step::Done(total)
        } else {
            Step::Continue
        }
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

    #[test]
    fn alu_add_w_computes_result_and_sets_flags() {
        let mut regs = regs();
        regs.d[5] = 0x020D_2596; // source Dn; low word 0x2596
        regs.sr = 0x2717; // CCR dirty + supervisor; this add should clear the CCR
        let mut bus = FlatBus::new();

        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::AddW,
            a: Operand::DataRegLow16(5),
            b: Operand::Scratch(0),
            dst: Dest::Scratch(1),
        }]);
        st.scratch[0] = 0x3FE0; // operand

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            cycles, 0,
            "ALU is internal/overlapped — 0 standalone cycles"
        );
        assert_eq!(st.scratch[1], 0x6576, "0x2596 + 0x3FE0");
        assert_eq!(regs.sr, 0x2700, "CCR cleared, system byte preserved");
        assert!(bus.log.is_empty(), "ALU touches no bus");
    }

    #[test]
    fn alu_writes_result_to_data_register_low_word_preserving_high() {
        let mut regs = regs();
        regs.d[6] = 0x47A4_1526; // high word must survive a .w write-back
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::AddW,
            a: Operand::DataRegLow16(6),
            b: Operand::Scratch(0),
            dst: Dest::DataRegLow16(6),
        }]);
        st.scratch[0] = 0xFC2B;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[6], 0x47A4_1151,
            "low word = 0x1526 + 0xFC2B; high word preserved"
        );
    }

    #[test]
    fn imm_word_operand_reads_prefetch_word_1() {
        let mut regs = regs();
        regs.prefetch = [0xDE7C, 0x8EF1];
        regs.d[7] = 0x1BC0_F680;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::AddW,
            a: Operand::DataRegLow16(7),
            b: Operand::ImmWord,
            dst: Dest::DataRegLow16(7),
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[7], 0x1BC0_8571,
            "0xF680 + prefetch[1] (0x8EF1) low word"
        );
    }

    #[test]
    fn run_to_completion_drives_all_micro_ops() {
        let mut regs = regs();
        let mut bus = FlatBus::new();
        bus.poke(0x1000, 0x12);
        bus.poke(0x1001, 0x34);

        let mut st = MicroState::from_ops(&[
            MicroOp::ReadWord {
                addr: Operand::Scratch(0),
                fc: Fc::Data,
                dst: 1,
            },
            MicroOp::Internal { cycles: 2 },
        ]);
        st.scratch[0] = 0x1000;

        let cycles = st.run_to_completion(&mut regs, &mut bus);

        assert_eq!(cycles, 6, "4 (word read) + 2 (internal)");
        assert!(st.is_done());
        assert_eq!(st.scratch[1], 0x1234);
        assert_eq!(bus.log.len(), 1, "exactly one bus access in the recipe");
    }

    /// A 3-micro-op recipe (read → internal → write), pre-seeded so it round-trips a value through scratch.
    fn sample_recipe() -> MicroState {
        let mut st = MicroState::from_ops(&[
            MicroOp::ReadWord {
                addr: Operand::Scratch(0),
                fc: Fc::Data,
                dst: 1,
            },
            MicroOp::Internal { cycles: 4 },
            MicroOp::WriteWord {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                value: Operand::Scratch(1),
            },
        ]);
        st.scratch[0] = 0x1000; // source address
        st.scratch[2] = 0x2000; // destination address
        st
    }

    fn sample_bus() -> FlatBus {
        let mut bus = FlatBus::new();
        bus.poke(0x1000, 0xBE);
        bus.poke(0x1001, 0xEF);
        bus
    }

    #[test]
    fn step_micro_op_quiesces_one_micro_op_at_a_time() {
        let mut bus = sample_bus();
        let mut cpu = Cpu68000::new(regs());
        cpu.begin(sample_recipe());

        // Stop right after the read: the machine is observable between micro-ops.
        assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        assert_eq!(bus.log.len(), 1, "quiesced right after the read access");

        // The internal cycle is a boundary too (still no second bus access).
        assert_eq!(cpu.step_micro_op(&mut bus), Step::Continue);
        assert_eq!(bus.log.len(), 1);

        // The write completes the instruction and reports the total: 4 + 4 + 4 = 12.
        assert_eq!(cpu.step_micro_op(&mut bus), Step::Done(12));
        assert_eq!(bus.log.len(), 2);
    }

    #[test]
    fn both_drivers_reach_identical_state_and_transaction_stream() {
        // Driver 1: run-to-completion.
        let mut regs_rtc = regs();
        let mut bus_rtc = sample_bus();
        let mut st = sample_recipe();
        let cycles_rtc = st.run_to_completion(&mut regs_rtc, &mut bus_rtc);

        // Driver 2: one micro-op at a time to completion.
        let mut bus_step = sample_bus();
        let mut cpu = Cpu68000::new(regs());
        cpu.begin(sample_recipe());
        let cycles_step = loop {
            if let Step::Done(c) = cpu.step_micro_op(&mut bus_step) {
                break c;
            }
        };

        assert_eq!(cycles_rtc, cycles_step, "both drivers agree on cycle count");
        assert_eq!(cpu.regs, regs_rtc, "both drivers reach identical registers");
        assert_eq!(
            bus_step.log, bus_rtc.log,
            "both drivers emit an identical transaction stream"
        );
    }
}
