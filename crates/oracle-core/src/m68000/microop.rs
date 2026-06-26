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
/// Prefetch`). 19 ≤ 20, so `MAX_OPS` is unchanged (using `SpPlus` instead of seven `EaCalc`s avoided a bump).
/// Public so the EA builder ([`super::ea::RecipeBuf`]) can size its fixed staging array to the same bound.
pub const MAX_OPS: usize = 20;

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
        }
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
                    _ => {}
                }
                // Compute at the operand-size flag boundary; carry the result (zero-extended to 32) + the new
                // low-byte CCR uniformly. MOVE is NOT arithmetic — it copies `a` and sets only N/Z (V/C
                // cleared) while PRESERVING X, so its `ccr` re-injects the live X bit (add/sub recompute X).
                let (result, ccr) = match op {
                    AluOp::MoveA | AluOp::Adda | AluOp::Suba => {
                        unreachable!("no-flag An-write op handled above")
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
                let refill = bus.read16(fetch_addr, regs.fc(true));
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
            MicroOp::LoadSr { value } => {
                // RTE's full-SR restore: the popped value masked to the implemented bits (0xA71F). Can switch
                // S (supervisor→user) / T — so the recipe runs the +6 stack pop BEFORE this, and any later
                // Prefetch reload follows the RESTORED mode's function code. NO bus, 0 cycles.
                regs.sr = (self.resolve(value, regs) as u16) & SR_IMPLEMENTED;
                0
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
}
