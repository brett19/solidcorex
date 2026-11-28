// Written for solidcorex. Apache-2.0.
//! Silhouettes against their closed forms (research 74 §24, HL1): every curve is checked
//! on its surface with `n · d = 0`, and every piece against the face's own parameter box,
//! never against the pin's output.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::many_single_char_names
)]

use std::f64::consts::{FRAC_1_SQRT_2, TAU};
use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{
    Brep, Document, Draft, EdgeGeom, EdgeId, EdgeShape, EdgeUse, Face, FaceRecord, LoopRecord,
    RegionId, ShellId, ShellRecord, UseGeom, VertexId, VertexRecord,
};
use scx_curve::{Curve, Curve2, Line, Line2};
use scx_hlr::{
    BadView, Branch, Exactness, HlrError, OnFace, Silhouette, SilhouetteCurve, Silhouettes,
    View, Why, prepare, silhouettes,
};
use scx_num::budget::{Budget, Stop};
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::invert::Inversion;
use scx_num::units::Length;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, KnotVector, NurbsCurve};
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::Surface;
use scx_testkit::bodies::made::{cone, cylinder, sphere, torus};
use scx_testkit::bodies::{Kind, generate};

// ---- helpers -------------------------------------------------------------------------------

const ABOVE: [Approach; 2] = [Approach::Above, Approach::Above];

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn frame(o: Point3, z: Vec3, x: Vec3) -> Frame {
    Frame::new(o, z, x).unwrap()
}

/// A tilted frame at `o`.
fn tilted(o: Point3) -> Frame {
    frame(o, v(0.3, -0.5, 0.8), v(1.0, 0.2, 0.1))
}

/// The view along `d`, up the world's z (or x, when `d` is near z).
fn view(d: Vec3) -> View {
    let up = if d.cross(v(0.0, 0.0, 1.0)).length() > 0.1 * d.length() {
        v(0.0, 0.0, 1.0)
    } else {
        v(1.0, 0.0, 0.0)
    };
    View::new(d, up).unwrap()
}

/// Directions in general position, and the axes.
fn directions() -> Vec<Vec3> {
    vec![
        v(1.0, 0.0, 0.0),
        v(0.0, 1.0, 0.0),
        v(0.0, 0.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(1.0, -2.0, 0.5),
        v(-0.3, 0.7, -2.0),
        v(0.05, -0.02, 1.0),
        v(-1.0, 0.4, 0.1),
    ]
}

fn run_on(
    body: &Brep,
    view: &View,
    settings: &Settings,
    stop: &Stop,
) -> Result<Silhouettes, HlrError> {
    let doc = Document::default();
    let mut scratch = Scratch::new();
    let mut cx = Cx::new(&doc, settings, &mut scratch, stop, &NoTrace);
    silhouettes(body, view, &mut cx)
}

fn run(body: &Brep, view: &View) -> Silhouettes {
    run_on(body, view, &Settings::new(), &Stop::new()).unwrap()
}

fn finish(d: Draft) -> Brep {
    d.finish(&Document::default())
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

fn generated(kind: Kind, seed: u64, size: usize) -> Brep {
    finish(generate(kind, seed, size).unwrap().draft)
}

fn proved(s: &Silhouette) -> &[Extent1d] {
    match &s.on_face {
        OnFace::Proved(p) => p,
        other @ OnFace::Undecided { .. } => panic!("face {:?}: not proved: {other:?}", s.face),
    }
}

fn length(pieces: &[Extent1d]) -> f64 {
    pieces.iter().map(Extent1d::length).sum()
}

fn whole_turn(s: &Silhouette) {
    let p = proved(s);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!((p[0].min(), p[0].max()), (0.0, TAU));
}

fn face_of<'a>(body: &'a Brep, s: &Silhouette) -> Face<'a> {
    body.face(s.face).unwrap()
}

/// The surface normal at `q` on `face`'s surface (closed-form inversion), and how far `q`
/// is from the surface.
fn normal_at(face: Face<'_>, q: Point3) -> (Vec3, f64) {
    let inv = face
        .surface()
        .invert(q, None, None, &mut Budget::unlimited(), &Stop::new())
        .unwrap();
    let f = inv.foot();
    (*face.surface().normal(f.at, ABOVE).unwrap(), f.distance)
}

/// Every piece's points lie on the face's surface with `n · d = 0`, at a few parameters
/// inside it (an end may be a cone's apex, where there is no normal).
fn on_surface_and_normal_to_sight(body: &Brep, s: &Silhouette, view: &View, tol: f64) {
    let face = face_of(body, s);
    for piece in proved(s) {
        for i in 0..8 {
            let t = piece.min() + piece.length() * (f64::from(i) + 0.5) / 8.0;
            let q = s.curve.point(t);
            let (n, dist) = normal_at(face, q);
            assert!(dist <= tol, "off the surface by {dist}");
            assert!(n.dot(*view.d()).abs() <= tol, "n.d = {}", n.dot(*view.d()));
        }
    }
}

/// The midpoint of every piece lies in the face's parameter box (wrapped by a period).
fn midpoints_in_box(body: &Brep, s: &Silhouette) {
    let face = face_of(body, s);
    let uv = face.uv_box().unwrap();
    for piece in proved(s) {
        let q = s.curve.point(piece.mid());
        let inv = face
            .surface()
            .invert(q, None, Some(&uv), &mut Budget::unlimited(), &Stop::new())
            .unwrap();
        assert!(
            matches!(inv, Inversion::Inside(_)),
            "{piece:?} of face {:?}",
            s.face
        );
    }
}

fn axis_distance(q: Point3, f: &Frame) -> (f64, f64) {
    let r = q - f.origin();
    let h = r.dot(*f.z());
    ((r - *f.z() * h).length(), h)
}

// ---- views ---------------------------------------------------------------------------------

#[test]
fn a_view_is_refused_without_directions() {
    let e = |d: Vec3, up: Vec3| View::new(d, up).unwrap_err();
    assert_eq!(
        e(v(0.0, 0.0, 0.0), v(0.0, 0.0, 1.0)),
        HlrError::BadView(BadView::Direction)
    );
    assert_eq!(
        e(v(f64::NAN, 0.0, 1.0), v(0.0, 0.0, 1.0)),
        HlrError::BadView(BadView::Direction)
    );
    assert_eq!(
        e(v(1.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
        HlrError::BadView(BadView::Up)
    );
    assert_eq!(
        e(v(1.0, 0.0, 0.0), v(-2.0, 0.0, 0.0)),
        HlrError::BadView(BadView::Parallel)
    );
    assert_eq!(
        e(v(1.0, 0.0, 0.0), v(f64::INFINITY, 0.0, 0.0)).kind(),
        ErrorKind::Refused
    );
}

#[test]
fn a_view_is_a_right_handed_frame_into_the_screen() {
    let w = View::new(v(0.0, 0.0, -2.0), v(0.0, 3.0, 0.0)).unwrap();
    // x = d × up = -z × y = x; y = x × d = x × -z = y.
    assert!((*w.x() - v(1.0, 0.0, 0.0)).length() < 1e-15);
    assert!((*w.y() - v(0.0, 1.0, 0.0)).length() < 1e-15);
    assert!((*w.d() - v(0.0, 0.0, -1.0)).length() < 1e-15);
    let s = w.screen(p(3.0, 4.0, 5.0), p(1.0, 1.0, 1.0));
    assert_eq!((s.x, s.y), (2.0, 3.0));
}

// ---- spheres -------------------------------------------------------------------------------

#[test]
fn a_sphere_outline_is_its_great_circle_normal_to_the_view() {
    let doc = Document::default();
    let c = p(1.0, -2.0, 3.0);
    let body = sphere(&tilted(c), mm(2.5), &doc).unwrap().body;
    for d in directions() {
        let w = view(d);
        let got = run(&body, &w);
        assert!(got.unsupported.is_empty());
        assert_eq!(got.curves.len(), 1);
        let s = &got.curves[0];
        assert_eq!(s.accuracy, Exactness::Exact);
        let SilhouetteCurve::Circle(circle) = &s.curve else {
            panic!("{:?}", s.curve)
        };
        assert!((circle.radius().mm() - 2.5).abs() < 1e-15);
        assert!(circle.frame().origin().distance(c) < 1e-15);
        assert!(circle.frame().z().cross(*w.d()).length() < 1e-15);
        whole_turn(s);
        on_surface_and_normal_to_sight(&body, s, &w, 1e-14);
    }
}

#[test]
fn sphere_lunes_share_the_outline_once() {
    for seed in 1..5 {
        let body = generated(Kind::Sphere, seed, 4);
        for d in directions() {
            let w = view(d);
            let got = run(&body, &w);
            assert!(got.unsupported.is_empty());
            let mut total = 0.0;
            for s in &got.curves {
                total += length(proved(s));
                midpoints_in_box(&body, s);
                on_surface_and_normal_to_sight(&body, s, &w, 1e-12);
            }
            assert!((total - TAU).abs() < 1e-12, "seed {seed} {d:?}: {total}");
        }
    }
}

// ---- cylinders and cones -------------------------------------------------------------------

#[test]
fn a_cylinder_has_two_generators_at_its_radius_across_the_view() {
    let doc = Document::default();
    let f = tilted(p(0.5, 0.25, -1.0));
    let (r, h) = (1.5, 4.0);
    let made = cylinder(&f, mm(r), mm(h), &doc).unwrap();
    for d in directions() {
        let w = view(d);
        let got = run(&made.body, &w);
        let across = f.z().cross(*w.d());
        if across.length() < 1e-9 {
            assert!(got.curves.is_empty());
            continue;
        }
        assert_eq!(got.curves.len(), 2, "{d:?}");
        let side = across.normalized().unwrap();
        let mut signs = Vec::new();
        for s in &got.curves {
            assert_eq!(s.face, made.faces.wall);
            let SilhouetteCurve::Line(line) = &s.curve else {
                panic!("{:?}", s.curve)
            };
            let pieces = proved(s);
            assert_eq!(pieces.len(), 1);
            assert!(pieces[0].min().abs() < 1e-12 && (pieces[0].max() - h).abs() < 1e-12);
            for t in [0.0, 1.0, h] {
                let (rho, z) = axis_distance(line.point(t), &f);
                assert!((rho - r).abs() < 1e-12 && (z - t).abs() < 1e-12);
            }
            let off = (line.origin() - f.origin()).dot(*side) / r;
            assert!((off.abs() - 1.0).abs() < 1e-12);
            signs.push(off.signum());
            on_surface_and_normal_to_sight(&made.body, s, &w, 1e-12);
        }
        assert_ne!(signs[0], signs[1]);
    }
}

#[test]
fn a_cone_has_generators_only_outside_its_half_angle() {
    let doc = Document::default();
    let f = tilted(p(-1.0, 2.0, 0.5));
    let h = 3.0;
    for radii in [[2.0, 1.0], [2.0, 0.0], [0.5, 1.5]] {
        let made = cone(&f, [mm(radii[0]), mm(radii[1])], mm(h), &doc).unwrap();
        let k = ((radii[1] - radii[0]) / h).abs();
        for d in directions() {
            let w = view(d);
            let got = run(&made.body, &w);
            let da = w.d().dot(*f.z());
            let across = (*w.d() - *f.z() * da).length();
            let expect = if (k * da).abs() < across - 1e-9 {
                2
            } else if (k * da).abs() > across + 1e-9 {
                0
            } else {
                continue;
            };
            assert_eq!(got.curves.len(), expect, "{radii:?} {d:?}");
            for s in &got.curves {
                assert_eq!(s.face, made.faces.wall);
                let SilhouetteCurve::Line(line) = &s.curve else {
                    panic!("{:?}", s.curve)
                };
                let pieces = proved(s);
                assert_eq!(pieces.len(), 1, "{pieces:?}");
                // The ends are the rims (or the apex), at axial 0 and h.
                let ends = [line.point(pieces[0].min()), line.point(pieces[0].max())];
                let (r0, z0) = axis_distance(ends[0], &f);
                let (r1, z1) = axis_distance(ends[1], &f);
                let mut got_ends = [(z0, r0), (z1, r1)];
                got_ends.sort_by(|a, b| a.0.total_cmp(&b.0));
                assert!(got_ends[0].0.abs() < 1e-12 && (got_ends[0].1 - radii[0]).abs() < 1e-12);
                assert!(
                    (got_ends[1].0 - h).abs() < 1e-12 && (got_ends[1].1 - radii[1]).abs() < 1e-12,
                    "{radii:?} {d:?} {got_ends:?} {pieces:?}"
                );
                on_surface_and_normal_to_sight(&made.body, s, &w, 1e-12);
            }
        }
    }
}

#[test]
fn cylinder_and_cone_walls_hold_each_generator_once() {
    for kind in [Kind::Cylinder, Kind::Cone] {
        for seed in 1..5 {
            let body = generated(kind, seed, 4);
            for d in directions() {
                let w = view(d);
                let got = run(&body, &w);
                assert!(got.unsupported.is_empty());
                // Generators are told apart by their origins: the faces share one surface.
                let mut by_line: Vec<(Point3, Vec<Extent1d>)> = Vec::new();
                for s in &got.curves {
                    let SilhouetteCurve::Line(line) = &s.curve else {
                        panic!("{:?}", s.curve)
                    };
                    let at = line.origin();
                    midpoints_in_box(&body, s);
                    on_surface_and_normal_to_sight(&body, s, &w, 1e-12);
                    let pieces = proved(s).to_vec();
                    match by_line.iter_mut().find(|(e, _)| e.distance(at) < 1e-9) {
                        Some((_, all)) => all.extend(pieces),
                        None => by_line.push((at, pieces)),
                    }
                }
                for (_, pieces) in &by_line {
                    assert_eq!(pieces.len(), 1, "{kind:?} seed {seed} {d:?}: {pieces:?}");
                }
            }
        }
    }
}

#[test]
fn a_generator_on_a_shared_edge_is_held_by_both_faces() {
    // Seed 0: radius 1, height 2 about the world's z, rulings at 0, 90, 180 and 270
    // degrees; seen along x the generators at +-y are the rulings at 90 and 270 degrees, on
    // the boundary of the faces either side. Each face holds it (the region test says
    // `Near`); telling the copies apart is the visibility stage's work.
    let body = generated(Kind::Cylinder, 0, 4);
    let got = run(&body, &view(v(1.0, 0.0, 0.0)));
    assert_eq!(got.curves.len(), 4);
    for s in &got.curves {
        let SilhouetteCurve::Line(line) = &s.curve else {
            panic!("{:?}", s.curve)
        };
        assert!(line.origin().x.abs() < 1e-15 && (line.origin().y.abs() - 1.0).abs() < 1e-15);
        let p = proved(s);
        assert_eq!(p.len(), 1);
        assert!(
            p[0].min().abs() < 1e-15 && (p[0].max() - 2.0).abs() < 1e-15,
            "{p:?}"
        );
    }
}

// ---- tori ----------------------------------------------------------------------------------

#[test]
fn a_torus_seen_along_its_axis_shows_its_equators() {
    let doc = Document::default();
    // Along the world's z the view's components across the axis are exact zeros, and the
    // equators come as circles; along a tilted axis they are rounding noise, and the general
    // contour gives the same equators (the special forms are taken only on exact zeros).
    let level = frame(p(1.0, 1.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    for (f, circles) in [(level, true), (tilted(p(1.0, 1.0, 1.0)), false)] {
        let body = torus(&f, mm(3.0), mm(1.0), &doc).unwrap().body;
        for d in [*f.z(), -*f.z()] {
            let w = view(d);
            let got = run(&body, &w);
            let mut radii = Vec::new();
            for s in &got.curves {
                whole_turn(s);
                on_surface_and_normal_to_sight(&body, s, &w, 1e-14);
                assert_eq!(matches!(s.curve, SilhouetteCurve::Circle(_)), circles);
                let rho: Vec<(f64, f64)> = (0..16)
                    .map(|i| axis_distance(s.curve.point(TAU * f64::from(i) / 16.0), &f))
                    .collect();
                for &(r, h) in &rho {
                    assert!((r - rho[0].0).abs() < 1e-14 && h.abs() < 1e-14, "{rho:?}");
                }
                radii.push(rho[0].0);
            }
            radii.sort_by(f64::total_cmp);
            assert_eq!(radii.len(), 2);
            assert!((radii[0] - 2.0).abs() < 1e-14 && (radii[1] - 4.0).abs() < 1e-14);
        }
    }
}

#[test]
fn a_torus_seen_across_its_axis_shows_four_circles() {
    let doc = Document::default();
    let f = frame(p(1.0, 1.0, 1.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    let body = torus(&f, mm(3.0), mm(1.0), &doc).unwrap().body;
    let w = view(v(1.0, 2.0, 0.0));
    let got = run(&body, &w);
    assert_eq!(got.curves.len(), 4);
    let star = f.z().cross(*w.d()).normalized().unwrap();
    let mut found = Vec::new();
    for s in &got.curves {
        whole_turn(s);
        on_surface_and_normal_to_sight(&body, s, &w, 1e-14);
        let SilhouetteCurve::Circle(c) = &s.curve else {
            panic!("{:?}", s.curve)
        };
        let rel = c.frame().origin() - f.origin();
        found.push((c.radius().mm(), rel.dot(*f.z()), rel.dot(*star)));
    }
    found.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let want = [
        (1.0, 0.0, -3.0),
        (1.0, 0.0, 3.0),
        (3.0, -1.0, 0.0),
        (3.0, 1.0, 0.0),
    ];
    for (g, e) in found.iter().zip(want) {
        assert!(
            (g.0 - e.0).abs() < 1e-15 && (g.1 - e.1).abs() < 1e-15 && (g.2 - e.2).abs() < 1e-14,
            "{g:?} {e:?}"
        );
    }
}

#[test]
fn a_torus_contour_is_the_spine_ellipse_offset_by_the_minor_radius() {
    let doc = Document::default();
    let f = tilted(p(-2.0, 0.5, 3.0));
    let (big, small) = (3.0, 1.25);
    let body = torus(&f, mm(big), mm(small), &doc).unwrap().body;
    for d in directions() {
        let w = view(d);
        let got = run(&body, &w);
        if got
            .curves
            .iter()
            .any(|s| !matches!(s.curve, SilhouetteCurve::Torus(_)))
        {
            continue;
        }
        assert_eq!(got.curves.len(), 2, "{d:?}");
        let branches: Vec<Branch> = got
            .curves
            .iter()
            .map(|s| match &s.curve {
                SilhouetteCurve::Torus(c) => c.branch(),
                _ => unreachable!(),
            })
            .collect();
        assert!(branches.contains(&Branch::Plus) && branches.contains(&Branch::Minus));
        for s in &got.curves {
            whole_turn(s);
            on_surface_and_normal_to_sight(&body, s, &w, 1e-13);
            let SilhouetteCurve::Torus(c) = &s.curve else {
                unreachable!()
            };
            for i in 0..16 {
                let t = TAU * f64::from(i) / 16.0;
                let (st, ct) = fmath::sin_cos(t);
                let spine = f.origin() + (*f.x() * ct + *f.y() * st) * big;
                let tangent = *f.x() * -st + *f.y() * ct;
                let q = w.screen(c.point(t), f.origin());
                let o = w.screen(spine, f.origin());
                let tan = Vec2::new(tangent.dot(*w.x()), tangent.dot(*w.y()));
                let off = Vec2::new(q.x - o.x, q.y - o.y);
                assert!((fmath::hypot(off.x, off.y) - small).abs() < 1e-13);
                assert!((off.x * tan.x + off.y * tan.y).abs() < 1e-13);
                // The torus's own normal, independently, is the contour's.
                assert!(
                    (c.normal(t) - normal_at(face_of(&body, s), c.point(t)).0).length() < 1e-12
                );
            }
        }
    }
}

#[test]
fn torus_patches_share_each_branch_once() {
    for seed in 1..5 {
        let body = generated(Kind::Torus, seed, 3);
        for d in directions() {
            let w = view(d);
            let got = run(&body, &w);
            assert!(got.unsupported.is_empty());
            let (mut plus, mut minus, mut circles) = (0.0, 0.0, 0usize);
            for s in &got.curves {
                midpoints_in_box(&body, s);
                on_surface_and_normal_to_sight(&body, s, &w, 1e-12);
                match &s.curve {
                    SilhouetteCurve::Torus(c) if c.branch() == Branch::Plus => {
                        plus += length(proved(s));
                    }
                    SilhouetteCurve::Torus(_) => minus += length(proved(s)),
                    _ => circles += 1,
                }
            }
            assert_eq!(circles, 0);
            assert!((plus - TAU).abs() < 1e-12, "seed {seed} {d:?}: {plus}");
            assert!((minus - TAU).abs() < 1e-12, "seed {seed} {d:?}: {minus}");
        }
    }
}

// ---- far from the origin, scenes, determinism ----------------------------------------------

#[test]
fn far_from_the_origin_the_closed_forms_hold() {
    let doc = Document::default();
    let far = p(1e6, -2e6, 3e5);
    let f = tilted(far);
    let s = sphere(&f, mm(2.0), &doc).unwrap().body;
    let c = cylinder(&f, mm(1.0), mm(5.0), &doc).unwrap().body;
    let w = view(v(1.0, -2.0, 0.5));
    let got = run(&s, &w);
    assert_eq!(got.curves.len(), 1);
    whole_turn(&got.curves[0]);
    let got = run(&c, &w);
    assert_eq!(got.curves.len(), 2);
    for s in &got.curves {
        let pieces = proved(s);
        assert_eq!(pieces.len(), 1);
        assert!(pieces[0].min().abs() < 1e-9 && (pieces[0].max() - 5.0).abs() < 1e-9);
        on_surface_and_normal_to_sight(&c, s, &w, 1e-9);
    }
}

#[test]
fn a_placed_body_has_the_silhouettes_of_the_body_made_there() {
    let doc = Document::default();
    let place = tilted(p(10.0, -4.0, 2.0));
    let home = frame(Point3::ORIGIN, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    let local = cylinder(&home, mm(1.0), mm(3.0), &doc).unwrap().body;
    let there = cylinder(&place, mm(1.0), mm(3.0), &doc).unwrap().body;
    let ball = sphere(&home, mm(2.0), &doc).unwrap().body;
    let settings = Settings::new();
    let stop = Stop::new();
    let mut scratch = Scratch::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let scene = prepare(&[(&local, place), (&ball, home)], &mut cx).unwrap();
    // The sphere's box is its own, not its poles'.
    let b = scene.bounds().unwrap();
    assert!(b.min().x <= -2.0 + 1e-6 && b.min().y <= -2.0 + 1e-6);
    for d in directions() {
        let w = view(d);
        let got = scene.silhouettes(&w, &mut cx).unwrap();
        assert_eq!(got.len(), 2);
        let want = run(&there, &w);
        assert_eq!(got[0].curves.len(), want.curves.len(), "{d:?}");
        for g in &got[0].curves {
            let (SilhouetteCurve::Line(gl), pieces) = (&g.curve, proved(g)) else {
                panic!()
            };
            let m = want.curves.iter().find(|e| {
                let SilhouetteCurve::Line(el) = &e.curve else {
                    return false;
                };
                el.origin().distance(gl.origin()) < 1e-12
            });
            let m = m.expect("a matching generator");
            let SilhouetteCurve::Line(el) = &m.curve else {
                unreachable!()
            };
            assert!((el.direction() - gl.direction()).length() < 1e-12);
            let mp = proved(m);
            assert!((mp[0].min() - pieces[0].min()).abs() < 1e-12);
            assert!((mp[0].max() - pieces[0].max()).abs() < 1e-12);
        }
        assert_eq!(got[1].curves.len(), 1);
    }
}

#[test]
fn twice_gives_the_same_bits() {
    let body = generated(Kind::Torus, 1, 3);
    let w = view(v(1.0, -2.0, 0.5));
    let a = format!("{:?}", run(&body, &w));
    let b = format!("{:?}", run(&body, &w));
    assert_eq!(a, b);
}

// ---- stop, budget, trace -------------------------------------------------------------------

#[test]
fn stop_and_budget_end_the_work_by_name() {
    let body = generated(Kind::Sphere, 0, 3);
    let w = view(v(1.0, 1.0, 1.0));
    let stop = Stop::new();
    stop.set();
    let e = run_on(&body, &w, &Settings::new(), &stop).unwrap_err();
    assert_eq!((e.kind(), e.name()), (ErrorKind::Cancelled, "stopped"));
    for limit in [0, 1, 2] {
        let e = run_on(&body, &w, &Settings::new().with_budget(limit), &Stop::new()).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Exhausted, "{limit}");
    }
}

// ---- NURBS faces ---------------------------------------------------------------------------

/// A sheet being written: the outside shell on both sides of every face.
struct B {
    d: Draft,
    outside: ShellId,
}

impl B {
    fn new() -> Self {
        let mut d = Draft::new();
        let outside = d.add_shell(ShellRecord::new(RegionId::OUTSIDE)).unwrap();
        Self { d, outside }
    }

    fn vertex(&mut self, q: [f64; 3]) -> VertexId {
        self.d
            .add_vertex(VertexRecord::new(p(q[0], q[1], q[2])))
            .unwrap()
    }

    fn edge(&mut self, curve: Curve, lo: f64, hi: f64, ends: [VertexId; 2]) -> EdgeId {
        let interval = Extent1d::new(lo, hi).unwrap();
        self.d
            .add_edge(EdgeGeom::new(EdgeShape::Curve { curve, interval }, ends))
            .unwrap()
    }

    fn line(&mut self, a: VertexId, b: VertexId) -> EdgeId {
        let q0 = self.d.vertex(a).unwrap().point();
        let q1 = self.d.vertex(b).unwrap().point();
        self.edge(
            Curve::Line(Line::new(q0, q1 - q0).unwrap()),
            0.0,
            1.0,
            [a, b],
        )
    }

    fn face(&mut self, surface: Surface, uses: Vec<(EdgeId, Sense, Curve2)>) {
        let uses = uses
            .into_iter()
            .map(|(e, s, c)| {
                EdgeUse::new(
                    e,
                    s,
                    UseGeom::Pcurve {
                        curve: c,
                        interval: Extent1d::new(0.0, 1.0).unwrap(),
                    },
                )
            })
            .collect();
        self.d
            .add_face(FaceRecord::new(
                surface,
                [self.outside, self.outside],
                vec![LoopRecord::new(uses)],
            ))
            .unwrap();
    }
}

fn line2(origin: [f64; 2], dir: [f64; 2]) -> Curve2 {
    Curve2::Line(Line2::new(Point2::new(origin[0], origin[1]), Vec2::new(dir[0], dir[1])).unwrap())
}

/// Three quarter spans of a rational quadratic, double knots at 1/3 and 2/3.
fn arc_knots() -> Vec<f64> {
    let (a, b) = (1.0 / 3.0, 2.0 / 3.0);
    vec![0.0, 0.0, 0.0, a, a, b, b, 1.0, 1.0, 1.0]
}

/// The arc's control points `(x, y)` and weights, angle 0 to 270 degrees.
fn arc_net() -> [(f64, f64, f64); 7] {
    let s = FRAC_1_SQRT_2;
    [
        (1.0, 0.0, 1.0),
        (1.0, 1.0, s),
        (0.0, 1.0, 1.0),
        (-1.0, 1.0, s),
        (-1.0, 0.0, 1.0),
        (-1.0, -1.0, s),
        (0.0, -1.0, 1.0),
    ]
}

fn arc_at(z: f64, rational: bool) -> Curve {
    let pts: Vec<[f64; 3]> = arc_net().iter().map(|&(x, y, _)| [x, y, z]).collect();
    let w = rational.then(|| arc_net().iter().map(|&(_, _, w)| w).collect());
    Curve::Nurbs(Arc::new(
        NurbsCurve::<3>::new(KnotVector::new(2, arc_knots()).unwrap(), pts, w).unwrap(),
    ))
}

/// The 270 degree cylinder of radius 1 about z over `z` in `[0, 1]` (rational), or the
/// polynomial surface on the same net (not a cylinder).
fn net_surface(rational: bool) -> Surface {
    let pts: Vec<[f64; 3]> = arc_net()
        .iter()
        .flat_map(|&(x, y, _)| [[x, y, 0.0], [x, y, 1.0]])
        .collect();
    let w: Vec<f64> = arc_net().iter().flat_map(|&(_, _, w)| [w, w]).collect();
    Surface::Nurbs(Arc::new(
        NurbsSurface::<3>::new(
            KnotVector::new(2, arc_knots()).unwrap(),
            KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap(),
            Net::new(7, 2, pts).unwrap(),
            rational.then(|| Net::new(7, 2, w).unwrap()),
        )
        .unwrap(),
    ))
}

/// The net surface trimmed to `0 <= z <= top`: one face of four iso-parametric edges.
fn trimmed(top: f64, rational: bool) -> (Brep, EdgeId) {
    let mut b = B::new();
    let v00 = b.vertex([1.0, 0.0, 0.0]);
    let v10 = b.vertex([0.0, -1.0, 0.0]);
    let v11 = b.vertex([0.0, -1.0, top]);
    let v01 = b.vertex([1.0, 0.0, top]);
    let bottom = b.edge(arc_at(0.0, rational), 0.0, 1.0, [v00, v10]);
    let right = b.line(v10, v11);
    let upper = b.edge(arc_at(top, rational), 0.0, 1.0, [v01, v11]);
    let left = b.line(v00, v01);
    b.face(
        net_surface(rational),
        vec![
            (bottom, Sense::Forward, line2([0.0, 0.0], [1.0, 0.0])),
            (right, Sense::Forward, line2([1.0, 0.0], [0.0, top])),
            (upper, Sense::Reversed, line2([0.0, top], [1.0, 0.0])),
            (left, Sense::Reversed, line2([0.0, 0.0], [0.0, top])),
        ],
    );
    (finish(b.d), bottom)
}

/// The rational 270 degree cylinder over `z` in `[0, 0.5]` is recognised, and its NURBS
/// rims now split the generators: of the two generators across the view (at about 106.7
/// and 286.7 degrees), the first runs on the face over the whole height and the second
/// lies in the 90 degree gap and is no silhouette of the face.
#[test]
fn a_rational_cylinder_is_recognised_and_trimmed_by_its_nurbs_rims() {
    let (body, _) = trimmed(0.5, true);
    let w = view(v(1.0, 0.3, 0.2));
    let got = run(&body, &w);
    assert!(got.unsupported.is_empty());
    assert_eq!(got.curves.len(), 1, "{:?}", got.curves);
    let axis = frame(Point3::ORIGIN, v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0));
    let s = &got.curves[0];
    let Exactness::Recognized { deviation } = s.accuracy else {
        panic!("{:?}", s.accuracy)
    };
    assert!(deviation.mm() <= 1e-9, "{deviation:?}");
    let SilhouetteCurve::Line(line) = &s.curve else {
        panic!("{:?}", s.curve)
    };
    let (rho, _) = axis_distance(line.point(0.25), &axis);
    assert!((rho - 1.0).abs() < 1e-9);
    // The generator is across the view from the axis, inside the arc's 270 degrees.
    let rel = line.origin() - Point3::ORIGIN;
    let radial = rel - v(0.0, 0.0, rel.z);
    assert!(radial.dot(*w.d()).abs() < 1e-9);
    assert!(radial.x < 0.0 || radial.y > 0.0, "{radial:?}");
    // The piece runs from the bottom rim to the top one.
    let p = proved(s);
    assert_eq!(p.len(), 1, "{p:?}");
    let (z0, z1) = (line.point(p[0].min()).z, line.point(p[0].max()).z);
    assert!(z0.abs() < 1e-9 && (z1 - 0.5).abs() < 1e-9, "{z0} {z1}");
}

#[test]
fn a_freeform_face_is_named_unsupported() {
    let (body, _) = trimmed(0.5, false);
    let got = run(&body, &view(v(1.0, 0.3, 0.2)));
    assert!(got.curves.is_empty());
    assert_eq!(got.unsupported.len(), 1);
    assert_eq!(got.unsupported[0].why, Why::Freeform);
}
