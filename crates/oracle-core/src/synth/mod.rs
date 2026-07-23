//! `synth` — native, opt-in sound synthesis (Phase SY). **Feature-gated (`synth`), default OFF.**
//!
//! This module turns the live sound-chip **register-write stream** our core already taps (the same
//! `(chip, reg, value)` triples the [`crate::vgm::VgmLogger`] decodes) into **PCM audio samples produced
//! inside the core** — no external `vgm2wav` step, no audio device, no threads.
//!
//! ## Currency-neutrality (why this cannot move a state hash)
//!
//! Three independent layers, any one of which alone makes it neutral:
//! 1. **Caller-owned sink.** [`AudioSink`] is a [`BusEventSink`](crate::bus::BusEventSink) threaded
//!    through the opt-in [`run_frames_with_sink`](crate::system::System::run_frames_with_sink) seam. The
//!    default [`run_frames`](crate::system::System::run_frames) passes `&mut ()` and is byte-untouched.
//! 2. **Feature gate.** The whole module is absent from the default build the currency gates compile.
//! 3. **Not in `System`.** `AudioSink` is never stored in `System`, so it cannot enter `state_hash` or
//!    `export_state`.
//!
//! ## SY-1 scope
//!
//! A hand-rolled [`Sn76489`](sn76489::Sn76489) PSG synthesizer and the [`AudioSink`] pipeline
//! (decode → frame-batched render → pull buffer). The FM (YM2612) synthesizer is a later slice; FM writes
//! are decoded off the same stream but not yet turned into sound.

pub mod audio_sink;
pub mod sn76489;

pub use audio_sink::{AudioSink, DEFAULT_SAMPLE_RATE};
pub use sn76489::Sn76489;
