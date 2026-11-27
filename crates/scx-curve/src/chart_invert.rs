// Written for solidcorex. Apache-2.0.
//! The nearest point of an [`OnChart`] pcurve to a uv point (research 73 §21): a certified
//! search in the uv, as the NURBS surface's inversion is (scx-surface `nurbs_invert`).
//!
//! The foot is the nearest over the pcurve's span (`over`, met with a bounded 3-D curve's
//! domain): the span the lift was made and certified over. The search:
//!
//! - **Seeds.** [`SEEDS`] even pieces of the span; every piece's ends are evaluated, the
//!   best is the incumbent, polished by Newton.
//! - **Bounds.** Over a piece `B` the uv is enclosed by the centred form
//!   `uv(m) + uv'(B) (B - m)` met with the plain enclosure `uv(B)`, both [`Ivl`] through
//!   [`OnChart::jet_over`]. The distance from `p` to that box is a lower bound. A piece is
//!   dropped when its bound is not below the incumbent less the gap `eps`, or when
//!   `(uv - p) . uv'` keeps one sign over it (the distance is monotone there, so its least
//!   is at an end, and every end is evaluated).
//! - **Refinement.** The piece of least bound (then the earlier one: a total order) is
//!   halved at its middle, which is evaluated; a middle better than the incumbent by more
//!   than `eps` starts a Newton run there (a lesser gain is taken as it is, and the final
//!   incumbent is polished once if it was not). A piece with no `f64` inside is dropped (its ends
//!   are the only parameters it has).
//!
//! So the foot is the global nearest over the span's `f64` parameters to `2 eps`, where
//! `eps` is [`GAP_REL`] of the seeds' uv diagonal plus the first incumbent's distance: one
//! `eps` the search's gap, one the most the polish trades for a stationary parameter (near
//! the foot the distance is flat to the uv's rounding, so the parameter is set by the
//! derivative's sign, to rounding).
//!
//! The 3-D route (the 3-D curve's own inversion of the surface's point at `p`) is not
//! taken: a [`Chart`](crate::Chart) holds only what its inverse needs, not the surface's
//! point (a sphere's radius, a cone's angle), and the 3-D foot is the nearest in space,
//! which is the uv's only for a query on the curve. A caller with a 3-D point inverts the
//! 3-D edge.
//!
//! Each evaluation and each piece bounded costs one unit of the [`Budget`]; the [`Stop`]
//! is polled with each piece. Both end the search with an error, never a partial foot, and
//! neither chooses a path: a search that finishes gives the same bits under any limit.

use core::cmp::Ordering;
use std::collections::BinaryHeap;

use scx_num::budget::{Budget, Stop};
use scx_num::domain::ParamDomain;
use scx_num::extent1d::Extent1d;
use scx_num::interval::Ivl;
use scx_num::trig::Ranged;
use scx_num::vector2d::Point2;
use scx_nurbs::Approach;

use crate::chart::OnChart;
use crate::error::CurveError;
use crate::invert::{Foot, Inversion, POLISH_STEPS, place};

/// The gap the search closes to, relative to the seeds' uv diagonal plus the first
/// incumbent's distance (as scx-surface's NURBS inversion).
pub const GAP_REL: f64 = 1e-12;

/// The even pieces the search starts from.
const SEEDS: usize = 16;

/// How many times a Newton step is halved before the run ends.
const HALVINGS: u32 = 40;

/// A piece still to search: its bound, a sequence number (the order made), its ends.
#[derive(Debug, Clone, Copy)]
struct Piece {
    bound: f64,
    seq: u64,
    a: f64,
    b: f64,
}

impl PartialEq for Piece {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}

impl Eq for Piece {}

impl PartialOrd for Piece {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Piece {
    /// Reversed, so the heap's top is the least bound, then the least `seq`.
    fn cmp(&self, o: &Self) -> Ordering {
        o.bound
            .total_cmp(&self.bound)
            .then_with(|| o.seq.cmp(&self.seq))
    }
}

/// The search's state: the query, the span, the incumbent, the gap and the meters.
struct Search<'a> {
    pc: &'a OnChart,
    p: Point2,
    lo: f64,
    hi: f64,
    best: Foot<f64, Point2>,
    eps: f64,
    budget: &'a mut Budget,
}

impl OnChart {
    /// The nearest point of the pcurve's span to `p` in the uv, certified to a gap of
    /// [`GAP_REL`]; see the [module](crate::chart_invert) docs. A non-finite query gives a
    /// foot with NaN parts, as the analytic kinds' do.
    ///
    /// # Errors
    ///
    /// [`CurveError::Exhausted`] when `budget` runs out, [`CurveError::Stopped`] when `stop`
    /// is set; neither returns a partial foot.
    pub fn closest(
        &self,
        p: Point2,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Foot<f64, Point2>, CurveError> {
        let nan = f64::NAN;
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Ok(Foot {
                at: nan,
                point: Point2::new(nan, nan),
                distance: nan,
            });
        }
        let (lo, hi) = self.search_span();
        let ts: [f64; SEEDS + 1] = core::array::from_fn(|i| match i {
            0 => lo,
            i if i == SEEDS => hi,
            i => lo + (hi - lo) * (i as f64 / SEEDS as f64),
        });
        budget.charge(ts.len() as u64)?;
        let feet = ts.map(|t| self.foot(p, t));
        let mut best = feet[0];
        let (mut umin, mut umax) = (
            Point2::new(f64::MAX, f64::MAX),
            Point2::new(f64::MIN, f64::MIN),
        );
        for f in &feet {
            if f.distance < best.distance {
                best = *f;
            }
            umin = Point2::new(umin.x.min(f.point.x), umin.y.min(f.point.y));
            umax = Point2::new(umax.x.max(f.point.x), umax.y.max(f.point.y));
        }
        let eps = GAP_REL * ((umax - umin).length() + best.distance);
        let mut s = Search {
            pc: self,
            p,
            lo,
            hi,
            best,
            eps,
            budget,
        };
        s.polish(best.at)?;
        // The incumbent last polished: one taken from a middle by less than `eps` is polished
        // once at the end.
        let mut polished = s.best.at;
        let mut heap = BinaryHeap::new();
        let mut seq = 0_u64;
        for w in ts.windows(2) {
            if let &[a, b] = w {
                stop.check()?;
                if let Some(piece) = s.piece(a, b, eps, &mut seq)? {
                    heap.push(piece);
                }
            }
        }
        while let Some(piece) = heap.pop() {
            if piece.bound >= s.best.distance - eps {
                break;
            }
            stop.check()?;
            let Piece { a, b, .. } = piece;
            let m = 0.5 * (a + b);
            if !(a < m && m < b) {
                continue;
            }
            s.budget.charge(1)?;
            let f = self.foot(p, m);
            if f.distance < s.best.distance - eps {
                s.best = f;
                s.polish(m)?;
                polished = s.best.at;
            } else if f.distance < s.best.distance {
                s.best = f;
            }
            for (a, b) in [(a, m), (m, b)] {
                if let Some(piece) = s.piece(a, b, eps, &mut seq)? {
                    heap.push(piece);
                }
            }
        }
        if s.best.at != polished {
            s.polish(s.best.at)?;
        }
        Ok(s.best)
    }

    /// The nearest point to `p` ([`OnChart::closest`]), inside `domain` or not (`None`:
    /// the span).
    ///
    /// # Errors
    ///
    /// As [`OnChart::closest`].
    pub fn invert(
        &self,
        p: Point2,
        domain: Option<&Extent1d>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Inversion<f64, Point2>, CurveError> {
        Ok(place(self.closest(p, budget, stop)?, domain))
    }

    /// The span searched: `over`, met with a bounded 3-D curve's domain where they meet.
    fn search_span(&self) -> (f64, f64) {
        let (mut lo, mut hi) = (self.over().min(), self.over().max());
        if let ParamDomain::Bounded(d) = self.curve().domain() {
            let (a, b) = (lo.max(d.min()), hi.min(d.max()));
            if a < b {
                (lo, hi) = (a, b);
            }
        }
        (lo, hi)
    }

    fn foot(&self, p: Point2, t: f64) -> Foot<f64, Point2> {
        let point = self.point(t);
        Foot {
            at: t,
            point,
            distance: (point - p).length(),
        }
    }
}

impl Search<'_> {
    /// The piece `[a, b]` with its bound, or `None` where it is dropped (its bound not below
    /// the incumbent less `eps`, or the distance monotone over it). One unit.
    fn piece(
        &mut self,
        a: f64,
        b: f64,
        eps: f64,
        seq: &mut u64,
    ) -> Result<Option<Piece>, CurveError> {
        self.budget.charge(1)?;
        let bx = Ivl::new(a, b);
        let m = 0.5 * (a + b);
        let jet = self.pc.jet_over::<Ivl, 1>(bx, Approach::Above);
        let at_m = self.pc.point_over::<Ivl>(Ivl::from(m));
        let [d] = jet.d;
        let dt = bx - Ivl::from(m);
        let ex = meet(jet.point.x, at_m.x + d.x * dt) - Ivl::from(self.p.x);
        let ey = meet(jet.point.y, at_m.y + d.y * dt) - Ivl::from(self.p.y);
        let g = ex * d.x + ey * d.y;
        if g.lo > 0.0 || g.hi < 0.0 {
            return Ok(None);
        }
        let bound = (ex.sqr() + ey.sqr()).sqrt().lo;
        // A NaN bound (an undefined enclosure) is kept: it sorts last and is never dropped
        // by the comparison.
        if bound >= self.best.distance - eps {
            return Ok(None);
        }
        *seq += 1;
        Ok(Some(Piece {
            bound: if bound.is_nan() { 0.0 } else { bound },
            seq: *seq,
            a,
            b,
        }))
    }

    /// Newton on `|uv - p|^2 / 2` from `t`, kept to the span. A step is kept when it lowers
    /// the distance, or halves `|(uv - p) . uv'|` with the distance no more than `eps` above
    /// the run's start (near the foot the distance is flat to the uv's rounding, a few ulps
    /// of the coordinates, so only the derivative still tells the steps apart); else it is
    /// halved, up to [`HALVINGS`] times. Improves `best`; a run from `best` itself may
    /// replace it at up to `eps` more. One unit an evaluation.
    fn polish(&mut self, t: f64) -> Result<(), CurveError> {
        let from_best = self.best.at == t;
        self.budget.charge(1)?;
        let mut here = self.state(t);
        let limit = here.1.distance + self.eps;
        for _ in 0..POLISH_STEPS {
            let (t, foot, g, h) = here;
            if !(h > 0.0 && g.is_finite()) || g == 0.0 {
                break;
            }
            let mut step = -g / h;
            let mut moved = false;
            for _ in 0..HALVINGS {
                let next = (t + step).clamp(self.lo, self.hi);
                if next == t {
                    break;
                }
                self.budget.charge(1)?;
                let cand = self.state(next);
                if cand.1.distance <= limit
                    && (cand.1.distance < foot.distance || cand.2.abs() <= 0.5 * g.abs())
                {
                    (here, moved) = (cand, true);
                    break;
                }
                step *= 0.5;
            }
            if !moved {
                break;
            }
        }
        let foot = here.1;
        if foot.distance < self.best.distance || (from_best && foot.distance <= limit) {
            self.best = foot;
        }
        Ok(())
    }

    /// At `t` (two evaluations, charged as one): the foot, `g = (uv - p) . uv'` and Newton's divisor (`g'`, or `|uv'|^2`
    /// where `g'` is not positive).
    fn state(&self, t: f64) -> (f64, Foot<f64, Point2>, f64, f64) {
        // The foot from `point`, as every other foot, so its point is the curve's at `at`
        // to the bit (a jet's value can differ by an ulp).
        let foot = self.pc.foot(self.p, t);
        let [d1, d2] = self.pc.jet::<2>(t, Approach::Above).d;
        let e = foot.point - self.p;
        let speed = d1.dot(d1);
        let curv = speed + e.dot(d2);
        (t, foot, e.dot(d1), if curv > 0.0 { curv } else { speed })
    }
}

/// The common part of two enclosures of one value; the first where they miss (a NaN).
fn meet(a: Ivl, b: Ivl) -> Ivl {
    let (lo, hi) = (a.lo.max(b.lo), a.hi.min(b.hi));
    if lo <= hi { Ivl::new(lo, hi) } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::ChartKind;
    use crate::chart::tests::{chart, circle, frame, pairs, whole_turn};
    use crate::curve::Curve2;
    use crate::invert::Closest;
    use core::f64::consts::TAU;
    use scx_num::frame::Sense;

    fn run(pc: &OnChart, p: Point2) -> Foot<f64, Point2> {
        pc.closest(p, &mut Budget::unlimited(), &Stop::new())
            .unwrap()
    }

    /// The analytic pairs' pcurves, and the off-axis section `x = 1.2` of the sphere of
    /// radius 2 (azimuths within 0.15 turn of its start, so the start's constant reference
    /// lifts it continuously) in both senses.
    fn cases() -> Vec<OnChart> {
        let f = frame();
        let (x, y) = (*f.x(), *f.y());
        let mut out: Vec<OnChart> = pairs().into_iter().map(|(pc, _)| pc).collect();
        for sense in [Sense::Forward, Sense::Reversed] {
            let c = circle(f.origin() + x * 1.2, y, x, 1.6);
            out.push(whole_turn(c, chart(ChartKind::Sphere, sense)));
        }
        out
    }

    /// The nearest of `n + 1` even samples of the span.
    fn brute(pc: &OnChart, p: Point2, n: u32) -> f64 {
        let (lo, hi) = pc.search_span();
        (0..=n)
            .map(|i| {
                let t = lo + (hi - lo) * f64::from(i) / f64::from(n);
                (pc.point(t) - p).length()
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// A point on the curve inverts to its own parameter, at distance within rounding.
    #[test]
    fn points_on_the_curve_invert_to_their_parameter() {
        for pc in cases() {
            for i in 0..=40 {
                let t = TAU * f64::from(i) / 40.0;
                let p = pc.point(t);
                let f = run(&pc, p);
                assert!(f.distance <= 1e-12, "{t}: {}", f.distance);
                let back = pc.point(f.at);
                assert!((back - p).length() <= 1e-12);
                assert!((f.at - t).abs() <= 1e-7, "{t} -> {}", f.at);
            }
        }
    }

    /// Off the curve: the foot is no farther than a dense sampling's best, its point is the
    /// curve's at its parameter, and it is stationary or at an end.
    #[test]
    fn off_curve_feet_are_the_global_nearest() {
        for pc in cases() {
            let mid = pc.point(1.0);
            for (k, (du, dv)) in [
                (0.3, 0.2),
                (-0.7, 0.1),
                (0.05, -0.9),
                (2.0, 1.5),
                (-0.2, -0.05),
            ]
            .into_iter()
            .enumerate()
            {
                let p = Point2::new(mid.x + du, mid.y + dv);
                let f = run(&pc, p);
                let want = brute(&pc, p, 20_000);
                assert!(f.distance <= want + 1e-11, "{k}: {} > {want}", f.distance);
                assert_eq!(f.point, pc.point(f.at));
                assert_eq!(f.distance, (f.point - p).length());
                let (lo, hi) = pc.search_span();
                if lo < f.at && f.at < hi {
                    let j = pc.jet::<1>(f.at, Approach::Above);
                    let g = (j.point - p).dot(j.d[0]);
                    eprintln!(
                        "DBG {k} at={} d={} want={want} g={g} speed={} h={:?}",
                        f.at,
                        f.distance,
                        j.d[0].length(),
                        pc.jet::<2>(f.at, Approach::Above).d
                    );
                    assert!(g.abs() <= 1e-9 * (1.0 + j.d[0].length()), "{k}: g = {g}");
                }
            }
        }
    }

    /// The same query gives the same bits every time, and under any budget large enough.
    #[test]
    fn the_search_is_deterministic() {
        for pc in cases() {
            let p = pc.point(2.0) + scx_num::vector2d::Vec2::new(0.4, -0.3);
            let a = run(&pc, p);
            let mut used = Budget::unlimited();
            let b = pc.closest(p, &mut used, &Stop::new()).unwrap();
            assert_eq!(
                (a.at.to_bits(), a.distance.to_bits()),
                (b.at.to_bits(), b.distance.to_bits())
            );
            let mut exact = Budget::new(used.used());
            let c = pc.closest(p, &mut exact, &Stop::new()).unwrap();
            assert_eq!(c.at.to_bits(), a.at.to_bits());
            assert!(
                pc.closest(p, &mut Budget::new(used.used() - 1), &Stop::new())
                    .is_err()
            );
        }
    }

    /// The limits: a budget that runs out, a stop, a non-finite query, a foot outside a
    /// domain, and the enum's dispatch.
    #[test]
    fn limits_and_edges() {
        let pc = cases().pop().unwrap();
        let p = Point2::new(0.3, 0.4);
        let e = pc
            .closest(p, &mut Budget::new(5), &Stop::new())
            .unwrap_err();
        assert_eq!(
            e,
            CurveError::Exhausted(scx_num::budget::Exhausted { limit: 5 })
        );
        let stop = Stop::new();
        stop.set();
        assert_eq!(
            pc.closest(p, &mut Budget::unlimited(), &stop),
            Err(CurveError::Stopped)
        );
        let nan = run(&pc, Point2::new(f64::NAN, 0.0));
        assert!(nan.at.is_nan() && nan.distance.is_nan());
        let none = pc
            .invert(
                Point2::new(f64::INFINITY, 0.0),
                None,
                &mut Budget::unlimited(),
                &Stop::new(),
            )
            .unwrap();
        assert!(!none.is_inside());
        let q = pc.point(5.0);
        let inside = Extent1d::new(4.0, 6.0).unwrap();
        let outside = Extent1d::new(0.0, 1.0).unwrap();
        let mut b = Budget::unlimited();
        assert!(
            pc.invert(q, Some(&inside), &mut b, &Stop::new())
                .unwrap()
                .is_inside()
        );
        assert!(
            !pc.invert(q, Some(&outside), &mut b, &Stop::new())
                .unwrap()
                .is_inside()
        );
        let c2 = Curve2::from(pc.clone());
        let Ok(Closest::One(f)) = c2.closest(q, &mut Budget::unlimited(), &Stop::new()) else {
            panic!("one foot");
        };
        assert_eq!(f, run(&pc, q));
        // A span's ends: a query past the start inverts to the start.
        let start = pc.point(0.0);
        let d = pc.jet::<1>(0.0, Approach::Above).d[0];
        let q = start - d * 0.01;
        let f = run(&pc, q);
        assert!(f.distance <= (start - q).length(), "{}", f.distance);
    }
}
