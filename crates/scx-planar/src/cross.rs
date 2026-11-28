// Written for solidcorex. Apache-2.0.
//! Every crossing of two bounded plane curves, lines and conic arcs, with its
//! multiplicity: the image-space crossings a drawing needs (research 74 §5, HL2) and §31.
//!
//! [`crossings`] finds every place where two curves meet on their bounded ranges, or
//! returns [`CrossError::Undecided`]: it never misses one silently. The method is one
//! certified isolation, not a solver with a tolerance:
//!
//! - one curve is written implicitly (a line by its orientation, a conic arc by its
//!   barycentric equation `tau1^2 = 4 w^2 tau0 tau2` over its control triangle) and
//!   composed with the other's rational Bernstein form, so the crossings are the roots of
//!   one polynomial `F` on `[0, 1]` of degree at most 4, and the implicit curve's bounded
//!   range is the polynomials' signs (where on the line, which side of the triangle);
//! - the coefficients are intervals rounded outward (an exact sum or product of exact
//!   values stays exact, so a dyadic zero stays a zero), and every coefficient that is a
//!   single orientation or dot product carries its exact sign (the exact predicates of
//!   [`crate::cert`]): a joint the two curves share, or a point exactly on the other
//!   curve, is an exact zero, never a rounding accident;
//! - a range is dropped when `F` is certified one-signed or the range is certified off the
//!   implicit curve's bounded range; it holds exactly one simple crossing when its
//!   coefficients' signs vary once (Descartes' rule on the Bernstein form), plus the exact
//!   zeros at its ends with their exact multiplicity; otherwise it is halved;
//! - a range whose image is within the resolution is a cluster: the crossings in it are
//!   one meet, of multiplicity bounded by the least derivative of `F` certified one-signed
//!   (Rolle) and of the parity of `F`'s signs at its ends;
//! - a range on which no derivative is certified and every point is proved within the
//!   resolution of the implicit curve is a shared stretch, an [`Overlap`].
//!
//! Rational Bézier curves of degree 3 and more, and splines, wait on step 5's planar
//! widening: [`CrossError::Pending`].
//!
//! | solidcore at the pin | here |
//! |---|---|
//! | `segment_2d_intersection`: the first hit of two segments, within a tolerance | every meet and overlap of two curves, certified |
//! | `prism::profile::crossing`: a line missing a circle (`disc < 0`), or two circles missing (`h2 < 0`), by any distance is a touch, filtered later by a tolerance | a miss is certified and dropped; a touch has multiplicity 2 |
//! | `prism::profile::crossing::line_line`: parallel when the cross product is within `1e-14` of the lengths' product | parallel only when exactly so; a near-parallel crossing is found |
//! | `on_arc`: inside an arc's sweep by `atan2`, with slack | inside the control triangle, by exact orientations |
//! | `segment_segment_intersect`: ends on the other within a tolerance, then one closest point | the same meets, exact at ends, plus the overlap's two ends as one [`Overlap`] |
//! | `hlr::crossings`: chords sampled on a grid, tolerance `1e-9` times the scale | the exact curves, no sampling |
//! | `curve_crossings`: `global_curve_intersect` with a tolerance | certified isolation, no tolerance but the resolution |

use core::fmt;

use scx_algo::bernstein::binomial;
use scx_num::budget::{Budget, Exhausted, Stop, Stopped};
use scx_num::error::ErrorKind;
use scx_num::interval::{Ivl, cmax, cmin};
use scx_num::units::Resolution;
use scx_num::vector2d::Point2;

use crate::cert::{IP, Iv, dot_sign, exact_product, exact_sum, orient};

/// A bounded plane curve, parametrised by `t` in `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Curve2 {
    /// The segment from `start` (`t = 0`) to `end` (`t = 1`).
    Line {
        /// Where it starts.
        start: Point2,
        /// Where it ends.
        end: Point2,
    },
    /// A conic arc as a rational quadratic Bézier: `start` (`t = 0`) and `end` (`t = 1`)
    /// of weight 1, `apex` (where the end tangents meet) of weight `weight > 0`: an
    /// ellipse arc below 1 (a circular arc of half-angle `h` has weight `cos h`), a
    /// parabola at 1, a hyperbola above.
    Conic {
        /// Where it starts.
        start: Point2,
        /// The control point between: the end tangents' meeting point.
        apex: Point2,
        /// Where it ends.
        end: Point2,
        /// The apex's weight.
        weight: f64,
    },
    /// A rational Bézier curve of any degree, by its control points and their weights.
    /// Pending: crossings of these wait on step 5's planar widening, and are
    /// [`CrossError::Pending`].
    Bezier {
        /// The control points.
        points: Vec<Point2>,
        /// Their weights.
        weights: Vec<f64>,
    },
}

/// Where two curves meet: one crossing, or a cluster of crossings closer than the
/// resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meet {
    /// The parameters of the first curve that hold the meet, low to high; equal when it is
    /// exactly known (a shared end, an exact zero).
    pub a: [f64; 2],
    /// The parameters of the second curve that hold it, low to high.
    pub b: [f64; 2],
    /// A point of the first curve in the meet.
    pub at: Point2,
    /// How many crossings the meet holds, counted with multiplicity: 1 for a simple
    /// crossing, 2 for a tangency, 4 for an osculating circle at an ellipse's vertex.
    pub multiplicity: u32,
    /// Whether [`Meet::multiplicity`] is the exact count. It is not for a cluster
    /// narrower than the resolution whose count only a bound and a parity settle: there it
    /// is the largest count they allow (the curves come within the resolution; an exact
    /// tangency holds it exactly, a near miss closer than the resolution none).
    pub exact: bool,
}

impl Meet {
    /// Whether the first curve passes from one side of the second to the other here:
    /// an odd multiplicity.
    pub const fn crosses(&self) -> bool {
        self.multiplicity % 2 == 1
    }
}

/// A stretch the two curves share: every point of it is within the resolution of both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlap {
    /// The first curve's parameters at the stretch's two ends, low to high.
    pub a: [f64; 2],
    /// The second curve's parameters at the same two ends: high to low when the curves run
    /// opposite ways.
    pub b: [f64; 2],
}

/// Every meet and shared stretch of two curves, in the order of the first curve's
/// parameter.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Crossings {
    /// The meets.
    pub meets: Vec<Meet>,
    /// The shared stretches.
    pub overlaps: Vec<Overlap>,
}

/// One of the two curves of a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Which {
    /// The first.
    A,
    /// The second.
    B,
}

/// Crossings of what wait on step 5's planar widening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pending {
    /// A rational Bézier curve ([`Curve2::Bezier`]), of this degree.
    Bezier {
        /// Its degree.
        degree: usize,
    },
    /// Two conic arcs whose control points are each exactly collinear: neither has the
    /// implicit form the method needs.
    FlatPair,
}

/// Why the crossings of two curves cannot be given.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum CrossError {
    /// A coordinate or a weight is infinite or NaN.
    NotFinite {
        /// The curve.
        curve: Which,
    },
    /// A coordinate or a weight is outside `[1e-100, 1e9]` in magnitude (and not a
    /// coordinate 0), where the exact predicates would lose exactness.
    OutOfRange {
        /// The curve.
        curve: Which,
    },
    /// A line whose ends are one point, a weight not above 0, or a Bézier curve whose
    /// points and weights differ in number.
    Degenerate {
        /// The curve.
        curve: Which,
    },
    /// The pair waits on step 5's planar widening.
    Pending(Pending),
    /// The certified arithmetic cannot settle what the curves do on this range of one
    /// curve's parameter: crossings closer than the resolution that neither a bound nor a
    /// parity settles, or a resolution too fine for the depth of halving.
    Undecided {
        /// The curve whose parameter the range is of.
        curve: Which,
        /// The range.
        range: [f64; 2],
    },
    /// The budget ran out.
    Exhausted(Exhausted),
    /// The caller stopped the call.
    Stopped(Stopped),
}

impl CrossError {
    /// The kind (research 62 §10): the caller's input is `Refused`; what waits on step 5,
    /// or what the arithmetic cannot settle, is `Unsupported`.
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFinite { .. } | Self::OutOfRange { .. } | Self::Degenerate { .. } => {
                ErrorKind::Refused
            }
            Self::Pending(_) | Self::Undecided { .. } => ErrorKind::Unsupported,
            Self::Exhausted(e) => e.kind(),
            Self::Stopped(e) => e.kind(),
        }
    }

    /// The cause's stable name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NotFinite { .. } => "not_finite",
            Self::OutOfRange { .. } => "out_of_range",
            Self::Degenerate { .. } => "degenerate",
            Self::Pending(_) => "pending",
            Self::Undecided { .. } => "undecided",
            Self::Exhausted(e) => e.name(),
            Self::Stopped(e) => e.name(),
        }
    }
}

impl fmt::Display for Which {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::A => "the first curve",
            Self::B => "the second curve",
        })
    }
}

impl fmt::Display for CrossError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite { curve } => write!(f, "{curve} has a coordinate or weight not finite"),
            Self::OutOfRange { curve } => {
                write!(
                    f,
                    "{curve} has a coordinate or weight outside [1e-100, 1e9]"
                )
            }
            Self::Degenerate { curve } => write!(f, "{curve} is degenerate"),
            Self::Pending(Pending::Bezier { degree }) => write!(
                f,
                "crossings of a degree {degree} Bezier curve wait on the planar widening"
            ),
            Self::Pending(Pending::FlatPair) => {
                f.write_str("crossings of two flat conic arcs wait on the planar widening")
            }
            Self::Undecided { curve, range } => write!(
                f,
                "the crossings on {curve} over [{}, {}] cannot be settled",
                range[0], range[1]
            ),
            Self::Exhausted(e) => e.fmt(f),
            Self::Stopped(e) => e.fmt(f),
        }
    }
}

impl core::error::Error for CrossError {}

impl From<Exhausted> for CrossError {
    fn from(e: Exhausted) -> Self {
        Self::Exhausted(e)
    }
}

impl From<Stopped> for CrossError {
    fn from(e: Stopped) -> Self {
        Self::Stopped(e)
    }
}

/// Every meet and shared stretch of `a` and `b` on their bounded ranges, or
/// [`CrossError::Undecided`]: none is missed.
///
/// Two curves meeting more than the resolution `res` apart are separate meets; crossings
/// closer than it may be one [`Meet`] of their total multiplicity (two crossings an
/// isolation separates are reported apart even when closer). A stretch is an [`Overlap`]
/// when the curves are equal there to within rounding; two curves merely closer than the
/// resolution without meeting meet nowhere. A meet within the resolution of the second
/// curve's end, which the arithmetic cannot place inside or outside it, is reported, its
/// parameter clamped to the range.
///
/// The call is symmetric: swapping `a` and `b` swaps every bracket and gives the same
/// meets. It charges `budget` one unit for each range it examines, and polls `stop` as
/// often.
///
/// # Errors
///
/// [`CrossError`]: the input refused, a Bézier or flat pair pending, a cluster undecided,
/// the budget exhausted or the call stopped.
pub fn crossings(
    a: &Curve2,
    b: &Curve2,
    res: Resolution,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Crossings, CrossError> {
    let ra = Rat::new(a, Which::A)?;
    let rb = Rat::new(b, Which::B)?;
    for r in [&ra, &rb] {
        if let Shape::Bezier(degree) = r.shape {
            return Err(CrossError::Pending(Pending::Bezier { degree }));
        }
    }
    // Which curve is implicit: a line before a conic, a conic that is not flat before one
    // that is, and between equals the smaller by its numbers, so (a, b) and (b, a) are the
    // same computation.
    let rank = |r: &Rat| match r.shape {
        Shape::Line => 0,
        Shape::Conic if !r.flat() => 1,
        _ => 2,
    };
    let (ka, kb) = (rank(&ra), rank(&rb));
    if ka == 2 && kb == 2 {
        return Err(CrossError::Pending(Pending::FlatPair));
    }
    let a_implicit = ka < kb || (ka == kb && ra.key() < rb.key());
    let (param, implicit, pw) = if a_implicit {
        (&rb, &ra, Which::B)
    } else {
        (&ra, &rb, Which::A)
    };
    // Scale by a power of two (exact) so the largest coordinate is near 1: the products
    // of the method then neither overflow nor underflow.
    let big = ra
        .points
        .iter()
        .chain(&rb.points)
        .fold(0.0_f64, |m, p| m.max(p.x.abs()).max(p.y.abs()));
    // floor(log2 big) + 1, from the exponent bits (exact; a subnormal counts as -1021).
    let biased = ((big.to_bits() >> 52) & 0x7ff) as i32;
    let e = match (big > 0.0, biased) {
        (false, _) => 0,
        (true, 0) => -1021,
        (true, b) => b - 1022,
    };
    let down = pow2(-e);
    let job = Job {
        param: param.scaled(down),
        implicit: Imp::new(&implicit.scaled(down)),
        res: (res.zone().mm() * down).min(8.0),
        param_curve: param,
        which: pw,
    };
    let (meets, overlaps) = job.run(budget, stop)?;
    let mut out = Crossings { meets, overlaps };
    if a_implicit {
        for m in &mut out.meets {
            core::mem::swap(&mut m.a, &mut m.b);
            m.at = ra.point_at(m.a);
        }
        for o in &mut out.overlaps {
            core::mem::swap(&mut o.a, &mut o.b);
            if o.a[0] > o.a[1] {
                o.a.swap(0, 1);
                o.b.swap(0, 1);
            }
        }
        out.meets
            .sort_by(|x, y| x.a[0].total_cmp(&y.a[0]).then(x.a[1].total_cmp(&y.a[1])));
        out.overlaps
            .sort_by(|x, y| x.a[0].total_cmp(&y.a[0]).then(x.a[1].total_cmp(&y.a[1])));
    }
    Ok(out)
}

/// `2^e`, exactly, for `e` in `[-1022, 1023]`.
fn pow2(e: i32) -> f64 {
    let biased = (1023 + e).clamp(1, 2046);
    f64::from_bits((biased as u64) << 52)
}

/// The deepest halving of a parameter range.
const MAX_DEPTH: u32 = 100;

/// Extra halvings past the resolution to place a crossing inside or outside the second
/// curve's range.
const EXTRA: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Line,
    Conic,
    Bezier(usize),
}

/// A curve as a rational Bézier: control points and weights.
#[derive(Debug, Clone)]
struct Rat {
    shape: Shape,
    points: Vec<Point2>,
    weights: Vec<f64>,
}

impl Rat {
    fn new(c: &Curve2, which: Which) -> Result<Self, CrossError> {
        let (shape, points, weights) = match c {
            Curve2::Line { start, end } => (Shape::Line, vec![*start, *end], vec![1.0, 1.0]),
            Curve2::Conic {
                start,
                apex,
                end,
                weight,
            } => (
                Shape::Conic,
                vec![*start, *apex, *end],
                vec![1.0, *weight, 1.0],
            ),
            Curve2::Bezier { points, weights } => (
                Shape::Bezier(points.len().saturating_sub(1)),
                points.clone(),
                weights.clone(),
            ),
        };
        let coords = points.iter().flat_map(|p| [p.x, p.y]);
        if coords
            .clone()
            .chain(weights.iter().copied())
            .any(|x| !x.is_finite())
        {
            return Err(CrossError::NotFinite { curve: which });
        }
        let in_range = |x: f64| (1e-100..=1e9).contains(&x.abs());
        if coords.clone().any(|x| x != 0.0 && !in_range(x)) {
            return Err(CrossError::OutOfRange { curve: which });
        }
        if weights.iter().any(|w| *w <= 0.0) || points.len() != weights.len() || points.is_empty() {
            return Err(CrossError::Degenerate { curve: which });
        }
        if weights.iter().any(|w| !in_range(*w)) {
            return Err(CrossError::OutOfRange { curve: which });
        }
        if shape == Shape::Line && points.first() == points.last() {
            return Err(CrossError::Degenerate { curve: which });
        }
        Ok(Self {
            shape,
            points,
            weights,
        })
    }

    /// A conic whose control points are exactly collinear.
    fn flat(&self) -> bool {
        match self.points.as_slice() {
            [p0, p1, p2] => orient(*p0, *p1, *p2) == 0,
            _ => true,
        }
    }

    /// A total order on the numbers, to choose between two curves of one rank.
    fn key(&self) -> Vec<u64> {
        let p = self.points.iter().flat_map(|p| [p.x, p.y]);
        p.chain(self.weights.iter().copied())
            .map(|x| (x + 0.0).to_bits())
            .collect()
    }

    fn scaled(&self, k: f64) -> Self {
        Self {
            shape: self.shape,
            points: self
                .points
                .iter()
                .map(|p| Point2::new(p.x * k, p.y * k))
                .collect(),
            weights: self.weights.clone(),
        }
    }

    /// The point at the middle of a parameter bracket: exactly a control point at an end.
    fn point_at(&self, t: [f64; 2]) -> Point2 {
        let m = 0.5 * (t[0] + t[1]);
        match (m, self.points.first(), self.points.last()) {
            (0.0, Some(p), _) | (1.0, _, Some(p)) => *p,
            _ => {
                // De Casteljau on the homogeneous points.
                let mut h: Vec<[f64; 3]> = self
                    .points
                    .iter()
                    .zip(&self.weights)
                    .map(|(p, w)| [p.x * w, p.y * w, *w])
                    .collect();
                while h.len() > 1 {
                    h = h
                        .iter()
                        .zip(h.iter().skip(1))
                        .map(|(u, v)| {
                            [
                                (1.0 - m) * u[0] + m * v[0],
                                (1.0 - m) * u[1] + m * v[1],
                                (1.0 - m) * u[2] + m * v[2],
                            ]
                        })
                        .collect();
                }
                h.first().map_or(Point2::new(f64::NAN, f64::NAN), |q| {
                    Point2::new(q[0] / q[2], q[1] / q[2])
                })
            }
        }
    }
}

/// The implicit curve: its equation and the signs that bound its range.
#[derive(Debug, Clone, Copy)]
enum Imp {
    /// A line from `b0` to `b1`.
    Line { b0: Point2, b1: Point2 },
    /// A conic arc over the triangle `q`, of middle weight `w`, the triangle's orientation
    /// `sigma`.
    Conic { q: [Point2; 3], w: f64, sigma: i8 },
}

impl Imp {
    fn new(r: &Rat) -> Self {
        match (r.points.as_slice(), r.weights.as_slice()) {
            ([q0, q1, q2], [_, w, _]) => Self::Conic {
                q: [*q0, *q1, *q2],
                w: *w,
                sigma: orient(*q0, *q1, *q2),
            },
            (pts, _) => Self::Line {
                b0: pts.first().copied().unwrap_or_default(),
                b1: pts.last().copied().unwrap_or_default(),
            },
        }
    }
}

/// An interval coefficient, with the exact sign when one is known: a coefficient that is
/// one exact predicate's value, or a sum or product of coefficients whose exact signs
/// settle its own.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Coef {
    iv: Ivl,
    sign: Option<i8>,
}

impl Coef {
    const ZERO: Self = Self {
        iv: Ivl::new(0.0, 0.0),
        sign: Some(0),
    };

    /// The interval, clamped to the exact sign when one is given; the sign the interval
    /// shows otherwise.
    fn new(iv: Ivl, exact: Option<i8>) -> Self {
        let shown = if !iv.is_finite() {
            None
        } else if iv.lo > 0.0 {
            Some(1)
        } else if iv.hi < 0.0 {
            Some(-1)
        } else if iv.lo == 0.0 && iv.hi == 0.0 {
            Some(0)
        } else {
            None
        };
        let sign = exact.or(shown);
        let iv = match sign {
            Some(0) => Ivl::new(0.0, 0.0),
            Some(s) if s > 0 => Ivl::new(cmax(iv.lo, 0.0), cmax(iv.hi, 0.0)),
            Some(_) => Ivl::new(cmin(iv.lo, 0.0), cmin(iv.hi, 0.0)),
            None => iv,
        };
        Self { iv, sign }
    }

    fn positive(x: f64) -> Self {
        Self::new(Ivl::from(x), Some(1))
    }

    fn add(self, o: Self) -> Self {
        let hint = match (self.sign, o.sign) {
            (Some(0), s) | (s, Some(0)) => s,
            (Some(x), Some(y)) if x == y => Some(x),
            _ => None,
        };
        Self::new(
            exact_sum(self.iv, o.iv).unwrap_or_else(|| self.iv.add_ulp(o.iv)),
            hint,
        )
    }

    fn neg(self) -> Self {
        Self {
            iv: Ivl::new(-self.iv.hi, -self.iv.lo),
            sign: self.sign.map(|s| -s),
        }
    }

    fn sub(self, o: Self) -> Self {
        self.add(o.neg())
    }

    fn mul(self, o: Self) -> Self {
        let hint = self.sign.zip(o.sign).map(|(x, y)| x * y);
        Self::new(
            exact_product(self.iv, o.iv).unwrap_or_else(|| self.iv.mul_ulp(o.iv)),
            hint,
        )
    }

    /// Times a positive interval.
    fn scale(self, k: Ivl) -> Self {
        Self::new(
            exact_product(self.iv, k).unwrap_or_else(|| self.iv.mul_ulp(k)),
            self.sign,
        )
    }

    /// Half, exactly unless it falls below the normal range.
    fn half(self) -> Self {
        let (lo, hi) = (self.iv.lo * 0.5, self.iv.hi * 0.5);
        let iv = if lo * 2.0 == self.iv.lo && hi * 2.0 == self.iv.hi {
            Ivl::new(lo, hi)
        } else {
            Ivl::outward_ulp(lo, hi)
        };
        Self::new(iv, self.sign)
    }

    /// Exactly 0.
    fn is_zero(self) -> bool {
        self.sign == Some(0)
    }
}

/// `a - b`, both coordinates, when the subtraction is exact.
fn exact_diff(a: Point2, b: Point2) -> Option<[Ivl; 2]> {
    Some([
        exact_sum(Ivl::from(a.x), Ivl::from(-b.x))?,
        exact_sum(Ivl::from(a.y), Ivl::from(-b.y))?,
    ])
}

/// `(a - a0) x (b - b0)` as a point when every step is exact; `None` sends the caller to
/// the outward-rounded value.
fn exact_cross(a: Point2, a0: Point2, b: Point2, b0: Point2) -> Option<Ivl> {
    let ([dx, dy], [ex, ey]) = (exact_diff(a, a0)?, exact_diff(b, b0)?);
    let (l, r) = (exact_product(dx, ey)?, exact_product(dy, ex)?);
    exact_sum(l, Ivl::new(-r.hi, -r.lo))
}

/// `(a - a0) . (b - b0)` as a point when every step is exact.
fn exact_dot(a: Point2, a0: Point2, b: Point2, b0: Point2) -> Option<Ivl> {
    let ([dx, dy], [ex, ey]) = (exact_diff(a, a0)?, exact_diff(b, b0)?);
    exact_sum(exact_product(dx, ex)?, exact_product(dy, ey)?)
}

/// `r` is exactly `x / (x + y)`: the sum is exact and `r` times it gives `x` back exactly.
fn exact_quotient(x: f64, y: f64, r: f64) -> bool {
    exact_sum(Ivl::from(x), Ivl::from(y))
        .and_then(|s| exact_product(Ivl::from(r), s))
        .is_some_and(|p| p.lo == x)
}

/// A coefficient from an exact predicate's sign and its value's interval, times a positive
/// weight.
fn exact_coef(value: Iv, sign: i8, weight: f64) -> Coef {
    Coef::new(value.0, Some(sign)).scale(Ivl::from(weight))
}

/// A polynomial's Bernstein coefficients on a range.
type Poly = Vec<Coef>;

/// The two halves of a Bernstein polynomial (de Casteljau at 1/2).
fn halves(c: &[Coef]) -> (Poly, Poly) {
    let mut row: Poly = c.to_vec();
    let mut left = Vec::with_capacity(c.len());
    let mut right = Vec::with_capacity(c.len());
    while let (Some(f), Some(l)) = (row.first(), row.last()) {
        left.push(*f);
        right.push(*l);
        row = row
            .iter()
            .zip(row.iter().skip(1))
            .map(|(x, y)| x.add(*y).half())
            .collect();
    }
    right.reverse();
    (left, right)
}

/// The forward differences: the next derivative's coefficients, up to a positive factor.
fn diffs(c: &[Coef]) -> Poly {
    c.iter()
        .zip(c.iter().skip(1))
        .map(|(x, y)| y.sub(*x))
        .collect()
}

/// The product of two Bernstein polynomials.
fn product(f: &[Coef], g: &[Coef]) -> Poly {
    let (n, m) = (f.len().saturating_sub(1), g.len().saturating_sub(1));
    let mut out = vec![Coef::ZERO; n + m + 1];
    for (i, fi) in f.iter().enumerate() {
        for (j, gj) in g.iter().enumerate() {
            let k =
                Iv::exact(binomial(n, i) * binomial(m, j)).div(Iv::exact(binomial(n + m, i + j)));
            if let Some(o) = out.get_mut(i + j) {
                *o = o.add(fi.mul(*gj).scale(k.0));
            }
        }
    }
    out
}

/// The common strict sign of every coefficient, if there is one.
fn one_sign(c: &[Coef]) -> Option<i8> {
    let s = c.first()?.sign?;
    (s != 0 && c.iter().all(|x| x.sign == Some(s))).then_some(s)
}

/// The function is below 0 on the open range and at `hi`, and at `lo` too unless `lo` is
/// another range's end: the range holds none of the implicit curve.
fn below(c: &[Coef], lo_owned: bool) -> bool {
    c.iter().all(|x| matches!(x.sign, Some(s) if s <= 0))
        && c.last().is_some_and(|x| x.sign == Some(-1))
        && (!lo_owned || c.first().is_some_and(|x| x.sign == Some(-1)))
}

/// The function is at or above 0 on the whole range.
fn at_least_zero(c: &[Coef]) -> bool {
    c.iter().all(|x| matches!(x.sign, Some(s) if s >= 0))
}

/// The hull of the coefficients: the function's values on the range.
fn hull(c: &[Coef]) -> Ivl {
    c.iter()
        .map(|x| x.iv)
        .reduce(Ivl::hull)
        .unwrap_or_else(|| Ivl::from(0.0))
}

/// Exact zeros at each end and the signs between, when every one between is certified:
/// the roots at the ends (of those multiplicities) and, by Descartes, at most as many
/// inside as the signs between change.
struct Zeros {
    at_lo: u32,
    at_hi: u32,
    changes: u32,
    /// The sign just inside the low end.
    first: i8,
}

fn zeros(c: &[Coef]) -> Option<Zeros> {
    let at_lo = c.iter().take_while(|x| x.is_zero()).count();
    if at_lo == c.len() {
        return Some(Zeros {
            at_lo: at_lo as u32,
            at_hi: 0,
            changes: 0,
            first: 0,
        });
    }
    let at_hi = c.iter().rev().take_while(|x| x.is_zero()).count();
    let mid = c.get(at_lo..c.len() - at_hi)?;
    let signs: Option<Vec<i8>> = mid.iter().map(|x| x.sign.filter(|s| *s != 0)).collect();
    let signs = signs?;
    let changes = signs
        .iter()
        .zip(signs.iter().skip(1))
        .filter(|(x, y)| x != y)
        .count();
    Some(Zeros {
        at_lo: at_lo as u32,
        at_hi: at_hi as u32,
        changes: changes as u32,
        first: *signs.first()?,
    })
}

/// `F` and its derivatives on `[0, 1]`, up to positive factors: the forward differences of
/// its coefficients, order by order.
fn derivatives(f: &[Coef]) -> Vec<Poly> {
    let mut out = vec![f.to_vec()];
    while let Some(d) = out.last().filter(|d| d.len() > 1).map(|d| diffs(d)) {
        out.push(d);
    }
    out
}

/// The values of a Bernstein polynomial on `[0, 1]` over `[lo, hi]`: de Casteljau at the
/// interval parameter. Evaluated from the whole curve's coefficients, so a derivative's
/// sign is judged at its true size, not at a small range's (where the `k`-th differences
/// shrink as the range's width to the `k`).
fn eval_over(c: &[Coef], lo: f64, hi: f64) -> Ivl {
    let t = Ivl::new(lo, hi);
    let u = Ivl::outward_ulp(1.0 - hi, 1.0 - lo);
    let mut row: Vec<Ivl> = c.iter().map(|x| x.iv).collect();
    while row.len() > 1 {
        row = row
            .iter()
            .zip(row.iter().skip(1))
            .map(|(x, y)| x.mul_ulp(u).add_ulp(y.mul_ulp(t)))
            .collect();
    }
    row.first().copied().unwrap_or(Ivl::whole())
}

/// The least `k` with `F`'s `k`-th derivative certified one-signed on `[lo, hi]`: then `F`
/// has at most `k` roots there, counted with multiplicity (Rolle).
fn order_on(derivs: &[Poly], lo: f64, hi: f64) -> Option<u32> {
    derivs
        .iter()
        .position(|d| {
            let v = eval_over(d, lo, hi);
            v.is_finite() && (v.lo > 0.0 || v.hi < 0.0)
        })
        .map(|k| k as u32)
}

/// A range of the parameter curve, with every polynomial restricted to it.
#[derive(Debug, Clone)]
struct Piece {
    lo: f64,
    hi: f64,
    depth: u32,
    /// The composed equation, `F = D^deg (implicit . param)`.
    f: Poly,
    /// The implicit curve's range: the piece is in it where every one is at least 0.
    mem: Vec<Poly>,
    /// The parameter curve's homogeneous coordinates: `x w`, `y w`, `w`.
    hx: Poly,
    hy: Poly,
    d: Poly,
}

impl Piece {
    fn halves(&self) -> (Self, Self) {
        let mid = 0.5 * (self.lo + self.hi);
        let (fl, fr) = halves(&self.f);
        let (ml, mr): (Vec<Poly>, Vec<Poly>) = self.mem.iter().map(|m| halves(m)).unzip();
        let (xl, xr) = halves(&self.hx);
        let (yl, yr) = halves(&self.hy);
        let (dl, dr) = halves(&self.d);
        let depth = self.depth + 1;
        (
            Self {
                lo: self.lo,
                hi: mid,
                depth,
                f: fl,
                mem: ml,
                hx: xl,
                hy: yl,
                d: dl,
            },
            Self {
                lo: mid,
                hi: self.hi,
                depth,
                f: fr,
                mem: mr,
                hx: xr,
                hy: yr,
                d: dr,
            },
        )
    }

    /// Certified off the implicit curve's range.
    fn outside(&self) -> bool {
        self.mem.iter().any(|m| below(m, self.lo == 0.0))
    }

    /// Certified in it.
    fn inside(&self) -> bool {
        self.mem.iter().all(|m| at_least_zero(m))
    }

    /// The control points' box: the piece of the parameter curve lies in it.
    fn bbox(&self) -> (IP, IP) {
        let pts = self
            .hx
            .iter()
            .zip(&self.hy)
            .zip(&self.d)
            .map(|((x, y), w)| IP {
                x: Iv(x.iv).div(Iv(w.iv)),
                y: Iv(y.iv).div(Iv(w.iv)),
            });
        let mut lo = IP::exact(Point2::new(f64::INFINITY, f64::INFINITY));
        let mut hi = IP::exact(Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
        for p in pts {
            lo = IP {
                x: Iv::exact(lo.x.0.lo.min(p.x.0.lo)),
                y: Iv::exact(lo.y.0.lo.min(p.y.0.lo)),
            };
            hi = IP {
                x: Iv::exact(hi.x.0.hi.max(p.x.0.hi)),
                y: Iv::exact(hi.y.0.hi.max(p.y.0.hi)),
            };
        }
        (lo, hi)
    }

    /// An upper bound on the piece's diameter.
    fn diameter(&self) -> f64 {
        let (lo, hi) = self.bbox();
        let d = hi.sub(lo);
        d.norm2().sqrt().hi()
    }
}

/// One finding of the halving, in the parameter curve's order.
enum Event {
    Meet(Meet),
    Cluster(Leaf),
    Overlap { a: [f64; 2], b: [f64; 2] },
}

/// A range narrower than the resolution whose crossings no isolation settles.
struct Leaf {
    lo: f64,
    hi: f64,
    near: bool,
    b: [f64; 2],
    /// `F` at the two ends.
    first: Coef,
    last: Coef,
}

/// One call's state: the parameter curve, the implicit one, the resolution, all scaled.
struct Job<'a> {
    param: Rat,
    implicit: Imp,
    res: f64,
    /// The parameter curve as given.
    param_curve: &'a Rat,
    which: Which,
}

impl Job<'_> {
    fn root(&self) -> Piece {
        let pts = &self.param.points;
        let ws = &self.param.weights;
        let hx = pts
            .iter()
            .zip(ws)
            .map(|(p, w)| {
                Coef::new(
                    exact_product(Ivl::from(p.x), Ivl::from(*w))
                        .unwrap_or_else(|| Ivl::from(p.x).mul_ulp(Ivl::from(*w))),
                    None,
                )
            })
            .collect();
        let hy = pts
            .iter()
            .zip(ws)
            .map(|(p, w)| {
                Coef::new(
                    exact_product(Ivl::from(p.y), Ivl::from(*w))
                        .unwrap_or_else(|| Ivl::from(p.y).mul_ulp(Ivl::from(*w))),
                    None,
                )
            })
            .collect();
        let d = ws.iter().map(|w| Coef::positive(*w)).collect();
        let (f, mem) = match self.implicit {
            Imp::Line { b0, b1 } => {
                let dir = IP::exact(b1).sub(IP::exact(b0));
                let side = |p: &Point2, w: &f64| {
                    let e = IP::exact(*p).sub(IP::exact(b0));
                    let v = exact_cross(b1, b0, *p, b0).map_or_else(|| dir.cross(e), Iv);
                    exact_coef(v, orient(b0, b1, *p), *w)
                };
                let ahead = |p: &Point2, w: &f64| {
                    let e = IP::exact(*p).sub(IP::exact(b0));
                    let v = exact_dot(*p, b0, b1, b0).map_or_else(|| e.dot(dir), Iv);
                    exact_coef(v, dot_sign(b0, b1, *p), *w)
                };
                let behind = |p: &Point2, w: &f64| {
                    let e = IP::exact(b1).sub(IP::exact(*p));
                    let v = exact_dot(b1, *p, b1, b0).map_or_else(|| e.dot(dir), Iv);
                    exact_coef(v, dot_sign(b1, b0, *p), *w)
                };
                let f = pts.iter().zip(ws).map(|(p, w)| side(p, w)).collect();
                let m0 = pts.iter().zip(ws).map(|(p, w)| ahead(p, w)).collect();
                let m1 = pts.iter().zip(ws).map(|(p, w)| behind(p, w)).collect();
                (f, vec![m0, m1])
            }
            Imp::Conic {
                q: [q0, q1, q2],
                w,
                sigma,
            } => {
                // A_i(X): the orientation of X against the triangle's side opposite q_i.
                let sides = [(q1, q2), (q2, q0), (q0, q1)];
                let g: Vec<Poly> = sides
                    .iter()
                    .map(|(u, v)| {
                        let dir = IP::exact(*v).sub(IP::exact(*u));
                        pts.iter()
                            .zip(ws)
                            .map(|(p, wt)| {
                                let e = IP::exact(*p).sub(IP::exact(*u));
                                let val =
                                    exact_cross(*v, *u, *p, *u).map_or_else(|| dir.cross(e), Iv);
                                exact_coef(val, orient(*u, *v, *p), *wt)
                            })
                            .collect()
                    })
                    .collect();
                let [g0, g1, g2] = g.as_slice() else {
                    return Self::empty_root();
                };
                let w4 = (Iv::exact(w).sq() * Iv::exact(4.0)).0;
                let sq = product(g1, g1);
                let cross = product(g0, g2);
                let f = sq
                    .iter()
                    .zip(&cross)
                    .map(|(s, c)| s.sub(c.scale(w4)))
                    .collect();
                let mem = g
                    .iter()
                    .map(|gi| {
                        gi.iter()
                            .map(|c| if sigma > 0 { *c } else { c.neg() })
                            .collect()
                    })
                    .collect();
                (f, mem)
            }
        };
        Piece {
            lo: 0.0,
            hi: 1.0,
            depth: 0,
            f,
            mem,
            hx,
            hy,
            d,
        }
    }

    fn empty_root() -> Piece {
        Piece {
            lo: 0.0,
            hi: 1.0,
            depth: 0,
            f: Vec::new(),
            mem: Vec::new(),
            hx: Vec::new(),
            hy: Vec::new(),
            d: Vec::new(),
        }
    }

    fn run(
        &self,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(Vec<Meet>, Vec<Overlap>), CrossError> {
        let mut events = Vec::new();
        let root = self.root();
        let derivs = derivatives(&root.f);
        let mut stack = vec![root];
        while let Some(p) = stack.pop() {
            budget.charge(1)?;
            stop.check()?;
            if p.outside() || one_sign(&p.f).is_some() {
                continue;
            }
            let z = zeros(&p.f);
            let identically_zero = z.as_ref().is_some_and(|z| z.at_lo as usize == p.f.len());
            if let Some(z) = z.as_ref().filter(|z| z.changes <= 1 && !identically_zero) {
                self.isolated(&p, z, &mut events, budget, stop)?;
                continue;
            }
            let leaf = p.diameter() <= self.res;
            let shared =
                identically_zero || (order_on(&derivs, p.lo, p.hi).is_none() && self.near(&p));
            if shared && (leaf || p.inside()) {
                events.push(Event::Overlap {
                    a: [p.lo, p.hi],
                    b: [self.b_at(&p, false), self.b_at(&p, true)],
                });
                continue;
            }
            if leaf {
                events.push(Event::Cluster(Leaf {
                    lo: p.lo,
                    hi: p.hi,
                    near: self.near(&p),
                    b: self.b_range(&p.mem.iter().map(|m| hull(m)).collect::<Vec<_>>()),
                    first: p
                        .f
                        .first()
                        .map_or_else(|| Coef::new(Ivl::whole(), None), |c| *c),
                    last: p
                        .f
                        .last()
                        .map_or_else(|| Coef::new(Ivl::whole(), None), |c| *c),
                }));
                continue;
            }
            if p.depth >= MAX_DEPTH {
                return Err(self.undecided(p.lo, p.hi));
            }
            let (l, r) = p.halves();
            stack.push(r);
            stack.push(l);
        }
        self.gather(events, &derivs)
    }

    fn undecided(&self, lo: f64, hi: f64) -> CrossError {
        CrossError::Undecided {
            curve: self.which,
            range: [lo, hi],
        }
    }

    /// A piece whose roots are settled: exact zeros at its ends, and at most one simple
    /// root inside. A root at `lo` belongs to the piece before, except at 0.
    fn isolated(
        &self,
        p: &Piece,
        z: &Zeros,
        events: &mut Vec<Event>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(), CrossError> {
        let ends = |hi: bool| -> Vec<Ivl> {
            p.mem
                .iter()
                .map(|m| if hi { m.last() } else { m.first() }.map_or(Ivl::whole(), |c| c.iv))
                .collect()
        };
        if z.at_lo > 0 && p.lo == 0.0 {
            self.push_meet(events, [0.0, 0.0], &ends(false), z.at_lo, true);
        }
        if z.changes == 1 {
            self.refine(p, z.first, events, budget, stop)?;
        }
        if z.at_hi > 0 {
            self.push_meet(events, [p.hi, p.hi], &ends(true), z.at_hi, true);
        }
        Ok(())
    }

    /// Narrows a piece's one simple root by halving, to the resolution and until its place
    /// against the implicit curve's range is settled.
    fn refine(
        &self,
        p: &Piece,
        first: i8,
        events: &mut Vec<Event>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(), CrossError> {
        let mut cur = p.clone();
        let mut extra = 0;
        loop {
            let small = cur.diameter() <= self.res;
            let settled = cur.inside() || cur.outside();
            if (small && (settled || extra >= EXTRA)) || cur.depth >= MAX_DEPTH {
                break;
            }
            if small {
                extra += 1;
            }
            budget.charge(1)?;
            stop.check()?;
            let (l, r) = cur.halves();
            match l.f.last().and_then(|c| c.sign) {
                Some(0) => {
                    let mem: Vec<Ivl> = l
                        .mem
                        .iter()
                        .map(|m| m.last().map_or(Ivl::whole(), |c| c.iv))
                        .collect();
                    self.push_meet(events, [l.hi, l.hi], &mem, 1, true);
                    return Ok(());
                }
                Some(s) if s == first => cur = r,
                Some(_) => cur = l,
                None => break,
            }
        }
        let mem: Vec<Ivl> = cur.mem.iter().map(|m| hull(m)).collect();
        self.push_meet(events, [cur.lo, cur.hi], &mem, 1, true);
        Ok(())
    }

    /// A meet at a parameter bracket, unless the implicit curve's range certainly excludes
    /// it (some range value below 0).
    fn push_meet(&self, events: &mut Vec<Event>, a: [f64; 2], mem: &[Ivl], m: u32, exact: bool) {
        if mem.iter().any(|v| v.hi < 0.0) {
            return;
        }
        events.push(Event::Meet(Meet {
            a,
            b: self.b_range(mem),
            at: self.param_curve.point_at(a),
            multiplicity: m,
            exact,
        }));
    }

    /// The implicit curve's parameter over values of its range polynomials: a line's
    /// `s = m0 / (m0 + m1)`, a conic's `t = sqrt(g2) / (sqrt(g0) + sqrt(g2))`. Clamped to
    /// `[0, 1]`.
    fn b_range(&self, mem: &[Ivl]) -> [f64; 2] {
        let (u, v) = match (self.implicit, mem) {
            (Imp::Line { .. }, [m0, m1, ..]) => (*m0, *m1),
            (Imp::Conic { .. }, [g0, _, g2]) => (g2.sqrt(), g0.sqrt()),
            _ => return [0.0, 1.0],
        };
        // u / (u + v), increasing in u and decreasing in v, both taken at or above 0.
        let (ulo, uhi) = (cmax(u.lo, 0.0), cmax(u.hi, 0.0));
        let (vlo, vhi) = (cmax(v.lo, 0.0), cmax(v.hi, 0.0));
        let ratio = |x: f64, y: f64, up: bool| {
            let s = x + y;
            if x > 0.0 && y == 0.0 {
                1.0
            } else if x == 0.0 && y > 0.0 {
                0.0
            } else if s > 0.0 && s.is_finite() {
                let r = x / s;
                if exact_quotient(x, y, r) {
                    r
                } else if up {
                    r.next_up()
                } else {
                    r.next_down()
                }
            } else if up {
                1.0
            } else {
                0.0
            }
        };
        let lo = if ulo == 0.0 {
            0.0
        } else {
            ratio(ulo, vhi, false)
        };
        let hi = if vlo == 0.0 {
            1.0
        } else {
            ratio(uhi, vlo, true)
        };
        [lo.clamp(0.0, 1.0), hi.clamp(0.0, 1.0)]
    }

    /// The implicit curve's parameter at one end of a piece.
    fn b_at(&self, p: &Piece, hi: bool) -> f64 {
        let mem: Vec<Ivl> = p
            .mem
            .iter()
            .map(|m| if hi { m.last() } else { m.first() }.map_or(Ivl::whole(), |c| c.iv))
            .collect();
        let [lo, up] = self.b_range(&mem);
        0.5 * (lo + up)
    }

    /// Every point of the piece is proved within the resolution of the implicit curve's
    /// support (its line, or its whole conic).
    fn near(&self, p: &Piece) -> bool {
        let fmax = p.f.iter().map(|c| c.iv.mag()).fold(0.0_f64, f64::max);
        let dmin = p.d.iter().map(|c| c.iv.lo).fold(f64::INFINITY, f64::min);
        if dmin.is_nan() || dmin <= 0.0 || !fmax.is_finite() {
            return false;
        }
        match self.implicit {
            Imp::Line { b0, b1 } => {
                // |L(X)| / |b1 - b0| is the distance, and F = D L.
                let len = IP::exact(b1).sub(IP::exact(b0)).norm2().sqrt();
                let dist = Iv::exact(fmax).div(Iv::exact(dmin) * len);
                dist.0.hi <= self.res
            }
            Imp::Conic {
                q: [q0, q1, q2], w, ..
            } => {
                // F = D^2 Q with Q = A1^2 - 4 w^2 A0 A2. Along the gradient from a point X,
                // Q(s) <= q - g s + k s^2 with q = |Q(X)|, g = |grad Q(X)| and k bounding
                // half the Hessian: a zero within 2 q / g when 4 q k < g^2.
                let q = Iv::exact(fmax).div(Iv::exact(dmin).sq());
                let (lo, hi) = p.bbox();
                let x = IP {
                    x: lo.x.hull(hi.x),
                    y: lo.y.hull(hi.y),
                };
                let sides = [(q1, q2), (q2, q0), (q0, q1)];
                let a: Vec<(Iv, IP)> = sides
                    .iter()
                    .map(|(u, v)| {
                        let dir = IP::exact(*v).sub(IP::exact(*u));
                        (dir.cross(x.sub(IP::exact(*u))), dir.perp())
                    })
                    .collect();
                let [(a0, n0), (a1, n1), (a2, n2)] = a.as_slice() else {
                    return false;
                };
                let w4 = Iv::exact(w).sq() * Iv::exact(4.0);
                let two = Iv::exact(2.0);
                let grad = n1
                    .scale(*a1 * two)
                    .sub(n2.scale(*a0 * w4).add(n0.scale(*a2 * w4)));
                let g2 = grad.norm2();
                let k = n1.norm2() + w4 * (n0.norm2() * n2.norm2()).sqrt();
                let lhs = Iv::exact(4.0) * q * k;
                let g = g2.sqrt();
                let reach = two * q.div(g);
                crate::cert::greater(g2, lhs) == scx_num::certified::Verdict::Yes
                    && reach.0.hi <= self.res
            }
        }
    }

    /// The meets and shared stretches from the halving's findings: adjacent clusters into
    /// one meet each, adjacent stretches into one.
    fn gather(
        &self,
        events: Vec<Event>,
        derivs: &[Poly],
    ) -> Result<(Vec<Meet>, Vec<Overlap>), CrossError> {
        let mut meets = Vec::new();
        let mut overlaps: Vec<Overlap> = Vec::new();
        let mut run: Vec<Leaf> = Vec::new();
        for e in events {
            match e {
                Event::Cluster(l) => {
                    if run.last().is_some_and(|r| r.hi != l.lo) {
                        self.cluster(&run, derivs, &mut meets)?;
                        run.clear();
                    }
                    run.push(l);
                }
                other => {
                    if !run.is_empty() {
                        self.cluster(&run, derivs, &mut meets)?;
                        run.clear();
                    }
                    match other {
                        Event::Meet(m) => meets.push(m),
                        Event::Overlap { a, b } => match overlaps.last_mut() {
                            Some(o) if o.a[1] == a[0] => {
                                o.a[1] = a[1];
                                o.b[1] = b[1];
                            }
                            _ => overlaps.push(Overlap { a, b }),
                        },
                        Event::Cluster(_) => {}
                    }
                }
            }
        }
        if !run.is_empty() {
            self.cluster(&run, derivs, &mut meets)?;
        }
        meets.sort_by(|x, y| x.a[0].total_cmp(&y.a[0]).then(x.a[1].total_cmp(&y.a[1])));
        Ok((meets, overlaps))
    }

    /// One meet from a run of adjacent clusters: the least derivative certified one-signed
    /// on all of them bounds the roots (Rolle), the signs at the run's ends give their
    /// parity.
    fn cluster(
        &self,
        run: &[Leaf],
        derivs: &[Poly],
        meets: &mut Vec<Meet>,
    ) -> Result<(), CrossError> {
        let (Some(first), Some(last)) = (run.first(), run.last()) else {
            return Ok(());
        };
        let (lo, hi) = (first.lo, last.hi);
        let Some(k) = order_on(derivs, lo, hi) else {
            return Err(self.undecided(lo, hi));
        };
        let s0 = first.first.sign.filter(|s| *s != 0);
        let s1 = last.last.sign.filter(|s| *s != 0);
        let near = run.iter().any(|l| l.near);
        let zero_end = first.first.is_zero() || last.last.is_zero();
        let (m, exact) = if let (Some(x), Some(y)) = (s0, s1) {
            let odd = x != y;
            let m = if (k % 2 == 1) == odd {
                k
            } else {
                k.saturating_sub(1)
            };
            if m == 0 {
                return Ok(());
            }
            if !odd && !near {
                return Err(self.undecided(lo, hi));
            }
            (m, m == 1)
        } else {
            // An end's sign unknown (an exact zero there, or a value the rounding hides):
            // the bound alone, when a root is known there or the curves are proved near.
            if k == 0 || !(near || zero_end) {
                return Err(self.undecided(lo, hi));
            }
            (k, false)
        };
        let b = run
            .iter()
            .map(|l| l.b)
            .reduce(|x, y| [x[0].min(y[0]), x[1].max(y[1])])
            .unwrap_or([0.0, 1.0]);
        meets.push(Meet {
            a: [lo, hi],
            b,
            at: self.param_curve.point_at([lo, hi]),
            multiplicity: m,
            exact,
        });
        Ok(())
    }
}
