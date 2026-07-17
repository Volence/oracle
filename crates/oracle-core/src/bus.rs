//! The typed `Bus` protocol + the `SystemBus` split-borrow adapter.
//!
//! Chips never touch memory directly; each step they borrow a transient `&mut SystemBus` (only one
//! `&mut` live at a time, monomorphized, zero dispatch — no `Rc`/`RefCell`/raw pointers). Every access
//! emits a [`BusEvent`] to a sink, so instrumentation (watchpoints, decoders, the profiler) is an
//! event-stream *consumer* rather than a CPU special-case. Re-entrant cross-chip writes go through one
//! explicit deferred-write seam: such writes are queued and drained by [`SystemBus::apply_writes`]
//! after the access completes (jgenesis's `MainBusWrites` pattern, reimplemented).

use crate::state_hash::VRAM_SIZE;
use crate::system::RAM_SIZE;
use crate::vdp::Vdp;

/// 68000 work-RAM window base (`$FF0000`).
pub const RAM_BASE: u32 = 0xFF_0000;
/// Phase-0 synthetic VRAM window base. Lets the stub chip exercise the deferred-write seam through the
/// same `Bus` interface; the real VDP data-port semantics replace this when the VDP lands.
pub const VRAM_BASE: u32 = 0x10_0000;

/// Access width — byte, word, or long; the Genesis bus is big-endian (most-significant byte at the lowest
/// address). This is the SINGLE definition shared with the 68000 core: it is `m68000::microop::Size`
/// (bincode-serialized, used by every recipe + the `Bus68k` `Transaction` stream), re-exported here so the
/// generic bus layer and the CPU name the exact same type.
pub use crate::m68000::microop::Size;

/// What kind of bus access this is. `Tas` is the 68000's indivisible test-and-set read-modify-write —
/// ONE locked bus cycle, distinct from a separate `Read`+`Write` pair — so a consumer can tell the atomic
/// RMW apart from an ordinary access (it is also the access whose write the Mega Drive bus drops).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusOp {
    Read,
    Write,
    Tas,
}

/// One memory access, emitted per bus operation. `value` is the value read or (requested to be) written.
/// `fc` is the 68000 function code that drove the access (5 = supervisor data, 6 = supervisor program,
/// etc.); non-CPU masters (DMA, later chips) emit `fc = 0`, so instrumentation can attribute every access
/// to its master and space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BusEvent {
    pub op: BusOp,
    pub fc: u8,
    pub addr: u32,
    pub size: Size,
    pub value: u32,
}

/// A consumer of the bus event stream (watchpoints, recorders, decoders, the profiler...).
pub trait BusEventSink {
    fn on_event(&mut self, event: BusEvent);
}

/// Null sink — discards events (the hot path, with no instrumentation attached).
impl BusEventSink for () {
    fn on_event(&mut self, _event: BusEvent) {}
}

/// Recording sink — captures the full access stream (tests, tracing).
impl BusEventSink for Vec<BusEvent> {
    fn on_event(&mut self, event: BusEvent) {
        self.push(event);
    }
}

/// The typed bus protocol a chip uses to access the machine.
pub trait Bus {
    /// Read `size` bytes at `addr` (big-endian), emitting a read event.
    fn read(&mut self, addr: u32, size: Size) -> u32;
    /// Write `value` (`size` bytes, big-endian) at `addr`, emitting a write event. The write may be
    /// applied immediately or deferred depending on the target.
    fn write(&mut self, addr: u32, size: Size, value: u32);
}

/// Split-borrow adapter: borrows the `System`'s memory fields + an event sink for the duration of one
/// chip step, plus a private deferred-write queue.
pub struct SystemBus<'a, S: BusEventSink> {
    ram: &'a mut [u8],
    vram: &'a mut [u8],
    sink: &'a mut S,
    deferred: Vec<(u32, Size, u32)>,
}

impl<'a, S: BusEventSink> SystemBus<'a, S> {
    /// Build an adapter over the given memory regions and event sink.
    pub fn new(ram: &'a mut [u8], vram: &'a mut [u8], sink: &'a mut S) -> Self {
        Self {
            ram,
            vram,
            sink,
            deferred: Vec::new(),
        }
    }

    fn is_vram(addr: u32) -> bool {
        (VRAM_BASE..VRAM_BASE + VRAM_SIZE as u32).contains(&addr)
    }

    /// Resolve `addr` to its backing slice, the region base, and the index mask.
    fn slice_and_base(&mut self, addr: u32) -> (&mut [u8], u32, usize) {
        if Self::is_vram(addr) {
            (self.vram, VRAM_BASE, VRAM_SIZE - 1)
        } else {
            (self.ram, RAM_BASE, RAM_SIZE - 1)
        }
    }

    fn read_raw(&mut self, addr: u32, size: Size) -> u32 {
        let (buf, base, mask) = self.slice_and_base(addr);
        let n = size.bytes();
        let mut value = 0u32;
        for i in 0..n {
            let idx = (addr.wrapping_add(i).wrapping_sub(base) as usize) & mask;
            value = (value << 8) | buf[idx] as u32;
        }
        value
    }

    fn write_raw(&mut self, addr: u32, size: Size, value: u32) {
        let (buf, base, mask) = self.slice_and_base(addr);
        let n = size.bytes();
        for i in 0..n {
            let shift = 8 * (n - 1 - i);
            let byte = ((value >> shift) & 0xFF) as u8;
            let idx = (addr.wrapping_add(i).wrapping_sub(base) as usize) & mask;
            buf[idx] = byte;
        }
    }

    /// Drain the deferred-write queue into memory. Called once after each chip step.
    pub fn apply_writes(&mut self) {
        let pending = std::mem::take(&mut self.deferred);
        for (addr, size, value) in pending {
            self.write_raw(addr, size, value);
        }
    }
}

impl<'a, S: BusEventSink> Bus for SystemBus<'a, S> {
    fn read(&mut self, addr: u32, size: Size) -> u32 {
        let value = self.read_raw(addr, size);
        self.sink.on_event(BusEvent {
            op: BusOp::Read,
            fc: 0,
            addr,
            size,
            value,
        });
        value
    }

    fn write(&mut self, addr: u32, size: Size, value: u32) {
        self.sink.on_event(BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr,
            size,
            value,
        });
        if Self::is_vram(addr) {
            self.deferred.push((addr, size, value));
        } else {
            self.write_raw(addr, size, value);
        }
    }
}

// -----------------------------------------------------------------------------------------------------------
// MegaDriveBus — the CPU-facing `Bus68k` adapter over the real Mega Drive memory map.
// -----------------------------------------------------------------------------------------------------------

use crate::m68000::bus68k::{Bus68k, ADDR_MASK};

/// 8 KiB of Z80 RAM, visible to the 68000 at `$A00000` (mirrored across the 64 KiB `$A00000–$A0FFFF` window).
pub const Z80_RAM_SIZE: usize = 0x2000;

/// The byte returned from the version register at `$A10001` — a fixed placeholder (export/NTSC/no-expansion,
/// hardware version 0). Real region/timing + controller detection lands with the pads in a later phase.
pub const MD_VERSION: u8 = 0xA0;

/// Split-borrow adapter implementing the CPU-facing [`Bus68k`] over the `System`'s memory fields laid out per
/// the real Mega Drive map, emitting a [`BusEvent`] (with the real function code) per access. The CPU core
/// cannot tell it apart from the SST harness's `FlatBus` — the point of the unification. Every 24-bit address
/// has a deterministic answer; open bus returns the last word driven on the bus (`last_bus_word`). Writes
/// apply immediately in this pivot (no other master is live); the deferred-write seam plugs in with the VDP.
///
/// | Range | Behavior |
/// |---|---|
/// | `$000000–$3FFFFF` | ROM (read-only; past a short ROM's end → open bus) |
/// | `$400000–$7FFFFF` | open bus |
/// | `$A00000–$A0FFFF` | 8 KiB Z80 RAM (mirrored) |
/// | `$A10000–$A1001F` | I/O: `$A10001` = [`MD_VERSION`]; else 0 |
/// | `$A11100`/`$A11200` | Z80 BUSREQ/RESET: reads report bus granted (0); writes accepted |
/// | `$C00000`/`$C00002` | VDP data port (read = pre-cache buffer, write = VRAM/CRAM/VSRAM; recon R1) |
/// | `$C00004`/`$C00006` | VDP control port (read = status word, write = command; recon R1/R2) |
/// | `$C00008–$C0000F` | VDP HV counter (even byte = V, odd byte = H; recon R2) |
/// | `$E00000–$FFFFFF` | 64 KiB work RAM (mirrored) |
///
/// The VDP is borrowed here (`&mut Vdp`) alongside the master-clock reading (`now_mclk`) so the timing FSM
/// (h/v counters, status bits) reads live off the clock at access time.
pub struct MegaDriveBus<'a, S: BusEventSink> {
    rom: &'a [u8],
    ram: &'a mut [u8],
    z80_ram: &'a mut [u8],
    vdp: &'a mut Vdp,
    now_mclk: u64,
    last_bus_word: &'a mut u16,
    sink: &'a mut S,
}

impl<'a, S: BusEventSink> MegaDriveBus<'a, S> {
    /// Build an adapter over the given memory regions, the VDP + the master-clock reading, the open-bus
    /// latch, and an event sink.
    pub fn new(
        rom: &'a [u8],
        ram: &'a mut [u8],
        z80_ram: &'a mut [u8],
        vdp: &'a mut Vdp,
        now_mclk: u64,
        last_bus_word: &'a mut u16,
        sink: &'a mut S,
    ) -> Self {
        Self {
            rom,
            ram,
            z80_ram,
            vdp,
            now_mclk,
            last_bus_word,
            sink,
        }
    }

    /// The byte backing a mapped address, or `None` for open bus (unmapped ranges, past a short ROM's end,
    /// the VDP data port). Real memory (ROM / work RAM / Z80 RAM) and the fixed-constant registers return
    /// `Some`; the caller substitutes the open-bus latch for `None`.
    fn mapped_byte(&self, a: u32) -> Option<u8> {
        match a {
            // ROM: bytes present up to the ROM's length; past the end is open bus (no mirroring assumed).
            0x00_0000..=0x3F_FFFF => {
                let i = a as usize;
                (i < self.rom.len()).then(|| self.rom[i])
            }
            // Z80 RAM: 8 KiB mirrored across the 64 KiB window.
            0xA0_0000..=0xA0_FFFF => Some(self.z80_ram[(a as usize) & (Z80_RAM_SIZE - 1)]),
            // I/O: version register at $A10001; the rest read 0 (real pads land later).
            0xA1_0000..=0xA1_001F => Some(if a == 0xA1_0001 { MD_VERSION } else { 0x00 }),
            // Z80 BUSREQ ($A11100) / RESET ($A11200): report bus granted / reset released (0) so boot proceeds.
            0xA1_1100..=0xA1_1101 | 0xA1_1200..=0xA1_1201 => Some(0x00),
            // VDP ports ($C00000–$C0000F): stateful, handled as whole accesses in read16/write16/read8/
            // write8 (a byte-wise decode here would double a port access's side effects). Open bus for any
            // fallthrough (e.g. a TAS against a port — never a real access).
            0xC0_0000..=0xC0_000F => None,
            // Work RAM: 64 KiB mirrored across $E00000–$FFFFFF.
            0xE0_0000..=0xFF_FFFF => Some(self.ram[(a as usize) & (RAM_SIZE - 1)]),
            // $400000–$7FFFFF and every gap: open bus.
            _ => None,
        }
    }

    /// Store `byte` if `a` lands in writable memory (work RAM / Z80 RAM). Writes to ROM, the I/O / Z80-control
    /// / VDP-port regions are accepted (they still drive the bus + emit an event) but not stored — the
    /// placeholder scope until those chips land.
    fn store_byte(&mut self, a: u32, byte: u8) {
        match a {
            0xA0_0000..=0xA0_FFFF => self.z80_ram[(a as usize) & (Z80_RAM_SIZE - 1)] = byte,
            0xE0_0000..=0xFF_FFFF => self.ram[(a as usize) & (RAM_SIZE - 1)] = byte,
            _ => {}
        }
    }

    fn emit(&mut self, op: BusOp, fc: u8, addr: u32, size: Size, value: u32) {
        self.sink.on_event(BusEvent {
            op,
            fc,
            addr,
            size,
            value,
        });
    }

    /// Whether `a` (already masked) is a VDP port ($C00000–$C0000F).
    fn is_vdp_port(a: u32) -> bool {
        (0xC0_0000..=0xC0_000F).contains(&a)
    }

    /// A whole-word VDP port read (recon R1/R2), with its side effects (toggle clear, autoincrement,
    /// pre-cache refill). `a` is even (word-aligned).
    fn vdp_read_word(&mut self, a: u32) -> u16 {
        match a {
            0xC0_0000 | 0xC0_0002 => {
                let open_bus = *self.last_bus_word;
                self.vdp.data_read(open_bus)
            }
            0xC0_0004 | 0xC0_0006 => self.vdp.control_read_status(self.now_mclk),
            0xC0_0008..=0xC0_000F => self.vdp.hv_counter_read(self.now_mclk),
            _ => *self.last_bus_word,
        }
    }

    /// A whole-word VDP port write (recon R1). `a` is even (word-aligned). Returns the CPU wait cycles the
    /// access costs (recon R3: a data-port write to a full FIFO stalls the 68k via /DTACK) — folded into the
    /// instruction cost through the `Bus68k` wait channel. Control-port writes never stall.
    fn vdp_write_word(&mut self, a: u32, value: u16) -> u32 {
        match a {
            0xC0_0000 | 0xC0_0002 => self.vdp.data_write_at(value, self.now_mclk),
            0xC0_0004 | 0xC0_0006 => {
                self.vdp.control_write(value, self.now_mclk);
                0
            }
            _ => 0, // HV counter port writes ($C00008–$C0000F) are accepted but not stored.
        }
    }
}

impl<'a, S: BusEventSink> Bus68k for MegaDriveBus<'a, S> {
    fn read16(&mut self, addr: u32, fc: u8) -> (u16, u32) {
        let a = addr & ADDR_MASK;
        if fc == 7 {
            // The 68k interrupt-acknowledge (/INTAK) cycle: decode the acknowledged level from the address
            // (`0xFFFFFFF1 | level << 1`) and clear the VDP's pending latch for exactly that level — the ONLY
            // thing that clears the latches (recon R12, docs/2026-07-16-vdp-recon.md). The CPU discards the
            // returned value (Mega Drive VPA → autovector). Zero CPU changes: the interrupt recipe already
            // drives this fc=7 read (microop.rs IntAck), so the deassert rides the existing hook.
            let level = ((a >> 1) & 0x07) as u8;
            self.vdp.acknowledge(level);
            let v = *self.last_bus_word;
            self.emit(BusOp::Read, fc, a, Size::Word, v as u32);
            return (v, 0);
        }
        if Self::is_vdp_port(a) {
            let value = self.vdp_read_word(a);
            *self.last_bus_word = value;
            self.emit(BusOp::Read, fc, a, Size::Word, value as u32);
            return (value, 0);
        }
        let value = if let Some(hi) = self.mapped_byte(a) {
            let lo = self
                .mapped_byte((a.wrapping_add(1)) & ADDR_MASK)
                .unwrap_or(0);
            let v = ((hi as u16) << 8) | lo as u16;
            *self.last_bus_word = v; // a real word crossed the bus
            v
        } else {
            *self.last_bus_word // open bus: the last word driven, unchanged
        };
        self.emit(BusOp::Read, fc, a, Size::Word, value as u32);
        (value, 0)
    }

    fn write16(&mut self, addr: u32, fc: u8, value: u16) -> u32 {
        let a = addr & ADDR_MASK;
        if Self::is_vdp_port(a) {
            let wait = self.vdp_write_word(a, value);
            *self.last_bus_word = value;
            self.emit(BusOp::Write, fc, a, Size::Word, value as u32);
            return wait;
        }
        self.store_byte(a, (value >> 8) as u8);
        self.store_byte((a.wrapping_add(1)) & ADDR_MASK, (value & 0xFF) as u8);
        *self.last_bus_word = value;
        self.emit(BusOp::Write, fc, a, Size::Word, value as u32);
        0
    }

    fn read8(&mut self, addr: u32, fc: u8) -> (u8, u32) {
        let a = addr & ADDR_MASK;
        if Self::is_vdp_port(a) {
            // A byte read of a 16-bit VDP port does the stateful word access on the even base and returns
            // the addressed half (even = upper byte, odd = lower).
            let word = self.vdp_read_word(a & !1);
            let value = if a & 1 == 0 {
                (word >> 8) as u8
            } else {
                (word & 0xFF) as u8
            };
            *self.last_bus_word = word;
            self.emit(BusOp::Read, fc, a, Size::Byte, value as u32);
            return (value, 0);
        }
        let value = if let Some(b) = self.mapped_byte(a) {
            *self.last_bus_word = (b as u16) * 0x0101; // byte driven on both halves (placeholder open-bus rule)
            b
        } else if a & 1 == 0 {
            (*self.last_bus_word >> 8) as u8 // even address → UDS half
        } else {
            (*self.last_bus_word & 0xFF) as u8 // odd address → LDS half
        };
        self.emit(BusOp::Read, fc, a, Size::Byte, value as u32);
        (value, 0)
    }

    fn write8(&mut self, addr: u32, fc: u8, value: u8) -> u32 {
        let a = addr & ADDR_MASK;
        if Self::is_vdp_port(a) {
            // A byte write to a 16-bit VDP port drives the byte on both halves (the common byte-write model);
            // the stateful word write runs on the even base.
            let word = (value as u16) * 0x0101;
            let wait = self.vdp_write_word(a & !1, word);
            *self.last_bus_word = word;
            self.emit(BusOp::Write, fc, a, Size::Byte, value as u32);
            return wait;
        }
        self.store_byte(a, value);
        *self.last_bus_word = (value as u16) * 0x0101;
        self.emit(BusOp::Write, fc, a, Size::Byte, value as u32);
        0
    }

    fn tas(&mut self, addr: u32, fc: u8) -> (u8, u32) {
        // The Mega Drive bus controller does NOT honor the RMW write cycle of TAS (the Gargoyles/Ex-Mutants
        // quirk): the read happens, the write is DROPPED. So we read `orig`, DO NOT store `orig | 0x80`, and
        // still emit the Tas event (its value = the byte the CPU drove for the dropped write). The CPU gets
        // `orig` back for its flags.
        let a = addr & ADDR_MASK;
        let orig = if let Some(b) = self.mapped_byte(a) {
            b
        } else {
            (*self.last_bus_word & 0xFF) as u8
        };
        let written = orig | 0x80;
        *self.last_bus_word = (written as u16) * 0x0101;
        self.emit(BusOp::Tas, fc, a, Size::Byte, written as u32);
        (orig, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_returns_ram_byte_and_emits_event() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        ram[5] = 0x7E;
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        let v = bus.read(RAM_BASE + 5, Size::Byte);
        drop(bus);
        assert_eq!(v, 0x7E);
        assert_eq!(
            sink,
            vec![BusEvent {
                op: BusOp::Read,
                fc: 0,
                addr: RAM_BASE + 5,
                size: Size::Byte,
                value: 0x7E,
            }]
        );
    }

    #[test]
    fn word_read_is_big_endian() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        ram[0] = 0x12;
        ram[1] = 0x34;
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        let v = bus.read(RAM_BASE, Size::Word);
        drop(bus);
        assert_eq!(v, 0x1234);
    }

    #[test]
    fn ram_write_is_immediate() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        bus.write(RAM_BASE + 3, Size::Byte, 0xAB);
        let readback = bus.read(RAM_BASE + 3, Size::Byte);
        drop(bus);
        assert_eq!(readback, 0xAB);
        assert_eq!(ram[3], 0xAB);
    }

    #[test]
    fn word_write_is_big_endian() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        bus.write(RAM_BASE + 10, Size::Word, 0xBEEF);
        drop(bus);
        assert_eq!(ram[10], 0xBE);
        assert_eq!(ram[11], 0xEF);
    }

    #[test]
    fn vram_write_is_deferred_until_apply() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        let before = bus.read(VRAM_BASE, Size::Byte);
        bus.write(VRAM_BASE, Size::Byte, 0xCD);
        let mid = bus.read(VRAM_BASE, Size::Byte);
        bus.apply_writes();
        let after = bus.read(VRAM_BASE, Size::Byte);
        drop(bus);
        assert_eq!((before, mid, after), (0, 0, 0xCD));
        assert_eq!(vram[0], 0xCD);
    }

    struct WriteWatch {
        target: u32,
        hits: u32,
    }
    impl BusEventSink for WriteWatch {
        fn on_event(&mut self, event: BusEvent) {
            if event.op == BusOp::Write && event.addr == self.target {
                self.hits += 1;
            }
        }
    }

    #[test]
    fn system_bus_events_carry_fc_zero_for_a_non_cpu_master() {
        // SystemBus is not a CPU master — it has no function code, so every event it emits reports fc = 0.
        // (The CPU-side MegaDriveBus adapter fills the real 68000 FC; DMA and later chips also emit 0.)
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        bus.read(RAM_BASE, Size::Byte);
        bus.write(RAM_BASE, Size::Byte, 1);
        drop(bus);
        assert!(sink.iter().all(|e| e.fc == 0), "SystemBus emits fc = 0");
        // BusOp::Tas is a distinct third kind (the indivisible RMW), not a Read or a Write.
        assert_ne!(BusOp::Tas, BusOp::Read);
        assert_ne!(BusOp::Tas, BusOp::Write);
    }

    #[test]
    fn instrumentation_is_an_event_consumer() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut watch = WriteWatch {
            target: RAM_BASE + 0x20,
            hits: 0,
        };
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut watch);
        bus.write(RAM_BASE + 0x20, Size::Byte, 1);
        bus.read(RAM_BASE + 0x20, Size::Byte);
        bus.write(RAM_BASE + 0x20, Size::Byte, 2);
        bus.write(RAM_BASE + 0x21, Size::Byte, 3);
        drop(bus);
        assert_eq!(watch.hits, 2);
    }

    // --- MegaDriveBus: the real memory map ---------------------------------------------------------------

    use crate::m68000::bus68k::Bus68k;

    /// Backing store for a MegaDriveBus under test: a ROM, 64 KiB work RAM, 8 KiB Z80 RAM, a VDP, the
    /// master-clock reading, the open-bus latch.
    struct MdMem {
        rom: Vec<u8>,
        ram: Vec<u8>,
        z80_ram: Vec<u8>,
        vdp: Vdp,
        now_mclk: u64,
        last_bus_word: u16,
    }
    impl MdMem {
        fn new(rom: Vec<u8>) -> Self {
            Self {
                rom,
                ram: vec![0u8; RAM_SIZE],
                z80_ram: vec![0u8; Z80_RAM_SIZE],
                vdp: Vdp::power_on(&mut crate::rng::SplitMix64::new(1)),
                now_mclk: 0,
                last_bus_word: 0,
            }
        }
        fn bus<'a>(&'a mut self, sink: &'a mut Vec<BusEvent>) -> MegaDriveBus<'a, Vec<BusEvent>> {
            MegaDriveBus::new(
                &self.rom,
                &mut self.ram,
                &mut self.z80_ram,
                &mut self.vdp,
                self.now_mclk,
                &mut self.last_bus_word,
                sink,
            )
        }
    }

    #[test]
    fn rom_read_returns_the_rom_byte() {
        let mut rom = vec![0u8; 0x1000];
        rom[0] = 0x12;
        rom[1] = 0x34;
        rom[0x10] = 0xAB;
        let mut mem = MdMem::new(rom);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read16(0x00_0000, 6).0, 0x1234, "word from ROM");
        assert_eq!(bus.read8(0x00_0010, 6).0, 0xAB, "byte from ROM");
    }

    #[test]
    fn rom_is_read_only_a_write_does_not_change_it() {
        let mut mem = MdMem::new(vec![0x11u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0x00_0000, 6, 0xFFFF);
        assert_eq!(
            bus.read16(0x00_0000, 6).0,
            0x1111,
            "ROM unchanged by a write"
        );
    }

    #[test]
    fn rom_past_the_end_is_open_bus() {
        // A short 4 KiB ROM: reads past its end return the open-bus latch, NOT a mirror of the ROM.
        let mut mem = MdMem::new(vec![0x11u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // Drive a known word onto the bus (a work-RAM write), then read past the ROM end.
        bus.write16(0xE0_0000, 5, 0xBEEF);
        assert_eq!(
            bus.read16(0x20_0000, 6).0,
            0xBEEF,
            "past-end ROM read returns the last bus word"
        );
    }

    #[test]
    fn unmapped_range_is_open_bus_and_does_not_change_the_latch() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0000, 5, 0xCAFE); // latch := 0xCAFE
        assert_eq!(
            bus.read16(0x50_0000, 6).0,
            0xCAFE,
            "unmapped read = last word"
        );
        // A second open-bus read still sees the same latch (an open-bus read does not drive a new word).
        assert_eq!(bus.read16(0x60_0000, 6).0, 0xCAFE);
    }

    #[test]
    fn work_ram_reads_writes_and_mirrors_across_the_window() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0000, 5, 0x1357);
        assert_eq!(bus.read16(0xE0_0000, 5).0, 0x1357, "written then read back");
        // The 64 KiB work RAM is mirrored across the whole $E00000–$FFFFFF window.
        assert_eq!(bus.read16(0xFF_0000, 5).0, 0x1357, "mirror at $FF0000");
        assert_eq!(bus.read16(0xF1_0000, 5).0, 0x1357, "mirror at $F10000");
    }

    #[test]
    fn z80_ram_reads_writes_and_mirrors_in_its_window() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write8(0xA0_0000, 5, 0x9A);
        assert_eq!(bus.read8(0xA0_0000, 5).0, 0x9A, "Z80 RAM byte round-trips");
        // 8 KiB Z80 RAM mirrored across the 64 KiB window.
        assert_eq!(bus.read8(0xA0_2000, 5).0, 0x9A, "mirror at +0x2000");
    }

    #[test]
    fn version_register_returns_the_fixed_constant() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read8(0xA1_0001, 5).0, MD_VERSION, "version register");
    }

    #[test]
    fn z80_busreq_reports_the_bus_granted() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // Writes are accepted (boot code releases/asserts the bus), reads report granted so boot proceeds.
        bus.write8(0xA1_1100, 5, 0x01);
        assert_eq!(bus.read8(0xA1_1100, 5).0, 0x00, "bus granted (bit0 = 0)");
    }

    #[test]
    fn vdp_status_read_returns_the_live_status_word() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        // Line 100 (active), H=0x40 (dot 1280) — not v/h blank, so only the FIFO-empty placeholder is set.
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 1280;
        let expected = mem.vdp.status_word(mem.now_mclk);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read16(0xC0_0004, 5).0, expected, "live VDP status word");
        assert_eq!(expected, 0x0200, "FIFO-empty only during active display");
    }

    #[test]
    fn vdp_data_port_write_then_readback_through_the_bus() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // Register write $8F02: reg 15 (autoincrement) = 2, so the address advances by a word per access.
        bus.write16(0xC0_0004, 5, 0x8F02);
        // Set up a VRAM write at address 0x0100 (code 0x01): word 1 = 01_00 0001 0000 0000 = 0x4100,
        // word 2 = 0x0000 (CD5-CD2 = 0, A15-A14 = 0).
        bus.write16(0xC0_0004, 5, 0x4100);
        bus.write16(0xC0_0004, 5, 0x0000);
        bus.write16(0xC0_0000, 5, 0xBEEF); // data write → VRAM[0x0100..0x0101], addr → 0x0102
        bus.write16(0xC0_0000, 5, 0xCAFE); // → VRAM[0x0102..0x0103]
                                           // Set up a VRAM read at 0x0100 (code 0x00): word 1 = 0x0100, word 2 = 0x0000.
        bus.write16(0xC0_0004, 5, 0x0100);
        bus.write16(0xC0_0004, 5, 0x0000);
        assert_eq!(
            bus.read16(0xC0_0000, 5).0,
            0xBEEF,
            "first VRAM word reads back"
        );
        assert_eq!(
            bus.read16(0xC0_0000, 5).0,
            0xCAFE,
            "second VRAM word reads back"
        );
    }

    #[test]
    fn vdp_hv_counter_read_returns_the_live_counter() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 50 * crate::vdp::MCLK_PER_LINE + 800; // a mid-frame instant
        let expected = mem.vdp.hv_counter_read(mem.now_mclk);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(
            bus.read16(0xC0_0008, 5).0,
            expected,
            "HV counter word ((V<<8)|H) at $C00008"
        );
        // The HV counter is mirrored across $C0000A/$C0000C/$C0000E.
        assert_eq!(
            bus.read16(0xC0_000A, 5).0,
            expected,
            "HV counter mirror at $C0000A"
        );
    }

    /// Drive display-on + autoinc-2 + a VRAM-write command @ `$0100` into the VDP through the control port.
    fn vdp_setup_vram_write(bus: &mut MegaDriveBus<'_, Vec<BusEvent>>) {
        bus.write16(0xC0_0004, 5, 0x8140); // reg 1 = display enable (bit 6)
        bus.write16(0xC0_0004, 5, 0x8F02); // reg 15 = autoinc 2
        bus.write16(0xC0_0004, 5, 0x4100); // VRAM write command, word 1 (@ $0100)
        bus.write16(0xC0_0004, 5, 0x0000); // word 2
    }

    #[test]
    fn fifo_full_write_returns_wait_cycles() {
        // Five rapid data-port writes at the SAME instant on an active-display line (recon R3: 16 slots/line,
        // a VRAM word exits in 2 slots): the first four fill the 4-entry FIFO, the fifth stalls the 68k.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 500; // active line, mid-line
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        vdp_setup_vram_write(&mut bus);
        for _ in 0..4 {
            assert_eq!(
                bus.write16(0xC0_0000, 5, 0xBEEF),
                0,
                "the first four writes fill the FIFO without stalling"
            );
        }
        let wait = bus.write16(0xC0_0000, 5, 0xBEEF);
        assert!(
            wait > 0,
            "the fifth write to a full FIFO stalls the 68k (recon R3)"
        );
    }

    #[test]
    fn fifo_writes_spaced_past_a_slot_do_not_stall() {
        // Writes spaced well past the active VRAM slot cost (~427 mclk) drain between writes — the FIFO never
        // fills, so no write stalls.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 0;
        {
            let mut sink = Vec::new();
            let mut bus = mem.bus(&mut sink);
            vdp_setup_vram_write(&mut bus);
        }
        for i in 0..8u64 {
            mem.now_mclk = i * 1000; // active lines (0..2), 1000 mclk apart
            let mut sink = Vec::new();
            let mut bus = mem.bus(&mut sink);
            assert_eq!(
                bus.write16(0xC0_0000, 5, 0xBEEF),
                0,
                "a spaced write drains the FIFO first, so it never stalls"
            );
        }
    }

    #[test]
    fn flatbus_vdp_address_write_yields_no_wait() {
        // The SST harness bus (FlatBus) has no VDP: a write to the VDP data-port address is plain memory and
        // returns 0 wait forever — the invariant that keeps the SST corpus bit-identical.
        use crate::m68000::bus68k::FlatBus;
        let mut bus = FlatBus::new();
        assert_eq!(
            bus.write16(0xC0_0000, 5, 0xBEEF),
            0,
            "FlatBus word write: no wait"
        );
        assert_eq!(
            bus.write8(0xC0_0000, 5, 0xBE),
            0,
            "FlatBus byte write: no wait"
        );
    }

    #[test]
    fn open_bus_read_returns_the_last_word_driven_by_a_write() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0010, 5, 0xF00D); // drives 0xF00D onto the bus
        assert_eq!(
            bus.read16(0x40_0000, 6).0,
            0xF00D,
            "open bus = last driven word"
        );
    }

    #[test]
    fn megadrive_tas_drops_the_write_but_flags_from_the_read() {
        // The Gargoyles/Ex-Mutants quirk: on the Mega Drive the RMW WRITE cycle of TAS is dropped — the read
        // happens, the write does not. So the byte in RAM is UNCHANGED (contrast FlatBus, which stores
        // orig|0x80), while the CPU still gets `orig` back for its flags.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write8(0xE0_0100, 5, 0x35);
        let (orig, _wait) = bus.tas(0xE0_0100, 5);
        assert_eq!(orig, 0x35, "TAS returns the pre-modify byte for the flags");
        assert_eq!(
            bus.read8(0xE0_0100, 5).0,
            0x35,
            "the Mega Drive drops the TAS write — RAM is UNCHANGED"
        );
        assert!(
            sink.iter().any(|e| e.op == BusOp::Tas),
            "the Tas access is still logged"
        );
    }
}
