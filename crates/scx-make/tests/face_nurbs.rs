// Written for solidcorex. Apache-2.0.
//! `face` on a NURBS surface (research 75 MK15b): a sheet of one face over a uv box or a
//! region of iso-lines, judged against truth.
//!
//! - every edge is the surface's iso-curve: its points are the surface's own at the
//!   boundary's parameters (bit for bit where the surface is polynomial and the edge ends
//!   at a knot, within 1e-12 elsewhere and where it is rational);
//! - every pcurve is a line in uv that lifts onto its edge;
//! - the areas are closed forms: a planar bilinear patch (the quadrilateral of its four
//!   corners, by the cross product of its diagonals) and a NURBS cylinder's quarter, half
//!   and L (`pi r h` times the fraction of the turn);
//! - the sheet passes `scx_check::check` as class `Sheet`;
//! - bounds that are not iso-lines, a boundary that meets itself and parameters past the
//!   domain are typed refusals.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use core::f64::consts::{FRAC_1_SQRT_2, PI};
use std::sync::Arc;

use scx_brep::op::{Cx, NoTrace, Scratch, Settings};
use scx_brep::{Brep, Document, EdgeShape, FaceSide, UseGeom};
use scx_check::{Class, Scope};
use scx_curve::Curve;
use scx_make::{Bound, MakeError, Refusal, Unsupported, face};
use scx_mass::{MassSpec, RelAccuracy, mass};
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_nurbs::KnotVector;
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_planar::{Region, Segment};
use scx_surface::Surface;

fn doc() -> Document {
    Document::default()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

fn bx(u0: f64, u1: f64, v0: f64, v1: f64) -> Bound {
    Bound::Box(Extent2d::from_bounds(u0, v0, u1, v1).unwrap())
}

fn refused(r: Result<impl Sized, MakeError>) -> MakeError {
    match r {
        Err(e) => e,
        Ok(_) => panic!("made"),
    }
}

fn area_of(b: &Brep) -> f64 {
    let spec = MassSpec::new(RelAccuracy::new(1e-12).unwrap()).with_about(Point3::ORIGIN);
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

#[track_caller]
fn is_sheet(body: &Brep) {
    let doc = doc();
    let (mut scratch, stop, settings) = (Scratch::new(), Stop::new(), Settings::new());
    let mut cx = Cx::new(&doc, &settings, &mut scratch, &stop, &NoTrace);
    let class = scx_check::check(body, &mut cx, Scope::Import)
        .unwrap()
        .class;
    assert_eq!(class, Class::Sheet);
}

/// A bilinear patch with planar corners (a general quadrilateral in a tilted plane).
fn patch() -> (NurbsSurface<3>, [[f64; 3]; 4]) {
    // In the plane z = 0.5 x - 0.25 y + 1: (u, v) -> corners [00, 01, 10, 11].
    let at = |x: f64, y: f64| [x, y, 0.5 * x - 0.25 * y + 1.0];
    let c = [at(0.0, 0.0), at(0.5, 3.0), at(4.0, 0.5), at(5.5, 4.5)];
    let k = || KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    let net = Net::new(2, 2, vec![c[0], c[1], c[2], c[3]]).unwrap();
    (NurbsSurface::new(k(), k(), net, None).unwrap(), c)
}

/// The bilinear map of the corners, in the order `[00, 01, 10, 11]`.
fn bilinear(c: &[[f64; 3]; 4], u: f64, v: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| {
        (1.0 - u) * (1.0 - v) * c[0][i]
            + (1.0 - u) * v * c[1][i]
            + u * (1.0 - v) * c[2][i]
            + u * v * c[3][i]
    })
}

/// The area of a planar quadrilateral `a b c d` in order: half the cross of its diagonals.
fn quad_area(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    let p = [0, 1, 2].map(|i| c[i] - a[i]);
    let q = [0, 1, 2].map(|i| d[i] - b[i]);
    let x = [
        p[1] * q[2] - p[2] * q[1],
        p[2] * q[0] - p[0] * q[2],
        p[0] * q[1] - p[1] * q[0],
    ];
    0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt()
}

/// The full-turn NURBS cylinder of radius `r` and height `h`: four rational quadratic quarter
/// arcs (the knots of `u` at the quarters), `v` linear in the height.
fn cylinder(r: f64, h: f64) -> NurbsSurface<3> {
    let s = FRAC_1_SQRT_2;
    let ring = [
        (r, 0.0, 1.0),
        (r, r, s),
        (0.0, r, 1.0),
        (-r, r, s),
        (-r, 0.0, 1.0),
        (-r, -r, s),
        (0.0, -r, 1.0),
        (r, -r, s),
        (r, 0.0, 1.0),
    ];
    let (mut points, mut weights) = (Vec::new(), Vec::new());
    for (x, y, w) in ring {
        for z in [0.0, h] {
            points.push([x, y, z]);
            weights.push(w);
        }
    }
    let u = KnotVector::new(
        2,
        vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ],
    )
    .unwrap();
    let v = KnotVector::new(1, vec![0.0, 0.0, 1.0, 1.0]).unwrap();
    NurbsSurface::new(
        u,
        v,
        Net::new(9, 2, points).unwrap(),
        Some(Net::new(9, 2, weights).unwrap()),
    )
    .unwrap()
}

/// Every edge a NURBS curve on the surface at its own parameters, and every pcurve a line in
/// uv that lifts onto its edge, within `tol`. Returns the number of edges.
#[track_caller]
fn assert_iso_sheet(b: &Brep, tol: f64) -> usize {
    let mut edges = 0;
    for f in b.faces() {
        assert!(f.shell(FaceSide::Front).region().is_outside());
        assert_eq!(f.shell(FaceSide::Front).id(), f.shell(FaceSide::Back).id());
        for u in f.uses() {
            let (UseGeom::Pcurve { curve, interval }, EdgeShape::Curve { curve: c3, .. }) =
                (u.geom(), u.edge().shape())
            else {
                panic!("an iso edge has a curve and a pcurve");
            };
            assert!(
                matches!(c3, Curve::Nurbs(_)),
                "an iso edge is a NURBS curve"
            );
            assert!(matches!(curve, scx_curve::Curve2::Line(_)));
            edges += 1;
            for i in 0..=16 {
                let t = interval.min() + interval.length() * f64::from(i) / 16.0;
                let on = f.surface().point(curve.point(t));
                assert!(
                    on.distance(c3.point(t)) <= tol,
                    "pcurve off its edge at {t}"
                );
            }
        }
    }
    edges
}

/// The boundary of a box on the bilinear patch is its four sides, and they are the patch's
/// points bit for bit (a polynomial surface, corners on its knots).
#[test]
fn a_bilinear_patch_box_has_exact_edges() {
    let (s, c) = patch();
    let surface = Surface::Nurbs(Arc::new(s));
    let m = face(&surface, &bx(0.0, 1.0, 0.0, 1.0), &doc()).unwrap();
    assert_eq!(
        (
            m.body.faces().len(),
            m.body.edges().len(),
            m.body.vertices().len()
        ),
        (1, 4, 4)
    );
    assert_eq!(assert_iso_sheet(&m.body, 1e-12), 4);
    // The whole patch: the vertices are the corners, the edges' ends are them, bit for bit.
    for v in m.body.vertices() {
        let p = v.point();
        assert!(c.contains(&[p.x, p.y, p.z]), "vertex {p:?} is not a corner");
    }
    for e in m.body.edges() {
        let EdgeShape::Curve { curve, interval } = e.shape() else {
            panic!()
        };
        let ends = e.ends().map(|v| m.body.vertex(v).unwrap().point());
        assert_eq!(curve.point(interval.min()), ends[0]);
        assert_eq!(curve.point(interval.max()), ends[1]);
    }
    let want = quad_area(c[0], c[2], c[3], c[1]);
    assert!(rel(area_of(&m.body), want) < 1e-10);
    is_sheet(&m.body);
}

/// Boxes inside the patch: the area is the quadrilateral of the box's four corners, and
/// every edge point is the patch's point (the iso-curve of a bilinear surface is a line, the
/// patch's points on it within 1e-12).
#[test]
fn boxes_on_a_bilinear_patch_have_the_quadrilateral_area() {
    let (s, c) = patch();
    let surface = Surface::Nurbs(Arc::new(s));
    for [u0, u1, v0, v1] in [
        [0.1, 0.9, 0.2, 0.7],
        [0.0, 0.5, 0.0, 1.0],
        [0.3, 0.3125, 0.5, 1.0],
        [0.25, 1.0, 0.125, 0.875],
    ] {
        let m = face(&surface, &bx(u0, u1, v0, v1), &doc()).unwrap();
        assert_eq!(assert_iso_sheet(&m.body, 1e-12), 4);
        for e in m.body.edges() {
            let EdgeShape::Curve { curve, interval } = e.shape() else {
                panic!()
            };
            for i in 0..=8 {
                let t = interval.min() + interval.length() * f64::from(i) / 8.0;
                let p = curve.point(t);
                // On the patch: its distance from the plane is zero, and it is bilinear at
                // the parameters the edge holds.
                let plane = p.z - (0.5 * p.x - 0.25 * p.y + 1.0);
                assert!(plane.abs() < 1e-12, "{p:?} off the plane");
            }
        }
        let want = quad_area(
            bilinear(&c, u0, v0),
            bilinear(&c, u1, v0),
            bilinear(&c, u1, v1),
            bilinear(&c, u0, v1),
        );
        let got = area_of(&m.body);
        assert!(rel(got, want) < 1e-10, "{got} vs {want}");
        is_sheet(&m.body);
    }
}

/// The iso-edges lie on the surface: sampled at the edge's parameter the curve's point is
/// the surface's at the boundary's constant coordinate, within 1e-12 (rational) of it.
#[test]
fn the_cylinder_s_edges_are_its_points_to_1e_12() {
    let (r, h) = (2.5, 3.0);
    let s = cylinder(r, h);
    let surface = Surface::Nurbs(Arc::new(s));
    for [u0, u1, v0, v1] in [
        [0.0, 0.25, 0.0, 1.0],
        [0.1, 0.7, 0.2, 0.9],
        [0.3, 0.55, 0.0, 0.5],
    ] {
        let m = face(&surface, &bx(u0, u1, v0, v1), &doc()).unwrap();
        assert_eq!(assert_iso_sheet(&m.body, 1e-12), 4);
        for f in m.body.faces() {
            for u in f.uses() {
                let (UseGeom::Pcurve { curve, interval }, EdgeShape::Curve { curve: c3, .. }) =
                    (u.geom(), u.edge().shape())
                else {
                    panic!()
                };
                for i in 0..=32 {
                    let t = interval.min() + interval.length() * f64::from(i) / 32.0;
                    let want = surface.point(curve.point(t));
                    assert!(c3.point(t).distance(want) < 1e-12);
                    // Radius r, height in [0, h].
                    let p = c3.point(t);
                    assert!(
                        (scx_num::fmath::hypot(p.x, p.y) - r).abs() < 1e-12,
                        "radius at {t}"
                    );
                    assert!(p.z > -1e-12 && p.z < h + 1e-12);
                }
            }
        }
        is_sheet(&m.body);
    }
}

/// The NURBS cylinder's quarter, half and three quarters are `pi r h` times the fraction of
/// the turn: u at the knots of the quarters is the angle.
#[test]
fn a_nurbs_cylinder_quarter_has_the_closed_form_area() {
    let (r, h) = (2.5, 3.0);
    let surface = Surface::Nurbs(Arc::new(cylinder(r, h)));
    let wall = 2.0 * PI * r * h;
    for (u0, u1, frac) in [
        (0.0, 0.25, 0.25),
        (0.25, 0.5, 0.25),
        (0.0, 0.5, 0.5),
        (0.25, 0.75, 0.5),
        (0.0, 0.75, 0.75),
    ] {
        let m = face(&surface, &bx(u0, u1, 0.0, 1.0), &doc()).unwrap();
        let got = area_of(&m.body);
        assert!(rel(got, wall * frac) < 1e-10, "{u0}..{u1}: {got}");
        assert_eq!(
            (
                m.body.faces().len(),
                m.body.edges().len(),
                m.body.vertices().len()
            ),
            (1, 4, 4)
        );
        is_sheet(&m.body);
    }
    // Half the height of the quarter.
    let m = face(&surface, &bx(0.0, 0.25, 0.0, 0.5), &doc()).unwrap();
    assert!(rel(area_of(&m.body), wall * 0.125) < 1e-10);
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

/// A region of iso-lines on the cylinder: a half with a rectangular hole is the half less the
/// hole's fraction, an L is the half less a quadrant of it.
#[test]
fn a_region_of_iso_lines_on_the_cylinder_has_the_closed_form_area() {
    let (r, h) = (2.0, 4.0);
    let surface = Surface::Nurbs(Arc::new(cylinder(r, h)));
    let wall = 2.0 * PI * r * h;
    // The hole is u in [0.1, 0.2], v in [0.25, 0.75]: its u is not at a knot, so its arc is
    // the surface's own (the angle is not proportional to u), from the points.
    let angle = |u: f64| {
        let p = surface.point(Point2::new(u, 0.0));
        let a = scx_num::fmath::atan2(p.y, p.x);
        a.rem_euclid(2.0 * PI)
    };
    let holed = region(
        rect([0.0, 0.0], [0.5, 1.0]),
        vec![rect([0.1, 0.25], [0.2, 0.75])],
    );
    let m = face(&surface, &Bound::Region(holed), &doc()).unwrap();
    assert_eq!(assert_iso_sheet(&m.body, 1e-12), 8);
    let hole = r * (angle(0.2) - angle(0.1)) * h * 0.5;
    let want = wall * 0.5 - hole;
    let got = area_of(&m.body);
    assert!(rel(got, want) < 1e-10, "{got} vs {want}");
    assert_eq!(m.body.faces().next().unwrap().loops().count(), 2);
    is_sheet(&m.body);

    // An L: u in [0, 0.5] by v in [0, 1], less the quadrant u in [0.25, 0.5], v in [0.5, 1].
    let l = vec![
        line([0.0, 0.0], [0.5, 0.0]),
        line([0.5, 0.0], [0.5, 0.5]),
        line([0.5, 0.5], [0.25, 0.5]),
        line([0.25, 0.5], [0.25, 1.0]),
        line([0.25, 1.0], [0.0, 1.0]),
        line([0.0, 1.0], [0.0, 0.0]),
    ];
    let m = face(&surface, &Bound::Region(region(l, vec![])), &doc()).unwrap();
    assert_eq!(assert_iso_sheet(&m.body, 1e-12), 6);
    let want = wall * (0.5 - 0.25 * 0.5);
    let got = area_of(&m.body);
    assert!(rel(got, want) < 1e-10, "L {got} vs {want}");
    is_sheet(&m.body);
}

/// Typed refusals: a slanted segment, a box past the domain or empty, and a boundary that
/// meets itself (the cylinder's whole turn: the seam).
#[test]
fn what_is_not_an_iso_line_box_is_refused_by_name() {
    let (s, _) = patch();
    let surface = Surface::Nurbs(Arc::new(s));
    let tri = region(
        vec![
            line([0.0, 0.0], [1.0, 0.0]),
            line([1.0, 0.0], [0.0, 1.0]),
            line([0.0, 1.0], [0.0, 0.0]),
        ],
        vec![],
    );
    assert!(matches!(
        refused(face(&surface, &Bound::Region(tri), &doc())),
        MakeError::Unsupported(Unsupported::Segment(_))
    ));
    let arc = region(
        vec![Segment::circle(
            Point2::new(0.5, 0.5),
            0.25,
            scx_num::frame::Sense::Forward,
        )],
        vec![],
    );
    assert!(matches!(
        refused(face(&surface, &Bound::Region(arc), &doc())),
        MakeError::Unsupported(Unsupported::Segment(_))
    ));
    let uvbox = |e: MakeError| matches!(e, MakeError::Refused(Refusal::UvBox { .. }));
    assert!(uvbox(refused(face(
        &surface,
        &bx(0.0, 1.1, 0.0, 1.0),
        &doc()
    ))));
    assert!(uvbox(refused(face(
        &surface,
        &bx(-0.1, 1.0, 0.0, 1.0),
        &doc()
    ))));
    assert!(uvbox(refused(face(
        &surface,
        &bx(0.5, 0.5, 0.0, 1.0),
        &doc()
    ))));
    // A parameter within rounding of the end is the end.
    face(&surface, &bx(0.0, 1.0 + 1e-14, 0.0, 1.0), &doc()).unwrap();
    // The cylinder's whole turn meets itself at the seam; a turn less a sliver does not.
    let cyl = Surface::Nurbs(Arc::new(cylinder(2.0, 3.0)));
    assert_eq!(
        refused(face(&cyl, &bx(0.0, 1.0, 0.0, 1.0), &doc())),
        MakeError::Unsupported(Unsupported::NurbsFace)
    );
    face(&cyl, &bx(0.0, 0.99, 0.0, 1.0), &doc()).unwrap();
}

/// A NURBS plane: the exact form of a plane over a box is a bilinear patch, and its face is
/// the plane's rectangle.
#[test]
fn a_planes_nurbs_form_is_its_rectangle() {
    use scx_num::frame::Frame;
    use scx_num::vector3d::Vec3;
    use scx_surface::Plane;
    let frame = Frame::new(
        Point3::new(1.0, 2.0, 3.0),
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(-3.0, 0.0, 1.0),
    )
    .unwrap();
    let over = Extent2d::from_bounds(0.0, 0.0, 6.0, 4.0).unwrap();
    let n = Surface::Plane(Plane::new(frame)).to_nurbs(&over).unwrap();
    let surface = Surface::Nurbs(Arc::new(n));
    let m = face(&surface, &bx(1.0, 5.0, 0.5, 3.5), &doc()).unwrap();
    assert!(rel(area_of(&m.body), 4.0 * 3.0) < 1e-10);
    is_sheet(&m.body);
}
