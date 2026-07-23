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
//! `attenuation_to_volume` / `s_power_table`, the operator phase→volume pipeline, and — as of SY-3c — the
//! envelope generator: the 64-entry `s_increment_table` / `attenuation_increment`, the `clock_envelope`
//! rate-shift + non-linear-attack update, and `src/ymfm_opn.cpp`'s `cache_operator_data` effective-rate /
//! key-scale / sustain-level math; and — as of SY-3d — operator **detune**: the 32×4 `s_detune_adjustment`
//! table + `detune_adjustment` (`src/ymfm_fm.ipp`) and the OPN `compute_phase_step` detune application
//! (`src/ymfm_opn.cpp`); and — as of SY-3e — the **LFO**: `opn_registers_base::clock_noise_and_lfo`'s
//! `lfo_max_count` rate table + AM-triangle / PM-raw derivation (`src/ymfm_opn.cpp`), `lfo_am_offset`
//! (AMS scaling), and `opn_lfo_pm_phase_adjustment`'s 8×8 `s_lfo_pm_shifts` PM table + the OPN
//! `compute_phase_step` PM application (`src/ymfm_fm.ipp`).** Our generated tables are pinned to ymfm's
//! literal values by unit tests (see [`tests`]). Nuked-OPN2 (LGPL) is **not** consulted, ported, or used
//! as a fixture.
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
//! - **Envelope generator (SY-3c: OPN2-exact).** Attack / Decay / Sustain / Release, value living in the OPN
//!   10-bit attenuation domain (0 = loud, [`MAX_ATT`] = silent) so it sums with `TL` (`$40`) and the log-sin
//!   in one place. The EG is clocked on a global counter that advances **once every 3 native operator ticks**
//!   ([`EG_CLOCK_DIVIDER`], the OPN2 EG divisor); the per-state 6-bit **effective rate** is
//!   `2·rate_param + (keycode >> (3 − KS))` clamped to 63 ([`effective_rate`]), driving a `rate>>2` shift and
//!   ymfm's 64-entry [`S_INCREMENT_TABLE`]. **Attack is non-linear** (`env += (~env·inc) >> 4`); decay,
//!   second-decay and release add the increment linearly toward SL / silence. Key-on is **edge-triggered**
//!   (rising edge only — a held note re-asserted per tick does not restart). `rate_param == 0` freezes the
//!   phase (AR = 0 never opens); AR whose effective rate ≥ 62 opens instantly.
//! - **The 8 FM algorithms + operator-1 self-feedback** (`$B0`), and **stereo L/R pan** (`$B4`).
//! - **Operator detune (SY-3d).** The `$30` DT field (bits 4-6) adds a small, pitch-dependent signed
//!   phase-step displacement (a few cents) via ymfm's [`S_DETUNE_ADJUSTMENT`] table, indexed by DT&3 and the
//!   OPN 5-bit **detune keycode** ([`Channel::detune_key_code`]); DT bit 2 is the sign. Following ymfm's OPN
//!   `compute_phase_step`, the detune is summed into the base phase step (17-bit wrap) **before** the MUL
//!   multiplier, so DT=0 leaves the increment bit-identical to SY-3c.
//! - **LFO — tremolo (AM) + vibrato (PM) (SY-3e).** `$22` bit3 enables a global low-frequency oscillator
//!   whose rate (bits 0-2) selects one of 8 [`LFO_MAX_COUNT`] native-tick dividers (slowest ≈ 3.98 Hz). The
//!   LFO ([`Ym2612Synth::clock_lfo`], once per native tick) produces a **0-126 AM triangle** and a stepped
//!   signed **PM** value exactly as ymfm's `clock_noise_and_lfo`. **AM** (per-operator `$60` bit7 enable ×
//!   per-channel `$B4` AMS bits 4-5) adds [`lfo_am_offset`] to the operator attenuation (AMS=0 ⇒ none).
//!   **PM** (per-channel `$B4` FMS bits 0-2) perturbs every operator's phase increment through
//!   [`opn_lfo_pm_phase_adjustment`] / [`S_LFO_PM_SHIFTS`] (FMS=0 ⇒ none). With the LFO **disabled** AM = 0
//!   and PM = 0, so the datapath is bit-identical to SY-3d (see the note on `clock_lfo`).
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
//! - **SSG-EG** (`$90`) / **CSM / ch3-special** (SY-3f). Sub-frame `$2A` timing is SY-4.
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
const MAX_ATT: i32 = 0x3ff;

/// OPN2 envelope-generator clock divisor (ymfm `EG_CLOCK_DIVIDER` for `opn_registers_base`): the global EG
/// counter advances once every this many native operator ticks. So the EG runs at `NATIVE_RATE / 3 ≈ 17_756`
/// Hz, a third of the phase generator — the wall-clock speed of every attack/decay/release.
const EG_CLOCK_DIVIDER: u32 = 3;

/// ymfm `s_increment_table` (`src/ymfm_fm.ipp`, `attenuation_increment`): for each 6-bit effective rate (0-63)
/// a `u32` packing eight 4-bit attenuation increments; [`attenuation_increment`] selects nibble `index`
/// (0-7). Pinned to ymfm's literal values by [`tests::increment_table_matches_ymfm`]. Rates < 2 never move
/// (0x0), rates ≥ 60 add 8/step (0x8888_8888); the interior rows interleave 1/2/4/8 to spread the rate.
#[rustfmt::skip]
const S_INCREMENT_TABLE: [u32; 64] = [
    0x00000000, 0x00000000, 0x10101010, 0x10101010,
    0x10101010, 0x10101010, 0x11101110, 0x11101110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x10101010, 0x10111010, 0x11101110, 0x11111110,
    0x11111111, 0x21112111, 0x21212121, 0x22212221,
    0x22222222, 0x42224222, 0x42424242, 0x44424442,
    0x44444444, 0x84448444, 0x84848484, 0x88848884,
    0x88888888, 0x88888888, 0x88888888, 0x88888888,
];

/// ymfm `attenuation_increment(rate, index)`: nibble `index` (0-7) of [`S_INCREMENT_TABLE`]`[rate]` — the
/// attenuation step applied on an EG clock at effective `rate`, selected by 3 bits of the EG counter.
fn attenuation_increment(rate: u32, index: u32) -> u32 {
    (S_INCREMENT_TABLE[rate as usize] >> (4 * index)) & 0xf
}

/// ymfm `s_detune_adjustment` (`src/ymfm_fm.ipp`, `detune_adjustment`): for each OPN 5-bit key-code (0-31,
/// row) and each `DT & 3` magnitude (0-3, column), the **unsigned** 6-bit phase-step displacement. DT bit 2 is
/// the sign (`DT` 4-7 negate the magnitude). Pinned to ymfm's literal values by
/// [`tests::detune_table_matches_ymfm`]. (ymfm notes the table itself was verified against Nuked's equations,
/// but the table — not Nuked — is what we port.)
#[rustfmt::skip]
const S_DETUNE_ADJUSTMENT: [[u8; 4]; 32] = [
    [0,  0,  1,  2], [0,  0,  1,  2], [0,  0,  1,  2], [0,  0,  1,  2],
    [0,  1,  2,  2], [0,  1,  2,  3], [0,  1,  2,  3], [0,  1,  2,  3],
    [0,  1,  2,  4], [0,  1,  3,  4], [0,  1,  3,  4], [0,  1,  3,  5],
    [0,  2,  4,  5], [0,  2,  4,  6], [0,  2,  4,  6], [0,  2,  5,  7],
    [0,  2,  5,  8], [0,  3,  6,  8], [0,  3,  6,  9], [0,  3,  7, 10],
    [0,  4,  8, 11], [0,  4,  8, 12], [0,  4,  9, 13], [0,  5, 10, 14],
    [0,  5, 11, 16], [0,  6, 12, 17], [0,  6, 13, 19], [0,  7, 14, 20],
    [0,  8, 16, 22], [0,  8, 16, 22], [0,  8, 16, 22], [0,  8, 16, 22],
];

/// ymfm `detune_adjustment(detune, keycode)`: the signed phase-step displacement for a 3-bit `DT` and the OPN
/// 5-bit detune key-code. The magnitude is [`S_DETUNE_ADJUSTMENT`]`[keycode][DT & 3]`; `DT` bit 2 negates it.
fn detune_adjustment(dt: u8, keycode: u8) -> i32 {
    let mag = S_DETUNE_ADJUSTMENT[(keycode & 0x1f) as usize][(dt & 0x03) as usize] as i32;
    if dt & 0x04 != 0 {
        -mag
    } else {
        mag
    }
}

/// ymfm `lfo_max_count` (`opn_registers_base::clock_noise_and_lfo`, `src/ymfm_opn.cpp`): for each 3-bit LFO
/// rate (`$22` bits 0-2) the native-tick divider that advances the 7-bit LFO position living in bits 8-14 of
/// the LFO counter. Pinned to ymfm's literal values by [`tests::lfo_tables_match_ymfm`]. Rate 0 (÷109 → a
/// ~13_824-tick period ≈ 3.85 Hz) is the slowest; rate 7 (÷6) the fastest. (ymfm notes the counter carries a
/// deliberate hardware-matching off-by-one, reproduced here.)
const LFO_MAX_COUNT: [u8; 8] = [109, 78, 72, 68, 63, 45, 9, 6];

/// ymfm `s_lfo_pm_shifts` (`opn_lfo_pm_phase_adjustment`, `src/ymfm_fm.ipp`): indexed by `[FMS (0-7)][abs_pm
/// (0-7)]`, each byte packs two 4-bit right-shift amounts (low nibble + high nibble) applied to the top 7 bits
/// of FNUM to build the PM phase displacement. Pinned to ymfm's literal values by
/// [`tests::lfo_tables_match_ymfm`]. `0x77` ⇒ shift 7+7 (both fall off the 7-bit FNUM) ⇒ zero displacement.
#[rustfmt::skip]
const S_LFO_PM_SHIFTS: [[u8; 8]; 8] = [
    [0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77],
    [0x77, 0x77, 0x77, 0x77, 0x72, 0x72, 0x72, 0x72],
    [0x77, 0x77, 0x77, 0x72, 0x72, 0x72, 0x17, 0x17],
    [0x77, 0x77, 0x72, 0x72, 0x17, 0x17, 0x12, 0x12],
    [0x77, 0x77, 0x72, 0x17, 0x17, 0x17, 0x12, 0x07],
    [0x77, 0x77, 0x17, 0x12, 0x07, 0x07, 0x02, 0x01],
    [0x77, 0x77, 0x17, 0x12, 0x07, 0x07, 0x02, 0x01],
    [0x77, 0x77, 0x17, 0x12, 0x07, 0x07, 0x02, 0x01],
];

/// ymfm `lfo_am_offset` (`opn_registers_base`, `src/ymfm_opn.cpp`): scale the 6-bit LFO AM triangle
/// (`lfo_am`, 0-63) by the channel's 2-bit AM-sensitivity (`ams`, 0-3) into an attenuation offset (10-bit
/// domain). `am_shift = (1 << (ams ^ 3)) − 1` gives shifts 7/3/1/0 for AMS 0/1/2/3 → AMS=0 always yields 0
/// (no tremolo), AMS=3 the deepest (`lfo_am << 1`, ≈ 11.8 dB peak-to-peak once summed into the attenuation).
fn lfo_am_offset(lfo_am: u32, ams: u8) -> u32 {
    let am_shift = (1u32 << ((ams & 0x03) ^ 3) as u32) - 1;
    (lfo_am << 1) >> am_shift
}

/// ymfm `opn_lfo_pm_phase_adjustment` (`src/ymfm_fm.ipp`): the signed PM displacement summed into the 12-bit
/// FNUM (before the block shift) for a given `fnum_bits` (top 7 bits of FNUM), channel PM-sensitivity `fms`
/// (1-7; the caller skips `fms == 0`) and the LFO's raw PM value `pm` (−7..7). Two right-shifts from
/// [`S_LFO_PM_SHIFTS`] are summed, boosted for `fms > 5`, then `>> 2`, then re-signed. `pm == 0` (and every
/// FMS's `abs_pm == 0` entry `0x77`) yields 0, so a disabled/rest LFO leaves the phase step unperturbed.
fn opn_lfo_pm_phase_adjustment(fnum_bits: u32, fms: u32, pm: i32) -> i32 {
    let abs_pm = pm.unsigned_abs();
    let shifts = S_LFO_PM_SHIFTS[(fms & 0x07) as usize][(abs_pm & 0x07) as usize] as u32;
    let mut adjust =
        ((fnum_bits >> (shifts & 0x0f)) + (fnum_bits >> ((shifts >> 4) & 0x0f))) as i32;
    if fms > 5 {
        adjust <<= fms - 5;
    }
    adjust >>= 2;
    if pm < 0 {
        -adjust
    } else {
        adjust
    }
}

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
    /// `DT` (`$30` bits 4-6): detune. Bits 0-1 are the magnitude (indexing [`S_DETUNE_ADJUSTMENT`]); bit 2 is
    /// the sign (DT 4-7 = negative). Applied to the phase step in [`Channel::op_inc`].
    dt: u8,
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
    /// `AM` enable (`$60` bit7): when set, the LFO tremolo ([`lfo_am_offset`]) is added to this operator's
    /// attenuation. When clear the operator ignores the LFO AM entirely (SY-3e).
    am_enable: bool,

    // --- runtime state ---
    /// 20-bit phase accumulator (one full sine cycle = `1 << 20`; the top 10 bits index the sine).
    phase: u32,
    /// Current envelope attenuation in the OPN 10-bit domain (0 = loud, [`MAX_ATT`] = silent). Held as `i32`
    /// so the non-linear attack (`env += (~env·inc) >> 4`, arithmetic-shift signed) matches ymfm exactly.
    env: i32,
    /// Envelope phase.
    eg: EgState,
    /// The operator's last `$28` key bit, for **edge-triggered** key-on (audit F2): attack restarts only on a
    /// rising edge, so a held note re-asserted every tick does not retrigger.
    key_state: bool,
    /// This operator's last signed output (`compute_volume`) — for operator-1 self-feedback.
    prev_out: i32,
    /// The output before that (feedback sums the last two).
    prev_out2: i32,
}

impl Operator {
    fn new() -> Self {
        Self {
            mul: 0,
            dt: 0,
            tl: 0,
            ks: 0,
            ar: 0,
            d1r: 0,
            d2r: 0,
            sl: 0,
            rr: 0,
            am_enable: false,
            phase: 0,
            env: MAX_ATT,
            eg: EgState::Off,
            key_state: false,
            prev_out: 0,
            prev_out2: 0,
        }
    }

    /// Start the attack (ymfm `start_attack` on a key-on rising edge): (re)enter Attack from phase 0. The
    /// attack begins from the operator's **current** attenuation (OPN does not reset it), except that an
    /// effective attack rate ≥ 62 opens instantly (`env = 0`). No-op if already attacking.
    fn start_attack(&mut self, effective_ar: u8) {
        if self.eg == EgState::Attack {
            return;
        }
        self.eg = EgState::Attack;
        self.phase = 0;
        self.prev_out = 0;
        self.prev_out2 = 0;
        if effective_ar >= 62 {
            self.env = 0;
        }
    }

    /// Key this operator off: fall into the release phase (unless already fully silent).
    fn key_off(&mut self) {
        if self.eg != EgState::Off {
            self.eg = EgState::Release;
        }
    }

    /// Advance the envelope one **EG clock** (ymfm `clock_envelope`): `eg_counter` is the global EG counter
    /// (advances once per [`EG_CLOCK_DIVIDER`] native ticks), `kc` the channel key-code for key-scaling. State
    /// transitions run first, then the effective rate drives a `rate>>2` shift into [`S_INCREMENT_TABLE`]; the
    /// low 11 bits of the shifted counter gate whether this clock actually moves the envelope. Attack is the
    /// non-linear `env += (~env·inc) >> 4`; every other state adds the increment toward silence.
    fn clock_envelope(&mut self, eg_counter: u32, kc: u8) {
        // Attack → Decay when the envelope has risen to full volume.
        if self.eg == EgState::Attack && self.env <= 0 {
            self.eg = EgState::Decay;
        }
        // Decay → Sustain when the attenuation has fallen to the sustain level (checked immediately after the
        // attack transition, so an SL=0 patch drops straight to Sustain — matches ymfm's ordering).
        if self.eg == EgState::Decay && self.env >= sustain_att(self.sl) {
            self.eg = EgState::Sustain;
        }
        if self.eg == EgState::Off {
            return;
        }

        // The 6-bit effective rate for the current phase (`rate_param == 0` ⇒ frozen).
        let rate: u32 = match self.eg {
            EgState::Attack => effective_rate(self.ar, self.ks, kc),
            EgState::Decay => effective_rate(self.d1r, self.ks, kc),
            EgState::Sustain => effective_rate(self.d2r, self.ks, kc),
            // RR is 4-bit; the OPN maps it to the 6-bit rate as `(rr << 1) | 1`.
            EgState::Release => effective_rate((self.rr << 1) | 1, self.ks, kc),
            EgState::Off => return,
        } as u32;

        let rate_shift = rate >> 2;
        let counter = eg_counter << rate_shift;
        // Not on this rate's boundary yet → no envelope change this clock.
        if counter & 0x7ff != 0 {
            return;
        }
        let pos = if rate_shift <= 11 { 11 } else { rate_shift };
        let increment = attenuation_increment(rate, (counter >> pos) & 0x7) as i32;

        if self.eg == EgState::Attack {
            // ymfm attack: non-linear approach to 0. Rates 62/63 open instantly (handled at key-on) and are
            // a documented no-op here.
            if rate < 62 {
                self.env += (!self.env).wrapping_mul(increment) >> 4;
                if self.env < 0 {
                    self.env = 0;
                }
            }
        } else {
            // Decay / second-decay / release: linear rise toward silence.
            self.env += increment;
            if self.env >= 0x400 {
                self.env = MAX_ATT;
                // Second-decay and release that reach full attenuation are silent and frozen.
                if self.eg != EgState::Decay {
                    self.eg = EgState::Off;
                }
            }
        }
    }

    /// The 10-bit envelope attenuation (envelope + `TL` + LFO AM) in the OPN attenuation domain. `TL` (7-bit)
    /// is shifted into the 10-bit domain (`<< 3`, each step ≈ 8 units); `am_offset` is the LFO tremolo
    /// contribution (0 unless the LFO is enabled and this operator has AM enabled — see [`lfo_am_offset`]).
    /// The sum may exceed [`MAX_ATT`] and [`attenuation_to_volume`]'s shift saturates it to silence.
    fn env_att(&self, am_offset: u32) -> u32 {
        let e = self.env.clamp(0, MAX_ATT) as u32;
        e + ((self.tl as u32) << 3) + am_offset
    }

    /// The operator's signed output for the current phase, given phase modulation `opmod` (in the 10-bit
    /// phase's units — a modulator's output already right-shifted by the caller) and this operator's LFO AM
    /// attenuation offset `am_offset` (0 when no tremolo applies). This is ymfm's `compute_volume`:
    /// log-sin(phase) + `(env + AM)·4` → exp table → sign from the phase quadrant (bit 9).
    fn compute(
        &self,
        opmod: i32,
        am_offset: u32,
        log_sin: &[u16; TABLE_LEN],
        pow: &[u16; TABLE_LEN],
    ) -> i32 {
        if self.eg == EgState::Off {
            return 0;
        }
        let phase10 = ((self.phase >> 10) as i32).wrapping_add(opmod) as u32;
        let sin_att = abs_sin_attenuation(phase10, log_sin);
        // Envelope/TL/AM live in the 10-bit attenuation domain; `<< 2` converts to the exp table's 4×-finer
        // units (256 units = one octave) before summing with the already-exp-domain log-sin attenuation.
        let vol = attenuation_to_volume(sin_att + (self.env_att(am_offset) << 2), pow) as i32;
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
    /// AM sensitivity (`$B4` bits 4-5, 0-3): how deeply the LFO tremolo scales into this channel's operator
    /// attenuation (SY-3e; AMS=0 ⇒ no tremolo). See [`lfo_am_offset`].
    ams: u8,
    /// FM/PM sensitivity (`$B4` bits 0-2, 0-7): how deeply the LFO vibrato perturbs this channel's phase
    /// increment (SY-3e; FMS=0 ⇒ no vibrato). See [`opn_lfo_pm_phase_adjustment`].
    fms: u8,
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
            ams: 0,
            fms: 0,
        }
    }

    /// The channel key-code (5-bit) for envelope key-scaling: `block` in bits 4-2, fnum top bits in 1-0.
    fn key_code(&self) -> u8 {
        (self.block << 2) | ((self.fnum >> 9) & 0x03) as u8
    }

    /// The OPN **detune** 5-bit key-code (ymfm `cache_operator_data`, `src/ymfm_opn.cpp`): the top 4 bits of
    /// `block_freq` (= block + F10) shifted up one, OR'd with a magic low bit derived from FNUM bits 7-10 via a
    /// 16-entry constant (`bitfield(0xfe80, …)`). **This differs from [`Self::key_code`] (the EG key-scale
    /// keycode) in bit 0 only:** the EG uses fnum bit 9; ymfm's detune/keycode uses the magic FNUM-mix bit. We
    /// match ymfm exactly for the detune lookup rather than reuse the EG keycode.
    fn detune_key_code(&self) -> u8 {
        let block_freq = ((self.block as u32) << 11) | (self.fnum as u32 & 0x7ff);
        let hi = ((block_freq >> 10) & 0x0f) << 1;
        let magic = (0xfe80u32 >> ((block_freq >> 7) & 0x0f)) & 0x01;
        (hi | magic) as u8
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

    /// The per-operator phase increment, following ymfm's OPN `compute_phase_step` (`src/ymfm_opn.cpp`):
    ///
    /// ```text
    /// ps  = base >> 1                 // ymfm (fnum·2 << block) >> 2, == base>>1 in our units
    /// ps  = (ps + detune) & 0x1_ffff  // detune summed BEFORE MUL, 17-bit wrap (ymfm)
    /// inc = (ps · (2·MUL)) >> 1        // MUL as an x.1 value (MUL=0 → ×0.5)
    /// ```
    ///
    /// With `detune = 0` and `base` even (any block ≥ 1) this is bit-identical to SY-3c's `(base·2·MUL) >> 2`;
    /// they can differ only for a subsonic block-0 note with an odd FNUM (where `base>>1` truncates one phase
    /// unit earlier), which is the ymfm-faithful behaviour.
    fn op_inc(base: u32, mul: u8, detune: i32) -> u32 {
        let ps_pre = base >> 1;
        let ps = ps_pre.wrapping_add(detune as u32) & 0x1_ffff;
        let mul_x2 = if mul == 0 { 1 } else { 2 * mul as u32 };
        (ps * mul_x2) >> 1
    }

    /// Apply the LFO vibrato (PM) to the channel's phase base, following ymfm's OPN `compute_phase_step`:
    /// the PM displacement is summed into the **12-bit FNUM** (`fnum·2`) *before* the block shift, then the
    /// block-shifted value re-enters the SY-3d [`Self::op_inc`] pipeline. Returns a `base'` such that
    /// `op_inc(base', …)`'s internal `>> 1` reproduces ymfm's `(fnum12 << block) >> 2`.
    ///
    /// **No-op guarantees (byte-identity with SY-3d):** `fms == 0` returns `base` untouched; and even with
    /// `fms != 0`, `pm == 0` (LFO disabled or at a rest point) leaves the FNUM unchanged
    /// ([`opn_lfo_pm_phase_adjustment`] returns 0), so `base'` still equals `base`.
    fn pm_base(&self, base: u32, pm: i32) -> u32 {
        if self.fms == 0 {
            return base;
        }
        let fnum_bits = ((self.fnum as u32) >> 4) & 0x7f; // top 7 bits of the 11-bit FNUM
        let adj = opn_lfo_pm_phase_adjustment(fnum_bits, self.fms as u32, pm);
        let fnum12 = ((((self.fnum as u32) & 0x7ff) << 1) as i32).wrapping_add(adj) as u32 & 0xfff;
        (fnum12 << self.block) >> 1
    }

    /// Advance this channel by one native tick and return its `(left, right)` FM contribution (already
    /// FM-scaled and panned). On an EG clock (`eg_clock = Some(counter)`, once per [`EG_CLOCK_DIVIDER`] native
    /// ticks) the envelopes advance first; then operator outputs are computed at the current phases (OPN2
    /// log-sin/exp pipeline, ymfm modulation units), then all phases advance every native tick.
    fn tick(
        &mut self,
        eg_clock: Option<u32>,
        lfo_am: u32,
        lfo_pm: i32,
        log_sin: &[u16; TABLE_LEN],
        pow: &[u16; TABLE_LEN],
    ) -> (i32, i32) {
        let kc = self.key_code();
        if let Some(counter) = eg_clock {
            for op in self.ops.iter_mut() {
                op.clock_envelope(counter, kc);
            }
        }
        let base = self.base_phase_step();

        // LFO tremolo (SY-3e): the AM attenuation offset is per-channel (`ams`) scaled but only added to
        // operators that have AM enabled (`$60` bit7). `lfo_am == 0` (LFO disabled/at rest) ⇒ all zero.
        let am_offsets: [u32; 4] = std::array::from_fn(|i| {
            if self.ops[i].am_enable {
                lfo_am_offset(lfo_am, self.ams)
            } else {
                0
            }
        });

        // Operator-1 self-feedback: sum the last two Op1 outputs, right-shifted by `10 - feedback` (ymfm).
        let fb = if self.feedback == 0 {
            0
        } else {
            (self.ops[0].prev_out + self.ops[0].prev_out2) >> (10 - self.feedback)
        };
        let out1 = self.ops[0].compute(fb, am_offsets[0], log_sin, pow);
        self.ops[0].prev_out2 = self.ops[0].prev_out;
        self.ops[0].prev_out = out1;

        // The 8 FM algorithms wire Op1..Op4 into modulator/carrier roles. A modulator's signed output is
        // right-shifted by 1 (ymfm) before it is summed into the modulated operator's phase; carriers sum.
        let carriers: i32 = match self.algorithm {
            0 => {
                // Op1→Op2→Op3→Op4→out (serial).
                let o2 = self.ops[1].compute(out1 >> 1, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(o2 >> 1, am_offsets[2], log_sin, pow);
                self.ops[3].compute(o3 >> 1, am_offsets[3], log_sin, pow)
            }
            1 => {
                // (Op1+Op2)→Op3→Op4→out.
                let o2 = self.ops[1].compute(0, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute((out1 + o2) >> 1, am_offsets[2], log_sin, pow);
                self.ops[3].compute(o3 >> 1, am_offsets[3], log_sin, pow)
            }
            2 => {
                // Op1→Op4, Op2→Op3→Op4→out.
                let o2 = self.ops[1].compute(0, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(o2 >> 1, am_offsets[2], log_sin, pow);
                self.ops[3].compute((out1 + o3) >> 1, am_offsets[3], log_sin, pow)
            }
            3 => {
                // Op1→Op2→Op4, Op3→Op4→out.
                let o2 = self.ops[1].compute(out1 >> 1, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(0, am_offsets[2], log_sin, pow);
                self.ops[3].compute((o2 + o3) >> 1, am_offsets[3], log_sin, pow)
            }
            4 => {
                // Op1→Op2→out, Op3→Op4→out (two parallel chains).
                let o2 = self.ops[1].compute(out1 >> 1, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(0, am_offsets[2], log_sin, pow);
                let o4 = self.ops[3].compute(o3 >> 1, am_offsets[3], log_sin, pow);
                o2 + o4
            }
            5 => {
                // Op1 modulates Op2, Op3, Op4; all three → out.
                let o2 = self.ops[1].compute(out1 >> 1, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(out1 >> 1, am_offsets[2], log_sin, pow);
                let o4 = self.ops[3].compute(out1 >> 1, am_offsets[3], log_sin, pow);
                o2 + o3 + o4
            }
            6 => {
                // Op1→Op2→out; Op3→out; Op4→out.
                let o2 = self.ops[1].compute(out1 >> 1, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(0, am_offsets[2], log_sin, pow);
                let o4 = self.ops[3].compute(0, am_offsets[3], log_sin, pow);
                o2 + o3 + o4
            }
            _ => {
                // Algorithm 7: all four operators → out (fully additive).
                let o2 = self.ops[1].compute(0, am_offsets[1], log_sin, pow);
                let o3 = self.ops[2].compute(0, am_offsets[2], log_sin, pow);
                let o4 = self.ops[3].compute(0, am_offsets[3], log_sin, pow);
                out1 + o2 + o3 + o4
            }
        };

        // Advance every operator's phase for the next tick. Detune (SY-3d) uses the OPN detune keycode and is
        // summed into the phase step before MUL; LFO vibrato (SY-3e) perturbs the FNUM before the block shift
        // via [`Self::pm_base`] (a no-op while FMS=0 or the LFO is at rest). See [`Self::op_inc`].
        let dkc = self.detune_key_code();
        let pm_base = self.pm_base(base, lfo_pm);
        for op in self.ops.iter_mut() {
            let det = detune_adjustment(op.dt, dkc);
            let inc = Self::op_inc(pm_base, op.mul, det);
            op.advance(inc);
        }

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
    /// Global envelope-generator counter (ymfm `m_env_counter >> 2`): advances by 1 once every
    /// [`EG_CLOCK_DIVIDER`] native ticks and drives every operator's [`Operator::clock_envelope`].
    eg_counter: u32,
    /// Native-tick subcounter feeding the `/3` EG divisor (0-2); the EG clocks when it wraps.
    eg_subcount: u32,
    /// LFO enable (`$22` bit3). When clear, [`Self::clock_lfo`] holds [`Self::lfo_am`] / [`Self::lfo_pm`] at 0
    /// so the datapath is bit-identical to SY-3d (SY-3e).
    lfo_enabled: bool,
    /// LFO rate (`$22` bits 0-2, 0-7): indexes [`LFO_MAX_COUNT`] for the counter divider.
    lfo_rate: u8,
    /// ymfm `m_lfo_counter`: the free-running LFO counter (7-bit AM/PM position in bits 8-14), advanced once
    /// per native tick by [`Self::clock_lfo`].
    lfo_counter: u32,
    /// Current LFO AM triangle value (0-63), refreshed each native tick; scaled per channel by
    /// [`lfo_am_offset`]. 0 while the LFO is disabled.
    lfo_am: u32,
    /// Current LFO raw PM value (−7..7), refreshed each native tick; scaled per channel/pitch by
    /// [`opn_lfo_pm_phase_adjustment`]. 0 while the LFO is disabled.
    lfo_pm: i32,
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
            eg_counter: 0,
            eg_subcount: 0,
            lfo_enabled: false,
            lfo_rate: 0,
            lfo_counter: 0,
            lfo_am: 0,
            lfo_pm: 0,
        }
    }

    /// Feed one decoded FM register write: `bank` (0 = part I / channels 0-2, 1 = part II / channels 3-5),
    /// the latched register `reg`, and its `value` — the identical triple the VGM path records.
    pub fn write(&mut self, bank: u8, reg: u8, value: u8) {
        match reg {
            // --- bank-0-only global registers ---
            // $22 LFO: bit3 = enable, bits 0-2 = rate (SY-3e). Global (both banks share one LFO).
            0x22 if bank == 0 => {
                self.lfo_enabled = value & 0x08 != 0;
                self.lfo_rate = value & 0x07;
            }
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
            // Timers ($24-$27), ch3-special ($A8-$AE), SSG handled above/ignored.
            _ => {}
        }
    }

    /// `$28` key on/off: low 3 bits select the channel (0-2 = ch 1-3, 4-6 = ch 4-6); bits 4-7 are the
    /// per-operator key mask (bit4 = Op1 … bit7 = Op4). Key-on is **edge-triggered** (audit F2): attack
    /// restarts only when an operator's key bit rises 0→1, so a driver that re-asserts `$28` for a held note
    /// does not retrigger.
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
        let kc = channel.key_code();
        for op in 0..4 {
            let want = value & (0x10 << op) != 0;
            let operator = &mut channel.ops[op];
            if want == operator.key_state {
                continue; // no edge → hold current envelope (a re-asserted held note must not restart)
            }
            operator.key_state = want;
            if want {
                let effective_ar = effective_rate(operator.ar, operator.ks, kc);
                operator.start_attack(effective_ar);
            } else {
                operator.key_off();
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
            0x30 => {
                o.mul = value & 0x0F;
                o.dt = (value >> 4) & 0x07; // detune (SY-3d): magnitude in bits 0-1, sign in bit 2
            }
            0x40 => o.tl = value & 0x7F,
            0x50 => {
                o.ks = (value >> 6) & 0x03;
                o.ar = value & 0x1F;
            }
            0x60 => {
                o.d1r = value & 0x1F;
                o.am_enable = value & 0x80 != 0; // AM enable (SY-3e): gates the LFO tremolo per operator
            }
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

    /// `$B4-$B6` stereo pan (bit7 = left, bit6 = right) + LFO sensitivity (SY-3e): AMS (bits 4-5) sets the
    /// tremolo depth, FMS (bits 0-2) the vibrato depth.
    fn write_pan(&mut self, bank: u8, reg: u8, value: u8) {
        let Some(ch) = Self::channel_index(bank, reg) else {
            return;
        };
        let c = &mut self.channels[ch];
        c.pan_l = value & 0x80 != 0;
        c.pan_r = value & 0x40 != 0;
        c.ams = (value >> 4) & 0x03;
        c.fms = value & 0x07;
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

    /// Advance the global LFO one native tick (ymfm `clock_noise_and_lfo` for the OPNA/OPN2 register set).
    ///
    /// While disabled the counter is pinned to 0 and both AM and PM are **0** — the datapath then matches
    /// SY-3d bit-for-bit. (ymfm instead pins its disabled AM to `0x3f`, so an operator with AM enabled sees a
    /// constant added attenuation even with the LFO off — the "MegaDrive Venom" quirk. We deliberately do
    /// **not** reproduce that here: the SY-3e byte-identity contract requires a disabled LFO to leave SY-3d
    /// untouched, and the target ROM enables the LFO anyway, so the quirk is unobservable for it.)
    ///
    /// When enabled: `subcount` (the counter's low byte) drives the [`LFO_MAX_COUNT`] divider; on a crossing
    /// the counter jumps to advance the 7-bit position in bits 8-14 (with ymfm's intentional off-by-one). The
    /// AM triangle is the low 6 bits of that position, inverted on the first half of the period; the raw PM is
    /// the low 3 bits of a 5-bit field, reflected and negated by the top two bits.
    fn clock_lfo(&mut self) {
        if !self.lfo_enabled {
            self.lfo_counter = 0;
            self.lfo_am = 0;
            self.lfo_pm = 0;
            return;
        }
        let subcount = self.lfo_counter & 0xff;
        self.lfo_counter = self.lfo_counter.wrapping_add(1);
        if subcount >= LFO_MAX_COUNT[self.lfo_rate as usize] as u32 {
            // Push bit 8 (advance the 7-bit position) and zero the low byte; ymfm uses 0x101 rather than 0x100
            // to match a hardware off-by-one that makes the fastest rate slightly faster than published.
            self.lfo_counter = self.lfo_counter.wrapping_add(0x101 - subcount);
        }
        // AM: low 6 bits of the position (bits 8-13); inverted while bit 14 is 0 → a 0..63 triangle.
        let mut am = (self.lfo_counter >> 8) & 0x3f;
        if (self.lfo_counter >> (8 + 6)) & 1 == 0 {
            am ^= 0x3f;
        }
        self.lfo_am = am;
        // PM: low 3 bits (bits 10-12); reflected on bit 13, negated on bit 14 → a stepped −7..7 value.
        let mut pm = ((self.lfo_counter >> 10) & 0x7) as i32;
        if (self.lfo_counter >> (10 + 3)) & 1 != 0 {
            pm ^= 7;
        }
        self.lfo_pm = if (self.lfo_counter >> (10 + 4)) & 1 != 0 {
            -pm
        } else {
            pm
        };
    }

    /// Advance the whole FM chip by one **native** operator tick and return the summed `(left, right)` FM
    /// output. Channel 6's FM is skipped while the DAC drives it (`$2B` bit7); the PCM stream is added at the
    /// output rate in [`Self::next_sample`].
    fn tick_native(&mut self) -> (i32, i32) {
        // The EG advances once every EG_CLOCK_DIVIDER native ticks; on those ticks pass the incremented global
        // counter down so the operators clock their envelopes (otherwise `None` = phase-only tick).
        self.eg_subcount += 1;
        let eg_clock = if self.eg_subcount >= EG_CLOCK_DIVIDER {
            self.eg_subcount = 0;
            self.eg_counter = self.eg_counter.wrapping_add(1);
            Some(self.eg_counter)
        } else {
            None
        };
        // The LFO advances every native tick (ymfm clocks it once per engine sample, ungated by the EG
        // divisor) and feeds the same AM/PM values to all channels this tick.
        self.clock_lfo();
        let lfo_am = self.lfo_am;
        let lfo_pm = self.lfo_pm;
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
            // Always advance the channel (envelope + phase) — including ch6 while the DAC drives it (audit
            // F3): its EG must keep evolving muted so there is no stale-volume pop when the DAC turns off. The
            // FM output is simply discarded for ch6 during DAC playback (the PCM stream plays in its place).
            let (cl, cr) = ch.tick(eg_clock, lfo_am, lfo_pm, log_sin, pow);
            if i == 5 && *dac_enabled {
                continue;
            }
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

/// The effective 6-bit envelope rate (ymfm `effective_rate`, `cache_operator_data`): `2·base + keyscale`,
/// clamped to 63, where `base` is a 5-bit rate register (or the RR-derived `(rr<<1)|1`) and
/// `keyscale = kc >> (3 − KS)` (ymfm `keycode >> (op_ksr ^ 3)`). `base == 0` freezes the phase (returns 0).
fn effective_rate(base: u8, ks: u8, kc: u8) -> u8 {
    if base == 0 {
        return 0;
    }
    let ks_add = kc >> (3 - ks.min(3));
    (2 * base as u16 + ks_add as u16).min(63) as u8
}

/// The attenuation (10-bit domain) at which Decay hands off to Sustain, from the 4-bit `SL` field (ymfm
/// `cache.eg_sustain = (sl | ((sl+1) & 0x10)) << 5`): each step = 32 units, and the `SL=15` special case maps
/// to 31·32 = `0x3E0` (effectively silence).
fn sustain_att(sl: u8) -> i32 {
    let level = if sl == 15 { 31 } else { sl as i32 };
    level << 5
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

    /// SY-3c: the EG increment table must match ymfm's literal `s_increment_table` (`src/ymfm_fm.ipp`).
    /// Pins the generation of the per-rate attenuation steps to the ymfm fixture (Fork-1 requirement).
    #[test]
    fn increment_table_matches_ymfm() {
        // Rates 0-1 never move the envelope (all nibbles 0).
        for i in 0..8 {
            assert_eq!(attenuation_increment(0, i), 0, "rate 0 index {i}");
            assert_eq!(attenuation_increment(1, i), 0, "rate 1 index {i}");
        }
        // Rate 2 = 0x1010_1010: nibbles alternate 0,1 (the slowest non-frozen rate).
        assert_eq!(attenuation_increment(2, 0), 0);
        assert_eq!(attenuation_increment(2, 1), 1);
        assert_eq!(attenuation_increment(2, 2), 0);
        assert_eq!(attenuation_increment(2, 3), 1);
        // Interior rows, verbatim from ymfm.
        assert_eq!(S_INCREMENT_TABLE[6], 0x1110_1110);
        assert_eq!(S_INCREMENT_TABLE[11], 0x1111_1110);
        assert_eq!(S_INCREMENT_TABLE[48], 0x1111_1111);
        assert_eq!(S_INCREMENT_TABLE[52], 0x2222_2222);
        // High-rate region: rate 48 adds 1/step, 52 adds 2, 56 adds 4, 60-63 add 8 (fastest).
        for i in 0..8 {
            assert_eq!(attenuation_increment(48, i), 1, "rate 48 index {i}");
            assert_eq!(attenuation_increment(52, i), 2, "rate 52 index {i}");
            assert_eq!(attenuation_increment(56, i), 4, "rate 56 index {i}");
            assert_eq!(attenuation_increment(60, i), 8, "rate 60 index {i}");
            assert_eq!(attenuation_increment(63, i), 8, "rate 63 index {i}");
        }
    }

    /// SY-3c: the effective-rate / key-scaling formula `2·rate_param + (keycode >> (3 − KS))` (clamped to 63,
    /// `rate_param == 0` frozen), spot-checked across the KS shift and the clamp.
    #[test]
    fn effective_rate_key_scaling() {
        // rate_param 0 → frozen regardless of key-scale.
        assert_eq!(effective_rate(0, 3, 31), 0);
        // KS=0: keyscale = kc >> 3. AR=31, kc=18 → 62 + 2 = 64 → clamped to 63.
        assert_eq!(effective_rate(31, 0, 18), 63);
        // KS=3: keyscale = kc >> 0 = kc. rate_param=10, kc=28 → 20 + 28 = 48.
        assert_eq!(effective_rate(10, 3, 28), 48);
        // KS=0: rate_param=10, kc=28 → 20 + (28 >> 3 = 3) = 23.
        assert_eq!(effective_rate(10, 0, 28), 23);
        // KS=1: keyscale = kc >> 2. rate_param=5, kc=16 → 10 + (16 >> 2 = 4) = 14.
        assert_eq!(effective_rate(5, 1, 16), 14);
        // Release path: (rr<<1)|1 with rr=15 → 31; KS=0, kc=0 → 62.
        assert_eq!(effective_rate(31, 0, 0), 62);
    }

    /// SY-3c / audit F2: key-on is edge-triggered. Re-asserting `$28` for a held note must NOT restart the
    /// attack (no phase reset, no envelope reset); a genuine key-off→key-on edge DOES restart from phase 0.
    #[test]
    fn key_on_is_edge_triggered() {
        let mut fm = Ym2612Synth::new(44_100);
        fm.write(0, 0xB0, 0x07);
        fm.write(0, 0x30, 0x01); // MUL=1
        fm.write(0, 0x40, 0x00); // TL=0
        fm.write(0, 0x50, 0x08); // KS=0, AR=8 (gradual attack, not the AR>=62 instant path)
        fm.write(0, 0x60, 0x00);
        fm.write(0, 0x70, 0x00);
        fm.write(0, 0x80, 0x00);
        fm.write(0, 0xA4, 0x24);
        fm.write(0, 0xA0, 0x3B);
        fm.write(0, 0xB4, 0xC0);
        fm.write(0, 0x28, 0x10); // key on op1 (rising edge → attack, phase reset to 0)

        // Advance: the phase winds forward and the gradual attack stays in Attack.
        for _ in 0..500 {
            let _ = fm.next_sample();
        }
        let phase_before = fm.channels[0].ops[0].phase;
        let env_before = fm.channels[0].ops[0].env;
        assert_ne!(phase_before, 0, "phase should have advanced");
        assert_eq!(
            fm.channels[0].ops[0].eg,
            EgState::Attack,
            "gradual AR keeps this op in Attack"
        );

        // Re-assert the SAME key-on (held note) → must be a no-op.
        fm.write(0, 0x28, 0x10);
        assert_eq!(
            fm.channels[0].ops[0].phase, phase_before,
            "held re-key must not reset phase"
        );
        assert_eq!(
            fm.channels[0].ops[0].env, env_before,
            "held re-key must not reset the envelope"
        );
        assert_eq!(
            fm.channels[0].ops[0].eg,
            EgState::Attack,
            "held re-key must not restart the attack"
        );

        // A real key-off then key-on (a 0→1 edge) DOES restart the attack from phase 0.
        fm.write(0, 0x28, 0x00);
        fm.write(0, 0x28, 0x10);
        assert_eq!(
            fm.channels[0].ops[0].phase, 0,
            "a fresh key-on edge restarts the attack from phase 0"
        );
        assert_eq!(fm.channels[0].ops[0].eg, EgState::Attack);
    }

    /// SY-3c / audit F3: while the DAC drives channel 6 (`$2B` bit7), ch6's FM output is discarded but its
    /// envelope and phase must keep advancing (so there is no stale-volume pop when the DAC later turns off).
    #[test]
    fn dac_enabled_ch6_envelope_still_advances() {
        let mut fm = Ym2612Synth::new(44_100);
        // Program ch6 (bank 1, within-part 2 → global index 5) op1 with a gradual attack.
        fm.write(1, 0xB2, 0x07); // algorithm 7
        fm.write(1, 0x32, 0x01); // MUL=1
        fm.write(1, 0x42, 0x00); // TL=0
        fm.write(1, 0x52, 0x08); // KS=0, AR=8 (gradual, not instant)
        fm.write(1, 0xA6, 0x24);
        fm.write(1, 0xA2, 0x3B);
        fm.write(0, 0x2B, 0x80); // DAC enabled → ch6 FM muted (output discarded)
        fm.write(0, 0x28, 0x16); // key on ch6 op1 (rising edge)

        let env_start = fm.channels[5].ops[0].env;
        let phase_start = fm.channels[5].ops[0].phase;
        for _ in 0..4000 {
            fm.begin_frame(735);
            let _ = fm.next_sample();
        }
        let env_end = fm.channels[5].ops[0].env;
        let phase_end = fm.channels[5].ops[0].phase;
        assert!(
            env_end < env_start,
            "ch6 envelope must keep advancing while DAC-muted ({env_start} -> {env_end})"
        );
        assert_ne!(
            phase_end, phase_start,
            "ch6 phase must keep advancing while DAC-muted"
        );
    }

    /// SY-3d: the detune table must match ymfm's literal `s_detune_adjustment` (`src/ymfm_fm.ipp`), and
    /// `detune_adjustment` must select column `DT & 3` and negate on `DT` bit 2 (Fork-1 fixture requirement).
    #[test]
    fn detune_table_matches_ymfm() {
        // Representative rows, quoted verbatim from ymfm's s_detune_adjustment (keycode = row).
        assert_eq!(S_DETUNE_ADJUSTMENT[0], [0, 0, 1, 2]);
        assert_eq!(S_DETUNE_ADJUSTMENT[4], [0, 1, 2, 2]);
        assert_eq!(S_DETUNE_ADJUSTMENT[15], [0, 2, 5, 7]);
        assert_eq!(S_DETUNE_ADJUSTMENT[19], [0, 3, 7, 10]);
        assert_eq!(S_DETUNE_ADJUSTMENT[27], [0, 7, 14, 20]);
        assert_eq!(S_DETUNE_ADJUSTMENT[31], [0, 8, 16, 22]);
        // detune_adjustment: DT&3 picks the magnitude column; DT bit 2 (DT 4-7) negates it.
        assert_eq!(detune_adjustment(0, 31), 0, "DT=0 is no detune");
        assert_eq!(
            detune_adjustment(3, 31),
            22,
            "DT=3 → max positive at keycode 31"
        );
        assert_eq!(detune_adjustment(4, 31), 0, "DT=4 → -0");
        assert_eq!(
            detune_adjustment(7, 31),
            -22,
            "DT=7 → max negative at keycode 31"
        );
        assert_eq!(detune_adjustment(1, 15), 2, "row 15 col 1");
        assert_eq!(detune_adjustment(5, 15), -2, "DT=5 negates row 15 col 1");
    }

    /// SY-3d: detune is summed into the phase step before MUL (ymfm OPN `compute_phase_step`). DT=0 leaves the
    /// increment bit-identical to SY-3c; a nonzero DT shifts it by a few phase units (a few cents — never an
    /// octave); the shift is symmetric in sign and scales with MUL (proving the pre-MUL ordering).
    #[test]
    fn detune_shifts_phase_step_and_is_noop_at_zero() {
        let base = 1083u32 << 4; // the 440 Hz test note (fnum=1083, block=4 → base even)
        let sy3c = (base * 2) >> 2; // SY-3c's (base·2·MUL)>>2 with MUL=1
        assert_eq!(
            Channel::op_inc(base, 1, 0),
            sy3c,
            "DT=0 must not change the phase increment"
        );
        // Symmetric small shift around the base.
        let up = Channel::op_inc(base, 1, 22);
        let dn = Channel::op_inc(base, 1, -22);
        assert!(
            up > sy3c && dn < sy3c,
            "detune must shift the step around the base"
        );
        assert_eq!(
            up - sy3c,
            22,
            "positive detune adds the raw displacement (MUL=1)"
        );
        assert_eq!(
            sy3c - dn,
            22,
            "negative detune subtracts symmetrically (MUL=1)"
        );
        assert!(
            up - sy3c <= 30,
            "detune shift is a few phase units, not octave-scale"
        );
        // MUL scales the detune because ymfm adds it BEFORE the ×MUL multiply: with MUL=2 the shift doubles.
        let up_mul2 = Channel::op_inc(base, 2, 22);
        let base_mul2 = Channel::op_inc(base, 2, 0);
        assert_eq!(
            up_mul2 - base_mul2,
            2 * (up - sy3c),
            "detune shift scales with MUL (added before the multiplier)"
        );
    }

    /// SY-3d: two carriers on the same note with **opposite-sign** detune drift out of phase, so their sum
    /// beats (the amplitude swings through deep cancellation dips) — whereas matched detune stays phase-locked
    /// at a steady amplitude. This is the audible signature of operator detune.
    #[test]
    fn opposite_detune_produces_beating() {
        // Render two algorithm-7 carriers (Op1 op-field 0, Op2 op-field 2) at one note; return the min and max
        // windowed peak amplitude over ~2 s.
        fn windowed_peak_range(dt_op1: u8, dt_op2: u8) -> (i32, i32) {
            let sr = 44_100u32;
            let mut fm = Ym2612Synth::new(sr);
            fm.write(0, 0xB0, 0x07); // algorithm 7 (all carriers), no feedback
            for (field_reg, dt) in [(0x00u8, dt_op1), (0x08u8, dt_op2)] {
                fm.write(0, 0x30 | field_reg, ((dt & 0x07) << 4) | 0x01); // DT + MUL=1
                fm.write(0, 0x40 | field_reg, 0x00); // TL=0
                fm.write(0, 0x50 | field_reg, 0x1F); // KS=0, AR=31 (instant attack)
                fm.write(0, 0x60 | field_reg, 0x00); // D1R=0
                fm.write(0, 0x70 | field_reg, 0x00); // D2R=0
                fm.write(0, 0x80 | field_reg, 0x00); // SL=0, RR=0 (sustained)
            }
            fm.write(0, 0xA4, 0x24);
            fm.write(0, 0xA0, 0x3B);
            fm.write(0, 0xB4, 0xC0); // stereo both
            fm.write(0, 0x28, 0x30); // key on Op1 (bit4) + Op2 (bit5)
            let mut min_peak = i32::MAX;
            let mut max_peak = 0i32;
            let win = (sr / 40) as usize; // 25 ms windows
            for _ in 0..80 {
                let mut peak = 0i32;
                for _ in 0..win {
                    let (l, _r) = fm.next_sample();
                    peak = peak.max(l.abs());
                }
                min_peak = min_peak.min(peak);
                max_peak = max_peak.max(peak);
            }
            (min_peak, max_peak)
        }
        // Matched detune → phase-locked → steady amplitude (min ≈ max).
        let (lo0, hi0) = windowed_peak_range(0, 0);
        assert!(
            hi0 - lo0 < hi0 / 4,
            "matched detune must hold a steady amplitude, got {lo0}..{hi0}"
        );
        // Opposite-sign detune (+mag vs −mag) → drift → beating: deep amplitude swing.
        let (lo, hi) = windowed_peak_range(1, 5);
        assert!(
            lo * 2 < hi,
            "opposite detune must beat (deep amplitude cancellation), got {lo}..{hi}"
        );
    }

    /// SY-3e: the LFO rate/PM tables must match ymfm's literals (`clock_noise_and_lfo` / `s_lfo_pm_shifts`),
    /// and the AMS scaling ([`lfo_am_offset`]) + PM adjustment ([`opn_lfo_pm_phase_adjustment`]) must obey
    /// ymfm's shift math — including AMS=0 ⇒ no tremolo and `pm == 0` ⇒ no vibrato (Fork-1 fixture pinning).
    #[test]
    fn lfo_tables_match_ymfm() {
        // lfo_max_count, verbatim from ymfm.
        assert_eq!(LFO_MAX_COUNT, [109, 78, 72, 68, 63, 45, 9, 6]);
        // s_lfo_pm_shifts spot-checks, verbatim from ymfm (row = FMS, col = abs_pm).
        assert_eq!(S_LFO_PM_SHIFTS[0], [0x77; 8]);
        assert_eq!(
            S_LFO_PM_SHIFTS[3],
            [0x77, 0x77, 0x72, 0x72, 0x17, 0x17, 0x12, 0x12]
        );
        assert_eq!(
            S_LFO_PM_SHIFTS[7],
            [0x77, 0x77, 0x17, 0x12, 0x07, 0x07, 0x02, 0x01]
        );

        // lfo_am_offset: AMS 0/1/2/3 → shift 7/3/1/0 of (am<<1). AMS=0 is always 0 (no tremolo).
        for am in 0..=63u32 {
            assert_eq!(lfo_am_offset(am, 0), 0, "AMS=0 must never add tremolo");
        }
        assert_eq!(lfo_am_offset(63, 1), (63 << 1) >> 3); // 15
        assert_eq!(lfo_am_offset(63, 2), 63 << 1 >> 1); // 63
        assert_eq!(lfo_am_offset(63, 3), 63 << 1); // 126 (deepest)
                                                   // Depth is monotone in AMS.
        assert!(lfo_am_offset(63, 3) > lfo_am_offset(63, 2));
        assert!(lfo_am_offset(63, 2) > lfo_am_offset(63, 1));

        // opn_lfo_pm_phase_adjustment: pm==0 (and every FMS col-0 = 0x77) yields 0 → no vibrato at rest.
        for fms in 1..=7u32 {
            assert_eq!(opn_lfo_pm_phase_adjustment(0x7f, fms, 0), 0, "pm=0 ⇒ no PM");
        }
        // Sign symmetry: +pm and −pm mirror.
        let up = opn_lfo_pm_phase_adjustment(0x7f, 7, 7);
        let dn = opn_lfo_pm_phase_adjustment(0x7f, 7, -7);
        assert_eq!(up, -dn, "PM must be symmetric in sign");
        assert!(up > 0, "max +PM at FMS=7 must be positive");
        // Depth scales with FMS: deeper sensitivity ⇒ larger |adjustment| for the same pitch + PM phase.
        let d7 = opn_lfo_pm_phase_adjustment(0x7f, 7, 7);
        let d5 = opn_lfo_pm_phase_adjustment(0x7f, 5, 7);
        let d3 = opn_lfo_pm_phase_adjustment(0x7f, 3, 7);
        assert!(
            d7 > d5 && d5 > d3,
            "PM depth must grow with FMS: {d3} {d5} {d7}"
        );
        // Never octave-scale: even the deepest (FMS=7, max PM, top FNUM) nudge stays a small fraction of the
        // 12-bit FNUM (an octave = doubling the FNUM). 190 out of a ≈4064 FNUM ≈ 0.8 semitone.
        assert!(
            d7 * 8 < 0xfff,
            "PM adjustment must be a small pitch nudge, got {d7}"
        );
    }

    /// SY-3e: the LFO produces the expected AM triangle and signed PM oscillation. Clock it directly and
    /// confirm: AM spans the full 0..63 triangle; PM swings through both signs and zero; the AM period at
    /// rate 0 is ≈ 3.98 Hz (slowest); and a faster rate has a much shorter period.
    #[test]
    fn lfo_waveform_triangle_and_period() {
        fn run(rate_reg: u8, ticks: usize) -> (Vec<u32>, Vec<i32>) {
            let mut fm = Ym2612Synth::new(44_100);
            fm.write(0, 0x22, rate_reg); // enable + rate
            let mut am = Vec::with_capacity(ticks);
            let mut pm = Vec::with_capacity(ticks);
            for _ in 0..ticks {
                fm.clock_lfo();
                am.push(fm.lfo_am);
                pm.push(fm.lfo_pm);
            }
            (am, pm)
        }
        // Mean spacing (in ticks) between successive triangle minima (start of each am==0 run).
        fn period_ticks(am: &[u32]) -> f64 {
            let mut starts = Vec::new();
            let mut in_zero = false;
            for (i, &v) in am.iter().enumerate() {
                if v == 0 && !in_zero {
                    starts.push(i);
                    in_zero = true;
                } else if v != 0 {
                    in_zero = false;
                }
            }
            assert!(
                starts.len() >= 3,
                "need several periods, got {}",
                starts.len()
            );
            let diffs: Vec<usize> = starts.windows(2).map(|w| w[1] - w[0]).collect();
            diffs.iter().sum::<usize>() as f64 / diffs.len() as f64
        }

        let (am0, pm0) = run(0x08, 60_000); // rate 0
        assert_eq!(*am0.iter().min().unwrap(), 0, "AM triangle reaches 0");
        assert_eq!(*am0.iter().max().unwrap(), 63, "AM triangle reaches 63");
        assert!(pm0.iter().any(|&v| v > 0), "PM must go positive");
        assert!(pm0.iter().any(|&v| v < 0), "PM must go negative");
        assert!(pm0.contains(&0), "PM must pass through 0");

        let hz0 = NATIVE_RATE / period_ticks(&am0);
        assert!(
            (3.6..4.3).contains(&hz0),
            "rate-0 LFO AM must be ≈ 3.98 Hz, got {hz0:.2} Hz"
        );

        // Fastest rate 7 is dramatically quicker than the slowest.
        let (am7, _) = run(0x0F, 60_000);
        let hz7 = NATIVE_RATE / period_ticks(&am7);
        assert!(
            hz7 > hz0 * 8.0,
            "rate 7 ({hz7:.1} Hz) must be far faster than rate 0 ({hz0:.2} Hz)"
        );
    }

    /// SY-3e: render a sustained carrier and measure the peak-amplitude swing across 25 ms windows. With LFO
    /// tremolo enabled the amplitude wobbles; the swing must (a) vanish when AMS=0, (b) vanish when the
    /// operator's AM-enable ($60 bit7) is clear, and (c) grow with AMS.
    #[test]
    fn lfo_am_tremolo_scales_and_gates() {
        // `bfield` = $B4 low bits (ams<<4 | fms); `am_en` sets $60 bit7; `lfo` = $22 value.
        fn swing(bfield: u8, am_en: bool, lfo: u8) -> i32 {
            let sr = 44_100u32;
            let mut fm = Ym2612Synth::new(sr);
            fm.write(0, 0xB0, 0x07); // algorithm 7, bare carrier
            fm.write(0, 0x30, 0x01); // MUL=1
            fm.write(0, 0x40, 0x00); // TL=0 (full baseline; AM only attenuates)
            fm.write(0, 0x50, 0x1F); // KS=0, AR=31 (instant, steady)
            fm.write(0, 0x60, if am_en { 0x80 } else { 0x00 }); // D1R=0 + AM-enable bit
            fm.write(0, 0x70, 0x00);
            fm.write(0, 0x80, 0x00); // SL=0, RR=0 → sustained full volume
            fm.write(0, 0x22, lfo); // LFO
            fm.write(0, 0xA4, 0x24);
            fm.write(0, 0xA0, 0x3B);
            fm.write(0, 0xB4, 0xC0 | bfield); // pan both + AMS/FMS
            fm.write(0, 0x28, 0x10); // key on op1
            let win = (sr / 40) as usize; // 25 ms « the ~250 ms LFO period
            let mut lo = i32::MAX;
            let mut hi = 0i32;
            for _ in 0..80 {
                let mut peak = 0i32;
                for _ in 0..win {
                    let (l, _r) = fm.next_sample();
                    peak = peak.max(l.abs());
                }
                lo = lo.min(peak);
                hi = hi.max(peak);
            }
            hi - lo
        }
        // AMS=3 (0x30) with AM enabled and the LFO on → deep tremolo swing.
        let deep = swing(0x30, true, 0x08);
        // AMS=1 (0x10) → shallower swing.
        let shallow = swing(0x10, true, 0x08);
        // AMS=0 → no tremolo even with AM enabled + LFO on.
        let ams0 = swing(0x00, true, 0x08);
        // AM-enable clear → no tremolo even at AMS=3 + LFO on.
        let gated = swing(0x30, false, 0x08);

        assert!(
            deep > shallow,
            "AMS=3 must wobble deeper than AMS=1 ({shallow} vs {deep})"
        );
        assert!(
            shallow > 100,
            "AMS=1 must still show real tremolo, got {shallow}"
        );
        assert!(
            ams0 < shallow / 4,
            "AMS=0 must give ~no tremolo, got {ams0}"
        );
        assert!(
            gated < shallow / 4,
            "AM-enable clear must gate the tremolo, got {gated}"
        );
    }

    /// SY-3e / byte-identity: a disabled LFO must leave the SY-3d datapath untouched. A rich patch (AMS=3,
    /// FMS=7, AM-enable) rendered with the LFO **off** is sample-for-sample identical to the same note with
    /// no AMS/FMS/AM-enable at all; and the same rich patch with the LFO **on** genuinely differs (the LFO
    /// is doing something).
    #[test]
    fn lfo_disabled_is_byte_identical_to_sy3d() {
        fn render(bfield: u8, am_en: bool, lfo: u8, n: usize) -> Vec<(i32, i32)> {
            let mut fm = Ym2612Synth::new(44_100);
            fm.write(0, 0xB0, 0x07);
            fm.write(0, 0x30, 0x71); // DT=7, MUL=1 (exercise the detune path too)
            fm.write(0, 0x40, 0x00);
            fm.write(0, 0x50, 0x1F);
            fm.write(0, 0x60, if am_en { 0x80 } else { 0x00 });
            fm.write(0, 0x70, 0x00);
            fm.write(0, 0x80, 0x00);
            fm.write(0, 0x22, lfo);
            fm.write(0, 0xA4, 0x24);
            fm.write(0, 0xA0, 0x3B);
            fm.write(0, 0xB4, 0xC0 | bfield);
            fm.write(0, 0x28, 0x10);
            (0..n).map(|_| fm.next_sample()).collect()
        }
        // Rich patch, LFO OFF ($22=0) vs bare patch, LFO OFF → identical (disabled LFO neutralizes AMS/FMS/AM).
        let rich_off = render(0x37 /* AMS=3, FMS=7 */, true, 0x00, 20_000);
        let bare_off = render(0x00, false, 0x00, 20_000);
        assert_eq!(
            rich_off, bare_off,
            "a disabled LFO must be bit-identical to SY-3d (no AMS/FMS/AM effect)"
        );
        // Same rich patch, LFO ON → must differ (proving the LFO changes the output).
        let rich_on = render(0x37, true, 0x08, 20_000);
        assert_ne!(rich_on, rich_off, "an enabled LFO must change the output");
    }

    /// SY-3e: FMS vibrato perturbs pitch. With the LFO on, FMS=7 diverges from FMS=0 (same note), whereas
    /// FMS=0 with the LFO on stays identical to the LFO fully disabled (no PM path taken).
    #[test]
    fn lfo_fms_vibrato_moves_pitch() {
        fn render(fms: u8, lfo: u8, n: usize) -> Vec<i32> {
            let mut fm = Ym2612Synth::new(44_100);
            fm.write(0, 0xB0, 0x07);
            fm.write(0, 0x30, 0x01);
            fm.write(0, 0x40, 0x00);
            fm.write(0, 0x50, 0x1F);
            fm.write(0, 0x60, 0x00);
            fm.write(0, 0x70, 0x00);
            fm.write(0, 0x80, 0x00);
            fm.write(0, 0x22, lfo);
            fm.write(0, 0xA4, 0x24);
            fm.write(0, 0xA0, 0x3B);
            fm.write(0, 0xB4, 0xC0 | fms);
            fm.write(0, 0x28, 0x10);
            (0..n).map(|_| fm.next_sample().0).collect()
        }
        let no_vib = render(0x00, 0x08, 30_000); // LFO on, FMS=0 → no PM
        let vib = render(0x07, 0x08, 30_000); // LFO on, FMS=7 → vibrato
        let lfo_off = render(0x07, 0x00, 30_000); // LFO off entirely
                                                  // FMS=0 takes no PM path → identical to the LFO being off.
        assert_eq!(
            no_vib, lfo_off,
            "FMS=0 must leave the phase step unperturbed"
        );
        // FMS=7 vibrato genuinely bends the pitch → the waveform drifts out of step with the un-modulated one.
        let diverged = no_vib.iter().zip(&vib).filter(|(a, b)| a != b).count();
        assert!(
            diverged > no_vib.len() / 10,
            "FMS=7 vibrato must move the pitch (only {diverged} samples differed)"
        );
    }
}
