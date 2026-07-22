//! The `Z80Bus` split-borrow adapter — the Genesis Z80 address view (ZC12).
//!
//! The analog of [`crate::bus::MegaDriveBus`] for the sound CPU: it borrows the `System`'s memory fields
//! for the duration of one Z80 step and presents the Z80's own 16-bit address space. This is the
//! **Z-skeleton** shape — the address decode is defined in full, but only Z80 RAM (`$0000-$1FFF` + its
//! mirror) is live; the ports and the 68k bank window are **scaffolded stubs** (documented per arm), because
//! nothing executes yet and their real handlers need chips/latches that land in later slices. When the
//! Z-execute slice turns [`super::Z80::step`] on, these arms fill in against the shared 68k fields; when
//! Phase RT lands, the FM/PSG writes become the `BusEvent` VGM tap.
//!
//! Genesis Z80 memory map (Plutiedev "Using the Z80"):
//!
//! | Z80 address | Target | This slice |
//! |---|---|---|
//! | `$0000-$1FFF` | Z80 RAM (8 KiB) | **live** — the shared `z80_ram` buffer |
//! | `$2000-$3FFF` | Z80 RAM mirror | **live** — mirrored (`& 0x1FFF`) |
//! | `$4000-$4003` | YM2612 FM address/data | stub: read = not-busy status, write dropped (Phase RT tap) |
//! | `$6000` | bank-address register | stub: 9-bit serial bank latch (Z-execute) |
//! | `$7F11` | PSG (SN76489), write-only | stub (Phase RT tap) |
//! | `$7F00-$7F1F` | VDP port mirror | stub (Z-execute) |
//! | `$8000-$FFFF` | 68k bank window | stub: `(bank << 15) \| (addr & 0x7FFF)` into 68k space (Z-execute) |

use super::Z80Io;
use crate::bus::Z80_RAM_SIZE;

/// Split-borrow adapter over the `System`'s Z80-visible memory for one Z80 step. Holds only the Z80 RAM
/// this slice; the ports + 68k bank window are decoded but stubbed (they gain their borrows/latches when
/// [`super::Z80::step`] executes in the Z-execute slice). No `Rc`/`RefCell`/`unsafe` — one `&mut` at a time.
pub struct Z80Bus<'a> {
    z80_ram: &'a mut [u8],
}

impl<'a> Z80Bus<'a> {
    /// Build an adapter over the shared 8 KiB Z80 RAM.
    pub fn new(z80_ram: &'a mut [u8]) -> Self {
        Self { z80_ram }
    }
}

impl Z80Io for Z80Bus<'_> {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            // Z80 RAM (8 KiB), mirrored across $0000-$3FFF.
            0x0000..=0x3FFF => self.z80_ram[(addr as usize) & (Z80_RAM_SIZE - 1)],
            // YM2612 FM: read = status with bit7 (BUSY) clear = not busy (reuses the 68k-side model,
            // crate::bus F2/F4). The real chip lands with the FM core.
            0x4000..=0x4003 => 0x00,
            // Bank register ($6000), PSG ($7F11, write-only), VDP mirror ($7F00-$7F1F), and the 68k bank
            // window ($8000-$FFFF) are not reachable until Z-execute turns on execution; open-bus stub.
            _ => 0x00,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        // Z80 RAM (8 KiB), mirrored across $0000-$3FFF, is the only live target this slice. FM
        // ($4000-$4003) / PSG ($7F11) writes drop — the BusEvent VGM tap is Phase RT. The bank register
        // ($6000) 9-bit serial latch and the 68k bank window ($8000-$FFFF) routing land in Z-execute.
        if let 0x0000..=0x3FFF = addr {
            self.z80_ram[(addr as usize) & (Z80_RAM_SIZE - 1)] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z80_ram_reads_writes_and_mirrors() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let mut bus = Z80Bus::new(&mut ram);
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
        let mut bus = Z80Bus::new(&mut ram);
        for a in [0x4000u16, 0x4001, 0x4002, 0x4003] {
            assert_eq!(
                bus.read(a) & 0x80,
                0,
                "FM status bit7 (BUSY) clear at {a:#06X}"
            );
        }
    }
}
