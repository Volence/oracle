//! The Mega Drive I/O controller block (`$A10003–$A1001F`) — data / control registers, the serial-register
//! stubs, and the injected 3-button pad state. The version register at `$A10001` stays a bus constant
//! ([`crate::bus::MD_VERSION`]); everything below it lives here.
//!
//! Byte formats and the 3-button TH protocol are pinned in `docs/2026-07-17-io-recon.md` (IO1–IO6). The
//! read model is IO3: `read = (latch & ctrl) | (device & !ctrl)`. Input is **injected state only**
//! ([`Io::set_pad`]) — there is no host-input path anywhere in the core.
//!
//! **Currency note:** `Io` is in **neither** frozen currency (Oracle `state_hash` / `export_state`) — an
//! export-v2 candidate, exactly like the VDP SAT cache. It rides the internal bincode snapshot so pad state
//! and the register latches survive snapshot/restore for determinism, but it is deliberately *not* emitted by
//! `export_state` (which would move the frozen golden). When a differential consumer needs pad state in the
//! currency, it lands in the v2 layout bump.

/// TH — the select line, bit 6 of a port's Data register (recon IO4). The game drives it as an output to
/// pick which nibble the 3-button pad presents.
const TH_BIT: u32 = 6;

/// Which register an odd address in `$A10003..=$A1001F` selects, and for which port (0 = P1, 1 = P2, 2 = EXP).
/// The version register (`$A10001`) is **not** here — the bus answers it with [`crate::bus::MD_VERSION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoReg {
    /// Parallel Data register (recon IO3).
    Data,
    /// Control / direction register (recon IO2).
    Ctrl,
    /// Serial transmit data (stub — reads back the last write).
    TxData,
    /// Serial receive data (stub — reads `0`, no device driving the line).
    RxData,
    /// Serial control (stub — reads back the last write).
    SCtrl,
}

/// Decode an odd I/O address to `(port, register)`, or `None` if it is not one of the 15 mapped registers
/// (recon IO1). Even addresses and `$A10001` (version) return `None`.
pub fn io_reg(addr: u32) -> Option<(usize, IoReg)> {
    use IoReg::*;
    let hit = match addr {
        0xA1_0003 => (0, Data),
        0xA1_0005 => (1, Data),
        0xA1_0007 => (2, Data),
        0xA1_0009 => (0, Ctrl),
        0xA1_000B => (1, Ctrl),
        0xA1_000D => (2, Ctrl),
        0xA1_000F => (0, TxData),
        0xA1_0011 => (0, RxData),
        0xA1_0013 => (0, SCtrl),
        0xA1_0015 => (1, TxData),
        0xA1_0017 => (1, RxData),
        0xA1_0019 => (1, SCtrl),
        0xA1_001B => (2, TxData),
        0xA1_001D => (2, RxData),
        0xA1_001F => (2, SCtrl),
        _ => return None,
    };
    Some(hit)
}

/// The byte a 3-button pad drives given the TH line it sees (recon IO4). Active-low: a pressed button reads
/// `0`, released reads `1`. Bit 7 (and, in the TH-high set, the undriven high bits) float high via the port
/// pull-ups. TH (bit 6) is normally an output, so its read-back comes from the console latch via the IO3
/// model — the value placed here is masked out for an output TH.
fn pad_device_byte(pad: Pad, th_high: bool) -> u8 {
    let lo = |pressed: bool| -> u8 {
        if pressed {
            0
        } else {
            1
        }
    };
    if th_high {
        // bits 7,6 pull-up high; 5=C 4=B 3=Right 2=Left 1=Down 0=Up.
        0b1100_0000
            | (lo(pad.c) << 5)
            | (lo(pad.b) << 4)
            | (lo(pad.right) << 3)
            | (lo(pad.left) << 2)
            | (lo(pad.down) << 1)
            | lo(pad.up)
    } else {
        // bit 7 pull-up high; 6=TH(0); 5=Start 4=A; bits 3,2 forced low (the MD-pad detection signature);
        // 1=Down 0=Up.
        0b1000_0000 | (lo(pad.start) << 5) | (lo(pad.a) << 4) | (lo(pad.down) << 1) | lo(pad.up)
    }
}

/// One 3-button Mega Drive pad's button state. `true` = the button is held this instant. Injected state only
/// (see [`Io::set_pad`]); serialized as part of the machine snapshot. Active-low on the wire is applied at
/// read time (recon IO4), not stored here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Pad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub b: bool,
    pub c: bool,
    pub a: bool,
    pub start: bool,
}

/// The I/O controller block. Index 0 = Port 1 (Player 1), 1 = Port 2 (Player 2), 2 = EXP (modem/EXT — no
/// pad). See the module docs for the currency disposition.
#[derive(Clone, Debug, Default, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Io {
    /// Data-register latch per port. Every written bit is retained; only pins configured as outputs actually
    /// drive the wire (recon IO3). Read back through [`Io::read_data`].
    data: [u8; 3],
    /// Control (direction) register per port: bit = 1 output, bit = 0 input; bit 7 = TH-interrupt enable
    /// (recon IO2). Power-on `$00` = all inputs.
    ctrl: [u8; 3],
    /// Serial TxData latch per port (stub — reads back the last write; no serial peripheral is attached).
    txdata: [u8; 3],
    /// Serial S-Control latch per port (stub — reads back the last write).
    sctrl: [u8; 3],
    /// Injected pad state. Ports 0/1 only; EXP has no pad. Never driven by host input (recon IO4).
    pad: [Pad; 2],
}

impl Io {
    /// Inject 3-button pad state for port 0 (Player 1) or 1 (Player 2). EXP (port 2) has no pad. This is the
    /// sole input path — deterministic injected state, no host coupling. The next Data-register read reflects
    /// it (recon IO4).
    pub fn set_pad(&mut self, port: usize, pad: Pad) {
        assert!(port < 2, "only ports 0 (P1) and 1 (P2) have a pad");
        self.pad[port] = pad;
    }

    /// The currently injected pad state for a port (0 = P1, 1 = P2).
    pub fn pad(&self, port: usize) -> Pad {
        assert!(port < 2, "only ports 0 (P1) and 1 (P2) have a pad");
        self.pad[port]
    }

    /// Read a Data register (recon IO3): output pins return the latch, input pins return the pad device byte.
    /// `TH_line` is the latch's bit 6 when TH is an output, else pull-up high. EXP (port 2) has no pad, so its
    /// device byte is that of an all-released pad.
    pub fn read_data(&self, port: usize) -> u8 {
        let ctrl = self.ctrl[port];
        let latch = self.data[port];
        let th_high = if ctrl & (1 << TH_BIT) != 0 {
            latch & (1 << TH_BIT) != 0
        } else {
            true // input pin floats high
        };
        let pad = if port < 2 {
            self.pad[port]
        } else {
            Pad::default()
        };
        let device = pad_device_byte(pad, th_high);
        (latch & ctrl) | (device & !ctrl)
    }

    /// Write a Data register: every bit is latched; only output pins drive the wire (recon IO3).
    pub fn write_data(&mut self, port: usize, byte: u8) {
        self.data[port] = byte;
    }

    /// Read a Control (direction) register (recon IO2).
    pub fn read_ctrl(&self, port: usize) -> u8 {
        self.ctrl[port]
    }

    /// Write a Control (direction) register.
    pub fn write_ctrl(&mut self, port: usize, byte: u8) {
        self.ctrl[port] = byte;
    }

    /// Read serial TxData (stub: the last byte written — decision 2 in the plan).
    pub fn read_txdata(&self, port: usize) -> u8 {
        self.txdata[port]
    }

    /// Write serial TxData (retained; drives no real UART).
    pub fn write_txdata(&mut self, port: usize, byte: u8) {
        self.txdata[port] = byte;
    }

    /// Read serial S-Control (stub: the last byte written).
    pub fn read_sctrl(&self, port: usize) -> u8 {
        self.sctrl[port]
    }

    /// Write serial S-Control (retained).
    pub fn write_sctrl(&mut self, port: usize, byte: u8) {
        self.sctrl[port] = byte;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pad_round_trips_through_the_accessor() {
        let mut io = Io::default();
        io.set_pad(
            0,
            Pad {
                start: true,
                up: true,
                ..Default::default()
            },
        );
        io.set_pad(
            1,
            Pad {
                a: true,
                ..Default::default()
            },
        );
        assert_eq!(
            io.pad(0),
            Pad {
                start: true,
                up: true,
                ..Default::default()
            }
        );
        assert_eq!(
            io.pad(1),
            Pad {
                a: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn default_is_all_released_all_input() {
        let io = Io::default();
        assert_eq!(io.pad(0), Pad::default());
        assert_eq!(io.pad(1), Pad::default());
    }

    #[test]
    #[should_panic(expected = "only ports 0")]
    fn exp_port_has_no_pad() {
        Io::default().pad(2);
    }

    /// A port configured for a normal 3-button read: TH is the only output (`ctrl = $40`).
    fn configured(latch: u8) -> Io {
        let mut io = Io::default();
        io.write_ctrl(0, 0x40);
        io.write_data(0, latch);
        io
    }

    #[test]
    fn th_high_reports_c_b_right_left_down_up() {
        // TH=1 (latch $40). Press C (bit5) + Right (bit3). Active-low → those bits read 0, the rest 1.
        // device = 0b1101_0111 (0xD7); read = latch|(device&!ctrl) = 0xD7.
        let mut io = configured(0x40);
        io.set_pad(
            0,
            Pad {
                c: true,
                right: true,
                ..Default::default()
            },
        );
        assert_eq!(io.read_data(0), 0xD7);
    }

    #[test]
    fn th_low_reports_start_a_and_forces_bits_2_3_low() {
        // TH=0 (latch $00). Press Start (bit5). A released (bit4=1); bits 3,2 forced 0; Down/Up=1.
        // device = 0b1001_0011 (0x93); read = 0x93.
        let mut io = configured(0x00);
        io.set_pad(
            0,
            Pad {
                start: true,
                ..Default::default()
            },
        );
        assert_eq!(io.read_data(0), 0x93);
        // The detection signature: bits 3 and 2 are 0 no matter what (nothing maps there at TH=0).
        assert_eq!(io.read_data(0) & 0b0000_1100, 0);
    }

    #[test]
    fn all_released_reads_high_active_low() {
        // TH=1, nothing pressed → the low six bits are all 1 (released). read = 0xFF.
        assert_eq!(configured(0x40).read_data(0), 0xFF);
    }

    #[test]
    fn input_pins_take_the_device_output_pins_return_the_latch() {
        // Only TH output ($40): the six button bits come from the device regardless of the latch.
        let mut only_th = configured(0x40);
        only_th.write_data(0, 0x7F); // try to drive the button bits — ignored, they are inputs
        only_th.set_pad(
            0,
            Pad {
                up: true,
                ..Default::default()
            },
        );
        assert_eq!(
            only_th.read_data(0) & 0x01,
            0,
            "Up (input) reads the device, not the latch"
        );
        // All pins output ($7F low 7): the low seven bits read straight back from the latch, buttons ignored.
        let mut all_out = Io::default();
        all_out.write_ctrl(0, 0x7F);
        all_out.write_data(0, 0x2A);
        all_out.set_pad(
            0,
            Pad {
                up: true,
                c: true,
                ..Default::default()
            },
        );
        assert_eq!(all_out.read_data(0) & 0x7F, 0x2A);
    }

    #[test]
    fn exp_port_reads_an_all_released_pad() {
        // Port 2 (EXP) has no pad; a normal read config sees the released device byte.
        let mut io = Io::default();
        io.write_ctrl(2, 0x40);
        io.write_data(2, 0x40);
        assert_eq!(io.read_data(2), 0xFF);
    }

    #[test]
    fn serial_registers_are_deterministic_stubs() {
        let mut io = Io::default();
        io.write_txdata(1, 0x5A);
        io.write_sctrl(1, 0x3C);
        assert_eq!(io.read_txdata(1), 0x5A, "TxData reads back the last write");
        assert_eq!(
            io.read_sctrl(1),
            0x3C,
            "S-Control reads back the last write"
        );
        // RxData is handled at the bus (reads 0) — there is no serial device driving the receive line.
    }

    #[test]
    fn io_reg_maps_every_documented_register() {
        use IoReg::*;
        let table = [
            (0xA1_0003, (0, Data)),
            (0xA1_0005, (1, Data)),
            (0xA1_0007, (2, Data)),
            (0xA1_0009, (0, Ctrl)),
            (0xA1_000B, (1, Ctrl)),
            (0xA1_000D, (2, Ctrl)),
            (0xA1_000F, (0, TxData)),
            (0xA1_0011, (0, RxData)),
            (0xA1_0013, (0, SCtrl)),
            (0xA1_0015, (1, TxData)),
            (0xA1_0017, (1, RxData)),
            (0xA1_0019, (1, SCtrl)),
            (0xA1_001B, (2, TxData)),
            (0xA1_001D, (2, RxData)),
            (0xA1_001F, (2, SCtrl)),
        ];
        for (addr, want) in table {
            assert_eq!(io_reg(addr), Some(want), "{addr:#X}");
        }
        // The version register and the even bytes are not I/O registers.
        assert_eq!(io_reg(0xA1_0001), None, "version reg is not in the io map");
        assert_eq!(io_reg(0xA1_0002), None, "even byte");
        assert_eq!(io_reg(0xA1_0004), None, "even byte");
    }
}
