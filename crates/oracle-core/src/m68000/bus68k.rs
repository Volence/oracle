//! The FC-aware bus the 68000 talks to.
//!
//! The 68000 has a 16-bit data bus and a 24-bit address bus: a byte or a word is one access (a byte drives
//! one bus half — UDS for an even address, LDS for an odd one), a long is two word accesses at `addr` and
//! `addr+2`, and each carries a function code (FC0–FC2) classifying it (supervisor/user × data/program).
//! This is kept separate from the generic [`crate::bus::Bus`] for now; unifying them — adding the function
//! code to `BusEvent` — is a follow-up once the micro-op stepping model is in place (the real VDP/DMA
//! semantics land with it).

use super::microop::Size;

/// 68000 address bus width (24 bits). Every access is masked to this.
pub const ADDR_MASK: u32 = 0x00FF_FFFF;

/// Read or write — plus [`TxKind::Tas`], the indivisible test-and-set read-modify-write the 68000 performs
/// as ONE locked bus cycle (the SST stream's `'t'` token), distinct from a separate `Read`+`Write` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxKind {
    Read,
    Write,
    /// The atomic `TAS` read-modify-write: a single indivisible bus cycle that reads the byte and writes it
    /// back with bit 7 set. Logged as ONE transaction whose `value` is the WRITTEN byte (`orig | 0x80`).
    Tas,
}

/// One bus transaction, in the order it happened. `fc` is the 68000 function code (5 = supervisor
/// data, 6 = supervisor program, etc.). `addr` is already masked to the 24-bit bus. `size` records
/// whether this was a byte or a word access (matching the SST stream's size token); for a byte access
/// `value` is the on-bus byte zero-extended into the low 8 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub kind: TxKind,
    pub fc: u8,
    pub addr: u32,
    pub size: Size,
    pub value: u16,
}

/// The FC-aware bus the 68000 prototype/framework talks to. The 68000 data bus is 16 bits, so a word is
/// one access and a byte drives a single bus half (UDS for an even address → the upper byte, LDS for an
/// odd address → the lower byte); a long is **two** word accesses (the micro-op builder emits the two
/// `read16`/`write16` halves itself), so the bus exposes no separate long primitive.
pub trait Bus68k {
    fn read16(&mut self, addr: u32, fc: u8) -> u16;
    fn write16(&mut self, addr: u32, fc: u8, value: u16);
    /// Read the byte at `addr` (the addressed cell; the UDS/LDS half is an electrical detail — the value
    /// is `mem[addr]`).
    fn read8(&mut self, addr: u32, fc: u8) -> u8;
    /// Write `value` to the byte at `addr`.
    fn write8(&mut self, addr: u32, fc: u8, value: u8);
    /// The indivisible `TAS` read-modify-write: atomically read the byte `orig` at `addr`, write
    /// `orig | 0x80` back, and return `orig`. ONE locked bus cycle — the SST stream's single `'t'`
    /// transaction (logged with `value = orig | 0x80`, the WRITTEN byte), NOT a separate read+write pair.
    /// Byte-only → never faults on parity.
    fn tas(&mut self, addr: u32, fc: u8) -> u8;
}

/// A flat 16 MiB recording bus for tests/diagnostics: big-endian word access over the 24-bit space,
/// logging every transaction in order.
pub struct FlatBus {
    mem: Vec<u8>,
    pub log: Vec<Transaction>,
}

impl FlatBus {
    pub fn new() -> Self {
        Self {
            mem: vec![0u8; 0x0100_0000],
            log: Vec::new(),
        }
    }

    /// Raw byte poke (not logged) — used to set up initial memory.
    pub fn poke(&mut self, addr: u32, val: u8) {
        self.mem[(addr & ADDR_MASK) as usize] = val;
    }

    /// Raw byte peek (not logged).
    pub fn peek(&self, addr: u32) -> u8 {
        self.mem[(addr & ADDR_MASK) as usize]
    }
}

impl Default for FlatBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus68k for FlatBus {
    fn read16(&mut self, addr: u32, fc: u8) -> u16 {
        let a = (addr & ADDR_MASK) as usize;
        let b = ((addr.wrapping_add(1)) & ADDR_MASK) as usize;
        let value = ((self.mem[a] as u16) << 8) | self.mem[b] as u16;
        self.log.push(Transaction {
            kind: TxKind::Read,
            fc,
            addr: addr & ADDR_MASK,
            size: Size::Word,
            value,
        });
        value
    }

    fn write16(&mut self, addr: u32, fc: u8, value: u16) {
        let a = (addr & ADDR_MASK) as usize;
        let b = ((addr.wrapping_add(1)) & ADDR_MASK) as usize;
        self.mem[a] = (value >> 8) as u8;
        self.mem[b] = (value & 0xFF) as u8;
        self.log.push(Transaction {
            kind: TxKind::Write,
            fc,
            addr: addr & ADDR_MASK,
            size: Size::Word,
            value,
        });
    }

    fn read8(&mut self, addr: u32, fc: u8) -> u8 {
        // A byte access drives one half of the 16-bit data bus (UDS for an even address → the upper byte;
        // LDS for an odd address → the lower byte); either way the addressed memory cell is `mem[addr]`,
        // and the on-bus value the SST suite records is that single byte. Logged byte-granular (`Size::Byte`)
        // with the byte zero-extended into the `u16` value field.
        let a = (addr & ADDR_MASK) as usize;
        let value = self.mem[a];
        self.log.push(Transaction {
            kind: TxKind::Read,
            fc,
            addr: addr & ADDR_MASK,
            size: Size::Byte,
            value: value as u16,
        });
        value
    }

    fn write8(&mut self, addr: u32, fc: u8, value: u8) {
        let a = (addr & ADDR_MASK) as usize;
        self.mem[a] = value;
        self.log.push(Transaction {
            kind: TxKind::Write,
            fc,
            addr: addr & ADDR_MASK,
            size: Size::Byte,
            value: value as u16,
        });
    }

    fn tas(&mut self, addr: u32, fc: u8) -> u8 {
        // The indivisible test-and-set: read `orig`, write `orig | 0x80`, log ONE Tas transaction whose
        // value is the WRITTEN byte, return `orig`. A single locked RMW bus cycle (the SST `'t'` token) —
        // never a separate read+write pair. Byte-granular (`Size::Byte`).
        let a = (addr & ADDR_MASK) as usize;
        let orig = self.mem[a];
        self.mem[a] = orig | 0x80;
        self.log.push(Transaction {
            kind: TxKind::Tas,
            fc,
            addr: addr & ADDR_MASK,
            size: Size::Byte,
            value: (orig | 0x80) as u16,
        });
        orig
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `read8` returns the addressed cell and logs a byte-granular transaction whose value is that single
    /// byte (zero-extended). Pinned against the real SST `ADD.b (A1),D7` anchor case (`de11`): the byte at
    /// the EVEN address `0x97EA9E` is `0x45` (69), driven on the UDS half, and the suite records the value
    /// as the byte itself (69) — NOT shifted into a word half.
    #[test]
    fn read8_even_address_returns_upper_half_byte_and_logs_byte_transaction() {
        let mut bus = FlatBus::new();
        bus.poke(0x97_EA9E, 0x45);
        let v = bus.read8(0x97_EA9E, 5);
        assert_eq!(
            v, 0x45,
            "read8 returns the addressed byte (SST anchor de11)"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 0x97_EA9E,
                size: Size::Byte,
                value: 0x45, // the on-bus byte, zero-extended — pinned to the real case
            }]
        );
    }

    /// `read8` at an ODD address drives the LDS half but still returns `mem[addr]` and logs the raw byte.
    /// Pinned against the real SST byte read at the odd address `13367077` (`0xCBFFA5`) with value `0xE4`.
    #[test]
    fn read8_odd_address_returns_lower_half_byte() {
        let mut bus = FlatBus::new();
        bus.poke(13_367_077, 0xE4);
        let v = bus.read8(13_367_077, 5);
        assert_eq!(
            v, 0xE4,
            "read8 at an odd address returns the addressed byte"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Read,
                fc: 5,
                addr: 13_367_077,
                size: Size::Byte,
                value: 0xE4,
            }]
        );
    }

    /// `tas` is the indivisible test-and-set bus cycle: atomically READ `orig`, WRITE `orig | 0x80`, and
    /// return `orig`, logging ONE `Tas` transaction whose value is the WRITTEN byte (`orig | 0x80`). Pinned
    /// to the real SST `4ad2 [TAS (A2)]` anchor's `'t'` transaction `['t', 10, 5, 2840449, '.b', 181]`: at
    /// `addr = 2840449` (fc 5), the written byte is `181 = 0xB5`, so `orig = 0x35` and `mem` ends `0xB5`.
    #[test]
    fn tas_reads_orig_writes_or_0x80_and_logs_one_tas_transaction() {
        let mut bus = FlatBus::new();
        bus.poke(2_840_449, 0x35);
        let orig = bus.tas(2_840_449, 5);
        assert_eq!(
            orig, 0x35,
            "tas returns the pre-modify byte (the READ value)"
        );
        assert_eq!(
            bus.peek(2_840_449),
            0xB5,
            "tas writes orig | 0x80 (0x35 | 0x80 == 0xB5)"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Tas,
                fc: 5,
                addr: 2_840_449,
                size: Size::Byte,
                value: 0xB5, // the WRITTEN byte (orig | 0x80) — pinned to the anchor's `'t'` value 181
            }],
            "tas logs exactly ONE Tas transaction (value = the written byte)"
        );
    }

    /// `write8` stores a single byte at the exact address and logs the raw byte (pinned to the real SST
    /// byte write at the odd address `13367077` with value `0xA3`).
    #[test]
    fn write8_stores_single_byte_and_logs_it() {
        let mut bus = FlatBus::new();
        bus.write8(13_367_077, 5, 0xA3);
        assert_eq!(bus.peek(13_367_077), 0xA3, "the addressed byte was written");
        assert_eq!(
            bus.peek(13_367_078),
            0x00,
            "the neighbouring byte is untouched (single-byte access)"
        );
        assert_eq!(
            bus.log,
            vec![Transaction {
                kind: TxKind::Write,
                fc: 5,
                addr: 13_367_077,
                size: Size::Byte,
                value: 0xA3,
            }]
        );
    }
}
