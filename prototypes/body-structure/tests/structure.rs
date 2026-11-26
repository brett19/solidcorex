//! Behaviour the report relies on. Cell-count assertions share a global counter, so they run in
//! one test (`cells_carry_and_invalidate`).

use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use scx_body_structure::body::*;
use scx_body_structure::counting;
use scx_body_structure::ids::{GenTable, Slot};
use scx_body_structure::model::{Source, CELL_COMPUTES};
use scx_body_structure::table::Chunked;

fn same_result<B: Body>() -> (f64, usize) {
    let src = Source::torus(20, 20);
    let b = B::build(src.clone());
    let mut d = b.edit();
    d.apply_patch(&src.patch(9), 1.5);
    let (nb, out) = d.finish();
    (nb.traverse() + b.traverse(), out.fates.len())
}

#[test]
fn every_layout_gives_the_same_body() {
    let want = same_result::<TableBody<PlainF>>();
    assert_eq!(want, same_result::<TableBody<WholeF>>());
    assert_eq!(want, same_result::<TableBody<ChunkF>>());
    assert_eq!(want, same_result::<TableBody<TrieF>>());
    assert_eq!(want, same_result::<FaceBody<PlainF>>());
    assert_eq!(want, same_result::<FaceBody<ChunkF>>());
    assert_eq!(want, same_result::<FaceBody<Chunk16F>>());
    assert_eq!(want, same_result::<FaceBody<TrieF>>());
    // a 3x3 patch touches a 5x5 block: the patch, its edge neighbours and corner neighbours
    assert_eq!(want.1, 25);
}

#[test]
fn cells_carry_and_invalidate() {
    let src = Source::torus(16, 16);
    let old = FaceBody::<ChunkF>::build(src.clone());
    let n = old.n_faces() as u32;
    for f in 0..n {
        old.tree(f);
        old.bbox(f);
    }
    // (1) an edit of one face: untouched faces carry their filled cells, by pointer, no recompute
    let mut d = old.edit();
    d.apply_patch(&[5 * 16 + 5], 2.0);
    let (new, out) = d.finish();
    assert_eq!(out.fates.len(), 9);
    assert_eq!(out.fates.get(&(5 * 16 + 5)), Some(&FaceFate::Modified { geometry: true }));
    let before = CELL_COMPUTES.load(Relaxed);
    for f in 0..n {
        if out.fates.contains_key(&f) {
            assert_ne!(old.record_ptr(f), new.record_ptr(f));
            assert!(new.cells(f).tree.get().is_none(), "touched face {f} must start empty");
        } else {
            assert_eq!(old.record_ptr(f), new.record_ptr(f), "untouched face {f} is shared");
            assert!(new.cells(f).tree.get().is_some());
            new.tree(f);
            new.bbox(f);
        }
    }
    assert_eq!(CELL_COMPUTES.load(Relaxed), before, "no untouched face recomputed");

    // (2) a cell filled through the new body is visible through the old: it is the same face
    let mut d = old.edit();
    d.apply_patch(&[0], 3.0);
    let (newer, _) = d.finish();
    let f = 10 * 16 + 10;
    assert_eq!(old.record_ptr(f), newer.record_ptr(f));
    let fresh_old = {
        let mut d = old.edit();
        d.set_surface(f, scx_body_structure::model::geo(100, 9.0));
        d.finish().0
    };
    // fresh_old's face f is new; fill it there, then share it onwards
    assert!(fresh_old.cells(f).tree.get().is_none());
    let next = fresh_old.edit().finish().0; // an edit that touches nothing
    next.tree(f);
    assert!(fresh_old.cells(f).tree.get().is_some(), "filled through next, seen through fresh_old");

    // (3) changing a shared edge's tolerance invalidates both faces' cells, nothing else
    let e = 2 * (3 * 16 + 3);
    let faces = old.edge(e).faces;
    let mut d = old.edit();
    d.set_edge_tol(e, 1e-5);
    let (t, out) = d.finish();
    assert_eq!(out.fates.len(), 2);
    for f in 0..n {
        let hit = faces.contains(&f);
        assert_eq!(t.cells(f).tree.get().is_none(), hit, "face {f}");
    }
}

#[test]
fn racing_readers_get_one_value() {
    let b = Arc::new(TableBody::<ChunkF>::build(Source::torus(40, 40)));
    let n = b.n_faces() as u32;
    let results: Vec<Vec<f64>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8)
            .map(|t| {
                let b = Arc::clone(&b);
                s.spawn(move || {
                    // each reader walks the faces from a different start: they collide on fills
                    (0..n).map(|i| b.tree((i + t * 197) % n)[3]).collect::<Vec<f64>>()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    for t in 0..8u32 {
        // reader t's value for face f equals reader 0's
        for i in 0..n {
            let f = (i + t * 197) % n;
            assert_eq!(results[t as usize][i as usize], results[0][f as usize]);
        }
    }
    // every face's cell holds one value, and a later read returns that same allocation
    for f in 0..n {
        assert!(Arc::ptr_eq(&b.tree(f), &b.tree(f)));
    }
}

#[test]
fn transient_draft_writes_in_place() {
    let src = Source::torus(64, 64);
    let patch = src.patch(40);
    let shared = FaceBody::<ChunkF>::build(src.clone());
    let r0 = counting::read();
    let mut d = shared.edit();
    d.apply_patch(&patch, 1.0);
    let (a, _) = d.finish();
    let shared_alloc = counting::read().allocated_since(&r0);
    drop(a);
    let unique = FaceBody::<ChunkF>::build(src);
    let r0 = counting::read();
    let mut d = unique.into_draft();
    d.apply_patch(&patch, 1.0);
    let (b, _) = d.finish();
    let transient_alloc = counting::read().allocated_since(&r0);
    drop(b);
    // the transient draft allocates only the new geometry; the shared one also copies chunks
    // and face records. Allow for other tests' allocations on other threads.
    assert!(transient_alloc < shared_alloc, "{transient_alloc} vs {shared_alloc}");
}

#[test]
fn generational_ids_refuse_stale() {
    let mut t: GenTable<u64, Chunked<Slot<u64>, 16>> = GenTable::new();
    let a = t.insert(1);
    let b = t.insert(2);
    assert_eq!(t.remove(a), Some(1));
    let c = t.insert(3);
    assert_eq!(c.index, a.index, "the slot is reused");
    assert_eq!(t.get(a), None, "the old id is refused");
    assert_eq!(t.get(c), Some(&3));
    assert_eq!(t.get(b), Some(&2));
}
