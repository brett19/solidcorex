# 42: reads on the edit door

Date: 2026-11-25. Base: origin/main `b57f0e0323`. Nothing pushed to GitHub or the fork, nothing filed.
Scratch branches (local and on the build server side refs only): `cache-reads23-probe` (the compile walk,
commits "probe r0" to "probe r12") and `cache-reads23-measure` (the measuring probe). Server worktree:
a local worktree, trimmed with `sc-trim-bin.sh`.
Raw data: `scratch42/measure3.log`, probe source `scratch42/zz_reads_probe.rs`, walk tools
`scratch42/{flip.py,round.sh,graph.py}`.

Labels: MEASURED (the probe), READ (code read on main), INFERRED (from the two together).

## Result in one line

No entry point is READY (class i). Nothing was built on `cache-reads23`, so there is no PR file and no
counts table: nothing changed, so no counter could rise. Every read entry point reaches a T or C
leaf when compiled against `&Brep`. On bodies that have all their trims, which is every body
the operations made in the probe, no T leaf fires (MEASURED). The blocker left in practice is C: the
trimmed surface cache and its tree, which are filled into the surface's slot.

## 1. Method

- Compile walk (READ). In the probe branch the 32 entry points went from `with_body`/`measure_body`
  to `read_body`. About 110 kernel functions were flipped to `&Brep` until `solidcore-ops --lib`
  compiled. Each spot where a `&mut` was really needed was cut with a never-run stub,
  `zz_leaf(b, "<class>:<what>")`. `graph.py` then lists the stub labels each entry point reaches. It
  merges functions that share a name, so it may over-report. Nothing it reports is missed.
- Measurement (MEASURED). A temporary test, `solidcore-ops/src/zz_reads_probe.rs`, ran on origin/main.
  - Each entry point is called once on a newly made body, so no earlier call has filled a cache.
  - A temporary switch makes `measure_body` run on the body itself instead of a copy. Without it,
    the copy hides what the kernel writes.
  - Before and after each call it takes per-class digests of the body: trims (and their count),
    face, edge and vertex tolerances, edge-use gap caches, vertex-use gaps, vertex-use UV, face
    domains, edge and vertex geometry, surface Debug (geometry plus cache slots), and the whole
    Debug. Write stamps (`Brep::write_stamps`, made `pub` in the scratch branch) are compared
    separately.
  - Bodies:
    - box, cylinder, sphere, torus
    - box minus cylinder, box union sphere (Booleans with curved faces)
    - a box with three edges rounded (fillet)
    - spline corpus files `fillets/15858-round-spline-loft-larger-end-rim:b2` and
      `booleans/3830-split-cup-by-extended-spline-surface:b21`
  - Two states for each body:
    - fresh, as the operation left it
    - stripped: every UV trim deleted, standing in for a body whose trims are still to be made

  On all 9 bodies, `create_missing_uv_trim_curves` made nothing: every mated pair already had its
  trim (24/48, 6/12, ... edge uses) and no stamp moved (MEASURED, run 2). So "fresh" is the same
  state as "after `create_missing_uv_trim_curves`", and the stripped state is the one that exercises
  the T leaves.

## 2. Per entry point

Leaves (READ, compile walk) with file:line on main. The fresh and stripped columns give the body
classes that changed, across the 9 bodies (MEASURED). "stamps" means only the write stamps moved, with
no value changed. Tolerances (`T.tols`), domains, geometry, vertex-use UV and the editing flags never
changed in any call (MEASURED).

| Entry point (door) | Leaves reached by `&Brep` compile | Fresh body | Stripped body | Verdict |
|---|---|---|---|---|
| tangent_chain (with and without order) | binormal T/C | nothing | nothing | iv (T in type, dormant in practice) |
| tangent_face_chain_ordered | binormal T/C | nothing | nothing | iv (as above) |
| carried_faces (measure_body) | binormal T/C | nothing | T (5/9) | iv |
| face_plane_normal | binormal T/C | nothing | T (4/9) | iv |
| to_record / with_record | T create_missing_uv_trim_curves (convert record.rs:131) | nothing | T all | iv |
| volume / brep_volume (measure_body) | rep.15 E1 mass trims | nothing | T all | iv |
| face_hit_by_ray, fire_ray | binormal + LiveBrepObjects | stamps (5/9) | T | iv |
| face_hits_by_ray | binormal + Live + G calc_vertex_face_gap + BrepFaceServices | stamps (9/9) | T | iv |
| face_distances, face_distances_within | LiveBrepObjects | stamps (8/9) | T | iv |
| edges_lie_on_face | BrepFaceServices + FaceClassify + T create_uv_trim_curves | stamps (9/9) | T | iv |
| edge_faces_along, edges_share_wedge (measure_body) | binormal T/C | C (3/9) | T, C | iii+iv |
| face_normal_at_edge | binormal T/C | C (3/18) | T, C | iii+iv |
| continuity_report | binormal + Live | C (sphere, torus) | C | iii |
| face_line_hits, face_line_nearest | C FaceLineCache + T QueryTopology::Mutable + T UvLineObjects | C (8/18) | T, C | iii+iv |
| face_normal_at_point, point_on_face | C surface cache + T QueryTopology::Mutable | C (18/18) | T, C | iii+iv |
| classify_points_with_normals | as above + classify Target | C (3/9) | T, C | iii+iv |
| face_section | BrepFaceServices + T normalized_evaluate | C (8/9) | T, C | iii+iv |
| check (measure_body) | rep.15 E2: F set_validate_only, G gaps, fix_reversed | G eu+vu gaps (9/9), C 2/9 | C | ii+iii+G |
| check_solid (measure_body) | rep.15 E2 + volume | G gaps (8/9) | T all | ii+iii+iv+G |
| beads (measure_body) | binormal T/C (on_body) | not run | not run | iv (READ) |
| inside_points | none by compile, but `&Brep` takes the snapshot `curve_on_classify` path (query/face/p2.rs:2172-2220, adds a body copy per UV line) | not run | not run | iv: the count would rise |
| first_side (surface_cvs) | G compute_uv_point → `global_point_solve` → face_surface_cache (vertexuse.rs:1033) | not run | not run | iii |
| project_curves, principal_curvatures, closest_to | LiveBrepObjects | not run | not run | iv/iii (READ) |

W (a real write of meaning) was not seen in any read entry point.

## 3. The leaves (READ)

All are on main `b57f0e0323`.

- **L1. Binormal trim, T + C.** `topology/edgeuse/p3.rs`:
  - :3088 `get_or_create_uv_trim_curve`, guarded by `!has_trim`
  - :3172 `face_surface_cache` → `note_face_surface_cache` (p3.rs:335 → query/face/p2.rs:662)
  - :3223 `create_uv_trim_curve`, only when there is no trim and no surface foot

  12 entry points reach it. The T part fires only when the trim is missing (MEASURED: never on fresh
  bodies).
- **L2. Trimmed surface cache and tree, C.**
  - `query/face/p2.rs:433` `face_surface_cache` keeps the cache in the surface's `CacheSlot`
    (`surface/surface/p2.rs:509`, already `Arc<Mutex<..>>`).
  - `:673` `face_cache_tree` → `build_tree_with_face(&mut BrepCacheFace{b,f})` at :685. The build
    needs `&mut Brep` because it may create the face's trims and it toggles editing.

  Reached by L1, by LiveBrepObjects/BrepFaceServices (`query/brep/ray.rs:173,198`), by FaceLineCache
  (`topology/face/p4.rs` ~704) and by `global_point_solve` (vertexuse.rs:1033). It is the only class
  that changes value on fresh bodies (11 entry points, MEASURED).
- **L3. Point classify on the mutable path, T.**
  - `query/face/p2.rs:841` → `QueryTopology::Mutable` → `ensure_trim_curve` (`topology/loopuse.rs:159`)
  - `:1095` builds the tree

  5 entry points reach it. The read-only arm already exists: `point_classify_prepared` (:862) uses
  `QueryTopology::Prepared`. It answers from existing trims and an existing tree, and refuses
  (Unported) when one is missing.
- **L4. LiveBrepObjects / BrepFaceServices frontier.** They hold `RefCell<&mut Brep>` and write
  through `prepare_surface_cache`/`face_surface_cache` (ray.rs:173,198) and `with_face_entity_mut`.
  About 11 entry points reach it. On fresh bodies this is stamps only for 6 of them, with no value
  changed (MEASURED). It is `&mut` access without a write, so N in practice, plus L2 where a face's
  cache is new.
- **L5. check's leaves** (report 15 E2): the `set_validate_only` toggle (F), the gap caches (G,
  MEASURED on 9/9 fresh bodies) and `fix_reversed`. Used by check and check_solid.
- **L6. `create_missing_uv_trim_curves` in to_record** (convert `brep/record.rs:131`), T, 1 entry
  point. MEASURED: it makes nothing on fresh bodies.

## 4. Blockers ranked by entry points unblocked (INFERRED)

1. **Trims as a precondition, not something made on read.** This means L1 :3088/:3223, L3, L6 and the
   mass trims. A read is allowed to assume every trim exists, as `QueryTopology::Prepared` already
   does, and the edit door's ops make sure of it. The probe gives two reasons this holds:
   `create_missing_uv_trim_curves` was a no-op on all 9 bodies, and no fresh call made a trim. This
   unblocks the T part of about 26 entry points. On its own it makes 6 READY: tangent_chain,
   tangent_face_chain_ordered, carried_faces, face_plane_normal, to_record and volume. Their only
   measured writes were T on stripped bodies.

   The risk: a body that reaches a read with a trim still missing, such as a record with no pcurve,
   or a failed trim (`drop_curve_fail`). With this change it gets a refusal instead of an answer, so
   a test over the whole corpus must prove that never happens. Note this is not
   `read_else_edit`: a missing trim is reported as an error, never sent back to the edit door.
2. **The trimmed tree as a fill-once cache behind `&self`.** That is, `build_tree_with_face` over
   `&Brep` for a face whose trims all exist. The slot already lives behind a shared lock, so filling
   it from `&self` fits the pure fill-once rule. Combined with 1, this unblocks the C group:
   face_line_hits, face_line_nearest, face_normal_at_point, point_on_face,
   classify_points_with_normals, face_normal_at_edge, edge_faces_along, edges_share_wedge,
   continuity_report, face_section and first_side (11). It also lets LiveBrepObjects and
   BrepFaceServices become `&Brep`, which brings in the ray, distance and lie-on group (6).
3. **The editing-flag toggles as arguments (F).** These are `set_brep_editing_enabled` inside
   `BrepCacheFace` and `set_validate_only` in check. They are needed for 2 and for check.
4. **Gap caches as fill-once memos (G).** These are the edge-use and vertex-use gaps written by check
   and check_solid, plus calc_vertex_face_gap on face_hits_by_ray. 3 entry points.
5. **inside_points.** After 1 and 2 it must call the `&mut`-free classify directly. Otherwise
   `curve_on_classify` keeps adding a snapshot copy for each UV line.

The smallest single kernel change that unblocks the most is 1. A change to
`get_or_create_uv_trim_curve`'s read callers alone (binormal, `ensure_trim_curve`, `normalized_evaluate`)
moves 6 entry points to READY. With 2 it makes about 23 of the 32 READY.

## 5. measure_body callers and what forces the copy (READ + MEASURED)

| Caller | What forces the copy |
|---|---|
| brep_volume (body.rs:475) | mass trims (T): MEASURED only on stripped bodies |
| check (check.rs:117) | F `set_validate_only`, G gaps (MEASURED 9/9 fresh), C on 2/9 |
| check_solid (measure.rs:794) | check's leaves plus volume |
| edges_share_wedge (blend.rs:2206), edge_faces_along (:2598), carried_faces (:2726), tangent_chain without order (:2802) | binormal T/C. C measured on the fillet body for edge_faces_along and edges_share_wedge. Nothing measured for carried_faces and tangent_chain on fresh bodies |
| beads (bead.rs:87) | binormal T/C through `on_body` (READ) |
| band_room (blend/room.rs:121) | not walked; it is not on the read list |
| test-only: ball_corner_tests `enclose_volume`, test_corpus `brep_area` | mass trims |

## 6. What was built

Nothing on `cache-reads23`: no entry point is class (i). The scratch branches hold only probe code
(`zz_leaf` stubs, the `ZZ_DIRECT` switch, `write_stamps` made pub). They stay local and on the build server
side refs `push/cache-reads23-probe` and `push/cache-reads23-measure`.

## 7. Limits

- Not run by the probe: beads, inside_points, first_side, project_curves, principal_curvatures and
  closest_to. Their verdicts are READ only.
- The probe calls each entry point once, with one set of arguments. A path not taken there (for
  example `create_uv_trim_curve` at :3223, or a seam branch) is covered only by the compile walk.
- The stripped state is made by hand. It stands for bodies with lazy trims; it is not a state that
  main's ops are seen to leave.
