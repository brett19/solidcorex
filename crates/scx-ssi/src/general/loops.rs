// Written for solidcorex. Apache-2.0.
//! Loop-free pairs, critical points and seeds (research 73 §4.4, §4.6, row SM5).
//!
//! For each pair of pieces that [`Candidates`] could not prove apart, [`Survey::examine`]
//! gives a [`Plan`]: a proof that the pair holds no closed loop of the intersection, or the
//! classified points that any loop in it must surround, and in either case the **seeds**,
//! the proved points where the intersection crosses the pieces' edges. Every curve of the
//! intersection in the pair then has a seed: an arc ends on an edge, and a loop crosses the
//! ray from a point it surrounds. Row SM7 marches from them.
//!
//! # The proofs
//!
//! With `N_A = S_u x S_v` over a piece of the first surface and `N_B` over one of the second,
//! each enclosed over its uv box ([`Patch`]: interval arithmetic on the piece's own formula):
//!
//! 1. **Sederberg-Meyers** ([`Loops::Monotone`]). With `P` the cross product of the normal
//!    boxes' middle directions, when `P . (N_A x N_B)` excludes 0 over the boxes, the tangent
//!    of every intersection curve (`N_A x N_B`) has a component along `P` of one sign, so
//!    `P . x` is strictly monotone along each curve: no closed loop, and no point where the
//!    normals are collinear (so no touch, node or tangent curve).
//! 2. **Graphs with no critical point** ([`Loops::NoCritical`]). Let `e` be the bisector of
//!    the two normal cones' axes and `π` a projection along `e`. When the 2 x 2 interval
//!    matrix `π [S_u S_v]` is regular over each piece (each a graph over the plane, `π . S`
//!    injective: the segment between two uv points maps by a matrix in that box), a closed
//!    loop in the pair projects to a Jordan curve whose inside both graphs cover. The height
//!    difference `h = k . (S_A - S_B)` along `k = e1 x e2` is 0 on it, so it has an interior
//!    extremum, where the tangent planes are parallel. Those points are the roots of
//!
//!    ```text
//!    e1 . (S_A(u, v) - S_B(s, t)) = 0,   e2 . (S_A - S_B) = 0,
//!    N_A . S_B,s = 0,                    N_A . S_B,t = 0,
//!    ```
//!
//!    solved over the two pieces by [`solve::roots`] (each root proved unique by Krawczyk). The
//!    normal cones (axis and a lower bound of the half-angle's cosine, by outward rounding,
//!    no `acos`) prove there are none when they are disjoint, without the solve.
//! 3. **Classified critical points** ([`Loops::Critical`]). Each root is classified by the
//!    gap `|S_A - S_B|` and the relative form `Q = II_A - C^T II_B C` (both along `N_A`, in
//!    the first surface's uv; `C` the map from it to the second's), whose definiteness is the
//!    Hessian's of `h`: [`CriticalKind`]. A loop surrounds a maximum of `h` above 0 or a
//!    minimum below it, so only those ([`CriticalKind::Extremum`] with `crossing`, and
//!    [`CriticalKind::Degenerate`]) are given a ray: the line of constant `v` from the point to
//!    the piece's edge, which every loop around the point crosses. Its roots are seeds.
//!
//! A pair none of these settle (a normal box spread past 90 degrees, a graph test that
//! fails) is split: the piece whose proof failed (the larger in 3D when both or neither
//! did) is halved across its longer side in 3D, and the halves [`Candidates`] proves apart
//! from the other piece are dropped. A pair whose pieces are both at the resolution is
//! [`SsiError::Unresolved`] there. A pair whose solve reaches the resolution (a critical
//! set that is not a point, such as a line along which the tangent planes are parallel:
//! no smaller pair isolates it) is refused where the solve stopped, unless halving it to the
//! resolution proves every part apart; the first part that is not ends it.
//!
//! # Seeds
//!
//! Each piece's four edges (u low, u high, v low, v high: [`Side`]) are intersected with the
//! other piece by the same solver: three equations `S_A(tau) = S_B(s, t)` in three unknowns,
//! each root proved by Krawczyk, never a thresholded sample. Seeds are in a fixed order: by
//! pair, then surface (first, second), then side, then parameter. A seed whose enclosure
//! meets an earlier one's on both surfaces, or whose point is within the coincidence of an
//! earlier one's, is that seed (a corner, an edge two pairs share, or one point at both
//! ends of a seam), kept once, by the lower pair.

use scx_algo::krawczyk::ProvedRoot;
use scx_num::budget::{Budget, Stop};
use scx_num::extent2d::Extent2d;
use scx_num::interval::{Ivl, round_up};
use scx_num::units::{Accuracy, Length, Resolution};
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_surface::Surface;

use super::patch::{Jet, Jet1, Patch, dot, lift, mid};
use super::pieces::{Candidates, PiecePair};
use super::solve::{self, Enclosed, System, meets};
use crate::error::{SingularKind, SsiError};

/// The pairs of two surfaces' pieces with what is needed to prove them loop-free and seed
/// them: [`Candidates`] and the two surfaces.
#[derive(Debug)]
pub struct Survey<'s> {
    found: Candidates<'s>,
    surfaces: [&'s Surface; 2],
}

/// One pair of pieces, settled: why it holds no loop (or which points a loop must surround)
/// and the seeds on its edges.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The pair (a descendant, by halving, of one of [`Candidates::pairs`]).
    pub pair: PiecePair,
    /// The proof.
    pub loops: Loops,
    /// The points where the intersection crosses the pieces' edges, in order (module docs).
    pub seeds: Vec<Seed>,
}

/// Why a pair holds no closed loop, or where its loops can be.
#[derive(Clone, Debug, PartialEq)]
pub enum Loops {
    /// Sederberg-Meyers: every curve in the pair is strictly monotone along `along`, so
    /// none is closed and none has a singular point.
    Monotone {
        /// The direction (unnormalised).
        along: Vec3,
    },
    /// Both pieces are graphs across `across` and have no point with collinear normals:
    /// no closed loop and no singular point.
    NoCritical {
        /// The direction both pieces are graphs across (unnormalised: `e1 x e2`).
        across: Vec3,
    },
    /// Both pieces are graphs across `across`; these are all their points with collinear
    /// normals, and every closed loop in the pair surrounds one with a ray.
    Critical {
        /// As in [`Loops::NoCritical`].
        across: Vec3,
        /// The points, in the order of their first uv coordinate.
        points: Vec<Critical>,
    },
}

impl Loops {
    /// Whether the pair is proved to hold no closed loop.
    #[must_use]
    pub fn loop_free(&self) -> bool {
        match self {
            Self::Monotone { .. } | Self::NoCritical { .. } => true,
            Self::Critical { points, .. } => points.iter().all(|p| p.ray.is_empty()),
        }
    }
}

/// A point where the two pieces' normals are collinear, proved the only one in a box.
#[derive(Clone, Debug, PartialEq)]
pub struct Critical {
    /// The middle of the two surfaces' points.
    pub point: Point3,
    /// Its uv on the first surface, then on the second.
    pub uv: [Point2; 2],
    /// The distance between the two surfaces' points (its upper bound).
    pub gap: Length,
    /// What it is.
    pub kind: CriticalKind,
    /// The seeds on the ray from it (only for the kinds a loop can surround).
    pub ray: Vec<Seed>,
    /// The root's enclosure, `[u, v, s, t]`.
    enclosure: [Ivl; 4],
}

/// The kind of a collinear-normal point (73 §4.6): by whether the gap is within the
/// coincidence (a contact) and the relative form's definiteness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalKind {
    /// A contact with a definite form, and no loop around it wider than the coincidence: an
    /// isolated touch.
    Touch,
    /// A contact with an indefinite form: two branches cross.
    Node,
    /// A contact whose form is singular within its bound: not built (73 §4.6).
    Singular(SingularKind),
    /// No contact, an indefinite form: a saddle of the height, which no loop surrounds alone.
    Saddle,
    /// A definite form and no touch: a maximum or minimum of the height. `crossing` when it
    /// lies on the far side of 0 (a maximum above, a minimum below), where a loop can surround
    /// it.
    Extremum {
        /// Whether a loop can surround it.
        crossing: bool,
    },
    /// No contact, a form singular within its bound: treated as a point a loop can surround.
    Degenerate,
}

/// A proved point of the intersection on an edge of a piece (or on a ray).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seed {
    /// The point (on the surface whose edge it is).
    pub point: Point3,
    /// Its uv on the first surface, then on the second.
    pub uv: [Point2; 2],
    /// Where it was found.
    pub site: Site,
    /// The root's uv enclosure on each surface.
    enclosure: [[Ivl; 2]; 2],
}

impl Seed {
    /// Whether two seeds are one point: their enclosures meet on both surfaces, or their
    /// points are within `tol` (the coincidence: one point across a seam, or two within the
    /// depth rule's reach, 04 R73).
    #[must_use]
    pub fn same_as(&self, o: &Self, tol: f64) -> bool {
        (meets(&self.enclosure[0], &o.enclosure[0]) && meets(&self.enclosure[1], &o.enclosure[1]))
            || within_tol(self.point, o.point, tol)
    }
}

/// Where a seed was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    /// On an edge of the first surface's piece (0) or the second's (1).
    Edge {
        /// 0 or 1.
        surface: usize,
        /// Which edge.
        side: Side,
    },
    /// On the ray from a critical point, along the first surface's `u`.
    Ray,
}

/// An edge of a piece, in seed order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    /// `u` at its low end.
    ULow,
    /// `u` at its high end.
    UHigh,
    /// `v` at its low end.
    VLow,
    /// `v` at its high end.
    VHigh,
}

impl<'s> Survey<'s> {
    /// The survey of `a` over `ae` and `b` over `be`: [`Candidates::new`] with the same
    /// arguments, and the two surfaces.
    ///
    /// # Errors
    ///
    /// As [`Candidates::new`].
    pub fn new(
        a: &'s Surface,
        ae: &Extent2d,
        b: &'s Surface,
        be: &Extent2d,
        acc: [Accuracy; 2],
        res: Resolution,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Self, SsiError> {
        Ok(Self {
            found: Candidates::new(a, ae, b, be, acc, res, budget, stop)?,
            surfaces: [a, b],
        })
    }

    /// The pairs the proofs of apartness left.
    #[must_use]
    pub const fn candidates(&self) -> &Candidates<'s> {
        &self.found
    }

    /// Every candidate pair examined, in order, with seeds and critical points that an
    /// earlier plan already holds removed (module docs).
    ///
    /// # Errors
    ///
    /// As [`Survey::examine`].
    pub fn plans(&self, budget: &mut Budget, stop: &Stop) -> Result<Vec<Plan>, SsiError> {
        let tol = self.found.tolerance();
        let mut out: Vec<Plan> = Vec::new();
        for &pair in self.found.pairs() {
            for mut plan in self.examine(pair, budget, stop)? {
                plan.seeds.retain(|s| {
                    !out.iter()
                        .any(|p| p.seeds.iter().any(|o| o.same_as(s, tol)))
                });
                if let Loops::Critical { points, .. } = &mut plan.loops {
                    points.retain(|c| {
                        !out.iter().any(|p| match &p.loops {
                            Loops::Critical { points, .. } => points.iter().any(|o| {
                                meets(&o.enclosure, &c.enclosure)
                                    || within_tol(o.point, c.point, tol)
                            }),
                            _ => false,
                        })
                    });
                }
                out.push(plan);
            }
        }
        Ok(out)
    }

    /// The plans of `pair`: one, or one per descendant where the pair must be halved (a
    /// normal box too wide for a proof), depth first, lower half first.
    ///
    /// Charges one unit per pair tried and one per box of the solver, polling `stop` after
    /// each charge.
    ///
    /// # Errors
    ///
    /// [`SsiError::Unresolved`] where a pair or a solver box at the resolution is undecided
    /// (a tangency on an edge, a curve of critical points, a coincident patch);
    /// [`SsiError::Budget`] and [`SsiError::Stopped`].
    pub fn examine(
        &self,
        pair: PiecePair,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<Plan>, SsiError> {
        let mut out = Vec::new();
        let mut stack = vec![pair];
        while let Some(p) = stack.pop() {
            budget.charge(1)?;
            stop.check()?;
            let split = match self.plan(&p, budget, stop) {
                Ok(Ok(plan)) => {
                    out.push(plan);
                    continue;
                }
                Ok(Err(which)) => self.split(&p, which, budget, stop)?,
                // A solve that reached the resolution (a critical set that is not isolated,
                // such as a line where the tangent planes are parallel): no smaller pair
                // isolates it, so the pair is refused unless halving proves it apart.
                Err(e @ SsiError::Unresolved { .. }) => {
                    self.apart_or(&p, e, budget, stop)?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            stack.extend(split.into_iter().rev());
        }
        Ok(out)
    }

    /// The plan of one pair, or the piece (0 or 1) to split when it has none.
    fn plan(
        &self,
        p: &PiecePair,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Result<Plan, usize>, SsiError> {
        let [sa, sb] = self.surfaces;
        let Some(pa) = Patch::new(sa, &p.a.uv) else {
            return Ok(Err(0));
        };
        let Some(pb) = Patch::new(sb, &p.b.uv) else {
            return Ok(Err(1));
        };
        let mut loops = match self.loops(p, &pa, &pb, budget, stop)? {
            Ok(l) => l,
            Err(which) => return Ok(Err(which)),
        };
        if let Loops::Critical { points, .. } = &mut loops {
            for c in points.iter_mut() {
                if c.may_surround() {
                    c.ray = self.ray(p, &pa, &pb, c, budget, stop)?;
                }
            }
        }
        let seeds = self.edges(p, &pa, &pb, budget, stop)?;
        Ok(Ok(Plan {
            pair: *p,
            loops,
            seeds,
        }))
    }

    /// `Ok` when every part of `pair` is proved apart by halving it (the larger piece first)
    /// down to the resolution, else `refusal`: the first part, depth first, that reaches the
    /// resolution unproved ends it, so a pair holding a contact costs one descent.
    fn apart_or(
        &self,
        pair: &PiecePair,
        refusal: SsiError,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<(), SsiError> {
        let mut stack = vec![*pair];
        while let Some(p) = stack.pop() {
            match self.split(&p, larger(&p), budget, stop) {
                Ok(children) => stack.extend(children.into_iter().rev()),
                Err(SsiError::Unresolved { .. }) => return Err(refusal),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// The children of `pair` that are not proved apart: piece `which` halved (the other
    /// when it is at the resolution) across the uv direction it is longer in, in 3D.
    ///
    /// # Errors
    ///
    /// [`SsiError::Unresolved`] where `pair` lies when neither piece can be halved;
    /// [`SsiError::Budget`] and [`SsiError::Stopped`] (one unit per child).
    fn split(
        &self,
        p: &PiecePair,
        which: usize,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<PiecePair>, SsiError> {
        for w in [which, 1 - which] {
            let piece = if w == 0 { p.a } else { p.b };
            if piece.bbox.max_length() <= self.found.floor() {
                continue;
            }
            let Some(halves) = self.surfaces.get(w).and_then(|s| halves(s, &piece.uv)) else {
                continue;
            };
            let mut out = Vec::with_capacity(2);
            for uv in halves {
                budget.charge(1)?;
                stop.check()?;
                let made = if w == 0 {
                    self.found.piece_of_a(uv)
                } else {
                    self.found.piece_of_b(uv)
                };
                let Some(h) = made else {
                    return Err(p.unresolved());
                };
                let child = if w == 0 {
                    PiecePair::new(h, p.b)
                } else {
                    PiecePair::new(p.a, h)
                };
                if !self.found.apart(&child) {
                    out.push(child);
                }
            }
            return Ok(out);
        }
        Err(p.unresolved())
    }

    /// The proof for one pair, or the piece (0 or 1) to split when there is none.
    fn loops(
        &self,
        p: &PiecePair,
        pa: &Patch<'_>,
        pb: &Patch<'_>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Result<Loops, usize>, SsiError> {
        let (ja, jb) = match (
            pa.jet(ivl_u(&p.a.uv), ivl_v(&p.a.uv)),
            pb.jet(ivl_u(&p.b.uv), ivl_v(&p.b.uv)),
        ) {
            (Some(ja), Some(jb)) => (ja, jb),
            (None, Some(_)) => return Ok(Err(0)),
            (Some(_), None) => return Ok(Err(1)),
            (None, None) => return Ok(Err(larger(p))),
        };
        let (na, nb) = (ja.normal(), jb.normal());
        let (ca, cb) = match (Cone::of(na), Cone::of(nb)) {
            (Some(ca), Some(cb)) => (ca, cb),
            (None, Some(_)) => return Ok(Err(0)),
            (Some(_), None) => return Ok(Err(1)),
            (None, None) => return Ok(Err(larger(p))),
        };
        // Sederberg-Meyers along the axes' cross product.
        let along = ca.axis.cross(cb.axis);
        if along.length_squared() > 0.0 {
            let d = dot(lift(along), na.cross(nb));
            if excludes_zero(d) {
                return Ok(Ok(Loops::Monotone { along }));
            }
        }
        // Graphs across the axes' bisector.
        let sign = if ca.axis.dot(cb.axis) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let Some(e) = (ca.axis + cb.axis * sign).normalized() else {
            return Ok(Err(larger(p)));
        };
        let (e1, e2) = across(e.as_vec());
        match (graph(&ja, e1, e2), graph(&jb, e1, e2)) {
            (true, true) => {}
            (false, true) => return Ok(Err(0)),
            (true, false) => return Ok(Err(1)),
            (false, false) => return Ok(Err(larger(p))),
        }
        let across = e1.cross(e2);
        if ca.apart_from(&cb) {
            return Ok(Ok(Loops::NoCritical { across }));
        }
        let sys = Crit {
            a: *pa,
            b: *pb,
            e1,
            e2,
        };
        let start = [
            ivl_u(&p.a.uv),
            ivl_v(&p.a.uv),
            ivl_u(&p.b.uv),
            ivl_v(&p.b.uv),
        ];
        let roots = solve::roots(&sys, start, self.found.floor(), budget, stop)?;
        if roots.is_empty() {
            return Ok(Ok(Loops::NoCritical { across }));
        }
        let tol = self.found.tolerance();
        let mut points = Vec::with_capacity(roots.len());
        for r in roots {
            let Some(c) = classify(&sys, &r, tol) else {
                return Ok(Err(larger(p)));
            };
            points.push(c);
        }
        Ok(Ok(Loops::Critical { across, points }))
    }

    /// The seeds on the pieces' edges, in order (module docs).
    fn edges(
        &self,
        p: &PiecePair,
        pa: &Patch<'_>,
        pb: &Patch<'_>,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<Seed>, SsiError> {
        let mut out: Vec<Seed> = Vec::new();
        for (surface, (own, other), (ou, ov)) in [
            (0, (pa, pb), (&p.a.uv, &p.b.uv)),
            (1, (pb, pa), (&p.b.uv, &p.a.uv)),
        ] {
            for side in [Side::ULow, Side::UHigh, Side::VLow, Side::VHigh] {
                let (along_u, fixed, range) = match side {
                    Side::ULow => (false, ou.min().x, ivl_v(ou)),
                    Side::UHigh => (false, ou.max().x, ivl_v(ou)),
                    Side::VLow => (true, ou.min().y, ivl_u(ou)),
                    Side::VHigh => (true, ou.max().y, ivl_u(ou)),
                };
                let sys = Iso {
                    curve: *own,
                    along_u,
                    fixed,
                    other: *other,
                };
                let start = [range, ivl_u(ov), ivl_v(ov)];
                for r in solve::roots(&sys, start, self.found.floor(), budget, stop)? {
                    let s = sys.seed(&r, surface, Site::Edge { surface, side });
                    let tol = self.found.tolerance();
                    if let Some(s) = s.filter(|s| !out.iter().any(|o| o.same_as(s, tol))) {
                        out.push(s);
                    }
                }
            }
        }
        Ok(out)
    }

    /// The seeds on the ray from `c` along the first surface's `u` to its piece's edge.
    fn ray(
        &self,
        p: &PiecePair,
        pa: &Patch<'_>,
        pb: &Patch<'_>,
        c: &Critical,
        budget: &mut Budget,
        stop: &Stop,
    ) -> Result<Vec<Seed>, SsiError> {
        let [u, v] = [c.uv[0].x, c.uv[0].y];
        let end = p.a.uv.max().x;
        if u.partial_cmp(&end) != Some(core::cmp::Ordering::Less) {
            return Ok(Vec::new());
        }
        let sys = Iso {
            curve: *pa,
            along_u: true,
            fixed: v,
            other: *pb,
        };
        let start = [Ivl::new(u, end), ivl_u(&p.b.uv), ivl_v(&p.b.uv)];
        let mut out = Vec::new();
        for r in solve::roots(&sys, start, self.found.floor(), budget, stop)? {
            out.extend(sys.seed(&r, 0, Site::Ray));
        }
        Ok(out)
    }
}

impl Critical {
    /// Whether a loop can surround it (it is given a ray).
    #[must_use]
    pub fn may_surround(&self) -> bool {
        matches!(
            self.kind,
            CriticalKind::Extremum { crossing: true } | CriticalKind::Degenerate
        )
    }
}

/// A normal cone: an axis and a lower bound of the cosine of its half-angle (positive: the
/// half-angle is under 90 degrees).
struct Cone {
    axis: Vec3,
    cos: f64,
}

impl Cone {
    /// The cone of the normal box `n`: its middle direction, and `(axis . n)_lo / (|axis| |n|)_hi`
    /// rounded down. `None` when that is not positive.
    fn of(n: Vec3<Ivl>) -> Option<Self> {
        let axis = mid(n).normalized()?.as_vec();
        let a = lift(axis);
        let num = dot(a, n);
        let den = Ivl::from(a.norm().hi) * Ivl::from(n.norm().hi);
        if num.lo.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater) || !den.is_finite() {
            return None;
        }
        let cos = (Ivl::from(num.lo) / Ivl::from(den.hi)).lo;
        (cos > 0.0).then_some(Self { axis, cos })
    }

    /// Whether no direction of this cone is parallel to one of `o` or its opposite: the
    /// angle between the axes (folded to at most 90 degrees) exceeds the sum of the
    /// half-angles, `|cos theta|_hi < (c_a c_b - s_a s_b)_lo` with `s = sqrt(1 - c^2)`
    /// rounded up.
    fn apart_from(&self, o: &Self) -> bool {
        let (a, b) = (lift(self.axis), lift(o.axis));
        let cos_theta = (dot(a, b) / (a.norm() * b.norm())).mag();
        let one = Ivl::from(1.0);
        let (ca, cb) = (Ivl::from(self.cos), Ivl::from(o.cos));
        let sa = (one - ca * ca).sqrt();
        let sb = (one - cb * cb).sqrt();
        let bound = (ca * cb - sa * sb).lo;
        cos_theta.is_finite() && cos_theta < bound
    }
}

/// The pair's piece with the larger box (0 or 1).
fn larger(p: &PiecePair) -> usize {
    usize::from(p.a.bbox.max_length() < p.b.bbox.max_length())
}

/// The two halves of a piece across the uv direction it is longer in, in 3D (by the first
/// partials' magnitudes at its middle; by uv width where they cannot be enclosed). `None`
/// when the middle does not split the side in `f64`.
fn halves(s: &Surface, uv: &Extent2d) -> Option<[Extent2d; 2]> {
    let (lo, hi, m) = (uv.min(), uv.max(), uv.mid());
    let (du, dv) = (hi.x - lo.x, hi.y - lo.y);
    let speed = Patch::new(s, uv)
        .and_then(|p| p.jet(Ivl::from(m.x), Ivl::from(m.y)))
        .map(|j| (j.su.norm().hi, j.sv.norm().hi))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .unwrap_or((1.0, 1.0));
    if speed.0 * du >= speed.1 * dv {
        (lo.x < m.x && m.x < hi.x).then(|| {
            [
                Extent2d::from_bounds(lo.x, lo.y, m.x, hi.y),
                Extent2d::from_bounds(m.x, lo.y, hi.x, hi.y),
            ]
        })
    } else {
        (lo.y < m.y && m.y < hi.y).then(|| {
            [
                Extent2d::from_bounds(lo.x, lo.y, hi.x, m.y),
                Extent2d::from_bounds(lo.x, m.y, hi.x, hi.y),
            ]
        })
    }
    .and_then(|[a, b]| Some([a?, b?]))
}

/// Two directions spanning the plane across `e` (any two, as long as they are independent;
/// the proofs use them as given).
fn across(e: Vec3) -> (Vec3, Vec3) {
    let (x, y, z) = (e.x.abs(), e.y.abs(), e.z.abs());
    let pick = if x <= y && x <= z {
        Vec3::X
    } else if y <= z {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let e1 = e
        .cross(pick)
        .normalized()
        .map_or(pick, scx_num::math::UnitVec3::as_vec);
    (e1, e.cross(e1))
}

/// Whether `π [S_u S_v]` is a regular interval matrix over the jet's box: the projection of
/// the piece onto the plane of `e1`, `e2` is injective (module docs).
fn graph(j: &Jet, e1: Vec3, e2: Vec3) -> bool {
    let (a, b) = (lift(e1), lift(e2));
    let det = dot(a, j.su) * dot(b, j.sv) - dot(a, j.sv) * dot(b, j.su);
    excludes_zero(det)
}

/// Whether two points are within `tol` of each other (the distance rounded up).
fn within_tol(a: Point3, b: Point3, tol: f64) -> bool {
    let d = lift(a - b);
    d.norm().hi <= tol
}

fn excludes_zero(x: Ivl) -> bool {
    x.lo > 0.0 || x.hi < 0.0
}

fn ivl_u(e: &Extent2d) -> Ivl {
    Ivl::new(e.min().x, e.max().x)
}

fn ivl_v(e: &Extent2d) -> Ivl {
    Ivl::new(e.min().y, e.max().y)
}

/// The critical-point system (module docs) over `[u, v, s, t]`.
struct Crit<'s> {
    a: Patch<'s>,
    b: Patch<'s>,
    e1: Vec3,
    e2: Vec3,
}

impl Crit<'_> {
    fn jets(&self, x: &[Ivl; 4]) -> Option<(Jet, Jet)> {
        Some((self.a.jet(x[0], x[1])?, self.b.jet(x[2], x[3])?))
    }
}

impl System<4> for Crit<'_> {
    fn over(&self, x: &[Ivl; 4]) -> Option<Enclosed<4>> {
        let (a, b) = self.jets(x)?;
        let (e1, e2) = (lift(self.e1), lift(self.e2));
        let n = a.normal();
        let nu = a.suu.cross(a.sv) + a.su.cross(a.suv);
        let nv = a.suv.cross(a.sv) + a.su.cross(a.svv);
        let d = a.p - b.p;
        let zero = Ivl::from(0.0);
        let f = [dot(e1, d), dot(e2, d), dot(n, b.su), dot(n, b.sv)];
        let columns = [
            [dot(e1, a.su), dot(e2, a.su), dot(nu, b.su), dot(nu, b.sv)],
            [dot(e1, a.sv), dot(e2, a.sv), dot(nv, b.su), dot(nv, b.sv)],
            [
                zero - dot(e1, b.su),
                zero - dot(e2, b.su),
                dot(n, b.suu),
                dot(n, b.suv),
            ],
            [
                zero - dot(e1, b.sv),
                zero - dot(e2, b.sv),
                dot(n, b.suv),
                dot(n, b.svv),
            ],
        ];
        Some(Enclosed {
            f,
            columns,
            size: a.size().max(b.size()),
            at: a.mid(),
        })
    }
}

/// The kind of a proved root of the critical system, from the jets over its tight box;
/// `None` when they cannot be enclosed there.
fn classify(sys: &Crit<'_>, r: &ProvedRoot<4>, tol: f64) -> Option<Critical> {
    let (a, b) = sys.jets(&r.root)?;
    let (e1, e2) = (lift(sys.e1), lift(sys.e2));
    let n = a.normal();
    let k = e1.cross(e2);
    let kn = dot(k, n);
    let d = a.p - b.p;
    let gap = d.norm();
    let height = dot(k, d);
    // The relative form in the first surface's uv: Q = L_A - C^T L_B C, both along N_A, with
    // C = (π J_B)^-1 (π J_A).
    let la = [dot(n, a.suu), dot(n, a.suv), dot(n, a.svv)];
    let lb = [dot(n, b.suu), dot(n, b.suv), dot(n, b.svv)];
    let pa = [
        [dot(e1, a.su), dot(e1, a.sv)],
        [dot(e2, a.su), dot(e2, a.sv)],
    ];
    let pb = [
        [dot(e1, b.su), dot(e1, b.sv)],
        [dot(e2, b.su), dot(e2, b.sv)],
    ];
    let det_b = pb[0][0] * pb[1][1] - pb[0][1] * pb[1][0];
    let inv_b = [
        [pb[1][1] / det_b, Ivl::from(0.0) - pb[0][1] / det_b],
        [Ivl::from(0.0) - pb[1][0] / det_b, pb[0][0] / det_b],
    ];
    // C = inv_b pa, then Q = L_A - C^T L_B C, entry by entry.
    let [[i00, i01], [i10, i11]] = inv_b;
    let [[a00, a01], [a10, a11]] = pa;
    let [c00, c01] = [i00 * a00 + i01 * a10, i00 * a01 + i01 * a11];
    let [c10, c11] = [i10 * a00 + i11 * a10, i10 * a01 + i11 * a11];
    let [l0, l1, l2] = lb;
    let [lc00, lc01] = [l0 * c00 + l1 * c10, l0 * c01 + l1 * c11];
    let [lc10, lc11] = [l1 * c00 + l2 * c10, l1 * c01 + l2 * c11];
    let q11 = la[0] - (c00 * lc00 + c10 * lc10);
    let q12 = la[1] - (c00 * lc01 + c10 * lc11);
    let q22 = la[2] - (c01 * lc01 + c11 * lc11);
    let det = q11 * q22 - q12 * q12;
    // The height's Hessian is Q / (k . N_A): its first entry's sign is q11's times kn's.
    let h11 = q11 * kn;
    let contact = gap.lo <= tol;
    let kind = if det.lo > 0.0 {
        let max = h11.hi < 0.0;
        let min = h11.lo > 0.0;
        let above = height.lo > 0.0;
        let below = height.hi < 0.0;
        let crossing = !((max && below) || (min && above));
        // A loop proved to surround it (a maximum proved above 0, a minimum proved below)
        // is a touch only when it is within `tol` in radius: a shallow dip can be wide. With
        // H = Q / kn and A's first form G, the radius squared is at most 2 |height| / mu, mu
        // the smaller eigenvalue of H against G, and mu >= det(G^-1 H) / tr(G^-1 H), which is
        // det Q / (kn (q11 g22 + q22 g11 - 2 q12 g12)). A gap within `tol` on the other side,
        // or of a sign not proved, is a touch (§4.6).
        let wide = || {
            let (g11, g12, g22) = (dot(a.su, a.su), dot(a.su, a.sv), dot(a.sv, a.sv));
            let tr = q11 * g22 + q22 * g11 - Ivl::from(2.0) * q12 * g12;
            let spread = Ivl::from(2.0)
                * Ivl::from(height.mag())
                * Ivl::from(tr.mag())
                * Ivl::from(kn.mag());
            spread.hi > (Ivl::from(tol) * Ivl::from(tol) * det).lo
        };
        let surrounded = (max && above) || (min && below);
        if contact && !(surrounded && wide()) {
            CriticalKind::Touch
        } else {
            CriticalKind::Extremum { crossing }
        }
    } else if det.hi < 0.0 {
        if contact {
            CriticalKind::Node
        } else {
            CriticalKind::Saddle
        }
    } else if contact {
        CriticalKind::Singular(SingularKind::DegenerateForm)
    } else {
        CriticalKind::Degenerate
    };
    let m = r.root.map(Ivl::mid);
    let (pa_at, pb_at) = (a.mid(), b.mid());
    Some(Critical {
        point: pa_at + (pb_at - pa_at) * 0.5,
        uv: [Point2::new(m[0], m[1]), Point2::new(m[2], m[3])],
        gap: Length::from_mm(round_up(gap.hi)),
        kind,
        ray: Vec::new(),
        enclosure: r.root,
    })
}

/// A line of constant `v` (`along_u`) or `u` on one patch, against another: three equations
/// `S(tau) - S_o(s, t) = 0` over `[tau, s, t]`.
struct Iso<'s> {
    curve: Patch<'s>,
    along_u: bool,
    fixed: f64,
    other: Patch<'s>,
}

impl Iso<'_> {
    fn curve_jet(&self, tau: Ivl) -> Option<Jet1> {
        let f = Ivl::from(self.fixed);
        if self.along_u {
            self.curve.jet1(tau, f)
        } else {
            self.curve.jet1(f, tau)
        }
    }

    /// The seed of a root; `surface` is the curve's (0 or 1). `None` when the jets cannot
    /// be enclosed at it.
    fn seed(&self, r: &ProvedRoot<3>, surface: usize, site: Site) -> Option<Seed> {
        let m = r.root.map(Ivl::mid);
        let point = self.curve_jet(Ivl::from(m[0]))?.mid();
        let f = Ivl::from(self.fixed);
        let own = if self.along_u {
            [r.root[0], f]
        } else {
            [f, r.root[0]]
        };
        let theirs = [r.root[1], r.root[2]];
        let pt = |x: [Ivl; 2]| Point2::new(x[0].mid(), x[1].mid());
        let (uv, enclosure) = if surface == 0 {
            ([pt(own), pt(theirs)], [own, theirs])
        } else {
            ([pt(theirs), pt(own)], [theirs, own])
        };
        Some(Seed {
            point,
            uv,
            site,
            enclosure,
        })
    }
}

impl System<3> for Iso<'_> {
    fn over(&self, x: &[Ivl; 3]) -> Option<Enclosed<3>> {
        let c = self.curve_jet(x[0])?;
        let o = self.other.jet1(x[1], x[2])?;
        let ct = if self.along_u { c.su } else { c.sv };
        let d = c.p - o.p;
        let zero = Ivl::from(0.0);
        let neg = |v: Vec3<Ivl>| [zero - v.x, zero - v.y, zero - v.z];
        Some(Enclosed {
            f: [d.x, d.y, d.z],
            columns: [[ct.x, ct.y, ct.z], neg(o.su), neg(o.sv)],
            size: c.size().max(o.size()),
            at: c.mid(),
        })
    }
}
