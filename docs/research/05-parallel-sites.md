# 05 — Parallel sites in kernel operations

Checkout: `sc-wt/cache-arch` (main, 782ef9afc). Read-only investigation: nothing was built or run.

**Path convention.** `K/` stands for `solidcore/crates/solidcore/src/`. Other crates are written from `solidcore/crates/` (e.g. `solidcore-tess/src/par.rs`). Line numbers are those of this checkout.

**Verdicts.** READY = already pure, only signatures or `!Sync` types block it. CACHE-BLOCKED = pure once trims and caches are prebuilt and immutable. ORDER-COUPLED = an iteration's result depends on what earlier iterations wrote; needs an algorithm change. SEQUENTIAL = marching or graph mutation.

**UNVERIFIED** marks anything inferred from signatures, comments or structure and not confirmed by reading the code path end to end.

---

## 0. Headline findings

1. **The Boolean's face-pair loop is already split into gather and apply**, but its gather phase is not read-only. `intersect_topology` first collects every pair's curves and classifications into a `Vec` (`K/boolean/topology_intersector/p2.rs:1612-1722`) and only then builds proto topology from them (`:1738-1812`). The gather still takes `&mut` on both bodies, and makes four kinds of write: lazy UV trims, gap memos, trimmed surface caches, and real edits (edge tolerance raises, vertex tolerance raises, edge curve rewrites).
2. **Tolerance raising during classification is the main order coupling.** The module doc states it outright: "Every caller of `CurveOnClassify` ... can so raise an edge's tolerance ... later intervals on that edge see the raised one" (`K/query/face/p2.rs:28-40`). Prebuilding caches does not remove this; it needs a rule change (see §3).
3. **Lazy UV trim creation raises edge tolerances** (`K/topology/edgeuse/p3.rs:1756-1772`), and trim creation itself fetches the face's trimmed surface cache, which may attach another trim (`:1746-1755`). So "all trims exist" is a real precondition, and the order they are built in fixes the tolerances.
4. **Three purity seams already exist** and should be copied rather than reinvented: `PreparationRequired` (read-only solve asks its mutable caller to prepare, then restarts), `LazyBrep` / `SnapshotObjects` (shared body, private copy on first lazy write), and the `ReadOnly` classification adapter (every edit defers).
5. **The existing threaded code does not touch the kernel `Brep` or `Kernel` at all.** Hidden lines (`K/hlr.rs`) and the mesher (`solidcore-tess`) both run on the portable record and the `eval` evaluator. They are good models for the fork-join shape and ordered results, but they say nothing about making `Brep`/`Kernel` shareable.
6. **Geometry values are closer to shareable than the `Kernel` is.** Curve and surface memos are `OnceLock`/`Arc<Mutex>` (`K/curve/curve/p5.rs:479`, `K/surface/surface/p8.rs:394`, `K/nurbs/b_spline_surface/p1.rs:461`). The hazard on surfaces is two `AtomicBool` evaluation-mode flags toggled through `&` (`K/nurbs/b_spline_surface/p1.rs:430-437`).

---

## 1. Site table

| # | Site | Loop (file:line) | Unit | Verdict |
|---|---|---|---|---|
| B1 | Boolean: face boxes | `K/boolean/topology_intersector/p2.rs:1583-1600` | per face | READY |
| B2 | Boolean: face-pair intersect + classify | `K/boolean/topology_intersector/p2.rs:1612-1722`; body `K/topology/face/p3.rs:2655-3031` | per face pair | ORDER-COUPLED (rare paths) / CACHE-BLOCKED (common path) |
| B3 | Boolean: strip rules across pairs | `p2.rs:1727-1737` | whole list | SEQUENTIAL (cheap) |
| B4 | Boolean: proto edges / vertices from the gathered list | `p2.rs:1738-1812` | per pair, per interval | ORDER-COUPLED |
| B5 | Boolean: split intersecting proto edges | `p2.rs:1828-1848`; `K/topology/proto_topology/p3.rs:1197` | per proto-edge pair, O(n²) | ORDER-COUPLED |
| B6 | Boolean: drop edge-less proto vertices on proto edges | `p2.rs:1852-1871` | per PV × PE | ORDER-COUPLED |
| B7 | Boolean: resolve, imprint, relate | `p2.rs:1443-1487` | — | SEQUENTIAL |
| B8 | Boolean: classify common-edge faces, with restart | `K/boolean/merge/p4.rs:2046-2123` | per edge pair | SEQUENTIAL |
| B9 | Boolean: classify remaining face groups by point-in-body | `K/boolean/merge/p4.rs:1429-1506` | per face group | CACHE-BLOCKED |
| S1 | SSI: tree builds for the two caches | `K/intersect/surface_intersector/p5.rs:559-576` | per surface (2) | CACHE-BLOCKED |
| S2 | SSI: boundary start points | `K/intersect/surface_intersector/p3.rs:473-560` | 2 surfaces × 4 iso curves | ORDER-COUPLED (dedup on insert) UNVERIFIED |
| S3 | SSI: trace curves; interior curves from node pairs | `K/intersect/surface_intersector/p4.rs:774-880` | per start point / node pair | SEQUENTIAL |
| C1 | Face point classify (`PointClassify`) | `K/query/brep/ray/live.rs:734-776` | per point | CACHE-BLOCKED |
| C2 | Curve-on-face classify (`CurveOnClassify`) | `K/query/face/p2.rs:2196-2251` | per curve, per interval | ORDER-COUPLED |
| C3 | Body point classify by ray fire | `K/query/brep.rs:843-990` | per point | CACHE-BLOCKED |
| M1 | Mass properties per face | `K/mass/integrator/properties.rs:2063-2168` | per face (× 2 uses) | CACHE-BLOCKED |
| M2 | Mass: wire edges | `properties.rs:2169-2184` | per edge | READY |
| F1 | Fillet: solver geometry | `K/blend/fillet_executive/p3.rs:3223-3235` | per blend edge (solver) | ORDER-COUPLED (latch, memos) UNVERIFIED |
| F2 | Fillet: rail classification | `p3.rs:3784-3795` | per solver | ORDER-COUPLED |
| F3 | Fillet: corners, faces, stitch, merge | `p3.rs:3620`, `:3837`, `:3850` | per corner / solver | SEQUENTIAL |
| O1 | Offset: per-face offset surface | `K/offset/offset_executive/p2.rs:822-830` | per face | ORDER-COUPLED as written; geometry part CACHE-BLOCKED UNVERIFIED |
| O2 | Offset: per-edge-use blends, side walls, vertex caps | `p2.rs:854-921` | per edge use / vertex | ORDER-COUPLED UNVERIFIED |
| O3 | Offset: piecewise merge + stitch | `p2.rs:936-938` | — | SEQUENTIAL |
| H1 | Stitch / heal | `K/heal/stitch/p1.rs:1343`, `:1942`, `:2267` | per vertex / edge pair | SEQUENTIAL |
| V1 | Validity: tolerance pass | `K/topology/brep/validation/assert_valid/brep.rs:319-460` | per edge use | CACHE-BLOCKED when validate-only; ORDER-COUPLED when updating |
| V2 | Validity: per-entity checks | `brep.rs:598-619` | per shell / face / loop / edge / vertex | CACHE-BLOCKED |
| Q1 | First face hit by a ray | `K/query/brep.rs:764-811` | per ray | CACHE-BLOCKED |
| Q2 | Closest distances, faces × points | `K/query/api_queries.rs:1274-1327` | per face, then per point | READY (already gather/apply) |
| Q3 | Body-body contact bracket | `K/query/contact.rs:649-728` | per piece pair | ORDER-COUPLED (shared bound) UNVERIFIED |
| X1 | BrepCache `build_trees` | `K/topology/brep_cache.rs:156-270` | per vertex / edge / face | boxes READY; insertion SEQUENTIAL |
| X2 | Surface cache tree build | `K/surface/surface_cache/p3.rs:1143-1146`, `:845-946` | per first-generation patch; recursive | ORDER-COUPLED (one flag, node ids) |
| X3 | Curve cache tree build | `K/curve/curve_cache.rs:921-990` | per span | READY (cheap) |
| T1 | Mesher | `solidcore-tess/src/mesher/mod.rs:328`, `:600`, `:621`, `:660`, `:959` | per face / per edge | already parallel |
| T2 | Hidden lines | `K/hlr.rs:1106`, `:1235`, `:1249`, `:1256`, `:2421`, `:2573`, `:2731` | per track / row / pair | already parallel |
| R1 | Kernel → record | `solidcore-convert/src/brep/record.rs:130-260` | per edge use (trims), per edge, per face | trims SEQUENTIAL; rest READY UNVERIFIED |
| R2 | Record → kernel | `solidcore-convert/src/brep.rs:321-637` | per edge / face | READY UNVERIFIED |
| P1 | Sweep / prism / primitive | `K/sweep/`, `K/prism/`, `K/primitive/` | — | SEQUENTIAL (construction); not examined in depth |

---

## 2. Per-site detail

### B1 — Boolean face boxes

- **Loop:** `p2.rs:1583-1600`, called for both operands at `:1601`.
- **Reads:** the body through `&self` (`face_calculate_bounding_box(&self, k, f, false)`, trait at `K/boolean/topology_intersector/p1.rs:623`); the face's vertex and edge lists.
- **Writes:** none on the body. The underlying call goes through `LazyBrep` (`K/query/face/p1.rs:580-597`), so a lazy write would land on a private copy. May fill curve cache memos, which are `Sync`.
- **Order dependence:** none. An uncomputable box becomes an unbounded box (`:1591-1595`), per face.
- **Gather / apply:** `Vec<Extent3d>` in face order; then `SpatialIndex::new` (`:1606-1609`).
- **Blockers:** `&Kernel` is `!Sync`; cancellation poll per face (`:1586`).

### B2 — Boolean face-pair loop (the main site)

- **Loop:** outer over `faces1` at `p2.rs:1612`, inner over box-search candidates at `:1618`. Counter: `work_counters.add_face_pair()` at `:1625`.
- **Item count and skew:** up to n1 × n2 before the box filter; the spatial index (`:1606-1609`) and the disjoint-box test (`:1645-1649`) cut it to near-neighbours. Cost is very skewed: most surviving pairs end at "no curve or point" (`:1695-1697`). Coincident pairs classify every edge of both faces (`K/topology/face/p3.rs:2718-2752`); the comment there records the old n·m behaviour.
- **Reads:** both bodies; each face's surface, UV domain and tolerance (`face/p3.rs:2686-2688`); edge curves and intervals; the intersection table (`check_if_have_existing_ssi`, `p2.rs:1636`); kernel tolerances and `k.fixes`.
- **How it gets caches:** the adapter keeps a `FaceIntersectCaches` per operand (`K/boolean/merge/p4/arena_pair.rs:1606-1632`). Each call takes the per-face `BTreeMap<FaceId, SurfaceCache>` out of its slot (`face/p3.rs:2780-2784`), runs the SSI, and puts it back (`:2804-2809`).
- **Writes during the read phase:**
  1. Trimmed surface caches, built on first use on the body (`K/query/brep/ray/live.rs:665-696`). Building one creates the face's UV trims.
  2. UV trim curves, and with them edge tolerance raises (`K/topology/edgeuse/p3.rs:1756-1772`).
  3. Vertex/face and edge/face gap memos on vertex uses and edge uses (`K/topology/vertexuse.rs:715-732`).
  4. Edge tolerance raised by a curved interval near an edge (`K/query/face/p2.rs:28-40`).
  5. Vertex tolerance raised to the edge's zone (`K/intersect/curve_class/p5.rs:2015-2019`).
  6. Large-gap refinement: edge curve rewritten and its UV trims deleted (`K/intersect/curve_class/p2.rs:993-1002`, `:1410-1414`).
  7. The seam memo `seams: BTreeMap<FaceId, Option<bool>>` (`face/p3.rs:2595`, `:2621-2626`).
  8. New curve handles registered in the adapter's store (`arena_pair.rs:1647-1651`).
  9. Kernel: work counters, cancellation polls. No RNG draw was found on this path.
- **Order dependence:** yes, in three ways.
  - Writes 2, 4, 5 and 6 change what a later pair on the same edge or vertex reads. Write 6 sets `objects.edited`, which clears every kept face cache (`face/p3.rs:2990-2998`); the code itself treats this as "the B-rep changed".
  - Writes 1, 3 and 7 are memo fills of deterministic functions of the body, so they are order-free once 2, 4, 5, 6 are dealt with.
  - Write 8 fixes handle numbers by visit order.
- **Gather result:** `(f1, f2, are_coincident, FaceIntersection, int_pts)`, exactly the tuple pushed at `p2.rs:1721`, with curves as owned values rather than store handles.
- **Apply:** register the curves in pair order (fixes handle numbering), extend `deleted` (`:1687-1693`), then B3 and B4.
- **Verdict:** CACHE-BLOCKED for a body whose trims exist and whose tolerances already cover every gap; ORDER-COUPLED for writes 4, 5, 6. See §3.

### B3 — Strip rules

`drop_strip_crossings` and `strip_contacts` (`p2.rs:1729-1736`) read the whole gathered list and declare contacts before any proto vertex is made. This is the reason the loop was split in the first place (comment at `:1609-1611`). Sequential, after the join.

### B4 — Proto topology from the gathered list

- **Loop:** `p2.rs:1738-1812`.
- **Writes:** `find_or_create_proto_edge` (`:1762`) and `find_or_create_proto_vertex_pcs` (`:1798`) insert into the manager's lists, deduplicating by tolerance against everything inserted so far (`K/topology/proto_topology/p1/proto.rs:4218-4263`, the nearest contained proto vertex wins).
- **Order dependence:** yes, by construction: find-or-create against a growing list.
- **Embedded pure work:** the single-point classifications at `:1787-1788` (two `face_point_3d_classify` per point) are C1 calls and could be gathered with B2.
- **Verdict:** ORDER-COUPLED. Keep sequential, in pair order.

### B5 — Split intersecting proto edges

- **Loop:** `p2.rs:1828-1848`; the list is re-read after each split because splitting appends.
- **Counter:** `proto_edge_intersections` (`K/topology/proto_topology/p3.rs:1264`).
- **Skew:** the comment at `p3.rs:1254-1258` says that with long outlines nearly every pair ends at the line-box test.
- **Order dependence:** yes. The early exits read state that earlier splits changed: shared end proto vertices (`:1218-1224`), the overlap list (`:1238`), and the current interval (`:1287-1291`).
- **Possible gather:** the curve/curve intersection itself (`:1265-1272`) reads only two curves, two intervals and a tolerance. A split shortens an interval but keeps the curve, so solutions for the original pair could be computed in parallel and filtered by the current interval at apply time. UNVERIFIED that this reproduces the sequential result bit for bit.
- **Verdict:** ORDER-COUPLED.

### B9 — Classify remaining faces

- **Loop:** `K/boolean/merge/p4.rs:1429-1506`.
- **Unit:** one representative face per unmarked connected group. `collect_faces` marks the group as it goes (`:1446-1455`), so choosing representatives is sequential.
- **Reads:** a point on an edge of the face (`:1477-1481`), then `classify_point_editing_disabled` on the opposite body (`:1483-1490`), which is C3.
- **Writes:** the opposite body is `&mut` only for the lazy writes of C3; the comment says "The pass only reads `opposite`, so one query (and its cache) serves every face" (`:1426-1427`).
- **Gather / apply:** gather the region per representative; apply the keep/delete decision in face order.
- **Verdict:** CACHE-BLOCKED. Item count is small (one per face group), each a ray fire.

### S1-S3 — Surface/surface intersection internals

- **Driver:** `do_intersection_in`, `K/intersect/surface_intersector/p5.rs:530`.
- **Phases:** get both trees (`:559-576`); set per-cache flags `point_test_enabled` and `process_boundary_curves` (`:581-582`); root box test (`:611-616`); tree-pair candidate search `solve_it` (`:632-640`); boundary start points (`p3.rs:462`); traces; interior curves from node pairs (`p4.rs:774`).
- **Writes on the cache during a solve:** the two flags above are set on the cache itself, so a cache shared between two concurrent SSIs would be written by both. Lazily computed `patch_boundary_bounding_planes` (`:586-590`).
- **Order dependence inside one SSI:** interior node pairs are skipped when near a curve already found (`p4.rs:788-815`) and a start point is dropped when on a found curve (`:839-841`). Tracing is marching. This is sequential by nature.
- **Verdict:** parallelise across face pairs (B2), not inside one SSI. S1 is the tree build (X2).

### C1 — Face point classification

- **Entry:** `BrepFaceServices::classify`, `K/query/brep/ray/live.rs:734-776`, calling `point_classify_live` with the kept cache for the face.
- **Reads:** the face's trimmed cache tree, its loops and trims.
- **Writes:** takes the body mutably through a `RefCell` (`:748`); lazy trims and gap memos.
- **Order dependence:** none once trims and gaps exist.
- **Verdict:** CACHE-BLOCKED.

### C2 — Curve-on-face classification

- **Entry:** `curve_on_classify_mut_with_seams`, `K/query/face/p2.rs:2251`.
- **Three flavours already exist:** on the face's own body (`curve_on_classify_mut`, `:2196`); on a private snapshot copy (`curve_on_classify_cached`, `:2163-2183`, counted by `classification_snapshots`); and the `ReadOnly` adapter where every edit returns `Unported` (`K/intersect/curve_class/p5.rs:153-190`).
- **Order dependence:** yes; see B2 writes 4, 5, 6.
- **Verdict:** ORDER-COUPLED.

### C3 — Body point classification

- **Entry:** `BrepQuery::classify`, `K/query/brep.rs:843`.
- **Reads:** the `BrepCache` (a `OnceCell`, `:590`, built on first use at `:705-712`), the body box (`OnceCell`, `:594`), kept face caches (`KeptFaceCaches`, a `Cell`, `K/query/brep/ray/live.rs:149-153`).
- **Writes:** lazy trims on the body; the three cells above.
- **Order dependence:** the ray directions are a fixed table (`CLASSIFY_RAYS`, `:971-979`), no RNG. First successful ray wins, per point. None across points once prepared.
- **Verdict:** CACHE-BLOCKED. `OnceCell` and `Cell` make `BrepQuery` `!Sync`.

### M1 — Mass properties per face

- **Loop:** `K/mass/integrator/properties.rs:2063-2168`. Counter: `mass_iso_evaluations` (`K/topology/face/p2.rs:2013`), once per integrand evaluation.
- **Reads:** face attributes, region voidness, then `face_properties_in` (`:622-669`).
- **Writes:** `boundary(k, b, f)` takes `&mut Brep` to make the oriented trim boundary (`:272-290`), so lazy trims. `Unclamped` toggles two `AtomicBool` flags on the face's own surface for the duration (`:447-487`, `:651-655`).
- **Pure core:** `boundary_properties(k, surface, &trims, &uv_domain, &fbox, ...)` (`:656-668`) takes no body.
- **Order dependence:** none between faces. The total is a running `f64` sum (`add`, `:1413-1426`), so the fold must stay in face order.
- **Gather / apply:** per face, the `PreciseProperties` deltas (one or two uses) plus the optional closure; apply `add` / `add_selected` in face order. The optional `terms` vector (`:2057`, `record`) already is the list of summands.
- **Verdict:** CACHE-BLOCKED. Likely the cleanest large win.

### F1, F2 — Fillet

- **Loops:** `calculate_solver_geometry` (`K/blend/fillet_executive/p3.rs:3223-3235`), `classify_solver_rails_with_adjusted` (`:3784-3795`).
- **Writes:** each iteration takes `&mut Kernel` and `&mut self`. Kernel state on this path: the result-changing latch `fillet_point_step` (`K/kernel.rs:820-827`, "the first finite-difference step ... latches it; every later evaluation reuses the latched value"), `blend_station_memo`, `blend_w_memo`, `blend_net_tolerance`, `blend_counts`. Failures are recorded on the executive (`:3226-3231`). Classification appends to a shared `adjusted` list (`:3786`).
- **Order dependence:** the latch is first-wins and changes results. Whether one solver's geometry reads another's is UNVERIFIED.
- **Verdict:** ORDER-COUPLED until the latch is decided before the fork.

### O1, O2 — Offset / shell

- **Loops:** per face at `K/offset/offset_executive/p2.rs:822-830`; per edge use at `:854-900`; per vertex at `:907-921`.
- **Order dependence:** `loop_points` "keeps the last copied shell face's vertex-loop points ... the vertex caps reuse it below" (`:819-821`), so a later phase reads what the last iteration left. Each iteration both makes geometry and adds topology to the offset body. An extended-surface memo keyed by caller key lives on the executive (`K/offset/offset_executive/p1.rs:215`, `:892-934`).
- **Verdict:** ORDER-COUPLED as written. Splitting "make the offset surface" from "insert the face" would make the first half a per-face gather. UNVERIFIED.

### V1, V2 — Validity

- **V1:** `validate_and_update` walks each face's edge uses (`brep.rs:319-460`), reading vertex/edge, vertex/face and edge/face gaps. With `validate_only` it writes only gap memos. Otherwise it grows vertex and edge tolerances as it goes (doc at `K/topology/brep/validation/assert_valid.rs:472-481`), which later edge uses read.
- **V2:** `sub_topology` (`brep.rs:582-621`) runs one check per entity and ANDs the result; reports are appended to one `AssertArray` "in native order". A shared `EdgeSets` memo sits in the context (`K/topology/brep/validation/assert_valid/edge.rs:169-174`). The comment at `brep.rs:583-584` says a large body's check "runs for seconds, the edges' curve analyses most of it".
- **Kernel memos hit:** `curve_analysis` (`K/kernel.rs:142`), `loop_area_memo`.
- **Gather / apply:** per entity `(ok, Vec<report>)`; concatenate in entity order.
- **Verdict:** V2 CACHE-BLOCKED; V1 CACHE-BLOCKED when validate-only, ORDER-COUPLED when updating.

### Q1-Q3 — Ray and distance queries

- **Q1** `first_face_hit` (`K/query/brep.rs:764-811`): one `solve_mutable_brep_point_kept` per ray; ties broken by "the solve's first" (`:789-797`). CACHE-BLOCKED. No batch entry point exists in the kernel.
- **Q2** `faces_closest_distances_within` (`K/query/api_queries.rs:1274-1327`): makes each face's box and shape first "while the B-rep can be read" (`:1290-1305`), then solves per (face, point) into a row. Already gather/apply. READY, apart from `&mut Kernel` in the signature and `LiveBrepObjects`.
- **Q3** `contact::bracket` (`K/query/contact.rs:649-728`): a shared `upper` bound is tightened as pieces are visited (`:585-590`), so pruning depends on visit order. UNVERIFIED whether the final bracket is order-invariant.

### X1-X3 — see §D.

### T1 — Mesher (already parallel)

- **What it extracts from the kernel:** nothing from `Brep` or `Kernel`. It takes a schema-4 record (`solidcore_record::brep::Brep`, `solidcore-tess/src/mesher/mod.rs:72`) and builds one `EvalSurface` per face (`:589-598`), the portable evaluator's form (`solidcore::eval::plan`).
- **Parallel phases:** face sizes (`:600`), edge nodes (`:621`), face boundaries (`:660`), boundary crossings (`:959`), face tessellation (`:328`).
- **Shape:** every phase is `par::map(what, &items, parallel, f) -> Vec<Result<R>>` in item order (`solidcore-tess/src/par.rs:72-93`). Results are folded sequentially in item order straight after (`mod.rs:623-641`, `:666-679`).
- **Fixpoint rounds:** face tessellation runs in rounds (`:326-360`): plan all faces in parallel, sequentially collect the edge splits they ask for into a `BTreeMap`, re-plan the affected faces.
- **Thresholds:** parallel only above a count of curved faces (`PARALLEL_FACES`, `PARALLEL_SIZES`, `PARALLEL_CURVES`, `PARALLEL_EDGES`).

### T2 — Hidden lines (already parallel)

- **How it prepares shareable data:** `prepare_cancellable` (`K/hlr.rs:282`) converts the portable body (`hlr::body::Brep`, an `eval` record) into `Prepared` (`:180-190`): plain `Vec<EdgeData>`, `Vec<FaceData>`, a box and an offset. It is immutable afterwards and shared by `&[Prepared]`.
- **Shared context:** `Ctx` (`:814-840`) holds only `&` data plus `AtomicUsize` totals and an `AtomicBool` stop flag.
- **Fork primitive:** `Ctx::map` (`:847-868`) returns results in item order; `map_on_threads` (`:873-949`) hands out indices from an atomic counter and writes each result to its slot.
- **No threads started by the kernel:** the caller passes a `Workers` (`:661-666`); `Serial` runs on the calling thread.
- **Budget:** `rays` is a total over all workers (`:823-826`).

### R1, R2 — Record conversion

- **R1** `to_record` (`solidcore-convert/src/brep/record.rs:130`): its first loop gives every edge use on a face its UV trim, in `brep::p2::edgeuses` order (`:131-136`). This is already a "make every trim" pass with a fixed order. The per-edge (`:168`) and per-face (`:188`) conversions then read the body's data arrays. UNVERIFIED that they are pure.
- **R2** `to_kernel` → `data` (`solidcore-convert/src/brep.rs:321-637`): per-edge and per-face geometry conversion, then `build`. Kernel memos on this path: `recertify_memo` (`K/surface/procedural_blend/convert.rs:790-793`). UNVERIFIED in detail.
- **STEP** (`solidcore-step`): not examined beyond file sizes; it produces records, not kernel bodies.

---

## 3. The ORDER-COUPLED items and what each would take

| Item | Coupling (code) | What a redesign needs |
|---|---|---|
| Edge tolerance raised by a curved interval near an edge | `K/query/face/p2.rs:28-40` | Gather must not raise. Return "wants raise (edge, distance)" as part of the value; apply the max per edge after the join; re-run only pairs touching a raised edge. Changes results versus today's order. |
| Vertex tolerance raised to edge zone | `K/intersect/curve_class/p5.rs:2015-2019`, gated by `ok_to_set_tolerance` | Same proposal pattern, or settle vertex ≥ edge tolerance in the prepare step so the branch never fires. |
| Large-gap refinement rewrites an edge curve and deletes its trims | `K/intersect/curve_class/p2.rs:993-1002`, `:1410-1414`; flagged at `K/topology/face/p3.rs:2990-2998` | Treat as an escape: the iteration returns "needs edit", and the pair is redone sequentially after the join (the `PreparationRequired` shape). |
| Lazy UV trim raises edge tolerance | `K/topology/edgeuse/p3.rs:1756-1772` | Build all trims before the fork in a fixed order. |
| Proto vertex / edge find-or-create | `K/topology/proto_topology/p1/proto.rs:4218-4263`; `K/boolean/topology_intersector/p2.rs:1762`, `:1798` | Keep sequential in pair order. |
| Proto edge splitting | `K/topology/proto_topology/p3.rs:1218-1291` | Keep sequential; optionally precompute raw curve/curve solutions. |
| SSI interior curves and start-point dedup | `K/intersect/surface_intersector/p4.rs:788-841` | None; sequential inside one SSI. |
| `switch_to_fast_subdivision` in tree build | `K/surface/surface_cache/p3.rs:903-910` | Make the choice a function of the node, not of visit order (see §D). |
| `fillet_point_step` latch | `K/kernel.rs:820-827`, `:1120-1134` | Latch before the fork. |
| Offset `loop_points` carry-over | `K/offset/offset_executive/p2.rs:819-830` | Split geometry from insertion. |
| Validity tolerance update | `K/topology/brep/validation/assert_valid.rs:472-481` | Parallel only in validate-only mode. |
| Ray fallback restarts | `K/query/brep/ray.rs:11-14`: "can revisit earlier candidates with raised tolerances" | Disappears on a prepared body. |

---

## A. Preconditions: what a "prepared" body means

| # | Precondition | Why (code) | Sites |
|---|---|---|---|
| A1 | Every edge use on a face has its UV trim (or a recorded failure) | `get_or_create_uv_trim_curve` writes the body and raises tolerance, `K/topology/edgeuse/p3.rs:1690-1772`; failed trims are tracked at `K/query/brep/ray.rs:57`, `:113-116` | B2, B9, C1, C2, C3, M1, V1, V2, Q1, R1 |
| A2 | Edge and vertex tolerances are final: each covers its trims and gaps, and vertex ≥ edge zone | raises at `edgeuse/p3.rs:1764`, `K/query/face/p2.rs:28-40`, `K/intersect/curve_class/p5.rs:2015-2019` | B2, C2, V1 |
| A3 | Vertex/edge, vertex/face and edge/face gap memos are filled | `K/topology/vertexuse.rs:715-732`; `K/topology/edgeuse/p1.rs:906` | B2, C2, V1 |
| A4 | Every face's trimmed surface cache is built with its tree (`get_tree`), boundary implanted | `face_surface_cache(.., get_tree = true)`, `K/query/brep/ray.rs:172-246`; state-only caches without a tree also exist (`K/query/brep/ray/live.rs:700-730`) | B2, B9, C1, C3, Q1, S1 |
| A5 | Surface NURB forms made for analytic surfaces | `revolution_base_with_nurb`, `base_with_nurb`, `K/surface/surface_cache/p3.rs:1012-1034`; `Arc<OnceLock>` slot at `K/nurbs/b_spline_surface/p1.rs:461` | X2, M1, B2 |
| A6 | `BrepCache` built (three spatial trees) and body box known | `K/query/brep.rs:590-594`, `:705-712` | B9, C3, Q1, Q2 |
| A7 | Per-face seam flag known | `K/topology/face/p3.rs:2595`, `:2621-2626` | B2 |
| A8 | Per-face boxes computed | `K/boolean/topology_intersector/p2.rs:1583-1601` | B2, Q2 |
| A9 | Curve cache for each edge curve built | memo in the curve value, `K/curve/curve/p5.rs:466-479` | B5, S3, X1 |
| A10 | Not mid-edit: editing disabled and loops closed | `K/query/brep/ray.rs:193-197`; `classify_point_editing_disabled`, `K/boolean/merge/p1.rs:456-470` | C1, C3, B9 |
| A11 | Surface evaluation-mode flags at rest (`out_of_bounds_enabled`, `continues_past_domain`) | `K/nurbs/b_spline_surface/p1.rs:430-437` | M1 and every evaluator |
| A12 | Kernel latches decided and scopes entered before the fork (`fillet_point_step`, `blend_net_tolerance`) | `K/kernel.rs:820-827`, `:869-877` | F1, X2 |

Notes:

- A1 and A4 are entangled. Building a trim fetches the trimmed cache, which may attach another trim to the same edge use (`edgeuse/p3.rs:1746-1755`). The prepare step must run to a fixpoint, not one pass.
- A4 depends on the caller. In a Boolean the cache factory may defer the tree (`K/query/brep/ray.rs:176-180`); the cache built while editing differs (WR-732, `:182-189`). "Prepared" should mean "built outside editing".
- A2 cannot be fully prebuilt for the Boolean: some raises depend on the other operand's curves. That is the residue handled in §3.

---

## B. Determinism hazards

**Floating-point reductions**

- Mass totals: a running sum over faces, edges, vertices (`K/mass/integrator/properties.rs:1413-1426`, `:2137-2163`). Must fold in entity order.
- Tree boxes: parent box = union of children (`K/surface/surface_cache/p3.rs:930-934`). Union is min/max, so order-free.
- Contact bracket bounds (`K/query/contact.rs:585-590`): min over candidates is order-free, but which witness is kept on ties is not. UNVERIFIED.

**HashMap / HashSet in non-test kernel code** (a grep of `K/` found three)

- `LoopAreaMemo.signs` (`K/mass/loop_area.rs:228`): lookup by a key holding every input bit (`:247-259`). No iteration.
- `RecertifyMemo.bounds` (`K/surface/procedural_blend/convert.rs:792`): lookup by full text key. No iteration. Clears itself past a size cap (`:813-818`), which changes speed, not results.
- `owner: HashMap<VertexId, usize>` (`K/prism/sheet.rs:118`): `entry().or_insert()` only (`:168`).
- So iteration order reaches no result in the kernel crate. Sibling crates (`solidcore-tess`, `-step`, `-record`, `-convert`, `-ops`) do use hash maps; whether any is iterated into output is UNVERIFIED.

**RNG draws**

- `IsInnerShellOf` draws ray directions for wire and vertex shells (`K/topology/shell.rs:1715-1731`).
- `points_in_face` draws a V dither (`K/topology/face/p2.rs:1653-1658`); callers are the shell test (`K/topology/shell.rs:1683`), the traverser (`K/topology/topology_traverser.rs:748`) and feature recognition (`K/blend/feature_executive.rs:2002`, `:2027`).
- None of these is inside a candidate parallel loop in §1; all are in region nesting or sequential phases. Body point classification uses a fixed direction table instead.
- Kernel copies clone the RNG state (`K/boolean/merge/p4/kernel_adapter.rs:63`; `K/topology/edgeuse/p3.rs:1198`). A per-worker kernel copy would therefore draw the same numbers on every worker; harmless only while workers draw none.

**First-wins memo fills**

- Result-changing latch: `fillet_point_step` (`K/kernel.rs:820-837`). Two workers could latch different values. Already has a process-wide `OnceLock` variant for kernel pools (`:829-837`), which is first-wins by time.
- Station memo and thumbweight memo (`K/kernel.rs:880-900`): documented as "a hit returns what a fresh solve from the same seeds gives". Whether the key includes every input is for the cache inventory; UNVERIFIED here.
- `CurveMemo` holds one entry and replaces it when the key differs (`K/curve/curve/p5.rs:531-538`): two workers asking with different keys would thrash, not diverge.
- `BoundaryMemo` is fill-once by key (`K/surface/surface/p8.rs:394-420`).
- Trimmed cache per face: today one is built per face per operation. If two workers each built "the same" cache, the results agree only if nothing in A1-A3 changed between the builds.

**State toggled through shared references**

- `out_of_bounds_enabled`, `continues_past_domain` on `BSplineSurface` (`K/nurbs/b_spline_surface/p1.rs:430-437`), toggled in about 60 places (most in `K/surface/surface/p3.rs`, `p8.rs`, `K/blend/fillet_standard_solver/`). A worker that toggles a flag on a surface another worker is evaluating changes that worker's answer. Safe only when each item owns the surface it toggles, or toggles a clone (`clone_for_evaluation`, as `K/topology/face/p3.rs:2762-2763` does).
- `point_test_enabled`, `process_boundary_curves` on a `SurfaceCache` (`K/intersect/surface_intersector/p5.rs:581-582`).
- `blend_net_tolerance` on the kernel (`K/kernel.rs:869-877`), entered by `NetScope` in every tree build (`K/surface/surface_cache/p3.rs:868`).

**Identity and handle numbering**

- Geometry keys come from a shared counter (`K/geom/context.rs:412-424`); curve handles from the adapter's store (`K/boolean/merge/p4/arena_pair.rs:1647-1651`); tree node ids from `add_node` order. Anything created inside a worker must be numbered in the apply step.

**Cancellation**

- `check_cancelled` is polled inside every candidate loop (e.g. `p2.rs:1586`, `:1613`, `:1619`).
- A token can be set to fire at its n-th poll (`K/kernel.rs:691-710`). With workers, which piece of work is the n-th poll is not fixed, so tests using `after_polls` would see different partial states. The result is still `Cancelled` and never a partial `Ok` (`K/kernel.rs:853-858`).

**Error selection**

- Sequential code returns the first error in iteration order through `?`.
- The mesher keeps every item's `Result` in order (`solidcore-tess/src/par.rs:72-93`), so the lowest-index error can be chosen.
- Hidden lines returns the first failure by time (`K/hlr.rs:908-917`), which is not deterministic when two items fail.
- Several loops turn some errors into values and pass others on (`ignore_soft`, `is_unported_or_cancelled`, e.g. `p2.rs:1588-1596`). The per-item result must carry the error so the apply step can replay that logic in order.
- `note_upstream_ub` appends ids "in the order first met" (`K/kernel.rs:838-842`).

**Diagnostics that tests assert on**

- `WorkCounters` are `Cell<u64>` (`K/kernel.rs:181-205`) and tests assert exact counts (e.g. `K/topology/proto_topology/p1.rs:883-959`). Per-worker counts summed after the join keep the totals exact.

---

## C. Existing seams to copy

| Seam | Where | What it gives |
|---|---|---|
| Gather-then-apply in the Boolean | `K/boolean/topology_intersector/p2.rs:1609-1722` then `:1738-1812` | The fork point and the gather value already exist. |
| `PreparationRequired` + `RayPreparation` | `K/error.rs:335-340`; `K/query/brep/ray.rs:51-124` | A read-only solve that names the missing trim or cache, returns, and is restarted after its mutable caller prepares. Each request is attempted once (`:106-111`). |
| `LazyBrep` | `K/query/brep.rs:1062-1140` | A shared body with a private copy made at the first lazy write. |
| `SnapshotObjects` / `CurveOnClassifyCache` | `K/query/face/p2.rs:2163-2183` | Curve classification on a private copy; one snapshot serves many calls. |
| `ReadOnly` vs `Mutable` classification adapters | `K/intersect/curve_class/p5.rs:153-230` | Every edit is one of four named methods, so "what would this iteration write" is already enumerable. |
| `Reading` vs `Changing` sector computation | `K/topology/vertexuse.rs:762-790` | A read-only variant that gives up (`None`) instead of writing. |
| `topology_entity_for_query` vs `topology_entity_mut` | `K/intersect/curve_class/p5.rs:117-127`; `K/topology/face/p3.rs:2309-2323` | Lazy memo writes are already told apart from real edits; the `edited` flag records the latter. |
| `cache_kernel` | `K/boolean/merge/p4/kernel_adapter.rs:59-108` | A full recipe for a per-task kernel copy: which fields are copied, which start empty, which are shared. |
| `FaceIntersectCaches` take / put back | `K/topology/face/p3.rs:2780-2809`; `K/query/brep/ray/live.rs:143-153` | The current cache ownership pattern that a prepared, immutable cache would replace. |
| Mass `terms` | `K/mass/integrator/properties.rs:2036-2057` | The per-face summands as a list. |
| Closest distances | `K/query/api_queries.rs:1286-1327` | Shapes made first, then solves into rows. |
| Mesher `par::map` and rounds | `solidcore-tess/src/par.rs:72-93`; `solidcore-tess/src/mesher/mod.rs:326-360` | Ordered parallel map, per-item panic guard, 8 MiB worker stacks (`par.rs:7-10`), size thresholds, fixpoint rounds. |
| Hidden-line `Workers` | `K/hlr.rs:658-676`, `:847-949` | Threads injected by the caller, since library code starts none. |
| `to_record` trim pass | `solidcore-convert/src/brep/record.rs:131-136` | An existing "every use gets its trim" pass with a fixed order. |
| Validate-only tolerance pass | `K/topology/brep/validation/assert_valid.rs:482-506` | The gap computation without the tolerance writes. |

---

## D. Recursion and tree builds suited to `join`

### Surface cache tree (`K/surface/surface_cache/p3.rs`)

- **Structure:** `build_tree_base` makes one first-generation node per Bezier patch (`:1077-1140`), then subdivides each to tolerance (`:1143-1146`); `subdivide_to_tolerances_body` (`:879-963`) tests a node, splits it in two, recurses on both children, then unions their boxes.
- **Bounds:** depth ≤ `MAX_SUBDIVISION_DEPTH` = 512 (`:141`); total nodes ≤ `MAX_SUBDIVISION_NODES` = 2^22 on 64-bit (`:147-148`). The comment records 2,014,255 nodes upstream for one 1e-4-scale face. Cancellation is polled at every node (`:891`).
- **Is it a deterministic function of its input?** Nearly. Three things depend on visit order:
  1. `switch_to_fast_subdivision` (`:903-910`): set in the tolerance test (`K/surface/surface_cache/p2.rs:695`), consumed by the next node that needs splitting. It couples siblings.
  2. Node ids and patch slots are allocated in depth-first order (`add_node`); the node-count limit is checked against the running total (`:915`).
  3. The pole memo is shared for the outermost call (`:870-875`); it is a speed memo.
- **Fit for `join`:** the first-generation loop is a flat data-parallel site (one subtree per patch) and the recursion is a natural `join` on the two children. It needs subtrees built as separate values and spliced in a fixed order so ids match, the node budget split or checked at splice time, and item 1 made a function of the node.
- **Trimmed caches** add a boundary implant after the base tree (`get_tree_with_face`, `K/query/brep/ray.rs:240-244`); not examined. UNVERIFIED.
- **Stack:** the mesher gives its workers 8 MiB because "the kernel's evaluator and its projections recurse" (`solidcore-tess/src/par.rs:7-10`).

### BrepCache `build_trees` (`K/topology/brep_cache.rs:156-270`)

- Three spatial trees (vertices, edge curves, faces) over one padded body box (`:171-190`).
- Each entity's box is computed, then inserted with `add_to_spatial_tree` (`:236-270`, `K/geom/tree.rs:963`).
- Box computation per edge and per face is independent (gather). Insertion order shapes the tree, so it stays sequential "in upstream vertex/edge/face order" (`:153`). The three trees are independent of each other: a three-way `join`.
- Deterministic function of the body and the keys.

### Curve cache tree (`K/curve/curve_cache.rs:921-990`)

- Bottom-up: one leaf per span (box per span, independent), then parents pairwise by generation. No recursion. Deterministic. Cheap; unlikely to repay a fork.

### SSI tree-pair search

- `solve_it` (`K/intersect/surface_intersector/p5.rs:632-640`) walks two trees for candidate node pairs. A dual-tree descent is a `join` shape, but its output order feeds the order-coupled interior-curve pass (`p4.rs:788`), so results would need sorting to the sequential order. Not examined in depth. UNVERIFIED.

### Other

- `SpatialIndex` for face-pair search (`K/topology/brep/validation/spatial.rs:17`, used at `p2.rs:1606`): built once from boxes; not examined.
- Mesher and hidden-line structures are outside the kernel body model.

---

## E. Shortlist

Ranked by likely payoff against how little stands in the way.

| Rank | Site | Why it pays | Exact blockers |
|---|---|---|---|
| 1 | **M1 mass properties per face** | Every face is an independent adaptive integration; the pure core `boundary_properties` takes no body. | `boundary()` needs `&mut Brep` for trims (A1); `Unclamped` flag toggle on the surface (A11); `&Kernel` `!Sync` (counter `Cell`, cancel poll); sum must fold in face order. |
| 2 | **B2 Boolean face-pair gather** | The dominant Boolean phase; already separated from apply. | A1-A4, A7; the three real edits in §3 need the propose/escape rule; curve handle registration moves to apply; `FaceIntersectCaches` take/put-back must become shared immutable caches; SSI sets flags on the cache (`p5.rs:581-582`). |
| 3 | **X2 surface cache tree build** | Feeds B2, C1, C3, Q1 and the prepare step itself; up to millions of nodes per face. Two levels: across faces (flat) and within a face (`join`). | `switch_to_fast_subdivision` sibling coupling; node id / slot numbering; global node budget; `NetScope` on the kernel; A5. Across faces, also A1 (a trimmed tree needs the trims). |
| 4 | **Prepare pass itself (A1 + A4 for all faces)** | It is the precondition of everything else and is itself per-face work. | Trim creation raises edge tolerance and an edge is shared by two faces, so per-face trim building is coupled through the edge. Needs a fixed order per edge, or trims built in parallel and raises applied after. |
| 5 | **V2 validity per entity** | Runs "for seconds" on large bodies; per-entity checks with an ordered report list. | `&mut Brep` for lazy trims, gaps and face caches (A1, A3, A4); shared `EdgeSets`; kernel memos `curve_analysis` (`RefCell`) and `loop_area_memo`; reports concatenated in entity order. |
| 6 | **B1 / X1 boxes (face boxes, BrepCache boxes)** | Trivial to fork; needed by B2 and A6. | `&Kernel` only. Low payoff alone. |
| 7 | **B9 / C3 body point classification batches** | Each item is a full ray fire; also the shape of any future ray-batch API (Q1). | A1, A4, A6, A10; `BrepQuery` holds `OnceCell` and `Cell` (`K/query/brep.rs:590-594`; `live.rs:149-153`); representatives chosen sequentially. Few items per Boolean. |
| 8 | **Q2 closest distances faces × points** | Already gather/apply; rows are independent. | `&mut Kernel` in the signature; `LiveBrepObjects` holds the body for the pass. |
| 9 | **B5 proto-edge pair intersections** | O(n²) pair loop counted by `proto_edge_intersections`. | Order-coupled early exits; only the raw curve/curve solve can be gathered, and equivalence is unproven. |
| 10 | **F1 fillet solver geometry per edge** | Surface tracing per blend edge is heavy. | `&mut Kernel`; result-changing `fillet_point_step` latch; station and thumbweight memos; failure recording on the executive; independence of solvers unverified. |

Not on the list: SSI internals (S2, S3), imprint/merge (B7, B8), stitch and heal (H1), offset as written (O1, O2). These are sequential by nature or need restructuring first.

---

## What was not examined

- `solidcore-step` beyond file sizes; `solidcore-ssi`; `goldens`.
- `solidcore-ops` call patterns (only that a session owns one `Kernel`, `solidcore-ops/src/session.rs:210`).
- Generic blend, fillet corners, `K/heal/heal_data`, sweep/prism/primitive internals.
- Trimmed-cache boundary implant; the SSI dual-tree search; `contact::bracket` internals.
- Hash-map iteration in sibling crates.
