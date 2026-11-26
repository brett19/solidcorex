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
