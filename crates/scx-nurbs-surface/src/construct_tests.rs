// Written for solidcorex. Apache-2.0.
//! Tests of `iso`, `restrict` and `CompatibleSurfaces` against truth: closed forms (a
//! cylinder's circle, a bilinear patch's line) and the source surface itself.

use scx_num::error::ErrorKind;
use scx_num::extent2d::Extent2d;
use scx_num::hash::Rng;
use scx_nurbs::{Approach, KnotVector, NurbsError};
use scx_testkit::data::SurfaceData;
use scx_testkit::sample::{self, Knots, Spec, Weights};

use crate::{CompatibleSurfaces, Net, NurbsSurface, NurbsSurfaceError, ParamAxis, RestrictCause};

const AXES: [ParamAxis; 2] = [ParamAxis::U, ParamAxis::V];
const ABOVE: [Approach; 2] = [Approach::Above; 2];

fn close(got: &[f64; 3], want: &[f64; 3], tol: f64) -> bool {
    got.iter()
        .zip(want)
        .all(|(g, w)| (g - w).abs() <= tol * (1.0 + w.abs()))
}

fn build(d: &SurfaceData<3>) -> NurbsSurface<3> {
    let u = KnotVector::new(d.degrees.0, d.knots_u.clone()).unwrap();
    let v = KnotVector::new(d.degrees.1, d.knots_v.clone()).unwrap();
    let (rows, cols) = (u.point_count(), v.point_count());
    let points = Net::new(rows, cols, d.points.clone()).unwrap();
    let weights = d.weights.clone().map(|w| Net::new(rows, cols, w).unwrap());
    NurbsSurface::new(u, v, points, weights).unwrap()
}

fn has_break_in(p: usize, knots: &[f64]) -> bool {
    knots
        .iter()
        .filter(|&&k| k > 0.0 && k < 1.0)
        .any(|&k| knots.iter().filter(|&&x| x == k).count() > p)
}

/// Random surfaces without breaks, of the knot kinds given and polynomial or rational.
fn surfaces(seed: u64, kinds: &[Knots]) -> Vec<NurbsSurface<3>> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    for (pu, pv) in [(1, 2), (2, 3), (3, 1), (2, 2)] {
        for &knots in kinds {
            for weights in [Weights::Unit, Weights::Moderate] {
                let spec = |degree| Spec {
                    degree,
                    count: degree + 4,
                    knots,
                };
                let d = sample::surface(&mut rng, (spec(pu), spec(pv)), weights);
                if !has_break_in(pu, &d.knots_u) && !has_break_in(pv, &d.knots_v) {
                    out.push(build(&d));
                }
            }
        }
    }
    out
}

/// The NURBS cylinder of radius `r` and height `h` about the z axis: the rational quadratic
/// circle (nine points, four quarters) in u, a line in v from z = 0 to z = h.
fn cylinder(r: f64, h: f64) -> NurbsSurface<3> {
    let s = core::f64::consts::FRAC_1_SQRT_2;
    let ring = [
        (r, 0.0, 1.0),
        (r, r, s),
        (0.0, r, 1.0),
        (-r, r, s),
        (-r, 0.0, 1.0),
        (-r, -r, s),
        (0.0, -r, 1.0),
        (r, -r, s),
        (r, 0.0, 1.0),
    ];
    let mut points = Vec::new();
    let mut weights = Vec::new();
    for (x, y, w) in ring {
        for z in [0.0, h] {
            points.push([x, y, z]);
            weights.push(w);
        }
    }
    let u = KnotVector::new(
        2,
        vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
    )
    .unwrap();
    let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    NurbsSurface::new(
        u,
        v,
        Net::new(9, 2, points).unwrap(),
        Some(Net::new(9, 2, weights).unwrap()),
    )
    .unwrap()
}

/// The point at `t` of the circle of radius `r` at height `z` that `cylinder` parametrizes:
/// each quarter is the rational quadratic Bezier arc with middle weight `1/sqrt 2`, written
/// out here, the quarter turned by `k` right angles.
fn circle(r: f64, z: f64, t: f64) -> [f64; 3] {
    let k = ((t * 4.0).floor() as usize).min(3);
    let s = t * 4.0 - k as f64;
    let w = core::f64::consts::FRAC_1_SQRT_2;
    let (b0, b1, b2) = ((1.0 - s) * (1.0 - s), 2.0 * s * (1.0 - s) * w, s * s);
    let x = (b0 * r + b1 * r) / (b0 + b1 + b2);
    let y = (b1 * r + b2 * r) / (b0 + b1 + b2);
    // Turn (x, y) by k right angles.
    let (x, y) = (0..k).fold((x, y), |(x, y), _| (-y, x));
    [x, y, z]
}

#[test]
fn the_iso_curve_of_a_cylinder_at_constant_height_is_its_circle() {
    let (r, h) = (2.5, 3.0);
    let cyl = cylinder(r, h);
    // Heights on a knot of v's ends, near an end, and at random, and in between.
    let mut rng = Rng::new(7);
    let mut heights = vec![0.0, 1.0, 0.5, 1e-9, 1.0 - 1e-9];
    heights.extend((0..20).map(|_| rng.next_f64()));
    for t in heights {
        let c = cyl.iso(ParamAxis::V, t).unwrap();
        // The curve is the circle's own: the same knots and degree, rational.
        assert_eq!(c.degree(), 2);
        assert_eq!(c.knots(), cyl.knots(ParamAxis::U));
        assert!(c.is_rational());
        for i in 0..=2000 {
            let u = f64::from(i) / 2000.0;
            let got = c.point(u, Approach::Above).unwrap();
            let want = circle(r, h * t, u);
            assert!(close(&got, &want, 1e-14), "t {t} u {u}: {got:?} {want:?}");
            let radius = got[0].hypot(got[1]);
            assert!((radius - r).abs() <= 1e-14 * r, "radius {radius} at {u}");
        }
    }
}

#[test]
fn the_iso_curve_of_a_cylinder_across_the_circle_is_a_line() {
    let cyl = cylinder(2.0, 3.0);
    // 0.25 is a knot of multiplicity two (the degree): no insertion. 0.1 needs two.
    for t in [0.0, 0.1, 0.25, 0.6, 1.0] {
        let c = cyl.iso(ParamAxis::U, t).unwrap();
        assert_eq!((c.degree(), c.points().len()), (1, 2), "t {t}");
        let want = circle(2.0, 0.0, t);
        for (p, z) in c.points().iter().zip([0.0, 3.0]) {
            assert!(close(p, &[want[0], want[1], z], 1e-14), "t {t}: {p:?}");
        }
    }
}

#[test]
fn the_iso_curve_of_a_bilinear_patch_is_a_line() {
    let p = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 2.0]],
        [[3.0, 1.0, 0.5], [4.0, 5.0, -1.0]],
    ];
    let k = || KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let net = Net::new(2, 2, vec![p[0][0], p[0][1], p[1][0], p[1][1]]).unwrap();
    let s = NurbsSurface::new(k(), k(), net, None).unwrap();
    let lerp = |a: [f64; 3], b: [f64; 3], t: f64| [0, 1, 2].map(|i| a[i] + t * (b[i] - a[i]));
    for t in [0.0, 0.125, 0.5, 0.9, 1.0] {
        // v -> S(t, v): from the u-lerp of the v = 0 edge to that of the v = 1 edge.
        let c = s.iso(ParamAxis::U, t).unwrap();
        assert_eq!(
            (c.degree(), c.knots()),
            (1, [0.0, 0.0, 1.0, 1.0].as_slice())
        );
        assert!(!c.is_rational());
        let ends = [lerp(p[0][0], p[1][0], t), lerp(p[0][1], p[1][1], t)];
        assert!(
            c.points()
                .iter()
                .zip(&ends)
                .all(|(a, b)| close(a, b, 1e-15))
        );
        // u -> S(u, t).
        let c = s.iso(ParamAxis::V, t).unwrap();
        let ends = [lerp(p[0][0], p[0][1], t), lerp(p[1][0], p[1][1], t)];
        assert!(
            c.points()
                .iter()
                .zip(&ends)
                .all(|(a, b)| close(a, b, 1e-15))
        );
        // And bilinear: a point of the curve is a point of the patch.
        for v in [0.0, 0.3, 1.0] {
            let got = c.point(v, Approach::Above).unwrap();
            assert!(close(&got, &s.point([v, t], ABOVE).unwrap(), 1e-14));
        }
    }
}

#[test]
fn iso_curves_of_random_surfaces_lie_on_them() {
    let mut rng = Rng::new(11);
    let mut curves = 0;
    for s in surfaces(
        21,
        &[
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ],
    ) {
        for axis in AXES {
            let (other, ks) = match axis {
                ParamAxis::U => (ParamAxis::V, s.knots(ParamAxis::U).to_vec()),
                ParamAxis::V => (ParamAxis::U, s.knots(ParamAxis::V).to_vec()),
            };
            for _ in 0..6 {
                let t = sample::parameter(&mut rng, &ks, (0.0, 1.0));
                let c = s.iso(axis, t).unwrap();
                assert_eq!(c.knots(), s.knots(other));
                assert_eq!(c.is_rational(), s.is_rational());
                for _ in 0..40 {
                    let w = sample::parameter(&mut rng, s.knots(other), (0.0, 1.0));
                    let uv = match axis {
                        ParamAxis::U => [t, w],
                        ParamAxis::V => [w, t],
                    };
                    let got = c.point(w, Approach::Above).unwrap();
                    let want = s.point(uv, ABOVE).unwrap();
                    assert!(
                        close(&got, &want, 1e-11),
                        "{axis} {t} {w}: {got:?} {want:?}"
                    );
                }
                curves += 1;
            }
        }
    }
    assert!(curves > 100, "{curves}");
}

#[test]
fn iso_refuses_with_its_cause_and_changes_nothing() {
    let s = surfaces(5, &[Knots::Uniform]).remove(0);
    let before = s.clone();
    for t in [-0.5, 1.5, f64::NAN, f64::INFINITY] {
        for axis in AXES {
            let e = s.iso(axis, t).unwrap_err();
            assert!(
                matches!(e, NurbsSurfaceError::Edit { axis: a, .. } if a == axis),
                "{e:?}"
            );
            assert_eq!(e.kind(), ErrorKind::Refused);
        }
    }
    // An unclamped end cannot be reached by insertion.
    let k = KnotVector::new(2, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
    let flat = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let pts: Vec<[f64; 3]> = (0..6).map(|i| [f64::from(i), 0.0, 0.0]).collect();
    let u = NurbsSurface::new(k, flat, Net::new(3, 2, pts).unwrap(), None).unwrap();
    assert!(matches!(
        u.iso(ParamAxis::U, 2.0),
        Err(NurbsSurfaceError::Edit {
            cause: NurbsError::Unclamped,
            ..
        })
    ));
    assert!(u.iso(ParamAxis::U, 2.5).is_ok());
    assert_eq!(s, before);
}

#[test]
fn restrict_agrees_with_the_source_on_its_box() {
    let mut rng = Rng::new(31);
    let mut boxes = 0;
    for s in surfaces(
        41,
        &[
            Knots::Uniform,
            Knots::Random,
            Knots::Clustered,
            Knots::Multiple,
        ],
    ) {
        let interval = |rng: &mut Rng, axis| loop {
            let ks = s.knots(axis);
            let a = sample::parameter(rng, ks, (0.0, 1.0));
            let b = sample::parameter(rng, ks, (0.0, 1.0));
            if (a - b).abs() > 1e-3 {
                break (a.min(b), a.max(b));
            }
        };
        let mut cases = vec![
            (0.0, 1.0, 0.0, 1.0),
            (0.0, 0.5, 0.25, 1.0),
            (0.3, 1.0, 0.0, 0.6),
        ];
        for _ in 0..8 {
            let (u0, u1) = interval(&mut rng, ParamAxis::U);
            let (v0, v1) = interval(&mut rng, ParamAxis::V);
            cases.push((u0, u1, v0, v1));
        }
        for (u0, u1, v0, v1) in cases {
            let b = Extent2d::from_bounds(u0, v0, u1, v1).unwrap();
            let r = s.restrict(&b).unwrap();
            assert!(r.domain().are_same(&b), "{u0} {u1} {v0} {v1}");
            assert_eq!(r.is_rational(), s.is_rational());
            // Same degrees; end knots of full multiplicity.
            for axis in AXES {
                assert_eq!(r.degree(axis), s.degree(axis));
                assert!(r.knot_vector(axis).is_clamped());
            }
            for i in 0..=120 {
                let t = f64::from(i) / 120.0;
                // The corners and edges, and interior points.
                let uv = [u0 + t * (u1 - u0), v0 + (1.0 - t * t) * (v1 - v0)];
                let got = r.point(uv, ABOVE).unwrap();
                let want = s.point(uv, ABOVE).unwrap();
                assert!(close(&got, &want, 1e-10), "{uv:?}: {got:?} {want:?}");
            }
            boxes += 1;
        }
    }
    assert!(boxes > 100, "{boxes}");
}

#[test]
fn restricting_to_the_domain_is_the_surface() {
    for s in surfaces(51, &[Knots::Random]) {
        assert_eq!(s.restrict(&s.domain()).unwrap(), s);
    }
}

#[test]
fn restrict_refuses_what_it_cannot_do_and_changes_nothing() {
    let s = surfaces(5, &[Knots::Uniform]).remove(0);
    let before = s.clone();
    let b = |u0, v0, u1, v1| Extent2d::from_bounds(u0, v0, u1, v1).unwrap();
    assert!(matches!(
        s.restrict(&b(-0.1, 0.0, 0.5, 1.0)),
        Err(NurbsSurfaceError::Restrict(RestrictCause::Outside {
            axis: ParamAxis::U,
            ..
        }))
    ));
    assert!(matches!(
        s.restrict(&b(0.0, 0.2, 0.5, 1.5)),
        Err(NurbsSurfaceError::Restrict(RestrictCause::Outside {
            axis: ParamAxis::V,
            ..
        }))
    ));
    let e = s.restrict(&b(0.0, 0.3, 0.5, 0.3)).unwrap_err();
    assert!(matches!(
        e,
        NurbsSurfaceError::Restrict(RestrictCause::Empty {
            axis: ParamAxis::V,
            ..
        })
    ));
    assert_eq!(e.kind(), ErrorKind::Refused);
    assert_eq!(s, before);
}

/// Shifts and scales the knots of `axis` of `s`: the same surface on another domain.
fn rescaled(s: &NurbsSurface<3>, axis: ParamAxis, from: f64, by: f64) -> NurbsSurface<3> {
    let map = |kv: &KnotVector| {
        KnotVector::new(
            kv.degree(),
            kv.knots().iter().map(|&k| from + by * k).collect(),
        )
        .unwrap()
    };
    let (u, v) = match axis {
        ParamAxis::U => (
            map(s.knot_vector(ParamAxis::U)),
            s.knot_vector(ParamAxis::V).clone(),
        ),
        ParamAxis::V => (
            s.knot_vector(ParamAxis::U).clone(),
            map(s.knot_vector(ParamAxis::V)),
        ),
    };
    NurbsSurface::new(u, v, s.points().clone(), s.weights().cloned()).unwrap()
}

#[test]
fn compatible_surfaces_evaluate_as_their_sources() {
    let all = surfaces(61, &[Knots::Uniform, Knots::Random, Knots::Multiple]);
    let mut rng = Rng::new(71);

    let mut groups = 0;
    for axis in AXES {
        for group in all.chunks(3).filter(|g| g.len() == 3) {
            // Different degrees, knots, weights and domains along the axis.
            let sources = [
                group[0].clone(),
                rescaled(&group[1], axis, -2.0, 3.0),
                rescaled(&group[2], axis, 5.0, 0.5),
            ];
            let before = sources.clone();
            let c = CompatibleSurfaces::from_surfaces(axis, &sources).unwrap();
            assert_eq!(sources, before, "inputs are not changed");
            assert_eq!((c.axis(), c.len()), (axis, 3));
            let degree = sources.iter().map(|s| s.degree(axis)).max().unwrap();
            let other = |a: ParamAxis| {
                if a == ParamAxis::U {
                    ParamAxis::V
                } else {
                    ParamAxis::U
                }
            };
            let (a, b) = c.knot_vector().ends();
            for (s, out) in sources.iter().zip(c.surfaces()) {
                assert_eq!(out.knots(axis), c.knot_vector().knots());
                assert_eq!(out.degree(axis), degree);
                assert_eq!(out.knots(other(axis)), s.knots(other(axis)));
                assert_eq!(
                    out.is_rational(),
                    sources.iter().any(NurbsSurface::is_rational)
                );
                let (sa, sb) = s.knot_vector(axis).ends();
                for _ in 0..200 {
                    let t = sample::parameter(&mut rng, c.knot_vector().knots(), (a, b));
                    let w = sample::parameter(&mut rng, s.knots(other(axis)), (0.0, 1.0));
                    let ts = sa + (t - a) / (b - a) * (sb - sa);
                    let (got_uv, want_uv) = match axis {
                        ParamAxis::U => ([t, w], [ts, w]),
                        ParamAxis::V => ([w, t], [w, ts]),
                    };
                    let got = out.point(got_uv, ABOVE).unwrap();
                    let want = s.point(want_uv, ABOVE).unwrap();
                    assert!(
                        close(&got, &want, 1e-9),
                        "{axis} {got_uv:?}: {got:?} {want:?}"
                    );
                }
            }
            groups += 1;
        }
    }
    assert!(groups >= 4, "{groups}");
}

#[test]
fn surfaces_that_already_share_their_knots_come_back_unchanged() {
    let s: Vec<_> = surfaces(81, &[Knots::Random])
        .into_iter()
        .filter(|s| !s.is_rational())
        .collect();
    for axis in AXES {
        let one = &s[0];
        // Another surface on the same knots along the axis.
        let twin = one.transformed(&scx_num::math::Affine::IDENTITY).unwrap();
        let c = CompatibleSurfaces::from_surfaces(axis, &[one.clone(), twin]).unwrap();
        assert_eq!(c.surfaces(), [one.clone(), one.clone()]);
    }
}

#[test]
fn compatible_surfaces_refuse_with_their_cause() {
    assert_eq!(
        CompatibleSurfaces::<3>::from_surfaces(ParamAxis::U, &[]),
        Err(NurbsSurfaceError::NoSurfaces)
    );
    // Unclamped knots that differ cannot be made equal by insertion.
    let k = KnotVector::new(2, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
    let flat = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let pts: Vec<[f64; 3]> = (0..6).map(|i| [f64::from(i), 0.0, 0.0]).collect();
    let u = NurbsSurface::new(k, flat, Net::new(3, 2, pts).unwrap(), None).unwrap();
    let other = surfaces(5, &[Knots::Uniform]).remove(0);
    assert!(matches!(
        CompatibleSurfaces::from_surfaces(ParamAxis::U, &[u, other]),
        Err(NurbsSurfaceError::Edit {
            axis: ParamAxis::U,
            cause: NurbsError::Unclamped
        })
    ));
}
