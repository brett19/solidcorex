// Written for solidcorex. Apache-2.0.
//! `sheet_seams` (research 74 §52): where separate single-face sheets meet and the order of
//! contact proved along the seam, against constructions whose answers are closed forms.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::float_cmp
)]

use std::f64::consts::{FRAC_PI_2, PI};

use scx_bodyq::{
    BodyqError, BodyqRefusal, ContinuityTolerances, Limit, Order, SheetSeams, sheet_seams,
};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, FaceRecord, RegionId, ShellId, ShellRecord,
    VertexId, VertexRecord,
};
use scx_curve::{Circle, Curve, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Plane, Surface};

/// A one-face sheet being written: both sides of the face the outside.
struct Sheet {
    draft: Draft,
    outside: ShellId,
}

fn p(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}

impl Sheet {
    fn new() -> Self {
        let mut draft = Draft::new();
        let outside = draft
            .add_shell(ShellRecord::new(RegionId::OUTSIDE))
            .unwrap();
        Self { draft, outside }
    }

    fn vertex(&mut self, a: [f64; 3]) -> VertexId {
        self.draft.add_vertex(VertexRecord::new(p(a))).unwrap()
    }

    fn edge(&mut self, curve: Curve, lo: f64, hi: f64, ends: [VertexId; 2]) -> EdgeId {
        self.draft
            .add_edge(EdgeGeom::new(
                EdgeShape::Curve {
                    curve,
                    interval: Extent1d::new(lo, hi).unwrap(),
                },
                ends,
            ))
            .unwrap()
    }

    /// The straight edge from `a` to `b`.
    fn line(&mut self, a: [f64; 3], b: [f64; 3]) -> (EdgeId, [VertexId; 2]) {
        let (va, vb) = (self.vertex(a), self.vertex(b));
        let line = Line::new(p(a), p(b) - p(a)).unwrap();
        (self.edge(Curve::Line(line), 0.0, 1.0, [va, vb]), [va, vb])
    }

    fn face(mut self, surface: Surface, loop_: Vec<(EdgeId, Sense)>) -> Brep {
        let doc = Document::default();
        let loops = self
            .draft
            .exact_loops(&surface, &[loop_], &doc)
            .unwrap();
        self.draft
            .add_face(FaceRecord::new(
                surface,
                [self.outside, self.outside],
                loops,
            ))
            .unwrap();
        self.draft.finish(&doc).map_err(|u| u.cause).unwrap().0
    }
}

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(p(o), Vec3::new(z[0], z[1], z[2]), Vec3::new(x[0], x[1], x[2])).unwrap()
}

const FWD: Sense = Sense::Forward;
const REV: Sense = Sense::Reversed;

/// A rectangle of the plane `frame` (its z the sheet's normal, loop anticlockwise about it),
/// over `[x0, x1] x [y0, y1]` of its x and y.
fn rectangle(f: &Frame, x: [f64; 2], y: [f64; 2]) -> Brep {
    let at = |a: f64, b: f64| {
        let q = f.origin() + *f.x() * a + *f.y() * b;
        [q.x, q.y, q.z]
    };
    let mut s = Sheet::new();
    let (e0, _) = s.line(at(x[0], y[0]), at(x[1], y[0]));
    let (e1, _) = s.line(at(x[1], y[0]), at(x[1], y[1]));
    let (e2, _) = s.line(at(x[1], y[1]), at(x[0], y[1]));
    let (e3, _) = s.line(at(x[0], y[1]), at(x[0], y[0]));
    s.face(
        Surface::Plane(Plane::new(*f)),
        vec![(e0, FWD), (e1, FWD), (e2, FWD), (e3, FWD)],
    )
}

fn tol() -> ContinuityTolerances {
    ContinuityTolerances {
        gap: Length::from_mm(1e-6),
        angle: 1e-6,
        curvature: 1e-6,
        curvature_rate: 1e-6,
        curvature_accel: 1e-6,
    }
}

fn seams(sheets: &[&Brep], zone: f64) -> SheetSeams {
    sheet_seams(
        sheets,
        Length::from_mm(zone),
        &tol(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b}");
}

#[test]
fn coplanar_sheets_on_one_surface_meet_at_g4_by_identity() {
    let world = Frame::WORLD;
    let a = rectangle(&world, [0.0, 2.0], [0.0, 1.0]);
    let b = rectangle(&world, [2.0, 4.0], [0.0, 1.0]);
    let got = seams(&[&a, &b], 1e-3);
    assert!(got.unproved.is_empty());
    assert_eq!(got.seams.len(), 1);
    let s = got.seams[0];
    assert_eq!(s.meet.sheets, [0, 1]);
    near(s.along[0][0], 0.0, 1e-12);
    near(s.along[0][1], 1.0, 1e-12);
    assert_eq!(s.continuity.proved, Some(Order::G4));
    assert_eq!(s.continuity.limit, None);
}

#[test]
fn coplanar_sheets_on_two_frames_are_proved_g4_by_the_series() {
    // the same plane through another origin: the surfaces differ as values
    let a = rectangle(&Frame::WORLD, [0.0, 2.0], [0.0, 1.0]);
    let moved = frame([5.0, 3.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let b = rectangle(&moved, [-3.0, -1.0], [-3.0, -2.0]);
    let got = seams(&[&a, &b], 1e-3);
    assert_eq!(got.seams.len(), 1, "{got:?}");
    assert_eq!(got.seams[0].continuity.proved, Some(Order::G4));
}

#[test]
fn a_shifted_sheet_meets_over_the_part_they_share() {
    let world = Frame::WORLD;
    let a = rectangle(&world, [0.0, 2.0], [0.0, 1.0]);
    let b = rectangle(&world, [2.0, 4.0], [0.5, 1.5]);
    let got = seams(&[&a, &b], 1e-3);
    assert_eq!(got.seams.len(), 1);
    let s = got.seams[0];
    // a's edge runs up x = 2 over y in [0, 1]; the shared part is y in [0.5, 1]
    near(s.along[0][0], 0.5, 1e-9);
    near(s.along[0][1], 1.0, 1e-9);
    // b's edge runs down from y = 1.5: the same part is t in [0.5, 1]
    near(s.along[1][0], 0.5, 1e-9);
    near(s.along[1][1], 1.0, 1e-9);
    assert!(!s.same_sense);
    assert_eq!(s.continuity.proved, Some(Order::G4));
}

#[test]
fn sheets_that_only_touch_or_stand_apart_have_no_seam() {
    let world = Frame::WORLD;
    let a = rectangle(&world, [0.0, 2.0], [0.0, 1.0]);
    // a corner to corner
    let corner = rectangle(&world, [2.0, 3.0], [1.0, 2.0]);
    // a clear gap
    let far = rectangle(&world, [2.5, 4.0], [0.0, 1.0]);
    let got = seams(&[&a, &corner, &far], 1e-3);
    assert!(got.seams.is_empty(), "{got:?}");
    assert!(got.unproved.is_empty(), "{got:?}");
    // fewer than two sheets: nothing to meet
    assert_eq!(seams(&[&a], 1e-3), SheetSeams::default());
    assert_eq!(seams(&[], 1e-3), SheetSeams::default());
}

/// The sheet across x = 2 with its edge bulging to `x = 2 + sag` along the way: an arc of the
/// circle through (2, 0) and (2, 1), over a face to its right.
fn bulged(sag: f64) -> Brep {
    let r = (0.25 + sag * sag) / (2.0 * sag);
    let cx = 2.0 + sag - r;
    let theta = fmath::atan2(0.5, 2.0 - cx);
    let mut s = Sheet::new();
    let (v0, v1) = (s.vertex([2.0, 0.0, 0.0]), s.vertex([2.0, 1.0, 0.0]));
    let circle = Circle::new(
        frame([cx, 0.5, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        Length::from_mm(r),
        Sense::Forward,
    )
    .unwrap();
    let arc = s.edge(Curve::Circle(circle), -theta, theta, [v0, v1]);
    let (e1, _) = s.line([2.0, 0.0, 0.0], [4.0, 0.0, 0.0]);
    let (e2, _) = s.line([4.0, 0.0, 0.0], [4.0, 1.0, 0.0]);
    let (e3, _) = s.line([4.0, 1.0, 0.0], [2.0, 1.0, 0.0]);
    s.face(
        Surface::Plane(Plane::new(Frame::WORLD)),
        vec![(e1, FWD), (e2, FWD), (e3, FWD), (arc, REV)],
    )
}

#[test]
fn edges_that_part_between_their_ends_are_unproved_until_the_zone_covers_the_gap() {
    let a = rectangle(&Frame::WORLD, [0.0, 2.0], [0.0, 1.0]);
    let b = bulged(0.3);
    // the arc leaves the chord by 0.3: a zone of 0.1 does not hold the run
    let narrow = seams(&[&a, &b], 0.1);
    assert!(narrow.seams.is_empty(), "{narrow:?}");
    assert_eq!(narrow.unproved.len(), 1);
    assert_eq!(narrow.unproved[0].sheets, [0, 1]);
    // a zone of 0.5 holds it, and the planes agree all along
    let wide = seams(&[&a, &b], 0.5);
    assert!(wide.unproved.is_empty(), "{wide:?}");
    assert_eq!(wide.seams.len(), 1);
    assert_eq!(wide.seams[0].continuity.proved, Some(Order::G4));
}

/// The patch of the unit cylinder about z over `u` in `[0, pi/2]` and `z` in `[0, 2]`, its
/// normal outward.
fn quarter_pipe() -> Brep {
    let mut s = Sheet::new();
    let bottom_frame = frame([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let top_frame = frame([0.0, 0.0, 2.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let (a, b, c, d) = (
        s.vertex([1.0, 0.0, 0.0]),
        s.vertex([0.0, 1.0, 0.0]),
        s.vertex([0.0, 1.0, 2.0]),
        s.vertex([1.0, 0.0, 2.0]),
    );
    let circle = |f: Frame| Curve::Circle(Circle::new(f, Length::from_mm(1.0), Sense::Forward).unwrap());
    let low = s.edge(circle(bottom_frame), 0.0, FRAC_PI_2, [a, b]);
    let high = s.edge(circle(top_frame), 0.0, FRAC_PI_2, [d, c]);
    let line = |s: &mut Sheet, p0: [f64; 3], p1: [f64; 3], ends: [VertexId; 2]| {
        let l = Line::new(p(p0), p(p1) - p(p0)).unwrap();
        s.edge(Curve::Line(l), 0.0, 1.0, ends)
    };
    let right = line(&mut s, [0.0, 1.0, 0.0], [0.0, 1.0, 2.0], [b, c]);
    let seam = line(&mut s, [1.0, 0.0, 0.0], [1.0, 0.0, 2.0], [a, d]);
    let wall = Surface::Cylinder(
        Cylinder::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap(),
    );
    s.face(
        wall,
        vec![(low, FWD), (right, FWD), (high, REV), (seam, REV)],
    )
}

/// The rectangle of the plane x = 1 over `y` in `[-1, 0]` and `z` in `[0, 2]`, tangent to the
/// pipe along its seam. `normal_x` is the sign of the plane's normal along x.
fn tangent_plane(normal_x: f64) -> Brep {
    let f = if normal_x > 0.0 {
        frame([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    } else {
        frame([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0])
    };
    let mut s = Sheet::new();
    let (e0, _) = s.line([1.0, -1.0, 0.0], [1.0, 0.0, 0.0]);
    let (e1, _) = s.line([1.0, 0.0, 0.0], [1.0, 0.0, 2.0]);
    let (e2, _) = s.line([1.0, 0.0, 2.0], [1.0, -1.0, 2.0]);
    let (e3, _) = s.line([1.0, -1.0, 2.0], [1.0, -1.0, 0.0]);
    let loop_ = if normal_x > 0.0 {
        vec![(e0, FWD), (e1, FWD), (e2, FWD), (e3, FWD)]
    } else {
        vec![(e3, REV), (e2, REV), (e1, REV), (e0, REV)]
    };
    s.face(Surface::Plane(Plane::new(f)), loop_)
}

#[test]
fn a_plane_tangent_to_a_cylinder_is_g1_and_its_curvature_jumps_by_one_over_r() {
    let pipe = quarter_pipe();
    for normal_x in [1.0, -1.0] {
        // either way the plane faces, it is read on the side that agrees across the seam
        let plane = tangent_plane(normal_x);
        for order in [[&pipe, &plane], [&plane, &pipe]] {
            let got = seams(&order, 1e-6);
            assert_eq!(got.seams.len(), 1, "{normal_x}: {got:?}");
            let c = got.seams[0].continuity;
            assert_eq!(c.proved, Some(Order::G1), "{normal_x}: {c:?}");
            let Some(Limit::Refuted { order: o, drop, .. }) = c.limit else {
                panic!("{c:?}")
            };
            assert_eq!(o, Order::G2);
            // the pipe bends by 1 / r = 1 across the seam, the plane by 0; which sheet is first
            // sets the sign
            let sign = if std::ptr::eq(order[0], &pipe) { -1.0 } else { 1.0 };
            assert!(
                drop.lo <= sign + 1e-9 && drop.hi >= sign - 1e-9,
                "{normal_x}: {drop:?}"
            );
            assert!(drop.hi - drop.lo < 1e-6);
            near(c.measured[1].bracket.hi, 0.0, 1e-9);
        }
    }
}

/// The whole unit pipe of height 2 about z as one wall sheet, with its two rims and its seam.
fn pipe() -> Brep {
    let mut s = Sheet::new();
    let (lo, hi) = (s.vertex([1.0, 0.0, 0.0]), s.vertex([1.0, 0.0, 2.0]));
    let circle = |z: f64| {
        Curve::Circle(
            Circle::new(
                frame([0.0, 0.0, z], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
                Length::from_mm(1.0),
                Sense::Forward,
            )
            .unwrap(),
        )
    };
    let low = s.edge(circle(0.0), 0.0, 2.0 * PI, [lo, lo]);
    let high = s.edge(circle(2.0), 0.0, 2.0 * PI, [hi, hi]);
    let l = Line::new(p([1.0, 0.0, 0.0]), Vec3::new(0.0, 0.0, 2.0)).unwrap();
    let seam = s.edge(Curve::Line(l), 0.0, 1.0, [lo, hi]);
    let wall = Surface::Cylinder(
        Cylinder::new(Frame::WORLD, Length::from_mm(1.0), Sense::Forward).unwrap(),
    );
    s.face(
        wall,
        vec![(low, FWD), (seam, FWD), (high, REV), (seam, REV)],
    )
}

/// The unit disc of the plane at height `z` (normal up).
fn disc(z: f64) -> Brep {
    let mut s = Sheet::new();
    let v = s.vertex([1.0, 0.0, z]);
    let c = Circle::new(
        frame([0.0, 0.0, z], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        Length::from_mm(1.0),
        Sense::Forward,
    )
    .unwrap();
    let rim = s.edge(Curve::Circle(c), 0.0, 2.0 * PI, [v, v]);
    s.face(
        Surface::Plane(Plane::new(frame([0.0, 0.0, z], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]))),
        vec![(rim, FWD)],
    )
}

#[test]
fn a_disc_capping_a_tube_meets_it_along_the_whole_circle_at_a_right_angle() {
    let (tube, bottom, top) = (pipe(), disc(0.0), disc(2.0));
    let got = seams(&[&tube, &bottom, &top], 1e-6);
    assert!(got.unproved.is_empty(), "{got:?}");
    assert_eq!(got.seams.len(), 2, "{got:?}");
    for (s, sheet) in got.seams.iter().zip([1, 2]) {
        assert_eq!(s.meet.sheets, [0, sheet]);
        near(s.along[0][0], 0.0, 1e-12);
        near(s.along[0][1], 2.0 * PI, 1e-12);
        // the faces meet, and at a right angle all along
        assert_eq!(s.continuity.proved, Some(Order::G0));
        let Some(Limit::Refuted { order, .. }) = s.continuity.limit else {
            panic!("{s:?}")
        };
        assert_eq!(order, Order::G1);
        let angle = s.continuity.measured[1].bracket;
        assert!(angle.lo <= FRAC_PI_2 + 1e-9 && angle.hi >= FRAC_PI_2 - 1e-9, "{angle:?}");
    }
    // the discs are two sheets that do not meet each other
    let apart = seams(&[&bottom, &top], 1e-6);
    assert!(apart.seams.is_empty() && apart.unproved.is_empty());
}

#[test]
fn refusals_budget_stop_and_determinism() {
    let a = rectangle(&Frame::WORLD, [0.0, 2.0], [0.0, 1.0]);
    let b = rectangle(&Frame::WORLD, [2.0, 4.0], [0.0, 1.0]);
    let (mut budget, stop) = (Budget::unlimited(), Stop::new());
    for zone in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            sheet_seams(&[&a, &b], Length::from_mm(zone), &tol(), &mut budget, &stop),
            Err(BodyqError::Refused(BodyqRefusal::Tolerance)),
            "{zone}"
        );
    }
    let mut bad = tol();
    bad.curvature = -1.0;
    assert_eq!(
        sheet_seams(&[&a, &b], Length::from_mm(1e-3), &bad, &mut budget, &stop),
        Err(BodyqError::Refused(BodyqRefusal::Tolerance))
    );
    let run = |budget: &mut Budget| {
        let got = sheet_seams(&[&a, &b], Length::from_mm(1e-3), &tol(), budget, &stop);
        (got, budget.used())
    };
    let (first, work) = run(&mut Budget::unlimited());
    assert_eq!(run(&mut Budget::unlimited()), (first.clone(), work));
    assert!(first.is_ok() && work > 0);
    assert!(matches!(
        run(&mut Budget::new(work - 1)).0,
        Err(BodyqError::Exhausted { .. })
    ));
    let halt = Stop::new();
    halt.set();
    assert_eq!(
        sheet_seams(&[&a, &b], Length::from_mm(1e-3), &tol(), &mut Budget::unlimited(), &halt),
        Err(BodyqError::Cancelled)
    );
}
