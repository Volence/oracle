//! Hand-rolled **minimal** YM2612 (OPN2) FM synthesizer — Phase SY-2.
//!
//! The Genesis FM chip is a 4-operator, 6-channel phase-modulation (FM) synthesizer. This module turns
//! the **same `(bank, reg, value)` register-write triples** the [`crate::vgm::VgmLogger`] decodes into PCM
//! — it is the FM counterpart of the SY-1 [`Sn76489`](super::sn76489::Sn76489) hand-roll. The bar is
//! **recognizable music**, not cycle-accuracy: the accurate ymfm-grade port is SY-3.
//!
//! ## Model (what SY-2 implements)
//!
//! - **6 channels × 4 operators.** Each operator is a sine phase-generator plus an ADSR-ish envelope.
//! - **Phase generator.** Per-channel `fnum`/`block` (`$A0-$A2` low byte + `$A4-$A6` block/fnum-hi latch)
//!   plus per-operator `MUL` (`$30`) give each operator a real-Hz frequency; phase advances at the output
//!   sample rate. Frequency = `fnum · 2^(block-1) · (7_670_453 / 144) / 2^20 · mul`.
//! - **Envelope generator.** Attack / Decay / Sustain / Release keyed by `$28`, in the OPN 10-bit
//!   attenuation domain (0 = loud, 1023 = silent), with `AR/D1R/SL/D2R/RR` (`$50/$60/$70/$80`) driving
//!   approximate per-sample rates and `TL` (`$40`) as a fixed offset. Rates are *approximated* (see
//!   deferred list) — the exact OPN rate/key-scale tables are SY-3.
//! - **The 8 FM algorithms + operator-1 self-feedback** (`$B0`), and **stereo L/R pan** (`$B4`).
//!
//! ## Deferred to SY-3 (documented inaccuracies, not bugs)
//!
//! - **DAC / PCM channel-6** (`$2A` stream, `$2B` enable): the PCM sample path is deferred; SY-2 only
//!   **mutes FM channel 6 while DAC is enabled** (as the hardware does) so it does not emit stale tones.
//! - **LFO** (`$22` AMS/FMS), **SSG-EG** (`$90`), **CSM / channel-3 special mode** (`$27` + `$A8-$AE`),
//!   and **operator detune** (`$30` DT field): all skipped. Detune-off means no inter-operator beating.
//! - **Exact envelope rate & key-scale tables**: SY-2 uses a calibrated approximation, so envelope timing
//!   and timbre are "close", not sample-exact. This is the same long-tail the design doc calls out.
//!
//! The synth is float-based (`f32`) — it is a **synthesis** helper, never part of `System`, `state_hash`,
//! or `export_state`, so it carries no currency obligations.

/// YM2612 FM clock (Hz) — the master FM clock the phase generator's real-Hz frequency is derived from.
const YM2612_CLOCK: f32 = 7_670_453.0;
/// The FM operator sample-clock divisor: the chip advances an operator once per 144 master cycles, so the
/// native operator rate is `YM2612_CLOCK / 144 ≈ 53_267 Hz`. Used only to scale `fnum`/`block` → real Hz.
const FM_CLOCK_DIV: f32 = 144.0;

/// Envelope attenuation is a 10-bit value: 0 = full volume, [`MAX_ATT`] = silence.
const MAX_ATT: f32 = 1023.0;
/// Full attenuation span in decibels (OPN2 ≈ 96 dB over the 10-bit range). Used to build [`exp table`].
const ATT_DB_RANGE: f32 = 96.0;

/// Per-sample envelope-rate scale (attenuation units per sample at effective rate 0's `2^0` base). Chosen
/// so a mid rate (~31) sweeps the full range in ~0.2 s at 44.1 kHz — musically reasonable decay timing.
/// Approximate: the exact OPN rate table is SY-3.
const RATE_BASE: f32 = 0.0006;
/// Attack is the same rate curve as decay, sped up by this factor (attack is much faster than decay on
/// real OPN2). Approximate.
const ATTACK_SPEED: f32 = 12.0;

/// Depth of inter-operator phase modulation: a full-amplitude (±1.0) modulator shifts the carrier phase by
/// this many cycles. `1.0` is a neutral, recognizable FM depth (modulator TL still scales it down).
const MOD_SCALE: f32 = 1.0;

/// Per-carrier output level (pre-mix, pre-clamp). Comparable to one SN76489 tone channel (~4000) so FM and
/// PSG sit at similar loudness before the sink sums + clamps them.
const FM_LEVEL: f32 = 3500.0;

/// Number of entries in the sine and exp lookup tables (10-bit phase / attenuation resolution).
const TABLE_LEN: usize = 1024;

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

/// One FM operator: a sine phase generator plus an envelope generator and its programmed parameters.
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
    /// Phase accumulator in cycles/turns `[0,1)`.
    phase: f32,
    /// Current envelope attenuation (0 = loud, [`MAX_ATT`] = silent).
    env: f32,
    /// Envelope phase.
    eg: EgState,
    /// This operator's last normalized output (±amplitude) — for operator-1 self-feedback.
    prev_out: f32,
    /// The output before that (feedback averages the last two).
    prev_out2: f32,
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
            phase: 0.0,
            env: MAX_ATT,
            eg: EgState::Off,
            prev_out: 0.0,
            prev_out2: 0.0,
        }
    }

    /// Key this operator on: (re)start the attack from phase 0 (OPN resets operator phase on key-on).
    fn key_on(&mut self) {
        self.eg = EgState::Attack;
        self.phase = 0.0;
        self.prev_out = 0.0;
        self.prev_out2 = 0.0;
    }

    /// Key this operator off: fall into the release phase (unless already fully silent).
    fn key_off(&mut self) {
        if self.eg != EgState::Off {
            self.eg = EgState::Release;
        }
    }

    /// Advance the envelope one sample using this operator's effective rates (`kc` = channel key-code for
    /// key-scaling). Approximate: linear-in-attenuation decays, faster linear attack.
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

    /// The current linear output amplitude (0..~1): envelope attenuation + TL, mapped through the exp
    /// table. Silent operators return 0.
    fn amplitude(&self, exp_table: &[f32; TABLE_LEN]) -> f32 {
        if self.eg == EgState::Off {
            return 0.0;
        }
        // TL step = 8 attenuation units; sum with the envelope and clamp to the table range.
        let att = (self.env + (self.tl as f32) * 8.0).clamp(0.0, MAX_ATT);
        exp_table[att as usize & (TABLE_LEN - 1)]
    }

    /// Produce this operator's normalized output for the current sample and advance its phase.
    ///
    /// `phase_inc` is the per-sample phase step (cycles) for the channel's pitch; `mod_turns` is the
    /// summed phase modulation (in cycles) from feedback / modulator operators. Output is `±amplitude`.
    fn next(
        &mut self,
        phase_inc: f32,
        mod_turns: f32,
        sine: &[f32; TABLE_LEN],
        exp_table: &[f32; TABLE_LEN],
        kc: u8,
    ) -> f32 {
        self.step_envelope(kc);
        let amp = self.amplitude(exp_table);
        let idx = (((self.phase + mod_turns).rem_euclid(1.0)) * TABLE_LEN as f32) as usize;
        let out = sine[idx & (TABLE_LEN - 1)] * amp;
        // Advance and wrap the phase.
        self.phase += phase_inc;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }
        // Roll the feedback history (only operator 1 uses it, but harmless to keep for all).
        self.prev_out2 = self.prev_out;
        self.prev_out = out;
        out
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

    /// Per-sample phase increment (cycles) for this channel's pitch, before per-operator `MUL`.
    fn base_phase_inc(&self, sample_rate: f32) -> f32 {
        // f = fnum · 2^(block-1) · (clock/144) / 2^20 ; phase step = f / sample_rate.
        let fm_rate = YM2612_CLOCK / FM_CLOCK_DIV;
        let block_scale = if self.block == 0 {
            0.5
        } else {
            (1u32 << (self.block - 1)) as f32
        };
        let f = self.fnum as f32 * block_scale * fm_rate / (1u32 << 20) as f32;
        f / sample_rate
    }

    /// Render one sample for this channel, returning `(left, right)` scaled output.
    fn next_sample(
        &mut self,
        sample_rate: f32,
        sine: &[f32; TABLE_LEN],
        exp_table: &[f32; TABLE_LEN],
    ) -> (f32, f32) {
        let base_inc = self.base_phase_inc(sample_rate);
        let kc = self.key_code();

        // Per-operator phase increment: MUL=0 → ×0.5, else ×MUL.
        let op_inc = |mul: u8| -> f32 {
            if mul == 0 {
                base_inc * 0.5
            } else {
                base_inc * mul as f32
            }
        };

        // Operator-1 self-feedback: average the last two Op1 outputs, scaled by the feedback level.
        let fb = if self.feedback == 0 {
            0.0
        } else {
            let avg = (self.ops[0].prev_out + self.ops[0].prev_out2) * 0.5;
            avg * ((1u32 << self.feedback) as f32 / 128.0)
        };

        let inc0 = op_inc(self.ops[0].mul);
        let out1 = self.ops[0].next(inc0, fb, sine, exp_table, kc);

        let inc1 = op_inc(self.ops[1].mul);
        let inc2 = op_inc(self.ops[2].mul);
        let inc3 = op_inc(self.ops[3].mul);

        // The 8 FM algorithms wire Op1..Op4 into modulator/carrier roles. `MOD_SCALE` converts a modulator's
        // normalized output into carrier phase shift; carriers are summed.
        let m = MOD_SCALE;
        let carriers: f32 = match self.algorithm {
            0 => {
                // Op1→Op2→Op3→Op4→out (serial).
                let o2 = self.ops[1].next(inc1, out1 * m, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, o2 * m, sine, exp_table, kc);
                self.ops[3].next(inc3, o3 * m, sine, exp_table, kc)
            }
            1 => {
                // (Op1+Op2)→Op3→Op4→out.
                let o2 = self.ops[1].next(inc1, 0.0, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, (out1 + o2) * m, sine, exp_table, kc);
                self.ops[3].next(inc3, o3 * m, sine, exp_table, kc)
            }
            2 => {
                // Op1→Op4, Op2→Op3→Op4→out.
                let o2 = self.ops[1].next(inc1, 0.0, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, o2 * m, sine, exp_table, kc);
                self.ops[3].next(inc3, (out1 + o3) * m, sine, exp_table, kc)
            }
            3 => {
                // Op1→Op2→Op4, Op3→Op4→out.
                let o2 = self.ops[1].next(inc1, out1 * m, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, 0.0, sine, exp_table, kc);
                self.ops[3].next(inc3, (o2 + o3) * m, sine, exp_table, kc)
            }
            4 => {
                // Op1→Op2→out, Op3→Op4→out (two parallel chains).
                let o2 = self.ops[1].next(inc1, out1 * m, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, 0.0, sine, exp_table, kc);
                let o4 = self.ops[3].next(inc3, o3 * m, sine, exp_table, kc);
                o2 + o4
            }
            5 => {
                // Op1 modulates Op2, Op3, Op4; all three → out.
                let o2 = self.ops[1].next(inc1, out1 * m, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, out1 * m, sine, exp_table, kc);
                let o4 = self.ops[3].next(inc3, out1 * m, sine, exp_table, kc);
                o2 + o3 + o4
            }
            6 => {
                // Op1→Op2→out; Op3→out; Op4→out.
                let o2 = self.ops[1].next(inc1, out1 * m, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, 0.0, sine, exp_table, kc);
                let o4 = self.ops[3].next(inc3, 0.0, sine, exp_table, kc);
                o2 + o3 + o4
            }
            _ => {
                // Algorithm 7: all four operators → out (fully additive).
                let o2 = self.ops[1].next(inc1, 0.0, sine, exp_table, kc);
                let o3 = self.ops[2].next(inc2, 0.0, sine, exp_table, kc);
                let o4 = self.ops[3].next(inc3, 0.0, sine, exp_table, kc);
                out1 + o2 + o3 + o4
            }
        };

        let sample = carriers * FM_LEVEL;
        let l = if self.pan_l { sample } else { 0.0 };
        let r = if self.pan_r { sample } else { 0.0 };
        (l, r)
    }
}

/// The full minimal YM2612 FM synthesizer: 6 channels, the register-write decode, and shared lookup tables.
pub struct Ym2612Synth {
    /// Output sample rate (Hz).
    sample_rate: f32,
    /// The 6 FM channels (channels 0-2 = part I / bank 0, channels 3-5 = part II / bank 1).
    channels: [Channel; 6],
    /// DAC (channel-6 PCM) enable, `$2B` bit7. When set, FM channel 6 is muted (PCM output is SY-3).
    dac_enabled: bool,
    /// Sine lookup: one cycle, `[-1, 1]`.
    sine: [f32; TABLE_LEN],
    /// Attenuation → linear-amplitude lookup (index = 10-bit attenuation).
    exp_table: [f32; TABLE_LEN],
}

/// Register-address → operator-index map. The low-nibble operator field of an `$30-$9F` write addresses
/// hardware slots in the order S1,S3,S2,S4; this maps that field (0-3) to the algorithm operator index
/// (Op1..Op4 → 0..3).
const SLOT_MAP: [usize; 4] = [0, 2, 1, 3];

impl Ym2612Synth {
    /// A fresh FM synth producing `sample_rate` Hz output (all channels silent at reset).
    pub fn new(sample_rate: u32) -> Self {
        let mut sine = [0.0f32; TABLE_LEN];
        for (i, s) in sine.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * i as f32 / TABLE_LEN as f32).sin();
        }
        let mut exp_table = [0.0f32; TABLE_LEN];
        for (i, e) in exp_table.iter_mut().enumerate() {
            // Attenuation (10-bit) → dB → linear amplitude. Index 0 = full volume, MAX = ~silent.
            let db = i as f32 * (ATT_DB_RANGE / TABLE_LEN as f32);
            *e = 10.0f32.powf(-db / 20.0);
        }
        Self {
            sample_rate: sample_rate as f32,
            channels: [Channel::new(); 6],
            dac_enabled: false,
            sine,
            exp_table,
        }
    }

    /// Feed one decoded FM register write: `bank` (0 = part I / channels 0-2, 1 = part II / channels 3-5),
    /// the latched register `reg`, and its `value` — the identical triple the VGM path records.
    pub fn write(&mut self, bank: u8, reg: u8, value: u8) {
        match reg {
            // --- bank-0-only global registers ---
            0x28 if bank == 0 => self.key_on_off(value),
            0x2B if bank == 0 => self.dac_enabled = value & 0x80 != 0,
            // Per-channel and per-operator registers exist in both banks.
            0x30..=0x9F => self.write_operator(bank, reg, value),
            0xA0..=0xA2 => self.write_fnum_low(bank, reg, value),
            0xA4..=0xA6 => self.write_fnum_high(bank, reg, value),
            0xB0..=0xB2 => self.write_alg_feedback(bank, reg, value),
            0xB4..=0xB6 => self.write_pan(bank, reg, value),
            // Timers ($24-$27), LFO ($22), DAC data ($2A), ch3-special ($A8-$AE), SSG handled above/ignored.
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
            0x30 => o.mul = value & 0x0F, // (detune bits 4-6 deferred to SY-3)
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
            0x90 => {} // SSG-EG deferred to SY-3.
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

    /// Produce one stereo output sample `(left, right)` as the sum of all six channels' contributions.
    /// Channel 6 (index 5) is muted while the DAC is enabled (its PCM output is SY-3).
    pub fn next_sample(&mut self) -> (i32, i32) {
        let mut l = 0.0f32;
        let mut r = 0.0f32;
        for (i, ch) in self.channels.iter_mut().enumerate() {
            if i == 5 && self.dac_enabled {
                // Still advance nothing extra — muted DAC channel contributes silence in SY-2.
                continue;
            }
            let (cl, cr) = ch.next_sample(self.sample_rate, &self.sine, &self.exp_table);
            l += cl;
            r += cr;
        }
        (l as i32, r as i32)
    }
}

/// The effective 6-bit envelope rate: `2·base + keyscale`, clamped to 63. `base` is a 5-bit rate register
/// (or the RR-derived value); `ks`/`kc` add the key-scale contribution. Approximate (exact table is SY-3).
fn effective_rate(base: u8, ks: u8, kc: u8) -> u8 {
    if base == 0 {
        return 0;
    }
    let ks_add = kc >> (3 - ks.min(3));
    (2 * base as u16 + ks_add as u16).min(63) as u8
}

/// Attenuation units added per sample at effective envelope `rate` (0-63). Rate 0 = frozen. Exponential in
/// the rate (each +4 rate ≈ ×2 speed) — a calibrated approximation of the OPN rate table.
fn rate_increment(rate: u8) -> f32 {
    if rate == 0 {
        0.0
    } else {
        RATE_BASE * 2.0f32.powf(rate as f32 / 4.0)
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

    /// Program channel 0, operator 1 (algorithm 7 additive so Op1 is a bare carrier) to a known pitch and
    /// key it on; render one second and count zero crossings — proving the phase generator emits the
    /// expected fundamental frequency.
    #[test]
    fn operator_produces_expected_pitch() {
        let sample_rate = 44_100u32;
        let mut fm = Ym2612Synth::new(sample_rate);

        // Algorithm 7 (all operators are carriers), no feedback: $B0 = 0x07.
        fm.write(0, 0xB0, 0x07);
        // Op1 params: MUL=1 (so pitch is exact, not ×0.5), TL=0 (loudest), AR=31 (instant attack),
        // D1R=0/D2R=0 (no decay), SL=0, RR=0 → a sustained full-volume sine.
        fm.write(0, 0x30, 0x01); // MUL=1 for operator-address-field 0 (Op1) of channel 0
        fm.write(0, 0x40, 0x00); // TL=0
        fm.write(0, 0x50, 0x1F); // KS=0, AR=31
        fm.write(0, 0x60, 0x00); // D1R=0
        fm.write(0, 0x70, 0x00); // D2R=0
        fm.write(0, 0x80, 0x00); // SL=0, RR=0
                                 // Pitch: fnum=1083, block=4 → f = 1083·8·(7670453/144)/2^20 ≈ 440.1 Hz.
                                 // $A4 = block(4)<<3 | fnum_hi(1083>>8 = 4) = 0x20 | 0x04 = 0x24 ; $A0 = fnum low = 1083 & 0xFF = 0x3B.
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        // Stereo both sides.
        fm.write(0, 0xB4, 0xC0);
        // Key on Op1 only: channel 0, mask bit4 → 0x10.
        fm.write(0, 0x28, 0x10);

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

        // A full cycle = 2 crossings → expect ≈ 2·440 ≈ 880/sec. Allow a modest band (attack ramp + the
        // fixed-point index edge). This is a pitch check, not a sample-exact check.
        assert!(
            (860..=900).contains(&crossings),
            "expected ~880 zero crossings for a 440 Hz operator, got {crossings}"
        );
    }

    /// A channel that is never keyed on must be pure silence.
    #[test]
    fn unkeyed_channel_is_silent() {
        let mut fm = Ym2612Synth::new(44_100);
        // Program a channel fully but never send a key-on.
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

    /// DAC-enable ($2B bit7) mutes FM channel 6 (index 5) so it emits no stale FM tone.
    #[test]
    fn dac_enable_mutes_channel_six() {
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

        // Enable DAC → channel 6 must be silent.
        fm.write(0, 0x2B, 0x80);
        let mut peak = 0i32;
        for _ in 0..4410 {
            let (l, r) = fm.next_sample();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert_eq!(peak, 0, "DAC-enabled channel 6 must be muted, peak {peak}");
    }
}
