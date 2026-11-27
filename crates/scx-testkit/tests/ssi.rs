// Written for solidcorex. Apache-2.0.
//! The analytic surface-surface truths (research 63 T5) against closed forms, against the
//! curve-surface truths (T2: lines through the section's samples meet both surfaces there,
//! found by a different polynomial), against the surfaces' parametric forms at the
//! samples' uv, and WR-680's exact section of the pin's NURBS cylinder.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic
)]

use scx_num::hash::Rng;
use scx_testkit::analytic::{Cone, Cylinder, Frame, Line, Plane, Sphere, Torus, V3};
use scx_testkit::curve_surface::{self as cs, step_uv_of, surface_point, CurveKind, SurfaceKind};
use scx_testkit::dd::{self, Dd};
use scx_testkit::gen;
use scx_testkit::poly::to_dd;
use scx_testkit::ssi::{
    bernstein, cylinder_section, intersect, Contact, Curve, Error, Piece, Section, ARCS,
};
use scx_testkit::Rat;

fn norm(a: V3) -> f64 {
    dd::dot(&a, &a).sqrt().to_f64()
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dist(a: V3, b: V3) -> f64 {
    norm(sub(a, b))
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
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

fn plane(origin: [f64; 3], x: [f64; 3], y: [f64; 3]) -> SurfaceKind {
    SurfaceKind::Plane(Plane {
        frame: Frame {
            origin,
            x,
            y,
            z: [0.0; 3],
        },
    })
}

fn sphere(origin: [f64; 3], radius: f64) -> SurfaceKind {
    SurfaceKind::Sphere(Sphere {
        frame: at(origin),
        radius,
    })
}

fn cylinder(radius: f64) -> SurfaceKind {
    SurfaceKind::Cylinder(Cylinder {
        frame: axes(),
        radius,
    })
}

fn cone(radius: f64, semi_angle: f64) -> SurfaceKind {
    SurfaceKind::Cone(Cone {
        frame: axes(),
        radius,
        semi_angle,
    })
}

fn torus(z: f64, major: f64, minor: f64) -> SurfaceKind {
    SurfaceKind::Torus(Torus {
        frame: at([0.0, 0.0, z]),
        major,
        minor,
    })
}

fn horizontal(z: f64) -> SurfaceKind {
    plane([0.0, 0.0, z], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
}

fn pieces(s1: &SurfaceKind, s2: &SurfaceKind) -> Vec<Piece> {
    match intersect(s1, s2).unwrap() {
        Section::Curves(p) => {
            check(s1, s2, &p, 1e-26);
            p
        }
        other => panic!("{other:?}: {s1:?} {s2:?}"),
    }
}

/// Every sample is on its curve, within `tol` of both surfaces (the exact implicit value),
/// and on both surfaces at its uv (their parametric forms) and back from its pin uv.
fn check(s1: &SurfaceKind, s2: &SurfaceKind, pieces: &[Piece], tol: f64) {
    for p in pieces {
        assert!(!p.samples.is_empty());
        for s in &p.samples {
            assert!(dist(p.curve.point(s.t), s.point) == 0.0);
            assert!(s.gap < tol, "{s1:?} {s2:?} {p:?}: gap {}", s.gap);
            for (i, surf) in [s1, s2].into_iter().enumerate() {
                let on = surface_point(surf, s.uv[i]);
                assert!(
                    dist(on, s.point) < 1e3 * tol,
                    "{surf:?} {s:?}: {}",
                    dist(on, s.point)
                );
                if let Some(pin) = s.pin_uv[i] {
                    let back = surface_point(surf, step_uv_of(surf, pin));
                    assert!(dist(back, s.point) < 1e3 * tol, "{surf:?} {s:?}");
                }
            }
        }
    }
}

fn kinds(p: &[Piece]) -> Vec<(&'static str, Contact)> {
    p.iter()
        .map(|p| {
            let k = match p.curve {
                Curve::Point(_) => "point",
                Curve::Line { .. } => "line",
                Curve::Circle { .. } => "circle",
                Curve::Ellipse { .. } => "ellipse",
                Curve::Hyperbola { .. } => "hyperbola",
            };
            (k, p.contact)
        })
        .collect()
}

/// A conic's centre and semi-diameters.
fn conic(p: &Piece) -> (V3, V3, V3) {
    match p.curve {
        Curve::Circle { centre, a, b } | Curve::Ellipse { centre, a, b } => (centre, a, b),
        Curve::Hyperbola { centre, a, b } => (centre, a, b),
        _ => panic!("{p:?}"),
    }
}

/// A circle of radius `r` about `c` (perpendicular equal semi-diameters).
fn is_circle(p: &Piece, c: [f64; 3], r: f64, tol: f64) {
    let (centre, a, b) = conic(p);
    assert!(dist(centre, dd::lift(c)) < tol, "{p:?}");
    assert!(
        (norm(a) - r).abs() < tol && (norm(b) - r).abs() < tol,
        "{p:?}"
    );
    assert!(dd::dot(&a, &b).to_f64().abs() < tol, "{p:?}");
}

use Contact::{Tangent as T, Transversal as X};

// ---------------------------------------------------------------- closed forms

#[test]
fn planes_meet_in_a_line_or_coincide_or_not_at_all() {
    let xy = horizontal(0.0);
    let p = pieces(&xy, &plane([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]));
    assert_eq!(kinds(&p), [("line", X)]);
    let Curve::Line { origin, direction } = p[0].curve else {
        panic!()
    };
    assert!(norm(origin) == 0.0 && norm(cross(direction, dd::lift([0.0, 1.0, 0.0]))) == 0.0);
    assert_eq!(intersect(&xy, &horizontal(1.0)).unwrap(), Section::Empty);
    // The same plane through another point, by other axes.
    let same = plane([1.0, 2.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
    assert_eq!(intersect(&xy, &same).unwrap(), Section::Coincident);
}

#[test]
fn a_sphere_cut_at_height_h_is_a_circle_of_radius_sqrt_r2_minus_h2() {
    let s = sphere([0.0; 3], 2.0);
    let p = pieces(&s, &horizontal(1.0));
    assert_eq!(kinds(&p), [("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, 1.0], 3f64.sqrt(), 1e-15);
    // The tangent plane is a point; past it, nothing.
    let p = pieces(&horizontal(2.0), &s);
    assert_eq!(kinds(&p), [("point", T)]);
    assert_eq!(p[0].curve, Curve::Point(dd::lift([0.0, 0.0, 2.0])));
    assert_eq!(intersect(&s, &horizontal(2.5)).unwrap(), Section::Empty);
}

#[test]
fn an_oblique_cylinder_section_is_an_ellipse_by_apollonius() {
    // z = x across the unit cylinder: semi-axes 1 and sqrt 2, so |a|^2 + |b|^2 = 3 and
    // |a x b| = sqrt 2 for any conjugate pair.
    let p = pieces(
        &cylinder(1.0),
        &plane([0.0; 3], [1.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    );
    assert_eq!(kinds(&p), [("ellipse", X)]);
    let (c, a, b) = conic(&p[0]);
    assert!(norm(c) < 1e-30);
    let sum = dd::dot(&a, &a) + dd::dot(&b, &b);
    assert!((sum - Dd::new(3.0)).abs().to_f64() < 1e-30);
    assert!((norm(cross(a, b)) - 2f64.sqrt()).abs() < 1e-15);
    // Across the axis, a circle of the radius.
    let p = pieces(&cylinder(1.5), &horizontal(3.0));
    assert_eq!(kinds(&p), [("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, 3.0], 1.5, 1e-30);
}

#[test]
fn planes_along_the_cylinder_give_two_lines_a_tangent_line_or_none() {
    let cyl = cylinder(1.0);
    let wall = |x: f64| plane([x, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let p = pieces(&cyl, &wall(0.6));
    assert_eq!(kinds(&p), [("line", X), ("line", X)]);
    let mut ys: Vec<f64> = p
        .iter()
        .map(|p| match p.curve {
            Curve::Line { origin, .. } => origin[1].to_f64(),
            _ => panic!(),
        })
        .collect();
    ys.sort_by(f64::total_cmp);
    assert!(
        (ys[0] + 0.8).abs() < 1e-15 && (ys[1] - 0.8).abs() < 1e-15,
        "{ys:?}"
    );
    let p = pieces(&wall(1.0), &cyl);
    assert_eq!(kinds(&p), [("line", T)]);
    assert_eq!(intersect(&cyl, &wall(1.25)).unwrap(), Section::Empty);
}

#[test]
fn cone_sections_are_each_conic_by_the_planes_tilt() {
    let a = 0.5;
    let tan = (dd::sin_cos(Dd::new(a)).0 / dd::sin_cos(Dd::new(a)).1).to_f64();
    let c = cone(0.0, a);
    // Across the axis at height 1: a circle of radius tan a.
    let p = pieces(&c, &horizontal(1.0));
    assert_eq!(kinds(&p), [("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, 1.0], tan, 1e-15);
    // Through the apex, less steep than the cone: the apex alone.
    let p = pieces(&c, &plane([0.0; 3], [1.0, 0.0, 0.25], [0.0, 1.0, 0.0]));
    assert_eq!(kinds(&p), [("point", T)]);
    // Through the axis: two generators, y = +-z tan a.
    let p = pieces(&c, &plane([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]));
    assert_eq!(kinds(&p), [("line", X), ("line", X)]);
    for piece in &p {
        let Curve::Line { direction: d, .. } = piece.curve else {
            panic!()
        };
        assert!(((d[1] / d[2]).abs().to_f64() - tan).abs() < 1e-15, "{d:?}");
    }
    // Tilted less than the cone: an ellipse; parallel to the axis off it: a hyperbola of
    // two branches.
    let p = pieces(
        &c,
        &plane([0.0, 0.0, 1.0], [1.0, 0.0, 0.1], [0.0, 1.0, 0.0]),
    );
    assert_eq!(kinds(&p), [("ellipse", X)]);
    let p = pieces(
        &c,
        &plane([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    );
    assert_eq!(kinds(&p), [("hyperbola", X), ("hyperbola", X)]);
    // The branches' vertices are at z = +-1 / tan a on x = 1.
    for piece in &p {
        let (c, a, _) = conic(piece);
        let v = [c[0] + a[0], c[1] + a[1], c[2] + a[2]];
        assert!(((v[2].abs()).to_f64() - 1.0 / tan).abs() < 1e-14, "{v:?}");
        assert!((v[0].to_f64() - 1.0).abs() < 1e-15);
    }
    // Through a cone with a radius: at 45 degrees, less steep than its generators (about
    // 61 degrees), an ellipse; at about 72 degrees, a hyperbola.
    let tilted = |s: f64| plane([0.0, 0.0, 1.0], [1.0, 0.0, s], [0.0, 1.0, 0.0]);
    assert_eq!(
        kinds(&pieces(&cone(0.5, a), &tilted(1.0))),
        [("ellipse", X)]
    );
    let p = pieces(&cone(0.5, a), &tilted(3.0));
    assert_eq!(kinds(&p), [("hyperbola", X), ("hyperbola", X)]);
}

#[test]
fn two_spheres_meet_on_their_radical_plane() {
    let s = sphere([0.0; 3], 1.0);
    let p = pieces(&s, &sphere([0.0, 0.0, 1.0], 1.0));
    assert_eq!(kinds(&p), [("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, 0.5], 0.75f64.sqrt(), 1e-15);
    // Outside and inside tangency, apart, the same and concentric.
    let p = pieces(&s, &sphere([0.0, 0.0, 2.0], 1.0));
    assert_eq!(kinds(&p), [("point", T)]);
    assert_eq!(p[0].curve, Curve::Point(dd::lift([0.0, 0.0, 1.0])));
    let p = pieces(&sphere([0.0; 3], 2.0), &sphere([0.0, 0.0, 1.0], 1.0));
    assert_eq!(p[0].curve, Curve::Point(dd::lift([0.0, 0.0, 2.0])));
    assert_eq!(
        intersect(&s, &sphere([0.0, 3.0, 0.0], 1.0)).unwrap(),
        Section::Empty
    );
    assert_eq!(intersect(&s, &s).unwrap(), Section::Coincident);
    assert_eq!(
        intersect(&s, &sphere([0.0; 3], 0.5)).unwrap(),
        Section::Empty
    );
}

#[test]
fn coaxial_quadrics_meet_in_circles_at_the_meridians_crossings() {
    // Sphere 2 and cylinder 1: h = +-sqrt 3.
    let p = pieces(&sphere([0.0; 3], 2.0), &cylinder(1.0));
    assert_eq!(kinds(&p), [("circle", X), ("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, -(3f64.sqrt())], 1.0, 1e-15);
    is_circle(&p[1], [0.0, 0.0, 3f64.sqrt()], 1.0, 1e-15);
    // Cylinder 2 round sphere 2: the equator, double.
    let p = pieces(&cylinder(2.0), &sphere([0.0; 3], 2.0));
    assert_eq!(kinds(&p), [("circle", T)]);
    // A cone from the sphere's centre: h = +-1 / sqrt(1 + k^2).
    let a = 0.7;
    let (sn, cn) = dd::sin_cos(Dd::new(a));
    let p = pieces(&cone(0.0, a), &sphere([0.0; 3], 1.0));
    assert_eq!(kinds(&p), [("circle", X), ("circle", X)]);
    is_circle(&p[1], [0.0, 0.0, cn.to_f64()], sn.to_f64(), 1e-15);
    // A cone's apex on the sphere: the apex point, and a circle.
    let apex = SurfaceKind::Cone(Cone {
        frame: at([0.0, 0.0, -1.0]),
        radius: 0.0,
        semi_angle: a,
    });
    let p = pieces(&apex, &sphere([0.0; 3], 1.0));
    assert_eq!(kinds(&p), [("point", T), ("circle", X)]);
    // Equal cylinders coincide; the same cone moved along its axis does not.
    assert_eq!(
        intersect(&cylinder(1.0), &cylinder(1.0)).unwrap(),
        Section::Coincident
    );
    assert_eq!(
        intersect(&cylinder(1.0), &cylinder(2.0)).unwrap(),
        Section::Empty
    );
}

#[test]
fn tori_meet_planes_quadrics_and_tori_across_their_axis() {
    let t = torus(0.0, 2.0, 1.0);
    let p = pieces(&t, &horizontal(0.0));
    assert_eq!(kinds(&p), [("circle", X), ("circle", X)]);
    is_circle(&p[0], [0.0; 3], 1.0, 1e-30);
    is_circle(&p[1], [0.0; 3], 3.0, 1e-30);
    let p = pieces(&horizontal(1.0), &t);
    assert_eq!(kinds(&p), [("circle", T)]);
    is_circle(&p[0], [0.0, 0.0, 1.0], 2.0, 1e-30);
    assert_eq!(intersect(&t, &horizontal(1.5)).unwrap(), Section::Empty);
    // Cylinders through the tube's centre circle and round its outer equator; spheres
    // touching its equators.
    let p = pieces(&t, &cylinder(2.0));
    assert_eq!(kinds(&p), [("circle", X), ("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, -1.0], 2.0, 1e-15);
    assert_eq!(kinds(&pieces(&t, &cylinder(3.0))), [("circle", T)]);
    assert_eq!(kinds(&pieces(&sphere([0.0; 3], 3.0), &t)), [("circle", T)]);
    assert_eq!(kinds(&pieces(&t, &sphere([0.0; 3], 1.0))), [("circle", T)]);
    // Equal tori a unit apart: h = 1/2, rho = 2 +- sqrt 3 / 2.
    let p = pieces(&t, &torus(1.0, 2.0, 1.0));
    assert_eq!(kinds(&p), [("circle", X), ("circle", X)]);
    is_circle(&p[0], [0.0, 0.0, 0.5], 2.0 - 0.75f64.sqrt(), 1e-15);
    // Major radii 2 and 3, minor 1: rho = 5/2, h = +-sqrt 3 / 2.
    let p = pieces(&t, &torus(0.0, 3.0, 1.0));
    assert_eq!(kinds(&p), [("circle", X), ("circle", X)]);
    is_circle(&p[1], [0.0, 0.0, 0.75f64.sqrt()], 2.5, 1e-15);
    assert_eq!(intersect(&t, &t).unwrap(), Section::Coincident);
}

#[test]
fn pairs_without_a_closed_form_and_bad_data_are_typed_causes() {
    let t = torus(0.0, 2.0, 1.0);
    let tilted = plane([0.0; 3], [1.0, 0.0, 0.5], [0.0, 1.0, 0.0]);
    assert_eq!(intersect(&t, &tilted), Err(Error::NoClosedForm));
    let off_axis = SurfaceKind::Cylinder(Cylinder {
        frame: at([0.5, 0.0, 0.0]),
        radius: 1.0,
    });
    assert_eq!(
        intersect(&cylinder(1.0), &off_axis),
        Err(Error::NoClosedForm)
    );
    let skew = SurfaceKind::Sphere(Sphere {
        frame: Frame {
            x: [0.0, 1.0, 0.0],
            y: [-1.0, 0.0, 0.0],
            ..axes()
        },
        radius: 1.0,
    });
    assert_eq!(
        intersect(&sphere([0.0; 3], 1.0), &skew),
        Err(Error::NoClosedForm)
    );
    assert_eq!(
        intersect(&sphere([0.0; 3], -1.0), &horizontal(0.0)),
        Err(Error::Surface(cs::Error::DegenerateSurface))
    );
    assert_eq!(
        intersect(&sphere([f64::NAN, 0.0, 0.0], 1.0), &horizontal(0.0)),
        Err(Error::Surface(cs::Error::NonFinite))
    );
    let flat = plane([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
    assert_eq!(
        intersect(&flat, &sphere([0.0; 3], 1.0)),
        Err(Error::Surface(cs::Error::DegenerateFrame))
    );
}

// ---------------------------------------------------------------- seeded

/// A line from `p` in a random direction meets the surface at `t = 0`, by T2.
fn t2_meets_at(rng: &mut Rng, s: &SurfaceKind, p: V3) {
    let line = CurveKind::Line(Line {
        origin: dd::round(p),
        direction: gen::direction(rng),
    });
    let cs::Meeting::Points(hits) = cs::intersect(&line, s).unwrap() else {
        return;
    };
    let best = hits
        .iter()
        .map(|h| h.t.to_f64().abs())
        .fold(f64::INFINITY, f64::min);
    // The rounding of p to doubles moves it by ~1e-16, along a line at any angle to s.
    assert!(best < 1e-12, "{s:?} {p:?}: nearest T2 hit at t = {best}");
}

#[test]
fn random_plane_sections_are_on_both_surfaces_and_agree_with_t2() {
    let mut rng = Rng::new(0x7501);
    for i in 0..60 {
        let f = gen::frame(&mut rng, 1.0);
        let q = match i % 3 {
            0 => SurfaceKind::Cylinder(Cylinder {
                frame: f,
                radius: 0.25 + rng.next_f64(),
            }),
            1 => SurfaceKind::Cone(Cone {
                frame: f,
                radius: 0.25 + rng.next_f64(),
                semi_angle: 0.1 + 1.2 * rng.next_f64(),
            }),
            _ => SurfaceKind::Sphere(Sphere {
                frame: f,
                radius: 0.25 + rng.next_f64(),
            }),
        };
        let on = gen::point_on(&mut rng, &q);
        let p = SurfaceKind::Plane(gen::plane_through(&mut rng, on));
        let pieces = match intersect(&q, &p).unwrap() {
            Section::Curves(v) => v,
            other => panic!("{other:?}"),
        };
        check(&q, &p, &pieces, 1e-25);
        for piece in &pieces {
            if let Some(s) = piece.samples.get(1) {
                t2_meets_at(&mut rng, &q, s.point);
                t2_meets_at(&mut rng, &p, s.point);
            }
        }
    }
}

#[test]
fn random_coaxial_and_sphere_pairs_are_on_both_surfaces_and_agree_with_t2() {
    let mut rng = Rng::new(0x7502);
    let mut met = 0;
    for i in 0..60 {
        // A random frame shared at one origin, or an axis frame shifted by a dyadic step.
        let f = if i % 2 == 0 {
            gen::frame(&mut rng, 1.0)
        } else {
            gen::axis_frame(&mut rng)
        };
        let shift = if i % 2 == 0 {
            0.0
        } else {
            (rng.next_signed_unit() * 16.0).round() / 16.0
        };
        let (a, b) = (
            gen::revolved(&mut rng, f),
            gen::revolved(&mut rng, gen::along(f, shift)),
        );
        let pairs = [
            (a, b),
            (
                SurfaceKind::Sphere(Sphere {
                    frame: f,
                    radius: 0.5 + rng.next_f64(),
                }),
                SurfaceKind::Sphere(Sphere {
                    frame: Frame {
                        origin: core::array::from_fn(|j| {
                            f.origin[j] + 0.5 * rng.next_signed_unit()
                        }),
                        ..f
                    },
                    radius: 0.5 + rng.next_f64(),
                }),
            ),
        ];
        for (a, b) in pairs {
            let section = intersect(&a, &b).unwrap();
            let Section::Curves(pieces) = section else {
                continue;
            };
            met += 1;
            check(&a, &b, &pieces, 1e-24);
            for piece in &pieces {
                let s = &piece.samples[piece.samples.len() / 2];
                t2_meets_at(&mut rng, &a, s.point);
                t2_meets_at(&mut rng, &b, s.point);
            }
        }
    }
    assert!(met > 40, "{met}");
}

#[test]
fn near_degenerate_planes_stay_certified() {
    let mut rng = Rng::new(0x7503);
    for i in 0..20 {
        // Within 1e-9 of tangent to a sphere: a small circle, or nothing.
        let gap = if i % 2 == 0 { -1e-9 } else { 1e-9 };
        let (s, p) = gen::near_tangent_sphere_plane(&mut rng, gap);
        let (s, p) = (SurfaceKind::Sphere(s), SurfaceKind::Plane(p));
        match intersect(&s, &p).unwrap() {
            Section::Curves(v) => {
                assert!(gap < 0.0);
                check(&s, &p, &v, 1e-25);
                let (_, a, _) = conic(&v[0]);
                let SurfaceKind::Sphere(sp) = s else { panic!() };
                let expect = (2.0 * sp.radius * 1e-9).sqrt();
                assert!(
                    (norm(a) / expect - 1.0).abs() < 1e-5,
                    "{} {expect}",
                    norm(a)
                );
            }
            Section::Empty => assert!(gap > 0.0),
            other => panic!("{other:?}"),
        }
        // Nearly parallel to a cylinder's axis: a long ellipse, certified to its size.
        let f = gen::frame(&mut rng, 1.0);
        let c = SurfaceKind::Cylinder(Cylinder {
            frame: f,
            radius: 0.5,
        });
        let eps = 1e-6 * (1.0 + rng.next_f64());
        let p = SurfaceKind::Plane(gen::near_axial_plane(&mut rng, f.origin, f.z, eps));
        let v = pieces(&c, &p);
        assert_eq!(kinds(&v), [("ellipse", X)]);
        let (_, a, b) = conic(&v[0]);
        let long = norm(a).max(norm(b));
        assert!(long > 0.1 / eps, "{long}");
        check(&c, &p, &v, 1e-25 * long);
    }
}

// ---------------------------------------------------------------- WR-680

fn r(x: f64) -> Rat {
    Rat::from_f64(x).unwrap()
}

fn rat3(v: [f64; 3]) -> [Rat; 3] {
    v.map(r)
}

/// The rational quadratic `sum w_i B_i(s) P_i / sum w_i B_i(s)` at `s`, exactly.
fn bezier(points: &[[Rat; 3]; 3], weights: &[Rat; 3], s: &Rat) -> [Rat; 3] {
    let b = bernstein();
    let mut num = [Rat::zero(), Rat::zero(), Rat::zero()];
    let mut den = Rat::zero();
    for i in 0..3 {
        let c = &b[i].eval(s) * &weights[i];
        den = &den + &c;
        for j in 0..3 {
            num[j] = &num[j] + &(&c * &points[i][j]);
        }
    }
    num.map(|x| x.checked_div(&den).unwrap())
}

#[test]
fn wr680_the_oblique_cylinder_section_is_exact_on_the_pins_nurbs_cylinder() {
    let mut rng = Rng::new(0x7504);
    let cyl = Cylinder {
        frame: Frame {
            origin: [0.25, -0.5, 1.0],
            ..axes()
        },
        radius: 1.5,
    };
    let tilt = Plane {
        frame: Frame {
            origin: [0.0, 0.0, 1.25],
            x: [1.0, 0.0, 0.375],
            y: [0.25, 1.0, -0.125],
            z: [0.0; 3],
        },
    };
    let spans = cylinder_section(&cyl, &tilt).unwrap();
    let n = {
        let (x, y) = (rat3(tilt.frame.x), rat3(tilt.frame.y));
        [
            &(&x[1] * &y[2]) - &(&x[2] * &y[1]),
            &(&x[2] * &y[0]) - &(&x[0] * &y[2]),
            &(&x[0] * &y[1]) - &(&x[1] * &y[0]),
        ]
    };
    let o = rat3(tilt.frame.origin);
    let generic = pieces(&SurfaceKind::Cylinder(cyl), &SurfaceKind::Plane(tilt));
    assert_eq!(kinds(&generic), [("ellipse", X)]);
    let c = dd::lift(cyl.frame.origin);
    let w = r(core::f64::consts::FRAC_1_SQRT_2);
    for (k, span) in spans.iter().enumerate() {
        assert_eq!(span.weights, [Rat::one(), w.clone(), Rat::one()]);
        assert_eq!(span.knots.0, r(scx_testkit::pin_uv::CYLINDER_U_KNOTS[k]));
        // The pin's NURBS cylinder's net on this span, from its arc and the height.
        let rim: [[Rat; 3]; 3] = core::array::from_fn(|i| {
            let [a, b] = ARCS[k][i];
            let o = rat3(cyl.frame.origin);
            [
                &o[0] + &(&r(cyl.radius) * &Rat::from(a)),
                &o[1] + &(&r(cyl.radius) * &Rat::from(b)),
                o[2].clone(),
            ]
        });
        for j in 0..12 {
            let s = if j == 0 {
                Rat::zero()
            } else {
                r(rng.next_f64())
            };
            let p = bezier(&span.points, &span.weights, &s);
            // On the plane exactly.
            let d = (0..3).fold(Rat::zero(), |acc, i| &acc + &(&(&p[i] - &o[i]) * &n[i]));
            assert!(d.is_zero(), "span {k} s {s:?}: {d:?}");
            // On the pin's surface at (u(s), v(s)) exactly: its rim curve at the span's
            // local parameter, raised by v = v_num / weight along the axis.
            let v = span
                .v_num
                .eval(&s)
                .checked_div(&span.weight.eval(&s))
                .unwrap();
            let base = bezier(&rim, &span.weights, &s);
            let on = [base[0].clone(), base[1].clone(), &base[2] + &v];
            assert_eq!(p, on, "span {k}");
            // And within the weight's rounding of the true section.
            let q = p.each_ref().map(to_dd);
            let lq = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
            let rho = (lq[0] * lq[0] + lq[1] * lq[1]).sqrt().to_f64();
            assert!((rho - 1.5).abs() < 1e-15, "{rho}");
            // u(s) is the pin's u of the point's angle, to that rounding.
            let theta = dd::atan2(lq[1], lq[0]);
            let u = scx_testkit::pin_uv::cylinder_u(theta);
            let us = to_dd(&span.u.eval(&s));
            let wrap = 4.0 * core::f64::consts::PI;
            let du = (u - us).to_f64();
            assert!(du.abs() < 1e-14 || (du.abs() - wrap).abs() < 1e-14, "{du}");
        }
    }
    // A plane along the axis has no elliptic section.
    let along = Plane {
        frame: Frame {
            origin: [0.0; 3],
            x: [0.0, 0.0, 1.0],
            y: [1.0, 0.0, 0.0],
            z: [0.0; 3],
        },
    };
    assert_eq!(cylinder_section(&cyl, &along), Err(Error::NoClosedForm));
}
