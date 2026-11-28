// Written for solidcorex. Apache-2.0.
//! [`sheet_seams`]: where separate sheets meet, and how smoothly (research 74 §52, the pin's
//! `continuity::sheet_seams`).
//!
//! **A seam** is a run along which a boundary edge of one sheet (a curve edge with one fin) and
//! one of another sheet lie together within the caller's `zone`. The run's ends are ends of
//! the curves: at each end of the run one curve ends on the other. So each curve's end is
//! dropped on the other curve (`Curve::invert`, closed form on every analytic kind); an end
//! within the zone lands. Each curve's run is the span of its landing ends and the feet of the
//! other's landing ends. Fewer than two distinct landing ends, or a run of no length, is no
//! seam. Otherwise the run is *proved* when each curve over its run stays within the zone of the
//! other, certified both ways by `scx_surface::curve_curve_gap` (a one-sided Hausdorff bracket)
//! under the one budget; a run that is not proved is [`SheetSeams::unproved`]: the edges' ends
//! meet but they may part between them. That also covers a run that would wrap past a closed
//! curve's seam, which one parameter interval cannot hold.
//!
//! **Measured as an edge is.** The seam is then BW4's [`continuity`](crate::continuity) over the
//! first sheet's curve across the run, between the two sheets' faces: G0 to G4 proved in turn
//! along the whole run, the first order not proved named, and the drops measured at nine
//! stations. The second sheet's face is read on the side that agrees with the first across the
//! seam, as sewing the two would orient it: the two fins running opposite ways along the run
//! (the sense of the second curve against the first read from the tangents at the run's middle,
//! so a full circle needs no seam-point guess). The pin measured stations by the nearest point
//! of each surface to a guess; this proves, as `continuity` does.
//!
//! The sheets are read only. A NURBS edge curve is `Unsupported(Nurbs)` (its inversion is
//! pending) and so is a NURBS face. A sheet given twice meets itself along every edge.

use scx_brep::{Brep, Edge, EdgeId, EdgeShape, Entity};
use scx_curve::Curve;
use scx_num::budget::{Budget, Stop};
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::units::Length;
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_surface::{Aim, GapError, Short, curve_curve_gap};

use crate::along::{Edge as AlongEdge, Faces};
use crate::continuity::{Continuity, ContinuityTolerances, along, claims_of};
use crate::error::{BodyqDefect, BodyqError, BodyqRefusal, BodyqUnsupported};

/// Two sheets' boundary edges that meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetMeet {
    /// The two sheets, by their index in the list given (the first the lower).
    pub sheets: [usize; 2],
    /// Each sheet's edge.
    pub edges: [EdgeId; 2],
}

/// A seam between two sheets, proved over its run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetSeam {
    /// Which edges meet.
    pub meet: SheetMeet,
    /// The run on each edge's curve: `[low, high]` of its parameter, the first sheet's then the
    /// second's.
    pub along: [[f64; 2]; 2],
    /// Whether the second edge's curve runs the same way as the first's along the run.
    pub same_sense: bool,
    /// The order of contact proved along the run and the drops measured: the first sheet's
    /// curve over `along[0]`, between the two sheets' faces.
    pub continuity: Continuity,
}

/// Where separate sheets meet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetSeams {
    /// The seams proved, in order of the sheets, then of the first sheet's edges, then the
    /// second's.
    pub seams: Vec<SheetSeam>,
    /// Pairs of edges whose ends meet within the zone but whose run is not proved within it: they
    /// may part between those ends.
    pub unproved: Vec<SheetMeet>,
}

/// A boundary edge: its curve, interval, its one fin's face and whether the fin runs along the
/// curve.
struct Boundary<'a> {
    id: EdgeId,
    curve: &'a Curve,
    interval: Extent1d,
    face: scx_brep::FaceId,
    forward: bool,
}

fn boundary(body: &Brep) -> Vec<Boundary<'_>> {
    body.edges()
        .filter_map(|e: Edge<'_>| {
            let EdgeShape::Curve { curve, interval } = e.shape() else {
                return None;
            };
            let mut fins = e.fins();
            let (Some(fin), None) = (fins.next(), fins.next()) else {
                return None;
            };
            Some(Boundary {
                id: e.id(),
                curve,
                interval: *interval,
                face: fin.face,
                forward: fin.sense == Sense::Forward,
            })
        })
        .collect()
}

/// The nearest point of `b`'s curve over its interval to `p`: its parameter and distance. A foot
/// off the interval is replaced by the nearer end.
fn nearest(b: &Boundary<'_>, p: Point3) -> Result<(f64, f64), BodyqError> {
    let nurbs = || {
        BodyqError::Unsupported(BodyqUnsupported::Nurbs {
            at: Entity::Edge(b.id),
        })
    };
    let inv = b.curve.invert(p, Some(&b.interval)).map_err(|_| nurbs())?;
    if inv.is_inside() {
        let f = inv.foot();
        return Ok((f.at, f.distance));
    }
    let (lo, hi) = (b.interval.min(), b.interval.max());
    let (dl, dh) = (b.curve.point(lo).distance(p), b.curve.point(hi).distance(p));
    Ok(if dl <= dh { (lo, dl) } else { (hi, dh) })
}

/// The run of `a` and `b`: `[a_lo, a_hi]` and `[b_lo, b_hi]`, or `None` for no run.
fn run(
    a: &Boundary<'_>,
    b: &Boundary<'_>,
    zone: f64,
) -> Result<Option<[[f64; 2]; 2]>, BodyqError> {
    let mut on_a: Vec<f64> = Vec::new();
    let mut on_b: Vec<f64> = Vec::new();
    for (from, to, own, other) in [(a, b, &mut on_a, &mut on_b), (b, a, &mut on_b, &mut on_a)] {
        for t in [from.interval.min(), from.interval.max()] {
            let (at, d) = nearest(to, from.curve.point(t))?;
            if d <= zone {
                own.push(t);
                other.push(at);
            }
        }
    }
    let span = |ts: &[f64], i: &Extent1d| {
        let lo = ts.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let least = 1e-9 * (i.max() - i.min());
        (hi - lo > least).then_some([lo, hi])
    };
    Ok(span(&on_a, &a.interval).zip(span(&on_b, &b.interval)).map(|(x, y)| [x, y]))
}

/// Whether each curve over its run stays within `zone` of the other: `Some(true)` proved,
/// `Some(false)` not (refuted or open).
fn proved(
    a: &Boundary<'_>,
    b: &Boundary<'_>,
    [ra, rb]: [[f64; 2]; 2],
    zone: f64,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<bool, BodyqError> {
    let (Some(ia), Some(ib)) = (
        Extent1d::new(ra[0], ra[1]),
        Extent1d::new(rb[0], rb[1]),
    ) else {
        return Ok(false);
    };
    for (x, ix, y, iy) in [(a, &ia, b, &ib), (b, &ib, a, &ia)] {
        match curve_curve_gap(x.curve, ix, y.curve, iy, Aim::Against(zone), budget, stop) {
            Ok(g) if g.upper() <= zone => {}
            Ok(_) => return Ok(false),
            Err(GapError::Undecided { why, .. }) => match why {
                Short::Exhausted(e) => return Err(BodyqError::of_budget(e)),
                Short::Stopped => return Err(BodyqError::Cancelled),
                _ => return Ok(false),
            },
            Err(_) => return Err(BodyqError::Defect(BodyqDefect::Gap { edge: x.id })),
        }
    }
    Ok(true)
}

/// The seams where the boundary edges of `sheets` meet within `zone`, each measured.
///
/// Every pair of curve edges with one fin, one from each of two different sheets, is tried in
/// order (sheets, then the first's edges, then the second's), as the [module](self) says. One
/// `budget` covers the foot searches, the proofs of the runs and the measures.
///
/// # Errors
///
/// Refused for a `zone` that is not finite and above zero or a tolerance that is not finite or
/// is negative ([`Tolerance`](BodyqRefusal::Tolerance)); Unsupported for a NURBS edge curve or a
/// NURBS face of a seam; Exhausted or Cancelled.
pub fn sheet_seams(
    sheets: &[&Brep],
    zone: Length,
    tol: &ContinuityTolerances,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<SheetSeams, BodyqError> {
    let zone = zone.mm();
    if !(zone.is_finite() && zone > 0.0) {
        return Err(BodyqError::Refused(BodyqRefusal::Tolerance));
    }
    let claims = claims_of(tol)?;
    let edges: Vec<Vec<Boundary<'_>>> = sheets.iter().map(|s| boundary(s)).collect();
    let mut out = SheetSeams::default();
    for (i, (sa, ea)) in sheets.iter().zip(&edges).enumerate() {
        for (j, (sb, eb)) in sheets.iter().zip(&edges).enumerate().skip(i + 1) {
            for a in ea {
                for b in eb {
                    stop.check()?;
                    budget.charge(1)?;
                    let meet = SheetMeet {
                        sheets: [i, j],
                        edges: [a.id, b.id],
                    };
                    let Some(runs) = run(a, b, zone)? else {
                        continue;
                    };
                    if !proved(a, b, runs, zone, budget, stop)? {
                        out.unproved.push(meet);
                        continue;
                    }
                    let same_sense = same_sense(a, b, runs)?;
                    // Agreeing sides run the seam opposite ways; else the second sheet's
                    // face is read from its other side.
                    let agree = (a.forward == b.forward) != same_sense;
                    let turn = if agree { 1.0 } else { -1.0 };
                    let surface = |s: &Brep, f| {
                        s.face(f).map(scx_brep::Face::surface).ok_or(BodyqError::Refused(
                            BodyqRefusal::NoEntity {
                                entity: Entity::Face(f),
                            },
                        ))
                    };
                    let faces = Faces::between(
                        [(surface(sa, a.face)?, a.face), (surface(sb, b.face)?, b.face)],
                        turn,
                    )?;
                    let [lo, hi] = runs[0];
                    let e = AlongEdge::across(a.curve, lo, hi, faces);
                    let continuity = along(&e, &claims, budget, stop)?;
                    out.seams.push(SheetSeam {
                        meet,
                        along: runs,
                        same_sense,
                        continuity,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Whether `b`'s curve runs the way `a`'s does along the run: the tangents at the middle of
/// `a`'s run and at its foot on `b`.
fn same_sense(a: &Boundary<'_>, b: &Boundary<'_>, runs: [[f64; 2]; 2]) -> Result<bool, BodyqError> {
    let m = 0.5 * (runs[0][0] + runs[0][1]);
    let ja = a.curve.jet::<1>(m, Approach::Above);
    let (at, _) = nearest(b, ja.point)?;
    let jb = b.curve.jet::<1>(at, Approach::Above);
    let [da] = ja.d;
    let [db] = jb.d;
    Ok(da.dot(db) > 0.0)
}
