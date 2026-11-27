// Written for solidcorex. Apache-2.0.
//! Faces far thinner than they are wide: the pin's quarter shells (radius 1e4 and 10,
//! thickness down to 1e-6), whose annular caps' loops lie within a thickness of each
//! other. Every face's region must still be proved, so it has a box and a point test.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::f64::consts::FRAC_1_SQRT_2;

use scx_brep::{Brep, Document, UvSide};
use scx_num::vector2d::Point2;
use scx_num::vector3d::Point3;
use scx_surface::Surface;
use scx_testkit::bodies::{build_resolution, quarter_shell};

fn shell(r: f64, t: f64) -> Brep {
    let fine = Document::with_resolution(build_resolution());
    quarter_shell(r, t, 1.0)
        .unwrap()
        .finish(&fine)
        .map_err(|u| u.cause)
        .unwrap()
        .0
}

#[test]
fn every_face_of_a_thin_shell_has_a_box_holding_it() {
    for (r, t) in [(1e4, 1e-3), (10.0, 1e-4), (10.0, 1e-6)] {
        let b = shell(r, t);
        for f in b.faces() {
            let bx = f
                .bounding_box()
                .unwrap_or_else(|| panic!("({r}, {t}) face {} has no box", f.id()));
            // Every corner of the shell the face touches is inside its box.
            for e in f.uses().map(scx_brep::Use::edge) {
                let eb = e.bounding_box().unwrap();
                assert!(bx.intersect(eb).is_some(), "({r}, {t}) face {}", f.id());
            }
        }
    }
}

#[test]
fn a_thin_cap_tells_its_middle_from_inside_its_inner_arc() {
    for (r, t) in [(1e4, 1e-3), (10.0, 1e-4), (10.0, 1e-6)] {
        let b = shell(r, t);
        let caps: Vec<_> = b
            .faces()
            .filter(|f| matches!(f.surface(), Surface::Plane(p) if p.frame().z().z.abs() == 1.0))
            .collect();
        assert_eq!(caps.len(), 2, "({r}, {t})");
        for f in caps {
            let Surface::Plane(p) = f.surface() else {
                unreachable!()
            };
            let z = p.frame().origin().z;
            let uv = |x: f64, y: f64| {
                let d = Point3::new(x, y, z) - p.frame().origin();
                Point2::new(d.dot(*p.frame().x()), d.dot(*p.frame().y()))
            };
            let c = FRAC_1_SQRT_2;
            let mid = r + t / 2.0;
            assert_eq!(
                f.contains_uv(uv(mid * c, mid * c)),
                Some(UvSide::Inside),
                "({r}, {t}) face {}",
                f.id()
            );
            assert_eq!(
                f.contains_uv(uv(r / 2.0 * c, r / 2.0 * c)),
                Some(UvSide::Outside),
                "({r}, {t}) face {}",
                f.id()
            );
        }
    }
}
