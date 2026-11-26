# Review of tangency-labs/solidcraft#20945 (head 7330f9c5d2, branch cache-a2-gaps, fixes #20939)

**Verdict: PASS WITH FIXES.** The library change is correct and complete for what it claims: every gap cache of every edge use and vertex use is cleared on a non-identity scale, and `set_interval` clears exactly the caches that depend on the interval. No code defect found. The proof package is not ready yet:

1. **Run every goldens unit, not 20 chosen ones** (`cargo run --profile goldens -p goldens -- --all --jobs 4 --goldens`, forwards and `--reverse`). The PR changes what the ~40 library callers of `set_interval` that already existed do, not only the three it re-routes. Eight units that dump cache-initialised flags reach those callers and were not run: `topology.brep_p1`, `topology.edgeuse_p2`, `topology.edgeuse_p3`, `topology.face_p3`, `topology.vertex`, `topology.brep_p10`, `heal.gap`, `offset.offset_executive_p2`. CI has not run either: every job is "skipping" on the draft. So "43 rows, all in topology.edge" is unverified. Any extra moved row goes into the expectation commit with its proof.
2. **Make the 43 moved rows PROVEN in code.** They are labelled "MEASURED + ARGUED". Under "Reviewer briefs: whole-diff coverage", an ARGUED or MEASURED row blocks ready. Add one committed test next to `set_interval_clears_the_gaps_of_the_old_interval`: a body with every cache filled, then `edge::reverse_orientation` on one edge, then for every edge use and vertex use the cached gap equals the forced recalculation **including `edge_t`** (not only `gap_3d`, which is unchanged to rounding by a reversal). That test fails on main (stored edge_t 0.7635 against 0.2365), and it turns the mechanical inventory into a proven cause.
3. **Correct the "Direct interval writes examined and left" paragraph.** "The remaining writes are in tests" is false. Four library writes to `Edge::interval` remain:
   - `blend/fillet_executive/p1.rs:1553`
   - `blend/fillet_corner/p1.rs:3325` and `:3333`
   - `blend/fillet_corner/p2.rs:1748`

   Either route them through `edge::set_interval` (a one-line change each) or list each with its reason. The fillet_corner writes follow the fillet `set_edge_curve`, which clears the edge/face and vertex gaps but not `edge_ccw_edge_gap`. The fillet_executive write follows a curve re-parametrisation.
4. **Relabel.** "Upstream has the same behaviour (PROVEN, by reading the pinned source)" is a reading, so ARGUED; the rules reserve PROVEN for claims asserted in code. The same applies to the vertex-use part of "The other caches (PROVEN by the tests below and by reading)": no test asserts the vertex-use caches after a scale. Either relabel it or extend test 1 (fix 5).
5. **Tighten the tests** (small):
   - `issue_20939_a_scaled_body_caches_its_own_edge_gaps`: `edge_gap` uses `.ok().flatten()`, so an error on both the cached and the forced read compares equal and passes silently. Use `.unwrap()`. Also assert the vertex uses' `vertex_edge_gap` and `vertex_face_gap` equal their forced recalculation.
   - `issue_20939_checks_and_tolerances_after_a_scale_read_the_real_gaps`: assert `c1.0` (Level 2 valid), not only "no Bad Gap line".
   - `set_interval_clears_the_gaps_of_the_old_interval`: assert the closed form its doc states (the vertex/edge and edge/edge gaps are 1.0 after shortening a 10 mm edge by 1 at each end), not only cached == recalculated and whole != recalculated.

Optional, not blocking:
- Squash the final "format the #20939 test" commit into the test commit, so the expectation commit is the last one (Validate-then-publish item 3).
- See finding F6 on the join's neighbour cache.

## Findings (most serious first)

### F1 (proof package, blocks ready): the moved-row inventory rests on 20 hand-picked units, and the selection misses the existing set_interval paths
`topology/edge.rs:388-407`: `set_interval` now clears caches for **every** caller. Callers that existed before this PR include:
- the UV-trim recovery in `CreateUVTrimCurve` (`topology/edgeuse/p3.rs:1019`, `:1043`)
- the vertex snap in `topology/brep/p1.rs:2085`, `:2112`
- the lamina edges in `topology/brep/p2.rs:3425`, `:3474`, `:3511`
- `topology/brep/p3.rs:4187`, `:4296`
- `topology/brep_constructor.rs:1359`
- offset `step_gaps.rs` (four sites)
- fillet removal and the fillet solver, corner, geom and intersector (about 15 sites)
- `edge.rs:2886`

Failure scenario: a `topology.face_p3` case creates UV trims, the recovery path shortens an edge, and the unit's state dump (it records `is_init` flags, as do the eight units listed in fix 1) now shows 1 to 0 for rows the PR does not list. The body says "CI runs the rest", but CI has not run (draft; `gh pr checks` shows every gate job skipping).
Verified: the list of units that dump flags (`grep is_init crates/goldens/src/units`); the callers (grep). Not verified: whether any of those rows actually move (needs the full goldens run).

### F2 (proof package, blocks ready): the 43 rows are MEASURED + ARGUED, with no committed assertion
`goldens/expected/topology.edge.jsonl`. I recounted from the diff: 43 removed and 43 added rows, the same case set, 36 `*_reverse` / `*_rb_e0_reverse` rows and 7 `*_orient_seq` rows. In **all 43** (not a sample), the only changed field is `values.state.blocks`. In every changed block, sum and abs_sum fall by the same integer n, and the weighted sum by an integer in [n(n+1)/2, 256n]. Per row, n is 3 to 8 for the reversals and 4 to 16 for orient_seq, consistent with up to 4 edge uses times 2 flags per reversed edge (radial structure: two face uses per face). The dump (`goldens/src/units/topology_edge.rs:810-833`) records `max_edge_face_gap.is_init` per edge use and `vertex_edge_gap` / `vertex_face_gap.is_init` per vertex use. No interval, tolerance or link entry moves.

No row loses an answer: no gap value is dumped, only flags. So nothing is lost coverage.

The builder's "why the old flags were wrong" is MEASURED and labelled so. What it actually shows is that the stored **parameter** went stale: edge_t 0.7635 against 0.2365. `reverse_parameterization` maps t to a+b-t, and the orientation flip moves the vertex use to the other end. It does not show a wrong **value**: 5.62e-15 against 4.09e-15 is rounding noise on an exact-zero gap. Say "stale parameters, values equal to rounding". That justifies clearing, because readers that take `edge_t` (healing) would use a wrong point. Clearing rather than remapping (t' = a+b-t) is correct and costs one measurement on the next read. Remapping is an optimisation, not a missing fix. Fix 2 makes this PROVEN.

### F3 (body accuracy): four library writes of `Edge::interval` remain, and the body says the rest are tests
Verified by locating each against its file's first `#[cfg(test)]`:
- `blend/fillet_executive/p1.rs:1553`: after `orient_edge_sequence` (now clearing through `reverse_orientation`), `trim` and `edit_parameterization`.
- `blend/fillet_corner/p1.rs:3325`, `:3333` and `blend/fillet_corner/p2.rs:1748`: after the fillet `set_edge_curve` (`fillet_geom/p4.rs:193`), which resets edge/face and vertex gaps but leaves the edge's and its clockwise neighbours' `edge_ccw_edge_gap` measured over the old curve and interval.

Impact today is small: the only reader of `edge_ccw_edge_gap` is the legacy healer, `heal/heal_data/p2.rs:472`. Still, the body's claim is wrong.
The writes the body does name check out:
- `brep/p9.rs` MakeEdgeInFace (a new edge)
- `brep_constructor.rs:1163` (building)
- `brep_data_write`, `brep/p10.rs:1134` (whole-record copy)

`topology/brep/p8.rs:1179` writes a segment struct, not an `Edge`.

### F4 (labels): "PROVEN, by reading the pinned source"
I checked the source myself: `SmEdge.h:446` `SetInterval` is `{ m_vInterval = crInterval ; }`, and `SmEdgeuse::Notify` resets the edge/face gap only on `SPLIT_IN_BREP`, `CHANGE_GEOMETRY` and `SPLIT`. The claim is true, but it is a reading, so ARGUED. "PROVEN: fails on main" for each test is a test run, so MEASURED. That label is acceptable if the revert was really run; I could not rerun it.

### F5 (tests): weak assertions
Covered in fix 5. In addition:
- Test 1 compares only `gap_3d`, so it would not catch a stale `edge_t`/`face_uv` after a scale. That is irrelevant here because the caches are cleared, but it is the same blind spot as F2.
- File placement: `tests/issue_1570_transform_matrix.rs` is the existing transform_matrix file, which is fine.
- `tests/join_owns_curve_by_edge_identity.rs` is about join curve ownership. A `set_interval` unit test fits better in `topology/edge.rs`'s `mod tests` (the area's existing tests). Minor.

### F6 (pre-existing, not worsened): after the join, the deleted edge's other neighbour keeps an edge/edge gap that names a deleted edge use
`topology/brep/p5.rs:909/965` calls `set_interval(survivor)` **before** `squeeze_edge` (`:982`). Take the loop X to S to D to B (S survivor, D deleted, V between them) and its mate B' to D' to S' to X':
- `set_interval` clears S' and S'.cw, which is D' at that moment.
- After the squeeze, B'.ccw is S', but B'.`edge_ccw_edge_gap` still names D', whose record `squeeze_edge` removed (`p3.rs` "Remove and destroy all squeezeEdge->edgeuses").
- Symmetrically, the X whose ccw was D, when D precedes S.

On main nothing was cleared at all, so the PR neither creates nor widens this. The reader is the legacy healer only. The builder's out-of-scope note is accurate.

A cheap improvement: clear the cw neighbours of the deleted edge's uses before the squeeze, or call `set_interval(survivor)` again after it. Also pre-existing: the survivor's vertex uses at V move to the far vertex (`p3.rs` "Move all deleteVertex->vertexuses") and keep `vertex_face_gap` measured at V's point (MergeInBrep does not reset vertex-use caches: `vertexuse.rs:2141` resets on PostEdit, SplitInBrep, ChangeGeometry and Split only). Neither is in this PR's claim.

### F7 (new error path, low risk): `set_interval` now dereferences `cw` and `vertexuse`
`edge.rs:394-405`: `b.edgeuse_mut(cw)?` and `b.vertexuse_mut(vu)?` return `NotFound` on a dangling link, where the old plain assignment always succeeded. Callers that run in the middle of an edit (offset `step_gaps`, fillet removal ring, the constructor's temporary radial-sector edge at `brep_constructor.rs:1359`) would now fail if a link dangles at that moment.
- The constructor case: the temporary edge has no edge uses when `set_interval` runs, so it is safe.
- The split (`p9.rs:1247`) and join (`p5.rs:909/965`) call it on intact topology, so they are safe.

Settled by `cargo test -p solidcore` plus the full goldens (fix 1).

### Checks that passed (verified by reading)
- **Cache enumeration.** Only four gap caches exist on the records (`topology.rs:608-610`, `656-658`). `loopuse::Closed3d::max_gap` and `topology_intersector` `max_gap` are results, not caches.
  - `max_edge_face_gap`: depends on the edge curve, the edge interval and the face surface. Mates share it.
  - `edge_ccw_edge_gap`: depends on this edge's end parameter and the ccw edge's start parameter, so on both intervals and curves.
  - `vertex_edge_gap`: depends on the vertex point and the edge curve at the edge use's start parameter. Synced to siblings on the same edge.
  - `vertex_face_gap`: depends on the vertex point and the face surface.
- **set_interval completeness.**
  - It clears all of e's edge uses: mates, all four radial uses, and wire uses.
  - It clears the edge/edge gaps measured at e's end (its own uses) and at e's start (each use's `cw`, whose ccw is that use).
  - It clears every vertex use at either end: each use's start vertex use, mates covering the far end, and siblings, which are always uses of e.
  - The far vertex of a single-use edge has no cache measured against e.
  - `vertex_face_gap` does not depend on the interval and is correctly left alone.

  Not too much: clearing is O(edge uses) stores. No loop reads a gap between successive `set_interval` calls with `force = false` in a way that now recomputes per iteration: the `CreateUVTrimCurve` recovery calls it at most twice per edge use.
- **Transform completeness** (`p5.rs:3692-3706`).
  - Edge-use clearing walks `p1::edges` (every edge in the arena, wire edges included) and every edge use on each edge (both members of a pair, and uses without a face).
  - Vertex-use caches: `vertex::set_point` (`vertex.rs:1374`) always sends `ChangeGeometry`, which reaches every vertex use (`vertexuse.rs:2141` resets both gaps). It does not compare the old point with the new one, so a vertex at the origin under a pure scale, or any transform with zero translation, is still reset.
  - Between the vertex moves and the end of `transform_in`, the only reader is the `replace_surface` fallback (`p5.rs:3669`, `create_uv_trims = true`). It sends ChangeGeometry and reads `vertex_face_gap` with the already-moved vertex, already-transformed curves and the replaced surface, so whatever it caches is current.
- **"Scale is not (1,1,1)" is the right test.** `transform_matrix` sends a length-changing matrix either down the low-work path (orthogonal rows, so scale = the row lengths) or through the SVD (`mirror.rs:512-603`), where the length change sits in `scale_ut` = |U rows| times the singular values. A matrix that changes lengths has a singular value other than 1, so some `transform_in` call gets a scale other than 1 and clears. Rotations come through with a scale of (1,1,1) to rounding (`approx_eq`).
- **Mirrors.** A reflection is det < 0, so it goes down the SVD path: `transform_in` with scale (1,1,1), then `mirror_in`. `mirror_in` replaces each surface through `replace_surface` (ChangeGeometry, which clears the edge/face gap) and moves the vertices through `set_point`. It does not clear `edge_ccw_edge_gap`. Distances are kept, so the value is right; the stored `edge_t` is right if `create_mirror_curve` keeps the parametrisation (inferred, not checked). Pre-existing and outside this PR.
- **Rotation keeps caches.** Rigid transforms of analytic and B-spline geometry keep their parametrisation (inferred from the transform code path, not checked per curve kind). Where a surface is replaced instead, ChangeGeometry clears its edge/face gaps.
- **The split** (`p9.rs:1247-1329`): the previous direct writes had no side effects that the setter lacks. The setter adds clearing only: no notification, no validation, no clamp. **The join**: same. **`reverse_orientation`** (`edge.rs:2380`): same, and the interval written is identical.
- **Upstream reading** is correct (F4).

## Q4: results in the wild (inferred from code; no build)
- **Body::transform** (`solidcore-ops/src/body.rs:347`) calls `transform_matrix(.., true)`. Tolerances scale by max |s_i| (`p5.rs:3710-3743`, floored at the zone), and a true gap scales by at most max |s_i| (plus NURBS conversion, which is exact for analytic geometry). So a body valid before the scale stays valid with honest gaps. Honest gaps cannot newly refuse a valid scaled body in `check_brep` or the ops post-operation check (rounding aside).
- **Scaled down:** main's stale, larger gaps gave a spurious `Bad Gap` and raised tolerances (the issue's 0.001 case). Now the body checks valid and keeps its tolerances.
- **Scaled up:** `matched_tolerances` (`boolean/operand_tolerances.rs:57`) tightens a copy, checks it, and applies only if the copy is valid. With honest gaps, `tightened` regrows edges to the real gap (about 5.3e-3 in the issue) instead of the stale 5.3e-6, so the coarse operand keeps tolerances that cover its gaps. On main the copy check also read the stale cache, passed, and left an operand under its real gap that later checks could refuse.

  So the new path can only make `matched_tolerances` decline (operands left as they came), never refuse: fewer later refusals, not more. A user sees larger, honest tolerances on a scaled-up operand in a Boolean, and no spurious refusal on a scaled-down body.
- **A scale through `transform(.., scale_tolerances = false)`:** honest gaps grow and tolerances do not, so the check can now report a real Bad Gap that main missed. This is correct behaviour. Only kernel-internal callers and tests use `false`; the issue's own test helper uses it for a pure rotation, where nothing is cleared.
- **Tests that scale and then operate:**
  - `solidcore-ops/tests/fixes.rs:28` (non-uniform scale then check)
  - kernel cases `solids/cov-scale-uniform-about-a-corner` and `solids/cov-scale-sphere-to-ellipsoid` (scale last, graded result)
  - app `crates/engine/src/tests.rs:3447` `booleans_across_the_walls_of_a_non_uniformly_scaled_body` (`solid.scale` then combine)
  - `solidcore/tests/issue_1570_transform_matrix.rs` (old and new)

  The body names none of these as run except the last. Boxes and spheres have zero gaps, so a change there is unlikely.

## Q5: interaction with #20944 (cache-a3-size, head ed9b77d369)
- **Textual:** #20944 replaces two lines inside the tolerance block (main `p5.rs:3705-3708`: `set_this_model_size_estimate(size)` without `k`, and drops `set_context_model_size_estimate`). #20945 inserts its block before the `WR-507` comment (main `:3691`). The hunks do not overlap and their context does not collide, so no textual conflict.
- **Semantic:** the edits are independent. #20945 decides which gaps are honest; #20944 decides the zone that tolerances are floored at. One coupling: `calc_max_edge_face_gap` builds its gap function with `context_xsect_tol_3d(k, ..)`, and after #20944 a scale no longer updates the context's model size. That tolerance only marks samples as in tolerance (`heal/gap.rs:1484-1491`), so the gap value should not change (inferred).
- **Order:** either. Whichever merges second should re-run `topology.brep_p5`, `topology.brep_mirror`, `topology.brep_p1` and both PRs' new tests (`issue_20939_*`, `wr507_508_transform_matrix_tolerances`).

## Q7: hygiene
- `Fixes #20939`: present.
- `Changes a default: no`: correct (kernel results, not a UX default).
- `Test change:` line: present, and it names the three tests and the 43 rows.
- No local paths and no session links in the body, commits or diff.
- No hit for thread, parallel or concurren(cy) in the diff, the commit messages or the body (searched).
- Commit e6441258c6 is an empty claim commit. The last commit (7330f9c5d2) is a test format commit after the expectation commit (fix optional).

## Files in the diff (6/6 reviewed)
| File | What I checked |
|---|---|
| `solidcore/crates/solidcore/src/topology/brep/p5.rs` | Join: both interval writes routed through the setter, called before the squeeze (F6). `transform_in`: clearing placement after the curve/surface transforms and the reflection; the scale predicate against `transform_matrix`'s decomposition; coverage of wire uses and mates; vertex-use reset through `set_point`; the readers between the vertex moves and the end; tolerance block unchanged (Q5). |
| `solidcore/crates/solidcore/src/topology/brep/p9.rs` | The split's six interval writes routed through the setter on intact topology; no side effect lost; the trimmed intervals reach the setter afterwards as before. |
| `solidcore/crates/solidcore/src/topology/edge.rs` | `set_interval`'s clear set against each cache's dependencies (complete, not excessive); the new `?` dereferences (F7); `reverse_orientation` writes the same interval; doc comment accurate. |
| `solidcore/crates/goldens/expected/topology.edge.jsonl` | 43 rows recounted (36 + 7). All 43 blocks checked mechanically: only cache-flag 1 to 0 signatures, no other field (F2). |
| `solidcore/crates/solidcore/tests/issue_1570_transform_matrix.rs` | Hits the issue's exact bodies and scales; compares cached against forced; the twin comparison covers the check, tolerance pass, tightening and copy check. Weaknesses in F5 and fix 5. |
| `solidcore/crates/solidcore/tests/join_owns_curve_by_edge_identity.rs` | Exercises `set_interval` directly on a box edge with 4 uses; asserts cached == recalculated; no closed form; placement (F5). |

## Could not verify without a build
- **Each new test fails on main and passes here:** run `cargo test -p solidcore --test issue_1570_transform_matrix issue_20939` and `--test join_owns_curve_by_edge_identity set_interval_clears`, once on head and once with `src/` reverted to origin/main.
- **The moved-row set is complete:** `cargo run --profile goldens -p goldens -- --all --jobs 4 --goldens` (and `--reverse`). The eight flag-dumping units in fix 1 matter most.
- **No new NotFound** from `set_interval` in mid-edit callers: `cargo test -p solidcore`, plus `cargo test -p solidcore-ops --test kernel_cases` (also covers `cov-scale-*`) and `--test fixes`.
- **The app path:** `cargo test -p <engine crate> booleans_across_the_walls_of_a_non_uniformly_scaled_body`.
- **`create_mirror_curve` and the rigid curve/surface transforms keep parametrisation** (the stored `edge_t`/`face_uv` stay right under a rotation or mirror): inferred from reading only.
- **The #20944 coupling through `context_xsect_tol_3d` does not change a gap value:** run the issue_20939 tests on a merge of both branches.

Verified = read in code or computed from the diff (the cache list, the clear sets, the transform paths, the upstream lines, all 43 golden rows, the library interval writes, CI status, hygiene search). Inferred = the Q4 user-visible effects, the rotation/mirror parametrisation, the #20944 coupling, and F7's risk level.
