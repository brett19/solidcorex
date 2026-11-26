# 07 — The Boolean as gather / apply: what one operation's plan really costs

Read-only study on 782ef9afc (`sc-wt/cache-arch`). Nothing was built or run.
Paths are relative to `solidcore/crates/solidcore/src` unless they start with `OPS/` (`solidcore/crates/solidcore-ops/src`).

Evidence marks:
- **[V]** I read the cited lines myself.
- **[A]** a delegated reader read the cited lines and reported them; I did not re-read them. Treat as "probably right, verify before relying on it".
- **[I]** inference from what was read.

Short names used throughout: **pair phase** = the face-pair loop of `intersect_topology` (`boolean/topology_intersector/p2.rs:1612-1718`); **slot** = the trimmed cache kept on a surface (`Surface::take_trim_cache` / `store_trim_cache`); **face routine** = `create_uv_trim_curves` (`topology/face/p3.rs:1295-1480`); **edge-use routine** = `get_or_create_uv_trim_curve` (`topology/edgeuse/p3.rs:1690-1773`).

---

## 0. Findings that change the picture

1. **The pair phase is already a gather in its control flow, and already has the request/fulfil loop — twice.** The gather Vec exists (`p2.rs:1612`, pushed at `:1716`, consumed from `:1735`) [V]. `with_face_caches` (`boolean/topology_intersector/p1.rs:4705-4777`) is a complete "solve read-only, return a request, fulfil on the body, re-run" loop for face caches and UV trims [V]. The pair phase does not use it; it uses a second mechanism (`BrepFaceServices`, `query/brep/ray/live.rs:614-732`) that builds on the body inside the solve through a `RefCell<&mut Brep>` [V].
2. **Within one pair, each face's trimmed cache comes from two different homes that build different caches.** The surface intersection gets it from `FaceIntersectCaches` via `query/brep/ray.rs:197-248`, which with the WR-732 fix on implants the face's trims even though the body is editing-enabled (`ray.rs:211,221-223`) [V]. The classification of the resulting curves gets it from the slot via `query/face/p2.rs:429,2490`, whose build takes the editing branch `trim_srf_cache.rs:797-805` and is **standalone (untrimmed, no trim creation)** because `BrepCacheFace` does not implant while editing [V for the branch; A for "default false, set only by `MappedCacheFace::implanting_while_editing`"]. So "the face's cache" is not one thing during the pair phase today.
3. **Every cache fetch copies the cache.** `cache_mapped` returns `cache.map_handles(map)` for both a hit and a miss (`live.rs:679-698`) [V]: two full tree copies per face pair that reaches the surface intersection, on top of the builds.
4. **Trim creation is order-dependent across faces, not just "lazy".** The fit tolerance of a new trim is the edge's current tolerance (`edgeuse/p3.rs:498`), creation can change the shared edge's interval and re-trim the neighbouring face's existing trim (`:1010-1040`, `:1184-1214`), and the face routine's second loop moves trim ends and raises vertex tolerances even when every trim already existed (`face/p3.rs:1389-1468`) [A]. "Prepare before the loop" therefore gives a different body than today's lazy order on some inputs (section 4).
5. **Three writes in the pair phase are genuinely coupled (C)**: the curved-interval edge tolerance raise, the vertex-to-edge-zone raise, and the large-gap / surf-surf-surf refinement that rewrites an edge curve and deletes trims. Everything else is preparable or scratch (section 2).
6. **Inside one phase a cache table needs no key and no version** — provided the three C writes are turned into requests, because they are the only things that change a cache's inputs mid-phase (section 5).
7. **The Boolean has no rollback wrapper and no operand copy** on its main path. Operands are moved in; a failure leaves them consumed (section 1).

---

## 1. The Boolean as it is

### 1.1 Phase table

G = conceptually gather (pure computation over independent items). A = apply (topology edit). G* = gather-shaped but writes today.

| # | Phase | Where | Unit | Kind |
|---|---|---|---|---|
| 0a | ops entry: operands taken out of their bodies (moved, not copied); contract check (`is_manifold_solid`, or full `prove` under `Contract::Proved`) | `OPS/boolean.rs:153-164`, `OPS/body.rs:602` [A] | body | G |
| 0b | `matched_tolerances`: trial-copy the coarse operand, tighten its tolerances, full check, then apply to the real operand | `boolean/operand_tolerances.rs:57-88`, called `OPS/boolean.rs:161` [A] | body | G (shortest edge) + A (tolerance writes) |
| 1 | `glue_self_touching_edges` on each operand (snapshot + glue + `refresh_cached_topology`, only when a pair is proved coincident) | `boolean/api_brep.rs:585-586`, body `:399-555` [V call, A body] | edge bucket | G (find pairs) + A |
| 2 | Construct merge: tolerances (`min_edge_bbox_size` over every edge), 20° angle; attributes moved into the engine | `boolean/merge/p3.rs:298-327`; `merge/p2.rs:170-218` [V call, A body] | edge | G |
| 3 | Temporary transform when the operands sit far from the origin (in place, restored on exit) | `boolean/merge/p4.rs:2303-2335` [V]; `merge/p1.rs:1367` [A] | body | A |
| 4 | **Modes set**: `doing_boolean(true)` on the kernel context(s); `editing_enabled = true` on both operands | `merge/p4.rs:2380-2384` [V] | — | — |
| 5 | Adapter setup: `cache_kernels` forks two kernels; `ArenaPair::new` with empty `face_caches`, empty `BrepCache` cells | `merge/p4/kernel_adapter.rs:925-969`, `:59-115`; `merge/p4/arena_pair.rs:263-290` [V] | — | — |
| 6 | Pre-pair edits: `split_edges_at_vertices` both ways, `coincident_line_edges`, nearest-vertex distance, proto manager | `topology_intersector/p2.rs:1409-1440` [V] | vertex × line edge | G (drops) + A (edge splits) |
| 7 | Shell vertices and wire/lamina edges against the other body; vertices-on-edges become proto vertices | `p2.rs:1554-1576` [V] | vertex, edge | G* + A (proto) |
| 8 | **Broad phase**: face boxes (through `&self`), spatial index over operand 2 | `p2.rs:1582-1607` [V] | face | G |
| 9 | **Pair phase**: per candidate pair, existing-SSI lookup, box test, coincidence check, `face_face_intersect_curves`, parameters of the classifications; result pushed to `intersections` | `p2.rs:1612-1718` [V]; body `topology/face/p3.rs:2655-3031` [V] | face pair | **G*** |
| 10 | Strip rules over the whole list; declared contacts | `p2.rs:1720-1733` [V] | list | G (sequential, cheap) |
| 11 | Proto edges / vertices / faces from the list (find-or-create against a growing list; two face point classifications per single point) | `p2.rs:1735-1814` [V] | pair, interval | A (proto); embedded G at `:1784-1785` |
| 12 | Combine proto vertices; split intersecting proto edges (O(n²), list grows); drop edge-less proto vertices | `p2.rs:1823-1870` [V] | proto pair | A (proto), order-coupled |
| 13 | `resolve_intersections` (proto vertices, proto edges, overlap groups; `_mut` only for lazy trims/gaps/caches) | `p2.rs:1446`; `topology_intersector/p1.rs:2358` [A] | proto entity | A (proto) + G* |
| 14 | **Imprint**: vertex definitions, edge definitions (`make_edge_in_face`, face splits, inner-loop redistribution), vertex fit, glue coincident vertices, vertex cover | `p2.rs:1456-1484`; `p1.rs:2431-2651` [A] | definition | A |
| 15 | `set_relationships` (mate maps; raises both tolerances to the larger) | `p2.rs:1487`, body `:1893` [V call, A body] | definition | A |
| 16 | `editing_stages` round loop (≤10): `mark_common` (close intersection loops, `fix_gaps`: edits) | `merge/p4.rs:2046-2066`, `:1018-1124` [V] | common vertex | A |
| 17 | **Classification 1**: `classify_edge_faces` per common edge pair, both directions: radial sector, coincidence, `fix_face_coincidence` (edit + round restart), marks, flood fill | `p4.rs:2083-2120`, `:1128-1395` [V] | edge use | **G*** (sector) + A (marks, relate, fix) |
| 18 | `resolve_tangent_patches` | `p4.rs:2125`, `:509-605` [A] | patch | A (lists), cheap |
| 19 | **Classification 2**: `classify_remaining`: one `BrepQuery` per direction; per unmarked face group a body point classification with editing flipped off | `p4.rs:2126`, `:1397-1534`; `merge/p1.rs:456-470` [V] | face group | **G*** (ray) + A (marks) |
| 20 | Merge: `process_wires`, `make_other_faces` (region lookup then copy face), `delete_faces`, region void flags, `find_and_set_infinite` (rays), `put_outer_shells_first`, `propagate_regions` | `p4.rs:2134-2218` [V calls, A bodies] | face, region | A, with G inside (`find_region_of_other_face`, infinite-region rays) |
| 21 | `post_process`: per intersection edge `is_topological_without_growth` (pure, heavy) then delete edge / merge faces | `p4.rs:2219-2230`; `merge/p1.rs:624` [A] | edge | G (test) + A |
| 22 | Exit of `manifold_boolean_translated`: restore `editing_enabled`, release mark lock, slit-loop removal and zero-area face collapse, `finish_boolean` (`delete_stale_caches` with an empty map = no-op; `doing_boolean(false)`) | `p4.rs:2424-2457`, `:2512-2579`, `:2021-2029` [V] | — | A |
| 23 | Owners adopted, attributes restored; `refresh_cached_topology` on result and kept other | `merge/p3.rs:368-375`; `api_brep.rs:630-634` [V] | entity | A (metadata) |
| 24 | ops exit: `update_tolerances_after_boolean` (centre wide vertices, `validate_and_update_tolerances`), `finished` (closedness; full `prove` under Proved) | `OPS/boolean.rs:163-164`; `topology/brep/validation/wide_vertices.rs:22` [A] | body | A + G |

### 1.2 Things the brief assumed that the code does not do

- **No operand copy, no rollback wrapper.** `merge_boolean_by` takes `Brep` by value (`api_brep.rs:573-580`) [V]; ops `take()`s the held B-reps (`OPS/boolean.rs:117`) and runs under `run_taking`/`run_with`, which catches panics and discards the kernel, not under `Brep::with_rollback` [A]. The only copies are the tolerance trial copy (0b), the glue snapshot (1) and `slice`'s `brep_copy` (`OPS/split.rs:341`) [A]. `manifold_boolean`'s doc says so: "`Err` does not promise retained ownership or rollback" (`p4.rs:2254-2256`) [V].
- **No `validate_only` use** on the Boolean path [A, by grep]. It matters only as the one flag that blocks the tolerance raise in `set_uv_trim_curve` (`edgeuse/p1.rs:808-811`) [A].
- **No kernel-side validity check or tolerance settle on the result.** All of it is in ops (row 24). `imprint_with` and non-Proved `union_all` levels get only a topology test [A].
- **`cache_kernel` is not used by the pair phase's surface caches.** The forked kernels build only the `BrepCache` (`arena_pair.rs:1909-1926`) [V]. Face caches are built with the real kernel `k` passed to `intersect_topology` (`p2.rs:1526`), with `doing_boolean == true`.

### 1.3 Mode state during the gather-like phases

| Phase | `editing_enabled` | `doing_boolean` | Consequence for a cache build |
|---|---|---|---|
| Pair (9) | true on both | true | Factory defers the tree (`surface/surface/p1.rs:1045`) [V]. `ray.rs` path: with fix `brep_ray_classifies_current_trims` implants trims while editing, else standalone. Slot path: standalone. |
| Classification 1 (17) | true | true | Slot builds are standalone (`trim_srf_cache.rs:797`) [V]; trims come only from the edge-use routine. |
| Classification 2 (19) | **false** inside each ray (`merge/p1.rs:465-467`) [V] | true | Tree deferred by the factory, then built at once by `get_tree_with_face` (`ray.rs:242-246`) on the implant branch: face routine on every reached face. |

`FaceCacheKey` (`live.rs:117-140`) contains `editing_enabled` and `doing_boolean` precisely because of this table [V].

---

## 2. Every write in a gather-like phase

Classes: **P** preparable up front; **R** requestable (gather returns a request, apply fulfils, item re-runs); **C** coupled (a later item's answer depends on the write); **S** scratch.

### 2.1 Pair phase (row 9)

| # | Write | Where | Fires | Class |
|---|---|---|---|---|
| 1 | Work counter `face_pairs`; cancellation polls | `p2.rs:1614,1620,1625` [V] | always | S |
| 2 | Face's trimmed cache built into `FaceIntersectCaches.surfaces` (through `RefCell<&mut Brep>`) | `face/p3.rs:2775-2806`; `live.rs:671-699`; `ray.rs:197-248` [V] | once per face that reaches an SSI (non-coincident pair surviving the box test), until the table is cleared | **P** (per candidate face) |
| 3 | UV trims of **every** edge use of that face, by the face routine, inside write 2 (WR-732 on) | `trim_srf_cache.rs:821-832`; `face/p3.rs:1321-1387` [V/A] | with write 2 when trims are missing: fresh primitives always; bodies that were displayed/saved rarely | **P**, order-sensitive (section 4) |
| 4 | Face routine's second loop: trim end moves on this trim and its neighbour in the loop, vertex tolerance raise to 2×gap, edge raise via `keep_trim_covered` | `face/p3.rs:1389-1468`; `face/p2.rs:3296-3506,3581-3642` [A] | with write 2, **even when all trims existed**; edits only when a corner UV gap exceeds `EFF_ZERO·(1+|uv|)` | **P**; idempotence UNVERIFIED (8.3) |
| 5 | Edge tolerance raise from a new trim (`finish` and `set_uv_trim_curve`), cascading to both vertices; possible edge interval change and re-trim of other faces' trims of the edge | `edgeuse/p3.rs:1162-1171,1010-1040,1184-1214`; `edgeuse/p1.rs:807-822` [A] | with each created trim when the fit misses the current tolerance; interval change rare | **P** as part of 3; C if left lazy |
| 6 | Edge/face and vertex/face gap memos on edge uses and vertex uses | `edgeuse/p1.rs:906-919`; `vertexuse.rs:715-733` [A/V] | commonly, first ask per use | **P** (memo of a pure function of the body once 3-5 are settled) |
| 7 | Surface's owner cleared and restored during trim creation; `editing_enabled` set true and restored by `implant_face` | `edgeuse/p3.rs:643-646`; `trim_srf_cache.rs:822-826` [A/V] | with 3 | S (disappears when 3 moves to prepare) |
| 8 | Cache copy per fetch; SSI sets `point_test_enabled`, `process_boundary_curves` and lazily computes bounding planes on its copy | `live.rs:686,693`; `intersect/surface_intersector/p5.rs:580-588` [V] | always | S (per-item copy today; should become per-item scratch beside a shared immutable cache) |
| 9 | Seam memo `seams: BTreeMap<FaceId, Option<bool>>` | `face/p3.rs:2595,2621-2626,2910-2919` [V] | once per face | **P** |
| 10 | Slot cache of the face made and kept (standalone under editing); slot taken and put back around every use | `query/face/p2.rs:2490` (`intersect_ts_bound`), `:1127-1137` (point classify), `intersect/point_class.rs:2160`; `p2.rs:568-613` [V] | commonly, once a curve is classified on the face | **P** for the make; the take/put is S but is a **shared-state write through `&Brep`** that must go (a concurrent reader sees `CheckedOut` and takes another code path, `p2.rs:589,1129`) |
| 11 | UV trim created by the edge-use routine on first need inside classification (loop walks, binormals, boundary intersection) | `topology/loopuse.rs:155-167`; `query/face/p2.rs:2314,2442`; `edgeuse/p3.rs:3412-3424` [V/A] | only for edge uses the face routine did not cover (faces classified but never given an SSI cache: coincident pairs; fix off) | **P** if the face is prepared, else **R** (already expressible: `QueryTopology::Requested`) |
| 12 | **Edge tolerance raised by a curved interval running beside an edge**, cascading to vertices | `query/face/p2.rs:2419-2437`; decision `intersect/curve_class/p3.rs:2382-2389` [V] | rare: curved interval, past the band, within tolerance of an end vertex; `curved_edge_runs` counts the neighbouring non-raising branch (`p3.rs:2377`), the raise itself has no counter | **C** |
| 13 | **Vertex tolerance raised to the edge's zone** when `vtx_zone < edge_zone` | `intersect/curve_class/p5.rs:2015-2019,200-204` [V] | whenever an edge is intersected whose end vertex is tighter than the edge; default `ok_to_set_tolerance = true` (`p1.rs:746`) | **C** by today's rule; **P** if "vertex ≥ edge zone" is settled in prepare |
| 14 | **Large-gap refinement**: edge curve (and its mate's) rewritten through a refined point, every UV trim of the edge deleted, classification curve marked stale | `intersect/curve_class/p2.rs:1011-1032,1344-1370,1410-1414`; called `p5.rs:1754` [V] | rare path by construction (gaps beyond tolerance at a curve/edge crossing); no counter | **C** |
| 15 | **Surf/surf/surf vertex block**: classification curve bent through a vertex, UV trims of the vertex's edges deleted | `intersect/curve_class/p5.rs:1295-1352` [V] | drops in `[1e-7, xsect tol]` that are not on both surfaces within 1e-8; in the pair phase the adapter lends no mutable curve (`face/p3.rs:2306-2308`), so reachability here is UNVERIFIED (8.4) | **C** if reachable |
| 16 | `set_props_stage2_mut` on the face | `intersect/curve_class/p4.rs:369-388` [V] | UNVERIFIED whether reached from the pair phase | — |
| 17 | `FaceIntersectCaches` cleared when `extract_geometry_mut` lent a body for an edit; **not** cleared for edits made during `curve_on_classify_mut` (a fresh `objects` with `edited: false` is made at `:2981-2989`) | `face/p3.rs:2940-2999` [V] | with 13-15 in the extract step | S, but see 2.3 |
| 18 | New curves registered in the adapter's curve store (handle numbers in visit order); `deleted` list extended | `arena_pair.rs:1647-1660`; `p2.rs:1690-1695` [V] | always | S → move to apply, in pair order |
| 19 | `set_point_class_parameters_mut` on each classification | `p2.rs:1712-1715` [V] | every pair with results | writes the classification (the item's own value); body writes only via lazy trims/gaps (= 6, 11) [I] |
| 20 | Curve cache memos, NURB forms of analytic surfaces, curve analysis and loop-area memos on the kernel (`RefCell`) | `curve/curve/p5.rs:478`; `kernel.rs:142,904,910` [A] | commonly | S/P; geometry memos are `OnceLock`, kernel memos need a per-worker kernel |

**Counts, pair phase:** P = 7 (2, 3, 4, 5, 6, 9, 10-make), R = 1 (11 residue), C = 3 certain (12, 13, 14) + 1 conditional (15), S = 7 (1, 7, 8, 10-take/put, 17, 18, 20), undetermined = 2 (16, 19).

### 2.2 Classification phases (rows 17, 19)

| # | Write | Where | Fires | Class |
|---|---|---|---|---|
| c1 | UV trim created by the edge-use routine during the radial sector's binormal step-offs, on **both** bodies; edge tolerance raise to 2×raise distance; vertex-face gap memos reset | `topology/edge.rs:799-940`; `query/edge.rs:693-839`; `edgeuse/p3.rs:3034-3043,1690`; `edgeuse/p1.rs:765-822` [A] | at most once per edge use; only for edge uses still without a trim after the imprint (split pieces, `fix_gaps`, covered edges). Imprinted edges usually arrive with the intersection's UV curve as trim (`topology_intersector/p1.rs:816`) [A/I]. Frequency UNVERIFIED | **P** (trims of every edge use around each common edge, both bodies) |
| c2 | Slot cache of a face stored when a trim's UV point fails its double-check | `edgeuse/p3.rs:3125`; `query/face/p2.rs:650` [A] | rare [I] | P |
| c3 | `within_partner` → point classification on a `clone_sharing_surface_caches()` copy: trims and raises land on the copy and are dropped, but the partner face's slot cache is stored through the shared slot | `merge/p4.rs:791-862`; `topology.rs:1185-1194`; `query/face/p1.rs:964` [A] | only tangent-patch candidates (surfaces of different analytic kinds) | S (copy) + P (slot) |
| c4 | Marks (`state.primary_marks` / `other_marks`), `faces.*` lists, `tangent_patches`; flood fill by `collect_faces` | `merge/p4.rs:1162,1306-1320,1334-1343` [V] | always | **C** by design: a mark decides which later edge uses are examined |
| c5 | `fix_face_coincidence`: edge relation map, `merge_coincident_surfaces` edits; `modified` restarts the whole round | `p4.rs:1282-1286`; `kernel_adapter.rs:1238-1329` [V/A] | coincident face pairs | **C** (an edit) |
| c6 | `relate_faces` face mate map | `p4.rs:1305`; `kernel_adapter.rs:1226-1236` [V/A] | per compatible pair | C (apply-side state) |
| d1 | `BrepQuery`: `BrepCache` and body box in `OnceCell`s | `p4.rs:1428`; `query/brep.rs:588-595,676-712` [V/A] | once per direction, on first ray | **P** |
| d2 | `editing_enabled` flipped off and back around each ray | `merge/p1.rs:456-470` [V] | every ray | S (becomes a property of the prepare step) |
| d3 | Face cache with tree built for each face a ray reaches, kept in `KeptFaceCaches` under `FaceCacheKey`; reused by later groups in the same direction | `live.rs:80-100,149-185,671-699`; `ray.rs:197-248` [V/A] | once per reached face per direction | **P** |
| d4 | Face routine inside d3 (editing is off, so the implant branch runs): trims, edge/vertex raises, end moves, possibly the face tolerance | `trim_srf_cache.rs:794-838` [V] | with d3 | **P**; **C** if left lazy: a raise made while building face A's tree is seen by face B's later tree but not by A's kept cache [A/I] |
| d5 | Slot take/put, and a tree built into the slot cache when it has none | `query/face/p2.rs:1127-1137,1046-1056` [V] | per face classification inside a ray when the kept cache did not decide | S (take/put) + P (tree) |
| d6 | Edge-use routine via `QueryTopology::Mutable` in loop walks | `topology/loopuse.rs:155-167`; `query/loop.rs:332` [V/A] | when the cache did not decide and a walked edge use has no trim | P (covered by d4) else R |
| d7 | Marks set by `collect_faces` before (primary pass) or after (other pass) the ray | `p4.rs:1446-1455,1508-1517` [V] | per group | **C** by design: chooses the representatives |
| d8 | Kernel counters and memos (`loop_walks`, `containment_edge_drops`, `seam_tests`, curve analysis, loop area) | `query/loop.rs:344,497`; `kernel.rs:142,904` [A] | always | S |

**Counts, classification:** P = 6 (c1, c2, d1, d3, d4, d5-tree), R = 1 (d6 residue), C = 4 (c4, c5, c6, d7 — all apply-side decisions, none a geometric coupling), S = 4 (c3-copy, d2, d5-take/put, d8).

The classification phases have **no C write of the tolerance kind** once d4 is done up front: the rays read tolerances but nothing in the ray path raises them after the trims exist [A/I]. Their C items are the marks and the coincidence repair, which belong to apply anyway. The parallel part is: (17) the radial sector per (common edge, edge use), computed speculatively and discarded for faces already marked; (19) one ray per representative, where choosing representatives is a sequential flood fill that needs no geometry.

### 2.3 The C items, exactly

**C1. Curved-interval edge raise (write 12).** `classify_edge_interval` returns `EdgeToleranceRaise { edge, tolerance: gap }` when a curved interval lies beside an edge at a certified distance `gap` larger than the band but an end is within tolerance of an end vertex (`curve_class/p3.rs:2366-2389`). The caller raises the edge at once and marks the interval on-edge (`query/face/p2.rs:2424-2437`). The module doc states the coupling: "later intervals on that edge see the raised one" (`query/face/p2.rs:38-40`) [V].
- *How a later answer depends:* another pair's interval near the same edge is tested against the edge's zone; with the raised tolerance it classifies as on-edge without its own raise, and the vertex cascade widens vertex capture for every later pair at those vertices.
- *Reformulation:* the decision function is already split from the write (it returns a value). Gather returns the raise as a request and leaves the interval unclassified; apply takes, per edge, the **max** over all requests, raises once, and re-runs the pairs that have a face on that edge. Tolerances only grow, each edge is raised to a value some item certified, and a re-run can only request a raise on an edge whose current tolerance is still below its own certified gap, so the loop terminates in at most (number of edges with requests) rounds; in practice one [I]. This is deterministic and order-free. **It is not the same as today**: today the first pair in loop order sets the tolerance and later pairs with a larger gap raise again; pairs classified *before* the raise keep answers computed at the old tolerance, which the fixpoint would recompute. Results move on exactly the inputs where this branch fires. A count of how often it fires in the rig decides whether that is 0 cases or many (8.1).

**C2. Vertex raised to the edge zone (write 13).** In the curve/edge intersection, `if vtx_zone < edge_zone { set_vertex_tolerance(vtx, edge_zone) }` (`curve_class/p5.rs:2015-2019`) [V].
- *How a later answer depends:* the raised vertex captures later intersection points that would otherwise classify on the edge or the face.
- *Reformulation:* the target value depends only on the body (edge tolerance), not on the item. The local variable is already forced (`vtx_zone = edge_zone` at `:2019` regardless of the write), so the item's own answer does not need the write. Establish "vertex tolerance ≥ zone of each of its edges" for the candidate faces' edges in the prepare step and the branch never fires. `edge::set_tolerance(.., true, true)` already cascades to vertices [A], so the invariant can only be broken by bodies that arrive with it broken (imports, records). Whether establishing it eagerly moves results depends on how many vertices would be raised that today's Boolean never visits (8.2). Otherwise it joins C1's request loop unchanged (max per vertex).

**C3. Large-gap refinement (write 14) and the surf/surf/surf block (write 15).** These replace an edge's curve and delete all its UV trims (`curve_class/p2.rs:1344-1370,1410-1414`; `p5.rs:1309,1350`) [V]. The code itself treats this as "the B-rep changed": `objects.edited` clears both operands' kept face caches and the seam memo (`face/p3.rs:2993-2998`).
- *How a later answer depends:* every later pair on a face of that edge intersects a different edge curve and rebuilds trims and caches from it.
- *Reformulation:* none that keeps the pair independent; the edit is a repair of the operand. The read-only adapter already refuses it (`curve_class/p2.rs:981-990`, `p5.rs:153-190`), but as `SolidError::Unported` ("DISPATCH GAP"), not as a typed request. Make it a request `RepairEdge(edge, point)`; apply performs the repairs in pair order, drops the prepared state of the affected faces, and re-runs every pair that touches a repaired edge. Order among several repairs on one edge stays sequential and fixed. This reproduces today's result only if no pair *before* the repairing pair in loop order touched that edge; otherwise today's earlier pairs saw the unrepaired edge and the re-run sees the repaired one.
- *An existing inconsistency:* an edit lent during `curve_on_classify_mut` does not set the `edited` flag that is checked (a fresh `IntersectObjectsMut` is built for the extract step, `face/p3.rs:2940-2948` vs `:2981-2989`), so writes 12-13 made in the classify step leave the kept SSI caches in place while the same writes made in the extract step clear them [V]. Whether a stale kept cache is ever read after such a raise is UNVERIFIED (8.5); it is the kind of history-dependent answer F6 says is reportable on its own.

---

## 3. What "prepared operand" means for a Boolean

### 3.1 The list

For the pair phase to make no write on the common path, each **candidate face** (a face with at least one box-test survivor) of each operand needs:

| # | Thing | Forced today? | Where |
|---|---|---|---|
| a | UV trim on every edge use of the face (or a recorded "cannot be made") | Lazily, by the first SSI on the face (WR-732 on) or by classification | `trim_srf_cache.rs:832`; `loopuse.rs:157` |
| b | Corner fix of the face's loops done (trim ends meet at vertices; vertex ≥ 2×corner gap) | Same call as (a), re-run on every cache build of the face | `face/p3.rs:1389-1468` |
| c | Edge tolerance covers each of its trims; vertex ≥ zone of its edges | (a) raises edges; the vertex invariant is enforced lazily by write 13 | `edgeuse/p1.rs:807-822`; `curve_class/p5.rs:2015` |
| d | Edge/face and vertex/face gap memos filled | Lazily | `edgeuse/p1.rs:906`; `vertexuse.rs:715` |
| e | The face's trimmed cache **with tree**, boundary implanted — one cache, used by both the SSI and the classification | Twice, in two homes, with different content (finding 0.2) | `ray.rs:197`; `query/face/p2.rs:429` |
| f | Seam flag | Lazily, memoised per operand | `face/p3.rs:2621` |
| g | Face box | Yes, up front for all faces | `p2.rs:1599` |
| h | NURB forms of analytic surfaces, edge curve caches | Lazily, in `OnceLock` memos of the geometry values | report 05 §A5, A9 (not re-read) |
| i | `BrepCache` of each operand (shell vertices, wire edges, later rays) | Lazily, once per adapter, dropped by `merge_context` | `arena_pair.rs:1909-1926,2155` |

For classification 2 the same list applies to **every face of the opposite body a ray can reach**, on the post-imprint body, with editing off.

### 3.2 Which bodies arrive without them

- **Fresh primitives:** no trims, no caches, no gap memos [I from the lazy design; not checked per primitive constructor].
- **Result of a previous Boolean:** the imprinted edges carry the intersection's UV curve as trim (`topology_intersector/p1.rs:816`) [A]; faces the pair phase gave an SSI cache have all trims; untouched faces of a fresh operand still have none. Slot caches survive on surfaces (shared with copies) and are reused while editing even when `face_was_modified` (`query/face/p2.rs:576-586`) [V]. `delete_stale_caches` at exit is a no-op (`merge/p4.rs:2021-2029`) [V].
- **Imported bodies:** trims made under `with_sampled_trim_raise` (`kernel.rs:1036-1039`, review F2.6-7) — different raise rule from the proved one a Boolean would use [review, V there].
- **Bodies from a record:** `to_kernel` runs with `make_uv_trim_curves: false` (review F10) — whatever trims the record carried, no caches, no gap memos.
- **Bodies that were displayed or saved in the app:** already went through a completion loop (`to_record`, review F2 last paragraph), so (a) holds; (b)-(e) do not.

### 3.3 Cost of eager preparation, and per-face on demand

Today's cost is already "one face-routine pass plus one implanted tree per candidate face that reaches an SSI", plus a second (standalone) slot cache per classified face, plus two tree copies per pair. Preparing **candidate faces only, driven by the broad phase** costs the first term for a slightly larger set (candidate faces whose every pair ends at the coincidence check or `crossing_inside_coincident_pair` never reach an SSI today) and removes the second and third terms [I].

Preparing every face of both operands would be the import regression the kernel already documents (126 s against 20 s, `kernel.rs:1036-1039`, cited by the review) and is not needed: the candidate set is known at `p2.rs:1599-1607` before the first pair.

The broad phase must be split in two for this: today candidates are computed per `f1` inside the loop (`p2.rs:1615-1618`). Computing the whole candidate list first is a pure reordering of a pure computation [V that it reads only boxes].

### 3.4 The review's F2 points under a prepared-operand rule

- **A trim can fail by design** (`edgeuse/p3.rs:1717-1732`). Today the face routine stops at the first failure (`face/p3.rs:1383`), the cache factory falls back to the plain untrimmed cache (`surface/surface/p1.rs:1121-1126`), and with WR-732 while editing the answer is `FaceTrimsUnavailable` (`ray.rs:226-239`) [V]. A prepared-face entry must therefore be a `Result`: `Prepared(cache)` or `Unavailable(error)`. `with_face_caches` already does exactly this: `built: BTreeMap<O, Result<SurfaceCache<O>>>`, and "the failure of a build is what the solve's later requests for it return" (`p1.rs:4196-4201,4553-4555`) [V]. No new field on the edge use is needed inside one operation; the failure lives in the phase table and dies with it. This keeps today's behaviour per face (partial trims and raises stay on the body, the pair fails or falls back as before).
- **Healing forbids creation** (`uv_space_ok == false`, `edgeuse/p3.rs:1711-1715`). Not on the Boolean's path: the face routine and the classification pass `uv_space_ok = true` (`face/p3.rs:188-196` [A]; `face/p3.rs:2957,2969` [V]). The prepare step is called by the Boolean, not by healing; the rule is "prepare is an operation's choice", not a body-wide invariant.
- **Stitched faces deliberately trimless** (`cover_joined_faces`, `edgeuse/p3.rs:2026`). Their edges carry a proved tolerance that assumes no trim, and `other_trims_bound` treats a trimless face differently (`:1941-1983`) [A]. A Boolean on such a body already creates those trims today, lazily, the first time the face reaches an SSI — so preparing a *candidate* face changes nothing that today's Boolean does not also change, as long as the same routine runs. Preparing *non-candidate* faces would. This is the strongest argument for candidate-driven preparation and against "complete the operand".
- **Raise mode is ambient** (`prove_lazy_trim_raise`). Inside one Boolean it is constant; `cache_kernel` copies it (`kernel_adapter.rs:65`) [V]. No change.
- **Which routine?** The face routine, because that is what the SSI's cache build runs today (`trim_srf_cache.rs:832`). Using the edge-use routine would drop the corner fix and vertex raises and move results.

---

## 4. The trim-creation problem in this setting

### 4.1 What each routine writes [A unless marked]

**Edge-use routine** (`edgeuse/p3.rs:1690-1773`; creation body `:474-1104`):
- nothing when the edge use or its mate (the same face's other side, `edgeuse/p1.rs:496-509`) already has a trim;
- the trim on the Same-oriented member, a drop-failure record, reset of both vertex uses' vertex/face gap memos (`edgeuse/p1.rs:757-805`);
- edge/face gap memo (`:666`);
- edge tolerance raise in `finish` (`:1162-1171`) and to 2×raise distance in `set_uv_trim_curve` (`edgeuse/p1.rs:807-822`), each cascading to the vertices;
- rarely: edge interval change and re-trim of every other edge use's trim of the edge, **including the neighbouring face's** (`:1010-1040`, `:1184-1214`);
- on failure: `status: Err`, edge use left bare, gap memo kept (`:1717-1732`) [V].

**Face routine** (`face/p3.rs:1295-1480`):
- loop 1: the edge-use routine for each edge use without a trim (`:1321-1387`), aborting at the first failure (`:1383`);
- loop 2, **for every edge use even when all trims existed** (`:1389-1468`): may create a trim (through `normalized_evaluate`), then where consecutive trims do not meet in UV: trims and re-parameterises both, moves both end control points onto the vertex UV (`face/p2.rs:3388-3496`), raises both edges to a proved bound (`keep_trim_covered`, `face/p2.rs:3581-3642`), and raises the vertex to 2×the 3D corner gap (`:1465-1467`).

### 4.2 Why one face's trims touch its neighbours

1. Edge tolerance is shared; three sites raise it (above).
2. Vertex tolerance is shared; raised by the cascade and by the corner fix.
3. Edge interval is shared; a change re-trims the neighbour's trim directly.
4. Nothing refits a neighbour's existing trim after a raise, and nothing invalidates a neighbour's kept cache [A; the second half UNVERIFIED].
5. No recursion into other faces: creation clears its own face's surface owner so nested cache lookups build standalone (`edgeuse/p3.rs:637-646`) [A].

### 4.3 Inputs of one creation that are outputs of another (the order coupling)

- **Fit tolerance** = the edge's current tolerance (`edgeuse/p3.rs:498`, `:405-458`): the second face's trim of a shared edge is fitted at whatever the first face's creation left. The trim *curve* can differ, not only the tolerance.
- **Raise amount** depends on whether other faces of the edge already hold trims (`other_trims_bound`, `:1941-1983`) and on `tol < 2×upstream` (`:1833-1837`): the final edge tolerance is not a max over independent requests.
- **Interval**: whoever goes first decides whether the other drops over the new interval or has its existing trim cut.
- **Vertex tolerance** sets the acceptance of a short drop and the short-seam-piece threshold (`:954-992`, `:811-843`).
- **Snap tolerances** of the corner fix use the edges' current tolerances (`face/p2.rs:3419-3453`).

So creation is a sequential algorithm over faces. It stays in apply.

### 4.4 Does "prepare candidate faces in face-list order before the pair loop" equal today's lazy order?

Today's order of face-routine calls in the pair phase: pairs in (`faces1` order, then ascending index in `faces2`) (`p2.rs:1613-1619`); within a pair, whichever face's cache the surface intersector asks for first (UNVERIFIED which, 8.6); a face's first SSI triggers its routine. Interleaved with it: edge-use-routine creations and writes 12-14 from the classification of each earlier pair.

A prepare step reproduces this body state **exactly** if all of the following hold:
1. it visits faces in the order of their first SSI in today's loop — computable from the candidate list alone, since which pairs reach an SSI depends on the coincidence check and `crossing_inside_coincident_pair`, both pure reads (`p1.rs:1010-1026` [V]; `coincident_strip.rs:170` takes `&mut` — UNVERIFIED what it writes, 8.7), *not* plain face-list order of each operand;
2. the two operands are independent (true: creation on one body reads nothing of the other);
3. no classification write of an earlier pair (edge-use trims on faces not yet prepared, writes 12-14) lands on an edge or vertex of a face prepared later. On the common path those writes do not fire, or fire on faces already prepared;
4. the face routine is not re-run on a face later in the phase with a different outcome. Today it **is** re-run: after any `clear_face_caches` and whenever the kept cache has no tree (`live.rs:677-687`). With one table built once, the re-runs disappear. If loop 2 is idempotent on an unchanged face that changes nothing; if not, results move (8.3).

What makes the result differ, concretely: (i) plain face-list order instead of first-SSI order, when two candidate faces share an edge whose fit misses the tolerance; (ii) preparing a candidate face that today never reaches an SSI (all its pairs coincident or skipped) — it gets the face routine where today it gets edge-use trims or none; (iii) a C write between two faces' creations; (iv) non-idempotent loop 2; (v) WR-732 off, where today's SSI cache is standalone and creates no trims at all — a prepared trimmed cache is then a different (more correct) cache. (v) is moot while SolidCraft runs every flag, and the flag is slated for retirement.

For classification 2 the same analysis applies with a simpler order: today's order is "faces as rays reach them", which depends on ray geometry; a prepare step in face-list order over the opposite body differs from it whenever two reached faces share an under-fitted edge. There the set to prepare is not known before the rays are fired, which is the case for the request loop rather than up-front preparation (section 6).

---

## 5. The caches in this setting

### 5.1 What a pair needs, and where it comes from today

| Need | Home today | Built how often per Boolean (worst case) |
|---|---|---|
| Trimmed cache + tree of each face, for the SSI | **H1** `FaceIntersectCaches.surfaces` in `ArenaPair.face_caches` (`arena_pair.rs:204`), lent through `BrepFaceServices` | Once per face per `ArenaPair` lifetime between clears. Cleared by any `brep_mut` / `with_edit_env` / named edit lend (`arena_pair.rs:301-313,1253,1295,1390,1726,1797,1884,2067,2156,2273,2363`) and by `objects.edited` (`face/p3.rs:2993`). In the pair loop itself no clearing call is made on the common path [V for the loop body's calls: `face_surface`, `check_if_have_existing_ssi`, `surface_coincidence_check`, `face_intersect`, `set_point_class_parameters_mut` → query lend]. A new `ArenaPair` (and empty H1) is made for each adapter stage: `intersect_insert_relate`, `close_intersection_loop`, `fix_gaps`, `delete_faces`, `fix_face_coincidence` (`kernel_adapter.rs:703,953,1007,1065,1123,1291`) [V] |
| Same face's cache, for classification and point drops | **H2** the surface slot (`query/face/p2.rs:429-642`) | Once per surface while kept; standalone under editing; rebuilt after `face_was_modified` once editing is off |
| Same, for topology solves (shell vertices, wire edges, proto edge/vertex work, imprint) | **H3** `with_face_caches`' per-call `FaceCaches<O>` (`topology_intersector/p1.rs:4196-4211,4715-4722`) | **Once per solve call per face** — the table is a local of each call; plus one restart of the solve per request; `keeping_trees` additionally builds H2 (`p1.rs:4584-4619`) |
| Same, for body rays in classification 2 and `find_and_set_infinite` | **H4** `KeptFaceCaches` in a `BrepQuery` (`live.rs:149-185`) under `FaceCacheKey` | Once per reached face per `BrepQuery` (one per direction in `classify_remaining`; others per caller) |
| (read-only ray fallback) | **H5** `RayPreparation.surface_caches` (`ray.rs:59,119-122`) | per solve; not on the Boolean's main path [A] |
| `BrepCache` (three spatial trees) | `ArenaPair.caches: [OnceCell<BrepCache>; 2]` built with the forked `cache_kernels` (`arena_pair.rs:199,1909-1926`); a separate one in each `BrepQuery` | Once per `ArenaPair` stage until a `merge_context` edit takes it (`:2155`); once per `BrepQuery` |
| Edge curve caches, NURB forms | Memos inside the curve / surface values (`OnceLock`) | Once per value while `memoize_curve_caches`; per copy otherwise (report 01, not re-read) |
| Per-fetch copy of a face cache in the solver's handle space | `map_handles` on every hit (`live.rs:686`, `ray.rs:85`, `p1.rs:4265`) | **Every fetch** |

So in the worst case a face's trimmed tree is built once in H1, once in H2, once per topology solve that touches it in H3, and — on the post-imprint body — once per direction in H4. The task brief's "four homes" are H1-H4.

### 5.2 What one per-operation table would replace

A table `face -> Result<prepared cache>` per operand, filled in the prepare step and read-only in the gather, replaces **H1 entirely** and **H2 and H3 for the duration of the pair phase** (rows 7-9). It cannot outlive the phase: rows 11-16 edit the bodies. Classification 2 needs a second table, built on the post-imprint body with editing off; it replaces H4 for that phase. H3's uses inside the imprint are per-edit and stay request-driven.

To be one table the two builds of finding 0.2 must become one cache. That is the first real decision: the trimmed implanted cache is the right one (the standalone slot cache under editing is upstream's "face may be in an intermediate state" guard, which does not apply to an operand before the imprint). Making the classification read the trimmed cache where it reads a standalone one today can move results; it needs its own count and cases.

### 5.3 The key: every input the build reads

| Input | Source | Constant during the pair phase? |
|---|---|---|
| Kernel tolerances | `FaceCacheKey.tolerances` (`live.rs:118`) | yes |
| Context scalars (zone tol, model size estimate, large/small ratio) | `live.rs:119,130-134` | yes [I: no setter seen on the pair path] |
| `doing_boolean` | `live.rs:120`; `surface/surface/p1.rs:1045` | yes (true) |
| `fixes` | `live.rs:121` | yes |
| `editing_enabled`, `implants_while_editing` | `live.rs:122`; `trim_srf_cache.rs:797` | yes (true) — but the two homes pass different `implants_while_editing` (0.2) |
| Surface owner cleared | `query/face/p2.rs:438` | toggled only inside trim creation (write 7); constant once creation is in prepare |
| The surface value, its out-of-bounds flag, offset distance, NURB made or not | review F3 | yes, absent write 14 on a neighbouring edge (does not touch the surface) [I] |
| Face tolerance (owner zone tol) | `trim_srf_cache.rs:835-837` | changed by trim creation (`sync_face_surface_tolerance`) → constant after prepare |
| Face UV domain, loop list | `trim_srf_cache.rs:842,882` | yes (no topology edit in the phase) |
| **Each edge use's trim** | `trim_srf_cache.rs:851` | constant after prepare **except writes 14, 15** (trims deleted) |
| **Edge tolerance** (per trim zone tol) | `trim_srf_cache.rs:856-859` | constant after prepare **except writes 12, 14** |
| **Vertex tolerance, vertex-use UV point** | `trim_srf_cache.rs:872-876` | constant after prepare **except writes 12 (cascade), 13** |
| `prove_lazy_trim_raise` | `edgeuse/p3.rs:1835` | yes |
| Which creation routine ran, and in what order | section 4 | fixed by the prepare step |
| Body identity | address compare today (`ray.rs:70-76`) | one table per operand, held by the phase |

### 5.4 Answer: a phase-scoped table needs no key and no version

Every row is either constant for the whole pair phase or changes only through writes 12-15. Those are exactly the C items, and the plan in 2.3 removes them from the gather: they become requests fulfilled between gather rounds. So:

- **Inside a gather round nothing a cache was built from can change**, by construction. "Built in this phase" is the whole validity condition. No `FaceCacheKey`, no face version, no fan-out of bumps from edge and vertex setters (review F5).
- **Between rounds** the single apply thread knows which edges and vertices it wrote, and drops exactly the table entries of the faces around them (a topology walk from the written entity, done once at the write site), then re-prepares them. This is today's `clear_face_caches` made precise: today it drops both operands' whole tables.
- **The type system can carry the rule**: the gather holds `&Brep` for the phase, so no setter can run; the table is created from `&mut Brep` in prepare and frozen when the borrow is downgraded. Staleness is a borrow error, not a bug to test for.
- The key returns only when a table is to **outlive** the phase (between operations, review's alternative plan step 5). That is a different cache with a different owner.

The same argument holds for the classification-2 table on the post-imprint body, where there are no C writes of the tolerance kind at all (2.2).

---

## 6. A general mechanism

The evidence supports one, because the Boolean already contains each part in some form.

1. **`Prepared<'b>`: a read-only view of a body for one phase.** Holds `&'b Brep` plus the phase table. Constructed only by a prepare step that had `&mut Brep`. Queries on it never create anything; a missing item is a request, never a write.
   *Closest existing code:* `QueryTopology::Prepared(&Brep)` and `Requested(&Brep, &RayPreparation)` (`topology/loopuse.rs:134-141`); `point_classify_prepared` (`query/face/p2.rs:850`); `PreparedFaceQueries` (`surface/surface_cache/face_context.rs:218-251`); `RaySurfaceContext { brep: &Brep, preparation }` (`query/brep/ray.rs:325-328`); the read-only `Access` adapter (`intersect/curve_class/p5.rs:153-190`); `FaceCacheObjects { geometry: &G, caches: &FaceCaches }` (`topology_intersector/p1.rs:4215-4218`).
2. **Phase table: `face -> Result<Arc<prepared cache>>`, plus seam flag; no key (5.4).** Failures are entries. Solvers borrow the cache and keep their flags, bounding planes and handle mapping in per-item scratch instead of copying the tree.
   *Closest:* `FaceCaches<O>.built: BTreeMap<O, Result<SurfaceCache<O>>>` (`p1.rs:4198`); `FaceIntersectCaches` (`face/p3.rs:2589-2596`); `RayPreparation.surface_caches` + `failed_trims` (`ray.rs:57-59`); `KeptFaceCaches` (`live.rs:149`).
3. **Typed requests, returned as part of the item's value.** `NeedFace(face)`, `NeedTrim(edge use)`, `RaiseEdge(edge, tol)`, `RaiseVertex(vertex, tol)`, `RepairEdge(edge, point)`. A request is data in the result, not an error and not a `Cell` side channel.
   *Closest:* `PreparationRequest::{UvTrim, SurfaceCache}` (`ray.rs:62-66`) with `SolidError::PreparationRequired`; `EdgeToleranceRaise` (`curve_class/p3.rs:2383-2388`) — already a returned value; `LargeGap` (`curve_class/p2.rs:1038`) — already a value handed from the read-only head to the mutable tail. Today's read-only adapters answer the three edit kinds with `SolidError::Unported` (`p5.rs:160-180`, `p2.rs:987-990`, `query/face/p2.rs:1049`), which is the thing to replace.
4. **The loop: prepare → gather → fulfil → re-run only affected items.** Fulfil is single-threaded, in item order; raises are combined by max per entity; fulfilment drops the table entries of faces around what it wrote; items whose faces were touched re-run; each item's request is attempted once.
   *Closest:* `with_face_caches` (`p1.rs:4705-4777`) — whole solve re-run, one request at a time, "made no progress" guard at `:4750-4755`; `RayPreparation::prepare` with its `attempted` set (`ray.rs:100-125`); `ShellRayWorld::solve_shape_ray` (`topology/shell.rs:1086-1113`) [A]. The existing loops re-run from scratch per request and are documented as quadratic (`ray.rs:13-15`); gathering all requests of all items before fulfilling removes that.
5. **Per-worker kernel for scratch.** Counters, kernel memos and cancellation polls go to a forked kernel; counters are summed in item order at the join.
   *Closest:* `cache_kernel` (`kernel_adapter.rs:59-108`), which already lists what is copied, shared or started empty.
6. **Not needed:** `LazyBrep` / `SnapshotObjects` (copy on first lazy write, `query/brep.rs:1062-1112`, `query/face/p2.rs:1764`) solve the same problem by discarding or copying back writes; under a prepared view there is nothing to copy. They can retire per call site.

**First, smallest, independently landable change in the Boolean.** Stop copying a kept face cache on every fetch in the pair phase: hold each `FaceIntersectCaches` entry behind `Arc` and hand solvers a shared reference plus their own scratch — or, if the solver API cannot take a borrow in one step, first add the count and land the reuse that the count exposes.

Concretely, in order:
1. *(instrumentation, tiny)* Add two `WorkCounters`: `face_cache_trees_built` (at the three build sites `ray.rs:243`, `query/face/p2.rs:673`, `topology_intersector/p1.rs:4763`) and `face_cache_copies` (at `live.rs:686,693`). No counter for cache builds, cache copies or trim creations exists today [A: all 22 counters and every `add_*` site checked; V: the `add_*` list at `kernel.rs:431-554`].
2. *(the change)* Prepare the candidate faces' SSI caches before the pair loop, driven by the broad-phase candidate list, **in first-SSI order**, through the same `prepare_surface_cache` the lazy path calls, into the same `FaceIntersectCaches`. The loop then finds every cache present. By 4.4 the body state is identical when conditions 1-4 hold; condition 4 holds trivially because no extra build is added, only moved.

   This changes no result on the common path and by itself removes no work — its justification is only as the enabling step, which the rules do not accept alone. So the step to land first is the one with a count:
3. *(the change that stands on its own)* Give `with_face_caches`' topology solves in rows 7 and 13 the operand's `FaceIntersectCaches` entries instead of a fresh per-call table. **Count that demonstrates it:** `face_cache_trees_built` per Boolean on a case with wire edges or many proto edges, before and after: today one build per face per solve call (H3), after one per face per phase. The no-result-change argument: the H3 build and the H1 build call the same factory on the same unchanged body under the same modes — to be confirmed by reading `prepare_face_surface_cache` (`surface/surface_cache/face_context.rs:343`) against `ray.rs:197`, which I did not do (8.8). If they differ (as H1 and H2 do), this step is a result change and needs cases.

If 8.8 shows H1 and H3 differ, the fallback first step is the copy removal alone: `face_cache_copies` per face pair goes from 2 to 0, and the tree is provably the same object.

---

## 7. Cost

### 7.1 PRs to make the pair phase and the classification phases write-free on the common path

| Step | PRs | Result change? |
|---|---|---|
| Counters for tree builds, copies, trim creations, the three C branches | 1 | no |
| Share kept caches instead of copying (solver API takes a borrowed cache + scratch) | 2-4 | no; ported solver signatures move |
| One cache per face in the pair phase (resolve H1 trimmed vs H2 standalone) | 2-3 | **yes**, where classification read the standalone cache |
| Broad phase split out; candidate-driven prepare in first-SSI order | 1-2 | no on the common path; needs the rig as proof |
| Typed requests replacing `Unported` in the read-only adapters (trim, cache, raise edge, raise vertex, repair edge); pair phase runs on the read-only adapters | 3-5 | no if the fulfil loop reproduces loop order; see next row |
| Fulfil loop for C1-C3 (max per entity, re-run affected pairs) | 2-3 | **yes** on inputs where a C branch fires (2.3) |
| Vertex ≥ edge zone established in prepare (retires C2) | 1 | possibly; needs 8.2 |
| Pair loop takes `&Brep` for both operands; curve registration and `deleted` moved to apply | 1-2 | no (handle numbers keep pair order) |
| Classification 1: prepare trims around common edges; sector computed read-only | 2-3 | no on the common path |
| Classification 2: table on the post-imprint body with editing off; ray on `RaySurfaceContext` with requests; `editing_enabled` flip retired; slot take/put removed from the prepared path | 3-5 | order of lazy raises changes (d4) → possibly |
| **Total** | **18-29** | 3 steps with real result changes, each needing cases, pictures and enclosure arguments under AGENTS §1 |

This is for one operation, with no threading, and assumes the WR-732 flag is retired first (otherwise two build behaviours must be carried).

### 7.2 What stays sequential regardless

Trim creation and every tolerance raise (order-defined, section 4); proto vertex / edge find-or-create and proto edge splitting (rows 11-12); resolve, imprint, relate (13-15); loop closing and gap fixing (16); marks, flood fills, coincidence repair and its round restart (17, 19); all of the merge (20); the edit half of post-process (21); exit (22-24); the fulfilment of every request. In item counts the gather covers face pairs, radial sectors, classification rays, and (cheaply reachable later) the infinite-region rays and `is_topological_without_growth` tests.

### 7.3 What carries over to the next operations

| Operation | Carries unchanged | Operation-specific |
|---|---|---|
| Mass properties per face | `Prepared` view, phase table, per-worker kernel; no requests expected on a prepared body; needs an order-free reduction (sum in face order) | which faces: all |
| Point classification batches | everything in classification 2; representatives are the caller's points | none |
| Validity check per entity | view, table, `NeedTrim`/`NeedFace` requests; the tolerance-update mode is the C1 pattern (collect raises, max, apply) and is parallel only in validate-only mode (report 05) | the raise rules |
| Fillet | view and table for the scene faces; per-worker kernel; request loop | the latch and blend memos on the kernel (report 03), stations as items, its own C items — a separate study |

Boolean-specific: the candidate-driven prepare order (first-SSI order), the three C reformulations, curve handle registration order, the two-body `ArenaPair` adapter, and the split of classification 1 into speculative sectors plus ordered marks.

The general part (view, keyless phase table, typed requests, fulfil loop, per-worker kernel) is perhaps a third of the PRs above; the rest is the Boolean's own algorithm work. The review's phrase "an algorithm plan per operation" is accurate, but the per-operation plan is mostly a *list of that operation's C writes and its prepare order*; the machinery is shared.

---

## 8. UNVERIFIED, and the counts that would settle each

1. **How often C1 (curved-interval edge raise) fires.** Count: a counter at `query/face/p2.rs:2429`, run over the rig's Boolean cases. Zero means the fixpoint reformulation moves nothing.
2. **How often C2 (vertex raised to edge zone) fires, and how many vertices of candidate faces violate "vertex ≥ edge zone" on entry.** Count: a counter at `curve_class/p5.rs:2017`, and a one-off walk counting violating vertices per operand at `p2.rs:1599`.
3. **Whether the face routine's loop 2 is idempotent** on a face whose trims all exist and whose body is unchanged (second run writes nothing). Count: trim-end moves and vertex raises at `face/p3.rs:1437,1466` on the second and later calls for a face. Not zero means every cache rebuild today moves the body, and a build-once table changes results.
4. **Whether the surf/surf/surf vertex block (write 15) and `set_props_stage2_mut` (write 16) are reachable from the pair phase.** `IntersectObjectsMut::curve_mut` returns `None` (`face/p3.rs:2306`), which would turn the block's curve edit into an invalid-input error unless `wrapped` is set. Count: hits at `curve_class/p5.rs:1309,1350` and `p4.rs:379` under `face_intersect_breps_cached`.
5. **Whether a kept SSI cache is read after a tolerance write made in the classify step** (the `edited` flag gap, 2.3). Count: for each kept-cache hit at `live.rs:679`, compare the edge and vertex tolerances the cache was implanted with against the body's current ones.
6. **Which face's cache the surface intersector requests first within a pair**, which fixes today's creation order. Read `surface/surface/p*` `global_surface_intersect_with_context`; or log the face id at `live.rs:688`.
7. **What `crossing_inside_coincident_pair` (`coincident_strip.rs:170`) and `check_if_have_existing_ssi` (`p2.rs:1635`) write** through `&mut brep`. Not read.
8. **Whether `prepare_face_surface_cache` (H3, `surface/surface_cache/face_context.rs:343`) and `ray.rs:197` (H1) build the same cache** under the pair phase's modes. Not read. Decides whether step 3 of section 6 is result-neutral.
9. **How often trims are missing in classification 1 and 2** after the imprint. Count: creations at `edgeuse/p3.rs:1764` attributed to the caller phase.
10. **Per-Boolean build and copy totals** (5.1's worst cases are from reading call structure, not from a run). Count: the two proposed counters, per phase, on three cases: two fresh boxes; a 100-face part against a tool; a 2D Boolean of one face against many.
11. **Whether a neighbouring face's kept cache is ever invalidated** after `trim_other_uv_curves` or a tolerance raise on a shared edge [A: none found].
12. **`merge_coincident_surfaces` editing without changing edge counts** (no round restart) [A].
13. **The ops-layer and app-layer callers beyond `OPS/boolean.rs`**: the delegated reader found no `crates/smlib` under this checkout; review F2's statement about app completion loops was not re-verified.
14. **Fresh-primitive state** (no trims, no caches) is inferred from the lazy design, not read per constructor.
15. Everything marked [A] in sections 1, 2.2 and 4.
