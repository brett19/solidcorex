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
| `nurbs` | `Curve<D>` and `Surface<D>`: exact B-spline and NURBS evaluation with any number of derivatives by two methods, knot insertion, degree elevation, splitting (curve, and surface along `u` or `v`), blossoms | exact |
| `analytic` | Line, circle, ellipse, plane, cylinder, cone, sphere and torus: points and derivatives of any order, and closest points | double-double; exact for the line and the plane |
| `quadric` | `Implicit`: the plane, cylinder, cone, sphere and torus as exact implicit equations in their own frames; `check`, an exact decision whether a rational surface lies on one, with the first-order distance where it does not; `Layout`, the closed-form map between NURB and STEP parameters (section 9) | exact decision; double-double map |
| `bound` | `Bound`: a closed interval of `Rat`, rounded outward; certified `sqrt`, reciprocal, `sin_cos` and `tan` | certified |
| `poly` | `Poly`: polynomials over `Rat`; square-free parts (Yun), Sturm sequences, every real root isolated to a width | exact |
| `project` | All the stationary points of the distance from a point to an ellipse (T1), a torus or a cone (T3), each enclosed, with their kinds and the certified nearest | certified |
| `sample` | Seeded generators: knots (uniform, random, clustered within ulps, repeated), weights (unit, moderate, near zero), points, curves, surfaces, parameters on and beside knots, frames, near-tangent pairs | |
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
- **The ellipse has no Newton closest point (JUDGED).** Newton's method from a poor start can
  converge to the wrong foot, and a reference that is sometimes wrong is worse than none. The
  certified foot is in `project` (section 4a).

## 4a. Projection truths: T1 and T3 (research 63 §8)

`project` answers, for a point `q`, every stationary point of the distance to the shape, each with an
enclosure, and which of them is the global nearest. The answer is a `Projection<P>`:

- `loci`: each a `Locus::Point(Stationary<P>)` (a parameter, the point, a certified distance
  `Bound`, and a `Kind`: `Min`, `Saddle`, `Max`, `Flat` or `Apex`) or a `Locus::Circle` (a whole
  circle of stationary points, for a query on an axis of symmetry).
- `nearest`: `Nearest::Unique(i)`, or `Nearest::Tied(is)` for loci whose distance intervals still
  meet at the last precision tried.

Parameters are `Enclosed { mid: Dd, radius }`. Points are doubles with a radius. Distances are
`Bound`s: exact rationals rounded outward, so a distance is never wrong, only wide.

**T1, the ellipse** (`Ellipse::stationary`). With `s = tan(t/2)` the point is
`V(s) / (1 + s^2)`, `V(s) = (d - A) s^2 + 2 B s + (d + A)`, `d = C - q`, `A = a X`, `B = b Y`. The
squared distance is `N(s) / (1 + s^2)^2`, `N = |V|^2`, and its derivative has the sign of the quartic
`Q = N' (1 + s^2) - 4 s N`. Every coefficient is an exact rational (the frame's doubles are exact), so
`Q`'s real roots are isolated exactly (square-free part, Sturm sequence, bisection) to a width of
`2^-bits`. `t = pi` is `s = infinity`: it is found from the quartic of the turned-over ellipse (`A`
and `B` negated), which has a root at 0 exactly when `t = pi` is stationary. Each root's kind is the
sign change of `Q` across it, or for an exact root the lowest derivative not zero. The distance is
enclosed by `sqrt(N / (1 + s^2)^2)` over the root's interval, with the curvature bound
`E = M w^2 / 2` (`M = 2 (K^2 + (D + K) K)`, `K = |A|_1 + |B|_1`, `D = |d|_1`) added for the width
`w`. `Q` zero everywhere (a circle, the query on its axis) gives one `Locus::Circle`. It runs at
110 bits and refines to 220 and 440 when a kind or the nearest is not decided.

**T3, the torus and the cone** (`Torus::stationary`, `Cone::stationary(q, Nappes)`). Both are
surfaces of revolution, so every stationary point lies in the meridian plane through `q`. There the
torus is two circles of radius `r` centred at signed radii `+-R`, and the cone is two lines. The feet
on them have closed forms in `rho` (the distance from the axis) and `h` (the height):

- torus, circle at `c`: `w = c + sigma r (rho - c) / D`, `h_p = sigma r h / D`,
  `D^2 = (rho - c)^2 + h^2`, at distance `|D - r|` (`sigma = 1`) or `D + r` (`sigma = -1`);
- cone, generators `w = +-(r + v tan a)`: the foot `v` and the distance
  `|rho -+ r -+ h tan a| / sqrt(1 + tan^2 a)` for each.

The kind of a foot is the sign pair of the two principal terms: along the meridian (near foot a
minimum, far foot a maximum) and around the axis (a positive signed radius `w` a minimum). `rho^2`
and `h^2` are exact rationals; `rho`, `h`, `tan a` and the quotients are `Bound`s at 192 bits,
refined to 384 and 768 as for T1.

Degenerate cases:

- the query on the axis: the feet sweep circles (`Locus::Circle`), decided exactly from `rho^2 = 0`;
- the query on the torus tube's centre circle: the whole meridian circle is stationary (`Min` for a
  ring torus, `Flat` for a horn or spindle), decided exactly;
- the cone's apex, and the query at the apex (distance exactly 0);
- horn and spindle tori: the pinches on the axis (`h = +-sqrt(r^2 - R^2)`, one at the centre for a
  horn), and a foot exactly on the axis (decided exactly, `on_axis_exactly`), which is a pinch.

`Nappes::One` is the cone from its apex through the reference circle (STEP's and the pin's);
`Nappes::Both` adds the other.

**Decisions (JUDGED, for review).**

1. **The ideal surface of revolution.** T3 takes the shape to be exactly rotationally symmetric about
   `(O, z)`, `z` normalised, rather than the parameterisation with the frame's rounded `x` and `y`
   as given (section 4's rule for the analytic primitives). The closed forms need the symmetry, and
   with them every degenerate case is decided exactly. The two shapes differ by the frame's
   rounding (about 1e-16 relative), which is far below any library tolerance; the tests check the
   points against the parameterisation to 1e-13 in rotated frames, and to 1e-27 in exactly
   orthonormal ones (signed permutations). The angle `u` is measured from the radial part of `q - O`,
   which is exactly square to `z`. T1 uses the ellipse's frame as given: its quartic is exact for
   any frame.
2. **Parameter radii include the double-double function error.** `u` and `v` come from `dd::atan2`
   on enclosed values: their radius is the enclosure's propagated width plus `1e-29` for the function.
   Points and distances are certified without it, from the exact forms.
3. **`Tied` means not separated.** Loci whose distance intervals still meet at the last precision
   (440 or 768 bits) are reported tied, not ordered by midpoint. Real ties (the ends of an ellipse's
   minor axis from its centre) are tied for this reason; nothing is claimed about near ties.
4. **Singular points are listed always.** The cone's apex and a horn or spindle torus's pinches are
   listed as `Kind::Apex` whatever the query, as candidates for the nearest: the distance is not
   differentiable there, and a one-nappe cone's nearest point can be its apex. A caller that wants
   only smooth stationary points filters them out. A foot that lands exactly on a pinch is that
   pinch, listed once.
5. **Flat meridian circle.** On a horn or spindle torus the tube's centre circle meets the axis, so
   the meridian circle at distance `r` is stationary but not a minimum in every direction; it is
   `Flat` there and `Min` on a ring torus.

## 5. Self-tests

`cargo test -p scx-testkit` runs 39 integration tests in 5 files plus 3 unit tests (MEASURED). The
`nurbs.rs` binary takes about 11.5 s of wall time with a warm build. About 7 s of that is the existing
surface insertion and elevation test, which took the same time before this change. The new tests take 0.3 to 1.3 s each. The
T8 and T4 tests are listed in section 9.
T1 and T3: `tests/project.rs`
takes about 0.5 s; each of its tests is under 0.7 s alone.

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
- **`tests/project.rs`** (T1, T3) and the `poly` unit tests.
  - Certified `sin_cos` holds the double-double values to 1e-30, with `sin^2 + cos^2` holding 1;
    `tan` at the double below `pi/4` is `1 + 2 (x - pi/4)` to 1e-31; square roots of squares are
    exact. Root isolation: exact dyadic roots come back as points, multiplicities are found, and the
    intervals are disjoint.
  - Ellipse: the stationary points match a dense scan (sign changes of the derivative on 2,048
    samples, then bisection) to 1e-14 in angle, with as many minima as maxima, and no sample of 512
    is nearer than the nearest. The nearest agrees with Eberly's robust bisection (a second,
    independent method) to 1e-15 relative. With equal axes it is the circle's closed form. The
    degenerate queries (the centre, with its tie and the exact `t = pi`; inside the evolute's cusp;
    on a circle's axis) and queries from 1e-6 to 1e-15 off the evolute, where two stationary points
    nearly meet, stay certified.
- **`tests/ellipse_foot.rs`** (T1, against facts that do not use the quartic; 0.3 s in debug).
  - The residual `(q - C(t)) . C'(t)`, enclosed by certified `sin_cos` on the frame as given, has
    opposite certified signs at the two ends of every simple foot's angle enclosure, in the direction
    its kind says, so a true foot lies inside it: 16 random queries and 12 near the evolute, in the
    plane and lifted off it.
  - An exact rational evolute point, `(9, -32)` for `a = 75, b = 50`, gives one `Flat` foot at
    `t = 2 atan(1/2)`, the exact point `(45, 40)` at distance `sqrt(6480)`, with one residual sign on
    both sides; moved `2^-20` either way it splits into two simple feet or goes.
  - Queries mirrored in `x`, `y` or the plane give the mirrored feet (`pi - t`, `-t`, `t`) with
    equal kinds and distances, and so does an exact move (a signed permutation frame at a dyadic
    origin); a query on an axis is its own mirror, its nearest pair tied.
  - The circle (`a = b`) matches its closed form (feet at `atan2(q_y, q_x)` and opposite, squared
    distances `(rho -+ r)^2 + z^2`); its centre, in the plane and above it, is one `Locus::Circle`,
    the residual exactly zero all round.
  - In every case a 4,096-point `f64` sample finds no point nearer than the certified nearest's
    lower end, less its own rounding.
  - Torus (ring, horn, spindle) and cone, in exactly orthonormal frames: each stationary point has a
    gradient of the squared distance under 1e-27 and a Hessian with its kind's signs, the Morse count
    `#min - #saddle + #max = 0` holds on a ring torus, and no sample of a 40 by 40 grid is nearer.
    The ring torus's nearest agrees with the double-double Newton foot (a second method). In rotated
    frames the points agree with the parameterisation to 1e-13.
  - Closed-form limits: a torus with `R = 2^-30` is the sphere (`||q - O| - r|` to `2 R`); a cone with
    `a = 2^-30` is the cylinder (`|rho - 1|` to `(|h| + 1) a`, and `|rho - 1 - h a|` to 1e-14).
  - The degenerate queries: on the axis, the tube's centre circle, a horn's centre, a spindle's pinch
    (exactly 0), the apex (exactly 0), on the axis above and below the apex for one nappe and both.
- **`tests/project_t3.rs`** (T3, against facts that do not use its closed forms).
  - Certified orthogonality: at every foot, the implicit equation of the ideal surface of
    revolution and the cross product of the residual with its gradient are evaluated in interval
    arithmetic over the foot's certified box. Both hold zero and are narrow (width under 1e-24 of
    the scale), in exact and rotated frames, for ring, horn and spindle tori and both nappe choices
    of the cone; a one-nappe foot is on the nappe. Circles of feet are checked at two opposite
    points, and singular points (apex, pinches) are checked on the surface.
  - Symmetry: a quarter turn of the query about the axis, a mirror in a meridian plane, and (torus)
    a mirror in the equator leave the sorted distance enclosures and kinds bit for bit; points move
    with the query, `u` by the turn, `v` fixed (negated by the equator). A turn by any angle,
    rounded, agrees to 1e-13.
  - Dense sampling: no point of a 73 by 73 grid over the whole parameter range (the admitted nappe
    only, for one nappe), nor of a fine grid round each minimum, is nearer than the certified
    nearest's lower end less 1e-26, nor nearer than that minimum; random, axis and equator queries.
  - Axis and apex: an axis query gives a ring torus exactly two circles (a minimum and a maximum,
    the near one at `sqrt(h^2 + R^2) - r`), a spindle its circles and both pinches, a cone the apex
    and a circle at `|h| sin a` (none below the apex for one nappe); the apex itself gives distance
    exactly zero, and a query `2^-40` below it inside the dual cone has the apex nearest.
- **`tests/sample.rs`.**
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

The projection truths (optimised build, 300 calls each): ellipse `stationary`, 4.6 ms (the same within
1e-9 of the evolute); torus, 1.2 ms (ring) to 1.7 ms (spindle); cone, 1.6 ms. A tier-1 test can afford a
hundred or so.

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
- Knot removal is not provided. It is the inverse of insertion, and a library's removal can be checked
  by inserting again with the references.
- There is no exact on-surface check for surfaces of revolution with a general profile, or for offsets.
  `quadric` covers the five analytic kinds only.

- Intersection truths (63's T2 and T5) are in `curve_surface` and `ssi`: research 64.

## 8. Decisions taken here (for review)

1. `scx-testkit` is a dev-dependency only, unpublished, checked by `xtask layers` (section 1).
2. The double-double references and the big integers moved here, and their old copies were removed.
3. Library lints apply to the kit's library code (no indexing or `unwrap`), and the kit is optimised in
   dev builds.
4. `Side` is a required argument at every evaluation. Outside the domain the end pieces continue.
5. Insertion uses Boehm's algorithm, checked against blossoms. Elevation uses the blossom formula,
   checked against the Bernstein formula and shape equality. Elevation refuses unclamped ends.
6. Analytic frames are used as given. Closest points use double-double Newton steps from closed-form
   starts. The ellipse Newton closest point is left out.
7. Splitting, insertion and elevation of a surface along one direction are each one method, and the
   direction is a private `Along` enum, not a flag (section 9).
8. The on-quadric decision is exact and needs positive weights. The pin's evaluation and map are
   checked against the closed form only to rounding (section 9).
9. The projection truths (section 4a): T3 on the ideal surface of revolution; parameter radii with
   the function error; `Tied` as not separated; singular points always listed; the flat meridian
   circle of a horn or spindle torus.

## 9. T4 and T8 (added 11-27)

### T8: higher-order surface derivatives and the tensor-product operations

New in `nurbs`: `Curve::split`, `Surface::split_u` and `Surface::split_v`. Each returns the two
clamped halves at a parameter inside the domain, by inserting it to full multiplicity. There is also
`Surface::from_homogeneous`, the surface form of the curve constructor of the same name. Insertion,
elevation and splitting along `u` or `v` share one private implementation. It applies the curve
operation to each row or column of the homogeneous net, with the direction as a private
`Along { U, V }`.

The new tests in `tests/nurbs.rs` are below. All are seeded, and all agree exactly.

| Test | What it proves | Time |
|---|---|---|
| `surface_de_boor_and_basis_sum_agree_exactly_to_order_five` | De Boor and the basis sum agree to order 5 in each direction, on both sides, on 12 surfaces of degree up to 4 | 1.1 s |
| `high_order_partials_are_the_power_basis_truth_before_and_after_insertion` | Partials to order `min(p + q + 1, 6)` equal an independent truth, before and after one insertion in each direction. The truth comes from each span's power-basis expansion, and the quotient rule by Leibniz. The orders above the degree come out exactly zero in the numerator | 0.6 s |
| `tensor_product_insertion_elevation_and_split_are_the_curve_operations_per_line` | Each surface operation equals the curve operation on each row or column | 1.3 s |
| `tensor_product_operations_commute_across_directions` | Operations along `u` and along `v` commute exactly | 1.3 s |
| `split_halves_are_the_surface_on_each_side` | Each half of a split equals the surface on its side, to order 2 | 0.6 s |
| `curve_split_halves_are_the_curve_on_each_side` | The same for curves | 0.3 s |

Three of the tests take just over the 1 s target. They were kept at this size so that every degree
up to 4 and every knot kind is still covered (JUDGED).

### T4: rational images of the analytic surfaces

**The decision.** Each kind is written as an exact polynomial `F` in local coordinates
`L = M^-1 (P - O)`. The frame `M` is used as given: it may be non-orthonormal, with integer entries,
and it is inverted exactly. The polynomials are:

| Kind | `F(L)` | Degree `k` |
|---|---|---|
| Plane | `z` | 1 |
| Cylinder | `x^2 + y^2 - r^2` | 2 |
| Cone | `x^2 + y^2 - (r + s z)^2` | 2 |
| Sphere | `x^2 + y^2 + z^2 - r^2` | 2 |
| Torus | `(x^2 + y^2 + z^2 + R^2 - r^2)^2 - 4 R^2 (x^2 + y^2)` | 4 |

On one span patch, a rational surface `P = N / w` of degrees `(p, q)` gives `w^k F(N / w)`, which is
a polynomial of degrees `(kp, kq)`. It is zero everywhere if and only if it is zero on a grid of
`(kp + 1)(kq + 1)` distinct points. `check` evaluates it exactly on that grid of interior points,
on every non-empty span patch. The weights must be positive (otherwise `F(P)` and `w^k F(P)` can
differ in where they are zero), and a span too narrow for distinct doubles is refused (`NarrowSpan`).
`Verdict` gives the number of samples, the number off, and the worst first-order distance
`|F| / |grad F|`, with `grad_P = M^-T grad_L`.

The self-tests in `tests/quadric.rs` (10 tests, 0.6 s for the slowest) show the following:

- Exact rational images of every kind are on it. These use weights that are powers of two, and
  non-orthonormal integer frames. They stay on it after insertion, elevation and splitting.
- A one-ulp change to one control point or one weight is off.
- The standard 9-point circle with its weight `cos 45°` rounded is off by about that rounding. This is
  why the pin's NURBs are never exactly on their kind, and why the difftest compares a distance to a
  tolerance.
- Bad inputs are errors.

**The map.** The pin's arcs are of the standard form. A sweep `Δ` is cut into `n` equal arcs: 1 if
under 91°, 2 if under 181°, 3 if under 271°, and 4 otherwise. Each arc spans one knot interval, with
end weights 1 and middle weight `cos(h)`, where `h = Δ / 2n`. Within arc `i`, the NURB parameter `s`
in `[0, 1]` gives the angle

    φ = start + 2ih + h + 2 atan((2s - 1) tan(h / 2))

and the inverse is `s = (1 + tan((φ_local - h) / 2) / tan(h / 2)) / 2`. Straight directions are linear.
`Param` holds one direction (`Linear` or `Arcs`, with the angles in double-double, so that a full turn
is exact). `Layout` holds both, with an `Order` of `Same` or `Swapped`. The tests show that the map
is the arc's own angle to 1e-28 against an independent double-double evaluation, and that it round-trips.
They also show that the closed-form point satisfies the exact equation to 1e-29.

### The pin against T4

`crates/scx-difftest/tests/diff/pin_quadric.rs` builds 54 analytic surfaces in the pin: 6 planes, 12
spheres, 12 tori, and 24 cones and cylinders. They cover full and partial sweeps, and every
combination of `swap_uv` and `inside_out`. For each surface, the test does the following:

- Checks the pin's `to_nurbs` with `check`.
- Finds the layout. The best fit among the two orders and the four reversals, by NURB points against
  the closed form, must fit to `1e-13` of the surface's size.
- At 24 parameters per surface, checks:
  - the pin's `convert_uv_from_nurbs_to_step` against the closed-form map, to `1e-11` degrees;
  - the pin's NURB evaluation against the closed-form point;
  - the kind's own `evaluate_step_point` at the mapped parameter, both to `1e-13` relative.

A NaN counts as a failure everywhere.

Results (MEASURED, optimised build):

| Kind | Worst distance off | Layout fit | Map | NURB point | STEP point | Failures |
|---|---|---|---|---|---|---|
| Plane | 1.8e-17 | 4.7e-17 | 1.2e-32° | 6.3e-17 | - | 0 |
| Sphere | 1.7e-16 | 4.6e-16 | 1.6e-13° | 5.3e-16 | 1.4e-16 | 0 |
| Torus | 2.3e-16 | 3.3e-16 | 1.8e-13° | 7.7e-16 | 2.3e-16 | 0 |
| Cone and cylinder | 1.9e-16 | 2.7e-16 | 9.3e-14° | 4.0e-16 | 2.1e-16 | 0 |

No pin defect was found. Every result is within rounding of the truth.

The layout the pin uses (MEASURED, from the fits):

| `swap_uv` | `inside_out` | NURB `(u, v)` against STEP |
|---|---|---|
| false | false | the same order, nothing reversed |
| true | false | swapped |
| false | true | the same order, `v` (the generator) reversed |
| true | true | swapped, with the NURB `u` (the generator) reversed |

For planes, the NURB parameters are the STEP parameters. A bounded cylinder's `u` knots are arc
lengths `r * angle`, not angles: with `r = 2`, the knot at a half turn is one ulp above `2π`. A
sphere's pole control point is about 1.5 ulp from the pole. Both are rounding, not defects.

Decisions (JUDGED):

- The decision is exact, but the pin is held only to a distance. A NURB with a rounded `cos(h)` cannot
  be exactly on its kind, so exactness would fail on rounding alone.
- The layout is found by fitting, not taken from the pin's flags. The test then records which layout
  each flag combination gives, so a change in the pin's convention shows up as a change in the table,
  not as a silent pass.
- The torus is left out of the insertion and elevation test, as its degree-4 grids make it too slow
  for the 1 s target. Its exact image is still checked.
