//! The word-granular, FC-aware bus the 68000 talks to.
//!
//! The 68000 has a 16-bit data bus and a 24-bit address bus: every access is a word (byte/word = one
//! access, long = two word accesses at `addr` and `addr+2`), and each carries a function code (FC0–FC2)
//! classifying it (supervisor/user × data/program). This is kept separate from the generic
//! [`crate::bus::Bus`] for now; unifying them — adding the function code to `BusEvent` — is a follow-up
//! once the micro-op stepping model is in place (the real VDP/DMA semantics land with it).

/// 68000 address bus width (24 bits). Every access is masked to this.
pub const ADDR_MASK: u32 = 0x00FF_FFFF;

/// Read or write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxKind {
    Read,
    Write,
}

/// One word bus transaction, in the order it happened. `fc` is the 68000 function code (5 = supervisor
/// data, 6 = supervisor program, etc.). `addr` is already masked to the 24-bit bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub kind: TxKind,
    pub fc: u8,
    pub addr: u32,
    pub value: u16,
}

/// The word-granular bus the 68000 prototype/framework talks to (FC-aware).
pub trait Bus68k {
    fn read16(&mut self, addr: u32, fc: u8) -> u16;
    fn write16(&mut self, addr: u32, fc: u8, value: u16);
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
            value,
        });
    }
}
