// Written for solidcorex. Apache-2.0.
//! scx-algo's box tree against the pin's `SpatialIndex` (part of [`crate::algo`]'s cases),
//! at `n` = 100, 1,000 and 10,000 boxes.
//!
//! The cases: building the tree over `n` boxes (`btree_build_<n>`), and, from a built tree,
//! every pair of boxes not disjoint at tolerance 0 (`btree_self_pairs_<n>`). The pin has no
//! self-join; its side is what its callers do (`assert_valid/face.rs`): a box query per item,
//! keeping the items after it. Each case reads one set of `n` boxes, so its length is `n`,
//! not [`crate::BATCH`]. The boxes are seeded: centres uniform in a cube whose side grows as
//! `n^(1/3)`, so each box meets a few others at every size, and half-widths uniform in
//! `[0, 1)`.

use crate::Rng;
use crate::case::Case;
use crate::pin_spatial::{self, SpatialIndex};

use scx_algo::box_tree::BoxTree;
use scx_num::extent3d::Extent3d as XE;
use scx_num::fmath;
use scx_num::vector3d::Point3 as XP;
use solidcore::geom::extent3d::Extent3d as SE;
use solidcore::math::Point3 as SP;
use std::hint::black_box;

/// The set sizes.
const SIZES: [usize; 3] = [100, 1_000, 10_000];

/// A box: min and max corners.
type Raw = ([f64; 3], [f64; 3]);

/// `n` seeded boxes.
fn boxes(seed: u64, n: usize) -> Vec<Raw> {
    #[allow(clippy::cast_precision_loss)] // n is at most 10^4
    let side = 1.5 * fmath::cbrt(n as f64);
    let mut r = Rng::new(seed);
    (0..n)
        .map(|_| {
            let c: [f64; 3] = r.array(0.0, side);
            let h: [f64; 3] = r.array(0.0, 1.0);
            (
                [c[0] - h[0], c[1] - h[1], c[2] - h[2]],
                [c[0] + h[0], c[1] + h[1], c[2] + h[2]],
            )
        })
        .collect()
}

fn xbox(b: &Raw) -> Option<XE> {
    XE::from_corners(
        XP::new(b.0[0], b.0[1], b.0[2]),
        XP::new(b.1[0], b.1[1], b.1[2]),
    )
}

fn sbox(b: &Raw) -> SE {
    SE::from_points(
        SP::new(b.0[0], b.0[1], b.0[2]),
        SP::new(b.1[0], b.1[1], b.1[2]),
    )
}

/// Whether `name` is one of this module's cases, whose length is its `n`.
pub fn is_sized(name: &str) -> bool {
    name.starts_with("algo/btree_")
}

/// The pin's pairs `i < j` of not-disjoint boxes, as its callers find them.
fn pin_self_pairs(index: &SpatialIndex, boxes: &[SE]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (i, b) in boxes.iter().enumerate() {
        out.extend(
            index
                .boxes_in_box(b, 0.0)
                .into_iter()
                .filter(|&j| j > i)
                .map(|j| (i, j)),
        );
    }
    out
}

/// The box tree's cases.
pub fn cases(out: &mut Vec<Case>) {
    for (k, n) in (0u64..).zip(SIZES) {
        let raw = boxes(81 + k, n);
        let xs: Vec<XE> = raw.iter().filter_map(xbox).collect();
        let ss: Vec<SE> = raw.iter().map(sbox).collect();
        debug_assert_eq!(xs.len(), n);
        out.push(Case::fold(
            &format!("algo/btree_build_{n}"),
            (xs.clone(), ss.clone()),
            |b| {
                let t = BoxTree::new(b);
                (black_box(&t).bounds(), t.len())
            },
            |b| {
                let t = SpatialIndex::new(b.iter().copied().map(Some));
                pin_spatial::root(black_box(&t))
            },
        ));
        let tree = BoxTree::new(&xs);
        let index = SpatialIndex::new(ss.iter().copied().map(Some));
        let mut case = Case::fold(
            &format!("algo/btree_self_pairs_{n}"),
            (vec![tree], vec![(index, ss)]),
            |t| t.iter().map(|t| t.self_pairs(0.0)).collect::<Vec<_>>(),
            |p| {
                p.iter()
                    .map(|(index, b)| pin_self_pairs(index, b))
                    .collect::<Vec<_>>()
            },
        );
        case.len = n;
        out.push(case);
    }
}
