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
}
