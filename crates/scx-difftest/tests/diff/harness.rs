// Written for solidcorex. Apache-2.0.
//! The differential harness: input pools, a seeded generator, a fingerprint of every
//! result, and the `diff!` macro that runs one expression against scx-num and against the
//! pinned solidcore.
//!
//! Two checks per input (owner's rule of 2026-11-26: correct and deterministic, not bit
//! for bit with solidcore):
//! - **Deterministic:** scx-num's result, computed twice, is the same to the bit.
//! - **Agrees with solidcore:** the same shape (variants, errors, integers, lengths,
//!   panics) and every float within [`Tol`]. Exact agreement is counted separately, so
//!   drift from the pin is visible even where it is allowed.
//!
//! Where the pin itself is wrong, a [`PinDefect`] (listed in `pin_defects.rs`) names the
//! inputs by a predicate: those are not compared with the pin, and scx's result must pass
//! the defect's own check against the true value instead.

use std::fmt::Debug;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Once;

use scx_num::certified::{next_down, next_up};

/// SplitMix64 (Steele, Lea and Flood 2014), written out so the inputs depend on nothing.
#[derive(Clone)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    /// Uniform in `[lo, hi)`.
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * u
    }
}

/// A seed from a test name (FNV-1a), so every test draws its own fixed inputs.
pub fn seed_of(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The edge-case scalars every pool starts from.
pub fn edge_f64() -> Vec<f64> {
    vec![
        0.0,
        -0.0,
        f64::from_bits(1), // smallest denormal
        -f64::from_bits(1),
        f64::from_bits(0x000F_FFFF_FFFF_FFFF), // largest denormal
        f64::MIN_POSITIVE,
        1.0e-300,
        1.0e-13,
        1.0e-12,
        -1.0e-12,
        1.2e-8,
        1.0e-6,
        1.0e-5,
        2.0e-5,
        0.5,
        1.0,
        -1.0,
        2.0,
        3.0,
        -2.5,
        core::f64::consts::PI,
        10.0,
        1000.0,
        1234567.0,
        -1234567.0,
        1.0e20,
        1.0e300,
        -1.0e300,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
    ]
}

/// The thresholds scx-num compares against: each tolerance and numerical zero, the
/// size-relative zeros `EFF_ZERO * (1 + |d|)` at sizes the pools use, the default zone and
/// its half and double. Standing rule (README, "Differential tests"): every threshold a
/// moved function compares with is listed here, so the pools hold it and one ulp either side.
pub fn thresholds() -> Vec<f64> {
    use scx_num::certified::UNDERFLOW_ROOT;
    use scx_num::consts::{EPS_PARAM, PARALLEL_ANGLE_RAD, ZONE_DEFAULT};
    use scx_num::math::{
        BIG_DOUBLE, EFF_ZERO, EFF_ZERO_DEG, EFF_ZERO_RAD, EFF_ZERO_SQ, EFF_ZERO_SQRT,
        INFINITE_PARAMETER, TWO_PI,
    };
    // (each one ulp either side too: `threshold_f64`)
    vec![
        EFF_ZERO,
        EFF_ZERO_SQ,
        EFF_ZERO_SQRT,
        EFF_ZERO_DEG,
        EFF_ZERO_RAD,
        EPS_PARAM,
        EFF_ZERO * (1.0 + 1.0),
        EFF_ZERO * (1.0 + TWO_PI),
        EFF_ZERO * (1.0 + 10.0),
        EFF_ZERO * (1.0 + 50.0),
        ZONE_DEFAULT,
        ZONE_DEFAULT / 2.0,
        2.0 * ZONE_DEFAULT,
        PARALLEL_ANGLE_RAD,
        1.0e-9, // the 2D helpers' length floor (`unit_2d`, `three_point_circle_2d`)
        // the pin's `SM_UNDEF_DOUBLE` sentinel (scx: none; an unset value is an `Option`)
        solidcore::math::UNDEF_DOUBLE,
        UNDERFLOW_ROOT,
        BIG_DOUBLE,
        INFINITE_PARAMETER,
        // the pin's `SM_USE_DEFAULT`, which its `*_tol3d` helpers replace (scx: `None`)
        solidcore::geom::tol::USE_DEFAULT,
    ]
}

/// Each threshold, and one ulp below and above it (the zone and `EFF_ZERO` with both signs).
pub fn threshold_f64() -> Vec<f64> {
    let mut v = Vec::new();
    for t in thresholds() {
        v.extend([next_down(t), t, next_up(t)]);
    }
    for t in [scx_num::math::EFF_ZERO, scx_num::consts::ZONE_DEFAULT] {
        v.extend([-next_down(t), -t, -next_up(t)]);
    }
    v
}

/// A type the harness can draw inputs of.
pub trait Gen: Sized + Clone + Debug {
    fn pool(rng: &mut SplitMix64) -> Vec<Self>;
}

impl Gen for f64 {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = edge_f64();
        v.extend(threshold_f64());
        for _ in 0..6 {
            v.push(rng.uniform(-10.0, 10.0));
        }
        v
    }
}

impl Gen for bool {
    fn pool(_: &mut SplitMix64) -> Vec<Self> {
        vec![false, true]
    }
}

/// Indices and sizes: small (also indexing the fixed case lists, of at most 13), plus one
/// past any 2D/3D/4D component, and large.
impl Gen for usize {
    fn pool(_: &mut SplitMix64) -> Vec<Self> {
        (0..=12).chain([64]).collect()
    }
}

/// Seeds: zero, one, all ones, the default seed and SplitMix64's increment (the state
/// wraps to zero after one step from its negation), plus random words.
impl Gen for u64 {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![
            0,
            1,
            u64::MAX,
            0x5EED_5EED_5EED_5EED,
            0x9E37_79B9_7F4A_7C15,
            0x9E37_79B9_7F4A_7C15_u64.wrapping_neg(),
        ];
        for _ in 0..6 {
            v.push(rng.next_u64());
        }
        v
    }
}

impl Gen for i32 {
    fn pool(_: &mut SplitMix64) -> Vec<Self> {
        vec![-1, 0, 1, 2, 3, 4, 5, 6, 100, i32::MIN, i32::MAX]
    }
}

impl<T: Gen> Gen for Option<T> {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![None];
        v.extend(T::pool(rng).into_iter().map(Some));
        v
    }
}

fn vec_pool<const N: usize>(rng: &mut SplitMix64, special: &[[f64; N]]) -> Vec<[f64; N]> {
    let e = edge_f64();
    let mut v: Vec<[f64; N]> = e.iter().map(|&x| [x; N]).collect();
    v.extend_from_slice(special);
    for t in [
        scx_num::math::EFF_ZERO,
        1.0e-9,
        scx_num::consts::ZONE_DEFAULT,
        scx_num::math::EFF_ZERO_SQRT,
    ] {
        for x in [next_down(t), t, next_up(t)] {
            let mut a = [0.0; N];
            a[0] = x;
            v.push(a);
        }
    }
    for _ in 0..40 {
        let mut a = [0.0; N];
        for c in &mut a {
            *c = e[rng.below(e.len())];
        }
        v.push(a);
    }
    for _ in 0..30 {
        let mut a = [0.0; N];
        for c in &mut a {
            *c = rng.uniform(-10.0, 10.0);
        }
        v.push(a);
    }
    v
}

impl Gen for [f64; 2] {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        vec_pool(
            rng,
            &[
                [1.0, 0.0],
                [0.0, 1.0],
                [-1.0, 0.0],
                [0.0, -0.0],
                [3.0, 4.0],
                [0.6, 0.8],
                [1.0, 1.0e-13],
                [1.0e-13, 1.0],
                [1.0, 1.0e-9],
            ],
        )
    }
}

impl Gen for [f64; 3] {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        vec_pool(
            rng,
            &[
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [-1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0],
                [-0.0, 0.0, -0.0],
                [3.0, 4.0, 0.0],
                [0.6, 0.8, 0.0],
                [1.0, 2.0, 3.0],
                [1.0, 1.0e-13, 0.0],
                [1.0, 0.0, 1.0e-9],
                [1.0, 1.0, 1.0],
                [1.0e-7, 0.0, 0.0],
                // the empty periodic extent (`min = BIG_DOUBLE`, `max = -BIG_DOUBLE`)
                [1.0e20, -1.0e20, core::f64::consts::TAU],
                [1.0e20, -1.0e20, 1.0],
            ],
        )
    }
}

impl Gen for [f64; 4] {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        vec_pool(
            rng,
            &[
                [0.0, 0.0, 1.0, 1.0],
                [1.0, 1.0, 0.0, 0.0],
                [-1.0, -2.0, 3.0, 4.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 1.0, 1.0],
            ],
        )
    }
}

impl Gen for [f64; 6] {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        vec_pool(
            rng,
            &[
                [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 0.0, 0.0, 0.0],
                [-1.0, -2.0, -3.0, 4.0, 5.0, 6.0],
                [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0, 0.0, 1.0],
                [-1.0e20, -1.0e20, -1.0e20, 1.0e20, 1.0e20, 1.0e20],
            ],
        )
    }
}

impl Gen for [[f64; 4]; 4] {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let id = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut v = vec![id, [[0.0; 4]; 4], [[1.0; 4]; 4]];
        let mut t = id;
        t[3] = [1.0, 2.0, 3.0, 1.0];
        v.push(t);
        let mut r = id;
        r[0] = [0.0, 1.0, 0.0, 0.0];
        r[1] = [-1.0, 0.0, 0.0, 0.0];
        v.push(r);
        let mut s = id;
        s[0][0] = 2.0;
        s[1][1] = 3.0;
        s[2][2] = -1.0;
        v.push(s);
        let mut flat = id;
        flat[2] = [0.0, 0.0, 0.0, 0.0];
        v.push(flat);
        let mut tiny = id;
        tiny[0][0] = 1.0e-13;
        v.push(tiny);
        // an affine map with a zero linear part, one with a subnormal-scaled linear part,
        // and the identity scaled to EFF_ZERO and one ulp either side
        let mut zero_linear = [[0.0; 4]; 4];
        zero_linear[3][3] = 1.0;
        v.push(zero_linear);
        for s in [
            1.0e-310,
            f64::from_bits(1),
            next_down(scx_num::math::EFF_ZERO),
            scx_num::math::EFF_ZERO,
            next_up(scx_num::math::EFF_ZERO),
        ] {
            let mut m = id;
            for (i, row) in m.iter_mut().enumerate().take(3) {
                row[i] = s;
            }
            v.push(m);
        }
        let e = edge_f64();
        for &x in &e {
            let mut m = id;
            m[1][2] = x;
            m[3][0] = x;
            v.push(m);
        }
        for _ in 0..20 {
            let mut m = id;
            for row in m.iter_mut().take(3) {
                for c in row.iter_mut().take(3) {
                    *c = rng.uniform(-3.0, 3.0);
                }
            }
            for c in m[3].iter_mut().take(3) {
                *c = rng.uniform(-10.0, 10.0);
            }
            v.push(m);
        }
        for _ in 0..10 {
            let mut m = [[0.0; 4]; 4];
            for row in &mut m {
                for c in row.iter_mut() {
                    *c = rng.uniform(-3.0, 3.0);
                }
            }
            v.push(m);
        }
        v
    }
}

/// A dense matrix recipe: `(rows, cols, row-major entries)`.
#[derive(Clone, Debug)]
pub struct MatR(pub usize, pub usize, pub Vec<f64>);

impl Gen for MatR {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![
            MatR(0, 0, vec![]),
            MatR(1, 1, vec![0.0]),
            MatR(1, 1, vec![2.0]),
        ];
        for n in 1..=4 {
            let mut id = vec![0.0; n * n];
            for i in 0..n {
                id[i * n + i] = 1.0;
            }
            v.push(MatR(n, n, id));
            v.push(MatR(n, n, vec![1.0; n * n]));
        }
        v.push(MatR(
            3,
            3,
            vec![2.0, 1.0, 0.0, 1.0, 3.0, 1.0, 0.0, 1.0, 4.0],
        ));
        v.push(MatR(
            3,
            3,
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0],
        ));
        v.push(MatR(
            3,
            3,
            vec![1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 0.0, 1.0],
        ));
        v.push(MatR(2, 3, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        v.push(MatR(3, 2, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        v.push(MatR(4, 2, vec![1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, -1.0]));
        let e = edge_f64();
        for &x in &e {
            v.push(MatR(2, 2, vec![x, 1.0, 1.0, x]));
            v.push(MatR(3, 3, vec![1.0, x, 0.0, x, 2.0, 0.0, 0.0, 0.0, 3.0]));
        }
        for _ in 0..30 {
            let r = 1 + rng.below(4);
            let c = 1 + rng.below(4);
            let d = (0..r * c).map(|_| rng.uniform(-5.0, 5.0)).collect();
            v.push(MatR(r, c, d));
        }
        for _ in 0..20 {
            let n = 1 + rng.below(4);
            let mut d = vec![0.0; n * n];
            for i in 0..n {
                for j in i..n {
                    let x = rng.uniform(-5.0, 5.0);
                    d[i * n + j] = x;
                    d[j * n + i] = x;
                }
            }
            v.push(MatR(n, n, d));
        }
        v
    }
}

/// A short list of scalars (point sets, right-hand sides, values to clamp).
#[derive(Clone, Debug)]
pub struct List<T>(pub Vec<T>);

impl<T: Gen> Gen for List<T> {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let p = T::pool(rng);
        let mut v = vec![List(vec![])];
        for x in p.iter().take(12) {
            v.push(List(vec![x.clone()]));
        }
        for _ in 0..40 {
            let n = 1 + rng.below(6);
            v.push(List(
                (0..n).map(|_| p[rng.below(p.len())].clone()).collect(),
            ));
        }
        v
    }
}

/// Argument tuples drawn from the pools: every pair for two arguments (when that is
/// at most 10000 cases), else the diagonal and 3000 seeded draws.
pub trait Args: Sized + Clone + Debug {
    fn cases(rng: &mut SplitMix64) -> Vec<Self>;
}

impl Args for () {
    fn cases(_: &mut SplitMix64) -> Vec<Self> {
        vec![()]
    }
}

impl<A: Gen> Args for (A,) {
    fn cases(rng: &mut SplitMix64) -> Vec<Self> {
        A::pool(rng).into_iter().map(|a| (a,)).collect()
    }
}

impl<A: Gen, B: Gen> Args for (A, B) {
    fn cases(rng: &mut SplitMix64) -> Vec<Self> {
        let pa = A::pool(rng);
        let pb = B::pool(rng);
        if pa.len() * pb.len() <= 10000 {
            let mut v = Vec::new();
            for a in &pa {
                for b in &pb {
                    v.push((a.clone(), b.clone()));
                }
            }
            v
        } else {
            let n = pa.len().max(pb.len());
            let mut v: Vec<_> = (0..n)
                .map(|i| (pa[i % pa.len()].clone(), pb[i % pb.len()].clone()))
                .collect();
            for _ in 0..3000 {
                v.push((
                    pa[rng.below(pa.len())].clone(),
                    pb[rng.below(pb.len())].clone(),
                ));
            }
            v
        }
    }
}

macro_rules! args_tuple {
    ($($t:ident $p:ident),+) => {
        impl<$($t: Gen),+> Args for ($($t,)+) {
            fn cases(rng: &mut SplitMix64) -> Vec<Self> {
                $(let $p = $t::pool(rng);)+
                let n = [$($p.len()),+].into_iter().max().unwrap_or(0);
                let mut v: Vec<Self> = (0..n).map(|i| ($($p[i % $p.len()].clone(),)+)).collect();
                for _ in 0..3000 {
                    v.push(($($p[rng.below($p.len())].clone(),)+));
                }
                v
            }
        }
    };
}
args_tuple!(A a, B b, C c);
args_tuple!(A a, B b, C c, D d);
args_tuple!(A a, B b, C c, D d, E e);
args_tuple!(A a, B b, C c, D d, E e, F f);
args_tuple!(A a, B b, C c, D d, E e, F f, G g);

/// One scalar of a fingerprint: a float (by its bits) or an integer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Item {
    F(u64),
    I(u64),
}

impl Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::F(b) => write!(f, "{:e}", f64::from_bits(*b)),
            Item::I(i) => write!(f, "{i}"),
        }
    }
}

/// The fingerprint of a result: every float and integer in it, in order, plus text for
/// the parts that are not numbers (enum variants, error classes and operations).
/// `==` is bit-exact.
#[derive(Default, PartialEq, Eq, Debug)]
pub struct Out {
    pub bits: Vec<Item>,
    pub text: String,
}

/// How far a float may move from the pinned solidcore's. Two floats agree when they are
/// equal as numbers (so `0.0` and `-0.0` agree), are both NaN (any payload), or are both
/// finite and differ by at most `max(abs, rel * max(|a|, |b|))`. Infinities agree only
/// with the same infinity.
#[derive(Clone, Copy, Debug)]
pub struct Tol {
    pub rel: f64,
    pub abs: f64,
}

impl Tol {
    /// The default: a few rounding errors of drift, no more.
    pub const DEFAULT: Tol = Tol {
        rel: 1e-12,
        abs: 1e-300,
    };

    pub fn floats_agree(self, a: f64, b: f64) -> bool {
        if a == b || (a.is_nan() && b.is_nan()) {
            return true;
        }
        if !a.is_finite() || !b.is_finite() {
            return false;
        }
        (a - b).abs() <= self.abs.max(self.rel * a.abs().max(b.abs()))
    }

    /// Same shape and every float within the tolerance.
    pub fn agree(self, x: &Out, s: &Out) -> bool {
        x.text == s.text
            && x.bits.len() == s.bits.len()
            && x.bits.iter().zip(&s.bits).all(|p| match p {
                (Item::F(a), Item::F(b)) => {
                    self.floats_agree(f64::from_bits(*a), f64::from_bits(*b))
                }
                (Item::I(a), Item::I(b)) => a == b,
                _ => false,
            })
    }
}

impl Out {
    pub fn tag(&mut self, s: &str) {
        self.text.push_str(s);
        self.text.push('|');
    }
}

pub trait Bits {
    fn bits(&self, o: &mut Out);
}

impl Bits for f64 {
    fn bits(&self, o: &mut Out) {
        o.bits.push(Item::F(self.to_bits()));
    }
}
macro_rules! int_bits {
    ($($t:ty),*) => {$(
        impl Bits for $t {
            fn bits(&self, o: &mut Out) {
                o.bits.push(Item::I(*self as u64));
            }
        }
    )*};
}
int_bits!(bool, usize, u32, u64, i32, i64);
impl Bits for () {
    fn bits(&self, _: &mut Out) {}
}
impl Bits for &str {
    fn bits(&self, o: &mut Out) {
        o.tag(self);
    }
}
impl<T: Bits + ?Sized> Bits for &T {
    fn bits(&self, o: &mut Out) {
        (**self).bits(o);
    }
}
impl<T: Bits + ?Sized> Bits for &mut T {
    fn bits(&self, o: &mut Out) {
        (**self).bits(o);
    }
}
impl<T: Bits> Bits for Option<T> {
    fn bits(&self, o: &mut Out) {
        match self {
            None => o.tag("None"),
            Some(x) => {
                o.tag("Some");
                x.bits(o);
            }
        }
    }
}
impl<T: Bits, E: Bits> Bits for Result<T, E> {
    fn bits(&self, o: &mut Out) {
        match self {
            Ok(x) => {
                o.tag("Ok");
                x.bits(o);
            }
            Err(e) => {
                o.tag("Err");
                e.bits(o);
            }
        }
    }
}
impl<T: Bits> Bits for [T] {
    fn bits(&self, o: &mut Out) {
        o.bits.push(Item::I(self.len() as u64));
        for x in self {
            x.bits(o);
        }
    }
}
impl<T: Bits> Bits for Vec<T> {
    fn bits(&self, o: &mut Out) {
        self.as_slice().bits(o);
    }
}
impl<T: Bits, const N: usize> Bits for [T; N] {
    fn bits(&self, o: &mut Out) {
        self.as_slice().bits(o);
    }
}
macro_rules! tuple_bits {
    ($($t:ident $i:tt),+) => {
        impl<$($t: Bits),+> Bits for ($($t,)+) {
            fn bits(&self, o: &mut Out) {
                $(self.$i.bits(o);)+
            }
        }
    };
}
tuple_bits!(A 0);
tuple_bits!(A 0, B 1);
tuple_bits!(A 0, B 1, C 2);
tuple_bits!(A 0, B 1, C 2, D 3);
tuple_bits!(A 0, B 1, C 2, D 3, E 4);
tuple_bits!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple_bits!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);

/// Fingerprint the value, or the fact that it panicked.
pub fn shot<R: Bits>(f: impl FnOnce() -> R) -> Out {
    let mut o = Out::default();
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r.bits(&mut o),
        Err(_) => o.tag("PANIC"),
    }
    o
}

static QUIET: Once = Once::new();

/// A known defect of the pin on some inputs of one test. `applies` picks the inputs (a
/// predicate, never a list); on those the pin is not compared, and scx's result must pass
/// `check` against the true value instead. A defect that applies to no input fails the
/// test, so a stale one cannot stay.
pub struct PinDefect<A, R> {
    pub reason: &'static str,
    pub applies: Applies<A>,
    /// `Err` with what is wrong when scx's result is not the true value.
    pub check: fn(&A, &R) -> Result<(), String>,
}

/// A known defect of the pin judged by an oracle: on the inputs `applies` picks, the pin is
/// not compared, and scx's result must agree (at the test's tolerance) with `truth`, the
/// true result fingerprinted as the test's body fingerprints its own (see [`want`]). For a
/// result whose truth is easier to compute than to test piecewise.
pub struct TruthDefect<A> {
    pub reason: &'static str,
    pub applies: Applies<A>,
    pub truth: fn(&A) -> Out,
}

/// The fingerprint of a true value, for [`TruthDefect::truth`].
pub fn want<R: Bits>(r: R) -> Out {
    let mut o = Out::default();
    r.bits(&mut o);
    o
}

/// Which inputs a [`PinDefect`] or [`TruthDefect`] covers.
pub enum Applies<A> {
    /// The inputs this predicate picks.
    Inputs(fn(&A) -> bool),
    /// The inputs on which scx and the pin disagree past the tolerance: where scx moved to a
    /// different correct answer (`04-decisions`: results are validated against the truth,
    /// not the pin), and the check holds it to that truth.
    Disagreeing,
}

/// Run `xf` and `sf` on every case, at the default tolerance.
pub fn run<A: Args, RX: Bits, RS: Bits>(name: &str, xf: impl Fn(A) -> RX, sf: impl Fn(A) -> RS) {
    run_tol(name, Tol::DEFAULT, xf, sf);
}

/// [`run`], with a known defect of the pin.
pub fn run_defect<A: Args, RX: Bits, RS: Bits>(
    name: &str,
    defect: &PinDefect<A, RX>,
    xf: impl Fn(A) -> RX,
    sf: impl Fn(A) -> RS,
) {
    let check = |c: &A, r: &RX, _: &Out| (defect.check)(c, r);
    run_with(
        name,
        Tol::DEFAULT,
        Some((defect.reason, &defect.applies, &check)),
        xf,
        sf,
    );
}

/// [`run`], with a known defect of the pin judged by an oracle.
pub fn run_truth<A: Args, RX: Bits, RS: Bits>(
    name: &str,
    defect: &TruthDefect<A>,
    xf: impl Fn(A) -> RX,
    sf: impl Fn(A) -> RS,
) {
    let check = |c: &A, _: &RX, ox: &Out| {
        let w = (defect.truth)(c);
        if Tol::DEFAULT.agree(ox, &w) {
            Ok(())
        } else {
            Err(format!("the truth is {w:?}"))
        }
    };
    run_with(
        name,
        Tol::DEFAULT,
        Some((defect.reason, &defect.applies, &check)),
        xf,
        sf,
    );
}

/// Run `xf` twice and `sf` once on every case; fail if `xf` is not deterministic or does
/// not agree with `sf` within `tol`.
pub fn run_tol<A: Args, RX: Bits, RS: Bits>(
    name: &str,
    tol: Tol,
    xf: impl Fn(A) -> RX,
    sf: impl Fn(A) -> RS,
) {
    run_with(name, tol, None, xf, sf);
}

/// A defect's reason, the inputs it covers, and its check of scx's result and fingerprint.
type Defect<'a, A, RX> = (
    &'static str,
    &'a Applies<A>,
    &'a dyn Fn(&A, &RX, &Out) -> Result<(), String>,
);

fn run_with<A: Args, RX: Bits, RS: Bits>(
    name: &str,
    tol: Tol,
    defect: Option<Defect<'_, A, RX>>,
    xf: impl Fn(A) -> RX,
    sf: impl Fn(A) -> RS,
) {
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_| {})));
    let mut rng = SplitMix64(seed_of(name));
    let cases = A::cases(&mut rng);
    let (mut exact, mut unstable, mut bad) = (0usize, Vec::new(), Vec::new());
    let (mut defects, mut applied) = (0usize, 0usize);
    let mut drift: Option<String> = None;
    for c in &cases {
        let ox = shot(|| xf(c.clone()));
        let ox2 = shot(|| xf(c.clone()));
        let os = shot(|| sf(c.clone()));
        if ox != ox2 {
            unstable.push(format!("{c:?}\n    run 1: {ox:?}\n    run 2: {ox2:?}"));
        }
        let covered = |d: &Defect<'_, A, RX>| match d.1 {
            Applies::Inputs(f) => f(c),
            Applies::Disagreeing => ox != os && !tol.agree(&ox, &os),
        };
        if let Some((reason, _, check)) = defect.filter(covered) {
            applied += 1;
            let checked = catch_unwind(AssertUnwindSafe(|| check(c, &xf(c.clone()), &ox)))
                .unwrap_or_else(|_| Err("scx panicked".to_owned()));
            match checked {
                Ok(()) => defects += 1,
                Err(why) => bad.push(format!(
                    "{c:?}\n    pin defect ({reason}), check failed: {why}\n    scx: {ox:?}"
                )),
            }
        } else if ox == os {
            exact += 1;
        } else if tol.agree(&ox, &os) {
            drift.get_or_insert_with(|| {
                format!("{c:?}\n    scx:       {ox:?}\n    solidcore: {os:?}")
            });
        } else {
            bad.push(format!(
                "{c:?}\n    scx:       {ox:?}\n    solidcore: {os:?}"
            ));
        }
    }
    println!(
        "difftest {name}: {} inputs, {exact} exact, {} within tolerance, {defects} pin defects, {} mismatches, {} nondeterministic",
        cases.len(),
        cases.len() - exact - defects - bad.len(),
        bad.len(),
        unstable.len()
    );
    if let Some(d) = &drift {
        println!("difftest {name}: first input within tolerance only:\n{d}");
    }
    assert!(
        unstable.is_empty(),
        "{name}: {} of {} inputs are not deterministic; first:\n{}",
        unstable.len(),
        cases.len(),
        unstable
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    if let Some((reason, ..)) = defect {
        assert!(
            applied > 0,
            "{name}: the pin defect ({reason}) applies to no input; remove it"
        );
    }
    // The quiet panic hook swallows the assertion's text: print the first ones first.
    for b in bad.iter().take(3) {
        println!("difftest {name}: differs:\n{b}");
    }
    assert!(
        bad.is_empty(),
        "{name}: {} of {} inputs differ; first:\n{}",
        bad.len(),
        cases.len(),
        bad.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// `diff!(name, |a: T, ...| body)` runs `body` with `m` naming scx-num's modules and again
/// with `m` naming solidcore's, on the same inputs; `diff!(name, |...| xbody, sbody)`
/// takes the two bodies separately (for the signatures that changed);
/// `diff!(name, pin_defect = f, |...| body)` (or `xbody, sbody`) excuses the inputs of the
/// [`PinDefect`] `f()`; `truth_defect = f` likewise for a [`TruthDefect`].
#[macro_export]
macro_rules! diff {
    ($name:ident, truth_defect = $d:path, |$($a:ident : $t:ty),*| $body:expr) => {
        $crate::diff!($name, truth_defect = $d, |$($a: $t),*| $body, $body);
    };
    ($name:ident, truth_defect = $d:path, |$($a:ident : $t:ty),*| $xb:expr, $sb:expr) => {
        #[test]
        #[allow(unused_variables, unused_mut, clippy::redundant_closure_call)]
        fn $name() {
            $crate::harness::run_truth::<($($t,)*), _, _>(
                concat!(module_path!(), "::", stringify!($name)),
                &$d(),
                |($($a,)*): ($($t,)*)| {
                    #[allow(unused_imports)]
                    use $crate::facade::x as m;
                    #[allow(unused_imports)]
                    use m::ext::*;
                    $xb
                },
                |($($a,)*): ($($t,)*)| {
                    #[allow(unused_imports)]
                    use $crate::facade::s as m;
                    #[allow(unused_imports)]
                    use m::ext::*;
                    $sb
                },
            );
        }
    };
    ($name:ident, || $body:expr) => {
        $crate::diff!($name, | | $body, $body);
    };
    ($name:ident, || $xb:expr, $sb:expr) => {
        $crate::diff!($name, | | $xb, $sb);
    };
    ($name:ident, |$($a:ident : $t:ty),*| $body:expr) => {
        $crate::diff!($name, |$($a: $t),*| $body, $body);
    };
    ($name:ident, pin_defect = $d:path, |$($a:ident : $t:ty),*| $body:expr) => {
        $crate::diff!($name, pin_defect = $d, |$($a: $t),*| $body, $body);
    };
    ($name:ident, pin_defect = $d:path, |$($a:ident : $t:ty),*| $xb:expr, $sb:expr) => {
        #[test]
        #[allow(unused_variables, unused_mut, clippy::redundant_closure_call)]
        fn $name() {
            $crate::harness::run_defect::<($($t,)*), _, _>(
                concat!(module_path!(), "::", stringify!($name)),
                &$d(),
                |($($a,)*): ($($t,)*)| {
                    #[allow(unused_imports)]
                    use $crate::facade::x as m;
                    #[allow(unused_imports)]
                    use m::ext::*;
                    $xb
                },
                |($($a,)*): ($($t,)*)| {
                    #[allow(unused_imports)]
                    use $crate::facade::s as m;
                    #[allow(unused_imports)]
                    use m::ext::*;
                    $sb
                },
            );
        }
    };
    ($name:ident, |$($a:ident : $t:ty),*| $xb:expr, $sb:expr) => {
        #[test]
        #[allow(unused_variables, unused_mut, clippy::redundant_closure_call)]
        fn $name() {
            $crate::harness::run::<($($t,)*), _, _>(
                concat!(module_path!(), "::", stringify!($name)),
                |($($a,)*): ($($t,)*)| {
                    #[allow(unused_imports)]
                    use $crate::facade::x as m;
                    #[allow(unused_imports)]
                    use m::ext::*;
                    $xb
                },
                |($($a,)*): ($($t,)*)| {
                    #[allow(unused_imports)]
                    use $crate::facade::s as m;
                    #[allow(unused_imports)]
                    use m::ext::*;
                    $sb
                },
            );
        }
    };
}
