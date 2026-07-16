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
use super::registers::{
    Registers, CCR_C, CCR_N, CCR_V, CCR_X, CCR_Z, SR_IMPLEMENTED, SR_SUPERVISOR, SR_TRACE,
};

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

/// Evaluate one of the 16 68000 branch conditions (`cc` = the `cccc` field of a `Bcc`/`DBcc`/`Scc`) against
/// the live CCR (the low byte of `sr`: X|N|Z|V|C). A **pure** helper — NOT a micro-op — called by `decode`
/// to resolve a conditional branch's taken/not-taken path at decode time (so the interpreter stays a flat
/// linear recipe). `T` (cc 0, the always-taken `BRA`) and `F` (cc 1, the always-false code that is actually
/// `BSR`, decoded elsewhere) are the two flag-independent conditions.
#[inline]
pub fn condition_true(cc: u8, sr: u16) -> bool {
    let c = sr & CCR_C != 0;
    let v = sr & CCR_V != 0;
    let z = sr & CCR_Z != 0;
    let n = sr & CCR_N != 0;
    match cc & 0xF {
        0 => true,            // T  — always (BRA)
        1 => false,           // F  — never (the BSR encoding; decoded separately)
        2 => !c && !z,        // HI
        3 => c || z,          // LS
        4 => !c,              // CC / HS
        5 => c,               // CS / LO
        6 => !z,              // NE
        7 => z,               // EQ
        8 => !v,              // VC
        9 => v,               // VS
        10 => !n,             // PL
        11 => n,              // MI
        12 => n == v,         // GE
        13 => n != v,         // LT
        14 => (n == v) && !z, // GT
        _ => z || (n != v),   // LE (cc 15)
    }
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
/// (MOVEM-class) get a generator variant later. Sized to the **measured M3 worst recipe**: `MOVE.l
/// (abs.l),(abs.l)` — a long source from `abs.l` (two-word address assembly + two-word read) and a long
/// `abs.l` destination (two-word address assembly + two-word write), no destination read (MOVE is
/// write-only). That recipe is **17 micro-ops**:
///   src: `EaCalc(HI), Prefetch, EaCalc(addr), Prefetch, Read.hi, EaCalc(lo addr), Read.lo, Combine32`  (8)
///   alu: `Alu{Move}` (parks the 32-bit copy)                                                            (1)
///   dst: `EaCalc(HI), Prefetch, EaCalc(addr), Write.hi, EaCalc(lo addr), Write.lo, Prefetch, Prefetch`  (8)
/// 20 = 17 + headroom. The **E3 address-error frame** (`install_address_error` → the 14-byte group-0 frame)
/// is the new longest recipe at **19** micro-ops: `Internal(n4), EnterException, AdjustAddr(SP,−14)`, the
/// **7** frame writes (`PCL/SR/PCH/IR/aLo/SSW/aHi`, each a single `Write` at an [`Operand::SpPlus`] address —
/// no per-write `EaCalc`, which is what keeps it ≤ 20), then the 9-op shared
/// `vector_fetch_and_reload` (`LoadImm, Read, EaCalc, Read, Combine32, SetPc, Prefetch, Internal(n2),
/// Prefetch`). 19 ≤ 20 was the prior bound.
///
/// **Bumped to 40 in C4 (`MOVEM`) — the one structural change.** `MOVEM` expands its register-list mask into
/// a per-register linear recipe (one [`MicroOp::MovemStore`]/[`MicroOp::MovemLoad`] per set register, each
/// doing its own bus access(es) internally), so the worst recipe is `MOVEM.l (xxx).l,<16 regs>` mem→reg:
/// three leading `Prefetch`s + two `EaCalc`s (the two-ext-word `abs.l` assembly) + 16 `MovemLoad`s + the
/// phantom `Read` + one trailing `Prefetch` = 23 ops. 40 = comfortable headroom for both the per-register
/// representation and the C5 `.l` extension (each register stays ONE op doing two internal word accesses, so
/// the op count does not grow with size). [`MicroState`] stays a FIXED `[MicroOp; MAX_OPS]` array (bincode /
/// `Copy` invariant — NOT a `Vec`); the bump only widens the fixed array. Public so the EA builder
/// ([`super::ea::RecipeBuf`]) can size its fixed staging array to the same bound.
pub const MAX_OPS: usize = 40;

/// Number of scratch slots carrying values between micro-ops within one instruction. Sized to the **E3
/// address-error frame** — the new worst recipe (`install_address_error`): it carries five live frame-field
/// values (stacked-PC slot 0, captured-SR slot 1, faulting-addr slot 2, IR slot 8, SSW slot 9) **disjoint**
/// from the shared `vector_fetch_and_reload` slots 3..=7 (vector addr / handler-hi / vector-lo-addr /
/// handler-lo / assembled handler), so no field aliases the vector fetch. The prior worst (`MOVE.l
/// (abs.l),(abs.l)`) used slots 0..=5. Fixed-size for bincode snapshot/restore.
const SCRATCH_SLOTS: usize = 10;

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

impl Size {
    /// Number of bytes this access width touches. (The generic `crate::bus` layer re-exports this enum,
    /// so its byte-count helper lives here on the single definition.)
    pub fn bytes(self) -> u32 {
        match self {
            Size::Byte => 1,
            Size::Word => 2,
            Size::Long => 4,
        }
    }
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
    /// The full 16-bit status register, zero-extended: `regs.sr as u32` (T | S | I2-I0 | X/N/Z/V/C — the WHOLE
    /// SR incl the system byte, NOT just the CCR). The write value of `MOVEfromSR` (`EA/Dn.w = SR`), fed to a
    /// no-flag word write ([`MicroOp::SetWord`]) so the store leaves the SR itself byte-identical.
    Sr,
    /// A constant zero — an inert leg of an [`MicroOp::EaCalc`] (e.g. the index/base a mode doesn't use).
    Zero,
    /// A constant `2` — the word stride between the two halves of a long memory access. The low word of a
    /// `.l` operand lives at `addr + 2`; an [`MicroOp::EaCalc`] adds this to the materialized base to form
    /// the low half's address.
    WordStep,
    /// A decode-time **shift count** baked into the recipe as a literal value (the immediate count 1-8 of a
    /// register shift's `ccc != 0 ? ccc : 8`, or the constant `1` of a memory shift-by-1). Resolved as the
    /// literal `u8` (the exec masks it `& 63`). Mirrors the constant operands [`Operand::Zero`]/
    /// [`Operand::WordStep`]; distinct from [`Operand::DataRegFull`] (the dynamic `Dn`-count form, which the
    /// recipe passes directly and the exec masks `& 63` at run time).
    ShiftCount(u8),
    /// A decode-time **quick immediate** baked into the recipe as a literal value (the `ADDQ`/`SUBQ`
    /// immediate `qqq != 0 ? qqq : 8`, a constant 1-8, zero-extended). Resolved as the literal `u8`. Mirrors
    /// the constant operands [`Operand::Zero`]/[`Operand::WordStep`]/[`Operand::ShiftCount`]; distinct from
    /// [`Operand::ImmWord`] (a fetched extension word) — the value is fully known at decode time and needs no
    /// bus access.
    Quick(u8),
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
    /// The sign-extended 8-bit branch displacement of a `Bcc`/`BSR`: `sign_extend8(prefetch[0] & 0xFF)`. It
    /// comes from the **opcode** word (`prefetch[0]`), NOT `prefetch[1]` (the word-form displacement). Used by
    /// a taken byte-form branch's [`MicroOp::TargetCalc`].
    BranchDisp8,
    /// `regs.pc.wrapping_add(n)` — the **return-address base** of a `BSR`/`JSR` push (`n` = the instruction's
    /// byte length: 2 for a byte-form BSR, 4 for a word-form BSR / a one-extension-word JSR, 6 for an `abs.l`
    /// JSR). The pushed 32-bit return address is `pc + n` (`pc` is the opcode address at decode time — the
    /// push runs **before** any `Prefetch` advances it), computed UNMASKED via [`MicroOp::TargetCalc`].
    PcPlus(u8),
    /// `regs.addr_reg(7).wrapping_add(n)` — the active A7 plus a signed byte offset, used as a frame-write
    /// **address** without a per-write [`MicroOp::EaCalc`]. The address-error abort's 14-byte group-0 frame
    /// (E3) pushes seven words at fixed offsets `B+0..B+12` from the post-`AdjustAddr` stack top; `SpPlus`
    /// addresses each (`B = A7` after `AdjustAddr(SP,−14)`) so the whole frame recipe stays under
    /// [`MAX_OPS`]. A7 is the supervisor stack here (the abort already set S), routed via
    /// [`Registers::addr_reg`].
    SpPlus(i8),
}

/// Where a [`MicroOp::Alu`] result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum Dest {
    /// A scratch slot (e.g. an intermediate later written to memory).
    Scratch(Slot),
    /// The full 32 bits of data register `Dn` — a `.l` write-back (no preserved bits).
    DataReg(u8),
    /// The full 32 bits of address register `An` (the active A7 when `n == 7`, written through
    /// [`Registers::addr_reg_set`] so A7 hits the right stack pointer) — the `MOVEA` write-back. An is
    /// always written full-width (a `.w` MOVEA sign-extends to 32 first), so there is no `.w`/`.b` An dest.
    AddrReg(u8),
    /// The low word of data register `Dn` (its high word is preserved — a `.w` write-back).
    DataRegLow16(u8),
    /// The low byte of data register `Dn` (its upper 24 bits are preserved — a `.b` write-back).
    DataRegLow8(u8),
    /// **No write-back** — flag-only. The [`MicroOp::Alu`] sets the CCR and writes nothing (no register, no
    /// scratch). The compare family (`CMP`/`CMPM`/`CMPI`/`TST` via [`AluOp::Cmp`], and later `CMPA` via
    /// `Cmpa`) computes a subtraction purely for its flags.
    None,
}

/// An ALU operation a [`MicroOp::Alu`] performs (computing into scratch and updating the CCR). The
/// operand width is carried separately by [`MicroOp::Alu`]'s `size`. Grows with arithmetic/logic coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum AluOp {
    /// Add: `dst = a + b`, setting X/N/Z/V/C (at the operand-size boundary).
    Add,
    /// Subtract: `dst = a - b` (a is the minuend), setting X/N/Z/V/C (at the operand-size boundary).
    Sub,
    /// Move: `dst = a` (b is ignored). NOT arithmetic — copies the value and sets **N** (msb at the operand
    /// size), **Z** (value == 0 at the operand size), clears **V** and **C**, and leaves **X** untouched.
    /// The flag op of the `MOVE` family (`MOVE`, not `MOVEA` — `MOVEA` sets no flags). The size-truncated
    /// value is written to `dst` (low8/low16/full32 for byte/word/long).
    Move,
    /// MoveA: `dst = a` (b is ignored), affecting **NO flags** (the `MOVEA` family). A `.w` MoveA
    /// **sign-extends** the source word to 32 bits; a `.l` MoveA writes the full 32 bits unchanged (byte
    /// MOVEA is illegal and never decoded). The result is always written full-width to an address register
    /// (`Dest::AddrReg`), so there is no size-masked write-back. Distinct from [`AluOp::Move`] (which sets
    /// N/Z and writes a size-truncated value).
    MoveA,
    /// Compare: `a - b` (a the minuend) at the operand-size boundary, setting **N/Z/V/C exactly as
    /// [`AluOp::Sub`]** but **PRESERVING X** (CMP/CMPM/CMPI/TST never touch X) and writing **no value** (paired
    /// with [`Dest::None`]). The CCR is `(sub_ccr & !CCR_X) | (regs.sr & CCR_X)` — the subtraction's N/Z/V/C
    /// with the live X re-injected. `TST <ea>` reuses this with `b = Operand::Zero` (`a - 0`). The flag op of
    /// the compare family; distinct from [`AluOp::Sub`] (which recomputes X and writes a result back).
    Cmp,
    /// CompareA: `An(full 32) − b` computed at the **long boundary**, where `b` is **sign-extended word→long
    /// when `size == Word`** (else the full long) — exactly mirroring [`AluOp::MoveA`]'s internal
    /// sign-extension, but applied to the `b` operand rather than `a`. Sets **N/Z/V/C** (from the long
    /// subtraction), **PRESERVES X** (CMPA never touches X — like [`AluOp::Cmp`]), and writes **no value**
    /// (paired with [`Dest::None`]). The minuend `a` is always [`Operand::AddrReg`] (the destination An, full
    /// 32 bits). The flag op of `CMPA <ea>,An`; distinct from [`AluOp::Cmp`] (which compares at the
    /// operand-size boundary with no sign-extension) and [`AluOp::MoveA`] (which writes An and sets no flags).
    Cmpa,
    /// AddA: `An = An + b` computed at the **long boundary**, affecting **NO flags** (the `ADDA` family —
    /// address arithmetic). The addend `b` is **sign-extended word→long when `size == Word`** (else the full
    /// long; byte ADDA is illegal and never decoded), exactly mirroring [`AluOp::MoveA`]'s internal
    /// sign-extension. `a` is the destination [`Operand::AddrReg`] (the augend An, full 32 bits) and the result
    /// is written full-width to that same An ([`Dest::AddrReg`]). Shares the no-flag early-return shape of
    /// [`AluOp::MoveA`] (writes An, leaves the SR untouched), but `a + b` instead of a copy. Distinct from
    /// [`AluOp::Add`] (which computes at the operand-size boundary, sets X/N/Z/V/C, and writes a data register).
    Adda,
    /// SubA: `An = An − b` computed at the **long boundary**, affecting **NO flags** (the `SUBA` family —
    /// address arithmetic). The subtrahend `b` is **sign-extended word→long when `size == Word`** (else the
    /// full long; byte SUBA is illegal and never decoded), exactly mirroring [`AluOp::MoveA`]'s internal
    /// sign-extension. `a` is the destination [`Operand::AddrReg`] (the minuend An, full 32 bits) and the result
    /// is written full-width to that same An ([`Dest::AddrReg`]). A near-exact mirror of [`AluOp::Adda`] (the
    /// no-flag An-write early-return shape), but `a − b` instead of `a + b`. Distinct from [`AluOp::Sub`] (which
    /// computes at the operand-size boundary, sets X/N/Z/V/C, and writes a data register).
    Suba,
    /// And: bitwise `result = a & b` at the operand-size boundary — the flag op of the `AND` family. Shares the
    /// **MOVE flag shape** ([`move_flags`]): sets **N = msb(result at size)**, **Z = (result == 0 at size)**,
    /// clears **V** and **C**, and **PRESERVES X** (logic never touches X — the live X is re-injected as
    /// `ccr_nz | (regs.sr & CCR_X)`, exactly as [`AluOp::Move`]). The size-masked result is written back
    /// (low8/low16/full32 for a `Dn` dest, or parked in [`Dest::Scratch`] for a memory dest the trailing `Write`
    /// stores). AND is commutative, so the `<ea>,Dn` (`a = Dn`) and `Dn,<ea>` (`a = memory`) directions reuse
    /// the same op. Distinct from [`AluOp::Add`] (which recomputes X and sets a real V/C) and [`AluOp::Move`]
    /// (which copies `a`, ignoring `b`).
    And,
    /// Or: bitwise `result = a | b` at the operand-size boundary — the flag op of the `OR` family. Identical to
    /// [`AluOp::And`] in every respect except the bit operation (`|` instead of `&`): shares the **MOVE flag
    /// shape** ([`move_flags`]) — sets **N = msb(result at size)**, **Z = (result == 0 at size)**, clears **V**
    /// and **C**, and **PRESERVES X** (logic never touches X — the live X is re-injected as
    /// `ccr_nz | (regs.sr & CCR_X)`). The size-masked result is written back (low8/low16/full32 for a `Dn` dest,
    /// or parked in [`Dest::Scratch`] for a memory dest the trailing `Write` stores). OR is commutative, so the
    /// `<ea>,Dn` (`a = Dn`) and `Dn,<ea>` (`a = memory`) directions reuse the same op. Distinct from
    /// [`AluOp::Add`] (which recomputes X and sets a real V/C) and [`AluOp::And`] (which masks rather than sets).
    Or,
    /// Eor: bitwise `result = a ^ b` at the operand-size boundary — the flag op of the `EOR` family. Identical to
    /// [`AluOp::And`]/[`AluOp::Or`] in every respect except the bit operation (`^` instead of `&`/`|`): shares the
    /// **MOVE flag shape** ([`move_flags`]) — sets **N = msb(result at size)**, **Z = (result == 0 at size)**,
    /// clears **V** and **C**, and **PRESERVES X** (logic never touches X — the live X is re-injected as
    /// `ccr_nz | (regs.sr & CCR_X)`). The size-masked result is written back (low8/low16/full32 for a `Dn` dest —
    /// the `EOR Dn,Dn` register form — or parked in [`Dest::Scratch`] for a memory dest the trailing `Write`
    /// stores). EOR exists only in the `Dn,<ea>` direction (`a = the EA = Dn` or memory, `b = the source Dn`);
    /// it is commutative so the operand order is inert. Distinct from [`AluOp::Add`] (which recomputes X and sets
    /// a real V/C) and [`AluOp::And`]/[`AluOp::Or`] (the same flag shape, only the bit op differs).
    Eor,
    /// Neg: **unary** `result = (0 − a) & mask` at the operand-size boundary — the flag op of the `NEG` family
    /// (`NEG <ea>` = `dst = 0 − dst`). It is byte-identical to [`AluOp::Sub`] with `a = 0, b = the operand` (NEG
    /// is literally `0 − d`), so the exec arm delegates to the same `sub_{b,w,l}` helpers with `lhs = 0` and
    /// `rhs = a` — `b` is **ignored** (the recipe passes [`Operand::Zero`]). Full SUBTRACT flags: **N = msb**,
    /// **Z = (result == 0)**, **V = (a == sign-min)** (the 0-minus-itself overflow, set only when `a` is the most
    /// negative value), **C = X = (a != 0)** (the borrow of `0 − a`). The size-masked result is written back
    /// (low8/low16/full32 for a `Dn` dest, or parked in [`Dest::Scratch`] for a memory dest the trailing `Write`
    /// stores — the read-then-write RMW). Distinct from [`AluOp::Sub`] (a binary `a − b` from a real second
    /// operand) and the logic ops (which preserve X); NEG recomputes X exactly as subtraction does.
    Neg,
    /// Negx: **unary** `result = (0 − a − X_in) & mask` at the operand-size boundary — the flag op of the `NEGX`
    /// family (`NEGX <ea>` = `dst = 0 − dst − X`). This op is **dedicated** (NOT a `Sub`/`Cmp` delegation): it is
    /// the one op with **STICKY Z** and an **incoming X** that participates in BOTH the value and the borrow.
    /// `X_in = (regs.sr >> 4) & 1`, `Z_in = (regs.sr >> 2) & 1`. Flags: **N = msb(result)**; **Z is STICKY —
    /// `Z_final = Z_in AND (result == 0)`** (NEGX never SETS Z, only CLEARS it — the multi-precision idiom: a
    /// non-zero limb clears Z, a zero limb leaves the running Z untouched, so a plain `result == 0` is WRONG on
    /// the `result == 0 && Z_in == 0` case); **V = `(a & result & signbit) != 0`**; **C = X = NOT(a == 0 AND
    /// X_in == 0)** (the borrow of `0 − a − X_in` — set unless both `a` and `X_in` are zero). The size-masked
    /// result is written back (low8/low16/full32 for a `Dn` dest, or parked in [`Dest::Scratch`] for a memory
    /// dest the trailing `Write` stores — the read-then-write RMW). `b` is **ignored** (the recipe passes
    /// [`Operand::Zero`]). Distinct from [`AluOp::Neg`] (no X-in, plain `Z = result == 0`) and [`AluOp::Sub`]
    /// (binary, no sticky Z).
    Negx,
    /// Addx: **binary** `result = (a + b + X_in) & mask` at the operand-size boundary — the flag op of the
    /// `ADDX` family (`ADDX Dy,Dx` / `ADDX -(Ay),-(Ax)`, the EXTENDED/multi-precision add). Like [`AluOp::Negx`]
    /// this op is **dedicated** (NOT an `Add` delegation): the incoming X (`X_in = (regs.sr >> 4) & 1`)
    /// participates in BOTH the value and the carry, and **Z is STICKY** (`Z_final = Z_in AND (result == 0)` —
    /// never SET, only cleared; a plain `result == 0` is WRONG when `Z_in == 0 && result == 0`). `a` is the
    /// destination operand (`Dx` / dst `-(Ax)`), `b` the source (`Dy` / src `-(Ay)`). Flags: **C = X =
    /// (raw > mask)** (the carry-out of the extended sum); **V = msb(~(a ^ b) & (a ^ result))** (standard binary
    /// add overflow — both operands same sign, result the other); **N = msb(result)**. `raw = (a + b + X_in)`
    /// computed wide; `result = raw & mask` written back (low8/low16/full32 for a `Dn` dest, or parked in
    /// [`Dest::Scratch`] for the `-(Ax)` memory dest the trailing `Write` stores). Distinct from [`AluOp::Add`]
    /// (no X-in, plain `Z = result == 0`) and [`AluOp::Negx`] (unary `0 − a − X`).
    Addx,
    /// Subx: **binary** `result = (a − b − X_in) & mask` at the operand-size boundary — the flag op of the
    /// `SUBX` family (`SUBX Dy,Dx` / `SUBX -(Ay),-(Ax)`, the EXTENDED/multi-precision subtract). Like
    /// [`AluOp::Addx`]/[`AluOp::Negx`] this op is **dedicated** (NOT a `Sub` delegation): the incoming X
    /// (`X_in = (regs.sr >> 4) & 1`) participates in BOTH the value and the borrow, and **Z is STICKY**
    /// (`Z_final = Z_in AND (result == 0)` — never SET, only cleared; a plain `result == 0` is WRONG when
    /// `Z_in == 0 && result == 0`). `a` is the destination operand (`Dx` / dst `-(Ax)`), `b` the source
    /// (`Dy` / src `-(Ay)`). Flags: **C = X = (raw < 0)** (the borrow-out of the extended difference);
    /// **V = msb((a ^ b) & (a ^ result))** (standard binary subtract overflow — operands opposite sign,
    /// result differs from the minuend); **N = msb(result)**. `raw = (a − b − X_in)` computed as a signed
    /// wide value; `result = raw & mask` written back (low8/low16/full32 for a `Dn` dest, or parked in
    /// [`Dest::Scratch`] for the `-(Ax)` memory dest the trailing `Write` stores). Distinct from
    /// [`AluOp::Sub`] (no X-in, plain `Z = result == 0`) and [`AluOp::Addx`] (extended add).
    Subx,
    /// Abcd: **binary BCD** `result = dst +₁₀ src + X_in` (byte-only) — the flag op of the `ABCD` family
    /// (`ABCD Dy,Dx` / `ABCD -(Ay),-(Ax)`, the packed-decimal add). Like [`AluOp::Addx`]/[`AluOp::Negx`] this
    /// op is **dedicated** (NOT an `Add` delegation): the incoming X (`X_in = (regs.sr >> 4) & 1`) participates
    /// in BOTH the value and the carry, and **Z is STICKY** (`Z_final = Z_in AND (result == 0)` — never SET,
    /// only cleared). `a` is the destination (`Dx` / dst `-(Ax)`), `b` the source (`Dy` / src `-(Ay)`). The
    /// decimal correction (0-mismatch-verified against the vendored `ABCD` stream): `binary = dst + src + X_in`;
    /// `lowc = 6 if (dst&0xf) + (src&0xf) + X_in > 9 else 0`; **C = X = (binary > 0x99)** (the high carry —
    /// **without** `lowc` folded in); `res = (binary + lowc + (0x60 if C else 0)) & 0xff`; **N = msb(res)**;
    /// **V = msb(res & ~binary)** (the undefined-but-deterministic overflow the 68000 actually produces).
    /// Distinct from [`AluOp::Add`]/[`AluOp::Addx`] (binary, non-decimal) and [`AluOp::Sbcd`] (the decimal
    /// SUBTRACT with its carry/result asymmetry).
    Abcd,
    /// Sbcd: **binary BCD** `result = dst −₁₀ src − X_in` (byte-only) — the flag op of the `SBCD` family
    /// (`SBCD Dy,Dx` / `SBCD -(Ay),-(Ax)`, the packed-decimal subtract). Like [`AluOp::Abcd`] this op is
    /// **dedicated** (X into value and borrow, sticky Z). It carries a **REAL carry/result ASYMMETRY**
    /// (load-bearing — 28 divergent cases): `binary = dst − src − X_in` (signed); `lowc = 6 if
    /// (dst&0xf) − (src&0xf) − X_in < 0 else 0`; **C = X = ((binary − lowc) < 0)** — the borrow keys on
    /// `binary − lowc`; **`highc = 0x60 if binary < 0 else 0`** — the RESULT's high correction keys on `binary`
    /// **(NOT `binary − lowc`)**; `res = (binary − lowc − highc) & 0xff`; **N = msb(res)**; **V = msb(~res &
    /// binary)**. The two conditions differ for small-positive `binary` with a strongly-negative low nibble
    /// (C=1 but no 0x60 result correction) — a single shared condition is WRONG; they MUST be computed
    /// separately. 0-mismatch-verified against the vendored `SBCD` stream. Distinct from [`AluOp::Sub`]/
    /// [`AluOp::Subx`] (binary, non-decimal) and [`AluOp::Abcd`] (the decimal ADD).
    Sbcd,
    /// Nbcd: **binary BCD** `result = 0 −₁₀ operand − X_in` (byte-only) — the flag op of the `NBCD` family
    /// (`NBCD <ea>`, the packed-decimal *negate* over the single data-alterable EA). It is EXACTLY the
    /// [`AluOp::Sbcd`] core with `dst = 0` and `src = operand`: the recipe reads the EA into `a`/lhs (so
    /// `src = lhs & 0xFF`, `dst = 0`, `b`/rhs ignored), and the value + N/V/C/X flags are computed by the
    /// shared `sbcd_core` (the SAME carry/result asymmetry: **C = X = ((binary − lowc) < 0)**, `highc = 0x60
    /// if binary < 0`, **V = msb(~res & binary)**, **N = msb(res)**). Like SBCD it is DEDICATED (X into the
    /// value AND the borrow) with a **STICKY Z** (`Z_in AND res==0`, never set). 0-mismatch-verified against
    /// the vendored `NBCD` stream. Distinct from [`AluOp::Neg`]/[`AluOp::Negx`] (binary, non-decimal negate).
    Nbcd,
    /// Not: **unary** `result = (~a) & mask` at the operand-size boundary — the flag op of the `NOT` family
    /// (`NOT <ea>` = `dst = ~dst`). It is **logic-shaped**, identical to [`AluOp::Eor`] in every respect except
    /// the bit operation (`~a` instead of `a ^ b`): it shares the **MOVE flag shape** ([`move_flags`]) — sets
    /// **N = msb(result at size)**, **Z = (result == 0 at size)**, clears **V** and **C**, and **PRESERVES X**
    /// (logic never touches X — the live X is re-injected as `ccr_nz | (regs.sr & CCR_X)`, never computed). The
    /// size-masked result is written back (low8/low16/full32 for a `Dn` dest, or parked in [`Dest::Scratch`] for
    /// a memory dest the trailing `Write` stores — the read-then-write RMW). `b` is **ignored** (the recipe
    /// passes [`Operand::Zero`]). Distinct from [`AluOp::Neg`]/[`AluOp::Negx`] (which RECOMPUTE X as a borrow)
    /// and [`AluOp::Eor`] (binary `a ^ b` from a real second operand — NOT is the unary complement of `a`).
    Not,
    /// Ext: **unary** `Dn`-only sign-extend whose width follows `size` — `EXT.w` (`size == Word`) sign-extends
    /// the low **byte** of `a` to 16 bits (`res = sign_extend8→16(a & 0xFF)`) and writes the **low word** (the
    /// high word of `Dn` is preserved — the recipe pairs it with [`Dest::DataRegLow16`]); `EXT.l`
    /// (`size == Long`) sign-extends the low **word** to 32 bits (`res = sign_extend16→32(a & 0xFFFF)`) and
    /// writes the **full 32** ([`Dest::DataReg`]). It is **logic-shaped** (the same MOVE flag shape as
    /// [`AluOp::Not`]): **N = msb of the result at `size`** (bit15 for `.w`, bit31 for `.l`), **Z = (result == 0
    /// at `size`)**, clears **V** and **C**, and **PRESERVES X** (re-injected `ccr_nz | (regs.sr & CCR_X)`,
    /// never computed). `b` is **ignored** (the recipe passes [`Operand::Zero`]); the recipe supplies `a` at the
    /// **input** size (the low byte for `.w`, the low word for `.l`). Distinct from [`AluOp::MoveA`] (which also
    /// word-sign-extends but writes An and sets no flags) and [`AluOp::Swap`] (a halfword swap, not a
    /// sign-extend). `Dn`-only — never a memory op.
    Ext,
    /// Swap: **unary** `Dn`-only 16-bit halfword swap — `res = (a >> 16) | (a << 16)` on the **full 32 bits**
    /// (`size` is always `Long`). It is **logic-shaped** (the same MOVE flag shape as [`AluOp::Not`]/[`AluOp::Ext`]):
    /// **N = bit31 of the swapped result**, **Z = (result == 0)**, clears **V** and **C**, and **PRESERVES X**
    /// (re-injected `ccr_nz | (regs.sr & CCR_X)`, never computed). `b` is **ignored** (the recipe passes
    /// [`Operand::Zero`]); `a` is the full register ([`Operand::DataRegFull`]) and the result is written full-width
    /// ([`Dest::DataReg`]). `Dn`-only — never a memory op. Distinct from [`AluOp::Ext`] (a sign-extend) and
    /// [`AluOp::Not`] (a bitwise complement).
    Swap,
    /// Tas: **unary** test-and-set's flag+value compute for the `TAS Dn` register form — the flags come from
    /// the byte READ (the INPUT `a`), the written value is `(a & 0xFF) | 0x80` (bit 7 ALWAYS set). It is
    /// **logic-shaped** ([`move_flags`] over the input byte): **N = bit7(`a` & 0xFF)**, **Z = (`a` & 0xFF ==
    /// 0)**, clears **V** and **C**, and **PRESERVES X** (re-injected `ccr_nz | (regs.sr & CCR_X)`, never
    /// computed). The KEY subtlety vs [`AluOp::Not`]: the flag INPUT (`a`) DIFFERS from the WRITE value
    /// (`a | 0x80`) — NOT computes its flags on the result `~a`, whereas TAS computes on the unmodified input
    /// then writes `a | 0x80`. `b` is **ignored** (the recipe passes [`Operand::Zero`]); `a` is the low byte
    /// ([`Operand::DataRegLow8`]) and the result is written to the low byte ([`Dest::DataRegLow8`], the upper
    /// 24 bits preserved). `Dn`-only this form (memory TAS is the atomic RMW micro-op, not this Alu op). Always
    /// `Size::Byte`.
    Tas,
    /// Btst: the **bit test** of `BTST` — test the single bit of operand `a` selected by bit number `b`,
    /// setting **only Z** (`Z = NOT(the tested bit)`). The bit width follows `size`: **`Size::Long` for a `Dn`
    /// operand** (32 bits, `pos = b mod 32`), **`Size::Byte` for a memory / `#imm` / PC-relative operand** (8
    /// bits, `pos = b mod 8`). The whole flag formula is `pos = b mod bits`; `bit = (a >> pos) & 1`;
    /// `ccr = (regs.sr & (X|N|V|C)) | (Z if bit == 0)` — **X, N, V, C are ALL PRESERVED** (only Z is touched;
    /// the SR system byte is preserved by the shared write-back mask). BTST writes **no value** (paired with
    /// [`Dest::None`]) — it is read-only (`BCHG`/`BCLR`/`BSET` add a write in later commits). `a` is the operand
    /// (the value tested), `b` is the bit number ([`Operand::DataRegFull`] for the dynamic form / a scratch slot
    /// holding the captured `prefetch[1]` ext word for the static form). Distinct from the logic ops (which set
    /// N/Z and clear V/C) and the compare ops (which set N/Z/V/C): BTST touches Z and Z ALONE.
    Btst,
    /// Bchg: the **bit test-and-toggle** of `BCHG` — `Btst` (`Z = NOT(the PRE-modify bit)`, X/N/V/C + the SR
    /// system byte all PRESERVED) PLUS the write of `a ^ (1 << pos)` (toggle the tested bit). The Z flag is from
    /// the bit BEFORE the toggle (the read value), NOT after. The bit width follows `size` exactly as `Btst`:
    /// **`Size::Long` for a `Dn` dest** (32 bits, `pos = b mod 32`, the FULL 32-bit register written with one
    /// bit flipped) / **`Size::Byte` for a memory dest** (8 bits, `pos = b mod 8`, the byte written with one
    /// bit flipped). `a` is the operand, `b` the bit number ([`Operand::DataRegFull`] dynamic / a scratch slot
    /// holding the captured `prefetch[1]` static); the recipe pairs it with [`Dest::DataReg`] (`Dn`) or
    /// [`Dest::Scratch`] (memory, the `ea_dst` write source).
    Bchg,
    /// Bclr: the **bit test-and-clear** of `BCLR` — `Btst` (`Z = NOT(the PRE-clear bit)`, X/N/V/C + the SR
    /// system byte all PRESERVED) PLUS the write of `a & !(1 << pos)` (clear the tested bit). The Z flag is from
    /// the bit BEFORE the clear (the read value), NOT after — identical Z shape to `Bchg`, only the written
    /// value differs (clear vs toggle). The bit width follows `size` exactly as `Btst`/`Bchg`: **`Size::Long`
    /// for a `Dn` dest** (32 bits, `pos = b mod 32`, the FULL 32-bit register written with one bit cleared) /
    /// **`Size::Byte` for a memory dest** (8 bits, `pos = b mod 8`, the byte written with one bit cleared). `a`
    /// is the operand, `b` the bit number ([`Operand::DataRegFull`] dynamic / a scratch slot holding the
    /// captured `prefetch[1]` static); the recipe pairs it with [`Dest::DataReg`] (`Dn`) or [`Dest::Scratch`]
    /// (memory, the `ea_dst` write source). BCLR's REGISTER form is 2 cycles SLOWER than BCHG/BSET (base idle
    /// `n4`, not `n2`) — carried by `bit_recipe`'s `reg_base` parameter.
    Bclr,
    /// Bset: the **bit test-and-set** of `BSET` — `Btst` (`Z = NOT(the PRE-set bit)`, X/N/V/C + the SR system
    /// byte all PRESERVED) PLUS the write of `a | (1 << pos)` (set the tested bit). The Z flag is from the bit
    /// BEFORE the set (the read value), NOT after — identical Z shape to `Bchg`/`Bclr`, only the written value
    /// differs (set vs toggle/clear). The bit width follows `size` exactly as `Btst`/`Bchg`/`Bclr`:
    /// **`Size::Long` for a `Dn` dest** (32 bits, `pos = b mod 32`, the FULL 32-bit register written with one
    /// bit set) / **`Size::Byte` for a memory dest** (8 bits, `pos = b mod 8`, the byte written with one bit
    /// set). `a` is the operand, `b` the bit number ([`Operand::DataRegFull`] dynamic / a scratch slot holding
    /// the captured `prefetch[1]` static); the recipe pairs it with [`Dest::DataReg`] (`Dn`) or [`Dest::Scratch`]
    /// (memory, the `ea_dst` write source). BSET's REGISTER form uses the SAME base idle as BCHG (`n2`, NOT
    /// BCLR's `n4`) — carried by `bit_recipe`'s `reg_base` parameter.
    Bset,
    /// Asl: **arithmetic shift LEFT** by `cnt = b & 63` — the foundational op of the shift/rotate family and
    /// the ONLY one that owns **V** (the sign bit changed at ANY point during the shift). `a` is the operand
    /// (size-masked to `x`), `b` the count source ([`Operand::ShiftCount`] for the immediate/memory forms /
    /// [`Operand::DataRegFull`] for the dynamic `Dn`-count form — the exec masks `& 63`); `size` → `n =
    /// 8/16/32`, `mask = (1<<n)-1`, `signbit = 1<<(n-1)`. Value: `res = (x << cnt) & mask` when `cnt < n`,
    /// else `0`. **C** = the last bit shifted out of the operand — `bit(n-cnt)` when `1 <= cnt <= n`, else `0`;
    /// **X = C**. **V** (closed form, 0-mismatch verified vs the vendored stream): for `cnt >= n`, `V = (x !=
    /// 0)` (`x == mask` shifts a 0 into the sign, so the sign DOES change → V=1, NOT `x != 0 && x != mask`);
    /// for `cnt < n`, `V` = (the top `cnt+1` bits of the n-bit field are not all-equal). **N** = msb(res),
    /// **Z** = (res == 0). **ZERO COUNT** (`cnt == 0`, possible only via the dynamic `Dn` form): the value is
    /// unchanged, **V = 0, C = 0, X PRESERVED** (re-injected — the shift never ran), N/Z from the unchanged
    /// operand. The size-masked result is written back (low8/low16/full32 for a `Dn` dest via the recipe's
    /// `dn_dest`, or parked in [`Dest::Scratch`] for the word memory shift-by-1 the trailing `Write` stores).
    /// Every Rust shift is guarded (the `cnt >= n` branch; the V top-mask shifts by `n-1-cnt ∈ 0..n-1`).
    Asl,
    /// Asr: **arithmetic shift RIGHT** by `cnt = b & 63` — the sign-EXTENDING right shift (`Dn`'s msb is
    /// replicated into the vacated top bits). Reuses the shared [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*`
    /// machinery VERBATIM (only the `AluOp` + the AS/right decode arm differ from ASL). `a` is the operand
    /// (size-masked to `x`), `b` the count source ([`Operand::ShiftCount`] for the immediate/memory forms /
    /// [`Operand::DataRegFull`] for the dynamic `Dn`-count form — the exec masks `& 63`); `size` → `n =
    /// 8/16/32`, `mask = (1<<n)-1`, `signbit = 1<<(n-1)`. Value (sign-extending): for `cnt >= n` the result is
    /// all-sign-bits (`mask` if the operand is negative, else `0`); for `0 < cnt < n` it is `(x >> cnt)` with
    /// the top `cnt` bits filled by the sign (`(mask << (n-cnt)) & mask` when negative). **C** = the last bit
    /// shifted out of the OPERAND — `bit(cnt-1)` when `1 <= cnt <= n`, else **0** (THE ASR CARRY QUIRK: for
    /// `cnt > n`, C = 0, **NOT** the sign bit — even though the *value* still sign-extends to all-sign-bits; a
    /// naive "last bit out = sign for over-shift" mismatches 1642 ASR.b cases); **X = C**. **V = 0** always
    /// (ASR never sets V — only ASL owns V). **N** = msb(res), **Z** = (res == 0). **ZERO COUNT** (`cnt == 0`,
    /// possible only via the dynamic `Dn` form): the value is unchanged, **V = 0, C = 0, X PRESERVED** (the
    /// shift never ran — re-inject the live X), N/Z from the unchanged operand. The size-masked result is
    /// written back (low8/low16/full32 for a `Dn` dest via `dn_dest`, or [`Dest::Scratch`] for the word memory
    /// shift-by-1). Every Rust shift is guarded (the `cnt == 0` and `cnt >= n` branches keep `n - cnt ∈
    /// 1..n-1` and `cnt - 1 ∈ 0..n-1`, never `>= 32`).
    Asr,
    /// Lsl: **logical shift LEFT** by `cnt = b & 63` — IDENTICAL to [`AluOp::Asl`]'s value and carry, with the
    /// SOLE difference that **V is FORCED to 0** (a logical shift never tracks the sign change — only ASL owns
    /// V). Reuses the shared [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*` machinery VERBATIM (only the
    /// `AluOp` + the LS/left decode arm differ from ASL). `a` is the operand (size-masked to `x`), `b` the
    /// count source ([`Operand::ShiftCount`] for the immediate/memory forms / [`Operand::DataRegFull`] for the
    /// dynamic `Dn`-count form — the exec masks `& 63`); `size` → `n = 8/16/32`, `mask = (1<<n)-1`, `signbit =
    /// 1<<(n-1)`. Value: `res = (x << cnt) & mask` when `cnt < n`, else `0` (an over-shift clears the register).
    /// **C** = the last bit shifted out of the operand — `bit(n-cnt)` when `1 <= cnt <= n`, else `0`; **X = C**.
    /// **V = 0** ALWAYS (the only difference from ASL — LSL does NOT compute the sign-changed V). **N** =
    /// msb(res), **Z** = (res == 0). **ZERO COUNT** (`cnt == 0`, possible only via the dynamic `Dn` form): the
    /// value is unchanged, **V = 0, C = 0, X PRESERVED** (re-injected — the shift never ran), N/Z from the
    /// unchanged operand. The size-masked result is written back (low8/low16/full32 for a `Dn` dest via
    /// `dn_dest`, or [`Dest::Scratch`] for the word memory shift-by-1). Every Rust shift is guarded (the
    /// `cnt == 0` and `cnt >= n` branches keep `n - cnt ∈ 1..n-1`, never `>= 32`).
    Lsl,
    /// Lsr: **logical shift RIGHT** by `cnt = b & 63` — the **zero-fill** right shift (contrast [`AluOp::Asr`],
    /// which sign-EXTENDS). Reuses the shared [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*` machinery VERBATIM
    /// (only the `AluOp` + the LS/right decode arm differ). `a` is the operand (size-masked to `x`), `b` the
    /// count source ([`Operand::ShiftCount`] for the immediate/memory forms / [`Operand::DataRegFull`] for the
    /// dynamic `Dn`-count form — the exec masks `& 63`); `size` → `n = 8/16/32`, `mask = (1<<n)-1`, `signbit =
    /// 1<<(n-1)`. Value: `res = x >> cnt` when `cnt < n`, else `0` (zero-fill — vacated top bits are 0, never
    /// the sign). **C** = the last bit shifted out of the operand — `bit(cnt-1)` when `1 <= cnt <= n`, else `0`
    /// (the same form as ASR's carry; for LSR there is no sign so `cnt > n` → 0 is natural); **X = C**. **V =
    /// 0** always. **N** = msb(res) — always 0 for any `cnt >= 1` (the msb is zero-filled). **Z** = (res == 0).
    /// **ZERO COUNT** (`cnt == 0`, possible only via the dynamic `Dn` form): the value is unchanged, **V = 0,
    /// C = 0, X PRESERVED** (re-injected — the shift never ran), N/Z from the unchanged operand (so N CAN be 1
    /// here — it is NOT forced to 0). The size-masked result is written back (low8/low16/full32 for a `Dn` dest
    /// via `dn_dest`, or [`Dest::Scratch`] for the word memory shift-by-1). Every Rust shift is guarded (the
    /// `cnt == 0` and `cnt >= n` branches keep `cnt - 1 ∈ 0..n-1`, never `>= 32`).
    Lsr,
    /// Rol: **rotate LEFT** by `cnt = b & 63` — a plain bit-rotate that does NOT pass through X (contrast
    /// [`AluOp::Roxl`], which threads X through an `(n+1)`-bit rotate). Reuses the shared
    /// [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*` machinery VERBATIM (only the `AluOp` + the RO/left
    /// decode arm differ). `a` is the operand (size-masked to `x`), `b` the count source
    /// ([`Operand::ShiftCount`] for the immediate/memory forms / [`Operand::DataRegFull`] for the dynamic
    /// `Dn`-count form — the exec masks `& 63`); `size` → `n = 8/16/32`, `mask = (1<<n)-1`, `signbit =
    /// 1<<(n-1)`. Value: `r = cnt % n`; `res = x` when `cnt == 0 || r == 0` (a whole-register rotation leaves
    /// the value unchanged), else `((x << r) | (x >> (n - r))) & mask`. **C** = the last bit rotated out —
    /// `(x >> ((n - (cnt % n)) % n)) & 1` for `cnt != 0`, else `0` (a zero count is the ONLY way ROL clears C).
    /// **X is PRESERVED** (ROL/ROR never touch X — re-inject the live X, NEVER set X = C). **V = 0** always.
    /// **N** = msb(res), **Z** = (res == 0). **ZERO COUNT** (`cnt == 0`, possible only via the dynamic `Dn`
    /// form): the value is unchanged, **V = 0, C = 0, X PRESERVED**, N/Z from the unchanged operand. A NONZERO
    /// multiple of `n` (`r == 0`, e.g. ROL.b #8): the value is unchanged but C still comes from the formula
    /// (= the operand's low-bit region), NOT 0. The size-masked result is written back (low8/low16/full32 for a
    /// `Dn` dest via `dn_dest`, or [`Dest::Scratch`] for the word memory rotate-by-1). Every Rust shift is
    /// guarded (the `x >> (n - r)` term runs only for `r != 0`, keeping `n - r ∈ 1..n-1`, never `>= 32`).
    Rol,
    /// Ror: **rotate RIGHT** by `cnt = b & 63` — ROL's right-direction twin, a plain bit-rotate that does NOT
    /// pass through X (contrast [`AluOp::Roxr`], which threads X through an `(n+1)`-bit rotate — S7). Reuses the
    /// shared [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*` machinery VERBATIM (only the `AluOp` + the RO/right
    /// decode arm differ). `a` is the operand (size-masked to `x`), `b` the count source
    /// ([`Operand::ShiftCount`] for the immediate/memory forms / [`Operand::DataRegFull`] for the dynamic
    /// `Dn`-count form — the exec masks `& 63`); `size` → `n = 8/16/32`, `mask = (1<<n)-1`, `signbit =
    /// 1<<(n-1)`. Value: `r = cnt % n`; `res = x` when `cnt == 0 || r == 0` (a whole-register rotation leaves
    /// the value unchanged), else `((x >> r) | (x << (n - r))) & mask`. **C** = the last bit rotated out —
    /// `(x >> ((cnt - 1) % n)) & 1` for `cnt != 0`, else `0` (a zero count is the ONLY way ROR clears C).
    /// **X is PRESERVED** (ROL/ROR never touch X — re-inject the live X, NEVER set X = C). **V = 0** always.
    /// **N** = msb(res), **Z** = (res == 0). **ZERO COUNT** (`cnt == 0`, possible only via the dynamic `Dn`
    /// form): the value is unchanged, **V = 0, C = 0, X PRESERVED**, N/Z from the unchanged operand. A NONZERO
    /// multiple of `n` (`r == 0`, e.g. ROR.b #8): the value is unchanged but C still comes from the formula
    /// (= the operand's high-bit region), NOT 0. The size-masked result is written back (low8/low16/full32 for a
    /// `Dn` dest via `dn_dest`, or [`Dest::Scratch`] for the word memory rotate-by-1). Every Rust shift is
    /// guarded (the `x << (n - r)` term runs only for `r != 0`, keeping `n - r ∈ 1..n-1`, never `>= 32`).
    Ror,
    /// Roxl: **rotate LEFT THROUGH X** by `cnt = b & 63` — the FIRST X-threading rotate. Unlike [`AluOp::Rol`]/
    /// [`AluOp::Ror`] (which leave X untouched) and [`AluOp::Asl`]/[`AluOp::Asr`]/[`AluOp::Lsl`]/[`AluOp::Lsr`]
    /// (which set X = C from the value), ROXL treats the `X:operand` pair as an `(n+1)`-bit register — X sits
    /// ABOVE the msb — and rotates it LEFT by `cnt % (n+1)`; the final bit ejected into X is BOTH the new X and C,
    /// so the result DEPENDS ON THE INCOMING X. Reuses the shared [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*`
    /// machinery VERBATIM (only the `AluOp` + the ROX/left decode arm differ). `a` is the operand (size-masked to
    /// `x`), `b` the count source ([`Operand::ShiftCount`] for the immediate/memory forms /
    /// [`Operand::DataRegFull`] for the dynamic `Dn`-count form — the exec masks `& 63`); `size` → `n = 8/16/32`,
    /// `mask = (1<<n)-1`, `signbit = 1<<(n-1)`, `xin = (sr >> 4) & 1`. Value: `per = n + 1`, `eff = cnt % per`;
    /// `comb = ((xin << n) | x)` in `per` bits (a `u64` so the `.l` 33-bit case does not overflow `u32`), rotated
    /// LEFT by `eff` (`comb = ((comb << eff) | (comb >> (per - eff))) & ((1<<per)-1)` for `eff != 0`, else
    /// unchanged — guard `per - eff` when `eff == 0`), `res = (comb & mask) as u32`. **C = X = (comb >> n) & 1**
    /// (the bit ejected into X). **V = 0** always. **N** = msb(res), **Z** = (res == 0). **ZERO COUNT** (`cnt ==
    /// 0`, possible only via the dynamic `Dn` form): the value is UNCHANGED, **C = X (the INCOMING X — NOT 0), X
    /// UNCHANGED**, V = 0, N/Z from the unchanged operand. A cnt that WRAPS the `(n+1)` PERIOD (`eff == 0` with
    /// `cnt != 0`, e.g. ROXL.b #9) returns the value to its start. The size-masked result is written back
    /// (low8/low16/full32 for a `Dn` dest via `dn_dest`, or [`Dest::Scratch`] for the word memory rotate-by-1).
    Roxl,
    /// Roxr: **rotate RIGHT THROUGH X** by `cnt = b & 63` — ROXL's right-direction twin (S7, the FINAL shift/
    /// rotate op). Unlike [`AluOp::Rol`]/[`AluOp::Ror`] (which leave X untouched) and [`AluOp::Asl`]/
    /// [`AluOp::Asr`]/[`AluOp::Lsl`]/[`AluOp::Lsr`] (which set X = C from the value), ROXR treats the `X:operand`
    /// pair as an `(n+1)`-bit register — X sits ABOVE the msb — and rotates it RIGHT by `cnt % (n+1)`; the final
    /// bit ejected into X is BOTH the new X and C, so the result DEPENDS ON THE INCOMING X. Reuses the shared
    /// [`shift_recipe`]/[`Operand::ShiftCount`]/`dn_*` machinery VERBATIM (only the `AluOp` + the ROX/right decode
    /// arm differ). `a` is the operand (size-masked to `x`), `b` the count source ([`Operand::ShiftCount`] for the
    /// immediate/memory forms / [`Operand::DataRegFull`] for the dynamic `Dn`-count form — the exec masks `& 63`);
    /// `size` → `n = 8/16/32`, `mask = (1<<n)-1`, `signbit = 1<<(n-1)`, `xin = (sr >> 4) & 1`. Value: `per = n +
    /// 1`, `eff = cnt % per`; `comb = ((xin << n) | x)` in `per` bits (a `u64` so the `.l` 33-bit case does not
    /// overflow `u32`), rotated RIGHT by `eff` (`comb = ((comb >> eff) | (comb << (per - eff))) & ((1<<per)-1)` for
    /// `eff != 0`, else unchanged — guard `per - eff` when `eff == 0`), `res = (comb & mask) as u32`. **C = X =
    /// (comb >> n) & 1** (the bit ejected into X — the new value at the X position). **V = 0** always. **N** =
    /// msb(res), **Z** = (res == 0). **ZERO COUNT** (`cnt == 0`, possible only via the dynamic `Dn` form): the
    /// value is UNCHANGED, **C = X (the INCOMING X — NOT 0), X UNCHANGED**, V = 0, N/Z from the unchanged operand.
    /// A cnt that WRAPS the `(n+1)` PERIOD (`eff == 0` with `cnt != 0`, e.g. ROXR.b #9) returns the value to its
    /// start. The size-masked result is written back (low8/low16/full32 for a `Dn` dest via `dn_dest`, or
    /// [`Dest::Scratch`] for the word memory rotate-by-1).
    Roxr,
    /// Mulu: **unsigned** 16×16→32 multiply — `Dn = (a & 0xFFFF) * (b & 0xFFFF)` written FULL-32 to `Dn`
    /// ([`Dest::DataReg`]), where `a` = the low word of `Dn` (the multiplicand, [`Operand::DataRegLow16`]) and
    /// `b` = the source word (the multiplier, resolved by `ea_src(Size::Word)` — a register / scratch / the
    /// immediate). Flags are the MOVE/logic shape on the FULL 32-bit product: **N = bit31(product)**, **Z =
    /// (product == 0)**, clears **V** and **C**, **PRESERVES X** (re-injected `ccr_nz | (regs.sr & CCR_X)` —
    /// only N/Z/V/C change). THE TIMING IS DATA-DEPENDENT ON THE SOURCE: unlike every other Alu op (which
    /// returns 0 cycles and lets the recipe's idle ops own the cost), the Mulu exec arm RETURNS its own step
    /// cycle cost = **`34 + 2 * popcount(b & 0xFFFF)`** (the count comes from the resolved multiplier `b`, not
    /// knowable at decode for a memory source). The full instruction length is `38 + 2*popcount + ea_cost`,
    /// emerging as the operand read + the prefetch refill + this count-dependent idle. An early-return op (like
    /// MoveA/Adda/Suba) — it does its own SR + Dn write and bumps `self.step`, never reaching the shared
    /// write-back. Distinct from [`AluOp::Suba`] (an An-write no-flag op) and [`AluOp::And`] (a 0-cycle
    /// fixed-flag logic op).
    Mulu,
    /// Muls: **signed** 16×16→32 multiply — `Dn = sx16(a) * sx16(b)` written FULL-32 to `Dn`
    /// ([`Dest::DataReg`]), where `a` = the low word of `Dn` (the multiplicand, [`Operand::DataRegLow16`]) and
    /// `b` = the source word (the multiplier, resolved by `ea_src(Size::Word)` — a register / scratch / the
    /// immediate). BOTH operands are SIGN-EXTENDED from 16 to 32 bits (two's complement) BEFORE the multiply;
    /// the low 32 bits of the signed product is the result. Flags are the MOVE/logic shape on the FULL 32-bit
    /// product: **N = bit31(product)**, **Z = (product == 0)**, clears **V** and **C**, **PRESERVES X**
    /// (re-injected `ccr_nz | (regs.sr & CCR_X)` — only N/Z/V/C change), IDENTICAL to [`AluOp::Mulu`]. THE
    /// TIMING IS DATA-DEPENDENT ON THE SOURCE, but DIFFERS from MULU: the Muls exec arm RETURNS its own step
    /// cycle cost = **`34 + 2 * booth(b & 0xFFFF)`** where `booth` is the Booth-recoding TRANSITION count of the
    /// source — the number of `01`/`10` bit-pair transitions in `(b & 0xFFFF) << 1` (a 17-bit value with a 0
    /// appended at the LSB), i.e. `sum over i in 0..15 of (bit(i) != bit(i+1))` of `(b << 1)`. This is NOT the
    /// 1-bit popcount MULU uses (e.g. `0xFFFF` → popcount 16 but Booth 1; `0x5555` → popcount 8 but Booth 16),
    /// so MULS timing differs from MULU even for the same source. The full instruction length is `38 +
    /// 2*booth + ea_cost`, emerging as the operand read + the prefetch refill + this count-dependent idle. An
    /// early-return op (like MoveA/Adda/Suba/Mulu) — it does its own SR + Dn write and bumps `self.step`, never
    /// reaching the shared write-back.
    Muls,
    /// Divu: **unsigned** 16-bit divide — the full 32-bit `Dn` (`a` = [`Operand::DataRegFull`], the dividend) is
    /// divided by the resolved word source (`b & 0xFFFF`, the divisor). On success `q = dividend / divisor`,
    /// `r = dividend % divisor`, and `Dn = ((r & 0xFFFF) << 16) | (q & 0xFFFF)` (quotient low 16, remainder high
    /// 16, [`Dest::DataReg`]). THREE outcomes:
    /// - **div0** (`divisor == 0`): take the DIVIDE-BY-ZERO trap (vector 5, the standard 6-byte frame). The CCR
    ///   is set to **N=0, Z=0, V=0, C=0, X PRESERVED** BEFORE the frame captures the SR, then the in-flight
    ///   `MicroState` is rewritten into the vector-5 frame ([`install_div0_trap`](MicroState), mirroring CHK's
    ///   vector-6 install). `Dn` UNCHANGED; saved PC = the live `regs.pc`. Returns 0 (the frame's leading idle +
    ///   bus ops count the cycles).
    /// - **overflow** (`q > 0xFFFF`, equiv `(dividend >> 16) >= divisor`): `Dn` UNCHANGED. CCR **V=1, C=0, N/Z/X
    ///   PRESERVED** (only V set, C cleared — NOT a partial-state N/Z). Returns the flat overflow idle.
    /// - **normal**: write `Dn`; CCR **N = bit15(q), Z = (q & 0xFFFF == 0), V=0, C=0, X PRESERVED** (only N/Z
    ///   change). Returns the variable bit-serial division idle.
    ///
    /// Like MULU/MULS this is an early-return op whose TIMING IS DATA-DEPENDENT ON THE SOURCE — the exec arm
    /// RETURNS its own step cycle cost and self-books `self.cycles`. The documented mode-0 division cost is
    /// `76 + 2*n_keep + 4*n_restore` (normal) / `10` (overflow). UNLIKE MULU/MULS, the recipe places the final
    /// `Prefetch` AFTER this Alu (the `[idle, prefetch]` order the data pins, not MUL's `[prefetch, idle]`), so
    /// the Alu returns the documented cost **minus 4** (the trailing `Prefetch` books that 4); the mode-0
    /// instruction total is the documented cost, and a memory source adds its EA bus cost on top.
    Divu,
    /// Divs: **signed** 16-bit divide — the SIGNED twin of [`AluOp::Divu`]. The full 32-bit `Dn`
    /// (`a` = [`Operand::DataRegFull`]) is the dividend `sdd = sx32(Dn)` (an `i32`); the resolved word source is
    /// the divisor `sds = sx16(b)` (an `i16`). On success `q = sdd / sds` with **C-style truncation toward
    /// zero** (computed in `i64` so the `i32::MIN / -1` overflow case can be detected, not panic), `r = sdd −
    /// q·sds` (so the **remainder takes the DIVIDEND's sign**), and `Dn = ((r & 0xFFFF) << 16) | (q & 0xFFFF)`
    /// (quotient low 16, remainder high 16, [`Dest::DataReg`]). THREE outcomes, identical in shape to DIVU:
    /// - **div0** (`divisor == 0`): the SAME vector-5 divide-by-zero trap as DIVU (CCR N=Z=V=C=0/X kept set
    ///   BEFORE the frame captures the SR, then [`install_div0_trap`](MicroState)). `Dn` UNCHANGED. (DIVS has NO
    ///   div0 sample in the vendored data — implemented for correctness only, like the DBcc-expired path.)
    /// - **overflow** (`q` out of the signed-16 range — `q > 0x7FFF || q < -0x8000`, INCLUDING the
    ///   `0x8000_0000 / -1` case where `q = +0x8000_0000`): `Dn` UNCHANGED. CCR **V=1, C=0, N/Z/X PRESERVED**
    ///   (identical to DIVU's overflow rule — only V set, NOT a partial-state N/Z). Returns the flat overflow
    ///   cost `16` (`dividend ≥ 0`) / `18` (`dividend < 0`) — the `+2` is the negate-dividend prologue (NOT the
    ///   normal-loop length; there are ZERO late-overflow cases).
    /// - **normal**: write `Dn`; CCR **N = bit15(q) (`(q>>15)&1`), Z = (q & 0xFFFF == 0), V=0, C=0, X
    ///   PRESERVED** (only N/Z change). Returns the variable abs-value restoring-division cost (`110 +
    ///   2*n_restore + sign terms`, see [`divs_cycles`]).
    ///
    /// Like DIVU this is an early-return op whose TIMING IS DATA-DEPENDENT ON THE SOURCE — the exec arm RETURNS
    /// its own step cost (the documented cost minus 4, the trailing `Prefetch` booking the 4) and self-books
    /// `self.cycles`. Reuses `div_recipe` + the vector-5 div0 frame VERBATIM (only the value/flag/timing math
    /// differs from DIVU).
    Divs,
}

/// A bitwise logic operation a [`MicroOp::SrLogic`] applies to the status register — the three privileged
/// `*toSR` ops: `ANDItoSR` (`And`), `ORItoSR` (`Or`), `EORItoSR` (`Eor`). The operand is the immediate word;
/// the result is masked to the implemented SR bits (`SR_IMPLEMENTED`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum LogicOp {
    /// `ANDItoSR`: `sr &= value` — can clear bits, including **S** (switch supervisor→user).
    And,
    /// `ORItoSR`: `sr |= value` — can only set bits (never clears S).
    Or,
    /// `EORItoSR`: `sr ^= value` — toggles bits (can flip S either way).
    Eor,
}

/// MULS Booth-recoding TRANSITION count of a 16-bit source — the data-dependent cycle driver for
/// [`AluOp::Muls`]. The 68000's MULS uses Booth recoding: the multiplier is scanned in overlapping bit pairs
/// of `(src << 1)` (a 17-bit value with a 0 appended at the LSB), and each `01`/`10` transition costs 2
/// cycles. So `booth_transitions(src) = sum over i in 0..15 of (bit(i) != bit(i+1))` of `(src << 1)` —
/// equivalently the popcount of the XOR of adjacent bits over that 17-bit window. This is NOT the 1-bit
/// popcount MULU uses: e.g. `0xFFFF` → popcount 16 but Booth 1 (one solid run, `(0x1FFFE)` has a single
/// 0→1 then a single 1→0 outside the window, so 1 transition in bits 0..15); `0x5555` → popcount 8 but Booth
/// 16 (fully alternating). The exec arm returns `34 + 2 * booth_transitions(src)` cycles.
#[inline]
fn booth_transitions(src16: u32) -> u32 {
    // `shifted` is the 17-bit value with the LSB forced to 0. Count, over i in 0..15, the positions where bit i
    // differs from bit i+1 of `shifted` (each is one Booth 01/10 transition).
    let shifted = (src16 & 0xFFFF) << 1;
    let mut count = 0u32;
    for i in 0..16 {
        let bit_i = (shifted >> i) & 1;
        let bit_i1 = (shifted >> (i + 1)) & 1;
        if bit_i != bit_i1 {
            count += 1;
        }
    }
    count
}

/// DIVU's documented mode-0 division cost — the data-dependent cycle driver for the normal (non-overflow,
/// non-div0) [`AluOp::Divu`] path. A faithful port of the 68000's 16-step bit-serial restoring-division
/// microcode (each microword = 2 cycles), reduced to a closed form (0-mismatch verified against the vendored
/// DIVU stream): align the divisor into the high word, then shift the dividend through, counting `n_keep`
/// (a quotient-bit-1 with no mandatory subtract) and `n_restore` (a quotient-bit-0) over the **first 15**
/// iterations only — the 16th iteration's shift folds into a fixed finalization (no keep/restore microword),
/// so the `i < 15` guard is LOAD-BEARING (counting all 16 over-counts by 2–4). Returns `76 + 2*n_keep +
/// 4*n_restore` (range 88..130). NOTE the caller is responsible for `(dividend >> 16) >= divisor` overflow
/// detection (the flat-10 early-out) and `divisor != 0` BEFORE calling this; here `divisor` is always nonzero.
#[inline]
fn divu_cycles(dividend: u32, divisor: u32) -> u32 {
    let aligned = (divisor & 0xFFFF) << 16;
    let mut work = dividend;
    let (mut n_keep, mut n_restore) = (0u32, 0u32);
    for i in 0..16 {
        let msb = work >> 31;
        work <<= 1; // a u32 shift already drops the bit above 32 (the `& 0xFFFF_FFFF` is implicit)
        if msb != 0 {
            work = work.wrapping_sub(aligned); // mandatory subtract (no keep/restore microword)
        } else if work >= aligned {
            work = work.wrapping_sub(aligned);
            if i < 15 {
                n_keep += 1;
            }
        } else if i < 15 {
            n_restore += 1; // restore (quotient bit 0)
        }
    }
    76 + 2 * n_keep + 4 * n_restore
}

/// DIVS's documented mode-0 division cost — the data-dependent cycle driver for the normal (non-overflow,
/// non-div0) [`AluOp::Divs`] path. The signed twin of [`divu_cycles`]: a faithful port of the 68000's
/// abs-value restoring-division microcode (0-mismatch verified against the vendored DIVS stream). The DIVS main
/// loop is the SIMPLER restoring division (no mandatory-subtract shortcut — `n_restore` is the only varying
/// loop term): take `|dividend|`/`|divisor|`, align the divisor into the high word, shift the abs-dividend
/// through, counting `n_restore` (a quotient-bit-0) over the **first 15** iterations only (the `i < 15` guard
/// is LOAD-BEARING — the 16th iteration is the fixed finalization). The base is `110 + 2*n_restore`, plus the
/// negate-dividend prologue (`+2` if `dividend < 0`) and the 3-way sign-correction term (`+12` if
/// `divisor < 0`, else `+14` if `dividend < 0`, else `+10`). Returns the full cost (range 126..152). The
/// caller is responsible for the overflow early-out (flat 16/18) and `divisor != 0` BEFORE calling this.
#[inline]
fn divs_cycles(dividend: u32, divisor: u32) -> u32 {
    let add = (dividend as i32).unsigned_abs();
    let az = (divisor as i16).unsigned_abs() as u32;
    let aligned = az << 16;
    let mut work = add;
    let mut n_restore = 0u32;
    for i in 0..16 {
        work <<= 1; // a u32 shift already drops the bit above 32 (the `& 0xFFFF_FFFF` is implicit)
        if work >= aligned {
            work = work.wrapping_sub(aligned);
        } else if i < 15 {
            n_restore += 1; // restore (quotient bit 0)
        }
    }
    let mut base = 110 + 2 * n_restore;
    base += if (dividend as i32) < 0 { 2 } else { 0 }; // negate-dividend prologue
    base += if (divisor as i16) < 0 {
        12
    } else if (dividend as i32) < 0 {
        14
    } else {
        10
    };
    base
}

/// The leading-idle width of the DIVU/DIVS divide-by-zero (vector-5) frame — the cycles between the divisor
/// read and the frame push. Pinned to the SOLE vendored div0 sample (`op=0x80ef`, mode 5 `d16(A7)`, len 46):
/// prefix `[Prefetch, Read]` = 8, then this `n8` + the 6-byte frame (writes 12 + vector 8 + reload 10 = 30) =
/// 38, total 46. The detection idle is independent of the EA, so it is fixed (single-sample caveat documented
/// in the runner, like the DBcc-expired path; DIVS has no div0 sample).
const DIV0_TRAP_IDLE: u8 = 8;

/// The MOVE flag computation at `size`: copy the (size-truncated) value, set N=msb / Z=(value==0), clear
/// V/C. Returns `(result, ccr_nz)` where `ccr_nz` carries **only** N/Z/V/C (X is preserved by the caller —
/// MOVE never touches X). The result is zero-extended to 32 bits (the data-register write-back masks per
/// size). Distinct from `add_*`/`sub_*` (which compute X and a real V/C from a real operation).
#[inline]
fn move_flags(value: u32, size: Size) -> (u32, u16) {
    let (result, neg) = match size {
        Size::Byte => {
            let v = value & 0xFF;
            (v, v & 0x80 != 0)
        }
        Size::Word => {
            let v = value & 0xFFFF;
            (v, v & 0x8000 != 0)
        }
        Size::Long => (value, value & 0x8000_0000 != 0),
    };
    let mut ccr = 0u16;
    if neg {
        ccr |= CCR_N;
    }
    if result == 0 {
        ccr |= CCR_Z;
    }
    // V and C are always cleared; X is NOT in `ccr` (the caller preserves it).
    (result, ccr)
}

/// The MOVEA write value at `size`: a `.w` MOVEA **sign-extends** the source word to 32 bits; a `.l`
/// writes the full 32 bits unchanged (byte MOVEA is illegal and never reaches here). No flags — MOVEA never
/// touches the CCR (distinct from [`move_flags`], which computes N/Z).
#[inline]
fn movea_value(value: u32, size: Size) -> u32 {
    match size {
        Size::Word => sign_extend16(value as u16),
        Size::Long => value,
        Size::Byte => unreachable!("byte MOVEA is illegal"),
    }
}

/// The shared **binary-BCD subtract** core (byte-only) computing `dst −₁₀ src − X_in` and its CCR. Used by
/// BOTH [`AluOp::Sbcd`] (`dst`/`src` from the two operands) and [`AluOp::Nbcd`] (the BCD *negate*, which is
/// EXACTLY `sbcd_core(0, operand, X_in)` — `0 −₁₀ operand − X`). It carries the **REAL carry/result
/// ASYMMETRY** (0-mismatch-verified against the vendored `SBCD`/`NBCD` streams — 28 divergent SBCD cases):
/// `binary = dst − src − X_in` (signed); `lowc = 6 if (dst&0xf) − (src&0xf) − X_in < 0 else 0`;
/// **C = X = ((binary − lowc) < 0)** — the borrow keys on `binary − lowc`; **`highc = 0x60 if binary < 0
/// else 0`** — the RESULT's high correction keys on `binary` **(NOT `binary − lowc`)**; the two conditions
/// are computed SEPARATELY (a single shared condition is WRONG); `res = (binary − lowc − highc) & 0xff`;
/// **N = msb(res)**; **V = msb(~res & binary)**. Returns `(res, ccr)` with the N/V/C/X bits set; Z is STICKY
/// and applied by the CALLER (it needs the incoming SR). `xin` is the incoming X flag (0 or 1).
#[inline]
fn sbcd_core(dst: i32, src: i32, xin: i32) -> (u32, u16) {
    let binary = dst - src - xin;
    let lowc = if (dst & 0xF) - (src & 0xF) - xin < 0 {
        6
    } else {
        0
    };
    // The borrow keys on `binary − lowc`; the 0x60 result correction keys on `binary` — they DIVERGE
    // (compute separately). X = C on every op.
    let carry = (binary - lowc) < 0;
    let highc = if binary < 0 { 0x60 } else { 0 };
    let res = ((binary - lowc - highc) & 0xFF) as u32;
    let mut ccr = 0u16;
    if res & 0x80 != 0 {
        ccr |= CCR_N;
    }
    // V = msb(~res & binary) — the fitted, 0-mismatch overflow rule (bit 7 of the AND).
    if (!(res as i32) & binary) & 0x80 != 0 {
        ccr |= CCR_V;
    }
    if carry {
        ccr |= CCR_C | CCR_X;
    }
    (res, ccr)
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
    /// Consume `cycles` master cycles with no bus access (compute / idle `n` cycles). The field is `u16`
    /// because `RESET` idles the bus-reset line for **124** cycles (`[Internal(4), Internal(124), Prefetch]`,
    /// len 132) — beyond the `u8` range the shorter idles (`n2`/`n4`/`n6`) used elsewhere fit in.
    Internal { cycles: u16 },
    /// Apply an address-register side effect: `An += delta` (the `(An)+`/`-(An)` auto-(in/de)crement),
    /// written through [`Registers::addr_reg_set`] so `An == A7` hits the active stack pointer. A 0-cycle,
    /// non-bus one-shot — separate from the operand access so the bump is snapshot-visible and can straddle
    /// a prefetch.
    AdjustAddr { reg: u8, delta: i8 },
    /// Compute an effective address `resolve(base) + resolve(index) + resolve(disp)` (the **full 32-bit**
    /// internal address — **no** 24-bit mask; the bus masks at access time) into scratch slot `dst`. A
    /// **fixed** 3-way `wrapping_add` — there is deliberately **no per-mode match inside `exec_one`**; the
    /// decode-time builder picks which operands feed each leg (`Zero` for an inert one), so every EA mode
    /// shares this single hot-path arm. A 0-cycle, non-bus, snapshot-visible internal step: the materialized
    /// EA is a serializable mid-instruction value. The masking lives at the [`Bus68k`] access (the 68000
    /// address registers are 32-bit; only the external bus drops the top 8 pins), so the address-error abort
    /// (E3) can stack the full 32-bit faulting EA.
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
    /// an address (like [`MicroOp::EaCalc`], which is also unmasked; the bus masks at access time).
    Combine32 { hi: Slot, lo: Operand, dst: Slot },
    /// Set the program counter to a branch destination: `regs.pc = resolve(value).wrapping_sub(4)`. The −4
    /// **primes** the two [`MicroOp::Prefetch`] ops that must follow: each reads at `pc+4` and advances `pc`
    /// by 2, so after `SetPc{value}` + two `Prefetch`s the queue holds `[word@value, word@value+2]` and
    /// `pc == value` (the exact analog of why a sequential `Prefetch` reads `pc+4`: the queue is two words
    /// ahead). **NO 24-bit mask** — the PC stays full 32-bit (a backward branch can land `pc` with high bits
    /// set; only the bus address `read16` masks). A 0-cycle, non-bus, snapshot-visible internal step.
    SetPc { value: Operand },
    /// Compute a branch target / pushed return address `scratch[dst] = resolve(base) + resolve(index) +
    /// resolve(disp)` — the **UNMASKED twin** of [`MicroOp::EaCalc`]. A stored PC / pushed return address is
    /// the full 32-bit value (a backward `Bcc` to `0xFFFF_DB42` must NOT be masked to 24 bits). Like
    /// `EaCalc` this does **NO** 24-bit mask, but it is kept distinct because a target is never a bus
    /// address (it feeds `SetPc`/a frame push, never `Read`/`Write`). A fixed 3-way `wrapping_add` (`Zero`
    /// for an inert leg); a 0-cycle, non-bus, snapshot-visible internal step.
    TargetCalc {
        base: Operand,
        index: Operand,
        disp: Operand,
        dst: Slot,
    },
    /// Decrement the LOW word of data register `Dn` by 1, preserving its high word, affecting **NO flags** —
    /// the `DBcc` loop counter: `d[reg] = (d[reg] & 0xFFFF_0000) | (d[reg].wrapping_sub(1) & 0xFFFF)`. When
    /// the low word is `0` it wraps to `0xFFFF` (the high word is unchanged — the borrow does not propagate),
    /// which is the `−1` the `DBcc` decode-time check reads to terminate the loop. A 0-cycle, non-bus,
    /// snapshot-visible internal step. Distinct from [`MicroOp::Alu`] `Sub` (which sets flags and can write a
    /// full-width result) — `DBcc` never touches the CCR.
    DecrementDnWord { reg: u8 },
    /// Load the condition codes (the low 5 bits, X/N/Z/V/C) of `value` into the CCR, preserving the SR system
    /// byte: `sr = (sr & 0xFF00) | (resolve(value) & 0x1F)` — the `RTR` CCR pop. The popped stack word's low
    /// byte carries the saved CCR; only bits 4-0 are programmer-visible (bits 7-5 read as 0), so the mask is
    /// `0x1F` (pinned to the `RTR` data: a popped `0x..F6` lands `0x16`). A 0-cycle, non-bus internal step.
    LoadCcr { value: Operand },
    /// Enter exception processing: capture the live SR into `scratch[save_sr]` (so the frame push can stack
    /// the SR that was current *at the fault/trap*), then transform the running SR — **set S** (supervisor)
    /// and **clear T** (trace): `scratch[save_sr] = sr; sr = (sr | SR_SUPERVISOR) & !SR_TRACE`. Setting S
    /// routes the subsequent A7 accesses to the supervisor stack via the existing
    /// [`Registers::addr_reg`](super::registers::Registers::addr_reg) S-bit selection (a user→supervisor
    /// switch is a no-op on the all-supervisor vendored data, but the path is exercised structurally by every
    /// frame push). A 0-cycle, non-bus, snapshot-visible internal step.
    EnterException { save_sr: Slot },
    /// Materialize a constant into a scratch slot: `scratch[dst] = value`. Used to stage a fixed bus address
    /// (the exception vector address `(32+n)*4`) into scratch so a plain [`MicroOp::Read`] can fetch the
    /// handler from it. A 0-cycle, non-bus, snapshot-visible internal step.
    LoadImm { value: u32, dst: Slot },
    /// Write a constant byte `value` to a byte destination, affecting **NO flags** — the no-flag analog of
    /// [`MicroOp::LoadImm`] (which targets a [`Slot`]), generalized to a [`Dest`]. The conditional byte write
    /// of `Scc <ea>` (`0xFF` if the condition is true else `0x00`, resolved at decode time): into
    /// [`Dest::DataRegLow8`] it writes the low byte and **preserves the upper 24 bits**, into
    /// [`Dest::Scratch`] it parks the byte (zero-extended) for the trailing memory `Write` to store. Unlike
    /// [`AluOp::Move`] (which CLR uses and which SETS N/Z) this touches no CCR bit. A 0-cycle, non-bus,
    /// snapshot-visible internal step.
    SetByte { value: u8, dst: Dest },
    /// Write a resolved WORD `value` to a word destination, affecting **NO flags** — the word analog of
    /// [`MicroOp::SetByte`]. The store value of `MOVEfromSR` (`EA/Dn.w = SR`, via [`Operand::Sr`]): into
    /// [`Dest::DataRegLow16`] it writes the low word and **preserves the upper 16 bits** (the `Dn` register
    /// form — `Dn.w = SR`, high word kept), into [`Dest::Scratch`] it parks the word (zero-extended) for the
    /// trailing memory `Write` to store (the data-alterable RMW form). Unlike [`AluOp::Move`] (which CLR uses
    /// and which SETS N/Z) this touches no CCR bit, so the SR is byte-identical before/after — the load-bearing
    /// no-flag invariant of MOVEfromSR (an instruction that WRITES the SR value but does NOT modify the SR). A
    /// 0-cycle, non-bus, snapshot-visible internal step.
    SetWord { value: Operand, dst: Dest },
    /// The atomic indivisible **TAS memory** read-modify-write: ONE locked bus cycle (the SST `'t'`
    /// transaction, 10 cyc = read 4 + indivisible modify 2 + write 4) at `resolve(addr)`. Via
    /// [`Bus68k::tas`](super::bus68k::Bus68k::tas) it reads `orig`, writes `orig | 0x80`, and logs ONE `Tas`
    /// transaction (value = the WRITTEN byte). The flags come from the byte READ (`orig`) — N = bit7(orig) /
    /// Z = (orig == 0), V/C cleared, X PRESERVED — while the written value is `orig | 0x80` (DISTINCT, the
    /// same flag/value split as [`AluOp::Tas`]). Always Data FC, byte-only → NEVER faults (a byte access
    /// drives one bus half regardless of parity). It is a SINGLE bus access = ONE quiesce boundary
    /// (indivisible); the recipe must NOT split it into a `Read`+`Write` pair (which would emit `'r'`+`'w'`).
    TasRmw { addr: Operand },
    /// Restore the FULL status register from a popped value, masked to the implemented bits:
    /// `regs.sr = (resolve(value) as u16) & SR_IMPLEMENTED` (`0xA71F` — T | S | I2-I0 | CCR; the unimplemented
    /// bits read as 0). `RTE`'s SR restore — unlike [`MicroOp::LoadCcr`] (which keeps only the low 5 CCR bits
    /// and preserves the SR system byte), this writes the WHOLE SR, so it can flip **S** (supervisor→user) and
    /// **T**. The recipe must run any A7-relative stack pop (the `+6` frame pop) BEFORE this op, so the pop hits
    /// the supervisor stack while S is still set; a later [`MicroOp::Prefetch`] reload then runs under the
    /// RESTORED mode's function code (FC2 user-program if S cleared, FC6 supervisor-program otherwise). A
    /// 0-cycle, non-bus, snapshot-visible internal step. (The `*toSR` write-back shares the same mask via its
    /// own op in a later commit.)
    LoadSr { value: Operand },
    /// `STOP #imm`'s halt step: flag the recipe as a completed `STOP` so the orchestrator moves the CPU to
    /// [`CpuState::Stopped`] at completion, and book the instruction's cycle cost. `STOP` is `4(0/0)` (Yacht
    /// L908 — no bus access; the SR load is a preceding 0-cycle [`MicroOp::LoadSr`]), so this op costs the
    /// full **4** cycles and touches no bus. It is the recipe's last op; wake-on-interrupt is the CPU driver's
    /// job (Push A / A4), not this op's.
    Stop,
    /// Set the SR interrupt priority mask (I2–I0) to `level` (M68000UM §6.3.2 — the interrupt exception raises
    /// the processor priority to the level being acknowledged). Runs AFTER the frame's [`MicroOp::EnterException`]
    /// captured the OLD SR, so the stacked SR carries the pre-interrupt mask. A 0-cycle, non-bus internal step.
    SetIntMask { level: u8 },
    /// The interrupt-acknowledge (`ni`) bus cycle: a word access in **CPU space (FC=7)** at the IACK address
    /// (`0xFFFF_FFF1 | level << 1`). On the Mega Drive the peripheral asserts VPA → the vector is generated
    /// internally (autovector = 24 + level), so the read value is DISCARDED; the cycle exists to place the
    /// acknowledged level on the bus (the `ni` in Yacht L1549). A 4-cycle bus access.
    IntAck { level: u8 },
    /// `CHK <ea>,Dn`'s compare-and-maybe-trap. Signed-compares the low word of `Dn` against `0` and against
    /// `bound` (the resolved EA operand, sign-extended from its low 16). Sets the CCR: **Z=V=C cleared, X
    /// kept**, and **N = 1 if `Dn.w < 0`, N = 0 if `Dn.w > bound`, else N PRESERVED** (the two predicates do
    /// NOT coincide — when `bound < Dn.w < 0`, N is set by `Dn<0` while the idle below is chosen by `Dn>bound`;
    /// confirmed against 547 vendored `neg&&over` cases). If `Dn.w < 0 || Dn.w > bound` the CHK exception is
    /// taken: this reuses the Shape-B execution-time abort — `install_chk_trap` rewrites the in-flight
    /// `MicroState` into the standard 6-byte frame to **vector 6** (`0x18`) with a leading idle of
    /// **n4 if `Dn>bound` else n6**, saved PC =
    /// the live `regs.pc` (this op runs AFTER `ea_src`'s prefetch(es), so `regs.pc` already equals the saved
    /// return PC), and pushed SR = the live SR *with the N just set*. On the no-trap path it is a 0-cycle,
    /// non-bus internal step (the recipe's trailing `Internal(6)` is the no-trap tail). `bound` is the scratch
    /// slot for a memory operand, [`Operand::DataRegLow16`] for a `Dn`-direct bound, or a scratch slot holding
    /// the captured immediate for `#imm` (the decode captures `prefetch[1]` before the refills shift it out, so
    /// this op runs last in every mode). The same op handles every source mode.
    ChkTrap { dn: u8, bound: Operand },
    /// The privileged `*toSR` write-back: `regs.sr = (regs.sr <op> (resolve(value) as u16)) & SR_IMPLEMENTED`
    /// — the `ANDItoSR`/`ORItoSR`/`EORItoSR` ops. The whole SR (T | S | I2-I0 | CCR) is rewritten, so an
    /// `And`/`Eor` can clear **S** (switch supervisor→user) or **T**; the recipe runs this op AFTER the
    /// instruction's leading discard read (under the OLD function code) and BEFORE the two re-prefetch reads
    /// (which then run under the NEW mode's function code — FC2 user-program if S was cleared, FC6
    /// supervisor-program otherwise; this mid-instruction FC switch is the load-bearing pin). Shares the
    /// `SR_IMPLEMENTED` (`0xA71F`) mask with [`MicroOp::LoadSr`] (`RTE`'s restore). A 0-cycle, non-bus,
    /// snapshot-visible internal step.
    SrLogic { op: LogicOp, value: Operand },
    /// The `*toCCR` write-back: `regs.sr = (regs.sr & 0xFF00) | (((regs.sr <op> (resolve(value) as u16)) &
    /// 0x1F))` — the `ANDItoCCR`/`ORItoCCR`/`EORItoCCR` ops. The CCR-masking twin of [`MicroOp::SrLogic`]:
    /// where `SrLogic` rewrites the WHOLE SR (masked to `SR_IMPLEMENTED` `0xA71F`, so an `And`/`Eor` can clear
    /// S/T), `CcrLogic` touches ONLY the CCR byte — the SR **system byte (bits 8-15: T | S | I2-I0) is
    /// PRESERVED** (`sr & 0xFF00`) and only the low-5 CCR bits (X/N/Z/V/C) change; **S/T/I can NEVER change**,
    /// so there is no mid-instruction FC switch (both trailing prefetches stay under the live mode's FC).
    /// Shares [`LogicOp`] with `SrLogic`. A 0-cycle, non-bus, snapshot-visible internal step.
    CcrLogic { op: LogicOp, value: Operand },
    /// `EXG`'s register exchange — swap the two registers named by the opmode form, affecting **NO flags**
    /// (the SR is untouched). `opmode` (the `(opcode >> 3) & 0x1F` field) selects the form: `0x08` = `EXG
    /// Dx,Dy` (swap the two DATA registers `d[rx]` / `d[ry]`); `0x09` = `EXG Ax,Ay` (swap the two ADDRESS
    /// registers via [`Registers::addr_reg`]/[`Registers::addr_reg_set`], so a reg-7 leg hits the ACTIVE A7
    /// = `ssp`/`usp` per the S bit); `0x11` = `EXG Dx,Ay` (swap the DATA register `d[rx]` with the ADDRESS
    /// register `addr_reg(ry)`, again A7-aware). `rx = (opcode >> 9) & 7`, `ry = opcode & 7`. The whole
    /// 32-bit register contents trade places (EXG is long-only). A 0-cycle, non-bus, snapshot-visible
    /// internal step — the recipe's trailing `Internal(2)` idle books the len-6 cost. Distinct from every
    /// `Alu` op (no CCR touch, no size mask) and from [`MicroOp::AdjustAddr`] (a one-sided register bump).
    ExgRegs { opmode: u8, rx: u8, ry: u8 },
    /// `MOVEfromUSP` / `MOVEtoUSP`'s register↔USP transfer, affecting **NO flags** (the SR is untouched — no
    /// CCR change, no privilege gate). `to_usp` selects the direction: `false` = `MOVEfromUSP` (`An = usp`,
    /// full 32 bits) / `true` = `MOVEtoUSP` (`usp = An`, full 32 bits). `an` is the address register number
    /// (`opcode & 7`). A7-aware via [`Registers::addr_reg`]/[`Registers::addr_reg_set`], so `an == 7` reads /
    /// writes the ACTIVE A7 (`ssp` in supervisor mode) — `MOVEfromUSP A7` sets `ssp = usp`, `MOVEtoUSP A7`
    /// sets `usp = ssp` (never a raw `a[7]`, which does not exist — A7 lives in `ssp`/`usp`). A 0-cycle,
    /// non-bus, snapshot-visible internal step — the recipe's trailing `Prefetch` books the len-4 cost.
    /// Mirrors [`MicroOp::ExgRegs`] (the other flag-free A7-aware register op). BOTH ops are privileged on the
    /// 68000, but every vendored case runs in supervisor mode, so the privilege-violation trap is UNEXERCISED
    /// and NOT implemented — correctness-only, like the T-bit trace.
    MoveUsp { to_usp: bool, an: u8 },
    /// `MOVEM` **register→memory** per-register store — one register's word(s) written to the running
    /// transfer address, affecting **NO flags**. The register-list mask is expanded at DECODE time into one
    /// of these per set register (ascending bit order for control modes, REVERSED for `-(An)`), so the recipe
    /// stays a flat linear list (both drivers + snapshot/restore keep working, like the Bcc/DBcc decode-time
    /// expansion). The running address lives in `scratch[addr_slot]` (**never** An — so an `An`-in-list
    /// `-(An)` store writes the INITIAL An, the 68000 behaviour; the recipe's trailing `MoveA` writes the
    /// final address to An).
    ///
    /// `predec` selects the address discipline: `false` (control-alterable, forward) writes at
    /// `scratch[addr_slot]` then advances it `+= size`; `true` (`-(An)`) DECREMENTS `scratch[addr_slot] -=
    /// size` FIRST, then writes at the decremented address (the predecrement-before-store order, iterated over
    /// the reversed list). The write is `size`-wide: a word writes the low 16 of the register; a long writes
    /// both words big-endian (hi @ `addr`, lo @ `addr+2`) — the FORWARD (control) long store writes hi FIRST
    /// then lo, but the `-(An)` predecrement long store REVERSES the bus ACCESS order (lo @ `addr+2` first, then
    /// hi @ `addr` — the MOVE.l `-(An)` "low half first" precedent), pinned against the vendored MOVEM.l stream.
    /// A7 is read
    /// via [`Registers::addr_reg`] (A7-aware) when `reg == 15` (the `A7` list bit). Each word access logs its
    /// own bus transaction and costs 4 cycles; an ODD write address raises the group-0 address error (E3) via
    /// [`Self::install_address_error`] (low5 = `0x05`, a data write) — since the running address is a scratch
    /// slot the register file is unchanged on the abort (An is written only by the recipe's trailing step).
    MovemStore {
        reg: u8,
        size: Size,
        addr_slot: Slot,
        predec: bool,
    },
    /// `MOVEM` **memory→register** per-register load — one register loaded from the running transfer address,
    /// affecting **NO flags**. The decode-time mask expansion emits one per set register (ascending bit
    /// order). The FULL 32-bit register is written (data via `regs.d`, address via [`Registers::addr_reg_set`],
    /// A7-aware when `reg == 15`); a **word** load **SIGN-EXTENDS** to 32 bits (bit15-set word → `0xFFFF….`),
    /// a **long** load (C5) reads two words and writes the combined 32.
    ///
    /// The running address ALWAYS lives in `scratch[addr_slot]` (advanced `+= size` after each load) — even
    /// for `(An)+`, so an `An`-in-list load does NOT corrupt the pointer (the 68000 postincrement wins: An
    /// ends `base + n·size`, ignoring any value loaded into An). The `(An)+` pointer setup, the abort-commit
    /// `An += size` (a leading [`MicroOp::AdjustAddr`], so a faulting first read leaves `An = base + 2`, one
    /// WORD, even for a long — mirroring the E4 "(An)+ commits before the faulting read" ordering), and the
    /// final `An = base + n·size` write (a trailing [`AluOp::MoveA`]) are all supplied by the recipe, NOT this
    /// op. The trailing phantom `Read` uses the final `scratch[addr_slot]` and does NOT advance it. Each word
    /// access logs its own bus transaction and costs 4 cycles; an ODD read address raises the group-0 address
    /// error (E3) via [`Self::install_address_error`] (low5 = `0x15`, a data read).
    MovemLoad {
        reg: u8,
        size: Size,
        addr_slot: Slot,
    },
    /// `MOVEP` **register→memory** per-byte store — one byte of a DATA register `Dn` scattered to the running
    /// ALTERNATING address, affecting **NO flags**. The byte-sized `+2`-stride cousin of [`MicroOp::MovemStore`]:
    /// writes `((regs.d[dn] >> shift) & 0xFF)` as a **byte** at `scratch[addr_slot]` (`Fc::Data`), then advances
    /// the running address `scratch[addr_slot] += 2` (the historical 8-bit-peripheral even/odd interleave). The
    /// recipe emits one per transferred byte, big-endian: word `shift ∈ [8, 0]`, long `[24, 16, 8, 0]`. The
    /// register operand is ALWAYS a plain data register `regs.d[dn]` — never A7 (MOVEP's register is a DATA
    /// register). Byte-addressed → NO alignment fault is EVER possible. Costs 4 cycles, logs one `w .b`
    /// transaction (mirroring `MovemStore`'s per-word access + cycle booking).
    MovepStore { dn: u8, shift: u8, addr_slot: Slot },
    /// `MOVEP` **memory→register** per-byte load — one byte read from the running ALTERNATING address and merged
    /// into a DATA register `Dn`, affecting **NO flags**. The byte-sized `+2`-stride cousin of
    /// [`MicroOp::MovemLoad`]: reads a **byte** at `scratch[addr_slot]` (`Fc::Data`), then MERGES
    /// `regs.d[dn] = (regs.d[dn] & !(0xFFu32 << shift)) | ((byte as u32) << shift)` and advances
    /// `scratch[addr_slot] += 2`. The mask-then-or merge makes the word high-word-PRESERVE (shifts `[8, 0]` leave
    /// bits 16-31 untouched) and the long full-32 overwrite (shifts `[24, 16, 8, 0]` cover every byte) fall out
    /// for free — do NOT zero `Dn` first. The register operand is ALWAYS a plain data register `regs.d[dn]` —
    /// never A7. Byte-addressed → NO alignment fault. Costs 4 cycles, logs one `r .b` transaction.
    MovepLoad { dn: u8, shift: u8, addr_slot: Slot },
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
    /// The original opcode word this recipe was decoded from (set by [`decode`](super::decode::decode);
    /// `0` for hand-built recipes). Latched here because the address-error abort (E3) stacks it as the IR
    /// field and folds it into the SSW — it must survive the prefetch shifts that overwrite `regs.prefetch`.
    opcode: u16,
    /// Set by [`MicroOp::Stop`] when the recipe is a completed `STOP`: the orchestrator moves the CPU to
    /// [`CpuState::Stopped`] once the recipe finishes. A terminal signal the driver reads at completion,
    /// so `STOP`'s state change flows through the same shared cook both drivers call.
    stop_requested: bool,
    /// True when this recipe represents an instruction that was **not executed / was aborted** — a
    /// decode-time exception (illegal / privilege / line-A / line-F) or an execution-time address/bus-error
    /// abort. The `begin_next` trace dispatch reads it to **suppress a pending trace** (M68000UM §6.3.8: no
    /// trace after an illegal/privileged/faulted instruction). Deliberately left `false` for instructions
    /// that DO execute and then trap (`TRAP`/`TRAPV`/`CHK`/div0), so their trace sequences *after* the trap.
    suppresses_trace: bool,
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
            opcode: 0,
            stop_requested: false,
            suppresses_trace: false,
        }
    }

    /// True once a completed [`MicroOp::Stop`] has run — the orchestrator then enters [`CpuState::Stopped`].
    pub fn requests_stop(&self) -> bool {
        self.stop_requested
    }

    /// True when this recipe is a not-executed / aborted entry (illegal / privilege / line-A/F / address-bus
    /// fault) — the `begin_next` trace dispatch reads it to suppress a pending trace (M68000UM §6.3.8).
    pub fn suppresses_trace(&self) -> bool {
        self.suppresses_trace
    }

    /// Mark this recipe as trace-suppressing (see [`MicroState::suppresses_trace`]). Set by the decode-time
    /// exception builder and the address-error install; left `false` for executed-then-trapping instructions.
    pub fn mark_suppresses_trace(&mut self) {
        self.suppresses_trace = true;
    }

    /// Latch the original opcode this recipe was decoded from (see [`MicroState::opcode`]). Called by
    /// [`decode`](super::decode::decode) right after building the recipe; hand-built recipes leave it `0`.
    pub fn set_opcode(&mut self, opcode: u16) {
        self.opcode = opcode;
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
            Operand::Sr => regs.sr as u32,
            Operand::Zero => 0,
            Operand::WordStep => 2,
            Operand::ShiftCount(c) => c as u32,
            Operand::Quick(q) => q as u32,
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
            Operand::BranchDisp8 => sign_extend8((regs.prefetch[0] & 0xFF) as u8),
            Operand::PcPlus(n) => regs.pc.wrapping_add(n as u32),
            Operand::SpPlus(n) => regs.addr_reg(7).wrapping_add(n as i32 as u32),
        }
    }

    /// Install the execution-time **address-error** abort in place (Shape B — the new E3 mechanism): a
    /// faulting word/long bus access (or odd program fetch) detected inside [`Self::exec_one`] rewrites this
    /// in-flight `MicroState` into the group-0 **14-byte exception frame** recipe, seeded from live state.
    ///
    /// The rewrite is a pure data operation: reassign `ops`/`len` and rewind `step` to 0, **preserving**
    /// `cycles` (the faulting micro-op never touched the bus or counted cycles — it returns 0 here; the
    /// frame's leading `Internal(n4)` counts the idle) and `opcode` (the latched IR the frame stacks). Both
    /// drivers keep looping over `exec_one` across the new recipe, so the run-to-completion and quiesce
    /// paths cannot diverge, and the rewritten state is still ordinary fixed-size bincode (snapshot-safe
    /// across the abort).
    ///
    /// `faulting_addr` is the **full 32-bit** access address (the frame stacks it unmasked — see
    /// [`MicroOp::EaCalc`]); `low5` is the SSW low five bits (`read | program | fc`). The SSW high 11 bits
    /// come from the latched `opcode` (not the shifted prefetch). The pushed SR is captured by the frame's
    /// own [`MicroOp::EnterException`] (the LIVE SR at the fault), and the stacked PC is the live `regs.pc`
    /// — no special-casing (a program fault already ran `SetPc{target}` so `regs.pc == target − 4`; a data
    /// fault has `regs.pc == instruction_pc + 2×prefetches_done`).
    fn install_address_error(&mut self, regs: &Registers, faulting_addr: u32, low5: u16) -> u32 {
        use super::ea::RecipeBuf;
        use super::exception::{
            build_address_error_frame, AERR_FAULT_ADDR_SLOT, AERR_IR_SLOT, AERR_SSW_SLOT,
            AERR_STACKED_PC_SLOT,
        };
        let ssw = (self.opcode & 0xFFE0) | low5;
        self.scratch[AERR_STACKED_PC_SLOT as usize] = regs.pc;
        self.scratch[AERR_FAULT_ADDR_SLOT as usize] = faulting_addr;
        self.scratch[AERR_IR_SLOT as usize] = self.opcode as u32;
        self.scratch[AERR_SSW_SLOT as usize] = ssw as u32;
        // (The save-SR slot is filled by the frame's EnterException, capturing the live SR at the fault.)
        let mut buf = RecipeBuf::new();
        build_address_error_frame(&mut buf);
        let ops = buf.as_ops();
        self.ops = [MicroOp::Internal { cycles: 0 }; MAX_OPS];
        self.ops[..ops.len()].copy_from_slice(ops);
        self.len = ops.len() as u8;
        self.step = 0;
        0
    }

    /// Install the `CHK` exception (vector 6) in place — the Shape-B reuse for a CHK out-of-bounds trap. The
    /// faulting [`MicroOp::ChkTrap`] (which has already set the CCR — the live SR now carries CHK's N) rewrites
    /// this in-flight `MicroState` into the standard **6-byte frame** recipe ([`build_chk_frame`]), seeded with
    /// the live `regs.pc` as the stacked return PC. `idle` is the leading-idle width (`n4` when `Dn>bound`,
    /// else `n6` — pinned to the vendored `4396`/`4d91` anchors). Like [`Self::install_address_error`] the
    /// rewrite is a pure data operation (reassign `ops`/`len`, rewind `step`, preserve `cycles`/`opcode`); both
    /// drivers keep looping over `exec_one` across the new recipe, and the rewritten state stays fixed-size
    /// bincode (snapshot-safe across the trap). Returns 0 (the `ChkTrap` micro-op itself costs no cycles — the
    /// leading idle inside the frame counts).
    fn install_chk_trap(&mut self, regs: &Registers, idle: u8) -> u32 {
        use super::ea::RecipeBuf;
        use super::exception::{build_chk_frame, CHK_SAVED_PC_SLOT};
        self.scratch[CHK_SAVED_PC_SLOT as usize] = regs.pc;
        // (The save-SR slot is filled by the frame's EnterException, capturing the live SR — with CHK's N.)
        let mut buf = RecipeBuf::new();
        build_chk_frame(&mut buf, idle);
        let ops = buf.as_ops();
        self.ops = [MicroOp::Internal { cycles: 0 }; MAX_OPS];
        self.ops[..ops.len()].copy_from_slice(ops);
        self.len = ops.len() as u8;
        self.step = 0;
        0
    }

    /// Install the **divide-by-zero** exception (vector 5) in place — the Shape-B reuse for a DIVU/DIVS div0
    /// trap, the twin of [`Self::install_chk_trap`] (which is vector 6). The faulting [`AluOp::Divu`] arm
    /// (which has already set the CCR — N=0/Z=0/V=0/C=0, X preserved — so the live SR the frame captures
    /// carries the div0 CCR) rewrites this in-flight `MicroState` into the standard **6-byte frame** recipe
    /// ([`build_div0_frame`]), seeded with `saved_pc` as the stacked return PC. `idle` is the leading-idle
    /// width — pinned to the sole vendored `op=0x80ef` div0 sample (`idle = 8`, len 46). Like the CHK install
    /// the rewrite is a pure data operation (reassign `ops`/`len`, rewind `step`, preserve `cycles`/`opcode`);
    /// both drivers keep looping over `exec_one` across the new recipe, and the rewritten state stays
    /// fixed-size bincode (snapshot-safe across the trap). Returns 0 (the Alu micro-op costs no cycles — the
    /// frame's leading idle + bus ops count).
    ///
    /// `saved_pc` is the stacked return PC — the **faulting instruction's own address** (the DIVU opcode), NOT
    /// `regs.pc` (the leading prefetch(es) have already advanced it past the ext word(s)). The `Divu` arm
    /// computes it by undoing those advances (`regs.pc - 2*prefetches_before_the_Alu`), pinned to the sole
    /// div0 sample (saved PC `0xc00` = the instruction start). This DIFFERS from CHK (which runs its trap LAST,
    /// after every prefetch, so its saved PC is the next-instruction `= regs.pc`).
    fn install_div0_trap(&mut self, saved_pc: u32, idle: u8) -> u32 {
        use super::ea::RecipeBuf;
        use super::exception::{build_div0_frame, DIV0_SAVED_PC_SLOT};
        self.scratch[DIV0_SAVED_PC_SLOT as usize] = saved_pc;
        // (The save-SR slot is filled by the frame's EnterException, capturing the live SR — with the div0 CCR.)
        let mut buf = RecipeBuf::new();
        build_div0_frame(&mut buf, idle);
        let ops = buf.as_ops();
        self.ops = [MicroOp::Internal { cycles: 0 }; MAX_OPS];
        self.ops[..ops.len()].copy_from_slice(ops);
        self.len = ops.len() as u8;
        self.step = 0;
        0
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
                // Address-error abort (E3): a word/long bus access to an ODD address never reaches the bus —
                // the 68000 aborts the instruction and installs the group-0 14-byte frame. (A byte access
                // drives one bus half regardless of parity, so it can never fault.) `address` is the FULL
                // 32-bit EA (EaCalc no longer masks); the frame stacks it unmasked. low5 for a read =
                // read(0x10) | program(0x08 only for a program-space read) | fc (5 sv-data / 6 sv-program) —
                // a data read is 0x15 (incl. the ADD/SUB RMW, which always faults on the read, never the
                // write).
                if !matches!(size, Size::Byte) && address & 1 != 0 {
                    let program = matches!(fc, Fc::Program);
                    let low5 = 0x10 | (if program { 0x08 } else { 0 }) | regs.fc(program) as u16;
                    return self.install_address_error(regs, address, low5);
                }
                let fc = regs.fc(matches!(fc, Fc::Program));
                // A byte access uses read8 (the single addressed cell, zero-extended); a word uses read16.
                // A long is never a single `Read` — it is two word `Read`s assembled by `Combine32`, so the
                // builder only ever emits word `Read`s for a long operand.
                let (value, wait) = match size {
                    Size::Byte => {
                        let (v, w) = bus.read8(address, fc);
                        (v as u32, w)
                    }
                    Size::Word => {
                        let (v, w) = bus.read16(address, fc);
                        (v as u32, w)
                    }
                    Size::Long => unreachable!("a long Read is two word Reads + Combine32"),
                };
                self.scratch[dst as usize] = value;
                4 + wait
            }
            MicroOp::Write {
                addr,
                fc,
                size,
                value,
            } => {
                let address = self.resolve(addr, regs);
                // Address-error abort (E3): a word/long write to an ODD address never reaches the bus. low5
                // for a write = 0 (read bit clear) | program(0x08, never set for a data write) | fc — a data
                // write is 0x05. MOVE's odd-destination is the only write-fault family in the data, and it
                // stacks the SR with MOVE's CCR already updated (the `EnterException` in the frame captures
                // the live SR at the fault, after the MOVE's `Alu` ran).
                if !matches!(size, Size::Byte) && address & 1 != 0 {
                    let program = matches!(fc, Fc::Program);
                    let low5 = (if program { 0x08 } else { 0 }) | regs.fc(program) as u16;
                    return self.install_address_error(regs, address, low5);
                }
                let fc = regs.fc(matches!(fc, Fc::Program));
                let v = self.resolve(value, regs);
                // A long is never a single `Write` — it is two word `Write`s (the builder feeds the hi word
                // via `Operand::ScratchHi16` and the lo word via `Operand::Scratch`, each truncated to 16).
                let wait = match size {
                    Size::Byte => bus.write8(address, fc, v as u8),
                    Size::Word => bus.write16(address, fc, v as u16),
                    Size::Long => unreachable!("a long Write is two word Writes"),
                };
                4 + wait
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
                // The no-flag An-write ops (MOVEA / ADDA / SUBA) write the full 32-bit An and leave the entire
                // SR untouched (distinct from MOVE/ADD/SUB, which set the CCR). Handled first so they never
                // reach the flag write-back below. MOVEA copies the (word-sign-extended / full-32) source;
                // ADDA/SUBA add/subtract it to/from An at the LONG boundary, where the addend is sign-extended
                // word→long when `size == Word` (mirroring MoveA) else the full long (byte ADDA/SUBA is illegal
                // and never decoded). `a` is the source (MoveA) or the destination An (ADDA/SUBA = the minuend
                // /augend), `b` is the source (ADDA/SUBA).
                match op {
                    AluOp::MoveA => {
                        let value = movea_value(lhs, size);
                        match dst {
                            Dest::AddrReg(n) => regs.addr_reg_set(n as usize, value),
                            _ => unreachable!("MoveA writes only Dest::AddrReg"),
                        }
                        self.step += 1;
                        return 0;
                    }
                    AluOp::Adda => {
                        let addend = match size {
                            Size::Word => sign_extend16(rhs as u16),
                            Size::Long => rhs,
                            Size::Byte => unreachable!("byte ADDA is illegal"),
                        };
                        let value = lhs.wrapping_add(addend);
                        match dst {
                            Dest::AddrReg(n) => regs.addr_reg_set(n as usize, value),
                            _ => unreachable!("Adda writes only Dest::AddrReg"),
                        }
                        self.step += 1;
                        return 0;
                    }
                    AluOp::Suba => {
                        let subtrahend = match size {
                            Size::Word => sign_extend16(rhs as u16),
                            Size::Long => rhs,
                            Size::Byte => unreachable!("byte SUBA is illegal"),
                        };
                        let value = lhs.wrapping_sub(subtrahend);
                        match dst {
                            Dest::AddrReg(n) => regs.addr_reg_set(n as usize, value),
                            _ => unreachable!("Suba writes only Dest::AddrReg"),
                        }
                        self.step += 1;
                        return 0;
                    }
                    // MULU — the UNSIGNED 16×16→32 multiply. An early-return op (like MoveA/ADDA/SUBA) because
                    // the TIMING IS DATA-DEPENDENT ON THE SOURCE: the step's cycle cost is `34 + 2*popcount(b16)`
                    // (the multiplier `b`, resolved here at exec — for a memory source `b` is read mid-instruction
                    // and is not knowable at decode), not the usual 0. `a` = the low word of Dn (multiplicand),
                    // `b` = the source word; the product is `(a&0xFFFF) * (b&0xFFFF)` written FULL-32 to Dn. Flags:
                    // N = bit31(product), Z = (product == 0), V = 0, C = 0, X PRESERVED (move_flags(product, Long)
                    // then re-inject the live X). The full instruction length is `38 + 2*popcount + ea_cost`, this
                    // count-dependent idle being the count's contribution (the operand read + the prefetch refill
                    // are the recipe's other steps).
                    AluOp::Mulu => {
                        let b16 = rhs & 0xFFFF;
                        let product = (lhs & 0xFFFF).wrapping_mul(b16);
                        let (_r, ccr_nz) = move_flags(product, Size::Long);
                        regs.sr = (regs.sr & 0xFF00) | ccr_nz | (regs.sr & CCR_X);
                        match dst {
                            Dest::DataReg(n) => regs.d[n as usize] = product,
                            _ => {
                                unreachable!("MULU writes only Dest::DataReg (the full-32 product)")
                            }
                        }
                        // Early-return op (like MoveA/ADDA/SUBA): advance the cursor AND book the cycles here,
                        // since the shared `self.step += 1; self.cycles += cycles` tail (the end of `exec_one`)
                        // is bypassed by this `return`. The data-dependent cost is `34 + 2*popcount(src16)`.
                        let cycles = 34 + 2 * b16.count_ones();
                        self.step += 1;
                        self.cycles += cycles;
                        return cycles;
                    }
                    // MULS — the SIGNED 16×16→32 multiply. Like MULU, an early-return op whose TIMING IS
                    // DATA-DEPENDENT ON THE SOURCE, but the count is the Booth-recoding TRANSITION count (NOT the
                    // popcount): `34 + 2*booth(b16)` where booth = the number of `01`/`10` bit-pair transitions in
                    // `(b16 << 1)` (a 17-bit value, a 0 appended at the LSB) — `sum over i in 0..15 of (bit(i) !=
                    // bit(i+1))` of `(b16 << 1)`. BOTH operands are SIGN-EXTENDED 16→32 (two's complement) before
                    // the multiply; the low 32 bits of the signed product is written FULL-32 to Dn. Flags identical
                    // to MULU: N = bit31(product), Z = (product == 0), V = 0, C = 0, X PRESERVED. The full
                    // instruction length is `38 + 2*booth + ea_cost`.
                    AluOp::Muls => {
                        let a16 = lhs & 0xFFFF;
                        let b16 = rhs & 0xFFFF;
                        // Sign-extend both 16-bit operands to 32 bits (two's complement), multiply as i32, keep the
                        // low 32 bits of the signed product.
                        let sa = (a16 as u16) as i16 as i32;
                        let sb = (b16 as u16) as i16 as i32;
                        let product = (sa.wrapping_mul(sb)) as u32;
                        let (_r, ccr_nz) = move_flags(product, Size::Long);
                        regs.sr = (regs.sr & 0xFF00) | ccr_nz | (regs.sr & CCR_X);
                        match dst {
                            Dest::DataReg(n) => regs.d[n as usize] = product,
                            _ => {
                                unreachable!("MULS writes only Dest::DataReg (the full-32 product)")
                            }
                        }
                        // Booth-recoding transition count of `(b16 << 1)`: the number of `01`/`10` bit-pair
                        // transitions in the 17-bit LSB-extended source — `sum over i in 0..15 of (bit(i) !=
                        // bit(i+1))` of `(b16 << 1)`.
                        let booth = booth_transitions(b16);
                        // Early-return op (like MULU): advance the cursor AND book the cycles here, since the shared
                        // tail is bypassed by this `return`. The data-dependent cost is `34 + 2*booth(src16)`.
                        let cycles = 34 + 2 * booth;
                        self.step += 1;
                        self.cycles += cycles;
                        return cycles;
                    }
                    // DIVU — the UNSIGNED 16-bit divide. An early-return op (like MULU/MULS) whose TIMING IS
                    // DATA-DEPENDENT ON THE SOURCE; it self-books `self.cycles` and returns its step cost. `a` =
                    // the FULL 32-bit dividend (DataRegFull), `b` = the resolved word source (`b & 0xFFFF` = the
                    // divisor). THREE outcomes — div0 (rewrite-MicroState into the vector-5 trap), overflow
                    // (Dn unchanged, only V/C change), normal (write Dn, only N/Z change). UNLIKE MULU/MULS the
                    // final `Prefetch` trails this Alu (the `[idle, prefetch]` order the data pins), so the Alu
                    // returns the documented division cost MINUS 4 — the trailing `Prefetch` books that 4, so the
                    // mode-0 instruction total is the documented cost and a memory source adds its EA cost on top.
                    AluOp::Divu => {
                        let dividend = lhs; // a = DataRegFull(dn): the full 32-bit dividend
                        let divisor = rhs & 0xFFFF; // the resolved word source
                        if divisor == 0 {
                            // DIVIDE-BY-ZERO trap (vector 5). Set the CCR (N=0,Z=0,V=0,C=0, X preserved) BEFORE
                            // the frame's EnterException captures the live SR, then rewrite the in-flight
                            // MicroState into the vector-5 6-byte frame. Dn UNCHANGED. The stacked PC is the
                            // FAULTING instruction's own address — undo the leading prefetch(es)' pc advance
                            // (`regs.pc - 2*prefetches_done`), since the div0 trap saves the instruction start,
                            // unlike CHK (which runs last and saves the next-instruction pc).
                            regs.sr = (regs.sr & 0xFF00) | (regs.sr & CCR_X);
                            let prefetches_done = self.ops[..self.step as usize]
                                .iter()
                                .filter(|o| matches!(o, MicroOp::Prefetch))
                                .count() as u32;
                            let saved_pc = regs.pc.wrapping_sub(2 * prefetches_done);
                            return self.install_div0_trap(saved_pc, DIV0_TRAP_IDLE);
                        }
                        if (dividend >> 16) >= divisor {
                            // OVERFLOW (quotient > 0xFFFF): Dn UNCHANGED. CCR V=1, C=0, N/Z/X PRESERVED — only V
                            // set and C cleared (NOT a partial-state N/Z). Flat division cost 10 (idle = 10 - 4).
                            regs.sr =
                                (regs.sr & 0xFF00) | (regs.sr & (CCR_X | CCR_N | CCR_Z)) | CCR_V;
                            let idle = 10 - 4;
                            self.step += 1;
                            self.cycles += idle;
                            return idle;
                        }
                        // NORMAL: q = dividend / divisor, r = dividend % divisor; Dn = (rem << 16) | quot.
                        let q = dividend / divisor;
                        let r = dividend % divisor;
                        let value = ((r & 0xFFFF) << 16) | (q & 0xFFFF);
                        // CCR: N = bit15(q), Z = (q & 0xFFFF == 0), V=0, C=0, X PRESERVED (only N/Z change).
                        let n = if (q >> 15) & 1 != 0 { CCR_N } else { 0 };
                        let z = if (q & 0xFFFF) == 0 { CCR_Z } else { 0 };
                        regs.sr = (regs.sr & 0xFF00) | (regs.sr & CCR_X) | n | z;
                        match dst {
                            Dest::DataReg(dn) => regs.d[dn as usize] = value,
                            _ => unreachable!(
                                "DIVU writes only Dest::DataReg (quotient low / remainder high)"
                            ),
                        }
                        // The variable bit-serial division cost (76 + 2*n_keep + 4*n_restore); the Alu returns it
                        // MINUS 4 (the trailing Prefetch books that 4 — see the `[idle, prefetch]` order above).
                        let idle = divu_cycles(dividend, divisor) - 4;
                        self.step += 1;
                        self.cycles += idle;
                        return idle;
                    }
                    // DIVS — the SIGNED 16-bit divide, the signed twin of DIVU (same three-outcome shape, the
                    // same vector-5 div0 frame, the same Alu-returns-cycles minus-4 mechanism). `a` = the FULL
                    // 32-bit dividend `sdd = sx32(Dn)` (an i32); `b & 0xFFFF` sign-extended is the divisor `sds =
                    // sx16(b)` (an i16). The quotient TRUNCATES TOWARD ZERO (computed in i64 so the i32::MIN/-1
                    // overflow case is DETECTED, not a panic), and the remainder TAKES THE DIVIDEND'S SIGN
                    // (`r = sdd - q*sds`).
                    AluOp::Divs => {
                        let dividend = lhs; // a = DataRegFull(dn): the full 32-bit dividend
                        let divisor = rhs & 0xFFFF; // the resolved word source
                        if divisor == 0 {
                            // DIVIDE-BY-ZERO trap (vector 5) — IDENTICAL to DIVU. CCR N=0,Z=0,V=0,C=0, X
                            // preserved BEFORE the frame captures the live SR, then rewrite into the vector-5
                            // 6-byte frame. Dn UNCHANGED. Saved PC = the faulting instruction's own address
                            // (undo the leading prefetch(es)' pc advance). (DIVS has NO div0 vendored sample —
                            // implemented for correctness only.)
                            regs.sr = (regs.sr & 0xFF00) | (regs.sr & CCR_X);
                            let prefetches_done = self.ops[..self.step as usize]
                                .iter()
                                .filter(|o| matches!(o, MicroOp::Prefetch))
                                .count() as u32;
                            let saved_pc = regs.pc.wrapping_sub(2 * prefetches_done);
                            return self.install_div0_trap(saved_pc, DIV0_TRAP_IDLE);
                        }
                        let sdd = dividend as i32;
                        let sds = (divisor as u16) as i16;
                        // Truncating division in i64 — q can be +0x8000_0000 (the i32::MIN/-1 case), which an i32
                        // divide would PANIC on; here it is detected as overflow below.
                        let q64 = (sdd as i64) / (sds as i64);
                        if !(-0x8000..=0x7FFF).contains(&q64) {
                            // OVERFLOW (q out of [-0x8000, 0x7FFF]): Dn UNCHANGED. CCR V=1, C=0, N/Z/X PRESERVED
                            // (identical to DIVU). Flat division cost 16 (dividend >= 0) / 18 (dividend < 0) — the
                            // +2 is the negate-dividend prologue; the Alu returns that MINUS 4.
                            regs.sr =
                                (regs.sr & 0xFF00) | (regs.sr & (CCR_X | CCR_N | CCR_Z)) | CCR_V;
                            let total = if sdd >= 0 { 16 } else { 18 };
                            let idle = total - 4;
                            self.step += 1;
                            self.cycles += idle;
                            return idle;
                        }
                        // NORMAL: q truncates toward zero, r = sdd - q*sds (remainder takes the dividend's sign);
                        // Dn = ((r & 0xFFFF) << 16) | (q & 0xFFFF).
                        let q = q64 as i32;
                        let r = sdd.wrapping_sub(q.wrapping_mul(sds as i32));
                        let value = (((r as u32) & 0xFFFF) << 16) | ((q as u32) & 0xFFFF);
                        // CCR: N = bit15(q) ((q>>15)&1), Z = (q & 0xFFFF == 0), V=0, C=0, X PRESERVED.
                        let qm = (q as u32) & 0xFFFF;
                        let n = if (qm >> 15) & 1 != 0 { CCR_N } else { 0 };
                        let z = if qm == 0 { CCR_Z } else { 0 };
                        regs.sr = (regs.sr & 0xFF00) | (regs.sr & CCR_X) | n | z;
                        match dst {
                            Dest::DataReg(dn) => regs.d[dn as usize] = value,
                            _ => unreachable!(
                                "DIVS writes only Dest::DataReg (quotient low / remainder high)"
                            ),
                        }
                        // The variable abs-value restoring-division cost (110 + 2*n_restore + sign terms); the
                        // Alu returns it MINUS 4 (the trailing Prefetch books that 4).
                        let idle = divs_cycles(dividend, divisor) - 4;
                        self.step += 1;
                        self.cycles += idle;
                        return idle;
                    }
                    _ => {}
                }
                // Compute at the operand-size flag boundary; carry the result (zero-extended to 32) + the new
                // low-byte CCR uniformly. MOVE is NOT arithmetic — it copies `a` and sets only N/Z (V/C
                // cleared) while PRESERVING X, so its `ccr` re-injects the live X bit (add/sub recompute X).
                let (result, ccr) = match op {
                    AluOp::MoveA
                    | AluOp::Adda
                    | AluOp::Suba
                    | AluOp::Mulu
                    | AluOp::Muls
                    | AluOp::Divu
                    | AluOp::Divs => {
                        unreachable!(
                            "early-return op (no-flag An-write / data-dependent-cycle MULU/MULS/DIVU/DIVS) handled above"
                        )
                    }
                    AluOp::Move => {
                        let (r, ccr_nz) = move_flags(lhs, size);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // AND is bitwise `a & b` with the MOVE flag shape: N = msb / Z = (result == 0) at size,
                    // V/C cleared, X PRESERVED (re-inject the live X — logic never touches X). `move_flags`
                    // masks `a & b` to the operand size and computes N/Z; the size-masked result is the
                    // write-back value (or the parked memory store).
                    AluOp::And => {
                        let (r, ccr_nz) = move_flags(lhs & rhs, size);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // OR is bitwise `a | b` with the same MOVE flag shape as AND (only the bit op differs):
                    // N = msb / Z = (result == 0) at size, V/C cleared, X PRESERVED (re-inject the live X).
                    AluOp::Or => {
                        let (r, ccr_nz) = move_flags(lhs | rhs, size);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // EOR is bitwise `a ^ b` with the same MOVE flag shape as AND/OR (only the bit op differs):
                    // N = msb / Z = (result == 0) at size, V/C cleared, X PRESERVED (re-inject the live X).
                    AluOp::Eor => {
                        let (r, ccr_nz) = move_flags(lhs ^ rhs, size);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // NOT is the UNARY bitwise complement `~a` with the same MOVE flag shape as AND/OR/EOR (only
                    // the bit op differs — `~lhs` instead of `lhs ^ rhs`, `rhs`/`b` ignored, passed
                    // `Operand::Zero` by the recipe): N = msb / Z = (result == 0) at size, V/C cleared, X
                    // PRESERVED (re-inject the live X — logic never touches X). `move_flags` masks `!lhs` to the
                    // operand size and computes N/Z; the size-masked result is the write-back value.
                    AluOp::Not => {
                        let (r, ccr_nz) = move_flags(!lhs, size);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // CMP is SUB's N/Z/V/C with X PRESERVED (never written) and no write-back. Compute the
                    // subtraction's flags exactly as Sub, then strip its X and re-inject the live X.
                    AluOp::Cmp => {
                        let (r, sub_ccr) = match size {
                            Size::Word => {
                                let (r, ccr) = sub_w(lhs as u16, rhs as u16);
                                (r as u32, ccr)
                            }
                            Size::Byte => {
                                let (r, ccr) = sub_b(lhs as u8, rhs as u8);
                                (r as u32, ccr)
                            }
                            Size::Long => sub_l(lhs, rhs),
                        };
                        (r, (sub_ccr & !CCR_X) | (regs.sr & CCR_X))
                    }
                    // CMPA is `An − b` at the LONG boundary, `b` sign-extended word→long when size == Word
                    // (mirroring MoveA's internal sign-extension), else the full long. N/Z/V/C from sub_l, X
                    // PRESERVED (re-inject the live X), no write-back. `a` (An) is always full 32 bits.
                    AluOp::Cmpa => {
                        let b = match size {
                            Size::Word => sign_extend16(rhs as u16),
                            Size::Long => rhs,
                            Size::Byte => unreachable!("byte CMPA is illegal"),
                        };
                        let (r, sub_ccr) = sub_l(lhs, b);
                        (r, (sub_ccr & !CCR_X) | (regs.sr & CCR_X))
                    }
                    // NEG is the UNARY `0 − a` — byte-identical to `Sub(0, a)`, so delegate to the same sub_*
                    // helpers with `lhs = 0, rhs = a` (the resolved operand `a`; `b`/`rhs` is ignored, passed as
                    // `Operand::Zero` by the recipe). The operand order is load-bearing: `sub_*(0, a)` makes
                    // V/C/X come out as the borrow/overflow of `0 − a`. N/Z/V/C + X = C straight from the helper.
                    AluOp::Neg => match size {
                        Size::Word => {
                            let (r, ccr) = sub_w(0, lhs as u16);
                            (r as u32, ccr)
                        }
                        Size::Byte => {
                            let (r, ccr) = sub_b(0, lhs as u8);
                            (r as u32, ccr)
                        }
                        Size::Long => sub_l(0, lhs),
                    },
                    // NEGX is the UNARY `0 − a − X_in` — a DEDICATED op (no Sub/Cmp delegation): it carries the
                    // STICKY Z and the incoming X that participates in BOTH the value and the borrow. X_in /
                    // Z_in are the LIVE CCR bits (`sr >> 4 & 1` / `sr >> 2 & 1`). The flag formulas are
                    // 0-mismatch-verified against the vendored NEGX stream: N = msb(res); Z = STICKY
                    // (`Z_in AND res == 0` — NEGX only ever CLEARS Z, so a plain `res == 0` is WRONG when
                    // `res == 0 && Z_in == 0`); V = `(a & res & signbit) != 0`; C = X = NOT(`a == 0 && X_in == 0`)
                    // (the borrow of `0 − a − X_in`). `b`/`rhs` is ignored (passed `Operand::Zero` by the recipe).
                    AluOp::Negx => {
                        let (mask, signbit) = match size {
                            Size::Byte => (0xFFu32, 0x80u32),
                            Size::Word => (0xFFFF, 0x8000),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
                        };
                        let d = lhs & mask;
                        let xin = u32::from(regs.sr & CCR_X != 0);
                        let res = 0u32.wrapping_sub(d).wrapping_sub(xin) & mask;
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        // STICKY Z: keep the incoming Z bit only when the result is zero; clear it otherwise.
                        if res == 0 && regs.sr & CCR_Z != 0 {
                            ccr |= CCR_Z;
                        }
                        if d & res & signbit != 0 {
                            ccr |= CCR_V;
                        }
                        if !(d == 0 && xin == 0) {
                            ccr |= CCR_C | CCR_X;
                        }
                        (res, ccr)
                    }
                    // ADDX is the BINARY `a + b + X_in` — a DEDICATED op (no Add delegation): the incoming X
                    // participates in BOTH the value and the carry, and Z is STICKY. X_in / Z_in are the LIVE
                    // CCR bits (`sr >> 4 & 1` / `sr >> 2 & 1`). 0-mismatch-verified against the vendored ADDX
                    // stream: `raw = a + b + X_in` (wide); `res = raw & mask`; C = X = (raw > mask) (carry-out);
                    // V = msb(~(a^b) & (a^res)) (standard add overflow — both operands one sign, result the
                    // other); N = msb(res); Z = STICKY (`Z_in AND res == 0` — a non-zero limb clears Z, a zero
                    // limb leaves the running Z untouched, so a plain `res == 0` is WRONG on `res == 0 &&
                    // Z_in == 0`).
                    AluOp::Addx => {
                        let (mask, signbit) = match size {
                            Size::Byte => (0xFFu32, 0x80u32),
                            Size::Word => (0xFFFF, 0x8000),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
                        };
                        let dst = lhs & mask;
                        let src = rhs & mask;
                        let xin = u32::from(regs.sr & CCR_X != 0);
                        let raw = u64::from(dst) + u64::from(src) + u64::from(xin);
                        let res = (raw as u32) & mask;
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        // STICKY Z: keep the incoming Z bit only when the result is zero; clear it otherwise.
                        if res == 0 && regs.sr & CCR_Z != 0 {
                            ccr |= CCR_Z;
                        }
                        if (!(dst ^ src)) & (dst ^ res) & signbit != 0 {
                            ccr |= CCR_V;
                        }
                        if raw > u64::from(mask) {
                            ccr |= CCR_C | CCR_X;
                        }
                        (res, ccr)
                    }
                    // SUBX is the BINARY `a − b − X_in` — a DEDICATED op (no Sub delegation): the incoming X
                    // participates in BOTH the value and the borrow, and Z is STICKY. X_in / Z_in are the LIVE
                    // CCR bits (`sr >> 4 & 1` / `sr >> 2 & 1`). 0-mismatch-verified against the vendored SUBX
                    // stream: `raw = dst − src − X_in` (signed wide); `res = raw & mask`; C = X = (raw < 0)
                    // (borrow-out); V = msb((dst^src) & (dst^res)) (standard sub overflow — minuend and
                    // subtrahend opposite sign, result differs from the minuend); N = msb(res); Z = STICKY
                    // (`Z_in AND res == 0` — a non-zero limb clears Z, a zero limb leaves the running Z
                    // untouched, so a plain `res == 0` is WRONG on `res == 0 && Z_in == 0`).
                    AluOp::Subx => {
                        let (mask, signbit) = match size {
                            Size::Byte => (0xFFu32, 0x80u32),
                            Size::Word => (0xFFFF, 0x8000),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
                        };
                        let dst = lhs & mask;
                        let src = rhs & mask;
                        let xin = i64::from(regs.sr & CCR_X != 0);
                        let raw = i64::from(dst) - i64::from(src) - xin;
                        let res = (raw as u32) & mask;
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        // STICKY Z: keep the incoming Z bit only when the result is zero; clear it otherwise.
                        if res == 0 && regs.sr & CCR_Z != 0 {
                            ccr |= CCR_Z;
                        }
                        if (dst ^ src) & (dst ^ res) & signbit != 0 {
                            ccr |= CCR_V;
                        }
                        if raw < 0 {
                            ccr |= CCR_C | CCR_X;
                        }
                        (res, ccr)
                    }
                    // ABCD is the BINARY BCD `dst +₁₀ src + X_in` (byte-only) — a DEDICATED op (no Add
                    // delegation): the incoming X participates in BOTH the value and the carry, and Z is STICKY.
                    // 0-mismatch-verified against the vendored ABCD stream: `binary = dst + src + X_in`;
                    // `lowc = 6 if (dst&0xf)+(src&0xf)+X_in > 9 else 0`; C = X = (binary > 0x99) (the HIGH carry —
                    // NOT the low correction); `res = (binary + lowc + (0x60 if C else 0)) & 0xff`; N = msb(res);
                    // V = msb(res & ~binary) (the undefined-but-deterministic overflow the real chip produces);
                    // Z = STICKY (`Z_in AND res == 0`). `a`/lhs = dst, `b`/rhs = src.
                    AluOp::Abcd => {
                        let dst = (lhs & 0xFF) as i32;
                        let src = (rhs & 0xFF) as i32;
                        let xin = i32::from(regs.sr & CCR_X != 0);
                        let binary = dst + src + xin;
                        let lowc = if (dst & 0xF) + (src & 0xF) + xin > 9 {
                            6
                        } else {
                            0
                        };
                        let carry = binary > 0x99;
                        let highc = if carry { 0x60 } else { 0 };
                        let res = ((binary + lowc + highc) & 0xFF) as u32;
                        let mut ccr = 0u16;
                        if res & 0x80 != 0 {
                            ccr |= CCR_N;
                        }
                        // STICKY Z: keep the incoming Z bit only when the result is zero; clear it otherwise.
                        if res == 0 && regs.sr & CCR_Z != 0 {
                            ccr |= CCR_Z;
                        }
                        // V = msb(res & ~binary) — the fitted, 0-mismatch overflow rule (bit 7 of the AND).
                        if (res as i32 & !binary) & 0x80 != 0 {
                            ccr |= CCR_V;
                        }
                        if carry {
                            ccr |= CCR_C | CCR_X;
                        }
                        (res, ccr)
                    }
                    // SBCD is the BINARY BCD `dst −₁₀ src − X_in` (byte-only) — a DEDICATED op (no Sub delegation):
                    // the incoming X participates in BOTH the value and the borrow, and Z is STICKY. It carries a
                    // REAL carry/result ASYMMETRY (0-mismatch-verified against the vendored SBCD stream — 28
                    // divergent cases): `binary = dst − src − X_in` (signed); `lowc = 6 if (dst&0xf)−(src&0xf)−X_in
                    // < 0 else 0`; C = X = ((binary − lowc) < 0) — the borrow keys on `binary − lowc`; highc =
                    // 0x60 if binary < 0 (the RESULT's high correction keys on `binary`, NOT `binary − lowc`);
                    // `res = (binary − lowc − highc) & 0xff`; N = msb(res); V = msb(~res & binary); Z = STICKY.
                    // The carry and highc conditions DIFFER (small-positive binary + strongly-negative low nibble)
                    // — computed SEPARATELY; a single shared condition is WRONG. `a`/lhs = dst, `b`/rhs = src.
                    AluOp::Sbcd => {
                        let dst = (lhs & 0xFF) as i32;
                        let src = (rhs & 0xFF) as i32;
                        let xin = i32::from(regs.sr & CCR_X != 0);
                        let (res, mut ccr) = sbcd_core(dst, src, xin);
                        // STICKY Z: keep the incoming Z bit only when the result is zero; clear it otherwise.
                        if res == 0 && regs.sr & CCR_Z != 0 {
                            ccr |= CCR_Z;
                        }
                        (res, ccr)
                    }
                    // NBCD is the BCD NEGATE `0 −₁₀ operand − X_in` (byte-only) — EXACTLY the SBCD core with
                    // `dst = 0` and `src = operand` (0-mismatch-verified against the vendored NBCD stream). Like
                    // SBCD it is a DEDICATED op (no Sub delegation): the incoming X folds into BOTH the value and
                    // the borrow, and Z is STICKY. The recipe reads the single data-alterable EA into `a`/lhs, so
                    // `src = (lhs & 0xFF)` and `dst = 0`; the SAME carry/result asymmetry, N, and V = msb(~res &
                    // binary) rules apply (delegated to `sbcd_core`). `b`/rhs is ignored (recipe passes Zero).
                    AluOp::Nbcd => {
                        let src = (lhs & 0xFF) as i32;
                        let xin = i32::from(regs.sr & CCR_X != 0);
                        let (res, mut ccr) = sbcd_core(0, src, xin);
                        // STICKY Z: keep the incoming Z bit only when the result is zero; clear it otherwise.
                        if res == 0 && regs.sr & CCR_Z != 0 {
                            ccr |= CCR_Z;
                        }
                        (res, ccr)
                    }
                    // EXT is the UNARY `Dn`-only sign-extend whose width follows `size`: EXT.w sign-extends the
                    // low BYTE of `a` to 16 bits (`res = sign_extend8→16(a & 0xFF)`, written to the low word — the
                    // high word of Dn is preserved by `Dest::DataRegLow16`); EXT.l sign-extends the low WORD to
                    // 32 bits (`res = sign_extend16→32(a & 0xFFFF)`, full 32). Logic-shaped (the MOVE flag shape):
                    // N = msb at `size` (bit15 for .w, bit31 for .l), Z = (result == 0 at size), V/C cleared, X
                    // PRESERVED (re-inject the live X). `b`/`rhs` is ignored (passed `Operand::Zero` by the recipe).
                    AluOp::Ext => {
                        let res = match size {
                            Size::Word => sign_extend8(lhs as u8) & 0xFFFF,
                            Size::Long => sign_extend16(lhs as u16),
                            Size::Byte => unreachable!("EXT is .w/.l only"),
                        };
                        let (r, ccr_nz) = move_flags(res, size);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // SWAP is the UNARY `Dn`-only 16-bit halfword swap on the FULL 32 bits: `res = (a >> 16) |
                    // (a << 16)` (`size` is always Long). Logic-shaped (the MOVE flag shape): N = bit31 of the
                    // swapped result, Z = (result == 0), V/C cleared, X PRESERVED (re-inject the live X). `b`/`rhs`
                    // is ignored (passed `Operand::Zero` by the recipe).
                    AluOp::Swap => {
                        // `(lhs >> 16) | (lhs << 16)` on a 32-bit value is exactly a 16-bit rotate.
                        let res = lhs.rotate_left(16);
                        let (r, ccr_nz) = move_flags(res, Size::Long);
                        (r, ccr_nz | (regs.sr & CCR_X))
                    }
                    // TAS (Dn register form): the flags are computed on the INPUT byte `a` — N = bit7(a&0xFF) /
                    // Z = (a&0xFF == 0), V/C cleared, X PRESERVED (re-inject the live X) — while the WRITTEN
                    // value is `(a & 0xFF) | 0x80` (bit 7 always set). The KEY subtlety vs NOT: the flag input
                    // (`a`) DIFFERS from the write value (`a|0x80`); NOT flags on the result `~a`, TAS flags on
                    // the unmodified input. `b`/`rhs` is ignored (passed `Operand::Zero` by the recipe).
                    AluOp::Tas => {
                        let (_orig, ccr_nz) = move_flags(lhs, Size::Byte);
                        let res = (lhs & 0xFF) | 0x80;
                        (res, ccr_nz | (regs.sr & CCR_X))
                    }
                    // BTST tests one bit of `a` (the operand) selected by `b` (the bit number), setting ONLY Z =
                    // NOT(bit). The bit width follows `size`: Long → 32 (`Dn` operand, mod 32), else 8 (memory /
                    // imm / PC-rel operand, mod 8). X/N/V/C are ALL preserved — `ccr = (sr & (X|N|V|C)) | Z`; the
                    // SR system byte is preserved by the shared `(sr & 0xFF00) | ccr` write-back below. The
                    // returned value is inert (the recipe pairs BTST with `Dest::None`, so nothing is written).
                    AluOp::Btst => {
                        let bits: u32 = if size == Size::Long { 32 } else { 8 };
                        let pos = rhs % bits;
                        let bit = (lhs >> pos) & 1;
                        let z = if bit == 0 { CCR_Z } else { 0 };
                        let preserved = regs.sr & (CCR_X | CCR_N | CCR_V | CCR_C);
                        (lhs, preserved | z)
                    }
                    // BCHG is BTST + TOGGLE: identical Z = NOT(the PRE-modify bit) (X/N/V/C preserved, only Z
                    // changes), then the written value is `a ^ (1 << pos)` (flip the tested bit). The Z flag is
                    // from the bit BEFORE the toggle (`lhs`), NOT the result. The bit width follows `size`: Long
                    // → 32 (`Dn` dest, mod 32, FULL-32 write with one bit flipped), else 8 (memory dest, mod 8,
                    // byte with one bit flipped). The recipe pairs this with `Dest::DataReg` (Dn) / `Dest::Scratch`
                    // (the `ea_dst` byte write source).
                    AluOp::Bchg => {
                        let bits: u32 = if size == Size::Long { 32 } else { 8 };
                        let pos = rhs % bits;
                        let bit = (lhs >> pos) & 1;
                        let z = if bit == 0 { CCR_Z } else { 0 };
                        let preserved = regs.sr & (CCR_X | CCR_N | CCR_V | CCR_C);
                        (lhs ^ (1 << pos), preserved | z)
                    }
                    // BCLR is BTST + CLEAR: identical Z = NOT(the PRE-clear bit) (X/N/V/C preserved, only Z
                    // changes), then the written value is `a & !(1 << pos)` (clear the tested bit). The Z flag is
                    // from the bit BEFORE the clear (`lhs`), NOT the result. Same bit-width-follows-`size` rule as
                    // BCHG: Long → 32 (`Dn` dest, mod 32, FULL-32 write with one bit cleared), else 8 (memory
                    // dest, mod 8, byte with one bit cleared). The recipe pairs this with `Dest::DataReg` (Dn) /
                    // `Dest::Scratch` (the `ea_dst` byte write source).
                    AluOp::Bclr => {
                        let bits: u32 = if size == Size::Long { 32 } else { 8 };
                        let pos = rhs % bits;
                        let bit = (lhs >> pos) & 1;
                        let z = if bit == 0 { CCR_Z } else { 0 };
                        let preserved = regs.sr & (CCR_X | CCR_N | CCR_V | CCR_C);
                        (lhs & !(1 << pos), preserved | z)
                    }
                    // BSET is BTST + SET: identical Z = NOT(the PRE-set bit) (X/N/V/C preserved, only Z changes),
                    // then the written value is `a | (1 << pos)` (set the tested bit). The Z flag is from the bit
                    // BEFORE the set (`lhs`), NOT the result. Same bit-width-follows-`size` rule as BCHG/BCLR:
                    // Long → 32 (`Dn` dest, mod 32, FULL-32 write with one bit set), else 8 (memory dest, mod 8,
                    // byte with one bit set). The recipe pairs this with `Dest::DataReg` (Dn) / `Dest::Scratch`
                    // (the `ea_dst` byte write source).
                    AluOp::Bset => {
                        let bits: u32 = if size == Size::Long { 32 } else { 8 };
                        let pos = rhs % bits;
                        let bit = (lhs >> pos) & 1;
                        let z = if bit == 0 { CCR_Z } else { 0 };
                        let preserved = regs.sr & (CCR_X | CCR_N | CCR_V | CCR_C);
                        (lhs | (1 << pos), preserved | z)
                    }
                    // ASL — arithmetic shift LEFT by `cnt = b & 63` (the resolved count). `x` = the size-masked
                    // operand `a`; `n` = 8/16/32. ASL is the ONE shift that owns V (the sign bit changed at ANY
                    // point during the shift). Value `res = (x << cnt) & mask` when `cnt < n`, else 0. C = the
                    // last bit shifted out of the operand (`bit(n-cnt)` for `1 <= cnt <= n`, else 0); X = C. V
                    // (closed form): `cnt >= n` → `V = (x != 0)` (`x == mask` shifts a 0 into the sign, so it
                    // DOES change → V=1); `cnt < n` → the top `cnt+1` bits are not all-equal. ZERO COUNT
                    // (`cnt == 0`, only the dynamic `Dn` form): value unchanged, V=0, C=0, **X PRESERVED**
                    // (re-inject the live X — the shift never ran), N/Z from the unchanged operand. All Rust
                    // shifts are guarded: `res` uses `cnt < n`; the V top-mask shifts by `n-1-cnt ∈ 0..n-1`
                    // (only in the `cnt < n` branch); `x >> (n-cnt)` runs only for `1 <= cnt <= n`.
                    AluOp::Asl => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let res = if cnt < n { (x << cnt) & mask } else { 0 };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        if cnt == 0 {
                            // Zero count: V=0, C=0, X PRESERVED (the shift never ran — re-inject the live X).
                            ccr |= regs.sr & CCR_X;
                        } else {
                            // C = the last bit shifted out of the operand (0 once `cnt > n`); X = C.
                            let c = if cnt <= n { (x >> (n - cnt)) & 1 } else { 0 };
                            if c != 0 {
                                ccr |= CCR_C | CCR_X;
                            }
                            // V = the sign bit changed at any point during the shift.
                            let v = if cnt >= n {
                                x != 0
                            } else {
                                // The top `cnt+1` bits of the n-bit field (positions n-1-cnt..=n-1): the sign
                                // flipped iff they are not all-equal (both a 0 and a 1 present).
                                let top_mask = mask & !((1u32 << (n - 1 - cnt)) - 1);
                                let top = x & top_mask;
                                top != 0 && top != top_mask
                            };
                            if v {
                                ccr |= CCR_V;
                            }
                        }
                        (res, ccr)
                    }
                    // ASR — arithmetic shift RIGHT by `cnt = b & 63` (the resolved count). `x` = the size-masked
                    // operand `a`; `n` = 8/16/32. Sign-EXTENDING: the vacated top bits are filled with the
                    // operand's sign bit. Value: `cnt == 0` → x unchanged; `cnt >= n` → all-sign-bits (`mask` if
                    // negative, else 0); `0 < cnt < n` → `(x >> cnt)` OR the top `cnt` sign-fill bits. C = the
                    // last bit shifted out of the OPERAND — `bit(cnt-1)` for `1 <= cnt <= n`, else 0 (THE QUIRK:
                    // `cnt > n` → C = 0, NOT the sign bit — even though the value still sign-extends); X = C.
                    // V = 0 always. ZERO COUNT (`cnt == 0`, only the dynamic `Dn` form): value unchanged, V=0,
                    // C=0, **X PRESERVED** (re-inject the live X — the shift never ran), N/Z from the unchanged
                    // operand. All Rust shifts are guarded: the `cnt == 0` branch keeps `res = x` (avoids the
                    // `mask << (n-cnt)` shift-by-`n`); the sign-fill runs only for `0 < cnt < n` (`n - cnt ∈
                    // 1..n-1`); `x >> cnt` runs only for `0 < cnt < n`; `x >> (cnt-1)` runs only for `1 <= cnt
                    // <= n` (`cnt - 1 ∈ 0..n-1`).
                    AluOp::Asr => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let neg = x & signbit != 0;
                        let res = if cnt == 0 {
                            x
                        } else if cnt >= n {
                            if neg {
                                mask
                            } else {
                                0
                            }
                        } else {
                            (x >> cnt) | (if neg { (mask << (n - cnt)) & mask } else { 0 })
                        };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        if cnt == 0 {
                            // Zero count: V=0, C=0, X PRESERVED (the shift never ran — re-inject the live X).
                            ccr |= regs.sr & CCR_X;
                        } else {
                            // C = the last bit shifted out of the operand — bit(cnt-1) for 1<=cnt<=n, else 0
                            // (THE QUIRK: cnt>n → C=0, NOT the sign bit). X = C. V = 0 always (never set).
                            let c = if cnt <= n { (x >> (cnt - 1)) & 1 } else { 0 };
                            if c != 0 {
                                ccr |= CCR_C | CCR_X;
                            }
                        }
                        (res, ccr)
                    }
                    // LSL — logical shift LEFT by `cnt = b & 63` (the resolved count). `x` = the size-masked
                    // operand `a`; `n` = 8/16/32. IDENTICAL to ASL's value and carry — the SOLE difference is
                    // **V is forced to 0** (a logical shift never tracks the sign change; only ASL owns V).
                    // Value `res = (x << cnt) & mask` when `cnt < n`, else 0. C = the last bit shifted out of
                    // the operand (`bit(n-cnt)` for `1 <= cnt <= n`, else 0); X = C. V = 0 always. ZERO COUNT
                    // (`cnt == 0`, only the dynamic `Dn` form): value unchanged, V=0, C=0, **X PRESERVED**
                    // (re-inject the live X — the shift never ran), N/Z from the unchanged operand. All Rust
                    // shifts are guarded: `res` uses `cnt < n`; `x >> (n-cnt)` runs only for `1 <= cnt <= n`
                    // (`n - cnt ∈ 0..n-1`).
                    AluOp::Lsl => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let res = if cnt < n { (x << cnt) & mask } else { 0 };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        if cnt == 0 {
                            // Zero count: V=0, C=0, X PRESERVED (the shift never ran — re-inject the live X).
                            ccr |= regs.sr & CCR_X;
                        } else {
                            // C = the last bit shifted out of the operand (0 once `cnt > n`); X = C. V = 0
                            // ALWAYS (the only difference from ASL — LSL never computes the sign-changed V).
                            let c = if cnt <= n { (x >> (n - cnt)) & 1 } else { 0 };
                            if c != 0 {
                                ccr |= CCR_C | CCR_X;
                            }
                        }
                        (res, ccr)
                    }
                    // LSR — logical shift RIGHT by `cnt = b & 63` (the resolved count). `x` = the size-masked
                    // operand `a`; `n` = 8/16/32. ZERO-FILL: the vacated top bits are 0 (contrast ASR, which
                    // sign-extends). Value `res = x >> cnt` when `cnt < n`, else 0 (an over-shift clears the
                    // register). C = the last bit shifted out of the operand — `bit(cnt-1)` for `1 <= cnt <= n`,
                    // else 0 (same form as ASR's carry; with no sign, `cnt > n` → 0 is natural); X = C. V = 0
                    // always. N = msb(res) — always 0 for any `cnt >= 1` (the msb is zero-filled). ZERO COUNT
                    // (`cnt == 0`, only the dynamic `Dn` form): value unchanged, V=0, C=0, **X PRESERVED**
                    // (re-inject the live X — the shift never ran), N/Z from the unchanged operand (so N CAN be
                    // 1 here — NOT forced to 0). All Rust shifts are guarded: `res` uses `cnt < n`; `x >>
                    // (cnt-1)` runs only for `1 <= cnt <= n` (`cnt - 1 ∈ 0..n-1`).
                    AluOp::Lsr => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let res = if cnt < n { x >> cnt } else { 0 };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        if cnt == 0 {
                            // Zero count: V=0, C=0, X PRESERVED (the shift never ran — re-inject the live X).
                            ccr |= regs.sr & CCR_X;
                        } else {
                            // C = the last bit shifted out of the operand — bit(cnt-1) for 1<=cnt<=n, else 0
                            // (cnt>n → C=0 naturally — zero-fill has nothing left to shift out); X = C. V = 0.
                            let c = if cnt <= n { (x >> (cnt - 1)) & 1 } else { 0 };
                            if c != 0 {
                                ccr |= CCR_C | CCR_X;
                            }
                        }
                        (res, ccr)
                    }
                    // ROL — rotate LEFT by `cnt = b & 63` (the resolved count). `x` = the size-masked operand
                    // `a`; `n` = 8/16/32. A plain bit-rotate that does NOT pass through X (contrast ROXL, which
                    // threads X). `r = cnt % n`; value `res = x` when `cnt == 0 || r == 0` (a whole-register
                    // rotation leaves the value unchanged), else `((x << r) | (x >> (n - r))) & mask`. C = the
                    // last bit rotated out — `(x >> ((n - (cnt % n)) % n)) & 1` for `cnt != 0`, else 0 (a zero
                    // count is the ONLY way ROL clears C — a nonzero multiple of n with `r == 0` still takes C
                    // from the formula). **X is PRESERVED** (ROL/ROR never touch X — re-inject the live X,
                    // NEVER set X = C). V = 0 always. N = msb(res), Z = (res == 0). Every Rust shift is guarded:
                    // the `x >> (n - r)` term runs only for `r != 0` (so `n - r ∈ 1..n-1`); the C-shift exponent
                    // `(n - (cnt % n)) % n` is in `0..n-1`.
                    AluOp::Rol => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let r = cnt % n;
                        let res = if cnt == 0 || r == 0 {
                            x
                        } else {
                            ((x << r) | (x >> (n - r))) & mask
                        };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        // C: the last bit rotated out — bit `(n - (cnt % n)) % n` of the operand — for any
                        // nonzero count (incl. a nonzero multiple of n); a zero count clears C. V = 0.
                        let c = if cnt == 0 {
                            0
                        } else {
                            (x >> ((n - (cnt % n)) % n)) & 1
                        };
                        if c != 0 {
                            ccr |= CCR_C;
                        }
                        // X is PRESERVED (ROL/ROR never touch X — re-inject the live X, NEVER set X = C).
                        ccr |= regs.sr & CCR_X;
                        (res, ccr)
                    }
                    // ROR — rotate RIGHT by `cnt = b & 63` (the resolved count). ROL's right-direction twin.
                    // `x` = the size-masked operand `a`; `n` = 8/16/32. A plain bit-rotate that does NOT pass
                    // through X (contrast ROXR, which threads X — S7). `r = cnt % n`; value `res = x` when
                    // `cnt == 0 || r == 0` (a whole-register rotation leaves the value unchanged), else
                    // `((x >> r) | (x << (n - r))) & mask`. C = the last bit rotated out — `(x >> ((cnt - 1) %
                    // n)) & 1` for `cnt != 0`, else 0 (a zero count is the ONLY way ROR clears C — a nonzero
                    // multiple of n with `r == 0` still takes C from the formula). **X is PRESERVED** (ROL/ROR
                    // never touch X — re-inject the live X, NEVER set X = C). V = 0 always. N = msb(res), Z =
                    // (res == 0). Every Rust shift is guarded: the `x << (n - r)` term runs only for `r != 0`
                    // (so `n - r ∈ 1..n-1`); the C-shift exponent `(cnt - 1) % n` is in `0..n-1` (cnt >= 1 in
                    // that branch).
                    AluOp::Ror => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let r = cnt % n;
                        let res = if cnt == 0 || r == 0 {
                            x
                        } else {
                            ((x >> r) | (x << (n - r))) & mask
                        };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        // C: the last bit rotated out — bit `(cnt - 1) % n` of the operand — for any nonzero
                        // count (incl. a nonzero multiple of n); a zero count clears C. V = 0.
                        let c = if cnt == 0 {
                            0
                        } else {
                            (x >> ((cnt - 1) % n)) & 1
                        };
                        if c != 0 {
                            ccr |= CCR_C;
                        }
                        // X is PRESERVED (ROL/ROR never touch X — re-inject the live X, NEVER set X = C).
                        ccr |= regs.sr & CCR_X;
                        (res, ccr)
                    }
                    // ROXL — rotate LEFT THROUGH X by `cnt = b & 63` (the resolved count). The FIRST X-threading
                    // rotate: treat the `X:operand` pair as an `(n+1)`-bit register (X above the msb) and rotate
                    // it LEFT by `cnt % (n+1)`; the bit ejected into X is BOTH the new X and C, so the result
                    // depends on the INCOMING X (unlike ROL/ROR, which leave X untouched, or ASL/ASR/LSL/LSR,
                    // which set X = C from the value). `x` = the size-masked operand `a`; `n` = 8/16/32; `xin =
                    // (sr >> 4) & 1`. ZERO COUNT (cnt == 0): value unchanged, C = X = xin (the INCOMING X — NOT
                    // 0), X UNCHANGED. Else: `per = n + 1`, `eff = cnt % per`; `comb = (xin << n) | x` in `per`
                    // bits (a `u64` so the `.l` 33-bit case does not overflow `u32`), rotated left by `eff`
                    // (guarded — `comb >> (per - eff)` is computed only for `eff != 0`, so `per - eff ∈ 1..per`);
                    // `res = (comb & mask) as u32`; C = X = `(comb >> n) & 1`. V = 0 always. N = msb(res), Z =
                    // (res == 0).
                    AluOp::Roxl => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let xin = (u32::from(regs.sr) >> 4) & 1;
                        let (res, c) = if cnt == 0 {
                            // Zero count: value unchanged, C = X (the incoming X), X unchanged.
                            (x, xin)
                        } else {
                            let per = n + 1;
                            let eff = cnt % per;
                            // The (n+1)-bit register: X above the msb. Use u64 so the .l (33-bit) case fits.
                            let combined = (u64::from(xin) << n) | u64::from(x);
                            let permask = (1u64 << per) - 1;
                            let rotated = if eff == 0 {
                                combined
                            } else {
                                ((combined << eff) | (combined >> (per - eff))) & permask
                            };
                            let res = (rotated & u64::from(mask)) as u32;
                            let c = ((rotated >> n) & 1) as u32;
                            (res, c)
                        };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        // X = C (the bit ejected into X) — both set together. V = 0.
                        if c != 0 {
                            ccr |= CCR_C | CCR_X;
                        }
                        (res, ccr)
                    }
                    // ROXR — rotate RIGHT THROUGH X by `cnt = b & 63` (the resolved count). ROXL's right-direction
                    // twin (S7, the FINAL shift/rotate): treat the `X:operand` pair as an `(n+1)`-bit register (X
                    // above the msb) and rotate it RIGHT by `cnt % (n+1)`; the bit ejected into X is BOTH the new X
                    // and C, so the result depends on the INCOMING X (unlike ROL/ROR, which leave X untouched, or
                    // ASL/ASR/LSL/LSR, which set X = C from the value). `x` = the size-masked operand `a`; `n` =
                    // 8/16/32; `xin = (sr >> 4) & 1`. ZERO COUNT (cnt == 0): value unchanged, C = X = xin (the
                    // INCOMING X — NOT 0), X UNCHANGED. Else: `per = n + 1`, `eff = cnt % per`; `comb = (xin << n)
                    // | x` in `per` bits (a `u64` so the `.l` 33-bit case does not overflow `u32`), rotated RIGHT
                    // by `eff` (guarded — `comb << (per - eff)` is computed only for `eff != 0`, so `per - eff ∈
                    // 1..per`); `res = (comb & mask) as u32`; C = X = `(comb >> n) & 1`. V = 0 always. N =
                    // msb(res), Z = (res == 0).
                    AluOp::Roxr => {
                        let (mask, signbit, n) = match size {
                            Size::Byte => (0xFFu32, 0x80u32, 8u32),
                            Size::Word => (0xFFFF, 0x8000, 16),
                            Size::Long => (0xFFFF_FFFF, 0x8000_0000, 32),
                        };
                        let x = lhs & mask;
                        let cnt = rhs & 63;
                        let xin = (u32::from(regs.sr) >> 4) & 1;
                        let (res, c) = if cnt == 0 {
                            // Zero count: value unchanged, C = X (the incoming X), X unchanged.
                            (x, xin)
                        } else {
                            let per = n + 1;
                            let eff = cnt % per;
                            // The (n+1)-bit register: X above the msb. Use u64 so the .l (33-bit) case fits.
                            let combined = (u64::from(xin) << n) | u64::from(x);
                            let permask = (1u64 << per) - 1;
                            let rotated = if eff == 0 {
                                combined
                            } else {
                                ((combined >> eff) | (combined << (per - eff))) & permask
                            };
                            let res = (rotated & u64::from(mask)) as u32;
                            let c = ((rotated >> n) & 1) as u32;
                            (res, c)
                        };
                        let mut ccr = 0u16;
                        if res & signbit != 0 {
                            ccr |= CCR_N;
                        }
                        if res == 0 {
                            ccr |= CCR_Z;
                        }
                        // X = C (the bit ejected into X) — both set together. V = 0.
                        if c != 0 {
                            ccr |= CCR_C | CCR_X;
                        }
                        (res, ccr)
                    }
                    AluOp::Add | AluOp::Sub => match size {
                        Size::Word => {
                            let (r, ccr) = match op {
                                AluOp::Add => add_w(lhs as u16, rhs as u16),
                                _ => sub_w(lhs as u16, rhs as u16),
                            };
                            (r as u32, ccr)
                        }
                        Size::Byte => {
                            let (r, ccr) = match op {
                                AluOp::Add => add_b(lhs as u8, rhs as u8),
                                _ => sub_b(lhs as u8, rhs as u8),
                            };
                            (r as u32, ccr)
                        }
                        Size::Long => match op {
                            AluOp::Add => add_l(lhs, rhs),
                            _ => sub_l(lhs, rhs),
                        },
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
                    // An is only ever written by the no-flag early-return ops (MoveA / ADDA / SUBA, handled
                    // above), never by Add/Sub/Move/Cmp/Cmpa (which reach this flag write-back).
                    Dest::AddrReg(_) => unreachable!("AddrReg dest is MoveA/ADDA/SUBA-only"),
                    // Flag-only (CMP family): the CCR is already set above; nothing is written back. The
                    // `result` is the discarded subtraction value.
                    Dest::None => {}
                }
                0
            }
            MicroOp::Prefetch => {
                // Address-error abort (E3): a program fetch of an ODD instruction word (a taken
                // branch / jump / RTS-RTR-RTE return whose target is odd) never reaches the bus. The
                // faulting address is `pc + 4` (the queue refill address); after a taken branch's `SetPc`
                // left `pc = target − 4`, that is exactly the odd `target`, and `regs.pc` (= target − 4) is
                // the stacked PC. low5 = read(0x10) | program(0x08) | fc6 = 0x1E.
                let fetch_addr = regs.pc.wrapping_add(4);
                if fetch_addr & 1 != 0 {
                    let low5 = 0x10 | 0x08 | regs.fc(true) as u16;
                    return self.install_address_error(regs, fetch_addr, low5);
                }
                let (refill, wait) = bus.read16(fetch_addr, regs.fc(true));
                regs.prefetch[0] = regs.prefetch[1];
                regs.prefetch[1] = refill;
                regs.pc = regs.pc.wrapping_add(2);
                4 + wait
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
                // FIXED 3-way wrapping_add — no per-mode branch. The builder selects the legs. The EA is the
                // FULL 32-bit internal address — **NOT** 24-bit-masked here: the 68000 keeps the address
                // register file at 32 bits and only the external bus drops the top 8 pins, so masking belongs
                // at the bus access (`Bus68k` masks `read16`/`write16`/`read8`/`write8`), not in the address
                // arithmetic. Pinned by the address-error abort (E3): the group-0 frame stacks the **full
                // 32-bit** faulting address (`d06c` stacks `0xAB091E2D`, `d8b9` stacks `0x956FE889`), which the
                // 24-bit mask would have destroyed. The bus access via this EA still hits the masked cell (the
                // bus masks), so every prior family's transaction stream is byte-identical.
                let ea = self
                    .resolve(base, regs)
                    .wrapping_add(self.resolve(index, regs))
                    .wrapping_add(self.resolve(disp, regs));
                self.scratch[dst as usize] = ea;
                0
            }
            MicroOp::Combine32 { hi, lo, dst } => {
                // Assemble the 32-bit long value — NO mask (this is a value, not an address).
                let value = (self.scratch[hi as usize] << 16) | self.resolve(lo, regs);
                self.scratch[dst as usize] = value;
                0
            }
            MicroOp::SetPc { value } => {
                // pc = target - 4; the two Prefetch ops that follow reload the queue at `target`. NO mask.
                regs.pc = self.resolve(value, regs).wrapping_sub(4);
                0
            }
            MicroOp::TargetCalc {
                base,
                index,
                disp,
                dst,
            } => {
                // The UNMASKED 3-way add — a branch target / pushed PC is the full 32-bit value (no ADDR_MASK).
                let target = self
                    .resolve(base, regs)
                    .wrapping_add(self.resolve(index, regs))
                    .wrapping_add(self.resolve(disp, regs));
                self.scratch[dst as usize] = target;
                0
            }
            MicroOp::DecrementDnWord { reg } => {
                // Dn low word −= 1 (high word preserved, NO flags); 0 wraps to 0xFFFF without a borrow into
                // the high word — the `DBcc` loop counter, decoded at instruction start to pick the branch.
                let d = regs.d[reg as usize];
                regs.d[reg as usize] = (d & 0xFFFF_0000) | (d.wrapping_sub(1) & 0xFFFF);
                0
            }
            MicroOp::LoadCcr { value } => {
                // RTR's CCR pop: low 5 bits (X/N/Z/V/C) into the CCR, SR system byte preserved; bits 7-5 of
                // the CCR read as 0 (mask 0x1F, pinned to the RTR data). NO bus, 0 cycles.
                let v = self.resolve(value, regs) as u16;
                regs.sr = (regs.sr & 0xFF00) | (v & 0x1F);
                0
            }
            MicroOp::EnterException { save_sr } => {
                // Capture the live SR for the frame push, then enter supervisor (set S) and clear T. Setting S
                // routes subsequent A7 accesses to the supervisor stack via `addr_reg`'s S-bit selection.
                self.scratch[save_sr as usize] = regs.sr as u32;
                regs.sr = (regs.sr | SR_SUPERVISOR) & !SR_TRACE;
                0
            }
            MicroOp::LoadImm { value, dst } => {
                // Materialize a constant (the vector address) into scratch so a plain Read can use it.
                self.scratch[dst as usize] = value;
                0
            }
            MicroOp::SetByte { value, dst } => {
                // The no-flag conditional byte write (Scc). Into Dn's low byte preserve the upper 24 bits;
                // into a scratch slot park the byte (zero-extended) for the trailing memory Write. NO flags.
                match dst {
                    Dest::DataRegLow8(n) => {
                        regs.d[n as usize] = (regs.d[n as usize] & 0xFFFF_FF00) | (value as u32);
                    }
                    Dest::Scratch(s) => self.scratch[s as usize] = value as u32,
                    _ => unreachable!("SetByte writes only DataRegLow8 / Scratch"),
                }
                0
            }
            MicroOp::SetWord { value, dst } => {
                // The no-flag WORD write (MOVEfromSR's `EA/Dn.w = SR`). Into Dn's low word preserve the upper
                // 16 bits; into a scratch slot park the word (zero-extended) for the trailing memory Write. NO
                // flags — the SR is byte-identical before/after (the value it WRITES is SR, but it does not
                // modify SR).
                let v = self.resolve(value, regs) & 0xFFFF;
                match dst {
                    Dest::DataRegLow16(n) => {
                        regs.d[n as usize] = (regs.d[n as usize] & 0xFFFF_0000) | v;
                    }
                    Dest::Scratch(s) => self.scratch[s as usize] = v,
                    _ => unreachable!("SetWord writes only DataRegLow16 / Scratch"),
                }
                0
            }
            MicroOp::TasRmw { addr } => {
                // The atomic indivisible TAS memory RMW (ONE locked `Tas` bus cycle): read `orig`, write
                // `orig | 0x80`, log ONE Tas transaction (value = the WRITTEN byte). The flags come from the
                // READ byte `orig` — N = bit7(orig) / Z = (orig == 0), V/C cleared, X PRESERVED — while the
                // written value is `orig | 0x80` (DISTINCT). Data FC. Byte-only → never faults (one bus
                // access = one quiesce boundary). 10 cyc (read 4 + indivisible modify 2 + write 4).
                let address = self.resolve(addr, regs);
                let (orig, wait) = bus.tas(address, regs.fc(false));
                let (_r, ccr_nz) = move_flags(orig as u32, Size::Byte);
                regs.sr = (regs.sr & 0xFF00) | ccr_nz | (regs.sr & CCR_X);
                10 + wait
            }
            MicroOp::LoadSr { value } => {
                // RTE's full-SR restore: the popped value masked to the implemented bits (0xA71F). Can switch
                // S (supervisor→user) / T — so the recipe runs the +6 stack pop BEFORE this, and any later
                // Prefetch reload follows the RESTORED mode's function code. NO bus, 0 cycles.
                regs.sr = (self.resolve(value, regs) as u16) & SR_IMPLEMENTED;
                0
            }
            MicroOp::Stop => {
                // STOP #imm's halt: the SR was loaded by the preceding LoadSr; flag the Stopped transition for
                // the orchestrator to apply at completion. 4(0/0) — no bus access (Yacht L908).
                self.stop_requested = true;
                4
            }
            MicroOp::SetIntMask { level } => {
                // Raise the processor priority to the acknowledged interrupt level (§6.3.2). The frame's
                // EnterException already captured the OLD SR, so the stacked SR keeps the pre-interrupt mask.
                regs.set_int_mask(level);
                0
            }
            MicroOp::IntAck { level } => {
                // The interrupt-acknowledge (`ni`) cycle in CPU space (FC=7). Mega Drive VPA → autovector, so
                // the read value is discarded (the vector is 24 + level, generated by the recipe). The access
                // exists to show the acknowledged level on the bus. 4 cycles.
                let iack_addr = 0xFFFF_FFF1 | ((level as u32) << 1);
                let (_, wait) = bus.read16(iack_addr, 7);
                4 + wait
            }
            MicroOp::ChkTrap { dn, bound } => {
                // Signed compare Dn.w against 0 and the bound (both sign-extended from their low 16). The bound
                // is resolved BEFORE any frame install (the install seeds the saved-PC slot, which may alias the
                // bound slot for a memory/`#imm` operand — read first, write second).
                let dn_val = (regs.d[dn as usize] & 0xFFFF) as i16 as i32;
                let bound_val = (self.resolve(bound, regs) & 0xFFFF) as i16 as i32;
                let neg = dn_val < 0;
                let over = dn_val > bound_val;
                // CCR: Z=V=C cleared, X kept; N = 1 if Dn<0, 0 if Dn>bound, else preserved.
                let n_bit = if neg {
                    CCR_N
                } else if over {
                    0
                } else {
                    regs.sr & CCR_N
                };
                regs.sr = (regs.sr & 0xFF00) | (regs.sr & CCR_X) | n_bit;
                if neg || over {
                    // Out of bounds → take the CHK exception (vector 6). The leading idle is n4 when Dn>bound,
                    // else n6 (the two predicates differ — `over` picks the idle, `neg` already picked N).
                    let idle = if over { 4 } else { 6 };
                    return self.install_chk_trap(regs, idle);
                }
                0
            }
            MicroOp::SrLogic { op, value } => {
                // The privileged `*toSR` write-back: apply the bitwise op against the immediate, then mask to
                // the implemented SR bits (0xA71F). Can clear S/T (And/Eor) — the recipe runs the two
                // re-prefetch reads AFTER this, so they follow the NEW mode's function code. NO bus, 0 cycles.
                let v = self.resolve(value, regs) as u16;
                let combined = match op {
                    LogicOp::And => regs.sr & v,
                    LogicOp::Or => regs.sr | v,
                    LogicOp::Eor => regs.sr ^ v,
                };
                regs.sr = combined & SR_IMPLEMENTED;
                0
            }
            MicroOp::CcrLogic { op, value } => {
                // The `*toCCR` write-back: apply the bitwise op against the immediate, then keep ONLY the CCR
                // low-5 bits — the SR system byte (0xFF00: T | S | I) is PRESERVED, so S/T/I never change and
                // there is no FC switch. NO bus, 0 cycles.
                let v = self.resolve(value, regs) as u16;
                let combined = match op {
                    LogicOp::And => regs.sr & v,
                    LogicOp::Or => regs.sr | v,
                    LogicOp::Eor => regs.sr ^ v,
                };
                regs.sr = (regs.sr & 0xFF00) | (combined & 0x1F);
                0
            }
            MicroOp::ExgRegs { opmode, rx, ry } => {
                // EXG's register exchange — swap the two whole 32-bit registers per the opmode form, NO flags
                // (SR untouched). A7-aware via `addr_reg`/`addr_reg_set` so a reg-7 address leg hits the active
                // stack pointer (ssp/usp per S). NO bus, 0 cycles (the trailing Internal(2) books the len-6 cost).
                let (rx, ry) = (rx as usize, ry as usize);
                match opmode {
                    0x08 => {
                        // EXG Dx,Dy — swap two data registers.
                        regs.d.swap(rx, ry);
                    }
                    0x09 => {
                        // EXG Ax,Ay — swap two address registers (A7-aware).
                        let ax = regs.addr_reg(rx);
                        let ay = regs.addr_reg(ry);
                        regs.addr_reg_set(rx, ay);
                        regs.addr_reg_set(ry, ax);
                    }
                    _ => {
                        // EXG Dx,Ay (opmode 0x11) — swap a data register with an address register (A7-aware).
                        let dx = regs.d[rx];
                        let ay = regs.addr_reg(ry);
                        regs.d[rx] = ay;
                        regs.addr_reg_set(ry, dx);
                    }
                }
                0
            }
            MicroOp::MoveUsp { to_usp, an } => {
                // MOVEfromUSP / MOVEtoUSP — the flag-free register↔USP transfer, NO SR change. A7-aware via
                // addr_reg/addr_reg_set so `an == 7` hits the active A7 (ssp in supervisor): from-USP A7 sets
                // ssp = usp, to-USP A7 sets usp = ssp. NO bus, 0 cycles (the trailing Prefetch books the len-4).
                let an = an as usize;
                if to_usp {
                    regs.usp = regs.addr_reg(an);
                } else {
                    regs.addr_reg_set(an, regs.usp);
                }
                0
            }
            MicroOp::MovemStore {
                reg,
                size,
                addr_slot,
                predec,
            } => {
                // MOVEM register→memory per-register store. The register value (word: low 16; long: hi then lo
                // word) is written at the running address in `scratch[addr_slot]` — NEVER at An, so an
                // An-in-list `-(An)` store writes the INITIAL An (An is set by the recipe's trailing MoveA).
                // `predec` selects the address discipline: forward (control) writes then advances; `-(An)`
                // decrements FIRST then writes (the predecrement-before-store order, over the reversed list).
                let step = match size {
                    Size::Word => 2u32,
                    Size::Long => 4,
                    Size::Byte => unreachable!("MOVEM is word/long only"),
                };
                // The full 32-bit register value (data via regs.d; address via addr_reg, A7-aware for reg 15).
                let value = if reg < 8 {
                    regs.d[reg as usize]
                } else {
                    regs.addr_reg((reg - 8) as usize)
                };
                let fc = regs.fc(false); // MOVEM is always Data space
                let mut wait = 0u32;
                if predec {
                    // -(An): decrement the running address by `size` FIRST, then write at it.
                    let addr = self.scratch[addr_slot as usize].wrapping_sub(step);
                    self.scratch[addr_slot as usize] = addr;
                    match size {
                        Size::Word => {
                            // Odd write address → group-0 address error (E3), low5 = 0x05 (data write).
                            if addr & 1 != 0 {
                                return self.install_address_error(regs, addr, 0x05);
                            }
                            wait += bus.write16(addr, fc, value as u16);
                        }
                        Size::Long => {
                            // -(An) LONG store word order (REVERSED, like the MOVE.l `-(An)` "low half first"
                            // precedent): the LOW word is written FIRST at `addr+2`, then the HIGH word at
                            // `addr`. Both still land big-endian (hi @ addr, lo @ addr+2) — only the bus ACCESS
                            // ORDER is reversed. Pinned against the vendored MOVEM.l `-(An)` stream. Since the
                            // FIRST bus access is at `addr+2`, an odd base faults THERE (the reported access
                            // address = `addr+2 = An−2`, one word above the decremented base) — the running
                            // address is a scratch slot, so the register file is unchanged on the abort.
                            let lo_addr = addr.wrapping_add(2);
                            if lo_addr & 1 != 0 {
                                return self.install_address_error(regs, lo_addr, 0x05);
                            }
                            wait += bus.write16(lo_addr, fc, value as u16);
                            wait += bus.write16(addr, fc, (value >> 16) as u16);
                        }
                        Size::Byte => unreachable!(),
                    }
                } else {
                    // Forward (control): write at the running address, then advance it.
                    let addr = self.scratch[addr_slot as usize];
                    if addr & 1 != 0 {
                        return self.install_address_error(regs, addr, 0x05);
                    }
                    match size {
                        Size::Word => wait += bus.write16(addr, fc, value as u16),
                        Size::Long => {
                            wait += bus.write16(addr, fc, (value >> 16) as u16);
                            wait += bus.write16(addr.wrapping_add(2), fc, value as u16);
                        }
                        Size::Byte => unreachable!(),
                    }
                    self.scratch[addr_slot as usize] = addr.wrapping_add(step);
                }
                // 4 cycles per word access (word = 1, long = 2), plus any bus wait cycles.
                wait + match size {
                    Size::Word => 4,
                    Size::Long => 8,
                    Size::Byte => unreachable!(),
                }
            }
            MicroOp::MovemLoad {
                reg,
                size,
                addr_slot,
            } => {
                // MOVEM memory→register per-register load. Reads the register's word(s) from the running
                // address in `scratch[addr_slot]` and writes the FULL 32-bit register (a WORD load SIGN-EXTENDS
                // to 32 bits), then advances the running address. The running address is ALWAYS the scratch slot
                // (never An) — so an An-in-list `(An)+` load does not corrupt the pointer; the recipe's trailing
                // MoveA writes the final An = base + n·size.
                let step = match size {
                    Size::Word => 2u32,
                    Size::Long => 4,
                    Size::Byte => unreachable!("MOVEM is word/long only"),
                };
                let fc = regs.fc(false); // Data space
                let addr = self.scratch[addr_slot as usize];
                if addr & 1 != 0 {
                    // Odd read address → group-0 address error (E3), low5 = 0x15 (data read). For (An)+ the
                    // abort-commit AdjustAddr already bumped An (one WORD), so the aborted (An)+ leaves An += 2.
                    return self.install_address_error(regs, addr, 0x15);
                }
                let mut wait = 0u32;
                let value = match size {
                    Size::Word => {
                        // Sign-extend the loaded word to 32 bits.
                        let (v, w) = bus.read16(addr, fc);
                        wait += w;
                        sign_extend16(v)
                    }
                    Size::Long => {
                        let (hv, hw) = bus.read16(addr, fc);
                        let (lv, lw) = bus.read16(addr.wrapping_add(2), fc);
                        wait += hw + lw;
                        ((hv as u32) << 16) | lv as u32
                    }
                    Size::Byte => unreachable!(),
                };
                if reg < 8 {
                    regs.d[reg as usize] = value;
                } else {
                    regs.addr_reg_set((reg - 8) as usize, value);
                }
                self.scratch[addr_slot as usize] = addr.wrapping_add(step);
                wait + match size {
                    Size::Word => 4,
                    Size::Long => 8,
                    Size::Byte => unreachable!(),
                }
            }
            MicroOp::MovepStore {
                dn,
                shift,
                addr_slot,
            } => {
                // MOVEP register→memory per-byte store. Write one byte of the DATA register Dn at the running
                // alternating address, then step the address by 2. Byte access → NEVER an alignment fault.
                let addr = self.scratch[addr_slot as usize];
                let fc = regs.fc(false); // MOVEP is always Data space
                let byte = ((regs.d[dn as usize] >> shift) & 0xFF) as u8;
                let wait = bus.write8(addr, fc, byte);
                self.scratch[addr_slot as usize] = addr.wrapping_add(2);
                4 + wait
            }
            MicroOp::MovepLoad {
                dn,
                shift,
                addr_slot,
            } => {
                // MOVEP memory→register per-byte load. Read one byte at the running alternating address and merge
                // it into Dn (mask-then-or → the word high-word-preserve / long full-32 overwrite fall out), then
                // step the address by 2. Byte access → NEVER an alignment fault.
                let addr = self.scratch[addr_slot as usize];
                let fc = regs.fc(false); // Data space
                let (b, wait) = bus.read8(addr, fc);
                let byte = b as u32;
                regs.d[dn as usize] = (regs.d[dn as usize] & !(0xFFu32 << shift)) | (byte << shift);
                self.scratch[addr_slot as usize] = addr.wrapping_add(2);
                4 + wait
            }
        };
        self.step += 1;
        self.cycles += cycles;
        cycles
    }
}

/// The processor's high-level execution state (Exodus's serialized set, minus a separate "Exception" —
/// in-flight recipes already cover exception entry). Plain serialized data, so snapshot/restore of a
/// stopped or halted CPU is free.
///
/// The nominal idle a `Stopped` CPU consumes per `begin_next` poll when nothing wakes it — purely a
/// `run_until` progress device (Push C), NOT a pinned timing. Yacht has no STOP-wait entry, so the real
/// stopped-idle cadence is docket residue; 4 (one bus-cycle granule) keeps `run_until` advancing.
const STOPPED_IDLE_SLICE: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum CpuState {
    /// Executing instructions normally.
    Normal,
    /// `STOP #imm` halted fetching; wakes on an unmasked interrupt or reset (wake lands in Push A / A4).
    Stopped,
    /// The double-fault terminal state (a group-0 fault while stacking a group-0 frame); only reset exits.
    Halted,
}

/// One 68000, driven by the micro-op framework. Between instructions `inflight` is `None`; while quiesced
/// mid-instruction it holds the resumable cursor. The whole CPU is bincode-serializable, so a debugger can
/// stop at a bus-access boundary, snapshot, restore, and resume.
#[derive(Clone, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Cpu68000 {
    pub regs: Registers,
    inflight: Option<MicroState>,
    /// The high-level processor state. `Normal` unless `STOP` (→ `Stopped`) or a double fault
    /// (→ `Halted`) has fired. Serialized, so a stopped/halted snapshot round-trips.
    state: CpuState,
    /// Latched when an instruction completes with T set at its start and did not suppress the trace
    /// (M68000UM §6.3.8): the NEXT [`Cpu68000::step`] services the vector-9 trace exception before decoding.
    /// Serialized, so a snapshot taken between the traced instruction and its trace round-trips. This is the
    /// first of `begin_next`'s async event latches (the interrupt latch joins it in A4).
    trace_pending: bool,
    /// The externally-asserted interrupt request level (0 = none, 1–7). Latched via [`Cpu68000::set_ipl`]
    /// (the System drives it in Push C; tests set it directly). `begin_next` takes the interrupt when
    /// `ipl > SR` interrupt mask (M68000UM §6.3.2). Serialized so a snapshot round-trips the pending request.
    ipl: u8,
    /// Latched external reset request ([`Cpu68000::assert_reset`]). `begin_next` services it FIRST — reset is
    /// group 0, the highest priority — running the power-on reset sequence and returning to `Normal` from any
    /// state (it is the only exit from `Stopped`/`Halted`). Serialized so a snapshot round-trips the request.
    reset_pending: bool,
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
    /// Power on with the given register file, no instruction in flight, in the Normal state.
    pub fn new(regs: Registers) -> Self {
        Self {
            regs,
            inflight: None,
            state: CpuState::Normal,
            trace_pending: false,
            ipl: 0,
            reset_pending: false,
        }
    }

    /// Latch the external interrupt request level (0 = none, 1–7). `begin_next` services it when it exceeds
    /// the SR interrupt mask (M68000UM §6.3.2). The System drives this in Push C.
    pub fn set_ipl(&mut self, level: u8) {
        self.ipl = level;
    }

    /// Assert an external reset. `begin_next` runs the power-on reset sequence on the next
    /// [`Cpu68000::step`] (group 0, above everything), returning to `Normal` from any state — the only exit
    /// from `Stopped`/`Halted`. The System drives this from `System::reset` in Push C.
    pub fn assert_reset(&mut self) {
        self.reset_pending = true;
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
        let Cpu68000 {
            regs,
            inflight,
            state: cpu_state,
            // step_micro_op is the mid-instruction driver; trace/interrupt/reset are begin_next concerns.
            trace_pending: _,
            ipl: _,
            reset_pending: _,
        } = self;
        let recipe = inflight.as_mut().expect("no instruction in flight");
        recipe.exec_one(regs, bus);
        if recipe.is_done() {
            let total = recipe.cycles;
            // A completed STOP recipe leaves the processor Stopped (both drivers honor this).
            if recipe.requests_stop() {
                *cpu_state = CpuState::Stopped;
            }
            *inflight = None;
            Step::Done(total)
        } else {
            Step::Continue
        }
    }

    /// **The orchestrator fast path** (D1): decide the next unit of work, then run it to completion.
    /// Returns the CPU cycles consumed. A thin superset of [`Cpu68000::run_instruction`] — the SST harness
    /// keeps calling `run_instruction`/`step_micro_op` directly, so this path grows without touching the gate.
    ///
    /// For now (A1) the only work is "decode `prefetch[0]` and run it", plus applying a completed `STOP`'s
    /// transition to [`CpuState::Stopped`]. Async events (trace, interrupt) and the `Stopped`/`Halted` wake
    /// slot into a `begin_next` decision point as A3/A4 land.
    pub fn step(&mut self, bus: &mut impl Bus68k) -> u32 {
        // begin_next priority dispatch. RESET is group 0 — the highest priority, above everything, and the
        // ONLY exit from `Stopped`/`Halted` (M68000UM §6.3.1). Serviced FIRST: run the power-on reset
        // sequence and return to `Normal` from any state.
        if self.reset_pending {
            self.reset_pending = false;
            self.state = CpuState::Normal;
            let mut recipe = crate::m68000::decode::reset_exception_recipe();
            return recipe.run_to_completion(&mut self.regs, bus);
        }
        // Trace (a boundary event pended by the PREVIOUS instruction) outranks decoding the next instruction
        // (M68000UM §6.2.3 group-1 order; the interrupt arm joins here in A4). A `Stopped` CPU (after `STOP`)
        // resumes on an interrupt whose level exceeds the mask (§6.3.2) — reset-wake is handled above. The
        // wake advances past the 2-word `STOP` (pc += 4); there is NO separate wake refill/bus activity — the
        // interrupt arm below reloads the queue via its own vector fetch, so the only pinned stream is the
        // interrupt's 44(5/3) (Yacht L1549; any wake-latency idle is docket residue). On wake we fall through
        // to service that interrupt (stacked PC = the post-STOP addr).
        if self.state == CpuState::Stopped {
            if self.ipl > self.regs.int_mask() {
                self.state = CpuState::Normal;
                self.regs.pc = self.regs.pc.wrapping_add(4); // advance past the 2-word STOP
            } else {
                // Remain stopped, consuming a nominal idle slice so `run_until` (Push C) makes progress. This
                // per-poll idle cost is NOT pinned (no Yacht STOP-wait entry) — a progress device, not timing.
                return STOPPED_IDLE_SLICE;
            }
        }
        if self.trace_pending {
            self.trace_pending = false;
            let mut recipe = crate::m68000::decode::trace_exception_recipe();
            return recipe.run_to_completion(&mut self.regs, bus);
        }
        // Interrupt (a boundary event): taken when the request level STRICTLY exceeds the SR mask (§6.3.2).
        // Ranked below trace, above decode. (Level-7 nonmaskable / the `ipl == mask == 7` edge is docketed.)
        if self.ipl > self.regs.int_mask() {
            let level = self.ipl;
            let mut recipe = crate::m68000::decode::interrupt_exception_recipe(level);
            return recipe.run_to_completion(&mut self.regs, bus);
        }
        // Latch T at the START of the instruction (before it can change SR) — §6.3.8.
        let trace_armed = self.regs.sr & SR_TRACE != 0;
        let mut recipe = crate::m68000::decode::decode(&self.regs);
        let suppresses_trace = recipe.suppresses_trace();
        let cycles = recipe.run_to_completion(&mut self.regs, bus);
        if recipe.requests_stop() {
            self.state = CpuState::Stopped;
        }
        // Pend the trace for the next begin_next, unless the instruction did not execute / was aborted
        // (illegal/privilege/line-A/F/fault). Executed-then-trapping instructions do NOT suppress, so their
        // trace correctly sequences after the trap.
        if trace_armed && !suppresses_trace {
            self.trace_pending = true;
        }
        cycles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m68000::bus68k::{FlatBus, Transaction, TxKind};
    use crate::m68000::registers::{Registers, SR_INT_MASK, SR_SUPERVISOR};

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

    /// A bus that reports a fixed number of wait cycles on every access (VDP-port waits / bus contention).
    /// `FlatBus` returns 0; this proves `exec_one` folds a non-zero wait into the instruction's cycle count.
    struct WaitBus {
        wait: u32,
    }
    impl Bus68k for WaitBus {
        fn read16(&mut self, _addr: u32, _fc: u8) -> (u16, u32) {
            (0, self.wait)
        }
        fn write16(&mut self, _addr: u32, _fc: u8, _value: u16) -> u32 {
            self.wait
        }
        fn read8(&mut self, _addr: u32, _fc: u8) -> (u8, u32) {
            (0, self.wait)
        }
        fn write8(&mut self, _addr: u32, _fc: u8, _value: u8) -> u32 {
            self.wait
        }
        fn tas(&mut self, _addr: u32, _fc: u8) -> (u8, u32) {
            (0, self.wait)
        }
    }

    #[test]
    fn bus_wait_cycles_are_added_to_the_access_cost_in_exec_one() {
        let mut regs = regs();
        // A word read: base 4 + the bus's 3 wait cycles = 7. (FlatBus would return 0 wait → 4.)
        let mut st = MicroState::from_ops(&[MicroOp::Read {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Word,
            dst: 1,
        }]);
        st.scratch[0] = 0x1000;
        let cycles = st.exec_one(&mut regs, &mut WaitBus { wait: 3 });
        assert_eq!(cycles, 7, "a 4-cycle word read + 3 wait cycles = 7");

        // A byte write: base 4 + 2 wait = 6.
        let mut stw = MicroState::from_ops(&[MicroOp::Write {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Byte,
            value: Operand::Scratch(1),
        }]);
        stw.scratch[0] = 0x2001;
        let cycles = stw.exec_one(&mut regs, &mut WaitBus { wait: 2 });
        assert_eq!(cycles, 6, "a 4-cycle byte write + 2 wait cycles = 6");
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

    // --- Push A / A1: STOP + CpuState ------------------------------------------------------------
    // STOP #imm (0x4E72): load the immediate word into SR (masked to SR_IMPLEMENTED), then halt
    // fetching. Yacht L908: 4(0/0), no bus access. Privileged (gated at decode when in user mode).
    // Wake-on-interrupt is deferred to A4; here we only pin the SR load + the Stopped transition.

    /// A fresh CPU powers on in the Normal processor state.
    #[test]
    fn new_cpu_is_in_normal_state() {
        let cpu = Cpu68000::new(regs());
        assert_eq!(cpu.state, CpuState::Normal);
    }

    #[test]
    fn stop_loads_sr_and_enters_stopped_state() {
        let mut r = regs(); // supervisor (S=1) — STOP is privileged
        r.pc = 0x0C00;
        r.prefetch = [0x4E72, 0x2700]; // STOP #$2700 (S=1, I=7, T=0)
        let mut cpu = Cpu68000::new(r);
        let mut bus = FlatBus::new();

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4, "STOP = 4(0/0)");
        assert_eq!(
            cpu.regs.sr, 0x2700,
            "SR loaded from the immediate (masked to SR_IMPLEMENTED)"
        );
        assert_eq!(
            cpu.state,
            CpuState::Stopped,
            "CPU entered the Stopped state"
        );
        assert!(bus.log.is_empty(), "STOP performs no bus access");
    }

    #[test]
    fn stop_enters_stopped_via_step_micro_op_driver_too() {
        let mut r = regs();
        r.pc = 0x0C00;
        r.prefetch = [0x4E72, 0x2700];
        let mut cpu = Cpu68000::new(r);
        let mut bus = FlatBus::new();

        cpu.start_instruction();
        let cycles = loop {
            if let Step::Done(c) = cpu.step_micro_op(&mut bus) {
                break c;
            }
        };
        assert_eq!(cycles, 4);
        assert_eq!(cpu.regs.sr, 0x2700, "both drivers load SR identically");
        assert_eq!(cpu.state, CpuState::Stopped, "both drivers reach Stopped");
        assert!(bus.log.is_empty());
    }

    #[test]
    fn stop_quiescable_and_serializable_at_every_boundary() {
        // 2 micro-ops (LoadSr, Stop) → in-flight boundaries after 0 and 1 of them. Snapshot/restore the
        // whole CPU at each boundary, resume, and require the same final SR + Stopped state. The Stopped
        // transition is applied by the driver at completion, so it must survive a mid-STOP snapshot.
        let cfg = bincode::config::standard();
        for pause_after in 0..=1 {
            let mut r = regs();
            r.pc = 0x0C00;
            r.prefetch = [0x4E72, 0x2700];
            let mut cpu = Cpu68000::new(r);
            let mut bus = FlatBus::new();
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
                cpu2.regs.sr, 0x2700,
                "resume from boundary {pause_after} loaded SR"
            );
            assert_eq!(
                cpu2.state,
                CpuState::Stopped,
                "resume from boundary {pause_after} reached Stopped"
            );
            assert!(
                bus.log.is_empty(),
                "STOP does no bus access from boundary {pause_after}"
            );
        }
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
    fn ea_calc_sums_base_index_disp_into_scratch() {
        // EaCalc is a FIXED 3-way wrapping_add — no per-mode match, NO 24-bit mask (the bus masks at access).
        // base = A1, index = ·(Zero), disp = sign_extend16(prefetch[1]). This sum stays within 24 bits.
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
            "0xFFFFF0 + (-8) = 0xFFFFE8 (within 24 bits; EaCalc does not mask)"
        );
        assert!(bus.log.is_empty(), "EaCalc touches no bus");
    }

    #[test]
    fn ea_calc_keeps_the_full_32bit_sum_unmasked() {
        // EaCalc carries the FULL 32-bit internal address — it does NOT mask to 24 bits (the bus masks at
        // access time). base near the top of the 24-bit space + a positive disp would, with the old mask,
        // have wrapped to 0x0E; now the 25th bit survives so the address-error abort can stack it unmasked.
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
            st.scratch[0], 0x0100_000E,
            "0xFFFFFE + 0x10 = 0x0100000E, UNMASKED (the bus masks to 0x0E at access)"
        );
    }

    #[test]
    fn disp_word_resolves_sign_extended_prefetch_word_1() {
        // Operand::DispWord = sign_extend16(prefetch[1]) as u32 — a full 32-bit sign extension; EaCalc no
        // longer masks (the bus masks at access). Resolve it via a Zero+Zero+DispWord EaCalc (the abs.w shape).
        let mut regs = regs();
        regs.prefetch = [0xDA78, 0xCC1A]; // abs.w disp 0xCC1A → sign-extend → 0xFFFFCC1A
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::Zero,
            disp: Operand::DispWord,
            dst: 1,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            st.scratch[1], 0xFFFF_CC1A,
            "abs.w EA = sign_extend16(0xCC1A), UNMASKED (the bus masks to 0xFFCC1A at access)"
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

        // (pc+2) + sign_extend16(disp) = 0xC02 + (-10014) = -6940 → 0xFFFF_E4E4, UNMASKED (bus masks at access).
        assert_eq!(
            st.scratch[0], 0xFFFF_E4E4,
            "d16(PC) EA = (pc+2) + sign_extend16(disp), UNMASKED"
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

        // (0xD1CC << 16) = 0xD1CC_0000, UNMASKED (EaCalc keeps the full 32 bits; the bus masks at access).
        assert_eq!(
            st.scratch[0], 0xD1CC_0000,
            "abs.l HIGH = (prefetch[1] << 16), UNMASKED"
        );
    }

    #[test]
    fn ext_word_raw_resolves_to_prefetch_word_1_unmodified() {
        // Operand::ExtWordRaw = prefetch[1] as u32 — the abs.l LOW word capture, read from the queue
        // AFTER the interleaved Prefetch (NEVER from that prefetch's bus-return value). Combined with the
        // already-captured HIGH it forms the full 32-bit address (the bus masks to 24 bits at access).
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

        // sign_extend8(0xF0) = 0xFFFF_FFF0, UNMASKED (EaCalc keeps the full 32 bits; the bus masks at access).
        assert_eq!(
            st.scratch[0], 0xFFFF_FFF0,
            "BriefDisp8 = sign_extend8(prefetch[1] & 0xFF), UNMASKED"
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

        // sign_extend16(0xF008) = 0xFFFF_F008, UNMASKED (EaCalc keeps the full 32 bits; the bus masks).
        assert_eq!(
            st.scratch[0], 0xFFFF_F008,
            "BriefIndex (D, W) = sign_extend16(Dn low 16), UNMASKED"
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

        // sign_extend16(0x8001) = 0xFFFF_8001, UNMASKED (EaCalc keeps the full 32 bits; the bus masks).
        assert_eq!(
            st.scratch[0], 0xFFFF_8001,
            "BriefIndex (A, W) = sign_extend16(An low 16), UNMASKED"
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

    #[test]
    fn alu_move_w_sets_n_z_clears_v_c_and_preserves_x() {
        // MOVE is NOT arithmetic: it copies the value and sets N=bit15, Z=(value==0 at size), V=0, C=0, and
        // leaves X untouched. Pinned to the real SST `3490 [MOVE.w (A0),(A2)]`: source word 0x9F6D (read into
        // scratch 0) → bit15 set → N; non-zero → no Z; X was set in SR 0x2715 and must SURVIVE. CCR 0x15
        // (X|Z|C) → 0x18 (X|N): X preserved, N set, Z/V/C cleared. The value is parked in scratch 1.
        let mut regs = regs();
        regs.sr = 0x2715; // CCR = X|Z|C, supervisor
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Move,
            size: Size::Word,
            a: Operand::Scratch(0),
            b: Operand::Zero,
            dst: Dest::Scratch(1),
        }]);
        st.scratch[0] = 0x0000_9F6D;

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "Move is an internal/overlapped op — 0 cycles");
        assert_eq!(
            st.scratch[1], 0x0000_9F6D,
            "value copied to scratch, parked"
        );
        assert_eq!(regs.sr, 0x2718, "X preserved, N set, Z/V/C cleared");
        assert!(bus.log.is_empty(), "Move touches no bus");
    }

    #[test]
    fn alu_move_w_sets_z_on_zero_value_preserving_x() {
        // A zero source word → Z set, N clear, V/C clear, X preserved. With X clear in the input CCR.
        let mut regs = regs();
        regs.sr = 0x2700; // CCR clear
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Move,
            size: Size::Word,
            a: Operand::Scratch(0),
            b: Operand::Zero,
            dst: Dest::DataRegLow16(3),
        }]);
        st.scratch[0] = 0x0000_0000;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.sr, 0x2704, "Z set; N/V/C clear; X preserved (was 0)");
        assert_eq!(regs.d[3] & 0xFFFF, 0, "zero written to Dn low word");
    }

    #[test]
    fn alu_movea_w_sign_extends_word_to_full_32_and_changes_no_flags() {
        // MOVEA.w writes the full An, SIGN-EXTENDING the source word to 32 bits, and affects NO flags. A
        // source word with bit15 set (0xCB69) lands as 0xFFFFCB69 in An; the CCR is untouched. Pinned to the
        // real SST `3856 [MOVEA.w (A6),A4]`: source word 0x... → 0xFFFFxxxx, SR identical before/after.
        let mut regs = regs();
        regs.sr = 0x2715; // CCR = X|Z|C, supervisor — must survive UNCHANGED
        regs.a[4] = 0x1234_5678; // prior An contents — fully overwritten
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::MoveA,
            size: Size::Word,
            a: Operand::Scratch(0),
            b: Operand::Zero,
            dst: Dest::AddrReg(4),
        }]);
        st.scratch[0] = 0x0000_CB69; // source word, bit15 set

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "MoveA is an internal/overlapped op — 0 cycles");
        assert_eq!(
            regs.a[4], 0xFFFF_CB69,
            "source word sign-extended to the full 32-bit An"
        );
        assert_eq!(regs.sr, 0x2715, "no flags affected by MOVEA");
        assert!(bus.log.is_empty(), "MoveA touches no bus");
    }

    #[test]
    fn alu_movea_l_writes_full_32_and_changes_no_flags() {
        // MOVEA.l writes the full 32-bit source straight to An (no sign-extension needed) and affects NO
        // flags. Pinned to the real SST `2642 [MOVEA.l D2,A3]`: D2's full 32 bits land in A3.
        let mut regs = regs();
        regs.sr = 0x2708; // CCR = N — must survive UNCHANGED
        regs.a[3] = 0xDEAD_BEEF;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::MoveA,
            size: Size::Long,
            a: Operand::Scratch(0),
            b: Operand::Zero,
            dst: Dest::AddrReg(3),
        }]);
        st.scratch[0] = 0x7A8B_9CFF; // full 32-bit source

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.a[3], 0x7A8B_9CFF, "full 32-bit source written to An");
        assert_eq!(regs.sr, 0x2708, "no flags affected by MOVEA");
    }

    #[test]
    fn alu_movea_dest_routes_a7_through_the_active_stack_pointer() {
        // Dest::AddrReg(7) must write the active A7 (ssp in supervisor mode) via addr_reg_set, not a[7].
        let mut regs = regs(); // supervisor
        regs.ssp = 0x0000_0800;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::MoveA,
            size: Size::Long,
            a: Operand::Scratch(0),
            b: Operand::Zero,
            dst: Dest::AddrReg(7),
        }]);
        st.scratch[0] = 0x0012_3456;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.ssp, 0x0012_3456,
            "A7 dest hit the supervisor stack pointer"
        );
    }

    // --- F0: the branch primitive (condition_true, SetPc, TargetCalc, BranchDisp8). ---

    #[test]
    fn condition_true_evaluates_all_16_conditions_against_the_ccr() {
        // condition_true(cc, sr) reads ONLY the CCR low byte (X|N|Z|V|C). Each condition is pinned to its
        // 68000 truth table. Build SR values that isolate each flag (system byte 0x2700 supervisor, plus the
        // CCR bits under test).
        let sup = 0x2700u16;
        // T (cc 0, BRA) — always true; F (cc 1, BSR) — always false. Independent of flags.
        assert!(condition_true(0, sup), "T always true");
        assert!(
            condition_true(0, sup | 0x1F),
            "T always true (all flags set)"
        );
        assert!(!condition_true(1, sup), "F always false");
        assert!(!condition_true(1, sup | 0x1F), "F always false");
        // HI (cc 2) = !C & !Z.
        assert!(condition_true(2, sup), "HI: C=0,Z=0");
        assert!(!condition_true(2, sup | CCR_C), "HI false when C set");
        assert!(!condition_true(2, sup | CCR_Z), "HI false when Z set");
        // LS (cc 3) = C | Z.
        assert!(!condition_true(3, sup), "LS: C=0,Z=0 false");
        assert!(condition_true(3, sup | CCR_C), "LS true when C set");
        assert!(condition_true(3, sup | CCR_Z), "LS true when Z set");
        // CC/HS (cc 4) = !C; CS/LO (cc 5) = C.
        assert!(condition_true(4, sup), "CC: C=0 true");
        assert!(!condition_true(4, sup | CCR_C), "CC false when C set");
        assert!(!condition_true(5, sup), "CS: C=0 false");
        assert!(condition_true(5, sup | CCR_C), "CS true when C set");
        // NE (cc 6) = !Z; EQ (cc 7) = Z.
        assert!(condition_true(6, sup), "NE: Z=0 true");
        assert!(!condition_true(6, sup | CCR_Z), "NE false when Z set");
        assert!(!condition_true(7, sup), "EQ: Z=0 false");
        assert!(condition_true(7, sup | CCR_Z), "EQ true when Z set");
        // VC (cc 8) = !V; VS (cc 9) = V.
        assert!(condition_true(8, sup), "VC: V=0 true");
        assert!(!condition_true(8, sup | CCR_V), "VC false when V set");
        assert!(!condition_true(9, sup), "VS: V=0 false");
        assert!(condition_true(9, sup | CCR_V), "VS true when V set");
        // PL (cc 10) = !N; MI (cc 11) = N.
        assert!(condition_true(10, sup), "PL: N=0 true");
        assert!(!condition_true(10, sup | CCR_N), "PL false when N set");
        assert!(!condition_true(11, sup), "MI: N=0 false");
        assert!(condition_true(11, sup | CCR_N), "MI true when N set");
        // GE (cc 12) = N == V; LT (cc 13) = N != V.
        assert!(condition_true(12, sup), "GE: N=0,V=0 (equal) true");
        assert!(
            condition_true(12, sup | CCR_N | CCR_V),
            "GE: N=1,V=1 (equal) true"
        );
        assert!(!condition_true(12, sup | CCR_N), "GE: N=1,V=0 false");
        assert!(!condition_true(13, sup), "LT: N=0,V=0 (equal) false");
        assert!(condition_true(13, sup | CCR_N), "LT: N=1,V=0 true");
        assert!(condition_true(13, sup | CCR_V), "LT: N=0,V=1 true");
        // GT (cc 14) = (N == V) & !Z; LE (cc 15) = Z | (N != V).
        assert!(condition_true(14, sup), "GT: N==V, Z=0 true");
        assert!(!condition_true(14, sup | CCR_Z), "GT false when Z set");
        assert!(!condition_true(14, sup | CCR_N), "GT false when N!=V");
        assert!(condition_true(15, sup | CCR_Z), "LE true when Z set");
        assert!(condition_true(15, sup | CCR_N), "LE true when N!=V");
        assert!(!condition_true(15, sup), "LE: Z=0, N==V false");
    }

    #[test]
    fn branch_disp8_resolves_sign_extended_low_byte_of_opcode() {
        // Operand::BranchDisp8 = sign_extend8(prefetch[0] & 0xFF) — the byte-branch displacement comes from
        // the OPCODE word (prefetch[0]), not prefetch[1]. Resolve via an inert-base EaCalc.
        let mut regs = regs();
        regs.prefetch = [0x636A, 0xDEAD]; // opcode 0x636A → low byte 0x6A = +106; prefetch[1] must be ignored
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EaCalc {
            base: Operand::Zero,
            index: Operand::Zero,
            disp: Operand::BranchDisp8,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            st.scratch[0], 106,
            "BranchDisp8 = sign_extend8(prefetch[0] & 0xFF) = +106"
        );
    }

    #[test]
    fn branch_disp8_sign_extends_negative_low_byte() {
        let mut regs = regs();
        regs.prefetch = [0x62F0, 0x0000]; // low byte 0xF0 → sign-extend → -16 → 0xFFFF_FFF0
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Combine32 {
            hi: 7, // scratch[7] is 0 → (0<<16)|disp resolves BranchDisp8 unmasked
            lo: Operand::BranchDisp8,
            dst: 0,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            st.scratch[0], 0xFFFF_FFF0,
            "BranchDisp8 sign-extends 0xF0 to 0xFFFF_FFF0 (unmasked via Combine32)"
        );
    }

    #[test]
    fn target_calc_sums_three_legs_without_masking() {
        // TargetCalc is the UNMASKED twin of EaCalc: scratch[dst] = base + index + disp, NO 24-bit mask (a
        // branch target / pushed PC is the full 32-bit value). Pin a backward branch whose target's high bits
        // are set: base = pc+2 (0xFFFF_E000+2), disp = -0x100 → 0xFFFF_DF02, which EaCalc would have masked.
        let mut regs = regs();
        regs.pc = 0xFFFF_E000;
        regs.prefetch = [0x6000, 0xFF00]; // word disp 0xFF00 → sign-extend → -256
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::TargetCalc {
            base: Operand::PcOfExt,
            index: Operand::Zero,
            disp: Operand::DispWord,
            dst: 0,
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "TargetCalc is an internal compute — 0 cycles");
        assert_eq!(
            st.scratch[0], 0xFFFF_DF02,
            "(pc+2) + (-256) = 0xFFFF_DF02, UNMASKED (EaCalc would mask to 0x00FF_DF02)"
        );
        assert!(bus.log.is_empty(), "TargetCalc touches no bus");
    }

    #[test]
    fn set_pc_writes_value_minus_4_unmasked() {
        // SetPc { value } sets regs.pc = resolve(value) - 4 (the −4 primes the two Prefetch ops that follow
        // to reload the queue at `value`, leaving pc == value). NO mask — the PC stays full 32-bit. 0 cycles,
        // no bus.
        let mut regs = regs();
        regs.pc = 0x0000_0C00;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SetPc {
            value: Operand::Scratch(0),
        }]);
        st.scratch[0] = 0xFFFF_DB42; // a backward branch target with high bits set

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "SetPc is an internal compute — 0 cycles");
        assert_eq!(
            regs.pc, 0xFFFF_DB3E,
            "pc = target - 4 (0xFFFF_DB42 - 4), UNMASKED"
        );
        assert!(bus.log.is_empty(), "SetPc touches no bus");
    }

    #[test]
    fn set_pc_then_two_prefetch_lands_pc_at_target_and_reloads_queue() {
        // The branch reload invariant: SetPc(target) sets pc = target-4, then the two Prefetch ops read at
        // target then target+2 (FC=6 program) and leave pc == target with prefetch = [word@target,
        // word@target+2]. This is the universal taken-branch tail.
        let mut regs = regs();
        regs.pc = 0x0000_0C00;
        regs.prefetch = [0x6000, 0x0000];
        let mut bus = FlatBus::new();
        // The two words at the branch target.
        bus.poke(0x0000_1000, 0x12);
        bus.poke(0x0000_1001, 0x34);
        bus.poke(0x0000_1002, 0x56);
        bus.poke(0x0000_1003, 0x78);
        let mut st = MicroState::from_ops(&[
            MicroOp::SetPc {
                value: Operand::Scratch(0),
            },
            MicroOp::Prefetch,
            MicroOp::Prefetch,
        ]);
        st.scratch[0] = 0x0000_1000; // target

        let cycles = st.run_to_completion(&mut regs, &mut bus);

        assert_eq!(cycles, 8, "two word prefetch reads = 8 cycles (SetPc is 0)");
        assert_eq!(regs.pc, 0x0000_1000, "pc landed exactly at the target");
        assert_eq!(
            regs.prefetch,
            [0x1234, 0x5678],
            "queue reloaded with the two words at target / target+2"
        );
        assert_eq!(
            bus.log,
            vec![
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 0x0000_1000,
                    size: Size::Word,
                    value: 0x1234,
                },
                Transaction {
                    kind: TxKind::Read,
                    fc: 6,
                    addr: 0x0000_1002,
                    size: Size::Word,
                    value: 0x5678,
                },
            ],
            "both reloads are supervisor-program (FC 6) word reads at target / target+2"
        );
    }

    // --- F2: the return-address base operand (PcPlus). ---

    #[test]
    fn pc_plus_resolves_to_pc_plus_n_unmasked() {
        // Operand::PcPlus(n) = regs.pc.wrapping_add(n) — the BSR/JSR return-address base, computed UNMASKED
        // (a pushed return address keeps its full 32 bits). Resolve via a TargetCalc (the unmasked twin of
        // EaCalc) so the high bits survive. pc near the top of the 32-bit space + n wraps without masking.
        let mut regs = regs();
        regs.pc = 0xFFFF_FFFE;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::TargetCalc {
            base: Operand::PcPlus(4),
            index: Operand::Zero,
            disp: Operand::Zero,
            dst: 0,
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "PcPlus resolves inside a 0-cycle TargetCalc");
        assert_eq!(
            st.scratch[0], 0x0000_0002,
            "0xFFFF_FFFE + 4 wraps to 0x0000_0002 (UNMASKED 32-bit add)"
        );
        assert!(bus.log.is_empty(), "TargetCalc touches no bus");
    }

    #[test]
    fn pc_plus_2_and_4_select_byte_and_word_return_addresses() {
        // The byte-form BSR pushes pc+2; the word-form BSR pushes pc+4. PcPlus(2)/PcPlus(4) pin both.
        let mut regs = regs();
        regs.pc = 0x0000_0C00;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[
            MicroOp::TargetCalc {
                base: Operand::PcPlus(2),
                index: Operand::Zero,
                disp: Operand::Zero,
                dst: 0,
            },
            MicroOp::TargetCalc {
                base: Operand::PcPlus(4),
                index: Operand::Zero,
                disp: Operand::Zero,
                dst: 1,
            },
        ]);

        st.run_to_completion(&mut regs, &mut bus);

        assert_eq!(st.scratch[0], 0x0000_0C02, "byte BSR return = pc + 2");
        assert_eq!(st.scratch[1], 0x0000_0C04, "word BSR return = pc + 4");
    }

    // --- F5: the DBcc loop counter (DecrementDnWord). ---

    #[test]
    fn decrement_dn_word_subtracts_one_from_low_word_preserving_high_no_flags() {
        // DecrementDnWord: Dn low word −= 1, high word preserved, NO flags. Pinned to the real SST
        // `59c8 [DBcc D0, #]`: D0 0x2602_5C43 → 0x2602_5C42 (low word 0x5C43 → 0x5C42; high 0x2602 survives);
        // the CCR is untouched (a dirty SR must SURVIVE unchanged — DBcc never writes flags).
        let mut regs = regs();
        regs.d[0] = 0x2602_5C43;
        regs.sr = 0x271C; // CCR = X|N|Z, supervisor — must survive UNCHANGED
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::DecrementDnWord { reg: 0 }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            cycles, 0,
            "DecrementDnWord is an internal compute — 0 cycles"
        );
        assert_eq!(regs.d[0], 0x2602_5C42, "low word −1, high word preserved");
        assert_eq!(regs.sr, 0x271C, "no flags affected by the DBcc decrement");
        assert!(bus.log.is_empty(), "DecrementDnWord touches no bus");
    }

    #[test]
    fn decrement_dn_word_wraps_zero_to_ffff_without_borrowing_into_high_word() {
        // The counter-expiry case: low word 0 wraps to 0xFFFF (the −1 the DBcc decode reads to terminate the
        // loop) WITHOUT borrowing into the high word — 0x0003_0000 → 0x0003_FFFF, not 0x0002_FFFF.
        let mut regs = regs();
        regs.d[3] = 0x0003_0000;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::DecrementDnWord { reg: 3 }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[3], 0x0003_FFFF,
            "low word 0 → 0xFFFF; the borrow does NOT propagate into the high word"
        );
    }

    // --- F6: the RTR CCR pop (LoadCcr). ---

    #[test]
    fn load_ccr_loads_low_5_bits_into_ccr_preserving_system_byte() {
        // RTR's CCR pop: low 5 bits (X/N/Z/V/C) into the CCR; bits 7-5 of the popped low byte are dropped
        // (mask 0x1F), the SR system byte is preserved. Pinned to the real SST `4e77 [RTR] 1`: SR 0x2715,
        // popped CCR word 0x6FF6 (low byte 0xF6) → final SR 0x2716 (CCR = 0xF6 & 0x1F = 0x16; system byte
        // 0x27 preserved).
        let mut regs = regs();
        regs.sr = 0x2715;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::LoadCcr {
            value: Operand::Scratch(0),
        }]);
        st.scratch[0] = 0x6FF6; // the popped stack word

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "LoadCcr is an internal compute — 0 cycles");
        assert_eq!(
            regs.sr, 0x2716,
            "CCR = popped & 0x1F (0xF6 → 0x16); system byte 0x27 preserved"
        );
        assert!(bus.log.is_empty(), "LoadCcr touches no bus");
    }

    #[test]
    fn load_ccr_drops_bits_7_5_of_the_popped_byte() {
        // A popped low byte 0x80 (bit7 set, all CCR bits clear) yields CCR 0x00 — bits 7-5 are not CCR bits.
        // Pinned to the real SST `4e77 [RTR] 5`: SR 0x2700, popped CCR 0xB780 → final SR 0x2700.
        let mut regs = regs();
        regs.sr = 0x2700;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::LoadCcr {
            value: Operand::Scratch(0),
        }]);
        st.scratch[0] = 0xB780;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.sr, 0x2700,
            "0x80 & 0x1F = 0 → CCR cleared, system byte kept"
        );
    }

    #[test]
    fn enter_exception_captures_sr_then_sets_supervisor_clears_trace() {
        // From a user-mode, trace-on SR (T set, S clear), EnterException stacks the LIVE SR into scratch and
        // transforms the running SR: S set, T cleared, the rest preserved.
        let mut regs = regs();
        regs.sr = 0x8004; // T=1, S=0, Z=1
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EnterException { save_sr: 1 }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            cycles, 0,
            "EnterException is an internal transform — 0 cycles"
        );
        assert_eq!(
            st.scratch[1], 0x8004,
            "the LIVE (pre-entry) SR was captured"
        );
        assert_eq!(
            regs.sr, 0x2004,
            "S set (0x2000), T cleared (0x8000), the rest preserved"
        );
        assert!(bus.log.is_empty(), "EnterException touches no bus");
    }

    #[test]
    fn enter_exception_is_a_no_op_transform_when_already_supervisor_trace_off() {
        // The all-supervisor vendored shape: S already 1, T already 0 → the running SR is unchanged, and the
        // captured SR equals the original (the value the frame push then stacks).
        let mut regs = regs();
        regs.sr = 0x2707; // S=1, T=0 (a real TRAP anchor SR)
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::EnterException { save_sr: 1 }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(st.scratch[1], 0x2707, "captured SR = the original");
        assert_eq!(regs.sr, 0x2707, "already S=1/T=0 → no change");
    }

    #[test]
    fn load_imm_materializes_a_constant_into_scratch() {
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::LoadImm { value: 128, dst: 3 }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "LoadImm is an internal compute — 0 cycles");
        assert_eq!(st.scratch[3], 128, "the constant landed in scratch slot 3");
        assert!(bus.log.is_empty(), "LoadImm touches no bus");
    }

    #[test]
    fn set_byte_writes_low_byte_preserving_upper_24_with_no_flags() {
        // SetByte is the no-flag byte constant write (Scc's conditional 0xFF/0x00). Into Dn's low byte it
        // preserves the upper 24 bits and touches NO flags (the whole SR is unchanged).
        let mut regs = regs();
        regs.d[4] = 0x2CC2_60E3; // upper 24 = 0x2CC260
        let sr_before = regs.sr;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SetByte {
            value: 0xFF,
            dst: Dest::DataRegLow8(4),
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "SetByte is an internal compute — 0 cycles");
        assert_eq!(
            regs.d[4], 0x2CC2_60FF,
            "0xFF written to the low byte, upper 24 bits preserved"
        );
        assert_eq!(
            regs.sr, sr_before,
            "SetByte touches NO flags (SR unchanged)"
        );
        assert!(bus.log.is_empty(), "SetByte touches no bus");
    }

    #[test]
    fn set_byte_writes_scratch_slot_zero_extended() {
        // The memory-destination form parks the byte in a scratch slot (the trailing Write stores it).
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SetByte {
            value: 0x00,
            dst: Dest::Scratch(1),
        }]);
        st.scratch[1] = 0xDEAD_BEEF; // proves the whole slot is overwritten (zero-extended)

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0);
        assert_eq!(st.scratch[1], 0x0000_0000, "0x00 parked in scratch slot 1");
        assert!(bus.log.is_empty());
    }

    #[test]
    fn set_word_writes_sr_to_low_word_preserving_upper_16_with_no_flags() {
        // SetWord is the no-flag word write (MOVEfromSR's `Dn.w = SR`). Into Dn's low word it preserves the
        // upper 16 bits and touches NO flags — the SR value is WRITTEN but the SR itself is byte-identical.
        let mut regs = regs();
        regs.d[6] = 0x1B91_A995; // upper 16 = 0x1B91
        regs.sr = 0x270B; // the full 16-bit SR (system byte + CCR)
        let sr_before = regs.sr;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SetWord {
            value: Operand::Sr,
            dst: Dest::DataRegLow16(6),
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "SetWord is an internal compute — 0 cycles");
        assert_eq!(
            regs.d[6], 0x1B91_270B,
            "SR (0x270B) written to the low word, upper 16 bits preserved"
        );
        assert_eq!(
            regs.sr, sr_before,
            "SetWord touches NO flags (SR byte-identical — the no-flag invariant)"
        );
        assert!(bus.log.is_empty(), "SetWord touches no bus");
    }

    #[test]
    fn set_word_parks_sr_in_scratch_zero_extended() {
        // The memory-destination form parks the word in a scratch slot (the trailing Write stores it),
        // zero-extended over the whole slot.
        let mut regs = regs();
        regs.sr = 0x2714;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SetWord {
            value: Operand::Sr,
            dst: Dest::Scratch(1),
        }]);
        st.scratch[1] = 0xDEAD_BEEF; // proves the whole slot is overwritten (zero-extended)

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0);
        assert_eq!(
            st.scratch[1], 0x0000_2714,
            "SR parked in scratch slot 1 (zero-extended)"
        );
        assert!(bus.log.is_empty());
    }

    #[test]
    fn tas_flags_on_input_byte_writes_or_0x80_preserving_x() {
        // TAS Dn: the flags come from the READ (input) byte — N = bit7(byte), Z = (byte == 0), V/C cleared,
        // X PRESERVED — while the WRITTEN value is `byte | 0x80` (bit 7 ALWAYS set), DISTINCT from the flag
        // input (unlike NOT, whose flags are on the result `~a`). bit7 ALREADY set: N = 1, the write keeps
        // bit7 (the low byte is unchanged); the upper 24 bits are preserved; enter X1 V1 C1 → X kept, V/C
        // cleared.
        let mut regs = regs();
        regs.d[3] = 0x9FCE_9483; // low byte 0x83 (bit7 set), upper 24 = 0x9FCE94
        regs.sr |= CCR_X | CCR_V | CCR_C; // X = 1 (must be kept), V/C set (must be cleared)
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Tas,
            size: Size::Byte,
            a: Operand::DataRegLow8(3),
            b: Operand::Zero,
            dst: Dest::DataRegLow8(3),
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "the Tas Alu is a 0-cycle internal compute");
        assert_eq!(
            regs.d[3], 0x9FCE_9483,
            "0x83 | 0x80 == 0x83 — low byte unchanged, upper 24 preserved"
        );
        assert_ne!(regs.sr & CCR_N, 0, "N = bit7(input byte) = 1");
        assert_eq!(regs.sr & CCR_Z, 0, "Z = (input byte == 0) = 0");
        assert_eq!(regs.sr & CCR_V, 0, "V cleared");
        assert_eq!(regs.sr & CCR_C, 0, "C cleared");
        assert_ne!(regs.sr & CCR_X, 0, "X PRESERVED");
        assert!(bus.log.is_empty(), "the Tas Alu touches no bus");
    }

    #[test]
    fn tas_zero_byte_sets_z_and_writes_0x80() {
        // byte == 0 → Z = 1, the written value is `0x00 | 0x80 == 0x80` (the flag input 0x00 DIFFERS from the
        // written 0x80); N = bit7(0x00) = 0; the upper 24 bits are preserved.
        let mut regs = regs();
        regs.d[2] = 0xB3CB_1000; // low byte 0x00, upper 24 = 0xB3CB10
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Alu {
            op: AluOp::Tas,
            size: Size::Byte,
            a: Operand::DataRegLow8(2),
            b: Operand::Zero,
            dst: Dest::DataRegLow8(2),
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            regs.d[2], 0xB3CB_1080,
            "0x00 | 0x80 == 0x80 written, upper 24 preserved"
        );
        assert_ne!(regs.sr & CCR_Z, 0, "Z = (input byte == 0) = 1");
        assert_eq!(regs.sr & CCR_N, 0, "N = bit7(0x00) = 0");
        assert_eq!(regs.sr & CCR_V, 0, "V cleared");
        assert_eq!(regs.sr & CCR_C, 0, "C cleared");
    }

    #[test]
    fn tas_rmw_atomic_read_modify_write_one_tas_transaction_flags_from_read_byte() {
        // MicroOp::TasRmw is the ATOMIC indivisible memory RMW: ONE `Tas` bus cycle (10 cyc) reads `orig`,
        // writes `orig | 0x80`, and logs ONE transaction (value = the WRITTEN byte). The flags come from the
        // READ byte `orig` — N = bit7(orig) / Z = (orig == 0), V/C cleared, X PRESERVED — while the written
        // value is `orig | 0x80` (DISTINCT). Pinned to the `4ad2 [TAS (A2)]` anchor's `'t'` transaction
        // `['t', 10, 5, 2840449, '.b', 181]`: orig 0x35 → written 0xB5 (181), N = 0 / Z = 0, X kept.
        let mut regs = regs(); // supervisor → data FC 5
        regs.sr |= CCR_X | CCR_V | CCR_C; // X = 1 (must be kept), V/C set (must be cleared)
        regs.a[2] = 2_840_449; // A2 (addr_reg(2) == a[2]) holds the anchor's EA
        let mut bus = FlatBus::new();
        bus.poke(2_840_449, 0x35); // orig — bit7 clear (N = 0), non-zero (Z = 0)

        let mut st = MicroState::from_ops(&[MicroOp::TasRmw {
            addr: Operand::AddrReg(2),
        }]);
        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(
            cycles, 10,
            "the atomic TAS RMW is 10 cyc (read 4 + modify 2 + write 4)"
        );
        assert_eq!(
            bus.peek(2_840_449),
            0xB5,
            "wrote orig | 0x80 (0x35 | 0x80 == 0xB5)"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Tas,
                fc: 5,
                addr: 2_840_449,
                size: Size::Byte,
                value: 0xB5, // ONE Tas transaction, value = the WRITTEN byte (pinned to anchor value 181)
            }],
            "exactly ONE atomic Tas transaction"
        );
        assert_eq!(regs.sr & CCR_N, 0, "N = bit7(read byte 0x35) = 0");
        assert_eq!(regs.sr & CCR_Z, 0, "Z = (read byte == 0) = 0");
        assert_eq!(regs.sr & CCR_V, 0, "V cleared");
        assert_eq!(regs.sr & CCR_C, 0, "C cleared");
        assert_ne!(regs.sr & CCR_X, 0, "X PRESERVED");
    }

    #[test]
    fn tas_rmw_sets_n_from_read_bit7_and_z_on_zero_byte() {
        // Two more TasRmw flag pins: bit7-set read byte → N = 1 (the write keeps bit7), and a zero read byte
        // → Z = 1 with the written value 0x80 (flag input 0x00 DIFFERS from the written 0x80).
        let mut regs_a = regs();
        let mut regs_b = regs();
        regs_a.a[1] = 0x0000_3000; // A1 (addr_reg(1) == a[1])
        let mut bus = FlatBus::new();
        bus.poke(0x3000, 0xC3); // bit7 set → N = 1
        let mut st = MicroState::from_ops(&[MicroOp::TasRmw {
            addr: Operand::AddrReg(1),
        }]);
        st.exec_one(&mut regs_a, &mut bus);
        assert_eq!(
            bus.peek(0x3000),
            0xC3,
            "0xC3 | 0x80 == 0xC3 (bit7 already set)"
        );
        assert_ne!(regs_a.sr & CCR_N, 0, "N = bit7(0xC3) = 1");
        assert_eq!(regs_a.sr & CCR_Z, 0, "Z = 0 (0xC3 != 0)");

        regs_b.a[1] = 0x0000_3100;
        let mut bus2 = FlatBus::new();
        bus2.poke(0x3100, 0x00); // zero read byte → Z = 1, written 0x80
        let mut st2 = MicroState::from_ops(&[MicroOp::TasRmw {
            addr: Operand::AddrReg(1),
        }]);
        st2.exec_one(&mut regs_b, &mut bus2);
        assert_eq!(bus2.peek(0x3100), 0x80, "0x00 | 0x80 == 0x80 written");
        assert_ne!(regs_b.sr & CCR_Z, 0, "Z = (read byte == 0) = 1");
        assert_eq!(regs_b.sr & CCR_N, 0, "N = bit7(0x00) = 0");
    }

    // --- E3: the SpPlus frame-write operand + the execution-time address-error abort. ---

    #[test]
    fn sp_plus_resolves_to_active_a7_plus_signed_offset() {
        // Operand::SpPlus(n) = regs.addr_reg(7).wrapping_add(n) — the frame-write address (A7 is the
        // supervisor SP here). Resolve it via a Write so we see the bus address it produced.
        let mut regs = regs(); // supervisor → A7 == ssp
        regs.ssp = 0x0000_2000;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Write {
            addr: Operand::SpPlus(12),
            fc: Fc::Data,
            size: Size::Word,
            value: Operand::Scratch(0),
        }]);
        st.scratch[0] = 0xBEEF;

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(bus.log[0].addr, 0x0000_200C, "SpPlus(12) = ssp + 12");
        assert_eq!(bus.peek(0x0000_200C), 0xBE, "the word landed at ssp + 12");
    }

    #[test]
    fn odd_word_read_installs_the_address_error_frame_in_place() {
        // A word Read to an ODD address never touches the bus — it rewrites the MicroState into the 14-byte
        // group-0 frame IN PLACE: `step` rewinds to 0, `cycles` + `opcode` are preserved, the frame fields
        // are seeded into scratch, and the first installed micro-op is the leading n4 idle. The full frame
        // transaction stream is pinned end-to-end by the SST anchor `d850` in the runner; this pins the
        // in-place install mechanism.
        let mut regs = regs(); // supervisor (S=1)
        regs.pc = 0x0000_2222;
        regs.ssp = 0x0000_3000;
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Read {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Word,
            dst: 1,
        }]);
        st.set_opcode(0xD850);
        st.scratch[0] = 0x0010_0001; // odd address → address error
        st.cycles = 4; // pretend a leading prefetch already ran

        let cost = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cost, 0, "the faulting micro-op itself is free");
        assert_eq!(st.cycles, 4, "accrued cycles preserved across the abort");
        assert_eq!(st.step, 0, "step rewound to the start of the frame recipe");
        assert!(bus.log.is_empty(), "the odd access never reached the bus");
        assert_eq!(
            st.ops[0],
            MicroOp::Internal { cycles: 4 },
            "the frame's leading n4 idle"
        );
        assert!(!st.is_done(), "the 14-byte frame recipe is now in flight");
        // The seeded frame fields (slots per `exception::AERR_*`: pc=0, fault-addr=2, IR=8, SSW=9).
        assert_eq!(st.scratch[0], 0x0000_2222, "stacked PC = live regs.pc");
        assert_eq!(st.scratch[2], 0x0010_0001, "faulting address (full 32-bit)");
        assert_eq!(st.scratch[8], 0xD850, "IR = the latched opcode");
        assert_eq!(
            st.scratch[9], 0xD855,
            "SSW = (opcode & 0xFFE0) | 0x15 (data read)"
        );
    }

    // --- E6: the privileged SR-logic op (the `*toSR` write-back) + the widened Internal cycle field. ---

    #[test]
    fn sr_logic_and_masks_to_implemented_bits() {
        // ANDItoSR: sr = (sr & imm) & SR_IMPLEMENTED. Pinned to the vendored `027c [ANDItoSR #] 1` STAY case:
        // sr 0x271E & imm 0xFF7D = 0x271C; & 0xA71F = 0x271C (S stays set). A 0-cycle internal step.
        let mut regs = regs();
        regs.sr = 0x271E;
        regs.prefetch = [0x027C, 0xFF7D]; // the immediate is prefetch[1]
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SrLogic {
            op: LogicOp::And,
            value: Operand::ImmWord,
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "SrLogic is an internal transform — 0 cycles");
        assert_eq!(regs.sr, 0x271C, "(0x271E & 0xFF7D) & 0xA71F = 0x271C");
        assert!(bus.log.is_empty(), "SrLogic touches no bus");
    }

    #[test]
    fn sr_logic_and_can_clear_supervisor() {
        // The SWITCH case `027c [ANDItoSR #] 2`: sr 0x2717 & imm 0x4CBE = 0x0416; & 0xA71F = 0x0416 (S cleared).
        let mut regs = regs();
        regs.sr = 0x2717;
        regs.prefetch = [0x027C, 0x4CBE];
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SrLogic {
            op: LogicOp::And,
            value: Operand::ImmWord,
        }]);

        st.exec_one(&mut regs, &mut bus);

        assert_eq!(regs.sr, 0x0416, "S cleared by the AND mask");
        assert_eq!(regs.sr & SR_SUPERVISOR, 0, "supervisor bit cleared");
    }

    #[test]
    fn sr_logic_or_and_eor_mask_to_implemented_bits() {
        // ORItoSR sets bits (never clears S); EORItoSR toggles. Both mask to 0xA71F. Pinned to the formula
        // verified across all 8065 cases of each file.
        let mut regs_or = regs();
        regs_or.sr = 0x2700;
        regs_or.prefetch = [0x007C, 0xFFFF]; // OR with all-ones → all implemented bits set
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::SrLogic {
            op: LogicOp::Or,
            value: Operand::ImmWord,
        }]);
        st.exec_one(&mut regs_or, &mut bus);
        assert_eq!(regs_or.sr, 0xA71F, "(0x2700 | 0xFFFF) & 0xA71F = 0xA71F");

        let mut regs2 = regs();
        regs2.sr = 0x2707;
        regs2.prefetch = [0x0A7C, 0xFFFF]; // EOR with all-ones → toggle every implemented bit
        let mut st2 = MicroState::from_ops(&[MicroOp::SrLogic {
            op: LogicOp::Eor,
            value: Operand::ImmWord,
        }]);
        st2.exec_one(&mut regs2, &mut bus);
        assert_eq!(regs2.sr, 0x8018, "(0x2707 ^ 0xFFFF) & 0xA71F = 0x8018");
    }

    // --- A0: the `*toCCR` write-back — the CCR-masking twin of SrLogic (system byte PRESERVED). ---

    #[test]
    fn ccr_logic_and_preserves_system_byte() {
        // ANDItoCCR: sr = (sr & 0xFF00) | ((sr & imm) & 0x1F). Pinned to the vendored `023c [ANDItoCCR #] 1`
        // case: sr 0x2709 & imm 0xD39A → (0x2709 & 0xD39A) & 0x1F = 0x08; system byte 0x2700 preserved → 0x2708.
        // The immediate's high byte (0xD3) is DON'T-CARE — it never touches the SR system byte. A 0-cycle step.
        let mut regs = regs();
        regs.sr = 0x2709;
        regs.prefetch = [0x023C, 0xD39A]; // the immediate is prefetch[1]
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::CcrLogic {
            op: LogicOp::And,
            value: Operand::ImmWord,
        }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 0, "CcrLogic is an internal transform — 0 cycles");
        assert_eq!(
            regs.sr, 0x2708,
            "(0x2709 & 0xD39A) & 0x1F = 0x08; system byte 0x2700 preserved"
        );
        assert_eq!(
            regs.sr & 0xFF00,
            0x2700,
            "the SR system byte (T/S/I) is PRESERVED — S never clears"
        );
        assert!(bus.log.is_empty(), "CcrLogic touches no bus");
    }

    #[test]
    fn ccr_logic_or_and_eor_preserve_system_byte() {
        // ORItoCCR sets CCR bits (never touches S); EORItoCCR toggles CCR bits. Both keep the system byte and
        // force CCR bits 5-7 to 0 (only the low-5 X/N/Z/V/C change). The immediate high byte is don't-care.
        let mut regs_or = regs();
        regs_or.sr = 0x2700; // system byte set, CCR clear
        regs_or.prefetch = [0x003C, 0xFFFF]; // OR with all-ones → all CCR bits set, system byte untouched
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::CcrLogic {
            op: LogicOp::Or,
            value: Operand::ImmWord,
        }]);
        st.exec_one(&mut regs_or, &mut bus);
        assert_eq!(
            regs_or.sr, 0x271F,
            "(0x2700 | 0xFFFF) → CCR low-5 all set (0x1F), system byte 0x2700 PRESERVED"
        );

        let mut regs2 = regs();
        regs2.sr = 0x2707; // system byte set + some CCR bits
        regs2.prefetch = [0x0A3C, 0xFFFF]; // EOR with all-ones → toggle every CCR bit
        let mut st2 = MicroState::from_ops(&[MicroOp::CcrLogic {
            op: LogicOp::Eor,
            value: Operand::ImmWord,
        }]);
        st2.exec_one(&mut regs2, &mut bus);
        assert_eq!(
            regs2.sr, 0x2718,
            "(0x2707 ^ 0xFFFF) & 0x1F = 0x18; system byte 0x2700 PRESERVED"
        );
    }

    #[test]
    fn internal_carries_a_wide_cycle_count() {
        // RESET idles 124 cycles — the widened u16 `Internal` cycle field exceeds the old u8 range.
        let mut regs = regs();
        let mut bus = FlatBus::new();
        let mut st = MicroState::from_ops(&[MicroOp::Internal { cycles: 124 }]);

        let cycles = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cycles, 124, "Internal costs its declared wide cycle count");
        assert!(bus.log.is_empty(), "Internal touches no bus");
    }

    #[test]
    fn odd_byte_read_does_not_fault() {
        // A BYTE access drives one bus half regardless of parity, so an odd byte address never raises an
        // address error — it reads normally.
        let mut regs = regs();
        let mut bus = FlatBus::new();
        bus.poke(0x0010_0001, 0x7A);
        let mut st = MicroState::from_ops(&[MicroOp::Read {
            addr: Operand::Scratch(0),
            fc: Fc::Data,
            size: Size::Byte,
            dst: 1,
        }]);
        st.scratch[0] = 0x0010_0001; // odd, but a byte access is fine

        let cost = st.exec_one(&mut regs, &mut bus);

        assert_eq!(cost, 4, "an odd byte read is a normal 4-cycle access");
        assert_eq!(st.scratch[1], 0x7A, "the byte was read");
        assert!(st.is_done(), "no abort — the single Read completed");
    }

    // --- Push A / A3.1: the trace frame's write order (Yacht L1548 `ns nS ns` = `s S s`) is a permutation
    // of the standard frame's (`ns ns nS` = `s s S`) — SAME final layout, different on-bus order. Pin the
    // DISTINCTION itself so a refactor can never silently collapse `push_trace_frame` into the standard one.
    #[test]
    fn trace_frame_write_order_is_s_capital_s_s_not_s_s_capital_s() {
        use crate::m68000::ea::RecipeBuf;
        use crate::m68000::exception::{push_standard_frame, push_trace_frame};

        // Run a frame builder with seeded saved-PC (slot 0) + saved-SR (slot 1) and A7 = 0x1000, capturing
        // its three word writes. Post-push B = SSP - 6 = 0x0FFA; the frame lands SR @ B+0 = 0x0FFA,
        // PCH @ B+2 = 0x0FFC, PCL @ B+4 = 0x0FFE — identical layout for both builders.
        fn run(build: fn(&mut RecipeBuf, Slot, Slot)) -> Vec<Transaction> {
            let mut buf = RecipeBuf::new();
            build(&mut buf, 0, 1);
            let mut st = buf.finish();
            st.scratch[0] = 0x1234_5678; // saved PC (PCH = 0x1234, PCL = 0x5678)
            st.scratch[1] = 0x0000_2700; // saved SR
            let regs_seed = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0,
                ssp: 0x1000, // supervisor → A7 = SSP = 0x1000
                pc: 0,
                sr: SR_SUPERVISOR,
                prefetch: [0; 2],
            };
            let mut regs = regs_seed;
            let mut bus = FlatBus::new();
            while !st.is_done() {
                st.exec_one(&mut regs, &mut bus);
            }
            bus.log
        }

        let std_log = run(push_standard_frame);
        let trace_log = run(push_trace_frame);

        // Both push exactly three words, and the FINAL frame is identical (same {addr,value} multiset).
        assert_eq!(std_log.len(), 3, "standard frame = 3 writes");
        assert_eq!(trace_log.len(), 3, "trace frame = 3 writes");
        let mut std_sorted = std_log.clone();
        let mut trace_sorted = trace_log.clone();
        std_sorted.sort_by_key(|t| t.addr);
        trace_sorted.sort_by_key(|t| t.addr);
        assert_eq!(
            std_sorted, trace_sorted,
            "identical final frame — same three writes at the same addresses/values"
        );

        // ...but the on-bus ORDER differs exactly in the `s S s` vs `s s S` positions: both write PCL @ B+4
        // FIRST (index 0 identical); then the standard frame writes SR @ B+0 then PCH @ B+2, while the trace
        // frame writes PCH @ B+2 then SR @ B+0 — the second and third writes are swapped.
        assert_eq!(std_log[0], trace_log[0], "both write PCL @ B+4 first");
        assert_eq!(std_log[0].addr, 0x0FFE, "PCL @ B+4");
        assert_eq!(std_log[1].addr, 0x0FFA, "standard 2nd: SR @ B+0");
        assert_eq!(std_log[2].addr, 0x0FFC, "standard 3rd: PCH @ B+2");
        assert_eq!(trace_log[1].addr, 0x0FFC, "trace 2nd: PCH @ B+2");
        assert_eq!(trace_log[2].addr, 0x0FFA, "trace 3rd: SR @ B+0");
        // Stated as the exact permutation, so neither builder can silently converge into the other:
        assert_eq!(
            std_log[1], trace_log[2],
            "standard's SR write = trace's third write"
        );
        assert_eq!(
            std_log[2], trace_log[1],
            "standard's PCH write = trace's second write"
        );
        assert_ne!(
            std_log, trace_log,
            "the two builders never produce the same on-bus order"
        );
    }

    // --- Push A / A3.1: begin_next trace dispatch. T latched at instruction START (M68000UM §6.3.8); a
    // trace exception (vector 9) is serviced on the FOLLOWING begin_next, stacked PC = the next instruction.
    fn poke_w(bus: &mut FlatBus, addr: u32, val: u16) {
        bus.poke(addr, (val >> 8) as u8);
        bus.poke(addr + 1, val as u8);
    }

    /// A program at 0x0C00 (words) with a vector-9 trace handler (0x2000) and a vector-4 illegal handler
    /// (0x3000), each a pair of NOPs, and the supervisor stack at 0x1000. Any word not in `program` reads as
    /// NOP so trailing prefetches are always valid.
    fn trace_env(sr: u16, program: &[u16]) -> (Cpu68000, FlatBus) {
        let mut bus = FlatBus::new();
        for a in (0x0C00u32..0x0C20).step_by(2) {
            poke_w(&mut bus, a, 0x4E71); // NOP fill
        }
        for (i, &w) in program.iter().enumerate() {
            poke_w(&mut bus, 0x0C00 + 2 * i as u32, w);
        }
        poke_w(&mut bus, 0x26, 0x2000); // vector 9 @ 0x24 → 0x0000_2000 (trace)
        poke_w(&mut bus, 0x12, 0x3000); // vector 4 @ 0x10 → 0x0000_3000 (illegal)
        poke_w(&mut bus, 0x22, 0x4000); // vector 8 @ 0x20 → 0x0000_4000 (privilege)
        for a in [0x2000u32, 0x2002, 0x3000, 0x3002, 0x4000, 0x4002] {
            poke_w(&mut bus, a, 0x4E71);
        }
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000,
            ssp: 0x0000_1000,
            pc: 0x0C00,
            sr,
            prefetch: [
                *program.first().unwrap_or(&0x4E71),
                *program.get(1).unwrap_or(&0x4E71),
            ],
        };
        (Cpu68000::new(regs), bus)
    }

    /// Two NOPs at 0x0C00/0x0C02, a vector-9 handler (0x2000) of NOPs, supervisor stack at 0x1000.
    fn trace_setup(sr: u16) -> (Cpu68000, FlatBus) {
        trace_env(sr, &[0x4E71, 0x4E71])
    }

    #[test]
    fn trace_taken_after_a_plain_instruction_with_t_set() {
        // SR = S=1, T=1, I=7 (0xA700). step #1 runs the NOP; step #2 must service the trace exception.
        let (mut cpu, mut bus) = trace_setup(0xA700);
        cpu.step(&mut bus); // NOP at 0x0C00 → pc = 0x0C02
        assert_eq!(cpu.regs.pc, 0x0C02, "NOP advanced to the next instruction");
        bus.log.clear(); // isolate the trace exception's transactions
        cpu.step(&mut bus); // must be the trace exception, NOT the second NOP

        assert_eq!(
            cpu.regs.pc, 0x2000,
            "reloaded at the vector-9 trace handler"
        );
        assert_eq!(cpu.regs.sr, 0x2700, "S kept, T cleared for the handler");
        assert_eq!(cpu.regs.ssp, 0x0FFA, "6-byte frame pushed");
        // Frame: stacked PC = 0x0C02 (the NEXT instruction), SR = 0xA700 (T still set — captured pre-clear).
        assert_eq!(bus.peek(0x0FFA), 0xA7, "stacked SR high (T set)");
        assert_eq!(bus.peek(0x0FFB), 0x00, "stacked SR low");
        assert_eq!(bus.peek(0x0FFC), 0x00, "stacked PC high");
        assert_eq!(
            bus.peek(0x0FFE),
            0x0C,
            "stacked PC = 0x0C02 (next instruction), high byte"
        );
        assert_eq!(bus.peek(0x0FFF), 0x02, "stacked PC low byte");
        // The trace frame's `s S s` write order: PCL @ 0x0FFE first, PCH @ 0x0FFC second, SR @ 0x0FFA third.
        let writes: Vec<u32> = bus
            .log
            .iter()
            .filter(|t| t.kind == TxKind::Write)
            .map(|t| t.addr)
            .collect();
        assert_eq!(
            writes,
            vec![0x0FFE, 0x0FFC, 0x0FFA],
            "trace `s S s` write order"
        );
        // Vector 9 fetched at 0x24 (FC=5, supervisor data).
        assert!(
            bus.log
                .iter()
                .any(|t| t.kind == TxKind::Read && t.addr == 0x24 && t.fc == 5),
            "vector 9 fetched at 0x24"
        );
    }

    #[test]
    fn no_trace_when_t_is_clear() {
        // T=0 (0x2700): both NOPs just execute; no trace is ever serviced.
        let (mut cpu, mut bus) = trace_setup(0x2700);
        cpu.step(&mut bus); // NOP @ 0x0C00 → 0x0C02
        cpu.step(&mut bus); // NOP @ 0x0C02 → 0x0C04 (NOT a trace)
        assert_eq!(cpu.regs.pc, 0x0C04, "ran both NOPs — no trace");
        assert_eq!(cpu.regs.ssp, 0x1000, "nothing stacked");
        assert!(
            !bus.log.iter().any(|t| t.addr == 0x24),
            "vector 9 never fetched"
        );
    }

    #[test]
    fn a_t_clearing_instruction_still_traces() {
        // The start-of-instruction T LATCH decides: ANDI #0x2700,SR from T=1 CLEARS T, yet still traces
        // (M68000UM §6.3.8 — T is sampled at the instruction's start, before it changes SR).
        let (mut cpu, mut bus) = trace_env(0xA700, &[0x027C, 0x2700]); // ANDI #0x2700,SR
        cpu.step(&mut bus); // ANDItoSR: sr 0xA700 → 0x2700 (T cleared), pc → 0x0C04
        assert_eq!(cpu.regs.sr & SR_TRACE, 0, "the instruction cleared T");
        assert_eq!(cpu.regs.pc, 0x0C04);
        bus.log.clear();
        cpu.step(&mut bus); // still traces — latch was set at the start
        assert_eq!(cpu.regs.pc, 0x2000, "T-clearing instruction still traced");
        assert_eq!(
            bus.peek(0x0FFE),
            0x0C,
            "stacked PC = 0x0C04 (next instr) high"
        );
        assert_eq!(bus.peek(0x0FFF), 0x04, "stacked PC low");
        assert_eq!(
            bus.peek(0x0FFA),
            0x27,
            "stacked SR = the post-instruction SR (T already clear)"
        );
    }

    #[test]
    fn a_t_setting_instruction_does_not_trace_itself() {
        // The dual: ORI #0x8000,SR from T=0 SETS T, but does NOT trace after itself (latch was clear); the
        // NEXT instruction (now T=1) is the one that traces.
        let (mut cpu, mut bus) = trace_env(0x2700, &[0x007C, 0x8000]); // ORI #0x8000,SR
        cpu.step(&mut bus); // ORItoSR: sr 0x2700 → 0xA700 (T set), pc → 0x0C04
        assert_eq!(cpu.regs.sr & SR_TRACE, SR_TRACE, "the instruction set T");
        assert_eq!(
            cpu.regs.pc, 0x0C04,
            "no trace after the T-setting instruction itself"
        );
        assert_eq!(cpu.regs.ssp, 0x1000, "nothing stacked yet");
        bus.log.clear();
        cpu.step(&mut bus); // the NEXT instruction (NOP, now with T latched) runs...
        assert_eq!(cpu.regs.pc, 0x0C06, "the next NOP executed");
        bus.log.clear();
        cpu.step(&mut bus); // ...and NOW the trace fires
        assert_eq!(
            cpu.regs.pc, 0x2000,
            "trace fired after the first T=1-latched instruction"
        );
    }

    #[test]
    fn a_decode_time_exception_suppresses_the_pending_trace() {
        // Illegal / privilege instructions are "not executed" — §6.3.8 suppresses the trace after them.
        // (a) ILLEGAL (0x4AFC), supervisor + T=1 → vector-4 frame, no trace follows.
        let (mut cpu, mut bus) = trace_env(0xA700, &[0x4AFC, 0x4E71]);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.pc, 0x3000, "illegal → vector-4 handler");
        bus.log.clear();
        cpu.step(&mut bus);
        assert_ne!(cpu.regs.pc, 0x2000, "no trace after ILLEGAL");
        assert!(
            !bus.log.iter().any(|t| t.addr == 0x24),
            "vector 9 not fetched"
        );

        // (b) PRIVILEGE: MOVE #imm,SR (0x46FC) in USER mode with T=1 → vector-8 frame, no trace follows.
        let (mut cpu, mut bus) = trace_env(0x8000, &[0x46FC, 0x2700]); // T=1, S=0 (user)
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.pc, 0x4000,
            "privileged op in user mode → vector-8 handler"
        );
        assert_eq!(
            cpu.regs.sr & SR_SUPERVISOR,
            SR_SUPERVISOR,
            "entered supervisor"
        );
        bus.log.clear();
        cpu.step(&mut bus);
        assert_ne!(cpu.regs.pc, 0x2000, "no trace after a privilege violation");
        assert!(
            !bus.log.iter().any(|t| t.addr == 0x24),
            "vector 9 not fetched"
        );
    }

    #[test]
    fn trace_exception_runs_identically_on_both_drivers() {
        // The trace recipe itself must run bit-identically on run_to_completion and step_micro_op (the SST
        // rigor, applied to the hand-authored trace frame). regs.pc = the next-instruction address at entry.
        let mk = || {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0x0000_3000,
                ssp: 0x0000_1000,
                pc: 0x0C02,
                sr: 0xA700,
                prefetch: [0x4E71, 0x4E71],
            };
            let mut bus = FlatBus::new();
            poke_w(&mut bus, 0x26, 0x2000); // vector 9 → 0x2000
            for a in [0x2000u32, 0x2002] {
                poke_w(&mut bus, a, 0x4E71);
            }
            (regs, bus)
        };
        let (mut regs_rtc, mut bus_rtc) = mk();
        crate::m68000::decode::trace_exception_recipe()
            .run_to_completion(&mut regs_rtc, &mut bus_rtc);

        let (regs_step, mut bus_step) = mk();
        let mut cpu = Cpu68000::new(regs_step);
        cpu.begin(crate::m68000::decode::trace_exception_recipe());
        while cpu.step_micro_op(&mut bus_step) == Step::Continue {}

        assert_eq!(cpu.regs, regs_rtc, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
    }

    // --- Push A / A4.1: autovectored interrupts. Taken between instructions when level > SR mask (strict,
    // M68000UM §6.3.2); vector = 24 + level; mask ← level; stacked PC = the would-be next instruction.
    /// A program at 0x0C00 with autovector handlers for L4 (vector 28 @ 0x70 → 0x2800) and L6 (vector 30 @
    /// 0x78 → 0x2000), supervisor stack at 0x1000. `ipl` is latched via `set_ipl`.
    fn int_env(sr: u16, ipl: u8, program: &[u16]) -> (Cpu68000, FlatBus) {
        let mut bus = FlatBus::new();
        for a in (0x0C00u32..0x0C20).step_by(2) {
            poke_w(&mut bus, a, 0x4E71);
        }
        for (i, &w) in program.iter().enumerate() {
            poke_w(&mut bus, 0x0C00 + 2 * i as u32, w);
        }
        poke_w(&mut bus, 0x72, 0x2800); // vector 28 (L4) @ 0x70 → 0x0000_2800
        poke_w(&mut bus, 0x7A, 0x2000); // vector 30 (L6) @ 0x78 → 0x0000_2000
        for a in [0x2000u32, 0x2002, 0x2800, 0x2802] {
            poke_w(&mut bus, a, 0x4E71);
        }
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000,
            ssp: 0x0000_1000,
            pc: 0x0C00,
            sr,
            prefetch: [
                program.first().copied().unwrap_or(0x4E71),
                program.get(1).copied().unwrap_or(0x4E71),
            ],
        };
        let mut cpu = Cpu68000::new(regs);
        cpu.set_ipl(ipl);
        (cpu, bus)
    }

    #[test]
    fn interrupt_taken_when_level_exceeds_mask() {
        // S=1, I-mask=0, T=0 (0x2000); ipl=6. begin_next services the interrupt BEFORE the first NOP.
        let (mut cpu, mut bus) = int_env(0x2000, 6, &[0x4E71, 0x4E71]);
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.pc, 0x2000,
            "reloaded at the level-6 autovector handler (vector 30)"
        );
        assert_eq!(
            cpu.regs.sr, 0x2600,
            "S kept, T clear, mask raised to level 6"
        );
        assert_eq!(cpu.regs.ssp, 0x0FFA, "6-byte frame pushed");
        // OLD SR (mask 0) stacked, stacked PC = 0x0C00 (the would-be next instruction — none ran yet).
        assert_eq!(bus.peek(0x0FFA), 0x20, "stacked SR = old SR (mask 0) high");
        assert_eq!(bus.peek(0x0FFB), 0x00, "stacked SR low");
        assert_eq!(bus.peek(0x0FFC), 0x00, "stacked PC high");
        assert_eq!(bus.peek(0x0FFE), 0x0C, "stacked PC = 0x0C00 high");
        assert_eq!(bus.peek(0x0FFF), 0x00, "stacked PC low");
        assert!(
            bus.log
                .iter()
                .any(|t| t.kind == TxKind::Read && t.addr == 0x78 && t.fc == 5),
            "vector 30 fetched at 0x78 (autovector = 24 + level)"
        );
    }

    #[test]
    fn interrupt_postponed_when_level_not_above_mask() {
        // level == mask → inhibited (levels ≤ current priority are postponed). mask=6, ipl=6.
        let (mut cpu, mut bus) = int_env(0x2600, 6, &[0x4E71, 0x4E71]);
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.pc, 0x0C02,
            "level == mask → postponed, the NOP ran"
        );
        assert_eq!(cpu.regs.ssp, 0x1000, "nothing stacked");

        // level < mask → postponed, but the request is HELD; once the mask drops below it, it fires.
        let (mut cpu, mut bus) = int_env(0x2600, 4, &[0x4E71, 0x4E71]); // mask 6, ipl 4
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.pc, 0x0C02, "level < mask → postponed");
        cpu.regs.set_int_mask(3); // now ipl 4 > mask 3
        bus.log.clear();
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.pc, 0x2800,
            "held request fires when the mask drops (L4 → vector 28)"
        );
        assert!(
            bus.log
                .iter()
                .any(|t| t.kind == TxKind::Read && t.addr == 0x70 && t.fc == 5),
            "L4 autovector fetched at 0x70 (24 + 4 = 28)"
        );
    }

    #[test]
    fn interrupt_frame_stream_is_pcl_then_iack_then_pch_then_sr() {
        // Pin the IACK-interleaved `s S s` order (Yacht L1549 `... ns ni n- n nS ns ...`): PCL write, then
        // the FC=7 acknowledge cycle, then the PCH and SR writes — the frame split around the IACK.
        let (mut cpu, mut bus) = int_env(0x2000, 6, &[0x4E71, 0x4E71]);
        cpu.step(&mut bus);
        // The transactions up to (not including) the vector fetch at 0x78.
        let frame: Vec<(TxKind, u32, u8)> = bus
            .log
            .iter()
            .take_while(|t| !(t.kind == TxKind::Read && t.addr == 0x78))
            .map(|t| (t.kind, t.addr, t.fc))
            .collect();
        assert_eq!(frame.len(), 4, "PCL, IACK, PCH, SR before the vector fetch");
        assert_eq!(
            (frame[0].0, frame[0].1),
            (TxKind::Write, 0x0FFE),
            "1st: PCL @ B+4"
        );
        assert_eq!(
            (frame[1].0, frame[1].2),
            (TxKind::Read, 7),
            "2nd: IACK in CPU space (FC=7)"
        );
        assert_eq!(
            (frame[2].0, frame[2].1),
            (TxKind::Write, 0x0FFC),
            "3rd: PCH @ B+2"
        );
        assert_eq!(
            (frame[3].0, frame[3].1),
            (TxKind::Write, 0x0FFA),
            "4th: SR @ B+0"
        );
    }

    #[test]
    fn interrupt_timing_is_44_cycles() {
        let (mut cpu, mut bus) = int_env(0x2000, 6, &[0x4E71, 0x4E71]);
        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 44, "interrupt exception = 44(5/3) (Yacht L1549)");
    }

    #[test]
    fn level_7_taken_when_mask_below_7() {
        // ipl=7, mask=6 → taken via the ordinary comparison (7 > 6). Vector 31 @ 0x7C. (The nonmaskable
        // ipl=7/mask=7 edge case is docketed — NOT asserted here.)
        let (mut cpu, mut bus) = int_env(0x2600, 7, &[0x4E71, 0x4E71]);
        poke_w(&mut bus, 0x7E, 0x2000); // vector 31 (L7) @ 0x7C → 0x2000
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.pc, 0x2000, "L7 with mask 6 → taken (vector 31)");
        assert_eq!(cpu.regs.sr & SR_INT_MASK, 0x0700, "mask raised to 7");
    }

    #[test]
    fn interrupt_exception_runs_identically_on_both_drivers() {
        let mk = || {
            let regs = Registers {
                d: [0; 8],
                a: [0; 7],
                usp: 0x0000_3000,
                ssp: 0x0000_1000,
                pc: 0x0C00,
                sr: 0x2000,
                prefetch: [0x4E71, 0x4E71],
            };
            let mut bus = FlatBus::new();
            poke_w(&mut bus, 0x7A, 0x2000); // vector 30 → 0x2000
            for a in [0x2000u32, 0x2002] {
                poke_w(&mut bus, a, 0x4E71);
            }
            (regs, bus)
        };
        let (mut regs_rtc, mut bus_rtc) = mk();
        crate::m68000::decode::interrupt_exception_recipe(6)
            .run_to_completion(&mut regs_rtc, &mut bus_rtc);

        let (regs_step, mut bus_step) = mk();
        let mut cpu = Cpu68000::new(regs_step);
        cpu.begin(crate::m68000::decode::interrupt_exception_recipe(6));
        while cpu.step_micro_op(&mut bus_step) == Step::Continue {}

        assert_eq!(cpu.regs, regs_rtc, "drivers agree on final registers");
        assert_eq!(bus_step.log, bus_rtc.log, "drivers agree on transactions");
    }

    #[test]
    fn trace_outranks_interrupt_and_the_interrupt_handler_is_not_traced() {
        // The end-to-end trace > interrupt chain (M68000UM §6.3.8): a T=1 instruction completes with an
        // interrupt pending → trace services FIRST, the interrupt SECOND (execution resumes in the interrupt
        // handler), and the interrupt handler's first instruction is NOT traced (T was cleared by the trace
        // exception's own EnterException).
        let mut bus = FlatBus::new();
        for a in (0x0C00u32..0x0C10).step_by(2) {
            poke_w(&mut bus, a, 0x4E71); // NOP program
        }
        poke_w(&mut bus, 0x26, 0x2000); // vector 9 (trace)      → 0x2000
        poke_w(&mut bus, 0x7A, 0x4000); // vector 30 (L6 int)    → 0x4000
        for a in [0x2000u32, 0x2002, 0x4000, 0x4002] {
            poke_w(&mut bus, a, 0x4E71);
        }
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000,
            ssp: 0x0000_1000,
            pc: 0x0C00,
            sr: 0xA000, // T=1, S=1, mask=0
            prefetch: [0x4E71, 0x4E71],
        };
        let mut cpu = Cpu68000::new(regs);

        cpu.step(&mut bus); // 1: the NOP runs (T latched); no interrupt pending yet
        assert_eq!(cpu.regs.pc, 0x0C02, "NOP executed");
        cpu.set_ipl(6); // interrupt now pending on the boundary

        cpu.step(&mut bus); // 2: TRACE is serviced first (outranks the interrupt)
        assert_eq!(cpu.regs.pc, 0x2000, "trace serviced first");
        assert_eq!(
            cpu.regs.sr & SR_TRACE,
            0,
            "T cleared by the trace's EnterException"
        );

        cpu.step(&mut bus); // 3: the INTERRUPT is serviced next; execution resumes in its handler
        assert_eq!(
            cpu.regs.pc, 0x4000,
            "interrupt serviced second → interrupt handler"
        );
        assert_eq!(cpu.regs.sr & SR_INT_MASK, 0x0600, "mask raised to 6");
        // The interrupt frame is pushed BELOW the trace frame (SSP 0x0FFA → 0x0FF4); its stacked PC is the
        // trace handler's start (0x2000) — "execution resumes in the interrupt handler" (§6.3.8), and RTE
        // from it returns into the trace handler.
        assert_eq!(
            cpu.regs.ssp, 0x0FF4,
            "second (interrupt) frame stacked below the trace frame"
        );
        assert_eq!(
            bus.peek(0x0FF8),
            0x20,
            "interrupt frame stacked the trace-handler PC (0x2000)"
        );
        assert_eq!(bus.peek(0x0FF9), 0x00, "…low byte");

        bus.log.clear();
        cpu.step(&mut bus); // 4: the interrupt handler's first instruction — NOT traced (T=0)
        assert_eq!(cpu.regs.pc, 0x4002, "handler NOP ran; not traced");
        assert!(
            !bus.log.iter().any(|t| t.addr == 0x24),
            "no trace exception in the interrupt handler"
        );
    }

    // --- Push A / A4.2: STOP wake. A Stopped CPU resumes on an interrupt whose level > mask; the wake
    // advances past the 2-word STOP (pc += 4) and the interrupt (its own reload refills the queue) stacks the
    // post-STOP PC. T=0 path only — fully pinned, independent of A3.2's T-fork.
    #[test]
    fn stopped_cpu_wakes_on_interrupt_and_stacks_the_post_stop_pc() {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000,
            ssp: 0x0000_1000,
            pc: 0x0C00,
            sr: 0x2700,                 // supervisor (STOP is privileged)
            prefetch: [0x4E72, 0x2000], // STOP #0x2000 (S=1, mask 0, T=0)
        };
        let mut bus = FlatBus::new();
        poke_w(&mut bus, 0x0C00, 0x4E72);
        poke_w(&mut bus, 0x0C02, 0x2000);
        for a in (0x0C04u32..0x0C10).step_by(2) {
            poke_w(&mut bus, a, 0x4E71);
        }
        poke_w(&mut bus, 0x7A, 0x4000); // vector 30 (L6) → 0x4000
        for a in [0x4000u32, 0x4002] {
            poke_w(&mut bus, a, 0x4E71);
        }
        let mut cpu = Cpu68000::new(regs);

        let c1 = cpu.step(&mut bus); // STOP → Stopped
        assert_eq!(c1, 4, "STOP = 4(0/0)");
        assert_eq!(cpu.regs.sr, 0x2000, "SR loaded (mask 0)");
        assert_eq!(cpu.state, CpuState::Stopped);
        assert_eq!(
            cpu.regs.pc, 0x0C00,
            "pc still at the STOP (advance deferred to the wake)"
        );

        cpu.set_ipl(6);
        cpu.step(&mut bus); // wake + interrupt
        assert_eq!(cpu.state, CpuState::Normal, "woke out of Stopped");
        assert_eq!(
            cpu.regs.pc, 0x4000,
            "reloaded at the L6 handler (vector 30)"
        );
        assert_eq!(cpu.regs.sr & SR_INT_MASK, 0x0600, "mask raised to 6");
        // Stacked PC = 0x0C04 (the instruction AFTER the 2-word STOP), NOT 0x0C00.
        assert_eq!(
            bus.peek(0x0FFE),
            0x0C,
            "stacked PC = 0x0C04 (post-STOP) high"
        );
        assert_eq!(bus.peek(0x0FFF), 0x04, "stacked PC low");
    }

    /// STOP #0x2700 at 0x0C00 (loads mask 7), an L6 handler, supervisor stack — for the "stays stopped" cases.
    fn stop_env(stop_imm: u16) -> (Cpu68000, FlatBus) {
        let regs = Registers {
            d: [0; 8],
            a: [0; 7],
            usp: 0x0000_3000,
            ssp: 0x0000_1000,
            pc: 0x0C00,
            sr: 0x2700,
            prefetch: [0x4E72, stop_imm],
        };
        let mut bus = FlatBus::new();
        poke_w(&mut bus, 0x0C00, 0x4E72);
        poke_w(&mut bus, 0x0C02, stop_imm);
        for a in (0x0C04u32..0x0C10).step_by(2) {
            poke_w(&mut bus, a, 0x4E71);
        }
        poke_w(&mut bus, 0x7A, 0x4000); // vector 30 (L6) → 0x4000
        for a in [0x4000u32, 0x4002] {
            poke_w(&mut bus, a, 0x4E71);
        }
        (Cpu68000::new(regs), bus)
    }

    #[test]
    fn stopped_cpu_stays_stopped_while_the_interrupt_is_masked() {
        // STOP #0x2700 → mask 7. An L6 request (6 ≤ 7) is masked → the CPU stays stopped, no bus activity.
        let (mut cpu, mut bus) = stop_env(0x2700);
        cpu.step(&mut bus); // STOP → Stopped (mask 7)
        assert_eq!(cpu.state, CpuState::Stopped);
        cpu.set_ipl(6);
        cpu.step(&mut bus); // masked → remains stopped
        assert_eq!(cpu.state, CpuState::Stopped, "L6 ≤ mask 7 → stays stopped");
        assert_eq!(cpu.regs.pc, 0x0C00, "pc unchanged while stopped");
        assert!(bus.log.is_empty(), "no bus activity while stopped-idle");
        // Once a higher request arrives (L7 > 7? no — level 7 needs the edge; use dropping the mask instead):
        // an L6 request now exceeds a lowered mask and wakes it.
        cpu.regs.set_int_mask(5); // mask now 5 < ipl 6
        cpu.step(&mut bus);
        assert_eq!(
            cpu.state,
            CpuState::Normal,
            "wakes once the pending level exceeds the mask"
        );
        assert_eq!(cpu.regs.pc, 0x4000, "serviced the L6 handler");
    }

    #[test]
    fn stopped_cpu_snapshot_mid_idle_wakes_identically() {
        // The boundary discipline applied to the one state where the CPU can sit indefinitely: snapshot/restore
        // a Stopped CPU partway through idle (nothing pending), then raise ipl — the wake must proceed
        // identically to an uninterrupted run.
        let cfg = bincode::config::standard();

        // Reference run: STOP → Stopped, then wake immediately.
        let (mut cpu_ref, mut bus_ref) = stop_env(0x2000); // mask 0
        cpu_ref.step(&mut bus_ref); // STOP
        cpu_ref.set_ipl(6);
        cpu_ref.step(&mut bus_ref); // wake + interrupt

        // Snapshot run: STOP → Stopped, idle-poll twice (nothing pending), snapshot, restore, THEN wake.
        let (mut cpu, mut bus) = stop_env(0x2000);
        cpu.step(&mut bus); // STOP
        assert_eq!(
            cpu.step(&mut bus),
            STOPPED_IDLE_SLICE,
            "idle poll 1 — stays stopped"
        );
        assert_eq!(
            cpu.step(&mut bus),
            STOPPED_IDLE_SLICE,
            "idle poll 2 — stays stopped"
        );
        assert_eq!(cpu.state, CpuState::Stopped, "still stopped mid-idle");
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut restored, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        restored.set_ipl(6);
        restored.step(&mut bus); // wake + interrupt, on the restored CPU

        assert_eq!(
            restored.regs, cpu_ref.regs,
            "the snapshot/restore-across-idle wake matches the uninterrupted wake"
        );
        assert_eq!(restored.state, cpu_ref.state, "both end Normal");
        // The interrupt frame is identical (post-STOP stacked PC).
        assert_eq!(bus.peek(0x0FFE), bus_ref.peek(0x0FFE));
        assert_eq!(bus.peek(0x0FFF), bus_ref.peek(0x0FFF));
    }

    // --- Push B / B6: the power-on reset sequence (M68000UM §6.3.1 + §6.2.1 + Yacht L1546) ----------------
    // /RESET 40(6/0): fetch SSP MSW/LSW @ $0/$2, PC MSW/LSW @ $4/$6, force S=1/T=0/I=7 (SR=0x2700), NO
    // stacking, refill the queue at PC. The reset vector is the ONE vector in supervisor PROGRAM space → all
    // six reads are FC=6 (not FC=5). Not vendored in SST — hand-authored vectors only.

    /// A FlatBus seeded with a reset vector table (SSP @ $0, PC @ $4) and handler words at the PC.
    fn reset_vector_bus() -> FlatBus {
        let mut bus = FlatBus::new();
        // SSP = 0x00FFFFF0 (hi word 0x00FF @ $0, lo word 0xFFF0 @ $2).
        for (a, v) in [(0u32, 0x00u8), (1, 0xFF), (2, 0xFF), (3, 0xF0)] {
            bus.poke(a, v);
        }
        // PC = 0x00000400 (hi 0x0000 @ $4, lo 0x0400 @ $6).
        for (a, v) in [(4u32, 0x00u8), (5, 0x00), (6, 0x04), (7, 0x00)] {
            bus.poke(a, v);
        }
        // Handler at $400: two NOP words for the prefetch refill.
        for (a, v) in [
            (0x400u32, 0x4Eu8),
            (0x401, 0x71),
            (0x402, 0x4E),
            (0x403, 0x71),
        ] {
            bus.poke(a, v);
        }
        bus
    }

    /// Registers in a deliberately non-supervisor, garbage state — reset must overwrite SSP/PC and force
    /// supervisor regardless of the prior state.
    fn pre_reset_regs() -> Registers {
        Registers {
            d: [0xDEAD_BEEF; 8],
            a: [0xBAAD_F00D; 7],
            usp: 0x1111_1111,
            ssp: 0x2222_2222,
            pc: 0x3333_3333,
            sr: 0x0000, // user mode (S=0), no mask — reset forces S=1/I=7
            prefetch: [0xAAAA, 0xBBBB],
        }
    }

    #[test]
    fn reset_recipe_fetches_ssp_and_pc_forces_supervisor_and_refills() {
        let mut bus = reset_vector_bus();
        let mut cpu = Cpu68000::new(pre_reset_regs());
        let mut recipe = crate::m68000::decode::reset_exception_recipe();
        let cycles = recipe.run_to_completion(&mut cpu.regs, &mut bus);

        assert_eq!(cycles, 40, "/RESET is 40(6/0) (Yacht L1546)");
        assert_eq!(cpu.regs.ssp, 0x00FF_FFF0, "SSP fetched from $0/$2");
        assert_eq!(cpu.regs.pc, 0x0000_0400, "PC fetched from $4/$6");
        assert_eq!(
            cpu.regs.sr, 0x2700,
            "forced supervisor (S=1), trace off (T=0), mask level 7"
        );
        assert_eq!(
            cpu.regs.prefetch,
            [0x4E71, 0x4E71],
            "prefetch queue refilled at PC"
        );
        // Six reads, ALL FC=6 (supervisor program — the reset vector's space), NO writes (no stacking).
        let reads: Vec<_> = bus
            .log
            .iter()
            .map(|t| (t.kind, t.fc, t.addr, t.value))
            .collect();
        assert_eq!(
            reads,
            vec![
                (TxKind::Read, 6, 0x0, 0x00FF),
                (TxKind::Read, 6, 0x2, 0xFFF0),
                (TxKind::Read, 6, 0x4, 0x0000),
                (TxKind::Read, 6, 0x6, 0x0400),
                (TxKind::Read, 6, 0x400, 0x4E71),
                (TxKind::Read, 6, 0x402, 0x4E71),
            ],
            "SSP($0/$2), PC($4/$6), two prefetches — all FC=6, no stack writes"
        );
        assert!(
            bus.log.iter().all(|t| t.kind == TxKind::Read),
            "reset stacks nothing (M68000UM §6.2.4)"
        );
    }

    #[test]
    fn reset_recipe_runs_identically_on_both_drivers() {
        // Fast path.
        let mut b1 = reset_vector_bus();
        let mut r1 = Cpu68000::new(pre_reset_regs());
        crate::m68000::decode::reset_exception_recipe().run_to_completion(&mut r1.regs, &mut b1);
        // Quiesce path (one micro-op at a time), snapshotting/restoring the recipe at every boundary.
        let mut b2 = reset_vector_bus();
        let mut r2 = Cpu68000::new(pre_reset_regs());
        r2.begin(crate::m68000::decode::reset_exception_recipe());
        while let Step::Continue = r2.step_micro_op(&mut b2) {}
        assert_eq!(
            r1.regs, r2.regs,
            "final register state matches across drivers"
        );
        assert_eq!(b1.log, b2.log, "transaction stream matches across drivers");
    }

    #[test]
    fn reset_wakes_a_stopped_cpu() {
        let mut bus = reset_vector_bus();
        let mut cpu = Cpu68000::new(pre_reset_regs());
        cpu.state = CpuState::Stopped;
        cpu.assert_reset();
        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 40, "the reset sequence ran");
        assert_eq!(cpu.state, CpuState::Normal, "reset left the Stopped state");
        assert_eq!(cpu.regs.pc, 0x0000_0400);
        assert_eq!(cpu.regs.ssp, 0x00FF_FFF0);
        assert_eq!(cpu.regs.sr, 0x2700);
    }

    #[test]
    fn reset_wakes_a_halted_cpu() {
        // The double-fault terminal state — only reset leaves it.
        let mut bus = reset_vector_bus();
        let mut cpu = Cpu68000::new(pre_reset_regs());
        cpu.state = CpuState::Halted;
        cpu.assert_reset();
        cpu.step(&mut bus);
        assert_eq!(cpu.state, CpuState::Normal, "only reset leaves Halted");
        assert_eq!(cpu.regs.pc, 0x0000_0400);
        assert_eq!(cpu.regs.sr, 0x2700);
    }

    #[test]
    fn reset_preempts_a_pending_interrupt() {
        // Reset is group 0 — the highest priority, above an unmasked interrupt.
        let mut bus = reset_vector_bus();
        let mut cpu = Cpu68000::new(pre_reset_regs());
        cpu.set_ipl(7); // an interrupt would otherwise be taken (pre-reset mask 0)
        cpu.assert_reset();
        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 40, "reset ran (40), not the 44-cycle interrupt");
        assert_eq!(
            cpu.regs.pc, 0x0000_0400,
            "PC from the reset vector, not an interrupt vector"
        );
        assert_eq!(cpu.regs.sr, 0x2700, "reset forced the mask to 7");
    }

    #[test]
    fn reset_pending_survives_snapshot_restore() {
        let mut cpu = Cpu68000::new(pre_reset_regs());
        cpu.assert_reset();
        let cfg = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&cpu, cfg).unwrap();
        let (mut restored, _): (Cpu68000, usize) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        // The restored CPU still services the reset on its next step.
        let mut bus = reset_vector_bus();
        restored.step(&mut bus);
        assert_eq!(
            restored.regs.pc, 0x0000_0400,
            "the latched reset round-tripped"
        );
    }
}
