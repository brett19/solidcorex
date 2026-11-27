// Written for solidcorex. Apache-2.0.
//! Closed line-and-arc profiles in the plane: the cross-sections that
//! [`super::Primitive::Extrusion`] sweeps.
//!
//! The function is the signed distance to the profile's curves, negative inside, with
//! inside meaning odd winding (so nested loops make holes, whatever their senses). It is
//! computed at one point in double-double; its enclosure over a box comes from the
//! distance being 1-Lipschitz (see [`Profile`]).

use super::CsgError;
use super::ivl::Enclosure;
use crate::dd::{self, Dd};
use crate::profile::Seg;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Line,
    /// The arc about `c` of radius `r` from angle `start` through `sweep` (positive) in
    /// the sense `s` (`+1` counter-clockwise, `-1` clockwise).
    Arc {
        c: [Dd; 2],
        r: Dd,
        start: Dd,
        sweep: Dd,
        s: f64,
    },
    /// The whole circle, travelled in the sense `s` from angle `start`.
    Circle {
        c: [Dd; 2],
        r: Dd,
        start: Dd,
        s: f64,
    },
}

/// A validated piece: its kind and its two ends in double-double.
#[derive(Clone, Copy, Debug)]
struct Piece {
    kind: Kind,
    a: [Dd; 2],
    b: [Dd; 2],
}

/// A closed profile: loops of [`Seg`]s, each loop's pieces end to end. A whole circle is an
/// arc with equal ends, and a loop on its own.
///
/// **Its function.** `g(q) = +-dist(q, C)`, where `C` is the union of the pieces and of the
/// chords bridging each junction gap (at most [`Profile::gap`]), negative when the winding
/// number of the loops about `q` is odd. `C` is closed and `g` is continuous, so
/// `g = 0` on the region's boundary.
///
/// **Its enclosure over a box `B`** (centre `c`, half-diagonal `rho`) is
/// `g~(c) +- (rho + E)`, where `g~` is `g` computed at `c` and `E` bounds `|g~(c) - g(c)|`.
/// It holds every `g(q)`, `q` in `B`, because the signed distance to a closed set is
/// 1-Lipschitz: on one side of `C`, `|dist(a) - dist(b)| <= |a - b|`; on opposite sides
/// the segment `ab` meets `C` at some `t`, and `dist(a) + dist(b) <= |a - t| + |t - b| =
/// |a - b|`. The sign needs only the parity at `c` to be right when `|g~(c)| > rho + E`:
/// then `c` is farther than `rho > 0` from `C`, and the winding sum, an exact multiple of
/// `2 pi` in reals, is computed to far better than `pi` (each angle term in
/// double-double, conditioned by `length / distance`).
///
/// `E` is the junction gap (the distance to the pieces exceeds that to `C` by at most it)
/// plus `2^-90 (S + |c|)` for the double-double arithmetic, `S` the profile's magnitude:
/// each term is a few dozen double-double operations of relative error about `2^-104`.
#[derive(Clone, Debug)]
pub struct Profile {
    loops: Vec<Vec<Piece>>,
    gap: f64,
    scale: f64,
    lo: [f64; 2],
    hi: [f64; 2],
}

/// `2^-90`.
const E90: f64 = f64::from_bits(933 << 52);
/// `2^-52`.
const E52: f64 = f64::from_bits(971 << 52);

fn sub2(a: [Dd; 2], b: [Dd; 2]) -> [Dd; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn norm2(v: [Dd; 2]) -> Dd {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn less(a: Dd, b: Dd) -> bool {
    (a - b).hi < 0.0
}

fn tau() -> Dd {
    dd::consts().pi * 2.0
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

impl Piece {
    fn of(seg: Seg) -> Result<Piece, CsgError> {
        match seg {
            Seg::Line { start, end } => {
                if !finite(&[start[0], start[1], end[0], end[1]]) {
                    return Err(CsgError::Size { what: "line end" });
                }
                Ok(Piece {
                    kind: Kind::Line,
                    a: dd::lift(start),
                    b: dd::lift(end),
                })
            }
            Seg::Arc {
                centre,
                start,
                end,
                ccw,
            } => {
                let ok = finite(&[centre[0], centre[1], start[0], start[1], end[0], end[1]]);
                if !ok {
                    return Err(CsgError::Size { what: "arc" });
                }
                let (c, a, b) = (dd::lift(centre), dd::lift(start), dd::lift(end));
                let r = norm2(sub2(a, c));
                // The ends lie on the circle: the far end within a relative `1e-9`.
                let off = (norm2(sub2(b, c)) - r).abs().hi;
                if !(r.hi > 0.0 && off <= 1e-9 * r.hi) {
                    return Err(CsgError::Size { what: "arc" });
                }
                let s = if ccw { 1.0 } else { -1.0 };
                let da = sub2(a, c);
                let t0 = dd::atan2(da[1], da[0]);
                if start == end {
                    return Ok(Piece {
                        kind: Kind::Circle { c, r, start: t0, s },
                        a,
                        b: a,
                    });
                }
                let db = sub2(b, c);
                let mut sweep = (dd::atan2(db[1], db[0]) - t0) * s;
                sweep = sweep - tau() * (sweep / tau()).to_f64().floor();
                Ok(Piece {
                    kind: Kind::Arc {
                        c,
                        r,
                        start: t0,
                        sweep,
                        s,
                    },
                    a,
                    b,
                })
            }
        }
    }

    /// The unsigned distance from `p` to the piece.
    fn distance(&self, p: [Dd; 2]) -> Dd {
        match self.kind {
            Kind::Line => {
                let (u, w) = (sub2(self.b, self.a), sub2(p, self.a));
                let l2 = u[0] * u[0] + u[1] * u[1];
                if l2.hi <= 0.0 {
                    return norm2(w);
                }
                let t = ((w[0] * u[0] + w[1] * u[1]) / l2).to_f64().clamp(0.0, 1.0);
                let t = Dd::new(t);
                norm2([w[0] - u[0] * t, w[1] - u[1] * t])
            }
            Kind::Arc {
                c,
                r,
                start,
                sweep,
                s,
            } => {
                let d = sub2(p, c);
                let tau = tau();
                let mut t = (dd::atan2(d[1], d[0]) - start) * s;
                t = t - tau * (t / tau).to_f64().floor();
                if (t - sweep).hi <= 0.0 {
                    (norm2(d) - r).abs()
                } else {
                    let (da, db) = (norm2(sub2(p, self.a)), norm2(sub2(p, self.b)));
                    if less(da, db) { da } else { db }
                }
            }
            Kind::Circle { c, r, .. } => (norm2(sub2(p, c)) - r).abs(),
        }
    }

    /// The angle the piece turns through as seen from `p`.
    fn angle(&self, p: [Dd; 2]) -> Dd {
        let chord = |a: [Dd; 2], b: [Dd; 2]| {
            let (u, v) = (sub2(a, p), sub2(b, p));
            dd::atan2(u[0] * v[1] - u[1] * v[0], u[0] * v[0] + u[1] * v[1])
        };
        match self.kind {
            Kind::Line => chord(self.a, self.b),
            Kind::Arc { c, r, s, .. } => {
                if !less(norm2(sub2(p, c)), r) {
                    // Outside the circle the arc never winds round p: the chord's angle.
                    return chord(self.a, self.b);
                }
                // Inside, the direction to the arc turns monotonically in its sense, by the
                // angle from a to b measured that way, in [0, 2 pi).
                let mut psi = chord(self.a, self.b) * s;
                if psi.hi < 0.0 {
                    psi = psi + tau();
                }
                psi * s
            }
            Kind::Circle { c, r, .. } => {
                if less(norm2(sub2(p, c)), r) {
                    tau()
                } else {
                    Dd::new(0.0)
                }
            }
        }
    }

    fn reach(&self) -> f64 {
        match self.kind {
            Kind::Line => [self.a, self.b]
                .iter()
                .flatten()
                .fold(0.0, |m: f64, x| m.max(x.hi.abs())),
            Kind::Arc { c, r, .. } | Kind::Circle { c, r, .. } => {
                c[0].hi.abs().max(c[1].hi.abs()) + r.hi
            }
        }
    }

    fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        match self.kind {
            Kind::Line => {
                let (a, b) = (dd::round(self.a), dd::round(self.b));
                (
                    [a[0].min(b[0]), a[1].min(b[1])],
                    [a[0].max(b[0]), a[1].max(b[1])],
                )
            }
            Kind::Arc { c, r, .. } | Kind::Circle { c, r, .. } => {
                let (c, r) = (dd::round(c), r.hi);
                ([c[0] - r, c[1] - r], [c[0] + r, c[1] + r])
            }
        }
    }

    /// The point at `t` in `[0, 1]` along the piece, with the unit normal to the left of
    /// its direction.
    fn at(&self, t: f64) -> ([f64; 2], [f64; 2]) {
        match self.kind {
            Kind::Line => {
                let (a, b) = (dd::round(self.a), dd::round(self.b));
                let d = [b[0] - a[0], b[1] - a[1]];
                let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(f64::MIN_POSITIVE);
                ([a[0] + t * d[0], a[1] + t * d[1]], [-d[1] / l, d[0] / l])
            }
            Kind::Arc {
                c,
                r,
                start,
                sweep,
                s,
            } => {
                let (sn, co) = dd::sin_cos(start + sweep * (t * s));
                let p = dd::round([c[0] + r * co, c[1] + r * sn]);
                let n = [-co.to_f64() * s, -sn.to_f64() * s];
                (p, n)
            }
            Kind::Circle { c, r, start, s } => {
                let (sn, co) = dd::sin_cos(start + tau() * (t * s));
                (
                    dd::round([c[0] + r * co, c[1] + r * sn]),
                    [-co.to_f64() * s, -sn.to_f64() * s],
                )
            }
        }
    }
}

impl Profile {
    /// A profile from its loops. Each loop is its pieces in order, each ending where the
    /// next starts (and the last where the first starts), to within `1e-9` of the profile's
    /// magnitude; a whole circle (an arc with equal ends) is a loop on its own. Loops may nest (odd winding is
    /// inside); they are not checked for crossings, which only widen the band.
    ///
    /// # Errors
    /// [`CsgError::Size`] for a non-finite or out-of-range piece, [`CsgError::ProfileShape`]
    /// for no loops, an empty loop or a circle with company, [`CsgError::ProfileOpen`] when
    /// consecutive pieces do not meet.
    pub fn new(loops: &[Vec<Seg>]) -> Result<Profile, CsgError> {
        if loops.is_empty() {
            return Err(CsgError::ProfileShape);
        }
        let mut built = Vec::with_capacity(loops.len());
        for l in loops {
            let circles = l
                .iter()
                .filter(|s| matches!(s, Seg::Arc { start, end, .. } if start == end))
                .count();
            if l.is_empty() || (circles > 0 && l.len() > 1) {
                return Err(CsgError::ProfileShape);
            }
            built.push(
                l.iter()
                    .map(|s| Piece::of(*s))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        let scale = built
            .iter()
            .flatten()
            .fold(f64::MIN_POSITIVE, |m, p| m.max(p.reach()));
        let mut gap: f64 = 0.0;
        for l in &built {
            for (p, q) in l.iter().zip(l.iter().cycle().skip(1)) {
                gap = gap.max(norm2(sub2(q.a, p.b)).hi);
            }
        }
        if gap > 1e-9 * scale {
            return Err(CsgError::ProfileOpen { gap });
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for (a, b) in built.iter().flatten().map(Piece::bounds) {
            lo = [lo[0].min(a[0]), lo[1].min(a[1])];
            hi = [hi[0].max(b[0]), hi[1].max(b[1])];
        }
        Ok(Profile {
            loops: built,
            // The arithmetic of the ends is part of the gap: 2^-90 of the magnitude.
            gap: (gap + scale * E90).next_up(),
            scale,
            lo,
            hi,
        })
    }

    /// The largest junction gap, rounded up (with the ends' own rounding).
    pub fn gap(&self) -> f64 {
        self.gap
    }

    /// A box holding the profile: `(lo, hi)`.
    pub fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        (self.lo, self.hi)
    }

    /// The signed distance at `p` in double-double: negative inside.
    pub fn signed_distance(&self, p: [f64; 2]) -> Dd {
        let q = dd::lift(p);
        let mut d: Option<Dd> = None;
        let mut turn = Dd::new(0.0);
        for piece in self.loops.iter().flatten() {
            let e = piece.distance(q);
            d = Some(match d {
                Some(m) if less(m, e) => m,
                _ => e,
            });
            turn = turn + piece.angle(q);
        }
        let d = d.unwrap_or_default();
        #[allow(clippy::cast_possible_truncation)] // a winding number: small
        let n = (turn / tau()).to_f64().round() as i64;
        if n.rem_euclid(2) == 1 { -d } else { d }
    }

    /// Whether `p` is inside (odd winding), for points off the curves.
    pub fn inside(&self, p: [f64; 2]) -> bool {
        self.signed_distance(p).hi < 0.0
    }

    /// The enclosure of the signed distance over the box `x` by `y`.
    pub(crate) fn enclose(&self, x: Enclosure, y: Enclosure) -> Enclosure {
        let (cx, cy) = (x.mid(), y.mid());
        let (ex, ey) = (x.reach_from(cx), y.reach_from(cy));
        let rho = Enclosure::point(ex)
            .sqr()
            .add(Enclosure::point(ey).sqr())
            .sqrt()
            .hi;
        let d = self.signed_distance([cx, cy]).to_f64();
        let e = self.gap + (self.scale + cx.abs() + cy.abs()) * E90;
        // `to_f64` rounds once more: half an ulp of d, covered by 2^-52 |d|.
        let m = Enclosure::point(rho)
            .add(Enclosure::point(e))
            .add(Enclosure::point(d.abs() * E52))
            .hi;
        Enclosure::around(d, m)
    }

    /// The pieces, for the points on purpose: each loop's sample functions.
    pub(crate) fn pieces(&self) -> impl Iterator<Item = PieceRef<'_>> {
        self.loops.iter().flat_map(|l| {
            l.iter()
                .zip(l.iter().cycle().skip(1))
                .map(|(p, next)| PieceRef { p, next })
        })
    }

    /// The magnitude `S`.
    pub(crate) fn scale(&self) -> f64 {
        self.scale
    }
}

/// A piece and the piece after it, for sampling.
#[derive(Clone, Copy)]
pub(crate) struct PieceRef<'a> {
    p: &'a Piece,
    next: &'a Piece,
}

impl PieceRef<'_> {
    /// The point at `t` in `[0, 1]` and the unit normal to the left.
    pub(crate) fn at(&self, t: f64) -> ([f64; 2], [f64; 2]) {
        self.p.at(t)
    }

    /// Whether the junction at the piece's end is a corner (not a smooth join), and the
    /// next piece's left normal there.
    pub(crate) fn corner(&self) -> Option<[f64; 2]> {
        if matches!(self.p.kind, Kind::Circle { .. }) {
            return None;
        }
        let (_, n0) = self.p.at(1.0);
        let (_, n1) = self.next.at(0.0);
        let c = n0[0] * n1[1] - n0[1] * n1[0];
        let d = n0[0] * n1[0] + n0[1] * n1[1];
        (c.abs() > 1e-6 || d < 0.0).then_some(n1)
    }
}
