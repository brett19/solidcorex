// Written for solidcorex. Apache-2.0.
//! BQ2's tests: each case's answer is worked by hand from the closed forms (the normals,
//! the curvatures `1/r` and the second-order height `d(s) = a s + b s^2`), in both operand
//! orders, and for each face in both senses.
//!
//! The senses follow from the method: reversing `f` turns `w = n_f x T` round, so its left
//! and right swap; reversing `g` swaps its front and back. [`each_sense`] checks all four
//! from the forward answer.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use core::f64::consts::TAU;

use scx_brep::{Brep, Document, FaceSide, RegionId};
use scx_curve::{Circle, Curve, Line};
use scx_make::{block, sphere};
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector3d::{Point3, Vec3};
use scx_ssi::analytic::Contact;
use scx_surface::{Cylinder, Plane, Sphere, Surface};

use super::{Along, Sheet, side_at, sides_on};
use crate::error::{BodyqError, BodyqRefusal, BodyqUnsupported};
use crate::{side_at_touch, sides_along};

use FaceSide::{Back, Front};
use Sense::{Forward, Reversed};

const C: f64 = 1e-5;

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

fn v(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}

fn frame(o: [f64; 3], z: [f64; 3], x: [f64; 3]) -> Frame {
    Frame::new(pt(o), v(z), v(x)).unwrap()
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// The plane through `o` with natural normal `n` (or `-n`, reversed).
fn plane(o: [f64; 3], n: [f64; 3], x: [f64; 3], sense: Sense) -> Surface {
    let n = match sense {
        Forward => n,
        Reversed => [-n[0], -n[1], -n[2]],
    };
    Plane::new(frame(o, n, x)).into()
}

/// The cylinder of `r` about the line through `o` along `axis`: forward, its normal points
/// out.
fn cylinder(o: [f64; 3], axis: [f64; 3], x: [f64; 3], r: f64, sense: Sense) -> Surface {
    Cylinder::new(frame(o, axis, x), Length::from_mm(r), sense)
        .unwrap()
        .into()
}

/// The sphere of `r` about `o`, its poles on z: forward, its normal points out.
fn ball(o: [f64; 3], r: f64, sense: Sense) -> Surface {
    ball_on(o, Z, r, sense)
}

/// The sphere of `r` about `o`, its poles on `axis`.
fn ball_on(o: [f64; 3], axis: [f64; 3], r: f64, sense: Sense) -> Surface {
    let x = if axis == X { Y } else { X };
    Sphere::new(frame(o, axis, x), Length::from_mm(r), sense)
        .unwrap()
        .into()
}

fn line(o: [f64; 3], d: [f64; 3]) -> Curve {
    Line::new(pt(o), v(d)).unwrap().into()
}

fn along(curve: &Curve, span: [f64; 2], contact: Contact) -> Along<'_> {
    Along {
        curve,
        span: Extent1d::new(span[0], span[1]).unwrap(),
        contact,
        coincidence: Length::from_mm(C),
    }
}

fn sides(f: &Surface, g: &Surface, along: &Along<'_>) -> Option<[FaceSide; 2]> {
    sides_on(
        &Sheet::whole(f),
        &Sheet::whole(g),
        along,
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

fn touch(f: &Surface, g: &Surface, at: [f64; 3]) -> Option<FaceSide> {
    side_at(
        &Sheet::whole(f),
        &Sheet::whole(g),
        pt(at),
        &mut Budget::unlimited(),
        &Stop::new(),
    )
    .unwrap()
}

/// The answer for `f` in `sf` and `g` in `sg`, from the answer with both forward.
fn sensed(forward: [FaceSide; 2], sf: Sense, sg: Sense) -> [FaceSide; 2] {
    let [l, r] = forward;
    let [l, r] = if sf == Reversed { [r, l] } else { [l, r] };
    if sg == Reversed {
        [l.other(), r.other()]
    } else {
        [l, r]
    }
}

/// Checks `sides` for every pair of senses against the forward answer.
fn each_sense(
    make: impl Fn(Sense, Sense) -> (Surface, Surface),
    along: &Along<'_>,
    forward: [FaceSide; 2],
) {
    for sf in [Forward, Reversed] {
        for sg in [Forward, Reversed] {
            let (f, g) = make(sf, sg);
            assert_eq!(
                sides(&f, &g, along),
                Some(sensed(forward, sf, sg)),
                "senses {sf:?}, {sg:?}"
            );
        }
    }
}

/// As [`each_sense`] at a touch point: reversing `f` changes nothing (the face lies on one
/// side all round), reversing `g` swaps the side.
fn each_sense_touch(
    make: impl Fn(Sense, Sense) -> (Surface, Surface),
    at: [f64; 3],
    forward: FaceSide,
) {
    for sf in [Forward, Reversed] {
        for sg in [Forward, Reversed] {
            let (f, g) = make(sf, sg);
            let want = if sg == Reversed {
                forward.other()
            } else {
                forward
            };
            assert_eq!(touch(&f, &g, at), Some(want), "senses {sf:?}, {sg:?}");
        }
    }
}

// Crossings.

/// `f`: the plane `z = 0`, normal `+z`; `g`: the plane `x = 0`, normal `+x`; they cross
/// along the y axis, run `+y`. From `f`: `w = z x y = -x`, `a = -1`, so its left (`x < 0`)
/// is behind `g`. From `g`: `w = x x y = +z`, `a = +1`, so its left (`z > 0`) is in front
/// of `f`.
#[test]
fn plane_against_plane() {
    let y = line([0.0; 3], Y);
    let on = along(&y, [-1.0, 1.0], Contact::Transversal);
    let floor = |s| plane([0.0; 3], Z, X, s);
    let wall = |s| plane([0.0; 3], X, Y, s);
    each_sense(|sf, sg| (floor(sf), wall(sg)), &on, [Back, Front]);
    each_sense(|sf, sg| (wall(sf), floor(sg)), &on, [Front, Back]);
}

/// `f`: the plane `z = 1`; `g`: the cylinder of radius 2 about the x axis. They cross along
/// the line `y = sqrt 3, z = 1`, run `+x`, where `n_g = (0, sqrt 3, 1) / 2`.
///
/// From `f`: `w = +y`, `a = sqrt 3 / 2 > 0`, so its left (`y` growing) is outside. The
/// height over the cylinder is `sqrt((sqrt 3 + s)^2 + 1) - 2`, whose second-order part is
/// `s^2 / 16`: `b = 1/16`, and the formula gives `-II_g(w_g) / 2` with `|w_g| = 1/2` and
/// `II_g = -1/2` per unit, the same.
///
/// From `g`: `w = n_g x x = (0, 1/2, -sqrt 3 / 2)`, heading down round the circle:
/// `a = -sqrt 3 / 2`, so its left is below the plane.
#[test]
fn plane_against_cylinder() {
    let s3 = 3.0_f64.sqrt();
    let ruling = line([0.0, s3, 1.0], X);
    let on = along(&ruling, [-1.0, 1.0], Contact::Transversal);
    let flat = |s| plane([0.0, 0.0, 1.0], Z, X, s);
    let round = |s| cylinder([0.0; 3], X, Y, 2.0, s);
    each_sense(|sf, sg| (flat(sf), round(sg)), &on, [Front, Back]);
    each_sense(|sf, sg| (round(sf), flat(sg)), &on, [Back, Front]);
}

/// `f`: the plane `z = 0`; `g`: the cylinder of radius 2 about the z axis; they cross along
/// the circle of radius 2 run counterclockwise about `+z`, `T = (-sin t, cos t, 0)`. From
/// `f`: `w = z x T = -(cos t, sin t, 0)`, inward, so its left is inside. From `g`:
/// `w = n_g x T = +z`, so its left is in front of the plane.
#[test]
fn plane_against_cylinder_along_a_circle() {
    let circle: Curve = Circle::new(frame([0.0; 3], Z, X), Length::from_mm(2.0), Forward)
        .unwrap()
        .into();
    let on = along(&circle, [0.0, TAU], Contact::Transversal);
    let flat = |s| plane([0.0; 3], Z, X, s);
    let round = |s| cylinder([0.0; 3], Z, X, 2.0, s);
    each_sense(|sf, sg| (flat(sf), round(sg)), &on, [Back, Front]);
    each_sense(|sf, sg| (round(sf), flat(sg)), &on, [Front, Back]);
}

/// `A`: the cylinder of radius 3 about the z axis; `B`: of radius 2 about the x axis. They
/// cross at `p = (2 sqrt 2, 1, sqrt 3)` (`x^2 + y^2 = 9`, `y^2 + z^2 = 4`), where
/// `n_A = (2 sqrt 2, 1, 0) / 3`, `n_B = (0, 1, sqrt 3) / 2` and `n_A . n_B = 1/6`. The curve
/// runs `T = n_A x n_B`; the piece here is its tangent line over a span short enough to
/// stay within `1e-7` of both.
///
/// From `A`: `w ~ n_A x (n_A x n_B) = n_A (n_A . n_B) - n_B`, `a ~ (n_A . n_B)^2 - 1 < 0`:
/// its left is inside `B`. From `B`: `w ~ n_B x (n_A x n_B) = n_A - n_B (n_A . n_B)`,
/// `a ~ 1 - (n_A . n_B)^2 > 0`: its left is outside `A`.
#[test]
fn cylinder_against_cylinder_crossing() {
    let s3 = 3.0_f64.sqrt();
    let p = [8.0_f64.sqrt(), 1.0, s3];
    let na = v([8.0_f64.sqrt() / 3.0, 1.0 / 3.0, 0.0]);
    let nb = v([0.0, 0.5, s3 / 2.0]);
    let t = na.cross(nb).normalized().unwrap().as_vec();
    let tangent = line(p, [t.x, t.y, t.z]);
    let on = along(&tangent, [-1e-4, 1e-4], Contact::Transversal);
    let a = |s| cylinder([0.0; 3], Z, X, 3.0, s);
    let b = |s| cylinder([0.0; 3], X, Y, 2.0, s);
    each_sense(|sf, sg| (a(sf), b(sg)), &on, [Back, Front]);
    each_sense(|sf, sg| (b(sf), a(sg)), &on, [Front, Back]);
}

/// A crossing shallower than coincidence reads no side (SS4's depth rule): the plane
/// `z = 2 - e` cuts the cylinder of radius 2 about the x axis at `y = sqrt(4 e - e^2)`, at
/// a depth `e` below its top. With `e = 1e-7`, below `C`, `a^2 = e (1 - e/4)` against
/// `4 |b| C ~ C`: no side. With `e = 1e-3` the crossing is clear, and as
/// [`plane_against_cylinder`] its left is outside.
#[test]
fn a_crossing_shallower_than_coincidence_reads_no_side() {
    for (e, want) in [(1e-7_f64, None), (1e-3, Some([Front, Back]))] {
        let y = (4.0 * e - e * e).sqrt();
        let ruling = line([0.0, y, 2.0 - e], X);
        let on = along(&ruling, [-1.0, 1.0], Contact::Transversal);
        let flat = plane([0.0, 0.0, 2.0 - e], Z, X, Forward);
        let round = cylinder([0.0; 3], X, Y, 2.0, Forward);
        assert_eq!(sides(&flat, &round, &on), want, "depth {e}");
    }
}

// Tangencies along a curve.

/// `f`: the plane `z = 0`; `g`: the cylinder of radius 2 about the line `y = 0, z = 2`,
/// along `x`. They touch along the x axis, where `n_g = -z`. From `f`: `w = +y`, `a = 0`,
/// `II_f = 0`, `II_g(y) = -1/2` (out-facing): `b = (0 - (-1/2)) / 2 = 1/4 > 0`, so both
/// sides are outside. From `g`: `w = (-z) x x = -y`, `II_f = -1/2`, `n_f . n_g = -1`,
/// `II_g = 0`: `b = 1/4`, both sides above the plane.
#[test]
fn plane_tangent_to_cylinder() {
    let x = line([0.0; 3], X);
    let on = along(&x, [-1.0, 1.0], Contact::Tangent);
    let flat = |s| plane([0.0; 3], Z, X, s);
    let round = |s| cylinder([0.0, 0.0, 2.0], X, Y, 2.0, s);
    each_sense(|sf, sg| (flat(sf), round(sg)), &on, [Front, Front]);
    each_sense(|sf, sg| (round(sf), flat(sg)), &on, [Front, Front]);
}

/// `A`: radius 1 about the x axis. Outside it, `B`: radius 2 about `y = 0, z = 3`; inside
/// it... the other way, `B'`: radius 2 about `y = 0, z = -1`, holding `A`. Both touch `A`
/// along `y = 0, z = 1`, run `+x`; `n_A = +z`, `II_A = -1`, `II_B = -1/2`.
///
/// - `A` from `B` (`n_B = -z`): `b = ((-1)(-1) + 1/2) / 2 = 3/4`, outside `B`; `B` from
///   `A`: `b = ((-1/2)(-1) + 1) / 2 = 3/4`, outside `A`.
/// - `A` from `B'` (`n_B' = +z`): `b = (-1 + 1/2) / 2 = -1/4`, inside `B'`; `B'` from `A`:
///   `b = (-1/2 + 1) / 2 = 1/4`, outside `A`.
#[test]
fn cylinder_tangent_to_cylinder() {
    let x = line([0.0, 0.0, 1.0], X);
    let on = along(&x, [-1.0, 1.0], Contact::Tangent);
    let a = |s| cylinder([0.0; 3], X, Y, 1.0, s);
    let outer = |s| cylinder([0.0, 0.0, 3.0], X, Y, 2.0, s);
    let holder = |s| cylinder([0.0, 0.0, -1.0], X, Y, 2.0, s);
    each_sense(|sf, sg| (a(sf), outer(sg)), &on, [Front, Front]);
    each_sense(|sf, sg| (outer(sf), a(sg)), &on, [Front, Front]);
    each_sense(|sf, sg| (a(sf), holder(sg)), &on, [Back, Back]);
    each_sense(|sf, sg| (holder(sf), a(sg)), &on, [Front, Front]);
}

/// Two surfaces agreeing to second order across the curve read no side: two planes on
/// one another (`b = 0`), and a cylinder in a cylinder of the same radius rotated about
/// their shared ruling (the second-order terms cancel: `II = -1` on both, `n . n = 1`).
#[test]
fn osculating_surfaces_read_no_side() {
    let x = line([0.0; 3], X);
    let on = along(&x, [-1.0, 1.0], Contact::Tangent);
    let a = plane([0.0; 3], Z, X, Forward);
    let b = plane([0.0; 3], Z, Y, Forward);
    assert_eq!(sides(&a, &b, &on), None);
    let top = line([0.0, 0.0, 1.0], X);
    let on = along(&top, [-1.0, 1.0], Contact::Tangent);
    let c = cylinder([0.0; 3], X, Y, 1.0, Forward);
    let d = cylinder([0.0; 3], X, Z, 1.0, Forward);
    assert_eq!(sides(&c, &d, &on), None);
}

// Touch points.

/// `f`: the plane `z = 0`; `g`: the sphere of radius 2 about `(0, 0, 2)`, touching at the
/// origin with `n_g = -z`. From `f`: `q(t) = (0 - (-1/2)) / 2 = 1/4` for every unit `t`,
/// outside. From `g`: `q = ((-1/2)(-1) - 0) / 2 = 1/4`, above the plane. With the sphere
/// below (about `(0, 0, -2)`, `n_g = +z`): the plane is still outside it, and the sphere
/// is behind the plane, `q = (-1/2) / 2`.
///
/// The touch is at the spheres' pole, where the uv's partials do not span the tangent
/// plane (the form is read a step off it); the same spheres with their poles on x touch at
/// their equator.
#[test]
fn sphere_tangent_to_plane() {
    let flat = |s| plane([0.0; 3], Z, X, s);
    let above = |s| ball_on([0.0, 0.0, 2.0], X, 2.0, s);
    let below = |s| ball_on([0.0, 0.0, -2.0], X, 2.0, s);
    each_sense_touch(|sf, sg| (flat(sf), above(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (above(sf), flat(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (flat(sf), below(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (below(sf), flat(sg)), [0.0; 3], Back);

    let flat = |s| plane([0.0; 3], Z, X, s);
    let above = |s| ball([0.0, 0.0, 2.0], 2.0, s);
    let below = |s| ball([0.0, 0.0, -2.0], 2.0, s);
    each_sense_touch(|sf, sg| (flat(sf), above(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (above(sf), flat(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (flat(sf), below(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (below(sf), flat(sg)), [0.0; 3], Back);
}

/// A sphere of radius 1 about `(0, 0, -1)` inside one of radius 2 about `(0, 0, -2)`,
/// touching at the origin, both normals `+z`: the small one from the big,
/// `q = (-1 + 1/2) / 2 < 0`, inside; the big from the small, `q = (-1/2 + 1) / 2 > 0`,
/// outside.
#[test]
fn sphere_tangent_inside_sphere() {
    let small = |s| ball([0.0, 0.0, -1.0], 1.0, s);
    let big = |s| ball([0.0, 0.0, -2.0], 2.0, s);
    each_sense_touch(|sf, sg| (small(sf), big(sg)), [0.0; 3], Back);
    each_sense_touch(|sf, sg| (big(sf), small(sg)), [0.0; 3], Front);
}

/// Crossed cylinders touching at a point: `A` of radius 1 about `y = 0, z = 1` along `x`,
/// `II_A(t) = -t_y^2`. Below it, `B` of radius 1 about `x = 0, z = -1` along `y`,
/// `II_B(t) = -t_x^2`: `n_A . n_B = -1`, `q = (t_y^2 + t_x^2) / 2`, definite, outside both
/// ways. Above it instead, `B'` of radius 2 about `x = 0, z = 2`: `n . n = 1`,
/// `q = (-t_y^2 + t_x^2 / 2) / 2`, indefinite (they cross there), no side. A plane touching
/// a cylinder along a ruling, asked at a point, is semidefinite: no side.
#[test]
fn touch_points_of_crossed_cylinders() {
    let a = |s| cylinder([0.0, 0.0, 1.0], X, Y, 1.0, s);
    let b = |s| cylinder([0.0, 0.0, -1.0], Y, X, 1.0, s);
    each_sense_touch(|sf, sg| (a(sf), b(sg)), [0.0; 3], Front);
    each_sense_touch(|sf, sg| (b(sf), a(sg)), [0.0; 3], Front);
    let crossing = cylinder([0.0, 0.0, 2.0], Y, X, 2.0, Forward);
    assert_eq!(touch(&a(Forward), &crossing, [0.0; 3]), None);
    assert_eq!(touch(&crossing, &a(Forward), [0.0; 3]), None);
    let flat = plane([0.0; 3], Z, X, Forward);
    assert_eq!(touch(&flat, &a(Forward), [0.0; 3]), None);
}

// The public queries, on bodies.

fn doc() -> Document {
    Document::default()
}

fn solid(body: &Brep) -> RegionId {
    body.regions().find(|r| r.is_solid()).unwrap().id()
}

/// `A`: the block `[0, 2]^3`; `B`: the block `[1, 3] x [-1, 1] x [1, 3]`. `B`'s face
/// `x = 1` (normal `-x`) cuts `A`'s top `z = 2` (normal `+z`) along `x = 1, z = 2`,
/// `y` in `[0, 1]`, run `+y`. From `A`'s top: `w = -x`, `a = (-x) . (-x) = 1`, so its left
/// (`x < 1`) is in front of `B`'s face, outside `B`; its right is in `B`. From `B`'s face:
/// `w = (-x) x y = -z`, `a = -1`, its left (`z < 2`) is inside `A`.
#[test]
fn sides_along_two_blocks() {
    let world = |o: [f64; 3]| frame(o, Z, X);
    let a = block(&world([0.0; 3]), [Length::from_mm(2.0); 3], &doc()).unwrap();
    let b = block(&world([1.0, -1.0, 1.0]), [Length::from_mm(2.0); 3], &doc()).unwrap();
    let top = a.body.face(a.faces.top).unwrap();
    let wall = b.body.face(b.faces.sides[3]).unwrap();
    let cut = line([1.0, 0.0, 2.0], Y);
    let on = along(&cut, [0.0, 1.0], Contact::Transversal);
    let mut budget = Budget::unlimited();
    let got = sides_along(top, wall, &on, &mut budget, &Stop::new()).unwrap();
    assert_eq!(got.left, RegionId::OUTSIDE);
    assert_eq!(got.right, solid(&b.body));
    // The first sample decides: one inversion onto each face.
    assert_eq!(budget.used(), 2);
    let got = sides_along(wall, top, &on, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(got.left, solid(&a.body));
    assert_eq!(got.right, RegionId::OUTSIDE);
    // The same inputs, the same answer.
    let again = sides_along(wall, top, &on, &mut Budget::unlimited(), &Stop::new()).unwrap();
    assert_eq!(again, got);
}

/// A ball of radius 1 about `(1, 1, 3)` on the block `[0, 2]^3`, touching its top at
/// `(1, 1, 2)`: the top is outside the ball, the ball is outside the block.
#[test]
fn side_at_touch_of_a_ball_on_a_block() {
    let a = block(&frame([0.0; 3], Z, X), [Length::from_mm(2.0); 3], &doc()).unwrap();
    let s = sphere(&frame([1.0, 1.0, 3.0], Z, X), Length::from_mm(1.0), &doc()).unwrap();
    let top = a.body.face(a.faces.top).unwrap();
    let round = s.body.face(s.faces.face).unwrap();
    let at = pt([1.0, 1.0, 2.0]);
    let (mut budget, stop) = (Budget::unlimited(), Stop::new());
    assert_eq!(
        side_at_touch(top, round, at, &mut budget, &stop).unwrap(),
        RegionId::OUTSIDE
    );
    assert_eq!(budget.used(), 2);
    assert_eq!(
        side_at_touch(round, top, at, &mut budget, &stop).unwrap(),
        RegionId::OUTSIDE
    );
}

#[test]
fn errors() {
    let a = block(&frame([0.0; 3], Z, X), [Length::from_mm(2.0); 3], &doc()).unwrap();
    let b = block(
        &frame([0.0, 0.0, 2.0], Z, X),
        [Length::from_mm(2.0); 3],
        &doc(),
    )
    .unwrap();
    let top = a.body.face(a.faces.top).unwrap();
    let bottom = b.body.face(b.faces.bottom).unwrap();
    // Stacked blocks: the top and the bottom on one another read no side.
    let edge = line([0.0, 0.0, 2.0], X);
    let on = along(&edge, [0.0, 2.0], Contact::Tangent);
    let err = sides_along(top, bottom, &on, &mut Budget::unlimited(), &Stop::new()).unwrap_err();
    assert_eq!(
        err,
        BodyqError::Unsupported(BodyqUnsupported::NoClearSide {
            face: top.id(),
            other: bottom.id(),
        })
    );
    assert_eq!(err.name(), "no_clear_side");
    let at = pt([1.0, 1.0, 2.0]);
    assert!(matches!(
        side_at_touch(top, bottom, at, &mut Budget::unlimited(), &Stop::new()),
        Err(BodyqError::Unsupported(
            BodyqUnsupported::NoClearSide { .. }
        ))
    ));
    let nan = pt([f64::NAN, 1.0, 2.0]);
    assert_eq!(
        side_at_touch(top, bottom, nan, &mut Budget::unlimited(), &Stop::new()),
        Err(BodyqError::Refused(BodyqRefusal::NotFinite))
    );
    // A budget of one stops at the second inversion.
    let wall = b.body.face(b.faces.sides[0]).unwrap();
    let cross = line([0.0, 0.0, 2.0], X);
    let on = along(&cross, [0.0, 2.0], Contact::Transversal);
    assert!(matches!(
        sides_along(top, wall, &on, &mut Budget::new(1), &Stop::new()),
        Err(BodyqError::Exhausted { limit: 1 })
    ));
    let stop = Stop::new();
    stop.set();
    assert_eq!(
        sides_along(top, wall, &on, &mut Budget::unlimited(), &stop),
        Err(BodyqError::Cancelled)
    );
}
