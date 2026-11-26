# 24 — Stage C: point classification and ray queries on a body held by shared reference

Read-only study, same method and vocabulary as report 07. Code read: worktree
a local worktree (main at 21699e974a plus an unrelated validation change). Paths are
relative to `solidcore/crates/solidcore/src`; `OPS/` = `solidcore/crates/solidcore-ops/src`.
Marks: **[V]** read in code for this report, **[A]** taken from an earlier report or PR body,
**[I]** inferred, not checked line by line.

#21107 (`LazyBrep::Prepared`, `read_else_edit`, `missing_uv_trim`, `classification_builds_tree`,
the `_prepared` measures) was merged as 26d93c7d1e, 27 minutes after the worktree's base, so it is
**not in the worktree**. It is cited from its diff and called "post-base" below.

---

## 0. Findings that change the picture

1. **No classification or ray path draws a random number.** Point classification fires fixed axis
   rays (`CLASSIFY_RAYS`, `query/brep.rs:520-524`: +x/-x, +y/-y, +z/-z), and `ray_fire` has no
   generator either (`topology/brep/p2.rs:2070-2200`) [V]. The only `Kernel::seeded_rng` users
   (`kernel.rs:1092`) are `is_inner_shell_of` (`topology/shell.rs:1732`), `points_in_face`
   (`topology/face/p2.rs:1723`) and `is_inner_shell` (`topology/topology_traverser.rs:784`), and
   none of them is reached from these entries [V for the grep, I for reachability]. So #20942 does
   not matter here.
2. **The standing copy writes into the real body's cache slots.** `clone_standing_for_self`
   (`topology.rs:1320`) gives each copy surface the original's cache slot
   (`topology.rs:1274-1302`, `stand_surface_for`) [V]. A trimmed cache whose tree is built on the
   copy (against the copy's UV trims and raised tolerances) is stored in the slot the real body
   reads (`query/face/p2.rs:575-626`, `:1144`) [V]. So "the read works on a private copy" holds for
   topology and tolerances but not for caches. Later reads and edits on the real body find that
   cache, already holding a tree. This is item C2.
3. **On a body that already has every UV trim and every face cache tree, nothing in these paths
   writes, yet the copy is still made.** `Target::mutable()` (`query/brep.rs:1140`) and
   `LazyBrep::get_mut()` (`query/brep.rs:1107`) clone on first *access* to the `&mut` form, not on
   the first write. The boundary solve asks for the `&mut` form for any body with faces
   (`brep.rs:843-991`), and `cell_directed_max` asks for it for every classified solution
   (`query/face/p3.rs:849-851`) [V]. So the one copy left per call (report 19) is pure overhead on
   prepared bodies, and an up-front flag test can remove it with identical answers.
4. **Both C items disappear on a prepared body**, unlike the Boolean's pair-phase couplings. The
   read-only machinery is mostly built already (`QueryTopology::Prepared`,
   `point_classify_prepared`, `RaySurfaceContext`, post-base `LazyBrep::Prepared` and
   `read_else_edit`). What is missing is a read-only twin of `BrepFaceServices` that serves face
   caches from the slots without building them.

---

## 1. The operations as they are

### 1.1 Inventory of public entries (question 1)

| Entry (OPS) | Door | Kernel chain to the first write | What is written, how often |
|---|---|---|---|
| `Body::classify_point` `measure.rs:682`, `classify_points` `:697` | `read_body` (&Brep) | `locate_points` `measure.rs:190` → `BrepClassifier::of_brep` `brep.rs:1152` → `classify` `brep.rs:843` → boundary solve `target.mutable()` `brep.rs:1140` (copy) → `solve_mutable_brep_point_kept` `query/brep/ray/live.rs:81` | standing copy: always, once per call, on a body with faces. On the copy: UV trims and tolerance raises when trims are missing. Cache slot: a build when a slot is empty or treeless (shared with the body, finding 2). Per-call memo in a `Cell` [V] |
| `Body::face_at` `pick.rs:167` | `read_body` | `BrepClassifier::point_3d_classify` `pick.rs:173`, same chain | same, one point [V] |
| `Body::bounds` `measure.rs:631` | `read_body` | `brep_bounding_box` `query/api_queries.rs:911` → `calculate_tight_bounding_box` `brep.rs:255` → `qf3::calculate_tight_bounding_box_into` `query/face/p3.rs:490` → `cell_directed_max` `p3.rs:769` → `point_classify(lb.get_mut()?)` `p3.rs:849-851` (copy) | copy at the first classified extremum. On the copy: trims via the loop walk, and the own cache's tree via `face_cache_tree` `query/face/p2.rs:1061`, which lands in the shared slot [V] |
| `face_hit_by_ray` `measure.rs:805` | `with_body` (snapshot, `&mut`) | `BrepQuery::first_face_hit` `brep.rs:764` → `solve_mutable_brep_point_kept` | on the real body: trims, tolerance raises, slot caches. Per-call memo [V] |
| `fire_ray` `measure.rs:1026` | `with_body` | `topology::brep::p2::ray_fire` `topology/brep/p2.rs:2070` → `solve_mutable_brep_point` (fresh face-cache table, **not** the kept one) | the same, plus the per-call face-cache table rebuilt (copies from the slot) [V] |
| `face_distances` `measure.rs:1079` | `with_body` | `LiveBrepObjects::new(b,…)` `measure.rs:1097` → `face_closest_point_in_shape` per point | lazy classification writes through `BrepFaceServices` [V for the call, I for which writes fire] |
| `closest_to_point/curve` `measure.rs:853-1008` | `with_body` | `api_queries::closest_to_members` `query/api_queries.rs:1058` → `LiveBrepObjects::new` | as above [V] |
| `face_distances_within` `measure.rs:1157` | `with_body` | `api_queries::faces_closest_distances_within` `:1279` → `LiveBrepObjects::new` | as above [V] |
| `face_normal_at_point`, `classify_points_with_normals` `measure.rs:1354,1381` | `with_body` | `api_queries::face_parameters_at_point` `:2311` → `query::face::p1::point_3d_classify_at` | trims on the face's loops (Mutable walk) [V for the chain] |
| `point_on_face` `measure.rs:1432` | `with_body` | `query::face::p1::point_on_face` `query/face/p1.rs:1075` → `point_3d_classify_at` | as above [V] |
| `face_normal_at_edge`, `face_plane_normal` `measure.rs:1329,1491` | `with_body` | `api_queries::face_normal_at_edge` `:2360` → `edgeuse_face_frame` | the edge use's UV trim when missing [I] |
| `inside_points` (#21072 table) | `with_body` | `api_queries::face_internal_point` `:932` → `topology/face/p2.rs:1471 calculate_an_internal_point(&mut)` | trims through UV-line classification [I]; no random draw in its first 40 lines [V] |

Do the results depend on earlier calls' writes? Across calls, yes, but only through state that
persists: (a) trims and tolerances on the real body (`with_body` entries only); (b) slot caches
(every entry, finding 2). Within one call, each later point, ray or face sees the earlier ones'
trims, tolerances and caches (C1, §2.3). The per-call memos (`KeptFaceCaches`, evaluation copies)
die with the call.

### 1.2 Phase table: `classify_points` (the batch form; `classify_point` and `face_at` are n = 1)

| # | Phase | file:line | Kind |
|---|---|---|---|
| 0 | shared door: body lock, `&Brep` | `OPS/measure.rs:701` | — |
| 1 | zone tolerance | `OPS/measure.rs:195` | read |
| 2 | `BrepClassifier::of_brep`: a `BrepQuery` with an empty `OnceCell<BrepCache>`, `KeptFaceCaches` and bbox, and no copy yet | `brep.rs:1152-1228`, `:588-595` | read |
| 3 | per point: boundary solve. Without faces, read-only `BrepObjects`. Otherwise `target.mutable()` makes the **copy** once, then `solve_mutable_brep_point_kept`: `FaceCacheKey::of`, `kept.take`, `LiveBrepObjects::with_face_caches`, `brep_point_solve`, `kept.put` | `brep.rs:843-930`, `live.rs:81-102` | read-like, writes (§2) |
| 4 | regions of the boundary hits; a single region returns early | `brep.rs:930-970` | read |
| 5 | per point: up to 6 ray fires (+axis, then -axis if no answer; stop at the first region found), each `ray_fire_with_bbox_and_solver` with the same kept solve. Best hit = nearest, then lowest dimension, then lowest parameter, **in solver order** | `brep.rs:972-985`, `topology/brep/p2.rs:2128-2200` | read-like, writes (§2) |
| 6 | `located`: the region's material | `OPS/measure.rs:218-241` | read |
| 7 | results in input order | `OPS/measure.rs:197-200` | — |

`bounds`: 0 → vertices → edges (`check_cancelled`; an edge inside the box so far is skipped) →
faces (`calculate_tight_bounding_box_into`, skipped the same way; the extrema are classified per
solution in `cell_directed_max`) → union [V `brep.rs:255-299`, `p3.rs:769-880`]. The rays
(`face_hit_by_ray`, `fire_ray`) are phases 3/5 alone on the real body.

---

## 2. Every write in a read-like phase

### 2.1 The list

| W | Write | file:line | Through | Frequency | Class |
|---|---|---|---|---|---|
| W1 | standing copy of the whole body (edges' curves and faces' surfaces as evaluation copies) | `brep.rs:1140` (Target), `:1107` (LazyBrep), `topology.rs:1342-1360` | new allocation | **always** once per `read_body` call on a body with faces [V] | not a write to the body; the cost to remove |
| W2 | UV trims made by the face routine during a tree build (`create_uv_trim_curves(…, adjust=true)`) | `query/face/p2.rs:256-259`, via `build_tree_with_face` `:680` and `make_trim_srf_cache_with_face` `:619` | `&mut` copy (read_body) or real body (with_body) | common on fresh bodies (5-6 edge uses on simple bodies [A brief]); none when every trim exists [I, U2] | **C1** (P when nothing is missing) |
| W3 | UV trims made by the edge-use routine in the Mutable loop walk (`contains_uv_point_with_topology`), and in `compute_uv_point(ok_to_make)` | `query/face/p2.rs:1258`, `:312` | same | as W2 | **C1** |
| W4 | edge/vertex tolerance raises that come with W2/W3 (adjust = true) | as W2 | same | rare [A: none moved on measured bodies] | part of **C1** |
| W5 | trimmed cache built and **stored into the surface slot** (the slot is shared with the real body, finding 2) | `query/face/p2.rs:615-626`, `:1144`, `query/brep/ray.rs:197-248` | `Arc<Mutex>` slot through the copy | once per face per body lifetime, on the first query to reach that face [I] | within a call **P**; across calls **C2** |
| W6 | tree built on an existing treeless own cache during classification | `query/face/p2.rs:1061` | slot | rare after a solve has run on the body [I] | **P** (the post-base `classification_builds_tree` flag test predicts it) |
| W7 | `KeptFaceCaches.put`, `builds` counter | `live.rs:151-231` | `Cell` in `BrepQuery` | always | **S** (phase table) |
| W8 | `LiveBrepObjects` evaluation copies of curves and surfaces (`OnceCell`) | `live.rs:427-470` | `OnceCell` | always, once per entity read | **S** (per-worker cursor) |
| W9 | lend: one `Arc` copy per face (#21019) or `map_handles` copy on a cache hit | `live.rs:739-796` | per-call map | always, once per face per call | **S** |
| W10 | `set_brep_editing_enabled` toggled and restored inside a tree build | `query/face/p2.rs:251` | `&mut` | each build | **S** |
| W11 | `BrepQuery` `OnceCell<BrepCache>` and bbox | `brep.rs:705`, `:677` | `OnceCell` | once per call | **S** |
| W12 | kernel work counters (`add_face_extreme_control_points` and the solver counters) | `query/face/p3.rs:798-799` etc. | `&Kernel` interior | always | **S** |
| W13 | gap caches and tolerance growth during a classification, through `with_face_entity_mut` | `live.rs:504-519` | `&mut` copy or body | rare [I, U5] | **R** (refuse in read form, fall back) |
| W14 | `classifies_on_face` retry at 100× the tolerance, on `&mut` | `live.rs:391-426` → `query::face::p1::point_3d_classify` | `&mut` | when the first classification fails [I] | same writes as W3: **C1**/P |
| W15 | slot take-and-store with no change (`take_trim_cache` / `store_trim_cache`) | `query/face/p2.rs:1134-1145` | slot | every classification | **S** (value-preserving; the post-base `classification_builds_tree` does the same through `&Brep`) |

Counts: **P 2** (W6, and W5 within a call), **R 1** (W13), **C 2** (C1 = W2+W3+W4+W14; C2 = W5
across calls), **S 7** (W7-W12, W15). W1 is the copy, not a write.

### 2.2 Why `&mut` (question 2: who owns or borrows what)

| Type | Owns / borrows | Why `&mut` or a copy |
|---|---|---|
| `BrepQuery` `brep.rs:588-595` | `ProvenBrepIdentity`, `OnceCell<BrepCache>`, `KeptFaceCaches` (a `Cell`), `OnceCell<Extent3d>`; not `Sync` | its methods take `&mut Brep` only to pass it on (`bounding_box` `:677` needs only `&`) [V] |
| `LiveBrepObjects` `live.rs:298-309` | `BrepFaceServices<'a>`, identity, `Option<&BrepCache>`, evaluation copies `Vec<Option<OnceCell<Box<…>>>>` | `classifies_on_face` and `with_face_entity_mut` lend `&mut Brep` to the face classifications [V] |
| `BrepFaceServices` `live.rs:629-632` | `RefCell<&'a mut Brep>`, `RefCell<FaceCaches>` (`kept` BTreeMap and `lent` Arc map) | on a cache miss, `prepare_surface_cache` → `face_surface_cache` with `BrepCacheFace{&mut}` (W2, W5, W10); classification goes through `point_classify_live` on `QueryTopology::Mutable` (W3, W6) [V] |
| `BrepCacheFace` `query/face/p2.rs:231-371` | `&mut Brep`, face id | the `CacheFace` trait has `&mut` methods: `set_brep_editing_enabled`, `create_uv_trim_curves`, `vertexuse_uv_point`, `point_classify` [V]. Used only while a cache is built |
| `LazyBrep` `brep.rs:1062-1111` | `Own(&mut)` or `Shared{source,copy}`; post-base also `Prepared(&Brep)` | copies on the first `get_mut`, written or not (finding 3) |
| `Target` `brep.rs:1115-1144` / `BrepClassifier` `:1152-1228` | the same pattern; the classifier keeps one copy for all its points | the same |
| `RayPreparation` / `RaySurfaceContext` `query/brep/ray.rs:53-126, 325-432` | `&Brep` plus a table of prepared caches, pending requests and attempt sets | already read-only, with requests. Production uses the live path; the read-only path restarts per request (documented quadratic, `ray.rs:10-15`) [V] |

**The smallest split.**
- (i) Shared, reusable, `Sync`: `PreparedBrep<'b> { brep: &'b Brep, identity, cache: BrepCache,
  bbox, faces: BTreeMap<FaceId, Result<Arc<SurfaceCache<CacheObject>>>> }`, filled from the slots
  (a lend, no build).
- (ii) Per call or per worker: the evaluation-copy `OnceCell`s (W8), counters (W12), the
  `PointClassification` scratch, and the copy-path fallback state.
- `BrepFaceServices` and `BrepCacheFace` stay as they are, on the prepare and fallback side only.

### 2.3 The C items, exactly

- **C1: lazy trim creation and tolerance growth inside one call.** Point *i*'s boundary solve or
  ray (or face *i*'s extremum in `bounds`) makes trims on the copy, by the face routine (W2) or the
  edge-use routine (W3), and may raise tolerances (W4). Point *j > i* then classifies against those
  trims and tolerances, and builds its face caches after them. Which routine makes a given trim,
  and so its curve and tolerance effect, depends on which item reaches it first. Evidence that the
  order matters: making all trims first, then reading, gives different answers on 7 corpus bodies
  (rounding-size) [A #21107 PR body, probe (b)]. **A deterministic reformulation exists, but it is
  not result-identical:** "create missing trims as one visible op, then read" (the #21105
  decision) changes those bodies' answers. **The result-identical option** is to run the read form
  only when nothing is missing (cheap flag test) and otherwise today's path. That is the plan in §7.
- **C2: copy-built caches persist in the real body's slots across calls.** A `read_body` call on a
  body without trims builds a face's trimmed cache and tree against the copy's trims (W5), and
  stores it in the slot the body shares (finding 2). The body's next query reuses that cache
  (`query/face/p2.rs:585-592`). Its tree and implanted boundary reflect trims and tolerances the
  body does not have, so the next call's answer can depend on whether a read ran before it
  [I; U3]. No reformulation is needed for prepared bodies, which build nothing. For unprepared
  bodies the options are (a) leave it as it is today, or (b) have the copy path store nothing
  (each copy gets a private slot), which costs rebuilds on every repeat (the measured 1-4 builds
  per query, now repeated). **Recommendation: (a)**, and settle U3 before considering (b).

---

## 3. What "prepared" means for these entries

The read form writes nothing on the common path when all of these hold:

| # | Requirement | Cheap up-front test | Exists today? |
|---|---|---|---|
| P1 | every edge use of every face has a UV trim (its own or its mate's) | post-base `missing_uv_trim(b, None).is_none()`: one walk over the edge uses, no geometry [A diff] | after any edit-door measure (mass, area: 5-6 trims made) or a classification on the real body; usually **not** on a body fresh from an operation [A] |
| P2 | no face's own trimmed cache needs its tree built by classification | post-base `classification_builds_tree(b,f)` for every face: one slot take-and-store per face [A diff] | yes, once any solve has reached the face |
| P3 | (solves and rays only) every face the solve may touch has a slot cache that the live path would reuse as it is: `Stored`, tree built, and not (`face_was_modified` and the B-rep not editing) | new `solve_cache_ready(b,f)`, the same slot read as P2 plus the `:585` condition [I] | after the first solve on the body (real or copy, finding 2) |
| P4 | `BrepCache` | built from `&Brep` (`brep.rs:705`) | built per call: an S write, not a precondition |
| P5 | the bbox | from `&Brep` | the same |

If a test fails, take today's path (copy or edit), so answers do not move. The cost of preparing is
zero extra work: the caches are those the first query builds anyway. The trims are the #21105
visible op (`create_missing_uv_trim_curves`), which the caller runs once when it chooses to. That is
a result change on 7 bodies (C1), so it is not automatic.

---

## 4. Caches and the phase-scoped table

- What the read phase needs: one solve cache per face (trimmed, with tree), mapped to `CacheObject`
  handles; the `BrepCache`; and per face the classification's own trimmed cache, which is the same
  slot object.
- Where they come from today: the surface slot (H1) on a hit, copied per call by `map_handles` or
  lent by `Arc` (#21019, `live.rs:739-796`) [V]. On a miss, built by `prepare_surface_cache`
  (`ray.rs:172-248`) and stored in the slot. `KeptFaceCaches` keeps the per-call copies across the
  points and rays of one call, keyed by `FaceCacheKey` (tolerances, context bits, `doing_boolean`,
  fixes, `editing_enabled`; `live.rs:119-142`) [V]. Tests pin this at one build per face per
  classifier (`brep.rs:1711`) [V].
- `fire_ray` does not use the kept table (`topology/brep/p2.rs:2086-2098`): each call starts a
  fresh one. The slot still prevents rebuilds, but the per-face copy or lend repeats. Switching to
  the kept table is P: same factory, same body [I, U9].
- **A keyless phase-scoped table holds here, as in the Boolean.** In the read form the body is
  borrowed shared, so nothing `FaceCacheKey` covers can change during the phase. Tolerances and
  topology are frozen by the borrow; the modes are fixed per call. The key reduces to a debug
  assertion. Across calls the slot is already the keyed, persistent home, and the table only holds
  `Arc` lends of it. Keeping the table itself across calls (beside `Held`, dropped on `DerefMut`,
  report 11) is worth it only if the per-call `BrepCache` build and lends show up in a count (U7).

---

## 5. Items, order and combination (question 4)

- **Items.** `classify_points`: points (1 to thousands; cost skewed by the ray fallback, 0 to 6
  fires per point). `bounds`: vertices, edges and faces (extremum solves per face cell; a few
  classifications per face). Rays: one item per call. `face_distances` and `within`: points × faces.
- **Independence.** In the read form each point is a pure function of (prepared body, point).
  Today the only links between points are C1 and the per-call memo (P).
- **Early exits.** All are per item: a single region returns at once; the first axis that finds a
  region stops; backward fires only if forward found nothing (`brep.rs:972-980`) [V]. In `bounds`,
  an edge or face inside the box so far is skipped. The skip changes which faces run (and so C1 on
  the copy), not the union [I: min/max are exact].
- **Order-dependent choices.** In `ray_fire` and `first_face_hit` the hit is chosen within one
  solve's solution list, and ties go to the solver's order (`topology/brep/p2.rs:2185-2200`,
  `brep.rs:764-807`, strict `<`) [V]. This is per item and deterministic. There are no
  floating-point sums across items.
- **Random draws:** none (finding 1). Should a later entry draw (e.g. `points_in_face`), #20942
  already makes it a function of the call: draw *i* goes to point *i*
  (`topology/face/p2.rs:1697-1723`) [V], which is index-addressable per item.

## 6. What stays sequential

- The door and the fallback decision: one up-front test per call.
- The copy or edit fallback path, unchanged.
- Fulfilling anything (the visible trim op, cache builds on a miss).
- The reduction: results in input order, the box union.

---

## 7. Cost and the PR sequence (question 5)

Rules applied: no step adds work on any path. Each fallback is chosen by a cheap up-front test (a
topology walk plus one slot read per face), never by a mid-run restart on the common path. Counts
are made locally with side-channel instrumentation, not repo counters.

0. *(local measurement, no PR)* On the corpus reads, count: standing copies per call; the fraction
   of calls whose body passes P1+P2(+P3); slot stores made from a copy (C2); mid-run
   `PreparationRequired` refusals once a test has passed (expected 0). This settles U1, U3 and U4
   and decides whether steps 2-3 are worth landing.
1. **`bounds` read form** (no decision; mirrors #21107). In `Body::bounds`, if P1 and P2 hold, call
   `calculate_tight_bounding_box(k, LazyBrep::Prepared(b))`; otherwise call it as today
   (`Shared`). On an unexpected `PreparationRequired`, rerun the `Shared` form (counted; expected
   never).
   - Results: identical. `point_classify_prepared` shares the walk (`query/face/p2.rs:846-856`),
     and #21107's probe (c) found the read form bit-identical after the mutable form.
   - Falls: standing copies per `bounds` call on prepared bodies, 1 → 0.
   - Must not rise: tree builds (0 in the read form), trims made (0), refusals.
   - Unblocks: nothing in the #21072 table (`bounds` is already `read_body`); removes report 19's
     remaining copy for this entry.
2. **Read-only face services and `Target::Prepared`** (no decision).
   - Add `PreparedFaceServices { brep: &Brep, faces: FaceCaches lent from the slots }`. It
     implements what `BrepFaceServices` serves. A miss is `PreparationRequired`, never a build.
     Classification goes through `point_classify_prepared` with the lent cache as
     `prepared_cache` (`query/face/p2.rs:1102-1124` already accepts it).
   - Add a prepared `query::face::p1::point_3d_classify_at` over `QueryTopology`, for
     `classifies_on_face`.
   - `BrepQuery::classify` takes a `Target::Prepared`, and `BrepClassifier` uses it when P1, P2
     and P3 hold.
   - Results: identical when the tests pass [I: the live path on a cache hit copies the same
     cache, and its Mutable walk writes nothing when every trim exists; confirm with probe U6].
   - Falls: standing copies per `classify_point(s)`/`face_at` call, 1 → 0.
   - Must not rise: lends (1 per face per call, as today), tree builds (0), refusals.
   - Unblocks: the services the next steps share.
   - **Decision D1, mid-run refusal** (W13 after the test passed). Options: (a) rerun only that
     point on the copy path; the copy is then made once and kept for the remaining points. (b)
     Restart the whole call on the copy path. **Recommend (a)**: no work is lost for the other
     points.
3. **`with_body` ray and distance entries → `read_else_edit`** (no decision): `face_hit_by_ray`,
   `fire_ray`, `face_distances`, `closest_to_point/curve`, `face_distances_within`.
   - The read form runs on the step 2 services when P1-P3 hold; otherwise the edit runs as today
     under the same lock.
   - `fire_ray` also moves to the kept table (P, U9).
   - Results: identical when the tests pass (#21107 probe (c) pattern; rerun it for rays).
   - Falls: snapshots per call 1 → 0 on prepared bodies; per-call copies or lends in `fire_ray`
     (from per-solve to per-call).
   - Unblocks 5 rows of #21072's table (`face_hit_by_ray`, `fire_ray`, `face_distances`,
     `closest_to`, `face_distances_within`), and with them `principal_curvatures`, which uses the
     same `LiveBrepObjects::new` [I].
4. **Face-point entries → `read_else_edit`** (no decision): `point_on_face`,
   `face_normal_at_point`, `classify_points_with_normals`, through step 2's prepared
   `point_3d_classify_at`. The test is P1 restricted to the face (`missing_uv_trim(b, Some(f))`)
   plus P2 for that face. Same counts. Unblocks 3 rows.
5. **Edge-frame entries** (no decision): `face_normal_at_edge`, `face_plane_normal`, read form when
   the edge use's trim exists (post-base `existing_uv_trim_curve`). Unblocks 2 rows.
   `inside_points` follows step 4 if its UV-line classifications use only the face's own trims
   (U8).
6. **Decision D2, bodies that fail the test** (fresh bodies without trims).
   - Options: (a) stay on the copy or edit fallback (today's results); (b) the operation that makes
     the body runs `create_missing_uv_trim_curves` once as a visible op (#21105), after which every
     read takes the read form; (c) the read form returns typed requests and the owner fulfils them
     (the Boolean's parts 3-4).
   - **Recommend (a) now, and (b) per producing operation as its own result-change PR with
     cases.** (b) moves answers on about 7 corpus bodies (C1). (c) is not needed for these
     entries.
7. **Decision D3, cross-call home for the prepared table** (beside `Held`, dropped on `DerefMut`,
   report 11). Recommend only if U7 shows the per-call `BrepCache` build plus lends matter for
   repeated UI picks. The slot already keeps the expensive part.

The "result change" steps are D2(b) only. Steps 1-5 are each a few hundred lines; the shared part
is step 2.

### 7.1 Shared with the Boolean's five parts (report 07 §6)

| Part | Here |
|---|---|
| 1 `Prepared<'b>` view | **same**: `Target::Prepared` / `LazyBrep::Prepared` / `QueryTopology::Prepared` |
| 2 phase table face → `Result<Arc<cache>>` | **same type**, but filled by lending from the slots: no builds, no key |
| 3 typed requests | **not needed** (a one-shot fallback instead); only D2(c) would need it |
| 4 fulfil loop | **not needed**: `read_else_edit` is one fallback, not a loop |
| 5 per-worker kernel | only for a parallel `classify_points` batch later; the items are already independent in the read form |

## 8. What the Boolean pair loop and the fillet can reuse (question 6)

- `PreparedFaceServices` (step 2) is the read-only twin the Boolean's classification phases
  (report 07 rows 17 and 19) need for their point classifications and ray fires on operands. Its
  `FaceCaches` lent-Arc map is report 07's phase table.
- `SidedFaceServices` and `CurveFaceServices` (`live.rs:979`, `:1101`) are vectors of
  `BrepFaceServices` used by the Boolean and fillet surface intersectors. Once a prepared twin of
  `BrepFaceServices` exists, their twins are mechanical.
- The tests: `missing_uv_trim`, `classification_builds_tree` and the new `solve_cache_ready` are
  the per-face "is this face prepared" checks the Boolean's candidate-face preparation needs.
- The prepared `point_3d_classify_at` serves the fillet's face-point classifications.

## 9. UNVERIFIED, and the count that settles each

| U | Question | Count / run |
|---|---|---|
| U1 | how often a read passes P1+P2(+P3), and copies per call today | per corpus read call: `clone_standing_for_self` calls; the test outcome |
| U2 | does a tree build write anything (trims, tolerances, implants) when every trim exists (report 07 §8.3) | trims made and tolerances changed during `build_tree_with_face` on bodies where `missing_uv_trim` is `None` |
| U3 | does a copy-built cache in the shared slot (C2) change a later answer on the real body | corpus: read then edit-door query, against the edit-door query alone; compare answers; count slot stores made from copies |
| U4 | mid-run refusals in the read form after the test passed | `PreparationRequired` count per call (expect 0) |
| U5 | how often gap caches or tolerances are written through `with_face_entity_mut` during classification | writes per call on corpus reads |
| U6 | bit-identity of the read form against the live form for classifications and rays | #21107 probe (c) pattern: N points and rays per corpus body, both ways |
| U7 | cost of the per-call `BrepCache` build plus lends, against the copy | time and count per `classify_point` on large bodies |
| U8 | whether `calculate_an_internal_point` needs trims beyond the face's own | trims made per `inside_points` call on prepared bodies |
| U9 | whether `fire_ray` on the kept table changes answers | corpus rays, both ways; copies or lends per call before and after |
