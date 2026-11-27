// Moved from solidcore `solidcore/crates/solidcore/src/geom/pseudo_box.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8, with `Extent3d::circumscribe` from
// `solidcore/crates/solidcore/src/geom/extent3d.rs`.
// Upstream record (verbatim from those files): Upstream: `source/SMLib/src/SmPseudoBox.cpp`,
// `source/SMLib/inc/SmPseudoBox.h`. Upstream: `source/SMLib/src/SmExtent3d.cpp`,
// `source/SMLib/inc/SmExtent3d.h`.
// Changes: a box is an immutable value built whole (no `init`, setters, `add_point` or
// `_into` forms); its basis is a `BoxBasis` checked once, so coordinates cannot fail and the
// `UNDEF` fallbacks are gone; intervals are never inside out; the measures take explicit
// tolerances and return `Option`s; projections take unit vectors and fail rather than return
// a box of NaN corners. Results unchanged otherwise; the cases that differ are in the
// Deviations below.
//! Pseudo boxes: boxes spanned by three unit, possibly non-orthogonal, basis vectors and an
//! interval along each (tight bounding volumes for curves and surfaces).
//!
//! Upstream: `source/SMLib/src/SmPseudoBox.cpp`, `source/SMLib/inc/SmPseudoBox.h`.
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`BoxBasis`], [`BoxBasis::new`] | `SetBasis` + the `InvertAbsolute` determinant check |
//! | [`BoxBasis::coordinates`] | `InvertAbsolute` |
//! | [`PseudoBox`] | `SmPseudoBox` |
//! | [`PseudoBox::new`] | `SmPseudoBox(xDir, yDir, zDir, xIvl, yIvl, zIvl)` |
//! | [`PseudoBox::from_extent3d`] | `SmPseudoBox(const SmExtent3d&)`, `operator=(const SmExtent3d&)` |
//! | [`PseudoBox::bounding`] | `SetBasis`, `Init`, then `AddPoint3d` for each point |
//! | [`PseudoBox::unbounded`] | `SetUnbounded` |
//! | [`PseudoBox::with_interval`] | `SetInterval` |
//! | [`PseudoBox::expanded`] | `ExpandAbsolute` |
//! | [`PseudoBox::union`] | `Union` |
//! | [`PseudoBox::contains_point`] | `ContainsPoint3d` |
//! | [`PseudoBox::evaluate`] | `Evaluate` |
//! | [`PseudoBox::classify_point`] | `ClassifyPoint3d` |
//! | [`PseudoBox::distance_to_point`] | `DistanceToPoint` |
//! | [`PseudoBox::intersect_line`] | `IntersectLine` |
//! | [`PseudoBox::max_dimension`], [`PseudoBox::xy_area`], [`PseudoBox::volume`] | `GetMaxDimension`, `GetXYArea`, `GetVolume` |
//! | [`PseudoBox::point_sized`], [`PseudoBox::linear`], [`PseudoBox::planar`] | `IsPointSized`, `IsLinear`, `IsPlanar` |
//! | [`PseudoBox::relative_zero`] | `IsPointSized`/`IsLinear`/`IsPlanar`'s `dTol == SM_EFF_ZERO` case |
//! | [`PseudoBox::are_disjoint`] | `AreDisjoint` (no projection) |
//! | [`PseudoBox::are_project_disjoint`] | `AreProjectDisjoint`, `AreDisjoint` (with a projection normal) |
//! | [`PseudoBox::is_contained_by`] | `IsContainedBy` |
//! | [`PseudoBox::planes`] | `CalcPlanes` |
//! | [`PseudoBox::is_bounded`], [`PseudoBox::boundary_types`] | `IsBounded` |
//! | [`PseudoBox::axis_map`], [`PseudoBox::alignment_with`], [`SignedAxis`] | `IsAxisAligned`, `AreAligned` and their `alAxisMap` |
//! | [`PseudoBox::corners`], [`PseudoBox::edges`] | `CalcCorners`/`GetCorners`, `GetEdges` |
//! | [`PseudoBox::project_to_plane`], [`Projection`] | `ProjectToPlane`, `SmProjectionType` |
//! | [`PseudoBox::circumscribing_extent`] | `SmExtent3d::Circumscribe` (`SmExtent3d.cpp`) |
//! | `impl` [`ScaledZero`] | `SmTol::GetScaledZero(const SmPseudoBox&)` (`SmTol.cpp`) |
//!
//! Deviations:
//! - A basis is checked when it is made: [`BoxBasis::new`] refuses what `SetBasis` refuses
//!   (`|b0 · (b1 × b2)| < SM_EFF_ZERO`) and also what makes `InvertAbsolute` fail later
//!   (the unitised determinant within `SM_EFF_ZERO`) and non-finite vectors. Upstream
//!   ignored `SetBasis` and `InvertAbsolute` failures in most callers and then used the
//!   uninitialised point (`SM_UNDEF_DOUBLE` coordinates) or kept the previous basis; here
//!   no such box exists.
//! - Intervals are `Extent1d`s, never inside out, so `HasNegativeVolume`, `Init`'s empty
//!   box and `DistanceToPoint`'s "outside more than three planes" `SM_ERR` (only an
//!   inside-out box gives it) are gone. [`PseudoBox::bounding`] of no finite coordinates is
//!   `None`. It starts from an empty hull, not upstream's `±1e20`, so a coordinate past
//!   `1e20` is kept.
//! - Coordinates and the basis determinant use upstream's six-term expansion
//!   (`smgu_Determinant3Vectors`), so a point on a face classifies as upstream's does.
//! - [`PseudoBox::intersect_line`] refuses a line through a point, or along a direction,
//!   that is not finite (`InvalidInput`): upstream clipped its NaN parameters on.
//! - `IsPointSized`/`IsLinear`/`IsPlanar` read `dTol == SM_EFF_ZERO` as "scale it by the
//!   box": the caller passes [`PseudoBox::relative_zero`] for that. Each returns its
//!   outputs only when the test passes.
//! - `IsPlanar`'s normal runs along the last flat interval's direction (the pin's WR-790):
//!   upstream's is `basis[0] × basis[1]` whichever interval is flat.
//! - `DistanceToPoint` near an edge or a vertex intersects two or three of the box's
//!   planes; those cannot fail for a checked basis, but if rounding made them fail the
//!   distance is the largest distance outside a single plane (a lower bound) where
//!   upstream used the uninitialised point.
//! - A projection takes unit vectors. `ProjectToPlane` writes into a caller's box whose
//!   basis survives a rejected `SetBasis`; here a rejected projected basis is the axes,
//!   which is what upstream's callers (all starting from a fresh box) got. A projection
//!   whose box corners are not finite (a perspective eye on the view plane) fails: upstream
//!   made a box of NaN, and `AreDisjoint` called NaN corners separated (#59).
//!   [`PseudoBox::are_project_disjoint`] then answers `false`, "not known disjoint".
//!   A perspective projection of a box not strictly on one side of the plane through the
//!   eye parallel to the view plane fails too: its image is unbounded (the corners behind
//!   the eye fold through infinity), where upstream answered a bounded box of the corners.
//! - `IsAxisAligned`/`AreAligned` return a map of [`SignedAxis`] in place of `±1, ±2, ±3`.

use scx_num::error::{NumError, Result};
use scx_num::extent1d::{Extent1d, ExtentPointType, Hull1d};
use scx_num::extent3d::{BoundaryType, Extent3d, LineHit};
use scx_num::math::{Point3, BIG_DOUBLE, EFF_ZERO, EFF_ZERO_SQ};
use scx_num::tol::{are_same, is_zero, is_zero_to_tol, ScaledZero};
use scx_num::vector3d::{Axis, UnitVec3, Vec3};

use crate::geom::line::line_closest_point;
use crate::geom::plane::{intersect_three_planes, intersect_two_planes, Plane, PlanesIntersection};
use crate::polynomial::solve_quadratic;

/// Three unit, linearly independent basis vectors (`m_aBasis` with `m_bOrthogonalBasis`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxBasis {
    vectors: [Vec3; 3],
    det: f64,
    orthogonal: bool,
}

impl BoxBasis {
    /// The axes `x, y, z`.
    pub const AXES: Self = Self {
        vectors: [Vec3::X, Vec3::Y, Vec3::Z],
        det: 1.0,
        orthogonal: true,
    };

    /// The basis of `b0`, `b1`, `b2`, each unitised (`SetBasis`): `None` when the vectors
    /// are nearly dependent (`|b0 · (b1 × b2)| < SM_EFF_ZERO` as given, or within
    /// `SM_EFF_ZERO` once unitised) or not finite once unitised.
    #[must_use]
    pub fn new(b0: Vec3, b1: Vec3, b2: Vec3) -> Option<Self> {
        // Basis vectors must be linearly independent; a NaN determinant is refused too
        // (an infinite one is judged once unitised).
        let raw = det3(b0, b1, b2);
        if raw.is_nan() || raw.abs() < EFF_ZERO {
            return None;
        }
        // Basis vectors must be unitised.
        let unit = |b: Vec3| {
            let d = b.length_squared();
            if is_zero(d - 1.0) {
                b
            } else {
                b / scx_num::vector3d::sqrt(d)
            }
        };
        let vectors = [unit(b0), unit(b1), unit(b2)];
        let [v0, v1, v2] = vectors;
        let det = det3(v0, v1, v2);
        if is_zero(det) || !det.is_finite() {
            return None;
        }
        let orthogonal = v0.dot(v1).abs() < EFF_ZERO
            && v1.dot(v2).abs() < EFF_ZERO
            && v2.dot(v0).abs() < EFF_ZERO;
        Some(Self {
            vectors,
            det,
            orthogonal,
        })
    }

    /// The three unit vectors.
    #[must_use]
    pub const fn vectors(&self) -> [Vec3; 3] {
        self.vectors
    }

    /// Whether the vectors are pairwise square (each `|dot| < SM_EFF_ZERO`).
    #[must_use]
    pub const fn is_orthogonal(&self) -> bool {
        self.orthogonal
    }

    /// The coordinates `(a, b, c)` with `p = a b0 + b b1 + c b2` (`InvertAbsolute`, Cramer's
    /// rule).
    #[must_use]
    pub fn coordinates(&self, p: Point3) -> [f64; 3] {
        let [b0, b1, b2] = self.vectors;
        let p = p.to_vec();
        let det = self.det;
        [
            det3(p, b1, b2) / det,
            det3(b0, p, b2) / det,
            det3(b0, b1, p) / det,
        ]
    }
}

/// A projection onto a plane (`SmProjectionType` with its point and vector arguments).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// `SM_PT_PARALLEL`: along `normal` onto the plane through `point`.
    Parallel {
        /// A point of the plane.
        point: Point3,
        /// The plane normal.
        normal: UnitVec3,
    },
    /// `SM_PT_PERSPECTIVE`: towards `eye` onto the plane (`point`, `normal`).
    Perspective {
        /// A point of the plane.
        point: Point3,
        /// The plane normal.
        normal: UnitVec3,
        /// The eye.
        eye: Point3,
    },
    /// `SM_PT_ROTATION`: about the axis (`point`, `axis`) into the plane spanned by `axis`
    /// and `x_axis`.
    Rotation {
        /// A point of the rotation axis.
        point: Point3,
        /// The rotation axis direction.
        axis: UnitVec3,
        /// A direction square to the axis in the target plane.
        x_axis: UnitVec3,
    },
}

impl Projection {
    /// `cpProjPoint`: the plane point or the point on the rotation axis.
    #[must_use]
    pub const fn point(&self) -> Point3 {
        match *self {
            Self::Parallel { point, .. }
            | Self::Perspective { point, .. }
            | Self::Rotation { point, .. } => point,
        }
    }

    /// `cpProjUnitVector`: the plane normal or the rotation axis.
    #[must_use]
    pub const fn unit_vector(&self) -> UnitVec3 {
        match *self {
            Self::Parallel { normal, .. } | Self::Perspective { normal, .. } => normal,
            Self::Rotation { axis, .. } => axis,
        }
    }
}

/// A basis axis's direction along a reference axis ([`PseudoBox::axis_map`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignedAxis {
    /// The reference axis it runs along.
    pub axis: Axis,
    /// Whether it runs against that axis.
    pub reversed: bool,
}

/// What [`PseudoBox::point_sized`] finds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointSized {
    /// The box centre.
    pub center: Point3,
    /// The largest half interval length.
    pub deviation: f64,
}

/// What [`PseudoBox::linear`] finds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Linear {
    /// The box centre.
    pub point: Point3,
    /// Basis vector 0.
    pub tangent: Vec3,
    /// The larger of the second and third half interval lengths.
    pub deviation: f64,
}

/// What [`PseudoBox::planar`] finds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planar {
    /// The box centre.
    pub point: Point3,
    /// Along the last flat interval's direction: the cross product of the other two basis
    /// vectors in cyclic order; `basis[0] × basis[1]` when the last interval is flat. Not
    /// unitised, as upstream.
    pub normal: Vec3,
    /// The smallest half interval length.
    pub deviation: f64,
}

/// A box spanned by three unit, not necessarily orthogonal, basis vectors and an interval
/// along each (`SmPseudoBox`): the points `a b0 + b b1 + c b2` with `a`, `b`, `c` in the
/// intervals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PseudoBox {
    basis: BoxBasis,
    intervals: [Extent1d; 3],
}

/// Corner index pairs of the 12 box edges, in `ProjectToPlane`'s order.
const EDGE_CORNERS: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// A box being sized by points: one hull per basis coordinate.
struct Sizer {
    basis: BoxBasis,
    hulls: [Hull1d; 3],
}

impl Sizer {
    const fn new(basis: BoxBasis) -> Self {
        Self {
            basis,
            hulls: [Hull1d::EMPTY; 3],
        }
    }

    /// `AddPoint3d`: a NaN coordinate leaves its hull alone, as upstream's `<`/`>` do.
    fn add(&mut self, p: Point3) {
        for (h, a) in self.hulls.iter_mut().zip(self.basis.coordinates(p)) {
            h.add(a);
        }
    }

    fn finish(self) -> Option<PseudoBox> {
        let [h0, h1, h2] = self.hulls;
        Some(PseudoBox {
            basis: self.basis,
            intervals: [h0.finish()?, h1.finish()?, h2.finish()?],
        })
    }
}

impl ScaledZero for PseudoBox {
    /// `SmTol::GetScaledZero(const SmPseudoBox&)`: `SM_EFF_ZERO (1 + max |end|)` over the
    /// three intervals.
    fn scaled_zero(&self) -> f64 {
        let [i0, i1, i2] = &self.intervals;
        EFF_ZERO
            * (1.0
                + i0.max_dimension()
                    .max(i1.max_dimension())
                    .max(i2.max_dimension()))
    }
}

/// The determinant of the matrix with rows `a`, `b`, `c`, expanded in upstream's six terms
/// (`smgu_Determinant3Vectors`). As accurate as [`Vec3::triple_product`], which rounds
/// differently; the box's basis tests, coordinates and orientation keep upstream's rounding
/// so that a point on a face classifies as upstream's does.
fn det3(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    a.x * b.y * c.z - a.x * b.z * c.y + a.y * b.z * c.x - a.y * b.x * c.z + a.z * b.x * c.y
        - a.z * b.y * c.x
}

/// The largest of three, as upstream's `SM_MAX3` (`a > b ? a : b`, then with `c`).
fn max3(a: f64, b: f64, c: f64) -> f64 {
    let ab = if a > b { a } else { b };
    if ab > c {
        ab
    } else {
        c
    }
}

/// The smallest of three, as upstream's `SM_MIN3`.
fn min3(a: f64, b: f64, c: f64) -> f64 {
    let ab = if a < b { a } else { b };
    if ab < c {
        ab
    } else {
        c
    }
}

impl PseudoBox {
    /// The box of `basis` and `intervals` (`SmPseudoBox(xDir, yDir, zDir, xIvl, yIvl,
    /// zIvl)`).
    #[must_use]
    pub const fn new(basis: BoxBasis, intervals: [Extent1d; 3]) -> Self {
        Self { basis, intervals }
    }

    /// The axis-aligned box of `e` (`SmPseudoBox(const SmExtent3d&)`).
    #[must_use]
    pub const fn from_extent3d(e: &Extent3d) -> Self {
        Self {
            basis: BoxBasis::AXES,
            intervals: [e.x_interval(), e.y_interval(), e.z_interval()],
        }
    }

    /// The smallest box of `basis` holding `points` (`SetBasis`, `Init`, then `AddPoint3d`
    /// for each); `None` when an axis has no coordinate that is not NaN.
    #[must_use]
    pub fn bounding<I: IntoIterator<Item = Point3>>(basis: BoxBasis, points: I) -> Option<Self> {
        let mut sizer = Sizer::new(basis);
        for p in points {
            sizer.add(p);
        }
        sizer.finish()
    }

    /// The box of `basis` unbounded along every vector (`SetUnbounded`: each interval
    /// `±SM_INFINITE_PARAMETER`).
    #[must_use]
    pub const fn unbounded(basis: BoxBasis) -> Self {
        Self {
            basis,
            intervals: [Extent1d::UNBOUNDED; 3],
        }
    }

    /// This box with interval `axis` (0, 1, 2 for x, y, z) replaced (`SetInterval`).
    #[must_use]
    pub fn with_interval(self, axis: Axis, ivl: Extent1d) -> Self {
        let [i0, i1, i2] = self.intervals;
        let intervals = match axis {
            Axis::X => [ivl, i1, i2],
            Axis::Y => [i0, ivl, i2],
            Axis::Z => [i0, i1, ivl],
        };
        Self { intervals, ..self }
    }

    /// Every interval grown by `d` at both ends (`ExpandAbsolute`; [`Extent1d::expanded`]).
    #[must_use]
    pub fn expanded(self, d: f64) -> Self {
        Self {
            intervals: self.intervals.map(|i| i.expanded(d)),
            ..self
        }
    }

    /// The box of `basis` holding both boxes' corners (`Union`); `None` only when no corner
    /// has finite coordinates.
    #[must_use]
    pub fn union(&self, other: &Self, basis: BoxBasis) -> Option<Self> {
        Self::bounding(basis, self.corners().into_iter().chain(other.corners()))
    }

    /// The basis.
    #[must_use]
    pub const fn basis(&self) -> &BoxBasis {
        &self.basis
    }

    /// The intervals along the three basis vectors.
    #[must_use]
    pub const fn intervals(&self) -> &[Extent1d; 3] {
        &self.intervals
    }

    /// Whether `p` is within `tol` of the box in each basis coordinate (`ContainsPoint3d`).
    #[must_use]
    pub fn contains_point(&self, p: Point3, tol: f64) -> bool {
        let [a0, a1, a2] = self.basis.coordinates(p);
        let [i0, i1, i2] = &self.intervals;
        i0.contains_value(a0, tol) && i1.contains_value(a1, tol) && i2.contains_value(a2, tol)
    }

    /// The point at normalised coordinates `(nx, ny, nz)`, 0 to 1 across each interval
    /// (`Evaluate`).
    #[must_use]
    pub fn evaluate(&self, nx: f64, ny: f64, nz: f64) -> Point3 {
        let [b0, b1, b2] = self.basis.vectors;
        let [i0, i1, i2] = &self.intervals;
        (b0 * i0.evaluate(nx) + b1 * i1.evaluate(ny) + b2 * i2.evaluate(nz)).to_point()
    }

    /// Where `p` lies against each interval (`ClassifyPoint3d`).
    #[must_use]
    pub fn classify_point(&self, p: Point3, tol: f64) -> [ExtentPointType; 3] {
        let [a0, a1, a2] = self.basis.coordinates(p);
        let [i0, i1, i2] = &self.intervals;
        [
            i0.classify_point(a0, tol),
            i1.classify_point(a1, tol),
            i2.classify_point(a2, tol),
        ]
    }

    /// The distance from `p` to the box, 0 inside (`DistanceToPoint`): to the one face
    /// plane `p` is outside, the line two meet at, or the vertex three meet at.
    #[must_use]
    pub fn distance_to_point(&self, p: Point3) -> f64 {
        let mut found: [(Plane, f64); 3] = [(Plane::new(Point3::ORIGIN, UnitVec3::Z), 0.0); 3];
        let mut count = 0;
        for plane in self.planes() {
            let res = plane.normal.dot(p.to_vec()) - plane.point.to_vec().dot(*plane.normal);
            // the point is outside this plane
            if res > EFF_ZERO {
                // A box never inverted is outside at most one of each opposite pair.
                if let Some(slot) = found.get_mut(count) {
                    *slot = (plane, res);
                }
                count += 1;
            }
        }
        // the largest distance outside a single plane: a lower bound, should an edge or a
        // vertex fail to intersect
        let outside = found
            .iter()
            .take(count)
            .fold(0.0_f64, |m, &(_, d)| m.max(d));
        match (count, found) {
            (0, _) => 0.0,
            (1, [(_, d), ..]) => d,
            // closest to the edge the two planes meet at
            (2, [(a, _), (b, _), _]) => match intersect_two_planes(&a, &b) {
                // no closest point (`p` not finite) is no distance
                Some(PlanesIntersection::Line { point, direction }) => {
                    line_closest_point(point, *direction, p)
                        .map_or(f64::NAN, |t| (point + t * *direction).distance(p))
                }
                _ => outside,
            },
            // closest to the vertex
            (3, [(a, _), (b, _), (c, _)]) => {
                intersect_three_planes(&a, &b, &c).map_or(outside, |v| v.distance(p))
            }
            _ => outside,
        }
    }

    /// Where the line `p + t v` crosses the box (`IntersectLine`, slab by slab), from the
    /// midpoint of the crossing parameters tested against the box.
    ///
    /// # Errors
    /// [`NumError::InvalidInput`] when `|v|² < SM_EFF_ZERO_SQ`.
    pub fn intersect_line(&self, p: Point3, v: Vec3) -> Result<LineHit> {
        // a point or direction not finite is no line (as `Extent3d::intersect_line`)
        if !p.is_finite() || !v.is_finite() || v.length_squared() < EFF_ZERO_SQ {
            return Err(NumError::invalid_input(
                "pseudo_box::PseudoBox::intersect_line",
                "undefined or zero line",
            ));
        }
        let [b0, b1, b2] = self.basis.vectors;
        let [i0, i1, i2] = &self.intervals;
        // the box's plane normals
        let normals = [b1.cross(b2), b2.cross(b0), b0.cross(b1)];
        // the min and max corners
        let min_pt = (i0.min() * b0 + i1.min() * b1 + i2.min() * b2).to_point();
        let max_pt = (i0.max() * b0 + i1.max() * b1 + i2.max() * b2).to_point();

        let slab = |n: Vec3| -> (f64, f64) {
            let ndv = n.dot(v);
            if ndv > EFF_ZERO {
                ((min_pt - p).dot(n) / ndv, (max_pt - p).dot(n) / ndv)
            } else if ndv < -EFF_ZERO {
                ((max_pt - p).dot(n) / ndv, (min_pt - p).dot(n) / ndv)
            } else {
                (-BIG_DOUBLE, BIG_DOUBLE)
            }
        };
        let [n0, n1, n2] = normals;
        let (tmin_x, tmax_x) = slab(n0);
        let (tmin_y, tmax_y) = slab(n1);
        let (tmin_z, tmax_z) = slab(n2);

        // the crossing runs from the largest entry to the smallest exit
        let d_min = max3(tmin_z, tmin_x, tmin_y);
        let d_max = min3(tmax_z, tmax_x, tmax_y);

        let mid = p + ((d_min + d_max) / 2.0) * v;
        let tol = EFF_ZERO * (1.0 + mid.max_dimension());
        Ok(if !self.contains_point(mid, tol) {
            LineHit::Miss
        } else if (d_min - d_max).abs() < EFF_ZERO {
            // the average makes the crossing exactly one parameter
            LineHit::Touch {
                t: (d_min + d_max) / 2.0,
            }
        } else if d_min < d_max {
            LineHit::Through {
                enter: d_min,
                exit: d_max,
            }
        } else {
            LineHit::Miss
        })
    }

    /// The longest interval length (`GetMaxDimension`).
    #[must_use]
    pub fn max_dimension(&self) -> f64 {
        let [i0, i1, i2] = &self.intervals;
        max3(i0.length(), i1.length(), i2.length())
    }

    /// The area of the face spanned by basis vectors 0 and 1 (`GetXYArea`).
    #[must_use]
    pub fn xy_area(&self) -> f64 {
        let [i0, i1, _] = &self.intervals;
        let [b0, b1, _] = self.basis.vectors;
        i0.length() * i1.length() * b0.cross(b1).length()
    }

    /// The volume, signed by the basis's handedness (`GetVolume`).
    #[must_use]
    pub fn volume(&self) -> f64 {
        let [i0, i1, i2] = &self.intervals;
        let [b0, b1, b2] = self.basis.vectors;
        i0.length() * i1.length() * i2.length() * b0.triple_product(b1, b2)
    }

    /// `SM_EFF_ZERO (1 + max_dimension)`: the tolerance upstream's flatness tests use when
    /// asked for `SM_EFF_ZERO`.
    #[must_use]
    pub fn relative_zero(&self) -> f64 {
        EFF_ZERO * (1.0 + self.max_dimension())
    }

    fn lengths(&self) -> [f64; 3] {
        self.intervals.map(|i| i.length())
    }

    /// The centre and the largest half length when every half interval length is below
    /// `tol` (`IsPointSized`).
    #[must_use]
    pub fn point_sized(&self, tol: f64) -> Option<PointSized> {
        let [d0, d1, d2] = self.lengths();
        (d0 / 2.0 < tol && d1 / 2.0 < tol && d2 / 2.0 < tol).then(|| PointSized {
            center: self.evaluate(0.5, 0.5, 0.5),
            deviation: max3(d0, d1, d2) / 2.0,
        })
    }

    /// The centre line when the second and third half interval lengths are below `tol`
    /// (`IsLinear`).
    #[must_use]
    pub fn linear(&self, tol: f64) -> Option<Linear> {
        let [_, d1, d2] = self.lengths();
        (d1 / 2.0 < tol && d2 / 2.0 < tol).then(|| Linear {
            point: self.evaluate(0.5, 0.5, 0.5),
            tangent: self.basis.vectors[0],
            deviation: if d1 > d2 { d1 } else { d2 } / 2.0,
        })
    }

    /// The centre plane when some half interval length is below `tol` (`IsPlanar`).
    #[must_use]
    pub fn planar(&self, tol: f64) -> Option<Planar> {
        let [d0, d1, d2] = self.lengths();
        let flat = |d: f64| d / 2.0 < tol;
        if !(flat(d0) || flat(d1) || flat(d2)) {
            return None;
        }
        // across the last flat interval (WR-790)
        let [b0, b1, b2] = self.basis.vectors;
        let normal = if !flat(d2) && flat(d1) {
            b2.cross(b0)
        } else if !flat(d2) && flat(d0) {
            b1.cross(b2)
        } else {
            b0.cross(b1)
        };
        Some(Planar {
            point: self.evaluate(0.5, 0.5, 0.5),
            normal,
            deviation: min3(d0, d1, d2) / 2.0,
        })
    }

    /// Whether some plane of `planes` has every corner of `corners` at least `SM_EFF_ZERO`
    /// outside it.
    fn some_plane_separates(planes: &[Plane; 6], corners: &[Point3; 8]) -> bool {
        planes.iter().any(|pl| {
            let d = pl.point.to_vec().dot(*pl.normal);
            corners
                .iter()
                .all(|c| pl.normal.dot(c.to_vec()) - d >= EFF_ZERO)
        })
    }

    /// Whether a face plane of one box has the other box wholly outside it (`AreDisjoint`
    /// without a projection). `false` says only that no face plane separates them.
    #[must_use]
    pub fn are_disjoint(&self, other: &Self) -> bool {
        // all of other's corners outside one of this box's planes
        Self::some_plane_separates(&self.planes(), &other.corners())
            // and the other way round
            || Self::some_plane_separates(&other.planes(), &self.corners())
    }

    /// [`PseudoBox::are_disjoint`] for both boxes projected by `proj`
    /// (`AreProjectDisjoint`; `AreDisjoint` with a projection normal is the
    /// [`Projection::Parallel`] case through the origin). `false` when either projection
    /// fails.
    #[must_use]
    pub fn are_project_disjoint(&self, other: &Self, proj: &Projection) -> bool {
        match (self.project_to_plane(proj), other.project_to_plane(proj)) {
            (Ok(this), Ok(that)) => this.are_disjoint(&that),
            _ => false,
        }
    }

    /// Whether every corner is within `tol` of `other` (`IsContainedBy`).
    #[must_use]
    pub fn is_contained_by(&self, other: &Self, tol: f64) -> bool {
        self.corners().iter().all(|c| other.contains_point(*c, tol))
    }

    /// The six face planes, normals out of the box (`CalcPlanes`): the low and high face
    /// across basis vector 0, then 1, then 2, each through its face's centre.
    #[must_use]
    pub fn planes(&self) -> [Plane; 6] {
        let [b0, b1, b2] = self.basis.vectors;
        let [i0, i1, i2] = &self.intervals;
        let no_flip = self.basis.det > 0.0;
        let origin = b0 * i0.min() + b1 * i1.min() + b2 * i2.min();
        let vec0 = b0 * i0.length();
        let vec1 = b1 * i1.length();
        let vec2 = b2 * i2.length();

        // The cross product of two vectors of a checked basis is at least its determinant
        // long, so it always has a direction.
        let pair = |normal: Vec3, a: Vec3, b: Vec3, far: Vec3| {
            let n = normal.normalized().unwrap_or(UnitVec3::Z);
            let p_lo = (origin + 0.5 * (a + b)).to_point();
            let v_lo = if no_flip { -n } else { n };
            (Plane::new(p_lo, v_lo), Plane::new(p_lo + far, -v_lo))
        };
        let (p0, p1) = pair(b1.cross(b2), vec1, vec2, vec0);
        let (p2, p3) = pair(b2.cross(b0), vec2, vec0, vec1);
        let (p4, p5) = pair(b0.cross(b1), vec0, vec1, vec2);
        [p0, p1, p2, p3, p4, p5]
    }

    /// Whether every interval is bounded (`IsBounded`).
    #[must_use]
    pub fn is_bounded(&self) -> bool {
        self.intervals.iter().all(Extent1d::is_bounded)
    }

    /// Each interval's boundedness (`IsBounded`'s out-params).
    #[must_use]
    pub fn boundary_types(&self) -> [BoundaryType; 3] {
        self.intervals.map(|i| i.boundary_type())
    }

    fn signed_axes(&self, refs: [Vec3; 3]) -> Option<[SignedAxis; 3]> {
        let mut map = [SignedAxis {
            axis: Axis::X,
            reversed: false,
        }; 3];
        for (m, b) in map.iter_mut().zip(self.basis.vectors) {
            let len = b.length();
            *m = Axis::ALL.into_iter().zip(refs).find_map(|(axis, r)| {
                let dot = b.dot(r) / len;
                if are_same(dot, 1.0) {
                    Some(SignedAxis {
                        axis,
                        reversed: false,
                    })
                } else if are_same(dot, -1.0) {
                    Some(SignedAxis {
                        axis,
                        reversed: true,
                    })
                } else {
                    None
                }
            })?;
        }
        Some(map)
    }

    /// For an orthogonal basis along the coordinate axes, the axis each basis vector runs
    /// along (`IsAxisAligned`).
    #[must_use]
    pub fn axis_map(&self) -> Option<[SignedAxis; 3]> {
        if !self.basis.orthogonal {
            return None;
        }
        self.signed_axes([Vec3::X, Vec3::Y, Vec3::Z])
    }

    /// For an orthogonal basis along `other`'s basis vectors, which of them each basis
    /// vector runs along (`AreAligned`).
    #[must_use]
    pub fn alignment_with(&self, other: &Self) -> Option<[SignedAxis; 3]> {
        if !self.basis.orthogonal {
            return None;
        }
        self.signed_axes(other.basis.vectors.map(|o| o / o.length()))
    }

    /// The corners: `000, 100, 110, 010`, then the same at the top of interval 2
    /// (`CalcCorners`).
    #[must_use]
    pub fn corners(&self) -> [Point3; 8] {
        let [b0, b1, b2] = self.basis.vectors;
        let [i0, i1, i2] = &self.intervals;
        // the min corner
        let c0 = (b0 * i0.min() + b1 * i1.min() + b2 * i2.min()).to_point();
        // vectors to the far corners
        let del0 = b0 * i0.length();
        let del1 = b1 * i1.length();
        let del2 = b2 * i2.length();
        let c1 = c0 + del0;
        let c2 = c1 + del1;
        let c3 = c0 + del1;
        [c0, c1, c2, c3, c0 + del2, c1 + del2, c2 + del2, c3 + del2]
    }

    /// The 12 edges as point pairs, in upstream's order (`GetEdges`).
    #[must_use]
    pub fn edges(&self) -> [(Point3, Point3); 12] {
        const N: [([f64; 3], [f64; 3]); 12] = [
            ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 1.0, 1.0]),
            ([0.0, 1.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 0.0, 0.0]),
            ([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            ([1.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
            ([1.0, 1.0, 1.0], [1.0, 1.0, 0.0]),
            ([1.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
            ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            ([0.0, 0.0, 1.0], [1.0, 0.0, 1.0]),
            ([0.0, 1.0, 1.0], [1.0, 1.0, 1.0]),
            ([0.0, 1.0, 0.0], [1.0, 1.0, 0.0]),
        ];
        N.map(|([ax, ay, az], [bx, by, bz])| (self.evaluate(ax, ay, az), self.evaluate(bx, by, bz)))
    }

    /// The box `proj` projects this one to (`ProjectToPlane`): its basis puts x along the
    /// longest projected edge, its intervals hold the projected corners, and for a
    /// rotation the extreme points of the rotated edges.
    ///
    /// # Errors
    /// [`NumError::InvalidInput`] when the projected corners or basis vectors are not
    /// finite (a perspective eye on the plane) or no orthogonal frame can be built about
    /// the longest projected vector.
    pub fn project_to_plane(&self, proj: &Projection) -> Result<Self> {
        const OP: &str = "pseudo_box::PseudoBox::project_to_plane";
        let proj_point = proj.point();
        let proj_unit = *proj.unit_vector();
        if let Projection::Perspective { normal, eye, .. } = *proj {
            // Seen from the eye, a box reaching the plane through the eye parallel to the
            // view plane has an unbounded image: every corner must be strictly on one side.
            let sides = self.corners().map(|c| {
                let r = c - eye;
                let s = r.dot(*normal);
                let tol = EFF_ZERO * (1.0 + r.length());
                if s > tol {
                    1
                } else if s < -tol {
                    -1
                } else {
                    0
                }
            });
            if !(sides.iter().all(|&s| s == 1) || sides.iter().all(|&s| s == -1)) {
                return Err(NumError::invalid_input(
                    OP,
                    "the box reaches the eye's plane: its perspective image is unbounded",
                ));
            }
        }

        // projected vector * interval lengths, and the projected basis vectors, sorted by
        // length in upstream's insertion order
        let mut len = [0.0_f64; 4];
        let mut basis = [Vec3::ZERO; 4];
        for (ii, (b, ivl)) in self.basis.vectors.iter().zip(&self.intervals).enumerate() {
            let projected = match *proj {
                Projection::Parallel { normal, .. } => b.project_to_plane(*normal),
                Projection::Rotation { axis, x_axis, .. } => {
                    b.rotate_project_to_plane(*axis, *x_axis)
                }
                Projection::Perspective { point, normal, eye } => {
                    b.perspective_project_to_plane(point, point, *normal, eye)
                }
            };
            basis[3] = projected;
            len[3] = projected.length() * (ivl.max() - ivl.min());
            match ii {
                0 => {
                    basis[1] = basis[3];
                    len[1] = len[3];
                }
                1 => {
                    basis[2] = basis[3];
                    len[2] = len[3];
                    if len[2] > len[1] {
                        len[0] = len[2];
                        basis[0] = basis[2];
                    } else {
                        len[0] = len[1];
                        basis[0] = basis[1];
                        len[1] = len[2];
                        basis[1] = basis[2];
                    }
                }
                _ => {
                    if len[3] > len[0] {
                        len[2] = len[1];
                        basis[2] = basis[1];
                        len[1] = len[0];
                        basis[1] = basis[0];
                        len[0] = len[3];
                        basis[0] = basis[3];
                    } else if len[3] > len[1] {
                        len[2] = len[1];
                        basis[2] = basis[1];
                        len[1] = len[3];
                        basis[1] = basis[3];
                    } else {
                        len[2] = len[3];
                        basis[2] = basis[3];
                    }
                }
            }
        }
        if !basis.iter().take(3).all(|b| b.is_finite()) {
            return Err(NumError::invalid_input(
                OP,
                "projected basis vector is not finite",
            ));
        }

        let unit = |v: Vec3| v.normalized().map_or(Vec3::Z, |u| *u);
        let out_basis = if len[0] < EFF_ZERO {
            // every basis vector projects to (nearly) nothing
            BoxBasis::AXES
        } else {
            // x along the longest projected vector
            let x_cross_y = basis[0].cross(basis[1]);
            let x_cross_z = basis[0].cross(basis[2]);
            if x_cross_y.length_squared() > EFF_ZERO_SQ {
                // projected x and y are independent; a rejected basis leaves the axes
                BoxBasis::new(unit(basis[0]), unit(basis[1]), unit(x_cross_y))
                    .unwrap_or(BoxBasis::AXES)
            } else if x_cross_z.length_squared() > EFF_ZERO_SQ {
                // projected x and z are independent
                BoxBasis::new(unit(basis[0]), unit(basis[2]), unit(x_cross_z))
                    .unwrap_or(BoxBasis::AXES)
            } else {
                // any orthogonal basis about basis[0] with x and y in the projection plane
                let y_ref = match *proj {
                    Projection::Rotation { axis, x_axis, .. } => {
                        let x_cross_rot_axis = basis[0].cross(*axis);
                        let x_cross_x_axis = basis[0].cross(*x_axis);
                        if x_cross_rot_axis.length_squared() > x_cross_x_axis.length_squared() {
                            *axis
                        } else {
                            *x_axis
                        }
                    }
                    _ => basis[0].cross(proj_unit),
                };
                let [x, y, z] = basis[0].make_unit_ortho_vectors(Some(y_ref))?;
                // upstream does not refresh the orthogonality flag here: the fresh box's
                // `true`, which a unit orthogonal frame is
                BoxBasis::new(*x, *y, *z).ok_or(NumError::invalid_input(
                    OP,
                    "orthogonal frame about the projected vector is degenerate",
                ))?
            }
        };

        // size the new box by every projected corner
        let mut corners = self.corners();
        let mut sizer = Sizer::new(out_basis);
        for c in &mut corners {
            *c = c.project_point_to_plane(proj_point, proj_unit);
            if !c.is_finite() {
                return Err(NumError::invalid_input(
                    OP,
                    "projected corner is not finite",
                ));
            }
            sizer.add(*c);
        }
        if let Projection::Rotation {
            point: c_pt,
            axis: n,
            x_axis: x,
        } = *proj
        {
            add_rotated_edge_extrema(
                &corners,
                c_pt.to_vec(),
                *n,
                *x,
                proj_point,
                proj_unit,
                &mut sizer,
            );
        }
        sizer
            .finish()
            .ok_or(NumError::invalid_input(OP, "projected box is empty"))
    }

    /// The axis-aligned box holding this one (`SmExtent3d::Circumscribe`): copied interval
    /// by interval for an axis-aligned box (so partly unbounded ones stay partly
    /// unbounded), fully unbounded for an unaligned unbounded one, else the bounding box of
    /// the corners.
    #[must_use]
    pub fn circumscribing_extent(&self) -> Extent3d {
        if let Some(map) = self.axis_map() {
            // basis `ii` runs along axis `m.axis`: its interval belongs there, negated when
            // reversed
            let mut ivls = [Extent1d::point(0.0); 3];
            for (ivl, m) in self.intervals.iter().zip(map) {
                if let Some(slot) = ivls.get_mut(axis_index(m.axis)) {
                    *slot = if m.reversed { ivl.negated() } else { *ivl };
                }
            }
            let [x, y, z] = ivls;
            return Extent3d::from_intervals(x, y, z);
        }
        if self.is_bounded() {
            // the corners' bounding box; corners of a bounded box are finite
            return Extent3d::hull(self.corners()).unwrap_or(Extent3d::from_intervals(
                Extent1d::UNBOUNDED,
                Extent1d::UNBOUNDED,
                Extent1d::UNBOUNDED,
            ));
        }
        Extent3d::from_intervals(
            Extent1d::UNBOUNDED,
            Extent1d::UNBOUNDED,
            Extent1d::UNBOUNDED,
        )
    }
}

const fn axis_index(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    }
}

/// Adds the points of the 12 edges (between the projected `corners`) where a rotation about
/// (`c`, `n`) into the plane of `n` and `x` takes them furthest along the new box's first
/// two basis vectors: the roots in `(0, 1)` of a quadratic in the edge parameter.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn add_rotated_edge_extrema(
    corners: &[Point3; 8],
    c: Vec3,
    n: Vec3,
    x: Vec3,
    proj_point: Point3,
    proj_unit: Vec3,
    out: &mut Sizer,
) {
    let [a0, a1, _] = out.basis.vectors;
    let dcc = c.dot(c);
    let a0x = a0.dot(x);
    let a0n = a0.dot(n);
    let a1x = a1.dot(x);
    let a1n = a1.dot(n);

    for (i0, i1) in EDGE_CORNERS {
        let (Some(p0), Some(p1)) = (corners.get(i0), corners.get(i1)) else {
            continue;
        };
        let (p0, p1) = (p0.to_vec(), p1.to_vec());
        let p10 = p1 - p0;

        // the quadratic's terms that depend on the edge
        let p0p0 = p0.dot(p0);
        let p0p1 = p0.dot(p1);
        let p1p1 = p1.dot(p1);

        let p0p10 = p0.dot(p10);
        let p1p10 = p1.dot(p10);
        let p0n = p0.dot(n);
        let p1n = p1.dot(n);
        let p10n = p10.dot(n);
        let p0c = p0.dot(c);
        let p1c = p1.dot(c);
        let p10c = p10.dot(c);
        let cn = c.dot(n);

        let ax_ss_coef =
            (p0p10 * p0p10) - 2.0 * (p0p10 * p1p10) + (p1p10 * p1p10) + (p0n * p0n * p10n * p10n)
                - 2.0 * (p0n * p1n * p10n * p10n)
                + (p1n * p1n * p10n * p10n)
                + (p0p10 * p0n * -2.0 * p10n)
                - (p1p10 * p0n * -2.0 * p10n)
                - (p0p10 * p1n * -2.0 * p10n)
                + (p1p10 * p1n * -2.0 * p10n);
        let ax_s_coef = 2.0 * (p0p10 * p1p10) - 2.0 * (p0p10 * p0p10) - 2.0 * (p0p10 * -p10c)
            + 2.0 * (p1p10 * -p10c)
            - 2.0 * (p0p10 * p0n * -2.0 * p10n)
            + (p1p10 * p0n * -2.0 * p10n)
            - (-p10c * p0n * -2.0 * p10n)
            + (p0p10 * p1n * -2.0 * p10n)
            + (-p10c * p1n * -2.0 * p10n)
            - (p0p10 * -cn * -2.0 * p10n)
            + (p1p10 * -cn * -2.0 * p10n)
            - 2.0 * (p0n * p0n * p10n * p10n)
            + 2.0 * (p1n * p0n * p10n * p10n)
            - 2.0 * (p0n * -cn * p10n * p10n)
            + 2.0 * (p1n * -cn * p10n * p10n);
        let ax_const = 1.0 * (p0p10 * p0p10)
            + 2.0 * (p0p10 * -p10c)
            + 1.0 * (-p10c * -p10c)
            + 1.0 * (p0p10 * p0n * -2.0 * p10n)
            + 1.0 * (p0p10 * -cn * -2.0 * p10n)
            + 1.0 * (-p10c * p0n * -2.0 * p10n)
            + 1.0 * (-p10c * -cn * -2.0 * p10n)
            + 1.0 * (p0n * p0n * p10n * p10n)
            + 2.0 * (p0n * -cn * p10n * p10n)
            + 1.0 * (-cn * -cn * p10n * p10n);

        let an_ss_coef =
            1.0 * (p0p0 + p0n * -p0n) - 2.0 * (p0p1 + p1n * -p0n) + 1.0 * (p1p1 + p1n * -p1n);
        let an_s_coef = -2.0 * (p0p0 + p0n * -p0n) + 2.0 * (p0p1 + p1n * -p0n)
            - 2.0 * (-p0c + p0n * cn)
            + 2.0 * (-p1c + p1n * cn);
        let an_const = 1.0 * (p0p0 + p0n * -p0n) + 2.0 * (-p0c + p0n * cn) + 1.0 * (dcc + -cn * cn);

        // for both basis vectors
        for (ax, an) in [(a0x, a0n), (a1x, a1n)] {
            let coef = [
                ax * ax * ax_ss_coef - p10n * p10n * an * an * an_ss_coef,
                ax * ax * ax_s_coef - p10n * p10n * an * an * an_s_coef,
                ax * ax * ax_const - p10n * p10n * an * an * an_const,
            ];
            // solve A s² + B s + C = 0
            let scaled_zero = EFF_ZERO * (1.0 + max3(coef[0], coef[1], coef[2]));
            let roots = solve_quadratic(&coef, scaled_zero);
            let sols = roots.as_slice();
            for (mm, &s) in sols.iter().enumerate() {
                // unique roots strictly inside (0, 1) (`SM_IS_BETWEEN_TO_TOL(s, 0, 1,
                // SM_EFF_ZERO)`)
                let between = (0.0 + EFF_ZERO) < s && s < (1.0 - EFF_ZERO);
                let unique = mm == 0
                    || match sols {
                        [s0, s1, ..] => !is_zero_to_tol(s0 - s1, EFF_ZERO),
                        _ => true,
                    };
                if between && unique {
                    // the edge point, projected
                    let p = ((1.0 - s) * p0 + s * p1).to_point();
                    out.add(p.project_point_to_plane(proj_point, proj_unit));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> PseudoBox {
        PseudoBox::new(BoxBasis::AXES, [Extent1d::spanning(0.0, 1.0); 3])
    }

    #[test]
    fn dependent_basis_is_refused() {
        assert!(BoxBasis::new(Vec3::X, Vec3::Y, Vec3::X).is_none());
        assert!(BoxBasis::new(Vec3::X, Vec3::Y, Vec3::new(f64::NAN, 0.0, 1.0)).is_none());
        assert!(BoxBasis::new(Vec3::X * 2.0, Vec3::Y, Vec3::Z).is_some_and(|b| b.is_orthogonal()));
    }

    #[test]
    fn coordinates_invert_a_skew_basis() {
        let b = BoxBasis::new(Vec3::X, Vec3::new(1.0, 1.0, 0.0), Vec3::Z).unwrap();
        let [b0, b1, b2] = b.vectors();
        let p = (2.0 * b0 + 3.0 * b1 - 1.0 * b2).to_point();
        let [a0, a1, a2] = b.coordinates(p);
        assert!((a0 - 2.0).abs() < 1e-15 && (a1 - 3.0).abs() < 1e-15 && (a2 + 1.0).abs() < 1e-15);
    }

    #[test]
    fn distance_to_point_is_euclidean_for_a_cube() {
        let b = unit_box();
        assert!(b.distance_to_point(Point3::new(0.5, 0.5, 0.5)).abs() < 1e-15);
        assert!((b.distance_to_point(Point3::new(3.0, 0.5, 0.5)) - 2.0).abs() < 1e-15);
        assert!((b.distance_to_point(Point3::new(4.0, 5.0, 0.5)) - 5.0).abs() < 1e-14);
        assert!((b.distance_to_point(Point3::new(3.0, 3.0, 2.0)) - 3.0).abs() < 1e-14);
    }

    #[test]
    fn line_crossings_of_a_cube() {
        let b = unit_box();
        let hit = b
            .intersect_line(Point3::new(-1.0, 0.5, 0.5), Vec3::X)
            .unwrap();
        assert_eq!(
            hit,
            LineHit::Through {
                enter: 1.0,
                exit: 2.0
            }
        );
        let miss = b
            .intersect_line(Point3::new(-1.0, 2.0, 0.5), Vec3::X)
            .unwrap();
        assert_eq!(miss, LineHit::Miss);
        assert!(b.intersect_line(Point3::ORIGIN, Vec3::ZERO).is_err());
    }

    #[test]
    fn separated_boxes_are_disjoint() {
        let a = unit_box();
        let b = PseudoBox::new(BoxBasis::AXES, [Extent1d::spanning(2.0, 3.0); 3]);
        assert!(a.are_disjoint(&b) && b.are_disjoint(&a));
        assert!(!a.are_disjoint(&a));
        assert!(a.is_contained_by(&a.expanded(0.5), 0.0));
    }

    #[test]
    fn circumscribe_reversed_axes() {
        let basis = BoxBasis::new(-Vec3::Y, Vec3::X, Vec3::Z).unwrap();
        let b = PseudoBox::new(
            basis,
            [
                Extent1d::spanning(1.0, 2.0),
                Extent1d::spanning(3.0, 4.0),
                Extent1d::spanning(5.0, 6.0),
            ],
        );
        let e = b.circumscribing_extent();
        assert_eq!((e.x_min(), e.x_max()), (3.0, 4.0));
        assert_eq!((e.y_min(), e.y_max()), (-2.0, -1.0));
        assert_eq!((e.z_min(), e.z_max()), (5.0, 6.0));
    }

    #[test]
    fn eye_on_the_plane_fails_the_projection() {
        let b = unit_box();
        let proj = Projection::Perspective {
            point: Point3::ORIGIN,
            normal: UnitVec3::Z,
            eye: Point3::new(0.5, 0.5, 0.0),
        };
        assert!(b.project_to_plane(&proj).is_err());
        assert!(!b.are_project_disjoint(&b, &proj));
    }
}
