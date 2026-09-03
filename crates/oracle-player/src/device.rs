//! The host audio device: the real `cpal` output stream, the player's own SPSC ring, and the counters that
//! make the pacing measurable.
//!
//! The stream body is [`crate::audio::fill_output`] — the minifb player's own callback, unmodified. What is
//! added around it is instrumentation, because **`fill_output` reports nothing about what it could not
//! serve**: it zero-fills a short tail and returns. A pacing measurement that cannot count underruns is not
//! a pacing measurement, so the ring's occupancy is read immediately before the call and the shortfall
//! counted.
//!
//! # Two counters, not one
//!
//! Starvations are split into **total** and **steady-state** (excluding the first
//! [`WARMUP_CALLBACKS`]). The pre-roll is two frames of silence and the device's *first* callback can ask
//! for a quantum several times that, so a starve at the very start is the reservoir filling, not the loop
//! failing. Reporting one number for both is the believable wrong answer.
//!
//! # Gain
//!
//! `gain` multiplies on the **producer** side ([`crate::audio::push_frame`]), so a gain of 0.0 leaves the
//! ring dynamics, the feedback loop and every underrun count genuine while the amplitude is exactly zero.
//! Every measurement mode forces 0.0: this machine's owner is using it.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::audio;

/// Device callbacks treated as warm-up and excluded from the steady-state starvation count and from the
/// leanest-ring figure. ~1.5 s at a typical callback rate.
pub const WARMUP_CALLBACKS: u64 = 60;

/// Counters the real-time callback raises and the report reads. The callback may not allocate, lock or
/// print, so everything it has to say it says through these.
#[derive(Default)]
pub struct Counters {
    pub callbacks: AtomicU64,
    /// Callbacks that found the ring short of what they needed — i.e. that zero-filled a tail.
    pub starved: AtomicU64,
    /// The same, from [`WARMUP_CALLBACKS`] onward. **This is the pacing verdict.**
    pub starved_steady: AtomicU64,
    /// Total ring samples missing across all starved callbacks.
    pub starved_samples: AtomicU64,
    /// Ring occupancy, in samples, at the leanest steady-state callback. `u64::MAX` = none yet.
    pub min_occupancy: AtomicU64,
}

pub struct Device {
    sink: oracle_core::synth::AudioSink,
    prod: audio::AudioProd,
    frame_samples: usize,
    counters: Arc<Counters>,
    /// Ring samples the producer could not push because the ring was full — the *other* end of the
    /// feedback loop from a starve, and the one that ran to 5.8 M in the spike's overflowing run.
    dropped: u64,
    /// The callback's "discard what you are holding" flag, **kept this side too** — see [`Device::resync`].
    ///
    /// ⚑ Parcel 1 left this owned by the callback alone, on the stated grounds that there was "no save-state
    /// load or ROM reload here yet ... a caller that does not exist". `PLAYER-PUMPREPORT` is the parcel that
    /// makes the caller exist: `emulator/restore`, `emulator/reload_rom` and `emulator/reset` all arrive over
    /// the socket now, and each of them leaves the whole ring holding PCM from a timeline that is gone.
    flush: Arc<AtomicBool>,
    gain: f32,
    rate: u32,
    channels: usize,
    _stream: cpal::Stream,
}

/// Write to the real stderr, bypassing libtest's output capture and any `println!` buffering. A degraded
/// measurement that announces itself through `println!` announces itself into a void.
pub fn loud(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
    let _ = std::io::stderr().flush();
}

impl Device {
    /// Open the default output device. Returns `None` — loudly, and saying which reach limit was hit — on
    /// any failure, exactly as the minifb player does.
    pub fn open(gain: f32) -> Option<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use ringbuf::traits::Observer;

        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                loud(
                    "audio: NO default output device — pacing is UNMEASURED, not measured-and-fine",
                );
                return None;
            }
        };
        let default_cfg = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                loud(&format!(
                    "audio: no default output config ({e}) — pacing is UNMEASURED"
                ));
                return None;
            }
        };
        if default_cfg.sample_format() != cpal::SampleFormat::F32 {
            loud(&format!(
                "audio: device sample format {:?} is not f32 — pacing is UNMEASURED",
                default_cfg.sample_format()
            ));
            return None;
        }
        let rate = default_cfg.sample_rate().0;
        let channels = default_cfg.channels() as usize;
        let config: cpal::StreamConfig = default_cfg.config();

        let sink = oracle_core::synth::AudioSink::new(rate);
        let (mut prod, mut cons) = audio::make_ring(rate);
        let frame_samples = audio::frame_samples(rate);
        audio::preroll_silence(&mut prod, frame_samples);

        let counters = Arc::new(Counters::default());
        counters.min_occupancy.store(u64::MAX, Ordering::Relaxed);
        let cb = Arc::clone(&counters);
        // The callback's "discard what you are holding" flag, held at BOTH ends: the callback checks it
        // (`audio::fill_output`) and `Device::resync` raises it. The producer half cannot drain the ring
        // itself — `clear` is a consumer operation — so this flag is the whole hand-off.
        let cb_flush = Arc::new(AtomicBool::new(false));
        let flush = Arc::clone(&cb_flush);

        let data_cb = move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
            // Ring samples this callback is about to want, given the device's channel count. Read BEFORE
            // `fill_output`, because afterwards the shortfall is indistinguishable from a clean serve.
            let need = match channels {
                2 => out.len(),
                1 => out.len() * 2,
                ch => (out.len() / ch.max(1)) * 2,
            };
            let have = cons.occupied_len();
            audio::fill_output(&mut cons, out, channels, &cb_flush);
            let index = cb.callbacks.fetch_add(1, Ordering::Relaxed);
            if index >= WARMUP_CALLBACKS {
                cb.min_occupancy.fetch_min(have as u64, Ordering::Relaxed);
            }
            if have < need {
                cb.starved.fetch_add(1, Ordering::Relaxed);
                cb.starved_samples
                    .fetch_add((need - have) as u64, Ordering::Relaxed);
                if index >= WARMUP_CALLBACKS {
                    cb.starved_steady.fetch_add(1, Ordering::Relaxed);
                }
            }
        };

        let stream = match device.build_output_stream::<f32, _, _>(
            &config,
            data_cb,
            |e| loud(&format!("audio stream error: {e}")),
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                loud(&format!(
                    "audio: failed to build output stream ({e}) — pacing is UNMEASURED"
                ));
                return None;
            }
        };
        if let Err(e) = stream.play() {
            loud(&format!(
                "audio: failed to start output stream ({e}) — pacing is UNMEASURED"
            ));
            return None;
        }
        loud(&format!(
            "audio: real device open at {rate} Hz / {channels} ch, gain {gain}{}",
            if gain == 0.0 {
                "  (SILENT BY CONSTRUCTION — ring dynamics real, amplitude exactly zero)"
            } else {
                ""
            }
        ));
        Some(Self {
            sink,
            prod,
            frame_samples,
            counters,
            dropped: 0,
            flush,
            gain,
            rate,
            channels,
            _stream: stream,
        })
    }

    /// **Put audio back in step with a machine that was replaced under the player** — a client's
    /// `emulator/restore`, `emulator/reload_rom` or `emulator/reset`, arriving through
    /// [`Host::pump`](oracle_aether::host::Host::pump) between two of this window's own frames.
    ///
    /// Two repairs, for two different failures, and the second is the severe one:
    ///
    /// 1. **Drop the ring backlog.** Up to [`crate::audio::RING_FRAMES`] frames of already-rendered PCM
    ///    belong to the timeline the machine has left. Playing them out is an audible burp of the past.
    /// 2. **Rebuild the sink** ([`crate::audio::resync_sink`]). [`AudioSink::on_step_boundary`] renders only
    ///    when the frame index it is handed is **strictly greater** than the last one it saw
    ///    (`Some(prev) if frame > prev`). All three of those methods can move the machine's frame index
    ///    *backwards* — `emulator/reset` puts it back to 0 outright, because `System::reset` rebuilds the
    ///    `System` and its scheduler — so a sink carried across the jump renders **nothing at all** until the
    ///    machine climbs back past where it was. A reset one minute into a game is a minute of total silence,
    ///    and nothing in the window says why.
    ///
    /// This is the same pair `oracle-frontend`'s `resync_audio` performs, reached through the same two
    /// shared functions; only the state it lives on is this crate's.
    pub fn resync(&mut self) {
        audio::resync_sink(&mut self.sink);
        self.flush.store(true, Ordering::Release);
    }

    /// Drain the synth's last emulated frame and push it into the ring, counting what would not fit.
    pub fn push_frame(&mut self) {
        let pcm = self.sink.drain();
        self.dropped += audio::push_frame(&mut self.prod, &pcm, self.gain) as u64;
    }

    pub fn sink_mut(&mut self) -> &mut oracle_core::synth::AudioSink {
        &mut self.sink
    }

    pub fn prod(&self) -> &audio::AudioProd {
        &self.prod
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn ring_capacity(&self) -> usize {
        audio::ring_capacity(&self.prod)
    }
}
