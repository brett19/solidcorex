# 14: Boolean coupling probes (local, never pushed)

A local worktree (branch `cache-measure`). Crate `solidcore/crates/solidcore`.
Nothing here has been compiled: by instruction, no builds were run while writing it.

## Run

```
cargo test --profile goldens -p solidcore --test zz_boolean_coupling_probe -- --nocapture --test-threads 1
```

There are two tests: `coupling_and_preparation` (probes 1 and 3) and `trim_pass_idempotence` (probe 2).
- Grep for `^COUPLE`, `^PREP`, `^PREPCOUNTS` and `^IDEM`.
- `COUPLE TOTAL` gives each counter summed over every Boolean that ran.
- `COUPLE FIRED_IN` gives, per counter, how many Booleans it was non-zero in.

## Files

- `tests/zz_boolean_coupling_probe.rs` (new). It is registered as a `[[test]]` in `crates/solidcore/Cargo.toml`.
- `src/zz_probe_counts.rs` holds the new counters plus three helpers:
  - the pair-phase flag: `IN_PAIR_PHASE` and the `PairPhase` guard. `enter()` swaps the flag to true and counts `PAIR_PHASE_ENTRIES`; `Drop` restores the previous value, so an early `?` out of the loop clears it.
  - `bump_ph(all, pair)` bumps `all` always and `pair` only inside the loop. `bump_in_pair(pair)` bumps `pair` only inside the loop.
  - `note_paired_faces` and `take_paired` record the (side, FaceId index) of every face that reaches a face intersection inside the loop, in first-appearance order.

## Instrumentation edits (all marked `// LOCAL PROBE`, src/ relative)

| file:line | counter(s) | what it marks |
|---|---|---|
| boolean/topology_intersector/p2.rs:1613 / 1720 | `PairPhase::enter()` / `drop` | the pair loop `for (ii,&f1) in faces1...` in `intersect_topology` |
| boolean/merge/p4/arena_pair.rs:1589 | `note_paired_faces` | `ArenaPair::face_intersect`, after its K_FACE check: side 0 = brep1, 1 = brep2, index = FaceId |
| query/face/p2.rs:2436 | C1_APPLIED(_PAIR) | after `edge::set_tolerance` on the `EdgeToleranceRaise` returned by classify |
| query/face/p2.rs:2439 | C1_RETURNED_NOT_APPLIED | a raise was returned but `edge` is None, so nothing is written |
| intersect/curve_class/p3.rs:2361 | C1_ON_EDGE_AS_IS | `classify_edge_interval`: the interval is on the edge as it is, so no raise is considered |
| intersect/curve_class/p3.rs:2379 | C1_NOT_NEEDED | gap < xsect tol, so `add_curved_edge_run` runs and no raise is needed ("considered, not needed") |
| intersect/curve_class/p5.rs:2017 / 2019 / 2022 | C2_CONSIDERED / C2_NEEDED / C2_APPLIED | before `if vtx_zone < edge_zone`, inside it, after `access.set_vertex_tolerance` |
| intersect/curve_class/p5.rs:1301 / 1335 | C4_CONSIDERED / C4_APPLIED | surf/surf/surf vertex block: entry, and the first line inside `if found {` |
| intersect/curve_class/p2.rs:984 | C3_READ_ONLY_DEFERRED | `refine_cci_to_surfaces_in`: a large gap exists, so it is deferred to the mutable pass |
| intersect/curve_class/p2.rs:1033 | C3_LARGE_GAP_ENTERED | `refine_cci_to_surfaces_mut` before `refine_large_gap(` |
| intersect/curve_class/p2.rs:1376 | C3_EDGE_CURVE_REWRITTEN | `tail_apply`: `r.modified_edge_curve \|\| r.modified_mate_curve` |
| intersect/curve_class/p2.rs:1418 | C3_TRIMS_DELETED | after `delete_uv_trim_curve` in `tail_after_apply` |
| intersect/curve_class/p2.rs:1476 | C3_MUTABLE_REFUSED | top of `fn mutable_gap` (the Unported "DISPATCH GAP" refusal) |
| topology/face/p3.rs:1305 | FACE_TRIM_PASSES_PAIR | `create_uv_trim_curves` entry, inside the loop |
| topology/face/p3.rs:2974 | FFI_CLASSIFY_STEP_EDITED | after the second `curve_on_classify_mut_with_seams`, when `objects.edited` |
| topology/face/p3.rs:2997 | FFI_EXTRACT_EDIT_CLEARS | the extract step's `if objects.edited {`, which clears the kept face caches |
| topology/edgeuse/p3.rs:482 | UV_TRIM_CREATIONS_PAIR | `create_uv_trim_curve`, inside the loop |
| surface/trim_srf_cache.rs:723 | TRIM_TREE_BUILDS_WITH_FACE_PAIR | `build_tree_with_face`, inside the loop |
| boolean/topology_intersector/p1.rs:4740 | WFC_TRIM_RESTARTS | `with_face_caches`: after `caches.trims.prepare` and before `continue` (a `PreparationRequired` restart) |
| boolean/topology_intersector/p1.rs:4777 | WFC_CACHE_RESTARTS | after `made.insert(face, cache)` (restart after building a surface cache) |

Behaviour is unchanged. Every edit is a counter increment on an atomic, a flag swap, or a Vec push behind a Mutex. The `else` branch added at query/face/p2.rs:2439 holds only a bump.

## Probe 1: `COUPLE <scenario>/<op>: ...`

**Mechanism.** Each scenario:
1. builds the target and the tool on a fresh `Kernel`;
2. clears every counter (`take`, `take_sites`, `take_paired`, `work_counters.reset`);
3. runs the Boolean alone and reads the counters.

**Each line carries:**
- `faces=a+b`, `face_pairs` (`k.work_counters.face_pairs()`) and `paired_faces` (distinct faces that reached `face_intersect`);
- `vtx_below_edge=a+b`: operand vertices whose tolerance is below one of their edges. C2 can only fire at such vertices;
- `uv_trims_in_pair` and `uv_trims_outside_pair`, then every counter as `NAME=value` (the `_PAIR` twins included).

A failing Boolean prints `FAILED <err>`. A failing build prints `SKIPPED`.

**Scenarios** (union/diff/inter where cheap):
- from zz_cache_count_probe: plate_drilled, slot_through_holes, ball_drilled, torus_box (D, I), torus_ball, round_drilled (D, U);
- tangential contacts: cyl_tangent_plane (U/D/I) and the same sunk 1e-6 (U/D), sphere_tangent_box (U/D/I), steinmetz (U/D/I);
- coincident faces: shared_face at 0, 1e-6 and 1e-4 overlap (U/D/I each);
- edge_in_face: a cube turned 45° about X with its edge on the top face (U/D), and the same sunk 1 (U/D/I);
- vertex_on_edge: a cube turned 45° about Z with a vertex on the plate edge (U/D);
- vertical_edge_in_side_face (U/D/I);
- scale_1e3 and scale_1e5: an r 0.1 and an r 0.001 pin through a 100 mm plate (D/U);
- regression geometries: grazing_column (issue_1057, a C1 case), hair_off_pin (fork_577f7be, I and D);
- for probe 2's trims: cylinder_slot (a slot across the seam) and ball_less_corner.

**What the outcomes mean:**
- **C1 and C2 near zero everywhere except grazing or tangent cases**: order coupling is rare, and the plan can treat the pair phase as read-only except for a short list of raises. These can be replayed after a parallel or reordered pass.
- **C1 or C2 applied in ordinary drills, with `_PAIR` equal to the total**: the raises happen in the loop and change what later pairs see. Reordering or parallelising pairs needs those writes deferred and merged (max of tolerances commutes), or a second pass.
- **C3_MUTABLE_REFUSED or C4_CONSIDERED > 0 inside the pair phase**: those paths error, because `IntersectObjectsMut::curve_mut` lends no curve.
  - For C3 that is the "DISPATCH GAP" Unported refusal; for C4 it is its `curve_mut` error.
  - These are refusals, not writes. Expect the scenario to FAIL, or to take a fallback.
  - C3_READ_ONLY_DEFERRED counts how often the read-only pass met a large gap at all.
- **FFI_EXTRACT_EDIT_CLEARS > 0**: a C2 write in the extract step has dropped the face caches. Each one is a cache rebuild the plan must either avoid or budget for.
- **uv_trims_in_pair > 0**: trims are still made lazily inside the loop, so the operands were not fully prepared. This motivates probe 3. Read it together with TRIM_TREE_BUILDS_WITH_FACE_PAIR and WFC_*_RESTARTS.
- **Expected on reading the code**: UV trims and tree builds inside the pair phase for every fresh-primitive operand. C1_NOT_NEEDED and C1_ON_EDGE_AS_IS should dominate C1_APPLIED. C2_CONSIDERED > 0 but C2_NEEDED rare, since `edge::set_tolerance` cascades to the vertices. C1_APPLIED > 0 in grazing_column. C3 and C4 near zero apart from the tangent and hair-off cases.

## Probe 3: `PREP <scenario>/<variant>: ...`

**Mechanism.** The same scenario is rerun on a fresh kernel, preparing the operands first with `create_uv_trim_curves(k, b, f, true, None)` per face. This is the call the cache build makes (query/face/p2.rs:258), with the editing flag set as `implant_face` sets it. Variants:
- `all`: every face of both operands, in `topology::brep::p1::faces` order;
- `paired`: only the faces `note_paired_faces` saw in the plain run, in face-list order;
- `paired_first_use`: the same faces in the order the loop first met them. This is extra, showing whether order alone matters.

**Each line compares with the plain run:**
- `core` (tolerances, edge curves and intervals, vertex points, face domains) and `full` (core plus the trims and gap memos) snapshots, giving SAME, or DIFFER with counts by kind and the first two differences;
- volume, area and the `check_brep` Level2 verdict (`valid`, number of lines, the first three);
- `prepared=` and `prep_errors=`, and uv_trims_in_pair and face_pairs plain versus variant.

A `PREPCOUNTS` line gives the variant's full counters. If paired face indices are not faces of the operands as built, a `NOTE` line says so.

**Outcomes:**
- **`all` SAME on core, volume, area and verdict, with uv_trims_in_pair = 0**: eager preparation can replace the lazy builds without changing results. This is the go signal for preparing before the loop.
- **`all` differs but `paired` SAME**: preparing faces that never pair changes something, probably tolerance raises from `adjust_tolerances=true` on faces that never pair. The plan should prepare only faces that will pair, and needs a broad-phase that knows them.
- **`paired` differs from `paired_first_use`**: the trim pass is order-coupled across faces, through shared-edge tolerance raises. Preparation then needs a fixed order, or a fixpoint (see probe 2).
- **Differences only in `full`, not `core`**: these are trims or gap memos that differ in representation only. They are harmless if volume, area and verdict agree.
- **uv_trims_in_pair still > 0 after `all`**: trims are deleted and rebuilt inside the loop (C3_TRIMS_DELETED, or split edges). Preparing cannot remove all lazy work.

## Probe 2: `IDEM <body>/<pass>: ...`

**Mechanism.** For each body:
1. take snapshot s1 (the probe 3 snapshot: every UV trim's interval, end points and control points/knots to full bits, edge curves, vertex points, tolerances, gap memos);
2. run pass 1 of `create_uv_trim_curves(..., true, None)` over every face in face-list order, then take snapshot s2;
3. run pass 2, then take snapshot s3.

**Printed lines:**
- `IDEM <b>/repeat`: s2 against s3, which is the idempotence question;
- `IDEM <b>/first`: s1 against s2;
- `IDEM <b>/gaps`: the returned `UvTrimCurveGaps` maxima per pass, errors, and the number of faces whose gaps differ.

**Bodies:**
- fresh box, cylinder, sphere, torus;
- the seam pin;
- a filleted slab;
- the result of every probe-1 scenario, which includes cylinder_slot (a cylinder side face cut by a slot), the torus cuts and ball_less_corner.

**Outcomes:**
- **`repeat` SAME everywhere**: one preparation pass is a fixpoint, and the cache can be built once, eagerly, with order-free reuse.
- **`repeat` DIFFER in `edge_tol`/`vertex_tol`**: the second pass raises tolerances again. Each pass widens the edges it adjusts, so preparation would drift with the number of rebuilds. The plan must make the pass monotone-idempotent or skip faces whose trims exist.
- **`repeat` DIFFER only in `eu_trim`**: trims are regenerated with different control data. Equal-geometry rebuilds would break bitwise goldens, and the cache should keep trims rather than rebuild them.
- **`first` DIFFER**: expected for fresh primitives, since there are no trims before the first pass.

## Compile confidence and risks

I checked every path against the source: imports, `Brep` accessors, `create_uv_trim_curves`, `UvTrimCurveGaps`, the fields, `transform_matrix`, `create_canonical_circle`, `create_linear_sweep`, `FaceId::from_index`/`index`, `CurveKind::{BSpline, Ellipse, Line}`, `BSplineCurve::{control_points, knots}`, `Ellipse::base`, `work_counters.{reset, face_pairs}` and `SolidError::invalid_input`.

I put my confidence that it compiles first time at about 80%. Likely nits:
- Inference on the `vec![(..., fn as Build), ...]` list. The first element is cast, so the fn items coerce to it.
- A `&&str`/`&str` mismatch in `compare`.
- Unused-import warnings.

The probe-counter names are the statics declared in zz_probe_counts.rs.

Semantic risks:
- **Pair attribution** assumes `face_intersect` sides map to brep1/brep2 by unchanged FaceId index. The probe prints a `NOTE` if any recorded face is not live in the as-built operand.
- **Snapshot evaluation**: `evaluate_point` on a trim could build a lazy memo. The snapshot reads it in the same order for each run, so this adds noise to `full` only, not to `core`.
- **`check_brep` mutates**: it runs after the snapshot and the measures, so it cannot affect the comparison.
- **Geometry**: the tangent, coincident and 1e-5 scenarios may fail inside the Boolean. That is reported as FAILED and is itself a data point.
- **Run time**: probe 3 runs up to 4 Booleans per scenario (about 190 in all), plus probe 2's about 50. Expect minutes in goldens profile.
