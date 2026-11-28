// Written for solidcorex. Apache-2.0.
//! [`project_curve`] and [`drop_curve`] (research 75 SF1) against their closed forms: a circle
//! projected onto a plane is its exact ellipse (semi-axes from the tilt, and the
//! conjugate-diameter invariants for any direction); a line dropped onto a cylinder in a
//! plane through its axis is a ruling; a line square to it is an arc. Every piece is also
//! sampled: each image point lies on the surface, and on the projection line through its
//! source point (or is the kind's own nearest point to it, for the drop). The pin's
//! `project_curves` and `drop_curves` tests that assert geometry are ported here (the face
//! and seam parts of them are `scx-bodyq`'s).

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic,
    clippy::disallowed_methods
)]

use core::f64::consts::PI;

use scx_curve::{Circle, Curve, Line};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::hash::Rng;
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_surface::{
    Closest, Cone, CurveKind, Cylinder, Plane, ProjectError, Projected, Sphere, Surface,
    SurfaceKind, drop_curve, project_curve,
};

const TOL: f64 = 1e-9;

fn res() -> Resolution {
    Resolution::new(Length::from_mm(1e-6)).unwrap()
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn unit(x: f64, y: f64, z: f64) -> UnitVec3 {
    v(x, y, z).normalized().unwrap()
}

fn ext(a: f64, b: f64) -> Extent1d {
    Extent1d::new(a, b).unwrap()
}

fn plane(o: Point3, n: Vec3) -> Surface {
    let x = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    Surface::Plane(Plane::new(Frame::new(o, n, x).unwrap()))
}

fn z_frame(o: Point3) -> Frame {
    Frame::new(o, Vec3::Z, Vec3::X).unwrap()
}

fn cylinder(r: f64) -> Surface {
    Surface::Cylinder(
        Cylinder::new(
            z_frame(p(0.0, 0.0, 0.0)),
            Length::from_mm(r),
            Sense::Forward,
        )
        .unwrap(),
    )
}

fn cone(r: f64, half: f64) -> Surface {
    Surface::Cone(
        Cone::new(
            z_frame(p(0.0, 0.0, 0.0)),
            Length::from_mm(r),
            half,
            Sense::Forward,
        )
        .unwrap(),
    )
}

fn segment(a: Point3, b: Point3) -> (Curve, Extent1d) {
    (Line::new(a, b - a).unwrap().into(), ext(0.0, 1.0))
}

fn circle(f: Frame, r: f64) -> Curve {
    Circle::new(f, Length::from_mm(r), Sense::Forward)
        .unwrap()
        .into()
}

fn closest(s: &Surface, q: Point3) -> Closest {
    match s {
        Surface::Plane(k) => k.closest(q),
        Surface::Cylinder(k) => k.closest(q),
        Surface::Cone(k) => k.closest(q),
        _ => panic!("not a kind under test"),
    }
}

fn distance(s: &Surface, q: Point3) -> f64 {
    match closest(s, q) {
        Closest::One(f) => f.distance,
        Closest::AlongU { distance, .. } => distance,
        other => panic!("{other:?}"),
    }
}

fn near(a: Point3, b: Point3, tol: f64) -> bool {
    (a - b).length() <= tol
}

/// The image point of the source's `t` on a piece whose parameter is kept.
fn image_of(piece: &Projected, t: f64) -> Point3 {
    piece.curve.point(piece.kept.unwrap().apply(t))
}

fn samples(e: Extent1d) -> impl Iterator<Item = f64> {
    (0..=16).map(move |k| e.min() + e.length() * f64::from(k) / 16.0)
}

/// Every image point is on `s` and on the projection line along `d` through its source
/// point; the image's extent maps the source's ends to its own.
fn check_projected(c: &Curve, s: &Surface, d: Vec3, pieces: &[Projected]) {
    for piece in pieces {
        let sh = piece.kept.unwrap();
        assert!((sh.apply(piece.from.min()) - piece.extent.min()).abs() <= TOL);
        assert!((sh.apply(piece.from.max()) - piece.extent.max()).abs() <= TOL);
        for t in samples(piece.from) {
            let (src, img) = (c.point(t), image_of(piece, t));
            let scale = 1.0 + src.to_vec().length();
            assert!(distance(s, img) <= TOL * scale, "off the surface at {t}");
            assert!(
                (img - src).cross(d).length() <= TOL * scale,
                "off the line at {t}"
            );
        }
    }
}

/// Every image point is the kind's nearest point to its source point. A piece's end on the
/// axis (or at a cone's apex), where the feet are a set, is checked by the image lying on the
/// surface, as the limit of the feet beside it.
fn check_dropped(c: &Curve, s: &Surface, pieces: &[Projected]) {
    for piece in pieces {
        for t in samples(piece.from) {
            let (src, img) = (c.point(t), image_of(piece, t));
            let Closest::One(f) = closest(s, src) else {
                assert!(
                    t == piece.from.min() || t == piece.from.max(),
                    "no single foot at {t}"
                );
                assert!(distance(s, img) <= TOL * (1.0 + src.to_vec().length()));
                continue;
            };
            assert!(
                near(img, f.point, TOL * (1.0 + src.to_vec().length())),
                "{t}"
            );
        }
    }
}

fn semi_axes(c: &Curve) -> (f64, f64, Point3) {
    let Curve::Ellipse(e) = c else {
        panic!("not an ellipse: {c:?}")
    };
    let (a, b) = (e.semi_x().mm(), e.semi_y().mm());
    (a.max(b), a.min(b), e.frame().origin())
}

// ------------------------------------------------------------------ the plane

#[test]
fn a_circle_projected_square_onto_a_tilted_plane_is_its_ellipse() {
    // A circle of radius 3 at height 5, projected along z onto a plane tilted by beta: the
    // ellipse with semi-axes 3 (along the tilt's hinge) and 3 / cos(beta).
    for beta in [0.2f64, 0.7, 1.3] {
        let n = v(beta.sin(), 0.0, beta.cos());
        let s = plane(p(0.0, 0.0, -1.0), n);
        let c = circle(z_frame(p(1.0, 2.0, 5.0)), 3.0);
        let e = ext(0.0, 2.0 * PI);
        let out = project_curve(&c, e, &s, UnitVec3::Z, res()).unwrap();
        assert_eq!(out.len(), 1);
        let (a, b, centre) = semi_axes(&out[0].curve);
        assert!((a - 3.0 / beta.cos()).abs() <= TOL * a, "{beta}: {a}");
        assert!((b - 3.0).abs() <= TOL, "{beta}: {b}");
        // The centre is the centre's image: (1, 2) over the plane.
        let h = -1.0 - (1.0 * n.x) / n.z;
        assert!(near(centre, p(1.0, 2.0, h), TOL), "{centre:?}");
        check_projected(&c, &s, Vec3::Z, &out);
    }
}

#[test]
fn a_tilted_circle_projected_along_z_onto_a_level_plane_shrinks_by_the_cosine() {
    for gamma in [0.3f64, 1.0, 1.5] {
        let n = v(0.0, gamma.sin(), gamma.cos());
        let c = circle(Frame::new(p(4.0, -1.0, 7.0), n, Vec3::X).unwrap(), 2.5);
        let s = plane(p(0.0, 0.0, 0.0), Vec3::Z);
        let e = ext(0.3, 2.0);
        let out = project_curve(&c, e, &s, UnitVec3::Z, res()).unwrap();
        let (a, b, centre) = semi_axes(&out[0].curve);
        assert!(
            (a - 2.5).abs() <= TOL && (b - 2.5 * gamma.cos()).abs() <= TOL,
            "{a} {b}"
        );
        assert!(near(centre, p(4.0, -1.0, 0.0), TOL));
        check_projected(&c, &s, Vec3::Z, &out);
    }
}

#[test]
fn a_circle_projected_along_any_direction_keeps_the_conjugate_diameters_invariants() {
    // The image is c' + u cos t + v sin t with u, v the projected radius vectors: a conjugate
    // pair, so a^2 + b^2 = |u|^2 + |v|^2 and a b = |u x v|.
    let mut rng = Rng::new(0x5f1);
    let mut r = || rng.next_signed_unit();
    for _ in 0..64 {
        let f = Frame::new(
            p(r() * 9.0, r() * 9.0, r() * 9.0),
            v(r(), r(), r()),
            v(r(), r(), r()),
        )
        .unwrap();
        let n = v(r(), r(), r());
        let d = v(r(), r(), r()).normalized().unwrap();
        if d.dot(n).abs() < 0.2 * n.length() || d.dot(*f.z()).abs() < 0.2 {
            continue;
        }
        let s = plane(p(r(), r(), r()), n);
        let c = circle(f, 1.5);
        let out = project_curve(&c, ext(0.0, 2.0 * PI), &s, d, res()).unwrap();
        let nn = n.normalized().unwrap();
        let proj = |w: Vec3| w - *d * (w.dot(*nn) / d.dot(*nn));
        let (u, w) = (proj(*f.x() * 1.5), proj(*f.y() * 1.5));
        let (a, b, _) = semi_axes(&out[0].curve);
        let sum = u.dot(u) + w.dot(w);
        assert!((a * a + b * b - sum).abs() <= 1e-9 * sum);
        assert!((a * b - u.cross(w).length()).abs() <= 1e-9 * sum);
        check_projected(&c, &s, *d, &out);
    }
}

#[test]
fn a_segment_projects_onto_two_levels_at_their_heights() {
    // The pin's `a_segment_projects_onto_a_box_s_record_face_at_its_index`: (2,3,20)-(8,3,20)
    // along -z lands on the top (z = 10) and the bottom (z = 0).
    let (c, e) = segment(p(2.0, 3.0, 20.0), p(8.0, 3.0, 20.0));
    let d = unit(0.0, 0.0, -3.0);
    for h in [10.0, 0.0] {
        let s = plane(p(0.0, 0.0, h), Vec3::Z);
        let out = project_curve(&c, e, &s, d, res()).unwrap();
        let Curve::Line(_) = out[0].curve else {
            panic!("{:?}", out[0].curve)
        };
        assert!(near(image_of(&out[0], 0.0), p(2.0, 3.0, h), TOL));
        assert!(near(image_of(&out[0], 1.0), p(8.0, 3.0, h), TOL));
        check_projected(&c, &s, *d, &out);
    }
}

#[test]
fn a_line_above_a_level_drops_to_its_foot() {
    // The pin's `a_line_above_a_box_drops_to_its_foot_on_the_top`.
    let (c, e) = segment(p(2.0, 5.0, 15.0), p(8.0, 5.0, 15.0));
    let s = plane(p(0.0, 0.0, 10.0), Vec3::Z);
    let out = drop_curve(&c, e, &s, res()).unwrap();
    assert!(near(image_of(&out[0], 0.0), p(2.0, 5.0, 10.0), TOL));
    assert!(near(image_of(&out[0], 1.0), p(8.0, 5.0, 10.0), TOL));
    check_dropped(&c, &s, &out);
}

#[test]
fn a_plane_refuses_a_direction_in_it_and_a_circle_seen_edge_on() {
    let s = plane(p(0.0, 0.0, 0.0), Vec3::Z);
    let (c, e) = segment(p(0.0, 0.0, 1.0), p(1.0, 0.0, 1.0));
    assert_eq!(
        project_curve(&c, e, &s, UnitVec3::X, res()),
        Err(ProjectError::Along)
    );
    // A circle in the xz plane seen along x, onto the plane x = 0: a segment.
    let c = circle(Frame::new(p(3.0, 0.0, 0.0), Vec3::Y, Vec3::X).unwrap(), 1.0);
    let s = plane(p(0.0, 0.0, 0.0), Vec3::X);
    let r = project_curve(&c, ext(0.0, 1.0), &s, UnitVec3::X, res());
    assert_eq!(r, Err(ProjectError::Flattened));
    assert_eq!(ProjectError::Flattened.kind(), ErrorKind::Refused);
}

// ------------------------------------------------------------------ the cylinder

#[test]
fn a_line_beside_a_cylinder_drops_onto_its_wall_at_the_radius() {
    // The pin's `a_line_beside_a_cylinder_drops_onto_its_wall_at_the_radius`: a line at
    // x = 8 dropped onto R = 5 is the ruling at x = 5, over the same heights.
    let s = cylinder(5.0);
    let (c, e) = segment(p(8.0, 0.0, 2.0), p(8.0, 0.0, 18.0));
    let out = drop_curve(&c, e, &s, res()).unwrap();
    assert_eq!(out.len(), 1);
    let Curve::Line(_) = out[0].curve else {
        panic!()
    };
    assert!(near(image_of(&out[0], 0.0), p(5.0, 0.0, 2.0), TOL));
    assert!(near(image_of(&out[0], 1.0), p(5.0, 0.0, 18.0), TOL));
    check_dropped(&c, &s, &out);
}

#[test]
fn a_slanted_line_through_the_axis_drops_to_a_ruling_on_each_side() {
    let s = cylinder(5.0);
    let (c, e) = segment(p(-8.0, 0.0, 0.0), p(8.0, 0.0, 4.0));
    let out = drop_curve(&c, e, &s, res()).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].from, out[1].from), (ext(0.0, 0.5), ext(0.5, 1.0)));
    assert!(near(image_of(&out[0], 0.0), p(-5.0, 0.0, 0.0), TOL));
    assert!(near(image_of(&out[1], 1.0), p(5.0, 0.0, 4.0), TOL));
    check_dropped(&c, &s, &out);
}

#[test]
fn a_chord_across_the_seam_drops_to_an_arc_at_its_height() {
    // The pin's #20836 case: the chord (20, -3, 20)-(20, 3, 20) on R = 10 drops to the arc at
    // z = 20 from (R d/h, -R a/h) to (R d/h, R a/h), h = hypot(d, a). (The pin's split at the
    // face's seam is the body's.)
    let (rr, d, a) = (10.0, 20.0, 3.0);
    let s = cylinder(rr);
    let (c, e) = segment(p(d, -a, 20.0), p(d, a, 20.0));
    let out = drop_curve(&c, e, &s, res()).unwrap();
    assert_eq!(out.len(), 1);
    let Curve::Circle(arc) = &out[0].curve else {
        panic!("{:?}", out[0].curve)
    };
    assert_eq!(arc.radius().mm(), rr);
    let h = (d * d + a * a).sqrt();
    let ends = (
        out[0].curve.point(out[0].extent.min()),
        out[0].curve.point(out[0].extent.max()),
    );
    assert!(
        near(ends.0, p(rr * d / h, -rr * a / h, 20.0), TOL),
        "{ends:?}"
    );
    assert!(
        near(ends.1, p(rr * d / h, rr * a / h, 20.0), TOL),
        "{ends:?}"
    );
    assert_eq!(out[0].kept, None);
    // Every foot of the chord lies on the arc, within its extent, in order.
    let mut last = f64::NEG_INFINITY;
    for t in samples(e) {
        let Closest::One(f) = closest(&s, c.point(t)) else {
            panic!()
        };
        let q = f.point - arc.frame().origin();
        let theta =
            (q.dot(*arc.frame().y()) * Sense::Forward.sign()).atan2(q.dot(*arc.frame().x()));
        let theta = if theta < out[0].extent.min() - 1e-12 {
            theta + 2.0 * PI
        } else {
            theta
        };
        assert!(out[0].extent.contains_value(theta, 1e-12));
        assert!(near(out[0].curve.point(theta), f.point, TOL));
        assert!(theta >= last);
        last = theta;
    }
    // The other way along, the arc runs the other way.
    let (c, e) = segment(p(d, a, 20.0), p(d, -a, 20.0));
    let back = drop_curve(&c, e, &s, res()).unwrap();
    let start = back[0].curve.point(back[0].extent.min());
    assert!(
        near(start, p(rr * d / h, rr * a / h, 20.0), TOL),
        "{start:?}"
    );
}

#[test]
fn a_line_on_the_axis_is_refused() {
    let (c, e) = segment(p(0.0, 0.0, 0.0), p(0.0, 0.0, 9.0));
    assert_eq!(
        drop_curve(&c, e, &cylinder(5.0), res()),
        Err(ProjectError::OnAxis)
    );
    assert_eq!(
        drop_curve(&c, e, &cone(1.0, 0.4), res()),
        Err(ProjectError::OnAxis)
    );
}

#[test]
fn a_ruling_projects_across_a_cylinder_to_its_two_hits() {
    let s = cylinder(5.0);
    let (c, e) = segment(p(8.0, 1.0, 0.0), p(8.0, 1.0, 6.0));
    let d = unit(-1.0, 0.0, 0.5);
    let out = project_curve(&c, e, &s, d, res()).unwrap();
    assert_eq!(out.len(), 2);
    let x = 24f64.sqrt();
    // Nearer hit first (the smaller distance along the line).
    for (piece, x) in out.iter().zip([x, -x]) {
        let q = image_of(piece, 0.0);
        assert!((q.x - x).abs() <= TOL && (q.y - 1.0).abs() <= TOL, "{q:?}");
        // s = (8 - x) / |d_xy| along d, so z rises by s d_z.
        let sd = (8.0 - x) / d.x.abs();
        assert!((q.z - sd * d.z).abs() <= TOL, "{q:?}");
    }
    check_projected(&c, &s, *d, &out);
    // Grazing: one hit; past the radius: none.
    let (c, e) = segment(p(8.0, 5.0, 0.0), p(8.0, 5.0, 6.0));
    assert_eq!(project_curve(&c, e, &s, d, res()).unwrap().len(), 1);
    let (c, e) = segment(p(8.0, 6.0, 0.0), p(8.0, 6.0, 6.0));
    assert_eq!(project_curve(&c, e, &s, d, res()), Ok(vec![]));
    assert_eq!(
        project_curve(&c, e, &s, UnitVec3::Z, res()),
        Err(ProjectError::Along)
    );
}

// ------------------------------------------------------------------ the cone

#[test]
fn a_meridian_line_projects_along_a_cone_s_axis_to_its_generator() {
    let s = cone(2.0, 0.3);
    let (c, e) = segment(p(3.0, 3.0, -4.0), p(6.0, 6.0, 9.0));
    let out = project_curve(&c, e, &s, UnitVec3::Z, res()).unwrap();
    assert_eq!(out.len(), 1);
    for t in samples(e) {
        let (q, src) = (image_of(&out[0], t), c.point(t));
        assert!((q.x - src.x).abs() <= TOL && (q.y - src.y).abs() <= TOL);
    }
    check_projected(&c, &s, Vec3::Z, &out);
    // Through the axis: a generator on each side, split where it crosses.
    let (c, e) = segment(p(-6.0, 0.0, 1.0), p(3.0, 0.0, 1.0));
    let out = project_curve(&c, e, &s, -UnitVec3::Z, res()).unwrap();
    assert_eq!(out.len(), 2);
    assert!((out[0].from.max() - 6.0 / 9.0).abs() <= TOL);
    check_projected(&c, &s, Vec3::Z, &out);
}

#[test]
fn a_meridian_line_drops_onto_a_cone_leaving_out_the_apex() {
    let s = cone(2.0, 0.3);
    let (c, e) = segment(p(-6.0, 0.0, 1.0), p(5.0, 0.0, 8.0));
    let out = drop_curve(&c, e, &s, res()).unwrap();
    assert_eq!(out.len(), 2);
    check_dropped(&c, &s, &out);
    // Below the apex: the feet are the apex until r + k^2 rho + k h > 0.
    let s = cone(0.0, 0.5);
    let (c, e) = segment(p(10.0, 0.0, -20.0), p(50.0, 0.0, -20.0));
    let out = drop_curve(&c, e, &s, res()).unwrap();
    assert_eq!(out.len(), 1);
    let k = 0.5f64.tan();
    let rho = 20.0 / k;
    assert!(
        (out[0].from.min() - (rho - 10.0) / 40.0).abs() <= TOL,
        "{:?}",
        out[0].from
    );
    check_dropped(&c, &s, &out);
    // Wholly below: nothing but the apex.
    let (c, e) = segment(p(1.0, 0.0, -20.0), p(5.0, 0.0, -20.0));
    assert_eq!(drop_curve(&c, e, &s, res()), Err(ProjectError::Flattened));
}

// ------------------------------------------------------------------ refusals

#[test]
fn pairs_without_a_closed_form_name_the_fitter() {
    let fitter = |r: Result<Vec<Projected>, ProjectError>, curve, surface| {
        let e = r.unwrap_err();
        assert_eq!(e, ProjectError::NeedsFitter { curve, surface });
        assert_eq!(
            (e.kind(), e.name()),
            (ErrorKind::Unsupported, "projection_needs_fitter")
        );
    };
    let o = circle(Frame::new(p(9.0, 0.0, 0.0), Vec3::X, Vec3::Y).unwrap(), 1.0);
    let full = ext(0.0, 2.0 * PI);
    fitter(
        drop_curve(&o, full, &cylinder(5.0), res()),
        CurveKind::Circle,
        SurfaceKind::Cylinder,
    );
    fitter(
        project_curve(&o, full, &cone(1.0, 0.3), UnitVec3::Z, res()),
        CurveKind::Circle,
        SurfaceKind::Cone,
    );
    // A slanted line projected onto a cylinder, or off a plane through the axis.
    let (c, e) = segment(p(8.0, 1.0, 0.0), p(9.0, 1.0, 6.0));
    let d = UnitVec3::X;
    fitter(
        project_curve(&c, e, &cylinder(5.0), d, res()),
        CurveKind::Line,
        SurfaceKind::Cylinder,
    );
    let (c, e) = segment(p(8.0, 1.0, 0.0), p(9.0, 2.0, 6.0));
    fitter(
        drop_curve(&c, e, &cylinder(5.0), res()),
        CurveKind::Line,
        SurfaceKind::Cylinder,
    );
    fitter(
        drop_curve(&c, e, &cone(1.0, 0.3), res()),
        CurveKind::Line,
        SurfaceKind::Cone,
    );
    // A meridian line projected onto a cone off its axis.
    let (c, e) = segment(p(3.0, 0.0, 0.0), p(6.0, 0.0, 1.0));
    fitter(
        project_curve(&c, e, &cone(1.0, 0.3), d, res()),
        CurveKind::Line,
        SurfaceKind::Cone,
    );
    let sphere = Surface::Sphere(
        Sphere::new(
            z_frame(p(0.0, 0.0, 0.0)),
            Length::from_mm(3.0),
            Sense::Forward,
        )
        .unwrap(),
    );
    fitter(
        drop_curve(&c, e, &sphere, res()),
        CurveKind::Line,
        SurfaceKind::Sphere,
    );
}
