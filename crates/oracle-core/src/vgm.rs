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
//! - **Sub-frame timing (SY-4b, opt-in):** [`VgmLogger::with_subframe_waits`] additionally captures each
//!   record's absolute master-clock via the SY-4a [`on_event_at`](BusEventSink::on_event_at) seam. In that
//!   mode the renderer converts each mclk to an absolute 44100 Hz sample (`mclk * 735 / MCLK_PER_FRAME`) and
//!   emits the sample delta between consecutive records as `0x61 nn nn` waits (splitting on 65535), so writes
//!   land at their true intra-frame sample rather than being quantized to frame start. The default `new()`
//!   logger is unchanged and byte-identical.

use crate::bus::{BusEvent, BusEventSink, BusOp};
use crate::system::MCLK_PER_FRAME;

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
const CMD_WAIT: u8 = 0x61; // wait N samples: `0x61 nn nn` (little-endian u16)
const CMD_WAIT_FRAME: u8 = 0x62; // wait 735 samples (1/60 s)
const CMD_END: u8 = 0x66; // end of sound data

/// Largest sample count expressible in one `0x61 nn nn` command.
const WAIT_MAX: u64 = 0xFFFF;

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
    /// Absolute master-clock (mclk) per record, in lockstep with `records` (SY-4b sub-frame timing). Captured
    /// from the [`on_event_at`](BusEventSink::on_event_at) seam; frame-derived (`frame * MCLK_PER_FRAME`) for
    /// untimed callers that use the plain `on_event` path.
    mclks: Vec<u64>,
    /// The mclk of the write currently being decoded, staged by `on_event_at` and consumed by `on_event`.
    /// `None` when reached via the untimed `on_event` path (then the frame-derived mclk is used).
    pending_mclk: Option<u64>,
    /// Whether `render_vgm` emits sub-frame `0x61` waits (opt-in, [`with_subframe_waits`](Self::with_subframe_waits))
    /// instead of the default frame-bucketed `0x62` waits.
    subframe: bool,
    /// Status counter: completed YM2612 register writes.
    fm_writes: u64,
    /// Status counter: SN76489 byte writes.
    psg_writes: u64,
}

impl VgmLogger {
    /// A fresh logger: all latches/counters zero, no records. Renders **frame-bucketed** VGM (one `0x62` per
    /// frame boundary) — the default, byte-identical to the RT-3 goldens.
    pub fn new() -> Self {
        Self {
            fm_addr_latch: [0; 2],
            psg_latch: 0,
            frame: 0,
            records: Vec::new(),
            mclks: Vec::new(),
            pending_mclk: None,
            subframe: false,
            fm_writes: 0,
            psg_writes: 0,
        }
    }

    /// A logger that renders **sub-frame-accurate** VGM: each record is placed at its true intra-frame sample
    /// (derived from the absolute mclk captured via the SY-4a `on_event_at` seam), emitting `0x61` sample-delta
    /// waits between consecutive records. Decode/records are identical to [`new`](Self::new); only `render_vgm`
    /// differs. Use with `run_frames_with_sink` on a real machine so the timed emission sites feed real mclks.
    pub fn with_subframe_waits() -> Self {
        Self {
            subframe: true,
            ..Self::new()
        }
    }

    /// The decoded register-write records, in arrival order.
    pub fn records(&self) -> &[VgmRecord] {
        &self.records
    }

    /// The absolute master-clock (mclk) of each record, in lockstep with [`records`](Self::records). Populated
    /// from the SY-4a `on_event_at` seam (frame-derived for untimed callers); the timing source `render_vgm`
    /// uses in sub-frame mode, exposed for callers that want the raw sub-frame write timeline.
    pub fn mclks(&self) -> &[u64] {
        &self.mclks
    }

    /// Push a record and its absolute mclk together, keeping `records` and `mclks` in lockstep.
    fn push_record(&mut self, r: VgmRecord, mclk: u64) {
        self.records.push(r);
        self.mclks.push(mclk);
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
        self.mclks.clear();
        self.pending_mclk = None;
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
        if self.subframe {
            // Sub-frame mode (SY-4b): place each record at its absolute 44100 Hz sample derived from the
            // record's own mclk, and emit the sample delta between consecutive records as `0x61` waits.
            let mut prev_sample: Option<u64> = None;
            for (i, r) in self.records.iter().enumerate() {
                let sample = self.mclks[i] * SAMPLES_PER_FRAME as u64 / MCLK_PER_FRAME;
                if let Some(ps) = prev_sample {
                    // `saturating_sub`: cross-master interleaving (Z80 frontier lags the 68k) can present a
                    // record whose mclk dips below its predecessor's; treat that as a zero-length wait rather
                    // than winding time backwards.
                    let mut delta = sample.saturating_sub(ps);
                    while delta > 0 {
                        let chunk = delta.min(WAIT_MAX);
                        cmds.push(CMD_WAIT);
                        cmds.extend_from_slice(&(chunk as u16).to_le_bytes());
                        total_samples += chunk as u32;
                        delta -= chunk;
                    }
                }
                prev_sample = Some(sample);
                push_write(&mut cmds, r);
            }
        } else {
            // Default frame-bucketed mode (RT6): one 0x62 at each frame boundary. No wait precedes the first
            // record. Byte-identical to the RT-3 goldens.
            let mut prev_frame: Option<u64> = None;
            for r in &self.records {
                if let Some(pf) = prev_frame {
                    if r.frame != pf {
                        cmds.push(CMD_WAIT_FRAME);
                        total_samples += SAMPLES_PER_FRAME;
                    }
                }
                prev_frame = Some(r.frame);
                push_write(&mut cmds, r);
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

/// Append one record's VGM write command — `0x52`/`0x53 aa dd` for the two YM2612 ports, `0x50 dd` for the
/// PSG. Shared by both the frame-bucketed and sub-frame render paths so the command encoding never diverges.
fn push_write(cmds: &mut Vec<u8>, r: &VgmRecord) {
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

impl BusEventSink for VgmLogger {
    fn on_step_boundary(&mut self, _pc: u32, frame: u64) {
        self.frame = frame;
    }

    fn on_event(&mut self, e: BusEvent) {
        if e.op != BusOp::Write {
            return;
        }
        // The write's absolute mclk: from `on_event_at` if timed, else frame-derived (frame start) for untimed
        // callers — the latter keeps sub-frame deltas within a frame at zero, matching frame-bucketed semantics.
        let mclk = self
            .pending_mclk
            .take()
            .unwrap_or(self.frame * MCLK_PER_FRAME);
        let value = e.value as u8;
        // Classify on `addr` ALONE (fc-agnostic, RT3): the Z80 window and the 68k window fold into one chip.
        match e.addr {
            // FM bank-0 address latch — no record.
            0x4000 | 0xA0_4000 => self.fm_addr_latch[0] = value,
            // FM bank-0 data — completes a triple.
            0x4001 | 0xA0_4001 => {
                self.push_record(
                    VgmRecord {
                        chip: SoundChip::Ym2612,
                        port: 0,
                        reg: self.fm_addr_latch[0],
                        value,
                        frame: self.frame,
                    },
                    mclk,
                );
                self.fm_writes += 1;
            }
            // FM bank-1 address latch — no record.
            0x4002 | 0xA0_4002 => self.fm_addr_latch[1] = value,
            // FM bank-1 data — completes a triple.
            0x4003 | 0xA0_4003 => {
                self.push_record(
                    VgmRecord {
                        chip: SoundChip::Ym2612,
                        port: 1,
                        reg: self.fm_addr_latch[1],
                        value,
                        frame: self.frame,
                    },
                    mclk,
                );
                self.fm_writes += 1;
            }
            // PSG: self-describing byte. A `bit7=1` byte latches the channel/type selector; every byte records.
            0x7F11 | 0xC0_0011 => {
                if value & 0x80 != 0 {
                    self.psg_latch = (value >> 4) & 0x07;
                }
                self.push_record(
                    VgmRecord {
                        chip: SoundChip::Psg,
                        port: 0,
                        reg: self.psg_latch,
                        value,
                        frame: self.frame,
                    },
                    mclk,
                );
                self.psg_writes += 1;
            }
            _ => {}
        }
    }

    /// Timestamped delivery (SY-4a seam): stage the absolute mclk, then dispatch through the shared decode.
    /// `on_event` consumes the staged mclk; address-latch writes that produce no record discard it, and the
    /// next timed write re-stages its own — so `mclks` stays in exact lockstep with `records`.
    fn on_event_at(&mut self, e: BusEvent, mclk: u64) {
        self.pending_mclk = Some(mclk);
        self.on_event(e);
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

    /// Feed a fixed write sequence through the SY-4a `on_event_at` seam with a variety of mclks into a DEFAULT
    /// (frame-bucketed) logger, and prove the render is byte-identical to feeding the SAME sequence untimed via
    /// `on_event`. Pins that the default logger ignores mclk entirely — the RT-3 goldens cannot move.
    #[test]
    fn default_mode_is_byte_identical_regardless_of_mclk() {
        // Timed path: writes carry real, wildly different mclks (mid-frame and cross-frame).
        let mut timed = VgmLogger::new();
        timed.on_step_boundary(0, 0);
        timed.on_event_at(write_event(0x4000, 0x28), 0);
        timed.on_event_at(write_event(0x4001, 0xF0), 123_456);
        timed.on_event_at(write_event(0x7F11, 0x9F), 500_000);
        timed.on_step_boundary(0, 1);
        timed.on_event_at(write_event(0x4000, 0x30), MCLK_PER_FRAME + 77);
        timed.on_event_at(write_event(0x4001, 0x11), MCLK_PER_FRAME + 800_000);

        // Untimed path: identical decode, frame stamps only.
        let mut untimed = VgmLogger::new();
        untimed.on_step_boundary(0, 0);
        untimed.on_event(write_event(0x4000, 0x28));
        untimed.on_event(write_event(0x4001, 0xF0));
        untimed.on_event(write_event(0x7F11, 0x9F));
        untimed.on_step_boundary(0, 1);
        untimed.on_event(write_event(0x4000, 0x30));
        untimed.on_event(write_event(0x4001, 0x11));

        assert_eq!(
            timed.render_vgm(),
            untimed.render_vgm(),
            "default mode must ignore mclk — byte-identical to the untimed frame-bucketed render"
        );
        // And the render still carries frame-bucketed 0x62 waits (one frame boundary), no 0x61.
        let vgm = timed.render_vgm();
        assert_eq!(vgm.iter().filter(|&&b| b == CMD_WAIT_FRAME).count(), 1);
        assert_eq!(vgm.iter().filter(|&&b| b == CMD_WAIT).count(), 0);
    }

    #[test]
    fn subframe_mode_emits_mclk_derived_sample_deltas() {
        // Absolute sample = mclk * 735 / 896_040. Chosen mclks (two per frame, across a frame boundary):
        //   A: mclk 0                    → sample 0
        //   B: mclk 122_000             → 122_000*735/896_040 = 100
        //   C: mclk 896_040 (= 1 frame) → 735
        //   D: mclk 1_018_040          → 835
        // Deltas between consecutive records: 100, 635, 100  → total 835 samples.
        let mut log = VgmLogger::with_subframe_waits();
        log.on_step_boundary(0, 0);
        log.on_event_at(write_event(0x4000, 0x28), 0); // latch (no record)
        log.on_event_at(write_event(0x4001, 0xF0), 0); // A
        log.on_event_at(write_event(0x4000, 0x30), 122_000); // latch
        log.on_event_at(write_event(0x4001, 0x11), 122_000); // B
        log.on_step_boundary(0, 1);
        log.on_event_at(write_event(0x7F11, 0x9F), MCLK_PER_FRAME); // C (PSG)
        log.on_event_at(write_event(0x4000, 0x22), 1_018_040); // latch
        log.on_event_at(write_event(0x4001, 0x33), 1_018_040); // D

        let vgm = log.render_vgm();
        let data = &vgm[0x40..];
        // Expected command stream: A, wait 100, B, wait 635, C, wait 100, D, end.
        #[rustfmt::skip]
        let expected: &[u8] = &[
            CMD_YM2612_PORT0, 0x28, 0xF0,        // A
            CMD_WAIT, 100, 0,                    // 100 samples
            CMD_YM2612_PORT0, 0x30, 0x11,        // B
            CMD_WAIT, 0x7B, 0x02,                // 635 samples (0x027B)
            CMD_PSG, 0x9F,                       // C
            CMD_WAIT, 100, 0,                    // 100 samples
            CMD_YM2612_PORT0, 0x22, 0x33,        // D
            CMD_END,
        ];
        assert_eq!(data, expected, "sub-frame 0x61 sample-delta stream");
        assert_eq!(
            read_le_u32(&vgm, 0x18),
            835,
            "header total samples = sum of 0x61 waits (100+635+100)"
        );
    }

    #[test]
    fn subframe_mode_splits_gaps_over_65535_samples() {
        // A gap whose sample delta exceeds one 0x61's u16 range must split into multiple 0x61 commands that
        // sum to the exact delta. Second write at a large mclk: 200_000_000*735/896_040 = 164_054 samples.
        let big_mclk: u64 = 200_000_000;
        let expected_delta = big_mclk * SAMPLES_PER_FRAME as u64 / MCLK_PER_FRAME;
        assert!(expected_delta > WAIT_MAX, "test needs a >65535 gap");

        let mut log = VgmLogger::with_subframe_waits();
        log.on_event_at(write_event(0x4001, 0xF0), 0); // record at sample 0
        log.on_event_at(write_event(0x4003, 0x11), big_mclk); // record at the big sample

        let vgm = log.render_vgm();
        // Sum all 0x61 wait payloads and confirm they equal the delta, split into <=65535 chunks.
        let data = &vgm[0x40..];
        let mut i = 0;
        let mut sum: u64 = 0;
        let mut chunks = 0;
        while i < data.len() {
            match data[i] {
                CMD_WAIT => {
                    let n = u16::from_le_bytes([data[i + 1], data[i + 2]]) as u64;
                    assert!(n <= WAIT_MAX);
                    sum += n;
                    chunks += 1;
                    i += 3;
                }
                CMD_YM2612_PORT0 | CMD_YM2612_PORT1 => i += 3,
                CMD_PSG => i += 2,
                CMD_END => break,
                other => panic!("unexpected command byte {other:#04x}"),
            }
        }
        assert_eq!(sum, expected_delta, "split waits sum to the exact delta");
        assert!(chunks >= 3, "164_054 samples splits into >=3 chunks");
        assert_eq!(read_le_u32(&vgm, 0x18), expected_delta as u32);
    }
}
