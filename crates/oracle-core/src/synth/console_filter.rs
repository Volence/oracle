//! The Mega Drive's **analog output stage** — a first-order (one-pole) low-pass on the final mix.
//!
//! ## Why this exists
//!
//! Our synth emits samples straight from the chip mix. Real hardware does not: the YM2612/SN76489 mix
//! leaves the console through a discrete RC low-pass before it reaches the AV jack. Without it we emit a
//! reconstruction staircase with far more high-frequency energy than any console ever produced —
//! measurably ~23x (13.6 dB) the artifact-band power of a Model 1 in the S3K "SEGA" chant, heard as
//! "poppy/crackly".
//!
//! This is a **hardware-accuracy** fix, not a taste knob: the filter is on the real signal path, so
//! omitting it is the deviation. It is deliberately applied to the FINAL MIX (FM + PSG + DAC), because on
//! hardware the RC network sits downstream of everything.
//!
//! ## There is no single "accurate" cutoff
//!
//! The corner is **revision-dependent**, so this is *model selection*, not one number to hardcode:
//!
//! | Board | Output stage | Modelled here |
//! |---|---|---|
//! | Model 1, VA0-VA2 | one-pole, 3386 Hz | yes |
//! | Model 1 VA3-VA6 / Model 2 | one-pole, 2842 Hz | yes |
//! | Model 1 VA7, Model 2 VA0-VA1.8 | *second*-order Sallen-Key, high Q | **no** |
//!
//! Because no revision is "the" right answer, [`ConsoleModel`] names each one and **the default is
//! deliberately [`ConsoleModel::Unfiltered`]** — that keeps the pre-existing output bit-identical while
//! the choice is made by ear on A/B renders. Selecting a variant is a one-line call
//! ([`crate::synth::AudioSink::set_console_model`]); there is intentionally no config-file layer.
//!
//! ### Where the numbers come from — they are COMPUTED, not measured
//!
//! Both cutoffs fall straight out of `f = 1/(2*pi*R*C)` with the pre-amp's 10 kOhm feedback resistor:
//!
//! | R | C | `1/(2*pi*R*C)` | quoted in sources as |
//! |---|---|---|---|
//! | 10 kOhm | 4700 pF | 3386.3 Hz | "3.39 kHz" / "3.38 kHz" |
//! | 10 kOhm | 5600 pF | 2842.1 Hz | "2.84 kHz" |
//!
//! The capacitor values are stated directly in the Sega-16 "Mega Amp" thread (VA3-VA6 ships 5600 pF;
//! VA0-VA2 is equivalent to 4700 pF), and ConsoleMods quotes the same two knees for a Model 2 mod plus a
//! third data point (1000 pF -> "15.8 kHz"; `1/(2*pi*10k*1n)` = 15915 Hz) that closes the arithmetic
//! three-for-three. Two consequences:
//!
//! - These are **not precision targets**. Era-typical ceramics run +/-10%, so real consoles scatter by a
//!   few hundred Hz around nominal. Chasing the third digit would be false precision.
//! - The 10 kOhm Model 1 feedback resistor is **inferred** from the arithmetic closing on three
//!   independent quoted knees, not read off a schematic. Strong, but not a printed source.
//!
//! Sources:
//! - jsgroth, "Genesis & Sega CD - Audio Filtering" <https://jsgroth.dev/blog/posts/genesis-audio-filtering/>
//! - jsgroth, "Emulating the YM2612: Part 5 - Analog Output"
//!   <https://jsgroth.dev/blog/posts/emulating-ym2612-part-5/>
//! - Sega-16 "Mega Amp" thread (capacitor values; the VA0-VA2 vs VA3-VA6 grouping)
//!   <http://www.sega-16.com/forum/archive/index.php/t-26568-p-3.html>
//! - ConsoleMods, "Genesis: Audio Circuit Mod" <https://consolemods.org/wiki/Genesis:Audio_Circuit_Mod>
//!
//! **Provenance caveat.** Both Sega-16 sources are by the same author, and the ConsoleMods page describes a
//! mod deliberately tuned to hit those same knees. This is a coherent, self-consistent *single-authority*
//! account corroborated by arithmetic — not independent replication. It is the best available evidence and
//! worth implementing against; it should not be logged as multiply-confirmed measurement. Ace also notes
//! VA0-VA2 conversion needs new pre-amp *resistors as well as* capacitors, so the tidy "same R, different C"
//! story above is itself an approximation.
//!
//! **Known limitation — later boards are NOT "unfiltered".** Model 1 VA7 and Model 2 VA0-VA1.8 carry a
//! *second-order Sallen-Key* stage with a deliberately high Q, which peaks near cutoff instead of rolling
//! off monotonically. A one-pole cannot reproduce that at any frequency, so those boards are **not**
//! modelled — do not reach for [`ConsoleModel::Unfiltered`] as a stand-in. A second-order variant is a
//! separate, additive follow-up.
//!
//! No hardware source was found for a **high-pass / DC-blocking** corner; jgenesis's 5 Hz is that
//! project's own engineering choice, so none is modelled here.
//!
//! Neither `ymfm` (this project's pinned OPN2 reference) nor anything under `docs/reference/` models this
//! stage: `ymfm` stops at the digital chip output, which is the correct scope for a *chip* emulator but
//! leaves the console's analog stage to the *system* emulator — us.
//!
//! ## Discretisation — bilinear transform, pinned against the reference
//!
//! The analog RC pole is mapped to a first-order IIR by the **bilinear transform** with `K = tan(pi*fc/fs)`:
//!
//! ```text
//! b0 = b1 = K / (1 + K)        a1 = (K - 1) / (1 + K)
//! y[n] = b0*(x[n] + x[n-1]) - a1*y[n-1]
//! ```
//!
//! This is not a free choice — it is what the reference implementation does, and we can prove it. jgenesis
//! publishes its 3390 Hz coefficients at `fs = 53267.0387` Hz as
//! `b = [0.1684983368367697, 0.1684983368367697]`, `a = [1.0, -0.6630033263264605]`. The equal `b` pair is
//! already the bilinear signature, and evaluating the formulas above at that cutoff and rate reproduces
//! both numbers to ten significant figures. An impulse-invariant `a = 1 - exp(-2*pi*fc/fs)` gives 0.3295
//! instead — a different filter. `coefficients_match_the_published_reference` pins this.
//!
//! Why it matters beyond provenance: the bilinear form has a true zero at Nyquist, so it keeps attenuating
//! across the top of the band, whereas an impulse-invariant one-pole flattens out there and under-attenuates
//! by ~1.7 dB at 15 kHz — exactly the band this change exists to clean up.
//!
//! Unity DC gain falls out exactly (`(b0 + b1) / (1 + a1) = 1`), so the filter can neither shift the mix's
//! level nor introduce a DC offset.

/// Which console revision's analog output stage to model.
///
/// The two filtered variants are real hardware, differing only in the pre-amp RC values. The third is
/// "no output stage at all", which is a *measurement baseline*, not a board — see [`Self::Unfiltered`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleModel {
    /// Model 1, VA0-VA2 — one-pole low-pass at 3386 Hz. The brightest filtered revision.
    Model1Va0Va2,
    /// Model 1 VA3-VA6 / Model 2 — one-pole low-pass at 2842 Hz. Darker.
    Model1Va3Va6,
    /// **No output stage** — the raw chip mix, exactly as rendered before SY-6b.
    ///
    /// This is the current default *only* because it keeps the pre-existing output bit-identical while
    /// the revision to ship is still being chosen by ear. It is a measurement baseline and a safe
    /// default, **not a console**: every Mega Drive revision filters its output somehow. In particular
    /// this is *not* a model of Model 1 VA7, which carries a strong second-order Sallen-Key stage and is
    /// if anything more filtered than VA0-VA2, not less.
    #[default]
    Unfiltered,
}

impl ConsoleModel {
    /// The output-stage low-pass cutoff (-3 dB) in Hz, or `None` for a revision with no low-pass.
    pub fn cutoff_hz(self) -> Option<f64> {
        match self {
            // 1/(2*pi * 10 kOhm * 4700 pF)
            ConsoleModel::Model1Va0Va2 => Some(3386.3),
            // 1/(2*pi * 10 kOhm * 5600 pF)
            ConsoleModel::Model1Va3Va6 => Some(2842.1),
            ConsoleModel::Unfiltered => None,
        }
    }

    /// A short stable identifier, for logs and dev tools.
    pub fn name(self) -> &'static str {
        match self {
            ConsoleModel::Model1Va0Va2 => "model1-va0-va2",
            ConsoleModel::Model1Va3Va6 => "model1-va3-va6",
            ConsoleModel::Unfiltered => "unfiltered",
        }
    }

    /// Parse a [`Self::name`] identifier, for dev tools and examples.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "model1-va0-va2" | "va0" | "va2" => Some(ConsoleModel::Model1Va0Va2),
            "model1-va3-va6" | "va3" | "va6" => Some(ConsoleModel::Model1Va3Va6),
            "unfiltered" | "none" | "off" | "raw" => Some(ConsoleModel::Unfiltered),
            _ => None,
        }
    }

    /// Every modelled revision, for A/B tooling.
    pub const ALL: [ConsoleModel; 3] = [
        ConsoleModel::Model1Va0Va2,
        ConsoleModel::Model1Va3Va6,
        ConsoleModel::Unfiltered,
    ];
}

/// A one-pole (first-order) low-pass, bilinear-transformed from an analog RC:
/// `y[n] = b0*(x[n] + x[n-1]) - a1*y[n-1]`.
///
/// The pole `-a1 = (1 - K)/(1 + K)` lies strictly inside the unit circle for every `K > 0`, so the filter
/// is unconditionally stable. DC gain is exactly 1, and there is an exact zero at Nyquist.
#[derive(Debug, Clone, Copy)]
pub struct OnePoleLowPass {
    /// Feed-forward coefficient; `b0 == b1` for the bilinear one-pole.
    b0: f64,
    /// Feedback coefficient (note the sign convention: `y[n] = ... - a1*y[n-1]`).
    a1: f64,
    /// `x[n-1]`.
    x1: f64,
    /// `y[n-1]`.
    y1: f64,
}

impl OnePoleLowPass {
    /// A filter with the given -3 dB cutoff at the given sample rate, starting at rest.
    pub fn new(cutoff_hz: f64, sample_rate: u32) -> Self {
        let fs = sample_rate.max(1) as f64;
        // Clamp the degenerate ends so `K` stays finite and positive: `tan` blows up as fc -> fs/2, and a
        // non-positive cutoff would give K <= 0 (an unstable or stuck filter).
        let fc = cutoff_hz.clamp(1.0, fs * 0.499);
        let k = (std::f64::consts::PI * fc / fs).tan();
        Self {
            b0: k / (1.0 + k),
            a1: (k - 1.0) / (1.0 + k),
            x1: 0.0,
            y1: 0.0,
        }
    }

    /// Feed one sample and return the filtered output.
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * (x + self.x1) - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    /// Clear the filter memory (e.g. on reset), so no audio survives across a discontinuity.
    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    /// The feed-forward coefficient `b0` (`== b1`). Exposed for tests.
    pub fn b0(&self) -> f64 {
        self.b0
    }

    /// The feedback coefficient `a1`. Exposed for tests.
    pub fn a1(&self) -> f64 {
        self.a1
    }

    /// The filter's magnitude response at `f` Hz, as a linear amplitude gain.
    ///
    /// `H(z) = b0*(1 + z^-1) / (1 + a1*z^-1)` evaluated on the unit circle at `w = 2*pi*f/fs`.
    pub fn magnitude_at(&self, freq_hz: f64, sample_rate: u32) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq_hz / sample_rate.max(1) as f64;
        let (sin, cos) = w.sin_cos();
        let num = self.b0 * ((1.0 + cos) * (1.0 + cos) + sin * sin).sqrt();
        let den = ((1.0 + self.a1 * cos) * (1.0 + self.a1 * cos)
            + (self.a1 * sin) * (self.a1 * sin))
            .sqrt();
        num / den
    }
}

/// The console's stereo output stage: one independent [`OnePoleLowPass`] per channel.
///
/// For [`ConsoleModel::Unfiltered`] the stage holds no filter and
/// [`Self::process`] returns its input **unchanged** — not "filtered with a very high cutoff". That exact
/// identity is what makes the unfiltered path bit-identical to the pre-SY-6b output.
#[derive(Debug, Clone, Copy)]
pub struct ConsoleOutputFilter {
    model: ConsoleModel,
    /// `None` for a revision with no output low-pass.
    stereo: Option<(OnePoleLowPass, OnePoleLowPass)>,
}

impl ConsoleOutputFilter {
    /// The output stage of `model` at `sample_rate`.
    pub fn new(model: ConsoleModel, sample_rate: u32) -> Self {
        let stereo = model.cutoff_hz().map(|fc| {
            let f = OnePoleLowPass::new(fc, sample_rate);
            (f, f)
        });
        Self { model, stereo }
    }

    /// Which board revision this models.
    pub fn model(&self) -> ConsoleModel {
        self.model
    }

    /// Whether this revision actually filters (false for [`ConsoleModel::Unfiltered`]).
    pub fn is_filtering(&self) -> bool {
        self.stereo.is_some()
    }

    /// Filter one stereo sample pair. Exactly the identity for an unfiltered revision.
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        match &mut self.stereo {
            Some((fl, fr)) => (fl.process(l), fr.process(r)),
            None => (l, r),
        }
    }

    /// Clear both channels' memory.
    pub fn reset(&mut self) {
        if let Some((l, r)) = &mut self.stereo {
            l.reset();
            r.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: u32 = 44_100;

    /// The two modelled board revisions carry the measured cutoffs, and they are genuinely different.
    #[test]
    fn console_models_carry_their_measured_cutoffs() {
        assert_eq!(ConsoleModel::Model1Va0Va2.cutoff_hz(), Some(3386.3));
        assert_eq!(ConsoleModel::Model1Va3Va6.cutoff_hz(), Some(2842.1));
        assert_eq!(
            ConsoleModel::Unfiltered.cutoff_hz(),
            None,
            "the unfiltered baseline has no output low-pass"
        );
        assert!(
            ConsoleModel::Model1Va3Va6.cutoff_hz() < ConsoleModel::Model1Va0Va2.cutoff_hz(),
            "the later filtered board is the darker one"
        );
        // The default is deliberately the UNFILTERED revision: the revision to ship is still being
        // chosen by ear, and this keeps the pre-existing output bit-identical until it is.
        assert_eq!(ConsoleModel::default(), ConsoleModel::Unfiltered);
        // Every variant round-trips through its name, so dev tools can select one.
        for m in ConsoleModel::ALL {
            assert_eq!(ConsoleModel::from_name(m.name()), Some(m));
        }
        assert_eq!(ConsoleModel::from_name("nonsense"), None);
    }

    /// The pole must stay strictly inside the unit circle — the stability condition — for everything from
    /// a subsonic cutoff to one past Nyquist, including hostile inputs that would otherwise make `tan`
    /// blow up or go negative.
    #[test]
    fn pole_is_always_inside_the_unit_circle() {
        for &fc in &[-100.0, 0.0, 1.0, 100.0, 2842.1, 3386.3, 20_000.0, 1.0e9] {
            let f = OnePoleLowPass::new(fc, FS);
            let pole = -f.a1();
            assert!(
                pole.abs() < 1.0,
                "pole {pole} outside the unit circle for fc {fc}"
            );
            assert!(f.b0() > 0.0 && f.b0() < 1.0, "b0 {} out of range", f.b0());
            // Unity DC gain, (b0 + b1) / (1 + a1), must hold for every one of them.
            let dc = 2.0 * f.b0() / (1.0 + f.a1());
            assert!((dc - 1.0).abs() < 1e-9, "DC gain {dc} != 1 for fc {fc}");
        }
    }

    /// Our coefficients must reproduce the reference implementation's published ones. jgenesis publishes,
    /// for a 3390 Hz one-pole at fs = 53267.0387 Hz:
    /// `b = [0.1684983368367697, 0.1684983368367697]`, `a = [1.0, -0.6630033263264605]`.
    ///
    /// This is the test that settles *which discretisation* is correct: the bilinear transform hits both
    /// numbers, while an impulse-invariant `1 - exp(-2*pi*fc/fs)` would give 0.3296 for `b0`.
    #[test]
    fn coefficients_match_the_published_reference() {
        // The reference rate is not an integer; the nearest integer rate moves the coefficients by ~1e-6,
        // so compare at that tolerance rather than pretending to bit-equality.
        let f = OnePoleLowPass::new(3390.0, 53_267);
        assert!(
            (f.b0() - 0.168_498_336_836_769_7).abs() < 1e-6,
            "b0 {} != published 0.1684983368367697",
            f.b0()
        );
        assert!(
            (f.a1() - (-0.663_003_326_326_460_5)).abs() < 1e-6,
            "a1 {} != published -0.6630033263264605",
            f.a1()
        );
        // Guard against silently reverting to the impulse-invariant form.
        let impulse_invariant = 1.0 - (-2.0 * std::f64::consts::PI * 3390.0 / 53267.0387_f64).exp();
        assert!(
            (f.b0() - impulse_invariant).abs() > 0.1,
            "b0 must NOT be the impulse-invariant alpha"
        );
    }

    /// DC preservation: a constant input converges to exactly that constant, and then stays there. This
    /// is the property that guarantees the filter cannot change the mix's level or add a DC offset.
    #[test]
    fn dc_input_converges_to_unity_gain() {
        let mut f = OnePoleLowPass::new(3386.3, FS);
        let dc = 12_345.0;
        for _ in 0..2000 {
            f.process(dc);
        }
        let y = f.process(dc);
        assert!(
            (y - dc).abs() < 1e-6,
            "DC gain must be 1: got {y} for input {dc}"
        );
    }

    /// Step response at the real cutoffs: monotonically rising, never overshooting (a one-pole cannot
    /// ring), and settling at exactly the step height.
    #[test]
    fn step_response_is_monotone_and_settles_exactly() {
        for model in [ConsoleModel::Model1Va0Va2, ConsoleModel::Model1Va3Va6] {
            let mut f = OnePoleLowPass::new(model.cutoff_hz().unwrap(), FS);
            let mut prev = 0.0;
            for i in 1..=4000 {
                let y = f.process(1.0);
                assert!(y >= prev, "{}: step must be monotone (n={i})", model.name());
                assert!(
                    y <= 1.0,
                    "{}: must never overshoot (n={i}: {y})",
                    model.name()
                );
                prev = y;
            }
            assert!(
                (prev - 1.0).abs() < 1e-9,
                "{}: the step must settle at exactly 1, got {prev}",
                model.name()
            );
        }
    }

    /// The discretisation must converge on the analog RC it models: given enough oversampling, the step
    /// response reaches 63.2% after one time constant (`t = 1/(2*pi*fc)` seconds), the defining property
    /// of the RC circuit.
    ///
    /// This is deliberately checked at a heavily oversampled cutoff. At the console's own 3386 Hz one time
    /// constant spans only ~2.1 samples at 44.1 kHz, where *any* discretisation departs visibly from the
    /// continuous exponential — testing it there would pin a sampling artifact rather than the filter.
    #[test]
    fn step_response_approaches_the_analog_time_constant_when_oversampled() {
        for &fc in &[50.0, 100.0, 400.0] {
            let mut f = OnePoleLowPass::new(fc, FS);
            let n_tau = (FS as f64 / (2.0 * std::f64::consts::PI * fc)).round() as usize;
            let mut at_tau = 0.0;
            for i in 1..=n_tau {
                at_tau = f.process(1.0);
                let _ = i;
            }
            assert!(
                (at_tau - 0.632).abs() < 0.01,
                "fc {fc}: one time constant should reach ~63.2%, got {at_tau}"
            );
        }
    }

    /// Impulse response: `h[0] = b0`, `h[1] = b0*(1 - a1)`, then a geometric decay by the pole `-a1`.
    /// It must converge to zero rather than sustaining or growing, and its sum (the DC gain) must be 1.
    #[test]
    fn impulse_response_decays_geometrically() {
        let mut f = OnePoleLowPass::new(3386.3, FS);
        let pole = -f.a1();
        let h0 = f.process(1.0);
        assert!((h0 - f.b0()).abs() < 1e-12, "h[0] = b0");
        let h1 = f.process(0.0);
        assert!(
            (h1 - f.b0() * (1.0 - f.a1())).abs() < 1e-12,
            "h[1] = b0*(1 - a1)"
        );
        let mut sum = h0 + h1;
        let mut prev = h1;
        // Stop once it has decayed into the noise: past that point the samples underflow to exactly 0.0
        // and a strict "smaller than the last one" check would compare 0 against 0.
        for _ in 0..200 {
            let y = f.process(0.0);
            assert!(
                (y - prev * pole).abs() < 1e-12,
                "h[n] must decay by exactly the pole"
            );
            assert!(y.abs() < prev.abs(), "impulse response must decay");
            sum += y;
            prev = y;
        }
        assert!(prev.abs() < 1e-9, "impulse response must reach zero");
        // Drain the remaining tail so the sum below is the complete DC gain.
        for _ in 0..2000 {
            sum += f.process(0.0);
        }
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "the impulse response must sum to the DC gain of 1, got {sum}"
        );
    }

    /// The cutoff really is the -3 dB point, and the skirt falls at the first-order 6 dB/octave rate.
    #[test]
    fn magnitude_response_is_minus_three_db_at_cutoff() {
        for model in [ConsoleModel::Model1Va0Va2, ConsoleModel::Model1Va3Va6] {
            let fc = model.cutoff_hz().expect("a filtered revision");
            let f = OnePoleLowPass::new(fc, FS);
            let db = |x: f64| 20.0 * x.log10();

            assert!(db(f.magnitude_at(1.0, FS)).abs() < 0.01, "unity at DC");
            let at_fc = db(f.magnitude_at(fc, FS));
            assert!(
                (at_fc + 3.0).abs() < 0.35,
                "{}: expected -3 dB at {fc} Hz, got {at_fc:.2} dB",
                model.name()
            );
            // Above cutoff the first-order asymptote holds: each octave costs roughly 6 dB.
            let oct1 = db(f.magnitude_at(fc * 2.0, FS));
            let oct2 = db(f.magnitude_at(fc * 4.0, FS));
            let slope = oct2 - oct1;
            assert!(
                (-8.5..=-4.5).contains(&slope),
                "{}: expected roughly 6 dB/octave, got {slope:.2} dB",
                model.name()
            );
            // Monotone rolloff all the way to Nyquist, where the bilinear form has an exact zero.
            let mut last = f.magnitude_at(fc, FS);
            for step in 1..=40 {
                let hz = fc + step as f64 * (FS as f64 / 2.0 - fc) / 40.0;
                let m = f.magnitude_at(hz, FS);
                assert!(
                    m <= last,
                    "{}: response must fall at {hz:.0} Hz",
                    model.name()
                );
                last = m;
            }
            assert!(
                f.magnitude_at(FS as f64 / 2.0, FS) < 1e-12,
                "{}: a bilinear one-pole must have a zero at Nyquist",
                model.name()
            );
        }
    }

    /// The point of the whole change: the filter must crush the DAC's reconstruction artifact band
    /// (above ~7.4 kHz) far harder than it touches the signal band (below ~5 kHz).
    #[test]
    fn artifact_band_is_attenuated_far_more_than_the_passband() {
        let f = OnePoleLowPass::new(ConsoleModel::Model1Va0Va2.cutoff_hz().unwrap(), FS);
        let db = |x: f64| 20.0 * x.log10();
        let at_1k = db(f.magnitude_at(1_000.0, FS));
        let at_15k = db(f.magnitude_at(15_000.0, FS));
        assert!(
            at_1k > -1.5,
            "1 kHz must stay essentially intact: {at_1k:.2} dB"
        );
        assert!(
            at_15k < -13.0,
            "15 kHz reconstruction artifact must be crushed: {at_15k:.2} dB"
        );
        assert!(
            at_1k - at_15k > 11.0,
            "artifact band must be attenuated far more than the passband (contrast {:.1} dB)",
            at_1k - at_15k
        );
    }

    /// The stereo stage filters its channels independently — a signal on the left must not leak right.
    #[test]
    fn stereo_channels_are_independent() {
        let mut c = ConsoleOutputFilter::new(ConsoleModel::Model1Va0Va2, FS);
        for _ in 0..200 {
            let (_, r) = c.process(10_000.0, 0.0);
            assert_eq!(r, 0.0, "a left-only signal must not bleed into the right");
        }
        let (l, _) = c.process(10_000.0, 0.0);
        assert!(
            l > 9_000.0,
            "left should have settled near its input, got {l}"
        );
    }

    /// `reset` clears the memory, so a filter that has been driven hard starts a new run from silence.
    #[test]
    fn reset_clears_filter_memory() {
        let mut c = ConsoleOutputFilter::new(ConsoleModel::Model1Va0Va2, FS);
        for _ in 0..500 {
            c.process(20_000.0, -20_000.0);
        }
        c.reset();
        let (l, r) = c.process(0.0, 0.0);
        assert_eq!((l, r), (0.0, 0.0), "reset must leave no residue");
    }

    /// The unfiltered revision must be the EXACT identity — bit-identical, not "nearly". This is the
    /// property that lets the default path stay byte-for-byte what it was before SY-6b.
    #[test]
    fn unfiltered_revision_is_the_exact_identity() {
        let mut c = ConsoleOutputFilter::new(ConsoleModel::Unfiltered, FS);
        assert!(!c.is_filtering(), "the unfiltered baseline must not filter");
        // Values chosen to expose any smoothing: alternating full-scale, plus awkward fractions.
        for (i, &v) in [32_767.0, -32_768.0, 0.0, 1.0, -1.0, 12_345.678, -9_999.5]
            .iter()
            .cycle()
            .take(200)
            .enumerate()
        {
            let (l, r) = c.process(v, -v);
            assert_eq!(l, v, "left must pass through untouched (sample {i})");
            assert_eq!(r, -v, "right must pass through untouched (sample {i})");
        }
        c.reset(); // must be a harmless no-op
        assert_eq!(c.process(7.0, -7.0), (7.0, -7.0));
    }

    /// The filtered revisions really do filter — guarding against a wiring slip that leaves every
    /// variant as a pass-through.
    #[test]
    fn filtered_revisions_actually_filter() {
        for model in [ConsoleModel::Model1Va0Va2, ConsoleModel::Model1Va3Va6] {
            let mut c = ConsoleOutputFilter::new(model, FS);
            assert!(c.is_filtering(), "{} must filter", model.name());
            let (l, _) = c.process(10_000.0, 0.0);
            assert!(
                l < 10_000.0,
                "{}: the first sample of a step must be attenuated, got {l}",
                model.name()
            );
        }
    }

    /// A higher cutoff must pass a given high frequency more than a lower one — i.e. the model selector
    /// actually changes the sound in the expected direction.
    #[test]
    fn later_board_is_darker_at_the_same_frequency() {
        let early = OnePoleLowPass::new(ConsoleModel::Model1Va0Va2.cutoff_hz().unwrap(), FS);
        let late = OnePoleLowPass::new(ConsoleModel::Model1Va3Va6.cutoff_hz().unwrap(), FS);
        for &f in &[2_000.0, 5_000.0, 10_000.0] {
            assert!(
                late.magnitude_at(f, FS) < early.magnitude_at(f, FS),
                "the VA3-VA6 board must pass less at {f} Hz"
            );
        }
    }

    /// The coefficient must track the sample rate: the same cutoff at a higher rate needs a smaller
    /// alpha, and the response at a given *frequency* must stay put.
    #[test]
    fn cutoff_is_sample_rate_independent() {
        let db = |x: f64| 20.0 * x.log10();
        let f44 = OnePoleLowPass::new(3386.3, 44_100);
        let f48 = OnePoleLowPass::new(3386.3, 48_000);
        assert!(
            f48.b0() < f44.b0(),
            "a higher sample rate needs a smaller b0 for the same cutoff"
        );
        let d = db(f44.magnitude_at(3386.3, 44_100)) - db(f48.magnitude_at(3386.3, 48_000));
        assert!(
            d.abs() < 0.1,
            "the -3 dB point must sit at the same frequency regardless of rate (delta {d:.3} dB)"
        );
    }
}
