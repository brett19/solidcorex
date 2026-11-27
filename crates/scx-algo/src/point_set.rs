// Moved from solidcore `solidcore/crates/solidcore/src/geom/point_set.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmPointSet.cpp`,
// `source/SMLib/inc/SmPointSet.h`.
// Changes: a set is an immutable value whose fit, frame and boxes are computed when it is
// built (no lazy cache, `clear_cache` or `&mut self` getters); a given normal is a
// `UnitVec3` and is not written back; a closed sequence is its own type and an open one is
// gone; the tight box is a `&self` query. Results unchanged otherwise; the cases that differ
// are in the Deviations below.
//! Point sets and closed point loops: the best-fit frame, the axis-aligned and pseudo
//! bounding boxes, a tightened pseudo box, the enclosed area and winding-number point
//! containment.
//!
//! Upstream: `source/SMLib/src/SmPointSet.cpp`, `source/SMLib/inc/SmPointSet.h`.
//!
//! | Rust | Upstream |
//! |---|---|
//! | [`PointSet`] | `SmPointSet3d` |
//! | [`PointSet::new`] | `SmPointSet3d(rTArray, dTol3d)`, then `SetUpAxes` and `SetUpBBoxes` |
//! | [`PointSet::with_normal`] | `SmPointSet3d(rTArray, dTol3d, bPlanar, pOptNormal)` |
//! | [`PointSet::centroid`], [`PointSet::normal`], [`PointSet::bbox`], [`PointSet::pbox`] | `GetCentroid`, `GetNormal`, `GetBBox` |
//! | [`PointSet::tight_pbox`], [`PointSet::tight_pbox_about`] | `GetTightPBox` (`SetUpTightPseudoBox`) |
//! | [`PointLoop`] | `SmPointSequence` with `bClosed` |
//! | [`PointLoop::area`] | `SmPointSequence::GetArea` |
//! | [`PointLoop::containment`] | `SmPointSequence::PointContainment` |
//! | [`PointContainment`] | `SmPointObjectContainmentType` |
//!
//! Cost: [`PointSet::new`] does the work upstream spreads over its getters, once: the plane
//! fit (three passes over the points and a 3x3 eigen solve) and one pass for both boxes.
//! [`PointSet::with_normal`] skips the eigen solve (one pass for the centroid, one for the
//! boxes). [`PointSet::tight_pbox`] is not cached: six box passes for the rotations, and for
//! planar and linear sets two more passes and a seventh box for the hull-edge alignment.
//! [`PointLoop::area`] is one pass, [`PointLoop::containment`] one pass after the box test.
//!
//! Deviations:
//! - Upstream computes each property on first request and caches it, so the centroid
//!   depends on call order: before `GetNormal` it is a block-wise average (summed in blocks
//!   of 50), after it the plane fit's plain average. Here it is the plain average always
//!   (the fit's, or [`best_fit_point`]'s for a given normal): the two round apart by ulps.
//! - `GetTightPBox(pOptInputPBoxZ)` reuses the cached box when the new `z` is within
//!   `SM_EFF_ZERO` of its third vector, so its `x` and `y` axes depend on earlier calls.
//!   [`PointSet::tight_pbox_about`] always builds the frame of the given `z`.
//! - A given normal and box `z` are `UnitVec3`s and are not written back (upstream
//!   unitises the caller's vectors in place); upstream's zero-length normal, which it
//!   ignores, is the caller's `None` (use [`PointSet::new`]).
//! - The deviations are not computed for a set with a given normal (upstream leaves them
//!   unset); they are `None` there.
//! - An open `SmPointSequence` contains nothing and its area is that of its closing; only
//!   the closed form is ported, as [`PointLoop`].
//! - [`PointLoop::area`] of no points is 0 (upstream's `polygon_area` fails and it
//!   answers -1).
//! - A set with no points has no boxes (`None`; upstream's are empty extents).
//! - `SetUpTightPseudoBox` keeps upstream's arithmetic, with its projections used
//!   (`WR-14`, as the pin) and Linear sets aligned as well as Planar ones (`WR-10`, as the
//!   pin). A rotated basis that `SetBasis` refuses is skipped (upstream keeps the previous
//!   basis, whose box is no smaller).
//! - The plane fit's gap is the largest absolute distance ([`best_fit_plane`]), so a set
//!   bent to one side of its plane is [`PointSetType::Scattered`], with the frame and box
//!   that follow.

use scx_num::error::Result;
use scx_num::extent1d::ExtentPointType;
use scx_num::extent3d::Extent3d;
use scx_num::math::{BIG_DOUBLE, EFF_ZERO, PI};
use scx_num::vector3d::{Point3, UnitVec3, Vec3};

use crate::geom::fit::{PointSetType, best_fit_plane, best_fit_point};
use crate::geom::line::segment_point_distance;
use crate::geom::polygon::polygon_area;
use crate::pseudo_box::{BoxBasis, PseudoBox};

/// A point against a closed point loop (`SmPointObjectContainmentType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointContainment {
    /// The winding count was inconsistent (`SM_POC_UNKNOWN`).
    Unknown,
    /// `SM_POC_INSIDE`.
    Inside,
    /// `SM_POC_OUTSIDE`.
    Outside,
    /// Within tolerance of a point or a segment (`SM_POC_ON_BOUNDARY`).
    OnBoundary,
}

/// How far the points stray from the set's fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deviations {
    /// The largest point/plane distance (point/line for a linear set, 0 for fewer
    /// dimensions).
    pub max_plane_gap: f64,
    /// The largest point/centroid distance.
    pub max_centroid_gap: f64,
}

/// A set of points with its best-fit frame and bounding boxes (`SmPointSet3d`).
///
/// The frame `[x_axis, y_axis, normal]` comes from the best-fit plane (or line) through the
/// points, or from a normal given when the set is built.
#[derive(Debug, Clone, PartialEq)]
pub struct PointSet {
    points: Vec<Point3>,
    tol_3d: f64,
    set_type: PointSetType,
    centroid: Point3,
    frame: [UnitVec3; 3],
    deviations: Option<Deviations>,
    bbox: Option<Extent3d>,
    pbox: Option<PseudoBox>,
}

/// `[z, x, y]` of `v`'s frame as `[x, y, z]`.
fn frame_of(v: UnitVec3) -> [UnitVec3; 3] {
    // a unit vector always has a frame
    let [z, x, y] =
        v.make_unit_ortho_vectors(None)
            .unwrap_or([UnitVec3::Z, UnitVec3::X, UnitVec3::Y]);
    [x, y, z]
}

/// The box of `frame` around `points`.
fn box_of(frame: [UnitVec3; 3], points: &[Point3]) -> Option<PseudoBox> {
    let [x, y, z] = frame;
    PseudoBox::bounding(BoxBasis::new(*x, *y, *z)?, points.iter().copied())
}

impl PointSet {
    /// The set of `points`, distinct to `tol_3d` (taken as `|tol_3d|`), with the frame of
    /// its best-fit plane: for a linear set `x` is along the line, for a planar or
    /// scattered one `normal` is the plane's, and for fewer dimensions the frame is the
    /// axes.
    ///
    /// # Errors
    /// The plane fit's, when its eigen solve does not converge (points not finite).
    pub fn new(points: Vec<Point3>, tol_3d: f64) -> Result<Self> {
        let tol_3d = tol_3d.abs();
        let fit = best_fit_plane(&points, tol_3d)?;
        let axes = [UnitVec3::X, UnitVec3::Y, UnitVec3::Z];
        let frame = match (fit.set_type, fit.direction) {
            (PointSetType::Linear, Some(d)) => {
                // `x` along the line: the line's frame, rolled
                let [x, y, z] = frame_of(d);
                [z, x, y]
            }
            (PointSetType::Planar | PointSetType::Scattered, Some(n)) => frame_of(n),
            _ => axes,
        };
        Ok(Self::with_frame(
            points,
            tol_3d,
            fit.set_type,
            fit.point,
            frame,
            Some(Deviations {
                max_plane_gap: fit.max_plane_gap,
                max_centroid_gap: fit.max_centroid_gap,
            }),
        ))
    }

    /// The set of `points` in the plane of `normal`, never checked against them: the set
    /// is taken as [`PointSetType::Planar`] (`SmPointSet3d` with `pOptNormal`).
    #[must_use]
    pub fn with_normal(points: Vec<Point3>, tol_3d: f64, normal: UnitVec3) -> Self {
        let tol_3d = tol_3d.abs();
        let centroid = best_fit_point(&points, tol_3d).center;
        let frame = frame_of(normal);
        Self::with_frame(points, tol_3d, PointSetType::Planar, centroid, frame, None)
    }

    fn with_frame(
        points: Vec<Point3>,
        tol_3d: f64,
        set_type: PointSetType,
        centroid: Point3,
        frame: [UnitVec3; 3],
        deviations: Option<Deviations>,
    ) -> Self {
        let bbox = Extent3d::hull(points.iter().copied());
        let pbox = box_of(frame, &points);
        Self {
            points,
            tol_3d,
            set_type,
            centroid,
            frame,
            deviations,
            bbox,
            pbox,
        }
    }

    /// The points.
    #[must_use]
    pub fn points(&self) -> &[Point3] {
        &self.points
    }

    /// The distinct-point tolerance.
    #[must_use]
    pub const fn tol_3d(&self) -> f64 {
        self.tol_3d
    }

    /// How the points spread (given as planar for a set with a given normal).
    #[must_use]
    pub const fn set_type(&self) -> PointSetType {
        self.set_type
    }

    /// The average of the points (the origin for none).
    #[must_use]
    pub const fn centroid(&self) -> Point3 {
        self.centroid
    }

    /// The frame `[x_axis, y_axis, normal]`.
    #[must_use]
    pub const fn frame(&self) -> [UnitVec3; 3] {
        self.frame
    }

    /// The frame's normal (`GetNormal`).
    #[must_use]
    pub const fn normal(&self) -> UnitVec3 {
        self.frame[2]
    }

    /// The fit's deviations; `None` for a set with a given normal.
    #[must_use]
    pub const fn deviations(&self) -> Option<Deviations> {
        self.deviations
    }

    /// The axis-aligned bounding box (`GetBBox`); `None` for no points.
    #[must_use]
    pub const fn bbox(&self) -> Option<Extent3d> {
        self.bbox
    }

    /// The bounding box in the set's frame; `None` for no points.
    #[must_use]
    pub const fn pbox(&self) -> Option<PseudoBox> {
        self.pbox
    }

    /// The frame's box rotated about the normal to (roughly) its smallest base area
    /// (`GetTightPBox`); `None` for no points. Not cached: see the module's cost note.
    #[must_use]
    pub fn tight_pbox(&self) -> Option<PseudoBox> {
        self.pbox.map(|b| self.tighten(b))
    }

    /// As [`PointSet::tight_pbox`], for the box with third vector `z` and the frame `z`
    /// makes (`GetTightPBox(pOptInputPBoxZ)`).
    #[must_use]
    pub fn tight_pbox_about(&self, z: UnitVec3) -> Option<PseudoBox> {
        let [zz, x, y] = z.make_unit_ortho_vectors(None).ok()?;
        box_of([x, y, zz], &self.points).map(|b| self.tighten(b))
    }

    /// `SetUpTightPseudoBox`: five rotations of `start` about its normal, keeping the
    /// smallest base area; for a planar or linear set, also the rotation that lays a hull
    /// edge on the box boundary.
    fn tighten(&self, start: PseudoBox) -> PseudoBox {
        const ROT_CNT: usize = 5;
        let points = &self.points;
        let rotated = |b: &PseudoBox, axis: Vec3, ang: f64| {
            let [x, y, z] = b.basis().vectors();
            let basis = BoxBasis::new(
                x.rotate_vec_about_axis(axis, ang),
                y.rotate_vec_about_axis(axis, ang),
                z,
            )?;
            PseudoBox::bounding(basis, points.iter().copied())
        };

        let plane_point = start.evaluate(0.5, 0.5, 0.5);
        let [x0, y0, _] = start.basis().vectors();
        let plane_normal = x0.cross(y0).normalized().map_or(Vec3::Z, |n| *n);
        let mut best = start;
        let mut area = start.xy_area();
        for ii in 0..ROT_CNT {
            #[allow(clippy::cast_precision_loss)] // ii < 6
            let ang = (ii + 1) as f64 * (PI / 2.0 / (ROT_CNT + 1) as f64);
            if let Some(b) = rotated(&start, plane_normal, ang) {
                let a = b.xy_area();
                if a < area {
                    area = a;
                    best = b;
                }
            }
        }

        if !matches!(self.set_type, PointSetType::Planar | PointSetType::Linear) {
            return best;
        }

        // align the box with a hull edge: the points on each boundary of x and y, as
        // [start of x, end of x, start of y, end of y]
        let [plane_x, plane_y, plane_z] = best.basis().vectors();
        let mut bounds = [plane_y, plane_y, plane_x, plane_x].map(Bound::new);
        for (ii, &p) in points.iter().enumerate() {
            let [c0, c1, _] = best.classify_point(p, EFF_ZERO);
            let hits = [
                c0 == ExtentPointType::Start,
                c0 == ExtentPointType::End,
                c1 == ExtentPointType::Start,
                c1 == ExtentPointType::End,
            ];
            for (b, hit) in bounds.iter_mut().zip(hits) {
                if hit {
                    b.index = ii;
                    b.found += 1;
                }
            }
        }
        // a hull edge already lies on a boundary
        if bounds.iter().any(|b| b.found > 1) {
            return best;
        }

        let project = |p: Point3| p.project_point_to_plane(plane_point, plane_z);
        for b in &mut bounds {
            b.end = project(points.get(b.index).copied().unwrap_or(Point3::ORIGIN));
        }
        for (ii, &raw) in points.iter().enumerate() {
            if bounds.iter().any(|b| b.index == ii) {
                continue;
            }
            let this = project(raw);
            for b in &mut bounds {
                // a failure keeps the previous angle, as upstream
                b.ang = plane_normal
                    .ccw_angle_between(b.reference, this - b.end)
                    .unwrap_or(b.ang);
                b.min = min2(b.ang, b.min);
                b.max = max2(b.ang, b.max);
            }
        }
        // the largest rotation that keeps every point inside
        let [beg0, end0, beg1, end1] = bounds;
        let pos0 = min2(end0.min, beg0.min + PI);
        let neg0 = max2(end0.max - PI, beg0.max);
        let pos1 = min2(end1.min + PI, beg1.min);
        let neg1 = max2(end1.max, beg1.max - PI);
        let turn = if min2(pos0, pos1) > -max2(neg0, neg1) {
            min2(pos0, pos1)
        } else {
            max2(neg0, neg1)
        };
        match rotated(&best, plane_normal, turn) {
            Some(b) if b.xy_area() < area => b,
            _ => best,
        }
    }
}

/// One boundary plane of a box in the hull-edge alignment: the last point on it, how many
/// are, that point projected, the direction angles are measured from, and the angles seen.
#[derive(Clone, Copy)]
struct Bound {
    index: usize,
    found: usize,
    end: Point3,
    reference: Vec3,
    ang: f64,
    min: f64,
    max: f64,
}

impl Bound {
    const fn new(reference: Vec3) -> Self {
        Self {
            index: 0,
            found: 0,
            end: Point3::ORIGIN,
            reference,
            ang: 0.0,
            min: BIG_DOUBLE,
            max: -BIG_DOUBLE,
        }
    }
}

/// Upstream's `SM_MIN2`: `b` unless `a < b`.
fn min2(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}
/// Upstream's `SM_MAX2`: `b` unless `a > b`.
fn max2(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// A closed loop of points, each joined to the next and the last to the first
/// (`SmPointSequence` with `bClosed`).
#[derive(Debug, Clone, PartialEq)]
pub struct PointLoop {
    set: PointSet,
}

impl PointLoop {
    /// The loop through the set's points, in order.
    #[must_use]
    pub const fn new(set: PointSet) -> Self {
        Self { set }
    }

    /// The set of the loop's points.
    #[must_use]
    pub const fn set(&self) -> &PointSet {
        &self.set
    }

    /// The signed area enclosed, projected along the set's normal, positive
    /// counter-clockwise (`GetArea`); 0 for no points.
    #[must_use]
    pub fn area(&self) -> f64 {
        polygon_area(&self.set.points, self.set.normal())
    }

    /// The quadrant of `test - origin` in the set's `x`/`y` axes (`CalcQuadNum`): 0 for
    /// `+x+y`, 1 for `-x+y`, 2 for `-x-y`, 3 for `+x-y`.
    fn quadrant(&self, origin: Point3, test: Point3) -> i32 {
        let v = test - origin;
        let [x, y, _] = self.set.frame;
        match (v.dot(*x) > 0.0, v.dot(*y) > 0.0) {
            (true, true) => 0,
            (true, false) => 3,
            (false, true) => 1,
            (false, false) => 2,
        }
    }

    /// Where `point` is against the loop, by its winding number (`PointContainment`).
    #[must_use]
    pub fn containment(&self, point: Point3) -> PointContainment {
        let tol = self.set.tol_3d;
        let points = &self.set.points;
        let (Some(bbox), Some(&last)) = (self.set.bbox, points.last()) else {
            return PointContainment::Outside;
        };
        if !bbox.contains_point(point, tol) {
            return PointContainment::Outside;
        }
        let normal = self.set.normal();
        let mut this_pt = last;
        let mut prev_quad = self.quadrant(point, this_pt);
        let mut accum = 0;
        for &p in points {
            let prev_pt = this_pt;
            this_pt = p;
            if this_pt.closer_than(tol, point) {
                return PointContainment::OnBoundary;
            }
            let this_quad = self.quadrant(point, this_pt);
            let delta = this_quad - prev_quad;
            if delta != 0 {
                if segment_point_distance(prev_pt, this_pt, point).0 <= tol {
                    return PointContainment::OnBoundary;
                }
                accum += match delta {
                    1 | -3 => 1,
                    -1 | 3 => -1,
                    // a diagonal step: clockwise or counter-clockwise about the point
                    _ if (prev_pt - point).cross(this_pt - point).dot(*normal) > 0.0 => 2,
                    _ => -2,
                };
            }
            prev_quad = this_quad;
        }
        match accum {
            -1..=1 => PointContainment::Outside,
            -5..=-3 | 3..=5 => PointContainment::Inside,
            _ => PointContainment::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn a_square_loop_contains_its_middle() -> Result<()> {
        let pts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
        ];
        let lp = PointLoop::new(PointSet::with_normal(pts, 1e-9, UnitVec3::Z));
        assert_eq!(lp.containment(p(0.5, 0.5, 0.0)), PointContainment::Inside);
        assert_eq!(lp.containment(p(2.0, 0.5, 0.0)), PointContainment::Outside);
        assert_eq!(
            lp.containment(p(1.0, 0.5, 0.0)),
            PointContainment::OnBoundary
        );
        assert!((lp.area() - 1.0).abs() < 1e-15);
        let set = PointSet::new(lp.set().points().to_vec(), 1e-9)?;
        assert_eq!(set.set_type(), PointSetType::Planar);
        Ok(())
    }

    #[test]
    fn a_tight_box_is_no_larger() -> Result<()> {
        let pts = vec![
            p(0.0, 0.0, 0.0),
            p(3.0, 1.0, 0.0),
            p(2.5, 2.5, 0.0),
            p(-0.5, 1.5, 0.0),
        ];
        let set = PointSet::new(pts, 1e-9)?;
        let (Some(b), Some(t)) = (set.pbox(), set.tight_pbox()) else {
            panic!("four points have boxes");
        };
        assert!(t.xy_area() <= b.xy_area());
        Ok(())
    }
}
