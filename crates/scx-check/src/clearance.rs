// Written for solidcorex. Apache-2.0.
//! Vertex clearance (research 74 §3.3, the vertex half of `CF2b`'s planned clearance; built
//! by RP1, research 75 §13): two vertices of one body within coincidence of each other.
//!
//! Coincidence is 04's merged-points rule as `scx-stitch` reads it: two vertices are one
//! point when their distance is within the larger of their reaches (each recorded accuracy,
//! at least the resolution's zone). A body with such a pair breaks clearance, and a stitch
//! refuses the same pair (`StitchRefusal::VerticesWithinCoincidence`).
//!
//! **Method.** The vertices binned in a grid of cubes of side twice the body's widest
//! reach; a pair within that reach lies in one cube or two adjacent ones, since the
//! coordinates' quotients by the side differ by at most a half (and a rounding). Each vertex
//! is tested against the later vertices (in id order) of its own and its 26 neighbouring
//! cubes. An axis-aligned body puts whole faces in one `x` slice, which a sort along one
//! axis would test pair by pair; the grid tests only points within two sides of each other.
//! A body whose coordinates are past `2^52` sides falls back to one cube (every pair). The
//! distance is enclosed in intervals from the stored points, so the bracket holds the true
//! distance. A finding needs the upper bound within the allowance; a bracket that straddles
//! it is undecided ([`Why::Precision`]). Spread-out vertices cost `O(V log V)`; vertices
//! crowded within the widest reach cost up to `O(V²)` pair tests, each charged one budget
//! unit. It runs after the gaps, on what they left of the budget.

use std::collections::BTreeMap;

use scx_brep::{Brep, VertexId};
use scx_num::budget::{Budget, Stop};
use scx_num::interval::Ivl;
use scx_num::units::Length;
use scx_num::vector3d::Point3;
use scx_surface::Bracket;

use crate::error::CheckError;
use crate::findings::{Finding, Undecided, Why};
use crate::gaps::{Found, reach};

/// Every pair of vertices of `body` within coincidence, the lower id first. Each candidate
/// pair (in one cube or neighbouring ones) costs one unit; once the budget runs out, every
/// candidate left is undecided ([`Why::Exhausted`], the bracket `[0, inf)`), as a gap search
/// out of its share is (74 R6). Never a false finding, and never an error but the stop.
pub(crate) fn run(body: &Brep, budget: &mut Budget, stop: &Stop) -> Result<Found, CheckError> {
    let res = body.resolution();
    let items: Vec<(VertexId, Point3, f64)> = body
        .vertices()
        .map(|v| (v.id(), v.point(), reach(v.accuracy(), res)))
        .collect();
    // A charge past the limit would still count its work, so the remaining budget is read
    // first and the work stays within the limit.
    let n = items.len() as u64;
    let mut starved = budget.remaining() < n || budget.charge(n).is_err();
    let widest = items.iter().map(|it| it.2).fold(0.0, f64::max);
    let grid = Grid::new(items.iter().map(|it| it.1), widest);
    let mut out = Found {
        list: Vec::new(),
        undecided: Vec::new(),
    };
    for (i, &(a, p, ra)) in items.iter().enumerate() {
        if i % 1024 == 0 {
            stop.check().map_err(|_| CheckError::Cancelled)?;
        }
        for j in grid.near(p).filter(|&j| j > i) {
            let Some(&(b, q, rb)) = items.get(j) else {
                continue;
            };
            let allowed = ra.max(rb);
            let (a, b) = if a < b { (a, b) } else { (b, a) };
            starved = starved || budget.remaining() < 1 || budget.charge(1).is_err();
            if starved {
                if let Some(distance) = Bracket::new(0.0, f64::INFINITY) {
                    out.undecided.push(Undecided::VerticesWithinCoincidence {
                        a,
                        b,
                        distance,
                        allowed: Length::from_mm(allowed),
                        why: Why::Exhausted,
                    });
                }
                continue;
            }
            let Some(distance) = distance(p, q) else {
                continue;
            };
            match distance.exceeds(allowed) {
                Some(true) => {}
                Some(false) => out.list.push(Finding::VerticesWithinCoincidence {
                    a,
                    b,
                    distance,
                    allowed: Length::from_mm(allowed),
                }),
                None => out.undecided.push(Undecided::VerticesWithinCoincidence {
                    a,
                    b,
                    distance,
                    allowed: Length::from_mm(allowed),
                    why: Why::Precision,
                }),
            }
        }
    }
    Ok(out)
}

/// Points binned in cubes of side twice a reach, by index.
struct Grid {
    side: f64,
    /// `None`: one cube holds every point.
    cells: Option<BTreeMap<[i64; 3], Vec<usize>>>,
    all: Vec<usize>,
}

/// Past this many sides a coordinate's quotient is no longer an exact integer's neighbour.
const EXACT: f64 = 4_503_599_627_370_496.0;

impl Grid {
    fn new(points: impl Iterator<Item = Point3>, reach: f64) -> Self {
        let side = 2.0 * reach;
        let points: Vec<Point3> = points.collect();
        let all: Vec<usize> = (0..points.len()).collect();
        let mut cells: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
        for (i, &p) in points.iter().enumerate() {
            let Some(key) = cell(p, side) else {
                return Self {
                    side,
                    cells: None,
                    all,
                };
            };
            cells.entry(key).or_default().push(i);
        }
        Self {
            side,
            cells: Some(cells),
            all,
        }
    }

    /// The indices in the cube of `p` and its 26 neighbours: every point within the reach.
    fn near(&self, p: Point3) -> impl Iterator<Item = usize> + '_ {
        let (keys, all): (Vec<[i64; 3]>, &[usize]) = match (&self.cells, cell(p, self.side)) {
            (Some(_), Some([x, y, z])) => {
                let mut keys = Vec::with_capacity(27);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            keys.push([x + dx, y + dy, z + dz]);
                        }
                    }
                }
                (keys, &[])
            }
            _ => (Vec::new(), &self.all),
        };
        let cells = self.cells.as_ref();
        all.iter().copied().chain(
            keys.into_iter()
                .filter_map(move |k| cells.and_then(|c| c.get(&k)))
                .flatten()
                .copied(),
        )
    }
}

/// The cube of `p`; `None` past [`EXACT`] sides or for a zero side.
fn cell(p: Point3, side: f64) -> Option<[i64; 3]> {
    if side <= 0.0 || !side.is_finite() {
        return None;
    }
    let k = |c: f64| {
        let q = (c / side).floor();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "within 2^52, checked just before"
        )]
        (q.is_finite() && q.abs() < EXACT).then(|| q as i64)
    };
    Some([k(p.x)?, k(p.y)?, k(p.z)?])
}

/// The enclosed distance between two stored points; `None` when it overflows.
fn distance(p: Point3, q: Point3) -> Option<Bracket> {
    let d = |a: f64, b: f64| Ivl::from(a) - Ivl::from(b);
    let (x, y, z) = (d(p.x, q.x), d(p.y, q.y), d(p.z, q.z));
    let n = (x * x + y * y + z * z).sqrt();
    if !(n.lo.is_finite() && n.hi.is_finite()) {
        return None;
    }
    Bracket::new(n.lo.max(0.0), n.hi)
}
