//! `AudioSink` — a [`BusEventSink`] that synthesizes PCM audio from the live sound-chip write stream.
//!
//! This is the SY-1 pipeline seam. It rides the **exact same caller-owned
//! [`run_frames_with_sink`](crate::system::System::run_frames_with_sink) seam** the proven
//! [`VgmLogger`](crate::vgm::VgmLogger) uses, decodes the **identical** register-write triples
//! (`on_event`), and renders PCM at each **frame boundary** (`on_step_boundary`). Because it is
//! caller-owned it is never part of `System`, `state_hash`, or `export_state`, so it inherits the
//! VgmLogger's currency-neutrality for free — the null `()` / `run_frames` path is byte-untouched.
//!
//! Scope: **PSG + FM**. SY-1 added the PSG; SY-2 adds the minimal hand-rolled YM2612 (see
//! [`Ym2612Synth`]) and mixes it into the same render. Both chips are driven off the identical decoded
//! `(bank, reg, value)` write stream. Output is native `sample_rate` Hz stereo `i16` (the PSG is mono and
//! duplicated to both channels; the FM carries its own stereo pan), a fixed `sample_rate / 60` samples per
//! NTSC frame.

use crate::bus::{BusEvent, BusEventSink, BusOp};
use crate::synth::sn76489::Sn76489;
use crate::synth::ym2612_synth::Ym2612Synth;

/// The canonical output sample rate for SY-1 (Hz).
pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;

/// A [`BusEventSink`] that renders the machine's PSG register writes to interleaved stereo `i16` PCM.
pub struct AudioSink {
    /// Output sample rate (Hz).
    sample_rate: u32,
    /// Samples rendered per NTSC frame (`sample_rate / 60`).
    samples_per_frame: u32,
    /// The hand-rolled SN76489 synthesizer (SY-1).
    psg: Sn76489,
    /// The minimal hand-rolled YM2612 FM synthesizer (SY-2).
    fm: Ym2612Synth,
    /// Per-bank latched YM2612 register number (`[bank0, bank1]`) — the latch-then-data protocol, decoded
    /// exactly as the [`VgmLogger`](crate::vgm::VgmLogger) does.
    fm_addr_latch: [u8; 2],
    /// Interleaved L,R,L,R… output buffer.
    out: Vec<i16>,
    /// The last frame index seen on a step boundary; `None` until the first boundary.
    last_frame: Option<u64>,
}

impl AudioSink {
    /// A fresh sink producing `sample_rate` Hz stereo `i16`. Use [`DEFAULT_SAMPLE_RATE`] for 44.1 kHz.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            samples_per_frame: sample_rate / 60,
            psg: Sn76489::new(sample_rate),
            fm: Ym2612Synth::new(sample_rate),
            fm_addr_latch: [0; 2],
            out: Vec::new(),
            last_frame: None,
        }
    }

    /// The output sample rate (Hz).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The rendered PCM so far, interleaved L,R,L,R… (borrow; does not clear).
    pub fn samples(&self) -> &[i16] {
        &self.out
    }

    /// Take the rendered PCM and clear the internal buffer (for a streaming frontend that pulls per
    /// callback). Returns interleaved L,R,L,R… samples.
    pub fn drain(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.out)
    }

    /// Number of **stereo frames** (L+R pairs) rendered so far.
    pub fn len_frames(&self) -> usize {
        self.out.len() / 2
    }

    /// Render one NTSC video frame worth of audio (`samples_per_frame` stereo samples) from the current
    /// chip state, appending to the output buffer.
    fn render_frame(&mut self) {
        // Snapshot this frame's queued DAC ($2A) bytes for even-spread ZOH playback across the frame's
        // samples (SY-3a). Called once per frame, before the per-sample loop.
        self.fm.begin_frame(self.samples_per_frame);
        for _ in 0..self.samples_per_frame {
            // PSG is mono on the Genesis → the same value feeds both output channels.
            let psg = self.psg.next_sample() as i32;
            // FM carries its own stereo pan.
            let (fm_l, fm_r) = self.fm.next_sample();
            let l = (psg + fm_l).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let r = (psg + fm_r).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            self.out.push(l);
            self.out.push(r);
        }
    }

    /// Flush the final in-progress frame after a run completes.
    ///
    /// Rendering happens *at* frame boundaries, so the writes applied during the last frame of a run are
    /// not otherwise rendered (no boundary follows them). The `synth_render` example calls this once so an
    /// N-frame run yields ~N frames of audio.
    pub fn finish(&mut self) {
        if self.last_frame.is_some() {
            self.render_frame();
        }
    }
}

impl BusEventSink for AudioSink {
    fn on_step_boundary(&mut self, _pc: u32, frame: u64) {
        match self.last_frame {
            None => self.last_frame = Some(frame),
            Some(prev) if frame > prev => {
                // One or more video frames elapsed; the just-ended frame's writes are all applied, so
                // render it (and any wholly-silent frames skipped over) at the current chip state.
                for _ in prev..frame {
                    self.render_frame();
                }
                self.last_frame = Some(frame);
            }
            _ => {}
        }
    }

    fn on_event(&mut self, e: BusEvent) {
        if e.op != BusOp::Write {
            return;
        }
        let value = e.value as u8;
        // Classify on `addr` alone (fc-agnostic), exactly as the VgmLogger does — same source of truth.
        match e.addr {
            // SN76489 PSG (Z80 window $7F11, 68k window $C00011): one self-describing byte.
            0x7F11 | 0xC0_0011 => self.psg.write(value),
            // YM2612 FM, latch-then-data per bank (Z80 $4000-$4003 / 68k $A04000-$A04003). Even ports latch
            // the register number; odd ports complete a `(bank, reg, value)` write into the FM synth.
            0x4000 | 0xA0_4000 => self.fm_addr_latch[0] = value,
            0x4001 | 0xA0_4001 => self.fm.write(0, self.fm_addr_latch[0], value),
            0x4002 | 0xA0_4002 => self.fm_addr_latch[1] = value,
            0x4003 | 0xA0_4003 => self.fm.write(1, self.fm_addr_latch[1], value),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::Size;

    fn write_event(addr: u32, value: u8) -> BusEvent {
        BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr,
            size: Size::Byte,
            value: value as u32,
        }
    }

    /// A frame boundary renders exactly one frame of stereo audio; the buffer grows by
    /// `2 · samples_per_frame` per elapsed frame.
    #[test]
    fn frame_boundary_renders_one_frame_of_stereo() {
        let mut sink = AudioSink::new(44_100);
        assert_eq!(sink.samples_per_frame, 735);

        sink.on_step_boundary(0, 0); // first boundary: latch, no render
        assert_eq!(sink.samples().len(), 0);

        sink.on_step_boundary(0, 1); // one frame elapsed → 735 stereo samples = 1470 i16
        assert_eq!(sink.samples().len(), 1470);
        assert_eq!(sink.len_frames(), 735);
    }

    /// PSG writes routed through `on_event` reach the synth and produce non-silent audio.
    #[test]
    fn psg_writes_produce_audio() {
        let mut sink = AudioSink::new(44_100);
        sink.on_step_boundary(0, 0);
        // Program tone0 to ~440 Hz at full volume via the PSG port (both windows accepted).
        sink.on_event(write_event(0x7F11, 0x8E));
        sink.on_event(write_event(0x7F11, 0x0F));
        sink.on_event(write_event(0xC0_0011, 0x90));
        sink.on_step_boundary(0, 1);

        let pcm = sink.samples();
        assert_eq!(pcm.len(), 1470);
        assert!(
            pcm.iter().any(|&s| s != 0),
            "an audible tone was programmed but the frame was silent"
        );
        // Interleaved mono → L and R of each pair are identical.
        assert!(
            pcm.chunks_exact(2).all(|p| p[0] == p[1]),
            "PSG output must be duplicated identically to both stereo channels"
        );
    }

    /// FM register writes routed through `on_event` (the latch-then-data protocol across both windows)
    /// reach the FM synth and produce non-silent audio.
    #[test]
    fn fm_writes_produce_audio() {
        let mut sink = AudioSink::new(44_100);
        sink.on_step_boundary(0, 0);
        // Program channel 0, Op1, algorithm 7, a keyed carrier at ~440 Hz — via the latch-then-data ports
        // (bank 0: even = $4000 latch, odd = $4001 data), mixing Z80 and 68k windows to prove both fold in.
        let mut fm = |reg: u8, val: u8| {
            sink.on_event(write_event(0x4000, reg));
            sink.on_event(write_event(0x4001, val));
        };
        fm(0xB0, 0x07); // algorithm 7
        fm(0x30, 0x01); // MUL=1
        fm(0x40, 0x00); // TL=0
        fm(0x50, 0x1F); // AR=31
        fm(0xA4, 0x24); // block/fnum-hi
        fm(0xA0, 0x3B); // fnum low
        fm(0xB4, 0xC0); // pan both
                        // Key on via the 68k window ($A04000/$A04001) — same chip, different window.
        sink.on_event(write_event(0xA0_4000, 0x28));
        sink.on_event(write_event(0xA0_4001, 0x10));
        sink.on_step_boundary(0, 1);

        let pcm = sink.samples();
        assert_eq!(pcm.len(), 1470);
        assert!(
            pcm.iter().any(|&s| s != 0),
            "an FM carrier was keyed on but the frame was silent"
        );
    }

    /// `drain` returns the buffer and clears it.
    #[test]
    fn drain_takes_and_clears() {
        let mut sink = AudioSink::new(44_100);
        sink.on_step_boundary(0, 0);
        sink.on_step_boundary(0, 1);
        let taken = sink.drain();
        assert_eq!(taken.len(), 1470);
        assert_eq!(sink.samples().len(), 0);
    }
}
