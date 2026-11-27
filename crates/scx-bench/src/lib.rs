// Written for solidcorex. Apache-2.0.
//! The benchmarks of scx-num and scx-algo against the pinned solidcore
//! (docs/02-testing.md §8.1). The cases live here, one module per library crate
//! ([`num`], [`algo`]), and two bench targets per crate run them: `<crate>` times each with
//! criterion, `<crate>_ic` counts its instructions under Callgrind (gungraun). Both are thin:
//! the rules are kept here.
//!
//! - [`Rng`] and [`seeded`]: the one seeded input set both sides of a case read, made from
//!   plain numbers and converted to each side's types before any timing or counting;
//! - [`edges`]: an edge set (NaN, infinities, degenerate inputs) cycled to [`BATCH`]
//!   entries in a seeded order, for a case's separate edge group;
//! - [`Consume`]: every scalar of a result, folded into a [`Checksum`], so no part of a
//!   result can go unread (its tests check that each bit of each scalar reaches the sum);
//! - [`Case`]: one batch of each side, every input through `black_box` and every result
//!   consumed;
//! - [`pin`]: the pin's `geom/krawczyk.rs`, made public by the build script.

use core::convert::Infallible;
use core::fmt;

pub mod case;
// The cases' setup and evaluators index fixed-size arrays.
#[allow(clippy::indexing_slicing)]
pub mod algo;
#[allow(clippy::indexing_slicing)]
pub mod num;

pub use case::{Case, Side};

/// The inputs a timed iteration runs over: throughput is reported per element.
pub const BATCH: usize = 1024;

/// `SplitMix64` (Steele, Lea and Flood 2014): the same stream on every platform.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// The stream of `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`: the top 53 bits.
    #[allow(clippy::cast_precision_loss)] // 53 bits are exact in an f64
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// `N` values uniform in `[lo, hi)`.
    pub fn array<const N: usize>(&mut self, lo: f64, hi: f64) -> [f64; N] {
        core::array::from_fn(|_| self.range(lo, hi))
    }

    /// Uniform in `0..n` (`0` when `n` is `0`), by the multiply-high reduction.
    pub fn below(&mut self, n: usize) -> usize {
        let wide = u128::from(self.next_u64()) * n as u128;
        usize::try_from(wide >> 64).unwrap_or(0)
    }
}

/// [`BATCH`] inputs from the stream of `seed`.
pub fn seeded<T>(seed: u64, mut make: impl FnMut(&mut Rng) -> T) -> Vec<T> {
    let mut rng = Rng::new(seed);
    (0..BATCH).map(|_| make(&mut rng)).collect()
}

/// The edge inputs `set`, cycled to [`BATCH`] entries and shuffled by the stream of
/// `seed` (Fisher and Yates), so no branch predictor learns the cycle.
pub fn edges<T: Clone>(seed: u64, set: &[T]) -> Vec<T> {
    let mut out: Vec<T> = set.iter().cycle().take(BATCH).cloned().collect();
    let mut rng = Rng::new(seed);
    for i in (1..out.len()).rev() {
        out.swap(i, rng.below(i + 1));
    }
    out
}

/// A running checksum of a result's scalars: each one's bits rotate in, so a scalar
/// left unread (or computed wrong) changes the sum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Checksum(pub u64);

impl Checksum {
    /// Fold in 64 bits.
    pub fn bits(&mut self, x: u64) {
        self.0 = self.0.rotate_left(7) ^ x;
    }

    /// Fold in an `f64`'s bits.
    pub fn f64(&mut self, x: f64) {
        self.bits(x.to_bits());
    }
}

/// A result read whole: every scalar it holds goes into the checksum. Implemented for
/// the scx-num and scx-algo results the benchmarks time, and for the pin's.
pub trait Consume {
    /// Fold every scalar of `self` into `sum`.
    fn consume(&self, sum: &mut Checksum);
}

impl Consume for f64 {
    fn consume(&self, sum: &mut Checksum) {
        sum.f64(*self);
    }
}

impl Consume for bool {
    fn consume(&self, sum: &mut Checksum) {
        sum.bits(u64::from(*self));
    }
}

impl Consume for u64 {
    fn consume(&self, sum: &mut Checksum) {
        sum.bits(*self);
    }
}

impl Consume for usize {
    fn consume(&self, sum: &mut Checksum) {
        sum.bits(*self as u64);
    }
}

impl<T: Consume> Consume for [T] {
    fn consume(&self, sum: &mut Checksum) {
        sum.bits(self.len() as u64);
        for x in self {
            x.consume(sum);
        }
    }
}

impl<T: Consume, const N: usize> Consume for [T; N] {
    fn consume(&self, sum: &mut Checksum) {
        for x in self {
            x.consume(sum);
        }
    }
}

impl<T: Consume> Consume for Vec<T> {
    fn consume(&self, sum: &mut Checksum) {
        self.as_slice().consume(sum);
    }
}

impl<T: Consume> Consume for Option<T> {
    fn consume(&self, sum: &mut Checksum) {
        sum.bits(u64::from(self.is_some()));
        if let Some(x) = self {
            x.consume(sum);
        }
    }
}

/// The success, or that it failed: an error's content is not timed work.
impl<T: Consume, E> Consume for Result<T, E> {
    fn consume(&self, sum: &mut Checksum) {
        sum.bits(u64::from(self.is_ok()));
        if let Ok(x) = self {
            x.consume(sum);
        }
    }
}

impl<A: Consume, B: Consume> Consume for (A, B) {
    fn consume(&self, sum: &mut Checksum) {
        self.0.consume(sum);
        self.1.consume(sum);
    }
}

mod scx_impls {
    use super::{Checksum, Consume};
    use scx_algo::local_solve_nd::{NdReport, Outcome};
    use scx_algo::polynomial::Roots;
    use scx_num::extent1d::Extent1d;
    use scx_num::extent3d::Extent3d;
    use scx_num::interval::Ivl;
    use scx_num::math::Affine;
    use scx_num::vector3d::{Point3, UnitVec3, Vec3};

    impl Consume for Vec3 {
        fn consume(&self, sum: &mut Checksum) {
            [self.x, self.y, self.z].consume(sum);
        }
    }

    impl Consume for Point3 {
        fn consume(&self, sum: &mut Checksum) {
            [self.x, self.y, self.z].consume(sum);
        }
    }

    impl Consume for UnitVec3 {
        fn consume(&self, sum: &mut Checksum) {
            self.as_vec().consume(sum);
        }
    }

    impl Consume for Extent1d {
        fn consume(&self, sum: &mut Checksum) {
            [self.min(), self.max()].consume(sum);
        }
    }

    impl Consume for Extent3d {
        fn consume(&self, sum: &mut Checksum) {
            self.min().consume(sum);
            self.max().consume(sum);
        }
    }

    impl Consume for Affine {
        fn consume(&self, sum: &mut Checksum) {
            self.x_axis().consume(sum);
            self.y_axis().consume(sum);
            self.z_axis().consume(sum);
            self.translation_part().consume(sum);
        }
    }

    impl Consume for Ivl {
        fn consume(&self, sum: &mut Checksum) {
            [self.lo, self.hi].consume(sum);
        }
    }

    impl Consume for Roots {
        fn consume(&self, sum: &mut Checksum) {
            self.as_slice().consume(sum);
        }
    }

    impl<T: Consume> Consume for Outcome<T> {
        fn consume(&self, sum: &mut Checksum) {
            self.is_root().consume(sum);
            self.value().consume(sum);
        }
    }

    impl<const N: usize> Consume for NdReport<N> {
        fn consume(&self, sum: &mut Checksum) {
            self.outcome.consume(sum);
            self.iterations.consume(sum);
        }
    }
}

mod pin_impls {
    use super::{Checksum, Consume};
    use solidcore::blend::fillet_section::interval::Ivl;
    use solidcore::geom::extent1d::Extent1d;
    use solidcore::geom::extent3d::Extent3d;
    use solidcore::geom::local_solve1d::SolveOutcome;
    use solidcore::geom::local_solve_nd::NdSolution;
    use solidcore::geom::polynomial::Roots;
    use solidcore::geom::vector3d::Vec3;
    use solidcore::math::Transform;

    impl Consume for Vec3 {
        fn consume(&self, sum: &mut Checksum) {
            [self.x, self.y, self.z].consume(sum);
        }
    }

    impl Consume for Extent1d {
        fn consume(&self, sum: &mut Checksum) {
            [self.min(), self.max()].consume(sum);
        }
    }

    impl Consume for Extent3d {
        fn consume(&self, sum: &mut Checksum) {
            self.min().consume(sum);
            self.max().consume(sum);
        }
    }

    impl Consume for Transform {
        fn consume(&self, sum: &mut Checksum) {
            self.m.consume(sum);
        }
    }

    impl Consume for Ivl {
        fn consume(&self, sum: &mut Checksum) {
            [self.lo, self.hi].consume(sum);
        }
    }

    impl Consume for Roots {
        fn consume(&self, sum: &mut Checksum) {
            (**self).consume(sum);
        }
    }

    impl Consume for SolveOutcome {
        fn consume(&self, sum: &mut Checksum) {
            (self.found, self.t).consume(sum);
        }
    }

    impl Consume for NdSolution {
        fn consume(&self, sum: &mut Checksum) {
            self.found.consume(sum);
            self.x.consume(sum);
        }
    }
}

/// The evaluators' error for the scx solvers: there is none, as the benchmark functions
/// cannot fail.
#[derive(Debug, Clone, Copy)]
pub struct NoError(Infallible);

impl fmt::Display for NoError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {}
    }
}

impl scx_algo::error::EvalError for NoError {
    fn ends_solve(&self) -> bool {
        match self.0 {}
    }

    fn passes_through(&self) -> bool {
        match self.0 {}
    }

    fn never_ignored(&self) -> bool {
        match self.0 {}
    }
}

/// The pin's `geom/krawczyk.rs` with its items made public (they are `pub(crate)` at the
/// pin), from the build script; the names it imports are the pin's own.
#[allow(
    clippy::all,
    clippy::pedantic,
    dead_code,
    unused_imports,
    missing_docs,
    unreachable_pub
)]
pub mod pin {
    pub use solidcore::blend::fillet_section::certificate::inverse4;
    use solidcore::blend::fillet_section::interval::{round_up, Ivl};
    fn cmax(a: f64, b: f64) -> f64 {
        if a < b {
            b
        } else {
            a
        }
    }
    pub use solidcore::blend::fillet_section::station::solve4;
    use solidcore::blend::fillet_section::tay::Ring;
    use solidcore::blend::fillet_section::taylor_model::TmN;
    use solidcore::error::Result;
    use solidcore::geom::vector3d::Vec3;
    include!(concat!(env!("OUT_DIR"), "/pin_krawczyk.rs"));
}

#[cfg(test)]
mod tests {
    use super::{edges, seeded, Checksum, Consume, Rng, BATCH};
    use scx_num::extent3d::Extent3d;
    use scx_num::vector3d::Point3;

    #[test]
    fn stream_is_fixed() {
        // SplitMix64's published first outputs for seed 0
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(r.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        let v = seeded(7, Rng::unit);
        assert_eq!(v.len(), BATCH);
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
    }

    #[test]
    fn edges_cycle_every_entry() {
        let e = edges(3, &[0u32, 1, 2]);
        assert_eq!(e.len(), BATCH);
        for k in 0..3u32 {
            assert!(e.iter().filter(|&&x| x == k).count() >= BATCH / 3);
        }
        assert_ne!(&e[..3], &[0, 1, 2]);
    }

    /// Every bit of every bound of a box reaches the sum: a fold that read only some
    /// bounds (the fault this crate's rules exist for) would not.
    #[test]
    fn checksum_reads_every_bound() {
        let b =
            Extent3d::from_corners(Point3::new(1.0, 2.0, 3.0), Point3::new(4.0, 5.0, 6.0)).unwrap();
        let sum = |e: &Extent3d| {
            let mut s = Checksum::default();
            e.consume(&mut s);
            s
        };
        let base = sum(&b);
        let [lo, hi] = [b.min(), b.max()];
        for i in 0..6 {
            for bit in [0, 31, 52, 63] {
                let mut c = [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z];
                c[i] = f64::from_bits(c[i].to_bits() ^ (1 << bit));
                let p = |a: &[f64]| Point3::new(a[0], a[1], a[2]);
                if let Some(e) = Extent3d::from_corners(p(&c[..3]), p(&c[3..])) {
                    assert_ne!(sum(&e), base, "bound {i} bit {bit}");
                }
            }
        }
    }

    /// Every case an instruction-count target names is one its crate's `cases()` makes,
    /// and every case is named there: the two targets measure the same set.
    #[test]
    fn instruction_count_targets_name_every_case() {
        for (file, cases) in [
            (include_str!("../benches/num_ic.rs"), super::num::cases()),
            (include_str!("../benches/algo_ic.rs"), super::algo::cases()),
        ] {
            let mut named: Vec<&str> = file
                .split("(case(\"")
                .skip(1)
                .filter_map(|s| s.split('"').next())
                .collect();
            named.sort_unstable();
            named.dedup();
            let mut made: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
            made.sort_unstable();
            assert_eq!(named, made);
            for c in &cases {
                assert_eq!(c.len, BATCH, "{}", c.name);
            }
        }
    }
}
