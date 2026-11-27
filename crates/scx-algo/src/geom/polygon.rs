// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h`.
// Changes: `polygon_area` of no points is 0 (the pin errors); failures are `None`; the
// parallelogram's sine is `fmath::sin` and its diagonals must be finite; the triangle's
// sides are scaled when their squares overflow. Results unchanged otherwise.
//! Polygons and triangles: signed area, squared triangle area, the foot of a triangle's apex,
//! a parallelogram's diagonals.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`polygon_area`] | `smgu_PolygonArea` |
//! | [`triangle_area_squared`] | `smgu_TriangleAreaSquared` |
//! | [`triangle_decomposition`] | `smgu_TriangleDecomposition` |
//! | [`parallelogram_cross_vectors`] | `smgu_ParallelogramCrossVectors` |
//!
//! Deviations:
//! - `smgu_PolygonArea` with no points reads `sPoints[-1]` upstream and the pin fails with
//!   `InvalidInput`; no points enclose no area, so it is 0 here.
//! - `smgu_TriangleDecomposition` and `smgu_ParallelogramCrossVectors` fail with
//!   `SM_ERR_INVALID_INPUT` for sides that make no triangle or vectors with no direction;
//!   `None` here. Parallel `base` and `side` (a sine of exactly 0) are `None` too;
//!   upstream divides by the zero sine. So are diagonals with a component not finite
//!   (distances not finite, or past `f64`'s range), where upstream answers it.
//! - `smgu_TriangleDecomposition` squares the sides, so sides past `1e154` give NaN
//!   upstream. Here such sides are taken over the longest one and the answer scaled back.

use scx_num::math::{Point3, EFF_ZERO};
use scx_num::tol::is_zero;
use scx_num::vector3d::{sqrt, UnitVec3, Vec3};

/// The signed area of the closed polygon `points` seen along `unit_normal`
/// (`smgu_PolygonArea`): `½ Σ (p_{i-1} × p_i) · n`, positive when the points run
/// counter-clockwise about the normal. 0 for no points.
#[must_use]
pub fn polygon_area(points: &[Point3], unit_normal: UnitVec3) -> f64 {
    let Some(&first_last) = points.last() else {
        return 0.0;
    };
    let mut last = first_last.to_vec();
    let mut total_area = 0.0;
    for &curr in points {
        let curr = curr.to_vec();
        total_area += last.cross(curr).dot(*unit_normal);
        last = curr;
    }
    total_area / 2.0
}

/// The squared area of the triangle `p0 p1 p2` (`smgu_TriangleAreaSquared`): `|(p1 - p0) ×
/// (p2 - p0)|² / 4`, written out as upstream writes it.
#[must_use]
pub fn triangle_area_squared(p0: Point3, p1: Point3, p2: Point3) -> f64 {
    let ay_bz = (p1.y - p0.y) * (p2.z - p0.z);
    let az_bx = (p1.z - p0.z) * (p2.x - p0.x);
    let ax_by = (p1.x - p0.x) * (p2.y - p0.y);

    let az_by = (p1.z - p0.z) * (p2.y - p0.y);
    let ax_bz = (p1.x - p0.x) * (p2.z - p0.z);
    let ay_bx = (p1.y - p0.y) * (p2.x - p0.x);

    ((ay_bz - az_by) * (ay_bz - az_by)
        + (az_bx - ax_bz) * (az_bx - ax_bz)
        + (ax_by - ay_bx) * (ax_by - ay_bx))
        / 4.0
}

/// Where the apex of the triangle with these side lengths stands over its base
/// (`smgu_TriangleDecomposition`): `(along, height)`, the distance along the base from its
/// left end to the apex's foot and the apex's height. `None` for a zero base, legs no longer
/// than the base (within `SM_EFF_ZERO · (1 + base)`), or a foot beyond the right leg.
#[must_use]
pub fn triangle_decomposition(
    base_length: f64,
    left_leg_length: f64,
    right_leg_length: f64,
) -> Option<(f64, f64)> {
    if is_zero(base_length) {
        return None;
    }
    let legs = left_leg_length + right_leg_length;
    let least = base_length + (EFF_ZERO * (1.0 + base_length));
    if legs.is_nan() || least.is_nan() || legs < least {
        return None;
    }
    let (q, height) = apex_foot(base_length, left_leg_length, right_leg_length)?;
    Some((base_length - q, height))
}

/// The apex foot's distance `q` from the base's right end and the apex height, or `None`
/// when the foot is beyond the right leg. When a square overflows (sides past `1e154`),
/// the sides are taken over the longest one and the answer scaled back.
fn apex_foot(base: f64, left: f64, right: f64) -> Option<(f64, f64)> {
    let q = (left * left - right * right - base * base) / (-2.0 * base);
    let longest = base.abs().max(left.abs()).max(right.abs());
    if !q.is_finite() && longest.is_finite() && [base, left, right].iter().all(|s| s.is_finite()) {
        let (b, l, r) = (base / longest, left / longest, right / longest);
        let q = (l * l - r * r - b * b) / (-2.0 * b);
        if q.is_nan() || q.abs() > r {
            return None;
        }
        return Some((q * longest, sqrt(r * r - q * q) * longest));
    }
    if q.is_nan() || right.is_nan() || q.abs() > right {
        return None;
    }
    Some((q, sqrt(right * right - q * q)))
}

/// The two diagonals of the parallelogram with sides along `base` and `side`, its top and
/// bottom `distance_top_to_bottom` apart and its sides `distance_between_sides` apart
/// (`smgu_ParallelogramCrossVectors`): lower left to upper right, then lower right to upper
/// left. `None` when `base` or `side` has no direction, they are parallel, or a diagonal is
/// not finite.
#[must_use]
pub fn parallelogram_cross_vectors(
    base: Vec3,
    side: Vec3,
    distance_top_to_bottom: f64,
    distance_between_sides: f64,
) -> Option<(Vec3, Vec3)> {
    let base = *base.normalized()?;
    let side = *side.normalized()?;
    // the sine as `|base × side|`, which is 0 for parallel units (upstream's
    // `sin(angle_between)` is `1.2e-16` at `π`)
    let sine_angle = base.cross(side).length();
    // parallel sides span no parallelogram (upstream divides by zero)
    if sine_angle.is_nan() || sine_angle == 0.0 {
        return None;
    }
    let leng_side = distance_top_to_bottom / sine_angle;
    let leng_bot = distance_between_sides / sine_angle;
    let low_left_to_up_right = base * leng_bot + side * leng_side;
    let low_right_to_up_left = side * leng_side - base * leng_bot;
    (low_left_to_up_right.is_finite() && low_right_to_up_left.is_finite())
        .then_some((low_left_to_up_right, low_right_to_up_left))
}
