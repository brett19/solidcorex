// Written for solidcorex. Apache-2.0.
//! The certified projections (T1, T3) against closed forms (the circle, the sphere and the
//! cylinder limits), against independent methods (a dense scan of the ellipse's derivative,
//! Eberly's bisection, the double-double Newton feet of `analytic`, a sampled search), and
//! against what a stationary point is (a zero gradient, a Hessian of the kind's signs, the
//! torus's Morse count), with the degenerate queries one by one.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::analytic::{Circle, Cone, Ellipse, Frame, Torus, V3};
use scx_testkit::bound::{self, Bound};
use scx_testkit::dd::{self, Dd};
use scx_testkit::gen::{self, TorusKind};
use scx_testkit::project::{Enclosed, Kind, Locus, Nappes, Nearest, Projection, Uv};
use scx_testkit::Rat;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn norm(a: V3) -> f64 {
    dd::dot(&a, &a).sqrt().to_f64()
}

fn lift(v: [f64; 3]) -> V3 {
    dd::lift(v)
}

fn mid(b: &Bound) -> f64 {
    b.to_dd().to_f64()
}

/// An exactly orthonormal frame (a signed permutation) at a dyadic origin.
fn exact_frame(rng: &mut Rng) -> Frame {
    let o: [f64; 3] = core::array::from_fn(|_| (rng.next_signed_unit() * 64.0).round() / 64.0);
    let perms = [[0, 1, 2], [1, 2, 0], [2, 0, 1]];
    let p = perms[(rng.next_u64() % 3) as usize];
    let s = if rng.next_u64() & 1 == 0 { 1.0 } else { -1.0 };
    let e = |i: usize, k: f64| {
        let mut v = [0.0; 3];
        v[i] = k;
        v
    };
    // x, y, z = e_p0, s e_p1, s e_p2 is right-handed up to the permutation's parity; the
    // shapes don't care.
    Frame {
        origin: o,
        x: e(p[0], 1.0),
        y: e(p[1], s),
        z: e(p[2], s),
    }
}

fn nearest_distance<P>(p: &Projection<P>) -> f64 {
    mid(p.nearest_locus().unwrap().distance())
}

/// Every enclosure is narrow.
fn check_narrow<P>(p: &Projection<P>, tol: f64) {
    for l in &p.loci {
        let d = l.distance();
        assert!(
            d.radius() <= tol * (1.0 + mid(d)),
            "wide distance {:?}",
            d.to_f64s()
        );
    }
}

// ---------------------------------------------------------------- bound

#[test]
fn certified_sin_cos_and_tan_hold_the_double_double_values() {
    let mut rng = Rng::new(59);
    for _ in 0..200 {
        let x = 4.0 * rng.next_signed_unit();
        let (s, c) = bound::sin_cos(x, 120).unwrap();
        let (ds, dc) = dd::sin_cos(Dd::new(x));
        for (b, d) in [(&s, ds), (&c, dc)] {
            assert!(b.radius() < 1e-34, "{x}: radius {}", b.radius());
            assert!((mid(b) - d.hi).abs() <= 1e-15 * d.hi.abs().max(1e-300) + 1e-300);
            assert!(((b.to_dd() - d).to_f64()).abs() < 1e-30, "{x}");
        }
        // sin^2 + cos^2 = 1 inside the enclosures.
        let one = s.sqr().add(&c.sqr());
        assert!(one.contains(&Rat::one()));
    }
    // At the double below pi/4, tan x = 1 + 2 (x - pi/4) + O((x - pi/4)^2).
    let x = core::f64::consts::FRAC_PI_4;
    let t = bound::tan(x, 100).unwrap();
    let want = Dd::new(1.0) + (Dd::new(x) - dd::consts().pi * Dd::new(0.25)) * Dd::new(2.0);
    assert!(((t.to_dd() - want).to_f64()).abs() < 1e-31, "{t:?}");
    assert!(bound::tan(0.0, 100).is_none() && bound::tan(2.0, 100).is_none());
    let r = Bound::exact(Rat::from(2)).sqrt(100).unwrap();
    assert!(r.sqr().contains(&Rat::from(2)) && r.radius() < 1e-29);
    assert_eq!(
        Bound::exact(Rat::from(9)).sqrt(80).unwrap(),
        Bound::exact(Rat::from(3))
    );
}

// ---------------------------------------------------------------- T1: the ellipse

/// `f'(t) / 2 = (P - q) . P'`.
fn ellipse_slope(e: &Ellipse, q: V3, t: Dd) -> Dd {
    let d = e.derivs(t, 1);
    dd::dot(&sub(d[0], q), &d[1])
}

/// The stationary angles from a dense scan of the derivative's sign and bisection.
fn scan(e: &Ellipse, q: V3, n: usize) -> Vec<f64> {
    let pi = dd::consts().pi;
    let at = |i: usize| pi * (2.0 * i as f64 / n as f64 - 1.0);
    let mut out = Vec::new();
    for i in 0..n {
        let (mut a, mut b) = (at(i), at(i + 1));
        let (fa, fb) = (ellipse_slope(e, q, a), ellipse_slope(e, q, b));
        if fa.hi == 0.0 {
            out.push(a.hi);
            continue;
        }
        if (fa.hi > 0.0) == (fb.hi > 0.0) || fb.hi == 0.0 {
            continue;
        }
        for _ in 0..110 {
            let m = (a + b) * 0.5;
            if (ellipse_slope(e, q, m).hi > 0.0) == (fa.hi > 0.0) {
                a = m;
            } else {
                b = m;
            }
        }
        out.push(a.hi);
    }
    out
}

fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    d.min(2.0 * std::f64::consts::PI - d)
}

/// Eberly's bisection for the distance from `(y0, y1)` to the axis-aligned ellipse with
/// semi-axes `e0 >= e1` ("Distance from a point to an ellipse", 2013), in double-double.
fn eberly(e0: f64, e1: f64, y0: f64, y1: f64) -> f64 {
    let (e0, e1, y0, y1) = (
        Dd::new(e0),
        Dd::new(e1),
        Dd::new(y0.abs()),
        Dd::new(y1.abs()),
    );
    let one = Dd::new(1.0);
    let hyp = |a: Dd, b: Dd| (a * a + b * b).sqrt();
    if y1.hi > 0.0 {
        if y0.hi > 0.0 {
            let (z0, z1) = (y0 / e0, y1 / e1);
            let r0 = (e0 / e1) * (e0 / e1);
            let f = |s: Dd| {
                let (a, b) = (r0 * z0 / (s + r0), z1 / (s + one));
                a * a + b * b - one
            };
            let (mut lo, mut hi) = (z1 - one, hyp(r0 * z0, z1) - one);
            for _ in 0..220 {
                let m = (lo + hi) * 0.5;
                if f(m).hi > 0.0 {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            let s = (lo + hi) * 0.5;
            let (x0, x1) = (r0 * y0 / (s + r0), y1 / (s + one));
            return hyp(x0 - y0, x1 - y1).to_f64();
        }
        return (y1 - e1).abs().to_f64();
    }
    let k = e0 * e0 - e1 * e1;
    if (y0 * e0).hi < k.hi {
        let x0 = e0 * e0 * y0 / k;
        let x1 = e1 * (one - (x0 / e0) * (x0 / e0)).sqrt();
        return hyp(x0 - y0, x1).to_f64();
    }
    (y0 - e0).abs().to_f64()
}

fn check_ellipse_points(e: &Ellipse, q: [f64; 3], p: &Projection<Enclosed>) {
    for s in p.points() {
        let d = e.derivs(s.param.mid, 0)[0];
        assert!(
            norm(sub(d, s.point)) <= s.point_radius + 1e-28,
            "point off its parameter"
        );
        let dist = norm(sub(s.point, lift(q)));
        assert!(
            (dist - mid(&s.distance)).abs() <= s.point_radius + 1e-15 * dist,
            "distance {dist} vs {:?}",
            s.distance.to_f64s()
        );
        assert!(s.param.radius < 1e-25, "radius {}", s.param.radius);
    }
}

#[test]
fn ellipse_stationary_points_match_a_dense_scan() {
    let mut rng = Rng::new(1);
    for case in 0..24 {
        let e = gen::ellipse(&mut rng);
        let q = gen::query(&mut rng, &e.frame, 2.5);
        let p = e.stationary(q).unwrap();
        check_narrow(&p, 1e-28);
        check_ellipse_points(&e, q, &p);
        let found = scan(&e, lift(q), 2048);
        let truth: Vec<f64> = p.points().map(|s| s.param.mid.hi).collect();
        assert!(matches!(truth.len(), 2..=4), "case {case}: {truth:?}");
        for f in &found {
            assert!(
                truth.iter().any(|t| angle_gap(*t, *f) < 1e-14),
                "case {case}: scan root {f} not in {truth:?}"
            );
        }
        for t in &truth {
            let seen = found.iter().any(|f| angle_gap(*t, *f) < 1e-14);
            let paired = truth.iter().filter(|u| angle_gap(**u, *t) < 1e-2).count() > 1;
            assert!(
                seen || paired,
                "case {case}: {t} missed by the scan: {found:?}"
            );
        }
        // The minima and maxima alternate round the closed curve.
        let mins = p.points().filter(|s| s.kind == Kind::Min).count();
        let maxs = p.points().filter(|s| s.kind == Kind::Max).count();
        assert_eq!(mins, maxs, "case {case}");
        // The nearest is a minimum, and no sample is closer.
        let near = p.nearest_locus().unwrap().point().unwrap();
        assert_eq!(near.kind, Kind::Min);
        let dn = mid(&near.distance);
        for i in 0..512 {
            let t = -3.2 + 6.4 * f64::from(i) / 512.0;
            let pt = e.derivs(t, 0)[0];
            assert!(norm(sub(pt, lift(q))) >= dn * (1.0 - 1e-15));
        }
    }
}

#[test]
fn ellipse_nearest_matches_eberly() {
    let mut rng = Rng::new(2);
    for _ in 0..40 {
        let a = 0.25 + rng.next_f64();
        let b = a * (0.1 + 0.9 * rng.next_f64());
        let frame = Frame {
            origin: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            z: [0.0, 0.0, 1.0],
        };
        let e = Ellipse {
            frame,
            semi_x: a,
            semi_y: b,
        };
        let q = [
            2.0 * rng.next_signed_unit(),
            2.0 * rng.next_signed_unit(),
            0.0,
        ];
        let p = e.stationary(q).unwrap();
        let want = eberly(a, b, q[0], q[1]);
        let got = nearest_distance(&p);
        assert!(
            (got - want).abs() <= 1e-15 * want,
            "{got} vs {want} at {q:?}"
        );
        // and the double-double agreement, closer than a double can say
        let d = p.nearest_locus().unwrap().distance();
        let w = Bound::of_f64(want).unwrap();
        assert!(d.sub(&w).abs().hi().to_f64() <= 1e-14 * want);
    }
}

#[test]
fn ellipse_with_equal_axes_is_the_circle() {
    let mut rng = Rng::new(3);
    for _ in 0..20 {
        let frame = gen::frame(&mut rng, 1.0);
        let r = 0.25 + rng.next_f64();
        let q = gen::query(&mut rng, &frame, 2.0);
        let e = Ellipse {
            frame,
            semi_x: r,
            semi_y: r,
        };
        let c = Circle { frame, radius: r };
        let p = e.stationary(q).unwrap();
        // A query on the axis has no single foot on the circle; the exact case is below.
        let Some(foot) = c.closest(q) else { continue };
        let got = nearest_distance(&p);
        assert!(
            (got - foot.distance.to_f64()).abs() <= 1e-15 * (1.0 + got),
            "{got} vs {:?}",
            foot.distance
        );
        let near = p.nearest_locus().unwrap().point().unwrap();
        assert!((near.param.mid - foot.param).abs().to_f64() < 1e-20);
        assert_eq!(p.points().count(), 2);
    }
    // Exactly a circle and the query exactly on its axis: one circle of stationary points.
    let frame = exact_frame(&mut rng);
    let e = Ellipse {
        frame,
        semi_x: 0.75,
        semi_y: 0.75,
    };
    let q = core::array::from_fn(|i| frame.origin[i] + 0.5 * frame.z[i]);
    let p = e.stationary(q).unwrap();
    assert_eq!(p.nearest, Nearest::Unique(0));
    let Locus::Circle(c) = &p.loci[0] else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.distance.to_f64s(),
        (0.9013878188659973, 0.9013878188659974)
    );
    assert_eq!(c.radius, Bound::exact(Rat::from_f64(0.75).unwrap()));
}

#[test]
fn ellipse_degenerate_queries() {
    let frame = Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };
    let e = Ellipse {
        frame,
        semi_x: 2.0,
        semi_y: 1.0,
    };
    // At the centre: the four vertices, exactly; the two ends of the minor axis tie.
    let p = e.stationary([0.0; 3]).unwrap();
    assert_eq!(p.points().count(), 4);
    let Nearest::Tied(t) = &p.nearest else {
        panic!("{:?}", p.nearest)
    };
    assert_eq!(t.len(), 2);
    for i in t {
        let d = p.loci[*i].distance();
        assert!(d.contains(&Rat::one()) && d.radius() < 1e-30, "{d:?}");
        assert_eq!(p.loci[*i].kind(), Kind::Min);
    }
    // t = pi is found from the turned-over quartic, exactly.
    assert!(p
        .points()
        .any(|s| s.param.mid == dd::consts().pi && s.distance == Bound::exact(Rat::from(2))));
    // On the major axis inside the evolute's cusp (|x| < (a^2 - b^2) / a = 1.5): the
    // vertex (2, 0) is a local maximum between two symmetric minima off the axis.
    let p = e.stationary([0.5, 0.0, 0.0]).unwrap();
    let ks: Vec<Kind> = p.points().map(|s| s.kind).collect();
    assert_eq!(ks.iter().filter(|k| **k == Kind::Min).count(), 2, "{ks:?}");
    assert!(matches!(p.nearest, Nearest::Tied(_)));
    // At the evolute's cusp (a^2 - b^2) / a = 1.5: the vertex is a degenerate minimum, a
    // root of multiplicity 3.
    let p = e.stationary([1.5, 0.0, 0.0]).unwrap();
    assert_eq!(p.points().count(), 2, "{p:?}");
    let v = p.points().find(|s| s.param.mid.hi == 0.0).unwrap();
    assert_eq!(v.kind, Kind::Min);
    assert_eq!(v.distance, Bound::exact(Rat::from_f64(0.5).unwrap()));
    // Off the plane, on the normal through the centre.
    let p = e.stationary([0.0, 0.0, 3.0]).unwrap();
    assert_eq!(p.points().count(), 4);
    // Rejected input.
    assert!(Ellipse { semi_y: 0.0, ..e }.stationary([1.0; 3]).is_none());
    assert!(e.stationary([f64::NAN, 0.0, 0.0]).is_none());
    let flat = Ellipse {
        frame: Frame {
            y: [1.0, 0.0, 0.0],
            ..frame
        },
        ..e
    };
    assert!(flat.stationary([1.0; 3]).is_none());
}

#[test]
fn ellipse_near_the_evolute_stays_certified() {
    let mut rng = Rng::new(4);
    for case in 0..24 {
        let e = gen::ellipse(&mut rng);
        let gap = [1e-6, 1e-9, 1e-12, 1e-15][case % 4];
        let q = gen::ellipse_evolute(&mut rng, &e, gap, 0.0);
        let p = e.stationary(q).unwrap();
        check_ellipse_points(&e, q, &p);
        let mins = p.points().filter(|s| s.kind == Kind::Min).count();
        let maxs = p.points().filter(|s| s.kind == Kind::Max).count();
        assert_eq!(mins, maxs);
        let near = p.nearest_locus().unwrap().point().unwrap();
        let dn = mid(&near.distance);
        for i in 0..1024 {
            let t = -3.2 + 6.4 * f64::from(i) / 1024.0;
            let pt = e.derivs(t, 0)[0];
            assert!(norm(sub(pt, lift(q))) >= dn * (1.0 - 1e-15));
        }
    }
}

// ---------------------------------------------------------------- T3: shared checks

/// The gradient of `|S - q|^2 / 2` at the stationary parameter is zero, and the Hessian's
/// signs are the kind's (the parameterisation used as given: exact frames only).
fn check_surface_point(
    derivs: impl Fn(Dd, Dd) -> [V3; 6],
    q: [f64; 3],
    s: &scx_testkit::project::Stationary<Uv>,
    tol: f64,
) {
    let (u, v) = (s.param.0.mid, s.param.1.mid);
    let [p, su, sv, suu, suv, svv] = derivs(u, v);
    assert!(norm(sub(p, s.point)) <= s.point_radius + tol, "point off");
    if s.kind == Kind::Apex {
        return; // a singular point: the parameterisation's gradient there means nothing
    }
    let r = sub(p, lift(q));
    let scale = 1.0 + norm(r);
    let (gu, gv) = (dd::dot(&r, &su).to_f64(), dd::dot(&r, &sv).to_f64());
    assert!(
        gu.abs() <= tol * scale && gv.abs() <= tol * scale,
        "gradient {gu} {gv}"
    );
    if s.kind == Kind::Flat {
        return;
    }
    let a = (dd::dot(&suu, &r) + dd::dot(&su, &su)).to_f64();
    let b = (dd::dot(&suv, &r) + dd::dot(&su, &sv)).to_f64();
    let c = (dd::dot(&svv, &r) + dd::dot(&sv, &sv)).to_f64();
    if a.abs().min(c.abs()) <= tol * scale {
        return; // a Hessian at the rounding level (a query a hair off the axis) has no sign
    }
    let det = a * c - b * b;
    let tr = a + c;
    match s.kind {
        Kind::Min => assert!(det > 0.0 && tr > 0.0, "not a minimum: {a} {b} {c}"),
        Kind::Max => assert!(det > 0.0 && tr < 0.0, "not a maximum: {a} {b} {c}"),
        Kind::Saddle => assert!(det < 0.0, "not a saddle: {a} {b} {c}"),
        _ => {}
    }
}

fn surface_derivs(d: scx_testkit::Partials<V3>) -> [V3; 6] {
    let at = |i, j| *d.get(i, j).unwrap();
    [at(0, 0), at(1, 0), at(0, 1), at(2, 0), at(1, 1), at(0, 2)]
}

// ---------------------------------------------------------------- T3: the torus

#[test]
fn torus_stationary_points_are_stationary_with_their_kinds() {
    let mut rng = Rng::new(5);
    for kind in [TorusKind::Ring, TorusKind::Horn, TorusKind::Spindle] {
        for case in 0..16 {
            let frame = exact_frame(&mut rng);
            let t = Torus {
                frame,
                ..gen::torus(&mut rng, kind)
            };
            let q = gen::query(&mut rng, &frame, 3.0);
            let p = t.stationary(q).unwrap();
            check_narrow(&p, 1e-50);
            for s in p.points() {
                check_surface_point(|u, v| surface_derivs(t.derivs((u, v), 2)), q, s, 1e-27);
            }
            let off_axis = p.loci.iter().all(|l| l.point().is_some());
            if kind == TorusKind::Ring && off_axis {
                // Morse: #min - #saddle + #max = 0 on a (smooth) torus.
                assert_eq!(p.points().count(), 4);
                let n = |k| p.points().filter(|s| s.kind == k).count() as i64;
                assert_eq!(
                    n(Kind::Min) - n(Kind::Saddle) + n(Kind::Max),
                    0,
                    "{kind:?} {case}"
                );
            }
            // No sample is closer than the nearest.
            let dn = nearest_distance(&p);
            for i in 0..40 {
                for j in 0..40 {
                    let (u, v) = (-3.2 + 0.16 * f64::from(i), -3.2 + 0.16 * f64::from(j));
                    let pt = *t.derivs((u, v), 0).get(0, 0).unwrap();
                    assert!(norm(sub(pt, lift(q))) >= dn * (1.0 - 1e-15) - 1e-300);
                }
            }
            // The double-double Newton foot, where it has one, agrees.
            if let (Some(foot), Nearest::Unique(_)) = (t.closest(q), &p.nearest) {
                if kind == TorusKind::Ring {
                    assert!((foot.distance.to_f64() - dn).abs() <= 1e-15 * (1.0 + dn));
                }
            }
        }
    }
}

#[test]
fn torus_in_rotated_frames_agrees_to_rounding() {
    let mut rng = Rng::new(6);
    for _ in 0..24 {
        let t = gen::torus(&mut rng, TorusKind::Ring);
        let q = gen::query(&mut rng, &t.frame, 3.0);
        let p = t.stationary(q).unwrap();
        if let Some(foot) = t.closest(q) {
            let dn = nearest_distance(&p);
            assert!((foot.distance.to_f64() - dn).abs() <= 1e-13 * (1.0 + dn));
        }
        for s in p.points() {
            check_surface_point(|u, v| surface_derivs(t.derivs((u, v), 2)), q, s, 1e-13);
        }
    }
}

#[test]
fn torus_with_a_tiny_hole_is_the_sphere() {
    let mut rng = Rng::new(7);
    for _ in 0..20 {
        let frame = gen::frame(&mut rng, 1.0);
        let t = Torus {
            frame,
            major: dd::pow2(-30),
            minor: 1.0,
        };
        let q = gen::query(&mut rng, &frame, 3.0);
        let p = t.stationary(q).unwrap();
        let len = norm(sub(lift(q), lift(frame.origin)));
        assert!((nearest_distance(&p) - (len - 1.0).abs()).abs() <= 2.0 * dd::pow2(-30));
    }
}

#[test]
fn torus_degenerate_queries() {
    let frame = Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };
    let ring = Torus {
        frame,
        major: 2.0,
        minor: 0.5,
    };
    // On the axis: a near circle and a far circle.
    let p = ring.stationary([0.0, 0.0, 1.5]).unwrap();
    assert_eq!(p.loci.len(), 2);
    let Locus::Circle(near) = p.nearest_locus().unwrap() else {
        panic!()
    };
    assert_eq!(near.kind, Kind::Min);
    assert!((mid(&near.distance) - 2.0).abs() < 1e-30); // sqrt(1.5^2 + 2^2) - 0.5
                                                        // At the centre: the inner equator, at R - r.
    let p = ring.stationary([0.0; 3]).unwrap();
    assert!((nearest_distance(&p) - 1.5).abs() < 1e-30);
    // On the tube's centre circle: the meridian circle at distance r, the nearest.
    let p = ring.stationary([2.0, 0.0, 0.0]).unwrap();
    let Locus::Circle(c) = p.nearest_locus().unwrap() else {
        panic!("{p:?}")
    };
    assert_eq!(c.distance, Bound::exact(Rat::from_f64(0.5).unwrap()));
    assert_eq!(c.kind, Kind::Min);
    assert_eq!(p.loci.len(), 3);
    // Horn: the centre is a pinch, the query on it.
    let horn = Torus {
        frame,
        major: 1.0,
        minor: 1.0,
    };
    let p = horn.stationary([0.0; 3]).unwrap();
    let near = p.nearest_locus().unwrap().point().unwrap();
    assert_eq!(near.kind, Kind::Apex);
    assert_eq!(near.distance, Bound::exact(Rat::zero()));
    // Horn, the query in the hole's plane inside: the pinch is the nearest, listed once.
    let p = horn.stationary([0.25, 0.0, 0.0]).unwrap();
    let apexes = p.points().filter(|s| s.kind == Kind::Apex).count();
    assert_eq!(apexes, 1, "{p:?}");
    let near = p.nearest_locus().unwrap().point().unwrap();
    assert_eq!(near.kind, Kind::Apex);
    assert!((mid(&near.distance) - 0.25).abs() < 1e-30);
    // Spindle (R = 3/4, r = 5/4, pinches at h = +-1): on the axis at a pinch.
    let spindle = Torus {
        frame,
        major: 0.75,
        minor: 1.25,
    };
    let p = spindle.stationary([0.0, 0.0, 1.0]).unwrap();
    let near = p.nearest_locus().unwrap().point().unwrap();
    assert_eq!(
        (near.kind, near.distance.clone()),
        (Kind::Apex, Bound::exact(Rat::zero()))
    );
    assert_eq!(p.points().filter(|s| s.kind == Kind::Apex).count(), 2);
    // Spindle, on the tube's centre circle: the far side's near foot (|2R - r| = 1/4) is
    // nearer than the meridian circle and the pinches (both r = 5/4).
    let p = spindle.stationary([0.75, 0.0, 0.0]).unwrap();
    let near = p.nearest_locus().unwrap().point().unwrap();
    assert_eq!(
        near.distance,
        Bound::exact(Rat::from_f64(0.25).unwrap()),
        "{p:?}"
    );
    // Rejected input.
    assert!(Torus { minor: 0.0, ..ring }.stationary([1.0; 3]).is_none());
    assert!(Torus {
        major: -1.0,
        ..ring
    }
    .stationary([1.0; 3])
    .is_none());
}

// ---------------------------------------------------------------- T3: the cone

#[test]
fn cone_stationary_points_are_stationary_with_their_kinds() {
    let mut rng = Rng::new(8);
    for case in 0..40 {
        let frame = exact_frame(&mut rng);
        let c = Cone {
            frame,
            ..gen::cone(&mut rng)
        };
        let q = gen::query(&mut rng, &frame, 3.0);
        for nappes in [Nappes::One, Nappes::Both] {
            let p = c.stationary(q, nappes).unwrap();
            check_narrow(&p, 1e-50);
            for s in p.points() {
                if s.kind != Kind::Apex {
                    check_surface_point(|u, v| surface_derivs(c.derivs((u, v), 2)), q, s, 1e-27);
                }
            }
            let dn = nearest_distance(&p);
            let (sa, ca) = dd::sin_cos(Dd::new(c.semi_angle));
            let apex_v = -c.radius / (sa / ca).to_f64();
            for i in 0..40 {
                for j in 0..40 {
                    let u = -3.2 + 0.16 * f64::from(i);
                    let v = match nappes {
                        Nappes::One => apex_v + 0.1 * f64::from(j),
                        Nappes::Both => apex_v - 2.0 + 0.1 * f64::from(j),
                    };
                    let pt = *c.derivs((u, v), 0).get(0, 0).unwrap();
                    let d = norm(sub(pt, lift(q)));
                    assert!(
                        d >= dn * (1.0 - 1e-12) - 1e-12,
                        "case {case} {nappes:?}: {d} < {dn}"
                    );
                }
            }
            if nappes == Nappes::One {
                if let Some(foot) = c.closest(q) {
                    if p.nearest_locus().unwrap().kind() == Kind::Min {
                        assert!(
                            (foot.distance.to_f64() - dn).abs() <= 1e-15 * (1.0 + dn),
                            "case {case}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cone_with_a_tiny_angle_is_the_cylinder() {
    let mut rng = Rng::new(9);
    for _ in 0..20 {
        let frame = gen::frame(&mut rng, 1.0);
        let c = Cone {
            frame,
            radius: 1.0,
            semi_angle: dd::pow2(-30),
        };
        let q = gen::query(&mut rng, &frame, 3.0);
        let d = sub(lift(q), lift(frame.origin));
        // The frame's z is a unit vector only to rounding: measure along it, normalised.
        let z = lift(frame.z);
        let dz = dd::dot(&d, &z);
        let h = dz / dd::dot(&z, &z).sqrt();
        let rho2 = dd::dot(&d, &d) - dz * dz / dd::dot(&z, &z);
        let rho = if rho2.to_f64() > 0.0 {
            rho2.sqrt().to_f64()
        } else {
            0.0
        };
        let p = c.stationary(q, Nappes::One).unwrap();
        let got = nearest_distance(&p);
        // The cylinder, to the angle's order: within |h| a + a of |rho - 1|.
        let slack = (h.to_f64().abs() + 1.0) * dd::pow2(-30);
        assert!((got - (rho - 1.0).abs()).abs() <= slack, "{got} {rho}");
        // And to the next order: the radius at height h is 1 + h tan a, tan a = a + O(a^3).
        let sloped = (rho - 1.0 - h.to_f64() * dd::pow2(-30)).abs();
        assert!((got - sloped).abs() <= 1e-14, "{got} {sloped}");
    }
}

#[test]
fn cone_degenerate_queries() {
    let frame = Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };
    let quarter = core::f64::consts::FRAC_PI_4; // the double a hair below pi/4: tan a < 1
    let c = Cone {
        frame,
        radius: 0.0,
        semi_angle: quarter,
    };
    // The query is the apex.
    let p = c.stationary([0.0; 3], Nappes::One).unwrap();
    assert_eq!(p.loci.len(), 1);
    assert_eq!(p.loci[0].kind(), Kind::Apex);
    assert_eq!(*p.loci[0].distance(), Bound::exact(Rat::zero()));
    // On the axis above: a circle of feet at |h| sin a, and the apex.
    let p = c.stationary([0.0, 0.0, 2.0], Nappes::One).unwrap();
    let Locus::Circle(circle) = p.nearest_locus().unwrap() else {
        panic!("{p:?}")
    };
    assert!((mid(&circle.distance) - 2.0f64.sqrt()).abs() < 1e-15);
    // On the axis below: one nappe has only the apex; both have the circle too.
    let p = c.stationary([0.0, 0.0, -2.0], Nappes::One).unwrap();
    assert_eq!(p.loci.len(), 1);
    assert!((nearest_distance(&p) - 2.0).abs() < 1e-30);
    let p = c.stationary([0.0, 0.0, -2.0], Nappes::Both).unwrap();
    assert_eq!(p.loci.len(), 2);
    assert!(matches!(p.nearest_locus().unwrap(), Locus::Circle(_)));
    // Below the apex, off the axis, inside the dual cone: the apex is the nearest.
    let p = c.stationary([0.5, 0.0, -2.0], Nappes::One).unwrap();
    assert_eq!(p.nearest_locus().unwrap().kind(), Kind::Apex);
    // In the apex's plane with both nappes: the two feet tie, exactly.
    let p = c.stationary([1.0, 0.0, 0.0], Nappes::Both).unwrap();
    let Nearest::Tied(t) = &p.nearest else {
        panic!("{:?}", p.nearest)
    };
    assert_eq!(t.len(), 2);
    let p = c.stationary([1.0, 0.0, 0.0], Nappes::One).unwrap();
    assert!(matches!(p.nearest, Nearest::Unique(_)));
    // Rejected input.
    assert!(Cone { radius: -1.0, ..c }
        .stationary([1.0; 3], Nappes::One)
        .is_none());
    assert!(Cone {
        semi_angle: 0.0,
        ..c
    }
    .stationary([1.0; 3], Nappes::One)
    .is_none());
    assert!(Cone {
        semi_angle: 1.6,
        ..c
    }
    .stationary([1.0; 3], Nappes::One)
    .is_none());
}
