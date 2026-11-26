# 46. Who depends on the tree build's tolerance raise?

Scratch branch `cache-tanfield` (local, plus the server ref `push/cache-tanfield`; never pushed
upstream). Base: `cache-tree-no-gapfix` at 731078cbe5, which makes `BrepCacheFace::create_uv_trim_curves`
call `create_missing_face_uv_trim_curves` (loop 1 only). Scratch switches, all read from the
environment:

- `ZZ_FULL=1`: the tree build runs the whole `create_uv_trim_curves` again (main's behaviour).
  Unset: the branch.
- `ZZ_SKIPTF=1`: the prototype fix (part B). Two gap measurements skip an edge whose curve is a
  tangent field.
- `ZZ_TF_LOG` / `ZZ_BT`: log each corner-step change made inside a tree build (`ZZR` lines),
  optionally with a backtrace. `ZZ_VT`: log every vertex tolerance set above 1. `ZZ_ST`: stitch
  lamina detail. `ZZ_CHK`: a check at L2 in the `topology.brep_p10` runner.

Builds and runs: the build server, `--profile goldens`, Scripts are in
`scratch46/`; logs are in scratch space on the build server and `tf-t*.log`.

Labels: MEASURED (run here), READ (from code or docs), INFERRED (reasoned, not run).

## Answer

**The 53 is not a gap. The fillet does not need it.**

- **What the 53 is.** The tree build measures the gap from each vertex to the start of its edge's
  curve. When that curve is a tangent field, the "point" it evaluates is a unit-length vector, so
  the gap comes out as the vertex's distance from the origin, about 26.6. The raise then sets the
  vertex tolerance to twice that: 53.15 at (0,17,20) and 51.74 at (0,20,17). The UV gap at both
  corners was already 0. (MEASURED)
- **Why the fillet "needs" it.** The 53 does nothing for the geometry. The oversized balls make
  the stitch merge vertex V24 away, so the result becomes an open body that the fillet finish
  accepts. Main's result is invalid at L2 (Tests #61 and #82), not a manifold, and has volume 0
  against the expected 7957.72. The test passes only because it counts faces. (MEASURED)
- **Why the branch fails.** A second site makes the same mistake: `farthest_end_gap` in
  `make_edge_in_face_with_vu` (WR-469) still sets V24 to 53.15. Two coincident copies of V24 then
  never glue, 4 edges stay unmated, the result gathers both sides, and the round returns `Err`.
  (MEASURED)
- **The fix.** Skip tangent-field edges in both gap measurements. The round then:
  - returns `Ok` with 9 faces;
  - is a manifold solid;
  - has the exact volume 7957.723438;
  - has every vertex tolerance at most 4e-5;
  - fails L2 only on the input's own tangent-field edge.

  The full goldens run is byte-identical to the branch's (the same 80 rows move), and the blend
  binary goes 865/1 → 866/0. (MEASURED)
- **gen0298.** Heal on read already sets every vertex tolerance between 78 and 248 on a body
  with a diagonal of 142. Main's tree build then doubles four of them. The body is invalid in both
  modes (8× Test #82). Nothing relies on the raise. Neither row satisfies invariants 7 and 10;
  the branch's row is the less wrong of the two. (MEASURED/READ)
- **Main-wide scan.** Across the 283 goldens and the kernel and ops tests, the large raises
  (above 1% of the diagonal) fall into four groups:
  - fixtures with a deliberately moved vertex, where the check should report the gap;
  - corrupt or healed imported bodies: `brep_p10` gen0298, `brep_p7` read_sphere.smb_modified0,
    `offset_geometry_creation` gen0050, gen0658 and gen0732;
  - one test that moves vertices on a copy;
  - this tangent-field test.

  None is a correct body that needs the raise. (MEASURED)

## A. The tangent-field test

### What the test does (READ)

`issue_2656_blend_regressions::a_tangent_field_edge_at_the_smooth_vertex_keeps_the_cap` (tests/blend):

1. Builds a 20 mm box and rounds the edge through [10,20,20] at r3.
2. Replaces the curve of the round's end arc on x = 0, from (0,17,20) to (0,20,17), with a
   `TangentFieldCurve` (`CurveKind::TangentField`) of ±y tangents over a UV line.
3. Rounds the edge through [0,8.5,20] at r1. That round runs into the arc's end at the smooth
   vertex (0,17,20).
4. Asserts `Ok` and 9 faces.

Its doc comment says what it guarantees (tangency-labs/solidcore#2679):

- `end_at_smooth_vertex` cannot decide a run-on from a tangent field (the field has no
  second or third derivative). So the End keeps its cap rather than failing the round with `SM_ERR`,
  as #2657 did.
- "The body is not checked a solid: the arc's curve is a field of vectors, not the arc, so nothing
  that reads its points can mate it."

So the test was written to guarantee the corner decision, not a valid solid. Its 9-face count is
the only thing it asserts about the result.

### What a tangent field evaluates to (READ)

`TangentFieldCurve::evaluate_point` (curve/tangent_field.rs) returns a cross-tangent *vector*. This
is the blended start and end tangents projected onto the surface's tangent plane, so its length is
at most 1. It is not a point. Upstream makes these curves only as temporaries inside
`CreateCornerBlend`. A file can still read one in.

### The input body is already invalid (MEASURED)

`check_brep` at L2 on the body before the r1 round (`ZZ_PRECHECK`, branch mode) reports:

- `SmBrep::Test #61 Valid: Bad Edge fails pE->ValidateGeometry(FALSE)` on the tangent-field edge.
- `AssertValid skipped: ValidatePointers failed`.

### What the 53 is (MEASURED)

On main, two raises happen inside tree builds during `create_corners`. Both are loop 2 of
`create_uv_trim_curves_body`, at `vertex::set_tolerance(v, 2 * gap_3d)`:

| Vertex | Face | Before | After | gap_3d | UV gap | Corner kinds | Path into the tree build |
|---|---|---|---|---|---|---|---|
| V24 (0,17,20) | plane x = 0 | 4e-5 | 53.1507 | 26.5754 | 0 | tangent-field, line | `FilletCorner::create_noting_roll_overs` → `check_nx1_degeneracy(_status)` → `evaluate_binormal_uv` → `face_surface_cache` → `build_tree_in` |
| V26 (0,20,17) | the r3 round's cone | 4e-5 | 51.7397 | 25.8699 | 0 | tangent-field, line | `end_at_smooth_vertex` → `continuing_edges` → `check_tangential_edgeuse_at_vertex` → `evaluate_binormal(_uv)` → the same tree build |

- The body's bounding-box diagonal is 34.64, so the ratios are 1.53 and 1.49.
- `gap_3d` is |`normalized_evaluate(eu, 0, uv = false)` − vertex point|. For this edge that
  evaluates the tangent field, so it measures |unit-or-shorter vector − (0,17,20)|. That is about
  |vertex| = 26.2 (MEASURED: 26.58 and 25.87).
- The 53 is twice that: **the distance from the origin to the vertex, doubled**. It is not a gap
  at all. The UV gap at both corners was already 0, so the corner fix had nothing to close.

A second, independent site makes the same mistake. On the branch, with tree builds no longer
raising, V24 (and a copy, V2) still go to 53.15:

- The raise is in `make_edge_in_face_with_vu_with_curve_identity`, the WR-469 block
  (topology/brep/p9.rs ~2058).
- `farthest_end_gap` evaluates every other edge at the vertex, including the tangent field.
- It is reached from the corner's `merge_corner_curve` → `TopologyIntersector::capture_curve_classes`
  → imprint. Then `max_tolerances` / `update_object_tolerance` copy the 53.15 onto the
  intersector's vertex V2.

MEASURED (`ZZ_VT`): main mode makes 9 raises above 1; branch mode makes 2.

### Why the fillet "needs" it: GatheredBothSides (MEASURED)

The fillet finish (blend/fillet_executive/p3.rs ~5080–5160, READ):

- stitches, retrying 5 times with the tolerance doubled each time, then orients;
- accepts `GatheredBothSides` from `orient_body` only when lamina > 0 and
  `finish_keeps_open_result` holds.

**Branch mode.** V24 = 53.15 comes from the WR-469 site alone. The corner face's stitch then leaves
4 lamina edges at every pass (2e-5 up to 8e-5):

- 26 (V24–V33)
- 42 (V24–V39)
- 43 (V33–V37)
- 44 (V37–V39)

V24 and V37 are two coincident copies of (0,17,20), both at 53.15. They never glue, so edges
42/44 and 26/43 stay unmated. `orient_body` gathers both sides, the finish refuses it, and
`blend` returns `Err((0, "<no comment>"))`.

**Main mode.** The extra tree-build raises put V26 at 51.74 and grow V24/V34/V35 to 143.63
(then 184.1):

- The stitch glues within those balls. That merges V24 away and leaves edges 42 and 46 with one
  vertex each.
- 5 lamina edges remain (39, 41, 42, 43, 46) around (1,0,20). The finish's open-result path
  accepts this, and `blend` returns `Ok` with 9 faces.

So the fillet does not need the raise for any geometric reason:

- It needs the *bigger* wrong tolerance so that the stitch collapses the corner topology into a
  shape the finish accepts as an open result.
- With the gap measured correctly, there is nothing to raise and the stitch mates everything
  (below).

### Is main's round correct? (MEASURED)

No. After the r1 round on main:

- **Validity at L2.** It fails:
  - Test #61 Bad Edge.
  - Test #82 too wide (vertex offsets 51.74 and 143.63).
  - `AssertValid skipped: ValidatePointers failed`.
- **Topology.** Not a manifold solid: edges 42 and 46 have one vertex, and 5 lamina edges remain.
- **Volume.** `brep_volume` returns 0.0 against the expected 7957.723438.
- **Tolerances.** Vertices at 51.74, 143.63 and 184.1 on a 34.6 mm body. Each ball holds the
  whole body, which breaks invariant 7 and the §5.1 Test 82 limit.
- **Cap geometry.** The cap face exists (that is the 9th face), but its corner is collapsed: V24 is
  merged away.

The test passes only because it counts faces.

## B. The defect and the smallest sound fix

### The defect (READ, MEASURED)

Two tolerance-growth sites treat a tangent-field edge curve as a point curve and "cover" a
distance that is not a gap:

1. Loop 2 of `create_uv_trim_curves_body`, which every caller runs except the branch's tree build.
2. `farthest_end_gap` in `make_edge_in_face_with_vu_with_curve_identity` (WR-469).

A tangent field has no positions, so no vertex tolerance can or should cover its "ends". The
branch removes (1) from tree builds but leaves (1) in other callers and leaves (2) everywhere.

### The prototype (MEASURED)

`ZZ_SKIPTF=1`: both sites skip an edge whose curve is `CurveKind::TangentField`. At (1), `gap_3d`
is taken as 0 for that edge use. At (2), the edge is skipped in `farthest_end_gap`. This is the
scratch commit "ZZ skip tangent-field gaps (opt-in)". A real change would be a curve predicate
("has positions") used at both sites, with no switch.

**The test.** It passes in both modes (branch, and branch + `ZZ_FULL=1`). The result:

- `blend` returns `Ok`.
- 9 faces, 20 edges, 13 vertices.
- `is_manifold_solid` is true.
- The volume is 7957.723438189298, matching the expected 7957.723438.
- Every vertex tolerance is at most 4e-5, so the tolerances are sane.
- Check L2 reports only `Test #61 Bad Edge` (the tangent-field edge 35) and the pointer skip
  that follows from it. These are exactly the input body's own two lines. The round adds nothing
  invalid.

**The goldens.** All 283 units, forwards, against the branch:

- Branch alone: 273 pass, 10 units / 80 rows move. These are the branch's own 80:
  - feature_executive 2
  - fillet_executive_p3 11
  - corpus.parity 1
  - heal_data_p3 5
  - stitch_p1 9
  - stitch_p2 9
  - assert_valid 29
  - brep_p10 1
  - brep_validation 1
  - face_p3 12
- Branch + prototype: byte-identical output for all 283 units (timings aside). **No row moves
  beyond the branch's 80.**

**The blend tests.** The `blend` test binary has 887 tests, including all `issue_2656_*`,
`fillet_*`, `fork_*` and `wr*` modules:

- Branch: 865 pass, 1 fails (this test), 21 ignored.
- Branch + prototype: 866 pass, 0 fail, 21 ignored.

INFERRED: the skip changes nothing for a body without tangent-field edge curves. A valid body
never has one, because upstream only makes them as temporaries.

### Other options, if a curve-kind check is unwanted

- (a) Restate the test to assert what #2679 guarantees:
  - `end_at_smooth_vertex` keeps the cap (`Ok(false)`, as the neighbouring cancel test does);
  - `create_corners` succeeds (MEASURED: it does on the branch);
  - and accept that the full round of an invalid body is refused.
  This is honest but loses a round that the prototype shows can be built exactly.
- (b) Refuse an edge with a tangent-field curve by name at fillet entry (invalid input).
  This is a valid refusal, since the input fails Test #61.
- (c) Heal on read: rebuild such an edge's curve from its trims. This is bigger, and there is no
  case for it yet.

The prototype is the smallest change that makes the round correct, and it moves no golden.

## C. gen0298 (`topology.brep_p10`)

**The case.** `Shell/problemShell16.smb`, `zone_tol -1`, `scale -1`, op Make. The body has
6 faces, 12 edges and 8 vertices, and its vertex-point diagonal is 142.4. (MEASURED, scratch
`ZZ_CHK` hook after the first export)

**What covers the huge gaps.** Heal on read. `heal_brep` (`heal_made` → `HealData::heal_targets`)
sets every vertex tolerance from 1.8e-3 / 2.9e-3 to between 78.1 and 247.8. This happens in both
modes (MEASURED, `ZZ_VT` backtrace). It is the heal path that §7.1 and §8 describe: measured, not
enclosed, and not recorded.

On main, the tree build then raises V4–V7 a second time to twice the corner gap:

| Vertex | Before | After |
|---|---|---|
| V4 | 115.9 | 235.1 |
| V5 | 148.7 | 324.9 |
| V6 | 247.8 | 545.6 |
| V7 | 244.5 | 497.4 |

The corners are B-spline/B-spline, the UV gap is 0, and the ratio to the diagonal is 1.65–3.83
(MEASURED). These raises are the 846 per block that the branch's gen0298 row moves by.

**Is it invalid under `check_brep`?** Yes, in both modes (MEASURED, check at L2 after the build):

- Main mode (`ZZ_FULL=1`): 3× Test #61 Bad Edge and 8× Test #82 (vertex tolerance too wide,
  offsets 78–546).
- Branch: 10× Test #61 and 8× Test #82.

The extra 7 Test #61 lines on the branch are edges whose ends the doubled vertex balls swallowed
on main. Main's raise hides them; it does not mend them.

Main-before-#20785 was not built. #20785 (WR-732) is an ancestor of the branch base. READ: the
WR-732 findings note records that gen0298's tolerances "roughly double" there and that vertex 4
"now covers its edge 11's end at 171.2, which its old tolerance did not". INFERRED: before #20785
the body still carried the heal-on-read tolerances of 78–248. Those already fail Test #82, since
every ball holds most of the body, so it was invalid there too.

**Does anyone rely on the raise?** No, as far as measured:

- The op is Make. The row only records the tolerances.
- No other row or test moves with it. The branch moves this one row in this unit.
- READ: no consumer reads gen0298 downstream.

**Is the branch's row right under invariants 9 and 10?**

- Invariant 9 (constructions) does not apply: this is imported data.
- Under invariant 10, imported data may grow only to an *enclosed, recorded* gap. Neither the
  heal-on-read growth nor main's tree-build doubling is enclosed or recorded.
- Under invariant 7, both are past the size where any growth is valid: each ball reaches entities
  it does not bound.

So neither row is "right". The branch's row is the more honest of the two:

- It drops the second, unenclosed doubling, which invariant 10 forbids.
- It leaves the check reporting all 10 bad edges.

The right row would come from refusing, or flagging, the heal growth past §8's bound. That is
separate work.

## D. Every tree-build raise on main

Counter: the `ZZR` log in loop 2 of `create_uv_trim_curves_body`. It records every vertex or
edge tolerance change made while the call stack is inside `BrepCacheFace::create_uv_trim_curves`,
with `ZZ_FULL=1`, i.e. main's behaviour. Size is measured as the vertex-point bounding-box
diagonal.

Runs:

- the 283 golden units forwards (`l46/f1`);
- the kernel and ops test binaries, single-threaded so that each line carries its test name
  (`l46/t1`, 16 binaries).

**Totals (MEASURED).**

- Goldens: all 283 units PASS in main mode. There are 271 tree-build changes in 11 units:
  - heal_data_p3 12
  - stitch_p1 84
  - stitch_p2 56
  - offset_executive_p2 2
  - offset_geometry_creation 4
  - sweep.api_brep 2
  - assert_valid 61
  - brep_p10 4
  - brep_p7 1
  - brep_validation 1
  - topology.vertex 44
- Tests, by binary: blend 8, boolean 19, geometry 4, offset 8, solidcore lib 29, sweep 4.
- Every test binary passes in main mode, except 3 tests in `geometry`. These are the branch's own
  new or changed tests, which expect the corner left alone:
  - `brep_query_cache_lifetime::a_uv_corner_gap_is_left_as_it_is_and_every_reader_answers_as_before`
  - `issue_1419_brep_check::a_moved_box_corner_keeps_its_tolerance_and_reports_its_edges`
  - `issue_1419_brep_check::local_vertex_tolerances_do_not_refuse_a_valid_body_or_its_cut`

**Raises above 1% of the diagonal (MEASURED).** Golden cases were attributed per case with
`--filter`, which matches substrings, so a parent name can catch its variants.

| Where | Cases / test | Raise | Ratio | Curve kinds, surface | On the branch |
|---|---|---|---|---|---|
| `topology.assert_valid` (35) | the mutation-6 fixtures (a vertex moved by 1 or 0.1): `l2_box_level2_m6_v-1_i0..7`, `l2_box_tests_m6_v-1_i0..7`, `box_mutation6_v0.1_i0/i3`, `box_mutation6_capi`, `file_Solids_box_mutation6_*`, `cylinder_mutation6_v0.1_i0/i3`, `sphere_mutation6_v0.1_i0/i3`, `update_{box,boxfile,cylinder,patch_sheet}_m6_i0_v1_l0/l2`, `gen0002` | 1e-5 → 2 (gap 1), or 1e-5 → 0.2 (gap 0.1) | 0.0115–0.89 | line/line on planes; ellipse/ellipse on spheres; line/ellipse on a cylinder and a B-spline surface | **Differs:** these are the branch's 29 moved `assert_valid` rows (the 10 listed by the runner are all in this set; the other 19 are INFERRED to be). The check reports the moved vertex instead of first widening its tolerance over the gap. Correct by design. |
| `topology.brep_validation` (1) | `box_mutation_6` | 1e-5 → 0.2 | 0.053 | line/line, plane | **Differs:** the branch's 1 moved row, same reason. |
| `topology.brep_p10` (4) | gen0298 | V4–V7 doubled (116–248 → 235–546) | 1.65–3.83 | B-spline/B-spline | **Differs:** the branch's 1 moved row (§C). Invalid in both modes. |
| `topology.brep_p7` (1) | `read_sphere.smb_modified0` | 4.4 → 8 | 1.33 | B-spline/B-spline, sphere, UV gap 30.6 | **Same:** the unit does not move on the branch. |
| `offset.offset_geometry_creation` (4) | gen0050 (2), gen0658, gen0732 | 2.4e-3 → 16 and 16.9; 2e-5 → 20.4; 20.75 → 21.0 | 0.92–1.21 | line/B-spline, line/line, planes | **Same:** the unit does not move on the branch. |
| blend test | `issue_2656_blend_regressions::a_tangent_field_edge_at_the_smooth_vertex_keeps_the_cap` | 4e-5 → 53.15 and 51.74 | 1.53, 1.49 | tangent-field/line, plane and cone | **Differs:** fails on the branch (§A). With the prototype, passes and is correct (§B). |
| geometry tests | `issue_1419_brep_check::a_moved_box_corner_keeps_its_tolerance_and_reports_its_edges` | 1e-5 → 2 | 0.49 | line/line | **Differs, by design:** the branch's own test, which asserts the moved corner is reported. |
| geometry tests | `issue_1419_brep_check::local_vertex_tolerances_do_not_refuse_a_valid_body_or_its_cut` | 5.7e-3 → 0.2 | 0.0115 | line/line | **Differs, by design:** the branch's own test. |
| solidcore lib | `topology::vertexuse::tests::read_only_uv_sector_is_the_sector_on_a_copy` (24 raises) | 1e-5 → 0.5 | 0.0174 | line/line, ellipse, plane and cylinder | **Same:** the test moves every vertex by 0.25 and compares sectors on copies. It passes in both modes. |

**Reading (INFERRED from the table).**

- Every large raise falls in one of these groups:
  - a fixture that moves a vertex on purpose, so that the check will report it;
  - an imported or corrupted body that heal on read has already made invalid (gen0298,
    read_sphere.smb_modified0; gen0050/0658/0732 are generated cut cases whose 2e-3 → 16
    raises cover a gap of half the body);
  - the tangent field.
- None is a valid body whose correct outcome depends on the raise.
- The only outcomes that differ on the branch are the branch's own 31 check rows, the gen0298
  row and the tangent-field test. The prototype in §B fixes the last of these without moving any
  golden.

## Notes

- One wait loop used `pgrep` against the rules; it was stopped at once and nothing else used it.
- An early main-mode goldens pass was void: `xargs -I U` replaced the U in `ZZ_FULL`. It was
  rerun with `-I @@` (`l46/f1`; the void run was discarded).
- The server worktree's build directory was cleaned with `cargo clean` at the end. The worktree
  itself remains, for removal with `cargo xtask worktree-rm`.
