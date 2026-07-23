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
//! ## Scope (SY-1 + SY-2)
//!
//! SY-1: a hand-rolled [`Sn76489`](sn76489::Sn76489) PSG synthesizer and the [`AudioSink`] pipeline
//! (decode → frame-batched render → pull buffer). SY-2: a minimal hand-rolled
//! [`Ym2612Synth`](ym2612_synth::Ym2612Synth) FM synthesizer (6 channels × 4 operators, the 8 algorithms,
//! an ADSR-ish envelope), mixed into the same render. The FM synth's data source is the **identical**
//! `(bank, reg, value)` write stream the PSG and VGM paths consume. DAC/PCM, LFO, and exact OPN rate/
//! key-scale tables are SY-3 (see [`ym2612_synth`]).

pub mod audio_sink;
pub mod sn76489;
pub mod ym2612_synth;

pub use audio_sink::{AudioSink, DEFAULT_SAMPLE_RATE};
pub use sn76489::Sn76489;
pub use ym2612_synth::Ym2612Synth;
