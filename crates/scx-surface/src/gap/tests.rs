// Written for solidcorex. Apache-2.0.
//! The gaps against closed forms: across tangency and near-zero gaps, every bracket holds
//! the truth, and an aimed width is met.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;

use scx_curve::{Circle, Curve, Ellipse, Line};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::extent2d::Extent2d;
use scx_num::fmath;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, Vec3};

use super::*;
use crate::{Cone, Cylinder, Plane, Sphere, Torus, pcurve_of};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn over(a: f64, b: f64) -> Extent1d {
    Extent1d::new(a, b).unwrap()
}

fn frame_at(o: Point3, z: Vec3, x: Vec3) -> Frame {
    Frame::new(o, z, x).unwrap()
}

/// A tilted frame away from the origin, so no case leans on a world axis.
fn tilted() -> Frame {
    frame_at(
        Point3::new(1.0, 2.0, -3.0),
        Vec3::new(0.3, -0.2, 0.9),
        Vec3::new(1.0, 0.1, 0.0),
    )
}

/// The point at local `(x, y, z)` of `f`.
fn at(f: &Frame, x: f64, y: f64, z: f64) -> Point3 {
    f.origin() + *f.x() * x + *f.y() * y + *f.z() * z
}

fn circle(f: &Frame, centre: (f64, f64, f64), r: f64) -> Curve {
    let fr = frame_at(at(f, centre.0, centre.1, centre.2), *f.z(), *f.x());
    Curve::Circle(Circle::new(fr, mm(r), Sense::Forward).unwrap())
}

fn line(p: Point3, d: Vec3) -> Curve {
    Curve::Line(Line::new(p, d).unwrap())
}

fn plane(f: Frame) -> Surface {
    Surface::Plane(Plane::new(f))
}

fn cylinder(f: Frame, r: f64) -> Surface {
    Surface::Cylinder(Cylinder::new(f, mm(r), Sense::Forward).unwrap())
}

fn sphere(f: Frame, r: f64) -> Surface {
    Surface::Sphere(Sphere::new(f, mm(r), Sense::Forward).unwrap())
}

const W: f64 = 1e-9;

/// The bracket holds `truth` (with the rounding of computing `truth` itself) and is at most
/// `W` wide.
fn holds(b: Bracket, truth: f64) {
    let slack = 1e-12 * (1.0 + truth.abs());
    assert!(
        b.lower() <= truth + slack && truth <= b.upper() + slack,
        "{b:?} does not hold {truth}"
    );
    assert!(b.width() <= W, "{b:?} wider than {W}");
}

fn gap(c: &Curve, o: Extent1d, s: &Surface) -> Bracket {
    let mut budget = Budget::new(1_000_000);
    curve_surface_gap(c, &o, s, Aim::Width(W), &mut budget, &Stop::new()).unwrap()
}

#[test]
fn a_line_above_a_plane_is_its_height() {
    let f = tilted();
    let s = plane(f);
    let c = line(at(&f, 0.0, 0.0, 2.5), *f.x() + *f.y() * 0.5);
    holds(gap(&c, over(-3.0, 4.0), &s), 2.5);
    // A slant line from the plane: the far end.
    let d = line(at(&f, 0.0, 0.0, 0.0), *f.x() + *f.z() * 0.25);
    holds(gap(&d, over(0.0, 2.0), &s), 0.5);
}

#[test]
fn a_circle_in_its_plane_has_no_gap() {
    let f = tilted();
    let b = gap(
        &circle(&f, (0.3, -0.2, 0.0), 2.0),
        over(0.0, TAU),
        &plane(f),
    );
    holds(b, 0.0);
    assert!(b.upper() < 1e-12, "{b:?}");
}

#[test]
fn a_tilted_circle_strays_by_its_tilt() {
    let f = tilted();
    let (r, a) = (3.0, 0.4_f64);
    let z = *f.z() * fmath::cos(a) + *f.y() * fmath::sin(a);
    let c = Curve::Circle(
        Circle::new(
            frame_at(at(&f, 1.0, 1.0, 0.0), z, *f.x()),
            mm(r),
            Sense::Forward,
        )
        .unwrap(),
    );
    holds(gap(&c, over(0.0, TAU), &plane(f)), r * fmath::sin(a));
    // Over a part away from the extremes: its far end.
    holds(
        gap(&c, over(0.0, 1.0), &plane(f)),
        r * fmath::sin(a) * fmath::sin(1.0),
    );
}

#[test]
fn a_coaxial_circle_on_a_cylinder_closes_at_once() {
    let f = tilted();
    let s = cylinder(f, 2.0);
    let b = gap(&circle(&f, (0.0, 0.0, 1.5), 2.0), over(0.0, TAU), &s);
    holds(b, 0.0);
    assert!(b.upper() < 1e-12, "{b:?}");
    holds(
        gap(&circle(&f, (0.0, 0.0, -4.0), 2.75), over(0.0, TAU), &s),
        0.75,
    );
    let mut budget = Budget::new(1_000_000);
    let one = curve_surface_gap(
        &circle(&f, (0.0, 0.0, 1.5), 2.0),
        &over(0.0, TAU),
        &s,
        Aim::Width(W),
        &mut budget,
        &Stop::new(),
    )
    .unwrap();
    assert!(budget.used() < 64, "{} units for {one:?}", budget.used());
}

#[test]
fn an_off_axis_circle_touches_a_cylinder_inside() {
    // Radius r, centre e off the axis: rho runs over [r - e, r + e], so the gap e is reached
    // at two interior points and the circle crosses the cylinder between them.
    let f = tilted();
    let (r, e) = (2.0, 1e-3);
    holds(
        gap(
            &circle(&f, (e, 0.0, 0.5), r),
            over(0.0, TAU),
            &cylinder(f, r),
        ),
        e,
    );
    // Near zero.
    let e = 1e-7;
    holds(
        gap(
            &circle(&f, (0.0, e, 0.5), r),
            over(0.0, TAU),
            &cylinder(f, r),
        ),
        e,
    );
}

#[test]
fn a_tangent_line_leaves_a_cylinder_at_its_ends() {
    let f = tilted();
    let (r, l) = (1.5, 2.0);
    let c = line(at(&f, r, 0.0, 0.7), *f.y());
    holds(
        gap(&c, over(-l, l), &cylinder(f, r)),
        (r * r + l * l).sqrt() - r,
    );
    // Along the axis: constant.
    let d = line(at(&f, r + 0.25, 0.0, 0.0), *f.z());
    holds(gap(&d, over(-5.0, 5.0), &cylinder(f, r)), 0.25);
}

#[test]
fn circles_against_a_sphere() {
    let f = tilted();
    let r = 2.0;
    let s = sphere(f, r);
    // A latitude: on it.
    let h = 0.8_f64;
    let lat = circle(&f, (0.0, 0.0, h), (r * r - h * h).sqrt());
    holds(gap(&lat, over(0.0, TAU), &s), 0.0);
    // A great circle's radius off by 0.1.
    holds(
        gap(&circle(&f, (0.0, 0.0, 0.0), r + 0.1), over(0.0, TAU), &s),
        0.1,
    );
    // A tangent line through the pole: the ends.
    let l = 0.5;
    let t = line(at(&f, 0.0, 0.0, r), *f.x());
    holds(gap(&t, over(-l, l), &s), (r * r + l * l).sqrt() - r);
}

#[test]
fn a_cone_by_its_generator_and_by_its_apex() {
    let f = tilted();
    let (r, half) = (1.0, PI / 4.0);
    let s = Surface::Cone(Cone::new(f, mm(r), half, Sense::Forward).unwrap());
    let k = fmath::tan(half);
    // A coaxial circle: the generator's distance.
    let (z, rho) = (2.0, 2.5);
    let truth = (rho - r - k * z).abs() / (1.0 + k * k).sqrt();
    holds(
        gap(&circle(&f, (0.0, 0.0, z), rho), over(0.0, TAU), &s),
        truth,
    );
    // Behind the apex (at z = -1): the apex's distance.
    let (z, rho) = (-3.0, 0.5);
    let truth = (rho * rho + (z + 1.0) * (z + 1.0)).sqrt();
    holds(
        gap(&circle(&f, (0.0, 0.0, z), rho), over(0.0, TAU), &s),
        truth,
    );
}

#[test]
fn a_torus_from_its_equators() {
    let f = tilted();
    let (big, small) = (3.0, 1.0);
    let s = Surface::Torus(Torus::new(f, mm(big), mm(small), Sense::Forward).unwrap());
    holds(
        gap(
            &circle(&f, (0.0, 0.0, 0.0), big + small),
            over(0.0, TAU),
            &s,
        ),
        0.0,
    );
    holds(
        gap(&circle(&f, (0.0, 0.0, 0.0), big + 0.25), over(0.0, TAU), &s),
        0.75,
    );
    holds(
        gap(&circle(&f, (0.0, 0.0, 0.5), big), over(0.0, TAU), &s),
        0.5,
    );
}

#[test]
fn an_ellipse_against_a_plane() {
    let f = tilted();
    let (a, b, tilt) = (3.0, 1.0, 0.3_f64);
    let z = *f.z() * fmath::cos(tilt) + *f.x() * fmath::sin(tilt);
    let e = Curve::Ellipse(
        Ellipse::new(
            frame_at(f.origin(), z, *f.y()),
            mm(a),
            mm(b),
            Sense::Forward,
        )
        .unwrap(),
    );
    // `|a cos t (X . n) + b sin t (Y . n)|` at its largest.
    let ef = frame_at(f.origin(), z, *f.y());
    let lean = fmath::hypot(a * ef.x().dot(*f.z()), b * ef.y().dot(*f.z()));
    holds(gap(&e, over(0.0, TAU), &plane(f)), lean);
}

/// A quadratic and a two-piece cubic NURBS curve, each in a plane `lift` above a plane
/// surface: the gap is `lift` everywhere, by the jets the curve carries (no series form).
#[test]
fn a_nurbs_curve_bowing_over_a_plane_is_its_height() {
    use scx_nurbs::{KnotVector, NurbsCurve};
    let f = tilted();
    let lift = 6.8e-5;
    let pts = |xy: &[(f64, f64)]| -> Vec<[f64; 3]> {
        xy.iter()
            .map(|&(x, y)| {
                let p = at(&f, x, y, lift);
                [p.x, p.y, p.z]
            })
            .collect()
    };
    let bow = NurbsCurve::new(
        KnotVector::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).unwrap(),
        pts(&[(0.2, 0.3), (0.9, 0.5), (0.3, 1.7)]),
        None,
    )
    .unwrap();
    holds(
        gap(&Curve::Nurbs(Arc::new(bow)), over(0.0, 1.0), &plane(f)),
        lift,
    );
    let wave = NurbsCurve::new(
        KnotVector::new(3, vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0]).unwrap(),
        pts(&[(0.0, 0.0), (0.4, 1.0), (0.8, -1.0), (1.2, 1.0), (1.6, 0.0)]),
        Some(vec![1.0, 0.7, 1.3, 0.9, 1.0]),
    )
    .unwrap();
    holds(
        gap(&Curve::Nurbs(Arc::new(wave)), over(0.0, 1.0), &plane(f)),
        lift,
    );
}

#[test]
fn a_nurbs_surface_is_pending() {
    let f = tilted();
    let n = Plane::new(f)
        .to_nurbs(&Extent2d::from_bounds(-1.0, -1.0, 1.0, 1.0).unwrap())
        .unwrap();
    let s = Surface::Nurbs(Arc::new(n));
    let c = line(at(&f, 0.0, 0.0, 1.0), *f.x());
    let mut budget = Budget::new(100);
    let e = curve_surface_gap(
        &c,
        &over(0.0, 1.0),
        &s,
        Aim::Width(W),
        &mut budget,
        &Stop::new(),
    );
    assert_eq!(e, Err(GapError::Pending(Pending::NurbsSurface)));
}

#[test]
fn a_budget_run_out_is_undecided_with_the_bracket_reached() {
    let f = tilted();
    let (r, e) = (2.0, 0.3);
    let c = circle(&f, (e, 0.1, 0.0), r);
    let s = cylinder(f, r + 0.05);
    // rho runs to r + |(e, 0.1)| at its largest.
    let truth = (r + fmath::hypot(e, 0.1) - (r + 0.05))
        .abs()
        .max((r - fmath::hypot(e, 0.1) - (r + 0.05)).abs());
    let mut budget = Budget::new(12);
    let e = curve_surface_gap(
        &c,
        &over(0.0, TAU),
        &s,
        Aim::Width(1e-12),
        &mut budget,
        &Stop::new(),
    );
    let Err(GapError::Undecided {
        reached,
        why: Short::Exhausted(_),
    }) = e
    else {
        panic!("{e:?}");
    };
    assert!(
        reached.lower() <= truth && truth <= reached.upper(),
        "{reached:?}"
    );
    holds(gap(&c, over(0.0, TAU), &s), truth);
    // Stopped.
    let stop = Stop::new();
    stop.set();
    let mut budget = Budget::new(1000);
    let e = curve_surface_gap(
        &c,
        &over(0.0, TAU),
        &s,
        Aim::Width(1e-12),
        &mut budget,
        &stop,
    );
    assert!(
        matches!(
            e,
            Err(GapError::Undecided {
                why: Short::Stopped,
                ..
            })
        ),
        "{e:?}"
    );
}

#[test]
fn an_allowance_is_decided_or_undecided() {
    let f = tilted();
    let c = line(at(&f, 0.0, 0.0, 0.01), *f.x());
    let s = plane(f);
    let run = |a: f64| {
        let mut budget = Budget::new(1000);
        curve_surface_gap(
            &c,
            &over(0.0, 1.0),
            &s,
            Aim::Against(a),
            &mut budget,
            &Stop::new(),
        )
    };
    assert_eq!(run(0.001).unwrap().exceeds(0.001), Some(true));
    assert_eq!(run(0.1).unwrap().exceeds(0.1), Some(false));
    // At the allowance itself the bracket cannot leave it: undecided, never a guess.
    assert!(matches!(run(0.01), Ok(b) if b.exceeds(0.01) == Some(false)) || run(0.01).is_err());
    assert!(matches!(
        curve_surface_gap(
            &c,
            &over(0.0, 1.0),
            &s,
            Aim::Width(0.0),
            &mut Budget::new(9),
            &Stop::new()
        ),
        Err(GapError::BadAim { .. })
    ));
}

#[test]
fn the_same_call_gives_the_same_bracket() {
    let f = tilted();
    let c = circle(&f, (0.4, 0.1, 0.3), 2.0);
    let s = cylinder(f, 2.1);
    assert_eq!(gap(&c, over(0.0, TAU), &s), gap(&c, over(0.0, TAU), &s));
}

// Pcurves.

fn pgap(s: &Surface, p: &Curve2, c: &Curve, o: Extent1d) -> Bracket {
    let mut budget = Budget::new(1_000_000);
    pcurve_edge_gap(s, p, c, &o, Aim::Width(W), &mut budget, &Stop::new()).unwrap()
}

fn res() -> Resolution {
    Resolution::new(mm(1e-9)).unwrap()
}

#[test]
fn a_pcurve_on_a_plane_and_a_lifted_edge() {
    let f = tilted();
    let s = plane(f);
    let o = over(-1.0, 2.0);
    let c = line(at(&f, 0.2, 0.3, 0.0), *f.x() + *f.y() * 0.5);
    let p = pcurve_of(&c, &s, o, res()).unwrap();
    holds(pgap(&s, &p, &c, o), 0.0);
    let lifted = line(at(&f, 0.2, 0.3, 1e-6), *f.x() + *f.y() * 0.5);
    holds(pgap(&s, &p, &lifted, o), 1e-6);
}

#[test]
fn a_circles_pcurve_on_a_cylinder_across_its_arc_breaks() {
    let f = tilted();
    let s = cylinder(f, 2.0);
    let o = over(0.0, TAU);
    let c = circle(&f, (0.0, 0.0, 0.5), 2.0);
    let p = pcurve_of(&c, &s, o, res()).unwrap();
    holds(pgap(&s, &p, &c, o), 0.0);
    // The edge a little outside its pcurve's circle, same parameter.
    holds(pgap(&s, &p, &circle(&f, (0.0, 0.0, 0.5), 2.001), o), 0.001);
}

#[test]
fn a_latitudes_pcurve_on_a_sphere() {
    let f = tilted();
    let s = sphere(f, 2.0);
    let o = over(0.3, 5.0);
    let c = circle(&f, (0.0, 0.0, 1.0), 3.0_f64.sqrt());
    let p = pcurve_of(&c, &s, o, res()).unwrap();
    holds(pgap(&s, &p, &c, o), 0.0);
}

// Points and curves.

#[test]
fn a_vertex_against_its_edges_end() {
    let f = tilted();
    let c = line(at(&f, 0.0, 0.0, 0.0), *f.x());
    let b = vertex_curve_gap(at(&f, 2.0, 3e-7, 0.0), &c, 2.0).unwrap();
    assert!(
        b.lower() <= 3e-7 + 1e-12 && 3e-7 <= b.upper() + 1e-12 && b.width() < 1e-12,
        "{b:?}"
    );
    assert_eq!(
        vertex_curve_gap(f.origin(), &c, f64::NAN),
        Err(GapError::NotFinite)
    );
}

fn ppgap(p: Point3, c: &Curve, o: Extent1d) -> Bracket {
    let mut budget = Budget::new(1_000_000);
    point_curve_gap(p, c, &o, Aim::Width(W), &mut budget, &Stop::new()).unwrap()
}

#[test]
fn a_point_against_a_curve() {
    let f = tilted();
    // The centre of a circle: every point nearest.
    holds(
        ppgap(
            at(&f, 0.0, 0.0, 0.0),
            &circle(&f, (0.0, 0.0, 0.0), 2.0),
            over(0.0, TAU),
        ),
        2.0,
    );
    // Off a segment's middle, and past its end (the end nearest).
    let c = line(at(&f, 0.0, 0.0, 0.0), *f.x());
    holds(ppgap(at(&f, 0.5, 0.25, 0.0), &c, over(0.0, 1.0)), 0.25);
    holds(ppgap(at(&f, 2.0, 0.0, 0.0), &c, over(0.0, 1.0)), 1.0);
    // Nearly on it.
    holds(ppgap(at(&f, 0.5, 1e-8, 0.0), &c, over(0.0, 1.0)), 1e-8);
}

fn ccgap(a: &Curve, ao: Extent1d, b: &Curve, bo: Extent1d) -> Bracket {
    let mut budget = Budget::new(1_000_000);
    curve_curve_gap(a, &ao, b, &bo, Aim::Width(W), &mut budget, &Stop::new()).unwrap()
}

#[test]
fn curves_against_curves() {
    let f = tilted();
    let full = over(0.0, TAU);
    // Concentric circles: every point equally far, so every piece must be bounded below the
    // largest point's lower bound; the cost grows as the width to the -1/3 (74 §17), so
    // this plateau is asked to 1e-6 only.
    let (a, b) = (
        circle(&f, (0.0, 0.0, 0.0), 2.0),
        circle(&f, (0.0, 0.0, 0.0), 2.5),
    );
    let mut budget = Budget::new(100_000);
    let e = curve_curve_gap(
        &a,
        &full,
        &b,
        &full,
        Aim::Width(1e-6),
        &mut budget,
        &Stop::new(),
    );
    let e = e.unwrap();
    assert!(
        e.lower() <= 0.5 && 0.5 <= e.upper() && e.width() <= 1e-6,
        "{e:?}"
    );
    // Coincident curves: the plateau at zero is cheap.
    holds(ccgap(&a, full, &a, full), 0.0);
    // A segment against a longer parallel one, then a shorter one (its ends overhang).
    let s = line(at(&f, 0.0, 0.0, 0.0), *f.x());
    let t = line(at(&f, 0.0, 0.1, 0.0), *f.x());
    holds(ccgap(&s, over(0.0, 1.0), &t, over(-1.0, 2.0)), 0.1);
    holds(
        ccgap(&s, over(0.0, 1.0), &t, over(0.25, 0.75)),
        (0.1 * 0.1 + 0.25 * 0.25).sqrt(),
    );
    // An arc against its chord: the sagitta.
    let (r, half) = (2.0, 0.6_f64);
    let arc = circle(&f, (0.0, 0.0, 0.0), r);
    let chord = Curve::Line(
        Line::through(arc.point(FRAC_PI_2 - half), arc.point(FRAC_PI_2 + half)).unwrap(),
    );
    holds(
        ccgap(
            &arc,
            over(FRAC_PI_2 - half, FRAC_PI_2 + half),
            &chord,
            over(0.0, 1.0),
        ),
        r * (1.0 - fmath::cos(half)),
    );
}
