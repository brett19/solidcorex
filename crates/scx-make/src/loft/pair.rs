// Derived from solidcore `solidcore/crates/solidcore/src/sweep/loft_sections.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: none (not in SMLib). SolidCraft's loft
// section matching (`crates/kernel/src/ build.rs` `loft_numbered_loops`, `loft_pairing.rs`, and
// the circle re-splitting of `numbered.rs`), moved into the kernel so no application builds it.
// Changes: only the pairing is here (the cost, the search over every start by divide and
// conquer, and the band); the winding, frame, cuts and circle splitting are left to MK10. A
// banded answer is always marked `Unproven`, where the pin called a walk off the band's edge
// the least. A moved band is pinned to the walk's ends, so the end of the walk stays inside it.
// The area and centroid of arcs are in closed form. Work is charged to a `Budget` and polled
// against a `Stop`. Errors are typed (`PairError`).

//! The pairing of loft sections' vertices (research 75 §4.1, §4.6, Q5; task MK9).
//!
//! A loft's walls run from a point of one section to a point of the next, so the vertices of
//! two neighbouring sections are put in correspondence round both loops in order: each
//! vertex of either is paired with a vertex of the other, or left alone on one of the other's
//! segments (where the loft cuts that segment, MK10). The pairing is the cyclic alignment of
//! least cost:
//!
//! - **The cost.** Each section is moved to its centroid and scaled to unit area, in the plane
//!   the caller gives both in. Pairing vertex `i` with vertex `j` costs the distance between
//!   them plus, weighted by [`TURN`], half the sum of how much their arriving and their leaving
//!   unit tangents differ (`1 - cos`). A lone vertex costs the same against the nearest point
//!   of the segment it lands on, plus [`UNPAIRED`], so that two points which line up pair.
//! - **The search.** An alignment is a monotone walk through the grid of both loops'
//!   vertices, from `(0, s)` to `(m, s + n)` for a start `s` of the second loop. Two least
//!   walks from different starts can be chosen not to cross, so each start's walk is searched
//!   only between the walks of the starts either side of it, halving the starts each time
//!   (Maes' divide and conquer): about `m·n·log2 n` cells, not `m·n²`.
//! - **The limit.** When the grid, `(m + 1)(n + 1)` cells, is within [`MAX_CELLS`], that
//!   search runs and its answer is [`Optimality::Optimal`]. Past it, the walk is searched from
//!   one start through a band of the grid about where each vertex's share of the length puts
//!   it, the band moved onto the walk it finds up to eight times; that answer is
//!   [`Optimality::Unproven`], its cost an upper bound on the optimum. The limit is a constant
//!   of the operation, not the remaining budget, so which search runs is a function of the
//!   input alone (75 Q5).
//!
//! A banded walk that keeps off its band's edge is **not** proved least, against 75 Q5's
//! first reading: the band has one start, and a walk off the band's edge is least only
//! within the band, since a cheaper walk may leave the band entirely. The pin's own case of a
//! 10 x 1 and a 1.5 x 1 rectangle in about 500 segments keeps off its edge and costs 1.8%
//! more than the optimum (research 75 §13, MK9 as built).
//!
//! Every cell visited is charged one unit to the caller's [`Budget`], and the [`Stop`] flag is
//! polled once a grid row.
//!
//! Not here (MK10's): winding the sections alike, carrying a shared frame from plane to
//! plane, cutting segments where lone vertices land, and splitting full circles at the
//! neighbouring section's directions.

use core::f64::consts::TAU;
use core::fmt;

use scx_num::budget::{Budget, Exhausted, Stop, Stopped};
use scx_num::error::ErrorKind;
use scx_num::fmath;
use scx_num::frame::Sense;
use scx_num::vector2d::{Point2, Vec2};
use scx_planar::Segment;

/// The most cells, `(m + 1)(n + 1)`, the grid of two sections of `m` and `n` segments may
/// have for the exact search over every start (the pin's limit: two sections of about 2,000
/// segments each). Past it the answer is banded and [`Optimality::Unproven`].
pub const MAX_CELLS: usize = 4_000_000;

/// The weight of the tangent terms against the distance between the points.
const TURN: f64 = 1.0;
/// What a lone vertex costs on top of its distance and tangents: less than any corner's
/// mismatch, so a sharp corner pairs with a sharp corner, but enough that two points which
/// line up pair rather than both cutting the other section.
const UNPAIRED: f64 = 0.1;
/// How many times the band is moved onto the walk it found; the last band's walk is kept.
const BAND_PASSES: usize = 8;
/// A vertex turns more than this (`1 - cos` of the angle between its tangents, about 26°) to
/// be a corner the band is anchored at.
const CORNER_TURN: f64 = 0.1;
/// Two walks' costs closer than this share of the larger are a tie, broken by the earlier
/// start, so the answer does not hang on the last bits of a sum.
const TIE: f64 = 1e-12;

/// One step of an alignment, in order round both loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Event {
    /// Vertex `first` of the first section is paired with vertex `second` of the second.
    Pair {
        /// The first section's vertex.
        first: usize,
        /// The second section's vertex.
        second: usize,
    },
    /// Vertex `vertex` of the first section is alone, landing on the second's segment `on`.
    FirstAlone {
        /// The first section's vertex.
        vertex: usize,
        /// The second section's segment it lands on.
        on: usize,
    },
    /// Vertex `vertex` of the second section is alone, landing on the first's segment `on`.
    SecondAlone {
        /// The second section's vertex.
        vertex: usize,
        /// The first section's segment it lands on.
        on: usize,
    },
}

/// What is known of an alignment's cost against the least over every alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Optimality {
    /// The least over every start and walk, found by the search over the whole grid.
    Optimal,
    /// The least within a band from one start: the cost is an upper bound on the optimum.
    Unproven,
}

/// The pairing of two neighbouring sections.
///
/// Vertex `k` of a section is the start of its segment `k`. The events run from vertex 0 of
/// the first section and vertex `start` of the second, round both once: every vertex of
/// each appears in exactly one event.
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    /// The second section's vertex the walk starts at.
    pub start: usize,
    /// The steps, in order round both loops.
    pub events: Vec<Event>,
    /// The sum of the steps' costs.
    pub cost: f64,
    /// Whether the cost is proved the least.
    pub optimality: Optimality,
}

/// The pairing of a loft's sections: `alignments[k]` pairs section `k` with section `k + 1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pairing {
    /// One alignment per neighbouring pair of sections.
    pub alignments: Vec<Alignment>,
}

impl Pairing {
    /// Whether every alignment is [`Optimality::Optimal`].
    #[must_use]
    pub fn is_optimal(&self) -> bool {
        self.alignments
            .iter()
            .all(|a| a.optimality == Optimality::Optimal)
    }
}

/// Why a section is refused: the caller can change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PairRefusal {
    /// The section has no segments.
    Empty {
        /// Which section.
        section: usize,
    },
    /// A coordinate or radius of the section is not finite.
    NotFinite {
        /// Which section.
        section: usize,
        /// Which segment.
        segment: usize,
    },
    /// A segment has no length: a line or an arc whose ends are the same point, or a radius
    /// of zero.
    Degenerate {
        /// Which section.
        section: usize,
        /// Which segment.
        segment: usize,
    },
    /// The section encloses no area, so it has no centroid or scale to be compared at.
    NoArea {
        /// Which section.
        section: usize,
    },
    /// The section winds the other way round from the one before it.
    Winding {
        /// Which section.
        section: usize,
    },
}

/// Why no pairing was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PairError {
    /// The input is wrong.
    Refused(PairRefusal),
    /// A segment of a kind this pairing does not read yet.
    UnsupportedSegment {
        /// Which section.
        section: usize,
        /// Which segment.
        segment: usize,
    },
    /// The sections are past [`MAX_CELLS`] and so large that even a band one column wide
    /// does not fit in it.
    TooLarge {
        /// The first section's segment count.
        first: usize,
        /// The second's.
        second: usize,
    },
    /// The work budget ran out.
    Exhausted(Exhausted),
    /// The caller stopped the pairing.
    Stopped(Stopped),
    /// A search found no walk to the grid's end: our fault.
    Unreached,
}

impl PairError {
    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::Refused(_) => ErrorKind::Refused,
            Self::UnsupportedSegment { .. } | Self::TooLarge { .. } => ErrorKind::Unsupported,
            Self::Exhausted(_) => ErrorKind::Exhausted,
            Self::Stopped(_) => ErrorKind::Cancelled,
            Self::Unreached => ErrorKind::Defect,
        }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Refused(PairRefusal::Empty { .. }) => "empty_section",
            Self::Refused(PairRefusal::NotFinite { .. }) => "not_finite",
            Self::Refused(PairRefusal::Degenerate { .. }) => "degenerate_segment",
            Self::Refused(PairRefusal::NoArea { .. }) => "no_area",
            Self::Refused(PairRefusal::Winding { .. }) => "winding",
            Self::UnsupportedSegment { .. } => "segment_kind",
            Self::TooLarge { .. } => "too_large",
            Self::Exhausted(e) => e.name(),
            Self::Stopped(_) => "stopped",
            Self::Unreached => "unreached",
        }
    }
}

impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(PairRefusal::Empty { section }) => {
                write!(f, "section {section} has no segments")
            }
            Self::Refused(PairRefusal::NotFinite { section, segment }) => {
                write!(f, "segment {segment} of section {section} is not finite")
            }
            Self::Refused(PairRefusal::Degenerate { section, segment }) => {
                write!(f, "segment {segment} of section {section} has no length")
            }
            Self::Refused(PairRefusal::NoArea { section }) => {
                write!(f, "section {section} encloses no area")
            }
            Self::Refused(PairRefusal::Winding { section }) => write!(
                f,
                "section {section} winds the other way round from the one before it"
            ),
            Self::UnsupportedSegment { section, segment } => write!(
                f,
                "segment {segment} of section {section} is of a kind not paired yet"
            ),
            Self::TooLarge { first, second } => write!(
                f,
                "sections of {first} and {second} segments are too large to pair within {MAX_CELLS} cells"
            ),
            Self::Exhausted(e) => e.fmt(f),
            Self::Stopped(_) => write!(f, "the pairing was stopped"),
            Self::Unreached => write!(f, "the pairing's search reached no end"),
        }
    }
}

impl core::error::Error for PairError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Exhausted(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Exhausted> for PairError {
    fn from(e: Exhausted) -> Self {
        Self::Exhausted(e)
    }
}

impl From<Stopped> for PairError {
    fn from(e: Stopped) -> Self {
        Self::Stopped(e)
    }
}

/// The alignment of least cost between `first` and `second`, two closed loops of segments
/// in one plane's coordinates, wound the same way: [`Optimality::Optimal`] when their grid
/// is within [`MAX_CELLS`], else the banded answer, [`Optimality::Unproven`].
///
/// # Errors
///
/// [`PairError::Refused`] for an empty, non-finite, degenerate, arealess or oppositely
/// wound section; [`PairError::TooLarge`] past even the band; [`PairError::Exhausted`] and
/// [`PairError::Stopped`] from the budget and the flag.
pub fn align(
    first: &[Segment],
    second: &[Segment],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Alignment, PairError> {
    let (a, b) = (Loop::new(0, first)?, Loop::new(1, second)?);
    same_winding(&a, &b, 1)?;
    aligned(&a, &b, MAX_CELLS, &mut Work { budget, stop })
}

/// The banded alignment of `first` and `second` within `cells` cells, whatever their size:
/// the answer [`align`] gives past [`MAX_CELLS`] (with `cells` that), always
/// [`Optimality::Unproven`]. Public so that its cost can be set against the exact search's
/// on sections both can pair.
///
/// # Errors
///
/// As [`align`]; [`PairError::TooLarge`] when a band one column wide does not fit in
/// `cells`.
pub fn align_banded(
    first: &[Segment],
    second: &[Segment],
    cells: usize,
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Alignment, PairError> {
    let (a, b) = (Loop::new(0, first)?, Loop::new(1, second)?);
    same_winding(&a, &b, 1)?;
    let g = Grid {
        a: a.side(),
        b: b.side(),
    };
    let mut work = Work { budget, stop };
    let w = banded_walk(&g, true, cells, &mut work)?;
    Ok(w.alignment(&g, Optimality::Unproven))
}

/// The pairing of a loft's sections in order, each with the next ([`align`]).
///
/// # Errors
///
/// As [`align`], naming the section by its index in `sections`. Every section is checked
/// before any is paired.
pub fn pair(
    sections: &[&[Segment]],
    budget: &mut Budget,
    stop: &Stop,
) -> Result<Pairing, PairError> {
    let loops = sections
        .iter()
        .enumerate()
        .map(|(k, s)| Loop::new(k, s))
        .collect::<Result<Vec<_>, _>>()?;
    for (k, w) in loops.windows(2).enumerate() {
        if let [a, b] = w {
            same_winding(a, b, k + 1)?;
        }
    }
    let mut work = Work { budget, stop };
    let alignments = loops
        .windows(2)
        .filter_map(|w| match w {
            [a, b] => Some((a, b)),
            _ => None,
        })
        .map(|(a, b)| aligned(a, b, MAX_CELLS, &mut work))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Pairing { alignments })
}

fn same_winding(a: &Loop, b: &Loop, section: usize) -> Result<(), PairError> {
    if (a.area > 0.0) == (b.area > 0.0) {
        Ok(())
    } else {
        Err(PairError::Refused(PairRefusal::Winding { section }))
    }
}

/// The exact search within `cells`, else the band.
fn aligned(a: &Loop, b: &Loop, cells: usize, work: &mut Work<'_>) -> Result<Alignment, PairError> {
    let g = Grid {
        a: a.side(),
        b: b.side(),
    };
    let (m, n) = (g.a.count(), g.b.count());
    if (m + 1).saturating_mul(n + 1) <= cells {
        Ok(best_walk(&g, work)?.alignment(&g, Optimality::Optimal))
    } else {
        Ok(banded_walk(&g, true, cells, work)?.alignment(&g, Optimality::Unproven))
    }
}

/// The caller's budget and flag: each grid row charges its cells and polls the flag.
struct Work<'a> {
    budget: &'a mut Budget,
    stop: &'a Stop,
}

impl Work<'_> {
    fn charge(&mut self, cells: usize) -> Result<(), PairError> {
        self.stop.check()?;
        self.budget.charge(cells as u64)?;
        Ok(())
    }
}

// --- Segments ------------------------------------------------------------------------------

fn vec(p: Point2) -> Vec2 {
    Vec2::new(p.x, p.y)
}

fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// `v` turned a quarter turn counter-clockwise.
fn perp(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

fn unit_at(t: f64) -> Vec2 {
    let (s, c) = fmath::sin_cos(t);
    Vec2::new(c, s)
}

/// A segment as the pairing reads it.
#[derive(Debug, Clone, Copy)]
enum Seg {
    Line {
        a: Vec2,
        b: Vec2,
    },
    /// The arc about `c` of radius `r` from the angle `from`, turning `sweep` (signed:
    /// counter-clockwise when positive).
    Arc {
        c: Vec2,
        r: f64,
        from: f64,
        sweep: f64,
    },
}

impl Seg {
    /// The segment, or why it cannot be read: `None` when it has no length.
    fn of(s: &Segment) -> Result<Option<Self>, ()> {
        match *s {
            Segment::Line { start, end } => {
                let (a, b) = (vec(start), vec(end));
                Ok((a != b).then_some(Self::Line { a, b }))
            }
            Segment::Arc {
                centre,
                start,
                end,
                sense,
            } => {
                let c = vec(centre);
                let (u, w) = (vec(start) - c, vec(end) - c);
                let r = 0.5 * (u.length() + w.length());
                if start == end || r <= 0.0 || r.is_nan() {
                    return Ok(None);
                }
                // The angle turned from start to end in the arc's sense, in (0, 2π].
                let turned = |p: Vec2, q: Vec2| {
                    let t = fmath::atan2(cross(p, q), p.dot(q));
                    if t <= 0.0 { t + TAU } else { t }
                };
                let sweep = match sense {
                    Sense::Forward => turned(u, w),
                    Sense::Reversed => -turned(w, u),
                };
                Ok(Some(Self::Arc {
                    c,
                    r,
                    from: fmath::atan2(u.y, u.x),
                    sweep,
                }))
            }
            Segment::Circle {
                centre,
                radius,
                sense,
            } => Ok((radius > 0.0).then_some(Self::Arc {
                c: vec(centre),
                r: radius,
                from: 0.0,
                sweep: match sense {
                    Sense::Forward => TAU,
                    Sense::Reversed => -TAU,
                },
            })),
            _ => Err(()),
        }
    }

    fn point(&self, t: f64) -> Vec2 {
        match *self {
            Self::Line { a, b } => a + (b - a) * t,
            Self::Arc { c, r, from, sweep } => c + unit_at(from + sweep * t) * r,
        }
    }

    /// The unit tangent at `t`.
    fn tangent(&self, t: f64) -> Vec2 {
        match *self {
            Self::Line { a, b } => {
                let d = b - a;
                d / d.length()
            }
            Self::Arc { from, sweep, .. } => perp(unit_at(from + sweep * t)) * sweep.signum(),
        }
    }

    fn length(&self) -> f64 {
        match *self {
            Self::Line { a, b } => (b - a).length(),
            Self::Arc { r, sweep, .. } => r * sweep.abs(),
        }
    }

    /// Its term of the loop's signed area, `∮ (x dy - y dx) / 2`.
    fn area(&self) -> f64 {
        match *self {
            Self::Line { a, b } => 0.5 * cross(a, b),
            Self::Arc { c, r, sweep, .. } => {
                0.5 * (r * r * sweep + cross(c, self.point(1.0) - self.point(0.0)))
            }
        }
    }

    /// Its terms of the loop's first moments, `∮ x² / 2 dy` and `-∮ y² / 2 dx` (the
    /// integrals of `x` and `y` over the area).
    fn moments(&self) -> (f64, f64) {
        match *self {
            Self::Line { a, b } => (
                (b.y - a.y) / 6.0 * (a.x * a.x + a.x * b.x + b.x * b.x),
                -(b.x - a.x) / 6.0 * (a.y * a.y + a.y * b.y + b.y * b.y),
            ),
            Self::Arc { c, r, from, sweep } => {
                // x = cx + r cos θ, dy = r cos θ dθ; y = cy + r sin θ, dx = -r sin θ dθ.
                let fx = |t: f64| {
                    let (s, co) = fmath::sin_cos(t);
                    c.x * c.x * r * s
                        + 2.0 * c.x * r * r * (t / 2.0 + s * co / 2.0)
                        + r * r * r * (s - s * s * s / 3.0)
                };
                let fy = |t: f64| {
                    let (s, co) = fmath::sin_cos(t);
                    -c.y * c.y * r * co
                        + 2.0 * c.y * r * r * (t / 2.0 - s * co / 2.0)
                        + r * r * r * (-co + co * co * co / 3.0)
                };
                let to = from + sweep;
                (0.5 * (fx(to) - fx(from)), 0.5 * (fy(to) - fy(from)))
            }
        }
    }

    /// The parameter of its point nearest `p`.
    fn nearest(&self, p: Vec2) -> f64 {
        match *self {
            Self::Line { a, b } => {
                let d = b - a;
                ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0)
            }
            Self::Arc { c, from, sweep, .. } => {
                let q = p - c;
                if q == Vec2::ZERO {
                    return 0.5;
                }
                let w = sweep.abs();
                // How far round the arc, in its direction of travel, the point's direction lies.
                let u = ((fmath::atan2(q.y, q.x) - from) * sweep.signum()).rem_euclid(TAU);
                if u <= w {
                    u / w
                } else if TAU - u < u - w {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }

    /// The segment moved by `-centre` and scaled by `1 / scale`.
    fn normalized(self, centre: Vec2, scale: f64) -> Self {
        match self {
            Self::Line { a, b } => Self::Line {
                a: (a - centre) / scale,
                b: (b - centre) / scale,
            },
            Self::Arc { c, r, from, sweep } => Self::Arc {
                c: (c - centre) / scale,
                r: r / scale,
                from,
                sweep,
            },
        }
    }
}

/// A checked section: its segments at its centroid and unit area, with each one's given
/// start, and its signed area as given.
struct Loop {
    segs: Vec<Seg>,
    starts: Vec<Vec2>,
    area: f64,
}

impl Loop {
    fn new(section: usize, given: &[Segment]) -> Result<Self, PairError> {
        if given.is_empty() {
            return Err(PairError::Refused(PairRefusal::Empty { section }));
        }
        let mut segs = Vec::with_capacity(given.len());
        for (segment, s) in given.iter().enumerate() {
            let finite = match *s {
                Segment::Line { start, end } => [start.x, start.y, end.x, end.y]
                    .iter()
                    .all(|x| x.is_finite()),
                Segment::Arc {
                    centre, start, end, ..
                } => [start.x, start.y, end.x, end.y, centre.x, centre.y]
                    .iter()
                    .all(|x| x.is_finite()),
                Segment::Circle { centre, radius, .. } => {
                    [centre.x, centre.y, radius].iter().all(|x| x.is_finite())
                }
                _ => true,
            };
            if !finite {
                return Err(PairError::Refused(PairRefusal::NotFinite {
                    section,
                    segment,
                }));
            }
            match Seg::of(s) {
                Ok(Some(seg)) if seg.length() > 0.0 && seg.length().is_finite() => segs.push(seg),
                Ok(_) => {
                    return Err(PairError::Refused(PairRefusal::Degenerate {
                        section,
                        segment,
                    }));
                }
                Err(()) => return Err(PairError::UnsupportedSegment { section, segment }),
            }
        }
        let area: f64 = segs.iter().map(Seg::area).sum();
        if !(area.is_finite() && area != 0.0) {
            return Err(PairError::Refused(PairRefusal::NoArea { section }));
        }
        let (mx, my) = segs
            .iter()
            .map(Seg::moments)
            .fold((0.0, 0.0), |(x, y), (dx, dy)| (x + dx, y + dy));
        let centre = Vec2::new(mx / area, my / area);
        let scale = area.abs().sqrt();
        let starts = given
            .iter()
            .map(|s| (vec(s.start()) - centre) / scale)
            .collect();
        Ok(Self {
            segs: segs
                .into_iter()
                .map(|s| s.normalized(centre, scale))
                .collect(),
            starts,
            area,
        })
    }

    fn side(&self) -> Side {
        Side::new(self.segs.clone(), self.starts.clone())
    }
}

fn wrap(i: usize, n: usize) -> usize {
    i.checked_rem(n).unwrap_or(0)
}

/// How much two unit tangents differ: 0 when they agree, 2 when they are opposite.
fn turn(a: Vec2, b: Vec2) -> f64 {
    1.0 - a.dot(b)
}

/// One loop as the grid reads it, at its centroid and unit area.
struct Side {
    segs: Vec<Seg>,
    at: Vec<Vec2>,
    /// The unit tangent arriving at each vertex, and leaving it.
    before: Vec<Vec2>,
    after: Vec<Vec2>,
    len: Vec<f64>,
}

impl Side {
    fn new(segs: Vec<Seg>, at: Vec<Vec2>) -> Self {
        let n = segs.len();
        let after = segs.iter().map(|s| s.tangent(0.0)).collect();
        let before = (0..n)
            .map(|i| {
                segs.get(wrap(i + n - 1, n))
                    .map_or(Vec2::ZERO, |s| s.tangent(1.0))
            })
            .collect();
        let len = segs.iter().map(Seg::length).collect();
        Self {
            segs,
            at,
            before,
            after,
            len,
        }
    }

    fn count(&self) -> usize {
        self.segs.len()
    }

    fn at(&self, i: usize) -> Vec2 {
        self.at.get(i).copied().unwrap_or(Vec2::ZERO)
    }

    fn before(&self, i: usize) -> Vec2 {
        self.before.get(i).copied().unwrap_or(Vec2::ZERO)
    }

    fn after(&self, i: usize) -> Vec2 {
        self.after.get(i).copied().unwrap_or(Vec2::ZERO)
    }

    fn len(&self, i: usize) -> f64 {
        self.len.get(i).copied().unwrap_or(0.0)
    }
}

/// The two loops of a pairing: `a` along the grid's rows, `b` along its columns.
struct Grid {
    a: Side,
    b: Side,
}

impl Grid {
    /// Vertex `i` of `a` paired with vertex `j` of `b`.
    fn pair(&self, i: usize, j: usize) -> f64 {
        let (a, b) = (&self.a, &self.b);
        (a.at(i) - b.at(j)).length()
            + TURN * 0.5 * (turn(a.before(i), b.before(j)) + turn(a.after(i), b.after(j)))
    }

    /// Vertex `i` of `x` alone, on segment `e` of `y`.
    fn alone(x: &Side, i: usize, y: &Side, e: usize) -> f64 {
        let Some(s) = y.segs.get(e) else {
            return f64::INFINITY;
        };
        let q = x.at(i);
        let t = s.nearest(q);
        let tangent = s.tangent(t);
        (q - s.point(t)).length()
            + TURN * 0.5 * (turn(x.before(i), tangent) + turn(x.after(i), tangent))
            + UNPAIRED
    }

    fn alone_a(&self, i: usize, e: usize) -> f64 {
        Self::alone(&self.a, i, &self.b, e)
    }

    fn alone_b(&self, j: usize, e: usize) -> f64 {
        Self::alone(&self.b, j, &self.a, e)
    }
}

/// A step of a walk through the grid: from node `(i, j)`, vertex `i` of `a` paired with
/// vertex `j` of `b`, or either alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Pair,
    A,
    B,
}

/// A walk from node `(0, start)` to `(m, start + n)`.
#[derive(Clone, Debug)]
struct Walk {
    start: usize,
    cost: f64,
    steps: Vec<Step>,
}

impl Walk {
    /// For each row, the least and the greatest column the walk visits there.
    fn bounds(&self, m: usize) -> (Vec<usize>, Vec<usize>) {
        let (mut lo, mut hi) = (vec![usize::MAX; m + 1], vec![0usize; m + 1]);
        let (mut i, mut j) = (0usize, self.start);
        let mut visit = |i: usize, j: usize| {
            if let (Some(l), Some(h)) = (lo.get_mut(i), hi.get_mut(i)) {
                *l = (*l).min(j);
                *h = (*h).max(j);
            }
        };
        visit(i, j);
        for s in &self.steps {
            match s {
                Step::Pair => (i, j) = (i + 1, j + 1),
                Step::A => i += 1,
                Step::B => j += 1,
            }
            visit(i, j);
        }
        (lo, hi)
    }

    fn alignment(&self, g: &Grid, optimality: Optimality) -> Alignment {
        let (m, n) = (g.a.count(), g.b.count());
        let (mut i, mut j) = (0usize, self.start);
        let events = self
            .steps
            .iter()
            .map(|s| match s {
                Step::Pair => {
                    let e = Event::Pair {
                        first: i,
                        second: wrap(j, n),
                    };
                    (i, j) = (i + 1, j + 1);
                    e
                }
                Step::A => {
                    let e = Event::FirstAlone {
                        vertex: i,
                        on: wrap(j + n - 1, n),
                    };
                    i += 1;
                    e
                }
                Step::B => {
                    let e = Event::SecondAlone {
                        vertex: wrap(j, n),
                        on: wrap(i + m - 1, m),
                    };
                    j += 1;
                    e
                }
            })
            .collect();
        Alignment {
            start: self.start,
            events,
            cost: self.cost,
            optimality,
        }
    }
}

/// How a node was reached.
const BY_PAIR: u8 = 0;
const BY_A: u8 = 1;
const BY_B: u8 = 2;
const AT_START: u8 = 3;

/// The least-cost walk from `(0, s)` to `(m, s + n)` through the nodes of row `i` at
/// columns `lo[i]..=hi[i]`. Ties go to a pair, then to `a`'s vertex alone.
fn walk(
    g: &Grid,
    s: usize,
    lo: &[usize],
    hi: &[usize],
    work: &mut Work<'_>,
) -> Result<Walk, PairError> {
    let (m, n) = (g.a.count(), g.b.count());
    // The rows' column ranges, kept to the columns a walk from this start can reach.
    let ranges: Vec<(usize, usize)> = (0..=m)
        .map(|i| {
            (
                lo.get(i).copied().unwrap_or(0).max(s),
                hi.get(i).copied().unwrap_or(0).min(s + n),
            )
        })
        .collect();
    let width = |r: (usize, usize)| if r.1 >= r.0 { r.1 - r.0 + 1 } else { 0 };
    let mut offsets = Vec::with_capacity(m + 1);
    let mut total = 0usize;
    for &r in &ranges {
        offsets.push(total);
        total += width(r);
    }
    let mut back = vec![AT_START; total];
    let mut prev: Vec<f64> = Vec::new();
    let mut prev_range = (1usize, 0usize);
    for (i, &(l, h)) in ranges.iter().enumerate() {
        work.charge(width((l, h)))?;
        let mut cur = vec![f64::INFINITY; width((l, h))];
        let off = offsets.get(i).copied().unwrap_or(0);
        let within = |c: usize| c >= prev_range.0 && c <= prev_range.1;
        for j in l..=h {
            let (mut best, mut how) =
                (if i == 0 && j == s { 0.0 } else { f64::INFINITY }, AT_START);
            if i > 0
                && j >= 1
                && within(j - 1)
                && let Some(&p) = prev.get(j - 1 - prev_range.0)
            {
                let c = p + g.pair(i - 1, wrap(j - 1, n));
                if c < best {
                    (best, how) = (c, BY_PAIR);
                }
            }
            if i > 0
                && within(j)
                && let Some(&p) = prev.get(j - prev_range.0)
            {
                let c = p + g.alone_a(i - 1, wrap(j + n - 1, n));
                if c < best {
                    (best, how) = (c, BY_A);
                }
            }
            if j > l
                && let Some(&p) = cur.get(j - 1 - l)
            {
                let c = p + g.alone_b(wrap(j - 1, n), wrap(i + m - 1, m));
                if c < best {
                    (best, how) = (c, BY_B);
                }
            }
            if let Some(x) = cur.get_mut(j - l) {
                *x = best;
            }
            if let Some(x) = back.get_mut(off + j - l) {
                *x = how;
            }
        }
        prev = cur;
        prev_range = (l, h);
    }
    let end = s + n;
    if !within_range(end, prev_range) {
        return Err(PairError::Unreached);
    }
    let cost = prev
        .get(end - prev_range.0)
        .copied()
        .filter(|c| c.is_finite())
        .ok_or(PairError::Unreached)?;
    let mut steps = Vec::with_capacity(m + n);
    let (mut i, mut j) = (m, end);
    while (i, j) != (0, s) {
        let (l, _) = ranges.get(i).copied().ok_or(PairError::Unreached)?;
        let how = offsets
            .get(i)
            .zip(j.checked_sub(l))
            .and_then(|(o, k)| back.get(o + k))
            .copied()
            .ok_or(PairError::Unreached)?;
        let (di, dj, step) = match how {
            BY_PAIR => (1, 1, Step::Pair),
            BY_A => (1, 0, Step::A),
            BY_B => (0, 1, Step::B),
            _ => return Err(PairError::Unreached),
        };
        steps.push(step);
        i = i.checked_sub(di).ok_or(PairError::Unreached)?;
        j = j.checked_sub(dj).ok_or(PairError::Unreached)?;
    }
    steps.reverse();
    Ok(Walk {
        start: s,
        cost,
        steps,
    })
}

fn within_range(c: usize, r: (usize, usize)) -> bool {
    c >= r.0 && c <= r.1
}

/// Whether `w` is better than `best`: cheaper beyond [`TIE`], or as cheap from an earlier
/// start.
fn better(w: &Walk, best: &Walk) -> bool {
    let tol = TIE * best.cost.abs().max(1.0);
    w.cost < best.cost - tol || (w.cost <= best.cost + tol && w.start < best.start)
}

/// The least-cost walk over every start, by divide and conquer over the starts.
fn best_walk(g: &Grid, work: &mut Work<'_>) -> Result<Walk, PairError> {
    let (m, n) = (g.a.count(), g.b.count());
    let first = walk(g, 0, &vec![0; m + 1], &vec![n; m + 1], work)?;
    let (lo, hi) = first.bounds(m);
    // The walk from start n is the first one moved on by a turn.
    let hi_n: Vec<usize> = hi.iter().map(|c| c + n).collect();
    let mut best = first;
    between(g, (0, &lo), (n, &hi_n), &mut best, work)?;
    Ok(best)
}

/// Every start strictly between `low.0` and `high.0`: searched to the right of `low.1` (the
/// leftmost columns the walk from `low.0` visits on each row) and to the left of `high.1`
/// (the rightmost of the walk from `high.0`). The range of starts halves at each level, so
/// the recursion is at most `log2 n` deep.
fn between(
    g: &Grid,
    low: (usize, &[usize]),
    high: (usize, &[usize]),
    best: &mut Walk,
    work: &mut Work<'_>,
) -> Result<(), PairError> {
    if high.0 <= low.0 + 1 {
        return Ok(());
    }
    let m = g.a.count();
    let mid = low.0 + (high.0 - low.0) / 2;
    let w = walk(g, mid, low.1, high.1, work)?;
    let (lo, hi) = w.bounds(m);
    if better(&w, best) {
        *best = w;
    }
    between(g, low, (mid, &hi), best, work)?;
    between(g, (mid, &lo), high, best, work)
}

/// The corners of a loop ([`CORNER_TURN`]).
fn corners(side: &Side) -> Vec<usize> {
    (0..side.count())
        .filter(|&i| turn(side.before(i), side.after(i)) > CORNER_TURN)
        .collect()
}

/// The corners of the two loops paired as the polygons through them pair: `(i, j)` vertex
/// numbers in order. Empty when either loop has fewer than three corners, two corners of a
/// loop are one point, or the corner polygons are too large even for a band: the pairs only
/// centre the band, so without them it is centred on the shares round the whole loops.
fn corner_pairs(
    g: &Grid,
    cells: usize,
    work: &mut Work<'_>,
) -> Result<Vec<(usize, usize)>, PairError> {
    let (ca, cb) = (corners(&g.a), corners(&g.b));
    if ca.len() < 3 || cb.len() < 3 {
        return Ok(Vec::new());
    }
    let polygon = |side: &Side, c: &[usize]| -> Option<Side> {
        let at: Vec<Vec2> = c.iter().map(|&k| side.at(k)).collect();
        let segs = (0..at.len())
            .map(|k| {
                let (a, b) = (at.get(k).copied()?, at.get(wrap(k + 1, at.len())).copied()?);
                (a != b).then_some(Seg::Line { a, b })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Side::new(segs, at))
    };
    let (Some(a), Some(b)) = (polygon(&g.a, &ca), polygon(&g.b, &cb)) else {
        return Ok(Vec::new());
    };
    let small = Grid { a, b };
    let walked = if (ca.len() + 1).saturating_mul(cb.len() + 1) > cells {
        match banded_walk(&small, false, cells, work) {
            Err(PairError::TooLarge { .. }) => return Ok(Vec::new()),
            w => w?,
        }
    } else {
        best_walk(&small, work)?
    };
    let n = cb.len();
    let (mut i, mut j) = (0usize, walked.start);
    let mut out = Vec::new();
    for st in &walked.steps {
        match st {
            Step::Pair => {
                if let (Some(&x), Some(&y)) = (ca.get(i), cb.get(wrap(j, n))) {
                    out.push((x, y));
                }
                (i, j) = (i + 1, j + 1);
            }
            Step::A => i += 1,
            Step::B => j += 1,
        }
    }
    Ok(out)
}

/// For the rows `i0..i1` of `a` (counted on round the loop), the columns of `b` (from `j0`
/// to at most `j1`, counted on likewise) whose share of `b`'s length from `j0` is nearest the
/// row vertex's share of `a`'s length from `i0`.
fn share_columns(g: &Grid, (i0, i1): (usize, usize), (j0, j1): (usize, usize)) -> Vec<usize> {
    let (m, n) = (g.a.count(), g.b.count());
    let la: f64 = (i0..i1).map(|i| g.a.len(wrap(i, m))).sum();
    let lb: f64 = (j0..j1).map(|j| g.b.len(wrap(j, n))).sum();
    let share = |d: f64, l: f64| if l > 0.0 { d / l } else { 0.0 };
    let mut out = Vec::with_capacity(i1.saturating_sub(i0));
    let (mut k, mut db, mut da) = (j0, 0.0f64, 0.0f64);
    for i in i0..i1 {
        let target = share(da, la);
        while k < j1 {
            let next = db + g.b.len(wrap(k, n));
            if (share(next, lb) - target).abs() <= (share(db, lb) - target).abs() {
                (db, k) = (next, k + 1);
            } else {
                break;
            }
        }
        out.push(k);
        da += g.a.len(wrap(i, m));
    }
    out
}

/// The band's centre column for every row (0 to m), from `s` for row 0 to `s + n` for row
/// m: between paired corners, each of `a`'s vertices at its share of the length between
/// them; with none, from `b`'s vertex that pairs most cheaply with `a`'s vertex 0, at its
/// share of the length round.
fn band_centres(
    g: &Grid,
    anchored: bool,
    cells: usize,
    work: &mut Work<'_>,
) -> Result<Option<Vec<usize>>, PairError> {
    let (m, n) = (g.a.count(), g.b.count());
    let anchors = if anchored {
        corner_pairs(g, cells, work)?
    } else {
        Vec::new()
    };
    let Some(&(i_first, _)) = anchors.first() else {
        let Some(s) = (0..n).min_by(|x, y| g.pair(0, *x).total_cmp(&g.pair(0, *y))) else {
            return Ok(None);
        };
        let mut c = share_columns(g, (0, m), (s, s + n));
        c.push(s + n);
        return Ok(Some(c));
    };
    // The paired corners' columns counted on round `b` so they increase, from n on.
    let mut on: Vec<(usize, usize)> = Vec::with_capacity(anchors.len());
    for &(i, j) in &anchors {
        let floor = on.last().map_or(n, |&(_, p)| p + 1);
        let mut c = j + n;
        while c < floor {
            c += n;
        }
        on.push((i, c));
    }
    let Some(&(i0, j0)) = on.first() else {
        return Ok(None);
    };
    // The columns of rows i_first to i_first + m - 1, piece by piece between paired corners.
    let mut cols: Vec<usize> = Vec::with_capacity(m);
    for (k, &(ia, ja)) in on.iter().enumerate() {
        let (ib, jb) = on.get(k + 1).copied().unwrap_or((i0 + m, j0 + n));
        cols.extend(share_columns(g, (ia, ib), (ja, jb)));
    }
    // Row i < i_first comes round after the last corner: its column, one turn of `b` back.
    let mut centre: Vec<usize> = (0..m)
        .map(|i| {
            let r = wrap(i + m - i_first, m);
            let c = cols.get(r).copied().unwrap_or(j0);
            if i < i_first { c.saturating_sub(n) } else { c }
        })
        .collect();
    let Some(&s) = centre.first() else {
        return Ok(None);
    };
    centre.push(s + n);
    // Columns from below n, so the start is one of `b`'s vertex numbers.
    let shift = s.checked_div(n).unwrap_or(0).saturating_mul(n);
    Ok(Some(
        centre
            .into_iter()
            .map(|c| c.saturating_sub(shift))
            .collect(),
    ))
}

/// Makes the band's centres run from `s` on row 0 to `end` on the last row and never back:
/// then each row's band meets the next one's and the walk's ends are in it. (The pin centres a
/// moved band on the middle of each row's run of the walk, which puts the last row's centre
/// short of the end when the walk runs along that row for more than the band's width, so the
/// end falls outside the band and no walk is found.)
fn settle(centre: &mut [usize], s: usize, end: usize) {
    let mut floor = s;
    for c in centre.iter_mut() {
        *c = (*c).clamp(floor, end);
        floor = *c;
    }
    if let Some(c) = centre.first_mut() {
        *c = s;
    }
    if let Some(c) = centre.last_mut() {
        *c = end;
    }
}

/// The least-cost walk from one start through a band of columns about where each of `a`'s
/// vertices falls ([`band_centres`]), as wide as `cells` allows (at most
/// `(m + 1)(2h + 1) + 2n` cells). A walk that runs along an edge of the band (not the grid's)
/// was held off whatever lies past it, so the band is moved to centre on the walk and searched
/// again, up to [`BAND_PASSES`] times; the last band's walk is kept. Keeping off the edge
/// proves nothing past the band, so the answer is never more than a bound.
fn banded_walk(
    g: &Grid,
    anchored: bool,
    cells: usize,
    work: &mut Work<'_>,
) -> Result<Walk, PairError> {
    let (m, n) = (g.a.count(), g.b.count());
    let h = cells
        .saturating_sub(n.saturating_mul(2))
        .checked_div(m + 1)
        .unwrap_or(0)
        .saturating_sub(1)
        / 2;
    if h == 0 {
        return Err(PairError::TooLarge {
            first: m,
            second: n,
        });
    }
    let mut centre = band_centres(g, anchored, cells, work)?.ok_or(PairError::Unreached)?;
    let s = centre.first().copied().ok_or(PairError::Unreached)?;
    let mut last: Option<Walk> = None;
    for _ in 0..BAND_PASSES {
        settle(&mut centre, s, s + n);
        // Each row's band also spans the columns of the rows either side, so a walk can step
        // from any column of one row's band into the next row and run along it.
        let column = |i: usize, c: usize| centre.get(i).copied().unwrap_or(c);
        let lo: Vec<usize> = (0..=m)
            .map(|i| {
                let c = column(i, s);
                let before = i.checked_sub(1).map_or(c, |b| column(b, c));
                c.min(before).saturating_sub(h).max(s)
            })
            .collect();
        let hi: Vec<usize> = (0..=m)
            .map(|i| {
                let c = column(i, s);
                (c.max(column(i + 1, c)) + h).min(s + n)
            })
            .collect();
        let w = walk(g, s, &lo, &hi, work)?;
        let (vlo, vhi) = w.bounds(m);
        let held = (0..=m).any(|i| {
            let at = |v: &[usize], b: &[usize]| v.get(i).zip(b.get(i)).is_some_and(|(x, y)| x == y);
            (at(&vlo, &lo) && lo.get(i).is_some_and(|&b| b > s))
                || (at(&vhi, &hi) && hi.get(i).is_some_and(|&b| b < s + n))
        });
        if !held {
            return Ok(w);
        }
        centre = vlo
            .iter()
            .zip(&vhi)
            .map(|(&a, &b)| a + b.saturating_sub(a) / 2)
            .collect();
        last = Some(w);
    }
    last.ok_or(PairError::Unreached)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn poly(pts: &[(f64, f64)]) -> Vec<Segment> {
        (0..pts.len())
            .map(|k| {
                let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                Segment::line(p(a.0, a.1), p(b.0, b.1))
            })
            .collect()
    }

    fn next(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (*seed >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A counter-clockwise star-shaped polygon of `n` vertices, each at its own angle and
    /// distance.
    fn random_polygon(seed: &mut u64, n: usize, centre: (f64, f64)) -> Vec<Segment> {
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|k| {
                let a = (k as f64 + 0.8 * next(seed)) * TAU / n as f64;
                let r = 5.0 + 10.0 * next(seed);
                (centre.0 + r * fmath::cos(a), centre.1 + r * fmath::sin(a))
            })
            .collect();
        poly(&pts)
    }

    fn grid(a: &[Segment], b: &[Segment]) -> Grid {
        Grid {
            a: Loop::new(0, a).unwrap().side(),
            b: Loop::new(1, b).unwrap().side(),
        }
    }

    fn free() -> (Budget, Stop) {
        (Budget::unlimited(), Stop::new())
    }

    /// The least cost of every walk from every start, by enumerating them: no table, no
    /// pruning, so it shares nothing with the search but the cost of a step.
    fn brute(g: &Grid) -> f64 {
        fn go(g: &Grid, i: usize, j: usize, s: usize, sofar: f64, best: &mut f64) {
            let (m, n) = (g.a.count(), g.b.count());
            if (i, j) == (m, s + n) {
                *best = best.min(sofar);
                return;
            }
            if i < m && j < s + n {
                go(g, i + 1, j + 1, s, sofar + g.pair(i, wrap(j, n)), best);
            }
            if i < m {
                go(
                    g,
                    i + 1,
                    j,
                    s,
                    sofar + g.alone_a(i, wrap(j + n - 1, n)),
                    best,
                );
            }
            if j < s + n {
                go(
                    g,
                    i,
                    j + 1,
                    s,
                    sofar + g.alone_b(wrap(j, n), wrap(i + m - 1, m)),
                    best,
                );
            }
        }
        let mut best = f64::INFINITY;
        for s in 0..g.b.count() {
            go(g, 0, s, s, 0.0, &mut best);
        }
        best
    }

    /// The cost of a walk's steps, summed again.
    fn cost_of(g: &Grid, w: &Walk) -> f64 {
        let (m, n) = (g.a.count(), g.b.count());
        let (mut i, mut j, mut c) = (0usize, w.start, 0.0);
        for s in &w.steps {
            match s {
                Step::Pair => {
                    c += g.pair(i, wrap(j, n));
                    (i, j) = (i + 1, j + 1);
                }
                Step::A => {
                    c += g.alone_a(i, wrap(j + n - 1, n));
                    i += 1;
                }
                Step::B => {
                    c += g.alone_b(wrap(j, n), wrap(i + m - 1, m));
                    j += 1;
                }
            }
        }
        assert_eq!((i, j), (m, w.start + n));
        c
    }

    /// Random polygons of 1 to 6 vertices (with arcs among them): the search's cost is the
    /// least of every walk from every start, and its steps sum to it.
    #[test]
    fn the_search_finds_the_brute_force_optimum() {
        let mut seed = 7u64;
        let mut cases = 0;
        for _ in 0..400 {
            let m = 1 + (next(&mut seed) * 6.0) as usize;
            let n = 1 + (next(&mut seed) * 6.0) as usize;
            let a = if m < 3 {
                bulged(&random_polygon(&mut seed, 3, (0.0, 0.0)), m)
            } else {
                random_polygon(&mut seed, m, (0.0, 0.0))
            };
            let off = (next(&mut seed) * 4.0, next(&mut seed) * 4.0);
            let b = if n < 3 {
                bulged(&random_polygon(&mut seed, 3, off), n)
            } else {
                random_polygon(&mut seed, n, off)
            };
            let g = grid(&a, &b);
            let (mut budget, stop) = free();
            let fast = best_walk(
                &g,
                &mut Work {
                    budget: &mut budget,
                    stop: &stop,
                },
            )
            .unwrap();
            let slow = brute(&g);
            assert!(
                (fast.cost - slow).abs() <= 1e-12 * slow.max(1.0),
                "{} by {}: {} against {slow}",
                g.a.count(),
                g.b.count(),
                fast.cost
            );
            assert!((cost_of(&g, &fast) - fast.cost).abs() <= 1e-12 * fast.cost.max(1.0));
            cases += 1;
        }
        assert_eq!(cases, 400);
    }

    /// The first `k` (1 or 2) vertices of a triangle as a loop of arcs: one whole circle
    /// through the first, or two arcs bulging out between the first two.
    fn bulged(tri: &[Segment], k: usize) -> Vec<Segment> {
        let (a, b) = (tri[0].start(), tri[1].start());
        let c = p(0.5 * (a.x + b.x), 0.5 * (a.y + b.y));
        if k == 1 {
            let r = fmath::hypot(a.x - c.x, a.y - c.y);
            vec![Segment::circle(c, r, Sense::Forward)]
        } else {
            vec![
                Segment::arc(c, a, b, Sense::Forward),
                Segment::arc(c, b, a, Sense::Forward),
            ]
        }
    }

    /// The centroid and area terms in closed form: a half disc of radius 2 about (1, -3) has
    /// area 2π and its centroid 8 / 3π above the centre.
    #[test]
    fn a_half_disc_has_its_closed_form_area_and_centroid() {
        let c = p(1.0, -3.0);
        let segs = [
            Segment::arc(c, p(3.0, -3.0), p(-1.0, -3.0), Sense::Forward),
            Segment::line(p(-1.0, -3.0), p(3.0, -3.0)),
        ];
        let s: Vec<Seg> = segs.iter().map(|s| Seg::of(s).unwrap().unwrap()).collect();
        let area: f64 = s.iter().map(Seg::area).sum();
        let (mx, my) = s
            .iter()
            .map(Seg::moments)
            .fold((0.0, 0.0), |(x, y), (a, b)| (x + a, y + b));
        assert!((area - 2.0 * core::f64::consts::PI).abs() < 1e-14);
        assert!((mx / area - 1.0).abs() < 1e-14, "{}", mx / area);
        let y = -3.0 + 8.0 / (3.0 * core::f64::consts::PI);
        assert!((my / area - y).abs() < 1e-14, "{}", my / area);
    }

    /// The pin's case of a 10 x 1 and a 1.5 x 1 rectangle in 498 and 500 segments through a
    /// band of 40,000 cells: the band's walk keeps off its band's edge, yet the search over
    /// every start finds one cheaper, so keeping off the edge proves nothing.
    #[test]
    fn a_banded_walk_off_its_edge_is_not_the_optimum() {
        let a = cut_rectangle(10.0, 1.0, [227, 22, 227, 22]);
        let b = cut_rectangle(1.5, 1.0, [150, 100, 150, 100]);
        let g = grid(&a, &b);
        let (mut budget, stop) = free();
        let mut work = Work {
            budget: &mut budget,
            stop: &stop,
        };
        let banded = banded_walk(&g, true, 40_000, &mut work).unwrap();
        let best = best_walk(&g, &mut work).unwrap();
        // Off the edge: one more pass through the band centred on it finds it again.
        let (m, n) = (g.a.count(), g.b.count());
        let whole = walk(
            &g,
            banded.start,
            &vec![0; m + 1],
            &vec![2 * n; m + 1],
            &mut work,
        )
        .unwrap();
        assert!((banded.cost - whole.cost).abs() <= 1e-9 * whole.cost);
        assert!(
            best.cost < banded.cost * (1.0 - 0.01),
            "optimum {} band {}",
            best.cost,
            banded.cost
        );
    }

    /// The `w` x `h` rectangle from (0, 0), its sides cut into `cuts` equal segments.
    pub(super) fn cut_rectangle(w: f64, h: f64, cuts: [usize; 4]) -> Vec<Segment> {
        let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
        let mut pts = Vec::new();
        for (k, &c) in cuts.iter().enumerate() {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            for q in 0..c {
                let t = q as f64 / c as f64;
                pts.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
        poly(&pts)
    }
}
