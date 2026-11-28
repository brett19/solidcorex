// Written for solidcorex. Apache-2.0.
//! Cases from the pin's `hlr/tests.rs` that have a closed-form truth on the testkit's bodies
//! and that HL1 and HL2 did not already carry (research 74 §43, HL3). Each is judged against
//! the truth in its comment, not against what the pin printed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::many_single_char_names
)]

use std::f64::consts::TAU;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, EdgeShape};
use scx_hlr::{Branch, SilhouetteCurve, Source, View, Visibility, prepare, silhouettes};
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_testkit::bodies::made::{block, cone, torus};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn view(d: Vec3) -> View {
    let up = if d.cross(v(0.0, 0.0, 1.0)).length() > 0.1 * d.length() {
        v(0.0, 0.0, 1.0)
    } else {
        v(1.0, 0.0, 0.0)
    };
    View::new(d, up).unwrap()
}

// ---- torus tubes in front of an edge -----------------------------------------------------------

/// The pin's four thin-tube ray cases (`a_thin_torus_tube_stops_a_ray`, `a_thin_tube_on_a_huge_
/// torus_stops_a_ray`, `a_hair_thin_tube_on_a_kilometre_torus_stops_a_ray`, `an_edge_under_a_
/// torus_is_hidden_where_the_tube_covers_it`), one closed form: seen from above, a point `(x, y)`
/// of a thing below the torus (`z = 0`, tube radius `r`, spine radius `R`) is covered iff
/// `|hypot(x, y) - R| <= r`. A box lies below with edges along `x` at `y = +-1`, so each edge
/// crosses the ring twice.
#[test]
fn a_tube_hides_exactly_the_points_within_its_radius_of_the_spine_seen_from_above() {
    let doc = Document::default();
    for (big, small) in [(10.0, 1.0), (1.0e3, 1.0), (1.0e3, 0.01), (1.0e6, 0.01)] {
        let t = torus(&Frame::WORLD, mm(big), mm(small), &doc).unwrap().body;
        let half = big + 5.0;
        let slab = block(&Frame::WORLD, [mm(2.0 * half), mm(2.0), mm(1.0)], &doc)
            .unwrap()
            .body;
        let at = Frame::new(p(-half, -1.0, -6.0), v(0.0, 0.0, 1.0), v(1.0, 0.0, 0.0)).unwrap();
        let bodies = [(&t, Frame::WORLD), (&slab, at)];
        let settings = Settings::new();
        let mut scratch = Scratch::new();
        let stop = Stop::new();
        let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        let scene = prepare(&bodies, &mut cx).unwrap();
        let drawing = scene
            .hidden_lines(&view(v(0.0, 0.0, -1.0)), &mut cx)
            .unwrap();
        let (mut hidden, mut checked) = (0usize, 0usize);
        for l in drawing.lines.iter().filter(|l| l.body == 1) {
            let Source::Edge(e) = &l.source else { continue };
            let EdgeShape::Curve { curve, .. } = slab.edge(*e).unwrap().shape() else {
                continue;
            };
            for s in &l.spans {
                for k in 0..9 {
                    let tt = s.t.min() + s.t.length() * (f64::from(k) + 0.5) / 9.0;
                    let q = curve.point(tt);
                    // Only the top face's edges are in the torus's view; the bottom ones
                    // are the slab's own to hide.
                    if (q.z + at.origin().z + 5.0).abs() > 1e-9 {
                        continue;
                    }
                    let (x, y) = (q.x + at.origin().x, q.y + at.origin().y);
                    let off = x.hypot(y) - big;
                    // A point on the tube's very boundary is the closed convention's.
                    if (off.abs() - small).abs() < 1e-7 * (1.0 + small) {
                        continue;
                    }
                    let covered = off.abs() <= small;
                    let says = matches!(s.visibility, Visibility::Hidden { .. });
                    assert_eq!(says, covered, "R={big} r={small} at ({x}, {y}): {s:?}");
                    checked += 1;
                    hidden += usize::from(covered);
                }
            }
        }
        assert!(
            checked > 100 && hidden > 0,
            "R={big} r={small}: {checked} {hidden}"
        );
    }
}

// ---- torus silhouettes -----------------------------------------------------------------------------

fn contour_points(body: &Brep, w: &View) -> Vec<Point3> {
    let doc = Document::default();
    let settings = Settings::new();
    let mut scratch = Scratch::new();
    let stop = Stop::new();
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let got = silhouettes(body, w, &mut cx).unwrap();
    let mut pts = Vec::new();
    for s in &got.curves {
        for i in 0..4096 {
            pts.push(s.curve.point(TAU * f64::from(i) / 4096.0));
        }
    }
    pts
}

/// `a_torus_seen_obliquely_spans_its_outer_diameter`: the width of a torus (spine radius `R`,
/// tube `r`) along a screen direction `u` is the support function `R |u_axial-plane| + r` on each
/// side of its centre, and the contour is where the extremes lie.
#[test]
fn an_obliquely_seen_torus_spans_the_support_function_of_its_spine_circle_plus_the_tube() {
    let doc = Document::default();
    let (big, small) = (5.0, 1.25);
    let t = torus(&Frame::WORLD, mm(big), mm(small), &doc).unwrap().body;
    for d in [
        v(1.0, 2.0, 0.7),
        v(0.3, -0.4, 1.0),
        v(1.0, 0.0, 0.05),
        v(-2.0, 1.0, 3.0),
    ] {
        let w = view(d);
        let pts = contour_points(&t, &w);
        for u in [*w.x(), *w.y()] {
            let want = big * u.x.hypot(u.y) + small;
            let (hi, lo) = pts.iter().fold((f64::MIN, f64::MAX), |(hi, lo), q| {
                let s = q.x * u.x + q.y * u.y + q.z * u.z;
                (hi.max(s), lo.min(s))
            });
            assert!(
                (hi - want).abs() < 1e-5 && (lo + want).abs() < 1e-5,
                "{d:?}: {hi} {lo} {want}"
            );
        }
    }
}

/// `a_torus_seen_nearly_along_its_axis_keeps_its_silhouettes`: tilting the view off the axis by
/// `e` keeps two contour branches whose points lie on the surface with `n . d = 0`, down to a
/// tilt of rounding size, where the equators' circles (exact zeros) take over.
#[test]
fn a_torus_seen_a_hair_off_its_axis_keeps_two_silhouettes_with_n_dot_d_zero() {
    let doc = Document::default();
    let t = torus(&Frame::WORLD, mm(5.0), mm(1.0), &doc).unwrap().body;
    let face = t.faces().next().unwrap();
    for e in [1e-3, 1e-6, 1e-9, 1e-12, 1e-15, 0.0] {
        let w = view(v(e, 0.0, -1.0));
        let doc = Document::default();
        let settings = Settings::new();
        let mut scratch = Scratch::new();
        let stop = Stop::new();
        let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        let got = silhouettes(&t, &w, &mut cx).unwrap();
        // Two branches (or the two equators): two whole-turn curves per torus face, merged.
        let branches = got
            .curves
            .iter()
            .filter(|s| matches!(&s.curve, SilhouetteCurve::Torus(c) if c.branch() == Branch::Plus))
            .count();
        assert!(branches <= 2 && !got.curves.is_empty(), "{e}");
        for s in &got.curves {
            let f = t.face(s.face).unwrap();
            for i in 0..16 {
                let q = s.curve.point(TAU * f64::from(i) / 16.0);
                let inv = f
                    .surface()
                    .invert(q, None, None, &mut Budget::unlimited(), &Stop::new())
                    .unwrap();
                let n = *f
                    .surface()
                    .normal(inv.foot().at, [Approach::Above; 2])
                    .unwrap();
                assert!(inv.foot().distance < 1e-12, "{e}: off the surface");
                assert!(
                    n.dot(*w.d()).abs() < 1e-9 + e,
                    "{e}: n.d = {}",
                    n.dot(*w.d())
                );
            }
        }
        let _ = face;
    }
}

// ---- cones far out and turned -----------------------------------------------------------------------

/// `nearly_coaxial_cones_far_out_keep_their_own_silhouettes` and `cones_with_slightly_turned_axes_
/// keep_their_own_outlines`: each cone's generators lie on its own surface with `n . d = 0` even
/// when two cones far from the origin differ in axis by 1e-9.
#[test]
fn cones_far_out_with_nearly_equal_axes_each_keep_their_own_generators() {
    let doc = Document::default();
    let far = p(1.0e5, -2.0e5, 3.0e5);
    for tilt in [0.0, 1e-9, 1e-6, 1e-3] {
        let axis = v(tilt, 0.0, 1.0);
        let f = Frame::new(far, axis, v(1.0, 0.0, -tilt)).unwrap();
        let body = cone(&f, [mm(2.0), mm(0.5)], mm(3.0), &doc).unwrap().body;
        let w = view(v(1.0, 0.3, -0.2));
        let settings = Settings::new();
        let mut scratch = Scratch::new();
        let stop = Stop::new();
        let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
        let got = silhouettes(&body, &w, &mut cx).unwrap();
        assert_eq!(got.curves.len(), 2, "{tilt}");
        for s in &got.curves {
            let face = body.face(s.face).unwrap();
            let scx_hlr::OnFace::Proved(pieces) = &s.on_face else {
                panic!("{s:?}")
            };
            for piece in pieces {
                for i in 0..8 {
                    let t = piece.min() + piece.length() * (f64::from(i) + 0.5) / 8.0;
                    let q = s.curve.point(t);
                    let inv = face
                        .surface()
                        .invert(q, None, None, &mut Budget::unlimited(), &Stop::new())
                        .unwrap();
                    let n = *face
                        .surface()
                        .normal(inv.foot().at, [Approach::Above; 2])
                        .unwrap();
                    assert!(
                        inv.foot().distance < 1e-9,
                        "{tilt}: {}",
                        inv.foot().distance
                    );
                    assert!(n.dot(*w.d()).abs() < 1e-9, "{tilt}: {}", n.dot(*w.d()));
                }
            }
        }
    }
}
