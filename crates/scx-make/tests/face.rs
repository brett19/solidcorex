// Written for solidcorex. Apache-2.0.
//! `face` (research 75 MK15; the pin's `face_on`, `sheet_on`): a sheet of one face on each
//! analytic surface over a uv box or a region of iso-lines, judged against closed-form
//! areas (a cylinder patch `du dv`, a cone's frustum patch, a sphere zone, a torus patch),
//! the structure counted from the box (corners a period, a seam or a pole makes one), the
//! edges' kinds (every edge a line or a circle, or collapsed at a pole), the pcurves on
//! their edges, and the loop's orientation in uv (positive, and `du dv` for a box).
//!
//! Removed on purpose, from the pin's tests: those that read a face back through the
//! pin's surface-kind recogniser or its topological queries (`scx-bodyq`'s, step 6), and
//! those on NURBS faces (MK15b: iso-curve extraction is not built).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{PI, TAU};
use std::sync::Arc;

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, EdgeShape, FaceSide, UseGeom};
use scx_curve::Curve;
use scx_make::{Bound, MakeError, Refusal, Unsupported, face};
use scx_mass::{MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_planar::{Region, Segment};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, Torus};

fn mm(v: f64) -> Length {
    Length::from_mm(v)
}

fn doc() -> Document {
    Document::default()
}

fn plain() -> Frame {
    Frame::new(
        Point3::ORIGIN,
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

fn tilted() -> Frame {
    Frame::new(
        Point3::new(1250.0, -2125.0, 600.0),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-3.0, 0.5, 1.0),
    )
    .unwrap()
}

/// The surfaces under test, with their closed-form area over a box.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Plane,
    Cylinder(f64),
    Cone(f64, f64),
    Sphere(f64),
    Torus(f64, f64),
}

impl Kind {
    fn surface(self, frame: Frame, sense: Sense) -> Surface {
        match self {
            Self::Plane => Surface::Plane(Plane::new(frame)),
            Self::Cylinder(r) => Surface::Cylinder(Cylinder::new(frame, mm(r), sense).unwrap()),
            Self::Cone(r, a) => Surface::Cone(Cone::new(frame, mm(r), a, sense).unwrap()),
            Self::Sphere(r) => Surface::Sphere(Sphere::new(frame, mm(r), sense).unwrap()),
            Self::Torus(r, s) => Surface::Torus(Torus::new(frame, mm(r), mm(s), sense).unwrap()),
        }
    }

    /// The period of `u`.
    fn period(self) -> f64 {
        match self {
            Self::Cylinder(r) => TAU * r,
            _ => 1.0,
        }
    }

    /// The area over `[u0, u1] x [v0, v1]`, from the angles and heights of the surface's
    /// own points (the parameter is not proportional to the angle: it is the pin's
    /// rational quarter arcs), so by Archimedes' hat box for the sphere zone.
    fn area(self, [u0, u1, v0, v1]: [f64; 4]) -> f64 {
        let reference = self.surface(plain(), Sense::Forward);
        let at = |u: f64, v: f64| reference.point(Point2::new(u, v));
        let v_ref = match self {
            Self::Sphere(_) => 0.5,
            Self::Cone(..) => 1.0,
            _ => 0.0,
        };
        let swept = |a: f64, b: f64, length: f64, full: f64| {
            if length >= full * (1.0 - 1e-12) {
                TAU
            } else {
                (b - a).rem_euclid(TAU)
            }
        };
        let theta = |u: f64| {
            let p = at(u, v_ref);
            p.y.atan2(p.x)
        };
        let dtheta = swept(theta(u0), theta(u1), u1 - u0, self.period());
        let (du, dv) = (u1 - u0, v1 - v0);
        match self {
            Self::Plane => du * dv,
            Self::Cylinder(r) => r * dtheta * dv,
            Self::Cone(r, a) => {
                let k = a.tan();
                dtheta * (1.0 + k * k).sqrt() * (r * dv + k * (v1 * v1 - v0 * v0) / 2.0)
            }
            Self::Sphere(r) => r * dtheta * (at(0.0, v1).z - at(0.0, v0).z),
            Self::Torus(r, s) => {
                let psi = |v: f64| {
                    let p = at(0.0, v);
                    p.z.atan2(p.x - r)
                };
                let dpsi = swept(psi(v0), psi(v1), dv, 1.0);
                dtheta * s * (r * dpsi + at(0.0, v1).z - at(0.0, v0).z)
            }
        }
    }

    fn apex(self) -> f64 {
        match self {
            Self::Cone(r, a) => -r / a.tan(),
            _ => 0.0,
        }
    }
}

fn area_of(b: &Brep, about: Point3) -> f64 {
    let spec = MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(about);
    mass(
        b,
        &spec,
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .area
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

fn counts(b: &Brep) -> (usize, usize, usize) {
    (b.faces().len(), b.edges().len(), b.vertices().len())
}

fn bx(u0: f64, u1: f64, v0: f64, v1: f64) -> Bound {
    Bound::Box(Extent2d::from_bounds(u0, v0, u1, v1).unwrap())
}

/// A sheet of one face: no solid, one shell, every edge's pcurves on it, every edge a line
/// or a circle (or collapsed), and the loop counter-clockwise in uv.
fn assert_exact_sheet(b: &Brep, expect_uv_area: Option<f64>) {
    assert_eq!(b.regions().len(), 1);
    assert_eq!(b.shells().len(), 1);
    for f in b.faces() {
        assert!(f.shell(FaceSide::Front).region().is_outside());
        assert_eq!(f.shell(FaceSide::Front).id(), f.shell(FaceSide::Back).id());
        let mut poly: Vec<Point2> = Vec::new();
        for u in f.uses() {
            match (u.geom(), u.edge().shape()) {
                (UseGeom::Pcurve { curve, interval }, EdgeShape::Curve { curve: c3, .. }) => {
                    assert!(
                        matches!(c3, Curve::Line(_) | Curve::Circle(_)),
                        "an iso edge is a line or a circle"
                    );
                    let n = 16;
                    for i in 0..=n {
                        let t = interval.min() + interval.length() * f64::from(i) / f64::from(n);
                        let on = f.surface().point(curve.point(t));
                        assert!(on.distance(c3.point(t)) < 1e-9, "pcurve off its edge");
                    }
                    let pts = (0..=n).map(|i| {
                        let s = f64::from(i) / f64::from(n);
                        let t = if u.sense() == Sense::Forward {
                            interval.min() + interval.length() * s
                        } else {
                            interval.max() - interval.length() * s
                        };
                        curve.point(t)
                    });
                    poly.extend(pts);
                }
                (UseGeom::Point(_), EdgeShape::Collapsed) => {}
                (UseGeom::Pcurve { curve, interval }, EdgeShape::Collapsed) => {
                    let (a, b) = if u.sense() == Sense::Forward {
                        (interval.min(), interval.max())
                    } else {
                        (interval.max(), interval.min())
                    };
                    poly.push(curve.point(a));
                    poly.push(curve.point(b));
                }
                other => panic!("{other:?}"),
            }
        }
        if let Some(want) = expect_uv_area {
            if f.loops().count() == 1 {
                let n = poly.len();
                let signed: f64 = (0..n)
                    .map(|i| {
                        let (p, q) = (poly[i], poly[(i + 1) % n]);
                        p.x * q.y - q.x * p.y
                    })
                    .sum::<f64>()
                    / 2.0;
                assert!(rel(signed, want) < 1e-9, "uv area {signed} vs {want}");
            }
        }
    }
}

struct Case {
    kind: Kind,
    uv: [f64; 4],
    /// (faces, edges, vertices)
    counts: (usize, usize, usize),
}

fn cases() -> Vec<Case> {
    let r = 2.5;
    let cylinder_turn = TAU * r;
    let cone = Kind::Cone(3.0, 0.4);
    let apex = cone.apex();
    let sphere = Kind::Sphere(4.0);
    let torus = Kind::Torus(7.0, 2.0);
    let c = |kind, uv, counts| Case { kind, uv, counts };
    vec![
        c(Kind::Plane, [-1.0, 3.5, 0.5, 2.0], (1, 4, 4)),
        c(Kind::Cylinder(r), [0.3, 4.1, -1.0, 2.5], (1, 4, 4)),
        c(Kind::Cylinder(r), [0.7, 0.7 + cylinder_turn, -1.0, 2.5], (1, 3, 2)),
        c(cone, [0.1, 0.8, 0.5, 2.0], (1, 4, 4)),
        c(cone, [0.0, 1.0, 0.5, 2.0], (1, 3, 2)),
        // From the apex: its edge is collapsed.
        c(cone, [0.2, 0.9, apex, 2.0], (1, 4, 3)),
        c(cone, [0.0, 1.0, apex, 2.0], (1, 3, 2)),
        c(sphere, [0.2, 0.7, 0.25, 0.8], (1, 4, 4)),
        c(sphere, [0.0, 1.0, 0.25, 0.8], (1, 3, 2)),
        c(sphere, [0.1, 0.6, 0.0, 0.5], (1, 4, 3)),
        c(sphere, [0.1, 0.6, 0.4, 1.0], (1, 4, 3)),
        c(sphere, [0.1, 0.6, 0.0, 1.0], (1, 4, 2)),
        c(sphere, [0.0, 1.0, 0.0, 1.0], (1, 3, 2)),
        c(torus, [0.1, 0.5, 0.2, 0.9], (1, 4, 4)),
        c(torus, [0.0, 1.0, 0.2, 0.9], (1, 3, 2)),
        c(torus, [0.1, 0.5, 0.0, 1.0], (1, 3, 2)),
        c(torus, [0.0, 1.0, 0.0, 1.0], (1, 2, 1)),
    ]
}

/// Every box on every surface is the closed-form area, at two frames and both senses.
#[test]
fn a_face_on_a_box_has_the_closed_form_area() {
    for case in cases() {
        for frame in [plain(), tilted()] {
            for sense in [Sense::Forward, Sense::Reversed] {
                let s = case.kind.surface(frame, sense);
                let [u0, u1, v0, v1] = case.uv;
                let m = face(&s, &bx(u0, u1, v0, v1), &doc()).unwrap();
                let what = format!("{:?} {:?} {sense:?}", case.kind, case.uv);
                assert_eq!(counts(&m.body), case.counts, "{what}");
                assert_exact_sheet(&m.body, Some((u1 - u0) * (v1 - v0)));
                let got = area_of(&m.body, frame.origin());
                let want = case.kind.area(case.uv);
                assert!(rel(got, want) < 1e-10, "{what}: {got} vs {want}");
                assert_eq!(m.body.face(m.faces).unwrap().surface(), &s);
            }
        }
    }
}

/// The whole faces of the constructors' surfaces are their boxes: the sphere `4 pi r^2`,
/// the torus `4 pi^2 R r`, a cylinder wall `2 pi r h`.
#[test]
fn the_whole_faces_have_the_textbook_areas() {
    let d = doc();
    let s = Kind::Sphere(3.0).surface(tilted(), Sense::Forward);
    let m = face(&s, &bx(0.0, 1.0, 0.0, 1.0), &d).unwrap();
    assert!(rel(area_of(&m.body, Point3::ORIGIN), 4.0 * PI * 9.0) < 1e-10);
    let s = Kind::Torus(5.0, 1.5).surface(plain(), Sense::Forward);
    let m = face(&s, &bx(0.0, 1.0, 0.0, 1.0), &d).unwrap();
    assert!(rel(area_of(&m.body, Point3::ORIGIN), 4.0 * PI * PI * 5.0 * 1.5) < 1e-10);
    let s = Kind::Cylinder(2.0).surface(plain(), Sense::Forward);
    let m = face(&s, &bx(0.0, 4.0 * PI, 0.0, 7.0), &d).unwrap();
    assert!(rel(area_of(&m.body, Point3::ORIGIN), TAU * 2.0 * 7.0) < 1e-10);
}

/// A window of the cylinder is its constructor's wall piece: the edges are the circle
/// arcs of the rims and the rulings, with the closed-form lengths.
#[test]
fn a_cylinder_patch_is_bounded_by_arcs_and_rulings() {
    let r = 2.0;
    let s = Kind::Cylinder(r).surface(tilted(), Sense::Forward);
    let m = face(&s, &bx(0.5, 2.0, 1.0, 4.0), &doc()).unwrap();
    let mut lines = 0;
    let mut circles = 0;
    for e in m.body.edges() {
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            panic!("collapsed");
        };
        match curve {
            Curve::Line(l) => {
                lines += 1;
                let len = l.point(interval.max()).distance(l.point(interval.min()));
                assert!((len - 3.0).abs() < 1e-9, "{len}");
            }
            Curve::Circle(c) => {
                circles += 1;
                assert!((c.radius().mm() - r).abs() < 1e-12);
                assert!(interval.length() > 0.0);
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((lines, circles), (2, 2));
}

/// Making is deterministic, to the bit.
#[test]
fn making_a_face_twice_gives_one_body() {
    let s = Kind::Torus(7.0, 2.0).surface(tilted(), Sense::Reversed);
    let b = bx(0.1, 0.9, 0.2, 0.7);
    let (a, c) = (
        face(&s, &b, &doc()).unwrap().body,
        face(&s, &b, &doc()).unwrap().body,
    );
    assert_eq!(scx_record::encode(&a).unwrap(), scx_record::encode(&c).unwrap());
}

fn line(a: [f64; 2], b: [f64; 2]) -> Segment {
    Segment::line(Point2::new(a[0], a[1]), Point2::new(b[0], b[1]))
}

fn rect(a: [f64; 2], b: [f64; 2]) -> Vec<Segment> {
    let p = [a, [b[0], a[1]], b, [a[0], b[1]]];
    (0..4).map(|i| line(p[i], p[(i + 1) % 4])).collect()
}

fn region(outer: Vec<Segment>, holes: Vec<Vec<Segment>>) -> Region {
    Region::new(outer, holes, doc().resolution()).unwrap()
}

/// A region of iso-lines on each kind: a rectangle with a rectangular hole is the outer
/// box's area less the hole's, with `4 + 4` edges; an L is the sum of two boxes.
#[test]
fn a_region_of_iso_lines_has_the_closed_form_area() {
    let kinds = [
        (Kind::Plane, [0.0, 6.0, 0.0, 4.0], [1.0, 2.0, 1.0, 3.0]),
        (Kind::Cylinder(2.0), [0.0, 6.0, 0.0, 4.0], [1.0, 2.0, 1.0, 3.0]),
        (Kind::Cone(3.0, 0.4), [0.1, 0.8, 0.5, 2.5], [0.3, 0.5, 1.0, 2.0]),
        (Kind::Sphere(4.0), [0.1, 0.8, 0.2, 0.9], [0.3, 0.5, 0.4, 0.6]),
        (Kind::Torus(7.0, 2.0), [0.1, 0.8, 0.2, 0.9], [0.3, 0.5, 0.4, 0.6]),
    ];
    for (kind, o, h) in kinds {
        for frame in [plain(), tilted()] {
            let s = kind.surface(frame, Sense::Forward);
            let r = region(
                rect([o[0], o[2]], [o[1], o[3]]),
                vec![rect([h[0], h[2]], [h[1], h[3]])],
            );
            let m = face(&s, &Bound::Region(r), &doc()).unwrap();
            assert_eq!(counts(&m.body), (1, 8, 8), "{kind:?}");
            assert_exact_sheet(&m.body, None);
            let want = kind.area(o) - kind.area(h);
            let got = area_of(&m.body, frame.origin());
            assert!(rel(got, want) < 1e-10, "{kind:?}: {got} vs {want}");
            assert_eq!(m.body.face(m.faces).unwrap().loops().count(), 2);

            // An L: the box less its upper right quarter.
            let (um, vm) = ((o[0] + o[1]) / 2.0, (o[2] + o[3]) / 2.0);
            let l = vec![
                line([o[0], o[2]], [o[1], o[2]]),
                line([o[1], o[2]], [o[1], vm]),
                line([o[1], vm], [um, vm]),
                line([um, vm], [um, o[3]]),
                line([um, o[3]], [o[0], o[3]]),
                line([o[0], o[3]], [o[0], o[2]]),
            ];
            let m = face(&s, &Bound::Region(region(l, vec![])), &doc()).unwrap();
            assert_eq!(counts(&m.body), (1, 6, 6), "{kind:?}");
            let want = kind.area(o) - kind.area([um, o[1], vm, o[3]]);
            let got = area_of(&m.body, frame.origin());
            assert!(rel(got, want) < 1e-10, "L {kind:?}: {got} vs {want}");
        }
    }
}

/// On a plane a region's arcs and circles are exact too: a disc with a round hole is the
/// sheet of the region.
#[test]
fn a_region_on_a_plane_may_have_arcs() {
    let c = |x: f64, y: f64, r: f64, s| Segment::circle(Point2::new(x, y), r, s);
    let r = region(
        vec![c(1.0, 2.0, 3.0, Sense::Forward)],
        vec![vec![c(1.5, 2.0, 1.0, Sense::Reversed)]],
    );
    let s = Kind::Plane.surface(tilted(), Sense::Forward);
    let m = face(&s, &Bound::Region(r), &doc()).unwrap();
    assert_eq!(counts(&m.body), (1, 2, 2));
    let want = PI * (9.0 - 1.0);
    assert!(rel(area_of(&m.body, Point3::ORIGIN), want) < 1e-10);
    assert_exact_sheet(&m.body, None);
}

fn refused(r: Result<impl Sized, MakeError>) -> MakeError {
    match r {
        Err(e) => e,
        Ok(_) => panic!("made"),
    }
}

#[test]
fn boxes_and_regions_off_the_surface_are_refused() {
    let d = doc();
    let sphere = Kind::Sphere(4.0).surface(plain(), Sense::Forward);
    let cyl = Kind::Cylinder(2.0).surface(plain(), Sense::Forward);
    let cone = Kind::Cone(3.0, 0.4);
    let cone_s = cone.surface(plain(), Sense::Forward);
    let torus = Kind::Torus(7.0, 2.0).surface(plain(), Sense::Forward);
    let uvbox = |e: MakeError| matches!(e, MakeError::Refused(Refusal::UvBox { .. }));
    // Past a pole, longer than a period, empty, not finite.
    assert!(uvbox(refused(face(&sphere, &bx(0.0, 0.5, -0.1, 0.5), &d))));
    assert!(uvbox(refused(face(&sphere, &bx(0.0, 0.5, 0.5, 1.2), &d))));
    assert!(uvbox(refused(face(&sphere, &bx(0.0, 1.5, 0.2, 0.5), &d))));
    assert!(uvbox(refused(face(&cyl, &bx(0.0, 14.0, 0.0, 1.0), &d))));
    assert!(uvbox(refused(face(&torus, &bx(0.0, 0.5, 0.0, 1.2), &d))));
    assert!(uvbox(refused(face(&cone_s, &bx(0.0, 0.5, cone.apex() - 1.0, 1.0), &d))));
    assert!(uvbox(refused(face(&cyl, &bx(1.0, 1.0, 0.0, 1.0), &d))));
    // A region longer than a period.
    let wide = region(rect([0.0, 0.2], [1.5, 0.6]), vec![]);
    assert!(uvbox(refused(face(&sphere, &Bound::Region(wide), &d))));
    // A region's arc off a plane, a slanted line, and a line on a pole.
    let arc = region(
        vec![Segment::circle(Point2::new(0.5, 0.5), 0.1, Sense::Forward)],
        vec![],
    );
    let seg = |e: MakeError| matches!(e, MakeError::Unsupported(Unsupported::Segment(_)));
    assert!(seg(refused(face(&sphere, &Bound::Region(arc), &d))));
    let tri = region(
        vec![
            line([0.0, 0.0], [1.0, 0.0]),
            line([1.0, 0.0], [0.0, 1.0]),
            line([0.0, 1.0], [0.0, 0.0]),
        ],
        vec![],
    );
    assert!(seg(refused(face(&cyl, &Bound::Region(tri), &d))));
    let at_pole = region(rect([0.1, 0.0], [0.4, 0.5]), vec![]);
    assert!(seg(refused(face(&sphere, &Bound::Region(at_pole), &d))));
    // A region at another resolution than the document's.
    let other = Region::new(
        rect([0.0, 0.0], [1.0, 1.0]),
        vec![],
        scx_num::units::Resolution::new(mm(1e-3)).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        refused(face(&cyl, &Bound::Region(other), &d)),
        MakeError::Refused(Refusal::Resolution { .. })
    ));
}

/// A NURBS surface waits for iso-curve extraction (MK15b).
#[test]
fn a_nurbs_surface_is_unsupported_for_now() {
    let over = Extent2d::from_bounds(0.0, 0.0, 1.0, 1.0).unwrap();
    let n = Surface::Plane(Plane::new(plain())).to_nurbs(&over).unwrap();
    let e = refused(face(&Surface::Nurbs(Arc::new(n)), &bx(0.0, 1.0, 0.0, 1.0), &doc()));
    assert_eq!(e, MakeError::Unsupported(Unsupported::NurbsFace));
    assert_eq!(e.name(), "nurbs_face");
}
