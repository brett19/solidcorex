// Written for solidcorex. Apache-2.0.
//! The soundness account of scx-num's interval arithmetic, tested: for random inputs
//! every interval result CONTAINS the exact result. The oracle is written here and uses
//! nothing of the code under test: exact sums and products of doubles as Shewchuk
//! expansions (Knuth's two-sum, Dekker's two-product with Veltkamp's split, no FMA), and
//! double-double Taylor series for sine and cosine (the arc cosine checked through the
//! cosine of its bounds). A comparison the double-double oracle cannot decide (its
//! difference under 1e-28 of the value) is counted, never passed.
//!
//! The operations and their account (`docs/research/53-interval-and-algo.md`):
//! - `+ - * /` (`Ivl`'s operators): the floating result widened by `|x| 2.3e-16 + 1e-300`
//!   outward, more than the half-ulp rounding error; checked at the four corners (each
//!   operation is monotone or bilinear in its operands, so its extremes are at corners).
//! - `add_ulp`..`div_ulp`: one ulp outward of the rounded corners.
//! - `sqrt`: IEEE square root is correctly rounded, so widening one step suffices;
//!   checked as `lo^2 <= x <= hi^2`.
//! - `acos_s`, `sin_cos_s` (`ring`): the C library's result widened by 1e-15 relative;
//!   an assumption on the library (about 4.5 ulp), tested here, not proved.
//! - Taylor models, the certificate's series, interval vectors: built from the above.
//!   Checked at sampled points against exact polynomial values.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::interval::Ivl;
use scx_num::ring::TayScalar;
use scx_num::series::{Ser, Trunc};
use scx_num::taylor_model::TmN;
use scx_num::vector3d::Vec3;

const CASES: usize = 20_000;

/// SplitMix64 (Steele, Lea and Flood 2014).
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    fn int(&mut self, k: i64) -> i64 {
        (self.next_u64() % (2 * k as u64 + 1)) as i64 - k
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// A double with a random 53-bit significand, sign and binary exponent in `[lo, hi]`.
    fn normal(&mut self, lo: i32, hi: i32) -> f64 {
        let m = (1.0 + self.unit()) * pow2(lo + self.below((hi - lo + 1) as u64) as i32);
        if self.next_u64() & 1 == 0 { m } else { -m }
    }
    /// An operand: mostly normal doubles of moderate size, sometimes 0, -0, 1, an integer,
    /// or a short-significand dyadic (so sums and products are often exact).
    fn operand(&mut self, lo: i32, hi: i32) -> f64 {
        match self.below(10) {
            0 => 0.0,
            1 => -0.0,
            2 => self.int(8) as f64,
            3 => self.int(1 << 20) as f64 * pow2(lo.max(-30) + self.below(20) as i32),
            _ => self.normal(lo, hi),
        }
    }
}

/// `2^e`, exact for `-1022 <= e <= 1023`.
fn pow2(e: i32) -> f64 {
    f64::from_bits(((e + 1023) as u64) << 52)
}

// ---- exact arithmetic of doubles -------------------------------------------------------

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a;
    let hi = c - (c - a);
    (hi, a - hi)
}

/// `a b = p + e` exactly, for products and error terms that neither overflow nor fall
/// below the normal range (the callers keep their operands there).
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

/// A sum of doubles held exactly: a nonoverlapping expansion, components in increasing
/// magnitude, zeros removed (Shewchuk 1997, `GROW-EXPANSION` with zero elimination).
#[derive(Clone, Debug, Default)]
struct Exact(Vec<f64>);

impl Exact {
    fn of(x: f64) -> Self {
        let mut e = Self::default();
        e.add(x);
        e
    }
    fn add(&mut self, b: f64) {
        let mut q = b;
        let mut out = Vec::with_capacity(self.0.len() + 1);
        for &c in &self.0 {
            let (s, h) = two_sum(q, c);
            if h != 0.0 {
                out.push(h);
            }
            q = s;
        }
        if q != 0.0 {
            out.push(q);
        }
        self.0 = out;
    }
    fn add_prod(&mut self, a: f64, b: f64) {
        let (p, e) = two_prod(a, b);
        self.add(e);
        self.add(p);
    }
    fn plus(&self, o: &Self) -> Self {
        let mut r = self.clone();
        for &c in &o.0 {
            r.add(c);
        }
        r
    }
    fn neg(&self) -> Self {
        Self(self.0.iter().map(|c| -c).collect())
    }
    fn minus(&self, o: &Self) -> Self {
        self.plus(&o.neg())
    }
    fn scale(&self, b: f64) -> Self {
        let mut r = Self::default();
        for &c in &self.0 {
            r.add_prod(c, b);
        }
        r
    }
    fn times(&self, o: &Self) -> Self {
        let mut r = Self::default();
        for &c in &o.0 {
            r = r.plus(&self.scale(c));
        }
        r
    }
    /// -1, 0 or 1: the sign of the largest component (nonoverlapping, so of the sum).
    fn sign(&self) -> i32 {
        match self.0.last() {
            None => 0,
            Some(&x) if x > 0.0 => 1,
            Some(_) => -1,
        }
    }
    /// `x <= self`.
    fn ge(&self, x: f64) -> bool {
        self.minus(&Exact::of(x)).sign() >= 0
    }
    /// `self <= x`.
    fn le(&self, x: f64) -> bool {
        self.minus(&Exact::of(x)).sign() <= 0
    }
}

fn prod(a: f64, b: f64) -> Exact {
    let mut e = Exact::default();
    e.add_prod(a, b);
    e
}

fn sum(a: f64, b: f64) -> Exact {
    let mut e = Exact::of(a);
    e.add(b);
    e
}

// ---- the tally --------------------------------------------------------------------------

#[derive(Default)]
struct Tally {
    checked: usize,
    undecided: usize,
    undecided_first: Vec<String>,
    failures: Vec<String>,
}

impl Tally {
    fn ok(&mut self, holds: bool, what: impl FnOnce() -> String) {
        self.checked += 1;
        if !holds && self.failures.len() < 20 {
            self.failures.push(what());
        } else if !holds {
            self.failures.push(String::new());
        }
    }
    fn finish(self, name: &str) {
        println!(
            "enclosure {name}: {} checks, {} undecided, {} failures",
            self.checked,
            self.undecided,
            self.failures.len()
        );
        for u in &self.undecided_first {
            println!("  undecided: {u}");
        }
        assert!(
            self.failures.is_empty(),
            "{name}: {} failures, first: {:#?}",
            self.failures.len(),
            &self.failures[..self.failures.len().min(5)]
        );
    }
}

fn ivl(rng: &mut Rng, lo: i32, hi: i32) -> Ivl {
    let a = rng.operand(lo, hi);
    match rng.below(4) {
        0 => Ivl::from(a),
        _ => {
            let b = rng.operand(lo, hi);
            if a <= b {
                Ivl::new(a, b)
            } else {
                Ivl::new(b, a)
            }
        }
    }
}

fn corners(a: Ivl, b: Ivl) -> [(f64, f64); 4] {
    [(a.lo, b.lo), (a.lo, b.hi), (a.hi, b.lo), (a.hi, b.hi)]
}

/// `r` holds `x ⊕ y` at every corner, the exact result scaled by `2^k` on both sides
/// (`k > 0` lifts results below the normal range into it, exactly).
fn check_corners(
    t: &mut Tally,
    name: &str,
    a: Ivl,
    b: Ivl,
    r: Ivl,
    k: i32,
    exact: impl Fn(f64, f64) -> Exact,
) {
    if r.lo > r.hi || r.lo.is_nan() || r.hi.is_nan() {
        t.ok(false, || {
            format!("{name}({a:?}, {b:?}) = {r:?}: not an interval")
        });
        return;
    }
    let (rlo, rhi) = (r.lo * pow2(k), r.hi * pow2(k));
    for (x, y) in corners(a, b) {
        let v = exact(x, y);
        let holds =
            (r.lo == f64::NEG_INFINITY || v.ge(rlo)) && (r.hi == f64::INFINITY || v.le(rhi));
        t.ok(holds, || {
            format!("{name}({a:?}, {b:?}) = {r:?} misses the corner ({x:e}, {y:e})")
        });
    }
}

/// The quotient's corners: `lo <= x / y <= hi` as `lo y <= x <= hi y` for `y > 0`
/// (reversed for `y < 0`), scaled by `2^k`.
fn check_quotient(t: &mut Tally, name: &str, a: Ivl, b: Ivl, r: Ivl, k: i32) {
    if r.lo > r.hi || r.lo.is_nan() || r.hi.is_nan() {
        t.ok(false, || {
            format!("{name}({a:?}, {b:?}) = {r:?}: not an interval")
        });
        return;
    }
    for (x, y) in corners(a, b) {
        let xs = x * pow2(k);
        let below = r.lo == f64::NEG_INFINITY || {
            let ly = prod(r.lo, y);
            if y > 0.0 { ly.le(xs) } else { ly.ge(xs) }
        };
        let above = r.hi == f64::INFINITY || {
            let hy = prod(r.hi, y);
            if y > 0.0 { hy.ge(xs) } else { hy.le(xs) }
        };
        t.ok(below && above, || {
            format!("{name}({a:?}, {b:?}) = {r:?} misses the corner ({x:e}, {y:e})")
        });
    }
}

fn excludes_zero(b: Ivl) -> bool {
    b.lo > 0.0 || b.hi < 0.0
}

type Ops = [(&'static str, fn(Ivl, Ivl) -> Ivl); 4];

const OPERATORS: Ops = [
    ("+", |a, b| a + b),
    ("-", |a, b| a - b),
    ("*", |a, b| a * b),
    ("/", |a, b| a / b),
];
const ULP: Ops = [
    ("add_ulp", Ivl::add_ulp),
    ("sub_ulp", Ivl::sub_ulp),
    ("mul_ulp", Ivl::mul_ulp),
    ("div_ulp", Ivl::div_ulp),
];

fn arithmetic(ops: Ops, seed: u64, name: &str) {
    let mut rng = Rng(seed);
    let mut t = Tally::default();
    for case in 0..CASES {
        // moderate operands, then operands whose results fall below the normal range
        // (scaled by 2^600 for the oracle)
        let tiny = case % 4 == 3;
        let (lo, hi, k, s) = if tiny {
            (-540, -500, 600, 300)
        } else {
            (-100, 100, 0, 0)
        };
        let a = ivl(&mut rng, lo, hi);
        let b = ivl(&mut rng, lo, hi);
        let up = |x: f64| x * pow2(s);
        for (i, (op, f)) in ops.iter().enumerate() {
            let r = f(a, b);
            match i {
                0 => check_corners(&mut t, op, a, b, r, k, |x, y| {
                    sum(up(x), up(y)).scale(pow2(k - s))
                }),
                1 => check_corners(&mut t, op, a, b, r, k, |x, y| {
                    sum(up(x), -up(y)).scale(pow2(k - s))
                }),
                2 => check_corners(&mut t, op, a, b, r, k, |x, y| prod(up(x), up(y))),
                _ if excludes_zero(b) => check_quotient(&mut t, op, a, b, r, 0),
                _ => t.ok(r.lo == f64::NEG_INFINITY && r.hi == f64::INFINITY, || {
                    format!("{op}({a:?}, {b:?}) = {r:?}: the divisor holds 0, not the whole line")
                }),
            }
        }
    }
    t.finish(name);
}

#[test]
fn operators_enclose_the_exact_results_at_every_corner() {
    arithmetic(OPERATORS, 0x1A7E_0001, "operators");
}

#[test]
fn ulp_operations_enclose_the_exact_results_at_every_corner() {
    arithmetic(ULP, 0x1A7E_0002, "ulp operations");
}

#[test]
fn sqrt_bounds_square_around_the_operand() {
    let mut rng = Rng(0x1A7E_0003);
    let mut t = Tally::default();
    for _ in 0..CASES {
        let x = ivl(&mut rng, -200, 200);
        let r = x.sqrt();
        if x.hi < 0.0 || x.hi.is_nan() {
            t.ok(r.lo == f64::NEG_INFINITY && r.hi == f64::INFINITY, || {
                format!("sqrt({x:?}) = {r:?}")
            });
            continue;
        }
        // lo^2 <= max(x.lo, 0) and x.hi <= hi^2, lo >= 0 or below
        let lo_ok = r.lo <= 0.0 || prod(r.lo, r.lo).le(x.lo.max(0.0));
        let hi_ok = r.hi >= 0.0 && prod(r.hi, r.hi).ge(x.hi);
        t.ok(lo_ok && hi_ok, || format!("sqrt({x:?}) = {r:?}"));
    }
    t.finish("sqrt");
}

#[test]
fn interval_vectors_enclose_exact_dot_cross_and_norm() {
    let mut rng = Rng(0x1A7E_0004);
    let mut t = Tally::default();
    for _ in 0..CASES / 4 {
        let a: [f64; 3] = core::array::from_fn(|_| rng.operand(-60, 60));
        let b: [f64; 3] = core::array::from_fn(|_| rng.operand(-60, 60));
        let (ia, ib) = (Vec3::from(a.map(Ivl::from)), Vec3::from(b.map(Ivl::from)));
        let d = ia.dot(ib);
        let mut e = Exact::default();
        for i in 0..3 {
            e.add_prod(a[i], b[i]);
        }
        t.ok(e.ge(d.lo) && e.le(d.hi), || {
            format!("dot({a:?}, {b:?}) = {d:?}")
        });
        let c = ia.cross(ib).to_array();
        for (i, (p, q)) in [(1, 2), (2, 0), (0, 1)].into_iter().enumerate() {
            let v = prod(a[p], b[q]).minus(&prod(a[q], b[p]));
            t.ok(v.ge(c[i].lo) && v.le(c[i].hi), || {
                format!("cross({a:?}, {b:?})[{i}] = {:?}", c[i])
            });
        }
        let n = ia.norm();
        let mut sq = Exact::default();
        for x in a {
            sq.add_prod(x, x);
        }
        let lo_ok = n.lo <= 0.0 || prod(n.lo, n.lo).minus(&sq).sign() <= 0;
        let hi_ok = prod(n.hi, n.hi).minus(&sq).sign() >= 0;
        t.ok(lo_ok && hi_ok, || format!("norm({a:?}) = {n:?}"));
    }
    t.finish("interval vectors");
}

// ---- double-double sine, for the arc cosine and the sine and cosine -----------------------

#[derive(Clone, Copy, Debug)]
struct Dd(f64, f64);

fn quick(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd(s, b - (s - a))
}

fn dd_add(a: Dd, b: Dd) -> Dd {
    let (s, e) = two_sum(a.0, b.0);
    let (t, f) = two_sum(a.1, b.1);
    let d = quick(s, e + t);
    quick(d.0, d.1 + f)
}

fn dd_mul(a: Dd, b: Dd) -> Dd {
    let (p, e) = two_prod(a.0, b.0);
    quick(p, e + (a.0 * b.1 + a.1 * b.0))
}

fn dd_div(a: Dd, d: f64) -> Dd {
    let q1 = a.0 / d;
    let (p, e) = two_prod(q1, d);
    let r = ((a.0 - p) - e) + a.1;
    quick(q1, r / d)
}

/// pi and pi/2 to about 1e-33.
const PI: Dd = Dd(core::f64::consts::PI, 1.224_646_799_147_353_2e-16);
const PIO2: Dd = Dd(core::f64::consts::FRAC_PI_2, 6.123_233_995_736_766e-17);

/// `sin x` for `|x| <= pi/2`, relative error about 1e-31 (Taylor to degree 61).
fn dd_sin(x: Dd) -> Dd {
    let x2 = dd_mul(x, x);
    let mut term = x;
    let mut s = x;
    for k in 1..=30_u32 {
        term = dd_div(dd_mul(term, x2), -f64::from((2 * k) * (2 * k + 1)));
        s = dd_add(s, term);
    }
    s
}

/// The sign of `a - x`, or `None` where the oracle cannot tell (under 1e-28 relative).
fn cmp(a: Dd, x: f64) -> Option<i32> {
    let d = dd_add(a, Dd(-x, 0.0));
    let scale = a.0.abs().max(x.abs());
    if d.0.abs() <= 1e-28 * scale {
        None
    } else {
        Some(if d.0 > 0.0 { 1 } else { -1 })
    }
}

/// The sign of `a - b`, or `None` where the oracle cannot tell (under 1e-28 relative).
fn cmp_dd(a: Dd, b: Dd) -> Option<i32> {
    let d = dd_add(a, Dd(-b.0, -b.1));
    let scale = a.0.abs().max(b.0.abs());
    if d.0.abs() <= 1e-28 * scale {
        None
    } else {
        Some(if d.0 > 0.0 { 1 } else { -1 })
    }
}

/// A double near the interesting points of `[lo, hi]`, or uniform in it.
fn near(rng: &mut Rng, lo: f64, hi: f64, points: &[f64]) -> f64 {
    let x = match rng.below(3) {
        0 => {
            let p = points[rng.below(points.len() as u64) as usize];
            let d = pow2(-(rng.below(60) as i32)) * rng.unit();
            if rng.next_u64() & 1 == 0 {
                p + d
            } else {
                p - d
            }
        }
        1 => points[rng.below(points.len() as u64) as usize],
        _ => lo + (hi - lo) * rng.unit(),
    };
    x.clamp(lo, hi)
}

fn decide(t: &mut Tally, c: Option<i32>, want: i32, what: impl FnOnce() -> String) {
    match c {
        None => {
            t.undecided += 1;
            if t.undecided_first.len() < 3 {
                t.undecided_first.push(what());
            }
        }
        Some(s) => t.ok(s == want || s == 0, what),
    }
}

#[test]
fn acos_encloses_through_the_cosine_of_its_bounds() {
    let mut rng = Rng(0x1A7E_0005);
    let mut t = Tally::default();
    for _ in 0..CASES {
        let pts = [-1.0, -0.5, 0.0, 0.5, 1.0];
        let p = near(&mut rng, -1.0, 1.0, &pts);
        let q = near(&mut rng, -1.0, 1.0, &pts);
        let y = Ivl::new(p.min(q), p.max(q));
        let r = y.acos_s();
        let (l, h) = (r.lo, r.hi);
        // l <= acos(y.hi): l <= 0, or 1 - cos l = 2 sin^2(l/2) <= 1 - y.hi
        if l > 0.0 {
            let s = dd_sin(Dd(l / 2.0, 0.0));
            let omc = dd_mul(Dd(2.0, 0.0), dd_mul(s, s));
            let (a, b) = two_sum(1.0, -y.hi);
            let c = cmp_dd(Dd(a, b), omc);
            // acos(-1) = pi: the cosine is flat there, so decide by l <= pi directly
            let c = if l > PI.0 {
                Some(-1)
            } else if y.hi == -1.0 {
                Some(1)
            } else {
                c
            };
            decide(&mut t, c, 1, || {
                format!(
                    "acos({y:?}) = {r:?}: the lower end is above acos({:e})",
                    y.hi
                )
            });
        } else {
            t.ok(true, String::new);
        }
        // h >= acos(y.lo): h >= pi, or 1 + cos h = 2 sin^2((pi - h)/2) <= 1 + y.lo
        if h < PI.0 + 4e-16 {
            let d = dd_add(PI, Dd(-h, 0.0));
            let c = if d.0 <= 0.0 {
                Some(1)
            } else {
                let s = dd_sin(Dd(d.0 / 2.0, d.1 / 2.0));
                let opc = dd_mul(Dd(2.0, 0.0), dd_mul(s, s));
                let (a, b) = two_sum(1.0, y.lo);
                cmp_dd(Dd(a, b), opc)
            };
            // acos(1) = 0: decide by h >= 0 directly
            let c = if h < 0.0 {
                Some(-1)
            } else if y.lo == 1.0 {
                Some(1)
            } else {
                c
            };
            decide(&mut t, c, 1, || {
                format!(
                    "acos({y:?}) = {r:?}: the upper end is below acos({:e})",
                    y.lo
                )
            });
        } else {
            t.ok(true, String::new);
        }
    }
    // outside [-1, 1]: the whole line
    for y in [
        Ivl::new(-1.5, 0.0),
        Ivl::new(0.0, 1.0 + 1e-16 * 3.0),
        Ivl::new(f64::NAN, 0.0),
    ] {
        let r = y.acos_s();
        t.ok(r.lo == f64::NEG_INFINITY && r.hi == f64::INFINITY, || {
            format!("acos({y:?}) = {r:?}")
        });
    }
    t.finish("acos");
}

#[test]
fn sin_cos_enclose_on_the_first_quadrant() {
    let mut rng = Rng(0x1A7E_0006);
    let mut t = Tally::default();
    for _ in 0..CASES {
        let top = PIO2.0;
        let pts = [0.0, 1e-8, 0.5, 1.0, top];
        let p = near(&mut rng, 0.0, top, &pts);
        let q = near(&mut rng, 0.0, top, &pts);
        let x = Ivl::new(p.min(q), p.max(q));
        let (s, c) = x.sin_cos_s();
        let sin = |v: f64| dd_sin(Dd(v, 0.0));
        let cos = |v: f64| dd_sin(dd_add(PIO2, Dd(-v, 0.0)));
        decide(&mut t, cmp(sin(x.lo), s.lo), 1, || {
            format!("sin({x:?}) = {s:?}: low")
        });
        decide(&mut t, cmp(sin(x.hi), s.hi), -1, || {
            format!("sin({x:?}) = {s:?}: high")
        });
        decide(&mut t, cmp(cos(x.hi), c.lo), 1, || {
            format!("cos({x:?}) = {c:?}: low")
        });
        decide(&mut t, cmp(cos(x.lo), c.hi), -1, || {
            format!("cos({x:?}) = {c:?}: high")
        });
    }
    for x in [
        Ivl::new(-1e-300, 0.5),
        Ivl::new(0.5, 1.6),
        Ivl::new(f64::NAN, 1.0),
    ] {
        let (s, c) = x.sin_cos_s();
        let whole = |r: Ivl| r.lo == f64::NEG_INFINITY && r.hi == f64::INFINITY;
        t.ok(whole(s) && whole(c), || {
            format!("sin_cos({x:?}) = {s:?} {c:?}")
        });
    }
    t.finish("sin and cos");
}

// ---- Taylor models -----------------------------------------------------------------------

/// A function the model encloses: its coefficients and its remainder, picked inside the
/// model's (each a short dyadic, so its value at a dyadic t is a short exact expansion).
struct Picked<const N: usize> {
    tm: TmN<N>,
    coeffs: Vec<f64>,
    e: f64,
}

impl<const N: usize> Picked<N> {
    fn value(&self, t: f64) -> Exact {
        let mut acc = Exact::default();
        for &c in self.coeffs.iter().rev() {
            acc = acc.scale(t);
            acc.add(c);
        }
        acc.add(self.e);
        acc
    }
}

fn dyadic(rng: &mut Rng, k: i64, bits: i32) -> f64 {
    rng.int(k) as f64 * pow2(-bits)
}

/// A model of degree up to `deg` (past `N - 1` it folds into the remainder), coefficients
/// some points and some intervals of width up to 1/64, a remainder of width up to 1/256;
/// `positive` makes its constant at least 1 above the sum of the rest's bounds.
fn picked<const N: usize>(rng: &mut Rng, deg: usize, positive: bool) -> Picked<N> {
    let mut lo: Vec<f64> = (0..=deg).map(|_| dyadic(rng, 64, 6)).collect();
    let mut width: Vec<f64> = (0..=deg)
        .map(|_| {
            if rng.below(2) == 0 {
                0.0
            } else {
                dyadic(rng, 4, 8).abs()
            }
        })
        .collect();
    if rng.below(5) == 0 {
        width.iter_mut().for_each(|w| *w = 0.0);
    }
    let rem_lo = if rng.below(3) == 0 {
        0.0
    } else {
        dyadic(rng, 4, 10)
    };
    let rem_w = if rem_lo == 0.0 {
        0.0
    } else {
        dyadic(rng, 4, 10).abs()
    };
    if positive {
        let rest: f64 = lo
            .iter()
            .zip(&width)
            .skip(1)
            .map(|(c, w)| c.abs() + w)
            .sum();
        lo[0] = rest + 1.0 + rem_lo.abs() + rem_w;
    }
    let coeffs: Vec<f64> = lo
        .iter()
        .zip(&width)
        .map(|(&c, &w)| match rng.below(3) {
            0 => c,
            1 => c + w,
            _ => c + (w / 2.0),
        })
        .collect();
    let ivls: Vec<Ivl> = lo
        .iter()
        .zip(&width)
        .map(|(&c, &w)| Ivl::new(c, c + w))
        .collect();
    let rem = Ivl::new(rem_lo, rem_lo + rem_w);
    let e = if rng.below(2) == 0 { rem.lo } else { rem.hi };
    Picked {
        tm: TmN::<N>::from_poly(&ivls, rem),
        coeffs,
        e,
    }
}

fn holds(r: Ivl, v: &Exact) -> bool {
    r.lo <= r.hi && v.ge(r.lo) && v.le(r.hi)
}

fn taylor_models<const N: usize>(seed: u64, name: &str) {
    let mut rng = Rng(seed);
    let mut t = Tally::default();
    for _ in 0..400 {
        let deg = rng.below(N as u64 + 3) as usize;
        let deg_b = rng.below(N as u64 + 3) as usize;
        let deg_q = rng.below(N as u64 + 3) as usize;
        let a = picked::<N>(&mut rng, deg, false);
        let b = picked::<N>(&mut rng, deg_b, false);
        let p = picked::<N>(&mut rng, deg, true);
        let q = picked::<N>(&mut rng, deg_q, true);
        let x = dyadic(&mut rng, 64, 4);
        let models = [
            ("at", a.tm),
            ("+", a.tm + b.tm),
            ("-", a.tm - b.tm),
            ("*", a.tm * b.tm),
            ("scale", a.tm.scale(Ivl::from(x))),
            ("add_ivl", a.tm.add_ivl(Ivl::from(x))),
        ];
        let range = a.tm.range();
        for k in -16..=16 {
            let s = f64::from(k) / 16.0;
            let (fa, fb, fp, fq) = (a.value(s), b.value(s), p.value(s), q.value(s));
            let truth = [
                fa.clone(),
                fa.plus(&fb),
                fa.minus(&fb),
                fa.times(&fb),
                fa.scale(x),
                fa.plus(&Exact::of(x)),
            ];
            for ((op, m), v) in models.iter().zip(&truth) {
                let r = m.at(s);
                t.ok(holds(r, v), || {
                    format!("{op} at {s}: {r:?} misses {:?}", v.0)
                });
            }
            t.ok(holds(range, &fa), || {
                format!("range {range:?} misses the value at {s}")
            });
            // 1/p: lo p <= 1 <= hi p (p >= 1)
            let r = p.tm.recip().at(s);
            t.ok(fp.scale(r.lo).le(1.0) && fp.scale(r.hi).ge(1.0), || {
                format!("recip at {s}: {r:?}")
            });
            // sqrt p: lo^2 <= p <= hi^2
            let r = p.tm.sqrt_tm().at(s);
            let lo_ok = r.lo <= 0.0 || prod(r.lo, r.lo).minus(&fp).sign() <= 0;
            t.ok(lo_ok && prod(r.hi, r.hi).minus(&fp).sign() >= 0, || {
                format!("sqrt at {s}: {r:?}")
            });
            // a/q: lo q <= a <= hi q (q >= 1)
            let r = (a.tm / q.tm).at(s);
            t.ok(
                fq.scale(r.lo).minus(&fa).sign() <= 0 && fq.scale(r.hi).minus(&fa).sign() >= 0,
                || format!("/ at {s}: {r:?}"),
            );
        }
    }
    t.finish(name);
}

#[test]
fn taylor_models_enclose_exact_polynomial_values() {
    taylor_models::<11>(0x1A7E_0007, "taylor models, degree 10");
}

#[test]
fn low_degree_taylor_models_fold_their_tails_soundly() {
    taylor_models::<4>(0x1A7E_0008, "taylor models, degree 3");
}

// ---- the certificate's series --------------------------------------------------------------

/// `C(n, k)`, exact for the small n here.
fn binom(n: u32, k: u32) -> f64 {
    (0..k).fold(1.0, |acc, j| acc * f64::from(n - j) / f64::from(j + 1))
}

#[test]
fn interval_series_enclose_exact_products_and_sums() {
    let mut rng = Rng(0x1A7E_0009);
    let mut t = Tally::default();
    for _ in 0..300 {
        let (s, e) = (rng.below(8) as usize, rng.below(5) as usize);
        let tr = Trunc::new(s, e).unwrap();
        let mut a = [[0.0; 5]; 8];
        let mut b = [[0.0; 5]; 8];
        let mut sa = Ser::<Ivl>::constant(Ivl::from(0.0));
        let mut sb = sa.clone();
        for i in 0..=s {
            for k in 0..=e {
                a[i][k] = dyadic(&mut rng, 1 << 12, 10);
                b[i][k] = dyadic(&mut rng, 1 << 12, 10);
                sa.set(i, k, Ivl::from(a[i][k]));
                sb.set(i, k, Ivl::from(b[i][k]));
            }
        }
        let m = sa.mul(&sb, tr);
        let ad = sa.add(&sb, tr);
        let sd = sa.sub(&sb, tr);
        for i in 0..=s {
            for k in 0..=e {
                let mut v = Exact::default();
                for i1 in 0..=i {
                    for k1 in 0..=k {
                        v.add_prod(a[i1][k1], b[i - i1][k - k1]);
                    }
                }
                t.ok(holds(m.at(i, k), &v), || {
                    format!("mul c[{i}][{k}] = {:?}", m.at(i, k))
                });
                t.ok(holds(ad.at(i, k), &sum(a[i][k], b[i][k])), || {
                    format!("add c[{i}][{k}]")
                });
                t.ok(holds(sd.at(i, k), &sum(a[i][k], -b[i][k])), || {
                    format!("sub c[{i}][{k}]")
                });
            }
        }
    }
    t.finish("series products and sums");
}

/// `(c0 + c1 sigma + c2 eps)^(-1/2)`: its coefficient of `sigma^i eps^k` is
/// `B_n C(n, i) c1^i c2^k c0^(-n - 1/2)`, `n = i + k`, `B_n = (-1)^n C(2n, n) / 4^n`. With
/// `A = (-1)^n C(2n, n) C(n, i) c1^i c2^k` (so the value is `A / (4^n c0^n sqrt c0)`), a bound
/// `x` of the value's sign holds it iff `x^2 16^n c0^(2n + 1)` is at most (for the bound
/// nearer 0) or at least (for the one farther) `A^2`.
#[test]
fn inverse_square_root_series_enclose_the_binomial_series() {
    let mut rng = Rng(0x1A7E_000A);
    let mut t = Tally::default();
    for _ in 0..400 {
        let (s, e) = (rng.below(8) as usize, rng.below(5) as usize);
        let tr = Trunc::new(s, e).unwrap();
        let c0 = (8 + rng.below(57)) as f64 / 16.0;
        let c1 = dyadic(&mut rng, 16, 4);
        let c2 = dyadic(&mut rng, 16, 4);
        let mut x = Ser::<Ivl>::constant(Ivl::from(c0));
        x.set(1, 0, Ivl::from(c1));
        x.set(0, 1, Ivl::from(c2));
        let y = x.inv_sqrt(tr).unwrap();
        for i in 0..=s {
            for k in 0..=e {
                let n = (i + k) as u32;
                let r = y.at(i, k);
                let mut a = Exact::of(if n.is_multiple_of(2) { 1.0 } else { -1.0 });
                a = a.scale(binom(2 * n, n)).scale(binom(n, i as u32));
                for _ in 0..i {
                    a = a.scale(c1);
                }
                for _ in 0..k {
                    a = a.scale(c2);
                }
                let a2 = a.times(&a);
                let lift = |b: f64| {
                    let mut v = prod(b, b).scale(pow2(4 * n as i32));
                    for _ in 0..(2 * n + 1) {
                        v = v.scale(c0);
                    }
                    v
                };
                let ok = match a.sign() {
                    0 => r.lo <= 0.0 && r.hi >= 0.0,
                    1 => {
                        (r.lo <= 0.0 || lift(r.lo).minus(&a2).sign() <= 0)
                            && r.hi > 0.0
                            && lift(r.hi).minus(&a2).sign() >= 0
                    }
                    _ => {
                        (r.hi >= 0.0 || lift(r.hi).minus(&a2).sign() <= 0)
                            && r.lo < 0.0
                            && lift(r.lo).minus(&a2).sign() >= 0
                    }
                };
                t.ok(ok, || {
                    format!("inv_sqrt({c0}, {c1}, {c2}) c[{i}][{k}] = {r:?}")
                });
            }
        }
    }
    t.finish("series inverse square root");
}

// ---- the gap found ---------------------------------------------------------------------------

/// FOUND, NOT CHANGED (`docs/research/53-interval-and-algo.md`, "Soundness"): an operator
/// whose floating lower end overflows to `+inf` (or upper end to `-inf`) widens it to
/// `inf - inf = NaN`, not to the whole line its documentation promises. The pinned
/// solidcore does the same. Pinned here so a fix is a visible, ruled change.
#[test]
fn an_overflowing_end_widens_to_nan_found_not_changed() {
    let big = Ivl::from(f64::MAX);
    let r = big + big;
    assert!(r.lo.is_nan() && r.hi == f64::INFINITY, "{r:?}");
    let r = Ivl::from(-f64::MAX) + Ivl::from(-f64::MAX);
    assert!(r.lo == f64::NEG_INFINITY && r.hi.is_nan(), "{r:?}");
    let r = Ivl::new(f64::MAX, f64::MAX) * Ivl::from(2.0);
    assert!(r.lo.is_nan(), "{r:?}");
    let r = Ivl::new(f64::INFINITY, f64::INFINITY).sqrt();
    assert!(r.lo.is_nan(), "{r:?}");
    // the magnitude of such an interval is NaN, which `f64::max` then drops
    assert!((big + big).mag().is_nan());
    assert_eq!(1.0_f64.max((big + big).mag()), 1.0);
}
