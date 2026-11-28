// Written for solidcorex. Apache-2.0.
//! [`CompatibleSurfaces`]: surfaces made to share one knot vector and degree along an axis.
//!
//! The pin's `srf_tool::make_compatible` moves knots of different surfaces that lie within
//! a tolerance of each other to their average, and leaves the surfaces "as far as upstream
//! got" on an error (77 §4.2). Here the lines of every surface along the axis are made
//! compatible together by [`CompatibleCurves::from_curves`](scx_nurbs::CompatibleCurves),
//! exactly: the parameter of every surface is mapped affinely onto the first surface's
//! domain along the axis, degrees are raised to the highest, the union of the knots is
//! inserted, and nothing is moved. The inputs are borrowed; an error changes nothing.

use scx_nurbs::{CompatibleCurves, KnotVector, NurbsCurve, NurbsError};

use crate::{NurbsSurface, NurbsSurfaceError, ParamAxis, Result};

/// Surfaces that share one knot vector, and so one degree, along `axis`; the other
/// direction of each is its own.
///
/// Built only by [`CompatibleSurfaces::from_surfaces`]. Surface `i` is the geometry of
/// source `i` with the `axis` parameter mapped affinely onto the first source's domain
/// along `axis`.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibleSurfaces<const D: usize> {
    axis: ParamAxis,
    knots: KnotVector,
    surfaces: Vec<NurbsSurface<D>>,
}

/// The compatible lines do not form surfaces.
const SPLIT: NurbsSurfaceError = NurbsSurfaceError::Defect {
    why: "the compatible lines do not split back into surfaces",
};

impl<const D: usize> CompatibleSurfaces<D> {
    /// `surfaces` made compatible along `axis`: one knot vector there (the union of the
    /// knots, at the highest degree, on the first surface's domain), every surface the same
    /// geometry as its source with that parameter relabelled. The result is rational if any
    /// source is, with unit weights on the polynomial ones. Polynomial sources that already share
    /// their `axis` knot vector come back bit for bit.
    ///
    /// # Errors
    ///
    /// [`NurbsSurfaceError::NoSurfaces`] for an empty slice; [`NurbsSurfaceError::Edit`] over
    /// the cause [`CompatibleCurves::from_curves`](scx_nurbs::CompatibleCurves::from_curves)
    /// gives for the lines (`Unclamped` when work is needed and an end knot is not of full
    /// multiplicity, `Unstable` or `Degree` from the elevation, `Multiplicity` where the
    /// mapping rounds two knots together).
    pub fn from_surfaces(axis: ParamAxis, surfaces: &[NurbsSurface<D>]) -> Result<Self> {
        if surfaces.is_empty() {
            return Err(NurbsSurfaceError::NoSurfaces);
        }
        let edit = |cause| NurbsSurfaceError::Edit { axis, cause };
        // Every line of every surface: the lines of one surface share their knots already,
        // so the union is the same as over one line each, and each gets the same edit.
        let mut curves: Vec<NurbsCurve<D>> = Vec::new();
        let mut counts = Vec::with_capacity(surfaces.len());
        for s in surfaces {
            let lines = s.lines(axis)?;
            counts.push(lines.count());
            curves.extend((0..lines.count()).filter_map(|i| lines.curve(i)));
        }
        let all = CompatibleCurves::from_curves(&curves).map_err(edit)?;
        let (knots, points, rational) = all.into_parts();
        let n = knots.point_count();
        let mut out = Vec::with_capacity(surfaces.len());
        let mut start = 0;
        for (s, &count) in surfaces.iter().zip(&counts) {
            let range = start * n..(start + count) * n;
            start += count;
            let lines = piece(&knots, count, &points, rational.as_ref(), range).map_err(edit)?;
            out.push(s.with_lines(axis, lines)?);
        }
        if start * n != points.len() {
            return Err(SPLIT);
        }
        Ok(Self {
            axis,
            knots,
            surfaces: out,
        })
    }

    /// The axis the surfaces are compatible along.
    #[must_use]
    pub const fn axis(&self) -> ParamAxis {
        self.axis
    }

    /// The knot vector every surface has along [`Self::axis`].
    #[must_use]
    pub fn knot_vector(&self) -> &KnotVector {
        &self.knots
    }

    /// The number of surfaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.surfaces.len()
    }

    /// Always false: there is at least one surface.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    /// The surfaces, in the order of the sources.
    #[must_use]
    pub fn surfaces(&self) -> &[NurbsSurface<D>] {
        &self.surfaces
    }

    /// Surface `i`.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<&NurbsSurface<D>> {
        self.surfaces.get(i)
    }

    /// The surfaces.
    #[must_use]
    pub fn into_surfaces(self) -> Vec<NurbsSurface<D>> {
        self.surfaces
    }
}

/// The `count` lines at `range` of the values `all` as compatible curves on `knots`.
fn piece<const D: usize>(
    knots: &KnotVector,
    count: usize,
    points: &[[f64; D]],
    rational: Option<&scx_nurbs::Homogeneous<D>>,
    range: core::ops::Range<usize>,
) -> core::result::Result<CompatibleCurves<D>, NurbsError> {
    let slice = |v: &[[f64; D]]| v.get(range.clone()).map(<[_]>::to_vec).unwrap_or_default();
    match rational {
        None => CompatibleCurves::new(knots.clone(), count, slice(points), None),
        Some(h) => CompatibleCurves::from_homogeneous(
            knots.clone(),
            count,
            slice(&h.weighted),
            h.weights.get(range).map(<[_]>::to_vec).unwrap_or_default(),
        ),
    }
}
