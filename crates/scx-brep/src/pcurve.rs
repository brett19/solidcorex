// Written for solidcorex. Apache-2.0.
//! The exact pcurve writer (research 63 §6.5 BR5, §6.11): each use of a face gets the
//! closed-form pcurve [`pcurve_of`] gives its edge's curve on the face's surface, moved by
//! whole periods into the window its loop needs, and each collapsed edge at a pole the
//! pole's segment in uv. Where no closed form exists the writer refuses by name,
//! [`PcurveWriteError::NeedsFit`]: fitting is the fitted writer's (BR9), not this one's.
//!
//! What it writes is exact, so every use's `bound` is `None` (63 §6.2: exact kinds carry
//! no record and no proof). Exact here means within the body's resolution: `pcurve_of`'s
//! lifting check runs at the approximation tolerance, half the zone, so the check's
//! sampling (9 parameters; a sinusoidal miss between two of them is at most
//! `1 / cos(pi / 8)` of the larger sampled one) still leaves the pcurve within the zone
//! everywhere.
//!
//! The rules it keeps (07 §3 rule 5, 63 §6.7, the checks of [`finish`](crate::finish)):
//!
//! - **Periods.** A loop's first use with a curve keeps `pcurve_of`'s base window; each
//!   next one is moved by the whole periods that close the gap to the use before. What a
//!   whole period cannot close (beyond the two ends' budgets, taken to uv by the surface's
//!   speed) is refused, [`PcurveWriteError::PoleNeedsEdge`] where the joint is at a pole
//!   and [`PcurveWriteError::LoopOpen`] elsewhere. A loop that comes back a whole period
//!   away winds round the surface and needs a seam edge, [`PcurveWriteError::Winds`].
//! - **Poles.** A collapsed edge whose vertex is a pole of the surface (a sphere's two, a
//!   cone's apex) gets the segment along `u` at the pole's `v` from where the loop
//!   arrives to where it leaves, over `[0, 1]`, in the direction that keeps the face on
//!   the left: toward `-u` when the face lies below the pole's `v`, `+u` above. Its length
//!   is moved by whole periods into `(0, period]`, a zero gap being a whole turn. A pcurve
//!   never runs along a pole's line itself (no dog-leg) and never passes a pole inside a
//!   use (`pcurve_of` refuses a meridian across one, [`PcurveWriteError::CrossesPole`]).
//!   A collapsed edge elsewhere is the point where the loop stands.
//! - **Inner loops.** Each loop after the first is moved by whole periods so its centre is
//!   nearest the first loop's.

use core::fmt;

use scx_curve::{AngleAxis, AngleLine2, AngleWave2, Curve2, Line2};
use scx_num::domain::UvDomain;
use scx_num::error::ErrorKind;
use scx_num::extent1d::Extent1d;
use scx_num::frame::Sense;
use scx_num::units::Resolution;
use scx_num::vector2d::{Point2, Vec2};
use scx_num::vector3d::Point3;
use scx_nurbs::Approach;
use scx_surface::{PcurveError, Pole, Surface, pcurve_of};

use crate::document::Document;
use crate::draft::Draft;
use crate::error::Absent;
use crate::id::{EdgeId, FaceId};
use crate::record::{EdgeShape, EdgeUse, LoopRecord, UseGeom};

/// Why the writer gave no pcurves. Nothing is written when it errs.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PcurveWriteError {
    /// The draft holds no such face or edge.
    Absent(Absent),
    /// The edge's curve is not attached yet.
    Pending {
        /// The edge.
        edge: EdgeId,
    },
    /// The edge's curve lies on the surface but has no exact pcurve there (no closed form
    /// for the pair, or a near configuration the closed form misses, or a curve within
    /// the edge's reach of the surface but not within the approximation tolerance): it
    /// needs a fitted one (BR9).
    NeedsFit {
        /// The edge.
        edge: EdgeId,
    },
    /// The edge's curve leaves the surface: at its parameter `at`, the farthest sample, it is
    /// `miss` (mm) from it, beyond the edge's reach.
    OffSurface {
        /// The edge.
        edge: EdgeId,
        /// The curve's parameter.
        at: f64,
        /// The distance from the surface, in mm.
        miss: f64,
    },
    /// A sphere's meridian passes a pole at `at` inside the edge: split the edge there.
    CrossesPole {
        /// The edge.
        edge: EdgeId,
        /// The curve's parameter at the pole.
        at: f64,
    },
    /// `pcurve_of` refused the edge's arguments (an interval of no length).
    Pcurve {
        /// The edge.
        edge: EdgeId,
        /// Its cause.
        cause: PcurveError,
    },
    /// Use `at` of loop `lp` does not start where the use before it ends, by `gap` in uv,
    /// and no whole period closes it.
    LoopOpen {
        /// The loop.
        lp: usize,
        /// The use.
        at: usize,
        /// The gap left, in uv (the larger coordinate).
        gap: f64,
    },
    /// Use `at` of loop `lp` meets the use before it at a pole across the pole's line: the
    /// loop needs a collapsed edge there (a pole is a vertex with a collapsed edge).
    PoleNeedsEdge {
        /// The loop.
        lp: usize,
        /// The use.
        at: usize,
    },
    /// Loop `lp` comes back a whole period from where it starts: it winds round the
    /// surface and needs a seam edge.
    Winds {
        /// The loop.
        lp: usize,
    },
    /// Loop `lp` has no use with a curve, so nothing places it in uv.
    NoCurveUse {
        /// The loop.
        lp: usize,
    },
    /// Use `at` of loop `lp` is a collapsed edge after another one: the writer gives one
    /// pole segment per joint.
    CollapsedRun {
        /// The loop.
        lp: usize,
        /// The use.
        at: usize,
    },
}

impl PcurveWriteError {
    /// [`ErrorKind::Unsupported`] for what a fitted or a further writer would give
    /// ([`Self::NeedsFit`], [`Self::NoCurveUse`], [`Self::CollapsedRun`]), and for a
    /// pcurve cause of that kind; [`ErrorKind::Refused`] for the rest, which the caller
    /// fixes in the topology (split, add a seam or a pole edge) or the geometry.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NeedsFit { .. } | Self::NoCurveUse { .. } | Self::CollapsedRun { .. } => {
                ErrorKind::Unsupported
            }
            Self::Pcurve { cause, .. } => cause.kind(),
            Self::Absent(_)
            | Self::Pending { .. }
            | Self::OffSurface { .. }
            | Self::CrossesPole { .. }
            | Self::LoopOpen { .. }
            | Self::PoleNeedsEdge { .. }
            | Self::Winds { .. } => ErrorKind::Refused,
        }
    }

    /// The cause's stable name (62 D6).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Absent(a) => a.name(),
            Self::Pending { .. } => "pcurve_edge_pending",
            Self::NeedsFit { .. } => "pcurve_needs_fit",
            Self::OffSurface { .. } => "pcurve_off_surface",
            Self::CrossesPole { .. } => "pcurve_crosses_pole",
            Self::Pcurve { cause, .. } => cause.name(),
            Self::LoopOpen { .. } => "pcurve_loop_open",
            Self::PoleNeedsEdge { .. } => "pcurve_pole_needs_edge",
            Self::Winds { .. } => "pcurve_loop_winds",
            Self::NoCurveUse { .. } => "pcurve_no_curve_use",
            Self::CollapsedRun { .. } => "pcurve_collapsed_run",
        }
    }
}

impl fmt::Display for PcurveWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Absent(a) => write!(f, "{a}"),
            Self::Pending { edge } => write!(f, "edge {edge} has no curve yet"),
            Self::NeedsFit { edge } => {
                write!(f, "edge {edge} has no exact pcurve: it needs a fitted one")
            }
            Self::OffSurface { edge, at, miss } => write!(
                f,
                "edge {edge} leaves the surface: {miss} mm from it at {at}"
            ),
            Self::CrossesPole { edge, at } => {
                write!(f, "edge {edge} crosses a pole at {at}: split it there")
            }
            Self::Pcurve { edge, cause } => write!(f, "edge {edge}: {cause}"),
            Self::LoopOpen { lp, at, gap } => write!(
                f,
                "loop {lp}: use {at} starts {gap} in uv from where the use before ends"
            ),
            Self::PoleNeedsEdge { lp, at } => write!(
                f,
                "loop {lp}: use {at} meets the use before across a pole: add a collapsed edge"
            ),
            Self::Winds { lp } => {
                write!(f, "loop {lp} winds round the surface: add a seam edge")
            }
            Self::NoCurveUse { lp } => write!(f, "loop {lp} has no use with a curve"),
            Self::CollapsedRun { lp, at } => {
                write!(f, "loop {lp}: use {at} is a second collapsed edge in a row")
            }
        }
    }
}

impl core::error::Error for PcurveWriteError {}

impl From<Absent> for PcurveWriteError {
    fn from(a: Absent) -> Self {
        Self::Absent(a)
    }
}

impl Draft {
    /// The loop records of a face on `surface` whose loops run through `loops`' edges in
    /// their senses (the first loop the outer one), every use given its exact pcurve as
    /// the [module](self) says, every `bound` `None`. For [`Draft::add_face`].
    ///
    /// # Errors
    ///
    /// [`PcurveWriteError`]: an edge the draft does not hold or whose curve is pending, an
    /// edge with no exact pcurve ([`PcurveWriteError::NeedsFit`], Unsupported) or off the
    /// surface, and a loop the rules cannot place in uv.
    pub fn exact_loops(
        &self,
        surface: &Surface,
        loops: &[Vec<(EdgeId, Sense)>],
        doc: &Document,
    ) -> Result<Vec<LoopRecord>, PcurveWriteError> {
        let res = doc.resolution();
        let out = loops
            .iter()
            .enumerate()
            .map(|(lp, uses)| self.loop_uses(surface, lp, uses, res))
            .collect::<Result<Vec<_>, _>>()?;
        placed(surface, out)
    }

    /// One loop of a face on `surface` through `uses`' edges in their senses, written
    /// alone: as [`Draft::exact_loops`] writes it as a face's only loop, with the centre
    /// [`placed`] needs to set it among the face's other loops. So `exact_loops` of some
    /// loops is `placed` of each one's `exact_loop`, and a caller that needs each loop
    /// alone first (to sort loops into faces) writes each once.
    ///
    /// # Errors
    ///
    /// As [`Draft::exact_loops`] for one loop.
    pub fn exact_loop(
        &self,
        surface: &Surface,
        uses: &[(EdgeId, Sense)],
        doc: &Document,
    ) -> Result<ExactLoop, PcurveWriteError> {
        self.loop_uses(surface, 0, uses, doc.resolution())
    }

    /// Writes every use of `face` with its exact pcurve from its edge, as
    /// [`Draft::exact_loops`] gives for the face's own surface, edges and senses; the
    /// uses' edges and senses, and so the fins, stay as they are. All or nothing.
    ///
    /// # Errors
    ///
    /// [`PcurveWriteError::Absent`] for a face the draft does not hold; otherwise as
    /// [`Draft::exact_loops`]. The face is unchanged on error.
    pub fn write_pcurves(&mut self, face: FaceId, doc: &Document) -> Result<(), PcurveWriteError> {
        let view = self.face(face).ok_or_else(|| Absent {
            entity: face.into(),
        })?;
        let surface = view.surface().clone();
        let loops: Vec<Vec<(EdgeId, Sense)>> = view
            .record()
            .loops
            .iter()
            .map(|l| l.uses.iter().map(|u| (u.edge, u.sense)).collect())
            .collect();
        let written = self.exact_loops(&surface, &loops, doc)?;
        let rec = self.face_mut(face).ok_or_else(|| Absent {
            entity: face.into(),
        })?;
        for (l, w) in rec.loops.iter_mut().zip(written) {
            for (u, w) in l.uses.iter_mut().zip(w.uses) {
                u.geom = w.geom;
                u.bound = None;
            }
        }
        Ok(())
    }

    /// Loop `lp`'s uses, and the centre of their starts in uv.
    fn loop_uses(
        &self,
        surface: &Surface,
        lp: usize,
        uses: &[(EdgeId, Sense)],
        res: Resolution,
    ) -> Result<ExactLoop, PcurveWriteError> {
        let zone = res.zone().mm();
        let domain = surface.domain();
        let periods = [domain.u.period(), domain.v.period()];
        let items = uses
            .iter()
            .map(|&(edge, sense)| self.item(surface, edge, sense, res))
            .collect::<Result<Vec<_>, _>>()?;
        let n = items.len();
        let first = items
            .iter()
            .position(|i| matches!(i.kind, Kind::Curve { .. }))
            .ok_or(PcurveWriteError::NoCurveUse { lp })?;
        let mut placed: Vec<Option<(UseGeom, [Point2; 2])>> = vec![None; n];
        // Where the loop stands, the budget of the end that stands there, and the `v` of
        // the last pcurve's middle (which side of a pole the face lies on).
        let mut at: Option<(Point2, f64)> = None;
        let mut side_v = 0.0;
        for j in 0..n {
            let k = (first + j) % n;
            let Some(item) = items.get(k) else { continue };
            let geom = match &item.kind {
                Kind::Curve { pc, interval } => {
                    let mut pc = pc.clone();
                    let [start, _] = use_ends(&pc, *interval, item.sense);
                    if let Some((end, budget)) = at {
                        let m = whole(end - start, periods);
                        if m != [0.0, 0.0] {
                            pc = shifted(&pc, m, periods)
                                .ok_or(PcurveWriteError::NeedsFit { edge: item.edge })?;
                        }
                        let [start, _] = use_ends(&pc, *interval, item.sense);
                        joint(surface, end, start, budget + item.start_budget, lp, k)?;
                    }
                    let ends = use_ends(&pc, *interval, item.sense);
                    side_v = pc.point(0.5 * (interval.min() + interval.max())).y;
                    at = Some((ends[1], item.end_budget));
                    (
                        UseGeom::Pcurve {
                            curve: pc,
                            interval: *interval,
                        },
                        ends,
                    )
                }
                Kind::Collapsed { pole } => {
                    let Some((start, _)) = at else {
                        return Err(PcurveWriteError::NoCurveUse { lp });
                    };
                    let next = (k + 1) % n;
                    let target = match (items.get(next).map(|i| &i.kind), placed.get(next)) {
                        (_, Some(Some((_, [s, _])))) => *s,
                        (Some(Kind::Curve { pc, interval }), _) => {
                            let nx = items.get(next).map_or(Sense::Forward, |i| i.sense);
                            use_ends(pc, *interval, nx)[0]
                        }
                        _ => return Err(PcurveWriteError::CollapsedRun { lp, at: next }),
                    };
                    // The loop then stands at this use's end, with its budget, as `finish`
                    // measures the next joint.
                    if let (Some(p), Some(period)) = (pole, periods[0]) {
                        let sign = if side_v < p.v { -1.0 } else { 1.0 };
                        let du = toward(target.x - start.x, period, sign);
                        let end = start + Vec2::new(du, 0.0);
                        at = Some((end, item.end_budget));
                        (
                            pole_segment(start, end, item.sense, item.edge)?,
                            [start, end],
                        )
                    } else {
                        at = Some((start, item.end_budget));
                        (UseGeom::Point(start), [start, start])
                    }
                }
            };
            if let Some(slot) = placed.get_mut(k) {
                *slot = Some(geom);
            }
        }
        // The loop closes: the last end back at the first start, within no whole period.
        let first_start = placed
            .get(first)
            .and_then(|p| p.as_ref())
            .map(|(_, [s, _])| *s);
        if let (Some((end, budget)), Some(start)) = (at, first_start) {
            if whole(end - start, periods) != [0.0, 0.0] {
                return Err(PcurveWriteError::Winds { lp });
            }
            let b = items.get(first).map_or(zone, |i| i.start_budget);
            joint(surface, end, start, budget + b, lp, first)?;
        }
        let mut sum = Vec2::ZERO;
        let mut out = Vec::with_capacity(n);
        for (item, p) in items.iter().zip(placed) {
            let Some((geom, [s, _])) = p else { continue };
            sum += Vec2::new(s.x, s.y);
            out.push(EdgeUse::new(item.edge, item.sense, geom));
        }
        let centre = Point2::new(sum.x / n as f64, sum.y / n as f64);
        Ok(ExactLoop {
            record: LoopRecord::new(out),
            centre,
        })
    }

    /// One use's edge, resolved: its raw pcurve or its pole, and its ends' budgets.
    fn item(
        &self,
        surface: &Surface,
        edge: EdgeId,
        sense: Sense,
        res: Resolution,
    ) -> Result<Item, PcurveWriteError> {
        let zone = res.zone().mm();
        let reach =
            |r: Option<&scx_num::cert::Recorded>| r.map_or(zone, |r| r.acc.length().mm().max(zone));
        let view = self.edge(edge).ok_or_else(|| Absent {
            entity: edge.into(),
        })?;
        let shape = view.shape().ok_or(PcurveWriteError::Pending { edge })?;
        let re = reach(view.accuracy());
        let vertex = |v: Option<crate::view::Vertex<'_, crate::view::Drafting<'_>>>| {
            v.map(|v| (v.point(), reach(v.accuracy())))
        };
        let ends = match sense {
            Sense::Forward => [vertex(view.start()), vertex(view.end())],
            Sense::Reversed => [vertex(view.end()), vertex(view.start())],
        };
        // The use's own bound is `None`: its reach is the zone.
        let budget = |e: &Option<(Point3, f64)>| zone + re + e.map_or(0.0, |(_, rv)| rv);
        let (start_budget, end_budget) = (budget(&ends[0]), budget(&ends[1]));
        let kind = match shape {
            EdgeShape::Curve { curve, interval } => {
                match pcurve_of(curve, surface, *interval, res) {
                    Ok(pc) => Kind::Curve {
                        pc,
                        interval: *interval,
                    },
                    Err(PcurveError::NoClosedForm { .. } | PcurveError::Inexact { .. }) => {
                        return Err(PcurveWriteError::NeedsFit { edge });
                    }
                    Err(PcurveError::NotOnSurface { miss, .. }) if miss <= re => {
                        return Err(PcurveWriteError::NeedsFit { edge });
                    }
                    Err(PcurveError::NotOnSurface { at, miss }) => {
                        return Err(PcurveWriteError::OffSurface { edge, at, miss });
                    }
                    Err(PcurveError::CrossesPole { at }) => {
                        return Err(PcurveWriteError::CrossesPole { edge, at });
                    }
                    Err(cause) => return Err(PcurveWriteError::Pcurve { edge, cause }),
                }
            }
            EdgeShape::Collapsed => {
                let pole = ends[0].and_then(|(p, rv)| {
                    poles(surface)
                        .into_iter()
                        .flatten()
                        .find(|pole| (pole.point - p).length() <= zone + re + rv)
                });
                Kind::Collapsed { pole }
            }
        };
        Ok(Item {
            edge,
            sense,
            kind,
            start_budget,
            end_budget,
        })
    }
}

/// A use as the writer resolved it.
struct Item {
    edge: EdgeId,
    sense: Sense,
    kind: Kind,
    /// The 3-D budget of the use's start and of its end: its own reach (the zone), its
    /// edge's and its vertex's.
    start_budget: f64,
    end_budget: f64,
}

enum Kind {
    /// An edge with a curve: its pcurve as `pcurve_of` gives it, in the base window.
    Curve { pc: Curve2, interval: Extent1d },
    /// A collapsed edge, at `pole` when its vertex is one of the surface's.
    Collapsed { pole: Option<Pole> },
}

/// The surface's poles: a sphere's two, a cone's apex.
fn poles(surface: &Surface) -> [Option<Pole>; 2] {
    match surface {
        Surface::Sphere(s) => s.poles().map(Some),
        Surface::Cone(c) => [Some(c.pole()), None],
        _ => [None, None],
    }
}

/// Where a use in `sense` over `interval` of `pc` starts and ends.
fn use_ends(pc: &Curve2, interval: Extent1d, sense: Sense) -> [Point2; 2] {
    let (a, b) = (pc.point(interval.min()), pc.point(interval.max()));
    match sense {
        Sense::Forward => [a, b],
        Sense::Reversed => [b, a],
    }
}

/// The whole periods nearest `gap`, per direction; 0 where a direction is not periodic.
fn whole(gap: Vec2, periods: [Option<f64>; 2]) -> [f64; 2] {
    let w = |g: f64, p: Option<f64>| p.map_or(0.0, |p| (g / p).round());
    [w(gap.x, periods[0]), w(gap.y, periods[1])]
}

/// `pc` moved by `m[i]` whole periods along u and v; `None` for a kind that has no exact
/// move here (one `pcurve_of` gives only on a plane, which has no period).
fn shifted(pc: &Curve2, m: [f64; 2], periods: [Option<f64>; 2]) -> Option<Curve2> {
    let tau = core::f64::consts::TAU;
    let d = Vec2::new(
        m[0] * periods[0].unwrap_or(0.0),
        m[1] * periods[1].unwrap_or(0.0),
    );
    match pc {
        Curve2::Line(l) => Line2::new(l.origin() + d, l.direction())
            .ok()
            .map(Curve2::Line),
        Curve2::AngleLine(a) => {
            // The angle moves by whole turns, the other coordinate by the period itself.
            let (turns, other) = match a.axis() {
                AngleAxis::U => (m[0], d.y),
                AngleAxis::V => (m[1], d.x),
            };
            AngleLine2::new(
                a.arcs(),
                a.axis(),
                a.start() + turns * tau,
                a.sense(),
                a.other() + other,
                a.rate(),
            )
            .ok()
            .map(Curve2::AngleLine)
        }
        Curve2::AngleWave(w) => {
            let (c, s) = w.amplitudes();
            AngleWave2::new(
                w.arcs(),
                w.start() + m[0] * tau,
                w.sense(),
                w.mean() + d.y,
                c,
                s,
            )
            .ok()
            .map(Curve2::AngleWave)
        }
        // The chart's periods are the surface's.
        Curve2::OnChart(o) => o.shifted(m).ok().map(Curve2::from),
        _ => None,
    }
}

/// The joint from `end` to `start` holds within `budget` (3-D) taken to uv by the
/// surface's larger speed at `end`, as `finish` checks it; otherwise the use `at` of loop
/// `lp` is refused, across a pole's line when `end` lifts to a pole.
fn joint(
    surface: &Surface,
    end: Point2,
    start: Point2,
    budget: f64,
    lp: usize,
    at: usize,
) -> Result<(), PcurveWriteError> {
    let gap = (end.x - start.x).abs().max((end.y - start.y).abs());
    if gap == 0.0 {
        return Ok(());
    }
    let jet = surface.jet::<1>(end, [Approach::Above; 2]);
    let len = |d: Option<scx_num::vector3d::Vec3>| d.map_or(0.0, scx_num::vector3d::Vec3::length);
    let speed = len(jet.get(1, 0)).max(len(jet.get(0, 1)));
    let allowed = if speed > 0.0 { budget / speed } else { 0.0 };
    if gap <= allowed {
        return Ok(());
    }
    let p = surface.point(end);
    let at_pole = poles(surface)
        .into_iter()
        .flatten()
        .any(|pole| (pole.point - p).length() <= budget);
    Err(if at_pole {
        PcurveWriteError::PoleNeedsEdge { lp, at }
    } else {
        PcurveWriteError::LoopOpen { lp, at, gap }
    })
}

/// `gap` moved by whole `period`s to the sign of `sign`, in `(0, period]` in size: the
/// length of a pole segment.
fn toward(gap: f64, period: f64, sign: f64) -> f64 {
    let g = gap * sign;
    let mut m = g - period * (g / period).floor();
    // A gap a rounding short of a whole period is a whole period; one a rounding past zero
    // is zero, and a segment of zero is the whole turn.
    if m > period * (1.0 - 1e-12) || m < period * 1e-12 {
        m = period;
    }
    m * sign
}

/// The pole segment from `start` to `end` in the use's direction, over `[0, 1]`.
fn pole_segment(
    start: Point2,
    end: Point2,
    sense: Sense,
    edge: EdgeId,
) -> Result<UseGeom, PcurveWriteError> {
    let (origin, dir) = match sense {
        Sense::Forward => (start, end - start),
        Sense::Reversed => (end, start - end),
    };
    let line = Line2::new(origin, dir).map_err(|_| PcurveWriteError::NeedsFit { edge })?;
    let interval = Extent1d::new(0.0, 1.0).ok_or(PcurveWriteError::NeedsFit { edge })?;
    Ok(UseGeom::Pcurve {
        curve: Curve2::Line(line),
        interval,
    })
}

/// One loop's uses with their exact pcurves, written alone by [`Draft::exact_loop`], and
/// the centre of their starts in uv.
#[derive(Debug, Clone, PartialEq)]
pub struct ExactLoop {
    record: LoopRecord,
    centre: Point2,
}

impl ExactLoop {
    /// The loop as written alone: a face's only loop.
    #[must_use]
    pub const fn record(&self) -> &LoopRecord {
        &self.record
    }

    /// The loop as written alone, owned.
    #[must_use]
    pub fn into_record(self) -> LoopRecord {
        self.record
    }
}

/// The loop records of a face on `surface` whose loops, each written alone, are `loops`
/// (the first the outer one): each loop after the first moved by whole periods so its
/// centre is nearest the first's, as [`Draft::exact_loops`] places them.
///
/// # Errors
///
/// [`PcurveWriteError::NeedsFit`] for a use whose pcurve cannot be moved by a period.
pub fn placed(
    surface: &Surface,
    mut loops: Vec<ExactLoop>,
) -> Result<Vec<LoopRecord>, PcurveWriteError> {
    nearest_first(&mut loops, &surface.domain())?;
    Ok(loops.into_iter().map(ExactLoop::into_record).collect())
}

/// Moves each loop after the first by whole periods so its centre is nearest the first's.
fn nearest_first(loops: &mut [ExactLoop], domain: &UvDomain) -> Result<(), PcurveWriteError> {
    let periods = [domain.u.period(), domain.v.period()];
    let Some(c0) = loops.first().map(|l| l.centre) else {
        return Ok(());
    };
    for ExactLoop {
        record: LoopRecord { uses },
        centre: c,
    } in loops.iter_mut().skip(1)
    {
        let m = whole(c0 - *c, periods);
        if m == [0.0, 0.0] {
            continue;
        }
        let d = Vec2::new(
            m[0] * periods[0].unwrap_or(0.0),
            m[1] * periods[1].unwrap_or(0.0),
        );
        for u in uses.iter_mut() {
            u.geom = match &u.geom {
                UseGeom::Point(p) => UseGeom::Point(*p + d),
                UseGeom::Pcurve { curve, interval } => UseGeom::Pcurve {
                    curve: shifted(curve, m, periods)
                        .ok_or(PcurveWriteError::NeedsFit { edge: u.edge })?,
                    interval: *interval,
                },
            };
        }
        *c += d;
    }
    Ok(())
}
