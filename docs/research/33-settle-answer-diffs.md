# 33. Answers that change when the body is settled first

**Question.** Report 30's clean run (6.0) compared two set-ups. M0 is main reading the raw body. S is the body settled first (`create_missing_uv_trim_curves`, then the face trim pass to a fixed point) and read through the lazy tree. They differ on:

- 50 ray answers, over 18 variant-states;
- 19 mass answers, over 8 variant-states;
- 4 validity-check reports.

Main reading the settled body (MS) gives the same answers as S, so settling is the cause. For each difference, this report asks what differs and which answer is right.

**Labels.** MEASURED means a probe printed it. READ means taken from code or an earlier log. INFERRED means a conclusion drawn from measured facts.

**Probe.** Scratch tests `zz_d_all` and `zz_d2_all` in `solidcore-convert/tests/zz_tree_value.rs`, on local branch `cache-settle-diffs-1`. They are built on the report-30 prototype, never pushed, and run serially. A thread-local hook dumps the solver's solution list for each ray in both `ray_fire` paths.

## Summary

Categories:
- (a) settled right, main-raw wrong;
- (b) main-raw right, settled wrong;
- (c) both within tolerance of the truth, or bit-level ties;
- (d) undetermined.

| Family | Differences | (a) | (b) | (c) | (d) | Worst case |
|---|---|---|---|---|---|---|
| Ray | 50 | 0 | 0 | 50 | 0 | open_shell stripped ray16: same face, hit t differs by 1.1e-12. S is 1.1e-12 from the independent root, main 5e-14. Tolerance is 1e-5. |
| Mass | 19 | 0 | 4 (one mechanism) | 15 | 0 | (b): RO:shared_face_overlap1e-6_union, face F6. Area 59.999994 (main, which is the analytic value) against 60.000000 (S): relative 1e-7. Body area error 1.8e-5 (main) against 2.4e-5 (S). |
| Check | 4 reports | 2 | 2 (warning level) | 0 | 0 | vase1 and vase3 stripped: the face pass writes 2D trims kinked at an interior knot (cos 0.9957 and 0.9885; the check's limit is 0.99985). |

Main is order-dependent on raw bodies (MEASURED): the same ray in a different read history gives a different answer, by up to 3.6e-14 in t. The settled body gives identical answers in all four read orders tried.

## 1. Rays: all 50 are last-bit ties (c)

**What differs (MEASURED, `an.py` over the M0 and S answers).** In all 50 differences, both sides agree on:
- point class (Face);
- the face hit;
- the start region.

What differs:
- `gap_3d` and `pre_snap_param` (the hit's ray parameter t), by 2e-16 to 7e-14 absolute;
- `uv_param`, by up to 2e-14;
- one exception, open_shell stripped ray16: Δt = 1.12e-12 and Δu = 9.6e-13.

The solver's full solution list has exactly one solution in both paths, every time: the same face, `num_variables = 2` (face interior), and the same t to 1e-9. So no hit is added or removed and none is duplicated; only the closest-hit selection is reported.

**Ground truth (MEASURED).** For each differing ray I intersected the ray with every face's surface independently:
- the surface's own `drop_point` gives a signed distance along the ray;
- that distance is sampled at 600 points, and every sign change is bisected 80 times.

In all 50 cases the first crossing on the reported face equals both answers to better than 1e-13:

| Case | Δ(main, truth) | Δ(S, truth) |
|---|---|---|
| blade_hub_1 ray10 | 5.2e-14 | 6.8e-14 |
| blade_hub_3 ray00 | 1.4e-14 | 0 |
| open_shell ray16 | 5.0e-14 | 1.1e-12 |

Which side is closer splits about evenly: S is closer 24 times, main 19 times, and they are equal 7 times. These are solver round-off, not a decision.

**Near an edge or vertex? (MEASURED)** No. The hit points' nearest edges are 0.052 away at the least (blade_hub_3 ray00); the next is 0.083 (torii_fillet ray09), and most are more than 0.5 away. Tolerances are at most 1.4e-4. No difference is near a boundary.

**Inside/outside classification (MEASURED, but not used).** I also classified each crossing by a 3D side test against the face's edge curves. Its per-face calibration agreed on only 6 of 12 sample points on several faces, so its inside/outside verdicts were not used. The verdict rests on two facts:
- the hit t matches the independent root;
- the nearest boundary is at least 5,000 tolerances away, so the trim cannot decide the hit.

**Why the bits differ (INFERRED).** The body does differ near these faces:
- Settling raises edge and vertex tolerances. For example, on blade_hub_1 face F19, edge E12 goes from 5.2e-5 to 1.068e-4.
- The loose face boxes grow with the tolerances: by 5.5e-5 on F19 and by 4e-5 on blade_hub_2 F29 (MEASURED).
- Main's own copy raises the same tolerances while it fires rays (MEASURED). On test1, edges E1 and E2 are 2.404e-5 on the raw body and 4.375e-5 on both main's copy and S.

The solver's start guess and candidate order depend on these boxes and on which trims exist, so Newton converges to a different last bit. The trims of the hit faces deviate from their edges by under 1e-12 on both sides (MEASURED, `trim_devs`), so no trim end moved at a hit.

**Order dependence (MEASURED) explains five of the differences outright.** For test1, case_int, ShellBrep_983, revolution2 and RO:cylinder_slot_diff, ray01 (ray02 for ShellBrep) gives a different answer depending on history:
- fired early on main's single copy, it gives the M0 answer;
- fired after the other rays (reversed order), it gives exactly the S answer.

The two answers are simply two of main's own possible outcomes.

## 2. Mass

Per-face reads use the loose face box (`query::face::p1::calculate_bounding_box(.., false)`; READ, `face_box` in `mass/integrator/properties.rs`), never the tight box.

Each comparison is value against value. Main's refusals on raw bodies (PREP) were counted as refusals in the clean run and are left out here.

Reference methods (MEASURED; each validated on a face of known area):
- **Planar faces:** Stokes' theorem over the 3D edge curves (no pcurves). Polylines of m = 500, 1000 and 2000 segments, then Richardson extrapolation.
- **Other faces:** Green's theorem in uv over edge points dropped onto the surface, with 8-point Gauss-Legendre in 6 panels and Richardson extrapolation.
- **Validation:** on blade_hub_1 F19 (planar), Green-Richardson gives 26.68161963776 against the kernel's 26.68161964169 (1.5e-10 relative).

The reference is only as good as the edges lie on their surface. The worst drop gap is reported with each case below.

| Case | Keys | Main-raw | S | Relative | Reference | Verdict and cause |
|---|---|---|---|---|---|---|
| blade_hub_1 / cliff_1 as_is | f19.cen, f34.cen | centroid | centroid, 1 ulp apart in one or two coordinates | ≤1.9e-16 | F19 Green: A 26.681619638, C (233.0349861056, −9.288249187, 231.1020201145). F34 (cone): Green A 570.17699, limited by a 1.9e-5 edge gap. | (c). Area is bit-identical on both sides; the centroid is unchanged at ACC 1e-7, 1e-9 and 1e-11. Cause: face-box origin moved by the tolerance raises (E12, E66, E63, E51, E23 on F19). |
| blade_hub_2 / cliff_2 as_is | f29.cen | as above | z differs by 1 ulp | 1.6e-16 | Green A 39.48569817 against the kernel's 39.48569820 | (c), same cause (E45, E48, E12, E23, E30 raised). |
| blade_hub_3 as_is | f45.cen | as above | 1 ulp | 1.7e-16 | Green A 88.99905730 against the kernel's 88.99905766 (gap 1.5e-13) | (c), same cause. |
| open_shell as_is | area, est, f0.area, f0.cen, f4.area, f4.cen | F0 A = 4433.313729157 | 4433.313700315 | 6.5e-9 | Stokes 4433.3135932; Green 4433.3141305. The two references disagree by 1.2e-7 relative. | (c). Both kernel values are within ACC 1e-7 of both references, which cannot decide between them. Cause (MEASURED): the face pass moved F0's trim on E3 to 9.48e-6 from its edge (raw: 3.2e-9); E3's tolerance is 1.43e-5. F4 differs by 5.6e-11 with trims equal to 4 digits. |
| RO:shared_face_overlap1e-6_union as_is | area, est, f6.area, f6.cen | F6 A = 59.999994, Cx = 14.9999995 | 60.000000, Cx = 14.999999 | 1e-7 | **Analytic: 59.999994 and Cx = 14.9999995; body area 839.999976.** | **(b)**: see below. |
| vase2 as_is | f8.area, f8.cen | A = 7722.021694419, Cx = −1.486745e-5 | A = 7722.021694179, Cx = −1.486791e-5 | 3.1e-11 on area; 3.1e-5 of a near-zero coordinate | Green A 7722.0021954. Weak reference: E20 and E22 lie 8.2e-4 off the offset surface, above their own tolerance of 7.45e-4, on both sides. | (c). See the vase2 note below. |

**vase2: not issue #21660.** This is a trim/tolerance effect, not the wrong-tight-box issue:
- the face box is bit-identical on both sides, and per-face reads never touch the tight box (MEASURED and READ);
- the only measured body difference on F8 is E21's tolerance, 1.699e-4 → 2.258e-4;
- trim deviations are equal to 4 digits.

The x-centroid differs by 4.6e-10 absolute on a body ±76 wide (relative 6e-12 of size). The 3.1e-5 relative figure is relative to a coordinate that is close to zero.

**The (b) case, precisely (MEASURED in `zz_d2` SFO; READ: the body builder `shared_face`).**

The body: a 10 mm cube, unioned with a 10×6×6 box that starts at x = 10 − 1e-6, after a record round trip. The union left the body inconsistent at 1e-6, inside its 1e-5 tolerance:
- side faces F7 and F10 are bounded by box-b edges E14 and E12 at x = 9.999999, so they read 60;
- F6 and F8 are bounded by the intersection edges E13 and E15, whose curves lie at x = 10.000000;
- the end vertices of E13 and E15 (V10, V8, V9, V11) sit at x = 9.999999.

On the raw body, F6's trim on E13 follows its edge curve to 1.8e-15, so the area is 59.999994. That is the analytic value and centroid.

The face pass on F6 alone, and no other step, moves that trim 1.000e-6 off E13 to meet the vertices, which gives 60.000000:
- `create_missing` changes nothing;
- passes on the other faces change nothing on F6;
- the F8 pass moves E15's trim the same way, but F8 already read 60 on the raw body.

Body area against the analytic 839.999976:

| | Main-raw | S |
|---|---|---|
| `area` | 839.999994 (error 1.8e-5) | 840.0 (error 2.4e-5) |
| `est` | 839.999988 (error 1.2e-5) | 840.0 (error 2.4e-5) |

The volume is the same on both sides and matches the analytic value (1359.999964). Main's own `area` and `est` disagree with each other on the raw body.

The defect, stated exactly: **the face trim pass's gap fix rewrites a pcurve that matched its edge curve so that it ends at the vertices instead. Where the vertices and the edge curve disagree inside tolerance, this moves the trimmed area by edge length × the disagreement (here 6 × 1e-6).** It stays inside tolerance and the change is relative 1e-7. It makes the four side faces agree with each other, but away from the analytic answer.

## 3. Validity checks

| Body (stripped) | Main-raw | S | Justified by the body state? |
|---|---|---|---|
| box_extrusion2 | valid, no reports | Three reports. "Tolerance: Edge tolerance relative to brep tolerance": E16 at 0.010366 against a limit of 0.01. Two "Vertex tolerance relative to brep tolerance": V9 and V12 at 0.010366. | Yes (MEASURED). `create_missing` alone produces the same three reports, and main's own copy after its 20 rays shows them too. Main's answers already come from that state; checking the raw body hides it. **(a)** |
| nurb_box1 | valid | The same three reports, for E8, V0 and V5 at 0.010593. | Yes, the same way (`create_missing` and main's copy both show them). **(a)** |
| vase1 | valid | `assert_valid = false`: one "Bad CurveKink — 2D BSplineCurve Warning: Kink at interior knot", value 0.99567 against 0.99985 at parameter 4.2897. | It is true of S's trims. It comes from the face pass: `create_missing` alone is clean, and main's copy after rays is clean. **(b), warning level.** |
| vase3 | valid | Two "Bad CurveKink" warnings, 0.98846 at parameters 5.18740 and 5.18739. | The same. **(b), warning level.** |

**As-is bodies (MEASURED).** All four as-is bodies, raw and properly settled, check clean on both sides. My first round settled a `clone_sharing_surface_caches` copy, which gave spurious "UVTrimCurve Owner" and "Edge ValidateGeometry" reports on vase1 and vase3. That is a probe artefact; the second round settles a fresh body and finds nothing.

So these reports appear only on stripped bodies.

## 4. Is main order-dependent on the same raw body?

Yes, for rays (MEASURED, 13 ray variant-states). The orders tried:
- the original order;
- ray→mass;
- mass→ray;
- cls→ray→mass;
- each ray on a fresh copy;
- reversed ray order on one copy.

The first four reproduce M0 exactly. Fresh-copy and reversed runs differ in 1 to 11 additional rays per body, for example:
- blade_hub_1: ray10, ray12, ray18;
- case_int: 11 rays.

These differences are always the same face, class and region, with Δt from 0 to 3.6e-14.

The channel is one standing copy shared by all 20 rays: each ray's lazily built trims and raised tolerances are seen by later rays (main's copy ends with S's tolerances).

Mass on raw bodies did not depend on the earlier ray, cls or box reads in any order tried: 0 differing keys.

On the settled body there were 0 differences across:
- one copy;
- a fresh copy per ray;
- reversed order through main;
- reversed order through the lazy tree.

So main on raw bodies is history-dependent at round-off level; settled is not.

## Notes

- The clean log reverses the brief's numbers. It shows open_shell as_is mass maxrel 6.506e-9 and RO:shared_face_overlap1e-6_union 1.000e-7.
- O:shared_face_overlap1e-6_union shows no mass difference only because main refuses all 13 face reads on the raw (non-round-tripped) body.
- The vase tight box was never called; the per-face reads alone covered vase2.
