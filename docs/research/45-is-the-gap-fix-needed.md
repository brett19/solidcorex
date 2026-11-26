# 45. Is the per-corner gap fix needed in the tree build?

Scratch branch `cache-nogapfix-a` (local, plus the server ref `push/cache-nogapfix-a`, never
pushed upstream). Base: `cache-leaf3-b` (53a7f14856 plus the scratch log). The switch
`ZZ_NOGAP` is read once per thread into a thread-local. It acts only on the face the tree
build (`BrepCacheFace::create_uv_trim_curves`) passes in, and only in loop 2 of
`create_uv_trim_curves_body`. All other callers of the routine always run main.

Modes:
- 0: main (logging only).
- 1: skip step 2 (no per-corner fix and no vertex raise).
- 2: variant (i), no end moves but an honest vertex raise.
- 3: variant (ii), fix only where an end lies outside the vertex tolerance.
- 4: variant (iii), the tree implants a fixed copy and the body is put back.

The goldens are all 283 units, run forwards with `--profile goldens` on server2.

## Answer

**Not needed by the tree, and no consumer is found that needs it.**

- READ: no consumer of closed UV corners was found.
  - The implant uses vertex-use balls and leaves of at least uv_size/20, so it absorbs
    sub-leaf gaps.
  - Containment closes an open loop with a chord.
  - Mass gap closure cones the gaps itself.
  - The loop-closure validity test reads the 3D edge curves.
- MEASURED: step 2 never closes the gaps that could matter. 1,306 of its 1,309 trim moves in
  tree builds close a gap already inside the vertex tolerance. Every gap over dom/20 (2,760
  corners) is left open by its own cutoff.
- Skipping step 2 in the tree build (mode 1):
  - 54 golden rows change in 7 of 283 units;
  - no unit errors, and no rows are added, removed or broken;
  - the solidcore-ops results are identical to main.
- Every changed row traces to the **tree build writing to the body**: trims moved inside
  tolerance, or the vertex tolerance raised to 2x gap, which a later answer then reads. None
  traces to the tree needing a closed loop.
- Mode 1 makes a full-trim tree build write nothing to the body (0 of 27,067 builds, against
  393 in main).

## READ

- **Upstream's reason** (SmFace.cpp 7570-7899, 8416-8567): "make sure every loop->Corner
  UVGap is small by adjusting UVTrimCurve UVControlPoint positions", and "tighten UVGaps
  after Edges have been dropped to Surfaces". The comment admits that moving end points can
  kink a curve. Where a corner lies outside the 3D tolerance it only WARNs; it does not heal.
  No consumer that needs closed UV loops is named.
- **The fix's own window.** `fix_uv_trim_curves` returns early when the gap is below
  EFF_ZERO/10 or above min_domain/20. A large gap is never closed: it is the very case a
  closed-loop consumer would need.
- **Tree build/implant.** The tree implants the edge-use trims (`clone_exact`) and the vertex
  uses (sized by vertex tolerance over surface speed). It then floods containment with body
  `point_classify`. The smallest implant leaf is uv_size/20, the same order as the fix's
  cutoff. Nothing in the implant walks a loop for closure. The vertex-use ball and the leaf
  size already absorb a sub-leaf corner gap.
- **Point containment.** The classify falls back to the vertex ball and then to the loop
  walk. The walk closes a non-closed loop with a chord (query/loop.rs 745-781). An open UV
  corner is tolerated by construction.
- **Mass properties.** `gap_closure` closes the UV gaps itself, with cones onto the vertex. It
  does not rely on the fix.
- **Validity.** The loop closure check (`tlu::is_closed_3d`, assert_valid/loops.rs check 5)
  reads the ends of the 3D edge curves, not the trims. Trims do not enter the loop closure
  test.
- **The bracket.** It saves PreRescue, clears the surface owner, runs
  begin/end_surface_owner_cleared, restores the owner and calls
  `sync_face_surface_tolerance`. It wraps the whole routine, not only step 2. Loop 1
  (creating a missing trim, with its own edge and vertex tolerance raises) runs in every
  mode and still writes to the body.

## MEASURED: corners the fix acts on in the tree build (mode 0)

Every corner of a face the tree build visits was logged with the following:
- the UV gap;
- the 3D images of both trim ends against the vertex (d0, d1), before and after;
- the vertex tolerance before and after;
- the two edge tolerances before and after.

| corner class (tree build, 283 units forwards) | count |
|---|---|
| lines logged | 4,200 |
| candidates (not at a pole) | 4,069 |
| fix moved the trims | 1,309 |
| . small gap, fixed, ends inside vtol | 1,306 |
| . small gap, fixed, ends **outside** vtol | 3 (heal.face_props f0 v1: 0.266 vs 1e-5; offset.offset_geometry_creation: 0.249 and 0.168 vs 2e-5; bodies broken on purpose) |
| gap > dom/20, not fixed, ends inside vtol | 2,449 (mostly single-edge loops eu==cw on periodic faces, spanning the 2π period; 3D gap ~1e-15) |
| gap > dom/20, not fixed, ends outside vtol | 311 |
| ends still outside vtol after the fix | 310 |
| vertex tolerance raised by step 2 | 167 corners (131 at zero UV gap, no trim moved) |
| edge tolerance raised | 22 |

- Fixed corners, end distance / vtol:
  - median 0.0048 (UV gap 7.7e-9);
  - q90 0.479 (5.7e-5);
  - q99 0.626 (0.037);
  - max 2.66e4.
- 1,306 of the 1,309 moves close a gap already inside the vertex tolerance.
- Tree corners outside vtol occur in 5 units only:
  - heal.heal_data_p3: 224;
  - heal.face_props: 64;
  - intersect.curve_class_p4: 12;
  - offset.offset_geometry_creation: 12;
  - intersect.curve_class_p1: 2.
- The vertex raises (167) fall in these units:
  - topology.assert_valid: 61;
  - topology.vertex: 44;
  - heal.stitch_p1: 24;
  - heal.stitch_p2: 17;
  - heal.heal_data_p3: 12;
  - offset.offset_geometry_creation: 4;
  - offset.offset_executive_p2: 2;
  - sweep.api_brep: 2;
  - topology.brep_validation: 1.
- In assert_valid, the vertex was moved by 1.0 on purpose (mutation 6). The validator's own
  tree build raises the vertex tolerance from 1e-5 to 2.0.
- Non-tree callers ran 2,987 candidates and fixed 2,890 (outside the switch).

## MEASURED: body writes per tree build (TB snapshot before/after each build)

| mode | full trims, body unchanged | full trims, body changed | missing trims (always changed) |
|---|---|---|---|
| 0 main | 26,628 | 393 (271 trim, 111 vtol, 10 vtol+trim) | 4,407 |
| 1 skip | 27,067 | **0** | 4,407 (new trims; 1,054 + 16 also tol+vtol from loop 1) |

| 2 variant (i) | 26,872 | 149 (vtol) | 4,407 (154 also vtol) |
| 3 variant (ii) | 26,911 | 110 (108 vtol, 2 trim) | 4,407 |
| 4 variant (iii) | 13,605 | 9,928 (trim) | 4,310 (+470 trim) |

Mode 4's restore is not faithful (see variant iii).

## MEASURED: goldens, 283 units forwards

| mode | units passing | changed rows | units changed |
|---|---|---|---|
| 0 main | 283/283 | 0 | 0 |
| 1 skip step 2 | 276 | 54 (0 added, removed or broken) | feature_executive 2, fillet_executive_p3 4, heal_data_p3 5, stitch_p1 6, stitch_p2 7, assert_valid 29, brep_validation 1 |
| 2 variant (i) | 277 | 49 | feature_executive 2, fillet_executive_p3 5, heal.face_props 22, stitch_p1 9, stitch_p2 9, assert_valid 2 |
| 3 variant (ii) | 278 | 22 | feature_executive 2, fillet_executive_p3 4, heal_data_p3 3, stitch_p1 6, stitch_p2 7 |
| 4 variant (iii) | 260 | 1,385 changed, 115 broken | 23 units (prototype defect; see below) |

solidcore-ops tests, one thread per binary:
- MEASURED for mode 1 vs mode 0: the same pass/fail counts per binary.
- Both have one failure, `hlr::tests::a_box_with_rounded_corners_is_drawn_with_its_ball_corners_as_spheres`,
  with the identical numbers 4803.159770066 vs 4803.159771426. The failure is already on
  the base and is not caused by the switch.

### Mode 1 rows by class (54 changed rows in 7 units, 0 added, 0 removed, 0 broken; main is 283/283)

- **(a) body state only**
  - heal.stitch_p1 (6 rows) and heal.stitch_p2 (7 rows), the cov_cube4/5 and gap-at-edge-tol
    cases. The stitched result's output vertex tolerances are smaller without step 2: for
    example 3.786e-5 -> 2.893e-5, 3.371e-5 -> 2.940e-5, and 1.012e-5 -> 1e-5. Main's larger
    values are the tree build's vertex raise, copied into the result.
  - blend.feature_executive single_face_fillet1019_8 (1 row): base_geometry differs at
    about 1e-8 relative (moved trim control points).
- **(b) an answer differs**
  - blend.fillet_executive_p3 (4 rows: stubs4-delete-edge-sphere_diff_3cylinders-e1/e4,
    r0.1/r0.5). Mass properties: the volume moves 38227.381745950406 -> 38227.381745877836
    (1.9e-12 relative). The near-zero products of inertia move at 1e-14 and 1e-8. This is
    integration over trims moved inside tolerance, so it is noise level. Neither side is
    more right.
  - blend.feature_executive single_face_fillet1019_6 (1 row). The fillet result's base
    B-rep differs:
    - base_graph ints differ at 88 of 2,052 (an entity order or index change);
    - base_record at 219 of 1,344;
    - base_geometry at 38 of 502, with edge tolerances 2.918e-4 -> 2.918e-4 (+5e-14) and
      4.687e-4 -> 4.687e-4 (+4e-11);
    - one vertex block differs (0 -> 1.33e-5 and a reordering);
    - the feature0 B-rep values differ at <1e-12 relative.

    Every count is unchanged. The fillet chose a different order, or tolerances, because
    the input trims differed inside tolerance. INFERRED: equivalent bodies; not decided
    which is "right".
  - heal.heal_data_p3 (5 rows: degen_props_sliver_w1e-05_* and degen_props_curved_sliver;
    the WR-57 rows). One dumped heal property flag moves from 1.0 to 0.0 (out[28] in
    curved_sliver). These are the rows whose vertex-face gap WR-57 certified as 0. The tree
    build in main moves trims or the vertex tolerance at the sliver's far corner (gap 1e-5
    against vtol 1e-5). INFERRED: with step 2 skipped, the heal property no longer sees
    the raised tolerance or the moved trim. The flag change follows from the gaps.
- **(b/c) validity reports**
  - topology.assert_valid: 29 changed rows out of 32 cases. The cases are box, cylinder,
    sphere and cone, mutation 6 (a vertex moved by 0.1 or 1), level-2 and update variants.
  - topology.brep_validation: 1 row (box_mutation_6).
  - Main's reports include "Vertex tolerance is too wide to excuse by clearance (offset
    0.2, tolerance 0.02)". There, the check's own tree build has already raised the moved
    vertex's tolerance to 2 x gap.
  - Mode 1 instead reports the missing edge/vertex gap entries, for example "start
    edge/vertex gap must be less than edge + vertex tolerance", and "Bad Edge fails
    ValidateGeometry".
  - The valid/invalid verdict is unchanged (invalid in both).
  - **Mode 1 is the more right side.** A validity check should not widen the tolerance of
    the defect it is checking.
- **(c) failures/refusals appearing or vanishing:** none. No unit errors or panics in mode 1.

### Are the (b) rows confined to corners whose gap exceeds the vertex tolerance?

**No** (MEASURED).
- Among mode 1's changed units, only heal.heal_data_p3 has tree corners outside the vertex
  tolerance.
- blend.feature_executive (58 fixes) and blend.fillet_executive_p3 (178 fixes) have none.
  Their rows come from trim moves inside tolerance.
- stitch_p1/p2, assert_valid and brep_validation come from the vertex raise, mostly at
  zero UV gap.

## Variants (MEASURED)

**(i) No end moves; raise the vertex tolerance honestly** to 2 x max(end distance, gap).
- 49 rows in 6 units, and the build still writes to the body (149 full builds).
- heal_data_p3 and brep_validation go back to main. In exchange, the following change:
  - **heal.face_props, 22 rows (class b).** On the bodies broken on purpose (ends 0.17-0.27
    off a 1e-5 vertex), the honest raise to about 0.5 changes the healer's problem codes
    and counts: 1000 -> 1040, a count of 6 -> 4, and several flags. Main's answer
    reflects the defect; the honest raise hides it.
  - **assert_valid, 2 rows (b/c).** In update_patch_sheet_m6_i0_v1_l2, the vertex
    tolerance becomes 2.0000000000166 instead of 2.0, and the one update report vanishes
    (violated_or_updated 1 -> 0).
  - stitch_p1 and stitch_p2 (9 + 9 rows, class a): the output vertex tolerances are
    larger, for example 2.000e-5 -> 2.198e-5.
  - fillet_executive_p3 (5 rows): integration noise as in mode 1, plus one result edge
    tolerance 1e-5 -> 1.94e-5 (bottle-e8).
  - feature_executive: 2 rows, as in mode 1.
- Worse than mode 1: it raises tolerances further and hides defects.

**(ii) Fix only where an end lies outside the vertex tolerance.**
- 22 rows in 5 units:
  - feature_executive 2, fillet_executive_p3 4 and heal_data_p3 3: the same kind of rows
    as in mode 1;
  - stitch_p1/p2 6 + 7: the same rows as mode 1.
- assert_valid and brep_validation match main, because the raise still fires at the moved
  vertex.
- It still writes to the body at 110 full builds (108 vtol raises, 2 trim moves).
- It removes 1,306 inside-tolerance moves. It keeps the vertex raise that masks the
  mutation-6 defect in the validator.

**(iii) Fix only the tree's copy at implant time.**
- The prototype runs the main routine, captures the trims and tolerances into a
  thread-local override that the tree reads, then writes the saved fields back.
- **The restore is not faithful.** The TB snapshot shows 9,928 full builds that still changed
  trims. The likely causes are holder-aware trims (an edge use whose trim is held by its
  mate) and a restore through `edgeuse_mut` that does not reproduce the original holder or
  stamp.
- The result was 1,385 changed rows and 115 broken in 23 units. These measure the
  prototype's defect, not the idea.
- NOT ESTABLISHED. A faithful (iii) needs a tree-local trim table instead of a write-back.
  Modes (i) and (ii) already show that the tree does not need the fixed copy.

**Step 5 (a class (a)-only variant):** none of the variants is class (a) only. All of them
keep the fillet integration-noise rows and the feature_executive rows. So the
"0 body diff" check was not run as a gate. Mode 1's TB table above already shows 0 body
writes for full-trim builds.

## The bracket, if step 2 leaves the tree build (READ + MEASURED)

- **Owner clear and owner rescue:** still needed. Loop 1 runs in every mode. In all 4,407
  builds with a missing trim it creates trims, and in 1,070 of them it raises edge and
  vertex tolerances.
- **`sync_face_surface_tolerance`:** still needed for the same loop 1 raises.
- **For a face whose trims all exist:** mode 1's tree build writes nothing (MEASURED,
  27,067 of 27,067). INFERRED: the bracket there is a no-op and could be skipped when
  loop 1 has nothing to create. The editing flag was not measured separately.
- The tree build still writes to the body whenever a trim is missing. That is loop 1's
  business, not step 2's.


## Recommendation

**Remove step 2 from the tree build's call** (mode 1). Keep it for the explicit callers
(heal, make, the operations) unchanged.

Why:
- The tree does not need step 2 (READ, and MEASURED: no failures or refusals appear or vanish).
- 1,306 of its 1,309 moves are cosmetic.
- Its vertex raise makes a validity check widen the tolerance of the defect it is checking.
- It makes a read-only query write to the body at 393 builds.

What moves in published results: **54 golden rows in 7 units**, each needing a derivation.
- **assert_valid 29 + brep_validation 1 (30):** the reports now name the real edge/vertex
  gaps of the mutation-6 vertex move, not the excused, widened tolerance. Mode 1 is the
  right side. The verdict is unchanged.
- **stitch_p1 6 + stitch_p2 7 (13), class (a):** the stitched output vertex tolerances are
  smaller (no tree-side 2x raise). The smaller values are the right ones.
- **heal_data_p3 (5), the WR-57 sliver rows:** one heal property flag 1 -> 0. The WR-57
  certificate test already says these vertex-face gaps are 0. INFERRED: mode 1 is right;
  this needs confirming against that test before filing.
- **fillet_executive_p3 (4):** mass properties moved at about 2e-12 relative. Integration
  noise; neither side is better.
- **feature_executive (2):** fillet1019_8 differs at 1e-8 relative. fillet1019_6's base
  B-rep has the same counts but differs in entity order, plus a vertex block. INFERRED:
  equivalent bodies. A soundness probe (`soundness_probe::moved_cases_results_measured`)
  should confirm both sides are sound before filing.

Variant (ii) would cut the moved rows to 22. It still writes to the body and keeps the
validator masking, so it is not recommended. Variant (i) is worse than both.
Keeping step 2 in the tree build is defensible only to avoid the 54-row derivation
cost. It buys nothing the tree uses.
