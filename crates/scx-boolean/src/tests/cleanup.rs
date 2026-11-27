// Written for solidcorex. Apache-2.0.
//! Cells left where they need not be (BO8, 72 §14.42): a body against an exact copy of
//! itself whose closed edges share their vertex, jog cells on merged near-coplanar fronts,
//! and the work of operands that do not cross.

use scx_brep::Document;
use scx_num::budget::{Budget, Stop};
use scx_num::units::Length;
use scx_testkit::mesh::{Limits, Mesh, Violation, violations};

use super::cuboid;
use super::face_face::{X, Z, cyl};
use super::whole::{INTERSECT, SUBTRACT, UNION, checked, crease, near, op, volume};

#[test]
fn a_cylinder_less_itself_is_empty_and_with_itself_is_itself() {
    // Each circle closes at the seam's vertex, so the edges of one copy lie along the other's
    // from that vertex round to itself: every face is `On` alone, as for a block.
    let doc = Document::default();
    let a = cyl([0.0; 3], Z, X, 2.0, 10.0, doc);
    let b = cyl([0.0; 3], Z, X, 2.0, 10.0, doc);
    let less = op(&a, &b, SUBTRACT, doc).unwrap();
    assert_eq!(less.body.faces().count(), 0);
    assert!(less.body.regions().all(|r| !r.is_solid()));
    for which in [UNION, INTERSECT] {
        let d = checked(&a, &b, which, doc);
        assert_eq!(d.body.faces().count(), 3, "{which:?}");
        assert_eq!(d.body.edges().count(), 3, "{which:?}");
        assert_eq!(d.body.vertices().count(), 2, "{which:?}");
        near(volume(&d.body), core::f64::consts::PI * 40.0, 1e-9);
    }
}

#[test]
fn near_coplanar_fronts_merge_without_their_jog() {
    // A 10^3 cube and the block at x = 5 moved `dy` along y: within coincidence the fronts
    // are one face, and the edges either side of the step's jog are one line, so the jog's
    // vertices go with it: the 15 x 10 x 10 block's cells. Past it the step stays.
    for (dy, cells) in [
        (0.0, [6, 12, 8]),
        (1e-8, [6, 12, 8]),
        (1e-6, [6, 12, 8]),
        (5e-6, [6, 12, 8]),
        (4e-5, [10, 24, 16]),
    ] {
        let doc = Document::default();
        let a = cuboid([0.0; 3], [10.0; 3], doc);
        let b = cuboid([5.0, dy, 0.0], [15.0, 10.0 + dy, 10.0], doc);
        let d = checked(&a, &b, UNION, doc);
        let got = [
            d.body.faces().count(),
            d.body.edges().count(),
            d.body.vertices().count(),
        ];
        assert_eq!(got, cells, "dy {dy}");
        let v = volume(&d.body);
        assert!(
            (v - 1500.0 - 50.0 * dy).abs() <= 1e-9 * 1500.0 + 100.0 * dy,
            "dy {dy}: {v}"
        );
    }
}

#[test]
fn a_crease_inside_the_resolution_of_an_edge_pinches_at_one_vertex() {
    // The overlap is a tetrahedron about 3e-6 across, all its corners within the resolution
    // of one point: the union is the two blocks meeting at that point (D5), and the mesh's
    // one non-manifold vertex is there, not a flaw of the build.
    let doc = Document::default();
    let a = cuboid([0.0; 3], [10.0; 3], doc);
    let b = crease(3e-6, doc);
    let d = checked(&a, &b, UNION, doc);
    let body = &d.body;
    assert_eq!(body.regions().filter(|r| r.is_solid()).count(), 2);
    near(volume(body), 3000.0, 1e-9);
    let pinch: Vec<_> = body.vertices().filter(|v| v.edges().count() == 4).collect();
    assert_eq!(pinch.len(), 1);
    let at = pinch[0].point();
    let spec = scx_tess::TessSpec::new(Length::from_mm(0.1), body.resolution()).unwrap();
    let tess = scx_tess::tessellate(body, &spec, &mut Budget::unlimited(), &Stop::new()).unwrap();
    let mesh = Mesh {
        vertices: tess.positions.iter().map(|q| [q.x, q.y, q.z]).collect(),
        triangles: tess.triangles.clone(),
    };
    let found = violations(&mesh, Limits::default());
    assert!(!found.is_empty());
    for v in found {
        let Violation::NonManifoldVertex { vertex } = v else {
            panic!("{v:?}");
        };
        assert!(
            tess.positions[vertex as usize].distance(at) <= 1e-5,
            "{v:?}"
        );
    }
}

#[test]
fn shells_that_are_operand_shells_whole_cost_no_volume_signs() {
    // Each set of faces here is one operand shell, uncut: its sign is the operand's, so
    // nesting measures none and the work is the stages' own, below a crossing pair's.
    let doc = Document::default();
    let cube = cuboid([0.0; 3], [10.0; 3], doc);
    let crossing = op(&cube, &cuboid([5.0; 3], [15.0; 3], doc), UNION, doc).unwrap();
    let apart = checked(
        &cube,
        &cuboid([20.0, 0.0, 0.0], [30.0, 10.0, 10.0], doc),
        UNION,
        doc,
    );
    let cavity = checked(&cube, &cuboid([3.0; 3], [7.0; 3], doc), SUBTRACT, doc);
    for (d, v) in [(&apart, 2000.0), (&cavity, 1000.0 - 64.0)] {
        assert!(d.report.work < crossing.report.work, "{}", d.report.work);
        near(volume(&d.body), v, 1e-9);
        assert_eq!(
            d.body.regions().filter(|r| r.is_solid()).count(),
            1 + usize::from(v > 1999.0)
        );
    }
    // A cavity in an operand leaves its region two shells: those signs are measured, and the
    // cavity stays in the lump that holds it.
    let holed = cavity.body;
    let d = checked(
        &holed,
        &cuboid([20.0, 0.0, 0.0], [30.0, 10.0, 10.0], doc),
        UNION,
        doc,
    );
    near(volume(&d.body), 2000.0 - 64.0, 1e-9);
    assert_eq!(d.body.regions().filter(|r| r.is_solid()).count(), 2);
    let voids = d
        .body
        .regions()
        .filter(|r| !r.is_solid() && !r.is_outside());
    assert_eq!(voids.count(), 1);
}
