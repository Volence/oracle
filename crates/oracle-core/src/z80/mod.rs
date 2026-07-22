//! Zilog Z80 CPU core — the Genesis sound-driver host.
//!
//! This is the **Z-skeleton** slice (`docs/2026-07-22-z80-core-design.md`, ZC13): the architectural
//! register state, the [`Z80Io`] memory protocol, and the [`bus::Z80Bus`] Genesis adapter, wired into
//! [`crate::system::System`] and held in reset. **Nothing executes yet** — [`Z80::step`] is a deliberate
//! stub, and every committed fixture leaves the Z80 in reset (power-on `z80_running = false`), so it steps
//! zero instructions and every frozen currency stays byte-identical.
//!
//! Execution model (settled in the design, ZC1): instruction-atomic decode-execute over a fully
//! serializable struct — **not** the 68000's resumable micro-op recipe framework. The Z80's gate is
//! ZEXDOC/ZEXALL (architectural results at instruction boundaries), not SST's per-cycle bus trace, so no
//! sub-instruction cursor is needed; the whole Z80 is captured by the [`Z80`] struct between `step()` calls.
//! The full documented opcode set + the ZEXDOC harness land in the **Z-execute** slice.

pub mod bus;

pub use bus::Z80Bus;

/// The Z80 memory protocol — the analog of [`crate::m68000::bus68k::Bus68k`] for the sound CPU. [`Z80::step`]
/// is generic over it so the future ZEXDOC/ZEXALL harness can drive a bare [`Z80`] over a flat 64 KiB test
/// bus (ZC10) exactly as SST drives `Cpu68000` over `FlatBus`, while the running machine drives it over the
/// [`Z80Bus`] Genesis adapter (ZC12). The Z80's I/O-port space (`IN`/`OUT`) is unused on the Genesis and
/// lands with the harness; only the memory protocol is defined here.
pub trait Z80Io {
    /// Read one byte at the 16-bit Z80 address `addr`.
    fn read(&mut self, addr: u16) -> u8;
    /// Write one byte `value` at the 16-bit Z80 address `addr`.
    fn write(&mut self, addr: u16, value: u8);
}

/// The Z80's programmer-visible architectural + interrupt state (ZC8). Pure owned data with **no** `System`
/// or bus reference, so a snapshot is an O(struct) copy and the future ZEX harness can drive a bare `Z80`.
/// Register pairs are stored as `u16` (AF = A|F, etc.) so the main and shadow files round-trip exactly.
/// `Clone` + bincode `Encode`/`Decode` + `PartialEq`/`Eq`, mirroring
/// [`Registers`](crate::m68000::registers::Registers) / `Cpu68000`.
///
/// The undocumented-flag support fields (`wz` = MEMPTR, `q` = last-flag-write tracker) are present from the
/// start so the snapshot/export layout never churns, but are **inert** until the ZEXALL accuracy follow-up
/// (ZC11); the first executing version targets ZEXDOC (documented flags only).
#[derive(Clone, Debug, Default, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Z80 {
    // Main register file (A|F, B|C, D|E, H|L).
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    // Alternate (shadow) file — swapped by `EX AF,AF'` and `EXX`.
    af2: u16,
    bc2: u16,
    de2: u16,
    hl2: u16,
    // Index + control registers.
    ix: u16,
    iy: u16,
    sp: u16,
    pc: u16,
    /// Interrupt vector base (used by IM 2).
    i: u8,
    /// Memory-refresh counter (bit 7 is preserved across the per-M1 increment).
    r: u8,
    /// Interrupt-enable flip-flops.
    iff1: bool,
    iff2: bool,
    /// Interrupt mode (0/1/2).
    im: u8,
    /// `HALT` executed; waiting for an interrupt or reset.
    halted: bool,
    /// `/INT` asserted (VDP vblank), not yet taken.
    int_pending: bool,
    /// MEMPTR — drives the undocumented YF/XF of `BIT n,(HL)` etc. **RESERVED**, inert until ZEXALL (ZC11).
    wz: u16,
    /// Last-flag-write tracker for the `SCF`/`CCF` undocumented flags. **RESERVED**, inert until ZEXALL.
    q: u8,
}

// Flag-register bit masks (F layout, bits 7..0): `S Z YF H XF P/V N C`. YF/XF (bits 5/3) are the
// undocumented pair (ZEXALL scope, ZC11) — set here from the standard result-bit convention but masked
// out of the documented-flag gate.
const FLAG_C: u8 = 1 << 0; // carry / borrow
const FLAG_N: u8 = 1 << 1; // add/subtract (BCD)
const FLAG_PV: u8 = 1 << 2; // parity / overflow
const FLAG_XF: u8 = 1 << 3; // undocumented (bit 3 of result)
const FLAG_H: u8 = 1 << 4; // half carry / half borrow
const FLAG_YF: u8 = 1 << 5; // undocumented (bit 5 of result)
const FLAG_Z: u8 = 1 << 6; // zero
const FLAG_S: u8 = 1 << 7; // sign
/// The undocumented copies `YF`/`XF` taken from the result's bits 5/3 in an ordinary ALU op.
const FLAG_XY: u8 = FLAG_XF | FLAG_YF;

/// Which index register a `DD`/`FD` prefix selects for the opcode it prefixes (ZC3b). `(HL)` becomes
/// `(IX+d)`/`(IY+d)` and the `H`/`L` halves become `IXH`/`IXL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexReg {
    Ix,
    Iy,
}

/// A flat, public view of the Z80's architectural register state — for out-of-band validation (the
/// SST-z80 harness and the design's "introspect `z80_ram`/registers" driver check, ZC13) and future
/// debugger introspection. The main 8-bit registers are exposed individually and the shadow file as
/// 16-bit pairs, matching the SST-z80 corpus's field layout so a harness maps 1:1. This is the Z80
/// analog of the public [`Registers`](crate::m68000::registers::Registers) the 68000 SST runner builds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Z80Regs {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub af_: u16,
    pub bc_: u16,
    pub de_: u16,
    pub hl_: u16,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub iff1: bool,
    pub iff2: bool,
    pub im: u8,
    pub halted: bool,
    pub wz: u16,
    pub q: u8,
}

impl Z80 {
    /// Build a `Z80` from a flat register view (test/introspection entry point; `int_pending` starts
    /// clear — the SST-z80 corpus has no pending-interrupt input).
    pub fn from_regs(r: &Z80Regs) -> Self {
        Self {
            af: ((r.a as u16) << 8) | r.f as u16,
            bc: ((r.b as u16) << 8) | r.c as u16,
            de: ((r.d as u16) << 8) | r.e as u16,
            hl: ((r.h as u16) << 8) | r.l as u16,
            af2: r.af_,
            bc2: r.bc_,
            de2: r.de_,
            hl2: r.hl_,
            ix: r.ix,
            iy: r.iy,
            sp: r.sp,
            pc: r.pc,
            i: r.i,
            r: r.r,
            iff1: r.iff1,
            iff2: r.iff2,
            im: r.im,
            halted: r.halted,
            int_pending: false,
            wz: r.wz,
            q: r.q,
        }
    }

    /// Read the architectural register state as a flat view (the inverse of [`Z80::from_regs`]).
    pub fn regs(&self) -> Z80Regs {
        Z80Regs {
            a: (self.af >> 8) as u8,
            f: self.af as u8,
            b: (self.bc >> 8) as u8,
            c: self.bc as u8,
            d: (self.de >> 8) as u8,
            e: self.de as u8,
            h: (self.hl >> 8) as u8,
            l: self.hl as u8,
            af_: self.af2,
            bc_: self.bc2,
            de_: self.de2,
            hl_: self.hl2,
            ix: self.ix,
            iy: self.iy,
            sp: self.sp,
            pc: self.pc,
            i: self.i,
            r: self.r,
            iff1: self.iff1,
            iff2: self.iff2,
            im: self.im,
            halted: self.halted,
            wz: self.wz,
            q: self.q,
        }
    }

    /// Power on in the reset state (ZC9): the Z80 `/RESET` strictly defines `PC = 0`, `I = 0`, `R = 0`,
    /// `IFF1 = IFF2 = 0`, `IM = 0`, not halted; SP and the main/index registers are architecturally
    /// undefined and pinned here to **all-zero** (a legitimate reset model that keeps `export_state`
    /// region 4 frozen at go-live — see the design's reset-fill call). Identical to [`Z80::default`].
    pub fn new() -> Self {
        Self::default()
    }

    // ---- 8-bit register accessors over the packed pairs (high byte = first-named register). ----
    fn a(&self) -> u8 {
        (self.af >> 8) as u8
    }
    fn set_a(&mut self, v: u8) {
        self.af = (self.af & 0x00FF) | ((v as u16) << 8);
    }
    fn flags(&self) -> u8 {
        self.af as u8
    }
    fn set_flags(&mut self, f: u8) {
        self.af = (self.af & 0xFF00) | f as u16;
    }

    /// Read one of the eight 8-bit operands by its 3-bit encoding (`0=B 1=C 2=D 3=E 4=H 5=L 6=(HL) 7=A`).
    /// Encoding `6` is the memory operand `(HL)`, read over `bus`.
    fn reg8_get<B: Z80Io>(&mut self, sel: u8, bus: &mut B) -> u8 {
        match sel & 7 {
            0 => (self.bc >> 8) as u8,
            1 => self.bc as u8,
            2 => (self.de >> 8) as u8,
            3 => self.de as u8,
            4 => (self.hl >> 8) as u8,
            5 => self.hl as u8,
            6 => bus.read(self.hl),
            _ => self.a(),
        }
    }

    /// Write one of the eight 8-bit operands by its 3-bit encoding. Encoding `6` writes `(HL)` over `bus`.
    fn reg8_set<B: Z80Io>(&mut self, sel: u8, val: u8, bus: &mut B) {
        match sel & 7 {
            0 => self.bc = (self.bc & 0x00FF) | ((val as u16) << 8),
            1 => self.bc = (self.bc & 0xFF00) | val as u16,
            2 => self.de = (self.de & 0x00FF) | ((val as u16) << 8),
            3 => self.de = (self.de & 0xFF00) | val as u16,
            4 => self.hl = (self.hl & 0x00FF) | ((val as u16) << 8),
            5 => self.hl = (self.hl & 0xFF00) | val as u16,
            6 => bus.write(self.hl, val),
            _ => self.set_a(val),
        }
    }

    /// The per-M1 refresh-counter increment: bits 0..6 count, bit 7 is preserved (UM008 §"R").
    fn inc_r(&mut self) {
        self.r = (self.r & 0x80) | (self.r.wrapping_add(1) & 0x7F);
    }

    /// Fetch an M1 opcode byte at `PC`, advancing `PC` and bumping the refresh counter (one M1 cycle).
    fn next_opcode<B: Z80Io>(&mut self, bus: &mut B) -> u8 {
        let b = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        self.inc_r();
        b
    }

    /// Fetch a non-M1 operand/displacement byte at `PC`, advancing `PC` only (no refresh bump).
    fn next_byte<B: Z80Io>(&mut self, bus: &mut B) -> u8 {
        let b = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        b
    }

    /// Execute one Z80 instruction over `bus`, returning the T-states it consumed (the value the ×15 mclk
    /// catch-up in [`crate::system::System::run_until`] scales into the frontier).
    ///
    /// Instruction-atomic decode-execute (ZC1/ZC2): one non-yielding call that fetches the opcode, walks the
    /// prefix front end (ZC3b), runs the handler, and returns at the instruction boundary with all state in
    /// the struct. This slice implements the documented base opcodes `NOP` (`0x00`) + the 8-bit `LD` block
    /// (`0x40-0x7F`, incl. `HALT` `0x76`) + the 8-bit ALU `A,r` block (`0x80-0xBF`); the prefix groups
    /// (`CB`/`ED`/`DD`/`FD`/`DDCB`/`FDCB`) are decoded structurally but their leaf handlers are the next
    /// slice (Z-execute), and the rest of the base table is not yet decoded.
    pub fn step<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        if self.halted {
            // HALT idle: the CPU runs internal NOPs (refresh continues) until an interrupt/reset clears
            // `halted` (interrupt acceptance is a later slice). Not exercised by SST (no halted-initial case).
            self.inc_r();
            return 4;
        }
        let opcode = self.next_opcode(bus);
        self.execute(opcode, bus)
    }

    /// The prefix-accumulating front end (ZC3b): `CB`/`ED` select an alternate table, `DD`/`FD` set an
    /// index-register override for the following opcode, and `DDCB`/`FDCB` fetch the displacement **before**
    /// the final opcode byte. The alternate-table and index-override leaf handlers are stubbed this slice.
    fn execute<B: Z80Io>(&mut self, opcode: u8, bus: &mut B) -> u32 {
        match opcode {
            0xCB => self.execute_cb(bus),
            0xED => self.execute_ed(bus),
            0xDD => self.execute_indexed(IndexReg::Ix, bus),
            0xFD => self.execute_indexed(IndexReg::Iy, bus),
            _ => self.execute_base(opcode, bus),
        }
    }

    /// Base-table (unprefixed) opcodes. Implemented this slice: `NOP`, the `LD r,r'`/`LD r,(HL)`/
    /// `LD (HL),r`/`HALT` block (`0x40-0x7F`), and the 8-bit ALU `A,r|(HL)` block (`0x80-0xBF`).
    fn execute_base<B: Z80Io>(&mut self, opcode: u8, bus: &mut B) -> u32 {
        match opcode {
            0x00 => 4, // NOP
            0x40..=0x7F => self.op_ld_block(opcode, bus),
            0x80..=0xBF => self.op_alu_block(opcode, bus),
            other => unimplemented!(
                "Z80 base opcode {other:#04X} is not decoded yet (this slice: NOP + 0x40-0xBF)"
            ),
        }
    }

    /// The 8-bit load block `0x40-0x7F`: `LD dst,src` with `dst = (op>>3)&7`, `src = op&7` under the
    /// standard `0=B..7=A` encoding (`6 = (HL)`), except `0x76` (`dst==src==6`) which is `HALT`. Loads never
    /// touch the flags. Timing: 7 T-states if either operand is `(HL)`, else 4.
    fn op_ld_block<B: Z80Io>(&mut self, opcode: u8, bus: &mut B) -> u32 {
        let dst = (opcode >> 3) & 7;
        let src = opcode & 7;
        if dst == 6 && src == 6 {
            self.halted = true;
            return 4;
        }
        let v = self.reg8_get(src, bus);
        self.reg8_set(dst, v, bus);
        if src == 6 || dst == 6 {
            7
        } else {
            4
        }
    }

    /// The 8-bit ALU block `0x80-0xBF`: `op = (opcode>>3)&7` selects
    /// `0=ADD 1=ADC 2=SUB 3=SBC 4=AND 5=XOR 6=OR 7=CP`, `src = opcode&7` selects the operand (`6=(HL)`).
    /// All operate on `A` (accumulator) and set the documented flags. Timing: 7 if `(HL)`, else 4.
    fn op_alu_block<B: Z80Io>(&mut self, opcode: u8, bus: &mut B) -> u32 {
        let op = (opcode >> 3) & 7;
        let src = opcode & 7;
        let s = self.reg8_get(src, bus);
        self.alu8(op, s);
        if src == 6 {
            7
        } else {
            4
        }
    }

    /// Apply an 8-bit ALU operation `A op s`, storing the result into `A` (except `CP`, which discards the
    /// result and keeps `A`) and updating `F` with the documented flags (ZC14). `YF`/`XF` are set from the
    /// result's bits 5/3 (the ordinary-op convention) but are masked out of the documented-flag gate; the
    /// operand-sourced `YF`/`XF` of `CP` are ZEXALL scope (ZC11).
    fn alu8(&mut self, op: u8, s: u8) {
        let a = self.a();
        let carry_in = self.flags() & FLAG_C; // 0 or 1
        match op {
            0 => {
                let (r, f) = add8(a, s, 0);
                self.set_a(r);
                self.set_flags(f);
            }
            1 => {
                let (r, f) = add8(a, s, carry_in);
                self.set_a(r);
                self.set_flags(f);
            }
            2 => {
                let (r, f) = sub8(a, s, 0);
                self.set_a(r);
                self.set_flags(f);
            }
            3 => {
                let (r, f) = sub8(a, s, carry_in);
                self.set_a(r);
                self.set_flags(f);
            }
            4 => {
                let (r, f) = logic8(a & s, true);
                self.set_a(r);
                self.set_flags(f);
            }
            5 => {
                let (r, f) = logic8(a ^ s, false);
                self.set_a(r);
                self.set_flags(f);
            }
            6 => {
                let (r, f) = logic8(a | s, false);
                self.set_a(r);
                self.set_flags(f);
            }
            _ => {
                // CP: compare (A − s), discard the result, keep A; flags as for SUB.
                let (_r, f) = sub8(a, s, 0);
                self.set_flags(f);
            }
        }
    }

    // ---- Prefix leaf handlers — structurally reached, opcode bodies land in the Z-execute slice. ----

    /// `CB`-prefixed rotate/shift/bit ops. Structurally reached; bodies are the Z-execute slice.
    fn execute_cb<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        let sub = self.next_opcode(bus);
        unimplemented!("Z80 CB-prefixed opcode {sub:#04X} is the Z-execute slice")
    }

    /// `ED`-prefixed extended ops. Structurally reached; bodies are the Z-execute slice.
    fn execute_ed<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        let sub = self.next_opcode(bus);
        unimplemented!("Z80 ED-prefixed opcode {sub:#04X} is the Z-execute slice")
    }

    /// `DD`/`FD`-prefixed index-register (`IX`/`IY`) forms. A `DD`/`FD` sets an override for the following
    /// opcode; a run of `DD`/`FD` collapses (each is one M1, the last winning). `DDCB`/`FDCB` fetch the
    /// displacement `d` **before** the final opcode byte (ZC3b) — that irregular order is honored here. The
    /// leaf bodies (index-overridden base, `DDCB`/`FDCB`) are the Z-execute slice.
    fn execute_indexed<B: Z80Io>(&mut self, idx: IndexReg, bus: &mut B) -> u32 {
        let sub = self.next_opcode(bus);
        match sub {
            0xDD => self.execute_indexed(IndexReg::Ix, bus),
            0xFD => self.execute_indexed(IndexReg::Iy, bus),
            0xED => self.execute_ed(bus), // a following ED ignores the index prefix (documented)
            0xCB => {
                // DDCB/FDCB: displacement precedes the opcode byte (neither is an M1 refresh cycle).
                let d = self.next_byte(bus) as i8;
                let op = self.next_byte(bus);
                self.execute_ddcb(idx, d, op)
            }
            other => unimplemented!(
                "Z80 {idx:?}-prefixed base opcode {other:#04X} is the Z-execute slice"
            ),
        }
    }

    /// `DDCB`/`FDCB` indexed bit/shift ops (displacement already fetched). Body is the Z-execute slice.
    fn execute_ddcb(&mut self, idx: IndexReg, d: i8, op: u8) -> u32 {
        unimplemented!("Z80 {idx:?}CB opcode {op:#04X} (d={d}) is the Z-execute slice")
    }
}

/// 8-bit add with carry-in (`ADD`/`ADC`): returns `(result, flags)` with documented flags exact.
fn add8(a: u8, s: u8, carry: u8) -> (u8, u8) {
    let sum = a as u16 + s as u16 + carry as u16;
    let result = sum as u8;
    let mut f = 0u8;
    if result & 0x80 != 0 {
        f |= FLAG_S;
    }
    if result == 0 {
        f |= FLAG_Z;
    }
    if ((a & 0x0F) + (s & 0x0F) + carry) & 0x10 != 0 {
        f |= FLAG_H;
    }
    // Overflow: operands share a sign that differs from the result's sign.
    if (a ^ result) & (s ^ result) & 0x80 != 0 {
        f |= FLAG_PV;
    }
    // N = 0 (add).
    if sum & 0x100 != 0 {
        f |= FLAG_C;
    }
    f |= result & FLAG_XY;
    (result, f)
}

/// 8-bit subtract with borrow-in (`SUB`/`SBC`/`CP`): returns `(result, flags)` with documented flags exact.
fn sub8(a: u8, s: u8, carry: u8) -> (u8, u8) {
    let diff = a as i16 - s as i16 - carry as i16;
    let result = diff as u8;
    let mut f = FLAG_N; // N = 1 (subtract).
    if result & 0x80 != 0 {
        f |= FLAG_S;
    }
    if result == 0 {
        f |= FLAG_Z;
    }
    let half = (a as i16 & 0x0F) - (s as i16 & 0x0F) - carry as i16;
    if half & 0x10 != 0 {
        f |= FLAG_H;
    }
    // Overflow: operands differ in sign and the result's sign differs from the minuend's.
    if (a ^ s) & (a ^ result) & 0x80 != 0 {
        f |= FLAG_PV;
    }
    if diff < 0 {
        f |= FLAG_C;
    }
    f |= result & FLAG_XY;
    (result, f)
}

/// 8-bit logic result flags (`AND`/`OR`/`XOR`): `H = half` (`AND` sets H, `OR`/`XOR` clear it), `P/V` =
/// even parity, `N = C = 0`. Returns `(result, flags)`.
fn logic8(result: u8, half: bool) -> (u8, u8) {
    let mut f = 0u8;
    if result & 0x80 != 0 {
        f |= FLAG_S;
    }
    if result == 0 {
        f |= FLAG_Z;
    }
    if half {
        f |= FLAG_H;
    }
    if result.count_ones().is_multiple_of(2) {
        f |= FLAG_PV; // parity even
    }
    f |= result & FLAG_XY;
    (result, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_on_reset_state_is_all_zero() {
        // ZC9 reset model: PC/I/R/IFF/IM/HALT defined to 0, SP + main/index registers pinned to all-zero.
        // This is what keeps export_state region 4 frozen through go-live.
        let z = Z80::new();
        assert_eq!(z, Z80::default());
        let bytes = bincode::encode_to_vec(&z, bincode::config::standard())
            .expect("Z80 is infallibly encodable");
        assert!(
            bytes.iter().all(|&b| b == 0),
            "the reset-state Z80 serializes as all-zero"
        );
    }

    #[test]
    fn snapshot_round_trips_every_register() {
        // A fully-populated Z80 (every field distinct) must survive bincode encode/decode byte-for-byte, so
        // the determinism/rewind snapshot carries the Z80 exactly (ZC8/ZC9 currency 1). Fields are private,
        // so drive them through the same serialization the snapshot uses.
        // Distinct sentinels in every pair + control byte so a dropped/reordered field would be caught.
        let populated = Z80 {
            af: 0x0102,
            bc: 0x0304,
            de: 0x0506,
            hl: 0x0708,
            af2: 0x090A,
            bc2: 0x0B0C,
            de2: 0x0D0E,
            hl2: 0x0F10,
            ix: 0x1112,
            iy: 0x1314,
            sp: 0x1516,
            pc: 0x1718,
            i: 0x19,
            r: 0x1A,
            iff1: true,
            iff2: false,
            im: 2,
            halted: true,
            int_pending: true,
            wz: 0x1B1C,
            q: 0x1D,
        };
        let bytes =
            bincode::encode_to_vec(&populated, bincode::config::standard()).expect("encodable");
        let (back, _len): (Z80, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).expect("decodable");
        assert_eq!(
            back, populated,
            "every Z80 register round-trips through bincode"
        );
    }
}
