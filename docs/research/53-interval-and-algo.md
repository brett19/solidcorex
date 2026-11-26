# 53. Interval arithmetic into `scx-num`, and `scx-algo`

Written for solidcorex. This covers solidcore at the pin 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (read-only at
`../sc-wt/scx-pin`) and solidcorex `main` from ad94840 onwards. Builds and tests ran on the build server.
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

## 6. Review fixes (the independent review of scx-num, up to ad94840)

Each its own commit:

1. `layers.txt` gains `[external]`, an allow-list for crates outside the workspace; solidcore may never
   be listed (0e1268c).
2. `cargo xtask headers` checks each derivation header against the pin: the named files exist and each
   file's upstream record is quoted as the pin has it (b55cb95). Files written for solidcorex are
   diffed against the pin by `bodydiff` (README review checklist, item 9: no checklist existed in the
   repository before; it is in the README now).
3. `error.rs` has a derivation header (4b1f100).
4. The difftest fingerprints an error by class, operation, `InvalidInput` reason or typed failure fields
   (floats by bits), its text, and its cause (98edfa0); the text comparison found
   `VectorVanishes` without its class prefix, fixed. Every variant's text: the unit test above.
5. The difftest's build script refuses a pin checkout at another commit or with changes (05df7d0).
6. Threshold pools and coverage: section 4; a standing rule in the README.
7. `minor_skips_row_and_column`'s second assertion restored (e6f23f0); `bodydiff` runs over tests too.
8. `LARGE_SMALL_SIZE_RATIO` is its own literal (7e963c8); a test holds the two ratios equal for now.
9. Standing steps in xtask (`pin`, `bodydiff`, `cov`, `movecheck`), documented in the README.
10. Report 52 corrected (the det3 claim, the 5e-7 entry), with a det3 test against integer determinants
    (5ef2233).
11. Section 3 and `libm_table.rs`.
12. NITs: `Changes:` lines name every differing fn; the small_matrix record; the manifest parse; `fast`
    lints the difftest and runs the xtask's tests; the README says dev, not `--release`; `.rs` files
    under `src/bin` are no longer skipped. **Left open:** whether the extents should keep public fields
    (they are public at the pin; making them private changes every caller) is a design question for the
    owner.


## 7. Heat

Interval operations per certified bound, counted at the pin with a thread-local counter on
the nine `Ivl` operations (`sqrt`, the four `*_ulp`, `+ - * /`) in a scratch clone (never
committed), over the 101 `blend::fillet_section` tests (84.6 s in the dev profile with the
counter). A bound is one outermost `gn_enclose` or `gn_enclose_over` call.

| bound | calls | min | median | p90 | max | total ops |
|---|---|---|---|---|---|---|
| `gn_enclose`, certified | 920 | 737 | 33,077 | 56,291 | 158,775 | 32,280,713 |
| `gn_enclose_over`, certified | 20 | 2,056 | 16,715 | 70,056 | 90,785 | 551,070 |
| `gn_enclose_over`, refused | 6 | 290 | 2,095 | 2,095 | 2,095 | 9,330 |

946 bounds, 32.8 million interval operations: a median of about 33,000 per bound. Time per
operation was not measured, so where within a bound the time goes is not claimed here.

## 8. Step 2: what moved into `scx-algo`

`crates/scx-algo` (layer 1, depends only on `scx-num`; `layers.txt` lists it second;
default member; instrumented by `cargo xtask cov` by default):

| scx-algo file | from (pin) | lines pin / here | tests pin / here |
|---|---|---|---|
| polynomial.rs | geom/polynomial.rs | 362 / 368 | 6 / 6 |
| local_solve1d.rs | geom/local_solve1d.rs | 1602 / 1652 | 13 / 13 |
| local_solve_nd.rs | geom/local_solve_nd.rs | 1231 / 1333 | 16 / 15 |
| error.rs | (new; section 12) | - / 203 | - |

Bit-identical: `bodydiff` finds 17 fns the same, 0 differing; the `Changes:` line is paths only.
**API:** unchanged and minimal (`Roots` with `new`, `as_slice`, `Deref<[f64]>`;
`solve_quadratic`, `solve_cubic`, `solve_quartic`). **Errors:** none (every path returns
roots, as at the pin), so no error enum yet; the solvers' enum comes with them (section 5).
**Signatures:** unchanged (`zero_tol: f64`; no kernel).

Tests: the pin's 6 unit tests; 5 property tests (`tests/properties.rs`: every root returned
is a root, at most `degree` returned, every root at least 0.05 from the others found (0.5
for the quartic), the quartic and cubic fallbacks equal the lower degree deduplicated, quartic
roots at least `zero_tol` apart); 5 difftests (`t_algo.rs`: the three solvers over the
coefficient and threshold pools, the `Roots` type, and 27,984 polynomials built from real
roots in {-3, -2, -1, -0.5, 0, 1e-9, 0.5, 1, 2, 3} with repeats, a complex pair, the constant
moved by 0, ±1e-13, ±1e-10, ±1e-7, and leading coefficients 1, -2, 1e-13, 0, at every
tolerance of the f64 pool): **0 mismatches.** Coverage of polynomial.rs under the difftest:
278 of 279 regions; the one left is `Roots::push` when four roots are already stored, which
no solver reaches (the quartic adds at most four).

**The local solvers** moved with the error design of section 12. `bodydiff`: 1d 30 fns the
same, 19 differing; nd 29 the same, 19 differing; every difference is named in `Changes:`
(the error type, `type Error` on the evaluator traits, and the tests' evaluators).
`tests::tol_scaled_zero_of_equations` is dropped (it tested a kernel tolerance helper, not the
solver); the bad-Jacobian test is rewritten over `NumError`. Behaviour is otherwise unchanged.

Difftests (`t_solve.rs`, facade in `solvers.rs`), per the owner's condition: both sides run
the same recording evaluators, whose errors are the pin's own `SolidError` (wrapped as
`XErr` here); the scx error is mapped one for one onto the pin's (`solve_pin`) and compared
with its `Display` text, kind, operation, typed failure and cause. Each run also compares
the evaluator's log (every point evaluated, in order; `signal_errors` or whether a Jacobian
was asked; whether that call failed) and the solver state after it (iteration counter,
current and two previous values, brackets, found accuracy, boundary hits, termination
reason). `*_clean`: every configuration (14 functions incl. a Newton two-cycle, a divergent
bracketed one, vanishing steps and NaN; 5 intervals incl. periodic; the 4 algorithms;
`solve_by_brent` both ways; `solve_by_bisection`; 12 equation sets incl. analytic and
finite-difference Jacobians, a degenerate interval, singular, rootless, triple root;
3 boundary handlers; step cutting; iteration limits). `*_fail`: a reduced set, failing at
each call of the clean run in turn (up to 48: first evaluation, Newton steps, oscillation
guesses, bisection and Brent at the bracket ends, the finite-difference Jacobian's
evaluations, the scaled candidate, the reevaluation), once and from there on, with each of
7 classes (Unported, Cancelled, OutOfFuel, PreparationRequired, FaceTrimsUnavailable, an
upstream code, InvalidInput): over 70,000 runs. Plus the default `evaluate`,
`compute_jacobian` direct, size mismatches, settings, `IterQueue1d`. **0 mismatches.**
Coverage under the difftest: local_solve1d 98.26% of regions, local_solve_nd 92.48%
(the rest are `?` on infallible matrix and clamp calls, and the tiny-step exits), error.rs
82%.

**Krawczyk** (`krawczyk.rs`, with `certificate::inverse4` and `station::solve4`/`smax`/`Mat`
beside it): `krawczyk`, `krawczyk_region` and `newton` are generic over the callbacks' error
`E`; `solve4`'s singular system is `NumError::failed("blend::fillet_section::station",
Singular { measure })` in place of the blend's `SectionDegenerate(NormalsAgree)`: same class,
operation and failing inputs, different `Display` text (by design: the pin's text names the
blend). Bodydiff: 22 same, 3 differ, 7 only at the pin (the unmoved `station`/`certificate`
items), 2 only here. The pin's items are `pub(crate)`, so the difftest compares against the
pin file itself, made public by `scx-difftest`'s build script (`pin_krawczyk.rs`).
`t_kraw.rs`: `inverse4`/`solve4`, the 2x2/3x3 inverses, `newton` in 2 and 3 unknowns, the
Krawczyk test in 1 and 2 unknowns, the operator's parts, Taylor-model entries; every callback
logs its calls and fails at a chosen call with the pin's own error or a `None`, so call counts,
points, boxes and the whole error are compared at each call position. 0 mismatches;
`krawczyk_region` and `solve4` fully covered.

**`hash_table`** (`HashTable`, `ObjsInVoxels`): `find_coincident_pairs` takes `&Tolerances` in
place of `&Kernel` (it read only `xsect_tol3d`); `SolidError::InvalidInput` is
`NumError::InvalidInput` with the same operation and text; the box corner is read through
`Extent3d::min()`. Bodydiff: 57 same, 5 differ, all named. `t_hash.rs`: seeded scripts of up to
160 operations (adds at quantised, often coincident points, with and without the updated
list; removes; pair searches with limit index, explicit bucket lists past the table and the
gap histogram; voxel and box queries; resizes), comparing every result, error text and the
whole model after each step; and 200-step scripts on the bare bucket table. 0 mismatches. The
uncovered regions are `?` on `place` inside `add` and defensive `break`/`continue` in `remove`
and the pair walk.

## 9. Found, not changed

1. **Interval overflow to NaN** (section 2), with the counterexample
   `Ivl::from(f64::MAX) + Ivl::from(f64::MAX)` = `[NaN, inf]`. The only operation found that
   is not an enclosure. Pinned by `an_overflowing_end_widens_to_nan_found_not_changed`.
2. **The quartic's accuracy on clustered roots.** Ferrari's method (pin and here, bit for
   bit) returns, for roots 0.00874, 0.0359, 0.0435, -0.689 (coefficients
   `[-9.389680767756892e-6, 0.0015388801638414014, -0.05843609829791384, 0.6006785459030644, 1.0]`),
   the roots 0.00959, 0.0326, 0.0459, -0.689: forward errors up to 3e-3 and a relative
   residual of 0.018. In another drawn case a root at 2.6105 came back as 2.6065 (other roots 0.04 to 0.3 away). Not an enclosure claim (the
   solver claims none), but callers (conic intersection, `geom_utility`) take the roots as
   exact; whether they polish them with Newton is for the curve layer's move to check.
   Pin call sites, read-only at the pin:
   - `solve_quartic`: one production caller, `curve/conic.rs:645`
     (`Conic::intersect_implicit_conic`, tolerance `EFF_ZERO`). **No refinement:** the roots
     are only filtered (an ellipse keeps |s| <= 1 + `EFF_ZERO` with its side shift; a
     hyperbola clamps and keeps roots inside its natural interval). Its caller
     `intersect_conic` (`conic.rs:702-708`) inverts each point onto the other conic and
     **drops** it when the distance exceeds `distance_tol`: a residual check that discards,
     never polishes. `intersect_conic` has one production caller,
     `intersect/adv_surface_intersector.rs:1734` (`EFF_ZERO_SQRT * 100`). Tests:
     `conic.rs:984`, `:987`. So a clustered case like the one above loses intersection
     points (dropped by the distance check) or keeps them up to `distance_tol` off.
   - `solve_cubic`: no production caller outside `polynomial.rs` (internal calls at
     `:201`, `:221`, `:233`, the quartic's resolvent and its fallbacks).
   - For comparison, `solve_quadratic` callers: `nurbs/b_spline_curve/p1.rs:134`,
     `curve/conic.rs:71`, `curve/ellipse/p1.rs:134`, `geom/geom_utility/p1.rs:120`,
     `blend/identify_fillet.rs:54`, `surface/surface/p2.rs:269` (not examined for
     refinement; the quadratic has no clustered-root defect found).
3. The libm dependence of section 3.

## 10. Test counts and timings (the build server, final)

| check | result | wall |
|---|---|---|
| `cargo xtask fast` (fmt, clippy x2, layers, headers, tier 1, xtask tests) | passed | 2.2 s warm |
| tier 1 scx-num | 148 unit, 11 enclosure, 10 property, 1 libm table | 0.85 + 0.94 + 0.02 + 0.01 s |
| tier 1 scx-algo | 53 unit, 5 property | 0.03 s |
| difftest (`cargo test -p scx-difftest`, dev) | 324 tests, 0 mismatches (the control mismatches as expected) | 2.7 s run |
| wasm32 (`cargo build --target wasm32-unknown-unknown -p scx-num -p scx-algo`) | builds | 0.2 s warm |
| `cargo xtask movecheck` | pin, layers, headers (50 files, 24 derived), bodydiff, cov: passed | |
| coverage under the difftest | 13,325 of 13,751 regions (96.90%), lines 97.72%; local_solve1d 98.26%, local_solve_nd 92.48%, krawczyk 98.62% | 4.0 s |

## 11. What remains for `nurbs`

`certificate.rs` (with `inverse4` to `scx-algo` beside Krawczyk), `station.rs`, the `Tay` series
and `GnAreaCells` move with `blend`/`nurbs`, which need the curve and surface types first. Krawczyk
and `hash_table` are in `scx-algo` (11-26). `tree`, `grid`, `solution_array` and `global_solver` need curve
caches, surfaces and objects, so they wait for those layers or are split there.

## 12. The local solvers' errors (decided by the owner, 2026-11-26)

Built as proposed in section 5 (decision row in `docs/04-decisions.md` section 3).
`scx_algo::error`:

- `EvalError` (trait on the evaluator's error): `ends_solve` (pin: `Unported`, `Cancelled`,
  `OutOfFuel`; used where the 1d first evaluation stops and in the nd `eval_outcome`),
  `passes_through` (those and `PreparationRequired`; the pin's `eval_failed` /
  `SolidError::geom_call`), `never_ignored` (those, `FaceTrimsUnavailable` and an
  uncertified blend; the pin's `ignored`), and `defect(operation, why)` so the default
  `evaluate` / `set_intervals` bodies return what the pin's do.
- The bad Jacobian is **not** an evaluator class: the pin reads it from
  `Matrix::solve_linear_system`'s status (`is_bad_jacobian`, `local_solve_nd.rs:60-100`,
  applied at `:514`), which here is a layer-0 `NumError::Failed`. An evaluator that fails
  with an `SM_ERR` error is an ordinary failed evaluation in both.
- `SolveError<E>`: `InvalidInput` and `Failed { SolveFailure }` (the solver's own),
  `Num(NumError)`, `CallFailed { operation, cause: E }` (pin `Failed` with
  `GeomFailure::CallFailed { class: cause.kind() }` caused by the cause), `Eval(E)` (the
  evaluator's error unchanged). Display texts are the pin's.
- Names: the glossary (`docs/08-glossary.md`) has no solver entries, and its "cause" (a
  refusal cause) and "bound" (a certified number) mean something else, so the names follow
  the pin: evaluator, evaluation, termination reason, bracket, call failed. The field
  `cause` of `CallFailed` is the pin's `source` of a call failure, not a refusal cause.
- `NotCertified` (an uncertified blend) cannot be built outside the pin crate
  (`SolidError::blend` is `pub(crate)`), so the difftest drives the other classes; its
  `never_ignored` row is checked by reading `ignored` (`error.rs`), not by a run.

### Evaluator state (pin, all 27 production impls; 19 more are in test modules)

Every evaluator holding a kernel holds `&Kernel` (shared). Whether evaluation writes the
kernel's `caches` through interior mutability was not checked.

| state kept between calls | evaluators (pin file:line) |
|---|---|
| none (could be `&self`) | `FindStepOff` topology/edgeuse/p1.rs:1616; `FindCurveDistanceExtrema` curve/curve_cache.rs:1620; `CurvePropertyEfo` curve/curve/p4.rs:1389; `CpDistAtEnfo` p4.rs:2226; `FindSilSingEfo` intersect/surface_silhouette.rs:1192; `FindSingularParamWithGivenTangentDir` surface/surface/p1.rs:4449; `FindPsNormalEnfo` surface/surface/p3.rs:2064; `PolyValue` surface/step_window.rs:1264; `FindPvIntersect` surface/volume/p1.rs:1635; `CsNormalizeEnfo`, `SspIntersectEnfo`, `SsiNormalizeEnfo`, `SsNormalizeEnfo` surface/surface/p4.rs:292, 438, 583, 725; `CcTangentIntEnfo` curve/curve/p3.rs:253 |
| last foot point / last evaluation, read as the next seed or the answer | `ClippedRadiusFn` blend/fillet_solver/p3.rs:822 (`uv`, `other_uv`, `guess_t`, last f, f', found); `SssIntersectEnfo` surface/surface/p3.rs:2181 (last 3 surface points and tangents, read as the solution) |
| a plane refit to the last solution | `SphereSolveFn` fillet_solver/p3.rs:840; `SphereEval` fillet_solver/p2.rs:287; `SphereEquations` fillet_standard_solver/p2.rs:1354; `GuessSphereEnfo` fillet_standard_solver/p2.rs:1069 |
| counters, sticky flags, last residual | `FindPcExtremaEfo` curve/curve/p3.rs:1040 (scale chosen on the first call); `FindCcNormalEnfo` p3.rs:238 (`count`, `step_back`); `CsIntersectEnfo` surface/surface/p4.rs:168 (`count`, `do_minimize`, `last_sum`); `SssIntersectEnfo` (`count`) |
| `solution_size` / `scaled_zero`, and the intervals from `set_intervals` | `CcIntersectEnfo` curve/curve/p3.rs:202; `BestLineFitEnfo` p3.rs:1259 |
| a cache written through `&mut` to outside data | the 7 fillet ones (`ClippedRadiusFn`, the sphere four, `RailUvEval` p2.rs:295, `RailRailEval` p2.rs:303): offset distances and base-surface extensions on the solver's offset surfaces (`&mut FilletSolver` / `&mut dyn FilletSolverOps`) |

For a later read-only compute phase: the 14 stateless ones can be shared; the counter,
plane and last-point ones are cheap to clone per worker (their state is per solve); the
seven fillet ones cannot, since they write the fillet solver's offset caches, so each
worker needs its own fillet solver (or those caches move behind a per-worker store).

