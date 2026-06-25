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

/// 68000 work-RAM window base (`$FF0000`).
pub const RAM_BASE: u32 = 0xFF_0000;
/// Phase-0 synthetic VRAM window base. Lets the stub chip exercise the deferred-write seam through the
/// same `Bus` interface; the real VDP data-port semantics replace this when the VDP lands.
pub const VRAM_BASE: u32 = 0x10_0000;

/// Access width. The Genesis bus is big-endian: the most-significant byte is at the lowest address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Byte,
    Word,
    Long,
}

impl Size {
    /// Number of bytes this access width touches.
    pub fn bytes(self) -> u32 {
        match self {
            Size::Byte => 1,
            Size::Word => 2,
            Size::Long => 4,
        }
    }
}

/// Whether a bus access reads or writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusOp {
    Read,
    Write,
}

/// One memory access, emitted per `Bus` operation. `value` is the value read or (requested to be) written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BusEvent {
    pub op: BusOp,
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
            addr,
            size,
            value,
        });
        value
    }

    fn write(&mut self, addr: u32, size: Size, value: u32) {
        self.sink.on_event(BusEvent {
            op: BusOp::Write,
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
}
