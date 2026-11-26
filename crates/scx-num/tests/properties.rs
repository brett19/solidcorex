// Written for solidcorex. Apache-2.0.
//! Tier-1 property tests of scx-num, each against an oracle that does not use the code
//! under test: exact integer arithmetic (`i128`), the IEEE bit layout, plain `min`/`max`
//! over the inputs, the documented size table, or a residual computed separately.
//! Inputs come from a fixed-seed SplitMix64 written out here, so every run draws the
//! same cases.

#![allow(missing_docs, clippy::pedantic)]

use scx_num::certified::{next_down, next_up};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::extent3d::Extent3d;
use scx_num::math::Transform;
use scx_num::matrix::Matrix;
use scx_num::small_matrix::{
    determinate_3x3, determinate_4x4, inverse_3x3, inverse_4x4, mult_4x4, vec_triple_product,
};
use scx_num::tol::zone_for_size;
use scx_num::vector2d::Vec2;
use scx_num::vector3d::Vec3;

const CASES: usize = 2000;

/// SplitMix64 (Steele, Lea and Flood 2014).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// An integer in `[-k, k]`.
    fn int(&mut self, k: i64) -> i64 {
        (self.next_u64() % (2 * k as u64 + 1)) as i64 - k
    }
    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * u
    }
    /// Any finite double, by its bits.
    fn finite(&mut self) -> f64 {
        loop {
            let x = f64::from_bits(self.next_u64());
            if x.is_finite() {
                return x;
            }
        }
    }
}

fn ivec(r: &mut Rng, k: i64) -> [i64; 3] {
    [r.int(k), r.int(k), r.int(k)]
}
fn fvec(a: [i64; 3]) -> Vec3 {
    Vec3::new(a[0] as f64, a[1] as f64, a[2] as f64)
}

/// `next_up`/`next_down` against the IEEE layout: for a finite positive `x` the next
/// double up has bits one higher, for a negative one bits one lower.
#[test]
fn next_up_and_down_step_one_bit_pattern() {
    let mut r = Rng::new(1);
    for _ in 0..CASES {
        let x = r.finite();
        let b = x.to_bits();
        let up = if x > 0.0 {
            f64::from_bits(b + 1)
        } else if x < 0.0 {
            f64::from_bits(b - 1)
        } else {
            f64::from_bits(1)
        };
        if x != f64::MAX {
            assert_eq!(next_up(x).to_bits(), up.to_bits(), "{x:e}");
        }
        assert!(next_down(x) < x);
        assert_eq!(next_down(next_up(x)), if x == 0.0 { 0.0 } else { x });
    }
    assert_eq!(next_up(f64::MAX), f64::INFINITY);
    assert_eq!(next_up(-0.0).to_bits(), 1);
    assert_eq!(next_down(0.0).to_bits(), (-f64::from_bits(1)).to_bits());
}

/// Dot, cross and triple products of integer vectors against exact `i128` arithmetic
/// (entries up to 2^15, so every partial result is exact in `f64`).
#[test]
fn products_match_integer_arithmetic() {
    let mut r = Rng::new(2);
    for _ in 0..CASES {
        let (a, b, c) = (ivec(&mut r, 1 << 15), ivec(&mut r, 1 << 15), ivec(&mut r, 1 << 15));
        let w = |v: [i64; 3]| v.map(i128::from);
        let (a2, b2, c2) = (w(a), w(b), w(c));
        let dot = a2[0] * b2[0] + a2[1] * b2[1] + a2[2] * b2[2];
        let cross = [
            a2[1] * b2[2] - a2[2] * b2[1],
            a2[2] * b2[0] - a2[0] * b2[2],
            a2[0] * b2[1] - a2[1] * b2[0],
        ];
        let triple = cross[0] * c2[0] + cross[1] * c2[1] + cross[2] * c2[2];
        assert_eq!(fvec(a).dot(fvec(b)), dot as f64);
        assert_eq!(fvec(a).cross(fvec(b)).to_array(), cross.map(|x| x as f64));
        assert_eq!(fvec(a).cross(fvec(b)).dot(fvec(c)), triple as f64);
        let af = a.map(|x| x as f64);
        let bf = b.map(|x| x as f64);
        let cf = c.map(|x| x as f64);
        assert_eq!(vec_triple_product(&af, &bf, &cf), triple as f64);
        assert_eq!(
            determinate_3x3(af[0], af[1], af[2], bf[0], bf[1], bf[2], cf[0], cf[1], cf[2]),
            triple as f64
        );
    }
}

/// The exact determinant of a 4x4 integer matrix, by the Leibniz sum over permutations.
fn det4_exact(m: &[[i64; 4]; 4]) -> i128 {
    let mut sum = 0i128;
    let mut p = [0usize, 1, 2, 3];
    // Heap's algorithm over the 24 permutations, tracking the sign.
    let mut c = [0usize; 4];
    let mut sign = 1i128;
    let term = |p: &[usize; 4]| (0..4).map(|i| i128::from(m[i][p[i]])).product::<i128>();
    sum += sign * term(&p);
    let mut i = 0;
    while i < 4 {
        if c[i] < i {
            if i % 2 == 0 {
                p.swap(0, i);
            } else {
                p.swap(c[i], i);
            }
            sign = -sign;
            sum += sign * term(&p);
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    sum
}

/// 4x4 determinant and product of integer matrices against exact arithmetic.
#[test]
fn four_by_four_matches_integer_arithmetic() {
    let mut r = Rng::new(3);
    for _ in 0..CASES {
        let mut a = [[0i64; 4]; 4];
        let mut b = [[0i64; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                a[i][j] = r.int(64);
                b[i][j] = r.int(64);
            }
        }
        let af = a.map(|row| row.map(|x| x as f64));
        let bf = b.map(|row| row.map(|x| x as f64));
        assert_eq!(determinate_4x4(&af), det4_exact(&a) as f64);
        let p = mult_4x4(&af, &bf);
        for i in 0..4 {
            for j in 0..4 {
                let e: i64 = (0..4).map(|k| a[i][k] * b[k][j]).sum();
                assert_eq!(p[i][j], e as f64);
            }
        }
    }
}

/// A random unimodular integer matrix (determinant +-1): the identity under row swaps
/// and small row additions, with entries kept below 1000.
fn unimodular<const N: usize>(r: &mut Rng) -> [[i64; N]; N] {
    let mut m = [[0i64; N]; N];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1;
    }
    for _ in 0..12 {
        let i = (r.next_u64() % N as u64) as usize;
        let j = (r.next_u64() % N as u64) as usize;
        if i == j {
            continue;
        }
        if r.next_u64() % 3 == 0 {
            m.swap(i, j);
        } else {
            let k = r.int(2);
            let next: [i64; N] = std::array::from_fn(|c| m[i][c] + k * m[j][c]);
            if next.iter().all(|x| x.abs() < 1000) {
                m[i] = next;
            }
        }
    }
    m
}

/// Inverses of unimodular integer matrices: the cofactors are integers and the
/// determinant is +-1, so `A * inverse(A)` is the identity exactly.
#[test]
fn inverses_of_unimodular_matrices_are_exact() {
    let mut r = Rng::new(4);
    for _ in 0..CASES {
        let a = unimodular::<3>(&mut r);
        let af = a.map(|row| row.map(|x| x as f64));
        let inv = inverse_3x3(&af[0], &af[1], &af[2]).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let s: f64 = (0..3).map(|k| af[i][k] * inv[k][j]).sum();
                assert_eq!(s, if i == j { 1.0 } else { 0.0 }, "{a:?}");
            }
        }
        let b = unimodular::<4>(&mut r);
        let bf = b.map(|row| row.map(|x| x as f64));
        let inv = inverse_4x4(&bf).unwrap();
        let id = mult_4x4(&bf, &inv);
        for (i, row) in id.iter().enumerate() {
            for (j, &x) in row.iter().enumerate() {
                assert_eq!(x, if i == j { 1.0 } else { 0.0 }, "{b:?}");
            }
        }
    }
}

/// An integer translation and its inverse carry an integer point there and back exactly.
#[test]
fn translation_round_trip_is_exact() {
    let mut r = Rng::new(5);
    for _ in 0..CASES {
        let d = fvec(ivec(&mut r, 1 << 20));
        let p = fvec(ivec(&mut r, 1 << 20));
        let t = Transform::translation(d);
        let q = t.transform_point(p);
        assert_eq!(q.to_array(), (p + d).to_array());
        let back = t.inverse().unwrap().transform_point(q);
        assert_eq!(back.to_array(), p.to_array());
    }
}

/// Interval union and intersection against `min`/`max` of the four ends.
#[test]
fn intervals_union_and_intersect() {
    let mut r = Rng::new(6);
    for _ in 0..CASES {
        let (a0, a1, b0, b1) = (
            r.uniform(-1e3, 1e3),
            r.uniform(-1e3, 1e3),
            r.uniform(-1e3, 1e3),
            r.uniform(-1e3, 1e3),
        );
        let a = Extent1d::from_min_max(a0, a1);
        let b = Extent1d::from_min_max(b0, b1);
        let (alo, ahi) = (a0.min(a1), a0.max(a1));
        let (blo, bhi) = (b0.min(b1), b0.max(b1));
        assert_eq!((a.min(), a.max()), (alo, ahi));
        let u = a.union(&b);
        assert_eq!((u.min(), u.max()), (alo.min(blo), ahi.max(bhi)));
        let lo = alo.max(blo);
        let hi = ahi.min(bhi);
        match a.intersect(&b) {
            Ok(i) => {
                assert!(lo <= hi);
                assert_eq!((i.min(), i.max()), (lo, hi));
            }
            Err(_) => assert!(lo > hi),
        }
        let v = r.uniform(-1.1e3, 1.1e3);
        if (v - alo).abs() > 1e-6 && (v - ahi).abs() > 1e-6 {
            assert_eq!(a.contains_value(v, 0.0), alo < v && v < ahi, "{v} in [{alo}, {ahi}]");
        }
    }
}

/// A box grown point by point is the component-wise `min`/`max` of the points; a 2D box
/// contains a point (well away from its sides) exactly when each coordinate is between.
#[test]
fn boxes_bound_their_points() {
    let mut r = Rng::new(7);
    for _ in 0..CASES / 4 {
        let n = 1 + (r.next_u64() % 8) as usize;
        let pts: Vec<Vec3> = (0..n)
            .map(|_| Vec3::new(r.uniform(-1e3, 1e3), r.uniform(-1e3, 1e3), r.uniform(-1e3, 1e3)))
            .collect();
        let mut e = Extent3d::new();
        for &p in &pts {
            e.add_point(p);
        }
        let lo = pts.iter().fold([f64::INFINITY; 3], |m, p| {
            [m[0].min(p.x), m[1].min(p.y), m[2].min(p.z)]
        });
        let hi = pts.iter().fold([f64::NEG_INFINITY; 3], |m, p| {
            [m[0].max(p.x), m[1].max(p.y), m[2].max(p.z)]
        });
        assert_eq!(e.min().to_array(), lo);
        assert_eq!(e.max().to_array(), hi);

        let (u0, u1) = (r.uniform(-10.0, 10.0), r.uniform(-10.0, 10.0));
        let (v0, v1) = (r.uniform(-10.0, 10.0), r.uniform(-10.0, 10.0));
        let b = Extent2d::from_points(Vec2::new(u0, v0), Vec2::new(u1, v1));
        let p = Vec2::new(r.uniform(-12.0, 12.0), r.uniform(-12.0, 12.0));
        let away = [u0, u1].iter().all(|s| (p.x - s).abs() > 1e-6)
            && [v0, v1].iter().all(|s| (p.y - s).abs() > 1e-6);
        if away {
            let inside =
                u0.min(u1) < p.x && p.x < u0.max(u1) && v0.min(v1) < p.y && p.y < v0.max(v1);
            assert_eq!(b.contains_point(p, 0.0), inside);
        }
    }
}

/// `zone_for_size` against the size table of `SmTol::SizeEstimateToZoneTol3d`.
#[test]
fn zone_follows_the_size_table() {
    let table = [
        (0.00005, 0.005, 1e-7),
        (0.005, 0.5, 1e-6),
        (0.5, 500.0, 1e-5),
        (500.0, 50000.0, 1e-4),
        (50000.0, 5e6, 1e-3),
    ];
    let mut r = Rng::new(8);
    for &(lo, hi, zone) in &table {
        for _ in 0..CASES / 5 {
            // Log-uniform strictly inside the range.
            let s = (lo as f64).ln() + r.uniform(0.0, 1.0) * (hi / lo).ln();
            let s = s.exp().clamp(lo * (1.0 + 1e-12), hi * (1.0 - 1e-12));
            assert_eq!(zone_for_size(s), zone, "{s}");
        }
    }
    // The ends, as the table's half-open ranges give them.
    assert_eq!(zone_for_size(0.00005), 1e-7);
    assert_eq!(zone_for_size(0.005), 1e-6);
    assert_eq!(zone_for_size(0.5), 1e-5);
    assert_eq!(zone_for_size(500.0), 1e-5);
    assert_eq!(zone_for_size(50000.0), 1e-4);
    assert_eq!(zone_for_size(5e6), 1e-3);
}

/// A unitized vector has length 1 to a few ulps and points the same way.
#[test]
fn unitize_gives_a_parallel_unit_vector() {
    let mut r = Rng::new(9);
    for _ in 0..CASES {
        let scale = 10f64.powf(r.uniform(-100.0, 100.0));
        let v = Vec3::new(r.uniform(-1.0, 1.0), r.uniform(-1.0, 1.0), r.uniform(-1.0, 1.0))
            * scale;
        let Ok(u) = v.unitize() else {
            continue;
        };
        let len = (u.x * u.x + u.y * u.y + u.z * u.z).sqrt();
        assert!((len - 1.0).abs() <= 4.0 * f64::EPSILON, "{v:?}");
        assert!(u.dot(v) > 0.0);
        let c = u.cross(v * (1.0 / scale));
        assert!(c.x.abs().max(c.y.abs()).max(c.z.abs()) <= 1e-14, "{v:?}");
    }
}

/// Solving a diagonally dominant system leaves a residual at rounding level, computed
/// here from the matrix entries directly.
#[test]
fn linear_solve_residual_is_small() {
    let mut r = Rng::new(10);
    for _ in 0..CASES / 4 {
        let n = 1 + (r.next_u64() % 6) as usize;
        let mut a = vec![0.0; n * n];
        for i in 0..n {
            let mut off = 0.0;
            for j in 0..n {
                if i != j {
                    a[i * n + j] = r.uniform(-1.0, 1.0);
                    off += a[i * n + j].abs();
                }
            }
            a[i * n + i] = (off + 1.0) * if r.next_u64() % 2 == 0 { 1.0 } else { -1.0 };
        }
        let b: Vec<f64> = (0..n).map(|_| r.uniform(-10.0, 10.0)).collect();
        let m = Matrix::from_row_major(n, n, a.clone()).unwrap();
        let x = m.solve_linear_system(&b).unwrap().x;
        for i in 0..n {
            let ax: f64 = (0..n).map(|j| a[i * n + j] * x[j]).sum();
            assert!((ax - b[i]).abs() <= 1e-12 * (1.0 + b[i].abs()), "{n} {i}");
        }
    }
}
