# 59 - Truth references: `scx-testkit`

Written for solidcorex, 2026-11-27. Under the ruling of 11-27 (04 §1; 01 §9.2; 02 §2 to 5) results
are checked against the truth, and the pin comparison is a smoke detector. Before `scx-nurbs`,
`scx-curve` and `scx-surface` are ported, they need references to be checked against. This note
records what was built, how it checks itself, what it costs, and the decisions taken.

- **Labels:**
  - READ: read from the code or the docs.
  - MEASURED: run here; the command is given.
  - JUDGED: a design judgement.

## 1. The crate and its rule

`crates/scx-testkit`. It depends on `scx-num` only, and on no external crate.

- **The rule (layers.txt `[testkit]`).**
  - A test kit may depend on any library.
  - A library or the difftest may depend on it only as a dev-dependency.
  - Its manifest must say `publish = false`, literally: not inherited, so the rule is visible where
    it applies.
  - `cargo xtask layers` checks all three. Its unit test `a_test_kit_is_a_dev_dependency_never_published`
    covers a normal dependency on the kit, a tool on the kit, the kit on the pin, and a kit that may
    be published.
  - A live check: making `scx-testkit` a normal dependency of `scx-algo` fails with "scx-algo may not
    depend on scx-testkit (a test kit is a dev-dependency of a library or difftest crate only)"
    (MEASURED).
- **Moved into it.**
  - The big integers of `scx-difftest/tests/diff/exact.rs`: the file is deleted, and
    `pin_defects.rs` imports `scx_testkit::exact`.
  - The double-double references of `scx-num/tests/fmath.rs`. They are now `scx_testkit::dd`, and
    the `fmath` test uses them from there. The four `fmath` tests are unchanged.
- **Found on the way.** The manifest reader of `xtask` read a `[dev-dependencies.x]` or
  `[build-dependencies.x]` table twice, because `dependencies.` is inside both. This was harmless
  while every dependency was treated alike, and it now matters. Fixed, and covered by
  `deps_of_every_table_are_read`.
- **Built optimised.** `[profile.dev.package.scx-testkit] opt-level = 3`. The exact big-integer
  arithmetic takes about 117 s for the NURBS tests unoptimised and 8 to 10 s optimised (MEASURED,
  `cargo test -p scx-testkit --test nurbs`). Only this crate is affected, so every other build is
  unchanged.

Everything works on plain data: `f64` arrays, with knots and weights as slices. Nothing depends on a
geometry type or on `scx-nurbs`'s API, so a library and its reference cannot share a bug through a
shared type (JUDGED).

## 2. The modules

| Module | What | Exact or not |
|---|---|---|
| `exact` | Big integers: `Int` (sign and magnitude, schoolbook, binary gcd, division), and `ldexp`, `ratio` and `sqrt_ratio` for the difftest | exact |
| `rat` | `Rat`: `num / den * 2^exp`, kept reduced, so `==` and `Hash` are exact. Every finite double converts exactly. `to_f64` is correctly rounded (ties to even, the subnormals and the overflow included) | exact |
| `dd` | `Dd { hi, lo }`: double-double arithmetic (operator traits), and `ln`, `exp`, `sin_cos`, `atan`, `atan2` and the constants, to about 2^-100 | about 104 bits |
| `nurbs` | `Curve<D>` and `Surface<D>`: exact B-spline and NURBS evaluation with any number of derivatives by two methods, knot insertion, degree elevation, blossoms | exact |
| `analytic` | Line, circle, ellipse, plane, cylinder, cone, sphere and torus: points and derivatives of any order, and closest points | double-double; exact for the line and the plane |
| `gen` | Seeded generators: knots (uniform, random, clustered within ulps, repeated), weights (unit, moderate, near zero), points, curves, surfaces, parameters on and beside knots, frames, near-tangent pairs | |
| `data` | `CurveData` and `SurfaceData`: plain data, with `exact()` to get the reference | |

Every NURBS result is an `Exact<D> { exact: [Rat; D], rounded: [f64; D] }`. A library is checked in
one of two ways:

- against `rounded`, the one correct rounding, for a result that should be correctly rounded or within
  so many ulps of it;
- against `exact`, for an error bound in its own terms.

Surface partials come as `Partials<T>`, which is indexed `get(i, j)` for the derivative of order `i`
in `u` and `j` in `v`.

## 3. NURBS: design

- **Homogeneous form.** Each control point is held as `(w P, w)` in `Rat`.
  - Evaluation, derivatives, insertion and elevation are linear in that form, so they are exact
    without any division but the last.
  - Rational derivatives come from the homogeneous ones by the quotient rule:
    - for a curve, `C^(k) = (A^(k) - sum_{i=1..k} C(k,i) w^(i) C^(k-i)) / w`;
    - for a surface, the double sum.
- **Two independent evaluations.**
  - `derivs` uses de Boor's algorithm on the control points of each derivative curve,
    `R^(k)_j = (p - k + 1)(R^(k-1)_j - R^(k-1)_{j-1}) / (u_{j+p-k+1} - u_j)`.
  - `derivs_by_basis` sums the basis functions and their derivatives (Cox-de Boor and the derivative
    recurrence) over the control points.
  - The two share only the span choice and the quotient rule. They agree exactly on every sample
    (section 5).
- **The span and `Side` (JUDGED).** At an interior knot, the value and derivatives of a spline of
  lower continuity depend on which piece is meant, so the caller says which:
  - `Side::Below` is the piece that ends at the knot;
  - `Side::Above` is the piece that starts there.

  An argument the call always needs is part of the call, not an option with a default. Spans are
  clamped to the domain's first and last nondegenerate spans, so outside the domain the end piece
  continues (its polynomial). This is what a library's evaluator does when it is asked slightly
  outside, and it is checked by the Bernstein form at parameters in [-1, 2].
- **Insertion (Boehm).** `insert_knot(u, times)` inserts `u` after the span of `Side::Above`. The
  routine refuses in two cases:
  - `u` is outside the domain;
  - the new multiplicity would pass `p + 1`.

  It is checked independently: each new control point is the blossom of the old curve at the new
  knots `t_{i+1..=i+p}`.
- **Elevation (blossoms) (JUDGED).** In `elevate(times)`, each new control point is the average of
  the `p + 1` blossoms, with one of its `p + 1` new knots left out:
  `Q_j = 1/(p+1) sum_m b(t_{j+1}, .., t_{j+p+1} without one)`.
  - This is short and obviously right.
  - The faster form of the textbooks (split into Bezier pieces, elevate each, remove the knots
    again) has many index paths, and a reference should have few.
  - Elevation needs clamped ends (`Error::Unclamped`). An unclamped curve is clamped by insertion
    first. That is the caller's step, made explicit, not a silent one.
- **Errors.** `nurbs::Error` is an enum with one variant per reason:
  - `TooFewPoints`, `KnotCount`, `WeightCount`, `NonFinite`, `KnotsDecrease`, `Multiplicity`,
    `EmptyDomain`, `ZeroWeight` (including a weight function that is zero at the parameter),
    `OutsideDomain` and `Unclamped`.

  It implements `Display` and `std::error::Error`. There are no codes.
- **No library lints relaxed.** The crate obeys the workspace's library lints: no indexing, no
  `unwrap`, no panic. Internal impossibilities come back as `Option` and become `Error::EmptyDomain`.
  This is not ideal, but no input reaches it (JUDGED).

## 4. Analytic: design

- **Parameterisation.** Each shape is parameterised as STEP does (ISO 10303-42), and its frame vectors
  are used as given.
  - A frame of rounded doubles is not quite orthonormal. The shape it describes is the shape, not a
    rounding of an ideal one, so a library and its reference see the same shape (JUDGED).
- **One derivative rule.** Every shape is a sum of terms `f(u) g(v) V`, where `f` and `g` have the form
  `a + b t + c cos t + d sin t`, so one rule gives every derivative of every shape. The trigonometric
  derivatives cycle with period 4.
- **Parameters.** Parameters are `impl Into<Dd>`, so a closest point's double-double parameter can be
  evaluated without rounding it first.
- **Closest points.**
  - Line and plane: exact, from `t = (q - P) . D / D . D` and from the 2 by 2 normal equations.
  - Circle, cylinder, cone, sphere and torus: Newton's method in double-double on the
    parameterisation itself (the gradient of the squared distance, and its Hessian). It starts from
    the closed form for an orthonormal frame and stops at a step of 1e-31 relative.
  - `None` where the foot is not unique or the start means nothing:
    - on the axis (within 2^-50 of the scale);
    - at a torus tube's centre circle;
    - beyond a cone's apex;
    - when Newton's method does not reach a minimum.
- **The ellipse has no closest point (JUDGED).** Its foot has no closed form, and Newton's method from
  a poor start can converge to the wrong foot. A reference that is sometimes wrong is worse than none.
  An exact or certified one (the quartic's roots isolated) can be added when `scx-curve` needs it.

## 5. Self-tests

`cargo test -p scx-testkit` runs 23 tests in 4 files plus 3 unit tests, in about 10 s with a warm
build (MEASURED).

- **`tests/arith.rs`.**
  - Every `Rat` operation (`+`, `-`, `*`, `/`, the comparison and the conversion) rounds as IEEE does,
    tested on 12,000 random pairs over four exponent ranges: normal, wide, results that are
    subnormal, and results that overflow. 2,000 more cases have subnormal operands.
  - `Rat` and `Dd` agree exactly on two-term sums and products.
  - Their three-term products agree to 2^-100.
- **`tests/nurbs.rs`.**
  - A Bezier curve is its Bernstein sum exactly, rational, for degrees 1 to 5, inside and past its ends.
  - The rational quarter circle (weights 1, 1, 2) satisfies `x^2 + y^2 = 1` exactly, and its first
    and second derivatives and its curvature equal the closed forms exactly.
  - De Boor and the basis sum agree exactly to order 3, on both sides, on 4 knot kinds by 3 weight
    kinds by degrees 1 to 4 (more than 1,000 samples). The surface forms agree to order 2 on all four
    side pairs (more than 500 samples).
  - At a knot of multiplicity `m`, derivatives `0..=p-m` agree across the two sides (more than 100
    joins), and a generic C1 quadratic breaks at the second derivative.
  - Knot insertion, up to the multiplicity limit, leaves every sample exactly unchanged, to order 2
    on both sides. Each new point is the blossom.
  - Elevation of a Bezier curve gives the Bernstein formula
    `Q_i = i/(p+1) P_{i-1} + (1 - i/(p+1)) P_i` exactly, in homogeneous form. Elevation by 1 or 2 of
    every generated curve leaves the shape exactly unchanged.
  - Surface insertion along `u` and `v`, and elevation along both, leave the shape exactly unchanged.
  - A surface extruded from a curve is that curve, and its `v` partials are zero.
  - Each bad input is refused with its reason, and rounding with nothing done gives the data back.
- **`tests/analytic.rs`.**
  - The identities hold to 1e-28 relative: `C'' = -(C - O)` and the 3rd and 4th derivatives of the
    circle; the ellipse; the sphere's `S_vv` and `S_uu`; the cylinder's `S_v`; the cone's `S_v`; the
    torus's `S_vv`.
  - The double-double ellipse agrees with an `fmath` evaluation to 1e-14.
  - The double-double line and plane agree with the exact ones.
  - Each closest point (circle, sphere, cylinder, torus, cone) has a residual normal to its tangents
    to 1e-26, and no point of a 48 by 48 sample (256 for the circle) is closer. For the sphere the
    distance is `||q - O| - r|`.
  - The exact line and plane residuals are exactly normal.
  - The degenerate queries give `None`.
  - The near-tangent pairs are at their gap.
- **`tests/gen.rs`.**
  - Equal seeds give equal data.
  - Every generated curve is valid and clamped.
  - The clustered, repeated and near-zero kinds really occur: each is counted to be above 100.
  - Parameters land on and beside knots.

## 6. Cost per call

MEASURED with an uncommitted timing test (optimised build, one thread, 2000 calls per curve evaluation, 200 per surface evaluation or insertion, 50 per
elevation, on the
development machine; not committed, as the rules require). The test used degree 3 in each direction,
8 by 6 control points, and `D = 3`.

| Call | Polynomial | Rational (weights 0.5 to 2) | Near-zero weights |
|---|---|---|---|
| Curve `point` | 0.2 to 0.3 ms | 0.3 ms | 0.3 ms |
| Curve `derivs` to order 2 (de Boor) | 0.5 ms | 1.4 ms | 1.5 to 1.6 ms |
| Curve `derivs_by_basis` to order 2 | 0.9 ms | 1.8 ms | 1.9 ms |
| Curve `insert_knot` once | 26 us | 48 us | 50 to 55 us |
| Curve `elevate` by 1 | 2.7 to 2.8 ms | 5.3 ms | 5.3 to 5.9 ms |
| Surface `derivs` to order 2 (de Boor) | 11 ms | 22 to 23 ms | 23 to 24 ms |
| Surface `derivs_by_basis` to order 2 | 8.6 ms | 17 to 18 ms | 19 to 20 ms |

Random and clustered knots cost about the same (the ranges above cover both). The analytic references
cost, in double-double: circle `derivs` to order 2, 1.4 us; circle `closest`, 6.8 us; sphere `derivs` to
order 2, 3.5 us; sphere `closest`, 14 us; torus `closest`, 14 us.

Use:

- The exact references cost milliseconds a call: about 0.3 to 2 ms for a curve, and 10 to 25 ms for a
  surface's second-order partials. A tier-1 test can afford hundreds of curve samples and tens of
  surface samples; the self-tests above take about 10 s in all. Sweeps of thousands belong in tier 2.
- The time probably goes mostly to reducing every intermediate `Rat` by a gcd (not profiled). Keeping sums unreduced until
  the end, or evaluating once per span in Bezier form, would be faster. Neither has been done: the
  references are fast enough for their tests and are simpler without it (JUDGED).
- The double-double analytic references cost a few microseconds and can be used freely.

## 7. What it does not do yet

- There is no exact closest point on a NURBS curve or surface. A library's projection can be checked
  through the references' derivatives (orthogonality of the residual, exactly or in double-double),
  which is what the analytic tests do.
- There is no exact or certified ellipse foot (section 4).
- Knot removal is not provided. It is the inverse of insertion, and a library's removal can be checked
  by inserting again with the references.
- Surface derivatives of order 3 and up work, but are tested only to order 2.

## 8. Decisions taken here (for review)

1. `scx-testkit` is a dev-dependency only, unpublished, checked by `xtask layers` (section 1).
2. The double-double references and the big integers moved here, and their old copies were removed.
3. Library lints apply to the kit's library code (no indexing or `unwrap`), and the kit is optimised in
   dev builds.
4. `Side` is a required argument at every evaluation. Outside the domain the end pieces continue.
5. Insertion uses Boehm's algorithm, checked against blossoms. Elevation uses the blossom formula,
   checked against the Bernstein formula and shape equality. Elevation refuses unclamped ends.
6. Analytic frames are used as given. Closest points use double-double Newton steps from closed-form
   starts. The ellipse closest point is left out.
