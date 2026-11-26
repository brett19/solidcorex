# 28 - How often is a UV trim acceptable without a raise? (D2 reach, measured)

Local measurement, nothing pushed. Settles report 27's "first step 3" and its UNVERIFIED items. Base: branch
`zz-trim-raise-chk` at d981bb2c37 plus probe commits (local a local worktree, branch
`zz-trim-reach`). Counts are exact; timings are the min of 3 (one run when the first exceeds 0.3 s). Server runs
on the build server, goldens profile. Raw logs and tally script: the raw data (kept outside this repository)
(`A.log` bounds+timing, `B.log` bounds+curve-dependence, `C.log` bounds+item 4; `tally.py`; `A.md` = every
per-body table).

## 0. Answer

Criterion: certified `B <= tol_edge + tol_face` evaluated BEFORE any raise, with `B` from the kernel's own
bound (`trim_deviation_bound` / `trim_image_gap_bound_within`), `tol_edge` read at entry to the build,
`tol_face` the trim's own face.

| body class (variant) | trims built | certified B <= lim | certified B > lim | NO certified bound | raised by today's code |
|---|---|---|---|---|---|
| kernel-made, primitives, trims made during construction | 19 | 14 (73.7%) | 0 | 5 | 0 |
| kernel-made, every trim dropped and re-made (strip + create_missing) | 42 | 18 (42.9%) | 0 | 24 | 0 |
| after an operation, trims made inside the operation | 1,336 | 539 (40.3%) | 0 | 797 | 0 |
| after an operation, every trim dropped and re-made | 1,844 | 536 (29.1%) | 0 | 1,308 | 4 |
| after an operation, trims missing as built (create_missing) | 290 | 33 (11.4%) | 0 | 257 | 0 |
| imported corpus, plain read then create_missing | 613 | 608 (99.2%) | 2 | 3 | 161 |
| imported corpus, trims made inside the read | 8 | 4 | 0 | 4 | 0 |
| record round trip of imports, every trim dropped and re-made | 708 | 424 (59.9%) | 0 | 284 | 59 |
| record round trip of operation results, export (missing trims) | 290 | 33 (11.4%) | 0 | 257 | 0 |

Reading it:
- **A certified bound failing is rare**: 2 trims of 6,100+ (below), both the same edge in `blade_hub_1` and its twin
  `cliff_1`, ratio B/lim = 1.011.
- **The big hole is "no certified bound"**, not "bound too large". Every no-bound trim is one of: a LINE trim on a
  plane (3,880 trims: Plane/Line edge/Line trim), a Plane/BSpline-edge/Line-trim pair (124), or a
  ProceduralBlend face (14). `TrimPair::of` (`query/curve_surface_gap.rs:3316`) returns `None` for a trim whose own
  evaluation carries rounding (`w_slack != 0`, a Line trim), so the kernel computes no `B` there today (today's
  code takes upstream's sampled raise on those, rule `no_raise`/`U_*`). All of these have a sampled whole
  distance <= lim (1,308 of 1,308 after-op; 24 of 24 kernel-made; 3 of 3 imported), mostly 1e-14 to 1e-12
  (an exact drop), but a sample proves nothing (report 27 section 3.4). So D2 as written would REFUSE or need a new
  exact bound for planar line trims; that is most trims of box-like bodies.
- Excluding the no-bound trims, certified acceptance is 100% for kernel-made and after-op bodies and 99.7% for imports
  (608 of 610).
- **Today's code raises where D2 would not need to**: of the 161 imported trims whose edge or vertex tolerance was
  raised in create_missing, 159 satisfy B <= lim before the raise (and 4 of the 4 raised op trims). Edge
  tolerances in the corpus are mostly already large (e.g. vase2 edge 13: 1.04e-4 against a face of 1e-5).
- Fresh kernel bodies against the kernel default alone (B <= 2 x default tolerance): identical counts to the
  table for K and O (the edges and faces sit at the default); imports 80.6% (494 of 613), because many import edges
  carry a tolerance above the default. So fresh kernel bodies pass as often as above; imports pass only thanks to
  their own larger tolerances.
- The imported set is 28 files but only 12 build any trim after the read (the rest carry file trims). blade_hub_1/
  cliff_1 and blade_hub_2/cliff_2 are the same body (identical numbers), so import counts double-count them.
  The first `check_brep` rows in `A.md` are exactly 2x the create_missing rows (the check builds each trim twice).

### Per surface / curve kind (certified bound present; `A.md` has the full tables)

| surface / edge curve / trim | trims | no certified bound | note |
|---|---|---|---|
| Plane / Line / Line | 3,880 | 3,880 | every one |
| Plane / BSpline / Line | 124 | 124 | every one |
| ProceduralBlend / BSpline / BSpline | 14 | 14 | every one |
| Plane / BSpline / BSpline | 175 | 0 | 67 certified-pass shortfall is the Line-trim rows above, not this row |
| Cone, Sphere, Torus, SurfOfRevolution, BSpline, Offset (any edge curve) | 2,500+ | 0 | all certified; 2 fail (SurfOfRevolution/BSpline, ratio 1.011) |

## 1. Instrumentation

All in `solidcore/crates/solidcore/src`:
- `zz_reach.rs` (new): the per-trim measurement (`begin`, `bounds`, `end`, flags, event line).
- `topology/edgeuse/p3.rs` `get_or_create_uv_trim_curve` (about :1766-1874): `begin` before `create_uv_trim_curve`
  (records edge/face/vertex tolerances at entry, kinds, a body copy for item 3; passes a local `DropCurveFail` to read
  `create_path_type`), `bounds` after the build and before the raise, `end` after `set_uv_trim_curve`.
- Flags: `finish` (:1201, the 1.01x raise), `trim_other_uv_curves` (:1242, re-trims), the two `set_interval` sites
  (:1033, :1057), `brute_force` (:1531), `pick_seam_curve` call (:876), the four `delete_uv_trim_curve` cleanup sites
  (duplicate from reentrant creation). The earlier `zz_raise.rs` hooks (distance, whole distance) are kept.
- Bound calls (inside `bounds`, hooks suspended): `trim_deviation_bound(..., [0.5*tol, inf])` = today's call = `B_cert`
  (timed); `[lim, lim]` = the certified within-test (timed); `[2*default, 2*default]` for the default-tolerance test.
- Test file: `solidcore/crates/solidcore-convert/tests/zz_reach.rs` (corpus: the 28 files of
  `heal_edge_tolerance_covers_edge_face_gap.rs`; 5 primitives; 47 Boolean scenario/op pairs copied from
  `zz_boolean_coupling_probe.rs`, covering curved faces, tangent, near-coincident (1e-6, 1e-4 overlap), scale 1e-3/1e-5;
  rounds (constant, variable, chain of 3 edges); offset out (box, cylinder) and in (rounded slab); shell (box, cylinder);
  linear sweep with seam; two revolves; record round trip through `solidcore_convert::brep::{to_record,to_kernel}`).
  `zz_sync_{brep,curve,surface}.rs` for item 6.
- Stages: `build`/`read` (inside the construction), `asis_*` (body as built), `strip_*` (every trim removed, then re-made),
  `plain_*` (corpus), `rt_*` (round trip), `*_cm` = `create_missing_uv_trim_curves`, `*_check` = first `check_brep` Level2.
- Every event line carries: tol_e0 (entry), tol_f, tol_e_after, sampled dist/dev/whole, `B_cert`, `B_within`,
  `lim`, raise applied, rule of `trim_raise_distance`, create path, flags.

## 2. Creation paths (item 2)

Totals over all stages that build trims (A.md "Creation paths" has every body):
- **Interval recovery (`set_interval` + re-trim of other trims): 0** in 13,239 builds.
- **Brute force 101 points: 0.** **Reentrant seam creation (nested build) and duplicate-trim cleanup: 0.**
- **Loosened-fit 1.01x raise (`ii > 1`)**: 4 builds in the first check and 2 in create_missing (the same 2 trims,
  DropAndTrimCurveOkay), all imports (vase3 edges 24 and 34: 6.7e-5 -> raise to 6.73e-5 inside the creation). 0 elsewhere.
- Seam pick (`PickOneSeamRecovery`): kernel-made 10 of 19 at construction (cylinder, cone, sphere, torus), 100 of 1,844 after-op, 4 of
  613 imports. `DropCurveOkay` is the common import path (597 of 613); `Okay` (= the pcurve already matched) is the
  common kernel path.
So the recovery paths report 27 worried about almost never run on this set; the one escape is the fit-loosening raise on 2 trims.

## 3. Does the trim curve depend on the tolerance it was fitted under? (item 3)

Each build repeated on copies: at the body's current tolerances (A) and after setting the edge and its vertices to
today's post-build values (B). Compared by the curve's `Debug` text (shortest-round-trip floats: bitwise) and its
interval bits, plus the edge interval after.
- A equals the real build in every case (`a_vs_real=same`, 13,239 of 13,239).
- Where today's code changed a tolerance (tolchanged=1) the curve DIFFERED in: imports 33 of 161 (create_missing; 66 of
  322 in the check, the doubled builds), after-op (stripped) 2 of 4. Never differs where no tolerance moved.
  The differing builds are all `DropCurveOkay` on edges whose other trims had raised the edge first (rules
  `B_lowered` 33, `U_covers_bound` 2; the twins hub_1/cliff_1 and hub_2/cliff_2 are included in the list below): blade_hub_1 5+1, blade_hub_2 6, blade_hub_3 5, cliff_1 5+1,
  cliff_2 6, revolution_mold 4, ball_less_corner 2. The interval did not change (iv_moved 0 everywhere).
- So the curve is a function of tolerance (the fit target), as report 27 inferred; the dependence is real on about 20% of
  trims where a raise happens, 0% elsewhere. Under D2 (no raise) the curve is built at one tolerance, so it is order
  independent by construction; today's curves for those 35 trims are the ones that would move.

## 4. Does a missing raise change a read? (item 4)

Bodies after create_missing, tolerances of every edge and vertex restored to their pre-phase values (probe `restore`),
against today's body; 200 mixed points (60 random incl. outside, up to 70 on faces, up to 70 near edge chords at 1e-6,
1e-5, 1e-4 of size; `points` in the log is below 200 when fewer were placeable), tight box, volume and area
(accuracy 1e-7). `C.log` `ZZ4` lines. No body in the kernel-made or operation set changed anything.

| body | raised edges / vertices | classification | box | volume | area |
|---|---|---|---|---|---|
| blade_hub_1, cliff_1 | 34 / 53 | 0 of 200 differ | same | same (bits) | same (bits) |
| blade_hub_2, cliff_2 | 25 / 38 | 0 of 200 | same | same | same |
| blade_hub_3 | 34 / 55 | 0 of 200 | same | same | same |
| revolution_mold | 5 / 8 | 0 of 200 | same | same | same |
| **vase2** | 2 / 0 | 0 of 174 | same | **differs** 1104219.0602539964 vs 1104219.0602538523 (1.3e-13 rel) | **differs** 131878.3740725389 vs 131878.36447475338 (7.3e-8 rel) |
| **vase3** | 2 / 0 | 0 of 200 | same | **differs** 1103584.0777679742 vs ...679675 (6e-15 rel) | **differs** 127630.10825506294 vs 127630.10374442594 (3.5e-8 rel) |
| cylinder_slot (strip) | 1 / 2 | 0 of 158 | same | same | same |
| ball_less_corner (strip) | 2 / 3 | 0 of 118 | same | same | same |
| round_drilled (strip) | 1 / 1 | 0 of 162 | same | same | same |
| all other 22 corpus files and 100+ op/primitive bodies | 0 / 0 | not compared (nothing raised) | | | |

Cause of the only differences (vase2 edges 17 and 21, vase3 edges 24 and 34): today's raise covered a sampled
distance (1.13e-4, 6.66e-5) with 2x, taking the edge from 1.29e-4 to 2.26e-4 (and 7.3e-5 to 1.33e-4), although certified
B (1.129e-4, 6.66e-5) was within tol_e + tol_f (1.39e-4, 8.3e-5). Volume and area move at the 1e-8 level, which is the edge
tolerance entering the mass integrator's face boundary handling (INFERRED, not isolated). No classification
flipped on 8 body-states x 200 points; this covers `Plane` faces with BSpline edges next to offset surfaces only; points
within the pcurve deviation of a boundary are probably rare in a random set (not stressed).

Not measured for item 4: bodies where the raise happened INSIDE an operation (booleans, fillets): restoring needs
the operation re-run without raises (the probe only restores after the trim phase).

## 5. Cost (item 5)

`B_cert` is today's computation (`[0.5 tol, inf]`, a full bisection above half the tolerance); `within` is
`[lim, lim]`. Today's code computes `B` on only 204 of the 3,862 trims in the build, read and create_missing stages (rules `B_lowered` 189, `U_covers_bound` 15):
the rest have `tol >= 2 x upstream` and take rule `no_raise`.

| class | trims timed | sum B_cert | sum within | max B_cert | max within |
|---|---|---|---|---|---|
| imported (create_missing + check) | 1,847 | 22.0 s | 7.2 s | 0.134 s | 0.034 s |
| kernel-made | 145 | 0.96 s | 0.71 s | 0.075 s | 0.052 s |
| after op | 7,738 | 13.7 s | 9.8 s | 0.090 s | 0.070 s |
| round trip imports | 1,321 | 15.7 s | 5.6 s | 1.23 s | 0.124 s |

(All stages summed: check rows are double the create_missing rows.) For trims that have a bound (1,164 in the
create_missing stages) the totals are 10.83 s against 4.90 s: the within-test is 2.2x cheaper than the full bound, and
about 3x on the imports. Per body (create_missing only, trims with a bound): blade_hub_3 150 trims 1.79 s full / 0.59 s within;
blade_hub_1 and cliff_1 (120) 1.51 / 0.45; blade_hub_2 and cliff_2 (88) 1.03 / 0.36; revolution_mold 0.32 / 0.14; ball_drilled 0.55 / 0.40.
So a within-test on every trim of blade_hub_1 costs about 0.45 s (3.7 ms per trim), against about 1.2 s for report 27's
whole prepare. It would be paid on every trim, not only the 34 that raise today. The no-bound trims cost microseconds (nothing computed).

## 6. `Sync` (item 6)

`assert_sync::<T>()` and `assert_send::<T>()` compile, as three separate test targets, for `Brep`, `curve::curve::Curve` and
`surface::surface::Surface`: all three are `Send + Sync` today. The comment at `curve/curve/p5.rs:482` ("`Curve` is `Send`
and not `Sync`") is stale: `CurveMemo` holds `OnceLock<Arc<Mutex<..>>>`, which is `Sync`. A trim cell on `Brep` holding a
`Curve` stays `Sync`.

## 7. What this means for D2

1. For imports and for curved faces D2 verifies lazily with no raise in 99% of trims (608/613 imported, 100% of
   certified Cone/Sphere/Torus/BSpline/SurfOfRevolution trims). Report 27's guess that blade_hub-like imports would fail
   (B 3.9e-5 against tau about 1e-5 to 2e-5) is not what the corpus shows: those bodies' edges already carry 1e-5 to
   4.4e-4 tolerance, and the one failing edge is at ratio 1.011.
2. D2's real obstacle is **planar line trims**: 3,880 + 124 trims (about 40% of all builds, the majority of box-like
   op results) have no certified bound, so D2's check cannot pass them without a new exact bound (a line trim on a plane
   is exact analytically; the code returns `None` because the line's evaluation rounding is not bounded by `TrimPair::of`).
3. D2 moves results for 161 imported trims (their raises disappear) with no classification change, and a 1e-8 area/volume
   change on 2 of 28 files. It also changes 35 trim curves (item 3).
4. The creation recovery paths (interval recovery, brute force, reentrant seams) were not exercised by this set (0 of 13,239); only the 1.01x
   fit-loosening raise ran, on 2 import trims. Their correctness under D2 stays untested here.

## 8. Not measured / caveats

- Kernel-made bodies mostly have authored trims (the primitive builders set them); the "stripped" rows are a what-if
  (every trim treated as a cache), not today's behaviour.
- The record round trip: the 28 corpus files that import (`to_kernel`) succeed for 17 of them; 8 are refused ("the
  plane's axes are not square", blade_hub_1-3, bottle, cliff_1-2, open_shell, test1), 3 cannot export (vase1-3: a face
  surface with no NURBS form); 5 intersection results cannot export ("shell count"). The round trip carries pcurves, so
  no trim is built on import (`rt_import` and `rt_cm` rows are 0 or the 12 round-tripped ops).
- Item 4 restores only after the trim phase; trims raised inside an operation, the face routine's vertex raise
  `2*gap_3d`, and read paths other than classification/box/volume/area (rays, fillets on the result, `check_brep`
  report text) are not compared. The check route's results were not compared (item 4 phase is create_missing only).
- Certified B uses the kernel's own enclosure; `B_cert <= 0.5 tol` values may be loose upper bounds (the bisection
  stops there), fine for acceptance, not for a ratio. 0 within/cert disagreements in 13,239.
- Timings include only the bound calls; the clone per trim of the curve-dependence run is not timed (run B).
- Face tolerance is the face of the trim's edge use, not the maximum over the edge's faces.
- The probe suspends its own hooks during extra computations, but the extra `fit_tolerance` calls could cache an edge-use gap
  (not observed; the first-check and create_missing results agree with `zz_raise` results).

## 9. Scratch

The probe commits and logs are kept outside this repository.
