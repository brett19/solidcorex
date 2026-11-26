# 09: Whole surface-cache copies: where they come from and how to remove them

Repo a local worktree @ 782ef9afc. Paths are relative to `solidcore/crates/solidcore/src`.
Counts come from raw data kept outside this repository (probe `tests/zz_cache_count_probe.rs` in a local worktree).

The counts measured in that log:

| Case | SurfaceCache::map_handles copies | Clones | Other counts |
|---|---|---|---|
| round/fillet | 260 | 0 | TRIM_CACHE_MAP_HANDLES=0, UV_TRIM_CREATIONS=14, FACE_TRIM_PASSES=20, BASE_TREE_BUILDS=2 |
| ball/drill (one curved pair) | 6 | 2 | |
| torus/box (face_pairs=2) | 8 | 2 | |
| check_brep | 2 | | |
| planar/cylinder slot | 0 | | |

The marking used throughout:

- **[V]** means verified by reading the code at the cited lines.
- **[I]** means inferred.
- **UNVERIFIED** means it needs a measurement (see Q7).

---

## Root cause (one paragraph)

**[V]** `SurfaceSolveContext<O>` (`surface/surface/p1.rs:1180-1215`) has two methods, `build_surface_cache` and `surface_cache_without_get_tree`. Both return an **owned** `SurfaceCache<O>`.

Some contexts keep one cache per face for the length of an operation:

- `FaceCacheContext` (Boolean and fillet topology intersector);
- `BrepFaceServices` with `FaceCaches` (`query/brep/ray/live.rs`);
- `RayPreparation` (`query/brep/ray.rs`);
- `TrimmedFaceContext` (`intersect/surface_intersector/p3.rs`).

The owned return type means each of these must copy the whole kept cache on **every fetch**. The copy is `map_handles(Ok)`, an identity handle map.

**[V]** The topology intersector's `with_face_caches` loop (`boolean/topology_intersector/p1.rs` ~4700) restarts the solve from the beginning after each `PreparationRequired`, which is raised for a cache or UV-trim preparation. Every rerun repeats every earlier fetch, and so every earlier copy.

**[I]** The fillet's UV_TRIM_CREATIONS=14 means at least 14 preparations, so it probably reruns many times. That multiplication is the most likely source of most of the 260.

---

## Q1: Where the copies come from

### Copy-producing sites (non-test) [V]

| # | Site | What is copied | Notes |
|---|---|---|---|
| P1 | `boolean/topology_intersector/p1.rs:4265` | `FaceCacheContext::build_surface_cache`: `caches.built[..].map_handles(Ok)` | identity, once per fetch |
| P2 | `boolean/topology_intersector/p1.rs:4303` | `surface_cache_without_get_tree`: copy of `caches.states` | only to answer state questions |
| P3 | `topology/face/p4.rs:582` | `FaceLineCache::build`: lease, `face_cache_tree`, `map_handles(\|_\| Ok(surface))`, `set_surface_handle` | handle **value** changes; once per line solve |
| P4 | `query/brep/ray.rs:85` | `RayPreparation::surface_cache`: `map_handles(Ok)` | |
| P5, P6 | `query/brep/ray/live.rs:686`, `:693` | `BrepFaceServices::cache_mapped` (kept `FaceCaches` entry) | once per fetch |
| P7, P8 | `query/brep/ray/live.rs:718`, `:726` | `cache_state_mapped` | state copy |
| P9 | `intersect/surface_intersector/p3.rs:2017` | `TrimmedFaceContext::build_surface_cache`: copies `self.cache` again | |
| P10, P11 | `intersect/surface_intersector/p3.rs:2088`, `:2097` | trimmed with a face: one copy for the face context, one for the solve cache | 2 copies per call |
| P12 | `intersect/surface_intersector/p3.rs:2109` | untrimmed with a handle | |
| P13 | `intersect/surface_intersector/p3.rs:2170` | `surface_drop_and_trim_curve` | |
| P14 | `surface/surface/p10.rs:230` | `drop_and_trim_curve`: `c.map_handles(Ok)` | only because `drop_and_trim_curve_in` takes `Option<&mut SurfaceCache>` |
| P15 | `surface/surface/p2.rs:1505` | `keep_standalone_trim_cache` | TrimSrfCache |
| P16 | `surface/surface/p2.rs:1545` | `copy_cache_slot_of`: copy of a B-rep into new arenas | TrimSrfCache |
| P17 | `query/face/p2.rs:548` | `surface_cache_copy_of_face`, used only by offset (`offset/offset_executive/edgeuse.rs:684/686`) | TrimSrfCache |
| P18 | `surface/trim_srf_cache.rs:472` | TrimSrfCache `map_handles` | not read in detail |
| P19 | `intersect/surface_intersector/p5.rs:722-726` | `caches.iter().map(SurfaceCache::clone_for_evaluation)` for the boundary start points | the "2 clones" |
| P20 | `surface/surface/p7.rs:1714-1731` | `global_point_solve_0_in`: `owned = c.clone(); owned.point_test_enabled = false` | a clone only to set a mode |

**[V]** `clone_for_evaluation` (`surface/surface_cache/p1.rs:1750`) is `self.clone()`. TRIM_CACHE_MAP_HANDLES=0 in every measured case, so P15-P18 do not fire in any of them.

### Consumers that ask for a copy only to read or set something [V]

- **`surface/surface/p4.rs:1735`** (`global_point_solve_in`, fast-drop branch, context but no held cache). It calls `context.surface_cache_without_get_tree(...)`. It then reads only `is_trimmed()`, `point_test_enabled`, `face_was_modified` and `face()` (`p4.rs:1738-1747`). This reaches P2 or P7/P8.
- **`surface/surface/p7.rs:2333-2337`**: `context.build_surface_cache(...).surface_continuity(uv_domain)`. A whole copy to read one value.
- **`intersect/topology_solver/p1.rs:2159`** (silhouettes): `ctx.build_surface_cache`. It then reads and writes alt caches **on the copy**, so the writes are thrown away.
- **`intersect/surface_intersector/p5.rs:163-209`**: `intersection_surface_caches_with_context` calls `ctx.build_surface_cache` once per surface. At `p5.rs:575-582` it then sets `point_test_enabled = false` and `process_boundary_curves = true` on those owned copies and never restores them.
- **Other `build_surface_cache` callers:**
  - `surface/surface/p7.rs:2525-2552` (`solve_surface_cache_with`);
  - `surface/surface/p8.rs` ~1550-1560 (`solve_cache_from_context`), `:2283` and `:3046`;
  - `exact_pieces.rs:339`.

### Is a copy's content the same as the kept cache? [V]

No. `map_handles` (`surface_cache/p1.rs:1873-1967`) differs from both the original and from `Clone` (`p1.rs:1694-1737`):

- the surface goes through `clone_for_evaluation`, an exact copy, whereas `Clone` uses `clone_keeping_owner` and fixes up the knots;
- `subdivision_handles` becomes `None`;
- `pole_memo` becomes `None`;
- `trimmed_face` and `trimmed_face_key` are dropped when the binding is not current. As a result `face()` (`p1.rs:2196`) can return `Err` on a copy where the original would bind.

Anything that replaces a copy with a borrow must keep these differences, or show they cannot change a result.

### 1a. The fillet's 260

**Verified chain pieces [V]:**

- **Entry.** `fillet_edges` (`blend/fillet_executive/api.rs:202`) calls `keeping_a_solid_closed` (`api.rs:127`), which runs `prepare_constant_radius_edges` and `do_fillet`, then `stitch_and_orient`. The probe resets the counters before `fillet_edges`, so the 260 also includes stitch and orient.
- **Topology intersector.** The fillet uses `TopologyIntersector<FilletObject>`:
  - `blend/fillet_executive/p1.rs:847`;
  - `session/insertion.rs:22`;
  - `session/insertion/live.rs:57`;
  - `session/original_trim.rs:67`.
- **Solves under `solve_with_face_caches`.** Each fetch inside these solves is a P1 or P2 copy, and the `with_face_caches` rerun loop repeats them:
  - `boolean/topology_intersector/p2.rs:974` (`iir_edge_brep_into`, one `brep_curve_solve` per edge);
  - `p2.rs:1328` (`edge_coincident_with_brep`);
  - `p2.rs:2640` (`edge_face_coincidence`);
  - `p1.rs:993`.
- **Fillet geometry, a fresh `BrepFaceServices` per call (P5-P8):**
  - `blend/fillet_geom/p5.rs:1738` (end side-edge SSI);
  - `blend/fillet_geom/p2.rs:4152` (rail interpolation drop curve);
  - `blend/fillet_geom/p6.rs:692` (`calc_blending_rail` point solve).
- **SSI under a face context.** Each SSI adds about 2 copies through `p5.rs:163-209`. The boundary start search then calls `surface_global_curve_intersect` (`p3.rs:559`) for each boundary iso curve whose box meets the intersection box. That is up to 4 per surface, and each call copies through P9-P13.
- **Orient** (`topology/brep/p3.rs:5162`, `orient_ray_votes`). For each face, up to 2 ray pairs. Each pair runs:
  - one `global_point_solve_into_with_context` with `BrepFaceServices::new(b)` (~`p3.rs:5262`);
  - 2 × `brep_curve_solve` (`p3.rs:5383`), each building `LiveBrepObjects::new` with **fresh** `FaceCaches`. So every face a ray reaches costs one copy (P6/P8).
- **Fast-drop state copies.** Plane-like faces in point solves reach `p4.rs:1735`, which copies through P2 or P7/P8.

**Split [UNVERIFIED].** There are two candidate explanations:

- **Scenario A:** topology intersector P1/P2 copies multiplied by the rerun loop. 14 preparations means at least 15 attempts.
- **Scenario B:** orient ray votes plus fast-drop state copies.

**[I]** Scenario A is the likely main share. FACE_TRIM_PASSES=20 and UV_TRIM_CREATIONS=14 fit a long rerun chain, and each attempt replays every earlier solve's fetches.

### 1b. The Boolean's 6 copies + 2 clones (ball/drill) and 8 + 2 (torus/box)

- **[V]** The 2 clones are P19, made once per SSI with a curved pair.
- **[V]** The SSI on a curved pair fetches 2 caches through `FaceCacheContext` (P1 via `p5.rs:163-209`).
- **[I]** The remaining ~4 copies (6 for torus/box) come from the boundary start search, through P9-P13 (`p3.rs:2017/2088/2097/2109/2170`). This is up to 4 iso curves per surface, filtered by the box test. Possibly some `p4.rs:1735` fast-drop state copies (P2) also contribute.
- **[I]** The exact per-site split is **UNVERIFIED**.
- **[V]** The slot case has 0 copies. That fits: plane and cylinder pairs go through analytic paths that never fetch a cache.

### 1c. check_brep's 2

**[V]** The two candidates:

- `topology/brep/validation/pointers.rs:1624`: `face_contact` builds `BrepFaceServices::with_caches(b, take(caches))` and then calls `drop_point_with_context`.
- `topology/brep/validation/assert_valid/seams.rs:123`: `BrepFaceServices::new` and then `assert_valid_with_face_context`.

**[I]** Each probably fetches once through `cache_mapped` or `cache_state_mapped` (P5-P8), giving 2. **UNVERIFIED** which one supplies which copy.

---

## Q2: Why each copy exists

The classes are: (a) the handle type or value really differs; (b) the consumer mutates the cache (including lazily, or to set a mode); (c) ownership or lifetime only (the trait returns an owned value, or the signature wants `&mut`); (d) incidental (the consumer reads a few fields).

| Class | Count | Sites |
|---|---|---|
| (a) handle differs | 3 | P3 `face/p4.rs:582` (new surface handle value); P16 `surface/p2.rs:1545` (new arenas); P17 `query/face/p2.rs:548` (offset maps to its own handles) |
| (b) mutates | 2 | P19 `p5.rs:725` (boundary search writes flags; the clones keep evaluation separate) [I on exact reason]; P20 `p7.rs:1717` (clone to set `point_test_enabled = false`) |
| (c) ownership only | 11 | P1, P4, P5, P6, P9, P10, P11, P12, P13, P14, P15 |
| (d) incidental | 3 | P2, P7, P8 (state copies: their only consumer, `p4.rs:1735`, reads 4 fields) |
| not classified | 1 | P18 `trim_srf_cache.rs:472` (not read; 0 in every measured case) |

Classification on the consumer side [V]:

- `p4.rs:1735` is (d).
- `p7.rs:2333` is (d).
- `topology_solver/p1.rs:2159` is (b), but its writes are lost, so in effect it is (d) plus wasted work.
- `p5.rs:580` is (b), mode setting only.

**[V]** Of the classes that fire in the measured cases (TRIM sites are 0), only (c), (d) and (b) remain, plus P3 if line solves reach it. **[I]** Every one of the 260, 8 and 2 is removable without changing what any consumer computes. Of these, only the mode-setting (b) cases need the solve modes moved first (Q3).

---

## Q3: `point_test_enabled` and `process_boundary_curves`

The fields are at `surface/surface_cache/p1.rs:1683` and `:1685`. `new` defaults them to `process_boundary_curves = true` and `point_test_enabled = false` (~`p1.rs:1772`).

### Writers [V]

**Set-and-restore (a solve mode written into shared state):**

1. `intersect/surface_drop_curve.rs:1720-1726` (drop trace)
2. `intersect/surface_tracer.rs:2104-2110` (`do_trace_base`)
3. `intersect/surface_intersector/p3.rs:446-457` (`find_boundary_start_points`, `process_boundary_curves`)
4. `query/tess.rs:488-491`
5. `query/tess.rs:670-679`
6. `surface/surface_cache/p3.rs:1182-1187` (`build_tree_with`, together with `face_containment_done`)
7. `surface/trim_srf_cache.rs:808-811` (implant)

**Permanent writers (they define what the cache is: trimmed or standalone):**

- `surface/trim_srf_cache.rs:415-416` (`new`), `:583-584` (`set_standalone_flags`), `:903-904` (build with trims)
- `surface/surface_cache/p3.rs:1248-1249` (`get_tree`, standalone trimmed)
- `blend/hcr.rs:708-709`
- `query/tess.rs:701-702`
- test only: `query/face/p2.rs:2890-2892`

**Writers on a private copy (mode set by copying):**

- `intersect/surface_intersector/p5.rs:580-581`
- `surface/surface/p7.rs:1718` and `:1729`

### Readers [V]

`point_test_enabled` (9):

- `topology/vertexuse.rs:1040`
- `query/brep/ray.rs:561`
- `intersect/point_class.rs:2162`, `:2482`
- `surface/trim_srf_cache.rs:650` (`point_test_in`)
- `surface/surface_cache/p1.rs:2228` (`point_test`), `:2254` (`point_test_with`)
- `surface/surface/p4.rs:1739`, `:1785`
- `surface/surface/p1.rs:3967` (`is_node_valid`)

`process_boundary_curves` (5 reads plus the getter at `surface_cache/p1.rs:2154`):

- `surface/surface/p10.rs:785` (`CsiGlobalSolver`, impl at `:709`)
- `surface/surface/p9.rs:1131` (`LsiGlobalSolver`, impl at `:334`)
- `surface/surface/p11.rs:1186` (`CsGlobalSolver`, impl at `:982`)
- `surface/surface/p11.rs:1590` (`SsGlobalSolver`, impl at `:1276`)
- `surface/surface/p8.rs:3397` (an argument to `sp_local_solve`)

**[V]** The local solvers already take `process_boundary_curves` as a parameter:

- `p6.rs:1855` and `:2410`;
- `p8.rs:244`;
- `p5.rs:2052` and `:2118`;
- `p11.rs:1291`.

### Signature change

**[V]** No per-solve mode struct exists today. The existing contexts are all about *where caches come from*, not *how to solve*:

- `SurfaceSolveContext` (`surface/surface/p1.rs:1180`);
- `DropSolveContext` (`intersect/surface_drop_lib/p3.rs:158`);
- `CoverSolveContext` (`surface/surface/p5.rs:1646`);
- `TopologySolveOptions` (topology solver p2).

**[V]** The global solver structs already hold a per-solve `surface_cache: Option<&SurfaceCache>` (`p10.rs:221, 298, 343, 922`). That makes them the natural home for a `SolveModes { point_test: bool, process_boundary_curves: bool }` field.

**Proposed change [I]:** the 7 set-and-restore writers and the 2 copy writers build a `SolveModes` and pass it down. The readers read the modes from the solver struct or from an argument instead of the cache. The permanent writers stay; they become the cache's *default* modes, which a solve starts from.

**Functions touched [I]:**

- 9 reader functions;
- 4 global-solver constructors plus the `sp_local_solve` caller;
- 7 set-and-restore writers;
- 2 copy writers;
- the `point_test` and `point_test_with` signatures.

That is about 24 functions in about 14 files, plus the call paths that thread the modes from a writer to its solver. Those paths were not enumerated, so the total is **UNVERIFIED**.

---

## Q4: Handle views (`O`)

### Where `O` is stored [V]

- **Tree** (`geom/tree.rs`):
  - `SurfaceAux.face` and `.surface` (`geom/tree.rs:270-286`);
  - `ObjectList.object` (`tree.rs:~316`);
  - `Tree.owner_curve` and `owner_surface` (`tree.rs:444-447`).
- **Surface cache:**
  - `BezierAux2d<O>` (`surface_cache/p1.rs:1224`);
  - `VertexListItem<O>` (`p1.rs:1154`);
  - `SurfaceCache` fields `surface_handle`, `trimmed_face`, the vertex/edgeuse/poly-edge lists and `subdivision_handles`.
- **Conversion:** `Tree::map_objects` (`tree.rs:779-822`) rebuilds every node and list item to change `O`. That rebuild is the inner cost of every copy.

### Reader kinds [V]

1. **Identity comparison** (`Some(x) != cache.surface_handle()` and similar), about 10:
   - `surface/surface/p9.rs:814, 823`;
   - `p10.rs:414, 424`;
   - `p11.rs:891`;
   - `p8.rs:3289`;
   - `geom/grid.rs:1242`;
   - `geom/tree.rs:1338, 1344`;
   - `surface_cache/p3.rs:499`.
2. **Emitted into `Solution.objects`** and handed back to the caller:
   - `p9.rs:726-728` (`owner_object()`, `original_surface`);
   - `p11.rs:1032-1033`;
   - `original_surface` (`surface/surface/p2.rs:5321`).

   This is what the caller later turns into topology. 22 non-test lines read `surface_handle()`, `owner_surface()` or `original_surface()`.
3. **Face lookup:** `face()` (`surface_cache/p1.rs:2196`) returns `trimmed_face: Option<O>` as the caller's face handle.
4. **Handles that carry data through `SolutionObject`** (`geom/solution_array.rs:287`: `kind`, `natural_interval`, `natural_uv_domain`). Example: the fillet's `UvSurfaceHandle(Extent2d)` (`blend/fillet_geom/p4.rs:265`) carries its UV domain *inside the handle*. `IdenticalSolutions` merging reads it.
5. **Tree-internal writes:**
   - `surface_cache/p3.rs:1213`;
   - `trim_srf_cache.rs:933`;
   - `surface_cache/p1.rs:2939/2972`;
   - `surface_cache/p2.rs:1342`.

   About 6 non-test lines in total.

### If the cache stored a fixed id type [I]

- Kinds 1 and 5 keep working unchanged, because equality on ids is the same test.
- Kind 2 must map an id back to the caller's `O` where the solution is emitted. That needs a per-solve table, or a `SurfaceSolveContext` method.
- Kind 3 likewise maps out.
- Kind 4 **breaks**: data-carrying handles such as `UvSurfaceHandle` cannot be a fixed id, so the domain must come from the cache's surface instead (`natural_uv_domain`).
- The `map_handles` differences (pole memo and subdivision handles cleared, stale face binding dropped) would have to become explicit rules, not side effects of a copy.

Net: a fixed id removes the reason for kind (a) copies (P3, P16, P17). It is a larger refactor than the (c)/(d) fixes, and it is **not needed** for the 260, 8 or 2.

---

## Q5: Smallest steps that can each land on their own

Each step changes only which object is read, never the values a solver sees. The checks for every step:

- the step's count assertion (from S0);
- the `blend` and `booleans` test binaries;
- the difftest corpus (fillet goldens such as `big-rad-filleted_box-*` and `stubs4-shell_fillet2-e1-r0.5`; Boolean torus/ball/drill cases);
- `tests/fork_edf7414_perforated_plate_work.rs` (Boolean work counters);
- `tests/fillet_face_owned_surface_context.rs`.

**[I]** Golden names were chosen by topic, not traced to these sites.

**S0. Counters only.**

- Change: add `surface_cache_copies` and `cache_trees_built` to `WorkCounters`, and a test that pins today's counts: 260 and 2 for the fillet, 6+2 and 8+2 for the Booleans, 2 for check_brep.
- Files: `kernel.rs` and the producer sites (Q6).
- Moves: nothing. This is the baseline AGENTS.md requires.
- Risk: none.

**S1. Borrowed state query in place of `surface_cache_without_get_tree`.**

- Change: add `SurfaceSolveContext::surface_cache_state(k, …) -> CacheState { trimmed, point_test_enabled, face_was_modified, face: Result<Option<O>> }`, computed on the kept cache, and call it at `p4.rs:1735`.
- Files:
  - `surface/surface/p1.rs` (trait);
  - `boolean/topology_intersector/p1.rs` (4303);
  - `query/brep/ray/live.rs` (718/726);
  - `intersect/surface_intersector/p3.rs` (context impl);
  - `surface/surface/p4.rs`.
- Moves: P2, P7 and P8, meaning every fast-drop state copy. The fillet share is **UNVERIFIED**; the fillet is plane-heavy, so possibly large.
- Why results do not change: the same four values are read off the same cache. `face` must apply the stale-binding rule that `map_handles` applies.
- Risk: low. That stale-binding rule is the only trap.

**S2. Solve modes as arguments (Q3).**

- Change: remove the P20 clone and the `p5.rs:580` mutation, and turn the 7 set-and-restore writers into arguments.
- Moves: P20 (fillet count unknown), and P19's 2 clones per SSI, if their only purpose is isolating the flag writes. That purpose is **UNVERIFIED**.
- Why results do not change: each solver reads the same boolean, from an argument instead of the field.
- Risk: medium. It touches about 24 functions, and a missed reader silently keeps the old field.
- Must land before S3, so that consumers lending the kept cache cannot write modes into it.

**S3. Lend instead of copy for same-type kept caches.**

- Change: replace `build_surface_cache -> SurfaceCache<O>` with `with_surface_cache(k, …, &mut dyn FnMut(&mut SurfaceCache<O>) -> Result<R>)`, or with a `Cow`-like `CacheRef` return. Implement it for `FaceCacheContext`, `BrepFaceServices`, `RayPreparation` and `TrimmedFaceContext`.
- Moves: P1, P4, P5, P6 and P9-P13.
- **[I]** This is the bulk of the 260 under Scenario A, and the Boolean's 6 and 8 except the clones.
- Why results do not change: identity `map_handles(Ok)` copies equal the original, except for the cleared pole memo, the cleared subdivision handles and the dropped stale face.
  - The pole memo is a memo, so clearing it changes no value [I].
  - The subdivision handles and the stale face must be shown unused, or kept as rules.
  - Consumers that write the copy now write the kept cache. That is acceptable only after S2. The alt-cache writes at `topology_solver/p1.rs:2159` would start persisting: that is extra kept memo, not a different answer [I], and needs a golden run.
- Risk: medium to high. The trait has many implementations, and the lazy tree build on `&mut` becomes visible across fetches. That is a **benefit** for reruns, which stop rebuilding.

**S4. Hoist `FaceCaches` across the orient pass.**

- Change: one `LiveBrepObjects` / `FaceCaches` for all of `orient_ray_votes` (`topology/brep/p3.rs:5162-5383`) in place of a fresh one per ray.
- Moves: the orient share only (**UNVERIFIED**, at most 2 rays × faces × 3 solves).
- Risk: low.

**S5. Make `drop_and_trim_curve` borrow** (`surface/surface/p10.rs:230`).

- Change: take `Option<&SurfaceCache>`, or lend `&mut`, once S3 exists.
- Moves: P14 (0 in measured cases unless drops reach it).
- Risk: low.

**Does hoisting alone cut 260 to a handful? No.**

- **[V]** The copy is made on every fetch, even in contexts that are already hoisted: `FaceCacheContext` keeps its caches for the whole operation and still copies at P1 on each fetch.
- **[V]** The rerun loop replays fetches.
- **[I]** Hoisting (S4) moves only the fresh-services share. The 260 reaches a handful only with S1 + S3, with S2 as S3's prerequisite.

---

## Q6: Two work counters

- **`cache_trees_built`.** Bump it in `SurfaceCache::build_tree_base_with` (`surface/surface_cache/p3.rs:984`). **[V]** That function takes `k`.
  - It matches the probe's BASE_TREE_BUILDS and TRIM_TREE_BUILDS_WITH_FACE, so either count one counter for both or keep two.
- **`surface_cache_copies`.** `map_handles` (`surface_cache/p1.rs:1873`) has no `&Kernel` **[V]**, and `Clone` cannot take one. There are two options:
  1. Add `k: &Kernel` to `SurfaceCache::map_handles` and `TrimSrfCache::map_handles`. This touches every caller (20 sites above plus tests), but every production caller already has `k` in scope **[I]**, since the trait methods take `k`.
  2. Count at the producer sites (P1-P20), where `k` is in scope **[V for P1/P2/P5-P13]**. This avoids a signature change but needs one line per site.

  Option 1 is safer: no site can be missed. `clone_for_evaluation` (P19) and P20 need explicit bumps at their call sites, because they go through `Clone`.
- **Test files.** Follow the `*_work.rs` pattern (for example `tests/fork_edf7414_perforated_plate_work.rs:94-121`, which resets and then reads `k.work_counters`):
  - `tests/fillet_cache_copies_work.rs`, registered with a `#[path]` line in `tests/blend/main.rs`;
  - a Boolean counterpart in the `tests/booleans/` area's `main.rs`.

  `every_test_file_is_built` (`tests/topology/main.rs`) enforces the registration. Assert the exact counts, so that each step lowers them visibly.

---

## Q7: What stays UNVERIFIED, and how to settle it

1. **How the fillet's 260 splits across P1/P2 (topology intersector, rerun-multiplied), P5-P8 (fillet geometry, orient, fast drop) and P9-P13 (SSI boundary search).** Settle it with a per-site counter: `#[track_caller]` on `map_handles`, `cache_mapped`, `cache_state_mapped` and each `build_surface_cache` impl, binned by `Location::caller()`. Add an attempt index in `with_face_caches`, so that copies per rerun show directly.
2. **The Boolean's 6 and 8 split (2 from P1 plus about 4 from P9-P13?).** Same probe.
3. **Which validation call makes check_brep's 2 copies.** Same probe.
4. **Whether P19's clones exist only to isolate the flag writes.** Read `find_boundary_start_points` and its callees for writes beyond `p3.rs:446-457` (alt caches, tree growth). Then rerun the probe after S2.
5. **Whether the subdivision handles cleared by `map_handles` are read by any consumer of an identity copy.** Settle by grepping the `subdivision_handles` readers and counting hits under the probe.
6. **The threading cost of `SolveModes`, meaning the number of intermediate functions.** Enumerate the call paths from each set-and-restore writer to the readers.
7. **Whether P3 (`FaceLineCache`) fires in the fillet,** since it counts under SURF_CACHE_MAP_HANDLES. Same per-site probe.
