# Review: `cache-plane-trims` (carry a record's plane pcurves into the rebuilt SMLib body)

Branch `cache-plane-trims`, 5 commits on origin/main 0f162cb3fc (bb8cca1a0b..ac927f117b).
Read-only review: I read the code, did not build, and ran no tests. Line numbers are on the branch.

## Verdict: PASS WITH FIXES

The map is correct for every case the code admits. Fallbacks run main's path. I found no way for a
crafted record to cause a panic, a loop or unbounded allocation. There are no blockers. Six
SHOULD-FIX items: four in the PR text, one in the layering, one in the measurements.

## 1. Correctness of the map

**Two maps, not one.** The change runs in two stages, so a single "inverse of the export" is not
the whole story.

- **Import (record to flat arrays): `import.rs:698-731`.** SMLib's frame is x = U/|U|,
  z = (U×V)/|U×V|, y = z×x, with the record's origin. With m = [U·x, V·x, U·y, V·y], the record
  point origin + sU + tV is origin + (m0 s + m1 t)x + (m2 s + m3 t)y. This is exact, because U and
  V lie in span(x, y).
  - The map is linear with no translation, so applying it to a rational pcurve's homogeneous
    (wx, wy) is also correct (`import.rs:651-659`). `trim()` divides by h[2] later
    (`import.rs:758`).
  - Since z follows U×V, det m = (U·x)(V·y) - (V·x)(U·y) = |U×V| > 0 always. A mirrored record
    plane (c < 0, or a < 0) becomes a positive-determinant map onto a frame whose normal is the
    record's, so a "negative determinant" case never reaches the trim.
  - Scaled and sheared axes are exact up to rounding. `Plane` and `PlaneThrough` share `plane()`.
  - The bilinear-patch branch (`import.rs:701-707`, `arrays_keeping_splines` only) has knots
    [u0,u0,u1,u1], so the patch's NURBS (u, v) equal the record's (s, t) exactly. Carrying with
    m = None is right there.
- **Rebuild (flat arrays to SMLib): `convert.rs:1920-1945`, `:2428-2434`, `:1972-1982`.** The
  arrays' plane is `Plane::create_canonical` with uv_scale (1, 1). `convert_uv_from_nurbs_to_step`
  is the base identity for a plane (`surface/surface/p2.rs:2772-2783`), so `plane_map` is exactly
  [0,1,0,0,0,1]. Its computed inverse is that identity up to the sign of zero
  (i2 = -0/1 = -0.0, offsets -0.0). Adding ±0 never changes a non-zero coordinate and is harmless
  for a zero one.
  - The frames agree: import y = cross(z, x), and convert `placement` uses `set_canonical(origin,
    x, z.cross(x))` (`convert.rs:2062-2066`).

**Reversed uses.** `trim()` reverses the points and negates and reverses the knots
(`import.rs:762-765`). `bspline_curve_2d` with `!same` reverses them back
(s = s_lo + s_hi - src[n-1-j]). The affine map is applied to points only, so it commutes with the
reversal. This is the same path B-spline faces already take.

**Knots.** Knots map affinely onto the edge's SMLib range (`convert.rs:1994-1998`):
`lo + (s - s_lo)·(hi-lo)/(s_hi-s_lo)`.

- The first knot equals lo exactly.
- The last knot can land a few ulps on either side of hi, so yes, the end parameter can move
  outside the edge's interval by rounding.
- AssertValid Edgeuse 25/26 allows 100·scaled_zero (`assert_valid/edge.rs:718-728`), so that is
  harmless.
- The radian/degree map is linear, so the trim's parameter stays an affine image of the edge's.
  The "4 ulps" claim is sound as a rounding statement.
- NIT: pin the last degree+1 knots to `hi` (and the first to `lo`) so the interval is exact by
  construction.

**Wrong curve that still passes.** `along_edge` compares the pcurve and the edge **at the same
parameter** t (`import.rs:676-692`). That rules out:
- a full circle traversed the other way (fails at t0 + 0.2Δ);
- a phase-shifted closed curve;
- the complementary arc;
- the wrong branch of a closed curve;

unless the edge is shorter than about its tolerance, which Edge 7 forbids.

What it cannot rule out is a pcurve that matches at the 6 samples and wiggles between them, for
example a high-degree spline whose control polygon oscillates.
- For degree-1 pcurves on line edges (every plane line use), the deviation is linear in t, so the
  two end samples bound it everywhere. The check is rigorous there.
- For conics it is very strong: two distinct conics share at most 4 points, and here 6 are matched
  in parameter too.
- For general splines it is a sample, not a bound. See §2.

## 2. The verification

- **Is it sufficient?** Not as a proof. It is no weaker than what the record already promises:
  `record_check::agreement` samples 3 points per use against `e.tolerance`
  (`solidcore/src/eval/agreement.rs:300-322`), and this samples 6 against
  max(edge tolerance, body tolerance).
  - The tolerance document's rule (validity-and-tolerances.md §5.2: a trim within τ_e + τ_f
    everywhere) is met rigorously for line uses and to sampling for curved ones.
  - The document also says the pcurve does not enter the edge/face gap checks, and that the
    kernel accepts trims "set without a raise (read from a file)" (`edgeuse/p3.rs:1826-1830`).
- **Ends at vertices and orientation.** These are not checked separately. The end samples t0 and
  t1 tie each end to the edge curve's ends, so to the vertex within τ_e + τ_v. Orientation follows
  from the parameter-matched comparison plus the existing reversal.
- **Post-build steps.**
  - The trim-raise step in `get_or_create_uv_trim_curve` (`edgeuse/p3.rs:1782-1803`) is skipped
    for carried trims. This explains the 2/4 → 0/0 tolerance raises, and it agrees with §8:
    nothing is grown without a bound.
  - The vertex UV gap fix and the vertex tolerance adjustment live in the face-level
    `create_uv_trim_curves_body` second loop (`face/p3.rs:1390-1470`). That loop runs over every
    use whether or not its trim pre-existed, so it is **not** skipped wherever it ran on main.
  - The per-use lazy path main used never ran it.
  - I found nothing downstream that depends on a step the carry now skips.
- **Hostile input (crash safety).**
  - Records reaching the carry pass `validate_built`: finite values, budgeted counts, and a
    pcurve interval within 1e-9 of the edge's (`solidcore-record/src/brep/topology.rs:146-155`).
  - The carry has a fixed 6 steps, no indexing that can panic, `Option`/`Result` throughout, and
    a NaN distance compares false, which falls back.
  - Its allocation is O(pcurve size): a knot clone for a non-identity m, plus one `CurveEval` per
    use.
  - Nearly parallel axes: `plane()` refuses ln == 0 as before. Near 0, m stays bounded by |U|, |V|
    and the worst outcome is a fallback.
  - A wiggling crafted pcurve can install a trim that passes, and that could skew mass integrals
    or classification. It produces a wrong answer from a lying record, not a crash, and the
    record's own agreement check accepts the same record.
  - **NIT:** say in the doc comment of `along_edge` that it is a sample. Optionally, carry only
    pcurves of degree ≤ 2 (what the kernel writes for lines and arcs), where the samples are
    effectively a proof.
- **SHOULD-FIX (new refusal path, trivial):** `convert.rs:2431` calls `plane_map(k, &surface)?` for
  every plane face on the rebuild path, so an `Err` from
  `convert_uv_from_nurbs_to_step`/`kind_with_nurb` now refuses the whole rebuild, where main never
  called it. It is practically unreachable for a canonical plane, but the rule is "valid input
  never becomes a refusal". Map an error to `TrimUv::Rebuilt` (`.ok().flatten()`) instead of `?`.

## 3. Fallback and conditions

| Condition | Where | Result |
|---|---|---|
| Record face not `Plane`/`PlaneThrough` (incl. a revolution the analytic pass made a plane) | `own_plane`, `import.rs:258-260`, `:314` | `None`, a bare use: main's behaviour. Correct. |
| Failed check | `import.rs:666` | `None`: main's behaviour (the test checks 46 carried, 2 missing). Correct. |
| `arrays_with_bare_uses` | `record = None`, `import.rs:135` | No plane is carried; `any_trims` is false for an all-plane body, exactly as main. Correct. |
| `arrays_touching` | `record = Some(original)` | `touching::shared` keeps the face order (`touching.rs:291-302`), so `own_plane(fi)` indexes the right face. A glued use now points at its representative edge; if that edge's parameterisation differs, the check fails and falls back. Correct. |
| NURBS form on a plane face | `import.rs:279-283` | `Uv::Record`, carried as before. |
| "Parallel axes (no inverse map)" | PR text | **Not a fallback.** Parallel record axes are refused in `plane()` (`import.rs:710-712`), as on main. `plane_map_inverse` returning `None` cannot happen for a canonical SMLib plane. See §7. |

One behaviour change is not listed. In `arrays_keeping_splines`, a non-unit plane goes as a
bilinear NURBS face. On main its uses got no trims (`plane()` returned `()`, so `Ok(false)`). Now
they carry the record's pcurve. That is correct (see §1), but the PR should mention it.

## 4. Results

- **Consistent with the mechanism:**
  - Plane trims made fall from 35,021 to 10,389.
  - Checks make 0.
  - Tolerance raises go 2/4 → 0/0, because the trim raise is skipped (§2).
  - The 12 circle uses: the live trim runs about 3e-10 past the edge's end, a live-body artefact
    the record never held. Not a defect here.
- **All-8 bit-equality 99 → 83.** Consistent: main's rebuilt trims are made by the same algorithm
  as the live body's lazily made trims, so the integrals reproduce the live body's bits. The
  record's pcurves are a different (exact) curve. Area and volume improving while centroid and
  inertia bits worsen fits this. It does not indicate a defect.
- **SHOULD-FIX (PR text): the user-visible change relative to main is not stated.**
  - A reopened design is measured on the rebuilt body, so what changes for a user is main-rebuilt
    vs branch-rebuilt.
  - From the PR's own numbers: both are within 1e-9 relative of the live body except one body, so
    main vs branch is within about 2e-9 relative on all bodies but that one and the two whose area
    moved past 1e-9 on main and now agrees with live. Those two change by more than 1e-9 on
    reopen.
  - The PR should give the measured maximum relative change of each mass field between main and
    the branch, and how many bodies change at all.
- **SHOULD-FIX (PR text):** "After a rebuild, every planar edge use holds its trim, so none is made
  again" is too strong.
  - 10,389 plane trims are still made in the corpus. These probably come from faces made by
    operations, the bare-use path, and failed checks; the PR should say where.
  - Qualify the claim to "every planar use whose pcurve passes the check".

## 5. Tests

- **`a_records_plane_pcurves_are_the_rebuilt_bodys_trims`:** fails on main for the right reason.
  The test helper `uses_without_trims` would return 48 for the box, failing the first assertion.
  It covers identity frames with line and circle (rational) pcurves, and points bit-equal.
- **`a_planes_pcurves_are_carried_on_any_axes`:** covers scaled, sheared, c < 0 (a turned face),
  a < 1 with b < 0, `Plane` and `PlaneThrough`, and reversed uses (each cube edge has one forward
  and one reversed use). The claim is met for line pcurves.
  - **SHOULD-FIX (coverage):** no rational or circle pcurve goes through a non-identity m. The
    cube has only lines, and `cube_on_axes` skips `RationalSpline` (`tests.rs`,
    `let Curve2::Spline .. else continue`). Add a cylinder whose cap planes are on sheared axes, so
    the homogeneous mapping (`import.rs:651-659`) is exercised.
- **NIT (weak assertion):** for `trusted = false`, `missing_trims == 0` does not show the healer
  *kept* the carried trims if the healer makes every trim anyway. Compare a trim's control points
  with the carried ones instead.
- **NIT (PR text):** "untrusted … sets the same tolerances as on main (2e-5 …)" is in the Tests
  section, but the test asserts no tolerance when untrusted (commit ac927f117b). Reword it as an
  observation.
- **`a_plane_pcurve_off_its_edge_is_made_again`:** good. It tests an end-sample failure, not a
  between-sample one, which the check cannot see by design.
- **The changed existing line (`a_cube_record_is_valid…`):** justified. The old line asserted the
  removed behaviour, and the new value (2 points per use) follows from degree-1 line pcurves. The
  `Test change:` line is present.

## 6. Performance

- **Added per load.** For each plane face, 3 `convert_uv_from_nurbs_to_step` calls. For each
  plane use:
  - a `CurveEval::new`;
  - 6 pcurve and 6 curve evaluations;
  - one `BSplineCurve` built and attached.
- **Removed.** The kernel's drop-curve fit and its certified bound per plane use, when one is first
  asked for.
- **Bounded?** Yes: linear in uses, a fixed 6 samples. On the corpus it is +0.07 s of rebuild
  against −0.14 s of trim making.
- **Smaller in every realistic case?** No. A body rebuilt and never queried pays about
  0.07 s / 464 ≈ 0.15 ms per rebuild and saves nothing.
  - With 188 bodies rebuilt, copied and dropped, every **copy** of such a body now clones the
    carried trims as well.
  - **SHOULD-FIX (measurement):** the PR's "Paths that could do more work" omits body copy and
    drop, and the memory held. Count the extra trim curves cloned per copy and the bytes held per
    rebuilt body, before and after, on those 188.
- **NIT:** the check builds `CurveEval` twice per edge (once per use). Caching per edge halves it.
- **Would a lazy carry be simple?** No. The kernel already accepts candidate trims at face-level
  creation (`opt_uv_curves`/`find_best_uv_curve`, `face/p3.rs:1331-1345`), but the per-use lazy
  path (`get_or_create_uv_trim_curve`) has no place to find a pending curve.
  - Lazy would need a kernel-side store of pending trims on the body, consulted on first fetch:
    a solidcore change across the seam.
  - Eager is the reasonable choice at this cost, provided the copy and memory numbers above are
    small.

## 7. Rules

- **Layering, SHOULD-FIX.** `along_edge` and `OnPlane::point` (`import.rs:641-692`) are a second
  implementation of the kernel's "pcurve lifts onto its edge" check (`solidcore::eval::agreement`,
  3 samples, same job). The kernel-rules skill's "One implementation" rule and layers rule 1 (dot,
  sqrt on coordinates) apply.
  - `crates/smlib` is not in `xtask/src/app_geometry.rs` CRATES (line 56), so CI will not catch
    it, and the file already does such maths in `plane()`.
  - Suggestion: expose the kernel agreement's per-use lift check (with a sample count), or add
    that primitive in solidcore and call it from ops. Keep only the frame mapping here.
  - Paths: the change adds uses of `solidcore_ops::pcurve::eval` and
    `solidcraft_geom::brep::eval::CurveEval`. These are ops and app-geom, not solidcore
    internals, so `cargo xtask layers --paths smlib` should be unchanged. That is not verified;
    run it.
- **No new flag:** met. **No panic paths:** none found in the added non-test code. All the
  `unwrap()` calls are in tests.
- **PR text.**
  - `Changes a default: no` and `Test change:` are present.
  - `Fixes #ISSUE` is a placeholder (NIT: needs a real issue).
  - SHOULD-FIX: the bullet "the plane's axes are parallel (no inverse map)" is inaccurate (§3).
    It is also the only occurrence of the word "parallel" in the change, so removing it clears the
    word scan.
  - No mention of threads, threading or concurrency was found in the code, comments, commits or
    PR body.
  - The PR should also mention the bilinear-patch behaviour change (§3) and the main-vs-branch
    mass change (§4).

## Summary table

| # | Class | Finding |
|---|---|---|
| A | SHOULD-FIX | PR does not state main-vs-branch mass change on reopen (two bodies' area moves > 1e-9); give per-field max change and count |
| B | SHOULD-FIX | "every planar use holds its trim" overclaims; explain the 10,389 plane trims still made |
| C | SHOULD-FIX | "parallel axes" fallback bullet is wrong (they are refused in `plane()`; inverse `None` unreachable); also clears the word scan |
| D | SHOULD-FIX | `plane_map(k,&surface)?` at `convert.rs:2431` adds a refusal path to every rebuild; map an error to `TrimUv::Rebuilt` |
| E | SHOULD-FIX | `along_edge` duplicates `solidcore::eval::agreement`'s lift check in an app crate; reuse or expose the kernel's |
| F | SHOULD-FIX | No test of a rational/circle pcurve through a non-identity plane map |
| G | SHOULD-FIX | Perf section omits body copy/drop and memory held for the 188 rebuilt-copied-dropped bodies |
| H | NIT | 6-point check is a sample, not a bound, for splines above degree 2; say so or restrict to degree ≤ 2 |
| I | NIT | Pin the trim's end knots to the edge's lo/hi exactly |
| J | NIT | Untrusted test: `missing_trims == 0` does not prove the healer kept the carried trims; PR claims untested 2e-5 tolerances |
| K | NIT | `CurveEval` built twice per edge; `plane_map_inverse`'s absolute 1e-12 det threshold is not scale-free (moot for the identity) |
| L | NIT | `Fixes #ISSUE` placeholder; mention the `arrays_keeping_splines` bilinear-plane trims now carried |
