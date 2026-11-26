// Written for solidcorex. Apache-2.0.
//! `hash.rs`: `Rng` against the pin's `kernel::Rng`, and `mix` of one word against the
//! pin generator's first draw (the identity `mix` is defined to keep).

use crate::diff;

// The first draws of each kind from `seed`, `n` of them (at most 12).
diff!(
    rng_sequence,
    |seed: u64, n: usize| {
        let mut r = scx_num::hash::Rng::new(seed);
        let k = n.min(12);
        (
            (0..k).map(|_| r.next_u64()).collect::<Vec<_>>(),
            (0..k).map(|_| r.next_f64()).collect::<Vec<_>>(),
            (0..k).map(|_| r.next_signed_unit()).collect::<Vec<_>>(),
        )
    },
    {
        let mut r = solidcore::kernel::Rng::new(seed);
        let k = n.min(12);
        (
            (0..k).map(|_| r.next_u64()).collect::<Vec<_>>(),
            (0..k).map(|_| r.next_f64()).collect::<Vec<_>>(),
            (0..k).map(|_| r.next_signed_unit()).collect::<Vec<_>>(),
        )
    }
);
diff!(
    rng_default_seed,
    || scx_num::hash::Rng::DEFAULT_SEED,
    solidcore::kernel::Rng::DEFAULT_SEED
);
diff!(
    mix_one_word,
    |seed: u64| scx_num::hash::mix(&[seed]),
    solidcore::kernel::Rng::new(seed).next_u64()
);
