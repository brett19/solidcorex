# Review: report 56, crate granularity

Adversarial read-only review of `docs/research/56-crate-granularity.md`. The report proposes
going from 25 to 32 core crates under the owner's rule (04 §1). I read the report, 01 §1 and
§7, `layers.txt`, the xtask `check_layers`, and the pinned kernel code behind each claim I
sampled. I reran 49's `depmap.py` against the pinned kernel crate's `src/`: 918 files, 38,056
references, 0 unresolved. That matches the report's header. Paths below are relative to that
`src/`. I built nothing.

Evidence tags: MEASURED (a count from the depmap output), READ (code or docs I read), JUDGED
(my reasoning). Severity: **blocking**, **should fix**, **minor**.

## Verdict: accept with the listed changes

The method holds, and most of the counts reproduce. These reproduce exactly (MEASURED):

- the 33 curve-to-surface references (free 19, dispatch 14);
- prism core against projections, 15/2;
- the measures/classification knot, 46/26;
- check into measures 16/1, into classification 4, into mass 2, into SSI 0;
- the five blend knots (120/3, 110/5, 102/10, 98/20, 41/37);
- the hcr users;
- defeature's size and users;
- NLib curves into surfaces, 10.

The split list and the four tests are reasonable. The dependency graph in §5.1 is not yet
acyclic as drawn, though. Two moves create upward edges (B1, B2), and two groups of code have
no home (B3, B4). B5 lists 57 upward references from the geometry areas that the report never
mentions. Fix the blocking items, then make the 01 edits in §9. None of them changes the crate
count much.

## Blocking

### B1. Moving the topology solver to scx-brep creates brep -> bodyq (MEASURED, READ)

- `intersect/topology_solver` refers to classification 12 times. The items are
  `point_3d_classify`, `point_classify_prepared`/`_requested`, `curve_on_classify(_mut)`,
  `RayPreparation`, `PointClassification`, `CurveClassification` and `ClassificationInit`.
- The brep code that uses the solver: `topology/shell.rs` (`is_inner_shell`, `ray_fire`,
  `brep_curve_solve`), brep `p1`, `p2`, `p3`, `p10` and `brep_cache`. `geom/tools.rs` also uses
  `TopologySolveOptions`.
- With the solver in scx-brep, brep -> bodyq, and bodyq already depends on brep, so the two
  form a cycle.
- **Fix.** Keep the solver in scx-bodyq. Move shell nesting and the other brep-side solver reads
  up to scx-bodyq as free functions over `&Brep`. Re-count brep -> bodyq after the move and put
  0 in §5.1.

### B2. The "planar core" reads topology (MEASURED)

- `prism/read.rs` uses `Brep`, `EdgeId`, `start_vertex` and `end_vertex` (4 references).
- `prism/profile.rs` uses `query::tangent_edge::bernstein_roots`.
- §3.4's claim that the planar core has no surface references is true. Even so, scx-planar
  would depend upward on scx-brep and scx-bodyq.
- **Fix.** Move `read.rs` (the walk over a Brep) to scx-make or scx-bodyq. Move
  `bernstein_roots` to scx-algo. State in §3.4 that planar -> brep is 0 after the move.

### B3. The curve records and `eval/` have no home (READ, MEASURED)

- `curve/from_parameters`, `arc_of` and `step_edge` (1,353 lines) are not placed anywhere.
  - `from_parameters` uses `surface::implicit` (`conic_turn`,
    `curve_on_quadrics_with_constants`, `QuadricEquation`) and the `eval` records.
  - `step_edge` uses topology's `drop_limit`.
  - `spline_projection`, which is bound for scx-planar, calls `from_parameters` 5 times. That
    makes planar -> surface.
  - Dispatch uses `ellipse_form_is_exact` from the same file.
- `eval/` holds the plain-data Curve, Surface and Curve2 records. Its users are
  `from_parameters`, the projections, prism, teeth and the STEP crate. 01 put it in `scx-geom`,
  which 56 dissolves, and 56 never re-homes it.
- `curve/from_parameters.rs:77` already has a `pub struct EdgeCurve`, and so does
  `solidcore-ops/measure.rs`. The proposed enum's name clashes with both.
- **Fix.**
  - Put `eval/` in scx-curve. It has 0 references into surfaces (MEASURED).
  - Split `from_parameters` by item. The conic-on-quadric proof goes to scx-surface. The
    parameter-to-curve constructors stay in scx-curve.
  - Move `step_edge` above brep.
  - Rename one of the `EdgeCurve` types, or note the clash in §7.

### B4. `intersect/api_intersectors` is filed as classification but holds three things (READ)

- It contains a surface-surface wrapper that calls `SurfaceIntersector::new`. In bodyq that
  gives bodyq -> ssi, which breaks the report's own rule that the STEP translator does not
  depend on ssi.
- It also contains `intersect_curves`, which `nurbs/gordon.rs` uses. Gordon is bound for
  scx-surface, so this gives surface -> bodyq.
- **Fix.** Split the module by item: curve-curve to scx-curve, the SSI wrapper to scx-ssi, and
  the rest to bodyq.

### B5. The geometry areas have 57 upward references that the report does not address (MEASURED)

These come from `curve/`, `surface/` and `nurbs/b_spline*` into higher areas:

- `procedural_blend` uses `offset/draft` (`DraftDefinition`, `Draft`) and `sweep/section_blend`
  (`SectionBlendDefinition`, `EndOrder`, `Motion`, `SectionFrame`), about 15 references.
- `surface/surface` p2 and `surf_of_extrusion` use point and curve classification types, about
  10 references.
- `fold_certificate` uses `offset/construction/bernstein` (`Bernstein2`, `Binomials`,
  `MAX_DEGREE`).
- curve p5, surface p1 and `speed.rs` use `mass/integrator` (`integrate_*`, `gamma`).
- `crv_on_surf` and `crv_in_volume` use `curve_class`'s `*_discontinuity_intervals`.
- Surface -> geomq/ssi/hlr is 24 references, not 17: ssi 12, geomq 11, hlr 1. These include
  `b_spline_surface_n_lib` -> `query/seam_bound` (4) and `DropSolveContext` (7).
- **Fix.** Add a §3.5 table that gives each group a direction: the definitions move down, or the
  code that uses them moves up. The integrator and the Bernstein helpers belong in scx-algo or
  scx-num. The draft and section-blend definitions belong with the surface kind that stores
  them. Re-derive §5.1 after the table.

## Should fix

### S1. The offset -> blend count is stale and contradicts itself (MEASURED)

- At the pin it is 56, not 62: fillet_section 41, defeature 9, other 6.
- §9's C3 edit says "56 are the certified section", but §4.7 says 41.
- The other 6 are not all in one file. `CircularCrossSectionFsg` is used in
  `offset_geometry_creation.rs`.
- **Fix.** Use 56 = 41 + 9 + 6 in both places and name the files.

### S2. brep -> check is helpers, not only assert hooks (MEASURED)

- By my grouping the 29 references include:
  - `derived_face_shell` and five other `derived_*` helpers;
  - `edge_tube_reaches` and `vertex_ball_reaches`, 4 references;
  - `standing`, `lost_manifold`, `volume_sign` and `FailureClass`;
  - `validate_pointers`.
- brep -> mass (15) includes `face_centroid`, `region_material`, `no_unsure_region` and
  `OdeIntegrator`. It is not just signs.
- **Fix.** §4.4 should say these helpers move down into scx-brep or scx-algo. They cannot be
  dropped.

### S3. Stitch uses boolean and validation (READ)

- `heal/stitch/p1.rs`'s `api_stitch_face` uses `MergeState`, `MergeOperand` and
  `TopologyIntersector`. It is a degenerate entry that always returns an error.
- Stitch also uses `vertex_ball_reaches` and `GROWTH_PAST_RATIO`.
- **Fix.** Delete or move `api_stitch_face`, and move the helpers down (see S2).

### S4. fillet_section's pieces have no stated homes (READ, MEASURED)

- `certificate.rs` (4,594 lines) and `station.rs` (2,074) are mostly blend-section solvers.
  They hold `GnStation`, `GnSystem`, `gn_solve_station_with`, `GnDefs` and `NetScope`, and
  they refer to the kernel's `Surface`. They are not pure NLib.
- The rest of fillet_section (`section`, `definition`, `recertify`, `feet_tm` and others; about
  9,000 lines) is not placed.
- `section.rs` refers to `TsectPnt`, and `teeth` uses `fillet_section::interval::Ivl` (6). The
  claim that teeth "uses only num, NURBS, prism core, eval" is therefore inaccurate. The num
  move fixes the `Ivl` reference.
- Line totals: blend is 174,700 lines without fillet_section and 191,056 with it. The report
  gives 191,020 for the total without fillet_section. I measured the surfaces group at 125,143
  lines, not 120,890.
- **Fix.** Add a placement table for fillet_section, file by file.

### S5. Defeature's `join_with` belongs in scx-curve (READ)

- `join_with` and `join_with_gap` are pure curve operations that match on `CurveKind`. They
  should not go to scx-local.
- defeature -> blend also has `fillet_executive::create_face_from_surface` (1).
- **Fix.** Add both to §4.7.

### S6. Splitting the dispatch costs more than "one forwarding arm" (MEASURED, JUDGED)

- `curve/curve` has about 45 match sites for each moved variant: CrvOnSurf 48, TangentField 46,
  CrvInVolume 42, Projected 43. Every dispatch method splits in two.
- 49 cannot see method calls, so the 14 dispatch references understate the work.
- `CurveEvaluate::as_crv_on_surf` (`curve/curve/p2.rs:335`) names an on-surface type from the
  trait that is meant to live in scx-curve.
- **Fix.** Cost the split in match sites. Remove `as_crv_on_surf` from the trait and make it a
  match on `EdgeCurve`.

### S7. The C10 intersection-curve kind has no home under this graph (READ, JUDGED)

- 01 C10 plans an intersection-curve kind under the shared `Procedural` mechanism. Under 56 it
  would be an `EdgeCurve` arm in scx-surface, but its evaluator (refinement on two surfaces)
  sits in geomq, which is above scx-surface.
- `Procedural` is used by both `OffsetCurve` in scx-curve and the surface kinds, and 56 does not
  say where it lives.
- **Fix.** Put `Procedural` in scx-curve. Either place the intersection-curve evaluator in
  scx-surface, or say that this kind lives in geomq behind an `EdgeCurve` arm that only stores
  data.

### S8. "No dyn" conflicts with the pin and with 01 (READ)

- The pin's `DropSolveContext` holds a `dyn SurfaceSolveContext`.
- 01 C2's fallback is a `FaceCell` trait implemented in bodyq, which is a trait used to break a
  cycle.
- **Fix.** §7 should name these two cases. Say whether each is removed or allowed as an
  exception.

### S9. The case for keeping geomq separate is weak (JUDGED)

- scx-tess gets geomq through brep, and surface code uses projection plumbing itself (B5). The
  "separate adopter" behind test 3 is marginal.
- I agree with keeping geomq separate for now.
- **Fix.** State the trigger for merging it: if, after B5, surface -> geomq is still more than
  0, merge geomq into scx-surface.

### S10. scx-make would gain scx-hlr through `Hcr` (READ)

- The access traits for sweep and primitive have `retain_hcr` and `hcr_session`. That gives
  scx-make an hlr dependency, and through it tess and ssi.
- **Fix.** Move the hcr session to the caller, or put the hcr-retaining path in a crate above
  hlr.

### S11. Rust patterns: `#[non_exhaustive]` and `KINDS` (READ)

- The orphan rule as stated, `impl From<Curve> for EdgeCurve` and the local trait impls are all
  correct.
- `#[non_exhaustive]` forces a `_` arm in every other crate, even inside one workspace. That
  removes the compiler's exhaustiveness check from scx-record's exact encoding, which has no
  generic fallback.
- `KINDS: &[CurveKind]` cannot be a const slice of variants that carry payloads. The test needs
  sample values.
- **Fix.** Leave out `#[non_exhaustive]` until publication (01 §7.4 already defers it, and the
  crates are `publish = false`). Drive the test from sample constructors for each kind, or from
  a fieldless tag enum plus one constructor per tag.

### S12. The layers.txt proposal has holes (READ, JUDGED)

- Rule 1 is circular. It says a crate "may list any crate it reaches transitively", but what it
  reaches comes from its own declarations.
- Nothing ties the file to 01 or §5.3. A new edge that keeps the graph acyclic (for example
  planar -> surface) passes in a one-line PR.
- Rule 2 counts dev edges. The testkit dev-dependency (testkit -> brep, plus brep dev ->
  testkit) is then a cycle. Unit tests inside brep that use testkit would also hit the
  duplicate-crate type mismatch.
- Rule 3 must say "unused" is checked against Cargo.toml.
- The current check has no testkit role (Library, Difftest, Tool only).
- Dropping the module-level check leaves no internal order check inside the 145k-line
  scx-surface.
- **Fix.**
  - Encode the §5.3 subset closures as assertions the check runs.
  - Exclude dev edges into a testkit role, and keep testkit use in integration tests.
  - Keep the module order check for scx-surface and scx-blend.

## Minor

- **M1** (MEASURED). The §3.3 table rows sum to 36, not 33. `OrientType` in p6 is counted twice,
  and uv-trim is 5, not 6.
- **M2** (MEASURED). Surface -> dispatch is 134, not 132, and surface -> free is 118, not 117.
- **M3** (MEASURED). mass -> measures includes `classification_builds_tree` and `LazyBrep`, so
  it is not "all bounding boxes".
- **M4** (READ). The §9 edits to 01 need to carry B3 (the eval row, which now goes to
  scx-curve), S7 (C10) and S8 (C2). As drafted they leave 01's eval row pointing at a crate 56
  removes.

## Summary of findings

| id | severity | one line |
|---|---|---|
| B1 | blocking | topology solver into brep makes brep -> bodyq; keep it in bodyq |
| B2 | blocking | prism/read.rs and bernstein_roots make planar -> brep and -> bodyq |
| B3 | blocking | curve records and eval/ have no home; EdgeCurve name already used |
| B4 | blocking | api_intersectors mixes SSI, curve-curve and classification |
| B5 | blocking | 57 upward references from geometry areas are not addressed |
| S1 | should fix | offset -> blend is 56 = 41 + 9 + 6; §4.7 and §9 disagree |
| S2 | should fix | brep -> check and brep -> mass are helpers that must move down |
| S3 | should fix | stitch uses boolean via api_stitch_face |
| S4 | should fix | fillet_section files have no placement; certificate/station are not NLib |
| S5 | should fix | join_with belongs in scx-curve |
| S6 | should fix | dispatch split is about 45 match sites per variant; as_crv_on_surf |
| S7 | should fix | C10 intersection-curve kind and Procedural have no home |
| S8 | should fix | dyn SurfaceSolveContext and FaceCell contradict "no dyn" |
| S9 | should fix | state the trigger for merging geomq into surface |
| S10 | should fix | Hcr in access traits pulls hlr into scx-make |
| S11 | should fix | omit non_exhaustive until publication; KINDS needs samples |
| S12 | should fix | layers.txt: circular rule 1, no tie to §5.3, dev-edge cycle |
| M1-M4 | minor | tallies, small counts, mass wording, 01 edit coverage |

## Re-review of the revision (sections 9, 11 and appendix B)

The revised 56 has a scripted acyclicity proof (§9), its inputs (appendix B) and a response table
(§11). The proof script is not in the repository, so I rebuilt it from §9.1 and appendix B:

- units cut from the 918 kernel files at the pin;
- appendix B.1 longest-prefix file assignment;
- the B.2 item moves;
- the 9.3 deletions;
- the rising rule;
- the closure check against the Deps column of 5.1.

It runs over 49's `refs.tsv` and `name_uses.tsv` at the pin, as above. Paths are relative to
the kernel crate's `src/`.

### R0. The rerun reproduces the stated result (MEASURED)

**Files.** Every file has a crate. Not-ported files come to 39,866 lines by my line count, against
39,516 in §9.2.

**Moved code.** My rerun converges in 4 rounds, as §9 says, and the large flows match:

| Flow | Revision | My rerun |
|---|---|---|
| brep → bodyq | 224 units, 11,166 lines | 219 units, 12,297 lines (my units include trait impls) |
| curve → surface | 44, 4,246 | 42, 4,194 |
| brep → record | 65, 2,221 | 65, 2,221 |
| surface → record | 45, 1,743 | 45, 1,743 |
| volume → surface | 18, 1,393 | 18, 1,393 |
| surface → geomq | 16, 1,381 | 16, 1,381 |
| brep → check | 9, 271 | 9, 271 |

**References outside the graph.** I find 13, not 0. All of them sit in items the rising rule
cannot move, because it moves only functions and impls:

- the `ShellRayWorld` trait in `topology/shell.rs` names `TopologySolver`, `RayPreparation` and
  `TopologyObjects`;
- `IntersectorVirtuals::evaluate_law_point` (`intersect/surface_intersector/p4.rs:341`), which
  stays in ssi, takes `&mut FilletLaw` from blend;
- `VertexSector` (`topology/face/p4.rs:875`), which stays in brep, has a field of bodyq's type
  `ClassifyProblem`.

Each of these gives brep → bodyq or ssi → blend, so each is a cycle. Some of my 13 may be
differences in how my parser cuts units. These three are real (READ).

**Fix.** Move `ShellRayWorld` and `VertexSector` to bodyq. Take `evaluate_law_point` off the ssi
trait.

### R1. The proof counts only path references, so callers were left behind. **Blocking** (MEASURED, READ)

49's data records named-item paths and imported names. It does not record two kinds of call:

- calls between functions in the same module, by bare name;
- method calls.

The rising rule therefore lifts an inner function and leaves its wrapper behind. I scanned every
unit for calls, by bare name or method syntax, to the 372 risen functions whose names are unique
in the kernel. Calls to those that land outside the caller's closure: 208 distinct caller-callee pairs,
across 44 crate pairs. That is a lower bound, because the other 253 of the 625 risen functions were
not scanned. The largest
pairs:

- **brep → bodyq, 46 calls.** Examples:
  - `transform_with_notifications` calls `transform_in` (`topology/brep/p5.rs:3722`);
  - `mirror_with_notifications` calls `mirror_in` (`topology/brep/mirror.rs:104`);
  - `stitch_faces` calls `stitch_faces_with_identity`;
  - `remove_topological_joins` calls `remove_topological_in`;
  - `orient_body` calls `orient_ray_votes`;
  - `ensure_trim_curve` calls `prepared_trim` (`topology/loopuse.rs:167`);
  - `shell_is_degenerate` calls `shell_bounds_for_degeneracy`.
- **surface → geomq, 16 calls.** For example, the `global_line_intersect_*` family in
  `surface/surface/p8.rs` calls `base_global_line_intersect_from`.
- **num → algo, 13 calls.** `Transform::mirror_simple` calls `concat_transform`, which rose only
  because it calls `mult_4x4` (`geom/geom_utility/p2.rs:1043`). `pseudo_box` calls `set_basis`.
- **nurbs → nurbs-surface, 13 calls.** `extend_by_dist_at_depth` calls `remove_knot` and
  `bad_weight`.
- Also: curve → surface 11, surface → blend 10, surface → bodyq 8, brep → check 5,
  brep → stitch 4, surface → ssi 4.

Each of these is an edge outside the declared graph. Most of them close a cycle. So the claims
"0 references outside the declared graph" and "no cycles" do not hold at the pin.

I also reran the rising rule with these call edges added. It does not converge on a sensible
graph: it drags most of the kernel into scx-offset. Part of that is false matches, but it shows
that rising alone is the wrong tool for these pairs. The fix is to move the shared helpers down
(R2).

**Fix.**
1. Add call edges to the proof. A reference-only proof is not one. Count same-module bare calls
   with 49's tokenizer, and method calls by name, with a stop list or with rust-analyzer.
2. Rerun after R2 to R5.
3. Commit the script beside 49's scripts, so appendix B can be checked without rebuilding it.

### R2. B1's 224 "classification reads" are mostly edits, and moved for the wrong reason. **Blocking** (MEASURED, READ)

- My rerun moves 167 brep functions and methods to bodyq. 105 of them (8,790 of 11,086 lines)
  take a `&mut` Brep, self or model. Examples: `transform_in`, `mirror_in`, `copy_brep`,
  `set_tolerance`, `brep_set_tolerance`, `stitch_faces_with_identity`,
  `stitch_and_orient_*`, `remove_topological_in`, `merge_curves_on_surface`,
  `create_faces_with_curves`, `split_closed`, `nest_shells`.
- These are brep's edits, not reads. §4.3 justifies the move as "free functions over `&Brep`".
  That justification does not cover them.
- Many rose only because they call body, face or edge bounding boxes. Those boxes are
  `calculate_bounding_box` and `calculate_tight_bounding_box` in `query/brep.rs`,
  `query/face/p1.rs`, `query/face/p3.rs` and `query/edge.rs`. B.1 assigns them to bodyq, but they
  need only brep, curve and num, plus `LazyBrep` (MEASURED).
  - `transform_in`'s only bodyq reference is `calculate_bounding_box`.
- Others rose through `QueryTopology`. That is the prepared and requested trim access in
  `topology/loopuse.rs`, a cache wrapper that B.2 moves to bodyq.
- The revision contradicts itself here. §10's own C2 line says "the cells take the bounding
  boxes". Cells live in scx-brep, so under this assignment the cells cannot be filled without
  brep → bodyq.

**Fix.**
1. Move the box functions and `LazyBrep` down to scx-brep.
2. Keep `QueryTopology`, with the trim access, in scx-brep. Move only the ray-preparation variant
   up.
3. Move `mult_4x4` to num.
4. Rerun. What then rises to bodyq should be shell nesting, ray firing and point-in-face, and
   much smaller.

### R3. Planar still reaches bodyq, and DXF with it. **Blocking** (MEASURED, READ)

- `recognized_path` (`prism/arc_conversion.rs:256`) rises to bodyq only because it calls
  `loop_circle` and `loop_ellipse`. Those sit in `prism/recognition.rs`, which B.1 assigns to
  bodyq, but they use only `Loop2`, `Seg2` and `Vec2`.
- §3.4 calls `recognized_path` a body read. It is not one: it takes a `PlanarPath`.
- Its caller `path_shapes` stays in planar and calls it by bare name. The DXF export in the ops
  crate (`dxf/export.rs:102`) calls `path_shapes`.
- So the claim "planar now depends only on curve" fails, and scx-dxf would need bodyq.
- The other three body reads (`edge_run`, `planar_edges`, `planar_edges_tolerance`) are real.

**Fix.** Move `loop_circle` and `loop_ellipse` to planar, beside `conic_ellipse`. Leave
`recognized_path` in planar.

### R4. make → hlr is 0 only because the sweep's silhouette step was deleted. **Should fix** (READ)

- The solid sweep calls HCR itself: `sweep/topology_sweep/engine.rs:1218` runs `a.hcr(...)` on
  the original body.
- `access.rs:474-519` builds the `Hcr`, computes visibility, and reads `silhouette_edges` back to
  shape the sweep.
- The store also has a `Body::Hcr` variant.
- No outside caller wants these hidden curves; the operation does. So "the caller keeps the
  session" does not apply.
- scx-hlr now needs only bodyq (layer 7), so make → hlr is acyclic.

**Fix.** Either declare make → hlr, or move the silhouette-edge query the sweep needs into bodyq.
State which one, and drop the 9.3 row.

### R5. The api_intersectors three-way split holds (MEASURED)

- My rerun puts `intersect_curves`, `curve_crossings` and `CurveIntersections` in curve.
- It puts `intersect_surfaces`, `intersect_surfaces_within` and
  `refuse_points_where_a_plane_crosses` in ssi.
- The other 41 items go to bodyq.
- I found no left-behind caller in the file.

### Rulings on the four disputes

**B3: the author is right on scope and wrong on mechanism** (READ, MEASURED).

- step_edge:
  - `closed_parameter` does not read the edge's vertices. Its only upward reference is
    `drop_limit` (`topology/brep_constructor.rs:324`).
  - `drop_limit` is a pure curve function: it drops a point onto a `Curve` and snaps it to an end.
  - `closed` calls `closed_parameter` three times by bare name (`curve/step_edge.rs:324-342`).
    Moving `closed_parameter` up alone would leave an upward call behind.
- from_parameters:
  - `on_conic` moves up, but its callers stay: `conic_on_form`, `record_of`, `conic` and
    `ellipse_form_is_exact`.
  - `ellipse_form_is_exact` is called from `curve/curve/projected_pair.rs:70`.
  - What `on_conic` needs (`QuadricEquation`, `curve_on_quadrics_with_constants`,
    `spine_on_offsets`, in `surface/implicit.rs`) uses only `BSplineCurve` and `gamma`.
  - My own first-round fix sent these up. That was wrong too.
- **Ruling.** Move `drop_limit` and those three items down to scx-curve, beside `conic_turn`.
  Then nothing leaves step_edge or from_parameters.

**S2: the author is right for the assert hooks and wrong for edge raising and transform** (MEASURED).

- `raise_moved_edge` and `raise_joined_edge` rose to check only because they call
  `vertex_ball_reaches`. Brep's `cover_moved_edge` and `cover_joined_edge_trims` still call them
  (`topology/edgeuse/p3.rs:2349, 2945`).
- `transform_matrix_with_notifications` calls `degenerate_low_work_transform`
  (`topology/brep/mirror.rs:449`).
- These are brep's own edits. Taking them up drags the edit API into scx-check.
- **Ruling.** Move `vertex_ball_reaches` down to scx-brep with `edge_tube_reaches`.
  `is_valid_and_outward`, `validate_pointers` and the `local_operation` self-check may go up to
  their operations.

**S9: the author is right on the trigger and wrong that the count is 0** (MEASURED, JUDGED).

- A trigger based on the kind's contract is better than mine. Mine was a count, and the rising
  rule can drive any count to 0.
- But surface → geomq is not 0. Sixteen calls stay: `global_line_intersect_*` in
  `surface/surface/p8.rs`, `drop_curve_owned_by*` in `surface/plane/p1.rs`,
  `create_corner_blend_*` and `coons_side_*` in `b_spline_surface_n_lib`, and
  `clip_exact_curves_with_context` in `exact_pieces.rs`.
- **Ruling.** Each of those families must move whole. Then apply the author's trigger. Line
  intersection on a surface is a query, so it does not trigger a merge.

**B5: the author is right** (MEASURED).

- My 57 was an incomplete list.
- At the revised B.1 file assignment, before B.2 and rising, I count about 200 references from
  the geometry crates into non-geometry crates, excluding record. That is 463 counting
  references between geometry crates. 162 under the draft's assignment is plausible.

### Mass in bodyq (JUDGED, MEASURED)

**Keeping mass merged is acceptable now. The stated reason is not the real one.**

- Mass's references into bodyq at the pin are:
  - `QueryTopology` 31;
  - body boxes and their options 10 (`calculate_bounding_box`, `BoundingBoxOptions`);
  - `classification_builds_tree` 1 and `LazyBrep` 1.
- None are point or curve classification calls. `require_classification_trees`
  (`mass/integrator/properties.rs:2757`) is a guard on a cache's state, not a use of
  classification.
- Under test 1 the cut fails today, so merging is what the rule says.
- But after R2 moves the boxes and `QueryTopology` down, mass may reach only brep. The adopter
  "mass of saved bodies" would then avoid about 29,000 lines of classification and pass test 3.

**Fix.** Make the revisit trigger "rerun after R2", not "after C2's cells".

### Conflicts in the rewritten §10 with 01 and 04 (READ)

- **Boxes.** C2's "the cells take the bounding boxes" conflicts with B.1, which puts the boxes in
  bodyq (R2).
- **`FaceCell`.** "The `FaceCell` trait fallback is dropped" is fine for the fallback (01 C2,
  `FaceCell` implemented in bodyq). But 01 §3.2 also defines `pub trait FaceCell` as the cell
  read-set contract, in scx-brep. §10 should say that only the bodyq implementation fallback
  goes, not the trait.
- **Honest signatures.** §4.3 cites the cache work's goal of honest `&Brep` reads and `&mut Brep`
  writes. Moving `&mut Brep` edits into the read crate (R2) works against that goal.
- **Hcr row.** The §1.3 row for `blend` says `hcr` goes to scx-hlr. That is fine, but the sweep
  still needs it (R4).
- I found no conflict with the 04 rows on tolerance newtypes or `#[non_exhaustive]`.

### Verdict on the revision: not accepted yet

The structure, the split list and the response to most findings are right. My rerun reproduces
the proof's result. But the proof sees only path references, and the graph fails once
same-module and method calls are counted.

Must fix:

1. **R1.** Add call edges to the proof and commit the script.
2. **R2.** Move the boxes, `LazyBrep` and `QueryTopology` down to brep, then rerun. Brep's edits
   stay in brep.
3. **R3.** Move `loop_circle` and `loop_ellipse` to planar.
4. **R0.** Fix the three non-function items.
5. **B3 and S2.** Apply the downward moves in the rulings: `drop_limit`, the quadric test and
   `vertex_ball_reaches`.
6. **R4.** Declare make → hlr, or move the silhouette query into bodyq.

After that, rerun the proof with call edges. If it shows 0 outside the graph and no cycles,
accept.

## Re-review 2 (third revision: SCIP proof, 30 crates)

### Verdict: accept the geometry layers and the method; change the body layer before building

**Accept.** Accept the method, the geometry layers (layers 0 to 5) and the operation crates above
`scx-stitch`.

**Change.** The body layer must change before we build to it: `scx-brep`, `scx-record`, mass,
`scx-stitch` and `scx-check`.

**Why.** The proof is now complete for the pin's call graph. But for the body layer it proves the
pin's design acyclic, not v2's:

- the four heaviest edges it forces (brep → ssi, record → stitch, mass inside bodyq, and the
  25,327-line classification sink into brep) come from solidcore designs that 01 already
  replaces;
- so does curve → nurbs-surface.

The geometry layers get that treatment already. Section 9.3 removes edges for v2 designs
(`AnyCurve`, O1, K1, P1). The body layer never gets the same treatment.

Below, each forcing edge is ruled intrinsic or incidental, with the v2 design that removes each
incidental one. The section ends with the graph to build to and an item-by-item list for a direct
ruling. No further revision round is needed: what remains is a decision, and
`cargo xtask layers` checks the result at port time.

**What I could and could not check (READ).**

- I did not rerun `56-data/prove.py`. The SCIP index is not in the repository, and making it
  would mean running rust-analyzer's build of the pin. That is outside this review's no-build
  rule. So the proof's counts below are the author's, not mine.
- I did read `prove.py` and `assignment.py`. The method matches §9.1. The removals in
  `assignment.REMOVE` are each tied to a named design change. The only edges dropped wholesale
  are those to files that are not ported (errors, caches, `Kernel`, the object model). Those
  match v2's replacements (01 §5, §3, §6.1, §2.6).
- The R0 to R5 fixes and the four rulings are applied as written (11.1).
- I accept the proof as a measurement of the pin.

### E1. brep → ssi: incidental (READ)

The five item edges sit in four operations that live in brep files. They are there only because
solidcore's `SmBrep` and `SmFace` classes carry operations as methods:

| Pin item | What it is | v2 home |
|---|---|---|
| `topology/brep/planar_section.rs` `surface_planar_section_curves` | a surface cut by a plane. It takes `&Surface`, an `Extent3d` and a plane, and no body. Used by `cutting.rs` (`SmApiCut`) and by blend, primitive and sweep code | a surface-plane query in `scx-ssi`. The cut itself is "split by surface or sheet", which 01 §1.1 gives to `scx-boolean` |
| `topology/brep/p1.rs` `brep_intersect_mut_with_partial`, `brep_intersect_self_with_partial` | body-body intersection | `scx-boolean`, which has "sections" and "trims of sheets by sheets" (01 §1.1) |
| `topology/face/p3.rs` `face_intersect_breps_cached` | face-face intersection. Its callers are the merge (`boolean/merge/p4/arena_pair.rs`), the blend executives and `api_intersectors` | `scx-boolean`'s topology intersector (01 §1.1) |
| the tangency witness (`intersect/curve_class/witness.rs`) | calls `global_surface_intersect` at `:694`. It is active only when `face_intersect_breps_cached` sets a tangency partner (`curve_class/p1.rs:626`) | goes with face-face intersection, to `scx-boolean` |
| `topology/brep/p1.rs:3738` `create_parallel_projection_curves` | projecting a curve onto a body. Its callers are `sweep/directed.rs` and `topology/api_projection.rs` | a read over `&Brep`: `scx-bodyq`, or `scx-hlr`'s "projected outlines" (01 §1.1) |

With these moved, `scx-brep` needs no intersection of two surfaces. Under 01 C2 it needs
`scx-surface` only, for the cells' boxes and boundary trees (01 §3.1). The body data model subset
loses `scx-ssi`.

### E2. record → stitch, and mass "because the reader heals": incidental (READ)

**What the pin does.** `topology/brep/p10.rs:528-530` calls `heal_made` only when
`options.healer_enabled`. It does so only for data written by an older healer
(`data.healer_version() < HEALER_VERSION`), or when `always_heal` is set. That is migration of
solidcore-era files.

**What v2 says.**

- `scx-record` is "the exact stored form ... versioned, keeping ids" and depends on layers 0-5
  only (01 §1.1, §2.2).
- 01 §11.1 names "stored form not a fixed point" as a fault the design avoids.
- Pin records reach `Brep` through `scx-difftest`'s converter (01 §1.2). Healing a body is an
  explicit call to `scx-stitch` or `scx-repair`.
- A file written by scx-record is exact, so its reader has nothing to heal.

**Ruling.**

- `scx-record` depends on `scx-brep`.
- If the app must open saved solidcore designs, that is a converter crate above `scx-stitch`, not
  the reader.
- This meets 56's own trigger (4.4, 9.3). So mass splits out: `scx-mass` on `scx-brep`.
  - Test 1 already passes (mass reaches only brep, 4.4).
  - Test 3 now passes too. The "mass of saved bodies" adopter takes num to brep, plus record and
    mass. It avoids `scx-bodyq`, `scx-check` and `scx-stitch`.
  - The 78 mass items that brep's edits call (4.4) stay in brep only if an edit that v2 keeps in
    brep still calls them. See E4.
- `require_classification_trees` is a guard on a cache's state. It goes with C2's cells.

### E3. curve → nurbs-surface: incidental (READ)

The three causes in 5.3:

- **`IZ`** (`geom/nurbs_enclosure.rs:55`) is `pub(crate) const IZ: Ivl = Ivl::new(0.0, 0.0)`, the
  zero interval. It moves to `scx-num` with `Ivl`. This is a test 1 move of one constant.
- **IGES 126** is the rational B-spline curve entity. It goes to `scx-nurbs` with curves, and 128
  goes to `scx-nurbs-surface`. 01 §1.1 already puts the conversion with NLib. `nurbs/iges.rs`
  splits by entity (`iges_126_curve`, `clamp_curve` and `correct_curve_knots` against the
  surface trio). The shared `clamp_dir` and `correct_dir` go down.
- **`eval/`** is the pin's second evaluator, "for geometry given by value ... as applications
  built on solidcore hold it" (`eval.rs:1-7`). It exists because the pin's kernel kinds are not
  plain data. In v2 they are: `Curve` and `Surface` are owned enums with no links back (01 §7.1).
  Each kind evaluates itself, generic over the scalar (01 §1.1, C10). So `eval/` has no v2
  counterpart as one unit:
  - its curve half goes into `scx-curve`'s kinds;
  - its surface half (`spline`, `collapse`, `uv`, `surface`, `sphere`, `pole`, `ruling`) goes
    into `scx-surface`'s kinds. That is upward, which is why the author's trial of moving it
    down could not clear the edge.

**Ruling.** `scx-curve` depends on `scx-nurbs` and `scx-algo`, not on `scx-nurbs-surface`. The
curves subset loses NURBS surfaces (5.3).

### E4. The 25,327-line classification sink into brep, `ShellRayWorld`, `VertexSector`: incidental (READ)

"Edits fire rays" is true at the pin. In v2 the edits that fire them are not in `scx-brep`:

| Pin edit that classifies | Calls | v2 |
|---|---|---|
| `find_infinite_region` (`topology/brep/p9.rs:167`) | `ray_fire` 4 times | regions are stored: `RegionId(0)` is the infinite region (01 §2.1). Nothing to find |
| `nest_closed_shells_as_cavities` (`p6.rs:2382`), `fire_at_inner_shell` | ray firing | which region a shell bounds is decided by the operation that makes it: stitch, boolean, make. Those sit above `scx-bodyq` and call its classification |
| `make_edge_in_face_with_vu_with_curve_identity` (`p9.rs:1518`) | `find_vertex_sector` | Euler operations that infer structure from geometry. v2's `Draft` takes explicit structure (`add_edge`, `add_loop`) and `finish` checks only structure (01 §2.3, §7.2). The sector choice belongs to the caller: imprint in `scx-boolean` |
| `transform_in` (`p5.rs:3730`) | `calculate_bounding_box` | `transform` rewrites geometry and "cells refill lazily" (01 §2.4). No box is computed |
| `mirror_in` | `find_and_set_infinite_region` | regions are stored, and a mirror flips face sides (01 §2.1). No search |
| `orient_body`, stitch entry points | ray votes | orientation is in `scx-stitch` (01 §1.1) |
| `QueryTopology` prepared and requested trims | builds trims on read | pcurves are stored on the `EdgeUse`. Operations make the trims of what they create (01 §2.1, §11.1). The machinery goes |

**What of the sink is intrinsic to brep.** The boxes, the uv boxes and the point-in-face boundary
trees. These are cells (01 §3.1), filled under the `FaceCell` read set (01 §3.2). Everything else
in the sink is classification proper, and it belongs in `scx-bodyq`:

- `TopologySolver`;
- ray firing;
- curve classification;
- `ShellRayWorld`;
- `VertexSector`;
- `ClassifyProblem`.

**Soundness of the sink.**

- It is sound as a proof step for the pin: it clears the cycle without rewriting.
- It is unsound as a design. It puts most of those 25,327 lines, which are reads, into the data
  model. Only the box and tree fills are not reads.
- It contradicts 01 §1.1. 01 §1.1 says `scx-brep` is a model "that can be built, walked, edited
  and moved", and that `scx-bodyq` holds "everything that reads a finished body".
- It grows brep to 118,000 lines, and every body user pays for that.

**Ruling.** Reverse the sink. Classification goes to `scx-bodyq`. The classifying edits go to the
operation crates above it, as in the table.

### E5. The 21 orphan-rule impls: they show the wrong cut, so do not fix them at port time (READ)

- 17 of the 21 are in `scx-bodyq`, and 3 more are classification impls in boolean and brep (56 §8).
  - `Mutable` and `MutableParams` are in `curve_class`.
  - `GlobalSolve` is for `TopologySolver`.
  - `ShellRayWorld` is for `BrepQuery`.
  - `SurfaceSolveContext` and `FaceQueries` are for the live ray contexts.
  - `CacheFace` is for `BrepCacheFace`.
- Two things follow from that list:
  - They are classification's internal seams, split across brep and bodyq by the E4 sink.
  - They are the pin's cache contexts, which v2 replaces: cells and `Scratch` (01 §3.1, §3.3).
    `topology/brep_cache` itself is not ported (9.2).
- A wrapper type per impl would carry the pin's context layer into scx just to satisfy the
  orphan rule.
- With E4 reversed, classification is whole in one crate. Most of these impls fall inside it or
  vanish with the contexts.
- The remaining one (`From` for `Curve` in offset) is a one-line port fix.

**Ruling.** The cut is wrong, not the port. Recount after E4.

### E6. make → hlr: intrinsic, keep (READ)

`sweep/topology_sweep/engine.rs:1218` builds a silhouette-mode scene of the tool body along the
sweep's inverse placement. `access.rs:474-519` computes visibility and reads
`silhouette_edges`. The boundary of a swept solid is generated by the tool's silhouette with
respect to the motion. That is the algorithm, not a solidcore layout choice.

- `scx-hlr` is about 11,000 lines on `scx-bodyq`.
- `scx-make` already takes `scx-boolean`, so the edge adds little weight.

**Ruling.** Accept as 4.5 states.

Whether the sweep needs occlusion, or only the silhouette split, is a port question. If it needs
only the split, the silhouette split may sink to `scx-bodyq` later. That does not change the
graph.

### E7. The surface and geomq merge (S9): intrinsic, accept (READ, JUDGED)

**Point inversion.** `Surface::local_point_solve` and `global_point_solve_in` go to
`surface_point_solve`. Every procedural kind needs inversion for its own contract. Examples:

- a sweep or offset over a procedural base;
- the procedural blend whose certified section 4.7 moves into `scx-surface`;
- point refinement on two surfaces for intersection curves.

That coupling is intrinsic.

**Projection on the surface types.** The `drop_curve` dispatch on every kind is the pin's
methods-on-classes layout, and so incidental. In v2 the projection's dispatch would be a `match`
in the query module (7.2). But moving it would not save the merge, because the inversion coupling
remains.

**The adopter.** The lost adopter (a surface designer who never projects) is weak, as 4.3 says.

**Ruling.** Accept the merge. Projection is a module of `scx-surface`, entered through its own
functions, not through `Surface` methods. `scx-surface` at about 195,000 lines is the cost. C1's
build-time trigger stays the revisit.

### E8. Smaller edges the same lens changes (READ)

- **stitch → check.**
  - `extended_side` (`topology/brep/side_extend.rs:355`) stitches, then runs the full check. It
    is "extend", an `scx-local` operation (01 §1.1), so it sits above both.
  - `end_shell`'s `validate_pointers` is a structure check, which is `Draft::finish` (01 §2.3).
  - `radial_sweep`'s `is_outward_solid` is orientation, a classification read in `scx-bodyq`.
  - **Ruling.** `scx-stitch` depends on `scx-bodyq`, not on `scx-check`. `scx-check` and
    `scx-stitch` become siblings. `scx-step` takes both.
- **ssi ← check, 18 items.** The intersecting geometry checks (`curve_bounded_surface_assert_valid`)
  are validity checks. They belong in `scx-check` on `scx-ssi`, not in ssi (01 §2.3: geometric
  validity is the check's).

### The graph to build to (JUDGED; the line counts move and are not measured)

| Layer | Crate | Deps | Change from 56 §5.1 |
|---:|---|---|---|
| 0 | `scx-num` | std | adds `IZ` |
| 1 | `scx-algo` | num | |
| 1 | `scx-nurbs` | num | adds IGES 126 |
| 1 | `scx-p21` | num | |
| 2 | `scx-nurbs-surface` | nurbs, algo | IGES 128 only |
| 2 | `scx-curve` | nurbs, algo | drops nurbs-surface. `eval/`'s curve half only (E3) |
| 3 | `scx-surface` | curve, nurbs-surface | adds `eval/`'s surface half (E3). Geomq merged (E7) |
| 3 | `scx-planar` | curve | |
| 4 | `scx-sketch`, `scx-teeth`, `scx-dxf` | planar | |
| 4 | `scx-ssi` | surface | adds the surface-plane section (E1). Loses the 18 check items (E8) |
| 4 | `scx-brep` | surface | drops ssi. Data model, cells (boxes, boundary trees), structure checks, `transform`. No classification (E1, E4) |
| 5 | `scx-tess` | brep | |
| 5 | `scx-record` | brep | was on stitch (E2) |
| 5 | `scx-mass` | brep | new: mass leaves bodyq (E2) |
| 5 | `scx-bodyq` | brep, planar | takes back classification, the topology solver and ray firing (E4) |
| 6 | `scx-check` | bodyq, ssi | sibling of stitch (E8) |
| 6 | `scx-stitch` | bodyq | was on check. Takes orientation and nesting (E4, E8) |
| 6 | `scx-hlr` | bodyq | |
| 7 | `scx-boolean` | bodyq, ssi, stitch | takes cut, body and face intersection, the tangency witness, imprint's sector choice (E1, E4) |
| 7 | `scx-step` | p21, stitch, check | |
| 7 | `scx-deform` | stitch, check | |
| 8 | `scx-make` | boolean, stitch, check, planar, hlr | E6 kept |
| 8 | `scx-repair` | boolean, stitch, check | |
| 9 | `scx-local` | make | takes `extended_side` (E8) |
| 9 | `scx-mesh` | make, tess | |
| 9 | `scx-sheetmetal` | make | |
| 9 | `scx-taper` | make | |
| 10 | `scx-blend` | local, repair | |
| 11 | `scx-offset` | blend, taper | |

That is 31 core crates. The subsets change as follows (5.3):

- the curves take num, algo, nurbs and curve;
- the body data model takes the curves, plus nurbs-surface, surface and brep, with no SSI;
- loading takes the body data model, plus record;
- mass of saved bodies takes loading, plus mass;
- a viewer takes loading, plus tess.

Every edge in the table is acyclic by layer. `cargo xtask layers` checks it as each crate is
ported.

### Item-by-item ruling

| Item | Rule |
|---|---|
| §2 rule, four tests | accept |
| §9 method, SCIP graph, `56-data/` | accept as the measurement of the pin. Keep it for port-time checks |
| §9.3 removals (`AnyCurve`, O1, K1, N1, P1, T1, C3) | accept |
| C1 reversed to (a) with `AnyCurve` | accept |
| `scx-nurbs` / `scx-nurbs-surface` split | accept, with IGES split by entity (E3) |
| curve → nurbs-surface | change: drop it (E3) |
| volume maps merged into `scx-surface` | accept |
| geomq merged into `scx-surface` (S9) | accept (E7) |
| `scx-ssi` | accept, plus the surface-plane section, minus the check items (E1, E8) |
| brep → ssi | change: drop it. Move the four operations up (E1) |
| the 25,327-line bodyq → brep sink | change: reverse it. Brep keeps only the cell fills (E4) |
| `ShellRayWorld`, `VertexSector`, `ClassifyProblem`, `TopologySolver` in brep | change: to `scx-bodyq` (E4) |
| the 21 orphan impls as a port fix | change: recount after E4. Do not wrap the pin's contexts (E5) |
| record → stitch | change: record on brep. Healing is the caller's or the converter's (E2) |
| mass merged in bodyq | change: split `scx-mass` on brep (E2) |
| stitch → check | change: stitch on bodyq, check sibling (E8) |
| `scx-check` split from bodyq | accept |
| make → hlr | accept (E6) |
| blend → local, defeature in `scx-local` | accept |
| api_intersectors three-way split | accept |
| `scx-p21`, `scx-teeth` in core, `scx-dxf` on planar | accept |
| `layers.txt` as a design graph with subset assertions (§6) | accept, with the graph above |

### Summary of Re-review 2

| Edge | Class | v2 design that removes it (01) | Ruling |
|---|---|---|---|
| brep → ssi | incidental | §1.1 (`scx-boolean` sections, split, intersector); §2.3 | drop |
| record → stitch | incidental | §1.1 and §2.2 exact stored form; §1.2 difftest converter; §11.1 | drop: record on brep |
| mass in bodyq (reader heals) | incidental | same as record | split `scx-mass` |
| curve → nurbs-surface | incidental | §7.1 plain-data kinds evaluate themselves; §1.1 IGES with NLib | drop |
| classification sink, `ShellRayWorld`, `VertexSector` in brep | incidental | §2.1 stored regions; §2.3, §7.2 explicit `Draft`; §2.4 lazy cells; §3.1 cells; §11.1 stored trims | reverse |
| stitch → check | mostly incidental | §2.3 structure at `finish`; §1.1 extend in `scx-local` | stitch on bodyq |
| 21 orphan impls | incidental (cache contexts) | §3.1, §3.3 cells and `Scratch` | recount after the reversal |
| make → hlr | intrinsic | none needed | keep |
| surface ← geomq (S9) | intrinsic (inversion) | none; the projection's dispatch is incidental, but moving it does not change the result | keep merged |
