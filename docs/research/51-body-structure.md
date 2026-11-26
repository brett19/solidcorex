# 51. The body's data structure: what to share, how to chunk it, and what an edit reports

**Question.** How should an immutable, reference-counted finished body be laid out, so that an edit of
k of n faces costs about k and not n, the cells of untouched faces carry over, and reads stay fast?
What does solidcore do today, and how much of a body does each kind of operation actually leave alone?

**Labels.** MEASURED: counted or timed here; the data is in `51-data/` and the rerun steps are in
section 8. READ: read from source. INFERRED: reasoned from the two.

**Where the numbers come from.**
- solidcore numbers come from a scratch probe on server2. The tree is solidcore main e3b1b2dbb6, and
  the probe is a test file that was never committed.
- Prototype numbers come from `prototypes/body-structure/`, written from scratch with std only. No
  solidcore code is in it.
- Times are wall-clock on server2 under `nice 10`, release build, single thread. Read the times as
  ratios. The bytes are exact.

## Summary

- **Untouched fraction per operation, input faces (MEASURED, by content; section 3):**

  | Operation | Untouched input faces |
  |---|---|
  | Boolean hole in a box | 67% |
  | Pattern instance, 17th / 65th / 65th-on-filleted hole | 91% / 97% / 98.5% |
  | Extrude-join boss on a box / on a 70-face plate | 83% / 98.6% |
  | Corner cut on corpus parts | 58-89% |
  | Fillet, one box edge | 33% |
  | Fillet, one rim on the 70-face plate | **7%** (63 faces it did not need to touch were rewritten; edges 99% kept) |
  | Shell, open top | 83-95% (box keeps 5 face ids, plate4 only 5 of 22; every edge and vertex gets a new id) |
  | Offset | 0% |
  | Transform | 0% |

  Edges and vertices are kept at 94-100% for every local operation.
- **Recommended representation:**
  - **Per-face records.** A face is one `Arc<FaceRecord>` holding its loops, edge uses, trims, surface
    and fill-once cells. Edges and vertices sit in id-indexed tables.
  - **Behind one table trait.** Start with whole-table copy-on-write (`Arc<Vec>`). Switch the trait's
    implementation to chunks of 64 if large bodies are edited repeatedly. Callers do not change: the
    prototype runs both through one test suite with identical results.
  - **The decisive design point is not the container.** Geometry and cells must be shared per face,
    because face caches are 5-160 times the body (section 1).
- **Option (1) is enough for the app corpus.** That option is whole-table copy-on-write, with geometry
  and cells behind per-face `Arc`s.
  - App bodies have a median of 8 faces, a p99 of 94 and a maximum of 123 (MEASURED from research 36's
    2,104 bodies).
  - Copying the records of a 1,024-face body costs 83 µs and 380 KB (prototype). At app sizes that is
    a few µs against operations of 5 ms to 3 s.
  - Chunking pays only at about 10,000 faces and up, where whole-table copy-on-write costs 0.8-15 ms
    and 3.6-37 MB per edit, against 10-50 µs and 35-125 KB chunked.
  - The price of chunking is reads: 1.1-2.5 times a plain `Vec` on pure topology walks (section 4).
- **Today (MEASURED):**
  - A solidcore body costs 9-21 KB per face.
  - 71-80% of it is the use records (faceuse, loopuse, edgeuse, vertexuse), and trims are 34-53% of
    the body on NURBS parts.
  - Every copy is deep. The cheapest that keeps ids costs 15.6 µs and 55 KB on a box, and 579 µs and
    2.7 MB on the 134-face filleted plate.

## 1. What a solidcore body holds (MEASURED; structure READ)

**Method.**
- Each of the body's ten arenas is cloned on its own, and the bytes the clone allocates are counted.
- Geometry is counted the same way, by cloning every face's surface, every edge's curve and every edge
  use's trim. The arena figures include their geometry.
- "Face caches" is the live growth after building each face's trimmed-surface cache and its tree
  (`face_surface_cache` then `build_tree`, then `release`) on a private copy.

KB; uses = faceuses + loopuses + edgeuses + vertexuses arenas.

| Body | Faces | Whole body | Faces (incl. surfaces) | Loops | Edges (incl. curves) | Vertices | Uses (incl. trims) | Trims (count) | Attrs | Surfaces | Curves | Face caches | Caches / body |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| box | 6 | 55.0 | 5.2 | 0.4 | 11.0 | 1.7 | 44.4 | 18.9 (24) | 0 | 4.7 | 5.5 | 44 | 0.8x |
| plate4_drilled | 22 | 379.6 | 58.9 | 4.3 | 62.0 | 4.4 | 292.0 | 100.7 (120) | 3.0 | 44.2 | 36.3 | 2,839 | 7.5x |
| plate8_drilled | 70 | 1,352 | 219.9 | 15.9 | 215.0 | 12.7 | 1,035 | 345.9 (408) | 10.2 | 162.7 | 128.5 | 11,246 | 8.3x |
| plate8_drilled_filleted | 134 | 2,751 | 502.2 | 27.9 | 399.2 | 30.7 | 1,961 | 547.9 (664) | 0 | 321.2 | 205.8 | 23,016 | 8.4x |
| blade_hub_3.smb | 46 | 618.7 | 58.4 | 1.9 | 104.3 | 7.2 | 553.7 | 270.7 (252) | 0 | 54.6 | 95.4 | 11,423 | 18.5x |
| vase3.smb | 19 | 358.6 | 58.2 | 0.8 | 80.8 | 2.8 | 234.5 | 145.5 (85) | 0 | 56.6 | 77.4 | 6,301 | 17.6x |
| cliff_1.smb | 43 | 577.0 | 61.8 | 1.8 | 92.8 | 6.7 | 501.0 | 243.7 (234) | 0 | 58.3 | 84.5 | 10,967 | 19.0x |
| test8.smb | 2 | 108.0 | 21.2 | 0.2 | 29.9 | 0.4 | 57.6 | 48.2 (10) | 0 | 21.7 | 29.4 | 17,332 | 160x |
| crown.smb | 13 | 347.3 | 19.5 | 0.5 | 84.1 | 2.1 | 252.3 | 182.9 (72) | 0 | 18.5 | 81.6 | 1,674 | 4.8x |
| Platte.smb (largest .smb) | 122 | 1,563 | 210.0 | 5.7 | 177.0 | 18.7 | 1,191 | 545.8 (654) | 0 | 199.9 | 154.0 | 13,676 | 8.7x |
| tiny_rounds_coarse_tolerance.step | 3 | 36.1 | 9.6 | 0.2 | 6.2 | 0.8 | 14.3 | 8.7 (11) | 0 | 6.5 | 5.5 | 406 | 11.2x |

Rows for the cylinder, the sphere, vase1, blade_hub_1, cliff_2, nurb_box1, bottle and
cylinder_spline_rims.step are in `51-data/solidcore-probe.tsv`.

- **The use records are most of the body.** solidcore is a radial-edge structure (READ): four edge
  uses per manifold edge, faceuse pairs and loopuse pairs. Excluding trims, the uses are 46% of a box
  and 51% of the filleted plate.
  - An edge use averages about 670 B excluding its trim. It carries its gap caches inline (READ:
    `max_edge_face_gap`, `edge_ccw_edge_gap`, `drop_curve_fail`).
  - A rewrite with half-edges or plain loop-edge lists and no inline caches would roughly halve the
    topology (INFERRED).
- **Trims are the largest single piece of geometry.** They are 34-53% of NURBS bodies and 17-40% of
  analytic ones, which agrees with research 47.
- **Face caches dwarf the body:** 5-19 times on real parts and 160 times on test8. These are the
  trimmed-surface cache plus the tree.
  - This is why sharing an untouched face's cells matters far more than how the records are chunked.
  - A cache that has to be rebuilt after every edit costs more than any copy of the body (INFERRED).
  - Research 26's per-face tree figures were not re-derived here.
- **Attributes are small.** Bodies built by Booleans carry about 3-10 KB; the others carry none.
- **The app corpus is small (MEASURED, research 36 data):** 2,104 bodies, a median of 8 faces, a
  mean of 15, a p90 of 36, a p99 of 94 and a maximum of 123.
- **No large STEP part was measured.** The public NIST set could not be fetched: server2 has no
  `unzip`, and the rules forbid installing packages. The largest STEP regression,
  `rod_doubled_edge.step`, gave no body through `Body::from_record`. Bodies of 10,000 faces or more
  are therefore covered only by the prototype. This is an open risk (section 7).

## 2. Copies and snapshots today (MEASURED, median of 5)

µs / bytes allocated.

| Body | Faces | `brep_copy` (new ids) | `clone_standing_for_self` | `clone_sharing_surface_caches` | `with_rollback` no-op (snapshot) |
|---|---|---|---|---|---|
| box | 6 | 50.7 / 207 KB | 15.6 / 56 KB | 2.6 / 16 KB | 15.0 / 54 KB |
| plate8_drilled | 70 | 1,263 / 4.10 MB | 332 / 1.38 MB | 74.6 / 478 KB | 378 / 1.38 MB |
| plate8_drilled_filleted | 134 | 2,289 / 7.25 MB | 579 / 2.80 MB | 147 / 948 KB | 631 / 2.76 MB |
| blade_hub_3.smb | 46 | 691 / 2.09 MB | 161 / 634 KB | 28.1 / 165 KB | 170 / 625 KB |
| Platte.smb | 122 | 1,845 / 6.23 MB | 460 / 1.60 MB | 93.6 / 488 KB | 481 / 1.56 MB |

- **Every kind of copy is linear in the body (READ).**
  - `Shared<Arena>` makes a whole-arena deep copy on the first write (`Arc::make_mut`).
  - Only the edges and edgeuses arenas are `Shared`, and only `clone_sharing_surface_caches` shares
    them, which is why it is 4-6 times cheaper.
  - The snapshot does not share, because the copy constructor's owner reset fails check Test #61.
  - `brep_copy` costs 3-4 times `clone_standing_for_self`, because it renumbers and rebuilds through
    the attribute machinery.
- **Every copy loses the caches.** A cloned `Surface` starts with an empty cache slot (READ), so it
  loses its trimmed cache and tree, which are 5-19 times the body. The copy itself is cheap; the
  rebuild of the caches it dropped is not.
- **In context:**
  - Research 38: 1,376 raw copies take 607 ms, 492 snapshots take 116 ms, and copy, snapshot,
    rebuild and record traffic are 4.5% of evaluation.
  - The cache rebuilds after copies are not in that 4.5% (INFERRED from research 26 and 36).

## 3. How much of a body each operation leaves untouched (MEASURED)

**Matching.** Every vertex, edge and face of each input gets a fingerprint, and each output entity is
matched against the input's fingerprints as a multiset.
- **Vertex:** its point and tolerance, quantised to 1e-8.
- **Edge:** its two vertex prints (unordered), its tolerance, and its curve kind with three sample
  points (at 10%, 50% and 90% of the interval, unordered).
- **Face:** its tolerance, its uv domain, the surface sampled on a 3x3 grid of the domain, and the
  multiset of its edges' prints. The face's edges are taken through its edge uses, because a body
  read from `.smb` leaves `Loop::edges` empty.
- **"Kept by id":** the same id with the same print. solidcore never reuses an arena slot (READ), so
  this is meaningful.
- **Control:** `brep_copy` of every corpus part matches 100% of faces, edges and vertices.

Input faces kept (by content / by id), edges kept, vertices kept. Booleans report the target
operand; the tool operand keeps 0% for cuts, and 2 of 3 faces for a join (its top and side).

| Operation | Input | Faces | Edges | Vertices | Time |
|---|---|---|---|---|---|
| Boolean: hole through a box | 6 faces | 67% / 67% | 100% | 100% | 8 ms |
| Pattern instance: 17th hole in plate4 | 22 | 91% / 91% | 100% | 100% | 221 ms |
| Pattern instance: 65th hole in plate8 | 70 | 97% / 97% | 100% | 100% | 1.46 s* |
| Boolean: hole in filleted plate8 | 134 | 98.5% / 98.5% | 100% | 100% | 3.29 s* |
| Extrude-join: boss on a box | 6 | 83% / 83% | 100% | 100% | 5 ms |
| Extrude-join: boss on plate8 | 70 | 98.6% / 98.6% | 100% | 100% | 1.46 s* |
| Fillet: one edge of a box | 6 | 33% / 33% | 58% | 75% | 6 ms |
| Fillet: one rim of plate8 | 70 | **7% / 7%** | 99% | 99% | 1.49 s* |
| Fillet: all 64 top rims of plate8 | 70 | 7% / 7% | 37% | 53% | 3.28 s* |
| Shell: box, top open | 6 | 83% / 83% | 100% / 0% | 100% / 0% | 55 ms |
| Shell: plate4, top open | 22 | 95% / 23% | 100% / 0% | 100% / 0% | 2.11 s |
| Offset: box +1 | 6 | 0% | 0% | 0% | 55 ms |
| Offset: plate4 +0.5 | 22 | 0% | 0% | 0% | 1.23 s |
| Transform: plate8 translated | 70 | 0% | 0% | 0% | (in place) |
| Corner cut: blade_hub_3.smb | 46 | 89% | 94% | 96% | 150 ms |
| Corner cut: cliff_1 / blade_hub_1 | 43 | 88% | 94% | 96% | 112 ms |
| Corner cut: bottle | 16 | 88% | 93% | 96% | 122 ms |
| Corner cut: crown | 13 | 85% | 97% | 100% | 20 ms |
| Corner cut: cliff_2 | 30 | 60% | 73% | 80% | 124 ms |
| Corner cut: vase1 / vase3 | 19 | 74% / 58% | 83% / 77% | 88% | 214 / 312 ms |
| Corner cut: nurb_box1 / test8 | 5 / 2 | 40% / 50% | 67% / 71% | 83% / 80% | 207 / 64 ms |

\* These times include building the input plate in the same call.

Corner cuts on Platte.smb and the two STEP parts missed the part and are left out. The first and
last 15% of each corner box did not reach the material.

- **The fillet result is a finding about solidcore, not about filleting.** One rim fillet on plate8
  rewrote 63 of the 64 other holes' faces, which it did not need to change (diagnosed with a per-field
  comparison, MEASURED):
  - their uv domains came back transposed, from (u 0..15.7, v 2..7) to (u 2..7, v 0..15.7), so the
    surface was re-parameterised;
  - their tolerances, loops and edges are unchanged, and 202 of 204 edges are kept.
  - For sharing, a re-parameterised face is a new face: its trims and cells are invalid. A kernel
    that wants sharing must not rewrite faces it does not need to (INFERRED).
  - Shell gives every edge and vertex a new id, and most plate4 faces too, but keeps 83-95% of faces by content. The body is copied
    first and then edited (READ: `shell_brep` takes `&Brep` and returns a new body), so identity is
    lost although the content survives.
- **Offset and transform keep nothing, by construction.** A body that stores a placement (a rigid
  transform applied at read time) could keep 100% for a transform (INFERRED; a design choice, see
  section 7).
- **On local operations, edges and vertices survive at 94-100% even when faces do not.** The small
  "2.8% of edge uses" proxy quoted in `03-upfront-planning.md` is consistent with this: a Boolean
  changes very little.

## 4. Candidate representations (prototype, MEASURED)

**The prototype body** (`prototypes/body-structure`) is a closed torus quad mesh: every edge has two
faces, every vertex four edges, and every face one loop of four uses.
- Geometry sits behind `Arc`. Per face there is a 100-coefficient surface, two 24-coefficient curves
  and four 16-coefficient trims, so the records hold about 372 B per face and geometry about 1.9 KB.
- **The edit.** It changes the surfaces of k faces in a square patch. It also moves their vertices,
  gives every edge at a moved vertex a new curve, and re-trims every face on a changed edge, so one
  edited face touches 9 faces and 12 edges.
- **Floor.** The new geometry alone is 7.6 KB for k = 1, measured with a transient draft that copies
  nothing.

### 4.1 Edit cost: time / bytes newly allocated / fraction of the old body's bytes shared

k = 1 (9 faces touched):

| Layout | n=100 | n=1,024 | n=10,000 | n=102,400 |
|---|---|---|---|---|
| (1) table, whole-table copy-on-write | 11 µs / 44 KB / 80% | 83 µs / 380 KB / 83% | 789 µs / 3.6 MB / 84% | 14.5 ms / 37 MB / 84% |
| (2) table, chunks of 64 | 8 µs / 31 KB / 86% | 13 µs / 47 KB / 98% | 21 µs / 69 KB / 99.7% | 51 µs / 125 KB / 99.94% |
| (4) table, 32-way trie | 8 µs / 26 KB / 88% | 11 µs / 37 KB / 98% | 12 µs / 37 KB / 99.8% | 14 µs / 40 KB / 99.98% |
| (3) per-face, plain `Vec` of `Arc` | 6 µs / 28 KB / 88% | 33 µs / 201 KB / 91% | 326 µs / 1.9 MB / 92% | 7.1 ms / 19 MB / 92% |
| (3) per-face, chunks of 64 | 5 µs / 22 KB / 90% | 5.5 µs / 26 KB / 99% | 9.6 µs / 36 KB / 99.8% | 32 µs / 81 KB / 99.96% |
| (3) per-face, chunks of 16 | 3 µs / 15 KB / 93% | 3.9 µs / 17 KB / 99% | 13 µs / 34 KB / 99.9% | 98 µs / 215 KB / 99.9% |
| (3+4) per-face, trie | 4 µs / 16 KB / 93% | 5.4 µs / 21 KB / 99% | 5.9 µs / 22 KB / 99.9% | 7.7 µs / 23 KB / 99.99% |

k = n/100, patch:

| Layout | n=1,024, k=10 (28 faces) | n=10,000, k=100 (144) | n=102,400, k=1,024 (1,156) |
|---|---|---|---|
| (1) whole-table copy-on-write | 88 µs / 406 KB | 835 µs / 3.9 MB | 15.4 ms / 39 MB |
| (2) table, chunks of 64 | 26 µs / 97 KB | 148 µs / 493 KB | 1.31 ms / 3.5 MB |
| (3) per-face, chunks of 64 | 15 µs / 68 KB | 93 µs / 369 KB | 1.09 ms / 2.9 MB |
| (3+4) per-face, trie | 15 µs / 59 KB | 86 µs / 322 KB | 1.00 ms / 2.6 MB |

k = n/10 (`51-data/prototype-bench.tsv`). Every layout converges on the cost of the new geometry:
12-24 ms and 24-57 MB at n = 102,400.

Scattered edits (k faces spread over the body) are the worst case for chunks. At n = 102,400 and
k = 1,024:
- per-face chunks of 64: 10.3 ms / 17 MB;
- per-face trie: 8.6 ms / 12.9 MB;
- whole-table copy-on-write: 20 ms / 44 MB.

- **Whole-table copy-on-write is the same as a plain deep copy.** The edit writes every table, so
  the whole-table form copies them all: 14.5 ms against 15.3 ms for plain at n = 102,400 (MEASURED).
  Finer per-kind tables would not help, because every real edit writes faces, edges and vertices
  together.
- **Sharing geometry and cells costs nothing extra and is most of the win.** Even the whole-table
  form shares 83% of the old body's bytes, because the geometry is behind `Arc`. solidcore deep-copies
  its geometry and drops its caches.

### 4.2 Read cost (ns; best of 5)

Columns: random face lookup / full traversal (face, loop, uses, edges, vertices) per face /
adjacency walk per step / filled-cell read.

| Layout | n=1,024 | n=102,400 | n=102,400 after 10 scattered edits |
|---|---|---|---|
| plain `Vec` (baseline) | 1.18 / 4.9 / 8.2 / 2.2 | 2.74 / 22.5 / 8.2 / 7.2 | 2.76 / 14.2 / 8.2 / 7.0 |
| (1) whole-table copy-on-write | 1.18 / 4.8 / 8.5 / 2.2 | 2.97 / 18.8 / 8.5 / 7.0 | 2.97 / 10.3 / 8.5 / 7.2 |
| (2) table, chunks of 64 | 1.33 / 12.8 / 12.8 / 2.5 | 4.50 / 41.9 / 12.9 / 8.4 | 4.79 / 39.8 / 12.8 / 9.7 |
| (4) table, trie | 1.80 / 18.0 / 14.9 / 3.8 | 11.6 / 124 / 23.0 / 23.1 | 11.8 / 158 / 23.0 / 23.5 |
| (3) per-face, plain `Vec` of `Arc` | 1.20 / 4.9 / 7.5 / 2.2 | 4.00 / 24.8 / 7.5 / 5.2 | 3.91 / 28.9 / 7.4 / 5.0 |
| (3) per-face, chunks of 64 | 1.49 / 10.3 / 10.5 / 2.6 | 6.82 / 52.9 / 10.4 / 7.2 | 7.31 / 63.0 / 10.4 / 7.8 |
| (3) per-face, trie | 2.05 / 16.1 / 12.5 / 3.7 | 17.8 / 117 / 18.0 / 17.7 | 22.1 / 140 / 17.6 / 18.6 |

- **Chunks cost 1.1-2.6 times a plain vector on topology reads,** and a trie 1.5-5.5 times. Most of
  the cost is in the traversal, which makes one chunked lookup per edge and per vertex.
- **Per-face records in a plain vector read at plain speed.** The walk is faster, because a face's
  loops sit inside its record.
- **The traversal is pure pointer-chasing.** A kernel read does geometry work of tens to hundreds of
  ns beside each lookup, so the overhead in practice is smaller (INFERRED, not measured).

### 4.3 Memory when nothing is shared (MEASURED)

Bytes per face of a freshly built body:
- plain: 2,276;
- whole-table copy-on-write: 2,276;
- chunks of 64: 2,280;
- trie: 2,285;
- per-face, chunks of 64: 2,267;
- per-face, plain `Vec` of `Arc`: 2,312.

The overhead is under 2% in every layout. An `Arc` header per face record (16 B) is offset by loops
stored inline instead of in their own table.

### 4.4 Ids

- **The ids of every layout are dense indices.** A draft keeps every surviving entity's index, so an
  id is stable across edits by construction. The tests check that every layout gives the same body.
- **Deletion.** A deleted entity leaves a slot:
  - Without reuse, the slot is a tombstone. This is what solidcore does (READ: `Arena` holds
    `Vec<Option<T>>`), and its only cost is the dead slot.
  - With reuse, a generational id is needed so that an old id cannot name the new occupant.
    `src/ids.rs` implements one over any table, and the test `generational_ids_refuse_stale` shows a
    stale id refused.
- **Recommendation: no reuse within a body's lineage.** The fate map (section 6) says "same id, same
  face". Reuse would make every id comparison across edits need a generation check. Compaction
  (renumbering) belongs only to the stored form, which returns an old-to-new map (INFERRED).

## 5. Fill-once cells (prototype, MEASURED by test)

- **Carried, not copied.** In the per-face layout a face's cells live inside its `Arc<FaceRecord>`.
  The test `cells_carry_and_invalidate` checks the following on a 256-face body, with box and tree
  filled on every face:
  - an edit of one face touches 9 faces;
  - each of the other 247 is the same pointer in the old and new body;
  - its cells are already filled, and reading them through the new body recomputes nothing (the
    global compute counter does not move).
  - In the table layout the cells are an `Arc<FaceCells>` per face record, which behaves the same.
- **A cell filled through the new body should be visible through the old one, and it is.** The cell
  belongs to the face, and an untouched face is the same face in both bodies. Its value is a pure
  function of the face record and the edges and vertices that record reads, and all of these are
  identical in both bodies by the definition of untouched.
  - Hiding the fill from the old body would mean copying the cells on every edit, which defeats the
    purpose.
  - It would also give one face two values over time, which breaks the "definite value" rule of
    `04-decisions.md` §5.
  - The test fills a cell through a descendant and reads it through the ancestor.
- **Racing readers.** In `racing_readers_get_one_value`, eight threads fill the trees of 1,600 faces
  in different orders. Every reader sees the same value, and later reads return the same allocation
  (`Arc::ptr_eq`).
  - `OnceLock::get_or_init` blocks a second reader of the *same* cell while the first computes. No
    lock covers the body or another cell.
  - If even that per-cell wait is unwanted, use compute-then-`set`. The loser drops its value and
    reads the winner's. That is two computations under contention, the same result by determinism,
    and no waiting.
- **What "untouched" must mean.** A face is untouched when its own record is unchanged *and* every
  edge and vertex it reads is unchanged.
  - The structure knows this through the draft, not through versions or content hashes.
  - Every write to an edge or vertex goes through the `Draft`, which marks the faces on that edge.
    The prototype uses `Edge::faces`; a vertex write reaches the faces through the vertex's edges.
  - `finish` gives every marked face fresh cells. The same test changes one shared edge's tolerance
    and checks that exactly its two faces lose their cells.
  - **Content identity measured worse.** Validating a cell on every read by hashing what it read
    (surface pointer, trim pointers, edge tolerances) costs 28.8 ns against 2.2 ns for a plain cell
    read at n = 1,024, and 248 ns against 7.2 ns at n = 102,400. That is 13-34 times on every read,
    to save bookkeeping that is paid once per write.
  - Edge versions would work too, but need the same per-read check.

## 6. Draft and finish (prototype, MEASURED)

- **The working form.** `body.edit()` returns a `Draft` that shares every part with the body. Writes
  go through `Table::get_mut`, which copies only what it reaches: one table, chunk, trie path or face
  record.
- **Transient mode.** `body.into_draft()` takes the body by value. Where the draft holds the only
  reference, `Arc::make_mut` writes in place with no copy.
  - k = 1: 7.6 KB, the new geometry and nothing else, against 22-83 KB for a shared draft at
    n = 1,024-102,400.
  - k = 1,024 at n = 102,400: 2.1 MB transient against 2.9 MB shared.
  - The test is `transient_draft_writes_in_place`.
  - An operation that builds intermediate bodies it never publishes should use it.
- **The cost of `finish()`** here is only the cell reset and the fate map: 0.2 µs for k = 1 and
  43 µs for 1,156 touched faces (per-face layouts). The table layouts take 2-8 times longer, from one
  `Arc` allocation per reset cell.
  - The real `finish` also builds missing trims and settles tolerances. That cost is the operation's,
    not the structure's: research 47 puts trims at about 847 B and their fit per trim.
- **Fates: the minimum persistent naming needs.** Research 38: the app tags copies of operands and
  reads Unchanged, Modified, Split, Merged and Generated back from origins. Proposed result type:

```rust
pub struct Outcome {
    /// Sparse: an input face absent from the map is unchanged and keeps its id.
    pub fates: BTreeMap<FaceId, FaceFate>,
    /// Faces with no input face, and what they were made from (fillet faces from edges, caps from
    /// a tool's faces, and so on).
    pub created: Vec<(FaceId, Origin)>,
}
pub enum FaceFate {
    Modified { geometry: bool },  // same id; false = only its boundary changed
    SplitInto(Vec<FaceId>),       // first keeps the input id
    MergedInto(FaceId),
    Deleted,
}
pub enum Origin { FromEdge(EdgeId), FromVertex(VertexId), FromFace(OperandFace), Free }
```

- **Notes on the result type:**
  - Multi-operand operations need `OperandFace = (operand index, FaceId)`.
  - The draft records fates as it writes, so the report costs O(touched) and not O(n).
  - `geometry: false` is what the app needs to keep a name on a face whose edges were trimmed.
  - Edges need the same report for edge naming (fillets name edges). The same shape works
    (INFERRED; not prototyped).

## 7. Recommendation

**Representation.**
- **Per-face records:** `Arc<FaceRecord>` holding loops, edge uses, trims, surface and its cells.
  Edges and vertices sit in tables. Geometry is immutable and shared by `Arc`.
- **All of it behind one `Table` trait** with dense, never-reused ids.
- **Ship the trait first with whole-table copy-on-write.** Its reads equal a plain `Vec`. Keep
  chunks of 64 as the drop-in for large bodies: it is O(k + n/64) per edit, at a read cost of
  1.1-2.3 times.
- **Do not use the trie.** It is the cheapest edit at 100,000 faces, but reads cost 1.5-5.5 times,
  and the chunks are within a factor of 2-4 at that size.
- **Invalidation goes through the draft** (section 5), not through content identity.

**The numbers that decide it.**
1. **App bodies are small:** a p99 of 94 faces and a maximum of 123. Whole-table copy-on-write of
   such a body costs a few µs, against operations of 5 ms to 3 s. Option (1) is enough for the app as
   measured, provided geometry and cells are per-face `Arc`s.
2. **Face caches are 5-160 times the body** (section 1). Today every copy drops them, and that, not
   the copy, is the cost to remove. Every option above removes it.
3. **Chunking wins only from about 10,000 faces:** 21 µs against 789 µs per edit, and 69 KB against
   3.6 MB. At 102,400 faces it is 51 µs against 14.5 ms, and 125 KB against 37 MB.
4. **Reads:** chunks cost 1.1-2.6 times on pure topology walks; per-face records in a plain vector
   cost 1.0-1.1 times.

**Open risks.**
- **Large STEP parts were not measured**, because the NIST fetch needs `unzip`. If imports of 10,000
  faces or more are edited repeatedly, chunks are needed, and the read overhead must then be measured
  on real kernel reads, not a microbenchmark.
- **solidcore operations rewrite faces they need not.** The fillet re-parameterised 63 untouched
  faces, and shell renumbers everything. A ported operation inherits that, and sharing then buys
  nothing for it. Each ported operation needs an "untouched fraction" check like section 3's.
- **Transforms.** At 0% kept, a placement on the body or the shell (applied on read) would make them
  free. It would also put a matrix on every read path, which is a design decision, not a detail.
- **Use records.** The radial-edge use records are 46-51% of today's body. The new record layout
  (half-edges, no inline gap caches) decides more of the memory than the container does.
- **Concurrent fills.** Fills from several readers are safe, but they write through `&` into shared
  cells. Any cell whose value depends on the order of fills breaks determinism. Cells must be pure
  functions of the face and its edges and vertices.

**What must be decided before real code is written on it.**
1. **The entity set and record layout:** half-edge or loop-edge lists, and where gap and tolerance
   data live (a cell, or a field that is part of "untouched").
2. **Exactly which edge and vertex fields a face's cells may read.** That set defines invalidation.
3. **The id policy:** dense, never reused, and compaction only in the stored form with an old-to-new
   map.
4. **Whether bodies carry a placement.**
5. **The `Outcome` type,** for faces and edges, agreed with the app's naming code (research 38).
6. **Per-cell waiting** (`get_or_init`) **or racing** (`set`).

## 8. How to rerun

- **Prototype** (std only; about 2 minutes on server2):

      rsync -a --exclude target prototypes/body-structure/ server2:sc-trash/scx-body/body-structure/
      SC_BG=1 coord/sc-slot.sh scx-body nice -n 10 cargo test --release \
          --manifest-path sc-trash/scx-body/body-structure/Cargo.toml --target-dir sc-trash/scx-body/target
      SC_BG=1 coord/sc-slot.sh scx-body nice -n 10 cargo run --release --bin body-bench \
          --manifest-path ... --target-dir ... -- all > prototype-bench.tsv

  The output is `51-data/prototype-bench.tsv`, with a header per section. Its `shared_frac` column
  is 1 - (bytes retained by the new body) / (bytes of the old body).
- **solidcore probe:**
  - Take a scratch worktree of solidcore main (e3b1b2dbb6) with `upstream` linked.
  - In `crates/solidcore-step/Cargo.toml`, change `unsafe_code` from forbid to deny, for the counting
    allocator.
  - Place the probe at `crates/solidcore-step/tests/scx_body_probe.rs`. It is kept only on server2
    and was never committed (the licence position forbids copying solidcore-adjacent code into this
    repository).
  - Run it with `SCX_OUT=dir SCX_STEP=a.step,b.step`, plus optionally `SCX_ONLY=sizes,copies,untouched`.
  - The output is `51-data/solidcore-probe.tsv` (sizes and copies, from the first run) and
    `51-data/solidcore-untouched.tsv` (from a second run, after the face-edge lookup was fixed to go
    through edge uses).
  - What the probe does is described in sections 1-3: each arena is cloned and counted, face caches
    are built on a private copy, entities are fingerprinted, and the prints are matched as multisets.
  - 30 s run time.
