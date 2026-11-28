// Written for solidcorex. Apache-2.0.
//! Exact iso-curves and exact pieces of a NURBS surface.
//!
//! The pin's `srf_tool::extract_iso_crv` runs the de Boor triangle on the span of the
//! parameter, extrapolates the end spans for a parameter outside the domain, and refuses a
//! knot of multiplicity above the degree with a bare error. [`NurbsSurface::iso`] does the
//! same arithmetic as a knot insertion to full multiplicity (the lines of the net along the
//! fixed direction as [`CompatibleCurves`](scx_nurbs::CompatibleCurves), so the knot work is
//! done once and each line gets the curve insertion bit for bit), and then reads the
//! control points that lie on the surface. A parameter outside the domain is refused.
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `srf_tool::p1::extract_iso_crv(&Surface, t, Direction::U)`, the u-curve at `v = t` | [`NurbsSurface::iso`]`(ParamAxis::V, t)`: the axis is the parameter held fixed |
//! | `extract_iso_crv(.., Direction::V)`, the v-curve at `u = t` | `iso(ParamAxis::U, t)` |
//! | `extract_iso_crv(.., Direction::UV)`: `CallFailed` | none: the axis is a [`ParamAxis`] |
//! | a parameter outside the domain extrapolates | [`NurbsSurfaceError::Edit`] over `ParameterOutside` |
//! | `srf_tool::p1::extract_patch` (a box of parameters, the pin's own tolerances) | [`NurbsSurface::restrict`]: splits at the box's ends, no snapping |

use scx_num::extent2d::Extent2d;
use scx_nurbs::{NurbsCurve, NurbsError};

use crate::{NurbsSurface, NurbsSurfaceError, ParamAxis, RestrictCause, Result};

/// The curve built from a surface's lines is not valid.
const CURVE: NurbsSurfaceError = NurbsSurfaceError::Defect {
    why: "the iso-curve of a valid surface is not a valid curve",
};

/// The inserted knot is missing from the refined knot vector.
const KNOT: NurbsSurfaceError = NurbsSurfaceError::Defect {
    why: "the knot just inserted is not in the knot vector",
};

impl<const D: usize> NurbsSurface<D> {
    /// The curve of the surface where the parameter `axis` equals `t`, along the other
    /// parameter: `iso(U, t)` is `v -> S(t, v)` on the knots of v, `iso(V, t)` is
    /// `u -> S(u, t)` on the knots of u. Exact up to the rounding of the insertion: the
    /// knot `t` is inserted along `axis` until its multiplicity is the degree, where the
    /// surface passes through the control points, and those are the curve's. No
    /// parameter is moved.
    ///
    /// At an end of a clamped domain the curve is the boundary row of the net, copied. A
    /// rational surface gives a rational curve (homogeneous points, so the weights are the
    /// curve's own).
    ///
    /// # Errors
    ///
    /// [`NurbsSurfaceError::Edit`] over `ParameterNotFinite` or `ParameterOutside` for a `t`
    /// that is not in the domain of `axis`, and over [`NurbsError::Unclamped`] for an end of
    /// an unclamped domain, which no insertion reaches.
    pub fn iso(&self, axis: ParamAxis, t: f64) -> Result<NurbsCurve<D>> {
        let edit = |cause| NurbsSurfaceError::Edit { axis, cause };
        let kv = self.knot_vector(axis);
        let p = kv.degree();
        let (lo, hi) = kv.ends();
        let lines = self.lines(axis)?;
        let (lines, at) = if t == lo || t == hi {
            if !kv.is_clamped() {
                return Err(edit(NurbsError::Unclamped));
            }
            let at = if t == lo { 0 } else { kv.point_count() - 1 };
            (lines, at)
        } else {
            let times = p.saturating_sub(kv.multiplicity(t));
            // Checks `t` against the domain too, with no knots to add.
            let lines = lines.insert_knot(t, times).map_err(edit)?;
            // The knots at `t` are the last `p` before `k`: control point `k - p` is on the
            // surface.
            let k = lines
                .knot_vector()
                .knots()
                .iter()
                .rposition(|&x| x == t)
                .ok_or(KNOT)?;
            (lines, k - p)
        };
        let n = lines.knot_vector().point_count();
        let other = self.knot_vector(match axis {
            ParamAxis::U => ParamAxis::V,
            ParamAxis::V => ParamAxis::U,
        });
        let pick = |all: &[[f64; D]]| -> Vec<[f64; D]> {
            all.chunks_exact(n)
                .filter_map(|l| l.get(at).copied())
                .collect()
        };
        match (lines.weighted_points(), lines.weights()) {
            (Some(wp), Some(w)) => {
                let w: Vec<f64> = w
                    .chunks_exact(n)
                    .filter_map(|l| l.get(at).copied())
                    .collect();
                NurbsCurve::from_homogeneous(other.clone(), pick(wp), w)
            }
            _ => NurbsCurve::new(other.clone(), pick(lines.points()), None),
        }
        .map_err(|_| CURVE)
    }

    /// Every refusal of [`NurbsSurface::restrict`] that needs no work: the box inside the
    /// domain, and with an area.
    pub(crate) fn check_box(&self, uv: &Extent2d) -> Result<()> {
        let ranges = [
            (ParamAxis::U, uv.x_min(), uv.x_max()),
            (ParamAxis::V, uv.y_min(), uv.y_max()),
        ];
        // Every refusal before any work.
        for (axis, lo, hi) in ranges {
            let (a, b) = self.knot_vector(axis).ends();
            if !(a <= lo && hi <= b) {
                return Err(NurbsSurfaceError::Restrict(RestrictCause::Outside {
                    axis,
                    range: [lo, hi],
                    domain: [a, b],
                }));
            }
            if lo == hi {
                return Err(NurbsSurfaceError::Restrict(RestrictCause::Empty {
                    axis,
                    at: lo,
                }));
            }
        }
        Ok(())
    }

    /// The piece of the surface over the box `uv`, keeping the parameters: `S'(u, v) = S(u, v)`
    /// on `uv`, and the domain of `S'` is `uv` (the pin's `extract_patch`, without its
    /// tolerances). Each end of the box that is inside a domain is a split
    /// ([`NurbsSurface::split`]); an end on the domain's end is no work, so a box that is the
    /// domain returns a copy.
    ///
    /// The result has end knots of multiplicity `degree + 1` at every cut, and the knots of
    /// the source in between; nothing is snapped, so a cut a hair from a knot leaves both.
    ///
    /// # Errors
    ///
    /// [`NurbsSurfaceError::Restrict`] over [`RestrictCause::Outside`] when the box is not
    /// inside the domain, and [`RestrictCause::Empty`] when it has no width in a direction
    /// (take [`NurbsSurface::iso`]); the splits' own errors ([`NurbsSurfaceError::Edit`]),
    /// such as an unclamped end, as they come.
    pub fn restrict(&self, uv: &Extent2d) -> Result<Self> {
        self.check_box(uv)?;
        let ranges = [
            (ParamAxis::U, uv.x_min(), uv.x_max()),
            (ParamAxis::V, uv.y_min(), uv.y_max()),
        ];
        let mut piece = self.clone();
        for (axis, lo, hi) in ranges {
            let (a, b) = self.knot_vector(axis).ends();
            if a < lo {
                piece = piece.split(axis, lo)?.1;
            }
            if hi < b {
                piece = piece.split(axis, hi)?.0;
            }
        }
        Ok(piece)
    }
}
