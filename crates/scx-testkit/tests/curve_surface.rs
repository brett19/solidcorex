// Written for solidcorex. Apache-2.0.
//! The curve-surface intersection truths (research 63 T2) against closed forms, against
//! configurations built to meet at known parameters, against the surfaces' parametric
//! forms and closest points (independent of the implicit equations the truths solve), and
//! against a sign scan of the exact implicit value along the curve.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use scx_num::hash::Rng;
use scx_testkit::analytic::{
    Circle, Cone, Cylinder, Ellipse, Frame, Line, Plane, Sphere, Torus, V3,
};
use scx_testkit::curve_surface::{
    self as cs, Contact, CurveKind, Error, Hit, Meeting, SurfaceKind, curve_point, intersect,
    pin_uv_of, residual, surface_point,
};
use scx_testkit::dd::{self, Dd};
use scx_testkit::sample;

fn norm(a: V3) -> f64 {
    dd::dot(&a, &a).sqrt().to_f64()
}

fn dist(a: V3, b: V3) -> f64 {
    norm([a[0] - b[0], a[1] - b[1], a[2] - b[2]])
}

fn axes() -> Frame {
    Frame {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    }
}

fn at(origin: [f64; 3]) -> Frame {
    Frame { origin, ..axes() }
}

/// A circle (even `i`) or an ellipse through a random point of `s`.
fn conic(rng: &mut Rng, s: &SurfaceKind, i: usize) -> CurveKind {
    let p = sample::point_on(rng, s);
    if i % 2 == 0 {
        sample::circle_through(rng, p)
    } else {
        sample::ellipse_through(rng, p)
    }
}

fn points(c: &CurveKind, s: &SurfaceKind) -> Vec<Hit> {
    match intersect(c, s).unwrap() {
        Meeting::Points(h) => h,
        Meeting::OnSurface => panic!("on the surface: {c:?} {s:?}"),
    }
}

fn ts(h: &[Hit]) -> Vec<f64> {
    h.iter().map(|h| h.t.to_f64()).collect()
}

fn mults(h: &[Hit]) -> Vec<u32> {
    h.iter().map(|h| h.multiplicity).collect()
}

fn surfaces(rng: &mut Rng) -> Vec<SurfaceKind> {
    let f = |rng: &mut Rng| sample::frame(rng, 1.0);
    vec![
        SurfaceKind::Plane(Plane { frame: f(rng) }),
        SurfaceKind::Cylinder(Cylinder {
            frame: f(rng),
            radius: 0.25 + rng.next_f64(),
        }),
        SurfaceKind::Cone(Cone {
            frame: f(rng),
            radius: 0.25 + rng.next_f64(),
            semi_angle: 0.1 + 1.2 * rng.next_f64(),
        }),
        SurfaceKind::Sphere(Sphere {
            frame: f(rng),
            radius: 0.25 + rng.next_f64(),
        }),
        SurfaceKind::Torus(Torus {
            frame: f(rng),
            major: 1.0 + rng.next_f64(),
            minor: 0.2 + 0.5 * rng.next_f64(),
        }),
    ]
}

/// Every hit is on the curve at its `t` (by construction), on the surface at its uv
/// (through the parametric form, which the truth never uses), and its pin uv maps back.
fn check_hits(c: &CurveKind, s: &SurfaceKind, hits: &[Hit], tol: f64) {
    for h in hits {
        assert!(dist(curve_point(c, h.t), h.point) < tol);
        let on = surface_point(s, h.uv);
        assert!(
            dist(on, h.point) < tol,
            "{c:?} {s:?} {h:?}: surface at uv {:?} is {}",
            dd::round(on),
            dist(on, h.point)
        );
        if let Some(p) = h.pin_uv {
            let back = cs::implicit(s).unwrap().pin_layout().to_step(p);
            assert!(dist(surface_point(s, back), h.point) < tol, "{s:?} {h:?}");
        }
        assert_eq!(h.contact == Contact::Crossing, h.multiplicity == 1);
    }
    for w in hits.windows(2) {
        assert!(w[0].t.to_f64() <= w[1].t.to_f64());
    }
}

// ---------------------------------------------------------------- closed forms

#[test]
fn a_line_through_a_spheres_centre_meets_it_at_plus_minus_r_over_d() {
    let mut rng = Rng::new(0x7201);
    for _ in 0..50 {
        let frame = sample::frame(&mut rng, 1.0);
        let r = 0.25 + rng.next_f64();
        let d = sample::direction(&mut rng).map(|x| x * (0.5 + rng.next_f64()));
        let s = SurfaceKind::Sphere(Sphere { frame, radius: r });
        let c = CurveKind::Line(Line {
            origin: frame.origin,
            direction: d,
        });
        let h = points(&c, &s);
        let dn = norm(dd::lift(d));
        let want = r / dn;
        assert_eq!(mults(&h), [1, 1]);
        // The frame is orthonormal to an ulp, so the truth's sphere is r to an ulp.
        assert!((h[0].t.to_f64() + want).abs() < 1e-14 * want);
        assert!((h[1].t.to_f64() - want).abs() < 1e-14 * want);
        check_hits(&c, &s, &h, 1e-13);
    }
}

#[test]
fn the_torus_equator_line_meets_it_at_both_tube_walls_on_each_side() {
    let t = Torus {
        frame: axes(),
        major: 3.0,
        minor: 1.25,
    };
    let s = SurfaceKind::Torus(t);
    let c = CurveKind::Line(Line {
        origin: [0.0; 3],
        direction: [1.0, 0.0, 0.0],
    });
    let h = points(&c, &s);
    assert_eq!(ts(&h), [-4.25, -1.75, 1.75, 4.25]);
    assert_eq!(mults(&h), [1; 4]);
    // Exact roots are met exactly: a zero interval.
    assert!(h.iter().all(|h| h.t_radius == 0.0));
    check_hits(&c, &s, &h, 1e-15);
    // The pin's uv: u at 0 and pi are knots 0 and 1/2; v at 0 (outer) and pi (inner).
    let pin: Vec<(f64, f64)> = h
        .iter()
        .map(|h| {
            let (u, v) = h.pin_uv.unwrap();
            (u.to_f64(), v.to_f64())
        })
        .collect();
    assert_eq!(pin, [(0.5, 0.0), (0.5, 0.5), (0.0, 0.5), (0.0, 0.0)]);
}

#[test]
fn a_line_over_the_torus_top_touches_it_twice() {
    let s = SurfaceKind::Torus(Torus {
        frame: axes(),
        major: 3.0,
        minor: 1.0,
    });
    let c = CurveKind::Line(Line {
        origin: [0.0, 0.0, 1.0],
        direction: [0.5, 0.0, 0.0],
    });
    let h = points(&c, &s);
    assert_eq!(ts(&h), [-6.0, 6.0]);
    assert_eq!(mults(&h), [2, 2]);
    assert!(h.iter().all(|h| h.contact == Contact::Tangent));
    check_hits(&c, &s, &h, 1e-15);
}

#[test]
fn tangent_lines_to_the_quadrics_are_double_roots() {
    let cases = [
        (
            SurfaceKind::Sphere(Sphere {
                frame: at([1.0, 2.0, 3.0]),
                radius: 0.75,
            }),
            Line {
                origin: [1.75, 2.0, 3.0],
                direction: [0.0, 1.0, 0.5],
            },
            0.0,
        ),
        (
            SurfaceKind::Cylinder(Cylinder {
                frame: axes(),
                radius: 2.0,
            }),
            Line {
                origin: [2.0, -3.0, 0.0],
                direction: [0.0, 1.0, 0.25],
            },
            3.0,
        ),
        (
            // The cone x^2 + y^2 = (1 + z)^2 (semi-angle pi/4) and the line on its tangent
            // plane x = 1 + z off the generator: tangent along the generator, met once.
            SurfaceKind::Cone(Cone {
                frame: axes(),
                radius: 1.0,
                semi_angle: core::f64::consts::FRAC_PI_4,
            }),
            Line {
                origin: [1.0, 0.0, 0.0],
                direction: [0.0, 1.0, 0.0],
            },
            0.0,
        ),
        (
            SurfaceKind::Plane(Plane { frame: axes() }),
            Line {
                origin: [0.0, 0.0, 2.0],
                direction: [1.0, 0.0, -0.5],
            },
            4.0,
        ),
    ];
    for (s, l, t) in cases {
        let c = CurveKind::Line(l);
        let h = match intersect(&c, &s) {
            Ok(Meeting::Points(h)) => h,
            other => panic!("{other:?}"),
        };
        let want = if matches!(s, SurfaceKind::Plane(_)) {
            1
        } else {
            2
        };
        // The cone's slope is tan(pi/4) within 2^-104, not 1: its tangent line is a
        // double root or two roots 2^-52 apart, both within that rounding.
        if matches!(s, SurfaceKind::Cone(_)) {
            assert!(!h.is_empty() && h.len() <= 2, "{h:?}");
            assert!(h.iter().all(|x| (x.t.to_f64() - t).abs() < 1e-12));
            continue;
        }
        assert_eq!(mults(&h), [want], "{s:?}");
        assert_eq!(ts(&h), [t]);
        check_hits(&c, &s, &h, 1e-14);
    }
}

#[test]
fn curves_lying_on_their_surfaces_are_reported_as_such() {
    let cases = [
        (
            CurveKind::Circle(Circle {
                frame: at([0.0, 0.0, 5.0]),
                radius: 2.0,
            }),
            SurfaceKind::Cylinder(Cylinder {
                frame: axes(),
                radius: 2.0,
            }),
        ),
        (
            CurveKind::Circle(Circle {
                frame: Frame {
                    origin: [0.0; 3],
                    x: [0.0, 0.0, 1.0],
                    y: [0.0, 1.0, 0.0],
                    z: [-1.0, 0.0, 0.0],
                },
                radius: 1.5,
            }),
            SurfaceKind::Sphere(Sphere {
                frame: axes(),
                radius: 1.5,
            }),
        ),
        (
            CurveKind::Line(Line {
                origin: [0.0, 1.0, 0.0],
                direction: [0.0, 0.0, 1.0],
            }),
            SurfaceKind::Cylinder(Cylinder {
                frame: axes(),
                radius: 1.0,
            }),
        ),
        (
            CurveKind::Ellipse(Ellipse {
                frame: at([0.0, 0.0, 0.0]),
                semi_x: 3.0,
                semi_y: 0.5,
            }),
            SurfaceKind::Plane(Plane { frame: axes() }),
        ),
        (
            // The torus's top circle.
            CurveKind::Circle(Circle {
                frame: at([0.0, 0.0, 1.0]),
                radius: 3.0,
            }),
            SurfaceKind::Torus(Torus {
                frame: axes(),
                major: 3.0,
                minor: 1.0,
            }),
        ),
    ];
    for (c, s) in cases {
        assert_eq!(intersect(&c, &s), Ok(Meeting::OnSurface), "{c:?} {s:?}");
    }
}

#[test]
fn a_conics_root_at_infinity_is_its_half_turn() {
    // A circle standing on the plane z = 0, touching it at its t = pi.
    let c = CurveKind::Circle(Circle {
        frame: Frame {
            origin: [0.0, 0.0, 1.0],
            x: [0.0, 0.0, 1.0],
            y: [1.0, 0.0, 0.0],
            z: [0.0, 1.0, 0.0],
        },
        radius: 1.0,
    });
    let s = SurfaceKind::Plane(Plane { frame: axes() });
    let h = points(&c, &s);
    assert_eq!(mults(&h), [2]);
    assert_eq!(h[0].t, dd::consts().pi);
    check_hits(&c, &s, &h, 1e-15);
    // Lowered by a quarter it crosses at t = pi -+ acos(3/4)... and at the plane: the
    // ellipse x = 2 cos t, z = 0.5 + sin t crosses z = 0 at sin t = -1/2: 7 pi/6, 11 pi/6.
    let e = CurveKind::Ellipse(Ellipse {
        frame: Frame {
            origin: [0.0, 0.0, 0.5],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 0.0, 1.0],
            z: [0.0, -1.0, 0.0],
        },
        semi_x: 2.0,
        semi_y: 1.0,
    });
    let h = points(&e, &s);
    let pi = core::f64::consts::PI;
    assert_eq!(mults(&h), [1, 1]);
    assert!((h[0].t.to_f64() - 7.0 * pi / 6.0).abs() < 1e-15);
    assert!((h[1].t.to_f64() - 11.0 * pi / 6.0).abs() < 1e-15);
    check_hits(&e, &s, &h, 1e-15);
}

#[test]
fn a_circle_round_a_cylinder_wall_touches_it_four_times_when_inscribed() {
    // A circle of radius 2 in the plane x = 0 centred on the axis meets the cylinder of
    // radius 1 about z (x^2 + y^2 = 1) where y = +-1: z = +-sqrt3, each a crossing.
    let c = CurveKind::Circle(Circle {
        frame: Frame {
            origin: [0.0; 3],
            x: [0.0, 1.0, 0.0],
            y: [0.0, 0.0, 1.0],
            z: [1.0, 0.0, 0.0],
        },
        radius: 2.0,
    });
    let s = SurfaceKind::Cylinder(Cylinder {
        frame: axes(),
        radius: 1.0,
    });
    let h = points(&c, &s);
    assert_eq!(mults(&h), [1, 1, 1, 1]);
    let pi = core::f64::consts::PI;
    for (h, w) in h
        .iter()
        .zip([pi / 3.0, 2.0 * pi / 3.0, 4.0 * pi / 3.0, 5.0 * pi / 3.0])
    {
        assert!((h.t.to_f64() - w).abs() < 1e-15, "{h:?}");
    }
    check_hits(&c, &s, &h, 1e-15);
    // Radius 1: tangent at t = 0 and pi (y = +-1, z = 0), double each.
    let c = CurveKind::Circle(Circle {
        frame: Frame {
            origin: [0.0; 3],
            x: [0.0, 1.0, 0.0],
            y: [0.0, 0.0, 1.0],
            z: [1.0, 0.0, 0.0],
        },
        radius: 1.0,
    });
    let h = points(&c, &s);
    assert_eq!(mults(&h), [2, 2]);
    assert_eq!(ts(&h), [0.0, pi]);
}

#[test]
fn bad_inputs_are_typed_causes() {
    let line = CurveKind::Line(Line {
        origin: [0.0; 3],
        direction: [1.0, 0.0, 0.0],
    });
    let torus = |major, minor| {
        SurfaceKind::Torus(Torus {
            frame: axes(),
            major,
            minor,
        })
    };
    assert_eq!(
        intersect(&line, &torus(1.0, 1.0)),
        Err(Error::DegenerateSurface)
    );
    assert_eq!(
        intersect(&line, &torus(1.0, 2.0)),
        Err(Error::DegenerateSurface)
    );
    let flat = SurfaceKind::Sphere(Sphere {
        frame: Frame {
            z: [1.0, 0.0, 0.0],
            ..axes()
        },
        radius: 1.0,
    });
    assert_eq!(intersect(&line, &flat), Err(Error::DegenerateFrame));
    let zero = CurveKind::Line(Line {
        origin: [0.0; 3],
        direction: [0.0; 3],
    });
    assert_eq!(
        intersect(&zero, &torus(2.0, 1.0)),
        Err(Error::DegenerateCurve)
    );
    let nan = CurveKind::Line(Line {
        origin: [f64::NAN, 0.0, 0.0],
        direction: [1.0, 0.0, 0.0],
    });
    assert_eq!(intersect(&nan, &torus(2.0, 1.0)), Err(Error::NonFinite));
}

// ---------------------------------------------------------------- built to meet

/// Random lines through two points of each surface meet it at `t = 0` and `t = 1` among
/// the hits; every hit is on both.
#[test]
fn lines_through_two_surface_points_meet_there() {
    let mut rng = Rng::new(0x7202);
    for _ in 0..20 {
        for s in surfaces(&mut rng) {
            let (a, b) = sample::two_points_on(&mut rng, &s);
            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let c = CurveKind::Line(Line {
                origin: a,
                direction: d,
            });
            let h = points(&c, &s);
            check_hits(&c, &s, &h, 1e-12);
            if matches!(s, SurfaceKind::Plane(_)) {
                // Both points are on the plane to the rounding: the line is in it to the
                // rounding, met once anywhere (or not at all).
                continue;
            }
            // The rounded points are off the surface by an ulp or so: roots near 0 and 1.
            for want in [0.0, 1.0] {
                assert!(
                    h.iter().any(|h| (h.t.to_f64() - want).abs() < 1e-9),
                    "{s:?} {c:?} {:?}",
                    ts(&h)
                );
            }
        }
    }
}

/// Random circles and ellipses through a point of each surface meet it at `t = 0`.
#[test]
fn conics_through_a_surface_point_meet_there() {
    let mut rng = Rng::new(0x7203);
    for i in 0..12 {
        for s in surfaces(&mut rng) {
            let c = conic(&mut rng, &s, i);
            let h = points(&c, &s);
            check_hits(&c, &s, &h, 1e-12);
            assert!(
                h.iter().any(|h| {
                    let t = h.t.to_f64();
                    t.min(2.0 * core::f64::consts::PI - t) < 1e-8
                }),
                "{s:?} {c:?} {:?}",
                ts(&h)
            );
        }
    }
}

// ---------------------------------------------------------------- a second method

/// The odd-multiplicity hits are exactly the sign changes of the exact implicit value at
/// the curve's rounded points, scanned finely enough to separate them.
#[test]
fn crossings_are_the_sign_changes_of_the_implicit_value() {
    let mut rng = Rng::new(0x7204);
    for i in 0..4 {
        for s in surfaces(&mut rng) {
            let c = conic(&mut rng, &s, i);
            let h = points(&c, &s);
            let odd: Vec<f64> = h
                .iter()
                .filter(|h| h.multiplicity % 2 == 1)
                .map(|h| h.t.to_f64())
                .collect();
            // Skip scans that would put a root within a step of a sample.
            let n = 256;
            let step = 2.0 * core::f64::consts::PI / n as f64;
            if h.iter().any(|h| {
                let x = h.t.to_f64() / step;
                (x - x.round()).abs() < 1e-3
            }) {
                continue;
            }
            let sign = |k: usize| {
                let p = dd::round(curve_point(&c, Dd::new(k as f64 * step)));
                residual(&s, p).unwrap().signum()
            };
            let mut changes = Vec::new();
            let mut prev = sign(0);
            for k in 1..=n {
                let cur = sign(k % n);
                if cur != prev {
                    changes.push((k as f64 - 0.5) * step);
                }
                prev = cur;
            }
            assert_eq!(changes.len(), odd.len(), "{s:?} {c:?} {:?}", ts(&h));
            for (a, b) in changes.iter().zip(&odd) {
                assert!((a - b).abs() <= step, "{a} {b}");
            }
        }
    }
}

/// The hits' distance to the surface by its closest point (a different computation from
/// the implicit form) is zero to the rounding.
#[test]
fn hits_are_at_zero_distance_by_the_closest_point() {
    let mut rng = Rng::new(0x7205);
    for i in 0..10 {
        for s in surfaces(&mut rng) {
            let c = conic(&mut rng, &s, i);
            for h in points(&c, &s) {
                if h.pin_uv.is_none() {
                    // The cone's far nappe, which its closest point does not search.
                    continue;
                }
                let q = dd::round(h.point);
                let d = match &s {
                    SurfaceKind::Plane(p) => {
                        let (_, e) = p.closest_exact(q).unwrap();
                        dist(dd::lift(e.rounded), dd::lift(q))
                    }
                    SurfaceKind::Cylinder(x) => x.closest(q).unwrap().distance.to_f64(),
                    SurfaceKind::Cone(x) => x.closest(q).unwrap().distance.to_f64(),
                    SurfaceKind::Sphere(x) => x.closest(q).unwrap().distance.to_f64(),
                    SurfaceKind::Torus(x) => x.closest(q).unwrap().distance.to_f64(),
                };
                assert!(d < 1e-14, "{s:?} {c:?} {h:?}: {d}");
            }
        }
    }
}

// ---------------------------------------------------------------- near tangency

#[test]
fn a_line_near_tangent_to_a_sphere_splits_into_two_close_roots_or_none() {
    let mut rng = Rng::new(0x7206);
    for gap in [-1e-6, -1e-10, -1e-14, 1e-14, 1e-10] {
        let (sphere, plane) = sample::near_tangent_sphere_plane(&mut rng, gap);
        let f = plane.frame;
        let c = CurveKind::Line(Line {
            origin: f.origin,
            direction: f.x,
        });
        let s = SurfaceKind::Sphere(sphere);
        let h = points(&c, &s);
        // The exact distance from the sphere's centre to the rounded line decides.
        if gap < -1e-13 {
            assert_eq!(mults(&h), [1, 1], "{gap}");
            let r = sphere.radius;
            let half = (2.0 * r * -gap).sqrt();
            assert!(((h[1].t - h[0].t).to_f64() / 2.0 - half).abs() < 1e-6 * half + 1e-12);
        } else if gap > 1e-13 {
            assert!(h.is_empty(), "{gap} {h:?}");
        }
        check_hits(&c, &s, &h, 1e-12);
    }
}

#[test]
fn pin_uv_is_none_only_on_the_cones_far_nappe() {
    let s = SurfaceKind::Cone(Cone {
        frame: axes(),
        radius: 1.0,
        semi_angle: core::f64::consts::FRAC_PI_4,
    });
    let c = CurveKind::Line(Line {
        origin: [0.5, 0.0, -10.0],
        direction: [0.0, 0.0, 1.0],
    });
    let h = points(&c, &s);
    // x = 0.5 meets |1 + z| = 0.5 at z = -0.5 and z = -1.5 (the far nappe).
    assert_eq!(h.len(), 2);
    assert!(h[0].pin_uv.is_none() && h[1].pin_uv.is_some());
    // On the far nappe the STEP u is the half-turn: x = (1 + v) cos u with 1 + v < 0.
    assert!((h[0].uv.0.to_f64() - core::f64::consts::PI).abs() < 1e-15);
    check_hits(&c, &s, &h, 1e-14);
    let layout = cs::implicit(&s).unwrap().pin_layout();
    assert_eq!(pin_uv_of(&s, &layout, h[1].uv), h[1].pin_uv);
}

/// The first-order distance is the true distance off a plane, and off a sphere the closed
/// form `(rho^2 - r^2) / (2 rho)` at the radius `rho`; zero on each. The frames are exact
/// (signed world axes): a rounded frame's image of the sphere is off it by about `1e-16 r`,
/// which the closed form would not see.
#[test]
fn the_first_order_distance_is_the_closed_form_off_a_plane_and_a_sphere() {
    use scx_testkit::curve_surface::distance;
    let mut rng = Rng::new(0x7203);
    for _ in 0..50 {
        let f = sample::axis_frame(&mut rng);
        let r = 0.5 + rng.next_f64();
        let h = 1e-6 * rng.next_signed_unit();
        let sphere = SurfaceKind::Sphere(Sphere {
            frame: f,
            radius: r,
        });
        let plane = SurfaceKind::Plane(Plane { frame: f });
        let rho = r + h;
        let p = core::array::from_fn(|i| f.origin[i] + rho * f.x[i]);
        // The point as rounded: its radius and height, recomputed in double-double.
        let l = core::array::from_fn::<Dd, 3, _>(|i| Dd::new(p[i]) - Dd::new(f.origin[i]));
        let rho = dd::dot(&l, &l).sqrt();
        let want = ((rho * rho - Dd::new(r) * r) / (rho * 2.0)).abs().to_f64();
        let got = distance(&sphere, p).unwrap();
        assert!((got - want).abs() <= 1e-12 * want, "{got} {want}");
        let z: [Dd; 3] = f.z.map(Dd::new);
        let want = dd::dot(&l, &z).abs().to_f64() / dd::dot(&z, &z).sqrt().to_f64();
        let got = distance(&plane, p).unwrap();
        assert!(
            (got - want).abs() <= 1e-14 * want.max(1e-12),
            "{got} {want}"
        );
        let on = sample::point_on(&mut rng, &sphere);
        assert_eq!(
            residual(&sphere, on).unwrap().is_zero(),
            distance(&sphere, on).unwrap() == 0.0
        );
    }
}
