# 04 — Cache holders and consumers

Checkout: a local worktree at 782ef9afc. Paths are relative to `solidcore/crates/solidcore/src`.
Read-only investigation by grep and line-range reads; nothing was built or run.
Anything inferred from the code but not confirmed by following every path is marked **UNVERIFIED**.
Counts come from grep and exclude `#[cfg(test)] mod` tails and `*_tests.rs`; treat them as ±10%.

## Summary

- A face's trimmed surface cache has **four independent homes** with different lifetimes: the surface's `CacheSlot`, three per-operation maps (`FaceCaches`, `RayPreparation.surface_caches`, the Boolean's `FaceCaches<O>`), and per-call fresh builds. Only the slot sees `face_was_modified`.
- Every consumer works on a **deep copy** (`map_handles`), so nothing a solve does to its cache flows back. The only caches mutated in place and kept are the slot's (through the lease) and the per-operation maps' own entries.
- The Boolean's topology solves build a **fresh `FaceCaches` per solve call** (per shell vertex, per edge), so the same face's cache is rebuilt many times in one Boolean.
- Plain (untrimmed) surface caches have **no memo at all**: every surface solve without a caller cache rebuilds the Bezier tree. Curve caches already have a value-level memo (`CurveMemo`) that is close to the target shape.
- Solvers read trees through `&Tree`. What they write into a cache is almost entirely (a) lazy one-time refinements or (b) two mode flags flipped and restored. "Immutable built cache + per-query scratch" looks feasible.
- `CacheStore`/`CacheObj`/`CacheCheckOutIn` and `BrepCacheSlot` are vestigial: never instantiated outside tests.

---

## 1. Holder table

### 1.1 Trimmed / plain surface caches

| # | Holder (file:line) | Owner, lifetime | Populated by | Mutated while held; flows back? | Ends |
|---|---|---|---|---|---|
| S1 | `CacheSlot(Arc<Mutex<SlotState>>)` with `TrimCacheSlot::{Empty,Stored,CheckedOut}` — surface/surface/p2.rs:474, 488, 509 | The surface *object*: shared by a surface and every `clone_for_evaluation` copy (p2.rs:969, 1076). `Clone` starts an empty slot (p2.rs:621). Lifetime = as long as any sharing copy lives; effectively per surface value, across operations. Handle space `EntityRef`. | `store_trim_cache` (p2.rs:1388) from a lease; `keep_standalone_trim_cache` (p2.rs:1495) from a face-unaware build while the owner is cleared; `copy_cache_slot_of` (p2.rs:1531) by `map_handles` from another slot; `attach_trim_cache` (p2.rs:1406). | Yes, through the lease S2: tree built on demand, flags, containment, owner tolerance. All of it flows back on `release`. `face_was_modified` is set in place (p2.rs:1422). | `delete_trim_cache` (p2.rs:1449), `delete_stored_trim_cache` (p2.rs:5262), or drop of the last sharing surface. |
| S2 | `FaceSurfaceCache { cache, trimmed, kept }` — query/face/p2.rs:376 | A lease, per call. | `surface_cache_of_face` (p2.rs:556): taken from S1, or fresh build, or fallback plain cache. | Yes (`face_cache_tree`, `bind_face`, `set_owner_zone_tol_3d`, p2.rs:527-547). Flows back to S1 only if `kept`. | `release()` puts back (p2.rs:395) or drops. A lease dropped without `release` leaves S1 `CheckedOut` forever (**UNVERIFIED** that no path does this; `FaceSurfaceCacheGuard` at query/face/p1.rs:964 exists for that reason). |
| S3 | `FaceCaches = BTreeMap<FaceId, SurfaceCache<CacheObject>>` — query/brep/ray/live.rs:623, inside `BrepFaceServices { brep: RefCell<&mut Brep>, caches: RefCell<FaceCaches> }` (live.rs:615) | Per solve; carried between solves by S4/S5. | Fresh build through `query::brep::ray::face_surface_cache` (ray.rs:197) → `get_surface_cache_with_face` (ray.rs:234). **Does not consult S1.** | Entry replaced when a tree-less state cache is later needed with tree (live.rs:677-698). Consumers get `map_handles` copies (live.rs:684, 692, 714, 726). No flow back to S1. | `into_caches` to S4/S5, or dropped. |
| S4 | `KeptFaceCaches { kept: Cell<Option<(FaceCacheKey, FaceCaches)>>, builds }` — live.rs:149 | `BrepQuery` (query/brep.rs:593): per body *state*, by convention ("made again after the B-rep is edited", live.rs:144). | take/put of S3 around each solve (live.rs:176, 185, 194). | Only by S3. | Dropped with the `BrepQuery`; discarded on key mismatch (live.rs:176-180). |
| S5 | `FaceIntersectCaches { surfaces: FaceCaches, seams }` — topology/face/p3.rs:2589; `ArenaPair.face_caches: [_; 2]` — boolean/merge/p4/arena_pair.rs:204 | Per Boolean operand, between edits. | `mem::take` into `BrepFaceServices::with_caches`, back by `into_caches` (face/p3.rs:2778-2804). | Only by S3. | `clear()` on every mutable lend of the B-rep (arena_pair.rs:303, 309 and ~12 more sites: 1253, 1295, 1390, 1726, 1797, 1884, 2067, 2156, 2273, 2363). |
| S6 | `RayPreparation { pending, attempted, failed_trims, surface_caches: BTreeMap<FaceId, SurfaceCache<CacheObject>> }` — query/brep/ray.rs:53 | Per read-only solve with restarts (one per `loop`). | `prepare` → `prepare_surface_cache` (ray.rs:119-122), fresh, not from S1. | No; consumers get `map_handles` copies (ray.rs:84) or `&` (ray.rs:92). | Dropped when the solve returns. |
| S7 | Boolean `FaceCaches<O> { pending, built, states, tree_requested, trims, classified_faces }` — boolean/topology_intersector/p1.rs:4196 | **Per call** of `with_face_caches` (p1.rs:4704, constructed 4714). | `prepare_face_surface_cache[_state]` (p1.rs:4763, 4765) → surface/surface_cache/face_context.rs:368 → `get_surface_cache_with_face` (face_context.rs:387). Fresh, not from S1. A failed build is stored as `Err` and replayed. | No; `map_handles(Ok)` per request (p1.rs:4265, 4303). | Dropped at the end of the solve call. `solve_with_face_caches_keeping_trees` then separately builds S1 for each face that asked for a tree (p1.rs:4585-4617). |
| S8 | `HeldSolveCache<O> = Option<Result<SurfaceCache<O>>>` — surface/surface/p8.rs:127 | Caller of repeated line solves on one unchanged surface (topology/face/p4.rs, topology/topology_traverser.rs). | First solve builds, incl. a kept failure. | Tree built on first use; stays in the holder. | Dropped by caller. |
| S9 | `FaceLineCache { trim: Option<FaceSurfaceCache> }` — topology/face/p4.rs:565-592 | Per line solve. | Lease from S1 + `face_cache_tree`, then `map_handles(|_| surface)` copy for the solve (p4.rs:583). | Lease is; copy is not. | Lease released after the solve. |
| S10 | Intersector locals `Vec<SurfaceCache<O>>` (2 entries) — intersect/surface_intersector/p5.rs:396, 428; borrowed as `&mut [SurfaceCache<O>]` then `&[..]` (p5.rs:485-596), `[Option<&SurfaceCache>; 2]` in `AdvSurfaceIntersector` (adv_surface_intersector.rs:231) | Per surface/surface intersection. | `intersection_surface_caches[_with_context]` → context `build_surface_cache` (a copy of S3/S7) or `surface_cache` fresh. | Flags flipped (p5.rs:580-581; p3.rs:452-456), iso curves, bounding planes copied out (p5.rs:585). No flow back. | Dropped. |
| S11 | `ListedCache::{Lazy,Fetched}(SurfaceCache<usize>)`, `RayCaches` — blend/hcr.rs:539, 546 | Per HCR visibility tracer. | `surface_cache` fresh (hcr.rs:572) or `get_surface_cache_with_face` (hcr.rs:619, 684), not from S1. | `get_tree`; flags set (hcr.rs:708-710). | Dropped. |
| S12 | `TessSrfCache { trim: TrimSrfCache<TessHandle>, face, poly_brep, driver }` — query/tess.rs:337 | Per face tessellation. | `TrimSrfCache::new` in tess. | Heavily: subdivision, node marks and classes (tess.rs:889-1002, 1158), flags (489, 671, 701-703). Its own cache; nothing shared. | Dropped. **UNVERIFIED** whether any caller keeps it across calls (no `TessSrfCache::` reference outside query/tess.rs). |
| S13 | Per-solve owned cache in surface solves: `owned` in `global_point_solve_0_in` (surface/surface/p7.rs:1714-1731), `solve_surface_cache` (p7.rs:2502), `solve_surface_cache_with` (p7.rs:2525), p8.rs:1533, 1978-2010 | Per solve. | `get_surface_cache` fresh (p7.rs:2507) or context copy. A caller's `&SurfaceCache` is **cloned again** when `point_test_off` (p7.rs:1716). | `point_test_enabled = false`, `get_tree`. | Dropped. |
| S14 | Offset: `surface_cache_for` — offset/offset_executive/edgeuse.rs:645-693; `OffsetSurfaceSolveContext` — offset/offset_geometry_creation.rs:1159 | Per solve. | `solve_surface_cache_of_face` / `surface_cache_state_of_face` (query/face/p2.rs:476, 500): lease S1, build tree, `map_handles` copy, release. Else `get_surface_cache` / `make_surface_cache`. | Copy is not kept; S1 is updated. | Dropped. |
| S15 | `PointClassifyOptions::trim_cache: Option<&mut TrimSrfCache<EntityRef>>` — used at topology/face/p4.rs:462, face/p3.rs:496; `delete_stale_caches(caches: &mut BTreeMap<FaceId, TrimSrfCache<O>>)` — topology/brep/p2.rs:1627 | Caller-supplied. | Caller. | Tree built (query/face/p2.rs:1054-1055). | Caller. The Boolean passes an empty map (boolean/merge/p4.rs:2024-2029): a no-op. |
| S16 | `alt_caches: Vec<AltSrfCache>` inside `SurfaceCache` — surface/surface_cache/p1.rs:1638; `add_curves_to_cache` p2.rs:404 | Inside whichever cache it was added to. | Silhouette-type curve results keyed by `(type, approx_tol, angle_tol, AltCacheKey)`. | Appended, oldest evicted at `MAX_ALT_CACHES`. Copied by `clone`/`map_handles` (p1.rs:1710, 1936). | With the cache. Since consumers hold copies, an alt cache added to a copy is lost unless that copy is the lease (**UNVERIFIED** which callers add through a lease). |

### 1.2 Curve caches

| # | Holder | Owner, lifetime | Populated | Mutated / flows back | Ends |
|---|---|---|---|---|---|
| C1 | `CurveMemo(OnceLock<Arc<Mutex<Option<CurveMemoEntry>>>>)` in the curve value — curve/curve/p5.rs:479; key `CurveMemoKey` p5.rs:384 | Curve value; shared by clones; `&mut Curve` edit starts an empty memo (p5.rs doc 46-55). One entry. | `make_curve_cache_with_zone_tol` stores `cache.retag::<()>(None)` (p5.rs:337-357). | Immutable once stored; a hit returns a retagged **copy**. `memoized_cache_top_box` reads in place (p5.rs:367). | Replaced on key change; gone on edit. Off for curves on surfaces, iso curves, tangent fields, curves in volumes (`memo_pure`, p5.rs:437). |
| C2 | `&mut Option<CurveCache<O>>` lazy slots — ~22 parameters: curve/curve/p1.rs:1886, 1945; p6.rs:749, 828, 1736, 1772; p8.rs:856-1002; crv_on_surf.rs:1574-2665; intersect/curve_class/p9.rs:390, 1201; surface/surface/p8.rs:469-470, 982-984 | Per call or per caller loop. | First use builds (`make_curve_cache`, p1.rs:1988). | None after build. | Dropped. |
| C3 | `DropPointCaches(Vec<Option<CurveCache<CurveHandle>>>)` — curve/curve/p6.rs:216 | Caller loop over a curve list; `truncate` when curves are removed. | Lazy. | None. | Dropped. |
| C4 | `OrigCurveCaches(Vec<Option<CurveCache<OrigHandle>>>)` — curve/composite_curve/p1.rs:1985 | Caller across `curve_classify_cached` calls, index-keyed; "must not change while the slots are used". | Lazy. | None. | Dropped. |
| C5 | `edge_caches: BTreeMap<EdgeId, CurveCache<FilletObject>>` — blend/fillet_executive/session.rs:345, filled 7006-7013 | Fillet session (per operation). | `make_curve_cache_with_zone_tol` with the edge tolerance. | None. **UNVERIFIED** whether it is cleared when the session edits an edge. | With the session. |
| C6 | Locals: intersect/surface_intersector/p4.rs:786 `Vec<Option<CurveCache<()>>>`; composite_curve/p2.rs:320, 1266; surface_tracer.rs:1981; surface_drop_curve.rs:1066; surface_drop_lib/p3.rs:1343; surface/surface/p7.rs:1870, p8.rs:2869 | Per call. | Fresh (memo hit when pure). | None. | Dropped. |

### 1.3 B-rep spatial caches

| # | Holder | Owner, lifetime | Populated | Mutated | Ends |
|---|---|---|---|---|---|
| B1 | `BrepQuery { cache: OnceCell<BrepCache>, face_caches: KeptFaceCaches, bbox: OnceCell }` — query/brep.rs:588-595, `cache()` 705 | Per body state, by convention. ~30 construction sites (boolean 8, topology 7, blend 4, offset 2, heal 3, query 3+, primitive 1). | `make_brep_cache` on first ask (brep.rs:710). | No. | Dropped with the query. |
| B2 | `ArenaPair.caches: [OnceCell<BrepCache<H>>; 2]` with `cache_kernels: [Kernel; 2]` — boolean/merge/p4/arena_pair.rs:198-199, built 1909-1925 | Per `ArenaPair` (a Boolean stage's borrow of both operands). | `BrepCache::new` + `build_trees` with handle mapping to `H`. | No. **Not cleared by `clear_face_caches`**; relies on the pair being rebuilt after edits (**UNVERIFIED**). | Dropped with the pair. |
| B3 | Locals: offset/offset_executive/p1.rs:3331 (`make_original_brep_cache`, per offset); topology/brep/p2.rs:1429; topology/brep/p10.rs:2487; geom/tools.rs:706; intersect/ray_tracer.rs:1335 (poly B-rep) | Per call / per operation. | Fresh. | No. | Dropped. |
| B4 | `BrepCacheSlot { kernel, cache: &mut Option<BrepCache> }` — topology/brep/p2.rs:759 | Would be caller-supplied and kept in step by `notify` (p2.rs:794-840). | — | `rm_from_spatial_tree` / `add_to_spatial_tree`. | **Never constructed**: no `BrepCacheSlot {` anywhere; every `brep::p2::notify` caller sampled passes `None` (topology/brep/p3.rs:35 says so). Vestigial. |

### 1.4 Other holders named in the brief

- `LazyBrep::{Own, Shared{source, copy}}` — query/brep.rs:1062. Not a cache holder; it holds a private B-rep copy made at first edit (`clone_standing_for_self`, brep.rs:1105). That copy's face surfaces are `clone_for_evaluation` copies (topology.rs:1262) and therefore **share S1 with the source**.
- `CurveOnClassifyCache { snapshot: Option<Brep> }` — query/face/p2.rs:1544. Holds a `clone_sharing_surface_caches` snapshot (p2.rs:1554); gap caches, UV trims and raised tolerances are copied back by `apply_uv_trim_updates` (p2.rs:1561). Shares S1.
- `MappedCacheFace` — surface/surface_cache/face_context.rs. An adapter (`CacheFace<O>` over `BrepCacheFace` plus two handle maps); holds no cache.
- `BrepCacheFace { b, f }` — query/face/p2.rs. The `CacheFace<EntityRef>` adapter that lets a cache build edit the B-rep; holds no cache.
- `CachedVisits<O>` — intersect/curve_class/p9.rs:140. A per-query visit list (vertices, edge uses of leaf nodes met), returned by value; the cache is read through `&` (p9.rs:610-645).
- `BandCutCache { edges, faces, unseen, query: BrepQuery<'static> }` — blend/fillet_executive/session/band_cuts.rs:141. Boxes and evaluation geometry per edge/face plus a B1.
- `FaceCacheTarget` — boolean/topology_intersector/p1.rs:4169. A mutable lend of one face's B-rep with the two handle maps; holds no cache. Obtaining it clears S5 for that operand (arena_pair.rs `brep_m` path).

### 1.5 The ported upstream cache manager

`geom/a_object.rs`: `CacheStore<T>` (787), `CacheObj<T>` (741), `CacheId` (726), `AObject::cache_obj`/`set_cache_obj` (916, 921), `get_or_create_object_cache` (~1034), `delete_objects_cache` (954), `CacheCheckOutIn` (1098).

- `CacheStore::new()` appears only in tests: query/poly.rs:3882, intersect/ray_tracer.rs:1852, a_object.rs:1425, 1446.
- `set_cache_obj` is called only inside a_object.rs.
- Live signatures still thread it: `curve::p2::notify` (curve/curve/p2.rs:2433), `surface::p2::notify` (surface/surface/p2.rs:5277), `PolyNotifyEnv.caches` (query/poly.rs:441, used at 3287). No non-test code constructs a `PolyNotifyEnv` (only poly.rs:3883 and ray_tracer.rs:1853, both in test modules).
- `get_or_create_object_cache` cannot build: it returns "a new … cache is built from the …" errors (a_object.rs:1011, 1050, 1068).

**Vestigial.** The live equivalents are S1 (use count → `CheckedOut`), C1, and B1.

### 1.6 Relation graph

```
surface value ──owns──> S1 CacheSlot (Arc)  <──share── every clone_for_evaluation copy
                          │                 <──share── Brep::clone_sharing_surface_caches (topology.rs:1194)
                          │                 <──share── Brep::clone_standing_for_self (topology.rs:1230, 1262)
                          │                 --map_handles copy--> Brep::clone_for_trial slot (topology.rs:1290-1306)
                          │                 (empty)   <── Surface::clone, Brep::snapshot (surface p2.rs:1024)
                          │
        S1 --take/put--> S2 lease --map_handles--> S14 offset solve copy
                          │       --map_handles--> S9 line solve copy
                          │       --state bits read--> point_class.rs:2160, vertexuse.rs:1036, brep/p1.rs:1947
        S1 --take/put--> point_classify fast path (query/face/p2.rs:1127-1137)
        standalone build --map_handles(Face(f))--> S1   (keep_standalone_trim_cache, only while owner cleared)

fresh build (ray.rs:197) --> S3 FaceCaches --map_handles--> each solve's cache (S10, S13)
        S3 --take/put--> S4 KeptFaceCaches (BrepQuery)      key: FaceCacheKey
        S3 --take/put--> S5 FaceIntersectCaches (ArenaPair)  cleared on any &mut lend
        S3 --&--> point_classify_live fast path (live.rs:744-760)

fresh build (face_context.rs:368) --> S7 Boolean FaceCaches<O> --map_handles--> topology-solve caches
        after the solve: S1 built separately for the same faces (keep_face_cache_tree)

fresh build (ray.rs:121) --> S6 RayPreparation --map_handles--> solve; --&--> point_classify (p2.rs:1088)

get_surface_cache (no face) --> S13 per-solve cache          (no memo, no holder)
curve value ──owns──> C1 CurveMemo --retag copy--> C2..C6
body state ──by convention──> B1 BrepQuery ──owns──> S4, BrepCache
```

S1, S3/S4/S5, S6 and S7 never exchange caches. A face can therefore have up to four different trimmed caches alive at once inside one Boolean, built at different moments of the body's history.

---

## 2. Build-site counts

Live call sites (tests excluded).

| Entry function | Live | Sites, by subsystem |
|---|---|---|
| `make_surface_cache` | 6 | surface factory fallbacks p1.rs:1124, 1146, 1168; query/face/p2.rs:631 (failed trimmed build); offset edgeuse.rs:690; intersect/curve_class/p1.rs:2983 (built only to test for an error) |
| `make_trim_srf_cache` | 3 | surface/surface/p1.rs:1143, 1165 (inside `get_trim_surface_cache`, `get_surface_cache`); query/face/p2.rs:610 (owner cleared) |
| `make_trim_srf_cache_with_face` | 3 | surface/surface/p1.rs:1121; query/face/p2.rs:612; intersect/curve_class/p1.rs:2974 (per `compute_face_point_class_in` call, not kept) |
| `get_surface_cache_with_face` | 5 | query/face/p2.rs:594 (nested lookup); query/brep/ray.rs:234 (S3/S6); blend/hcr.rs:619, 684; face_context.rs:387 (S7) |
| `get_trim_surface_cache` | 1 | query/face/p2.rs:592 |
| `get_surface_cache` (the general per-solve builder) | 14 | surface solves p1.rs:2391, 2864, p4.rs:1783, p5.rs:1478-1480, p7.rs:2507, p8.rs:1533, p2.rs:5311 (re-export `surface_cache`); offset edgeuse.rs:659, 666, offset_geometry_creation.rs:745; blend fillet_solver/p3.rs:215, fillet_geom/p1.rs:2878 |
| `surface::p2::surface_cache` (wrapper of the above) | 18 | query/brep/ray/live.rs ×8; boolean topology_intersector/p1.rs:4249, 4252; intersect surface_intersector/p3.rs:383, p5.rs:195, ray_tracer.rs:1244, topology_solver/p1.rs:2160, curve_class/p1.rs:2101, surface_tracer.rs:2169; blend/hcr.rs:572; surface/p7.rs:2338 |
| `make_curve_cache` | 10 | curve p1.rs:1988, p7.rs:1140, p9.rs:1215; intersect surface_intersector/p4.rs:802, surface_drop_lib/p3.rs:1343, curve_class/p9.rs:619, surface_drop_curve.rs:1066, surface_tracer.rs:1981; surface p7.rs:1870, p8.rs:2869 |
| `make_curve_cache_with_zone_tol` | 3 | curve p5.rs:325, p8.rs:739; blend session.rs:7008 |
| `make_brep_cache` | 6 | query/brep.rs:710; offset p1.rs:3331; topology brep/p2.rs:1429, brep/p10.rs:2487; geom/tools.rs:706; intersect/ray_tracer.rs:1335 (plus `BrepCache::new`+`build_trees` at arena_pair.rs:1921) |
| `face_cache_tree` | 4 | query/face/p2.rs:533, 1054; topology/face/p4.rs:579; boolean topology_intersector/p1.rs:4613 (inside `keep_face_cache_tree`, called per built face at p1.rs:4596) |
| `query::face::p2::face_surface_cache` (slot lease) | 9 | topology vertexuse.rs:1036, face/p4.rs:578, brep/p1.rs:1947, brep/p9.rs:3059; query face/p1.rs:964, face/p2.rs:651, 2490; intersect point_class.rs:2160; boolean topology_intersector/p1.rs:4611 |
| `note_face_surface_cache` (side effects only) | 1 | topology/edgeuse/p3.rs:327 (called at 3125) |
| `solve_surface_cache_of_face` / `surface_cache_state_of_face` | 2 | offset edgeuse.rs:684, 686 |
| `prepare_surface_cache` / `prepare_face_surface_cache[_state]` | 4 | query/brep/ray.rs:121, ray/live.rs:688; boolean topology_intersector/p1.rs:4763, 4765 |
| context `build_surface_cache` (indirect) | 14 | boolean ×3, surface p1/p7/p8 ×6, intersect ×2, topology brep/p9, planar_section, exact_pieces |

By subsystem, roughly: surface solves 25, intersect 14, query 20, boolean 8, offset 7, blend 6, topology 8, tess 1 (own type), mass and heal 0 direct (they reach caches through `BrepQuery` and classification; **UNVERIFIED**).

### Repeated rebuilds of the same cache

1. **Boolean topology solves, per call.** `with_face_caches` constructs a new `FaceCaches` every call (topology_intersector/p1.rs:4714). Its callers sit inside loops:
   - `intersect_shell_vertices`, per shell vertex (p1.rs:4800, 4814);
   - `intersect_wire_lamina_edges`, per edge (p1.rs:4897, 4923);
   - `surface_global_curve_intersect` (p1.rs:993);
   - `iir_edge_brep_into`, per edge (p2.rs:974);
   - `edge_coincident_with_brep`, per vertex edge (p2.rs:1328);
   - `edge_face_coincidence`, per common face (p2.rs:2640).
   A face near N vertices and M edges has its trimmed cache and tree built up to N+M times, plus a separate tree-less `states` build each time the fast-drop test asks first.
2. **Boolean, three homes for one face.** Face/face intersection keeps S5; topology solves use S7; classification and `keep_face_cache_tree` use S1. Each builds its own.
3. **S5 is emptied by any mutable lend** of the operand (arena_pair.rs:303), including `face_cache_target` for an S7 build and `set_editing_enabled` (arena_pair.rs:1726). So building one face's cache for a topology solve discards every face/face cache of that operand.
4. **Restart cost.** Each `PreparationRequired` re-runs the solve from its start (p1.rs:4722-4741; ray.rs loops). A solve touching k uncached faces runs k+1 times, and trims add more.
5. **No plain-surface memo.** `solve_surface_cache` (surface/surface/p7.rs:2502-2520) builds the cache and tree per solve. Callers that solve repeatedly on one surface must opt in to `HeldSolveCache` (7 call sites use the `_held`/`_cached` variants).
6. **Copy per consumer.** Each `build_surface_cache` hit is a full `map_handles` deep copy: tree, patches, auxs, alt caches and an evaluation copy of the surface (surface_cache/p1.rs:1873-1960). `global_point_solve_0_in` clones once more to turn point testing off (p7.rs:1716).
7. **Built for side effects or one bit.** `note_face_surface_cache` (edgeuse/p3.rs:327), brep/p9.rs:3059 (`face_surface_cache(..)?.release()`), `live_face_fast_drop_test` (point_class.rs:2160), vertexuse.rs:1036, curve_class/p1.rs:2974-2983 (built, used for one classification, dropped).
8. **HCR**: the doc at blend/hcr.rs:588-594 states a face-owned cache "rebuilds on every call" of `GetTree` when the face has no trims or its B-rep is being edited (three `GetTree` calls per listed face).

---

## 3. Reuse rules of the trimmed surface cache

### 3.1 Path A — the slot lease (`surface_cache_of_face`, query/face/p2.rs:556-642)

Inputs: `surface` (an evaluation copy sharing S1), `cleared` (owner-cleared state), `b.editing_enabled()`, `k.context.doing_boolean()`.

| Slot state | Condition | Result | Kept? |
|---|---|---|---|
| `Stored` | `!(face_was_modified && !cleared && !b.editing_enabled())` (p2.rs:578) | Reused as is, whatever it was built with (face or standalone). Its surface copy adopts the current owner (p2.rs:571). | yes |
| `Stored` | `face_was_modified && !cleared && !editing_enabled` | `delete_trim_cache` (p2.rs:587), fall to build. | — |
| `CheckedOut` | `cleared` | `get_trim_surface_cache(standalone)` (p2.rs:592) | **no** |
| `CheckedOut` | `!cleared` | `get_surface_cache_with_face` (p2.rs:594); `trimmed` read back from `trimmed_dispatch` | **no** |
| `Empty` / deleted | `cleared` | `check_out`; `make_trim_srf_cache(standalone)` — no face, no UV trims made (p2.rs:610) | yes |
| `Empty` / deleted | `!cleared` | `check_out`; `make_trim_srf_cache_with_face` — creates UV trims, may raise tolerances (p2.rs:612) | yes |
| build failed | error is `Unported`/`Cancelled`/`InvalidInput`/`PortInvariant`/`NotFound` | slot deleted, error returned (p2.rs:622-628) | — |
| build failed | any other error | slot deleted; `make_surface_cache` plain, `trimmed = false` (p2.rs:630-638) | **no** |

Inside the factory (surface/surface/p1.rs:1040-1104):

- `k.context.doing_boolean()` → the tree is **not built** (p1.rs:1045); it waits for a `GetTree`.
- `face.brep_editing_enabled() && !face.implants_while_editing()` → tree built **without the face** and `set_standalone_flags()` (trim_srf_cache.rs:794-804, 583-585).
- A face with no loop uses → standalone flags (trim_srf_cache.rs:895-898).
- Otherwise `create_uv_trim_curves`, implant edge uses and vertex uses, shrink the domain, `mark_face_containment`, `Done` (trim_srf_cache.rs:830-916).

`face_cache_tree` (p2.rs:661-676) rebuilds the tree when there is none, or when `!have_t_surface_cache && surface.face().is_some() && containment != UnderConstruction`. So a cache built without the face while editing is rebuilt with the face on the next `GetTree` after editing ends — but only if someone calls `GetTree`.

### 3.2 Path B — classification fast path (`point_classify_with_topology`, query/face/p2.rs:1061-1145)

Order of sources, none of which builds a cache:

1. `prepared_cache` argument, else `RayPreparation::prepared_cache(f)` (p2.rs:1086-1090), used when `cache_is_current` (p2.rs:985-989).
2. `opts.trim_cache` (caller's `&mut TrimSrfCache`).
3. The slot: `take_trim_cache`; `Stored` is used and put back; `Empty` and `CheckedOut` give no cache (p2.rs:1127-1137).

Used only when `have_t_surface_cache && containment == Done && !face_was_modified && face_owned` (p2.rs:1038-1043). A missing tree is built for `Mutable` topology and is an `Unported` error for `Prepared`/`Requested` (p2.rs:1046-1056). Otherwise the loop walk runs.

### 3.3 Path C — state-bit readers

- `live_face_fast_drop_test` (intersect/point_class.rs:2147-2166): leases through A; result latched in `test_on` for the rest of that solve.
- `face_point_test_on` (point_class.rs:2473-2496): read-only; `Stored` is used if `!face_was_modified || editing_enabled`; anything else is an `Unported` error.
- vertexuse.rs:1033-1041, brep/p1.rs:1944-1953: lease through A unless `trim_cache_in_use()`, in which case **no test at all**.

### 3.4 Path D — operation maps (S3/S4/S5/S6/S7)

- S3 `cache_mapped` (live.rs:669-698): reuse if present **and has a tree**; else `prepare_surface_cache` (fresh, face-aware, `GetTree`).
- S3 `cache_state_mapped` (live.rs:706-733): reuse if present; else fresh without `GetTree`.
- S4: reused only under an equal `FaceCacheKey` = kernel tolerances, three context scalars, `doing_boolean`, `fixes`, `b.editing_enabled()` (live.rs:117-140). Nothing about the body itself.
- S7: `built` reused within the call; `states` reused until the face is in `tree_requested`, after which the state reader gets the built cache (p1.rs:4297-4300).
- ray.rs:197-242 adds its own rules: with `fixes.brep_ray_classifies_current_trims && editing_enabled`, an open loop or failed implant is `FaceTrimsUnavailable`, and the face is implanted while editing.

### 3.5 Inputs to these decisions that are not cached data

| Input | Where read |
|---|---|
| `Brep::editing_enabled()` | p2.rs:578; trim_srf_cache.rs:797; point_class.rs:2481; ray.rs:219; live.rs:138 |
| `Brep::is_surface_owner_cleared(f)` and the slot's `cleared_for` stack | p2.rs:438, 457; surface p2.rs:495, 1500; point_class.rs:2159, 2474; brep/p9.rs:3058 |
| Slot state `CheckedOut` (a caller up the stack holds the lease) | p2.rs:589; vertexuse.rs:1033; brep/p1.rs:1944 |
| `k.context.doing_boolean()` | surface p1.rs:1045; live.rs:136; curve p5.rs:424 |
| `k.fixes`, `k.tolerances`, context zone tol / model size / size ratio | live.rs:128-137; curve p5.rs:414-427; `cache_tolerances` surface p1.rs:931 |
| `NetScope` (blend nets) | surface_cache/p3.rs:1181 |
| Whether the surface has an owner (`surface.face()`) and which copy shares the slot (`shares_cache_slot_with`) | p2.rs:461; trim_srf_cache.rs:784 |
| Whether a `GetTree` has happened yet for this face in this attempt (`tree_requested`) | topology_intersector/p1.rs:4297 |
| Which API was used: `Mutable` vs `Prepared` vs `Requested` topology | p2.rs:1046-1056 |
| Whether the face currently has loops / UV trims | trim_srf_cache.rs:895; ray.rs:223 |
| The error *kind* of a failed build | p2.rs:623-638 |

### 3.6 History dependence: call sequences that give a different cache for the same face

1. **First lookup while the owner is cleared.** Inside `CreateUVTrimCurve(s)` (topology/edgeuse/p3.rs:646, face/p3.rs:1274) the lookup builds a *standalone* cache (no boundary, `point_test_enabled = false`, `process_boundary_curves = true`) and keeps it. Every later lookup reuses it (p2.rs:578) until some caller runs `face_cache_tree`. A first lookup outside that scope gives a trimmed cache with point testing on. `keep_standalone_trim_cache` (surface p2.rs:1495) makes the same thing happen through a face-unaware `make_trim_srf_cache`.
2. **First lookup while `editing_enabled`.** Tree built without the face, standalone flags. Readers of state bits (path C) see point testing off until a `GetTree` rebuilds it.
3. **First lookup during a Boolean.** No tree; constructor bits (`point_test_enabled = true`, `have_t_surface_cache = false`, trim_srf_cache.rs:414-418). The fast-drop test reads `point_test_enabled` and so is *on*; after a `GetTree` on a face without loops it turns *off*. The trait doc says so directly: "The bits then depend on whether the face's cache was built before" (surface/surface/p1.rs:1196-1203), and the port "keeps no record of that" (query/face/p2.rs:492-493; face_context.rs:349-351).
4. **Modified, then looked up while editing vs after.** `face_was_modified` + `editing_enabled` → stale cache reused; the same lookup after `set_editing_enabled(false)` → rebuilt. With `cleared`, reused even when editing is off.
5. **Nested lookup.** While a lease is out, an inner lookup builds a second cache that is not kept; what the inner code learns (tree, containment) is lost, and vertexuse.rs:1033 / brep/p1.rs:1944 skip the point test entirely.
6. **Cache present vs absent changes a classification result.** The fast path answers from node classes; the walk applies the source tolerance. The test at query/brep/ray.rs:480-486 records it: "An oversized source tolerance makes the uncached fallback snap to a vertex. Native cache-first classification returns the interior face."
7. **Which home answered.** S3/S6/S7 builds do not read S1 and S1 builds do not read them, so the same face can be served by a cache built before a trim existed in one home and after in another.
8. **Failed build fallback is not remembered in S1** (slot deleted, p2.rs:622): the next lookup retries the trimmed build. S7 and S8 *do* remember the failure. Same face, different retry behaviour by home.
9. **Scratch copies sharing S1.** A classification on a `clone_sharing_surface_caches` / `clone_standing_for_self` copy builds the cache against the copy's trims and stores it in the original's slot; the trims themselves stay on the copy (boolean/merge/p4.rs:794; curve_class/p4.rs:507, 579; face/p4.rs:2410; `LazyBrep::Shared`). The original later finds a cache and never creates its own UV trims through the cache build. **UNVERIFIED** as an observable difference; the aliasing itself is confirmed.
10. **`test_on` latch** (point_class.rs:2155-2167): decided once per solve from whatever the slot held at the first leaf.

---

## 4. Invalidation

### 4.1 What deletes or marks a cache stale

| Mechanism | Site | Effect |
|---|---|---|
| `SmFace::Notify(PreEdit)` | topology/face/p3.rs:503-511 | Sets `face_was_modified` on the given cache, else on S1 (`mark_trim_cache_face_modified`, surface p2.rs:1422). A checked-out cache gets it when stored (`face_modified_while_out`, p2.rs:1391-1394). **A mark, not a delete**, and it only takes effect per rule 3.1. Reaches S1 only. |
| Callers of face `PreEdit` | ~80 sites in topology/brep/p2–p10, face/p1, loopuse.rs:1178, edgeuse/p2.rs:1591, heal/stitch p1/p2, blend fillet_removal, fillet_executive p3 and session/finish, offset p1/p2, sweep store | By convention of the Euler-level operators. |
| Lookup-time delete | query/face/p2.rs:587 | Flagged cache dropped when not editing and not cleared. |
| Failed build | query/face/p2.rs:622 | Slot emptied. |
| `SmSurface::Notify(PreEdit)` | `edit_notify` surface p2.rs:5246-5257, used by `swap_uv_notifying` (p2.rs:3528, 3547); `transform_kind_at_depth` (p2.rs:3698-3704) | `delete_stored_trim_cache` (p2.rs:5262): deletes a **`Stored`** cache only; a `CheckedOut` one "stays with its caller" and is stored back afterwards. |
| New surface value | `Surface::clone` (p2.rs:621), `detach_for_edit` (p2.rs:1007), `snapshot_copy` (p2.rs:1024), `Brep::snapshot` | Empty slot; the old cache is simply unreachable. |
| `FaceIntersectCaches::clear` | arena_pair.rs:309 | S5 emptied on every `&mut` lend. |
| `KeptFaceCaches::take` key mismatch | live.rs:176-180 | S4 discarded. |
| New `BrepQuery` | query/brep.rs:641 | B1 + S4 start empty. |
| `brep::p2::notify(PreEdit/Destruction)` | topology/brep/p2.rs:821-825 | Would drop B4; never supplied. |
| `&mut Curve` edit | curve p5.rs:46-55 | C1 starts empty. |
| `Surface::kind_mut` | surface p2.rs:1634-1646 | Resets `boundary_memo`, `singularities_memo`, `continuity_memo`. **Does not touch `cache_slot`.** |
| `delete_stale_caches` | topology/brep/p2.rs:1627; boolean/merge/p4.rs:2024 | Operates on a caller map; the Boolean passes an empty one, so it deletes nothing. |
| a_object `delete_objects_cache` | curve p2.rs:2447; surface p2.rs:5290; poly.rs:3287 | Acts on the vestigial `CacheStore`. |

### 4.2 Edits that do NOT invalidate the face's trimmed cache

"Does not invalidate" means: no `PreEdit` face notification and no slot deletion in the function itself. A caller may notify separately; where it does not, S1 keeps a cache built from the old state.

| # | Edit | Evidence |
|---|---|---|
| 1 | Face tolerance | `face::p3::set_tolerance` (topology/face/p3.rs:422-452): writes `face.tolerance`, syncs the surface's cached tolerance, cascades to edges; no notify. The tree was implanted at the old tolerance. |
| 2 | Edge tolerance | `edge::set_tolerance` (topology/edge.rs:2302-2325): no notify. `SetUVTrimCurve` raises it during a cache build. |
| 3 | Vertex tolerance | `vertex::set_tolerance` (topology/vertex.rs:195-209): no notify. Implanted vertex uses carry `vertex_tolerance` (trim_srf_cache.rs:876). |
| 4 | Vertex position | `vertex::set_point` (vertex.rs:1374-1396): vertex `ChangeGeometry`, which goes to vertex uses (gap caches), not to the face cache flag. |
| 5 | Edge curve replaced | `edge::set_curve` (edge.rs:2838, 2905-2935): edge `ChangeGeometry` + B-rep `RmFromBrep`/`AddToBrep` (edge.rs:2671-2699); no face `PreEdit`. Skipped entirely when `!options.call_notify`. |
| 6 | Edge interval | `edge::set_interval` (edge.rs:394): plain field write. |
| 7 | Edge / edge-use orientation | `edge::set_orientation` (edge.rs:2332), `edgeuse::p1::set_orientation` (edgeuse/p1.rs:323): plain writes. |
| 8 | UV trim curve set, replaced or removed | `edgeuse::p1::set_uv_trim_curve` (edgeuse/p1.rs:715-790): resets `vertex_face_gap` and `drop_curve_fail` only. The cache holds the implanted old curve. |
| 9 | Loop-use wiring | `loopuse::set_edgeuse`, `set_loop`, `set_orientation` (topology/loopuse.rs:359-393): plain writes. Only `loopuse.rs:1178` (`face_notify_edit`) notifies. |
| 10 | Face UV domain | `face::p2::set_uv_domain` (topology/face/p2.rs:295): plain write. The cache's `uv_domain` was intersected with the face domain at build (trim_srf_cache.rs:880-890). |
| 11 | Face surface replaced | `set_surface` / `set_surface_notified` (topology/face/p1.rs:1254-1330): `ChangeGeometry`, not `PreEdit`. Invalidation happens only because a new `Surface` value has another slot; a surface that shares the old slot (an evaluation copy put back) keeps the old cache. |
| 12 | Surface geometry through `kind_mut()` | surface p2.rs:1634: memos reset, slot untouched. |
| 13 | `Surface::reverse`, `reparameterize`, `trim_with_domain` | surface p2.rs:3323, 3430, 3737: no `cache_slot` reference in the region 3323-3760 except `swap_uv_kind_notifying` and `transform_kind_at_depth`. **UNVERIFIED** whether an inner callee notifies. |
| 14 | Face notifications other than `PreEdit` | face/p3.rs:512 onward: `PostEdit`, `SplitInBrep`, `ChangeGeometry`, `Split`, `Destruction`, `MergeInBrep`, `RmFromBrep` do not mark the cache. An edge split in a face (`SplitInBrep`) changes the edge uses the tree names. |
| 15 | Any edit while `editing_enabled` | Marked, but reused (p2.rs:578) until a lookup after editing ends. |
| 16 | Any edit followed by an owner-cleared lookup | Marked, reused (`!cleared` in p2.rs:578). |
| 17 | Surface edit while the cache is checked out | `delete_stored_trim_cache` ignores `CheckedOut` (p2.rs:5264); the lease puts the pre-edit cache back. `face_modified_while_out` covers face edits only. |
| 18 | Edits on a copy that shares S1 | `clone_standing_for_self` working copies (topology.rs:1230-1262; "a working copy an operation offsets or shells", topology.rs:1237): a cache built on the edited copy lands in the original's slot, and a `PreEdit` on the copy flags the original's cache. `clone_for_trial` exists to stop exactly this (topology.rs:1278-1284), and its doc notes nested surfaces and edge-curve surfaces "still share their slots". |
| 19 | Everything, for S3–S7 | Those homes hold plain `SurfaceCache` copies that no notification reaches. They rely on "the B-rep must not have changed since" (live.rs:637-638; face/p3.rs:2647-2648) and on the owner clearing or dropping them. |
| 20 | `Brep::set_tolerance`, kernel tolerance or context changes | topology.rs:1712. S1 has no key at all; S4 and C1 key on kernel state, S1/S3/S5/S6/S7 do not. |

Items 1–10 are the "at least ten edit kinds" at the setter level; 11–20 are structural.

---

## 5. Per-query scratch inside caches

### 5.1 What gets written

| Written state | Where | Who triggers it | Class |
|---|---|---|---|
| Tree subdivision to tolerances (`build_tree`, `subdivide_to_tolerances`, `subdivide_node`, `subdivide_by_block`) | surface_cache/p3.rs:845-1220, p2.rs:844 | Build only. Outside the cache modules, `subdivide_*` is called only from query/tess.rs and geom/grid.rs. | (a) one-time build. Solvers do not refine. |
| Lazy tree build (`get_tree`, `get_tree_with_face`, `face_cache_tree`) | surface_cache/p3.rs:1226; trim_srf_cache.rs:736; query/face/p2.rs:661 | First solve that needs the tree: ~25 call sites (surface p1/p7/p8/p11, intersect surface_intersector/p5, surface_tracer, ray_tracer, grid, offset ×2). | (a) — but deferred only because of `doing_boolean` and editing. Could be made up front. |
| Face implant: edge-use and vertex-use lists in nodes, `on_boundary`, domain shrink | trim_srf_cache.rs:830-916, 944-953 | Trimmed build. | (a), and it **edits the body** (creates UV trims, raises tolerances). |
| Node classes and marks from `mark_face_containment` / `connected_region` (`node_class`, `is_marked`, propagated to parents) | surface_cache/p1.rs:2809, 2898-2971, 3384; tree.rs:1355-1403 | Once, at the end of the trimmed build (trim_srf_cache.rs:909). `connected_region` is called only from p1.rs. | (a). `is_marked` is used as build-time visited state and left set. |
| `face_containment_done` (`NotDone`/`UnderConstruction`/`Done`) | trim_srf_cache.rs:602, 761, 914; surface_cache/p3.rs:1185, 1244 | Build guard against re-entry. | (c) build state; with a pure build it disappears. |
| `have_t_surface_cache`, `face_was_modified` | trim_srf_cache.rs:414-418, 903-905; face/p3.rs:508 | Build; invalidation. | (c) state. Belongs in the key/validity of the cache, not in it. |
| `point_test_enabled`, `process_boundary_curves` flipped and restored | surface_tracer.rs:2103-2110; surface_drop_curve.rs:1719-1726; surface_intersector/p3.rs:447-457, p5.rs:580-581; surface/p7.rs:1718, 1729; hcr.rs:708-709; tess.rs:489-491, 671-679; trim_srf_cache.rs:809-811, 1756-1761 | Each solve, as a parameter of the solve. | (b) per-query **mode**, stored in the cache. This is the reason for the extra clone at p7.rs:1716. Should be solve arguments. |
| `pole_memo` | surface_cache/p1.rs:2007-2017; only inside `subdivide_to_tolerances_with` | Build. | (b) build scratch; `None` outside. |
| `iso_boundary_curves` (+ `iso_curve_tolerance`) | surface_cache/p1.rs:3137-3205 | First request at the cache domain; 3 outside callers (surface_tracer, surface_intersector/p3, surface_drop_curve). Re-made if a tighter tolerance is asked. | (a) pure function of surface, domain, tolerance. Monotonic if keyed by tolerance. |
| Patch boundary bounding planes | surface_cache/p1.rs:809, 2147; copied out at surface_intersector/p5.rs:585 ("copies: lazily computed") | First request. | (a). |
| Polar boxes on patches (`set_polar_box`, `build_polar_box`, `polar_box`) | surface_cache/p1.rs:1285, 2519, 2537; callers adv_surface_intersector ×2, surface p1 ×1, grid ×1 | First request. | (a). |
| Patch `zone_tol_3d` (`&mut self`) | surface_cache/p1.rs:1172, 1193 | First request. | (a) **UNVERIFIED** (not read in detail). |
| Alt caches appended (`add_curves_to_cache`) | surface_cache/p2.rs:404-420; lookup p1.rs:2347 | Silhouette-type results. | (a) memo keyed by `(type, tolerances, key)`; bounded with eviction, so not monotonic as written. |
| Owner tolerance on the cache's surface copy (`set_owner_zone_tol_3d`, `adopt_owner_of`) | surface_cache/p1.rs:1819; query/face/p2.rs:544, 571 | Each lookup refreshes it. | (c) body state mirrored into the cache. |
| Handles (`set_surface_handle`, `bind_trimmed_face`, `rekey_trimmed_face`) | surface_cache/p1.rs:1811, 1836, 1844 | Per consumer copy. | (b) view data. |
| Construction data (`subdivision_surfaces`, patch Beziers) cleared after build | surface_cache/p3.rs:1203-1215; trim_srf_cache.rs:922 | Build. | (b) build scratch. |
| Visit lists | `CachedVisits` (curve_class/p9.rs:140) | Per classification. | (b), **already outside the cache**; topology marks go through `Context::claim_mark` (geom/context.rs:808). |
| Topology solver object trees | intersect/topology_solver/p1.rs:854-855, p2.rs:825-830 | Per solve, its own trees. | (b), already separate. |
| `test_on` latch | point_class.rs:2155 | Per solve. | (b), already separate. |
| Tess marks, classes, subdivision | query/tess.rs:889-1002, 1158 | Tessellation, on its own `TessSrfCache`. | (c) for tess; not shared with solvers. |
| B-rep spatial tree add/remove | topology/brep/p2.rs:1400-1540 | Would follow notifications; unreachable (B4). | — |

The global solver itself takes `&Tree<O>` (geom/global_solver.rs:506, 600, 612, 635, 765); its `subdivide` picks existing children. No solver marks nodes.

### 5.2 `&mut` versus `&`

| Type | `&mut T<..>` parameters | of which `Option<&mut ..>` | `&T<..>` parameters | `&mut self` methods | `&self` methods |
|---|---|---|---|---|---|
| `SurfaceCache` | 39 | 12 | 43 | ~45 (surface_cache p1 35, p2 2, p3 8) | ~44 |
| `TrimSrfCache` | 8 | 2 | 3 | 17 | 17 |
| `CurveCache` | 1 (+ ~22 `&mut Option<CurveCache>` lazy slots) | — | 2 | 6 | 21 |
| `BrepCache` | 0 | — | 1 | 3 (tree_mut accessors, unused live) | 25 |
| `Tree` | 12 | 4 | 65 | 8 | 37 |

Of the `&mut SurfaceCache` parameters, the ones read in detail (surface/p8.rs:1609-3035, surface_drop_lib/p1.rs:1047-1200, plane/p1.rs:1142, surface_intersector/p3.rs:412-465, p5.rs:485-533) need `&mut` for three things only: build the tree if missing, flip the two mode flags, fetch lazily computed iso curves / planes / polar boxes. **UNVERIFIED** for the remainder, but the absence of `subdivide_*`, `node_mut`, `aux_mut`, `mark_face_containment` and `connected_region` calls outside the cache modules, tess and grid supports it.

### 5.3 Answer

"Immutable built cache + per-query scratch" is feasible on this evidence:

- Solvers never refine or mark the tree.
- The mode flags become solve arguments.
- Iso curves, bounding planes, polar boxes and alt caches become `OnceLock`-style monotonic fields or a side memo keyed by tolerance.
- The tree build becomes eager (or a `OnceLock`), which requires removing the `doing_boolean` deferral and the editing-time standalone build — both of which exist only because the build edits the body and the body may be mid-edit.

The real obstacle is not scratch. It is that the *build* is impure (section 7).

---

## 6. Handle spaces

### 6.1 Types in use

| Handle `O` | Consumer |
|---|---|
| `EntityRef` | S1, S2, the face lookups, `PointClassifyOptions::trim_cache` |
| `CacheObject` (`Vertex/Edge/Face/Curve/Surface/Brep/Other`) | `BrepCache` default, S3–S6, ray and live solves (18 uses of `SurfaceCache<CacheObject>`) |
| `H` (arena pair handle: operand index, kind, index, and an interval for curves) | Boolean: B2, S7 |
| `SideObject`, `CurveFaceObject` | face/face and curve/face services (live.rs:878, 999) |
| `SolveObject` | `TrimmedFaceContext` (surface_intersector/p3.rs:2010) |
| `IntersectionSurface::{First,Second}`, `NoSurfaceHandle` | context-free SSI (surface_intersector/p5.rs) |
| `Handle` (offset: arena + object) | offset edge-use solves |
| `FilletObject` | fillet session, insertion geometry intersector |
| `usize` | HCR tracer |
| `TessHandle` | tessellation |
| `NoObject`, `()`, `SilhouetteObject`/`SilhouetteHandle`, `TraceCacheObj` | context-free surface solves |
| `CurveHandle`, `OperandObject`, `PieceHandle`, `OrigHandle`, `Side` | curve caches |

### 6.2 Why caches are generic and copied

- The tree stores handles **in its data**: `owner_surface`/`owner_curve`, `NodeData::Surface(aux).surface` and `.face`, per-node edge-use and vertex-use lists, `ObjectList.object`, `subdivision_surfaces: Vec<(O, Surface)>`, `trimmed_face: Option<O>`.
- The global solver copies those handles into `Solution<O>` and compares them by `PartialEq` (`LocalSolve` checks "the original surface of the leaf node is the surface of the cache the solver was given", surface/p8.rs doc 59-60; the intersectors "compare the two surfaces by identity", face/p3.rs:2767).
- `SolutionObject` makes a handle carry *facts*: `kind()`, `natural_interval()`, `natural_uv_domain()`, `vertex_point()`, `poly_vertex_point()` (geom/solution_array.rs:289-314). `IdenticalSolutions` scales its merge tolerance by the object's natural interval. So `H` embeds an interval (arena_pair.rs:171-179) and `OrigHandle` *is* the interval.
- Two operands, or the two sides of one intersection, must be distinguishable in one solve, hence `SideObject`, `IntersectionSurface`, operand index in `H`.
- Each solve therefore gets a private copy in its space: `SurfaceCache::map_handles` is a full deep copy (surface_cache/p1.rs:1873-1960). Only a curve tree's `curve_data` is `Arc`-shared across retags (geom/tree.rs:441, 589).

### 6.3 Could one cache keyed by stable ids serve everyone through a thin view?

Yes for the data; the handle is only stored, never computed with (curve_cache.rs:369-372 says so for curves, and C1 already stores one `()`-tagged cache and retags it).

Shape: the cache stores a small local id per named object (`Surface`, `Face`, `Loopuse(i)`, `Edgeuse(i)`, `Vertexuse(i)` — exactly the set `cache_entity` accepts, ray.rs:152-170). A view `CacheView<'a, O> { cache: &'a Cache, map: &'a dyn Fn(LocalId) -> O }` supplies `O` when a solution is emitted.

What breaks or needs work:

1. **Every `Tree<O>` reader** (65 `&Tree` parameters, 37 `&self` methods) reads `O` out of nodes. They need the view, or the solver must be generic over the id and map at solution-emit time. This is the bulk of the mechanical work.
2. **`SolutionObject` facts** must come from the view, not the id. Fine for intervals and kinds; the view needs the body.
3. **Identity comparisons** between a node's surface and "the solve's surface" become id comparisons inside one cache — simpler — but two-tree solves need the pair (cache index, local id).
4. **Stable ids must survive body copies.** Today `copy_cache_slot_of` renames handles for a B-rep copied into new arenas and re-keys `trimmed_face_key` (surface p2.rs:1531-1556; surface_cache/p1.rs:1836). Local ids make that a no-op, provided `FaceId`/`EdgeuseId` do not leak into the cache.
5. **`subdivision_handles: Option<fn(u64) -> O>`** (surface_cache/p1.rs:1667) names construction halves during the build; build-only, can stay local.
6. **`TessSrfCache`** stores `PolyEdge` handles and a `PolyBrep`; it is a different cache and should stay one.
7. **`BrepCache<H>`** is built directly in `H` with a non-trivial mapping that reads the body (arena_pair.rs:1926-1945). A view works the same way.
8. `trimmed_face_key: Option<ObjectRef>` and the cache's own `Surface` copy with its owner are identity checks against the body (`check_face_binding`). With a body-keyed cache they are redundant.

Nothing found requires distinct *data* per handle space.

---

## 7. Restart protocols and side tables caused by impure cache builds

Root fact: building a face's trimmed cache calls `create_uv_trim_curves` on the body and may raise edge, vertex and face tolerances (trim_srf_cache.rs:830-840; `CacheFace` at trim_srf_cache.rs:226-325; query/face/p2.rs:539-546 "Creation may raise the owning face's tolerance even on an error"). Classification does the same lazily (`GetOrCreateUVTrimCurve`). So a read-only solve cannot build what it needs.

| # | Mechanism | Where | If cache building were a pure function of an immutable body |
|---|---|---|---|
| 1 | `SolidError::PreparationRequired` + `FaceCaches.pending: Cell<Option<(O, bool)>>`; the solve is abandoned, the cache is built on the owner's B-rep, the solve is re-run from its start | boolean/topology_intersector/p1.rs:4270-4274, 4304-4308, 4704-4776 | A `get_or_build(face)` on a concurrent map; no restart, no `pending`, no "made no progress" guard (p1.rs:4750-4754). |
| 2 | Two maps `built` and `states`, plus `tree_requested: RefCell<BTreeSet<O>>` | p1.rs:4196-4208, 4297-4300 | One cache per face. The with-tree / without-tree distinction exists only to reproduce the state bits of a tree-less cache made during a Boolean. |
| 3 | `trims: RayPreparation` + `classified_faces: RefCell<Vec<O>>` + `trim_request_face` / `trim_owner_face`: a classification needs a UV trim, the solve stops, the face owning the edge use is searched among the faces classified so far, the trim is made, restart | p1.rs:4202-4208, 4340-4380, 4641-4701, 4729-4741 | Trims are part of the built cache (or of the body, made in a preparation pass). `classified_faces` and the owner search disappear. |
| 4 | `RayPreparation { pending, trim_brep, attempted, failed_trims, surface_caches }` and its restart loops | query/brep/ray.rs:53-126; loops at offset/offset_executive/p1.rs:3348-3378, topology/shell.rs:1099-1113, topology_intersector/p1.rs:4720-4741; `trim_brep` compares B-rep **addresses** (ray.rs:71-78) | Same as 1 and 3. `failed_trims` becomes a field of the cache ("this edge use has no UV trim"). |
| 5 | `QueryTopology::{Mutable, Prepared, Requested}` and `PreparedFaceQueries` — three code paths through `point_classify` depending on whether the caller can edit | query/face/p2.rs:880-965, 1046-1056 | One read-only path. |
| 6 | `solve_with_face_caches_keeping_trees` → `keep_face_cache_tree`: after the solve, build S1 again for each face so later classification takes the fast path | p1.rs:4585-4617 | Gone: one home. |
| 7 | `ArenaPair::clear_face_caches` on every mutable lend, including the lend that builds another face's cache | arena_pair.rs:303-313 and call sites | Gone: building does not lend the body mutably. Invalidation becomes "the body value changed". |
| 8 | `BrepFaceServices { brep: RefCell<&mut Brep>, caches: RefCell<FaceCaches> }` with `try_borrow_mut` and `borrow_conflict` errors | query/brep/ray/live.rs:615-760 | `&Brep` and a shared cache map. No runtime borrow failures. |
| 9 | `KeptFaceCaches` take/put with `Cell<Option<..>>`; "a solve nested in another starts without them" | live.rs:144-196 | A shared map; nested solves see the same caches. |
| 10 | Slot `CheckedOut`, `check_out_trim_cache` before the build, `face_modified_while_out`, "a nested lookup builds a second cache that is not kept" | surface p2.rs:474-498, 1442; query/face/p2.rs:589-608 | Gone. The checkout exists because the build re-enters lookups of the same face while mutating the body. |
| 11 | Owner-cleared scopes: `Brep.surface_owner_cleared`, slot `cleared_for`, `begin/end_owner_cleared`, `keep_standalone_trim_cache`, `restore_surface_owner` | topology.rs:1170-1183, 1468; surface p2.rs:1470-1509; edgeuse/p3.rs:646; face/p3.rs:1274; query/face/p2.rs:2018-2028 | Gone. They model "during trim creation the surface has no face", i.e. the build observing its own half-done edit. `Brep::snapshot` refuses to run inside one (topology.rs:1341-1345). |
| 12 | `FaceContainment::UnderConstruction` and the save/restore of `point_test_enabled` around builds "to prevent bad recursion back into BuildTree" | surface_cache/p3.rs:1181-1189, 1244-1246; trim_srf_cache.rs:757-763, 806-811 | Gone with a non-re-entrant build. |
| 13 | Private B-rep copies so a `&Brep` query can make trims: `CurveOnClassifyCache.snapshot`, `LazyBrep::Shared`, `clone_sharing_surface_caches` scratch copies, `WithMadeTrimCurves`, and the selective copy-back (`apply_gap_updates`, `apply_uv_trim_updates`) | query/face/p2.rs:1544-1565, 41-55, 144-155; query/brep.rs:1062-1108; boolean/merge/p4.rs:794; curve_class/p4.rs:507, 579; face/p4.rs:2410; vertexuse.rs:2396 | Gone. Each is a whole-body copy per query whose edits are mostly thrown away. |
| 14 | `clone_for_trial` copying every slot so a trial "starts from this B-rep's cache state" | topology.rs:1278-1307 | The trial is a new body value with its own (initially shared, immutable) caches. |
| 15 | `doing_boolean` deferral of the tree and the "state bits before `GetTree`" API (`surface_cache_without_get_tree`, `surface_cache_state_of_face`, `prepare_face_surface_cache_state`, `cache_state_mapped`) | surface p1.rs:1040-1052, 1196-1214; query/face/p2.rs:488-510; face_context.rs:346-366; live.rs:706-733 | One eager, complete cache. This whole API reproduces an accident of upstream's laziness; removing it changes results where the fast-drop test's on/off bit differs (history case 3). |
| 16 | Deviation in `surface_cache_copy_of_face`: for a tree-less cache, make the face's UV trims now because the reader's classification is read-only | query/face/p2.rs:534-547 | Gone. |
| 17 | `FaceCacheKey` / `CurveMemoKey` including `doing_boolean` and `editing_enabled` | live.rs:117-140; curve p5.rs:384-430 | Keys shrink to tolerances + fixes (+ body version). |
| 18 | Failure memo in S7 (`Result` stored) and in `HeldSolveCache` | p1.rs:4198; surface p8.rs:127 | A pure build's failure is a cacheable value in the one home. |

Outside the Boolean the same protocol appears in offset (`solve_original_brep_point`, offset p1.rs:3341-3380), shell region nesting (topology/shell.rs:1092-1116), the fillet insertion intersector (`face_cache_target`, blend/fillet_executive/session/insertion/geometry/intersector.rs:492) and HCR.

The cost of making the build pure is that **UV trim curves and the tolerance raises they imply must exist before the cache is built**, as a separate, explicit step that edits the body ("prepare face"), or the cache must carry its own UV trims and fitted tolerances without writing them to the body. Which of the two is right is not decidable from this code alone: other code reads `edgeuse.uv_trim_curve` and the raised tolerances directly (**UNVERIFIED** how much; `TrimCoverRaises` at topology.rs:1902 suggests the raises are tracked).

---

## 8. Proposed target shape

From the evidence only.

```
Level            Cache                               Key                                   Notes
---------------  ----------------------------------  ------------------------------------  ---------------------------------------
curve value      CurveCache (spans + tree)           (kernel tol+fixes, owner zone tol)     Exists (CurveMemo). Make hits zero-copy:
                                                                                            Arc<CurveCache<LocalId>> + view, not retag.
surface value    SurfaceTree: Bezier decomposition,  (kernel tol+fixes, uv domain)          NEW. Today rebuilt per solve. No face,
                 continuities, patches                                                      no owner, no handles. Arc, immutable.
surface value    lazy monotonic extras: iso          + approx tol for iso curves            OnceLock / tolerance-keyed map beside
                 boundary curves, bounding planes,                                          the tree. Alt (silhouette) caches:
                 polar boxes                                                                same, keyed by (type, tols, key).
face (of a body  FaceTrim: SurfaceTree + implanted   (body version or face version,         Replaces S1, S3-S7. Built by a pure
value)           boundary + node classes + face      kernel tol+fixes)                      function of an immutable body whose UV
                 domain; or Err(build failure)                                              trims already exist. Stores LocalIds.
body value       BrepCache (3 spatial trees), bbox,  (body version, kernel tol+fixes)       Replaces B1/B2/B3. One per body value,
                 per-face seam flags, FaceTrim map                                          shared by all workers.
operation        handle views: LocalId -> O, and     per operation                          Replaces map_handles copies. Also holds
context          SolutionObject facts                                                       the operands' Arc<body caches>.
thread scratch   solve mode (point test on/off,      per solve                              Replaces flag flipping on the cache and
                 process boundary curves), visit                                            the clone at surface/p7.rs:1716.
                 lists, object trees, test_on latch
```

Rules that follow:

1. One home per cached thing; no take/put. Workers hold `Arc`s; the editing thread makes a new body value and drops or carries caches by key.
2. Invalidation is by key (body/face version), not by `face_was_modified` plus mode flags. The 20 non-invalidating edits of §4.2 stop mattering only if **every** low-level setter bumps the version; today invalidation hangs on ~80 operator-level `PreEdit` calls.
3. The trimmed build must not edit the body. UV trim creation and tolerance raising become an explicit preparation pass on the editing thread.
4. No tree-less caches, no `doing_boolean` deferral, no owner-cleared state, no `UnderConstruction`.

Where the evidence is insufficient:

- **Granularity of the face key.** Whether a per-face version is maintainable depends on how many writers bypass notifications (§4.2 shows the setters do; how many callers there are was not counted).
- **Result changes.** Removing history cases 1–3 and 6 of §3.6 changes which solutions the fast-drop point test keeps and some classification results. Goldens will move; how many is unknown.
- **Prepare pass scope.** Whether trims can be created for all faces up front without changing tolerance outcomes that today depend on the order of lazy creation was not examined.
- **Sub-domain caches.** `SurfaceCache::new` takes a `uv_domain`; whether solvers need trees over sub-domains (extended surfaces in offset, `extended_surfaces` in the fillet session) often enough to need a domain in the key was not measured.
- **Memory.** One eager tree per face of a large body versus today's on-demand copies was not estimated.
- **Tess.** `TessSrfCache` subdivides to tessellation tolerances and marks nodes; it should probably stay a separate per-tessellation structure seeded from the shared `SurfaceTree`. Not examined beyond query/tess.rs's field writes.
- **`BrepCache` incremental update.** The unused `BrepCacheSlot` path shows upstream updated the spatial trees in place on split/merge. Whether rebuild-per-body-value is affordable inside an operation that makes many small edits was not measured.
