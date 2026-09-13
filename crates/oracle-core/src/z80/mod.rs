//! Zilog Z80 CPU core — the Genesis sound-driver host.
//!
//! Built on the **Z-skeleton** slice (`docs/2026-07-22-z80-core-design.md`, ZC13): the architectural
//! register state, the [`Z80Io`] memory protocol, and the [`bus::Z80Bus`] Genesis adapter, wired into
//! [`crate::system::System`] and held in reset. The **Z-execute** opcode grind is now under way — the
//! documented base table lands incrementally, gated by the SingleStepTests/z80 corpus. Currency-neutral by
//! construction: every committed fixture leaves the Z80 in reset (power-on `z80_running = false`), so it
//! steps zero instructions and every frozen currency stays byte-identical; the opcodes are validated
//! out-of-band by the isolated SST-z80 harness (a bare [`Z80`] + flat test bus, never `System`).
//!
//! Execution model (settled in the design, ZC1): instruction-atomic decode-execute over a fully
//! serializable struct — **not** the 68000's resumable micro-op recipe framework. The Z80's gate is
//! SingleStepTests/z80 (architectural results at instruction boundaries), not SST's per-cycle bus trace, so
//! no sub-instruction cursor is needed; the whole Z80 is captured by the [`Z80`] struct between `step()`
//! calls. Coverage so far: the whole documented **un-prefixed base table** (`NOP`, the data/arith/rotate/misc
//! ops, the 8-bit `LD`/ALU blocks, and the branch/stack control flow), the **full `CB`-prefixed group**
//! (the rotates/shifts, `BIT`/`RES`/`SET`), the **documented `ED`-prefixed subset** (the 16-bit
//! arithmetic/loads, `NEG`, `RETN`/`RETI`, `IM`, the `I`/`R` loads, `RRD`/`RLD`, `IN r,(C)`/`OUT (C),r`, and
//! the block transfer/search/I/O groups), the **documented `DD`/`FD` (`IX`/`IY`) base ops**, and the
//! **documented `DDCB`/`FDCB` group** (the `(IX+d)`/`(IY+d)` rotates/shifts, `BIT`/`RES`/`SET`) — which
//! completes the **documented Z80 instruction set**, plus the undocumented `ED` **holes** (NONI) and the 20
//! undocumented `ED` **mirrors** (`NEG`/`RETN`/`IM` mirrors and the flags-only `IN (C)` / `OUT (C),0` pair),
//! each graded 1000/1000 against the SingleStepTests corpus, the 46+46 undocumented `IXH`/`IXL`
//! half-register forms, and the 224+224 undocumented `DDCB`/`FDCB` register-copy variants — **the whole
//! 256-opcode space of every table, documented and undocumented alike**, graded against all 1268 files of
//! the pinned corpus (1,268,000 cases). Nothing is deferred and nothing can panic from a guest byte; the
//! [`Z80Fault`] refusal remains as the landing pad for any future deferral (see that type for why a
//! structural refusal, not a `catch_unwind`).

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
    /// Read one byte from the 16-bit I/O port `port` (the `IN` instructions). The Genesis Z80 leaves the
    /// I/O-port space unused (Plutiedev), so the [`Z80Bus`] adapter stubs it as open bus; the SST-z80
    /// harness services it from each case's `ports` list. For `IN A,(n)`/`OUT (n),A` the port high byte is
    /// `A` and the low byte the immediate `n`.
    fn input(&mut self, port: u16) -> u8;
    /// Write one byte `value` to the 16-bit I/O port `port` (the `OUT` instructions).
    fn output(&mut self, port: u16, value: u8);
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
    /// **Latched refusal**: the encoding this core could not execute (see [`Z80Fault`]). `None` is the
    /// normal state. Once set, [`Z80::step`] executes nothing — it burns time so the caller's catch-up
    /// loop still terminates, but the guest program does not advance. Cleared only by [`Z80::reset`]
    /// (a hardware `/RESET` genuinely restarts the CPU, and a driver re-upload deserves a fresh start).
    ///
    /// Deliberately **not** in [`Z80Regs`] or [`Z80::export_region`]: it is not an architectural
    /// register, the SST corpus has no field for it, and region 4's 30-byte export layout must not move.
    /// It rides the internal bincode snapshot like every other non-architectural scalar.
    fault: Option<Z80Fault>,
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

/// **A guest byte the Z80 core cannot execute** — the latched, named refusal that replaced three
/// `unimplemented!()` sites.
///
/// **Why this exists and why it is not a `catch_unwind`.** Until this type, three encoding classes — the
/// undocumented `ED` mirrors, the `IXH`/`IXL` half-register forms, and the `DDCB`/`FDCB` register-copy
/// variants: **560 real instructions**, not illegal opcodes — reached `unimplemented!()`, and nothing in
/// this repo caught it. One byte in a guest's Z80 program killed the thread: in the player the window dies
/// mid-session, in the server the engine thread dies while the socket stays bound (an outage
/// indistinguishable from a hang). The precedent is in-tree: Vectorman's `FD FF` pinned the emulator at 26
/// frames until the prefix rule landed.
///
/// A `catch_unwind` at the handler boundary would catch the *symptom* while leaving the machine in an
/// unknown half-stepped state, and would evaporate silently under `panic = "abort"`. This is the
/// structural alternative: the core **refuses** at decode time, before any bus write, latches what it
/// refused, rewinds `PC` to the first byte of the offending instruction, and stops.
///
/// **It is a refusal, never a degradation.** Treating an unserved encoding as a `NOP` is the failure mode
/// this type exists to prevent: a Z80 sound driver that quietly runs *wrong* is far worse here than one
/// that stops and says which byte it could not run. (Contrast the documented `ED` **holes**, which really
/// are `NOP`s on hardware and are executed as such — that is a served behaviour, not a degradation.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Z80Fault {
    /// Address of the **first** byte of the refused instruction (its prefix), not of the byte the decoder
    /// happened to be holding. [`Z80::step`] rewinds `PC` here, so the machine points at what it refused.
    pub pc: u16,
    /// The encoding, prefix bytes first: `[ED, op]`, `[DD|FD, op]`, or `[DD|FD, CB, d, op]`. Only the
    /// first [`len`](Self::len) entries are meaningful.
    pub bytes: [u8; 4],
    /// How many of [`bytes`](Self::bytes) are meaningful (2 or 4).
    pub len: u8,
}

impl core::fmt::Display for Z80Fault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Z80 refused an unimplemented encoding")?;
        for b in &self.bytes[..self.len as usize] {
            write!(f, " {b:02X}")?;
        }
        write!(
            f,
            " at PC ${:04X}; the Z80 is stopped (it will not run wrong)",
            self.pc
        )
    }
}

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
            fault: None,
        }
    }

    /// The encoding this core refused, if any (see [`Z80Fault`]). `Some` means the Z80 is **stopped**: it
    /// hit a byte it cannot execute, and it is deliberately not guessing. Surfaced by
    /// [`System::z80_fault`](crate::system::System::z80_fault).
    pub fn fault(&self) -> Option<Z80Fault> {
        self.fault
    }

    /// Test-only: latch `fault` as though the core had refused an encoding, so a test can put a refusal the
    /// live decoder never raises (every encoding is served today) into a snapshot.
    #[cfg(test)]
    pub(crate) fn latch_fault_for_test(&mut self, fault: Z80Fault) {
        self.fault = Some(fault);
    }

    /// Latch a refusal (see [`Z80Fault`]) for an encoding no handler serves.
    ///
    /// Called from the decode arms **before any bus write or register mutation**, so the only state the
    /// aborted instruction leaves behind is the `PC` advance and the `R` refresh bumps of the fetches it
    /// already performed. [`Z80::step`] rewinds `PC` to the instruction's first byte; the `R` bumps are
    /// left alone on purpose — those M1 refresh cycles genuinely happened on hardware, and unwinding them
    /// would be inventing a machine state rather than reporting one.
    ///
    /// Returns a T-state count so the arm's `u32` contract is unchanged and the caller's catch-up loop
    /// still advances: the fetches really did consume M1 cycles, so 4 per fetched byte is charged.
    /// Only the **first** refusal is kept — a later `step` never gets far enough to raise a second.
    ///
    /// ⚑ **Currently no caller, and that is the point — do not delete it.** Every encoding in the base
    /// table and all five prefixed tables is now served, so the *decode* side of the guarantee is enforced
    /// by exhaustive `match`es rather than by a runtime refusal. This method is the standing landing pad
    /// for the next deferral. Deleting it because it is unused is precisely how the defect it fixed was
    /// born: three encoding classes were deferred with `unimplemented!()` because there was nowhere else
    /// for a deferral to go, and 560 real instructions could kill the process from a guest byte. The next
    /// person who defers an opcode must reach for this, not for a panic. Its behaviour stays under test
    /// (see `a_latched_refusal_stops_the_core` below).
    #[allow(dead_code)]
    fn refuse(&mut self, bytes: [u8; 4], len: u8) -> u32 {
        if self.fault.is_none() {
            // `pc` is rewritten by `step` to the instruction's first byte; this placeholder is never read.
            self.fault = Some(Z80Fault {
                pc: self.pc,
                bytes,
                len,
            });
        }
        4 * len as u32
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

    /// The 30-byte export-golden layout for `export_state` region 4 (ZC9), in a fixed little-endian order.
    /// The architectural register file, packed:
    ///
    /// | Bytes | Field |
    /// |---|---|
    /// | 8 | AF, BC, DE, HL (each LE `u16`) |
    /// | 8 | AF', BC', DE', HL' |
    /// | 4 | IX, IY |
    /// | 4 | SP, PC |
    /// | 2 | I, R |
    /// | 1 | IFF1·IFF2·IM packed (`iff1<<0 | iff2<<1 | im<<2`) |
    /// | 1 | HALT flag (0/1) |
    /// | 2 | WZ |
    ///
    /// At the reset state every field is zero (ZC9 all-zero reset model), so this emits all-zero bytes and the
    /// export golden does not move at Z-live go-live. `System::export_state` copies these 30 bytes and pads to
    /// the reserved `0x40` region (>2× margin).
    pub fn export_region(&self) -> [u8; 30] {
        let mut b = [0u8; 30];
        let mut w = |off: usize, v: u16| b[off..off + 2].copy_from_slice(&v.to_le_bytes());
        w(0, self.af);
        w(2, self.bc);
        w(4, self.de);
        w(6, self.hl);
        w(8, self.af2);
        w(10, self.bc2);
        w(12, self.de2);
        w(14, self.hl2);
        w(16, self.ix);
        w(18, self.iy);
        w(20, self.sp);
        w(22, self.pc);
        b[24] = self.i;
        b[25] = self.r;
        b[26] = (self.iff1 as u8) | ((self.iff2 as u8) << 1) | (self.im << 2);
        b[27] = self.halted as u8;
        b[28..30].copy_from_slice(&self.wz.to_le_bytes());
        b
    }

    /// Power on in the reset state (ZC9): the Z80 `/RESET` strictly defines `PC = 0`, `I = 0`, `R = 0`,
    /// `IFF1 = IFF2 = 0`, `IM = 0`, not halted; SP and the main/index registers are architecturally
    /// undefined and pinned here to **all-zero** (a legitimate reset model that keeps `export_state`
    /// region 4 frozen at go-live — see the design's reset-fill call). Identical to [`Z80::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a hardware `/RESET` to a **running** core — what the 68000 does by driving `$A11200` bit0 to 0
    /// (`MegaDriveBus::store_byte`). Until this existed, the `$A11200` write moved only the `z80_running`
    /// clock gate and the core kept every register it had, so a driver that reset its sound CPU resumed
    /// mid-stream instead of restarting; worst of all, a core sitting in `HALT` with `IFF1 = 0` when reset
    /// was asserted stayed halted **forever**, because [`Z80::accept_interrupt`] is the only other place
    /// that clears `halted` and a masked `/INT` never gets there. That is silent, permanent sound loss.
    ///
    /// **State model — Z80 UM008 §"RESET", as already pinned by this repo in
    /// `docs/2026-07-22-z80-core-design.md` (ZC9): the reset "*defines* PC=0, I=0, R=0, IFF1=IFF2=0, IM=0;
    /// SP and the main/index registers are architecturally undefined".** The same model, plus the edge rule
    /// and what is still unmodeled, is written down for the *line* in
    /// `docs/2026-07-22-z80-busreq-recon.md` (Z4) — which is also where the latch-only model came from, and
    /// why it survived: Z4 deferred the core reset on the ground that "the reset line only ever gates Z80
    /// execution, which does not exist yet", and nobody returned when Z-execute landed. So this clears exactly the defined
    /// set — `PC`, `I`, `R`, both interrupt flip-flops, the interrupt mode — and un-halts, and it
    /// deliberately **preserves** the register file (`AF`/`BC`/`DE`/`HL`, their shadows, `IX`/`IY`, `SP`):
    /// the Z80's register file is static storage that `/RESET` does not drive, and clearing state the
    /// hardware leaves alone would be the worse error in the generous direction. The internal `WZ`/`Q`
    /// support fields are internal machine state, not the programmer's register file, so they clear with
    /// the defined set.
    ///
    /// Two consequences worth stating because they are load-bearing:
    ///
    /// - **It agrees with ZC9's all-zero power-on pin rather than replacing it.** [`Z80::new`] is already
    ///   the reset state with every undefined register pinned to zero, so `reset()` applied at power-on is
    ///   the identity — `export_state` region 4 stays all-zero and the export golden cannot move.
    /// - **The hardware-faithful `AF = SP = $FFFF` variant is a deliberate non-choice, not an oversight.**
    ///   It is reported on real silicon but is outside UM008's defined list and outside ZC9's pin, and
    ///   adopting it would move the export golden; ZC9 says that fork is decided at Z-live and "not allowed
    ///   to drift", so it is not decided here.
    ///
    /// `int_pending` is **not** touched: it mirrors the *level* of the external `/INT` line (the VDP's
    /// vblank assert, `System::run_frames`), and resetting the CPU does not deassert a line another chip is
    /// driving. It is harmless while held — `IFF1 = 0` masks it — and `System` clears it at the next frame
    /// start.
    pub fn reset(&mut self) {
        self.pc = 0;
        self.i = 0;
        self.r = 0;
        self.iff1 = false;
        self.iff2 = false;
        self.im = 0;
        self.halted = false;
        self.wz = 0;
        self.q = 0;
        // A latched refusal (`Z80Fault`) clears here and only here. `/RESET` genuinely restarts the CPU at
        // `PC = 0`, and the 68000 driving `$A11200` bit0 is usually about to upload a *different* Z80
        // program; refusing to run the new one because the old one contained a byte we could not execute
        // would turn a recoverable stop into a permanent one.
        self.fault = None;
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
        // SPIKE (M24): the timing-neutral state control `M24_MUT=r` advances R by 2.
        let step = if crate::spike_m24::mutation() == crate::spike_m24::Mutation::NeutralR {
            crate::spike_m24::note_mut_fired();
            2
        } else {
            1
        };
        self.r = (self.r & 0x80) | (self.r.wrapping_add(step) & 0x7F);
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

    /// Fetch a little-endian 16-bit immediate at `PC` (low byte first), advancing `PC` by two.
    fn next_word<B: Z80Io>(&mut self, bus: &mut B) -> u16 {
        let lo = self.next_byte(bus) as u16;
        let hi = self.next_byte(bus) as u16;
        (hi << 8) | lo
    }

    /// Read a little-endian 16-bit word from `addr`/`addr+1`.
    fn read16<B: Z80Io>(&mut self, addr: u16, bus: &mut B) -> u16 {
        let lo = bus.read(addr) as u16;
        let hi = bus.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    /// Write a little-endian 16-bit word to `addr`/`addr+1` (low byte first).
    fn write16<B: Z80Io>(&mut self, addr: u16, val: u16, bus: &mut B) {
        bus.write(addr, val as u8);
        bus.write(addr.wrapping_add(1), (val >> 8) as u8);
    }

    /// Read one of the four 16-bit register pairs by its 2-bit encoding (`0=BC 1=DE 2=HL 3=SP`) — the pair
    /// selected by bits 5..4 of the `LD rr,nn`/`INC rr`/`DEC rr`/`ADD HL,rr` opcodes.
    fn rr_get(&self, sel: u8) -> u16 {
        match sel & 3 {
            0 => self.bc,
            1 => self.de,
            2 => self.hl,
            _ => self.sp,
        }
    }

    /// Write one of the four 16-bit register pairs by its 2-bit encoding (`0=BC 1=DE 2=HL 3=SP`).
    fn rr_set(&mut self, sel: u8, val: u16) {
        match sel & 3 {
            0 => self.bc = val,
            1 => self.de = val,
            2 => self.hl = val,
            _ => self.sp = val,
        }
    }

    /// Read the `PUSH`/`POP` register pair by its 2-bit encoding (`0=BC 1=DE 2=HL 3=AF`). Distinct from
    /// [`Self::rr_get`]: the stack forms replace the `SP` slot with `AF` (Z80 UM008 `PUSH qq`/`POP qq`).
    fn push_pair_get(&self, sel: u8) -> u16 {
        match sel & 3 {
            0 => self.bc,
            1 => self.de,
            2 => self.hl,
            _ => self.af,
        }
    }

    /// Write the `PUSH`/`POP` register pair by its 2-bit encoding (`0=BC 1=DE 2=HL 3=AF`).
    fn push_pair_set(&mut self, sel: u8, val: u16) {
        match sel & 3 {
            0 => self.bc = val,
            1 => self.de = val,
            2 => self.hl = val,
            _ => self.af = val,
        }
    }

    /// Evaluate a Z80 branch condition code by its 3-bit encoding (bits 5..3 of the conditional
    /// `JP`/`CALL`/`RET`; only the low two — `NZ`/`Z`/`NC`/`C` — appear in `JR cc`). `0=NZ 1=Z 2=NC 3=C
    /// 4=PO 5=PE 6=P 7=M` over the documented `Z`/`C`/`P/V`/`S` flag bits.
    fn cc(&self, sel: u8) -> bool {
        let f = self.flags();
        match sel & 7 {
            0 => f & FLAG_Z == 0,  // NZ
            1 => f & FLAG_Z != 0,  // Z
            2 => f & FLAG_C == 0,  // NC
            3 => f & FLAG_C != 0,  // C
            4 => f & FLAG_PV == 0, // PO (parity odd)
            5 => f & FLAG_PV != 0, // PE (parity even)
            6 => f & FLAG_S == 0,  // P (positive)
            _ => f & FLAG_S != 0,  // M (minus)
        }
    }

    /// Execute one Z80 instruction over `bus`, returning the T-states it consumed (the value the ×15 mclk
    /// catch-up in [`crate::system::System::run_until`] scales into the frontier).
    ///
    /// Instruction-atomic decode-execute (ZC1/ZC2): one non-yielding call that fetches the opcode, walks the
    /// prefix front end (ZC3b), runs the handler, and returns at the instruction boundary with all state in
    /// the struct. This slice completes the whole documented **un-prefixed base table**: `NOP`, the 8/16-bit
    /// loads, 8/16-bit `INC`/`DEC`, `ADD HL,rr`, the accumulator rotates, `DAA`/`CPL`/`SCF`/`CCF`, the `EX`
    /// forms + `EXX`, `JP (HL)`, `LD SP,HL`, `EI`/`DI`, `IN A,(n)`/`OUT (n),A`, the 8-bit `LD`/ALU blocks
    /// (`0x40-0xBF`), the ALU-immediate `A,n` ops, and the branch/stack control flow (`DJNZ`/`JR`/`JR cc`/
    /// `JP`/`JP cc`/`CALL`/`CALL cc`/`RET`/`RET cc`/`RST`/`PUSH`/`POP`), the full `CB`-prefixed group
    /// (rotates/shifts, `BIT`/`RES`/`SET`), the documented `ED`-prefixed subset (see [`Self::execute_ed`]), the
    /// documented `DD`/`FD` (`IX`/`IY`) base ops (see [`Self::execute_indexed_base`]), and the documented
    /// `DDCB`/`FDCB` bit/shift group (see [`Self::execute_ddcb`]) — the whole documented instruction set. Only
    /// the undocumented opcodes (the `ED` holes/mirrors, the `IXH`/`IXL` half-register forms, and the
    /// `DDCB`/`FDCB` register-copy variants are all implemented and corpus-graded too: the entire Z80
    /// instruction space, documented and undocumented, is served.
    pub fn step<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        // **Latched refusal (see `Z80Fault`): the Z80 is stopped.** It burns T-states so `catch_up_z80`'s
        // absolute-deadline loop still terminates, and it executes nothing — not even an interrupt, since
        // vectoring into a handler would resume a program the core has already said it cannot run. This is
        // checked FIRST, ahead of interrupt acceptance and the HALT idle, so no path can step past it.
        if self.fault.is_some() {
            return 4;
        }
        // Maskable interrupt acceptance (ZC14), sampled at the instruction boundary. Taken only when the /INT
        // line is asserted AND interrupts are enabled (IFF1). Acceptance also wakes a HALT. A masked request
        // (IFF1 = 0) is ignored — HALT then continues idling. The Genesis has no Z80 NMI source (Plutiedev).
        // SPIKE (M24): was the previous instruction EI? UM0080 p.18: "any pending interrupt request is not
        // accepted until after the instruction following EI is executed".
        let after_ei = crate::spike_m24::take_last_ei();
        if self.int_pending && self.iff1 {
            if after_ei && crate::spike_m24::mutation() == crate::spike_m24::Mutation::EiDelay {
                crate::spike_m24::note_mut_fired();
            } else {
                crate::spike_m24::note_accept(after_ei);
                return self.accept_interrupt(bus);
            }
        }
        if self.halted {
            // HALT idle: the CPU runs internal NOPs (refresh continues) until an accepted interrupt clears
            // `halted`. Not exercised by SST (no halted-initial case).
            self.inc_r();
            return 4;
        }
        // The instruction's first byte, kept so a refusal can rewind to it: a fault must name the byte the
        // core could not run, not the byte the decoder happened to be holding several fetches later.
        let instruction_pc = self.pc;
        let opcode = self.next_opcode(bus);
        let t = self.execute(opcode, bus);
        // A `Some` here is always FRESH — a pre-existing fault returned on line one above — so this rewinds
        // exactly the instruction that just refused, and can never re-rewind an older one.
        if let Some(fault) = self.fault.as_mut() {
            fault.pc = instruction_pc;
            self.pc = instruction_pc;
        }
        t
    }

    /// Accept a maskable interrupt (ZC14): clear IFF1/IFF2, un-halt, push PC, and vector per interrupt mode.
    /// On the Genesis the data bus floats to `$FF` during the interrupt-acknowledge M1, so IM 0 sees `RST 38h`
    /// (→ `$0038`, identical to IM 1) and IM 2 forms its vector as `(I << 8) | $FF`. Cost: 13 T-states for
    /// IM 0/1, 19 for IM 2 (Z80 UM008 §"Interrupt Response"). `int_pending` is consumed (the acknowledged
    /// request is cleared); a still-asserted line re-arms only when the driver `EI`s and the next VINT raises it.
    fn accept_interrupt<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        self.halted = false;
        self.iff1 = false;
        self.iff2 = false;
        self.int_pending = false;
        self.inc_r(); // the interrupt-acknowledge cycle is an M1 (refresh) cycle.
        self.sp = self.sp.wrapping_sub(2);
        self.write16(self.sp, self.pc, bus);
        if self.im == 2 {
            // IM 2: fetch the handler address from the vector table at (I << 8) | (floating bus = $FF).
            let vector = ((self.i as u16) << 8) | 0x00FF;
            self.pc = self.read16(vector, bus);
            19
        } else {
            // IM 0 (floating bus $FF = RST 38h) and IM 1 both vector to $0038.
            self.pc = 0x0038;
            13
        }
    }

    /// Set the maskable-interrupt (`/INT`) line level (ZC14). The Genesis VDP asserts this once per frame at
    /// vblank; `System` raises it on the VInt event and clears it at the next frame start. The Z80 samples it
    /// at each instruction boundary in [`Z80::step`].
    pub fn set_int_line(&mut self, asserted: bool) {
        self.int_pending = asserted;
    }

    /// SPIKE (M24): the `/INT` level as the core holds it.
    pub(crate) fn spike_int_line(&self) -> bool {
        self.int_pending
    }

    /// The prefix-accumulating front end (ZC3b): `CB`/`ED` select an alternate table, `DD`/`FD` set an
    /// index-register override for the following opcode, and `DDCB`/`FDCB` fetch the displacement **before**
    /// the final opcode byte. `CB`, the documented `ED` subset (holes = NONI), and the `DD`/`FD` documented
    /// forms + ignored-prefix rule are implemented; only the `IXH`/`IXL` half-register ops, the ED mirrors,
    /// the `ED` mirrors, the `IXH`/`IXL` half-register forms and the `DDCB`/`FDCB` register-copy variants
    /// are all implemented and corpus-graded.
    fn execute<B: Z80Io>(&mut self, opcode: u8, bus: &mut B) -> u32 {
        match opcode {
            0xCB => self.execute_cb(bus),
            0xED => self.execute_ed(bus),
            0xDD => self.execute_indexed(IndexReg::Ix, bus),
            0xFD => self.execute_indexed(IndexReg::Iy, bus),
            _ => self.execute_base(opcode, bus),
        }
    }

    /// Base-table (unprefixed) opcodes — now the **entire** un-prefixed base table. Alongside the earlier
    /// data/arithmetic/rotate/misc coverage (8/16-bit loads, 8/16-bit `INC`/`DEC`, `ADD HL,rr`, the
    /// accumulator rotates, `DAA`/`CPL`/`SCF`/`CCF`, the `EX` forms, `EXX`, `JP (HL)`, `LD SP,HL`, `EI`/`DI`,
    /// `IN A,(n)`/`OUT (n),A`, the 8-bit `LD`/ALU blocks, and the ALU-immediate `A,n` ops) this includes the
    /// branch/stack control flow: `DJNZ e`/`JR e`/`JR cc,e`, `JP nn`/`JP cc,nn`, `CALL nn`/`CALL cc,nn`,
    /// `RET`/`RET cc`, `RST p`, and `PUSH qq`/`POP qq`. Only the prefix escapes (`CB`/`ED`/`DD`/`FD`, handled
    /// in [`Self::execute`]) reach elsewhere, and **all four are served** — [`Self::execute`] dispatches
    /// `CB`, `ED`, and `DD`/`FD` (plus the `DDCB`/`FDCB` group). This said "`CB` is done and `ED`/`DD`/`FD`
    /// are the next slice" after those slices landed; this module's own header already said "Nothing is
    /// deferred" twenty-six lines up. Lens sweep.
    fn execute_base<B: Z80Io>(&mut self, opcode: u8, bus: &mut B) -> u32 {
        match opcode {
            0x00 => 4, // NOP

            // ---- DJNZ e (0x10): B -= 1 (no flags), branch by the signed displacement if B != 0. The
            // displacement is read (advancing PC past it) before the decrement, so it is relative to the
            // instruction following DJNZ. ----
            0x10 => {
                let e = self.next_byte(bus) as i8;
                let b = ((self.bc >> 8) as u8).wrapping_sub(1);
                self.bc = (self.bc & 0x00FF) | ((b as u16) << 8);
                if b != 0 {
                    self.pc = self.pc.wrapping_add(e as u16);
                    13
                } else {
                    8
                }
            }

            // ---- JR e (0x18): unconditional relative jump by the signed 8-bit displacement. ----
            0x18 => {
                let e = self.next_byte(bus) as i8;
                self.pc = self.pc.wrapping_add(e as u16);
                12
            }

            // ---- JR cc,e (0x20 NZ / 0x28 Z / 0x30 NC / 0x38 C): cc = bits 4..3 (only Z/C tested here).
            // The displacement is always consumed; the branch is taken only if the condition holds. ----
            0x20 | 0x28 | 0x30 | 0x38 => {
                let e = self.next_byte(bus) as i8;
                if self.cc((opcode >> 3) & 3) {
                    self.pc = self.pc.wrapping_add(e as u16);
                    12
                } else {
                    7
                }
            }

            // ---- 16-bit immediate load: LD rr,nn (rr = bits 5..4). ----
            0x01 | 0x11 | 0x21 | 0x31 => {
                let nn = self.next_word(bus);
                self.rr_set((opcode >> 4) & 3, nn);
                10
            }

            // ---- LD (BC/DE),A and LD A,(BC/DE). ----
            0x02 => {
                bus.write(self.bc, self.a());
                7
            }
            0x0A => {
                let v = bus.read(self.bc);
                self.set_a(v);
                7
            }
            0x12 => {
                bus.write(self.de, self.a());
                7
            }
            0x1A => {
                let v = bus.read(self.de);
                self.set_a(v);
                7
            }

            // ---- 16-bit INC/DEC rr (no flags). ----
            0x03 | 0x13 | 0x23 | 0x33 => {
                let sel = (opcode >> 4) & 3;
                self.rr_set(sel, self.rr_get(sel).wrapping_add(1));
                6
            }
            0x0B | 0x1B | 0x2B | 0x3B => {
                let sel = (opcode >> 4) & 3;
                self.rr_set(sel, self.rr_get(sel).wrapping_sub(1));
                6
            }

            // ---- 8-bit INC/DEC r|(HL) (dst = bits 5..3). ----
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                let dst = (opcode >> 3) & 7;
                let v = self.reg8_get(dst, bus);
                let (r, f) = inc8(v, self.flags());
                self.reg8_set(dst, r, bus);
                self.set_flags(f);
                if dst == 6 {
                    11
                } else {
                    4
                }
            }
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                let dst = (opcode >> 3) & 7;
                let v = self.reg8_get(dst, bus);
                let (r, f) = dec8(v, self.flags());
                self.reg8_set(dst, r, bus);
                self.set_flags(f);
                if dst == 6 {
                    11
                } else {
                    4
                }
            }

            // ---- 8-bit immediate load: LD r,n (dst = bits 5..3). ----
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
                let dst = (opcode >> 3) & 7;
                let n = self.next_byte(bus);
                self.reg8_set(dst, n, bus);
                if dst == 6 {
                    10
                } else {
                    7
                }
            }

            // ---- Accumulator rotates (H = N = 0; C from the rotated-out bit; S/Z/P/V preserved). ----
            0x07 => self.op_rlca(),
            0x0F => self.op_rrca(),
            0x17 => self.op_rla(),
            0x1F => self.op_rra(),

            // ---- EX AF,AF'. ----
            0x08 => {
                std::mem::swap(&mut self.af, &mut self.af2);
                4
            }

            // ---- ADD HL,rr. ----
            0x09 | 0x19 | 0x29 | 0x39 => {
                self.op_add_hl(self.rr_get((opcode >> 4) & 3));
                11
            }

            // ---- LD (nn),HL / LD HL,(nn). ----
            0x22 => {
                let addr = self.next_word(bus);
                self.write16(addr, self.hl, bus);
                16
            }
            0x2A => {
                let addr = self.next_word(bus);
                self.hl = self.read16(addr, bus);
                16
            }

            // ---- DAA / CPL / SCF / CCF. ----
            0x27 => self.op_daa(),
            0x2F => self.op_cpl(),
            0x37 => self.op_scf(),
            0x3F => self.op_ccf(),

            // ---- LD (nn),A / LD A,(nn). ----
            0x32 => {
                let addr = self.next_word(bus);
                bus.write(addr, self.a());
                13
            }
            0x3A => {
                let addr = self.next_word(bus);
                let v = bus.read(addr);
                self.set_a(v);
                13
            }

            // ---- 8-bit LD block and ALU A,r block (prior slice). ----
            0x40..=0x7F => self.op_ld_block(opcode, bus),
            0x80..=0xBF => self.op_alu_block(opcode, bus),

            // ---- ALU-immediate A,n (op = bits 5..3, reusing the register-form flag logic). ----
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                let n = self.next_byte(bus);
                self.alu8((opcode >> 3) & 7, n);
                7
            }

            // ---- POP qq (qq = bits 5..4: BC/DE/HL/AF): load the pair from the top of stack, SP += 2. Low
            // byte at the lower address. POP AF loads F wholesale (all bits, incl. the undocumented pair). ----
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                let val = self.read16(self.sp, bus);
                self.sp = self.sp.wrapping_add(2);
                self.push_pair_set((opcode >> 4) & 3, val);
                10
            }

            // ---- PUSH qq (qq = bits 5..4: BC/DE/HL/AF): SP -= 2, store the pair (low byte at the lower
            // address). PUSH AF stores F wholesale. ----
            0xC5 | 0xD5 | 0xE5 | 0xF5 => {
                let val = self.push_pair_get((opcode >> 4) & 3);
                self.sp = self.sp.wrapping_sub(2);
                self.write16(self.sp, val, bus);
                11
            }

            // ---- RET (0xC9): pop PC from the stack, SP += 2. ----
            0xC9 => {
                self.pc = self.read16(self.sp, bus);
                self.sp = self.sp.wrapping_add(2);
                10
            }

            // ---- RET cc (cc = bits 5..3): conditional return. ----
            0xC0 | 0xC8 | 0xD0 | 0xD8 | 0xE0 | 0xE8 | 0xF0 | 0xF8 => {
                if self.cc((opcode >> 3) & 7) {
                    self.pc = self.read16(self.sp, bus);
                    self.sp = self.sp.wrapping_add(2);
                    11
                } else {
                    5
                }
            }

            // ---- JP nn (0xC3): unconditional absolute jump. ----
            0xC3 => {
                let nn = self.next_word(bus);
                self.pc = nn;
                10
            }

            // ---- JP cc,nn (cc = bits 5..3): the immediate is always consumed; PC is set only if cc holds. ----
            0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA => {
                let nn = self.next_word(bus);
                if self.cc((opcode >> 3) & 7) {
                    self.pc = nn;
                }
                10
            }

            // ---- CALL nn (0xCD): push the return address (PC of the next instruction), then jump. ----
            0xCD => {
                let nn = self.next_word(bus);
                self.sp = self.sp.wrapping_sub(2);
                self.write16(self.sp, self.pc, bus);
                self.pc = nn;
                17
            }

            // ---- CALL cc,nn (cc = bits 5..3): the immediate is always consumed; push+jump only if cc holds. ----
            0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC => {
                let nn = self.next_word(bus);
                if self.cc((opcode >> 3) & 7) {
                    self.sp = self.sp.wrapping_sub(2);
                    self.write16(self.sp, self.pc, bus);
                    self.pc = nn;
                    17
                } else {
                    10
                }
            }

            // ---- RST p (p = bits 5..3 × 8 = opcode & 0x38): push PC, jump to the fixed page-0 vector. ----
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                self.sp = self.sp.wrapping_sub(2);
                self.write16(self.sp, self.pc, bus);
                self.pc = (opcode & 0x38) as u16;
                11
            }

            // ---- EXX (0xD9): swap BC/DE/HL with their shadow file (AF is unaffected; that is EX AF,AF'). ----
            0xD9 => {
                std::mem::swap(&mut self.bc, &mut self.bc2);
                std::mem::swap(&mut self.de, &mut self.de2);
                std::mem::swap(&mut self.hl, &mut self.hl2);
                4
            }

            // ---- OUT (n),A / IN A,(n): port = (A << 8) | n. Neither affects the flags. ----
            0xD3 => {
                let n = self.next_byte(bus);
                let port = ((self.a() as u16) << 8) | n as u16;
                bus.output(port, self.a());
                11
            }
            0xDB => {
                let n = self.next_byte(bus);
                let port = ((self.a() as u16) << 8) | n as u16;
                let v = bus.input(port);
                self.set_a(v);
                11
            }

            // ---- EX (SP),HL. ----
            0xE3 => {
                let tmp = self.read16(self.sp, bus);
                self.write16(self.sp, self.hl, bus);
                self.hl = tmp;
                19
            }

            // ---- JP (HL): an indirect load of HL into PC (not a conditional branch). ----
            0xE9 => {
                self.pc = self.hl;
                4
            }

            // ---- EX DE,HL. ----
            0xEB => {
                std::mem::swap(&mut self.de, &mut self.hl);
                4
            }

            // ---- DI / EI: set both interrupt-enable flip-flops (EI's one-instruction delay is unobserved
            // by the SST-z80 gate, which checks only the final IFF1/IFF2). ----
            0xF3 => {
                self.iff1 = false;
                self.iff2 = false;
                4
            }
            0xFB => {
                self.iff1 = true;
                self.iff2 = true;
                crate::spike_m24::note_ei(); // SPIKE (M24)
                4
            }

            // ---- LD SP,HL. ----
            0xF9 => {
                self.sp = self.hl;
                6
            }

            // Unreachable: the whole un-prefixed base table is now covered, and the four prefix escapes
            // (0xCB/0xED/0xDD/0xFD) are dispatched by `execute` before reaching here.
            0xCB | 0xED | 0xDD | 0xFD => unreachable!(
                "Z80 prefix byte {opcode:#04X} is dispatched by `execute`, never `execute_base`"
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

    /// `RLCA` (`0x07`): rotate `A` left circular; `C` = old bit 7. `H = N = 0`, `S/Z/P/V` preserved.
    fn op_rlca(&mut self) -> u32 {
        let a = self.a();
        let c = a >> 7;
        let r = (a << 1) | c;
        self.set_a(r);
        self.set_flags(rotate_a_flags(self.flags(), r, c != 0));
        4
    }

    /// `RRCA` (`0x0F`): rotate `A` right circular; `C` = old bit 0.
    fn op_rrca(&mut self) -> u32 {
        let a = self.a();
        let c = a & 1;
        let r = (a >> 1) | (c << 7);
        self.set_a(r);
        self.set_flags(rotate_a_flags(self.flags(), r, c != 0));
        4
    }

    /// `RLA` (`0x17`): rotate `A` left through carry; new bit 0 = old `C`, `C` = old bit 7.
    fn op_rla(&mut self) -> u32 {
        let a = self.a();
        let old_c = self.flags() & FLAG_C;
        let r = (a << 1) | old_c;
        self.set_a(r);
        self.set_flags(rotate_a_flags(self.flags(), r, a & 0x80 != 0));
        4
    }

    /// `RRA` (`0x1F`): rotate `A` right through carry; new bit 7 = old `C`, `C` = old bit 0.
    fn op_rra(&mut self) -> u32 {
        let a = self.a();
        let old_c = self.flags() & FLAG_C;
        let r = (a >> 1) | (old_c << 7);
        self.set_a(r);
        self.set_flags(rotate_a_flags(self.flags(), r, a & 1 != 0));
        4
    }

    /// `ADD HL,rr` (`0x09`/`0x19`/`0x29`/`0x39`): `HL += rr`. `N = 0`, `H` = carry out of bit 11, `C` =
    /// carry out of bit 15; `S/Z/P/V` preserved. `YF/XF` come from the result's high byte (undocumented,
    /// masked out of the documented-flag gate).
    fn op_add_hl(&mut self, rr: u16) {
        self.hl = self.add16(self.hl, rr);
    }

    /// `DAA` (`0x27`): decimal-adjust `A` after a binary add/subtract, using `N`/`H`/`C`. Sets `S Z P/V C H`;
    /// `N` preserved. The correction adds/subtracts `0x06` to the low digit and `0x60` to the high digit,
    /// with the direction chosen by `N` (UM008 §"DAA").
    fn op_daa(&mut self) -> u32 {
        let a = self.a();
        let n = self.flags() & FLAG_N != 0;
        let h = self.flags() & FLAG_H != 0;
        let c = self.flags() & FLAG_C != 0;
        let mut correction = 0u8;
        let mut new_c = false;
        if h || (a & 0x0F) > 9 {
            correction |= 0x06;
        }
        if c || a > 0x99 {
            correction |= 0x60;
            new_c = true;
        }
        let result = if n {
            a.wrapping_sub(correction)
        } else {
            a.wrapping_add(correction)
        };
        let new_h = if n {
            h && (a & 0x0F) < 6
        } else {
            (a & 0x0F) > 9
        };
        self.set_a(result);
        let mut f = if n { FLAG_N } else { 0 };
        if result & 0x80 != 0 {
            f |= FLAG_S;
        }
        if result == 0 {
            f |= FLAG_Z;
        }
        if new_h {
            f |= FLAG_H;
        }
        if result.count_ones().is_multiple_of(2) {
            f |= FLAG_PV;
        }
        if new_c {
            f |= FLAG_C;
        }
        f |= result & FLAG_XY;
        self.set_flags(f);
        4
    }

    /// `CPL` (`0x2F`): `A = !A`. Sets `H = N = 1`; `S/Z/P/V/C` preserved.
    fn op_cpl(&mut self) -> u32 {
        let r = !self.a();
        self.set_a(r);
        let mut f = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV | FLAG_C);
        f |= FLAG_H | FLAG_N;
        f |= r & FLAG_XY;
        self.set_flags(f);
        4
    }

    /// `SCF` (`0x37`): set carry. `C = 1`, `H = N = 0`; `S/Z/P/V` preserved.
    fn op_scf(&mut self) -> u32 {
        let mut f = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV);
        f |= FLAG_C;
        f |= self.a() & FLAG_XY; // undocumented, masked out of the documented-flag gate
        self.set_flags(f);
        4
    }

    /// `CCF` (`0x3F`): complement carry. `C = !C`, `H` = old `C`, `N = 0`; `S/Z/P/V` preserved.
    fn op_ccf(&mut self) -> u32 {
        let old_c = self.flags() & FLAG_C != 0;
        let mut f = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV);
        if !old_c {
            f |= FLAG_C;
        }
        if old_c {
            f |= FLAG_H;
        }
        f |= self.a() & FLAG_XY; // undocumented, masked out of the documented-flag gate
        self.set_flags(f);
        4
    }

    // ---- Prefix leaf handlers — structurally reached, opcode bodies land in the Z-execute slice. ----

    /// `CB`-prefixed rotate/shift/bit ops (the full `0xCB 0x00`-`0xFF` group). The sub-opcode is fetched over
    /// a second M1 cycle (refresh bumps twice for a CB-prefixed instruction), then decoded by its high two
    /// bits: `0x00-0x3F` = the eight rotate/shift ops `RLC RRC RL RR SLA SRA SLL SRL` (op = bits 5..3),
    /// `0x40-0x7F` = `BIT b`, `0x80-0xBF` = `RES b`, `0xC0-0xFF` = `SET b` (b = bits 5..3); the low three bits
    /// select the target (`0=B..7=A`, `6 = (HL)`, a read-modify-write of memory). Unlike the accumulator
    /// rotates, the CB rotates/shifts set the **full** documented flag set (`S Z` from the result, `H = N = 0`,
    /// `P/V` = parity, `C` = the shifted-out bit). `SLL` (op 6) is an undocumented opcode (shift left, bit 0
    /// forced to 1) implemented for table completeness; its documented flags follow the same rule. `RES`/`SET`
    /// touch no flags. The undocumented `YF/XF` (and `BIT`'s memory-sourced pair) stay masked out of the
    /// documented-flag gate.
    fn execute_cb<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        let sub = self.next_opcode(bus);
        let target = sub & 7;
        let is_hl = target == 6;
        match sub {
            // ---- Rotates/shifts (0x00-0x3F): op = bits 5..3, full documented flag set. ----
            0x00..=0x3F => {
                let op = (sub >> 3) & 7;
                let v = self.reg8_get(target, bus);
                let (r, carry) = self.rotate_shift(op, v);
                self.reg8_set(target, r, bus);
                self.set_flags(shift_rotate_flags(r, carry));
                if is_hl {
                    15
                } else {
                    8
                }
            }
            // ---- BIT b,r|(HL) (0x40-0x7F): Z = NOT(bit), H = 1, N = 0, S = (b==7 && set), P/V = Z, C
            // preserved. No target write. ----
            0x40..=0x7F => {
                let b = (sub >> 3) & 7;
                let v = self.reg8_get(target, bus);
                self.op_bit(b, v);
                if is_hl {
                    12
                } else {
                    8
                }
            }
            // ---- RES b,r|(HL) (0x80-0xBF): clear bit b; no flags. ----
            0x80..=0xBF => {
                let b = (sub >> 3) & 7;
                let v = self.reg8_get(target, bus);
                self.reg8_set(target, v & !(1 << b), bus);
                if is_hl {
                    15
                } else {
                    8
                }
            }
            // ---- SET b,r|(HL) (0xC0-0xFF): set bit b; no flags. ----
            _ => {
                let b = (sub >> 3) & 7;
                let v = self.reg8_get(target, bus);
                self.reg8_set(target, v | (1 << b), bus);
                if is_hl {
                    15
                } else {
                    8
                }
            }
        }
    }

    /// Apply a `CB`-space rotate/shift `op` (`0=RLC 1=RRC 2=RL 3=RR 4=SLA 5=SRA 6=SLL 7=SRL`) to `v`, returning
    /// `(result, carry_out)`. `RL`/`RR` rotate through the current `C` flag; the others are circular
    /// (`RLC`/`RRC`) or straight shifts. `SLA`/`SLL` shift left (bit 0 = 0 / 1); `SRA` is arithmetic (bit 7
    /// preserved), `SRL` logical (bit 7 = 0). The caller derives the documented flags from `(result, carry)`.
    fn rotate_shift(&self, op: u8, v: u8) -> (u8, bool) {
        let old_c = self.flags() & FLAG_C != 0;
        match op {
            0 => (v.rotate_left(1), v & 0x80 != 0),                // RLC
            1 => (v.rotate_right(1), v & 0x01 != 0),               // RRC
            2 => ((v << 1) | old_c as u8, v & 0x80 != 0),          // RL
            3 => ((v >> 1) | ((old_c as u8) << 7), v & 0x01 != 0), // RR
            4 => (v << 1, v & 0x80 != 0),                          // SLA
            5 => ((v >> 1) | (v & 0x80), v & 0x01 != 0),           // SRA
            6 => ((v << 1) | 1, v & 0x80 != 0),                    // SLL (undocumented)
            _ => (v >> 1, v & 0x01 != 0),                          // SRL
        }
    }

    /// `BIT b,r|(HL)`: test bit `b` of `v`. `Z` = NOT(bit set), `H = 1`, `N = 0`, `S` set only for `b == 7`
    /// with the bit set, `P/V` = `Z` (documented convention), `C` **preserved**. The undocumented `YF/XF`
    /// (from the operand for the register form, from `wz` for `(HL)`) are set from `v` here but masked out of
    /// the documented-flag gate.
    fn op_bit(&mut self, b: u8, v: u8) {
        let set = v & (1 << b) != 0;
        let mut f = (self.flags() & FLAG_C) | FLAG_H; // C preserved, H = 1, N = 0
        if !set {
            f |= FLAG_Z | FLAG_PV;
        }
        if b == 7 && set {
            f |= FLAG_S;
        }
        f |= v & FLAG_XY; // undocumented, masked out of the documented-flag gate
        self.set_flags(f);
    }

    /// `ED`-prefixed extended ops — the **documented** subset of the `0xED` table (Z-execute sub-slice 5).
    /// The sub-opcode is fetched over a second M1 cycle (refresh bumps twice for an ED-prefixed instruction),
    /// then decoded. Covered: `IN r,(C)`/`OUT (C),r`, `SBC HL,rr`/`ADC HL,rr`, `LD (nn),rr`/`LD rr,(nn)`,
    /// `NEG`, `RETN`/`RETI`, `IM 0/1/2`, `LD I,A`/`LD R,A`/`LD A,I`/`LD A,R`, `RRD`/`RLD`, the block
    /// transfer/search (`LDI`/`LDD`/`LDIR`/`LDDR`, `CPI`/`CPD`/`CPIR`/`CPDR`), and the block I/O
    /// (`INI`/`IND`/`INIR`/`INDR`, `OUTI`/`OUTD`/`OTIR`/`OTDR`). The undocumented ED HOLES execute as NONI
    /// (8-T no-ops — see the arm below); the undocumented mirrors of `NEG`/`RETN`/`IM`/`IN (C)`/
    /// `OUT (C),0` (`0x70`/`0x71`) are IMPLEMENTED here too, graded 20/20 × 1000 against the corpus.
    ///
    /// Repeating variants (`LDIR`/`LDDR`/`CPIR`/`CPDR`/`INIR`/`INDR`/`OTIR`/`OTDR`) are modeled per the
    /// SST instruction-atomic contract: one `step()` performs one iteration, and when the loop continues,
    /// `PC` is rewound to the `ED`-instruction start (`PC - 2`) so the next `step()` re-enters the same
    /// opcode; on the terminating iteration `PC` advances past the instruction. `R` keeps both M1 bumps
    /// regardless (the fixture records the post-fetch refresh). The undocumented `YF`/`XF` (and the
    /// block-op quirks that source them from internal values) stay masked out of the documented-flag gate.
    fn execute_ed<B: Z80Io>(&mut self, bus: &mut B) -> u32 {
        let sub = self.next_opcode(bus);
        match sub {
            // ---- IN r,(C) (0x40/48/50/58/60/68/78; reg = bits 5..3, encoding 6 = the undocumented
            // flags-only form 0x70, deferred): port = BC (B on the high address lines). Sets S/Z/P-V from
            // the value, H = N = 0, C preserved (unlike `IN A,(n)`, which is flagless). ----
            0x40 | 0x48 | 0x50 | 0x58 | 0x60 | 0x68 | 0x78 => {
                let val = bus.input(self.bc);
                self.reg8_set((sub >> 3) & 7, val, bus);
                self.set_flags(self.in_c_flags(val));
                12
            }

            // ---- IN (C) / `IN F,(C)` (0x70): the register slot's encoding `6` — the one that would name
            // `(HL)` — is instead the FLAGS-ONLY form. The port is read exactly as `IN r,(C)` and sets the
            // identical flags, but the value is discarded rather than stored anywhere. Undocumented, real,
            // and NOT a no-op: the read is externally visible and every flag but `C` moves. ----
            0x70 => {
                let val = bus.input(self.bc);
                self.set_flags(self.in_c_flags(val));
                12
            }

            // ---- OUT (C),r (0x41/49/51/59/61/69/79; reg = bits 5..3, encoding 6 = the undocumented
            // `OUT (C),0` form 0x71): port = BC, no flags. ----
            0x41 | 0x49 | 0x51 | 0x59 | 0x61 | 0x69 | 0x79 => {
                let val = self.reg8_get((sub >> 3) & 7, bus);
                bus.output(self.bc, val);
                12
            }

            // ---- OUT (C),0 (0x71): the `6` slot again — this one writes a constant. NMOS Z80 (the
            // Genesis part) drives **0**; the CMOS Z84C00 drives $FF. The corpus is NMOS, and so is the
            // hardware this emulator models. ----
            0x71 => {
                bus.output(self.bc, 0);
                12
            }

            // ---- SBC HL,rr / ADC HL,rr (rr = bits 5..4: BC/DE/HL/SP). Full 16-bit flags. ----
            0x42 | 0x52 | 0x62 | 0x72 => {
                self.op_sbc_hl(self.rr_get((sub >> 4) & 3));
                15
            }
            0x4A | 0x5A | 0x6A | 0x7A => {
                self.op_adc_hl(self.rr_get((sub >> 4) & 3));
                15
            }

            // ---- LD (nn),rr / LD rr,(nn) (rr = bits 5..4). 16-bit, little-endian; no flags. ----
            0x43 | 0x53 | 0x63 | 0x73 => {
                let addr = self.next_word(bus);
                self.write16(addr, self.rr_get((sub >> 4) & 3), bus);
                20
            }
            0x4B | 0x5B | 0x6B | 0x7B => {
                let addr = self.next_word(bus);
                let v = self.read16(addr, bus);
                self.rr_set((sub >> 4) & 3, v);
                20
            }

            // ---- NEG (0x44) and its seven undocumented MIRRORS (0x4C/54/5C/64/6C/74/7C): A = 0 - A, flags
            // as for `SUB 0,A`. The ED decoder ignores bits 5..3 for this opcode, so all eight encodings
            // are the same instruction — which is why treating the mirrors as no-ops would have been the
            // silent wrong answer: `NEG` rewrites A and every flag. ----
            0x44 | 0x4C | 0x54 | 0x5C | 0x64 | 0x6C | 0x74 | 0x7C => {
                let (r, f) = sub8(0, self.a(), 0);
                self.set_a(r);
                self.set_flags(f);
                8
            }

            // ---- RETN (0x45) / RETI (0x4D) and the six undocumented RETN MIRRORS (0x55/5D/65/6D/75/7D):
            // pop PC; all copy IFF2 -> IFF1 (the shared return-from-NMI/INT microcode does this on
            // hardware, which the SST corpus encodes). A mirror treated as a no-op would leave the return
            // address on the stack and fall through into whatever followed the call. ----
            0x45 | 0x4D | 0x55 | 0x5D | 0x65 | 0x6D | 0x75 | 0x7D => {
                self.pc = self.read16(self.sp, bus);
                self.sp = self.sp.wrapping_add(2);
                self.iff1 = self.iff2;
                14
            }

            // ---- IM 0/1/2 (0x46/56/5E) and their undocumented mirrors; no flags. The mode is selected by
            // bits 4..3 of the opcode, which is why the mirrors group as they do: 0x4E/66/6E join IM 0 (the
            // "IM 0/1" encodings, which behave as IM 0), 0x76 joins IM 1, 0x7E joins IM 2. Silently
            // no-oping one would leave the machine vectoring interrupts the wrong way. ----
            0x46 | 0x4E | 0x66 | 0x6E => {
                self.im = 0;
                8
            }
            0x56 | 0x76 => {
                self.im = 1;
                8
            }
            0x5E | 0x7E => {
                self.im = 2;
                8
            }

            // ---- LD I,A (0x47) / LD R,A (0x4F): no flags. R stores all 8 bits (bit 7 included). ----
            0x47 => {
                self.i = self.a();
                9
            }
            0x4F => {
                self.r = self.a();
                9
            }

            // ---- LD A,I (0x57) / LD A,R (0x5F): S/Z from the value, H = N = 0, P/V = IFF2, C preserved.
            // For `LD A,R` the value is R after both M1 refresh bumps (already applied by `next_opcode`). ----
            0x57 => {
                let v = self.i;
                self.set_a(v);
                self.set_flags(self.ld_a_ir_flags(v));
                9
            }
            0x5F => {
                let v = self.r;
                self.set_a(v);
                self.set_flags(self.ld_a_ir_flags(v));
                9
            }

            // ---- RRD (0x67) / RLD (0x6F): 4-bit nibble rotate through (HL); S/Z/P-V from A, H = N = 0,
            // C preserved. ----
            0x67 => {
                self.op_rrd(bus);
                18
            }
            0x6F => {
                self.op_rld(bus);
                18
            }

            // ---- Block transfer: LDI/LDD/LDIR/LDDR. ----
            0xA0 => self.block_ld(bus, true, false),
            0xA8 => self.block_ld(bus, false, false),
            0xB0 => self.block_ld(bus, true, true),
            0xB8 => self.block_ld(bus, false, true),

            // ---- Block search: CPI/CPD/CPIR/CPDR. ----
            0xA1 => self.block_cp(bus, true, false),
            0xA9 => self.block_cp(bus, false, false),
            0xB1 => self.block_cp(bus, true, true),
            0xB9 => self.block_cp(bus, false, true),

            // ---- Block input: INI/IND/INIR/INDR. ----
            0xA2 => self.block_in(bus, true, false),
            0xAA => self.block_in(bus, false, false),
            0xB2 => self.block_in(bus, true, true),
            0xBA => self.block_in(bus, false, true),

            // ---- Block output: OUTI/OUTD/OTIR/OTDR. ----
            0xA3 => self.block_out(bus, true, false),
            0xAB => self.block_out(bus, false, false),
            0xB3 => self.block_out(bus, true, true),
            0xBB => self.block_out(bus, false, true),

            // ---- Undocumented ED HOLES (`$00-$3F`, `$77`/`$7F`, `$80-$9F`, the four gaps between the
            // block groups, `$BC-$FF`): NONI — no operation, equivalent to two NOPs, 8 T-states, no
            // register/flag effect beyond the two M1 refreshes `next_opcode` already applied. Pinned from
            // "The Undocumented Z80 Documented" (Sean Young, §5.4: ED holes "have no effect, and take up
            // 8 T states"). Promoted from a deferred panic because a scored conformance ROM
            // (`m68k_opcode_sizes`) reaches `ED 00` for real: it uploads its Z80 program with 68k WORD
            // writes, and under the hardware-true one-byte-lands window rule (Q4) the Z80 executes the
            // half-landed stream — which real hardware runs straight through as NOPs. ----
            0x00..=0x3F
            | 0x77
            | 0x7F
            | 0x80..=0x9F
            | 0xA4..=0xA7
            | 0xAC..=0xAF
            | 0xB4..=0xB7
            | 0xBC..=0xFF => 8,
            // ⚑ There is deliberately **no catch-all arm left here**: with the 20 mirrors landed above, the
            // match covers all 256 `ED` opcodes by construction, so the compiler — not a runtime guard — is
            // what proves no `ED xx` can fall through. A future arm that narrows one of these ranges will
            // fail to compile rather than reintroduce a hole.
        }
    }

    /// The flag set `IN r,(C)` and the flags-only `IN (C)` (`ED 70`) both produce from the byte read:
    /// `S`/`Z` from the value, `P/V` = parity, `H = N = 0`, `C` **preserved** (unlike `IN A,(n)`, which is
    /// flagless). `YF`/`XF` come from the value's bits 5/3 (undocumented, masked out of the documented
    /// gate). One derivation, two callers — the flags-only form differs from `IN r,(C)` only in having
    /// nowhere to put the byte.
    fn in_c_flags(&self, val: u8) -> u8 {
        let mut f = self.flags() & FLAG_C; // C preserved
        if val & 0x80 != 0 {
            f |= FLAG_S;
        }
        if val == 0 {
            f |= FLAG_Z;
        }
        if val.count_ones().is_multiple_of(2) {
            f |= FLAG_PV;
        }
        f | (val & FLAG_XY)
    }

    /// `ADC HL,rr` (0x4A/5A/6A/7A): `HL = HL + rr + C`. `S/Z` from the 16-bit result, `H` = carry out of
    /// bit 11, `P/V` = signed overflow, `N = 0`, `C` = carry out of bit 15. `YF/XF` from the result's high
    /// byte (undocumented, masked).
    fn op_adc_hl(&mut self, rr: u16) {
        let hl = self.hl;
        let c = (self.flags() & FLAG_C) as u32;
        let sum = hl as u32 + rr as u32 + c;
        let result = sum as u16;
        let mut f = 0u8;
        if result & 0x8000 != 0 {
            f |= FLAG_S;
        }
        if result == 0 {
            f |= FLAG_Z;
        }
        if (hl & 0x0FFF) as u32 + (rr & 0x0FFF) as u32 + c > 0x0FFF {
            f |= FLAG_H;
        }
        if (hl ^ result) & (rr ^ result) & 0x8000 != 0 {
            f |= FLAG_PV;
        }
        if sum & 0x1_0000 != 0 {
            f |= FLAG_C;
        }
        f |= (result >> 8) as u8 & FLAG_XY;
        self.hl = result;
        self.set_flags(f);
    }

    /// `SBC HL,rr` (0x42/52/62/72): `HL = HL - rr - C`. `S/Z` from the result, `H` = borrow out of bit 12,
    /// `P/V` = signed overflow, `N = 1`, `C` = borrow out of bit 15. `YF/XF` from the result's high byte.
    fn op_sbc_hl(&mut self, rr: u16) {
        let hl = self.hl;
        let c = (self.flags() & FLAG_C) as i32;
        let diff = hl as i32 - rr as i32 - c;
        let result = diff as u16;
        let mut f = FLAG_N;
        if result & 0x8000 != 0 {
            f |= FLAG_S;
        }
        if result == 0 {
            f |= FLAG_Z;
        }
        if (hl & 0x0FFF) as i32 - (rr & 0x0FFF) as i32 - c < 0 {
            f |= FLAG_H;
        }
        if (hl ^ rr) & (hl ^ result) & 0x8000 != 0 {
            f |= FLAG_PV;
        }
        if diff < 0 {
            f |= FLAG_C;
        }
        f |= (result >> 8) as u8 & FLAG_XY;
        self.hl = result;
        self.set_flags(f);
    }

    /// Documented flags for `LD A,I` / `LD A,R`: `S/Z` from the loaded value, `H = N = 0`, `P/V = IFF2`,
    /// `C` preserved. `YF/XF` from the value (undocumented, masked).
    fn ld_a_ir_flags(&self, v: u8) -> u8 {
        let mut f = self.flags() & FLAG_C; // C preserved
        if v & 0x80 != 0 {
            f |= FLAG_S;
        }
        if v == 0 {
            f |= FLAG_Z;
        }
        if self.iff2 {
            f |= FLAG_PV;
        }
        f |= v & FLAG_XY;
        f
    }

    /// `RRD` (0x67): rotate the low nibble of `A`, the low nibble of `(HL)`, and the high nibble of `(HL)`
    /// one 4-bit digit to the right — `(HL)_lo -> A_lo`, `A_lo -> (HL)_hi`, `(HL)_hi -> (HL)_lo`.
    fn op_rrd<B: Z80Io>(&mut self, bus: &mut B) {
        let a = self.a();
        let m = bus.read(self.hl);
        let new_m = ((a << 4) & 0xF0) | (m >> 4);
        let new_a = (a & 0xF0) | (m & 0x0F);
        bus.write(self.hl, new_m);
        self.set_a(new_a);
        self.set_flags(self.rotate_digit_flags(new_a));
    }

    /// `RLD` (0x6F): the left-digit counterpart of [`Self::op_rrd`] — `(HL)_lo -> (HL)_hi`,
    /// `(HL)_hi -> A_lo`, `A_lo -> (HL)_lo`.
    fn op_rld<B: Z80Io>(&mut self, bus: &mut B) {
        let a = self.a();
        let m = bus.read(self.hl);
        let new_m = ((m << 4) & 0xF0) | (a & 0x0F);
        let new_a = (a & 0xF0) | (m >> 4);
        bus.write(self.hl, new_m);
        self.set_a(new_a);
        self.set_flags(self.rotate_digit_flags(new_a));
    }

    /// Documented flags for `RRD`/`RLD`: `S/Z` from `A`, `P/V` = even parity of `A`, `H = N = 0`,
    /// `C` preserved. `YF/XF` from `A` (undocumented, masked).
    fn rotate_digit_flags(&self, a: u8) -> u8 {
        let mut f = self.flags() & FLAG_C; // C preserved
        if a & 0x80 != 0 {
            f |= FLAG_S;
        }
        if a == 0 {
            f |= FLAG_Z;
        }
        if a.count_ones().is_multiple_of(2) {
            f |= FLAG_PV;
        }
        f |= a & FLAG_XY;
        f
    }

    /// Block transfer `LDI`/`LDD` (+ the `LDIR`/`LDDR` repeats): `(DE) = (HL)`, then `HL`/`DE` step by
    /// `±1` and `BC -= 1`. Flags: `S/Z/C` preserved, `H = N = 0`, `P/V = (BC != 0)`. `YF/XF` come from
    /// `A + (transferred byte)` (undocumented, masked). A repeat variant with `BC != 0` rewinds `PC` by 2.
    fn block_ld<B: Z80Io>(&mut self, bus: &mut B, inc: bool, repeat: bool) -> u32 {
        let val = bus.read(self.hl);
        bus.write(self.de, val);
        if inc {
            self.hl = self.hl.wrapping_add(1);
            self.de = self.de.wrapping_add(1);
        } else {
            self.hl = self.hl.wrapping_sub(1);
            self.de = self.de.wrapping_sub(1);
        }
        self.bc = self.bc.wrapping_sub(1);
        let mut f = self.flags() & (FLAG_S | FLAG_Z | FLAG_C); // preserved; H = N = 0
        if self.bc != 0 {
            f |= FLAG_PV;
        }
        let n = val.wrapping_add(self.a()); // undocumented YF/XF source (masked)
        f |= (n & 0x08) | ((n & 0x02) << 4);
        self.set_flags(f);
        if repeat && self.bc != 0 {
            self.pc = self.pc.wrapping_sub(2);
            21
        } else {
            16
        }
    }

    /// Block search `CPI`/`CPD` (+ the `CPIR`/`CPDR` repeats): compare `A - (HL)` (result discarded), then
    /// `HL` steps by `±1` and `BC -= 1`. Flags: `S/Z/H` from the compare, `N = 1`, `C` preserved,
    /// `P/V = (BC != 0)`. `YF/XF` come from `(result - H)` (undocumented, masked). A repeat variant rewinds
    /// `PC` by 2 while `BC != 0` **and** the compare did not match (`A != (HL)`).
    fn block_cp<B: Z80Io>(&mut self, bus: &mut B, inc: bool, repeat: bool) -> u32 {
        let a = self.a();
        let n = bus.read(self.hl);
        let (result, subf) = sub8(a, n, 0);
        if inc {
            self.hl = self.hl.wrapping_add(1);
        } else {
            self.hl = self.hl.wrapping_sub(1);
        }
        self.bc = self.bc.wrapping_sub(1);
        let mut f = FLAG_N;
        f |= subf & (FLAG_S | FLAG_Z | FLAG_H);
        f |= self.flags() & FLAG_C; // C preserved
        if self.bc != 0 {
            f |= FLAG_PV;
        }
        let temp = result.wrapping_sub((subf & FLAG_H != 0) as u8); // undocumented YF/XF source (masked)
        f |= (temp & 0x08) | ((temp & 0x02) << 4);
        self.set_flags(f);
        let matched = result == 0;
        if repeat && self.bc != 0 && !matched {
            self.pc = self.pc.wrapping_sub(2);
            21
        } else {
            16
        }
    }

    /// Block input `INI`/`IND` (+ the `INIR`/`INDR` repeats): read a byte from port `BC`, store it at
    /// `(HL)`, decrement `B`, and step `HL` by `±1`. Documented flags follow the accepted hardware model
    /// (`N` = value bit 7; `H = C = (value + ((C±1) & 0xFF)) > 0xFF`; `P/V` = parity of `((k & 7) ^ B)`;
    /// `Z = (B == 0)`; `S = B & 0x80`), which the SST corpus records even though Zilog lists them undefined.
    /// The repeating variants apply the extra loop correction to `H`/`P/V` when `B != 0` (see
    /// [`Self::block_io_repeat_flags`]). A repeat with `B != 0` rewinds `PC` by 2.
    fn block_in<B: Z80Io>(&mut self, bus: &mut B, inc: bool, repeat: bool) -> u32 {
        let val = bus.input(self.bc);
        bus.write(self.hl, val);
        let c = self.bc as u8;
        let b = ((self.bc >> 8) as u8).wrapping_sub(1);
        self.bc = (self.bc & 0x00FF) | ((b as u16) << 8);
        if inc {
            self.hl = self.hl.wrapping_add(1);
        } else {
            self.hl = self.hl.wrapping_sub(1);
        }
        let t = if inc {
            c.wrapping_add(1)
        } else {
            c.wrapping_sub(1)
        };
        let k = val as u16 + t as u16;
        let mut f = self.block_io_flags(val, b, k, repeat);
        f |= b & FLAG_XY; // undocumented (masked)
        self.set_flags(f);
        if repeat && b != 0 {
            self.pc = self.pc.wrapping_sub(2);
            21
        } else {
            16
        }
    }

    /// Block output `OUTI`/`OUTD` (+ the `OTIR`/`OTDR` repeats): read `(HL)`, decrement `B`, step `HL` by
    /// `±1`, then write the byte to port `BC` (the port carries the **decremented** `B` on its high half).
    /// Flag model as for [`Self::block_in`], but the carry term uses `L` (after the `HL` step):
    /// `k = value + L`. A repeat with `B != 0` rewinds `PC` by 2.
    fn block_out<B: Z80Io>(&mut self, bus: &mut B, inc: bool, repeat: bool) -> u32 {
        let val = bus.read(self.hl);
        let b = ((self.bc >> 8) as u8).wrapping_sub(1);
        self.bc = (self.bc & 0x00FF) | ((b as u16) << 8);
        if inc {
            self.hl = self.hl.wrapping_add(1);
        } else {
            self.hl = self.hl.wrapping_sub(1);
        }
        bus.output(self.bc, val); // port high = decremented B
        let l = self.hl as u8;
        let k = val as u16 + l as u16;
        let mut f = self.block_io_flags(val, b, k, repeat);
        f |= b & FLAG_XY; // undocumented (masked)
        self.set_flags(f);
        if repeat && b != 0 {
            self.pc = self.pc.wrapping_sub(2);
            21
        } else {
            16
        }
    }

    /// The documented block-I/O flag byte (`S Z H P/V N C`, no `YF/XF`). `val` is the transferred byte,
    /// `b` the post-decrement `B`, `k` the carry-term sum (`value + (C±1)` for input, `value + L` for
    /// output). When `repeat` and the loop continues (`b != 0`), the `H`/`P/V` loop correction is applied.
    fn block_io_flags(&self, val: u8, b: u8, k: u16, repeat: bool) -> u8 {
        let carry = k > 0xFF;
        let mut f = 0u8;
        if val & 0x80 != 0 {
            f |= FLAG_N;
        }
        if carry {
            f |= FLAG_H | FLAG_C;
        }
        let mut pv = (((k & 7) as u8) ^ b).count_ones().is_multiple_of(2);
        if b == 0 {
            f |= FLAG_Z;
        }
        if b & 0x80 != 0 {
            f |= FLAG_S;
        }
        if repeat && b != 0 {
            let (h, p) = self.block_io_repeat_flags(val, b, carry, pv);
            if h {
                f |= FLAG_H;
            } else {
                f &= !FLAG_H;
            }
            pv = p;
        }
        if pv {
            f |= FLAG_PV;
        }
        f
    }

    /// The loop correction (Patrik Rak) applied by the repeating block-I/O ops on every non-terminating
    /// iteration (`B != 0`): recomputes `H` and `P/V` from the base carry, the base parity, and the value's
    /// direction bit (its bit 7). Returns `(hf, pv)`. Derived from the SST corpus (behavioral test data),
    /// clean-room.
    fn block_io_repeat_flags(&self, val: u8, b: u8, carry: bool, base_pv: bool) -> (bool, bool) {
        // Even-parity predicate for the low 3 bits of an adjusted counter value.
        let par3 = |x: u8| (x & 7).count_ones().is_multiple_of(2);
        let hf;
        let p;
        if carry {
            if val & 0x80 != 0 {
                hf = (b & 0x0F) == 0x00;
                p = base_pv ^ par3(b.wrapping_sub(1)) ^ true;
            } else {
                hf = (b & 0x0F) == 0x0F;
                p = base_pv ^ par3(b.wrapping_add(1)) ^ true;
            }
        } else {
            hf = false;
            p = base_pv ^ par3(b) ^ true;
        }
        (hf, p)
    }

    /// `DD`/`FD`-prefixed index-register (`IX`/`IY`) forms. A `DD`/`FD` sets an override for the following
    /// opcode; a run of `DD`/`FD` collapses (each is one M1, the last winning). `DDCB`/`FDCB` fetch the
    /// displacement `d` **before** the final opcode byte (ZC3b) — that irregular order is honored here. The
    /// **documented** index-overridden base opcodes land in [`Self::execute_indexed_base`] and the documented
    /// `DDCB`/`FDCB` bit/shift ops in [`Self::execute_ddcb`]; a prefix on a non-HL opcode is IGNORED (+4 T,
    /// see `execute_indexed_base`'s fall-through). The undocumented `IXH`/`IXL` half-register forms and the
    /// `DDCB`/`FDCB` register-copy variants are implemented and corpus-graded.
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
                self.execute_ddcb(idx, d, op, bus)
            }
            other => self.execute_indexed_base(idx, other, bus),
        }
    }

    /// The **documented** `DD`/`FD`-prefixed base opcodes (this slice): the index register replaces `HL`, and
    /// `(HL)` becomes `(IX+d)`/`(IY+d)` with a signed displacement byte `d` fetched **after** the opcode (and
    /// before the immediate for `LD (IX+d),n`). Covered: `ADD IX,rr`, `LD IX,nn`/`LD (nn),IX`/`LD IX,(nn)`,
    /// `INC IX`/`DEC IX`, `INC (IX+d)`/`DEC (IX+d)`/`LD (IX+d),n`, `LD r,(IX+d)`/`LD (IX+d),r`,
    /// `ALU A,(IX+d)`, `POP IX`/`PUSH IX`/`EX (SP),IX`/`JP (IX)`/`LD SP,IX` (and every `FD`/`IY` counterpart).
    /// For the register↔`(IX+d)` moves the `r` operands are the **real** `B..A` registers (never `IXH`/`IXL`),
    /// since encoding `6` is the memory operand. `INC`/`DEC`/`ALU` on `(IX+d)` set flags exactly like their
    /// `(HL)` counterparts. A `DD`/`FD` prefix on an opcode with **no** `H`/`L`/`HL` involvement is IGNORED
    /// on hardware — the opcode executes exactly as unprefixed, +4 T-states for the prefix fetch ("The
    /// Undocumented Z80 Documented" §5.1; this includes the famous `DD EB` quirk: `EX DE,HL` always swaps
    /// `DE`/`HL`, never `IX`). The undocumented `IXH`/`IXL` half-register ops are implemented here too and
    /// corpus-graded (they have real substituted semantics, so they must NOT fall through to the
    /// ignored-prefix arm); the `DDCB`/`FDCB` group is [`Self::execute_ddcb`].
    fn execute_indexed_base<B: Z80Io>(&mut self, idx: IndexReg, op: u8, bus: &mut B) -> u32 {
        match op {
            // ---- ADD IX,rr (0x09/19/29/39): rr = bits 5..4 (BC/DE/IX/SP — the HL slot is the index reg, so
            // 0x29 is ADD IX,IX). Flags identical to ADD HL,rr, computed over IX as the accumulator. ----
            0x09 | 0x19 | 0x29 | 0x39 => {
                let addend = match (op >> 4) & 3 {
                    0 => self.bc,
                    1 => self.de,
                    2 => self.idx_get(idx),
                    _ => self.sp,
                };
                let result = self.add16(self.idx_get(idx), addend);
                self.idx_set(idx, result);
                15
            }

            // ---- LD IX,nn (0x21). ----
            0x21 => {
                let nn = self.next_word(bus);
                self.idx_set(idx, nn);
                14
            }

            // ---- LD (nn),IX (0x22) / LD IX,(nn) (0x2A): 16-bit, little-endian; no flags. ----
            0x22 => {
                let addr = self.next_word(bus);
                self.write16(addr, self.idx_get(idx), bus);
                20
            }
            0x2A => {
                let addr = self.next_word(bus);
                let v = self.read16(addr, bus);
                self.idx_set(idx, v);
                20
            }

            // ---- INC IX (0x23) / DEC IX (0x2B): 16-bit, no flags. ----
            0x23 => {
                self.idx_set(idx, self.idx_get(idx).wrapping_add(1));
                10
            }
            0x2B => {
                self.idx_set(idx, self.idx_get(idx).wrapping_sub(1));
                10
            }

            // ---- INC (IX+d) (0x34) / DEC (IX+d) (0x35): read-modify-write, flags as INC/DEC (HL). ----
            0x34 => {
                let addr = self.index_addr(idx, bus);
                let v = bus.read(addr);
                let (r, f) = inc8(v, self.flags());
                bus.write(addr, r);
                self.set_flags(f);
                23
            }
            0x35 => {
                let addr = self.index_addr(idx, bus);
                let v = bus.read(addr);
                let (r, f) = dec8(v, self.flags());
                bus.write(addr, r);
                self.set_flags(f);
                23
            }

            // ---- LD (IX+d),n (0x36): fetch order is opcode, d, then n. ----
            0x36 => {
                let addr = self.index_addr(idx, bus);
                let n = self.next_byte(bus);
                bus.write(addr, n);
                19
            }

            // ---- LD r,(IX+d) (0x46/4E/56/5E/66/6E/7E): dst = bits 5..3 (never 6 here — a real B..A reg). ----
            0x46 | 0x4E | 0x56 | 0x5E | 0x66 | 0x6E | 0x7E => {
                let addr = self.index_addr(idx, bus);
                let v = bus.read(addr);
                self.reg8_set((op >> 3) & 7, v, bus);
                19
            }

            // ---- LD (IX+d),r (0x70-0x77 except 0x76): src = bits 0..2 (never 6 here — a real B..A reg). ----
            0x70 | 0x71 | 0x72 | 0x73 | 0x74 | 0x75 | 0x77 => {
                let addr = self.index_addr(idx, bus);
                let v = self.reg8_get(op & 7, bus);
                bus.write(addr, v);
                19
            }

            // ---- ALU A,(IX+d) (0x86/8E/96/9E/A6/AE/B6/BE): op = bits 5..3, flags as ALU A,(HL). ----
            0x86 | 0x8E | 0x96 | 0x9E | 0xA6 | 0xAE | 0xB6 | 0xBE => {
                let addr = self.index_addr(idx, bus);
                let v = bus.read(addr);
                self.alu8((op >> 3) & 7, v);
                19
            }

            // ---- POP IX (0xE1) / PUSH IX (0xE5). ----
            0xE1 => {
                let v = self.read16(self.sp, bus);
                self.sp = self.sp.wrapping_add(2);
                self.idx_set(idx, v);
                14
            }
            0xE5 => {
                self.sp = self.sp.wrapping_sub(2);
                self.write16(self.sp, self.idx_get(idx), bus);
                15
            }

            // ---- EX (SP),IX (0xE3): swap the index register with the word on the top of stack. ----
            0xE3 => {
                let tmp = self.read16(self.sp, bus);
                self.write16(self.sp, self.idx_get(idx), bus);
                self.idx_set(idx, tmp);
                23
            }

            // ---- JP (IX) (0xE9): PC = IX (an indirect load, not a conditional branch). ----
            0xE9 => {
                self.pc = self.idx_get(idx);
                8
            }

            // ---- LD SP,IX (0xF9). ----
            0xF9 => {
                self.sp = self.idx_get(idx);
                10
            }

            // ---- Undocumented IXH/IXL HALF-REGISTER ops (46 op bytes under each prefix). The `DD`/`FD`
            // prefix substitutes `IXH`/`IXL` for `H`/`L` in **both** register-selector slots, uniformly —
            // which is why `LD IXH,IXL` ($65) exists and why $64 is `LD IXH,IXH` rather than the
            // documented `LD H,H`. The one slot that is never substituted is encoding `6`, which names the
            // memory operand and becomes `(IX+d)`; that is why $66/$6E (`LD H,(IX+d)` / `LD L,(IX+d)`) sit
            // with the documented arms above and are absent from the ranges here.
            //
            // ⚑ These would each execute *plausibly wrong* under the ignored-prefix rule — `LD B,IXH` as
            // `LD B,H`, the right shape with the wrong register — and the set is common in hand-optimised
            // Z80 sound drivers, which is exactly what this emulator runs. Corpus-graded against
            // SingleStepTests, not derived. T-states are the unprefixed cost + 4 for the prefix's M1. ----

            // INC/DEC IXH/IXL ($24/$25/$2C/$2D): flags exactly as `INC r`/`DEC r`; `C` preserved.
            0x24 | 0x25 | 0x2C | 0x2D => {
                let sel = (op >> 3) & 7; // 4 = IXH, 5 = IXL
                let v = self.idx_reg8_get(idx, sel);
                let (r, f) = if op & 1 == 0 {
                    inc8(v, self.flags())
                } else {
                    dec8(v, self.flags())
                };
                self.idx_reg8_set(idx, sel, r);
                self.set_flags(f);
                8
            }

            // LD IXH/IXL,n ($26/$2E): 7 T unprefixed + 4.
            0x26 | 0x2E => {
                let n = self.next_byte(bus);
                self.idx_reg8_set(idx, (op >> 3) & 7, n);
                11
            }

            // The 8-bit LD block with an index half in one or both slots. Both slots go through the
            // substituting accessors, so `LD B,IXH`, `LD IXH,A` and `LD IXH,IXL` are one arm, not three.
            // No flags.
            0x44
            | 0x45
            | 0x4C
            | 0x4D
            | 0x54
            | 0x55
            | 0x5C
            | 0x5D
            | 0x60..=0x65
            | 0x67
            | 0x68..=0x6D
            | 0x6F
            | 0x7C
            | 0x7D => {
                let v = self.idx_reg8_get(idx, op & 7);
                self.idx_reg8_set(idx, (op >> 3) & 7, v);
                8
            }

            // ALU A,IXH/IXL ($84/$85 … $BC/$BD): op = bits 5..3, flags exactly as `ALU A,r`.
            0x84 | 0x85 | 0x8C | 0x8D | 0x94 | 0x95 | 0x9C | 0x9D | 0xA4 | 0xA5 | 0xAC | 0xAD
            | 0xB4 | 0xB5 | 0xBC | 0xBD => {
                let v = self.idx_reg8_get(idx, op & 7);
                self.alu8((op >> 3) & 7, v);
                8
            }

            // ---- Every other base opcode has NO H/L/HL involvement, so the DD/FD prefix is IGNORED on
            // hardware: execute exactly as unprefixed, +4 T-states for the prefix's M1 ("The Undocumented
            // Z80 Documented" §5.1; `DD EB` still swaps DE/HL — the documented quirk). Reached for real
            // by `m68k_opcode_sizes` (`DD 00` in its word-uploaded Z80 program under the Q4 one-byte
            // window rule) and by Vectorman's driver (`FD FF`, the pre-existing 26-frame panic). ----
            other => 4 + self.execute_base(other, bus),
        }
    }

    /// Read the index register selected by a `DD`/`FD` prefix.
    fn idx_get(&self, idx: IndexReg) -> u16 {
        match idx {
            IndexReg::Ix => self.ix,
            IndexReg::Iy => self.iy,
        }
    }

    /// Write the index register selected by a `DD`/`FD` prefix.
    fn idx_set(&mut self, idx: IndexReg, v: u16) {
        match idx {
            IndexReg::Ix => self.ix = v,
            IndexReg::Iy => self.iy = v,
        }
    }

    /// Read the 8-bit operand a `DD`/`FD`-prefixed opcode names by its 3-bit selector, **with the index
    /// halves substituted for `H`/`L`**: `0=B 1=C 2=D 3=E 4=IXH 5=IXL 7=A`. The [`Self::reg8_get`] of the
    /// prefixed world.
    ///
    /// Selector `6` is the memory operand — under a prefix it is `(IX+d)`, which needs a displacement
    /// fetch and is handled by the documented arms — so it never reaches here. It maps to `A` only because
    /// the match must be total; every caller's opcode set excludes it by construction.
    fn idx_reg8_get(&self, idx: IndexReg, sel: u8) -> u8 {
        match sel & 7 {
            0 => (self.bc >> 8) as u8,
            1 => self.bc as u8,
            2 => (self.de >> 8) as u8,
            3 => self.de as u8,
            4 => (self.idx_get(idx) >> 8) as u8,
            5 => self.idx_get(idx) as u8,
            _ => self.a(),
        }
    }

    /// Write the 8-bit operand a `DD`/`FD`-prefixed opcode names by its 3-bit selector, with the index
    /// halves substituted for `H`/`L`. The inverse of [`Self::idx_reg8_get`], with the same note on
    /// selector `6`.
    fn idx_reg8_set(&mut self, idx: IndexReg, sel: u8, val: u8) {
        match sel & 7 {
            0 => self.bc = (self.bc & 0x00FF) | ((val as u16) << 8),
            1 => self.bc = (self.bc & 0xFF00) | val as u16,
            2 => self.de = (self.de & 0x00FF) | ((val as u16) << 8),
            3 => self.de = (self.de & 0xFF00) | val as u16,
            4 => self.idx_set(idx, (self.idx_get(idx) & 0x00FF) | ((val as u16) << 8)),
            5 => self.idx_set(idx, (self.idx_get(idx) & 0xFF00) | val as u16),
            _ => self.set_a(val),
        }
    }

    /// Fetch the signed displacement byte `d` (advancing `PC`, no refresh bump) and form the `(IX+d)`/`(IY+d)`
    /// effective address `index.wrapping_add(d as i8 as u16)`.
    fn index_addr<B: Z80Io>(&mut self, idx: IndexReg, bus: &mut B) -> u16 {
        let d = self.next_byte(bus) as i8;
        self.idx_get(idx).wrapping_add(d as u16)
    }

    /// 16-bit `ADD` core shared by `ADD HL,rr` and `ADD IX/IY,rr`: `augend + addend`, setting `N = 0`,
    /// `H` = carry out of bit 11, `C` = carry out of bit 15; `S/Z/P/V` preserved. `YF/XF` come from the
    /// result's high byte (undocumented, masked out of the documented-flag gate). Returns the 16-bit result.
    fn add16(&mut self, augend: u16, addend: u16) -> u16 {
        let sum = augend as u32 + addend as u32;
        let result = sum as u16;
        let mut f = self.flags() & (FLAG_S | FLAG_Z | FLAG_PV); // preserved; N = 0
        if (augend & 0x0FFF) + (addend & 0x0FFF) > 0x0FFF {
            f |= FLAG_H;
        }
        if sum > 0xFFFF {
            f |= FLAG_C;
        }
        f |= (result >> 8) as u8 & FLAG_XY;
        self.set_flags(f);
        result
    }

    /// `DDCB`/`FDCB` indexed rotate/shift/bit ops (the displacement `d` is already fetched — the ZC3b
    /// decode quirk: after the `DD`/`FD` and `CB` prefix bytes, the signed `d` is fetched **before** the
    /// final opcode byte). The operation targets `(IX+d)`/`(IY+d)`. Decoded by the op's high two bits like
    /// the `CB` group: `0x00-0x3F` = `RLC RRC RL RR SLA SRA SLL SRL` (op = bits 5..3), `0x40-0x7F` =
    /// `BIT b`, `0x80-0xBF` = `RES b`, `0xC0-0xFF` = `SET b` (b = bits 5..3). The rotates/shifts set the
    /// full documented flag set (`S Z` from the result, `H = N = 0`, `P/V` = parity, `C` = the shifted-out
    /// bit) and read-modify-write `(IX+d)`; `BIT b,(IX+d)` sets `Z = NOT(bit)`, `H = 1`, `N = 0`,
    /// `S = (b==7 && set)`, `P/V = Z` (its `YF/XF` come from an internal address source, masked out of the
    /// documented gate); `RES`/`SET` clear/set bit `b` of `(IX+d)` with no flags.
    ///
    /// **All 256 op bytes are served**, documented and undocumented. The op byte's low 3 bits are the
    /// `(HL)`-slot encoding, which under this prefix is always the indexed address — leaving those bits
    /// free, and on hardware they name a `B..A` register that receives a **second copy** of the result.
    /// `6` is the documented form and copies nowhere; the other seven are the undocumented register-copy
    /// variants. `BIT` is the exception: it produces no value, so its register slot is inert.
    fn execute_ddcb<B: Z80Io>(&mut self, idx: IndexReg, d: i8, op: u8, bus: &mut B) -> u32 {
        let addr = self.idx_get(idx).wrapping_add(d as u16);
        // The undocumented REGISTER-COPY slot. In the `CB` group the op byte's low 3 bits name the target;
        // under `DDCB`/`FDCB` the target is always `(IX+d)`, so those 3 bits are free — and on hardware they
        // name a `B..A` register that receives a **second copy** of the result. Encoding `6` is the
        // documented form (the one the corpus's `dd cb __ x6` files cover) and copies nowhere.
        //
        // ⚑ The register is a PLAIN `B..A`, never `IXH`/`IXL`: the index register is spoken for as the
        // address, so no substitution applies here. And `6` never reaches `reg8_set` below, so this can
        // never write `(HL)`.
        let copy_to = (op & 7 != 6).then_some(op & 7);
        match op {
            // ---- Rotates/shifts (op bits 5..3 select RLC/RRC/RL/RR/SLA/SRA/SLL/SRL): full documented flag
            // set, read-modify-write of (IX+d), plus the undocumented copy. ----
            0x00..=0x3F => {
                let v = bus.read(addr);
                let (r, carry) = self.rotate_shift((op >> 3) & 7, v);
                bus.write(addr, r);
                if let Some(sel) = copy_to {
                    self.reg8_set(sel, r, bus);
                }
                self.set_flags(shift_rotate_flags(r, carry));
                23
            }
            // ---- BIT b,(IX+d): test bit b; Z = NOT(bit), H = 1, N = 0, S = (b==7 && set), P/V = Z, C
            // preserved. No target write — and **no register copy either**: `BIT` produces no value, so the
            // register slot is inert for all eight encodings of each `b`. This is the one quarter of the
            // table where the undocumented variants are behaviourally identical to the documented form. ----
            0x40..=0x7F => {
                let v = bus.read(addr);
                self.op_bit((op >> 3) & 7, v);
                20
            }
            // ---- RES b,(IX+d): clear bit b; no flags; plus the undocumented copy. ----
            0x80..=0xBF => {
                let b = (op >> 3) & 7;
                let r = bus.read(addr) & !(1 << b);
                bus.write(addr, r);
                if let Some(sel) = copy_to {
                    self.reg8_set(sel, r, bus);
                }
                23
            }
            // ---- SET b,(IX+d): set bit b; no flags; plus the undocumented copy. ----
            _ => {
                let b = (op >> 3) & 7;
                let r = bus.read(addr) | (1 << b);
                bus.write(addr, r);
                if let Some(sel) = copy_to {
                    self.reg8_set(sel, r, bus);
                }
                23
            }
        }
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

/// 8-bit `INC`: `v + 1` with the documented flags — `S Z H P/V` from the result, `N = 0`, `C` **preserved**
/// (carried in via `old_flags`). `H` = carry out of bit 3 (low nibble was `0x0F`); `P/V` = overflow (`v`
/// was `0x7F`). `YF/XF` from the result's bits 5/3 (masked out of the documented gate).
fn inc8(v: u8, old_flags: u8) -> (u8, u8) {
    let r = v.wrapping_add(1);
    let mut f = old_flags & FLAG_C; // preserve carry, clear N
    if r & 0x80 != 0 {
        f |= FLAG_S;
    }
    if r == 0 {
        f |= FLAG_Z;
    }
    if (v & 0x0F) == 0x0F {
        f |= FLAG_H;
    }
    if v == 0x7F {
        f |= FLAG_PV;
    }
    f |= r & FLAG_XY;
    (r, f)
}

/// 8-bit `DEC`: `v - 1` with the documented flags — `S Z H P/V` from the result, `N = 1`, `C` **preserved**.
/// `H` = borrow from bit 4 (low nibble was `0x00`); `P/V` = overflow (`v` was `0x80`).
fn dec8(v: u8, old_flags: u8) -> (u8, u8) {
    let r = v.wrapping_sub(1);
    let mut f = (old_flags & FLAG_C) | FLAG_N; // preserve carry, set N
    if r & 0x80 != 0 {
        f |= FLAG_S;
    }
    if r == 0 {
        f |= FLAG_Z;
    }
    if (v & 0x0F) == 0x00 {
        f |= FLAG_H;
    }
    if v == 0x80 {
        f |= FLAG_PV;
    }
    f |= r & FLAG_XY;
    (r, f)
}

/// Flags for the `CB`-space rotates/shifts (`RLC`/`RRC`/`RL`/`RR`/`SLA`/`SRA`/`SLL`/`SRL`): the **full**
/// documented set — `S`/`Z` from the result, `H = N = 0`, `P/V` = even parity, `C` from the shifted-out bit.
/// `YF/XF` come from the result's bits 5/3 (undocumented, masked out of the documented-flag gate).
fn shift_rotate_flags(result: u8, carry: bool) -> u8 {
    let mut f = 0u8;
    if result & 0x80 != 0 {
        f |= FLAG_S;
    }
    if result == 0 {
        f |= FLAG_Z;
    }
    if result.count_ones().is_multiple_of(2) {
        f |= FLAG_PV; // parity even
    }
    if carry {
        f |= FLAG_C;
    }
    f |= result & FLAG_XY;
    f
}

/// Flags for the accumulator rotates (`RLCA`/`RRCA`/`RLA`/`RRA`): `H = N = 0`, `S/Z/P/V` preserved, `C` set
/// from the rotated-out bit, `YF/XF` from the result (undocumented, masked).
fn rotate_a_flags(old_flags: u8, result: u8, carry: bool) -> u8 {
    let mut f = old_flags & (FLAG_S | FLAG_Z | FLAG_PV);
    if carry {
        f |= FLAG_C;
    }
    f |= result & FLAG_XY;
    f
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
    fn reset_clears_the_defined_state_and_preserves_the_register_file() {
        // UM008 §"RESET" / design ZC9: the reset DEFINES PC = I = R = 0, IFF1 = IFF2 = 0, IM = 0 and lifts
        // HALT; SP and the main/index registers are architecturally undefined and are preserved here.
        let dirty = Z80Regs {
            a: 0x77,
            f: 0x5A,
            b: 0x11,
            c: 0x22,
            d: 0x33,
            e: 0x44,
            h: 0x55,
            l: 0x66,
            af_: 0x1234,
            bc_: 0x5678,
            de_: 0x9ABC,
            hl_: 0xDEF0,
            ix: 0x0BAD,
            iy: 0xF00D,
            sp: 0xFFF0,
            pc: 0x1234,
            i: 0x5A,
            r: 0x3C,
            iff1: true,
            iff2: true,
            im: 2,
            halted: true,
            wz: 0x4321,
            q: 0xAB,
        };
        let mut z = Z80::from_regs(&dirty);
        z.reset();
        let r = z.regs();
        assert_eq!(r.pc, 0, "PC cleared");
        assert_eq!(r.i, 0, "I cleared");
        assert_eq!(r.r, 0, "R cleared");
        assert!(!r.iff1 && !r.iff2, "both interrupt flip-flops cleared");
        assert_eq!(r.im, 0, "interrupt mode 0");
        assert!(!r.halted, "HALT lifted");
        assert_eq!((r.wz, r.q), (0, 0), "internal WZ/Q cleared");
        assert_eq!(
            Z80Regs {
                pc: 0,
                i: 0,
                r: 0,
                iff1: false,
                iff2: false,
                im: 0,
                halted: false,
                wz: 0,
                q: 0,
                ..dirty
            },
            r,
            "nothing outside the defined set moved: the register file is preserved"
        );
        // Applied at the power-on anchor the reset is the identity, so ZC9's all-zero pin — and the
        // export_state region-4 golden that rests on it — cannot move.
        let mut fresh = Z80::new();
        fresh.reset();
        assert_eq!(fresh, Z80::new(), "reset at power-on is the identity");
        assert_eq!(
            fresh.export_region(),
            [0u8; 30],
            "the reset state still serializes as all-zero (ZC9)"
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
            // A latched refusal rides the snapshot too: a rewind must not resurrect a Z80 that was
            // stopped, and a checkpoint taken after a fault must restore into the same stopped machine.
            fault: Some(Z80Fault {
                pc: 0x1E1F,
                bytes: [0xDD, 0xCB, 0x20, 0x21],
                len: 4,
            }),
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

    #[test]
    fn undocumented_ed_holes_execute_as_noni_8t_nops() {
        // The Undocumented Z80 Documented §5.4: every ED opcode outside the documented set and the
        // real-semantics mirrors is NONI — no effect, 8 T-states, PC past the 2-byte opcode, R bumped
        // twice by the two M1 fetches. Reached for real by `m68k_opcode_sizes` (its Z80 program is
        // word-uploaded, so only the high bytes land — Q4). A representative hole from each hole band:
        struct Mem([u8; 0x100]);
        impl Z80Io for Mem {
            fn read(&mut self, addr: u16) -> u8 {
                self.0[addr as usize & 0xFF]
            }
            fn write(&mut self, addr: u16, value: u8) {
                self.0[addr as usize & 0xFF] = value;
            }
            fn input(&mut self, _port: u16) -> u8 {
                0xFF
            }
            fn output(&mut self, _port: u16, _value: u8) {}
        }
        for hole in [0x00u8, 0x3F, 0x77, 0x7F, 0x9E, 0xA5, 0xAD, 0xB6, 0xBC, 0xFF] {
            let mut mem = Mem([0; 0x100]);
            mem.0[0] = 0xED;
            mem.0[1] = hole;
            let mut z = Z80::new();
            let before = z.clone();
            let t = z.step(&mut mem);
            assert_eq!(t, 8, "ED {hole:#04X}: 8 T-states");
            assert_eq!(z.pc, 2, "ED {hole:#04X}: PC past the 2-byte opcode");
            assert_eq!(z.r, 2, "ED {hole:#04X}: two M1 refreshes");
            let neutral = Z80 {
                pc: z.pc,
                r: z.r,
                ..before
            };
            assert_eq!(z, neutral, "ED {hole:#04X}: no other state effect");
        }
    }

    #[test]
    fn dd_fd_prefix_is_ignored_on_non_hl_opcodes() {
        // The Undocumented Z80 Documented §5.1: a DD/FD prefix before an opcode with no H/L/HL
        // involvement is ignored — the opcode executes exactly as unprefixed, +4 T for the prefix M1.
        // Reached for real by m68k_opcode_sizes (DD 00) and Vectorman (FD FF).
        struct Mem([u8; 0x100]);
        impl Z80Io for Mem {
            fn read(&mut self, addr: u16) -> u8 {
                self.0[addr as usize & 0xFF]
            }
            fn write(&mut self, addr: u16, value: u8) {
                self.0[addr as usize & 0xFF] = value;
            }
            fn input(&mut self, _port: u16) -> u8 {
                0xFF
            }
            fn output(&mut self, _port: u16, _value: u8) {}
        }

        // DD 00 = prefixed NOP: 4 (prefix) + 4 (NOP).
        let mut mem = Mem([0; 0x100]);
        mem.0[0] = 0xDD;
        mem.0[1] = 0x00;
        let mut z = Z80::new();
        assert_eq!(z.step(&mut mem), 8, "DD 00 = 8 T");
        assert_eq!(z.pc, 2);
        assert_eq!(z.r, 2, "prefix + opcode M1 refreshes");

        // DD EB — the famous quirk: EX DE,HL ALWAYS swaps DE/HL, never IX.
        let mut mem = Mem([0; 0x100]);
        mem.0[0] = 0xDD;
        mem.0[1] = 0xEB;
        let mut z = Z80 {
            de: 0x1234,
            hl: 0x5678,
            ix: 0x9ABC,
            ..Z80::new()
        };
        assert_eq!(z.step(&mut mem), 8, "DD EB = 4 + 4 T");
        assert_eq!(z.de, 0x5678, "DE <- HL (not IX)");
        assert_eq!(z.hl, 0x1234, "HL <- DE");
        assert_eq!(z.ix, 0x9ABC, "IX untouched");

        // FD FF = prefixed RST 38h: pushes PC-past-instruction, jumps to $0038; 4 + 11 T.
        let mut mem = Mem([0; 0x100]);
        mem.0[0] = 0xFD;
        mem.0[1] = 0xFF;
        let mut z = Z80 {
            sp: 0x0080,
            ..Z80::new()
        };
        assert_eq!(z.step(&mut mem), 15, "FD FF = 4 + 11 T");
        assert_eq!(z.pc, 0x0038, "RST 38h vector");
        assert_eq!(z.sp, 0x007E);
        assert_eq!(
            u16::from_le_bytes([mem.0[0x7E], mem.0[0x7F]]),
            2,
            "return address = past the 2-byte prefixed RST"
        );
    }

    #[test]
    fn export_region_reset_is_all_zero() {
        // The go-live guarantee: at reset the 30-byte export region is all zero, so export_state region 4
        // stays byte-frozen and the golden never moves (all-zero reset model, ZC9).
        assert_eq!(Z80::new().export_region(), [0u8; 30]);
    }

    #[test]
    fn export_region_lays_out_registers_at_fixed_offsets() {
        // Prove region 4 is genuinely DRIVEN from the struct (not still a hardcoded zero fill): distinct
        // sentinels in every field must land at their pinned ZC9 little-endian offsets.
        let z = Z80::from_regs(&Z80Regs {
            a: 0xA1,
            f: 0xF2, // AF = 0xA1F2
            b: 0xB3,
            c: 0xC4, // BC = 0xB3C4
            d: 0xD5,
            e: 0xE6, // DE = 0xD5E6
            h: 0x17,
            l: 0x28, // HL = 0x1728
            af_: 0x090A,
            bc_: 0x0B0C,
            de_: 0x0D0E,
            hl_: 0x0F10,
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
            wz: 0x1B1C,
            q: 0x1D,
        });
        let b = z.export_region();
        assert_eq!(&b[0..2], &0xA1F2u16.to_le_bytes(), "AF");
        assert_eq!(&b[2..4], &0xB3C4u16.to_le_bytes(), "BC");
        assert_eq!(&b[4..6], &0xD5E6u16.to_le_bytes(), "DE");
        assert_eq!(&b[6..8], &0x1728u16.to_le_bytes(), "HL");
        assert_eq!(&b[8..10], &0x090Au16.to_le_bytes(), "AF'");
        assert_eq!(&b[14..16], &0x0F10u16.to_le_bytes(), "HL'");
        assert_eq!(&b[16..18], &0x1112u16.to_le_bytes(), "IX");
        assert_eq!(&b[18..20], &0x1314u16.to_le_bytes(), "IY");
        assert_eq!(&b[20..22], &0x1516u16.to_le_bytes(), "SP");
        assert_eq!(&b[22..24], &0x1718u16.to_le_bytes(), "PC");
        assert_eq!(b[24], 0x19, "I");
        assert_eq!(b[25], 0x1A, "R");
        // IFF1·IFF2·IM packed: iff1=1, iff2=0, im=2 → 0b0000_1001 = 0x09.
        assert_eq!(b[26], 0b0000_1001, "IFF/IM packed");
        assert_eq!(b[27], 1, "HALT");
        assert_eq!(&b[28..30], &0x1B1Cu16.to_le_bytes(), "WZ");
    }

    /// A **latched refusal stops the core**, and only a `/RESET` restarts it. No guest byte reaches
    /// [`Z80::refuse`] any more (every encoding is served), so this drives it directly — the behaviour it
    /// guards is the landing pad's contract, not any particular opcode's.
    #[test]
    fn a_latched_refusal_stops_the_core() {
        struct Mem([u8; 0x100]);
        impl Z80Io for Mem {
            fn read(&mut self, addr: u16) -> u8 {
                self.0[addr as usize & 0xFF]
            }
            fn write(&mut self, addr: u16, value: u8) {
                self.0[addr as usize & 0xFF] = value;
            }
            fn input(&mut self, _port: u16) -> u8 {
                0xFF
            }
            fn output(&mut self, _port: u16, _value: u8) {}
        }
        let mut bus = Mem([0x3C; 0x100]); // INC A everywhere: a leak would move A immediately.
        let mut z80 = Z80::new();
        z80.set_a(0x10);

        let t = z80.refuse([0xDD, 0xCB, 0x05, 0x00], 4);
        assert!(
            t > 0,
            "a refusal must burn T-states so catch-up loops terminate"
        );
        let fault = z80.fault().expect("the refusal latched");
        assert_eq!(fault.len, 4);
        assert_eq!(&fault.bytes[..4], &[0xDD, 0xCB, 0x05, 0x00]);

        // Interrupts enabled BEFORE the baseline snapshot: this test must show the core declining to
        // vector, not merely record that the test itself flipped a flag afterwards.
        z80.iff1 = true;
        let stopped = z80.regs();
        for _ in 0..64 {
            assert!(
                z80.step(&mut bus) > 0,
                "a stopped Z80 still advances the clock"
            );
        }
        assert_eq!(
            z80.regs(),
            stopped,
            "a stopped Z80 executes nothing — not the next instruction, not anything"
        );

        // Not even an enabled, asserted interrupt: vectoring would resume a program already refused.
        z80.set_int_line(true);
        z80.step(&mut bus);
        assert_eq!(
            z80.regs(),
            stopped,
            "a stopped Z80 must not vector an interrupt into a program it refused to run"
        );

        z80.reset();
        assert!(z80.fault().is_none(), "/RESET clears the latched refusal");
        z80.step(&mut bus);
        assert_ne!(z80.regs(), stopped, "after /RESET the core runs again");
    }

    /// The refusal's diagnostic must name the **bytes** and the **address** — the whole point is that a
    /// human reading a log learns which byte the emulator could not run.
    #[test]
    fn the_refusal_diagnostic_names_the_bytes_and_the_address() {
        let f = Z80Fault {
            pc: 0x0100,
            bytes: [0xDD, 0xCB, 0x05, 0x00],
            len: 4,
        };
        let text = f.to_string();
        assert!(text.contains("DD CB 05 00"), "must name the bytes: {text}");
        assert!(text.contains("$0100"), "must name the address: {text}");
    }
}
