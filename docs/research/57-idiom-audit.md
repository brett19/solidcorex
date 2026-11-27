# 57 - Idiom audit: what scx-num, scx-algo and the plan carry over from SMLib

Written for solidcorex. The 04 row of 11-26 that turns `EvalFunction` and `EvalNFunctions` into
closures asked for this audit: "an idiom audit of the code and plan follows (research 57)". The goal
is a codebase that is idiomatic Rust throughout, with a clean modern interface. Code is
transliterated only where solidcore's structure is already idiomatic (04 §1, §3).

The scope covers:

- `crates/scx-num` (not the `fmath` and `eft` modules, which are being added separately);
- `crates/scx-algo`;
- `xtask`;
- the plan in 01 (all), 07, 08 and 09.

Nothing was edited, built or run. This report only proposes.

- **Labels:**
  - READ: read from the code or the docs.
  - MEASURED: counted with `grep -rn` (lines that match) in three places:
    - `scx` means `crates/scx-num/src` plus `crates/scx-algo/src`, without `fmath` and `eft`;
    - `dt` means `crates/scx-difftest`;
    - `pin` means the kernel crate's `src` at the pin (03e16c9ee7).
    The pattern is given with each count. A count is of lines, so a line with two uses counts once.
    Counts that use a word pattern such as `Transform\b` include doc comments.
  - JUDGED: a design judgement.
- **Ranks:**
  - **must fix before scx-geom**: scx-geom and scx-nurbs would write the old shape at hundreds or
    thousands of sites. Changing it afterwards multiplies the cost.
  - **fix when touched**: worth changing, but its consumers are later layers or it is local. Change
    it when the file or its first consumer is ported.
  - **acceptable**: kept, with the reason.

## 1. How the difftest constrains every change (READ)

Every finding below must say whether `scx-difftest` can still compare behaviour, so the mechanism
comes first.

- The difftest runs 323 `diff!` cases (MEASURED, `grep -c 'diff!('` over `tests/diff/t_*.rs`).
- Each case is one closure body. It compiles twice: once against module `x` (scx) and once against
  module `s` (the pin), both in `tests/diff/facade.rs`.
- A case therefore needs both sides to have the same paths, names and signatures. The places where
  they already differ are bridged by small adapter functions in `x`, for example `ht_find_pairs`,
  `ht_placed` and `kz_err`.
- Results are compared by fingerprint (`Bits`), within 1e-12 relative and with exact shape.
- Most fingerprints are written once for both sides by `both!` (for example
  `both!(extent1d::Extent1d => |e, o| { e.min().bits(o); e.max().bits(o); })`). Five enums are
  fingerprinted by their `Debug` text (`by_debug!`): `ExtentPointType`, `BoundaryType`, `Verdict`,
  `SmallTopoType` and `AxisRefusal`.

The consequence (JUDGED):

- **Any idiomatic change stays comparable, at a fixed price.** The price has three parts:
  1. an adapter in `x` with the pin's signature, which calls the new scx API;
  2. a `both!` split into two `impl Bits`, one per side, where the shape changed;
  3. an `x`-side map onto the pin's encoding, for example `None` written as the pin's inside-out
     pair, or radians converted to degrees at the adapter.
- None of the must-fix items needs more than these three. None loses a compared case.
- The cost runs the other way too. Because one closure body must compile on both sides, the facade
  pulls the scx API towards the pin's shape. That pull is the main reason the transliterated shapes
  below have survived. Section 7 proposes stating in 07 that the facade adapts to scx, never the
  reverse.

## 2. Must fix before scx-geom

### M1. Points are vectors: `pub type Point3 = Vec3`

- **Where:** `scx-num/src/vector3d.rs:183`, and `pub type Point2 = Vec2` at `vector2d.rs:79`.
- **SMLib pattern:** `SmPoint3d` and `SmVector3d` are one class in C++, and solidcore kept the
  alias.
- **Why it is a mistake:**
  - `p + q` on two points compiles, and so does `transform_vector(p)`. A translation applied to a
    direction is a silent error.
  - The plan already assumes distinct types:
    - 01 §1.1 gives layer 0 "vectors, points, frames";
    - 07 §5 rule 1 has `Frame { origin: Point3, x: UnitVec3, z: UnitVec3 }`;
    - 01 §7.3's `SurfaceFn` returns `Point3<T>`.
  - No `UnitVec3` exists today. `unitize` returns `Result<()>` and mutates in place.
- **Replacement:**

  ```rust
  pub struct Point3 { pub x: f64, pub y: f64, pub z: f64 }   // affine
  pub struct Vec3   { pub x: f64, pub y: f64, pub z: f64 }   // linear
  pub struct UnitVec3(Vec3);                                  // private field, |v| = 1 within EPS_REL

  impl Sub for Point3 { type Output = Vec3; }                 // p - q
  impl Add<Vec3> for Point3 { type Output = Point3; }         // p + v; no Point3 + Point3
  impl Vec3 { pub fn normalized(self) -> Option<UnitVec3>; }  // replaces unitize -> Result<()>
  impl Point3 { pub fn to_vec(self) -> Vec3; pub fn lerp(self, q: Self, t: f64) -> Self; }
  impl Deref for UnitVec3 { type Target = Vec3; }             // or as_vec()
  ```

  - Fields stay public on `Point3` and `Vec3`, since any value is valid. `UnitVec3` keeps its field
    private.
  - Whether to make them generic over the scalar (`Point3<T>`, as 01 §7.3 writes) is decided
    with C10. The point/vector split is needed either way.
- **Blast radius (MEASURED):**
  - in scx: `Point3\b` 137 lines, `Vec3\b` 527, `Point2\b` 53, `unitize()` 26;
  - in the pin's geometry areas (curve, surface, nurbs): `Point3\b` 1,768 and `Vec3\b` 5,749;
  - in the whole pin: `Point3\b` 7,586, `Vec3\b` 14,134 and `unitize()` 752.
  - scx-geom ports are where this is decided in practice.
- **Difftest:** comparable.
  - The recipes `v3`/`v2` gain a `p3` twin, and `both!(vector3d::Vec3 ..)` is copied for
    `Point3`. Only fields are read, so the fingerprints are unchanged.
  - `dt` names `Vec3` on 13 lines.

### M2. Two `Transform` types, both row-vector, one shaped like an SMLib volume

- **Where:**
  - `scx-num/src/math.rs:172`, `pub struct Transform { pub m: [[f64; 4]; 4] }`, documented as
    "upstream's row-vector 4×4 matrix (`SmTransform::m_adT`): `out = [p.x p.y p.z 1] * m`";
  - `scx-num/src/transform.rs:84`, a second `Transform` (SmTransform as an `SmVolume`).
- **SMLib patterns in `transform.rs`:**
  - two-phase construction: `new()` calls `set_canonical(&mut self, ..)` at `:235`; `init()` at
    `:225` and `init_ext()` at `:192` reset in place;
  - setters on a value type: `set_to_origin`, `set_to_x_axis`, `set_to_y_axis`, `set_to_z_axis`
    and `set_to_disp` (`:200`-`:220`);
  - `to_origin()` and `to_x_axis()` (`:163`, `:169`) are getters named like `to_` conversions;
  - `concat_transform(&mut self, other)` (`:298`) does what `Mul` would;
  - the `_simple` suffixes of the `SmVolume` virtuals: `scale_simple(scale, center: Option<Point3>)`,
    `mirror_simple -> Result<()>`, `invert_simple -> Result<()>`, `evaluate_simple`, and
    `is_curve_in_param_domain_simple` (which takes no curve). `natural_param_domain` is in this
    group too.
- **Why it is a mistake:**
  - Two types with one name give two transform algebras.
  - Both store row vectors, against 07 §5 rule 2: "Matrices are column vectors (`p' = M p`) in
    `num`. Rows exist only inside `convert` ... and carry the name `RowMajorTransform`."
  - The volume half (`evaluate_simple` with derivatives, a parameter domain) is a geometry kind.
    01 §1.3 places volume kinds in `scx-geom::volume`, not layer 0.
  - `Result<()>` plus mutation means a failed `invert_simple` leaves the caller holding a
    half-updated value, or one it must re-check.
- **Replacement:**

  ```rust
  pub struct Rigid  { rot: [[f64; 3]; 3], t: Vec3 }        // orthonormal, det +1
  pub struct Affine { m: [[f64; 3]; 3], t: Vec3 }           // column vectors: p' = m p + t
  pub struct Frame  { pub origin: Point3, pub x: UnitVec3, pub z: UnitVec3 }  // 07 §5

  impl Affine {
      pub const IDENTITY: Self;
      pub fn translation(v: Vec3) -> Self;
      pub fn scaling(s: Vec3, about: Point3) -> Self;        // replaces scale_simple(.., Option)
      pub fn mirror(plane_point: Point3, normal: UnitVec3) -> Self;
      pub fn inverse(&self) -> Option<Self>;                 // replaces invert_simple -> Result<()>
      pub fn apply(&self, p: Point3) -> Point3;
      pub fn apply_vec(&self, v: Vec3) -> Vec3;
      pub fn to_rigid(&self) -> Option<Rigid>;
  }
  impl Mul for Affine { type Output = Affine; }              // replaces concat_transform
  impl From<Rigid> for Affine {}
  impl From<Frame> for Rigid {}
  ```

  - The pin's row-major form lives only in the difftest facade, which transposes. A transpose is
    exact.
  - `transform.rs`'s volume behaviour moves to `scx-geom::volume` as the affine volume kind.
  - `math.rs`'s `transformed_box -> (Point3, Point3)` returns an extent.
    `transform_points(&mut [[f64; 3]])` becomes `apply` over `Point3` (or
    `points.iter().map(|&p| t.apply(p))`).
- **Blast radius (MEASURED):**
  - in scx: `Transform\b` 155 lines, `concat_transform(` 3, `_simple(` 16;
  - in the pin: `Transform\b` 559, with 207 in curve, surface and nurbs; `_simple(` 404, of which
    most are the volume virtuals.
- **Difftest:** comparable.
  - `both!(math::Transform ..)` and `both!(transform::Transform ..)` split. The `x` side
    fingerprints the transpose.
  - The 6-entry `TfR` recipe stays and builds a `Frame`.
  - Exact agreement is kept, because transposing and reordering a product of the same terms is
    not done. Only the storage order changes, and `apply` keeps the pin's summation order term by
    term.

### M3. Extents built on sentinels and in-place mutators

- **Where (READ):**
  - `scx-num/src/extent1d.rs:163`-`186` documents the sentinels. A fresh `Extent1d::new()` (and
    `Default`) is "inside out" (`min = BIG_DOUBLE = 1e20`, `max = -1e20`). Unbounded ends are
    `±INFINITE_PARAMETER = 1234567.0`.
  - The same file has `init()` (`:216`, a C++ reset), `is_init()` (`:551`, meaning "is empty"),
    `set_unbounded()` (`:221`), and `negate`/`translate` returning `&mut Self` (`:227`, `:235`).
  - `set_min_max`/`set_min`/`set_max` return `Result<()>` (`:344`-`:365`).
  - `extent2d.rs` and `extent3d.rs` repeat all this with u/v/w names for x/y/z: `set_u_min`,
    `set_u_interval` and `u_min` through `w_max`.
  - `extent2d.rs:376` has `scale(scale, scale_v: Option<f64>)`, a C++ default argument.
  - `periodic_extent1d.rs` adds three bool parameters: `from_parts(min, max, period,
    full_period: bool)`, `set_full_period(bool)` and `add_interval(from, to, forward: bool)`.
  - Its `set_min_max(min, max, period: Option<f64>, single_point: bool) -> Result<()>` documents:
    "As upstream, the extent is updated anyway" (`:400`-`:407`). That is an error result after a
    mutation.
- **SMLib pattern:** default construction to an "uninitialised" state, then `Init`/`IsInit`/`Set*`.
- **Why it is a mistake:**
  - An empty extent is a valid pair of floats. `mid()`, `length()` or `contains()` on it return
    numbers (1e20-scale or negative lengths), not a refusal.
  - An unbounded end is a real number, 1234567.0. A model 2 m long in parameter space sits on it.
  - 07 §6 rule 4 bans sentinels. 01 §10.1 item 3 lists `INFINITE_PARAMETER` but not the inside-out
    empty (`BIG_DOUBLE`, 55 scx lines).
- **Replacement:**

  ```rust
  /// A closed, non-empty interval: lo <= hi, neither NaN (constructors check).
  pub struct Extent1d { lo: f64, hi: f64 }
  impl Extent1d {
      pub fn new(lo: f64, hi: f64) -> Option<Self>;          // None if lo > hi or NaN
      pub const fn point(v: f64) -> Self;
      pub fn hull<I: IntoIterator<Item = f64>>(vs: I) -> Option<Self>;  // empty input -> None
      pub fn translated(self, d: f64) -> Self;               // by value, not &mut Self
      pub fn union(self, o: Self) -> Self;
      pub fn intersection(self, o: Self) -> Option<Self>;
  }
  pub struct Extent3d { lo: Point3, hi: Point3 }            // x/y/z, never u/v/w
  pub enum PeriodicExtent1d { Full { period: f64 }, Arc { start: f64, end: f64, period: f64 } }
  pub enum Sense { Forward, Backward }                       // replaces forward: bool
  ```

  - Empty is `Option<Extent1d>`. An accumulator is `Option<Extent1d>` folded with `union`, or
    `Extent1d::hull`.
  - Unbounded is a decision for 10.1 item 3:
    - either the end is `f64::INFINITY`, and every routine is checked for `inf - inf`;
    - or it is a named bound, `consts::PARAM_BOUND`, documented as the largest admissible
      parameter rather than as "no end".
    The second is cheaper and keeps the pin's numbers. Either is better than a value that reads as
    an ordinary end.
  - Mutators that can fail return a new value or an error and leave nothing half-done:
    `with_max(self, m) -> Result<Self>`.
- **Blast radius (MEASURED):**
  - in scx: `BIG_DOUBLE` 55 lines, `INFINITE_PARAMETER` 62, `Extent1d::new()` 6,
    `Extent3d::new()` 8, `.is_init()` 7, `set_min_max(` 14, the `set_[uvw]_(min|max)` setters 23,
    `set_[uvw]_interval` 8;
  - in the pin: `BIG_DOUBLE` 692, `INFINITE_PARAMETER` 206, `set_min_max(` 185, `.is_init()` 56,
    and `Extent[123]d` 3,031 in curve, surface and nurbs. `PeriodicExtent1d` appears in 5 files
    (geom, intersect, topology).
- **Difftest:** comparable.
  - `both!(extent*::Extent* ..)` splits. The `x` side writes `None` as `(1e20, -1e20)`, the pin's
    encoding.
  - The recipe `e1` keeps building from raw pairs: inside-out pairs map to `None`, the rest to
    `new`.
  - Cases about construction and reset, such as `e1_constructors` and `e1_init_unbounded`
    (`t_extent.rs:14`, `:21`), become adapter cases. They then compare "fresh accumulator then add"
    rather than the method names. 67 cases are in `t_extent.rs`.

### M4. Angles in degrees on vectors

- **Where:** `vector3d.rs:426` `is_parallel_to(self, o, ang_tol_deg: f64)`, `:434`
  `parallel_sense(self, o, ang_tol_deg) -> (bool, Option<bool>)`, and `:476`
  `is_perpendicular_to(self, o, ang_tol_deg)`.
- **Pattern:** SMLib's degree angle tolerance (07 §2 READ).
- **Why it is a mistake:**
  - It contradicts 07 §2 ("radians everywhere") and 01 §4.2 ("There is no angle tolerance").
  - `tol::are_parallel` takes radians and converts, as at the pin (07 §2 "Inconsistent").
  - `parallel_sense`'s `(bool, Option<bool>)` has three states written as two flags.
- **Replacement:**

  ```rust
  pub fn is_parallel_to(self, o: Self, max_angle: f64 /* radians */) -> bool;
  pub enum Alignment { Same, Opposite, Neither }
  pub fn alignment(self, o: Self, max_angle: f64) -> Alignment;  // replaces parallel_sense
  ```

- **Blast radius (MEASURED):**
  - in scx: `is_parallel_to(` 3 lines, `is_perpendicular_to(` 4, `parallel_sense(` 5,
    `ang_tol_deg` 8;
  - in the pin: 72, 41 and 10 of those calls, and `ang_tol_deg` 115.
- **Difftest:** comparable. The `x` adapter converts the pin's degree argument with the same
  `to_radians` the pin applies inside `are_parallel`, so agreement stays exact for the radian path.
  The degree path is within 1 ulp of the angle.

### M5. Tolerance functions: C++ default arguments, getters for constants, arity overloads

- **Where (`scx-num/src/tol.rs`):**
  - `zone_tol3d(res: Resolution, d: Option<f64>) -> f64` (`:126`), and the same for `xsect_tol3d`
    and `approx_tol3d` (`:132`, `:138`). `None` stands for `SM_USE_DEFAULT`.
  - `is_degenerate_vec3(res, v, override_xsect_tol: Option<f64>)` (`:465`).
  - `eff_zero()` (`:384`) and `scaled_zero()` (`:510`) are functions that return constants.
  - `scaled_zero2`/`3`/`4` (`:584`-`:637`) are overloads numbered by arity.
  - `tol3d(d) = d.abs()` (`:119`) and `tol_mul` (`:144`, `operator*=`).
- **Why it is a mistake:**
  - `Option<f64>` means "use the default". The caller who passes `Some(x)` has a different
    function and should call it by name.
  - Every function returns a bare `f64`, discarding the `Length`/`Resolution` newtypes 01 §4.1
    built. So `zone + param_tol` compiles.
  - Arity-numbered overloads are what C++ overloading looks like after transliteration.
- **Replacement:**

  ```rust
  impl Resolution { pub const fn zone(self) -> Length; pub fn xsect(self) -> Length; pub fn approx(self) -> Length; }
  pub fn coincident(res: Resolution, a: Accuracy, b: Accuracy) -> Length;   // 09 §3, already planned
  pub fn is_degenerate(v: Vec3, tol: Length) -> bool;                       // caller passes res.xsect() or its own
  pub const EPS_LEN: f64; pub const EPS_REL: f64;                           // constants, not fns
  pub fn scaled_zero<I: IntoIterator<Item = f64>>(magnitudes: I) -> f64;    // over max_dimension()s
  // or: impl<A: ScaledZero, B: ScaledZero> ScaledZero for (A, B), and for (A, B, C) ...
  ```

  This is 01 §10.1 item 1, which says "Re-express the `tol.rs` free functions over `Resolution`
  and `Accuracy`". It is ranked must-fix because the pin calls these functions from geometry
  hundreds of times.
- **Blast radius (MEASURED):**
  - in scx: the `zone|xsect|approx_tol3d(` calls 10 lines, `scaled_zero[234](` 11,
    `eff_zero()|scaled_zero()` 27;
  - in `dt`: 7, 3 and 11;
  - in the pin: 228, 28 and 70.
- **Difftest:** comparable. The adapters `zone_tol3d(tol, d)` and the others are kept in `x` with
  the pin's signatures, calling the new API. `t_tol.rs`'s 26 cases are unchanged.
- **Where `Length` meets `f64` (decided, review-57 N2):** `Length` at the tolerance API, `f64`
  inside the kernels. A tolerance is a `Length`, `Resolution` or `Accuracy` from where it is made
  to where it is compared: `Resolution::{zone, xsect, approx, contact, topology_zone}`,
  `coincident`, `coincidence_reach_3d`, `zone_reach_3d` and `zone_for_size` return `Length`, and
  `is_degenerate`, `map_to_1d`, `within_contact_tol` take one. Geometry (coordinates, distances,
  sizes, scaled zeros, parameter steps) stays `f64`. The one conversion is `Length::from_mm` in
  and `Length::mm` out: no `From`, no `Deref`, and `Resolution`/`Accuracy` lost their own
  `mm`/`from_mm`. A predicate converts at its comparison (`within_coincidence_tol`,
  `is_degenerate`); a ported site that feeds a tolerance into `f64` arithmetic calls `.mm()` once,
  where the arithmetic starts, so the pin's 228 sites take one `.mm()` each at most, not one per
  operation. Written into `units.rs`'s module doc.
- **Done (idiom builder B):** as above, with three choices the ruling left open. `max3`/`min3`/
  `max4`/`min4` are gone, but `max2`/`min2` stay rather than becoming `f64::max` folds: `f64::max`
  drops a NaN that C's `a > b ? a : b` keeps (`max2(1, NaN)` is NaN), so a fold would change
  results; `smos_3Max(a, b, c)` is `max2(max2(a, b), c)` to the bit, and the call sites in
  `extent3d.rs` nest it. `scaled_zero` is one iterator form (a left `max2` fold over magnitudes,
  `EFF_ZERO` for none); the sq/sqrt variants went, a caller squares or roots. The small-topology
  section and approximation tolerances fold into `Resolution::topology_zone` (no pin caller
  passes a small size class).

### M6. The solver rewrite the 04 closure row rules, with the parts it does not name

The row is ruled (04 §3, 11-26). It covers:

- the `EvalFunction`/`EvalNFunctions` traits (`local_solve1d.rs:199`, `local_solve_nd.rs:154`);
- the default `evaluate` that fails at runtime;
- the out-parameters and the `signal_errors` flag;
- the solver settings that sit on the evaluator (`solution_size`, `scaled_zero`, `is_converged`,
  `has_intervals`, `set_intervals`).

The rewrite should also remove the following, which come from the same C++ classes. All are in the
same files.

| Where | Pattern | Replacement |
|---|---|---|
| `local_solve1d.rs:188` `FunctionValue { f, f_prime, found_answer: bool }` | an out-struct with a "done" flag the evaluator sets | the closure returns `(f, df)` as ruled. "Found" is the solver's decision (the row: "no evaluator-declared convergence") |
| `local_solve1d.rs:237` `IterationValue::UNSET` (the `UNDEF_DOUBLE` 1.23456e-61 sentinel) | an unset sentinel | `Option<IterationValue>` |
| `local_solve1d.rs:349` `SolveOutcome { found: bool, t }`; `local_solve_nd.rs:294` `NdSolution { found: bool, x }` | a bool plus a value that means something else when false | `enum Outcome<T> { Root(T), Best(T) }`, or `Result<T, SolveError<E>>` with the best guess on the error |
| `local_solve1d.rs:415` `have_lower_bracket: bool` plus the separate `f`/`t` fields (and the same for upper) | parallel flags | `lower: Option<Bracket>` with `struct Bracket { t: f64, f: f64 }` |
| `local_solve1d.rs:509` `solve_it(&self, eval, guess, acceptable_accuracy)` | a setting outside the options; the name comes from `SolveIt` | `solve(&self, f: impl FnMut(f64) -> Result<(f64, f64), E>, guess: f64)`, with `acceptable_accuracy` in `LocalSolve1d` |
| `local_solve1d.rs:521` `solve_traced -> (Result, State)` | a debug twin | keep it as a test-only function, or fold it into a `Trace` sink. Not a second public entry point |
| `local_solve1d.rs:541` `solve_by_brent(eval, ax, bx, cx, tol, maximize: bool)` | a bool parameter that repeats `SolverAlgorithm` | `enum Goal { Minimize, Maximize }`, or two functions; the triple as `Bracket3 { a, b, c }` |
| `local_solve1d.rs:722` `solve_by_bisection(eval, 40, lo_t, hi_t, lo_f, hi_f)` | positional f64s | `(lo: Bracket, hi: Bracket, max_iter)` |
| `local_solve1d.rs` `Domain1d::with_interval(interval, is_periodic: bool)` | a bool picking the variant | construct the variant: `Domain1d::Periodic(e)` and `Domain1d::Bounded(e)` |
| `local_solve1d.rs:324` `IterQueue1d::is_oscillating(step)` (only `step == 2` works) and `add` silently ignoring a size-0 queue | a runtime parameter with one valid value; a silent no-op | `is_oscillating(&self)`, plus `IterQueue1d::new(NonZeroUsize)` |
| `local_solve1d.rs`, `local_solve_nd.rs:138` `code()`/`from_code()` on `SolverAlgorithm` and `TerminationReason`; `TerminationReason::{Undefined, StillIterating}` | integer status codes; states that are not outcomes | move codes into the facade (see F1); `TerminationReason` holds only how a finished solve ended |
| `local_solve_nd.rs:324` `LocalSolveNd<'a, E>` holding `&'a mut E`, setters (`set_step_cutting_enabled(bool)`, `set_desired_accuracy`, `set_maximum_iterations`, `set_boundary_handler(handler, max_hits)`) and post-solve getters (`termination_reason`, `guess_unevaluated`, `current_iteration`, `evaluator()`) | a long-lived solver object with two-phase use | `LocalSolveNd<const N>` options (`step_cutting: bool` is a field here, which is fine), `BoundaryHandler::TotalHits { max }` carrying its limit, and `solve(&self, f, guess: [f64; N]) -> Result<NdReport<N>, SolveError<E>>` returning reason and iterations |
| `local_solve_nd.rs:346` `new(evaluator, intervals: Option<&ExtentNd>, periodicities: Option<&[bool]>)`, and `extent1d.rs:949` `ExtentNd::clamp_vector(values, periodic: Option<&[bool]>, clamped: &mut Vec<f64>, counts: Option<&mut Vec<u64>>) -> Result<bool>` | parallel bool arrays; out-params; an `Option` meaning "unbounded" | `domain: [Domain1d; N]` in the options; `fn clamp(&self, x: [f64; N]) -> Clamped<N> { x, hit: [bool; N] }` |
| `local_solve_nd.rs:217` `compute_jacobian(eval, x: &mut [f64], f, jacobian: &mut Matrix, ..)` that perturbs `x` in place | an out-param, and a mutation of the caller's input | `fn jacobian<const N>(f, x: [f64; N], fx: [f64; N], domain) -> Result<[[f64; N]; N], E>` |
| `scx-algo/src/error.rs:48` `EvalError::defect(operation, why) -> Self` | it exists so trait defaults can fail at runtime | removed with the defaults |

- **Blast radius (MEASURED):**
  - in scx: `EvalFunction` 29 lines, `EvalNFunctions` 36, `found_answer` 27, `SolveOutcome` 16,
    `NdSolution` 8, `with_interval(` 9, `ExtentNd` 20, `clamp_vector(` 5, the ND setters 6,
    `LocalSolveNd::new` 13;
  - in the pin: 50, 85, 125, 31, 10, 60, 96, 5, 49 and 33. `solve_it(` appears in 30 files (8 in
    curve, 7 in surface, 7 in intersect, 4 in blend).
- **Difftest:** comparable.
  - `solvers.rs` already wraps both sides' evaluators (`XErr`, the injected error classes).
  - Its `x` evaluators become closures over the same recorders, so call sequences still compare
    one for one. 10 cases are in `t_solve.rs`.
  - `Outcome::Best` maps to the pin's `found == false`.
- **Done (idiom builder C):** as the table, with these choices the table left open.
  - **Closures.** 1-D Newton and Brent take `FnMut(f64) -> Result<(f64, f64), E>`. Brent is not
    derivative-free here: the pin's `SolveByBrent` reads `F'` to pick its step, so it keeps the
    pair. Only bisection is derivative-free (`FnMut(f64) -> Result<f64, E>`). N-D takes
    `FnMut(&[f64; N]) -> Result<([f64; N], [[f64; N]; N]), E>`, row-major.
  - **Convergence.** The evaluator's "found" (`found_answer`, `is_converged`) becomes a test in
    the options: `with_convergence(|t, f| ..)` (1-D) and `|x, f| ..` (N-D), traits `Converged1d`
    and `ConvergedNd`. The default, `Residual`, never declares a root early, which is what every
    pin evaluator that left `found_answer` false did. The pin's N-D `IsConverged` (accept a
    guess whose residual is below the desired accuracy) is a second trait method with a default
    of yes, so the default needs no setting.
  - **`SolverAlgorithm`** is `Newton` or `Brent(Goal)`. `SM_SA_BISECTION` is gone: the pin
    solves it as Newton (no branch reads it). The codes live in the difftest facade.
  - **`solve_by_brent`** returns the `F` it evaluated, not the pin's `-F` when maximising; the
    sign flip is the iteration's business.
  - **`TerminationReason`** keeps the five finished outcomes and adds `GuessUnevaluated` for the
    pin's `guess_unevaluated` flag (the pin reports it as `UnableToConverge` plus the flag);
    `Undefined` and `StillIterating` went.
  - **Brackets and iterates** are `Option<Sample>` (`Sample { t, f }`, named for what it is, not
    `Bracket`, since an evaluated point is also what bisection takes) and
    `Option<IterationValue>`; `found_accuracy` is `Option<f64>`.
  - **`solve_traced`** stays, behind `#[cfg(any(test, feature = "trace"))]`. The difftest turns
    the feature on; no other crate sees it. It returns the state only with an outcome: on an
    error there is no state to read, so the difftest compares failed solves by error and log.
  - **`IterQueue1d`** stores the steps only (it read nothing else), and `try_new` gives
    `NumError` for an allocation that fails.
  - **`jacobian(f, x, domain)`** has no `fx` argument: the pin's `ComputeJacobian` is central
    differences and never reads the value at `x`.
  - **`EvalError::defect`** went with the defaults. The solvers' tests that proved the defaults
    failed went with them (listed in the commit).

### M7. Krawczyk takes `&dyn Fn` and a `dyn` alias

- **Where:** `krawczyk.rs:131` `krawczyk`, `:147` `krawczyk_region` and `:291` `newton` take
  `&dyn Fn(..)` arguments. `:287` defines `pub type Inverse<const N: usize> = dyn Fn(&[[f64; N]; N])
  -> Option<[[f64; N]; N]>`. Both functions carry `#[allow(clippy::type_complexity)] // the closures'
  types, as at the pin`.
- **Pattern:** a C++ function pointer, kept as the pin wrote it.
- **Why it is a mistake:**
  - It sets a second closure convention beside M6's `impl FnMut`.
  - It makes an indirect call in the innermost interval loops of the procedural blend's
    certification.
  - It rules out `FnMut`, so an evaluator that memoises cannot be passed.
- **Replacement:**

  ```rust
  pub fn krawczyk<const N: usize, E>(
      f: impl FnMut(&[Ivl; N]) -> Result<[Ivl; N], E>,
      jac: impl FnMut(&[Ivl; N]) -> Result<[[Ivl; N]; N], E>,
      inverse: impl Fn(&[[f64; N]; N]) -> Option<[[f64; N]; N]>,
      region: &[Ivl; N],
  ) -> Result<Krawczyk<N>, E>;
  ```

  - The `type_complexity` allows go away, because there is no alias to write.
  - `inverse_of_columns2`/`3`/`4` become one `fn inverse<const N: usize>`, or stay as three
    implementations of one `trait Invert`. Either way there is one name.
- **Blast radius (MEASURED):**
  - in scx: `&dyn Fn` 5 lines, all in `krawczyk.rs`, and `Inverse<` 2;
  - in the pin: `krawczyk(`/`krawczyk_region(`/`newton(` 41 lines, `&dyn Fn` 314 (43 in curve,
    surface, nurbs and eval).
- **Difftest:** comparable, with no adapter change. `t_kraw.rs`'s 9 cases pass closures, which
  coerce to either form.
- **Done (idiom builder C):** `krawczyk`, `krawczyk_region` and `newton` take their callbacks by
  value as generic `FnMut`, so a caller passes `&f`, `&mut f` or a closure. The inverse is not a
  parameter: `newton` requires `[[f64; N]; N]: Invert`, a trait with the three closed-form
  inverses (sizes 2, 3, 4), each keeping its own rounding, so there is one name and no generic
  Gauss-Jordan that would round differently. `krawczyk_region` returns `ProvedRoot { root,
  region }` in place of a pair, which is what removed the last `type_complexity` allow. The
  `Inverse` alias went. The difftest's pin facade gains the same `Invert`, `newton` and
  `ProvedRoot` names over the pin's functions.

### M8. Silent fallbacks forced by `indexing_slicing = deny`

- **Where:**
  - `vector3d.rs:630` `Vec3::component(index) -> f64` returns 0.0 out of range, while
    `component_mut` returns `Result<&mut f64>`. `vector2d.rs:220` returns `Result<f64>` for the
    same read, so the two vector types disagree.
  - `series.rs:284` `Ser::at` returns 0 past the size, and `:294` `Ser::set` ignores writes past it.
  - `krawczyk.rs:370`'s private `Mat<T>` has `isize` indices, reads 0 out of bounds and skips
    writes. It is a transliterated nested vector, used by `solve4` on a fixed 4×5 system that is
    a `[[f64; 5]; 4]`.
  - `extent3d.rs:409` `min_at(i)`.
- **Pattern:** C++ `operator[]` on an unchecked array, wrapped to satisfy the lint with a value
  that cannot fail.
- **Why it is a mistake:**
  - The lint is right: library code must not panic.
  - A wrong index becoming a 0.0 is worse than a panic, though. A bug turns into a plausible
    geometric answer.
  - The fix is to make the index unable to be wrong, not to absorb it.
- **Replacement:**

  ```rust
  #[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Axis { X, Y, Z }
  impl Index<Axis> for Vec3 { type Output = f64; }      // total: no panic path
  impl IndexMut<Axis> for Vec3 {}
  impl Axis { pub const ALL: [Axis; 3]; }
  impl From<Vec3> for [f64; 3] {} impl From<[f64; 3]> for Vec3 {}  // the latter exists
  ```

  - Fixed sizes become const-generic arrays, iterated with `iter()`/`zip`, so no index exists.
  - Where a dynamic index is real (`Ser` at a runtime degree), return `Option<T>`.
- **Blast radius (MEASURED):**
  - in scx: `.component(` 9 lines;
  - in the pin: `.component(` 60 lines. The pattern also runs through every `[f64; 3]` loop in
    geometry.
- **Difftest:** comparable. The adapter `component(i)` maps `i` to an `Axis` and keeps the pin's 0.0
  for an out-of-range `i`, which only the facade then sees.

### M9. `SolveError` has no `source()`

- **Where:** `scx-algo/src/error.rs:185`, `impl<E: ..> std::error::Error for SolveError<E> {}`.
- **Why it is a mistake:**
  - 01 §5.1 says "Every error implements `std::error::Error` with `source()`".
  - `SolveError` is the pattern 01 §5.1 keeps for every solver.
  - `CallFailed { cause: E }`, `Eval(E)` and `Num(NumError)` all hold a source that is dropped.
  - `NumError` does it right (`error.rs`, `fn source` over `cause()`).
- **Replacement:** `impl<E: Error + 'static> Error for SolveError<E> { fn source(&self) -> .. {
  match self { CallFailed { cause, .. } | Eval(cause) => Some(cause), Num(e) => Some(e), _ => None
  } } }`.
- **Blast radius:** one impl. Every later crate copies it.
- **Difftest:** unaffected. Errors are compared by variant and text.

### M10. The global solver: a mutable solver object with virtual hooks

Decided 11-27 while porting `geom/global_solver.rs` and `geom/solution_array.rs` (the
pin's `SmGlobalSolver`, `SmSolution`, `SmSolutionArray`) to `scx-algo`'s `global_solve`,
`solution` and `split_tree`. Each decision below was argued for and then against; the
difftest is `scx-difftest/tests/diff/t_gsolve.rs`.

- **The C++ shapes found:**
  - `GlobalSolver` is a long-lived mutable object: `SolveIt` copies its arguments into
    fields (operation, category, tolerance, `opt_vectors`, best so far), swaps the caller's
    `SolutionArray` in and out, and keeps a recursion depth, a branch count and a cancel
    token. The hooks (`GlobalSolve`'s virtuals) read those fields.
  - The operation's vectors are `opt_vectors[i]`, indexed by convention per operation.
    `OperationCategory` is a second enum derived from the first.
  - "No best answer yet" is the sentinel `SM_BIG_DOUBLE` (1e20), reset to 0 for two of the
    three maximising operations.
  - `SolutionRequested` is a mode flag read deep inside `AddSortedSolution`.
  - `Solution` is all public fields with counts beside fixed arrays
    (`num_variables`/`parameters`, `num_objects`/`objects`).
  - `LocalSolve` returns a bool, "needs more subdivision".
- **Replacement:**
  - `solve(trees, accumulator, hooks) -> Result<Accumulator, SolveError<E>>`, a pure call.
    `SolveOptions { operation, keep, tolerance, incumbent_sq }` is the options value.
    `Accumulator` holds the options, the best so far and the sorted solutions, and is
    the only state a hook can change (`add_sorted`, `push`). The hooks are the trait
    `BranchSolver<T, O, N>`: `local_solve` is required; `may_contain`, `is_ready` and
    `split_choice` default to the module's generic tests (`may_contain`, `split_choice`,
    free functions a hook can call). Against: a hook that needs per-solve state must carry
    it itself. That is the point: the pin's hooks read solver fields that only `SolveIt`
    set.
  - `Operation` carries the vectors its generic tests read (`Rayfire { direction }`,
    `Directed* { normal, direction }`, `PerspectiveIntersect { plane_point, normal, eye }`
    and so on). Operations whose vectors only a hook reads carry none: the hook holds them.
    `Goal` replaces `OperationCategory`, as a method.
  - "No best yet" is `incumbent_sq: None`. It starts the maximising operations at 0 and
    the others at infinity. The pin started `ProjectedMaximize` at 1e20, so it returned
    no solution (pin defect `gsolve_projected_maximize`). Infinity also stops 1e20 from
    culling boxes more than 1e10 apart. The skip of a solution valued `SM_BIG_DOUBLE` is
    dropped, since no solver here makes the sentinel.
  - `Keep { All, Best, Nodes }` replaces `SolutionRequested`. `FindAmbiguities` is unused
    upstream and is left out.
  - `Next { Done, Split }` replaces the bool. A split is
    `Split { tree, order: ChildOrder }`, not `(usize, bool)`.
  - `Solution<O>` has private counts. It is built by `point`, `range` and `nodes` with
    const-generic arrays, then `with_objects`/`with_nodes`. `SolutionKind`, `EndClass`,
    `Pairing`/`Prefer` and `SortKey` are enums. The object is a trait
    (`parameter_count`, `parameter_interval`, `tolerance`) where the pin had `kind()` plus
    a dozen defaulted geometry lookups. The geometry queries of `Solution` (`point`,
    `point_of`) and the `SideRecord` stay with the crates that own the geometry.
  - The traversal is an explicit stack in the order of the pin's recursion: first half
    first, all of it. So there is no depth bound. `MAX_BRANCHES` (2^23) bounds the work, as
    solidcore's `MAX_SOLVE_BRANCHES` did.
  - There is no cancel token. A hook that is cancelled returns its own error, which ends
    the solve.
- **Corrections (results change; each is checked against brute force):**
  - The rayfire child order reads the children of the tree being split. The pin read the
    second tree's and failed when that was a leaf (pin defect `gsolve_rayfire`).
  - A two-tree ordering asked of a third tree leaves its children in order. The pin
    failed.
  - `object_for_parameter` reads each object's own parameter count. The pin read object
    0's kind on every pass and fell out of the loop answering object 0 (pin defect
    `object_for_parameter`).
- **Split from the pin's `tree.rs`:** `SplitTree<T>` is the global solver's tree: built
  bottom up, immutable, with an item `T` per node. The spatial object tree
  (`AddObject`/`GetObjectsInBox`) is `box_tree`. The surface neighbour searches belong to
  the surface crate, and the span data (`BezierSpan`) to the curve crate, as their `T`.
- **Not ported:** the `SurfaceProjectedIntersect` curve-on-surface test (it reads curves:
  the curve crate's hook) and `PolySingleValue` (the poly solver's type).
- **Difftest:** `t_gsolve`, 144 seeded scenarios per operation (Intersect as ranges,
  Intersect keeping nodes, Minimize, Minimize keeping the best, Maximize,
  ProjectedMinimize, ProjectedMaximize, Rayfire). It compares the solutions, the best
  value and the order the leaf pairs were solved in. `gsolve_truths` checks 2,100
  scenarios against brute force.
- **Speed (MEASURED, 11-27):** the same hooks on both sides over 1,024 seeded scenarios,
  trees built outside the loop, inputs through `black_box`, every field of every result
  folded into a checksum, the two sides interleaved, 11 runs on one core, the median.
  scx takes 0.49 to 0.51 of the pin's time on trees of up to 9 and 12 pieces (0.8 to 3.6
  us a solve), and 0.56 to 0.63 on trees of up to 64 and 96 (37 to 126 us), but 0.91 for
  Intersect there, whose time is in joining the ranges. In `scx-bench` (`algo/gsolve_*`,
  criterion, one core, the batch of 1,024): Intersect 0.613 against 1.249 ms, Minimize
  1.458 / 2.973, keeping the best 1.552 / 2.991, Maximize 1.517 / 3.114, ProjectedMinimize
  3.503 / 6.728, and Minimize on the large trees 40.06 / 72.21 ms (0.49 to 0.55). The
  private line and plane helpers are now `geom`'s; the times did not move.

## 3. Fix when touched

Each item gives where it is, the pattern, and the replacement. The blast radius is MEASURED as
scx / pin lines. Where the item names a consumer, change it when that consumer is ported.

| # | Where | Pattern | Replacement | scx / pin |
|---|---|---|---|---|
| F1 | `code()`/`from_code()` on `ExtentPointType`, `BoundaryType`, `SmallTopoType` (`tol.rs:150`), `SolverAlgorithm`, `TerminationReason` | SMLib integer enums | the codes live in the facade as `fn code(x) -> i32`; the types keep only Rust variants | `from_code(` 5 / 93; `.code()` 1 / 92 |
| F2 | `ExtentPointType::Unknown`, `BoundaryType::Unknown`, `SmallTopoType::NoChange` | "unset" variants | `Option<..>`, or a separate argument type for the setter's no-op | 20, 70, 22 lines naming the types / 131, 73, 163 |
| F3 | tuple returns: `Extent1d::is_value_on_boundary -> (bool, f64)` (`:501`), `is_value_on_periodic_boundary`, `Extent3d::is_point_on_boundary -> (bool, Vec3)` with ±1.0 codes (`:957`), `Extent2d::contains_point_dist -> (bool, f64)` | a C++ bool return plus an out-param | `-> Option<f64>` (the snapped value); `-> Option<[Side; 3]>` with `enum Side { Lo, Hi, Inside }` | `is_point_on_boundary(` 7 / 27; `is_value_on_boundary(` 2 / 66 |
| F4 | `Extent3d::contains_line_seg` (`:947`) is true if either end is inside, which is a pinned upstream bug | a name that states the opposite of the behaviour | `contains_either_end`, until the defect is fixed after the layer (10.1 item 10) | 2 / 2 |
| F5 | three equalities on `Extent1d`: `are_same`, `are_equal(tol)`, `are_equal_bounds` | C++ overload names | `approx_eq(&self, o, tol)`, plus `PartialEq` for exact | 21 / 223 |
| F6 | `Inversion { parameter: Option<f64>, contained: bool }` plus `is_success` | flag plus value | `enum Inversion { Inside(f64), Outside(f64), Failed }` | small |
| F7 | `ExtentNd(Vec<Extent1d>)` with `get(i) -> Option<Extent1d>`/`get_mut` | a hand-made `Vec` wrapper | `[Domain1d; N]` (M6). Otherwise `Deref<Target = [Extent1d]>` | 20 / 96 |
| F8 | `hash_table.rs`: `HashTable` is an intrusive linked bucket table with public `BucketItem` links, `new` then `init_size(&mut self)`, `ObjsInVoxels::new` then `set_size`, capacity tracked by hand to copy C++ growth timing (`grown_capacity`), x86 cast emulation (`cvttsd2si`, `ulong_from_f64`), internals exposed (`counts() -> (usize, usize, usize)`, `buckets()`, `bucket_items()`), `CoincidentPairs { first: Vec, second: Vec }` as parallel arrays, `FindPairsOptions { histogram: Option<&mut [u64; 11]> }` (an out-param inside options), `find_coincident_pairs(&mut self)` (a query that takes `&mut`), `add(.., updated: Option<&mut Vec<usize>>)`, and removed items left as tombstones | the whole `SmHashTable`/`SmObjsInVoxels` shape | `VoxelGrid<T>` over `BTreeMap<[i64; 3], Vec<ItemId>>`, or the existing `BoxTree` with a point-box query; `pairs(&self, ..) -> Vec<(ItemId, ItemId)>` sorted; no histogram in the API; the transliteration kept as a difftest-only module if its order must match. This answers 01 §10.1 item 7: not public in its current shape | `ObjsInVoxels` 60 / 68; `HashTable` 29 / 43; pin users are 2 geom files and 1 heal file, so the consumer is `scx-stitch` (layer 7) |
| F9 | `math.rs:692`-`822`: `rect_of_aspect`, `fit_aspect -> Option<(f64, bool)>`, `offset_quad -> Option<(Vec3, [Point3; 4], [Point3; 4])>`, `axis_along_own_axes -> Result<bool, AxisRefusal>` | app and sketch conveniences at layer 0 (01 principle 1: conveniences go in helpers) | move to `scx-planar`/`scx-sketch` or a helper, with named result structs | 8 / 8 (defined only in `math.rs` at the pin; the callers are in `solidcore-ops`) |
| F10 | `math.rs:1140`-`1224` `unit_2d`, `circumcentre_2d`, `three_point_circle_2d` on `[f64; 2]`; `[f64; 2]` used 37 times in scx | primitive obsession | `Point2`/`Vec2` (after M1) | `[f64; 2]` 37 / 1,236 |
| F11 | `vector3d.rs:115`-`145` free `sqrt` (maps a non-positive input to 0), `cbrt`, `acos`, `asin` in a vector module; `polynomial.rs` imports them from there | std names with other semantics, in the wrong module | `fmath::{sqrt_clamped, acos_clamped, ..}`, decided with the `fmath` work | small |
| F12 | `vector3d.rs:105` `NOZ = f64::MAX`, `NOW = -f64::MAX`, `Vec3::from_nl([f64; 4])`, `is_2d()` | NLib control-point sentinels | `Vec3::from_homogeneous(x, y, z: Option<f64>, w: Option<f64>)`; `is_2d` dropped (a 2-D point is a `Point2`) | `NOZ` 7 / 363, mostly nurbs: fix in step 2 with `scx-nurbs` |
| F13 | `matrix.rs:365` `lu_decompose(&mut self) -> Result<LuDecomposition>` overwrites the matrix and returns the pivots; `solve_with_lu(&self, index, rhs)` is valid only on a decomposed matrix; `sv_decompose(&mut self)` overwrites with U; `transpose(&mut self) -> Result<()>`; `at`/`set_at -> Result` | two-phase use through the same type | `fn lu(self) -> Result<Lu>` with `Lu::solve(&self, rhs)` (typestate); `fn svd(self) -> Result<Svd { u, w, v }>`; `fn transposed(&self) -> Matrix`; `Index<(usize, usize)>` is not allowed under the lints, so `get(r, c) -> Option<f64>` | pin users: `lu_decompose(` in 9 files (7 nurbs), `sv_decompose(` 2, `eigen_system_jacobi(` 2 |
| F14 | `box_tree.rs:235`, `:265`, `:336` `overlapping`, `pairs`, `self_pairs` return `Vec<usize>`/`Vec<(usize, usize)>` with `tol: f64` | untyped indices, a bare tolerance | keep the sorted `Vec` (deterministic, cheap); give a typed index once `scx-brep` ids exist (`BoxTree<B, I: Id>`); `tol: Length` | new code |
| F15 | `Extent3d`, `Extent2d` u/v/w names (`set_u_min`, `u_min`..`w_max`) | `SmExtent` member names | `lo.x`, `hi.z`, or `Index<Axis>` (M8) | in M3's count |
| F16 | `Vec3::approx_eq` is not symmetric | it carries upstream's one-sided relative test | name it `approx_eq_rel_to(self, reference)`, or make it symmetric when the defect is fixed | small |
| F17 | `Vec3::unit_ortho_vectors_on_failure -> [Option<Self>; 3]`, `make_unit_ortho_vectors(Option<Self>)` | an optional out-param and fallbacks | `fn ortho_frame(self) -> Option<Frame>` | small |
| F18 | `#[non_exhaustive]` is absent from scx-num and scx-algo (MEASURED: 0 lines) | nothing to carry; missing | add it to `ErrorKind`, `NumFailure` and `SolveFailure` now, since 01 §5.1 already requires it for error enums; the rest at publication (01 §7.4) | 0 / n.a. |
| F19 | `Length`, `Accuracy`, `Resolution` and `ParamTol` have no `Display`; `Verdict`/`Bounded` neither | missing | `impl Display` (with the unit, `"1e-5 mm"`) | small |
| F20 | `IntoIterator for &Roots` (`polynomial.rs`) | missing | add it; the `Deref<Target = [f64]>` already makes `.iter()` work | small |
| F21 | `PeriodicExtent1d::set_min_max(.., single_point: bool)` | a bool picking behaviour | part of M3's enum. Listed separately because its pin callers (53 on `set_full_period(`/`from_parts(`) are in intersect and topology | 6 / 53 |
| F22 | `transform.rs:269` `inv_transform_point -> Result<Point3>` on a transform that is never inverted in the value; no `transform_point` | asymmetric | `Affine::inverse() -> Option<Affine>` then `apply` (M2) | in M2's count |
| F23 | xtask: hand-parsed arguments (`match args.first()`), hand-parsed TOML (`parse_layers`, `strip_comment`), `Result<_, String>` everywhere, `rust_files(dir, out: &mut Vec<PathBuf>)` | stringly and out-param style | an `enum Command` parsed once; a `struct XtaskError(String)` that implements `Display`; returning the `Vec`. Zero dependencies stays (01 §7.4), so no clap or toml | internal |

**Count: 23 fix-when-touched items** (F1 to F23).

## 4. Acceptable as it is

| Item | Why it stays |
|---|---|
| Operation names as solidcore paths, `&'static str` (`"geom::local_solve1d::.."`, `NumError::operation`) | Ruled (04, 11-26; 01 §5.3) for the difftest era, so errors compare exactly. Later crates use typed causes (01 §5.1). Section 7 asks 01 to say that solidcore paths stop at layer 0 |
| `NumError`/`NumFailure` mirroring solidcore one for one | Ruled. It is idiomatic anyway: typed failures with their numbers, `const fn kind`, `source()`, and `NumError::call(op)` as a `map_err` adapter |
| `Executor: Sync` with `fn scope(&self, n, f: &(dyn Fn(usize) + Sync))` | `dyn` on purpose (01 C11): it must be object-safe to sit in `Settings` and to take an app's pool. `map` is generic on top. Not sealed, because users implement it |
| `Cx { trace: &'a dyn Trace, .. }` (01 §6.3) | `Trace` is a sink chosen at runtime; dyn is the right tool. `Cx` is the one context value, limited to operation crates, with layers 0-4 taking only what they read. That is the opposite of a context threaded everywhere |
| `eval_many(&self, params: &[f64], out: &mut [Point3])` (01 §9.4) | A batched evaluator that writes into caller storage is the idiom where allocation matters (compare `Read::read`). Keep it |
| `LocalSolve1d { domain, algorithm, desired_accuracy, max_iter }` with public fields and `const fn new` | Already an options value. Only `acceptable_accuracy` needs to move in (M6) |
| `Domain1d { Line, Bounded, Periodic }` | The right enum. Only the bool constructor goes (M6) |
| `SolveError<E>` with `EvalError` classification | Ruled. Generic over the evaluator's error, with no boxing |
| Krawczyk's const-generic `[Ivl; N]` and `[[f64; N]; N]` | Idiomatic. Only the `dyn` goes (M7) |
| Units: `Length`, `Accuracy(Length)`, `Resolution(Length)`, `ParamTol` with private fields, `new -> Option`, `const fn` | Idiomatic (like `NonZeroU32::new`). `TryFrom` would add nothing |
| `hash.rs`: `FixedHasher: Hasher`, `FixedState: BuildHasher`, `Rng` with `const fn new` | It implements the std traits, so std maps can use the fixed hash. Exactly right |
| `cert.rs`: `Claim`, `EdgeEnd`, `Method`, `Reads` (at most 4 keys in a fixed array, `NONE`, `new -> Option`, `keys() -> &[u64]`), `Cert` and `Recorded` with public fields | Plain data with checked constructors, and no allocation |
| `certified.rs`: `Verdict`, `Bounded`, and `above(a, b)` false for NaN | A three-valued answer and an explicit error bound; NaN refuses |
| `interval.rs` `Ivl`: ops traits, `From<f64>`, `const fn new` | Idiomatic. `add_ulp` and its kin are named variants of the operators, which is right when two roundings exist |
| `ring.rs`/`series.rs`/`taylor_model.rs`: the `Ring`, `TayScalar` and `GnScalar` traits, `TmN<const N>`, `SeriesStorage` over an array or a `Box<[..]>`, and `Trunc` with `const fn` builders | Scalar-generic code as 01 §7.3 wants. `Ser::add(&self, b, tr)` keeps inherent names because the truncation argument makes them three-argument functions, not operators |
| `limits.rs` `try_filled`, `try_reserve_total`, `fits::<T>` | Fallible allocation without panics |
| `polynomial.rs` `Roots` (a fixed array, `Deref<Target = [f64]>`, `as_slice`) | Idiomatic |
| `BoxTree<B: Bounds>` with `const AXES` | New code in Rust's shape. F14 is a refinement |
| `#[must_use]` on nearly every pure function, and `const fn` wherever possible | Already the habit |
| The workspace lints (`unsafe_code = forbid`, `indexing_slicing = deny`, unwrap/expect/panic denied, pedantic warned) | Right. M8 is about how the code answers the lint, not about the lint |
| Plan: `Brep`/`Draft` (01 §2.3) | A typestate pair: reads take `&Brep`, edits go through `Draft`, and `finish` seals. It returns `Box<Unfinished>` with the draft inside, which is the idiom for giving the input back on failure |
| Plan: typed `u32` ids, never reused, with lookups returning `Option<Face<'_>>` | Right. `Index<FaceId>` is not allowed under the lints, because deleted slots are `None` (see 5.2) |
| Plan: `Document::new(..).with_resolution(r)` and `Settings` as `#[non_exhaustive]` plus `Default` | By-value builders and an options struct |
| Plan: one error per crate, `#[non_exhaustive]` causes, `source()`, and `kind()` onto a shared enum | Idiomatic |
| Plan: `Carry<T>` as a trait (01 §2.6) | Three related callbacks (split, merge, coincident) and a user type. A trait is right, and closures would be three parameters |

## 5. The plan: C++ patterns planned, and idioms missing

### 5.1 Planned patterns to change

| Plan § | Pattern | Proposal |
|---|---|---|
| 01 §7.4 (semver) | lists `EvalFunction` among the traits left unsealed for users | contradicts the 04 closure row: there is no such trait. Drop it from the list |
| 01 §7.1, last bullet | "Evaluators in solver callbacks keep `&mut self`" | restate as "solver callbacks are `FnMut` closures (04, 11-26)" |
| 01 §7.2, last bullet; §10.1 item 6 | "`LocalSolve1d`'s setters become an options value" | already done for 1-D. Restate as the 04 closure row: both solvers become options plus a pure call, and `LocalSolveNd` loses its setters and getters (M6) |
| 01 §10.1 item 3 | the sentinel list | add the inside-out empty extent (`BIG_DOUBLE`), `IterationValue::UNSET`, `NOZ`/`NOW`, `INFINITE_PARAMETER_SQRT`, `BOUNDED_INFINITE_PARAM`, and the `Unknown`/`Undefined`/`StillIterating`/`NoChange` variants. Decide `INFINITE_PARAMETER` as a named bound or `±inf` (M3) |
| 01 §10.1 (missing) | nothing on points, vectors and transforms, though §1.1 promises "points, frames, rigid and affine transforms" | add an item before step 2: M1 and M2 |
| 07 §2 rule | "held as `num::Angle(f64)`" | conflicts with 01 §4.2 ("plain `f64` radians; an `Angle` newtype is added only if a layer shows unit mix-ups"). Pick one. JUDGED: 01's |
| 07 §5 rule 2 | "Matrices are column vectors in `num`" | both scx-num transforms are row-vector today (M2). State it as a step-1 change, not a description |
| 07 §6 rule 4 | "`Extent::EMPTY` ... replace them" | an `EMPTY` constant that is a valid inside-out pair is the sentinel under a new name. Write "`Option<Extent>`" |
| 01 §2.1; 08 Q5 | `Region` has `solid: bool`, and `RegionId(0)` is the outside | `enum RegionKind { Outside, Solid, Void }`. A bool cannot say "outside", which is why id 0 has to |
| 01 §2.6 | `merged: Vec<(I, Vec<I>)>`, `coincident: Vec<(Operand, I, I)>` | named records, `Merge { survivor, absorbed }` and `Coincidence { operand, entity, onto }`, as `Skipped<C>` in §5.2 already is |
| 01 §5.1 | `Items(Vec<(usize, BlendCause)>)` beside `Skipped<C> { item, round, cause }` | one named `ItemFailure<C> { item, cause }` for both |
| 01 §5.1 | `Defect { at: &'static str, .. }` | `at: &'static core::panic::Location<'static>`, filled by `#[track_caller]` on the constructor. It is free, exact and cannot drift from the code, where a string can |

### 5.2 Idioms the plan should state

These are missing or only implied. Each is one line in 07 §9 or 01 §7.

- **Traversal is by iterators over borrowing views.** 01 §1.1 says `scx-brep` has "traversal", but
  §2.3 shows only `face(&self, f)`. The plan should state:
  - `Brep::faces(&self) -> impl ExactSizeIterator<Item = Face<'_>>` in id order;
  - `Face::loops()`, `Loop::uses()`, `Edge::sides()` and `Vertex::edges()`;
  - `impl<'a> IntoIterator for &'a Loop`;
  - no `Vec` returned where an iterator serves.
- **No `Index` on fallible lookups.** Ids name deleted slots, and the lints forbid panics, so id
  lookups are `get`-shaped (`face(id) -> Option<Face>`). Say so, so nobody adds `Index<FaceId>`.
  Total indices (the `Axis` enum, M8) do get `Index`.
- **Naming (07 §9 rule 5 is two sentences).** Add the API Guidelines rules:
  - `as_` (free, borrowed), `to_` (costly or by value) and `into_` (consuming) for conversions;
  - getters without `get_` (already there);
  - `iter`/`iter_mut`/`into_iter`;
  - `try_` for fallible twins, `with_` for builders, `from_` for constructors;
  - no upstream suffixes: `_simple`, `_it` (`solve_it`), `_ext`, `2`/`3`/`4` arities.
  The upstream name stays only in the doc line (07 §9 rule 2).
- **Options: public fields now, `#[non_exhaustive]` plus `with_*` at publication.** 01 §7.2
  prescribes `with_*` because `#[non_exhaustive]` forbids struct literals outside the crate. Code
  today uses public fields and struct literals. Both are fine. State that the switch happens at
  publication, so builders do not grow early.
- **Bools.** No `bool` parameter that picks behaviour. Use a two-variant enum (`Sense`, `Goal`,
  `Alignment`). A `bool` field in an options struct is fine, because it is named at the use site.
- **Out-params.** None, except batched evaluation into caller storage (01 §9.4).
  `Result<(), E>` plus mutation is allowed only when the value is unchanged on `Err`.
- **Send and Sync.** 01 §7.1 states it for geometry. Make it a compile-time test per public type
  (`const _: () = { fn ok<T: Send + Sync>() {} let _ = ok::<Brep>; };`), so a `Cell` cannot slip
  in, which 01 §6.1 forbids anyway.
- **Conversions.** `From` and `TryFrom` between `[f64; 3]` and `Point3`/`Vec3` in both directions,
  `From<Rigid> for Affine`, and `From<Frame> for Rigid`. Validated newtypes keep `new -> Option`.
- **`Display` on every public value type a user prints** (units, ids, errors).
- **The facade adapts to scx, never the reverse.** Section 1 explains why this needs saying: one
  closure body compiles on both sides, so a case that does not compile pulls scx towards the pin.
  The rule is that the adapter goes in `x`.

The plan already covers `no_std` (01 §7.4: not a goal, cheap later), features (few and additive),
sealed traits, `#[non_exhaustive]`, `#[must_use]` (pedantic warns `must_use_candidate`) and
`const fn`.

## 6. Order

M1, M2 and M3 interlock: `Frame` needs `UnitVec3`, and extents are built from points. They are one
change to `scx-num`'s value types, made before step 2.

- M4, M5, M8 and M9 are small and independent.
- M6 and M7 are the closure row and its neighbour, in `scx-algo`.
- Each keeps the difftest at 0 mismatches through `x`-side adapters (section 1). The added adapter
  code is roughly 40 small functions plus the `both!` splits for six types (JUDGED, from the
  counts above).

## 7. Plan sections to rewrite

1. **01 §7.4**: drop `EvalFunction` from the unsealed list.
2. **01 §7.1 and §7.2**: closures are `FnMut`; both solvers are options plus a call.
3. **01 §10.1**:
   - item 3: complete the sentinel list (5.1);
   - item 6: restate as the closure row;
   - a new item: points/vectors/`UnitVec3` and `Rigid`/`Affine`/`Frame`, column vectors;
   - item 7: give the `hash_table` answer (F8).
4. **07 §2**: reconcile `Angle` with 01 §4.2.
5. **07 §5 rule 2**: mark it as a change to make.
6. **07 §6 rule 4**: write `Option<Extent>`, not `Extent::EMPTY`.
7. **07 §9 rule 5**: the naming, bool, out-param, conversion and facade rules of 5.2.
8. **01 §2.1, §2.6 and §5.1**: `RegionKind`, named change records, one item-failure type, and
   `Defect::at` as a `Location`.

## 8. `scx-nurbs` (step 2): decisions made while porting

Written 11-27 by the builder of `scx-nurbs` (01 §10.2 step 2: knot vectors, basis functions,
Bezier helpers, NURBS curves as plain data). The pin's sources are `nurbs/knots`,
`nurbs/basis_basic`, `nurbs/bezier`, `nurbs/crv_*` and `nurbs/math`. Each item names the pin's
shape, what replaced it, and why. Items marked "(ruling)" are proposals for the owner.

- **One validated value per concept; no setters.** The pin's curve is a long-lived struct with
  `knots_mut`, `cpts_mut` and per-call checks of counts and indices (`NurbsFailure::Index`,
  `ControlPointsShort`). Here `KnotVector::new` and `NurbsCurve::new`/`from_homogeneous` check
  everything once (degree, counts, finiteness, non-decreasing knots, multiplicity, positive
  weights, finite `w P`), and every edit (`inserted`, `removed`, `insert_knot`, `split`,
  `elevate`) returns a new value. The index errors have no counterpart: a built curve cannot
  have the wrong count.
- **Rationality in the type (F12).** `NOZ`/`NOW` marked "not rational" per point; a mixed curve
  could exist. Here a curve holds `Option<Rational { weights, weighted }>`: `None` is polynomial,
  and both `P` and `w P` are kept so neither form is recomputed per evaluation.
  `from_homogeneous` is the rational constructor from `(w P, w)`. F12's removal from `scx-num`
  (`NOZ`, `NOW`, `from_nl`, `is_2d`) is its own commit; research 63 §2.2 has
  `scx-nurbs-surface` follow this exactly.
- **`Approach::{Below, Above}` replaces `from_left: bool`** (63 X5, ruled L4). The pin's
  `Side::Left` is the span starting at the knot, which is `Approach::Above`. The testkit's
  `Side` is renamed to match when its owner next touches it; no crate exports `Side`.
- **Span finding is exact.** The pin's `find_span` moves the end spans inwards past spans
  shorter than an absolute `1e-12` (`U[spn+1] < U[spn] + 1e-12`), a threshold with no scale.
  Here the span is found by comparison alone (`partition_point` on `<` or `<=` by approach),
  and only spans of exactly zero length are skipped. Every difftest point away from the domain
  ends agrees with the pin bit for bit.
- **The domain ends evaluate.** The pin's `basis_eval` and `crv_eval` return
  `SolidError::Unsupported` at `u = U[p]` and `u = U[m-p]`; upstream callers evaluated the end
  control points themselves. Here both ends take the end span, which is the limit the pin's
  callers wanted. Outside the domain each end piece continues as its polynomial (59 §8 item 4).
- **A knot difference fails only when it is exactly zero or not finite.** The pin errs below
  `1e-24` (`NurbsFailure::Numerical`), a threshold with no scale. Here `NurbsError::Degenerate`
  carries the parameter.
- **Kernels return `Option`, mapped once to `Degenerate` (M8).** The inner loops iterate and do
  not index; where a slot could in principle be missing they return `None`, and the public call
  maps it to `NurbsError::Degenerate`. No silent fallback value is produced. (ruling: this is
  M8's pattern; the alternative is `debug_assert` plus a default, which 57 M8 rejected.)
- **Fixed-order derivatives as const generics.** The pin's `crv_derivs(u, d, out: &mut
  Vec<Vec3>)` and `basis_derivs(.., n, out)` are out-params sized at run time. Here
  `derivs::<K>(u, approach) -> [[f64; D]; K]` and `basis_derivs::<K>` return arrays on the stack
  (63 D3's fixed-order jets); `K - 1` above the degree gives zeros for a polynomial curve, and the
  rational quotient rule runs to any `K <= MAX_ORDER + 1`.
- **Degree caps: `MAX_DEGREE = 31`, `MAX_ORDER = 31`** (ruling). The pin's limit is
  `p < MAX_DEGREE` with its Bezier binomials from an 18-row Pascal table, so its degree
  elevation stops at 17. Here binomials are computed exactly in integers (`None` above `2^53`);
  31 bounds the stack scratch and keeps every binomial the rational derivatives and elevation
  use exact. Any cap from 25 up keeps that exactness.
- **Unclamped knots, and interior multiplicity up to `p + 1`, are valid** (ruling). The pin's
  `validate` (`N_KnotVectorIsValid`) requires end multiplicity exactly `p + 1` and interior
  multiplicity at most `p`. Here the domain is `[U[p], U[m-p]]` whatever the end knots
  (`is_clamped` reports it), and an interior knot of full multiplicity is a break in the curve
  whose two one-sided limits `Approach` selects; the testkit's truth treats both the same way.
  Above `p + 1` is `NurbsError::Multiplicity`. Insertion still keeps the result continuous
  (multiplicity at most `p` after it).
  [Changed 11-27 by 04's scx-nurbs ruling (2): the knot vector stays as said, but
  `NurbsCurve::new` and `from_homogeneous` refuse an interior knot of multiplicity `p + 1`
  (`KnotVector::check_continuous`, `NurbsError::Multiplicity` with `max = p`), so every curve
  is C0 and a break is a split. `elevate`'s own check for it went, as no curve can have one.
  The pin's `Curve::new` builds such curves (it checks only for empty inputs), so the
  difftests on `Knots::Multiple` curves with a break are pin defects (`break_refused`, its
  truth: the exact curve's limits either side of the knot differ).]
- **Affine for insertion and splitting, `combine2` for elevation.** The pin's `affine2` keeps a
  coordinate exactly when both points have it (a plane stays exactly a plane); its
  `elevate_degree` uses `combine2(alf, a, 1 - alf, b)` throughout. scx follows both, in the pin's
  order. Using `affine` in elevation too was tried: it differed from the pin on 210 elevation
  inputs, against 110 with `combine2` (all 110 the refusals below), so the pin's choice stays.
- **`NurbsError::Unstable` for an elevation that loses the curve.** A5.9 removes the inserted
  knots by extrapolating across the neighbouring spans, so a span ratio near `1e16` (knots a few
  ulps apart) multiplies the rounding by that much. The pin returns the result, with zero or
  negative weights on rational curves and errors up to `1e32`; scx refuses with `Unstable`
  (`ErrorKind::Degenerate`) when the elevated weights or points are no longer valid. Its
  polynomial answers on such curves are the pin's bit for bit, and as inaccurate: the ignored
  test `elevation_of_near_coincident_knots` holds the truth check for a stable elevation
  (Bezier decomposition with exact blossoms, or knot removal by least squares), a step-5
  follow-up (ruling: whether that is wanted before step 5).
- **Bezier helpers take any `u`.** The pin's `eval_one_basis` refuses `i > p` and `u` outside
  `[0, 1]`, and `split` refuses `u` outside `[0, 1]`. `B[i, n]` is `0` for `i > n` and both are
  polynomials defined everywhere, so scx answers (a Bezier curve extended past its ends). Both
  refuse a non-finite `u` with `ParameterNotFinite`, where the pin's `eval_one_basis` returned
  NaN as a value for a NaN `u`.
- **Typed errors per crate.** `NurbsError` is `#[non_exhaustive]`, `Copy`, with `kind() ->
  ErrorKind` per 62's taxonomy and a table from the pin's `NurbsFailure` in `error.rs`.
- **Not yet done, and shaped for.** The scalar-generic evaluation (`eval<T: Ring>`) that 63 §3
  wants for interval jets, `eval_many`, `tangent_dir`, fitting, deviation, enclosures and IGES 126
  are not here. The curve's fields are private and every edit is by value, so these add methods
  without changing the type.

## 9. M11: `scx-algo` geometry helpers, sort, pseudo boxes and point sets

These decisions were made while porting `geom` (`smgu_*`: `plane`, `line`, `polygon`, `fit`),
`sort`, `pseudo_box` and `point_set`. Each module's header lists its full Deviations; this
section records the choices that the owner should rule on or that a later port should follow.

- **`None` for an answer that is not finite or that overflows** (for the owner's ruling).
  A parameter, distance, crossing or common point that is not finite gives `None`. That
  covers input that is not finite, and a result past `f64`'s range. The pin answers NaN or
  infinity as a value, or a silent 0 (`smgu_LineClosestPoint`'s zero-direction branch).
  Where only an intermediate square overflows (components past `1e154`), the answer is
  taken scaled and given: `line_closest_point` by `v / max |v_i|`; `point_project_to_plane`
  along the unit normal when `k = d·n / |n|²` overflows; `triangle_decomposition` over the
  longest side. The segment functions are the exception. A segment of zero length keeps the
  pin's parameter 0, because a point is a fine segment. A segment with no answer gives the
  parameter NaN, because the result is a struct of a parameter and a point that callers
  read whole. Typing that struct as `Option` is the follow-up when the segment helpers next
  change. The alternative was to keep the pin's values, so callers would test
  `is_finite` themselves. In `t_gutil`, these answers are among the truth-checked pin defects
  (`line_closest_point` has 1551 of them, and `point_project_to_plane` has 1507; some of
  those are other causes).
- **Not ported, on purpose.** `smgu_SolveQuadraticEqn` is a one-line forward to
  `crate::polynomial::solve_quadratic`, so callers use that directly. The 3x3 and 4x4
  determinant, inverse and multiply helpers are already in `scx_num::small_matrix`.
  `smgu_VectorProjectToPlane` is `Vec3::project_to_plane`, with the same expression. The
  `smgu_*Copy` helpers are not needed, because arrays are `Copy`. `geom/mod.rs` holds the
  table from each upstream name to its replacement.
- **`det3`'s term order.** `smgu_Determinant3Vectors` maps to `Vec3::triple_product`
  (`a · (b × c)`). That has the same value, but it rounds differently. `pseudo_box` keeps a
  private `det3` with upstream's six terms, in upstream's order:
  `ax by cz - ax bz cy + ay bz cx - ay bx cz + az bx cy - az by cx`. A box's coordinates
  are three such ratios, so a point exactly on a face classifies as upstream's does: the
  roundings must match bit for bit where a coordinate is compared with an interval end.
  `plane::intersect_three_planes` uses `triple_product`, because no
  on-boundary decision hangs on it. 140 of its answers are within tolerance and none is a
  mismatch. Where a sign or a zero decides a classification, keep the six terms; anywhere
  else, use `triple_product`.
- **Idiom: the fit structs are SMLib's out-parameter bag.** `PointFit`, `LineFit` and
  `PlaneFit` are flat structs. Each has a `set_type`, an `Option` direction and gaps, all as
  public fields, the shape of `smgu_BestFit*ToPoints`' out-params. The type does not tie
  them together: `direction` is `Some` exactly for `Linear`, `Planar` and `Scattered`, and
  `max_plane_gap` means the line gap for a linear set. Only the docs say so. The idiomatic
  shape is one enum per fit, for example `PlaneFit::{Void, PointSized { center, gap },
  Linear { point, direction, gap }, Planar { .. }, Scattered { .. }}`, so a caller cannot
  read a normal that is not there. They were kept flat for this port so that the difftest
  compares field for field with the pin's structs. `point_set` is the first caller, and it
  already maps `set_type` and `direction` together. Change them when the fit next changes, or
  before `scx-geom` adds callers (§3 rule: fix when touched).
- **`point_set`'s given normal is a `UnitVec3` and is not written back.** The pin's
  `PointSet3d::new(.., Option<&mut Vec3>)` unitizes the caller's vector in place, which is an
  out-parameter. `PointSet::with_normal` takes a `UnitVec3`, so the type carries the
  contract, and the caller keeps what it passed. The pin's `unitize_or_z` loses huge and
  tiny vectors (it gives z). This is in `ps_with_normal`'s pin defect.
- **The pin's `SpatialIndex` is `BoxTree`; not ported.** The pin's call-local
  `SpatialIndex` (`topology/brep/validation/spatial.rs`) is a median-split tree over
  `Option<Extent3d>`. Callers use it in `IntersectTopology`'s face-pair and vertex/edge
  searches, in `FaceVertexIndex` and in `SmFace::AssertValid`'s coincident-vertex test.
  `crate::box_tree::BoxTree` gives the same candidates on every well-formed input. Here,
  well-formed means boxes that are finite with `min <= max`, a finite query and
  `tol >= 0`. Map it this way:
  - `SpatialIndex::new(boxes)` becomes `BoxTree::new`, with `None` taken as the unbounded
    box. That box is never disjoint, so it is always reported, as `SpatialIndex` reports it.
  - `boxes_in_box(q, tol)` becomes `overlapping(q, tol)`. Both use the same `are_disjoint`
    predicate.
  - `near_point(p, tol)` becomes `overlapping(&Extent3d::from_point(p), tol)`.

  The two differ only on degenerate inputs, where `SpatialIndex` returns a superset:
  - It always reports an inside-out box or a box with an infinite bound. `BoxTree` tests
    each of them exactly.
  - It returns every item when the query is not finite or `tol` is negative or not finite.
    `BoxTree` tests each item.
  - `near_point` compares `q - max > tol`, where `are_disjoint` compares `q > max + tol`.
    These round differently once the coordinates are much larger than `tol`.

  Every caller re-checks the candidates with its own predicate: a distance below
  `EFF_ZERO` against a pad of `2 EFF_ZERO`, or the exact pair test that the boolean already
  ran before the index was added. The superset therefore never changes a result. On the
  rounding difference: when `q > max + tol` fails to exclude an item that `q - max > tol`
  would exclude, the native check still rejects it. When the rounding goes the other way
  (`max + tol` rounds to `max`), `q - max` is at least an ulp of `q`, which is above the
  pad, so the native check rejects that item too. The difftest of the first caller ported
  (`FaceVertexIndex` or the boolean's pair search) should confirm this on its own inputs.
