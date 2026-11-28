// Written for solidcorex. Apache-2.0.
//! The loft's certificate: a proved bound of the distance between a surface and a rail
//! between two nodes (research 75 §4.3, solidcore's `span_bound`).

use scx_algo::bernstein::{Bernstein2, Bounded};
use scx_num::budget::{Budget, Stop};
use scx_num::certified::gamma;
use scx_num::units::{Accuracy, Length};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::{Approach, NurbsCurve};
use scx_nurbs_surface::{NurbsSurface, ParamAxis};

use super::{EPS, curve_spans, defect, forms, local_net, max_ratio, restricted};
use crate::error::SurfaceError;
use crate::nurbs_invert::{invert, patches, up};

/// The most times one segment of the correspondence is halved.
const MAX_HALVINGS: usize = 12;
/// The most segments [`span_bound`] bounds in all (research 75 R6). Past it no segment is
/// halved again; the bound stands.
const MAX_SEGMENTS: usize = 1 << 16;
/// The largest degree `p + q` of a patch's diagonal whose binomials `f64` holds exactly.
const MAX_DIAGONAL: usize = 52;

/// One end of a span: the rail's parameter there and the surface's parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    /// The rail's parameter.
    pub t: f64,
    /// The surface's parameters.
    pub uv: Point2,
}

/// What [`span_bound`] proves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpanBound {
    /// The distance between the surface and the rail along the correspondence is at most
    /// this, everywhere between the nodes.
    Bounded(Length),
    /// No bound holds: a piece's weights are not proved positive, or its coefficients are
    /// not finite, or a patch's degree is past what the diagonal reads exactly.
    Unbounded,
}

/// The proved bound of the distance between the surface `surface` and the rail `rail`
/// between the nodes `a` and `b` (research 75 §4.3).
///
/// **How.** The correspondence starts as the line between the nodes: the rail's parameter
/// and the surface's move together, linearly in `tau` on `[0, 1]`. Cut at the surface's
/// knot lines and the rail's knots, each piece has the surface's Bezier patch along a
/// segment of its square, which (restricted and turned so both parameters rise with
/// `tau`) is its diagonal `(tau, tau)`, a polynomial of degree `p + q` (Farouki and Rajan's
/// product rule), and the rail's span on its interval. In homogeneous form about the
/// rail's control points' centre, `S - R = (s r_w - r s_w) / (s_w r_w)`, each coordinate's
/// quotient bounded by its coefficients' (positive weights), and the distance by their
/// norm. A piece whose bound is above `tol` is halved at the rail's middle parameter, the
/// new node the rail's point there found on the surface (by inversion, not trusted: the
/// bound holds for any correspondence), at most 12 times; a piece whose rail point is itself
/// farther than `tol` from the surface keeps its bound (no correspondence brings it within).
/// Each segment is one unit of `budget` and polls `stop`; past `2^16` segments none is
/// halved again. Returns the largest bound over the pieces.
///
/// Solidcore returned an infinite bound past its segment limit; this returns the bound
/// reached, which is proved.
///
/// # Errors
///
/// [`SurfaceError::OutsideDomain`] for a node outside the surface's domain (`min` and `max`
/// the node's coordinate), [`SurfaceError::OffRail`] for one outside the rail's;
/// [`SurfaceError::Exhausted`] when `budget` runs out, [`SurfaceError::Stopped`] when `stop`
/// is set; [`SurfaceError::Defect`] where a Bernstein form cannot be built, and those of the
/// inversion.
pub fn span_bound(
    surface: &NurbsSurface<3>,
    rail: &NurbsCurve<3>,
    a: Node,
    b: Node,
    tol: Accuracy,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<SpanBound, SurfaceError> {
    let tol = tol.length().mm();
    for n in [a, b] {
        let (tl, th) = (rail.domain().min(), rail.domain().max());
        if !(tl <= n.t && n.t <= th) {
            return Err(SurfaceError::OffRail { at: n.t });
        }
        for (axis, x) in [(ParamAxis::U, n.uv.x), (ParamAxis::V, n.uv.y)] {
            let k = surface.knots(axis);
            let p = surface.degree(axis);
            let (lo, hi) = (
                k.get(p).copied().unwrap_or(f64::NAN),
                k.get(k.len().saturating_sub(p + 1))
                    .copied()
                    .unwrap_or(f64::NAN),
            );
            if !(lo <= x && x <= hi) {
                return Err(SurfaceError::OutsideDomain {
                    axis,
                    min: x,
                    max: x,
                });
            }
        }
    }
    // The coefficients about the centre of the rail's control points: the distance is a
    // length, so the shift rounds as one.
    let centre = centre(rail.points());
    let unit = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let mut pieces = Vec::new();
    for patch in patches(surface) {
        let net = local_net(&patch.net, centre, unit);
        pieces.push((patch.lo, patch.hi, forms(&net, patch.degrees)?));
    }
    let mut rails = Vec::new();
    for (s0, s1, net) in curve_spans(rail, centre, unit) {
        rails.push((s0, s1, forms(&net, (rail.degree(), 0))?));
    }
    let lines = |f: &dyn Fn(&(Point2, Point2, [Bounded; 4])) -> [f64; 2]| {
        let mut v: Vec<f64> = pieces.iter().flat_map(f).collect();
        v.sort_by(f64::total_cmp);
        v.dedup();
        v
    };
    let knots = [
        lines(&|p| [p.0.x, p.1.x]),
        lines(&|p| [p.0.y, p.1.y]),
        {
            let mut v: Vec<f64> = rails.iter().flat_map(|r| [r.0, r.1]).collect();
            v.sort_by(f64::total_cmp);
            v.dedup();
            v
        },
    ];
    let most = pieces
        .iter()
        .map(|p| p.2[3].f.degrees().0 + p.2[3].f.degrees().1)
        .max()
        .unwrap_or(0);
    if most > MAX_DIAGONAL {
        return Ok(SpanBound::Unbounded);
    }
    let binomials = pascal(most);
    let mut worst = 0.0_f64;
    let mut segments = 0_usize;
    let mut work = vec![Segment {
        uv0: a.uv,
        t0: a.t,
        uv1: b.uv,
        t1: b.t,
        depth: 0,
    }];
    while let Some(seg) = work.pop() {
        budget.charge(1).map_err(SurfaceError::Exhausted)?;
        stop.check().map_err(|_| SurfaceError::Stopped)?;
        segments += 1;
        if let Some(parts) = cut(seg, &knots) {
            work.extend(parts);
            continue;
        }
        let uv_mid = seg.uv0.midpoint(seg.uv1);
        let t_mid = 0.5 * (seg.t0 + seg.t1);
        let inside = |lo: f64, x: f64, hi: f64| lo <= x && x <= hi;
        let (Some(piece), Some(span)) = (
            pieces.iter().find(|(lo, hi, _)| {
                inside(lo.x, uv_mid.x, hi.x) && inside(lo.y, uv_mid.y, hi.y)
            }),
            rails.iter().find(|(s0, s1, _)| inside(*s0, t_mid, *s1)),
        ) else {
            // The nodes are inside both domains, and so is every segment between them.
            return Err(SurfaceError::Defect {
                reason: "a span's segment outside the surface's patches or the rail's spans",
            });
        };
        let bound = segment_bound(piece, span, seg, &binomials)?;
        let wide = bound.is_none_or(|b| b > tol);
        if wide && seg.depth < MAX_HALVINGS && segments < MAX_SEGMENTS {
            // The rail's middle found on the surface: the new node (the line's middle where
            // the foot is off the surface's domain).
            let q = rail
                .point(t_mid, Approach::Above)
                .map_err(|_| SurfaceError::Defect {
                    reason: "a rail's point inside its domain could not be evaluated",
                })?;
            let q = Point3::new(q[0], q[1], q[2]);
            let foot = invert(surface, q, Some(uv_mid), None, budget, stop)?.into_foot();
            if !(foot.distance <= tol) {
                // No correspondence brings this piece within the tolerance.
                match bound {
                    Some(b) => worst = worst.max(b),
                    None => return Ok(SpanBound::Unbounded),
                }
                continue;
            }
            let uv = if pieces.iter().any(|(lo, hi, _)| {
                inside(lo.x, foot.at.x, hi.x) && inside(lo.y, foot.at.y, hi.y)
            }) {
                foot.at
            } else {
                uv_mid
            };
            let depth = seg.depth + 1;
            work.push(Segment {
                uv0: uv,
                t0: t_mid,
                uv1: seg.uv1,
                t1: seg.t1,
                depth,
            });
            work.push(Segment {
                uv0: seg.uv0,
                t0: seg.t0,
                uv1: uv,
                t1: t_mid,
                depth,
            });
            continue;
        }
        match bound {
            Some(b) => worst = worst.max(b),
            None => return Ok(SpanBound::Unbounded),
        }
    }
    Ok(SpanBound::Bounded(Length::from_mm(worst)))
}

/// The centre of the box of `points`.
fn centre(points: &[[f64; 3]]) -> Point3 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in points {
        for k in 0..3 {
            if let (Some(l), Some(h), Some(&x)) = (lo.get_mut(k), hi.get_mut(k), p.get(k)) {
                *l = l.min(x);
                *h = h.max(x);
            }
        }
    }
    Point3::new(
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    )
}

/// A piece of the correspondence: its two nodes and how many times it was halved.
#[derive(Clone, Copy, Debug)]
struct Segment {
    uv0: Point2,
    t0: f64,
    uv1: Point2,
    t1: f64,
    depth: usize,
}

/// `seg` cut where it crosses a knot line (`knots`: the surface's `u` and `v` lines and the
/// rail's knots), the cut's crossing coordinate set to the knot itself (so a cut is not cut
/// again); `None` where it crosses none.
fn cut(seg: Segment, knots: &[Vec<f64>; 3]) -> Option<Vec<Segment>> {
    // each crossing: its fraction along the segment, which coordinate is the knot, the knot
    let mut cuts: Vec<(f64, usize, f64)> = Vec::new();
    let ends = [
        (seg.uv0.x, seg.uv1.x),
        (seg.uv0.y, seg.uv1.y),
        (seg.t0, seg.t1),
    ];
    for (which, ((x0, x1), ks)) in ends.iter().zip(knots).enumerate() {
        for &k in ks {
            if (*x0 < k && k < *x1) || (*x1 < k && k < *x0) {
                cuts.push(((k - x0) / (x1 - x0), which, k));
            }
        }
    }
    if cuts.is_empty() {
        return None;
    }
    cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out = Vec::new();
    let (mut uv, mut t) = (seg.uv0, seg.t0);
    for (f, which, knot) in cuts {
        let mut uv_at = Point2::new(
            seg.uv0.x + (seg.uv1.x - seg.uv0.x) * f,
            seg.uv0.y + (seg.uv1.y - seg.uv0.y) * f,
        );
        let mut t_at = seg.t0 + (seg.t1 - seg.t0) * f;
        match which {
            0 => uv_at.x = knot,
            1 => uv_at.y = knot,
            _ => t_at = knot,
        }
        out.push(Segment {
            uv0: uv,
            t0: t,
            uv1: uv_at,
            t1: t_at,
            depth: seg.depth,
        });
        (uv, t) = (uv_at, t_at);
    }
    out.push(Segment {
        uv0: uv,
        t0: t,
        uv1: seg.uv1,
        t1: seg.t1,
        depth: seg.depth,
    });
    Some(out)
}

/// Pascal's triangle to row `n`, exact in `f64` for `n <= 52`.
fn pascal(n: usize) -> Vec<Vec<f64>> {
    let mut rows: Vec<Vec<f64>> = vec![vec![1.0]];
    for i in 1..=n {
        let prev = rows.last().cloned().unwrap_or_default();
        let row = (0..=i)
            .map(|k| {
                let l = if k == 0 {
                    0.0
                } else {
                    prev.get(k - 1).copied().unwrap_or(0.0)
                };
                l + prev.get(k).copied().unwrap_or(0.0)
            })
            .collect();
        rows.push(row);
    }
    rows
}

fn binomial(b: &[Vec<f64>], n: usize, k: usize) -> f64 {
    b.get(n).and_then(|r| r.get(k)).copied().unwrap_or(f64::NAN)
}

/// `f` with its first parameter reversed (`ru`) and its second (`rv`).
fn reversed(f: &Bounded, ru: bool, rv: bool) -> Result<Bounded, SurfaceError> {
    if !(ru || rv) {
        return Ok(f.clone());
    }
    let (du, dv) = f.f.degrees();
    let c = f.f.coefficients();
    let mut out = Vec::with_capacity(c.len());
    for i in 0..=du {
        for j in 0..=dv {
            let (si, sj) = (if ru { du - i } else { i }, if rv { dv - j } else { j });
            out.push(c.get(si * (dv + 1) + sj).copied().unwrap_or(f64::NAN));
        }
    }
    Ok(Bounded {
        f: defect(Bernstein2::new(du, dv, out))?,
        err: f.err,
    })
}

/// The patch form `f` read along its diagonal `(s, s)`: degree `du + dv` in `s`, each
/// coefficient `c_k = sum_(i + j = k) b_ij C(du, i) C(dv, j) / C(du + dv, k)` (a convex
/// combination, so the carried error passes; the weights round twice each and the sum once
/// per term).
fn diagonal(f: &Bounded, b: &[Vec<f64>]) -> Result<Bounded, SurfaceError> {
    let (du, dv) = f.f.degrees();
    let n = du + dv;
    let c = f.f.coefficients();
    let mut out = vec![0.0; n + 1];
    for i in 0..=du {
        for j in 0..=dv {
            if let (Some(slot), Some(&x)) = (out.get_mut(i + j), c.get(i * (dv + 1) + j)) {
                *slot += x * (binomial(b, du, i) * binomial(b, dv, j) / binomial(b, n, i + j));
            }
        }
    }
    let err = up(f.err + gamma(n + 4) * (f.f.max_abs_coefficient() + f.err));
    Ok(Bounded {
        f: defect(Bernstein2::new(n, 0, out))?,
        err,
    })
}

/// The bound over one segment: the patch `piece` (its box in the surface's parameters and
/// its forms) and the rail's span `span` (its range and forms), or `None` where the
/// weights are not proved positive.
fn segment_bound(
    piece: &(Point2, Point2, [Bounded; 4]),
    span: &(f64, f64, [Bounded; 4]),
    seg: Segment,
    b: &[Vec<f64>],
) -> Result<Option<f64>, SurfaceError> {
    let (lo, hi, h) = piece;
    let local = |x: f64, lo: f64, hi: f64| ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    let (su0, su1) = (local(seg.uv0.x, lo.x, hi.x), local(seg.uv1.x, lo.x, hi.x));
    let (sv0, sv1) = (local(seg.uv0.y, lo.y, hi.y), local(seg.uv1.y, lo.y, hi.y));
    let mut s = Vec::with_capacity(4);
    for f in h {
        let f = restricted(f, (su0.min(su1), su0.max(su1)), (sv0.min(sv1), sv0.max(sv1)));
        s.push(diagonal(&reversed(&f, su0 > su1, sv0 > sv1)?, b)?);
    }
    let (s0, s1, rh) = span;
    let (r0, r1) = (local(seg.t0, *s0, *s1), local(seg.t1, *s0, *s1));
    let mut r = Vec::with_capacity(4);
    for f in rh {
        let f = restricted(f, (r0.min(r1), r0.max(r1)), (0.0, 1.0));
        r.push(reversed(&f, r0 > r1, false)?);
    }
    let (Some(sw), Some(rw)) = (s.get(3), r.get(3)) else {
        return Ok(None);
    };
    let den = defect(sw.mul(rw))?;
    let mut sum = 0.0_f64;
    for (sk, rk) in s.iter().zip(&r).take(3) {
        let num = defect(defect(sk.mul(rw))?.sub(&defect(rk.mul(sw))?))?;
        let Some(m) = max_ratio(&num, &den) else {
            return Ok(None);
        };
        sum += m * m;
    }
    let bound = up(sum.sqrt() * (1.0 + 4.0 * EPS));
    Ok(bound.is_finite().then_some(bound))
}
