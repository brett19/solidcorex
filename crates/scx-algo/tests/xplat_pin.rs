// Written for solidcorex. Apache-2.0.
//! The exact result bits of a sample of scx-num and scx-algo, pinned on every platform
//! (docs/02-testing.md section 6, "across platforms": the golden table of 01 section 10.2
//! step 12, for what exists). Each group runs a seeded, fixed set of inputs through its
//! functions and hashes every output bit with [`scx_num::hash::mix`]; the hashes are
//! compared with the committed table. `fmath` itself is pinned the same way in
//! `scx-num/tests/fmath_pin.rs`. The values are the same on x86_64, aarch64 and wasm32, or
//! one of them has a determinism break.
//!
//! `SCX_PRINT_XPLAT_TABLE=1 cargo test -p scx-algo --test xplat_pin -- --nocapture` prints
//! the table.

#![allow(missing_docs, clippy::pedantic, clippy::indexing_slicing)]

use core::fmt;

use scx_algo::error::EvalError;
use scx_algo::local_solve_nd::{LocalSolveNd, Outcome as OutcomeNd, jacobian};
use scx_algo::local_solve1d::{Bracket3, Domain1d, Goal, LocalSolve1d, Sample};
use scx_algo::polynomial::{solve_cubic, solve_quadratic, solve_quartic};
use scx_num::extent1d::Extent1d;
use scx_num::extent3d::Extent3d;
use scx_num::fmath;
use scx_num::hash::{Rng, finalize};
use scx_num::math::{Affine, circumcentre_2d, three_point_circle_2d};
use scx_num::vector3d::{Point3, Vec3};

/// Folds words into one hash, in order.
struct Sink(u64);

impl Sink {
    fn new() -> Self {
        Self(0)
    }
    fn word(&mut self, w: u64) {
        self.0 = finalize((self.0 ^ w).wrapping_add(0x9E37_79B9_7F4A_7C15));
    }
    fn f(&mut self, x: f64) {
        self.word(x.to_bits());
    }
    fn fs(&mut self, xs: &[f64]) {
        self.word(xs.len() as u64);
        for &x in xs {
            self.f(x);
        }
    }
    fn p(&mut self, p: Point3) {
        self.fs(&[p.x, p.y, p.z]);
    }
    fn v(&mut self, v: Vec3) {
        self.fs(&[v.x, v.y, v.z]);
    }
    fn opt(&mut self, o: Option<&[f64]>) {
        match o {
            Some(xs) => {
                self.word(1);
                self.fs(xs);
            }
            None => self.word(0),
        }
    }
}

/// A value over several scales, both signs, now and then exactly zero or huge.
fn wide(r: &mut Rng) -> f64 {
    let m = r.next_signed_unit();
    match r.next_u64() % 16 {
        0 => 0.0,
        1 => m * 1e300,
        2 => m * 1e-300,
        k => m * fmath::powi(10.0, (k as i32) - 8),
    }
}

fn pt(r: &mut Rng) -> Point3 {
    Point3::new(
        r.next_signed_unit() * 50.0,
        r.next_signed_unit() * 50.0,
        r.next_signed_unit() * 50.0,
    )
}

fn vec(r: &mut Rng) -> Vec3 {
    Vec3::new(
        r.next_signed_unit(),
        r.next_signed_unit(),
        r.next_signed_unit(),
    )
}

/// Circles through three points: exact expansions, then `atan2`.
fn circles() -> u64 {
    let mut r = Rng::new(1);
    let mut s = Sink::new();
    for i in 0..600 {
        let mut c = [[0.0; 2]; 3];
        for p in &mut c {
            *p = if i % 3 == 0 {
                [wide(&mut r), wide(&mut r)]
            } else {
                [r.next_signed_unit() * 10.0, r.next_signed_unit() * 10.0]
            };
        }
        s.opt(circumcentre_2d(c[0], c[1], c[2]).as_ref().map(|a| &a[..]));
        match three_point_circle_2d(c[0], c[1], c[2]) {
            Some((m, rad, sweep)) => s.opt(Some(&[m[0], m[1], rad, sweep])),
            None => s.opt(None),
        }
    }
    // one at 1e300 beside two at 1e-300, and three nearly on a line
    for c in [
        [[1e300, 1e300], [1e-300, 0.0], [0.0, 1e-300]],
        [[0.0, 0.0], [1.0, 1.0 + 1e-9], [2.0, 2.0]],
        [[0.0, 0.0], [1.0, 1e-7], [2.0, 0.0]],
    ] {
        match three_point_circle_2d(c[0], c[1], c[2]) {
            Some((m, rad, sweep)) => s.opt(Some(&[m[0], m[1], rad, sweep])),
            None => s.opt(None),
        }
    }
    s.0
}

/// `Affine`: rotation (sin, cos), scaling, reflection, maps, inverse, boxes.
fn affine() -> u64 {
    let mut r = Rng::new(2);
    let mut s = Sink::new();
    let put = |s: &mut Sink, a: &Affine| {
        s.v(a.x_axis());
        s.v(a.y_axis());
        s.v(a.z_axis());
        s.v(a.translation_part());
    };
    for _ in 0..300 {
        let (c, ax, ang) = (pt(&mut r), vec(&mut r), r.next_signed_unit() * 7.0);
        let rot = Affine::rotation(c, ax, ang).ok();
        let sc = Affine::uniform_scaling(c, r.next_signed_unit() * 3.0).ok();
        let re = Affine::reflection(c, vec(&mut r)).ok();
        for a in [rot, sc, re].into_iter().flatten() {
            put(&mut s, &a);
            let p = pt(&mut r);
            s.p(a.apply(p));
            s.v(a.apply_vec(vec(&mut r)));
            let (lp, ld) = a.apply_line(p, vec(&mut r));
            s.p(lp);
            s.v(ld);
            s.word(a.is_rigid(1e-9).into());
            s.fs(&a.axis_lengths());
            match a.inverse() {
                Some(i) => {
                    s.word(1);
                    put(&mut s, &i);
                    s.p(i.apply(p));
                }
                None => s.word(0),
            }
            let b = Extent3d::hull([pt(&mut r), pt(&mut r), pt(&mut r)]);
            match b.and_then(|b| a.transformed_box(b)) {
                Some(t) => {
                    s.word(1);
                    s.p(t.mid());
                    s.v(t.size());
                }
                None => s.word(0),
            }
        }
        if let (Some(a), Some(b)) = (rot, re) {
            put(&mut s, &(a * b));
        }
    }
    s.0
}

/// 1D and 3D extents: hulls, unions, intersections, growth, distances, spheres.
fn extents() -> u64 {
    let mut r = Rng::new(3);
    let mut s = Sink::new();
    for _ in 0..400 {
        let (a, b, c, d) = (wide(&mut r), wide(&mut r), wide(&mut r), wide(&mut r));
        if let (Some(x), Some(y)) = (Extent1d::hull([a, b]), Extent1d::hull([c, d])) {
            let put = |s: &mut Sink, e: Option<Extent1d>| match e {
                Some(e) => s.fs(&[e.evaluate(0.0), e.evaluate(1.0)]),
                None => s.word(0),
            };
            put(&mut s, Some(x.union(y)));
            put(&mut s, x.intersect(y));
            put(&mut s, x.expanded(r.next_f64()));
            put(&mut s, x.expanded_relative(r.next_f64()));
            put(&mut s, x.shrunk(r.next_f64()));
            put(&mut s, x.scaled(r.next_signed_unit()));
            s.fs(&[x.mid(), x.length(), x.distance_from(c), x.clamp_value(d)]);
        }
        let pts = [pt(&mut r), pt(&mut r), pt(&mut r), pt(&mut r)];
        if let (Some(x), Some(y)) = (
            Extent3d::hull(pts[..2].iter().copied()),
            Extent3d::hull(pts[2..].iter().copied()),
        ) {
            let u = x.union(y);
            s.p(u.mid());
            s.v(u.size());
            if let Some(i) = x.intersect(y) {
                s.v(i.size());
            }
            s.fs(&[
                x.max_distance(&y),
                x.min_distance(&y),
                x.max_distance_to_point(pts[0]),
                x.max_length(),
            ]);
            let sb = x.sphere_bound();
            s.p(sb.center);
            s.fs(&[sb.radius, sb.min_radius]);
            if let Some(e) = x.expanded_relative(r.next_f64()) {
                s.v(e.size());
            }
            s.fs(&x.span_along(pts[1], vec(&mut r)));
            match x.cone_bound_to(&y) {
                Ok((axis, ang)) => {
                    s.v(axis);
                    s.f(ang);
                }
                Err(_) => s.word(0),
            }
        }
    }
    s.0
}

/// Closed-form quadratic, cubic and quartic roots of seeded coefficients.
fn roots() -> u64 {
    let mut r = Rng::new(4);
    let mut s = Sink::new();
    for _ in 0..800 {
        let c: [f64; 5] = core::array::from_fn(|_| {
            (r.next_signed_unit() * 8.0).round() / 2.0 + r.next_signed_unit() * 0.01
        });
        for tol in [1e-8, 1e-12] {
            s.fs(&solve_quadratic(&[c[0], c[1], c[2]], tol));
            s.fs(&solve_cubic(&[c[0], c[1], c[2], c[3]], tol));
            s.fs(&solve_quartic(&c, tol));
        }
    }
    // three equal and double roots, a vanishing leading term
    s.fs(&solve_cubic(&[-1.0, 3.0, -3.0, 1.0], 1e-12));
    s.fs(&solve_quartic(&[1.0, -4.0, 6.0, -4.0, 1.0], 1e-12));
    s.fs(&solve_quartic(&[-6.0, 11.0, -6.0, 1.0, 0.0], 1e-12));
    s.0
}

#[derive(Debug)]
struct Never;
impl fmt::Display for Never {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("never")
    }
}
impl EvalError for Never {
    fn ends_solve(&self) -> bool {
        false
    }
    fn passes_through(&self) -> bool {
        false
    }
    fn never_ignored(&self) -> bool {
        false
    }
}

/// The 1D local solvers (Newton, Brent, bisection) on transcendental functions.
fn solve1d() -> u64 {
    let mut r = Rng::new(5);
    let mut s = Sink::new();
    for k in 0..120 {
        let (a, b) = (1.0 + r.next_f64() * 3.0, r.next_signed_unit() * 2.0);
        let f = move |t: f64| -> Result<(f64, f64), Never> {
            Ok((fmath::cos(a * t) - b * t, -a * fmath::sin(a * t) - b))
        };
        let guess = r.next_signed_unit() * 2.0;
        match LocalSolve1d::new().solve(f, guess) {
            Ok(o) => {
                s.word(u64::from(o.is_root()));
                s.f(o.into_value());
            }
            Err(_) => s.word(2),
        }
        let g = move |t: f64| -> Result<(f64, f64), Never> {
            Ok((fmath::exp(t) - 3.0 * t, fmath::exp(t) - 3.0))
        };
        let (lo, hi) = (r.next_signed_unit() - 1.0, 1.0 + r.next_f64() * 2.0);
        let goal = if k % 2 == 0 {
            Goal::Minimize
        } else {
            Goal::Maximize
        };
        match LocalSolve1d::new().solve_by_brent(
            g,
            Bracket3 {
                a: lo,
                b: (lo + hi) / 2.0,
                c: hi,
            },
            1e-9,
            goal,
        ) {
            Ok(m) => s.fs(&[m.t, m.f]),
            Err(_) => s.word(2),
        }
        let h = move |t: f64| -> Result<f64, Never> { Ok(fmath::atan(t) - b / 3.0) };
        let (l, u) = (
            Sample {
                t: -5.0,
                f: fmath::atan(-5.0) - b / 3.0,
            },
            Sample {
                t: 5.0,
                f: fmath::atan(5.0) - b / 3.0,
            },
        );
        match LocalSolve1d::new().solve_by_bisection(h, l, u, 50) {
            Ok(m) => s.fs(&[m.t, m.f]),
            Err(_) => s.word(2),
        }
    }
    s.0
}

/// The N-dimensional Newton solver with a finite-difference and an exact Jacobian.
fn solve_nd() -> u64 {
    let mut r = Rng::new(6);
    let mut s = Sink::new();
    for _ in 0..120 {
        let (a, b) = (r.next_signed_unit() * 2.0, r.next_signed_unit() * 2.0);
        let eqs = move |x: &[f64; 2]| [fmath::sin(x[0]) + x[1] - a, x[0] - fmath::cos(x[1]) - b];
        let guess = [r.next_signed_unit(), r.next_signed_unit()];
        let exact = move |x: &[f64; 2]| -> Result<([f64; 2], [[f64; 2]; 2]), Never> {
            Ok((eqs(x), [[fmath::cos(x[0]), 1.0], [1.0, fmath::sin(x[1])]]))
        };
        let put = |s: &mut Sink, rep: Result<scx_algo::local_solve_nd::NdReport<2>, _>| match rep {
            Ok(rep) => {
                s.word(matches!(rep.outcome, OutcomeNd::Root(_)).into());
                s.fs(&rep.outcome.into_value());
                s.word(rep.iterations);
            }
            Err(_) => s.word(2),
        };
        put(&mut s, LocalSolveNd::<2>::new().solve(exact, guess));
        let dom = [Domain1d::Line; 2];
        let diff = |x: &[f64; 2]| -> Result<([f64; 2], [[f64; 2]; 2]), Never> {
            let j = jacobian(&mut |y: &[f64; 2]| Ok::<_, Never>(eqs(y)), *x, &dom)
                .unwrap_or([[0.0; 2]; 2]);
            Ok((eqs(x), j))
        };
        put(&mut s, LocalSolveNd::<2>::new().solve(diff, guess));
    }
    s.0
}

fn table() -> Vec<(&'static str, u64)> {
    vec![
        ("circles", circles()),
        ("affine", affine()),
        ("extents", extents()),
        ("roots", roots()),
        ("solve1d", solve1d()),
        ("solve_nd", solve_nd()),
    ]
}

/// The pinned hashes (the same on every platform).
const PINNED: &[(&str, u64)] = &[
    ("circles", 0x95647a90fd2227d8),
    ("affine", 0x49ab13e94a8125f9),
    ("extents", 0x0f7be59348ab390f),
    ("roots", 0x81ea06b587b1a7d6),
    ("solve1d", 0x31224adf78560453),
    ("solve_nd", 0x1ebe485c28af86e8),
];

#[test]
fn results_are_pinned_on_every_platform() {
    let got = table();
    if std::env::var_os("SCX_PRINT_XPLAT_TABLE").is_some() {
        for (name, h) in &got {
            println!("    (\"{name}\", 0x{h:016x}),");
        }
    }
    let differ: Vec<&str> = got
        .iter()
        .filter(|(n, h)| PINNED.iter().find(|(m, _)| m == n).map(|(_, p)| p) != Some(h))
        .map(|(n, _)| *n)
        .collect();
    assert!(
        differ.is_empty(),
        "these results differ from the pinned ones: {differ:?}"
    );
}
