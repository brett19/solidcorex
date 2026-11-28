// Written for solidcorex. Apache-2.0.
//! `facing_trough` (research 74 §30, SU8): the pin's three tests ported to the typed result,
//! then closed-form checks on a tilted cylinder: the half is the one whose outward normal
//! faces the travel, the frame is right handed about the axis, the reach passes every corner
//! of the box.

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
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Cylinder, Half, MARGIN, TroughError, facing_trough};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

fn res() -> Resolution {
    Resolution::new(mm(1e-7)).unwrap()
}

const O: Point3 = Point3::new(30.0, 20.0, 0.0);
const LO: Point3 = Point3::new(-10.0, 22.0, 0.0);
const HI: Point3 = Point3::new(60.0, 28.0, 4.0);

fn upright(radius: f64) -> Cylinder {
    // The pin's: axis (0, 0, 2) through O (any length, so a unit here).
    Cylinder::new(
        Frame::new(O, Vec3::Z, Vec3::X).unwrap(),
        mm(radius),
        Sense::Forward,
    )
    .unwrap()
}

fn the_box() -> Extent3d {
    Extent3d::from_corners(LO, HI).unwrap()
}

#[test]
fn a_pick_facing_the_travel_runs_its_wings_back() {
    // The pick at 45 degrees on the +x, +y quarter, the travel +x: the line leaves the
    // cylinder there, so the trough is the +x half, its wings back along -x.
    let near = O + Vec3::new(10.0, 10.0, 0.0) * (1.0 / 2.0_f64.sqrt());
    let t = facing_trough(
        &upright(10.0),
        near,
        Vec3::new(3.0, 0.0, 0.0),
        &the_box(),
        res(),
    )
    .unwrap();
    assert_eq!(*t.frame().x(), Vec3::X);
    assert_eq!(*t.frame().y(), Vec3::Y);
    assert_eq!(t.half(), Half::Facing);
    assert_eq!(t.sweep(), -PI);
    assert!(t.wing_top().x < 0.0 && t.wing_bottom().x < 0.0);
    assert_eq!(t.top(), Point2::new(0.0, 10.0));
    assert_eq!(t.bottom(), Point2::new(0.0, -10.0));
    // The farthest box corner from the axis point, (-10, 28, 4), plus the radius, and the
    // margin.
    let far = Point3::new(-10.0, 28.0, 4.0).distance(O);
    assert!((t.reach().mm() - (far + 10.0) * (1.0 + MARGIN)).abs() < 1e-12);
    assert_eq!(t.wing_top(), Point2::new(-t.reach().mm(), 10.0));
}

#[test]
fn coming_from_the_far_side_the_wings_run_on_ahead() {
    // The same pick, the travel -x: the line goes in there, so the wings run on ahead along
    // the travel, the frame's +x.
    let near = O + Vec3::new(10.0, 10.0, 0.0) * (1.0 / 2.0_f64.sqrt());
    let t = facing_trough(
        &upright(10.0),
        near,
        Vec3::new(-1.0, 0.0, 0.0),
        &the_box(),
        res(),
    )
    .unwrap();
    assert_eq!(*t.frame().x(), -Vec3::X);
    assert_eq!(t.half(), Half::Away);
    assert_eq!(t.sweep(), PI);
    assert!(t.wing_top().x > 0.0 && t.wing_bottom().x > 0.0);
    // The frame is right handed about the axis: x cross y is the axis.
    assert!((t.frame().x().cross(*t.frame().y()) - Vec3::Z).length() < 1e-15);
}

#[test]
fn no_one_half_is_a_typed_cause() {
    let c = upright(10.0);
    let near = O + Vec3::new(0.0, 10.0, 0.0);
    // Along the axis.
    assert_eq!(
        facing_trough(&c, near, Vec3::Z * 2.0, &the_box(), res()),
        Err(TroughError::AlongAxis)
    );
    // On the edge as seen along the travel.
    assert!(matches!(
        facing_trough(&c, near, Vec3::X, &the_box(), res()),
        Err(TroughError::OnEdge { .. })
    ));
    // On the axis: the same plane, the same cause.
    assert!(matches!(
        facing_trough(&c, O, Vec3::X, &the_box(), res()),
        Err(TroughError::OnEdge { .. })
    ));
    // Numbers that are not finite.
    let pick = O + Vec3::new(10.0, 0.0, 0.0);
    assert_eq!(
        facing_trough(
            &c,
            Point3::new(f64::NAN, 0.0, 0.0),
            Vec3::X,
            &the_box(),
            res()
        ),
        Err(TroughError::NotFinite)
    );
    assert_eq!(
        facing_trough(
            &c,
            pick,
            Vec3::new(f64::INFINITY, 0.0, 0.0),
            &the_box(),
            res()
        ),
        Err(TroughError::NotFinite)
    );
}

fn tilted(radius: f64) -> Cylinder {
    let f = Frame::new(
        Point3::new(1.5, -2.0, 0.7),
        Vec3::new(0.3, -0.4, 0.86),
        Vec3::new(1.0, 0.2, 0.0),
    )
    .unwrap();
    Cylinder::new(f, mm(radius), Sense::Forward).unwrap()
}

#[test]
fn the_half_is_the_one_whose_normal_faces_the_travel() {
    let c = tilted(3.0);
    let f = *c.frame();
    let over =
        Extent3d::from_corners(Point3::new(-9.0, -9.0, -9.0), Point3::new(9.0, 9.0, 9.0)).unwrap();
    // A travel oblique to the axis: only its part across the axis counts.
    let travel = Vec3::new(0.7, 1.1, -0.4);
    let mut facing = 0;
    for i in 0..72 {
        let a = (f64::from(i) + 0.37) * TAU / 72.0;
        let out = *f.x() * a.cos() + *f.y() * a.sin();
        let near = f.origin() + out * 3.0 + *f.z() * 1.3;
        let t = facing_trough(&c, near, travel, &over, res()).unwrap();
        // The outward normal at the pick against the travel.
        let leaves = out.dot(travel) > 0.0;
        assert_eq!(t.half() == Half::Facing, leaves, "angle {a}");
        facing += usize::from(leaves);
        // The frame: origin on the axis, x across it along the travel, x cross y the axis.
        assert_eq!(t.frame().origin(), f.origin());
        assert!(t.frame().x().dot(*f.z()).abs() < 1e-15);
        assert!((t.frame().x().cross(*t.frame().y()) - *f.z()).length() < 1e-14);
        let across = travel - *f.z() * travel.dot(*f.z());
        assert!((*t.frame().x() - across * (1.0 / across.length())).length() < 1e-14);
    }
    // Half the ring leaves, half goes in.
    assert_eq!(facing, 36);
}

#[test]
fn the_reach_passes_every_corner_of_the_box_and_the_arc() {
    let c = tilted(3.0);
    let f = *c.frame();
    let over =
        Extent3d::from_corners(Point3::new(-4.0, 1.0, -2.0), Point3::new(8.0, 5.0, 6.0)).unwrap();
    let near = f.origin() + *f.x() * 3.0;
    let t = facing_trough(&c, near, *f.x(), &over, res()).unwrap();
    let mut far = 0.0_f64;
    for i in 0..8 {
        let p = Point3::new(
            if i & 1 == 0 { -4.0 } else { 8.0 },
            if i & 2 == 0 { 1.0 } else { 5.0 },
            if i & 4 == 0 { -2.0 } else { 6.0 },
        );
        far = far.max(p.distance(f.origin()));
    }
    assert!((t.reach().mm() - (far + 3.0) * (1.0 + MARGIN)).abs() < 1e-12);
    // The box's own corners are inside the extrusion's reach with room to spare.
    assert!(t.reach().mm() > far + 3.0);
    assert_eq!(t.radius().mm(), 3.0);
}

#[test]
fn a_pick_a_resolution_from_the_edge_is_ambiguous_and_one_beyond_it_is_not() {
    let c = upright(10.0);
    let on = |dx: f64| O + Vec3::new(dx, 10.0, 0.0);
    // The travel +x: the pick's offset along the travel is its dx.
    assert!(matches!(
        facing_trough(&c, on(5e-8), Vec3::X, &the_box(), res()),
        Err(TroughError::OnEdge { .. })
    ));
    assert!(matches!(
        facing_trough(&c, on(-9e-8), Vec3::X, &the_box(), res()),
        Err(TroughError::OnEdge { .. })
    ));
    assert_eq!(
        facing_trough(&c, on(2e-7), Vec3::X, &the_box(), res())
            .unwrap()
            .half(),
        Half::Facing
    );
    assert_eq!(
        facing_trough(&c, on(-2e-7), Vec3::X, &the_box(), res())
            .unwrap()
            .half(),
        Half::Away
    );
}

#[test]
fn a_run_twice_is_the_same_to_the_bit() {
    let c = tilted(3.0);
    let f = *c.frame();
    let over =
        Extent3d::from_corners(Point3::new(-4.0, 1.0, -2.0), Point3::new(8.0, 5.0, 6.0)).unwrap();
    let near = f.origin() + *f.y() * 3.0;
    let run = || facing_trough(&c, near, Vec3::new(0.2, 0.9, 0.1), &over, res()).unwrap();
    assert_eq!(run(), run());
}
