// Written for solidcorex. Apache-2.0.
//! [`plane_bound`]: how far a piece of one plane lies from another plane, as a bound over
//! the piece's box (research 74 §42.5, moved here from `scx-stitch` so that `scx-bodyq`
//! shares it, §46).
//!
//! The proof is one closed form. Take the box's centre `c` moved onto plane `a` (`q`). Any
//! point of `a` inside the box is within the box's half diagonal `R` of `q`, and the signed
//! distance to plane `b` is an affine function whose gradient has length `sin(angle)`, so it
//! changes by at most `sin(angle) R` from its value at `q`. The bound is
//! `|f(q)| + sin(angle) R`: every point of the piece is that near plane `b` or nearer, with
//! no sampling. A face with curved edges is covered by its box.
//!
//! Only planes are compared. Two planes whose normals differ by more than [`CREASE`] (the
//! sine of the angle) are a crease, not a near miss; two whose normals are within it of
//! pointing against each other are the same place with the other side out, and are told
//! apart ([`NotNear::Opposite`]).

use scx_num::extent3d::Extent3d;
use scx_num::vector3d::Vec3;

use crate::Plane;

/// Two planes whose normals differ by more than this (the sine of the angle) are creased
/// faces, not a near miss of one plane.
pub const CREASE: f64 = 1.0e-3;

/// Why two planes are not compared by distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotNear {
    /// The normals differ by more than [`CREASE`] (and do not point against each other):
    /// two planes that meet at an angle.
    Crease,
    /// The normals point against each other: the planes are one place only if one is
    /// turned over.
    Opposite,
}

/// A bound on how far the points of plane `a` inside `bx` lie from plane `b`.
///
/// # Errors
///
/// [`NotNear::Crease`] when the normals differ by more than [`CREASE`] (whichever way they
/// point), and [`NotNear::Opposite`] when they are within it of pointing against each other.
pub fn plane_bound(a: &Plane, b: &Plane, bx: &Extent3d) -> Result<f64, NotNear> {
    let (fa, fb) = (a.frame(), b.frame());
    let (za, zb) = (Vec3::from(fa.z()), Vec3::from(fb.z()));
    let sin = za.cross(zb).length();
    if sin > CREASE {
        return Err(NotNear::Crease);
    }
    if za.dot(zb) <= 0.0 {
        return Err(NotNear::Opposite);
    }
    let c = bx.mid();
    let q = c - za * (c - fa.origin()).dot(za);
    let at_q = (q - fb.origin()).dot(zb).abs();
    let radius = 0.5 * bx.size().length();
    Ok(at_q + sin * radius)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use scx_num::frame::Frame;
    use scx_num::vector3d::Point3;

    fn plane(o: [f64; 3], z: [f64; 3]) -> Plane {
        Plane::new(
            Frame::new(
                Point3::new(o[0], o[1], o[2]),
                Vec3::new(z[0], z[1], z[2]),
                Vec3::new(1.0, 0.0, 0.0)
                    .cross(Vec3::new(z[0], z[1], z[2]))
                    .cross(Vec3::new(z[0], z[1], z[2])),
            )
            .unwrap(),
        )
    }

    fn unit_box() -> Extent3d {
        Extent3d::from_corners(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 0.0)).unwrap()
    }

    #[test]
    fn the_same_plane_on_another_frame_is_nothing_apart() {
        let a = plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let b = plane([5.0, -3.0, 0.0], [0.0, 0.0, 1.0]);
        assert!(plane_bound(&a, &b, &unit_box()).unwrap() < 1e-15);
    }

    #[test]
    fn a_parallel_offset_is_the_offset() {
        let a = plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let b = plane([0.0, 0.0, 3e-6], [0.0, 0.0, 1.0]);
        let d = plane_bound(&a, &b, &unit_box()).unwrap();
        assert!((d - 3e-6).abs() < 1e-12, "{d}");
    }

    #[test]
    fn a_tilt_is_the_angle_over_the_extent() {
        // A tilt of 1e-5 about the y axis over a face 1 wide: the far edge is 1e-5 off, which
        // the bound (the centre's offset plus the angle times the half diagonal) covers and
        // overstates by 21%.
        let t: f64 = 1e-5;
        let a = plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let b = plane(
            [0.0, 0.0, 0.0],
            [scx_num::fmath::sin(t), 0.0, scx_num::fmath::cos(t)],
        );
        let d = plane_bound(&a, &b, &unit_box()).unwrap();
        assert!((t..=1.5 * t).contains(&d), "{d}");
    }

    #[test]
    fn opposite_normals_are_opposite() {
        let a = plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let b = plane([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        assert_eq!(
            plane_bound(&a, &b, &unit_box()).unwrap_err(),
            NotNear::Opposite
        );
    }

    #[test]
    fn a_right_angle_is_a_crease() {
        let a = plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        let b = plane([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_eq!(
            plane_bound(&a, &b, &unit_box()).unwrap_err(),
            NotNear::Crease
        );
    }
}
