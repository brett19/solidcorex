# 16. Stage C study: offset and shell, sweep, prism and primitives

Read-only study at 782ef9afc, using the method and vocabulary of report 07. Paths are relative to
`solidcore/crates/solidcore/src` unless they name another crate. Marks: [V] verified in code, [I] inferred,
UNVERIFIED (open, with the run that would settle it). Three fixes merged after this checkout (the kernel's
random generator, the fillet step latch, the body's size estimate in the context) are not reported as
open. The sweep, prism and primitive reading was done by a helper agent; its citations were spot-checked,
not re-read in full.

## 0. Summary

- **Offset has a clean read phase, and it already exists in code.** The face loop of the extended offset
  (the only path the app runs) calls `face::build(&Kernel, &OffsetGeometryCreation, &Brep, faceuse, ..)`
  (offset_executive/face.rs:153). It returns a plain value (offset surface, self-intersection curves),
  and the module header says "No topology is cloned or edited" (face.rs:15). The writes in that loop are
  the UV trims that `create_curves_from_face` makes on the input (P) and the insertion into the temporary
  result (apply). Split the loop into "compute every face, then insert in face order" and the heavy
  per-face work (offset B-spline approximation, self-intersection) runs over `&Brep`.
- **The edge phase is coupled.** Mapped edges intersect adjacent offset surfaces and settle shared offset
  vertices in a fixed order, with mated-edge and `marked` sets. It can be partly reformulated: compute the
  intersection curves as values, then mate in order. It is not needed for a first step.
- **Everything after the edge loop is assembly**: extension, crossing loops, piecewise merge, stitch,
  make_manifold, region flags. It stays sequential.
- **Shell of a solid ends in a Boolean difference** (p2.rs:2317). It gets that phase for free from the
  Boolean step.
- **Offset distance and pending extension (question B):** no writer touches an offset surface owned by a
  face. Every `set_offset_distance` writer acts on a fillet rail surface or on a private drop surface. One
  write goes through a shared reference: `evaluate_law_point`, surface_intersector/p1.rs:2185-2186, on
  fillet rails. The distance is a per-solve parameter there and could be an argument. A pending extension
  needs `allow_extension`, which is false on every surface the offset makes.
- **Sweep, prism and primitives are construction code.** They build faces from curves and parameters.
  The only body reads (prism measure/read/sheet) already take `&Brep`. Low payoff. No stage C step is
  needed beyond what the Boolean and the validity check bring.
- **Counts for the offset read-like phases:** P 4, R 1, C 3 (one of them on a path the app does not run),
  S 5.
- **Phase-scoped table:** yes, it holds. The face phase reads only the unedited input body.
- **Cost:** 5-8 PRs for the face phase, none expected to change results. The edge phase would add 4-6
  PRs, with 1-2 that may change results. Sweep: 0-2 optional PRs.

## 1. The operation as it is

### 1.1 Ops entries (solidcore-ops/src/offset.rs) [V]

Every entry runs under `with_body`, which snapshots the body. The options are always
`offset_options(distance) = {extend_convex_edges: true, merge_results: true}` (offset.rs:334), so the
kernel always takes the extended path `do_extended_offset` and never the rounded `do_solid_offset`.

| Entry | Input check | Kernel call | Ends in |
|---|---|---|---|
| `shell` :391 | `Expect::of` → `is_outward_solid` (body.rs:559); `shell_wall_room` for an inward wall | `thin_features::shell_brep` (thin_features.rs:233) → `shell_brep_simple` (p2.rs:2211) | `finish`: `is_manifold_solid` + `prove` (body.rs:650-690) |
| `offset_faces` :435 | `Expect::of`; room test over the other faces when moving inward | `OffsetFacesExecutive::offset_faces` (edits `b` in place) | `finish` |
| `thicken` :489 | none | `offset_brep_simple(-t/2)` if symmetric, then `shell_brep_full(t, &[])` | `finish` with `Expect::SOLID` |
| `offset_sheet` :524 | none | `offset_brep_simple` (p2.rs:2137) | `finish` |
| `draft` :622 | none | `draft_surface` + DRAFT_MOVED marks (:548, :560) | `finish` |

On a thin-feature refusal of an inward closed shell, the thin-feature fallback (thin_features.rs:167)
does the following:
- defeature through `defeature_and_rebuild` with a planar cap;
- shell what is left;
- `boolean_union` each feature back on (:206).

### 1.2 Kernel phases of the extended offset (`do_extended_offset`, p2.rs:965) [V]

| # | Phase | Where | Kind |
|---|---|---|---|
| 0 | API copy: `clone_standing_for_self_keeping_definitions`, `bp3::copy_brep` (shell); offset runs on the caller's `&mut Brep` because "lazy UV trim geometry updates require mutable access" | p2.rs:2211, 1995; 2094 | copy |
| 1 | `check_resolvable`, `refuse_meeting_planar_insets` | p2.rs:1545, 299 | read |
| 2 | `allocate_offset_brep` (temporary, `editing_enabled` true) | p2.rs:1568, 1591 | edit (new body) |
| 3 | **Face loop**, in face-list order: shell faces `copy_shell_face_prefix`; others `offset_face_prefix` (:1705), which runs `record_extension_reach`, `face::build`, singular/folding refusals, collapse records, `skip_offset_face_geometry` (:1776), then `build_offset_of_faceuse` (p1.rs:2111) into the temporary | p2.rs:985-994 | **read (compute)** then apply |
| 4 | `remove_extinct_branches`, `order_edgeuses_by_brep`, `keep_unmoved_edges` | p2.rs after :994 | edit of the temporary |
| 5 | Under `with_mark_lock` (:1000), the **edge loop**: WR-346 mates across collapsed faces, the convex-tangent pass, the main mapped-edge loop, deferred shell-tangent pairs, all through `build_offset_of_edgeuses_with_state` (p1.rs:1351) → `edgeuse::build` (edgeuse.rs:1410) → `edgeuse_offset` | p2.rs:1000 on | coupled compute + edit |
| 6 | `squeeze_collapsed_end_edge`, `reconstruct_periodic_boundaries`, `close_loop_gaps`, vertex caps (`make_cap_face` p1.rs:1817), step corners, lamina walls | p2.rs | edit |
| 7 | `extend_result_faces_prefix` (:1827), `resolve_crossing_loops` (creates UV trims, crossing_loops.rs:772, 1831), `restore_kept_edges` | p2.rs | edit |
| 8 | `piecewise_merge` (p1.rs:1938), `remove_excess_faces` (p1.rs:1055; BrepCache of the original at :1123) | p1.rs | edit + classification read |
| 9 | `stitch_merged_offset` (p2.rs:2479): Stitch, a widening trial on `clone_for_trial` copies (:2551, :2597 `check_brep_scoped` Level0, :2642), `with_rollback` + `stitch_into_solid` | p2.rs | edit, with independent trial reads |
| 10 | WR-667 cavity choice: `make_original_brep_cache` (p1.rs:3326, called p2.rs:1370), `cavity::is_cavity_region` point solves; #18570 material regions (local operation) | p2.rs | classification read |
| 11 | `make_manifold`, `find_and_set_infinite_region` (`editing_enabled` off and restored, :1431-1440), void flags | p2.rs | edit |
| 12 | `refresh_cached_topology`, `update_result_tolerances` = `validate_and_update_tolerances` (p2.rs:2121) | p2.rs | validity check with update |

Shell, after phase 12 of the offset it runs (`OffsetExecutive::shell_brep`, p2.rs:583):
- **Solid input:** `shell_difference` (p2.rs:2317) = `ManifoldKernelAdapter::manifold_boolean(..,
  Difference)` with `BooleanKernels { primary, other: None }` and `configure_operands` (tolerance, 20°).
  Then `make_shell_edges_exact`, `finish_attributes`, `refresh_cached_topology`.
- **Sheet input:** `merge_brep` of the source into the shell, `stitch_faces_with_context` up to 4 passes
  (tolerance ×10 each, :697-736), `merge_sheet_side_walls` (:2681), `make_shell_edges_exact`.

### 1.3 Sweep, prism and primitives (helper reading, spot-checked) [V unless marked]

This is construction. Inputs are curves, profiles and parameters. The result is built face by face and
ends in a stitch, merge or Boolean (§E). The few body reads are listed in §C.

## 2. Writes in the read-like phases (offset)

Read-like phases: 1, the compute half of 3, the compute half of 5 if split, and the classification reads
of 8, 9 and 10.

| # | Write | Where | Frequency | Class |
|---|---|---|---|---|
| W1 | UV trims created on the **input** by `create_curves_from_face` → `get_or_create_uv_trim_curve` (face/p2.rs:2834, call at about :2925), for shell faces and for every face in `build_offset_of_faceuse` (p1.rs:2177) | per face | common on a fresh body, none once trims exist | **P**: prepare trims of all input faces in face order (the same step as report 11's trim preparation) |
| W2 | Surface caches of input faces built through the face slot: `prepare_surface_solve_cache` / `prepare_surface_state_cache` (offset_geometry_creation.rs:736-757, :1175; edgeuse.rs:697-705 → `solve_surface_cache_of_face`) and `edgeuse_is_tangent_sector` (`&mut` for caches) in the convex-tangent pass | per face / per edge | common on curved faces; none on planes and cylinders (they use no cache) | **P**, or the phase table |
| W3 | BrepCache of the input for `remove_excess_faces` (p1.rs:1123) and cavity (p1.rs:3326) | once per call | rare (cavity: an inset with more than one finite region); remove_excess: every merge | **P** (build once over `&Brep`) |
| W4 | Trims and caches left on the **result** by cavity point solves | rare | rare | **R** (or P on the result before the classification) |
| W5 | Shared offset vertices and edges in the temporary, settled by the edge loop in mapped order (convex tangents first, deferred shell-tangent pairs last "so adjacent wall mitres settle the shared offset vertices first") | per edge | always | **C1** |
| W6 | Executive mating state: `note_mated_edgeuse` / `has_mated_edgeuse` (offset_geometry_creation.rs:601-612), `note_periodic_boundary_end`, the `marked` set under the mark lock | per edge | always | **C2**: a later edge skips or reuses what an earlier edge mated. Deterministic if the pairs are computed up front, as order fixes them today |
| W7 | `loop_points`, overwritten by every open face and read by every vertex cap (p2.rs:820-830, 906-927) | rounded path only | **never in the app** | **C3**: a stale value from the last open face reused by unrelated caps (port of upstream's static `sLoopPoints`). Reformulation: pass each cap its own points or none, as the extended path already does (p1.rs:1436-1443 passes `&[]`) |
| W8 | `get_or_create_gw_nurb` / `make_nurb` on the item's own new offset surface (p1.rs:2126-2155) | per face | always | **S** (item-owned value) |
| W9 | `EdgeBrep` / `FaceBrep` local `curves` / `surfaces` vectors (edgeuse.rs:1424-1427, face.rs:160-164) | per item | always | **S** |
| W10 | The executive's extended-surface memo (`get_create_extended_surface`, offset_geometry_creation.rs:684) | per base surface | common | **S** (or phase table) |
| W11 | `doing_boolean` (`with_doing_boolean` p2.rs:2902-2909; shell saves at :595, restores only on error at :775), `editing_enabled` (:1431-1440, :2721-2740), mark locks (:852, :1000) | per call | always | **S** (flags flipped and restored) |
| W12 | Notification records, attributes, collapsed-face lists, reach records | per item | always | **S** (collect per item, apply in order) |

Counts: P 3 (W1-W3) + the tangent-sector reads folded into W2 = **P 4** if counted as in report 07 (trims,
input caches, tangent-sector caches, BrepCache). **R 1** (W4). **C 3** (W5, W6, W7; W7 only on the unused
path). **S 5** (W8-W12).

For the face phase alone (the proposed first step) the only writes are W1, W2, W8, W9 and W12. All are P or
S, and **no C**.

## 3. What "prepared" means (offset face phase)

Before the face loop:
1. UV trims of every input edge use on faces that will be offset or copied (W1). Existing bodies coming
   from an earlier operation usually have them [I, report 11 and the m9 log measured "first pass only
   creates missing trims"]. Cost: one trim pass, which the loop pays today anyway, in the same order.
2. Surface caches (trimmed, kept in the face slot) of curved input faces (W2), or a phase table built on
   demand.
3. The input body's `refresh_cached_topology` state, which the API already does.

Not needed:
- `pending_extension` handling: `allow_extension` is false on every offset-made surface
  (offset_geometry_creation.rs:2881, :8252), and on input faces unless read from a file with the flag
  set (offset_surface.rs:2260).
- The offset distance: no offset-path writer.

## 4. Caches and the phase table

- The face phase reads input face surfaces, and their caches when the self-intersection or approximation
  needs a solve. `face::build` takes `&Brep`. So today the cache either comes through the slot's take/put
  (interior) or is built locally on the `FaceBrep` backend [I; UNVERIFIED which: count
  `make_surface_cache` versus slot hits in one shell of a B-spline body].
- The edge phase reads caches of both arenas through `EdgeBrep::surface_cache_for` (edgeuse.rs:636-690),
  taking `brep_mut(arena)`. Surfaces with no face owner fall back to `get_surface_cache` /
  `make_surface_cache`.
- **A phase table with no key would do for the face phase.** The input body is not edited from phase 1
  to phase 3. "Built in this phase" is valid while `&Brep` is held, as in the Boolean. For the edge phase
  it would hold for input-arena caches only. Result-arena faces are being edited, so they need
  build-on-use as today, or caches of the new offset surfaces prepared once at the end of phase 3.
- **The #21019 guard.** Stage B's `reads_leave_unchanged` lends a cache `Owned` when its geometry contains
  an OffsetSurface. The diff's only stated reason is the pending extension. Since an extension requires
  `allow_extension` (offset_surface.rs:444 default false; set true only at
  blend/fillet_solver/p5/recovery.rs:832 or from a file), the guard could narrow to "OffsetSurface with
  `allow_extension`". Offset results would then be lent `Shared`. One small PR, not result-changing.

## 5. Items

- **Face phase:** one item per non-shell input face (6 to a few hundred). The skew is large: planes are
  trivial; B-spline and procedural offsets approximate a surface and run a self-intersection, orders
  of magnitude slower. The value is `(Option<Surface>, SsiData<Curve>)` plus refusals, reach and collapse
  records. Combination: inserted into the temporary in face-list order. There is no floating-point sum.
  The early exits are refusals (`skip_offset_face_geometry` errors on SSI with merge_results, or more
  than 5 curves). Report the first refusal in face order to keep the error the same.
- **Edge phase (if split):** one item per mapped edge pair. Its value would be the intersection curve of
  two offset surfaces. Mating, trimming and vertex settling stay in order.
- **Stitch widening trials** (p2.rs:2551): independent trials on clones. Already read-like per trial,
  with few items.

## 6. Sequential regardless

These stay sequential:
- the temporary's assembly (phases 2, 4, 6-11);
- the mate and trim order of the edge loop;
- the piecewise merge, stitch, make_manifold and region flags;
- the shell's Boolean difference (whatever the Boolean step gives);
- `validate_and_update_tolerances` and the ops `prove` (the validity check step).

## 7. Cost

| PR | What | Results change? | Shared with report 07 §6? |
|---|---|---|---|
| 1 | Prepare input UV trims before the face loop, in face order | no (same trims, same order) | trim preparation (report 11) |
| 2-3 | Split phase 3: compute a `Vec` of per-face values, then insert in order; refusals reported in face order | no | no (offset-specific) |
| 4 | `create_curves_from_face` read form over `&Brep` with trims prepared (curves copied, no trim creation) | no | partly (read-form helpers) |
| 5 | Input surface caches through the phase table | no | yes (phase table) |
| 6 | Kernel per worker for the face items | no | yes (`cache_kernel`) |
| 7 | Narrow the #21019 OffsetSurface guard to `allow_extension` | no (copies only) | stage B |
| 8 | BrepCache of the input built once over `&Brep` (W3) | no | prepared view |
| (9-14) | Optional edge phase: intersection curves as values, mating in order; result-arena caches prepared at the end of phase 3 | 1-2 may (mitre and vertex settling order) | request loop for W4 |

The face phase costs 5-8 PRs, with no results expected to move. The edge phase would add 4-6 more.
`set_offset_distance` as an argument in the variable-radius intersector (B) belongs to the fillet step,
not here.

## 8. UNVERIFIED

- U1. Whether `face::build` gets input caches through slot take/put or builds them locally. Settle it by
  counting `make_surface_cache` and slot hits in a shell of a B-spline-faced body.
- U2. How many UV trims the face loop creates on corpus inputs that come from earlier operations. Count
  `get_or_create_uv_trim_curve` creations inside phase 3 on the 47-body set plus the shell goldens.
- U3. Whether `make_cap_face` uses `loop_points` at all: it passes them to
  `make_face_with_curves_with_state` (p1.rs:1853) with a single edge loop. It only matters on the rounded
  path, which the app does not use.
- U4. Whether any input face surface is an OffsetSurface with `allow_extension` true in practice (read
  from a file, offset_surface.rs:2260). Count on the corpus.
- U5. Whether the edge loop's results depend on order beyond the documented mitre settling. Shuffle the
  main mapped pass (keeping the convex-first and deferred-last groups) on the shell goldens.
- U6. The share of offset time in the face phase versus the edge and stitch phases. One timing run of
  shell and thicken on the corpus.
- U7. `OffsetFacesExecutive::offset_faces` was not read in depth; assumed to share phases 3-5 as a local
  operation.

## Answers

### A. Offset phases

- **Per-face compute over the unedited input:** `face::build` / `build_local` (face.rs:153, 176),
  `record_extension_reach`, the refusals, and the curve copies of `create_curves_from_face` once trims
  exist.
- **Per-edge compute:** offset edge curves from adjacent offset surfaces. They are interleaved with
  writes to the temporary and depend on order and on the mated and `marked` sets.
- **Building the result:** the rest.
- **The compute/assemble boundary:** clean inside phase 3 (between `face::build` and
  `build_offset_of_faceuse`). Only partial in phase 5.
- **`loop_points`:** carries the vertex-loop points of the last open face copied, and exists only on the
  rounded path (`do_solid_offset` p2.rs:803), which the app never runs. No face's offset result depends
  on it. The vertex caps there read a stale value from an unrelated face. The extended path passes `&[]`
  (p1.rs:1436-1443).

### B. `set_offset_distance` and `pending_extension`

- **`set_offset_distance` writers:**

  | Site | What it is | Class |
  |---|---|---|
  | surface_intersector/p1.rs:2185-2186 (`evaluate_law_point`) | through a shared reference, on fillet rails | per-solve parameter, could be an argument |
  | surface_drop_lib/p2.rs:177, 204, 721 | a private surface | scratch |
  | fillet_solver/p1.rs:1228/1250; fillet_custom_solver.rs:1038-1109, 1623-1667; fillet_standard_solver/p1.rs:1237/1243 | set, use and restore | per-solve |
  | fillet_solver/p1.rs:3026/3031; fillet_standard_solver/p1.rs:300, 791; fillet_standard_solver/p2.rs:1976/1983; ball_regular.rs:244; fillet_solver/surfaces.rs:60 | `&mut` on the solver's own rails, per station | per-solve |
  | fillet_solver/p4.rs:4845 (`sync_trace_offsets`) | syncs a copy | disappears once the distance is an argument |
  | fillet_geom/p1.rs:4016 | a local clone | scratch |

  The rest are tests. None is a real edit of a body. A clone gets a new `F64Cell` (offset_surface.rs:227),
  so copies do not share the cell.
- **`pending_extension`:** set only during evaluation when `allow_extension` is true and the point is
  outside the domain (offset_surface.rs:1371-1401). On offset paths the take/put at :560-580 puts back
  the same value. It is a per-solve fillet scope (`follow_extension_of` / `keep_extension_made_by`, used
  only by fillet code), not an edit.

### C. Sweep and primitives

This is construction code, with no heavy read phase over an input body.
- **Pure body reads that already take `&Brep`:** `prism::measure::section` :912, `equivalent` :1002,
  `read::planar_edges` :854, `sheet::sheet_regions` :115. Ops already calls them through `read_body`.
- **Reads interleaved with writes, each P:**
  - the topology sweep's per-entity `SweepGeometry` calls (geometry.rs:88-155, which return plain
    curves and surfaces; engine.rs:296-652 interleaves them with builds);
  - `flat_wall_plane` per side face (skin_flat_walls.rs:36-55);
  - the taper's keep/delete face filter (p2/taper.rs:788-828; #2756 notes faces grow as the square of
    the profile's curves).
- **Coupled:**
  - the topology sweep's same-entity memo (a later edge reuses vertices an earlier edge made,
    engine.rs:376-395);
  - the loft certify/refine rounds (sweep/loft_rails.rs:1016).
- **Whole-body silhouette sweep:** `sweep_whole`, engine.rs:1070-1196, is the only heavy read. It is not
  reached from ops.
- **Payoff:** low. These steps are small relative to the stitch, Boolean and `prove` they end in.

### D. The sweep store's kernel per body

- `SweepAccess::kernel(id)` (topology_sweep/access.rs:102-104) ports upstream's context per body. The
  engine saves, sets and restores `doing_boolean` on the destination's and the source's kernels
  (engine.rs:842-860, :1003-1008, restored in reverse order, also on failure).
- In production every `do_sweep` goes through `PrimitiveModel`, whose `kernel(_)` ignores the id and
  returns the one kernel (model.rs:325-330).
- `SweepStore` (store.rs:226-235) can hold several kernels:
  - borrowed (`register_kernel` :406) or moved in (`insert_kernel` :445);
  - all of them share one geometry-key sequence (`share_geometry_sequence`, :410, :449);
  - their production users (offset/offset_executive/merge.rs:159-165, fillet_executive/session/piecewise.rs:546-553)
    each register one borrowed kernel.
- No kernel is created or cloned in production. Nothing per kernel differs except mark locks and the
  toggled flag.
- **Fit with the stage C model:** it already matches "one kernel per unit of work". The shared geometry
  sequence is the one cross-kernel coupling: keys depend on interleaving if two kernels ever run at once
  [I].
- Stale: the `insert_kernel` doc still mentions the RNG (store.rs:444), which is now gone.

### E. Where they end in a Boolean, stitch or check

**Offset and shell:**

| Recipe | Ends in |
|---|---|
| Extended offset, every entry | `piecewise_merge` (p1.rs:1938) → `stitch_merged_offset` (p2.rs:2479, with `check_brep_scoped` Level0 in the trial, :2597) → `stitch_into_solid` → `make_manifold` → `find_and_set_infinite_region` → `validate_and_update_tolerances` (p2.rs:2121) → ops `finish`: `is_manifold_solid` + `prove` (body.rs:650-690) |
| Shell, solid input | additionally the Boolean difference `manifold_boolean(.., Difference)` (p2.rs:2317) |
| Shell, sheet input | `merge_brep` + `stitch_faces_with_context` ×≤4 (p2.rs:697-736) |
| Thin-feature fallback | `boolean_union` (thin_features.rs:206) |
| Input side | `is_outward_solid` (body.rs:559), `shell_wall_room` |

**Sweep, prism and primitives:**

| Recipe | Ends in |
|---|---|
| Linear and rotational sweep | `stitch` / `stitch_open` (p1/primitives.rs:1537/1540) or `stitch_and_orient` (p3/sweeps.rs:413, 576, 674) |
| Inside `do_sweep` | `piecewise_merge` (engine.rs:907-915), `stitch` + `orient` (:917-933), wire sub-sweep `union` (:973-980), `regularize_translation` (:993) |
| Curve sweeps | `stitch_and_orient` (p3/construct.rs:1044) |
| Skin / loft | `stitch_faces` (p2/skin.rs:582) + `flat_walls_as_planes` (api_surfaces.rs:336) |
| Taper | `stitch_faces` / `stitch_and_orient` (p2/taper.rs:466, 568, 639); fallback `piecewise_merge` :785 + `union` :834 |
| Sweep along a planar path | `boolean_trees` union (p2/planar_path.rs:684) + `stitch_faces` (:641, :695) |
| Segments / cornered | `stitch_and_orient` (segments.rs:192, cornered.rs:335) + `is_manifold_solid` (cornered.rs:337) |
| Screw | `stitch` (screw.rs:901) + `is_manifold_solid` (:908-917) |
| Primitives | `stitch` (primitives.rs:933, 1727); offset profile → `boolean_2d` (offset_profile.rs:167) |
| Ops, every recipe | `making` → `finish` → `prove` (solidcore-ops sweep.rs:550) |
| Ops, extra calls | `stitch_faces` (sweep.rs:946), `stitch_and_orient` (:1313), `api_stitch_into_solid` (:1371); prism `union_all_with` (prism.rs:251-254) and `boolean_2d` (prism.rs:69) |

Small finding:
- **Editing flags.** Several construction paths set `editing_enabled` true and clear it on success only,
  or never clear it. Never cleared: p2/skin.rs:277, p3/construct.rs:797, p2/planar_path.rs:196/200.
  Cleared on success only: p3/sweeps.rs, p2/rotational.rs, p1/primitives.rs.
- **Where this matters, and where not.** It does not matter for parallel work, since each recipe owns a
  new body. It does matter for "a result is a function of the input", if a later step reads the flag.
  UNVERIFIED; settle it by checking `editing_enabled` on each primitive's returned body in a test.

### F. Ranking and place in plan-v5's order

| Subject | Payoff | Difficulty | Place |
|---|---|---|---|
| Shell of a solid | its Boolean difference phase | none extra | rides on step 4 (Boolean) |
| Offset face phase | high on curved bodies (B-spline offset approximation + self-intersection per face, independent items) | low: the code already returns values over `&Brep`; needs trims prepared and one loop split | after step 4, before or beside step 5 (fillet); needs the phase table and trim preparation built in steps 2 to 4 |
| Offset tail (stitch, validity update, `prove`) | moderate | none extra | step 3 (validity check) covers `validate_and_update_tolerances` and `prove` |
| Offset edge phase | moderate | high (C1, C2: order-dependent mitre settling) | after fillet, optional |
| Sweep, prism, primitives | low (construction; the reads already take `&Brep`) | low | no own step; their stitches, Booleans and `prove` ride on steps 3 and 4 |

Verdict: the offset face phase is easier than the Boolean. It has no coupled write, and the per-face
function already exists in pure form. The offset edge phase is about as hard as the Boolean's coupled
paths, and it fires always, not rarely. Sweep and primitives need no stage C work of their own.
