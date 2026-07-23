//! The `Z80Bus` split-borrow adapter — the Genesis Z80 address view (ZC12).
//!
//! The analog of [`crate::bus::MegaDriveBus`] for the sound CPU: it borrows the `System`'s memory fields
//! for the duration of one Z80 step and presents the Z80's own 16-bit address space. This is the
//! **Z-live** shape — Z80 RAM, the `$6000` serial bank latch, and the `$8000-$FFFF` 68k bank window
//! (reaching ROM / work RAM / Z80 RAM) are **live**, so a released Z80 can fetch its driver code from RAM
//! and read music/sample data from ROM through the window. The FM/PSG ports decode (read = not-busy / open
//! bus, write = drop) — turning those writes into the `BusEvent` VGM tap is **Phase RT**. VDP-through-window
//! and I/O-through-window are the named deferrals (a sound driver rarely reaches them; routing them needs
//! the `Vdp`/`Io` borrows and is pinned for the RT/interrupt slices).
//!
//! Genesis Z80 memory map (Plutiedev "Using the Z80"):
//!
//! | Z80 address | Target | This slice |
//! |---|---|---|
//! | `$0000-$1FFF` | Z80 RAM (8 KiB) | **live** — the shared `z80_ram` buffer |
//! | `$2000-$3FFF` | Z80 RAM mirror | **live** — mirrored (`& 0x1FFF`) |
//! | `$4000-$4003` | YM2612 FM address/data | read = not-busy status; write dropped (Phase RT tap) |
//! | `$6000` | bank-address register | **live** — 9-bit LSB-first serial latch |
//! | `$7F11` | PSG (SN76489), write-only | decode: read open bus, write dropped (Phase RT tap) |
//! | `$7F00-$7F1F` | VDP port mirror | deferred: open bus / drop (needs the `Vdp` borrow) |
//! | `$8000-$FFFF` | 68k bank window | **live** — `(bank << 15) \| (addr & 0x7FFF)` → ROM / work RAM / Z80 RAM |

use super::Z80Io;
use crate::bus::{BusEvent, BusEventSink, BusOp, Size, Z80_RAM_SIZE};
use crate::system::RAM_SIZE;
use crate::ym2612::Ym2612;

/// Split-borrow adapter over the `System`'s Z80-visible memory for one Z80 step. Holds the Z80 RAM, the
/// cartridge ROM + work RAM the bank window reaches, and the serial bank latch. No `Rc`/`RefCell`/`unsafe` —
/// each field is one `&`/`&mut`, borrowed disjointly from the `System` for the step's duration. Generic over
/// the event `sink` (the same instrumentation channel the 68k-side `MegaDriveBus` feeds): FM/PSG register
/// writes tap into it as `BusEvent`s (the VGM logger's source), and the null sink (`()`) stays a no-op.
pub struct Z80Bus<'a, S: BusEventSink> {
    z80_ram: &'a mut [u8],
    rom: &'a [u8],
    ram: &'a mut [u8],
    /// The 9-bit bank register (`$6000`), serial-loaded LSB-first; selects the 32 KiB 68k page the
    /// `$8000-$FFFF` window maps to. Borrowed mutably so a `$6000` write persists into the `System`.
    bank: &'a mut u16,
    /// The YM2612 FM chip (its timers): a `$4000-$4003` read returns its live status byte (the driver clocks
    /// its sequencer off Timer-A overflow, bit0), and a write drives the address-latch/data protocol into its
    /// timer model **in addition to** the RT-1 VGM tap below. Split-borrowed like the 68k side; read/written at
    /// the Z80's own frontier time (`now_mclk`). See `docs/2026-07-22-fm-timer-design.md`.
    fm: &'a mut Ym2612,
    /// The Z80's current mclk (its frontier — the value at the start of this step), the absolute time the FM
    /// status/timer is anchored to. The Z80 reads the chip *behind* the 68000's `now`; both are absolute on the
    /// one shared timeline, so the flag is computed at this time (FM7).
    now_mclk: u64,
    /// The bus-event sink FM/PSG writes tap into (Phase RT). Threaded down from the run loop, monomorphized —
    /// `()` is the no-op hot path, `Vec<BusEvent>` records.
    sink: &'a mut S,
}

impl<'a, S: BusEventSink> Z80Bus<'a, S> {
    /// Build an adapter over the Z80 RAM, the cartridge ROM + work RAM the bank window reaches, the serial
    /// bank latch, the FM chip + the Z80's current mclk, and the event sink.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        z80_ram: &'a mut [u8],
        rom: &'a [u8],
        ram: &'a mut [u8],
        bank: &'a mut u16,
        fm: &'a mut Ym2612,
        now_mclk: u64,
        sink: &'a mut S,
    ) -> Self {
        Self {
            z80_ram,
            rom,
            ram,
            bank,
            fm,
            now_mclk,
            sink,
        }
    }

    /// Translate a `$8000-$FFFF` window address to its absolute 68000 address: the 9-bit bank selects the
    /// 32 KiB page, `addr & 0x7FFF` is the offset within it (Plutiedev "Z80 banking").
    fn window_addr(&self, addr: u16) -> u32 {
        ((*self.bank as u32) << 15) | (addr as u32 & 0x7FFF)
    }

    /// Read one byte of 68000 space through the bank window. ROM / work RAM / Z80 RAM are live; every other
    /// 68k region (VDP ports, I/O, FM, the Z80-arbitration registers) reads open bus (`$FF`) this slice —
    /// a sound driver reaches them through the window only in rare cases, deferred with the `Vdp`/`Io`
    /// borrows to the RT/interrupt slices.
    fn read_window(&self, a68k: u32) -> u8 {
        match a68k {
            // Cartridge ROM ($000000-$3FFFFF); past a short ROM's end is open bus.
            0x00_0000..=0x3F_FFFF => {
                let i = a68k as usize;
                if i < self.rom.len() {
                    self.rom[i]
                } else {
                    0xFF
                }
            }
            // Z80 RAM aliased at $A00000 (8 KiB mirrored across its 64 KiB window).
            0xA0_0000..=0xA0_FFFF => self.z80_ram[(a68k as usize) & (Z80_RAM_SIZE - 1)],
            // Work RAM ($E00000-$FFFFFF, 64 KiB mirrored).
            0xE0_0000..=0xFF_FFFF => self.ram[(a68k as usize) & (RAM_SIZE - 1)],
            // VDP / I/O / FM / Z80-arbitration through the window: deferred → open bus.
            _ => 0xFF,
        }
    }

    /// Write one byte of 68000 space through the bank window. Only writable memory (work RAM / Z80 RAM)
    /// stores; ROM and the port/register regions drop (the same placeholder scope as the 68k side).
    fn write_window(&mut self, a68k: u32, value: u8) {
        match a68k {
            0xA0_0000..=0xA0_FFFF => self.z80_ram[(a68k as usize) & (Z80_RAM_SIZE - 1)] = value,
            0xE0_0000..=0xFF_FFFF => self.ram[(a68k as usize) & (RAM_SIZE - 1)] = value,
            // ROM and every port/register region through the window: dropped this slice.
            _ => {}
        }
    }
}

impl<S: BusEventSink> Z80Io for Z80Bus<'_, S> {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            // Z80 RAM (8 KiB), mirrored across $0000-$3FFF.
            0x0000..=0x3FFF => self.z80_ram[(addr as usize) & (Z80_RAM_SIZE - 1)],
            // YM2612 FM: read = the live status byte (Timer-A overflow bit0, Timer-B overflow bit1, bit7 BUSY
            // clear). The SMPS driver clocks its sequencer off Timer-A overflow, so this must answer truthfully
            // (docs/2026-07-22-fm-timer-design.md). Anchored to the Z80's own frontier time.
            0x4000..=0x4003 => self.fm.read_status(self.now_mclk),
            // 68k bank window: translate through the 9-bit bank and read 68000 space.
            0x8000..=0xFFFF => {
                let a = self.window_addr(addr);
                self.read_window(a)
            }
            // Bank register ($6000, write-only), PSG ($7F11, write-only), VDP mirror ($7F00-$7F1F): open bus.
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Z80 RAM (8 KiB), mirrored across $0000-$3FFF.
            0x0000..=0x3FFF => self.z80_ram[(addr as usize) & (Z80_RAM_SIZE - 1)] = value,
            // Bank register ($6000): serial load, LSB-first — each write shifts bit0 of the byte into the top
            // of the 9-bit latch (Plutiedev "Z80 banking"). After 9 writes the full page is selected.
            0x6000..=0x60FF => *self.bank = (*self.bank >> 1) | (((value as u16) & 1) << 8),
            // 68k bank window: translate through the 9-bit bank and write 68000 space.
            0x8000..=0xFFFF => {
                let a = self.window_addr(addr);
                self.write_window(a, value);
            }
            // YM2612 FM ($4000-$4003) / SN76489 PSG ($7F11): tap the register write into the bus-event stream
            // (Phase RT — the VGM logger consumes it). `fc = 0` because the Z80 is a non-68000 master (the
            // DMA/other-master convention in crate::bus). The RAW Z80-side address ($4000 / $7F11) is emitted,
            // NOT the 68k FM window ($A04000): a consumer unifies the two at the register-file level. FM writes
            // ADDITIONALLY drive the timer model (the tap is for the VGM logger; the timer update is what makes
            // the driver's Timer-A overflow poll fire — docs/2026-07-22-fm-timer-design.md). PSG has no timer.
            0x4000..=0x4003 | 0x7F11 => {
                self.sink.on_event_at(
                    BusEvent {
                        op: BusOp::Write,
                        fc: 0,
                        addr: addr as u32,
                        size: Size::Byte,
                        value: value as u32,
                    },
                    self.now_mclk,
                );
                if let 0x4000..=0x4003 = addr {
                    self.fm.write_port(addr, value, self.now_mclk);
                }
            }
            // VDP mirror ($7F00-$7F1F, excluding the PSG at $7F11) drops (deferred).
            _ => {}
        }
    }

    fn input(&mut self, _port: u16) -> u8 {
        // The Genesis Z80 does not use the I/O-port space (Plutiedev "Using the Z80"): `IN` reads open bus.
        0xFF
    }

    fn output(&mut self, _port: u16, _value: u8) {
        // The Genesis Z80's I/O-port space is unused; `OUT` writes are dropped.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a bus over fresh buffers with an explicit bank value and a null sink (helper for the map tests
    /// that do not care about the event stream). A fresh unprogrammed FM chip at mclk 0 (status reads 0x00).
    fn bus_with<'a>(
        ram: &'a mut [u8],
        rom: &'a [u8],
        work: &'a mut [u8],
        bank: &'a mut u16,
        fm: &'a mut Ym2612,
        sink: &'a mut (),
    ) -> Z80Bus<'a, ()> {
        Z80Bus::new(ram, rom, work, bank, fm, 0, sink)
    }

    #[test]
    fn z80_ram_reads_writes_and_mirrors() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut bus = bus_with(&mut ram, &rom, &mut work, &mut bank, &mut fm, &mut sink);
        bus.write(0x0001, 0x9A);
        assert_eq!(bus.read(0x0001), 0x9A, "Z80 RAM byte round-trips");
        // 8 KiB RAM mirrored across $2000-$3FFF.
        assert_eq!(bus.read(0x2001), 0x9A, "mirror at +0x2000");
        bus.write(0x3FFF, 0x5C);
        assert_eq!(bus.read(0x1FFF), 0x5C, "top-of-RAM mirror round-trips");
    }

    #[test]
    fn fm_ports_read_not_busy() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut bus = bus_with(&mut ram, &rom, &mut work, &mut bank, &mut fm, &mut sink);
        for a in [0x4000u16, 0x4001, 0x4002, 0x4003] {
            assert_eq!(
                bus.read(a) & 0x80,
                0,
                "FM status bit7 (BUSY) clear at {a:#06X}"
            );
        }
    }

    #[test]
    fn bank_register_serial_loads_lsb_first() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut bus = bus_with(&mut ram, &rom, &mut work, &mut bank, &mut fm, &mut sink);
        // Load the 9-bit page value 0b1_0000_0001 = 0x101 LSB-first: bit0 first ... bit8 last.
        for bit in [1u8, 0, 0, 0, 0, 0, 0, 0, 1] {
            bus.write(0x6000, bit);
        }
        assert_eq!(bank, 0x101, "9 LSB-first writes select the page");
    }

    #[test]
    fn bank_window_reads_rom() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        // ROM byte at 68k $008000 (page 1, offset 0).
        let mut rom = vec![0u8; 0x1_0000];
        rom[0x8000] = 0x7E;
        let mut work = vec![0u8; RAM_SIZE];
        // bank = 1 → window base = 1 << 15 = $8000.
        let mut bank = 1u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut bus = bus_with(&mut ram, &rom, &mut work, &mut bank, &mut fm, &mut sink);
        assert_eq!(
            bus.read(0x8000),
            0x7E,
            "window reads ROM at (bank<<15)|offset"
        );
    }

    #[test]
    fn bank_window_reads_and_writes_work_ram() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        // 68k work RAM $FF0000 = bank 0x1FE (0x1FE << 15 = $FF0000), window offset 0.
        let mut bank = 0x1FEu16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut bus = bus_with(&mut ram, &rom, &mut work, &mut bank, &mut fm, &mut sink);
        bus.write(0x8000, 0x42);
        assert_eq!(bus.read(0x8000), 0x42, "window round-trips 68k work RAM");
        assert_eq!(
            work[0], 0x42,
            "the write landed in the shared work RAM buffer"
        );
    }

    #[test]
    fn fm_and_psg_writes_tap_into_the_event_sink() {
        // The Phase RT tap: an FM ($4000-$4003) or PSG ($7F11) register write emits a Write BusEvent (fc = 0,
        // byte-sized, the RAW Z80-side address + the byte the Z80 drove) into the sink; the value is otherwise
        // dropped (no synthesis this slice). Z80-RAM and bank-register writes emit NOTHING.
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut fm = Ym2612::new();
        {
            let mut bus = Z80Bus::new(&mut ram, &rom, &mut work, &mut bank, &mut fm, 0, &mut sink);
            // FM address/data ports + the PSG port, each with a distinct value.
            bus.write(0x4000, 0x22);
            bus.write(0x4001, 0x33);
            bus.write(0x4002, 0x44);
            bus.write(0x4003, 0x55);
            bus.write(0x7F11, 0x9F);
            // A Z80-RAM write and a bank-register write must NOT tap.
            bus.write(0x0001, 0xAB);
            bus.write(0x6000, 0x01);
        }
        let expected = [
            (0x4000u32, 0x22u32),
            (0x4001, 0x33),
            (0x4002, 0x44),
            (0x4003, 0x55),
            (0x7F11, 0x9F),
        ];
        assert_eq!(
            sink.len(),
            expected.len(),
            "only the 5 FM/PSG writes tap — RAM and bank writes emit nothing"
        );
        for (event, (addr, value)) in sink.iter().zip(expected) {
            assert_eq!(event.op, BusOp::Write, "FM/PSG tap is a Write");
            assert_eq!(event.fc, 0, "the Z80 is a non-68000 master (fc = 0)");
            assert_eq!(
                event.size,
                Size::Byte,
                "FM/PSG register writes are byte-sized"
            );
            assert_eq!(event.addr, addr, "raw Z80-side address");
            assert_eq!(event.value, value, "the byte the Z80 drove");
        }
    }
}
