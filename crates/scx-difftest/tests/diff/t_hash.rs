// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `hash_table.rs`. Scripted runs of the voxel model: a seed draws up to 160
//! operations (adds of items at quantised points, so that many coincide, with and
//! without the updated-voxel list; removes; coincident-pair searches with a limit index,
//! an explicit bucket list (indices past the table included) and the gap histogram;
//! voxel and box queries; resizes), and every operation's result and the whole model
//! after it (point items, buckets, bucket items, counts, capacities, box) are compared.
//! The scx side reads no tolerances (the pairs' tolerances are the zone sums'), the pin
//! the default kernel's.

use crate::diff;

/// A SplitMix64 step.
fn next(s: &mut u64) -> u64 {
    *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *s;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `None` as `usize::MAX`.
fn o(x: Option<usize>) -> usize {
    x.unwrap_or(usize::MAX)
}

/// One operation's record: (operation, integers, numbers, error text).
type Rec = (usize, Vec<usize>, Vec<f64>, Option<String>);

/// The model's state, as a record.
macro_rules! state {
    ($v:expr) => {{
        let v = &$v;
        let h = v.hash_table();
        let mut ints = vec![
            v.len(),
            usize::from(v.is_empty()),
            v.removed_point_item_count(),
            v.point_capacity(),
            v.counts().0,
            v.counts().1,
            v.counts().2,
            h.capacity(),
            o(h.first_used_bucket()),
            h.bucket_count(),
            h.used_bucket_count(),
            h.total_bucket_item_count(),
            h.used_bucket_item_count(),
            h.removed_bucket_item_count(),
        ];
        ints.extend(h.buckets().iter().map(|&b| o(b)));
        for b in h.bucket_items() {
            ints.extend([
                o(b.next_used_bucket),
                o(b.next_bucket_item),
                o(b.bucket_index),
                o(b.item),
                b.item_id,
                usize::from(b.is_removed()),
            ]);
        }
        let mut nums = Vec::new();
        for p in v.point_items() {
            ints.push(p.item as usize);
            match m::ht_placed(p) {
                Some(q) => {
                    ints.push(1);
                    nums.extend(q);
                }
                None => ints.push(0),
            }
        }
        if let Some(b) = m::ht_bbox(&v) {
            let (lo, hi) = (b.min(), b.max());
            nums.extend([lo.x, lo.y, lo.z, hi.x, hi.y, hi.z]);
        }
        (255usize, ints, nums, None::<String>)
    }};
}

diff!(ht_voxels, |seed: usize, cnt: [f64; 3], scale: f64| {
    use m::hash_table::{FindPairsOptions, ObjsInVoxels, GAP_HISTOGRAM_BUCKETS};
    let mut r = seed as u64 * 0x1234_5678 + cnt[0].to_bits();
    let dims = cnt.map(|c| (c.abs() as usize) % 5);
    let unit = m::e3([0.0, 0.0, 0.0, scale, scale, scale]);
    let bbox = unit.as_ref().filter(|_| seed % 4 != 0);
    let mut out: Vec<Rec> = Vec::new();
    let mut v = match ObjsInVoxels::<u32>::new(dims[0], dims[1], dims[2], bbox, seed % 70) {
        Ok(v) => v,
        Err(e) => return vec![(0, Vec::new(), Vec::new(), Some(m::err_text(&e)))],
    };
    out.push(state!(v));
    let q = |r: &mut u64| (next(r) % 9) as f64 / 8.0 * scale + (next(r) % 3) as f64 * 1e-7;
    for _ in 0..160 {
        let op = next(&mut r) % 12;
        let rec: Rec = match op {
            0..=4 => {
                let id = (next(&mut r) % 40) as u32;
                let p = m::p3([q(&mut r), q(&mut r), q(&mut r)]);
                let zt = [0.0, 1e-6, 1e-3, 0.1][(next(&mut r) % 4) as usize];
                let mut upd = Vec::new();
                let res = if next(&mut r) % 2 == 0 {
                    v.add(id, p, zt, Some(&mut upd))
                } else {
                    v.add(id, p, zt, None)
                };
                match res {
                    Ok(x) => (1, [vec![o(x)], upd].concat(), Vec::new(), None),
                    Err(e) => (1, upd, Vec::new(), Some(m::err_text(&e))),
                }
            }
            5 | 6 => {
                let id = (next(&mut r) % 40) as u32;
                (
                    2,
                    vec![v.remove(id), v.find_point_items(id).len()],
                    Vec::new(),
                    None,
                )
            }
            7 | 8 => {
                let mut hist = [0u64; GAP_HISTOGRAM_BUCKETS];
                let bl: Vec<usize> = (0..next(&mut r) % 4)
                    .map(|_| (next(&mut r) % 80) as usize)
                    .collect();
                let mode = next(&mut r);
                let opts = FindPairsOptions {
                    limit_index: (mode % 3 == 0).then(|| (next(&mut r) % 30) as usize),
                    buckets: (mode % 5 == 0).then_some(&bl[..]),
                    histogram: (mode % 2 == 0).then_some(&mut hist),
                };
                match m::ht_find_pairs(&mut v, opts) {
                    Ok(p) => (
                        3,
                        [p.first, p.second, hist.map(|h| h as usize).to_vec()].concat(),
                        Vec::new(),
                        None,
                    ),
                    Err(e) => (3, Vec::new(), Vec::new(), Some(m::err_text(&e))),
                }
            }
            9 => {
                let p = m::p3([q(&mut r) * 1.3 - 0.1, q(&mut r), q(&mut r) * 0.9]);
                let i = (next(&mut r) % 70) as usize;
                let (ix, iy, iz) = (
                    (next(&mut r) % 5) as usize,
                    (next(&mut r) % 5) as usize,
                    (next(&mut r) % 5) as usize,
                );
                let hit = v.map_point_to_voxel(p);
                let mut ints = vec![v.map_indices_to_voxel_index(ix, iy, iz)];
                let mut nums = Vec::new();
                if let Some(b) = m::ht_voxel_bbox(&v, i) {
                    let (lo, hi) = (b.min(), b.max());
                    nums.extend([lo.x, lo.y, lo.z, hi.x, hi.y, hi.z]);
                }
                if let Some(h) = hit {
                    ints.extend([h.index, h.ix, h.iy, h.iz]);
                    let (lo, hi) = (h.extent.min(), h.extent.max());
                    nums.extend([lo.x, lo.y, lo.z, hi.x, hi.y, hi.z]);
                }
                let err = match v.map_voxel_index_to_indices(i) {
                    Ok((a, b, c)) => {
                        ints.extend([a, b, c]);
                        None
                    }
                    Err(e) => Some(m::err_text(&e)),
                };
                (4, ints, nums, err)
            }
            10 => {
                let d = [next(&mut r) % 4, next(&mut r) % 4, next(&mut r) % 4].map(|x| x as usize);
                let res = v.set_size(d[0], d[1], d[2], bbox, (next(&mut r) % 70) as usize);
                (
                    5,
                    d.to_vec(),
                    Vec::new(),
                    res.err().map(|e| m::err_text(&e)),
                )
            }
            _ => {
                let gaps = [0.0, 1e-9, 1e-6, 1e-3, 0.5, 2.0, f64::NAN];
                let g = gaps[(next(&mut r) % 7) as usize];
                (
                    6,
                    vec![m::hash_table::gap_histogram_index(g)],
                    Vec::new(),
                    None,
                )
            }
        };
        out.push(rec);
        out.push(state!(v));
    }
    out
});

diff!(ht_table, |seed: usize, estimate: usize, buckets: usize| {
    use m::hash_table::HashTable;
    let mut out: Vec<(Vec<usize>, Option<String>)> = Vec::new();
    let mut h = match HashTable::new(estimate * 7, buckets * 13) {
        Ok(h) => h,
        Err(e) => return vec![(Vec::new(), Some(m::err_text(&e)))],
    };
    let mut r = seed as u64 + 77;
    for _ in 0..200 {
        let op = next(&mut r) % 6;
        let res = match op {
            0..=2 => {
                let b = (next(&mut r) % (buckets as u64 * 13 + 3)) as usize;
                h.add((next(&mut r) % 50) as usize, b, (next(&mut r) % 9) as usize)
                    .err()
                    .map(|e| m::err_text(&e))
            }
            3 => {
                let idx: Vec<usize> = (0..next(&mut r) % 5)
                    .map(|_| (next(&mut r) % 60) as usize)
                    .collect();
                let n = h.remove(&idx);
                out.push((vec![n], None));
                None
            }
            4 => {
                h.inc_max_item_count();
                None
            }
            _ => h
                .init_size((next(&mut r) % 30) as usize, (next(&mut r) % 30) as usize)
                .err()
                .map(|e| m::err_text(&e)),
        };
        let mut s = vec![
            h.capacity(),
            o(h.first_used_bucket()),
            h.bucket_count(),
            h.used_bucket_count(),
            h.total_bucket_item_count(),
            h.used_bucket_item_count(),
            h.removed_bucket_item_count(),
        ];
        s.extend(h.buckets().iter().map(|&b| o(b)));
        for b in h.bucket_items() {
            s.extend([
                o(b.next_used_bucket),
                o(b.next_bucket_item),
                o(b.bucket_index),
                o(b.item),
                b.item_id,
            ]);
        }
        out.push((s, res));
    }
    out
});
