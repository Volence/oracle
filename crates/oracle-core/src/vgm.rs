//! `VgmLogger` — the Phase-RT FM/PSG register-tap consumer (RT-2).
//!
//! A [`BusEventSink`] that decodes the machine's sound-chip register writes out of the bus-event stream and
//! accumulates them as normalized [`VgmRecord`]s, then renders those records to canonical VGM on demand. It is
//! a **pure consumer**: it touches no `System` state, synthesizes no audio (RT8 — no operators, envelopes, DAC
//! mix, or LFSR), and is **currency-neutral by construction** — an opt-in caller-owned sink over an existing
//! event stream (the null `()` path is untouched; no committed fixture releases the Z80 or plays sound, so it
//! captures zero writes in every gate).
//!
//! The decode follows the design recon `docs/2026-07-22-phase-rt-design.md`:
//! - **FM (RT1):** the YM2612 is a two-step latch-then-data protocol per bank. An address-port write latches the
//!   register number; a data-port write completes a `(bank, latched reg, value)` triple.
//! - **PSG (RT2):** the SN76489 is a single write-only port of self-describing bytes — `bit7=1` latches the
//!   channel/type selector (and writes the low nibble), `bit7=0` is a bare data byte; every byte is recorded.
//! - **One chip, two windows (RT3):** each chip is reachable from the Z80 (`$4000-$4003`/`$7F11`, fc=0) and the
//!   68000 (`$A04000-$A04003`/`$C00011`, fc=5/6). The logger classifies on `addr` ALONE (fc-agnostic), folding
//!   both windows into the same decoder state.
//! - **Representation (RT4):** the normalized record is the source of truth; VGM is a render target.
//! - **Timing (RT6):** frame-bucketed waits — each record is stamped with the frame index latched from
//!   [`on_step_boundary`](BusEventSink::on_step_boundary); the renderer emits one 735-sample frame-wait (`0x62`)
//!   at each frame boundary.

use crate::bus::{BusEvent, BusEventSink, BusOp};

/// The two Genesis sound chips a `VgmLogger` decodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundChip {
    /// The YM2612 FM synthesizer (`$4000-$4003` / `$A04000-$A04003`).
    Ym2612,
    /// The SN76489 PSG (`$7F11` / `$C00011`).
    Psg,
}

/// One decoded sound-chip register write, normalized independent of which window (Z80 or 68k) issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VgmRecord {
    /// Which chip this write targets.
    pub chip: SoundChip,
    /// YM2612 bank (0 = part I, 1 = part II / channels 4-6); 0 for the PSG.
    pub port: u8,
    /// YM2612 latched register number; for the PSG, the tracked channel/type selector (or 0).
    pub reg: u8,
    /// The data byte written.
    pub value: u8,
    /// The frame index this write landed in (RT6 timestamp source).
    pub frame: u64,
}

/// VGM command bytes (VGM spec).
const CMD_PSG: u8 = 0x50; // SN76489 write: `0x50 dd`
const CMD_YM2612_PORT0: u8 = 0x52; // YM2612 part I write: `0x52 aa dd`
const CMD_YM2612_PORT1: u8 = 0x53; // YM2612 part II write: `0x53 aa dd`
const CMD_WAIT_FRAME: u8 = 0x62; // wait 735 samples (1/60 s)
const CMD_END: u8 = 0x66; // end of sound data

/// Samples per NTSC frame at the 44100 Hz VGM timebase (`0x62`).
const SAMPLES_PER_FRAME: u32 = 735;

/// SN76489 clock (Hz) written to VGM header offset `0x0C`.
const SN76489_CLOCK: u32 = 3_579_545;
/// YM2612 clock (Hz) written to VGM header offset `0x2C`.
const YM2612_CLOCK: u32 = 7_670_453;

/// The `BusEventSink` that decodes + records FM/PSG register writes and renders canonical VGM.
pub struct VgmLogger {
    /// Per-bank latched YM2612 register number (RT1): `[bank0, bank1]`.
    fm_addr_latch: [u8; 2],
    /// SN76489 channel/type selector from the last `bit7=1` byte (RT2).
    psg_latch: u8,
    /// The frame index latched from `on_step_boundary` (RT6).
    frame: u64,
    /// The normalized record log (RT4).
    records: Vec<VgmRecord>,
    /// Status counter: completed YM2612 register writes.
    fm_writes: u64,
    /// Status counter: SN76489 byte writes.
    psg_writes: u64,
}

impl VgmLogger {
    /// A fresh logger: all latches/counters zero, no records.
    pub fn new() -> Self {
        Self {
            fm_addr_latch: [0; 2],
            psg_latch: 0,
            frame: 0,
            records: Vec::new(),
            fm_writes: 0,
            psg_writes: 0,
        }
    }

    /// The decoded register-write records, in arrival order.
    pub fn records(&self) -> &[VgmRecord] {
        &self.records
    }

    /// Completed YM2612 register writes recorded so far.
    pub fn fm_writes(&self) -> u64 {
        self.fm_writes
    }

    /// SN76489 byte writes recorded so far.
    pub fn psg_writes(&self) -> u64 {
        self.psg_writes
    }

    /// Whether the logger has captured no writes.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Clear records + counters + latches — the `vgm_start` reset (RT5).
    pub fn reset(&mut self) {
        self.fm_addr_latch = [0; 2];
        self.psg_latch = 0;
        self.frame = 0;
        self.records.clear();
        self.fm_writes = 0;
        self.psg_writes = 0;
    }

    /// Render the recorded writes to canonical VGM 1.50 bytes (RT4/RT6).
    ///
    /// A 0x40-byte header (`"Vgm "` ident, EoF/data offsets, version, the SN76489 + YM2612 clocks, total
    /// samples) followed by the command stream in record arrival order — `0x52`/`0x53` for the two YM2612
    /// ports, `0x50` for the PSG — with one 735-sample frame-wait (`0x62`) emitted at each frame boundary
    /// (when a record's `frame` differs from the previous record's), and `0x66` end-of-data.
    pub fn render_vgm(&self) -> Vec<u8> {
        // Build the command stream first so the header's total-sample count is exact.
        let mut cmds: Vec<u8> = Vec::new();
        let mut total_samples: u32 = 0;
        let mut prev_frame: Option<u64> = None;
        for r in &self.records {
            // Frame-bucketed wait: one 0x62 at each frame boundary (RT6). No wait precedes the first record.
            if let Some(pf) = prev_frame {
                if r.frame != pf {
                    cmds.push(CMD_WAIT_FRAME);
                    total_samples += SAMPLES_PER_FRAME;
                }
            }
            prev_frame = Some(r.frame);
            match r.chip {
                SoundChip::Ym2612 => {
                    cmds.push(if r.port == 0 {
                        CMD_YM2612_PORT0
                    } else {
                        CMD_YM2612_PORT1
                    });
                    cmds.push(r.reg);
                    cmds.push(r.value);
                }
                SoundChip::Psg => {
                    cmds.push(CMD_PSG);
                    cmds.push(r.value);
                }
            }
        }
        cmds.push(CMD_END);

        // 0x40-byte VGM 1.50 header (all multi-byte fields little-endian).
        let mut out = vec![0u8; 0x40];
        out[0x00..0x04].copy_from_slice(b"Vgm ");
        // 0x04 EoF offset: relative to 0x04 → total file size - 4.
        let total_len = 0x40 + cmds.len();
        write_le_u32(&mut out, 0x04, (total_len - 0x04) as u32);
        // 0x08 version.
        write_le_u32(&mut out, 0x08, 0x0000_0150);
        // 0x0C SN76489 clock.
        write_le_u32(&mut out, 0x0C, SN76489_CLOCK);
        // 0x18 total samples (sum of all waits).
        write_le_u32(&mut out, 0x18, total_samples);
        // 0x2C YM2612 clock.
        write_le_u32(&mut out, 0x2C, YM2612_CLOCK);
        // 0x34 VGM data offset: relative to 0x34 → data starts at 0x40, so 0x40 - 0x34 = 0x0C.
        write_le_u32(&mut out, 0x34, 0x0C);

        out.extend_from_slice(&cmds);
        out
    }
}

impl Default for VgmLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Write `value` as a little-endian u32 at `off` in `buf`.
fn write_le_u32(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

impl BusEventSink for VgmLogger {
    fn on_step_boundary(&mut self, _pc: u32, frame: u64) {
        self.frame = frame;
    }

    fn on_event(&mut self, e: BusEvent) {
        if e.op != BusOp::Write {
            return;
        }
        let value = e.value as u8;
        // Classify on `addr` ALONE (fc-agnostic, RT3): the Z80 window and the 68k window fold into one chip.
        match e.addr {
            // FM bank-0 address latch — no record.
            0x4000 | 0xA0_4000 => self.fm_addr_latch[0] = value,
            // FM bank-0 data — completes a triple.
            0x4001 | 0xA0_4001 => {
                self.records.push(VgmRecord {
                    chip: SoundChip::Ym2612,
                    port: 0,
                    reg: self.fm_addr_latch[0],
                    value,
                    frame: self.frame,
                });
                self.fm_writes += 1;
            }
            // FM bank-1 address latch — no record.
            0x4002 | 0xA0_4002 => self.fm_addr_latch[1] = value,
            // FM bank-1 data — completes a triple.
            0x4003 | 0xA0_4003 => {
                self.records.push(VgmRecord {
                    chip: SoundChip::Ym2612,
                    port: 1,
                    reg: self.fm_addr_latch[1],
                    value,
                    frame: self.frame,
                });
                self.fm_writes += 1;
            }
            // PSG: self-describing byte. A `bit7=1` byte latches the channel/type selector; every byte records.
            0x7F11 | 0xC0_0011 => {
                if value & 0x80 != 0 {
                    self.psg_latch = (value >> 4) & 0x07;
                }
                self.records.push(VgmRecord {
                    chip: SoundChip::Psg,
                    port: 0,
                    reg: self.psg_latch,
                    value,
                    frame: self.frame,
                });
                self.psg_writes += 1;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::Size;

    /// Build a `Write` BusEvent for the given raw address/value (byte-sized, fc = 0).
    fn write_event(addr: u32, value: u8) -> BusEvent {
        BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr,
            size: Size::Byte,
            value: value as u32,
        }
    }

    #[test]
    fn fm_decode_latch_then_data_across_both_windows() {
        let mut log = VgmLogger::new();

        // Bank-0 via the Z80 window: addr latch $28, then data $F0 → one triple {Ym2612,0,0x28,0xF0}.
        log.on_event(write_event(0x4000, 0x28));
        assert!(
            log.records().is_empty(),
            "an address write alone produces NO record"
        );
        log.on_event(write_event(0x4001, 0xF0));

        // Bank-0 via the 68k window ($A04000/$A04001): proves the two windows unify (fc-agnostic).
        log.on_event(write_event(0xA0_4000, 0x30));
        log.on_event(write_event(0xA0_4001, 0x77));

        // Bank-1 via the Z80 window.
        log.on_event(write_event(0x4002, 0xB4));
        log.on_event(write_event(0x4003, 0x02));

        assert_eq!(
            log.records(),
            &[
                VgmRecord {
                    chip: SoundChip::Ym2612,
                    port: 0,
                    reg: 0x28,
                    value: 0xF0,
                    frame: 0
                },
                VgmRecord {
                    chip: SoundChip::Ym2612,
                    port: 0,
                    reg: 0x30,
                    value: 0x77,
                    frame: 0
                },
                VgmRecord {
                    chip: SoundChip::Ym2612,
                    port: 1,
                    reg: 0xB4,
                    value: 0x02,
                    frame: 0
                },
            ]
        );
        assert_eq!(log.fm_writes(), 3);
        assert_eq!(log.psg_writes(), 0);
    }

    #[test]
    fn psg_decode_tracks_the_latch_across_both_windows() {
        let mut log = VgmLogger::new();

        // Latch byte (bit7=1): selector = (0x9F >> 4) & 7 = 1; both the latch byte and the following data byte
        // are recorded.
        log.on_event(write_event(0x7F11, 0x9F));
        assert_eq!(log.psg_latch, 1, "bit7=1 latch updated the selector");
        // Data byte (bit7=0): recorded against the tracked selector.
        log.on_event(write_event(0x7F11, 0x3F));

        // A 68k-window write ($C00011) records too, and this one re-latches (bit7=1, selector = 0).
        log.on_event(write_event(0xC0_0011, 0x80));
        assert_eq!(
            log.psg_latch, 0,
            "the $80 latch byte moved the selector to 0"
        );

        assert_eq!(
            log.records(),
            &[
                VgmRecord {
                    chip: SoundChip::Psg,
                    port: 0,
                    reg: 1,
                    value: 0x9F,
                    frame: 0
                },
                VgmRecord {
                    chip: SoundChip::Psg,
                    port: 0,
                    reg: 1,
                    value: 0x3F,
                    frame: 0
                },
                VgmRecord {
                    chip: SoundChip::Psg,
                    port: 0,
                    reg: 0,
                    value: 0x80,
                    frame: 0
                },
            ]
        );
        assert_eq!(log.psg_writes(), 3);
        assert_eq!(log.fm_writes(), 0);
    }

    #[test]
    fn on_step_boundary_stamps_the_frame_and_renders_one_wait_between_frames() {
        let mut log = VgmLogger::new();

        // Frame 5: a completed FM triple.
        log.on_step_boundary(0, 5);
        log.on_event(write_event(0x4000, 0x22));
        log.on_event(write_event(0x4001, 0x11));
        assert_eq!(
            log.records()[0].frame,
            5,
            "the record carries the latched frame"
        );

        // Advance to frame 6: a second FM triple.
        log.on_step_boundary(0, 6);
        log.on_event(write_event(0x4000, 0x23));
        log.on_event(write_event(0x4001, 0x12));
        assert_eq!(log.records()[1].frame, 6);

        // The render places exactly one 0x62 frame-wait between the two writes (one frame boundary).
        let vgm = log.render_vgm();
        let waits = vgm.iter().filter(|&&b| b == CMD_WAIT_FRAME).count();
        assert_eq!(waits, 1, "exactly one 0x62 between the two frames");
    }

    #[test]
    fn render_vgm_header_and_command_stream() {
        let mut log = VgmLogger::new();
        // One FM bank-0 write and one PSG write in frame 0.
        log.on_event(write_event(0x4000, 0x28));
        log.on_event(write_event(0x4001, 0xF0));
        log.on_event(write_event(0x7F11, 0x9F));

        let vgm = log.render_vgm();
        assert_eq!(&vgm[0x00..0x04], b"Vgm ", "VGM ident");
        assert_eq!(
            read_le_u32(&vgm, 0x0C),
            SN76489_CLOCK,
            "SN76489 clock at 0x0C"
        );
        assert_eq!(
            read_le_u32(&vgm, 0x2C),
            YM2612_CLOCK,
            "YM2612 clock at 0x2C"
        );
        assert_eq!(read_le_u32(&vgm, 0x08), 0x0000_0150, "version 1.50");
        assert_eq!(read_le_u32(&vgm, 0x34), 0x0C, "data offset (0x40)");
        assert_eq!(
            read_le_u32(&vgm, 0x04),
            (vgm.len() - 0x04) as u32,
            "EoF offset relative to 0x04"
        );

        // The command stream (starts at 0x40): 0x52 0x28 0xF0, then 0x50 0x9F, then 0x66.
        let data = &vgm[0x40..];
        assert_eq!(
            data,
            &[CMD_YM2612_PORT0, 0x28, 0xF0, CMD_PSG, 0x9F, CMD_END]
        );
        assert_eq!(*data.last().unwrap(), CMD_END, "ends with 0x66");
        // No frame boundary crossed → total samples 0.
        assert_eq!(read_le_u32(&vgm, 0x18), 0, "no waits, zero total samples");
    }

    fn read_le_u32(buf: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
    }
}
