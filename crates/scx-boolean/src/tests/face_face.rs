// Written for solidcorex. Apache-2.0.
//! Stage 4 on blocks, cylinders and spheres: every answer worked by hand from the geometry.

use core::f64::consts::{PI, TAU};

use scx_brep::Brep;
use scx_brep::Document;
use scx_brep::op::Settings;
use scx_make::{cylinder, sphere};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::Vec3;
use scx_ssi::analytic::Contact;
use scx_surface::SurfaceKind;

use super::{call, cuboid, pt, run, unit};
use crate::boolean::{Interference, interfere};
use crate::faceface::{FaceFace, Section, face_face};
use crate::geom::{Operands, Side};
use crate::{BooleanError, BooleanUnsupported, Stage};

fn ff(a: &Brep, b: &Brep) -> (Interference, FaceFace) {
    let i = run(a, b).unwrap();
    let f = face_face(
        Operands { a, b },
        &i,
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap();
    (i, f)
}

fn r(x: f64) -> f64 {
    let y = (x * 1e6).round() / 1e6;
    if y == 0.0 { 0.0 } else { y }
}

/// A section's length, by a fine polyline (the curves here are lines and circles).
fn length(s: &Section) -> f64 {
    let n = 4096;
    let at = |k: usize| {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let f = k as f64 / n as f64;
        s.curve.point(s.t.min() + f * s.t.length())
    };
    (0..n).map(|k| at(k).distance(at(k + 1))).sum()
}

/// A section as the tests compare it: length, then its end points (sorted), then contact.
#[derive(Debug, PartialEq, PartialOrd)]
struct Cut {
    len: f64,
    ends: Option<[[f64; 3]; 2]>,
    tangent: bool,
}

fn cuts(i: &Interference, f: &FaceFace) -> Vec<Cut> {
    let p = |k: usize| {
        let q = i.points[k].at;
        [r(q.x), r(q.y), r(q.z)]
    };
    // The ends are merged points, and the curve runs between them.
    for s in &f.sections {
        if let Some([e0, e1]) = s.ends {
            assert!(s.curve.point(s.t.min()).distance(i.points[e0].at) < 1e-9);
            assert!(s.curve.point(s.t.max()).distance(i.points[e1].at) < 1e-9);
        }
    }
    let mut out: Vec<Cut> = f
        .sections
        .iter()
        .map(|s| Cut {
            len: r(length(s)),
            ends: s.ends.map(|[a, b]| {
                let mut e = [p(a), p(b)];
                e.sort_by(|x, y| x.partial_cmp(y).unwrap());
                e
            }),
            tangent: s.contact == Contact::Tangent,
        })
        .collect();
    out.sort_by(|x, y| x.partial_cmp(y).unwrap());
    out
}

fn cut(len: f64, ends: [[f64; 3]; 2]) -> Cut {
    let mut ends = ends.map(|e| e.map(r));
    ends.sort_by(|x, y| x.partial_cmp(y).unwrap());
    Cut {
        len: r(len),
        ends: Some(ends),
        tangent: false,
    }
}

fn sorted(mut v: Vec<Cut>) -> Vec<Cut> {
    v.sort_by(|x, y| x.partial_cmp(y).unwrap());
    v
}

/// The lying pieces as (side, length, along a boundary edge), sorted.
fn lying(a: &Brep, b: &Brep, f: &FaceFace) -> Vec<(Side, f64, bool)> {
    let mut v: Vec<(Side, f64, bool)> = f
        .lying
        .iter()
        .map(|l| {
            let body = if l.side == Side::A { a } else { b };
            let ed = body.edge(l.edge).unwrap();
            let scx_brep::EdgeShape::Curve { curve, .. } = ed.shape() else {
                panic!("a lying edge has a curve");
            };
            let len = curve.point(l.t.min()).distance(curve.point(l.t.max()));
            (l.side, r(len), l.along.is_some())
        })
        .collect();
    v.sort_by(|x, y| x.partial_cmp(y).unwrap());
    v
}

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(
        pt(o),
        Vec3::new(z[0], z[1], z[2]),
        Vec3::new(x[0], x[1], x[2]),
    )
    .unwrap()
}

pub(super) const Z: [f64; 3] = [0.0, 0.0, 1.0];
pub(super) const X: [f64; 3] = [1.0, 0.0, 0.0];

pub(super) fn cyl(
    o: [f64; 3],
    z: [f64; 3],
    x: [f64; 3],
    radius: f64,
    h: f64,
    doc: Document,
) -> Brep {
    cylinder(
        &frame(o, z, x),
        Length::from_mm(radius),
        Length::from_mm(h),
        &doc,
    )
    .unwrap()
    .body
}

pub(super) fn ball(o: [f64; 3], radius: f64, doc: Document) -> Brep {
    sphere(&frame(o, Z, X), Length::from_mm(radius), &doc)
        .unwrap()
        .body
}

#[test]
fn overlapping_cubes_meet_in_six_half_unit_lines() {
    // b = a + (0.5, 0.5, 0.5). Each face of a's corner (1, 1, 1) meets the two faces of
    // b's corner (0.5, 0.5, 0.5) it is not parallel to in a line from the edge-face
    // point where b's edge enters it to the one where a's edge leaves b's face: 0.5 long.
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([0.5; 3], [1.5; 3], doc));
    let (i, f) = ff(&a, &b);
    let want = sorted(vec![
        cut(0.5, [[1.0, 0.5, 0.5], [1.0, 0.5, 1.0]]),
        cut(0.5, [[1.0, 0.5, 0.5], [1.0, 1.0, 0.5]]),
        cut(0.5, [[0.5, 1.0, 0.5], [1.0, 1.0, 0.5]]),
        cut(0.5, [[0.5, 1.0, 0.5], [0.5, 1.0, 1.0]]),
        cut(0.5, [[0.5, 0.5, 1.0], [1.0, 0.5, 1.0]]),
        cut(0.5, [[0.5, 0.5, 1.0], [0.5, 1.0, 1.0]]),
    ]);
    assert_eq!(cuts(&i, &f), want);
    assert!(f.coincident.is_empty());
    assert!(f.lying.is_empty());
    assert!(f.touches.is_empty());
}

#[test]
fn a_block_through_a_side_meets_in_three_lines_and_shares_its_top() {
    // b = [0.25, 0.75] x [-0.5, 0.5] x [0.5, 1] enters a's front face y = 0 through a
    // rectangle's three sides: x = 0.25 and x = 0.75 for z in [0.5, 1], and z = 0.5 for x
    // in [0.25, 0.75]. Its top lies in a's top, both facing +z; the top's other crossings
    // run along an edge of one of the faces, so they are lying pieces, not sections:
    // a's front top edge on b's top over x in [0.25, 0.75], b's x = 0.25 and x = 0.75 top
    // edges on a's top over y in [0, 0.5], and b's back top edge whole (0.5 long).
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([0.25, -0.5, 0.5], [0.75, 0.5, 1.0], doc));
    let (i, f) = ff(&a, &b);
    let want = sorted(vec![
        cut(0.5, [[0.25, 0.0, 0.5], [0.25, 0.0, 1.0]]),
        cut(0.5, [[0.75, 0.0, 0.5], [0.75, 0.0, 1.0]]),
        cut(0.5, [[0.25, 0.0, 0.5], [0.75, 0.0, 0.5]]),
    ]);
    assert_eq!(cuts(&i, &f), want);
    assert_eq!(f.coincident.len(), 1);
    assert!(f.coincident[0].same);
    assert_eq!(
        lying(&a, &b, &f),
        vec![
            (Side::A, 0.5, false),
            (Side::B, 0.5, false),
            (Side::B, 0.5, false),
            (Side::B, 0.5, false),
        ]
    );
}

#[test]
fn blocks_sharing_a_face_coincide_there_facing_apart() {
    // b = a + (1, 0, 0): a's x = 1 face is b's x = 1 face, the normals opposite. The other
    // pairs meet only along the shared face's edges: no sections, and no coincidence of
    // width between the tops (or bottoms, fronts, backs), which only touch along a line.
    let doc = Document::default();
    let (a, b) = (unit(doc), cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], doc));
    let (_, f) = ff(&a, &b);
    assert!(f.sections.is_empty());
    assert_eq!(f.coincident.len(), 1);
    assert!(!f.coincident[0].same);
    // Every lying edge runs along an edge of the face it lies on: the shared square's
    // four edges, each of both operands, each 1 long.
    let l = lying(&a, &b, &f);
    assert!(!l.is_empty());
    // Each edge of the square lies on two faces of the other: the shared one and the one
    // beyond its edge. 4 edges, 2 faces, 2 operands.
    assert_eq!(l.len(), 16);
    assert!(
        l.iter().all(|&(_, len, along)| along && len == 1.0),
        "{l:?}"
    );
    // The other pairs touch only at the square's corners, on both faces' boundaries.
    assert!(f.touches.is_empty(), "{:?}", f.touches);
}

#[test]
fn a_cylinder_through_a_block_meets_its_top_and_bottom_in_whole_circles() {
    // The unit cylinder from z = -1 to 2 through [-2, 2]^2 x [0, 1]: the wall meets the
    // top and bottom in unit circles inside both faces. Each is cut once, where the wall's
    // seam ruling (at +x) crosses the face, so each is one turn from that point to itself.
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let (i, f) = ff(&a, &b);
    let want = sorted(vec![
        cut(TAU, [[1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
        cut(TAU, [[1.0, 0.0, 1.0], [1.0, 0.0, 1.0]]),
    ]);
    assert_eq!(cuts(&i, &f), want);
    assert!(f.coincident.is_empty());
}

#[test]
fn a_cylinder_on_a_blocks_side_meets_it_in_half_circles_and_two_rulings() {
    // The unit cylinder about x = 2, y = 0 from z = -1 to 2, against [-2, 2]^2 x [0, 1]:
    // the top and bottom keep the half of each circle with x < 2 (pi long), cut where the
    // block's x = 2 edges cross the wall at y = +-1; the side x = 2 holds the axis, so it
    // meets the wall in the rulings y = +-1 over the face's z in [0, 1].
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([2.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let (i, f) = ff(&a, &b);
    let want = sorted(vec![
        cut(PI, [[2.0, -1.0, 0.0], [2.0, 1.0, 0.0]]),
        cut(PI, [[2.0, -1.0, 1.0], [2.0, 1.0, 1.0]]),
        cut(1.0, [[2.0, -1.0, 0.0], [2.0, -1.0, 1.0]]),
        cut(1.0, [[2.0, 1.0, 0.0], [2.0, 1.0, 1.0]]),
    ]);
    assert_eq!(cuts(&i, &f), want);
}

#[test]
fn a_plane_through_a_spheres_centre_meets_it_in_two_half_great_circles() {
    // The unit sphere against [0, 2] x [-2, 2]^2: the face x = 0 holds the centre and the
    // poles (0, 0, +-1), which are the sphere's vertices on that face, so the great circle
    // is cut there into two halves, pi each.
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([0.0, -2.0, -2.0], [2.0, 2.0, 2.0], doc);
    let (i, f) = ff(&a, &b);
    let want = sorted(vec![
        cut(PI, [[0.0, 0.0, -1.0], [0.0, 0.0, 1.0]]),
        cut(PI, [[0.0, 0.0, -1.0], [0.0, 0.0, 1.0]]),
    ]);
    assert_eq!(cuts(&i, &f), want);
}

#[test]
fn two_spheres_meet_in_a_circle_cut_at_a_seam() {
    // Unit spheres about the origin and (1, 0, 0) meet in the circle x = 1/2 of radius
    // sqrt(3)/2. a's seam meridian (y = 0, x > 0) crosses it at (1/2, 0, +-sqrt(3)/2);
    // b's (y = 0, x > 1) and both pairs of poles miss it. So two arcs between those
    // points, each half the circle: sqrt(3)/2 pi long.
    let doc = Document::default();
    let (a, b) = (ball([0.0; 3], 1.0, doc), ball([1.0, 0.0, 0.0], 1.0, doc));
    let (i, f) = ff(&a, &b);
    let h = 3.0_f64.sqrt() / 2.0;
    let want = sorted(vec![
        cut(h * PI, [[0.5, 0.0, -h], [0.5, 0.0, h]]),
        cut(h * PI, [[0.5, 0.0, -h], [0.5, 0.0, h]]),
    ]);
    assert_eq!(cuts(&i, &f), want);
}

#[test]
fn a_plane_tangent_to_a_cylinder_meets_it_in_one_tangent_ruling() {
    // The unit cylinder from z = 0 to 1, seam turned to -x, against [1, 2] x [-1, 1] x
    // [0, 1]: the face x = 1 touches the wall along the ruling x = 1, y = 0, from rim to
    // rim. The caps lie in the block's bottom and top but share only that line with them,
    // and the rims touch the block's bottom and top only along the cylinder's own edges.
    let doc = Document::default();
    let a = cyl([0.0; 3], Z, [-1.0, 0.0, 0.0], 1.0, 1.0, doc);
    let b = cuboid([1.0, -1.0, 0.0], [2.0, 1.0, 1.0], doc);
    let (i, f) = ff(&a, &b);
    let mut want = cut(1.0, [[1.0, 0.0, 0.0], [1.0, 0.0, 1.0]]);
    want.tangent = true;
    assert_eq!(cuts(&i, &f), vec![want]);
    assert!(f.coincident.is_empty(), "{:?}", f.coincident);
    // The wall meets the block's top and bottom only where the rims touch its x = 1 edges.
    assert!(f.touches.is_empty(), "{:?}", f.touches);
}

#[test]
fn crossed_cylinders_have_no_closed_form_here() {
    // Perpendicular cylinders of unlike radii meet in a quartic: no closed form.
    let doc = Document::default();
    let a = cyl([0.0; 3], Z, X, 1.0, 1.0, doc);
    let b = cyl([-2.0, 0.0, 0.5], X, Z, 0.5, 4.0, doc);
    let i = run(&a, &b).unwrap();
    let e = face_face(
        Operands { a: &a, b: &b },
        &i,
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap_err();
    let BooleanError::Unsupported(BooleanUnsupported::SsiPair { kinds, .. }) = e else {
        panic!("{e:?}");
    };
    assert_eq!(kinds, [SurfaceKind::Cylinder, SurfaceKind::Cylinder]);
    assert_eq!(e.name(), "ssi_pair");
}

#[test]
fn the_public_call_stops_at_classification() {
    let doc = Document::default();
    let a = cuboid([-2.0, -2.0, 0.0], [2.0, 2.0, 1.0], doc);
    let b = cyl([0.0, 0.0, -1.0], Z, X, 1.0, 3.0, doc);
    let e = call(&a, &b, doc, &Settings::new());
    assert_eq!(
        e,
        BooleanError::Unsupported(BooleanUnsupported::Stage(Stage::Classify))
    );
    assert_eq!(e.name(), "stage_classify");
}

#[test]
fn face_face_is_deterministic_and_mirrors_under_a_swap() {
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([0.0, -2.0, -2.0], [2.0, 2.0, 2.0], doc);
    let once = |a: &Brep, b: &Brep| {
        let mut budget = Budget::unlimited();
        let ops = Operands { a, b };
        let i = interfere(ops, &mut budget, &Stop::new()).unwrap();
        let f = face_face(ops, &i, &mut budget, &Stop::new()).unwrap();
        (i, f, budget.used())
    };
    let (i1, f1, w1) = once(&a, &b);
    let (i2, f2, w2) = once(&a, &b);
    assert_eq!((&i1, &f1, w1), (&i2, &f2, w2));
    // Swapped, the same curves (the cuts compare positions, not ids or sides).
    let (j, g, _) = once(&b, &a);
    assert_eq!(cuts(&i1, &f1), cuts(&j, &g));
    for (s, t) in f1.sections.iter().zip(&g.sections) {
        assert_eq!(s.faces, [t.faces[1], t.faces[0]]);
    }
}

#[test]
fn a_plane_tangent_to_a_sphere_touches_it_at_one_point() {
    // The unit sphere against [-2, 2] x [1, 2] x [-2, 2]: the face y = 1 touches it at
    // (0, 1, 0), inside both faces and away from every edge, so no merged point is there.
    let doc = Document::default();
    let a = ball([0.0; 3], 1.0, doc);
    let b = cuboid([-2.0, 1.0, -2.0], [2.0, 2.0, 2.0], doc);
    let (_, f) = ff(&a, &b);
    assert!(f.sections.is_empty(), "{:?}", f.sections);
    assert_eq!(f.touches.len(), 1);
    let t = f.touches[0];
    assert!(t.at.distance(pt([0.0, 1.0, 0.0])) < 1e-9);
    assert_eq!(t.point, None);
}
