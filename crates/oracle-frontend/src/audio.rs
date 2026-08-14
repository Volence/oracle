//! Phase SY-5a — the real-time-audio **substrate**: a lock-free SPSC ring buffer, `i16 → f32` conversion,
//! and a composite [`BusEventSink`] that drives the synth's [`AudioSink`] alongside the existing
//! pixel-attribution watch. This is the headless-testable half of Phase SY-5
//! (`docs/2026-07-23-phase-sy5-realtime-audio-design.md`, §2/§5/§6/§8 SY-5a).
//!
//! It deliberately does **not** open a host audio device — building and owning the `cpal` output stream
//! lives in `main.rs` (SY-5b). The callback's *body* is here as [`fill_output`] (it touches no cpal type, so
//! it stays unit-testable without a sound card). SY-5b consumes this substrate: `main()`'s loop drives
//! [`AudioAndWatch`] through
//! [`run_frames_with_sink`](oracle_core::system::System::run_frames_with_sink), drains each frame, and
//! [`push_frame`]s it into the [`AudioProd`] whose paired [`AudioCons`] is popped by the cpal callback.

use oracle_core::bus::Fanout;
use oracle_core::synth::AudioSink;
use oracle_core::watchpoints::Watchpoints;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};

/// Video frames of stereo audio the ring holds (its slack, in frames). ~4 frames keeps end-to-end latency
/// ~50–67 ms while absorbing a couple of late producer ticks (design §2.4). Retune by this one number.
pub const RING_FRAMES: usize = 4;

/// The ring element type: interleaved-stereo `f32` PCM (L,R,L,R…). Storing `f32` (not `i16`) makes the SY-5b
/// audio callback a pure `memcpy`; the `i16 → f32` conversion happens on the producer / non-real-time thread
/// (design §2.2).
pub type AudioRing = HeapRb<f32>;
/// Producer half of the SPSC ring — lives on the emulation (main) thread.
pub type AudioProd = HeapProd<f32>;
/// Consumer half of the SPSC ring — moves into SY-5b's cpal output callback.
pub type AudioCons = HeapCons<f32>;

/// Build the SPSC ring sized for [`RING_FRAMES`] video frames at `sample_rate` (interleaved stereo), and
/// [`Split`] it into a producer (main thread) and consumer (SY-5b's audio callback). Each frame is
/// `2 · (sample_rate / 60)` `f32` (interleaved stereo); capacity is floored at 2 so a degenerate rate still
/// yields a usable ring.
pub fn make_ring(sample_rate: u32) -> (AudioProd, AudioCons) {
    let per_frame = 2 * (sample_rate as usize / 60);
    let capacity = (RING_FRAMES * per_frame).max(2);
    AudioRing::new(capacity).split()
}

/// Convert one interleaved `i16` PCM sample to `f32` in `[-1.0, 1.0)`: `-32768 → -1.0`, `0 → 0.0`,
/// `32767 → ~0.99997` (design §2.2). Pure; no clamp needed — the synth already clamps to `i16` range at mix.
#[inline]
pub fn sample_i16_to_f32(s: i16) -> f32 {
    s as f32 / 32768.0
}

/// Number of volume steps the frontend's `-` / `=` keys move through. Step `VOLUME_STEPS` is unattenuated
/// output (the default) and step 0 is silence, so there are `VOLUME_STEPS + 1` settings in all.
pub const VOLUME_STEPS: u8 = 10;

/// The linear output gain for volume `step` (clamped to `0..=VOLUME_STEPS`) with the mute toggle `muted`.
/// Pure — this is the whole of the volume control's arithmetic, so it is testable without a sound card.
///
/// The curve is deliberately **linear** in amplitude (`step / VOLUME_STEPS`) rather than perceptual: it is
/// trivially auditable, exact at both ends (step 0 is true digital silence, full step is a bit-exact
/// pass-through of the synth's output), and the frontend is a development tool, not a mixing desk. Mute is
/// kept independent of the level so unmuting restores the level the user had chosen.
///
/// The result is always in `0.0..=1.0`, so applying it can only ever attenuate — [`push_frame`] cannot clip.
pub fn gain_for(step: u8, muted: bool) -> f32 {
    if muted {
        return 0.0;
    }
    f32::from(step.min(VOLUME_STEPS)) / f32::from(VOLUME_STEPS)
}

/// Convert an interleaved `i16` frame (from [`AudioSink::drain`]) at output gain `gain`, `push_slice` it into
/// the ring producer, and return the count of samples **dropped** on overrun. `push_slice` accepts fewer than
/// offered when the ring is full (design §2.5 overrun); the surplus is discarded — the producer **never**
/// blocks/spins waiting for the audio thread (that would stall video). Conversion is done here, on the
/// non-real-time producer thread.
///
/// `gain` comes from [`gain_for`] and is expected in `0.0..=1.0`; anything else (including a non-finite value)
/// is coerced into that range rather than trusted, because this is the last stop before the user's speakers.
/// Applying the gain on the *producer* side keeps the real-time callback a pure copy and needs no shared
/// atomic; the cost is that a volume change reaches the ears only once the ring's ~[`RING_FRAMES`] frames of
/// already-queued audio have played out (~67 ms — below the threshold where a key press feels laggy).
pub fn push_frame(prod: &mut AudioProd, pcm: &[i16], gain: f32) -> usize {
    let gain = if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let converted: Vec<f32> = pcm
        .iter()
        .copied()
        .map(|s| sample_i16_to_f32(s) * gain)
        .collect();
    let pushed = prod.push_slice(&converted);
    converted.len() - pushed
}

/// Rebuild `sink` at its current sample rate, discarding the frame clock and chip shadow it has accumulated.
///
/// **Mandatory after anything that moves the machine's clock backwards** — a soft reset, a ROM reload, or a
/// save-state load. [`AudioSink`] renders a video frame only when the frame index it is handed *advances*
/// past the highest it has seen ([`AudioSink::on_step_boundary`] ignores a frame `<=` the last one), and that
/// index is derived from the master clock (`scheduler.now() / MCLK_PER_FRAME`). A machine that restarts at
/// mclk 0 therefore hands the old sink frame 0, 1, 2… — all far below its stale high-water mark — and the
/// sink renders *nothing at all* until the counter climbs back, i.e. minutes of total silence.
///
/// Rebuilding also drops the synth's register shadow, which is the right call here for a second reason: after
/// a reset the real chip is reinitialised, so carrying yesterday's key-ons across would hold a note droning
/// until the driver happened to rewrite those registers.
pub fn resync_sink(sink: &mut AudioSink) {
    *sink = AudioSink::new(sink.sample_rate());
}

/// Fill one host output buffer from the ring — the body of the cpal callback, factored out so it is
/// unit-testable without a sound card (it touches no cpal type).
///
/// `flush` is the timeline-jump seam: loading a save state, soft-resetting, or reloading the ROM teleports
/// the machine to a different point in its timeline, so every sample still queued in the ring belongs to a
/// timeline that no longer exists. The emulation thread cannot drain the ring itself (it owns only the
/// producer half; `clear` is a consumer operation), so it raises this flag instead and the very next callback
/// discards the backlog. The buffer it was called for then underruns into silence — a sub-frame gap, versus
/// the ~[`RING_FRAMES`]-frame burp of stale audio you would otherwise hear. The frontend raises it from
/// `resync_audio`, alongside the [`resync_sink`] the same jump requires.
///
/// Channel handling matches the device: stereo = a straight interleaved copy; mono = average each L,R pair;
/// other counts = L,R into the first two lanes of each output frame, rest silent (design §3.3). Any shortfall
/// is zero-filled.
pub fn fill_output(cons: &mut AudioCons, out: &mut [f32], channels: usize, flush: &AtomicBool) {
    if flush.swap(false, Ordering::AcqRel) {
        cons.clear();
    }
    match channels {
        2 => {
            // Stereo: the ring is already interleaved L,R,L,R… — a pure copy, silence any shortfall.
            let n = cons.pop_slice(out);
            out[n..].fill(0.0);
        }
        1 => {
            // Mono: average each ring L,R pair into one sample; underrun → silence.
            for slot in out.iter_mut() {
                let mut lr = [0.0f32; 2];
                let got = cons.pop_slice(&mut lr);
                *slot = if got == 2 { (lr[0] + lr[1]) * 0.5 } else { 0.0 };
            }
        }
        ch => {
            // >2 (or a degenerate 0): L,R into the first two lanes of each output frame, rest silent.
            for out_frame in out.chunks_mut(ch.max(1)) {
                let mut lr = [0.0f32; 2];
                let got = cons.pop_slice(&mut lr);
                for (i, s) in out_frame.iter_mut().enumerate() {
                    *s = if i < got { lr[i] } else { 0.0 };
                }
            }
        }
    }
}

/// A composite [`BusEventSink`] that forwards **every** hook to the synth's [`AudioSink`] and, when present,
/// to the pixel-attribution [`Watchpoints`] — so real-time audio and the milestone-D3 watch tooling stay live
/// at the same time (design §6.2).
///
/// This used to be a hand-written struct with a hand-written seven-method `BusEventSink` impl. It is now the
/// *generic* [`Fanout`] combinator from the core, with the "only sometimes attached" watch expressed by the
/// core's `Option<S>` sink impl — same delivery order (audio first), same OR-composed `wants_*` queries, and
/// now the stop signal is composed too, for free. The name is kept because it says what the pair *is*.
///
/// Construct it with `AudioAndWatch::new(&mut audio_sink, watch_armed.then_some(&mut watchpoints))`; the two
/// halves are the public fields `a` (audio) and `b` (watch).
pub type AudioAndWatch<'a> = Fanout<&'a mut AudioSink, Option<&'a mut Watchpoints>>;

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::bus::{BusEvent, BusEventSink, BusOp, Size};
    use oracle_core::watchpoints::WatchOp;

    fn write_event(addr: u32, value: u8) -> BusEvent {
        BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr,
            size: Size::Byte,
            value: value as u32,
        }
    }

    /// Design §7 Test 1 — ring FIFO order + values, plus the underrun and overrun contracts (§2.5).
    #[test]
    fn ring_fifo_underrun_and_overrun() {
        // Capacity is exact so overrun is easy to force: RING_FRAMES=4 · 2 · (240/60=4) = 32 f32.
        let (mut prod, mut cons) = make_ring(240);
        assert_eq!(prod.push_slice(&[]), 0, "empty push is a no-op");

        // FIFO round-trip: push a known ramp, pop it, assert order + values.
        let ramp: Vec<f32> = (0..16).map(|i| i as f32).collect();
        assert_eq!(prod.push_slice(&ramp), 16);
        let mut out = [0.0f32; 16];
        assert_eq!(cons.pop_slice(&mut out), 16);
        assert_eq!(
            out.to_vec(),
            ramp,
            "samples come out in FIFO order, unchanged"
        );

        // Underrun: pop more than the ring holds → the caller zero-fills the shortfall (design §2.5).
        let mut under = [7.0f32; 8]; // sentinel; pop overwrites only the popped prefix
        let popped = cons.pop_slice(&mut under);
        assert_eq!(popped, 0, "ring is empty after the round-trip");
        for s in &mut under[popped..] {
            *s = 0.0; // the SY-5b callback fills the tail with silence
        }
        assert!(
            under.iter().all(|&s| s == 0.0),
            "underrun tail is 0.0 silence"
        );

        // Overrun: offer more than capacity → push_slice accepts < offered, surplus dropped, no panic.
        let cap = 32usize; // 4 frames · 2 · 4 samples/frame
        let big: Vec<f32> = (0..cap as i32 + 10).map(|i| i as f32).collect();
        let pushed = prod.push_slice(&big);
        assert!(
            pushed < big.len(),
            "overrun: push accepts fewer than offered"
        );
        assert_eq!(pushed, cap, "push fills exactly the free capacity");
        // The accepted prefix is intact and in order; the surplus 10 are simply gone.
        let mut drained = vec![0.0f32; cap];
        assert_eq!(cons.pop_slice(&mut drained), cap);
        assert_eq!(drained, big[..cap].to_vec());
    }

    /// Design §7 Test 2 — `i16 → f32` conversion end-points and range.
    #[test]
    fn i16_to_f32_conversion() {
        assert_eq!(sample_i16_to_f32(i16::MIN), -1.0); // -32768 → -1.0 exactly
        assert_eq!(sample_i16_to_f32(0), 0.0);
        let max = sample_i16_to_f32(i16::MAX); // 32767 → ~0.99997
        assert!(
            (max - 0.999_969_5).abs() < 1e-6,
            "max maps to ~0.99997, got {max}"
        );
        // Every representable i16 lands in [-1.0, 1.0].
        for s in [i16::MIN, -12345, -1, 0, 1, 12345, i16::MAX] {
            let f = sample_i16_to_f32(s);
            assert!((-1.0..=1.0).contains(&f), "{s} → {f} out of [-1.0, 1.0]");
        }
    }

    /// Design §7 Test 3 — producer smoke path: render frames into a persistent `AudioSink`, drain, convert,
    /// push into the ring, pop, and assert `2 · (rate/60)` f32 flow per rendered frame in FIFO order.
    #[test]
    fn producer_smoke_path_flows_samples() {
        let rate = 48_000u32;
        let per_frame = 2 * (rate as usize / 60); // 1600 f32 / frame
        let mut sink = AudioSink::new(rate);
        // A ring large enough to hold several frames without overrun for this test.
        let (mut prod, mut cons) = AudioRing::new(8 * per_frame).split();

        sink.on_step_boundary(0, 0); // first boundary: latch, renders nothing
                                     // Program an audible PSG tone so the frames are non-silent (proves real data flows, not just zeros).
        sink.on_event(write_event(0x7F11, 0x8E));
        sink.on_event(write_event(0x7F11, 0x0F));
        sink.on_event(write_event(0xC0_0011, 0x90));

        let mut total_popped = 0usize;
        let mut popped_any_nonzero = false;
        for f in 1..=3u64 {
            sink.on_step_boundary(0, f); // renders exactly one frame
            let pcm = sink.drain();
            assert_eq!(pcm.len(), per_frame, "one frame renders 2·(rate/60) i16");
            let dropped = push_frame(&mut prod, &pcm, gain_for(VOLUME_STEPS, false));
            assert_eq!(dropped, 0, "ring is sized to not overrun in this test");
            let mut out = vec![0.0f32; per_frame];
            let n = cons.pop_slice(&mut out);
            assert_eq!(n, per_frame, "every pushed sample pops back out");
            if out.iter().any(|&s| s != 0.0) {
                popped_any_nonzero = true;
            }
            total_popped += n;
        }
        assert_eq!(total_popped, 3 * per_frame, "three frames flow end-to-end");
        assert!(
            popped_any_nonzero,
            "the programmed tone must survive the ring as non-zero f32"
        );
    }

    /// [`fill_output`] without a flush request is the plain stereo copy + zero-filled underrun tail.
    #[test]
    fn fill_output_copies_stereo_and_silences_the_tail() {
        let (mut prod, mut cons) = make_ring(240); // capacity 32
        let flush = AtomicBool::new(false);
        assert_eq!(prod.push_slice(&[1.0, 2.0, 3.0, 4.0]), 4);

        let mut out = [-1.0f32; 8];
        fill_output(&mut cons, &mut out, 2, &flush);
        assert_eq!(
            out,
            [1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0],
            "the four queued samples copy through; the underrun tail is silence"
        );
        assert!(!flush.load(Ordering::Acquire), "no request, flag untouched");
    }

    /// **The save-state seam.** With the flush flag raised, the samples already queued from the pre-load
    /// timeline must be *discarded*, not played: the callback outputs silence and the ring is empty
    /// afterwards. Without this, a state load burps up to [`RING_FRAMES`] frames of stale audio.
    #[test]
    fn fill_output_flush_drops_the_stale_backlog() {
        let (mut prod, mut cons) = make_ring(240);
        let flush = AtomicBool::new(false);

        // Queue "old timeline" audio, then request a flush (what the main loop does on a state load).
        let stale: Vec<f32> = (1..=16).map(|i| i as f32).collect();
        assert_eq!(prod.push_slice(&stale), 16);
        flush.store(true, Ordering::Release);

        let mut out = [-1.0f32; 8];
        fill_output(&mut cons, &mut out, 2, &flush);
        assert!(
            out.iter().all(|&s| s == 0.0),
            "the callback that services the flush outputs silence, not stale samples: {out:?}"
        );
        assert!(
            !flush.load(Ordering::Acquire),
            "the request is consumed exactly once"
        );

        // Nothing of the old timeline survives, and the ring works normally again.
        let mut out2 = [-1.0f32; 4];
        fill_output(&mut cons, &mut out2, 2, &flush);
        assert_eq!(out2, [0.0; 4], "the backlog is gone, not merely delayed");
        assert_eq!(prod.push_slice(&[9.0, 8.0]), 2);
        fill_output(&mut cons, &mut out2, 2, &flush);
        assert_eq!(out2, [9.0, 8.0, 0.0, 0.0], "post-load audio flows again");
    }

    /// Mono and >2-channel devices keep their existing mixdown behaviour through [`fill_output`].
    #[test]
    fn fill_output_handles_mono_and_wide_devices() {
        let (mut prod, mut cons) = make_ring(240);
        let flush = AtomicBool::new(false);

        // Mono: each L,R pair averages into one output sample; the tail underruns to silence.
        assert_eq!(prod.push_slice(&[1.0, 3.0, -1.0, 1.0]), 4);
        let mut mono = [-1.0f32; 3];
        fill_output(&mut cons, &mut mono, 1, &flush);
        assert_eq!(mono, [2.0, 0.0, 0.0]);

        // 4-channel: L,R land in lanes 0,1 of each frame; lanes 2,3 are silent.
        assert_eq!(prod.push_slice(&[0.5, 0.25]), 2);
        let mut wide = [-1.0f32; 8];
        fill_output(&mut cons, &mut wide, 4, &flush);
        assert_eq!(wide, [0.5, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    /// The volume curve: full step is an exact pass-through, step 0 is exact silence, mute overrides any
    /// level, the mapping is monotone, and out-of-range steps saturate instead of exceeding unity gain.
    #[test]
    fn volume_gain_curve() {
        assert_eq!(gain_for(VOLUME_STEPS, false), 1.0, "full volume is unity");
        assert_eq!(gain_for(0, false), 0.0, "step 0 is exact silence");
        assert_eq!(
            gain_for(VOLUME_STEPS, true),
            0.0,
            "mute overrides the level"
        );
        assert_eq!(gain_for(0, true), 0.0);
        // Half-way is half amplitude (the curve is linear in amplitude by design).
        assert_eq!(gain_for(VOLUME_STEPS / 2, false), 0.5);
        // Monotone non-decreasing over the whole range, and never above unity — including a step past the top.
        for s in 0..VOLUME_STEPS {
            let (lo, hi) = (gain_for(s, false), gain_for(s + 1, false));
            assert!(hi > lo, "step {s} -> {} must increase the gain", s + 1);
            assert!((0.0..=1.0).contains(&hi));
        }
        assert_eq!(
            gain_for(u8::MAX, false),
            1.0,
            "an out-of-range step saturates at unity, never louder"
        );
    }

    /// [`push_frame`] applies the gain to what actually reaches the ring: full volume is bit-exact, mute
    /// queues true silence, half volume halves every sample, and a garbage gain is coerced rather than trusted.
    #[test]
    fn push_frame_applies_the_gain() {
        let pcm: [i16; 4] = [i16::MIN, -8192, 0, 16384];
        let expect = |gain: f32| -> Vec<f32> {
            let (mut prod, mut cons) = AudioRing::new(64).split();
            assert_eq!(push_frame(&mut prod, &pcm, gain), 0, "no overrun");
            let mut out = vec![0.0f32; pcm.len()];
            assert_eq!(cons.pop_slice(&mut out), pcm.len());
            out
        };
        let unattenuated: Vec<f32> = pcm.iter().copied().map(sample_i16_to_f32).collect();
        assert_eq!(
            expect(gain_for(VOLUME_STEPS, false)),
            unattenuated,
            "full volume is a bit-exact pass-through"
        );
        assert!(
            expect(gain_for(VOLUME_STEPS, true))
                .iter()
                .all(|&s| s == 0.0),
            "mute queues true silence"
        );
        assert_eq!(
            expect(0.5),
            unattenuated.iter().map(|s| s * 0.5).collect::<Vec<_>>(),
            "half gain halves every sample"
        );
        // Defensive coercion: an over-unity gain cannot amplify past the pass-through, and NaN is silence
        // rather than NaN samples going to the speakers.
        assert_eq!(expect(4.0), unattenuated, "an over-unity gain is clamped");
        assert!(expect(f32::NAN).iter().all(|&s| s == 0.0), "NaN is silence");
    }

    /// **The rewind bug [`resync_sink`] exists for.** A sink that has already seen a high frame index renders
    /// nothing when the machine restarts its clock (reset / ROM reload / state load hand it frame 0 again),
    /// because `on_step_boundary` only renders on an *advancing* frame. Rebuilding the sink re-latches it to
    /// the new timeline and audio resumes immediately.
    #[test]
    fn resync_sink_restores_rendering_after_the_machine_clock_rewinds() {
        let mut sink = AudioSink::new(44_100);
        // Drive it well into a run, with a PSG tone programmed so frames are non-silent.
        sink.on_step_boundary(0, 0);
        sink.on_event(write_event(0x7F11, 0x8E));
        sink.on_event(write_event(0x7F11, 0x0F));
        sink.on_event(write_event(0xC0_0011, 0x90));
        for f in 1..=500u64 {
            sink.on_step_boundary(0, f);
        }
        assert!(
            !sink.drain().is_empty(),
            "the sink was rendering at frame 500"
        );

        // The machine resets: the frame index restarts at 0 and climbs. The stale sink renders nothing.
        for f in 0..=5u64 {
            sink.on_step_boundary(0, f);
        }
        assert!(
            sink.drain().is_empty(),
            "a rewound clock silences a stale sink — this is the bug resync_sink fixes"
        );

        // After a resync the very same boundary sequence renders again.
        resync_sink(&mut sink);
        assert_eq!(sink.sample_rate(), 44_100, "the device rate is preserved");
        for f in 0..=5u64 {
            sink.on_step_boundary(0, f);
        }
        assert_eq!(
            sink.drain().len(),
            5 * 2 * (44_100 / 60),
            "five frames render after the resync"
        );
    }

    /// Design §7 Test 4 — composite-sink equivalence: `AudioAndWatch { watch: Some(..) }` must (a) render
    /// audio into its `AudioSink` and (b) record the *exact same* watch hits a standalone `Watchpoints` would
    /// for the identical event/boundary sequence (mirrors the SY-4a forwarder-equivalence test, `bus.rs:613`).
    #[test]
    fn composite_forwards_to_both_audio_and_watch() {
        // A bus-space write watch that the scripted event will hit.
        let watched = 0xFF_0100u32..=0xFF_0103u32;

        // --- Standalone reference: feed the sequence directly to a Watchpoints. ---
        let mut reference = Watchpoints::new(64);
        reference.add_watch(watched.clone(), WatchOp::Write, "ref");
        reference.on_step_boundary(0x1234, 0); // stamp PC/frame
        reference.on_event(write_event(0xFF_0100, 0xAB)); // in range → hit
        reference.on_event(write_event(0xFF_0200, 0xCD)); // out of range → no hit
        reference.on_event(write_event(0xFF_0102, 0xEF)); // in range → hit

        // --- Composite: same sequence through AudioAndWatch, with a live AudioSink attached. ---
        let mut audio = AudioSink::new(44_100);
        let mut watch = Watchpoints::new(64);
        watch.add_watch(watched, WatchOp::Write, "ref");
        {
            let mut sink = AudioAndWatch::new(&mut audio, Some(&mut watch));
            sink.on_step_boundary(0x1234, 0); // latches audio's first frame + stamps the watch
            sink.on_event(write_event(0xFF_0100, 0xAB));
            sink.on_event(write_event(0xFF_0200, 0xCD));
            sink.on_event(write_event(0xFF_0102, 0xEF));
            sink.on_step_boundary(0x5678, 1); // render one audio frame
        } // `sink` drops, releasing the &mut borrows of `audio`/`watch`

        // (a) The watch recorded exactly the standalone hits — the composite forwards faithfully.
        assert_eq!(
            watch.hits(),
            reference.hits(),
            "composite must record the identical watch hits a standalone Watchpoints would"
        );
        assert_eq!(watch.hits().len(), 2, "two in-range writes were watched");

        // (b) The AudioSink rendered a frame of audio through the same composite.
        assert_eq!(
            audio.samples().len(),
            1470,
            "one 44.1 kHz frame = 2·735 i16"
        );
    }

    /// The `watch: None` arm must be inert for the watch side while still driving audio (the fast path when no
    /// tile watch is armed), and it must report `wants_vdp_writes() == false`.
    #[test]
    fn composite_without_watch_drives_audio_only() {
        let mut audio = AudioSink::new(44_100);
        let mut sink = AudioAndWatch::new(&mut audio, None);
        assert!(
            !sink.wants_vdp_writes(),
            "audio-only composite wants no VDP writes"
        );
        sink.on_step_boundary(0, 0);
        sink.on_event(write_event(0x7F11, 0x8E));
        sink.on_event(write_event(0x7F11, 0x0F));
        sink.on_event(write_event(0xC0_0011, 0x90));
        sink.on_step_boundary(0, 1);
        assert_eq!(
            audio.samples().len(),
            1470,
            "audio still renders with no watch attached"
        );
    }
}
