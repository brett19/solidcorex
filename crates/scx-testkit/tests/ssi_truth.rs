// Written for solidcorex. Apache-2.0.
//! The closed-form SSI truths (research 73 SM0) against independent quadrature of their
//! defining integrals, against the surfaces' implicit equations, and against naive
//! parametrisations that share no code with the truths' (a plain square root over the
//! sampled angle, no analytic treatment of the ends).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use std::f64::consts::{PI, TAU};

use scx_num::fmath;
use scx_num::hash::Rng;
use scx_testkit::ssi_truth::{
    self as st, Extent, P3, Rigid, Truth, TruthCurve, bicylinder_area, bicylinder_volume,
    cylinders_parallel, cylinders_perpendicular, ellip_e, ellip_k, sphere_cylinder,
    sphere_lens_volume, sphere_sphere, sphere_union_area, t_union_volume, tiny_loop,
    torus_plane_parallel, torus_villarceau, viviani,
};

trait Pw {
    fn pw(self, n: i32) -> f64;
}
impl Pw for f64 {
    fn pw(self, n: i32) -> f64 {
        fmath::powi(self, n)
    }
}

// --- independent quadrature -----------------------------------------------------------------

/// Composite 24-point Gauss-Legendre (nodes by Newton on `P_24`).
fn gauss(f: impl Fn(f64) -> f64, a: f64, b: f64, panels: usize) -> f64 {
    const N: usize = 24;
    let mut nodes = [(0.0_f64, 0.0_f64); N];
    for (i, slot) in nodes.iter_mut().enumerate() {
        let mut x = fmath::cos(PI * (i as f64 + 0.75) / (N as f64 + 0.5));
        let mut dp = 1.0;
        for _ in 0..100 {
            let (mut p0, mut p1) = (1.0, x);
            for k in 2..=N {
                let kf = k as f64;
                (p0, p1) = (p1, ((2.0 * kf - 1.0) * x * p1 - (kf - 1.0) * p0) / kf);
            }
            dp = N as f64 * (x * p1 - p0) / (x * x - 1.0);
            let dx = p1 / dp;
            x -= dx;
            if dx.abs() < 1e-16 {
                break;
            }
        }
        *slot = (x, 2.0 / ((1.0 - x * x) * dp * dp));
    }
    let h = (b - a) / panels as f64;
    let mut sum = 0.0;
    for j in 0..panels {
        let mid = a + h * (j as f64 + 0.5);
        for &(x, w) in &nodes {
            sum += w * f(mid + 0.5 * h * x);
        }
    }
    sum * 0.5 * h
}

/// The closed polyline length of a loop's `n` chords.
fn chords(c: &TruthCurve, n: usize) -> f64 {
    let (a, b) = c.domain;
    let mut pts: Vec<P3> = (0..n)
        .map(|i| c.point(a + (b - a) * i as f64 / n as f64))
        .collect();
    pts.push(c.point(b));
    pts.windows(2).map(|w| st::dist(w[0], w[1])).sum()
}

/// Romberg extrapolation of the chord length (the error is a series in `h^2`).
fn polyline_length(c: &TruthCurve) -> f64 {
    let mut row: Vec<f64> = Vec::new();
    let mut n = 64;
    for _ in 0..6 {
        let l = chords(c, n);
        let mut new = vec![l];
        for (k, &prev) in row.iter().enumerate() {
            let f = 4.0_f64.pw(k as i32 + 1);
            new.push((f * new[k] - prev) / (f - 1.0));
        }
        row = new;
        n *= 2;
    }
    *row.last().unwrap()
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Everything a truth states, checked against the surfaces: residuals, closedness, lengths,
/// the node and touch points (on their parameters, on both surfaces, with parallel normals).
fn check(t: &Truth, tol: f64) {
    let name = &t.name;
    for (i, c) in t.curves.iter().enumerate() {
        for p in c.samples(257) {
            assert!(t.residual(p) <= tol, "{name} curve {i}: residual at {p:?}");
        }
        if c.extent == Extent::Loop {
            let (a, b) = c.domain;
            assert!(
                st::dist(c.point(a), c.point(b)) <= tol,
                "{name}: not closed"
            );
        }
        if let Some(l) = c.length {
            let q = polyline_length(c);
            assert!(
                (l - q).abs() <= 1e-9 * l,
                "{name} curve {i}: {l} against {q}"
            );
        }
    }
    for n in &t.nodes {
        assert!(t.residual(n.point) <= tol, "{name}: node off the surfaces");
        assert!(n.at.len() >= 2, "{name}: a node is visited at least twice");
        for &(i, p) in &n.at {
            let c = &t.curves[i];
            assert!(
                st::dist(c.point(p), n.point) <= tol,
                "{name}: node at {p} of curve {i}: {:?} against {:?}",
                c.point(p),
                n.point
            );
            assert!(
                p >= c.domain.0 && p < c.domain.1,
                "{name}: parameter {p} outside"
            );
        }
        tangent(t, n.point);
    }
    for &p in &t.touches {
        assert!(t.residual(p) <= tol, "{name}: touch off the surfaces");
        tangent(t, p);
    }
}

fn tangent(t: &Truth, p: P3) {
    let (na, nb) = (t.a.normal(p).unwrap(), t.b.normal(p).unwrap());
    let c = cross(na, nb);
    let s = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    assert!(s < 1e-7, "{}: normals cross {s} at {p:?}", t.name);
}

fn counts(t: &Truth) -> (usize, usize, usize) {
    (t.curves.len(), t.nodes.len(), t.touches.len())
}

// --- elliptic integrals and the bicylinder --------------------------------------------------

#[test]
fn elliptic_integrals_match_their_defining_integrals() {
    assert!((ellip_k(0.0) - PI / 2.0).abs() < 1e-15);
    assert!((ellip_e(0.0) - PI / 2.0).abs() < 1e-15);
    assert!((ellip_k(0.5) - 1.854_074_677_301_372).abs() < 1e-14);
    assert!((ellip_e(0.5) - 1.350_643_881_047_675_5).abs() < 1e-14);
    assert_eq!(ellip_e(1.0), 1.0);
    assert!(ellip_k(1.0).is_infinite() && ellip_k(1.5).is_nan());
    for m in [0.01, 0.25, 0.64, 0.9, 0.99] {
        let k = gauss(
            |u| 1.0 / (1.0 - m * fmath::sin(u).pw(2)).sqrt(),
            0.0,
            PI / 2.0,
            8,
        );
        let e = gauss(|u| (1.0 - m * fmath::sin(u).pw(2)).sqrt(), 0.0, PI / 2.0, 8);
        assert!((ellip_k(m) - k).abs() < 1e-13 * k, "K({m})");
        assert!((ellip_e(m) - e).abs() < 1e-13 * e, "E({m})");
    }
}

/// `V = int_{-r}^{r} 4 sqrt(r^2 - y^2) sqrt(R^2 - y^2) dy`, `y = r sin t`.
fn bicylinder_by_y(r: f64, big: f64) -> f64 {
    gauss(
        |t| {
            let (s, c) = fmath::sin_cos(t);
            4.0 * r * r * c * c * (big * big - r * r * s * s).sqrt()
        },
        -PI / 2.0,
        PI / 2.0,
        16,
    )
}

/// Slices across `x`: the section is `|y| <= w = sqrt(r^2 - x^2)` inside `y^2 + z^2 <= R^2`,
/// area `2 (w sqrt(R^2 - w^2) + R^2 asin(w / R))`; `x = r sin t`.
fn bicylinder_by_x(r: f64, big: f64) -> f64 {
    gauss(
        |t| {
            let c = fmath::cos(t);
            let w = r * c;
            2.0 * (w * (big * big - w * w).sqrt() + big * big * fmath::asin(w / big)) * r * c
        },
        -PI / 2.0,
        PI / 2.0,
        16,
    )
}

fn bicylinder_volume_and_area_body(full: bool) {
    for r in [1.0, 2.5, 7.0] {
        let v = bicylinder_volume(r, r).unwrap();
        assert!((v - 16.0 * r * r * r / 3.0).abs() <= 1e-15 * v);
        assert_eq!(bicylinder_area(r, r).unwrap(), 16.0 * r * r);
    }
    let v = bicylinder_volume(1.5, 2.0).unwrap();
    assert!((v - 26.1131656).abs() < 5e-8, "{v}");
    let pairs: &[(f64, f64)] = if full {
        &[(1.5, 2.0), (0.3, 1.0), (1.0, 1.01), (4.0, 9.0), (0.01, 3.0)]
    } else {
        &[(1.5, 2.0), (0.01, 3.0)]
    };
    for &(r, big) in pairs {
        let v = bicylinder_volume(r, big).unwrap();
        let (qy, qx) = (bicylinder_by_y(r, big), bicylinder_by_x(r, big));
        assert!(
            (v - qy).abs() <= 1e-12 * v,
            "{r} {big}: {v} against y-slices {qy}"
        );
        assert!(
            (v - qx).abs() <= 1e-12 * v,
            "{r} {big}: {v} against x-slices {qx}"
        );
        // The area: the r-cylinder's part (full height 2 sqrt(R^2 - y^2) round the circle),
        // plus the R-cylinder's two sides (width 2 sqrt(r^2 - y^2), arc element R dy / sqrt(R^2 - y^2)).
        let a1 = gauss(
            |t| 2.0 * r * (big * big - r * r * fmath::sin(t).pw(2)).sqrt(),
            0.0,
            TAU,
            16,
        );
        let a2 = gauss(
            |t| {
                let c = fmath::cos(t);
                let s = fmath::sin(t);
                4.0 * big * r * r * c * c / (big * big - r * r * s * s).sqrt()
            },
            -PI / 2.0,
            PI / 2.0,
            16,
        );
        let a = bicylinder_area(r, big).unwrap();
        assert!(
            (a - (a1 + a2)).abs() <= 1e-12 * a,
            "{r} {big}: area {a} against {}",
            a1 + a2
        );
    }
    // The limit r -> R of the elliptic forms is Steinmetz's.
    let near = bicylinder_volume(1.0 - 1e-9, 1.0).unwrap();
    assert!((near - 16.0 / 3.0).abs() < 1e-8);
    let near = bicylinder_area(1.0 - 1e-9, 1.0).unwrap();
    assert!((near - 16.0).abs() < 1e-6, "{near}");
    assert!(bicylinder_volume(2.0, 1.0).is_none() && bicylinder_volume(0.0, 1.0).is_none());
}

#[test]
fn tpipe_union_volume_is_rederived() {
    // The pin's tpipe: a 20-long pipe of radius 2 and a 10-long branch of radius 1.5 from its
    // axis; the branch's overlap with the pipe is half the bicylinder.
    let v = t_union_volume(2.0, 20.0, 1.5, 10.0).unwrap();
    assert!((v - 308.956_664_169_885).abs() < 1e-9, "{v}");
    // The union by direct quadrature: the pipe, plus the branch outside the pipe's wall,
    // `int_0^10 pi r^2 dy` less the part inside, `int 2 sqrt(r^2 - x^2) sqrt(R^2 - x^2) dx`.
    let inside = gauss(
        |t| {
            let (s, c) = fmath::sin_cos(t);
            2.0 * 1.5 * 1.5 * c * c * (4.0 - 1.5 * 1.5 * s * s).sqrt()
        },
        -PI / 2.0,
        PI / 2.0,
        16,
    );
    let direct = PI * 4.0 * 20.0 + PI * 1.5 * 1.5 * 10.0 - inside;
    assert!((v - direct).abs() < 1e-11, "{v} against {direct}");
    // The pin's own figure, 308.9567 (308.956697417 in the recipe table), is 3.3e-5 above the
    // truth: the pin's, not the truth's, error.
    assert!((v - 308.956_697_417).abs() < 4e-5);
    assert!(
        t_union_volume(2.0, 2.0, 1.5, 10.0).is_none()
            && t_union_volume(2.0, 20.0, 1.5, 1.0).is_none()
    );
}

#[test]
fn tpipe_loops_are_9_887974_long() {
    let t = cylinders_perpendicular(1.5, 2.0, 0.0).unwrap();
    check(&t, 1e-14);
    assert_eq!(counts(&t), (2, 0, 0));
    for c in &t.curves {
        let l = c.length.unwrap();
        assert!((l - 9.887974).abs() < 5e-7, "{l}");
        assert!((polyline_length(c) - l).abs() < 1e-11);
    }
}

// --- spheres ----------------------------------------------------------------------------------

#[test]
fn unit_spheres_meet_in_the_plane_x_half() {
    let t = sphere_sphere([0.0; 3], 1.0, [1.0, 0.0, 0.0], 1.0).unwrap();
    check(&t, 1e-15);
    assert_eq!(counts(&t), (1, 0, 0));
    let c = &t.curves[0];
    assert!((c.length.unwrap() - PI * 3.0_f64.sqrt()).abs() < 1e-15);
    for p in c.samples(64) {
        assert!((p[0] - 0.5).abs() < 1e-15);
        assert!((p[1] * p[1] + p[2] * p[2]).sqrt() - 3.0_f64.sqrt() / 2.0 < 1e-15);
    }
}

fn sphere_sphere_cases_body(full: bool) {
    let mut rng = Rng::new(0x55_a1);
    for _ in 0..(if full { 40 } else { 8 }) {
        let v = |rng: &mut Rng| {
            [
                rng.next_signed_unit(),
                rng.next_signed_unit(),
                rng.next_signed_unit(),
            ]
        };
        let (c1, c2) = (v(&mut rng), v(&mut rng));
        let d = st::dist(c1, c2);
        let (r1, r2) = (0.3 + rng.next_f64(), 0.3 + rng.next_f64());
        let Some(t) = sphere_sphere(c1, r1, c2, r2) else {
            panic!()
        };
        let meets = d < r1 + r2 && d > (r1 - r2).abs();
        assert_eq!(t.curves.len(), usize::from(meets));
        assert_eq!(t.is_empty(), !meets);
        check(&t, 1e-14);
        // The circle's radius against the sagitta of the lens: the circle's points are at r1
        // from c1 and r2 from c2 (the residual above), and the circle is planar and round.
        if meets {
            let (centre, rho, e1, e2) = st::sphere_sphere_frame(c1, r1, c2, r2).unwrap();
            let p = t.curves[0].point(0.7);
            let q = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
            let (a, b) = (
                q[0] * e1[0] + q[1] * e1[1] + q[2] * e1[2],
                q[0] * e2[0] + q[1] * e2[1] + q[2] * e2[2],
            );
            assert!(
                (a - rho * fmath::cos(0.7)).abs() < 1e-14
                    && (b - rho * fmath::sin(0.7)).abs() < 1e-14
            );
        }
    }
    // Touches and coincidence, exact in the arguments.
    let ext = sphere_sphere([0.0; 3], 1.0, [3.0, 0.0, 0.0], 2.0).unwrap();
    assert_eq!(ext.touches, vec![[1.0, 0.0, 0.0]]);
    check(&ext, 1e-15);
    let int = sphere_sphere([0.0; 3], 3.0, [1.0, 0.0, 0.0], 2.0).unwrap();
    assert_eq!(int.touches, vec![[3.0, 0.0, 0.0]]);
    check(&int, 1e-15);
    let int2 = sphere_sphere([0.0; 3], 2.0, [1.0, 0.0, 0.0], 3.0).unwrap();
    assert_eq!(int2.touches, vec![[-2.0, 0.0, 0.0]]);
    assert!(
        sphere_sphere([0.0; 3], 1.0, [0.0; 3], 1.0)
            .unwrap()
            .coincident
    );
    assert!(
        sphere_sphere([0.0; 3], 1.0, [0.0; 3], 2.0)
            .unwrap()
            .is_empty()
    );
    assert!(
        sphere_sphere([0.0; 3], 1.0, [5.0, 0.0, 0.0], 2.0)
            .unwrap()
            .is_empty()
    );
    assert!(
        sphere_sphere([0.0; 3], 5.0, [1.0, 0.0, 0.0], 2.0)
            .unwrap()
            .is_empty()
    );
    assert!(sphere_sphere([0.0; 3], 0.0, [1.0, 0.0, 0.0], 2.0).is_none());
}

/// The area of a sphere `ra` (centre at the origin) outside a sphere `rb` whose centre is
/// `d` away along the axis, by integrating `2 pi ra^2 sin(th)` over the polar angle `th` from
/// that axis from the boundary angle found by bisection on the other sphere's equation.
fn area_outside(ra: f64, rb: f64, d: f64) -> f64 {
    let outside = |th: f64| {
        let (dx, dy) = (ra * fmath::cos(th) - d, ra * fmath::sin(th));
        (dx * dx + dy * dy).sqrt() > rb
    };
    let (mut lo, mut hi) = (0.0, PI);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if outside(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    gauss(|th| TAU * ra * ra * fmath::sin(th), hi, PI, 4)
}

#[test]
fn sphere_lens_and_union() {
    for (r1, r2, d) in [
        (1.0, 1.0, 1.0),
        (2.0, 1.0, 2.5),
        (3.0, 1.5, 2.0),
        (1.0, 1.0, 1.99),
        (2.0, 1.0, 1.5),
    ] {
        // The lens by slices across the axis: the section is the smaller of the two discs.
        let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
        let v = gauss(|x| PI * (r1 * r1 - x * x), a, r1, 8)
            + gauss(|x| PI * (r2 * r2 - (x - d) * (x - d)), d - r2, a, 8);
        let lens = sphere_lens_volume(r1, r2, d);
        assert!((lens - v).abs() < 1e-12 * lens, "{lens} against {v}");
        let area = area_outside(r1, r2, d) + area_outside(r2, r1, d);
        let u = sphere_union_area(r1, r2, d);
        assert!((u - area).abs() < 1e-11 * u, "{u} against {area}");
    }
    // Apart, and one inside the other.
    assert_eq!(sphere_lens_volume(1.0, 1.0, 2.0), 0.0);
    assert_eq!(sphere_union_area(1.0, 1.0, 2.0), 8.0 * PI);
    assert!((sphere_lens_volume(3.0, 1.0, 1.0) - 4.0 * PI / 3.0).abs() < 1e-15);
    assert!((sphere_union_area(3.0, 1.0, 1.0) - 36.0 * PI).abs() < 1e-12);
}

#[test]
fn tiny_loops() {
    for delta in [1e-7, 1e-3, 0.5, 1.0, 1.9999] {
        let big = 1.0;
        let t = tiny_loop(big, delta).unwrap();
        check(&t, 3e-15);
        assert_eq!(counts(&t), (1, 0, 0));
        let rho = t.curves[0].point(0.0)[0];
        // The cap's area from the radius: pi (rho^2 + delta^2) = 2 pi R delta, the same cap
        // two ways.
        assert!(
            (PI * (rho * rho + delta * delta) - 2.0 * PI * big * delta).abs()
                < 1e-15 * delta.max(1e-7)
        );
        // The circle against the sphere's polar integral: the area above the plane is
        // `int 2 pi R^2 sin` from the angle `acos((R - delta) / R)`; the radius is `R sin` of it.
        let th = fmath::acos((big - delta) / big);
        let q = gauss(|u| TAU * big * big * fmath::sin(u), 0.0, th, 2);
        // acos near 1 loses digits (relative eps / theta^2): the check's, not the truth's.
        let tol = 1e-13 + 8e-16 / delta;
        assert!((q - 2.0 * PI * big * delta).abs() < tol * delta, "{q}");
        assert!((rho - big * fmath::sin(th)).abs() < tol * rho);
    }
    let t = tiny_loop(1.0, 1e-7).unwrap();
    assert!((t.curves[0].length.unwrap() - TAU * (1e-7_f64 * (2.0 - 1e-7)).sqrt()).abs() < 1e-18);
    assert_eq!(tiny_loop(2.0, 0.0).unwrap().touches, vec![[0.0, 0.0, 2.0]]);
    assert_eq!(tiny_loop(2.0, 4.0).unwrap().touches, vec![[0.0, 0.0, -2.0]]);
    assert!(tiny_loop(2.0, -1e-9).unwrap().is_empty() && tiny_loop(2.0, 4.1).unwrap().is_empty());
}

// --- naive references -----------------------------------------------------------------------

/// Runs of consecutive `Some` points.
fn runs(points: impl IntoIterator<Item = Option<P3>>) -> Vec<Vec<P3>> {
    let mut out = Vec::new();
    let mut cur: Vec<P3> = Vec::new();
    for p in points {
        match p {
            Some(p) => cur.push(p),
            None => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// A cylinder parametrised by the angle `t` whose points `(x(t), y(t))` meet a second
/// surface where `z^2 = g(t)`: the plain two-branch square root over a fine grid.
fn naive_angle(xy: impl Fn(f64) -> (f64, f64), g: impl Fn(f64) -> f64, n: usize) -> Vec<Vec<P3>> {
    let mut out = Vec::new();
    for s in [1.0, -1.0] {
        out.extend(runs((0..=n).map(|i| {
            let t = TAU * i as f64 / n as f64;
            let g = g(t);
            (g >= 0.0).then(|| {
                let (x, y) = xy(t);
                [x, y, s * g.sqrt()]
            })
        })));
    }
    out
}

/// The naive candidate against the truth: every candidate vertex on a truth curve to
/// `tight`, every truth sample within `loose` of the candidate's chords (the chord sag at a
/// square-root end).
fn compare(t: &Truth, naive: &[Vec<P3>], tight: f64, loose: f64, full: bool) {
    // The default run checks every 16th candidate vertex (the same `tight`) and 150 truth
    // samples; the full run every vertex and 600 samples.
    let (step, n) = if full { (1, 600) } else { (16, 150) };
    let thinned: Vec<Vec<P3>> = naive
        .iter()
        .map(|pl| pl.iter().copied().step_by(step).collect())
        .collect();
    let to = t.to_truth(&thinned);
    assert!(to <= tight, "{}: candidate off the truth by {to}", t.name);
    let from = t.from_truth(naive, n);
    assert!(
        from <= loose,
        "{}: truth off the candidate by {from}",
        t.name
    );
}

/// A second naive family, by the height `z`: `f` gives each branch's point or none.
fn naive_z(
    n: usize,
    lo: f64,
    hi: f64,
    branches: usize,
    f: impl Fn(f64) -> Vec<Option<P3>>,
) -> Vec<Vec<P3>> {
    let mut out = Vec::new();
    for b in 0..branches {
        out.extend(runs(
            (0..=n).map(|i| f(lo + (hi - lo) * i as f64 / n as f64)[b]),
        ));
    }
    out
}

/// The roots `+/- sqrt(a)` of `a^2 + b a + c = 0` for `a >= 0`, as `(x)` for both signs and
/// both roots (four, `None` where `a < 0`).
fn four_roots(b: f64, c: f64) -> [Option<f64>; 4] {
    let disc = (b * b - 4.0 * c).max(0.0).sqrt();
    let mut out = [None; 4];
    for (i, a) in [(-b + disc) / 2.0, (-b - disc) / 2.0]
        .into_iter()
        .enumerate()
    {
        if a >= 0.0 {
            out[2 * i] = Some(a.sqrt());
            out[2 * i + 1] = Some(-a.sqrt());
        }
    }
    out
}

// --- perpendicular cylinders ------------------------------------------------------------------

fn perpendicular_cylinders_by_offset_body(full: bool) {
    // (r, R, e, loops, nodes, touches)
    let cases = [
        (1.5, 2.0, 0.0, 2, 0, 0),
        (1.5, 2.0, 0.3, 2, 0, 0),
        (1.5, 2.0, 0.5, 1, 1, 0),
        (1.5, 2.0, 0.9, 1, 0, 0),
        (1.5, 2.0, 1.7, 1, 0, 0),
        (1.5, 2.0, 3.5, 0, 0, 1),
        (1.5, 2.0, 4.0, 0, 0, 0),
        (1.0, 1.0, 0.0, 2, 2, 0),
        (1.0, 1.0, 0.5, 1, 0, 0),
        (1.0, 1.0, 2.0, 0, 0, 1),
        (1.0, 3.0, 2.0, 1, 1, 0),
        (1.0, 3.0, 1.0, 2, 0, 0),
        (1.0, 3.0, 3.0, 1, 0, 0),
        (0.25, 8.0, 0.0, 2, 0, 0),
    ];
    for (r, big, e, loops, nodes, touches) in cases {
        let t = cylinders_perpendicular(r, big, e).unwrap();
        assert_eq!(
            (t.loops(), t.nodes.len(), t.touches.len()),
            (loops, nodes, touches),
            "r {r} R {big} e {e}"
        );
        assert_eq!(t.is_empty(), loops == 0 && touches == 0);
        check(&t, 1e-14 * big);
        let naive = naive_angle(
            |u| (r * fmath::cos(u), r * fmath::sin(u)),
            |u| big * big - (r * fmath::sin(u) - e).pw(2),
            1500,
        );
        if loops > 0 {
            let mut all = naive;
            all.extend(naive_z(1500, -big, big, 4, |z| {
                let mut v = Vec::new();
                for sg in [1.0, -1.0] {
                    let y = e + sg * (big * big - z * z).max(0.0).sqrt();
                    let x2 = r * r - y * y;
                    for sx in [1.0, -1.0] {
                        v.push((x2 >= 0.0 && z.abs() <= big).then(|| [sx * x2.sqrt(), y, z]));
                    }
                }
                v
            }));
            compare(&t, &all, 1e-9, 2e-3 * big, full);
        }
    }
    // One figure eight is one curve visited twice at its node.
    let eight = cylinders_perpendicular(1.5, 2.0, 0.5).unwrap();
    assert_eq!(eight.curves.len(), 1);
    assert_eq!(eight.nodes[0].at.len(), 2);
    assert_eq!(eight.nodes[0].point, [0.0, -1.5, 0.0]);
}

#[test]
fn perpendicular_cylinders_across_each_critical_offset() {
    // Just below and above e = R - r and e = R + r: loops 2 -> 1 -> 0 as the offset grows.
    let (r, big) = (1.5, 2.0);
    for (e, loops, touch) in [
        (0.5 - 1e-9, 2, false),
        (0.5 + 1e-9, 1, false),
        (3.5 - 1e-9, 1, false),
        (3.5 + 1e-9, 0, false),
        (3.5, 0, true),
    ] {
        let t = cylinders_perpendicular(r, big, e).unwrap();
        assert_eq!((t.loops(), !t.touches.is_empty()), (loops, touch), "e {e}");
        check(&t, 1e-14 * big);
    }
    assert!(cylinders_perpendicular(2.0, 1.5, 0.0).is_none());
    assert!(cylinders_perpendicular(1.0, 2.0, -0.1).is_none());
}

#[test]
fn steinmetz_is_two_ellipses_with_two_nodes() {
    for big in [1.0, 2.5] {
        let t = cylinders_perpendicular(big, big, 0.0).unwrap();
        for (i, s) in [1.0, -1.0].into_iter().enumerate() {
            let c = &t.curves[i];
            for p in c.samples(100) {
                assert!((p[2] - s * p[0]).abs() < 1e-15 * big, "the plane z = {s} x");
            }
            // The ellipse has semi-axes R and sqrt(2) R: its perimeter is 4 a E(1 - b^2/a^2),
            // here by the speed integral directly.
            let q = gauss(|u| big * (1.0 + fmath::sin(u).pw(2)).sqrt(), 0.0, TAU, 8);
            assert!(
                (c.length.unwrap() - q).abs() < 1e-12 * q,
                "{} {}",
                c.length.unwrap(),
                q
            );
        }
        assert_eq!(t.nodes.len(), 2);
        assert_eq!(t.nodes[0].point, [0.0, big, 0.0]);
        assert_eq!(t.nodes[1].point, [0.0, -big, 0.0]);
        check(&t, 1e-15 * big);
        let v = bicylinder_volume(big, big).unwrap();
        assert!((v - 16.0 * big.pw(3) / 3.0).abs() < 1e-14 * v);
    }
}

// --- parallel cylinders -----------------------------------------------------------------------

#[test]
fn parallel_cylinders() {
    // (r1, r2, e, lines, tangent, coincident)
    let cases = [
        (1.0, 1.0, 1.0, 2, false, false),
        (2.0, 1.0, 2.5, 2, false, false),
        (2.0, 1.0, 3.0, 1, true, false),
        (3.0, 1.0, 2.0, 1, true, false),
        (1.0, 3.0, 2.0, 1, true, false),
        (1.0, 1.0, 0.0, 0, false, true),
        (1.0, 2.0, 0.0, 0, false, false),
        (1.0, 1.0, 3.0, 0, false, false),
        (3.0, 1.0, 1.0, 0, false, false),
    ];
    for (r1, r2, e, lines, tangent, coincident) in cases {
        let t = cylinders_parallel(r1, r2, e, 5.0).unwrap();
        assert_eq!(t.lines(), lines, "{r1} {r2} {e}");
        assert_eq!((t.tangent, t.coincident), (tangent, coincident));
        assert_eq!(t.is_empty(), lines == 0 && !coincident);
        check(&t, 1e-15);
        for c in &t.curves {
            assert_eq!(c.extent, Extent::Line);
            // Along the axes.
            let (p, q) = (c.point(-5.0), c.point(5.0));
            assert_eq!((p[0], p[1], q[0], q[1]), (q[0], q[1], p[0], p[1]));
            let x = (e * e + r1 * r1 - r2 * r2) / (2.0 * e);
            assert_eq!(p[0], x);
        }
    }
    // Unit cylinders one apart: x = 1/2, y = +/- sqrt(3)/2.
    let t = cylinders_parallel(1.0, 1.0, 1.0, 1.0).unwrap();
    assert_eq!(t.curves[0].point(0.3), [0.5, 3.0_f64.sqrt() / 2.0, 0.3]);
}

// --- sphere and cylinder, Viviani -------------------------------------------------------------

#[test]
fn viviani_curve_and_its_node() {
    for a in [1.0, 1.7] {
        let t = viviani(a).unwrap();
        assert_eq!(counts(&t), (1, 1, 0));
        let c = &t.curves[0];
        assert_eq!(c.domain, (0.0, 2.0 * TAU));
        for k in 0..200 {
            let u = 4.0 * PI * k as f64 / 200.0;
            let want = [
                a * (1.0 + fmath::cos(u)),
                a * fmath::sin(u),
                2.0 * a * fmath::sin(0.5 * u),
            ];
            assert!(st::dist(c.point(u), want) < 1e-14 * a, "t = {u}");
        }
        assert_eq!(t.nodes[0].point, [2.0 * a, 0.0, 0.0]);
        assert_eq!(t.nodes[0].at, vec![(0, 0.0), (0, TAU)]);
        // Length 4 a int_0^{pi}... by the speed integral: |P'| = a sqrt(1 + cos^2(t/2)).
        let q = gauss(
            |u| a * (1.0 + fmath::cos(0.5 * u).pw(2)).sqrt(),
            0.0,
            4.0 * PI,
            16,
        );
        assert!(
            (c.length.unwrap() - q).abs() < 1e-12 * q,
            "{} {q}",
            c.length.unwrap()
        );
        check(&t, 1e-14 * a);
    }
}

fn sphere_cylinder_cases_body(full: bool) {
    // (rho, r, e, loops, nodes, touches)
    let cases = [
        (3.0, 1.0, 1.0, 2, 0, 0),
        (2.0, 1.0, 1.0, 1, 1, 0),
        (2.5, 1.0, 1.5, 1, 1, 0),
        (1.5, 1.0, 1.0, 1, 0, 0),
        (1.2, 1.0, 0.5, 1, 0, 0),
        (0.5, 1.0, 1.5, 0, 0, 1),
        (1.5, 1.0, 2.5, 0, 0, 1),
        (0.4, 1.0, 1.5, 0, 0, 0),
        (4.0, 1.0, 1.0, 2, 0, 0),
        (2.0, 1.0, 0.0, 2, 0, 0),
        (1.0, 1.0, 0.0, 1, 0, 0),
        (0.9, 1.0, 0.0, 0, 0, 0),
    ];
    for (rho, r, e, loops, nodes, touches) in cases {
        let t = sphere_cylinder(rho, r, e).unwrap();
        assert_eq!(
            (t.loops(), t.nodes.len(), t.touches.len()),
            (loops, nodes, touches),
            "rho {rho} r {r} e {e}"
        );
        check(&t, 1e-14 * rho);
        let naive = naive_angle(
            |u| (e + r * fmath::cos(u), r * fmath::sin(u)),
            |u| {
                let (x, y) = (e + r * fmath::cos(u), r * fmath::sin(u));
                rho * rho - x * x - y * y
            },
            1500,
        );
        if loops > 0 && e > 0.0 {
            let mut all = naive;
            all.extend(naive_z(1500, -rho, rho, 2, |z| {
                let sq = rho * rho - z * z;
                let x = (sq + e * e - r * r) / (2.0 * e);
                let y2 = sq - x * x;
                [1.0, -1.0]
                    .iter()
                    .map(|s| (y2 >= 0.0).then(|| [x, s * y2.sqrt(), z]))
                    .collect()
            }));
            compare(&t, &all, 1e-9, 2e-3 * rho, full);
        }
    }
    assert!(sphere_cylinder(1.0, 1.0, 0.0).unwrap().tangent);
    // Past the node on both sides: two loops for rho above r + e, one loop below.
    assert_eq!(sphere_cylinder(2.0 + 1e-9, 1.0, 1.0).unwrap().loops(), 2);
    assert_eq!(sphere_cylinder(2.0 - 1e-9, 1.0, 1.0).unwrap().loops(), 1);
    assert!(sphere_cylinder(1.0, 1.0, -1.0).is_none());
}

// --- torus and plane --------------------------------------------------------------------------

fn torus_plane_spiric_sections_body(full: bool) {
    let (big, small) = (3.0, 1.0);
    // (d, loops, nodes, touches)
    let cases = [
        (0.0, 2, 0, 0),
        (1.0, 2, 0, 0),
        (1.9, 2, 0, 0),
        (2.0, 1, 1, 0),
        (-2.0, 1, 1, 0),
        (2.1, 1, 0, 0),
        (3.0, 1, 0, 0),
        (3.9, 1, 0, 0),
        (4.0, 0, 0, 1),
        (-4.0, 0, 0, 1),
        (4.1, 0, 0, 0),
    ];
    for (d, loops, nodes, touches) in cases {
        let t = torus_plane_parallel(big, small, d).unwrap();
        assert_eq!(
            (t.loops(), t.nodes.len(), t.touches.len()),
            (loops, nodes, touches),
            "d {d}"
        );
        check(&t, 1e-14 * big);
        // The quartic's own roots in z at each x: no tube circle in sight.
        let runs_all: Vec<Vec<P3>> = {
            let mut out = Vec::new();
            for root in [1.0, -1.0] {
                for s in [1.0, -1.0] {
                    out.extend(runs((0..=1500).map(|i| {
                        let x = -(big + small) + 2.0 * (big + small) * i as f64 / 1500.0;
                        let q = (x * x + d * d).sqrt();
                        let z2 = root * 2.0 * big * q - (x * x + d * d + big * big - small * small);
                        (z2 >= 0.0).then(|| [x, d, s * z2.sqrt()])
                    })));
                }
            }
            out
        };
        if loops > 0 {
            let mut all = runs_all;
            all.extend(naive_z(1500, -small, small, 4, |z| {
                let w = d * d + z * z + big * big - small * small;
                four_roots(2.0 * w - 4.0 * big * big, w * w - 4.0 * big * big * d * d)
                    .iter()
                    .map(|x| x.map(|x| [x, d, z]))
                    .collect()
            }));
            compare(&t, &all, 1e-6, 4e-3, full);
        }
    }
    // d = 0 is two tube circles.
    let t = torus_plane_parallel(big, small, 0.0).unwrap();
    for (c, s) in t.curves.iter().zip([1.0, -1.0]) {
        assert!((c.length.unwrap() - TAU * small).abs() < 1e-15);
        for p in c.samples(50) {
            let centre = [s * big, 0.0, 0.0];
            assert!((st::dist(p, centre) - small).abs() < 1e-14);
        }
    }
    // The figure eight at R - r: node at (0, d, 0), visited at phi = pi and 3 pi.
    let t = torus_plane_parallel(big, small, 2.0).unwrap();
    assert_eq!(t.nodes[0].point, [0.0, 2.0, 0.0]);
    assert_eq!(t.nodes[0].at, vec![(0, PI), (0, 3.0 * PI)]);
    assert!(
        torus_plane_parallel(1.0, 1.0, 0.5).is_none()
            && torus_plane_parallel(3.0, 1.0, f64::NAN).is_none()
    );
    // Across the critical offsets.
    for (d, loops) in [
        (2.0 - 1e-9, 2),
        (2.0 + 1e-9, 1),
        (4.0 - 1e-9, 1),
        (4.0 + 1e-9, 0),
    ] {
        let t = torus_plane_parallel(big, small, d).unwrap();
        assert_eq!(t.loops(), loops, "d {d}");
        check(&t, 1e-14 * big);
    }
}

fn villarceau_circles_body(full: bool) {
    for (big, small) in [(3.0, 1.0), (2.0, 1.5), (10.0, 0.1)] {
        let t = torus_villarceau(big, small).unwrap();
        assert_eq!(counts(&t), (2, 2, 0));
        check(&t, 1e-14 * big);
        for (c, s) in t.curves.iter().zip([1.0, -1.0]) {
            assert!((c.length.unwrap() - TAU * big).abs() < 1e-14 * big);
            let centre = [s * small, 0.0, 0.0];
            for p in c.samples(100) {
                assert!(
                    (st::dist(p, centre) - big).abs() < 1e-14 * big,
                    "radius R about (+/- r, 0, 0)"
                );
            }
        }
        // The plane's tilt: sin(alpha) = r / R from the horizontal.
        let n = t.b.normal([0.0; 3]).unwrap();
        assert!((n[2] - (1.0 - (small / big).pw(2)).sqrt()).abs() < 1e-15);
        // The nodes are the plane's two points of tangency with the torus.
        let cos2 = 1.0 - (small / big).pw(2);
        assert!((t.nodes[0].point[1] - big * cos2).abs() < 1e-14 * big);
        assert_eq!(
            t.nodes[1].point,
            [0.0, -t.nodes[0].point[1], -t.nodes[0].point[2]]
        );
        // The naive section: in plane coordinates (x, u) the quartic is a quadratic in u^2.
        let (k, c2) = (big * big - small * small, cos2);
        let mut naive = Vec::new();
        for root in [1.0, -1.0] {
            for s in [1.0, -1.0] {
                naive.extend(runs((0..=1500).map(|i| {
                    let x = -(big + small) + 2.0 * (big + small) * i as f64 / 1500.0;
                    let b = 2.0 * (x * x + k) - 4.0 * big * big * c2;
                    let c = (x * x + k).pw(2) - 4.0 * big * big * x * x;
                    let disc = b * b - 4.0 * c;
                    let u2 = (-b + root * disc.max(0.0).sqrt()) / 2.0;
                    (disc > -1e-9 && u2 >= 0.0).then(|| {
                        let u = s * u2.sqrt();
                        [x, u * c2.sqrt(), u * (small / big)]
                    })
                })));
            }
        }
        let mut all = naive;
        all.extend(naive_z(1500, -big, big, 4, |u| {
            let q = u * u + k;
            four_roots(
                2.0 * q - 4.0 * big * big,
                q * q - 4.0 * big * big * u * u * c2,
            )
            .iter()
            .map(|x| x.map(|x| [x, u * c2.sqrt(), u * (small / big)]))
            .collect()
        }));
        compare(&t, &all, 1e-5 * big, 4e-3 * big, full);
    }
    assert!(torus_villarceau(1.0, 1.0).is_none());
}

// --- placement and the Hausdorff measure ------------------------------------------------------

fn placement_keeps_every_claim_body(full: bool) {
    let mut rng = Rng::new(0x73_01);
    let truths = [
        cylinders_perpendicular(1.0, 1.0, 0.0).unwrap(),
        cylinders_perpendicular(1.5, 2.0, 0.5).unwrap(),
        viviani(1.3).unwrap(),
        torus_plane_parallel(3.0, 1.0, 2.0).unwrap(),
        torus_villarceau(3.0, 1.0).unwrap(),
        cylinders_parallel(1.0, 1.0, 1.0, 3.0).unwrap(),
        sphere_cylinder(2.0, 1.0, 0.0).unwrap(),
        tiny_loop(1.0, 1e-3).unwrap(),
    ];
    let used = if full { truths.len() } else { 3 };
    for t in &truths[..used] {
        let m = Rigid::turn(
            [
                rng.next_signed_unit(),
                rng.next_signed_unit(),
                rng.next_signed_unit() + 2.0,
            ],
            rng.next_signed_unit() * PI,
            [
                5.0 * rng.next_signed_unit(),
                5.0 * rng.next_signed_unit(),
                5.0 * rng.next_signed_unit(),
            ],
        );
        let moved = t.placed(&m);
        check(&moved, 1e-13);
        assert_eq!(counts(&moved), counts(t));
        for (a, b) in t.curves.iter().zip(&moved.curves) {
            assert_eq!(a.length, b.length);
            for p in a.samples(20) {
                let q = m.point(p);
                assert!(moved.residual(q) < 1e-13);
            }
            let (pa, pb) = (a.samples(30), b.samples(30));
            for i in 0..29 {
                let (da, db) = (st::dist(pa[i], pa[i + 1]), st::dist(pb[i], pb[i + 1]));
                assert!((da - db).abs() < 1e-13, "a rigid motion keeps distances");
            }
        }
    }
}

#[test]
fn hausdorff_measures_both_directions() {
    let t = sphere_sphere([0.0; 3], 1.0, [1.0, 0.0, 0.0], 1.0).unwrap();
    let c = &t.curves[0];
    let n = 256;
    let mut ring = c.samples(n);
    ring.push(ring[0]);
    let h = t.hausdorff(&[ring.clone()], 1000);
    let rho = 3.0_f64.sqrt() / 2.0;
    assert!(h.to_truth < 1e-15);
    // The chord's sag: rho (1 - cos(pi / n)).
    let sag = rho * (1.0 - fmath::cos(PI / n as f64));
    assert!(
        h.from_truth <= sag * (1.0 + 1e-9) && h.from_truth > 0.9 * sag,
        "{} {sag}",
        h.from_truth
    );
    // A candidate 1e-3 off in x is 1e-3 from the truth, both ways.
    let shifted: Vec<P3> = ring.iter().map(|p| [p[0] + 1e-3, p[1], p[2]]).collect();
    let h = t.hausdorff(&[shifted], 1000);
    assert!((h.to_truth - 1e-3).abs() < 1e-9 && (h.from_truth - 1e-3).abs() < 1e-4);
    // A candidate missing half the loop is far in the other direction only.
    let half: Vec<P3> = ring[..n / 2].to_vec();
    let h = t.hausdorff(&[half], 1000);
    assert!(h.to_truth < 1e-15 && h.from_truth > 0.5);
    // Nearest point and parameter.
    let p = c.point(1.234);
    let (u, d) = c.nearest(p);
    assert!((u - 1.234).abs() < 1e-9 && d < 1e-15);
}

#[test]
fn perpendicular_cylinders_by_offset() {
    perpendicular_cylinders_by_offset_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn perpendicular_cylinders_by_offset_full() {
    perpendicular_cylinders_by_offset_body(true);
}

#[test]
fn sphere_cylinder_cases() {
    sphere_cylinder_cases_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn sphere_cylinder_cases_full() {
    sphere_cylinder_cases_body(true);
}

#[test]
fn torus_plane_spiric_sections() {
    torus_plane_spiric_sections_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn torus_plane_spiric_sections_full() {
    torus_plane_spiric_sections_body(true);
}

#[test]
fn villarceau_circles() {
    villarceau_circles_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn villarceau_circles_full() {
    villarceau_circles_body(true);
}

#[test]
fn bicylinder_volume_and_area() {
    bicylinder_volume_and_area_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn bicylinder_volume_and_area_full() {
    bicylinder_volume_and_area_body(true);
}

#[test]
fn sphere_sphere_cases() {
    sphere_sphere_cases_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn sphere_sphere_cases_full() {
    sphere_sphere_cases_body(true);
}

#[test]
fn placement_keeps_every_claim() {
    placement_keeps_every_claim_body(false);
}

#[test]
#[ignore = "slow exhaustive sweep"]
fn placement_keeps_every_claim_full() {
    placement_keeps_every_claim_body(true);
}
