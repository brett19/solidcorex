// Written for solidcorex. Apache-2.0.
//! The planar pieces of a prism's section: lines and circular arcs in a frame's `x, y`
//! coordinates, their areas, and the exact-form comparisons the prism queries make.
//!
//! Every length compared here is the caller's coincidence; there is no other tolerance.

use core::f64::consts::TAU;

use scx_num::fmath;
use scx_num::vector2d::{Point2, Vec2};

/// One piece of a planar loop, in the frame's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Piece {
    /// A straight piece from `a` to `b`.
    Line {
        /// Where it starts.
        a: Point2,
        /// Where it ends.
        b: Point2,
    },
    /// A circular arc about `center`: the point at angle `start` (radians from the frame's x
    /// axis) turning through `sweep` (positive counter-clockwise; a whole circle is `+-2 pi`).
    Arc {
        /// The centre.
        center: Point2,
        /// The radius.
        radius: f64,
        /// The angle of its start.
        start: f64,
        /// The signed turn.
        sweep: f64,
    },
}

fn at(center: Point2, radius: f64, angle: f64) -> Point2 {
    let (s, c) = fmath::sin_cos(angle);
    Point2::new(center.x + radius * c, center.y + radius * s)
}

impl Piece {
    /// The point it starts at.
    #[must_use]
    pub fn start_point(&self) -> Point2 {
        match *self {
            Self::Line { a, .. } => a,
            Self::Arc {
                center,
                radius,
                start,
                ..
            } => at(center, radius, start),
        }
    }

    /// The point it ends at.
    #[must_use]
    pub fn end_point(&self) -> Point2 {
        match *self {
            Self::Line { b, .. } => b,
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => at(center, radius, start + sweep),
        }
    }

    /// The same piece run the other way.
    #[must_use]
    pub fn reversed(&self) -> Self {
        match *self {
            Self::Line { a, b } => Self::Line { a: b, b: a },
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::Arc {
                center,
                radius,
                start: start + sweep,
                sweep: -sweep,
            },
        }
    }

    /// Its length.
    #[must_use]
    pub fn length(&self) -> f64 {
        match *self {
            Self::Line { a, b } => a.distance(b),
            Self::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }

    /// Its Green's-theorem term `1/2 int (x dy - y dx)` about the origin, and the sum of the
    /// absolute values of the products it was made from (the scale of its rounding).
    pub(crate) fn green(&self) -> (f64, f64) {
        match *self {
            Self::Line { a, b } => {
                let (p, q) = (a.x * b.y, b.x * a.y);
                (0.5 * (p - q), 0.5 * (p.abs() + q.abs()))
            }
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let (s0, c0) = fmath::sin_cos(start);
                let (s1, c1) = fmath::sin_cos(start + sweep);
                let r2 = radius * radius * sweep;
                let (px, py) = (center.x * (s1 - s0), center.y * (c1 - c0));
                let term = 0.5 * (r2 + radius * (px - py));
                let scale = 0.5 * (r2.abs() + radius * (px.abs() + py.abs()));
                (term, scale)
            }
        }
    }
}

/// The signed area of a closed loop of pieces (counter-clockwise positive), anchored at the
/// first piece's start so the rounding scales with the loop and not with where it is, and
/// the scale of its rounding error.
pub(crate) fn loop_area(pieces: &[Piece]) -> (f64, f64) {
    let Some(first) = pieces.first() else {
        return (0.0, 0.0);
    };
    let o = first.start_point();
    let (mut sum, mut scale) = (0.0, 0.0);
    for p in pieces {
        let (t, s) = shifted(p, o).green();
        sum += t;
        scale += s;
    }
    (sum, scale)
}

fn shifted(p: &Piece, o: Point2) -> Piece {
    let by = |q: Point2| Point2::new(q.x - o.x, q.y - o.y);
    match *p {
        Piece::Line { a, b } => Piece::Line { a: by(a), b: by(b) },
        Piece::Arc {
            center,
            radius,
            start,
            sweep,
        } => Piece::Arc {
            center: by(center),
            radius,
            start,
            sweep,
        },
    }
}

/// The loop run the other way.
pub(crate) fn reverse_loop(pieces: &[Piece]) -> Vec<Piece> {
    pieces.iter().rev().map(Piece::reversed).collect()
}

fn vec_of(a: Point2, b: Point2) -> Vec2 {
    Vec2::new(b.x - a.x, b.y - a.y)
}

fn cross(u: Vec2, v: Vec2) -> f64 {
    u.x * v.y - u.y * v.x
}

/// Whether `b` continues `a` as one piece: two straight pieces on one line, one way, or two
/// arcs of one circle turning one way, within `c`.
fn joins(a: &Piece, b: &Piece, c: f64) -> Option<Piece> {
    match (*a, *b) {
        (Piece::Line { a: p, b: q }, Piece::Line { a: q2, b: r }) => {
            let (u, v) = (vec_of(p, q), vec_of(q2, r));
            let (lu, lv) = (u.length(), v.length());
            let straight = cross(u, v).abs() <= c * lu.max(lv)
                && cross(u, vec_of(q, r)).abs() <= c * lu.max(vec_of(p, r).length());
            (straight && u.dot(v) > 0.0).then_some(Piece::Line { a: p, b: r })
        }
        (
            Piece::Arc {
                center: c0,
                radius: r0,
                start: s0,
                sweep: w0,
            },
            Piece::Arc {
                center: c1,
                radius: r1,
                sweep: w1,
                ..
            },
        ) => {
            let same = c0.distance(c1) <= c && (r0 - r1).abs() <= c && w0 * w1 > 0.0;
            if !same {
                return None;
            }
            let sweep = w0 + w1;
            // A turn within `c` of whole is the whole circle.
            let sweep = if (TAU - sweep.abs()) * r0 <= c {
                TAU.copysign(sweep)
            } else {
                sweep
            };
            Some(Piece::Arc {
                center: c0,
                radius: r0,
                start: s0,
                sweep,
            })
        }
        _ => None,
    }
}

/// The loop with each run of pieces on one line or one circle made one piece, round the
/// whole loop.
pub(crate) fn merged(pieces: &[Piece], c: f64) -> Vec<Piece> {
    let mut out: Vec<Piece> = pieces.to_vec();
    loop {
        let mut changed = false;
        let mut i = 0;
        while i < out.len() && out.len() > 1 {
            let j = (i + 1) % out.len();
            if j == i {
                break;
            }
            let joined = match (out.get(i), out.get(j)) {
                (Some(a), Some(b)) => joins(a, b, c),
                _ => None,
            };
            if let Some(m) = joined {
                if j == 0 {
                    // The last piece runs on into the first: the merged piece starts where
                    // the last did, and the first goes.
                    out.remove(0);
                    if let Some(last) = out.last_mut() {
                        *last = m;
                    }
                } else {
                    if let Some(slot) = out.get_mut(i) {
                        *slot = m;
                    }
                    out.remove(j);
                }
                changed = true;
            } else {
                i += 1;
            }
        }
        if !changed {
            return out;
        }
    }
}

/// Whether two pieces are the same piece within `c`.
fn same_piece(a: &Piece, b: &Piece, c: f64) -> bool {
    match (*a, *b) {
        (Piece::Line { a: p, b: q }, Piece::Line { a: r, b: s }) => {
            p.distance(r) <= c && q.distance(s) <= c
        }
        (
            Piece::Arc {
                center: c0,
                radius: r0,
                sweep: w0,
                ..
            },
            Piece::Arc {
                center: c1,
                radius: r1,
                sweep: w1,
                ..
            },
        ) => {
            let whole = |w: f64, r: f64| (TAU - w.abs()) * r <= c;
            c0.distance(c1) <= c
                && (r0 - r1).abs() <= c
                && w0 * w1 > 0.0
                && if whole(w0, r0) && whole(w1, r1) {
                    // a whole circle starts where its seam is: the seam is not part of it
                    true
                } else {
                    (w0 - w1).abs() * r0.max(r1) <= c
                        && a.start_point().distance(b.start_point()) <= c
                }
        }
        _ => false,
    }
}

/// Whether two closed loops are the same curve within `c`, from any start and cut into any
/// pieces.
pub(crate) fn same_loop(a: &[Piece], b: &[Piece], c: f64) -> bool {
    let (a, b) = (merged(a, c), merged(b, c));
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let n = a.len();
    (0..n).any(|k| {
        a.iter()
            .enumerate()
            .all(|(i, p)| b.get((i + k) % n).is_some_and(|q| same_piece(p, q, c)))
    })
}
