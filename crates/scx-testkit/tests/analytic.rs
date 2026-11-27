// Written for solidcorex. Apache-2.0.
//! The analytic references against their identities (a circle's second derivative is the
//! way back to its centre, a sphere's points lie at its radius), against the exact line
//! and plane, and their closest points against the orthogonality that defines them and
//! against a sampled search.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp
)]

use scx_num::hash::Rng;
use scx_testkit::Rat;
use scx_testkit::analytic::{
    Circle, Cone, Cylinder, Ellipse, Foot, Frame, Hyperbola, Line, Parabola, Plane, Sphere, Torus,
    V3,
};
use scx_testkit::dd::{self, Dd};
use scx_testkit::sample;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn norm(a: V3) -> f64 {
    dd::dot(&a, &a).sqrt().to_f64()
}

fn lift(v: [f64; 3]) -> V3 {
    dd::lift(v)
}

/// `|a - b| <= tol (1 + |b|)`.
fn near(a: V3, b: V3, tol: f64) -> bool {
    norm(sub(a, b)) <= tol * (1.0 + norm(b))
}

const TOL: f64 = 1e-28;

/// `cos u X + sin u Y` times `f`.
fn radial(frame: &Frame, u: f64, f: Dd) -> V3 {
    let (s, c) = dd::sin_cos(Dd::new(u));
    add(
        lift(frame.x).map(|x| x * c * f),
        lift(frame.y).map(|y| y * s * f),
    )
}

#[test]
fn curve_derivatives_satisfy_their_identities() {
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let frame = sample::frame(&mut rng, 2.0);
        let t = 8.0 * rng.next_signed_unit();
        let o = lift(frame.origin);
        let c = Circle {
            frame,
            radius: 0.1 + rng.next_f64(),
        };
        let d = c.derivs(t, 4);
        // C'' = -(C - O), C''' = -C', C'''' = C - O.
        assert!(near(d[2], sub(o, d[0]), TOL));
        assert!(near(d[3], d[1].map(|x| -x), TOL));
        assert!(near(d[4], sub(d[0], o), TOL));
        assert!(near(
            d[0],
            add(o, radial(&frame, t, Dd::new(c.radius))),
            TOL
        ));
        let e = Ellipse {
            frame,
            semi_x: 0.5 + rng.next_f64(),
            semi_y: 0.1 + rng.next_f64(),
        };
        let d = e.derivs(t, 2);
        assert!(near(d[2], sub(o, d[0]), TOL));
        // The dd point against the double one (fmath's sin and cos): an ulp or two.
        let (s, co) = scx_num::fmath::sin_cos(t);
        let f: [f64; 3] = core::array::from_fn(|k| {
            frame.origin[k] + e.semi_x * co * frame.x[k] + e.semi_y * s * frame.y[k]
        });
        assert!(norm(sub(d[0], lift(f))) < 1e-14, "{t}");
        let line = Line {
            origin: frame.origin,
            direction: frame.z,
        };
        let d = line.derivs(t, 2);
        let exact = line.point_exact(t).unwrap();
        let ex: V3 = core::array::from_fn(|k| {
            let rest = &exact.exact[k]
                - &(&Rat::from_f64(d[0][k].hi).unwrap() + &Rat::from_f64(d[0][k].lo).unwrap());
            Dd::new(rest.to_f64())
        });
        assert!(norm(ex) <= 1e-30 * (1.0 + norm(d[0])));
        assert_eq!(d[2], [Dd::new(0.0); 3]);
    }
}

#[test]
fn surface_derivatives_satisfy_their_identities() {
    let mut rng = Rng::new(2);
    for _ in 0..200 {
        let frame = sample::frame(&mut rng, 2.0);
        let (u, v) = (6.0 * rng.next_signed_unit(), 1.4 * rng.next_signed_unit());
        let o = lift(frame.origin);
        let z = lift(frame.z);
        let r = 0.1 + rng.next_f64();
        // Sphere: S_vv = -(S - O), S_uu = -(radial part), the radius from the centre.
        let s = Sphere { frame, radius: r }.derivs((u, v), 2);
        let at = |i, j| *s.get(i, j).unwrap();
        assert!(near(at(0, 2), sub(o, at(0, 0)), TOL));
        let (sv, cv) = dd::sin_cos(Dd::new(v));
        assert!(near(at(2, 0), radial(&frame, u, -(cv * r)), TOL));
        let want = add(add(o, radial(&frame, u, cv * r)), z.map(|x| x * sv * r));
        assert!(near(at(0, 0), want, TOL));
        // Cylinder: S_v = Z, S_uu = -(S - O - v Z), the mixed and v-second partials zero.
        let c = Cylinder { frame, radius: r }.derivs((u, v), 2);
        let at = |i, j| *c.get(i, j).unwrap();
        assert_eq!(at(0, 1), z);
        assert_eq!(at(1, 1), [Dd::new(0.0); 3]);
        assert!(near(at(2, 0), sub(add(o, z.map(|x| x * v)), at(0, 0)), TOL));
        // Cone: S_v = tan a radial(u) + Z; the v-second partial is zero.
        let a = 0.6 * rng.next_signed_unit();
        let k = Cone {
            frame,
            radius: r,
            semi_angle: a,
        }
        .derivs((u, v), 2);
        let (sa, ca) = dd::sin_cos(Dd::new(a));
        assert!(near(
            *k.get(0, 1).unwrap(),
            add(radial(&frame, u, sa / ca), z),
            TOL
        ));
        assert_eq!(*k.get(0, 2).unwrap(), [Dd::new(0.0); 3]);
        // Torus: S_vv = -(S - O - R radial(u)).
        let big = 1.0 + rng.next_f64();
        let t = Torus {
            frame,
            major: big,
            minor: r,
        }
        .derivs((u, v), 2);
        let centre = add(o, radial(&frame, u, Dd::new(big)));
        assert!(near(
            *t.get(0, 2).unwrap(),
            sub(centre, *t.get(0, 0).unwrap()),
            TOL
        ));
        // Plane: exact against double-double.
        let p = Plane { frame };
        let d = p.derivs((u, v), 2);
        assert_eq!(*d.get(1, 0).unwrap(), lift(frame.x));
        let exact = p.point_exact((u, v)).unwrap();
        assert!(near(lift(exact.rounded), *d.get(0, 0).unwrap(), 1e-16));
    }
}

/// The foot's residual is orthogonal to the tangents, and no sampled parameter is closer.
fn check_curve_foot(foot: &Foot<Dd>, q: [f64; 3], tangent: V3, sample: impl Fn(f64) -> V3) {
    let r = sub(lift(q), foot.point);
    let cos = dd::dot(&r, &tangent).to_f64().abs() / (norm(r) * norm(tangent)).max(1e-300);
    assert!(cos < 1e-26 || norm(r) < 1e-28, "residual not normal: {cos}");
    for i in 0..256 {
        let t = -4.0 + 8.0 * f64::from(i) / 256.0;
        assert!(norm(sub(sample(t), lift(q))) >= foot.distance.to_f64() * (1.0 - 1e-15));
    }
}

fn check_surface_foot(
    foot: &Foot<(Dd, Dd)>,
    q: [f64; 3],
    tangents: [V3; 2],
    sample: impl Fn(f64, f64) -> V3,
    vs: (f64, f64),
) {
    let r = sub(lift(q), foot.point);
    for t in tangents {
        let cos = dd::dot(&r, &t).to_f64().abs() / (norm(r) * norm(t)).max(1e-300);
        assert!(cos < 1e-26 || norm(r) < 1e-28, "residual not normal: {cos}");
    }
    for i in 0..48 {
        for j in 0..48 {
            let u = -3.2 + 6.4 * f64::from(i) / 48.0;
            let v = vs.0 + (vs.1 - vs.0) * f64::from(j) / 47.0;
            let d = norm(sub(sample(u, v), lift(q)));
            assert!(
                d >= foot.distance.to_f64() * (1.0 - 1e-12),
                "({u}, {v}) closer: {d} < {:?}",
                foot.distance
            );
        }
    }
}

#[test]
fn closest_points_are_normal_feet_and_nearest() {
    let mut rng = Rng::new(3);
    for _ in 0..40 {
        let frame = sample::frame(&mut rng, 1.0);
        let q: [f64; 3] = core::array::from_fn(|_| 3.0 * rng.next_signed_unit());
        let r = 0.2 + rng.next_f64();
        let circle = Circle { frame, radius: r };
        let foot = circle.closest(q).unwrap();
        let tangent = circle.derivs(foot.param, 1)[1];
        check_curve_foot(&foot, q, tangent, |t| circle.derivs(t, 0)[0]);

        let tangents =
            |d: &scx_testkit::Partials<V3>| [*d.get(1, 0).unwrap(), *d.get(0, 1).unwrap()];
        let param = |f: &Foot<(Dd, Dd)>| (f.param.0.to_f64(), f.param.1.to_f64());
        let exact = |f: &Foot<(Dd, Dd)>| f.param;

        let s = Sphere { frame, radius: r };
        let f = s.closest(q).unwrap();
        check_surface_foot(
            &f,
            q,
            tangents(&s.derivs(exact(&f), 1)),
            |u, v| *s.derivs((u, v), 0).get(0, 0).unwrap(),
            (-1.6, 1.6),
        );
        // For a near-orthonormal frame the distance is ||q - O| - r|.
        let o = sub(lift(q), lift(frame.origin));
        assert!((f.distance.to_f64() - (norm(o) - r).abs()).abs() < 1e-14);

        let c = Cylinder { frame, radius: r };
        let f = c.closest(q).unwrap();
        check_surface_foot(
            &f,
            q,
            tangents(&c.derivs(exact(&f), 1)),
            |u, v| *c.derivs((u, v), 0).get(0, 0).unwrap(),
            (-6.0, 6.0),
        );

        let t = Torus {
            frame,
            major: 1.0 + rng.next_f64(),
            minor: r * 0.5,
        };
        let f = t.closest(q).unwrap();
        check_surface_foot(
            &f,
            q,
            tangents(&t.derivs(exact(&f), 1)),
            |u, v| *t.derivs((u, v), 0).get(0, 0).unwrap(),
            (-3.2, 3.2),
        );

        let k = Cone {
            frame,
            radius: r,
            semi_angle: 0.3 + 0.4 * rng.next_f64(),
        };
        if let Some(f) = k.closest(q) {
            // Sampled on this nappe only: the parameterisation's other one is not the cone.
            let (sa, ca) = dd::sin_cos(Dd::new(k.semi_angle));
            let apex = -(Dd::new(r) * ca / sa).to_f64();
            let vs = (apex, param(&f).1.max(apex) + 4.0);
            check_surface_foot(
                &f,
                q,
                tangents(&k.derivs(exact(&f), 1)),
                |u, v| *k.derivs((u, v), 0).get(0, 0).unwrap(),
                vs,
            );
        }
    }
}

#[test]
fn exact_closest_points_have_exactly_normal_residuals() {
    let mut rng = Rng::new(4);
    let r = |x: f64| Rat::from_f64(x).unwrap();
    let dot =
        |a: &[Rat; 3], b: [f64; 3]| a.iter().zip(b).fold(Rat::zero(), |s, (x, y)| s + x * &r(y));
    for _ in 0..200 {
        let frame = sample::frame(&mut rng, 2.0);
        let q: [f64; 3] = core::array::from_fn(|_| 3.0 * rng.next_signed_unit());
        let qr = q.map(r);
        let line = Line {
            origin: frame.origin,
            direction: frame.x.map(|x| x * 3.0),
        };
        let (_, foot) = line.closest_exact(q).unwrap();
        let res: [Rat; 3] = core::array::from_fn(|k| &qr[k] - &foot.exact[k]);
        assert_eq!(dot(&res, line.direction), Rat::zero());
        let plane = Plane { frame };
        let (_, foot) = plane.closest_exact(q).unwrap();
        let res: [Rat; 3] = core::array::from_fn(|k| &qr[k] - &foot.exact[k]);
        assert_eq!(dot(&res, frame.x), Rat::zero());
        assert_eq!(dot(&res, frame.y), Rat::zero());
    }
}

#[test]
fn degenerate_queries_have_no_closest_point() {
    let mut rng = Rng::new(5);
    let frame = sample::frame(&mut rng, 1.0);
    let on_axis: [f64; 3] = core::array::from_fn(|k| frame.origin[k] + 0.5 * frame.z[k]);
    assert!(
        Circle { frame, radius: 1.0 }
            .closest(frame.origin)
            .is_none()
    );
    assert!(
        Sphere { frame, radius: 1.0 }
            .closest(frame.origin)
            .is_none()
    );
    assert!(Cylinder { frame, radius: 1.0 }.closest(on_axis).is_none());
    let tube: [f64; 3] = core::array::from_fn(|k| frame.origin[k] + 2.0 * frame.x[k]);
    let torus = Torus {
        frame,
        major: 2.0,
        minor: 0.5,
    };
    assert!(torus.closest(tube).is_none() || torus.closest(tube).unwrap().distance.to_f64() > 0.49);
    // Beyond a cone's apex (it is at v = -r / tan a below the origin).
    let below: [f64; 3] =
        core::array::from_fn(|k| frame.origin[k] - 5.0 * frame.z[k] + 0.01 * frame.x[k]);
    assert!(
        Cone {
            frame,
            radius: 1.0,
            semi_angle: 0.5,
        }
        .closest(below)
        .is_none()
    );
    let zero_line = Line {
        origin: frame.origin,
        direction: [0.0; 3],
    };
    assert!(zero_line.closest_exact([1.0, 2.0, 3.0]).is_none());
}

#[test]
fn near_tangent_pairs_are_tangent_to_the_gap() {
    let mut rng = Rng::new(6);
    for gap in [0.0, 1e-12, -1e-12, 1e-6] {
        let (circle, line) = sample::near_tangent_circle_line(&mut rng, gap);
        let (_, foot) = line.closest_exact(circle.frame.origin).unwrap();
        let d = norm(sub(lift(foot.rounded), lift(circle.frame.origin)));
        assert!((d - circle.radius - gap).abs() < 1e-15, "{gap}: {d}");
        let (sphere, plane) = sample::near_tangent_sphere_plane(&mut rng, gap);
        let (_, foot) = plane.closest_exact(sphere.frame.origin).unwrap();
        let d = norm(sub(lift(foot.rounded), lift(sphere.frame.origin)));
        assert!((d - sphere.radius - gap).abs() < 1e-15, "{gap}: {d}");
    }
}

/// The open conics: each derivative is the difference quotient of the one before (to the
/// quotient's error), the feet are normal and no sample is nearer, and a point on the
/// curve is its own foot.
#[test]
fn open_conics_have_their_derivatives_and_feet() {
    let mut rng = Rng::new(9);
    let h = 1e-12;
    for i in 0..60 {
        let frame = sample::frame(&mut rng, 2.0);
        let focal = 0.2 + rng.next_f64();
        let (a, b) = (
            0.2 + rng.next_f64(),
            (0.2 + rng.next_f64()) * if i % 2 == 0 { 1.0 } else { -1.0 },
        );
        let p = Parabola { frame, focal };
        let hy = Hyperbola {
            frame,
            semi_x: a,
            semi_y: b,
        };
        let t = 3.0 * rng.next_signed_unit();
        let quotient = |f: &dyn Fn(Dd) -> Vec<V3>, k: usize| {
            let (lo, hi) = (f(Dd::new(t) - Dd::new(h)), f(Dd::new(t) + Dd::new(h)));
            sub(hi[k], lo[k]).map(|x| x / (2.0 * h))
        };
        let pf = |t: Dd| p.derivs(t, 3);
        let hf = |t: Dd| hy.derivs(t, 3);
        for k in 0..3 {
            assert!(
                near(quotient(&pf, k), p.derivs(t, 3)[k + 1], 1e-10),
                "parabola {k}"
            );
            assert!(
                near(quotient(&hf, k), hy.derivs(t, 3)[k + 1], 1e-10),
                "hyperbola {k}"
            );
        }
        assert_eq!(p.derivs(t, 4)[3], [Dd::new(0.0); 3]);
        // Points on and off the curves.
        let on = |v: V3| dd::round(v);
        for (q, own) in [
            (on(p.derivs(t, 0)[0]), true),
            (sample::query(&mut rng, &frame, 3.0), false),
        ] {
            if let Some(f) = p.nearest(q) {
                if own {
                    assert!((f.param - Dd::new(t)).abs().hi < 1e-14, "{t} {:?}", f.param);
                } else {
                    let tangent = p.derivs(f.param, 1)[1];
                    check_curve_foot(&f, q, tangent, |s| p.derivs(4.0 * s, 0)[0]);
                }
            }
        }
        for (q, own) in [
            (on(hy.derivs(t, 0)[0]), true),
            (sample::query(&mut rng, &frame, 3.0), false),
        ] {
            if let Some(f) = hy.nearest(q) {
                if own {
                    assert!((f.param - Dd::new(t)).abs().hi < 1e-12, "{t} {:?}", f.param);
                } else {
                    let tangent = hy.derivs(f.param, 1)[1];
                    check_curve_foot(&f, q, tangent, |s| hy.derivs(s, 0)[0]);
                }
            }
        }
    }
    // Inside the parabola on its axis past the centre of curvature: two equal feet, a tie.
    let f = Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };
    let p = Parabola {
        frame: f,
        focal: 1.0,
    };
    assert_eq!(p.stationary([5.0, 0.0, 0.0]).unwrap().len(), 3);
    assert!(p.nearest([5.0, 0.0, 0.0]).is_none());
    assert_eq!(p.stationary([1.0, 0.0, 0.0]).unwrap().len(), 1);
    let hy = Hyperbola {
        frame: f,
        semi_x: 1.0,
        semi_y: 1.0,
    };
    let x = hy.nearest([1.0, 0.0, 0.0]).unwrap();
    assert_eq!(x.param.hi, 0.0);
}
