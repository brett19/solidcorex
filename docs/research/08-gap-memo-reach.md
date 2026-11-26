# 08 — Can an operation reach a stale gap memo?

Source: a local worktree at 782ef9afc. Paths are relative to
`solidcore/crates/solidcore/src` unless they start with `solidcore-ops/` or `solidcore-convert/`.
Nothing was built or run. "Verified" means I read the code at the cited lines; "inferred" means
I read one side of the link, or the condition that makes it fire could not be established by
reading. Measured facts come from `m5-history-kernel.log` (earlier probes).

## Verdict in short

- **`edge_ccw_edge_gap`: not reachable.** Its only reader is the legacy healer
  (`heal/heal_data/p2.rs:472`, `:270`), which runs once, on a body just built from a record,
  whose memos are all unset. The missing reset is real but no operation reads the value.
- **`max_edge_face_gap`: reachable, deterministically stale after a scaling transform.**
  `Body::transform` with a scale leaves every cached edge/face gap at its pre-scale distance.
  Whether the answer that depends on it is wrong depends on the size of the gap: for analytic
  edges the measured gap is 0 or 3.6e-15, and a wrong factor on that changes nothing. A wrong
  tolerance needs an edge whose real gap is of the order of the tolerance (an approximated
  intersection curve, an imported body). I could not establish by reading that any body made
  from primitives has one. Probe 8 measures it.
- **`vertex_edge_gap`, `vertex_face_gap`: reachable in principle on three paths** (lazy trim
  recovery, an edge join's squeeze, a trim end moved in place), each bounded by a tolerance and
  each with a condition I could not show firing. No measured body shows any of them.
- **ops `Body::check` is immune**: it checks a `brep_copy`, and a copy's memos start unset. What
  a stale memo can change at ops level is a tolerance, through `after_boolean` and
  `matched_tolerances`.

## 1. The memos

All four are plain fields; "unset" is `is_init() == false`.

| Memo | Stored | Filled by | Reset by |
|---|---|---|---|
| `max_edge_face_gap` (edge use and its mate) | `topology.rs:606`, `Default` at `:624` | `edgeuse::p1::calc_max_edge_face_gap(cache = true)` `topology/edgeuse/p1.rs:972-1049`, only from `max_edge_face_gap` `:906-920`; `CurveOnClassifyCache::apply_gap_updates` `query/face/p2.rs:1596-1597` (copy from a snapshot) | `edgeuse::p2::notify` on `SplitInBrep`, `ChangeGeometry`, `Split` `topology/edgeuse/p2.rs:925-933`; fillet helpers `blend/fillet_geom/p4.rs:193-209`, `p5.rs:525-528`, `p1.rs:3976-3979`, `blend/fillet_corner/p1.rs:1045-1046`; `Edgeuse::default()` (every new record, every `brep_copy`: `topology/brep/p3.rs:2620-2625`) |
| `edge_ccw_edge_gap` (edge use) | `topology.rs:608`, `Default` at `:625` | `calc_edge_ccw_edge_gap(cache = true)` `edgeuse/p1.rs:1061-1120`, only from `edge_ccw_edge_gap` `:945-959` | `brep::p2::replace_curve` `topology/brep/p2.rs:2383` only; `Default`. **No notification resets it** (verified: `edgeuse/p2.rs:913-941` touches only `max_edge_face_gap`) |
| `vertex_edge_gap` (vertex use and its siblings on the edge) | `topology.rs:654`, `Default` at `:669` | `calc_vertex_edge_gap(cache = true)` `topology/vertexuse.rs:551-616`, from `vertex_edge_gap` `:687-705` | `vertexuse::notify` on `PostEdit`, `SplitInBrep`, `ChangeGeometry`, `Split` `vertexuse.rs:2096-2102`; `replace_curve` `brep/p2.rs:2387`; `boolean/merge/p1.rs:1143-1148`; `wide_vertices.rs:232-237`; fillet helpers (`fillet_geom/p1.rs:1946`, `:3983`, `p4.rs:181`, `:206`, `p5.rs:506`, `:532`, `fillet_corner/p1.rs:1052`, `p3.rs:551`); `Default` |
| `vertex_face_gap` (vertex use) | `topology.rs:656`, `Default` at `:670` | `calc_vertex_face_gap(cache = true)` `vertexuse.rs:630-677`, from `vertex_face_gap` `:715-733`; `apply_gap_updates` `query/face/p2.rs:1591-1592` | as `vertex_edge_gap`, plus `set_uv_trim_curve` `edgeuse/p1.rs:771-778` (start vertex use of the edge use and its mate), `init_vertex_face_gap` `vertexuse.rs:740` (callers in `topology/vertex/corner.rs:134,361,374`) |

How the notifications arrive (verified):
- `edge::notify` forwards `ChangeGeometry`, `PostEdit`, `SplitInBrep`, `MergeInBrep` to the edge
  uses and vertex uses (`topology/edge.rs:2697-2731`). The edge-use handler ignores `PostEdit`
  and `MergeInBrep`; the vertex-use handler ignores `MergeInBrep`.
- `vertex::notify` forwards `PostEdit`, `SplitInBrep`, `MergeInBrep`, `ChangeGeometry` to its
  vertex uses (`topology/vertex.rs:1341-1359`).
- `face::p3::notify` forwards `PostEdit`, `SplitInBrep`, `ChangeGeometry`, `Split`
  (`topology/face/p3.rs:513-558`; the edge-use walk is skipped on `PostEdit` without a listener).
- **The B-rep level `SplitInBrep` and `MergeInBrep` (`brep::p2::notify`, `topology/brep/p2.rs:827-831`)
  only move entries in the spatial tree.** They do not reach `edge::notify`. `split_edge`
  (`topology/brep/p9.rs:1437-1446`) and the edge join (`topology/brep/p5.rs:1524-1543`) notify at
  that level only, so a split or a join resets no memo.
- Arena clones (`Brep::clone_for_trial`, `snapshot`/`restore`, `topology.rs:1291`, `:1343`,
  `:1400`) keep the memos; `brep_copy` drops them.

Readers. "Trusts" = `force_calc = false` or a direct field read.

| Reader | Memo | Trusts? | Where |
|---|---|---|---|
| `validate_and_update_body` (the check's tolerance tests 0-4, and the tolerance update) | vertex/edge, vertex/face, edge/face | trusts | `topology/brep/validation/assert_valid/brep.rs:360-368`, `:452` |
| `EdgeProps::set_props`, `VertexProps::set_props`, `fix_tol_sizes` (legacy healer) | all four | trusts | `heal/heal_data/p2.rs:472-473`, `:720`, `:735`, `:270-285`, `:3124-3125`, `:3207-3208` |
| `edge::max_up_dim_gap_3d`, `max_down_dim_gap_3d` (from `tol::update_object_tolerance`, `geom/tol.rs:1883`) | edge/face, vertex/edge | trusts | `topology/edge.rs:1492`, `:1530-1531` |
| `vertex::max_gap_3d` (`geom/tol.rs:1818`) | vertex/edge, vertex/face | trusts | `topology/vertex.rs:1191-1195`, `:1231-1234` |
| trim creation, third attempt and deviation bound | edge/face | trusts | `topology/edgeuse/p3.rs:434-437`, `:666`, `:2790-2791` |
| classification | edge/face, vertex/face | trusts | `query/face/p2.rs:1615`, `:1698`; `topology/face/p4.rs:2407-2411`; `sweep/topology_sweep/store/classification.rs:131` |
| corner tests | vertex/face | trusts | `topology/vertex/corner.rs:143`, `:246`, `:255`; `topology/vertexuse.rs:1943`; `topology/face/p1.rs:1949`; `topology/face/p2.rs:3352` |
| `edge_form` | edge/face | trusts | `topology/edge/edge_form.rs:60` |
| fillet session | edge/face | trusts | `blend/fillet_executive/session.rs:7272` |
| `centre_vertex`, `largest_gap` | vertex/edge, vertex/face | recomputes | `topology/brep/validation/wide_vertices.rs:86`, `:133-136` |
| boolean vertex relocation | vertex/edge, vertex/face | recomputes | `boolean/merge/p1.rs:981`, `:1133` |

`edge_ccw_edge_gap` has no reader outside `heal/heal_data/p2.rs` (verified by grep over the crate).

## 2. The edits

| Edit | Where | Notification / reset | Stale after it |
|---|---|---|---|
| `edge::set_interval` itself | `topology/edge.rs:394-397` | none | whatever the caller does not reset |
| Lazy trim creation, "edge trim recovery": the edge's interval is cut to the dropped UV curve's | `topology/edgeuse/p3.rs:1010`, `:1034` | none. The trim attached afterwards resets `vertex_face_gap` of two vertex uses (`edgeuse/p1.rs:771-778`) | `vertex_edge_gap` at the moved end, `max_edge_face_gap` of every use of the edge, `edge_ccw_edge_gap`. The move is accepted only when the vertex is within `edge tol + vertex tol` of the drop's end (`edgeuse/p3.rs:962-990`, `geom/tol.rs:428-430`) |
| `update_vertex_edge_intervals`: edge end moved to the drop of the vertex | `topology/brep/p1.rs:2085`, `:2112` | `PreEdit` only | `max_edge_face_gap`, `edge_ccw_edge_gap`. The vertex memos were reset just before in all three callers by `SetPoint` (`topology/brep/p5.rs:477-485`; `boolean/merge/p4/arena_pair.rs:1377`, `:1398`; fillet intersector `:201-207`: caller order inferred for the last two) |
| Edge split | `topology/brep/p9.rs:1247-1248`, `:1306-1329` | B-rep level `SplitInBrep` only | The kept edge uses keep `max_edge_face_gap` measured over the whole edge (an over-estimate) and `edge_ccw_edge_gap` of the old neighbour. The old end vertex use moves to the new edge with its memo (`:1392-1396`), whose value still holds |
| Edge join, same-geometry branch (`DeleteTopologicalVertex`) | `topology/brep/p5.rs:907` | B-rep level `MergeInBrep` only (`:968-974`) | `max_edge_face_gap` of the survivor (measured over the shorter interval), `edge_ccw_edge_gap` |
| `squeeze_edge`: the deleted vertex's uses move to the survivor vertex | `topology/brep/p3.rs:4455-4470` | B-rep level `MergeInBrep` only | `vertex_edge_gap`, `vertex_face_gap` of every moved use (they measure the deleted vertex's point). Callers that then call `SetPoint` on the survivor reset them (`brep/p5.rs:477`); the join does not |
| Edge join, other branch; `replace_3d_curve`; `set_curve`; offset `set_edge_curve`; `give_edge_curve` | `brep/p5.rs:950-963`; `topology/edge.rs:2858-2879`, `:2899-2924`; `offset/offset_executive/step_gaps.rs:1635-1657`; `topology/brep/p2.rs:3076-3092` | `ChangeGeometry` through `edge::notify` before the curve is replaced | only `edge_ccw_edge_gap` (and `replace_curve` resets that too, `brep/p2.rs:2383`). The interval written after the curve is safe unless something reads a gap between the two statements (none seen) |
| Fillet `set_edge_curve` and its callers | `blend/fillet_geom/p4.rs:193-209`, `:1234-1235` | direct resets | only `edge_ccw_edge_gap` |
| Fillet removal, "curve was long enough: just update the edge's domain" | `blend/fillet_removal.rs:1252-1253`, `:1346`; `blend/fillet_removal/ring.rs:883` | none | `max_edge_face_gap`, `edge_ccw_edge_gap`; vertex memos until the later `SetPoint` (`fillet_removal.rs:1600`; order inferred) |
| `reverse_orientation` | `topology/edge.rs:2360-2370` | none | none in value (same points) |
| `vertex::set_point` | `topology/vertex.rs:1374-1396` | `ChangeGeometry` to the vertex's uses | none |
| Direct vertex point writes | `validation/wide_vertices.rs:106`, `:226`; `boolean/merge/p1.rs:947-978`; `topology/vertex/corner.rs:366-374` | each recomputes with force or resets (read: `wide_vertices.rs:86-137`, `:226-237`; `merge/p1.rs:951`, `:971`) | none |
| **Scaling transform**: edge curves and face surfaces transformed in place | `topology/brep/p5.rs:3603-3668` | `PreEdit` on each edge and face; vertices go through `SetPoint` (`:3585-3601`, `:615-640`) | **`max_edge_face_gap` and `edge_ccw_edge_gap` of every edge use**, by the scale factor. Vertex memos are reset. A rigid move leaves the values right up to rounding |
| `set_uv_trim_curve` (set, delete, re-trim through it) | `topology/edgeuse/p1.rs:715-800` | resets `vertex_face_gap` on two vertex uses | none found |
| Trims edited in place: `trim_other_uv_curves`, `fix_uv_trim_curves_notified`; direct `uv_trim_curve = None` in the join | `topology/edgeuse/p3.rs:1184-1214`; `topology/face/p2.rs` (called at `face/p3.rs:1437`); `topology/brep/p5.rs:964-967` | none | `vertex_face_gap` where its UV point comes from the trim's end (inferred, not traced into `compute_uv_point`) |
| Surface replaced | `replace_surface` (`brep/p2.rs:3408-3418`, `brep/p5.rs:3655-3664`) | `ChangeGeometry` through `face::notify` (inferred from `face/p3.rs:513`; `replace_surface` body not read) | none expected |
| Tolerance setters | `edge::set_tolerance`, `vertex::set_tolerance` | none | none: the four gaps are distances. (`calc_max_edge_face_gap` samples with the context's tolerance, not the edge's, `edgeuse/p1.rs:1002`) |

## 3. The reach

### 3.1 Who fills and who reads, on a real body

- **Kernel booleans leave no memo.** Measured: the result of `boolean_difference` has every
  edge/face and vertex memo unset (`m5-history-kernel.log:80`, `before=-1.23456e-61`).
- **`solidcore-ops` fills them after every boolean.** `boolean_with` runs `after_boolean` on the
  result (`solidcore-ops/src/boolean.rs:161-163`, `:77-79`), which is
  `update_tolerances_after_boolean` (`validation/wide_vertices.rs:22-25`) →
  `validate_and_update_tolerances(false, Level2)` → `validate_and_update_body`, which asks for
  every vertex/edge, vertex/face and edge/face gap with `force_calc = false`
  (`assert_valid/brep.rs:360-363`, `:452`). The API offset does the same
  (`offset/offset_executive/p2.rs:2105`, `:2121-2125`). All links verified.
- **`check_brep` fills them too**, on the body it is given (`assert_valid/brep.rs:69` →
  `:238-240`). But ops `Body::check` checks a `brep_copy` (`solidcore-ops/src/check.rs:117-122`,
  `solidcore-ops/src/session.rs:766-779`), so the user's body gets no memo from a check, and the
  check never reads the body's memos. Verified.
- **A boolean keeps operand 1's records.** `merge_boolean(k, brep1, brep2, ..)` works in
  `brep1` (`boolean/api_brep.rs:654-662`); operand 2 comes in as new records (inferred from
  `brep/p3.rs:2620`; I did not trace `merge_boolean`). So the memos an ops boolean left on a body
  are still on its surviving edge and vertex uses when the next boolean's `after_boolean` reads
  them.

### 3.2 Scaling transform (the one deterministic case)

Chain, every link verified by reading:
1. `Body::transform` → `transform_matrix(k, b, matrix, true)` in place
   (`solidcore-ops/src/body.rs:341-349`) → `transform_with_notifications`
   (`topology/brep/mirror.rs:452-470`) → `transform_in` (`topology/brep/p5.rs:3568-3727`).
2. Vertices: `SetPoint` → `ChangeGeometry` → vertex memos reset. Curves and surfaces:
   `PreEdit`, then `transform` in place. `max_edge_face_gap` is untouched.
3. Tolerances: each becomes `max(zone of the new size, old × scale)` (`p5.rs:3690-3725`).
4. Next reader on the same body: `after_boolean` of the next ops boolean
   (`assert_valid/brep.rs:452-468`), or `matched_tolerances` → `tightened(coarse)`
   (`solidcore-ops/src/boolean.rs:161`, `boolean/operand_tolerances.rs:57-110` (`tightened` at `:88`)).

What the stale value does:
- Scale up by `s`: the memo says `g`, the gap is `s·g`. The scaled tolerance still covers it
  (`s·tol ≥ s·g`), so nothing is wrong until tolerances are shrunk and regrown from the memos.
  `tightened` does exactly that: every edge tolerance over `need` is set to `need`, then
  grown to `2 × memo` where the memo exceeds it. An edge whose real gap `s·g` is over `need`
  while `g` is not ends below its gap. The guard does not catch it: the trial is a `brep_copy`
  (`operand_tolerances.rs:69-75`), whose memos are fresh, so the trial passes on correct
  tolerances and the operand is then tightened from stale ones (`:76-78`).
- Scale down by `s`: the memo is `1/s` too large. `after_boolean` grows an edge to `2g` where
  `g` exceeds the scaled tolerance: a tolerance larger than the geometry needs. The kernel's own
  `check_brep` on that body would report test 4 on a good edge; ops never calls it on the body.

Missing link: a gap large enough. Measured edge/face gaps on `ball_less_corner`, `bar_into_post`
and `plate_with_hole` are 0.0 or 3.55e-15 (`m5-history-kernel.log:80,111,142`). With `g` at
1e-15 no scale changes a decision. The case needs an edge whose curve is an approximation of the
intersection (gap near the approximation tolerance) or an imported body with real gaps. Whether
crossed cylinders or an off-centre drilled ball give such edges in this kernel I could not tell
by reading; probe 8 builds both and prints the gaps.

### 3.3 Lazy trim creation changes an interval

Chain: `check_brep` → `brep()` → `tolerances()` fills the vertex/edge memos
(`assert_valid/brep.rs:69`) → later in the same call the walk makes missing trims
(`:225`; report 02 §1.5) → `create_uv_trim_curve` may cut the edge's interval
(`edgeuse/p3.rs:1010`, `:1034`) → the next reader trusts the memo. The same order holds inside
one tolerance update followed by any read that makes trims (mass properties, classification).

- Size: the new end is within `edge tol + vertex tol` of the vertex, and the check's limit is
  half the vertex tolerance (`assert_valid/brep.rs:309`), so the real gap can be over the limit
  while the memo says 0: the check would pass a body it should report, and the update would
  leave the vertex tolerance under twice its gap.
- Missing link: the recovery firing. It needs the dropped UV curve to come back shorter than the
  edge (`edgeuse/p3.rs:967-990`). No test names the path
  (`CreateUvTrimCurvePathType::OnePercentEdgeTrimRecovery1/2` appears only at its definition and
  these two writes), and in every measured probe no read changed any edge interval (no
  `edge_interval` difference anywhere in `m5-history-kernel.log`). Not shown reachable.
- The face-level routine (`topology/face/p3.rs:1295-1470`) reaches the same two writes through
  `create_uv_trim_curve`; I did not find a further interval write in it (not read line by line).

### 3.4 Boolean imprint, splits and joins

- Split (`brep/p9.rs:1147-1452`): no reset. Kept edge uses hold an over-estimate of the
  edge/face gap, which only matters if their tolerance later shrinks below it. Harmless as read.
- Join in the boolean's cleanup (`boolean/merge/p1.rs:821-828` →
  `brep/p5.rs:769-989`): in the same-geometry branch the survivor's interval grows by the
  deleted edge's stretch and the deleted vertex's use on the survivor moves to the far vertex
  with its memos (`brep/p3.rs:4455-4470`). If those memos were filled between the split that
  made the vertex and the join, `after_boolean` reads the deleted vertex's gaps for the far
  vertex. Fillers in that window exist for `vertex_face_gap` (classification and its
  `apply_gap_updates`), and I found none for `vertex_edge_gap`. The join accepts a far vertex
  within the largest of three tolerances (`brep/p5.rs:817-821`, `:887-893`), so the hidden gap
  is bounded by that. `cover_joined_line_gaps` (`brep/p5.rs:1015-1090`) recomputes directly, for
  lines only. Inferred chain; no evidence it fires.
- `update_vertex_edge_intervals` after a vertex merge: vertex memos already reset; the
  edge/face memo is off by a move no larger than the vertex tolerance. Harmless as read.

### 3.5 Fillet

Fillet geometry edits reset the three memos directly (table in §1). Fillet removal's bare
interval writes (§2) leave the edge/face memo over a shorter interval; the fillet session reads
it with `force_calc = false` (`blend/fillet_executive/session.rs:7272`). I did not trace whether
removal and that read meet on one body in one operation.

### 3.6 Heal and stitch

- The legacy healer (`HealData`) is the only reader of `edge_ccw_edge_gap` and the only pass
  that may shrink a tolerance from all four memos (`heal/heal_data/p2.rs:3174`, `:3240`). It is
  called from `make_topology_from_data` and `heal_made` only (`topology/brep/p10.rs:527`,
  `:552`), and in the product from record conversion of an untrusted record
  (`solidcore-convert/src/brep.rs:308`), on a body built a few lines above. Its memos are unset
  when the healer first asks, and each edge's and vertex's props are set once per run
  (`heal/heal_data/p2.rs:2793-2806`). Ops healing does not use it
  (`solidcore-ops/src/heal.rs:17-21`). Verified for the callers; the "once per run" claim is
  from the caching loop, not from reading every later step.
- The stitch scope of the check reads the same memos as §3.1 (`assert_valid/brep.rs:407-426`).
  Stitch edits were not traced.

### 3.7 The validity check

Fills at Level 2 as in §3.1, then reads nothing else from the memos. A stale small vertex gap
makes tests 0-3 pass a gap over half the vertex tolerance; a stale small edge/face gap makes
test 4 pass an edge off its face by more than its tolerance; stale large values do the reverse.
At kernel level this is reachable by `check_brep` → scale → `check_brep` on one body. At ops
level the check is of a fresh copy and is always right.

### 3.8 Tolerance passes that read gaps

| Pass | Shrinks? | Reads | Reached from |
|---|---|---|---|
| `update_tolerances_after_boolean` | no | three memos, trusting | every ops boolean result; `tightened` |
| `tightened` (`matched_tolerances`) | **yes, then regrows from the memos** | same | ops booleans whose operands' tolerances differ enough (`solidcore-ops/src/boolean.rs:161`, `:205`, `:293`) |
| offset `update_result_tolerances` | no | same | API offset result (a new body: memos are those filled during the offset) |
| `HealData::fix_tol_sizes` | yes | four memos | record conversion only, fresh body |
| `tol::update_object_tolerance(check_gaps)` | no | vertex and edge maxima, trusting | callers not traced |

`tightened` is the damaging one, and §3.2 is the only path I can show delivering it a memo that
is wrong by more than a tolerance.

## 4. Reproduction

Added `probe_8_stale_gap_reached_by_an_operation` (and helpers) at the end of
a local worktree.
Not built, not run.

For each of four bodies (`ball_less_corner`, `bar_into_post`, new `crossed_cylinders`, new
`ball_drilled_off_centre`) and each scale (1000, 0.001, 0.0001) it makes three identical bodies:
build, `update_tolerances_after_boolean` (what ops runs after a boolean),
`transform_matrix(.., true)`. Body 2 then has every memo cleared (the fix, emulated). It prints:

| Line | Compares | Expectation |
|---|---|---|
| `…/setup` | counts, largest held and forced edge/face and vertex/edge gap, body tolerance | the magnitudes that decide everything below |
| `…/gap` | each memo the scaled body holds against its forced recalculation | DIFFER wherever the gap is not exactly 0 (ratio = the scale); vertex memos absent (reset by the transform) |
| `…/check_brep_of_the_body` | kernel check of body 1 against body 2 | SAME for analytic bodies; DIFFER at 0.0001 if a held gap exceeds 1e-7 |
| `…/tolerance_update_outputs`, `…/tolerances_after_update` | the next `after_boolean` | as above |
| `…/tolerances_after_tightening` | `tightened` emulated at a hundredth of the body tolerance | DIFFER at 1000 if a real gap exceeds that zone while the held one does not |
| `…/check_of_a_copy_after_tightening` | the check as ops runs it, on a `brep_copy` of each | **a DIFFER here is the user-visible wrong answer**: an operand left with an edge tolerance under its gap |

Compile risks (each checked against the source, none compiled):
- `solidcore::topology::brep::validation::update_tolerances_after_boolean` (re-export at
  `validation.rs:40`), `solidcore::topology::brep::p3::brep_copy` (`p3.rs:1849`),
  `solidcore::topology::face::p3::set_tolerance` (`face/p3.rs:422`), `Brep::set_tolerance`
  (`topology.rs:1712`), `Brep::edgeuse_mut` / `vertexuse_mut` (`topology.rs:1678`, `:1685`),
  `Arena::iter` / `ids` (`topology.rs:307`, `:316`), `is_init` / `reset` on the gap types
  (`heal/gap.rs`).
- `ToleranceGaps` is formatted with `{:?}` (it derives `Debug`).
- The array of `(&str, Build)` relies on fn items coercing to the `Build` pointer type, as the
  file's other probes do.
- `Arena::iter` yields `(I, &T)`; the `filter(|(_, eu)| ..)` closures rely on auto-deref.
- Runtime, not compile: the two new booleans may fail (the probe prints SKIPPED), and
  `TURN_Z_TO_X` must pass `transform_matrix`'s low-work test (rows orthonormal, right-handed:
  checked by hand).

What I could not construct: a reproduction of §3.3 (no public way to make a dropped UV curve
come back short) or §3.4 (needs a memo filled between a split and a join inside one boolean).

## 5. Verdict and fix

| Memo | Reachable stale? | Wrong answer? |
|---|---|---|
| `edge_ccw_edge_gap` | no reader in any operation | no |
| `max_edge_face_gap` | yes: scaling transform (verified chain) | only with a real gap near the tolerance: under-set edge tolerance through `tightened`, over-set through `after_boolean`. Unproven for bodies built from primitives; probe 8 decides |
| `vertex_edge_gap` | only through the trim recovery or a join (conditions not shown) | bounded by `edge tol + vertex tol`; not shown |
| `vertex_face_gap` | join's squeeze, trims edited in place (inferred) | not shown |

By the project's rule, this is not yet a fix to take: no real use is shown reaching a wrong
answer. If probe 8 prints a DIFFER on `check_of_a_copy_after_tightening` (or on
`tolerances_after_tightening`), the scale case qualifies.

Smallest correct fix, in order of evidence:
1. **`transform_in`** (`topology/brep/p5.rs:3568`): when a scale other than 1 is applied, reset
   `max_edge_face_gap` and `edge_ccw_edge_gap` on every edge use (or multiply them by a uniform
   scale). This closes the only verified chain.
2. **`edge::set_interval`** (`topology/edge.rs:394`): reset `max_edge_face_gap` and
   `edge_ccw_edge_gap` on the edge's uses, `edge_ccw_edge_gap` on each use's clockwise
   neighbour, and `vertex_edge_gap` on the edge's vertex uses. This covers the trim recovery
   (`edgeuse/p3.rs:1010`, `:1034`), `update_vertex_edge_intervals` (`brep/p1.rs:2085`, `:2112`)
   and fillet removal (`fillet_removal.rs:1253`, `:1346`, `:1453-1454`, `ring.rs:883`, `:933-935`).
3. The direct `Edge::interval` writes that bypass the setter need the same reset: the join
   (`brep/p5.rs:907`, `:963`) and the split (`brep/p9.rs:1247-1248`, `:1306-1329`). Routing them
   through `set_interval` is the simpler form.
4. **`squeeze_edge`** (`brep/p3.rs:4455-4470`): reset both vertex memos on each vertex use it
   moves to the survivor.
5. `edgeuse::p2::notify` (`edgeuse/p2.rs:925-933`): reset `edge_ccw_edge_gap` beside
   `max_edge_face_gap`. Tidy only: nothing reads it.

Items 2-4 are defensible as one change ("an interval write clears the gaps it measures") but
have no reproduction; item 1 has a deterministic stale value and a test waiting to be run.
