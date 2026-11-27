// Written for solidcorex. Apache-2.0.
//! `cylinder`, `cone`, `sphere` and `torus` (research 72 MK2): the pin's
//! `primitives_are_closed_solids_with_analytic_properties`,
//! `complete_spheres_and_tori_have_analytic_properties` and
//! `precise_sphere_properties_scale_and_translate` as outcomes against closed-form truth
//! (`scx_testkit::solid`, T9), point membership of every face against the CSG oracle
//! (`scx_testkit::csg`, T10), the BR8 oracle on each body, the five
//! `primitive_infinite_region_searches` as one test that each region is the one built, and
//! the size, torus and cone refusals of `api_primitives`.
//!
//! Volume, area and centroid are read by `scx_mass::mass` (relative accuracy `1e-13`) and
//! compared to the closed forms; each tolerance is also checked against the Estimate's error
//! bound, so the bound supports it.

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
use scx_make::{
    ConeFaces, Dimension, MakeError, Refusal, Unsupported, block, cone, cylinder, sphere, torus,
};
use scx_mass::{Mass, MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::frame::Frame;
use scx_num::units::{Length, Resolution};
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

/// The frame at `o` with the world's axes.
fn at(o: [f64; 3]) -> Frame {
    Frame::new(
        Point3::new(o[0], o[1], o[2]),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

/// A frame turned off every world axis, at `o`.
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

/// Faces, edges and vertices.
fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().len(), b.edges().len(), b.vertices().len())
}

/// Collapsed edges.
fn collapsed(b: &Brep) -> usize {
    b.edges()
        .filter(|e| *e.shape() == EdgeShape::Collapsed)
        .count()
}

/// What a closed one-solid body is: two regions with a shell each, every curve edge with
/// exactly two fins of opposite senses and every collapsed one with one, every face's front in the outside's shell and its back in
/// the solid's, every pcurve exact (no recorded bound), the Euler-Poincare count of a closed
/// surface of `genus` over the curve edges (a collapsed edge is a pole, not a side), and the
/// BR8 oracle silent.
fn assert_closed_solid(b: &Brep, genus: i64) {
    assert_closed_with(b, genus, Tolerance::default());
}

/// [`assert_closed_solid`] for a body at distance `c` from the origin: the BR8 oracle's
/// tolerances grow to a few ulps of `c` (its defaults, `1e-7` mm and `1e-9` in uv, are
/// below one ulp at 1e8), since a vertex rounded there moves its pcurve ends as much.
fn assert_closed_far(b: &Brep, genus: i64, c: f64) {
    let ulps = 8.0 * f64::EPSILON * c;
    let d = Tolerance::default();
    let tol = Tolerance {
        space: d.space.max(ulps),
        uv: d.uv.max(ulps),
    };
    assert_closed_with(b, genus, tol);
}

fn assert_closed_with(b: &Brep, genus: i64, tol: Tolerance) {
    assert_eq!(b.regions().len(), 2);
    assert_eq!(b.shells().len(), 2);
    let outside = b.outside().shells().next().unwrap().id();
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        if *e.shape() == EdgeShape::Collapsed {
            // A pole is used once, by the face round it.
            assert_eq!(f.len(), 1, "{:?}", e.id());
        } else {
            assert_eq!(f.len(), 2, "{:?}", e.id());
            assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
        }
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
    let e = e - collapsed(b);
    let euler = v as i64 - e as i64 + f as i64 - (loops as i64 - f as i64);
    assert_eq!(euler, 2 - 2 * genus, "V - E + F - (L - F)");
    let bad = violations(&b.edit(), tol);
    assert!(bad.is_empty(), "{bad:?}");
}

/// The body's mass properties about `o`, by `scx_mass::mass` at relative accuracy `1e-13`.
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

/// `got` within `tol` of `want` (a relative size `want` times `tol`, or `tol` absolute for a
/// zero `want`), and its estimated error within the same: the estimate, not only the
/// outcome, supports the tolerance.
fn assert_within(got: f64, error: f64, want: f64, tol: f64, what: &str) {
    let allowed = tol * want.abs().max(1e-300);
    assert!(
        (got - want).abs() <= allowed && error <= allowed,
        "{what}: {got:e} (estimated error {error:e}) vs {want:e}, allowed {allowed:e}"
    );
}

/// Checks the body against `truth` (in its standard placement) carried to `frame`: volume
/// and area to `tol` relative, each with `scx_mass`'s error estimate within the same, and
/// the centroid to `tol` of the body's size `scale`.
fn assert_mass(b: &Brep, frame: &Frame, truth: &Solid, scale: f64, tol: f64, what: &str) {
    let placed = truth.place(&truth_frame(frame));
    let m = measure(b, frame.origin());
    assert_within(
        m.volume,
        m.error.volume,
        placed.volume(),
        tol,
        &format!("{what}: volume"),
    );
    assert_within(
        m.area,
        m.error.area,
        placed.area,
        tol,
        &format!("{what}: area"),
    );
    let got = m.centroid().unwrap().to_array();
    let want = placed.moments.centroid();
    for i in 0..3 {
        // The centroid's error is the first moment's error and the volume's, over the volume.
        let err = [m.error.first.x, m.error.first.y, m.error.first.z][i] / m.volume
            + (got[i] - m.about.to_array()[i]).abs() * m.error.volume / m.volume;
        assert!(
            (got[i] - want[i]).abs() <= tol * scale && err <= tol * scale,
            "{what}: centroid {got:?} (estimated error {err:e}) vs {want:?}"
        );
    }
}

/// Each face sampled in uv; on it the oracle says `Band`, and a step `d` along the front
/// normal is `Out` and one back is `In`.
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
                    iu.min() + iu.length() * f64::from(i) / 12.0,
                    iv.min() + iv.length() * f64::from(j) / 12.0,
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
                    "{what} {:?} at {p2:?}",
                    face.id()
                );
                assert_eq!(
                    side(p - n * d),
                    Side::In,
                    "{what} {:?} at {p2:?}",
                    face.id()
                );
            }
        }
        assert!(seen > 20, "{what} {:?}: {seen} samples", face.id());
    }
}

fn leaf(frame: &Frame, p: Primitive) -> Csg {
    Csg::leaf(&truth_frame(frame), p).unwrap()
}

/// The pin's specs (cylinder r5 h20, cone r5 to 2 h20, at the origin), the two cones to an
/// apex, and each again at a tilted frame far from the origin: counts, the closed-solid
/// structure, volume, area and centroid against T9, and membership against T10.
#[test]
fn primitives_are_closed_solids_with_analytic_properties() {
    for frame in [at([0.0; 3]), tilted([120.0, -40.0, 7.5])] {
        let m = cylinder(&frame, mm(5.0), mm(20.0), &doc()).unwrap();
        assert_eq!(counts(&m.body), (3, 3, 2));
        assert_eq!(collapsed(&m.body), 0);
        assert_closed_solid(&m.body, 0);
        let truth = solid::cylinder(5.0, 20.0).unwrap();
        assert_mass(&m.body, &frame, &truth, 20.0, 1e-12, "cylinder");
        let csg = leaf(
            &frame,
            Primitive::Cylinder {
                radius: 5.0,
                height: 20.0,
            },
        );
        assert_membership(&m.body, &csg, 0.05, "cylinder");
        let wall = m.body.face(m.faces.wall).unwrap();
        assert!(matches!(wall.surface(), Surface::Cylinder(_)));

        for (r0, r1, f, e, v, caps) in [
            (5.0, 2.0, 3, 3, 2, (true, true)),
            (5.0, 0.0, 2, 3, 2, (true, false)),
            (0.0, 5.0, 2, 3, 2, (false, true)),
            (2.0, 5.0, 3, 3, 2, (true, true)),
        ] {
            let what = format!("cone {r0} to {r1}");
            let m = cone(&frame, [mm(r0), mm(r1)], mm(20.0), &doc()).unwrap();
            assert_eq!(counts(&m.body), (f, e, v), "{what}");
            let apexes = usize::from(r0 == 0.0) + usize::from(r1 == 0.0);
            assert_eq!(collapsed(&m.body), apexes, "{what}");
            assert_eq!(
                (m.faces.bottom.is_some(), m.faces.top.is_some()),
                caps,
                "{what}"
            );
            assert_closed_solid(&m.body, 0);
            let truth = solid::cone(r0, r1, 20.0).unwrap();
            assert_mass(&m.body, &frame, &truth, 20.0, 1e-12, &what);
            let csg = leaf(
                &frame,
                Primitive::Frustum {
                    radius0: r0,
                    radius1: r1,
                    height: 20.0,
                },
            );
            assert_membership(&m.body, &csg, 0.05, &what);
        }
    }
}

/// A cone with equal radii is a cylinder: the wall is a cylinder's and the body is the
/// cylinder's.
#[test]
fn a_cone_of_equal_radii_is_a_cylinder() {
    let frame = tilted([3.0, 4.0, 5.0]);
    let c = cone(&frame, [mm(2.5); 2], mm(6.0), &doc()).unwrap();
    let ConeFaces {
        bottom: Some(_),
        top: Some(_),
        wall,
    } = c.faces
    else {
        panic!("both caps")
    };
    assert!(matches!(
        c.body.face(wall).unwrap().surface(),
        Surface::Cylinder(_)
    ));
    let truth = solid::cylinder(2.5, 6.0).unwrap();
    assert_mass(&c.body, &frame, &truth, 6.0, 1e-12, "cone 2.5 to 2.5");
}

/// The pin's curved specs (sphere r3 at (1, -2, 0.5), torus R5 r1.5 at (0, 1, -1)), and each
/// at a tilted frame: counts (the sphere's poles are collapsed edges), the closed-solid
/// structure, volume, area and centroid against T9, membership against T10, and that the
/// sphere's bounding box holds it.
#[test]
fn complete_spheres_and_tori_have_analytic_properties() {
    for frame in [at([1.0, -2.0, 0.5]), tilted([1.0, -2.0, 0.5])] {
        let m = sphere(&frame, mm(3.0), &doc()).unwrap();
        assert_eq!(counts(&m.body), (1, 3, 2));
        assert_eq!(collapsed(&m.body), 2);
        assert_closed_solid(&m.body, 0);
        let truth = solid::sphere(3.0).unwrap();
        assert_mass(&m.body, &frame, &truth, 3.0, 1e-12, "sphere");
        assert_membership(
            &m.body,
            &leaf(&frame, Primitive::Sphere { radius: 3.0 }),
            0.02,
            "sphere",
        );
        // The body's box holds the sphere. It is the conservative box `scx-brep` keeps for
        // pruning, not the tight one the pin compared (`tight: true`), so only holding is
        // claimed.
        let bx = m.body.bounding_box().unwrap();
        let c = frame.origin().to_array();
        let (lo, hi) = (bx.min().to_array(), bx.max().to_array());
        for i in 0..3 {
            assert!(
                lo[i] <= c[i] - 3.0 && hi[i] >= c[i] + 3.0,
                "sphere box {bx:?}"
            );
        }
    }
    for frame in [at([0.0, 1.0, -1.0]), tilted([0.0, 1.0, -1.0])] {
        let m = torus(&frame, mm(5.0), mm(1.5), &doc()).unwrap();
        assert_eq!(counts(&m.body), (1, 2, 1));
        assert_closed_solid(&m.body, 1);
        let truth = solid::torus(5.0, 1.5).unwrap();
        assert_mass(&m.body, &frame, &truth, 6.5, 1e-12, "torus");
        assert_membership(
            &m.body,
            &leaf(
                &frame,
                Primitive::Torus {
                    major: 5.0,
                    minor: 1.5,
                },
            ),
            0.02,
            "torus",
        );
    }
}

/// Spheres of radius 2 at the origin and 5 at (1, -2, 0.5): area `4 pi r^2` and volume
/// `4/3 pi r^3` to the pin's `1e-9`, read off the body as built.
#[test]
fn precise_sphere_properties_scale_and_translate() {
    for (r, c) in [(2.0, [0.0; 3]), (5.0, [1.0, -2.0, 0.5])] {
        let frame = at(c);
        let m = sphere(&frame, mm(r), &doc()).unwrap();
        let m = measure(&m.body, frame.origin());
        assert_within(m.area, m.error.area, 4.0 * PI * r * r, 1e-9, "sphere area");
        assert_within(
            m.volume,
            m.error.volume,
            4.0 / 3.0 * PI * r * r * r,
            1e-9,
            "sphere volume",
        );
    }
}

/// The pin's `primitive_infinite_region_searches`, as one statement: each body has the two
/// regions it was built with, the outside (infinite) and one solid, every face between them
/// with its front normal pointing out of the solid, and the volume the truth's. The pin
/// searched for the infinite region after building; here it is decided at construction.
/// Rows: the cylinder r4 h14 at (10, 20, -1); a thin cylinder (h 5e-4); a far cylinder at
/// (600, 0, 0); the cones 5 to 0 and 5 to 2 of height 7; the box 8 x 14 x 3; the huge box
/// 1000 x 1e7 x 4e7 at a 1e-9 mm document.
#[test]
fn each_region_is_the_one_built() {
    let p = [10.0, 20.0, -1.0];
    let bodies: Vec<(&str, Brep, Frame, Solid)> = vec![
        (
            "cylinder r4 h14",
            cylinder(&at(p), mm(4.0), mm(14.0), &doc()).unwrap().body,
            at(p),
            solid::cylinder(4.0, 14.0).unwrap(),
        ),
        (
            "thin cylinder",
            cylinder(&at(p), mm(4.0), mm(5e-4), &doc()).unwrap().body,
            at(p),
            solid::cylinder(4.0, 5e-4).unwrap(),
        ),
        (
            "far cylinder",
            cylinder(&at([600.0, 0.0, 0.0]), mm(4.0), mm(14.0), &doc())
                .unwrap()
                .body,
            at([600.0, 0.0, 0.0]),
            solid::cylinder(4.0, 14.0).unwrap(),
        ),
        (
            "cone 5 to 0",
            cone(&at(p), [mm(5.0), mm(0.0)], mm(7.0), &doc())
                .unwrap()
                .body,
            at(p),
            solid::cone(5.0, 0.0, 7.0).unwrap(),
        ),
        (
            "cone 5 to 2",
            cone(&at(p), [mm(5.0), mm(2.0)], mm(7.0), &doc())
                .unwrap()
                .body,
            at(p),
            solid::cone(5.0, 2.0, 7.0).unwrap(),
        ),
        (
            "box 8x14x3",
            block(&at(p), [mm(8.0), mm(14.0), mm(3.0)], &doc())
                .unwrap()
                .body,
            at(p),
            solid::block(8.0, 14.0, 3.0).unwrap(),
        ),
        (
            "huge box at a 1e-9 document",
            block(
                &at([0.0; 3]),
                [mm(1000.0), mm(1e7), mm(4e7)],
                &Document::with_resolution(Resolution::new(mm(1e-9)).unwrap()),
            )
            .unwrap()
            .body,
            at([0.0; 3]),
            solid::block(1000.0, 1e7, 4e7).unwrap(),
        ),
    ];
    for (what, b, frame, truth) in bodies {
        let regions: Vec<_> = b.regions().collect();
        assert_eq!(regions.len(), 2, "{what}");
        assert!(!b.outside().is_solid(), "{what}");
        assert_eq!(regions.iter().filter(|r| r.is_solid()).count(), 1, "{what}");
        let outside = b.outside().shells().next().unwrap().id();
        for face in b.faces() {
            assert_eq!(face.shell(FaceSide::Front).id(), outside, "{what}");
            // The front normal at the middle of the face points away from the solid: every
            // body here is convex, so away from its centroid.
            let uv = face.uv_box().unwrap();
            let mid = Point2::new(uv.x_interval().mid(), uv.y_interval().mid());
            assert_eq!(face.contains_uv(mid), Some(UvSide::Inside), "{what}");
            let s = face.surface();
            let n: Vec3 = *s.normal(mid, [Approach::Above; 2]).unwrap();
            let c = truth.place(&truth_frame(&frame)).moments.centroid();
            let away = s.point(mid) - Point3::new(c[0], c[1], c[2]);
            assert!(n.dot(away) > 0.0, "{what} {:?}", face.id());
        }
        let m = measure(&b, frame.origin());
        assert_within(m.volume, m.error.volume, truth.volume(), 1e-11, what);
    }
}

/// MK1's far rows handed to MK2 (72 §14.7): the cylindrical box's radii and lengths at
/// 1e3, 1e6 and 1e8 along every axis. A tube (inner and outer radius) is a profile's
/// extrusion; here each radius is its own cylinder. The cones, sphere and torus join them at
/// a tilted frame.
#[test]
fn far_cylinders_build_closed_and_valid() {
    let mut failures = Vec::new();
    for c in [1e3, 1e6, 1e8] {
        for (length, inner, outer) in [(1.0, 1.0, 2.0), (1e-3, 1.0, 2.0), (1.0, 1e-3, 2e-3)] {
            for r in [inner, outer] {
                let frame = at([c; 3]);
                match cylinder(&frame, mm(r), mm(length), &doc()) {
                    Err(e) => failures.push(format!("r {r:e} h {length:e} at {c:e}: {e}")),
                    Ok(m) => {
                        assert_eq!(counts(&m.body), (3, 3, 2));
                        assert_closed_far(&m.body, 0, c);
                    }
                }
            }
        }
    }
    // And the other kinds there, the apex and the poles included.
    for c in [1e3, 1e6, 1e8] {
        let frame = tilted([c; 3]);
        let made = [
            (
                "cone 1 to 0",
                cone(&frame, [mm(1.0), mm(0.0)], mm(1.0), &doc()).map(|m| m.body),
            ),
            (
                "cone 0 to 1",
                cone(&frame, [mm(0.0), mm(1.0)], mm(1.0), &doc()).map(|m| m.body),
            ),
            ("sphere 1", sphere(&frame, mm(1.0), &doc()).map(|m| m.body)),
            (
                "torus 2 1",
                torus(&frame, mm(2.0), mm(1.0), &doc()).map(|m| m.body),
            ),
        ];
        for (what, r) in made {
            match r {
                Err(e) => failures.push(format!("{what} at {c:e}: {e}")),
                Ok(b) => assert_closed_far(&b, i64::from(what.starts_with("torus")), c),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

fn refused_size(r: Result<impl core::fmt::Debug, MakeError>, dimension: Dimension) {
    match r {
        Err(e @ MakeError::Refused(Refusal::Size { dimension: d, .. })) => {
            assert_eq!(d, dimension);
            assert_eq!(e.kind(), ErrorKind::Refused);
            assert_eq!(e.name(), "size");
        }
        other => panic!("{dimension:?}: {other:?}"),
    }
}

/// `api_primitives`' size refusals: a radius or height not finite and above the resolution
/// is refused naming it.
#[test]
fn sizes_not_above_the_resolution_are_refused_by_name() {
    let f = at([0.0; 3]);
    let zone = Resolution::DEFAULT.zone().mm();
    let bad = [
        -3.0,
        -zone,
        0.0,
        zone,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for v in bad {
        refused_size(sphere(&f, mm(v), &doc()), Dimension::Radius);
        refused_size(cylinder(&f, mm(v), mm(1.0), &doc()), Dimension::Radius);
        refused_size(cylinder(&f, mm(1.0), mm(v), &doc()), Dimension::Height);
        refused_size(
            cone(&f, [mm(1.0), mm(1.0)], mm(v), &doc()),
            Dimension::Height,
        );
        refused_size(torus(&f, mm(v), mm(0.5), &doc()), Dimension::Major);
        refused_size(torus(&f, mm(2.0), mm(v), &doc()), Dimension::Minor);
        if v != 0.0 {
            // Zero is an apex; every other bad radius is refused.
            refused_size(
                cone(&f, [mm(v), mm(1.0)], mm(1.0), &doc()),
                Dimension::BaseRadius,
            );
            refused_size(
                cone(&f, [mm(1.0), mm(v)], mm(1.0), &doc()),
                Dimension::TopRadius,
            );
        }
    }
    // Just above the resolution is a size.
    assert!(sphere(&f, mm(2.0 * zone), &doc()).is_ok());
}

/// `validators_match_upstream`: a cone of radii 0 and 1 is valid; both zero is refused.
#[test]
fn a_cone_with_both_radii_zero_is_refused() {
    let f = at([0.0; 3]);
    let e = cone(&f, [mm(0.0); 2], mm(1.0), &doc()).unwrap_err();
    assert_eq!(e, MakeError::Refused(Refusal::ZeroRadii));
    assert_eq!((e.kind(), e.name()), (ErrorKind::Refused, "zero_radii"));
    assert!(cone(&f, [mm(0.0), mm(1.0)], mm(1.0), &doc()).is_ok());
}

/// `a_torus_whose_tube_crosses_its_axis_is_refused_naming_the_radii`: a spindle torus is
/// refused; the horn torus, which the pin built, is `Unsupported` (72 D3), on purpose.
#[test]
fn spindle_tori_are_refused_and_horn_tori_unsupported() {
    let f = at([0.0; 3]);
    let zone = Resolution::DEFAULT.zone().mm();
    for (major, minor) in [(1.0, 2.0), (1.0, 1e9), (2.0, 2.0 + 4.0 * zone)] {
        let e = torus(&f, mm(major), mm(minor), &doc()).unwrap_err();
        assert!(
            matches!(e, MakeError::Refused(Refusal::SelfIntersecting { .. })),
            "{major} {minor}: {e:?}"
        );
        assert_eq!(
            (e.kind(), e.name()),
            (ErrorKind::Refused, "self_intersecting")
        );
    }
    for (major, minor) in [(2.0, 2.0), (2.0, 2.0 + 0.5 * zone), (2.0, 2.0 - 0.5 * zone)] {
        let e = torus(&f, mm(major), mm(minor), &doc()).unwrap_err();
        assert_eq!(
            e,
            MakeError::Unsupported(Unsupported::HornTorus { major, minor }),
            "{major} {minor}"
        );
        assert_eq!((e.kind(), e.name()), (ErrorKind::Unsupported, "horn_torus"));
    }
    // A tube just clear of the axis is a ring torus.
    assert!(torus(&f, mm(2.0), mm(2.0 - 2.0 * zone), &doc()).is_ok());
    // A major radius at the resolution is a size refusal before the torus kinds.
    refused_size(torus(&f, mm(2e-12), mm(0.5), &doc()), Dimension::Major);
}

/// The same inputs make the same body, bit for bit.
#[test]
fn making_is_deterministic() {
    let f = tilted([7.0, 8.0, 9.0]);
    let bits = |b: &Brep| -> Vec<[u64; 3]> {
        b.vertices()
            .map(|v| v.point().to_array().map(f64::to_bits))
            .collect()
    };
    let a = torus(&f, mm(3.0), mm(1.0), &doc()).unwrap();
    let b = torus(&f, mm(3.0), mm(1.0), &doc()).unwrap();
    assert_eq!(a.faces, b.faces);
    assert_eq!(bits(&a.body), bits(&b.body));
    let a = cone(&f, [mm(0.0), mm(2.0)], mm(3.0), &doc()).unwrap();
    let b = cone(&f, [mm(0.0), mm(2.0)], mm(3.0), &doc()).unwrap();
    assert_eq!(a.faces, b.faces);
    assert_eq!(bits(&a.body), bits(&b.body));
}
