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

use super::bus68k::{Bus68k, ADDR_MASK};
use super::registers::{Registers, CCR_C, CCR_N, CCR_V, CCR_X, CCR_Z};

/// Sign-extend a 16-bit value to 32 bits (the displacement / `abs.w` address extension).
#[inline]
fn sign_extend16(v: u16) -> u32 {
    v as i16 as i32 as u32
}

/// Sign-extend an 8-bit value to 32 bits (the `d8(An,Xn)` / `d8(PC,Xn)` brief-extension displacement).
#[inline]
fn sign_extend8(v: u8) -> u32 {
    v as i8 as i32 as u32
}

/// 16-bit `ADD` (`a + b`) → `(result, new CCR low byte)`. Sets X/N/Z/V/C per the 68000.
#[inline]
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

/// 16-bit `SUB` (`a - b`, a the minuend) → `(result, new CCR low byte)`. Sets X/N/Z/V/C per the 68000.
#[inline]
fn sub_w(a: u16, b: u16) -> (u16, u16) {
    let result = a.wrapping_sub(b);
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
    if (am != bm) && (rm != am) {
        ccr |= CCR_V;
    }
    if (a as u32) < (b as u32) {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

/// 8-bit `ADD` (`a + b`) → `(result, new CCR low byte)`. Same shape as [`add_w`] at the byte boundary:
/// sign bit `0x80`, carry/extend when the sum exceeds `0xFF` (`0x100`). Sets X/N/Z/V/C.
#[inline]
fn add_b(a: u8, b: u8) -> (u8, u16) {
    let sum = a as u16 + b as u16;
    let result = sum as u8;
    let am = a & 0x80 != 0;
    let bm = b & 0x80 != 0;
    let rm = result & 0x80 != 0;
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
    if sum > 0xFF {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

/// 8-bit `SUB` (`a - b`, a the minuend) → `(result, new CCR low byte)`. Byte boundary (`0x80` sign,
/// borrow when `a < b`). Sets X/N/Z/V/C.
#[inline]
fn sub_b(a: u8, b: u8) -> (u8, u16) {
    let result = a.wrapping_sub(b);
    let am = a & 0x80 != 0;
    let bm = b & 0x80 != 0;
    let rm = result & 0x80 != 0;
    let mut ccr = 0u16;
    if rm {
        ccr |= CCR_N;
    }
    if result == 0 {
        ccr |= CCR_Z;
    }
    if (am != bm) && (rm != am) {
        ccr |= CCR_V;
    }
    if a < b {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

/// 32-bit `ADD` (`a + b`) → `(result, new CCR low byte)`. Same shape as [`add_w`] at the long boundary:
/// sign bit `0x8000_0000`, carry/extend when the 33-bit sum exceeds `0xFFFF_FFFF`. Sets X/N/Z/V/C.
#[inline]
fn add_l(a: u32, b: u32) -> (u32, u16) {
    let sum = a as u64 + b as u64;
    let result = sum as u32;
    let am = a & 0x8000_0000 != 0;
    let bm = b & 0x8000_0000 != 0;
    let rm = result & 0x8000_0000 != 0;
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
    if sum > 0xFFFF_FFFF {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

/// 32-bit `SUB` (`a - b`, a the minuend) → `(result, new CCR low byte)`. Long boundary (`0x8000_0000`
/// sign, borrow when `a < b`). Sets X/N/Z/V/C.
#[inline]
fn sub_l(a: u32, b: u32) -> (u32, u16) {
    let result = a.wrapping_sub(b);
    let am = a & 0x8000_0000 != 0;
    let bm = b & 0x8000_0000 != 0;
    let rm = result & 0x8000_0000 != 0;
    let mut ccr = 0u16;
    if rm {
        ccr |= CCR_N;
    }
    if result == 0 {
        ccr |= CCR_Z;
    }
    if (am != bm) && (rm != am) {
        ccr |= CCR_V;
    }
    if a < b {
        ccr |= CCR_C | CCR_X;
    }
    (result, ccr)
}

/// Maximum micro-ops in one opcode's recipe. Most opcodes need ≤ a handful; unbounded families
/// (MOVEM-class) get a generator variant later. Sized to the worst in-scope EA recipe (a byte
/// `Dn,(abs.l)` RMW = 8 ops) with headroom. Public so the EA builder ([`super::ea::RecipeBuf`]) can
/// size its fixed staging array to the same bound.
pub const MAX_OPS: usize = 12;

/// Number of scratch slots carrying values between micro-ops within one instruction. Sized to the
/// worst in-scope recipe (≤ 4) with headroom.
const SCRATCH_SLOTS: usize = 6;

/// Index into the scratch register file.
pub type Slot = u8;

/// Which 68000 function-code class a bus access uses: data or program space (the supervisor/user half is
/// derived from the live SR by [`Registers::fc`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Fc {
    Data,
    Program,
}

/// Operand/access size — byte, word, or long. Tags `Read`/`Write` (how wide a bus access is) and `Alu`
/// (which flag boundaries apply). A `.l` operand is **two** word bus accesses (hi at `addr`, lo at
/// `addr+2`) assembled via [`MicroOp::Combine32`] — `Read`/`Write` themselves stay word-granular, so
/// `Size::Long` tags only the [`MicroOp::Alu`] flag boundary (the 32-bit `add_l`/`sub_l`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Size {
    Byte,
    Word,
    Long,
}

/// A value resolved at execution time — an address or an operand. Grows with addressing-mode coverage
/// (immediates, indexed modes); a micro-op references registers symbolically so the recipe stays a
/// `Copy` template independent of live register contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Operand {
    /// A value computed by an earlier micro-op and stored in a scratch slot.
    Scratch(Slot),
    /// The HIGH word of a scratch slot: `scratch[s] >> 16`. The hi word of a long value parked in scratch —
    /// fed to the first `Write` of a long memory write (`Write` truncates to the low 16). Distinct from
    /// [`Operand::Scratch`] (which a long write uses for the lo word).
    ScratchHi16(Slot),
    /// The full 32 bits of data register `Dn` — the source value for a long `ADD.l`/`SUB.l`.
    DataRegFull(u8),
    /// The low word of data register `Dn`, zero-extended.
    DataRegLow16(u8),
    /// The low byte of data register `Dn`, zero-extended — the source value for a byte `ADD.b`/`SUB.b`.
    DataRegLow8(u8),
    /// The low word of address register `An` (the active A7 when `n == 7`), zero-extended — the source
    /// value for a legal `<op>.w An,Dn` (the full `An` register, of which only the low word is used).
    AddrRegLow16(u8),
    /// Address register `An` (the active A7 when `n == 7`) — used as a bus address.
    AddrReg(u8),
    /// The immediate word currently in the prefetch queue (`prefetch[1]`, the word after the opcode).
    ImmWord,
    /// A constant zero — an inert leg of an [`MicroOp::EaCalc`] (e.g. the index/base a mode doesn't use).
    Zero,
    /// A constant `2` — the word stride between the two halves of a long memory access. The low word of a
    /// `.l` operand lives at `addr + 2`; an [`MicroOp::EaCalc`] adds this to the materialized base to form
    /// the low half's address.
    WordStep,
    /// The displacement word currently in the prefetch queue, sign-extended: `sign_extend16(prefetch[1])`.
    /// The `d16(An)`/`abs.w` extension word; captured by [`MicroOp::EaCalc`] **before** the refill that
    /// shifts it out of the queue.
    DispWord,
    /// The address of the extension word: `regs.pc.wrapping_add(2)`. The PC-relative base for `d16(PC)` —
    /// the displacement is relative to where the extension word lives (one word past the opcode), so the
    /// [`MicroOp::EaCalc`] must run **before** any `Prefetch` advances `pc`.
    PcOfExt,
    /// The high half of an `abs.l` address: `(prefetch[1] as u32) << 16`. Captured from the queue **before**
    /// the interleaved `Prefetch` that shifts the low word in.
    ExtWordHi,
    /// The low half of an `abs.l` address: `prefetch[1] as u32` (zero-extended, unmodified). Read from the
    /// queue **after** the interleaved `Prefetch` — **never** from that prefetch's bus-return value (which
    /// would double-count the queue).
    ExtWordRaw,
    /// The sized, sign-extended **index** of a `d8(An,Xn)` / `d8(PC,Xn)` brief extension word
    /// (`prefetch[1]`): bit15 selects the index register file (`1` = `regs.addr_reg`, A7-aware; `0` =
    /// `regs.d`), bits14-12 the register number, bit11 the size (`0` = W → sign-extend the low 16 to 32;
    /// `1` = L → the full 32 bits). This is the one isolated runtime branch in the whole EA machinery —
    /// kept in this single pure resolver, **not** a per-mode switch in `exec_one`.
    BriefIndex,
    /// The sign-extended 8-bit displacement of a `d8(An,Xn)` / `d8(PC,Xn)` brief extension word
    /// (`prefetch[1]`): `sign_extend8(prefetch[1] & 0xFF)`. The high byte (D/A, index reg, W/L) is the
    /// [`Operand::BriefIndex`] half, not part of the displacement.
    BriefDisp8,
}

/// Where a [`MicroOp::Alu`] result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Dest {
    /// A scratch slot (e.g. an intermediate later written to memory).
    Scratch(Slot),
    /// The full 32 bits of data register `Dn` — a `.l` write-back (no preserved bits).
    DataReg(u8),
    /// The low word of data register `Dn` (its high word is preserved — a `.w` write-back).
    DataRegLow16(u8),
    /// The low byte of data register `Dn` (its upper 24 bits are preserved — a `.b` write-back).
    DataRegLow8(u8),
}

/// An ALU operation a [`MicroOp::Alu`] performs (computing into scratch and updating the CCR). The
/// operand width is carried separately by [`MicroOp::Alu`]'s `size`. Grows with arithmetic/logic coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum AluOp {
    /// Add: `dst = a + b`, setting X/N/Z/V/C (at the operand-size boundary).
    Add,
    /// Subtract: `dst = a - b` (a is the minuend), setting X/N/Z/V/C (at the operand-size boundary).
    Sub,
}

/// One resumable step. Bus-access steps emit a [`Transaction`](super::bus68k::Transaction) and cost
/// 4 master cycles (one word access); compute/idle steps carry their own cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum MicroOp {
    /// Read a `size` operand at `addr` (data/program per `fc`) into scratch slot `dst` (zero-extended).
    Read {
        addr: Operand,
        fc: Fc,
        size: Size,
        dst: Slot,
    },
    /// Write the low `size` of `value` at `addr` (data/program per `fc`).
    Write {
        addr: Operand,
        fc: Fc,
        size: Size,
        value: Operand,
    },
    /// Refill the prefetch queue (read at `pc+4`), advance the queue and `pc` by one word.
    Prefetch,
    /// Compute `op(a, b)` at `size` into `dst` and update the CCR. An internal (overlapped) step — no bus
    /// access, 0 standalone cycles.
    Alu {
        op: AluOp,
        size: Size,
        a: Operand,
        b: Operand,
        dst: Dest,
    },
    /// Consume `cycles` master cycles with no bus access (compute / idle `n` cycles).
    Internal { cycles: u8 },
    /// Apply an address-register side effect: `An += delta` (the `(An)+`/`-(An)` auto-(in/de)crement),
    /// written through [`Registers::addr_reg_set`] so `An == A7` hits the active stack pointer. A 0-cycle,
    /// non-bus one-shot — separate from the operand access so the bump is snapshot-visible and can straddle
    /// a prefetch.
    AdjustAddr { reg: u8, delta: i8 },
    /// Compute an effective address `(resolve(base) + resolve(index) + resolve(disp)) & ADDR_MASK` into
    /// scratch slot `dst`. A **fixed** 3-way `wrapping_add` — there is deliberately **no per-mode match
    /// inside `exec_one`**; the decode-time builder picks which operands feed each leg (`Zero` for an
    /// inert one), so every EA mode shares this single hot-path arm. A 0-cycle, non-bus, snapshot-visible
    /// internal step: the materialized EA is a serializable mid-instruction value.
    EaCalc {
        base: Operand,
        index: Operand,
        disp: Operand,
        dst: Slot,
    },
    /// Assemble a 32-bit long value `(scratch[hi] << 16) | resolve(lo)` into scratch slot `dst`. The two
    /// halves of a long operand: `hi` is the high word (already in a scratch slot from the first `Read`),
    /// `lo` resolves the low word (the second `Read`'s scratch slot, or `prefetch[1]` for `#imm.l`). A
    /// 0-cycle, non-bus, snapshot-visible internal step. **No 24-bit mask** — this is an operand VALUE, not
    /// an address (distinct from [`MicroOp::EaCalc`], which masks to `ADDR_MASK`).
    Combine32 { hi: Slot, lo: Operand, dst: Slot },
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
    #[inline]
    fn resolve(&self, op: Operand, regs: &Registers) -> u32 {
        match op {
            Operand::Scratch(s) => self.scratch[s as usize],
            Operand::ScratchHi16(s) => self.scratch[s as usize] >> 16,
            Operand::DataRegFull(n) => regs.d[n as usize],
            Operand::DataRegLow16(n) => regs.d[n as usize] & 0xFFFF,
            Operand::DataRegLow8(n) => regs.d[n as usize] & 0xFF,
            Operand::AddrRegLow16(n) => regs.addr_reg(n as usize) & 0xFFFF,
            Operand::AddrReg(n) => regs.addr_reg(n as usize),
            Operand::ImmWord => regs.prefetch[1] as u32,
            Operand::Zero => 0,
            Operand::WordStep => 2,
            Operand::DispWord => sign_extend16(regs.prefetch[1]),
            Operand::PcOfExt => regs.pc.wrapping_add(2),
            Operand::ExtWordHi => (regs.prefetch[1] as u32) << 16,
            Operand::ExtWordRaw => regs.prefetch[1] as u32,
            Operand::BriefIndex => {
                // The single isolated runtime branch: decode the brief extension word's index spec.
                let ext = regs.prefetch[1];
                let reg = ((ext >> 12) & 7) as usize;
                let raw = if ext & 0x8000 != 0 {
                    regs.addr_reg(reg) // bit15 = 1 → address register (A7-aware)
                } else {
                    regs.d[reg] // bit15 = 0 → data register
                };
                if ext & 0x0800 != 0 {
                    raw // bit11 = 1 → long: the full 32 bits
                } else {
                    sign_extend16(raw as u16) // bit11 = 0 → word: sign-extend the low 16
                }
            }
            Operand::BriefDisp8 => sign_extend8((regs.prefetch[1] & 0xFF) as u8),
        }
    }

    /// **Driver 1 — run-to-completion** (the default fast path): execute every remaining micro-op in
    /// order, returning the total master cycles. Drives the *same* [`Self::exec_one`] the quiesce path
    /// uses, so the two paths cannot diverge.
    #[inline]
    pub fn run_to_completion(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
        let mut total = 0;
        while !self.is_done() {
            total += self.exec_one(regs, bus);
        }
        total
    }

    /// Execute exactly the next micro-op, advancing the cursor; returns the master cycles it cost.
    /// This is the single shared "cook" both drivers call — identical behavior by construction.
    #[inline]
    pub fn exec_one(&mut self, regs: &mut Registers, bus: &mut impl Bus68k) -> u32 {
        let cycles = match self.ops[self.step as usize] {
            MicroOp::Read {
                addr,
                fc,
                size,
                dst,
            } => {
                let address = self.resolve(addr, regs);
                let fc = regs.fc(matches!(fc, Fc::Program));
                // A byte access uses read8 (the single addressed cell, zero-extended); a word uses read16.
                // A long is never a single `Read` — it is two word `Read`s assembled by `Combine32`, so the
                // builder only ever emits word `Read`s for a long operand.
                let value = match size {
                    Size::Byte => bus.read8(address, fc) as u32,
                    Size::Word => bus.read16(address, fc) as u32,
                    Size::Long => unreachable!("a long Read is two word Reads + Combine32"),
                };
                self.scratch[dst as usize] = value;
                4
            }
            MicroOp::Write {
                addr,
                fc,
                size,
                value,
            } => {
                let address = self.resolve(addr, regs);
                let fc = regs.fc(matches!(fc, Fc::Program));
                let v = self.resolve(value, regs);
                // A long is never a single `Write` — it is two word `Write`s (the builder feeds the hi word
                // via `Operand::ScratchHi16` and the lo word via `Operand::Scratch`, each truncated to 16).
                match size {
                    Size::Byte => bus.write8(address, fc, v as u8),
                    Size::Word => bus.write16(address, fc, v as u16),
                    Size::Long => unreachable!("a long Write is two word Writes"),
                }
                4
            }
            MicroOp::Alu {
                op,
                size,
                a,
                b,
                dst,
            } => {
                let lhs = self.resolve(a, regs);
                let rhs = self.resolve(b, regs);
                // Compute at the operand-size flag boundary; carry the result (zero-extended to 32) + CCR
                // uniformly.
                let (result, ccr) = match size {
                    Size::Word => {
                        let (r, ccr) = match op {
                            AluOp::Add => add_w(lhs as u16, rhs as u16),
                            AluOp::Sub => sub_w(lhs as u16, rhs as u16),
                        };
                        (r as u32, ccr)
                    }
                    Size::Byte => {
                        let (r, ccr) = match op {
                            AluOp::Add => add_b(lhs as u8, rhs as u8),
                            AluOp::Sub => sub_b(lhs as u8, rhs as u8),
                        };
                        (r as u32, ccr)
                    }
                    Size::Long => match op {
                        AluOp::Add => add_l(lhs, rhs),
                        AluOp::Sub => sub_l(lhs, rhs),
                    },
                };
                regs.sr = (regs.sr & 0xFF00) | ccr;
                match dst {
                    Dest::Scratch(s) => self.scratch[s as usize] = result,
                    Dest::DataReg(n) => regs.d[n as usize] = result,
                    Dest::DataRegLow16(n) => {
                        regs.d[n as usize] = (regs.d[n as usize] & 0xFFFF_0000) | (result & 0xFFFF);
                    }
                    Dest::DataRegLow8(n) => {
                        regs.d[n as usize] = (regs.d[n as usize] & 0xFFFF_FF00) | (result & 0xFF);
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
            MicroOp::AdjustAddr { reg, delta } => {
                let cur = regs.addr_reg(reg as usize);
                regs.addr_reg_set(reg as usize, cur.wrapping_add(delta as i32 as u32));
                0
            }
            MicroOp::EaCalc {
                base,
                index,
                disp,
                dst,
            } => {
                // FIXED 3-way wrapping_add — no per-mode branch. The builder selects the legs.
                let ea = self
                    .resolve(base, regs)
                    .wrapping_add(self.resolve(index, regs))
                    .wrapping_add(self.resolve(disp, regs))
                    & ADDR_MASK;
                self.scratch[dst as usize] = ea;
                0
            }
            MicroOp::Combine32 { hi, lo, dst } => {
                // Assemble the 32-bit long value — NO mask (this is a value, not an address).
                let value = (self.scratch[hi as usize] << 16) | self.resolve(lo, regs);
                self.scratch[dst as usize] = value;
                0
            }
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

        let mut st = MicroState::from_ops(&[MicroOp::Read {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Word,
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
                size: Size::Word,
                value: 0xABCD,
            }]
        );
    }

    #[test]
    fn write_word_writes_value_at_address_and_emits_transaction() {
        let mut regs = regs();
        let mut bus = FlatBus::new();

        let mut st = MicroState::from_ops(&[MicroOp::Write {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Word,
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
                size: Size::Word,
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
                size: Size::Word,
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
            op: AluOp::Add,
            size: Size::Word,
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
            op: AluOp::Add,
            size: Size::Word,
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
    fn alu_sub_w_computes_difference_and_sets_flags() {
        let mut regs = regs();
        regs.d[5] = 0x3752_7B7D; // minuend Dn; low 0x7B7D
        regs.sr = 0x271D;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Sub,
            size: Size::Word,
            a: Operand::DataRegLow16(5),
            b: Operand::Scratch(0),
            dst: Dest::DataRegLow16(5),
        }]);
        st.scratch[0] = 0xF2BF; // subtrahend

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.d[5], 0x3752_88BE, "0x7B7D - 0xF2BF (borrow wraps)");
        assert_eq!(
            regs.sr, 0x271B,
            "N|V|C|X: negative result, signed overflow, borrow"
        );
    }

    #[test]
    fn imm_word_operand_reads_prefetch_word_1() {
        let mut regs = regs();
        regs.prefetch = [0xDE7C, 0x8EF1];
        regs.d[7] = 0x1BC0_F680;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Word,
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
    fn adjust_addr_postincrements_an_with_zero_cost() {
        // (An)+ side effect: An += delta, no bus access, 0 cycles.
        let mut regs = regs();
        regs.a[2] = 0x0010_0040;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::AdjustAddr { reg: 2, delta: 2 }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            cycles, 0,
            "AdjustAddr is a 0-cycle one-shot register side effect"
        );
        assert_eq!(
            regs.a[2], 0x0010_0042,
            "An post-incremented by the word step"
        );
        assert_eq!(st.step, 1);
        assert!(bus.log.is_empty(), "AdjustAddr touches no bus");
    }

    #[test]
    fn adjust_addr_predecrements_an() {
        // -(An) side effect: An -= step (delta negative).
        let mut regs = regs();
        regs.a[5] = 0x0010_0040;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::AdjustAddr { reg: 5, delta: -2 }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.a[5], 0x0010_003E,
            "An pre-decremented by the word step"
        );
    }

    #[test]
    fn adjust_addr_routes_a7_through_the_active_stack_pointer() {
        // A7 is ssp/usp, not a[7]; AdjustAddr must write through addr_reg_set.
        let mut regs = regs(); // supervisor mode
        regs.ssp = 0x0010_0000;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::AdjustAddr { reg: 7, delta: 2 }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.ssp, 0x0010_0002,
            "A7 adjust hit the supervisor stack pointer"
        );
    }

    #[test]
    fn run_to_completion_drives_all_micro_ops() {
        let mut regs = regs();
        let mut bus = FlatBus::new();
        bus.poke(0x1000, 0x12);
        bus.poke(0x1001, 0x34);

        let mut st = MicroState::from_ops(&[
            MicroOp::Read {
                addr: Operand::Scratch(0),
                fc: Fc::Data,
                size: Size::Word,
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
            MicroOp::Read {
                addr: Operand::Scratch(0),
                fc: Fc::Data,
                size: Size::Word,
                dst: 1,
            },
            MicroOp::Internal { cycles: 4 },
            MicroOp::Write {
                addr: Operand::Scratch(2),
                fc: Fc::Data,
                size: Size::Word,
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
    fn c0_word_read_carries_size_word_and_is_byte_identical() {
        // C0 vocabulary: `Read`/`Write` take a `size`, `Alu` takes a `size`, `AluOp` is {Add,Sub}.
        // The word path must behave exactly as `ReadWord`/`WriteWord`/`AluOp::AddW` did before.
        let mut regs = regs();
        let mut bus = FlatBus::new();
        bus.poke(0x1000, 0xAB);
        bus.poke(0x1001, 0xCD);

        let mut st = MicroState::from_ops(&[MicroOp::Read {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Word,
            dst: 1,
        }]);
        st.scratch[0] = 0x1000;

        let cycles = st.exec_one(&mut regs, &mut bus);
        assert_eq!(cycles, 4, "a word bus access is 4 master cycles");
        assert_eq!(st.scratch[1], 0xABCD, "operand landed in scratch slot 1");
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x1000,
                size: Size::Word,
                value: 0xABCD,
            }]
        );
    }

    #[test]
    fn c0_alu_add_sub_with_size_word_match_old_behavior() {
        let mut regs_add = regs();
        regs_add.d[5] = 0x020D_2596;
        regs_add.sr = 0x2717;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Word,
            a: Operand::DataRegLow16(5),
            b: Operand::Scratch(0),
            dst: Dest::Scratch(1),
        }]);
        st.scratch[0] = 0x3FE0;
        st.exec_one(&mut regs_add, &mut bus);
        assert_eq!(st.scratch[1], 0x6576, "0x2596 + 0x3FE0");
        assert_eq!(regs_add.sr, 0x2700, "CCR cleared, system byte preserved");

        let mut regs2 = regs();
        regs2.d[5] = 0x3752_7B7D;
        regs2.sr = 0x271D;
        let mut st2 = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Sub,
            size: Size::Word,
            a: Operand::DataRegLow16(5),
            b: Operand::Scratch(0),
            dst: Dest::DataRegLow16(5),
        }]);
        st2.scratch[0] = 0xF2BF;
        st2.exec_one(&mut regs2, &mut bus);
        assert_eq!(regs2.d[5], 0x3752_88BE, "0x7B7D - 0xF2BF (borrow wraps)");
        assert_eq!(regs2.sr, 0x271B, "N|V|C|X");
    }

    #[test]
    fn ea_calc_sums_base_index_disp_into_scratch_masked() {
        // EaCalc is a FIXED 3-way wrapping_add masked to the 24-bit bus — no per-mode match.
        // base = A1, index = ·(Zero), disp = sign_extend16(prefetch[1]).
        let mut regs = regs();
        regs.a[1] = 0x00FF_FFF0;
        regs.prefetch = [0xD46D, 0xFFF8]; // disp word = 0xFFF8 → sign-extend → -8
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::AddrReg(1),
            index: Operand::Zero,
            disp: Operand::DispWord,
            dst: 2,
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            cycles, 0,
            "EaCalc is an internal compute — 0 standalone cycles"
        );
        assert_eq!(
            st.scratch[2], 0x00FF_FFE8,
            "0xFFFFF0 + (-8) = 0xFFFFE8, masked to 24 bits"
        );
        assert!(bus.log.is_empty(), "EaCalc touches no bus");
    }

    #[test]
    fn ea_calc_wraps_around_the_24bit_bus() {
        // base near the top of the 24-bit space + a positive disp wraps under ADDR_MASK.
        let mut regs = regs();
        regs.a[3] = 0x00FF_FFFE;
        regs.prefetch = [0x0000, 0x0010]; // disp = +0x10
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::AddrReg(3),
            index: Operand::Zero,
            disp: Operand::DispWord,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            st.scratch[0], 0x0000_000E,
            "0xFFFFFE + 0x10 wraps to 0x0E under the 24-bit mask"
        );
    }

    #[test]
    fn disp_word_resolves_sign_extended_prefetch_word_1() {
        // Operand::DispWord = sign_extend16(prefetch[1]) as u32 — a full 32-bit sign extension before
        // EaCalc masks it. Resolve it via a Zero+Zero+DispWord EaCalc (the abs.w shape).
        let mut regs = regs();
        regs.prefetch = [0xDA78, 0xCC1A]; // abs.w disp 0xCC1A → sign-extend → 0xFFCC1A
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::Zero,
            disp: Operand::DispWord,
            dst: 1,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            st.scratch[1], 0x00FF_CC1A,
            "abs.w EA = sign_extend16(0xCC1A) masked to 24 bits"
        );
    }

    #[test]
    fn pc_of_ext_resolves_to_pc_plus_2() {
        // Operand::PcOfExt = regs.pc.wrapping_add(2) — the PC-relative base is the *extension-word*
        // address (the word after the opcode), captured by EaCalc BEFORE any Prefetch advances pc.
        // d16(PC) shape: EaCalc(PcOfExt, ·, DispWord). base = pc+2, disp = sign_extend16(prefetch[1]).
        let mut regs = regs();
        regs.pc = 0x0000_0C00;
        regs.prefetch = [0xD07A, 0xD8E2]; // disp 0xD8E2 → sign-extend → -10014
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::PcOfExt,
            index: Operand::Zero,
            disp: Operand::DispWord,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        // (pc+2) + sign_extend16(disp) = 0xC02 + (-10014) = -6940 → 0xFFFF_E4E4 → masked 0xFF_E4E4.
        assert_eq!(
            st.scratch[0], 0x00FF_E4E4,
            "d16(PC) EA = (pc+2) + sign_extend16(disp), masked to 24 bits"
        );
    }

    #[test]
    fn ext_word_hi_resolves_to_prefetch_word_1_shifted_left_16() {
        // Operand::ExtWordHi = (prefetch[1] as u32) << 16 — the abs.l HIGH word capture, taken from the
        // queue BEFORE the first interleaved Prefetch shifts the LOW word in.
        let mut regs = regs();
        regs.prefetch = [0xD079, 0xD1CC]; // abs.l high word = 0xD1CC
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::Zero,
            disp: Operand::ExtWordHi,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        // (0xD1CC << 16) = 0xD1CC_0000 → masked to 24 bits = 0x00CC_0000.
        assert_eq!(
            st.scratch[0], 0x00CC_0000,
            "abs.l HIGH = (prefetch[1] << 16) masked to 24 bits"
        );
    }

    #[test]
    fn ext_word_raw_resolves_to_prefetch_word_1_unmodified() {
        // Operand::ExtWordRaw = prefetch[1] as u32 — the abs.l LOW word capture, read from the queue
        // AFTER the interleaved Prefetch (NEVER from that prefetch's bus-return value). Combined with the
        // already-captured HIGH it forms the full 24-bit address.
        let mut regs = regs();
        regs.prefetch = [0x0000, 0x9C2A]; // post-prefetch: prefetch[1] is now the LOW word 0x9C2A
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Scratch(0), // a prior EaCalc deposited the HIGH word here
            index: Operand::Zero,
            disp: Operand::ExtWordRaw,
            dst: 1,
        }]);
        st.scratch[0] = 0x00CC_0000; // HIGH word from the first EaCalc

        st.exec_one(&mut regs, &mut bus);

        // 0x00CC_0000 + 0x9C2A = 0x00CC_9C2A.
        assert_eq!(
            st.scratch[1], 0x00CC_9C2A,
            "abs.l ADDR = HIGH + ExtWordRaw (prefetch[1] low word)"
        );
    }

    #[test]
    fn operand_zero_resolves_to_zero() {
        // An inert EaCalc leg: Zero contributes nothing to the sum.
        let mut regs = regs();
        regs.a[4] = 0x0012_3456;
        regs.prefetch = [0x0000, 0x0000];
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::AddrReg(4),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(st.scratch[0], 0x0012_3456, "base alone; Zero legs inert");
    }

    #[test]
    fn brief_disp8_resolves_sign_extended_low_byte_of_ext_word() {
        // Operand::BriefDisp8 = sign_extend8(prefetch[1] & 0xFF). The brief extension word's low byte is a
        // signed 8-bit displacement; the upper byte (D/A, index reg, W/L) is NOT part of the disp.
        let mut regs = regs();
        regs.prefetch = [0xD075, 0xA2F0]; // brief ext low byte 0xF0 → sign-extend → -16
        let mut bus = FlatBus::new();
        // Resolve via an inert-base EaCalc so we can read the resolved value directly.
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::Zero,
            disp: Operand::BriefDisp8,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        // sign_extend8(0xF0) = 0xFFFF_FFF0 → masked to 24 bits = 0x00FF_FFF0.
        assert_eq!(
            st.scratch[0], 0x00FF_FFF0,
            "BriefDisp8 = sign_extend8(prefetch[1] & 0xFF), masked"
        );
    }

    #[test]
    fn brief_index_data_reg_word_sign_extends_low16() {
        // bit15 = 0 (D), bits14-12 = 3 (D3), bit11 = 0 (W → sign-extend low 16). Brief ext = 0x3000.
        let mut regs = regs();
        regs.d[3] = 0x1234_F008; // low 16 = 0xF008 → sign-extend → 0xFFFF_F008
        regs.prefetch = [0xD030, 0x3000];
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::BriefIndex,
            disp: Operand::Zero,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        // sign_extend16(0xF008) = 0xFFFF_F008 → masked to 24 bits = 0x00FF_F008.
        assert_eq!(
            st.scratch[0], 0x00FF_F008,
            "BriefIndex (D, W) = sign_extend16(Dn low 16)"
        );
    }

    #[test]
    fn brief_index_data_reg_long_uses_full_32() {
        // bit15 = 0 (D), bits14-12 = 3 (D3), bit11 = 1 (L → full 32). Brief ext = 0x3800.
        let mut regs = regs();
        regs.d[3] = 0x0012_F008; // full 32 bits used
        regs.prefetch = [0xD030, 0x3800];
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::BriefIndex,
            disp: Operand::Zero,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        // full 0x0012_F008 → masked to 24 bits = 0x0012_F008.
        assert_eq!(
            st.scratch[0], 0x0012_F008,
            "BriefIndex (D, L) = full 32 bits of Dn"
        );
    }

    #[test]
    fn brief_index_addr_reg_word_sign_extends_low16() {
        // bit15 = 1 (A), bits14-12 = 5 (A5), bit11 = 0 (W). Brief ext = 0xD000 (1101 0000 0000 0000).
        let mut regs = regs();
        regs.a[5] = 0x00AB_8001; // low 16 = 0x8001 → sign-extend → 0xFFFF_8001
        regs.prefetch = [0xD075, 0xD000];
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::BriefIndex,
            disp: Operand::Zero,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        // sign_extend16(0x8001) = 0xFFFF_8001 → masked to 24 bits = 0x00FF_8001.
        assert_eq!(
            st.scratch[0], 0x00FF_8001,
            "BriefIndex (A, W) = sign_extend16(An low 16)"
        );
    }

    #[test]
    fn brief_index_addr_reg_long_uses_full_32_and_a7_aware() {
        // bit15 = 1 (A), bits14-12 = 7 (A7 → active stack pointer), bit11 = 1 (L). Brief ext = 0xF800.
        let mut regs = regs(); // supervisor → A7 == ssp
        regs.ssp = 0x0034_5678;
        regs.prefetch = [0xD075, 0xF800];
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::BriefIndex,
            disp: Operand::Zero,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            st.scratch[0], 0x0034_5678,
            "BriefIndex (A7, L) reads the active stack pointer, full 32 bits"
        );
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

    #[test]
    fn alu_add_b_uses_0x80_overflow_and_0x100_carry_boundary_and_writes_low8() {
        // Pinned to the real SST `d604 [ADD.b D4,D3]`: D3 low byte 0x5C + D4 low byte 0x2D = 0x89. Two
        // positive bytes producing a bit7-set (negative) byte → N and V set; no carry out of bit7 → C/X
        // clear. The result is written to D3's LOW BYTE only — the upper 24 bits (0xD83A3F) are preserved.
        let mut regs = regs();
        regs.d[3] = 0xD83A_3F5C; // dest minuend; low byte 0x5C
        regs.d[4] = 0x8019_832D; // source; low byte 0x2D
        regs.sr = 0x2708; // CCR = N (from a prior op); the add recomputes it
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Byte,
            a: Operand::DataRegLow8(3),
            b: Operand::DataRegLow8(4),
            dst: Dest::DataRegLow8(3),
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[3], 0xD83A_3F89,
            "low byte = 0x5C + 0x2D = 0x89; upper 24 bits preserved"
        );
        assert_eq!(
            regs.sr, 0x270A,
            "CCR = N|V (negative byte, signed overflow)"
        );
    }

    #[test]
    fn alu_add_b_sets_carry_and_extend_on_byte_overflow() {
        // 0xF0 + 0x20 = 0x110 → low byte 0x10, carry out of bit7 → C and X set; result bit7 clear → N clear;
        // operands have differing signs (0xF0 negative, 0x20 positive) → no V.
        let mut regs = regs();
        regs.d[0] = 0x1234_56F0;
        regs.d[1] = 0x0000_0020;
        regs.sr = 0x2700;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Byte,
            a: Operand::DataRegLow8(0),
            b: Operand::DataRegLow8(1),
            dst: Dest::DataRegLow8(0),
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.d[0], 0x1234_5610, "low byte wrapped to 0x10");
        assert_eq!(regs.sr, 0x2711, "X|C set (carry out of bit7); N/Z/V clear");
    }

    #[test]
    fn alu_sub_b_uses_byte_boundaries_and_writes_low8() {
        // 0x10 - 0x20 = -0x10 → 0xF0 (borrow). Byte borrow → C and X set; result bit7 set → N; minuend and
        // subtrahend differ in sign? 0x10 positive, 0x20 positive → same sign, no overflow → V clear.
        let mut regs = regs();
        regs.d[2] = 0xAABB_CC10;
        regs.d[3] = 0x0000_0020;
        regs.sr = 0x2700;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Sub,
            size: Size::Byte,
            a: Operand::DataRegLow8(2),
            b: Operand::DataRegLow8(3),
            dst: Dest::DataRegLow8(2),
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[2], 0xAABB_CCF0,
            "low byte = 0x10 - 0x20 = 0xF0; upper 24 bits preserved"
        );
        assert_eq!(regs.sr, 0x2719, "X|N|C set (borrow, negative byte)");
    }

    #[test]
    fn byte_read_zero_extends_into_scratch_and_logs_byte_size() {
        // A byte `Read` accesses one cell (`read8`) and zero-extends it into the scratch slot. Pinned to the
        // real SST byte at the even address 0x97EA9E with value 0x45 (the `de11 [ADD.b (A1),D7]` operand).
        let mut regs = regs();
        let mut bus = FlatBus::new();
        bus.poke(0x97_EA9E, 0x45);
        let mut st = MicroState::from_ops(&[MicroOp::Read {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Byte,
            dst: 1,
        }]);
        st.scratch[0] = 0x97_EA9E;

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 4, "a byte bus access is 4 master cycles");
        assert_eq!(
            st.scratch[1], 0x0000_0045,
            "byte zero-extended into scratch"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x97_EA9E,
                size: Size::Byte,
                value: 0x45,
            }]
        );
    }

    #[test]
    fn combine32_assembles_hi_lo_into_long_value_without_masking() {
        // Combine32: (scratch[hi] << 16) | resolve(lo). NO 24-bit mask — it is an operand VALUE, so a hi
        // word above the 24-bit address span survives (distinct from EaCalc, which masks to ADDR_MASK).
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Combine32 {
            hi: 0,
            lo: Operand::Scratch(1),
            dst: 2,
        }]);
        st.scratch[0] = 0x0000_FF80; // hi word 0xFF80 — above the 24-bit mask
        st.scratch[1] = 0x0000_1234; // lo word

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "Combine32 is an internal compute — 0 cycles");
        assert_eq!(
            st.scratch[2], 0xFF80_1234,
            "long value assembled hi<<16 | lo, UNMASKED"
        );
        assert!(bus.log.is_empty(), "Combine32 touches no bus");
    }

    #[test]
    fn scratch_hi16_resolves_to_high_word_of_scratch() {
        // Operand::ScratchHi16(s) = scratch[s] >> 16 — the hi word fed to the first Write of a long store.
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Write {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Word,
            value: Operand::ScratchHi16(1),
        }]);
        st.scratch[0] = 0x2000;
        st.scratch[1] = 0xABCD_1234; // hi word 0xABCD

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            bus.peek(0x2000),
            0xAB,
            "Write stored the hi word's upper byte"
        );
        assert_eq!(
            bus.peek(0x2001),
            0xCD,
            "Write stored the hi word's lower byte"
        );
    }

    #[test]
    fn data_reg_full_resolves_to_full_32_and_dest_data_reg_writes_full_32() {
        // Operand::DataRegFull(n) = regs.d[n] (full 32); Dest::DataReg(n) writes the full 32-bit result.
        let mut regs = regs();
        regs.d[4] = 0x1357_9BDF;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Long,
            a: Operand::DataRegFull(4),
            b: Operand::Scratch(0),
            dst: Dest::DataReg(4),
        }]);
        st.scratch[0] = 0x0000_0001;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[4], 0x1357_9BE0,
            "full 32-bit add written to all of D4"
        );
    }

    #[test]
    fn alu_add_l_uses_0x80000000_boundary_and_writes_full_32() {
        // Pinned to the real SST `d491 [ADD.l (A1),D2]`: D2 0x7F165E69 + operand 0x2026E993 = 0x9F3D47FC.
        // bit31 set → N; not zero → no Z; two positives summing to a negative → V; no carry out of bit31 → no
        // C/X. SR 0x270E → 0x270A (N|V).
        let mut regs = regs();
        regs.d[2] = 0x7F16_5E69;
        regs.sr = 0x270E;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Long,
            a: Operand::DataRegFull(2),
            b: Operand::Scratch(0),
            dst: Dest::DataReg(2),
        }]);
        st.scratch[0] = 0x2026_E993;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.d[2], 0x9F3D_47FC, "0x7F165E69 + 0x2026E993");
        assert_eq!(regs.sr, 0x270A, "N|V (negative result, signed overflow)");
    }

    #[test]
    fn alu_add_l_sets_carry_and_extend_on_32bit_overflow() {
        // 0xFFFF_FFFF + 0x0000_0002 = 0x1_0000_0001 → low 32 = 0x1; carry out of bit31 → C and X; result
        // bit31 clear → no N; operands differ in sign → no V.
        let mut regs = regs();
        regs.d[0] = 0xFFFF_FFFF;
        regs.sr = 0x2700;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Add,
            size: Size::Long,
            a: Operand::DataRegFull(0),
            b: Operand::Scratch(0),
            dst: Dest::DataReg(0),
        }]);
        st.scratch[0] = 0x0000_0002;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.d[0], 0x0000_0001, "wrapped to 0x1");
        assert_eq!(regs.sr, 0x2711, "X|C set; N/Z/V clear");
    }

    #[test]
    fn alu_sub_l_computes_difference_at_long_boundary() {
        // 0x0000_0001 - 0x0000_0002 = 0xFFFF_FFFF (borrow). Borrow → C and X; result bit31 set → N; same-sign
        // minuend/subtrahend → no V.
        let mut regs = regs();
        regs.d[1] = 0x0000_0001;
        regs.sr = 0x2700;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Sub,
            size: Size::Long,
            a: Operand::DataRegFull(1),
            b: Operand::Scratch(0),
            dst: Dest::DataReg(1),
        }]);
        st.scratch[0] = 0x0000_0002;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.d[1], 0xFFFF_FFFF, "0x1 - 0x2 borrows to 0xFFFF_FFFF");
        assert_eq!(regs.sr, 0x2719, "X|N|C set (borrow, negative result)");
    }

    #[test]
    fn byte_write_stores_low_byte_of_value_and_logs_byte_size() {
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Write {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Byte,
            value: Operand::Scratch(1),
        }]);
        st.scratch[0] = 0x2001; // odd address — drives the LDS half
        st.scratch[1] = 0x0000_12A3; // only the low byte 0xA3 is written

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(
            bus.peek(0x2001),
            0xA3,
            "the low byte was written at the address"
        );
        assert_eq!(bus.peek(0x2000), 0x00, "the neighbour byte is untouched");
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 0x2001,
                size: Size::Byte,
                value: 0xA3,
            }]
        );
    }
}
