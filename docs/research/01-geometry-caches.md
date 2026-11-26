# 01 - Geometry-side caches (Surface, Curve, nurbs/)

Checkout: a local worktree at 782ef9afc. Paths are relative to `solidcore/crates/solidcore/src`.
Read-only investigation by grep and line-range reads; nothing was built or run. Anything not confirmed in code is marked **UNVERIFIED**.

Classification used throughout:

- **PURE** = pure memo: a deterministic function of the owner's own immutable value plus explicit parameters.
- **CONTEXT** = context memo: also depends on something outside the owner (face trims, owner tolerance, kernel tolerances or flags, options, earlier calls).
- **NOT-A-CACHE** = mode flag, in-use marker, identity, progress, or data that changes results / cannot be recomputed if dropped.

## 0. Index of everything found

| # | Item | Defined | Held by | Class (worst piece) |
|---|------|---------|---------|---------------------|
| S1 | `Surface.owner`, `owner_chain_face`, `owner_zone_tol_3d` | surface/surface/p2.rs:430-441 | `Surface` | NOT-A-CACHE (topology handle + face tolerance mirrored into the value) |
| S2 | `CacheSlot(Arc<Mutex<SlotState>>)` / `TrimCacheSlot` | surface/surface/p2.rs:474-529 | `Surface.cache_slot` | NOT-A-CACHE (holds a CONTEXT cache, an in-use marker, a deferred flag, a scope stack, and serves as object identity) |
| S3 | `MadeKindSlot(Arc<OnceLock<Box<SurfaceKind>>>)` | surface/surface/p2.rs:549 | `Surface.made_kind` | PURE in value, but observable ("has a NURB") |
| S4 | `BoundaryMemo(OnceLock<Box<..>>)` | surface/surface/p8.rs:394 | `Surface.boundary_memo` | PURE (keyed by kernel state + domain) |
| S5 | `SingularitiesMemo(Mutex<Vec<..>>)` | surface/surface/p3.rs:648 | `Surface.singularities_memo` | PURE given key; key includes face tolerance (CONTEXT input made explicit) |
| S6 | `ContinuityMemo(OnceLock<Arc<Mutex<Option<..>>>>)` | surface/surface/p6.rs:161 | `Surface.continuity_memo` | PURE (keyed by kernel state) |
| S7 | `Surface.notify_identity: IdentitySlot` | geom/context.rs:303 | `Surface`, `Curve` | NOT-A-CACHE (issued identity) |
| S8 | `Surface.definition: Option<Box<KeptDefinition>>` | surface/surface/p2.rs:467 | `Surface` | NOT-A-CACHE (it is the definition; dropped by evaluation copies) |
| S9 | `SurfaceCache<O>` | surface/surface_cache/p1.rs:1630 | callers, `TrimSrfCache.base`, kernel/topology stores | mixed: CONTEXT + NOT-A-CACHE |
| S10 | `AltSrfCache` / `AltCacheKey` | surface/surface_cache/p1.rs:1354,1380 | `SurfaceCache.alt_caches` | CONTEXT (fuzzy key, LRU of 1 call history) |
| S11 | `PatchBoundaryBoundingPlanes` | surface/surface_cache/p1.rs:669 | `SurfaceCache.bounding_planes` | PURE (lazy, `&mut`) |
| S12 | `PoleMemo` | surface/surface_cache/p1.rs:427 | `SurfaceCache.pole_memo` | PURE, scoped to one build |
| S13 | `TrimSrfCache<O>` | surface/trim_srf_cache.rs:391 | `TrimCacheSlot::Stored`, face caches | CONTEXT (face trims, tolerances, B-rep editing flag) |
| S14 | `CacheTolerances` / `SurfaceCacheOptions` | surface/surface/p1.rs:915, surface_cache/p1.rs:1525 | `SurfaceCache.opts` | PURE (computed from surface + kernel) |
| S15 | `HeldSolveCache<O>` | surface/surface/p8.rs:127 | caller local (`topology_traverser.rs:744`) | holder only |
| S16 | `Kernel.blend_net_tolerance: Cell<f64>` (`NetScope`) | surface/procedural_blend.rs:155, kernel.rs:874 | `Kernel` | NOT-A-CACHE (ambient mode changing `Surface::evaluate`) |
| S17 | `PendingExtension(Mutex<Option<Box<Surface>>>)` | surface/offset_surface.rs:269 | `OffsetSurface` | see section 7 |
| C* | curve side | see section 5 | | |
| N* | nurbs side | see section 6 | | |

---

## 1. The `Surface` value (surface/surface/p2.rs:427-468)

```
kind: SurfaceKind                      // the geometry
owner: Option<ObjectRef>               // S1
owner_chain_face: Option<ObjectRef>    // S1
owner_zone_tol_3d: Option<(ObjectRef, f64)>  // S1
cache_slot: CacheSlot                  // S2
notify_identity: IdentitySlot          // S7
made_kind: MadeKindSlot                // S3
boundary_memo: BoundaryMemo            // S4
singularities_memo: SingularitiesMemo  // S5
continuity_memo: ContinuityMemo        // S6
definition: Option<Box<KeptDefinition>>// S8
```

### 1.1 What each kind of copy does to each field

| Field | `Clone` (p2:612) | `clone_for_evaluation[_depth]` (p2:637,920) | `clone_exact_bounded` (p2:1030) | `clone_keeping_owner` (p2:1773) | `clone_sharing_cache` (p2:1577) | `snapshot_copy` (p2:1021) | `detach_for_edit` (p2:1007) |
|---|---|---|---|---|---|---|---|
| `kind` | `self.kind().clone()` = copy constructor, **runs the knot fix-up** and copies the *made* kind if one exists (p2:617) | exact copy (`clone_exact`), nested surfaces by `clone_for_evaluation(depth+1)` | exact | as `Clone` | as `Clone` | exact | unchanged |
| owner triple | reset to `None` (p2:618-620) | **kept** (p2:966-968) | kept (p2:1073-1075) | kept (`restore_owner_state`, p2:1763) | reset | kept | unchanged |
| `cache_slot` | **new empty** (p2:621) | **shared Arc** (p2:969) | **shared Arc** (p2:1076) | new empty | **shared Arc** (p2:1579) | new empty (p2:1024) | new empty |
| `notify_identity` | empty | empty | empty | empty | empty | `duplicate_for_snapshot()` (p2:1025) | unchanged |
| `made_kind` | new empty (but the made kind was folded into `kind`) | **shared Arc** (p2:971) | **shared Arc** (p2:1078) | new empty | new empty | shared | detached (`detach_nurb_cells`, p2:985) |
| `boundary_memo` | empty | empty (p2:972) | empty | empty | empty | empty | unchanged |
| `singularities_memo` | empty | empty (p2:973) | empty | empty | empty | empty | unchanged |
| `continuity_memo` | empty (p2:626) | **shared** (p2:974) | **shared** (p2:1081) | empty | empty | shared | unchanged |
| `definition` | cloned | **dropped** (`None`, p2:975); `clone_for_evaluation_keeping_definition` (p2:653) re-adds it | cloned | cloned | cloned | cloned | unchanged |

Consequences that matter for a redesign:

- There is no single "copy". Five different copies exist and they disagree on every cache field. `clone_for_evaluation` is called at 293 sites crate-wide (66 surface, 56 topology, 46 blend, 38 query, 37 intersect, 22 curve, 16 offset, 5 boolean, 4 primitive, 3 topology.rs); `clone_exact()` 114; `clone_exact_bounded` 64; `clone_keeping_owner` 9.
- `Clone` is **not value-preserving**: it runs upstream's copy-constructor knot fix-up ("moves near-coincident knots of a fillet surface", surface_cache/p1.rs:1744-1745) and a torus resets its inside-out flag (p2:584). So `clone()` != `clone_exact_bounded()` as geometry. Memos that are "shared by exact copies, emptied by Clone" (ContinuityMemo) encode this.
- Evaluation copies share `cache_slot` and `made_kind` precisely so that a copy "stands for the same surface object" as upstream's pointer (p2:499-507, 541-547). The Arc is the port's replacement for pointer aliasing.
- Edits: `kind_mut()` (p2:1634) moves the made kind in, detaches Plane/Extrusion NURB cells, and resets the three memos, but **does not touch `cache_slot`**. `transform_at_depth` (p2:3649) resets the three memos and deletes a *Stored* trim cache only if the kind sends `PreEdit` (p2:3691-3703: "a surface of revolution without NURB sends none"). `swap_uv_notifying` deletes it via `edit_notify` (p2:5246). A cache that is `CheckedOut` at the time survives the edit with its caller (p2:5260-5268).

### 1.2 S1 - owner data on the value: see section B.

### 1.3 S7 - `notify_identity: IdentitySlot` (geom/context.rs:291-303)

`Option<Issued{domain: Arc<DomainToken>, value: u64}>`. Filled through `&mut` (`notify_identity_slot`, p2:1231) by the context of the first notification that names the object. NOT-A-CACHE: an issued identity, not derivable; every copy starts empty except `snapshot_copy`. It makes two otherwise equal surface values distinguishable. Mirrors upstream object address. Send + Sync (Arc of a ZST token and a u64).

### 1.4 S8 - `definition`

Not a cache: the procedural definition of which the B-spline net is the certified approximation. Recorded here only because `clone_for_evaluation` silently drops it (p2:975), so "evaluation copy" and "the surface" are different values for any code that reads `procedural_blend()`; `Brep::standing_copy(keep_definitions)` (topology.rs:1253) exists to paper over that.

---

## 2. S2 - `CacheSlot` / `SlotState` / `TrimCacheSlot`

Definition: surface/surface/p2.rs:474-529. Held in `Surface.cache_slot`. Mirrors upstream `SmAObject::m_pCacheObj` + the `SM_OC_TRIMSRF` entry of the global `SmCacheMgr` queue (p2:470-472), plus `SmCacheCheckOutIn`.

| Piece | Stores | Class | Evidence |
|---|---|---|---|
| `SlotState.trim = Stored(Box<TrimSrfCache<EntityRef>>)` | the face's trimmed surface cache, handles in the face's topology space (`EntityRef::Face/Edgeuse/Vertexuse`) | **CONTEXT** | built from the face's loops, UV trim curves, vertex tolerances and `Brep::editing_enabled` (trim_srf_cache.rs:797, 830-917); tree deferred when `k.context.doing_boolean()` (surface/surface/p1.rs:1045) |
| `SlotState.trim = CheckedOut` | in-use marker | **NOT-A-CACHE** | query/face/p2.rs:589-602: a nested lookup that sees `CheckedOut` builds a second, unkept cache; topology/vertexuse.rs:1033 and topology/brep/p1.rs:1944 skip the lookup entirely and then run with `test_on = false` |
| `SlotState.face_modified_while_out: bool` | deferred "face was modified" for a cache that is out | **NOT-A-CACHE** | applied at put-back, p2:1392-1394 |
| `SlotState.cleared_for: Vec<FaceId>` | stack of active `CreateUVTrimCurve(s)` scopes that "cleared the owner" | **NOT-A-CACHE** (dynamic scope state stored on a geometry value) | p2:1470-1481; read at p2:1499-1501 to decide whether a standalone cache gets *kept as the face's* |
| The `Arc` pointer itself | object identity | **NOT-A-CACHE** | `shares_cache_slot_with` used as "same surface object": blend/fillet_solver/trace_reach.rs:154, query/face/p2.rs:461, p2:1504 |

**Key / validity.** No key. One entry per slot. Validity is tracked by hand:
- deleted on `SmSurface::Notify(PRE_EDIT)` of *some* edits (swap UV p2:5254; transform p2:3703) - only when `Stored`;
- `face_was_modified` set by `SmFace::Notify(PRE_EDIT)` (topology/face/p3.rs:503-511) and then: reused anyway while `b.editing_enabled()`, deleted and rebuilt otherwise (query/face/p2.rs:578-587; intersect/point_class.rs:2481);
- **not** invalidated by: `kind_mut()` edits; owner change (`set_owner`); face tolerance change (`set_owner_zone_tol_3d` updates the surface, the cache's own surface copy is re-synced only on the next lease, query/face/p2.rs:571 `adopt_owner_of`, 528-531 `bind_face`); kernel tolerance / context flag changes (no kernel key at all); topology edits that do not send `PreEdit` to the face.

**Fill.** Through `&self` (`Mutex`), lazily, by `query::face::p2::surface_cache_of_face` (query/face/p2.rs:556) and by `keep_standalone_trim_cache` (p2:1495, called from `make_trim_srf_cache`, surface/surface/p1.rs:1076). Building needs `&mut Brep` because it creates the face's UV trim curves (trim_srf_cache.rs:832) and may raise the face tolerance (trim_srf_cache.rs:833-837).

**Take / put protocol.**
- Takers: `surface_cache_of_face` (query/face/p2.rs:568; released by `FaceSurfaceCache::release`, :395), `query/face/p2.rs:1127/1137`, `face_point_test_on` (intersect/point_class.rs:2477/2483, take-read-put), `copy_cache_slot_of` (p2:1536).
- While out, the cache's own surface copy is re-pointed at the shared slot (p2:1374) and at put-back gets a private slot again "so that the slot does not own itself" (p2:1390) - an Arc cycle avoided by hand.
- A nested observer sees `CheckedOut` and: builds a throwaway cache the same way (query/face/p2.rs:589-602), or gets `InvalidInput` (`copy_cache_slot_of`, p2:1538), or gets `Unported` (`face_point_test_on`, intersect/point_class.rs:2486-2495), or disables point testing (vertexuse.rs:1033-1041).
- Error between take and put: `FaceSurfaceCache` has no `Drop`; `release()` must be called explicitly (query/face/p2.rs:395). `surface_cache_copy_of_face` runs the body in a closure and releases after (query/face/p2.rs:527-553), but any caller that returns early with `?` while holding a `FaceSurfaceCache{kept: true}` leaves the slot `CheckedOut` forever: later lookups then silently take the "in use" branch. (A guard type exists in one place only: `FaceSurfaceCacheGuard`, query/face/p1.rs:882.) A failed build resets the slot to `Empty` (query/face/p2.rs:622).

**Copy semantics.** Shared by evaluation/exact copies, `clone_sharing_cache`, `Brep::clone_sharing_surface_caches` (topology.rs:1194-1213); fresh on `Clone`, `snapshot_copy`, `detach_for_edit`. `Brep::clone_for_trial` detaches then **deep-copies the stored cache with handle mapping** (`copy_cache_slot_of`, topology.rs:1291-1306 -> p2:1531-1557: `map_handles`, `adopt_owner_of`, `rekey_trimmed_face`).

**Can clearing it change a result?** Yes, in several ways:
1. `face_point_test_on` returns `Unported` when no cache is kept, one is in use, or the kept one is stale (intersect/point_class.rs:2473-2495) - an *error vs. answer* difference decided purely by cache presence.
2. "Upstream also reuses a cache an earlier call built, with the built tree's bits; the port keeps no record of that" (query/face/p2.rs:492-493) and "The bits then depend on whether the face's cache was built before" (surface/surface/p1.rs:1198-1200): `point_test_enabled` differs between a cache with and without its tree (trim_srf_cache.rs:412-417 vs 582-586/899), and `Surface::global_point_solve` reads those bits to decide whether to cull solutions (surface/surface/p4.rs:1738-1740).
3. A kept cache carries `face_was_modified`; dropping it resets that to false, which re-enables point testing (trim_srf_cache.rs:650).
4. Whether the kept cache "was built with the face or not" is not re-checked: "a cache the surface already has is reused as it is (whether or not it was built with the face)" (query/face/p2.rs:407-409). A standalone cache adopted via `cleared_for` (p2:1499-1506) has `point_test_enabled=false`, a face-built one has `true`.

**Thread safety.** `Arc<Mutex<..>>` is Send + Sync if `TrimSrfCache<EntityRef>` is Send. Concurrent readers of one `&Surface` would be *serialised but order-dependent*: first thread takes `Stored`, the others see `CheckedOut` and take a different code path (rebuild unkept / Unported / test off). This is the single worst item for fork-join.

---

## 3. S3 - `MadeKindSlot`

Definition p2:531-579; helper `clone_kind_exact` p2:585. Mirrors upstream `GetOrCreateGwNurbPointer` called from `const` methods.

- **Stores**: a whole second `SurfaceKind` (cone / sphere / torus / surface of revolution) with its NURB made. Not just the NURB: `MakeNurbWithSweepParams` "may trim the generator (`AdjustSTEPInterval`) and it rebuilds the polar converter" (p2:535-537).
- **Class**: PURE as a value ("depends only on the surface's own data", p2:547) - but takes `k: &Kernel` (`make_nurb(k)`, p2:1686-1689), so strictly PURE-given-kernel; **UNVERIFIED** which kernel fields `make_nurb` reads. Not keyed by kernel.
- **Fill**: `&self`, `OnceLock::get_or_init` (p2:566), lazy, entry `Surface::kind_with_nurb` (p2:1659) and everything named `*_with_nurb` (`natural_uv_domain_with_nurb`, `degree_with_nurb`, ...). A failed make is *not* stored but its status is ignored (`ignore_status`, p2:1686) so a half-made kind can be stored (**UNVERIFIED** whether a failing `make_nurb` leaves `kind` partially modified).
- **Observable**: `Surface::kind()` returns the made kind once filled (p2:1604-1606), so `kind()` readers, `Clone` and `operator==` see a *different value* before and after the first `kind_with_nurb`. Kernel-free readers answer differently: `natural_uv_domain_if_built()` returns "its NURB-less base's empty domain" for a lazy analytic (p2:2330-2352; used at trim_srf_cache.rs:408-410 to initialise `SurfaceCache.uv_domain`, curve_bounded_surface.rs:316, srf_in_volume.rs:547, offset_surface.rs:602, surface/p5.rs:2544). `type_code()` and `clone_for_evaluation_depth` read `self.kind` (the *unmade* one, p2:922, 1865) while `kind()` reads the made one.
- **Copies**: shared by evaluation/exact copies; `Clone` folds the made kind into the copy's `kind`; `take_for_edit` (p2:573) moves it in on any `&mut` access, cloning it if the Arc is shared.
- **Invalidation**: `kind_mut`, `transform`, `into_kind`, `detach_nurb_cells`.
- **Clearing changes a result?** Values no; but "has a NURB" is read as state in ~10 places (`gw_nurb().is_none()` p2:1664-1671, `natural_uv_domain_if_built`). With an eager "always made" value this disappears.
- **Thread safety**: OnceLock - safe; two racing threads both compute, first wins, both see the same value. Send + Sync.
- Plane and SurfOfExtrusion do not use this slot; they keep the NURB in their own `NurbSlot` (section 6).

---

## 4. Surface memos (all "Not upstream")

All three are gated by `Kernel::memoize_curve_caches` (kernel.rs:851; default on, off with feature `curve-memo-off`, kernel.rs:1003) and by a "pure kind" whitelist: **BSpline, Plane, Cone, Sphere, Torus only** (p3:614-624, p6:220-227, p8:354-361). Every other kind (revolution, extrusion, offset, STEP, curve-bounded, in-volume, procedural blend, ball patch) is recomputed each call, with the stated reason "the other kinds hold curves, other surfaces or volumes with state of their own".

Shared key part `CurveMemoKey::of_kernel` (curve/curve/p5.rs:407-423): `k.tolerances`, `context.this_zone_tol_3d`, `stored_model_size_estimate`, `stored_large_small_size_ratio` (bits), `context.doing_boolean()`, `k.fixes` (the whole `UpstreamFixes`). This is the code's own statement of which kernel state geometry builds read.

| | S4 `BoundaryMemo` | S5 `SingularitiesMemo` | S6 `ContinuityMemo` |
|---|---|---|---|
| Def | p8:394; key p8:342; entry p8:379 | p3:648; key p3:593 | p6:161; entry p6:165 |
| Stores | `Vec<Option<Curve>>`: the four natural boundary 3D curves for `CoincidenceCheck` (and, inside them, their own `CurveMemo`s) | up to 4 `(key, Singularities)` | `Arc<SurfaceContinuities{u,v}>` |
| Key | kernel key + domain bits | kernel key + `approx_tol` bits + `pole_points` + `pole_normals` | kernel key |
| Class | PURE | PURE given key. The tolerance in the key is `surface_approx_tol_3d(k, surface, face_tolerance)` (p3:626) which reads the **face tolerance on the surface** (p3:185) -> CONTEXT input, but captured in the key | PURE |
| Mechanism | `OnceLock<Box<Entry>>`; one entry ever; a different key later is computed and **not stored** (p8:422-426) | `Mutex<Vec>`; first 4 distinct keys kept, later ones never stored (p3:670) | `OnceLock<Arc<Mutex<Option<Entry>>>>`; single entry, **last writer wins** (p6:207) |
| Fill | `&self`, lazy, `boundary_curves()` p8:413 | `&self`, lazy, `Surface::get_singularities*` (p3:719+) | `&self`, lazy, `Surface::continuities` p6:337 |
| Failure | not kept | "Only a search that succeeded is kept (a cancelled one is not)" | not kept |
| Copies | every copy starts empty | every copy starts empty | exact copies share, `Clone` empties |
| Invalidated by | `kind_mut`, `transform` | same | same |
| Not invalidated by | owner / tolerance change (in key or irrelevant) | owner change (key carries tol) | - |
| Clearing changes result? | No (test `boundary_memo_hit_is_the_fresh_build`, p8:5218) | No by design | No (test p1:5291 `domain_with_continuity_is_the_same_under_every_cache_history`) |
| Send/Sync | Send + Sync iff `Curve: Send + Sync` (see section 8) | Send + Sync | Send + Sync |
| Concurrency | OnceLock: benign race | Mutex: serialised, benign | serialised, benign |

Observation: these three are the model the redesign wants (value-keyed, `&self`, explicit kernel key) - but they are bolted onto a value that also carries S1-S3, they use three different mechanisms and three different capacity/eviction rules, and the whitelist excludes exactly the kinds that are expensive.

---

## 5a. S9 - `SurfaceCache<O>` field by field (surface/surface_cache/p1.rs:1630-1692)

Mirrors `SmSurfaceCache`. `O` is the caller's opaque handle type; the same cache is re-typed with `map_handles` (p1:1873) to run in another solve's handle space.

| Field | Stores | Class | Evidence / notes |
|---|---|---|---|
| `trimmed_dispatch: bool` | "this was an `SmTrimSrfCache`" (dynamic type after `into_base`) | NOT-A-CACHE (type tag) | selects virtual `PointTest`/`GetTree` behaviour: p1:2228-2256, p3:1233 |
| `trimmed_face: Option<O>` | **topology handle** of the bound face | NOT-A-CACHE (topology binding) | set by `bind_trimmed_face` p1:1844; read by `face()` p1:2196 |
| `trimmed_face_key: Option<ObjectRef>` | owner key validated at bind | NOT-A-CACHE | `face()` errors "binding is stale" if the surface copy's owner no longer matches (p1:2215) |
| `alt_caches: Vec<AltSrfCache>` | silhouette curves, newest first, max `MAX_ALT_CACHES` | CONTEXT | S10 below |
| `tree: Option<Tree<O>>` | the Bezier decomposition tree | CONTEXT + scratch | existence depends on `doing_boolean()` (surface/surface/p1.rs:1045); node boxes are grown by the **face tolerance** (`expand_by_zone_tol: true`, p1:2674, p2:537, p2:1040, p3:1274/1288 -> surface/surface/p1.rs:2172-2173 `self.face_zone_tol_3d()`); for a trimmed cache nodes hold edge-use / vertex-use / poly-edge handles and inside/outside classes. See section A |
| `surface: Surface` | an owned **exact copy** of the cached surface, owner and face tolerance kept | NOT-A-CACHE (a second copy of the input, mutated later) | `surface_mut()` "only for the bookkeeping of the surface object it stands for (its cache slot)" p1:1993; `set_owner_zone_tol_3d` p1:1819 rewrites its tolerance after build; `adopt_owner_of` rewrites its owner at each lease |
| `surface_handle: Option<O>` | handle of the surface in `O` space | NOT-A-CACHE (binding) | mirrored into the tree (`set_owner_surface`, p1:1814) |
| `uv_domain: Extent2d` | domain covered | CONTEXT for trimmed (natural domain intersect **face domain**, trim_srf_cache.rs:888-894); for a standalone `TrimSrfCache::new` it is `natural_uv_domain_if_built()` (trim_srf_cache.rs:409), i.e. depends on whether the NURB was already made | |
| `u_continuities`, `v_continuities` | continuity at each knot | PURE | from `Surface::continuities` (S6) at `BuildTreeBase` |
| `patches: Vec<BezierPatch<O>>` | per-leaf Bezier patch + aux + boxes | CONTEXT (boxes use face tol; contain topology handles in `aux.edgeuse_list` etc., p1:1877-1906) | `bezier` is dropped after build (p3:1207) |
| `auxs: Vec<BezierAux2d<O>>` | per-node aux: `polar_box` (lazily stored by `polar_box(&mut)`, p1:2519), `uv_domain`, `has_inside_part`, `corner_nodes`, `vertex_list`, `edgeuse_list`, `poly_edge_list`, `chord_height_satisfied`, `angle_tol_satisfied` | CONTEXT + progress | topology handles; tolerance-test progress bits |
| `opts: SurfaceCacheOptions` | tessellation controls | PURE input (S14) | mutable via `options_mut` p1:2041 |
| `iso_curve_tolerance: f64` | tolerance of the cached iso boundary curves | **NOT-A-CACHE (history)** | p1:3146-3161: curves are reused if a later request's tol is **>=** the stored one, so the curves returned depend on the tightest earlier request |
| `tessellation_algorithm`, `start_fast_subdivision_angle_deg`, `switch_to_fast_subdivision`, `trim_subdivision_factor` | mode / tuning; `switch_to_fast_subdivision` is written by `test_against_tolerances` during a build (p2:438-439) | NOT-A-CACHE (mode + build progress) | |
| `iso_boundary_curves: Option<[Curve;4]>` | natural boundary iso curves | CONTEXT (see `iso_curve_tolerance`) | filled lazily through `&mut self` by `iso_boundary_curves` p1:3137; callers intersect/surface_tracer.rs, surface_intersector/p3.rs, surface_drop_curve.rs |
| `subdivision_surfaces: Vec<(O, Surface)>` | construction surfaces | build scratch | cleared after build (p3:1205; trim_srf_cache.rs:923) unless `for_derived_cache` |
| `subdivision_handles: Option<fn(u64)->O>`, `subdivision_handle_count` | names for split halves | build scratch | dropped by `map_handles` (p1:1957) |
| `bounding_planes: PatchBoundaryBoundingPlanes` | S11 | PURE, lazy `&mut` | |
| `have_t_surface_cache: bool` | "tree holds the face boundary lists" | **NOT-A-CACHE** | decides rebuild in `get_tree` (p3:1229, trim_srf_cache.rs:746), node validity (surface/surface/p1.rs:3967), `ts_bound_cached_visits` (intersect/curve_class/p9.rs:612), blend/hcr.rs:707 |
| `face_was_modified: bool` | invalidation mark | **NOT-A-CACHE** | disables point test instead of rebuilding (p1:2228, 2254; trim_srf_cache.rs:650) |
| `process_boundary_curves: bool` | solver mode | **NOT-A-CACHE** | read by every local solver (surface/surface/p5.rs:2118, p6:1929, p9:1131, p10:785, p11:1310); toggled temporarily by solvers (surface_tracer.rs:2104-2110, surface_drop_curve.rs:1720-1726, surface_intersector/p3.rs:449-456, p5.rs:580-581) |
| `point_test_enabled: bool` | solver mode | **NOT-A-CACHE** | same togglers; read p1:2228, surface/surface/p4.rs:1739, p1.rs:3967 |
| `face_containment_done: FaceContainment` | NotDone / UnderConstruction / Done | **NOT-A-CACHE** (re-entrancy guard + progress) | p3:1183-1188, trim_srf_cache.rs:759-763; read in `get_tree` to avoid recursive build |
| `pole_memo: Option<PoleMemo>` | S12 | PURE, scoped | `None` outside `subdivide_to_tolerances_with`; never copied (p1:1735, 1965) |

**Key / validity.** None. A `SurfaceCache` has no record of the kernel state, the face tolerance, the trims, or the options history it was built under, except implicitly in its own surface copy.

**Fill.** `&mut self` everywhere: `build_tree` (p3:1161), `get_tree` (p3:1226, lazy), `subdivide_*`, `iso_boundary_curves`, `polar_box`, `nth_bounding_plane`, `add_curves_to_cache`, `mark_face_containment`. Solvers that only read take `&SurfaceCache` (e.g. `global_point_solve_into`, surface/surface/p7.rs:1703) and **clone the whole cache to flip one flag** (`owned = c.clone(); owned.point_test_enabled = false`, p7:1716-1719).

**Copies.** `Clone` (p1:1694): surface by `clone_keeping_owner` (so *with knot fix-up* - the copy's surface is not the exact surface), face binding dropped if stale, `pole_memo` dropped. `clone_for_evaluation` (p1:1750): exact surfaces and iso curves. `map_handles` (p1:1873): exact copies, handle remap, `subdivision_handles` dropped. All deep copies of the tree and arenas.

**Clearing changes results?** Yes - flags above; `iso_curve_tolerance` history; alt-cache fuzzy match; tree refinement (section A).

**Upstream.** `SmSurfaceCache` (table at p1:1-57).

### S10 - `AltSrfCache` (p1:1380) / `AltCacheKey` (p1:1354)

Stores silhouette 3D/UV curves with the parameters they were made under (`approx_tol`, `angle_tol`, user vec/bool/long/double/pointer). Lookup `has_cached_curves` (p1:2337) = first entry for which `match_cache` (p1:1440) holds: **fuzzy** compare (`EFF_ZERO`-scaled), absent key parts match anything. Insert `add_curves_to_cache` (p2:404): newest first, evicts the oldest at `MAX_ALT_CACHES`. Only consumer: intersect/topology_solver/p1.rs:2174-2253. CONTEXT: user data + tolerance; hit depends on insertion history (two entries within `EFF_ZERO` -> the newer wins). `user_pointer: Option<u64>` is an opaque identity. Upstream `SmAltSrfCache`.

### S11 - `PatchBoundaryBoundingPlanes` (p1:669)

`planes_computed` + per-boundary plane data. Lazily computed through `&mut` by `nth_bounding_plane` (p2:132); "a failing computation gives 'no plane' (and is retried by the next call)" (p2:125-126). PURE (surface + domain + kernel). Consumer: intersect/surface_intersector/p2.rs:297-300; p5.rs:588 clones it out of the cache. Upstream `SmPatchBoundaryBoundingPlanes`.

### S12 - `PoleMemo` (p1:427)

`Empty | Unported | Poles(u64, Vec<Point3>)`: the `GetSingularities` result reused across the corner tests of one `subdivide_to_tolerances_with` call. `begin_pole_memo` / `end_pole_memo` (p1:2007-2018). PURE; "Performance only" (p1:126-135). Not upstream. Overlaps S5 (`SingularitiesMemo`) - two memos of the same function.

### S14 - `CacheTolerances` (surface/surface/p1.rs:915-970)

`cache_tolerances(k, surface)`: five point evaluations over the natural domain -> size estimate -> `chord_height_tol = size/40` (or `/15` when both degrees <= 2), `angle_tol = 20 deg` (or 40). PURE function of (surface, kernel); recomputed on every cache build; it makes the NURB as a side effect (`natural_uv_domain_with_nurb`, `degree_with_nurb`). Becomes `SurfaceCacheOptions` (`cache_options`, p1:964). For a `ProceduralBlend` the evaluations depend on the ambient `NetScope` (S16) - **UNVERIFIED** whether `cache_tolerances` is always called inside one.

### S15 - `HeldSolveCache<O> = Option<Result<SurfaceCache<O>>>` (surface/surface/p8.rs:127)

A caller-held slot so repeated `global_line_intersect_held` calls on one face reuse one cache, *including a remembered build failure*. Only holder: `topology/topology_traverser.rs:744` (`BTreeMap<FaceId, HeldSolveCache<RaySurface>>`). `LineCacheSource` (p8:130) enumerates the four provenance modes (Fresh / Caller / Held / Face) one solver has to support today. Comment p8:36-37: valid "on one unchanged surface".

---

## 5b. S13 - `TrimSrfCache<O>` (surface/trim_srf_cache.rs:391-399)

Mirrors `SmTrimSrfCache`.

| Field | Stores | Class | Evidence |
|---|---|---|---|
| `base: SurfaceCache<O>` | everything in 5a, with flags initialised "anticipating a face with trim curves": `point_test_enabled = true`, `process_boundary_curves = false`, `have_t_surface_cache = false`, `trimmed_dispatch = true` (:412-419) | CONTEXT + NOT-A-CACHE | |
| `edgeuse_curves: Vec<(O, EdgeuseGeometry{curve: BSplineCurve, owner_zone_tol_3d})>` | a copy of each implanted edge use's **UV trim curve** and its edge tolerance, "read again when subdivision moves an edge use into the new children" | CONTEXT (per-face topology data copied into the cache) | :383-386, :396; filled at :852-865 |
| `poly_edge_geometry: Vec<(O, PolyEdgeGeometry{start,end,tolerance,surface: Surface})>` | implanted polygon edges with **another exact Surface copy each** | CONTEXT | :360-366, :398 |

Build (`build_tree_with_face` :719 -> `build_tree_body` :768 -> `implant_face_body` :830):
1. base tree if missing;
2. if the B-rep is being edited and the adapter does not `implants_while_editing` -> `set_standalone_flags` (:797-805): the *same face* yields an untrimmed cache depending on a B-rep mode flag;
3. else: **mutates the B-rep** (`create_uv_trim_curves`, :832; temporarily sets `brep_editing_enabled(true)`, :822-826), re-reads the face tolerance into the cache's surface copy (:835-837), implants every edge use and vertex use (subdividing the tree for non-planar faces, :846), sets `uv_domain` to natural intersect face domain (:888-894), then flags (`point_test_enabled = true`, `have_t_surface_cache = true`) and `mark_face_containment_with` (classifies every unknown region by neighbour or by a **face point classification** of the region's node midpoint, surface_cache/p1.rs:2925-2960), then `face_containment_done = Done`;
4. a face with no loop uses is "treated as a standalone surface" (:897-900);
5. `finish_build` (:590) restores the guard on error and clears `have_t_surface_cache`.

**Per-face data held**: face handle, edge-use / vertex-use / poly-edge handles in tree nodes and aux lists, node inside/outside classes, UV trim curve copies, edge tolerances, vertex UV tolerances (`VertexListItem.uv_tol`), face domain, face tolerance (on the surface copy).

**Key / validity**: none; `face_was_modified` + `Brep::editing_enabled()` decide reuse (query/face/p2.rs:578). `check_face_binding` (:686) only checks the handle.

**`point_test_in` (:642-668)**: returns `(true, None)` when `!point_test_enabled || face_was_modified` - i.e. a stale cache answers "every point is inside" rather than being rebuilt.

**Copies**: derives `Clone` (so base `Clone` semantics, with knot fix-up on the surface); `map_handles` (:454) exact.

**Fill**: `&mut self` + `&mut dyn CacheFace<O>` (mutable topology service). Entry points: `make_trim_srf_cache[_with_face]`, `get_surface_cache_with_face`, `get_trim_surface_cache`, `get_surface_cache` (surface/surface/p1.rs:1062-1170), `prepare_face_surface_cache` (surface_cache/face_context.rs:335-398), `query::face::p2::face_surface_cache`.

**Factory fall-through**: every `get_*` tries the trimmed cache first and, on a non-fatal error, silently falls back to a plain `SurfaceCache` wrapped by `from_base` (p1:1121-1127, 1143-1149, 1165-1169; query/face/p2.rs:629-638). So the *type* of cache a solve sees depends on whether the trimmed build failed.

---

## 5c. Every place that reads cache state to decide an answer (surface side)

| State read | Where | Effect |
|---|---|---|
| `point_test_enabled`, `face_was_modified`, `trimmed_dispatch` | surface_cache/p1.rs:2228, 2251-2256 | `PointTest` returns true unconditionally |
| same | trim_srf_cache.rs:650 | same, face-given variant |
| `point_test_enabled`, `face_was_modified`, `have_t_surface_cache` | surface/surface/p1.rs:3967 (`is_node_valid`) | whether outside-face tree nodes are culled from a solve |
| `is_trimmed && point_test_enabled && !face_was_modified` | surface/surface/p4.rs:1738-1740, 1784-1786 | whether `GlobalPointSolve` drops solutions failing the fast-drop face test |
| same | intersect/point_class.rs:2162, 2482; topology/vertexuse.rs:1040; query/brep/ray.rs:561 | whether callers classify solution points |
| trim cache present / in use | intersect/point_class.rs:2477-2495 | answer vs `Unported` |
| `trim_cache_in_use()` | topology/vertexuse.rs:1033; topology/brep/p1.rs:1944 | skips cache lookup -> no point test |
| `face_was_modified && !editing_enabled` | query/face/p2.rs:578; surface/surface/p1.rs:991 (`validate_surface_cache`); topology/brep/p2.rs:1642 | delete + rebuild vs reuse |
| `have_t_surface_cache`, `face_containment_done`, `tree().is_none()` | surface_cache/p3.rs:1228-1231; trim_srf_cache.rs:745-748; face_context.rs:390-393 | whether `GetTree` (re)builds |
| `face_was_modified \|\| !have_t_surface_cache \|\| face_containment_done == NotDone` | intersect/curve_class/p9.rs:611-613 | `ts_bound_cached_visits` returns `None` (caller takes the non-tree path) |
| `!have_t_surface_cache` | blend/hcr.rs:707-710 | rewrites three flags on the cache |
| `tree().is_some()` with `context.is_some()` | surface/surface/p8.rs:1997-2030, 2837; intersect/surface_intersector/p5.rs:201, 561 | whether to call `get_tree` (which can fail `Unported` for face-owned trimmed caches, p3:1234-1238) |
| `process_boundary_curves` | surface/surface/p5.rs:2118, p6.rs:1929, p8.rs:3397, p9.rs:1131, p10.rs:785, p11.rs:1186/1310/1590; intersect/surface_intersector/p3.rs:449 | whether local solvers also search the natural boundary curves (changes the solution set) |
| `iso_curve_tolerance` | surface_cache/p1.rs:3148 | reuse of iso curves made at a tighter tolerance |
| alt-cache match | surface_cache/p1.rs:2347-2353 | silhouette curves reused |
| `k.context.doing_boolean()` | surface/surface/p1.rs:1045 | tree built now vs deferred -> flag values at hand-out |
| "has a NURB" (`made_kind`, `NurbSlot`) | surface/surface/p2.rs:1664-1671, 2330-2352; trim_srf_cache.rs:409 | kernel-free domain is empty for an unmade analytic |
| `trimmed_face_key` vs surface owner | surface_cache/p1.rs:2196-2216 | `face()` = handle / `Unported` / "binding is stale" |

---

## 5. Curve side

### 5.1 The `Curve` value (curve/curve/p2.rs:603-616)

| Field | Class | Evidence |
|---|---|---|
| `kind: CurveKind` | the value | |
| `owner: Option<ObjectRef>` | NOT-A-CACHE (topology handle: edge, edge use, face or curve) | read by `curve_approx_tol_3d` to refuse a build (curve/curve_cache.rs:1188) |
| `notify_identity: IdentitySlot` | NOT-A-CACHE (issued identity) | empty on every copy except `snapshot_copy` (p2:640-643) |
| `memo: CurveMemo` | cache slot (5.2) | |
| `owner_zone_tol_3d: Option<(Option<ObjectRef>, f64)>` | NOT-A-CACHE: the owning edge's tolerance pushed in by whoever holds the B-rep | `set_owner_zone_tol_3d` p2:1354; valid only while the recorded owner equals `owner` (p2:1339-1344) |

Copies:

| Copy | Line | kind | owner | zone tol | memo | identity |
|---|---|---|---|---|---|---|
| `Clone` | p2:620 | copy constructor: **knot fix-up** for B-splines (nurbs/b_spline_curve/p1.rs:529-533) | None | None | **shared** | new |
| `clone_exact_bounded` | p2:648 | exact (CrvOnSurf via `clone_exact_shared`) | kept | kept | shared | new |
| `clone_for_evaluation[_depth]` | p2:787/792 | exact | kept | kept | shared | new |
| `clone_detached` (the cache's own copy) | p2:861 | exact | None | None | new empty | new |
| `snapshot_copy` | p2:640 | exact | kept | kept | shared | duplicated |
| `kind_mut` | p2:993 | - | - | - | replaced by empty | - |
| `assign_edited` | p2:987 | from the edited value | kept | kept | the edited value's | kept |

There is no `clone_keeping_owner` for curves; `clone_for_evaluation` plays that role.

Hazard: `Clone` shares the memo on the claim "same geometry" (p2:618-619), but the B-spline copy constructor runs `fixup_knot_vector`, which "can move near-coincident knots" (nurbs/b_spline_curve/p1.rs:510-511). A clone whose knots moved would share a cache built for the pre-fix-up curve (or the reverse). **UNVERIFIED** whether any live curve is changed by the fix-up after construction. Note the asymmetry with `Surface`, where `Clone` *empties* the continuity memo for exactly this reason (surface/surface/p6.rs:156-158).

Interior-mutable state inside `CurveKind` values (CrvOnSurf mutexes, nested Surface slots) is not reset by `kind_mut` on the outer curve unless the kind's own setters do it (crv_on_surf.rs:662, 711, 754, 1785, 1827, 1863, 1907 drop `curve_cache`).

### 5.2 `CurveMemo` / `CurveMemoKey` / `CurveMemoEntry` (curve/curve/p5.rs:379-536)

1. `CurveMemo(OnceLock<Arc<Mutex<Option<CurveMemoEntry>>>>)` p5:479, held in `Curve.memo`.
2. Stores one entry: `key: CurveMemoKey` + `cache: CurveCache<()>` (handle-free full build, p5:465-468).
3. **Class: CONTEXT memo with an explicit key.** Key (p5:379-387, 407-423): `k.tolerances`; context scalars bit for bit (`this_zone_tol_3d`, `stored_model_size_estimate`, `stored_large_small_size_ratio`); `doing_boolean`; `k.fixes`; `owner_zone_tol_bits`. Given the key it is meant to be a pure function of the curve value. **UNVERIFIED** that the key is exhaustive (the `tessellate` body was not traced for other kernel reads; no `k.rng` hit in curve_cache.rs).
4. Validity: one slot, **last build wins** (`store` overwrites, p5:529-535); key equality on lookup (p5:514). Invalidated by `kind_mut`. Owner change does not invalidate (owner enters only through the tolerance in the key, p5:392-394). So the same curve under two edges of different tolerance has two valid caches but one slot: the builds evict each other.
5. Bypassed when: `!k.memoize_curve_caches` (p5:396); `memo_pure` fails - Iso, CrvOnSurf, TangentField, CrvInVolume anywhere in the nesting, or depth > 32 (p5:433-461); `retag` fails because the tree names other objects (p5:352).
6. Fill: `&self`, lazy, in `make_curve_cache_with_zone_tol` (p5:344-356).
7. No take/put: `lookup` locks, **deep-copies the whole cache** via `retag(handle)`, unlocks; `store` locks and overwrites. The lock is not held during a build, so a concurrent build of the same key builds twice. An error stores nothing. A poisoned lock is recovered (p5:511). `memoized_cache_top_box` (p5:365) reads the top box in place.
8. Clearing cannot change a result if the key is exhaustive (the stated premise, p5:474-475).
9. Send + Sync. Concurrent readers serialised and correct; clones on different threads contend on one mutex and each hit copies the tree.
10. Not upstream (upstream: `SmCacheMgr`, keyed by object).

Entry points: `make_curve_cache` p5:319, `build_cache` p8:734, `global_point_solve_cached` p1:1986; about 67 call sites of `make_curve_cache*` / `memoized_cache_top_box` / `.analytic_curve(k` across 20 files.

### 5.3 `CurveCache<O>` (curve/curve_cache.rs:302-327), mirrors `SmCurveCache`

| Field | Class | Notes |
|---|---|---|
| `curve: Curve` | copy of the input, **then mutated** | `clone_detached` in the memo build (p5:564); `tessellate` mutates it through `kind_mut` to make a missing NURB (curve_cache.rs:605-620), so the cached curve is not bit-identical to the source |
| `curve_handle: Option<O>` | NOT-A-CACHE (caller handle / topology ref; the tree's owner object) | `retag` swaps it (curve_cache.rs:372) |
| `interval` | PURE (natural interval) | |
| `tree: Option<Tree<O>>` | CONTEXT (edge tolerance, kernel) | built once by `tessellate`/`build_tree`; no `tree_mut`; no post-build refinement (section A) |
| `continuities` | PURE given `CONTINUITY_ANGLE` 0.5 deg | full trees only |
| `tried_analytic: bool`, `analytic: Option<Curve>` | lazy memo through `&mut self` + progress flag | `analytic_curve` curve_cache.rs:512-522; errors swallowed by `ignored(..)`; **UNVERIFIED** whether a cancellation latches "no analytic" |
| `spans`, `parents` | build scratch, moved into the tree's `Arc<CurveTreeData>` (geom/tree.rs:441) | comments at 317-320 |
| `opts: CurveCacheOptions` (curve_cache.rs:262) | explicit input | `chord_height_tolerance = approximate_length(5)/min(250, 30*spans)`, angle 15 deg, `make_full_tree`, `compute_aux_tree_data`, ratio 1e-4 (p5:551-575) |
| `owner_zone_tol_3d: Option<f64>` | CONTEXT input frozen at build | approximation tolerance = half of it, else half the system tolerance (curve_cache.rs:1184-1206, 601-604) |

- Fill: `&mut self` (`tessellate` :589, `build_tree` :921); eager at construction; only `analytic` is lazy.
- Copy: derives `Clone` (so `curve` / `analytic` go through `Curve::clone`: knot fix-up, owner dropped). `retag` uses `clone_detached` (exact). No `map_handles`.
- Cache state deciding answers: `global_point_solve_cached` takes the analytic fast path when `cache.analytic_curve(k)` is `Some` (curve/curve/p1.rs:1992-2010); the build fails `Unported` when the curve has an owner but no recorded tolerance (curve_cache.rs:1188-1196); an ellipse's `make_nurb_owned_with_edge_tol` uses the stored tolerance (:618).
- Plain data; Send + Sync if `O` is.

### 5.4 Caller-held curve-cache slots (Not upstream, "performance only")

| Name | Def | Holders | Key |
|---|---|---|---|
| `DropPointCaches(Vec<Option<CurveCache<CurveHandle>>>)` | curve/curve/p6.rs:216 | locals in intersect/surface_intersector/p1.rs:1964, 3258; p4.rs:625; adv_surface_intersector.rs:610, 619; adv_surface_intersector/tangent_contacts.rs:606 | slot index = caller's curve index; **no key** |
| `OrigCurveCaches(Vec<Option<CurveCache<OrigHandle>>>)` | curve/composite_curve/p1.rs:1985 | local in composite_curve/p2.rs:1442 | same |
| `&mut Option<CurveCache<O>>` parameters | `global_point_solve_cached` p1:1936; `global_curve_intersect_cached*` p8:848-951; `drop_point_cached` p6:740; `boundary_intersections` surface/surface/p8.rs:462 | callers | same |

CONTEXT memo with an unchecked contract: "the slot must belong to this curve, unchanged since the cache was built" (p6:211-214, p1:1926-1931). Kernel and zone tolerances are not re-checked on reuse. Lazy fill through `&mut`; `truncate` (p6:221) is the only invalidation.

### 5.5 `CrvOnSurf` lazy state (curve/crv_on_surf.rs:283-343), mirrors `SmCrvOnSurf`

| Slot | Stores | Class | Evidence |
|---|---|---|---|
| `break_cache: Mutex<BreakCache>` (:327) = `m_bNeedBreaks`, `m_vBreaks` | `need_breaks`, `breaks: Vec<f64>` | **NOT-A-CACHE**: can be caller-supplied and then defines `GetKnots` | `get_or_create_bs_approx_as` stores caller breaks (:1367-1371); `set_breaks_cache` :705; read from file :2268; `knots_with_status` returns cached breaks whenever `!need_breaks && len >= 2`, ignoring its `interval` argument (:1062-1070) |
| `approx_cache: Mutex<ApproxCache>` (:338) = `m_pBSApproxCurve`, `m_dMaxGap3d`, `m_bParametrizationMatches` | approximating B-spline, achieved gap, flag | **CONTEXT, order-dependent** | returns the cached curve whenever `max_gap_3d <= approx_tol` (:1384-1387): the curve returned for tolerance T depends on which tolerance was asked first |
| `curve_cache: Mutex<Option<(CurveMemoKey, CurveCache<CurveHandle>)>>` (:343) | a whole `CurveCache` + kernel key | CONTEXT, keyed by `of_kernel` only (no zone tolerance) | Not upstream as a field |

- Take/put, two levels: `drop_point_ref_as(&self)` (:1740-1773) makes `clone_exact_shared`, takes `self.curve_cache` out of the mutex, runs on the copy, then `keep_caches_of(copy)` (:435-453) **overwrites this curve's break and approx caches wholesale** and puts the curve cache back. Inside, `drop_point_base_preserving_breaks` (:1660-1705) takes the cache, discards it if the key differs, lends it, puts it back. Put-back runs on error too. A concurrent/nested observer sees `None` and builds its own; whichever finishes last overwrites (lost update). No lock is held across the solve.
- Copies: `clone_exact_shared` / `clone_for_evaluation` copy break and approx caches exactly and start `curve_cache` empty (:378-382, 396-400). `Clone` also refits the approximation's knots (:361-364) and detaches NURB cells (:354-356).
- **Geometry cache reaching topology-bound state**: the inner `surface: Box<Surface>` is copied with `clone_exact_bounded`, which shares the source surface's `cache_slot`, `made_kind` and `continuity_memo` (surface/surface/p2.rs:1076-1081). A CrvOnSurf copy made from a face surface therefore shares that face's trimmed-cache slot. `Brep::clone_for_trial` says so: "the surfaces of edge curves still share their slots" (topology.rs:1283-1285).
- Sync by type; concurrent `&CrvOnSurf` use is order-dependent.

---

## 6. nurbs/

| Item | Def | Class | Notes |
|---|---|---|---|
| `NurbSlot(Arc<OnceLock<Surface>>)` | nurbs/b_spline_surface/p1.rs:461; field `BSplineSurface.nurb` | PURE in value ("depends only on the surface's own data", :452-453); mirrors `GetOrCreateGwNurbPointer` via const cast | Filled from `&self` (`fill`, get_or_init, :503-508). Fillers: `Plane::base_with_nurb` (surface/plane/p1.rs:406), `SurfOfExtrusion::base_with_nurb(k)` (surf_of_extrusion/p1.rs:768; takes `k`, **UNVERIFIED** which kernel fields). Shared by `clone_exact_shared` (:886), `Plane::clone_shared` (plane/p1.rs:318), `clone_for_evaluation_shared`; `clone_exact` deep-copies (:878); `copy`/`Clone` deep-copy + knot fix-up (:854). Every `&mut` path detaches (`Arc::make_mut`: `as_mut` :479, `take` :494, `detach` :500). **The same slot also holds a NURB set explicitly**, so it is the value's storage and a cache at once |
| "has a NURB" as state | | NOT-A-CACHE aspect | read by `IntersectWithPlane`'s test, `operator==`, copy constructor, `ToggleSwapUVBit`, transforms (:448-450); `Curve::is_linear` branches on `gw_nurb().is_some()` (curve/curve/p5.rs:662-668) |
| `BSplineSurface.out_of_bounds_enabled: AtomicBool` | :430 | **NOT-A-CACHE**: evaluation-mode flag toggled through `&self` for a scope | `set_out_of_bounds_enabled_in_place` (:998); callers mass/integrator/properties.rs:506, 517 and query/face/p3.rs:3959. Copied by all clones. Changes evaluation results for any concurrent reader |
| `BSplineSurface.continues_past_domain: AtomicBool` | :437 | **NOT-A-CACHE**: scope flag, never copied | set/restored at mass/integrator/properties.rs:507, 518; "integrations of one surface must not overlap" (:435-436) |
| `BSplineCurve` | nurbs/b_spline_curve/p1.rs:296-308 | no lazy slot | `nurb: Option<Curve>`; `get_or_create_gw_nurb` / `make_nurb` are `&mut` (:615, :645). Analytic curves make a NURB on a temporary (`Line::base_with_nurb` returns `Cow::Owned`, curve/line.rs:1027-1034) |
| `nurbs::crv_basic::Curve{pw: Arc<Vec<CPoint>>, u: Arc<Vec<f64>>}` | nurbs/crv_basic.rs:142-146 | NOT-A-CACHE: copy-on-write storage | `Arc::make_mut` at :263, :276. Safe, Send + Sync |
| `nurbs::srf_basic` `net`, `u`, `v` as `Arc` | nurbs/srf_basic.rs:207-216 | same | `make_mut` at :410, :439, :504, :511 |
| `AnalysisMemo` | nurbs/b_spline_curve/analysis.rs:40; held in `Kernel` (`curve_analysis: RefCell`, kernel.rs:142) | PURE by full content key (curve bits, interval, property, min separation, tolerances, fixes; :66-70) | Kernel-level (other agent). FIFO cap 4096. Filled through `&Kernel` + `RefCell`. Not upstream |

No other lazy fields turned up in nurbs/ under the OnceLock/Mutex/Atomic/Arc/Cell greps.

---

## 7. Other geometry-adjacent items

| Item | Def | Class / notes |
|---|---|---|
| S17 `PendingExtension(Mutex<Option<Box<Surface>>>)` | surface/offset_surface.rs:269; field `OffsetSurface.pending_extension` (:211) | CONTEXT, order-dependent. Mirrors `m_pExtendedSurface` written by const evaluators. **Take/put**: `with_extension` / `with_current_extension` take the box out while the closure runs and put it back (:559-581); documented invariant: the closure "must not call a `&self` method of this same offset (it would see only the stored extension, and an extension it made would be overwritten when this one is put back)" (:569-573). `&mut` methods fold it in (`fold_pending_extension` :585). `follow_extension_of`, `keep_extension_made_by`, `take_extension_of` move extensions between copies "standing for one object" (:598-626). Which extension exists depends on which out-of-domain points were evaluated earlier; **UNVERIFIED** whether evaluated values differ between extensions |
| `OffsetSurface.offset_distance: F64Cell(AtomicU64)` | offset_surface.rs:199, 247 | **NOT-A-CACHE: the surface's defining parameter, settable through `&self`** (`set_offset_distance`, :505) "so that a copy of a fillet solver's offset surface ... can follow the distance the solver sets" (:195-198). 24 non-test call sites: blend/fillet_standard_solver 7, blend/fillet_solver 7, intersect/surface_intersector 4, intersect/surface_drop_lib 3, blend/fillet_intersector 1, fillet_geom 1, fillet_executive 1. Any cache over an offset surface is silently stale after a set |
| S16 `NetScope{tolerance: &Cell<f64>}` | surface/procedural_blend.rs:155-236; `Kernel.blend_net_tolerance` kernel.rs:874 | NOT-A-CACHE. Ambient scope deciding whether a `ProceduralBlend` evaluation answers from its net or its definition (:145-153). Same surface and (u, v) -> different values under different scopes. Every `SurfaceCache` tree build enters `NetScope::new(k)` (surface_cache/p3.rs:1180), i.e. caches are built on the net |
| `BlendDefinition.identity: Arc<DefinitionIdentity>` | surface/procedural_blend.rs:243, 280 | NOT-A-CACHE: allocation identity (`Arc::ptr_eq`) used as the key of the kernel's `StationMemo` (blend/fillet_section/definition.rs:393; `Kernel.blend_station_memo: RefCell`, kernel.rs:883). Geometry evaluation **writes kernel state through `&Kernel`** |
| `RecertifyMemo` | surface/procedural_blend/convert.rs:791; `Kernel.recertify_memo: RefCell` (kernel.rs:910) | PURE by content key (debug text of definition and net + tolerances + fixes, :781-787). Kernel-level |
| `SetbackFill` behind `Arc` | surface/procedural_blend/corner.rs:184 | shared immutable definition data ("Clone shares a Setback's fill", surface/surface/p2.rs:809-811). Not a cache |
| `UserSurface(Arc<Surface>)` | intersect/surface_intersector/p1.rs:304 | identity carrier (pointer equality), not a cache |
| `Starts(OnceLock<Vec<(Vec3,[f64;2])>>)` | eval/uv.rs:240, eval/plan/uv.rs:197 | PURE memo of a start grid in the record evaluator (`eval/`), `&self`, Sync; not kernel `Surface` |
| `Query.trims` / `Query.forms: Vec<OnceCell<..>>` | eval/query.rs:132-134 | PURE per face / edge of an immutable `Record`; `OnceCell` makes `Query` `!Sync`; holds per-face data by index. Record evaluator |
| `CachedVisits<O>` | intersect/curve_class/p9.rs:140 | a result struct (what a `TrimSrfCache` walk visited), consumer side |
| `geom::a_object::CacheStore` / `CacheObj` / `CacheMaker` / `CacheCheckOutIn` | geom/a_object.rs:741-1087 | port of `SmAObject::m_pCacheObj` / `SmCacheMgr`. Appears as a parameter at curve/curve/p2.rs:2434, surface/surface/p2.rs:5278 (`notify`), query/poly.rs:441. Several headers say "`SmCacheCheckOutIn` has no counterpart" (intersect/surface_tracer.rs:67, surface_drop_curve.rs:55). **Not the live cache path** - the live path is S2 |
| `geom::point_set::Cache(u8)` | geom/point_set.rs:137 | PURE flag bits (`axes`, `normal`, `centroid` set) on a plain value; **UNVERIFIED** fill mechanism |
| `surface/surface/replay.rs` `ReplaySeam{hook: Option<Arc<Mutex<Hook>>>, depth: Arc<AtomicU32>}` | replay.rs:68 | test-only seam (feature `wr57-replay`), kernel-held; not a cache |
| `Cell` fields in intersect/surface_drop_lib/boundary_moves.rs:121-123, surface_intersector/tangent_tracer.rs:210-215 | | per-solve scratch on stack-local structs, not caches |

---

## 8. Thread safety today

- **`Kernel` is `Send` and not `Sync`** (asserted `Send` only, kernel.rs:1156-1160; `Cell` / `RefCell` fields at kernel.rs:142, 874, 883, 899, 904, 910). Every evaluation and cache build takes `k: &Kernel`, so today no two threads can evaluate with one kernel, regardless of the geometry types.
- **`Surface` and `Curve`: probably `Send + Sync` today; the comment at curve/curve/p5.rs:476 ("`Curve` is `Send` and not `Sync`") looks stale.** No `!Sync` member was found reachable from either type: fields are `Mutex`, `OnceLock`, `Arc`, `AtomicBool`, `AtomicU64` only; no `Cell`, `RefCell`, `Rc`, raw pointer or non-Sync `dyn` field in surface/, curve/, nurbs/ (the `Box<dyn Fn>` at blend/fillet_section/certificate.rs:5498 is in a local test-case struct; `NetScope`'s `&Cell` is a stack guard, not a field of a surface). Other comments require Sync explicitly: offset_surface.rs:241-245 ("so that the surface stays `Sync`: a B-rep's edge arenas ... are shared ... through an `Arc`, and the B-rep must stay `Send`"), crv_on_surf.rs:325-326, b_spline_surface/p1.rs:425-427, 451-452. **UNVERIFIED**: nothing was compiled; not every `KeptDefinition` payload was expanded; there is no `assert_sync::<Surface>()` in the crate.
- **Being `Sync` does not make concurrent readers correct.** Per item:

| Item | Concurrent `&` readers |
|---|---|
| S2 cache slot | serialised by the mutex, **order-dependent**: later readers see `CheckedOut` and take another path (rebuild unkept / `Unported` / point test off / slow classification) |
| S3 `MadeKindSlot`, `NurbSlot` | correct (first fill wins, same value); but `kind()` / `natural_uv_domain_if_built()` flip under a reader |
| S4-S6 surface memos, `CurveMemo` | correct; serialised; duplicate work possible; `CurveMemo` thrashes when two keys alternate |
| `PendingExtension` | **order-dependent / lost update** (take-run-put without a lock held) |
| `CrvOnSurf` break/approx/curve caches | **order-dependent / lost update** (wholesale overwrite in `keep_caches_of`) |
| `offset_distance` | **data-dependent**: a writer changes the surface under readers |
| `out_of_bounds_enabled`, `continues_past_domain` | **wrong**: scope flags on a shared value |
| `SurfaceCache` / `TrimSrfCache` / `CurveCache` | plain data; `&` is read-only; all fills need `&mut` -> not shareable while any lazy fill (`get_tree`, `iso_boundary_curves`, `polar_box`, `nth_bounding_plane`, `analytic_curve`) is outstanding |
| ProceduralBlend evaluation | reads `Kernel.blend_net_tolerance` (Cell), writes `Kernel.blend_station_memo` (RefCell) |

---

## A. The tree (`geom::tree::Tree<O>`)

**Short answer.** No query refines a tree after it is built, so an answer never depends on *how far earlier queries refined it*. Answers do depend on the tree's leaf structure, on node classes, and on flags stored beside it - and those depend on build-time context and on cache history. Confidence: **high** that no query subdivides (every subdivide / implant entry point was grepped crate-wide); **high** that answers depend on leaf structure and flags (cited below); **medium** on how large the history effects are in practice (nothing was executed).

### A1. What a tree holds

`Tree<O>` (geom/tree.rs:424-448): `max_elements_per_node`, `min_size_ratio`, `node_reduction`, `owner: Option<TreeOwner>`, `top`, `nodes: Vec<TreeNode<O>>`, `lists: Vec<ObjectList<O>>`, `curve_data: Option<Arc<CurveTreeData>>` (:441; spans and parents, immutable after install, shared across clones and retags, header :47-52), `owner_curve`, `owner_surface`. Plain `Clone + PartialEq` data.

`TreeNode<O>` (:327-342): `parent`, `child1`, `child2`, `bbox`, `aux_type`, `data: NodeData<O>`, `geom_type`. For surface trees `data = NodeData::Surface(SurfaceAux)` (:270-287): `split_dir`, `is_marked`, `node_class` (Unknown / Inside / Outside / OnBoundary), `face: Option<O>` (**a topology handle on the node**), `surface: Option<O>`, `slot` (index into the cache's `patches` / `auxs`).

Per-node data kept in the `SurfaceCache` arenas: `BezierAux2d<O>` (surface_cache/p1.rs:1223-1247) and `BezierPatch<O>` (:1290-1302) - listed in 5a.

| Node datum | Kind | Writer |
|---|---|---|
| links, `bbox`, `split_dir`, `slot`, `uv_domain`, `pseudo_box`, conts | structural: function of surface + options + zone tolerance | build only (surface_cache/p3.rs:385-670, 879-960, 1060-1148) |
| `is_marked` | one-shot **build** mark, not per-query scratch | `set_marked` p1:3378 via `connected_region` p1:2843/2879; `TrimSrfCache::set_on_boundary` trim_srf_cache.rs:944-959; `propagate_to_parents` geom/tree.rs:1381; query/tess.rs:889-998 |
| `node_class`, `face` | face classification stored on nodes (CONTEXT) | `mark_face_containment_with` p1:2968-2977; `set_on_boundary`; `propagate_to_parents` tree.rs:1386-1403; children of an OnBoundary node reset to Unknown p3:661-664 |
| `edgeuse_list`, `poly_edge_list`, `vertex_list` | implanted face boundary (CONTEXT, topology handles) | `implant_*` trim_srf_cache.rs:987-1557 |
| `polar_box` | lazily stored pure memo | `polar_box` / `build_polar_box` p1:2519-2585 |
| `chord_height_satisfied`, `angle_tol_satisfied`, `bezier`, `subdivision_surfaces`, `pole_memo` | construction scratch | p2:498-504; p3:871-875, 1204-1215 |

**Marks are never reset.** The only `is_marked: false` writes are node creations (p1:3109, p2:990/1150, p3:518/1093). `mark_face_containment_with` loops "find an unmarked Unknown leaf, mark its connected region, classify it" (p1:2921-2984); a second run finds nothing. So `connected_region` (p1:2809, `&mut self`) is a build step that leaves permanent marks, not a reusable query.

**No solver writes marks or classes into nodes.** A crate-wide grep for `is_marked`, `node_class =`, `set_marked`, `propagate_to_parents` hits only surface_cache/p1 and p3, trim_srf_cache.rs, query/tess.rs and geom/tree.rs. Solvers only *read* `node_class`: surface/surface/p1.rs:3956-3978 (`is_node_valid`), query/face/p2.rs:1009-1013, query/brep/ray.rs:474, geom/grid.rs:926-936.

### A2. When built, how far

**Plain cache.** `make_surface_cache` (surface/surface/p1.rs:1010-1031): `SurfaceCache::new` then `build_tree(k, false)` eagerly. `build_tree_base_with` (p3:984-1057): one exact Bezier patch per knot span for B-spline-backed kinds (:1033-1038), else up to 5 x 5 bicubic approximations (:1040). Each first-generation node is subdivided until `test_against_tolerances` passes (p3:879-960, :1139-1141), bounded by `MAX_SUBDIVISION_DEPTH` / `MAX_SUBDIVISION_NODES` (:886, :915). `build_tree_top` (p1:2998) pairs them up to one top node.

Build inputs: the surface value; `SurfaceCacheOptions` (p2:484-487) from `cache_tolerances`; the **owner's tolerance** `zone_tol_3d(k, self.surface())` (p2:494) = face tolerance cached on the surface, else the kernel system tolerance, feeding the `expand_by_zone_tol` boxes (p2:537); `surface.continuities(k)` (p3:1054); NURB-making through `&` (p3:1009-1027) which fills `MadeKindSlot` / `NurbSlot`; `NetScope::new(k)` (p3:868, 990, 1180); build-mode fields mutated during the recursion (`switch_to_fast_subdivision` p3:903-910, `tessellation_algorithm`).

**Trimmed cache.** `TrimSrfCache::build_tree_in` (trim_srf_cache.rs:757-816): base tree if none (:770-772), then `implant_face_body` (:830-917) - described in 5b. Implanting **subdivides leaves of the shared tree further** for non-planar faces (`do_subdivision` :846; `subdivide_node` :1270, :1300-1320). So a trimmed tree is *not* "the plain tree plus annotations".

**Deferral.** `trim_srf_cache_tree` skips the build when `k.context.doing_boolean()` (surface/surface/p1.rs:1037-1052); the first `get_tree` then builds it.

**Lazy rebuild rule.** `SurfaceCache::get_tree(&mut self)` (p3:1226-1256) builds when there is no tree **or** when `!have_t_surface_cache && face_owned && face_containment_done != UnderConstruction`. So a face-owned surface's *untrimmed* cache rebuilds its whole tree on every `get_tree` unless the guard is set; blend/hcr.rs:700-711 sets `UnderConstruction` on purpose to stop that. The same test is duplicated at trim_srf_cache.rs:744-751, query/face/p2.rs:668-674 and surface_cache/face_context.rs:388-396.

**Tessellation.** `TessSrfCache` (query/tess.rs:337) re-subdivides from the top with its own tester (tess.rs:567) and has its own mark pass (tess.rs:860-1012). Other agent's area.

**CurveCache.** `tessellate(&mut self, k)` runs once right after `new` (curve_cache.rs:589) and calls `build_tree` (:921). Inputs: the curve, `CurveCacheOptions` (:262-281), `curve_approx_tol_3d(k, &curve, self.owner_zone_tol_3d)` (:599). No post-build subdivision. One lazy `&mut` fill: `analytic_curve` (:512-522).

### A3. Is the tree refined during queries? No.

Every caller of `subdivide_node`, `subdivide_node_split`, `subdivide_to_tolerances*`, `subdivide_by_block` and `implant_*` is in surface_cache/p2+p3 (build), trim_srf_cache.rs (build + implant), query/tess.rs:567 (tessellation build), or a test (geom/grid.rs:1537). No hits in intersect/, surface/surface/, curve/, boolean/, query/face or query/brep.

Solvers borrow the tree immutably: `global_point_solve_0_in` takes `cache: Option<&SurfaceCache<O>>` (surface/surface/p7.rs:1699-1767); `SsGlobalSolver.caches` is `[Option<&SurfaceCache>; 2]` (p7:3476).

The `&mut SurfaceCache` in solver signatures (surface/surface/p8.rs:1609-3035, intersect/surface_tracer.rs:754-2118, intersect/surface_drop_curve.rs:1043-2278, intersect/surface_drop_lib/p1.rs:1047-1200) is used for exactly five things:
1. `get_tree` lazy build / rebuild: p8:1517, p7:2512/2541, surface_tracer.rs:1908/2170, surface_intersector/p5.rs:202/562, adv_surface_intersector.rs:408, ray_tracer.rs:1264, offset/offset_geometry_creation.rs:748, offset/offset_executive/edgeuse.rs:654.
2. `iso_boundary_curves` lazy fill: surface_intersector/p3.rs:495, surface_drop_curve.rs:1088, surface_tracer.rs:1690.
3. Stored `polar_box` fill: only surface/surface/p1.rs:2447 and geom/grid.rs:271. Solver paths use the non-storing twin `polar_box_unstored(&self)` (surface_cache/p1.rs:2597); adv_surface_intersector.rs:921 says why: "built without storing it here (the caches are shared), which gives the same box".
4. Alt caches and bounding planes: `add_curves_to_cache` (topology_solver/p1.rs:2243), `patch_boundary_bounding_planes()` (surface_intersector/p5.rs:588).
5. Flag flips around a solve: `point_test_enabled` / `process_boundary_curves` saved-set-restored (surface_tracer.rs:2104-2110, surface_drop_curve.rs:1720-1726, surface_intersector/p3.rs:449-456, p5.rs:580-581; blend/hcr.rs:707-711), or on a whole-cache clone (p7:1716-1730).

### A4. Three traced paths

**(a) Point drop / inversion - `Surface::global_point_solve_0_in` (surface/surface/p7.rs:1699).** Branch and bound over the tree. `SpGlobalSolver::may_contain_answers` (p7:3380-3427) prunes with `is_node_valid` and, for Normalize, the pseudo box and `polar_box_unstored`. Leaves get local solves. `is_node_valid` (surface/surface/p1.rs:3956-3978) culls an `Outside` node only when `point_test_enabled && !face_was_modified && have_t_surface_cache`. Kept solutions pass `point_test_with` (surface_cache/p1.rs:2244-2269), which returns valid unconditionally when `!point_test_enabled || face_was_modified`, else classifies through the operation's `FaceQueries`. So the solution set depends on the leaf decomposition (one local solve per leaf, dedup via `identical_solutions`, p7:3447), the three flags, and the node classes - not on any refinement by earlier queries.

**(b) Line / curve-surface intersection - `LsiGlobalSolver::local_solve_branch` (surface/surface/p10.rs:399-560).** Per leaf: `patch.aux.uv_domain`, `patch.pseudo_box`, `surface_node.bbox`, patch continuities, `is_node_valid` (:432), box-line interval clipping (:487-520). The comment at p10:531-535 records an answer that changed with the per-leaf seed: a grazing leaf "seeded its solve from the middle of the interval, which can settle on the root behind the start ... the root ahead was never looked for, so the ray missed a wall". So leaf boundaries decide which roots are found; a different decomposition (trim-implant subdivision or none, a different chord tolerance, a different face tolerance) can give a different root set. The cache source is explicitly four-way (`LineCacheSource`, p8:130-139), and `HeldSolveCache` keeps the first build's outcome, including an error.

**(c) Point classification in a trimmed cache - `point_classify_with_topology` (query/face/p2.rs:1061-1145).** The fast path runs only if a cache is available and current: `cache_is_current` = `have_t_surface_cache && face_containment_done == Done && !face_was_modified` (:984-988). The cache is the caller's `trim_cache`, else `s.take_trim_cache()`, which yields nothing when the slot is `Empty` or `CheckedOut` (:1125-1131) - a nested observer during a checkout silently takes the slow path. Fast path `classify_trim_cache` (:990-1019): `Outside` leaf -> `Unknown`; `Inside` leaf -> `Face` with `gap_3d = 0.0` and `pre_snap_param = BIG_DOUBLE`; `OnBoundary` / `Unknown` leaf -> falls through to the exact vertex / edge / loop classification.
- Consequence 1 (confirmed in code): output fields differ between fast and slow paths (`gap_3d`, `pre_snap_param`).
- Consequence 2 (**UNVERIFIED**, inferred): a point in an `Inside` leaf but within tolerance of an edge in a neighbouring leaf would be `Face` on the fast path where the exact path could say `Edge`.
- Leaf classes were decided at build by `face.point_classify` at leaf midpoints with the face tolerance of that moment (surface_cache/p1.rs:2945-2952) and are never revisited; `face_was_modified` makes the cache ignored, not repaired.

### A5. Other history-dependent state beside the tree

- `iso_boundary_curves` (surface_cache/p1.rs:3137-3207): returns cached curves when the domain matches and `approx_tol_3d >= iso_curve_tolerance`; `iso_curve_tolerance` is overwritten even on calls that do not cache (:3161); it starts at 0.001 (:1790), so the first call with a tolerance below 0.001 is not cached while the identical second call is. Both content and the `cached` flag are order-dependent.
- Flags read as answers: see 5c. The `SurfaceSolveContext` doc states it: "The bits then depend on whether the face's cache was built before, as upstream's and as the live readers' ... do" (surface/surface/p1.rs:1198-1200).
- Tolerance frozen at build: tree boxes use the zone tolerance read at build (p2:494). `implant_face_body` refreshes the cache surface's tolerance after trims are created (trim_srf_cache.rs:835-837), but the base tree was already subdivided with the earlier value. Effect on results **UNVERIFIED**.

### A6. Existing history-independence assertions

- Test `domain_with_continuity_is_the_same_under_every_cache_history` (surface/surface/p1.rs:5291); its doc notes upstream answers differently after the "cache without tree" history.
- Tests `numerical_trim_queries_survive_caller_and_held_cache_reuse` (surface/surface/p8.rs:5006), `boundary_memo_hit_is_the_fresh_build` (p8:5218), `pole_memo_matches_lazy_singularity_test` (surface_cache/p1.rs:3482).
- Header: "Other surface caches are built afresh instead of being kept on the surface ... Whether upstream's cache had its tree built is not modelled" (surface/surface/p2.rs:206-210).
- `CacheSlot` doc: "Single-threaded use only orders the accesses, so the result is deterministic" (p2:506-507) - i.e. determinism is explicitly conditional on one thread.

### A7. Fresh versus long-lived

Fresh per call (the default): `get_surface_cache`, `make_surface_cache`, `make_trim_srf_cache` (surface/surface/p1.rs:1010-1170); `solve_surface_cache` (p7:2502: "The cache is built for the solve (no cache store)"); `Surface::solve_cache_of` (p8:1527-1548); `surface_cache` (p2:5306).

Long-lived (one built tree serves many queries): the `Surface.cache_slot` trimmed cache (S2), used by the face lookups in query/face/p2.rs; `HeldSolveCache`; caller-passed `Option<&mut SurfaceCache>` (p8:1609, 2602; surface_drop_lib/p1.rs:1047); and operation-level holders named for linkage only (other agents): `FaceIntersectCaches.surfaces` (topology/face/p3.rs:2589), `KeptFaceCaches` (query/brep/ray/live.rs:149, a `Cell<Option<..>>` with take / hand-back), `BrepCache` (topology/brep_cache.rs:101), intersector-owned copies via `SurfaceCache::clone_for_evaluation`, kernel `Caches` (kernel.rs:137, holds only `curve_analysis`).

---

## B. Why geometry values carry owner data

### B1. Surface

Non-test readers in surface/, curve/, intersect/, geom/ (counts by grep; test bodies inside the same files excluded by inspection where listed):

| Reader | Sites | What it is used for |
|---|---|---|
| `Surface::face_zone_tol_3d()` | 7 non-test: surface/surface/p1.rs:730, p1.rs:2173, p3.rs:185, p5.rs:1165, p5.rs:1470, assert_valid.rs:638, + 1 in sweep/ | `GetFace() ? face tolerance : default` |
| `p1::zone_tol_3d(k, surface)` (wraps the above, p1:729) | about 31 call sites: intersect/curve_class/p1.rs 7, trim_srf_cache.rs 4, surface/surface/p1.rs 3, intersect/topology_solver/p1.rs 3, surface_cache/p1.rs 2, curve_class/p8.rs 2, and one each in surface_point_solve/p3, surface_cache/p2, face_context, surface/p4, surface_drop_lib/p2, point_class, projected_curve, ellipse/p1, curve/p2, crv_on_surf, crv_in_volume | `SmTol::GetZoneTol3d(pSurface)` |
| `Surface::owner_zone_tol_3d()` direct | 8 non-test: plane/p1.rs:1601, 1635; plane/p2.rs:202, 1677; surface/p2.rs:3236; surface/p4.rs:2192; cone/p3.rs:834; intersect/point_class.rs:1938 | kind-specific intersections and classification |
| `Surface::face()` | 24 in surface/ + intersect/ (surface_cache/p1.rs 5, trim_srf_cache.rs 5, surface/p8 3, surface_cache/p3 2, point_class 2, ...) | "is this surface face-owned" and the face key for cache binding |
| `Surface::owner()` | about 45 in surface/ + intersect/ (surface/p11 9, offset_surface 6, p9 4, p7 3, p4 3, p2 3, assert_valid 3, curve_class/p2 3, ...) | owner type tests, offset base chain, validity |
| `reached_face()` | 4 | internal |

Characterisation - four distinct roles hidden in one triple:

1. **A tolerance input** (the large majority). The face's zone tolerance is read *during evaluation-like work and cache building*:
   - node bounding boxes of every surface cache are grown by it (`expand_by_zone_tol: true` at surface_cache/p1.rs:2674, p2.rs:537, p2.rs:1040, p3.rs:1274, 1288 -> `Surface::calculate_bounding_box` -> surface/surface/p1.rs:2172-2173; nurbs/b_spline_surface/p1.rs:1652-1658 `expand_absolute(owner_zone_tolerance(req))`);
   - the subdivision test reads it (surface_cache/p2.rs:494);
   - the singularity search tolerance is half of it (surface/surface/p3.rs:172-188) and so is part of the `SingularitiesMemo` key;
   - surface-surface and curve-surface intersection distance tolerances (p5:1165, p5:1470, p4:2192; plane/p2.rs:1677; p2:3236).
   **So the same surface under two faces with different tolerances builds different trees and finds different singular sides.** For a surface without a face the kernel's system tolerance is used instead (p1:732).
2. **A mode switch.** "Has a face" changes behaviour, independent of the tolerance value: `get_tree` rebuild rule (surface_cache/p3.rs:1227-1231); `TrimSrfCache::build_tree_body` refusing a face-owned surface without a face adapter (trim_srf_cache.rs:785-790); `SurfaceCache::face()` returning `Unported` for a face-owned surface with no binding (surface_cache/p1.rs:2211-2214); `PlaneOwner::of` (plane/p2.rs:200-204); the `Unported` for "a bare face handle" without recorded tolerance (p2:1841-1844).
3. **A topology back-pointer** used to *find* the face from the geometry: `owner_face` (intersect/point_class.rs:2450-2459), `bind_trimmed_face` validating the key (surface_cache/p1.rs:1844-1853), `rekey_trimmed_face` (:1836).
4. **A scope hack.** `begin_owner_cleared` / `Brep::is_surface_owner_cleared` emulate upstream temporarily nulling `m_pOwner` during `CreateUVTrimCurve(s)` so that code underneath sees "no face" (topology.rs:1166-1183; p2:1464-1481; query/face/p2.rs:437-439, 457-462, 564-566).

Maintenance burden: the B-rep must push the tolerance onto the surface at every rebind or tolerance change (`set_owner_zone_tol_3d`: 41 call sites - surface 17, topology 11, curve 5, query 4, intersect 3, heal 1), down the offset-base chain (`update_owned_bases`, p2:1782-1796), and onto every *cache's copy* of the surface (`SurfaceCache::set_owner_zone_tol_3d`, surface_cache/p1.rs:1819; `adopt_owner_of`, p2:1432). `RescueSurfaceOwner` (topology.rs:1212) exists to restore owner state that `Clone` dropped.

### B2. Curve

- `Curve::owner_zone_tol_3d()` non-test readers: 7, all tolerance lookups - curve/curve/p5.rs:325 (`make_curve_cache`: cache build + memo key), p8.rs:717 and 743 (top box / cache for curve-curve intersection), curve_cache.rs:1185 (`curve_approx_tol_3d`: tessellation tolerance), p5.rs:1746 and 1816 (`length`), p3.rs:622 (`find_parameter_at_arc_length`), geom/tol.rs:893.
- `Curve::owner()` non-test readers in curve/: 4 - curve_cache.rs:605 (passed to `make_nurb` on analytic kinds during `tessellate`), curve_cache.rs:1188 (refuses the build with `Unported` when an owner exists and no tolerance is recorded), curve/curve/p6.rs:962 (analytic `global_point_solve_step`), p4.rs:1256 (`assert_valid`). `Curve::edge(owner_of)` (p2:1396) walks the owner chain to an edge (geom/tol.rs:897).

For a curve the owner enters cache building as **one scalar** (the edge tolerance; half of it is the tessellation tolerance) plus a guard (owner without tolerance -> build fails). The `CurveMemo` already treats it as an explicit key component and builds on an owner-less copy (p5:392-394, 564, 578-580). That is the pattern the surface side lacks.

---

## C. Facts most likely to break "a surface/curve is an immutable shareable value; each cache is computed once from explicit inputs"

1. **A `&Surface` is not immutable today.** `OffsetSurface::set_offset_distance(&self)` changes the defining parameter (surface/offset_surface.rs:505; 24 call sites in fillet solvers and intersectors); `BSplineSurface.out_of_bounds_enabled` and `continues_past_domain` are per-scope evaluation-mode flags written through `&self` (nurbs/b_spline_surface/p1.rs:430, 437; mass/integrator/properties.rs:506-518; query/face/p3.rs:3959). These are modes of an evaluation, stored on the value.
2. **Evaluation is not a function of (surface, uv).** A `ProceduralBlend` answers from its net or its definition depending on the kernel's ambient `NetScope` (`Kernel.blend_net_tolerance: Cell<f64>`, surface/procedural_blend.rs:155-236) and writes the kernel's `StationMemo` (`RefCell`) keyed by an `Arc` allocation identity. `Kernel` is `!Sync`, and every evaluation takes `&Kernel` - workers cannot share one kernel whatever is done to the geometry types.
3. **Geometry values carry topology.** `Surface.owner` / `owner_chain_face` / `owner_zone_tol_3d` and `Curve.owner` / `owner_zone_tol_3d` make "the same surface under two faces" two different values with different caches: the face tolerance grows every tree node box, sets the singularity tolerance, and "has a face" switches code paths and error outcomes (section B). The tolerance must become an explicit build input, as `CurveMemoKey.owner_zone_tol_bits` already does for curves.
4. **The trimmed-surface cache slot is the worst offender**: `Arc<Mutex<SlotState>>` shared by every evaluation/exact copy, holding a face-bound cache built by *mutating the B-rep* (`create_uv_trim_curves`, raising face tolerance; trim_srf_cache.rs:832-837), with a take / `CheckedOut` / put protocol whose observers change behaviour (rebuild unkept, `Unported`, point test off, slow classification path), no RAII on the lease, a deferred-modification flag, and a dynamic-scope stack (`cleared_for`). It has no key: validity is `face_was_modified` plus `Brep::editing_enabled()`.
5. **Cache presence and cache history change answers, not only speed** (5c): `face_point_test_on` returns `Unported` without a kept cache (intersect/point_class.rs:2473-2495); point classification returns different fields via the tree fast path (query/face/p2.rs:1009-1016); `GlobalPointSolve` culls or keeps solutions by `point_test_enabled && !face_was_modified` (surface/surface/p4.rs:1738-1740); a stale cache answers "inside" instead of being rebuilt (trim_srf_cache.rs:650-653); iso boundary curves and CrvOnSurf approximations are whatever the tightest earlier request built (surface_cache/p1.rs:3146-3161; crv_on_surf.rs:1384-1387).
6. **`SurfaceCache` mixes four things in one struct**: a pure decomposition (tree geometry, continuities, bounding planes), a face overlay (implanted boundary, node classes, face / edge-use / vertex-use handles, face domain), solver mode flags (`point_test_enabled`, `process_boundary_curves`) that solvers flip in place or clone the whole cache to flip, and lazy sub-caches behind `&mut` (`get_tree`, `iso_boundary_curves`, `polar_box`, `nth_bounding_plane`, alt caches). And the overlay is not separable as-is: implanting trims subdivides the base tree further (trim_srf_cache.rs:1270-1320).
7. **The tree itself is effectively write-once** - no solver subdivides or marks it (section A) - which is the good news: the decomposition can become an immutable value. But its build inputs today include `doing_boolean()` (build now vs deferred, surface/surface/p1.rs:1045), `Brep::editing_enabled()` (trimmed vs standalone flags, trim_srf_cache.rs:797-805), the ambient `NetScope`, and the face tolerance *at that instant*.
8. **`get_tree`'s rebuild rule depends on flags, not on a key**: `!have_t_surface_cache && face_owned && face_containment_done != UnderConstruction` rebuilds the whole tree on every call for a face-owned surface's untrimmed cache; `blend/hcr.rs:707-710` sets `UnderConstruction` permanently just to stop it. The rule is duplicated in four places.
9. **There are five kinds of copy and they disagree** (1.1, 5.1): `Clone` changes geometry (knot fix-up, torus inside-out reset) and drops owner + caches; evaluation copies keep the owner, share the cache slot and made-NURB cell, and drop the definition; exact copies keep everything and share; snapshot and trial copies each do something else. The Arc-shared slots exist to emulate upstream pointer aliasing ("stands for the same surface object"), and the slot pointer is also used as **object identity** (`shares_cache_slot_with`: blend/fillet_solver/trace_reach.rs:154, query/face/p2.rs:461). A redesign needs an explicit identity for "same surface" before the slots can go.
10. **Every cache owns its own copy of the surface** (`SurfaceCache.surface`, each `PolyEdgeGeometry.surface`, `CurveCache.curve`, `CrvOnSurf.surface`), mutated after the copy (owner re-adopted per lease, tolerance re-synced, NURB made in `tessellate`). Caches are therefore not "computed from the value" but carry a diverging second value, and nested copies re-share the original's slots (a CrvOnSurf copy shares the face surface's trim-cache slot, surface/surface/p2.rs:1076-1081).
11. **Lazily made NURBs are observable**: `kind()` returns a different `SurfaceKind` after the first `kind_with_nurb` (surface/surface/p2.rs:1604-1606); kernel-free readers (`natural_uv_domain_if_built`) return an empty domain before it; `type_code()` / copy code read the unmade `self.kind` while `kind()` reads the made one; `NurbSlot` is both the storage of an explicit NURB and a cache. Making the NURB eagerly at construction removes a whole class of state.
12. **Three interior-mutable take/put slots lose updates under nesting or concurrency**: `PendingExtension` (offset_surface.rs:559-581, with a written "must not re-enter" invariant), CrvOnSurf `keep_caches_of` (crv_on_surf.rs:435-453), and the trim cache slot. None holds a lock across the work.
13. **Invalidation is manual and incomplete.** `kind_mut()` resets the three surface memos and the curve memo but not the trim cache slot; `transform` deletes a stored trim cache only when the kind sends `PreEdit`; a checked-out cache survives any edit; caller-held slots (`DropPointCaches`, `OrigCurveCaches`, `HeldSolveCache`, `&mut Option<CurveCache>`) have no key at all, only a comment-level contract; `CurveCache` / `SurfaceCache` record no kernel state they were built under.
14. **The existing "pure" memos show both the target and its current limits.** `CurveMemo`, `BoundaryMemo`, `SingularitiesMemo`, `ContinuityMemo` are value-keyed with an explicit kernel key (`CurveMemoKey::of_kernel`: tolerances, three context scalars, `doing_boolean`, all `UpstreamFixes`) - the code's own list of ambient inputs. But: they use three different mechanisms and capacity rules (single slot last-wins, single slot first-wins, first four); they are restricted to a whitelist (BSpline / Plane / Cone / Sphere / Torus surfaces; curves without a surface or volume inside) precisely because the other kinds contain the impure state above; `CurveMemo` deep-copies the whole cache on every hit and thrashes when one curve is built under two tolerances; and `Curve::Clone` shares the memo across a knot fix-up.
15. **Overlap to collapse**: `PoleMemo` vs `SingularitiesMemo` (same function); `SurfaceCache.u/v_continuities` vs `ContinuityMemo`; `SurfaceCache.iso_boundary_curves` vs `BoundaryMemo` (both natural boundary curves, different tolerance handling); `MadeKindSlot` vs `NurbSlot` (two made-NURB mechanisms by kind); `CurveMemo` vs `CrvOnSurf.curve_cache` vs caller-held `Option<CurveCache>` (three homes for one curve cache); trimmed cache in `Surface.cache_slot` vs operation-level face cache maps vs `HeldSolveCache` (three homes for one face's surface cache); and `geom::a_object::CacheStore` (the ported `SmCacheMgr`), which is not the live path.

---

## Appendix: verification notes

- Sections 5, 6, most of 7 and section A were gathered by two parallel sub-investigations over the same checkout and then spot-checked: `OffsetSurface.offset_distance` / `PendingExtension` (offset_surface.rs:192-272, 500-583), `out_of_bounds_enabled` (b_spline_surface/p1.rs:424-437), CrvOnSurf approx cache (crv_on_surf.rs:1362-1388), `classify_trim_cache` and the slot take in classification (query/face/p2.rs:984-1019, 1120-1137), the ray-seed comment (surface/surface/p10.rs:531-535), `iso_boundary_curves` (surface_cache/p1.rs:3137-3207) were re-read directly. Line numbers in those sections that were not re-read are as reported by the sub-investigation.
- Not done: no compile-time check of `Send` / `Sync`; `tessellate` and the `make_nurb` bodies were not traced for kernel reads beyond `CurveMemoKey`'s own list; `KeptDefinition` payloads were not fully expanded; kernel-level memos (`StationMemo`, `ThumbweightMemo`, `LoopAreaMemo`, `AnalysisMemo`, `RecertifyMemo`) and operation-level cache holders are named only.
