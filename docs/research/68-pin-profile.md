# 68 - Pin profile of gear_60 and plate_12 (research 66, step 0)

Status: **measured; proposed for review.** This is step 0 of 66 (§1, "Step 0, before any rank is
tasked"), answering review-66 F1 and F12.

The question: 66 ranked its top ten from a profile of solidcore main dbaedda4a, a month before the
pin. Do those shares still hold on the pin (solidcore 03e16c9e)? And why does the pin's
`drop_point_fast` miss the gear's cylinder?

Short answers:

- **The shares do not hold.** On the pin, gear_60's largest pre-pin node is gone:
  `classify_face_interval` falls from 40.0% to 5.7%, and the grid search under it from 35.5% to 0.0%.
- **`drop_point_fast` does not miss on the pin.** Every call on both cases takes the fast path,
  including the cylinder's 209 calls.
- **It did not miss at dbaedda4a either.** There, `classify_face_interval` asked 3,540 cylinder
  Intersect solves. The analytic path correctly returned "no point". Upstream's rule then ran the
  generic grid search anyway, which confirmed "no point" 3,540 times out of 3,540.
- **What changed is pair selection.** The pin pairs faces by trim-tight boxes: 572 face pairs
  intersected against 43,560, so those solves never arise (§4.2). The rule that ran the grid search
  is still in the pin.
- **What is left is a flatter profile.** On gear_60 the biggest costs are:
  - the uv trim-loop walk behind point-in-face (`contains_uv_point_with_topology`, 15.8%);
  - a planar face-pair prefilter that is new since 66's profile (`chords_apart` → `clip_to_slab`, 9.7%);
  - mass properties (14.1%);
  - imprinting (19.8%).

  On plate_12, a third of the run is building the 144 cylinders. The time goes into a certified
  ray fire inside stitching (`create_cylinder` 34.5%).
- **Order.** §5 proposes a new order for 66's top ten: primitives without stitching (old rank 5)
  moves to the top, and rank 1 is narrowed to the trim-loop walk.

## Labels

- **MEASURED**: from the runs described in §1, on the pin 03e16c9e unless marked. Shares are of
  profile samples. Counts come from side-channel counters in a local copy, never committed.
- **PRE-PIN**: from the 2026-10-27 benchmark profile of solidcore main dbaedda4a, the source of
  66's figures. That file is not in this repository.
- **INFERRED**: a judgement made here.

## 1. Method

**Cases.** These rebuild the recipes of the 2026-10-27 benchmark runner exactly:

- **gear_60**: a cylinder, r 50 and h 10. Sixty boxes (10 × gap × 12, with gap = 2π·45/60·0.45) are
  each placed at (45, −gap/2, −1) and rotated about z by 2πi/60 + 0.1234 using
  `topology::brep::p5::transform`. Each box is subtracted with `boolean_difference`.
- **plate_12** (`plate_holes_12`): a 100 × 100 × 10 box. A 12 × 12 grid of 144 r 2, h 12 cylinders at z = −1 are
  centred on a pitch grid. Each is subtracted in turn.
- **Both** then run `brep_volume(1e-6)` and `brep_area(1e-6)`.

Both cases give the same face counts as before (242 and 150), a solid body, and deterministic volume
and area bits across repeats. MEASURED.

**Build.** The pin's crate was copied out of the read-only checkout and built in release with line
tables (`debug = "line-tables-only"`), on the pin's toolchain (1.99.1). Nothing was changed in the
pin itself.

**Sampler.** The `pprof` crate (0.14), sampling in-process with SIGPROF at 997 Hz. This is the same
method and rate as the 2026-10-27 profile. Host `perf` was unavailable (`perf_event_paranoid` is
restrictive), and the setting was left alone.

| case | repeats | samples |
|---|---|---|
| gear_60 | 8 | 11,772 |
| plate_12 | 4 | 11,341 |

Each case's whole run is sampled: building the operands, the booleans, volume and area.
"Inclusive" is the share of samples with the function anywhere on the stack. "Self" is the share
with it as the leaf, after small generic helpers (`get`, `is_finite`, iterator adaptors, allocator
internals) are folded into their nearest named caller.

**Counts.** A second copy of the pin had local counters added to four functions, keyed by surface
kind, caller and operation:

- `global_point_solve_in` (`surface/surface/p4.rs`);
- `drop_point_from` (`surface/surface/p5.rs`);
- `point_3d_classify_at` (`query/face/p1.rs`);
- `classify_face_interval_with` (`intersect/curve_class/p7.rs`), as a thread-local "inside
  classify_face_interval" flag.
- the face-pair loop of `boolean/topology_intersector/p2.rs`: pairs considered, skipped by each
  prefilter, and intersected;
- the fallback exit of `global_point_solve_in`: did the generic solver find anything.

The same patch was applied to a copy of dbaedda4a, to compare (§4.2).

**Timing.** Unprofiled minima, MEASURED on a shared, loaded machine. Treat them as ±15%.

| case | wall | build | booleans | volume | area |
|---|---|---|---|---|---|
| gear_60, pin | 682 ms | 77 | 484 | 110 | 9 |
| plate_12, pin | 2,199 ms | 723 | 893 | 411 | 166 |
| gear_60, pre-pin record | 890 ms | — | 817 | 19 | — |
| plate_12, pre-pin record | 1,870 ms | 269 | 848 | 578 | 175 |

The pre-pin rows come from a different machine and day, so compare shares, not milliseconds. On the
pin, plate_12's volume (81904.42631532307) is closer to the exact value (81904.42) than the pre-pin
81904.95.

**Same machine, same hour.** Both copies carry the counters (the old one fires about 40× more of
them on gear_60), so take these as upper bounds on the old side. MEASURED, one run each.

| case | dbaedda4a | pin 03e16c9e |
|---|---|---|
| gear_60 wall (booleans) | 1,081-1,118 ms (989-1,022) | 694 ms (492) |
| plate_12 wall | 2,208-2,244 ms | 2,223 ms |
| plate_12 build / booleans / volume | 324 / 1,059 / 626-669 | 747 / 930 / 385 |

The gear's booleans take about half the time on the pin. plate_12's total is unchanged, but its
make-up moved: the build cost more than doubled (the certified ray fire in stitching), and
booleans and volume got cheaper.

## 2. gear_60 on the pin

### 2.1 Inclusive, top 25 named nodes

Wrapper chains are collapsed: each row is the outermost function of its chain that carries the
share. MEASURED.

| # | function | incl. % | self % |
|---|---|---|---|
| 1 | `boolean_difference` (all 60 cuts) | 72.5 | 0.0 |
| 2 | `intersect_insert_relate` | 56.2 | 0.0 |
| 3 | `intersect_topology` | 35.4 | 0.0 |
| 4 | `imprint_body` | 19.8 | 0.0 |
| 5 | `point_classify` (all callers) | 17.1 | 0.0 |
| 6 | `contains_uv_point_with_topology` (uv trim-loop walk) | 15.8 | 2.7 |
| 7 | `brep_volume` | 14.1 | 0.0 |
| 8 | `make_edge_in_face` | 11.7 | 0.0 |
| 9 | `set_point_class_parameters` → `global_point_solve_in` | 10.9 | 0.0 |
| 10 | `gap_closure` (mass, closing trims onto edges) | 10.6 | 0.0 |
| 11 | `editing_stages` | 10.4 | 0.0 |
| 12 | `kronrod_integrate` (all callers) | 9.9 | 1.4 |
| 13 | `live_face_fast_drop_test` → `point_classify` | 9.8 | 0.0 |
| 14 | `chords_apart` → `clip_to_slab` (planar pair prefilter) | 9.7 | 6.9 |
| 15 | `face_intersect` | 9.6 | 0.0 |
| 16 | `curve_on_classify_body` | 8.3 | 0.0 |
| 17 | `redistribute_inner_loops` | 8.0 | 0.0 |
| 18 | `create_box` → stitch | 7.6 | 0.0 |
| 19 | `classify_face_interval` | 5.7 | 0.0 |
| 20 | `make_other_faces` → `make_faceuse` | 5.0 | 0.0 |
| 21 | `eval_with_tangent` (under the loop walk) | 4.9 | 1.4 |
| 22 | `lifted_loop_area_sign` (cone outer loop) | 4.7 | 0.0 |
| 23 | `seed_records` | 3.6 | 0.0 |
| 24 | `transform` (rotating the boxes) | 3.6 | 0.0 |
| 25 | `face_properties_in` | 3.4 | 0.0 |

More, for the nodes 66 named:

- **Allocator.** malloc, free and realloc paths take about 9.4%; memcpy and memset another 5.0%.
- **Bookkeeping.** `delete_faces` 2.8%, `delete_face` 2.8%, `find_key_index` 2.8%,
  `is_missing_seam_at_vertices` 2.8%, `shell_connectivity` 2.3%, `check_keys` 2.2%, `drop_point_fast`
  1.6% and `classify_edge_faces` 1.6%.
- **Mass.** `brep_area` 1.2%.
- **Intersection.** `global_surface_intersect` 0.3%.
- **Gone.** `global_point_solve_1` (the grid search), `check_bad_guess_stride` and `fails_eval` are
  0.0%.

### 2.2 Self, top 15

| function | self % |
|---|---|
| `clip_to_slab` | 6.9 |
| `derivs_over` | 3.2 |
| `rational_basis_derivs_into` | 2.9 |
| dealloc | 2.8 |
| `BTreeMap` `find_key_index` | 2.8 |
| `contains_uv_point_with_topology` | 2.7 |
| realloc | 2.4 |
| `edgeuse` accessor | 2.3 |
| `basis_derivs_into` | 2.1 |
| `set2` | 2.0 |
| `first_derivs_net_into` | 1.9 |
| `evaluate` | 1.6 |
| `kronrod_integrate_with` | 1.4 |
| `eval_with_tangent` | 1.4 |
| `kind_entity` | 1.3 |

`clip_to_slab`'s self time is mostly `f64::is_finite` (6.7% inclusive). It checks each of eight
corners, and builds a fresh `Vec` of corners per call, for 16 pieces of every edge. MEASURED for the
share; the cause was read from the code.

## 3. plate_12 on the pin

### 3.1 Inclusive, top 25 named nodes

MEASURED.

| # | function | incl. % | self % |
|---|---|---|---|
| 1 | `boolean_difference` (all 12 cuts) | 44.1 | 0.0 |
| 2 | `create_cylinder` → `conic_solid` | 34.5 | 0.0 |
| 3 | `stitch_solid` → `do_stitching_body` | 32.7 | 0.0 |
| 4 | `find_and_set_infinite_region` | 29.5 | 0.0 |
| 5 | `partials_over_piece` (interval enclosures) | 28.0 | 7.2 |
| 6 | `brep_ray_fire` (from the infinite-region search) | 27.7 | 0.0 |
| 7 | `certified_pass` (certified line-surface roots) | 24.1 | 0.0 |
| 8 | `intersect_insert_relate` | 20.7 | 0.0 |
| 9 | certified `isolate` | 20.1 | 0.0 |
| 10 | `editing_stages` | 19.4 | 0.0 |
| 11 | `kronrod_integrate` (all callers) | 19.2 | 2.7 |
| 12 | `brep_volume` | 15.1 | 0.0 |
| 13 | `make_faceuse` | 14.5 | 0.0 |
| 14 | `visit` (isolate's subdivision) | 13.4 | 0.0 |
| 15 | `imprint_body` | 12.6 | 0.0 |
| 16 | `bound_trim_pair` (via `trim_same_parameter_deviation_bound`) | 11.3 | 0.0 |
| 17 | `make_edge_in_face` | 11.0 | 0.0 |
| 18 | `face_properties_in` (via `trim_integrand`) | 9.2 | 0.0 |
| 19 | `intersect_topology` | 7.6 | 0.0 |
| 20 | `cut` (isolate's split) | 6.7 | 0.0 |
| 21 | `face_intersect` | 6.7 | 0.0 |
| 22 | `widening_tiny` | 6.2 | 6.2 |
| 23 | `brep_area` | 6.2 | 0.0 |
| 24 | `gap_closure` | 5.9 | 0.0 |
| 25 | `redistribute_inner_loops` | 3.9 | 0.0 |

Also:

- **Edit post-processing.** `post_process` 3.4%.
- **Bookkeeping.** `seed_records` 2.4%, `glue_edges` 2.2%, `find_key_index` 1.9%, `check_keys`
  1.3%, `delete_face` 0.4%.
- **Point solves.** `drop_point_fast` 1.5%, `classify_face_interval` 1.0%, grid search 0.0%.
- **Allocator.** About 10.1%.

The `make_faceuse` share is almost all one path: `copy_uv_trim_curves_of_other_face` →
`trim_same_parameter_deviation_bound` → `bound_trim_pair` → `piece_reparametrised`.

### 3.2 Self, top 15

| function | self % |
|---|---|
| `partials_over_piece` | 7.2 |
| `widening_tiny` | 6.2 |
| `checked` (interval) | 4.7 |
| `round_down` | 4.1 |
| `first_derivs_net_into` | 4.1 |
| `evaluate` | 3.8 |
| dealloc | 2.9 |
| `with_capacity_in` | 2.8 |
| realloc | 2.7 |
| `kronrod_integrate_with` | 2.7 |
| `cmax` | 2.2 |
| `find_key_index` | 1.9 |
| `ders_basis` | 1.6 |
| `cmin` | 1.5 |
| `univariate_first` | 1.3 |

Interval arithmetic (`widening_tiny`, `checked`, `round_down`, `cmax`, `cmin`) is about 19% self.
It sits in the certified ray fire and in `bound_trim_pair`.

## 4. Old against pin, for the nodes 66 named

### 4.1 Shares

PRE-PIN comes from the 2026-10-27 record; pin is MEASURED here.

| node | gear_60 pre-pin | gear_60 pin | plate_12 pre-pin | plate_12 pin |
|---|---|---|---|---|
| `classify_face_interval` → `point_3d_classify` | 40.0 | 5.7 (classify 3.9) | 4.8 | 1.0 |
| … `global_point_solve` (grid search) | 35.5 | 0.0 | — | 0.0 |
| … `check_bad_guess_stride` | 27.4 | 0.0 | — | 0.0 |
| … `fails_eval` → `eval_second_derivatives` | 15.7 | 0.0 | — | 0.0 |
| `point_classify` → `contains_uv_point_with_topology` (all callers) | — | 15.8 | — | — |
| `intersect_topology` | 55.5 | 35.4 | 8.4 | 7.6 |
| face pairs intersected (count) | 43,560 | 572 | 33,480 | 288 |
| `face_intersect` | 48.4 | 9.6 | — | 6.7 |
| `chords_apart` → `clip_to_slab` | (absent) | 9.7 | (absent) | — |
| `editing_stages` | 25.5 | 10.4 | 14.3 | 19.4 |
| `make_faceuse` | 6.7 | 5.0 | — | 14.5 |
| `delete_faces` | 16.8 | 2.8 | 6.6 | — |
| `delete_face` | 13.0 | 2.8 | — | 0.4 |
| `seed_records` | 5.7 | 3.6 | — | 2.4 |
| `shell_connectivity` | 4.9 | 2.3 | — | — |
| `check_keys` | 2.4 | 2.2 | — | 1.3 |
| `BTreeMap::find_key_index` | 12.4 | 2.8 | — | 1.9 |
| `imprint` → `make_edge_in_face` | — | 11.7 | 21.1 | 11.0 |
| `redistribute_inner_loops` | — | 8.0 | 11.4 | 3.9 |
| stitching (`create_*` → `stitch_*`) | — | 7.0 (boxes) | 13.2 (cylinders) | 32.7 (cylinders) |
| mass (`brep_volume` + `brep_area`) | about 2 | 15.3 | 30 + 9 | 21.3 |
| … of which `gap_closure` | (absent) | 10.6 | (absent) | 5.9 |
| … of which Kronrod / Cash–Karp RK | — | 9.9 (Kronrod) | 24 (RK) | 19.2 (Kronrod) |

"—" means that node was not recorded for that case. "(absent)" means the function does not exist at
dbaedda4a.

Readings, INFERRED from the table:

- **gear_60.** The boolean dropped from about 92% of the run to 72.5%. The point-solve cost was
  removed rather than moved. Bookkeeping fell about fourfold (dense Marks, the seed-record fixes).
  Mass grew from almost nothing to 15%, mostly `gap_closure`, which is new since the old profile
  (#16735).
- **plate_12.** The plate's cost moved into building primitives (stitching rose 13.2 → 32.7%) and
  into `make_faceuse`'s trim-deviation bound. Both are certified paths added after dbaedda4a.
  Mass fell from about 39% to 21%: RK was replaced by Kronrod.

### 4.2 `drop_point_fast` and the cylinder

**Counts on the pin**, per case run once. Every call hit the fast path: MEASURED. "HIT" means
`drop_point_fast` returned the point and the result was used. There were zero misses, zero errors
and zero falls to the generic solver (`global_point_solve_1`) in either case.

| case | caller | surface | op | calls | outcome |
|---|---|---|---|---|---|
| gear_60 | `classify_face_interval`, face test off | plane | Intersect | 264 | all HIT |
| gear_60 | `global_point_solve_face_tested`, elsewhere | plane | Intersect | 1,092 | all HIT |
| gear_60 | `global_point_solve_face_tested`, elsewhere | cylinder (Cone) | Minimize | 120 | all HIT |
| gear_60 | `global_point_solve`, face test off, elsewhere | cylinder (Cone) | Minimize | 6 | all HIT |
| gear_60 | `drop_point_from` | plane | Minimize / Normalize | 2,956 / 540 | all HIT |
| gear_60 | `drop_point_from` | cylinder (Cone) | Minimize / Normalize | 24 / 59 | all HIT |
| plate_12 | `classify_face_interval` | plane | Intersect | 288 | all HIT |
| plate_12 | face-tested, elsewhere | plane | Intersect | 576 | all HIT |
| plate_12 | face-tested, elsewhere | cylinder (Cone) | Minimize | 288 | all HIT |
| plate_12 | face test off, elsewhere | cylinder (Cone) | Minimize | 1,152 | all HIT |
| plate_12 | `drop_point_from` | plane | Minimize | 122,304 | all HIT |
| plate_12 | `drop_point_from` | cylinder (Cone) | Minimize / Normalize | 4,030 / 288 | all HIT |

**What the cylinder does on the pin.** The pin's cylinder is an analytic `SurfaceKind::Cone` record,
not a NURBS surface. `Surface::drop_point_fast` sends Cone to `rev().drop_point_fast_using`, the
closed-form revolution inversion. This needs no grid and no Newton. READ, and confirmed by the counts.

`point_3d_classify_at` is called without a guess 264 times (gear_60) and 288 times (plate_12), all
from `classify_face_interval`. Every one is on a plane, because:

- the pin no longer intersects the face pairs whose curves run off the cylinder (§4.2, "Why the pin
  no longer pays it");
- the cylinder's remaining intervals are settled by their classified ends
  (`edge_curve_on_classify`, 1.8%), so they need no interior point solve.

MEASURED for the counts; INFERRED for the second reason, from the call site (`classify_face_interval_on`
solves only when neither end was classified).

So the gear's point-in-face cost on the pin is not inversion. Inversion is solved. The cost is the
**face test that follows a hit**:

- `live_face_fast_drop_test` → `point_classify` → `contains_uv_point_with_topology`, 9.8% under
  `set_point_class_parameters` alone and 15.8% across all callers;
- the uv trim-loop walk evaluates every trim edge (`eval_with_tangent` 4.9%), with no uv-box reject
  before it.

MEASURED.

**Why dbaedda4a fell to the grid search.** It did not miss either, in the sense of losing a point. The same counters on a copy of
dbaedda4a give the answer. MEASURED.

- **The calls.** On gear_60, `classify_face_interval` made 7,378 point solves without a guess (pin:
  264). Of these, 3,540 were cylinder Intersect solves.
- **The fast path.** `drop_point_fast` ran on every one, through the same Cone → revolution
  dispatch, and returned an empty set. The test point was not on the cylinder within tolerance, so
  "no solution" was the right answer.
- **The fallback.** `global_point_solve_in` treats an empty fast result as "did not work" and runs
  the generic solver (`global_point_solve_1`, the grid search with `check_bad_guess_stride` and
  `fails_eval`). That is upstream's rule ("arrive here when DropPointFast wasn't appropriate or
  didn't work"). The generic solver returned empty all 3,540 times.

So the pre-pin 35.5% was the grid search *confirming an analytic empty*. It was not a missed
inversion.

**Why the pin no longer pays it.** Those solves came from face pairs that never meet. MEASURED pair
counts:

| case | pairs intersected, dbaedda4a | pairs considered, pin | pairs intersected, pin |
|---|---|---|---|
| gear_60 | 43,560 | 656 | 572 |
| plate_12 | 33,480 | 288 | 288 |

The pin pairs faces by trim-tight boxes (`proved_bounding_box`, "not upstream"), not by the
surface's whole control net, and skips pairs farther apart than tolerance (WR-527). That is the 76×
cut. Its three newer prefilters skip only 84 more of gear_60's 656:
- `pair_apart_off_planes` skips 52;
- `chords_apart` skips 32 of the 416 plane-plane pairs it tests;
- `spans_apart` skips none.

On plate_12 none of the three fires. READ for the code, MEASURED for the counts.

**Still latent on the pin.** READ. The pin's `global_point_solve_in` keeps the same rule: an empty
`drop_point_fast` result still falls through to the grid search. These two cases no longer reach it,
but any case that asks an analytic surface about a point off it pays a full grid search to confirm
"none". Trusting an analytic empty for Intersect would remove that. It is truth-checked, not
bit-identical: the generic solver could in principle find a point the closed form rejects at the
tolerance boundary. INFERRED; listed in §5 as a guard, not a rank.

## 5. 66's top ten against the pin shares

| 66 rank | technique | pin evidence | moves |
|---|---|---|---|
| 1 | Point-in-face: uv-box reject, analytic inversion, reuse along a curve | Inversion already analytic and always hit. The trim-loop walk is 15.8% of gear_60 (2,441 loop walks, 24,077 containment edge drops) and absent from plate_12's top 25 | **down, narrowed**: a certified uv-box (and loop-box) reject before `contains_uv_point_with_topology`, plus a per-face loop cache. Merge with rank 2 |
| 2 | Per-face classifier cache | The setup that was costly (`check_bad_guess_stride`) is gone. What a cache would hold now is loop boxes and edge polylines for the walk | **merged into new 2** |
| 3 | Dense ids and live iteration | `find_key_index` 2.8% / 1.9% (was 12.4%); `editing_stages` 10.4% on gear_60 | **down** to about 7th |
| 4 | Box prefilter in `redistribute_inner_loops` | 3.9% of plate_12 (was 11.4%); 8.0% of gear_60, mostly `lifted_loop_area_sign` | **down**, below the top ten unless retargeted at `lifted_loop_area_sign` |
| 5 | Primitives built without stitching | `create_cylinder` 34.5% of plate_12, nearly all in the certified ray fire that finds the infinite region; boxes 7.6% of gear_60 | **up to 1** |
| 6 | Per-face mass memo | Mass 15.3% of gear_60 (`gap_closure` 10.6%) and 21.3% of plate_12 | **up to 3**, with `gap_closure`'s cost as the new target |
| 7 | Face-pair culling with one box tree | The pin already culls, and the culling itself costs 9.7% of gear_60 (`clip_to_slab`) to skip 32 of the 416 plane-plane pairs it tests | **retarget**: make the existing prefilter cheap (no per-call `Vec`, one finiteness check per box, edge-piece boxes built once per face). Bit-identical. New 4 |
| 8 | Edge samples as a pure function of the edge | Tessellation not in these cases | unchanged; unmeasured |
| 9 | Gauss-Kronrod | Already on the pin (`kronrod_integrate`, 9.9% / 19.2%) | **done on the pin**; port it, do not re-adopt |
| 10 | Filtered exact predicates | Not on these paths | unchanged |

**New candidates the pin profile shows.** MEASURED share, INFERRED fix.

- **The trim-deviation bound in `make_faceuse`.** `bound_trim_pair`, 11.3% of plate_12, is
  certified interval work on every copied trim pair. Carry the bound with the trim when the pcurve is
  copied unchanged, or compute it once per edge.
- **Allocator.** About 10% of both cases: per-call `Vec`s in hot predicates (`clip_to_slab`, the
  interval code's `with_capacity_in`). Scratch reuse is bit-identical.
- **Guard: trust an analytic empty.** The grid search after an empty `drop_point_fast` costs nothing
  on these cases now. It was 35.5% of gear_60 when pair selection was loose. Keep the port from
  re-inheriting the fallback: an analytic Intersect that returns empty should be final, after a
  truth check at the tolerance boundary (§4.2).

**Proposed order.** INFERRED from the shares above; gains are on these two cases only.

1. **Primitives with their topology known, so no stitch and no infinite-region ray fire** (old 5).
   Up to 33% of plate_12 and 7% of gear_60. Truth-checked: the same body, possibly a different
   entity order.
2. **Point-in-face trim-loop cost** (old 1 and 2, merged). A certified uv-box reject before the loop
   walk, and loop boxes and edge polylines cached per face for the boolean. Up to 16% of gear_60.
   Bit-identical if the boxes enclose.
3. **Per-face mass memo, and `gap_closure` cost** (old 6). 15 to 21% of both cases.
4. **Cheap face-pair prefilter** (old 7, retargeted). 9.7% of gear_60. Bit-identical.
5. **Trim-deviation bound carried with copied trims** (new). 11% of plate_12.
6. **Allocation in hot predicates** (new). About 10%, spread out.
7. **Dense ids and live iteration** (old 3). 3 to 10% now.
8. **Edge samples as a pure function of the edge** (old 8). Unchanged.
9. **Filtered exact predicates** (old 10). Unchanged.
10. **Box prefilter in inner-loop redistribution** (old 4). Narrowed to `lifted_loop_area_sign`
    (4.7% of gear_60).

Gauss-Kronrod (old 9) leaves the list: the pin already has it, and the port carries it.

Before any of these is tasked, re-measure it on scx HEAD for the layers solidcorex has ported. These
shares are for the pin's boolean, which solidcorex does not have yet (66 F12).

## 6. What this does not cover

- These are two recipes. Their mix (planar cuts on a cylinder; small cylinders through a plate) is
  not a corpus. A rank that moves here should be checked on the next corpus profile before it is
  tasked.
- Timings are from a loaded shared machine; shares are steadier than milliseconds, but each share
  has about ±1 point of sampling noise at these sample counts.
- Nothing here measures scx HEAD.
