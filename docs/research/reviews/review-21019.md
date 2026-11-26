# Review of tangency-labs/solidcraft#21019 (head 876cdf675c)

Branch `cache-b3-lend`. Fixes #20976. Steps: step 1 merge 9829322dd8 (state read without copy), step 2 merge 8362f8f991 (SolveModes), step 3 876cdf675c (lend one shared copy).
The review read every hunk of the step diffs and of the whole diff. Nothing was built and no tests were run (forbidden by the brief).

## Verdict: PASS WITH FIXES

I found no correctness bug. I could not construct a call sequence where any of the three steps changes a result. These fixes are needed before ready:

1. **Run the missing goldens units.** Run them forwards and `--reverse`, then add them to the body's MEASURED list. These units cover files that the diff changes, or code paths that now read through the shared copy, and are not in the list:
   - `surface.surface_p4` (step 1 rewrote the fast-drop state read in `surface/surface/p4.rs:1728-1760`)
   - `surface.surface_p1` (`is_point_valid_with` / `is_node_valid` now take `modes`)
   - `surface.surface_p3`
   - `intersect.surface_intersector_p1`, `intersect.surface_intersector_p2`, `intersect.surface_intersector_p4` (step 2 removed the permanent flag writes in `do_intersection_in`)
   - `boolean.topology_intersector_p2`, `boolean.topology_intersector_p3`
   - `blend.fillet_geom_p2` and `mass.integrator` (the only in-place B-spline `out_of_bounds_enabled` toggles, now possibly on a shared copy)
   - `intersect.ray_tracer` (live context lend)
   - `query.face_p1`, `query.face_p3`, `query.face_p4`
2. **Add tests that pin the step-3 guard and the drop rule** (in `query/brep/ray.rs`, next to the existing lend tests):
   - (a) A face whose surface or iso curve has an `OffsetSurface` or `CrvOnSurf` is lent `Owned`, not `Shared`. Today nothing tests `reads_leave_unchanged` returning false (`surface/surface_cache/p1.rs:2132`).
   - (b) After a real read solve through the lent copy, the lent copy's Debug still equals a fresh `map_handles` copy. The solve is a `global_curve_intersect` with no cache given, through `BrepFaceServices`, on a B-spline face so the atomics and memos run. `a_kept_cache_is_lent_as_one_copy` (`ray.rs:741`) compares Debug only before any solve, on a cylinder, so it does not test the "reads leave it unchanged" claim.
   - (c) A face whose kept cache has no tree, then gets a tree through `cache_mapped`, drops any earlier lent entry. Also, `clear` drops lent copies.
3. **PR body corrections:**
   - **Memory cost.** State that each lent face holds one extra full `SurfaceCache` (tree, patches, auxs) for as long as the `FaceCaches` lives. The `FaceCaches` moves with `KeptFaceCaches` across solves. Today the body says only "kept beside the kept cache".
   - **Tangent field.** "tangent-field caches change on read" is not supported. `curve/tangent_field.rs` holds no `Cell`, `Mutex`, `OnceLock` or atomic. Say the exclusion is conservative.
   - **Shared B-spline flags.** Add to the ARGUED list: the lent surface's B-spline `out_of_bounds_enabled` / `continues_past_domain` atomics are now shared by successive readers, where each reader used to have its own. This is safe because the only in-place writers restore on exit, and no solve context is reachable inside them, so no nested lend can see the toggled value:
     - `with_out_of_bounds_enabled_in_place` (`surface/surface_point_solve/p2.rs:1305`), used at `p2.rs:1021`, `p3.rs:446` and `blend/fillet_geom/p2.rs:2945`;
     - `mass/integrator/properties.rs:503-518`.
   - **New error path.** Name it: `BrepFaceServices::lend` returns `borrow_conflict` if the lent map is already borrowed (`live.rs:775-796`, `try_borrow_mut`). This is unreachable by reading, because `classify` takes no solve context.

## Findings (most serious first)

1. **[Medium, coverage] The step-3 safety claim rests on reading alone, and its tests do not exercise it.**
   - `query/brep/ray.rs:741` and `:764` show that the lent copy is the same object every time and that a mutable consumer gets its own copy. Neither runs a read solve through the `Shared` copy and then compares it to a fresh copy.
   - Neither test uses a geometry that `reads_leave_unchanged` rejects.
   - Scenario: a future change adds interior state to a curve kind that `any_nested_geometry` does not exclude, such as a memo that is not pure. The lent copy then carries that state from one reader to the next, and no unit test catches it.
   - Fix 2(a)/(b).
2. **[Medium, coverage] Goldens units missing for changed paths** (fix 1).
   - Step 1 changed `surface/surface/p4.rs`, but `surface.surface_p4` was not run.
   - Step 2 removed the permanent `c.point_test_enabled = false; c.process_boundary_curves = true;` on `caches[0..2]` in `intersect/surface_intersector/p5.rs` (old ~573-580). The intersector units p1, p2 and p4 were not run.
   - Inferred: by reading, the bound caches feed only trees, patches and polar boxes into the intersector (`adv_surface_intersector.rs:432-447, 492, 914`). They reach no flag reader after the boundary start points. The boundary pass gets `[SolveModes{false,true};2]` at `p5.rs:729`. So I expect 0 changed, but this is not measured.
3. **[Low, behaviour, argued] Copies from `TrimmedFaceContext` (`intersect/surface_intersector/p3.rs:2020-2045`) now carry the cache's built flags, not the forced (false, true).**
   - This is where they go:
     - its `build_surface_cache`;
     - the `solve_cache` that `coincident_drop` gets (`surface/surface/p8.rs:2905`, then `drop_and_trim_curve_in_context`, then `drop_curve_trace`, then `drop_trace`).
   - I checked the readers:
     - The drop's point solves use `global_point_solve_point_test_off` without a cache (`surface_drop_curve.rs:238-258`).
     - The point and coincidence solves inside `global_curve_intersect_from` pass `point_test_off` (`p8.rs:2778, 2789, 2894`).
     - The CSI tree solves use `gs.modes` (`p8.rs:3029`, `p9.rs`).
     - The tracer's set/restore was removed on the argument that no step reads the flags.
   - I found no reader of the flags, so there is no scenario. The body mentions the trimmed-face context copies at line 23, so this is acceptable as stated.
4. **[Low, body] Memory cost, tangent-field wording, shared atomics and the new error path** (fix 3).
5. **[Info] Limit of `point_test_off` / `SolveModes`.**
   - The fast drop in `global_point_solve_in` (`p4.rs:1714`, `1783`) tests the face only if the solve's mode and `cache.point_test_enabled` are both on.
   - So an override can turn point testing off but never on. This matches the old clone-and-clear in `p7.rs`, and the body states it at p8 `global_curve_intersect_from`. No change in behaviour.
6. **[Info] The Cs/Ss solvers read `cache.solve_modes()` (built flags) directly** (`p11.rs:1031, 1122, 1209, 1465, 1552, 1680`; `p7.rs:1883` and `2078`).
   - They are built only in `global_curve_solve_in` / `global_surface_solve_in`, and no override caller reaches them. Inferred: before, nobody flipped the flags on caches that reach them either. These are classified as correct, not missed sites.
7. **[Info] hcr.** `blend/hcr.rs:699-711` keeps the `face_containment_done = UnderConstruction` write. It applies to the hcr's own silhouette cache, a copy that is dropped after the trace and is never lent. Fine.

## Remaining flag reads at the PR head (B)

- **Build-time writes**, unchanged by design: `trim_srf_cache.rs:415-416, 602-603, 827-830, 922-923`; `surface_cache/p3.rs:1248-1249`.
- **Build-internal read:** `surface_cache/p3.rs:1182`.
- **State export:** `SurfaceCache::state` / `mapped_state` (`surface_cache/p1.rs:2066, 2087`).
- **State readers:** `intersect/point_class.rs:2162, 2482`; `topology/vertexuse.rs:1034`.
- **Solve-time reads:**
  - These now go through `SolveModes::of(cache, self.modes)`: `p8.rs:3369+`, `p9.rs:765+, 831+, 1132`, `p10.rs` (both validity checks and the boundary test), `p7.rs` SP local solve.
  - The Cs/Ss solvers use built flags on purpose (finding 6).
  - The p4 fast drop is an AND with the override (finding 5).
  - `trim_srf_cache.rs:669` point classify reads the cache's own flag. This is right for a classify that is not a solve.
- **Tessellation set/restore:** `query/tess.rs`, untouched.
- I found no missed solve-time site.

## Step 1 state, field by field (C)

- `SurfaceCacheState` has trimmed, point_test_enabled, face_was_modified, the face binding, and have-tree. It is read in place:
  - `mapped_state` first runs `check_evaluation_copy_depth`, which is the copy's error path. Then it filters the binding the way `map_handles` / `Clone` do.
  - This is pinned by `kept_cache_state_reads_as_its_copy` (`ray.rs:631`) and `mapped_state_depth_check_fails_as_the_copy_with_a_pending_extension` (`surface_cache/p1.rs:4170`).
- The topology intersector `built_cache` (`boolean/topology_intersector/p1.rs:4307-4326`) is a faithful extraction. The tree_requested branch now reads the built cache's state in place, where it used to go through `build_surface_cache`. Same map, same pending and error arms.
- Offset `EdgeBrep` (`offset/offset_executive/edgeuse.rs:636-725`):
  - The split into face / solve / state functions keeps each arm.
  - The state path's map changed from `cache_handle(arena)` (a `Result`) to `|e| Handle::topology(arena, e)`. These are identical; `cache_handle` never fails (`edgeuse.rs:584`).
  - The dropped `set_surface_handle(handle)` does not matter, because the state carries no surface handle and its readers (`p4.rs:1728-1760`) never read one.
- `query/face/p2.rs` `read_surface_cache_of_face`: the lease is still released on every path (`lease.release()` runs after the closure, on both Ok and Err).

## File table (25 files, all read hunk by hunk)

| file | step | verdict |
|---|---|---|
| goldens/src/units/surface_surface_p2.rs | 2 | `modes: None` added, OK |
| goldens/src/units/surface_surface_p5.rs | 2 | `modes: None` added, OK |
| solidcore/src/blend/hcr.rs | 2 | set/restore removed; containment write kept on its own copy, OK |
| solidcore/src/boolean/topology_intersector/p1.rs | 1 | `built_cache` extracted, state in place, OK |
| solidcore/src/intersect/surface_drop_curve.rs | 2 | drop_trace set/restore removed; point solves are point-test-off and take no cache, OK |
| solidcore/src/intersect/surface_intersector/p3.rs | 2 | modes threaded into boundary pass; new test placed correctly, OK |
| solidcore/src/intersect/surface_intersector/p5.rs | 2 | permanent writes removed, override at :729; goldens p1/p2/p4 missing (fix 1) |
| solidcore/src/intersect/surface_tracer.rs | 2 | do_trace set/restore removed, OK (argued) |
| solidcore/src/offset/offset_executive/edgeuse.rs | 1 | split into face / solve / state, OK |
| solidcore/src/offset/offset_geometry_creation.rs | 1 | trait returns state, OK |
| solidcore/src/query/brep/ray.rs | 1,3 | tests; step-3 tests too narrow (fix 2) |
| solidcore/src/query/brep/ray/live.rs | 3 | FaceCaches {kept, lent}, lend; OK, new error path to name |
| solidcore/src/query/face/p2.rs | 1 | `read_surface_cache_of_face`, OK |
| solidcore/src/surface/surface/p1.rs | 1,2,3 | state default, modes on validity, `LentSurfaceCache`; OK |
| solidcore/src/surface/surface/p2.rs | 3 | `any_nested_geometry` refactor; check unchanged, OK |
| solidcore/src/surface/surface/p4.rs | 1 | fast drop reads state; `surface.surface_p4` goldens missing (fix 1) |
| solidcore/src/surface/surface/p6.rs | 2 | coincidence checker `point_test_off`, OK |
| solidcore/src/surface/surface/p7.rs | 2 | clone removed, modes override, OK |
| solidcore/src/surface/surface/p8.rs | 2,3 | `*_under` / `*_from` modes; lend in line Fresh and curve no-cache arms, OK |
| solidcore/src/surface/surface/p9.rs | 2 | CSI reads `SolveModes::of`, OK |
| solidcore/src/surface/surface/p10.rs | 2 | same, OK |
| solidcore/src/surface/surface/p11.rs | 2 | Cs/Ss take `cache.solve_modes()`, OK (finding 6) |
| solidcore/src/surface/surface_cache.rs | 1,2 | re-exports, OK |
| solidcore/src/surface/surface_cache/p1.rs | 1,2,3 | state, SolveModes, `reads_leave_unchanged`, tests; OK |
| solidcore/src/surface/trim_srf_cache.rs | 1 | `mapped_state` on the trim cache, OK |

## Public API and callers (F)

- `SurfaceSolveContext::surface_cache_without_get_tree` now returns `SurfaceCacheState`. A new method `lend_surface_cache` has a default, so external implementors still compile:
  - `goldens .../intersect_topology_solver_p2.rs:287`
  - `goldens .../query_api_queries.rs:714`
  - `solidcore tests/support/brep_pair.rs:106`

  None of them overrides either method.
- `OffsetBrep::prepare_surface_state_cache` changed its return type. Its only implementor is `EdgeBrep`, which was updated.
- `FaceCaches` has no external users.
- `BrepFaceServices::new` callers are unaffected: `crates/smlib/src/core/split.rs:307`, `solidcore-ops/src/split.rs:215` and goldens `topology_brep_planar_section.rs:366`.
- This tree has no `cfg(feature = "cpp")` and no difftest crate, so the body's "difftest/cpp clippy not run" is moot. The full-CI check still applies: `cargo clippy --workspace --all-targets -- -D warnings`.

## PR body hygiene (G)

- `Fixes #20976`, `Changes a default: no` and `Test change:` are present.
- Counts are labelled "Measured with temporary counters, not part of this change". The PROVEN, ARGUED and MEASURED labels are used.
- No local paths and no session links in the body. The commits have only Co-Authored-By trailers.
- No thread or concurrency wording around `Arc` in the body, the diff comments or the commit messages ("handed to every reader in turn", "successive solves").
- The `Arc` is justified as sharing between successive readers. Fine.
- Missing items: see fix 3.

## Cannot be verified without a build

- `cargo test -p solidcore query::brep::ray` (the three new tests, and the edited `fast_drop_point_test_on_a_face_without_loops_while_the_tree_is_deferred`).
- `cargo test -p solidcore surface::surface_cache::p1` (`solve_modes_override_the_point_test_and_leave_the_flags`, `mapped_state_depth_check_fails_as_the_copy_with_a_pending_extension`).
- `cargo test -p solidcore intersect::surface_intersector::p3::tests::boundary_solves_run_in_the_given_modes_and_leave_the_cache_flags`.
- For each unit in fix 1, forwards and with `--reverse`: `cargo run --profile goldens -p goldens -- --unit <unit> --goldens`.
- `cargo clippy --workspace --all-targets -- -D warnings`, since `default-members` hides some crates.
- The count table (temporary counters, not reproducible from the tree).

## Verified (by reading) vs inferred

**Verified:**
- The kept map is written only by `insert` (private; drops lent), `clear` (both maps) and the test-only `get_mut` (drops lent).
- `cache_mapped` replaces a kept cache only when it is missing or has no tree.
- Lent and kept move together through `KeptFaceCaches` and `with_caches`.
- Every `Shared` lend uses the identity map.
- The binding check reads only plain owner fields.
- In `do_intersection_in`, the bound caches feed no flag reader.
- The drop point solves take no cache.
- The external implementors still compile without changes.
- The body has the required lines and no local paths.

**Inferred (argued, not measured):**
- The memos behind the shared copy are pure.
- B-spline in-place toggles never overlap a lend.
- The `borrow_conflict` path in `lend` is unreachable.
- The Cs/Ss solvers are never reached from an override caller.
- 0 golden changes in the missing units.
