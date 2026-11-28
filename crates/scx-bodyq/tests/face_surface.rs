// Written for solidcorex. Apache-2.0.
//! A face's surface for callers (research 74 §34, BW11): its kind, its point and normal at a
//! uv, the nearest point of it, and the quadric a NURBS face is. The pin's
//! `face_surface_eval` tests ran blends and balls that do not exist here; these are the
//! closed forms of a block, a cylinder and a flat NURBS patch.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod along_common;

use along_common::{doc, nurbs_meets_plane};
use scx_bodyq::{BodyqError, BodyqRefusal, face_surface};
use scx_brep::FaceId;
use scx_num::budget::{Budget, Stop};
use scx_num::frame::Frame;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::{Surface, SurfaceKind};
use scx_testkit::bodies::made::{block, cylinder};

fn mm(x: f64) -> Length {
    Length::from_mm(x)
}

#[test]
fn a_blocks_top_is_a_plane_with_its_normal_and_nearest_point() {
    let m = block(&Frame::WORLD, [mm(10.0), mm(6.0), mm(4.0)], &doc()).unwrap();
    let f = face_surface(&m.body, m.faces.top).unwrap();
    assert_eq!(f.kind(), SurfaceKind::Plane);
    assert!(matches!(f.surface(), Surface::Plane(_)));
    let d = f.domain();
    assert!(
        d.x_max() - d.x_min() >= 10.0 - 1e-9 && d.y_max() - d.y_min() >= 6.0 - 1e-9,
        "{d:?}"
    );
    let uv = d.mid();
    let p = f.point(uv).unwrap();
    assert!((p.z - 4.0).abs() < 1e-12, "{p:?}");
    let n = f.normal(uv).unwrap().unwrap();
    assert!((n.z.abs() - 1.0).abs() < 1e-15, "{n:?}");
    let c = f
        .closest_point(
            Point3::new(3.0, 2.0, 9.0),
            None,
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
    assert!(
        (c.distance.mm() - 5.0).abs() < 1e-12 && (c.point.z - 4.0).abs() < 1e-12,
        "{c:?}"
    );
    // A plane is its own recognition, deviation 0.
    let r = f
        .recognized(Accuracy::ZERO, &mut Budget::unlimited(), &Stop::new())
        .unwrap()
        .unwrap();
    assert_eq!(r.deviation.mm(), 0.0);
    // Not finite and no such face.
    assert_eq!(
        f.point(Point2::new(f64::NAN, 0.0)),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    assert!(face_surface(&m.body, FaceId::from_raw(99)).is_err());
}

#[test]
fn a_cylinders_wall_has_its_radius_normal_and_nearest_point() {
    let m = cylinder(&Frame::WORLD, mm(3.0), mm(7.0), &doc()).unwrap();
    let f = face_surface(&m.body, m.faces.wall).unwrap();
    assert_eq!(f.kind(), SurfaceKind::Cylinder);
    let c = f
        .closest_point(
            Point3::new(20.0, 0.0, 5.0),
            None,
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
    assert!((c.distance.mm() - 17.0).abs() < 1e-12, "{c:?}");
    assert!(
        (c.point - Point3::new(3.0, 0.0, 5.0)).length() < 1e-12,
        "{c:?}"
    );
    // The point at the uv of the foot is the foot, and the normal there is radial.
    let p = f.point(c.uv).unwrap();
    assert!((p - c.point).length() < 1e-12);
    let n = f.normal(c.uv).unwrap().unwrap();
    assert!(
        (n.dot(Vec3::new(1.0, 0.0, 0.0)).abs() - 1.0).abs() < 1e-12,
        "{n:?}"
    );
}

#[test]
fn a_nurbs_patch_is_recognised_as_the_plane_it_is() {
    let (sheet, _, patch) = nurbs_meets_plane();
    let f = face_surface(&sheet, patch).unwrap();
    assert_eq!(f.kind(), SurfaceKind::Nurbs);
    let r = f
        .recognized(
            mm(1e-6).into_accuracy(),
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
    // The patch is flat: the certificate finds the plane within the accuracy.
    let r = r.expect("a plane");
    assert!(matches!(r.surface, Surface::Plane(_)), "{r:?}");
    assert!(r.deviation.mm() <= 1e-6);
    let c = f
        .closest_point(
            Point3::new(2.0, -1.0, 4.0),
            None,
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .unwrap();
    // The patch is the plane y + z = 0 over x in 0..4 and y in -2..0: the foot of (2, -1, 4)
    // on the plane is (2, -2.5, 2.5), past the patch's edge at y = -2: the nearest point of the
    // face is on that edge, (2, -2, 2).
    assert!(
        (c.point - Point3::new(2.0, -2.0, 2.0)).length() < 1e-9,
        "{c:?}"
    );
}

trait IntoAccuracy {
    fn into_accuracy(self) -> Accuracy;
}

impl IntoAccuracy for Length {
    fn into_accuracy(self) -> Accuracy {
        Accuracy::new(self).unwrap()
    }
}
