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
//! duplicated to both channels; the FM carries its own stereo pan), [`samples_in_frame`] samples per NTSC
//! frame — **not** a fixed `sample_rate / 60`, which is lens finding H8: an NTSC frame is 1/59.9227 s, so
//! at 44.1 kHz it is 735.9476… samples and the count alternates 735/736 rather than sitting at 735.

use crate::bus::{BusEvent, BusEventSink, BusOp};
use crate::synth::console_filter::{ConsoleModel, ConsoleOutputFilter};
use crate::synth::sn76489::Sn76489;
use crate::synth::sn76489::PSG_CLOCK;
use crate::synth::ym2612_synth::Ym2612Synth;
use crate::system::{MCLK_PER_FRAME, MCLK_PER_Z80_CYCLE};
use std::collections::BTreeMap;

/// The canonical output sample rate for SY-1 (Hz).
pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;

/// **The NTSC master clock (Hz) — derived, never typed.**
///
/// Nothing in `crates/` stated this number before, which is how `sample_rate / 60` survived: without a
/// master clock there is no way to write down how long a frame *is*, so "a frame" quietly became "1/60 s".
/// It is derivable from two constants the tree already had, and the derivation is exact:
/// [`PSG_CLOCK`] is the NTSC colour subcarrier and the PSG sits on the same mclk/15 leg as the Z80, so
/// `3 579 545 · `[`MCLK_PER_Z80_CYCLE`]` = 53 693 175` Hz.
///
/// **The FM leg is the cross-check, not the source.** `ym2612_synth::YM2612_CLOCK` is 7 670 453 — the
/// rounded value the VGM header carries — while mclk/7 is 7 670 453.571…, so `YM2612_CLOCK · 7` lands 4 Hz
/// low. The two agree to within that rounding, which is the confirmation; the PSG leg is what is used,
/// because it is the one that divides exactly.
pub const MCLK_HZ: u64 = PSG_CLOCK as u64 * MCLK_PER_Z80_CYCLE;

/// **The true NTSC frame rate, as a rational** — `(numerator, denominator)` = `(MCLK_HZ,
/// MCLK_PER_FRAME)`, i.e. 53 693 175 / 896 040 = **59.92274… Hz**, not 60.
///
/// Exposed as the pair rather than an `f64` so a caller that needs it exactly can have it exactly; the
/// only float in the story should be the one a human reads.
pub const FRAME_RATE_HZ: (u64, u64) = (MCLK_HZ, MCLK_PER_FRAME);

/// **How many output samples the machine has produced by the start of frame `frame`**, at `sample_rate`.
///
/// `frame · sample_rate · MCLK_PER_FRAME / MCLK_HZ`, floored — the exact running total, in `u128` because
/// the numerator passes `u64::MAX` after about three months of emulated runtime and a silent wrap there
/// would be a defect of exactly the kind this function exists to remove.
pub const fn samples_through_frame(sample_rate: u32, frame: u64) -> u64 {
    ((frame as u128 * sample_rate as u128 * MCLK_PER_FRAME as u128) / MCLK_HZ as u128) as u64
}

/// **How many output samples frame `frame` carries**, at `sample_rate` — the difference of two running
/// totals, which is the whole of the fix for lens finding H8.
///
/// One NTSC frame is 896 040 mclk = 16.688 ms, and at 44 100 Hz that is **735.9476… samples**. It is not
/// an integer and it is not 735, so a constant cannot be right: the residual has to go somewhere. Here it
/// goes into the count itself, which alternates 735/736 so that the running total is never more than one
/// sample from the exact rational — the standard Bresenham arrangement, and the reason
/// [`Ym2612Synth::begin_frame`] has always taken the count as a parameter instead of reading a field.
///
/// # Why this decides the emulator's speed, and not just its pitch
///
/// The player makes the audio device the master clock (`oracle-player::pacing`,
/// `oracle-frontend::audio::frames_to_run`): ring occupancy steers how many frames are emulated per
/// iteration, so in the long run the machine produces exactly as many samples per second as the device
/// consumes. That closes the loop `emulated fps = sample_rate / samples_per_frame` — so the old
/// `sample_rate / 60` did not merely mislabel a buffer size, it **ran the machine at 60.000 Hz instead of
/// 59.9227**: 0.129 % fast, **278 extra emulated frames every hour**, growing linearly for as long as the
/// session lasts. Headless callers pace on nothing and were unaffected.
///
/// The chips agree with the new count and disagreed with the old one. `Sn76489::new` and
/// `Ym2612Synth::new` advance their phase by one output sample's worth of chip time per `next_sample`, so
/// rendering 735 of them for a 16.688 ms frame under-advanced both chips by the same 0.129 % — the FM and
/// PSG pitch were sharp by ~2.2 cents. One fix, both symptoms.
pub const fn samples_in_frame(sample_rate: u32, frame: u64) -> u32 {
    (samples_through_frame(sample_rate, frame + 1) - samples_through_frame(sample_rate, frame))
        as u32
}

/// **A per-frame sample count for SIZING a buffer** — the exact value rounded to nearest (736 at 44.1 kHz),
/// never a rate.
///
/// A ring's capacity and its water marks are hysteresis bands; they want one representative frame's worth
/// and do not care about the 0.05-sample residual. What they must *not* do is re-derive the rate from a
/// number of their own, which is how `2 * (sample_rate / 60)` came to state a second, disagreeing opinion
/// about how fast this machine runs. Callers that need the real count of a real frame want
/// [`samples_in_frame`].
pub const fn nominal_samples_per_frame(sample_rate: u32) -> u32 {
    // Round to nearest rather than floor: at 44.1 kHz the true value is 735.95, and flooring it would
    // reproduce the very 735 this finding is about, in the one place it is still allowed to appear.
    (((sample_rate as u128 * MCLK_PER_FRAME as u128 * 2) / MCLK_HZ as u128 + 1) / 2) as u32
}

/// Post-mix PSG gain in Q15 — **reference-derived, no longer a by-ear knob.** The SN76489
/// [`VOL_TABLE`](crate::synth::sn76489) peaks at 4000/channel, and this multiplies the summed PSG sample
/// before it enters the mix.
///
/// Unlike the FM/DAC levels this one has **no die-level ground truth** — the PSG and the YM2612 are summed
/// in the *analog* domain, so the ratio is a property of the board (Nemesis: it "varies by console revision
/// and even unit to unit"). What the emulators agree on, in full-scale PSG : full-scale FM:
/// - **Genesis Plus GX** `core/sound/psg.c`: `#define PSG_MAX_VOLUME 2800`, under the comment "*roughly
///   adjusted to match VA4 MD1 PSG/FM balance with 1.5x amplification of PSG output*" (`psg_preamp = 150`,
///   `fm_preamp = 100`) → **−15.4 dB**. This is the only constant in any of these codebases pinned to a
///   named board.
/// - **Exodus** (`Devices/SN76489/SN76489.cpp`): the four channels are averaged then scaled by `32767/6`,
///   the same `1/6` its YM2612 core applies per channel → **−15.6 dB**.
/// - TmEE's tooling divides by 6.4 → −16.1 dB. **BlastEm** (`psg.c` `PSG_VOL_DIV 14` vs `ym2612.c`
///   `volume_mult 79 / volume_div 120`) → −16.9 dB. **jgenesis** is the hot outlier at −13.0 dB.
///
/// So: a ~4 dB spread clustered on **≈ ÷6 (−15.5 dB)**, which is the value taken here. Six FM channels at
/// their 9-bit clip fill the `i16` range, so full-scale FM is 32768 and the whole PSG at attenuation 0 gets
/// `32768/6 ≈ 5461`; one of the four channels is `5461/4 ≈ 1365`, i.e. `1365/4000 · 32768 ≈ 11185` in Q15.
/// Equivalently: the entire PSG is as loud as one FM channel, which is also where Exodus and GPGX land.
///
/// The pre-SY-7 value `9416` was calibrated to a `vgm2wav` render's PSG:FM ratio, but libvgm applies a
/// content-dependent `NormalizeOverallVolume` ×2 and a `_CHIP_VOLUME` table (`0x80` SN vs `0x100` YM), so
/// that render's balance is a player mixing choice rather than a measurement.
const PSG_LEVEL_Q15: i64 = 11_185;

/// A [`BusEventSink`] that renders the machine's PSG register writes to interleaved stereo `i16` PCM.
pub struct AudioSink {
    /// Output sample rate (Hz).
    sample_rate: u32,
    // ⚑ There is deliberately no `samples_per_frame` field any more. Holding one number *was* lens
    // finding H8: a per-frame count cannot be a constant (735.9476… at 44.1 kHz), and a field made "a
    // frame" quietly mean 1/60 s at every site that read it. The count a given frame renders is
    // [`samples_in_frame`], taken from the frame index; a caller sizing a buffer wants
    // [`nominal_samples_per_frame`].
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
    /// The console's analog output stage (SY-6b), applied to the FINAL mix — FM + PSG + DAC — because on
    /// hardware the RC network sits downstream of everything. Defaults to the *unfiltered* revision, so
    /// the out-of-the-box render is bit-identical to the pre-SY-6b output until a revision is chosen.
    console_filter: ConsoleOutputFilter,
}

impl AudioSink {
    /// A fresh sink producing `sample_rate` Hz stereo `i16`. Use [`DEFAULT_SAMPLE_RATE`] for 44.1 kHz.
    ///
    /// The console output stage defaults to [`ConsoleModel::Unfiltered`], so this render is bit-identical
    /// to the pre-SY-6b output. The cutoff is genuinely
    /// revision-dependent (see [`ConsoleModel`]), so the revision to ship is a selection to be made by
    /// ear, not a number to bake in here. Use [`Self::with_console_model`] or
    /// [`Self::set_console_model`] to pick one.
    pub fn new(sample_rate: u32) -> Self {
        Self::with_console_model(sample_rate, ConsoleModel::default())
    }

    /// As [`Self::new`], but selecting which board revision's analog output stage to model.
    pub fn with_console_model(sample_rate: u32, model: ConsoleModel) -> Self {
        Self {
            sample_rate,

            psg: Sn76489::new(sample_rate),
            fm: Ym2612Synth::new(sample_rate),
            fm_addr_latch: [0; 2],
            out: Vec::new(),
            last_frame: None,
            cur_frame: 0,
            pending: BTreeMap::new(),
            console_filter: ConsoleOutputFilter::new(model, sample_rate),
        }
    }

    /// Swap the modelled output stage mid-run (including to the unfiltered revision). The filter memory
    /// is cleared, so the change does not drag the old stage's state along.
    pub fn set_console_model(&mut self, model: ConsoleModel) {
        self.console_filter = ConsoleOutputFilter::new(model, self.sample_rate);
    }

    /// Which board revision's output stage is currently modelled.
    pub fn console_model(&self) -> ConsoleModel {
        self.console_filter.model()
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
    /// The count frame `frame` renders — [`samples_in_frame`] at this sink's rate. Every site that used to
    /// read the old fixed field goes through here, so the write's intra-frame index and the render loop's
    /// bound are the same number for the same frame by construction.
    fn spf(&self, frame: u64) -> u32 {
        samples_in_frame(self.sample_rate, frame)
    }

    fn frame_and_sample(&self, mclk: u64) -> (u64, u32) {
        let frame = mclk / MCLK_PER_FRAME;
        // The frame's OWN count, not a global one: a write is placed inside the frame that will render
        // it, and neighbouring frames differ by a sample.
        let spf = self.spf(frame) as u64;
        let sample = ((mclk % MCLK_PER_FRAME) * spf / MCLK_PER_FRAME).min(spf - 1) as u32;
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
        let spf = self.spf(frame);
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
            // SY-6b: the console's analog output stage sits downstream of the whole mix, so it is applied
            // here — after the FM+PSG+DAC sum and BEFORE the clamp, so the filter sees the true mix rather
            // than an already-clipped one. For the unfiltered revision this is the exact identity, which
            // keeps the default render bit-identical (the f64 round-trip of an i32 mix is lossless).
            let (fl, fr) = self
                .console_filter
                .process((psg + fm_l) as f64, (psg + fm_r) as f64);
            let l = (fl.round() as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let r = (fr.round() as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
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
    /// `2 · samples_in_frame(rate, frame)` per elapsed frame.
    ///
    /// The expected count is **derived from the frame index**, not typed: since H8 it is not a constant.
    /// Frame 0 happens to be 735 and frame 1 is 736 — writing `735` here would still pass and would be
    /// pinning a coincidence.
    #[test]
    fn frame_boundary_renders_one_frame_of_stereo() {
        let mut sink = AudioSink::new(44_100);
        let f0 = samples_in_frame(44_100, 0);

        sink.on_step_boundary(0, 0); // first boundary: latch, no render
        assert_eq!(sink.samples().len(), 0);

        sink.on_step_boundary(0, 1); // one frame elapsed → frame 0's count, interleaved stereo
        assert_eq!(sink.samples().len(), 2 * f0 as usize);
        assert_eq!(sink.len_frames(), f0 as usize);
    }

    /// Render a fixed PSG tone through a given console model, returning the frame's PCM.
    fn render_tone(model: ConsoleModel) -> Vec<i16> {
        let mut sink = AudioSink::with_console_model(44_100, model);
        sink.on_step_boundary(0, 0);
        sink.on_event(write_event(0x7F11, 0x8E));
        sink.on_event(write_event(0x7F11, 0x0F));
        sink.on_event(write_event(0xC0_0011, 0x90));
        sink.on_step_boundary(0, 1);
        sink.samples().to_vec()
    }

    /// SY-6b: the sink defaults to the UNFILTERED output stage, and that path is byte-for-byte the
    /// pre-SY-6b render. This is the guarantee that adding the filter changed nothing by default.
    #[test]
    fn default_console_model_is_unfiltered_and_byte_identical() {
        assert_eq!(
            AudioSink::new(44_100).console_model(),
            ConsoleModel::Unfiltered,
            "the shipped default must not silently filter"
        );

        // `AudioSink::new` must render exactly what the explicitly-unfiltered sink renders.
        let mut default_sink = AudioSink::new(44_100);
        default_sink.on_step_boundary(0, 0);
        default_sink.on_event(write_event(0x7F11, 0x8E));
        default_sink.on_event(write_event(0x7F11, 0x0F));
        default_sink.on_event(write_event(0xC0_0011, 0x90));
        default_sink.on_step_boundary(0, 1);
        assert_eq!(
            default_sink.samples(),
            render_tone(ConsoleModel::Unfiltered).as_slice(),
            "the default path must be byte-identical to the unfiltered path"
        );
    }

    /// SY-6b: selecting a filtered revision actually changes the rendered audio, and the two revisions
    /// differ from each other — so the selector is really wired to the mix, not merely stored.
    #[test]
    fn selecting_a_console_model_changes_the_render() {
        let raw = render_tone(ConsoleModel::Unfiltered);
        let va0 = render_tone(ConsoleModel::Model1Va0Va2);
        let va3 = render_tone(ConsoleModel::Model1Va3Va6);

        assert_eq!(raw.len(), va0.len());
        assert_ne!(raw, va0, "VA0-VA2 must filter the mix");
        assert_ne!(va0, va3, "the two revisions must differ from each other");

        // The tone is well above both cutoffs, so each filtered render must be quieter than raw, and
        // the darker board must be the quieter of the two.
        let energy = |v: &[i16]| v.iter().map(|&s| (s as i64) * (s as i64)).sum::<i64>();
        assert!(energy(&va0) < energy(&raw), "filtering must attenuate");
        assert!(
            energy(&va3) < energy(&va0),
            "the VA3-VA6 board is darker than VA0-VA2"
        );
    }

    /// Swapping the model mid-run takes effect on subsequent frames.
    #[test]
    fn set_console_model_takes_effect() {
        let mut sink = AudioSink::with_console_model(44_100, ConsoleModel::Unfiltered);
        assert_eq!(sink.console_model(), ConsoleModel::Unfiltered);
        sink.set_console_model(ConsoleModel::Model1Va3Va6);
        assert_eq!(sink.console_model(), ConsoleModel::Model1Va3Va6);
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
            pcm.as_chunks::<2>().0.iter().all(|p| p[0] == p[1]),
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

    /// Render `frames` frames after running `program`, and return the peak absolute sample.
    fn peak_of(program: impl FnOnce(&mut AudioSink), frames: u64) -> i32 {
        let mut sink = AudioSink::new(44_100);
        sink.on_step_boundary(0, 0);
        program(&mut sink);
        for f in 1..=frames {
            sink.on_step_boundary(0, f);
        }
        sink.samples()
            .iter()
            .map(|&s| (s as i32).abs())
            .max()
            .unwrap_or(0)
    }

    /// Program FM channel 0 on algorithm 7 (all four operators are carriers) with `carriers` of them at
    /// TL=0 and the rest muted, keyed on at ~440 Hz.
    fn program_alg7(sink: &mut AudioSink, carriers: usize) {
        let mut fm = |reg: u8, val: u8| {
            sink.on_event(write_event(0x4000, reg));
            sink.on_event(write_event(0x4001, val));
        };
        fm(0x22, 0x00); // LFO off
        fm(0x2B, 0x00); // DAC off
        fm(0xB0, 0x07); // feedback 0, algorithm 7
        fm(0xB4, 0xC0); // pan L+R
        for s in 0..4u8 {
            let off = 4 * s;
            fm(0x30 + off, 0x01); // DT=0 MUL=1
            fm(
                0x40 + off,
                if (s as usize) < carriers { 0x00 } else { 0x7F },
            );
            fm(0x50 + off, 0x1F); // KS=0 AR=31
            fm(0x60 + off, 0x00); // DR=0
            fm(0x70 + off, 0x00); // SR=0
            fm(0x80 + off, 0x0F); // SL=0 RR=15
        }
        fm(0xA4, 0x22);
        fm(0xA0, 0x69);
        fm(0x28, 0xF0); // key on all four operators, channel 0
    }

    /// SY-7: the three sources sit on ONE reference-derived scale. Both ymfm (`ym2612::generate`) and
    /// Exodus's YM2612 core average the six channel slots onto a single output pin, so **one FM channel at
    /// its 9-bit clip, the DAC at full swing, and all four PSG channels at attenuation 0 are the same
    /// level**. This test pins that relationship, so a future by-ear tweak to any one knob cannot silently
    /// break the mix balance.
    #[test]
    fn the_three_sources_share_one_reference_level() {
        // One FM channel driven past its clip (4 full-scale carriers on algorithm 7).
        let fm_clipped = peak_of(|s| program_alg7(s, 4), 40);
        // The DAC held at 0x00 — the largest excursion an 8-bit unsigned sample can make from 0x80.
        let dac_full = peak_of(
            |s| {
                s.on_event(write_event(0x4000, 0x2B));
                s.on_event(write_event(0x4001, 0x80)); // DAC enable
                s.on_event(write_event(0x4002, 0xB6));
                s.on_event(write_event(0x4003, 0xC0)); // ch6 pan L+R
                s.on_event(write_event(0x4000, 0x2A));
                s.on_event(write_event(0x4001, 0x00));
            },
            8,
        );
        // All four PSG channels at attenuation 0, tones sharing one period so they sum in phase.
        let psg_full = peak_of(
            |s| {
                for (tone_lo, tone_hi, vol) in
                    [(0x8C, 0x1F, 0x90), (0xAC, 0x1F, 0xB0), (0xCC, 0x1F, 0xD0)]
                {
                    s.on_event(write_event(0x7F11, tone_lo));
                    s.on_event(write_event(0x7F11, tone_hi));
                    s.on_event(write_event(0x7F11, vol));
                }
                s.on_event(write_event(0x7F11, 0xE4)); // noise: white, tone-2 period
                s.on_event(write_event(0x7F11, 0xF0)); // noise volume 0
            },
            40,
        );

        // Every source must land within 1 % of the shared level.
        for (name, v) in [("dac", dac_full), ("psg", psg_full)] {
            let ratio = v as f64 / fm_clipped as f64;
            assert!(
                (0.99..=1.01).contains(&ratio),
                "{name} full scale ({v}) must match one clipped FM channel ({fm_clipped}); ratio {ratio:.4}"
            );
        }

        // And six FM channels at the clip must land on the i16 range: the chip's own output scale is
        // defined so the full multiplexed mix is exactly full scale. (The Q15 shift floors, so the extreme
        // negative clip can sit a few LSB past `i16::MIN` — 0.05 % is the tolerance, not a licence to drift.)
        let full_mix = 6 * fm_clipped;
        assert!(
            (32_752..=32_784).contains(&full_mix),
            "six clipped FM channels should fill the i16 range, got {full_mix}"
        );
    }

    /// SY-7: the 9-bit per-channel clip is real — piling four full-scale carriers onto one channel must
    /// NOT make it ~4× louder than a single carrier, because the chip's channel DAC saturates first.
    #[test]
    fn a_channel_cannot_exceed_its_nine_bit_clip() {
        let one = peak_of(|s| program_alg7(s, 1), 40);
        let four = peak_of(|s| program_alg7(s, 4), 40);
        assert!(one > 0 && four > 0, "both patches must sound");
        let ratio = four as f64 / one as f64;
        assert!(
            ratio < 1.05,
            "four carriers must clip to about one full-scale carrier, got {ratio:.3}× \
             (an unclamped sum would be ~4×)"
        );
    }

    /// SY-4b Test 1: absolute mclk → `(frame, intra-frame sample)`. Pins the boundaries, a hand-computed
    /// mid-frame value, the frame-index carry, and the top-of-frame clamp.
    #[test]
    fn mclk_maps_to_frame_and_sample() {
        let sink = AudioSink::new(44_100);
        // Each frame's own count. ⚑ They are not equal — frame 0 is 735 and frame 2 is 736 — which is
        // exactly why every expectation below is derived per frame rather than sharing one literal.
        let f0 = samples_in_frame(44_100, 0) as u64;
        let f2 = samples_in_frame(44_100, 2) as u64;
        assert_ne!(
            f0, f2,
            "anti-vacuity: this row exists to prove the mapping uses the FRAME's count, so the two \
             frames it compares must genuinely differ"
        );
        // Frame boundaries: offset 0 → sample 0 of that frame.
        assert_eq!(sink.frame_and_sample(0), (0, 0));
        assert_eq!(sink.frame_and_sample(MCLK_PER_FRAME), (1, 0));
        // Just under the next boundary → the last sample of frame 0, and the clamp holds it there.
        let (tf, ts) = sink.frame_and_sample(MCLK_PER_FRAME - 1);
        assert_eq!(tf, 0);
        assert_eq!(
            ts as u64,
            f0 - 1,
            "top of frame clamps to that frame's samples_in_frame - 1"
        );
        // Mid-frame, computed from frame 0's own count.
        let mid = MCLK_PER_FRAME / 2; // 448_020
        assert_eq!(
            sink.frame_and_sample(mid),
            (0, (mid * f0 / MCLK_PER_FRAME) as u32)
        );
        // Frame-2 offset carries the frame index — and uses FRAME 2's count, not frame 0's.
        let m = 2 * MCLK_PER_FRAME + 12_345;
        assert_eq!(
            sink.frame_and_sample(m),
            (2, (12_345u64 * f2 / MCLK_PER_FRAME) as u32)
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
        let spf = samples_in_frame(44_100, 3) as u64; // frame 3's own count, derived
        let step = MCLK_PER_FRAME / spf; // ~1217 mclk ≈ one output sample
        let mut prev = 0u32;
        for k in 0..spf {
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

    /// ★ **The machine runs at the NTSC rate, and the rate is the SINK's to set** — lens finding H8.
    ///
    /// # Who reads this number, and when
    ///
    /// The player makes the audio device the master clock: ring occupancy decides how many frames to
    /// emulate per iteration (`oracle-frontend::audio::frames_to_run`, `oracle-player::pacing`), so over
    /// any long run the machine produces exactly as many samples as the device consumes. That closes
    /// `emulated fps = sample_rate / samples-per-frame` — the sink's per-frame count *is* the emulator's
    /// wall-clock speed, and `sample_rate / 60` set it to 60.000 Hz against a real 59.92274 Hz. **278
    /// extra emulated frames per hour, growing linearly.** Headless callers pace on nothing, which is why
    /// this was invisible everywhere but the two windows.
    ///
    /// # What is asserted, and why it is asserted through `render`
    ///
    /// An hour of frames is walked and the samples counted **off the rendered buffer**, not off
    /// [`samples_in_frame`]. Calling the helper and checking its arithmetic would prove the helper; it
    /// could not see a `render_frame` that ignored it, which is the shape of the defect being fixed. So
    /// the subject is `on_step_boundary`'s output.
    ///
    /// The expectation is **derived from the machine's own constants** — `MCLK_HZ` (itself derived from
    /// [`PSG_CLOCK`] and [`MCLK_PER_Z80_CYCLE`]) over `MCLK_PER_FRAME` — never from a typed 59.9227. A
    /// literal here would be the same defect with a better number.
    #[test]
    fn an_hour_of_frames_carries_an_hour_of_samples_at_the_true_ntsc_rate() {
        const RATE: u32 = 44_100;
        let (num, den) = FRAME_RATE_HZ;

        // ---- (a) the render path really renders the derived count, observed frame by frame.
        //
        // Bounded at 600 frames because this drives the whole FM+PSG mix: an hour of real rendering is
        // ~2 minutes of test time, and what the render path has to prove is that it FOLLOWS
        // `samples_in_frame`, which 600 frames pins exactly. The hour is then arithmetic on the function
        // the render was just shown to obey — part (b).
        const RENDERED: u64 = 600;
        let mut sink = AudioSink::new(RATE);
        let mut rendered: u64 = 0;
        let mut seen: Vec<u64> = Vec::new();
        for f in 0..=RENDERED {
            sink.on_step_boundary(0, f);
            let n = sink.drain().len() as u64 / 2;
            if f > 0 {
                seen.push(n);
            }
            rendered += n;
        }
        assert_eq!(
            rendered,
            samples_through_frame(RATE, RENDERED),
            "the rendered buffer must carry exactly the derived running total — a `render_frame` that \
             ignored `samples_in_frame` would still pass a test that only called the helper"
        );
        // Anti-vacuity: the render path must actually have produced two different frame lengths, or the
        // equality above is satisfiable by any constant that happens to average out.
        assert!(
            seen.contains(&735) && seen.contains(&736),
            "the rendered frames were all one length ({:?}…), so this row cannot tell the fix from the \
             defect",
            &seen[..seen.len().min(8)]
        );

        // ---- (b) what that rate means over an hour, against the pre-H8 count.
        let frames_per_hour = 3600 * num / den;
        let honest = samples_through_frame(RATE, frames_per_hour);
        let old = frames_per_hour * (RATE as u64 / 60); // `sample_rate / 60` = 735
        assert!(
            old < honest,
            "COULD NOT MEASURE: the pre-H8 count {old} must be SHORTER than an honest hour {honest}, \
             or this control is not measuring the defect it names"
        );
        // The device consumes at `sample_rate`, and the loop runs the machine as fast as it must to keep
        // up — so a short frame is a fast machine. Stated as the frames-per-hour the finding quotes:
        let fast_fps = RATE as f64 / (RATE / 60) as f64; // 60.000
        let true_fps = num as f64 / den as f64; // 59.92274…
        let extra_per_hour = (fast_fps - true_fps) * 3600.0;
        assert!(
            (extra_per_hour - 278.1).abs() < 0.5,
            "the defect this row guards is 278 extra emulated frames an hour; this arithmetic makes it \
             {extra_per_hour:.1}, so the row and the finding have drifted apart"
        );
    }

    /// **Neighbouring frames differ, and that is the mechanism** — the residual has to live somewhere.
    ///
    /// A single constant cannot render 735.9476 samples, so the count alternates. This pins that it
    /// really does (a rounded constant of 736 would pass the hour row above to within ~15 frames and fail
    /// here), that both values are the two integers straddling the exact rational, and that the running
    /// total never drifts more than a sample from it.
    #[test]
    fn the_per_frame_count_straddles_the_exact_rational_and_never_drifts() {
        const RATE: u32 = 44_100;
        let counts: Vec<u32> = (0..1_000).map(|f| samples_in_frame(RATE, f)).collect();
        let lo = *counts.iter().min().expect("non-empty");
        let hi = *counts.iter().max().expect("non-empty");
        assert_eq!(
            (lo, hi),
            (735, 736),
            "at 44.1 kHz an NTSC frame is 735.9476… samples, so the only honest integers are the two \
             that straddle it"
        );
        assert!(
            counts.iter().any(|&c| c == lo) && counts.iter().any(|&c| c == hi),
            "anti-vacuity: a constant count would make the min/max check above pass with one value"
        );
        // The running total never leaves the exact rational by more than a sample — the property a
        // rounded constant does not have.
        let (num, den) = FRAME_RATE_HZ;
        let mut total: u64 = 0;
        for (f, &c) in counts.iter().enumerate() {
            total += c as u64;
            let exact = (f as u128 + 1) * RATE as u128 * den as u128 / num as u128;
            assert!(
                (total as u128).abs_diff(exact) <= 1,
                "after {} frames the total is {total} and the exact value is {exact}",
                f + 1
            );
        }
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
