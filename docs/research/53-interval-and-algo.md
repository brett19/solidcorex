# 53. Interval arithmetic into `scx-num`, and `scx-algo`

Written for solidcorex. This covers solidcore at the pin 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (read-only at
`sc-wt/scx-pin`) and solidcorex `main` from 3e868cb onwards. Builds and tests ran on server2.
Trust: counts, coverage and differential results are MEASURED; soundness arguments are argued here and
tested, not machine-proved; cuts and rulings are JUDGED.

## 1. What moved (step 1: interval arithmetic)

Five files moved into `crates/scx-num`, bit-identical except for paths and the error type:

| scx-num file | from (pin) | lines pin / here | tests pin / here |
|---|---|---|---|
| interval.rs | blend/fillet_section/interval.rs | 607 / 617 | 6 / 6 |
| ring.rs | blend/fillet_section/tay.rs (the ring traits only) | 590 / 129 | 5 / 0 |
| taylor_model.rs | blend/fillet_section/taylor_model.rs | 553 / 573 | 3 / 3 |
| series.rs | blend/fillet_section/series.rs | 888 / 899 | 8 / 8 |
| interval_vector.rs | geom/interval_vector.rs | 46 / 57 | 0 / 0 |

`tay.rs`'s tests stay with the series `Tay` they test. **Stay in solidcore:** `certificate.rs`
(it takes B-spline curves and surfaces, the kernel and the face; it belongs with `nurbs`/`blend`,
its pure `inverse4` with `scx-algo` when Krawczyk moves) and `station.rs` (fillet stations over
faces; `blend`). The `Tay` series and `GnAreaCells` take the kernel and a surface counter (`blend`).

Error mapping (series.rs): `SolidError::InvalidInput` -> `NumError::InvalidInput`;
`BlendFailure::SectionDegenerate(VectorVanishes)` -> `NumFailure::VectorVanishes`;
`BlendFailure::Defect` -> `NumFailure::Defect`; operation strings unchanged; texts the pin's, class first.

## 2. Soundness account, and its test

Each operation's claim, and what `crates/scx-num/tests/enclosure.rs` checks it against (an oracle
written there: exact sums and products as Shewchuk expansions with Dekker/Veltkamp, no FMA;
double-double sine and cosine):

- `+ - * /`: the rounded result widened outward by `|x| 2.3e-16 + 1e-300`, more than half an ulp
  (2^-53 |x| <= 1.11e-16 |x|; below the normal range the error is under 2^-1075 < 1e-300).
  Checked at the four corners (each operation is monotone or bilinear in its operands).
- `add_ulp` .. `div_ulp`: one ulp outward of each rounded corner (round-to-nearest errs by half).
- `sqrt`: correctly rounded, then widened; checked as `lo^2 <= x <= hi^2` exactly.
- `ring::acos_s`, `sin_cos_s`: libm's result widened by 1e-15 relative. **An assumption on the
  library (about 4.5 ulp), tested, not proved.**
- Taylor models, series and interval vectors are built from the above; checked at sampled points
  against exact polynomial values.

Results: 11 enclosure tests and 10 property tests pass; undecidable double-double comparisons are counted,
never passed.

**DEFECT, found and not changed: an overflowing end widens to NaN.**
`Ivl::from(f64::MAX) + Ivl::from(f64::MAX)` is `[NaN, inf]`: the sum's lower end rounds to `inf`, and
`round_down(inf) = inf - (inf * 2.3e-16 + 1e-300) = inf - inf = NaN`. `NaN` is not a lower bound, and
`.mag()` of such an interval is NaN, which `f64::max` then drops (e.g. pin
`offset/curve_on_surfaces/analytic.rs:137`). It is the pin's behaviour bit for bit; the test
`an_overflowing_end_widens_to_nan_found_not_changed` pins it. Fix belongs with an owner ruling
(an overflowing end should become `-inf`/`+inf`).

## 3. Cross-platform: the functions that are not correctly rounded

scx-num calls these from non-test code; their results depend on the platform's libm (sqrt and
FMA are correctly rounded and excluded):

| module, function | calls (line) |
|---|---|
| extent3d `cone_bound_to` | asin 906 |
| math `is_regular` | powi 407, powi 412, log2 412 |
| math `rotation` | sin_cos 970 |
| math `length_2d` | hypot 1120 |
| math `three_point_circle_2d` | asin 1237 |
| math `arc_third_point_2d` | sin 1273, sin 1282, powi 1282 |
| ring `acos_clamped` / `sin_cos_s` / `acos_s` | acos 78; sin, cos 89, sin 125 (x2), cos 126 (x2); acos 115 (x2) |
| tol `zone_for_size` | powf 289, log10 289 |
| vector2d `rotate` | cos 209, sin 210 |
| vector3d `cbrt`, `acos`, `asin` | cbrt 123-124; acos 130, 137; asin 143, 150 |
| vector3d `rotate_vec_about_axis` | cos 784, sin 785 |
| vector3d `angle_between`, `signed_unit_angle` | acos 912, asin 917 (x2); atan2 945 |

`crates/scx-num/tests/libm_table.rs` pins a hash of each function's results over 448 fixed inputs
(pairs for the binary ones) on x86_64 Linux; on another platform it is ignored, and `--ignored` names
each function whose results differ there. Bit identity across platforms is not shown; not solved here.

## 4. Coverage and the threshold pools

`cargo xtask cov` runs the difftest under instrumentation. After the threshold pools (each threshold,
one ulp either side; zero, subnormal and EFF_ZERO-scaled transforms; the empty periodic extent; fixed
case lists for overflowing point sets, disjoint periodic spans and unit-direction triples):
**10,190 of 10,501 regions executed (97.04%)**, against 96.66% before. Of the eight branches the
review named, `is_regular` :402 and :407 and `add_interval` :269-272 now run; the rest are unreachable:

- `extent3d::meets_ray` :1078 (NaN slab): `dd` is finite and non-zero, `min`, `max`, `oo` finite, so
  `(min - oo) / dd` is finite or infinite, never NaN.
- `extent1d::contains_periodic_value` :872: `contained` implies `map < max + zero` on every path.
- `extent1d::clamp_vector` :980, :993: `clamped` and `counts` are resized to the dimension first.
- `math::centroid_and_mean_radius` :902 (a finite sum over at least one point divides to a finite
  value), :919 (each distance is under 1.35e154, as its square is finite, so the sum overflows only
  past 1e154 points), :927 (a finite total over a positive count).
- `math::three_point_circle_2d` :1234: `r <= 1e-9` needs a chord under 2e-9, so the circumcentre's
  determinant is under 8e-18 and it already returned `None` (it needs 1e-12); `r` infinite needs
  `|centre - s| > f64::MAX` while each offset component stays finite: an axis-aligned near-collinear
  triple puts the whole offset on one axis (at most `MAX`), and any other is limited by the ulp of its
  coordinates to about `|chord| 2^53 < 1e170`. Argued, not proved.

The other unexecuted regions are of three kinds: index guards on indices in range by construction
(`ok_or_else(out_of_range)?` and `let Some(..) else` in `matrix.rs`'s LU, Jacobi, SVD, transpose and
sort, `series` :523/:536, `taylor_model` :340/:348/:407); arms fixed by constants (`widening_tiny`'s and
`product_subnormal`'s `None`s, `taylor_model::mul`'s `N > TM_MAX`); and error accessors and texts no
public input reaches on the pin's side, now held to the pin's text by
`error::tests::every_variant_prints_the_pins_text` (the pin builds its failures only inside its crate).
`math::det3` and `Transform::from_matrix` are public only here (`pub(crate)` or absent at the pin)
and have unit tests, no differential counterpart.

## 5. Step 2 inventory (`scx-algo`), before anything moved

Candidates, from research 49 section 6 ("To `algo`"), with what each needs at the pin:

| pin file | lines | needs | verdict |
|---|---|---|---|
| geom/polynomial.rs | 362 | `vector3d::{acos, cbrt, sqrt}`, `EFF_ZERO_SQRT`, `PI` | **moves first**: pure, no error type |
| geom/local_solve1d.rs | 1602 | `Extent1d`, `tol::{in_tol, scaled_zero}`, `try_filled`, constants; `SolidError`, `GeomFailure::{Defect, BrentDidNotConverge, CallFailed}`, `ignored`; `TerminationReason` from local_solve_nd | moves with local_solve_nd; needs the callback error design below |
| geom/local_solve_nd.rs | 1231 | `ExtentNd`, `Matrix`, constants; `GeomFailure::{Defect, ZeroDifferenceStep}`, `is_bad_jacobian` reads the status | as above |
| geom/krawczyk.rs | 386 | `Ivl`, `Ring`, `TmN` (now in scx-num), `certificate::inverse4` (pure) | moves with `inverse4`; all items `pub(crate)` at the pin, so the difftest must compare against the pin's bodies copied, as `interval_vector` |
| geom/hash_table.rs | 1522 | `Extent3d`, `xsect_tol3d`, `Kernel` (for the tolerance only) | candidate: `&Kernel` -> `&Tolerances` |
| geom/solution_array.rs | 2722 | `geom::object`, `tree::NodeId`, extent2d domains of faces | cut: names objects and faces |
| geom/tree.rs | 2052 | `curve_cache::{BezierAux1d, BezierSpan}`, `object` | cut: curve caches (to `nurbs`/`curve`) |
| geom/grid.rs | 1686 | `Surface`, `SurfaceCache`, `PolarBox`, `PseudoBox`, `Kernel` | cut: surfaces |
| geom/global_solver.rs | 3019 | `CrvOnSurf`, `Kernel`, `CancelToken`, `UpstreamFixes`, tree, solution array | cut: curves, fixes, cancellation |

**Callbacks and mutable state.** `EvalFunction::evaluate(&mut self, ..)` (1-D) and
`EvalNFunctions::{evaluate, set_intervals}(&mut self, ..)` (N-D) are the callbacks. Mutable state is
needed for a real reason in both: kernel evaluators keep the last evaluation (a foot point, a curve
cache) to answer the next query near it and to report `solution_size` (N-D reads it after `evaluate`);
`set_intervals` hands the solver's box to the evaluator once. A `Fn`-style callback would force those
caches into `Cell`s. Kept as `&mut self`.

**Callback errors.** The solvers act on the class of an evaluator's error: `Unported`, `Cancelled` and
`OutOfFuel` pass through unchanged; others become `CallFailed` with the evaluator's error as cause;
`ignored` skips a failed sample; N-D's `is_bad_jacobian` reads the status `SM_ERR`. `scx-algo` cannot
name `SolidError` (it is above). Plan: the solvers are generic over the evaluator's error `E: AlgoCause`
(a trait with `passes_through(&self) -> bool`, `is_ignorable(&self)`, `is_bad_jacobian(&self)`), and
`AlgoError<E>` is `{ Own(own failure), Callback { operation, cause: E } }`, mapped one for one onto
the pin's `SolidError` by the difftest's facade.

## 6. Review fixes (the independent review of scx-num, up to 3e868cb)

Each its own commit:

1. `layers.txt` gains `[external]`, an allow-list for crates outside the workspace; solidcore may never
   be listed (9ffe5f7).
2. `cargo xtask headers` checks each derivation header against the pin: the named files exist and each
   file's upstream record is quoted as the pin has it (df048ab). Files written for solidcorex are
   diffed against the pin by `bodydiff` (README review checklist, item 9: no checklist existed in the
   repository before; it is in the README now).
3. `error.rs` has a derivation header (847765b).
4. The difftest fingerprints an error by class, operation, `InvalidInput` reason or typed failure fields
   (floats by bits), its text, and its cause (e2bd6d1); the text comparison found
   `VectorVanishes` without its class prefix, fixed. Every variant's text: the unit test above.
5. The difftest's build script refuses a pin checkout at another commit or with changes (15dca00).
6. Threshold pools and coverage: section 4; a standing rule in the README.
7. `minor_skips_row_and_column`'s second assertion restored (85021f5); `bodydiff` runs over tests too.
8. `LARGE_SMALL_SIZE_RATIO` is its own literal (73e961d); a test holds the two ratios equal for now.
9. Standing steps in xtask (`pin`, `bodydiff`, `cov`, `movecheck`), documented in the README.
10. Report 52 corrected (the det3 claim, the 5e-7 entry), with a det3 test against integer determinants
    (e2289f4).
11. Section 3 and `libm_table.rs`.
12. NITs: `Changes:` lines name every differing fn; the small_matrix record; the manifest parse; `fast`
    lints the difftest and runs the xtask's tests; the README says dev, not `--release`; `.rs` files
    under `src/bin` are no longer skipped. **Left open:** whether the extents should keep public fields
    (they are public at the pin; making them private changes every caller) is a design question for the
    owner.

Reviewer leftovers on server2, listed rather than removed (no `rm -rf`): `scx-review-3e868cb`
(a registered git worktree: `git worktree remove` would take it), `scx-review`,
`/tmp/scxbreak-1`.

## 7. Heat

(pending: interval operations per certified bound, measured with temporary counters in a scratch clone
of the pin, not part of any commit.)

## 8. Test counts, timings and the differential result

(final run recorded at the end of the step.)
