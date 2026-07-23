//! Hand-rolled SN76489 (PSG) synthesizer — Phase SY-1.
//!
//! The Genesis PSG is a Texas Instruments SN76489 clone: **three square-wave tone channels**, **one
//! noise channel** (a shift-register LFSR), and a **4-bit logarithmic attenuator per channel**. It is a
//! single write-only port of self-describing bytes (the same byte stream the [`crate::vgm::VgmLogger`]
//! records). This module turns that byte stream into PCM samples.
//!
//! Model (per-output-sample, native 44.1 kHz-class rate, no floats in the audio state):
//! - Each tone channel is a 10-bit period reload driving a fixed-point down-counter in the chip's
//!   `clock/16` tick domain. When the counter underflows, the channel output **toggles** (`±volume`),
//!   giving a bipolar square wave whose full-cycle frequency is `clock / (32 · period)`.
//! - The noise channel clocks a 16-bit LFSR (tap mask `0x0009`, white) or a single tap (periodic). Its
//!   counter drives a toggling output flip-flop (exactly like a tone channel); the LFSR advances only on
//!   that output's rising edge — every *other* underflow — so the effective shift rate is `clock/512`,
//!   `/1024`, `/2048` for rate_sel 0/1/2, and tone-2's output frequency `clock/(32·P)` for mode 3.
//!   Output is `±volume` from LFSR bit 0.
//! - Attenuation is a 16-entry table, 2 dB/step, entry 15 = silence.
//!
//! Determinism: the whole model is integer (Q16 fixed-point counters). This is a **synthesis** helper —
//! it is never part of `System`, `state_hash`, or `export_state` (see the module docs), so it carries no
//! currency obligations, but staying integer keeps it reproducible anyway.

/// SN76489 reference clock on the Genesis (Hz). Used only to derive the per-sample tick rate.
const PSG_CLOCK: u32 = 3_579_545;

/// 16-entry attenuation → linear-amplitude table, 2 dB per step (entry 15 = mute). Peak chosen so the
/// four channels summed stay well inside `i16` (4 × 4000 = 16000 ≪ 32767). Precomputed (no float at
/// runtime): `amp[i] = round(4000 · 10^(-i/10))`, `amp[15] = 0`.
const VOL_TABLE: [i16; 16] = [
    4000, 3177, 2524, 2005, 1592, 1265, 1005, 798, 634, 504, 400, 318, 252, 200, 159, 0,
];

/// LFSR tap mask for white noise (Genesis / SMS SN76489 variant): bits 0 and 3.
const NOISE_TAPS: u16 = 0x0009;
/// LFSR reset seed (bit 15 set) — the value the shift register is (re)loaded with on a noise-control write.
const LFSR_SEED: u16 = 0x8000;

/// One square-wave tone channel: a 10-bit period, a Q16 down-counter, a toggling output phase, and a
/// 4-bit attenuation index.
#[derive(Clone, Copy)]
struct Tone {
    /// 10-bit period reload (0 is treated as 1 to avoid a stuck counter).
    period: u16,
    /// Down-counter in Q16 `clock/16` ticks; underflow toggles [`phase`](Self::phase).
    counter_q16: i64,
    /// Output phase: `false` → `-volume`, `true` → `+volume`.
    phase: bool,
    /// Attenuation index into [`VOL_TABLE`] (0 = loudest, 15 = mute).
    atten: u8,
}

impl Tone {
    fn new() -> Self {
        Self {
            period: 1,
            counter_q16: 1 << 16,
            phase: true,
            atten: 15, // start muted, like a reset PSG
        }
    }

    /// Advance one output sample; return this channel's signed contribution.
    fn next_sample(&mut self, ticks_per_sample_q16: i64) -> i32 {
        let reload = (self.period.max(1) as i64) << 16;
        self.counter_q16 -= ticks_per_sample_q16;
        // High tones can toggle several times per output sample; the loop is bounded by
        // ticks_per_sample / period.
        while self.counter_q16 <= 0 {
            self.counter_q16 += reload;
            self.phase = !self.phase;
        }
        let vol = VOL_TABLE[self.atten as usize] as i32;
        if self.phase {
            vol
        } else {
            -vol
        }
    }
}

/// The noise channel: an LFSR clocked by a period-driven counter, plus feedback mode and attenuation.
#[derive(Clone, Copy)]
struct Noise {
    /// 16-bit linear-feedback shift register (output = bit 0).
    lfsr: u16,
    /// `true` = white noise (tapped feedback), `false` = periodic (single-tap).
    white: bool,
    /// Low two bits of the noise-control register: shift-rate selector (3 = "use tone-2 period").
    rate_sel: u8,
    /// Down-counter in Q16 `clock/16` ticks; underflow toggles [`output_flip`](Self::output_flip).
    counter_q16: i64,
    /// The noise counter's output flip-flop. Like a tone channel, the counter drives a toggling
    /// output; the LFSR is clocked only on that output's **rising edge** (every *other* underflow).
    /// This halves the LFSR shift rate versus the raw counter, matching real SN76489 hardware.
    output_flip: bool,
    /// Attenuation index into [`VOL_TABLE`].
    atten: u8,
}

impl Noise {
    fn new() -> Self {
        Self {
            lfsr: LFSR_SEED,
            white: false,
            rate_sel: 0,
            counter_q16: 1 << 16,
            output_flip: false,
            atten: 15,
        }
    }

    /// The noise counter's reload value (in `clock/16` ticks): three fixed divisors, or tone-2's period.
    fn reload_ticks(&self, tone2_period: u16) -> i64 {
        let ticks: u32 = match self.rate_sel & 0x03 {
            0 => 0x10,
            1 => 0x20,
            2 => 0x40,
            _ => tone2_period.max(1) as u32,
        };
        (ticks as i64) << 16
    }

    /// Clock the LFSR once (called on each counter underflow).
    fn clock_lfsr(&mut self) {
        let feedback = if self.white {
            (self.lfsr & NOISE_TAPS).count_ones() as u16 & 1
        } else {
            self.lfsr & 1
        };
        self.lfsr = (self.lfsr >> 1) | (feedback << 15);
    }

    fn next_sample(&mut self, ticks_per_sample_q16: i64, tone2_period: u16) -> i32 {
        let reload = self.reload_ticks(tone2_period);
        self.counter_q16 -= ticks_per_sample_q16;
        while self.counter_q16 <= 0 {
            self.counter_q16 += reload;
            // The counter drives a toggling output flip-flop; the LFSR advances only on that
            // output's rising edge (false→true), i.e. every *other* underflow. This makes the
            // effective LFSR shift period 2× the counter reload: clock/512, /1024, /2048 for
            // rate_sel 0/1/2, and exactly tone-2's output frequency clock/(32·P) for mode 3.
            self.output_flip = !self.output_flip;
            if self.output_flip {
                self.clock_lfsr();
            }
        }
        let vol = VOL_TABLE[self.atten as usize] as i32;
        if self.lfsr & 1 != 0 {
            vol
        } else {
            -vol
        }
    }
}

/// The full SN76489 PSG synthesizer: three tone channels, one noise channel, and the write-decode latch.
pub struct Sn76489 {
    tone: [Tone; 3],
    noise: Noise,
    /// The 3-bit register selector latched by the last `bit7=1` byte (channel<<1 | type).
    latched_reg: u8,
    /// `clock/16` ticks per output sample, Q16 fixed-point — derived from the output sample rate.
    ticks_per_sample_q16: i64,
}

impl Sn76489 {
    /// A fresh PSG clocked to produce `sample_rate` Hz output (all channels muted at reset).
    pub fn new(sample_rate: u32) -> Self {
        // (clock / 16) ticks per output sample, in Q16 fixed point.
        let ticks_per_sample_q16 = ((PSG_CLOCK as u64 * 65536) / (16 * sample_rate as u64)) as i64;
        Self {
            tone: [Tone::new(); 3],
            noise: Noise::new(),
            latched_reg: 0,
            ticks_per_sample_q16,
        }
    }

    /// Feed one PSG port byte (the identical self-describing byte the VGM path records).
    ///
    /// `bit7=1` latches the register selector (channel + type) and writes the low 4 data bits; `bit7=0`
    /// writes the high 6 data bits (tone period hi) / low 4 (attenuation, noise control) of the latched
    /// register.
    pub fn write(&mut self, byte: u8) {
        if byte & 0x80 != 0 {
            self.latched_reg = (byte >> 4) & 0x07;
            self.apply_low_nibble(byte & 0x0F);
        } else {
            self.apply_data(byte & 0x3F);
        }
    }

    /// Apply the 4-bit data field of a `bit7=1` latch byte to the just-latched register.
    fn apply_low_nibble(&mut self, data4: u8) {
        let ch = (self.latched_reg >> 1) as usize;
        let is_atten = self.latched_reg & 1 != 0;
        match (ch, is_atten) {
            // Tone frequency: replace the low 4 bits of the 10-bit period.
            (0..=2, false) => {
                let p = &mut self.tone[ch].period;
                *p = (*p & 0x3F0) | (data4 as u16);
            }
            // Tone attenuation.
            (0..=2, true) => self.tone[ch].atten = data4,
            // Noise control (channel 3, type 0): bit2 = feedback mode, bits1-0 = rate. Resets the LFSR.
            (3, false) => {
                self.noise.white = data4 & 0x04 != 0;
                self.noise.rate_sel = data4 & 0x03;
                self.noise.lfsr = LFSR_SEED;
            }
            // Noise attenuation (channel 3, type 1).
            (3, true) => self.noise.atten = data4,
            _ => {}
        }
    }

    /// Apply the 6-bit data field of a `bit7=0` byte to the latched register.
    fn apply_data(&mut self, data6: u8) {
        let ch = (self.latched_reg >> 1) as usize;
        let is_atten = self.latched_reg & 1 != 0;
        match (ch, is_atten) {
            // Tone frequency: the 6 bits are the high bits of the 10-bit period.
            (0..=2, false) => {
                let p = &mut self.tone[ch].period;
                *p = ((data6 as u16 & 0x3F) << 4) | (*p & 0x0F);
            }
            (0..=2, true) => self.tone[ch].atten = data6 & 0x0F,
            (3, false) => {
                self.noise.white = data6 & 0x04 != 0;
                self.noise.rate_sel = data6 & 0x03;
                self.noise.lfsr = LFSR_SEED;
            }
            (3, true) => self.noise.atten = data6 & 0x0F,
            _ => {}
        }
    }

    /// Produce one mono output sample (sum of the three tones + noise), advancing all channels.
    pub fn next_sample(&mut self) -> i16 {
        let tps = self.ticks_per_sample_q16;
        let tone2_period = self.tone[2].period;
        let mut acc: i32 = 0;
        for t in &mut self.tone {
            acc += t.next_sample(tps);
        }
        acc += self.noise.next_sample(tps, tone2_period);
        acc.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Program tone channel 0 to a known 10-bit period at full volume, render one second, and count zero
    /// crossings — proving the synth emits a square wave at the SN76489's specified pitch.
    #[test]
    fn tone0_produces_expected_pitch() {
        let sample_rate = 44_100u32;
        let mut psg = Sn76489::new(sample_rate);

        // Period 254 → f = 3_579_545 / (32 · 254) ≈ 440.4 Hz. Encode it:
        //   latch byte  : bit7=1, reg=0 (tone0 freq), low nibble = 254 & 0xF = 0xE  → 0x8E
        //   data byte   : bit7=0, high 6 bits = (254 >> 4) & 0x3F = 0x0F            → 0x0F
        psg.write(0x8E);
        psg.write(0x0F);
        // Attenuation 0 (loudest): latch byte, reg=1 (tone0 atten), low nibble 0 → 0x90.
        psg.write(0x90);

        // Render one second and count sign changes.
        let mut prev = psg.next_sample();
        let mut crossings = 0u32;
        for _ in 1..sample_rate {
            let s = psg.next_sample();
            if (s < 0) != (prev < 0) {
                crossings += 1;
            }
            prev = s;
        }

        // A full square-wave cycle = 2 crossings, so expected ≈ 2 · 440.4 ≈ 881 crossings/sec.
        // Allow a small band for the fixed-point / one-sample counting edge.
        assert!(
            (876..=884).contains(&crossings),
            "expected ~881 zero crossings for a 440 Hz tone, got {crossings}"
        );
    }

    /// A muted channel (attenuation 15) must be silent regardless of period.
    #[test]
    fn muted_channel_is_silent() {
        let mut psg = Sn76489::new(44_100);
        psg.write(0x8E); // tone0 period low
        psg.write(0x0F); // tone0 period high
        psg.write(0x9F); // tone0 atten = 15 (mute)
        for _ in 0..1000 {
            assert_eq!(psg.next_sample(), 0, "muted tone0 must emit silence");
        }
    }

    /// The noise channel must actually vary (LFSR runs), not sit at a constant level.
    #[test]
    fn noise_channel_varies() {
        let mut psg = Sn76489::new(44_100);
        psg.write(0xE4); // noise control: white (bit2), rate 0
        psg.write(0xF0); // noise atten = 0 (loudest)
        let first = psg.next_sample();
        let mut differed = false;
        for _ in 0..2000 {
            if psg.next_sample() != first {
                differed = true;
                break;
            }
        }
        assert!(differed, "white noise should not be a constant level");
    }

    /// Measure the LFSR shift period, in PSG-clock cycles, for a given noise-control byte by counting
    /// how many `clock/16` ticks elapse between two LFSR-state changes. Drives the noise counter
    /// directly (bypassing the output-sample rate) to isolate the counter+flip-flop timing.
    fn measure_lfsr_shift_period_clocks(noise_ctrl: u8, tone2_period: u16) -> i64 {
        let mut noise = Noise::new();
        // Program the noise-control register (bit2 = white/periodic, bits1-0 = rate).
        noise.white = noise_ctrl & 0x04 != 0;
        noise.rate_sel = noise_ctrl & 0x03;
        noise.lfsr = LFSR_SEED;

        // Advance one clock/16 tick at a time (tick = 1<<16 in Q16) and count ticks between two
        // consecutive LFSR shifts. The first change gives us a clean starting edge; the count to the
        // second change is one full shift period in clock/16 ticks.
        let one_tick_q16: i64 = 1 << 16;
        let prev_lfsr = noise.lfsr;
        loop {
            noise.next_sample(one_tick_q16, tone2_period);
            if noise.lfsr != prev_lfsr {
                break;
            }
        }
        let anchor = noise.lfsr;
        let mut ticks_between = 0i64;
        loop {
            noise.next_sample(one_tick_q16, tone2_period);
            ticks_between += 1;
            if noise.lfsr != anchor {
                break;
            }
        }
        // Each clock/16 tick is 16 PSG-clock cycles.
        ticks_between * 16
    }

    /// Pin the fixed-divisor noise rates: the LFSR must shift once every clock/512, /1024, /2048
    /// (rate_sel 0/1/2) — i.e. the output-flip-flop halves the raw counter reloads of 16/32/64
    /// clock/16-ticks into effective shift periods of 32/64/128 clock/16-ticks.
    #[test]
    fn noise_fixed_rates_are_clock_over_512_family() {
        // rate_sel 0 → 0xE0 latch (bit7, reg=noise-ctrl=6, white bit + rate). Use white feedback so
        // the LFSR state actually changes each shift. noise_ctrl low nibble: bit2=white, bits1-0=rate.
        assert_eq!(
            measure_lfsr_shift_period_clocks(0b100, 0),
            512,
            "rate_sel=0 must shift the LFSR every 512 PSG clocks"
        );
        assert_eq!(
            measure_lfsr_shift_period_clocks(0b101, 0),
            1024,
            "rate_sel=1 must shift the LFSR every 1024 PSG clocks"
        );
        assert_eq!(
            measure_lfsr_shift_period_clocks(0b110, 0),
            2048,
            "rate_sel=2 must shift the LFSR every 2048 PSG clocks"
        );
    }

    /// Pin mode 3 (rate_sel=3, "use tone-2 period"): the LFSR must clock at tone-2's *output* square
    /// frequency clock/(32·P), NOT double it. So the shift period is 32·P PSG clocks.
    #[test]
    fn noise_mode3_tracks_tone2_output_frequency() {
        let p: u16 = 10;
        // rate_sel=3, white feedback (bit2 set): low nibble 0b111.
        assert_eq!(
            measure_lfsr_shift_period_clocks(0b111, p),
            32 * p as i64,
            "mode-3 noise must shift at tone-2's output frequency clock/(32·P), not clock/(16·P)"
        );
    }
}
