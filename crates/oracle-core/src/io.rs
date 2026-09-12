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

/// One of the three I/O ports, each with a Data, Control and serial register set (recon IO1): Port 1
/// (Player 1), Port 2 (Player 2) and EXP (the modem/EXT connector). The bus's address decode ([`io_reg`])
/// is where one comes from, so a register access can only name a port that exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Port {
    /// Port 1, Player 1's controller port (Data at `$A10003`).
    P1,
    /// Port 2, Player 2's controller port (Data at `$A10005`).
    P2,
    /// EXP, the modem/EXT connector (Data at `$A10007`): a full register set, and no pad.
    Exp,
}

impl Port {
    /// Every port, in register order.
    pub const ALL: [Port; 3] = [Port::P1, Port::P2, Port::Exp];

    /// This port's slot in the block's per-port registers: `0`, `1` or `2`, in [`Port::ALL`] order.
    pub const fn index(self) -> usize {
        match self {
            Port::P1 => 0,
            Port::P2 => 1,
            Port::Exp => 2,
        }
    }

    /// The pad plugged into this port, or `None` for EXP, which has none.
    pub const fn pad_port(self) -> Option<PadPort> {
        match self {
            Port::P1 => Some(PadPort::P1),
            Port::P2 => Some(PadPort::P2),
            Port::Exp => None,
        }
    }
}

/// A port with a 3-button pad: Port 1 or Port 2. EXP is a [`Port`] and not one of these, so "the pad on
/// EXP" is not a value the pad accessors can be handed ([`Io`]'s doc shows the call failing to compile).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadPort {
    /// Port 1, Player 1.
    P1,
    /// Port 2, Player 2.
    P2,
}

impl PadPort {
    /// Both pad ports, in port order.
    pub const ALL: [PadPort; 2] = [PadPort::P1, PadPort::P2];

    /// This pad's slot: `0` for P1, `1` for P2. It is also the number the wire's `port` param and the
    /// player's status strip use for it.
    pub const fn index(self) -> usize {
        match self {
            PadPort::P1 => 0,
            PadPort::P2 => 1,
        }
    }

    /// The pad port numbered `index` (`0` = P1, `1` = P2), or `None` for any other number.
    ///
    /// **The one conversion from a number.** Everything below the wire takes a `PadPort`. A number arrives
    /// from outside in two places, the wire's `port` param (`oracle-aether`'s `parse_port`, which refuses
    /// in words) and the `motion_run` example's script parser, and both come through here.
    pub const fn from_index(index: usize) -> Option<PadPort> {
        match index {
            0 => Some(PadPort::P1),
            1 => Some(PadPort::P2),
            _ => None,
        }
    }

    /// The [`Port`] this pad is plugged into.
    pub const fn port(self) -> Port {
        match self {
            PadPort::P1 => Port::P1,
            PadPort::P2 => Port::P2,
        }
    }
}

/// Which register an odd address in `$A10003..=$A1001F` selects. The [`Port`] it belongs to is the other
/// half of [`io_reg`]'s answer. The version register (`$A10001`) is **not** here — the bus answers it with
/// [`crate::bus::MD_VERSION`].
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
pub fn io_reg(addr: u32) -> Option<(Port, IoReg)> {
    use IoReg::*;
    use Port::{Exp, P1, P2};
    let hit = match addr {
        0xA1_0003 => (P1, Data),
        0xA1_0005 => (P2, Data),
        0xA1_0007 => (Exp, Data),
        0xA1_0009 => (P1, Ctrl),
        0xA1_000B => (P2, Ctrl),
        0xA1_000D => (Exp, Ctrl),
        0xA1_000F => (P1, TxData),
        0xA1_0011 => (P1, RxData),
        0xA1_0013 => (P1, SCtrl),
        0xA1_0015 => (P2, TxData),
        0xA1_0017 => (P2, RxData),
        0xA1_0019 => (P2, SCtrl),
        0xA1_001B => (Exp, TxData),
        0xA1_001D => (Exp, RxData),
        0xA1_001F => (Exp, SCtrl),
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

/// The I/O controller block: one register set per [`Port`] (at [`Port::index`]) and one injected pad per
/// [`PadPort`] (at [`PadPort::index`]). See the module docs for the currency disposition.
///
/// # Which ports have what, by type (lens M76)
///
/// Every port has a Data, Control and serial register set, and those accessors take a [`Port`]: P1, P2
/// or EXP. Only P1 and P2 have a pad, and the pad accessors take a [`PadPort`], which has no EXP. So a
/// pad on EXP and a fourth port are not values any accessor can be handed. The one place a number
/// becomes a port is the wire's `port` param (`oracle-aether`'s `parse_port`, through
/// [`PadPort::from_index`]), which refuses any other number in words. Until M76 every accessor took a
/// `usize` and they disagreed: `pad(2)` panicked, `read_data(2)` answered as a released pad, and
/// `read_data(3)` indexed out of bounds.
///
/// The calls a caller can make compile and run:
///
/// ```
/// use oracle_core::io::{Io, Pad, PadPort, Port};
/// let mut io = Io::default();
/// io.set_pad(PadPort::P2, Pad { a: true, ..Pad::default() });
/// assert!(io.pad(PadPort::P2).a);
/// io.write_ctrl(Port::Exp, 0x40);
/// io.write_data(Port::Exp, 0x40);
/// assert_eq!(io.read_data(Port::Exp), 0xFF, "EXP's Data register answers, as for a released pad");
/// ```
///
/// A pad asked of EXP by number does not:
///
/// ```compile_fail,E0308
/// use oracle_core::io::Io;
/// let io = Io::default();
/// let _ = io.pad(2);
/// ```
///
/// Nor a register of a port that does not exist:
///
/// ```compile_fail,E0308
/// use oracle_core::io::Io;
/// let io = Io::default();
/// let _ = io.read_data(3);
/// ```
///
/// Nor EXP handed to a pad accessor:
///
/// ```compile_fail,E0308
/// use oracle_core::io::{Io, Port};
/// let io = Io::default();
/// let _ = io.pad(Port::Exp);
/// ```
///
/// Stable rustdoc does not check the `E0308` codes on these blocks (see
/// [`StateHash::compute`](crate::state_hash::StateHash::compute), where it was measured), so the control
/// above is what ties each to the rule: the same calls with the right types, which compile and run.
#[derive(Clone, Debug, Default, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Io {
    /// Data-register latch per port. Every written bit is retained; only pins configured as outputs actually
    /// drive the wire (recon IO3). Read back through [`Io::read_data`].
    data: [u8; Port::ALL.len()],
    /// Control (direction) register per port: bit = 1 output, bit = 0 input; bit 7 = TH-interrupt enable
    /// (recon IO2). Power-on `$00` = all inputs.
    ctrl: [u8; Port::ALL.len()],
    /// Serial TxData latch per port (stub — reads back the last write; no serial peripheral is attached).
    txdata: [u8; Port::ALL.len()],
    /// Serial S-Control latch per port (stub — reads back the last write).
    sctrl: [u8; Port::ALL.len()],
    /// Injected pad state, one per [`PadPort`]: EXP has no pad, so it has no slot here. Never driven by
    /// host input (recon IO4).
    pad: [Pad; PadPort::ALL.len()],
}

impl Io {
    /// Inject 3-button pad state for a pad port. This is the sole input path — deterministic injected state,
    /// no host coupling. The next Data-register read reflects it (recon IO4). Total: EXP has no pad, and a
    /// [`PadPort`] cannot name it.
    pub fn set_pad(&mut self, port: PadPort, pad: Pad) {
        self.pad[port.index()] = pad;
    }

    /// The currently injected pad state for a pad port.
    pub fn pad(&self, port: PadPort) -> Pad {
        self.pad[port.index()]
    }

    /// Read a Data register (recon IO3): output pins return the latch, input pins return the pad device byte.
    /// `TH_line` is the latch's bit 6 when TH is an output, else pull-up high. EXP has no pad
    /// ([`Port::pad_port`] is `None`), so its device byte is that of an all-released pad.
    pub fn read_data(&self, port: Port) -> u8 {
        let ctrl = self.ctrl[port.index()];
        let latch = self.data[port.index()];
        let th_high = if ctrl & (1 << TH_BIT) != 0 {
            latch & (1 << TH_BIT) != 0
        } else {
            true // input pin floats high
        };
        let pad = port.pad_port().map_or(Pad::default(), |p| self.pad(p));
        let device = pad_device_byte(pad, th_high);
        (latch & ctrl) | (device & !ctrl)
    }

    /// Write a Data register: every bit is latched; only output pins drive the wire (recon IO3).
    pub fn write_data(&mut self, port: Port, byte: u8) {
        self.data[port.index()] = byte;
    }

    /// Read a Control (direction) register (recon IO2).
    pub fn read_ctrl(&self, port: Port) -> u8 {
        self.ctrl[port.index()]
    }

    /// Write a Control (direction) register.
    pub fn write_ctrl(&mut self, port: Port, byte: u8) {
        self.ctrl[port.index()] = byte;
    }

    /// Read serial TxData (stub: the last byte written — decision 2 in the plan).
    pub fn read_txdata(&self, port: Port) -> u8 {
        self.txdata[port.index()]
    }

    /// Write serial TxData (retained; drives no real UART).
    pub fn write_txdata(&mut self, port: Port, byte: u8) {
        self.txdata[port.index()] = byte;
    }

    /// Read serial S-Control (stub: the last byte written).
    pub fn read_sctrl(&self, port: Port) -> u8 {
        self.sctrl[port.index()]
    }

    /// Write serial S-Control (retained).
    pub fn write_sctrl(&mut self, port: Port, byte: u8) {
        self.sctrl[port.index()] = byte;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pad_round_trips_through_the_accessor() {
        let mut io = Io::default();
        io.set_pad(
            PadPort::P1,
            Pad {
                start: true,
                up: true,
                ..Default::default()
            },
        );
        io.set_pad(
            PadPort::P2,
            Pad {
                a: true,
                ..Default::default()
            },
        );
        assert_eq!(
            io.pad(PadPort::P1),
            Pad {
                start: true,
                up: true,
                ..Default::default()
            }
        );
        assert_eq!(
            io.pad(PadPort::P2),
            Pad {
                a: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn default_is_all_released_all_input() {
        let io = Io::default();
        assert_eq!(io.pad(PadPort::P1), Pad::default());
        assert_eq!(io.pad(PadPort::P2), Pad::default());
    }

    /// **The two port types agree** (lens M76): the pad ports are exactly the ports whose
    /// [`Port::pad_port`] is `Some`, each maps back to the port it came from, a pad's slot is its port's
    /// register slot, and the only numbers that name a pad port are its own two slots.
    ///
    /// Replaces `exp_port_has_no_pad`, a `should_panic` on `pad(2)`. That call can no longer be written:
    /// [`Io`]'s doc shows it failing to compile, beside a control that compiles.
    #[test]
    fn the_pad_ports_are_exactly_the_ports_with_a_pad() {
        let with_pad: Vec<PadPort> = Port::ALL.iter().filter_map(|p| p.pad_port()).collect();
        assert_eq!(with_pad, PadPort::ALL, "the pad ports, in port order");
        assert_eq!(Port::Exp.pad_port(), None, "EXP has no pad (recon IO1)");
        for p in PadPort::ALL {
            assert_eq!(p.port().pad_port(), Some(p), "{p:?} round-trips through its port");
            assert_eq!(p.index(), p.port().index(), "{p:?}'s pad slot is its port's slot");
            assert_eq!(
                PadPort::from_index(p.index()),
                Some(p),
                "{p:?} is found by its own number"
            );
        }
        for (i, p) in Port::ALL.iter().enumerate() {
            assert_eq!(p.index(), i, "{p:?} is slot {i}, in register order");
        }
        for n in [PadPort::ALL.len(), Port::ALL.len(), usize::MAX] {
            assert_eq!(PadPort::from_index(n), None, "{n} names no pad port");
        }
    }

    /// A port configured for a normal 3-button read: TH is the only output (`ctrl = $40`).
    fn configured(latch: u8) -> Io {
        let mut io = Io::default();
        io.write_ctrl(Port::P1, 0x40);
        io.write_data(Port::P1, latch);
        io
    }

    #[test]
    fn th_high_reports_c_b_right_left_down_up() {
        // TH=1 (latch $40). Press C (bit5) + Right (bit3). Active-low → those bits read 0, the rest 1.
        // device = 0b1101_0111 (0xD7); read = latch|(device&!ctrl) = 0xD7.
        let mut io = configured(0x40);
        io.set_pad(
            PadPort::P1,
            Pad {
                c: true,
                right: true,
                ..Default::default()
            },
        );
        assert_eq!(io.read_data(Port::P1), 0xD7);
    }

    #[test]
    fn th_low_reports_start_a_and_forces_bits_2_3_low() {
        // TH=0 (latch $00). Press Start (bit5). A released (bit4=1); bits 3,2 forced 0; Down/Up=1.
        // device = 0b1001_0011 (0x93); read = 0x93.
        let mut io = configured(0x00);
        io.set_pad(
            PadPort::P1,
            Pad {
                start: true,
                ..Default::default()
            },
        );
        assert_eq!(io.read_data(Port::P1), 0x93);
        // The detection signature: bits 3 and 2 are 0 no matter what (nothing maps there at TH=0).
        assert_eq!(io.read_data(Port::P1) & 0b0000_1100, 0);
    }

    #[test]
    fn all_released_reads_high_active_low() {
        // TH=1, nothing pressed → the low six bits are all 1 (released). read = 0xFF.
        assert_eq!(configured(0x40).read_data(Port::P1), 0xFF);
    }

    #[test]
    fn input_pins_take_the_device_output_pins_return_the_latch() {
        // Only TH output ($40): the six button bits come from the device regardless of the latch.
        let mut only_th = configured(0x40);
        only_th.write_data(Port::P1, 0x7F); // try to drive the button bits — ignored, they are inputs
        only_th.set_pad(
            PadPort::P1,
            Pad {
                up: true,
                ..Default::default()
            },
        );
        assert_eq!(
            only_th.read_data(Port::P1) & 0x01,
            0,
            "Up (input) reads the device, not the latch"
        );
        // All pins output ($7F low 7): the low seven bits read straight back from the latch, buttons ignored.
        let mut all_out = Io::default();
        all_out.write_ctrl(Port::P1, 0x7F);
        all_out.write_data(Port::P1, 0x2A);
        all_out.set_pad(
            PadPort::P1,
            Pad {
                up: true,
                c: true,
                ..Default::default()
            },
        );
        assert_eq!(all_out.read_data(Port::P1) & 0x7F, 0x2A);
    }

    #[test]
    fn exp_port_reads_an_all_released_pad() {
        // EXP has no pad; a normal read config sees the released device byte.
        let mut io = Io::default();
        io.write_ctrl(Port::Exp, 0x40);
        io.write_data(Port::Exp, 0x40);
        assert_eq!(io.read_data(Port::Exp), 0xFF);
    }

    #[test]
    fn serial_registers_are_deterministic_stubs() {
        let mut io = Io::default();
        io.write_txdata(Port::P2, 0x5A);
        io.write_sctrl(Port::P2, 0x3C);
        assert_eq!(
            io.read_txdata(Port::P2),
            0x5A,
            "TxData reads back the last write"
        );
        assert_eq!(
            io.read_sctrl(Port::P2),
            0x3C,
            "S-Control reads back the last write"
        );
        // RxData is handled at the bus (reads 0) — there is no serial device driving the receive line.
    }

    #[test]
    fn io_reg_maps_every_documented_register() {
        use IoReg::*;
        use Port::{Exp, P1, P2};
        let table = [
            (0xA1_0003, (P1, Data)),
            (0xA1_0005, (P2, Data)),
            (0xA1_0007, (Exp, Data)),
            (0xA1_0009, (P1, Ctrl)),
            (0xA1_000B, (P2, Ctrl)),
            (0xA1_000D, (Exp, Ctrl)),
            (0xA1_000F, (P1, TxData)),
            (0xA1_0011, (P1, RxData)),
            (0xA1_0013, (P1, SCtrl)),
            (0xA1_0015, (P2, TxData)),
            (0xA1_0017, (P2, RxData)),
            (0xA1_0019, (P2, SCtrl)),
            (0xA1_001B, (Exp, TxData)),
            (0xA1_001D, (Exp, RxData)),
            (0xA1_001F, (Exp, SCtrl)),
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
