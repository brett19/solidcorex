// Written for solidcorex. Apache-2.0.
//! A CSG membership oracle and points chosen on purpose (research 72 §3.4, §12.4 row 7;
//! testkit task T10).
//!
//! A [`Csg`] is a Boolean tree of [`Primitive`]s, each placed at an
//! [`analytic::Frame`](crate::analytic::Frame). [`Csg::side`] says whether a point is
//! [`Side::In`], [`Side::Out`] or in the [`Side::Band`] around the boundary.
//!
//! **The rule (72 §12.4 row 7).** Each primitive has a continuous function `f` with the
//! solid `S = {f <= 0}`; a tree combines them by `min` (union), `max` (intersection),
//! `max(a, -b)` (difference) and `-f` (complement). The answer for `p` and a width `delta`
//! is the interval enclosure `[f]` of `f` over the box `p +- delta` (every coordinate):
//! `In` when `[f] < 0`, `Out` when `[f] > 0`, else `Band`. Callers take
//! `delta >= sqrt 3 * coincidence`.
//!
//! **Soundness.** The enclosure is computed with outward rounding ([`Enclosure`]), so it
//! holds `f(q)` for every `q` in the box (the inclusion property). So:
//! - `In` means `f < 0` on the whole box: every point within `delta` of `p` (Euclidean,
//!   which lies in the box) is in the open set `{f < 0}`, inside the solid.
//! - `Out` likewise means the box lies in `{f > 0}`, outside it.
//! - A point within `delta` of the boundary is never `In` or `Out`: the box then holds a
//!   boundary point `b`, and `f(b) = 0` because `f` is continuous and `b` is a limit of
//!   points with `f <= 0` and of points with `f > 0`. So `0` lies in `[f]` and the answer is
//!   `Band`.
//!
//! The tree's operations keep all three. `min`, `max` and negation are exact on intervals,
//! the regularised Boolean of the leaves' open interiors and exteriors is what a strict
//! sign certifies, and continuity carries through. No Lipschitz bound is needed: `f` is not
//! a distance (except a profile's, where the distance gives the enclosure).
//!
//! **What `Band` costs.** It is sound, not tight. It also covers points near a leaf's
//! boundary that is not the result's: at the shared face of two coplanar blocks in a union
//! both leaves are `0`, so the oracle says `Band` where the truth is `In`. And interval
//! arithmetic overestimates (most under a rotated frame), so `Band` reaches somewhat beyond
//! `delta`. [`Census`] measures the share of certain answers over seeded points; callers
//! require at least 95%.
//!
//! **Per primitive** (local coordinates `q = (x, y, z)`, `rho = sqrt(x^2 + y^2)`), each `f`
//! continuous with `S = {f <= 0}`:
//! - [`Primitive::HalfSpace`] has `z`.
//! - [`Primitive::Slab`] has `max(-z, z - h)`.
//! - [`Primitive::Block`] has `max(-x, x - a, -y, y - b, -z, z - c)`.
//! - [`Primitive::Cylinder`] has `max(rho - r, -z, z - h)`.
//! - [`Primitive::Frustum`] has `max((rho - r(z)) / sqrt(1 + s^2), -z, z - h)`, `r(z) = r0 + s z`,
//!   `s = (r1 - r0) / h`.
//! - [`Primitive::Sphere`] has `|q| - r`.
//! - [`Primitive::Torus`] has `sqrt((rho - R)^2 + z^2) - r`.
//! - [`Primitive::Extrusion`] has `max(g(x, y), -z, z - h)`, `g` the profile's signed
//!   distance ([`Profile`]).
//!
//! Each is a `max` of continuous functions (or one), and the solid is where every term is
//! at most zero, which is the definition of each solid above. The enclosure evaluates each
//! formula in interval arithmetic over the box `[q]` that the frame maps `p +- delta` to;
//! a profile's term is enclosed by its Lipschitz form instead ([`Profile`]).
//!
//! **Frames.** The solid of a leaf at frame `F` is `{o + x q_0 + y q_1 + z q_2 : q in S}`
//! with the axes as given (they need not be unit or orthogonal). The local box is
//! `R^-1 (p +- delta - o)`, with `R^-1 = adj(R) / det R` in interval arithmetic: sound for
//! any frame whose determinant is clearly away from zero.
//!
//! **Points on purpose** ([`Csg::probes`]): points on each face, edge and vertex of the
//! leaves that lie on the result's boundary, points where two leaves' surfaces meet, and
//! points at `+-k` times the coincidence along each normal, `k` in `{1/2, 2, 10}`.

mod ivl;
mod probe;
mod profile;

use core::fmt;

pub use ivl::Enclosure;
pub use probe::{Feature, FeatureKind, Probe, ProbeSpec};
pub use profile::{Profile, Seg};

use crate::analytic::Frame;
use scx_num::hash::Rng;

/// Where a point lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// Certainly inside: the whole box `p +- delta` is in the solid's interior.
    In,
    /// Certainly outside: the whole box is in the solid's exterior.
    Out,
    /// No claim: the box may meet the boundary (it does whenever `p` is within `delta`).
    Band,
}

/// Why a recipe was refused.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum CsgError {
    /// A frame is not finite, or its axes are (nearly) dependent.
    Frame,
    /// A size is not finite or out of its range; `what` names it.
    Size {
        /// The size.
        what: &'static str,
    },
    /// A profile with no loop, an empty loop, or a circle sharing its loop.
    ProfileShape,
    /// A profile whose consecutive pieces do not meet.
    ProfileOpen {
        /// The largest gap.
        gap: f64,
    },
}

impl fmt::Display for CsgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsgError::Frame => write!(f, "frame not finite or singular"),
            CsgError::Size { what } => write!(f, "{what}: not finite or out of range"),
            CsgError::ProfileShape => write!(f, "profile: no loop, empty loop or shared circle"),
            CsgError::ProfileOpen { gap } => write!(f, "profile open: gap {gap:e}"),
        }
    }
}

impl std::error::Error for CsgError {}

/// A solid in its local frame. Sizes are finite; the ranges are on each variant.
#[derive(Clone, Debug)]
pub enum Primitive {
    /// `z <= 0`.
    HalfSpace,
    /// `0 <= z <= height`, `height > 0`.
    Slab {
        /// Its height.
        height: f64,
    },
    /// `[0, a] x [0, b] x [0, c]`, each above zero: a corner at the origin.
    Block {
        /// `[a, b, c]`.
        size: [f64; 3],
    },
    /// `rho <= radius`, `0 <= z <= height`, both above zero.
    Cylinder {
        /// Its radius.
        radius: f64,
        /// Its height.
        height: f64,
    },
    /// A cone frustum: radius `radius0` at `z = 0` and `radius1` at `z = height`, both at
    /// least zero and not both zero, `height > 0` (a radius of zero is an apex).
    Frustum {
        /// The radius at `z = 0`.
        radius0: f64,
        /// The radius at `z = height`.
        radius1: f64,
        /// Its height.
        height: f64,
    },
    /// `|q| <= radius`, `radius > 0`.
    Sphere {
        /// Its radius.
        radius: f64,
    },
    /// The points within `minor` of the circle of radius `major` about `z`: `major >= 0`,
    /// `minor > 0` (a spindle when `minor > major`).
    Torus {
        /// The circle's radius.
        major: f64,
        /// The tube's radius.
        minor: f64,
    },
    /// The profile (in local `x, y`) swept along `z` from `0` to `height > 0`.
    Extrusion {
        /// The cross-section.
        profile: Profile,
        /// Its height.
        height: f64,
    },
}

/// A frame ready to map boxes: `R^-1` in intervals.
#[derive(Clone, Debug)]
struct Placement {
    origin: [f64; 3],
    axes: [[f64; 3]; 3],
    inv: [[Enclosure; 3]; 3],
}

fn ip(x: f64) -> Enclosure {
    Enclosure::point(x)
}

fn icross(a: [f64; 3], b: [f64; 3]) -> [Enclosure; 3] {
    let m = |i: f64, j: f64| ip(i).mul(ip(j));
    [
        m(a[1], b[2]).sub(m(a[2], b[1])),
        m(a[2], b[0]).sub(m(a[0], b[2])),
        m(a[0], b[1]).sub(m(a[1], b[0])),
    ]
}

/// `[f(a_0, b_0), f(a_1, b_1), f(a_2, b_2)]`.
fn zip3<T: Copy, U: Copy, R>(a: [T; 3], b: [U; 3], f: impl Fn(T, U) -> R) -> [R; 3] {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

impl Placement {
    fn new(f: &Frame) -> Result<Placement, CsgError> {
        let all = [f.origin, f.x, f.y, f.z];
        if !all
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() < 1e100)
        {
            return Err(CsgError::Frame);
        }
        let rows = [icross(f.y, f.z), icross(f.z, f.x), icross(f.x, f.y)];
        let [r0, _, _] = rows;
        let det = ip(f.x[0])
            .mul(r0[0])
            .add(ip(f.x[1]).mul(r0[1]))
            .add(ip(f.x[2]).mul(r0[2]));
        let vol = norm(f.x) * norm(f.y) * norm(f.z);
        if det.holds_zero() || det.mid().abs() <= 1e-9 * vol {
            return Err(CsgError::Frame);
        }
        let inv = rows.map(|r| r.map(|e| e.div(det)));
        Ok(Placement {
            origin: f.origin,
            axes: [f.x, f.y, f.z],
            inv,
        })
    }

    /// The local box holding `R^-1 (q - o)` for every `q` in `[lo, hi]`.
    fn local(&self, b: &[Enclosure; 3]) -> [Enclosure; 3] {
        let d = zip3(*b, self.origin, |e, o| e.sub(ip(o)));
        self.inv
            .map(|r| r[0].mul(d[0]).add(r[1].mul(d[1])).add(r[2].mul(d[2])))
    }

    /// `o + R q`, rounded.
    fn world(&self, q: [f64; 3]) -> [f64; 3] {
        let [x, y, z] = self.axes;
        let s = zip3(self.origin, x, |o, a| o + q[0] * a);
        let s = zip3(s, y, |o, a| o + q[1] * a);
        zip3(s, z, |o, a| o + q[2] * a)
    }

    /// The unit world normal of a local normal `n`: `R^-T n`, normalised.
    fn normal(&self, n: [f64; 3]) -> [f64; 3] {
        let m = self.inv.map(|r| r.map(Enclosure::mid));
        let v = zip3(m[0], m[1], |a, b| n[0] * a + n[1] * b);
        let v = zip3(v, m[2], |a, b| a + n[2] * b);
        let l = norm(v).max(f64::MIN_POSITIVE);
        v.map(|c| c / l)
    }
}

fn positive(x: f64, what: &'static str) -> Result<(), CsgError> {
    if x.is_finite() && x > 0.0 && x < 1e100 {
        Ok(())
    } else {
        Err(CsgError::Size { what })
    }
}

impl Primitive {
    fn check(&self) -> Result<(), CsgError> {
        match self {
            Primitive::HalfSpace => Ok(()),
            Primitive::Slab { height } => positive(*height, "slab height"),
            Primitive::Block { size } => size.iter().try_for_each(|s| positive(*s, "block size")),
            Primitive::Cylinder { radius, height } => {
                positive(*radius, "cylinder radius")?;
                positive(*height, "cylinder height")
            }
            Primitive::Frustum {
                radius0,
                radius1,
                height,
            } => {
                positive(*height, "frustum height")?;
                positive(radius0.max(*radius1), "frustum radius")?;
                if radius0.min(*radius1) >= 0.0 {
                    Ok(())
                } else {
                    Err(CsgError::Size {
                        what: "frustum radius",
                    })
                }
            }
            Primitive::Sphere { radius } => positive(*radius, "sphere radius"),
            Primitive::Torus { major, minor } => {
                positive(*minor, "torus minor")?;
                if *major == 0.0 {
                    Ok(())
                } else {
                    positive(*major, "torus major")
                }
            }
            Primitive::Extrusion { height, .. } => positive(*height, "extrusion height"),
        }
    }

    /// The enclosure of `f` over the local box `q`.
    fn enclose(&self, q: &[Enclosure; 3]) -> Enclosure {
        let [x, y, z] = *q;
        let caps = |h: f64| z.neg().max(z.sub(ip(h)));
        let rho = || x.sqr().add(y.sqr()).sqrt();
        match self {
            Primitive::HalfSpace => z,
            Primitive::Slab { height } => caps(*height),
            Primitive::Block { size: [a, b, c] } => x
                .neg()
                .max(x.sub(ip(*a)))
                .max(y.neg())
                .max(y.sub(ip(*b)))
                .max(caps(*c)),
            Primitive::Cylinder { radius, height } => rho().sub(ip(*radius)).max(caps(*height)),
            Primitive::Frustum {
                radius0,
                radius1,
                height,
            } => {
                let slope = ip(*radius1).sub(ip(*radius0)).div(ip(*height));
                let r = ip(*radius0).add(slope.mul(z));
                // Divided by the slant's length so its width is that of a distance (a
                // steep cone's band would otherwise reach `|slope|` widths).
                let slant = ip(1.0).add(slope.sqr()).sqrt();
                rho().sub(r).div(slant).max(caps(*height))
            }
            Primitive::Sphere { radius } => {
                x.sqr().add(y.sqr()).add(z.sqr()).sqrt().sub(ip(*radius))
            }
            Primitive::Torus { major, minor } => rho()
                .sub(ip(*major))
                .sqr()
                .add(z.sqr())
                .sqrt()
                .sub(ip(*minor)),
            Primitive::Extrusion { profile, height } => profile.enclose(x, y).max(caps(*height)),
        }
    }

    /// A local box holding the solid, `None` when it is unbounded.
    fn local_bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        match self {
            Primitive::HalfSpace | Primitive::Slab { .. } => None,
            Primitive::Block { size } => Some(([0.0; 3], *size)),
            Primitive::Cylinder { radius: r, height } => Some(([-r, -r, 0.0], [*r, *r, *height])),
            Primitive::Frustum {
                radius0,
                radius1,
                height,
            } => {
                let r = radius0.max(*radius1);
                Some(([-r, -r, 0.0], [r, r, *height]))
            }
            Primitive::Sphere { radius: r } => Some(([-r; 3], [*r; 3])),
            Primitive::Torus { major, minor } => {
                let r = major + minor;
                Some(([-r, -r, -minor], [r, r, *minor]))
            }
            Primitive::Extrusion { profile, height } => {
                let (lo, hi) = profile.bounds();
                Some(([lo[0], lo[1], 0.0], [hi[0], hi[1], *height]))
            }
        }
    }
}

/// A placed primitive: one leaf of a [`Csg`].
#[derive(Clone, Debug)]
pub struct Leaf {
    placement: Placement,
    primitive: Primitive,
}

impl Leaf {
    /// Its primitive.
    pub fn primitive(&self) -> &Primitive {
        &self.primitive
    }

    fn enclose(&self, b: &[Enclosure; 3]) -> Enclosure {
        self.primitive.enclose(&self.placement.local(b))
    }

    /// `f` at a point (the enclosure of a zero-width box, its middle).
    fn value(&self, p: [f64; 3]) -> f64 {
        self.enclose(&p.map(ip)).mid()
    }
}

#[derive(Clone, Debug)]
enum Node {
    Leaf(Box<Leaf>),
    Union(Box<Node>, Box<Node>),
    Intersection(Box<Node>, Box<Node>),
    Difference(Box<Node>, Box<Node>),
    Complement(Box<Node>),
}

/// A Boolean tree of placed primitives. Its leaves are numbered depth first, left to
/// right ([`Csg::leaves`]).
#[derive(Clone, Debug)]
pub struct Csg(Node);

/// Magnitudes beyond this are not evaluated (`Band`): far from overflow in any formula.
const FAR: f64 = 1e150;

impl Csg {
    /// One primitive at a frame.
    ///
    /// # Errors
    /// [`CsgError::Frame`] for a non-finite or singular frame, [`CsgError::Size`] for a size
    /// out of its range.
    pub fn leaf(frame: &Frame, primitive: Primitive) -> Result<Csg, CsgError> {
        primitive.check()?;
        Ok(Csg(Node::Leaf(Box::new(Leaf {
            placement: Placement::new(frame)?,
            primitive,
        }))))
    }

    /// `self` union `other`.
    #[must_use]
    pub fn union(self, other: Csg) -> Csg {
        Csg(Node::Union(Box::new(self.0), Box::new(other.0)))
    }

    /// `self` intersection `other`.
    #[must_use]
    pub fn intersection(self, other: Csg) -> Csg {
        Csg(Node::Intersection(Box::new(self.0), Box::new(other.0)))
    }

    /// `self` minus `other`.
    #[must_use]
    pub fn difference(self, other: Csg) -> Csg {
        Csg(Node::Difference(Box::new(self.0), Box::new(other.0)))
    }

    /// The complement of `self`.
    #[must_use]
    pub fn complement(self) -> Csg {
        Csg(Node::Complement(Box::new(self.0)))
    }

    /// The leaves, depth first, left to right: a [`Feature`]'s `leaf` indexes this.
    pub fn leaves(&self) -> Vec<&Leaf> {
        let mut out = Vec::new();
        self.0.collect(&mut out);
        out
    }

    /// The enclosure of the tree's `f` over the box `p +- delta`, or `None` when `p` or
    /// `delta` is not finite, `delta < 0`, or a coordinate's magnitude passes `1e150`.
    pub fn enclose(&self, p: [f64; 3], delta: f64) -> Option<Enclosure> {
        let ok = (0.0..FAR).contains(&delta) && p.iter().all(|c| c.is_finite() && c.abs() < FAR);
        ok.then(|| self.0.enclose(&p.map(|c| Enclosure::around(c, delta))))
    }

    /// Where `p` lies, at width `delta` (callers take `delta >= sqrt 3 * coincidence`):
    /// the module's rule. `Band` too when [`Csg::enclose`] declines.
    pub fn side(&self, p: [f64; 3], delta: f64) -> Side {
        match self.enclose(p, delta) {
            Some(e) if e.lo > 0.0 => Side::Out,
            Some(e) if e.hi < 0.0 => Side::In,
            _ => Side::Band,
        }
    }

    /// A box holding the solid, `(lo, hi)`, or `None` when it is unbounded (a half-space,
    /// a slab or a complement not cut down by an intersection).
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        self.0.bounds()
    }

    /// The counts of [`Side`]s over `points` at width `delta`.
    pub fn census(&self, points: &[[f64; 3]], delta: f64) -> Census {
        let mut c = Census::default();
        for p in points {
            match self.side(*p, delta) {
                Side::In => c.inside += 1,
                Side::Out => c.outside += 1,
                Side::Band => c.band += 1,
            }
        }
        c
    }
}

impl Node {
    fn collect<'a>(&'a self, out: &mut Vec<&'a Leaf>) {
        match self {
            Node::Leaf(l) => out.push(l),
            Node::Union(a, b) | Node::Intersection(a, b) | Node::Difference(a, b) => {
                a.collect(out);
                b.collect(out);
            }
            Node::Complement(a) => a.collect(out),
        }
    }

    fn enclose(&self, b: &[Enclosure; 3]) -> Enclosure {
        match self {
            Node::Leaf(l) => l.enclose(b),
            Node::Union(x, y) => x.enclose(b).min(y.enclose(b)),
            Node::Intersection(x, y) => x.enclose(b).max(y.enclose(b)),
            Node::Difference(x, y) => x.enclose(b).max(y.enclose(b).neg()),
            Node::Complement(x) => x.enclose(b).neg(),
        }
    }

    fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        match self {
            Node::Leaf(l) => {
                let (lo, hi) = l.primitive.local_bounds()?;
                let mut out = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                for i in 0..8 {
                    let pick = [i & 1, i & 2, i & 4].map(|b| b != 0);
                    let q = zip3(
                        zip3(lo, hi, |a, b| (a, b)),
                        pick,
                        |(a, b), t| if t { b } else { a },
                    );
                    let w = l.placement.world(q);
                    out = (zip3(out.0, w, f64::min), zip3(out.1, w, f64::max));
                }
                // Room for the rounding of the corners.
                let s = out
                    .0
                    .iter()
                    .chain(&out.1)
                    .fold(1.0, |m: f64, v| m.max(v.abs()));
                Some((out.0.map(|v| v - 1e-12 * s), out.1.map(|v| v + 1e-12 * s)))
            }
            Node::Union(a, b) => {
                let ((al, ah), (bl, bh)) = (a.bounds()?, b.bounds()?);
                Some((zip3(al, bl, f64::min), zip3(ah, bh, f64::max)))
            }
            Node::Intersection(a, b) => match (a.bounds(), b.bounds()) {
                (Some((al, ah)), Some((bl, bh))) => {
                    Some((zip3(al, bl, f64::max), zip3(ah, bh, f64::min)))
                }
                (x, None) | (None, x) => x,
            },
            Node::Difference(a, _) => a.bounds(),
            Node::Complement(_) => None,
        }
    }
}

/// The counts of [`Side`]s over a set of points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Census {
    /// Points certainly inside.
    pub inside: usize,
    /// Points certainly outside.
    pub outside: usize,
    /// Points in the band.
    pub band: usize,
}

impl Census {
    /// The share of certain answers, in `[0, 1]` (`1` for no points).
    #[allow(clippy::cast_precision_loss)] // counts far below 2^53
    pub fn certain_fraction(&self) -> f64 {
        let n = self.inside + self.outside + self.band;
        if n == 0 {
            1.0
        } else {
            (self.inside + self.outside) as f64 / n as f64
        }
    }
}

/// `count` seeded points uniform in the box `[lo, hi]` grown about its centre by
/// `grow` of its size (`0.1` for "grown 10%": each half-extent times `1.1`).
pub fn box_points(lo: [f64; 3], hi: [f64; 3], grow: f64, count: usize, seed: u64) -> Vec<[f64; 3]> {
    let mut rng = Rng::new(seed);
    let c = zip3(lo, hi, |a, b| 0.5 * a + 0.5 * b);
    let h = zip3(lo, hi, |a, b| (0.5 * b - 0.5 * a) * (1.0 + grow));
    (0..count)
        .map(|_| {
            let r = [(); 3].map(|()| rng.next_signed_unit());
            zip3(zip3(c, h, |a, b| (a, b)), r, |(a, b), t| a + b * t)
        })
        .collect()
}
