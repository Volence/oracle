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

impl Z80 {
    /// Power on in the reset state (ZC9): the Z80 `/RESET` strictly defines `PC = 0`, `I = 0`, `R = 0`,
    /// `IFF1 = IFF2 = 0`, `IM = 0`, not halted; SP and the main/index registers are architecturally
    /// undefined and pinned here to **all-zero** (a legitimate reset model that keeps `export_state`
    /// region 4 frozen at go-live — see the design's reset-fill call). Identical to [`Z80::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute one Z80 instruction over `bus`, returning the T-states it consumed (the value the ×15 mclk
    /// catch-up in [`crate::system::System::run_until`] scales into the frontier).
    ///
    /// **Stub — the Z-skeleton slice does not execute.** The full documented opcode set is the Z-execute
    /// slice (ZC3b/ZC14). Every committed fixture holds the Z80 in reset (power-on `z80_running = false`),
    /// so `run_until`'s gated catch-up loop never calls this; the `todo!` is a deliberate, explicit hard
    /// stop that fires loudly if that reset invariant is ever violated before Z-execute lands.
    pub fn step<B: Z80Io>(&mut self, _bus: &mut B) -> u32 {
        todo!("Z80 instruction execution is the Z-execute slice; the Z-skeleton holds the Z80 in reset")
    }
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
