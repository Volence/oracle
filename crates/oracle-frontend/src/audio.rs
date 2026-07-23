//! Phase SY-5a — the real-time-audio **substrate**: a lock-free SPSC ring buffer, `i16 → f32` conversion,
//! and a composite [`BusEventSink`] that drives the synth's [`AudioSink`] alongside the existing
//! pixel-attribution watch. This is the headless-testable half of Phase SY-5
//! (`docs/2026-07-23-phase-sy5-realtime-audio-design.md`, §2/§5/§6/§8 SY-5a).
//!
//! It deliberately does **not** open a host audio device — that (`cpal` output stream) is SY-5b. The ring's
//! consumer is popped only by the tests here; the live `main()` loop is switched to
//! [`run_frames_with_sink`](oracle_core::system::System::run_frames_with_sink) + drain→push and the cpal
//! callback (the real consumer) in SY-5b. Until that wiring lands, the substrate is exercised solely by the
//! tests below, so the module carries `#![allow(dead_code)]` (the whole point of the SY-5a/SY-5b split is to
//! *bank and review* the ring/sink logic before touching the un-hearable-here device glue).
#![allow(dead_code)]

use oracle_core::bus::{BusEvent, BusEventSink};
use oracle_core::synth::AudioSink;
use oracle_core::vdp::VdpWrite;
use oracle_core::watchpoints::Watchpoints;
use ringbuf::traits::{Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

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

/// Convert an interleaved `i16` frame (from [`AudioSink::drain`]), `push_slice` it into the ring producer, and
/// return the count of samples **dropped** on overrun. `push_slice` accepts fewer than offered when the ring
/// is full (design §2.5 overrun); the surplus is discarded — the producer **never** blocks/spins waiting for
/// the audio thread (that would stall video). Conversion is done here, on the non-real-time producer thread.
pub fn push_frame(prod: &mut AudioProd, pcm: &[i16]) -> usize {
    let converted: Vec<f32> = pcm.iter().copied().map(sample_i16_to_f32).collect();
    let pushed = prod.push_slice(&converted);
    converted.len() - pushed
}

/// A composite [`BusEventSink`] that forwards **every** event to the synth's [`AudioSink`] and, when present,
/// to the pixel-attribution [`Watchpoints`] — so real-time audio and the milestone-D3 watch tooling stay live
/// at the same time (design §6.2). [`BusEvent`] is `Copy`, so forwarding the same event to both sinks is a
/// trivial copy.
pub struct AudioAndWatch<'a> {
    /// The persistent synth sink — attached every frame while audio is on.
    pub audio: &'a mut AudioSink,
    /// The tile watch — `Some` only while one is armed (mirrors the frontend's `watch_armed`).
    pub watch: Option<&'a mut Watchpoints>,
}

impl BusEventSink for AudioAndWatch<'_> {
    fn on_event(&mut self, e: BusEvent) {
        self.audio.on_event(e);
        if let Some(w) = &mut self.watch {
            w.on_event(e);
        }
    }

    fn on_event_at(&mut self, e: BusEvent, mclk: u64) {
        // AudioSink's SY-4b timed path; Watchpoints rides the default forwarder to `on_event`.
        self.audio.on_event_at(e, mclk);
        if let Some(w) = &mut self.watch {
            w.on_event_at(e, mclk);
        }
    }

    fn on_step_boundary(&mut self, pc: u32, frame: u64) {
        // Drives AudioSink's per-frame render; stamps the watch's PC/frame context.
        self.audio.on_step_boundary(pc, frame);
        if let Some(w) = &mut self.watch {
            w.on_step_boundary(pc, frame);
        }
    }

    fn wants_vdp_writes(&self) -> bool {
        // AudioSink wants none; only the watch (when present) can arm the currency-sensitive VDP capture.
        self.watch.as_ref().is_some_and(|w| w.wants_vdp_writes())
    }

    fn on_vdp_write(&mut self, wr: VdpWrite) {
        // AudioSink ignores VDP writes; forward only to the watch.
        if let Some(w) = &mut self.watch {
            w.on_vdp_write(wr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_core::bus::{BusOp, Size};
    use oracle_core::watchpoints::WatchOp;
    use ringbuf::traits::Consumer; // `pop_slice` — only the tests consume the ring in SY-5a

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
            let dropped = push_frame(&mut prod, &pcm);
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
            let mut sink = AudioAndWatch {
                audio: &mut audio,
                watch: Some(&mut watch),
            };
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
        let mut sink = AudioAndWatch {
            audio: &mut audio,
            watch: None,
        };
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
