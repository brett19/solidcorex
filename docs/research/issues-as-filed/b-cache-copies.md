The contexts that keep a face's surface cache for the length of an operation hand it out by deep copy. Every solve that asks for the cache gets a fresh copy of the whole decomposition (tree, patches, auxiliary data and an evaluation copy of the surface), and a second full copy when it only wants to read a few state bits. The kept cache is built once and then copied hundreds of times.

## Measured

On main at 782ef9afc, with temporary counters at `SurfaceCache::map_handles`, `Clone for SurfaceCache` and the tree build, binned by call site.

| Operation | Cache trees built | Whole-cache copies |
|---|---|---|
| One constant-radius fillet of one edge of a 100 x 100 x 10 box | 2 | 260 |
| One face pair of a Boolean between a sphere and a cylinder | 2 | 8 |
| One face pair of a Boolean between a torus and a sphere | 4 | 13 |

Where the fillet's 260 come from:

| Copies | Call site |
|---|---|
| 128 | `query/brep/ray/live.rs`, `BrepFaceServices::cache_mapped`, the kept-cache path |
| 128 | `query/brep/ray/live.rs`, `BrepFaceServices::cache_state_mapped`, the kept-cache path |
| 4 | the same two functions when the cache is first made |

So 128 solves each took one full copy to solve on and another full copy to read the state bits of.

In the Booleans the copies come from the same functions' first-use path (3 to 4), the surface intersector's trimmed face context in `intersect/surface_intersector/p3.rs` (4 to 8), and the cache's evaluation-copy clone (2).

Booleans between planes and cylinders build no cache tree and make no copy, so they are unaffected.

## Why the copies are made

`SurfaceSolveContext::build_surface_cache` and `surface_cache_without_get_tree` (`surface/surface/p1.rs`) return an owned `SurfaceCache`, so a context that holds one cache per face can only answer by copying it. Of the copy sites found by reading:

- most copy only because the cache cannot be lent across the call; the copy's content is the original's;
- three copy the whole cache to read a few fields of its state;
- two exist because the consumer changes its copy, one of them only to switch the point test off for one solve (`surface/surface/p7.rs`): the solve modes `point_test_enabled` and `process_boundary_curves` are stored in the cache instead of being passed to the solve;
- a few really need another handle type. None of those fire in the cases above.

## Proposed change

In three steps, none of which changes a result:

1. The state bits are read from the kept cache through a borrowed query instead of a copy. This removes half of the fillet's copies.
2. The two solve modes become arguments of the solve, so a consumer never needs its own copy to change them.
3. The contexts lend the kept cache to the solve instead of copying it.

A kept cache is then built once per face and read in place for as long as it is kept.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
