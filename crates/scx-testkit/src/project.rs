// Written for solidcorex. Apache-2.0.
//! Every stationary point of the distance from a point to an ellipse, a torus and a cone,
//! certified, with the certified nearest (research 59, research 63 §8 T1 and T3).
//!
//! A projection is the list of [`Locus`]: points where the distance's gradient in the
//! parameters is zero (or a singular point of the surface), and whole circles of them where
//! the query sits on an axis. Each carries an enclosure of its distance sure to hold the
//! true one, its kind (minimum, saddle, maximum), and its parameter and point to about
//! 30 digits. [`Nearest`] names the nearest locus once its distance is separated from every
//! other; where it cannot be (a symmetric query with two equal feet), the candidates.
//!
//! - **Ellipse** `C + a cos t X + b sin t Y` (T1), exact for the frame as given in 3D: with
//!   `s = tan(t/2)` the squared distance is `N(s) / (1 + s^2)^2` for a rational quartic `N`,
//!   and its stationary points are the real roots of a rational quartic `Q`, isolated with
//!   certainty ([`crate::poly`]); `t = pi` is a root of the same quartic with `X, Y` turned
//!   over. `Q` is zero only for a true circle with the query on its axis.
//! - **Torus** and **cone** (T3) as surfaces of revolution about the frame's `z` through its
//!   origin, the radial plane spanned by `x` and `y`. For a frame orthonormal to rounding
//!   this is the parameterised shape to rounding (research 59 §T3: the decision). The
//!   stationary points lie in the query's meridian plane, the feet on two circles (torus)
//!   or two lines (cone), solved in closed form in [`Bound`] arithmetic.
//!
//! Parameters and points are double-doubles from the enclosures' midpoints, their radii
//! the enclosure plus the double-double functions' error. A precision loop narrows the
//! enclosures until the nearest and every kind are decided, up to a limit.

use crate::analytic::{Cone, Ellipse, Torus, V3};
use crate::bound::{self, rat_to_dd, up, Bound};
use crate::dd::{self, Dd};
use crate::poly::Poly;
use crate::rat::Rat;

/// What a stationary point is to the distance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A local minimum.
    Min,
    /// A saddle: a minimum one way, a maximum the other.
    Saddle,
    /// A local maximum.
    Max,
    /// Neither, or not decided: an inflection, a degenerate direction, or an enclosure that
    /// still held zero at the precision limit.
    Flat,
    /// A singular point of the surface (a cone's apex, a horn or spindle torus's pinch),
    /// listed whether or not the distance is stationary there.
    Apex,
}

/// A parameter: its midpoint and a radius certain to reach the true value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Enclosed {
    /// The midpoint.
    pub mid: Dd,
    /// `|true - mid| <= radius`.
    pub radius: f64,
}

/// One stationary point.
#[derive(Clone, Debug, PartialEq)]
pub struct Stationary<P> {
    /// Its parameter.
    pub param: P,
    /// The point.
    pub point: V3,
    /// `|true point - point| <= point_radius` (Euclidean).
    pub point_radius: f64,
    /// Its distance from the query, enclosed.
    pub distance: Bound,
    /// What it is.
    pub kind: Kind,
}

/// A circle of stationary points, the distance the same all round: a query on the axis.
#[derive(Clone, Debug, PartialEq)]
pub struct StationaryCircle {
    /// Its centre (to double-double rounding).
    pub centre: V3,
    /// Its plane's normal, not normalised.
    pub normal: V3,
    /// Its radius, enclosed.
    pub radius: Bound,
    /// The distance of every point of it from the query, enclosed.
    pub distance: Bound,
    /// What it is across the circle ([`Kind::Flat`] when every point of the curve is
    /// stationary, or the kind changes along it).
    pub kind: Kind,
}

/// A stationary point or a circle of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Locus<P> {
    /// An isolated point.
    Point(Stationary<P>),
    /// A circle.
    Circle(StationaryCircle),
}

impl<P> Locus<P> {
    /// Its distance from the query, enclosed.
    pub fn distance(&self) -> &Bound {
        match self {
            Locus::Point(p) => &p.distance,
            Locus::Circle(c) => &c.distance,
        }
    }

    /// What it is.
    pub fn kind(&self) -> Kind {
        match self {
            Locus::Point(p) => p.kind,
            Locus::Circle(c) => c.kind,
        }
    }

    /// The point, for a point.
    pub fn point(&self) -> Option<&Stationary<P>> {
        match self {
            Locus::Point(p) => Some(p),
            Locus::Circle(_) => None,
        }
    }
}

/// Which locus is nearest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Nearest {
    /// This one, its distance certainly below every other's.
    Unique(usize),
    /// One of these: their enclosures still met at the precision limit (equal distances,
    /// as for a query on a symmetry plane, or too close to tell apart).
    Tied(Vec<usize>),
}

/// Every stationary locus and the nearest.
#[derive(Clone, Debug, PartialEq)]
pub struct Projection<P> {
    /// The loci, in no promised order.
    pub loci: Vec<Locus<P>>,
    /// The nearest.
    pub nearest: Nearest,
}

impl<P> Projection<P> {
    /// The nearest locus, or the first candidate of a tie.
    pub fn nearest_locus(&self) -> Option<&Locus<P>> {
        let i = match &self.nearest {
            Nearest::Unique(i) => *i,
            Nearest::Tied(v) => *v.first()?,
        };
        self.loci.get(i)
    }

    /// The isolated points.
    pub fn points(&self) -> impl Iterator<Item = &Stationary<P>> {
        self.loci.iter().filter_map(Locus::point)
    }
}

/// A surface parameter `(u, v)`.
pub type Uv = (Enclosed, Enclosed);

/// Which parts of a cone count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Nappes {
    /// `r + v tan a >= 0`: one nappe, ending at the apex (STEP's and the pin's cone).
    One,
    /// Every `v`: the double cone the parameterisation draws.
    Both,
}

// ---------------------------------------------------------------- exact vectors

type R3 = [Rat; 3];

fn r3(v: [f64; 3]) -> Option<R3> {
    Some([
        Rat::from_f64(v[0])?,
        Rat::from_f64(v[1])?,
        Rat::from_f64(v[2])?,
    ])
}

fn rdot(a: &R3, b: &R3) -> Rat {
    a.iter().zip(b).fold(Rat::zero(), |s, (x, y)| s + x * y)
}

fn rsub(a: &R3, b: &R3) -> R3 {
    [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
}

fn radd(a: &R3, b: &R3) -> R3 {
    [&a[0] + &b[0], &a[1] + &b[1], &a[2] + &b[2]]
}

fn rscale(a: &R3, k: &Rat) -> R3 {
    [&a[0] * k, &a[1] * k, &a[2] * k]
}

fn rcross(a: &R3, b: &R3) -> R3 {
    [
        &a[1] * &b[2] - &a[2] * &b[1],
        &a[2] * &b[0] - &a[0] * &b[2],
        &a[0] * &b[1] - &a[1] * &b[0],
    ]
}

/// `|x| + |y| + |z|`, at least the Euclidean length.
fn l1(a: &R3) -> Rat {
    a.iter().fold(Rat::zero(), |s, x| s + x.abs())
}

fn to_v3(a: &R3) -> V3 {
    [rat_to_dd(&a[0]), rat_to_dd(&a[1]), rat_to_dd(&a[2])]
}

/// A bound on the rounding of `to_v3(a)`: `2^-100` of its largest component, times three.
fn rounding(a: &R3) -> f64 {
    let big = a.iter().map(Rat::abs).max().unwrap_or_default();
    up(&big) * dd::pow2(-98)
}

/// `p + sum c_i v_i` at the coefficients' midpoints, and a radius reaching the value at
/// any coefficients inside their enclosures.
fn axpy(p: &R3, terms: &[(&Bound, &R3)]) -> (V3, f64) {
    let mut at = p.clone();
    let mut radius = 0.0;
    for (c, v) in terms {
        at = radd(&at, &rscale(v, &c.mid()));
        radius += c.radius() * up(&l1(v));
    }
    (to_v3(&at), radius * (1.0 + dd::pow2(-40)) + rounding(&at))
}

// ---------------------------------------------------------------- shared steps

/// The nearest by enclosed distance: `Unique` when one is certainly below the rest.
fn pick<'a>(ds: impl Iterator<Item = &'a Bound> + Clone) -> Nearest {
    let Some(top) = ds.clone().map(|d| d.hi().clone()).min() else {
        return Nearest::Tied(Vec::new());
    };
    let near: Vec<usize> = ds
        .enumerate()
        .filter(|(_, d)| d.lo() <= &top)
        .map(|(i, _)| i)
        .collect();
    match near.as_slice() {
        [i] => Nearest::Unique(*i),
        _ => Nearest::Tied(near),
    }
}

/// Runs `build` at each precision until it reports every kind decided and the nearest is
/// unique, or the last precision is reached.
fn settle<P>(
    levels: &[u32],
    mut build: impl FnMut(u32) -> Option<(Vec<Locus<P>>, bool)>,
) -> Option<Projection<P>> {
    let mut out = None;
    for &bits in levels {
        let (loci, decided) = build(bits)?;
        let nearest = pick(loci.iter().map(Locus::distance));
        let done = decided && matches!(nearest, Nearest::Unique(_));
        out = Some(Projection { loci, nearest });
        if done {
            break;
        }
    }
    out
}

/// The kind of a stationary point of a function of one variable from the signs of its
/// derivative just before and just after.
fn kind_of_signs(before: i32, after: i32) -> Kind {
    match (before, after) {
        (-1, 1) => Kind::Min,
        (1, -1) => Kind::Max,
        _ => Kind::Flat,
    }
}

/// The kind of a stationary point of a surface of revolution from its kinds round the axis
/// (from `w_sign`, the sign of its signed radius: `+` a minimum, `-` a maximum, `0` flat)
/// and along the meridian; `None` while `w`'s sign is not decided.
fn kind_of_surface(w_sign: Option<i32>, along: Kind) -> Option<Kind> {
    let around = match w_sign? {
        1 => Kind::Min,
        -1 => Kind::Max,
        _ => Kind::Flat,
    };
    Some(match (around, along) {
        (Kind::Min, Kind::Min) => Kind::Min,
        (Kind::Max, Kind::Max) => Kind::Max,
        (Kind::Min, Kind::Max) | (Kind::Max, Kind::Min) => Kind::Saddle,
        _ => Kind::Flat,
    })
}

/// `x` in `(-pi, pi]`.
fn wrap(x: Dd) -> Dd {
    let pi = dd::consts().pi;
    if x.hi > pi.hi {
        x - pi * 2.0
    } else if x.hi <= -pi.hi {
        x + pi * 2.0
    } else {
        x
    }
}

/// The double-double functions' error bound on an angle.
const ANGLE_ERR: f64 = 1.0e-29;

/// An angle at a midpoint, with radius.
fn angle(mid: Dd, radius: f64) -> Enclosed {
    Enclosed {
        mid,
        radius: radius + ANGLE_ERR,
    }
}

// ---------------------------------------------------------------- T1: the ellipse

/// The ellipse quartics for one orientation: `N(s) = |V(s)|^2` with
/// `V(s) = (d - A) s^2 + 2 B s + (d + A)`, `d = C - q`, and `Q = N' (1 + s^2) - 4 s N`,
/// whose sign is that of the distance's derivative.
struct Quartic {
    n: [Rat; 5],
    q: Poly,
}

impl Quartic {
    fn new(d: &R3, a: &R3, b: &R3) -> Quartic {
        let (v2, v1, v0) = (rsub(d, a), rscale(b, &Rat::from(2)), radd(d, a));
        let two = Rat::from(2);
        let n4 = rdot(&v2, &v2);
        let n3 = &two * &rdot(&v2, &v1);
        let n2 = rdot(&v1, &v1) + &two * &rdot(&v2, &v0);
        let n1 = &two * &rdot(&v1, &v0);
        let n0 = rdot(&v0, &v0);
        let k = |x: i64| Rat::from(x);
        let q = Poly::new(vec![
            n1.clone(),
            &k(2) * &n2 - &k(4) * &n0,
            &k(3) * &n3 - &k(3) * &n1,
            &k(4) * &n4 - &k(2) * &n2,
            -n3.clone(),
        ]);
        Quartic {
            n: [n0, n1, n2, n3, n4],
            q,
        }
    }

    /// The squared distance at `s`, exactly: `N(s) / (1 + s^2)^2`.
    fn g(&self, s: &Rat) -> Rat {
        let num = self
            .n
            .iter()
            .rev()
            .fold(Rat::zero(), |acc, c| &(&acc * s) + c);
        let den = Rat::one() + s * s;
        (num.checked_div(&(&den * &den))).unwrap_or_default()
    }
}

/// The kind of the exact root `m` of `q`: the sign of the first derivative not zero there.
fn kind_at_exact(q: &Poly, m: &Rat) -> Kind {
    let mut p = q.clone();
    for k in 1..=4 {
        p = p.derivative();
        let s = p.sign_at(m);
        if s != 0 {
            let before = if k % 2 == 0 { s } else { -s };
            return kind_of_signs(before, s);
        }
    }
    Kind::Flat
}

/// The kind at `s = 0` of `q`, from its lowest coefficient not zero.
fn kind_at_zero(q: &Poly) -> Kind {
    q.coeffs()
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, c)| !c.is_zero())
        .map_or(Kind::Flat, |(k, c)| {
            let s = c.signum();
            let before = if k % 2 == 0 { s } else { -s };
            kind_of_signs(before, s)
        })
}

impl Ellipse {
    /// Every stationary point of the distance from `q`, certified, and the nearest (T1).
    /// `None` for a non-finite value, a semi-axis not above zero, or parallel axes.
    pub fn stationary(&self, q: [f64; 3]) -> Option<Projection<Enclosed>> {
        let f = &self.frame;
        let (o, x, y, q) = (r3(f.origin)?, r3(f.x)?, r3(f.y)?, r3(q)?);
        let (a, b) = (Rat::from_f64(self.semi_x)?, Rat::from_f64(self.semi_y)?);
        let normal = rcross(&x, &y);
        if a.signum() <= 0 || b.signum() <= 0 || normal.iter().all(Rat::is_zero) {
            return None;
        }
        let (av, bv) = (rscale(&x, &a), rscale(&y, &b));
        let d = rsub(&o, &q);
        let fwd = Quartic::new(&d, &av, &bv);
        let point_at = |s: &Rat| {
            // cos t = (1 - s^2) / (1 + s^2), sin t = 2 s / (1 + s^2).
            let den = (Rat::one() + s * s).recip().unwrap_or_default();
            let c = &(Rat::one() - s * s) * &den;
            let sn = &(Rat::from(2) * s) * &den;
            radd(&o, &radd(&rscale(&av, &c), &rscale(&bv, &sn)))
        };
        if fwd.q.is_zero() {
            // A circle with `q` on its axis: every point at the same distance.
            let circle = StationaryCircle {
                centre: to_v3(&o),
                normal: to_v3(&normal),
                radius: Bound::exact(rdot(&av, &av)).sqrt(160)?,
                distance: Bound::exact(fwd.g(&Rat::zero())).sqrt(160)?,
                kind: Kind::Flat,
            };
            return Some(Projection {
                loci: vec![Locus::Circle(circle)],
                nearest: Nearest::Unique(0),
            });
        }
        let back = Quartic::new(
            &d,
            &rscale(&av, &Rat::from(-1)),
            &rscale(&bv, &Rat::from(-1)),
        );
        // |f''(t)| <= 2 (K^2 + (|d| + K) K), K = |A| + |B| bounding |P'| and |P''|.
        let k = l1(&av) + l1(&bv);
        let m = Rat::from(2) * (&k * &k + &(l1(&d) + k.clone()) * &k);
        settle(&[110, 220, 440], |bits| {
            let width = Rat::one().scale(-i64::from(bits));
            let mut loci = Vec::new();
            for root in fwd.q.real_roots(&width) {
                let s = root.at.mid();
                let w = root.at.width();
                let kind = if root.at.is_exact() {
                    kind_at_exact(&fwd.q, &s)
                } else {
                    kind_of_signs(fwd.q.sign_at(root.at.lo()), fwd.q.sign_at(root.at.hi()))
                };
                // |s - s*| <= w/2, so |t - t*| <= w; f'(t*) = 0, so the squared distance
                // at s is within M w^2 / 2 of the true one.
                let g = fwd.g(&s);
                let e = (&m * &(&w * &w)).scale(-1);
                let sq = Bound::new(&g - &e, &g + &e)?;
                let t = dd::atan(rat_to_dd(&s)) * 2.0;
                let p = point_at(&s);
                loci.push(Locus::Point(Stationary {
                    param: angle(t, up(&w)),
                    point: to_v3(&p),
                    point_radius: up(&(&k * &w)) + rounding(&p),
                    distance: sq.sqrt(bits + 20)?,
                    kind,
                }));
            }
            // t = pi: s' = 0 for the ellipse turned over.
            if back.q.coeffs().first().is_none_or(Rat::is_zero) {
                let p = rsub(&o, &av);
                loci.push(Locus::Point(Stationary {
                    param: angle(dd::consts().pi, 0.0),
                    point: to_v3(&p),
                    point_radius: rounding(&p),
                    distance: Bound::exact(back.g(&Rat::zero())).sqrt(bits + 20)?,
                    kind: kind_at_zero(&back.q),
                }));
            }
            Some((loci, true))
        })
    }
}

// ---------------------------------------------------------------- T3: the meridian plane

/// The query in a surface of revolution's frame, exactly where it can be.
struct Meridian {
    o: R3,
    z: R3,
    /// `d = q - O`.
    d: R3,
    /// `d` less its component along `z`: the radial direction, unnormalised.
    rv: R3,
    /// `z . z`.
    zz: Rat,
    /// `d . z`.
    dz: Rat,
    /// The squared height `(d . z)^2 / z . z`.
    h2: Rat,
    /// The squared distance from the axis.
    rho2: Rat,
    /// The angle of `d` round the axis (zero on the axis).
    u: Dd,
}

impl Meridian {
    fn new(o: [f64; 3], x: [f64; 3], y: [f64; 3], z: [f64; 3], q: [f64; 3]) -> Option<Meridian> {
        let (o, x, y, z, q) = (r3(o)?, r3(x)?, r3(y)?, r3(z)?, r3(q)?);
        let zz = rdot(&z, &z);
        if zz.is_zero() {
            return None;
        }
        let d = rsub(&q, &o);
        let dz = rdot(&d, &z);
        let along = dz.checked_div(&zz)?;
        let rv = rsub(&d, &rscale(&z, &along));
        let h2 = &dz * &along;
        let rho2 = rdot(&rv, &rv);
        let u = if rho2.is_zero() {
            Dd::new(0.0)
        } else {
            // From the radial part, exactly square to z: d's height would leak into the
            // angle through an x or y not quite square to z.
            dd::atan2(rat_to_dd(&rdot(&rv, &y)), rat_to_dd(&rdot(&rv, &x)))
        };
        Some(Meridian {
            o,
            z,
            d,
            rv,
            zz,
            dz,
            h2,
            rho2,
            u,
        })
    }

    /// `|z|`.
    fn zlen(&self, bits: u32) -> Option<Bound> {
        Bound::exact(self.zz.clone()).sqrt(bits)
    }

    /// The height along the unit axis.
    fn h(&self, bits: u32) -> Option<Bound> {
        Bound::exact(self.dz.clone()).div(&self.zlen(bits)?, bits)
    }

    /// The distance from the axis.
    fn rho(&self, bits: u32) -> Option<Bound> {
        Bound::exact(self.rho2.clone()).sqrt(bits)
    }

    /// The point `O + w e + h z_unit` for a signed radius `w` and a height `h` in the
    /// meridian plane (`e` the unit radial direction, off the axis only).
    fn point(&self, w: &Bound, h: &Bound, bits: u32) -> Option<(V3, f64)> {
        let beta = h.div(&self.zlen(bits)?, bits)?;
        if self.rho2.is_zero() {
            return Some(axpy(&self.o, &[(&beta, &self.z)]));
        }
        let alpha = w.div(&self.rho(bits)?, bits)?;
        Some(axpy(&self.o, &[(&alpha, &self.rv), (&beta, &self.z)]))
    }

    /// The angle round the axis of the meridian half-plane on the side `side` (+1 the
    /// query's, -1 the opposite).
    fn u_on(&self, side: i32) -> Dd {
        if side > 0 {
            self.u
        } else {
            wrap(self.u + dd::consts().pi)
        }
    }
}

fn rat(x: f64) -> Option<Rat> {
    Rat::from_f64(x)
}

fn b(x: Rat) -> Bound {
    Bound::exact(x)
}

// ---------------------------------------------------------------- T3: the torus

/// Whether the stationary point of the meridian circle centred at signed radius `c` on
/// branch `sigma` (+1 near, -1 far) lies exactly on the axis: `c + sigma r (rho - c) / D`
/// is zero, `D^2 = (rho - c)^2 + h^2 > 0`. Decided exactly, `rho` known by its square.
fn on_axis_exactly(m: &Meridian, c: &Rat, r: &Rat, sigma: i32) -> bool {
    let c2 = c * c;
    let k = &(r * r) - &c2;
    // sigma r (rho - c) = -c D: squared, (r^2 - c^2)(rho - c)^2 = c^2 h^2, and the sign
    // of sigma (rho - c) must be the opposite of c's.
    let rho_minus_c_sign = if c.signum() > 0 {
        (&m.rho2 - &c2).signum()
    } else {
        1
    };
    if k.is_zero() {
        return m.h2.is_zero() && sigma * rho_minus_c_sign == -c.signum();
    }
    let Some(t) = (&c2 * &m.h2).checked_div(&k) else {
        return false;
    };
    if t.signum() <= 0 {
        return false; // (rho - c)^2 = 0 would put q on the tube's centre circle
    }
    // rho = c + e sqrt(t): rho^2 - c^2 - t = 2 c e sqrt(t).
    let e = &(&m.rho2 - &c2) - &t;
    if &e * &e != &(&Rat::from(4) * &c2) * &t {
        return false;
    }
    let eps = e.signum() * c.signum();
    eps == rho_minus_c_sign && sigma * eps == -c.signum()
}

impl Torus {
    /// Every stationary point of the distance from `q`, certified, and the nearest (T3):
    /// four in the query's meridian plane (two on each meridian circle), circles of them
    /// for a query on the axis, the meridian circle itself for a query on the tube's centre
    /// circle. Ring, horn (`R = r`) and spindle (`R < r`) tori alike. `None` for a
    /// non-finite value, `R` or `r` not above zero, or a zero axis.
    pub fn stationary(&self, q: [f64; 3]) -> Option<Projection<Uv>> {
        let f = &self.frame;
        let m = Meridian::new(f.origin, f.x, f.y, f.z, q)?;
        let (big, small) = (rat(self.major)?, rat(self.minor)?);
        if big.signum() <= 0 || small.signum() <= 0 {
            return None;
        }
        settle(&[192, 384, 768], |bits| torus_loci(&m, &big, &small, bits))
    }
}

fn torus_loci(m: &Meridian, big: &Rat, small: &Rat, bits: u32) -> Option<(Vec<Locus<Uv>>, bool)> {
    let r = b(small.clone());
    let h = m.h(bits)?;
    let mut loci = pinches(m, big, small, &h, bits)?;
    let mut decided = true;
    if m.rho2.is_zero() {
        // On the axis: every meridian plane is the query's; the feet sweep circles.
        let dsq = &m.h2 + &(big * big);
        let dist = b(dsq.clone()).sqrt(bits)?;
        for sigma in [1, -1] {
            let sr = r.mul_rat(&Rat::from(i64::from(sigma)));
            let w = b(big.clone()).sub(&b(big.clone()).mul(&sr).div(&dist, bits)?);
            let hp = sr.mul(&h).div(&dist, bits)?;
            let distance = if sigma > 0 {
                dist.sub(&r).abs()
            } else {
                dist.add(&r)
            };
            if sigma > 0 && dsq == small * small {
                // D = r: the near foot is the query itself, a pinch, listed already.
                continue;
            }
            let (centre, _) = m.point(&b(Rat::zero()), &hp, bits)?;
            loci.push(Locus::Circle(StationaryCircle {
                centre,
                normal: to_v3(&m.z),
                radius: w.abs(),
                distance,
                kind: if sigma > 0 { Kind::Min } else { Kind::Max },
            }));
        }
        return Some((loci, decided));
    }
    let rho = m.rho(bits)?;
    for side in [1, -1] {
        let c = Rat::from(i64::from(side)) * big;
        if side > 0 && m.rho2 == big * big && m.h2.is_zero() {
            // On the tube's centre circle: the whole meridian circle is at distance r.
            let normal = to_v3(&rcross(&m.rv, &m.z));
            loci.push(Locus::Circle(StationaryCircle {
                centre: to_v3(&radd(&m.o, &m.d)),
                normal,
                radius: r.clone(),
                distance: r.clone(),
                kind: if big > small { Kind::Min } else { Kind::Flat },
            }));
            continue;
        }
        let off = rho.sub(&b(c.clone()));
        let dist = off.sqr().add(&b(m.h2.clone())).sqrt(bits)?;
        for sigma in [1, -1] {
            let sr = r.mul_rat(&Rat::from(i64::from(sigma)));
            let exact_axis = on_axis_exactly(m, &c, small, sigma);
            let w = if exact_axis {
                b(Rat::zero())
            } else {
                b(c.clone()).add(&sr.mul(&off).div(&dist, bits)?)
            };
            let hp = sr.mul(&h).div(&dist, bits)?;
            if exact_axis {
                // A foot on the axis is a pinch, listed already.
                continue;
            }
            let distance = if sigma > 0 {
                dist.sub(&r).abs()
            } else {
                dist.add(&r)
            };
            let along = if sigma > 0 { Kind::Min } else { Kind::Max };
            let kind = kind_of_surface(w.signum(), along);
            decided &= kind.is_some(); // listed Flat until decided
            let kind = kind.unwrap_or(Kind::Flat);
            let (point, point_radius) = m.point(&w, &hp, bits)?;
            // v from R + r cos v = side w, r sin v = hp, on the half-plane `side`.
            let cos_part = w.mul_rat(&Rat::from(i64::from(side))).sub(&b(big.clone()));
            let v = dd::atan2(hp.to_dd(), cos_part.to_dd());
            let v_radius = (hp.radius() + cos_part.radius()) / bound::down(small) * 2.0;
            loci.push(Locus::Point(Stationary {
                param: (angle(m.u_on(side), 0.0), angle(v, v_radius)),
                point,
                point_radius,
                distance,
                kind,
            }));
        }
    }
    Some((loci, decided))
}

/// The pinches of a horn or spindle torus (`R <= r`), where the surface meets its axis at
/// `h = +-sqrt(r^2 - R^2)`: singular points, listed as [`Kind::Apex`] whatever the query
/// (as the cone's apex is). None for a ring torus; one, at the centre, for a horn.
fn pinches(m: &Meridian, big: &Rat, small: &Rat, h: &Bound, bits: u32) -> Option<Vec<Locus<Uv>>> {
    let k = &(small * small) - &(big * big);
    if k.signum() < 0 {
        return Some(Vec::new());
    }
    let half = b(k.clone()).sqrt(bits)?;
    let heights = if k.is_zero() {
        vec![half]
    } else {
        vec![half.clone(), half.neg()]
    };
    heights
        .into_iter()
        .map(|hp| {
            let (point, point_radius) = m.point(&b(Rat::zero()), &hp, bits)?;
            let distance = b(m.rho2.clone()).add(&h.sub(&hp).sqr()).sqrt(bits)?;
            // Any u; v from R + r cos v = 0, r sin v = hp.
            let v = dd::atan2(hp.to_dd(), Dd::new(0.0) - Dd::new(big.to_f64()));
            let v_radius = hp.radius() / bound::down(small) * 2.0;
            Some(Locus::Point(Stationary {
                param: (angle(m.u, core::f64::consts::PI), angle(v, v_radius)),
                point,
                point_radius,
                distance,
                kind: Kind::Apex,
            }))
        })
        .collect()
}

// ---------------------------------------------------------------- T3: the cone

impl Cone {
    /// Every stationary point of the distance from `q`, certified, and the nearest (T3):
    /// the feet on the two generators in the query's meridian plane that `nappes` admits,
    /// a circle of them for a query on the axis, and the apex. `None` for a non-finite
    /// value, a negative radius, a semi-angle outside `(0, pi/2)`, or a zero axis.
    pub fn stationary(&self, q: [f64; 3], nappes: Nappes) -> Option<Projection<Uv>> {
        let f = &self.frame;
        let m = Meridian::new(f.origin, f.x, f.y, f.z, q)?;
        let r = rat(self.radius)?;
        if r.signum() < 0 {
            return None;
        }
        bound::tan(self.semi_angle, 64)?;
        if r.is_zero() && m.d.iter().all(Rat::is_zero) {
            // The query is the apex.
            let (point, point_radius) = (to_v3(&m.o), rounding(&m.o));
            return Some(Projection {
                loci: vec![Locus::Point(Stationary {
                    param: (
                        angle(Dd::new(0.0), core::f64::consts::PI),
                        angle(Dd::new(0.0), 0.0),
                    ),
                    point,
                    point_radius,
                    distance: b(Rat::zero()),
                    kind: Kind::Apex,
                })],
                nearest: Nearest::Unique(0),
            });
        }
        settle(&[192, 384, 768], |bits| {
            cone_loci(&m, &r, self.semi_angle, nappes, bits)
        })
    }
}

fn cone_loci(
    m: &Meridian,
    r: &Rat,
    semi_angle: f64,
    nappes: Nappes,
    bits: u32,
) -> Option<(Vec<Locus<Uv>>, bool)> {
    let t = bound::tan(semi_angle, bits)?;
    let rb = b(r.clone());
    let one_t2 = b(Rat::one()).add(&t.sqr());
    let slant = one_t2.sqrt(bits)?;
    let h = m.h(bits)?;
    let mut loci = Vec::new();
    let mut decided = true;
    // The apex, at height -r / t.
    let apex_h = rb.neg().div(&t, bits)?;
    let (point, point_radius) = m.point(&b(Rat::zero()), &apex_h, bits)?;
    let apex_d = b(m.rho2.clone()).add(&h.sub(&apex_h).sqr()).sqrt(bits)?;
    loci.push(Locus::Point(Stationary {
        param: (
            angle(Dd::new(0.0), core::f64::consts::PI),
            angle(apex_h.to_dd(), apex_h.radius()),
        ),
        point,
        point_radius,
        distance: apex_d,
        kind: Kind::Apex,
    }));
    if m.rho2.is_zero() {
        // On the axis: the feet sweep one circle, radius (r + h t) / (1 + t^2).
        let v = h.sub(&rb.mul(&t)).div(&one_t2, bits)?;
        let w = rb.add(&v.mul(&t));
        let sign = w.signum();
        decided &= sign.is_some();
        let keep = nappes == Nappes::Both || sign.is_none_or(|s| s > 0);
        if keep {
            let (centre, _) = m.point(&b(Rat::zero()), &v, bits)?;
            loci.push(Locus::Circle(StationaryCircle {
                centre,
                normal: to_v3(&m.z),
                radius: w.abs(),
                distance: rb.add(&h.mul(&t)).abs().div(&slant, bits)?,
                kind: Kind::Min,
            }));
        }
        return Some((loci, decided));
    }
    let rho = m.rho(bits)?;
    for side in [1, -1] {
        // side +1: (r + v t, v); side -1: (-(r + v t), v), in (radius, height).
        let s = Rat::from(i64::from(side));
        let v = rho
            .mul_rat(&s)
            .sub(&rb)
            .mul(&t)
            .add(&h)
            .div(&one_t2, bits)?;
        let radial = rb.add(&v.mul(&t)); // r + v t
        let w = radial.mul_rat(&s);
        let sign = radial.signum();
        if nappes == Nappes::One {
            decided &= sign.is_some();
            if sign.is_some_and(|x| x < 0) {
                continue; // beyond the apex
            }
        }
        let distance = rho
            .mul_rat(&s)
            .sub(&rb)
            .sub(&h.mul(&t))
            .abs()
            .div(&slant, bits)?;
        let kind = kind_of_surface(w.signum(), Kind::Min);
        decided &= kind.is_some(); // listed Flat until decided
        let kind = kind.unwrap_or(Kind::Flat);
        let (point, point_radius) = m.point(&w, &v, bits)?;
        loci.push(Locus::Point(Stationary {
            param: (angle(m.u_on(side), 0.0), angle(v.to_dd(), v.radius())),
            point,
            point_radius,
            distance,
            kind,
        }));
    }
    Some((loci, decided))
}
