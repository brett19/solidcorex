// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h`.
// Changes: degenerate and parallel cases, and answers that are not finite, give `None` in
// place of `SolidError`s, NaNs and the pin's silent parameter 0; a direction whose squared
// length overflows is measured scaled; a segment with no answer has parameter NaN; the
// angle to the perpendicular is in radians; out-params are tuples. Results unchanged
// otherwise (the degrees-to-radians change is at the facade).
//! Lines and segments: closest points, distances and plane crossings.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.
//!
//! A line is a point `p` and a direction `v` of any non-zero length; its parameter `t`
//! names the point `p + t v`. A segment runs from `start` (`t = 0`) to `end` (`t = 1`).
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`line_closest_point`] | `smgu_LineClosestPoint` |
//! | [`line_point_distance`] | `smgu_LinePointDistance` (3d) |
//! | [`line_line_closest_point`] | `smgu_LineLineClosestPoint` |
//! | [`line_plane_intersect`] | `smgu_LinePlaneIntersect` (point + normal) |
//! | [`line_plane_intersect_uv`] | `smgu_LinePlaneIntersect` (point + two tangents) |
//! | [`segment_point_distance`] | `smgu_SegmentPointDistance` |
//! | [`segment_point_angle_to_perp`] | `smgu_SegmentPointAngleDegToPerp` |
//!
//! Deviations:
//! - A direction with `|v|² < SM_EFF_ZERO_SQ` has no closest point: [`line_closest_point`]
//!   and [`line_point_distance`] give `None` where upstream returns parameter 0 (the
//!   line's "beginning"). A direction with a component not finite has none either. The
//!   segment functions keep upstream's answer for a segment of zero length (its start),
//!   since a point is a fine segment; with an input not finite, or a parameter past
//!   `f64`'s range, the parameter is NaN (upstream's 0 came from the zero-direction
//!   branch).
//! - A direction whose `|v|²` overflows (components past `1e154`) still has a closest
//!   point: the parameter is taken with `v / max |v_i|`. Upstream divides by the infinite
//!   square and answers 0 or NaN. Likewise a distance or an angle whose squares overflow
//!   is taken scaled (upstream answers infinity or NaN), and a parameter or distance that
//!   is not finite is `None`.
//! - `smgu_LineLineClosestPoint` and `smgu_LinePlaneIntersect` fail with
//!   `SM_ERR_INVALID_INPUT` for a zero direction or normal and `SM_ERR` for parallel
//!   input; both are `None` here: neither is the caller's fault, the question has no
//!   single answer. A crossing that is not finite (input not finite, or past `f64`'s
//!   range) is `None` too, where upstream answers the NaN or infinity.
//! - `smgu_SegmentPointAngleDegToPerp` returns degrees; [`segment_point_angle_to_perp`]
//!   returns radians (`|angle - π/2|`, from the same `AngleBetween`).

use scx_num::math::{EFF_ZERO, EFF_ZERO_SQ, HALF_PI, Point3};
use scx_num::vector3d::Vec3;

/// `d · v / |v|²`: how many `v`s `d` reaches along `v`; `None` when `v` has no direction
/// (`|v|² < SM_EFF_ZERO_SQ`, or a component not finite). A `|v|²` past `f64`'s range is
/// taken at `v / max |v_i|`.
fn param_along(d: Vec3, v: Vec3) -> Option<f64> {
    let len_sq = v.length_squared();
    // a NaN length has no direction either
    if len_sq.is_nan() || len_sq < EFF_ZERO_SQ {
        return None;
    }
    let t = if len_sq.is_finite() {
        d.dot(v) / len_sq
    } else {
        let scale = v.max_dimension();
        if !scale.is_finite() {
            return None;
        }
        let w = v / scale;
        d.dot(w) / w.length_squared() / scale
    };
    t.is_finite().then_some(t)
}

/// `|v|`, taken at `v / max |v_i|` when `|v|²` overflows; infinite only when `|v|` is.
fn length(v: Vec3) -> f64 {
    let len = v.length();
    if len.is_finite() || !v.is_finite() {
        return len;
    }
    v.length_and_unit().map_or(len, |(len, _)| len)
}

/// `v` scaled to `max |v_i| = 1` when `|v|²` overflows, else `v`: the same direction.
fn in_range(v: Vec3) -> Vec3 {
    let scale = v.max_dimension();
    if v.length_squared().is_finite() || !scale.is_finite() {
        v
    } else {
        v / scale
    }
}

/// The parameter of the point of the line (`line_pnt`, `line_vec`) closest to `test_point`
/// (`smgu_LineClosestPoint`); `None` when `line_vec` has no direction or the parameter is
/// not finite.
#[must_use]
pub fn line_closest_point(line_pnt: Point3, line_vec: Vec3, test_point: Point3) -> Option<f64> {
    param_along(test_point - line_pnt, line_vec)
}

/// The distance from `test_point` to the line and the parameter of its closest point
/// (`smgu_LinePointDistance`); `None` when `line_vec` has no direction or an answer is not
/// finite.
#[must_use]
pub fn line_point_distance(
    line_pnt: Point3,
    line_vec: Vec3,
    test_point: Point3,
) -> Option<(f64, f64)> {
    let param = param_along(test_point - line_pnt, line_vec)?;
    let pnt = line_pnt + param * line_vec;
    let distance = length(pnt - test_point);
    distance.is_finite().then_some((distance, param))
}

/// The parameters `(t1, t2)` of the closest points of two lines
/// (`smgu_LineLineClosestPoint`); `None` when a direction is zero or the lines are
/// parallel (`|v1 × v2|² < SM_EFF_ZERO_SQ`).
#[must_use]
pub fn line_line_closest_point(
    line_pnt1: Point3,
    line_vec1: Vec3,
    line_pnt2: Point3,
    line_vec2: Vec3,
) -> Option<(f64, f64)> {
    if !(line_vec1.length_squared() >= EFF_ZERO_SQ && line_vec2.length_squared() >= EFF_ZERO_SQ) {
        return None;
    }
    // parallel or coincident lines
    let cross = line_vec1.cross(line_vec2);
    let cross_sq = cross.length_squared();
    if cross_sq.is_nan() || cross_sq < EFF_ZERO_SQ {
        return None;
    }
    // minimum distance vector direction
    let cross = cross.normalized()?;
    // vectors perpendicular to each line and the minimum distance vector
    let plane_v1 = cross.cross(line_vec1).normalized()?;
    let plane_v2 = cross.cross(line_vec2).normalized()?;

    let t1 = line_plane_intersect(line_pnt1, line_vec1, line_pnt2, *plane_v2)?;
    let t2 = line_plane_intersect(line_pnt2, line_vec2, line_pnt1, *plane_v1)?;
    Some((t1, t2))
}

/// The parameter where the line crosses the plane through `plane_pnt` with normal
/// `plane_norm` of any non-zero length (`smgu_LinePlaneIntersect`); `None` for a zero
/// direction or normal, or when `|v · n| < SM_EFF_ZERO · max |v_i| + SM_EFF_ZERO_SQ`
/// (parallel).
#[must_use]
pub fn line_plane_intersect(
    line_pnt: Point3,
    line_vec: Vec3,
    plane_pnt: Point3,
    plane_norm: Vec3,
) -> Option<f64> {
    if !(line_vec.length_squared() >= EFF_ZERO_SQ && plane_norm.length_squared() >= EFF_ZERO_SQ) {
        return None;
    }
    let plane_eq_d = -plane_norm.dot(plane_pnt.to_vec());
    let tol = EFF_ZERO * line_vec.max_dimension() + EFF_ZERO_SQ;
    let denom = line_vec.dot(plane_norm);
    if denom.is_nan() || denom.abs() < tol {
        return None; // parallel
    }
    let t = -(plane_eq_d + line_pnt.to_vec().dot(plane_norm)) / denom;
    t.is_finite().then_some(t)
}

/// The line parameter `t` and plane coordinates `(u, v)` where the line crosses the plane
/// `plane_pt + u su + v sv` (`smgu_LinePlaneIntersect`, Cramer's rule); `None` for a zero
/// vector, tangents with a zero cross product, or a line parallel to the plane.
#[must_use]
pub fn line_plane_intersect_uv(
    line_pt: Point3,
    line_vec: Vec3,
    plane_pt: Point3,
    plane_su: Vec3,
    plane_sv: Vec3,
) -> Option<(f64, f64, f64)> {
    let norm = plane_su.cross(plane_sv);
    if ![line_vec, plane_su, plane_sv, norm]
        .iter()
        .all(|v| v.length_squared() >= EFF_ZERO_SQ)
    {
        return None;
    }
    let tol = EFF_ZERO * line_vec.max_dimension() + EFF_ZERO_SQ;
    let diff = line_pt - plane_pt;
    let denom = line_vec.dot(norm);
    if denom.is_nan() || denom.abs() < tol {
        return None; // parallel
    }
    let numer_u = diff.cross(plane_sv).dot(line_vec);
    let numer_v = plane_su.cross(diff).dot(line_vec);
    let numer_t = -norm.dot(diff);
    let tuv = (numer_t / denom, numer_u / denom, numer_v / denom);
    (tuv.0.is_finite() && tuv.1.is_finite() && tuv.2.is_finite()).then_some(tuv)
}

/// The line parameter nearest `test_point`: 0 for a zero-length segment (its start), NaN
/// when there is no answer (an input not finite, or a parameter past `f64`'s range).
fn line_param(line_start: Point3, line_vec: Vec3, test_point: Point3) -> f64 {
    match line_closest_point(line_start, line_vec, test_point) {
        Some(t) => t,
        // a point is a fine segment: its start
        None if line_vec.is_finite() && line_vec.length_squared() < EFF_ZERO_SQ => 0.0,
        None => f64::NAN,
    }
}

/// The segment parameter in `[0, 1]` nearest `test_point`: [`line_param`], clamped.
fn segment_param(line_start: Point3, line_vec: Vec3, test_point: Point3) -> f64 {
    // `clamp` keeps a NaN, as upstream's two `if`s do.
    line_param(line_start, line_vec, test_point).clamp(0.0, 1.0)
}

/// The distance from `test_point` to the segment and the parameter of its closest point
/// (`smgu_SegmentPointDistance`).
#[must_use]
pub fn segment_point_distance(
    line_start: Point3,
    line_end: Point3,
    test_point: Point3,
) -> (f64, f64) {
    let line_vec = line_end - line_start;
    let param = segment_param(line_start, line_vec, test_point);
    let found_pnt = line_start + param * line_vec;
    (length(found_pnt - test_point), param)
}

/// How far `test_point` is from square to the segment where it is nearest it
/// (`smgu_SegmentPointAngleDegToPerp`, in radians): `(|angle - π/2|, param)` with `angle`
/// between the segment and the gap from its nearest end to the point. 0 when the point's
/// foot is on the segment, the point is on it, or the angle is undefined.
#[must_use]
pub fn segment_point_angle_to_perp(
    line_start: Point3,
    line_end: Point3,
    test_point: Point3,
) -> (f64, f64) {
    let line_vec = line_end - line_start;
    let param = line_param(line_start, line_vec, test_point);
    if (0.0..=1.0).contains(&param) {
        return (0.0, param);
    }
    let param = param.clamp(0.0, 1.0);
    let seg_cl_pnt = line_start + line_vec * param;
    let vec_to_pnt = test_point - seg_cl_pnt;
    // targetPoint on the segment
    if vec_to_pnt.length_squared() < EFF_ZERO_SQ {
        return (0.0, param);
    }
    // gap/segment angle (0 to pi); undefined only for a point on the line
    let Ok(ang) = in_range(vec_to_pnt).angle_between(in_range(line_vec)) else {
        return (0.0, param);
    };
    ((ang - HALF_PI).abs(), param)
}
