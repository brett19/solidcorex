# 34 - Why a trim fails the lazy acceptance: true floor or fit error

Local measurement only. Nothing was pushed to GitHub or the fork, nothing was filed or commented.
- Base: `zz-tree-slot-s` (it has the per-test, thread-local switch `zz_lazy::set` and counters; `zz-lazytrim` has process-wide statics). It descends from `zz-lazytrim` (variant L's acceptance and the item-1 Line-on-Plane carry).
- Branch `cache-trimsplit-a` (local, a local worktree, head 50a78ecab7); server side ref `push/cache-trimsplit-a`, server worktree of the same name (build output trimmed).
- Labels: MEASURED (counted on the build server), READ (from the code, file:line), INFERRED (reasoned).
- Raw logs and the classifier: kept outside this repository (`ts-*.log` one line per trim, `an.py`).
- Code of the probe (all scratch, gated behind the probe's own calls): `solidcore/src/zz_split.rs`, hooks in `topology/edgeuse/p3.rs`, test `solidcore-convert/tests/zz_split_probe.rs`.

## Answer

Of 5,485 distinct trim builds on the prototype's full variant set, 24 fail the lazy acceptance (L = tol_edge + tol_face). Only 1 is a true floor. The rest are three different things, none of them "a trim that no fit could pass":

| class | distinct trims | meaning |
|---|---|---|
| FLOOR (G > L) | 1 | the edge's own gap to the surface is above the tolerance |
| FIT-NEAR (G between 0.5 L and L, trim D > L) | 10 | a better fit passes (9 of 10), but G already uses 82 to 91% of L |
| FIT (G <= 0.5 L, D > L) | 0 | none |
| PATH-ONLY (loosened or interval path, but the trim's bound B <= L) | 8 | the trim is fine; it is refused only because the build needed a write |
| BOUND-SLACK (D <= L, B > L) | 1 | trim is fine, proof loose, B/D = 29 |
| NO-PROOF (no certified bound) | 4 | all ProceduralBlend; true D is under 2% of L |
| SINGULAR (seam, pole, domain, crease) | 0 | none of the 24 |

The main finding is not in that table. The fitter's target tolerance (`fit_tolerance`, p3.rs:516) is the edge tolerance alone, but acceptance uses edge plus face tolerance, and the fitter's budget ignores the gap G the edge already has to the surface. Both sets of failures follow from that (see "Mechanisms").

## Method (MEASURED)

Corpus: the prototype's set, 5 primitives, 47 Boolean scenarios, 11 fillet/offset/shell/sweep/revolve bodies, 28 corpus files (26 distinct: blade_hub_1 = cliff_1 and blade_hub_2 = cliff_2, dropped as duplicates), record round trips of the kernel bodies; each as built and with every stored trim stripped. Rows: 10,967 distinct (label, variant, edge use pair). Each edge use pair is measured once (a trim serves an edge use and its mate). Stripped: 5,485 trims; as built: the stored ones (analysed as `Stored`) and the missing ones (as `Accepted` or a refusal). All with the lazy switch on (item-1 carry), per test thread. Serial per test, four test processes in parallel on the build server.

Per trim, the probe (`zz_split::probe_eu`):
1. Builds the trim as `lazy_uv_trim_curve` does (`build_lazy_trim`, Lazy mode, acceptance bound with enough = over = L). The outcome is Accepted, BoundExceeds, NoCertifiedBound, or a write refusal (FitLoosened, IntervalRecovery, BruteForce).
2. If no trim came out (a write refusal), builds it again with `create_uv_trim_curve` on a `clone_sharing_surface_caches` copy that may write, so the trim exists to be measured. L and fit always come from the original body.
3. Records the fit path through hooks in `create_uv_trim_curve_in_0`: `ii` (0 or 1 target, 2 or 3 loosened, brute force), the zone tolerance, the drop's own max distance and deviation, the reason each earlier `ii` was rejected, and the trim's control point and span counts.

Quantities:
- **G**, the edge-to-surface gap: the edge's 3D curve sampled over its interval, with its knot spans (the union of the edge's and trim's distinct knots) split into 200 intervals each (400 for an analytic edge in one span; capped at 20,000 samples, 20 per span at least; 42 rows hit the cap, files with over 100 spans such as test8, and have fewer than 200 per span; none of them fails or is in the table below). Each point is dropped onto the surface with `Surface::drop_point` over the natural UV domain, twice (seeded by the previous foot and by the trim's own UV, the nearer taken). The distance is also capped by the distance to the trim's image point at the same parameter (a surface point, so it is an upper bound too). Then 34 golden-section steps around the worst sample. G = max.
- **G certified**: `edge_face_gap_bound`, p3.rs:2820 (made `pub(crate)`), which is `curve_face_gap_bound_within` (curve_surface_gap.rs:1348) at its tightest (enough = -inf, over = +inf, floor = 1e-3 fit) plus a line's evaluation slack. An edge-to-surface enclosure exists where the surface has an exact NURBS net and the edge is a B-spline, ellipse or line. It does not exist for OffsetSurface (96 rows) and ProceduralBlend (16 rows) faces. Where both exist the reported G is min(sampled, certified); the sampled value is above the certificate in 101 rows, all imported files with B-spline surfaces (98 by less than 1e-3 L; 3 by up to 0.36 L on `fillet_ts`), which shows the drop landing in a non-nearest foot there. So sampled G can only overstate; the FLOOR call below has a certificate that agrees with the samples to 0.04%.
- **Kernel's own gap**: `calc_max_edge_face_gap_read` (the 20-sample `GapFunction`, edgeuse/p1.rs:1064), and the cached value when the use has one (1,938 of 5,485 stripped trims; the cached value equals the calculation wherever both exist).
- **D**, trim image to edge: for every trim image sample S(W(t)) the distance to the edge curve, and for every edge sample the distance to the trim's image, each by nearest sample then golden-section refinement (Hausdorff, as `trim_image_gap_bound` defines B: curve_surface_gap.rs:3235 doc). A polyline distance was rejected: its chord error (about 3e-5 at 200 points on a 10 mm arc) is above the tolerances.
- **F**, fit error: the same, but against the projection of the edge: for each image point, the distance to the foot of its nearest edge point; for each edge foot, the distance to the image. F is what a better fit could remove; D is about sqrt(G^2 + F^2).
- **B**: tight certified bound `trim_deviation_bound(.., [-inf, +inf], budget 4096 pieces)`; `Bacc` is the bound as acceptance computes it (any bound up to L, `inf` above L). B tight is `-` where none is enclosed or the budget ran out.
- Seam: two uses of the edge in one face use. Pole: the surface has singularities and the partials at the worst foot differ by more than 1000. Domain: fraction of feet on the natural UV border. Crease: surface normal turns by more than 10 degrees between neighbouring feet near the worst sample. (Heuristics; for a plane the domain test is not meaningful.)

Caveats:
- G, D and F are sampled, so D and F can be too high where a nearest search missed (a wrong foot) and too low between samples. A first run had such errors (G up to 5,000 L from a foot on another sheet of a close B-spline surface); the second seeding and the same-parameter cap fixed the gross ones. After the fix the maxima of G/L, F/L and D/L are 1.01, 2.4 (a stored trim) and 1.0.
- The prototype's counts (BoundExceeds 254, FitLoosened 74, NoCertifiedBound 42) are reads and slot entries over its whole run (stage 1 and 2, every fill order, repeated families). Here each trim is counted once, so the numbers are not comparable: distinct trims on the same set are BoundExceeds 11 (plus the 2 duplicate-file ones), FitLoosened 8, NoCertifiedBound 4 (INFERRED that the difference is repetition; the two sets are the same bodies).

## Counts by class and body class (MEASURED)

Failing trims, stripped variant (every trim built), by body class (the "stripped" class of the brief is this column; the other classes in the "as built" rows are the missing trims of the as-built body):

| body class (trims) | FLOOR | FIT | FIT-NEAR | PATH-ONLY | BOUND-SLACK | NO-PROOF | SINGULAR |
|---|---|---|---|---|---|---|---|
| kernel-made primitives (42) | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| after an operation (1,844) | 0 | 0 | 0 | 0 | 0 | 4 | 0 |
| imported corpus (1,713) | 1 | 0 | 10 | 8 | 1 | 0 | 0 |
| round trip (1,886) | 0 | 0 | 0 | 0 | 0 | 4 (the same 4 trims) | 0 |
| all stripped (5,485) | 1 | 0 | 10 | 8 | 1 | 8 (4 distinct) | 0 |

As built: the stored trims (kernel 41, op 1,554, imported 1,307, round trip 1,881) are only measured. The trims the as-built bodies lack are built and judged: op 290 (0 fail), imported 405 (3 fail: the cliff_1 FLOOR trim and two vase3 PATH-ONLY trims, which are also in the stripped set), round trip 4 (NO-PROOF, the same blend trims).

Distinct failing trims: 24 (20 imported, 4 blend). By outcome: BoundExceeds 11 (FLOOR 1, FIT-NEAR 9, BOUND-SLACK 1); FitLoosened 8 (PATH-ONLY 7, FIT-NEAR 1); IntervalRecovery 1 (PATH-ONLY); NoCertifiedBound 4. BruteForce, Reentrant and BuildFailed never occurred. None of the 24 is at a seam, pole or crease (flags all false).

Where they live: imported fillet files only (vase1, vase2, vase3, bottle, open_shell, shell_fillet2, cliff_1/blade_hub_1), plus `variable_round` (a variable-radius fillet, ProceduralBlend faces) and its round trip. No primitive, Boolean, offset, shell, sweep or revolve trim failed.

## The failing trims (MEASURED)

Values are absolute (model units). fit = the fitter's target; G = min(sampled, certified); path = fit path taken; x0.5, x0.25 = refit with target scaled; xL = refit with the target scaled to L (that is, fitted to edge + face tolerance).

| trim | outcome | class | L | fit | G | G/L | F | D | B tight | path | ncp / spans | surface / edge |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cliff_1 / blade_hub_1 eu316 | BoundExceeds | FLOOR | 4.543e-4 | 4.443e-4 | 4.593e-4 (cert 4.594e-4) | 1.01 | 2.2e-5 | 4.594e-4 | 4.595e-4 | target (ii 0) | 64 / 21 | SurfOfRevolution / BSpline |
| vase3 eu104,108,120,124 | BoundExceeds | FIT-NEAR | 5.412e-4 | 5.312e-4 | 4.920e-4 | 0.91 | 4.45e-4 | 5.671e-4 | 5.673e-4 | target | 61-76 / 20-25 | OffsetSurface / BSpline |
| vase2 eu66,70,90 | BoundExceeds | FIT-NEAR | 7.552e-4 | 7.452e-4 | 6.866e-4 | 0.91 | 3.93e-4 | 7.797e-4 | 7.800e-4 | target | 61 / 20 | OffsetSurface / BSpline |
| vase2 eu94 | BoundExceeds | FIT-NEAR | 8.675e-4 | 8.575e-4 | 7.887e-4 | 0.91 | 3.90e-4 | 8.799e-4 | 8.802e-4 | target | 76 / 25 | OffsetSurface / BSpline |
| vase2 eu58 | BoundExceeds | FIT-NEAR | 8.675e-4 | 8.575e-4 | 5.692e-4 | 0.66 | 6.81e-4 | 8.875e-4 | 8.878e-4 | target | 76 / 25 | OffsetSurface / BSpline |
| bottle eu128 | FitLoosened | FIT-NEAR | 8.302e-5 | 6.302e-5 | 6.807e-5 (cert 6.810e-5) | 0.82 | 9.28e-5 | 8.669e-5 | 1.880e-4 | loosened, zone 1.33e-4 | 46 / 15 | BSpline / BSpline |
| shell_fillet2 eu24 | BoundExceeds | BOUND-SLACK | 2.003e-4 | 9.49e-5 | 7.364e-5 | 0.37 | 5.7e-5 | 7.364e-5 | 2.153e-3 (B/D = 29) | target | 82 / 27 | BSpline / BSpline |
| shell_fillet2 eu0 | FitLoosened | PATH-ONLY | 2.003e-4 | 9.49e-5 | 1.025e-4 | 0.51 | 8.2e-8 | 1.025e-4 | 1.025e-4 | loosened (G > fit) | 34 / 11 | Plane / BSpline |
| vase1 eu46,54; vase2 eu50; vase3 eu16,24 | FitLoosened | PATH-ONLY | 0.83-1.06e-4 | 0.63-0.86e-4 | 0.67-0.87e-4 | 0.78-0.82 | 1e-13 | = G | = G | loosened (G > fit) | 4 / 1 | Plane / BSpline |
| open_shell eu40 | FitLoosened | PATH-ONLY | 4.321e-5 | 2.321e-5 | 3.019e-5 | 0.70 | 5.9e-8 | 3.024e-5 | 3.024e-5 | loosened (G > fit) | 2 / 1 | BSpline / Line |
| open_shell eu42 | IntervalRecovery | PATH-ONLY | 4.321e-5 | 2.321e-5 | 1.255e-5 | 0.29 | 6e-10 | 1.255e-5 | 1.255e-5 | target ii 1 (edge interval would move) | 4 / 1 | BSpline / Line |
| variable_round eu82, eu86 | NoCertifiedBound | NO-PROOF | 8.0e-5 | 4.0e-5 | 1.02e-6, 5.1e-7 | 0.013, 0.006 | 1.1e-8, 5.4e-9 | 1.02e-6, 5.1e-7 | none | target | 55 / 18 | ProceduralBlend / BSpline |
| variable_round eu80, eu84 | NoCertifiedBound | NO-PROOF | 8.0e-5 | 4.0e-5 | 2e-14 | 0 | 6e-14 | 4e-14 | none | target | 2 / 1 | ProceduralBlend / BSpline |

Kernel's own cached gap on these equals the 20-sample calculation wherever both exist (4,678 rows, 0 differ) and is below L in all 24. On `cliff_1 eu316` it is 4.130e-4, 10% under the true 4.593e-4 (the body's edge tolerance 4.443e-4 was healed to cover that 20-sample value: the true gap is 1.1% over L).

## Mechanisms (MEASURED unless marked)

1. **FLOOR (1 trim).** `cliff_1 eu316` (the same trim in `blade_hub_1`, a byte-identical file). The 3D edge is 4.593e-4 from the surface (sampling and certificate agree to 4e-4 relative) and L is 4.543e-4. The 20-sample edge-face gap the body trusts says 4.130e-4. No trim can pass here; a trim made by settle passes only because settle raises the edge tolerance (INFERRED: this is the case the raise code is for). The body's tolerance was set from an under-sampled gap (MEASURED: 4.13e-4 against 4.59e-4).

2. **PATH-ONLY (8 trims) are a fit target that is below the gap.** In 7 of 8 the edge's own gap G is above the fitter's target `fit` (vase1 8.71e-5 over 8.58e-5; vase2 7.05e-5 over 6.76e-5; vase3 6.66e-5 over 6.33e-5; open_shell 3.02e-5 over 2.32e-5; shell_fillet2 1.025e-4 over 0.949e-4). The drop at `ii` 0 and 1 then finds no curve within the target ("drop gave no curve", 8 of 8 FitLoosened), the build goes to `ii` 2, which is a write. The trim it then makes is fine (D = G, F about 0, B tight = D within 0.1%). Fitted again with the target scaled to L (xL, `fit` = edge + face tolerance), the build stays at `ii` 0 and is Accepted in all 7, with the same ncp (2 to 4) and B <= L. The 8th (open_shell eu42) is accepted at xL too. So the face tolerance, which acceptance counts and the fitter does not, is the whole difference. Halving the target does nothing for these (the build still needs `ii` 2; at a quarter it falls to brute force with 101 points).

3. **FIT-NEAR (10 trims): a better fit passes, with 0.5% to 1% headroom.** These are OffsetSurface faces (9; vase2, vase3, `Gcert` none) and one bottle edge. The fitter met its target (`fit` = edge tolerance; the build is at `ii` 0, drop distance = G), but the gap G is already 0.66 to 0.91 L, so G plus the fit's own error exceeds L (F = 0.58 to 0.88 L, D 1.04 to 1.05 L). The fit target is edge tolerance and does not subtract the gap (INFERRED from fit_tolerance, p3.rs:516, and the build's use of `zone_tol = fit_tol` at p3.rs:~720). Refit:
   - target halved (x0.5): 9 of 10 pass the bound (B 5.384e-4 against L 5.412e-4; 7.489e-4 against 7.552e-4; 8.652e-4 against 8.675e-4: margins 0.5% to 1%). Control points: 64 to 85, 73 to 94, 76 to 97, 61 to 82 (vase3), 61 to 82, 76 to 85, 76 to 79 (vase2). That is +9 to +27 points (+12 to +34%), about +21 on average.
   - target quartered (x0.25): the same 9 pass, ncp 85, 94, 82, 97 (vase3, equal to x0.5), 91, 91, 91, 124, 103 (vase2: 85 to 124).
   - more spans allowed: the fitter exposes none. The only cap is `MAX_CTRL_POINTS = 10,000` (intersect/surface_drop_lib/p2.rs:127) and `MAX_SPAN_DEPTH = 64` (p3.rs:169); the largest trim here has 124 points, so no cap was reached and none can be the cause. The one control the fitter has is `approx_tol_3d` (the `zone_tol` passed to `surface_drop_curve`, p3.rs:205).
   - xL (target scaled to L, i.e. looser): all 9 stay BoundExceeds, as expected.
   - The bottle edge (G 0.82 L, F 9.3e-5 above L) is not helped: x0.5 and x0.25 go to brute force (101 points, B inf), xL gives BoundExceeds. So 1 of 10 FIT-NEAR trims is not fixed by the fitter's one knob; it needs a different fit (INFERRED: the drop does not converge at the tighter tolerance on that surface).
   - No trim is FIT proper (G <= 0.5 L): every trim whose fit exceeds the budget has used at least 0.66 L of it on the gap.

4. **BOUND-SLACK (1 trim).** `shell_fillet2 eu24`: D = 7.36e-5 against L 2.0e-4 (37%), B tight 2.15e-3, B/D = 29 and above L by 10.8x, on two B-spline curves (82 control points, 27 spans). The trim is well inside; the proof is not. With the target scaled to L (xL) the same edge gets a 64-point trim with B = 1.98e-4, accepted, so the slack is a property of this trim's knot structure and not of the geometry (INFERRED: reparametrisation `psi`, curve_surface_gap.rs:2928, over many spans). Halving the target makes it worse (B inf at 85 and 103 points).

5. **NO-PROOF (4 trims).** One pair: ProceduralBlend face, B-spline edge, B-spline trim (`variable_round`, and the same blend after a record round trip). `trim_deviation_bound` returns `None` because a procedural blend has no exact net (`NetSurface::of_exact`, curve_surface_gap.rs: the doc of `trim_image_gap_bound`, 3235-3250), and neither does the edge-to-surface enclosure (no `Gcert`). True G and D are 1e-6, 5e-7, 2e-14 against L = 8e-5: they would pass with at most 1.3% of L. The Line-on-Plane pairs that were 4,018 on main are all covered by the item-1 carry (0 here).
   - Also without a certificate for the edge: OffsetSurface (trim bound exists, edge enclosure does not), 96 rows.

6. **SINGULAR: none.** The seam, pole, crease and domain flags are false on all 24. (Seams do occur among the other trims: sphere and cylinder seam edges, all accepted; the flag is not the cause of any failure.)

## Distributions over all trims (MEASURED, stripped variant, 5,485 trims)

G/L, F/L and D/L (G = min of sampled and certified). p50 / p90 / p99 / max:

| body class | n | G/L | F/L | D/L |
|---|---|---|---|---|
| kernel-made | 42 | 0 / 5e-10 / 8e-10 / 8e-10 | 0 / 8e-10 / 5e-9 / 5e-9 | 0 / 4e-10 / 6e-10 / 6e-10 |
| after an operation | 1,844 | 0 / 6e-10 / 0.11 / 0.25 | 0 / 9e-10 / 0.038 / 0.80 | 0 / 8e-10 / 0.15 / 0.80 |
| imported corpus | 1,713 | 1e-8 / 0.72 / 0.92 / 1.01 | 4e-9 / 0.11 / 0.73 / 1.12 | 7e-9 / 0.73 / 0.99 / 1.0 |
| round trip | 1,886 | 0 / 8e-10 / 0.11 / 0.25 | 0 / 1e-9 / 0.038 / 0.80 | 0 / 9e-10 / 0.15 / 0.80 |
| all | 5,485 | 4e-11 / 9e-5 / 0.91 / 1.01 | 2e-10 / 7e-5 / 0.43 / 1.12 | 2e-10 / 8e-4 / 0.92 / 1.0 |

Headroom counts, stripped:
- imported: G/L above 0.5 on 252 of 1,713 trims, above 0.9 on 96; F/L above 0.5 on 39; D/L above 0.5 on 261, above 1 on 11. These are the fillet files; their edges sit close to their tolerance.
- after an operation and round trip: G/L above 0.1 on 20 of 1,844 (and 1,886), none above 0.5; F/L above 0.5 on 1 trim (0.80, a 1e-8-size blend trim). They have ample room.
- kernel-made: no headroom problem at all (all below 1e-8).
- accepted trims with a tight bound: B/L above 0.5 on 245 of 5,457, above 0.9 on 103. So acceptance has a thin margin on about 2% of accepted trims, from the same fillet files.

B/D (certified over sampled, on trims with D >= 0.01 L, n = 982): median 1.00, p90 1.00, p99 1.06, max 29.2 (the `shell_fillet2` trim). So the bound is tight (within 6%) on all but one. Below D = 0.01 L the bound's floor (1e-3 of the fit) dominates, and B/D is not meaningful.

Kernel's cached edge/face gap against G: on 293 of 5,485 trims the true gap is more than 5% above the 20-sample value. The worst: `torii_unioned eu8` (G = 1.219e-3, certificate 1.220e-3, kernel 4.3e-5, 28 times larger; still below L = 1.230e-3, accepted); `cliff_1 eu316` above; `revolution_mold eu176` (3.64e-4 against 3.31e-4); `revolution2 eu142` (3.85e-4 against 3.57e-4). The kernel gap is above L on 0 trims; it never reports a failing trim as a floor.

## Cost of the probe (MEASURED, indicative)

Four processes in parallel; 247 s (kernel bodies), 468 s (corpus B), 476 s (round trips), 718 s (corpus A with the heavy files; the second run 14 min). 936 s of per-trim time for the stripped variants; most of it is the sampling and the tight bound, not the builds. Each run is under the 30 minute cap.

## The gap-closing pass at a vertex (READ)

Where it runs: `create_uv_trim_curves_body` (topology/face/p3.rs:1297). The first loop (p3.rs:1323) makes every edge use's trim with `get_or_create_uv_trim_curve`. The second loop (p3.rs:1391) walks each edge use `eu` and its clockwise neighbour `cw` at the vertex `v`, compares the UV end of `eu`'s trim with the UV end of `cw`'s (p3.rs:1400-1405), and if the gap is above a scaled zero and not at a pole (`gap_at_pole`, p3.rs:1407-1425) calls `fix_uv_trim_curves_notified` (p3.rs:1439; defined at topology/face/p2.rs:3366, not p3.rs).

What decides the common point (face/p2.rs:3422-3430): `uv_vertex = vertex_face_gap(vu).face_uv`, else `compute_uv_point(vu, ok_to_make = true)`. Both are `uv_point_in` (topology/vertexuse.rs:1312): the UV of the closest point on the face's surface to the vertex's 3D point. For an edge vertex use:
- guess = the UV start of this edge use's trim (vertexuse.rs:1382-1384), replaced by the previous edge use's trim end where the two are a period apart (a seam: vertexuse.rs:1398-1500, chosen from the trims' UV ends and tangents);
- `local_point_solve` from that guess (vertexuse.rs:1508); if it moved more than half a domain or its distance is above vertex plus face tolerance, a `global_point_solve` (vertexuse.rs:1536);
- with several global solutions the one nearest this trim's UV start wins (vertexuse.rs:1545-1556);
- a final seam wrap by the gap from the guess (vertexuse.rs:1565-1580).
A loop vertex use (a single vertex loop) takes the first global intersect then minimise solution (vertexuse.rs:1583-).
The moves are then: trim each curve at the drop point of that UV, stretch back over the interval (unless `uv_trim_corner_end_points_only`), move each end control point to the UV with `spread_end_move`, and raise the edge tolerance if the move left the trim uncovered (`keep_trim_covered`, p2.rs:3651).

Could it be computed before any trim exists, from the vertex and the surface alone? For most vertices, yes: the closest point of the vertex on the surface is the one input that matters (the guess only selects among solutions), and `calc_vertex_face_gap(.., ok_to_make = false)` (vertexuse.rs:624-672) already computes it by "a global solve without touching the UV trim curve" (its own comment, vertexuse.rs:656-658). The measurement says the vertex is within the edge's reach of the surface at this scale: the trim ends gap is a fraction of the tolerance (INFERRED).

What would stop it:
1. **Seams, poles and multi-solution surfaces.** The side of a seam, the corner of a doubly periodic surface and the choice among global solutions are made from the two trims' UV ends and tangents (vertexuse.rs:1382-1500, 1545). Without trims there is one UV per vertex per face, but the vertex has two on a seam and a line on a pole (the pass skips poles at p3.rs:1407). So the target depends on which edge use is asked, and the edge use's own direction is needed.
2. **The fitter takes no end constraint.** `surface_drop_curve` (edgeuse/p3.rs:205) and `Surface::drop_curve` (surface_drop_lib p3.rs:651, 704) take the surface, the 3D curve and its interval, a tolerance and flags, nothing about where the UV curve must start or end. It marches from the curve's start point. So it cannot be told to end at the vertex's UV; the ends are what the closing pass edits afterwards (`edit_end_point`, face/p2.rs:3300-3316). Fitting to a given end needs a constrained approximation in the drop library (a change to a 10,000-line port), or an equivalent one-step fix of the end control points as the pass does today.
3. **The 3D end of the edge is not the vertex.** The edge curve ends within the vertex tolerance of the vertex; the trim's end is the foot of the curve's end, the target is the foot of the vertex. They differ by up to the vertex-edge gap, which is the gap the pass closes. A trim fitted to end at the vertex's UV would carry that gap as fit error (F), which the acceptance counts (L = tol_edge + tol_face). The pass accounts for this with `keep_trim_covered`, which raises the edge's tolerance. A trim built to that end would need the same.
4. **Order and writes.** `vertex_face_gap` caches on the vertex use and syncs siblings (vertexuse.rs:640-668) and `compute_uv_point` with `ok_to_make = true` can make trims (it calls `normalized_evaluate` with ok-to-make). A pre-pass would be a write on the body unless cached lazily.
5. **Tolerances change as the pass runs** (p3.rs:1470 `vertex::set_tolerance`), and the fit target (`fit_tolerance`) reads the edge's and its faces' tolerances, so a trim fitted before the pass is fitted at a different target than after.

Net (INFERRED): the common UV point could be had from vertex plus surface for a vertex off a seam and pole, with `compute_uv_point`'s global arm; fitting to end there is not something the fitter can do today, and for seams and poles the trims are needed to pick the point. It does not remove the failures in this report, which are fit-target and bound issues of single trims.

## What follows (INFERRED)

- The cheapest change that removes 8 of 24 failures is on the fit target, not the trim: fit at `fit_tolerance + face tolerance` (the amount acceptance allows) so a trim over an edge whose gap is above the edge tolerance is not sent through the loosened path (xL: 7 of 7 `FitLoosened` PATH-ONLY accepted at `ii` 0, same control points). That is a tolerance the acceptance already counts; whether the fitter may use it is a kernel-rules question and was not tried on main.
- The 10 FIT-NEAR need a target of about L minus G minus the fit's own bound, not the whole edge tolerance. Halving it fixes 9 of 10 at +21 control points on average (12 to 34%), with 0.5 to 1% margin; so one halving is a cliff of margin, not a design. A target of `(L - G) / 2` with G read from the certified edge enclosure (where it exists: not for OffsetSurface or ProceduralBlend) would be the principled version.
- One of 5,485 is a floor: `cliff_1 eu316`, and it exists because the body's own tolerance was set from the 20-sample gap (4.13e-4 against 4.59e-4). It is settle's raise case; a lazy read refuses it correctly.
- BOUND-SLACK is one trim in 5,485 and is not worth a tighter proof by itself.
- NO-PROOF needs an exact net for ProceduralBlend, or a refit that passes `trim_deviation_bound`'s `None`; four trims in one fillet file.

## Not done

- Sampling G, D, F cannot prove a bound; D and F are estimates. The FLOOR call has a certificate, the FIT-NEAR calls rely on the post-refit B (certified) for passing and on sampled F for the cause.
- The "FIT-NEAR" class is my label for G in (0.5 L, L] with D > L. The brief's FIT (G well under L) has no members.
- `Reentrant` and `BruteForce` refusals never occurred in the lazy build; brute force appears only in the refit experiments (x0.25, bottle and PATH-ONLY trims).
- Seam, pole and crease flags are heuristics; they were false on all 24 failing trims, so the SINGULAR class is empty by that test, not proved empty.
- Not measured: trims in bodies outside this variant set, the stage-2 reads, the cost of a refit on main.

