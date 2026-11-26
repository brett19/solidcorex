# 23 Performance follow-up: the three things report 19 left unmeasured

Method as in report 19: temporary counters, scratch commits only (local worktree `sc-wt/cache-perf-x`, branches `cache-perf2-{a0,a1,e0,e1,f0,f1}`, never pushed to GitHub), built `--profile goldens` on server2, tests `--test-threads 1`. Kit: `perf-kit2/` (`zz_probe_counts.rs`, `zz_stale.rs`, `instr_a2.py`, `instr_e2.py`, `instr_f2.py`, probes `zz_perf2_{a,e,ops}_probe.rs`, `mkstate2.sh`, `run-state2.sh`, tables `tab_{a,e,f}.py`; it applies `perf-kit/instr.py` first). Raw logs: `perf-out2/` (`a0 a1 e0 e1 f0 f1 .run1.log` clean counts; `a0/a1 .stale.log` the same tests with the stale classification on; `a1.dump.log` the stale records). Counts were identical in the clean and the classified runs. Timings are indicative only (nothing moved beyond noise: corpus plain read 3.92 s e0 / 3.87-3.97 s e1).

States: A a4fba8af9c (a0) -> 8e0a9595dd (a1); F 5b1dc25542 (f0) -> 1562e5f516 (f1); E merge-base b039c29b90 (e0) -> b3358de953 (e1, head of cache-b3-lend).

## Headline (rises first)
- **A: five rises, all work that was wrongly skipped or an interval-changing clear, none avoidable by an equality test on the interval.** After an edge or variable fillet, `check_brep` (Level0 and Level2), `valid_and_outward_failure`, `update_tolerances_after_boolean` each do **+4** `calc_max_edge_face_gap` (20 -> 24); a second fillet on another edge **+3** (far edge) or **+2** (edge meeting the first fillet); Boolean and volume **+0**. The other three calculators do not move (edge/edge 0 -> 0, vertex/edge 30 -> 30, vertex/face 102 -> 102). Of the 4: 2 reused a stale record in a0 and 2 reused a record bit-identical to the fresh one (clear unnecessary by value, interval did change). No `set_interval` call with an identical interval cleared a cache anywhere (0 of 334 in Booleans, 0 of 8 per fillet, 0 in corpus reads).
- **F: nothing rose.** Each of the five unmeasured reads copies the whole body once less per call (1 -> 0; `edges_lie_on_face` 2 -> 1, the rest being its target's `with_body`). No other copy, cache copy or tree build moved.
- **E: nothing rose in copies or tree builds on the current head.** Whole-cache copies fall (fillet 260 -> 2, 5 Booleans 5 -> 0 each); the only added work is the `reads_leave_unchanged` scan: corpus read 34 scans, 0.55 us each on average, 55 nodes in total, none saves a copy; the largest cache met has 52 nodes. Not worth making lazy.

## 1. Change A (#20945): do the 16 cleared caches get recomputed after a fillet?

Body: 100 x 100 x 10 slab, R3 edge fillet on the top front edge (`fillet_edges`) or a variable-radius fillet (R1 -> R2); default kernel and `UpstreamFixes::all()` give the same counts (rows below are the edge fillet; variable fillet equal except where stated). Each step runs on a fresh fillet result ("alone") and the steps also run in order on one body ("seq"). Calls: (a) `check_brep` Level0 and Level2, (b) `topology::brep::valid_and_outward_failure`, (c) `fillet_edges` R2 on the top back edge (far) and on the top edge at x = 0 (adjacent), (d) `boolean_difference` with a 20 x 20 x 10 box through the round, (e) `update_tolerances_after_boolean` (the kernel call `solidcore-ops` boolean.rs `after_boolean` makes), (f) `no_unsure_region` + `brep_volume_default` (what `Body::volume` runs, on a copy in ops). Every step returned ok.

Computing calls of the four calculators (`calc_max_edge_face_gap` EF, `calc_edge_ccw_edge_gap` CCW, `calc_vertex_edge_gap` VE, `calc_vertex_face_gap` VF), a0 -> a1:

| step (alone, on the edge-fillet result) | EF | CCW | VE | VF | of the rise: stale / equal |
|---|---|---|---|---|---|
| the fillet itself (reference) | 52 -> 52 | 0 | 0 | 16 -> 16 | - |
| (a) check_brep Level0 | 20 -> 24 UP | 0 | 30 | 102 | 2 stale / 2 equal |
| (a) check_brep Level2 | 20 -> 24 UP | 0 | 30 | 102 | 2 / 2 |
| (b) valid_and_outward_failure | 20 -> 24 UP | 0 | 30 | 102 | 2 / 2 |
| (c) second fillet, far edge | 46 -> 49 UP | 0 | 0 | 12 | 2 / 1 |
| (c) second fillet, adjacent edge | 38 -> 40 UP (variable: 34 -> 36) | 0 | 0 | 9 | 2 / 0 |
| (d) Boolean with a box | 4 -> 4 | 0 | 0 | 24 | none |
| (e) update_tolerances_after_boolean | 20 -> 24 UP | 0 | 30 | 60 | 2 / 2 |
| (f) volume | 0 -> 0 | 0 | 0 | 0 | none |

The variable fillet gives the same rows (VF 104 / 88 in two of them). In the in-order run (seq): check0 +4 (2 / 2), then check2, outward, fillet2_far (36 -> 36), boolean: +0 (the first check refreshed them), fillet2_adj +1 (equal), update_tolerances_after_boolean EF 46 -> 54 (+8: 5 stale / 3 equal) and VE 54 -> 59 (+5, all equal): these are the caches the second fillet and the Boolean cleared through splits.

How stale was judged. In a0 every non-forced cache hit (four getters, and the cached read in edgeuse/p3.rs fit code) computed the fresh value with caching off and compared the record's `Debug` text (every f64 shortest round-trip, so bit for bit bar the sign of zero). In a1 the setter stored each cleared record, and the next computing call on it compared old against fresh. The two agree: a0 reused 2 stale + 2 equal records where a1 computes 4 (check0), 5 stale + 3 equal where a1 computes 8 (seq update), etc. a1 residual stale hits for EF: 0 in every step.

- **Stale** (old record differs from fresh): `edge_t` and `face_uv` still sit at the position found on the unsplit edge (old edge_t 0.0 or 0.5, fresh 3.0 or 0.6875, ...). `gap_3d` is bit-identical in 134 of 144 stale records (all 0.0) and differs by at most 4.3e-14 mm in the other 10. So the value used in a tolerance decision was nearly right; the parameter the gap is attached to was wrong. That is work wrongly skipped, justified.
- **Equal** (record bit-identical to fresh): the interval had changed (split), the max sample stayed where it was. The clear was unnecessary by value there. It is not detectable from the interval: in every one of these calls the new interval differs from the old.
- The clearing write for all of them: `topology/brep/p9.rs:1247`, `edge::set_interval(b, edge_id, if same { &ivl1 } else { &ivl2 })` in `make_vertex_split_edge_with_curve_identity_state` (the fillet splits its edges). 4 calls with caches, 16 initialised caches cleared (all EF; the `:1248` call on the new edge and the `:1306`/`:1307`/`:1328`/`:1329` calls find nothing initialised). Interval changed on all 4.

Of the 16 cleared per fillet, only 8 are recomputed by (a)/(b)/(e) (4 calls, each refilling an edge use and its mate); the other 8 stay empty after every step in the table, and in (d) and (f) all 16 stay empty (the Boolean and the volume read none of them).

`set_interval` calls setting an interval bit-identical to the current one, and the initialised caches those calls cleared (counters `SETINT_SAME_*`, a1):

| scenario | calls total | same-interval calls | caches cleared by same calls | caches cleared by changed-interval calls |
|---|---|---|---|---|
| edge fillet (default or all fixes) | 46 | 8 | 0 | 16 |
| variable fillet | 48 | 8 | 0 | 16 |
| second fillet (far / adjacent) | 22 | 8 | 0 | 16 / 12 |
| Boolean of the fillet result with a box | 61 | 16 | 0 | 0 |
| 49 Boolean scenarios (report 19 set) | 845 | 334 | 0 | 0 |
| corpus 28 files, plain read | 0 | 0 | 0 | 0 |
| corpus 28 files, rebuilding read | 1 | 0 | 0 | 8 |

The same-interval calls are the split sites `p9.rs:1306/1307/1328/1329` (one half keeps its interval). They clear nothing because nothing was initialised at that moment; a "skip if identical" guard would save no recompute in any scenario measured, only the one `Vec` clone of the edge-use list per call.

Booleans and corpus (a0 -> a1, any of the four calculators): all 49 Boolean scenarios equal. Corpus plain read and read+gaps+check2 equal. **Rises in the rebuilding corpus read**: EF 2111 -> 2115 (+4); read+gaps+check2: EF 2111 -> 2116 (+5), VE 2196 -> 2198 (+2). Source: the one `set_interval` call in the rebuild (`topology/edgeuse/p3.rs:1022`, `create_uv_trim_curve`, the interval did change) clears 8 caches. The 4 EF in the read: a0 read 4 stale records through the cached read in the tolerance fit (`STALE_EFC` 4 of 4 stale); the +1 EF in check2 is a stale record (1 call), the +2 VE are 1 stale + 1 equal. So 5 of 5 EF stale, VE 1 stale / 1 equal. Also from report 19: the scale-by-2 rise (EF 0 -> 42, `transform_in` clears, not the setter): a0 reused 24 stale hits among the 42 caches, the rest equal records (gap 0, parameters unchanged by a uniform scale of lines and planes).

Residual, not part of A (same in a0 and a1): vertex/edge records reused as stale in 46 hits of a plain corpus read (92 with check2), 12 in the scale scenario, and 1 vertex/face hit after fillet + Boolean + tolerance update in the in-order run. The setter clears `vertex_edge_gap` only for the edge use's own vertex use and never `vertex_face_gap`.

## 2. Change F (#21072): the five reads not driven in report 19

Ops functions: `section_curves` is the `read_body` in `cap::edges_lie_on_face` (cap.rs:134); `drop_curves` is `sweep::drop_curves` (sweep.rs:1673); `outline_cuts` is `Body::outline_cuts` (measure.rs:1238); `section_frames` is `sweep::railed::section_frames` (railed.rs:258); `edge_as_curve` is `curve_fit::edge_as_curve` (curve_fit.rs:399). Bodies: plate (box 100 x 20 x 10 less a cylinder, as in report 19), a loft skin of two splines (spline faces and edges), a sphere less a cylinder (outline_cuts, as in the measure tests), a cylinder. Each call twice on a fresh body (call 2 identical to call 1). Site labels name the ops call, so the target's `with_body` in `edges_lie_on_face` is told apart from the moved read.

Whole-body copies per call (`Brep::copy` root = snapshot = rollback), f0 -> f1:

| measure | body | whole-body copies | clone_sharing / standing / trial | whole-surface-cache copies | tree builds (base, trim, with-face) |
|---|---|---|---|---|---|
| section_curves (`edges_lie_on_face`) | plate | 2 -> 1 down (left: the target's `with_body`) | 0 | 2 -> 2 (the target's) | 2,0,2 -> same |
| same | skin | 2 -> 1 down | 0 | 0 | 0 |
| drop_curves | plate | 1 -> 0 down | 0 | 0 | 1,1,0 -> same |
| same | skin (kernel refuses the drop, result Err) | 1 -> 0 down | 0 | 0 | 1,1,0 -> same |
| outline_cuts | sphere less hole | 1 -> 0 down | 0 | 0 | 4,4,0 -> same |
| same | plate, line in the top face | 1 -> 0 down | 0 | 0 | 1,1,0 -> same |
| section_frames | cylinder | 1 -> 0 down | 0 | 0 | 0 |
| same | plate | 1 -> 0 down | 0 | 0 | 0 |
| edge_as_curve | skin, spline edge | 1 -> 0 down | 0 | 0 | 0 |
| same | plate, line edge | 1 -> 0 down | 0 | 0 | 0 |
| reference: `face_centroid` (still `with_body`) | plate | 2 -> 2 (it and `to_record`) | 0 | 0 | 0 |

No measure's total rose; none stayed equal except the target copy inside `edges_lie_on_face` and the unmoved reference. Combined with report 19 that is 22 of 22 moved measures driven (face_tolerance only through `face_square_level`, which calls unmoved readers: still not driven alone). Not driven: a spline-face `section_frames` (needs an edge chain with a support face; the plate and cylinder cover the call), a successful `drop_curves` on the skin.

## 3. Change E (#21019, head b3358de953, against merge-base b039c29b90)

All counts e0 -> e1 (same probe on both). Whole-cache copy = `SurfaceCache::map_handles`; tree builds = base / trim / with-face (identical on both sides in every scenario).

| scenario | whole-cache copies | tree builds | body copies | scans | scan nodes (max in one) | lends new / shared | scan time |
|---|---|---|---|---|---|---|---|
| corpus 28 reads, plain | 34 -> 34 | 34 / 0 / 34 same | 412 same | 34 (32 true, 2 false) | 55 (2) | 34 / 0 | 18.6 us total |
| corpus 28 reads, rebuilding | 39 -> 39 | 41 / 7 / 34 same | 412 same | 34 (32 / 2) | 55 (2) | 34 / 0 | 19.2 us |
| corpus read + gaps + check2, plain | 81 -> 81 | 91 / 6 / 85 same | 432 same | 69 (65 / 4) | 112 (52) | 69 / 0 | 41.9 us |
| corpus read + gaps + check2, rebuilding | 83 -> 83 | 94 / 13 / 81 same | 416 same | 66 (62 / 4) | 107 (49) | 66 / 0 | 32.7 us |
| edge fillet (default; all fixes the same) | 260 -> 2 down | 2 / 0 / 2 same | 19 same | 2 | 2 (1) | 2 / 128 | 0.4-0.5 us |
| variable fillet | 0 -> 0 | 0 same | 18 same | 0 | 0 | 0 | 0 |
| 49 Boolean scenarios, total | 83 -> 58 down | 30 / 0 / 22 same | 4 same | 0 | 0 | 0 | 0 |
| the 5 Booleans with a fast-drop state read (cyl tangent plane diff, sunk 1e-6 diff, shared face diff, +1e-6 diff, vertex on edge) | 5 -> 0 each | same | same | 0 | 0 | 0 | 0 |

Per corpus read: 1.2 scans per file, none saves a copy (`LENT_ARCS_SHARED` 0 of 65 lent copies). Per edge fillet: 2 scans save 128 copies. Per Boolean: 0 scans (the Booleans only read the state bits: `mapped_state` 5 + 5 depth checks per such Boolean, no copy). `mapped_state` / depth checks: 130 / 130 per edge fillet, 25 / 25 over the Booleans, 0 in reads. Nothing rose: no scenario has more copies, tree builds, or body copies than e0.

The scan (`SurfaceCache::reads_leave_unchanged`, surface/surface_cache/p1.rs:2132, called from `LiveBrepObjects::lend` at query/brep/ray/live.rs:786): for the cache's surface, subdivision surfaces, iso boundary curves and alternate curve caches, `any_nested_geometry` (surface/surface/p2.rs:782) walks the nested geometry (procedural blend supports, revolution generator curves) asking whether any node is an offset surface or a curve on surface / tangent field. On every cache met there was exactly one root (the face's surface; no subdivision surface, iso curve or alternate cache existed) and 1-2 nodes, except a few caches in the corpus under `check_brep` Level2 (largest 52 nodes; the surface kind was not recorded). Mean 0.55 us per scan (corpus read), 41.9 us for 69 scans of check2.

Where a scan runs without saving a copy (all through `lend` -> `reads_leave_unchanged`, live.rs:786, from `base_global_line_intersect_from`, surface/surface/p8.rs:2109, the `LineCacheSource::Fresh` branch of a point-in-region ray):
- corpus reads: `brep_find_and_set_infinite_region` -> `find_infinite_region` -> `brep_ray_fire` -> `ray_fire_live` -> `solve_mutable_brep_point_kept` -> `brep_point_solve` -> `solve_trees` -> `local_solve` -> `solve_point_face` -> `global_line_intersect_under`: 34 scans (one ray per file region), each on a face hit once, 0 shared.
- `check_brep` Level2 of the same bodies: `assert_valid::check_with_keys_counted` -> `brep` -> `find_infinite_region`: 35 further scans, 0 shared.
- the same chain also runs when a primitive is built (e.g. `create_sphere` through `split_closed`); those scans fall in the probes' setup, not in the Boolean rows (the kernel Booleans measured run none).
Where it saves: `FilletSession::create_fillet_brep` -> `band_cuts_the_part_cached` -> `band_cuts::keeps` -> `section_breaks_through` -> `global_curve_intersect_from` (p8.rs:2953): 2 scans, 128 lends shared.

Judgement: making the scan lazy (scan on the second lend of a face) would remove 34 scans and ~19 us from a ~3.9 s corpus read (0.0005 %) and 69 scans / 42 us from the check; it would add a flag and a second-lend scan to the fillet path where all the benefit is. Not worth a follow-up on counts. (Peak live caches, and the memory held by an Arc nobody shares, were not measured.)

## What I could not measure
- A is measured through kernel entry points (`check_brep`, `valid_and_outward_failure`, `fillet_edges`, `boolean_difference`, `update_tolerances_after_boolean`, `brep_volume_default`), not through `solidcore-ops` (its volume is a `measure_body` on a copy; its fillet adds a snapshot). Fillet removal, crease splitting: not driven. Shadow tracking keys are ids in one body lineage; across the Boolean (new body, copies share ids) the Boolean row's "cleared, not recomputed" lines are only indicative, though its calculator counts are exact.
- The corpus-rise attribution (5 of 5 EF stale, VE 1 / 1) is by the cached-read and shadow counters together; the sums match (4 + 1, and 1 + 1) but the calc-with-cache-off path in the fit is counted as a calculator call without a shadow entry.
- F: spline-face `section_frames`, a successful skin `drop_curves`, `face_tolerance` alone.
- E: peak memory; scan time is a wall-clock sum with a clock per scan (about 1 us of the measured 0.4-0.6 us may be clock overhead); the Boolean rows are the kernel API on 49 scenarios, not the ops layer.

## Scratch state left
Server worktree `sc-wt/cache-perf2-x`, its branch, its `bin` build output, the side refs `push/cache-perf2-*` and the logs/scripts under `~/cache-perf2-*` removed (`cargo xtask rm bin/*` then `git worktree remove`; `df -h /`: server2 76 % used, 204 G free; main machine `/` 47 % used, 885 G free). Remaining: local worktree `sc-wt/cache-perf-x` (no build output; branches `cache-perf-*` from report 19 and `cache-perf2-{a0,a1,e0,e1,f0,f1}`, never pushed to GitHub), `perf-kit2/`, `perf-out2/`. Nothing on GitHub touched.
