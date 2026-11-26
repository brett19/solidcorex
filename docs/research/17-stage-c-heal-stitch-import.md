# 17. Stage C study: bodies that arrive or are repaired (read from data, heal, stitch)

Private. Read-only design study at 782ef9afc, following `briefs/stage-c-study.md` and the vocabulary of
report 07 (P preparable, R requestable, C coupled, S scratch; the five shared parts of 07 section 6:
a prepared view over `&Brep`, the phase table, typed requests, the fulfil loop, one kernel per worker).

Evidence marks: [V] read in code for this report, [A] taken from an earlier report, [I] inferred.
Paths are relative to `solidcore/crates/solidcore/src` unless another crate is named. App crates were
read with `git show HEAD:`.

Not reported as open, because their fixes merged after this checkout:
- the kernel's random generator (`Kernel::rng` is still in this checkout);
- the fillet step latch;
- the body size estimate leaking through the context (`fix_tol_sizes` still reads the context's here);
- the trim pass now done by `topology::brep::create_missing_uv_trim_curves`.

## 0. Who reaches which path (this decides the payoff)

There are four ways a body is made from data, and they are reached very differently.

| Route | Entry | Heal? | Reached from |
|---|---|---|---|
| Record to kernel | `solidcore-convert/src/brep.rs:206` `to_kernel` → `build` :249 | only when `trusted == false` (`heal_made`, about :308) | every app body's kernel form: `solidcore-ops/src/body.rs:464` `Body::from_record`, from `crates/kernel/src/body.rs:1087` `ops_body` and about 40 other app sites, all `trusted = true` [V]. The one untrusted caller is `solidcore-ops/src/surface_cvs.rs:1003`, which converts its result once as a check and drops it [V]. `solidcore-step/src/kernel.rs:123` uses `trusted = true` [V]. |
| Flat arrays to kernel (SMLib port) | `crates/smlib/src/core/convert.rs:2577` `brep_from_flat` | when the arrays' SOURCE is not 1 (`healer_enabled: true`) | `solidcraft_smlib::Brep::from_record(.., true)` and the pcurves path (`arrays_with_bare_uses` sets trusted) [V]. No production caller passes `false` [V: grep]. |
| Kernel file read | `topology/brep/p7.rs:758` `read_from_file`, `:776` `read_from_data`; `topology/part_file.rs:290` `read_part_from_db`; `topology/api_files.rs:103` | when the data's healer version is below `HEALER_VERSION` (`topology/brep/p10.rs` `make_topology`, after `check_identities`) | kernel API, goldens, difftest and the perf corpus. No app caller [V: grep]. |
| Stitch | `heal/stitch/p2.rs:440` `do_stitching`; `topology/brep/p5.rs` `stitch_and_orient` | n/a | the sweep's joined ends (`solidcore-ops/src/sweep.rs:1371` `api_stitch_into_solid`); the fillet executive (`blend/feature_executive.rs:1431, 1457, 1474, 1963`, through `stitch_and_orient` :2311) [V]. |

So in the app the legacy healer practically never runs. Every in-app record is trusted. Heal matters
only for the kernel's own file reads (corpus, goldens) and for untrusted arrays. Every heal entry point
passes `make_uv_trim_curves = false`: `api_heal_brep` → `brep_heal_brep` (`heal/heal_data/p1.rs:157`),
`read_from_data`, and both conversions [V]. So the healer's last-but-one step, `MakeUvtrimcurves`, is
dormant in practice.

## 1. Phases

### 1a. Record to kernel (`to_kernel`, the in-app arrival)

| # | Phase | file:line | Kind |
|---|---|---|---|
| I1 | `validate_built` | `solidcore-convert/src/brep.rs` about :255 | read |
| I2 | `data(k: &Kernel, rec, bare)`: one pass over the regions and shells, then the edges (curves from parameters or NURBS, :402-445), the use numbering (:447-484), then the faces (`face_surface` with label proofs: `labelled`, `BallPatch::create_certified`, `recognised`; record pcurves made into trims with `uv_trim_on_edge`, :485-600), then the vertices (:603) and `own_tolerances` (:617) | :321-636 | **read-like, already `&Kernel`.** Per edge and per face it is independent; key numbering is a sequential counter. |
| I3 | `make_topology_from_data` → `make_topology`: copies borrowed geometry, `set_editing_enabled(false)`, zone tolerance, builds regions, vertices, faces (`build_faces`), edges, uses and links; then `check_identities`; manifold flag (UNSURE → `is_manifold_solid`, `topology/brep/p2.rs:187`, pure `&Brep`); the heal is off (`healer_enabled: false`) | `topology/brep/p10.rs:416-530` | edit (arena construction, linear), then two reads |
| I4 | record order taken; `adopt` (blend definitions) | brep.rs about :290-292, :677 | edit |
| I5 | untrusted only: `heal_made(.., make_uv = false)`; the order is kept only if the face, edge and vertex lists are unchanged | about :300-310 | the heal (section 1c) |
| I6 | `put_enclosing_shells_first` | brep.rs end of `build` | classification on multi-shell regions only (rare) |

There is no trim check, no cover and no rebuild here [V]. A use gets a trim only from its record pcurve,
and only when `made.trims` is true and the use is not bare (brep.rs about :562-583) [V].

### 1b. Kernel file read (`read_from_data`)

| # | Phase | file:line | Kind |
|---|---|---|---|
| F1 | `BrepData::read_from_file` (parse) | `topology/brep_data.rs:1631` | read of bytes |
| F2 | `make_topology_from_data(.., make_uv = false, add_analytics = true, force_same_tol = true)`, as I3, **with the heal on** when the data's healer version is old | `topology/brep/p7.rs:776-796`, `topology/brep/p10.rs:433-530` | edit, then the heal |
| F3a | the default: `check_and_fix_uv_trim_curves`, an end check per use that deletes in the same loop (`topology/brep/p2.rs:486-496`), then `cover_kept_uv_trim_curves` | `topology/brep/p2.rs:536-564` | **the cover is already read-then-apply:** every gap is measured on `&Brep` (`kept_trim_gap` :582-622), then the raises are applied and recorded in `TrimCoverRaises` |
| F3b | `rebuild_uv_trim_curves_flag`: `rebuild_uv_trim_curves` on every use that has a trim | `topology/brep/p2.rs:454-477` | edit; **no cover follows** [V] |

Note the order: the heal (inside F2) measures gaps and sets tolerances BEFORE the trims are checked or
rebuilt (F3) [V].

### 1c. Heal (`heal_brep` → `HealData::heal_brep`)

`topology/brep/p1.rs:3727` builds a fresh `HealData` and calls `heal/heal_data/p1.rs:296` `heal_brep`,
which collects every vertex, edge and face, then calls `reset` / `load_targets` / `heal_targets`.

`heal_targets` (`heal/heal_data/p3.rs:235-253`) takes `with_editing_enabled`, a `MarkLock` and fresh
`Marks`, then runs `heal_targets_with` (:255-393). Its 23 steps alternate between Cache (measure) and
Fix (edit):

| Step | Kind | Notes |
|---|---|---|
| CacheEdgeprops (`heal/heal_data/p2.rs:2756`) | read-like | per edge: `EdgeProps::set_props` (:447-540) reads `eu1::edge_ccw_edge_gap` and `eu1::max_edge_face_gap` with force = false. Those are memo fills on the edge's uses and the mate (`topology/edgeuse/p1.rs:906, 945, 972-1053`; 20 samples; no trim needed). `set_props_metadata` (:542) takes `&Brep`. Each edge's props keep **edge-use ids**, whose gap lengths are re-read from the memos later. Histograms and max / min / average edge length are accumulated in target order. |
| CacheVertexprops (:2879) | read-like | per vertex: `vertexuse::vertex_edge_gap` and `vertex_face_gap` with force = false (memo fills) |
| CacheFacepropsGaps (:2952) | read | `set_props(PropStage::Gaps)` on `&Brep` (`heal/face_props.rs:1718, 1835`) |
| FixTolsizes (:3013-3250) | **apply** | model size from the largest edge length, bucketed; brep tolerance; then faces (`set_face_tolerance`), then edges (largest face tolerance plus the gap lengths from the memos; can shrink), then vertices (the edges just set; `ball_clears` :3532). Coupled in layers, face → edge → vertex, and deterministic. |
| CacheCoinVertices / FixCoinVertices | read (voxel model in `HealData`) / edit | |
| CacheDegenFaces / FixDegenFaces / FixDegenEdges | read / edit / edit | |
| CacheCoinEdges / FixCoinEdges | stub / edit | |
| CacheMissedEdgexsects / FixMissedEdgexsects / FixUncontainedEdges / FixBadgaps | read / edit ×3 | |
| CacheFaceprops2 | **read in name, writes in fact** | `set_props_with_query_gaps` (`heal/face_props.rs:1745`) runs in place, then `cache.apply_uv_trim_updates(b)`. What the lazy face queries leave stays on `b` (its doc): UV trims made by `HasSeamProblem` (`topology/face/p4.rs:2593`; `loop_sample_uv_point` :2627 makes a missing trim) and by `ClassifyLoops`' bisectors, the tolerance growth that comes with them, and gap caches. |
| FixMoveseam / FixSplitedgeAtseam / FixBadsheets | edit ×3 | `fix_bad_sheets` takes a second `MarkLock` |
| CacheFaceprops3 | read on a copy | runs on a copy whose edits are copied back |
| FixSplitfaceAtseams / MakeUvtrimcurves (dormant) / FixInfiniteRegions | edit | `fix_infinite_region` (`heal/heal_data/p1.rs:804`) casts rays with a fresh `BrepQuery` |

Every fix that edits calls `update_target_lists` (`heal/heal_data/p3.rs:156-227`), which drops stale
props, adds the new entities as targets and re-runs `heal_targets` recursively up to the current step, at
most 32 deep. **Heal is interleaved by design:** measure, fix, re-measure what the fix made.

### 1d. Stitch (`do_stitching`, `heal/stitch/p2.rs:440-470`, body :473-978)

| # | Phase | file:line | Kind |
|---|---|---|---|
| T0 | editing on, `doing_boolean` on (saved and restored) | :458-462 | flags |
| T1 | remove lamina sliver faces (per face: test, then delete at once) | :486-496 | interleaved |
| T2 | squeeze edges shorter than the stitch tolerance (per edge: 5-sample length, then squeeze at once) | :507-575 | interleaved |
| T3 | up to 10 iterations of: | :577 | |
| T3a | split edges at vertices: box (`calculate_bounding_box(k, &mut *b)`), an edge spatial tree built once per iteration, then per vertex a tree query and an immediate `test_and_split_edge` | :586-720 | tree build is a read; the loop is interleaved |
| T3b | vertex glue: per-vertex lamina and wire flags and projection parameters (read), sorted sweep; per vertex its coincidence list (distances), then `select_topology_to_keep` and glue or squeeze at once | :720-918 | the sweep is a read; the glue is interleaved |
| T3c | edge glue per vertex (`glue_edges_at_vertex` :980): for each edge pair sharing both ends, `drop_point` of one curve's midpoint onto the other, a tangent check, then an immediate glue, then a rescan of the vertex | :920-930, :980-1100+ | interleaved |
| T4 | count lamina edges, notify faces | :933-952 | read / notify |
| T5 | region nesting by a ray (more than one region) | :954-970 | read (fresh query) |
| T6 | `finish_joins` (not upstream, #18166): per live edge with pending covers, `cover_joined_faces` (`topology/edgeuse/p3.rs:2026`), which raises to the proved face bound, settles the bounds, covers the raised trims and the vertex ends | `heal/stitch/p1.rs:1163-1201` | per-edge measure plus raise |
| T7 | in `stitch_and_orient` only: `covered_trimless_faces` before the orientation (p3.rs:2368, a read) and `keep_trimless_faces_covered` + `cover_joined_trims` after it (p3.rs:2522) | `topology/brep/p5.rs:2980-3022` | already read before, apply after |

**Stitch is interleaved:** the pairing searches and the joins alternate per vertex and per pair, and
later searches see earlier joins. That is the algorithm's meaning (greedy), not an accident of the port.
The covers (T6, T7) are the only part already shaped as measure, then apply.

## 2. Writes in the read-like phases

### Record to kernel / file read

| # | Write | file:line | Frequency | Class |
|---|---|---|---|---|
| WI1 | the object-key counter `next_key` in `data()`, and `env.next_key` | brep.rs :330-334; `TopologyEnv` | always | S: number by entity index after the parallel map |
| WI2 | kernel memos keyed by content, filled through `&Kernel` during label proofs and curve checks (`Caches::curve_analysis` RefCell, `loop_area_memo`, `recertify_memo`, the blend station and thumbweight memos) | `kernel.rs:142, 883, 899, 904, 910` | commonly | S: a hit equals a fresh solve (their docs) [V doc]; one per worker |
| WI3 | `blend_net_tolerance` / `blend_counts` Cells | `kernel.rs:874-877` | blend faces only | S |
| WI4 | `doing_boolean` flipped on the context around the build | `topology/brep/p10.rs:416-431` | always | S: mode flag, restored |
| WI5 | the sampled-raise mode flag (section C) | `kernel.rs:149, 1046-1051` | import only | S: becomes an argument |
| WI6 | F3a: the end check deletes a failing trim inside the checking loop | `topology/brep/p2.rs:487-491` | rare | P: each use's check reads only its own trim and its edge's ends [V], so check everything first and delete after |

Totals: P 1, R 0, C 0, S 5. The cover (F3a) is already read-then-apply and adds nothing.

### Heal (the Cache steps)

| # | Write | file:line | Frequency | Class |
|---|---|---|---|---|
| WH1 | edge-use gap memos (`max_edge_face_gap` on the use and its mate, `edge_ccw_edge_gap`) | `topology/edgeuse/p1.rs:906-1053`; `heal/heal_data/p2.rs:447-540` | always, every edge | P (or a value: return the gap, write the memo in apply). One item owns an edge and all its uses, so there is no overlap between items. |
| WH2 | vertex-use gap memos (`vertex_edge_gap`, `vertex_face_gap`) | `heal/heal_data/p2.rs:694-776` | always | P, as WH1 |
| WH3 | histograms, step statistics, max / min / average edge length (a floating-point sum in target order) | `heal/heal_data/p2.rs:2756, 2879` | always | S: fold the per-item values in target order |
| WH4 | per-target props records in `HealData` | `heal/heal_data/p2.rs` | always | S: the item's output value |
| WH5 | **CacheFaceprops2 / 3: lazy UV trims and their edge tolerance raises, plus gap caches, written on `b` by the seam and loop queries** | `heal/face_props.rs:1745`; `topology/face/p4.rs:2593, 2627` | commonly on faces whose uses have no trim (unmeasured) | **C1** (below) |
| WH6 | `MarkLock` and `Marks`; `with_editing_enabled`; `doing_boolean` | `heal/heal_data/p3.rs:235-253` | always | S |
| WH7 | `fix_infinite_region` / `brep_find_and_set_infinite_region` build a fresh `BrepQuery` | `heal/heal_data/p1.rs:804` | 2+ regions | S |
| WH8 | inside the apply step FixTolsizes: `ball_clears` sets the vertex tolerance, measures, then restores it (a scratch write on the body) | `heal/heal_data/p2.rs:3532-3541` | always per vertex | S: pass the radius as an argument to the measure |

**C1, spelled out:** when CacheFaceprops2 tests face A, it may build a missing UV trim on one of A's
uses. That build raises the shared edge's tolerance (`trim_raise_distance`, `topology/edgeuse/p3.rs`
about :1820-1880). The neighbouring face B is tested later, and its seam and loop tests read that edge's
tolerance and trim. So B's answer depends on A's write.

The deterministic reformulation is report 11's: create the missing trims in edge order before
CacheFaceprops2 (now `topology::brep::create_missing_uv_trim_curves`). The stage then writes nothing, and
C1 becomes P. This changes results: the trims and the raises happen in edge order instead of in
face-query order. CacheFaceprops3 runs the same queries on a copy, and the same prepare covers it.

A coupling between phases (not inside one): the edge props hold edge-use ids, and FixTolsizes reads the
gap lengths through them from the body's memos. Any memo reset in between (for example #20945's clear on
an interval change, or an edit from an earlier recursive fix) changes what FixTolsizes reads. Holding the
gaps as values (WH1) removes this.

Totals: P 2, R 0, C 1, S 5. The fix steps and `update_target_lists` are edits and stay sequential.

### Stitch (the searches inside T3, and T6 / T7)

| # | Write | file:line | Frequency | Class |
|---|---|---|---|---|
| WT1 | **T3a: `test_and_split_edge` splits an edge in the middle of the per-vertex scan; later vertices are tested against the split edges (the tree is the iteration start's, the test is on the current edge)** | `heal/stitch/p2.rs:640-720` | when a vertex lies on an edge | **C2** |
| WT2 | **T3b: glue or squeeze raises the kept vertex's tolerance (`update_object_tolerance`) and deletes the other; later coincidence tests use `vertex tol + vertex tol` from the current body** | :805-910 | always on a stitch | **C3** |
| WT3 | **T3c: an edge glue deletes one edge, raises the kept edge's tolerance and re-runs the vertex scan; later pair tests use the raised `edge tol + edge tol` and the remaining edges** | :980-1100+ | always on a stitch | **C4** |
| WT4 | `pending_covers` pushed per join, split per edge split | `heal/stitch/p1.rs:941, 1115, 1136` | per join | S: the operation's own list |
| WT5 | T6: `cover_joined_faces` per edge: raise to the proved bound, settle, cover the raised trims, cover the vertex ends | `topology/edgeuse/p3.rs:2026`; `heal/stitch/p1.rs:1196-1200` | per joined edge | P [I]: each edge's measures read its own curve, its own tolerance and fit (`fit_tolerance` of e), and its faces. The vertex raises are max-raises shared between edges, so the order does not matter if each only compares (UNVERIFIED U5). Same shape as `cover_kept_uv_trim_curves`. |
| WT6 | T7: `covered_trimless_faces` before the orientation, kept after it | `topology/brep/p5.rs:2980-3022` | `stitch_and_orient` | P: already measured before and applied after [V] |
| WT7 | `calculate_bounding_box(k, &mut *b)` per T3 iteration | `heal/stitch/p2.rs:590` | per iteration | P (UNVERIFIED whether the non-tight box writes anything) |
| WT8 | `Kernel::arena_notify_records` reused across glues and splits; the editing and doing-Boolean flags; marks; the ray query in T5 | `heal/stitch/p1.rs:2704-2726, 2781`; `kernel.rs:890` | always | S: the doc says it only reuses the allocation and is reset to default records per edit; a different body (progress tag) starts empty [V] |

**C2 to C4 are the stitch itself.** They are greedy joins whose later decisions read earlier joins, and
there is no join-free reformulation with the same results. What can move into a read phase is the
expensive measurement, kept apart from the cheap decision:
- in T3c, the `drop_point` of the midpoint and the two tangent evaluations are functions of the two curves
  alone. A glue keeps `keep`'s curve, and the deleted edge's pairs disappear. So they can be computed for
  every candidate pair on the body as it stands after T3b, and the sequential loop then decides with the
  current tolerances. This gives equal results only if `drop_point`'s answer does not depend on the
  `drop_tol` it is given (the code passes `this_tol`, or `BIG_DOUBLE` when the smallest gap is asked for):
  UNVERIFIED U4;
- in T3b, the distances within the sweep window are functions of the points, so they can be precomputed
  unless a glue moves the kept point (UNVERIFIED U6).

Totals: P 3, R 0, C 3, S 2.

**Overall: P 6, R 0, C 4, S 12.** Every C item:
- **C1:** heal's seam and loop stage builds lazy trims, and the raises reach the neighbouring faces'
  tests. Removed by the edge-ordered trim prepare; result-changing.
- **C2:** stitch's split-at-vertex scan.
- **C3:** stitch's vertex glue tolerances.
- **C4:** stitch's edge glue tolerances and topology.

C2 to C4 are inherent: they stay sequential, and only their measurements can be hoisted.

## 3. What "prepared" means

| Route | Must exist before a later read writes nothing | Exists today? | Cost to prepare |
|---|---|---|---|
| Record to kernel (in-app) | a UV trim on every face use; edge and vertex tolerances that cover them; the surface cache slots the read needs (stage B lends them) | trims: **no, some missing by design** (section B); tolerances: the record's own, raised to cover a missing trim only when the trim is first built; caches: none | the edge-ordered trim prepare at arrival: one `get_or_create_uv_trim_curve` per trimless use (a fit plus a proved raise each, `trim_raise_distance`); only on faces with `trims == false` |
| Kernel file read | as above, plus the trims deleted by the end check | trims: some missing (failed end checks, and absent in the file); tolerances: covered for the kept trims (`TrimCoverRaises`) | as above |
| Heal (as a read phase of its own) | the trims the seam stage would build (C1), so that CacheFaceprops2 and 3 write nothing | no | the same prepare, run before CacheFaceprops2 |
| Stitch result | trimless faces stay trimless by design, covered by a proved tolerance (T6, T7). Preparing them changes the result (report 07 §3.4 [A]). | n/a | none: do not prepare non-candidate faces |

"Prepared" for the conversion is the same object as report 11's prepared body. The conversion is the
natural place to make it, because it is the one point every in-app body passes. But
`make_uv_trim_curves: false` is chosen on purpose: the conversion is lazy, and `to_record` builds the
trims when it exports. Preparing at arrival moves cost to bodies that are never read (UNVERIFIED U7:
how many trimless uses an app session has and how many get read).

## 4. Caches and the keyless phase table

- **Conversion (I2):** builds geometry. It needs no surface cache, no spatial cache and no gap cache. The
  only shared state is the content-keyed kernel memos (S, one per worker). A phase table is not needed;
  trivially "yes".
- **Kernel file read F3a:** `kept_trim_gap` uses `trim_distance_bound` on `nurb_form` and procedural nets;
  no kept cache. Already a read phase.
- **Heal Cache steps:** they need the gaps (today memos on the body), and the seam stage needs surface
  caches and trims. Gaps as phase values keyed by entity, valid while the body is borrowed shared, would
  do: **yes**, as in the Boolean. The seam stage's surface caches fit the phase table once C1 is prepared.
  The table is dropped at the next Fix step, which is an edit, so it lives one Cache step at a time.
- **Stitch:** the edge spatial tree is already built per iteration and dropped. The other measurements (the
  pair drops, the face bounds in T6) need no kept cache. A keyless table holds only between two edits: for
  the T6 / T7 covers, and for a pair-measurement phase over the post-T3b body. **Across the join loop it
  does not hold**, because every join is an edit.

Answer: the keyless "built in this phase" table holds for read-from-data and for each heal Cache step. For
the stitch it holds only per sub-phase.

## 5. Items, combining, order

| Phase | Items | Typical count, skew | Value | Combining |
|---|---|---|---|---|
| I2 conversion | faces; edges | 10-10^4 per body; skew from label proofs (`labelled`, `BallPatch::create_certified`, `recognised`) and NURBS faces with many pcurves | `(Surface, uv_domain, trims flag, trims)`, `Curve` | placed by index; keys numbered by index; `own_tolerances` after (:617) is a pass over the vertices and edges |
| F3a cover | edge uses | all | `(e, gap)` | already a max per edge, applied in edge order |
| CacheEdgeprops / CacheVertexprops | edges / vertices | all | props plus gaps | histograms and length sum folded in target order (the order matters for the average: fold sequentially) |
| CacheFaceprops2 / 3 | faces | all | flags (seam, sheet, pole, missing / crossed / near-miss seam) | lists in face order |
| stitch T3c measure | edge pairs sharing both ends | few per vertex | `(distance, parameter, orientation)` | the sequential loop consumes them in the original order |
| stitch T6 | joined edges | the number of joins | raises per edge and vertex | max per entity, applied in edge order |

No early exit inside a read phase depends on order, apart from T3's 10-iteration cap and the rescan
bounds, which are sequential anyway.

## 6. What stays sequential

- the topology build (I3) and `adopt`. They are linear and cheap compared with I2 (UNVERIFIED U8);
- every heal Fix step, `update_target_lists` and its recursion, and FixTolsizes' face → edge → vertex
  layers. Inside the vertex layer, `ball_clears` → `vertex_ball_reaches` may read other vertices' balls
  (UNVERIFIED U3);
- the stitch's T1, T2 and the decisions of T3a to T3c, region nesting, and the orientation;
- `put_enclosing_shells_first`.

## 7. Cost

| Step | PRs | Changes results? | Shared with report 07's parts |
|---|---|---|---|
| Conversion: `data()` as a per-edge and per-face map returning values, with keys numbered after | 1-2 | no | kernel per worker (memos) |
| The sampled raise as an argument (section C), or wait for solidcore#3368, which removes the mode | 1 | no | none |
| Optional: the edge-ordered trim prepare at arrival, as a conversion option (report 11's prepare) | 1 | yes (1): lazy raises happen at arrival, in edge order | the prepared view |
| Kernel file read: end check first, deletions after | 1 (can share the cover's PR) | no | none |
| Heal: gaps as values from CacheEdgeprops / CacheVertexprops (shares report 12's gaps-as-values) | 1-2 | no | phase table, kernel per worker |
| Heal: trim prepare before CacheFaceprops2 / 3 | 1 | yes (1) | the prepared view |
| Stitch: `finish_joins` as measure-then-apply | 1 | no expected (U5) | phase table |
| Stitch: hoist T3c's pair measurements | 1-2 | maybe (U4) | phase table |

The routes total 3-5 PRs for conversion and the file read, 2-4 for heal and 2-3 for stitch, so **7-12
overall**. 1 or 2 steps change results (the two trim prepares, which are the same mechanism), maybe 3 with
the stitch's drop tolerance. Typed requests and the fulfil loop: **not needed anywhere** (R = 0).

## 8. Answers A to F

### A. The state a body has on arrival

| Route | UV trims on every use | Tolerances | Gap caches | Surface cache slots | NURB forms of analytic surfaces | Spatial cache |
|---|---|---|---|---|---|---|
| File import (`read_from_data`, check path) | some missing (the end check's deletions, and what the file lacks); the face routine is not run | final for kept trims (covered, `TrimCoverRaises`); raised on first read where a trim is later built; healed data sets them in FixTolsizes and may shrink them | filled by the heal's Cache steps if healed, possibly stale after its fixes; else empty | filled by the heal's seam stage if healed [I]; else empty | `add_analytics = true`; whether a form is built eagerly is UNVERIFIED (U9) | none kept |
| Record to kernel (app, trusted) | **some missing by design** (B) | the record's own (`own_tolerances`, zone raise with `force_same_tol`); raised on first read for trimless uses | empty | empty | present where the record gives a NURBS form (`labelled(k, label, net)`); a label without one goes through `from_parameters` and its form is built lazily [I] | none |
| Kernel operation result | some missing (lazy) [A, 07, 11] | raised by the operation; more on first read | partial; can be stale (#20945, draft) [A] | filled on faces the operation touched [A] | where computed | UNVERIFIED |
| Heal result | as before the heal, plus those the seam stage made (`make_uv = false` everywhere) | set by FixTolsizes, raised by the seam stage's trims | filled for every edge and vertex | filled where the seam stage queried [I] | unchanged | none (fresh queries dropped) |
| Stitch result | trimless on joined faces by design | covered by a proved bound with no trim (T6 / T7); a later trim build raises again | partial | partial | unchanged | none (tree dropped) |

What a read would have to prepare:
- record to kernel, file import and operation results: the missing trims in edge order. The cost is one
  fit plus a proved raise per trimless use; on the record route that is only faces with `trims == false`;
- heal result: the same, but the heal is rare in-app;
- stitch result: nothing. A read that needs a trim on a stitched trimless face should treat it as a
  request (an R item in that read), not prepare it eagerly, because preparing changes the tolerance that
  the cover proved.

### B. Does the record-to-kernel body carry every UV trim? Refuted [V], with a precise exception list

- A face built from a record **without a NURBS form** for an analytic label (cylinder, cone, sphere,
  torus through the `_ =>` arm) or for a `PlacedRevolution` (exact form made by the kernel) gets
  `trims: false` (`surface/face_surface.rs:100, 138`). Its uses get no trim; the kernel builds them from
  the edges on demand (field doc `surface/face_surface.rs:41-56`). App-built records do this: sketch
  profiles and `crates/kernel/src/surfaces.rs` emit `nurbs: None` [V: grep].
- **Bare uses** (`to_kernel_with_bare_uses`, the pcurves route `crates/smlib/src/pcurves.rs`) are left
  bare on purpose and exported at once.
- **Round-trip bodies carry every trim** [V by path]. `to_record` first gives every faced use without a
  trim the kernel's own (`solidcore-convert/src/brep/record.rs:130-136`, the edge-use routine with the
  default proved raise). `record_face` (`surface/face_surface.rs:367-470`) writes cone, sphere and torus
  as label plus NURBS form, a plane as scaled parameters, and a revolution or B-spline as its NURBS. All
  of these read back with `trims: true`. So a record the kernel wrote converts to a body with every trim.
- **Bodies made inside the kernel between records** (operation results) have lazy trims, some missing [A].
- **The stitch's trimless faces** can reach a record (the sweep's joined ends and the fillet's stitch both
  end in an ops body that is later exported). `to_record` then **builds the missing trims on the live
  body**, with the proved raise, before writing: `Body::to_record` documents that the body is edited
  (`solidcore-ops/src/body.rs`, `to_record` / `with_record`). The record therefore always has pcurves, and
  the trimless state and the cover that assumed it do not survive the export. The live kernel body is
  changed by the export: its tolerance can rise.

### C. The sampled-raise mode

- **Where it lives:** `Caches::prove_lazy_trim_raise` (`kernel.rs:149`, default true :156). It is kept by
  `Caches::clear` (:166) and copied into the boolean cache kernel (`boolean/merge/p4/kernel_adapter.rs:65`).
- **Where it is read:** exactly once, in `trim_raise_distance` (`topology/edgeuse/p3.rs:1836`):
  `if !raises || !k.caches.prove_lazy_trim_raise { return Ok(upstream) }` [V].
- **Who sets it:** only `Kernel::with_sampled_trim_raise` (`kernel.rs:1046-1051`), called from
  `crates/smlib/src/core/convert.rs:303` `to_arrays_sampled_trims`. That function is the export of a
  one-face sheet that `crates/smlib/src/pcurves.rs:47` `SmlibImportTrims` builds to get SMLib's pcurves
  for a file's translation. So the mode is set during an **export inside the import**, around
  `flat_from_brep`'s `get_or_create_uv_trim_curve` loop.
- **What differs:** with the mode on, a lazily built trim raises its edge by upstream's sampled distance.
  Without it, the raise is the proved bound `B` from `trim_deviation_bound`: it can be lower than
  upstream's where every other trim and trimless face stays covered, or it is the unproved fallback. Edge
  tolerances in the exported arrays, and hence the record's (`EDGE_TOLERANCE` read back at
  `pcurves.rs:70`), differ.
- **To make it an argument:** a `TrimRaise { Proved, Sampled }` parameter on `trim_raise_distance`, and
  through `get_or_create_uv_trim_curve` / `create_uv_trim_curve`, defaulting to Proved at the roughly 30+
  call sites (UNVERIFIED count). `flat_from_brep` passes Sampled; the kernel adapter copy and the Caches
  field go away. One PR, no result change. The doc already plans to remove the mode entirely
  (solidcore#3368: a fast proved raise); if that lands first, nothing is needed.

### D. Heal and stitch: read-then-edit or interleaved?

- **Heal: interleaved.** It runs Cache, Fix, recursive re-measure. The edge and vertex property passes are
  pure per-entity measurements apart from gap memos on the entity's own uses (WH1, WH2: P) and in-order
  statistics (S). CacheFacepropsGaps is pure. The seam and loop stage is not pure (C1). FixTolsizes is a
  deterministic layered apply.
- **Stitch: interleaved.** Each T3 search is followed at once by the join it found, and later searches
  read the joins (C2 to C4). The pairing measurements can be hoisted (subject to U4 and U6); the joins
  cannot. The covers after the joins (T6, T7) are measure-then-apply (P).
- Classification as section 2: heal P 2 / R 0 / C 1 / S 5; stitch P 3 / R 0 / C 3 / S 2.

### E. Shared state between reads of different bodies, and fresh against used kernels

- **Identity counters:** none shared. `data()`'s key counter and `TopologyEnv::next_key` are per call
  [V]. `check_identities` checks one body.
- **Notify maps:** `Kernel::notify_records` and `Kernel::arena_notify_records` are allocation reuse only,
  reset per edit; the arena map is tied to the body's progress tag (`heal/stitch/p1.rs:2713-2726`) [V]. The
  conversion makes its own `NotifyRecords::new()` (brep.rs :270) [V].
- **Content-keyed memos:** `curve_analysis`, `loop_area_memo`, `recertify_memo`, and the blend station and
  thumbweight memos. Their docs state that a hit returns what a fresh solve returns [V doc], so they do not
  change answers.
- **Mode flags:** `doing_boolean` (restored), the sampled raise (restored; a panic drops the kernel).
- **The context's size estimate:** fixed upstream (merged).

So a conversion or read on a fresh kernel should equal one on a used kernel: [I], not measured. It holds
if no other `Context` field that an earlier operation leaves set is read by these paths (UNVERIFIED U10;
report 11's pool reset rule covers the same question).

### F. Rank and place relative to plan-v5

1. **Read from data (record to kernel): highest payoff, easiest.** Every in-app body's kernel form passes
   through it, and `data()` already takes `&Kernel` and is per-entity independent. It has no C items, and
   its only result-changing step (the prepare at arrival) is optional. **Place: with plan-v5 stage C
   step 2 (reads between operations), as its first item, or alongside step 1** (it is in solidcore-convert
   and changes no kernel read). It is also where report 11's prepared body would be made, if the choice is
   to prepare on arrival.
2. **Stitch: moderate payoff, hardest of the three in its core.** It is reached through the sweep's joined
   ends and the fillet executive. The part worth doing is the covers (T6 / T7) and maybe the T3c pair
   measurements; the joins stay sequential. **Place: inside step 5 (fillet)**, since the fillet executive
   is where it runs often.
3. **Heal: low payoff in the app** (trusted records skip it), some payoff for kernel file reads (corpus,
   goldens), medium difficulty. The Cache steps are easy; C1 is removed by the same prepare; the Fix steps
   stay sequential. **Place: after step 3 (validity check)**, sharing its gaps-as-values work, or leave it
   unstudied further until a corpus count (U1) shows how many reads actually heal.

## 9. Unknowns (UNVERIFIED), with what would settle each

| # | Question | Settle by |
|---|---|---|
| U1 | How many corpus and golden files go through the heal on read (healer version below `HEALER_VERSION`)? | count the healer versions over the perf corpus and the goldens' inputs |
| U2 | Do `vertex_face_gap` or CacheDegenFaces build lazy trims (more C1)? | instrumented heal: trims created per step |
| U3 | Does `vertex_ball_reaches` read other vertices' tolerances (coupling inside FixTolsizes' vertex layer)? | read `topology/brep/validation/pointers.rs` near :1333 (`check_vertex_clearance`), or run with the vertex order reversed and compare bitwise |
| U4 | Does `drop_point`'s answer depend on `drop_tol` (the stitch pair hoist being exact)? | run the stitch goldens with `BIG_DOUBLE` always and compare the pairs found and the tolerances |
| U5 | Is `finish_joins` order-independent (vertex max-raises)? | reverse the edge order of the covers on the `heal.stitch_p1` / `p2` goldens and compare tolerances bitwise |
| U6 | Does a vertex glue move the kept vertex's point? | read `glue_vertices` / `glue_or_squeeze`, or instrument `set_point` during T3b |
| U7 | How many trimless uses an app session's conversions leave, and how many later reads build | counters on `to_kernel` (faces with `trims == false`) and on `get_or_create_uv_trim_curve` per session |
| U8 | Time split of `to_kernel`: `data()` against the build against `adopt` | profile on the 561-part assembly cited in `kernel.rs:1036-1039` |
| U9 | Are analytic surfaces' NURB forms built at arrival (`add_analytics`, `build_faces`)? | read `topology/brep/p10.rs` `build_faces` near :507 |
| U10 | Is a conversion on a used pool kernel bitwise equal to one on a fresh kernel? | convert the corpus records twice, on a fresh kernel and after a run of operations, and compare the records written back |
| U11 | Does T3a's `calculate_bounding_box(k, &mut *b)` with default options write to the body? | flip it to `&Brep` in a scratch copy and run a check build |
| U12 | The number of call sites a `TrimRaise` argument touches | `grep get_or_create_uv_trim_curve\|create_uv_trim_curve` |
