// Moved from solidcore `solidcore/crates/solidcore/src/offset/construction/bernstein.rs`, with
// `bernstein_roots` and `bernstein_halves` from
// `solidcore/crates/solidcore/src/query/tangent_edge.rs`, at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from those files): bernstein.rs: Upstream: none (not upstream;
// solidcore-native, tangency-labs/solidcore#2350 §1.1). tangent_edge.rs: Upstream: none (not
// in SMLib; SMLib's `IsTangentSector` samples five fixed stations,
// [`crate::topology::edgeuse::p2::is_tangent_sector`]).
// Changes: only these items of tangent_edge.rs moved (the rest needs curves, surfaces and the
// kernel). The binomial table is one shared table built on first use (`binomial`), so
// `Binomials` (`new`, `get`, `Default`) is gone and no operation takes it (`mul`, `elevate`,
// `add_scaled`, `add`, `sub` and the vector and bounded forms): the same Pascal sums, so the
// same values. The vector of three polynomials is the struct `Poly3` (`Bernstein3` and
// `Bounded3` its aliases) where the pin had `[Bernstein2; 3]` and `[Bounded; 3]`, and the
// pin's free functions are its methods: `dot`/`dot_bounded` are `Poly3::dot`,
// `cross`/`cross_bounded` `Poly3::cross`, `sub3`/`sub3_bounded` `Poly3::sub`,
// `mul3`/`mul3_bounded` `Poly3::mul_each`, `deriv_u3`/`deriv_u3_bounded` and
// `deriv_v3`/`deriv_v3_bounded` `Poly3::deriv_u` and `Poly3::deriv_v`, `restrict3`
// `Poly3::restrict`, `eval3` `Poly3::eval` and `unbounded3` `Poly3::unbounded`, over the
// trait `Algebra` that `Bernstein2` and `Bounded` implement (each method the type's own).
// `Bounded::quarter` takes two `Half`s where the pin took the bools `upper_u` and
// `upper_v`. `bernstein_roots` takes `RootLimits` and returns `Roots` (the places and
// whether all were isolated) where the pin took a depth, a `&mut usize` work budget and a
// `&mut Vec` it appended to and returned the flag; the recursion is the private
// `roots_in`, unchanged but for those, and its `bernstein_value` is this file's
// `eval_1d` (the same de Casteljau levels, `(1 - s) x + s y` for `x (1 - s) + y s`: the same
// products and sums, so the same bits). Errors are `NumError::InvalidInput` with the pin's
// operation and reasons. `gamma` is `scx_num::certified::gamma`, `Vec3`
// `scx_num::vector3d::Vec3`. The tests are in the difftest (`t_bern`), checked against exact
// rationals. Results unchanged bit for bit.
//! Tensor-product Bernstein polynomials on the unit square: the exact algebra the offset
//! certificates bound (products, derivatives, subdivision, coefficient bounds), and the
//! roots of a polynomial in one variable in Bernstein form.
//!
//! Upstream: none (not upstream; solidcore-native, tangency-labs/solidcore#2350 §1.1, and
//! solidcore's tangent-edge proof).
//!
//! | Rust | Upstream |
//! |------|----------|
//! | [`Bernstein2`], [`binomial`], [`Bounded`], [`Poly3`], [`Half`] | none |
//! | [`bernstein_roots`], [`RootLimits`], [`Roots`] | none |
//!
//! A polynomial of degrees `(du, dv)` is `sum c[i][j] B_i^du(s) B_j^dv(t)` on `[0, 1]^2`.
//! Its values lie in the convex hull of its coefficients, so [`Bernstein2::min_coefficient`]
//! and [`Bernstein2::max_coefficient`] bound it (Farouki & Rajan 1987); subdividing tightens
//! the bound. The product of degrees `(du, dv)` has the coefficients
//! `sum C(a_u, i_a) C(a_v, j_a) C(b_u, i_b) C(b_v, j_b) a_(i_a j_a) b_(i_b j_b) / (C(du, i) C(dv, j))`
//! over `i_a + i_b = i`, `j_a + j_b = j`; elevation is a product with the polynomial whose
//! coefficients are all 1; a derivative's coefficients are `n (c_(i+1) - c_i)`; a split is
//! de Casteljau's (Farin, *Curves and Surfaces for CAGD*, §5).
//!
//! A [`Bounded`] polynomial carries, beside the coefficients floating point gave it, a bound
//! of their distance from the exact polynomial's: a running error bound (Higham 2002, §3.3),
//! each operation's rounding counted as `gamma` of the magnitudes it combines
//! ([`product_rounding`]) and the errors of its operands carried through.
//!
//! [`bernstein_roots`] isolates the roots of a polynomial in one variable by the variation
//! diminishing property: no sign change in the coefficients means no root, exactly one
//! means one simple root (found by bisection to rounding), otherwise the halves are
//! searched, to the depth and number of subdivisions [`RootLimits`] allows.
//!
//! Deviations: none (new code). Degrees are bounded by [`MAX_DEGREE`].

use scx_num::certified::gamma;
use scx_num::vector3d::Vec3;
use scx_num::{NumError, Result};
use std::sync::OnceLock;

/// The operation the refusals report (solidcore's `offset::construction::bernstein`).
pub const OP: &str = "offset::construction::bernstein";

/// The largest degree in either direction a product may reach: above the certificates'
/// degrees, the highest of which is the regularity test's tangent bound on a G2 round's
/// rational septic net, `q1 (x2 + c^2)` of degree 100.
pub const MAX_DEGREE: usize = 128;

const fn invalid(reason: &'static str) -> NumError {
    NumError::invalid_input(OP, reason)
}

/// Pascal's triangle up to row `2 * MAX_DEGREE`, built once.
fn pascal() -> &'static [Vec<f64>] {
    static ROWS: OnceLock<Vec<Vec<f64>>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let mut rows: Vec<Vec<f64>> = Vec::with_capacity(2 * MAX_DEGREE + 1);
        let mut row = vec![1.0];
        for _ in 0..=2 * MAX_DEGREE {
            let next: Vec<f64> = core::iter::once(1.0)
                .chain(row.windows(2).map(|w| match *w {
                    [a, b] => a + b,
                    _ => 0.0,
                }))
                .chain(core::iter::once(1.0))
                .collect();
            rows.push(row);
            row = next;
        }
        rows
    })
}

/// `C(n, k)` as `f64` (Pascal's sums, rounded past `2^53`), for `n <= 2 * MAX_DEGREE`; zero
/// for `k > n` or a larger `n`.
#[must_use]
pub fn binomial(n: usize, k: usize) -> f64 {
    pascal()
        .get(n)
        .and_then(|r| r.get(k))
        .copied()
        .unwrap_or(0.0)
}

/// The operations a polynomial and a bounded polynomial share, so [`Poly3`] serves both.
pub trait Algebra: Sized {
    /// The product.
    ///
    /// # Errors
    ///
    /// `InvalidInput` when a degree of the product exceeds [`MAX_DEGREE`].
    fn mul(&self, other: &Self) -> Result<Self>;
    /// The sum, in the larger degrees.
    ///
    /// # Errors
    ///
    /// Those of the elevation.
    fn add(&self, other: &Self) -> Result<Self>;
    /// The difference, in the larger degrees.
    ///
    /// # Errors
    ///
    /// Those of the elevation.
    fn sub(&self, other: &Self) -> Result<Self>;
    /// The partial derivative in the first parameter.
    #[must_use]
    fn deriv_u(&self) -> Self;
    /// The partial derivative in the second parameter.
    #[must_use]
    fn deriv_v(&self) -> Self;
}

/// A tensor-product Bernstein polynomial on `[0, 1]^2`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bernstein2 {
    du: usize,
    dv: usize,
    /// `c[i * (dv + 1) + j]`.
    c: Vec<f64>,
}

impl Bernstein2 {
    /// The polynomial of degrees `(du, dv)` with coefficients `c` (row `i` holds
    /// `c[i][0..=dv]`).
    ///
    /// # Errors
    ///
    /// `InvalidInput` when `c` does not hold `(du + 1) (dv + 1)` values or a degree
    /// exceeds [`MAX_DEGREE`].
    pub fn new(du: usize, dv: usize, c: Vec<f64>) -> Result<Self> {
        if du > MAX_DEGREE || dv > MAX_DEGREE {
            return Err(invalid("Bernstein degree too large"));
        }
        if c.len() != (du + 1) * (dv + 1) {
            return Err(invalid("Bernstein coefficient count"));
        }
        Ok(Self { du, dv, c })
    }

    /// The constant `x`.
    #[must_use]
    pub fn constant(x: f64) -> Self {
        Self {
            du: 0,
            dv: 0,
            c: vec![x],
        }
    }

    /// The degrees `(du, dv)`.
    #[must_use]
    pub const fn degrees(&self) -> (usize, usize) {
        (self.du, self.dv)
    }

    /// The coefficients, row by row.
    #[must_use]
    pub fn coefficients(&self) -> &[f64] {
        &self.c
    }

    fn rows(&self) -> impl Iterator<Item = &[f64]> + '_ {
        self.c.chunks(self.dv + 1)
    }

    /// The smallest coefficient: a lower bound of the polynomial on the square.
    #[must_use]
    pub fn min_coefficient(&self) -> f64 {
        self.c.iter().copied().fold(f64::INFINITY, f64::min)
    }

    /// The largest coefficient: an upper bound of the polynomial on the square.
    #[must_use]
    pub fn max_coefficient(&self) -> f64 {
        self.c.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }

    /// The largest coefficient magnitude: a bound of `|f|` on the square.
    #[must_use]
    pub fn max_abs_coefficient(&self) -> f64 {
        self.c.iter().fold(0.0, |m, x| m.max(x.abs()))
    }

    /// `x * self`.
    #[must_use]
    pub fn scale(&self, x: f64) -> Self {
        Self {
            du: self.du,
            dv: self.dv,
            c: self.c.iter().map(|c| c * x).collect(),
        }
    }

    /// The product `self * other`, of degrees the sums of theirs.
    ///
    /// # Errors
    ///
    /// `InvalidInput` when a degree of the product exceeds [`MAX_DEGREE`].
    pub fn mul(&self, other: &Self) -> Result<Self> {
        let (du, dv) = (self.du + other.du, self.dv + other.dv);
        if du > MAX_DEGREE || dv > MAX_DEGREE {
            return Err(invalid("Bernstein product degree too large"));
        }
        let mut out = vec![0.0; (du + 1) * (dv + 1)];
        for (ia, ra) in self.rows().enumerate() {
            for (ja, &x) in ra.iter().enumerate() {
                let x = x * binomial(self.du, ia) * binomial(self.dv, ja);
                for (ib, rb) in other.rows().enumerate() {
                    let row = (ia + ib) * (dv + 1);
                    for (jb, &y) in rb.iter().enumerate() {
                        let y = y * binomial(other.du, ib) * binomial(other.dv, jb);
                        if let Some(slot) = out.get_mut(row + ja + jb) {
                            *slot += x * y;
                        }
                    }
                }
            }
        }
        for (i, row) in out.chunks_mut(dv + 1).enumerate() {
            for (j, slot) in row.iter_mut().enumerate() {
                *slot /= binomial(du, i) * binomial(dv, j);
            }
        }
        Self::new(du, dv, out)
    }

    /// The same polynomial written in degrees `(du, dv)` (at least its own).
    ///
    /// # Errors
    ///
    /// `InvalidInput` for a degree below the polynomial's own or above [`MAX_DEGREE`].
    pub fn elevate(&self, du: usize, dv: usize) -> Result<Self> {
        if du < self.du || dv < self.dv {
            return Err(invalid("Bernstein elevation below the degree"));
        }
        if du == self.du && dv == self.dv {
            return Ok(self.clone());
        }
        let ones = Self::new(
            du - self.du,
            dv - self.dv,
            vec![1.0; (du - self.du + 1) * (dv - self.dv + 1)],
        )?;
        self.mul(&ones)
    }

    /// `self + x * other`, in the larger of the two degrees.
    ///
    /// # Errors
    ///
    /// Those of [`Self::elevate`].
    pub fn add_scaled(&self, other: &Self, x: f64) -> Result<Self> {
        let (du, dv) = (self.du.max(other.du), self.dv.max(other.dv));
        let a = self.elevate(du, dv)?;
        let o = other.elevate(du, dv)?;
        let c = a.c.iter().zip(&o.c).map(|(p, q)| p + x * q).collect();
        Self::new(du, dv, c)
    }

    /// `self + other`.
    ///
    /// # Errors
    ///
    /// Those of [`Self::elevate`].
    pub fn add(&self, other: &Self) -> Result<Self> {
        self.add_scaled(other, 1.0)
    }

    /// `self - other`.
    ///
    /// # Errors
    ///
    /// Those of [`Self::elevate`].
    pub fn sub(&self, other: &Self) -> Result<Self> {
        self.add_scaled(other, -1.0)
    }

    /// The partial derivative in the first parameter (degree `du - 1`; the zero
    /// polynomial of degree `(0, dv)` for a constant in it).
    #[must_use]
    pub fn deriv_u(&self) -> Self {
        if self.du == 0 {
            return Self {
                du: 0,
                dv: self.dv,
                c: vec![0.0; self.dv + 1],
            };
        }
        #[allow(clippy::cast_precision_loss)] // a degree, at most `MAX_DEGREE`
        let k = self.du as f64;
        let rows: Vec<&[f64]> = self.rows().collect();
        let c = rows
            .windows(2)
            .flat_map(|w| match *w {
                [a, b] => a
                    .iter()
                    .zip(b.iter())
                    .map(|(x, y)| k * (y - x))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect();
        Self {
            du: self.du - 1,
            dv: self.dv,
            c,
        }
    }

    /// The partial derivative in the second parameter.
    #[must_use]
    pub fn deriv_v(&self) -> Self {
        if self.dv == 0 {
            return Self {
                du: self.du,
                dv: 0,
                c: vec![0.0; self.du + 1],
            };
        }
        #[allow(clippy::cast_precision_loss)] // a degree, at most `MAX_DEGREE`
        let k = self.dv as f64;
        let c = self
            .rows()
            .flat_map(|r| {
                r.windows(2)
                    .map(|w| match *w {
                        [x, y] => k * (y - x),
                        _ => 0.0,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        Self {
            du: self.du,
            dv: self.dv - 1,
            c,
        }
    }

    fn transpose(&self) -> Self {
        let mut c = Vec::with_capacity(self.c.len());
        for j in 0..=self.dv {
            c.extend(self.c.iter().skip(j).step_by(self.dv + 1));
        }
        Self {
            du: self.dv,
            dv: self.du,
            c,
        }
    }

    /// The two halves of the polynomial split at `t` of the second parameter, each
    /// reparametrised to `[0, 1]` (de Casteljau).
    #[must_use]
    pub fn split_v(&self, t: f64) -> (Self, Self) {
        let mut left = Vec::with_capacity(self.c.len());
        let mut right = Vec::with_capacity(self.c.len());
        for row in self.rows() {
            let (l, r) = split_1d(row, t);
            left.extend(l);
            right.extend(r);
        }
        (
            Self {
                du: self.du,
                dv: self.dv,
                c: left,
            },
            Self {
                du: self.du,
                dv: self.dv,
                c: right,
            },
        )
    }

    /// The two halves split at `t` of the first parameter.
    #[must_use]
    pub fn split_u(&self, t: f64) -> (Self, Self) {
        let (l, r) = self.transpose().split_v(t);
        (l.transpose(), r.transpose())
    }

    /// The polynomial on `[u0, u1] x [v0, v1]` (inside the square), reparametrised to
    /// the unit square.
    #[must_use]
    pub fn restrict(&self, u0: f64, u1: f64, v0: f64, v1: f64) -> Self {
        let mut p = self.clone();
        if u0 > 0.0 {
            p = p.split_u(u0).1;
        }
        if u1 < 1.0 {
            p = p.split_u((u1 - u0) / (1.0 - u0)).0;
        }
        if v0 > 0.0 {
            p = p.split_v(v0).1;
        }
        if v1 < 1.0 {
            p = p.split_v((v1 - v0) / (1.0 - v0)).0;
        }
        p
    }

    /// The value at `(u, v)` (de Casteljau).
    #[must_use]
    pub fn eval(&self, u: f64, v: f64) -> f64 {
        let mut scratch = Vec::new();
        let column: Vec<f64> = self.rows().map(|r| eval_1d(r, v, &mut scratch)).collect();
        eval_1d(&column, u, &mut scratch)
    }
}

impl Algebra for Bernstein2 {
    fn mul(&self, other: &Self) -> Result<Self> {
        Self::mul(self, other)
    }
    fn add(&self, other: &Self) -> Result<Self> {
        Self::add(self, other)
    }
    fn sub(&self, other: &Self) -> Result<Self> {
        Self::sub(self, other)
    }
    fn deriv_u(&self) -> Self {
        Self::deriv_u(self)
    }
    fn deriv_v(&self) -> Self {
        Self::deriv_v(self)
    }
}

/// de Casteljau's halves of the Bernstein coefficients `c` at `t`.
fn split_1d(c: &[f64], t: f64) -> (Vec<f64>, Vec<f64>) {
    let mut left = Vec::with_capacity(c.len());
    let mut right = Vec::with_capacity(c.len());
    let mut cur = c.to_vec();
    while let (Some(&first), Some(&last)) = (cur.first(), cur.last()) {
        left.push(first);
        right.push(last);
        if cur.len() == 1 {
            break;
        }
        cur = cur
            .windows(2)
            .map(|w| match *w {
                [a, b] => (1.0 - t) * a + t * b,
                _ => 0.0,
            })
            .collect();
    }
    right.reverse();
    (left, right)
}

/// The value at `t` of the Bernstein coefficients `c` (de Casteljau; 0 for none), reduced
/// in place in `cur` (its contents are replaced; passed in so one buffer serves many
/// calls). Each level replaces `cur[i]` by `(1 - t) cur[i] + t cur[i + 1]`, walking down
/// from the end so each right neighbour is still the previous level's.
fn eval_1d(c: &[f64], t: f64, cur: &mut Vec<f64>) -> f64 {
    cur.clear();
    cur.extend_from_slice(c);
    let mut len = cur.len();
    while len > 1 {
        len -= 1;
        let Some((last, head)) = cur.get_mut(..=len).and_then(<[f64]>::split_last_mut) else {
            break;
        };
        let mut right = *last;
        for x in head.iter_mut().rev() {
            let a = *x;
            *x = (1.0 - t) * a + t * right;
            right = a;
        }
    }
    cur.first().copied().unwrap_or(0.0)
}

/// A vector of three polynomials: a polynomial map of the square into space
/// ([`Bernstein3`]), or the same with error bounds ([`Bounded3`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Poly3<P> {
    /// The first component.
    pub x: P,
    /// The second component.
    pub y: P,
    /// The third component.
    pub z: P,
}

/// A polynomial map of the square into space.
pub type Bernstein3 = Poly3<Bernstein2>;

/// A [`Bernstein3`] with each component's error bound.
pub type Bounded3 = Poly3<Bounded>;

impl<P> Poly3<P> {
    /// The vector of `x`, `y` and `z`.
    pub const fn new(x: P, y: P, z: P) -> Self {
        Self { x, y, z }
    }

    /// Each component mapped by `f`.
    pub fn map<Q>(&self, mut f: impl FnMut(&P) -> Q) -> Poly3<Q> {
        Poly3 {
            x: f(&self.x),
            y: f(&self.y),
            z: f(&self.z),
        }
    }
}

impl<P: Algebra> Poly3<P> {
    /// `self . other`: `x x' + y y'`, then `+ z z'`.
    ///
    /// # Errors
    ///
    /// Those of [`Algebra::mul`] and [`Algebra::add`].
    pub fn dot(&self, other: &Self) -> Result<P> {
        self.x
            .mul(&other.x)?
            .add(&self.y.mul(&other.y)?)?
            .add(&self.z.mul(&other.z)?)
    }

    /// `self x other`.
    ///
    /// # Errors
    ///
    /// Those of [`Algebra::mul`] and [`Algebra::sub`].
    pub fn cross(&self, other: &Self) -> Result<Self> {
        let (a, b) = (self, other);
        Ok(Self {
            x: a.y.mul(&b.z)?.sub(&a.z.mul(&b.y)?)?,
            y: a.z.mul(&b.x)?.sub(&a.x.mul(&b.z)?)?,
            z: a.x.mul(&b.y)?.sub(&a.y.mul(&b.x)?)?,
        })
    }

    /// `self - other`, componentwise.
    ///
    /// # Errors
    ///
    /// Those of [`Algebra::sub`].
    pub fn sub(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            x: self.x.sub(&other.x)?,
            y: self.y.sub(&other.y)?,
            z: self.z.sub(&other.z)?,
        })
    }

    /// Each component times the scalar polynomial `s`.
    ///
    /// # Errors
    ///
    /// Those of [`Algebra::mul`].
    pub fn mul_each(&self, s: &P) -> Result<Self> {
        Ok(Self {
            x: self.x.mul(s)?,
            y: self.y.mul(s)?,
            z: self.z.mul(s)?,
        })
    }

    /// The componentwise first-parameter derivative.
    #[must_use]
    pub fn deriv_u(&self) -> Self {
        self.map(Algebra::deriv_u)
    }

    /// The componentwise second-parameter derivative.
    #[must_use]
    pub fn deriv_v(&self) -> Self {
        self.map(Algebra::deriv_v)
    }
}

impl Bernstein3 {
    /// The componentwise restriction ([`Bernstein2::restrict`]).
    #[must_use]
    pub fn restrict(&self, u0: f64, u1: f64, v0: f64, v1: f64) -> Self {
        self.map(|p| p.restrict(u0, u1, v0, v1))
    }

    /// The value at `(u, v)`.
    #[must_use]
    pub fn eval(&self, u: f64, v: f64) -> Vec3 {
        Vec3::new(self.x.eval(u, v), self.y.eval(u, v), self.z.eval(u, v))
    }
}

impl Bounded3 {
    /// The polynomials, without their bounds.
    #[must_use]
    pub fn unbounded(self) -> Bernstein3 {
        Poly3 {
            x: self.x.f,
            y: self.y.f,
            z: self.z.f,
        }
    }
}

/// The rounded operations behind one coefficient of a Bernstein product of degrees
/// `(du, dv)` ([`Bernstein2::mul`]), relative to the product of the factors' largest
/// coefficient magnitudes. Each of the at most `(du + 1)(dv + 1)` terms multiplies each
/// factor's coefficient by two binomials and the two together, five products; the binomials
/// come from Pascal's triangle, sums of positive numbers rounded past `2^53`, so one of row
/// `n` is within `gamma(n)`, and the four of a term are of rows adding to `du + dv`. The sum
/// takes a rounding per term, and the quotient two binomials of rows `du` and `dv` and two
/// divisions. By Vandermonde's identity the weights of each sum add to 1, so the terms'
/// magnitudes add to at most the factors' largest magnitudes multiplied. A degree elevation
/// is a product with ones and counts the same.
#[must_use]
pub fn product_rounding(du: usize, dv: usize) -> usize {
    let terms = du.saturating_add(1).saturating_mul(dv.saturating_add(1));
    du.saturating_add(dv)
        .saturating_mul(2)
        .saturating_add(terms)
        .saturating_add(7)
}

/// `x` raised past the rounding of the few operations that computed it from nonnegative
/// terms (at most 8, each within the unit roundoff of its value), so a bound computed in
/// floating point is a bound, and past underflow. Below the normal range a rounding is
/// absolute, at most half the least subnormal, in the operation counted as well as in the
/// bound's own arithmetic; one `f64::MIN_POSITIVE` (2^-1022) covers far more of them than
/// any form takes (2^51 times the least subnormal).
fn rounded_up(x: f64) -> f64 {
    x * (1.0 + gamma(16)) + f64::MIN_POSITIVE
}

/// Which half of `[0, 1]` a [`Bounded::quarter`] keeps in one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// `[0, 1/2]`.
    Lower,
    /// `[1/2, 1]`.
    Upper,
}

impl Half {
    /// The half's ends.
    #[must_use]
    pub const fn range(self) -> (f64, f64) {
        match self {
            Self::Lower => (0.0, 0.5),
            Self::Upper => (0.5, 1.0),
        }
    }
}

/// A polynomial as floating point computed it, and `err`, a bound of every coefficient's
/// distance from the coefficient the same operations on the exact operands give (a running
/// error bound, Higham 2002, §3.3; underflow aside, as in the standard model).
#[derive(Debug, Clone, PartialEq)]
pub struct Bounded {
    /// The polynomial as computed.
    pub f: Bernstein2,
    /// The bound of its coefficients' distance from the exact ones.
    pub err: f64,
}

impl Bounded {
    /// `f`, exact.
    #[must_use]
    pub const fn exact(f: Bernstein2) -> Self {
        Self { f, err: 0.0 }
    }

    /// Whether every coefficient and the bound are finite. An overflow leaves an infinite
    /// or NaN coefficient, which the coefficients' extremes pass over, so a form that is
    /// not finite bounds nothing.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.err.is_finite() && self.f.coefficients().iter().all(|x| x.is_finite())
    }

    /// The product: the operands' errors carried (`|a| e_b + e_a |b| + e_a e_b`) and the
    /// product's own rounding ([`product_rounding`]).
    ///
    /// # Errors
    ///
    /// Those of [`Bernstein2::mul`].
    pub fn mul(&self, other: &Self) -> Result<Self> {
        let f = self.f.mul(&other.f)?;
        let (ma, mb) = (self.f.max_abs_coefficient(), other.f.max_abs_coefficient());
        let (du, dv) = f.degrees();
        let carried = ma * other.err + self.err * mb + self.err * other.err;
        // `ma * mb` first: `gamma * ma` alone can underflow before the larger factor
        let err = rounded_up(carried + gamma(product_rounding(du, dv)) * (ma * mb));
        Ok(Self { f, err })
    }

    /// `self + x * other` ([`Bernstein2::add_scaled`]): both raised to the larger degrees
    /// (a product with ones each), then two roundings per coefficient.
    ///
    /// # Errors
    ///
    /// Those of [`Bernstein2::add_scaled`].
    pub fn add_scaled(&self, other: &Self, x: f64) -> Result<Self> {
        let f = self.f.add_scaled(&other.f, x)?;
        let (du, dv) = f.degrees();
        let size = self.f.max_abs_coefficient() + x.abs() * other.f.max_abs_coefficient();
        let ops = product_rounding(du, dv).saturating_add(2);
        let err = rounded_up(self.err + x.abs() * other.err + gamma(ops) * size);
        Ok(Self { f, err })
    }

    /// `self - other`.
    ///
    /// # Errors
    ///
    /// Those of [`Bernstein2::add_scaled`].
    pub fn sub(&self, other: &Self) -> Result<Self> {
        self.add_scaled(other, -1.0)
    }

    /// `self + other`.
    ///
    /// # Errors
    ///
    /// Those of [`Bernstein2::add_scaled`].
    pub fn add(&self, other: &Self) -> Result<Self> {
        self.add_scaled(other, 1.0)
    }

    /// `x * self`: one rounding.
    #[must_use]
    pub fn scale(&self, x: f64) -> Self {
        let f = self.f.scale(x);
        let err = rounded_up(x.abs() * self.err + gamma(1) * f.max_abs_coefficient());
        Self { f, err }
    }

    /// The first-parameter derivative: each coefficient `n (c_(i+1) - c_i)`, two roundings
    /// of its own value, and twice `n` times the carried error.
    #[must_use]
    pub fn deriv_u(&self) -> Self {
        let f = self.f.deriv_u();
        #[allow(clippy::cast_precision_loss)] // a degree, at most `MAX_DEGREE`
        let n = self.f.degrees().0 as f64;
        let err = rounded_up(2.0 * n * self.err + gamma(2) * f.max_abs_coefficient());
        Self { f, err }
    }

    /// The second-parameter derivative, as [`Self::deriv_u`].
    #[must_use]
    pub fn deriv_v(&self) -> Self {
        let f = self.f.deriv_v();
        #[allow(clippy::cast_precision_loss)] // a degree, at most `MAX_DEGREE`
        let n = self.f.degrees().1 as f64;
        let err = rounded_up(2.0 * n * self.err + gamma(2) * f.max_abs_coefficient());
        Self { f, err }
    }

    /// One quarter of the square, the half `u` of the first parameter by the half `v` of
    /// the second ([`Bernstein2::restrict`]): one de Casteljau split at 1/2 in each
    /// parameter, `du + dv` levels that each halve exactly and add with one rounding,
    /// convex combinations that carry the error unchanged.
    #[must_use]
    pub fn quarter(&self, u: Half, v: Half) -> Self {
        let ((u0, u1), (v0, v1)) = (u.range(), v.range());
        let f = self.f.restrict(u0, u1, v0, v1);
        let (du, dv) = f.degrees();
        let err = rounded_up(self.err + gamma(du + dv) * self.f.max_abs_coefficient());
        Self { f, err }
    }
}

impl Algebra for Bounded {
    fn mul(&self, other: &Self) -> Result<Self> {
        Self::mul(self, other)
    }
    fn add(&self, other: &Self) -> Result<Self> {
        Self::add(self, other)
    }
    fn sub(&self, other: &Self) -> Result<Self> {
        Self::sub(self, other)
    }
    fn deriv_u(&self) -> Self {
        Self::deriv_u(self)
    }
    fn deriv_v(&self) -> Self {
        Self::deriv_v(self)
    }
}

/// How far [`bernstein_roots`] searches: the deepest halving, and the most halvings in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootLimits {
    /// The most times a piece is halved.
    pub depth: u32,
    /// The most halvings in all.
    pub subdivisions: usize,
}

impl RootLimits {
    /// Solidcore's limits for a rail crossing (`query::tangent_edge`'s `MAX_DEPTH` and
    /// `MAX_SUBDIVISIONS`).
    pub const DEFAULT: Self = Self {
        depth: 48,
        subdivisions: 4096,
    };
}

impl Default for RootLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The roots [`bernstein_roots`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct Roots {
    /// The roots, in the order found: a piece's left end before its right, a piece before
    /// the one after it. Where `isolated` is false, the ends of each piece left undecided
    /// stand for the roots it holds.
    pub places: Vec<f64>,
    /// Whether every root was isolated: false when the limits ran out on a piece whose
    /// coefficients change sign more than once (a root of even multiplicity, or roots
    /// closer than the depth resolves).
    pub isolated: bool,
}

/// The roots in `[lo, hi]` of the polynomial with Bernstein coefficients `c` there: none
/// where the coefficients keep one sign, exactly one where they change sign once (found by
/// bisection to rounding), the end where an end coefficient is zero, otherwise those of each
/// half, within `limits`.
#[must_use]
pub fn bernstein_roots(c: &[f64], lo: f64, hi: f64, limits: RootLimits) -> Roots {
    let mut places = Vec::new();
    let mut work = limits.subdivisions;
    let isolated = roots_in(c, lo, hi, limits.depth, &mut work, &mut places);
    Roots { places, isolated }
}

/// [`bernstein_roots`] on one piece, `work` the halvings left, the roots appended to `out`.
fn roots_in(c: &[f64], lo: f64, hi: f64, depth: u32, work: &mut usize, out: &mut Vec<f64>) -> bool {
    let (Some(&first), Some(&last)) = (c.first(), c.last()) else {
        return true;
    };
    if first == 0.0 {
        out.push(lo);
    }
    if last == 0.0 {
        out.push(hi);
    }
    let signs: Vec<bool> = c.iter().filter(|x| **x != 0.0).map(|x| *x > 0.0).collect();
    let changes = signs
        .windows(2)
        .filter(|w| matches!(w, [x, y] if x != y))
        .count();
    if changes == 0 {
        return true;
    }
    if changes == 1 && first != 0.0 && last != 0.0 {
        let mut scratch = Vec::with_capacity(c.len());
        let (mut a, mut b) = (0.0, 1.0);
        for _ in 0..64 {
            let m = 0.5 * (a + b);
            if !(m > a && m < b) {
                break;
            }
            let value = eval_1d(c, m, &mut scratch);
            if (value > 0.0) == (first > 0.0) {
                a = m;
            } else {
                b = m;
            }
        }
        out.push(lo + (hi - lo) * 0.5 * (a + b));
        return true;
    }
    if depth == 0 || *work == 0 {
        out.extend([lo, hi]);
        return false;
    }
    *work -= 1;
    let mid = 0.5 * (lo + hi);
    let (l, r) = bernstein_halves(c);
    let left = roots_in(&l, lo, mid, depth - 1, work, out);
    let right = roots_in(&r, mid, hi, depth - 1, work, out);
    left && right
}

/// The Bernstein coefficients of the polynomial `c` on `[0, 1/2]` and `[1/2, 1]`.
fn bernstein_halves(c: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut b = c.to_vec();
    while let (Some(&f), Some(&l)) = (b.first(), b.last()) {
        left.push(f);
        right.push(l);
        b = b
            .windows(2)
            .map(|w| if let [x, y] = w { 0.5 * (x + y) } else { 0.0 })
            .collect();
    }
    right.reverse();
    (left, right)
}
