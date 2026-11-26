# 19 Performance check of the six cache changes (counts before/after)

Method: temporary counters (never committed; kit in `perf-kit/`: `zz_probe_counts.rs`, `instr.py` inserts a counter at the top of each named function, `zz_perf_probe.rs` and `zz_perf_ops_probe.rs` probes, `mkstate.sh`). One scratch commit per state, same script on each, pushed to side refs on server2, built `--profile goldens`, tests `--test-threads 1`, 3 runs each (counts identical across runs). Raw logs: `perf-out/<state>.k{1,2,3}.log` (kernel probe), `<f0|f1>.o{1,2,3}.log` (ops probe). Tables: `perf-kit/tables.py`.

States (before -> after): A a4fba8af9c -> 8e0a9595dd (#20945, merged); B 6ea59f66d9 -> bad95c31f7 (#20942); C 92e51afae8 -> ce756f4c8b (#20944); D 471cafa25c -> 5b5e3fbceb (#20941); E cd4f88a899 -> 4a9ad5ae50 (#21019 head; cd4f88 is its merge-base and is OLDER than A-D and F, so E is measured on its own old base); F 5b1dc25542 -> 1562e5f516 (#21072). Note #20945 and #21072 are now MERGED (not open as the brief said).

Status: A, B, C, D, E, F all MEASURED on the scenarios below. NOT MEASURED: fillet removal (no cheap builder), crease splitting as its own scenario, the ops-layer sequence driven through solidcore-ops (done in the kernel crate with the same kernel calls instead), and of the 22 F measures: `section_curves`, `drop_curves`, `outline_cuts`, `section_frames`, `edge_as_curve`, and `face_tolerance` except through `face_square_level`.

Timing (indicative only, same machine, 3 runs each, ms): corpus plain read 3.9 s both sides in every pair (A 3954/3917/3929 vs 3939/3935/3905; others within 1-2%); rebuilding read 2.3 s both; variable fillet 29-30 ms both; slot Boolean 34-37 ms both. No timing difference beyond noise.

## Headline
No count rose except (1) work that was wrongly skipped before (A: stale gap caches after a scale or interval change are now recomputed), (2) bookkeeping counters that are new or renamed by design (set_interval call count, lend and state calls), and (3) nothing in F. Specifically:
- A: after a scale by 2, the next `check_brep` Level2 measures 42 edge/face gaps it used to take stale from the cache (GAP_EF_CALC 0 -> 42). Rebuilding corpus reads: +5 edge/face gap computes and +2 vertex/edge over 2111 / 2196 (0.2% / 0.1%). Fillet: unchanged computes.
- B, C, D: no extra work.
- E: no tree build or cache copy rose; copies fall (fillet 260 -> 2); depth checks and `mapped_state` calls rise by design (130 per fillet) but cost nothing measurable.
- F: every moved measure's whole-body copies fell or stayed; none rose.

## A. #20945 gap caches cleared (MEASURED)
Counters: `calc_max_edge_face_gap` (edgeuse/p1.rs, GAP_EF_CALC), `calc_edge_ccw_edge_gap` (GAP_CCW_CALC), `calc_vertex_edge_gap`, `calc_vertex_face_gap` (vertexuse.rs, VTX_*_CALC), hits counted in the four getters where `!force_calc && is_init` (GAP_*_HIT, VTX_*_HIT), `cached_max_edge_face_gap` entry (GAP_EF_CACHED_READ), `edge::set_interval` entry (SET_INTERVAL_CALLS) plus a read-only count of initialised caches the A1 setter clears (SET_INTERVAL_INIT_CACHES; on a0 it counts caches that are left stale, 0 on every a0 scenario because a0's direct writes bypass the setter).

| scenario | counter | before | after |
|---|---|---|---|
| scale by 2, check2 after (drilled plate) | GAP_EF_CALC | 0 | 42 UP |
| same | GAP_EF_HIT / CACHED_READ | 90 / 1206 | 48 / 738 down |
| same | VTX_EDGE_CALC, VTX_FACE_CALC | 36, 162 | 36, 162 same |
| scale by 1 (identity) check2 after | all gap counts | equal | equal |
| Boolean after the scale | all gap counts | equal | equal (SET_INTERVAL_CALLS 1 -> 9) |
| corpus 28 files, plain read | gap calcs/hits | 2111 / 4222 / 2196 / 4724 | identical |
| corpus read + every gap + check2 (plain) | all | identical |
| corpus rebuilding read | GAP_EF_CALC | 2111 | 2115 UP (+4) |
| corpus rebuild read+gaps+check2 | GAP_EF_CALC / VTX_EDGE_CALC | 2111 / 2196 | 2116 / 2198 UP (+5 / +2) |
| same | GAP_EF_CACHED_READ, GAP_EF_HIT | 69710, 12644 | 69706, 12643 |
| fillet edge / variable, default and all fixes | GAP_EF_CALC, VTX_FACE_CALC | 52 / 16 | 52 / 16 same |
| same | SET_INTERVAL_CALLS | 6 / 8 | 22 / 24 (direct writes now through the setter) |
| same | SET_INTERVAL_INIT_CACHES (cleared) | n/a | 16 |
| same | GAP_EF_CACHED_READ | 78 / 70 | 34 / 30 down |
| Booleans (plate drilled, slot, ball, torus, Steinmetz, shared face, edge in face, scale 1e3/1e5, cylinder slot, ...) | any gap compute | identical | identical |
| seq Boolean, update_tolerances_after_boolean, scale 2, Boolean | gap computes | identical | identical (SET_INTERVAL_CALLS 1 -> 9 per Boolean) |

Scale scenario: `s_plate_drilled` scaled by 2 with its tool.

Rises and their reading, from the code:
- scale-2 check: `transform_in` (topology/brep/p5.rs) now clears every edge use's `max_edge_face_gap` and `edge_ccw_edge_gap` when the scale is not 1; `check_brep` reads them through `max_edge_face_gap` and recomputes 42. Before, the 42 values measured at the old scale were used. This is work wrongly skipped (stale value), not avoidable. Note the vertex gap counts do not move, as the vertex setters already cleared theirs. (The edge/edge `GAP_CCW_CALC` did not run in this check on either side.)
- Rebuilding corpus reads: +4/+5 edge/face and +2 vertex/edge gap computes, one `set_interval` call with 8 initialised caches cleared (an edge join or split inside the rebuild now goes through the setter). Stale values before; justified. 0.2% of the gap computes of that scenario.
- SET_INTERVAL_CALLS rises everywhere because the fillet, join, split and reverse writes moved into the setter: this is a call count of a cheap function (a loop over the edge's uses), not recomputation. Cost: one `Vec` clone of the edge use list per call.
- NOT measured: whether a later reader of the 16 caches cleared in a fillet recomputes them; inside the fillet itself no compute rises (52 both). Any later check of a filleted body would recompute at most 16 gaps, again stale-before.
- CTX_ZONE_TOL rose 1866 -> 2190 and NESTING_WALKS 8223 -> 8925 only in `scale2/check2-after`: these are the recomputations' own work.

Avoidable? Not with the current design: the cleared values are wrong. A scale by a uniform positive factor could rescale gap values instead of clearing, but the PR argues the stored parameters are also wrong for non-uniform scale; not pursued.

## B. #20942 per-call generator (MEASURED)
Counters: `points_in_face` entry and requested points (PIF_CALLS, PIF_POINTS_REQ), `points_in_face_drawing` entry (B1 only, PIF_DRAWING_CALLS), `rand_dither` entry (DITHER_DRAWS), `shell::is_inner_shell_of` entry and each `ray_brep_solve` in it (IIS_CALLS, IIS_RAYS), `topology_traverser::is_inner_shell` (TRAV_IIS_CALLS). Scenarios that reach it: cavity Boolean, ball in cavity Boolean, sphere tangent box union, check_brep Level2 of the cavity body.

| scenario | before | after |
|---|---|---|
| cavity diff | PIF_CALLS 1 (3 pts), DITHER 3, TRAV_IIS 1 | PIF_DRAWING 1, DITHER 3, TRAV_IIS 1 |
| ball cavity diff | PIF 1 (12 pts), DITHER 12, TRAV_IIS 1 | DRAWING 1, DITHER 12, TRAV_IIS 1 |
| sphere tangent box union | same as ball | same |
| check2 of cavity | PIF 1, DITHER 3, IIS_CALLS 1, IIS_RAYS 1 | DRAWING 1, DITHER 3, IIS_CALLS 1, IIS_RAYS 1 |

The PIF_CALLS -> PIF_DRAWING_CALLS change is the same call (the inner-shell tests now call the generator-taking variant). Draws, rays and calls identical; no count rose; nothing samples more. Every other scenario (corpus, fillets, 40 Booleans) has no differing counter. Seed construction cost is a struct build (`Rng::new`), not counted.

## C. #20944 body-owned size estimate (MEASURED)
Counters: `brep_size_zone_tol_3d`, `context_zone_tol_3d`, `calc_face_zone_tol_3d`, `Brep::this_model_size_estimate`, `query::brep::calculate_bounding_box`.

| scenario | counter | before | after |
|---|---|---|---|
| corpus 28 plain read | CALC_FACE_ZONE_TOL | 436 | 0 |
| same | BREP_SIZE_ZONE_TOL | 0 | 4218 UP |
| same | MODEL_SIZE_READS | 40 | 4258 UP |
| same | CTX_ZONE_TOL, BBOX_CALLS | 177523, 57 | equal |
| corpus rebuild, read+gaps+check2 | same pattern | | BREP_SIZE 4247/4250 UP, CTX and BBOX equal |
| fillets, scale check2 | BREP_SIZE_ZONE_TOL | 0 | 2-3 UP |
| Booleans (merge corner, scale 1e3/1e5, Steinmetz) | all of the five | equal | equal |

Reading: readers (edge, vertex heal props, corner search, `new_brep`) now call `brep_size_zone_tol_3d` (4218 calls in the corpus reads). Each call is two field reads and a compare, then either `size_estimate_to_zone_tol_3d` (a range chain) or `context_zone_tol_3d` (counted unchanged, since the context read count is equal before and after); no bounding box and no loop. BBOX_CALLS equal in every scenario. The only avoidable part would be hoisting the call out of the per-edge/per-vertex loops (4218 calls over 28 files); the cost is negligible and not worth it. Crease splitting: NOT measured separately.

## D. #20941 step latch removed (MEASURED)
Counters: `c6_compute_point_values` (CPV_CALLS), `FilletSolver::step_solve` (FSOLVER_STEP_SOLVE), `offset_evaluate_point` (OFFSET_EVAL_POINT), `latch_fillet_point_step` (LATCH_CALLS, before only).

| scenario | counter | before | after |
|---|---|---|---|
| edge fillet, default kernel | OFFSET_EVAL_POINT | 4 | 4 |
| variable fillet, default kernel | CPV / STEP_SOLVE / OFFSET_EVAL | 55 / 164 / 564 | 55 / 164 / 564 |
| variable fillet, all fixes | CPV / STEP_SOLVE / OFFSET_EVAL | 55 / 424 / 1084 | 55 / 424 / 1084 |
| edge fillet, all fixes | OFFSET_EVAL | 4 | 4 |
| latch calls | LATCH_CALLS | 55 | 0 down |

Nothing rose; the latch function calls vanish.

## E. #21019 lent caches (MEASURED, on its old base cd4f88)
Counters (kit patch plus): tree builds (BASE_TREE_BUILDS, TRIM_TREE_BUILDS*), `SurfaceCache::map_handles` and `clone`, `TrimSrfCache::map_handles`, `cache_state_mapped`, `state` / `mapped_state` / `check_evaluation_copy_depth` / `reads_leave_unchanged` (new), `lend` and its shared-hit and new-copy branches, `check_geometry_nesting` walks.

| scenario | counter | before | after |
|---|---|---|---|
| edge fillet (default, all fixes) | SURF_CACHE_MAP_HANDLES (whole-cache copies) | 260 | 2 down |
| same | tree builds (base, trim, with-face) | 2 / 0 / 2 | 2 / 0 / 2 same |
| same | lend calls: new copy / shared | n/a | 130: 2 new, 128 shared |
| same | `mapped_state` calls, depth checks | n/a | 130, 130 |
| same | NESTING_WALKS (depth-check geometry walks) | 4528 | 4528 same |
| five Booleans using the fast-drop state read (cyl tangent plane diff, shared face diff, +1e-6, sunk, vertex on edge) | map_handles | 5 | 0 down |
| same | mapped_state / depth checks | n/a | 5 / 5 |
| corpus 28 reads (plain and rebuild) | tree builds, map_handles | 34 / 34 | 34 / 34 same |
| same | lend calls (all new copies, nothing shared) | n/a | 34 (66-69 in the gap scenarios), `reads_leave_unchanged` 34 |
| ball drilled, torus box/ball, Boolean builders | trees, map_handles, clones | equal | equal |

Reading: the state value is now computed per read by `mapped_state` (`check_evaluation_copy_depth` plus a face lookup, no copy). `check_geometry_nesting` walks are equal to before (the depth check short-circuits for planes and B-splines; where the surface is something else the walk is the same one `map_handles` ran). In the corpus reads each lend is a new copy (one per face) with a `reads_leave_unchanged` walk added on top (34 extra geometry walks over 34 copies, no copy saved because each is read once). That is one additional geometry scan per copy where no sharing happens: new work, small, avoidable only by checking `reads_leave_unchanged` lazily on the second lend. Copies created per operation: fillet 2 (was 260), Booleans 0 where the old code had 5. Peak live caches: NOT measured (only the number of copies made).

## F. #21072 reads use read_body (MEASURED, 17 calls of the 22 driven, plate and ball)
Counters: `Brep::copy` root (BREP_COPY_ROOT, every whole-body copy), `snapshot`, `with_rollback`, `clone_sharing_surface_caches`, `clone_standing_for_self*`, `clone_for_trial`, `brep_copy_into`. Copies per call, call 1 and call 2 identical.

| measure (plate and ball) | root copies before | after |
|---|---|---|
| bounds | 2 (snapshot + standing copy) | 1 (standing copy in the tight box) down |
| classify_point, classify_points | 2 | 1 (standing copy) down |
| face_at | 2 | 1 down |
| is_manifold_solid, solid_count, edge_closest_point, edge_closest_to_ray, rim_edges, vertex_distance, edge_length, edge_middle, edge_face_gap_samples, faces_on_one_surface, edge_comb, iso_comb | 1 (snapshot) | 0 down |
| face_square_level (calls non-moved `face_plane_normal`, `face_centroid` on with_body) | 1 ball / 2 plate | same (not a moved copy) |

No measure went up. The remaining standing copy in `bounds`, `classify_point(s)` and `face_at` is the clone the brief expected (the tight bounding box / BrepClassifier clone on `&Brep`): it was there before and still is, so these three still copy once per call. Not driven: section_curves (part of `edges_lie_on_face`), drop_curves, outline_cuts, section_frames, edge_as_curve (they need curve arguments) : NOT MEASURED. (Measures for which no counter fired, e.g. `edge_length`, did not copy after the change.)

## What was instrumented
All via `perf-kit/instr.py` at the top of: edgeuse/p1.rs `calc_max_edge_face_gap`, `calc_edge_ccw_edge_gap`, `cached_max_edge_face_gap`, getters; vertexuse.rs `calc_vertex_edge_gap`, `calc_vertex_face_gap`, getters; edge.rs `set_interval`; face/p2.rs `points_in_face`, `points_in_face_drawing`, `rand_dither`; shell.rs `is_inner_shell_of` and the ray solve; topology_traverser.rs `is_inner_shell`; geom/tol.rs `brep_size_zone_tol_3d`, `context_zone_tol_3d`, `calc_face_zone_tol_3d`; brep/p1.rs `this_model_size_estimate`; query/brep.rs `calculate_bounding_box`; blend/fillet_solver p3/p4 and fillet_custom_solver.rs; kernel.rs `latch_fillet_point_step`; surface_cache/p1.rs, p3.rs, trim_srf_cache.rs, surface/p1.rs, p2.rs, edgeuse/p3.rs, face/p3.rs (the older kit counters); query/brep/ray/live.rs `lend` and `cache_state_mapped`; query/face/p2.rs cache reads; topology.rs `copy`, the clone and snapshot functions; brep/p3.rs `brep_copy_into`. Exact line numbers per state are in the scratch commits `cache-perf-<state>` (local worktree `sc-wt/cache-perf-x`) and the apply logs `perf-kit/instr-<state>.log`.

Caveats: counts use my own probes (one body per scenario, not whole corpora of Booleans); the D scenarios reach the latch only through the fillet solver's finite-difference path.

## Cleanup
Server worktree `cache-perf-x` and its `bin` removed with `cargo xtask`; server2 `/` 68% used (269G free). Remaining on server2: side refs `push/cache-perf-*` in `sc-main` and logs in `~/cache-perf-out`, `~/cache-perf-*.log`, scripts `~/cache-perf-run-state.sh`, `~/cache-perf-all.sh`. Local: worktree `sc-wt/cache-perf-x` (branches `cache-perf-{a0,a1,b0,b1,c0,c1,d0,d1,e0,e1,f0,f1}`, never pushed to GitHub), `perf-kit/`, `perf-out/` under reports/cache-arch.
