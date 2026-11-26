# 10 — The face-level UV trim pass: idempotence, callers, cost, scaling

Checkout: `sc-wt/cache-arch`, main at 782ef9afc. Paths are relative to
`solidcore/crates/solidcore/src`. Read-only analysis: nothing was built or run for this report. The
only numbers quoted come from logs already in this folder (`m1-cache-counts*.log`, produced by
`cache-measure-local.patch`, which bumps `FACE_TRIM_PASSES` at the top of
`create_uv_trim_curves_body` and `UV_TRIM_CREATIONS` at the top of `create_uv_trim_curve`).

Tags: **[V]** verified by reading code (or present in an existing log); **[I]** inferred;
**[U]** unverified.

## 0. Summary

- **Idempotent, with conditions.** [V/I] A second call with no edit in between writes nothing to the
  body in the normal case: loop 1 only does lookups, loop 2's gap fix does not fire again because
  the first pass closed the gap to rounding (about 1e-16·|uv|), well below the gate of
  1e-12·(1+|uv|), and the vertex tolerance raise uses a strict comparison against an unchanged
  value. Exceptions are listed in §2.3. The **return value is not idempotent**: curve/surface gap
  statistics are 0 on a repeat. No caller in the kernel reads the statistics apart from goldens
  (§2.4).
- **Where the passes come from.** [V, log] In the drill and slot probes,
  `TRIM_TREE_BUILDS_WITH_FACE=0`, so no pass comes from a cache implant. The Boolean's pass source
  is `update_faces` (`topology/proto_topology/p2.rs:3834-3835`), which runs two passes (old face
  and new face) through `calculate_uv_domain_from_uv_trim_curves` after every face split made by
  imprinting (`p2.rs:1710,1720`).
  - Drill: 8 passes = 2 × 4 splits, and the count stays 8 regardless of how many faces the plate has.
  - Slot: passes = 16·H + 10 for H holes, which is exactly 2 × (8 splits per hole + 5). Against
    face pairs (3H+4) that gives the "about 5 per pair" ratio (5.33 asymptotically).
  - Each pass sees a **new** face state, because a split is an edit. These passes are not wasted
    repeats of the same state. The waste is that each pass re-walks every edge use of the face,
    even though only the edge uses next to the split changed.
- **Cost of a repeat pass.** [V] Per edge use: 2 UV trim `evaluate_point` calls, 1 edge-curve
  `evaluate_point`, a few record reads and a gap test. Per call: a surface owner clear and restore,
  a memoised singularity lookup, the natural UV domain, and two Vec allocations. In
  `calculate_uv_domain` an O(E_f) bbox union follows. No current `WorkCounters` field
  (`kernel.rs:182-205`) counts any of this.
- **Scaling (#14453).** [I] There is a (splits of face f) × (edge uses of f) term. For the big
  cylinder face that is quadratic in the number of turns. The issue's profile limits it to about
  3% at 48 turns (`update_faces` 26.8% inclusive, of which `classify_param_on_faces` is 23.8%). It
  is real but not the main cost. Counters to add: `face_trim_passes` and
  `face_trim_edgeuses_walked` (§5.2).
- **Options.** (a) A per-face "trims settled" mark, which is risky because many edits bypass face
  notification. (b) Incremental work on the split only, which removes the quadratic term. (c)
  Skipping the pass in `face_trim_uv_domain` once the face is settled. (d) A safer coarse variant:
  a Brep edit generation. Prove any of them with bit-identical goldens and a debug shadow pass
  (§6).

## 1. What the routine does

Entry: `create_uv_trim_curves` (`topology/face/p3.rs:1231`) calls
`create_uv_trim_curves_notified(.., None)` (`:1249`). Wrappers: `topology/face/p1.rs:256`
(adjust=false), `topology/trimming_tools.rs:349` and `:769` (adjust=false),
`query/face/p2.rs:256` (`BrepCacheFace`, adjust=true).

### 1.1 Prologue and epilogue [V]

- `PreRescue` reads the face's `uv_domain`, the surface poles via `get_singularities_of_face`
  (memoised, `surface/surface/p3.rs:745-777`) and `natural_uv_domain_with_nurb`.
- `RescueSurfaceOwner::new` (`topology/face/p2.rs:3777-3810`), then `set_owner(None)` and
  `b.begin_surface_owner_cleared(f)`.
- After the body: `end_surface_owner_cleared`, `rescue.restore`, `sync_face_surface_tolerance`. The
  routine never changes the face tolerance, so the owner and the surface's copy of the zone
  tolerance return to exactly what they were before.

### 1.2 Body `create_uv_trim_curves_body` (`face/p3.rs:1295-1470`) [V]

**Loop 1 (about `:1320-1387`)**, for each loop use and each edge use:

- `total_dropped += 1`. An edge use without an edge is skipped.
- `if eu1::uv_trim_curve(b, eu)?.0.is_some() { continue; }` reads the edge use's own trim or
  its mate's holder (`topology/edgeuse/p1.rs:627`). **When every trim is already present, loop 1
  does nothing else.**
- When the trim is missing and caller UV curves were supplied: `find_best_uv_curve`, then
  `set_uv_trim_curve(New)` or `check_and_set_uv_trim_curve`.
- Otherwise: `get_or_create_uv_trim_curve(k, b, eu, true)` (`edgeuse/p3.rs:1690`) →
  `create_uv_trim_curve` → `trim_raise_distance` → `set_uv_trim_curve`
  (`edgeuse/p1.rs:715`). That call clears both mate slots, resets both start vertex uses'
  `vertex_face_gap` (`:771-778`) and raises the edge tolerance to 2·dist (`:811-823`).
- Mean and maximum curve/surface gaps are accumulated **only for edge uses that were missing a
  trim**.

**Loop 2 (about `:1389-1460`)**, for each edge use:

- Reads `cw = eu1::cw_edgeuse`, the vertex use and the vertex.
- `uv_pnt0 = normalized_evaluate(eu, 0.0, uv)` and `uv_pnt1 = normalized_evaluate(cw, 1.0, uv)`.
  On an existing trim this goes through `existing_uv_trim_curve` (`edgeuse/p3.rs:2774`), which
  reads only the cached `max_edge_face_gap` and `drop_curve_fail`, followed by one curve
  `evaluate_point` (`edgeuse/p3.rs:3402-3600`). `Extent1d::evaluate` returns exactly min or max at
  t=0 and t=1 (`geom/extent1d.rs:324`).
- If `uv_gap > EFF_ZERO·(1+max|uv|)` (EFF_ZERO = 1e-12, `math.rs:45`):
  - both ends are classified against the surface domain to decide `gap_at_pole`;
  - if the gap is not at a pole, `tol_3d = max(brep, vertex, both edge tolerances)` and
    `face2::fix_uv_trim_curves_notified` runs; if it fixes the gap, both ends are re-evaluated.
- `pnt_3d0 = normalized_evaluate(eu, 0.0, 3d)` evaluates the edge curve, and
  `gap_3d = |pnt_3d0 − vertex point|`.
- The curve/curve and 3D gaps are accumulated.
- `if adjust_tolerances && vtol < 2·gap_3d { vertex::set_tolerance(v, 2·gap_3d, only_larger) }`.
  `vertex::set_tolerance` (`topology/vertex.rs:195`) is a pure field write with no notification
  and no revision counter.

It returns `UvTrimCurveGaps`, with each mean divided by `total_dropped`.

### 1.3 `fix_uv_trim_curves_notified` (`face/p2.rs:3296-3480`) [V]

- It returns false and writes nothing if the gap is below EFF_ZERO/10 or above min_domain/20.
- Otherwise:
  - it computes the UV vertex point from the cached `vertex_face_gap` or from `compute_uv_point`;
  - it evaluates the surface point and normal and both edge points;
  - `trim_before` ×2 clones the trims and computes the fit tolerance;
  - unless `k.fixes.uv_trim_corner_end_points_only` is set, it runs `drop_point` on both trims and,
    if the parameter moved, `trim_with_zone_tol` + `edit_parameterization`;
  - `notified_end_point_edit` ×2 moves both ends onto the UV vertex point.
- `edit_end_point` (about `:3210-3245`):
  - BSpline: `spread_end_move` (`:3119`) moves the inner control points, using `take(stop)`, so
    the far end never moves. Then the end point itself is edited.
  - Line: the base edit plus `refresh_analytics`.
  - Any other kind: Err.
- `keep_trim_covered` ×2 (`:3581`) exits early if the tolerance did not drop and the debug-format
  of the curve kind is unchanged. Otherwise it computes `trim_deviation_bound` and may raise the
  edge tolerance.

### 1.4 Notification [V]

`face3::notify` PreEdit (`face/p3.rs:480-510`) sets `rectangular_trim = UNSURE` and marks the
trim cache `face_was_modified`. PostEdit walks the edge uses only when there are listeners (#14329).
When an env is supplied, edits notify only when the fix runs.

## 2. Idempotence

### 2.1 With every trim present, nothing in between [V]

| Write | Repeat? | Why |
|---|---|---|
| loop 1 trim creation, edge tolerance raise, vertex-use gap reset | no | skipped: the trim exists |
| loop 2 end move (`fix`) | no (normal case) | the first fix put both ends at the same UV point; the residue is rounding of order 1e-16·|uv|, far below 1e-12·(1+|uv|) |
| fix on an unfixable gap (pole, > domain/20, < EFF_ZERO/10) | the same no-op | gated identically, no write |
| vertex tolerance 2·gap_3d | no | gap_3d depends only on the edge curve (t=0 exactly) and the vertex point, both unchanged; the comparison is strict and the first pass already set vtol ≥ 2·gap_3d |
| owner, cleared slot, surface zone tolerance | restored | §1.1 |
| singularity memo, surface cache slot | not body state | memo only |

### 2.2 Result [I]

On the same face state the second pass changes no field of the body. This rests on reading the
code, not on a run (§7).

### 2.3 Exceptions where a repeat can write [I]

1. **The first pass after an edit** is not a repeat. It can create trims, move ends and raise
   tolerances.
2. **A trim whose interval end is not its end control point**, such as an unclamped or periodic
   B-spline. The end edit moves the control point, but evaluation at t=0/1 may not land exactly
   on it, so a residue above the gate is possible and the fix could fire again.
3. **A trim kind other than BSpline or Line** with an open gap: `edit_end_point` returns Err on
   every call, so a repeat errors again. That is repeatable, but it is not a no-op.
4. **The fix itself raised a tolerance** (`keep_trim_covered`). That changes `tol_3d` for the next
   pass but not the gap, so it should not re-fire. [U]
5. **The `drop_point` / `edit_parameterization` branch** reparametrizes the trim. If the drop
   moves the parameter again on a second call, the trim could be re-cut. In the normal case the
   gate stops this before the drop runs. [U]

### 2.4 The return value is not idempotent [V]

On a repeat, mean and maximum curve/surface gaps are 0, because loop 1 skips existing trims.
Curve/curve and 3D gaps come out the same. In the whole repo `UvTrimCurveGaps` and its fields are
read only by `face/p3.rs` itself and `goldens/src/units/topology_face_p3.rs`; the other callers
drop it (for example `topology/brep/p7.rs:386`, "the gap statistics are not read"). An early
return therefore has to keep the goldens unit's semantics, or else return cached statistics.

## 3. Callers

### 3.1 Inside the Boolean [V]

| Path | Passes | Notes |
|---|---|---|
| `update_faces` (`topology/proto_topology/p2.rs:3834-3835`) via `face_trim_uv_domain` (`boolean/arena_pair.rs:828`) → `calculate_uv_domain_from_uv_trim_curves` (`topology/face/p1.rs:569-640`, the pass at `:632`) | 2 per face split | called from `p2.rs:1710,1720` after `make_edge_in_face`; then `classify_param_on_faces`, the #14453 hot path. The pass is **unconditional** here |
| cache implant: `TrimSrfCache::implant_face_body` (`surface/trim_srf_cache.rs:832`) from `build_tree_in` (`:757`) | 1 per tree build with face | only when `!(editing_enabled && !implants_while_editing)`; `trim_srf_cache_tree` (`surface/surface/p1.rs:1037`) skips the build entirely while `doing_boolean()`. Editing-disabled windows: `topology_intersector/p2.rs:973,1327` (`close_intersection_loop`), `merge/p1.rs:465` (`classify_point_editing_disabled`, used at `merge/p1.rs:417`, `merge/p4.rs:1483`, `merge/p5.rs:73`). Boolean caches come from `with_face_caches` (`topology_intersector/p1.rs:4705`) and `prepare_face_surface_cache` (`surface/surface_cache/face_context.rs:343,387-395`) |
| ray classification: `query/brep/ray.rs:197` → `get_tree_with_face`; `implanting_while_editing` when `k.fixes.brep_ray_classifies_current_trims` (`:222`); `RayPreparation::prepare` (`:100-125`) | 1 per prepared face | `ray/live.rs:688,720` |
| `edge_face_intersect` (`topology_intersector/p1.rs:3012`), `edge_face_coincidence` (`p2.rs:2626`) | per edge × face | only reached from `merge/p2.rs merge_added_body`; `merge_added_face` is used only by goldens, so not in the Boolean |
| `face_face_coincidence` (`topology_intersector/p1.rs:3976`) | rare | only faces with identical edge sets |

**Log evidence [V, log]:** `drill/*` and `slot/*` show `TRIM_TREE_BUILDS_WITH_FACE=0` and
`UV_TRIM_CREATIONS` far below `FACE_TRIM_PASSES` (drill 2 vs 8; slot32 292 vs 522), so the passes
in those runs do not come from implants. `torus/box` has 2 implants out of 16 passes, and
`round/drill*` has 2 out of 8.

**Decomposition [I, strongly supported]:**

- Drill: 4 splits per hole (top face by the circle, bottom face by the circle, the tool cylinder
  split twice) × 2 = 8, constant regardless of the plate's face count, which matches the log.
- Slot: passes = 16H + 10 fits all four rows exactly (74, 138, 266, 522 for H = 4, 8, 16, 32).
  That is 8 `update_faces` calls per hole plus 5 fixed ones. face_pairs = 3H + 4, so passes per
  pair → 16/3 ≈ 5.33. **The "about 5 per pair" figure is 2 passes per split divided by the splits
  per pair, not 5 separate callers.**

### 3.2 Outside the Boolean [V]

- **Unconditional, via `calculate_uv_domain_from_uv_trim_curves(_into)`:**
  - offset: `offset/offset_executive/edgeuse.rs:778`, `offset/offset_executive/p2.rs:1879`;
  - topology: `topology/brep/p9.rs:2884`, `topology/face/curve_on.rs:51`, `face/p1.rs:1761`,
    `face/p2.rs:1055,1756`, `topology/brep/p4.rs:465`, `topology/edge.rs:377,1849`;
  - fillet/blend: `geometry.rs:628`, `intersector.rs:319`, `keep_features.rs:394`,
    `trim_at_creases.rs:2216`, `law_corners.rs:278,724`, `fillet_geom/p1.rs:3159`.
- **Only when trims are missing:**
  - `trimming_tools.rs:713` and `trimming_tools.rs:2235` (`check_face`);
  - `face/p1.rs:753` (`create_oriented_trim_boundary`).
- **Always, on new or changed faces:**
  - `heal/heal_data/p1.rs:786`;
  - `topology/brep/p6.rs:975` (`build_face`, `calculate_trims`);
  - `topology/brep/p2.rs:2336` (`replace_surface`, notified);
  - `topology/brep/p7.rs:1869` (`make_face_with_curves_transfer_impl`);
  - `topology/brep/seam_turn.rs:126`, `topology/brep/mirror.rs:213` (all faces),
    `offset/offset_geometry_creation.rs:2637`.
- **Cache paths:**
  - `query/face/p2.rs:540` (`surface_cache_state_of_face`, used only at
    `offset/offset_executive/edgeuse.rs:686`) runs a pass whenever the kept cache has no tree;
  - `surface_cache_of_face` (`query/face/p2.rs` about 560-640) and `face_cache_tree` (`:661`)
    rebuild, and so implant, when `!have_t_surface_cache` on a face-owned cache;
  - `compute_face_point_class_in` (`intersect/curve_class/p1.rs:2974`).
- **check2 after the Boolean:** the log shows passes equal to tree builds with face (torus 4/4,
  ball 3/3; drill/slot 0).

## 4. Cost of a repeat pass [V]

Let E_f be the number of edge uses of the face.

| Item | Count | Notes |
|---|---|---|
| UV trim `evaluate_point` | 2·E_f | start of eu, end of cw |
| edge curve `evaluate_point` | E_f | `pnt_3d0` |
| record reads (cw, vertex use, vertex, trim slot, cached gap) | O(E_f) | |
| gap compare, pole classify | E_f / only when the gap is open | |
| owner clear and restore, cleared-scope push and pop, `sync_face_surface_tolerance` | 1 | |
| `get_singularities_of_face` | 1 | memoised |
| `natural_uv_domain_with_nurb`, `upward_loopuses`, 2 Vec allocations | 1 | |
| bbox union (only in `calculate_uv_domain`) | O(E_f) | |

Mapping to `WorkCounters` (`kernel.rs:182-205`): **none of the 22 fields counts this work.** The
closest are `loop_walks` (not bumped here) and `vertex_drops` (point drops, not trims). For a cache
implant, the tree build that comes with the pass dominates; the pass itself is cheap arithmetic.

## 5. Scaling and #14453

### 5.1 Is there a pairs × edge-uses term? [I]

Yes. Every split of face f costs 2 passes, each O(E_f(t)), where E_f grows with every earlier
split. For a face split S times this is about Σ E_f ≈ S·E_avg, which is quadratic when E_f itself
grows linearly in S. In #14453 the big cylinder face gains edges with each turn of the faceted
groove. The issue reports 203-250 edges walked per `update_faces` call at 48 turns.

Bound from the issue's profile at 48 turns: `update_faces` is 26.8% inclusive and
`classify_param_on_faces` 23.8%, which leaves at most about 3% for the two passes, the bbox union
and the rest. **The term is real but minor. The dominant cost is
`contains_uv_point_with_topology` (41.1%) inside classification**, which has its own
(splits × edges) shape.

### 5.2 Counters to add

All proposed. Names follow the `WorkCounters` style.

| Counter | Where | Prediction |
|---|---|---|
| `face_trim_passes` | top of `create_uv_trim_curves_body`, `face/p3.rs:1295` | drill = 8 per hole; slot = 16H + 10 |
| `face_trim_edgeuses_walked` | loop 2, `face/p3.rs` about 1389 (one bump per edge use) | grows about S·E_avg in #14453; super-linear in turns |
| `face_trim_fixes` | `fix_uv_trim_curves_notified`, `face/p2.rs:3296`, when it returns true | about 0 on a repeat pass |
| `update_faces_calls` | `topology/proto_topology/p2.rs` at the start of `update_faces` (about 3833) | = face_trim_passes / 2 in drill and slot |
| caller tag | a thread-local phase label, or `#[track_caller]` on the face/p1.rs:256 and trimming_tools wrappers | attributes passes to update_faces, implant, ray, check |

## 6. Options that change no result

| Option | Mechanism | Risk |
|---|---|---|
| (a) per-face "trims settled" flag | set at the end of a pass; cleared on `set_uv_trim_curve` or delete on any edge use of the face or its mate; on vertex point or tolerance changes, edge curve, interval or tolerance changes, loop or edge-use insertion or removal, surface or domain changes; and on face notify PreEdit | **High.** Many writers do not notify the face: `set_uv_trim_curve` (`edgeuse/p1.rs:715`), `vertex::set_tolerance` (`vertex.rs:195`), and direct `*_mut` record edits (`topology.rs:1629-1690`). One missed clear gives a wrong skip |
| (a') Brep edit generation | a counter bumped in every `*_mut` accessor; a face is skipped if the stamp is unchanged | safe by construction but coarse. Inside `update_faces` every split bumps it, so it gives **nothing** for the #14453 path; it only helps back-to-back reads such as implant plus ray plus check |
| (b) incremental pass | after a split, walk only the edge uses of the new edge and their cw/ccw neighbours (the only ones whose trims or vertices changed) | fixes the quadratic term. It needs proof that untouched edge uses could not have changed: `make_edge_in_face` writes only near the split [U] |
| (c) skip the pass in `face_trim_uv_domain` when settled | the caller discards the statistics; only the domain bbox is needed | same requirement as (a). Unconditionally skipping changes results, because the first pass after a split can fix or raise |
| (d) one pass per face per phase | a table keyed by FaceId inside the Boolean | fails across splits. Useful for implant, ray and check cycles |

Because the pass is idempotent (§2), the skip ideas preserve results apart from the §2.3
exceptions. **To prove a change:**

- bit-identical goldens and body hashes on the corpus;
- a `debug_assertions` shadow: run the full pass on a clone whenever the skip fires and assert no
  field changed (trims, tolerances, vertex-use gaps);
- `face_trim_fixes` equal to 0 on every skipped occasion in the instrumented run.

## 7. Still unverified, and the measurement that settles it

| Item | Measurement |
|---|---|
| Passes in drill and slot come only from `update_faces` (2 per call) | `update_faces_calls` plus a caller tag on the existing probe; expect passes = 2·calls |
| The trims of a freshly split face are all present before `update_faces` | `UV_TRIM_CREATIONS` bumped from inside `update_faces`-driven passes (caller tag); the existing log suggests creations ≪ passes |
| A repeat pass never fixes or raises | call the pass twice back to back in a probe, count `face_trim_fixes` and vertex raises on the second call, and hash the body before and after |
| §2.3 items 2, 4, 5 | the same probe on unclamped or periodic trims and on pole faces |
| Share of #14453 | `face_trim_edgeuses_walked` and wall time at 12/24/48 turns; expect super-linear growth but ≤ about 3% of the total |
| Ray-path share of passes | caller tag on `ray.rs:197` |
