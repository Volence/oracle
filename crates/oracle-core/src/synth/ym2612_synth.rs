//! Hand-rolled YM2612 (OPN2) FM synthesizer — Phase SY-3b (**integer OPN2 core**).
//!
//! The Genesis FM chip is a 4-operator, 6-channel phase-modulation (FM) synthesizer. This module turns
//! the **same `(bank, reg, value)` register-write triples** the [`crate::vgm::VgmLogger`] decodes into PCM
//! — it is the FM counterpart of the SY-1 [`Sn76489`](super::sn76489::Sn76489) hand-roll.
//!
//! ## Table & operator attribution (Fork-1 decision, SY-3 plan)
//!
//! **Tables and operator semantics derived from ymfm (Copyright (c) Aaron Giles), BSD-3-Clause —
//! <https://github.com/aaronsgiles/ymfm> (`src/ymfm_fm.ipp`: `abs_sin_attenuation` / `s_sin_table`,
//! `attenuation_to_volume` / `s_power_table`, and the operator phase→volume pipeline).** Our generated
//! tables are pinned to ymfm's literal values by unit tests (see [`tests`]). Nuked-OPN2 (LGPL) is **not**
//! consulted, ported, or used as a fixture.
//!
//! ## Model (what SY-3b implements — the OPN2-exact integer datapath)
//!
//! - **6 channels × 4 operators.** Each operator is a phase generator plus an ADSR-ish envelope, both
//!   advanced at the chip's **native operator rate** (`7_670_453 / 144 ≈ 53_267 Hz`); the mix is then
//!   linearly resampled to the sink's 44.1 kHz (see [`Ym2612Synth::next_sample`]).
//! - **Integer operator pipeline** (replaces SY-2's float sine × linear-gain path): a 256-entry
//!   quarter-wave **log-sin** attenuation table + a 256-entry **exp/pow** table, both in the OPN2
//!   log-attenuation domain. Operator output = `pow[(logsin(phase) + (env+TL)·4) ] >> shift`, sign from the
//!   phase quadrant — i.e. **modulation sums into the phase, envelope/TL sum into the attenuation**, exactly
//!   as OPN2/OPL. The 20-bit phase's top 10 bits index the sine; inter-operator modulation and op-1 feedback
//!   are added to the phase in ymfm's units (`modulator >> 1`, `feedback >> (10-fb)`).
//! - **Envelope generator.** Attack / Decay / Sustain / Release keyed by `$28`, value living in the OPN
//!   10-bit attenuation domain (0 = loud, [`MAX_ATT`] = silent) so it sums with `TL` (`$40`) and the log-sin
//!   in one place. The per-sample **rates are still the SY-2 approximation** (`RATE_BASE`/`ATTACK_SPEED`,
//!   rescaled to the native tick) — the exact OPN rate/key-scale tables are the next slice (SY-3c).
//! - **The 8 FM algorithms + operator-1 self-feedback** (`$B0`), and **stereo L/R pan** (`$B4`).
//!
//! ## DAC / PCM channel-6 (SY-3a — preserved unchanged)
//!
//! - **DAC / PCM channel-6** (`$2A` stream, `$2B` enable). Each frame's ordered `$2A` bytes are spread
//!   evenly across the frame's output samples with a zero-order hold ([`Ym2612Synth::begin_frame`] +
//!   [`Ym2612Synth::dac_sample`]); the 8-bit unsigned samples are centered on `0x80` and scaled by
//!   [`DAC_SCALE`]. While `$2B` bit7 is set, FM channel 6 is muted and the PCM stream plays in its place.
//!   The DAC path runs at the **output** rate (added after the FM resample), untouched by SY-3b.
//!
//! ## Deferred to later SY-3 slices (documented inaccuracies, not bugs)
//!
//! - **Exact envelope rate & key-scale tables** (SY-3c), **detune** (`$30` DT, SY-3d), **LFO** (`$22`,
//!   SY-3e), **SSG-EG** (`$90`) / **CSM / ch3-special** (SY-3f). Sub-frame `$2A` timing is SY-4.
//!
//! The synth is integer/float-based — it is a **synthesis** helper, never part of `System`, `state_hash`,
//! or `export_state`, so it carries no currency obligations.

use std::f64::consts::PI;

/// YM2612 FM clock (Hz) — the master FM clock the phase generator's real-Hz frequency is derived from.
const YM2612_CLOCK: f64 = 7_670_453.0;
/// The FM operator sample-clock divisor: the chip advances an operator once per 144 master cycles.
const FM_CLOCK_DIV: f64 = 144.0;
/// The chip's native operator rate (Hz): `YM2612_CLOCK / 144 ≈ 53_267 Hz`. The phase/envelope tick runs
/// here and the result is resampled to the sink's output rate.
const NATIVE_RATE: f64 = YM2612_CLOCK / FM_CLOCK_DIV;

/// Envelope attenuation is a 10-bit value: 0 = full volume, [`MAX_ATT`] = silence.
const MAX_ATT: f32 = 1023.0;

/// Reference output rate the SY-2 envelope rates were calibrated at (Hz).
const OUTPUT_RATE_REF: f32 = 44_100.0;
/// Scale from the SY-2 per-output-sample envelope increment to a per-**native-tick** increment, so envelope
/// wall-clock timing is preserved now that the EG clocks at [`NATIVE_RATE`] (~53_267 Hz) instead of 44.1 kHz.
/// (Approximate EG — the exact rate table is SY-3c.)
const ENV_RATE_SCALE: f32 = OUTPUT_RATE_REF / (NATIVE_RATE as f32);

/// Per-native-tick envelope-rate scale (attenuation units per tick at effective rate 0's `2^0` base).
/// Approximate: the exact OPN rate table is SY-3c.
const RATE_BASE: f32 = 0.0006;
/// Attack is the same rate curve as decay, sped up by this factor (attack is much faster than decay on
/// real OPN2). Approximate.
const ATTACK_SPEED: f32 = 12.0;

/// Post-mix FM gain in Q15. One full-scale operator (`attenuation_to_volume(0)` = `0x1FE8` = 8168) maps to
/// `8168 · FM_LEVEL_Q15 >> 15 ≈ 3499`, matching SY-2's per-carrier ~3500 loudness so the mix sits at a
/// comparable level (the spectral-corr gate stays meaningful and the DAC headroom note below still holds).
const FM_LEVEL_Q15: i64 = 14_041;

/// DAC/PCM (channel-6) output scale — SY-3a (unchanged). The 8-bit unsigned sample is centered around `0x80`
/// to a signed `[-128, 127]`, then multiplied by this constant. Peak drum level (`128 · 28 = 3584`) is
/// comparable to one FM carrier (~3500). While the DAC is on, FM ch6 is muted, so the worst-case pre-clamp
/// sum stays inside the `i16` range — no systematic clipping introduced by the DAC.
const DAC_SCALE: i32 = 28;

/// Length of the log-sin and exp/pow lookup tables (8-bit quarter-wave index).
const TABLE_LEN: usize = 256;

/// Build the 256-entry quarter-wave **log-sin** attenuation table (ymfm `s_sin_table`).
///
/// `log_sin[i] = round(-log2(sin((i + 0.5) · π / 512)) · 256)`. Pinned to ymfm's literal `s_sin_table`
/// (entries 0-15 and the tail) by [`tests::log_sin_table_matches_ymfm`]. Values are in the OPN log domain:
/// 0 ≈ loudest (sin near ±1), ~`0x859` ≈ the flat part near a zero crossing.
fn build_log_sin() -> [u16; TABLE_LEN] {
    let mut t = [0u16; TABLE_LEN];
    for (i, v) in t.iter_mut().enumerate() {
        let s = ((i as f64 + 0.5) * PI / 512.0).sin();
        *v = (-(s.log2()) * 256.0).round() as u16;
    }
    t
}

/// Build the raw 256-entry **exp/pow** mantissa table (ymfm `s_power_table` **before** the `X` macro).
///
/// `pow_raw[i] = round((2^((255 - i) / 256) - 1) · 1024)` — descending `0x3fa … 0x000`. Pinned to ymfm's
/// literal values by [`tests::pow_table_matches_ymfm`].
fn build_pow_raw() -> [u16; TABLE_LEN] {
    let mut t = [0u16; TABLE_LEN];
    for (i, v) in t.iter_mut().enumerate() {
        *v = ((2f64.powf((255 - i) as f64 / 256.0) - 1.0) * 1024.0).round() as u16;
    }
    t
}

/// Build the 256-entry exp/pow table with ymfm's `X(a) = ((a | 0x400) << 2)` macro applied, so
/// [`attenuation_to_volume`] can index it directly. Entry 0 = `(0x3fa | 0x400) << 2 = 0x1FE8` (loudest).
fn build_pow() -> [u16; TABLE_LEN] {
    let raw = build_pow_raw();
    let mut t = [0u16; TABLE_LEN];
    for (i, v) in t.iter_mut().enumerate() {
        *v = (raw[i] | 0x400) << 2;
    }
    t
}

/// ymfm `abs_sin_attenuation`: the log-domain magnitude of `sin(phase)` for a 10-bit phase. Bit 8 folds the
/// quarter wave (`~input`); the low 8 bits index the table. The sign (bit 9) is applied by the caller.
fn abs_sin_attenuation(phase: u32, log_sin: &[u16; TABLE_LEN]) -> u32 {
    let input = if phase & 0x100 != 0 { !phase } else { phase };
    log_sin[(input & 0xff) as usize] as u32
}

/// ymfm `attenuation_to_volume`: a total log-attenuation → linear volume. The low 8 bits pick the mantissa,
/// the high bits are a power-of-two right shift (each 256 units = one octave = ÷2). The shift is clamped to
/// 31 (values beyond ~13 already round to silence) so the `u32` shift can never overflow.
fn attenuation_to_volume(atten: u32, pow: &[u16; TABLE_LEN]) -> u32 {
    let shift = (atten >> 8).min(31);
    (pow[(atten & 0xff) as usize] as u32) >> shift
}

/// The per-operator envelope phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EgState {
    /// Not sounding: attenuation pinned at [`MAX_ATT`].
    Off,
    /// Rising volume (attenuation falling toward 0) at the operator's attack rate.
    Attack,
    /// Falling volume (attenuation rising toward the sustain level) at D1R.
    Decay,
    /// Second decay (attenuation rising toward silence) at D2R, after reaching the sustain level.
    Sustain,
    /// Key-off decay (attenuation rising toward silence) at RR.
    Release,
}

/// One FM operator: a phase generator plus an envelope generator and its programmed parameters.
#[derive(Clone, Copy)]
struct Operator {
    // --- programmed parameters ---
    /// `MUL` (`$30` low nibble): frequency multiple. 0 means ×0.5.
    mul: u8,
    /// `TL` (`$40`, 0-127): total level — a fixed attenuation offset (each step = 8 att units ≈ 0.75 dB).
    tl: u8,
    /// `KS` (`$50` bits 6-7): key-scale — steepens the envelope rate with pitch.
    ks: u8,
    /// `AR` (`$50` bits 0-4): attack rate (0-31).
    ar: u8,
    /// `D1R` (`$60` bits 0-4): first decay rate (0-31).
    d1r: u8,
    /// `D2R` (`$70` bits 0-4): second decay / sustain rate (0-31).
    d2r: u8,
    /// `SL` (`$80` bits 4-7): sustain level (0-15); the attenuation at which Decay → Sustain.
    sl: u8,
    /// `RR` (`$80` bits 0-3): release rate (0-15).
    rr: u8,

    // --- runtime state ---
    /// 20-bit phase accumulator (one full sine cycle = `1 << 20`; the top 10 bits index the sine).
    phase: u32,
    /// Current envelope attenuation (0 = loud, [`MAX_ATT`] = silent), kept `f32` for the approximate rates.
    env: f32,
    /// Envelope phase.
    eg: EgState,
    /// This operator's last signed output (`compute_volume`) — for operator-1 self-feedback.
    prev_out: i32,
    /// The output before that (feedback sums the last two).
    prev_out2: i32,
}

impl Operator {
    fn new() -> Self {
        Self {
            mul: 0,
            tl: 0,
            ks: 0,
            ar: 0,
            d1r: 0,
            d2r: 0,
            sl: 0,
            rr: 0,
            phase: 0,
            env: MAX_ATT,
            eg: EgState::Off,
            prev_out: 0,
            prev_out2: 0,
        }
    }

    /// Key this operator on: (re)start the attack from phase 0 (OPN resets operator phase on key-on).
    fn key_on(&mut self) {
        self.eg = EgState::Attack;
        self.phase = 0;
        self.prev_out = 0;
        self.prev_out2 = 0;
    }

    /// Key this operator off: fall into the release phase (unless already fully silent).
    fn key_off(&mut self) {
        if self.eg != EgState::Off {
            self.eg = EgState::Release;
        }
    }

    /// Advance the envelope one native tick using this operator's effective rates (`kc` = channel key-code
    /// for key-scaling). Approximate: linear-in-attenuation decays, faster linear attack (exact EG is SY-3c).
    fn step_envelope(&mut self, kc: u8) {
        match self.eg {
            EgState::Off => {}
            EgState::Attack => {
                let r = effective_rate(self.ar, self.ks, kc);
                let inc = rate_increment(r) * ATTACK_SPEED;
                if inc <= 0.0 {
                    // AR effectively 0 → operator never opens (matches OPN AR=0 "stuck" behaviour).
                    return;
                }
                self.env -= inc;
                if self.env <= 0.0 {
                    self.env = 0.0;
                    self.eg = EgState::Decay;
                }
            }
            EgState::Decay => {
                let r = effective_rate(self.d1r, self.ks, kc);
                self.env += rate_increment(r);
                let sl_att = sustain_att(self.sl);
                if self.env >= sl_att {
                    self.env = sl_att;
                    self.eg = EgState::Sustain;
                }
            }
            EgState::Sustain => {
                let r = effective_rate(self.d2r, self.ks, kc);
                self.env += rate_increment(r);
                if self.env >= MAX_ATT {
                    self.env = MAX_ATT;
                    self.eg = EgState::Off;
                }
            }
            EgState::Release => {
                // RR is 4-bit; the OPN maps it to the 6-bit rate as `(rr << 1) | 1`.
                let r = effective_rate((self.rr << 1) | 1, self.ks, kc);
                self.env += rate_increment(r);
                if self.env >= MAX_ATT {
                    self.env = MAX_ATT;
                    self.eg = EgState::Off;
                }
            }
        }
    }

    /// The 10-bit envelope attenuation (envelope + `TL`) in the OPN attenuation domain. `TL` (7-bit) is
    /// shifted into the 10-bit domain (`<< 3`, each step ≈ 8 units); the sum may exceed [`MAX_ATT`] and
    /// [`attenuation_to_volume`]'s shift saturates it to silence.
    fn env_att(&self) -> u32 {
        let e = self.env.round().clamp(0.0, MAX_ATT) as u32;
        e + ((self.tl as u32) << 3)
    }

    /// The operator's signed output for the current phase, given phase modulation `opmod` (in the 10-bit
    /// phase's units — a modulator's output already right-shifted by the caller). This is ymfm's
    /// `compute_volume`: log-sin(phase) + `env·4` → exp table → sign from the phase quadrant (bit 9).
    fn compute(&self, opmod: i32, log_sin: &[u16; TABLE_LEN], pow: &[u16; TABLE_LEN]) -> i32 {
        if self.eg == EgState::Off {
            return 0;
        }
        let phase10 = ((self.phase >> 10) as i32).wrapping_add(opmod) as u32;
        let sin_att = abs_sin_attenuation(phase10, log_sin);
        // Envelope/TL live in the 10-bit attenuation domain; `<< 2` converts to the exp table's 4×-finer
        // units (256 units = one octave) before summing with the already-exp-domain log-sin attenuation.
        let vol = attenuation_to_volume(sin_att + (self.env_att() << 2), pow) as i32;
        if phase10 & 0x200 != 0 {
            -vol
        } else {
            vol
        }
    }

    /// Advance the 20-bit phase accumulator by `inc` (wrapping at one cycle = `1 << 20`).
    fn advance(&mut self, inc: u32) {
        self.phase = (self.phase.wrapping_add(inc)) & 0x000F_FFFF;
    }
}

/// One FM channel: four operators, pitch, algorithm/feedback, and stereo pan.
#[derive(Clone, Copy)]
struct Channel {
    /// Operators in algorithm order: index 0 = Op1 … index 3 = Op4.
    ops: [Operator; 4],
    /// 11-bit F-number.
    fnum: u16,
    /// 3-bit block (octave).
    block: u8,
    /// The `$A4-$A6` latch (block + fnum high 3 bits) pending until the `$A0-$A2` low-byte write.
    fnum_hi_latch: u8,
    /// Algorithm (0-7).
    algorithm: u8,
    /// Operator-1 self-feedback level (0-7).
    feedback: u8,
    /// Left-channel enable (`$B4` bit7).
    pan_l: bool,
    /// Right-channel enable (`$B4` bit6).
    pan_r: bool,
}

impl Channel {
    fn new() -> Self {
        Self {
            ops: [Operator::new(); 4],
            fnum: 0,
            block: 0,
            fnum_hi_latch: 0,
            algorithm: 0,
            feedback: 0,
            pan_l: true,
            pan_r: true,
        }
    }

    /// The channel key-code (5-bit) for envelope key-scaling: `block` in bits 4-2, fnum top bits in 1-0.
    fn key_code(&self) -> u8 {
        (self.block << 2) | ((self.fnum >> 9) & 0x03) as u8
    }

    /// The per-native-tick phase increment for `MUL=1` (before the per-operator MUL): `fnum · 2^block`, in
    /// the 20-bit phase's units. The MUL factor is applied per operator in [`Self::op_inc`].
    ///
    /// Derivation: `f = fnum · 2^(block-1) · NATIVE_RATE / 2^20`, so per native tick the phase (2^20 per
    /// cycle) advances `fnum · 2^(block-1)`. Carried here as `fnum << block` (= `2·` that) so `op_inc`'s
    /// `× MUL·2 >> 2` yields `fnum · 2^(block-1) · MUL` (and `MUL=0` → `× 0.5`).
    fn base_phase_step(&self) -> u32 {
        (self.fnum as u32) << self.block
    }

    /// The per-operator phase increment: `base · (2·MUL) >> 2` — i.e. `fnum · 2^(block-1) · MUL`, with
    /// `MUL=0` giving `× 0.5` (the OPN "MUL=0 means half" quirk).
    fn op_inc(base: u32, mul: u8) -> u32 {
        let mul_x2 = if mul == 0 { 1 } else { 2 * mul as u32 };
        (base * mul_x2) >> 2
    }

    /// Advance this channel by one native tick and return its `(left, right)` FM contribution (already
    /// FM-scaled and panned). Envelopes clock first, then operator outputs are computed at the current
    /// phases (OPN2 log-sin/exp pipeline, ymfm modulation units), then all phases advance.
    fn tick(&mut self, log_sin: &[u16; TABLE_LEN], pow: &[u16; TABLE_LEN]) -> (i32, i32) {
        let kc = self.key_code();
        for op in self.ops.iter_mut() {
            op.step_envelope(kc);
        }
        let base = self.base_phase_step();

        // Operator-1 self-feedback: sum the last two Op1 outputs, right-shifted by `10 - feedback` (ymfm).
        let fb = if self.feedback == 0 {
            0
        } else {
            (self.ops[0].prev_out + self.ops[0].prev_out2) >> (10 - self.feedback)
        };
        let out1 = self.ops[0].compute(fb, log_sin, pow);
        self.ops[0].prev_out2 = self.ops[0].prev_out;
        self.ops[0].prev_out = out1;

        // The 8 FM algorithms wire Op1..Op4 into modulator/carrier roles. A modulator's signed output is
        // right-shifted by 1 (ymfm) before it is summed into the modulated operator's phase; carriers sum.
        let carriers: i32 = match self.algorithm {
            0 => {
                // Op1→Op2→Op3→Op4→out (serial).
                let o2 = self.ops[1].compute(out1 >> 1, log_sin, pow);
                let o3 = self.ops[2].compute(o2 >> 1, log_sin, pow);
                self.ops[3].compute(o3 >> 1, log_sin, pow)
            }
            1 => {
                // (Op1+Op2)→Op3→Op4→out.
                let o2 = self.ops[1].compute(0, log_sin, pow);
                let o3 = self.ops[2].compute((out1 + o2) >> 1, log_sin, pow);
                self.ops[3].compute(o3 >> 1, log_sin, pow)
            }
            2 => {
                // Op1→Op4, Op2→Op3→Op4→out.
                let o2 = self.ops[1].compute(0, log_sin, pow);
                let o3 = self.ops[2].compute(o2 >> 1, log_sin, pow);
                self.ops[3].compute((out1 + o3) >> 1, log_sin, pow)
            }
            3 => {
                // Op1→Op2→Op4, Op3→Op4→out.
                let o2 = self.ops[1].compute(out1 >> 1, log_sin, pow);
                let o3 = self.ops[2].compute(0, log_sin, pow);
                self.ops[3].compute((o2 + o3) >> 1, log_sin, pow)
            }
            4 => {
                // Op1→Op2→out, Op3→Op4→out (two parallel chains).
                let o2 = self.ops[1].compute(out1 >> 1, log_sin, pow);
                let o3 = self.ops[2].compute(0, log_sin, pow);
                let o4 = self.ops[3].compute(o3 >> 1, log_sin, pow);
                o2 + o4
            }
            5 => {
                // Op1 modulates Op2, Op3, Op4; all three → out.
                let o2 = self.ops[1].compute(out1 >> 1, log_sin, pow);
                let o3 = self.ops[2].compute(out1 >> 1, log_sin, pow);
                let o4 = self.ops[3].compute(out1 >> 1, log_sin, pow);
                o2 + o3 + o4
            }
            6 => {
                // Op1→Op2→out; Op3→out; Op4→out.
                let o2 = self.ops[1].compute(out1 >> 1, log_sin, pow);
                let o3 = self.ops[2].compute(0, log_sin, pow);
                let o4 = self.ops[3].compute(0, log_sin, pow);
                o2 + o3 + o4
            }
            _ => {
                // Algorithm 7: all four operators → out (fully additive).
                let o2 = self.ops[1].compute(0, log_sin, pow);
                let o3 = self.ops[2].compute(0, log_sin, pow);
                let o4 = self.ops[3].compute(0, log_sin, pow);
                out1 + o2 + o3 + o4
            }
        };

        // Advance every operator's phase for the next tick.
        let inc0 = Self::op_inc(base, self.ops[0].mul);
        self.ops[0].advance(inc0);
        let inc1 = Self::op_inc(base, self.ops[1].mul);
        self.ops[1].advance(inc1);
        let inc2 = Self::op_inc(base, self.ops[2].mul);
        self.ops[2].advance(inc2);
        let inc3 = Self::op_inc(base, self.ops[3].mul);
        self.ops[3].advance(inc3);

        // Scale the summed carriers to the output loudness and route through the stereo pan.
        let s = ((carriers as i64 * FM_LEVEL_Q15) >> 15) as i32;
        let l = if self.pan_l { s } else { 0 };
        let r = if self.pan_r { s } else { 0 };
        (l, r)
    }
}

/// The full YM2612 FM synthesizer: 6 channels, the register-write decode, the OPN2 lookup tables, and the
/// native-rate → output-rate resampler.
pub struct Ym2612Synth {
    /// The 6 FM channels (channels 0-2 = part I / bank 0, channels 3-5 = part II / bank 1).
    channels: [Channel; 6],
    /// DAC (channel-6 PCM) enable, `$2B` bit7. When set, FM channel 6 is muted and the DAC PCM stream plays
    /// in its place (SY-3a).
    dac_enabled: bool,
    /// DAC sample bytes (`$2A` writes) collected during the current frame, in write order.
    dac_queue: Vec<u8>,
    /// The frame's DAC bytes, snapshotted from [`Self::dac_queue`] at the frame boundary; output sample `i`
    /// plays `dac_frame[i · N / samples_per_frame]` (zero-order hold across the frame).
    dac_frame: Vec<u8>,
    /// Output samples the current frame's [`Self::dac_frame`] is spread across (set by [`Self::begin_frame`]).
    dac_samples_per_frame: u32,
    /// The output-sample index within the current frame, advanced per sample.
    dac_sample_idx: u32,
    /// The most-recent DAC byte written; held (ZOH) across a DAC-enabled frame with zero `$2A` writes.
    dac_last: u8,
    /// 256-entry log-sin attenuation table (ymfm `s_sin_table`).
    log_sin: [u16; TABLE_LEN],
    /// 256-entry exp/pow table (ymfm `s_power_table`, `X` macro applied).
    pow: [u16; TABLE_LEN],
    /// Resampler: native ticks consumed per output sample (`NATIVE_RATE / output_rate ≈ 1.208`).
    native_per_output: f64,
    /// Resampler: fractional native-tick position in `[0, 1)` between [`Self::prev_native`] and
    /// [`Self::cur_native`].
    resample_frac: f64,
    /// Resampler: the older bracketing native FM sample `(left, right)`.
    prev_native: (i32, i32),
    /// Resampler: the newer bracketing native FM sample `(left, right)`.
    cur_native: (i32, i32),
}

/// Register-address → operator-index map. The low-nibble operator field of an `$30-$9F` write addresses
/// hardware slots in the order S1,S3,S2,S4; this maps that field (0-3) to the algorithm operator index
/// (Op1..Op4 → 0..3).
const SLOT_MAP: [usize; 4] = [0, 2, 1, 3];

impl Ym2612Synth {
    /// A fresh FM synth producing `sample_rate` Hz output (all channels silent at reset).
    pub fn new(sample_rate: u32) -> Self {
        Self {
            channels: [Channel::new(); 6],
            dac_enabled: false,
            dac_queue: Vec::new(),
            dac_frame: Vec::new(),
            dac_samples_per_frame: 0,
            dac_sample_idx: 0,
            dac_last: 0x80,
            log_sin: build_log_sin(),
            pow: build_pow(),
            native_per_output: NATIVE_RATE / sample_rate as f64,
            resample_frac: 0.0,
            prev_native: (0, 0),
            cur_native: (0, 0),
        }
    }

    /// Feed one decoded FM register write: `bank` (0 = part I / channels 0-2, 1 = part II / channels 3-5),
    /// the latched register `reg`, and its `value` — the identical triple the VGM path records.
    pub fn write(&mut self, bank: u8, reg: u8, value: u8) {
        match reg {
            // --- bank-0-only global registers ---
            0x28 if bank == 0 => self.key_on_off(value),
            // $2A DAC data: queue the 8-bit PCM sample for this frame's channel-6 ZOH playback (SY-3a).
            0x2A if bank == 0 => {
                self.dac_queue.push(value);
                self.dac_last = value;
            }
            0x2B if bank == 0 => self.dac_enabled = value & 0x80 != 0,
            // Per-channel and per-operator registers exist in both banks.
            0x30..=0x9F => self.write_operator(bank, reg, value),
            0xA0..=0xA2 => self.write_fnum_low(bank, reg, value),
            0xA4..=0xA6 => self.write_fnum_high(bank, reg, value),
            0xB0..=0xB2 => self.write_alg_feedback(bank, reg, value),
            0xB4..=0xB6 => self.write_pan(bank, reg, value),
            // Timers ($24-$27), LFO ($22), ch3-special ($A8-$AE), SSG handled above/ignored.
            _ => {}
        }
    }

    /// `$28` key on/off: low 3 bits select the channel (0-2 = ch 1-3, 4-6 = ch 4-6); bits 4-7 are the
    /// per-operator key mask (bit4 = Op1 … bit7 = Op4).
    fn key_on_off(&mut self, value: u8) {
        let ch = match value & 0x07 {
            0 => 0,
            1 => 1,
            2 => 2,
            4 => 3,
            5 => 4,
            6 => 5,
            _ => return, // 3 and 7 are invalid channel selectors.
        };
        let channel = &mut self.channels[ch];
        for op in 0..4 {
            if value & (0x10 << op) != 0 {
                channel.ops[op].key_on();
            } else {
                channel.ops[op].key_off();
            }
        }
    }

    /// The global channel index for a per-channel register: `bank·3 + (reg & 3)`. Returns `None` for the
    /// invalid `reg & 3 == 3` slot.
    fn channel_index(bank: u8, reg: u8) -> Option<usize> {
        let within = (reg & 0x03) as usize;
        if within == 3 {
            return None;
        }
        Some(bank as usize * 3 + within)
    }

    /// Decode + apply an operator register (`$30-$9F`): the low nibble carries channel (bits 0-1) and
    /// operator-address-field (bits 2-3, remapped via [`SLOT_MAP`]); the high nibble selects the parameter.
    fn write_operator(&mut self, bank: u8, reg: u8, value: u8) {
        let Some(ch) = Self::channel_index(bank, reg) else {
            return;
        };
        let op_field = ((reg >> 2) & 0x03) as usize;
        let op = SLOT_MAP[op_field];
        let o = &mut self.channels[ch].ops[op];
        match reg & 0xF0 {
            0x30 => o.mul = value & 0x0F, // (detune bits 4-6 deferred to SY-3d)
            0x40 => o.tl = value & 0x7F,
            0x50 => {
                o.ks = (value >> 6) & 0x03;
                o.ar = value & 0x1F;
            }
            0x60 => o.d1r = value & 0x1F, // (AM bit7 deferred with the LFO)
            0x70 => o.d2r = value & 0x1F,
            0x80 => {
                o.sl = (value >> 4) & 0x0F;
                o.rr = value & 0x0F;
            }
            0x90 => {} // SSG-EG deferred to SY-3f.
            _ => {}
        }
    }

    /// `$A0-$A2` F-number low byte: combines with the pending `$A4` block/fnum-hi latch to set pitch.
    fn write_fnum_low(&mut self, bank: u8, reg: u8, value: u8) {
        let Some(ch) = Self::channel_index(bank, reg) else {
            return;
        };
        let c = &mut self.channels[ch];
        c.fnum = (((c.fnum_hi_latch & 0x07) as u16) << 8) | value as u16;
        c.block = (c.fnum_hi_latch >> 3) & 0x07;
    }

    /// `$A4-$A6` block + F-number high bits: latched, applied on the following `$A0` write (as on hardware).
    fn write_fnum_high(&mut self, bank: u8, reg: u8, value: u8) {
        let Some(ch) = Self::channel_index(bank, reg) else {
            return;
        };
        self.channels[ch].fnum_hi_latch = value;
    }

    /// `$B0-$B2` algorithm (bits 0-2) + operator-1 feedback (bits 3-5).
    fn write_alg_feedback(&mut self, bank: u8, reg: u8, value: u8) {
        let Some(ch) = Self::channel_index(bank, reg) else {
            return;
        };
        let c = &mut self.channels[ch];
        c.algorithm = value & 0x07;
        c.feedback = (value >> 3) & 0x07;
    }

    /// `$B4-$B6` stereo pan (bit7 = left, bit6 = right). (AMS/FMS bits deferred with the LFO.)
    fn write_pan(&mut self, bank: u8, reg: u8, value: u8) {
        let Some(ch) = Self::channel_index(bank, reg) else {
            return;
        };
        let c = &mut self.channels[ch];
        c.pan_l = value & 0x80 != 0;
        c.pan_r = value & 0x40 != 0;
    }

    /// Begin a new render frame (SY-3a): snapshot this frame's queued DAC bytes for even-spread ZOH playback
    /// and reset the per-sample index. The sink calls this once, before the frame's `samples_per_frame`
    /// [`Self::next_sample`] calls.
    pub fn begin_frame(&mut self, samples_per_frame: u32) {
        self.dac_frame.clear();
        self.dac_frame.append(&mut self.dac_queue); // moves the bytes out and leaves `dac_queue` empty
        self.dac_samples_per_frame = samples_per_frame;
        self.dac_sample_idx = 0;
    }

    /// The DAC (channel-6 PCM) contribution `(left, right)` for the current output sample. Even-spread ZOH:
    /// output sample `i` plays `dac_frame[i · N / samples_per_frame]` (clamped to the last byte); a frame
    /// with no `$2A` writes holds [`Self::dac_last`]. The 8-bit unsigned byte is centered around `0x80` and
    /// scaled by [`DAC_SCALE`], then routed through channel 6's `$B6` stereo pan.
    fn dac_sample(&self) -> (i32, i32) {
        let byte = if self.dac_frame.is_empty() {
            self.dac_last
        } else {
            let n = self.dac_frame.len();
            let spf = self.dac_samples_per_frame.max(1) as usize;
            let idx = (self.dac_sample_idx as usize * n / spf).min(n - 1);
            self.dac_frame[idx]
        };
        let s = (byte as i32 - 128) * DAC_SCALE;
        let ch6 = &self.channels[5];
        let l = if ch6.pan_l { s } else { 0 };
        let r = if ch6.pan_r { s } else { 0 };
        (l, r)
    }

    /// Advance the whole FM chip by one **native** operator tick and return the summed `(left, right)` FM
    /// output. Channel 6's FM is skipped while the DAC drives it (`$2B` bit7); the PCM stream is added at the
    /// output rate in [`Self::next_sample`].
    fn tick_native(&mut self) -> (i32, i32) {
        let Ym2612Synth {
            channels,
            log_sin,
            pow,
            dac_enabled,
            ..
        } = self;
        let mut l = 0i32;
        let mut r = 0i32;
        for (i, ch) in channels.iter_mut().enumerate() {
            if i == 5 && *dac_enabled {
                continue;
            }
            let (cl, cr) = ch.tick(log_sin, pow);
            l += cl;
            r += cr;
        }
        (l, r)
    }

    /// Produce one stereo output sample `(left, right)` at the sink's output rate.
    ///
    /// The FM chip is clocked at its native ~53_267 Hz inside [`Self::tick_native`]; this method consumes
    /// `native_per_output ≈ 1.208` native ticks per call and **linearly interpolates** between the two
    /// bracketing native samples to resample to the output rate (cutting the aliasing the old direct-at-output
    /// path had). The SY-3a DAC contribution is added afterwards, at the output rate, unchanged.
    pub fn next_sample(&mut self) -> (i32, i32) {
        self.resample_frac += self.native_per_output;
        while self.resample_frac >= 1.0 {
            self.resample_frac -= 1.0;
            self.prev_native = self.cur_native;
            self.cur_native = self.tick_native();
        }
        let f = self.resample_frac as f32;
        let mut li = lerp(self.prev_native.0, self.cur_native.0, f);
        let mut ri = lerp(self.prev_native.1, self.cur_native.1, f);
        if self.dac_enabled {
            let (dl, dr) = self.dac_sample();
            li += dl;
            ri += dr;
        }
        self.dac_sample_idx += 1;
        (li, ri)
    }
}

/// Linear interpolation between two integer samples: `a + (b - a) · f`, `f ∈ [0, 1)`.
fn lerp(a: i32, b: i32, f: f32) -> i32 {
    a + ((b - a) as f32 * f) as i32
}

/// The effective 6-bit envelope rate: `2·base + keyscale`, clamped to 63. `base` is a 5-bit rate register
/// (or the RR-derived value); `ks`/`kc` add the key-scale contribution. Approximate (exact table is SY-3c).
fn effective_rate(base: u8, ks: u8, kc: u8) -> u8 {
    if base == 0 {
        return 0;
    }
    let ks_add = kc >> (3 - ks.min(3));
    (2 * base as u16 + ks_add as u16).min(63) as u8
}

/// Attenuation units added per **native tick** at effective envelope `rate` (0-63). Rate 0 = frozen.
/// Exponential in the rate (each +4 rate ≈ ×2 speed) — a calibrated approximation of the OPN rate table,
/// rescaled by [`ENV_RATE_SCALE`] so the native tick preserves the SY-2 wall-clock envelope timing.
fn rate_increment(rate: u8) -> f32 {
    if rate == 0 {
        0.0
    } else {
        RATE_BASE * 2.0f32.powf(rate as f32 / 4.0) * ENV_RATE_SCALE
    }
}

/// The attenuation (in the 10-bit domain) at which Decay hands off to Sustain, from the 4-bit `SL` field:
/// each step ≈ 32 units (≈ 3 dB); `SL=15` pins to the maximum (silence).
fn sustain_att(sl: u8) -> f32 {
    if sl >= 15 {
        MAX_ATT
    } else {
        (sl as f32) * 32.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated log-sin table must match ymfm's literal `s_sin_table` (verified verbatim against
    /// `src/ymfm_fm.ipp`). This pins the table-generation formula to the ymfm fixture (Fork-1 requirement).
    #[test]
    fn log_sin_table_matches_ymfm() {
        let t = build_log_sin();
        // Entries 0-15, quoted verbatim from ymfm's s_sin_table first line.
        let first16: [u16; 16] = [
            0x859, 0x6c3, 0x607, 0x58b, 0x52e, 0x4e4, 0x4a6, 0x471, 0x443, 0x41a, 0x3f5, 0x3d3,
            0x3b5, 0x398, 0x37e, 0x365,
        ];
        for (i, &v) in first16.iter().enumerate() {
            assert_eq!(t[i], v, "log_sin[{i}] must equal ymfm s_sin_table[{i}]");
        }
        // The tail (loudest, sin near ±1 → ~0 attenuation), verified against ymfm's final row.
        assert_eq!(t[247], 0x001, "log_sin[247]");
        assert_eq!(t[248], 0x000, "log_sin[248]");
        assert_eq!(t[255], 0x000, "log_sin[255]");
        // A couple of mid values (the formula reproduces ymfm exactly across the whole table).
        assert_eq!(t[64], 0x160, "log_sin[64]");
        assert_eq!(t[128], 0x07f, "log_sin[128]");
    }

    /// The generated exp/pow table must match ymfm's literal `s_power_table` (before and after the `X`
    /// macro), verified verbatim against `src/ymfm_fm.ipp`.
    #[test]
    fn pow_table_matches_ymfm() {
        let raw = build_pow_raw();
        // ymfm s_power_table first 8 and last 8 (the `a` inside `X(a)`).
        let first8: [u16; 8] = [0x3fa, 0x3f5, 0x3ef, 0x3ea, 0x3e4, 0x3df, 0x3da, 0x3d4];
        let last8: [u16; 8] = [0x014, 0x011, 0x00e, 0x00b, 0x008, 0x006, 0x003, 0x000];
        for (i, &v) in first8.iter().enumerate() {
            assert_eq!(raw[i], v, "pow_raw[{i}]");
        }
        for (i, &v) in last8.iter().enumerate() {
            assert_eq!(raw[248 + i], v, "pow_raw[{}]", 248 + i);
        }
        // Post-macro anchor: X(0x3fa) = (0x3fa | 0x400) << 2 = 0x1FE8 (loudest volume).
        let pow = build_pow();
        assert_eq!(pow[0], 0x1FE8, "pow[0] = X(0x3fa)");
        // attenuation_to_volume: zero attenuation is loudest; a large attenuation saturates to silence.
        assert_eq!(attenuation_to_volume(0, &pow), 0x1FE8);
        assert_eq!(attenuation_to_volume(0x2000, &pow), 0);
        // The shift clamp must never overflow, even for absurd attenuation.
        assert_eq!(attenuation_to_volume(0xFFFF, &pow), 0);
    }

    /// abs_sin_attenuation folds the quarter wave: index 0 (loudest? no — sin near 0, quietest) mirrors at
    /// bit 8, and the sign comes from bit 9. Spot-check the fold symmetry and that a full 1024-phase spans a
    /// sign flip.
    #[test]
    fn abs_sin_fold_and_sign() {
        let t = build_log_sin();
        // Bit-8 fold: phase 0x0FF and 0x100 both map to table[0xff] (mirror around the quarter boundary).
        assert_eq!(abs_sin_attenuation(0x0FF, &t), t[0xff] as u32);
        assert_eq!(abs_sin_attenuation(0x100, &t), t[0xff] as u32);
        // Phase 0 → table[0] (quietest edge), phase 0x1FF → also table[0] by the fold.
        assert_eq!(abs_sin_attenuation(0x000, &t), t[0] as u32);
        assert_eq!(abs_sin_attenuation(0x1FF, &t), t[0] as u32);
    }

    /// Program channel 0, operator 1 (algorithm 7 additive so Op1 is a bare carrier) to a known pitch and
    /// key it on; render one second and count zero crossings — proving the phase generator (native tick +
    /// resample) emits the expected fundamental frequency.
    #[test]
    fn operator_produces_expected_pitch() {
        let sample_rate = 44_100u32;
        let mut fm = Ym2612Synth::new(sample_rate);

        // Algorithm 7 (all operators are carriers), no feedback: $B0 = 0x07.
        fm.write(0, 0xB0, 0x07);
        // Op1 params: MUL=1 (so pitch is exact), TL=0 (loudest), AR=31 (instant attack),
        // D1R=0/D2R=0 (no decay), SL=0, RR=0 → a sustained full-volume tone.
        fm.write(0, 0x30, 0x01); // MUL=1 for operator-address-field 0 (Op1) of channel 0
        fm.write(0, 0x40, 0x00); // TL=0
        fm.write(0, 0x50, 0x1F); // KS=0, AR=31
        fm.write(0, 0x60, 0x00); // D1R=0
        fm.write(0, 0x70, 0x00); // D2R=0
        fm.write(0, 0x80, 0x00); // SL=0, RR=0
                                 // Pitch: fnum=1083, block=4 → f = 1083·8·(7670453/144)/2^20 ≈ 440.1 Hz.
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        fm.write(0, 0xB4, 0xC0); // stereo both

        fm.write(0, 0x28, 0x10); // key on Op1 only: channel 0, mask bit4

        // Render one second; count left-channel sign changes once the attack has opened.
        let mut crossings = 0u32;
        let mut prev = 0i32;
        let mut have_prev = false;
        for _ in 0..sample_rate {
            let (l, _r) = fm.next_sample();
            if have_prev && (l < 0) != (prev < 0) && l != 0 {
                crossings += 1;
            }
            if l != 0 {
                prev = l;
                have_prev = true;
            }
        }

        // A full cycle = 2 crossings → expect ≈ 2·440 ≈ 880/sec. Within-octave (440-880 Hz) requires
        // 880..1760 crossings; this tight band proves sample-accurate pitch, not just within-octave.
        assert!(
            (860..=900).contains(&crossings),
            "expected ~880 zero crossings for a 440 Hz operator, got {crossings}"
        );
    }

    /// A channel that is never keyed on must be pure silence.
    #[test]
    fn unkeyed_channel_is_silent() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0xB0, 0x07);
        fm.write(0, 0x30, 0x01);
        fm.write(0, 0x40, 0x00);
        fm.write(0, 0x50, 0x1F);
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        for _ in 0..2000 {
            assert_eq!(fm.next_sample(), (0, 0), "unkeyed channel must be silent");
        }
    }

    /// A carrier at maximum TL (127) is attenuated to inaudibility even when keyed on.
    #[test]
    fn max_total_level_is_effectively_silent() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0xB0, 0x07);
        fm.write(0, 0x30, 0x01);
        fm.write(0, 0x40, 0x7F); // TL=127 → maximum attenuation
        fm.write(0, 0x50, 0x1F); // AR=31
        fm.write(0, 0x80, 0x00);
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        fm.write(0, 0xB4, 0xC0);
        fm.write(0, 0x28, 0x10);
        let mut peak = 0i32;
        for _ in 0..4410 {
            let (l, _r) = fm.next_sample();
            peak = peak.max(l.abs());
        }
        assert_eq!(peak, 0, "TL=127 must render as silence, peak was {peak}");
    }

    /// After key-off the envelope must release toward silence: a keyed-on carrier with a finite release
    /// rate falls to zero output within a bounded time.
    #[test]
    fn key_off_decays_to_silence() {
        let sample_rate = 44_100u32;
        let mut fm = Ym2612Synth::new(sample_rate);
        fm.write(0, 0xB0, 0x07);
        fm.write(0, 0x30, 0x01);
        fm.write(0, 0x40, 0x00); // TL=0
        fm.write(0, 0x50, 0x1F); // AR=31 (fast attack)
        fm.write(0, 0x60, 0x00); // D1R=0
        fm.write(0, 0x70, 0x00); // D2R=0
        fm.write(0, 0x80, 0x0F); // SL=0, RR=15 (fast release)
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        fm.write(0, 0xB4, 0xC0);
        fm.write(0, 0x28, 0x10); // key on

        // Let it reach full volume.
        let mut peak_on = 0i32;
        for _ in 0..2205 {
            let (l, _r) = fm.next_sample();
            peak_on = peak_on.max(l.abs());
        }
        assert!(
            peak_on > 100,
            "keyed-on carrier must be audible, peak {peak_on}"
        );

        // Key off Op1 (mask bits all clear → 0x00 for channel 0).
        fm.write(0, 0x28, 0x00);
        // Render up to ~1 s; the tail must fall to exact silence.
        let mut last_nonzero = 0u32;
        for i in 0..sample_rate {
            let (l, r) = fm.next_sample();
            if l != 0 || r != 0 {
                last_nonzero = i;
            }
        }
        assert!(
            last_nonzero < sample_rate - 1,
            "release must decay to silence; still sounding at sample {last_nonzero}"
        );
    }

    /// Stereo pan: right-only ($B4 bit6) must put the signal on the right channel and zero on the left.
    #[test]
    fn pan_routes_to_one_side() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0xB0, 0x07);
        fm.write(0, 0x30, 0x01);
        fm.write(0, 0x40, 0x00);
        fm.write(0, 0x50, 0x1F);
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        fm.write(0, 0xB4, 0x40); // right only (bit6), left off
        fm.write(0, 0x28, 0x10);
        let mut left_energy = 0i64;
        let mut right_energy = 0i64;
        for _ in 0..4410 {
            let (l, r) = fm.next_sample();
            left_energy += l.abs() as i64;
            right_energy += r.abs() as i64;
        }
        assert_eq!(left_energy, 0, "left must be silent for a right-only pan");
        assert!(right_energy > 0, "right must carry the signal");
    }

    /// DAC-enable ($2B bit7) mutes the FM voice of channel 6 (index 5): with the DAC enabled but *no* $2A
    /// samples written (queue empty → holds the 0x80 center), channel 6 emits no stale FM tone (silence).
    #[test]
    fn dac_enable_mutes_channel_six_fm() {
        let mut fm = Ym2612Synth::new(44_100);
        // Program + key channel 6 (bank 1, ch index within-part 2 → global 5) with a loud carrier.
        fm.write(1, 0xB2, 0x07); // algorithm 7 for channel 6
        fm.write(1, 0x32, 0x01); // MUL=1, Op1, ch within-part 2
        fm.write(1, 0x42, 0x00); // TL=0
        fm.write(1, 0x52, 0x1F); // AR=31
        fm.write(1, 0xA6, 0x24); // block/fnum-hi
        fm.write(1, 0xA2, 0x3B); // fnum low
        fm.write(1, 0xB6, 0xC0); // pan both
        fm.write(0, 0x28, 0x16); // key on: channel selector 6 (=ch index 5), Op1 mask bit4

        // Enable DAC (no $2A data yet → DAC holds the 0x80 center = 0) → channel 6 must be silent.
        fm.write(0, 0x2B, 0x80);
        let mut peak = 0i32;
        for _ in 0..4410 {
            fm.begin_frame(735);
            let (l, r) = fm.next_sample();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert_eq!(
            peak, 0,
            "DAC-enabled channel 6 FM must be muted, peak {peak}"
        );
    }

    /// SY-3a: with the DAC enabled, a frame of $2A writes streams on channel 6 as signed PCM. The first
    /// output sample is the first byte and the last is the last byte (FM otherwise silent, so the samples
    /// are the DAC alone — the FM resampler contributes exactly 0).
    #[test]
    fn dac_streams_written_bytes() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0x2B, 0x80); // enable DAC (bit7)
        fm.write(1, 0xB6, 0xC0); // ch6 pan both so the PCM reaches L and R

        fm.write(0, 0x2A, 0xC0); // 0xC0 - 128 = +64 → positive
        fm.write(0, 0x2A, 0x40); // 0x40 - 128 = -64 → negative

        fm.begin_frame(735);
        let (l0, r0) = fm.next_sample();
        assert_eq!(
            l0,
            (0xC0i32 - 128) * DAC_SCALE,
            "first sample = first DAC byte"
        );
        assert_eq!(r0, l0, "ch6 pan-both duplicates DAC to L and R");
        assert!(l0 > 0, "0xC0 is above center → positive");

        let mut last = (0, 0);
        for _ in 1..735 {
            last = fm.next_sample();
        }
        assert_eq!(
            last.0,
            (0x40i32 - 128) * DAC_SCALE,
            "last sample = last DAC byte"
        );
        assert!(last.0 < 0, "0x40 is below center → negative");
    }

    /// SY-3a: the DAC path is inert while disabled. With the DAC OFF, writing $2A bytes changes nothing —
    /// channel 6 stays pure FM (here fully unprogrammed → silence), proving $2A is ignored unless enabled.
    #[test]
    fn dac_disabled_ignores_2a() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0x2A, 0xFF);
        fm.write(0, 0x2A, 0x00);
        fm.write(1, 0xB6, 0xC0);
        let mut peak = 0i32;
        for _ in 0..735 {
            fm.begin_frame(735);
            let (l, r) = fm.next_sample();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert_eq!(peak, 0, "DAC disabled → $2A writes must produce no output");
    }

    /// SY-3a: even-spread ZOH covers every output sample of the frame. N bytes spread across 735 samples →
    /// the first sample is byte 0, the last is byte N-1, and every sample maps to a valid in-range byte.
    #[test]
    fn dac_zoh_even_spread_covers_frame() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0x2B, 0x80);
        fm.write(1, 0xB6, 0xC0);
        let bytes: [u8; 5] = [0x90, 0xA0, 0xB0, 0xC0, 0xD0];
        for &b in &bytes {
            fm.write(0, 0x2A, b);
        }
        fm.begin_frame(735);

        let mut seen = [false; 5];
        let mut samples = Vec::with_capacity(735);
        for _ in 0..735 {
            samples.push(fm.next_sample().0);
        }
        assert_eq!(samples[0], (bytes[0] as i32 - 128) * DAC_SCALE);
        assert_eq!(samples[734], (bytes[4] as i32 - 128) * DAC_SCALE);
        for &s in &samples {
            let mut matched = false;
            for (i, &b) in bytes.iter().enumerate() {
                if s == (b as i32 - 128) * DAC_SCALE {
                    seen[i] = true;
                    matched = true;
                }
            }
            assert!(
                matched,
                "ZOH sample {s} is not one of the programmed DAC bytes"
            );
        }
        assert!(
            seen.iter().all(|&b| b),
            "even spread must cover every byte: {seen:?}"
        );
    }

    /// SY-3a: an empty ($2A-less) DAC frame holds the last written value (ZOH across frames), and toggling
    /// $2B enables/disables the whole DAC path.
    #[test]
    fn dac_hold_last_and_toggle() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(1, 0xB6, 0xC0);
        fm.write(0, 0x2B, 0x80); // enable
        fm.write(0, 0x2A, 0xE0); // last value 0xE0 → +96
        fm.begin_frame(735);
        let _ = fm.next_sample();

        // Next frame: no $2A writes → the DAC holds 0xE0.
        fm.begin_frame(735);
        let (l, _r) = fm.next_sample();
        assert_eq!(
            l,
            (0xE0i32 - 128) * DAC_SCALE,
            "empty frame holds last DAC byte"
        );

        // Disable the DAC → ch6 returns to (silent, unprogrammed) FM; the held value no longer sounds.
        fm.write(0, 0x2B, 0x00);
        fm.begin_frame(735);
        let (l2, r2) = fm.next_sample();
        assert_eq!((l2, r2), (0, 0), "$2B bit7 clear disables the DAC path");
    }
}
