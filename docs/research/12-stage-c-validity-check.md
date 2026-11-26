# 12. Stage C study: the validity check of a body

2026-11-22, read-only, on `main` 782ef9afc (`sc-wt/cache-arch`). Method and vocabulary
as in 07-boolean-gather-apply.md. Paths are relative to `solidcore/crates/solidcore/src`, OPS/ =
`solidcore/crates/solidcore-ops/src`. Evidence: **[V]** read myself, **[A]** taken from an earlier report
(02, 05, 08, 09) without re-reading, **[I]** inferred. Nothing was built or run.

Abbreviations: VAL = `topology/brep/validation`, AV = `VAL/assert_valid`.

## 0. The invariant

`docs/validity-and-tolerances.md` section 9, invariant 11 (:424-425), verbatim **[V]**:

> 11. A check is a function of the body: running it twice gives the same report, and the validate-only
> pass changes nothing (`validate_only`).

Section 7.3 (:320-325) says `validate_and_update_tolerances` is the other gap repair, that the offset
executive calls it, and that with `validate_only` it is AssertValid's tolerance pass **[V]**.

Today the invariant does **not** hold as written. The check writes memos, trees, NURB forms and, on some
bodies, trims, curve control points and edge tolerances (section 2). The RNG in test 12 breaks "twice gives
the same report" outright: one untouched body was valid 13 times and invalid 3 times in 16 calls (plan-v4,
section 1).

## 1. The operation as it is

Entry: `check_brep(k: &mut Kernel, b: &mut Brep, level)` (`AV.rs:347`), then `check_brep_scoped`
(:356-374), then `check_brep_with_keys_scoped` (:399-429) **[V]**.

| # | Phase | Where | Kind |
|---|---|---|---|
| P0 | Ops only: lock the body, `brep_copy` it under `catch_unwind`, unlock, check the copy | OPS/check.rs:113-160, OPS/session.rs:766-781 | edit (a whole-body copy) |
| P1 | `validate_pointers(k, b, …, level, walk=false, keys)`. Preflight, topology, faces (each via `check_face`, on a standing copy if the face has no trims, :446-478), loops and edge uses (`check_edgeuse_geometry`, :630), `check_edges` (:675), `check_vertices` (:676), vertex exclusion (test 82, :793). If any report failed, AssertValid is skipped | `AV.rs:407-408`; `VAL/pointers.rs:383-800` | read-like, with curve repairs (C1) |
| P2 | `brep()`: `sag_object`, then a second pointer pass `validate_pointers_excluding(…, 0, walk, keys)`. Returns at once if the pointer status fails | `AV/brep.rs:47-66` | read-like, same writes as P1 |
| P3 | Tolerance pass `validate_and_update(k, b, rep, validate_only=true)`. Per face, then per upward edge use with a `marked` set: vertex/edge gap, vertex/face gap, edge/face gap; tests 0-4 | `AV/brep.rs:69`, `:238-240`, `:265-494` | read-like; fills memos |
| P4 | Brep tests 0-21: test 8 (`BrepQuery::of_brep`, `find_infinite_region` rays, :104-109), per region tests 16/10/11, test 12 (`is_inner_shell_of`, RNG, :154), 17, 18/19 (`collect_regions`), 13/14, 21 (warning) | `AV/brep.rs:70-215` | read-like; RNG |
| P5 | Walk (`sub_topology`, :582-622): regions, shells, faces, loops, edges, vertices, in that order. Each loop calls `check_cancelled`; the results are ANDed | `AV/brep.rs:225`, `:582-622` | read-like; trims, trees, NURBs |
| P6 | Collect reports: pointer reports first, then AssertValid reports in native order; warnings (test 21) are excluded from `failures` | `AV.rs:147-267` | sequential, cheap |

Update mode is a separate entry: `validate_and_update_tolerances(k, b, validate_only=false, …)`
(`AV.rs:506-529`). It is P3 with raises. Callers **[V]**:
- `update_tolerances_after_boolean` (`VAL/wide_vertices.rs:22-25`), after `centre_wide_vertices`. This runs
  on every ops Boolean result (report 08 section 3.1), and inside `tightened` (`boolean/operand_tolerances.rs:88-111`).
- The offset executive (`offset/offset_executive/p2.rs:2122`).

Non-test kernel callers of `check_brep` **[V]** (grep):
- `boolean/operand_tolerances.rs:71`: a trial `brep_copy`, tighten, full check, then apply to the real operand.
- `offset/offset_executive/p2.rs:6048`.
- `topology/brep.rs:231` and `primitive_outward.rs:114`: Level0.
- `p7.rs:2989`, `:3037`.
- `local_operation.rs:1517`.

I did not find healing callers of `validate_and_update_tolerances`. Ops healing does not use the legacy
healer (report 08 section 3.6 **[A]**).

## 2. Writes in the read-like phases

Frequency is from the measurements on main given in the task (153 gap fields, no trims made when trims
exist, 2 to 4 trees on curved bodies) and from reading the code.

### 2.1 P1/P2 pointer passes

| # | Write | Where | Fires | Class |
|---|---|---|---|---|
| W1 | `fix_reversed` builds a NURB form (`make_nurb`) for a line, ellipse, hyperbola or parabola edge curve that has none | `VAL/pointers.rs:2123` to `:1980-2006` | always on the first check of a body with analytic edges [I]; idempotent afterwards | P |
| W2 | `fix_reversed` / `fix_reversed_on` move a control point when an end of the polygon turns back (dot < -0.95). This is on the edge curve (:2123) and on each UV trim (:2192-2196), "Native const geometry repairs use mutable curve access" (doc :18) | `:2050-2060` | rare [I]; the condition is a defect. UNVERIFIED count | **C1** |
| W3 | `check_face` on a face that has trims, run on `&mut b` | `:465-474` | unknown: the body of `trimming_tools::check_face` was not read | UNVERIFIED, probably S/P |
| W4 | A trimless face is checked on `clone_standing_for_self()` (a whole-body copy, once per check) | `:461-465` | when any face lacks trims | S (the copy is the cost) |
| W5 | `face_contact` moves the face caches into `BrepFaceServices::with_caches(b, take(caches))` and back | `:1613-1624` | per vertex/edge clearance query near the size limit | S; one of report 09's 2 copies is a candidate [A] |
| W6 | `vertex_ball_reaches`, `edge_tube_reaches` take `&mut Brep` (cache builds for the queries) | `:1341`, `:1531` | only for vertices or edges at or past the ratio, and in the stitched scope | P (trees) |
| W7 | `check_edgeuse_geometry` evaluates with `ok_to_make_uv_trim_curve = false` (WR-446) | `:1901-1915` | never makes a trim | none |

Because P1 and P2 run the same pointer checks, a W2 repair in P1 is seen by P2 and by every later test of
that edge **[V]**. That is the coupling.

### 2.2 P3 tolerance pass (validate-only)

| # | Write | Where | Fires | Class |
|---|---|---|---|---|
| W8 | Gap memos: `vertex_edge_gap`, `vertex_face_gap` (`force = false`) and `max_edge_face_gap` on vertex uses and edge uses | `AV/brep.rs:360-363`, `:452`; `topology/vertexuse.rs:551-733`; `topology/edgeuse/p1.rs:906-1060` | always: up to 153 fields on a full check (measured) | P (as values) / **C3** as memos |
| W9 | `calc_vertex_edge_gap` resets and writes the memo on every sibling use of the vertex on the same edge. `calc_vertex_face_gap` resets siblings on the face and writes its own | `vertexuse.rs:551-620`, `:630-678` | always | part of C3 |
| W10 | `vertex_face_gap` calls `compute_uv_point(k, b, vu, false)`: no trim is made, but a global point solve may build the face's surface cache | `vertexuse.rs:630-678` | curved faces | P (tree) |
| W11 | `b.validate_only()` is flipped on and restored | `AV/brep.rs:265-277` | always | S |
| W12 | Bounding box: only when updating | `:294-304` | never in the check | none |
| W13 | `ball_reaches` / `tube_reaches` memos in the stitched scope | `:401-434`, `:472-484` | stitched scope only | S |

The comment at `AV/brep.rs:355-359` **[V]** states the coupling: a sibling reset during a refresh shows in an
earlier gap. With `force = false`, a stale memo from before the check is trusted, so the check is not a
function of the body. After a scale, the kernel-level check reads stale gaps (report 08 sections 3.2 and 3.7 **[A]**).

### 2.3 P4 brep tests

| # | Write | Where | Fires | Class |
|---|---|---|---|---|
| W14 | `BrepQuery::of_brep(b)` and its `OnceCell`s; rays in `find_infinite_region(&mut Kernel, &mut Brep, &BrepQuery)` | `AV/brep.rs:104-109`, `brep/p9.rs:166` | always | P (built in prepare; the query is not Sync) |
| W15 | `k.rng` draws in `shell::is_inner_shell_of` | `topology/shell*.rs:1584`, draws at :1715-1731; called from `AV/brep.rs:154` | bodies with more than one shell in a region | S once A1 lands (per-call generator) |
| W16 | `MarkLock` on `k.context` for shell and region marks | `AV/shell.rs:93-99`, `:257-260` | always | S |

### 2.4 P5 walk

| # | Write | Where | Fires | Class |
|---|---|---|---|---|
| W17 | UV trim creation. Face tests 12/13: `tvu::compute_uv_point(k, b, vu, ok_to_make=true)` ("side effect: builds the UV trim curve", `AV/face.rs:191`). Vertex-use test 5: `is_sector_missing_seam(k, b, vu, true)` with `uv_space_okay=true`. Also possibly `evaluate_binormal` (edge use and radial order), and the callees of face tests 37/38 and `has_seam_problem_in_brep_mut` | `AV/face.rs:185-200`; `AV/vertex.rs`; `vertexuse.rs:1181`, `:1905`; `edgeuse/p3.rs:2873`; `face/p4.rs:2593`, `:2663` | only where a trim is missing: 0 on bodies whose trims exist (measured) | P (prepare missing trims) |
| W18 | Edge tolerance raise inside `set_uv_trim_curve` when a W17 trim is made: up to 2 x max_dist. It is blocked only when `brep.validate_only()` is set, and the flag is set only around P3, not during P5 | `edgeuse/p1.rs:807-822`; flag setters `brep.rs:272-275`, `AV/brep.rs:265-277` | only with W17 | **C2** |
| W19 | `set_uv_trim_curve` resets the vertex/face gap memos of both uses' vertex uses (so a W17 trim invalidates W8 values filled in P3) | `edgeuse/p1.rs:750-825` | only with W17 | part of C3 |
| W20 | Surface cache trees, through the cache slot: `point_classify_indexed` (face tests 12/13, `&mut Brep`), `point_3d_classify` (test 24), `surface_assert_valid` with `BrepFaceServices::new(&mut *b)` (`AV/seams.rs:101-133`), `is_within_xsect_tol_3d_of_faces` (vertex test 1), `has_seam_problem_in_brep_mut` | as listed | 2 to 4 trees on curved bodies, rebuilt on a repeat check (measured); 0 on plane/cylinder | P (phase table) |
| W21 | `edge_curve_check` takes `b.edge_mut(e).curve.as_mut()`. Parabola and hyperbola call `make_nurb` when the NURB is missing. The trim check takes `uv_trim_curve.as_mut()` | `AV/edge.rs:108-112`, `:133-135`; `edgeuse()` trim check | parabola/hyperbola only; W1 has usually made them already | P |
| W22 | `EdgeSets { loopuse_edgeuses, wire_edges }`, filled lazily by `in_loopuse` and `in_wire_shell` | `AV/edge.rs:171-174` | always | S |
| W23 | Kernel memos `curve_analysis`, `loop_area_memo`, `recertify_memo` (report 05 V2 [A]; I did not re-read where they are written) | | commonly | S (per-worker kernel) |
| W24 | `check_cancelled` | `AV/brep.rs:582-622` | always | S |
| W25 | `is_tangent_sector(k, b, eu, 5.0, false, false)`: "no new UV trim curves" | edge use, Same orientation | always on manifold edges | P (trees only) [I] |

### 2.5 Update mode (not the check; same pass with `validate_only = false`)

| # | Write | Where | Class |
|---|---|---|---|
| U1 | Vertex raise `vertex::set_tolerance(b, v, zone_tol3d(2·gap), true, true)` under the ratio limit. A vertex raised for edge use i is read by later edge uses at the same vertex, by their reports and by their raise decisions | `AV/brep.rs:372-393` | **C4** |
| U2 | Edge raise `edge::set_tolerance(b, e, grown, true, true)` when gap < size/800 and under the ratio | `:459-469` | **C5** |
| U3 | Bounding box recomputed | `:294-304` | sequential |

### 2.6 Counts

| Class | Count | Items |
|---|---|---|
| P | 7 | W1/W21 NURB forms; W8 gaps as values; W6/W10/W20/W25 trees; W14 `BrepQuery`; W17 missing trims |
| R | 0 | W17 could be R instead of P, but there is no reason: the set of missing trims is known before the walk |
| C | 3 in the check, plus 2 in update mode | listed below |
| S | 9 | W4 standing copy, W5 cache move, W11 flag flip, W13 reach memos, W15 RNG (after A1), W16 marks, W22 `EdgeSets`, W23 kernel memos, W24 cancel |
| Unknown | 1 | W3 `check_face` on a face with trims |

The C items:

- **C1. Curve repair in the pointer check (W2).** The check moves a control point of an edge curve or
  UV trim whose end turns back. P2 and every later test of that edge read the repaired curve. P1 run twice
  gives a different report the second time.
  - Reformulation: judge the turn on a scratch copy of the control points and report it. Return a
    `RepairCurve` request (07 already names `RepairEdge`), applied only by an explicit repair entry such as
    healing or `validate_geometry(fix_tolerances = true)`. The check alone changes no curve.
  - Result change: yes, where it fires. The check would then report the defect instead of fixing it
    silently.
- **C2. Edge raise from a trim made during the walk (W17 + W18).** It fires only when a trim is missing,
  because `validate_only` is not set in P5. Later edge and edge-use tests read the raised tolerance.
  - Reformulation: prepare the trims first, so W18 never fires inside the phase. Alternatively, set
    `validate_only` for the whole check, which changes results where the raise fires today.
- **C3. Gap memos as shared state (W8 + W9 + W19).** A gap read later depends on which earlier
  computation reset the siblings, and on whether a W17 trim reset the vertex/face gaps. Stale memos from
  before the check are trusted.
  - Reformulation: in the check, compute each gap as a plain value from the prepared body (`force`
    semantics, no memo write). `cached_max_edge_face_gap(b: &Brep, …)` (`edgeuse/p1.rs:929`) and the
    `SectorBrep::Reading` mode (`vertexuse.rs:760-800`) are the existing read-only paths.
  - Result change: only where a memo was stale (report 08's scale case), which is a fix.
- **C4/C5 (update mode only).** The vertex and edge raises are interleaved with the reports. This is
  07's C1 pattern: gather `RaiseVertex` and `RaiseEdge` with the largest value per entity, apply after.
  - The final tolerances are the same if raises never shrink, each is a max, and each decision reads only
    gaps (which do not depend on tolerances). [I] This needs a proof per decision: the vertex-raise guard
    reads `b.tolerance()` and the gap, not the current vertex tolerance (`:372-393` [V]). The edge guard
    reads gap and size.
  - The update-mode reports can differ: today a vertex raised at edge use i is tested with its new
    tolerance at edge use j > i. Only `update_tolerances_after_boolean` passes `None` for reports, so the
    tolerance callers see no difference [V] (`wide_vertices.rs:24`).

## 3. What "prepared" means

Before the read phase, on the owner:

1. **Every UV trim exists** on every edge use of a face (removes W17, W18, W19). This is plan A8's routine,
   "create every missing UV trim".
   - On bodies from Booleans, area or volume reads, or records, the trims usually exist already (0 made by
     the check, measured).
   - Under invariant 11, preparing trims on the user's body is itself a change. P1 already solves this for
     trimless faces with a standing copy (W4). The clean option is the same: prepare on a standing copy
     when any trim is missing, and check that copy. Cost: one whole-body copy, only for those bodies.
2. **Every analytic edge curve has its NURB form** (W1, W21). Better: `fix_reversed` reads the form
   without storing it, on a temporary, because the form is only used to read control points. Then nothing
   needs preparing.
3. **A phase table of surface cache trees** for the curved faces the check touches (W6, W10, W20, W25):
   2 to 4 trees on the measured curved bodies, none for planes and cylinders.
4. **`BrepQuery` built once**, outside the items (W14).
5. **Nothing for gaps**: they become per-item values (C3).

Already present:
- trims on most bodies;
- NURB forms after the first check;
- no trees (each check builds its own and drops them: plan B4).

Cost of preparing:
- the trees the check builds anyway;
- a standing copy only for bodies with missing trims.

## 4. Caches and the phase-scoped table

The check builds trees through the slot on each face it classifies (W20), in `surface_assert_valid` on a
cloned surface with `BrepFaceServices::new` (one of report 09's 2 copies, `AV/seams.rs:123` **[A]**), and in
`face_contact` (`VAL/pointers.rs:1624`, the other candidate). It rebuilds them on every repeat check
(measured).

**A phase-scoped table with no key holds here**, more simply than for the Boolean:
- With W17 prepared and C1 moved out, nothing in P1 to P5 edits a surface, a face's trims or a tolerance.
  So every build input is constant for the check.
- The check is one phase from start to end: no apply step falls between reads.

Condition: C1 must go first. W2 edits UV trim control points, which feed the trimmed cache (`TrimSrfCache`)
**[I]**. Surface-only trees are unaffected.

Holding the table for the whole check is exactly plan B4. In a pure phase the table can also be lent to
`surface_assert_valid` and `face_contact` instead of copied (report 09's 2 copies go; plan B3).

## 5. Items

- **P1/P2**: per face (check_face, tests 4-44), per loop and edge use (46-56, 111), per edge
  (`check_edges`), per vertex (`check_vertices`, test 82 clearance). The clearance queries near the limit
  are the expensive ones; the rest is pointer comparisons.
- **P3**: per marked edge use pair. Values: three gaps and four test outcomes. Combination: max gaps over
  the body (`AV/brep.rs:436-449`), which is order-free for finite values (max, not a sum).
- **P4**: per region and shell. Few items. Rays dominate test 8 and test 12. The RNG makes test 12's answer
  depend on the draws. After A1 it is a function of the call; with per-item generators seeded from (call,
  shell) it is also independent of item order [I].
- **P5**: per face, loop, edge, vertex. Report 05 V2 says "runs for seconds, the edges' curve analyses
  most of it" **[A]**. Skew: curved edges (curve analysis, radial order binormals, test 13 edge form) and
  curved faces (point classification, `surface_assert_valid`) far outweigh planar ones.
  - Value per item: a `Vec` of reports plus a `bool`.
  - Combination: `rtn &=` with no short-circuit (`AV.rs:679-711` [V]), so the AND is order-free.

Report order (question F):
- `AssertArray::add_unique_delete_duplicate` dedups per report as it is added (`VAL.rs`, `Reports::record`
  [V]). `Rep::add` calls `set_last_add(false)` on pass (`AV.rs:679-711` [V]).
- Merging per-item report vectors sequentially in today's entity order gives the same array **[I]**,
  provided the dedup runs during the merge and not per item.
- The only early exits are a failed pointer status (P2 returns at once) and a failed pointer report
  (AssertValid skipped). Both happen before the items.

Consumers of order **[V]**:
- `BrepCheck::lines` (pointer first, then native).
- Tests assert line text, e.g. `local_operation.rs:2120`, `mirror.rs:765`.
- Ops `Body::check` re-implements the line format (OPS/check.rs `collect`, `report_line`) and
  `CheckReport::failed_tests` parses "Test #N".
- `failures_beyond` compares by class and does not depend on order.

Keeping entity order in the merge keeps all of them.

## 6. What stays sequential regardless

- The pointer preflight and topology walk in P1, up to the early exits: cheap, and order defines the
  first failure.
- Merging reports with dedup, in entity order.
- Update mode's apply: raises, then the bounding box.
- Any repair split out of C1.
- Preparing trims (trim creation is sequential in the Boolean study too).

## 7. Cost

PRs to make the check write-free on the common path [I]:

| # | PR | Result change |
|---|---|---|
| 1 | Check gaps as values: P3 in validate-only mode computes without reading or writing the memos (C3, W8, W9) | yes, only where a memo was stale (a fix; report 08) |
| 2 | `fix_reversed` in the check judges on a scratch copy and reports; the repair stays in `validate_geometry(fix_tolerances)` and healing (C1, W1) | yes, where it fires |
| 3 | Missing trims prepared on the standing copy before AssertValid, as P1 already does; walk callees get `ok_to_make = false` (C2, W17-W19) | possibly, on bodies with missing trims (the raise no longer happens) |
| 4-6 | `&Brep` signatures for the walk callees through the prepared view: `point_classify_indexed`, `evaluate_binormal`, `compute_uv_point` (`SectorBrep::Reading`), `is_sector_missing_seam`, `is_within_xsect_tol_3d_of_faces`, `has_seam_problem_in_brep_mut`, face 37/38 and edge 13 helpers, `vertex_ball_reaches`, `edge_tube_reaches`, `edge_curve_check` | no |
| 7 | Phase table for the check's trees (= B4), lent to `surface_assert_valid` and `face_contact` (B3) | no |
| 8 | `BrepQuery` built once per check outside the items; `find_infinite_region` on `&Brep` | no |
| 9 | `check_brep(&Kernel-per-worker, &Brep)` entry; `EdgeSets` and memos per worker | no |
| 10 | Ops: `Body::check` runs on the locked body by shared reference, with no `brep_copy` (question E) | no |
| 11 | Update mode as gathered raises (C4, C5) | tolerances: no if section 2.5 holds; reports in update mode: yes, but no caller reads them |

Total: about 9 to 11 PRs, two or three with result changes (1 and 2 certainly, 3 possibly).

Shared with the Boolean's five parts (07 section 6):

| Part | Shared? |
|---|---|
| Prepared view | the same (`QueryTopology::Prepared`, `point_classify_prepared`) |
| Phase table | the same, simpler (one phase) |
| Typed requests | only `RepairCurve` (C1, as a report rather than a request) and update mode's `RaiseVertex`/`RaiseEdge` |
| Fulfil loop | not needed in the check: nothing to re-run once prepared |
| Kernel per worker | the same |

Specific to the check: PRs 1, 2, 3 and 10.

**Question E.** The ops copy (P0) costs one `brep_copy` per check: new arenas and ids via `merge_brep`
(report 02 section 4.1 [A]). Whether it shares the surface slot is UNVERIFIED. It protects two things:
1. the body from the check's writes (W1, W2, W8, W17, W18, W20);
2. the body from a panic mid-edit.

Once the check takes `&Brep`, a panic cannot leave the body half-edited, and nothing is written. So the copy
can go: the body lock is held shared for the check. It also stops the check of a copy from disagreeing
with what the body's own memos say (report 08 section 3.1).

`operand_tolerances.rs:69-75` copies for a different reason (it tightens the trial), so its copy stays.

**Question C (report versus repair).** Yes, they separate. The report is a list of plain values:
- per-item reports `(test, class, owner, value)`;
- gaps per vertex use and edge use;
- the turn-back findings (C1);
- the missing trims found in prepare.

Repair consumes the gaps (update mode, `tightened`) and the turn-backs (heal).

Today's interleaving matters in three places:
- C1, where the repair hides the defect from later tests;
- update mode, where later reports see raised tolerances;
- C3, where memo order matters.

None of them is needed for a correct answer.

## 8. UNVERIFIED

| Item | What would settle it |
|---|---|
| How often W2 (`fix_reversed` moves a point) fires | Count calls where `end_turns_back` is true, over the goldens and the corpus, in P1 and P2 separately |
| What `trimming_tools::check_face` writes on a face with trims (W3) | Read it, or snapshot edge-use trims, intervals and tolerances before and after P1 on the measured bodies |
| Whether `evaluate_binormal`, face 37/38 or edge 13 helpers can create trims | A trim-creation counter around P5 on a body with one trim deleted per edge use |
| Whether W18 has ever raised a tolerance during a check | Count `set_uv_trim_curve` raises with `validate_only == false` inside `check_brep` over the goldens |
| Which of `seams.rs:123` and `pointers.rs:1624` supplies report 09's 2 copies | A copy counter tagged by call site (plan B1) |
| Where the kernel memos `curve_analysis`, `loop_area_memo`, `recertify_memo` are written | Read their writers; they are S either way |
| Whether gathered raises give identical tolerances in update mode (C4/C5) | Run `update_tolerances_after_boolean` both ways over the Boolean goldens and compare every tolerance |
| Goldens moved by PR 1 to PR 3 | Run the golden suite on each PR |
| Whether `brep_copy` shares the surface slot (affects E's cost) | Tree-build counter across `measure_body("check")` on a curved body |
| Cost split of P5 (curve analyses versus classification) | Per-phase timer on a curved real design |

## Verdict

Easier than the Boolean. The check has no proto topology, no imprint and no fulfil loop. Its writes are
accidental: memos, trees and NURBs are P, trims are prepared, and the three C items reformulate cleanly.
Two of them are defects against invariant 11. The work is mostly signature plumbing to `&Brep`, plus two or
three small result-changing PRs.
