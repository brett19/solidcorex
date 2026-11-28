// Written for solidcorex. Apache-2.0.
//! `radial` (research 75 MK13): the pin's `radial_sweeps_close_at_the_hinge_or_on_two_annular_caps`
//! (a cylinder of radius 10 and height 20 drafted 1 in 10 about the plane `z = 0`, hinged at
//! the base, and about `z = -10`, capped at both ends) and
//! `a_cylinder_whose_top_rounds_contracts_with_a_cap_on_its_rims_plane` as outcomes against the
//! closed-form truth, the difference of the frusta: volume, area and centroid read by
//! `scx_mass::mass`, with each tolerance checked against the estimate's error bound; the face
//! kinds and counts; the BR8 oracle; membership against the CSG difference of the two
//! frusta; and the refusals.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use std::f64::consts::PI;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, FaceSide, UvSide};
use scx_make::{Dimension, MakeError, RadialCone, RadialFaces, Refusal, radial};
use scx_mass::{Mass, MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;
use scx_testkit::analytic::Frame as TruthFrame;
use scx_testkit::bodies::check::{Tolerance, violations};
use scx_testkit::csg::{Csg, Primitive, Side};
use scx_testkit::solid::{self, Solid};

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn doc() -> Document {
    Document::default()
}

fn at(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

fn tilted(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.3, -0.5, 0.8),
        Vec3::new(1.0, 0.2, 0.1),
    )
    .unwrap()
}

fn truth_frame(f: &Frame) -> TruthFrame {
    TruthFrame {
        origin: f.origin().to_array(),
        x: f.x().to_array(),
        y: f.y().to_array(),
        z: f.z().to_array(),
    }
}

fn spec(frame: Frame, h: f64, outer: [f64; 2], inner: [f64; 2]) -> RadialCone {
    RadialCone {
        frame,
        height: mm(h),
        outer: outer.map(mm),
        inner: inner.map(mm),
    }
}

/// The closed-form truth: the outer frustum less the inner one; the area is the two lateral
/// areas and the annuli (an end where the radii are equal has none).
fn truth(h: f64, o: [f64; 2], i: [f64; 2]) -> Solid {
    let big = solid::cone(o[0], o[1], h).unwrap();
    let small = solid::cone(i[0], i[1], h).unwrap();
    let lateral = |r: [f64; 2]| PI * (r[0] + r[1]) * (h * h + (r[0] - r[1]) * (r[0] - r[1])).sqrt();
    let annuli = PI * (o[0] * o[0] - i[0] * i[0]) + PI * (o[1] * o[1] - i[1] * i[1]);
    Solid {
        moments: big.moments.minus(&small.moments),
        area: lateral(o) + lateral(i) + annuli,
    }
}

fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().len(), b.edges().len(), b.vertices().len())
}

/// A closed one-solid body: two regions with a shell each, every edge with two fins of
/// opposite senses, every face's front in the outside and its back in the solid, exact
/// pcurves, the Euler-Poincare count of a solid torus's genus 1 (a hinge's wall faces have
/// one loop, a cap's two), and the BR8 oracle silent.
fn assert_closed_solid(b: &Brep) {
    assert_eq!(b.regions().len(), 2);
    assert_eq!(b.shells().len(), 2);
    let outside = b.outside().shells().next().unwrap().id();
    for e in b.edges() {
        assert_ne!(*e.shape(), EdgeShape::Collapsed);
        let f: Vec<_> = e.fins().collect();
        assert_eq!(f.len(), 2, "{:?}", e.id());
        assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
    }
    let mut loops = 0;
    for face in b.faces() {
        assert_eq!(face.shell(FaceSide::Front).id(), outside);
        assert!(face.shell(FaceSide::Back).region().is_solid());
        loops += face.loops().len();
        for u in face.uses() {
            assert!(u.bound().is_none(), "{:?}", face.id());
        }
    }
    let (f, e, v) = counts(b);
    let euler = v as i64 - e as i64 + f as i64 - (loops as i64 - f as i64);
    assert_eq!(euler, 0, "V - E + F - (L - F) of a solid torus");
    let bad = violations(&b.edit(), Tolerance::default());
    assert!(bad.is_empty(), "{bad:?}");
}

fn measure(b: &Brep, o: Point3) -> Mass {
    let spec = MassSpec::new(RelAccuracy::new(1e-13).unwrap()).with_about(o);
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn assert_within(got: f64, error: f64, want: f64, tol: f64, what: &str) {
    let allowed = tol * want.abs();
    assert!(
        (got - want).abs() <= allowed && error <= allowed,
        "{what}: {got:e} (estimated error {error:e}) vs {want:e}, allowed {allowed:e}"
    );
}

fn assert_mass(b: &Brep, frame: &Frame, t: &Solid, scale: f64, tol: f64, what: &str) {
    let placed = t.place(&truth_frame(frame));
    let m = measure(b, frame.origin());
    assert_within(m.volume, m.error.volume, placed.volume(), tol, what);
    assert_within(m.area, m.error.area, placed.area, tol, what);
    let got = m.centroid().unwrap().to_array();
    let want = placed.moments.centroid();
    for k in 0..3 {
        assert!(
            (got[k] - want[k]).abs() <= tol * scale,
            "{what}: centroid {got:?} vs {want:?}"
        );
    }
}

fn leaf(frame: &Frame, r: [f64; 2], h: f64) -> Csg {
    Csg::leaf(
        &truth_frame(frame),
        Primitive::Frustum {
            radius0: r[0],
            radius1: r[1],
            height: h,
        },
    )
    .unwrap()
}

/// Each face sampled in uv: on it the oracle says `Band`, a step along the front normal is
/// `Out` and one back is `In`.
fn assert_membership(b: &Brep, oracle: &Csg, d: f64, what: &str) {
    let delta = d * 1e-3;
    for face in b.faces() {
        let s = face.surface();
        let uv = face.uv_box().unwrap();
        let (iu, iv) = (uv.x_interval(), uv.y_interval());
        let mut seen = 0;
        for i in 1..12 {
            for j in 1..12 {
                let p2 = Point2::new(
                    iu.min() + iu.length() * (f64::from(i) + 0.37) / 13.0,
                    iv.min() + iv.length() * (f64::from(j) + 0.41) / 13.0,
                );
                if face.contains_uv(p2) != Some(UvSide::Inside) {
                    continue;
                }
                seen += 1;
                let p = s.point(p2);
                let n: Vec3 = *s.normal(p2, [Approach::Above; 2]).unwrap();
                let side = |q: Point3| oracle.side(q.to_array(), delta);
                assert_eq!(side(p), Side::Band, "{what} {:?} at {p2:?}", face.id());
                assert_eq!(
                    side(p + n * d),
                    Side::Out,
                    "{what} {:?} {p:?} {n:?}",
                    face.id()
                );
                assert_eq!(
                    side(p - n * d),
                    Side::In,
                    "{what} {:?} {p:?} {n:?}",
                    face.id()
                );
            }
        }
        assert!(seen >= 3, "{what} {:?}: {seen} samples", face.id());
    }
}

/// The face kinds by role: the walls are cylinders when their radii are equal and cones
/// otherwise, the caps planes.
fn assert_kinds(b: &Brep, f: &RadialFaces, o: [f64; 2], i: [f64; 2]) {
    let is_cyl = |id, r: [f64; 2]| {
        let s = b.face(id).unwrap().surface();
        if r[0] == r[1] {
            matches!(s, Surface::Cylinder(_))
        } else {
            matches!(s, Surface::Cone(_))
        }
    };
    assert!(is_cyl(f.outer, o));
    assert!(is_cyl(f.inner, i));
    for cap in [f.bottom, f.top].into_iter().flatten() {
        assert!(matches!(b.face(cap).unwrap().surface(), Surface::Plane(_)));
        assert_eq!(b.face(cap).unwrap().loops().len(), 2);
    }
    for w in [f.outer, f.inner] {
        assert_eq!(b.face(w).unwrap().loops().len(), 1);
    }
}

/// One spec at a frame: structure, kinds, closed form, membership.
fn check(frame: &Frame, h: f64, o: [f64; 2], i: [f64; 2], what: &str) {
    let m = radial(&spec(*frame, h, o, i), &doc()).unwrap();
    let hinges = usize::from(o[0] == i[0]) + usize::from(o[1] == i[1]);
    // Walls, caps; rims, seams; one vertex per rim.
    let want = (4 - hinges, 6 - hinges, 4 - hinges);
    assert_eq!(counts(&m.body), want, "{what}");
    assert_eq!(m.faces.bottom.is_some(), o[0] != i[0], "{what}");
    assert_eq!(m.faces.top.is_some(), o[1] != i[1], "{what}");
    assert_closed_solid(&m.body);
    assert_kinds(&m.body, &m.faces, o, i);
    assert_mass(&m.body, frame, &truth(h, o, i), h, 1e-12, what);
    let oracle = leaf(frame, o, h).difference(leaf(frame, i, h));
    assert_membership(&m.body, &oracle, 1e-3, what);
}

/// The pin's first test: the cylinder of radius 10 and height 20 less its draft of 1 in 10
/// about `z = 0` (radii 10 and 8, hinged at the base) and about `z = -10` (9 and 7, a cap at
/// each end), whose volumes are `pi h (300 - b^2 - b t - t^2) / 3`.
#[test]
fn radial_sweeps_close_at_the_hinge_or_on_two_annular_caps() {
    for frame in [at([0.0; 3]), tilted([120.0, -40.0, 7.5])] {
        for (neutral, inner) in [(0.0, [10.0, 8.0]), (-10.0, [9.0, 7.0])] {
            let what = format!("neutral {neutral}");
            check(&frame, 20.0, [10.0, 10.0], inner, &what);
            let expected = PI
                * 20.0
                * (300.0 - inner[0] * inner[0] - inner[0] * inner[1] - inner[1] * inner[1])
                / 3.0;
            assert!(
                (truth(20.0, [10.0, 10.0], inner).volume() - expected).abs() <= 1e-12 * expected
            );
            let m = radial(&spec(frame, 20.0, [10.0, 10.0], inner), &doc()).unwrap();
            let got = measure(&m.body, frame.origin()).volume;
            assert!(
                (got - expected).abs() <= 1e-12 * expected,
                "{what}: {got} vs {expected}"
            );
        }
    }
}

/// The pin's second test: a cylinder of radius 1 and height 0.7 standing on z = 0.3 (its top
/// rim's centre rounds), less its draft of radii 1 and 0.93: the volume is the cylinder less
/// the frustum, `pi h (3 - (1 + 0.93 + 0.93^2)) / 3`, closed by one annular cap at the top.
#[test]
fn a_cylinder_whose_top_rounds_has_a_cap_on_its_rims_plane() {
    let frame = at([0.1, 0.2, 0.3]);
    let m = radial(&spec(frame, 0.7, [1.0, 1.0], [1.0, 0.93]), &doc()).unwrap();
    let expected = PI * 0.7 * (3.0 - (1.0 + 0.93 + 0.93 * 0.93)) / 3.0;
    let got = measure(&m.body, frame.origin());
    assert_within(got.volume, got.error.volume, expected, 1e-12, "volume");
    assert!(m.faces.bottom.is_none() && m.faces.top.is_some());
    assert_closed_solid(&m.body);
    check(&frame, 0.7, [1.0, 1.0], [1.0, 0.93], "rounding top");
}

/// Cones outside and inside, widening and narrowing, hinged at either end or capped at both,
/// and a hollow cylinder (a tube).
#[test]
fn cones_hinged_at_either_end_or_capped_at_both() {
    for frame in [at([0.0; 3]), tilted([-30.0, 12.0, 250.0])] {
        for (o, i, what) in [
            ([8.0, 3.0], [6.0, 1.0], "narrowing, both caps"),
            ([3.0, 8.0], [1.0, 6.0], "widening, both caps"),
            ([8.0, 3.0], [8.0, 1.0], "hinge at the base"),
            ([8.0, 3.0], [5.0, 3.0], "hinge at the top"),
            ([5.0, 5.0], [4.0, 4.0], "tube"),
            ([5.0, 5.0], [5.0, 2.0], "cylinder, inner cone hinged"),
            ([8.0, 3.0], [2.0, 2.0], "inner cylinder in a cone"),
            ([8.0, 3.0], [1.0, 2.9], "crossing slopes"),
        ] {
            check(&frame, 12.0, o, i, what);
        }
    }
}

/// The same inputs make the same body, bit for bit.
#[test]
fn making_is_deterministic() {
    let s = spec(tilted([1.0, 2.0, 3.0]), 9.0, [7.0, 4.0], [5.0, 2.0]);
    let (a, b) = (radial(&s, &doc()).unwrap(), radial(&s, &doc()).unwrap());
    assert_eq!(a.faces, b.faces);
    let bits = |m: &scx_make::Made<RadialFaces>| -> Vec<[u64; 3]> {
        m.body
            .vertices()
            .map(|v| v.point().to_array().map(f64::to_bits))
            .collect()
    };
    assert_eq!(bits(&a), bits(&b));
}

fn refused(c: RadialCone) -> MakeError {
    let e = radial(&c, &doc()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Refused, "{e}");
    e
}

/// Sizes not finite and above the resolution are refused naming the size; an inner cone
/// that is not inside the outer one with a wall between is an `Annulus` refusal.
#[test]
fn bad_sizes_and_annuli_are_refused() {
    let f = Frame::WORLD;
    let good = spec(f, 10.0, [5.0, 5.0], [3.0, 3.0]);
    assert!(radial(&good, &doc()).is_ok());
    let zone = doc().resolution().zone().mm();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, zone] {
        for (dimension, change) in [
            (
                Dimension::Height,
                RadialCone {
                    height: mm(bad),
                    ..good
                },
            ),
            (
                Dimension::BaseRadius,
                RadialCone {
                    outer: [mm(bad), mm(5.0)],
                    ..good
                },
            ),
            (
                Dimension::TopRadius,
                RadialCone {
                    outer: [mm(5.0), mm(bad)],
                    ..good
                },
            ),
            (
                Dimension::InnerBaseRadius,
                RadialCone {
                    inner: [mm(bad), mm(3.0)],
                    ..good
                },
            ),
            (
                Dimension::InnerTopRadius,
                RadialCone {
                    inner: [mm(3.0), mm(bad)],
                    ..good
                },
            ),
        ] {
            let e = refused(change);
            let MakeError::Refused(Refusal::Size { dimension: d, .. }) = e else {
                panic!("{e:?}")
            };
            assert_eq!(d, dimension);
        }
    }
    for inner in [
        [5.0, 6.0],              // inner above outer at the top
        [6.0, 3.0],              // and at the base
        [5.0, 5.0],              // both ends hinged: no volume
        [5.0 - zone / 2.0, 3.0], // a wall thinner than the resolution at the base
    ] {
        let e = refused(spec(f, 10.0, [5.0, 5.0], inner));
        assert_eq!(e.name(), "annulus", "{inner:?}");
    }
}
