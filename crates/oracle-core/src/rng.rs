//! SplitMix64 — the core's single seeded RNG.
//!
//! Hand-rolled and dependency-free (no `rand`), so it can never drift across crate versions and
//! silently break `state_hash`. This is the *only* source of randomness in the core (it seeds
//! power-on RAM/VRAM). Deterministic by construction: the same seed always yields the same stream.

/// A deterministic [SplitMix64](https://prng.di.unimi.it/splitmix64.c) generator — one `u64` of state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Create a generator from a seed. Any seed value is valid.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Return the next 64-bit output and advance the state.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Published SplitMix64 test vector for seed 0 (Vigna's reference `splitmix64.c`).
    #[test]
    fn matches_published_vector_for_seed_zero() {
        let mut rng = SplitMix64::new(0);
        let got = [rng.next_u64(), rng.next_u64(), rng.next_u64()];
        assert_eq!(
            got,
            [
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F,
            ]
        );
    }

    #[test]
    fn is_deterministic_for_same_seed() {
        let mut a = SplitMix64::new(0xDEAD_BEEF_CAFE_F00D);
        let mut b = SplitMix64::new(0xDEAD_BEEF_CAFE_F00D);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
