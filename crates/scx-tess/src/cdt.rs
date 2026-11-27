// Written for solidcorex. Apache-2.0.
//! A constrained Delaunay triangulation in the plane (research 72 D4): points and
//! constraint segments in, the triangles of the region the constraints bound out, with no
//! point added.
//!
//! Every decision is an exact predicate ([`scx_num::predicates`]), so the triangles are a
//! function of the input bits. The points are inserted in an order fixed by their
//! coordinates whatever order they come in (along a Hilbert curve over their box, ties in
//! `(x, y)` order), and the segments in an order fixed by their ends, so the result is a
//! function of the point set and the segment set alone; a cocircular tie keeps the edge it
//! has (no flip on an exact zero), which the fixed order makes a fixed rule.
//!
//! - **Delaunay.** Incremental insertion into a triangle enclosing every point, located by
//!   a visibility walk from the last triangle made, then Lawson's flips.
//! - **Constraints.** Each segment's crossed edges are flipped away (Sloan, 1993: flip a
//!   crossed edge whose quadrilateral is strictly convex, else requeue it), then the new
//!   edges are made Delaunay again by Lawson's flips, never flipping a constraint. A point
//!   exactly on a segment splits it. Two segments crossing are refused ([`Fault::Crossing`]).
//! - **Inside.** A walk over the triangles from the enclosing triangle's corner, the side
//!   toggling across each constraint used an odd number of times; the triangles on the
//!   odd side are the region's. A constraint used twice (a slit) does not toggle.
//!
//! Every triangle out is counter-clockwise, strictly: a point is inserted strictly inside a
//! triangle or on an edge, and a flip is made only across a strictly convex quadrilateral.

use core::cmp::Ordering;
use std::collections::VecDeque;

use scx_num::budget::{Budget, Exhausted};
use scx_num::predicates::{incircle, orient2d};
use scx_num::vector2d::Point2;

const NONE: u32 = u32::MAX;

/// The Hilbert curve's cells per side.
const SIDE: u32 = 1 << 16;

/// Why a triangulation did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    /// Two distinct points at the same coordinates.
    Duplicate,
    /// Two segments cross. (A segment through a point that is not its end is split there.)
    Crossing,
    /// The structure lost a triangle it should hold: a bug.
    Lost,
    /// A triangle is on both sides of the constraints: a bug, or segments that bound no
    /// region.
    Parity,
    /// The budget ran out.
    Exhausted(Exhausted),
}

impl From<Exhausted> for Fault {
    fn from(e: Exhausted) -> Self {
        Self::Exhausted(e)
    }
}

#[derive(Debug, Clone, Copy)]
struct Tri {
    /// Corners, counter-clockwise.
    v: [u32; 3],
    /// The neighbour across the side opposite each corner.
    n: [u32; 3],
    /// How many times the side opposite each corner is a constraint.
    c: [u8; 3],
}

/// `a` turned so that its entry `k` (taken mod 3) comes first.
fn turn<T: Copy>(a: [T; 3], k: usize) -> [T; 3] {
    let [x, y, z] = a;
    match k % 3 {
        0 => [x, y, z],
        1 => [y, z, x],
        _ => [z, x, y],
    }
}

/// Entry `k` (mod 3) of `a`.
fn nth<T: Copy>(a: [T; 3], k: usize) -> T {
    let [x, ..] = turn(a, k);
    x
}

impl Tri {
    /// The same triangle with corner `k` first.
    fn turned(self, k: usize) -> Self {
        Self {
            v: turn(self.v, k),
            n: turn(self.n, k),
            c: turn(self.c, k),
        }
    }

    /// Counts one more constraint use on the side opposite corner `k`.
    fn bump(&mut self, k: usize) {
        let [a, b, c] = &mut self.c;
        let c = match k % 3 {
            0 => a,
            1 => b,
            _ => c,
        };
        *c = c.saturating_add(1);
    }
}

/// The triangles inside `segments`, each counter-clockwise by index into `points`.
///
/// One unit of `budget` per point and per flip.
pub(crate) fn triangulate(
    points: &[Point2],
    segments: &[[u32; 2]],
    budget: &mut Budget,
) -> Result<Vec<[u32; 3]>, Fault> {
    let mut t = Cdt::new(points)?;
    // Insertion along a Hilbert curve over the points' box, so each walk is short; ties
    // on the curve in `(x, y)` order, which puts equal points side by side.
    let pts = t.p.get(..t.n as usize).ok_or(Fault::Lost)?;
    let mut keyed: Vec<(u32, Point2, u32)> = hilbert_keys(pts)
        .into_iter()
        .zip(pts.iter().copied())
        .zip(0..)
        .map(|((k, p), i)| (k, p, i))
        .collect();
    keyed.sort_by(|(ka, a, _), (kb, b, _)| {
        ka.cmp(kb)
            .then(a.x.total_cmp(&b.x))
            .then(a.y.total_cmp(&b.y))
    });
    for w in keyed.windows(2) {
        if let [(_, a, _), (_, b, _)] = w
            && a == b
        {
            return Err(Fault::Duplicate);
        }
    }
    let order: Vec<u32> = keyed.into_iter().map(|(_, _, i)| i).collect();
    for &i in &order {
        budget.charge(1)?;
        t.insert(i, budget)?;
    }
    // Segments by their ends' places in the point order, so the order they come in, and
    // the indices the points have, change nothing.
    let mut rank = vec![0_u32; order.len()];
    for (r, &i) in (0..).zip(&order) {
        if let Some(slot) = rank.get_mut(i as usize) {
            *slot = r;
        }
    }
    let mut segs = Vec::with_capacity(segments.len());
    for &[a, b] in segments {
        let (Some(&ra), Some(&rb)) = (rank.get(a as usize), rank.get(b as usize)) else {
            return Err(Fault::Lost);
        };
        if a != b {
            segs.push([ra.min(rb), ra.max(rb)]);
        }
    }
    segs.sort_unstable();
    for [ra, rb] in segs {
        let (Some(&a), Some(&b)) = (order.get(ra as usize), order.get(rb as usize)) else {
            return Err(Fault::Lost);
        };
        t.constrain(a, b, budget)?;
    }
    t.inside()
}

/// Each point's place on a Hilbert curve of order 16 over the points' bounding box.
fn hilbert_keys(p: &[Point2]) -> Vec<u32> {
    let (mut lo, mut hi) = (
        Point2::new(f64::MAX, f64::MAX),
        Point2::new(f64::MIN, f64::MIN),
    );
    for q in p {
        lo = Point2::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Point2::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    let cell = |v: f64, lo: f64, hi: f64| {
        let w = hi - lo;
        if w > 0.0 {
            // A float to int cast saturates, so a rounding past either end is clamped.
            (((v - lo) / w * f64::from(SIDE)) as u32).min(SIDE - 1)
        } else {
            0
        }
    };
    p.iter()
        .map(|q| {
            let (mut x, mut y) = (cell(q.x, lo.x, hi.x), cell(q.y, lo.y, hi.y));
            let mut d = 0_u32;
            let mut s = SIDE / 2;
            while s > 0 {
                let (rx, ry) = (u32::from(x & s > 0), u32::from(y & s > 0));
                d += s * s * ((3 * rx) ^ ry);
                // Rotate the quadrant.
                if ry == 0 {
                    if rx == 1 {
                        x = SIDE - 1 - x;
                        y = SIDE - 1 - y;
                    }
                    core::mem::swap(&mut x, &mut y);
                }
                s /= 2;
            }
            d
        })
        .collect()
}

struct Cdt {
    p: Vec<Point2>,
    /// The number of input points; the enclosing triangle's corners follow them.
    n: u32,
    tris: Vec<Tri>,
    /// A triangle at each point, once inserted.
    at: Vec<u32>,
    /// The last triangle made, where a walk starts.
    last: u32,
}

enum Found {
    Inside(u32),
    OnSide(u32, usize),
}

/// A triangle round a point: its index, the point's corner, and its corners from the
/// point's on.
#[derive(Clone, Copy)]
struct Round {
    t: u32,
    i: usize,
    v: [u32; 3],
}

impl Cdt {
    fn new(points: &[Point2]) -> Result<Self, Fault> {
        let n = u32::try_from(points.len())
            .ok()
            .filter(|&n| n < NONE - 3)
            .ok_or(Fault::Lost)?;
        let (mut lo, mut hi) = (
            Point2::new(f64::MAX, f64::MAX),
            Point2::new(f64::MIN, f64::MIN),
        );
        for q in points {
            if !(q.x.is_finite() && q.y.is_finite()) {
                return Err(Fault::Lost);
            }
            lo = Point2::new(lo.x.min(q.x), lo.y.min(q.y));
            hi = Point2::new(hi.x.max(q.x), hi.y.max(q.y));
        }
        let (cx, cy) = (0.5 * lo.x + 0.5 * hi.x, 0.5 * lo.y + 0.5 * hi.y);
        let span = (hi.x - lo.x).max(hi.y - lo.y);
        // Every point lies within `span` of the centre on each axis, so inside the triangle
        // `x, y >= -m`, `x + y <= 2m` about it with room to spare.
        let m = if span > 0.0 { 4.0 * span } else { 1.0 };
        // `-0.0` as `0.0`, so equal points sort together.
        let mut p: Vec<Point2> = points
            .iter()
            .map(|q| Point2::new(q.x + 0.0, q.y + 0.0))
            .collect();
        p.push(Point2::new(cx - m, cy - m));
        p.push(Point2::new(cx + 3.0 * m, cy - m));
        p.push(Point2::new(cx - m, cy + 3.0 * m));
        let mut at = vec![NONE; points.len()];
        at.extend([0, 0, 0]);
        Ok(Self {
            p,
            n,
            tris: vec![Tri {
                v: [n, n + 1, n + 2],
                n: [NONE; 3],
                c: [0; 3],
            }],
            at,
            last: 0,
        })
    }

    fn pt(&self, v: u32) -> Result<Point2, Fault> {
        self.p.get(v as usize).copied().ok_or(Fault::Lost)
    }

    fn tri(&self, t: u32) -> Result<Tri, Fault> {
        self.tris.get(t as usize).copied().ok_or(Fault::Lost)
    }

    fn tri_mut(&mut self, t: u32) -> Result<&mut Tri, Fault> {
        self.tris.get_mut(t as usize).ok_or(Fault::Lost)
    }

    fn orient(&self, a: u32, b: u32, c: u32) -> Result<Ordering, Fault> {
        Ok(orient2d(self.pt(a)?, self.pt(b)?, self.pt(c)?))
    }

    /// The neighbour `u` of `t` turned so its first corner faces `t`, and that corner's
    /// place in `u`.
    fn facing(&self, t: u32, u: u32) -> Result<(Tri, usize), Fault> {
        let ut = self.tri(u)?;
        let j = ut.n.iter().position(|&x| x == t).ok_or(Fault::Lost)?;
        Ok((ut.turned(j), j))
    }

    /// Points the neighbour `of`'s side that faced `old` at `new`.
    fn repoint(&mut self, of: u32, old: u32, new: u32) -> Result<(), Fault> {
        if of == NONE {
            return Ok(());
        }
        if let Some(x) = self.tri_mut(of)?.n.iter_mut().find(|x| **x == old) {
            *x = new;
        }
        Ok(())
    }

    fn set(&mut self, t: u32, tri: Tri) -> Result<(), Fault> {
        *self.tri_mut(t)? = tri;
        for v in tri.v {
            *self.at.get_mut(v as usize).ok_or(Fault::Lost)? = t;
        }
        self.last = t;
        Ok(())
    }

    fn push(&mut self, tri: Tri) -> Result<u32, Fault> {
        let t = self.next_id(0)?;
        self.tris.push(tri);
        self.set(t, tri)?;
        Ok(t)
    }

    /// The index the triangle made `ahead` after the next will have.
    fn next_id(&self, ahead: usize) -> Result<u32, Fault> {
        u32::try_from(self.tris.len() + ahead)
            .ok()
            .filter(|&t| t != NONE)
            .ok_or(Fault::Lost)
    }

    /// The triangle holding `q`: a visibility walk, which ends on a Delaunay triangulation.
    fn locate(&self, q: u32) -> Result<Found, Fault> {
        let mut t = self.last;
        for _ in 0..=4 * self.tris.len() + 8 {
            let tri = self.tri(t)?;
            let mut on = None;
            let mut next = None;
            for k in 0..3 {
                let [_, a, b] = turn(tri.v, k);
                match self.orient(a, b, q)? {
                    Ordering::Less => {
                        next = Some(nth(tri.n, k));
                        break;
                    }
                    Ordering::Equal => on = Some(k),
                    Ordering::Greater => {}
                }
            }
            match next {
                Some(NONE) => return Err(Fault::Lost),
                Some(u) => t = u,
                None => {
                    return Ok(match on {
                        Some(k) => Found::OnSide(t, k),
                        None => Found::Inside(t),
                    });
                }
            }
        }
        Err(Fault::Lost)
    }

    fn insert(&mut self, q: u32, budget: &mut Budget) -> Result<(), Fault> {
        let mut todo = Vec::new();
        match self.locate(q)? {
            Found::Inside(t) => {
                let Tri {
                    v: [a, b, c],
                    n: [na, nb, nc],
                    c: [ca, cb, cc],
                } = self.tri(t)?;
                let (t1, t2) = (self.next_id(0)?, self.next_id(1)?);
                self.set(
                    t,
                    Tri {
                        v: [q, b, c],
                        n: [na, t1, t2],
                        c: [ca, 0, 0],
                    },
                )?;
                self.push(Tri {
                    v: [q, c, a],
                    n: [nb, t2, t],
                    c: [cb, 0, 0],
                })?;
                self.push(Tri {
                    v: [q, a, b],
                    n: [nc, t, t1],
                    c: [cc, 0, 0],
                })?;
                self.repoint(nb, t, t1)?;
                self.repoint(nc, t, t2)?;
                todo.extend([(t, 0), (t1, 0), (t2, 0)]);
            }
            Found::OnSide(t, k) => {
                let Tri {
                    v: [a, b, c],
                    n: [u, nb, nc],
                    c: [m, cb, cc],
                } = self.tri(t)?.turned(k);
                if u == NONE {
                    return Err(Fault::Lost);
                }
                // u is (d, c, b).
                let Tri {
                    v: [d, ..],
                    n: [_, mc, mb],
                    c: [_, uc, ub],
                } = self.facing(t, u)?.0;
                let (t1, u1) = (self.next_id(0)?, self.next_id(1)?);
                self.set(
                    t,
                    Tri {
                        v: [a, b, q],
                        n: [u1, t1, nc],
                        c: [m, 0, cc],
                    },
                )?;
                self.push(Tri {
                    v: [a, q, c],
                    n: [u, nb, t],
                    c: [m, cb, 0],
                })?;
                self.set(
                    u,
                    Tri {
                        v: [d, c, q],
                        n: [t1, u1, mb],
                        c: [m, 0, ub],
                    },
                )?;
                self.push(Tri {
                    v: [d, q, b],
                    n: [t, mc, u],
                    c: [m, uc, 0],
                })?;
                self.repoint(nb, t, t1)?;
                self.repoint(mc, u, u1)?;
                todo.extend([(t, 2), (t1, 1), (u, 2), (u1, 1)]);
            }
        }
        self.legalize(todo, false, budget)
    }

    /// Flips the side opposite corner `k` of `t`: `t = (a, b, c)` and its neighbour
    /// `(d, c, b)` become `(a, b, d)` (kept as `t`) and `(a, d, c)` (kept as the neighbour).
    fn flip(&mut self, t: u32, k: usize, budget: &mut Budget) -> Result<u32, Fault> {
        budget.charge(1)?;
        let Tri {
            v: [a, b, c],
            n: [u, nb, nc],
            c: [_, cb, cc],
        } = self.tri(t)?.turned(k);
        if u == NONE {
            return Err(Fault::Lost);
        }
        let Tri {
            v: [d, ..],
            n: [_, mc, mb],
            c: [_, uc, ub],
        } = self.facing(t, u)?.0;
        self.set(
            t,
            Tri {
                v: [a, b, d],
                n: [mc, u, nc],
                c: [uc, 0, cc],
            },
        )?;
        self.set(
            u,
            Tri {
                v: [a, d, c],
                n: [mb, nb, t],
                c: [ub, cb, 0],
            },
        )?;
        self.repoint(mc, u, t)?;
        self.repoint(nb, t, u)?;
        Ok(u)
    }

    /// Lawson's flips from `todo` until every side reached is locally Delaunay or a
    /// constraint. After a flip the two far sides are checked again, and with `all` the two
    /// near ones too: an insertion's todo has the new point at its corner, so its near sides
    /// are new and Delaunay already.
    fn legalize(
        &mut self,
        mut todo: Vec<(u32, usize)>,
        all: bool,
        budget: &mut Budget,
    ) -> Result<(), Fault> {
        while let Some((t, k)) = todo.pop() {
            let Tri {
                v: [a, b, c],
                n: [u, ..],
                c: [m, ..],
            } = self.tri(t)?.turned(k);
            if u == NONE || m > 0 {
                continue;
            }
            let [d, ..] = self.facing(t, u)?.0.v;
            if incircle(self.pt(a)?, self.pt(b)?, self.pt(c)?, self.pt(d)?) == Ordering::Greater {
                let u = self.flip(t, k, budget)?;
                todo.extend([(t, 0), (u, 0)]);
                if all {
                    todo.extend([(t, 2), (u, 1)]);
                }
            }
        }
        Ok(())
    }

    /// The triangles round `v`, counter-clockwise from the one `at` holds: a full turn for
    /// an inserted point.
    fn fan(&self, v: u32) -> Result<Vec<Round>, Fault> {
        let start = *self.at.get(v as usize).ok_or(Fault::Lost)?;
        let mut out = Vec::new();
        let mut t = start;
        for _ in 0..=self.tris.len() {
            let tri = self.tri(t)?;
            let i = tri.v.iter().position(|&x| x == v).ok_or(Fault::Lost)?;
            out.push(Round {
                t,
                i,
                v: turn(tri.v, i),
            });
            // Across the side (v, tri.v[i + 2]): the next triangle counter-clockwise.
            t = nth(tri.n, i + 1);
            if t == start {
                return Ok(out);
            }
            if t == NONE {
                return Err(Fault::Lost);
            }
        }
        Err(Fault::Lost)
    }

    /// The triangle with the side from `x` to `y` counter-clockwise, and the corner opposite.
    /// A crossed side may end at the enclosing triangle's corner, whose fan is open, so
    /// that end is looked up from the other.
    fn side(&self, x: u32, y: u32) -> Result<Option<(u32, usize)>, Fault> {
        Ok(if x < self.n {
            self.fan(x)?
                .into_iter()
                .find(|r| r.v[1] == y)
                .map(|r| (r.t, (r.i + 2) % 3))
        } else {
            self.fan(y)?
                .into_iter()
                .find(|r| r.v[2] == x)
                .map(|r| (r.t, (r.i + 1) % 3))
        })
    }

    /// Whether `w`, collinear with `a` and `b`, lies on the ray from `a` through `b`.
    fn ahead(&self, a: u32, b: u32, w: u32) -> Result<bool, Fault> {
        let (a, b, w) = (self.pt(a)?, self.pt(b)?, self.pt(w)?);
        Ok(if a.x == b.x {
            (w.y > a.y) == (b.y > a.y)
        } else {
            (w.x > a.x) == (b.x > a.x)
        })
    }

    fn mark(&mut self, t: u32, k: usize) -> Result<(), Fault> {
        let u = nth(self.tri(t)?.n, k);
        self.tri_mut(t)?.bump(k);
        if u == NONE {
            return Err(Fault::Lost);
        }
        let (_, j) = self.facing(t, u)?;
        self.tri_mut(u)?.bump(j);
        Ok(())
    }

    fn constrain(&mut self, a: u32, b: u32, budget: &mut Budget) -> Result<(), Fault> {
        let mut todo = vec![(a, b)];
        while let Some((a, b)) = todo.pop() {
            if let Some((t, k)) = self.side(a, b)? {
                self.mark(t, k)?;
                continue;
            }
            match self.crossed(a, b)? {
                Walk::Hit(w) => todo.extend([(w, b), (a, w)]),
                Walk::Crossed(sides) => {
                    let made = self.clear(a, b, sides, budget)?;
                    let (t, k) = self.side(a, b)?.ok_or(Fault::Lost)?;
                    self.mark(t, k)?;
                    let mut again = Vec::new();
                    for (x, y) in made {
                        if let Some(s) = self.side(x, y)? {
                            again.push(s);
                        }
                    }
                    self.legalize(again, true, budget)?;
                }
            }
        }
        Ok(())
    }

    /// The sides the segment from `a` to `b` crosses, each as (right end, left end), or the
    /// first point it runs through.
    fn crossed(&self, a: u32, b: u32) -> Result<Walk, Fault> {
        let mut start = None;
        for r in self.fan(a)? {
            let [_, y, z] = r.v;
            let (sy, sz) = (self.orient(a, b, y)?, self.orient(a, b, z)?);
            if sy == Ordering::Equal && self.ahead(a, b, y)? {
                return Ok(Walk::Hit(y));
            }
            if sz == Ordering::Equal && self.ahead(a, b, z)? {
                return Ok(Walk::Hit(z));
            }
            if sy == Ordering::Less && sz == Ordering::Greater {
                start = Some(r);
                break;
            }
        }
        let Round {
            mut t,
            i: mut k,
            v: [_, mut r, mut l],
        } = start.ok_or(Fault::Lost)?;
        let mut sides = Vec::new();
        for _ in 0..=self.tris.len() {
            let Tri {
                n: [u, ..],
                c: [m, ..],
                ..
            } = self.tri(t)?.turned(k);
            if m > 0 {
                return Err(Fault::Crossing);
            }
            sides.push((r, l));
            if u == NONE {
                return Err(Fault::Lost);
            }
            let (ut, j) = self.facing(t, u)?;
            let [w, ..] = ut.v;
            if w == b {
                return Ok(Walk::Crossed(sides));
            }
            // u is (w, l, r).
            match self.orient(a, b, w)? {
                Ordering::Equal => return Ok(Walk::Hit(w)),
                // Next the side (r, w), opposite l.
                Ordering::Greater => {
                    l = w;
                    k = (j + 1) % 3;
                }
                // Next the side (w, l), opposite r.
                Ordering::Less => {
                    r = w;
                    k = (j + 2) % 3;
                }
            }
            t = u;
        }
        Err(Fault::Lost)
    }

    /// Flips the crossed sides away (Sloan); returns the sides made that do not cross.
    fn clear(
        &mut self,
        a: u32,
        b: u32,
        sides: Vec<(u32, u32)>,
        budget: &mut Budget,
    ) -> Result<Vec<(u32, u32)>, Fault> {
        let cap = 16 * sides.len() * sides.len() + 64;
        let mut queue: VecDeque<(u32, u32)> = sides.into();
        let mut made = Vec::new();
        let mut turns = 0;
        while let Some((x, y)) = queue.pop_front() {
            turns += 1;
            if turns > cap {
                return Err(Fault::Lost);
            }
            let (t, k) = match self.side(x, y)? {
                Some(s) => s,
                None => self.side(y, x)?.ok_or(Fault::Lost)?,
            };
            let Tri {
                v: [p, x, y],
                n: [u, ..],
                ..
            } = self.tri(t)?.turned(k);
            let [q, ..] = self.facing(t, u)?.0.v;
            // The quadrilateral (p, x, q, y), counter-clockwise; strictly convex when both
            // new triangles are.
            let convex = self.orient(p, x, q)? == Ordering::Greater
                && self.orient(q, y, p)? == Ordering::Greater;
            if !convex {
                queue.push_back((x, y));
                continue;
            }
            self.flip(t, k, budget)?;
            let (sp, sq) = (self.orient(a, b, p)?, self.orient(a, b, q)?);
            let crosses = (sp == Ordering::Less && sq == Ordering::Greater)
                || (sp == Ordering::Greater && sq == Ordering::Less);
            if crosses {
                queue.push_back((p, q));
            } else {
                made.push((p, q));
            }
        }
        Ok(made)
    }

    /// The triangles on the odd side of the constraints, input corners only.
    fn inside(&self) -> Result<Vec<[u32; 3]>, Fault> {
        const UNSEEN: u8 = 2;
        let mut side = vec![UNSEEN; self.tris.len()];
        let start = *self.at.get(self.n as usize).ok_or(Fault::Lost)?;
        *side.get_mut(start as usize).ok_or(Fault::Lost)? = 0;
        let mut stack = vec![start];
        while let Some(t) = stack.pop() {
            let tri = self.tri(t)?;
            let s = *side.get(t as usize).ok_or(Fault::Lost)?;
            for (u, c) in tri.n.into_iter().zip(tri.c) {
                if u == NONE {
                    continue;
                }
                let want = s ^ (c & 1);
                let slot = side.get_mut(u as usize).ok_or(Fault::Lost)?;
                match *slot {
                    UNSEEN => {
                        *slot = want;
                        stack.push(u);
                    }
                    got if got != want => return Err(Fault::Parity),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for (tri, &s) in self.tris.iter().zip(&side) {
            if s == 1 {
                if tri.v.iter().any(|&v| v >= self.n) {
                    return Err(Fault::Parity);
                }
                out.push(tri.v);
            }
        }
        Ok(out)
    }
}

enum Walk {
    Hit(u32),
    Crossed(Vec<(u32, u32)>),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
    use std::collections::HashMap;

    use super::*;
    use scx_num::fmath;
    use scx_num::hash::Rng;
    use scx_num::predicates::orient2d;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    /// The closed rings as points and segments.
    fn rings(rings: &[Vec<Point2>]) -> (Vec<Point2>, Vec<[u32; 2]>) {
        let (mut pts, mut segs) = (Vec::new(), Vec::new());
        for r in rings {
            let base = pts.len() as u32;
            pts.extend_from_slice(r);
            let n = r.len() as u32;
            segs.extend((0..n).map(|k| [base + k, base + (k + 1) % n]));
        }
        (pts, segs)
    }

    fn run(pts: &[Point2], segs: &[[u32; 2]]) -> Result<Vec<[u32; 3]>, Fault> {
        triangulate(pts, segs, &mut Budget::unlimited())
    }

    fn shoelace(r: &[Point2]) -> f64 {
        (0..r.len())
            .map(|k| {
                let (a, b) = (r[k], r[(k + 1) % r.len()]);
                a.x * b.y - a.y * b.x
            })
            .sum::<f64>()
            / 2.0
    }

    /// Every triangle strictly counter-clockwise, each side used once each way at most,
    /// every segment a side, every side not a segment locally Delaunay, and the count.
    fn check(pts: &[Point2], segs: &[[u32; 2]], tris: &[[u32; 3]], count: usize) {
        assert_eq!(tris.len(), count);
        let mut sides: HashMap<(u32, u32), u32> = HashMap::new();
        for (t, &[a, b, c]) in tris.iter().enumerate() {
            let [pa, pb, pc] = [a, b, c].map(|i| pts[i as usize]);
            assert_eq!(orient2d(pa, pb, pc), Ordering::Greater, "{t}");
            for (x, y) in [(a, b), (b, c), (c, a)] {
                assert!(sides.insert((x, y), t as u32).is_none(), "side twice");
            }
        }
        let constrained = |x: u32, y: u32| segs.iter().any(|s| *s == [x, y] || *s == [y, x]);
        for &[a, b] in segs {
            assert!(
                sides.contains_key(&(a, b)) || sides.contains_key(&(b, a)),
                "segment {a} {b}"
            );
        }
        for (&(x, y), &t) in &sides {
            let Some(&u) = sides.get(&(y, x)) else {
                continue;
            };
            if constrained(x, y) {
                continue;
            }
            let apex = |t: u32, x: u32, y: u32| {
                *tris[t as usize]
                    .iter()
                    .find(|&&v| v != x && v != y)
                    .unwrap()
            };
            let (c, d) = (apex(t, x, y), apex(u, x, y));
            let at = |i: u32| pts[i as usize];
            assert_ne!(
                incircle(at(x), at(y), at(c), at(d)),
                Ordering::Greater,
                "side {x} {y} not Delaunay"
            );
        }
    }

    fn area(pts: &[Point2], tris: &[[u32; 3]]) -> f64 {
        tris.iter()
            .map(|t| shoelace(&t.map(|i| pts[i as usize])))
            .sum()
    }

    /// The triangles as coordinates, each from its least corner, sorted: what a run means
    /// whatever the indices.
    fn shape(pts: &[Point2], tris: &[[u32; 3]]) -> Vec<[(u64, u64); 3]> {
        let key = |i: u32| (pts[i as usize].x.to_bits(), pts[i as usize].y.to_bits());
        let mut out: Vec<_> = tris
            .iter()
            .map(|t| {
                let k = t.map(key);
                let m = (0..3).min_by_key(|&i| k[i]).unwrap();
                [k[m], k[(m + 1) % 3], k[(m + 2) % 3]]
            })
            .collect();
        out.sort_unstable();
        out
    }

    fn star(rng: &mut Rng, n: usize, centre: Point2, r: f64) -> Vec<Point2> {
        (0..n)
            .map(|k| {
                let a = core::f64::consts::TAU * k as f64 / n as f64;
                let s = r * (0.3 + 0.7 * rng.next_f64());
                p(centre.x + s * fmath::cos(a), centre.y + s * fmath::sin(a))
            })
            .collect()
    }

    #[test]
    fn a_square_is_two_triangles() {
        let (pts, segs) = rings(&[vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)]]);
        let tris = run(&pts, &segs).unwrap();
        check(&pts, &segs, &tris, 2);
        assert_eq!(area(&pts, &tris), 1.0);
    }

    #[test]
    fn stars_with_holes_hold_every_rule() {
        let mut rng = Rng::new(0x7a2);
        for case in 0..60 {
            let n = 5 + case * 3;
            let outer = star(&mut rng, n, p(0.0, 0.0), 10.0);
            let mut all = vec![outer];
            // Holes: small clockwise squares well inside the star's least radius.
            let holes = case % 4;
            for h in 0..holes {
                let c = p(-1.5 + 1.5 * h as f64, 0.2 * h as f64);
                let s = 0.4;
                all.push(vec![
                    p(c.x - s, c.y - s),
                    p(c.x - s, c.y + s),
                    p(c.x + s, c.y + s),
                    p(c.x + s, c.y - s),
                ]);
            }
            let (pts, segs) = rings(&all);
            let tris = run(&pts, &segs).unwrap();
            check(&pts, &segs, &tris, pts.len() - 2 + 2 * holes);
            let truth: f64 = all.iter().map(|r| shoelace(r)).sum();
            assert!((area(&pts, &tris) - truth).abs() < 1e-9 * truth, "{case}");
        }
    }

    #[test]
    fn a_comb_needs_many_flips_to_constrain() {
        // Long thin teeth: every tooth's sides cross the unconstrained Delaunay edges.
        let mut ring = vec![p(0.0, 0.0), p(41.0, 0.0)];
        for k in (0..20).rev() {
            let x = 2.0 * f64::from(k);
            ring.extend([
                p(x + 2.0, 30.0),
                p(x + 1.0, 30.0),
                p(x + 1.0, 1.0),
                p(x + 0.5, 1.0),
            ]);
        }
        ring.pop();
        ring.push(p(0.0, 30.0));
        let truth = shoelace(&ring);
        assert!(truth > 0.0);
        let (pts, segs) = rings(&[ring]);
        let tris = run(&pts, &segs).unwrap();
        check(&pts, &segs, &tris, pts.len() - 2);
        assert!((area(&pts, &tris) - truth).abs() < 1e-9 * truth);
    }

    #[test]
    fn the_result_is_a_function_of_the_point_and_segment_sets() {
        let mut rng = Rng::new(5);
        let outer = star(&mut rng, 40, p(3.0, -2.0), 6.0);
        let hole = vec![p(2.5, -2.5), p(2.5, -1.5), p(3.5, -1.5), p(3.5, -2.5)];
        let (pts, segs) = rings(&[outer.clone(), hole.clone()]);
        let want = shape(&pts, &run(&pts, &segs).unwrap());
        for turn in 1..8 {
            // Rotate the rings, permute the indices, shuffle and flip the segments.
            let mut o = outer.clone();
            o.rotate_left(turn * 3);
            let mut h = hole.clone();
            h.rotate_left(turn % 4);
            let (pts, segs) = rings(&[h, o]);
            let n = pts.len();
            let perm: Vec<usize> = (0..n).map(|i| (i * 7 + turn) % n).collect();
            let mut moved = vec![p(0.0, 0.0); n];
            for (i, &j) in perm.iter().enumerate() {
                moved[j] = pts[i];
            }
            let mut s: Vec<[u32; 2]> = segs
                .iter()
                .map(|&[a, b]| [perm[a as usize] as u32, perm[b as usize] as u32])
                .map(|[a, b]| if turn % 2 == 0 { [b, a] } else { [a, b] })
                .collect();
            s.reverse();
            assert_eq!(shape(&moved, &run(&moved, &s).unwrap()), want, "{turn}");
        }
    }

    #[test]
    fn collinear_runs_are_kept_whole() {
        // A rectangle with 30 points along each side: no triangle is flat.
        let mut ring = Vec::new();
        let k = 30;
        for i in 0..k {
            ring.push(p(f64::from(i) / f64::from(k) * 3.0, 0.0));
        }
        for i in 0..k {
            ring.push(p(3.0, f64::from(i) / f64::from(k)));
        }
        for i in 0..k {
            ring.push(p(3.0 - f64::from(i) / f64::from(k) * 3.0, 1.0));
        }
        for i in 0..k {
            ring.push(p(0.0, 1.0 - f64::from(i) / f64::from(k)));
        }
        let (pts, segs) = rings(&[ring]);
        let tris = run(&pts, &segs).unwrap();
        check(&pts, &segs, &tris, pts.len() - 2);
        assert!((area(&pts, &tris) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn cocircular_points_and_grids_triangulate_deterministically() {
        // A regular 24-gon (cocircular to rounding, often exactly in pairs) and a square
        // grid (every cell's corners exactly cocircular), as free points in a square.
        let n = 24;
        let gon: Vec<Point2> = (0..n)
            .map(|k| {
                let a = core::f64::consts::TAU * f64::from(k) / f64::from(n);
                p(fmath::cos(a), fmath::sin(a))
            })
            .collect();
        let (pts, segs) = rings(core::slice::from_ref(&gon));
        let tris = run(&pts, &segs).unwrap();
        check(&pts, &segs, &tris, 22);
        let mut turned = gon;
        turned.rotate_left(5);
        let (pts2, segs2) = rings(&[turned]);
        assert_eq!(
            shape(&pts, &tris),
            shape(&pts2, &run(&pts2, &segs2).unwrap())
        );

        let m = 8;
        let side = |i: u32| f64::from(i);
        let mut ring = Vec::new();
        for i in 0..m {
            ring.push(p(side(i), 0.0));
        }
        for i in 0..m {
            ring.push(p(side(m), side(i)));
        }
        for i in 0..m {
            ring.push(p(side(m - i), side(m)));
        }
        for i in 0..m {
            ring.push(p(0.0, side(m - i)));
        }
        let (mut pts, segs) = rings(&[ring]);
        for i in 1..m {
            for j in 1..m {
                pts.push(p(side(i), side(j)));
            }
        }
        let tris = run(&pts, &segs).unwrap();
        // 2n - 2 - B for n points with B on the hull: every cell is two triangles.
        let (all, hull) = (pts.len(), 4 * m as usize);
        check(&pts, &segs, &tris, 2 * all - 2 - hull);
        assert_eq!(tris.len(), 2 * (m * m) as usize);
        let mut rev = pts.clone();
        rev.reverse();
        let last = pts.len() as u32 - 1;
        let segs_rev: Vec<[u32; 2]> = segs.iter().map(|&[a, b]| [last - a, last - b]).collect();
        assert_eq!(
            shape(&pts, &tris),
            shape(&rev, &run(&rev, &segs_rev).unwrap())
        );
    }

    #[test]
    fn near_duplicate_points_one_ulp_apart_are_distinct() {
        let e = 1.0_f64.next_up();
        let ring = vec![
            p(0.0, 0.0),
            p(1.0, 0.0),
            p(e, 0.5),
            p(1.0, 1.0),
            p(0.0, 1.0),
        ];
        let (pts, segs) = rings(core::slice::from_ref(&ring));
        let tris = run(&pts, &segs).unwrap();
        check(&pts, &segs, &tris, 3);
        // A notch one ulp deep, the other way.
        let d = 1.0_f64.next_down();
        let ring = vec![
            p(0.0, 0.0),
            p(1.0, 0.0),
            p(1.0, 0.4),
            p(d, 0.5),
            p(1.0, 0.6),
            p(1.0, 1.0),
            p(0.0, 1.0),
        ];
        let (pts, segs) = rings(&[ring]);
        let tris = run(&pts, &segs).unwrap();
        check(&pts, &segs, &tris, 5);
    }

    #[test]
    fn a_point_on_a_segment_splits_it() {
        // The hole's corner touches the outer ring's bottom side between its ends.
        let pts = vec![
            p(0.0, 0.0),
            p(4.0, 0.0),
            p(4.0, 4.0),
            p(0.0, 4.0),
            p(2.0, 0.0),
            p(1.0, 1.0),
            p(3.0, 1.0),
        ];
        // Outer ring 0-1-2-3; hole 4-5-6 (clockwise) touching at 4.
        let segs = vec![[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 4]];
        let tris = run(&pts, &segs).unwrap();
        assert!((area(&pts, &tris) - (16.0 - 1.0)).abs() < 1e-12);
        for &[a, b, c] in &tris {
            assert_eq!(
                orient2d(pts[a as usize], pts[b as usize], pts[c as usize]),
                Ordering::Greater
            );
        }
    }

    #[test]
    fn a_slit_used_twice_keeps_both_sides() {
        let pts = vec![
            p(0.0, 0.0),
            p(2.0, 0.0),
            p(2.0, 2.0),
            p(0.0, 2.0),
            p(1.0, 1.0),
        ];
        let segs = vec![[0, 1], [1, 2], [2, 4], [4, 2], [2, 3], [3, 0]];
        let tris = run(&pts, &segs).unwrap();
        assert!((area(&pts, &tris) - 4.0).abs() < 1e-12);
        assert_eq!(tris.len(), 4);
    }

    #[test]
    fn crossings_and_duplicates_are_refused() {
        let pts = vec![p(0.0, 0.0), p(2.0, 2.0), p(0.0, 2.0), p(2.0, 0.0)];
        assert_eq!(run(&pts, &[[0, 1], [2, 3]]), Err(Fault::Crossing));
        let pts = vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 0.0), p(0.0, 1.0)];
        assert_eq!(run(&pts, &[[0, 1], [2, 3], [3, 0]]), Err(Fault::Duplicate));
        let pts = vec![p(-0.0, 0.0), p(0.0, 0.0), p(1.0, 1.0)];
        assert_eq!(run(&pts, &[]), Err(Fault::Duplicate));
    }

    #[test]
    fn the_budget_bounds_the_work() {
        let mut rng = Rng::new(9);
        let (pts, segs) = rings(&[star(&mut rng, 50, p(0.0, 0.0), 1.0)]);
        let mut b = Budget::unlimited();
        triangulate(&pts, &segs, &mut b).unwrap();
        let used = b.used();
        assert!(used >= 50);
        assert!(matches!(
            triangulate(&pts, &segs, &mut Budget::new(used - 1)),
            Err(Fault::Exhausted(_))
        ));
        triangulate(&pts, &segs, &mut Budget::new(used)).unwrap();
    }
}
