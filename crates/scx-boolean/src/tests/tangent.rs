// Written for solidcorex. Apache-2.0.
//! Tangent contacts through the whole Boolean (BO22, 75 §13.13): a line or circle where
//! the operands' faces touch with one normal and part again. Each case is judged against
//! its closed-form volume, in all four operations (the fourth with the operands swapped).
//!
//! The rule is D5's: a touch along a line or circle that ends on edges of the faces on
//! both sides is an edge of four fins, built and ordered by bend ([`crate::pinch`]); a
//! ruling or circle that ends inside a face (short of its rims) is `NonManifold` where both
//! sides of it are kept, and a point touch inside a face is a point loop (BO25). An
//! operation that keeps one side of the touch builds. The point touches of a sphere against
//! a cylinder or torus off its axis are in [`super::contacts`]'s point contacts.

use std::f64::consts::PI;

use scx_brep::{Brep, Document};

use super::contacts::{non_manifold, result};
use super::cuboid;
use super::face_face::{X, Z, ball, cyl};
use super::whole::{INTERSECT, SUBTRACT, UNION, volume};
use crate::BooleanOp;

const Y: [f64; 3] = [0.0, 1.0, 0.0];

/// What one operation answers: a body of this volume (zero is empty), the number of edges
/// of four fins on it, or `NonManifold`.
#[derive(Clone, Copy, Debug)]
enum Want {
    Built { volume: f64, pinched: usize },
    NonManifold,
}

use Want::{Built, NonManifold};

const fn built(volume: f64, pinched: usize) -> Want {
    Built { volume, pinched }
}

/// `a ∪ b`, `a − b`, `a ∩ b` and `b − a`, each as `want`.
fn case(name: &str, a: &Brep, b: &Brep, want: [Want; 4], doc: Document) {
    let ops: [(BooleanOp, bool); 4] = [
        (UNION, false),
        (SUBTRACT, false),
        (INTERSECT, false),
        (SUBTRACT, true),
    ];
    for ((which, swapped), w) in ops.into_iter().zip(want) {
        let (x, y) = if swapped { (b, a) } else { (a, b) };
        let tag = format!("{name} {which:?} swapped {swapped}");
        match w {
            NonManifold => non_manifold(x, y, which, doc, &tag),
            Built { volume: v, pinched } => {
                let body = result(x, y, which, doc);
                let fours = body.edges().filter(|e| e.fins().len() == 4).count();
                assert_eq!(fours, pinched, "{tag}: edges of four fins");
                if v == 0.0 {
                    assert_eq!(body.faces().count(), 0, "{tag}: empty");
                } else {
                    let got = volume(&body);
                    assert!((got - v).abs() <= 1e-9 * v, "{tag}: {got} against {v}");
                }
            }
        }
    }
}

/// The box [0, 10]^3, and a z = 2 cylinder of length 6 along y.
fn box_and_cylinder_volumes() -> (f64, f64) {
    (1000.0, PI * 4.0 * 6.0)
}

#[test]
fn a_cylinder_lying_on_a_box_touches_its_top_along_a_ruling_inside_it() {
    // The ruling runs from y = 2 to 8 on the top face z = 10: its ends are on the
    // cylinder's cap rims but inside the top face, so the union meets itself there and is
    // `NonManifold` (before BO22 it was built as two lumps that share no edge, the touch
    // lost). The other operations keep one side.
    let doc = Document::default();
    let (vb, vc) = box_and_cylinder_volumes();
    let bx = cuboid([0.0; 3], [10.0; 3], doc);
    let c = cyl([2.0, 5.0, 12.0], X, Y, 2.0, 6.0, doc);
    let want = [NonManifold, built(vb, 0), built(0.0, 0), built(vc, 0)];
    case("cylinder on top", &bx, &c, want, doc);
}

#[test]
fn a_cylinder_inside_a_box_touching_its_top_is_non_manifold_only_as_a_cavity() {
    // The same ruling from inside: the box less the cylinder is a cavity meeting the outer
    // boundary along it.
    let doc = Document::default();
    let (vb, vc) = box_and_cylinder_volumes();
    let bx = cuboid([0.0; 3], [10.0; 3], doc);
    let c = cyl([2.0, 5.0, 8.0], X, Y, 2.0, 6.0, doc);
    let want = [built(vb, 0), NonManifold, built(vc, 0), built(0.0, 0)];
    case("cylinder under top", &bx, &c, want, doc);
}

#[test]
fn a_sphere_in_a_cylinder_of_its_radius_touches_the_wall_along_a_circle() {
    // The equator, a whole circle on the wall: the cylinder less the sphere is two pieces
    // meeting along it, an edge of four fins (the wall's two halves and the sphere's).
    let doc = Document::default();
    let cy = cyl([0.0, 0.0, -5.0], Z, X, 2.0, 10.0, doc);
    let s = ball([0.0; 3], 2.0, doc);
    let (vcy, vs) = (PI * 4.0 * 10.0, 4.0 / 3.0 * PI * 8.0);
    let want = [
        built(vcy, 0),
        built(vcy - vs, 1),
        built(vs, 0),
        built(0.0, 0),
    ];
    case("sphere in cylinder", &cy, &s, want, doc);
}

#[test]
fn parallel_cylinders_touching_along_a_whole_ruling_unite_at_one_edge() {
    // Equal and level, so the tangent ruling runs cap rim to cap rim on both; on the left
    // one it is the wall's seam, and either difference cuts and rejoins that wall there.
    let doc = Document::default();
    let v = PI * 4.0 * 10.0;
    let l = cyl([0.0; 3], Z, X, 2.0, 10.0, doc);
    let r = cyl([4.0, 0.0, 0.0], Z, X, 2.0, 10.0, doc);
    let want = [built(2.0 * v, 1), built(v, 0), built(0.0, 0), built(v, 0)];
    case("level cylinders", &l, &r, want, doc);
}

#[test]
fn parallel_cylinders_touching_along_part_of_a_ruling_unite_non_manifold() {
    // Offset by 2.5 along the axis: the touch ends inside each wall, a contact D5 refuses
    // in the union; the other operations keep one side and build. The ruling is the left
    // wall's seam, so it reaches the right wall as an edge lying in it, not a section.
    let doc = Document::default();
    let v = PI * 4.0 * 10.0;
    let l = cyl([0.0; 3], Z, X, 2.0, 10.0, doc);
    let r = cyl([4.0, 0.0, 2.5], Z, X, 2.0, 10.0, doc);
    let want = [NonManifold, built(v, 0), built(0.0, 0), built(v, 0)];
    case("offset cylinders", &l, &r, want, doc);
}

#[test]
fn a_cylinder_inside_a_bigger_one_touches_its_wall_from_within() {
    // r = 2 in r = 3, the axes 1 apart: the larger less the smaller is a wall meeting
    // itself along the ruling, bending apart (curvatures 1/3 and 1/2 on one side).
    let doc = Document::default();
    let (vbig, vc) = (PI * 9.0 * 10.0, PI * 4.0 * 6.0);
    let big = cyl([0.0; 3], Z, X, 3.0, 10.0, doc);
    let small = cyl([1.0, 0.0, 2.0], Z, X, 2.0, 6.0, doc);
    let want = [
        built(vbig, 0),
        built(vbig - vc, 1),
        built(vc, 0),
        built(0.0, 0),
    ];
    case("cylinder in cylinder", &big, &small, want, doc);
}

#[test]
fn a_ball_inside_a_box_touching_its_top_leaves_a_cavity_meeting_it_at_a_point() {
    // A point touch inside the top face: the box less the ball meets itself there, a point
    // loop on each face (BO25), with no edge of four fins.
    let doc = Document::default();
    let bx = cuboid([0.0; 3], [10.0; 3], doc);
    let b = ball([5.0, 5.0, 7.0], 3.0, doc);
    let vb = 4.0 / 3.0 * PI * 27.0;
    let want = [
        built(1000.0, 0),
        built(1000.0 - vb, 0),
        built(vb, 0),
        built(0.0, 0),
    ];
    case("ball under top", &bx, &b, want, doc);
}
