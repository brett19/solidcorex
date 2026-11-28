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
//! Derivative conditions at the first and last row ([`Edge`]) hold on rational rows too, in
//! one of two stated meanings: [`Edge::Held`] gives the boundary derivative as the rational
//! curve with the given control points on the row's weights (the weights' own derivatives
//! across the edge are held at zero), and [`Edge::Homogeneous`] gives the derivatives of the
//! homogeneous columns `w P` and `w` themselves (the pin's meaning; C1 with a rational
//! neighbour whose homogeneous net is known).
//!
//! Not ported (decided): first derivatives at every grid point (`interp_first_derivs`) and
//! the corner twists they need (no row asks for a grid with prescribed slopes: MK12's ends
//! are [`Edge`]s); closed surfaces here (the closed skin is [`crate::skin`]'s); weighted
//! least squares (the pin has none for surfaces); `func_interp`'s own type (a surface
//! function is a `D = 1` surface here).

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
    /// The conditions of an [`Edge::Held`] or [`Edge::Homogeneous`] are not all of one order,
    /// so the lines would have different numbers of control points.
    EdgeOrders,
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
            Self::EdgeCount { .. } | Self::EdgeOrders => ErrorKind::Refused,
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
/// parametric derivative of that control column with respect to `v`. They must all have one
/// order.
#[derive(Debug, Clone, PartialEq)]
pub enum Edge<const D: usize> {
    /// G0: the row only.
    Point,
    /// One condition for each control point of a row: the derivative control polygon of the
    /// surface's boundary derivative. On polynomial rows `S_v(u, v0) = sum_i N_i(u) D_i`; on
    /// rational rows the boundary derivatives are the rational curves with control points
    /// `D_i` on the row's weights, `S_v(u, v0) = sum_i N_i(u) w_i D_i / sum_i N_i(u) w_i`
    /// (and so for the second and third), which the fit makes exact by holding the
    /// homogeneous derivatives at `w_i D_i` and the weights' derivatives at zero.
    Held(Vec<End<D>>),
    /// The derivatives of the homogeneous control columns, `(w P)_i` in `points` and `w_i` in
    /// `weights`, one of each per control point of a row, all of one order: a surface
    /// matching a rational neighbour's homogeneous net across the edge is C1 (C2, C3) with
    /// it. Polynomial rows are taken with unit weights, so the surface is rational.
    Homogeneous {
        /// `(w P)'` (and higher) of each control column.
        points: Vec<End<D>>,
        /// `w'` (and higher) of each control column.
        weights: Vec<End<1>>,
    },
}

impl<const D: usize> Edge<D> {
    /// The order of the conditions, once there is one for each of `lines`.
    pub(crate) fn order(&self, lines: usize) -> Result<usize> {
        let one_order = |orders: &mut dyn Iterator<Item = usize>| -> Result<usize> {
            let first = orders.next().unwrap_or(0);
            for o in orders {
                if o != first {
                    return Err(SurfaceFitError::EdgeOrders);
                }
            }
            Ok(first)
        };
        let count = |n: usize| -> Result<()> {
            if n == lines {
                Ok(())
            } else {
                Err(SurfaceFitError::EdgeCount { count: n, lines })
            }
        };
        match self {
            Self::Point => Ok(0),
            Self::Held(v) => {
                count(v.len())?;
                one_order(&mut v.iter().map(End::order))
            }
            Self::Homogeneous { points, weights } => {
                count(points.len())?;
                count(weights.len())?;
                one_order(
                    &mut points
                        .iter()
                        .map(End::order)
                        .chain(weights.iter().map(End::order)),
                )
            }
        }
    }

    /// Whether the edge asks for a rational surface.
    pub(crate) const fn is_homogeneous(&self) -> bool {
        matches!(self, Self::Homogeneous { .. })
    }

    /// The condition for line `i` of a polynomial fit.
    fn at(&self, i: usize) -> End<D> {
        match self {
            Self::Point => End::Point,
            Self::Held(v) | Self::Homogeneous { points: v, .. } => {
                v.get(i).copied().unwrap_or(End::Point)
            }
        }
    }

    /// The condition on the homogeneous column `(w P)_i` of a rational fit, `w` the weight of
    /// the edge row's control point `i`.
    fn numerator(&self, i: usize, w: f64) -> End<D> {
        match self {
            Self::Point => End::Point,
            Self::Held(v) => v.get(i).map_or(End::Point, |e| scaled(e, w)),
            Self::Homogeneous { points, .. } => points.get(i).copied().unwrap_or(End::Point),
        }
    }

    /// The condition on the weight column `w_i` of a rational fit.
    fn weight(&self, i: usize) -> End<1> {
        match self {
            Self::Point => End::Point,
            Self::Held(v) => v.get(i).map_or(End::Point, zeros),
            Self::Homogeneous { weights, .. } => weights.get(i).copied().unwrap_or(End::Point),
        }
    }
}

/// `e` with every derivative times `s`.
fn scaled<const N: usize>(e: &End<N>, s: f64) -> End<N> {
    let m = |d: [f64; N]| d.map(|x| x * s);
    match *e {
        End::Point => End::Point,
        End::Tangent { d1 } => End::Tangent { d1: m(d1) },
        End::Curvature { d1, d2 } => End::Curvature {
            d1: m(d1),
            d2: m(d2),
        },
        End::Hermite { d1, d2, d3 } => End::Hermite {
            d1: m(d1),
            d2: m(d2),
            d3: m(d3),
        },
    }
}

/// A one-dimensional condition of `e`'s order, every derivative 0.
fn zeros<const N: usize>(e: &End<N>) -> End<1> {
    let z = [0.0];
    match e {
        End::Point => End::Point,
        End::Tangent { .. } => End::Tangent { d1: z },
        End::Curvature { .. } => End::Curvature { d1: z, d2: z },
        End::Hermite { .. } => End::Hermite {
            d1: z,
            d2: z,
            d3: z,
        },
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
/// `end` add derivative conditions at the first and last row ([`Edge`]). Rational rows (or an
/// [`Edge::Homogeneous`]) give a rational surface (homogeneous fit, see the module).
///
/// This is skinning (CN4) with the parameters given. It does `rows.knot_vector().point_count()`
/// banded solves of `rows.count()` (+ order) unknowns each, twice for rational rows.
///
/// # Errors
///
/// As [`interpolate`]; [`SurfaceFitError::EdgeCount`] and [`SurfaceFitError::EdgeOrders`]
/// for the edge conditions;
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
    let count = rows.count();
    let cols = a + b + count;
    let homogeneous = rows.is_rational() || start.is_homogeneous() || end.is_homogeneous();
    if !homogeneous {
        let solve = |i: usize, data: &[[f64; D]]| {
            fit::interpolate_on(kv, data, v, &start.at(i), &end.at(i)).map(|s| s.points().to_vec())
        };
        return assemble(rows, kv, count, cols, solve, None, |_, _| {
            Err(FitError::Defect {
                why: "a polynomial fit has no weights",
            })
        });
    }
    let unit: Vec<f64>;
    let (wp, w) = if let (Some(wp), Some(w)) = (rows.weighted_points(), rows.weights()) {
        (wp, w)
    } else {
        unit = vec![1.0; rows.points().len()];
        (rows.points(), unit.as_slice())
    };
    let first = w.get(..lines).unwrap_or_default();
    let last = w.get(lines * count.saturating_sub(1)..).unwrap_or_default();
    let weight_at = |row: &[f64], i: usize| row.get(i).copied().unwrap_or(1.0);
    let solve = |i: usize, data: &[[f64; D]]| {
        let s = start.numerator(i, weight_at(first, i));
        let e = end.numerator(i, weight_at(last, i));
        fit::interpolate_on(kv, data, v, &s, &e).map(|s| s.points().to_vec())
    };
    let solve_w = |i: usize, data: &[[f64; 1]]| {
        fit::interpolate_on(kv, data, v, &start.weight(i), &end.weight(i))
            .map(|s| s.points().to_vec())
    };
    assemble(rows, kv, count, cols, solve, Some((wp, w)), solve_w)
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
    let solve = |_: usize, data: &[[f64; D]]| {
        fit::least_squares(kv, data, v, &End::Point, &End::Point).map(|s| s.points().to_vec())
    };
    let solve_w = |_: usize, data: &[[f64; 1]]| {
        fit::least_squares(kv, data, v, &End::Point, &End::Point).map(|s| s.points().to_vec())
    };
    let homogeneous = rows.weighted_points().zip(rows.weights());
    assemble(
        rows,
        kv,
        rows.count(),
        kv.point_count(),
        solve,
        homogeneous,
        solve_w,
    )
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

/// The columns of `rows`' control net fitted by `solve`, `cols` control points each, as a
/// surface on `rows`' knots along `u` and `kv` along `v`; with `homogeneous` the columns of
/// those homogeneous points `w P` and weights `w` (`solve_w`) instead, as a rational surface.
fn assemble<const D: usize>(
    rows: &CompatibleCurves<D>,
    kv: &KnotVector,
    count: usize,
    cols: usize,
    solve: impl Fn(usize, &[[f64; D]]) -> core::result::Result<Vec<[f64; D]>, FitError>,
    homogeneous: Option<(&[[f64; D]], &[f64])>,
    solve_w: impl Fn(usize, &[[f64; 1]]) -> core::result::Result<Vec<[f64; 1]>, FitError>,
) -> Result<NurbsSurface<D>> {
    let ku = rows.knot_vector();
    let n = ku.point_count();
    let surface = SurfaceFitError::Surface;
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
        Net::new(n, cols, out).map_err(surface)
    };
    match homogeneous {
        Some((wp, w)) => {
            let weighted = net(wp)?;
            let mut ws = Vec::with_capacity(n * cols);
            for i in 0..n {
                let column: Vec<[f64; 1]> = w
                    .chunks_exact(n)
                    .filter_map(|r| r.get(i).map(|&x| [x]))
                    .collect();
                ws.extend(solve_w(i, &column).map_err(along(ParamAxis::V))?);
            }
            let weights = Net::new(n, cols, ws.iter().map(|w| w[0]).collect()).map_err(surface)?;
            NurbsSurface::from_homogeneous(ku.clone(), kv.clone(), weighted, weights)
                .map_err(surface)
        }
        None => {
            NurbsSurface::new(ku.clone(), kv.clone(), net(rows.points())?, None).map_err(surface)
        }
    }
}
