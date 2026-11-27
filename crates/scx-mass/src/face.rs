// Written for solidcorex. Apache-2.0.
//! One face's boundary integral, the pin's method (`boundary_properties`): Green's theorem in
//! the face's uv turns `∫∫ F du dv` into `∮ -G(u, v) du` with `G(u, v) = ∫_base^v F(u, s) ds`
//! and `base` the loops' lowest `v`.
//!
//! - The outer integral runs over each trim piece by the 7-point Kronrod rule, split at the
//!   trim's own knots and where it crosses a break line of the surface; the inner one by the
//!   15-point rule, split at the surface's `v` breaks inside `(base, v)`. Both are held to a
//!   tenth of the accuracy asked.
//! - Where `du/dt` is exactly zero (a seam, a meridian) the integrand is zero and the inner
//!   integral is not taken.
//! - The outer loop runs counter-clockwise in uv seen from the front and the inner loops
//!   clockwise (`scx_brep::record`), so the sum is the integral over the face, its
//!   orientation the natural normal's `S_u x S_v`.
//! - A use's pcurve runs over the edge's interval, min to max, whatever the use's sense; a
//!   reversed use is integrated over the same interval and negated.
//! - Consecutive pieces of a loop meet where the next starts: a pole (a point use) is
//!   closed by the straight path between the pcurves either side, along the pole's line;
//!   a jump by whole periods (a loop crossing a seam) is no path at all, the seam's `du`
//!   being zero. A gap left after that is closed by a straight path ([`join`], the place
//!   for MA2's gap closure onto the edge's curve).
//! - Each face is integrated about its own reference ([`reference`]), so its subdivision and
//!   rounding depend on the face alone; the caller shifts the result.
//! - Not ported: the pin's banding of a long trim into twelve strips (`MIN_BANDS`). It only
//!   changes how the work is split; the step control here bisects where it is needed.

use scx_brep::{Entity, Face, FaceId, UseGeom};
use scx_curve::Curve2;
use scx_num::frame::Sense;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::Approach;
use scx_surface::Surface;

use crate::breaks::{Dir, crossings, curve_breaks, surface_breaks};
use crate::error::{MassDefect, MassError, MassUnsupported};
use crate::integrand::Set;
use crate::kronrod::{self, Fail, K7, K15, MAX_COMPONENTS, Meter, Tol};

/// The samples a piece is read at to find its crossings, its extent and its length.
const SAMPLES: u32 = 16;

/// A face's integral about its reference: [`Set`]-ordered values and error estimates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FaceIntegral {
    pub(crate) reference: Point3,
    pub(crate) value: [f64; MAX_COMPONENTS],
    pub(crate) error: [f64; MAX_COMPONENTS],
}

/// What a piece of a loop runs along.
#[derive(Clone, Copy)]
enum Path<'a> {
    /// A pcurve, forward or negated.
    Curve { curve: &'a Curve2, sign: f64 },
    /// A straight path `a + t d`, `t` in `[0, 1]`.
    Segment { a: Point2, d: Vec2 },
}

/// A piece of a loop: a path over `[lo, hi]`.
#[derive(Clone, Copy)]
struct Piece<'a> {
    path: Path<'a>,
    lo: f64,
    hi: f64,
}

impl Piece<'_> {
    /// The point and the derivative along the loop's direction at `t`.
    fn at(&self, t: f64) -> (Point2, Vec2) {
        match self.path {
            Path::Curve { curve, sign } => {
                let j = curve.jet::<1>(t, Approach::Above);
                let [d] = j.d;
                (j.point, d * sign)
            }
            Path::Segment { a, d } => (a + d * t, d),
        }
    }

    fn point(&self, t: f64) -> Point2 {
        match self.path {
            Path::Curve { curve, .. } => curve.point(t),
            Path::Segment { a, d } => a + d * t,
        }
    }

    /// Where the loop enters the piece.
    fn start(&self) -> Point2 {
        match self.path {
            Path::Curve { sign, .. } if sign < 0.0 => self.point(self.hi),
            _ => self.point(self.lo),
        }
    }

    /// Where the loop leaves it.
    fn end(&self) -> Point2 {
        match self.path {
            Path::Curve { sign, .. } if sign < 0.0 => self.point(self.lo),
            _ => self.point(self.hi),
        }
    }

    /// Its own breaks inside `(lo, hi)`.
    fn breaks(&self) -> Vec<f64> {
        match self.path {
            Path::Curve { curve, .. } => curve_breaks(curve, self.lo, self.hi),
            Path::Segment { .. } => Vec::new(),
        }
    }
}

/// The surface's periods: `u`, then `v`.
type Periods = (Option<f64>, Option<f64>);

/// `x` less the whole multiple of `period` nearest it.
fn reduce(x: f64, period: Option<f64>) -> f64 {
    match period {
        Some(p) => x - (x / p).round() * p,
        None => x,
    }
}

/// The path that closes a loop from `from` (where a piece ends) to `to` (where the next
/// starts), once whole periods are dropped: none where it runs along `v` alone, whose `du`
/// is zero. Exact pcurves meet at their vertex, so this closes only a pole and the rounding
/// of a pcurve's end; MA2's gap closure onto the edge's curve joins here, for a use with a
/// recorded accuracy.
fn join<'a>(from: Point2, to: Point2, periods: Periods) -> (Vec2, Option<Piece<'a>>) {
    let gap = to - from;
    let d = Vec2::new(reduce(gap.x, periods.0), reduce(gap.y, periods.1));
    let piece = (d.x != 0.0).then_some(Piece {
        path: Path::Segment { a: from, d },
        lo: 0.0,
        hi: 1.0,
    });
    (d, piece)
}

/// The face's loops as pieces, joined; refuses the windings the integral cannot close.
fn pieces(face: Face<'_>, periods: Periods) -> Result<Vec<Piece<'_>>, MassError> {
    let id = face.id();
    let mut out = Vec::new();
    let mut net_u = 0.0;
    for lp in face.loops() {
        let curves: Vec<Piece<'_>> = lp
            .uses()
            .filter_map(|u| match u.geom() {
                UseGeom::Pcurve { curve, interval } => Some(Piece {
                    path: Path::Curve {
                        curve,
                        sign: match u.sense() {
                            Sense::Forward => 1.0,
                            Sense::Reversed => -1.0,
                        },
                    },
                    lo: interval.min(),
                    hi: interval.max(),
                }),
                UseGeom::Point(_) => None,
            })
            .collect();
        let (mut wind_u, mut wind_v) = (0.0, 0.0);
        for (i, p) in curves.iter().enumerate() {
            let next = curves.get((i + 1) % curves.len()).unwrap_or(p);
            let (s, e) = (p.start(), p.end());
            let (d, closing) = join(e, next.start(), periods);
            wind_u += (e.x - s.x) + d.x;
            wind_v += (e.y - s.y) + d.y;
            out.push(*p);
            out.extend(closing);
        }
        if let Some(pv) = periods.1
            && wind_v.abs() > 0.5 * pv
        {
            return Err(MassError::Unsupported(MassUnsupported::VWrap { face: id }));
        }
        net_u += wind_u;
    }
    if let Some(pu) = periods.0
        && net_u.abs() > 0.5 * pu
    {
        return Err(MassError::Unsupported(MassUnsupported::NetWinding {
            face: id,
        }));
    }
    Ok(out)
}

/// `2^floor(log2 d)` for a finite `d > 0` in the normal range; 1 otherwise.
fn binade(d: f64) -> f64 {
    if d.is_normal() && d > 0.0 {
        f64::from_bits(d.to_bits() & 0x7ff0_0000_0000_0000)
    } else {
        1.0
    }
}

/// A box's middle rounded to the power-of-two grid a sixteenth of the box's binade, so it
/// is exact and moves only with the box: the face's (or body's) reference.
pub(crate) fn reference(mid: Point3, size: f64) -> Point3 {
    let g = binade(size) / 16.0;
    let r = |x: f64| (x / g).round() * g;
    Point3::new(r(mid.x), r(mid.y), r(mid.z))
}

/// The pin's area estimate of a box: half its surface, the larger of a face's projections.
fn area_estimate(sides: Vec3) -> f64 {
    let a = sides.x * sides.y + sides.y * sides.z + sides.z * sides.x;
    if a > 0.0 {
        a
    } else {
        let d = sides.x.max(sides.y).max(sides.z);
        d * d
    }
}

/// Maps the integrator's failure at `face` to the error it is.
pub(crate) fn fail(at: Entity, f: Fail) -> MassError {
    match f {
        Fail::Step => MassError::Defect(MassDefect::StepControl { at }),
        Fail::NotFinite => MassError::Defect(MassDefect::IntegrandNotFinite { at }),
        Fail::Exhausted(limit) => MassError::Exhausted { limit },
        Fail::Cancelled => MassError::Cancelled,
    }
}

/// What the inner integral reads.
struct Inner<'a> {
    surface: &'a Surface,
    set: Set,
    reference: Vec3,
    base: f64,
    accuracy: f64,
    /// Per component, the floor per unit of `u` times `v`.
    density: [f64; MAX_COMPONENTS],
    /// The surface's `v` breaks inside the face's `v` range.
    v_breaks: Vec<f64>,
}

impl Inner<'_> {
    /// `G(u, v)` into `g` and its error into `ge`.
    fn at(
        &self,
        u: f64,
        v: f64,
        g: &mut [f64],
        ge: &mut [f64],
        meter: &mut Meter<'_>,
    ) -> Result<(), Fail> {
        let (lo, hi, sign) = if v >= self.base {
            (self.base, v, 1.0)
        } else {
            (v, self.base, -1.0)
        };
        // NaN ends integrate nothing, as an empty interval.
        if lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less) {
            return Ok(());
        }
        let n = self.set.len();
        let mut ends = Vec::with_capacity(self.v_breaks.len() + 2);
        ends.push(lo);
        ends.extend(self.v_breaks.iter().copied().filter(|&b| lo < b && b < hi));
        ends.push(hi);
        for w in ends.windows(2) {
            let [a, b] = *w else { continue };
            let mut floor = [0.0; MAX_COMPONENTS];
            for (f, d) in floor.iter_mut().zip(&self.density) {
                *f = d * (b - a);
            }
            let tol = Tol {
                accuracy: self.accuracy,
                floor: floor.get(..n).unwrap_or_default(),
            };
            kronrod::integrate(
                &K15,
                (a, b),
                tol,
                g.get_mut(..n).unwrap_or_default(),
                ge.get_mut(..n).unwrap_or_default(),
                meter,
                |s, out, _| {
                    let j = self
                        .surface
                        .jet::<1>(Point2::new(u, s), [Approach::Above; 2]);
                    let (Some(su), Some(sv)) = (j.get(1, 0), j.get(0, 1)) else {
                        return Err(Fail::NotFinite);
                    };
                    self.set
                        .eval(j.point() - self.reference, su.cross(sv), out.value);
                    Ok(())
                },
            )?;
        }
        if sign < 0.0 {
            for x in g.iter_mut().take(n) {
                *x = -*x;
            }
        }
        Ok(())
    }
}

/// The face's `set` integrals about its reference, each piece held to `accuracy / 10`.
pub(crate) fn integrate(
    face: Face<'_>,
    set: Set,
    accuracy: f64,
    meter: &mut Meter<'_>,
) -> Result<FaceIntegral, MassError> {
    let id: FaceId = face.id();
    let at = Entity::from(id);
    let bbox = face
        .bounding_box()
        .ok_or(MassError::Unsupported(MassUnsupported::NoBox { at }))?;
    let size = bbox.max_length();
    let reference = reference(bbox.mid(), size);
    let surface = face.surface();
    let domain = surface.domain();
    let periods = (domain.u.period(), domain.v.period());
    let pieces = pieces(face, periods)?;

    // Each piece's samples: the extents, the base, the lengths and the crossings.
    let mut sampled = Vec::with_capacity(pieces.len());
    let (mut u_lo, mut u_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut v_lo, mut v_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut total = 0.0;
    for p in &pieces {
        let pts: Vec<(f64, Point2)> = (0..=SAMPLES)
            .map(|i| {
                let t = p.lo + (p.hi - p.lo) * f64::from(i) / f64::from(SAMPLES);
                (t, p.point(t))
            })
            .collect();
        let mut len = 0.0;
        for w in pts.windows(2) {
            if let [(_, a), (_, b)] = w {
                len += (*b - *a).length();
            }
        }
        for (_, q) in &pts {
            u_lo = u_lo.min(q.x);
            u_hi = u_hi.max(q.x);
            v_lo = v_lo.min(q.y);
            v_hi = v_hi.max(q.y);
        }
        total += len;
        sampled.push((pts, len));
    }
    let mut value = [0.0; MAX_COMPONENTS];
    let mut error = [0.0; MAX_COMPONENTS];
    if pieces.is_empty() || !(u_lo <= u_hi && v_lo <= v_hi) {
        return Ok(FaceIntegral {
            reference,
            value,
            error,
        });
    }
    let base = v_lo;
    let sides = bbox.max() - bbox.min();
    let area = area_estimate(sides);
    let powers = set.powers();
    let mut scale = [0.0; MAX_COMPONENTS];
    for (s, &p) in scale.iter_mut().zip(&powers) {
        *s = area * (0..p).fold(1.0, |a, _| a * size);
    }
    let uv_area = (u_hi - u_lo) * (v_hi - v_lo);
    let uv_area = if uv_area > 0.0 { uv_area } else { 1.0 };
    let mut density = [0.0; MAX_COMPONENTS];
    for (d, s) in density.iter_mut().zip(&scale) {
        *d = s / uv_area;
    }
    let inner = Inner {
        surface,
        set,
        reference: reference - Point3::new(0.0, 0.0, 0.0),
        base,
        accuracy: accuracy / 10.0,
        density,
        v_breaks: surface_breaks(surface, Dir::V, v_lo, v_hi),
    };
    let u_lines = surface_breaks(surface, Dir::U, u_lo, u_hi);
    let n = set.len();
    let total = if total > 0.0 { total } else { 1.0 };
    for (p, (pts, len)) in pieces.iter().zip(&sampled) {
        let mut ends = vec![p.lo, p.hi];
        ends.extend(p.breaks());
        let us: Vec<(f64, f64)> = pts.iter().map(|(t, q)| (*t, q.x)).collect();
        let vs: Vec<(f64, f64)> = pts.iter().map(|(t, q)| (*t, q.y)).collect();
        ends.extend(crossings(&us, &u_lines, |t| p.point(t).x));
        ends.extend(crossings(&vs, &inner.v_breaks, |t| p.point(t).y));
        ends.sort_by(f64::total_cmp);
        ends.dedup();
        let width = p.hi - p.lo;
        for w in ends.windows(2) {
            let [a, b] = *w else { continue };
            let share = len / total * (b - a) / width;
            let mut floor = [0.0; MAX_COMPONENTS];
            for (f, s) in floor.iter_mut().zip(&scale) {
                *f = s * share;
            }
            let tol = Tol {
                accuracy: accuracy / 10.0,
                floor: floor.get(..n).unwrap_or_default(),
            };
            kronrod::integrate(
                &K7,
                (a, b),
                tol,
                value.get_mut(..n).unwrap_or_default(),
                error.get_mut(..n).unwrap_or_default(),
                meter,
                |t, out, meter| {
                    let (q, d) = p.at(t);
                    if d.x == 0.0 {
                        return Ok(());
                    }
                    let mut g = [0.0; MAX_COMPONENTS];
                    let mut ge = [0.0; MAX_COMPONENTS];
                    inner.at(q.x, q.y, &mut g, &mut ge, meter)?;
                    for (((v, e), gv), gerr) in out
                        .value
                        .iter_mut()
                        .zip(out.error.iter_mut())
                        .zip(&g)
                        .zip(&ge)
                    {
                        *v = -gv * d.x;
                        *e = (gerr * d.x).abs();
                    }
                    Ok(())
                },
            )
            .map_err(|f| fail(at, f))?;
        }
    }
    Ok(FaceIntegral {
        reference,
        value,
        error,
    })
}
