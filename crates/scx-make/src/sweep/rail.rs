// Written for solidcorex. Apache-2.0.
//! Guide rails (research 75 MK7b): where a rail crosses a station's plane.
//!
//! A rail is a line, a circular arc or a clamped NURBS curve (a polyline is a degree-1
//! one). Each is taken as rational Bezier spans in homogeneous form: a line is one span of
//! degree 1, an arc one quadratic span per quarter turn or less (the middle weight
//! `cos(half the span's angle)`), a NURBS curve its spans after inserting every interior
//! knot to the degree. The plane through `p` square to the unit `t` meets a span at the
//! roots of its signed distance `(X(s) - p W(s)) . t`, whose Bernstein coefficients are the
//! control points' own, isolated by [`bernstein_roots`] (the pin's `rail_crossing`).
//!
//! - A coefficient within its rounding (`gamma_5` of the sum of its terms' sizes, from the
//!   five roundings of `sum_j (x_ij - p_j w_i) t_j`, inflated by `1 + gamma_6` for the
//!   bound's own) is 0: the control point lies in the plane to its rounding.
//! - The rail's own ends lie in the plane when within the document's resolution of it (the
//!   rule by which a path's pieces meet in research 75's `MK7a`, in place of the pin's
//!   relative `1e-9`): a rail drawn to start on the path's first station reaches it however
//!   its end rounded.
//! - A root at a knot is one value for both spans that share it (exact duplicates only).
//! - A plane met other than exactly once, with the distance changing sign over the rail, is
//!   refused by name ([`RailFault`]).

use core::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use scx_algo::bernstein::{RootLimits, bernstein_roots};
use scx_num::certified::gamma;
use scx_num::fmath;
use scx_num::vector3d::{Point3, UnitVec3, Vec3};
use scx_nurbs::NurbsCurve;

use crate::frames::Rotation;

/// A guide rail.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Rail {
    /// From `from` to `to`.
    Line {
        /// Its start.
        from: Point3,
        /// Its end.
        to: Point3,
    },
    /// From `start`, `angle` radians (in `(0, 2 pi]`) right-handed about `axis` through
    /// `centre`.
    Arc {
        /// A point of the axis.
        centre: Point3,
        /// The axis.
        axis: UnitVec3,
        /// Its start, off the axis.
        start: Point3,
        /// The angle run (radians).
        angle: f64,
    },
    /// A clamped NURBS curve over its domain.
    Nurbs(Arc<NurbsCurve<3>>),
}

/// Why a rail does not guide a station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RailFault {
    /// A line not finite or not longer than the resolution, an arc not finite, of no radius
    /// or of an angle outside `(0, 2 pi]`, or a NURBS curve not clamped.
    Shape,
    /// It does not reach the station's plane: a rail must run the path's length.
    Short,
    /// It touches the station's plane without crossing it.
    Touches,
    /// It crosses the station's plane more than once.
    Twice,
    /// It lies in the station's plane.
    InPlane,
    /// It runs too near the station's plane to tell where it crosses.
    TooNear,
}

impl RailFault {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Short => "short",
            Self::Touches => "touches",
            Self::Twice => "twice",
            Self::InPlane => "in_plane",
            Self::TooNear => "too_near",
        }
    }

    pub(crate) const fn says(self) -> &'static str {
        match self {
            Self::Shape => "is not a finite line, arc or clamped NURBS curve of some size",
            Self::Short => "does not reach the station's plane: it must run the path's length",
            Self::Touches => "touches the station's plane without crossing it",
            Self::Twice => "crosses the station's plane more than once",
            Self::InPlane => "lies in the station's plane",
            Self::TooNear => "runs too near the station's plane to tell where it crosses",
        }
    }
}

/// A rational Bezier span in homogeneous form `(w x, w y, w z, w)` over `[lo, hi]` of the
/// rail's parameter.
struct Span {
    lo: f64,
    hi: f64,
    hom: Vec<[f64; 4]>,
}

fn hom(p: Point3, w: f64) -> [f64; 4] {
    [p.x * w, p.y * w, p.z * w, w]
}

impl Rail {
    /// The rail's control points' count in its NURBS form: the skin's least station count
    /// follows it (the pin's `fractions`).
    pub(crate) fn control_points(&self) -> usize {
        match self {
            Self::Line { .. } => 2,
            Self::Arc { angle, .. } => 2 * quarters(*angle) + 1,
            Self::Nurbs(c) => c.points().len(),
        }
    }

    /// Checks the rail's shape at the resolution `tiny`.
    pub(crate) fn check(&self, tiny: f64) -> Result<(), RailFault> {
        let fine = match self {
            Self::Line { from, to } => {
                from.is_finite() && to.is_finite() && from.distance(*to) > tiny
            }
            Self::Arc {
                centre,
                start,
                angle,
                ..
            } => {
                let r = self.radial().map_or(0.0, |(_, r)| r.length());
                centre.is_finite()
                    && start.is_finite()
                    && *angle > 0.0
                    && *angle <= core::f64::consts::TAU
                    && r > tiny
                    && r.is_finite()
            }
            Self::Nurbs(c) => c.knot_vector().is_clamped(),
        };
        if fine { Ok(()) } else { Err(RailFault::Shape) }
    }

    /// An arc's foot of the start on the axis and its radius vector there.
    fn radial(&self) -> Option<(Point3, Vec3)> {
        let Self::Arc {
            centre,
            axis,
            start,
            ..
        } = *self
        else {
            return None;
        };
        let d = start - centre;
        let n = axis.as_vec();
        let radial = d - n * n.dot(d);
        Some((start - radial, radial))
    }

    fn spans(&self) -> Result<Vec<Span>, RailFault> {
        match self {
            Self::Line { from, to } => Ok(vec![Span {
                lo: 0.0,
                hi: 1.0,
                hom: vec![hom(*from, 1.0), hom(*to, 1.0)],
            }]),
            Self::Arc { axis, angle, .. } => {
                let (foot, radial) = self.radial().ok_or(RailFault::Shape)?;
                let k = quarters(*angle);
                let step = angle / k as f64;
                let w = fmath::cos(0.5 * step);
                let on = |phi: f64| foot + Rotation::about(*axis, phi).apply(radial);
                Ok((0..k)
                    .map(|i| {
                        let a = step * i as f64;
                        let mid =
                            foot + Rotation::about(*axis, a + 0.5 * step).apply(radial) * (1.0 / w);
                        Span {
                            lo: a,
                            hi: a + step,
                            hom: vec![hom(on(a), 1.0), hom(mid, w), hom(on(a + step), 1.0)],
                        }
                    })
                    .collect())
            }
            Self::Nurbs(c) => nurbs_spans(c),
        }
    }
}

/// The quarter turns (at least 1) an arc of `angle` is split into.
fn quarters(angle: f64) -> usize {
    let q = (angle / FRAC_PI_2).ceil();
    if q.is_finite() && q >= 1.0 {
        q as usize
    } else {
        1
    }
}

/// A clamped NURBS curve's Bezier spans: each interior breakpoint inserted to the degree.
fn nurbs_spans(c: &NurbsCurve<3>) -> Result<Vec<Span>, RailFault> {
    let p = c.degree();
    let domain = c.domain();
    let (lo, hi) = (domain.min(), domain.max());
    let breaks: Vec<(f64, usize)> = c.knot_vector().breakpoints().collect();
    let mut full = c.clone();
    for &(u, m) in &breaks {
        if u > lo && u < hi && m < p {
            full = full.insert_knot(u, p - m).map_err(|_| RailFault::Shape)?;
        }
    }
    let points: Vec<[f64; 4]> = match (full.weighted_points(), full.weights()) {
        (Some(wp), Some(w)) => wp
            .iter()
            .zip(w)
            .map(|(q, &w)| [q[0], q[1], q[2], w])
            .collect(),
        _ => full
            .points()
            .iter()
            .map(|q| [q[0], q[1], q[2], 1.0])
            .collect(),
    };
    let mut values: Vec<f64> = breaks.iter().map(|&(u, _)| u).collect();
    values.dedup();
    let mut out = Vec::new();
    for (j, pair) in values.windows(2).enumerate() {
        let (&[a, b], Some(hom)) = (pair, points.get(j * p..=j * p + p)) else {
            return Err(RailFault::Shape);
        };
        out.push(Span {
            lo: a,
            hi: b,
            hom: hom.to_vec(),
        });
    }
    if out.is_empty() {
        return Err(RailFault::Shape);
    }
    Ok(out)
}

/// The point of a rational Bezier span at `s` in `[0, 1]` (de Casteljau, homogeneous).
fn point_at(hom: &[[f64; 4]], s: f64) -> Option<Point3> {
    let mut work = hom.to_vec();
    let n = work.len();
    for level in 1..n {
        for i in 0..n - level {
            let (Some(&a), Some(&b)) = (work.get(i), work.get(i + 1)) else {
                return None;
            };
            let [ax, ay, az, aw] = a;
            let [bx, by, bz, bw] = b;
            let mixed = [
                ax + (bx - ax) * s,
                ay + (by - ay) * s,
                az + (bz - az) * s,
                aw + (bw - aw) * s,
            ];
            *work.get_mut(i)? = mixed;
        }
    }
    let [x, y, z, w] = *work.first()?;
    Some(Point3::new(x / w, y / w, z / w))
}

/// Where `rail` crosses the plane through `p` square to the unit `t`, `tiny` the document's
/// resolution (module docs).
pub(crate) fn crossing(
    rail: &Rail,
    p: Point3,
    t: UnitVec3,
    tiny: f64,
) -> Result<Point3, RailFault> {
    let spans = rail.spans()?;
    let t = t.as_vec();
    let floor = gamma(5) * (1.0 + gamma(6));
    let last_span = spans.len().saturating_sub(1);
    // (parameter, span, place in it) of each root, in order
    let mut roots: Vec<(f64, usize, f64)> = Vec::new();
    // the signed distances at the rail's own two ends (zeroed as the coefficients are)
    let mut ends: Option<(f64, f64)> = None;
    for (k, span) in spans.iter().enumerate() {
        let n = span.hom.len();
        let c: Vec<f64> = span
            .hom
            .iter()
            .enumerate()
            .map(|(i, &[x, y, z, w])| {
                let d = (x - p.x * w) * t.x + (y - p.y * w) * t.y + (z - p.z * w) * t.z;
                let size = (x.abs() + p.x.abs() * w) * t.x.abs()
                    + (y.abs() + p.y.abs() * w) * t.y.abs()
                    + (z.abs() + p.z.abs() * w) * t.z.abs();
                let rail_end = (k == 0 && i == 0) || (k == last_span && i + 1 == n);
                let reaches = rail_end && (d / w).abs() <= tiny;
                if d.abs() <= floor * size || reaches {
                    0.0
                } else {
                    d
                }
            })
            .collect();
        if c.iter().all(|x| *x == 0.0) {
            return Err(RailFault::InPlane);
        }
        if let (Some(&first), Some(&last)) = (c.first(), c.last()) {
            ends = Some((ends.map_or(first, |(s, _)| s), last));
        }
        let found = bernstein_roots(&c, 0.0, 1.0, RootLimits::DEFAULT);
        if !found.isolated {
            return Err(RailFault::TooNear);
        }
        for s in found.places {
            // a span's ends are its knots exactly, so a root at a knot is one value for both
            // spans that share it
            let x = if s == 0.0 {
                span.lo
            } else if s == 1.0 {
                span.hi
            } else {
                span.lo + (span.hi - span.lo) * s
            };
            if !roots.iter().any(|r| r.0 == x) {
                roots.push((x, k, s));
            }
        }
    }
    match roots.as_slice() {
        [(_, k, s)] => {
            let same_side = ends.is_some_and(|(a, b)| (a > 0.0 && b > 0.0) || (a < 0.0 && b < 0.0));
            if same_side {
                return Err(RailFault::Touches);
            }
            let span = spans.get(*k).ok_or(RailFault::Shape)?;
            point_at(&span.hom, *s).ok_or(RailFault::Shape)
        }
        [] => Err(RailFault::Short),
        _ => Err(RailFault::Twice),
    }
}
