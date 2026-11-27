// Written for solidcorex. Apache-2.0.
//! `axial_section` and `holds_axis` (research 74 §30, SU8) against closed forms on a tilted
//! frame away from the origin: each kind's meridians are known points, cuts are put at known
//! parameters, and the pieces must start and end there.

#![allow(
    missing_docs,
    clippy::pedantic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::panic,
    clippy::disallowed_methods
)]

use std::f64::consts::{PI, TAU};

use scx_num::extent3d::Extent3d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::{Length, Resolution};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_surface::{
    AxialError, AxialPiece, Cone, Cylinder, Holds, Plane, Sphere, Surface, Torus, axial_section,
    holds_axis,
};

const O: Point3 = Point3::new(1.5, -2.0, 0.7);

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn res() -> Resolution {
    Resolution::new(mm(1e-7)).unwrap()
}

/// The kinds' frame: `x`, `y`, `z` orthonormal and tilted.
fn tilted() -> Frame {
    Frame::new(O, Vec3::new(0.3, -0.4, 0.86), Vec3::new(1.0, 0.2, 0.0)).unwrap()
}

/// The plane through the axis `z`, spanned by `z` (its x) and the frame's `x` (its y), at
/// `shift` along the frame's `y`.
fn through_axis(f: &Frame, shift: f64) -> Plane {
    let o = f.origin() + *f.y() * shift;
    Plane::new(Frame::new(o, *f.y(), *f.z()).unwrap())
}

fn big_box() -> Extent3d {
    Extent3d::from_corners(
        Point3::new(-20.0, -20.0, -20.0),
        Point3::new(20.0, 20.0, 20.0),
    )
    .unwrap()
}

fn near(a: Point3, b: Point3, tol: f64) {
    assert!(
        a.distance(b) <= tol,
        "{a:?} against {b:?}: {}",
        a.distance(b)
    );
}

fn at(f: &Frame, x: f64, y: f64, z: f64) -> Point3 {
    f.origin() + *f.x() * x + *f.y() * y + *f.z() * z
}

fn all(_: Point3) -> Result<bool, ()> {
    Ok(true)
}

fn cylinder(r: f64) -> Surface {
    Cylinder::new(tilted(), mm(r), Sense::Forward)
        .unwrap()
        .into()
}

#[test]
fn the_plane_through_the_axis_holds_it_and_one_beside_it_does_not() {
    let f = tilted();
    let s = cylinder(3.0);
    assert_eq!(
        holds_axis(&s, &through_axis(&f, 0.0), &big_box(), res()).unwrap(),
        Holds::Yes
    );
    // A plane 0.25 off, parallel to the axis: the axis stands 0.25 off at both ends.
    let Holds::Off { by } = holds_axis(&s, &through_axis(&f, 0.25), &big_box(), res()).unwrap()
    else {
        panic!("held")
    };
    assert!((by.mm() - 0.25).abs() < 1e-12, "{}", by.mm());
    // Within the tolerance it holds.
    assert_eq!(
        holds_axis(&s, &through_axis(&f, 5e-8), &big_box(), res()).unwrap(),
        Holds::Yes
    );
}

#[test]
fn a_plane_tilted_to_the_axis_stands_off_by_the_reach_times_the_sine() {
    // World z axis, a plane through the origin tilted by alpha about x: the axis leaves it
    // at sin(alpha) per unit along the axis; the box reaches 7 and 3 along it.
    let alpha = 0.01_f64;
    let s: Surface = Cylinder::new(Frame::WORLD, mm(1.0), Sense::Forward)
        .unwrap()
        .into();
    let n = Vec3::new(0.0, alpha.cos(), alpha.sin());
    let plane = Plane::new(Frame::new(Point3::new(0.0, 0.0, 0.0), n, Vec3::X).unwrap());
    let over =
        Extent3d::from_corners(Point3::new(-1.0, -1.0, -3.0), Point3::new(1.0, 1.0, 7.0)).unwrap();
    let Holds::Off { by } = holds_axis(&s, &plane, &over, res()).unwrap() else {
        panic!("held")
    };
    assert!((by.mm() - 7.0 * alpha.sin()).abs() < 1e-12, "{}", by.mm());
}

#[test]
fn a_plane_square_to_the_axis_over_a_tiny_box_cuts_circles_not_meridians() {
    let s: Surface = Cylinder::new(Frame::WORLD, mm(1.0), Sense::Forward)
        .unwrap()
        .into();
    let plane = Plane::new(Frame::WORLD);
    let tiny =
        Extent3d::from_corners(Point3::new(-1.0, -1.0, 0.0), Point3::new(1.0, 1.0, 1e-9)).unwrap();
    assert_eq!(holds_axis(&s, &plane, &tiny, res()).unwrap(), Holds::Across);
    assert!(matches!(
        axial_section(&s, &plane, &tiny, res(), &[], all),
        Err(AxialError::AxisAcross)
    ));
}

#[test]
fn a_plane_or_a_nurbs_surface_has_no_axis() {
    let f = tilted();
    let p: Surface = Plane::new(f).into();
    assert!(matches!(
        holds_axis(&p, &through_axis(&f, 0.0), &big_box(), res()),
        Err(AxialError::NotRevolved)
    ));
    assert!(matches!(
        axial_section(&p, &through_axis(&f, 0.0), &big_box(), res(), &[], all),
        Err(AxialError::NotRevolved)
    ));
}

#[test]
fn a_cylinder_face_gives_its_lines_between_the_cuts() {
    let f = tilted();
    let r = 3.0;
    let s = cylinder(r);
    // The face is the strip 1 <= z <= 4 on both sides.
    let cuts = [
        at(&f, r, 0.0, 1.0),
        at(&f, r, 0.0, 4.0),
        at(&f, -r, 0.0, 1.0),
        at(&f, -r, 0.0, 4.0),
    ];
    let pieces = axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &cuts, |p| {
        let z = (p - O).dot(*f.z());
        Ok::<_, ()>((1.0..=4.0).contains(&z))
    })
    .unwrap();
    assert_eq!(pieces.len(), 2);
    // Each runs between its two cuts, whichever way round.
    for (start, end) in [
        (at(&f, r, 0.0, 1.0), at(&f, r, 0.0, 4.0)),
        (at(&f, -r, 0.0, 1.0), at(&f, -r, 0.0, 4.0)),
    ] {
        assert!(
            pieces
                .iter()
                .any(|p| matches!(*p, AxialPiece::Line { start: a, end: b }
                if a.distance(start) < 1e-12 && b.distance(end) < 1e-12)),
            "{pieces:?}"
        );
    }
}

#[test]
fn a_face_on_one_side_gives_one_line_and_a_missed_face_none() {
    let f = tilted();
    let s = cylinder(3.0);
    let cuts = [at(&f, 3.0, 0.0, 1.0), at(&f, 3.0, 0.0, 4.0)];
    let plane = through_axis(&f, 0.0);
    let side = |p: Point3| Ok::<_, ()>((p - O).dot(*f.x()) > 0.0);
    let pieces = axial_section(&s, &plane, &big_box(), res(), &cuts, side).unwrap();
    assert_eq!(pieces.len(), 1);
    // The plane misses the face when no span is the face's.
    let none = axial_section(&s, &plane, &big_box(), res(), &cuts, |_| Ok::<_, ()>(false)).unwrap();
    assert!(none.is_empty());
}

#[test]
fn cuts_within_the_tolerance_of_each_other_are_one() {
    let f = tilted();
    let s = cylinder(3.0);
    // An edge ends at z = 2 and the next starts 5e-8 on (below the 1e-7 tolerance): one cut,
    // so one span 2 to 4, not a sliver and a span.
    let cuts = [
        at(&f, 3.0, 0.0, 2.0),
        at(&f, 3.0, 0.0, 2.0 + 5e-8),
        at(&f, 3.0, 0.0, 4.0),
    ];
    let pieces = axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &cuts, all).unwrap();
    assert_eq!(pieces.len(), 1);
    let AxialPiece::Line { start, end } = pieces[0] else {
        panic!("{pieces:?}")
    };
    near(start, at(&f, 3.0, 0.0, 2.0), 1e-7);
    near(end, at(&f, 3.0, 0.0, 4.0), 1e-12);
}

#[test]
fn a_cone_gives_generators_from_the_apex() {
    let f = tilted();
    let a = 0.4_f64;
    let r0 = 2.0;
    for angle in [a, -a] {
        let s: Surface = Cone::new(f, mm(r0), angle, Sense::Forward).unwrap().into();
        // The apex is at v = -r0 / tan(angle); the nappe runs along +z for a positive angle.
        let apex = at(&f, 0.0, 0.0, -r0 / angle.tan());
        let into = if angle > 0.0 { 1.0 } else { -1.0 };
        let generator =
            |side: f64, d: f64| apex + (*f.z() * (into * a.cos()) + *f.x() * (side * a.sin())) * d;
        let cuts = [
            generator(1.0, 2.0),
            generator(1.0, 5.0),
            generator(-1.0, 3.0),
            generator(-1.0, 6.0),
        ];
        // The face is the band between distances 2 and 6 from the apex (the apex always
        // splits a generator, so the span from it to the first cut is tested too).
        let band = |p: Point3| Ok::<_, ()>((2.0..6.0).contains(&p.distance(apex)));
        let pieces =
            axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &cuts, band).unwrap();
        assert_eq!(pieces.len(), 2, "{angle}: {pieces:?}");
        for (d0, d1, side) in [(2.0, 5.0, 1.0), (3.0, 6.0, -1.0)] {
            assert!(
                pieces
                    .iter()
                    .any(|p| matches!(*p, AxialPiece::Line { start, end }
                    if start.distance(generator(side, d0)) < 1e-12
                        && end.distance(generator(side, d1)) < 1e-12)),
                "{angle}: {pieces:?}"
            );
        }
    }
}

#[test]
fn a_cone_face_that_holds_the_apex_runs_from_it() {
    let f = tilted();
    let (a, r0) = (0.4_f64, 2.0);
    let s: Surface = Cone::new(f, mm(r0), a, Sense::Forward).unwrap().into();
    let apex = at(&f, 0.0, 0.0, -r0 / a.tan());
    let gen_point = |side: f64, d: f64| apex + (*f.z() * a.cos() + *f.x() * (side * a.sin())) * d;
    // The face is the whole nappe to distance 4 on both generators: one cut each.
    let cuts = [gen_point(1.0, 4.0), gen_point(-1.0, 4.0)];
    let pieces = axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &cuts, |p| {
        Ok::<_, ()>(p.distance(apex) < 4.0)
    })
    .unwrap();
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    for side in [1.0, -1.0] {
        assert!(
            pieces
                .iter()
                .any(|p| matches!(*p, AxialPiece::Line { start, end }
                if start.distance(apex) < 1e-12 && end.distance(gen_point(side, 4.0)) < 1e-12)),
            "{pieces:?}"
        );
    }
}

fn on_sphere(f: &Frame, r: f64, theta: f64) -> Point3 {
    // The plane's x is the frame's z, its y the frame's x: the angle counts from z towards x.
    f.origin() + (*f.z() * theta.cos() + *f.x() * theta.sin()) * r
}

#[test]
fn a_sphere_gives_one_circle_whole_or_arcs_counter_clockwise() {
    let f = tilted();
    let r = 2.5;
    let s: Surface = Sphere::new(f, mm(r), Sense::Forward).unwrap().into();
    let plane = through_axis(&f, 0.0);
    // No cuts and the face keeps all: the whole circle.
    let whole = axial_section(&s, &plane, &big_box(), res(), &[], all).unwrap();
    assert_eq!(whole.len(), 1);
    let AxialPiece::Circle { centre, radius } = whole[0] else {
        panic!("{whole:?}")
    };
    near(centre, O, 1e-12);
    assert!((radius - r).abs() < 1e-15);
    // Cuts at 30 and 100 degrees; the face is the span between them.
    let (t0, t1) = (30.0_f64.to_radians(), 100.0_f64.to_radians());
    let cuts = [on_sphere(&f, r, t0), on_sphere(&f, r, t1)];
    let inside = |p: Point3| {
        let d = p - O;
        let th = d.dot(*f.x()).atan2(d.dot(*f.z())).rem_euclid(TAU);
        Ok::<_, ()>(th > t0 && th < t1)
    };
    let arcs = axial_section(&s, &plane, &big_box(), res(), &cuts, inside).unwrap();
    assert_eq!(arcs.len(), 1);
    let AxialPiece::Arc { centre, start, end } = arcs[0] else {
        panic!("{arcs:?}")
    };
    near(centre, O, 1e-12);
    near(start, on_sphere(&f, r, t0), 1e-12);
    near(end, on_sphere(&f, r, t1), 1e-12);
    // The other span wraps through angle 0: it runs from 100 degrees round to 30 + 360.
    let outside = |p: Point3| inside(p).map(|b| !b);
    let wrap = axial_section(&s, &plane, &big_box(), res(), &cuts, outside).unwrap();
    let AxialPiece::Arc { start, end, .. } = wrap[0] else {
        panic!("{wrap:?}")
    };
    near(start, on_sphere(&f, r, t1), 1e-12);
    near(end, on_sphere(&f, r, t0 + TAU), 1e-12);
}

#[test]
fn a_sphere_needs_only_its_centre_on_the_plane() {
    let f = tilted();
    let s: Surface = Sphere::new(f, mm(2.5), Sense::Forward).unwrap().into();
    // Any plane through the centre, not through "the" axis.
    let o = f.origin();
    let slanted = Plane::new(Frame::new(o, Vec3::new(1.0, 2.0, 3.0), Vec3::Z).unwrap());
    assert_eq!(
        holds_axis(&s, &slanted, &big_box(), res()).unwrap(),
        Holds::Yes
    );
    let miss = Plane::new(Frame::new(o + Vec3::new(0.0, 0.0, 0.3), Vec3::Z, Vec3::X).unwrap());
    let Holds::Off { by } = holds_axis(&s, &miss, &big_box(), res()).unwrap() else {
        panic!("held")
    };
    assert!((by.mm() - 0.3).abs() < 1e-12);
}

#[test]
fn a_torus_gives_two_circles_of_the_tube_radius_at_the_ring_radius() {
    let f = tilted();
    let (major, minor) = (5.0, 1.25);
    let s: Surface = Torus::new(f, mm(major), mm(minor), Sense::Forward)
        .unwrap()
        .into();
    let pieces = axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &[], all).unwrap();
    assert_eq!(pieces.len(), 2);
    for side in [major, -major] {
        assert!(
            pieces
                .iter()
                .any(|p| matches!(*p, AxialPiece::Circle { centre, radius }
                if centre.distance(at(&f, side, 0.0, 0.0)) < 1e-12
                    && (radius - minor).abs() < 1e-15)),
            "{pieces:?}"
        );
    }
}

#[test]
fn a_torus_arc_between_cuts_on_the_outer_circle() {
    let f = tilted();
    let (major, minor) = (5.0, 1.25);
    let s: Surface = Torus::new(f, major.into_mm(), minor.into_mm(), Sense::Forward)
        .unwrap()
        .into();
    // The circle at +major along the frame's x: points at angle t from the plane's x (the
    // frame's z) towards the plane's y... here the meridian's own e1 is the axis direction
    // and e2 = n x e1 the frame's x.
    let centre = at(&f, major, 0.0, 0.0);
    let on = |t: f64| centre + (*f.z() * t.cos() + *f.x() * t.sin()) * minor;
    let (t0, t1) = (0.5, 2.0);
    let cuts = [on(t0), on(t1)];
    let keep = |p: Point3| {
        let d = p - centre;
        let th = d.dot(*f.x()).atan2(d.dot(*f.z())).rem_euclid(TAU);
        // Only the +major circle's span between the cuts is the face's.
        Ok::<_, ()>(d.length() < 2.0 * minor && th > t0 && th < t1)
    };
    let pieces = axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &cuts, keep).unwrap();
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    let AxialPiece::Arc {
        centre: c,
        start,
        end,
    } = pieces[0]
    else {
        panic!("{pieces:?}")
    };
    near(c, centre, 1e-12);
    near(start, on(t0), 1e-12);
    near(end, on(t1), 1e-12);
}

trait IntoMm {
    fn into_mm(self) -> Length;
}
impl IntoMm for f64 {
    fn into_mm(self) -> Length {
        mm(self)
    }
}

#[test]
fn a_cut_off_the_section_is_refused_with_its_certified_gap_not_put_on_a_meridian() {
    let f = tilted();
    let s = cylinder(3.0);
    let plane = through_axis(&f, 0.0);
    // 0.01 off the cylinder's line, far past the section tolerance (2e-7).
    let off = at(&f, 3.0, 0.01, 2.0);
    let err = axial_section(&s, &plane, &big_box(), res(), &[off], all).unwrap_err();
    let AxialError::CutOffSection { index, gap } = err else {
        panic!("{err:?}")
    };
    assert_eq!(index, 0);
    assert!(
        gap.lower() <= 0.01 + 1e-12 && gap.upper() >= 0.01 - 1e-12,
        "{gap:?}"
    );
    assert!(gap.lower() > 0.0099, "{gap:?}");
    // Just inside the allowance (1.5e-7 against 2e-7) is on it.
    let near_enough = at(&f, 3.0, 1.5e-7, 2.0);
    let cuts = [near_enough, at(&f, 3.0, 0.0, 4.0)];
    assert!(axial_section(&s, &plane, &big_box(), res(), &cuts, all).is_ok());
    // Not finite.
    let nan = Point3::new(f64::NAN, 0.0, 0.0);
    assert!(matches!(
        axial_section(&s, &plane, &big_box(), res(), &[cuts[1], nan], all),
        Err(AxialError::CutNotFinite { index: 1 })
    ));
}

#[test]
fn a_plane_off_the_axis_is_refused_with_how_far() {
    let f = tilted();
    let s = cylinder(3.0);
    let err = axial_section(&s, &through_axis(&f, 0.5), &big_box(), res(), &[], all).unwrap_err();
    let AxialError::AxisOff { by } = err else {
        panic!("{err:?}")
    };
    assert!((by.mm() - 0.5).abs() < 1e-12);
}

#[test]
fn the_face_tests_error_comes_back_as_its_own() {
    let f = tilted();
    let s = cylinder(3.0);
    let cuts = [at(&f, 3.0, 0.0, 1.0), at(&f, 3.0, 0.0, 4.0)];
    let err = axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &cuts, |_| {
        Err("no")
    })
    .unwrap_err();
    assert_eq!(err, AxialError::OnFace("no"));
}

#[test]
fn a_run_twice_is_the_same_to_the_bit() {
    let f = tilted();
    let s: Surface = Torus::new(f, mm(5.0), mm(1.25), Sense::Forward)
        .unwrap()
        .into();
    let run = || axial_section(&s, &through_axis(&f, 0.0), &big_box(), res(), &[], all).unwrap();
    assert_eq!(run(), run());
    let _ = (PI, UnitVec3::X);
}
