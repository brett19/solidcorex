# 11. Stage C: reads of a body between operations

2026-11-22, on `origin/main` 782ef9afc. Read-only study following `briefs/stage-c-study.md` and the
method and vocabulary of report 07. Kernel paths are relative to `solidcore/crates/solidcore/src`.
`OPS/` = `solidcore/crates/solidcore-ops/src`. App paths are `crates/...`, read at HEAD.
Marks: [V] I read it in code, [A] taken from an earlier report (02) without re-reading, [I] inference.

## 0. Findings that change the picture

1. **Most app-facing reads go through a door that edits the body, not `measure_body`.** [V]
   - `OPS/session.rs:699-725` `with_body` runs `b.with_rollback(|br| f(k, br), keep)`. A successful
     call KEEPS everything the read wrote (trims, caches, memos) on the live body.
   - It derefs the body guard mutably. `OPS/body.rs:143-148` `DerefMut` sets `outward = None` and
     `primitive = None`. So **every read through `with_body` throws away the body's validity proof
     and its template.** The next `Contract::Proved` operation has to prove the body again.
   - `with_rollback` takes a snapshot first (`topology.rs:1343-1395`, `1413-1438`). That is a deep
     copy of every surface, curve and trim, made even for a pure read.
   - The readers in `OPS/measure.rs` that use it: mass_properties :491, area :529, bounds :548,
     classify_point :600, classify_points :617, is_manifold_solid :630, solid_count :655,
     edge_closest_point :673, the ray queries :700/:728/:953, face_distances :1006/:1086,
     rim_edges :1104, outline_cuts :1154, vertex_distance :1226, face normals :1247/:1277/:1315,
     face_area :1330, point_on_face :1360, face_centroid :1381, face_plane_normal :1409,
     face_tolerance :1471, edge_length :1486, edge_middle :1504.
   - `with_body` appears 97 times in `OPS/`, all calls counted, not only reads.
2. **`classify_point` copies the body twice.** [V]
   - The first copy is `with_body`'s snapshot.
   - The second is inside the kernel. `locate_points` (`OPS/measure.rs:190-201`) builds a
     `BrepClassifier` (`query/brep.rs:1150-1220`). Its `Target::mutable()` makes a standing copy
     the first time it writes. That happens on any body with faces once the boundary solve runs:
     the read-only `BrepObjects` solver is used only for a target with no faces (`query/brep.rs:843ff`).
   - So the kernel signature `locate_points(&Kernel, &Brep, ..)` is already `&Brep` in name only.
3. **The app's own wrapper is a third door.** [V] `crates/smlib/src/core.rs:310` `with_body` is
   `call_on(.., |k| f(k, &mut body.kernel()))`. It takes no snapshot, so it does not copy, but it
   keeps all writes. My inference [I] is that it also drops the proof through the same `DerefMut`.
   - smlib's mass_properties, area, bounds, classify_point(s), is_manifold_solid, solid_count and
     check (`crates/smlib/src/core/measure.rs:97-373`) all go through it.
4. **The body-wide "create every missing trim" step already exists as one function**, in open PR
   #20943 (ours, A8): `topology::brep::create_missing_uv_trim_curves`. [V from the diff]
   - It walks the edge uses in the order `edgeuses(b, None)`.
   - It is the trim half of the prepare step for every read family below.
5. **Mass properties flip mode flags on SHARED geometry.** [V] `Unclamped`
   (`mass/integrator/properties.rs:447-512`) sets and restores `set_out_of_bounds_enabled_in_place`
   and `set_continues_past_domain_in_place`. These are `AtomicBool` stores with `Relaxed` ordering
   on the B-spline base (`nurbs/b_spline_surface/p1.rs:997,1010`).
   - The base is shared by every copy of the surface: snapshots, standing copies, `Arc`.
   - Two readers of one surface would undo each other's flag, so this is a real data race on
     answers [I]. It is class S, but it is the one S item that must become a solve argument
     before any shared read can run.
6. **Face point classification writes through `&Brep`, even on its "prepared" path.** [V]
   - `point_classify_with_topology` (`query/face/p2.rs:1061`, slot code about :1127-1137) takes the
     surface's trim-cache slot (`take_trim_cache`) and stores it back.
   - On Prepared or Requested, a Stored cache with no tree is an `Unported` error
     (`classify_by_trim_cache`, about :1027), not a fall-back. So `point_classify_prepared` is
     usable only where the slot is Empty or holds a cache with a tree.

## 1. Phases, per read family

Two layers apply. The door is chosen by the ops or app wrapper. Below it sits the kernel read.

| # | Phase | Where | Read-like? |
|---|---|---|---|
| D1 | lock the body | `OPS/body.rs` Mutex; `read_held` `OPS/session.rs:745` | n/a |
| D2a | `with_body`: snapshot, run, keep or roll back, drop the proof | `OPS/session.rs:699-725`, `topology.rs:1343,1413` | **edit** (whole-body copy, then the writes are kept) |
| D2b | `measure_body`: `brep_copy`, unlock, run on the copy | `OPS/session.rs:766-781` | read (copies, writes thrown away) |
| D2c | `read_body`: `&Brep` under the lock | `OPS/session.rs:732-745` | read, no copy |
| D2d | smlib `with_body`: `&mut` guard, no snapshot | `crates/smlib/src/core.rs:310` | **edit** (writes kept) |

| Family | Kernel phases (boundaries) | Read-like? |
|---|---|---|
| Loose box | `query/brep.rs:175` `calculate_bounding_box`: the locked box, or vertices, edge boxes and face boxes (`query/face/p1.rs:579-640`) | read [V]. Only writes: NURB `OnceLock` memos |
| Tight box | `query/brep.rs:255`, then per face `qf3::calculate_tight_bounding_box_into`, which calls `point_classify(k, lb.get_mut()?)` (`query/face/p3.rs:851`) | read per face. Writes trims and the slot, or copies through `LazyBrep` [V] |
| Mass / volume / area | `brep_properties` :1977 (tight box), then `properties_in_box` :2049-2232. Per face, per use (up, mate): `face_properties_in` :622-671 (trims, `face_box`, `Unclamped`, `boundary_properties` :1053), then for a separating face `gap_closure` :1643, then wire edges, then shell vertices. Then `add`/`add_selected` :1413/:1429 | read over independent faces once trims exist [V] |
| Face area / centroid | `face_area` :2252, `face_centroid` :2271 (`&mut`) | same as mass, one face |
| Point classification (body) | `locate_points`: zone tol, `BrepClassifier::of_brep`, per point `point_3d_classify` (`query/brep.rs:843`): boundary solve (`solve_mutable_brep_point_kept`, `ray/live.rs:78-99`), then region rays along ±x, ±y, ±z | read per point. Writes face caches and trims through the copy [V] |
| Ray fire / face hit | `BrepQuery::first_face_hit` (`query/brep.rs:~764`, `&mut Brep`), Rayfire/All | same as the boundary solve |
| Face point classify / face queries | `query/face/p2.rs:~819` (Mutable), `:850` `point_classify_prepared` (`&Brep`) | read once the trims exist and the slot is fixed |
| Topology-only (manifold, census, counts, pick lists) | `brep_is_manifold_solid(b)`, `brep_material_census(b)` | pure read already [V] |
| Edge length | `properties.rs:2236`, takes `&Brep` | pure read [V] |
| Closest point / distances / normals | `q::edge_closest_point`, `api_queries.rs:~1274` and others | not read in detail [A]: same leaves as face classification |
| Validity check | `check_brep(_scoped)` (`OPS/check.rs:117`, smlib `measure.rs:357`) | writes gap memos, trees, a standing copy [A] (report 02) |
| Blend queries under `measure_body` | edges_share_wedge `OPS/blend.rs:2029` (`wedge_at_middle`), edge_faces_along :2421, carried_faces :2549, tangent_chain :2625, band_room `OPS/blend/room.rs:121` | callers [V], their kernel writes [A]/UNVERIFIED |

## 2. Writes inside the read-like phases

| # | Write | file:line | Fires | Class |
|---|---|---|---|---|
| W1 | UV trim creation (edge-use routine, which may raise the edge tolerance, #18162) | mass: `boundary`/`oriented_boundary` `properties.rs:272/278`, then `topology/face/p1.rs:731` `create_oriented_trim_boundary` (`get_or_create_uv_trim_curve` per use), `gap_closure` :1665; tight box and face classification via Mutable `ensure_trim_curve` (`topology/loopuse.rs:134-197`) | **commonly**: 5 to 6 per volume or area on simple bodies (plan-v4 §1); never once all trims exist | **P**. Prepare with `create_missing_uv_trim_curves` (#20943). Left lazy, it is the one **C** item (W1c below) |
| W1f | Trims for a face with NO UV curve at all, made by the face routine `create_uv_trim_curves` (corner fix) | `topology/face/p1.rs:731` | rare [I]: stitched or imported faces | **P**, but the face routine is not the edge-use routine: preparing with #20943 instead gives different trims on such faces (a result change). A prepare that calls the face routine for these faces keeps today's result |
| W2 | Surface trim-cache slot take and store back through `&Brep` (`Arc<Mutex>`) | `query/face/p2.rs:~1127-1137`; also `intersect/point_class.rs:2477-2483` [A] | on every face classification of a non-plane, non-cylinder face [I] | borrowing it: **S** (lend, don't take). Filling it: **P** (build a tree, or leave Empty) |
| W3 | `Unclamped` flag set and restore on the shared B-spline base | `properties.rs:447-512`; `nurbs/b_spline_surface/p1.rs:997,1010` | every B-spline face in mass [V] | **S**. Must become an argument (finding 5) |
| W4 | NURB form memos (`OnceLock`) | `degree_with_nurb` paths | commonly | **P** (idempotent and `Sync`; harmless even when left lazy) |
| W5 | `BrepQuery` `OnceCell`s: `BrepCache` and box | `query/brep.rs:589-595, ~673, ~696` | once per query | **P** (build eagerly) |
| W6 | `KeptFaceCaches` per face reached (`Cell`, keyed by `FaceCacheKey`) | `ray/live.rs:113-185`, `:78-99` | per classification or ray, on each face the solve reaches | **R** (which faces is unknown in advance), or **P** at the price of building every face |
| W7 | `LazyBrep`/`Target` standing copy on first write | `query/brep.rs:1062-1143` | every classification with faces [V] | **S**. Goes away once W1, W2 and W6 are prepared |
| W8 | Validity check: gap memos (up to 153 fields) and trees | report 02, plan-v4 §1 [A] | every full check | **P** (per entity, before the check; B4 for trees) |
| W9 | Validity check: `validate_pointers` standing copy | [A] | every check | **S** |
| W10 | Kernel counters, check's random draws | kernel | always | **S** (per worker; draws after A1) |
| W11 | Door writes: snapshot, trims kept, proof and template dropped | `OPS/session.rs:699-725`, `OPS/body.rs:143-148` | every `with_body` read [V] | **S** for a read: an artefact of the door, gone with a read door |

**W1c (C, reformulable).** [I from the code shape]
- In today's lazy order, face i's mass builds the trims of its edge uses. Attaching a trim can
  raise the shared edge's tolerance (#18162).
- Face j > i then reads that edge, whose tolerance and mate trim feed j's own fit and boundary.
  So face j's value depends on face i having come first.
- A deterministic reformulation exists: build every missing trim before the loop, in the fixed
  edge-use order of `create_missing_uv_trim_curves`. The coupling then disappears and the item
  becomes P.
- Bodies that already have every trim give bit-identical results. Bodies missing some trims can
  change where a tolerance raise fired. On the bodies measured in plan-v4 no tolerance moved.

**Counts: P 6 (W1, W1f, W2-fill, W4, W5, W8). R 1 (W6). C 1 (W1c, gone under an edge-ordered
prepare). S 6 (W2-borrow, W3, W7, W9, W10, W11).**

## 3. What "prepared" means for body reads

Below, "every edge use on a face" excludes wire edge uses.

| Family | Must exist before the read | Needs none of it |
|---|---|---|
| Loose box, topology queries, census, counts, edge length, pick lists | nothing | **yes** |
| Mass / volume / area / face area / centroid | a trim on every edge use on a face; `Unclamped` passed as an argument | |
| Tight box | everything mass needs, plus a slot that is Empty or holds a tree (it classifies face points) | |
| Face point classification, closest point, normals | trims on the visited uses; slot Empty or holding a tree | |
| Body classification, ray fire | a `BrepCache` and box, plus face caches with trees for the faces reached (R), plus trims | |
| Validity check | gap memos, trees and trims, entity by entity | (its own study) |

Which bodies have this today. [V] for the mechanism, [I] for how often.
- **Bodies that have passed through `to_record`** (display, save) **have a trim on every edge use
  on a face.** `Body::to_record` (`OPS/body.rs:~490`) reaches `with_record`, which reaches
  `with_body`, which reaches `to_record` (`solidcore-convert/src/brep/record.rs:130-136`). That
  runs the edge-use routine for every use missing its own trim, and `with_body` keeps the result.
- A fresh operation result that is measured before it is recorded has only the trims its
  operation left.
- No body has face caches kept between calls: `BrepQuery` is per call.
- Cost of preparing: trims are what the read builds anyway (5 to 6 on simple bodies), so it is
  not extra work. Face caches only for curved faces that are reached (planes and cylinders use none).

## 4. Caches, and whether the keyless phase table holds

- Trims live on the body and are kept after the first build.
- Face caches (`SurfaceCache` with tree) are built per `BrepQuery`, that is per call, kept in
  `KeptFaceCaches` while the call lasts, then dropped. A repeated `classify_point` rebuilds them
  every time. [V for the shape; the counts are UNVERIFIED (U5)]
- The `BrepCache` and box: once per query.
- **The keyless phase-scoped table holds, and fits better here than in the Boolean.** Inside a read
  nothing that a cache is built from changes:
  - trims are prepared;
  - no tolerance moves;
  - the solve key (`FaceCacheKey`: tolerances, context bits, editing flags) is constant for the call.
  "Built in this phase, valid while `&Brep` is borrowed" is therefore exact.
- **A further option between operations:** keep the table next to `Held` and drop it in the same
  `DerefMut` that drops the `Outward` proof (`OPS/body.rs:143-148`). That is already the body's
  rule for "anything derived from me is stale". The table then lives across reads with no key,
  and repeated classification stops rebuilding trees.
  - This needs W2 (the slot) gone, and the slot's identity rules (empty slots on snapshot, the
    same slot on standing copies) not to be relied on [A].

## 5. Items and how results combine

- **Mass:** the items are faces. Typical counts run from tens to a few thousand. Planes are cheap
  and B-spline faces with fine boundaries are costly, so the cost skew is large.
  - Each face gives a plain value: the delta, from 1 to 8 moment vectors (`add` :1413), plus a
    closure term.
  - The sum is order-dependent floating point. The order is fixed by:
    1. the face list order;
    2. per face, the uses up then mate;
    3. per use, its delta, then the face's closure (built once with `gap_closure`, its sign flipped
       per orientation);
    4. a non-separating face stops after its first use;
    5. then wire edges in edge order;
    6. then shell vertices.
  - `brep_properties_in_box_terms` (:2012) already records the summands. Its doc states that the
    total is bit for bit the same.
  - Summing a per-face list in that exact order reproduces the bits, because `0 + delta == delta`.
    The only difference is the sign of a zero total. [V for the order; the bit claim is the doc's
    plus my reading]
  - This holds only if each face's value does not depend on an earlier face's trim writes, which
    is W1c. So: prepare first, then the items are independent and the in-order sum is exact.
  - Early exit: `check_cancelled` only.
- **Classification:** the items are points (batches in `classify_points`). Each gives a class.
  There is no sum, only the order of requests (W6).
- **Tight box:** the items are faces, combined by min and max, which does not depend on order.
- **Rays:** the items are rays. Nearest-hit selection is a min with a tie rule (UNVERIFIED tie
  order).

## 6. What stays sequential regardless

- Trim creation, including any tolerance raise it makes.
- The fulfil step for face-cache requests.
- The final in-order sum.
- Taking and releasing the body lock.
- Gap-memo fills for the check.
- Anything a `with_body` edit does: `to_record`, a real edit.

## 7. Cost

PRs, in order. "rc" marks a result change.
1. **Unclamped as an argument** to the boundary integrator (`properties.rs:447-512`). This removes
   the only write to shared geometry. M, no rc.
2. **A read door in ops**: the readers listed in finding 1 move from `with_body` to `read_body`
   with a kernel, or to `measure_body` for those that still write.
   - Those that need only `&Brep` already move at once: bounds, is_manifold_solid, solid_count,
     edge_length.
   - This stops the proof from being dropped by reads.
   - S to M. No rc in answers, but later reads re-create trims that are no longer kept (U2).
3. **Prepare step**: `create_missing_uv_trim_curves` (#20943), extended to the face routine for
   faces with no UV curves (W1f). Callers run it once per body in a short edit before reading.
   S, **rc** on bodies missing trims where a raise fires (W1c) or where trimless faces exist.
4. **Mass, area, volume and the face measures on `&Brep`**: `face_properties_in` and
   `properties_in_box` read prepared trims through `QueryTopology::Prepared`, and `gap_closure`
   stops creating trims. M, no rc after PR 3.
5. **Slot borrow** instead of take and put in `point_classify_with_topology`. A Stored cache with
   no tree falls back instead of returning `Unported`. Builds on B3. M, no rc.
6. **Tight box over `point_classify_prepared`.** S, no rc.
7. **`BrepQuery` split** (question C). M to L, no rc.
8. **Classification and rays over the phase table with a request loop.** This removes the
   `BrepClassifier` copy. M to L. rc only if the order in which faces are fulfilled changes which
   cache answers (UNVERIFIED; the Boolean saw no such change in 45 comparisons).
9. **`measure_body` callers**: volume (`OPS/body.rs:412`) reads prepared, falling back to a copy
   when trims are missing. S.
10. **Body lock** (question D). M.
11. **Blend query callers** (5). 1 to 3 PRs, unknown until U6.
12. **Validity check** (B4 plus gap prepare). L, and it should be its own study.

**Total: 10 to 14 PRs, 1 with a result change (PR 3), plus possibly PR 8.**
- Shared with report 07's five parts:
  - part 1, the prepared view: PRs 4 and 6;
  - part 2, the phase table: PRs 7 and 8;
  - part 3, typed requests: `NeedFace` only. There are no raises and no repairs;
  - part 4, the loop: PR 8;
  - part 5, a kernel per worker: the pools already give one per call.
- About half of the work is shared. Specific to reads: the doors (2, 9, 10), Unclamped (1),
  the slot (5).

## 8. UNVERIFIED, each with what settles it

| # | Unknown | Count or run that settles it |
|---|---|---|
| U1 | How often bodies lack trims when they are measured (fresh results vs recorded) | At entry to every `with_body`/`measure_body` read: count edge uses on a face without `uv_trim_curve`. Corpus plus an app session |
| U2 | Whether stopping `with_body` from keeping read writes changes any later answer | Battery of reads in random order, keep vs discard; count differing answers (plan-v4 saw none on 3 bodies) |
| U3 | Faces with no UV curve at all at mass entry (W1f) | Count them per body over the corpus |
| U4 | That mass on a fully trimmed body writes nothing besides W3 and W4 | Count trim creations, tolerance changes and slot stores inside `properties_in_box` after the prepare (expect 0) |
| U5 | Face-cache rebuilds per repeated classification, and how often a Stored slot has no tree | B1 counters around `classify_points` called twice; count `Unported` from the prepared path |
| U6 | Kernel writes in `wedge_at_middle`, `faces_along_indices`, `carried_face_indices`, `edge_walk`, `kernel_band_room` | Count `&mut Brep` writes per call (trims, memos) on the corpus |
| U7 | Cost of the `with_body` snapshot per read vs the read itself | Time and bytes of `snapshot` per call in a measure battery |
| U8 | Whether smlib's `body.kernel()` guard drops the proof as the ops guard does | Read `Body::kernel` (one read); I infer yes |
| U9 | Tie order in nearest ray hit, and whether W6 fulfil order can change a classification | Compare answers with requests fulfilled in forward vs reverse face order |
| U10 | The closest-point and normal family's exact leaves (`api_queries.rs:~1274`, `face_classify_uv` :933) | Line reads; I expect the same leaves as face classification |

## Answers to A to F

**A. Can each family run on `&Brep` if prepared, and what does prepared contain?**
- **Loose box, topology, census, counts, edge length, pick lists:** they need nothing. Most are
  `&Brep` already.
- **Mass, volume and area:** trims on every edge use on a face, and Unclamped passed as an
  argument.
- **Tight box and face queries:** the mass requirements, plus a slot that is Empty or holds a tree,
  borrowed rather than taken.
- **Classification and rays:** the above, plus a `BrepCache`, the box and face caches. The face
  caches come per request, or all at once.
- **Check:** gap memos and trees in addition.

**B. Do recorded bodies count as prepared, and is `measure_body`'s copy avoidable now?**
- For trims, yes. `to_record` leaves every edge use on a face with a trim, and `with_body` keeps
  them.
- The copy is still not avoidable today, for three reasons:
  - `Unclamped` writes shared flags;
  - the tight box takes and puts the slot;
  - any later `with_body` read or edit can leave new trim-less faces.
- After PRs 1 and 5, `Body::volume` (`OPS/body.rs:412`) can drop its copy on bodies whose trims
  are all present, with a cheap walk to check that and a fallback.
- `Body::check` (`OPS/check.rs:117`) and `check_solid` (`OPS/measure.rs:642`) would still write
  (W8, W9).
- The five blend callers are unknown (U6).

**C. What shareable form should `BrepQuery` take?**
- Neither purely eager nor purely lazy. Split it into an immutable
  `PreparedBrepData { identity, BrepCache, bbox, faces: table FaceId -> Result<Arc<SurfaceCache>> }`,
  which is `Sync`, and a per-reader cursor holding scratch, counters and the pending-request list.
- The table is filled before the read and by fulfilling requests. It is not filled from a `Cell`
  during the read.
- Eager-only means building every face's tree, which costs too much on a body with many curved
  faces when a query reaches only a few. So use requests (`RayPreparation`'s
  `PreparationRequired` already is this, `ray.rs:51-121`). Use eager only for small bodies.

**D. Options for the ops Mutex.** (Analysis only.)
- **RwLock:** readers share `&Brep`. It needs every reader on a read path (PR 2) and W2 and W3
  removed. Rollback is unchanged, since edits take the write lock. `Outward` survives reads,
  because only `DerefMut` drops it. Reads that still write cannot take the read lock: they keep
  `measure_body`'s copy, or prepare under the write lock first and then downgrade.
- **Immutable snapshot (`Arc<Brep>`, replaced after each edit):** readers clone the `Arc` and run
  without holding any lock. An edit works on its own copy and publishes it, so a failed edit
  simply drops its copy, and the snapshot in `with_rollback` becomes this copy. The proof and the
  phase table attach to the `Arc` and die with it. The shared surface base (W3) and the slot (W2)
  are reachable through the `Arc` and must be fixed first. Of the three, this fits a read-heavy
  app best.
- **Copy on write per arena (`Shared<Arena>`):** only the edge and edgeuse arenas are candidates
  [A]. The original-pays hazard (report 02, Test #61) applies. It is the most work and gains little
  over the `Arc` swap for reads.

**E. Mass properties: what fixes the summation order, and does an in-order sum reproduce the bits?**
- The order is fixed by the face list, the uses up then mate, the delta before the closure, then
  wire edges, then shell vertices (§5).
- A per-face list summed in that order reproduces the bits. The exception is the sign of a zero
  total. The kernel's own `_terms` doc claims the same.
- This holds provided the trims are prepared first, so that no face's value depends on an earlier
  face's writes (W1c).

**F. Which families stand closest to a shared `&Brep`, with their blockers?**

| Rank | Family | Blockers |
|---|---|---|
| 1 | Topology, census, counts, edge length, loose box | only the door (`with_body` to `read_body`) |
| 2 | Mass, volume, area, face measures | the door, W3 (Unclamped), the trim prepare (rc on W1c and W1f), `&mut` signatures |
| 3 | Tight box and face point queries (normals, point on face, closest) | as 2, plus W2 (the slot borrow) |
| 4 | Body classification and rays | as 3, plus the `BrepQuery` split, W6 requests, removing the `BrepClassifier` copy |
| 5 | Blend queries | unknown writes (U6) |
| 6 | Validity check | gap memos, trees, standing copy, random draws (A1) |

**Verdict:** this is much easier than the Boolean. It has one coupled write, which disappears under
an edge-ordered prepare, no raises or repairs, and an order-exact sum that the kernel already
records. Most of the cost sits in the ops doors and two shared-state writes (Unclamped and the
slot), not in the kernel's algorithms.
