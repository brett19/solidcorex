// Written for solidcorex (docs/01-architecture.md §8.1, docs/07-conventions.md §7). `Rng` is
// solidcore's `kernel::Rng` at commit 03e16c9ee766de5b9b1ced845e5149d62ff493b8, the same
// sequence for the same seed (checked by the difftest).
//! The one fixed hash, and the random-number generator built on it.
//!
//! Every hash whose value can reach a result is [`mix`] (`SplitMix64` mixing, Steele, Lea and
//! Flood, "Fast splittable pseudorandom number generators", OOPSLA 2014). [`FixedState`]
//! puts it under a `HashMap` or `HashSet` for lookups. `DefaultHasher` and `RandomState`
//! are for tests only.
//!
//! [`Rng`] is `SplitMix64`. An operation never shares a stream: it starts its own generator
//! as `Rng::new(mix(&[seed, op, entity]))`, so its draws do not depend on what ran before
//! or on how work was scheduled.
//!
//! The same bits come out on every target: only wrapping integer arithmetic is used.

use core::hash::{BuildHasher, Hasher};

/// `SplitMix64`'s increment (the golden ratio in 64 bits).
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// `SplitMix64`'s output function: a bijection of `u64` with full avalanche.
#[must_use]
pub const fn finalize(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The fixed hash of a sequence of words: starting from 0, each word is xored in, then
/// the state takes one `SplitMix64` step (`finalize(h ^ w + GAMMA)`). Order matters, and
/// `mix(&[s])` is the first draw of `Rng::new(s)`.
#[must_use]
pub fn mix(words: &[u64]) -> u64 {
    words
        .iter()
        .fold(0, |h, &w| finalize((h ^ w).wrapping_add(GAMMA)))
}

/// A [`Hasher`] over [`mix`]: each `u64` written is one word. Bytes are taken eight at a
/// time, little-endian, the last chunk zero-padded and followed by the byte count, so the
/// value is the same on every target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FixedHasher {
    state: u64,
}

impl Hasher for FixedHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut w = [0u8; 8];
            w.copy_from_slice(c);
            self.write_u64(u64::from_le_bytes(w));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut w = [0u8; 8];
            for (d, s) in w.iter_mut().zip(rest) {
                *d = *s;
            }
            self.write_u64(u64::from_le_bytes(w));
        }
        self.write_u64(bytes.len() as u64);
    }

    fn write_u64(&mut self, w: u64) {
        self.state = finalize((self.state ^ w).wrapping_add(GAMMA));
    }

    fn write_usize(&mut self, w: usize) {
        self.write_u64(w as u64);
    }

    fn write_u32(&mut self, w: u32) {
        self.write_u64(u64::from(w));
    }
}

/// The [`BuildHasher`] of [`FixedHasher`], for `HashMap::with_hasher(FixedState)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FixedState;

impl BuildHasher for FixedState {
    type Hasher = FixedHasher;
    fn build_hasher(&self) -> FixedHasher {
        FixedHasher::default()
    }
}

/// Deterministic pseudo-random generator: `SplitMix64`. Tiny, no dependencies, passes
/// `BigCrush`, the same on every target, and `Send`. Not cryptographic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// The default seed (solidcore's `Rng::DEFAULT_SEED`).
    pub const DEFAULT_SEED: u64 = 0x5EED_5EED_5EED_5EED;

    /// A generator from `seed`. Equal seeds give equal sequences.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        finalize(self.state)
    }

    /// A uniform `f64` in `[0, 1)`, from the top 53 bits.
    #[allow(clippy::cast_precision_loss)] // 53 bits: exact
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// A uniform `f64` in `[-1, 1)`: `2 * next_f64() - 1`.
    pub fn next_signed_unit(&mut self) -> f64 {
        2.0 * self.next_f64() - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn rng_matches_the_reference_sequence() {
        // SplitMix64 from seed 0 (the reference implementation's first three outputs).
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn mix_of_one_word_is_the_first_draw() {
        for s in [0, 1, Rng::DEFAULT_SEED, u64::MAX] {
            assert_eq!(mix(&[s]), Rng::new(s).next_u64());
        }
        assert_eq!(mix(&[]), 0);
        assert_ne!(mix(&[1, 2]), mix(&[2, 1]));
    }

    #[test]
    fn unit_draws_are_in_range() {
        let mut r = Rng::new(Rng::DEFAULT_SEED);
        for _ in 0..1000 {
            let u = r.next_f64();
            assert!((0.0..1.0).contains(&u));
            let s = r.next_signed_unit();
            assert!((-1.0..1.0).contains(&s));
        }
    }

    #[test]
    fn hasher_words_are_mix() {
        let mut h = FixedState.build_hasher();
        h.write_u64(7);
        h.write_u64(9);
        assert_eq!(h.finish(), mix(&[7, 9]));
        let mut b = FixedState.build_hasher();
        b.write(&[1, 2, 3]);
        assert_eq!(b.finish(), mix(&[0x03_02_01, 3]));
        let mut m: HashMap<u64, u32, FixedState> = HashMap::with_hasher(FixedState);
        m.insert(5, 1);
        assert_eq!(m.get(&5), Some(&1));
    }
}
