// Written for solidcorex. Apache-2.0.
//! IGES entity 128 (rational B-spline surface): the parameter data and the surface, both ways.
//!
//! - [`Iges128`]: the entity's parameter list as the pin's `N_Iges128Srf` takes it (the counts
//!   `K1`, `K2` are the net's shape, the degrees `M1`, `M2`, the knots `S`, `T`, the weights
//!   `W`, the points `X Y Z`, the bounds `U(0..1)`, `V(0..1)`);
//! - [`from_iges`]: the pin's `N_Iges128Srf` with its clamp of the knots to the bounds
//!   (`N_IgesClampSrf`). Not its averaging reduction of repeated knots
//!   (`N_IgesCorrectSrfKnots`), which changes the surface: an end knot repeated more than
//!   `p + 1` times is dropped exactly (with its row or column of points, whose basis function
//!   is zero on the domain), and an interior knot repeated `p + 1` times, a break, is refused;
//! - [`to_iges`]: the entity of a surface, which the pin has no counterpart of.
//!
//! The file format (records, the `P` section, fixed-width fields) is out of scope: the pin's
//! 128 code does not read a file either. A reader fills an [`Iges128`] from the parameter data,
//! one [`Net`] row per u index.
//!
//! The entity's flags `PROP1` to `PROP5` (closed in u and v, polynomial, periodic in u and v)
//! are not read: the pin's `N_Iges128Srf` does not take them, and a writer's flags are often
//! wrong. Each is derived from the data instead: polynomial is every weight equal (as the
//! pin), closed and periodic say nothing a surface stores, because closed and periodic data
//! are the same net and knots.
//!
//! `from_iges(&to_iges(s)) == s` exactly for every surface with clamped knots whose weights
//! are not all equal (or that is polynomial). A rational surface of equal weights comes back
//! as its polynomial form (the same surface, as the pin); an unclamped surface comes back
//! clamped to its domain (the same surface on its domain).

mod error;

pub use error::{IgesError, KnotsCause};

use scx_nurbs::{KnotVector, MAX_DEGREE};

use crate::{Net, NetCause, NurbsSurface, NurbsSurfaceError, ParamAxis, WeightCause};

/// `Result` with [`IgesError`].
pub type Result<T> = core::result::Result<T, IgesError>;

/// The parameter data of IGES entity 128 (rational B-spline surface), in the pin's argument
/// order and index meaning, but owned.
#[derive(Debug, Clone, PartialEq)]
pub struct Iges128 {
    /// The degree in u (`M1`).
    pub m1: usize,
    /// The degree in v (`M2`).
    pub m2: usize,
    /// The u knots `S`: `K1 + M1 + 2` of them, `K1 + 1` the net's rows.
    pub s: Vec<f64>,
    /// The v knots `T`: `K2 + M2 + 2` of them, `K2 + 1` the net's columns.
    pub t: Vec<f64>,
    /// The weights `W[i][j]`, the points' shape. All equal means polynomial.
    pub weights: Net<f64>,
    /// The Euclidean control points `P[i][j]`.
    pub points: Net<[f64; 3]>,
    /// The u parameter bounds `[U(0), U(1)]`.
    pub u: [f64; 2],
    /// The v parameter bounds `[V(0), V(1)]`.
    pub v: [f64; 2],
}

/// The entity of a surface: its degrees, knots and points, all-one weights for a polynomial
/// surface, and its domain as the bounds.
#[must_use]
pub fn to_iges(s: &NurbsSurface<3>) -> Iges128 {
    let points = s.points().clone();
    let weights = s.weights().cloned().unwrap_or_else(|| points.map(|_| 1.0));
    let d = s.domain();
    Iges128 {
        m1: s.degree(ParamAxis::U),
        m2: s.degree(ParamAxis::V),
        s: s.knots(ParamAxis::U).to_vec(),
        t: s.knots(ParamAxis::V).to_vec(),
        weights,
        points,
        u: [d.x_min(), d.x_max()],
        v: [d.y_min(), d.y_max()],
    }
}

/// The surface of an entity (`N_Iges128Srf`): clamped to its bounds, with no interior knot
/// repeated more than its degree and no end knot more than `degree + 1` times.
///
/// The surface is rational unless every weight equals the first. The checks, in order: the
/// degrees; the knot counts, finiteness and order; the points and weights (finite, positive
/// weights, finite `w P`); the bounds against the knots (`U[p] <= U(0) < U(1) <= U[m - p]`,
/// each direction); then the surplus end knots are dropped and the knots clamped, which
/// refuses when no control point is left; then the surface's own checks, which refuse a break
/// (an interior knot repeated `p + 1` times).
///
/// Neither step changes the surface on the bounds. The control points
/// of a repaired surface are `w P / w` of the repaired homogeneous points; a surface that
/// needs no repair keeps its points exactly.
pub fn from_iges(e: &Iges128) -> Result<NurbsSurface<3>> {
    let Iges128 {
        m1,
        m2,
        ref s,
        ref t,
        ref weights,
        ref points,
        u,
        v,
    } = *e;
    for (axis, degree) in [(ParamAxis::U, m1), (ParamAxis::V, m2)] {
        if !(1..=MAX_DEGREE).contains(&degree) {
            return Err(IgesError::Degree { axis, degree });
        }
    }
    let (rows, cols) = (points.nrows(), points.ncols());
    // `M + K + 2`, with `K + 1` points.
    for (axis, knots, degree, count) in [(ParamAxis::U, s, m1, rows), (ParamAxis::V, t, m2, cols)] {
        let expected = count + degree + 1;
        if knots.len() != expected {
            return Err(knots_error(
                axis,
                KnotsCause::Count {
                    len: knots.len(),
                    expected,
                },
            ));
        }
        if let Some(index) = knots.iter().position(|k| !k.is_finite()) {
            return Err(knots_error(axis, KnotsCause::NotFinite { index }));
        }
        if let Some(i) = knots
            .iter()
            .zip(knots.iter().skip(1))
            .position(|(a, b)| b < a)
        {
            return Err(knots_error(axis, KnotsCause::Decrease { index: i + 1 }));
        }
    }
    check_input(points, weights)?;
    for (axis, knots, degree, b) in [(ParamAxis::U, s, m1, u), (ParamAxis::V, t, m2, v)] {
        let (Some(&low), Some(&high)) = (knots.get(degree), knots.iter().rev().nth(degree)) else {
            return Err(knots_error(
                axis,
                KnotsCause::Bounds {
                    low: b[0],
                    high: b[1],
                },
            ));
        };
        // Written to refuse NaN bounds.
        if !(low <= b[0] && b[0] < b[1] && b[1] <= high) {
            return Err(knots_error(
                axis,
                KnotsCause::Bounds {
                    low: b[0],
                    high: b[1],
                },
            ));
        }
    }

    // An end knot repeated more than p + 1 times: the extra basis functions are zero on the
    // domain, so the extra knots go with their rows and columns, exactly, before anything else
    // (the points kept are the ones given).
    let (mut ks, mut kt) = (s.clone(), t.clone());
    let mut pw: Vec<Vec<([f64; 3], f64)>> = points
        .rows()
        .zip(weights.rows())
        .map(|(prow, wrow)| prow.iter().copied().zip(wrow.iter().copied()).collect())
        .collect();
    drop_ends(&mut ks, &mut pw, m1);
    let mut cols = transpose(&pw);
    drop_ends(&mut kt, &mut cols, m2);
    let pw = transpose(&cols);
    let (nr, nc) = (pw.len(), pw.first().map_or(0, Vec::len));
    let flat: Vec<([f64; 3], f64)> = pw.into_iter().flatten().collect();
    let points =
        Net::new(nr, nc, flat.iter().map(|x| x.0).collect()).map_err(IgesError::Surface)?;
    let weights =
        Net::new(nr, nc, flat.iter().map(|x| x.1).collect()).map_err(IgesError::Surface)?;

    let rational = !all_equal(&weights);
    let mut rows_h: Vec<Vec<H>> = points
        .rows()
        .zip(weights.rows())
        .map(|(prow, wrow)| {
            prow.iter()
                .zip(wrow)
                .map(|(p, &w)| {
                    if rational {
                        [p[0] * w, p[1] * w, p[2] * w, w]
                    } else {
                        [p[0], p[1], p[2], 1.0]
                    }
                })
                .collect()
        })
        .collect();
    let changed = clamp(&mut rows_h, &mut ks, &mut kt, (m1, m2), u, v)?;

    let (ku, kv) = (basis(ParamAxis::U, m1, ks)?, basis(ParamAxis::V, m2, kt)?);
    let surface = if changed {
        let (nr, nc) = (rows_h.len(), rows_h.first().map_or(0, Vec::len));
        let flat: Vec<H> = rows_h.into_iter().flatten().collect();
        let xyz = Net::new(nr, nc, flat.iter().map(|h| [h[0], h[1], h[2]]).collect());
        let ws = Net::new(nr, nc, flat.iter().map(|h| h[3]).collect());
        match (xyz, ws, rational) {
            (Ok(x), Ok(w), true) => NurbsSurface::from_homogeneous(ku, kv, x, w),
            (Ok(x), Ok(_), false) => NurbsSurface::new(ku, kv, x, None),
            (Err(e), ..) | (_, Err(e), _) => Err(e),
        }
    } else {
        // The data as given: the points exactly.
        NurbsSurface::new(ku, kv, points, rational.then_some(weights))
    };
    surface.map_err(IgesError::Surface)
}

fn knots_error(axis: ParamAxis, cause: KnotsCause) -> IgesError {
    IgesError::Knots { axis, cause }
}

fn basis(axis: ParamAxis, degree: usize, knots: Vec<f64>) -> Result<KnotVector> {
    KnotVector::new(degree, knots).map_err(|e| knots_error(axis, KnotsCause::Basis(e)))
}

/// The points are finite, the weights have their shape and are positive and finite, and
/// (rational) `w P` is finite: the surface's own checks, on the data before the repairs.
fn check_input(points: &Net<[f64; 3]>, weights: &Net<f64>) -> Result<()> {
    let bad = IgesError::Surface;
    if !(weights.nrows() == points.nrows() && weights.ncols() == points.ncols()) {
        return Err(bad(NurbsSurfaceError::BadWeights(WeightCause::Shape {
            rows: weights.nrows(),
            cols: weights.ncols(),
            net_rows: points.nrows(),
            net_cols: points.ncols(),
        })));
    }
    if let Some((row, col, _)) = points.find(|p| p.iter().any(|x| !x.is_finite())) {
        return Err(bad(NurbsSurfaceError::BadNet(NetCause::NotFinite {
            row,
            col,
        })));
    }
    if let Some((row, col, &weight)) = weights.find(|&w| !(w > 0.0 && w.is_finite())) {
        return Err(bad(NurbsSurfaceError::BadWeights(
            WeightCause::NotPositive { row, col, weight },
        )));
    }
    if !all_equal(weights) {
        let weighted = points.zip_map(weights, |p, &w| p.map(|x| x * w));
        if let Some((row, col, _)) = weighted.find(|p| p.iter().any(|x| !x.is_finite())) {
            return Err(bad(NurbsSurfaceError::BadNet(
                NetCause::WeightedNotFinite { row, col },
            )));
        }
    }
    Ok(())
}

/// A homogeneous control point `[w x, w y, w z, w]` (`[x, y, z, 1]` when polynomial; the
/// last is then never read).
type H = [f64; 4];

/// An index outside an array.
struct Oob;

type Idx<T> = core::result::Result<T, Oob>;

/// The clamping of the net and knots to the bounds, u then v. Returns
/// whether the net changed. On a defect, which axis is not known here, so it is `U`'s; callers
/// map it.
fn clamp(
    rows: &mut Vec<Vec<H>>,
    s: &mut Vec<f64>,
    t: &mut Vec<f64>,
    (p, q): (usize, usize),
    ub: [f64; 2],
    vb: [f64; 2],
) -> Result<bool> {
    let defect = |axis| move |Oob| IgesError::Defect { axis };
    let mut changed = false;

    // Clamping: u on the rows, then v on the columns (`N_IgesClampSrf`).
    if let Some((qrows, uq)) = clamp_dir(s, rows, p, ub, |a, x, o, b| comb_rows(a, x, o, b))
        .map_err(defect(ParamAxis::U))?
    {
        if qrows.is_empty() {
            return Err(IgesError::Degenerate { axis: ParamAxis::U });
        }
        *rows = qrows;
        *s = uq;
        changed = true;
    }
    let mut cols = transpose(rows);
    if let Some((qcols, vq)) = clamp_dir(t, &cols, q, vb, |a, x, o, b| comb_rows(a, x, o, b))
        .map_err(defect(ParamAxis::V))?
    {
        if qcols.is_empty() {
            return Err(IgesError::Degenerate { axis: ParamAxis::V });
        }
        cols = qcols;
        *t = vq;
        changed = true;
    }
    *rows = transpose(&cols);
    Ok(changed)
}

/// Whether every weight equals the first (polynomial).
fn all_equal(weights: &Net<f64>) -> bool {
    let first = weights.points().first();
    weights.points().iter().all(|w| Some(w) == first)
}

/// `net[i][j]` -> `out[j][i]`.
fn transpose<T: Copy>(net: &[Vec<T>]) -> Vec<Vec<T>> {
    let cols = net.first().map_or(0, Vec::len);
    (0..cols)
        .map(|j| net.iter().filter_map(|row| row.get(j).copied()).collect())
        .collect()
}

/// `alf A + oma B` (`N_Combine2CPts`).
fn comb(alf: f64, a: &H, oma: f64, b: &H) -> H {
    let ([a0, a1, a2, a3], [b0, b1, b2, b3]) = (*a, *b);
    [
        alf * a0 + oma * b0,
        alf * a1 + oma * b1,
        alf * a2 + oma * b2,
        alf * a3 + oma * b3,
    ]
}

fn comb_rows(alf: f64, a: &[H], oma: f64, b: &[H]) -> Vec<H> {
    a.iter().zip(b).map(|(a, b)| comb(alf, a, oma, b)).collect()
}

/// `a[i]` for a signed index.
fn at<T: Clone>(a: &[T], i: isize) -> Idx<T> {
    usize::try_from(i)
        .ok()
        .and_then(|i| a.get(i))
        .cloned()
        .ok_or(Oob)
}

fn at_mut<T>(a: &mut [T], i: isize) -> Idx<&mut T> {
    usize::try_from(i)
        .ok()
        .and_then(|i| a.get_mut(i))
        .ok_or(Oob)
}

/// `N_IgesClampCrv` / one direction of `N_IgesClampSrf`, over knots `up` and control points
/// `pw` of degree `p`, to the bounds `b`. Junk end knots are overwritten in `up`. Returns
/// `None` when the knots are then already clamped, else the new points and knots.
fn clamp_dir<T: Clone>(
    up: &mut [f64],
    pw: &[T],
    p: usize,
    b: [f64; 2],
    comb: impl Fn(f64, &T, f64, &T) -> T,
) -> Idx<Option<(Vec<T>, Vec<f64>)>> {
    let m = isize::try_from(up.len()).map_err(|_| Oob)? - 1;
    let p = isize::try_from(p).map_err(|_| Oob)?;

    // Find knot spans and set multiplicities of bounds (as knots).
    let (mut span1, mut span2) = (0_isize, 0_isize);
    let mut mult1 = 0_isize;
    for i in 0..=m {
        let ui = at(up, i)?;
        if ui == b[0] {
            mult1 += 1;
        }
        if ui > b[0] {
            span1 = i - 1;
            break;
        }
    }
    let mut mult2 = 0_isize;
    for i in (0..=m).rev() {
        let ui = at(up, i)?;
        if ui == b[1] {
            mult2 += 1;
        }
        if ui < b[1] {
            span2 = i + mult2;
            break;
        }
    }

    // Junk in the first and last knots.
    if span1 >= p && mult1 == span1 {
        *at_mut(up, 0)? = b[0];
        mult1 += 1;
    }
    if span2 == m - 1 && mult2 >= p {
        *at_mut(up, m)? = b[1];
        mult2 += 1;
        span2 += 1;
    }

    // Already clamped at both ends.
    if mult1 == span1 + 1 && span2 == m && mult2 > p {
        return Ok(None);
    }

    // New indices.
    let is = span1 - p;
    let ie = span2 - mult2;
    let n = ie - is;
    let mq = span2 - span1 - mult2 + 2 * p + 1;
    if n < 0 {
        return Ok(Some((Vec::new(), Vec::new())));
    }
    let len_uq = usize::try_from(mq).map_err(|_| Oob)? + 1;

    // Initial control points.
    let mut qw = Vec::new();
    for i in is..=ie {
        qw.push(at(pw, i)?);
    }

    // Insert the left knot.
    let ll = span1 - p;
    for i in 1..=(p - mult1) {
        for j in 0..=(p - i - mult1) {
            let left = at(up, ll + i + j)?;
            let alf = (b[0] - left) / (at(up, span1 + j + 1)? - left);
            let oma = 1.0 - alf;
            let a = at(&qw, j + 1)?;
            let slot = at_mut(&mut qw, j)?;
            *slot = comb(alf, &a, oma, slot);
        }
    }

    // Insert the right knot.
    let lr = span2 - p;
    let lk = n - p + mult2;
    for i in 1..=(p - mult2) {
        for j in (0..=(p - i - mult2)).rev() {
            let k = lk + i + j;
            let mut left = at(up, lr + i + j)?;
            if left < b[0] {
                left = b[0];
            }
            let alf = (b[1] - left) / (at(up, span2 + j + 1)? - left);
            let oma = 1.0 - alf;
            let prev = at(&qw, k - 1)?;
            let slot = at_mut(&mut qw, k)?;
            *slot = comb(alf, slot, oma, &prev);
        }
    }

    // Load the knot vector.
    let pu = usize::try_from(p).map_err(|_| Oob)?;
    let mut uq = vec![b[0]; pu + 1];
    for i in (span1 + 1)..=(span2 - mult2) {
        uq.push(at(up, i)?);
    }
    uq.extend(core::iter::repeat_n(b[1], pu + 1));
    if uq.len() != len_uq {
        return Err(Oob);
    }
    Ok(Some((qw, uq)))
}

/// Drops the knots an end repeats beyond `p + 1` times, with the items (rows or columns of
/// control points) of the same index.
fn drop_ends<T>(knots: &mut Vec<f64>, items: &mut Vec<T>, p: usize) {
    let repeated = |k: &[f64], at_end: bool| {
        let end = if at_end { k.last() } else { k.first() };
        let run = k.iter();
        k.len() > p + 2
            && if at_end {
                run.rev().take(p + 2).all(|x| Some(x) == end)
            } else {
                run.take(p + 2).all(|x| Some(x) == end)
            }
    };
    while repeated(knots, false) {
        knots.remove(0);
        items.remove(0);
    }
    while repeated(knots, true) {
        knots.pop();
        items.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::hash::Rng;
    use scx_testkit::data::SurfaceData;
    use scx_testkit::nurbs::{Side, Surface as Exact};
    use scx_testkit::sample::{self, Knots, Spec, Weights};

    type Edit<'a> = &'a dyn Fn(&mut Vec<f64>, &mut Vec<[f64; 3]>);

    fn data(rng: &mut Rng, degrees: (usize, usize), knots: Knots, w: Weights) -> SurfaceData<3> {
        let spec = |degree| Spec {
            degree,
            count: degree + 3,
            knots,
        };
        sample::surface(rng, (spec(degrees.0), spec(degrees.1)), w)
    }

    /// The entity of generated data, with the given bounds.
    fn entity(d: &SurfaceData<3>, u: [f64; 2], v: [f64; 2]) -> Iges128 {
        let rows = d.knots_u.len() - d.degrees.0 - 1;
        let cols = d.knots_v.len() - d.degrees.1 - 1;
        let weights = d.weights.clone().unwrap_or_else(|| vec![1.0; rows * cols]);
        Iges128 {
            m1: d.degrees.0,
            m2: d.degrees.1,
            s: d.knots_u.clone(),
            t: d.knots_v.clone(),
            weights: Net::new(rows, cols, weights).unwrap(),
            points: Net::new(rows, cols, d.points.clone()).unwrap(),
            u,
            v,
        }
    }

    fn exact(e: &Iges128) -> Exact<3> {
        let w = e.weights.points();
        Exact::new(
            (e.m1, e.m2),
            &e.s,
            &e.t,
            e.points.points(),
            (!w.windows(2).all(|p| p.first() == p.get(1))).then_some(w),
        )
        .unwrap()
    }

    fn exact_of(s: &NurbsSurface<3>) -> Exact<3> {
        Exact::new(
            (s.degree(ParamAxis::U), s.degree(ParamAxis::V)),
            s.knots(ParamAxis::U),
            s.knots(ParamAxis::V),
            s.points().points(),
            s.weights().map(Net::points),
        )
        .unwrap()
    }

    /// `got` is the surface of `e` on its bounds: the exact points agree to rounding.
    fn same_on_bounds(e: &Iges128, got: &NurbsSurface<3>) {
        let (want, have) = (exact(e), exact_of(got));
        for i in 0..4 {
            for j in 0..4 {
                let uv = (
                    e.u[0] + (e.u[1] - e.u[0]) * (0.07 + 0.29 * f64::from(i)),
                    e.v[0] + (e.v[1] - e.v[0]) * (0.05 + 0.31 * f64::from(j)),
                );
                let side = (Side::Above, Side::Above);
                let (a, b) = (want.point(uv, side).unwrap(), have.point(uv, side).unwrap());
                for (x, y) in a.rounded.iter().zip(&b.rounded) {
                    assert!((x - y).abs() <= 1e-9 * (1.0 + x.abs()), "{x} {y} at {uv:?}");
                }
            }
        }
        let d = got.domain();
        assert_eq!(
            (d.x_min(), d.x_max(), d.y_min(), d.y_max()),
            (e.u[0], e.u[1], e.v[0], e.v[1])
        );
        assert!(got.knot_vector(ParamAxis::U).is_clamped());
        assert!(got.knot_vector(ParamAxis::V).is_clamped());
    }

    /// The surface of the data on `[0, 1]²`; `None` where it has a break.
    fn build(d: &SurfaceData<3>) -> Option<NurbsSurface<3>> {
        let e = entity(d, [0.0, 1.0], [0.0, 1.0]);
        let u = KnotVector::new(e.m1, e.s).ok()?;
        let v = KnotVector::new(e.m2, e.t).ok()?;
        let w = d.weights.is_some().then_some(e.weights);
        NurbsSurface::new(u, v, e.points, w).ok()
    }

    #[test]
    fn round_trip_is_exact() {
        let mut rng = Rng::new(5);
        let mut n = 0;
        for degrees in [(1, 1), (2, 3), (3, 2), (4, 4)] {
            for knots in [Knots::Uniform, Knots::Clustered, Knots::Multiple] {
                for w in [Weights::Unit, Weights::Moderate, Weights::NearZero] {
                    let d = data(&mut rng, degrees, knots, w);
                    let Some(s) = build(&d) else { continue };
                    assert_eq!(from_iges(&to_iges(&s)).unwrap(), s);
                    n += 1;
                }
            }
        }
        assert!(n > 20);
    }

    #[test]
    fn equal_weights_come_back_polynomial() {
        let mut rng = Rng::new(6);
        let d = data(&mut rng, (2, 2), Knots::Uniform, Weights::Unit);
        let mut e = entity(&d, [0.0, 1.0], [0.0, 1.0]);
        let poly = from_iges(&e).unwrap();
        assert!(!poly.is_rational());
        e.weights = e.weights.map(|_| 3.0);
        assert_eq!(from_iges(&e).unwrap(), poly);
        // One weight apart makes it rational.
        let mut w = e.weights.points().to_vec();
        w[4] = 2.0;
        e.weights = Net::new(e.weights.nrows(), e.weights.ncols(), w).unwrap();
        assert!(from_iges(&e).unwrap().is_rational());
    }

    #[test]
    fn bounds_inside_the_domain_clamp() {
        let mut rng = Rng::new(7);
        for w in [Weights::Unit, Weights::Moderate] {
            let d = data(&mut rng, (2, 3), Knots::Uniform, w);
            let e = entity(&d, [0.2, 0.9], [0.0, 0.55]);
            let got = from_iges(&e).unwrap();
            same_on_bounds(&e, &got);
            assert_eq!(got.is_rational(), d.weights.is_some());
        }
    }

    #[test]
    fn junk_ends_and_unclamped_knots_clamp() {
        let mut rng = Rng::new(8);
        let d = data(&mut rng, (3, 2), Knots::Uniform, Weights::Moderate);
        // u unclamped on both ends, v with junk first knots (an unclamped start).
        let mut d2 = d.clone();
        let m = d2.knots_u.len() - 1;
        for i in 0..3 {
            d2.knots_u[i] = -0.1 * (3 - i) as f64;
            d2.knots_u[m - i] = 1.0 + 0.1 * (3 - i) as f64;
        }
        for (i, k) in d2.knots_v.iter_mut().take(2).enumerate() {
            *k = -0.25 * (2 - i) as f64;
        }
        let e = entity(&d2, [0.0, 1.0], [0.0, 1.0]);
        let got = from_iges(&e).unwrap();
        same_on_bounds(&e, &got);
        // Junk at the very ends only: the end knots are overwritten, no point changes.
        let mut d3 = d.clone();
        d3.knots_u[0] = -5.0;
        let e3 = entity(&d3, [0.0, 1.0], [0.0, 1.0]);
        assert_eq!(from_iges(&e3).unwrap(), build(&d).unwrap());
    }

    #[test]
    fn clamped_input_is_unchanged() {
        let mut rng = Rng::new(9);
        let d = data(&mut rng, (2, 2), Knots::Multiple, Weights::Moderate);
        let e = entity(&d, [0.0, 1.0], [0.0, 1.0]);
        let got = from_iges(&e).unwrap();
        assert_eq!(got.knots(ParamAxis::U), d.knots_u.as_slice());
        assert_eq!(got.knots(ParamAxis::V), d.knots_v.as_slice());
        assert_eq!(got.points().points(), d.points.as_slice());
    }

    /// An end knot repeated `p + 2` times: the extra knot goes with its row (or column). The
    /// testkit's exact model takes no such knots, so the proof is by construction: take a
    /// surface, repeat its end knots once more with rows and columns of arbitrary points, and
    /// the result is the surface exactly. (The basis function of the extra point is zero on
    /// the domain, so the points cannot matter.)
    #[test]
    fn surplus_end_knots_drop_exactly() {
        let mut rng = Rng::new(10);
        for w in [Weights::Unit, Weights::Moderate] {
            let d = data(&mut rng, (2, 3), Knots::Uniform, w);
            let s = build(&d).unwrap();
            let mut e = to_iges(&s);
            // Rationality is decided on all the weights given, as the pin does.
            let junk_weight = if s.is_rational() { 2.0 } else { 1.0 };
            let (rows, cols) = (e.points.nrows(), e.points.ncols());
            // One more knot at each end of each direction, and a row and column of junk.
            e.s.insert(0, e.s[0]);
            e.s.push(e.s[e.s.len() - 1]);
            e.t.insert(0, e.t[0]);
            e.t.push(e.t[e.t.len() - 1]);
            let grid = |f: &dyn Fn(usize, usize) -> Option<usize>, fill: f64| {
                let mut pts = Vec::new();
                let mut ws = Vec::new();
                for i in 0..rows + 2 {
                    for j in 0..cols + 2 {
                        if let (Some(a), Some(b)) = (f(i, rows), f(j, cols)) {
                            pts.push(e.points.get(a, b).copied().unwrap());
                            ws.push(e.weights.get(a, b).copied().unwrap());
                        } else {
                            pts.push([fill, -fill, 7.0 * fill]);
                            ws.push(junk_weight);
                        }
                    }
                }
                (pts, ws)
            };
            let inner = |i: usize, n: usize| (i >= 1 && i <= n).then(|| i - 1);
            let (pts, ws) = grid(&inner, 123.0);
            e.points = Net::new(rows + 2, cols + 2, pts).unwrap();
            e.weights = Net::new(rows + 2, cols + 2, ws).unwrap();
            assert_eq!(from_iges(&e).unwrap(), s);
        }
    }

    /// An interior knot repeated `p + 1` times is a break: refused, not averaged.
    #[test]
    fn a_break_is_refused() {
        let mut rng = Rng::new(10);
        let mut d = data(&mut rng, (2, 2), Knots::Uniform, Weights::Unit);
        d.knots_u = vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0];
        d.points = d.points.iter().copied().cycle().take(6 * 5).collect();
        let e = entity(&d, [0.0, 1.0], [0.0, 1.0]);
        let Err(IgesError::Surface(err @ NurbsSurfaceError::BadKnots { axis, cause })) =
            from_iges(&e)
        else {
            panic!("a break was not refused");
        };
        assert_eq!(axis, ParamAxis::U);
        assert!(matches!(cause, scx_nurbs::NurbsError::Multiplicity { .. }));
        assert_eq!(err.kind(), scx_num::error::ErrorKind::Refused);
        // Bounds that leave the break outside are fine.
        let e = entity(&d, [0.0, 0.4], [0.0, 1.0]);
        assert!(from_iges(&e).is_ok());
    }

    #[test]
    fn refusals() {
        let mut rng = Rng::new(11);
        let d = data(&mut rng, (2, 2), Knots::Uniform, Weights::Moderate);
        let good = entity(&d, [0.0, 1.0], [0.0, 1.0]);
        assert!(from_iges(&good).is_ok());
        let err = |f: &dyn Fn(&mut Iges128)| {
            let mut e = good.clone();
            f(&mut e);
            from_iges(&e).unwrap_err()
        };
        let nets = |e: &mut Iges128, f: Edit<'_>| {
            let (r, c) = (e.points.nrows(), e.points.ncols());
            let (mut w, mut p) = (e.weights.points().to_vec(), e.points.points().to_vec());
            f(&mut w, &mut p);
            e.weights = Net::new(r, c, w).unwrap();
            e.points = Net::new(r, c, p).unwrap();
        };
        let knots = |axis, cause| IgesError::Knots { axis, cause };
        assert_eq!(
            err(&|e| e.m1 = 0),
            IgesError::Degree {
                axis: ParamAxis::U,
                degree: 0
            }
        );
        assert_eq!(
            err(&|e| e.m2 = 32),
            IgesError::Degree {
                axis: ParamAxis::V,
                degree: 32
            }
        );
        assert_eq!(
            err(&|e| {
                e.t.pop();
            }),
            knots(
                ParamAxis::V,
                KnotsCause::Count {
                    len: 7,
                    expected: 8
                }
            )
        );
        assert_eq!(
            err(&|e| e.s.push(1.0)),
            knots(
                ParamAxis::U,
                KnotsCause::Count {
                    len: 9,
                    expected: 8
                }
            )
        );
        assert_eq!(
            err(&|e| e.s[3] = f64::NAN),
            knots(ParamAxis::U, KnotsCause::NotFinite { index: 3 })
        );
        assert_eq!(
            err(&|e| e.t[2] = -1.0),
            knots(ParamAxis::V, KnotsCause::Decrease { index: 2 })
        );
        for (u, v) in [
            ([0.5, 0.5], [0.0, 1.0]),
            ([0.0, 1.0], [-0.1, 1.0]),
            ([0.0, 1.1], [0.0, 1.0]),
            ([f64::NAN, 1.0], [0.0, 1.0]),
        ] {
            let e = err(&|e| {
                e.u = u;
                e.v = v;
            });
            assert!(matches!(
                e,
                IgesError::Knots {
                    cause: KnotsCause::Bounds { .. },
                    ..
                }
            ));
        }
        for w in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let e = err(&|e| nets(e, &|ws, _| ws[2] = w));
            assert!(matches!(
                e,
                IgesError::Surface(NurbsSurfaceError::BadWeights(WeightCause::NotPositive {
                    row: 0,
                    col: 2,
                    ..
                }))
            ));
        }
        let e = err(&|e| nets(e, &|_, ps| ps[5][1] = f64::INFINITY));
        assert_eq!(
            e,
            IgesError::Surface(NurbsSurfaceError::BadNet(NetCause::NotFinite {
                row: 1,
                col: 0
            }))
        );
        let e = err(&|e| {
            nets(e, &|ws, ps| {
                ws[0] = 1e300;
                ps[0][0] = 1e10;
            });
        });
        assert!(matches!(
            e,
            IgesError::Surface(NurbsSurfaceError::BadNet(
                NetCause::WeightedNotFinite { .. }
            ))
        ));
        let mut e = good.clone();
        e.weights = Net::new(1, 1, vec![1.0]).unwrap();
        assert!(matches!(
            from_iges(&e),
            Err(IgesError::Surface(NurbsSurfaceError::BadWeights(
                WeightCause::Shape { .. }
            )))
        ));
    }

    /// Weights of any positive scale are surfaces (the pin refuses outside `[1e-3, 100]`).
    #[test]
    fn weights_have_no_range() {
        let mut rng = Rng::new(12);
        let d = data(&mut rng, (2, 2), Knots::Uniform, Weights::Moderate);
        let mut e = entity(&d, [0.0, 1.0], [0.0, 1.0]);
        e.weights = e.weights.map(|w| w * 1e-6);
        assert!(from_iges(&e).unwrap().is_rational());
        e.weights = e.weights.map(|w| w * 1e9);
        assert!(from_iges(&e).is_ok());
    }
}
