// Written for solidcorex. Apache-2.0.
//! Skinning (CN4; 77 §4.3): the surface exactly through compatible section curves at given
//! parameters, with a certified bound of how far it misses them.
//!
//! [`skin`] takes the sections as [`CompatibleCurves`] (CN1 makes them so, exactly), one
//! parameter per section along `v`, the degree along `v`, and how the skin ends ([`Skin`]):
//!
//! * [`Skin::Open`]: CN3's [`interpolate_rows`] with an [`Edge`] at each end (the row only, or
//!   held boundary derivatives up to the third, polynomial or rational);
//! * [`Skin::Closed`]: the sections are a ring whose last is the first again; the caller says
//!   so (R5), and the skin measures it: the certified same-parameter distance of the first
//!   and last section ([`scx_nurbs::deviation::distance`]) above `coincidence` is
//!   [`SkinError::NotClosed`] with the gap, never a silent open skin. Within it, the last
//!   section is replaced by the first, so the seam's rows are equal bit for bit, and the
//!   surface is `C^(q-1)` across the seam (as it is across every simple knot) by exact
//!   linear conditions, not estimated derivatives.
//!
//! [`Skinned::through`] bounds `|S(u, v_k) - C_k(u)|` for every section `C_k` and every `u`,
//! every rounding of the solve included: the surface is `sum_i N_i(u) X_i(v)` with `X_i`
//! the curve of control column `i`, and the section is `sum_i N_i(u) P_ki`, so the distance
//! is at most the largest `|X_i(v_k) - P_ki|` (the basis is non-negative and sums to one),
//! which FT1's [`scx_nurbs::deviation::interpolation_bound`] certifies per column. Rational
//! skins bound the homogeneous columns and the weights the same way and divide (below). A
//! closed skin adds the closure gap. The surface between the sections is the definition (a
//! skin's shape there is a choice, not an approximation of something), so nothing else is
//! bounded.
//!
//! | solidcore at the pin (`nurbs::srf_adv`) | here |
//! |---|---|
//! | `create_skin_srf(cur: &mut [Curve], cmp, cld, deg, dir, knot_tol)` | [`skin`]: borrowed sections already compatible (CN1, no knot snapping), parameters given, no `dir` (the sections are the `u` rows; [`NurbsSurface::swapped`] turns it) |
//! | closure by `curves_are_coincident(.., MODEL_TOL)` and the `cld` flag | [`Skin::Closed`] with the caller's coincidence; refused with the gap if apart |
//! | closed: C1 with the seam tangent estimated from a shifted interpolation | `C^(q-1)` by exact seam conditions |
//! | `create_skin_srf_boundary_derivs`, `_continuity` (homogeneous derivative strips) | [`Skin::Open`] with [`Edge::Held`] (derivative polygons, on the row's weights when rational) or [`Edge::Homogeneous`] (the pin's homogeneous meaning) |
//! | weights clamped to `[1e-3, 100]` | a weight not positive is refused (`NurbsSurface` checks) |
//!
//! # Closed skins
//!
//! With `m + 1` sections (the last the first), degree `q` and `r = q - 1`, each control
//! column `y_0..y_m` (`y_m = y_0`) is interpolated on the knots by averaging with
//! `a = ceil(r / 2)` derivatives held at the start and `b = floor(r / 2)` at the end: the
//! seam's derivatives `d_1..d_a` are unknown, held at the start, and `d_1..d_b` held at the
//! end too; the `a` conditions `X^(k)(v_m) = X^(k)(v_0)`, `k = b + 1..=r`, fix them. The fit
//! is linear in `d`, and the `a x a` system's matrix depends on the knots and parameters
//! only, so it is formed once from `a` one-dimensional responses and each column costs one
//! banded solve and one `a x a` back substitution: the work of an open skin.

use core::fmt;

use scx_num::certified::gamma;
use scx_num::error::ErrorKind;
use scx_nurbs::deviation::{self, DeviationError};
use scx_nurbs::fit::{self, End, FitError, MAX_END_ORDER};
use scx_nurbs::{Approach, BasisDerivs, CompatibleCurves, KnotVector, NurbsCurve};

use crate::fit::{Edge, SurfaceFitError, interpolate_rows};
use crate::{Net, NurbsSurface, ParamAxis};

/// The highest degree along `v` a [`Skin::Closed`] takes: its seam holds `ceil((q - 1) / 2)`
/// derivatives at the start, at most [`MAX_END_ORDER`].
pub const MAX_CLOSED_DEGREE: usize = 2 * MAX_END_ORDER + 1;

/// Rows of basis derivatives at a seam end: up to the sixth (`r` for the highest closed
/// degree), plus the values.
const SEAM_ROWS: usize = MAX_CLOSED_DEGREE + 1;

/// How a skin ends along `v`.
#[derive(Debug, Clone, PartialEq)]
pub enum Skin<const D: usize> {
    /// The first and last sections are the surface's boundaries, with the conditions given
    /// at each ([`Edge`]).
    Open {
        /// At the first section.
        start: Edge<D>,
        /// At the last section.
        end: Edge<D>,
    },
    /// The sections are a ring, the last the first again: the surface is closed along `v`
    /// and as smooth across the seam as at its simple knots. The first and last section must
    /// be within `coincidence` of each other at every parameter (certified); the last is then
    /// taken as the first.
    Closed {
        /// The distance within which the first and last section count as one (the caller's
        /// coincidence, at least 0).
        coincidence: f64,
    },
}

/// A skin and how far it misses its sections.
#[derive(Debug, Clone, PartialEq)]
pub struct Skinned<const D: usize> {
    /// The surface: the sections are its `u` rows at the parameters given (`v`).
    pub surface: NurbsSurface<D>,
    /// A certified bound of `|S(u, v_k) - C_k(u)|` over every section `C_k` and every `u`,
    /// the rounding of the solve included; for a closed skin plus the closure gap.
    pub through: f64,
}

/// Why a skin cannot be made.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum SkinError {
    /// The fit across the sections failed. A context frame over the fit's error.
    Fit(SurfaceFitError),
    /// A [`Skin::Closed`] whose first and last sections are farther apart than the
    /// coincidence: `gap` is the certified distance bound that exceeded it.
    NotClosed {
        /// The certified bound of the distance of the first and last section.
        gap: f64,
        /// The coincidence given.
        coincidence: f64,
    },
    /// The coincidence of a [`Skin::Closed`] is negative or not finite.
    Coincidence {
        /// The value given.
        value: f64,
    },
    /// Too few sections: a closed skin needs two distinct sections and the first again.
    TooFewSections {
        /// The sections given.
        count: usize,
        /// The least number.
        needed: usize,
    },
    /// A [`Skin::Closed`] of a degree above [`MAX_CLOSED_DEGREE`].
    ClosedDegree {
        /// The degree asked for.
        degree: usize,
        /// The highest closed degree.
        most: usize,
    },
    /// The seam conditions of a closed skin have no unique solution for these parameters.
    Seam,
    /// The closure gap could not be bounded. A context frame over the deviation error.
    Gap(DeviationError),
    /// The bound of how far the skin misses its sections could not be formed. A context frame
    /// over the deviation error.
    Through(DeviationError),
    /// A rational skin's weights are within the bound's rounding of zero, so no bound holds.
    WeightsLost,
    /// A step failed through the module's own fault. A bug.
    Defect {
        /// The step.
        why: &'static str,
    },
}

impl SkinError {
    /// What the caller can do about it (research 62 §1.1).
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Fit(e) => e.kind(),
            Self::Gap(e) | Self::Through(e) => e.kind(),
            Self::NotClosed { .. }
            | Self::Coincidence { .. }
            | Self::TooFewSections { .. }
            | Self::Seam
            | Self::WeightsLost => ErrorKind::Refused,
            Self::ClosedDegree { .. } => ErrorKind::Unsupported,
            Self::Defect { .. } => ErrorKind::Defect,
        }
    }

    /// The cause's stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Fit(_) => "skin_fit",
            Self::NotClosed { .. } => "skin_not_closed",
            Self::Coincidence { .. } => "skin_coincidence",
            Self::TooFewSections { .. } => "skin_too_few_sections",
            Self::ClosedDegree { .. } => "skin_closed_degree",
            Self::Seam => "skin_seam",
            Self::Gap(_) => "skin_gap",
            Self::Through(_) => "skin_through",
            Self::WeightsLost => "skin_weights_lost",
            Self::Defect { .. } => "skin_defect",
        }
    }
}

impl fmt::Display for SkinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fit(_) => f.write_str("the fit across the sections failed"),
            Self::NotClosed { gap, coincidence } => write!(
                f,
                "the first and last sections are up to {gap} apart, above the coincidence {coincidence}"
            ),
            Self::Coincidence { value } => write!(f, "the coincidence {value} is not usable"),
            Self::TooFewSections { count, needed } => {
                write!(f, "{count} sections, at least {needed} needed")
            }
            Self::ClosedDegree { degree, most } => {
                write!(f, "a closed skin of degree {degree}, at most {most}")
            }
            Self::Seam => f.write_str("the seam conditions have no unique solution"),
            Self::Gap(_) => f.write_str("the closure gap could not be bounded"),
            Self::Through(_) => f.write_str("the bound through the sections could not be formed"),
            Self::WeightsLost => f.write_str("a weight is within the bound's rounding of zero"),
            Self::Defect { why } => write!(f, "internal defect: {why}"),
        }
    }
}

impl std::error::Error for SkinError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Fit(e) => Some(e),
            Self::Gap(e) | Self::Through(e) => Some(e),
            _ => None,
        }
    }
}

type Result<T> = core::result::Result<T, SkinError>;

const fn defect(why: &'static str) -> SkinError {
    SkinError::Defect { why }
}

/// The surface of degree `degree` along `v` through `sections` at the parameters `params`
/// (one per section, increasing): `S(u, params[k])` is section `k` to within
/// [`Skinned::through`], and the surface has the sections' knots and degree along `u`.
/// Rational sections give a rational surface.
///
/// The work is one banded solve per control column (twice for rational sections), plus one
/// [`scx_nurbs::deviation::interpolation_bound`] per column, and for a closed skin one
/// certified distance of two sections and `ceil((degree - 1) / 2)` one-dimensional solves.
///
/// # Errors
///
/// [`SkinError::Fit`] with the fit's cause (too few sections for the degree, parameters that
/// do not increase or do not match the count, bad [`Edge`]s, a fitted weight that is not
/// positive); for [`Skin::Closed`]: [`SkinError::Coincidence`],
/// [`SkinError::TooFewSections`], [`SkinError::ClosedDegree`], [`SkinError::NotClosed`]
/// with the gap, [`SkinError::Seam`]; [`SkinError::Gap`], [`SkinError::Through`] and
/// [`SkinError::WeightsLost`] when a bound cannot be formed.
pub fn skin<const D: usize>(
    sections: &CompatibleCurves<D>,
    params: &[f64],
    degree: usize,
    how: &Skin<D>,
) -> Result<Skinned<D>> {
    match how {
        Skin::Open { start, end } => {
            let surface =
                interpolate_rows(sections, params, degree, start, end).map_err(SkinError::Fit)?;
            let through = through(&surface, sections, params)?;
            Ok(Skinned { surface, through })
        }
        Skin::Closed { coincidence } => closed(sections, params, degree, *coincidence),
    }
}

fn closed<const D: usize>(
    sections: &CompatibleCurves<D>,
    v: &[f64],
    q: usize,
    coincidence: f64,
) -> Result<Skinned<D>> {
    if !(coincidence.is_finite() && coincidence >= 0.0) {
        return Err(SkinError::Coincidence { value: coincidence });
    }
    let count = sections.count();
    if count < 3 {
        return Err(SkinError::TooFewSections { count, needed: 3 });
    }
    if q > MAX_CLOSED_DEGREE {
        return Err(SkinError::ClosedDegree {
            degree: q,
            most: MAX_CLOSED_DEGREE,
        });
    }
    let first = sections.curve(0).ok_or(defect("the first section"))?;
    let last = sections
        .curve(count - 1)
        .ok_or(defect("the last section"))?;
    let gap = deviation::distance(&first, &last).map_err(SkinError::Gap)?;
    if gap > coincidence {
        return Err(SkinError::NotClosed { gap, coincidence });
    }
    let rows = ring(sections)?;
    let fit_err = |cause| {
        SkinError::Fit(SurfaceFitError::Fit {
            axis: ParamAxis::V,
            cause,
        })
    };
    let r = q.saturating_sub(1);
    let (a, b) = (r.div_ceil(2), r / 2);
    let kv = fit::averaging(v, q, a, b).map_err(fit_err)?;
    let seam = Seam::new(&kv, v, a, b).map_err(|e| match e {
        Closure::Fit(cause) => fit_err(cause),
        Closure::Skin(e) => e,
    })?;
    let n = rows.knot_vector().point_count();
    let cols = kv.point_count();
    let columns = |data: &[[f64; D]]| -> Result<Vec<[f64; D]>> {
        let mut out = Vec::with_capacity(n * cols);
        for i in 0..n {
            out.extend(seam.column(&column(data, n, i)).map_err(|e| match e {
                Closure::Fit(cause) => fit_err(cause),
                Closure::Skin(e) => e,
            })?);
        }
        Ok(out)
    };
    let net_err = |e| SkinError::Fit(SurfaceFitError::Surface(e));
    let ku = rows.knot_vector().clone();
    let surface = match (rows.weighted_points(), rows.weights()) {
        (Some(wp), Some(w)) => {
            let weighted = Net::new(n, cols, columns(wp)?).map_err(net_err)?;
            let w1: Vec<[f64; 1]> = w.iter().map(|&x| [x]).collect();
            let mut ws = Vec::with_capacity(n * cols);
            for i in 0..n {
                let col = seam.column(&column(&w1, n, i)).map_err(|e| match e {
                    Closure::Fit(cause) => fit_err(cause),
                    Closure::Skin(e) => e,
                })?;
                ws.extend(col.iter().map(|x| x[0]));
            }
            let weights = Net::new(n, cols, ws).map_err(net_err)?;
            NurbsSurface::from_homogeneous(ku, kv, weighted, weights).map_err(net_err)?
        }
        _ => {
            let points = Net::new(n, cols, columns(rows.points())?).map_err(net_err)?;
            NurbsSurface::new(ku, kv, points, None).map_err(net_err)?
        }
    };
    let bound = through(&surface, &rows, v)?;
    let through = (bound + gap) * (1.0 + gamma(1));
    Ok(Skinned { surface, through })
}

/// `sections` with the last section replaced by the first, bit for bit.
fn ring<const D: usize>(sections: &CompatibleCurves<D>) -> Result<CompatibleCurves<D>> {
    let n = sections.knot_vector().point_count();
    let count = sections.count();
    let closed_copy = |data: &[[f64; D]]| -> Vec<[f64; D]> {
        let mut out = data.to_vec();
        let head: Vec<[f64; D]> = data.iter().take(n).copied().collect();
        if let Some(tail) = out.get_mut(n * (count - 1)..) {
            tail.copy_from_slice(&head);
        }
        out
    };
    let knots = sections.knot_vector().clone();
    let made = match (sections.weighted_points(), sections.weights()) {
        (Some(wp), Some(w)) => {
            let mut weights = w.to_vec();
            let head: Vec<f64> = w.iter().take(n).copied().collect();
            if let Some(tail) = weights.get_mut(n * (count - 1)..) {
                tail.copy_from_slice(&head);
            }
            CompatibleCurves::from_homogeneous(knots, count, closed_copy(wp), weights)
        }
        _ => CompatibleCurves::new(knots, count, closed_copy(sections.points()), None),
    };
    made.map_err(|e| SkinError::Fit(SurfaceFitError::Curves(e)))
}

/// Control point `i` of each row of `data` (rows of `n`).
fn column<const N: usize>(data: &[[f64; N]], n: usize, i: usize) -> Vec<[f64; N]> {
    data.chunks_exact(n)
        .filter_map(|r| r.get(i).copied())
        .collect()
}

/// Why a seam solve failed.
enum Closure {
    Fit(FitError),
    Skin(SkinError),
}

impl From<FitError> for Closure {
    fn from(e: FitError) -> Self {
        Self::Fit(e)
    }
}

/// The seam conditions of a closed column fit, formed once for its knots and parameters.
struct Seam<'a> {
    kv: &'a KnotVector,
    v: &'a [f64],
    a: usize,
    b: usize,
    start: BasisDerivs<SEAM_ROWS>,
    end: BasisDerivs<SEAM_ROWS>,
    /// The control values of the fit of zero data with seam derivative `j` one, `j < a`.
    responses: Vec<Vec<f64>>,
    /// The inverse of the `a x a` matrix of the residuals of `responses`.
    inverse: [[f64; MAX_END_ORDER]; MAX_END_ORDER],
}

impl<'a> Seam<'a> {
    fn new(
        kv: &'a KnotVector,
        v: &'a [f64],
        a: usize,
        b: usize,
    ) -> core::result::Result<Self, Closure> {
        let (v0, vm) = match (v.first(), v.last()) {
            (Some(&s), Some(&e)) => (s, e),
            _ => {
                return Err(Closure::Fit(FitError::TooFewPoints {
                    count: 0,
                    needed: 2,
                }));
            }
        };
        let start = kv
            .basis_derivs::<SEAM_ROWS>(v0, Approach::Above)
            .map_err(|_| Closure::Skin(defect("seam basis at the start")))?;
        let end = kv
            .basis_derivs::<SEAM_ROWS>(vm, Approach::Below)
            .map_err(|_| Closure::Skin(defect("seam basis at the end")))?;
        let mut seam = Self {
            kv,
            v,
            a,
            b,
            start,
            end,
            responses: Vec::with_capacity(a),
            inverse: [[0.0; MAX_END_ORDER]; MAX_END_ORDER],
        };
        let zeros = vec![[0.0]; v.len()];
        let mut m = [[0.0; MAX_END_ORDER]; MAX_END_ORDER];
        for j in 0..a {
            let mut d = [[0.0]; MAX_END_ORDER];
            if let Some(x) = d.get_mut(j) {
                *x = [1.0];
            }
            let mut e = [[0.0]; MAX_END_ORDER];
            if j < b {
                e = d;
            }
            let g = fit::interpolate_on(kv, &zeros, v, &held(a, d), &held(b, e))?;
            let g: Vec<f64> = g.points().iter().map(|x| x[0]).collect();
            let g1: Vec<[f64; 1]> = g.iter().map(|&x| [x]).collect();
            for (row, k) in m.iter_mut().zip(b + 1..=a + b) {
                if let Some(cell) = row.get_mut(j) {
                    *cell = seam.residual(&g1, k)?[0];
                }
            }
            seam.responses.push(g);
        }
        seam.inverse = invert(&m, a).ok_or(Closure::Skin(SkinError::Seam))?;
        Ok(seam)
    }

    /// `X^(k)(v_m) - X^(k)(v_0)` of the control values `x`.
    fn residual<const N: usize>(
        &self,
        x: &[[f64; N]],
        k: usize,
    ) -> core::result::Result<[f64; N], Closure> {
        let at = |b: &BasisDerivs<SEAM_ROWS>| -> core::result::Result<[f64; N], Closure> {
            let row = b
                .row(k)
                .ok_or(Closure::Skin(defect("a seam basis row is missing")))?;
            let mut out = [0.0; N];
            for (c, p) in row.iter().zip(x.iter().skip(b.first())) {
                for (o, y) in out.iter_mut().zip(p) {
                    *o += c * y;
                }
            }
            Ok(out)
        };
        let (s, e) = (at(&self.start)?, at(&self.end)?);
        Ok(core::array::from_fn(|c| e[c] - s[c]))
    }

    /// The control values of the closed fit of the column `data` (`data[m] == data[0]`):
    /// interpolation with the seam's derivatives, the last control value the first's.
    fn column<const N: usize>(
        &self,
        data: &[[f64; N]],
    ) -> core::result::Result<Vec<[f64; N]>, Closure> {
        let zero = [[0.0; N]; MAX_END_ORDER];
        let base = fit::interpolate_on(
            self.kv,
            data,
            self.v,
            &held(self.a, zero),
            &held(self.b, zero),
        )?;
        let mut x = base.points().to_vec();
        let mut residuals = [[0.0; N]; MAX_END_ORDER];
        for (r, k) in residuals.iter_mut().zip(self.b + 1..=self.a + self.b) {
            *r = self.residual(&x, k)?;
        }
        for (j, g) in self.responses.iter().enumerate() {
            let row = self
                .inverse
                .get(j)
                .ok_or(Closure::Skin(defect("seam row")))?;
            let d: [f64; N] = core::array::from_fn(|c| {
                -row.iter()
                    .zip(&residuals)
                    .take(self.a)
                    .map(|(m, r)| m * r[c])
                    .sum::<f64>()
            });
            for (p, gl) in x.iter_mut().zip(g) {
                for (y, dc) in p.iter_mut().zip(d) {
                    *y += gl * dc;
                }
            }
        }
        if let Some(&p0) = x.first()
            && let Some(last) = x.last_mut()
        {
            *last = p0;
        }
        Ok(x)
    }
}

/// The end condition holding the first `order` of `d`.
fn held<const N: usize>(order: usize, d: [[f64; N]; MAX_END_ORDER]) -> End<N> {
    let [d1, d2, d3] = d;
    match order {
        0 => End::Point,
        1 => End::Tangent { d1 },
        2 => End::Curvature { d1, d2 },
        _ => End::Hermite { d1, d2, d3 },
    }
}

/// The inverse of the leading `a x a` block of `m` by Gauss-Jordan elimination with partial
/// pivoting; `None` when a pivot is below `a * EPSILON` times the largest entry.
fn invert(
    m: &[[f64; MAX_END_ORDER]; MAX_END_ORDER],
    a: usize,
) -> Option<[[f64; MAX_END_ORDER]; MAX_END_ORDER]> {
    let mut l = *m;
    let mut inv = [[0.0; MAX_END_ORDER]; MAX_END_ORDER];
    for (i, row) in inv.iter_mut().enumerate().take(a) {
        *row.get_mut(i)? = 1.0;
    }
    let largest = l
        .iter()
        .take(a)
        .flat_map(|r| r.iter().take(a))
        .fold(0.0_f64, |s, x| s.max(x.abs()));
    #[allow(clippy::cast_precision_loss)]
    let floor = a as f64 * f64::EPSILON * largest;
    for c in 0..a {
        let p = (c..a).max_by(|&x, &y| {
            let (fx, fy) = (l[x][c].abs(), l[y][c].abs());
            fx.total_cmp(&fy)
        })?;
        if !(l.get(p)?.get(c)?.abs() > floor) {
            return None;
        }
        l.swap(c, p);
        inv.swap(c, p);
        let pivot = l[c][c];
        for k in 0..a {
            l[c][k] /= pivot;
            inv[c][k] /= pivot;
        }
        for r in 0..a {
            if r == c {
                continue;
            }
            let f = l[r][c];
            for k in 0..a {
                l[r][k] -= f * l[c][k];
                inv[r][k] -= f * inv[c][k];
            }
        }
    }
    Some(inv)
}

/// A certified bound of `|S(u, v_k) - C_k(u)|` over the rows `C_k` of `rows` at `v` (module
/// docs): FT1's interpolation bound per control column, divided through the weights for a
/// rational surface.
fn through<const D: usize>(
    s: &NurbsSurface<D>,
    rows: &CompatibleCurves<D>,
    v: &[f64],
) -> Result<f64> {
    let kv = s.knot_vector(ParamAxis::V);
    let n = rows.knot_vector().point_count();
    let bound = SkinError::Through;
    let curve_err = |e| SkinError::Through(DeviationError::Curve(e));
    let columns = |net: &Net<[f64; D]>, data: &[[f64; D]]| -> Result<f64> {
        let mut most = 0.0_f64;
        for i in 0..n {
            let row = net.row(i).ok_or(defect("a net row is missing"))?;
            let c = NurbsCurve::new(kv.clone(), row.to_vec(), None).map_err(curve_err)?;
            most = most
                .max(deviation::interpolation_bound(&c, v, &column(data, n, i)).map_err(bound)?);
        }
        Ok(most)
    };
    let (Some(swp), Some(sw)) = (s.weighted_points(), s.weights()) else {
        return columns(s.points(), rows.points());
    };
    let unit: Vec<f64>;
    let (wp, w) = match (rows.weighted_points(), rows.weights()) {
        (Some(wp), Some(w)) => (wp, w),
        _ => {
            unit = vec![1.0; rows.points().len()];
            (rows.points(), unit.as_slice())
        }
    };
    // |A' / W' - A / W| = |(A' - A) - X (W' - W)| / W' <= (dA + R dW) / (W_min - dW), with
    // X = A / W in the hull of the section's points (|X| <= R) and W at least the least
    // weight (a convex combination of the weights).
    let d_a = columns(swp, wp)?;
    let w1: Vec<[f64; 1]> = w.iter().map(|&x| [x]).collect();
    let mut d_w = 0.0_f64;
    for i in 0..n {
        let row = sw.row(i).ok_or(defect("a weight row is missing"))?;
        let c = NurbsCurve::new(kv.clone(), row.iter().map(|&x| [x]).collect(), None)
            .map_err(curve_err)?;
        d_w = d_w.max(deviation::interpolation_bound(&c, v, &column(&w1, n, i)).map_err(bound)?);
    }
    let reach = rows
        .points()
        .iter()
        .map(|p| p.iter().map(|x| x * x).sum::<f64>().sqrt())
        .fold(0.0_f64, f64::max)
        * (1.0 + gamma(D + 2));
    let least = w.iter().copied().fold(f64::INFINITY, f64::min);
    let lo = least * (1.0 - gamma(1)) - d_w;
    if !(lo > 0.0) {
        return Err(SkinError::WeightsLost);
    }
    let out = (d_a + reach * d_w) / lo * (1.0 + gamma(4));
    if out.is_finite() {
        Ok(out)
    } else {
        Err(SkinError::Through(DeviationError::BoundOverflows))
    }
}
