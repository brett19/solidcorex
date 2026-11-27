// Written for solidcorex. Apache-2.0.
//! Contacts between two segments: proved apart, proved crossing, or neither (touching).
//!
//! Two segments are apart when their distance is proved above the resolution, away from
//! the joints they share. The distance between two segments is attained at an end of one
//! of them, at a point where they meet, or at an interior critical pair whose connecting
//! line is normal to both. Each of these is an *event* with an interval location, a
//! verdict on whether it lies inside both segments, and a verdict on whether its distance
//! is above the resolution; the pair is apart when every event is proved outside, proved
//! far, or (for a shared joint) proved within the joint's neighbourhood. A meeting point
//! proved inside both segments where the two proved cross transversally is a crossing.
//!
//! Lines decide by the exact orientation sign; arcs by intervals rounded outward. A whole
//! circle is an arc whose angular range holds every direction; its seam point stands for
//! both its ends, which is sound: the seam lies on the circle, so a test of it against the
//! other segment is a test of a point of the circle. An arc's
//! radius is the band between its two end distances, so a decision holds for every circle
//! the arc may stand for.

use scx_num::certified::Verdict;
use scx_num::frame::Sense;
use scx_num::vector2d::Point2;

use crate::cert::{IP, Iv, greater, not, or, orient};
use crate::segment::Segment;

/// A segment prepared for the tests: a line with its ends in lexicographic order, or an
/// arc taken counter-clockwise, so a pair's answer does not depend on how a loop runs.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Shape {
    Line(LineG),
    Arc(ArcG),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LineG {
    pub(crate) a: Point2,
    pub(crate) b: Point2,
    ia: IP,
    d: IP,
    len2: Iv,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArcG {
    pub(crate) c: Point2,
    pub(crate) s: Point2,
    pub(crate) e: Point2,
    ic: IP,
    u: IP,
    w: IP,
    uw: Iv,
    /// The radius band: every radius between the two end distances.
    pub(crate) r: Iv,
    /// A whole circle: every direction is inside it, and `s` and `e` are its seam.
    full: bool,
}

impl Shape {
    pub(crate) fn new(seg: &Segment) -> Self {
        match *seg {
            Segment::Line { start, end } => {
                let (a, b) = if (start.x, start.y) <= (end.x, end.y) {
                    (start, end)
                } else {
                    (end, start)
                };
                let ia = IP::exact(a);
                let d = IP::exact(b).sub(ia);
                Self::Line(LineG {
                    a,
                    b,
                    ia,
                    d,
                    len2: d.norm2(),
                })
            }
            Segment::Arc {
                centre,
                start,
                end,
                sense,
            } => {
                let (s, e) = match sense {
                    Sense::Forward => (start, end),
                    Sense::Reversed => (end, start),
                };
                let ic = IP::exact(centre);
                let u = IP::exact(s).sub(ic);
                let w = IP::exact(e).sub(ic);
                Self::Arc(ArcG {
                    c: centre,
                    s,
                    e,
                    ic,
                    u,
                    w,
                    uw: u.cross(w),
                    r: u.norm2().sqrt().hull(w.norm2().sqrt()),
                    full: false,
                })
            }
            Segment::Circle { centre, radius, .. } => {
                // The seam is the circle's one point given as data; the band holds both the
                // radius and the seam's distance, which differ by its rounding.
                let s = seg.start();
                let ic = IP::exact(centre);
                let u = IP::exact(s).sub(ic);
                Self::Arc(ArcG {
                    c: centre,
                    s,
                    e: s,
                    ic,
                    u,
                    w: u,
                    uw: u.cross(u),
                    r: Iv::exact(radius).hull(u.norm2().sqrt()),
                    full: true,
                })
            }
        }
    }

    pub(crate) const fn ends(&self) -> [Point2; 2] {
        match self {
            Self::Line(l) => [l.a, l.b],
            Self::Arc(a) => [a.s, a.e],
        }
    }

    /// A box holding every point the segment may stand for, grown by `pad` each way:
    /// `[x_lo, x_hi, y_lo, y_hi]`.
    pub(crate) fn bbox(&self, pad: f64) -> [f64; 4] {
        let [p, q] = self.ends();
        let mut b = [p.x.min(q.x), p.x.max(q.x), p.y.min(q.y), p.y.max(q.y)];
        if let Self::Arc(a) = self {
            let r = a.r.hi();
            let axes = [
                (
                    IP::exact(Point2::new(1.0, 0.0)),
                    [a.c.x + r, a.c.x + r, a.c.y, a.c.y],
                ),
                (
                    IP::exact(Point2::new(-1.0, 0.0)),
                    [a.c.x - r, a.c.x - r, a.c.y, a.c.y],
                ),
                (
                    IP::exact(Point2::new(0.0, 1.0)),
                    [a.c.x, a.c.x, a.c.y + r, a.c.y + r],
                ),
                (
                    IP::exact(Point2::new(0.0, -1.0)),
                    [a.c.x, a.c.x, a.c.y - r, a.c.y - r],
                ),
            ];
            for (dir, ext) in axes {
                if a.contains_dir(dir) != Verdict::No {
                    b = [
                        b[0].min(ext[0]),
                        b[1].max(ext[1]),
                        b[2].min(ext[2]),
                        b[3].max(ext[3]),
                    ];
                }
            }
        }
        let scale = 1e-14 * (1.0 + b.iter().fold(0.0_f64, |m, v| m.max(v.abs())));
        let g = pad + scale;
        [b[0] - g, b[1] + g, b[2] - g, b[3] + g]
    }
}

impl ArcG {
    /// The direction `q` from the centre lies strictly inside the arc's angular range.
    pub(crate) fn contains_dir(&self, q: IP) -> Verdict {
        if self.full {
            return Verdict::Yes;
        }
        let a = self.u.cross(q).pos();
        let b = q.cross(self.w).pos();
        let minor = a.and(b);
        let major = or(a, b);
        match self.uw.pos() {
            Verdict::Yes => minor,
            _ if self.uw.neg_sign() == Verdict::Yes => major,
            // A half circle (or not decided between): both forms agree off its diameter.
            _ if minor == major => minor,
            _ => Verdict::Undecided,
        }
    }

    /// A direction from the centre strictly inside the arc.
    fn mid_dir(&self) -> IP {
        if self.full {
            return self.u.perp();
        }
        let uh = self.u.scale(Iv::exact(1.0).div(self.u.norm2().sqrt()));
        let wh = self.w.scale(Iv::exact(1.0).div(self.w.norm2().sqrt()));
        if self.uw.pos() == Verdict::Yes {
            uh.add(wh)
        } else if self.uw.neg_sign() == Verdict::Yes {
            uh.add(wh).neg()
        } else {
            self.u.perp()
        }
    }
}

/// Whether the point `p` is proved more than `res` from the segment.
pub(crate) fn point_apart(p: IP, s: &Shape, res: Iv) -> Verdict {
    let res2 = res.sq();
    match s {
        Shape::Line(l) => {
            let m = p.sub(l.ia);
            let ends = greater(m.norm2(), res2).and(greater(p.sub(l.ia.add(l.d)).norm2(), res2));
            let t = m.dot(l.d);
            let outside = or(t.neg_sign(), greater(t, l.len2));
            let perp = greater(m.cross(l.d).sq(), res2 * l.len2);
            ends.and(or(perp, outside))
        }
        Shape::Arc(a) => {
            let ends = greater(p.sub(IP::exact(a.s)).norm2(), res2)
                .and(greater(p.sub(IP::exact(a.e)).norm2(), res2));
            let q = p.sub(a.ic);
            ends.and(or(radial_far(q.norm2(), a.r, res), not(a.contains_dir(q))))
        }
    }
}

/// `|sqrt(rho2) - r| > res` for every radius `r` in the band.
fn radial_far(rho2: Iv, r: Iv, res: Iv) -> Verdict {
    let outside = greater(rho2, (r + res).sq());
    let inner = r - res;
    let inside = inner.pos().and(greater(inner.sq(), rho2));
    or(outside, inside)
}

/// What two segments do to each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Contact {
    /// Proved more than the resolution apart, away from shared joints.
    Apart,
    /// Not proved apart, and not proved to cross.
    Touching,
    /// Proved to cross transversally at a point interior to both.
    Crossing,
}

/// The running verdict over a pair's events.
struct Acc<'a> {
    joints: &'a [Point2],
    rho2: Iv,
    apart: Verdict,
    crossing: bool,
}

impl Acc<'_> {
    /// An event at `at`, inside both segments by `inside`, its distance above the
    /// resolution by `far`; `crosses` when the segments are proved transversal there.
    fn event(&mut self, inside: Verdict, far: Verdict, at: IP, crosses: bool) {
        if inside == Verdict::No || self.at_joint(at) {
            return;
        }
        if crosses && inside == Verdict::Yes {
            self.crossing = true;
        } else {
            self.apart = self.apart.and(far);
        }
    }

    fn at_joint(&self, at: IP) -> bool {
        self.joints
            .iter()
            .any(|j| greater(self.rho2, at.sub(IP::exact(*j)).norm2()) == Verdict::Yes)
    }

    /// The ends of `of` that are not shared joints, against `other`.
    fn ends(&mut self, of: &Shape, other: &Shape, res: Iv) {
        for p in of.ends() {
            if !self.joints.contains(&p) {
                self.apart = self.apart.and(point_apart(IP::exact(p), other, res));
            }
        }
    }

    fn contact(&self) -> Contact {
        if self.crossing {
            Contact::Crossing
        } else if self.apart == Verdict::Yes {
            Contact::Apart
        } else {
            Contact::Touching
        }
    }
}

/// The contact between `a` and `b`, which share exactly the points `joints` as joints
/// (empty unless adjacent in a loop). Events within `rho` of a shared joint belong to the
/// joint: `rho = 4 sqrt(R res) + 4 res` for the larger radius `R` in the pair, the reach of
/// a tangent joint's ambiguity (an arc stays within `res` of its tangent for
/// `sqrt(2 R res)`).
pub(crate) fn contact(a: &Shape, b: &Shape, joints: &[Point2], res: f64) -> Contact {
    let ires = Iv::exact(res);
    let rmax = [a, b]
        .iter()
        .map(|s| match s {
            Shape::Line(_) => 0.0,
            Shape::Arc(g) => g.r.hi(),
        })
        .fold(0.0_f64, f64::max);
    let rho = 4.0 * (rmax * res).sqrt() + 4.0 * res;
    let mut acc = Acc {
        joints,
        rho2: Iv::exact(rho * rho),
        apart: Verdict::Yes,
        crossing: false,
    };
    match (a, b) {
        (Shape::Line(l), Shape::Line(m)) => line_line(l, m, &mut acc),
        (Shape::Line(l), Shape::Arc(c)) | (Shape::Arc(c), Shape::Line(l)) => {
            line_arc(l, c, ires, &mut acc);
        }
        (Shape::Arc(p), Shape::Arc(q)) => {
            if arc_arc(p, q, ires, &mut acc) {
                return Contact::Apart;
            }
        }
    }
    if acc.crossing {
        return Contact::Crossing;
    }
    acc.ends(a, b, ires);
    acc.ends(b, a, ires);
    acc.contact()
}

/// Two lines meet at most once unless collinear; a proper crossing is decided exactly, and
/// any other meeting puts an end of one on the other, which the end tests see.
fn line_line(l: &LineG, m: &LineG, acc: &mut Acc<'_>) {
    let o1 = orient(l.a, l.b, m.a);
    let o2 = orient(l.a, l.b, m.b);
    let o3 = orient(m.a, m.b, l.a);
    let o4 = orient(m.a, m.b, l.b);
    if o1 * o2 < 0 && o3 * o4 < 0 {
        acc.crossing = true;
    }
}

fn in_unit(t: Iv) -> Verdict {
    t.pos().and((Iv::exact(1.0) - t).pos())
}

fn line_arc(l: &LineG, c: &ArcG, res: Iv, acc: &mut Acc<'_>) {
    let m = l.ia.sub(c.ic);
    let b = m.dot(l.d);
    let cr = l.d.cross(m);
    let cr2 = cr.sq();
    let disc = l.len2 * c.r.sq() - cr2;
    // Where the line meets the circle.
    if disc.hi() >= 0.0 {
        let root = disc.sqrt();
        let secant = disc.pos() == Verdict::Yes;
        for t in [(-b - root).div(l.len2), (-b + root).div(l.len2)] {
            let x = l.ia.add(l.d.scale(t));
            let inside = in_unit(t).and(c.contains_dir(x.sub(c.ic)));
            acc.event(inside, Verdict::No, x, secant);
        }
    }
    // The foot of the centre on the line: the critical pair.
    let t = (-b).div(l.len2);
    let f = l.ia.add(l.d.scale(t));
    let inside = in_unit(t).and(c.contains_dir(f.sub(c.ic)));
    let out = greater(cr2, l.len2 * (c.r + res).sq());
    let inner = c.r - res;
    let within = inner.pos().and(greater(l.len2 * inner.sq(), cr2));
    acc.event(inside, or(out, within), f, false);
}

/// The events of two arcs; `true` when they are proved apart outright (concentric, radii
/// more than the resolution apart).
fn arc_arc(p: &ArcG, q: &ArcG, res: Iv, acc: &mut Acc<'_>) -> bool {
    if p.c == q.c {
        let dr = p.r - q.r;
        let far = or(greater(dr, res), greater(-dr, res));
        if far == Verdict::Yes {
            return true;
        }
        // Concentric: the distance is least where the ranges overlap; an overlap without
        // an end of one inside the other is the same or the complementary range, which an
        // interior direction of each tells apart.
        for (of, inside) in [(q, p), (p, q)] {
            let dir = of.mid_dir();
            let at = of.ic.add(dir.scale(of.r.div(dir.norm2().sqrt())));
            acc.event(inside.contains_dir(dir), far, at, false);
        }
        return false;
    }
    let v = IP::exact(q.c).sub(p.ic);
    let d2 = v.norm2();
    let d = d2.sqrt();
    let (r1, r2) = (p.r, q.r);
    let k = d2 + r1.sq() - r2.sq();
    let disc = Iv::exact(4.0) * r1.sq() * d2 - k.sq();
    if disc.hi() >= 0.0 {
        let two_d2 = Iv::exact(2.0) * d2;
        let along = v.scale(k.div(two_d2));
        let root = disc.sqrt().div(two_d2);
        let secant = disc.pos() == Verdict::Yes;
        for side in [v.perp().scale(root), v.perp().scale(root).neg()] {
            let x = p.ic.add(along).add(side);
            let inside = p.contains_dir(x.sub(p.ic)).and(q.contains_dir(x.sub(q.ic)));
            acc.event(inside, Verdict::No, x, secant);
        }
    }
    // The critical pairs on the line of centres.
    for s1 in [1.0, -1.0] {
        for s2 in [1.0, -1.0] {
            let (k1, k2) = (Iv::exact(s1), Iv::exact(s2));
            let inside = p.contains_dir(v.scale(k1)).and(q.contains_dir(v.scale(k2)));
            let gap = d + k2 * r2 - k1 * r1;
            let far = or(greater(gap, res), greater(-gap, res));
            let at = p.ic.add(v.scale((k1 * r1).div(d)));
            acc.event(inside, far, at, false);
        }
    }
    false
}
