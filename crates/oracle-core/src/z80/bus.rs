//! The `Z80Bus` split-borrow adapter — the Genesis Z80 address view (ZC12).
//!
//! The analog of [`crate::bus::MegaDriveBus`] for the sound CPU: it borrows the `System`'s memory fields
//! for the duration of one Z80 step and presents the Z80's own 16-bit address space. This is the
//! **Z-live** shape — Z80 RAM, the `$6000` serial bank latch, and the `$8000-$FFFF` 68k bank window
//! (reaching ROM / work RAM / Z80 RAM) are **live**, so a released Z80 can fetch its driver code from RAM
//! and read music/sample data from ROM through the window. The FM/PSG ports decode (read = not-busy / open
//! bus, write = drop) — turning those writes into the `BusEvent` VGM tap is **Phase RT**. The `$7F04-$7F0F`
//! VDP status/HV mirror is **live** (K2 fix — routed to the real `Vdp`); VDP/I/O-through-the-bank-window
//! remain the named deferrals (a sound driver rarely reaches them; routing them needs the `Io` borrow and
//! is pinned for a later slice).
//!
//! Genesis Z80 memory map (Plutiedev "Using the Z80"):
//!
//! | Z80 address | Target | This slice |
//! |---|---|---|
//! | `$0000-$1FFF` | Z80 RAM (8 KiB) | **live** — the shared `z80_ram` buffer |
//! | `$2000-$3FFF` | Z80 RAM mirror | **live** — mirrored (`& 0x1FFF`) |
//! | `$4000-$4003` | YM2612 FM address/data | read = not-busy status; write dropped (Phase RT tap). **Known asymmetry (deferred):** the Z80-side decode deliberately stays `$4000-$4003` — `$4004-$5FFF` falls through to `$FF`/drop below — while the 68k-side window (K4-6) answers FM across the chip's full `$4000-$5FFF` select span (memtest-pinned there). The Z80-side span is unpinned and has zero corpus evidence (no driver touches `$4004+`), so widening it would move sound-currency surface for no gain; ledgered in `docs/2026-07-25-testrom-conformance.md` (K4-6) |
//! | `$6000` | bank-address register | **live** — 9-bit LSB-first serial latch |
//! | `$7F11` | PSG (SN76489), write-only | decode: read open bus, write dropped (Phase RT tap) |
//! | `$7F00-$7F03` | VDP data port mirror | read = open bus `$FF` (hardware LOCKS UP — ledgered `vdp-dataport-read-lockup`); write dropped |
//! | `$7F04-$7F07` | VDP control port mirror | **live** read — real status read of the `Vdp` (clears the write-toggle, K2); write dropped |
//! | `$7F08-$7F0F` | VDP HV counter mirror | **live** read — the live HV counter (side-effect-free); write dropped |
//! | `$7F10-$7F1F` | rest of the VDP mirror | read open bus, write dropped |
//! | `$8000-$FFFF` | 68k bank window | **live** — `(bank << 15) \| (addr & 0x7FFF)` → ROM / work RAM / Z80 RAM |

use super::Z80Io;
use crate::bus::{BusEvent, BusEventSink, BusOp, Size, Z80_RAM_SIZE};
use crate::system::RAM_SIZE;
use crate::vdp::Vdp;
use crate::ym2612::Ym2612;

/// One serial tick of the 9-bit `$6000` bank latch: shift right, load bit0 of the written byte into the
/// top (LSB-first, Plutiedev "Z80 banking"). **The single source of truth for BOTH paths to the register**:
/// the Z80's own `$6000-$60FF` write and the 68000's window write at the same Z80 offset (`$A06000+`
/// masked to 15 bits) tick the SAME latch — hardware has one register (Oracle `MDBusArbiter.cpp`
/// `Z80WindowBankswitch`, reached from both buses).
pub(crate) fn bank_latch_tick(bank: &mut u16, value: u8) {
    *bank = (*bank >> 1) | (((value as u16) & 1) << 8);
}

/// One byte read of the Z80-side `$7F00-$7FFF` VDP-port mirror (K2) — **the single source of truth for
/// BOTH paths**: the Z80's own read and the 68000's window read at the same 15-bit offset route here.
///
/// - `$7F04-$7F07`: a REAL control-port status read — same side effects as a 68k `$C00004` read (clears
///   the control-port write-toggle, the pinned recon-vdp semantic), same byte-lane split (even = status
///   high byte, odd = low). `open_bus = 0`: the Z80-side data bus keeps its own (unmodeled) residue, so
///   the floating upper 6 bits read 0 — the K2 pin, byte-identical from either bus (K4-5 note).
/// - `$7F08-$7F0F`: the live HV counter, side-effect-free (even = V, odd = H).
/// - `$7F00-$7F03` (data port): `$FF` — a real read locks up the machine (the ledgered
///   `vdp-dataport-read-lockup` known-difference); we return open bus instead of modeling the hang.
/// - `$7F10-$7FFF`: write-only / unused on hardware — open bus `$FF`.
pub(crate) fn vdp_mirror_read(vdp: &mut Vdp, zaddr: u16, now_mclk: u64) -> u8 {
    match zaddr {
        0x7F04..=0x7F07 => {
            let s = vdp.control_read_status(0, now_mclk);
            if zaddr & 1 == 0 {
                (s >> 8) as u8
            } else {
                (s & 0xFF) as u8
            }
        }
        0x7F08..=0x7F0F => {
            let hv = vdp.hv_counter_read(now_mclk);
            if zaddr & 1 == 0 {
                (hv >> 8) as u8
            } else {
                (hv & 0xFF) as u8
            }
        }
        _ => 0xFF,
    }
}

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
    /// The VDP, for the `$7F00-$7F1F` port mirror (K2): a `$7F04-$7F07` read is a real control-port status
    /// read — same side effects as the 68k's `$C00004` read (clears the control-port write-toggle), same
    /// byte-lane split (even = status high byte, odd = low). `$7F08-$7F0F` reads the HV counter
    /// (side-effect-free). Split-borrowed like `fm`; read at the Z80's own frontier time (`now_mclk`) —
    /// the same instant a 68k-side access at this moment would use.
    vdp: &'a mut Vdp,
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
    /// bank latch, the FM chip, the VDP (for the `$7F00-$7F1F` port mirror), the Z80's current mclk, and the
    /// event sink.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        z80_ram: &'a mut [u8],
        rom: &'a [u8],
        ram: &'a mut [u8],
        bank: &'a mut u16,
        fm: &'a mut Ym2612,
        vdp: &'a mut Vdp,
        now_mclk: u64,
        sink: &'a mut S,
    ) -> Self {
        Self {
            z80_ram,
            rom,
            ram,
            bank,
            fm,
            vdp,
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
            // VDP port mirror ($7F00-$7FFF): the shared [`vdp_mirror_read`] — status ($7F04-$7F07, real
            // side-effecting read at the Z80's own frontier time), HV counter ($7F08-$7F0F), and the
            // deliberate `$FF` arms (data port = the ledgered lockup known-difference; $7F10+ write-only).
            // The 68000's window read at the same 15-bit offset routes through the SAME function.
            0x7F00..=0x7FFF => vdp_mirror_read(self.vdp, addr, self.now_mclk),
            // 68k bank window: translate through the 9-bit bank and read 68000 space.
            0x8000..=0xFFFF => {
                let a = self.window_addr(addr);
                self.read_window(a)
            }
            // Bank register ($6000, write-only), PSG ($7F11, write-only), and the VDP DATA-port mirror
            // ($7F00-$7F03): open bus. The data-port mirror stays $FF deliberately — a Z80 read of the VDP
            // data port locks up real hardware (the ledgered `vdp-dataport-read-lockup` known-difference);
            // we return open bus instead of modeling the hang. $7F10-$7F1F (PSG mirror region) is
            // write-only on hardware.
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Z80 RAM (8 KiB), mirrored across $0000-$3FFF.
            0x0000..=0x3FFF => self.z80_ram[(addr as usize) & (Z80_RAM_SIZE - 1)] = value,
            // Bank register ($6000): serial load, LSB-first — each write shifts bit0 of the byte into the top
            // of the 9-bit latch (Plutiedev "Z80 banking"). After 9 writes the full page is selected.
            // The 68000's window write at the same offset ticks the SAME latch (see `bank_latch_tick`).
            0x6000..=0x60FF => bank_latch_tick(self.bank, value),
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

    /// A power-on VDP for the map tests (fixed seed — the seed only randomizes memory contents, not the
    /// port-visible state these tests read).
    fn fresh_vdp() -> Vdp {
        Vdp::power_on(&mut crate::rng::SplitMix64::new(1))
    }

    /// Build a bus over fresh buffers with an explicit bank value and a null sink (helper for the map tests
    /// that do not care about the event stream). A fresh unprogrammed FM chip at mclk 0 (status reads 0x00).
    #[allow(clippy::too_many_arguments)]
    fn bus_with<'a>(
        ram: &'a mut [u8],
        rom: &'a [u8],
        work: &'a mut [u8],
        bank: &'a mut u16,
        fm: &'a mut Ym2612,
        vdp: &'a mut Vdp,
        sink: &'a mut (),
    ) -> Z80Bus<'a, ()> {
        Z80Bus::new(ram, rom, work, bank, fm, vdp, 0, sink)
    }

    #[test]
    fn z80_ram_reads_writes_and_mirrors() {
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut vdp = fresh_vdp();
        let mut bus = bus_with(
            &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, &mut sink,
        );
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
        let mut vdp = fresh_vdp();
        let mut bus = bus_with(
            &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, &mut sink,
        );
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
        let mut vdp = fresh_vdp();
        let mut bus = bus_with(
            &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, &mut sink,
        );
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
        let mut vdp = fresh_vdp();
        let mut bus = bus_with(
            &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, &mut sink,
        );
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
        let mut vdp = fresh_vdp();
        let mut bus = bus_with(
            &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, &mut sink,
        );
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
        let mut vdp = fresh_vdp();
        {
            let mut bus = Z80Bus::new(
                &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, 0, &mut sink,
            );
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

    /// Build a bus over `vdp` at an explicit `now_mclk` (the K2 mirror tests care about the read instant).
    #[allow(clippy::too_many_arguments)]
    fn bus_at<'a>(
        ram: &'a mut [u8],
        rom: &'a [u8],
        work: &'a mut [u8],
        bank: &'a mut u16,
        fm: &'a mut Ym2612,
        vdp: &'a mut Vdp,
        now_mclk: u64,
        sink: &'a mut (),
    ) -> Z80Bus<'a, ()> {
        Z80Bus::new(ram, rom, work, bank, fm, vdp, now_mclk, sink)
    }

    #[test]
    fn vdp_status_mirror_reads_the_live_status_bytes() {
        // K2: $7F04-$7F07 mirror the VDP control port — a read returns the LIVE status word's bytes, with
        // the same byte-lane split as a 68k byte read of $C00004/5 (even = high byte, odd = low byte).
        use crate::vdp::MCLK_PER_LINE;
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        // One instant in active display (line 100) and one in vblank (line 240): the vblank status bit
        // (b3, in the LOW byte) must differ between them — the fabricated constant $FF cannot do that.
        for (mclk, in_vblank) in [
            (100 * MCLK_PER_LINE + 1000, false),
            (240 * MCLK_PER_LINE + 1000, true),
        ] {
            let mut vdp = fresh_vdp();
            let expected = vdp.status_word(mclk);
            assert_eq!(
                expected & (1 << 3) != 0,
                in_vblank,
                "test instant lands where intended"
            );
            let mut bus = bus_at(
                &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, mclk, &mut sink,
            );
            for base in [0x7F04u16, 0x7F06] {
                assert_eq!(
                    bus.read(base),
                    (expected >> 8) as u8,
                    "even byte = status high at {base:#06X} (mclk {mclk})"
                );
                assert_eq!(
                    bus.read(base + 1),
                    (expected & 0xFF) as u8,
                    "odd byte = status low at {:#06X} (mclk {mclk})",
                    base + 1
                );
            }
        }
    }

    #[test]
    fn vdp_status_mirror_read_clears_the_control_port_toggle() {
        // A status read through the Z80 mirror has the SAME side effect as a 68k $C00004 read: it clears
        // the control-port write-toggle (the pinned recon-vdp semantic). Discriminator: arm the toggle with
        // a command's first word, status-read through the mirror, then write $8F02 — if the toggle was
        // cleared it lands as a register write (reg 15 = 2); if not, it would complete the command instead.
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();

        // Control leg (proves the discriminator): armed toggle + NO mirror read → $8F02 is command word 2,
        // NOT a register write.
        let mut vdp = fresh_vdp();
        vdp.control_write(0x8F44, 0); // reg 15 = $44 (a known non-default)
        vdp.control_write(0x4100, 0); // VRAM-write command word 1 → arms the toggle
        vdp.control_write(0x8F02, 0); // completes the command (toggle armed)
        assert_eq!(
            vdp.regs()[0x0F],
            0x44,
            "without the mirror read, $8F02 is swallowed as command word 2"
        );

        // The real leg: armed toggle + a $7F05 status read through the Z80 bus → toggle cleared.
        let mut vdp = fresh_vdp();
        vdp.control_write(0x8F44, 0);
        vdp.control_write(0x4100, 0);
        {
            let mut bus = bus_at(
                &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, 0, &mut sink,
            );
            bus.read(0x7F05);
        }
        vdp.control_write(0x8F02, 0);
        assert_eq!(
            vdp.regs()[0x0F],
            0x02,
            "the mirror status read cleared the toggle — $8F02 is a register write again"
        );
    }

    #[test]
    fn vdp_hv_mirror_reads_the_live_hv_counter() {
        // $7F08-$7F0F mirror the HV counter port: even byte = V (word high), odd byte = H (word low) —
        // the same lane split as a 68k byte read of $C00008/9. Side-effect-free.
        use crate::vdp::MCLK_PER_LINE;
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mclk = 100 * MCLK_PER_LINE + 1000; // line 100 → V = 0x64, clearly not $FF
        let mut vdp = fresh_vdp();
        let expected = vdp.hv_counter_read(mclk);
        assert_eq!(expected >> 8, 0x64, "V counter at line 100");
        let mut bus = bus_at(
            &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, mclk, &mut sink,
        );
        for base in [0x7F08u16, 0x7F0A, 0x7F0C, 0x7F0E] {
            assert_eq!(
                bus.read(base),
                (expected >> 8) as u8,
                "even byte = V at {base:#06X}"
            );
            assert_eq!(
                bus.read(base + 1),
                (expected & 0xFF) as u8,
                "odd byte = H at {:#06X}",
                base + 1
            );
        }
    }

    #[test]
    fn vdp_data_mirror_stays_open_bus_with_no_side_effects() {
        // $7F00-$7F03 (the data port) is UNCHANGED by K2: reads return $FF and touch no VDP state — the
        // ledgered `vdp-dataport-read-lockup` known-difference (a Z80 data-port read hangs real hardware;
        // we return open bus instead of modeling the hang). The armed toggle must survive these reads.
        let mut ram = vec![0u8; Z80_RAM_SIZE];
        let rom = vec![0u8; 0x10];
        let mut work = vec![0u8; RAM_SIZE];
        let mut bank = 0u16;
        let mut sink = ();
        let mut fm = Ym2612::new();
        let mut vdp = fresh_vdp();
        vdp.control_write(0x8F44, 0); // reg 15 = $44
        vdp.control_write(0x4100, 0); // arm the toggle
        {
            let mut bus = bus_at(
                &mut ram, &rom, &mut work, &mut bank, &mut fm, &mut vdp, 0, &mut sink,
            );
            for a in [0x7F00u16, 0x7F01, 0x7F02, 0x7F03] {
                assert_eq!(bus.read(a), 0xFF, "data-port mirror read at {a:#06X}");
            }
        }
        vdp.control_write(0x8F02, 0); // toggle still armed → swallowed as command word 2
        assert_eq!(
            vdp.regs()[0x0F],
            0x44,
            "data-port mirror reads left the toggle armed (no side effects)"
        );
    }
}
