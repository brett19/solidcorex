# 75 - Step 7: the idiom hunt ahead of the full Boolean, make, deform and repair

Written for solidcorex, 2026-11-28. Step 7 of 01 §10.2 is layers 7 and 8:

- `scx-boolean` in full (beyond the analytic slice of step 4);
- `scx-deform`;
- `scx-make` in full;
- `scx-repair`.

It is done when the Boolean suite passes, the Boolean algebra properties hold, and the difftest
judges every step-7 operation by outcome.

This note does for step 7 what 72 did for step 4 and 74 did for step 6. Steps 5, 6 and 7 overlap,
so every task below names the step-5 and step-6 rows it waits on, and a task that waits on nothing
can start now. For each crate the note gives:

1. what the pin does, its size, and who calls it;
2. the algorithms and the lessons the pin records;
3. the idiom hazards, with file and line;
4. which tests assert truth and which only assert the pin's output;
5. the performance traps;
6. the geometry it needs, and what can be built now on analytic geometry;
7. the proposed Rust shape.

§1.3 is the capability ledger. §7 is the task table and §7.1 the graph. §8 gives the truth tests per
task. §9 holds the open questions, §10 the case against this plan, §11 the proposed 04 rows.

- **Labels** (as in 57, 63, 72 and 74):
  - READ: read from the pin's code or docs.
  - MEASURED: counted here; the method is given.
  - JUDGED: a design judgement.
- **Pin paths.** `K/` is the pin kernel's `src/`, `O/` is `solidcore-ops/src/`. `B/` is
  `K/boolean/`, `TI/` is `B/topology_intersector/`, `PT/` is `K/topology/proto_topology/`,
  `PC/` is `K/primitive/primitive_creation/`. `A/` is the app's
  `crates/kernel/src/` in the pin's repository (the SolidCraft kernel crate that calls ops). Line
  numbers are at the pin commit of 63.
- **Nothing here was built or run on the pin.** It was read with grep, sed and wc only.

## 0. How the counts were made (MEASURED)

- Lines are `wc -l` per file, tests included. "Non-test" means above the file's `#[cfg(test)]`.
- A **swallow** is a non-test `.ok()`, `unwrap_or*`, `Err(_) =>` or `let _ =`, plus the pin's named
  status-ignoring helpers (`ignored`, `ignore_soft`, `ignore_failure`, `ignore_status`,
  `status_free`). Each was counted by grep and read where it is listed by line.
- A **literal** is a float literal below 1e-2 in non-test code.
- "Truth test" means the assertion is a closed form, an algebraic property, an independent oracle or
  a stated invariant. "Pin-output test" means counts, kinds, goldens or layout compared with the pin
  itself.

| Area | Pin files | Lines | Non-test swallows | Unit tests | Integration tests |
|---|---|---|---|---|---|
| Boolean (beyond the slice) | `B/` (33,434), `PT/` (20,024), `K/topology/brep/{cutting, planar_section}.rs` (2,028), `O/{boolean, split, trim}.rs` (2,639) | ~58,100 | ~240 (arena_pair 55, TI p1 30, imprint_ends 23) | ~250 | `O/../tests/boolean.rs` 78, `boolean_api.rs` 29, trim 17, split 19, kernel_cases 269 entries |
| Make: revolve, sweeps, prisms | `K/sweep/` (26,303 with loft), `PC/` (17,390; all of `K/primitive/` is 20,700), `K/prism/{extrude, sheet}.rs` (662 of the module's 19,613, of which 12,142 are in non-test files), `O/{sweep, prism}.rs` and helpers | ~44,400 | ~140 plus 28 `ignored` | ~280 | ops sweep 37 plus a 26-case closed-form table, prism 35, kernel sweep suite ~140 |
| Make: loft, skins, patches | `loft_sections`, `loft_rails`, `warp`, `skin_*`, `section_blend`, `K/nurbs/{certificate, gordon, n_sided}.rs`, `sheet.rs`, `O/{surface, blend_surface, network}.rs` | ~17,400 | ~60 | ~120 | loft_rails 13, section_blend 9, two rails 4, skin 11, section orders 23 |
| Deform | `K/surface/{b_spline_volume*, bend, twist, unbend, srf_in_volume}`, `K/surface/volume/`, `K/curve/crv_in_volume.rs` | 15,351 | ~100 | 85 | none |
| Repair | `K/heal/` (23,663), `O/heal.rs` (329); in the app `A/{repair, heal, fill, delete_face, tolerance_heal}.rs` (708, 207, 1,675, 1,283, 372) | 28,237 | ~70 | 66 | ops heal 18 |

## 1. Shapes common to step 7, and the rulings

### 1.1 What the pin does on every path (READ)

- **Retry on failure.** One ops entry retries: `union_copies` (`O/boolean.rs:183-226`) retries a
  union that fails in the radial sector search with the operands swapped under
  `Contract::Proved`, then with `rebuild_native` operands. `boolean` and `boolean_with`
  (:151, :160) do not retry, but every app union goes through `union_copies` (the app's
  `crates/smlib/src/core/boolean.rs:51`), so every union the app makes can take the retry. The
  kernel's `glue_all_or_none` (`B/api_brep.rs:433`) restores a snapshot and goes on unglued.
  `classify_remaining` (`B/merge/p4.rs:1397`) casts a ray at fractions 0.45678 and 0.456789 along an
  edge and moves to the next edge when that fails (:1481, :1483). The curve sweep drops the exact
  procedural definition with `.ok()` and fits one instead (`PC/p3/construct.rs:991, :1000`).
  `planar_sides` falls back to the skin on any error (`O/sweep.rs:1225-1314`).
- **Build in NURBS, then recognise.** Every revolve edge builds the B-spline surface of revolution
  first (`K/sweep/rotational_sweep_geometry.rs:867, :925`), then accepts the analytic candidate only
  if 16 sampled side lengths agree within 1e-3 relative (`analytic_revolution.rs:172`). The linear
  sweep builds a NURBS extrusion and recognises a plane or cylinder
  (`trans_sweep_geometry.rs:302-307`). Flat skin walls are found after the B-spline is built
  (`skin_flat_walls.rs:36`, WR-782).
- **Tolerance ladders.** The Boolean widens by 10, 4, 1.25 and 5 times a tolerance at
  `B/merge/p2.rs:1717`, `PT/p2.rs:3707`, `PT/p1/proto.rs:2911` and `proto.rs:40` (ED_TOL_FACTOR),
  with CBI_FUDGE 10 and LOCAL_TOLERANCE_RATIO 1000 in `PT/p3.rs:947, :2443`. `matched_tolerances`
  (`B/operand_tolerances.rs:57`, #3292) copies the coarse operand and checks it at Level2. The SMLib
  healer resets every tolerance from a model-size bucket (`K/heal/heal_data/p2.rs:3035-3058`) and
  grows edge tolerances to `gap_gain * gap` (:3138).
- **Global mode flags.** `set_doing_boolean` is saved and restored by hand at 13 sites
  (`K/sweep/topology_sweep/engine.rs:937-956`) and in the skin (`PC/p2/skin.rs:277-302`), where
  `set_editing(brep, true)` is never unset (:323). The app's heal holds a `static AtomicBool` and a
  `thread_local` (`A/tolerance_heal.rs:31-37`, outside the kernel), switched by the global
  `set_heals_tolerance_defects` (re-exported at `A/lib.rs:623`; the review cited :621, which is
  the `heal_tolerance_defects` re-export two lines above).
- **Positional bools and integer modes.** `MergeOptions::new(false, true, false, false, true)`
  (`O/boolean.rs:425`); `TopologyIntersector`'s ten bools and a `u64` lamina mode
  (`TI/p1.rs:1221-1230`); `cap_ends: i32` with four meanings (`K/sweep/api_brep.rs:280`); 22
  `which: i32` parameters in `proto.rs`; `HealerOpType` compared by `code()`.
- **Out-parameters for the faces made.** `create_curve_sweep` fills three `&mut Vec<FaceId>`
  (`construct.rs:826`), the skin four (`skin.rs:123-135`), and `ruled_surface_from_offset` four,
  including `missing_faces: &mut bool` (`PC/p3/ruled.rs:69`).
- **Reads through `&mut`.** `SweepAccess`'s reads take `&mut self` (`access.rs:207, 442, 446, 518,
  522, 526`); `gap.rs:1047, 1189` take `&mut Brep` to read gaps; `sheet.rs:149, 172` likewise;
  `classification.rs:130` copies the whole B-rep per vertex-face gap query.
- **Caps instead of budgets.** MAX_CELLS 4e6 (loft pairing), MAX_HALVINGS 12 and MAX_SEGMENTS
  65536 (loft certificate), DEFAULT_MAX_WIRES 257, MAX_SHRINK 20000 (screw), repetitions 4096
  (whole-body sweep), ten mark and repair passes in `manifold_boolean` (`p4.rs:1030, :2046`),
  1<<16 face and edge lists.

### 1.2 Rulings for step 7 (JUDGED)

These apply the house style of 04 row 107 and 01 §5 to what §1.1 found. Each is argued where the
crate section applies it, and the contested ones are in §9.

- **R1. One Boolean core.** Every Boolean-family entry (two-operand, n-ary, imprint, split, section,
  trim, interference, intersection) is a selection or a truncation of one pipeline: pairs,
  interferences, merged points, face-face, imprint, classify, build (BO1-BO4). The cellular merge
  is that pipeline over n operands without the final selection (04 row 16). No entry has its own
  intersector.
- **R2. No retry, no operand swap, no rebuild.** A failure is a typed cause the caller sees. The
  pin's one retry (`union_copies`, §1.1) exists because its radial sector search at a shared edge
  depends on operand order; scx's ordering of faces about a shared edge is a function of the
  geometry alone (BO12's proof covers it), so the union is right on its first run or
  `Unsupported` with the cause named. The result must not depend on operand order beyond the
  stated rule (`a`'s ids are kept); BO10 tests it, including the #20408 collars in both orders.
- **R3. No tolerance ladders.** Coincidence is `max(res, acc₁, acc₂)` (09 §4) everywhere in step 7.
  None of the pin's factors (10, 4, 1.25, 5, 100, 1000) is ported. A stage that needs room proves a
  bound and records the accuracy it reached (a `Raise`), never widens a test.
- **R4. Build exactly, never build-then-recognise.** Make constructs the analytic kind directly
  from the profile when one exists (as MK3 does for extrude): a line revolved is a plane, cylinder
  or cone; an arc revolved is a sphere or torus. Where no analytic kind exists the surface is the
  step-5 procedural kind (revolution, extrusion, sweep), whose evaluation is exact. A NURBS fit is
  built only where the operation is a fit (loft, skin, Coons, Gordon), and then with a certificate.
- **R5. Choose before building, not after failing.** Where the pin catches an error and builds
  another way, scx decides by a predicate on the input first (a path of lines and arcs takes the
  exact construction; anything else takes the general one). Both branches must be correct;
  neither is a fallback for the other's bugs.
- **R6. Caps become budget; certificates get a fixed budget.** Every loop the pin caps charges the
  operation's `Budget` and polls `Stop`. A certificate's halving limit is a fixed per-operation
  amount (01 §5.2) that yields `Unproven` with its bound, never a refusal and never a size taken
  from the remaining budget.
- **R7. `Made<R>` and `Done<C>` carry what the out-params carried.** Constructors return
  `Made<R> { body, faces: R }` with a typed face record per construction (`ExtrudeFaces` is the
  model). Operations return `Done<C> { body, changes, report }`; lineage is `Changes`.
- **R8. No global modes.** `set_doing_boolean`, `set_editing`, `set_make_analytics` and the app's
  static have no counterpart. A behaviour that changes the answer is a typed option on the call
  (01 §7.2), part of every memo key it feeds.
- **R9. Reads are `&Brep`.** No read takes `&mut`. Cached data (box trees, boundary trees) is filled
  by `scx-brep` on first read and charged the same whether warm or cold (01 §6.3).
- **R10. Sampled acceptance becomes a proof or a typed answer.** The pin's 16-sample revolve test,
  6×6 deform exactness grid, 5×5 fold grid on Coons sheets and 20-sample gap peaks are replaced by
  construction (R4) or by a certified bound (the loft and Gordon certificates, SU6's gaps). Where
  no proof exists the answer is `Unproven` with a bound, as for the fold check (Q9).

### 1.3 Capability ledger (READ the pin's pub items and ops entries; JUDGED homes)

"Row" is the task in §7 or an existing row. "Not ported" means the capability is a workaround that
the rulings remove, with the reason.

| Pin capability | Called from ops | scx home | Row or replacement |
|---|---|---|---|
| `boolean`, `boolean_union/difference/intersection` (`B/api_brep.rs:687-722`) | yes | `scx_boolean::boolean` | BO1-BO9 (built); NURBS faces BO23 |
| `boolean_with(.., Contract)` (`O/boolean.rs:160`; `Contract` at `O/body.rs:645`) | yes | no mode: `boolean` always gives what `Contract::Proved` gives (§2.6) | BO10b |
| empty-result refutation (`Witnesses::refute`, `O/boolean.rs:281`; `Error::EmptyRefuted`) | yes | a postcondition of every `boolean` with an empty result (§2.6) | BO10b |
| `boolean_with_options(Imprint)` (:785) | `O/boolean.rs:400` (returns `(Body, bool)`), `trim.rs:311` | `scx_boolean::imprint`; the bool is whether `Done.changes` is empty | BO13 |
| `split_face_by_curves` (`O/trim.rs:237`): curves on one face, the regions returned for picking | yes | `scx_boolean::imprint_curves` | BO13b |
| `trim_by_dropped_curves` (`O/trim.rs:438`): curves dropped onto faces, regions kept by points | yes | `scx_surface` drop (SF1), then `imprint_curves` and `keep` by points | BO16b |
| `boolean_with_options(Slice)` | `O/split.rs:368` | `scx_boolean::split` | BO15 |
| `split_faces` (`O/split.rs:282`): faces cut by several tools in turn, `Error::ToolStopsShort` | yes | `scx_boolean::split_faces` (one pass over all tools) | BO15b |
| `split_body_by_sheet` (`O/split.rs:506`): sides named by one sheet face's normal | yes | `split` with `Tool::Sheet { sheet, facing }` | BO15 |
| `boolean_with_curves` (:804) | `trim.rs:311` | `Done` of `imprint` carries the section edges in `changes` | BO13 |
| `union_all` (`O/boolean.rs:310`) | yes | `scx_boolean::boolean_all` | BO11, then BO12 |
| `merge_breps`, `boolean_lists`, `boolean_trees`, `non_manifold_boolean` | no | `scx_boolean::merge` (cellular) | BO12 |
| `piecewise_merge` (kernel sweep and offset only) | no | `merge` plus `select` | BO12; callers in MK8 and step 8 |
| `boolean_merge` (stitch, measure in ops) | yes | `merge` plus `select(Union)`; measure's uses move to bodyq | BO12 |
| `boolean_2d` (:902; `O/prism.rs:82`) | yes | `scx_boolean::planar` | BO17 |
| `split_body` (`O/split.rs:423`, two Booleans on copies) | yes | `split` returns both sides from one pass | BO15 |
| `trim_sheet` (`O/trim.rs:276`): regions kept by the points nearest them | yes | `scx_boolean::trim` with `keep: &[Point3]` | BO16 |
| `remove_faces`, `remove_feature_with_crossing_patches`, `remove_round_junction`, `remove_ring_groove` (`O/trim.rs:767, :656, :670, :821`) | yes | `scx-local` (step 8, defeature) | step 8 |
| `project_curves`, `face_section`, `drop_curves` (`O/sweep.rs:2039, :2097, :2167`) | yes | projection and drop in `scx-surface` (SF1); `face_section` is `section` scoped to a face | SF1, BO14 |
| `PlaneCutter`, `cut`, `do_cut` (`K/topology/brep/cutting.rs`) | no | `section` by plane | BO14 |
| `create_planar_section_curves`, `face_plane_section_curves` (`planar_section.rs`) | sweep only | `section` returning curves | BO14 |
| face/face and body/body intersection | no ops API | `scx_boolean::intersect` | BO18 |
| interference | none in the pin | `scx_boolean::interference` | BO19 |
| same-body refusal (`ptr::eq`, `split.rs:510`) | yes | the same-body shortcut | BO20 |
| caller-stated incidences | none in the pin | `boolean_given` and `Incidences` | BO21 |
| tangent patches (`resolve_tangent_patches`, `p4.rs:509`, #17118) | inside | stated facts, closed form, then tracing | BO22 |
| `matched_tolerances` (`operand_tolerances.rs`) | yes | not ported: coincidence rule (R3; 01 §1.3) | — |
| `glue_self_touching_edges` (WR-754) | inside | not ported as a pass: a body that needs it fails `finish` | — |
| `union_copies` retries | yes | not ported (R2) | — |
| `temporary_brep_transform` origin shift (`p1.rs:1369`) | inside | not ported: scx's evaluation is exact or certified at any origin; BO10 tests a far-origin recipe | — |
| `extrude`, `create_linear_sweep` | `O/sweep.rs:815` | `scx_make::extrude` | MK3 (built); draft walls step 8 taper |
| `revolve` (`directed.rs:61`, `PC/p3/sweeps.rs:474, :618`) | `O/sweep.rs:876` | `scx_make::revolve` | MK4 (analytic), MK4b (procedural) |
| `sweep_along_segments`, `cornered_sweep` | `O/sweep.rs:1011` | `scx_make::sweep` on line and arc paths | MK5 |
| `create_curve_sweep_with_placements` | `O/sweep.rs:1069` | `scx_make::sweep` general path | MK7b |
| `stations`, `fractions` (rotation-minimising frames) | `controlled.rs` | `scx_make::frames` | MK7a |
| `screw_sweep*`, with the radial law (`O/sweep.rs:1824-1871`) and joined walls (`Walls::Joined`, `K/sweep/screw.rs:53`, the app's `join_spline_walls`) | `O/sweep.rs:1831, :1903` | `scx_make::screw` with `Screw { law, walls }` | MK6 |
| `SweepControl` twist, end scale, one or two rails (`A/sweep_path.rs:46-68`) | app | `scx_make::sweep`'s `PathFrame` | MK7b |
| `sweep_whole` (whole-body sweep by silhouette, `engine.rs:1171`) | through `screw_sweep_with_body`, member | `scx_make::sweep_solid` | MK8 |
| `create_sweep_along_planar_path` (`PC/p2/planar_path.rs`) | no | `sweep` on a planar path (MK5) | MK5 (no Boolean needed) |
| `radial_sweep` (`local_operation.rs:404`) | through local ops | `scx_make::radial` | MK13 |
| `extrude_region`, `sheet_regions` (`K/prism/`) | `O/prism.rs:184, :231` | `scx_make::prism`, `scx_make::sheet` | MK14 |
| `face_on`, `sheet_on`, `create_planar_faces` (`sheet.rs`, `api_trim_ops.rs:242`) | `O/prism.rs:123` | `scx_make::face` | MK15 |
| `match_loft_sections` | `O/sweep.rs:1133` | `scx_make::loft::pair` | MK9 |
| `loft`, `loft_sheet`, `loft_to_point*`, `skin_with_ends`, `sweep_stations` | `O/sweep.rs` | `scx_make::loft` | MK10 |
| `loft_rails`, `sweep_two_rails`, warp | `railed.rs`, `birail.rs` | `scx_make::loft` with rails | MK10b |
| loft certificate (`K/nurbs/certificate.rs`) | `O/curve_knots.rs:53` | `scx_make::certificate` (01 §1.3) | MK10 |
| Coons (`sheet.rs:469`), `n_sided`, `gordon` | `O/surface.rs:251, :285`; `network.rs` | `scx_surface::{coons, n_sided, gordon}` (01 §1.3), faces by `scx_make::face` | MK11 |
| `ruled` (`O/surface.rs:340`), `reverse_sheet` (:366) | yes | `scx_make::ruled`; `reverse` as a Draft method | MK17 |
| `section_blend`, `section_blend_surface` | `railed.rs`, `blend_surface.rs:168` | `scx_make::blend` | MK12 |
| `blend_edges`, `fillet_sheets`, `fillet_crossing` (`O/blend_surface.rs`) | yes | `scx_make::blend` on SSI | MK12b |
| `ruled_surface_from_offset` (taper only) | no | step 8 taper | — |
| `create_taper_extrude*`, `taper_walls` | through extrude | step 8 taper | — |
| `BendVolume`, `TwistVolume`, `UnbendVolume`, `SrfInVolume`, `CrvInVolume` | none | volume maps in step 5; `scx_deform::deform` | DF1-DF3 |
| `BSplineVolume` (a stub upstream: always `SM_ERR`) | none | not ported until a caller exists | Q6 |
| `heal_short_edges_and_slivers` (`tolerance_defects.rs:1221`) | `O/heal.rs:183` | `scx_repair::heal` | RP1-RP4 |
| Repair Part step 1: rebuild a face SMLib refuses as its analytic surface (`A/repair.rs:11-14`) | app | `scx_repair::rebuild_analytic` | RP7 |
| Repair Part step 2: sew free edges within a gap (`A/repair.rs:15-18`, `stitch_no_worse` at :246) | app | `scx_repair::sew` over `scx-stitch` | RP5 |
| Repair Part step 3: patch free-edge holes, planar or G1 (`A/repair.rs:19-21`, `patch_holes` :582, `A/fill.rs:100` `patch_edges_with`) | app | `scx_repair::patch_holes` over MK11 | RP6 |
| `RepairOptions { gap, patch_holes, heal_first }` (`A/repair.rs:51`) | app | separate entries and an ordered `&[Step]` list (§6.5) | RP8 |
| faces on one surface and edges on one curve merged (`A/heal.rs`) | app, after operations | `scx-brep` Draft methods, called by the Boolean's build stage and by repair | BR11 |
| delete face with heal (`A/delete_face.rs`) | app | `scx-local` (step 8) | step 8 |
| SMLib healer (`heal_data/`, `gap.rs`, `face_props.rs`) | `heal_made` (`K/topology/brep/p10.rs:546`) on untrusted records, from `solidcore-convert/src/brep.rs:317` | not ported: it widens tolerances (R3); counted first (RP0) | RP0, Q7 |

## 2. `scx-boolean`: beyond the analytic slice

### 2.1 What the pin does (READ)

**Size and entries.** `B/` is 33,434 lines: `topology_intersector/` (p1 6,650, p2 3,589, p3 2,142,
curved_strip 2,078, coincident_strip 1,112, duplicate_edge_run 362), `merge/` (p4 3,000 with
kernel_adapter 2,548 and arena_pair 2,448, p2 2,483, p1 2,124, p3 1,950, p5 799) and `api_brep.rs`
1,636. The proto topology it builds on is another 20,024 (`PT/p2.rs` 6,958, `p1/proto.rs` 5,187,
`p3.rs` 3,873, `p2/imprint_ends.rs` 1,522). The ops entries are `O/boolean.rs` 557, `O/split.rs`
553 and `O/trim.rs` 892.

The ops layer calls `boolean` and `boolean_with` (`O/boolean.rs:151, :160`), `union_all` (:310,
pairwise level by level; the only n-ary entry), `boolean2d` (:361), `imprint` (:400), `slice` and
`split_body` (`O/split.rs:368, :423`), `cut_face`, and `trim_sheet` (`O/trim.rs:276`).
`boolean_merge` is also called by ops stitch (:476) and measure (:3076, :3314, :3377, :3611).
`piecewise_merge`, `boolean_lists`, `boolean_trees`, `non_manifold_boolean`, `merge_breps` and
the plane cutter have no ops caller.

**The ops pipeline** (`O/boolean.rs`):

1. `matched_tolerances` (`operand_tolerances.rs:57`, #3292): a trial copy of the coarser operand
   and a Level2 check, FEATURE_MARGIN 0.25 (:22).
2. `Witnesses::of` copies the other operand (:276).
3. `sm::boolean`, then `update_tolerances_after_boolean`.
4. An empty result is checked against vertex locations (`refute`, :281).
5. In `union_copies` only (:183-226): on a radial-sector failure (:231), retry with the operands
   swapped under `Contract::Proved` (:204-215), then with `rebuild_native` operands (:216-226).
   `boolean` and `boolean_with` run once; every app union calls `union_copies`. Its regression
   is `tests/boolean.rs:50-75`, the #20408 collars: two swept parts whose coaxial 1.3 mm collars
   overlap over 2 mm, joined to the sum of the volumes less π·1.3²·2.

**The kernel** (`merge_boolean_by`, `api_brep.rs` near :606) glues self-touching edges first
(WR-754), then runs `manifold_boolean` (`p4.rs:2257`) when both operands are manifold solids and
`non_manifold_boolean` otherwise. `manifold_boolean`: intersect, insert and relate (`TI/p2.rs:1386`),
then `mark_common` (up to ten passes, :1030), coincidence repair (ten attempts, :2046),
`classify_edge_faces`, `classify_remaining` by ray (:1397), delete faces, post-process.
`intersect_topology` (`TI/p2.rs:1530`) pairs faces by a spatial index (WR-527, #1526), checks
surface coincidence, crossings inside coincident pairs (#15718, #1794, #16146), intersects face
pairs (`TI/p1.rs:924`), probes curved strips (#18166) and drops duplicate edge runs (WR-647).

**Recorded lessons** are dense: 48 issue references in `PT/p2.rs`, 32 in `proto.rs` and `PT/p3.rs`,
28 in `TI/p2.rs`. The ones step 7 must answer:

- #372 (`p3.rs:1402`): a failed imprint leaves the operands half edited. scx's operands are `&Brep`,
  so this cannot happen.
- #17118, #16937 (`set_relationships`, `resolve_tangent_patches`): tangent patches. BO22.
- #16935 (`close_intersection_loop`), #432 (`fix_gaps`): curves that do not close. In scx a curve
  from the certified march closes by construction (SM7, SM8) or the pair is `Unsupported`.
- WR-452, WR-453 (`imprint_ends.rs`): extend a curve to its vertex, else move the end. In scx the
  ends are merged points (stage 3) and the imprint cuts at them; nothing moves.
- #16637 (`proto.rs:2283`): widen to absorbed points. Not ported (R3): a merged point is within
  coincidence or it is not.
- #11652, #21801 (`coincidence_bound`, `api_brep.rs:329`), #16640 (`certified_line_gap`): certified
  coincidence. scx has SU6's bounds.

### 2.2 Idiom hazards (READ)

Non-test counts: `&mut Kernel` / `&mut Brep` / swallows / bool fields.

| File | &mut Kernel | &mut Brep | Swallows | Bool fields |
|---|---|---|---|---|
| `merge/p4/arena_pair.rs` | 10 | 8 | 55 | 10 |
| `TI/p1.rs` | 15 | 4 | 30 | 40 |
| `PT/p2/imprint_ends.rs` | 0 | 0 | 23 | 4 |
| `PT/p3.rs` | 4 | 0 | 17 | 3 |
| `TI/curved_strip.rs` | 0 | 0 | 12 | 0 |
| `PT/p2.rs` | 3 | 0 | 11 | 7 |
| `merge/p4.rs` | 0 | 38 | 10 | 19 |
| `TI/p2.rs` | 14 | 0 | 10 | 10 |
| `merge/p4/kernel_adapter.rs` | 3 | 27 | 8 | 7 |
| `merge/p3.rs` | 27 | 13 | 3 | 10 |

The worst sites:

1. `merge/p1.rs:195`, `p4.rs:400-404`: `Err(_) => Ok(None)` through `ignore_failure` and `ignored`.
2. `p4.rs:1481-1483`: ray classification at magic fractions, and on failure the next edge.
3. `p4.rs:809`: `Err(_) => return Ok(false)`.
4. `TI/p1.rs:5025, :5472`: `Err(_) => continue`.
5. `TI/p1.rs:1221-1230`: ten bools and a `u64` lamina mode, set by `set_intersect_lamina_edges`
   (`kernel_adapter.rs:718`).
6. `TI/p1.rs:924`: `face_face_intersect_curves` takes `Option<&mut Vec<_>>` out-params.
7. `merge/p1.rs:105`: `MergeOptions` as five positional bools.
8. `api_brep.rs:433`: `glue_all_or_none` returns 0 on failure and the merge continues.
9. `operand_tolerances.rs:71-73`: a failed check reads as "not matched".
10. `kernel_adapter.rs:318, :542`: `let _ =` on tolerance sync and SSI clearing.
11. `proto.rs`: 22 `which: i32` parameters; `topo_dimension` returns `i32` (:342).
12. `O/split.rs:149`: `Error::Status(SM_ERR)`.

No statics, thread-locals or HashMaps in the kernel's Boolean files. The only shared mutable state
is an `Arc<AtomicU32>` (`kernel_adapter.rs:87`) and two `OnceCell`s (`arena_pair.rs:199, :287`).

scx's slice already avoids every one of these (04 rows 135-155). The hazards matter for step 7
because the entries the slice lacks (imprint, slice, trim, the 2-D Boolean) are the ones that set
the mode flags: each mode becomes its own entry (R1).

### 2.3 Truths and tests (READ)

Most traced tests assert validity, solid count or topology counts (`[6,12,8]` inline at
`O/boolean.rs:116`, `[12,27,18]` at `tests/boolean.rs:3680`). Those record SMLib's seam and merge
choices and are not ported as assertions (72 §8.1 rule: counts printed, not compared). The truth
tests to port:

1. `tests/boolean.rs:37` two overlapping boxes: volumes 15, 7, 1.
2. `:3585` `joins_as_both_less_the_shared`: inclusion-exclusion; used at :3633, :3672.
3. `:2948`, `:2990`: ten wedges cut, then joined, in turn; equal to the target less or plus them.
4. `:3528`: a symmetric cut through a half pipe is the same for every length past it.
5. `:500`: a union that fills a void is the whole box.
6. `:2791`: truly empty differences and intersections; lens and circle areas at `:746`, `:4455`.
7. `boolean_api.rs:381`: closed solids with analytic volume.
8. `boolean_api.rs:483`, `:509`: A − A is empty, also with STEP tolerances and for a bore minus its
   copy.
9. `boolean_api.rs:873` (closed form at :843), `:563`: a tiny sphere against a box edge; a shallow
   cap.
10. `tests/trim.rs:371` (slot area on a half cylinder), `:627` (chains cut in either order agree);
    inline `O/boolean.rs:477, :489, :503` (union_all against closed forms).

The pin tests commutativity once (the either-order flag in `joins_as_both_less_the_shared`). It has
no associativity, idempotence, absorption or De Morgan test. BO10 adds them.

### 2.4 Perf traps (READ)

- Up to three Boolean attempts, each with whole-body copies and `rebuild_native`; a witness copy per
  subtraction or intersection; a trial copy and a Level2 check per Boolean (`matched_tolerances`).
  None is ported (R2, R3).
- `split_body` runs two Booleans on copies (`split.rs:483-484`); `both_sides` copies per side
  (:383). BO15 runs one pass and selects twice.
- Proto-vertex creation scans linearly (`proto.rs:4224`, `PT/p3.rs:2360, :2478`): O(n²) in merged
  points. scx's stage 3 uses a spatial index (BO1).
- `split_faces` loops tools × faces (`split.rs:282`); `trim_sheet` sorts a row on every insert.
- Curves are classified against faces twice (`TI/p3.rs:1635-1638`).
- Fixed sampling: curved strip PIECES 32, EDGE_PIECES 16, COMMON_POINT_STEPS 32, MINIMAX_STEPS 64.
  scx replaces each with a certified bound or the march's own step control.

### 2.5 Geometry it needs

| Piece | Needs | Analytic now? |
|---|---|---|
| Face pairs on NURBS faces | SM9 (SM1, SM6b, SM8 for the general intersector) | no |
| Imprint of a curve with no exact pcurve | the step-5 pcurve fitter | no |
| Classify on NURBS faces | step 5's ray against NURBS (or `sides_along` alone where every piece has a section) | no |
| Plane against any surface (sections) | `scx_ssi::analytic` for quadrics; SM9 for NURBS | quadrics yes |
| Tangent contact by tracing | SM6b | stated facts and closed forms yes (SM2) |
| 2-D Boolean of coplanar regions | `scx-planar` crossings (PL1) and PL6 | yes |
| Interference | stages 1-2 and BW3's contact bounds | yes |
| N-ary and cellular merge | the slice's stages, generalised | yes |

### 2.6 Proposed shape (JUDGED)

All entries take finished `&Brep` operands of one document and `&mut Cx`, and return `Done<C>`.
`BooleanError` stays the crate's one error type; new causes are added to the existing
`BooleanRefusal`, `BooleanUnsupported` and `BooleanDefect` (04 row 135).

```rust
// The existing entry (BO1), unchanged.
pub fn boolean(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// N-ary: one operation over many operands, in the order given (BO11, then BO12).
pub fn boolean_all(first: &Brep, rest: &[&Brep], op: BooleanOp, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// The cellular merge (BO12): every operand imprinted on every other, each cell labelled by the
// operands it lies in. A Boolean is a selection over cells.
pub fn merge(operands: &[&Brep], cx: &mut Cx<'_>) -> Result<Merged, BooleanError>;
pub struct Merged { pub body: Brep, pub changes: Changes, pub cells: Cells, pub report: Report<NoItems> }
impl Merged {
    pub fn select(&self, keep: &dyn Fn(Membership) -> bool, cx: &mut Cx<'_>)
        -> Result<Done<NoItems>, BooleanError>;
}

// Imprint (BO13): the tool's intersection with the target written into the target's faces and
// edges; nothing is removed. `Dimension` says what is imprinted: edges only, or edges and the
// tool's faces that lie on the target.
pub fn imprint(target: &Brep, tool: &Brep, what: Imprint, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// Split (BO15): both sides from one pass. `Tool` is a surface (bounded by the body) or a sheet,
// whose sides are named by one of its faces' normals (`split_body_by_sheet`).
pub fn split(body: &Brep, tool: &Tool, cx: &mut Cx<'_>) -> Result<Done<Sides>, BooleanError>;
pub enum Tool { Surface(Surface), Sheet { sheet: Brep, facing: FaceId } }
pub struct Sides { pub front: Vec<RegionId>, pub back: Vec<RegionId> }

// Sections (BO14): the sheet (or wire, for a sheet operand) where a surface meets the body.
pub fn section(body: &Brep, by: &Surface, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Trim a sheet by a sheet (BO16): the connected regions of `sheet`, cut where `by` meets it, that
// lie nearest the points in `keep` (as `trim_sheet` picks). A point as near two regions is
// Refused(AmbiguousPick { point }); a cut that separates nothing is Refused(DoesNotCut).
pub fn trim(sheet: &Brep, by: &Brep, keep: &[Point3], cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// The 2-D Boolean of two coplanar sheets (BO17).
pub fn planar(a: &Brep, b: &Brep, op: BooleanOp, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Intersection without a build (BO18): the curves and points where two bodies meet, as a wire body.
pub fn intersect(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Done<NoItems>, BooleanError>;

// Interference (BO19): a decided answer, never a body.
pub fn interference(a: &Brep, b: &Brep, cx: &mut Cx<'_>) -> Result<Interference, BooleanError>;
pub enum Interference { Apart { gap: Bracket }, Touch { at: Vec<Contact> }, Overlap { depth: Bracket } }

// Stated incidences (BO21): facts the caller asserts, each checked; a false one is Refused(FalseFact).
pub fn boolean_given(a: &Brep, b: &Brep, op: BooleanOp, facts: &Incidences, cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;

// Curves lying on one face (BO13b): imprinted, the regions they bound returned for picking.
pub fn imprint_curves(body: &Brep, face: FaceId, curves: &[Curve], cx: &mut Cx<'_>)
    -> Result<Done<Regions>, BooleanError>;

// Several faces cut by several tools in one pass (BO15b).
pub fn split_faces(body: &Brep, faces: &[FaceId], tools: &[Tool], cx: &mut Cx<'_>)
    -> Result<Done<NoItems>, BooleanError>;
```

- **Proof is the default, not a mode.** The pin's `boolean_with` takes `Contract::{Closed,
  Proved}` (`O/body.rs:645`): Closed checks topology only; Proved requires each solid operand
  proved valid and facing out, and the result proved likewise. scx has no `Contract` parameter.
  Operands are finished bodies (`scx-brep`'s `finish` holds the topology and the outward sense
  as data), and `boolean` always gives Proved's result: a solid result is a closed manifold that
  `finish` accepts and its outward sense is established from the operands' senses and the
  selection, not assumed. A result that cannot be established is a typed `Defect`, never a
  weaker result. The full geometric check stays outside the operation (04 row 107's result
  contract), and every test runs it.
- **The empty result is refuted (ported postcondition).** When the selection leaves no material,
  `boolean` checks the pin's witness before returning it: every vertex of each operand is
  classified against the other (charged to the budget, a cost linear in vertices), and one that
  shows material must remain (for a subtraction, a vertex of `a` strictly outside `b`; for an
  intersection, one strictly inside both) makes the result `Defect(EmptyRefuted { at })`. The
  pin returns this as an error to the caller (`Error::EmptyRefuted`); in scx it is a defect,
  because it means the pipeline is wrong, not the input.
- **The same-body shortcut** (BO20) is inside `boolean`: when `a` and `b` are the same `Brep`
  (`ptr::eq`, or equal body ids in one document after 04's id rules), union and intersection
  return `a` and subtraction the empty body, with no stages run. Two bodies that are equal but not
  the same are run through the pipeline; BO10's idempotence tests prove the pipeline gets them
  right.
- **Imprint: what exists and what is left.** `scx-boolean/src/imprint.rs` (1,227 lines) is the
  slice's stage 5, crate-private: it writes the interference and face-face results into
  `a.edit()`. BO13 adds the public entry that stops there (finishing the imprinted Draft with no
  selection), the `Imprint` choice (edges only, or edges and the tool's faces lying on the
  target), `Done.changes` as the pin's `(Body, bool)` (the bool is "changes not empty"), and
  tests. That is small, so BO13 runs before BO12 and accepts the rework when BO12 generalises the
  stage beneath it; the entry and its tests are unchanged by that rework.
- **Imprint order.** 01 says "imprint in a stated dimension order". The rule: vertices first, then
  edges, then faces, each in id order, so an edge's cut points exist before the edge is cut. That
  is already the slice's stage order; `imprint` stops after stage 5.
- **Membership** is a small bit set of operand indices, so a cell's label is exact. A cell that is
  `On` a coincident face carries the side of each operand's material, as BO4 already records.
- **Non-manifold results.** Sections, trims and the cellular merge produce sheets, laminas and
  regions that are not one manifold solid. `Unsupported(NonManifold)` stays only for what scx-brep
  cannot represent; everything 04 row 16 allows is built (BO25).

## 3. `scx-make`: revolve, sweeps, prisms, faces

### 3.1 What the pin does (READ)

**Revolve.** `analytic_revolution.rs` (1,041), `rotational_sweep_geometry.rs` (1,736) and
`PC/p2/rotational.rs` (366). A partial turn (`PC/p3/sweeps.rs:618`) splits curves on the axis
(WR-590), then runs the generic topology sweep with the rotational geometry; every edge builds the
B-spline surface of revolution, and the analytic candidate is accepted only if corners, start
tangent, turn sense and 16 sampled side lengths agree within 1e-3 (:172). A line perpendicular to
the axis gets a bilinear plane swollen by 1.1 (:907). A full turn (`:474`) tries the analytic
surface per curve and falls back to `SurfOfRevolution`, swallowing that failure (:579); spline ends
near the axis are snapped onto it (#17204); shells are stitched and nested (#478). Lessons: #545,
#1463 and #1490 (never inside out), #17085 (a cone below MIN_HEIGHT is refused), #1491.

**The topology sweep** (`topology_sweep/`, about 8,000 lines with `store.rs` 2,993, `engine.rs` 2,034
and `access.rs` 890) is the generic engine: a `SweepAccess` trait of about 60 methods, an
edge-to-face store, merge, and the whole-body sweep (`engine.rs:1171`) that runs HCR (0.001, 5°) to
find the silhouette, sweeps it, moves back faces and merges at 20°.

**Path sweeps.** `sweep_placed` (`O/sweep.rs:952`) tries a line path (linear sweep), an arc path
(rotational), a path of lines and arcs (`sweep_along_segments`, each piece built separately, then
`copy_faces`, planar caps and stitch) and otherwise the curve sweep (`construct.rs:869-1061`: a
traced NURBS net, the exact procedural definition kept if its bound is within ZONE_TOL_3D, else a
fitted one, both errors swallowed). The cornered sweep (`cornered.rs`, no ops caller) mitres by
rotation θ/2 stretched 1/cos(θ/2). The planar path (`PC/p2/planar_path.rs`) sweeps each piece,
trims mitres by `brep_intersect` and unions the pieces (WR-780, WR-793, WR-400). Stations
(`stations.rs`) give closed-form rotation-minimising frames on lines, arcs and helices; the
controlled sweep skins them cubically, uncertified (#13637), refusing corners and closed paths
(#13638, #13639). The screw sweep (`screw.rs`) shrinks spans by ×0.99 until the lift error is within
tolerance, carrying a `ScrewMotion` definition (#2572).

**Radial sweep** (`local_operation.rs:404`, cones only): outer and inner faces and annular caps
built separately, one stitch at the input bound, an outward check, then a Boolean subtract.

**Prisms and faces.** `extrude_region` (`K/prism/extrude.rs:37`), `sheet_regions`
(`K/prism/sheet.rs:115`, the only HashMap in these files), `create_planar_faces`
(`api_trim_ops.rs:242`), `face_on` (`sheet.rs:83`).

### 3.2 Idiom hazards (READ)

| File | .ok() | unwrap_or | Err(_)=> | let _ = | &mut Brep/Kernel | literals |
|---|---|---|---|---|---|---|
| `PC/p1/primitives.rs` | 7 | 3 | 0 | 0 | 67 | 4 |
| `api_primitives.rs` | 0 | 2 | 0 | 2 | 27 | 5 |
| `api_brep.rs` | 0 | 5 | 0 | 0 | 22 | 0 |
| `screw.rs` | 0 | 4 | 0 | 0 | 13 | 3 + ~10 inline |
| `store/classification.rs` | 13 | 0 | 0 | 1 | 3 | 0 |
| `PC/p3/construct.rs` | 6 | 3 | 1 | 3 | 0 | 0 |
| `O/sweep.rs` | 1 | 6 | 1 | 0 | 4 | ~25 |

Plus `ignored()` swallows: `analytic_revolution` 9, `primitives` 8, `PC/p3/sweeps` 5.

The worst sites:

1. `classification.rs:130`: copies the whole B-rep per vertex-face gap query; error swallowed.
2. `construct.rs:991, :1000`: exact and fitted definitions dropped by `.ok()`; `:731` a reskin
   failure ignored.
3. `rotational_sweep_geometry.rs:191`: `Err(_) => Ok(true)`, a failed line test reads as a line.
4. `engine.rs:155`: a failed cancellation check reads as "not cancelled".
5. `K/sweep/extrude.rs:98`: any error, including cancellation, becomes `InvalidInput`.
6. `PC/p3/sweeps.rs:579`: a face silently dropped.
7. `engine.rs:1219-1222`, `:1009`: HCR at 0.001 and 5°, merge at 20°; `planar_path.rs:641` stitch at
   0.01; `O/sweep.rs:134-149` PATH and DRAFT constants 1e-6 to 1e-12.
8. `rotational_sweep_geometry.rs:907, :967`: swell 1.1, normal agreement 0.8.
9. `access.rs`: six reads through `&mut self`; `bounding_box` (:620) with an out-param.
10. `api_brep.rs:280`: `cap_ends: i32` (none, at curves, at offset, both).
11. `TopologySweepOptions::self_intersection` stored and never read (`topology_sweep.rs:161`).
12. `set_doing_boolean` at 13 sites; `set_make_analytics`, `set_same`, `set_higher` setters.

### 3.3 Truths and tests (READ)

The truth tests to port (closed forms; the rest check face kinds, layout or the pin's own frames):

1. `tests/screw_sweep.rs:271` square tooth by Pappus, with the wall deviation within tolerance.
2. The ops 26-case closed-form table (`O/../tests/sweep.rs:188-505`, relative 1e-9 at :543):
   rectangles revolved full and quarter (:298, :302), a circle swept straight and along an arc
   (:307, :320), a bent path (:343), screw threads (:474, :492), extrudes and drafted frustums
   (:199, :224-281).
3. `tests/wr_590_revolve_splits_curve_on_axis.rs:85`: three faces and the Pappus volume.
4. `api_brep.rs:1735`: a whole sphere's volume.
5. `tests/sweep_cornered.rs:112` (area × length on an L), `:178` (the end-scale integral), `:363`.
6. `tests/sweep_stations.rs:74`, `:505`: the helix frame turns by its torsion (1e-12).
7. `tests/issue_2905_revolved_vase.rs:158`: the meridian volume.

### 3.4 Perf traps (READ)

- Every revolve edge builds the B-spline surface even when the analytic one wins, then samples 32
  lengths per side and 8 plane points. R4 removes both.
- The curve sweep fits the NURBS net and also builds the procedural definition.
- The whole-body sweep runs HCR and a merge per repetition, up to 4,096.
- The screw shrink loop runs up to 20,000 times, with an unbounded inner loop down to 1e-4.
- Segment and cornered sweeps build each piece as its own body, then copy and stitch globally. MK5
  builds one Draft with the shared section edges known.
- The planar path runs a Boolean union per piece and an SSI per mitre. MK5 cuts mitres by the
  closed-form intersection of each wall with the mitre plane.

### 3.5 Geometry it needs

| Construction | Surfaces | Analytic now? |
|---|---|---|
| Revolve of lines and arcs | plane, cylinder, cone, sphere, torus; exact pcurves (a meridian is a line in uv) | yes (MK4) |
| Revolve of other curves | the procedural revolution kind (step 5) | no (MK4b) |
| Sweep of a line-and-arc profile along a line-and-arc path | planes, cylinders, cones, tori; mitre edges are conics (plane against quadric, `scx_ssi::analytic`) | yes, when every wall is a quadric or torus (MK5) |
| Sweep along a general path, controlled sweep | the procedural sweep kind (step 5) | no (MK7b) |
| Screw sweep | the procedural sweep kind with a screw motion | no (MK6) |
| Whole-body sweep | HL5's silhouettes, the cellular merge, procedural kinds | no (MK8) |
| Radial sweep of cones | cones and planes | yes (MK13) |
| Prisms and sheets from planar regions | planes, cylinders (arcs) | yes (MK14) |
| Faces on surfaces | any `Surface`; iso-line edges are exact on every kind | yes (MK15) |

### 3.6 Proposed shape (JUDGED)

Constructors as in MK1-MK3: inputs are validated values (`scx_planar::Region`, `Frame`, `Span`),
the document is `&Document`, and the result is `Made<R>` with a typed face record.

```rust
pub fn revolve(profile: &Region, about: &Axis, turn: Turn, doc: &Document)
    -> Result<Made<RevolveFaces>, MakeError>;
pub enum Turn { Full, Partial { from: Angle, to: Angle } }
pub struct RevolveFaces { pub start: Option<Vec<FaceId>>, pub end: Option<Vec<FaceId>>, pub walls: Vec<Vec<FaceId>> }

pub fn sweep(profile: &Region, path: &Path, frame: PathFrame, doc: &Document)
    -> Result<Made<SweepFaces>, MakeError>;
pub fn frames(path: &Path, law: &FrameLaw, cx: &mut Cx<'_>) -> Result<Frames, MakeError>;
pub fn screw(profile: &Region, motion: &Screw, doc: &Document) -> Result<Made<SweepFaces>, MakeError>;
pub struct Screw { /* axis, pitch, turns, law: Option<RadialLaw>, walls: Walls { Pieces, Joined } */ }
pub struct PathFrame { /* orientation, twist, end scale, rails: Vec<Rail>, rail_fit: RailFit */ }
pub fn ruled(a: &Curve, b: &Curve, doc: &Document) -> Result<Made<FaceId>, MakeError>;
pub fn sweep_solid(tool: &Brep, path: &Path, cx: &mut Cx<'_>) -> Result<Done<NoItems>, MakeError>;
pub fn radial(cone: &RadialCone, doc: &Document) -> Result<Made<RadialFaces>, MakeError>;
pub fn prism(regions: &[Region], frame: &Frame, span: Span, doc: &Document)
    -> Result<Made<Vec<ExtrudeFaces>>, MakeError>;
pub fn sheet(regions: &[Region], frame: &Frame, doc: &Document) -> Result<Made<Vec<FaceId>>, MakeError>;
pub fn face(surface: &Surface, bound: &FaceBound, doc: &Document) -> Result<Made<FaceId>, MakeError>;
```

- **Profiles touching the axis.** A profile segment on the axis is not a face; its ends become
  pole vertices (a sphere's or cone's apex) with `OnChart` pcurves (SM1, SM1b). A profile curve
  that touches the axis at an interior point (a circle tangent to the axis, revolved to a horn
  torus) is split there and the touch point becomes a pole vertex shared by the two pieces' faces,
  as the pin does (WR-590, `PC/p3/sweeps.rs:181-194`, `split_curves_on_axis`). Only a curve that
  truly crosses the axis, so the body would pass through itself, is `Refused(CrossesAxis { at })`;
  the pin refuses the same case (`SweepFailure::EdgeCrossesRotationAxis`,
  `K/sweep/rotational_sweep_geometry.rs:364`). WR-590's test is ported as a build test (Q3).
- **The plane snap rule** (Q10). A profile line whose two ends lie at axial heights within the
  document's coincidence of each other revolves to a plane normal to the axis, at the start
  vertex's height; the end vertex lies within coincidence of that plane and its measured distance
  is recorded as the vertex's accuracy. Any other line revolves to a cylinder (both ends at one
  radius within coincidence) or a cone, however low. No height floor applies.
- **Caps.** `RevolveFaces::start` and `end` are `None` for a full turn. An open (uncapped) result is
  a separate constructor (`revolve_sheet`), never a `cap: bool`.
- **Mitres.** A corner in a line-and-arc path is a mitre plane through the corner bisecting the
  turn. Each wall meets it in a conic (or a line), computed in closed form; the wall is trimmed
  there with an exact pcurve. A path that turns back within its profile's reach is
  `Refused(PathTooTight)` with the radius that fits (a `Resize` remedy).
- **No stitching.** Like MK3, every construction knows its topology: the shared edges are made
  once and used by both faces, so `scx-stitch` is not called.
- **Errors.** `MakeError` gains the new causes in its existing `Refusal`, `Unsupported` and
  `Defect` (MK1). A construction that needs a step-5 kind not yet built is
  `Unsupported(NeedsKind { kind })`, so callers can see the boundary.

## 4. `scx-make`: loft, skins, patches

### 4.1 What the pin does (READ)

**Section pairing** (`loft_sections.rs`, 3,762): each section wound like the one before; vertices
paired by a cyclic least-cost alignment (distance after moving to the centroid and scaling to unit
area, plus tangent difference, `UNPAIRED` 0.1 per lone vertex), searched as a shortest path for
every start by divide and conquer (`best_walk`), capped at MAX_CELLS 4e6 and then a banded walk
of 8 passes. A lone vertex cuts the other section at the same share of length (#16943). Full
circles are cut at the neighbouring section's directions.

**The loft certificate** (`K/nurbs/certificate.rs`, 2,021): an upper bound on the distance from
the skin to a rail between stations, holding whatever nodes the untrusted projection gives. τ is
cut at both knot sets; the patch is restricted by de Casteljau and read along its diagonal by the
Farouki-Rajan product; the homogeneous difference `S_h W_r − R_h W_s` is bounded by its hull box
plus Higham γₙ terms; failing pieces halve (MAX_HALVINGS 12, MAX_SEGMENTS 65536). The rail loft
adds a station at each failing span's middle and re-skins, up to 257 wires, then refuses
(`RailPastStationCap`); no tolerance is loosened. Also `on_circle`, `turn_about_axis`,
`meridian_collapse`. Lessons #12920, #17790, #17907, #18018: why these are certificates and not
sampled tests.

**Rails and warp** (`loft_rails.rs`, `warp.rs`): one or two rails use an affine station map; three
or more fit an affine move then an inverse-multiquadric warp (#12188). The two-rail sweep is The
NURBS Book §10.4.

**Coons, Gordon, n-sided.** Coons (`sheet.rs:469`) makes opposite sides compatible and calls the
upstream patch; no certificate; folds are looked for on a 5×5 normal grid. The ops `boundary`
(`O/surface.rs:251`) takes three or four curves to a Coons patch with no stitch; five or more go
to `n_sided` (:256), whose patches are stitched into one sheet (`sheet_of`, `stitch_faces` at
:326). (The review said the Coons boundary stitches; it is the n-sided path that does.) MK11
builds the n-sided sheet with its shared sides known, so it needs no `scx-stitch`; RP6, which
sews a patch into a body, does. Gordon
(`K/nurbs/gordon.rs`) is `L1 + L2 − T` with crossings from curve intersections, refitting an
incompatible network by a monotone map doubling from 8 to 64 points per span, then certified and
refused past the tolerance. n-sided (`n_sided.rs`) fills N quads round a centre; G0 is certified,
G1 is not; rational curves are refused.

**Section blend** (`section_blend.rs`): Hermite end rows G0-G3, interior derivatives by finite
differences.

### 4.2 Idiom hazards (READ)

- `PC/p2/skin.rs:277-302, :323`: the global mode flags (§1.1).
- `skin.rs:123-135`: four `&mut Vec` out-params, `curves: &mut [Option<Curve>]`, a positional
  `cap_ends: bool`.
- `skin_flat_walls.rs:41, :51-57, :84-93`: `.ok()` chains; a failed replacement is skipped.
- `loft_rails.rs:1311, :1328`: projection errors dropped; `certificate.rs:514` falls back to the
  line midpoint (by design: the bound holds for any node).
- `O/sweep.rs:1269, :1310`: errors swallowed to fall back to the skin; `:1001` `Err(_) =>
  Error::Argument`.
- `loft_sections.rs:245, :326, :826, :1240-1259`: default directions and lengths by `unwrap_or`.
- `api_surface_ops.rs:177, :226`: approximation at a literal 0.001; `skin.rs:98` planarity at
  `tol*1000`.
- `loft_rails.rs:566, :1141`, `warp.rs:68, :222, :278`: collapse and pivot floors 1e-6 to 1e-12.
- `OffsetRuleOptions::surface_intersection: bool` (a mode); `create_coons_patch(tol, true, …)`.

### 4.3 Truths and tests (READ)

1. `gordon.rs:866`: a network of lines is the bilinear patch.
2. `O/surface.rs:543`: three arcs bound a sphere octant (side points at |r| = 10).
3. `O/../tests/sweep.rs:1359`: a loft through stacked circles is the cylinder (25π·12 to 1e-9);
   `:1334`, `:1398` closed-form centres and end weights.
4. `tests/loft_rails.rs:147`: similar sections on a straight rail give the frustum.
5. `tests/sweep_two_rails.rs:130`: diverging line rails scale the profile in closed form.
6. `tests/section_blend.rs:132`, `:47`: quadratic sections blend to the quadratic; two G0 sections
   give the ruled surface.
7. `certificate.rs:1658`, `:1678`: an exact arc is proved on its circle; a bulge is refuted.
8. `n_sided.rs:276`, `O/surface.rs:668`: a pentagon of lines fills flat.
9. `tests/skin_parallel_sections.rs:87`: the skin stays between its end planes.

The certificate is compared with an independent sample only through `sampled_off`
(`tests/loft_rails.rs:119`), which uses the kernel's own closest point. MK10's test samples with
the testkit's exact evaluation instead. Most `loft_sections` tests (37) assert the pin's pairing.

### 4.4 Perf traps (READ)

- Each rail-loft round re-skins every wire and projects every station onto every rail: about
  W·S·R closest-point calls per round, up to 257 wires. MK10 re-skins only the spans the new
  station changes where the interpolation is local, and certifies only those spans.
- Pairing visits up to 4e6 cells; a length bisection of 100 iterations per cut.
- Gordon rebuilds and recertifies on each refit doubling.
- `span_bound` allows 65,536 pieces per span, each with a projection.

### 4.5 Geometry it needs

All of §4 except pairing needs NURBS surface construction: global interpolation with given
parameters, skinning, knot insertion, degree elevation and compatibility, iso-curve extraction,
and Bernstein restriction and products for the certificate. `scx-nurbs` and `scx-nurbs-surface`
have evaluation and refinement; the interpolation and skinning are 01's "construction by
certified approximation" in step 5, which has no row yet (Q1). Pairing (MK9) needs only curves.
The Coons patch of four exact boundary curves needs only compatibility and the bilinear blend, so
it can start once knot insertion and degree elevation are in `scx-nurbs-surface`.

### 4.6 Proposed shape (JUDGED)

```rust
pub fn loft(sections: &[Section], how: &Loft, cx: &mut Cx<'_>) -> Result<Made<LoftFaces>, MakeError>;
pub struct Loft { /* rails, end conditions, closed, fit; #[non_exhaustive], with_* */ }
pub fn pair(sections: &[Section], budget: &mut Budget, stop: &Stop) -> Result<Pairing, MakeError>;
pub mod certificate { pub fn span_bound(/* skin, rail, stations */) -> Bound; }
pub struct LoftFaces { pub start: Option<FaceId>, pub end: Option<FaceId>, pub walls: Vec<Vec<FaceId>> }
```

- `Made::body`'s faces carry the certificate's bound as their recorded accuracy. A span the
  certificate cannot prove within the document's resolution under its fixed budget is recorded as
  a `Raise` with the bound reached (R6), never a refusal; `RailPastStationCap` becomes that.
- `Pairing` carries its status: exact within the fixed per-operation cell limit (the pin's
  MAX_CELLS), else the banded answer, `Proven` when its walk avoids the band's edge and `Unproven`
  with its cost as the bound otherwise (Q5). The limit is a constant, not the remaining budget, so
  the answer is a function of the input.
- Flat walls are planes because the section pair is planar and coplanar with the rail, decided
  before the skin is built (R5), not by replacing a built B-spline.
- `scx_surface::{coons, gordon, n_sided}` return a `NurbsSurface` with its certified deviation, and
  `scx_make::face` bounds it.

## 5. `scx-deform`

### 5.1 What the pin does (READ)

**The app cannot deform a body today.** No kernel function maps a body through a volume and ops has
no deform entry. Evidence (grep for `BendVolume`, `TwistVolume`, `UnbendVolume`, `SrfInVolume`,
`CrvInVolume` followed by a constructor, outside their own files): seven sites, all under
`#[cfg(test)]` (`K/heal/stitch/bounds.rs:1762`, test module from :1548;
`K/curve/crv_on_surf.rs:2863, :2890, :2956`, from :2311; `K/curve/curve/p2.rs:4876`, from :4607;
`K/surface/surface/p5.rs:2725`, from :2661; `K/surface/surface/p2.rs:8414, :8614`, from :8023).
The other files that name the kinds match on them to read or evaluate. The pin has volume geometry kinds that are read from records and evaluated:
`BendVolume` (`bend_volume.rs`, 1,973), `TwistVolume` (1,208), `UnbendVolume` (1,671), `SrfInVolume`
(1,370), `CrvInVolume` (`K/curve/crv_in_volume.rs`, 1,978), the generic `Volume` chain
(`surface/volume/p1.rs` 3,278, `p2.rs` 2,057). `BSplineVolume` is a stub that always returns
`SM_ERR`: upstream builds without `USE_VOLUMES`, and the port follows the build
(`K/surface/b_spline_volume.rs:32-58`; `load_nlib_transform` at :191 always errs). Sheet-metal develop does not use the unbend
volume; it unrolls with its own maps: `Wrap` (`K/develop/fold.rs:60-90`, the neutral-arc wrap)
and `bent_across` (:412, a `Rigid3`), and no file in `K/develop/` names a volume. `api_transform.rs:152-157` refuses
to transform a standalone volume curve or surface.

**The maps.** Bend sends (u, v, w) to (u cos(v/d), u sin(v/d), w), singular on the axis; unbend is
its inverse; twist rotates about the axis by an angle proportional to height. A `Volume` holds an
orienting transform and a chain of next maps, with derivatives by the chain rule. Inversion is a
guess then a global or local point solve. Exactness is decided by sampling a 6×6 grid and
recognising three bend cases (plane normal to the axis → plane; plane parallel to the neutral
plane → cylinder; plane normal to the binormal → plane through the axis), their unbend inverses,
and one twist case (plane normal to the axis → rotated plane). Lessons: WR-45 (third-derivative
terms wrong upstream), WR-46 (sign of Duw), WR-59 and WR-60 (box bounds and trimming across the
period), #17549, WR-49, WR-33.

### 5.2 Idiom hazards (READ)

`volume/p2.rs` has 12 `unwrap_or`, mostly `unwrap_or(Vec3::UNDEF)` (:228, :254, :1021, :1106,
:810); `bend_volume.rs:178` and `unbend_volume.rs:177` map an error to `(Vec3::Z, false)`; `let _ =`
drops statuses at `bend_volume.rs:648, :741, :847`; `volume/p1.rs:404` drops a next map it cannot
copy. `EvalOptions` (`p1.rs:202`) has four behaviour bools; owner flags are integers. No statics or
`&mut Brep`.

### 5.3 Truths and tests (READ)

85 unit tests, none of which deforms a body. Truths to port: the closed-form maps
(`bend:1567`, `twist:1078`, `unbend:1488`), finite-difference derivatives (`bend:1580`, `twist:1089`,
`unbend:1500`, `p1:2918`, `p1:3053`, `p1:2660`) and box containment (`bend:1775`).

### 5.4 Perf traps (READ)

Every domain test copies the volume and builds a curve in it (`p1.rs:833-895`); compound
evaluation walks the chain per point; the exactness test costs 36 samples per call.

### 5.5 Proposed shape (JUDGED)

```rust
pub enum VolumeMap { Bend(Bend), Twist(Twist), Unbend(Bend) /* chain: Vec<VolumeMap> */ }
pub fn deform(body: &Brep, map: &VolumeMap, cx: &mut Cx<'_>) -> Result<Done<NoItems>, DeformError>;
```

- The maps are geometry: step 5's volume maps, in `scx-surface` (01 §1.1). `scx-deform` holds only
  the body operation.
- Each face goes to its exact image where one exists, decided by the face's kind and its frame
  relative to the map (R4, R10), never by sampling: the pin's three bend cases, their inverses, and
  the twist cases (a plane normal to the axis; a cylinder or cone coaxial with the twist axis maps
  to itself, which the pin misses). Lines map to lines or arcs by the same rule.
- Every other face becomes the procedural in-volume kind (step 5), evaluated exactly through the
  map. Its pcurves are unchanged: the map acts on 3-D, and a face's uv stays the source's.
- Topology is unchanged, so the body is the source's Draft with new geometry: no stitch.
- A body that meets the bend axis is `Refused(MeetsAxis)` (the map is singular there).

## 6. `scx-repair`

### 6.1 What the pin does (READ)

There are two healers, and neither uses a Boolean.

1. **The certified healer** (`K/heal/tolerance_defects.rs`, 1,286), solidcore's own, the one the app
   uses (`O/heal.rs:183` from the app's tolerance heal, used by STEP import and Repair):
   1. cut zero-length Bézier pieces from B-spline edges, accepted within the edge tolerance and
      within half of it to the faces;
   2. cluster vertices joined by edges shorter than their tolerance (union-find), target the
      cluster box's centre;
   3. squeeze an edge only if interval de Boor enclosures put its whole curve within its tolerance
      of that point, re-end neighbouring edges there (lines rebuilt, B-spline end control points
      moved), each new curve certified within half its tolerance of its faces (GAP_SHARE 0.5);
   4. collapse two-edge faces whose curves are certified coincident, keeping the larger tolerance;
   5. repeat 3-4 at most faces+1 rounds.

   Refusals are reported and never retried, and no tolerance grows. The ops wrapper heals a copy,
   runs the full check on both, and keeps the copy only if no tolerance grew, it fails fewer tests,
   and no failure class got worse (`O/heal.rs:236-311`).
2. **The SMLib healer** (`heal_data/`, `gap.rs`, `face_props.rs`; about 22,000 lines), ported for
   parity, called only by `heal_made` (`K/topology/brep/p10.rs:546`, from
   `solidcore-convert/src/brep.rs:317`) on untrusted records. It widens tolerances by a model-size
   ladder and by `gap_gain * gap`. How many corpus records it actually fixes is not known (RP0).

The app's **Repair Part** (`A/repair.rs`) runs, in order:

0. the certified heal above, when `heal_first` is set;
1. an analytic rebuild: cylinder, cone, sphere and torus faces that SMLib refuses on their NURBS
   form are rebuilt as the analytic surface (`A/repair.rs:11-14`);
2. sewing within the gap: free edges paired within `gap` are glued, through `stitch_no_worse`, with
   each glued edge's tolerance at least twice the gap (`A/repair.rs:17`, the stitch tolerance
   `widest * 1.05` at :244);
3. hole patches, when `patch_holes` is set: free loops filled by `A/fill.rs:100` (a plane face, or
   a patch meeting its neighbours G0 or G1), then sewn again.

The steps are chosen by `RepairOptions { gap, patch_holes, heal_first }` (`A/repair.rs:51`). Beside
it, `A/heal.rs` merges faces on one surface and edges on one curve, and `A/delete_face.rs` deletes
faces and heals the opening (step 8's `scx-local`).

### 6.2 Idiom hazards (READ)

- `tolerance_defects.rs`: 7 `&mut Brep` or `&mut Kernel`, `:380` a failed line becomes "no curve",
  `:447`, `:493`, `:905` errors become an infinite gap; `:548` a literal `1e-12 * size`.
- `O/heal.rs:239-240`: `.ok()` on lookups, so a removed entity skips the growth check.
- The SMLib healer: `ignore_status` 9 times; 18 `let _ =` in `p2.rs`; 42 bool or tri-state fields;
  `HealerOpType` by integer code; `gap.rs` reads through `&mut Brep`.
- The app's `static AtomicBool` and `thread_local`, switched by the global
  `set_heals_tolerance_defects` (`A/lib.rs:623`): whether import and Repair heal depends on a
  process-wide setting, not on the call.
- `RepairOptions`' two bools (`patch_holes`, `heal_first`) choose which steps run; with `gap` they
  make one entry with four behaviours.
- `A/repair.rs:17`, :244: the sewn edge's tolerance is set by a factor (2× the gap, 1.05× the
  widest), not by the gap measured.

### 6.3 Truths and tests (READ)

The 18 tests in `O/../tests/heal.rs` are truths: `heals()` (:481-527) asserts that the full check
passes, exact counts, no tolerance grew, and `|ΔV| ≤ area × largest bound`. Port all of them, in
particular :534, :575, :611, :632, :958, :827, :1062; :1402 tests the SMLib path and is ported
only if Q7 keeps it. The 66 kernel unit tests and the goldens assert the pin's state.

### 6.4 Perf traps (READ)

Two full checks per accepted heal; `collapse_short_edges` restarts the edge scan after each pass
with a linear `find` per edge (:728-750); faces+1 outer rounds.

### 6.5 Proposed shape (JUDGED)

Each repair the app runs is its own entry, so no option bool chooses a step (the pin's
`RepairOptions { gap, patch_holes, heal_first }`, `A/repair.rs:51`). The app's Repair Part is an
ordered list of steps, run in order, each step's `Done` the next step's input and its `Changes`
composed.

```rust
// The certified healer (RP1-RP3): short-edge clusters, slivers, empty pieces.
pub fn heal(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<HealCause>, RepairError>;
// A face on a NURBS form that is exactly an analytic surface goes as that surface (RP7).
pub fn rebuild_analytic(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<RebuildCause>, RepairError>;
// Free edges that pair within `gap` sewn (RP5), through scx-stitch.
pub fn sew(body: &Brep, gap: Length, cx: &mut Cx<'_>) -> Result<Done<SewCause>, RepairError>;
// Loops of free edges no pair explains filled by a patch and sewn in (RP6).
pub fn patch_holes(body: &Brep, cx: &mut Cx<'_>) -> Result<Done<PatchCause>, RepairError>;
// The app's Repair Part (RP8): the steps in the order given.
pub fn repair(body: &Brep, steps: &[Step], cx: &mut Cx<'_>) -> Result<Done<StepCause>, RepairError>;
pub enum Step { Heal, RebuildAnalytic, Sew { gap: Length }, PatchHoles }
```

- Items are the defects found (each short-edge cluster, each sliver, each empty piece, each
  free-edge pair, each hole, each face to rebuild) in id order, so the default reports the lowest
  failing one and `Skip` mode does the rest (01 §5.2). This replaces "reported and never retried"
  and the pin's "a heal that cannot run is skipped and the steps below go on" (`A/repair.rs:9-10`),
  which becomes `Skip` mode stated by the caller.
- **heal.** Every edit is certified locally (step 3's enclosure, the GAP_SHARE rule on SU6's
  bounds), so the result is valid by construction. The run-time check-compare of the ops wrapper is
  not ported; the tests assert it instead (Q8). No accuracy grows: a cluster that would need a
  larger accuracy is a `HealCause`. Lines and planes exactly; B-spline end control points moved; an
  arc re-ended stays an arc if both ends stay on its circle, which the enclosure decides.
- **sew.** Each pair of free edges whose certified gap is within `gap` becomes one edge, keeping one
  of the pair's curves, as the pin does. The kept edge's accuracy is the certified gap bound
  itself, recorded in `report.raised` with the pair: an accuracy the edge measurably has, which 01
  requires every writer to record before `finish`. The pin's factor (`SmBrep::GlueEdgesGeneral`
  gives at least twice the gap, `A/repair.rs:17`, and the stitch tolerance `widest * 1.05`, :244)
  is not ported (R3). The body's resolution and every other edge stay as they were. A pair beyond
  `gap` is a `SewCause::TooFar { gap: Bracket }` with a `Resize` remedy.
- **patch_holes.** A loop of free edges is filled: by a plane face when the loop lies on one plane
  within coincidence and its neighbours are that plane (exact, G0); otherwise by MK11's n-sided or
  Coons patch meeting the neighbours G1. G0 is certified; a G1 join that the certificate cannot
  prove is recorded `Unproven` with its bound (R10), the patch kept, as 01's Setback hole fill does.
  Extending the neighbours is not done (as in the pin). The patch is sewn in by `sew`'s rule.
- **rebuild_analytic.** The pin rebuilds a cylinder, cone, sphere or torus face that SMLib refuses
  on its NURBS form (`A/repair.rs:11-14`). scx accepts NURBS faces, so the reason is different:
  exact analytic faces make every later SSI and classify closed-form. A face is rebuilt only when a
  certificate proves its NURBS form is the analytic surface (MK10's `on_circle` and
  `turn_about_axis`, the same Bernstein bounds); a face the certificate refutes or cannot decide is
  left as it is and listed, never rebuilt by sampling.
- **Face and edge merge** (`A/heal.rs`: faces on one surface meeting along an edge become one; two
  edges on one curve meeting at a vertex nothing else uses become one) goes in `scx-brep` as Draft
  methods (BR11), not in the Boolean's build stage. Reason: the Boolean's build calls it at the end
  (SMLib's Booleans do the same), repair calls it, and step 8's local operations will; the lowest
  crate all three use is `scx-brep`, and the edit is pure topology over a test of equal surfaces
  and curves that `scx-brep` already defines. Faces split on purpose (the pin's split lines) are
  kept apart by a typed set of protected edges passed in, not a flag on the body.

## 7. Task table

Model: Opus where a correctness argument is the work (a proof of completeness, a certificate, an
order-independence argument); Sonnet where the work is porting tests, mapping or plumbing. "Edges"
names each new Cargo dependency; each must be legal under layers.txt after LY2.

| Id | Crate | Work | Needs | Edges | Model |
|---|---|---|---|---|---|
| LY2 | layers | layers.txt: `scx-deform` after `scx-hlr`, `scx-repair` after `scx-make`. 01's table is amended in this branch (rows for boolean, deform, make, repair; §7.2). New crates take test bodies from testkit | — | — | Sonnet |
| BO10 | boolean (tests) | The algebra suite on the testkit recipes (§8.1), including the coaxial shared-edge collar recipes in both operand orders and the #20408 collars (`tests/boolean.rs:50-75`) as a mandatory truth; judged by volume, area, the T10 oracle and the contract; run twice and compared by bits | — | — | Sonnet |
| BO10b | boolean | Fix what BO10 finds on the slice, before BO12 builds on it; with it, proof as the default (no `Contract`) and the empty-result refutation as a postcondition (§2.6) | BO10 | — | Opus |
| BO11 | boolean | `boolean_all`: n-ary union, subtraction and intersection as a balanced fold in operand order, work summed in order; port `union_all`'s closed-form tests | BO10b | — | Sonnet |
| BO12 | boolean | `merge` and `select`: the cellular merge over n operands (stages 1-6 generalised, membership per cell, two-sided faces and laminas by 04 row 16); `boolean` and `boolean_all` become selections; proof that a cell's membership, and the radial order of faces about a shared edge, depend on the geometry alone and not on operand order | BO10b, BO25 | — | Opus |
| BO13 | boolean | `imprint`: the public entry over the existing stage 5 (`imprint.rs`), finishing with no selection; the `Imprint` choice; `Done.changes` for the pin's `(Body, bool)`. Runs before BO12, rework accepted (§2.6) | — | — | Sonnet |
| BO13b | boolean | `imprint_curves`: curves lying on one face imprinted, the regions they bound returned (`split_face_by_curves`); curves with exact pcurves now, others after the fitter | BO13 | — | Sonnet |
| BO14 | boolean | `section` by a surface: sheet result for a solid, wire for a sheet; scoped to one face for `face_section`; plane against quadrics now, NURBS after SM9 | BO25 | — | Opus |
| BO15 | boolean | `split` by a surface or a sheet (sides named by a sheet face's normal): both sides from one pass; port `O/split.rs` tests that assert areas and volumes | BO12 | — | Opus |
| BO15b | boolean | `split_faces`: several faces by several tools in one pass; `Refused(ToolStopsShort { tool, face })` when nothing splits and a tool ends inside a face | BO15 | — | Sonnet |
| BO16 | boolean | `trim` a sheet by a sheet, keeping the regions nearest given points; `AmbiguousPick`, `DoesNotCut`; port `tests/trim.rs:371`, `:627` | BO12 | — | Opus |
| BO16b | boolean | `trim_by_dropped_curves`: curves dropped onto faces (SF1), imprinted (BO13b), regions kept by points | BO13b, BO16, SF1 | boolean → surface (exists) | Sonnet |
| BO17 | boolean | `planar`: the 2-D Boolean of coplanar sheets on `scx-planar` regions, faces built with exact pcurves | — | — | Sonnet |
| BO18 | boolean | `intersect`: the curves and points two bodies meet in, as a wire body (shells listing wire edges exist in `scx-brep`, `finish.rs:100, :182`), from stages 1-4 | — | — | Sonnet |
| BO19 | boolean | `interference`: apart (with a certified gap), touching (contacts), or overlapping (a certified depth); stages 1-2 with BW3's bounds | BW3 | boolean → bodyq (exists) | Opus |
| BO20 | boolean | The same-body shortcut; tests that equal but distinct bodies give the same results through the pipeline | BO10b | — | Sonnet |
| BO21 | boolean | `boolean_given` and `Incidences`: stated face-on-face and edge-on-face facts, each checked (`Refused(FalseFact)`), used to skip their SSI | — | — | Opus |
| BO22 | boolean | Tangent contact: stated facts, then closed form (SM2), then tracing (SM6b); lifts the BO7 limits on tangent pairs | BO21; tracing waits on SM6b | — | Opus |
| BO23 | boolean | NURBS faces: stage 4 on `General` (SM9), stage 5 pcurves by the fitter, stage 6 classify by step 5's ray against NURBS | SM9, the fitter, the NURBS ray | — | Opus |
| BO24 | boolean | Lift `Unsupported(Pcurve)` on analytic faces whose cut curve has no closed-form pcurve (a quartic on a torus) by the fitter | the fitter | — | Opus |
| BO25 | boolean | Non-manifold results: sheets, laminas, mixed regions in the build stage; `Unsupported(NonManifold)` kept only for what scx-brep cannot hold | — | — | Opus |
| BR11 | brep | Draft methods merging faces on one surface across an edge and edges on one curve at a free vertex, with a protected-edge set; called by the Boolean's build stage | — | — | Sonnet |
| SF1 | surface | Curves projected along a direction and dropped along the normal onto a surface (`project_curves`, `drop_curves`): closed forms now (a line or circle onto a plane, a line onto a cylinder or cone along its axis), the fitter for the rest. Home: `scx-surface`, because the map is curve against surface with no view and no topology; the restriction to a face's trim is `scx-bodyq`'s existing curve-onto-body projection (01 row for bodyq). Not `scx-hlr`, whose projections are view-dependent visibility | general curves wait on the fitter | — | Opus |
| DT4 | difftest | The step-7 outcome difftest: t_slice widened to every step-7 entry and constructor, each side judged against truth, pin defects in `pin_defects.rs`, work rows in `slice_work.txt` | BO11, MK4 (first rows); grows with each task | — | Sonnet |
| BB1 | bench | The perf rows of §8.3 before and after each step-7 task: work counts against the pin with side-channel instrumentation, no committed counters | DT4 | — | Sonnet |
| MK4 | make | `revolve` of line and arc profiles, full and partial: plane, cylinder, cone, sphere, torus built directly with exact and `OnChart` pcurves; segments on the axis and interior axis touches (WR-590, horn torus) become pole vertices; the plane snap rule; `Refused(CrossesAxis)` only for a true crossing; `revolve_sheet`; WR-590 ported as a build test | SM1b for poles (built) | — | Opus |
| MK4b | make | `revolve` of other curves onto the procedural revolution kind | step 5 procedural kinds | — | Sonnet |
| MK5 | make | `sweep` of a line-and-arc profile along a line-and-arc path: walls by kind, closed-form mitres, one Draft, no stitch; the planar path is this case | MK4 | make → ssi (mitre conics) | Opus |
| MK6 | make | `screw` on the procedural sweep kind: the radial law (`O/sweep.rs:1824-1871`), `Walls::{Pieces, Joined}` (the app's `join_spline_walls`), an exact cylinder for a profile line along the axis | step 5 procedural kinds | — | Opus |
| MK7a | make | `frames`: rotation-minimising frames in closed form on lines, arcs and helices, least rotation at corners; port the stations tests | — | — | Sonnet |
| MK7b | make | `sweep` along a general path and the controlled sweep on the procedural sweep kind, with `PathFrame`'s twist, end scale and one or two rails with `RailFit` (`A/sweep_path.rs:34-68`) | MK7a, step 5 procedural kinds | — | Opus |
| MK8 | make | `sweep_solid`: the whole-body sweep from HL5's silhouettes and the cellular merge | HL5, BO12, MK7b | make → hlr, make → boolean | Opus |
| MK9 | make | `pair`: section pairing by the cyclic least-cost alignment, exact within a fixed cell limit, else the banded answer marked `Unproven` (Q5) | — | — | Opus |
| MK10 | make | `loft` with the certificate: skinning through the step-5 constructor, `span_bound` ported, local re-skin per added station, `Raise` on an unproved span | MK9, step 5 surface construction | — | Opus |
| MK10b | make | Rails: one and two rails, the warp for three or more, the two-rail sweep; certified by MK10 | MK10 | — | Opus |
| MK11 | surface, make | Coons (four exact boundaries, compatibility, certified deviation), Gordon, n-sided (its sheet built with shared sides known: no stitch); `face` bounds them; G1 joins certified or `Unproven` | knot insertion and degree elevation in `scx-nurbs-surface`; Gordon also MK10's constructor | — | Opus |
| MK12 | make | `section_blend` with Hermite ends G0-G3 | step 5 surface construction | — | Sonnet |
| MK12b | make | Sheets between sheets (`blend_edges`, `fillet_sheets`, `fillet_crossing`) | MK12, BO14, SM9 for NURBS sheets | — | Opus |
| MK13 | make | `radial`: the hollow cone built directly (outer, inner, annular caps) with no stitch or subtract | — | — | Sonnet |
| MK14 | make | `prism` and `sheet` over several planar regions; port `O/prism.rs` and `prism/tests.rs` truths | — | — | Sonnet |
| MK15 | make | `face` on an analytic surface by a uv box or a planar region in uv; iso-line edges exact (lines and circles) | — | — | Sonnet |
| MK15b | make, nurbs-surface | `face` on a NURBS surface: iso-curve extraction in `scx-nurbs-surface` (exact, by knot insertion to full multiplicity; not built yet); bounds other than iso lines wait on step 5's curve-on-surface kind | MK15; knot insertion | — | Sonnet |
| MK16 | make (tests) | The closed-form make table: the ops 26-case table, Pappus, cornered and stations truths, against T9 | MK4, MK5 | — | Sonnet |
| MK17 | make | `ruled` between two curves (lines and coplanar arcs exact by kind; others a degree-1 NURBS after compatibility) and `reverse` of a sheet as a Draft method (`O/surface.rs:340, :366`) | knot insertion and degree elevation (for NURBS curves) | — | Sonnet |
| DF1 | deform | Crate; `deform` with the exact image rules of §5.5 on analytic faces; `Refused(MeetsAxis)` | step 5 volume maps (bend, twist and unbend only) | deform → surface, brep | Opus |
| DF2 | deform | Faces with no exact image onto the in-volume procedural kind | DF1, step 5 procedural kinds | — | Sonnet |
| DF3 | deform (tests) | Jacobian volume closed forms, bend ∘ unbend = identity, derivatives against finite differences in exact arithmetic, the pin's map tests | DF1 | — | Sonnet |
| RP0 | difftest | Count, over the corpus's untrusted records, what the SMLib healer (`heal_made`, `K/topology/brep/p10.rs:546`, from `solidcore-convert/src/brep.rs:317`) changes: records it alters, by defect class, and how many then pass the check that failed before. Its result decides Q7 finally | — | — | Sonnet |
| RP1 | repair | Crate; `heal` with items per defect; short-edge clusters and squeezes on lines and planes exactly | SU6 (built) | repair → surface, brep, check (findings) | Opus |
| RP2 | repair | B-spline ends: interval enclosures (`scx-nurbs`'s `eval_over`, built), control-point moves, GAP_SHARE certified on SU6; zero-length piece cutting | RP1 | — | Opus |
| RP3 | repair | Sliver collapse by certified coincidence | RP1 | — | Opus |
| RP4 | repair (tests) | Port the 18 ops heal tests as truths; one-defect recipes at δ = 10·acc, acc, acc/10 | RP1-RP3 | — | Sonnet |
| RP5 | repair | `sew` within a gap over `scx-stitch`: the kept edge's accuracy is the certified gap, recorded; `TooFar` past it | RP1 | repair → stitch | Opus |
| RP6 | repair | `patch_holes`: plane faces exactly; otherwise MK11's patch meeting the neighbours G1, `Unproven` where uncertified; sewn in by RP5 | RP5, MK11 | repair → make | Opus |
| RP7 | repair | `rebuild_analytic`: a NURBS face proved to be a cylinder, cone, sphere or torus goes as that surface; refuted or undecided faces listed and kept | RP1, MK10's certificate module (portable before the loft) | — | Opus |
| RP8 | repair | `repair(&[Step])`: the steps in order, `Changes` composed, `Skip` mode across steps; the app's Repair Part as a step list | RP1, RP5 (RP6, RP7 as they land) | — | Sonnet |

### 7.1 Graph and critical path

```
LY2 (first: before DF1 and RP1 create crates)
BO10 ──> BO10b ──┬─> BO11 ──> DT4 ──> BB1
                 ├─> BO20
                 └─> BO12 (with BO25) ──┬─> BO15 ──> BO15b
                                        ├─> BO16 ──┐
                                        └─> MK8 (HL5, MK7b)
BO13 ──> BO13b ──────────────────────────────────┴─> BO16b <── SF1
BO25 ──> BO14 ──> MK12b
BO17, BO18, BO21, BR11 alone;  BO21 ──> BO22 (tracing: SM6b);  BW3 ──> BO19
BO23 (SM9, fitter, NURBS ray);  BO24 (fitter)
MK4 ──┬─> MK5 ──> MK16
      └─> MK4b (procedural kinds)
MK7a ──> MK7b (procedural kinds) ──> MK8;  MK6 (procedural kinds)
MK9 ──> MK10 (surface construction) ──> MK10b;  MK11, MK12 (surface construction) ──> MK12b
MK13, MK14, MK15 alone;  MK15 ──> MK15b;  MK17 (knot insertion)
DF1 (volume maps) ──┬─> DF2 (procedural kinds)
                    └─> DF3
RP0 alone (decides Q7)
RP1 ──┬─> RP2 ──┐
      ├─> RP3 ──┴─> RP4
      ├─> RP5 ──> RP6 (MK11)
      ├─> RP7 (MK10's certificate module)
      └─> RP8 (with RP5)
```

- **Can start now (17 rows, plus a module):** LY2, BO10, BO13, BO17, BO18, BO21, BO25, BR11, SF1 (closed forms), MK4,
  MK7a, MK9, MK13, MK14, MK15, RP0, RP1, and MK10's certificate module on its own (for RP7).
  Checked against main: BO18's wire body exists (`scx-brep` shells list wire edges); MK15 is
  narrowed to analytic surfaces, because iso extraction on NURBS surfaces is not in
  `scx-nurbs-surface` yet (MK15b); RP2's B-spline enclosures exist (`scx-nurbs/src/eval_over.rs`)
  and SU6's gaps exist (`scx-surface/src/gap.rs`), so RP2 is ready once RP1 lands.
- **Startable once a step-7 or step-6 row lands:** BO10b, BO11, BO12, BO13b, BO14 to BO16b, BO19
  (BW3), BO20, BO22's stated and closed-form halves, DT4, BB1, MK5, MK16, RP2 to RP5, RP8.
- **Waits on step 5:**
  - BO23 on SM9, the pcurve fitter and step 5's ray against NURBS;
  - BO24, SF1's general curves and BO13b's general curves on the fitter; BO22's tracing on SM6b;
  - MK4b, MK6, MK7b and DF2 on the procedural kinds;
  - MK10, MK10b, MK12 on step 5's surface construction; MK11, MK15b and MK17 (NURBS curves) on
    knot insertion and degree elevation; RP6 on MK11;
  - DF1 on the volume maps.
- **Waits on step 6:** BO19 on BW3; MK8 on HL5.
- **The critical path to "done when"** is BO10 → BO10b → BO12 (with BO25) → BO15 and BO16, with
  DT4 growing beside it. BO10 can fail on main today; BO10b fixes what it finds before BO12 builds
  on the pipeline. The NURBS Boolean (BO23) is outside the analytic gate, as in step 4.
- **Step-8 families that can start early:**
  - `scx-mesh` (mesh to body; make, tess, mass): now, on MK3 and MK14.
  - `scx-sheetmetal` (develop, fold): once MK4 lands. The pin's develop uses its own maps
    (`K/develop/fold.rs:60-90, :412`), so it waits on no deform row; bends are cylinders.
  - `scx-taper` (draft walls, the pin's `taper.rs` and `taper_walls.rs` for line and arc profiles,
    which are exact): once MK5 and BO12 land; offsets of non-planar walls wait on step 5's offset
    kind.
  - `scx-local` (move, replace and delete faces with heal; the defeature removals `remove_faces`,
    `remove_feature_with_crossing_patches`, `remove_round_junction`, `remove_ring_groove`;
    `A/delete_face.rs`): once BO12, BO25 and BR11 land.

### 7.2 Amendments to 01 in this branch

LY2's design changes four rows of 01's crate table (`docs/01-architecture.md`), amended in this
branch:

- `scx-boolean` (row at :100): depends on bodyq, ssi, mass; the stitch edge is dropped (Q2).
- `scx-deform` (:102): the bend, twist and unbend maps (no B-spline volume, Q6); depends on brep and
  surface; no stitch (topology is unchanged) and no check (the result contract runs outside).
- `scx-make` (:103): adds ssi (MK5's mitre conics).
- `scx-repair` (:104): the certified heal, sew within a gap, hole patches and analytic rebuild, none
  needing Booleans; depends on stitch, check, make and surface (n-sided patches); the boolean edge
  is dropped. 01 placed it at layer 8 for Booleans it does not need; it stays at layer 8 because
  RP6 depends on make.

## 8. Truth tests per task

### 8.1 The Boolean algebra (BO10)

On every pair (A, B) and triple (A, B, C) of the testkit's Boolean recipes (T12) and convex recipes,
with `≡` meaning: equal volume and area to 1e-9 relative (by `scx_mass`, within the Estimate's
bound), the same T10 classification at 1,000 seeded points outside the band, the contract passing,
and the same count of shells and regions.

| Property | Statement | Notes |
|---|---|---|
| Commutativity | A ∪ B ≡ B ∪ A, A ∩ B ≡ B ∩ A | ids differ (`a`'s are kept); not compared |
| Shared-edge collars | for coaxial collars that meet along a shared circular edge or overlap over a length ℓ: A ∪ B ≡ B ∪ A, and vol(A ∪ B) = vol(A) + vol(B) − πr²ℓ, in both operand orders | the radial order at the shared edge, which the pin retries for (R2); the #20408 test (`tests/boolean.rs:50-75`) is mandatory: as a testkit recipe of its geometry now, and its own two records through DT4 once they can be read |
| Idempotence | A ∪ A ≡ A, A ∩ A ≡ A, for A and a translated copy A′ = A (two bodies) | the pipeline path, not BO20's shortcut |
| Self difference | A − A′ = ∅ | the pin's `boolean_api.rs:483`; also the empty-result refutation |
| Absorption | A ∪ (A ∩ B) ≡ A, A ∩ (A ∪ B) ≡ A | |
| Partition | vol(A − B) + vol(A ∩ B) = vol(A); (A − B) ∪ (A ∩ B) ≡ A | |
| Inclusion-exclusion | vol(A ∪ B) = vol(A) + vol(B) − vol(A ∩ B) | the pin's `tests/boolean.rs:3585` |
| De Morgan within a box X ⊃ A ∪ B | X − (A ∪ B) ≡ (X − A) ∩ (X − B) | |
| Associativity | (A ∪ B) ∪ C ≡ A ∪ (B ∪ C), same for ∩ | |
| Distributivity | A ∩ (B ∪ C) ≡ (A ∩ B) ∪ (A ∩ C) | |
| N-ary order | `boolean_all` over every permutation of 4 operands ≡ one another | BO11, BO12 |
| Rigid motion | T(A ∘ B) ≡ T(A) ∘ T(B) for a rotation and a far translation (1e6) | replaces the pin's origin shift |
| Determinism | every result encoded by `scx-record` is bit-identical over two runs | |

A property is asserted only where every Boolean it uses is built. A pair where one side is built and
another is `Unsupported` is counted and printed, so the suite measures coverage as well. The
collars and #20408 are the exception: they must pass before BO10b closes.

### 8.2 Per task

| Task | Truths | Pin tests to port |
|---|---|---|
| BO10b | every BO10 failure fixed or filed with a typed cause; an empty result with a witness vertex is `Defect(EmptyRefuted)` on a recipe built to provoke it | `O/boolean.rs` refutation tests; `boolean_api.rs:483, :509` |
| BO11, BO12 | §8.1 n-ary order; closed-form volumes of unions of k boxes and k cylinders; cells counted against the arrangement of the operands (a closed form for boxes); the collars in every operand order | `O/boolean.rs:477, :489, :503`; `tests/boolean.rs:2948, :2990, :500, :50` |
| BO13, BO13b | the target's volume unchanged; section edges lie on both operands (certified gap); faces split into the closed-form count for a box imprinted by a cylinder; regions bounded by curves have their closed-form areas | `wr_619_imprint_skips_collapsed_definition` (outcome only); `split_face_by_curves` tests asserting areas |
| BO14 | section areas in closed form: plane through a sphere, cylinder, torus, box; the section's boundary on both surfaces | `planar_section.rs` tests that assert curves; `face_section` tests |
| BO15, BO15b | vol(front) + vol(back) = vol(body); each side's classification by the half-space oracle; a short tool refused | `tests/boolean.rs:3528`; split tests asserting volumes; `ToolStopsShort` tests |
| BO16, BO16b | the kept regions' areas in closed form; a pick on the cut refused | `tests/trim.rs:371`, `:627`; `trim_by_dropped_curves` tests asserting areas |
| BO17 | 2-D areas by the shoelace and arc segment formula, in exact rationals where the inputs are | `boolean2d` tests asserting areas |
| BO18 | intersection curves of box and cylinder, sphere and sphere, plane and torus in closed form | `intersect_faces_read_only` (9) |
| BO19 | gap and depth brackets contain the closed-form distance and penetration of spheres, boxes | — |
| BO21 | a true fact gives the same result as no fact, in less work; a false fact is `Refused(FalseFact)` | — |
| BO22 | tangent sphere-in-cylinder, cylinder-on-plane: closed-form volumes; the contact curve on both | `tests/boolean.rs` tangent cases, `curved_sliver_coincidence` |
| BO23 | the 73 §9 NURBS truths (SM0) as Boolean volumes | BO5's tests turn into results (SM9) |
| BO25 | a section sheet's area; laminas counted against the closed form | — |
| BR11 | merged faces' area equals the sum; protected edges survive; the contract passes | `A/heal.rs` tests asserting outcome |
| SF1 | a circle projected onto a plane along a direction is the closed-form ellipse; a line dropped onto a cylinder along its axis is a line | `project_curves`, `drop_curves` tests asserting geometry |
| MK4 | Pappus: volume 2π·x̄·A and area 2π·x̄·L per profile; a full sphere, torus, horn torus and frustum; poles valid by the contract; a true crossing refused | ops table :298, :302; `wr_590` (a build test: three faces and the Pappus volume); `api_brep.rs:1735`; `issue_2905:158` |
| MK5 | area × length on straight runs, Pappus on arcs, the mitre's exact conic | `sweep_cornered.rs:112, :178, :363`; ops table :307, :320, :343 |
| MK6 | Pappus for a square thread, the wall exactly on the screw surface; a law's offsets at its turns | `screw_sweep.rs:271`; ops table :474, :492 |
| MK7a | torsion of a helix frame, identity on lines, rotation on arcs, to 1e-12 | `sweep_stations.rs:74, :505` |
| MK7b | a twisted square prism's volume (twist preserves area); an end scale s gives the frustum volume; rails as MK10b | the app's `sweep_path.rs` tests asserting volumes |
| MK9 | the optimum against a brute-force search on small sections; pairing invariant under rotation of the start vertex; a banded answer that avoids its band edge reported as proven | `fork_ea59e9c_section_orders` (23) as outcomes |
| MK10 | the cylinder through stacked circles; the frustum on a straight rail; the certificate's bound above an exact sample on every span | ops `sweep.rs:1359, :1334, :1398`; `loft_rails.rs:147`; `certificate.rs:1658, :1678` |
| MK11 | the bilinear patch of four lines; a sphere octant from three arcs; a flat pentagon | `gordon.rs:866`; `O/surface.rs:543, :668`; `n_sided.rs:276` |
| MK12 | two G0 sections give the ruled surface; quadratic sections give the quadratic | `section_blend.rs:47, :132` |
| MK13 | hollow cone volume in closed form | `local_operation.rs` radial tests asserting volume |
| MK14, MK15, MK15b | area and volume per region; iso edges on the surface exactly | `prism/tests.rs` (73) where they assert quantities; `O/../tests/prism.rs` (35) likewise |
| MK17 | the ruled surface between two parallel lines is the plane strip of closed-form area; reversing twice is the identity | `ruled`, `reverse_sheet` tests asserting outcome |
| DF1, DF3 | the bent block's volume ∫ (u/d) dV in closed form; bend ∘ unbend = identity on points to a stated bound; plane cases exact by construction | `bend:1567, :1580`; `twist:1078, :1089`; `unbend:1488, :1500`; `p1:2660, :2918, :3053`; `bend:1775` |
| RP0 | none: a count, printed and recorded | — |
| RP1-RP4 | the check passes; no accuracy grew; `|ΔV| ≤ area × largest bound`; counts in closed form | all 18 of `O/../tests/heal.rs` except `:1402` (Q7) |
| RP5 | a box with one face moved off by δ < gap sews to a solid whose volume is within area × δ; each sewn edge's recorded accuracy contains the closed-form gap; δ > gap is `TooFar` | the app's `repair.rs` tests asserting outcome |
| RP6 | a box less one face patched back: the exact volume; a cylinder's missing cap patched planar; a curved hole's patch with its G1 bound recorded | `A/fill.rs` and `fill_general_tests.rs` tests asserting areas or continuity |
| RP7 | a NURBS cylinder, cone, sphere and torus rebuilt as the analytic surface; a perturbed one refuted and kept | the app's repair rebuild tests |
| RP8 | the step list in order equals the entries composed by hand, bit for bit | — |

### 8.3 Work counts for Q5 and R2 on large and hard inputs

The perf rule asks for before and after counts of added work. Two rulings add work in some cases;
BB1 measures them, with side-channel instrumentation in local runs (no committed counters), and
reports each as a table against the pin.

- **R2 (no union retry).** What is measured: per union, the work units charged (`Report.work`) and
  wall time in scx, against the pin's total for the same union including every attempt
  `union_copies` makes, the operand copies, the witness copy, and `matched_tolerances`' trial copy
  and Level2 check (counted by wrapping those calls in the bench harness, not in the pin's code).
  Inputs: DT4's Boolean recipes; the #20408 collars and BO10's collar recipes in both orders (the
  cases that take the pin's retry); and the largest corpus unions. The expected result is less
  work in scx on every case the pin retries, because scx runs once. The risk measured is the cost
  of BO12's geometry-only radial ordering at shared edges (sorting faces about an edge by exact
  tangent comparison, then curvature where tangents agree): counted as comparisons per shared
  edge, with the worst case on the collars. A rise above 10% against the slice's baseline on
  recipes with no shared edges fails, as the t_slice work rule does.
- **Q5 (exact pairing within a fixed limit, then banded).** What is measured: cells visited per
  section pair, by the exact search and by the band, against the pin's count on the same sections;
  and how many corpus lofts reach the band at all. Inputs: every loft in the corpus, plus generated
  section pairs of 10, 100, 1,000 and 2,000 vertices. The exact search costs O(mn log n) cells; the
  pin's MAX_CELLS of 4e6 (`loft_sections.rs:80`) is kept as the fixed limit so the branch taken is
  the pin's, and scx's added work is the band's proof check (whether the walk touches the band's
  edge), linear in the walk. BB1 records the cell counts before MK9 (the pin) and after.

## 9. Open questions

**Q1. Step 5 has no rows for the fitter, the procedural kinds, surface construction, the volume
maps or the ray against NURBS. Who writes them?**

- Recommend: the coordinator asks for one step-5 widening note (like 73 for the SSI) that gives
  these five their rows before step 7's waiting tasks are briefed. Step 7's tasks cite them by
  name until then. Seventeen of this note's 56 rows (and MK10's certificate module) can start now,
  and about 20 more wait only on other step-6 or step-7 rows.
- Against: a planning note for five large pieces delays them by a review cycle. Mitigation: the
  analytic step-7 rows (§7.1, "can start now") keep builders busy for that cycle.
- Answered by 77 (11-28): `research/77-step5-widening.md` gives the five pieces their rows (§7)
  and the step-7 rows each one unblocks.

**Q2. Does `scx-boolean` depend on `scx-stitch`?**

- Recommend: no. layers.txt already orders boolean below stitch, and the slice builds without it
  (BO3: known topology, no stitch). Every step-7 entry builds its topology from the imprint, and the
  pin's uses of stitch inside the Boolean (`boolean_2d`'s `StitchFaces`, the planar path) are
  replaced by known topology (BO17, MK5). LY2 removes the edge from 01's table.
- Against: some later entry (trimming sheets that only nearly meet) may want to sew. Mitigation:
  such a case is two operations at the caller's level (stitch, then Boolean), and the Boolean
  stays lower. `scx-repair` does keep its edge to `scx-stitch` (RP5's sew), and adds one to
  `scx-make` for RP6's patches (§7.2).

**Q3. A revolve profile that touches or crosses the axis.**

- What the pin does (READ): WR-590 is an interior touch: the profile meets the axis at one point
  inside it and leaves on the same side, so the revolve is a horn torus pinched at a pole. The pin
  splits the profile at the touch (`PC/p3/sweeps.rs:181-194`) and builds both halves, giving three
  faces and the Pappus volume. A true crossing, where the profile passes to the other side, is
  refused (`K/sweep/rotational_sweep_geometry.rs:364`).
- Recommend: the same split by meaning. MK4 makes an interior touch a pole vertex shared by the
  faces on either side of it (no split into two bodies: one solid with a pole, which `scx-brep`
  holds), and refuses only a true crossing with `Refused(CrossesAxis { at })`. WR-590 is ported as a
  build test: three faces, the Pappus volume, the contract passing.
- Against: a touch the input states only to within coincidence may be a crossing by a hair.
  Mitigation: the decision is the sign of the profile's distance from the axis on each side of the
  touch, decided by certified bounds; a touch that cannot be decided is `Unproven`, never guessed.

**Q4. N-ary Booleans: a fold now and the cellular merge later, or only the cellular merge?**

- Recommend: BO11's balanced fold now (small, starts at once, gives DT4 rows), replaced by
  selection over BO12's merge when it lands; the fold's tests stay as the oracle for the merge.
- Against: two implementations of one entry is waste, and a fold's intermediate bodies cost
  memory and work. Mitigation: BO11 is a few dozen lines over `boolean`; BB1 measures fold against
  merge before the fold is removed.

**Q5. Loft pairing: the pin falls back to a banded walk past 4e6 cells, which may miss the optimum.
Keep it?**

- Recommend: keep both, with the answer's status stated. The exact search runs up to a fixed
  per-operation cell limit (the pin's 4e6, a constant of the operation, not the remaining budget,
  so the branch taken is a function of the input alone, R6). Past it, the banded answer
  (`loft_sections.rs:1161-1336`) is returned with its bound: `Proven` when its walk does not run
  along the band's edge (the pin's own test that the band held the optimum), otherwise `Unproven`,
  with its cost as an upper bound on the optimum. Not `Exhausted`: an answer exists and is good.
  It is deterministic because the band is computed from the input.
- Against: an `Unproven` pairing may be worse than the optimum, and callers must handle the status.
  Mitigation: 4e6 cells is 2,000 by 2,000 vertices, far past CAD profiles; §8.3 counts how many
  corpus lofts reach the band at all.

**Q6. Is `scx-deform` in step 7 at all?**

- Recommend: yes, but narrow: DF1 to DF3 on the bend, twist and unbend maps only, and no B-spline
  volume (a stub in the pin, with no caller). The body operation is new capability, the pin's app
  has none, so it is last in priority among step-7 rows.
- Against: a crate with no caller is speculative work, and 01's "done when" does not name deform.
  Mitigation: DF1 is small once the maps exist, and sheet metal may want bend-of-body later; if
  step 5's volume maps slip, DF moves to step 8 without blocking anything.

**Q7. Port the SMLib healer?**

- Recommend: no. It widens tolerances by model-size buckets and gap gains (R3), samples gaps
  (R10), and is reached only from untrusted record conversion. Its useful parts are queries that
  SU6 and `scx-check` already do, and its seam fixes are `finish`'s and stitch's jobs.
- Against: some imported records may need its fixes to become valid, and the converter
  (`scx-import-sc`) may then refuse designs the pin opens. Mitigation: RP0 counts, before this is
  final, how many corpus records `heal_made` changes and how many of those then pass the check;
  each class it really fixes becomes a typed repair item (RP rows), certified.

**Q8. Repair keeps the pin's run-time "check before and after, keep if better" gate?**

- Recommend: no. Every edit is certified, so validity follows by construction; the tests assert
  the full check on every result. A run-time check costs two full checks per repair and hides bugs
  by rolling back.
- Against: a certified local edit can still break a global property (two squeezed clusters making
  faces cross). Mitigation: RP4 includes the adjacent-cluster recipes, and an interaction found
  there becomes a typed cause checked before the edit, not a rollback.

**Q9. A Coons or n-sided patch that may fold: the pin samples a 5×5 grid. What does scx do?**

- Recommend: certify the normal's sign over the patch by the Bernstein form of the cross product's
  component along a reference direction (exact on polynomial patches). A patch that fails is
  `Refused(Folds)` with the uv where it fails; one the budget cannot decide is `Unproven`.
- Against: the certificate costs a degree-(2p−1)·(2q−1) Bernstein form per patch. Mitigation: it is
  computed once per construction, and it replaces a grid that can miss folds.

**Q10. The revolve's analytic candidates: what of a cone whose height is below the pin's
MIN_HEIGHT (#17085)?**

- What the pin does (READ): it does not refuse. Below the floor, or when any analytic construction
  fails, the candidate is dropped and the face keeps its revolution surface or B-spline
  (`K/sweep/analytic_revolution.rs:26-30, :333`).
- Recommend: no fixed floor, and every line or arc profile gets its analytic surface. The plane snap
  rule (§3.6): a profile line whose two ends lie at axial heights within the coincidence
  (max(res, acc₁, acc₂)) of each other revolves to a plane normal to the axis at the start
  vertex's height; the end vertex's measured distance from that plane is recorded as its accuracy.
  Any other line is a cylinder (both ends at one radius within coincidence) or a cone, however
  low.
- Against: a near-flat cone is ill-conditioned for later SSI. Mitigation: SM2 and SM4 handle it by
  certified bounds; a conditioning warning is recorded, never a refusal or a fallback surface.

## 10. Against this plan

- **The plan's analytic gate is narrow.** Most SolidCraft designs that need step 7 have NURBS faces
  (lofts, fillets from step 8's blends, imported splines), and BO23, MK10 and MK11 wait on step 5.
  Against that: the analytic rows build the pipeline that BO23 widens, and DT4 rows measure each
  widening. The step's "done when" is met on analytic recipes first, as step 4's was.
- **BO10 may find a long tail.** The algebra suite will pass on few pairs at first (coincident faces
  everywhere in A ∪ A′). That is its purpose. If it delays BO12 badly, BO12 starts on the subset
  that passes and BO10 failures become issues.
- **The cellular merge is a big rewrite of a working Boolean.** BO12 changes the core of the slice.
  Against that: the slice already classifies by connected piece sets with membership, so BO12 is a
  generalisation of stage 6's labels from two operands to n, and the slice's tests are its first
  oracle.
- **Differing from the pin where it is not truth** (Q3's single solid with a pole, Q5's status on
  the banded answer, Q10's cone where the pin keeps a B-spline) moves difftest outcomes. Each is
  recorded in DT4 as a by-design difference with its reason, never as a pin defect.
- **No sampling anywhere is ambitious.** R10 replaces every sampled acceptance with a proof. Where a
  proof is not yet available (G1 on n-sided patches, which the pin also does not certify), the
  answer is `Unproven` with its bound, which callers must handle.
- **Make in one crate is large.** Revolve, sweeps, lofts and patches together are about 60,000 pin
  lines. Mitigation: modules per construction, and the geometry (Coons, Gordon, n-sided, the volume
  maps) lives in `scx-surface`, so `scx-make` holds topology only.

## 11. Proposed 04 rows (ruled by the coordinator, for Brett's review)

| date | status | decision | why/against | ref |
|---|---|---|---|---|
| 11-28 | Ruled 11-28 (coordinator), for review | **Step 7 plan.** `scx-boolean`'s entries (`boolean`, `boolean_all`, `merge`/`select`, `imprint`, `imprint_curves`, `split`, `split_faces`, `section`, `trim`, `planar`, `intersect`, `interference`, `boolean_given`) are one pipeline (R1), take `&Brep` and return `Done<C>` (`interference` returns a decided `Interference`); proof is the default of `boolean`, not a mode, and an empty result with a witness inside is `Defect(EmptyRefuted)` (a ported postcondition); no retry, operand swap or rebuild (R2: the pin's one retry is `union_copies`, which every app union goes through; BO12's geometry-only radial order at shared edges replaces it, and the #20408 collars are a mandatory truth); no tolerance ladder (R3). BO10b fixes what BO10 finds before BO12; BO13 runs before BO12, rework accepted. Face and edge merge are `scx-brep` Draft methods (BR11). Tasks: 75 §7 | The pin's modes and retries are the hazards; one pipeline means one set of proofs. Against: entries with different needs (2-D Boolean) may be faster alone; and the retry hides an order dependence BO12 must remove; BB1 measures both (75 §8.3) | 75 §1.2, §2.6, §8.3 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Make builds exactly.** Constructors build the analytic kind directly where one exists (R4); otherwise the step-5 procedural kind; NURBS only where the operation is a fit, with a certificate. Choice by a predicate on the input before building (R5). `Made<R>` with typed face records replaces out-params (R7); no `cap: bool` or integer cap modes; no stitch in constructors, including n-sided patches (shared sides known). Added: `ruled` and `reverse` (MK17), the screw's radial law and joined walls (MK6), the sweep's twist, end scale and rails (MK7b), `face` on analytic surfaces now and NURBS after iso extraction (MK15, MK15b) | Removes build-then-recognise sampling and its fallbacks. Against: more code paths than one general sweep; each is small and closed-form tested | 75 §3.6, §4.6 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Revolve splits at an interior axis touch and refuses only a true crossing.** A touch (WR-590, a horn torus) becomes a pole vertex in one solid; a crossing is `Refused(CrossesAxis)`, remedy `ChangeInput`; segments on the axis become pole vertices; WR-590 is a build test. No MIN_HEIGHT floor: the plane snap rule makes a segment perpendicular to the axis within coincidence a plane, every other segment a cone; the pin's kept revolution surface or B-spline is not ported | Matches the pin's meaning (it splits a touch and refuses a crossing) with exact surfaces. Against: a touch within coincidence of a crossing; decided by certified bounds or `Unproven` | 75 §3.6, Q3, Q10 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Loft pairing is exact within a fixed per-operation cell limit, then banded with its status**: past the limit the banded answer is returned, `Proven` if its walk avoids the band's edge, else `Unproven` with its cost as the bound (never `Exhausted`); deterministic since the band is a function of the input. Certificates have a fixed per-operation budget and record `Raise` on an unproved span, never `RailPastStationCap` | Keeps the pin's good answer on large sections and states what is proved. Against: callers must handle `Unproven`; 75 §8.3 counts how many corpus lofts reach the band | 75 §4.6, Q5, §8.3 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Repair is a list of certified steps.** Separate entries `heal` (the certified healer, items per defect, `Skip` mode, no accuracy grows), `rebuild_analytic` (only where a certificate proves the analytic surface), `sew` within a gap over `scx-stitch` (each sewn edge's accuracy is its certified gap, recorded; the pin's 2× factor not ported) and `patch_holes` (plane exact; else MK11's patch G1, `Unproven` where uncertified), and `repair(&[Step])` for the app's Repair Part, replacing `RepairOptions`' bools. No run-time check-compare gate; no global `set_heals_tolerance_defects`. The SMLib healer is not ported unless RP0's count of what `heal_made` fixes on the corpus shows a class worth a certified item | Certified edits make the gate redundant; steps make the options data. Against: imported records may need SMLib fixes; RP0 counts them first | 75 §6.5, Q7, Q8 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Deform is narrow in step 7**: bend, twist and unbend maps (geometry in `scx-surface`), `scx_deform::deform` with exact images decided by kind and frame and the procedural in-volume kind otherwise; no B-spline volume; depends on brep and surface only. Supersedes 01's `scx-deform` row (01:102), amended in this branch | The pin has no body deform and a stub B-spline volume. Against: no caller; it is last in priority and may move to step 8 | 75 §5.5, Q6, §7.2 |
| 11-28 | Ruled 11-28 (coordinator), for review | **Layers (LY2).** `scx-deform` after `scx-hlr`; `scx-repair` after `scx-make`; no `scx-boolean` → `scx-stitch` edge; new edges: `scx-make` → `scx-ssi`, `scx-hlr`, `scx-boolean`; `scx-repair` → `scx-stitch`, `scx-check`, `scx-make`, `scx-surface` (no `scx-boolean`); `scx-deform` → `scx-brep`, `scx-surface`. 01's rows for boolean, deform, make and repair (01:100-104) are amended in the same branch | Matches layers.txt as it stands and the built slice. Against: a later Boolean entry may want to sew; it composes at the caller | 75 §7, §7.2, Q2 |
| 11-28 | Ruled 11-28 (coordinator), for review | **The Boolean algebra suite (BO10) is the step-7 gate's property half**: the §8.1 properties on every built pair and triple, including coaxial shared-edge collars in both operand orders and the #20408 collars (mandatory), judged by volume, area, the T10 oracle, the contract and run-twice bits | The pin has one commutativity check; properties catch what recipes miss. Against: it will fail widely at first; BO10b fixes the slice's failures before BO12, the rest become issues | 75 §8.1 |

## 12. What this note did not do

- It did not build or run the pin, measure any work count, or run any scx code.
- It did not read copyleft kernels; every algorithm named is from the pin (Apache-2.0 here) or a
  published source (The NURBS Book, Farouki-Rajan, Higham).
- It did not plan step 5's fitter, procedural kinds, surface construction, volume maps or ray
  against NURBS (Q1), nor step 8's families beyond naming their early starts.
- It did not check the testkit recipes' coverage of step-7 cases; DT4 and BO10 will.

## 13. As built

### 13.1 RP0 as built (11-28)

**What was run.** `crates/scx-difftest/examples/heal_count.rs` (`cargo run --release -p
scx-difftest --example heal_count`, deterministic, no counters in any library). Each record is
built twice through the pin's `solidcore-convert` (`to_kernel_body`): trusted (no healer) and
untrusted (`heal_made` runs). Both bodies go through the pin's `check_brep` at level 2
(pointers, then `AssertValid`), failures classed by the failing report's name. A record is
*altered* when the healed body differs in entity counts, vertex or edge tolerances, or the
record it converts back to. Records: every distinct body of the rig corpus
(`kernel_cases`, 1,484 bodies, 1,248 distinct; all of them are marked trusted there, so
none reaches the healer in the pin: each is healed here as if it were untrusted) and the pin's
STEP fixtures read by `solidcore-step` (4 solids). The NIST STEP files are not on the build
machines (the fetch script needs `unzip`); `SCX_HEAL_STEP_DIR` runs any directory of them.

| | rig corpus | STEP fixtures |
|---|---|---|
| distinct records | 1,248 | 4 |
| refused to build, with or without the healer | 0 | 0 |
| failing the check before the healer | 1 | 0 |
| of those, passing after the healer | **0** | 0 |
| altered by the healer | 177 | 3 |
| of which valid before | 176 | 3 |
| altered, tolerances only | 131 | 3 |
| altered, entity counts (topology) | 31 | 0 |
| altered, geometry or trims | 15 | 0 |
| passing before, failing after | **15** | 0 |

The one record failing before (three reports: Face has Coincident Edges, Coincident
Vertices, partially Coincident Edges) is not fixed: two of the three reports remain. The 15
that go from passing to failing all fail `Tolerance` after healing (the healed
bodies carry a tolerance the check refuses; e.g. `booleans/5322-cut-with-a-5000-mm-tool.json:b3`,
`fillets/16019-chamfer-1-on-the-top-rim-of-a-cylinder-r125.json:b1`).

**Recommendation for Q7: do not port the healer.** On this corpus it repairs no failing
record (0 of 1) and makes 15 valid records fail the check, while widening tolerances on 131
and changing the topology of 31 valid ones. The class worth a certified item: none shown.
For (keeping the question open): the corpus holds no record that reaches the healer in the pin
(all trusted) and only 4 STEP solids, so real untrusted input is not sampled; the NIST files
should be run once through `SCX_HEAL_STEP_DIR` before the row is closed, and a Q7 reversal
needs a class of failing records with a count. Against porting, in numbers: 15 regressions,
0 fixes.

NIST run (coordinator, 11-28): the 33 NIST MBE PMI STEP files (public domain; fetched by the pin's
pinned-hash script) through the same harness (`SCX_HEAL_STEP_DIR`). 33 records: none refused; 2 fail
the check before the healer (Bad Knot Count, Bad Repeated CPts) and both still fail after it; the
healer alters 32 (31 tolerances only, 1 geometry or trims) and turns no valid record invalid. Over
all three sets the healer fixes 0 of 3 failing records, so Q7 is ruled: not ported.

### 13.2 BO18 as built (11-28)

`scx_boolean::intersect(a, b, cx) -> Result<Done<NoItems>, BooleanError>` runs stages 1-4 of `boolean` (`interfere`, `face_face`) and stops; `crates/scx-boolean/src/intersect.rs`. The result is a `Draft::new()` body of no faces:

- each stage-4 `Section` is a wire edge on its curve over its span, between the vertices of its merged points; a closed section no merged point lies on gets one vertex at its start;
- each `Lying` piece (an operand edge on the other body) is a wire edge on the operand edge's curve over the piece, written once when the other operand's edge also runs along it (same end vertices and a midpoint within the zone);
- each `Touch`, and each merged point no edge ends at, is a lone vertex (touches deduped against written vertices within the zone);
- a coincident pair is a region, so only its boundary (the lying pieces) appears;
- one shell of the outside region per connected piece (union-find over vertices, shells in order of lowest vertex), since a shell must be connected: two circles apart are two shells; a lone vertex is a shell of its own; no contact is a body with no shells.
- lineage is `Origin::Operand(op, entity)` for both operands (the draft is not `a.edit()`, so `Origin::Face` of `a` has no meaning).

Decisions. Fresh draft against `a.edit()` with everything removed: for, the result is a new body and carries no `a` ids; against, ids of `a` are not preserved for callers that want to map back (they use the lineage). Dedupe by geometry against by `along`: `along` is only set for edges lying along an edge, but the same piece can also come from two faces of one operand, so geometry (ends plus midpoint) covers both; against, a quadratic scan over written edges (fine at the sizes here). Lone vertices from unused merged points are included: the other choice (touches only) would lose a point where only edges meet.

Truths (tests/intersect.rs, 12 tests): disjoint cubes, no shells; overlapping cubes, six lines of 0.5 and six vertices; cylinder through a plate, two circles of 2 pi in two shells; cylinder on a block side, arcs pi, pi and rulings 1, 1 in one shell; cylinder with a square plane, a unit circle; a tilted plate, two ellipses of semi-axes 1 and sqrt 2 (perimeter by quadrature); two unit spheres, circle of length sqrt 3 pi (two arcs); plane through a sphere's centre, 2 pi; tangent plane, one lone vertex at the closed-form point; plane through a torus's mid plane, 20 pi (the inner and outer circles of R = 5, r = 1.5); operand swap and repeat agree; shared cube face, four unit edges once each. The solid `contract` does not apply to a wire body; `finish` checks it.

Open: no pin test of `intersect_faces_read_only` (9) was ported (it needs the pin's face-level API); the NURBS and general-SSI pairs stay `Unsupported(SsiPair)` as in `boolean` (BO22/BO23).

### 13.3 BO13 imprint as built (11-28)

`scx_boolean::imprint(target, tool, what, cx)` (`imprint_op.rs`) runs stages 1 to 5 and
finishes the draft with no selection. Choices 75 left open:

- **The result is the target's part of the stage-5 draft.** Stage 5 writes both operands whole;
  the entry removes the tool's faces, then the edges, vertices, shells and regions only they
  held, and finishes. `Done.changes` is then the target's own edit (split faces with their
  pieces, cut edges, new vertices, section edges whose origins include the tool's faces as
  operand 1); the pin's `(Body, bool)` is `!changes.is_empty()`. For: the target keeps its
  material and sense, so volume and area are the target's; nothing needs classifying. Against: a
  caller wanting the tool's faces cut as well must swap the operands and call again.
- **`Imprint::Edges` or `Imprint::EdgesAndFaces`.** Both write every transversal section and cut
  the target's edges at the merged points. `EdgesAndFaces` also writes the tool's edges lying in
  the target's faces (stage 4's `lying` with no `along`), which is what splits a target face a
  tool face rests on into the part under it and the rest; `Edges` drops those. For the split: a
  block standing on a face is then imprinted or not at the caller's choice (tests: 7 faces
  against 6 with no changes). Against: the pin has one Imprint mode; BO12 may find the choice
  unnecessary once cells replace pieces, and the variant can then go.
- **No new counters, no new stage code.** The entry filters stage 4's list and reuses stage 5;
  the cleanup repeats the draft cleanup the stage-5 tests used.

Truth tests (`tests/imprint.rs`, closed forms): offset cubes give 9 faces, 21 edges, 14 vertices,
areas 3 x 1/4 + 3 x 3/4 + 3 x 1; a thin cylinder through a cube gives 8 faces, 14 edges, 10
vertices, two disks of pi/16 and two faces of 1 - pi/16; a block on a face, a block apart,
determinism and operand swap, and a refusal of another document. Volume is 1 throughout.

### 13.4 MK14 as built (11-28)

`scx_make::prism(&[Region], &Frame, Span, &Document) -> Made<PrismFaces>` and
`scx_make::sheet(&[Region], &Frame, &Document) -> Made<SheetFaces>`. `PrismFaces { solids:
Vec<ExtrudeFaces> }` is one record per region, in the order given; `SheetFaces { faces: Vec<FaceId> }`
one face per region. The solids are written by the same code as `extrude` (which now calls
`extrude_into`), so each is exactly what `extrude` makes of its region.

Decisions, with cases for and against:

- **One solid per region, in one body; not a union.** Each region has its own solid region and
  shell, and its own shell in the outside region. For: the pin joins separate extrusions by
  `union_all`, and the regions are apart (below), so the union is the same solids with no
  Boolean. Against: the pin's body is one `union_all` result whose solids could be re-ordered;
  ours are in input order, which is what a caller wants to index by. The outside needs a shell
  per solid because `finish` refuses a disconnected shell (`ShellNotConnected`).
- **Regions that overlap, touch or cross are refused, `Unsupported::RegionsMeet { first, second }`,
  not merged.** The pin first unites the regions in the plane (overlap normalised, volume 300
  for two overlapping 10 x 10 squares) and refuses pieces of the union that touch. No planar
  Boolean exists until BO17, so the merge cannot be done; a refusal is the honest answer and is
  `Unsupported`, not `Refused`, since the input is valid. For: nothing is guessed, and BO17 can
  add the merge as a wider entry later without changing this one. Against: a caller with
  overlapping profiles must unite them first. A region in another's hole, or beside it, is kept
  (the pin's islands).
- **Meeting is decided by `Region::new` on a probe, not by new geometry.** For `a` and `b`, the
  probe is `a`'s outer loop with every other loop of both as holes; its contact check (every
  pair of segments within the resolution, crossing before touching, between loops too) names a
  crossing or touch before nesting, so those two errors mean the boundaries meet. If the
  boundaries are apart, one start point of each region is classified against the other
  (`Region::classify`): inside or on the boundary means one lies in the other's material. For:
  no new predicate in `scx-planar`, exact and certified as `Region::new` is, a hole-in-hole
  island handled by classification. Against: it builds a throwaway region per pair, so the check
  is O(k^2) probes for k regions and each probe is linear in the segments of both.
  Measured (release, throwaway timers, whole circles on a grid, one probe per pair): `prism` of 4, 16, 64
  and 256 regions takes 0.13, 0.44, 3.6 and 28 ms against 0.09, 0.30, 1.2 and 2.6 ms for the same
  regions extruded one by one; the sheet's 0.03, 0.20, 1.9 and 27 ms is nearly all the check. So the check
  added is quadratic, small to about 64 regions and dominant at 256; a bounding-box prune of pairs is the
  fix if a caller passes hundreds of regions (not done: no such caller yet).
  A pair's one start point decides nesting only because the boundaries are apart (a region is
  wholly inside or wholly outside another's material).
- **A sheet's face has both sides in one shell**, a shell per face (the `scx-brep` sheet form);
  its front is the frame's +z side and the outer loop runs counter-clockwise about it, so
  `scx_mass` gives the regions' area and no volume. The sheet is `z = 0` of the frame; there is
  no height argument (a sheet's place is its frame's).
- **`Assembly` gained `sheets`, `begin_solid` and `sheet_face`**, and the outside shell and solid
  shell are `Option`s (a sheet has neither): `face` on a sheet is `Defect::NoSolid`. `error.rs`
  gained `Refusal::NoRegions`, `Unsupported::RegionsMeet` and `Defect::NoSolid`.

Tests (`crates/scx-make/tests/prism.rs`, 11): see the file's header for the pin tests removed on
purpose and why.

### 13.5 MK13 as built (11-28)

`scx_make::radial(&RadialCone, &Document) -> Result<Made<RadialFaces>, MakeError>` builds the hollow cone in one `Draft`: outer wall, inner wall and an annular cap at each end that is not a hinge, with no stitch, no outward check and no subtract. `RadialCone { frame, height, outer: [Length; 2], inner: [Length; 2] }` gives radii at `z = 0` and `z = height`; `RadialFaces { outer, inner, bottom: Option, top: Option }`. Counts: both ends capped 4 faces, 6 edges, 4 vertices; one hinge 3, 5, 3 (the walls share the rim and its vertex). Walls are cylinders when their radii are equal, cones otherwise; the inner wall is the same surface with `Sense::Reversed` and a loop run the other way round, so its front is the hollow.

Decisions.
- Hinge by exact equality of the radii. For: the pin's hinge case is the draft whose neutral plane is the end plane, equal by construction, and exact equality needs no tolerance (R3). Against: a caller whose radii agree to rounding gets a cap of width zero refused; the refusal is `Refusal::Annulus` and the caller can set them equal.
- A wall thinner than the resolution without being a hinge, an inner radius above the outer, and two hinges (no volume) are `Refusal::Annulus { outer, inner }`; inner radii must be above the resolution (an inner apex is not made; the pin refuses it too, `RadialRimsNotAnnulus`).
- A reversed `Cone` runs `v` along `-z`, so the inner wall's half-angle is the negated one; found by the exact-pcurve write refusing the first build.
- New `Dimension::InnerBaseRadius` and `InnerTopRadius`. Edits to `lib.rs` are the module and its re-export; `error.rs` gains the two dimensions and the `Annulus` refusal.

Truths (`tests/radial.rs`, 5 tests): the pin's `radial_sweeps_close_at_the_hinge_or_on_two_annular_caps` (hinge and both-capped, volume `pi h (300 - b^2 - b t - t^2) / 3`) and `a_cylinder_whose_top_rounds_...` (volume cylinder less frustum) against the difference of the frusta, at an axis-aligned and a tilted far frame; narrowing and widening cones, hinges at either end, a tube, crossing slopes. Each: counts, closed-solid structure with Euler-Poincare of genus 1, the BR8 oracle, volume, area and centroid to 1e-12 with the mass estimate's error within, face kinds, and membership of each face against the CSG difference of two frusta; determinism; size and annulus refusals. The pin's stitch, outward-check and attribute-copy parts have no counterpart (nothing to stitch; attributes belong to the caller of the local operation).

### 13.6 BO10 as built (11-28)

`crates/scx-boolean/tests/algebra.rs` (about 1,100 lines, 15 live tests, 12 ignored; the live ones run in about 10 s wall under nextest).

**Shape.** An expression over a pool of 11 placed parts (8 in general position: three boxes, a cylinder, a sphere, a rotated block, a cone, a torus; then a block sharing A's z planes, a block abutting A's x = 10 face, and a sphere centred on A's x = 10 plane) is built twice: as bodies through `boolean`, and as a testkit `Csg` oracle. Two expressions are equivalent when each result passes `scx_check::contract` (`Contacts::Radial`), volume and area agree to 1e-9 relative plus the Estimates' errors, solid-region and shell counts agree, and 1,000 seeded points classify as the oracle says wherever it is certain at 2C. Laws: commutativity, idempotence (two separately built bodies), self difference, absorption, partition, inclusion-exclusion and partition in volume, De Morgan in a box (both forms), associativity, distributivity, rigid motion (rotation plus a 1e6 translation), determinism (`scx_record::encode` bit-identical over two runs). N-ary order waits on BO11 and BO12. A case where any Boolean answers `Unsupported` is counted and printed per law (`--no-capture`), never asserted.

**Collars.** Coaxial r = 1.3 collars, bare and each on a block, overlapping over 2 or meeting in a ring, in both operand orders: union volume = vA + vB - pi r^2 l, intersection = pi r^2 l, both differences, one solid of one shell. They pass today. The #20408 case is the blocks-overlap-2 variant (its geometry: r = 1.3, l = 2); the pin's two records wait for DT4.

**Known failures.** The failing cases are listed in `KNOWN` as (cause, law, label); the live sweeps skip them and one ignored test per cause runs its cases and asserts they pass, so the suite keeps them. Causes: wrong-points (9, silent wrong geometry), seeds (28), loops (16), build (17), classify (10), loosespan (9), uvundecided (6), piece (1), contract-undecided (16 incl. far), contract-nobox (23 incl. far).

**Choices, for and against.**
- Case-granular known list with one ignored test per cause, against one `#[ignore]` per law: a whole-law ignore hides the cases that pass, and one per case is 135 tests. Cost: the table is data that BO10b must delete as it fixes.
- Judging each result against its own oracle as well as against its twin: a law could hold between two equally wrong results (both Booleans missing the same region); the oracle catches that.
- Sphere centre: the first pool put the sphere's centre on A's x = 10 plane, which fails with `Seeds` in every combination with a box. A sphere off every plane passes those, so the pool holds both: the general one for the laws, `sphereOnFace` as a real contact case.

### 13.7 MK7a frames as built (11-28)

`scx_make::{Path, Piece, Stop, Station, Rotation, Corner, FrameError, frames}` (`frames.rs`,
`tests/frames.rs`). `Path::new(&[Piece], &Document)` validates (1 to 1000 pieces; lines longer
than the resolution; arcs and helices off their axis, angle in (0, 1000 turns]; pieces meeting
within the resolution; a corner that doubles back refused) and measures once, so `frames(&Path,
&[Stop]) -> Result<Vec<Station>, FrameError>` cannot fail on the path. A `Station` is the point,
unit tangent and a `Rotation` (the transport from the path's start, columns, `apply`, `after`,
`inverse`, `defect`). Closed forms as the pin's: identity on a line, the angle run about the axis
on an arc, the screw then `psi = -b phi / sqrt(a^2 + b^2)` about the tangent on a helix, the least
rotation `t0 x t1` by the angle at a corner (none within `G1_TANGENTS` = 1e-8 of each other).
`Stop::Corner(j)` is the mitre plane (half turn, bisector tangent); `Corner` carries the angle,
the joint, the fraction and the stretch `1 / cos(angle / 2)`. Rodrigues runs on `fmath`
(deterministic across platforms; the pin used the platform's libm).

Decisions and cases:
- **Own `FrameError`, not `MakeError`.** 75 §3.6 writes `Result<Frames, MakeError>`. For: a frame
  law has no body, three of `MakeError`'s four kinds cannot occur, and a new `Refusal` variant
  would touch `error.rs` that MK4, MK9, MK13 and MK14 also edit. Against: one error type for the
  crate. The sweep (MK7b) adds `From<FrameError> for MakeError` when it needs it; no change here.
- **No `Cx`, no laws.** The signature is pure (`&Path`, `&[Stop]`): the work is closed form and
  O(pieces) per stop, no budget, scratch or cancellation to meter. Twist, end scale, orientation
  and rails are MK7b's (the pin's `Laws`, `Orientation`, `RailFit`, `railed`); `fractions` (the
  skin's one station per degree rule) is a SolidCraft convention for the skin and goes with the
  sweep. The pin's `Stop::Corner` mitre stretch is `Corner::stretch`, applied by the mitre's user.
- **Literals into the document.** The pin's `1e-9 (1 + |P|)` for "pieces meet" and for a line
  with no length is the document's resolution here; `G1_TANGENTS` stays a named constant (the
  procedural sweep's rule, which step 5 will own; the sweep must use the same one).
- **Axis by type.** `Piece` takes a `UnitVec3`; the pin's "unit within 1e-12" check and its
  re-normalising are gone.
- `Piece` and `Stop` are `#[non_exhaustive]`: MK7b may add pieces when step 5 certifies a
  spline path's frame (#2572).

Truths (`tests/frames.rs`, 10 tests, closed forms, 1e-12): the line is the identity; a planar
arc turns the frame by the angle run (point, tangent, frame); a whole helix turn leaves the
tangent and turns the frame by `-2 pi b / sqrt(a^2 + b^2)` (pin :74); at a quarter and a half
turn the start normal becomes `cos psi N + sin psi B`, right- and left-handed (pin :505);
parallel transport turns about no axis along the chord on tilted helices of both hands and a
tilted arc (skew part along the chord < 1e-9, pin :569); a right-angle corner is a quarter turn
with the mitre the eighth and stretch sqrt 2, a fraction at the joint is the second piece's
start (pin :166); the corner is the least rotation at 1, 30, 90, 135 and 179 degrees (new); a
corner then a helix compose (pin :536); a kink of 1e-10 is no corner and 1e-7 is one (pin :768);
refusals of a doubling-back path, a gap, no pieces, a degenerate or axis-start piece, a negative
angle, fractions outside [0, 1] or NaN, and a closed arc's whole-turn identity. Orthonormal
defect below 1e-13 throughout. Removed on purpose, with the file header listing them: the pin's
twist/scale, orientation, rail, `fractions` and limit tests (MK7b's laws), and the stretch
half of the corner test (checked as `Corner::stretch`).

Cost: new code only, no existing path changed. A station walks the pieces before it, one
`sin_cos` and about 40 flops per rotation composed; 1,001 stations of a helix (the transport
test) run in a few milliseconds in a debug build.

### 13.8 MK9 as built (11-28)

`scx_make::loft::{align, align_banded, pair}`. `align(first, second, &mut Budget, &Stop) ->
Result<Alignment, PairError>` pairs two closed loops of `scx_planar::Segment` (lines, arcs, whole
circles) given in one plane's coordinates. `Alignment { start, events: Vec<Event>, cost,
optimality }`. Its events run round both loops in order, from vertex 0 of the first and vertex
`start` of the second: `Pair { first, second }`, `FirstAlone { vertex, on }` or
`SecondAlone { vertex, on }`, where `on` is the other loop's segment the lone vertex lands on.
`pair(&[&[Segment]], ..) -> Pairing { alignments }` pairs each section with the next. It checks
every section before pairing any, and refuses a section wound the other way from the one before
it. `align_banded(first, second, cells, ..)` always runs the band. It is public so that the band
can be measured against the exact search. `MAX_CELLS = 4_000_000`. The cost, the
divide-and-conquer search over starts and the band are the pin's (`loft_sections.rs`). The area
and centroid are in closed form for arcs.

Decisions, with cases for and against:

- **The banded answer is always `Unproven`, against Q5 and the 04 row's "`Proven` if its walk
  avoids the band's edge".** That rule proves nothing. The band has one start, so a walk that
  keeps off the band's edge is least only among walks inside the band from that start. A cheaper
  walk can start elsewhere or run wholly outside the band.
  - The pin's own case shows this (in-module test
    `a_banded_walk_off_its_edge_is_not_the_optimum`). It pairs a 10 x 1 rectangle in 498
    segments with a 1.5 x 1 rectangle in 500, through a band of 40,000 cells. The band's walk
    keeps off its edge and matches the whole grid's walk from the same start, yet the search
    over every start finds a walk more than 1% cheaper.
  - On random star polygons (`the_band_is_an_upper_bound_on_the_exact_search`: 200 pairs of 20
    to 120 vertices, half-width n/10), the band finds the optimum in 97 of 200. It is up to
    108% above it in the worst case.
  - For: the status is never false, and the type already carries both values.
  - Against: every loft past the limit is `Unproven`, even when the band happens to be optimal
    (on regular shapes it is: `on_regular_shapes_the_band_finds_the_optimum`).
  - Considered and not built:
    - A cheap lower bound, `Σ_i` (the least cost of a step into row `i`): O(mn) time and linear
      memory. It would turn some `Unproven` answers into `Optimal` when bound and cost meet, but
      on these shapes it is far from tight.
    - An exact search confined to a band by a proof of the band's width (Ukkonen-style
      cut-offs). Its bound needs a least step cost, and UNPAIRED alone (0.1) gives a width near
      the whole grid.
  - Either can come later without changing the API. The 04 row should drop "`Proven` if ...".
- **A moved band is pinned to the walk's ends (truth over pin).** The pin re-centres the band on
  the middle of each row's run of the walk. When the walk runs along the last row for more than
  the band's width, the last row's centre falls short of the end. The end then lies outside the
  band, and the pin's walk fails with "unpaired".
  - Seen here on a random 27 x 102 pair with half-width 1, now in
    `the_band_is_an_upper_bound_on_the_exact_search`.
  - `settle` makes the centres run from the start on row 0 to the end on the last row, never
    back.
  - With that, each row's band meets the next and the end is inside. The band's walk is always
    found when h >= 1.
- **Exact means exact.** `the_exact_search_is_the_brute_force_optimum` (integration test, 600
  random star polygons of 3 to 7 vertices) checks against code that shares nothing with the
  search. That code recomputes each step's cost from the polygon's vertices (shoelace centroid,
  unit area) and enumerates every monotone walk from every start.
  - The search's cost equals the brute-force least to 1e-12 relative.
  - Its events form a walk round both loops, and their recomputed cost is the reported one.
  - The in-module `the_search_finds_the_brute_force_optimum` does the same over the module's own
    step costs for 1 to 6 vertices, with arcs and whole circles among them.
  - Turning either loop's numbering round leaves the cost unchanged to 1e-9.
- **Its own error type, `PairError`, not `MakeError`.**
  - Variants:
    - `Refused(PairRefusal)`: `Empty`, `NotFinite`, `Degenerate`, `NoArea` or `Winding`.
    - `UnsupportedSegment`.
    - `TooLarge`: even a band one column wide does not fit.
    - `Exhausted` and `Stopped`.
    - `Unreached` (`Defect`).
  - For: MK4, MK13 and MK14 edit `error.rs` at the same time, and the pairing has no solid to
    refuse. MK10 maps it into `MakeError` with the section index kept.
  - Against: one more error type for a caller of `pair` alone.
- **Work.** Each cell visited charges one unit to the caller's `Budget`, and the `Stop` flag is
  polled once per grid row. Which search runs depends on the fixed `MAX_CELLS`, not on the
  remaining budget, so the answer is a function of the input (R6).
- **Scope left to MK10:**
  - winding the sections alike;
  - carrying a shared frame from plane to plane (the caller gives every section in one plane's
    coordinates);
  - cutting segments where lone vertices land (the pin's placement by share of length);
  - splitting whole circles at the neighbouring section's directions;
  - merging runs of one curve.
- **75 §8.2's `fork_ea59e9c_section_orders` is a blend cross-section test, not loft pairing.**
  In its place are the pin's loft-pairing outcomes, as tests on the alignment:
  - square to diamond;
  - 5000-gons and a 3000-gon with a 5000-gon, in the band;
  - split 5000-segment rectangles pairing all four corners;
  - both comb cases of 1100 teeth, past the limit even for their corner polygons.

Measured (release on a 24-core build machine, throwaway timer; random star polygons):

| m x n | search | cells charged | ms |
|---|---|---|---|
| 10 x 10 | exact | 558 | 0.02 |
| 100 x 100 | exact | 76,835 | 2.0 |
| 500 x 500 | exact | 2.59 M | 67 |
| 1000 x 1000 | exact | 11.5 M | 306 |
| 1999 x 1999 | exact | 49.7 M | 1,476 |
| 2000 x 3000 | band, 8 passes | 41.6 M | 1,051 |
| 3000 x 3000 | band | 6.65 M | 161 |
| 5000 x 5000 | band | 7.49 M | 180 |
| 10000 x 10000 | band | 7.86 M | 198 |

- The exact search charges about 11 to 12 times the grid at 1000 to 2000 vertices (`m·n·log2 n`
  / 2 plus overlaps), at about 30 ns a cell.
- A band charges at most `BAND_PASSES` times `MAX_CELLS` plus the corner polygons' search, so
  about 1 s at worst.
- A band of a tenth of the grid on the same random shapes is 1% to 5% above the optimum.

Tests:

- `crates/scx-make/tests/pair.rs`: 14 tests.
- In-module: 3 tests (brute force over the module's costs, the half disc's closed-form area and
  centroid, and the band counterexample).
- None removed.
### 13.9 BO17 `planar` as built (11-28)

`scx_boolean::planar(a, b, op, cx)` (module `planar.rs`, tests in `planar/tests.rs`). It reads
each face of each operand as a `scx_planar::Region` in the plane of `a`'s first face, builds the
arrangement of the two boundaries and builds the result as a sheet of planar faces in a new
draft, loops given exact pcurves by `Draft::exact_loops`. No stitch (Q2), no 3-D stage.

Decisions where 75 left a choice:

- **Selection by sides, not by cells.** Every piece between cuts is grouped with the pieces on
  the same curve; each group knows the faces of each operand on its left and right (from the
  operands' own uses, whose loops keep material on the left, else by `Region::classify` of its
  midpoint). The result's boundary is the groups whose left and right differ in the operation's
  answer. For: no angular ordering is needed to decide anything but loop tracing; coincident
  and shared edges fall out; an operand of several faces sharing edges is its union. Against:
  a midpoint on an operand's boundary the other's pieces do not name is `PlanarUndecided`.
- **Arcs in pieces of a quarter turn or less** (a circle is four), each with its exact rational
  quadratic for `crossings` (which takes no arc past a half turn). The cut points of a meet are
  moved onto both curves by the closed form (line-line, line-circle, circle-circle, nearest
  root to the certified point): the certified `at` was 1e-6 off, which showed as 4e-8 relative
  area error. Joined back by chain at degree-2 vertices on one line or circle, so a union of two
  rectangles has 4 edges, a lens 2, a whole circle 1.
- **Tracing**: at a vertex with several kept pieces leaving, the first met turning clockwise
  from the way back, ties (within 1e-9 rad) by larger signed curvature. This is the one angle
  tolerance; it only orders pieces already known to leave one vertex.
- **One shell per result face** (a sheet shell must be connected; `finish` refuses a shell of
  faces apart or empty). An empty answer is a body with no faces.
- **Fresh draft, not `a.edit()`**: the result is mostly new faces and edges; lineage is
  `Origin::Operand(0|1, face)` for every face on the left of a result face's boundary.
  Edge and vertex lineage are Free.
- **Result plane** is `a`'s first face's frame (`b`'s when `a` has none); faces of `b` in the
  opposite frame are the same set.
- New errors: `Refused(NotCoplanarSheet)`, `Refused(BadProfile)`, `Unsupported(PlanarEdge)`,
  `Unsupported(PlanarUndecided)`.

Known limit: a result face whose hole touches its outer loop at a point (a disc tangent inside
a plate, subtracted) is not a `Region` and is `PlanarUndecided`; the answer needs a face with a
pinched loop, which `scx-planar` does not take. Two rectangles touching at a corner are fine; two discs tangent from outside are `PlanarUndecided` (open: not yet separated into the certified crossings of two tangent quarter arcs or the midpoint test).
Cost: all pairs of atoms with meeting boxes are crossed (quadratic in the worst case; the
perforated plate of 16 discs, about 80 atoms a side, runs in 0.3 s in debug).
### 13.10 BO21 as built (11-28)

`scx_boolean::boolean_given(a, b, op, &Incidences, cx) -> Result<Done<NoItems>, BooleanError>`
(`crates/scx-boolean/src/given.rs`); `boolean` is `boolean_given` with `Incidences::NONE`, one
pipeline (R1). `Fact` is `FaceOnFace { a, b }` (operand 0's face, operand 1's face) or
`EdgeOnFace { operand, edge, face }` (the face on the other operand); `Incidences` collects them
sorted and each once. A false fact is `Refused(BooleanRefusal::FalseFact { fact })`.
`scx_surface` gained `curve_on_surface(c, ce, s, se, res, budget, stop) -> Result<OnSurface, _>`,
`OnSurface::{Proved, Isolated, Leaves}`: `intersect_curve`'s answer, labelled by how it was had.

How a fact is used and checked:

- An edge fact (stated, or implied by a face fact: every edge of either face on the other's
  surface) sends stage 2's edge-face test through `curve_on_surface` in place of
  `intersect_curve`. On a plane the curve's height is bounded in closed form (a line's two ends;
  `|gamma| + hypot(alpha, beta)` on a conic; a few products on the open conics); within
  `tol - rounding` the answer is the overlap's clip with no root isolated, one budget unit against
  two a piece. Otherwise it is `intersect_curve`'s own isolation (`isolate`, the old body moved
  out unchanged).
- A stated edge fact is false when the roots show the edge leaving (`Leaves`), or when stage 2
  never pairs that edge and face and the same test, run on its own, says so; a fact on a missing
  edge or face, an edge with no curve, or a third operand is false too. An implied fact never
  refuses: one the bound does not prove is answered by the roots as without it.
- A face fact is false when stage 4 finds no coincidence of exactly those two faces. Stage 4
  still runs for that pair.

**Same result, the argument.** The only change a fact makes is `intersect_curve` replaced by
`curve_on_surface` for one edge and face. `intersect_curve` answers `Overlap` pieces
(`path.clip`) exactly when every event (roots, turns, the ends or the closed curve's probes) is
within `tol`, `Event::new` computing each distance in floats. A proof bounds the true height by
the computed bound plus its rounding, and each event's computed distance by the true height
plus its rounding; both roundings are a few steps on numbers no larger than the coordinates'
size `m` (curve, axes origin, surface size), and `rounding = 64 eps m` covers both with room. So
proved implies every event near, and the answer is the same `path.clip(c, target, se)` call,
the same bits. Not proved, it is the same function. Tested to the bit: `scx-surface`'s
`a_stated_curve_is_answered_as_intersect_curve_answers_it` (300 on-surface cases of every kind
at two tolerances, plane cases all proved, curved ones all isolated at equal work, and the
random pairs of 10 seeds) and `scx-boolean/tests/given.rs` (bodies compared by
`scx_record::encode`, changes compared, volumes against closed forms).

**A false fact cannot change a result silently.** A stated fact is judged by the very test that
answers without it, so a false one meets a disagreement and is refused, and no answer is built
from it. A proved fact is true by the bound. An implied fact is never trusted: unproved, it is
the plain test. So the result with facts is `boolean`'s result or a refusal.

Decisions (analysis and adversarial pass):

1. *A closed-form bound on planes only, not SU6's certified gap.* The first build used
   `curve_surface_gap` (any analytic kind). Measured, it took 5 points for a line on a plane
   against isolation's 2 units: the facts cost more work (387 against 366 on the stacked
   cubes). For: analytic isolation is itself a closed form (two Bernstein solves a piece), so no
   bound saves much and only a closed form undercuts it; the plane is where stated facts arise
   (mates, bosses, pockets). Against: curved coincidences (coaxial walls, BO22's tangent
   contacts) get no saving and no proof independent of the roots; BO22 can add the gap where
   isolation is ill-conditioned (a double root), where it is worth more points.
2. *Face facts are checked by running stage 4, not skipped.* For: the analytic pair is O(1) and
   decides the coincidence the build needs; trusting the fact would need the overlap's regions
   anyway. Against: a NURBS face pair would gain from skipping; that waits on BO23.
3. *Implied edge facts never refuse.* For: the caller stated the faces, not each edge; an edge
   of a coincident face that the bound misses (rounding near `tol`) is still on it as the roots
   say. Against: a face fact whose faces' edges leave each other's surface is only caught by
   stage 4, which does catch it.
4. *Unpaired stated edge facts are checked alone* (`judge_edges`, with stage 2's reach and
   resolution). For: else a fact on an edge far from the face is silently ignored. Against: one
   extra test per such fact (cheap; the NURBS case is `Unsupported` as stage 2 would be).
5. *Work may be less, never more*, so a budget that exhausts `boolean` may suffice for
   `boolean_given`; the result when both finish is the same.
6. *The rounding constant is an error-model argument* (`64 eps m`), not machine-checked; the
   test at 0.99 `tol` (proved) and 1.5 `tol` (leaves, as `intersect_curve`) brackets it.

Measured (throwaway prints and timers, removed; debug counts, release times):

| case | facts | edge-face isolations | work | time (release, us) |
|---|---|---|---|---|
| stacked cubes, union | one face pair | 41 -> 34 (7 proved) | 366 -> 359 | 229 -> 227 |
| stacked cubes, union | 8 edges | 41 -> 34 (7 proved in stage 2, 1 unpaired fact proved alone) | 366 -> 360 | 232 -> 236 |
| stacked cubes, subtract | 8 edges | 41 -> 34 (7 proved in stage 2, 1 unpaired fact proved alone) | 363 -> 357 | 175 -> 170 |
| boss on a plate | rim circle | 2 -> 1 (1 proved) | 38 -> 35 | 57 -> 56 |
| boss on a plate | disc face pair | 2 -> 1 (1 proved) | 38 -> 35 | 63 -> 63 |
| hole through the plate | 2 rims | 5 -> 3 (2 proved) | 75 -> 69 | 108 -> 108 |
| coaxial cylinders | wall pair, 2 rims | 12 -> 12 (curved: none) | 102 -> 102 | 156 -> 168 |

  Per call (release, 200,000 calls, line and circle on a tilted plane): `intersect_curve` 205-223 ns
  and 842-851 ns, `curve_on_surface` 65 ns and 100 ns. The Boolean's times move within the main
  box's noise (two runs differed by up to 25 % on one case): stage 2's isolations are a few
  microseconds of 60-240. So on analytic planes the facts save the roots, not the Boolean's time;
  they matter where the test is dear (BO22's tangent pairs, NURBS once BO23 lands).

Tests: `crates/scx-boolean/tests/given.rs` (10): a stated face pair, stated edges (union,
subtract, intersect), a boss on a plate (rim, disc, through hole), coaxial walls, false edge facts
(unpaired, crossing, beside true ones), false face facts (parallel, perpendicular), facts naming
what is not there, near the coincidence (0.25 and 0.9 zone judged as the test judges, 50 zones
refused), no facts is `boolean`, `Incidences` order. `crates/scx-surface/tests/intersect.rs` (+2):
the bit-for-bit comparison above and the line just off a plane. None removed.

Open: curved bounds (a cylinder's ruling and parallel have closed forms too) if a caller's
curved facts show up in profiles; BO22 reuses `OnSurface` and `Fact` for tangent pairs.

### 13.11 MK15 as built (11-28)

`scx_make::face(&Surface, &Bound, &Document) -> Result<Made<FaceId>, MakeError>` makes a
sheet of one face on a plane, cylinder, cone, sphere or torus; `Bound` is `Box(Extent2d)` or
`Region(Region)` (in uv). Code in `scx-make/src/face.rs`; tests in `tests/face.rs` (8).

- **The uv is the pin's, not the angle.** Every turned kind's `u` (and a sphere's or torus's
  `v`) is a rational quadratic parameter, one arc per quarter turn, so an iso circle's
  sweep is `Arcs::angle(u1) - Arcs::angle(u0)`, not `2 pi du`. The first build assumed
  proportional and the writer refused the loop (`LoopOpen`); the closed forms in the tests
  take their angles from the surface's own points (`atan2`, heights), not from `Arcs`.
- **Box.** Handles a whole period (the seam is one edge used twice), a pole (sphere
  `v = 0` or `1`, cone apex: a collapsed edge; snapped within 1e-12) and a torus's both
  ways, so the constructors' whole faces are boxes (`[0,1]x[0,1]` torus: 1 vertex, 2 edges).
- **Region.** On a plane it is `sheet` of the region (arcs exact). Elsewhere only lines
  along `u` or `v` are iso-lines, so any other segment, and any on a pole, is
  `Unsupported::Segment`; extent past a period or the domain is `Refusal::UvBox`.
  Case for also taking arcs in uv on a cylinder (helices, ellipses): not exact, so not
  built; they would need the fitted writer.
- **NURBS** is `Unsupported::NurbsFace` until MK15b.
- Errors added: `Refusal::UvBox`, `Unsupported::NurbsFace`.
- Not ported: the pin's face-reading tests (step 6) and NURBS faces (MK15b).

### 13.12 MK4 as built (11-28)

`scx_make::revolve(&Region, &Frame, Turn, &Document) -> Result<Made<RevolveFaces>, MakeError>` and
`scx_make::revolve_sheet(..) -> Result<Made<RevolveSheetFaces>, MakeError>` turn a line-and-arc
region in the frame's `x, y` plane about the frame's y axis. `Turn::{Full, Partial { from, to }}`
(radians, right-handed about y, zero at the region's plane). `RevolveFaces { start: Option<FaceId>,
end: Option<FaceId>, walls: Vec<Vec<Wall>> }`, walls `[loop][segment]` in the order given to
`Region::new`; `Wall::{OnAxis, Face(FaceId), Split { before, after }}`. Every wall is built
directly with exact pcurves: a line is a plane, a cylinder or a cone; an arc is a sphere (centre on
the axis), a ring torus, or a horn torus (its circle touches the axis); every surface's frame is
`(axis point at the piece's height, z = axis, x = the start radial)`, so the parallels are the
surface's u lines and each meridian its v line at `u = 0` or `u = sweep`.

Decisions, with cases for and against:

- **Touches by the zone, crossings refused.** A profile point within the resolution's zone of the
  axis is put on it; a segment whose both ends are on it is `Wall::OnAxis` and sweeps nothing; a
  vertex on it is a pole (a collapsed edge per cone, sphere or horn-torus face; a plane passes
  through it). A segment reaching more than the zone across the axis (its side-mapped radius
  minimum, an arc's interior included) is `Refused(CrossesAxis { at })`. For: the pin's meaning
  (it splits a touch and refuses a crossing) with no new predicate; the zone is what makes two
  points one in the finished body. Against (the adversarial row in 04 11-28): a touch within the
  zone of a crossing is decided by the zone, not by certified bounds or `Unproven`. The profile is
  exact data in a `Region` (lines and circles from `f64`s), so the only uncertainty is the rounding
  of a radius computation, many orders below the zone; `Unproven` would never fire. Recorded as a
  deviation.
- **Interior axis touch: split, or re-seam a whole circle.** An arc whose circle touches the axis
  at `T = (0, c.h)` with `T` inside the arc is split there into two pieces (`Wall::Split`, both
  horn-torus faces); a whole circle touching the axis is re-seamed to start at `T`, so it is one
  horn-torus face whose loop runs from the pole (WR-590's three-face partial turns and one-face
  full turn). An arc whose circle crosses the axis outside the arc (`|c.r - r| > zone` with
  `c.r < r`) is `Unsupported::SpindleTorus`: its surface is a spindle torus, which `scx-surface`
  does not have; the pin keeps a revolution surface there (not ported).
- **Horn torus support across crates.** `Torus::horn(frame, radius, sense)` (major = minor) and
  `Torus::pole() -> Option<Pole>` (`v = 1/2`, the centre). The pcurve writer, `check/geometry.rs`
  and the testkit oracle read the torus pole as they read the sphere's and cone's. The writer's
  pole-side rule changed from `side_v < pole.v` to `side_v < start.v`: the horn pole sits inside
  the torus's periodic v range (at `v = 1/2`, or `3/2` a period on), so only the pole use's own
  start v says which side of it the loop is on; for the sphere and cone, whose poles are at the
  ends of the v interval, the two rules agree. `scx_make::torus` still refuses a horn torus
  (`Unsupported::HornTorus`); with the pole now supported that refusal could be lifted (open).
- **The plane snap rule at the mean height, not the start's.** A line whose ends are within the zone
  in height is a plane at their mean height; within the zone in radius, a cylinder at their mean
  radius. For the mean: each end is off by at most half the difference, not all of it, and the rule
  is symmetric in the segment's direction (a clockwise loop gives the same body). Against: 75 §3.6
  places it at the start vertex with the end's distance recorded as its accuracy; that distance is
  within the resolution, so `VertexRecord::accuracy` would be `None` either way. Deviation.
- **Orientation from the profile's side.** The side is `+1` when the region's far reach is on the
  frame's `+x`, else `-1`; each loop's walls are written for the solid on the left of the stored
  traversal and reversed (list and senses) for side `-1`. Each surface's sense follows from the
  piece's direction (`Forward` iff `o * dh > 0` for a cylinder or cone, iff the arc's turn agrees
  with `o` for a sphere or torus, the plane's normal against `o * sign(dr)`), so no orientation
  pass is needed.
- **A hole of a full turn is a void.** Its walls close on themselves apart from the outer loop's,
  so they get a void region's shell in front and a new shell of the solid behind
  (`Assembly::hollow`); a partial turn's caps join every loop into one shell. `revolve_sheet` puts
  each loop's walls in an outside shell of their own (`Assembly::sheet_shell`), since without caps
  the loops are pieces apart. `Assembly` also gained `sides` and `face_between`.
- **Caps** (partial turns) are planes through the axis, `z = D_from x Y` at the start and
  `Y x D_to` at the end, holding the region's loops at that angle.
- **Refused turns.** `Refusal::Turn { from, to }` for angles not finite, `to <= from`, a whole turn
  or more apart, or with the farthest point less than the zone from the other cap either way round.

Deviations from 75 §3.6 and its 04 row: the axis is the `Frame`'s y axis (no separate `Axis` type);
angles are `f64` radians (no `Angle` type in `scx-num`); caps are `Option<FaceId>` and walls a
`Wall` enum rather than `Vec<Vec<FaceId>>`, since an on-axis segment has no face and a split one
two; `revolve_sheet` takes a `Region` (closed loops), not an open path; touches by the zone, not
certified bounds; the plane snap at the mean height.

Measured (release, throwaway timers, best of 20 runs of 50, tilted frame): a 3 x 4 rectangle
revolved whole 21 us and a quarter turn 27 us (extruded: 13 us); a circle to a ring torus 9.7 us
against `scx_make::torus`'s 9.3 us; a half disc to a sphere 8.5 us against `sphere`'s 8.1 us;
WR-590's quarter turn 12 us. So a revolve costs what the matching primitive does, within 5%. The
cost added to existing paths is one comparison per pole lookup (`Torus::pole` in the writer and the
check) and the changed pole-side comparison; not measurable.

Tests (`crates/scx-make/tests/revolve.rs`, 16): a tube from a rectangle (plain and tilted frame,
full, quarter, 3.5 rad and almost-whole turns), the mirrored side, a solid cylinder (axis segment),
a cone to an apex and a frustum, a sphere from a half disc, a ring torus, a region with a round and
a square hole (voids for the full turn, genus 2 for partial), the plane and cylinder snaps, WR-590
(circle centre (5, 0) radius 5, quarter, half and three-quarter turns from 0, pi/2 and pi, and the
full turn), an arc split at its interior touch, the sheet, and the refusals (crossing by a line and
by an arc's bulge, spindle arc, eight bad turns); determinism; the given-order walls of a clockwise
loop. Each solid: Pappus's volume and area to 1e-9 with `scx_mass`'s estimate within it, face kinds
per segment, the closed-solid structure (fins, sides, exact pcurves, Euler-Poincare) and the BR8
oracle (the pinched WR-590 and split bodies: the oracle and exact pcurves, without the Euler count).
No pin test was removed. The pin's `issue_2905_revolved_vase` is a chamfer test on a captured
body, not a revolve test; its meridian (a circle of centre `(-16, 33)`, radius `sqrt 1690`, between
`z = 30` and `60`) is the far arc of a circle crossing the axis, an apple (spindle) torus, which this
revolve refuses as `Unsupported::SpindleTorus`. A real report needs it: the spindle surface is the
first open question.

### 13.13 SF1 as built (11-28)

`scx_surface::project_curve(&Curve, Extent1d, &Surface, along: UnitVec3, Resolution)` and `scx_surface::drop_curve(&Curve, Extent1d, &Surface, Resolution)` return `Result<Vec<Projected>, ProjectError>`. A `Projected { curve, extent, from, kept: Option<ParamShift> }` is one piece: the image curve over `extent`, the image of the source over `from`, run the same way, with `kept` the source-to-image parameter map where the image is an affine copy (`None` for the cylinder's arc, where only the ends and the order hold). `ProjectError` is `NeedsFitter { curve: CurveKind, surface: SurfaceKind }` (Unsupported, `projection_needs_fitter`: the one variant the step-5 fitter replaces), and the Refused `Along`, `Flattened`, `OnAxis`, plus `Image(CurveError)`. The module is `crates/scx-surface/src/project.rs`, with the closed-form table and the pin-vs-here table in its header.

Closed forms: onto a plane, every curve kind, through the projection affine and `Curve::transformed` (a circle to its exact ellipse on principal axes, a NURBS curve by its control points); onto a cylinder, a line parallel to the axis projected (0, 1 or 2 rulings) and a line dropped in a plane through the axis (a ruling per side) or square to the axis (an arc at its height); onto a cone, a line in a plane through the axis, projected along the axis or dropped (a generator per side). Every other pair is `NeedsFitter`.

Decisions.
- **The whole projection line, both signs.** For: one answer for a plane (the pin's SMLib projection is also unsigned), no split where the curve passes through the surface, and a caller wanting one side filters by the image's side. Against: a caller casting "down" onto a cylinder gets both rulings; the pieces are ordered by `s`, so the first is the near side.
- **A miss is `Ok(vec![])`, not an error.** For: a projection line that misses the surface is a final answer, not a failure (no failing default either way: the vector says what happened). Against: the pin's `Error::Empty` lets a caller `?` it; the face caller in `scx-bodyq` decides that, after its trim, where empty can also come from the trim.
- **No surface extent or uv box.** For: the analytic kinds are unbounded (but the cone's nappe, handled), and the face's trim belongs to `scx-bodyq` (75 SF1 row). Against: the pin's drop clipped to the face's uv box first; that is now the body's step.
- **A cone's apex part is left out of the drop**, not returned as a point: the feet there are the apex alone, not a curve. A piece whose image has no length is left out; when none is left the answer is `Flattened` (a line dropped square through a cylinder's axis, a circle seen edge-on).
- **Predicates within the zone over the extent** (parallel, meridian, square): R5, chosen before building, no fallback. Against: a line a hair outside the zone gets `NeedsFitter` though its image is nearly a line; the fitter answers it, which is what it is for.
- **`kept`.** An affine image keeps the parameter through a `ParamShift` (the plane's, rulings and generators: `NONE`), so a caller can map vertices; an arc's angle is not an affine function of the line's `t`, so it says `None` rather than a wrong map.

Truths (`crates/scx-surface/tests/project.rs`, 14 tests, plus 2 unit tests): a circle projected along z onto a plane tilted by beta is the ellipse with semi-axes r and r / cos(beta) about the centre's image; a tilted circle projected onto a level plane has semi-axes r and r cos(gamma); for 64 random circles, planes and directions the image keeps the conjugate-diameter invariants a^2 + b^2 = |u|^2 + |v|^2 and ab = |u x v|; a line dropped onto a cylinder in a plane through its axis is the ruling at the radius (and splits at the axis); a chord dropped onto a cylinder is the arc at its height with the closed-form ends. Every piece is sampled: each point on the surface and on the projection line through its source point, or equal to the kind's `closest` foot. Ported from the pin: `a_line_above_a_box_drops_to_its_foot_on_the_top`, `a_line_beside_a_cylinder_drops_onto_its_wall_at_the_radius`, the #20836 seam chord (its geometry; the seam split and the area are the body's), and `a_segment_projects_onto_a_box_s_record_face_at_its_index` (both levels). Not ported: `a_curve_over_nothing_of_the_face_is_refused` (a face concern; the analogue here is a miss being empty) and `rational_quarter_circle` (a record form, not geometry).
### 13.14 BR11 as built (11-28)

**What was built.** `scx-brep` `draft/heal.rs`: `Draft::merge_faces_across(edge, &protected, &doc)`
and `merge_faces`, `join_edges_at(vertex, &protected)` and `join_edges`, and `heal`
(faces, then edges, to a fixpoint). A protected set is a `&BTreeSet<EdgeId>`; an id that is not a
live edge is `Absent` before any write (the pin's out-of-range `Argument`). Reports are typed:
`FaceMerge { absorbed, survivor, removed }`, `EdgeJoin { absorbed, survivor, vertex }`, and
`Left { at, why }` for a candidate (equal surfaces or curves) that was left, `Why` one of
`Protected, Sides, Orientation, Loops, Pcurves(e), Fins, Interval, Uses`. Each merge is planned
read-only and then written, so a refusal writes nothing.

**Decisions, each with a case against.**

1. *Equal means `==`* on `Surface` and `Curve` (the test cells and `Changes` already use), not a
   geometric coincidence test. For: exact, no tolerance, and the pcurves carry over unchanged;
   pieces of one cut face or edge are equal. Against: two coplanar faces of different operands
   (different frames) are not merged; the Boolean's own merge, which asks the surface
   intersection, keeps doing that. A kernel-wide "same surface up to frame" test is open.
2. *A pair is merged by its twins* (every edge only the two faces bound), walking loops across
   them (the Boolean's splice). A connected group of mergeable faces (union-find over the
   removable twins) is merged at once, one loop walk a group, so a long strip is linear. A group
   goes pair by pair instead when a twin's two uses lie in different uv windows (a seam) or a
   protected, other-shell or one-sense edge lies inside it; the pair walk keeps seams and
   refuses what a group cannot. For: handles a disc in a plate (both loops vanish), collinear
   shared pieces and seams. Against: two code paths, and a dirty group is still quadratic.
   A first version merged pair by pair only: 17 s on the 4,102-face bar.
3. *Seams.* Twins are grouped by the uv offset between their two uses; the group in step with
   the first one in one window goes, the others stay as seams. A pair meeting across a seam only
   is written with `exact_loop`/`placed`. Against: windows are compared with 1e-7 in uv.
4. *Protected.* A protected edge is never removed and two faces sharing one never merge; two
   edges join only when both or neither are protected (the survivor is the lower id, the
   report names the absorbed one). Against: refusing every protected join would keep the
   `Id`s stable, but leaves a split line in many pieces and gives 16 edges where the pin gives
   15 (the pin's #21864 joins them too).
5. *Edge joins write no pcurve at a guess.* The two uses' pcurves must be equal Curve2 values
   running on (a turn on for circles and ellipses in uv); otherwise `Why::Uses`. Against: the
   wall of a cylinder whose seam lune has its window a turn off (`start` -2 pi) leaves its two
   circle arcs at the seam unjoined; closing that needs a uv shift of the rest of the loop
   (open, below). Rewriting with `write_pcurves` was rejected: it can fail after the edge is
   written.
6. *Joined accuracy* is the larger of the two, downgraded `Certified` to `Unproven` (a proof of
   each piece is not one of the whole); equal ones are kept.
7. *Survivor is the lower id*, so the result does not depend on which edge the walk met first.

**Tests (all in `tests/heal.rs`, 14).** Ported from the pin's `merge_tests.rs`: a split cube and
two unioned boxes (a lattice, 6 faces, 12 edges, 8 vertices, area `2(ab+bc+ca)`, volume `abc`,
the check passing), a box changes nothing and every planar generated kind changes nothing,
a picked edge keeps the faces apart (7, 15, 10), an unknown kept edge is refused before any
write, more than 4,096 kept edges (1,728 and 6,912 edges all protected), the 4,100-edge bar with
one kept cut. Not ported: `picks_near_nothing` and the point-list `MAX_POINTS` cap (ids are
typed, so neither failure exists); the stitch tests (`scx-stitch`). New: a disc in a plate, a
cylinder's wall, sides refused, one merge and one join by name, parts equal the whole.

**Measured** (a 24-core build box, unoptimised, whole test including the build and the check):
the 4,102-face bar, pair by pair 17.1 s, by group 4.2 s; the 20 x 20 x 20 lattice (2,400
faces, heal and parts) 0.25 s before, 0.21 s after. What is left of the bar's 4.2 s is the
edge joins: the bar has four 1,025-piece cube edges, and each of its ~4,100 joins rebuilds the
loops of its faces (clone and fin diff of a loop of about 1,000 uses). Joining a whole chain
at once would remove it.

**Open.** (a) join a chain of edges at once, linear in the chain; (b) the seam-window join
above (a generated cylinder's circles stay split at the seam lune, `Why::Uses`; for BO12 to
close with a uv shift of the rest of the loop); (c) merging coplanar faces of different frames (needs the intersection test of
`scx-ssi`, above this crate); (d) the Boolean's `build.rs` still has its own merge and join,
to be replaced by these calls (BO12).

### 13.15 RP1 as built (11-28)

**Built.** `crates/scx-repair`: `heal(&Brep, OnItemFailure, &mut Cx) -> Result<Done<HealCause>,
RepairError>` and `items(&Brep) -> Vec<Item>`. The one item kind so far is `Item::Cluster(Cluster
{ vertices, edges, survivor, reach, target })`. `RepairError` is 01 §5.1's shape: `Cause`, `Items`
(collect mode only), `Exhausted { item, work }`, `Cancelled`, `Defect`. Its `RepairCause` is
`OtherDocument` or `Heal(HealCause)`, and RP2-RP8 add their own causes beside `Heal`. There are 12
`HealCause` variants, each naming its entity: 6 refused (`Spread`, `TooLong`, `OffFace`,
`EndOffVertex`, `NotClear`, `NeighbourShort`) and 6 unsupported (`NeighbourKind`, `FaceKind`,
`Sliver`, `LoopVanishes`, `Pinch`, `NotFinite`). `scx-check` gains `Finding::VerticesWithinCoincidence
{ a, b, distance, allowed }` and its `Undecided` twin (research 74 §3.3's pair test, which `check`
did not have). These are found by binning the vertices in cubes of twice the widest reach and
testing neighbouring cubes, after the gaps and on the budget they leave; a pair the budget does not
reach is `Undecided { why: Exhausted }`, as a gap is (74 R6).

**How a cluster heals.**
- Vertices joined by a curve edge are clustered by union-find when their distance is certified
  within the larger reach.
- The survivor is the vertex of the largest reach (the lowest id on a tie). The target is the
  centre of the cluster's box, as in the pin.
- Edges with both ends in the cluster are squeezed: removed with their uses. Every point of the
  edge's curve must be within the survivor's reach of the target, enclosed over 16 interval pieces.
- Every other edge at the cluster must be a line on plane faces. It is rebuilt between its new ends
  over `[0, 1]`, and its pcurves are the line's projection.
- Each rebuilt edge is certified against the check's own allowances:
  - it is longer than its reach;
  - its end is at its vertex within `re + rv`;
  - the lift of each pcurve is within `min(re, rb)` of the line.
- The target is clear of every other vertex by more than the larger reach. The test uses the
  same cube binning, plus the targets of the items already done.
- No loop is left empty, with two or fewer lines, or passing the survivor twice.

**Choices, with the case for and against.**

1. **The pair finding covers every vertex pair, not only pairs an edge joins.**
   - For: it is the defect itself, and 74 §3.3 asks it of the whole body. A heal fixes what the
     check reports, so the finding must exist before the heal can be held to it.
   - Against: heal merges only pairs an edge joins. A pair with no edge between them stays a
     finding until a later item kind handles it (open question 1). The check now costs a sweep;
     see the measurements below.
8. **Cubes, not a sort along x.**
   - For: CAD bodies are axis-aligned. A block's faces put whole grids of vertices in one x
     slice, and the first build (an x-sorted window) tested those pair by pair: 3.33M units and
     139 ms at 9,602 vertices, 56% of the gaps' time. Cubes test only points within two sides of
     each other, and a pair within the reach lies in adjacent cubes, whatever the rounding.
   - Against: a body with one wide vertex makes the side large, and that brings back pair tests
     near it (still correct). Past 2^52 sides a body falls back to one cube, which tests every
     pair.
9. **Clearance after the gaps, out of budget = undecided.**
   - For: the check's contract is that a budget past the structure part never errors and never
     gives a false finding (`gaps.rs` and `structure.rs` tests). The first build charged
     clearance as a hard `Exhausted` before the gaps, and those tests failed.
   - Against: a pair left undecided for budget carries the bracket `[0, inf)`, which says
     nothing. A caller who wants the answer gives more budget.
2. **`heal` takes `OnItemFailure { Stop, Collect, Skip }` and has no rounds.** 75 §6.5 wrote
   `heal(body, cx)` with no mode.
   - For: 01 §5.2 makes the mode explicit and the default all-or-nothing. Each item is certified
     against the items done before it, so dropping a failing item cannot change a later item's
     answer. One pass is the fixpoint, and every `Skipped` is round 1.
   - Against: a heal that could only succeed after a later item lands (for example, a sliver that
     a later collapse removes) is not retried. RP3 may need rounds back, as a cap the mode states.
3. **No GAP_SHARE. The bound is `min(re, rb)`.**
   - For: the check's two use questions are "edge on face within `re`" and "use follows edge within
     `rb`". The pcurve is the exact projection, so one lift gap answers both. A heal certified this
     way passes the check by construction, with no factor to tune. The pin's 0.5 was a share of one
     tolerance.
   - Against: the bound is tighter than the pin's when `rb < re / 2`, so some clusters the pin heals
     are refused here. That is correct by the check, and no case in the tests needs it.
4. **Certifying the two ends certifies the whole line.**
   - For: the lift of a plane's projected line differs from the line by an affine function of the
     parameter, so its norm is largest at an end. The two ends are enclosed in intervals.
   - Against: this holds only for lines on planes. Curved neighbours need SU6's gaps (RP2), and are
     `Unsupported(NeighbourKind)` / `(FaceKind)` until then.
5. **The survivor keeps its accuracy, and face planes are kept rather than refit.**
   - For: "no accuracy grew" is the truth, and moving a face would need its other edges
     re-certified.
   - Against: a cluster whose re-ended edges leave a face by more than their reach is refused
     (`OffFace`). It is not healed by widening, which the pin's SMLib healer would do.
6. **A re-ended edge between two clusters is re-certified by the later one with both new ends.**
   - For: the plan stays per item, and the later certificate is about the final line.
   - Against: if the later item fails, the edge keeps the earlier item's line. That is still
     certified, since it was certified with the earlier ends.
7. **Edges.** repair → brep, curve, num, surface (normal dependencies); check, mass and testkit are
   dev-dependencies only.
   - For: RP1 code reads no finding at run time. The items are found from the body by the same
     test the check makes, and the tests hold each heal to `scx_check::check`.
   - Against: 75 §7 drew repair → check. RP8 or a caller that heals from findings would add it
     back.

**Tests.**
- `crates/scx-repair/tests/heal.rs`: 8 tests on constructed polyhedra.
  - The hip roof (ridge δ = 1e-4) heals to the square pyramid: 5/8/5. The check passes after; no
    accuracy grew; `|V − a²h/3| ≤ area × largest bound`. With the accuracy at δ/10 it has no item
    and is left unchanged. With the slanted edges at zone accuracy it is refused `OffFace`.
  - The chamfered prism heals its bottom pair and leaves its top pair, which would be a sliver.
    Stop, Collect and Skip each return their own answer.
  - Two chamfers share an edge between clusters.
  - Budget exhaustion is typed at its item, and so is the stop flag.
  - A body from another document is refused.
  - The heal is deterministic.
- `crates/scx-check/tests/clearance.rs`: 3 tests, at 10d, d and d/10 against the block's own
  distances, plus exhaustion. No test was removed.

**Measured.** These are throwaway timers (release build, not committed).
- `check` on testkit blocks, n = 5 / 10 / 20 / 40 (V = 152 / 602 / 2,402 / 9,602):
  - clearance adds V work units: 152 / 602 / 2,402 / 9,602, against 6,900 / 27,600 / 110,400 /
    441,600 before;
  - it takes 63-90 us / 0.30 ms / 1.3 ms / 6.0 ms, against the gaps' 3.9 / 15.4 / 61.3 / 244 ms
    (+1.5-2.5%).
  - The x-sorted first build took 2,172 / 22,142 / 256,482 / 3,329,762 units and 0.09 / 0.95 /
    10.8 / 139 ms.
- `heal` (Skip mode):
  - hip roof: 59 units, 11.5 us;
  - one chamfer: 115 units, 17.2 us;
  - two chamfers: 215 units, 29.8 us.
  - The check of the same bodies takes 116 / 207 / 244 us, so a heal costs about a tenth of one
    check. The pin runs two full checks per heal (75 §6.4).

**Open.**
1. Vertex pairs within coincidence that no edge joins: merging them changes topology, not edge
   ends. This is a later item kind, or ST1's identification.
2. Moving an edge can make it cross another face. No tube-clearance test is made, since 74 §3.3
   defines none, and the sweep covers only vertex pairs.
3. The cube grid is built per call, once in `check` and once in `heal`. A memo in `Scratch`
   could share it when a heal is followed by its check.

### 13.16 CN1 as built (11-28)

`CompatibleCurves::<D>::from_curves(&[NurbsCurve<D>]) -> Result<Self, NurbsError>` in `crates/scx-nurbs/src/compatible.rs` (77 §4.3, R77 row 4). Steps: each curve's knots are mapped affinely onto the first curve's domain (end knots exactly the first's ends, interior knots clamped inside; a curve already on that domain is borrowed, not copied); every curve is elevated to the highest degree with `NurbsCurve::elevate`; the union of interior knots (largest multiplicity over the curves after elevation) is inserted into each curve; if any input is rational the polynomial ones get unit weights. Inputs are `&`, the result is new, nothing is half done on an error. Inputs that already share a knot vector come back bit for bit. One new cause, `NurbsError::NoCurves` (Refused), for an empty slice; everything else refused is an existing cause: `Unclamped` (work needed and an end knot not of full multiplicity), `Unstable` or `Degree` (from elevation), `Multiplicity` (the mapping rounded two knots together).

Decisions.
- **No snapping.** Knots `0.5` and `0.5 + 1e-12` stay two knots (tested). For: the pin's averaging moves geometry unmeasured. Against: more knots on lofts of nearly aligned sections (77 §9 expects it).
- **Weights by homogeneous insertion and elevation**, not by a weight solve: exact up to rounding for any positive weights, so no rational case is refused for its weights. Polynomial curves among rational ones get weights of exactly 1.
- **Unclamped curves are refused when work is needed.** Insertion cannot reach the ends of an unclamped vector, so the knots could never be equal. Against: a surface net with unclamped rows that differ is refused; none exists in the corpus.
- **Per-knot insertion** (`insert_knot` copies the polygon per knot, O(k n)). For: reuses the checked single-curve edit. Against: a batch insertion would be O(n + k); not measured as a cost until MK10 lofts need it.

Work counts (the test family: 3 curves, degrees 1-5, mixed knot kinds and domains, summed over curves and families): knots in 1630, out 3819; control points in 1049, out 3096. The growth is elevation (each interior knot gains `q - p` copies) plus the union.

Tests (`compatible::tests`, 4): 1,000 parameters per curve of every sampled family evaluate as the source at the relabelled parameter to `1e-9 (1 + |x|)`, with knot vectors and degrees equal; unchanged inputs come back bit for bit; near knots stay two and polynomial curves get unit weights; the empty and unclamped refusals.

### 13.17 FT2 as built (11-28)

`scx_nurbs::fit` (77 §2, row FT2): `Param` and `parameters`; `averaging` and `approximation` knot
vectors; `interpolate` / `interpolate_on` at given parameters with `End` conditions; `least_squares`
and `weighted_least_squares`; one banded solver. Typed `FitError` (cause per variant, `kind()`,
`name()`); no flags, setters, out-params or failing defaults.

- **`End<N>`** is `Point` (G0), `Tangent{d1}` (G1), `Curvature{d1,d2}` (G2), `Hermite{d1,d2,d3}`
  (G3): parametric derivatives at the end, which the caller derives from a unit direction, a speed
  and a curvature. Every condition is stated; the pin's estimate of a missing end derivative from
  the data is not ported. Points are `[f64; N]` for any `N`, so a homogeneous fit is `N = D + 1`
  and `Spline::into_curve` / `NurbsCurve::from_homogeneous` finish it (replaces `FitPoint`).
- **Knots by averaging** use one rule for every pair of end orders `(a, b)`: the interior knots are
  the means of `p` consecutive entries of the parameters with the first repeated `a` more times and
  the last `b` more. It reproduces eq. 9.8 for `(0,0)` and the book's end-derivative knots for
  `(1,1)` (tested against hand values); the pin has five routines for these cases. Needs `a, b < p`.
- **Banded solve**: Givens rotations row by row (`band.rs`, private), `p + 1` entries per row of
  `R`, any number of right-hand sides at once. A square system (interpolation) and a rectangular one
  (least squares) use the same code; no pivoting, and no normal equations, so a fit's condition
  number is not squared (the pin forms `N^T N` and a dense LU). A pivot below
  `columns * EPSILON * max entry` is `Singular { column }` (a span without data, fewer data than
  unknowns): the named tolerance replaces the pin's `LU_TOL`.
- **Least squares** holds the end point and `order` derivatives by pinning the first and last
  `order + 1` control points (the `k`-th derivative at an end depends on `P[0..=k]` with a nonzero
  last coefficient), then solves for the rest.
- **Not ported** (decided): first derivatives at every point and the closed-curve conditions (CN3
  and MK12's), LU reuse between solves (`cpts_with_matrix`; a surface fit pushes its rows again),
  `square_to_plane`, and `approx_end_directions` (direction held, reach free). The last is FT3's:
  it needs the unknown reaches as two extra shared columns of the solve and is only meaningful with
  the certificate; the band solver takes them as a Schur step.

Tests (truth is the closed form, not the pin): samples of a polynomial of degree `d <= p` (p 1-6)
interpolate back to 2e-11 (relative to 1 + |value|) at 200 parameters, with and without its own end
derivatives; random data with every end combination up to G3 meets `C'`, `C''`, `C'''` to 1e-8
and passes through every point to 1e-10; least squares of exact data from a random spline on its
own knots gives the control points to 1e-10 (weighted, any positive weights: 1e-9), also with
Hermite ends held, and with perturbed data the held ends are still exact; the fit is a minimum
(200 random perturbations never lower the residual); a polynomial lies in the approximation-knot
space and is recovered; a homogeneous quarter circle; typed refusals; and a 20 000-point
interpolation (a dense matrix would be 3.2 GB).

**Cost** (throwaway counter and timer, release build, interpolation of `n + 1` points in 3D,
rotation-entry operations counted as `width + N` per rotation; nothing committed):

| p | n | rotations x (p+1+3) | per point | time |
|---|---|---|---|---|
| 3 | 100 / 1 600 / 25 600 | 791 / 13 454 / 212 065 | 7.9 / 8.4 / 8.3 | 32 us / 0.41 ms / 4.0 ms |
| 5 | 100 / 1 600 / 25 600 | 1 971 / 30 726 / 495 558 | 19.7 / 19.2 / 19.4 | 18 us / 0.27 ms / 5.4 ms |

Linear in the number of points (per-point work constant to 5% over a 256x range, and about
`p + 1` times its row width); a dense LU of the same system is `n^3/3 = 5.6e12` operations at
`n = 25 600` and `n^2` storage. Least squares adds the same per data row, plus the `O(p)` pinned
points. The pin's interpolation is dense (`RealMatrix`); its least squares is banded but through
normal equations.

For: one solver, no squared conditioning, linear time, every end condition linear and exact.
Against: the knot rule for `a >= p` is refused (a G3 end needs `p >= 4` by averaging; explicit knots
via `interpolate_on` accept `order <= p`); rotations cost about twice the flops of a Cholesky on
the normal equations at equal `n` (not measured against the pin; the factor is in `p`, not `n`).


### 13.18 MK10c as built (11-28)

MK10's certificate module on its own, without the loft: `scx_surface::certificate`. The public API:
- `Proof<P>`, one of:
  - `Proved { bound }`;
  - `Refuted { at, by }`, where `by` is a proved lower bound of the witness's distance and lies above the tolerance;
  - `Undecided { bound: Option<Length> }`, the bound reached.
- `on_analytic(&NurbsSurface<3>, &Analytic, Accuracy, &mut Budget, &Stop) -> Result<Proof<Point2>, SurfaceError>`, with `Analytic::{Plane, Cylinder, Cone, Sphere, Torus}` built by `Analytic::of(&Surface)`.
- `on_circle(&NurbsCurve<3>, &Circle, Accuracy, &mut Budget, &Stop) -> Result<Proof<f64>, _>`.
- `turn_about_axis(&NurbsCurve<3>, Point3, UnitVec3, Extent1d) -> Result<Turn, _>`, where `Turn` is `Positive`, `Negative` or `Undecided`.
- `span_bound(&NurbsSurface<3>, &NurbsCurve<3>, Node, Node, Accuracy, &mut Budget, &Stop) -> Result<SpanBound, _>`. `Node { t, uv }`; `SpanBound` is `Bounded(Length)` or `Unbounded`.

SU7's recognition certificate moved here from `recognize.rs`, which now calls the shared `prove`. RP7 gets a typed verdict for every kind, with no failing default.

Decisions.
- **Home crate: `scx-surface`, not `scx-make` as 01 had it.**
  - For:
    - The surface certificate is SU7's, already in `scx-surface`. It needs the analytic kinds, their axes and NURBS inversion, all of which live there. Putting it in `scx-make` would either duplicate it or make `scx-surface` call upward.
    - RP7 (`scx-repair`) and MK11's patch certification both use it. `scx-repair` sits above `scx-make` in the layers anyway, but nothing that only needs the certificate should have to pull in the constructors.
  - Against:
    - 01 named `scx-make`.
    - `scx-surface` gains about 1,900 lines (about 1,500 moved or new beside its tests) and so builds slower.
    - `span_bound`'s one caller, the loft (MK10), stays in `scx-make`.
  - The layers decide it: `span_bound` reads only a NURBS surface and curve, inversion and Bernstein forms. 01's `nurbs` row is amended.
- **R6 limits:**
  - `on_analytic` stops at 2^16 pieces in all;
  - `on_circle` at 2^12 pieces and 40 halvings of a span;
  - `span_bound` at 2^16 segments and 12 halvings.

  Past a limit the result is `Undecided` (or, for `span_bound`, the bound reached), never a refusal. Each piece charges one unit of the budget and polls the stop.
- **Refutation is rigorous.**
  - On a surface, a corner whose bound is past the tolerance becomes a witness only when its proved floor distance, less the corner's coordinate slack, is above the tolerance. The cone's floor uses both nappes: `|rho - |s|| cos`.
  - On a curve, a witness is a point read at a span's ends or middle, its distance less the evaluation's rounding.
  - The truth tests check that `by` is at most the closed-form distance at `at`.
- **No early exit for recognition.** `recognize`'s certificate no longer exits early on an unprovable piece. It now halves until each piece is proved, refuted or at a limit. A piece whose sampled distance is already within 1/64 of the tolerance is not halved (the pin's `NEAR`).

Deviations from the pin.
- `span_bound` returns the finite bound reached past its segment limit; the pin returned infinity. `Unbounded` is kept only for a piece with no bound at all: a weight not proved positive, or a diagonal of degree above 52.
- The pin's "sliver" term is replaced by a plain norm bound: `sqrt` of the sum of each coordinate's `max |N/D|`.
- `on_circle` bounds the distance from the circle itself, `hypot(|F/W^2| / (r + foot), |Z/W|)`. The pin ran two separate tests, one for the radius and one for the plane.
- `turn_about_axis` takes a `UnitVec3` and an `Extent1d`, where the pin refused a reversed interval at run time. It has no budget, since it makes one pass with no halving.
- New error `SurfaceError::OffRail { at }` (Refused) for a node outside the rail's domain.
- Not ported: `meridian_collapse` and `axis_foot`, which belong to step 8's blends.

Truths. There are 10 tests in `certificate/tests.rs`, judged against closed forms.

`on_circle`:
- The pin's `certificate.rs:1658` (an exact arc is proved, and is undecided at tolerance zero).
- `:1678`:
  - A 1e-4 bulge is refuted at 1e-7 with `1e-7 < by <=` its true distance.
  - It is proved at 1e-3 with a bound at least the largest of 1,001 samples.
  - Another radius, centre or plane is refuted.
- `:1709` (an arc 1e4 from the origin is proved at 1e-9).
- The budget is charged and the stop polled.

`turn_about_axis`:
- `:1575`: a quarter circle turns one way; it turns the other way about the opposite axis or reversed.
- `:1601`: a curve that turns back, or a radial line, is undecided; the part before the turn is proved.

`on_analytic`:
- The exact image of every kind is proved within 1e-9. This covers the plane, both senses of the cylinder and cone, the sphere and the torus, all in a tilted frame.
- A cylinder 0.01 wider is refuted with `by <= 0.01`.
- A cylinder with an interior control point moved out 1e-3:
  - At 1e-6 it is refuted, with `by` at most the closed-form distance at the witness.
  - At 1e-2 it is proved, with a bound at least the largest of 41 x 41 samples.

`span_bound`, on a cylinder of radius 10:
- A rail circle of radius 10.5 gives a bound in [0.5, 0.5005].
- A rail on the cylinder gives 2.2e-13.
- A free cubic rail's bound is at least its largest sampled distance.
- Nodes off either domain are refused with a typed error.

Costs, release build on the build server, timed with throwaway timers:
- **`span_bound`:**
  - The on-surface rail takes 1 segment in 6.7 us.
  - The free cubic rail takes 7 segments in 0.48 ms, about 70 us a segment.
  - The off-surface rail at a tolerance 5e-4 above the exact distance 0.5 takes 1,403 segments in 48 ms, about 34 us a segment. Each segment is one inversion of the rail's middle plus a diagonal of degree `du + dv` and its `max_ratio`. The segment count grows as the tolerance nears the true distance. MK10's caller should pass its loft tolerance, not the distance it expects.
- **`recognize`:** the suite takes 10 to 12 ms on base 7c1b668 and 10 ms on the branch. No change is measurable on its tests: the halving that replaced the early exit is bounded by `NEAR` and the piece limit.

### 13.20 BR11b as built (11-28)

**What was built.** `join_edges` and `heal` (`scx-brep` `draft/heal.rs`) join a whole chain of
edges on one curve at once: a maximal run through free vertices (each the end of exactly two
joinable edges, both or neither protected) is planned as one join, so each affected loop is
rebuilt once per chain, not once per pair. `join_edges_at` is unchanged and is the primitive:
a vertex that is in no chain, a chain of one pair, a closed ring, and a chain with any
refusal (`Why::Uses` for pcurves that are not equal and running on, or a use of a chain edge
outside a whole run of the chain) go pair by pair, in the order of the vertices, exactly as
before, so the refusals are reported as before.

**Decisions, each with a case against.**

1. *The pair check is one function (`link`) used by the single join and the chain walk*, so a
   chain is exactly the pairs a single join would accept. For: no second set of rules. Against:
   the chain walk evaluates each link twice (walk, then plan), which is linear and small next to
   the loops.
2. *A loop's run must cover the whole chain* (forward in order, reversed in the other), else
   the chain falls back to the pair path. For: it cannot accept what the pair path would refuse
   (a use pair of two edges but not the third). Against: a loop that legitimately holds part of
   a chain (not possible at a free vertex, where the only two edges are the loop's way through)
   would be refused the fast path.
3. *Survivor is the lowest id of the chain*, as the single joins leave it (each takes the
   lower); accuracy is folded in chain order, the interval by the same `run_on` from the left.
   Against: the fold order differs from the vertex order the pair path takes, so a tie in
   accuracy between unequal records could pick another record; no test finds one.
4. *Reports*: one `EdgeJoin` per vertex of the chain, naming the edge on the far side from the
   survivor, in chain order; the single joins name the pair's higher id in vertex order, so
   `absorbed` of a middle pair may differ while the set of absorbed edges is the same.

**Tests** (`tests/heal.rs`, one new, 15 in all): `joining_a_chain_at_once_equals_the_single_joins`
joins by `join_edges` and by `join_edges_at` at every vertex in order, on four lattices (with
and without the protected line) and five cylinders, and requires the same number of joins and
the same dump of every face, edge (geometry and fins) and vertex record. All 14 earlier tests
pass unchanged.

**Measured** (a 24-core box, unoptimised, whole test including build and check; a throwaway
counter of loops rebuilt and their uses, not committed): the 4,102-face bar 6.2 s to 0.36 s (4.2 s
on a quieter run before), loops rebuilt 8,188 to 12, uses copied 7,375,455 to 13,383; the 20 x
20 x 20 lattice's `heal` 0.30 s to 0.25 s, loops 456 to 24, uses 19,380 to 1,236.

**Open.** The cylinder wall's face merge stays pairwise across the seam (not a natural part of
the chain; a seam group needs the uv-window shift of 13.14 (b)). A ring of edges (a circle in
pieces) still joins pair by pair, quadratic in its pieces.

### 13.21 MK7b laws as built (11-28)

The first part of MK7b: the sweep's laws over MK7a's frames, everything that places the
profile at a station, and a typed entry for the walls. The walls wait on step 5's procedural
sweep kind (`Surface` has none). Code in `scx-make/src/sweep/{mod,rail}.rs`; tests in
`tests/sweep.rs` (19).

- `placements(&Path, normal: UnitVec3, &PathFrame, &[Stop], &Document) -> Result<Vec<Placement>,
  MakeError>`: a `Placement { from, at, m }` takes the profile's point `q`, drawn at the path's
  start, to `at + m (q - from)`. The shape laws are the pin's `shape(v) = k Rot(n, twist u) v`,
  `k = 1 + (end_scale - 1) u`, with `n` the profile's normal turned to point along `t0`. The
  orientation follows them: `Perpendicular` uses the frame's turn, `Parallel` uses none, and
  `Direction` uses `[t, r, t x r] [t0, r0, t0 x r0]^T`. At a `Stop::Corner` the columns are
  stretched by `Corner::stretch` along `to - from`, except for a parallel profile. With rails
  the map is the pin's `rail_map`. One rail gives `d` from the path to the rail, square to `t`.
  Two rails give `d` from the first rail to the second. The profile turns `d0` into `d`, sized
  by `|d| / |d0|`, evenly (`RailFit::Scale`) or only across (`Stretch`), and `at` puts the
  profile's point on the (first) rail onto it.
- `fractions(&Path, &PathFrame, &Document)`: SolidCraft's skin rule. It takes one station per
  degree of `|twist|` plus the tangent's turn (`Path::tangent_turn`, `a phi / sqrt(a^2 + b^2)`
  per helix) plus the corners' angles, at least a rail's control points less one, and clamps
  the count to `[MIN_SKIN_STATIONS, MAX_SKIN_STATIONS]` = `[16, 2048]`. The fractions are even
  in arc length with every joint included exactly. The rule is carried over, not derived; the
  kind's own bound replaces it.
- `sweep(&Region, &Frame, &Path, &PathFrame, &Document) -> Result<Made<SweepFaces>, MakeError>`
  checks the resolution, the laws and every station the skin would use, then answers
  `Unsupported::NeedsKind { kind: Needed::ProceduralSweep }`. The kind plugs in at the comment
  at the end of `sweep`, taking the region's segments and the placements; `SweepFaces { start,
  end, walls }` is the record it fills.
- A rail crosses a station's plane where the Bernstein coefficients of `(X - p W) . t` have a
  root. Each span is rational Bezier: one for a line, a quadratic per quarter for an arc, and
  the knots inserted to the degree for a clamped NURBS curve. The roots come from
  `scx_algo::bernstein_roots`, and the coefficients are zeroed within
  `gamma_5 (1 + gamma_6) size`. A rail's own end within the resolution reaches the plane. A
  root at a knot is one root, and the crossing must change sign over the rail. Anything else
  is `RailFault::{Short, Touches, Twice, InPlane, TooNear}`.
- Errors added: `Refusal::Path(FrameError)` with `From<FrameError> for MakeError`,
  `Refusal::Law(LawFault)`, `Unsupported::NeedsKind { kind: Needed }` and
  `Unsupported::RailsAtCorner`. `frames.rs` gains two crate-private readers (`joints`,
  `tangent_turn`); nothing else in it changed.

Decisions and cases:
- **`PathFrame` is an enum, `Shaped(Shape) | Railed { rails: Rails, fit }`, and `Rails` is
  `One | Two`.** 75 wrote a struct with `rails: Vec<Rail>` beside twist and scale. For: the
  pin's refusals "rails set the twist and scale" and "more than two rails" cannot be written.
  Against: a caller with a twist and rails must choose, where the pin's error told them why.
  The pin refused that pair anyway, so nothing is lost.
- **The least angle lives in its orientation.** `Parallel { least_crossing }` and
  `Direction { along, least_sine }` replace the pin's always-present fields that only one mode
  read (a field ignored unless a mode is set). For: no dead fields. Against: the caller writes
  SolidCraft's 5 degrees and `1e-3` themselves; the docs name them.
- **The caller's twist and scale limits are not ours.** The pin refused past
  `max_twist_turns` and `end_scale_within`, which are SolidCraft's UI policy. Only the geometric
  limits are refused here: twist finite and at most `MAX_TURNS` turns, scale finite and
  positive. For: one rule, no policy setter. Against: a caller that forgets its own limit gets
  a 1000-turn sweep. That is legal geometry, and the skin clamps at 2048 stations.
- **`Rail` is closed-form `Line | Arc | Nurbs(Arc<NurbsCurve<3>>)`, not any curve.** The pin
  took any `&Curve` and converted it to NURBS, refusing one with no NURBS form at run time.
  For: each variant splits exactly into rational Bezier spans, so the crossing is the roots
  of a polynomial, not a march, and a rail with no exact form cannot be passed. Against: an
  ellipse or other kind must come as NURBS. A helix has no exact NURBS form, so a helical rail
  is not possible yet (the pin refused it too). That is an open question.
- **The resolution replaces the pin's `1e-9 (1 + |P|)`** where a rail's end reaches a plane,
  as in MK7a for pieces meeting.
- **A placement is a self-contained affine map** (`from`, `at`, `m`), not the pin's
  `Station::translation(first)` that needed the path's first point beside it. For: the kind
  and the tests read one value. Against: `from` repeats in every placement (24 bytes).
- **Rails along a path with a corner are `Unsupported::RailsAtCorner`.** The pin did the same.
  A mitre's plane across a rail has no single crossing rule yet.
- **`sweep` validates before it answers Unsupported.** For: a caller gets its real refusal
  (a rail stopping short) now, not after step 5, and the station work is exercised. Against:
  it does work whose result it drops. That costs about 1 ms for 2049 stations (below).
- **`fractions` stays SolidCraft's uncertified count.** For: the skin is the kind's to bound.
  Deriving a count now would be a bound for a surface that does not exist. Against: nothing
  proves 1 degree is close enough. The rule is recorded as residue.

Idiom hunt: there are no setters, out-params or failing defaults. `Shape::PLAIN` is a value,
not a fallible default. The pin's `Laws` (a `Default` of nine fields, `rail_fit` ignored
without rails, `min_crossing` and `min_across` ignored outside their modes, `rails:
Vec<&Curve>` checked for length at run time) is gone.

Truths (`tests/sweep.rs`, 19 tests, closed forms, 1e-12 unless stated):
- On a straight path, twist and scale act linearly. `X` goes to `k_u (cos Tu, sin Tu, 0)`,
  `Z` is kept, a square's corner is turned and scaled, and a circle's radius is `10 k`
  (pin :138).
- A twist alone keeps the map orthonormal and the corner's angle at `pi/2 u`. This is the
  app's twisted prism's section; its volume waits for the walls.
- A diverging line rail scales by `(d0 + s x) / d0`, evenly or across (pin :238). An arc rail
  scales by `sqrt(R^2 - x^2) / R` (pin :268).
- **A rail is followed exactly** (new). Along a quarter arc, concentric arc rails of radii 13
  and 8 are crossed at `r (cos phi, sin phi)`, one or both. A line rail that rises out of the
  path's plane is crossed at its point on the station's plane, for both fits.
- Rails that stop short, touch, cross twice or lie in the plane are refused, and so are rails
  that meet the path. A polyline's vertex on a plane is one crossing, at a scale of 5/3. A
  cubic `x - 4 = -4 (1 - 2s)^2 (1 - s/2)` touches the plane `x = 4` and crosses `x = 3` twice
  (pin :291, :364, :387, :407).
- Two rails span the profile with `k = (4 + s x) / 4`, and the origin goes to `(x, 2 - 2k, 0)`
  (pin :420). Rails at a corner are `Unsupported` (pin :455).
- The parallel orientation is the identity map. The crossing is refused at 90 degrees, and at
  84 degrees it passes with 5 and fails with 7 (pin :627). The direction orientation along a
  helix takes `t0` to `t` and keeps the direction across, and a path along the direction is
  refused (pin :665).
- A profile normal against the path gives equal placements (pin :692).
- A right-angle corner's mitre is turned to the bisector and stretched by `sqrt 2` along
  `b`, with the scale at the corner's fraction. This is the stretch half MK7a left.
- The refusals (pin :706) are twist (infinite, NaN, 1001 turns), end scale (0, -1, NaN,
  infinite), least crossing and least sine out of range, a bad rail shape, and the frames'
  refusals coming through as `path`.
- `fractions` (pin :309): a quarter arc twisted a quarter turn gives 181 values; a line gives
  17; a helix with `a = 3, b = 4` gives 217 (216 degrees); a joint is included exactly; a rail
  of 40 points asks for 39 steps.
- `sweep` refuses first and otherwise answers `NeedsKind`.

Removed on purpose, as listed in the test file's header:
- From `out_of_range_paths_laws_and_fractions_are_refused`, the "more than two rails" and
  "rails with a twist" cases. They proved the refusal, and the type now forbids both.
- `the_caller_s_twist_and_scale_limits_refuse_past_them`. It proved SolidCraft's policy
  limits, which are now the caller's.

Remaining for the later MK7b part, once step 5 adds the kind:
- the walls and caps on the procedural sweep kind and their `SweepFaces` record;
- the volume truths: a twisted square prism keeps `area x length`, and an end scale gives the
  frustum's `h pi r^2 (1 + k + k^2) / 3`;
- the certified station count from the kind's bound, replacing `fractions`;
- a mitre's rail crossing (corners with rails);
- helical rails.

Cost: new code only, and no existing path changes. In a release build, 2049 placements along
a 20-turn helix with twist and scale take 0.39 ms (about 190 ns a station). Two line rails
take 0.82 ms (about 400 ns: two crossings and a frame). `fractions` takes 3 us. Measured
with a throwaway timer that was not committed.
