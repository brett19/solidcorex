// Written for solidcorex. Apache-2.0.
//! `block` (research 72 MK1): the pin's box tests (`boxes_far_from_the_origin_build_closed_and_valid`,
//! `small_boxes_at_1e4_answer_as_at_the_origin`, the size refusals of `api_primitives`) as
//! outcomes, and a block at a rotated frame against the closed-form truth of
//! `scx_testkit::solid` (volume, area, centroid) and its own geometry: vertex positions,
//! face planes, exact pcurves, the structure counts.
//!
//! Volume and area are read by `scx_mass::mass` (relative accuracy as each test states, about the frame's
//! origin) and compared to `solid::block`; each tolerance is also checked against the
//! Estimate's error bound.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, FaceId, FaceSide, UseGeom};
use scx_curve::Curve2;
use scx_make::{BlockFaces, Dimension, Made, MakeError, Refusal, block};
use scx_mass::{MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Surface;
use scx_testkit::analytic::Frame as TruthFrame;
use scx_testkit::solid;

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn doc() -> Document {
    Document::default()
}

fn at(c: f64) -> Frame {
    Frame::new(
        Point3::new(c, c, c),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

fn cube(frame: &Frame, s: f64) -> Result<Made<BlockFaces>, MakeError> {
    block(frame, [mm(s); 3], &doc())
}

/// Faces, edges and vertices.
fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().len(), b.edges().len(), b.vertices().len())
}

/// What a closed one-solid body is: two regions (outside and one solid) with a shell each, every
/// edge with exactly two fins of opposite senses, every face's front in the outside's shell
/// and its back in the solid's, and every pcurve exact (no recorded bound) and a line.
fn assert_closed_solid(b: &Brep, made: &BlockFaces) {
    assert_eq!(b.regions().len(), 2);
    assert_eq!(b.shells().len(), 2);
    let outside = b.outside().shells().next().unwrap().id();
    for e in b.edges() {
        let f: Vec<_> = e.fins().collect();
        assert_eq!(f.len(), 2, "{:?}", e.id());
        assert_ne!(f[0].sense, f[1].sense, "{:?}", e.id());
    }
    for id in [made.bottom, made.top].into_iter().chain(made.sides) {
        let face = b.face(id).unwrap();
        assert_eq!(face.shell(FaceSide::Front).id(), outside);
        assert!(face.shell(FaceSide::Back).region().is_solid());
        assert_eq!(face.loops().len(), 1);
        assert_eq!(face.uses().count(), 4);
        for u in face.uses() {
            assert!(u.bound().is_none());
            assert!(matches!(
                u.geom(),
                UseGeom::Pcurve {
                    curve: Curve2::Line(_),
                    ..
                }
            ));
        }
    }
}

/// The distinct faces of the record: six.
fn all(f: &BlockFaces) -> Vec<FaceId> {
    [f.bottom, f.top].into_iter().chain(f.sides).collect()
}

/// The outer ring of a planar face as points, in the loop's order.
fn ring(b: &Brep, id: FaceId) -> Vec<Point3> {
    b.face(id)
        .unwrap()
        .uses()
        .map(|u| u.start().point())
        .collect()
}

/// The body's volume and area about `o` with their error estimates, by `scx_mass::mass` at
/// relative accuracy `accuracy`: `(volume, its error, area, its error)`.
fn volume_and_area(b: &Brep, o: Point3, accuracy: f64) -> (f64, f64, f64, f64) {
    let spec = MassSpec::new(RelAccuracy::new(accuracy).unwrap()).with_about(o);
    let m = mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    (m.volume, m.error.volume, m.area, m.error.area)
}

/// The pin's rows: sides from 2e-4 to a 5e3 cube at 1e12.
#[test]
fn boxes_far_from_the_origin_build_closed_and_valid() {
    let rows: [(f64, &[f64]); 7] = [
        (1e3, &[2e-4, 1e-2, 1.0]),
        (1e4, &[2e-4, 1e-3]),
        (1e5, &[2e-4, 3e-4, 1e-2]),
        (1e6, &[2e-4, 1e-3, 1.5e-3]),
        (1e7, &[1e-3, 1e-2]),
        (1e8, &[1e-3, 1e-2, 1e-1]),
        (1e12, &[5e3]),
    ];
    let mut failures = Vec::new();
    for (c, sides) in rows {
        for &s in sides {
            match cube(&at(c), s) {
                Err(e) => failures.push(format!("box {s:e} at {c:e}: {e}")),
                Ok(m) => {
                    let b = &m.body;
                    if counts(b) != (6, 12, 8) {
                        failures.push(format!("box {s:e} at {c:e}: {:?}", counts(b)));
                        continue;
                    }
                    assert_closed_solid(b, &m.faces);
                    // Every vertex is a corner: its offset from the origin is 0 or s on
                    // each axis, to the rounding of a point at c.
                    let slack = 4.0 * c * f64::EPSILON + 1e-12;
                    for v in b.vertices() {
                        for (p, o) in [v.point().x, v.point().y, v.point().z]
                            .into_iter()
                            .zip([c; 3])
                        {
                            let d = p - o;
                            if d.abs() > slack && (d - s).abs() > slack {
                                failures.push(format!("box {s:e} at {c:e}: corner {d:e}"));
                            }
                        }
                    }
                    // The tolerance, and the Estimate's error bound within it; the
                    // accuracy asked is the tolerance (the far rows round at `c eps / s`).
                    let slack = 1e-9 + 4.0 * c * f64::EPSILON / s;
                    let (vol, vol_err, area, area_err) =
                        volume_and_area(b, Point3::new(c, c, c), slack.min(1.0));
                    let truth = solid::block(s, s, s).unwrap();
                    if (vol / truth.volume() - 1.0).abs() > slack
                        || (area / truth.area - 1.0).abs() > slack
                        || vol_err > slack * truth.volume()
                        || area_err > slack * truth.area
                    {
                        failures.push(format!(
                            "box {s:e} at {c:e}: V {vol:e} (err {vol_err:e}) A {area:e} (err {area_err:e})"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Cubes of 1e-4 and 5e-5 at 1e4: the answer is the one at the origin.
#[test]
fn small_boxes_at_1e4_answer_as_at_the_origin() {
    let answer = |c: f64, s: f64| -> Result<(usize, usize, usize), String> {
        cube(&at(c), s)
            .map(|m| counts(&m.body))
            .map_err(|e| e.to_string())
    };
    for s in [1e-4, 5e-5] {
        assert_eq!(answer(1e4, s), answer(0.0, s), "{s:e}");
        assert_eq!(answer(0.0, s), Ok((6, 12, 8)), "{s:e}");
    }
}

/// `api_primitives`' `invalid_inputs_are_rejected_before_the_kernel` for the box: a zero
/// size is refused, naming the size; so is a negative, a NaN, an infinite one and one at or
/// below the resolution. The refusal is `Refused`, and nothing is built.
#[test]
fn a_size_that_is_not_finite_and_above_the_resolution_is_refused_naming_it() {
    let zone = Resolution::DEFAULT.zone();
    let bad = [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        zone.mm(),
        0.5 * zone.mm(),
    ];
    for value in bad {
        for (i, dimension) in [Dimension::X, Dimension::Y, Dimension::Z]
            .into_iter()
            .enumerate()
        {
            let mut size = [mm(10.0); 3];
            size[i] = mm(value);
            let e = block(&Frame::WORLD, size, &doc()).unwrap_err();
            assert_eq!(e.kind(), ErrorKind::Refused);
            assert_eq!(e.name(), "size");
            let MakeError::Refused(Refusal::Size {
                dimension: d,
                value: v,
                floor,
            }) = e
            else {
                panic!("{e:?}")
            };
            assert_eq!(d, dimension);
            assert!(v == value || (v.is_nan() && value.is_nan()));
            assert_eq!(floor, zone);
        }
    }
    // The first size refused is named when several are bad.
    let e = block(&Frame::WORLD, [mm(0.0), mm(-1.0), mm(1.0)], &doc()).unwrap_err();
    assert!(matches!(
        e,
        MakeError::Refused(Refusal::Size {
            dimension: Dimension::X,
            ..
        })
    ));
    // Just above the resolution builds.
    assert!(block(&Frame::WORLD, [mm(2.0 * zone.mm()); 3], &doc()).is_ok());
}

/// The floor is the document's resolution, not a constant.
#[test]
fn the_size_floor_is_the_documents_resolution() {
    let coarse = Document::with_resolution(Resolution::new(mm(0.01)).unwrap());
    let e = block(&Frame::WORLD, [mm(0.01), mm(1.0), mm(1.0)], &coarse).unwrap_err();
    assert!(matches!(e, MakeError::Refused(Refusal::Size { floor, .. }) if floor == mm(0.01)));
    assert!(block(&Frame::WORLD, [mm(0.02), mm(1.0), mm(1.0)], &coarse).is_ok());
    // 5e-3 builds at the default resolution, and is refused at the coarse one.
    assert!(block(&Frame::WORLD, [mm(5e-3); 3], &doc()).is_ok());
    assert!(block(&Frame::WORLD, [mm(5e-3); 3], &coarse).is_err());
}

/// A block at a rotated and shifted frame, with unequal sizes: vertex positions, face
/// normals by role, volume, area and centroid against `solid::block` placed.
#[test]
fn a_rotated_block_matches_the_closed_form_and_its_roles() {
    let frame = Frame::new(
        Point3::new(3.0, -2.0, 7.5),
        Vec3::new(1.0, 2.0, 2.0),
        Vec3::new(2.0, -1.0, 0.0),
    )
    .unwrap();
    let (dx, dy, dz) = (3.0, 5.0, 0.75);
    let m = block(&frame, [mm(dx), mm(dy), mm(dz)], &doc()).unwrap();
    let b = &m.body;
    assert_eq!(counts(b), (6, 12, 8));
    assert_closed_solid(b, &m.faces);

    // Vertices: the eight corners, placed by the frame.
    let (x, y, z): (Vec3, Vec3, Vec3) = (*frame.x(), *frame.y(), *frame.z());
    let mut corners = Vec::new();
    for k in 0..8_u8 {
        let c = |bit: u8| f64::from(k >> bit & 1);
        corners.push(frame.origin() + (x * (c(2) * dx) + y * (c(1) * dy) + z * (c(0) * dz)));
    }
    for p in &corners {
        assert!(b.vertices().any(|v| v.point() == *p), "{p:?}");
    }

    // Roles: the outward normal and plane offset of each face.
    let want = [
        (m.faces.bottom, -z, 0.0),
        (m.faces.top, z, dz),
        (m.faces.sides[0], -y, 0.0),
        (m.faces.sides[1], x, dx),
        (m.faces.sides[2], y, dy),
        (m.faces.sides[3], -x, 0.0),
    ];
    for (id, n, off) in want {
        let Surface::Plane(pl) = b.face(id).unwrap().surface() else {
            panic!("not a plane")
        };
        let got: Vec3 = *pl.frame().z();
        assert!((got - n).length() < 1e-15, "{id:?}");
        // Every ring point is on the plane `n . (p - origin) = off`.
        for p in ring(b, id) {
            let along = n.dot(p - frame.origin());
            assert!((along - off).abs() < 1e-14, "{id:?}: {along} vs {off}");
        }
    }

    // Truth: volume and area, and the centroid, of the standard block carried to the frame.
    let truth = solid::block(dx, dy, dz).unwrap();
    let tf = TruthFrame {
        origin: frame.origin().to_array(),
        x: x.to_array(),
        y: y.to_array(),
        z: z.to_array(),
    };
    let placed = truth.place(&tf);
    let (vol, vol_err, area, area_err) = volume_and_area(b, frame.origin(), 1e-13);
    assert!(
        (vol / placed.volume() - 1.0).abs() < 1e-13 && vol_err < 1e-13 * placed.volume(),
        "{vol} (err {vol_err:e}) {}",
        placed.volume()
    );
    assert!(
        (area / placed.area - 1.0).abs() < 1e-13 && area_err < 1e-13 * placed.area,
        "{area} (err {area_err:e}) {}",
        placed.area
    );
    let centroid = placed.moments.centroid();
    let mean = corners.iter().fold(Vec3::ZERO, |s, p| s + p.to_vec()) / 8.0;
    for (a, t) in mean.to_array().into_iter().zip(centroid) {
        assert!((a - t).abs() < 1e-13, "{a} {t}");
    }
}

/// Each pcurve is the face's own surface mapped onto the edge: for every use, the plane's
/// point at the pcurve's parameter is the edge's curve at that parameter.
#[test]
fn every_pcurve_lies_on_its_edge() {
    let frame = at(250.0);
    let m = block(&frame, [mm(2.0), mm(3.0), mm(4.0)], &doc()).unwrap();
    for id in all(&m.faces) {
        let face = m.body.face(id).unwrap();
        let Surface::Plane(pl) = face.surface() else {
            panic!("not a plane")
        };
        for u in face.uses() {
            let UseGeom::Pcurve { curve, interval } = u.geom() else {
                panic!("point use")
            };
            let EdgeShape::Curve {
                curve: c3,
                interval: iv,
            } = u.edge().shape()
            else {
                panic!("collapsed")
            };
            assert_eq!(interval, iv);
            for t in [0.0, 0.25, 0.5, 1.0] {
                let on_face = pl.point(curve.point(t));
                assert!(on_face.distance(c3.point(t)) < 1e-11, "{id:?} t={t}");
            }
            // A use's ends are its vertices' points.
            let (s, e) = (u.start().point(), u.end().point());
            let (t0, t1) = if u.sense() == Sense::Forward {
                (0.0, 1.0)
            } else {
                (1.0, 0.0)
            };
            assert!(c3.point(t0).distance(s) < 1e-11 && c3.point(t1).distance(e) < 1e-11);
        }
    }
}

/// The same inputs make the same body, bit for bit.
#[test]
fn making_is_deterministic() {
    let frame = at(37.25);
    let a = cube(&frame, 1.5).unwrap();
    let b = cube(&frame, 1.5).unwrap();
    assert_eq!(a.faces, b.faces);
    let bits = |m: &Made<BlockFaces>| -> Vec<[u64; 3]> {
        m.body
            .vertices()
            .map(|v| v.point().to_array().map(f64::to_bits))
            .collect()
    };
    assert_eq!(bits(&a), bits(&b));
}

/// Smallest repro of an `scx_mass` defect: a 2e-4 cube at (1e3, 1e3, 1e3), measured about
/// its far corner at relative accuracy 1e-12, returns `Defect(StepControl)` (as does 1e-13;
/// 1e-9 is met). A defect means a kernel bug: the accuracy is out of reach at this
/// size and distance, so the answer should be met or a typed `Refused` or `Exhausted`.
#[test]
#[ignore = "scx-mass returns Defect(StepControl) for an unreachable accuracy; should be a typed refusal"]
fn far_small_box_at_1e_12_is_met_or_refused_not_a_defect() {
    let m = cube(&at(1e3), 2e-4).unwrap();
    let spec =
        MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(Point3::new(1e3, 1e3, 1e3));
    let r = mass(
        &m.body,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    );
    assert!(!matches!(r, Err(scx_mass::MassError::Defect(_))), "{r:?}");
}
