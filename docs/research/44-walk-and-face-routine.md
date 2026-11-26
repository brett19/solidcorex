# 44: the containment walk (L3) and the face routine (L2)

Date: 2026-11-25. Base: `cache-leaf-split-2` at `53a7f14856`. Nothing claimed, pushed to GitHub or the fork, or filed.

Labels: MEASURED (scratch logs over the goldens and ops tests), READ (code read at the base), INFERRED (drawn from those two).

## Assessment first

- **L3 can be lifted order-exactly, and it has been, in scratch.** The containment walk and the corner walk now
  read `&Brep`. They stop at the first edge use whose trim is missing (`Walked::NeedsTrim(eu)`). A `&mut`
  driver makes that one trim with main's call and feeds the result back. The walk then goes on from where
  it stopped, keeping every value it had read.
  - Changed rows: 0 of 283 units, forwards and `--reverse` (MEASURED).
  - The trim-build count is 475,734, identical to main. The ordered C/G sequence is byte-identical in every
    unit log, forwards and reverse, and in all 8 ops test binaries (327,235 builds each side) (MEASURED).
  - No work is added: no restart, no second walk (READ).
- **L2 cannot be lifted order-exactly as a split of the same shape.** The face routine is not "make the
  missing pieces, then read". On faces whose trims are all present it rewrites existing trims and raises
  tolerances.
  - Golden tree builds: 27,021 found every trim present. In 393 of them the pass still changed something of
    value: 352 rewrote a trim, others raised an edge or vertex tolerance (MEASURED).
  - No cheap test on the body says whether the pass is needed (MEASURED here and in report 30).
- **So the face routine does force the choice in the brief.**
  - (i) A results-changing "prepare the face" step that read entries require.
  - (ii) Leave tree builds as edits.
  - (iii) A third way that keeps results: an explicit `&mut` face pass the caller runs, recorded by a mark
    keyed on the write stamps (report 30's B then A). Its "is the pass done?" test is a stamp comparison,
    not a body property. It needs the bracket made stamp-neutral first, which nothing yet does: today every
    tree build moves the stamps (31,428 of 31,428 builds, MEASURED).

  Until (iii) is built, tree builds stay edits. The L3 split removes the trim-making from every read path
  that does not also build a tree.

## 1. What L3 does (READ, then MEASURED)

### Order of reads and writes in `contains_uv_point_with_topology` (query/loop.rs, base 336-824)

1. **Prelude: read only.**
   - Reads the loop use, its edge uses and the face.
   - Takes the surface: the caller's, or `clone_for_evaluation` of the face's.
   - Captures `zone_tol` from the face tolerance, then the natural domain, `point_3d`, `tol_uv` and the
     point's boundary sides.
2. **Corner walk (only when the point is on a natural side).** For every edge use in loop order: ensure the
   trim (create it if missing), then read its natural-interval ends; if that fails, drop the 3D edge ends.
   - No early exit.
   - The last corner's reads come after every corner's creation.
3. **Poles.** The poles, then a possible `common_poles` exit (OnBoundary).
4. **Last edge use.** `last_end_param` is read from the last edge's interval *before* `ensure(last_eu)`.
   Then that trim is evaluated.
5. **Each edge use i.** The steps for edge use i are:
   - an on-edge-curve drop that reads edge i's interval and tolerance *before* ensuring its trim, and exits
     with OnBoundary;
   - `ensure(eu_i)`, where false means exit Unknown;
   - an interval re-read;
   - stepping, which can exit on the trim or with Unknown;
   - the `last_end_*` values carried on to edge use i+1.
6. **Verdict** from the quadrant count.

Exits depend on the trims of edge uses before i (the on-trim solves, the quadrant count), so how far a walk
goes is not computable without trims.

### Measured per walk

Mutable walks that start with at least one trim missing, over all 283 golden units:

| | MEASURED |
|---|---|
| Walks | 5,040, in 47 units |
| Edge uses in their loops | 20,595 (mostly 4; up to 20+) |
| Missing at start | 15,048 |
| Trims the walks made | 13,480 (corner 3,560, last 2,703, main 7,217). None failed |
| Walks exiting before the last edge use | 2,081, all OnBoundary (on-edge or on-trim exits) |
| Walks exiting with trims still missing | 870 (1,692 trims never made) |
| Walks making nothing at all | 685, all OnBoundary before the first creation |
| Results | Inside 2,077, OnBoundary 2,081, Outside 882 |

So an "ensure the whole loop" step would make up to 1,692 trims that main never makes. Most of those trims
are made later by something else, but not all of them, and not in main's order.

What the walk's own creations changed (diffs taken before and after each creation, over the loop's edges,
their vertices and the face):

| Change | Count | Matters for |
|---|---|---|
| Own edge tolerance raised | 254 (corner 178, last 29, main 47) | on-edge test of the same edge read the old tolerance (main does too) |
| Own edge interval changed | 18 (corner 14, main 4) | as above, plus the interval the walk re-reads after creation |
| On-edge test read a value its own creation then changed | 51 walks, 8 units | order-exact: kept by construction; variant (b): moves the test |
| `last_end_param` read before its interval changed | 0 | |
| Face tolerance (`zone_tol`) changed mid-walk | 0 | |
| An earlier edge use's trim rewritten by a later creation | corner 9, last 20 | same in both orders |
| A later edge use's trim or edge changed (`trim_other_uv_curves`, mates) | 37 trims; 14 creations change a later read | variant (b) reorders these |
| Vertex tolerance raised | 164 creations | read by `point_classify`'s vertex test *before* the walks |

Ops tests (8 binaries, one thread each): 327,235 trim builds, the same in base and prototype (MEASURED).
`solidcore_ops`' `hlr::...ball_corners_as_spheres` fails in both base and prototype, so it was already
failing before this change.

## 2. Candidate splits

### (a) Order-exact: the resumable walk (built, MEASURED neutral)

The exit cannot be computed without trims, so the walk is a resumable state machine. It has three
stages: Corners, LastEnd{end_read}, and Edgeuse{i, edge_tested}. It keeps every value it has read, so a
resume never re-reads anything a creation could have moved:
- `last_end_param` is kept across the stop at `last_eu`;
- edge i's on-edge test is kept across the stop at `eu_i`.

`resume(k, &Brep, preparation, answer)` returns `Done(UvContainment)` or `NeedsTrim(eu)`, and stops only
when a trim is missing. A trim that is present is resolved in place by `trim_state`, which is the same
test `GetOrCreateUVTrimCurve`'s early return uses (READ, edgeuse/p3.rs `existing_uv_trim_curve`).

The corner walk splits the same way, one edge use at a time.

The one driver, `contains_uv_point_with_topology`, answers `NeedsTrim` per topology:

| Topology | Answer |
|---|---|
| Mutable | `get_or_create_uv_trim_curve(.., false, false, true)`, unchanged |
| Prepared | the Unported refusal, as before |
| Requested | `request_trim` plus `PreparationRequired`, as before (not extended) |

Commits on local branch `cache-leaf3-walk`:
- `98c4e08c79`: the split;
- `133e32db64`: reads the last edge where the lazy walk did, so the error order is exact for broken topology.

- `2c7fe37f57`: the one-step `ensure_trim_curve` is now `#[cfg(test)]`, since only the tests call it.

`cargo clippy --profile goldens -p solidcore --all-targets -- -D warnings` is clean (MEASURED).

### (b) Ensure the whole loop or face, then read (scratch, MEASURED, not blessed)

| Variant | Changed rows | Units |
|---|---|---|
| Loop first (`cache-leaf3-vl`) | 23 | api_primitives 4, brep_p4 11, brep_p6 7, brep_p10 1 |
| Face first (`cache-leaf3-vf`) | 57 | the same four (4 / 43 / 7 / 2), plus corpus.parity 1 |

Example: `brep_p10 gen0137` changes topology counts (n 3368 → 3416). `cyl_box_*` in api_primitives also
changes. (b) is results-changing.

## 3. L2: the face routine (READ, MEASURED where stated)

### What `face_cache_tree` → `build_tree_with_face` → `implant_face` does, in order (READ)

1. **Toggle.** Editing is turned on and restored on every exit (`implant_face`).
2. **Face pass.** `create_uv_trim_curves(adjust_tolerances = true)` (topology/face/p3.rs:1249-1480):
   - clears the surface owner and pushes `surface_owner_cleared`;
   - **missing trims**: `get_or_create_uv_trim_curve(.., true)` for each edge use without one, which may
     shorten the edge interval and raise edge and vertex tolerance;
   - **gap fix**: per adjacent edge-use pair off the poles, `fix_uv_trim_curves_notified` moves trim end
     points of trims that already exist;
   - **vertex tolerance**: raised to twice the 3D gap;
   - restores the owner, then `sync_face_surface_tolerance`.
3. **Tolerance sync.** The owner zone tolerance is refreshed from the face tolerance.
4. **Implant.** Reads trims, edge-use tolerances, `compute_uv_point` per vertex use (a global point solve)
   and vertex tolerances.
5. **Face containment.** `mark_face_containment` calls `BrepCacheFace::point_classify`. That is the
   mutable classify, so L3 again, nested.

### What the tree needs

- **The trims (with the gap fix applied) and the tolerances after the pass.** The tree is built from them.
  - Report 30: the pass-1 tree equals the pass-2 tree in 5,288 of 5,288 builds, and is built on the fixed
    trims.
  - In the goldens the gap fix fired 4,199 times out of 7,056 checks, and the vertex raise 178 times
    (MEASURED; this includes the face routine's other callers).
- **The editing toggle and the owner clear.** They are only scope state, restored by value. Report 30: no
  value ever changed. They could become arguments.
- **The tolerance sync.** It is needed only when the pass raised something.

### Is there a cheap "the face's pass has run" test? No body property (MEASURED)

| Golden tree builds (BrepCacheFace path) | 31,428 |
|---|---|
| All trims present at entry | 27,021 |
| ... of which the pass still changed a value | 393 (352 trim rewrites; edge tolerance, vertex tolerance and interval changes) |
| Some trim missing | 4,407, all of which changed something |
| Write stamps moved | 31,428 of 31,428, including the no-op builds (the bracket) |
| Face tolerance changed | 0 |

"All trims present" is wrong as a test in 393 builds, so it is not the test.

### The mark design and its clearing sites

**The mark** is per face: "the face pass has run, and nothing it reads has been written since".

**What clears it:**
- a write to any input: the face's edge-use trims, its edges' interval, tolerance and curve, its vertices'
  tolerance and point, its vertex uses' uv, its surface, its loops' topology;
- for shared edges and vertices, the marks of every face using them.

**Non-test write sites in solidcore (READ, regex count; a lower bound):**

| Write | Sites | Files |
|---|---|---|
| `set_uv_trim_curve(` calls | 37 | 23 |
| `edge::set_interval` calls | 54 | 22 |
| `edge::set_tolerance` calls | 38 | 27 |
| `vertex::set_tolerance` calls | 40 | 26 |
| `vertex::set_point` calls | 19 | 14 |
| `edge_mut(..).interval =` / `.tolerance =` | 2 / 6 | |
| `vertex_mut(..).tolerance =` / `.point =` | 6 / 17 | |
| `face_mut(..).surface =`, `set_surface(` | 7 + 7 | |
| `edgeuse_mut(..).uv_trim* =` | 12 | 9 |

On top of those:
- writes made after taking the record into a local (`let e = b.edge_mut(..)?; e.x = ..`): report 30 counted
  about 600 raw-field writes;
- topology edits.

Putting the clear inside the five setters covers 188 call sites through 5 definitions. About 70 direct
field writes and the topology edits would still need clears by hand. (INFERRED) A hand-placed mark is
fragile.

**The sound version is report 30's**, built in two steps:
- (B) make the bracket (owner clear, editing toggle, `surface_owner_cleared`, the tolerance sync) leave the
  stamps alone when nothing changes;
- (A) take as the mark the write stamps right after a face's pass, held by the caller.

That gives one explicit `&mut` operation, `face_pass(k, &mut Brep, f) -> PassMark`, and a read-only
`build_tree(&Brep, f, &PassMark)` that refuses when the stamps differ. Report 30 measured that the
read-only tree equals main's when the pass is a no-op.

The cost is zero clearing sites. The stamps are per arena, though, so any write anywhere voids every
face's mark: coarse, but sound.

## 4. Read entry points after this (READ; entry list and leaves from report 42)

T = a trim-making leaf. C = the tree (L2).

| Entry point | Trim-making leaves after L1 + `normalized_evaluate` (report 43) + L3 (this) | What it still needs |
|---|---|---|
| tangent_chain, tangent_face_chain_ordered, carried_faces, face_plane_normal | all T lifted | binormal's C (`note_face_surface_cache`) in the type |
| face_normal_at_point, point_on_face | L3 T lifted | L2 tree (`classify_by_trim_cache`, own cache → `face_cache_tree`) |
| classify_points_with_normals | L3 T lifted | L2 tree |
| face_line_hits, face_line_nearest | L3 T lifted | FaceLineCache (C), UvLineObjects (T, face/p2.rs:481) |
| edge_faces_along, edges_share_wedge, face_normal_at_edge, continuity_report | binormal T lifted | C |
| face_section | `normalized_evaluate` T lifted | BrepFaceServices (L4) + C |
| edges_lie_on_face | not lifted | `create_uv_trim_curves` (L2 face routine), FaceClassify, BrepFaceServices |
| to_record | not lifted | L6 `create_missing_uv_trim_curves` |
| volume, check_solid | not lifted | mass trims (report 15 E1); check also has F and G |
| ray, distance and lie-on group, project_curves, closest_to, ... | not touched | L4 LiveBrepObjects (C, stamps) |
| check | no T | F (`set_validate_only`), G gaps |
| inside_points | (L3 lifted via classify) | `curve_on_classify` snapshot path |
| first_side | none | G `compute_uv_point` → `global_point_solve` (C) |

With L3 lifted, every remaining blocker for the classify family is the tree (L2). No read entry point
becomes fully `&Brep` from L3 alone. The 4 binormal-only entries were already T-clean after report 43.

## 5. Evidence and scratch

**Local branches** (worktree `sc-wt/cache-leaf3-walk`):
- `cache-leaf3-walk` (the split);
- `cache-leaf3-vl` and `cache-leaf3-vf` (variant b);
- `cache-leaf3-b` (base log) and `cache-leaf3-p` (split log);
- `cache-leaf3-m` (walk log).

**Server side refs:** `push/cache-leaf3-*`.

**Scripts:** `scratch44/{measure,ops,chain,rows,cmp}.sh`, `walks.py`.

**Data:** `scratch44/data/*-summary.txt` and `m-fwd-walks.txt`.

**The log** (`zz_leaf.rs`, never pushed):
- C/G lines (report 43's);
- `W`: per walk;
- `EC`: per creation in a walk, with a value diff over the loop's edges, vertices and face;
- `TB`, `FX`, `VT`: per tree build, gap fix and vertex raise.
