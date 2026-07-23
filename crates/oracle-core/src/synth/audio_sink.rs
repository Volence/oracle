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
use crate::system::MCLK_PER_FRAME;
use std::collections::BTreeMap;

/// The canonical output sample rate for SY-1 (Hz).
pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;

/// Post-mix PSG gain in Q15 — **mix knob (by-ear tunable).** The SN76489
/// [`VOL_TABLE`](crate::synth::sn76489) peaks at 4000/channel and SY-1/SY-2 summed the PSG into the mix at
/// that full amplitude with **no scale**, which made the PSG (tones *and* the noise/cymbal channel) far too
/// loud next to the FM anchor in real playback. This is the third per-chip mix-level knob, symmetric with
/// [`FM_LEVEL_Q15`](crate::synth::ym2612_synth) and `DAC_SCALE`: it multiplies the summed PSG sample before
/// it enters the mix. Calibrated so our per-chip RMS matches the vgm2wav reference **PSG:FM ≈ 0.28** ratio
/// (a full-scale PSG tone was ~3.5× too loud relative to FM at unity; `9416/32768 ≈ 0.287` brings it in).
const PSG_LEVEL_Q15: i64 = 9_416;

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
    /// Current (in-progress) frame index — the frame an untimed [`Self::on_event`] write is attributed to.
    /// Tracks the most recent boundary frame.
    cur_frame: u64,
    /// Per-frame write buckets: `frame → (intra-frame sample, write)`. [`Self::on_event_at`] buckets each
    /// write by its OWN derived frame (overshoot-safe): a Z80 write a few ticks past a boundary carries
    /// frame `f+1` and waits in bucket `f+1` until that frame renders, rather than landing in the frame
    /// currently being flushed. [`Self::on_step_boundary`] drains every bucket `< frame`, so the map holds
    /// only 1-2 open frames.
    pending: BTreeMap<u64, Vec<(u32, BusEvent)>>,
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
            cur_frame: 0,
            pending: BTreeMap::new(),
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

    /// Turn an absolute master-clock into a `(frame, intra-frame sample)` pair (design §3.2). Integer math
    /// from the write's own mclk — the derived `frame` agrees with the run loop's boundary stamp
    /// (`scheduler.now() / MCLK_PER_FRAME`) by construction. The top-boundary clamp mirrors the DAC clamp.
    fn frame_and_sample(&self, mclk: u64) -> (u64, u32) {
        let frame = mclk / MCLK_PER_FRAME;
        let sample = ((mclk % MCLK_PER_FRAME) * self.samples_per_frame as u64 / MCLK_PER_FRAME)
            .min(self.samples_per_frame as u64 - 1) as u32;
        (frame, sample)
    }

    /// Enqueue a write into its frame's bucket at its intra-frame sample. Non-writes are dropped (only the
    /// chip-write stream is synthesized). Keyed by the write's OWN `frame`, so overshoot writes wait for the
    /// correct frame's render (design §3.3).
    fn enqueue(&mut self, frame: u64, sample: u32, e: BusEvent) {
        if e.op != BusOp::Write {
            return;
        }
        self.pending.entry(frame).or_default().push((sample, e));
    }

    /// Apply one write's register effect to the live chip state — the classification formerly in `on_event`,
    /// now invoked at the write's intra-frame sample from [`Self::render_frame`]. `$2A` DAC data is the one
    /// exception: it is placed at its true sample by [`Ym2612Synth::begin_frame`]'s ZOH track (queued in the
    /// render pre-pass), so it is skipped here.
    fn apply_write(&mut self, e: BusEvent) {
        let value = e.value as u8;
        // Classify on `addr` alone (fc-agnostic), exactly as the VgmLogger does — same source of truth.
        match e.addr {
            // SN76489 PSG (Z80 window $7F11, 68k window $C00011): one self-describing byte.
            0x7F11 | 0xC0_0011 => self.psg.write(value),
            // YM2612 FM, latch-then-data per bank (Z80 $4000-$4003 / 68k $A04000-$A04003). Even ports latch
            // the register number; odd ports complete a `(bank, reg, value)` write into the FM synth.
            0x4000 | 0xA0_4000 => self.fm_addr_latch[0] = value,
            0x4001 | 0xA0_4001 => {
                let reg = self.fm_addr_latch[0];
                // $2A DAC data is placed at its true sample via begin_frame's ZOH track, not applied here.
                if reg != 0x2A {
                    self.fm.write(0, reg, value);
                }
            }
            0x4002 | 0xA0_4002 => self.fm_addr_latch[1] = value,
            0x4003 | 0xA0_4003 => self.fm.write(1, self.fm_addr_latch[1], value),
            _ => {}
        }
    }

    /// Render one NTSC video frame worth of audio (`samples_per_frame` stereo samples), consuming `frame`'s
    /// write bucket. Instead of applying every write at the frame boundary (SY-3), the writes are walked in
    /// `sample` order and each one's register effect fires as the per-sample loop reaches its sample.
    fn render_frame(&mut self, frame: u64) {
        let spf = self.samples_per_frame;
        let mut bucket = self.pending.remove(&frame).unwrap_or_default();
        // Writes arrive mostly-sorted (monotone mclk); a stable sort by sample makes the per-sample walk
        // exact regardless of any cross-master (68k/Z80) interleaving within the frame.
        bucket.sort_by_key(|&(sample, _)| sample);

        // Pre-pass: extract this frame's DAC ($2A) data writes with their true intra-frame sample and queue
        // them as (sample, byte) pairs. The register latch is replayed on a scratch copy (the real latch is
        // advanced only in the apply loop below) so we know which odd-port data writes carry DAC data.
        let mut latch = self.fm_addr_latch;
        for &(sample, e) in &bucket {
            let v = e.value as u8;
            match e.addr {
                0x4000 | 0xA0_4000 => latch[0] = v,
                0x4002 | 0xA0_4002 => latch[1] = v,
                0x4001 | 0xA0_4001 if latch[0] == 0x2A => self.fm.queue_dac(sample, v),
                _ => {}
            }
        }
        // Snapshot the DAC ZOH track for this frame from the queued pairs (SY-4b true placement).
        self.fm.begin_frame(spf);

        let mut wi = 0usize;
        for i in 0..spf {
            // Apply every write scheduled at or before this sample, then generate the sample.
            while wi < bucket.len() && bucket[wi].0 <= i {
                self.apply_write(bucket[wi].1);
                wi += 1;
            }
            // PSG is mono on the Genesis → the same value feeds both output channels. Scale by the
            // PSG_LEVEL_Q15 mix knob (symmetric with the FM/DAC knobs) to hit the vgm2wav PSG:FM balance.
            let psg = ((self.psg.next_sample() as i64 * PSG_LEVEL_Q15) >> 15) as i32;
            // FM carries its own stereo pan.
            let (fm_l, fm_r) = self.fm.next_sample();
            let l = (psg + fm_l).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let r = (psg + fm_r).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            self.out.push(l);
            self.out.push(r);
        }
        // The sample clamp guarantees every write has `sample < spf`, so all were applied above; drain any
        // residual defensively so no write is silently lost.
        while wi < bucket.len() {
            self.apply_write(bucket[wi].1);
            wi += 1;
        }
    }

    /// Flush the final in-progress frame after a run completes.
    ///
    /// Rendering happens *at* frame boundaries, so the writes bucketed during the last frame of a run are
    /// not otherwise rendered (no boundary follows them). The `synth_render` example calls this once so an
    /// N-frame run yields ~N frames of audio.
    pub fn finish(&mut self) {
        if let Some(frame) = self.last_frame {
            self.render_frame(frame);
        }
    }
}

impl BusEventSink for AudioSink {
    /// The timestamped path the real 68k/Z80 buses call (SY-4b). Derive the write's frame + intra-frame
    /// sample from its absolute mclk and bucket it by its OWN frame — the writes are applied at their true
    /// sample when that frame renders, not batched at the boundary.
    fn on_event_at(&mut self, e: BusEvent, mclk: u64) {
        if e.op != BusOp::Write {
            return;
        }
        let (frame, sample) = self.frame_and_sample(mclk);
        self.enqueue(frame, sample, e);
    }

    /// Untimed fallback for direct callers with no timestamp (e.g. unit tests). Attributes the write to the
    /// current frame at sample 0 — behaviorally the SY-3 frame-batched semantics. Real runs go through
    /// [`Self::on_event_at`] and never hit this.
    fn on_event(&mut self, e: BusEvent) {
        self.enqueue(self.cur_frame, 0, e);
    }

    fn on_step_boundary(&mut self, _pc: u32, frame: u64) {
        match self.last_frame {
            None => {
                self.last_frame = Some(frame);
                self.cur_frame = frame;
            }
            Some(prev) if frame > prev => {
                // One or more video frames elapsed; render every frame strictly before the new one,
                // consuming each frame's write bucket at its true sub-frame timing. Buckets for the new
                // (not-yet-reached) frame stay queued.
                for f in prev..frame {
                    self.render_frame(f);
                }
                self.last_frame = Some(frame);
                self.cur_frame = frame;
            }
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

    /// SY-4b Test 1: absolute mclk → `(frame, intra-frame sample)`. Pins the boundaries, a hand-computed
    /// mid-frame value, the frame-index carry, and the top-of-frame clamp.
    #[test]
    fn mclk_maps_to_frame_and_sample() {
        let sink = AudioSink::new(44_100);
        assert_eq!(sink.samples_per_frame, 735);
        // Frame boundaries: offset 0 → sample 0 of that frame.
        assert_eq!(sink.frame_and_sample(0), (0, 0));
        assert_eq!(sink.frame_and_sample(MCLK_PER_FRAME), (1, 0));
        // Just under the next boundary → the last sample of frame 0 (734), and the clamp holds it there.
        let (tf, ts) = sink.frame_and_sample(MCLK_PER_FRAME - 1);
        assert_eq!(tf, 0);
        assert_eq!(ts, 734, "top of frame clamps to samples_per_frame - 1");
        // Hand-computed mid-frame value: offset * 735 / 896_040.
        let mid = MCLK_PER_FRAME / 2; // 448_020
        assert_eq!(
            sink.frame_and_sample(mid),
            (0, (mid * 735 / MCLK_PER_FRAME) as u32)
        );
        // Frame-2 offset carries the frame index with the same intra-frame math.
        let m = 2 * MCLK_PER_FRAME + 12_345;
        assert_eq!(
            sink.frame_and_sample(m),
            (2, (12_345u64 * 735 / MCLK_PER_FRAME) as u32)
        );
    }

    /// SY-4b Test 2 (overshoot): a write whose mclk is a few ticks past the frame-1 boundary carries frame 1
    /// and must render in frame 1, NOT in frame 0 while frame 0 is being flushed (design §3.3).
    #[test]
    fn overshoot_write_renders_in_its_own_frame() {
        let mut sink = AudioSink::new(44_100);
        sink.on_step_boundary(0, 0); // rendering position: frame 0
                                     // A Z80-style write just past the frame-1 boundary → derived frame 1, sample 0.
        let mclk = MCLK_PER_FRAME + 50;
        sink.on_event_at(write_event(0x7F11, 0x8E), mclk);
        sink.on_event_at(write_event(0x7F11, 0x0F), mclk);
        sink.on_event_at(write_event(0xC0_0011, 0x90), mclk);

        // Flush frame 0: the overshoot write is bucketed for frame 1, so frame 0 stays silent.
        sink.on_step_boundary(0, 1);
        assert_eq!(sink.samples().len(), 1470);
        assert!(
            sink.samples().iter().all(|&s| s == 0),
            "an overshoot write bucketed for frame 1 must not sound in the frame being flushed"
        );
        let after_f0 = sink.samples().len();

        // Flush frame 1: now the write renders.
        sink.on_step_boundary(0, 2);
        assert!(
            sink.samples()[after_f0..].iter().any(|&s| s != 0),
            "the overshoot write must render in frame 1, its own derived frame"
        );
    }

    /// SY-4b Test 3: non-decreasing mclk within a frame yields non-decreasing intra-frame sample indices.
    #[test]
    fn monotonic_mclk_yields_monotonic_sample() {
        let sink = AudioSink::new(44_100);
        let base = 3 * MCLK_PER_FRAME; // frame 3
        let step = MCLK_PER_FRAME / 735; // ~1219 mclk ≈ one output sample
        let mut prev = 0u32;
        for k in 0..735u64 {
            let (frame, sample) = sink.frame_and_sample(base + k * step);
            assert_eq!(frame, 3, "the swept mclk stays within frame 3");
            assert!(
                sample >= prev,
                "non-decreasing mclk must give non-decreasing sample ({sample} < {prev})"
            );
            prev = sample;
        }
        assert!(prev > 0, "the sample index must advance across the frame");
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
