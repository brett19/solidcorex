// Written for solidcorex. Apache-2.0.
//! Tensor interpolation and least squares of NURBS surfaces at given parameters (CN3).
//!
//! A surface through a grid of points, or through compatible curves ("rows") at given values
//! of the second parameter, is fitted in two passes of `scx_nurbs::fit`'s banded solves: the
//! data along `u` first (each a curve on one shared knot vector, so the results are
//! [`CompatibleCurves`]), then the control points of those curves along `v`. The two passes
//! are the whole fit: the tensor system is separable, so nothing larger is ever formed, and
//! the work is linear in the grid (one banded solve per line of each pass).
//!
//! | solidcore at the pin (`nurbs::srf_fit`) | here |
//! |---|---|
//! | `param_values`, `interp_params` | [`grid_parameters`] |
//! | `interp_given_knots` (the `n + p + 1` knot counts compared as `i64`) | [`interpolate_on`]; the counts are the knot vectors' and the data's, checked by the line fits |
//! | knot vectors by `interp_knot_vector` and `interpolate` in one | [`interpolate`] |
//! | `lst_sq_approx` (`n, m, p, q` by `i64`, the grid's rows overwritten when `n == 1`) | [`least_squares`] (net size and degrees as `usize`, the input untouched) |
//! | `func_interp` (a grid of values) | [`interpolate`] on a `D = 1` grid |
//! | none: the skin's column fit lives in `srf_adv` over `&mut [Curve]` | [`interpolate_rows`], [`least_squares_rows`] over [`CompatibleCurves`] |
//!
//! Rational data (rows with weights) are fitted in homogeneous space, `w P` and `w`, which are
//! both polynomial: the surface passes through the rows exactly (to rounding), and its
//! weights are whatever the fit gives (a weight that is not positive is a typed refusal,
//! never a clamp). A grid of points is polynomial.
//!
//! Not ported (decided): first derivatives at every grid point (`interp_first_derivs`, the
//! pin's tangent-prescribed skins), which need the twist at the corners and are CN4's with
//! its boundary conditions; closed surfaces; weighted least squares (the pin has none for
//! surfaces); `func_interp`'s own type (a surface function is a `D = 1` surface here).

use scx_nurbs::fit::{self, End, FitError, Param};
use scx_nurbs::{CompatibleCurves, KnotVector, NurbsError};

use scx_num::error::ErrorKind;

use core::fmt;

use crate::{Net, NurbsSurface, NurbsSurfaceError, ParamAxis};

/// Why a surface cannot be fitted.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum SurfaceFitError {
    /// A line fit along `axis` failed. A context frame: the cause is the `scx-nurbs` fit
    /// error, through [`std::error::Error::source`], and the kind is its kind.
    Fit {
        /// The direction of the failed fit (`U`: the rows of data, `V`: the columns).
        axis: ParamAxis,
        /// Why.
        cause: FitError,
    },
    /// The fitted lines are not valid compatible curves. A context frame over the error.
    Curves(NurbsError),
    /// The fitted control net is not a surface (for instance a fitted weight that is not
    /// positive). A context frame over the surface error.
    Surface(NurbsSurfaceError),
    /// An [`Edge::Held`] has `count` conditions for `lines` lines of the net.
    EdgeCount {
        /// The conditions given.
        count: usize,
        /// The lines of the net, one condition each.
        lines: usize,
    },
    /// The conditions of an [`Edge::Held`] are not all of one order, so the lines would have
    /// different numbers of control points.
    EdgeOrders,
    /// Derivative conditions on rational rows: the conditions are on the polynomial
    /// homogeneous part and no geometric meaning is guessed. Fit the polynomial rows, or
    /// use [`Edge::Point`].
    RationalEnds,
    /// A step failed through the module's own fault. A bug.
    Defect {
        /// The step.
        why: &'static str,
    },
}

impl SurfaceFitError {
    /// What the caller can do about it (research 62 §1.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Fit { cause, .. } => cause.kind(),
            Self::Curves(e) => e.kind(),
            Self::Surface(e) => e.kind(),
            Self::EdgeCount { .. } | Self::EdgeOrders | Self::RationalEnds => ErrorKind::Refused,
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Fit { .. } => "surface_fit_line",
            Self::Curves(_) => "surface_fit_curves",
            Self::Surface(_) => "surface_fit_surface",
            Self::EdgeCount { .. } => "surface_fit_edge_count",
            Self::EdgeOrders => "surface_fit_edge_orders",
            Self::RationalEnds => "surface_fit_rational_ends",
            Self::Defect { .. } => "surface_fit_defect",
        }
    }
}

impl fmt::Display for SurfaceFitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fit { axis, .. } => write!(f, "the fit along {axis} failed"),
            Self::Curves(_) => f.write_str("the fitted lines are not compatible curves"),
            Self::Surface(_) => f.write_str("the fitted net is not a surface"),
            Self::EdgeCount { count, lines } => {
                write!(f, "{count} end conditions for {lines} lines")
            }
            Self::EdgeOrders => f.write_str("the end conditions are not all of one order"),
            Self::RationalEnds => {
                f.write_str("derivative end conditions on rational rows are not supported")
            }
            Self::Defect { why } => write!(f, "internal defect: {why}"),
        }
    }
}

impl std::error::Error for SurfaceFitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Fit { cause, .. } => Some(cause),
            Self::Curves(e) => Some(e),
            Self::Surface(e) => Some(e),
            _ => None,
        }
    }
}

type Result<T> = core::result::Result<T, SurfaceFitError>;

/// What the fit across the rows (the `v` direction) is told about one edge of the surface, in
/// addition to passing through the first (last) row.
///
/// The conditions are on the net's lines: one [`End`] per control point of a row, in the
/// parametric derivative of that control column with respect to `v` (the derivative
/// control polygon of the surface's boundary derivative; for a polynomial surface the
/// derivative `S_v(u, v0) = sum_i N_i(u) D_i`). They must all have one order.
#[derive(Debug, Clone, PartialEq)]
pub enum Edge<const D: usize> {
    /// G0: the row only.
    Point,
    /// One condition for each control point of a row.
    Held(Vec<End<D>>),
}

impl<const D: usize> Edge<D> {
    /// The order of the conditions, once there is one for each of `lines`.
    fn order(&self, lines: usize) -> Result<usize> {
        match self {
            Self::Point => Ok(0),
            Self::Held(v) => {
                if v.len() != lines {
                    return Err(SurfaceFitError::EdgeCount {
                        count: v.len(),
                        lines,
                    });
                }
                let order = v.first().map_or(0, End::order);
                if v.iter().any(|e| e.order() != order) {
                    return Err(SurfaceFitError::EdgeOrders);
                }
                Ok(order)
            }
        }
    }

    /// The condition for line `i`.
    fn at(&self, i: usize) -> End<D> {
        match self {
            Self::Point => End::Point,
            Self::Held(v) => v.get(i).copied().unwrap_or(End::Point),
        }
    }
}

/// The parameters `(u, v)` of the grid's rows and columns: each line's parameters by `method`
/// ([`fit::parameters`]), averaged over the lines of the direction (the pin's `param_values`).
/// A line whose points all coincide has no chord parameters and is left out of the average.
///
/// # Errors
///
/// [`SurfaceFitError::Fit`] over `TooFewPoints` for a single row or column, and over
/// `ZeroLength` when every line of a direction is a point.
pub fn grid_parameters<const D: usize>(
    grid: &Net<[f64; D]>,
    method: Param,
) -> Result<(Vec<f64>, Vec<f64>)> {
    let along_u: Vec<Vec<[f64; D]>> = (0..grid.ncols())
        .filter_map(|j| grid.column(j).map(|c| c.copied().collect()))
        .collect();
    let along_v: Vec<Vec<[f64; D]>> = grid.rows().map(<[[f64; D]]>::to_vec).collect();
    Ok((
        average(&along_u, method, ParamAxis::U)?,
        average(&along_v, method, ParamAxis::V)?,
    ))
}

fn average<const D: usize>(
    lines: &[Vec<[f64; D]>],
    method: Param,
    axis: ParamAxis,
) -> Result<Vec<f64>> {
    let mut sum: Vec<f64> = Vec::new();
    let mut used = 0_usize;
    for line in lines {
        match fit::parameters(line, method) {
            Ok(t) => {
                if sum.is_empty() {
                    sum = vec![0.0; t.len()];
                }
                for (s, x) in sum.iter_mut().zip(&t) {
                    *s += x;
                }
                used += 1;
            }
            Err(FitError::ZeroLength) => {}
            Err(cause) => return Err(SurfaceFitError::Fit { axis, cause }),
        }
    }
    if used == 0 {
        return Err(SurfaceFitError::Fit {
            axis,
            cause: FitError::ZeroLength,
        });
    }
    Ok(sum.iter().map(|s| s / used as f64).collect())
}

/// The surface of degrees `(p, q)` through `grid` at the parameters `u` (rows of the grid)
/// and `v` (columns): `S(u[i], v[j]) = grid[i][j]` to rounding. The knot vectors are the
/// averages of the parameters ([`scx_nurbs::fit::averaging`]), `grid.nrows()` by
/// `grid.ncols()` control points.
///
/// # Errors
///
/// [`SurfaceFitError::Fit`] with the line fit's cause: too few points for the degree,
/// parameters that do not increase, a count of parameters that is not the grid's.
pub fn interpolate<const D: usize>(
    grid: &Net<[f64; D]>,
    u: &[f64],
    v: &[f64],
    degrees: (usize, usize),
) -> Result<NurbsSurface<D>> {
    let ku = fit::averaging(u, degrees.0, 0, 0).map_err(along(ParamAxis::U))?;
    let kv = fit::averaging(v, degrees.1, 0, 0).map_err(along(ParamAxis::V))?;
    interpolate_on(grid, &ku, &kv, u, v)
}

/// [`interpolate`] on the given knot vectors, which must have `grid.nrows()` and
/// `grid.ncols()` control points, clamped, with `u` and `v` ending on their domains.
///
/// # Errors
///
/// As [`interpolate`], and `KnotCount`, `EndsNotOnDomain` or `Singular` where the knots do
/// not suit the parameters.
pub fn interpolate_on<const D: usize>(
    grid: &Net<[f64; D]>,
    ku: &KnotVector,
    kv: &KnotVector,
    u: &[f64],
    v: &[f64],
) -> Result<NurbsSurface<D>> {
    let rows = fit_rows(grid, ku, u, |data| {
        fit::interpolate_on(ku, data, u, &End::Point, &End::Point).map(|s| s.points().to_vec())
    })?;
    interpolate_rows_on(&rows, kv, v, &Edge::Point, &Edge::Point)
}

/// The surface of degree `degree` along `v` through the curves `rows` at the parameters `v`:
/// `S(., v[k])` is row `k`, and the surface has `rows.knot_vector()` along `u`. `start` and
/// `end` add derivative conditions at the first and last row ([`Edge`]). Rational rows give
/// a rational surface (homogeneous fit, see the module).
///
/// This is skinning (CN4) with the parameters given. It does `rows.knot_vector().point_count()`
/// banded solves of `rows.count()` (+ order) unknowns each, twice for rational rows.
///
/// # Errors
///
/// As [`interpolate`]; [`SurfaceFitError::EdgeCount`], [`SurfaceFitError::EdgeOrders`] and
/// [`SurfaceFitError::RationalEnds`] for the edge conditions;
/// [`SurfaceFitError::Surface`] if a fitted weight is not positive.
pub fn interpolate_rows<const D: usize>(
    rows: &CompatibleCurves<D>,
    v: &[f64],
    degree: usize,
    start: &Edge<D>,
    end: &Edge<D>,
) -> Result<NurbsSurface<D>> {
    let lines = rows.knot_vector().point_count();
    let (a, b) = (start.order(lines)?, end.order(lines)?);
    let kv = fit::averaging(v, degree, a, b).map_err(along(ParamAxis::V))?;
    interpolate_rows_on(rows, &kv, v, start, end)
}

/// [`interpolate_rows`] on the given knot vector, which must give
/// `rows.count() + start.order + end.order` control points and be clamped on `v`'s ends.
///
/// # Errors
///
/// As [`interpolate_rows`].
pub fn interpolate_rows_on<const D: usize>(
    rows: &CompatibleCurves<D>,
    kv: &KnotVector,
    v: &[f64],
    start: &Edge<D>,
    end: &Edge<D>,
) -> Result<NurbsSurface<D>> {
    let lines = rows.knot_vector().point_count();
    let (a, b) = (start.order(lines)?, end.order(lines)?);
    if rows.is_rational() && (a, b) != (0, 0) {
        return Err(SurfaceFitError::RationalEnds);
    }
    let count = rows.count();
    let solve = |i: usize, data: &[[f64; D]]| -> core::result::Result<Vec<[f64; D]>, FitError> {
        fit::interpolate_on(kv, data, v, &start.at(i), &end.at(i)).map(|s| s.points().to_vec())
    };
    let solve_w = |data: &[[f64; 1]]| -> core::result::Result<Vec<[f64; 1]>, FitError> {
        fit::interpolate_on(kv, data, v, &End::Point, &End::Point).map(|s| s.points().to_vec())
    };
    assemble(rows, kv, count, a + b + count, solve, solve_w)
}

/// The surface of degrees `(p, q)` with `points = (nu, nv)` control points closest to `grid`
/// in the least-squares sense at the parameters `u` and `v`, with the four corners held (the
/// pin's `lst_sq_approx`); knot vectors by [`scx_nurbs::fit::approximation`]. The fit is
/// separable, so the result is the least-squares surface with the boundary rows and columns
/// fitted to the boundary data (each by its own least squares through its end points), which
/// is the pin's.
///
/// Data on a surface of the degrees and these knots give it back.
///
/// # Errors
///
/// As [`interpolate`]; `TooFewControlPoints` when a net size is not above the degree and
/// below the data count; `Singular` for a span without data.
pub fn least_squares<const D: usize>(
    grid: &Net<[f64; D]>,
    u: &[f64],
    v: &[f64],
    degrees: (usize, usize),
    points: (usize, usize),
) -> Result<NurbsSurface<D>> {
    let ku = fit::approximation(u, degrees.0, points.0).map_err(along(ParamAxis::U))?;
    let kv = fit::approximation(v, degrees.1, points.1).map_err(along(ParamAxis::V))?;
    least_squares_on(grid, &ku, &kv, u, v)
}

/// [`least_squares`] on the given clamped knot vectors.
///
/// # Errors
///
/// As [`least_squares`].
pub fn least_squares_on<const D: usize>(
    grid: &Net<[f64; D]>,
    ku: &KnotVector,
    kv: &KnotVector,
    u: &[f64],
    v: &[f64],
) -> Result<NurbsSurface<D>> {
    let rows = fit_rows(grid, ku, u, |data| {
        fit::least_squares(ku, data, u, &End::Point, &End::Point).map(|s| s.points().to_vec())
    })?;
    least_squares_rows_on(&rows, kv, v)
}

/// The surface closest to the curves `rows` at the parameters `v` in the least-squares sense,
/// of degree `degree` along `v` with `points` control points, through the first and last
/// row. Rational rows are fitted in homogeneous space.
///
/// # Errors
///
/// As [`least_squares`].
pub fn least_squares_rows<const D: usize>(
    rows: &CompatibleCurves<D>,
    v: &[f64],
    degree: usize,
    points: usize,
) -> Result<NurbsSurface<D>> {
    let kv = fit::approximation(v, degree, points).map_err(along(ParamAxis::V))?;
    least_squares_rows_on(rows, &kv, v)
}

/// [`least_squares_rows`] on the given clamped knot vector.
///
/// # Errors
///
/// As [`least_squares`].
pub fn least_squares_rows_on<const D: usize>(
    rows: &CompatibleCurves<D>,
    kv: &KnotVector,
    v: &[f64],
) -> Result<NurbsSurface<D>> {
    let solve = |_: usize, data: &[[f64; D]]| -> core::result::Result<Vec<[f64; D]>, FitError> {
        fit::least_squares(kv, data, v, &End::Point, &End::Point).map(|s| s.points().to_vec())
    };
    let solve_w = |data: &[[f64; 1]]| -> core::result::Result<Vec<[f64; 1]>, FitError> {
        fit::least_squares(kv, data, v, &End::Point, &End::Point).map(|s| s.points().to_vec())
    };
    assemble(rows, kv, rows.count(), kv.point_count(), solve, solve_w)
}

fn along(axis: ParamAxis) -> impl Fn(FitError) -> SurfaceFitError {
    move |cause| SurfaceFitError::Fit { axis, cause }
}

/// Each column of the grid (the points along `u`) fitted by `solve`, as `grid.ncols()`
/// compatible curves on `ku`.
fn fit_rows<const D: usize>(
    grid: &Net<[f64; D]>,
    ku: &KnotVector,
    u: &[f64],
    solve: impl Fn(&[[f64; D]]) -> core::result::Result<Vec<[f64; D]>, FitError>,
) -> Result<CompatibleCurves<D>> {
    if u.len() != grid.nrows() {
        return Err(SurfaceFitError::Fit {
            axis: ParamAxis::U,
            cause: FitError::ParameterCount {
                count: u.len(),
                points: grid.nrows(),
            },
        });
    }
    let mut points = Vec::new();
    for j in 0..grid.ncols() {
        let data: Vec<[f64; D]> =
            grid.column(j)
                .map(|c| c.copied().collect())
                .ok_or(SurfaceFitError::Defect {
                    why: "a grid column is missing",
                })?;
        points.extend(solve(&data).map_err(along(ParamAxis::U))?);
    }
    CompatibleCurves::new(ku.clone(), grid.ncols(), points, None).map_err(SurfaceFitError::Curves)
}

/// The columns of `rows`' control net fitted by `solve` (`solve_w` for the weights), `cols`
/// control points each, as a surface on `rows`' knots along `u` and `kv` along `v`.
fn assemble<const D: usize>(
    rows: &CompatibleCurves<D>,
    kv: &KnotVector,
    count: usize,
    cols: usize,
    solve: impl Fn(usize, &[[f64; D]]) -> core::result::Result<Vec<[f64; D]>, FitError>,
    solve_w: impl Fn(&[[f64; 1]]) -> core::result::Result<Vec<[f64; 1]>, FitError>,
) -> Result<NurbsSurface<D>> {
    let ku = rows.knot_vector();
    let n = ku.point_count();
    let net = |data: &[[f64; D]]| -> Result<Net<[f64; D]>> {
        let mut out = Vec::with_capacity(n * cols);
        for i in 0..n {
            let column: Vec<[f64; D]> = data
                .chunks_exact(n)
                .filter_map(|r| r.get(i).copied())
                .collect();
            if column.len() != count {
                return Err(SurfaceFitError::Defect {
                    why: "a row of the net is short",
                });
            }
            out.extend(solve(i, &column).map_err(along(ParamAxis::V))?);
        }
        Net::new(n, cols, out).map_err(SurfaceFitError::Surface)
    };
    let surface = SurfaceFitError::Surface;
    match (rows.weighted_points(), rows.weights()) {
        (Some(wp), Some(w)) => {
            let weighted = net(wp)?;
            let mut ws = Vec::with_capacity(n * cols);
            for i in 0..n {
                let column: Vec<[f64; 1]> = w
                    .chunks_exact(n)
                    .filter_map(|r| r.get(i).map(|&x| [x]))
                    .collect();
                ws.extend(solve_w(&column).map_err(along(ParamAxis::V))?);
            }
            let weights = Net::new(n, cols, ws.iter().map(|w| w[0]).collect()).map_err(surface)?;
            NurbsSurface::from_homogeneous(ku.clone(), kv.clone(), weighted, weights)
                .map_err(surface)
        }
        _ => NurbsSurface::new(ku.clone(), kv.clone(), net(rows.points())?, None).map_err(surface),
    }
}
