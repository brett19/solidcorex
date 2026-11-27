// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` and
// `solidcore/crates/solidcore/src/geom/geom_utility/p2.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h`. Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h` (enums from `source/SMLib/inc/SmTypes.h`).
// Changes: a plane is a [`Plane`] (a point and a `UnitVec3` normal), so the "normal is not
// unit" errors are gone; parallel planes give `None`; `intersect_two_planes` reports
// antiparallel planes on opposite sides as apart (the pin calls them coincident);
// `point_project_to_plane` gives `None` for a zero normal and projects along an overflowing
// one; a common point that is not finite is `None`. Results unchanged otherwise.
//! Planes: their common point or line, and the distance and projection of a point.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`Plane`] | a point and a unit normal passed as two arguments |
//! | [`intersect_three_planes`] | `smgu_IntersectThreePlanes` |
//! | [`intersect_two_planes`], [`PlanesIntersection`] | `smgu_IntersectTwoPlanes` |
//! | [`Plane::distance_to`] | `smgu_PlanePointDistance` |
//! | [`point_project_to_plane`] | `smgu_PointProjectToPlane` |
//!
//! Deviations:
//! - The unit-normal contract is the type: upstream checks `|v·v - 1| <= SM_EFF_ZERO` and
//!   fails; a [`Plane`] cannot be built without a unit normal.
//! - `smgu_IntersectTwoPlanes` calls parallel planes coincident when both signed distances
//!   `(p2 - p1)·v1` and `(p1 - p2)·v2` are below the scaled zero. For antiparallel normals
//!   the two are equal, so planes any distance apart with their normals facing away from
//!   each other passed. Here the distances are taken absolute: coincident means each
//!   plane's point lies within the scaled zero of the other plane.
//! - `smgu_PlanePointDistance` also accepted a normal shorter than unit; the type rules
//!   that out.
//! - `smgu_IntersectThreePlanes` divides on whatever the sum gives; a common point with a
//!   coordinate not finite (input not finite, or past `f64`'s range) is `None` here.
//! - `smgu_PointProjectToPlane` divides by `|n|²`: a zero normal gives NaN and one whose
//!   square overflows (components past `1e154`) moves the point by 0 or NaN. Here a normal
//!   with `|n|² < SM_EFF_ZERO_SQ` or a component not finite gives `None`, and an
//!   overflowing one is projected along by its unit.

use scx_num::math::{EFF_ZERO, EFF_ZERO_SQ, Point3};
use scx_num::vector3d::{UnitVec3, Vec3};

/// A plane through `point` with unit `normal`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// A point on the plane.
    pub point: Point3,
    /// The unit normal.
    pub normal: UnitVec3,
}

impl Plane {
    /// The plane through `point` with unit `normal`.
    #[must_use]
    pub const fn new(point: Point3, normal: UnitVec3) -> Self {
        Self { point, normal }
    }

    /// The unsigned distance from `p` to the plane (`smgu_PlanePointDistance`).
    #[must_use]
    pub fn distance_to(&self, p: Point3) -> f64 {
        (p - self.point).dot(*self.normal).abs()
    }
}

/// The common point of three planes (`smgu_IntersectThreePlanes`); `None` when the normals
/// are (nearly) dependent, `|n1 · (n2 × n3)| < SM_EFF_ZERO`, or the point is not finite.
#[must_use]
pub fn intersect_three_planes(a: &Plane, b: &Plane, c: &Plane) -> Option<Point3> {
    let (v1, v2, v3) = (*a.normal, *b.normal, *c.normal);
    let det = v1.triple_product(v2, v3);
    // Planes do not intersect - must be parallel. A NaN determinant fails too.
    if det.is_nan() || det.abs() < EFF_ZERO {
        return None;
    }
    let work = (a.point.to_vec().dot(v1) * v2.cross(v3))
        + (b.point.to_vec().dot(v2) * v3.cross(v1))
        + (c.point.to_vec().dot(v3) * v1.cross(v2));
    let point = (work / det).to_point();
    point.is_finite().then_some(point)
}

/// How two planes meet ([`intersect_two_planes`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanesIntersection {
    /// The planes meet along the line through `point` with unit `direction` (`n1 × n2`).
    Line {
        /// A point of the line: the common point with the plane through the midpoint of the
        /// two plane points, normal to both.
        point: Point3,
        /// The unit line direction.
        direction: UnitVec3,
    },
    /// The normals are parallel (`|n1 × n2|² < SM_EFF_ZERO_SQ`).
    Parallel {
        /// Each plane's point is within `SM_EFF_ZERO · (1 + max |coordinate|)` of the other
        /// plane.
        coincident: bool,
    },
}

/// The line two planes share, or that they are parallel (`smgu_IntersectTwoPlanes`).
/// `None` only for non-finite input, or when rounding leaves the third plane's
/// determinant below `SM_EFF_ZERO` (the pin fails there too).
#[must_use]
pub fn intersect_two_planes(a: &Plane, b: &Plane) -> Option<PlanesIntersection> {
    let (p1, v1, p2, v2) = (a.point, *a.normal, b.point, *b.normal);
    // a third plane orthogonal to both
    let plane3_pnt = p1.midpoint(p2);
    let plane3_norm = v1.cross(v2);

    if plane3_norm.length_squared() < EFF_ZERO_SQ {
        let scaled_zero = EFF_ZERO * (1.0 + p1.max_dimension().max(p2.max_dimension()));
        let dist1 = (p2 - p1).dot(v1).abs();
        let dist2 = (p1 - p2).dot(v2).abs();
        return Some(PlanesIntersection::Parallel {
            coincident: dist1 < scaled_zero && dist2 < scaled_zero,
        });
    }
    let direction = plane3_norm.normalized()?;
    let point = intersect_three_planes(a, b, &Plane::new(plane3_pnt, direction))?;
    Some(PlanesIntersection::Line { point, direction })
}

/// The normal projection of `point` onto the plane through `plane_pnt` with normal
/// `plane_norm` of any length (`smgu_PointProjectToPlane`); `None` when `plane_norm` has no
/// direction (`|plane_norm|² < SM_EFF_ZERO_SQ`, or a component not finite) or the
/// projection is not finite.
#[must_use]
pub fn point_project_to_plane(
    point: Point3,
    plane_pnt: Point3,
    plane_norm: Vec3,
) -> Option<Point3> {
    let len_sq = plane_norm.dot(plane_norm);
    if len_sq.is_nan() || len_sq < EFF_ZERO_SQ {
        return None;
    }
    if len_sq.is_finite() {
        let k = ((point.x - plane_pnt.x) * plane_norm.x
            + (point.y - plane_pnt.y) * plane_norm.y
            + (point.z - plane_pnt.z) * plane_norm.z)
            / len_sq;
        let projected = point - k * plane_norm;
        if projected.is_finite() {
            return Some(projected);
        }
    }
    // the square or the step overflows: project along the unit normal
    let n = *plane_norm.normalized()?;
    let projected = point - (point - plane_pnt).dot(n) * n;
    projected.is_finite().then_some(projected)
}
