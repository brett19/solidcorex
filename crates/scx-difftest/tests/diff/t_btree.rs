// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `box_tree.rs`, against the pin's `SpatialIndex`
//! (`topology/brep/validation/spatial.rs`, made public by the build script: `pin_spatial`).
//!
//! `BoxTree::overlapping` is compared with `SpatialIndex::boxes_in_box`, `self_pairs` with
//! `boxes_in_box` run on each item's own box keeping the later items (how the pin's callers
//! find pairs), and `pairs` with `boxes_in_box` on one index run for each box of the other
//! set. The two answer slightly different questions, and where they differ scx's answer
//! must be the brute-force one: every item (or pair) whose boxes are not disjoint within
//! `tol`, each pair tested in scx's documented orientation. The pin returns every item for
//! a negative or non-finite `tol` (its index finds candidates, and returns all of them when
//! it cannot cull), and tests the query box against the item's (`q.are_disjoint(b)`) where
//! scx tests the item's against the query (`b.are_disjoint(q)`); the two differ by a
//! rounding when a gap is within an ulp of `tol`.
//!
//! The sets: random boxes, boxes on an integer grid (ties and touching faces), boxes on a
//! grid of tenths (gaps within an ulp of a decimal `tol`), signed zeros, one box repeated,
//! points, and boxes near the largest finite `f64`; each at sizes 0 to 130.

use crate::harness::{Applies, Gen, PinDefect, SplitMix64, run_defect};
use scx_algo::box_tree::BoxTree;
use scx_num::extent3d::Extent3d as XE;
use scx_num::vector3d::Point3 as XP;

#[allow(
    clippy::all,
    clippy::pedantic,
    dead_code,
    missing_docs,
    unreachable_pub,
    unused_imports
)]
mod pin {
    use solidcore::geom::extent3d::Extent3d;
    use solidcore::math::Point3;
    include!(concat!(env!("OUT_DIR"), "/pin_spatial.rs"));
}
use pin::SpatialIndex;
use solidcore::geom::extent3d::Extent3d as SE;
use solidcore::math::Point3 as SP;

/// A box as `[x_min, y_min, z_min, x_max, y_max, z_max]`, `min <= max` and finite.
type B6 = [f64; 6];

/// The ways a box is drawn.
const KINDS: usize = 7;

/// One box of kind `kind`, among `n` (the region grows with `n`, so the density stays put).
fn draw(rng: &mut SplitMix64, kind: usize, n: usize) -> B6 {
    let side = 1.5 * scx_num::fmath::cbrt(n.max(1) as f64);
    let corner = |lo: [f64; 3], w: [f64; 3]| {
        [
            lo[0],
            lo[1],
            lo[2],
            lo[0] + w[0],
            lo[1] + w[1],
            lo[2] + w[2],
        ]
    };
    let mut c3 = |f: &mut dyn FnMut(&mut SplitMix64) -> f64| [f(rng), f(rng), f(rng)];
    match kind {
        // Random.
        0 => {
            let lo = c3(&mut |r| r.uniform(0.0, side));
            let w = c3(&mut |r| r.uniform(0.0, 1.0));
            corner(lo, w)
        }
        // An integer grid: equal keys and faces that touch.
        1 => {
            let m = side.ceil() as usize + 1;
            let lo = c3(&mut |r| r.below(m) as f64);
            let w = c3(&mut |r| r.below(3) as f64);
            corner(lo, w)
        }
        // A grid of tenths: gaps within an ulp of 0.1, 0.3 and their sums.
        2 => {
            let m = (10.0 * side) as usize + 1;
            let lo = c3(&mut |r| r.below(m) as f64 * 0.1);
            let w = c3(&mut |r| r.below(4) as f64 * 0.1);
            corner(lo, w)
        }
        // Signed zeros as bounds.
        3 => {
            const V: [f64; 4] = [-1.0, -0.0, 0.0, 1.0];
            let mut b = [0.0; 6];
            for a in 0..3 {
                let (p, q) = (V[rng.below(4)], V[rng.below(4)]);
                let (p, q) = if q < p { (q, p) } else { (p, q) };
                b[a] = p;
                b[a + 3] = q;
            }
            b
        }
        // One box.
        4 => [0.25, -0.5, 1.0, 1.25, 0.5, 1.0],
        // Points.
        5 => {
            let p = c3(&mut |r| r.below(4) as f64 * 0.5);
            corner(p, [0.0; 3])
        }
        // Near the largest finite value, where `max + tol` rounds.
        _ => {
            let big = f64::MAX / 2.0;
            let lo = c3(&mut |r| big * (0.5 + r.below(3) as f64 * 0.5));
            let w = c3(&mut |r| big * r.below(2) as f64 * 0.25);
            corner(lo, w)
        }
    }
}

/// A set of boxes.
#[derive(Clone, Debug)]
pub struct Set(Vec<B6>);

impl Gen for Set {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = Vec::new();
        for kind in 0..KINDS {
            for n in [0, 1, 2, 5, 9, 17, 40, 130] {
                v.push(Set((0..n).map(|_| draw(rng, kind, n)).collect()));
            }
        }
        // A mix of every kind.
        v.push(Set((0..64).map(|i| draw(rng, i % KINDS, 64)).collect()));
        v
    }
}

/// A query box.
#[derive(Clone, Debug)]
pub struct Query(B6);

impl Gen for Query {
    fn pool(rng: &mut SplitMix64) -> Vec<Self> {
        let mut v = vec![Query([-1e300, -1e300, -1e300, 1e300, 1e300, 1e300])];
        for kind in 0..KINDS {
            for n in [1, 17, 130] {
                for _ in 0..2 {
                    v.push(Query(draw(rng, kind, n)));
                }
            }
        }
        v
    }
}

/// A tolerance.
#[derive(Clone, Copy, Debug)]
pub struct T(f64);

impl Gen for T {
    fn pool(_: &mut SplitMix64) -> Vec<Self> {
        [
            0.0,
            -0.0,
            1e-300,
            0.1,
            0.3,
            0.5,
            1.0,
            1e300,
            -0.1,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ]
        .map(T)
        .to_vec()
    }
}

fn xbox(b: &B6) -> XE {
    XE::from_corners(XP::new(b[0], b[1], b[2]), XP::new(b[3], b[4], b[5]))
        // The sets hold finite boxes with min <= max.
        .unwrap()
}

fn sbox(b: &B6) -> SE {
    SE::from_points(SP::new(b[0], b[1], b[2]), SP::new(b[3], b[4], b[5]))
}

fn xboxes(s: &Set) -> Vec<XE> {
    s.0.iter().map(xbox).collect()
}

fn index(s: &Set) -> SpatialIndex {
    SpatialIndex::new(s.0.iter().map(|b| Some(sbox(b))))
}

/// The brute force: the items whose box is not disjoint from `q` within `tol`.
fn true_overlapping(s: &Set, q: &B6, tol: f64) -> Vec<usize> {
    let q = xbox(q);
    let b = xboxes(s);
    (0..b.len())
        .filter(|&i| !b[i].are_disjoint(&q, tol))
        .collect()
}

/// The brute force: the pairs `(i, j)` whose boxes are not disjoint within `tol`, `a`'s box
/// tested against `b`'s.
fn true_pairs(a: &[XE], b: &[XE], tol: f64, later_only: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (i, p) in a.iter().enumerate() {
        for (j, q) in b.iter().enumerate() {
            if (!later_only || j > i) && !p.are_disjoint(q, tol) {
                out.push((i, j));
            }
        }
    }
    out
}

fn same<R: PartialEq + std::fmt::Debug>(got: &R, truth: R) -> Result<(), String> {
    if *got == truth {
        Ok(())
    } else {
        Err(format!("the brute force gives {truth:?}"))
    }
}

fn overlapping_truth() -> PinDefect<(Set, Query, T), Vec<usize>> {
    PinDefect {
        reason: "the pin returns every item for a negative or non-finite tol, and tests the \
                 query against the box (a rounding apart when a gap is within an ulp of tol)",
        applies: Applies::Disagreeing,
        check: |(s, q, t), got| same(got, true_overlapping(s, &q.0, t.0)),
    }
}

#[test]
fn overlapping() {
    run_defect::<(Set, Query, T), _, _>(
        "t_btree::overlapping",
        &overlapping_truth(),
        |(s, q, t)| BoxTree::new(&xboxes(&s)).overlapping(&xbox(&q.0), t.0),
        |(s, q, t)| index(&s).boxes_in_box(&sbox(&q.0), t.0),
    );
}

fn self_pairs_truth() -> PinDefect<(Set, T), Vec<(usize, usize)>> {
    PinDefect {
        reason: "the pin returns every pair for a negative tol",
        applies: Applies::Disagreeing,
        check: |(s, t), got| {
            let b = xboxes(s);
            same(got, true_pairs(&b, &b, t.0, true))
        },
    }
}

#[test]
fn self_pairs() {
    run_defect::<(Set, T), _, _>(
        "t_btree::self_pairs",
        &self_pairs_truth(),
        |(s, t)| BoxTree::new(&xboxes(&s)).self_pairs(t.0),
        |(s, t)| {
            let idx = index(&s);
            let mut out = Vec::new();
            for (i, b) in s.0.iter().enumerate() {
                for j in idx.boxes_in_box(&sbox(b), t.0) {
                    if j > i {
                        out.push((i, j));
                    }
                }
            }
            out
        },
    );
}

fn pairs_truth() -> PinDefect<(Set, Set, T), Vec<(usize, usize)>> {
    PinDefect {
        reason: "the pin returns every pair for a negative tol",
        applies: Applies::Disagreeing,
        check: |(a, b, t), got| same(got, true_pairs(&xboxes(a), &xboxes(b), t.0, false)),
    }
}

#[test]
fn pairs() {
    run_defect::<(Set, Set, T), _, _>(
        "t_btree::pairs",
        &pairs_truth(),
        |(a, b, t)| BoxTree::new(&xboxes(&a)).pairs(&BoxTree::new(&xboxes(&b)), t.0),
        |(a, b, t)| {
            let idx = index(&b);
            let mut out = Vec::new();
            for (i, p) in a.0.iter().enumerate() {
                out.extend(idx.boxes_in_box(&sbox(p), t.0).into_iter().map(|j| (i, j)));
            }
            out
        },
    );
}
