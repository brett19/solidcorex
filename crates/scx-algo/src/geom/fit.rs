// Moved from solidcore `solidcore/crates/solidcore/src/geom/geom_utility/p1.rs` and
// `solidcore/crates/solidcore/src/geom/geom_utility/p2.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h`. Upstream: `source/SMLib/src/SmGeomUtility.cpp`,
// `source/SMLib/inc/SmGeomUtility.h` (enums from `source/SMLib/inc/SmTypes.h`).
// Changes: the fitted direction is an `Option<UnitVec3>` (the pin's zero vector is `None`);
// `PointSetType::Unknown` and its integer codes are gone; the scatter matrix stays private;
// the plane fit's gap is the largest absolute point/plane distance (the pin's is the
// largest signed one). Results unchanged otherwise.
//! Best-fit point, line and plane of a point set, and the set's classification.
//!
//! Upstream: `source/SMLib/src/SmGeomUtility.cpp`, `source/SMLib/inc/SmGeomUtility.h`.
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`PointSetType`] | `SmPointSetType` |
//! | [`best_fit_point`], [`PointFit`] | `smgu_BestFitPointToPoints` |
//! | [`best_fit_line`], [`LineFit`] | `smgu_BestFitLineToPoints` |
//! | [`best_fit_plane`], [`PlaneFit`] | `smgu_BestFitPlaneToPoints` |
//!
//! Deviations:
//! - `SM_PST_UNKNOWN` is not a result of any fit and is dropped.
//! - `smgu_BestFitLineToPoints`' optional `pOptM` (the scatter matrix) is not exposed:
//!   [`best_fit_plane`] builds it through the same private helper.
//! - The best-fit line is the largest-eigenvalue eigenvector of the scatter matrix `M` (the
//!   least-squares line), as in the pin (`WR-10`); pinned v1.0.1 took that of
//!   `del I - M` with un-negated off-diagonals.
//! - `smgu_BestFitPlaneToPoints` keeps the largest *signed* gap `n · (p - c)`, so points
//!   below the plane are never measured: a set bent to one side of its plane passes for
//!   planar however far it is bent. Here the gap is `|n · (p - c)|`.

use scx_num::error::Result;
use scx_num::math::Point3;
use scx_num::matrix::Matrix;
use scx_num::vector3d::{sqrt, UnitVec3, Vec3};

use crate::geom::line::line_point_distance;

/// How a point set spreads, to a tolerance (`SmPointSetType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointSetType {
    /// No points (`SM_PST_VOID`).
    Void,
    /// Within the tolerance of its centroid (`SM_PST_POINT_SIZED`).
    PointSized,
    /// Within the tolerance of its best-fit line (`SM_PST_LINEAR`).
    Linear,
    /// Within the tolerance of its best-fit plane (`SM_PST_PLANAR`).
    Planar,
    /// None of these (`SM_PST_SCATTERED`).
    Scattered,
}

/// Result of [`best_fit_point`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointFit {
    /// [`PointSetType::Void`], [`PointSetType::PointSized`] or [`PointSetType::Scattered`].
    pub set_type: PointSetType,
    /// The centroid (the origin for no points).
    pub center: Point3,
    /// The largest point/centroid distance.
    pub max_gap: f64,
}

/// The centroid of `points` and whether they all lie within `tol_3d` of it
/// (`smgu_BestFitPointToPoints`).
#[must_use]
pub fn best_fit_point(points: &[Point3], tol_3d: f64) -> PointFit {
    match points {
        [] => {
            return PointFit {
                set_type: PointSetType::Void,
                center: Point3::ORIGIN,
                max_gap: 0.0,
            }
        }
        [only] => {
            return PointFit {
                set_type: PointSetType::PointSized,
                center: *only,
                max_gap: 0.0,
            }
        }
        _ => {}
    }
    let mut sum = Vec3::ZERO;
    for p in points {
        sum += p.to_vec();
    }
    #[allow(clippy::cast_precision_loss)] // a point count is far below 2^53
    let center = (sum / points.len() as f64).to_point();

    let mut max_gap = 0.0;
    for p in points {
        let dist2 = center.distance_squared(*p);
        if max_gap < dist2 {
            max_gap = dist2;
        }
    }
    let max_gap = sqrt(max_gap);
    PointFit {
        set_type: if max_gap <= tol_3d {
            PointSetType::PointSized
        } else {
            PointSetType::Scattered
        },
        center,
        max_gap,
    }
}

/// Result of [`best_fit_line`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineFit {
    /// [`PointSetType::Linear`] or [`PointSetType::Scattered`] after a line was fitted, else
    /// the point fit's type.
    pub set_type: PointSetType,
    /// The centroid, a point of the line.
    pub point: Point3,
    /// The line direction; `None` when the points were not scattered about their centroid.
    pub direction: Option<UnitVec3>,
    /// The largest point/line distance (0 without a line).
    pub max_line_gap: f64,
    /// The largest point/centroid distance.
    pub max_centroid_gap: f64,
}

/// The symmetric scatter matrix `Σ (p - c)(p - c)ᵀ`, summed as upstream sums it.
fn scatter(points: &[Point3], c: Point3) -> Result<Matrix> {
    let mut m = Matrix::new(3, 3);
    for p in points {
        let yx = p.x - c.x;
        let yy = p.y - c.y;
        let yz = p.z - c.z;
        m.add_to(0, 0, yx * yx)?;
        m.add_to(0, 1, yx * yy)?;
        m.add_to(0, 2, yx * yz)?;
        m.add_to(1, 1, yy * yy)?;
        m.add_to(1, 2, yy * yz)?;
        m.add_to(2, 2, yz * yz)?;
    }
    // finish the symmetric matrix
    m.set_at(1, 0, m.at(0, 1)?)?;
    m.set_at(2, 0, m.at(0, 2)?)?;
    m.set_at(2, 1, m.at(1, 2)?)?;
    Ok(m)
}

/// The unit eigenvector, `(0, 0, 1)` for a vanishing one (upstream's ignored `Unitize`).
fn unit_or_z(v: Vec3) -> UnitVec3 {
    v.normalized().unwrap_or(UnitVec3::Z)
}

/// The fit, and the scatter matrix when the points were scattered about their centroid.
fn line_fit(points: &[Point3], tol_3d: f64) -> Result<(LineFit, Option<Matrix>)> {
    let fit = best_fit_point(points, tol_3d);
    let point = fit.center;
    if fit.set_type != PointSetType::Scattered {
        let line = LineFit {
            set_type: fit.set_type,
            point,
            direction: None,
            max_line_gap: 0.0,
            max_centroid_gap: fit.max_gap,
        };
        return Ok((line, None));
    }
    let m = scatter(points, point)?;
    // eigen solve on a copy: the plane fit reads `m` again
    let (_, [_, _, largest]) = m.clone().compute_3x3_eigen_vectors()?;
    // largest eigenvalue
    let direction = unit_or_z(largest);

    let mut max_line_gap = 0.0;
    for p in points {
        // a unit direction always has a closest point
        let dist = line_point_distance(point, *direction, *p).map_or(0.0, |(d, _)| d);
        if max_line_gap < dist {
            max_line_gap = dist;
        }
    }
    let line = LineFit {
        set_type: if max_line_gap <= tol_3d {
            PointSetType::Linear
        } else {
            PointSetType::Scattered
        },
        point,
        direction: Some(direction),
        max_line_gap,
        max_centroid_gap: fit.max_gap,
    };
    Ok((line, Some(m)))
}

/// The least-squares line through `points` and whether they all lie within `tol_3d` of it
/// (`smgu_BestFitLineToPoints`).
///
/// # Errors
/// The eigen solve's, when it does not converge (non-finite points).
pub fn best_fit_line(points: &[Point3], tol_3d: f64) -> Result<LineFit> {
    line_fit(points, tol_3d).map(|(line, _)| line)
}

/// Result of [`best_fit_plane`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneFit {
    /// The set's classification: the line fit's, refined to [`PointSetType::Planar`] or
    /// [`PointSetType::Scattered`] when the points were scattered about their line.
    pub set_type: PointSetType,
    /// The centroid, a point of the plane.
    pub point: Point3,
    /// The plane normal ([`PointSetType::Planar`], [`PointSetType::Scattered`]), the line
    /// direction ([`PointSetType::Linear`]), or `None`.
    pub direction: Option<UnitVec3>,
    /// The largest point/plane distance; the point/line gap for a linear set, 0 for fewer
    /// dimensions.
    pub max_plane_gap: f64,
    /// The largest point/centroid distance.
    pub max_centroid_gap: f64,
}

/// The least-squares plane through `points` and whether they all lie within `tol_3d` of it
/// (`smgu_BestFitPlaneToPoints`): the smallest-eigenvalue eigenvector of the scatter
/// matrix.
///
/// # Errors
/// The eigen solve's, when it does not converge (non-finite points).
pub fn best_fit_plane(points: &[Point3], tol_3d: f64) -> Result<PlaneFit> {
    let (line, m) = line_fit(points, tol_3d)?;
    let fit = PlaneFit {
        set_type: line.set_type,
        point: line.point,
        direction: line.direction,
        max_plane_gap: line.max_line_gap,
        max_centroid_gap: line.max_centroid_gap,
    };
    let (PointSetType::Scattered, Some(mut m)) = (fit.set_type, m) else {
        return Ok(fit);
    };
    let (_, [smallest, _, _]) = m.compute_3x3_eigen_vectors()?;
    let normal = unit_or_z(smallest);
    let mut max_gap = 0.0;
    for &p in points {
        let dist = normal.dot(p - fit.point).abs();
        if max_gap < dist {
            max_gap = dist;
        }
    }
    Ok(PlaneFit {
        set_type: if max_gap <= tol_3d {
            PointSetType::Planar
        } else {
            PointSetType::Scattered
        },
        direction: Some(normal),
        max_plane_gap: max_gap,
        ..fit
    })
}
