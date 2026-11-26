# Review: report 57, the idiom audit

Adversarial read-only review of `docs/research/57-idiom-audit.md`: its nine must-fix items
(M1-M9), its fix-when-touched list (F1-F23) and its plan edits. I checked them against the goal
(`docs/04-decisions.md` §1, 100% idiomatic Rust), against the binding rulings in
`docs/01-architecture.md` and `docs/04-decisions.md`, and against the code.

I read the report; `crates/scx-num/src` (`vector3d.rs`, `math.rs`, `transform.rs`,
`extent1d.rs`, `extent3d.rs`, `tol.rs`, `series.rs`, `interval_vector.rs`, `error.rs`,
`small_matrix.rs`); `crates/scx-algo/src` (`error.rs`, `krawczyk.rs`, `local_solve1d.rs`,
`local_solve_nd.rs`); the difftest (`crates/scx-difftest/tests/diff/`); and call sites in the
pin's `solidcore/crates/solidcore/src`. I recounted a sample of the report's numbers with grep.
I built nothing and ran nothing.

Evidence tags: READ (code or docs I read), MEASURED (a grep count), JUDGED (my reasoning).
Severity: blocking (must change before a builder starts on the item), should-fix (change the
report or plan), minor.

## Verdict: right direction, four items need reshaping first

Most of the audit holds. The counts I sampled are right, and the C++ shapes it names are real.
Four things need to change before builders start:

1. **M1 defers the one decision it cannot defer: is the scalar generic?** 01 line 74 already
   puts "evaluation generic over the scalar" in scx-geom. `series.rs` and `interval_vector.rs`
   already hold two more 3-vector types, and the audit does not count them (V1, V2).
2. **M2 says "exact agreement is kept". That holds for the column convention but not for
   merging the three inverses.** Split it into an API change now and the volume transform
   later (T1, T2).
3. **M3's replacement contradicts itself on NaN.** Its `Option<Extent>` for every fold puts a
   branch and 8 bytes into the hottest loops in the pin. Use a non-empty `Extent` plus an
   accumulator (E1, E2).
4. **The difftest claim of 0 mismatches is overstated** for M2's inverse, M3's values above
   1e20, M4's radian path and M9's Display text. Each needs a facade mapping or a stated,
   accepted drift (D1).

The audit also makes two factual errors (V3, E5) and misses one conflict with 01 (X1).

## 1. Counts (task 3)

### C1. The sampled counts match (MEASURED, no change)

I recounted 29 numbers. All of them match the report:

- In scx: `Point3` 137, `Vec3` 527, `Point2` 53, `unitize()` 26, `BIG_DOUBLE` 55,
  `INFINITE_PARAMETER` 62, `Transform` 155, `_simple(` 16, `ang_tol_deg` 8, `.component(` 9,
  `EvalFunction` 29, `EvalNFunctions` 36, `found_answer` 27, `&dyn Fn` 5, `[f64; 2]` 37,
  `non_exhaustive` 0.
- At the pin: `Point3` 7586, `Vec3` 14134, `unitize()` 752, `BIG_DOUBLE` 692,
  `INFINITE_PARAMETER` 206, `set_min_max(` 185, `is_init()` 56, `ang_tol_deg` 115,
  `.component(` 60, `is_parallel_to(` 72, `is_perpendicular_to(` 41, `parallel_sense(` 10,
  `_simple(` 404, `Transform` 559. `solve_it(` appears in 30 files.

I add counts the report needs but does not give. All are at the pin (MEASURED):

| pattern | uses | used in |
|---|---|---|
| `Extent1d::new()` | 127 | E1 |
| `Extent2d::new()` | 63 | E1 |
| `Extent3d::new()` | 131 | E1 |
| `add_point(` | 180 | E1 |
| `add_value(` | 108 | E1 |
| `expand_absolute(` | 156 | E3 |
| `from_raw(` | 133 | E3 |
| `has_negative_length(` | 13 | E3 |
| evaluators that set `found_answer = true` | 14 sites in 6 files | S1 |

The tolerance passed to `is_parallel_to` varies by call site: `EFF_ZERO_DEG` 8, a literal
`2.0` 8, `ang_tol` 7, `ANG_TOL_DEG` 5, and others. A single shared default would be wrong
(see A1).

## 2. M1: the point and vector types (task 1)

### V1. Decide the scalar question now (JUDGED from READ, blocking)

The audit defers "generic over the scalar?" to "decided with C10". But 01 line 74 already
decides it: scx-geom has evaluation generic over the scalar, and 01 §7.3 writes
`SurfaceFn ... -> Point3<T>`. The trait it names there, `JetScalar`, does not exist; the code
has `GnScalar`, `TayScalar` and `Ring`.

If `Point3` lands as a concrete f64 type and scx-geom then needs `Point3<T>`, all 137 scx uses
get touched twice, and every port in between goes to the wrong shape.

**Fix.** M1 picks one of these before the builder starts:

- **A. Concrete f64 types plus one generic triple.** `Point3`/`Vec3`/`UnitVec3` are f64, like
  glam. A single generic `V3<T>` with operator traits serves the jets and intervals. 01 §7.3
  then reads `-> V3<T>`.
- **B. A default type parameter: `Vec3<T = f64>`.** This follows the precedent of
  `HashMap<K, V, S = RandomState>`. `new` and the operators go in `impl<T: Ring> Vec3<T>`, and
  the f64-only methods (`unit`, angles, tolerant compares) go in `impl Vec3`. The f64 code still
  writes `Vec3`.

I lean to B. It gives one type, and the 01 §7.3 signature is right as written. Either option
removes V2. Also fix the trait name in 01 §7.3.

### V2. Two more 3-vector types the audit missed (READ, should-fix)

- `series.rs:554` defines `pub struct V3<T>{x, y, z}`, with C-style free functions `v_add`,
  `v_sub`, `v_scale` and `v_hull`, a constant `VZ`, and `VSer`.
- `interval_vector.rs` uses `[Ivl; 3]` with free functions `cross`, `dot`, `norm` and `unit`.

**Fix.** Add both to M1's scope as the users of whatever V1 decides. The free functions become
operator impls and methods.

### V3. `unitize` is described wrongly (READ, should-fix)

The audit says `unitize` returns `Result<()>` and mutates in place. In fact
`vector3d.rs:323` is `unitize(self) -> Result<Self>`; it consumes and returns. The real problem
is narrower: it returns the generic `Result` where `Option<UnitVec3>` fits, and its sibling
`unitize_or_z` gives a direction for the zero vector.

**Fix.** Correct the text. The proposed `unit(self) -> Option<UnitVec3>` stands.

### V4. Keep the point/vector split; add affine combination (JUDGED, should-fix)

The split is mainstream in geometry libraries: OCCT has `gp_Pnt`/`gp_Vec`/`gp_Dir`, and
nalgebra (`Point3`/`Vector3`/`Unit`), cgmath, euclid, truck and Fornjot all separate them.
Only glam uses one type. Keep the split.

What the report lacks:

- **NURBS needs weighted sums of points.** Give `Point3::affine_combination(&[(Point3, f64)])`,
  or hold control nets as `Vec3`/a homogeneous `Vec4` and say so. Otherwise de Boor code will
  convert at every step.
- **`UnitVec3` drifts under rotation.** Say whether `Affine * UnitVec3` renormalises (one sqrt,
  within 1e-12 of the pin) or uses a crate-private unchecked constructor.
- **Deref from `UnitVec3` to `Vec3`** is acceptable (nalgebra's `Unit` does it), but
  `as_vec()`/`From` is clearer. Pick one.

### V5. Smaller vector shapes the audit missed (READ, minor)

- `component(i)` returns 0.0 out of range (`vector3d.rs:630`), but `component_mut` returns a
  `Result`. M8 should cover both (see I1).
- `approx_eq` is not symmetric.
- The free functions `sqrt`/`cbrt`/`acos`/`asin` and the constants `NOZ`/`NOW` at
  `vector3d.rs:105-145` duplicate f64 methods.

**Fix.** F-tier when the file is touched. List them.

## 3. M2: transforms (task 1)

### T1. The column convention is API only; the merged inverse changes results (READ, blocking)

- `math::Transform` stores row vectors in `pub m`. `transform_point` adds the translation
  first, then the terms `p.x*tx.x + ...`.
- `transform::Transform` keeps `m` private, and column 3 holds the projective part.
- 07 §5 rule 2 already says column vectors.

Once `m` is private, the column convention means only the API's composition order: `a * b`
applies `b` first. The internal storage and the order of summation can stay as they are, so
point and vector results stay bit-equal. On that part the audit is right.

The inverses are a different matter. The code has three inverse algorithms:

1. `math.rs:1076`, `inverse()`: a certified `is_regular` check that refuses a fourth column
   other than (0,0,0,1).
2. `transform.rs`, `invert_simple`: `small_matrix::inverse_4x4`, a cofactor inverse whose
   singular test `is_zero(det)` is an absolute 1e-12 on the determinant. That test depends on
   scale.
3. `inv_transform_vector`: `inverse_3x3`, with a different singular test.

Merging them changes rounding. Near singularity it also flips the verdict, and a flipped
verdict is a shape mismatch. "Exact agreement is kept" is wrong for the inverse.

**Fix.** Make inverse 1 the `Affine::inverse`. Leave 2 and 3 with the projective transform
until it moves (T2). If the merge has to happen now, state the drift and add a facade adapter.

### T2. Split M2: the affine transform now, the volume transform at step 5 (JUDGED, blocking)

01 §10.2 puts the volume kinds at step 5 and the analytic kinds of scx-geom at step 2. The
projective `transform::Transform` (4x4 with a projective column, `new(origin, x, y, z:
Option<Vec3>)`, getters `to_origin`/`to_x_axis`, setters `set_to_*`, `concat_transform(&mut
self)`) serves volumes. Moving it now leaves it with no home crate. Converting it in place loses
its difftest cases or doubles the work.

**Fix.**

- **M2a, must-fix now.** `math::Transform` becomes `Affine` (and `Frame` where it holds a
  coordinate system), with the column API, `Mul`, and the certified inverse.
- **M2a needs a checked conversion.** Reading pin matrices needs
  `TryFrom<[[f64; 4]; 4]> for Affine`, which refuses a projective column.
- **Defer the `Rigid` type.** `to_rigid` needs a tolerance, and `is_rigid(axis_tol,
  affine_tol)` shows the pin needs two. Add it when a caller needs it.
- **M2b, defer to step 5.** The projective transform moves with the volume code and loses its
  getter/setter and `_simple` shape then.
- **Signed zero is harmless.** Any signed zero from the `0 * t` terms compares equal
  (`harness.rs:568`).

### T3. `math.rs` has two `impl Transform` blocks (READ, minor)

The second block, at `math.rs:959`, adds `rotation`, `uniform_scaling`, `reflection`,
`into_plane_frame` and `inverse`. M2a should bring them into one impl, grouped as constructors,
then operations.

## 4. M3: extents (task 1)

### E1. Use a non-empty `Extent` plus an accumulator, not `Option` everywhere (JUDGED, blocking)

The audit replaces the inside-out `new()` (1e20, -1e20) with `Option<Extent>` at every fold.
The hot paths are folds: 180 `add_point(` and 108 `add_value(` at the pin (MEASURED), in
bounding-box and hull loops. `Option<Extent3d>` grows from 48 to 56 bytes and adds a branch per
item.

The audit calls an EMPTY with ±inf "a sentinel under a new name". That is too strong. With
min = +inf and max = -inf, EMPTY is the identity of the union lattice: union and add need no
branch, and an empty test is `min > max`. parry's `Aabb::new_invalid` is a precedent. The thing
to avoid is an empty value that escapes into the API, not the identity element itself.

**Fix.** Keep `Extent` non-empty (min ≤ max) for values. Add a `Hull` accumulator:

- it starts at the ±inf identity;
- `add(p)` is a branchless min/max;
- `union` merges two hulls;
- `finish(self) -> Option<Extent>` returns `None` if nothing was added.

07 §6 rule 4 then reads "`Option<Extent>` for values that may be absent, an accumulator for
folds".

### E2. The invariant contradicts the constructors (READ, blocking)

The replacement states the invariant "neither bound is NaN, min ≤ max". It then offers
`const fn point(v) -> Self` and `translated(d)`, which pass NaN through, and it says NaN
propagates. The invariant and the constructors cannot both hold.

**Fix.** Make the invariant "not inverted" (`!(min > max)`). Then NaN takes the refusing path of
07 §6 rule 5. Or keep the NaN rule and make `point` return `Option`. Either works; the report
must pick one.

### E3. Constructors that can invert must return `Option` (READ, should-fix)

- `expand_absolute(d)` with negative d can invert the extent. It has 156 uses at the pin.
- `from_raw` stores the pair as given. It has 133 uses.
- `intersect` already returns `Err` when the extents are disjoint.
- 13 pin sites call `has_negative_length(`.

**Fix.** `shrunk(d) -> Option<Self>`, `from_bounds(min, max) -> Option<Self>`,
`intersect -> Option<Self>`. Keep the swapping `from_min_max` as a total `spanning(a, b)`.
`has_negative_length` and the exact compare in `is_init` against `BIG_DOUBLE` go away.

### E4. Moving to ±inf changes results above 1e20 (MEASURED, minor)

The pin's hull starts at ±1e20, so a value above 1e20 gives a wrong hull. ±inf fixes that, but
the result then differs from the pin. Such values are unlikely in the difftest, but the
0-mismatch claim needs that caveat (D1).

### E5. Factual slip in the example (READ, minor)

"A model 2 m long sits on 1234567": 1234567 mm is about 1.23 km, so the example is wrong as
written. Restate it in the model's units.

### E6. Defer the periodic part (JUDGED, minor)

`PeriodicExtent1d` has no consumers until scx-geom's curves. It is already F21. Take it out of
M3.

## 5. M4: angles (task 1)

### A1. Keep, but the radian path is not exact (READ, should-fix)

- 01 §4.2 removes `AngleTol`; angles are plain f64 radians.
- At the pin, `are_parallel(v1, v2, max_angle_rad)` in `tol.rs` calls
  `is_parallel_to(rad_to_deg(max_angle))`, which converts back with `deg_to_rad(a) =
  a * (PI / 180.0)`. That is a round trip from radians to degrees and back.
- A radian-native threshold differs by an ulp or two. A parallel test can flip only for an
  input within an ulp of the threshold.

**Fix.** Say "practically 0 mismatches; a flip needs an input within an ulp of the threshold".
Do not say "exact". Do not add one default tolerance: call sites pass at least five different
values (C1).

### A2. Prefer a cross-product parallel test (JUDGED, minor, defer)

`is_parallel_to` goes through `angle_between` (acos), which loses accuracy near 0 and π. That
is exactly where parallel tests happen. `|a × b| ≤ sin(tol)·|a||b|` is better conditioned.
Record it for later; it changes verdicts, so it does not belong in M4.

## 6. M5: tolerance names (task 1)

### N1. Keep, cheap; add the name pairs it missed (READ, should-fix)

`tol.rs` has near-identical names whose meanings differ:

- `zone_tol3d(res, Option<f64>)` and `zone_tol_3d(res, SmallTopoType)`, and the same pattern for
  the `xsect` and `approx` families.
- `eff_zero()` and `scaled_zero()` both return `EFF_ZERO`.
- The arity-numbered `scaled_zero2/3/4` at `tol.rs:584-667` are covered by the audit. Their
  siblings, `max2/min2/max3/min3/max4/min4` in `math.rs` (54 lines, C NaN semantics), are not.
- `is_degenerate_vec3(res, v, Option)` at `tol.rs:465` takes an option where two functions fit.

**Fix.** Add these to M5. `scaled_zero` over a max of magnitudes is monotone, so a slice or
iterator form is exact. `max4` and friends become `f64::max` folds; check that the NaN cases
match.

### N2. Decide where `Length` meets f64 (JUDGED, should-fix)

The proposal returns `Length` from the tolerance functions. That spreads `.mm()` into 228 pin
sites as they are ported. Decide before scx-geom: either `Length` at the API boundary and f64
inside the kernels, or `Length` throughout. Write the decision into M5.

## 7. M6: 1-D solver callbacks (task 1)

### S1. Keep; derivative-free methods should not require df (READ, should-fix)

The 04 row of 11-26 sets the 1-D callback to `FnMut(f64) -> Result<(f64, f64), E>`. Today
`FunctionValue {f, f_prime, found_answer}` always computes the derivative (see `eval_into` at
`local_solve1d.rs:812`). Brent and bisection would still pay for a df they never use.

**Fix.** Give Brent and bisection `FnMut(f64) -> Result<f64, E>`, and keep the pair for Newton.
This is consistent with the 04 row, which names the Newton form. Record it there.

Other notes on M6:

- Evaluator-declared convergence (`found_answer = true`) is set at 14 sites in 6 pin files
  (MEASURED). The options-held convergence test must cover each of them. List them.
- `[Domain1d; N]` cannot derive `Default` for a generic `N`. Use `[Domain1d::FULL; N]`, or
  `core::array::from_fn`.
- The `Outcome`/`Best` split is fine.

## 8. M7: n-D solver callbacks (task 2)

### K1. The proposed signature is not the current shape (READ, blocking)

Today, `krawczyk.rs:131`:

```rust
krawczyk(mid, c,
    value: &dyn Fn([Ivl; N]) -> Result<Option<[Ivl; N]>, E>,
    jacobian: &dyn Fn(..) -> Result<Option<[[Ivl; N]; N]>, E>)
```

`krawczyk_region` is at :147, and `newton(start, at: &dyn Fn, inverse: &Inverse<N>)` is at :291.

The audit's replacement has these errors:

- it drops `mid` and `c`;
- it drops the `Option` that says "no value here", which callers use;
- it adds an `inverse` argument to `krawczyk`, which belongs to `newton`.

**Fix.** Rewrite the signature from the code. Keep `mid` and `c` and the `Option`s, and use
`impl FnMut` in place of `&dyn Fn`.

### K2. Recursive callers must not take `impl FnMut` by value (JUDGED, should-fix)

`krawczyk_region` subdivides recursively. If each level takes `f: impl FnMut` and passes
`&mut f` down, the closure type nests one more `&mut` per level. Monomorphisation then never
ends and the build fails with a recursion limit error.

**Fix.** Take `f: &mut F` with a named `F: FnMut(..)` and pass `f` through.

Also say Fn or FnMut on purpose: FnMut rules out a parallel subdivision later.

### K3. The generic inverse changes rounding; use a trait (JUDGED, should-fix)

A generic Gauss `inverse<N>` replacing `inverse3` and `inverse_of_columns2/3/4` changes rounding
against the closed forms. Interval widths then differ, and so can verdicts.

**Fix.** Define `trait Invert` for `[[f64; 2]; 2]`, `[[f64; 3]; 3]` and `[[f64; 4]; 4]`, each
keeping its closed form. Bound with `where [[f64; N]; N]: Invert`, which compiles on stable.

### K4. Const N needs static dimensions at call sites (READ, minor)

The pin's fillet solver sizes its problem at run time (`ops.jacobian_size()?`, guesses built
with `vec!` of length 4). Const `N` is feasible, but each such site needs a `match` on the
dimension into const instances. Name these sites in M7.

The private `Mat<T>` with `isize` indices used by `solve4` is F-tier (see I1).

## 9. M8: out-of-range access (task 1)

### I1. Shrink to Axis indexing (READ, should-fix)

- **`Ser::at` is correct as written.** It returns 0 past its size, and `set` ignores writes
  there. For a truncated power series the coefficients past the order are zero, so this is
  the meaning, not a silent default. Document it and drop `Ser` from M8.
- **The real cases** are `Vec3::component(i)`, which gives 0.0, and `component_mut`, which
  gives `Result`. Vec2 behaves differently from Vec3, and Extent3d has a similar accessor.

**Fix.** M8 becomes `Index<Axis>`/`IndexMut<Axis>` for Vec2/Vec3 (an `Axis` enum, so no
out-of-range case exists), plus `Extent3d::min_at(Axis)`. Clippy's `indexing_slicing` does not
fire on `Index<Axis>` for a custom type. Krawczyk's private `Mat` moves to F-tier.

## 10. M9: errors (task 1)

### R1. Forward sources the idiomatic way (READ, should-fix)

In `scx-algo/src/error.rs`:

- `impl<E: Debug + Display + 'static> Error for SolveError<E> {}` gives no `source()`.
- `CallFailed`'s Display ends "caused by {cause}".
- `Eval` and `Num` show the inner error's text transparently.

The audit's fix returns the inner error from `source()` for all three. That is right for
`CallFailed`. It is wrong for the transparent variants: a reporter that walks the chain would
print the inner message twice.

**Fix.**

- `CallFailed`: `source()` returns `Some(cause)`, and Display drops "caused by".
- `Eval` and `Num`: transparent, so `source()` forwards `e.source()`. This is thiserror's
  `#[error(transparent)]` rule.
- Display text is compared in the difftest, so the facade maps the new text to the pin's (D1).

### R2. NumError is not "done right" (READ, minor)

`scx-num/src/error.rs` prints "; caused by {c}" in Display and also returns the cause from
`source()`. That reports the cause twice. Its strings are ruled to match the pin for now, so
record it as ruled residue, with the R1 rule as the end state. Do not cite it as the model.

## 11. Shapes the audit missed (task 4)

### X1. `ErrorKind` conflicts with 01 §5.1 (READ, blocking for M9's scope)

01 §5.1 fixes the kinds as InvalidInput / Unsupported / Exhausted / Cancelled / Defect.
scx-num's `ErrorKind` is InvalidInput / Degenerate / StageFailed / Failed. M9 does not mention
this.

**Fix.** M9 either maps scx-num's kinds onto the 01 set or records why layer 0 differs (the
ruled pin strings). Then a builder does not ship a third set.

### X2. Integer `code()` methods (READ, minor)

`extent1d.rs:117` and `extent3d.rs:122` keep C-style integer codes. Remove them with M3.

### X3. Three inverses, two transform impls, two extra vector types

Covered by T1, T3 and V2.

### X4. `ExtentNd::clamp_vector` (READ, minor)

The signature at `extent1d.rs:949` is:

```rust
clamp_vector(values, periodic: Option<&[bool]>, clamped: &mut Vec<f64>,
             counts: Option<&mut Vec<u64>>) -> Result<bool>
```

It has an out-parameter, an optional out-parameter, and a bool return whose meaning is not
named. Add it to F-tier with M6, since the solver domains are its callers.

## 12. The difftest claim (task 5)

### D1. "0 mismatches" holds only with stated exceptions (JUDGED from READ, should-fix)

The facade compiles one closure body against `x` and `s` and compares fingerprints within
1e-12 relative, with exact shape. 0.0 and -0.0 agree (`harness.rs:568`). There are 323 `diff!`
cases (MEASURED).

Pure renames, the column API order and the `Option` returns that the facade maps all keep 0
mismatches. Four changes can drift:

| item | change | cases at risk | handling |
|---|---|---|---|
| M2 | merged inverse | near-singular inverses (shape) | keep the certified inverse (T1) |
| M3 | hull at ±inf | values above 1e20 | accept, state it |
| M4 | radian thresholds | inputs within an ulp of a threshold | accept, state it |
| M9 | Display text | every error-text case | facade maps the text |

**Fix.** Replace "facade adapters keep 0 mismatches" with this table. A builder that sees a
mismatch then knows whether it is expected.

## 13. Plan edits against 01 and 04 (task 6)

### P1. Mostly consistent (READ, minor)

- The rule that the facade adapts to scx, rather than scx to the pin, matches 04 §1. Keep it.
- `Defect::at` as `&'static Location` is fine. Error strings at layer 0 stay ruled.
- 07 §2 has `num::Angle(f64)`. 04 has an `AngleTol` row that 01 §4.2 later reverses. The audit
  follows 01 and is right to; also mark the 04 row as superseded.
- 07 §6 rule 4 ("Option, Extent::EMPTY or typed Unset") must change as E1 says.
- 01 §7.3 needs the trait name fixed (V1).

## Verdicts per must-fix

| item | verdict | what changes |
|---|---|---|
| M1 | change | decide the scalar now (V1; I lean to `Vec3<T = f64>`); add V3<T> and `[Ivl; 3]`; fix the unitize text; add affine combination |
| M2 | change | split: M2a (affine, column API, certified inverse, `TryFrom`) now; M2b (projective transform) at step 5; defer `Rigid` |
| M3 | change | non-empty `Extent` plus `Hull` accumulator; "not inverted" invariant; `Option` for shrink, raw bounds and intersect; periodic part deferred |
| M4 | keep | say "practically 0" not "exact"; no single default tolerance |
| M5 | keep | add the name pairs and `max2..4`; decide the `Length`/f64 boundary |
| M6 | keep | derivative-free methods take `FnMut(f64) -> Result<f64, E>`; list the 14 convergence sites |
| M7 | keep | rewrite the signature from the code; `&mut F` for recursion; `Invert` trait |
| M8 | change | shrink to `Index<Axis>` and `min_at`; `Ser` stays as is; `Mat` to F-tier |
| M9 | change | transparent forwarding; resolve `ErrorKind` against 01 §5.1 |

## Build plan for the must-fix set

Builders must coordinate with whoever is editing `crates/scx-num` at the time.

**Builder A, scx-num value types, in this order:**

1. M1, with the V1 decision taken first and recorded in 04. It includes the V2 unification.
2. M8 and M4 together. Both are small and touch the same vector file.
3. M2a. It depends on M1's types.
4. M3 core. It depends on M1. M2a's `transformed_box` returns the new `Extent`, so M3 goes
   after it, or both go in one change.

**Builder B, in parallel with A:** M5, the tolerance names. It touches `tol.rs` and `math.rs`'s
`max2..4`. Rebase it after A's step 1 if `Length` reaches the vector types.

**Builder C, scx-algo, in parallel with A:**

1. M9 first. It has no dependencies, and X1 is decided with it.
2. M6 and M7 together, in one builder, since they share the solver options and error paths.
   They depend lightly on M3: the domains use `Extent1d`. Start them on today's `Extent1d`
   and rebase once A's step 4 lands.

M2b, `PeriodicExtent1d` and A2 wait for the step that needs them.

## Summary of findings

| id | severity | one line |
|---|---|---|
| V1 | blocking | M1 must decide scalar-generic now; 01 already requires it |
| T1 | blocking | merged inverse changes results; keep the certified one |
| T2 | blocking | split M2: affine now, projective transform at step 5 |
| E1 | blocking | non-empty Extent plus Hull accumulator, not Option in folds |
| E2 | blocking | M3 invariant contradicts `point`/`translated` on NaN |
| K1 | blocking | M7 signature drops mid, c, Option; misplaces inverse |
| X1 | blocking | scx-num ErrorKind conflicts with 01 §5.1 |
| V2 | should-fix | V3<T> in series.rs and `[Ivl; 3]` missed |
| V3 | should-fix | unitize returns `Result<Self>`, not in-place |
| V4 | should-fix | keep point/vector split; add affine combination, UnitVec3 drift |
| E3 | should-fix | shrink, raw bounds and intersect return Option |
| A1 | should-fix | radian path is near-exact, not exact |
| N1 | should-fix | missed name pairs, eff_zero duplicate, max2..4 |
| N2 | should-fix | decide the Length/f64 boundary |
| S1 | should-fix | derivative-free 1-D methods should not need df |
| K2 | should-fix | recursive impl FnMut blows up; use `&mut F` |
| K3 | should-fix | generic inverse changes rounding; Invert trait |
| I1 | should-fix | M8 shrinks to Axis indexing; Ser is correct |
| R1 | should-fix | transparent variants forward source, not self |
| D1 | should-fix | 0 mismatches needs four stated exceptions |
| T3, V5, E4, E5, E6, A2, K4, R2, X2, X4, P1 | minor | two impls, vector leftovers, 1e20, km slip, periodic, cross test, runtime N, NumError, codes, clamp_vector, plan text |
