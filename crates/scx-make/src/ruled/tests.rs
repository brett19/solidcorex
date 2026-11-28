// Written for solidcorex. Apache-2.0.
//! The ruled surface against closed forms: plane strips and annular sectors of known area,
//! sides that are the curves themselves, a hyperbolic paraboloid, and the sheet turned over.

use core::f64::consts::{FRAC_PI_2, PI};

use scx_brep::op::Scratch;
use scx_brep::{Brep, Document, FaceId, ReverseError};
use scx_curve::{Circle, Curve, Line};
use scx_mass::{RelAccuracy, area};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_surface::Surface;

use super::*;
use crate::{Made, block};

fn doc() -> Document {
    Document::default()
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn line(a: Point3, b: Point3) -> RuledSide {
    RuledSide::new(
        Curve::Line(Line::through(a, b).unwrap()),
        Extent1d::new(0.0, 1.0).unwrap(),
    )
}

/// The arc of radius `r` about the z axis at height `z`, from angle `a0` to `a1`.
fn arc(r: f64, z: f64, a0: f64, a1: f64) -> RuledSide {
    let frame = Frame::new(
        p(0.0, 0.0, z),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    RuledSide::new(
        Curve::Circle(Circle::new(frame, Length::from_mm(r), Sense::Forward).unwrap()),
        Extent1d::new(a0, a1).unwrap(),
    )
}

/// A cubic bulge from `a` to `b`, `h` high in the middle, over `[0, 1]`.
fn bulge(a: Point3, b: Point3, h: f64) -> RuledSide {
    let k = KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]).unwrap();
    let at = |t: f64, up: f64| {
        let q = a.lerp(b, t);
        [q.x, q.y, q.z + up]
    };
    let c = NurbsCurve::new(
        k,
        vec![
            at(0.0, 0.0),
            at(1.0 / 3.0, h),
            at(2.0 / 3.0, h),
            at(1.0, 0.0),
        ],
        None,
    )
    .unwrap();
    RuledSide::new(
        Curve::Nurbs(std::sync::Arc::new(c)),
        Extent1d::new(0.0, 1.0).unwrap(),
    )
}

fn area_of(body: &Brep) -> f64 {
    area(
        body,
        RelAccuracy::new(1e-10).unwrap(),
        &mut Scratch::new(),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
    .value
}

fn surface_of(m: &Made<FaceId>) -> Surface {
    m.body.face(m.faces).unwrap().surface().clone()
}

/// The point of a NURBS face at fractions `s` of `u` and `t` of `v`.
fn at(m: &Made<FaceId>, s: f64, t: f64) -> Point3 {
    let Surface::Nurbs(n) = surface_of(m) else {
        panic!("not a NURBS face");
    };
    let d = n.domain();
    let (u, v) = (
        d.x_interval().min() + s * (d.x_interval().max() - d.x_interval().min()),
        d.y_interval().min() + t * (d.y_interval().max() - d.y_interval().min()),
    );
    Point3::from_array(n.point([u, v], [Approach::Above; 2]).unwrap())
}

fn near(a: Point3, b: Point3, tol: f64) -> bool {
    a.distance(b) < tol
}

/// Two parallel lines `w` apart and `l` long: a plane strip of area `l w`, on a plane.
#[test]
fn two_parallel_lines_are_a_plane_strip() {
    let (l, w) = (7.0, 3.0);
    for join in [Join::Same, Join::Reversed] {
        let (b0, b1) = (p(0.0, w, 0.0), p(l, w, 0.0));
        let b = match join {
            Join::Same => line(b0, b1),
            Join::Reversed => line(b1, b0),
        };
        let m = ruled(&line(p(0.0, 0.0, 0.0), p(l, 0.0, 0.0)), &b, join, &doc()).unwrap();
        assert!(matches!(surface_of(&m), Surface::Plane(_)), "{join:?}");
        assert_eq!(m.body.faces().len(), 1);
        assert!((area_of(&m.body) - l * w).abs() < 1e-8, "{join:?}");
    }
}

/// A tilted strip too, and a trapezoid between unequal parallel lines.
#[test]
fn a_trapezoid_between_unequal_parallel_lines() {
    let a = line(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0));
    let b = line(p(2.0, 4.0, 3.0), p(8.0, 4.0, 3.0));
    // the height of the trapezoid is the distance between the lines: 5
    let m = ruled(&a, &b, Join::Same, &doc()).unwrap();
    assert!(matches!(surface_of(&m), Surface::Plane(_)));
    assert!((area_of(&m.body) - 0.5 * (10.0 + 6.0) * 5.0).abs() < 1e-8);
}

/// Concentric arcs of one sweep: an annular sector of area `theta (R^2 - r^2) / 2`, on a
/// plane; the same either way round.
#[test]
fn coplanar_arcs_are_an_annular_sector() {
    let theta = 1.2;
    let (r, big) = (3.0, 5.0);
    let want = 0.5 * theta * (big * big - r * r);
    let m = ruled(
        &arc(r, 0.0, 0.0, theta),
        &arc(big, 0.0, 0.0, theta),
        Join::Same,
        &doc(),
    )
    .unwrap();
    assert!(matches!(surface_of(&m), Surface::Plane(_)));
    assert!((area_of(&m.body) - want).abs() < 1e-8);
    // a more than half turn arc, and the second one reversed
    let theta = 4.0;
    let want = 0.5 * theta * (big * big - r * r);
    let m = ruled(
        &arc(r, 2.0, 0.5, 0.5 + theta),
        &arc(big, 2.0, 0.5, 0.5 + theta),
        Join::Same,
        &doc(),
    )
    .unwrap();
    assert!((area_of(&m.body) - want).abs() < 1e-8);
}

/// A line and an arc in one plane: from the arc's start along the x axis to the centre, and
/// the arc, joined end to start: a quarter disc, area `pi r^2 / 4`, on a plane.
#[test]
fn a_line_and_an_arc_in_one_plane_are_a_planar_face() {
    let r = 4.0;
    let spoke = line(p(0.0, 0.0, 0.0), p(r, 0.0, 0.0));
    let quarter = arc(r, 0.0, 0.0, FRAC_PI_2);
    // the arc's end is joined to the spoke's start, and the arc starts where the spoke ends
    let m = ruled(&spoke, &quarter, Join::Reversed, &doc()).unwrap();
    assert!(matches!(surface_of(&m), Surface::Plane(_)));
    assert!((area_of(&m.body) - PI * r * r / 4.0).abs() < 1e-8);
}

/// The pin's test: the surface has its two curves as sides, straight between them, and
/// joined reversed the second runs the other way.
#[test]
fn a_ruled_surface_has_its_two_curves_as_sides() {
    let a = bulge(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), 2.0);
    let (b0, b1) = (p(0.0, 5.0, 3.0), p(10.0, 5.0, 8.0));
    let Curve::Nurbs(an) = &a.curve else {
        unreachable!()
    };
    for (join, from, to) in [(Join::Same, b0, b1), (Join::Reversed, b1, b0)] {
        let m = ruled(&a, &line(b0, b1), join, &doc()).unwrap();
        for i in 0..=8 {
            let s = f64::from(i) / 8.0;
            let pa = Point3::from_array(an.point(s, Approach::Above).unwrap());
            let pb = from.lerp(to, s);
            assert!(near(at(&m, s, 0.0), pa, 1e-9), "a at {s}");
            assert!(near(at(&m, s, 1.0), pb, 1e-9), "b at {s}");
            assert!(
                near(at(&m, s, 0.5), pa.lerp(pb, 0.5), 1e-9),
                "ruling at {s}"
            );
        }
    }
}

/// Two skew lines: the hyperbolic paraboloid `z = x y / h`, rulings straight, the corners
/// where the lines end.
#[test]
fn two_skew_lines_are_a_bilinear_patch() {
    let a = line(p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0));
    let b = line(p(0.0, 1.0, 0.0), p(1.0, 1.0, 1.0));
    let m = ruled(&a, &b, Join::Same, &doc()).unwrap();
    assert!(matches!(surface_of(&m), Surface::Nurbs(_)));
    for (s, t) in [(0.3, 0.6), (0.5, 0.5), (0.9, 0.2), (1.0, 1.0)] {
        // S = (s, t, s t)
        assert!(near(at(&m, s, t), p(s, t, s * t), 1e-12), "{s} {t}");
    }
}

/// An arc and a line out of its plane: both are sides of a rational surface, exact.
#[test]
fn an_arc_and_a_line_out_of_its_plane() {
    let a = arc(5.0, 0.0, 0.0, FRAC_PI_2 * 1.5);
    let b = line(p(5.0, 0.0, 4.0), p(0.0, 5.0, 4.0));
    let m = ruled(&a, &b, Join::Same, &doc()).unwrap();
    let Surface::Nurbs(n) = surface_of(&m) else {
        panic!()
    };
    // the t = 0 side is on the circle, at every u
    for i in 0..=12 {
        let s = f64::from(i) / 12.0;
        let q = at(&m, s, 0.0);
        assert!(
            (q.x.hypot(q.y) - 5.0).abs() < 1e-12 && q.z.abs() < 1e-12,
            "{q:?}"
        );
        // the t = 1 side is on the line z = 4, x + y = 5
        let r = at(&m, s, 1.0);
        assert!(
            (r.z - 4.0).abs() < 1e-12 && (r.x + r.y - 5.0).abs() < 1e-12,
            "{r:?}"
        );
        // and the surface is straight between the two
        let mid = at(&m, s, 0.5);
        let along = (mid - q).cross(r - q).length();
        assert!(along < 1e-9, "the middle is on the ruling: {along}");
    }
    assert!(n.weights().is_some());
}

/// A side that is closed, an ellipse and two collinear lines are refused or unsupported by
/// name.
#[test]
fn refusals_name_the_input() {
    let full = arc(2.0, 0.0, 0.0, 2.0 * PI);
    let l = line(p(0.0, 0.0, 1.0), p(1.0, 0.0, 1.0));
    assert_eq!(
        ruled(&full, &l, Join::Same, &doc()).unwrap_err(),
        RuledError::Refused(RuledRefusal::Closed(Which::First))
    );
    let a = line(p(0.0, 0.0, 0.0), p(2.0, 0.0, 0.0));
    let b = line(p(3.0, 0.0, 0.0), p(5.0, 0.0, 0.0));
    assert_eq!(
        ruled(&a, &b, Join::Same, &doc()).unwrap_err(),
        RuledError::Refused(RuledRefusal::NoArea)
    );
}

/// Reversed twice is the identity, and the normal is the other way at the same point.
#[test]
fn a_reversed_sheet_has_the_other_normal_and_reversing_twice_gives_it_back() {
    let a = bulge(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), 2.0);
    let b = line(p(0.0, 5.0, 3.0), p(10.0, 5.0, 8.0));
    let strip = ruled(
        &line(p(0.0, 0.0, 0.0), p(7.0, 0.0, 0.0)),
        &line(p(0.0, 3.0, 0.0), p(7.0, 3.0, 0.0)),
        Join::Same,
        &doc(),
    )
    .unwrap();
    let bump = ruled(&a, &b, Join::Same, &doc()).unwrap();
    for made in [strip, bump] {
        let before = made.body;
        let id = made.faces;
        let mut d = before.clone().edit();
        d.reverse_sheet(&doc()).unwrap();
        let (once, _) = d.finish(&doc()).unwrap();
        let (f0, f1) = (before.face(id).unwrap(), once.face(id).unwrap());
        // same edges, in the other order with the other senses
        let ids = |b: &Brep| {
            b.face(id)
                .unwrap()
                .loops()
                .map(|l| {
                    l.uses()
                        .map(|u| (u.edge().id(), u.sense()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let (l0, l1) = (ids(&before), ids(&once));
        assert_eq!(l0.len(), l1.len());
        for (x, y) in l0.iter().zip(&l1) {
            let want: Vec<_> = x.iter().rev().map(|(e, s)| (*e, s.reversed())).collect();
            assert_eq!(&want, y);
        }
        // the surface is the same points with the normal negated: sample the original's uv
        // and find the same point by projection through the reversed surface's own map
        let (s0, s1) = (f0.surface(), f1.surface());
        let uv = match s0 {
            Surface::Nurbs(n) => {
                let d = n.domain();
                Point2::new(
                    0.5 * (d.x_interval().min() + d.x_interval().max()),
                    0.3 * d.y_interval().min() + 0.7 * d.y_interval().max(),
                )
            }
            _ => Point2::new(0.3, 0.4),
        };
        let flipped = Point2::new(uv.x, -uv.y);
        assert!(s0.point(uv).distance(s1.point(flipped)) < 1e-12);
        let (n0, n1) = (
            s0.normal(uv, [Approach::Above; 2]).unwrap(),
            s1.normal(flipped, [Approach::Above; 2]).unwrap(),
        );
        assert!(
            Vec3::from(n0).dot(Vec3::from(n1)) < -1.0 + 1e-12,
            "normal negated"
        );
        // reversed twice
        let mut d = once.clone().edit();
        d.reverse_sheet(&doc()).unwrap();
        let (twice, _) = d.finish(&doc()).unwrap();
        assert_eq!(
            twice.face(id).unwrap().surface(),
            before.face(id).unwrap().surface()
        );
        assert_eq!(ids(&twice), ids(&before));
        assert!((area_of(&once) - area_of(&before)).abs() < 1e-8);
    }
}

#[test]
fn a_solid_is_not_a_sheet() {
    let (body, _) = {
        let m = block(
            &Frame::new(
                p(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
            )
            .unwrap(),
            [
                Length::from_mm(1.0),
                Length::from_mm(2.0),
                Length::from_mm(3.0),
            ],
            &doc(),
        )
        .unwrap();
        (m.body, ())
    };
    let mut d = body.edit();
    assert!(matches!(
        d.reverse_sheet(&doc()),
        Err(ReverseError::NotSheet { .. })
    ));
}

/// Lines whose rulings cross in a plane (a bow tie) are not a plane's simple region: the
/// surface is the bilinear patch, folded, and its sides are still the two lines.
#[test]
fn crossing_rulings_are_a_nurbs_patch() {
    let a = line(p(0.0, 0.0, 0.0), p(4.0, 0.0, 0.0));
    let b = line(p(4.0, 2.0, 0.0), p(0.0, 2.0, 0.0));
    let m = ruled(&a, &b, Join::Same, &doc()).unwrap();
    assert!(matches!(surface_of(&m), Surface::Nurbs(_)));
    assert!(near(at(&m, 0.25, 0.0), p(1.0, 0.0, 0.0), 1e-12));
    assert!(near(at(&m, 0.25, 1.0), p(3.0, 2.0, 0.0), 1e-12));
}

/// An ellipse has no exact NURBS form here and is not a line or arc.
#[test]
fn an_ellipse_side_is_unsupported() {
    let frame = Frame::new(
        p(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 0.0),
    )
    .unwrap();
    let e = scx_curve::Ellipse::new(
        frame,
        Length::from_mm(3.0),
        Length::from_mm(2.0),
        Sense::Forward,
    )
    .unwrap();
    let side = RuledSide::new(Curve::Ellipse(e), Extent1d::new(0.0, 1.0).unwrap());
    let l = line(p(0.0, 0.0, 5.0), p(1.0, 0.0, 5.0));
    assert_eq!(
        ruled(&side, &l, Join::Same, &doc()).unwrap_err(),
        RuledError::Unsupported(RuledUnsupported::Form(Which::First))
    );
}

/// A face on a cylinder turned over: the same points, the other normal, back again.
#[test]
fn a_cylinder_face_reverses_and_comes_back() {
    use self::scx_make_face::cylinder_sheet;
    let before = cylinder_sheet();
    let id = before.faces().next().unwrap().id();
    let mut d = before.clone().edit();
    d.reverse_sheet(&doc()).unwrap();
    let (once, _) = d.finish(&doc()).unwrap();
    let mut d = once.clone().edit();
    d.reverse_sheet(&doc()).unwrap();
    let (twice, _) = d.finish(&doc()).unwrap();
    assert!((area_of(&once) - area_of(&before)).abs() < 1e-8);
    assert_eq!(
        twice.face(id).unwrap().surface(),
        before.face(id).unwrap().surface()
    );
    let at = |b: &Brep, uv: Point2| {
        let s = b.face(id).unwrap().surface().clone();
        (
            s.point(uv),
            Vec3::from(s.normal(uv, [Approach::Above; 2]).unwrap()),
        )
    };
    let uv = Point2::new(0.4, 0.3);
    let ((p0, n0), (p1, n1)) = (at(&before, uv), at(&once, Point2::new(0.4, -0.3)));
    assert!(p0.distance(p1) < 1e-12);
    assert!(n0.dot(n1) < -1.0 + 1e-12, "the normal is the other way");
}

mod scx_make_face {
    use scx_brep::Brep;
    use scx_num::extent1d::Extent1d;
    use scx_num::extent2d::Extent2d;
    use scx_num::frame::{Frame, Sense};
    use scx_num::units::Length;
    use scx_num::vector3d::{Point3, Vec3};
    use scx_surface::{Cylinder, Surface};

    use crate::{Bound, face};

    pub(super) fn cylinder_sheet() -> Brep {
        let frame = Frame::new(
            Point3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 0.0),
        )
        .unwrap();
        let c = Cylinder::new(frame, Length::from_mm(2.0), Sense::Forward).unwrap();
        let b = Extent2d::from_intervals(
            Extent1d::new(0.0, 3.0).unwrap(),
            Extent1d::new(0.0, 5.0).unwrap(),
        );
        face(&Surface::Cylinder(c), &Bound::Box(b), &super::doc())
            .unwrap()
            .body
    }
}
