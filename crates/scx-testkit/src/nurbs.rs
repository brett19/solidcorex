// Written for solidcorex. Apache-2.0.
//! Exact B-spline and NURBS curves and surfaces over exact rationals.
//!
//! The data are doubles (knots, control points, weights) taken exactly; control points
//! are held in homogeneous form `(w P, w)` as [`Rat`]s, so evaluation, derivatives, knot
//! insertion and degree elevation make no rounding error. Each result comes back exact and
//! correctly rounded ([`Exact`]).
//!
//! Two independent evaluations, for the references to check each other: [`Curve::derivs`]
//! runs de Boor's algorithm on the derivative curves' control points; [`Curve::derivs_by_basis`]
//! sums the basis functions and their derivatives (Cox-de Boor and its derivative
//! recurrence). Rational derivatives follow from the homogeneous ones by the quotient rule.
//!
//! A parameter selects a span of the domain `[u_p, u_n]`. At an interior knot [`Side`]
//! picks the piece that ends there or the one that starts there; outside the domain the
//! end piece continues (its polynomial, evaluated past the end).

use core::fmt;

use crate::data::{CurveData, SurfaceData};
use crate::rat::Rat;
use crate::Partials;

/// Which piece a parameter at an interior knot belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The piece that ends at the knot (the limit from below).
    Below,
    /// The piece that starts at the knot (the limit from above).
    Above,
}

/// Why a curve or surface cannot be built or evaluated.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// Fewer control points than `degree + 1`.
    TooFewPoints,
    /// The knot count is not `points + degree + 1`.
    KnotCount {
        /// `points + degree + 1`.
        expected: usize,
        /// The knots given.
        found: usize,
    },
    /// The weight count is not the point count.
    WeightCount {
        /// The points given.
        expected: usize,
        /// The weights given.
        found: usize,
    },
    /// A knot, coordinate, weight or parameter is infinite or NaN.
    NonFinite,
    /// The knots decrease somewhere.
    KnotsDecrease,
    /// A knot is repeated more than `degree + 1` times.
    Multiplicity,
    /// The domain `[u_p, u_n]` is a single value.
    EmptyDomain,
    /// A control point's weight is zero, or the weight function is zero at the parameter.
    ZeroWeight,
    /// A knot to insert lies outside the domain, or a split is not strictly inside it.
    OutsideDomain,
    /// Degree elevation needs end knots of multiplicity `degree + 1`.
    Unclamped,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooFewPoints => write!(f, "fewer control points than degree + 1"),
            Error::KnotCount { expected, found } => {
                write!(f, "{found} knots, expected {expected}")
            }
            Error::WeightCount { expected, found } => {
                write!(f, "{found} weights, expected {expected}")
            }
            Error::NonFinite => write!(f, "a value is infinite or NaN"),
            Error::KnotsDecrease => write!(f, "the knots decrease"),
            Error::Multiplicity => write!(f, "a knot repeated more than degree + 1 times"),
            Error::EmptyDomain => write!(f, "the domain is a single value"),
            Error::ZeroWeight => write!(f, "a weight is zero"),
            Error::OutsideDomain => write!(f, "the knot lies outside the domain"),
            Error::Unclamped => write!(f, "the end knots are not of multiplicity degree + 1"),
        }
    }
}

impl std::error::Error for Error {}

/// An exact value and its correctly rounded doubles.
#[derive(Clone, Debug, PartialEq)]
pub struct Exact<const D: usize> {
    /// The exact value.
    pub exact: [Rat; D],
    /// Each coordinate rounded to the nearest double (ties to even).
    pub rounded: [f64; D],
}

impl<const D: usize> Exact<D> {
    pub(crate) fn of(v: &[Rat]) -> Exact<D> {
        let exact: [Rat; D] = array(v);
        let rounded = exact.each_ref().map(Rat::to_f64);
        Exact { exact, rounded }
    }
}

/// A homogeneous control point `(w P, w)`, or a homogeneous derivative.
type Row = Vec<Rat>;

fn array<const D: usize>(v: &[Rat]) -> [Rat; D] {
    core::array::from_fn(|i| v.get(i).cloned().unwrap_or_default())
}

fn rat(x: f64) -> Result<Rat, Error> {
    Rat::from_f64(x).ok_or(Error::NonFinite)
}

/// The homogeneous points of plain data.
fn homogeneous<const D: usize>(
    points: &[[f64; D]],
    weights: Option<&[f64]>,
) -> Result<Vec<Row>, Error> {
    if let Some(w) = weights {
        if w.len() != points.len() {
            return Err(Error::WeightCount {
                expected: points.len(),
                found: w.len(),
            });
        }
    }
    let weight = |i: usize| weights.and_then(|w| w.get(i)).copied().unwrap_or(1.0);
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let w = rat(weight(i))?;
            if w.is_zero() {
                return Err(Error::ZeroWeight);
            }
            let mut row = p
                .iter()
                .map(|&x| Ok(&rat(x)? * &w))
                .collect::<Result<Row, _>>()?;
            row.push(w);
            Ok(row)
        })
        .collect()
}

/// One direction's knots, checked, with their exact values.
#[derive(Clone, Debug, PartialEq)]
struct Knots {
    degree: usize,
    /// The control point count `n`.
    count: usize,
    values: Vec<f64>,
    exact: Vec<Rat>,
}

impl Knots {
    fn new(degree: usize, values: &[f64], count: usize) -> Result<Knots, Error> {
        if count < degree + 1 {
            return Err(Error::TooFewPoints);
        }
        if values.len() != count + degree + 1 {
            return Err(Error::KnotCount {
                expected: count + degree + 1,
                found: values.len(),
            });
        }
        let exact = values
            .iter()
            .map(|&k| rat(k))
            .collect::<Result<Vec<_>, _>>()?;
        if values.windows(2).any(|w| matches!(w, [a, b] if a > b)) {
            return Err(Error::KnotsDecrease);
        }
        if values.windows(degree + 2).any(|w| w.first() == w.last()) {
            return Err(Error::Multiplicity);
        }
        let knots = Knots {
            degree,
            count,
            values: values.to_vec(),
            exact,
        };
        let (a, b) = knots.domain();
        if a >= b {
            return Err(Error::EmptyDomain);
        }
        Ok(knots)
    }

    /// `[u_p, u_n]`.
    fn domain(&self) -> (f64, f64) {
        let at = |i: usize| self.values.get(i).copied().unwrap_or(0.0);
        (at(self.degree), at(self.count))
    }

    /// The span `l` (`u_l < u_{l+1}`, `p <= l < n`) a parameter evaluates on.
    fn span(&self, u: f64, side: Side) -> usize {
        let live = self
            .values
            .windows(2)
            .enumerate()
            .skip(self.degree)
            .take(self.count - self.degree)
            .filter_map(|(l, w)| match w {
                [a, b] if a < b => Some((l, *a, *b)),
                _ => None,
            });
        let (mut first, mut last, mut chosen) = (None, self.degree, None);
        for (l, a, b) in live {
            first.get_or_insert(l);
            last = l;
            match side {
                // the last span starting at or below u
                Side::Above if a <= u => chosen = Some(l),
                // the first span ending at or above u
                Side::Below if chosen.is_none() && u <= b => chosen = Some(l),
                _ => {}
            }
        }
        match side {
            Side::Above => chosen.or(first).unwrap_or(self.degree),
            Side::Below => chosen.unwrap_or(last),
        }
    }

    /// The multiplicity of `u` among the knots.
    fn multiplicity(&self, u: f64) -> usize {
        self.values.iter().filter(|&&k| k == u).count()
    }

    /// Whether both ends have multiplicity `degree + 1`.
    fn clamped(&self) -> bool {
        let p = self.degree;
        let (a, b) = self.domain();
        self.values.iter().take(p + 1).all(|&k| k == a)
            && self.values.iter().skip(self.count).all(|&k| k == b)
    }

    /// The knots with `u` inserted once, after span `l`.
    fn inserted(&self, u: f64, l: usize) -> Result<Knots, Error> {
        let values: Vec<f64> = self
            .values
            .iter()
            .take(l + 1)
            .copied()
            .chain(core::iter::once(u))
            .chain(self.values.iter().skip(l + 1).copied())
            .collect();
        Knots::new(self.degree, &values, self.count + 1)
    }

    /// The knots of the curve elevated once: degree + 1, each distinct knot once more.
    fn elevated(&self) -> Result<Knots, Error> {
        let mut values = Vec::with_capacity(self.values.len() * 2);
        for (i, &k) in self.values.iter().enumerate() {
            values.push(k);
            if self.values.get(i + 1) != Some(&k) {
                values.push(k);
            }
        }
        let degree = self.degree + 1;
        let count = values.len() - degree - 1;
        Knots::new(degree, &values, count)
    }
}

// ---------------------------------------------------------------- the two evaluations

/// `a + t (b - a)`.
fn lerp(a: &[Rat], b: &[Rat], t: &Rat) -> Row {
    a.iter().zip(b).map(|(x, y)| x + &(t * &(y - x))).collect()
}

/// De Boor's triangle for the degree-`q` piece on span `l`: `pts` are `R_{l-q..=l}` and
/// level `r` uses `args[r - 1]` (all `u` for a point; distinct arguments give the blossom).
fn de_boor(kr: &[Rat], q: usize, l: usize, mut pts: Vec<Row>, args: &[Rat]) -> Option<Row> {
    for (r, a) in (1..=q).zip(args) {
        let lo = kr.iter().skip(l + r - q);
        let hi = kr.iter().skip(l + 1);
        pts = pts
            .windows(2)
            .zip(lo.zip(hi))
            .map(|(w, (k0, k1))| {
                let t = (a - k0).checked_div(&(k1 - k0))?;
                match w {
                    [x, y] => Some(lerp(x, y, &t)),
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()?;
    }
    pts.into_iter().next()
}

/// Homogeneous derivatives `0..=order` on span `l` from the local points `P_{l-p..=l}`,
/// by de Boor on each derivative curve's control points
/// `R^(k)_j = (p - k + 1) (R^(k-1)_j - R^(k-1)_{j-1}) / (u_{j+p-k+1} - u_j)`.
fn local_derivs(
    kr: &[Rat],
    p: usize,
    l: usize,
    pts: Vec<Row>,
    u: &Rat,
    order: usize,
) -> Option<Vec<Row>> {
    let h = pts.first().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(order + 1);
    let mut pts = pts;
    for k in 0..=order {
        if k > p {
            out.push(vec![Rat::zero(); h]);
            continue;
        }
        let q = p - k;
        out.push(de_boor(kr, q, l, pts.clone(), &vec![u.clone(); q])?);
        if k < order && q > 0 {
            let factor = Rat::from(q as i64);
            let lo = kr.iter().skip(l + k + 1 - p);
            let hi = kr.iter().skip(l + 1);
            pts = pts
                .windows(2)
                .zip(lo.zip(hi))
                .map(|(w, (a, b))| {
                    let s = factor.checked_div(&(b - a))?;
                    match w {
                        [x, y] => Some(x.iter().zip(y).map(|(x, y)| &s * &(y - x)).collect()),
                        _ => None,
                    }
                })
                .collect::<Option<Vec<_>>>()?;
        }
    }
    Some(out)
}

/// `new_i = a_i prev_i + b_i prev_{i+1}` for `i = l-q..=l`, `prev` holding `i = l-q+1..=l`
/// (zero outside), `a_i = na(u_i) / (u_{i+q} - u_i)`, `b_i = nb(u_{i+q+1}) /
/// (u_{i+q+1} - u_{i+1})`, a zero span giving a zero term (its function is zero).
fn basis_step(
    kr: &[Rat],
    q: usize,
    l: usize,
    prev: &[Rat],
    na: impl Fn(&Rat) -> Rat,
    nb: impl Fn(&Rat) -> Rat,
) -> Vec<Rat> {
    let zero = Rat::zero();
    let padded: Vec<&Rat> = core::iter::once(&zero)
        .chain(prev)
        .chain(core::iter::once(&zero))
        .collect();
    let a = kr.iter().skip(l - q).zip(kr.iter().skip(l));
    let b = kr.iter().skip(l - q + 1).zip(kr.iter().skip(l + 1));
    let term = |f: &Rat, n: Rat, lo: &Rat, hi: &Rat| {
        if f.is_zero() {
            Rat::zero()
        } else {
            (&n * f).checked_div(&(hi - lo)).unwrap_or_default()
        }
    };
    padded
        .windows(2)
        .zip(a.zip(b))
        .map(|(w, ((ui, uiq), (ui1, uiq1)))| match w {
            [f, g] => term(f, na(ui), ui, uiq) + term(g, nb(uiq1), ui1, uiq1),
            _ => Rat::zero(),
        })
        .collect()
}

/// `N^(k)_{i,p}(u)` for `i = l-p..=l` and `k = 0..=order`: Cox-de Boor for the values, then
/// `N^(k)_{i,q} = q (N^(k-1)_{i,q-1} / (u_{i+q} - u_i) - N^(k-1)_{i+1,q-1} / (u_{i+q+1} - u_{i+1}))`.
fn basis_derivs(kr: &[Rat], p: usize, l: usize, u: &Rat, order: usize) -> Vec<Vec<Rat>> {
    // table[k][q]: N^(k)_{i,q} for i = l-q..=l.
    let mut values = vec![vec![Rat::one()]];
    for q in 1..=p {
        let prev = values.last().cloned().unwrap_or_default();
        values.push(basis_step(kr, q, l, &prev, |ui| u - ui, |uiq1| uiq1 - u));
    }
    let mut table = vec![values];
    for _ in 1..=order {
        let last = table.last().cloned().unwrap_or_default();
        let row = (0..=p)
            .map(|q| {
                if q == 0 {
                    return vec![Rat::zero()];
                }
                let prev = last.get(q - 1).cloned().unwrap_or_default();
                let n = Rat::from(q as i64);
                basis_step(kr, q, l, &prev, |_| n.clone(), |_| -&n)
            })
            .collect();
        table.push(row);
    }
    table
        .into_iter()
        .map(|row| row.into_iter().nth(p).unwrap_or_default())
        .collect()
}

/// `sum_i c_i P_i`.
fn combine<'a>(coefs: &[Rat], pts: impl Iterator<Item = &'a Row>, h: usize) -> Row {
    coefs
        .iter()
        .zip(pts)
        .fold(vec![Rat::zero(); h], |acc, (c, p)| {
            acc.iter().zip(p).map(|(a, x)| a + &(c * x)).collect()
        })
}

fn binomial(n: usize, k: usize) -> Rat {
    let mut b: i64 = 1;
    for i in 0..k {
        b = b * (n - i) as i64 / (i as i64 + 1);
    }
    Rat::from(b)
}

/// Cartesian derivatives from homogeneous ones `A^(k) = (w C)^(k)`:
/// `C^(k) = (A^(k) - sum_{i=1..k} binom(k, i) w^(i) C^(k-i)) / w`.
fn quotient(hom: &[Row]) -> Result<Vec<Row>, Error> {
    let d = hom.first().map_or(0, |r| r.len().saturating_sub(1));
    let weight = |k: usize| {
        hom.get(k)
            .and_then(|r| r.get(d))
            .cloned()
            .unwrap_or_default()
    };
    let w = weight(0).recip().ok_or(Error::ZeroWeight)?;
    let mut out: Vec<Row> = Vec::with_capacity(hom.len());
    for (k, a) in hom.iter().enumerate() {
        let mut v: Row = a.iter().take(d).cloned().collect();
        for i in 1..=k {
            let c = &binomial(k, i) * &weight(i);
            if let Some(prev) = out.get(k - i) {
                v = v.iter().zip(prev).map(|(x, y)| x - &(&c * y)).collect();
            }
        }
        out.push(v.iter().map(|x| x * &w).collect());
    }
    Ok(out)
}

/// The surface form: `S^(k,l) = (A^(k,l) - sum_{(i,j) != 0} binom(k,i) binom(l,j)
/// w^(i,j) S^(k-i,l-j)) / w`.
fn quotient2(hom: &Partials<Row>) -> Result<Partials<Row>, Error> {
    let d = hom.get(0, 0).map_or(0, |r| r.len().saturating_sub(1));
    let weight = |i: usize, j: usize| {
        hom.get(i, j)
            .and_then(|r| r.get(d))
            .cloned()
            .unwrap_or_default()
    };
    let w = weight(0, 0).recip().ok_or(Error::ZeroWeight)?;
    let order = hom.order();
    let mut rows: Vec<Vec<Row>> = Vec::with_capacity(order + 1);
    for k in 0..=order {
        let mut row: Vec<Row> = Vec::with_capacity(order + 1 - k);
        for l in 0..=order - k {
            let mut v: Row = hom
                .get(k, l)
                .map(|a| a.iter().take(d).cloned().collect())
                .unwrap_or_default();
            for i in 0..=k {
                for j in 0..=l {
                    if i == 0 && j == 0 {
                        continue;
                    }
                    let c = &(&binomial(k, i) * &binomial(l, j)) * &weight(i, j);
                    let prev = if i == 0 {
                        row.get(l - j)
                    } else {
                        rows.get(k - i).and_then(|r| r.get(l - j))
                    };
                    if let Some(prev) = prev {
                        v = v.iter().zip(prev).map(|(x, y)| x - &(&c * y)).collect();
                    }
                }
            }
            row.push(v.iter().map(|x| x * &w).collect());
        }
        rows.push(row);
    }
    Ok(Partials::new(rows))
}

fn exacts<const D: usize>(rows: &[Row]) -> Vec<Exact<D>> {
    rows.iter().map(|r| Exact::of(r)).collect()
}

// ---------------------------------------------------------------- curves

/// An exact B-spline or NURBS curve in `D` dimensions.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve<const D: usize> {
    knots: Knots,
    /// `(w P, w)`.
    points: Vec<Row>,
}

impl<const D: usize> Curve<D> {
    /// The curve of degree `degree` with these knots, control points and weights (`None`:
    /// all 1, a polynomial B-spline), each double taken exactly.
    pub fn new(
        degree: usize,
        knots: &[f64],
        points: &[[f64; D]],
        weights: Option<&[f64]>,
    ) -> Result<Curve<D>, Error> {
        Ok(Curve {
            knots: Knots::new(degree, knots, points.len())?,
            points: homogeneous(points, weights)?,
        })
    }

    /// The degree.
    pub fn degree(&self) -> usize {
        self.knots.degree
    }

    /// The knots.
    pub fn knots(&self) -> &[f64] {
        &self.knots.values
    }

    /// `[u_p, u_n]`.
    pub fn domain(&self) -> (f64, f64) {
        self.knots.domain()
    }

    /// The span index `l` the parameter evaluates on.
    pub fn span(&self, u: f64, side: Side) -> usize {
        self.knots.span(u, side)
    }

    /// The control points, exactly.
    pub fn points(&self) -> Vec<[Rat; D]> {
        self.points
            .iter()
            .map(|r| {
                let w = r.get(D).and_then(Rat::recip).unwrap_or_default();
                array(&r.iter().take(D).map(|x| x * &w).collect::<Vec<_>>())
            })
            .collect()
    }

    /// The weights, exactly.
    pub fn weights(&self) -> Vec<Rat> {
        self.points
            .iter()
            .map(|r| r.get(D).cloned().unwrap_or_default())
            .collect()
    }

    /// The curve's data rounded to doubles (correctly, each value once). After an
    /// insertion or elevation this is near, not equal to, the exact curve.
    pub fn rounded(&self) -> CurveData<D> {
        let weights: Vec<f64> = self.weights().iter().map(Rat::to_f64).collect();
        CurveData {
            degree: self.degree(),
            knots: self.knots.values.clone(),
            points: self
                .points()
                .iter()
                .map(|p| p.each_ref().map(Rat::to_f64))
                .collect(),
            weights: if weights.iter().all(|&w| w == 1.0) {
                None
            } else {
                Some(weights)
            },
        }
    }

    fn local(&self, l: usize) -> Vec<Row> {
        let p = self.degree();
        self.points
            .iter()
            .skip(l - p)
            .take(p + 1)
            .cloned()
            .collect()
    }

    /// `C^(k)(u)` for `k = 0..=order`, by de Boor on the derivative curves.
    pub fn derivs(&self, u: f64, side: Side, order: usize) -> Result<Vec<Exact<D>>, Error> {
        let ur = rat(u)?;
        let l = self.span(u, side);
        let hom = local_derivs(
            &self.knots.exact,
            self.degree(),
            l,
            self.local(l),
            &ur,
            order,
        )
        .ok_or(Error::EmptyDomain)?;
        Ok(exacts(&quotient(&hom)?))
    }

    /// `C^(k)(u)` for `k = 0..=order`, by the sum of basis-function derivatives (an
    /// evaluation independent of [`Curve::derivs`]).
    pub fn derivs_by_basis(
        &self,
        u: f64,
        side: Side,
        order: usize,
    ) -> Result<Vec<Exact<D>>, Error> {
        let ur = rat(u)?;
        let l = self.span(u, side);
        let p = self.degree();
        let n = basis_derivs(&self.knots.exact, p, l, &ur, order);
        let local = self.local(l);
        let hom: Vec<Row> = n.iter().map(|c| combine(c, local.iter(), D + 1)).collect();
        Ok(exacts(&quotient(&hom)?))
    }

    /// `C(u)`.
    pub fn point(&self, u: f64, side: Side) -> Result<Exact<D>, Error> {
        self.derivs(u, side, 0)?
            .into_iter()
            .next()
            .ok_or(Error::EmptyDomain)
    }

    /// The blossom of the piece on span `l` at `args` (`degree` of them), as a point and
    /// its weight: with every argument `u` it is `C(u)`; at `(u_{i+1}, ..., u_{i+p})` it is
    /// control point `i`.
    pub fn blossom(&self, l: usize, args: &[f64]) -> Result<([Rat; D], Rat), Error> {
        let args = args
            .iter()
            .map(|&a| rat(a))
            .collect::<Result<Vec<_>, _>>()?;
        let row = de_boor(&self.knots.exact, self.degree(), l, self.local(l), &args)
            .ok_or(Error::EmptyDomain)?;
        let w = row.get(D).cloned().unwrap_or_default();
        let inv = w.recip().ok_or(Error::ZeroWeight)?;
        Ok((
            array(&row.iter().take(D).map(|x| x * &inv).collect::<Vec<_>>()),
            w,
        ))
    }

    /// The same curve with `u` inserted `times` times (Boehm's algorithm, exactly).
    pub fn insert_knot(&self, u: f64, times: usize) -> Result<Curve<D>, Error> {
        let mut c = self.clone();
        for _ in 0..times {
            let (knots, l) = c.knots.insert_plan(u)?;
            let ur = rat(u)?;
            let points = insert_line(&c.knots.exact, c.degree(), l, &ur, &c.points)
                .ok_or(Error::EmptyDomain)?;
            c = Curve { knots, points };
        }
        Ok(c)
    }

    /// The two pieces of the curve either side of `u`, strictly inside the domain: `u` is
    /// inserted up to multiplicity `degree`, and each piece takes the knots on its side with
    /// `degree + 1` copies of `u` at its end, and the control points those knots govern.
    /// Each piece is the curve exactly on its part of the domain.
    pub fn split(&self, u: f64) -> Result<(Curve<D>, Curve<D>), Error> {
        let c = self.insert_knot(u, self.knots.split_plan(u)?)?;
        let (low, high) = c.knots.halves(u)?;
        let n = c.points.len();
        let first = c.points.iter().take(low.count).cloned().collect();
        let second = c.points.iter().skip(n - high.count).cloned().collect();
        Ok((
            Curve {
                knots: low,
                points: first,
            },
            Curve {
                knots: high,
                points: second,
            },
        ))
    }

    /// The same curve with its degree raised by `times` (exactly, by blossoms). The end
    /// knots must have multiplicity `degree + 1`.
    pub fn elevate(&self, times: usize) -> Result<Curve<D>, Error> {
        let mut c = self.clone();
        for _ in 0..times {
            let knots = c.knots.elevate_plan()?;
            let points = elevate_line(&c.knots, &knots, &c.points).ok_or(Error::EmptyDomain)?;
            c = Curve { knots, points };
        }
        Ok(c)
    }
}

impl Knots {
    /// The knots after inserting `u` once, and the span it goes after.
    fn insert_plan(&self, u: f64) -> Result<(Knots, usize), Error> {
        if !u.is_finite() {
            return Err(Error::NonFinite);
        }
        let (a, b) = self.domain();
        if u < a || u > b {
            return Err(Error::OutsideDomain);
        }
        if self.multiplicity(u) > self.degree {
            return Err(Error::Multiplicity);
        }
        let l = self.span(u, Side::Above);
        Ok((self.inserted(u, l)?, l))
    }

    fn elevate_plan(&self) -> Result<Knots, Error> {
        if !self.clamped() {
            return Err(Error::Unclamped);
        }
        self.elevated()
    }

    /// How many times `u`, strictly inside the domain, is inserted before a split there:
    /// up to multiplicity `degree`.
    fn split_plan(&self, u: f64) -> Result<usize, Error> {
        if !u.is_finite() {
            return Err(Error::NonFinite);
        }
        let (a, b) = self.domain();
        if u <= a || u >= b {
            return Err(Error::OutsideDomain);
        }
        Ok(self.degree.saturating_sub(self.multiplicity(u)))
    }

    /// The knots of the two pieces either side of `u`, which has multiplicity `degree` or
    /// more here: the knots below `u` then `degree + 1` copies of it, and `degree + 1`
    /// copies then the knots above. Their counts are the knots below and above `u`.
    fn halves(&self, u: f64) -> Result<(Knots, Knots), Error> {
        let p = self.degree;
        let below: Vec<f64> = self.values.iter().copied().filter(|&k| k < u).collect();
        let above: Vec<f64> = self.values.iter().copied().filter(|&k| k > u).collect();
        let ends = core::iter::repeat_n(u, p + 1);
        let low: Vec<f64> = below.iter().copied().chain(ends.clone()).collect();
        let high: Vec<f64> = ends.chain(above.iter().copied()).collect();
        Ok((
            Knots::new(p, &low, below.len())?,
            Knots::new(p, &high, above.len())?,
        ))
    }
}

/// One line of control points with `u` inserted after span `l`:
/// `Q_i = (1 - a_i) P_{i-1} + a_i P_i`, `a_i = (u - u_i) / (u_{i+p} - u_i)` for
/// `i = l-p+1..=l`, the points either side kept.
fn insert_line(kr: &[Rat], p: usize, l: usize, u: &Rat, line: &[Row]) -> Option<Vec<Row>> {
    let middle = line
        .windows(2)
        .skip(l - p)
        .zip(kr.iter().skip(l - p + 1).zip(kr.iter().skip(l + 1)))
        .take(p)
        .map(|(w, (a, b))| {
            let t = (u - a).checked_div(&(b - a))?;
            match w {
                [x, y] => Some(lerp(x, y, &t)),
                _ => None,
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(
        line.iter()
            .take(l - p + 1)
            .cloned()
            .chain(middle)
            .chain(line.iter().skip(l).cloned())
            .collect(),
    )
}

/// One line of control points elevated from `old` to `new` knots:
/// `Q_j = 1/(p+1) sum_m b(t_{j+1}, ..., t_{j+p+1} without t_{j+m})`, `b` the blossom of
/// the old piece under a new span `s`, `j <= s <= j+p+1`, of the new domain.
fn elevate_line(old: &Knots, new: &Knots, line: &[Row]) -> Option<Vec<Row>> {
    let p = old.degree;
    let scale = Rat::one().checked_div(&Rat::from(p as i64 + 1))?;
    (0..new.count)
        .map(|j| {
            let s = (j.max(p + 1)..=(j + p + 1).min(new.count - 1))
                .find(|&s| matches!(new.values.get(s..s + 2), Some([a, b]) if a < b))?;
            let l = old.span(*new.values.get(s)?, Side::Above);
            let local: Vec<Row> = line.iter().skip(l - p).take(p + 1).cloned().collect();
            let args: Vec<Rat> = new.exact.iter().skip(j + 1).take(p + 1).cloned().collect();
            let h = local.first().map_or(0, Vec::len);
            let mut sum = vec![Rat::zero(); h];
            for m in 0..=p {
                let without: Vec<Rat> = args
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i != m)
                    .map(|(_, a)| a.clone())
                    .collect();
                let b = de_boor(&old.exact, p, l, local.clone(), &without)?;
                sum = sum.iter().zip(&b).map(|(x, y)| x + y).collect();
            }
            Some(sum.iter().map(|x| x * &scale).collect())
        })
        .collect()
}

// ---------------------------------------------------------------- surfaces

/// An exact B-spline or NURBS surface in `D` dimensions. Control points are row-major:
/// point `(i, j)` (`i` along `u`, `j` along `v`) is at `i * nv + j`.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface<const D: usize> {
    u: Knots,
    v: Knots,
    points: Vec<Row>,
}

impl<const D: usize> Surface<D> {
    /// The surface of degrees `(pu, pv)` with these knots, control points (row-major,
    /// `nu * nv` of them, `nu` and `nv` from the knot counts) and weights.
    pub fn new(
        degrees: (usize, usize),
        knots_u: &[f64],
        knots_v: &[f64],
        points: &[[f64; D]],
        weights: Option<&[f64]>,
    ) -> Result<Surface<D>, Error> {
        let count = |knots: &[f64], p: usize| knots.len().saturating_sub(p + 1);
        let (nu, nv) = (count(knots_u, degrees.0), count(knots_v, degrees.1));
        if nu * nv != points.len() {
            return Err(Error::KnotCount {
                expected: points.len(),
                found: nu * nv,
            });
        }
        Ok(Surface {
            u: Knots::new(degrees.0, knots_u, nu)?,
            v: Knots::new(degrees.1, knots_v, nv)?,
            points: homogeneous(points, weights)?,
        })
    }

    /// The surface whose homogeneous control points are `(weighted[k], weights[k])`, each
    /// double taken exactly: the form a NURBS library stores (`w P` and `w`), so that the
    /// reference is the stored surface itself, not one rebuilt from rounded quotients.
    pub fn from_homogeneous(
        degrees: (usize, usize),
        knots_u: &[f64],
        knots_v: &[f64],
        weighted: &[[f64; D]],
        weights: &[f64],
    ) -> Result<Surface<D>, Error> {
        if weights.len() != weighted.len() {
            return Err(Error::WeightCount {
                expected: weighted.len(),
                found: weights.len(),
            });
        }
        let points = weighted
            .iter()
            .zip(weights)
            .map(|(p, &w)| {
                let w = rat(w)?;
                if w.is_zero() {
                    return Err(Error::ZeroWeight);
                }
                let mut row = p.iter().map(|&x| rat(x)).collect::<Result<Row, _>>()?;
                row.push(w);
                Ok(row)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let count = |knots: &[f64], p: usize| knots.len().saturating_sub(p + 1);
        let (nu, nv) = (count(knots_u, degrees.0), count(knots_v, degrees.1));
        if nu * nv != points.len() {
            return Err(Error::KnotCount {
                expected: points.len(),
                found: nu * nv,
            });
        }
        Ok(Surface {
            u: Knots::new(degrees.0, knots_u, nu)?,
            v: Knots::new(degrees.1, knots_v, nv)?,
            points,
        })
    }

    /// `(pu, pv)`.
    pub fn degrees(&self) -> (usize, usize) {
        (self.u.degree, self.v.degree)
    }

    /// `(nu, nv)`: control points along `u` and along `v`.
    pub fn counts(&self) -> (usize, usize) {
        (self.u.count, self.v.count)
    }

    /// The knots along `u` and along `v`.
    pub fn knots(&self) -> (&[f64], &[f64]) {
        (&self.u.values, &self.v.values)
    }

    /// The domains along `u` and `v`.
    pub fn domain(&self) -> ((f64, f64), (f64, f64)) {
        (self.u.domain(), self.v.domain())
    }

    /// The spans along `u` and `v` a parameter pair evaluates on.
    pub fn span(&self, (u, v): (f64, f64), side: (Side, Side)) -> (usize, usize) {
        (self.u.span(u, side.0), self.v.span(v, side.1))
    }

    /// The control points, exactly, row-major.
    pub fn points(&self) -> Vec<[Rat; D]> {
        self.points
            .iter()
            .map(|r| {
                let w = r.get(D).and_then(Rat::recip).unwrap_or_default();
                array(&r.iter().take(D).map(|x| x * &w).collect::<Vec<_>>())
            })
            .collect()
    }

    /// The weights, exactly, row-major.
    pub fn weights(&self) -> Vec<Rat> {
        self.points
            .iter()
            .map(|r| r.get(D).cloned().unwrap_or_default())
            .collect()
    }

    /// The surface's data rounded to doubles (each value once).
    pub fn rounded(&self) -> SurfaceData<D> {
        let weights: Vec<f64> = self.weights().iter().map(Rat::to_f64).collect();
        SurfaceData {
            degrees: self.degrees(),
            knots_u: self.u.values.clone(),
            knots_v: self.v.values.clone(),
            points: self
                .points()
                .iter()
                .map(|p| p.each_ref().map(Rat::to_f64))
                .collect(),
            weights: if weights.iter().all(|&w| w == 1.0) {
                None
            } else {
                Some(weights)
            },
        }
    }

    /// Row `i` of the control points, `j` from `j0`, `len` of them.
    fn row(&self, i: usize, j0: usize, len: usize) -> Vec<Row> {
        self.points
            .iter()
            .skip(i * self.v.count + j0)
            .take(len)
            .cloned()
            .collect()
    }

    /// The partials `S^(i,j)` for `i + j <= order`, by de Boor along `v` for each local
    /// row, then along `u`.
    pub fn derivs(
        &self,
        (u, v): (f64, f64),
        side: (Side, Side),
        order: usize,
    ) -> Result<Partials<Exact<D>>, Error> {
        let (ur, vr) = (rat(u)?, rat(v)?);
        let (lu, lv) = self.span((u, v), side);
        let (pu, pv) = self.degrees();
        let along_v = (lu - pu..=lu)
            .map(|i| {
                local_derivs(
                    &self.v.exact,
                    pv,
                    lv,
                    self.row(i, lv - pv, pv + 1),
                    &vr,
                    order,
                )
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::EmptyDomain)?;
        let rows = (0..=order)
            .map(|k| {
                // the u-derivatives of the j-th v-derivative, j <= order - k
                (0..=order - k)
                    .map(|j| {
                        let pts: Vec<Row> = along_v
                            .iter()
                            .map(|d| d.get(j).cloned().unwrap_or_default())
                            .collect();
                        local_derivs(&self.u.exact, pu, lu, pts, &ur, k)
                            .and_then(|d| d.into_iter().nth(k))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::EmptyDomain)?;
        let s = quotient2(&Partials::new(rows))?;
        Ok(s.map(|r| Exact::of(r)))
    }

    /// The partials by the sum of basis-function derivatives along both directions (an
    /// evaluation independent of [`Surface::derivs`]).
    pub fn derivs_by_basis(
        &self,
        (u, v): (f64, f64),
        side: (Side, Side),
        order: usize,
    ) -> Result<Partials<Exact<D>>, Error> {
        let (ur, vr) = (rat(u)?, rat(v)?);
        let (lu, lv) = self.span((u, v), side);
        let (pu, pv) = self.degrees();
        let nu = basis_derivs(&self.u.exact, pu, lu, &ur, order);
        let nv = basis_derivs(&self.v.exact, pv, lv, &vr, order);
        let local: Vec<Vec<Row>> = (lu - pu..=lu)
            .map(|i| self.row(i, lv - pv, pv + 1))
            .collect();
        let rows = (0..=order)
            .map(|k| {
                (0..=order - k)
                    .map(|j| {
                        let (cu, cv) = (
                            nu.get(k).cloned().unwrap_or_default(),
                            nv.get(j).cloned().unwrap_or_default(),
                        );
                        let along: Vec<Row> = local
                            .iter()
                            .map(|r| combine(&cv, r.iter(), D + 1))
                            .collect();
                        combine(&cu, along.iter(), D + 1)
                    })
                    .collect()
            })
            .collect();
        let s = quotient2(&Partials::new(rows))?;
        Ok(s.map(|r| Exact::of(r)))
    }

    /// `S(u, v)`.
    pub fn point(&self, uv: (f64, f64), side: (Side, Side)) -> Result<Exact<D>, Error> {
        self.derivs(uv, side, 0)?
            .get(0, 0)
            .cloned()
            .ok_or(Error::EmptyDomain)
    }

    /// The knots along a direction.
    fn knots_along(&self, along: Along) -> &Knots {
        match along {
            Along::U => &self.u,
            Along::V => &self.v,
        }
    }

    /// The lines of control points along a direction: the columns (fixed `j`) along `u`,
    /// the rows (fixed `i`) along `v`.
    fn lines(&self, along: Along) -> Vec<Vec<Row>> {
        let (nu, nv) = self.counts();
        match along {
            Along::U => (0..nv)
                .map(|j| {
                    (0..nu)
                        .filter_map(|i| self.points.get(i * nv + j).cloned())
                        .collect()
                })
                .collect(),
            Along::V => (0..nu).map(|i| self.row(i, 0, nv)).collect(),
        }
    }

    /// The surface with the knots along `along` replaced by `knots` and each line along it
    /// by `f` of it.
    fn with_lines(
        &self,
        along: Along,
        knots: Knots,
        f: impl Fn(&[Row]) -> Option<Vec<Row>>,
    ) -> Result<Surface<D>, Error> {
        let lines = self
            .lines(along)
            .iter()
            .map(|line| f(line))
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::EmptyDomain)?;
        Ok(match along {
            Along::U => {
                let points = (0..knots.count)
                    .flat_map(|i| {
                        lines
                            .iter()
                            .map(move |c| c.get(i).cloned().unwrap_or_default())
                    })
                    .collect();
                Surface {
                    u: knots,
                    v: self.v.clone(),
                    points,
                }
            }
            Along::V => Surface {
                u: self.u.clone(),
                v: knots,
                points: lines.concat(),
            },
        })
    }

    /// The same surface with `u` inserted `times` times along `u`.
    pub fn insert_knot_u(&self, u: f64, times: usize) -> Result<Surface<D>, Error> {
        self.insert(u, times, Along::U)
    }

    /// The same surface with `v` inserted `times` times along `v`.
    pub fn insert_knot_v(&self, v: f64, times: usize) -> Result<Surface<D>, Error> {
        self.insert(v, times, Along::V)
    }

    fn insert(&self, t: f64, times: usize, along: Along) -> Result<Surface<D>, Error> {
        let mut s = self.clone();
        for _ in 0..times {
            let dir = s.knots_along(along);
            let (knots, l) = dir.insert_plan(t)?;
            let tr = rat(t)?;
            s = s.with_lines(along, knots, |line| {
                insert_line(&dir.exact, dir.degree, l, &tr, line)
            })?;
        }
        Ok(s)
    }

    /// The same surface with the degree along `u` raised by `times`.
    pub fn elevate_u(&self, times: usize) -> Result<Surface<D>, Error> {
        self.elevate(times, Along::U)
    }

    /// The same surface with the degree along `v` raised by `times`.
    pub fn elevate_v(&self, times: usize) -> Result<Surface<D>, Error> {
        self.elevate(times, Along::V)
    }

    fn elevate(&self, times: usize, along: Along) -> Result<Surface<D>, Error> {
        let mut s = self.clone();
        for _ in 0..times {
            let dir = s.knots_along(along);
            let knots = dir.elevate_plan()?;
            s = s.with_lines(along, knots.clone(), |line| elevate_line(dir, &knots, line))?;
        }
        Ok(s)
    }

    /// The two halves of the surface either side of `u` (strictly inside the domain along
    /// `u`): the pieces below and above it, each with end knots of multiplicity
    /// `degree + 1` at `u` (see [`Curve::split`]).
    pub fn split_u(&self, u: f64) -> Result<(Surface<D>, Surface<D>), Error> {
        self.split(u, Along::U)
    }

    /// The two halves of the surface either side of `v` along `v`.
    pub fn split_v(&self, v: f64) -> Result<(Surface<D>, Surface<D>), Error> {
        self.split(v, Along::V)
    }

    fn split(&self, t: f64, along: Along) -> Result<(Surface<D>, Surface<D>), Error> {
        let s = self.insert(t, self.knots_along(along).split_plan(t)?, along)?;
        let dir = s.knots_along(along);
        let (low, high) = dir.halves(t)?;
        let (n, below, above) = (dir.count, low.count, high.count);
        let first = s.with_lines(along, low, |line| {
            Some(line.iter().take(below).cloned().collect())
        })?;
        let second = s.with_lines(along, high, |line| {
            Some(line.iter().skip(n - above).cloned().collect())
        })?;
        Ok((first, second))
    }
}

/// A direction of a surface's parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Along {
    U,
    V,
}
