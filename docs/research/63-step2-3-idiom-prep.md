# 63 - Steps 2 and 3: the idiom hunt ahead of the builders

Written for solidcorex, 2026-11-27. When `scx-nurbs` lands, builders start the rest of step 2 and
then step 3 of 01 §10.2. Research 57 found the C++/SMLib shapes in what had been ported to
`scx-num` and `scx-algo`. This note does the same hunt ahead of time for the six crates that come
next, so each builder starts from a ruled shape and does not rediscover it:

- `scx-nurbs-surface`;
- `scx-curve`;
- `scx-surface`;
- the analytic part of `scx-ssi`;
- `scx-brep`;
- `scx-record`.

For each crate the note gives:

1. the shapes found at the pin, with citations and counts;
2. the proposed shape, consistent with 57's rulings;
3. a sketch of the public types;
4. the truth references the crate needs that `scx-testkit` lacks;
5. a split into builder tasks with their dependencies.

Section 9 then argues against each proposal and revises the ones that lose. Section 11 lists the
decisions, written as 04 rows. Section 12 records the coordinating session's rulings of 2026-11-27,
which adopt the corrections of `reviews/review-63.md`. The body is revised in place to match them,
and each place a ruling changed carries a bracketed "[Ruled 11-27: ...]" note. 04 §3 carries the
rulings as its 11-27 rows on this research.

- **Labels** (as in 57):
  - READ: read from the pin's code or the docs.
  - MEASURED: counted or run here; the method is given.
  - JUDGED: a design judgement.
- **Citations.**
  - Pin paths are relative to the pinned kernel's `src/` (crate `solidcore`, pin 03e16c9ee7).
  - `p2.rs:335` means line 335 of that file.
- **Scope.**
  - No crate code was changed.
  - Errors follow research 62 as ruled (its §10, 01 §5, the 11-27 rows of 04): each crate has its
    own error type with typed causes, `SolveError<E>` where a solver is wrapped, and no status
    codes. Every cause named below is given its kind, and every wrapper is marked as a transparent
    or a context frame (X13). [Ruled 11-27, M2: this line said 62 was in progress.]

## 0. How the counts were made (MEASURED)

- **Which file belongs to which crate.**
  - Every pin file was mapped to a crate by research 56's `assignment.py` table, longest prefix
    first.
  - Files whose path contains `test` were then dropped.
- **What a count means.**
  - A count is the number of matching lines found by `grep -cE` over the remaining files.
  - Comment and doc lines are included, as in 57. A count is a size, not an exact number of
    occurrences.
  - The doc comments name the C++ shape often, so a high count for `SM_ERR` partly measures how
    carefully the port documented the shape.
- **The six crates' shares of the pin:**

| Crate | Pin files (all) | Pin lines (all) |
|---|---:|---:|
| `scx-nurbs-surface` | 24 | 24,926 |
| `scx-curve` | 66 | 89,931 |
| `scx-surface` | 146 | 249,335 |
| `scx-ssi` | 13 | 21,334 |
| `scx-brep` | 53 | 123,585 |
| `scx-record` | 4 | 3,248 |

- **Step 2 ports a slice, not these totals.**
  - Curves: line, circle, ellipse and B-spline.
  - Surfaces: plane, cylinder, cone, sphere, torus and B-spline.
  - Queries: inversion, closed-form projection and curve-surface intersection.
  - Intersection: the analytic part of `scx-ssi`.
  - The counts show where each shape lives. The slice meets most of them, because the shapes sit in
    the dispatch types (`Curve`, `Surface`, the topology records) that every kind passes through.

## 1. Shapes common to all six crates

Every crate meets these shapes. Each is ruled once here, and the crate sections refer back to it.

### 1.1 Counts (MEASURED)

| Shape | Pattern (summary) | nurbs-surface | curve | surface | ssi | brep | record |
|---|---|---:|---:|---:|---:|---:|---:|
| Kernel passed for reading | `&Kernel` | 0 | 498 | 1,685 | 229 | 603 | 1 |
| Kernel passed for writing (the arena) | `&mut Kernel` | 0 | 0 | 3 | 0 | 418 | 6 |
| Tolerance read from the kernel | `k.tolerances`, `k.fixes` | 0 | 11 | 17 | 0 | 18 | 0 |
| Failing base-class default | `Err(base_err`, `base_*()`, `SM_ERR` | 2 | 267 | 603 | 38 | 287 | 0 |
| Unsupported kind or method at run time | `Unsupported*Kind`, "not implemented" | 0 | 170 | 380 | 66 | 227 | 0 |
| Upstream status inspected | `upstream_status` | 0 | 49 | 84 | 12 | 77 | 3 |
| Behaviour-picking bool parameter | `name: bool` in a signature | 55 | 247 | 750 | 50 | 501 | 15 |
| the span side as a bool | `from_left` | 3 | 72 | 181 | 0 | 3 | 0 |
| zero tangents replaced | `non_zero_tangents` | 0 | 59 | 92 | 5 | 2 | 0 |
| Integer flag or type code parameter | `kind/type/flag/mode: i32` etc. | 11 | 33 | 127 | 7 | 44 | 5 |
| Type test by integer | `is_kind_of`, `_TYPE` | 12 | 359 | 606 | 18 | 317 | 1 |
| Owner link on geometry | `ObjectRef`, `owner` | 0 | 435 | 887 | 17 | 1,495 | 6 |
| Identity and notification | `identity`, `notify` | 0 | 103 | 269 | 5 | 1,919 | 13 |
| Owner's zone tolerance cached on geometry | `owner_zone_tol` | 0 | 52 | 92 | 3 | 21 | 0 |
| NURB carried inside an analytic kind | `kind_with_nurb`, `gw_nurb` | 0 | 126 | 341 | 6 | 59 | 0 |
| Memo or cache inside geometry | `memo`, `cache` | 49 | 316 | 647 | 71 | 152 | 4 |
| Interior mutability | `RefCell`, `Cell`, `Rc` | 0 | 0 | 25 | 2 | 6 | 0 |
| Sentinel values | `UNDEF`, `NOZ`/`NOW`, `BIG_DOUBLE`, `INFINITE_PARAMETER`, ... | 116 | 552 | 1,045 | 71 | 404 | 2 |
| of these, `NOZ`/`NOW` | | 116 | 245 | 448 | 10 | 121 | 0 |
| of these, `INFINITE_PARAMETER` | | 0 | 16 | 100 | 10 | 0 | 0 |
| of these, `BIG_DOUBLE` | | 0 | 53 | 97 | 14 | 63 | 0 |
| A tuple with a bool | `(bool,` | 1 | 25 | 89 | 8 | 20 | 0 |
| Answer and "needs more work" bool | `needs_more` | 0 | 94 | 83 | 2 | 0 | 0 |
| Out-parameter: vector | `: &mut Vec<` | 2 | 22 | 52 | 15 | 41 | 1 |
| Out-parameter: slice | `: &mut [` | 44 | 38 | 78 | 17 | 20 | 0 |
| Out-parameter: scalar, point or `Option` | `: &mut f64`, `: &mut Point3`, ... | 8 | 47 | 76 | 2 | 38 | 0 |
| `Result<()>` (mutate and report) | `-> Result<()>` | 64 | 148 | 356 | 73 | 623 | 12 |
| `Result<bool>` | | 6 | 114 | 285 | 33 | 202 | 0 |
| Setter on a long-lived object | `fn set_*(&mut self` | 1 | 14 | 36 | 3 | 31 | 3 |
| Nested vectors | `Vec<Vec<` | 68 | 13 | 167 | 5 | 14 | 0 |
| `Vec<Vec<Vec3>>` partials | | 18 | 0 | 59 | 2 | 2 | 0 |
| `Result<Vec<Vec3>>` curve derivatives | | 6 | 19 | 37 | 0 | 0 | 0 |
| Angles in degrees | `to_radians`, `360.0`, `180.0` | 9 | 204 | 497 | 7 | 34 | 0 |
| Upstream arity or variant suffix | `_simple(` | 0 | 1 | 352 | 0 | 4 | 0 |
| Trait objects | `dyn` | 1 | 9 | 101 | 12 | 145 | 1 |
| Upstream class names in identifiers or docs | `` `Sm`` | 3 | 951 | 1,883 | 126 | 1,879 | 79 |

### 1.2 The rulings

**X1. No ambient kernel.**

- **Pin.**
  - `&Kernel` is threaded through almost every geometry signature. The analytic kinds ignore it:
    `CurveEvaluate::evaluate(&self, k: &Kernel, ..)` at `curve/curve/p2.rs:345`, implemented by
    `Conic`, `Line` and the others with `_k`.
  - It carries tolerances (`k.tolerances`), the arena (`&mut Kernel` in topology) and the
    notification stores (`topology/edge.rs:2539` `NotificationStores`, `:2558` `NotifyEnv`).
- **Proposal** (01 §6 and §12, already ruled).
  - Geometry functions take only what they read:
    - a `Length` or `Resolution` where a tolerance decides an answer;
    - `&mut Scratch` where they memoise;
    - a budget and `Stop` where they may run long.
  - The body is a value (`Brep`, `Draft`), not an arena inside a kernel.
  - A function that read `k` only to forward it loses the parameter.

**X2. No failing defaults.**

- **Pin.** A base-class virtual becomes a match arm or a free `base_*()` that returns an error at
  run time.
  - `curve/curve/p2.rs:327` has `base_err(BaseCurveMethod)`, with 9 methods. Arms return it at
    `:1598` (continuities), `:1805` (inversion), `:1919`, `:1941`, `:2008`, `:2051` (transform: for
    Hermite, conic, iso and offset curves), `:2097`, `:2138` and `:2209`.
  - `surface/surface/p2.rs:5195`-`5400` has the free defaults:
    - `base_copy`;
    - `base_adjust_step_uv_domain`;
    - `base_drop_point_fast`;
    - `base_evaluate`;
    - `base_intersect_with_plane -> (bool, SurfaceIntersection)` (`:5375`);
    - `base_edit` (`:5381`);
    - `base_split_at`;
    - `base_step_inversion -> (Point2, LocationType)` (`:5391`);
    - `base_knots -> (Vec<f64>, Vec<usize>)` (`:5396`).
- **Proposal.** Every operation on a kind enum is a total `match`. For a kind that cannot do an
  operation exactly, the choice is made per operation and written in the type:
  - (a) the kind is converted to an exact NURBS image, which does it;
  - (b) the operation is approximated, and the accuracy is recorded (04, 11-26 accuracy rule;
    11-27 transform rule);
  - (c) the operation does not exist for the kind. It is then a method of that kind only, or it
    returns `Option`/`Result` with a cause that names the kind, such as `Unsupported(SurfaceKindName)`.
    It is never a catch-all `SM_ERR`.
  - For the step-2 kinds (line, circle, ellipse, B-spline; plane, cylinder, cone, sphere, torus,
    B-spline) every operation in the slice is total. No step-2 match arm should return an
    unsupported error (JUDGED, from reading the overrides).

**X3. Geometry has no owner and no identity.**

- **Pin.**
  - `struct Curve` (`curve/curve/p2.rs:628`) holds `owner: Option<ObjectRef>`, a `notify_identity`,
    a memo shared by clones, and `owner_zone_tol_3d: Option<(Option<ObjectRef>, f64)>`.
  - `Clone` resets the owner. `snapshot_copy` and `clone_exact_bounded` exist beside it.
  - `struct Surface` (`surface/surface/p2.rs:428`) has the same fields, plus `owner_chain_face`
    (`:435`) and `made_kind` (`:451`), a lazily built NURB.
  - The plane intersection expands its box by "the tolerance of the plane's face"
    (`surface/sphere.rs:1195`, `PlaneOwner`).
- **Proposal** (01 §7.1, ruled).
  - Geometry is plain data: `Clone + Send + Sync + PartialEq`, with no owner link and no identity.
  - What the owner supplied comes in as an argument. The face's accuracy is a `Length` the caller
    passes (`scx-brep` reads it from the face record).
  - Identity lives in body ids only.
  - Clone is a clone. There is one copy operation, not three.

**X4. Caches are cells on immutable payloads, or the caller's `Scratch`.**

- **Pin.** `CurveMemo` is shared between clones and reset by every in-place edit
  (`transform_at_depth` resets it). Surface has a `CacheSlot`, and `take_made_kind_for_edit` at
  `surface/surface/p2.rs:1016`.
- **Proposal** (01 §3.1).
  - Large payloads (a NURBS net, a procedural definition) sit behind `Arc`. Their derived data are
    `OnceLock` fields of that payload, which is never edited, so no invalidation exists.
  - Small analytic kinds carry no cache.
  - A memo of results across calls lives in the caller's `Scratch`.

**X5. Bools that pick behaviour become enums or separate functions** (57 §5.2).

- **`from_left: bool` becomes `Approach::{Below, Above}`.**
  - This is the testkit's `Side { Below, Above }` (59 §8 item 4, `scx-testkit/src/nurbs.rs:26`)
    under a name of its own, with `scx-nurbs` defining the library copy and the testkit renamed to
    match when it lands.
  - The pin's own naming shows why: `EvaluateFlags::u_side` (`nurbs/b_spline_surface_n_lib/p2.rs:177`)
    documents "`bUFromLeft`: `TRUE` is `Side::Left`, the span starting at the knot". "Left" is the
    span to the right of the knot. A name by limit direction cannot be misread that way.
  - **One enum per meaning, named apart** [Ruled 11-27, L4]. Three `Side`s would otherwise meet in
    one prelude: 57 F3's boundary side `{ Lo, Hi, Inside }` becomes `BoundSide`, the evaluation side
    is `Approach`, and `scx-p21`'s product-structure `Side` stays in that crate, outside the geometry preludes. No crate in
    this note exports a type named `Side`.
- **`non_zero_tangents: bool` (upstream default TRUE) is dropped as a flag.**
  - Its doc (`curve/curve/p2.rs:343`) reads: "a zero first derivative is replaced by a
    tolerance-sized one along the limiting direction".
  - Proposal: derivatives are always the true derivatives. The limiting direction is its own
    function, `tangent_dir(t, side) -> Option<UnitVec3>`, taken from the first non-zero
    derivative.
  - Section 9 revises how callers reach it.
- **Other bools.** `classify: bool` on inversion (`curve/ellipse/p2.rs:1418`) and the like are
  split into two functions, or the cheap part is always returned.

**X6. Sentinels become types.**

- **Pin.**
  - `Vec3::UNDEF` fills failed outputs (`curve/curve/p1.rs:177`, `p3.rs:175`, `p7.rs:992`).
  - `NOZ` and `NOW` mark "not rational".
  - `INFINITE_PARAMETER` (`= 1234567.0`) bounds unbounded parameters.
  - `BIG_DOUBLE` marks inside-out empty extents.
  - `zone_tol == 0` means "use the context's tolerance" (`curve/ellipse/p2.rs:1407`).
  - `rectangular_trim: i32` holds `UNSURE` (`topology.rs:458` struct `Face`).
  - `tolerance: f64` is 0.0 when not set (`topology.rs:386`, `:413`).
- **Proposal** (57 M3, F12, 01 §10.1 item 3).
  - `Option` where a value may be absent, enums where the states are named, `Option<Extent>` for
    empty extents, and `from_homogeneous` for rationality (F12, owned by `scx-nurbs`).
  - **`INFINITE_PARAMETER`: unboundedness is a state of the type, not a value** (section 11, D1).
    [Ruled 11-27, D1 revised after review-63 H1. This item first proposed ±inf ends.]
    - A shared `ParamDomain` (P2) says, per parameter direction, whether the domain is periodic,
      bounded, half-open (bounded below or above only) or unbounded. A missing end is a variant,
      with no number behind it.
    - `Extent1d`, `Extent2d` and `Extent3d` ends become finite by construction: `new` refuses a
      non-finite end, `UNBOUNDED` and `is_infinite_param` go, and so do the sentinel branches in
      `shift` and `times`. That is a change to merged `scx-num` (today `extent1d.rs` stores
      unbounded ends as `±INFINITE_PARAMETER`, review-63 H1), so it is its own task, P0 (section 8),
      ahead of RE1, BR1, CU1 and SU1.
    - Neither ±inf nor a sentinel is ever stored, in memory or in a record.
    - Numeric work on an unbounded kind goes through an explicit finite working extent,
      `ParamDomain::working(&self, rule: Clamp) -> Extent1d`. The extent comes from the face's uv
      box or the edge's interval where one exists, and from a stated `Clamp` rule otherwise. No
      arithmetic ever touches a `ParamDomain` end.
    - The pin's two clamping lessons are kept as named `Clamp` rules (`math.rs:61-65`, 57 §5.1):
      - `BOUNDED_INFINITE_PARAM = 33.0`, the half-size used where an unbounded range must be
        sampled, becomes `Clamp::Sample`;
      - `INFINITE_PARAMETER_SQRT = 1111.111` exists because a parabola squares its parameter. It
        becomes `Clamp::Squared`: a kind whose point grows as the square of its parameter clamps
        at the square root of the bound, so the evaluated point stays within the bound.
    - The general bound is sized from the model, never a fixed 1.2e6. The pin's #18922
      (`offset/offset_geometry_creation.rs:~6885`) found 3.4e-11 rounding on a plane copy sized at
      1.2e6, which is the cost of a bound far larger than the part.
    - The pin's newer `eval/label.rs:23` (`UNBOUNDED = Range(-inf, inf)`) is not followed. ±inf does
      not fail loudly: `inf * 0` and `inf - inf` give NaN, and NaN comparisons are silently false
      (review-63 H1).

**X7. A pair with a bool becomes a sum type.**

- **Pin.**
  - `intersect_with_plane -> Result<(bool, SurfaceIntersection)>` (`surface/sphere.rs:1178`). The
    bool is `rbNeedsMoreIntersections`: "the general intersector is still needed".
  - The dispatcher adds a third element: `(bool, SurfaceIntersection, Option<PlaneIntersectWarning>)`
    (`surface/surface/p2.rs:3249`).
  - Inversion returns `(Point2, LocationType)` with a sentinel location on failure.
  - `KnotsWithStatus` (`curve/crv_on_surf.rs:1111`) returns knots with a status beside them.
- **Proposal** (57 F3, F6).
  - The plane case becomes `enum Closed { Solved(Section), General }`. `General` means "no closed
    form for this pair here". It is an answer, not an error.
  - Warnings go in the result struct, not in a third tuple slot.

**X8. No upstream status inspection.**

- **Pin.** `step_inversion_answered` (`surface/surface/p2.rs:2902`) treats an error as an answer
  when its upstream status is `SM_ERR_NOT_WITHIN_TOLERANCE` or `SM_ERR_OUTSIDE_OF_DOMAIN`.
  - Two "failures" are in fact results: "the nearest point is not on the surface" and "the point
    is outside the domain".
- **Proposal.** Inversion returns an `Inversion`-shaped result with those outcomes as variants (57
  F6, extended in section 4). No caller ever looks at a code.

**X9. Out-parameters.**

- **Pin.** `&mut Vec`, `&mut [..]` and `&mut f64` outputs, often with a `bool` or `Result<()>`
  return.
- **Proposal** (57 §5.2).
  - Return values.
  - The one kept exception is batched evaluation into caller storage, `eval_many(&self, &[f64],
    &mut [Point3])`.
  - `Result<(), E>` plus mutation only where the value is unchanged on `Err`.
  - The pin's `base_edit` comment ("puts s back as it was when the edit fails") shows the C++ needed
    a rollback. A by-value edit (`fn reversed(&self) -> Self`) has nothing to roll back.

**X10. Nested vectors and fixed sizes.**

- **Pin.**
  - Partials are `Vec<Vec<Vec3>>` (`surface/surface/p2.rs:2190` `evaluate`), and curve derivatives
    `Result<Vec<Vec3>>`.
  - Nets are `Vec<Vec<CPoint>>` (`nurbs/srf_basic.rs:241`).
  - The pin's second evaluator, `eval/`, already moved to fixed shapes (`Partials`,
    `derivs -> Option<(Vec3, Vec3, Vec3)>`, `eval/surface.rs:258`).
- **Proposal.**
  - Jets of fixed order, `CurveJet<const D: usize>` and `SurfaceJet<const D: usize>`, triangular
    where only `i + j <= D` is wanted (the pin's `only_upper_half`).
  - A net is one `Vec` with row-major indexing behind a typed accessor.
  - Section 9 gives the measurement. [Ruled 11-27, D3: the measurement is unverified until A1's
    benchmark is rerun fairly in the new `scx-bench` crate, with every input `black_box`ed and
    every output consumed whole (review-63 M4).]
  - `SurfaceJet<D>` stores the triangle `i + j <= D` only, not the full square, which at order 2
    would waste a third (review-63 M4).

**X11. Degrees and upstream names.**

- Angles are radians (57 M4).
  - The ellipse's inversion returns "the ellipse angle in degrees ... in `[0, 360)`"
    (`curve/ellipse/p2.rs:1411`).
  - Curve parameters of circles and ellipses are radians.
- No `_simple`, `_it`, `2`/`3` arity suffixes, and no `Sm` names in identifiers.
  - The upstream name stays only in a doc line (57 §5.2).
  - In `scx-surface`, `_simple` (352 lines) is almost entirely the volume code
    (`surface/b_spline_volume_n_lib.rs:134` and on), which comes in step 5.

**X12. Two evaluators become one.**

- **Pin.** The pin holds two evaluators:
  - the Kernel-bound kind methods;
  - `eval/`, a second one over plain data with no kernel (`EvalCurve`, `EvalSurface`, `Partials`,
    `Domain`, `Collapse`).
  - `eval`'s errors carry `String`s (`EvalError::{Unsupported, Invalid, TooLarge, Unevaluable,
    Degenerate, OffRange}`).
- **Proposal** (01 §7.1, review-56 E3). Each kind evaluates itself, once.
  - `eval/`'s plain-data shapes are the better starting point for the signatures, since they were
    written without the kernel.
  - The proposal keeps `eval/`'s caps and lessons:
    - `MAX_SPLINE_DEGREE`, `MAX_EVAL_KNOTS`;
    - collapse and pole detection;
    - "onto knots".
  - Its string causes become typed causes in each crate's own enum (X13).
  - No type here is named `EvalError`: that name is `scx_algo::error::EvalError`, the solver's
    trait (62 D8). The pin's `eval::EvalError` causes go to `CurveError`, `SurfaceError` and
    `NurbsSurfaceError`. [Ruled 11-27, L3.]

**X13. Errors follow 62 §10.** [Ruled 11-27, M2. Review-63 §2 calls this gap "X10"; 63's X10 is the
nested-vector ruling, so the error ruling is added here.]

- **Every cause has a kind**, computed from the cause (01 §5.1). The causes named in this note:

| Cause | Crate | Kind | Why |
|---|---|---|---|
| `TooLarge { cap }` (degree, knot count over `MAX_SPLINE_DEGREE`, `MAX_EVAL_KNOTS`) | curve, surface, nurbs-surface | `Exhausted` | a fixed cap is a typed limit cause (62 M1) |
| `Degenerate` (zero radius, parallel frame axes, a zero-length line) | curve, surface | `Refused` | 62 §3.1 |
| `OutsideDomain` | curve, surface | `Refused` | the caller asked outside the domain |
| `BadNet`, `BadKnots`, `BadWeights` | nurbs-surface | `Refused` | caller's data |
| `AnalyticNonSimilarity { kind }` (D4, until step 5) | brep (`TransformCause`) | `Unsupported` | a method we do not yet build |
| `NeedsFit { edge }` (a writer needs a fitted pcurve before step 5) | brep | `Unsupported` | D10 |
| `MissingCurve { edge }` (a non-collapsed edge with no curve at `finish`) | brep (`FinishCause`) | `Defect` | D8: only our own operations reach it |
| `Truncated`, `BadMagic`, `BadTag` | record | `Refused` | the bytes are the caller's input |
| `BadVersion { found }` | record | `Unsupported` | valid data from a newer writer that this reader does not build |
| `Invalid(Box<FinishCause>)` | record | inherits the finish cause (62 D11) | the decoder pre-checks curve presence, so `MissingCurve` is never reached from bytes |
| `Solve(SolveError<..>)` inside an iterative routine | curve, surface, ssi | inherits | 62 D11 |
| budget ran out, `Stop` fired | all iterative routines | `Exhausted`, `Cancelled` | 01 §6.3 |

- **Frames** (62 D12):
  - transparent: `CurveError::Solve`, `SurfaceError::Solve`, `SurfaceError::Curve` (a curve
    error met while evaluating a surface's own curve), `NurbsSurfaceError::Nurbs` (the
    `scx-nurbs` cause under it);
  - context: `ReadError { at, cause }` (adds the byte offset), `ReadCause::Invalid` (adds "while
    decoding"), `TransformError { entity, cause }` and `Unfinished` (add the entity id),
    `IntersectError { pair, cause }` (adds the surface pair).
- **Bounded or iterative, per routine** (review-63 M2). A closed form is bounded, so it is
  infallible or returns its `Refused` causes only. An iterative routine takes a budget and `Stop`
  (01 §6.3) and returns a `Result`, metered as the 04 budget row says. In this note:
  - bounded: every analytic `point`, `jet`, `normal`, `invert` of line, circle, plane, cylinder
    and sphere; the plane section of analytic kinds; the analytic SSI pairs; `to_nurbs`;
  - iterative: NURBS inversion, torus and cone inversion where the closed form hands over to a
    solve, ellipse inversion (root isolation), `intersect_curve` on any pair that isolates roots,
    knot removal, every future fit.

## 2. `scx-nurbs-surface`

The pin's sources here are `nurbs/srf_*`, `nurbs/nurbs_srf`, `nurbs/iges` (entity 128),
`geom/nurbs_enclosure` and the `Nurbs` record of `eval/spline`. The step-2 slice is evaluation and
the record.

### 2.1 Shapes found

| Shape | Where (READ) | Count (MEASURED) |
|---|---|---:|
| Net as `Vec<Vec<CPoint>>`, sizes as NURBS-book `n, m, p, q, r, s` | `nurbs/srf_basic.rs:241` `zero_net`, `:255` `Surface::new(net, p, q, u, v)`, accessors `:347`-`:383` | `Vec<Vec<` 68 |
| `NOW` weight sentinel for "not rational", checked point by point | `nurbs/srf_basic.rs:230` `rationality_is_uniform` (`first.w != NOW`) | `NOZ`/`NOW` 116 |
| Checked index returning `Result` for a programming error | `nurbs/srf_basic.rs:394` `cpt(i, j) -> Result<CPoint>`, `:406` `cpt_mut` | — |
| Partials `Vec<Vec<Vec3>>` with `EvaluateFlags` | `nurbs/b_spline_surface_n_lib/p2.rs:176` (`u_side`, `v_side`, `only_upper_half`, `non_zero_tangents`) | 18 |
| Write helpers with an `op: &'static str` and an out slice | `nurbs/index.rs:52` `set`, `:100` `put`, `:113` `put2`; `nurbs/basis_basic.rs:137`, `:152` | `&mut [` 44 |
| In-place knot edits returning `Result<()>` | `nurbs/basis_basic.rs:424` `reparam(knots: &mut [f64], ..)`, `:458` `compatible_knot_array(&mut [Vec<f64>])` | `Result<()>` 64 |
| bool parameters | through the surface tools | 55 |
| Mutable control-point access on a long-lived surface | `nurbs/srf_basic.rs:461` `displace_cpts`, `:503` `u_knots_mut` | — |

### 2.2 Proposed shapes

- **The net is one buffer with typed sizes.**
  - `Net<P> { rows: u32, cols: u32, points: Vec<P> }` with `get(i, j) -> Option<&P>` and
    `rows()`/`row(i)` iterators.
  - The pin's `cpt(i, j) -> Result` becomes `Option`, because a bad index is the caller's bug and
    is not an error to propagate. Internal loops iterate and do not index (57 M8).
- **Rationality is decided once, in the type.**
  - `NurbsSurface` holds `points: Net<Point3>` and `weights: Option<Net<f64>>`. Alternatively it
    takes the `scx-nurbs` choice; see the next item.
  - `None` is polynomial. A mixed net cannot exist, so `rationality_is_uniform` disappears.
- **Follow `scx-nurbs` exactly.**
  - The knot vector type, `Approach`, `from_homogeneous` (F12) and the degree caps are `scx-nurbs`'s.
  - This crate adds the tensor product and invents nothing parallel.
  - If `scx-nurbs` stores homogeneous points, so does the net.
- **Evaluation.**
  - `point(u, v) -> Point3`.
  - `jet::<D>(u, v, [Approach; 2]) -> SurfaceJet<D>`: triangular, true derivatives, no flags.
  - `eval_many(&self, uv: &[Point2], out: &mut [Point3])` as the one batched out-param.
  - Outside the knot range the end pieces continue, as the testkit's reference does (59 §8 item 4).
- **Edits by value.** `with_knots_inserted`, `elevated`, `split_at`, `reversed`, `swapped`
  (u and v swapped) and `transformed(&Affine)` return a new surface.
  - `displace_cpts` becomes `with_points_moved(&[((u32, u32), Vec3)])`, or a builder used before
    the surface is shared.
- **No `op: &'static str` on internal helpers.**
  - Errors are typed, and the location of an internal-invariant failure is a `Defect` with a
    `Location` (57 §5.1).
  - The helpers that exist only to avoid indexing disappear with the flat net.
- **The `Nurbs` record** (from `eval/spline`) is the exact stored form of a NURBS surface:
  - degrees, knots with multiplicities, points, optional weights;
  - validated by `NurbsSurface::new -> Result<_, NurbsError>`.
  - `scx-record` encodes this type. There is no second record type.

### 2.3 Sketch

```rust
pub struct Net<P> { rows: u32, cols: u32, points: Vec<P> }        // row-major
impl<P> Net<P> {
    pub fn new(rows: u32, cols: u32, points: Vec<P>) -> Option<Self>;
    pub fn get(&self, i: u32, j: u32) -> Option<&P>;
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[P]>;
}
pub struct NurbsSurface { u: scx_nurbs::Knots, v: scx_nurbs::Knots, net: Net<Point3>, weights: Option<Net<f64>>,
                          cells: SurfaceCells /* OnceLock: box tree, enclosure */ }
pub struct SurfaceJet<const D: usize> { /* d[i][j], i + j <= D; d[0][0] is the point */ }
impl NurbsSurface {
    pub fn new(u: Knots, v: Knots, net: Net<Point3>, weights: Option<Net<f64>>) -> Result<Self, NurbsSurfaceError>;
    pub fn point(&self, uv: Point2) -> Point3;
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<D>;
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]);
    pub fn domain(&self) -> Extent2d;
    pub fn split_at(&self, axis: ParamAxis, t: f64) -> Option<[Self; 2]>;
    pub fn with_knot_inserted(&self, axis: ParamAxis, t: f64, times: u32) -> Self;
    pub fn elevated(&self, axis: ParamAxis, by: u32) -> Self;
    pub fn transformed(&self, a: &Affine) -> Self;
}
pub enum ParamAxis { U, V }   // the uv counterpart of 57 M8's Axis
```

### 2.4 Truth references missing

- **Surface derivatives of order 3 and up.** The testkit computes them but tests only to order 2
  (59 §7). Add the exact tests before this crate claims `jet::<3>`.
- **Tensor-product knot insertion and elevation.** The curve versions exist (Boehm, blossoms).
  - The surface versions are the curve versions applied row by row and column by column.
  - The reference should be exactly that composition, plus the check that the surface is unchanged
    at exact rational sample points.
- **Splitting.** Exact equality of both halves to the original on their domains, in rationals.
- **IGES 128 round trip.** Parse(write(s)) == s on generated surfaces. This is an invariant, not a
  reference.

### 2.5 Builder tasks

| Task | Work | Depends on |
|---|---|---|
| NS1 | `Net`, `NurbsSurface::new` with its validation, `NurbsSurfaceError` | `scx-nurbs` landed |
| NS2 | `point`, `jet`, `eval_many`, difftest per function against `srf_*` evaluation | NS1; testkit order-3 tests |
| NS3 | by-value edits: insert, elevate, split, reverse, swap, transform | NS1 (parallel to NS2) |
| NS4 | enclosure (`geom/nurbs_enclosure`) as a cell | NS2 |
| NS5 | IGES 128 | NS1 (parallel) |

### 2.6 NS1 as built (11-27)

Proposed for review, not yet ruled.

- **Sizes are `usize`, not `u32`.** The sketch's `u32` saved nothing: every count is compared with
  `KnotVector::len()` and slice lengths, which are `usize`, and a `u32` would add a conversion
  (and a failure) at each. The caps that matter are the knot vector's degree caps.
- **`Net::new` returns `Result<Net, NurbsSurfaceError>`, not `Option`.** A size that does not
  fill the net is caller data, so it is refused with a cause (`NetCause::Size`), not dropped.
  A net is never empty; a ragged net cannot be expressed.
- **`NurbsSurface<const D: usize>`**, as `NurbsCurve<D>`: points `[f64; D]`, so 2-D (uv) and
  4-D uses need no second type. `Net<[f64; D]>` holds the points; a rational surface also keeps
  `w P` and the weights (as `NurbsCurve` keeps its homogeneous form, F12), built once.
- **Checks, in order, each once:** each knot vector refuses a break (`check_continuous`, the
  curve rule of ruling 2: every surface is C0 in each direction); the net's rows and columns
  match the two bases; every point finite; the weights' net has the points' shape; every weight
  finite and positive; every `w P` finite. `from_homogeneous` checks `w P` first and the derived
  `P = w P / w` last. Weights have no range: scaling every weight leaves the surface unchanged,
  so the pin's `[1e-3, 100]` is not a property of a surface.
- **No `TooLarge`.** The degree caps live in `KnotVector::new` (`NurbsError::Degree`), which a
  caller builds before the surface; nothing else here can be too large. The error has three
  variants (`BadNet`, `BadKnots`, `BadWeights`), not the four of X13; `TooLarge` joins if a
  cap of its own appears (a net size limit, say).
- **The kinds.** `BadNet` and `BadWeights` are `Refused`; `BadKnots` is a context frame and
  inherits the `scx-nurbs` kind (62 D11). The workspace's `ErrorKind` has the classes before
  62 until the error migration lands, so `Refused` is `InvalidInput` there for now.
- **`scx-algo` is not a dependency yet.** Nothing in NS1 uses it, nor in NS2 (below); it is
  added with the first use.
- **No knot fixup.** The pin's `BSplineSurface::from_data` snaps near knots together
  (`fixup_knot_vector`). scx stores the doubles given, exactly: the stored form is the record.

Difftest `t_nurbs_surface` (construction against the pin's `Surface::new` then `is_valid`; its
`from_data` checks nothing more). Where acceptance differs, scx is checked against the
definition (pin defect `nurbs_surface_new`):

| Input | pin | scx | Why scx is right |
|---|---|---|---|
| unclamped ends | refuses | builds | the basis is defined on the domain; scx-nurbs ruling |
| degrees 18 to 31 | refuses | builds | the ruled cap is 31 (scx-nurbs) |
| weights outside `[1e-3, 100]` (one at 1e-4, 1e3, 1e300; all scaled by 1e-6; the testkit's near-zero weights) | refuses | builds | positive weights of any scale give a surface |
| a NaN weight | builds | refuses | `w < 1e-3 \|\| w > 100` is false for NaN |
| a NaN interior knot | builds | refuses | `validate` compares with `<` |
| a NaN or infinite control point | builds | refuses | nothing checks coordinates |

Both refuse: degree 0 and 32, a break, extra or missing rows and columns, ragged nets, knot
counts that do not match, decreasing knots, infinite end knots, an empty domain, end
multiplicity `p + 2`, zero, negative and infinite weights, and `w P` that overflows.

#### NS2 as built (11-27)

Evaluation in `crates/scx-nurbs-surface/src/eval.rs` (a new module; `surface.rs` untouched),
`SurfaceJet` in `scx_num::jet::surface`, `BasisJet` in `scx_nurbs::basis`. Decided by analysis
and an adversarial pass; ruled 11-27 (all accepted).

- **The API follows `NurbsCurve`.** `point(uv: [f64; 2], side: [Approach; 2]) -> Result<[f64; D]>`,
  `jet::<N>(uv, side) -> Result<SurfaceJet<[f64; D], N>>` and
  `eval_many(&[[f64; 2]], side) -> Result<Vec<[f64; D]>>`. The parameter is a pair, not two
  arguments, so a uv point passes through as one value; one `Approach` per direction, as the
  pin's two `Side`s. `N` is the highest total order, inclusive (`jet::<2>` holds the second
  partials); the curve's `derivs::<K>` counts rows. The surface's `N` names what the jet holds
  (`SurfaceJet::order()`), and a row count would be `(N + 1)(N + 2) / 2`, which no const
  parameter can be.
- **`SurfaceJet<V, const D>` (the shape SU1 builds on).** The triangle `i + j <= D` only (X10,
  ruling D3): `from_fn`, `order`, `point`, `get(i, j) -> Option<V>`, `get_mut`, `iter` (in
  `from_fn`'s order, `i` outer) and `map`. Storage is `point`, `u: [V; D]` (the `S^(i,0)`) and
  `rest: [[V; D]; D]` (`j >= 1`), since stable Rust cannot size `[V; (D+1)(D+2)/2]`: exact at
  order 1, 7 for 6 at order 2, 13 for 10 at order 3; the spare entries are never returned.
  It is generic over the element because SU1's certified jets are interval vectors and NURBS is
  dimension-generic (`[f64; D]`); `map` changes element (to an interval jet, say). It has no
  arithmetic: a jet is a container, and the operations on it (normals, curvature) are SU1's.
- **`BasisJet<D>` (scx-nurbs).** One direction's basis values and derivatives to order `D` on
  the stack: `values: [f64; FULL]` and `derivs: [[f64; FULL]; D]` (row `k - 1` the `k`-th
  derivatives), read by `row(k)` for `k <= D`. The split exists because `[[f64; FULL]; D + 1]`
  is not stable Rust; `BasisDerivs<K>` keeps its row count. `derivs_into` (A2.3) now writes the
  values and the derivative rows separately, and `BasisDerivs::at` and the curve's `derivs_n`
  call it through `split_first_mut`: the arithmetic and its order are unchanged, so the curve's
  bits are unchanged (its tests and difftests pass as before). A test checks `BasisJet<D>`
  equals `BasisDerivs<D + 1>` bit for bit.
- **Rational partials by A4.4**, the quotient rule on the jets of `w P` and `w`, not by
  rational basis functions: one tensor sum per jet, shared by every partial. The point is
  `(w P)(u, v) / w(u, v)`, a division, where the pin multiplies by the reciprocal: one rounding,
  not two. The binomials are an exact f64 loop (`n <= MAX_ORDER`).
- **The jet's point is the point, bit for bit.** `tensor_jet` accumulates in `tensor_point`'s
  order, so `jet::<N>(uv).point() == point(uv)` for every `N`; a test holds it at orders 0, 1, 3.
- **`eval_many` reuses each basis** whose parameter is bit-equal to the previous point's (a grid
  row or column evaluates one basis per new value). The first error ends the batch: a batch is
  a value, not a list of outcomes; a caller that wants partial results calls `point` per entry.
- **Errors.** `Basis { axis, cause }`: a frame over the `scx-nurbs` cause (a non-finite
  parameter, a degenerate span, an order above `MAX_ORDER`), its kind inherited. `Degenerate
  { uv }`, `Refused`: a value of the result is not finite (an extrapolation that overflows). The
  pin returns non-finite points. Outside the domain the end pieces continue, as the curve does.
- **No `scx-algo`.** Nothing uses it; NS1's line that NS2 would add it is superseded. `cargo
  xtask layers` passes.

Adversarial pass:

- *A `SurfaceJet` of the square (`[[V; D + 1]; D + 1]`) is simpler and SU1 could ignore the
  extra corner.* The corner (`i + j > D`) is not computed, so the square would hold values that
  are not partials or cost the work of computing them; X10 and D3 rule the triangle. `get`
  returning `None` past the triangle makes the shape checkable.
- *Generic `V` is speculative.* SU1's interval jets are the first second user, and the evaluator
  is already generic over `D`; a `[f64; 3]`-only type would be a second type on SU1's first day.
- *`N` inclusive breaks symmetry with the curve's `K`.* It does; the curve counts rows because
  its jet is an array of `K` rows. For a triangle the order is the only natural size, and the
  doc comments name it. Ruled 11-27: `N` stays inclusive.
- *Dividing by `w` rather than multiplying by its reciprocal differs from the pin.* It differs in
  the last bit at most and is the more exact; the difftest holds both sides to the exact truth
  (truth over pin).
- *A4.4 loses accuracy where `w` is near zero.* So does every form; the difftest's tolerance
  propagates the bound through the quotient rule (below), and a near-zero weight surface passes
  against the exact truth with scx no further from it than the pin.
- *Reusing a basis in `eval_many` could change results.* Reuse is only on bit-equal parameters
  with the same side, so the basis is the same value; the difftest checks `eval_many` against
  `point` bitwise.

Difftest `t_nurbs_surface_eval` (4 tests, 1.5 s debug), against the pin's `Surface::eval` and
`Surface::derivs` with `upper` (the triangle), on `t_nurbs_surface`'s generated surfaces at the
domain's corners, a rounding error inside them, interior knots and their neighbours, points
outside and random ones, the four side pairs in turn. Where the two disagree, scx is checked
against the testkit's exact rational truth (`SurfaceData::derivs`), within a tolerance from a
forward error model, and must be no further from the truth than the pin (`closest`):

| Test | Inputs | Exact | Within tolerance | Pin defects | Mismatches |
|---|---|---|---|---|---|
| `nurbs_surface_point` | 1,118 | 548 | 204 | 366 | 0 |
| `nurbs_surface_jet` (`jet::<2>`) | 902 | 45 | 561 | 296 | 0 |
| `nurbs_surface_jet4` (`jet::<4>`) | 470 | 55 | 263 | 152 | 0 |
| `nurbs_surface_eval_many` | 1,118 | 447 | 319 | 352 | 0 |

- **Within tolerance** is the harness's agreement with the pin. **Pin defects** are every other
  disagreement: the surfaces the pin does not build (NS1's table), the domain's corners, which
  the pin's `basis_eval` refuses from the side past the end, and answers further from the pin
  than the harness allows. Each is checked against the exact truth: refused exactly when the
  data is not a surface or a parameter is not finite, else within the tolerance below and no
  further from the truth than the pin.
- **Point tolerance**: `4 m eps` of the absolute scale `Σ |N_i N_j| w |P| / |Σ N_i N_j w|`
  inside the domain, `max(m, 4) m eps` outside it (the basis grows like the distance to the
  power of the degree), `m = p + q + 2`.
- **Jet tolerance**: each partial's bound `E(k, l)` is propagated through A4.4,
  `E(k,l) = t_n (s_n + Σ c |S^(k-i,l-j)|) + Σ c E(k-i,l-j)`, `c = C(k,i) C(l,j) |w^(i,j)| / |w|`,
  `t_n = 1e-12 · 10^n` at order `n`: a near-zero weight makes the higher partials ill-conditioned
  and the bound follows.
- **The pool is cut to what the exact truth can afford**: the jets keep every case of degree
  `p + q <= 7` (order 2) or `<= 4` (order 4), and the higher ones at the corners only; inside an
  end at zero the rounding error is `eps b`, not `5e-324` (a degree-31 truth there took 5 s).
  `eval_many` is checked bitwise against `point`, not against the truth again.
- **The checks bite**: scaling one `v²` term by 0.999 gave 540 `jet` and 251 `jet4` mismatches.

Performance (release, a local timing loop, best of 7 over 200,000 calls at 1,024 scattered
parameters, 8 × 9 net; the machine was loaded, so the ratios move by about 0.2 between runs):

| Surface | `point` / `eval` | `jet::<1>` / `derivs(1, 1)` | `jet::<2>` / `derivs(2, 2)` | `eval_many`, per point |
|---|---|---|---|---|
| bicubic, polynomial | 99 / 67 ns (1.56 to 1.66) | 349 / 135 ns (2.1 to 2.5) | 422 / 425 ns (0.98 to 1.5) | 130 to 140 ns |
| bicubic, rational | 122 / 71 ns (1.58 to 1.75) | 386 / 187 ns (1.95 to 2.8) | 527 / 944 ns (0.53 to 1.41) | 124 to 141 ns |
| biquintic, rational | 152 / 115 ns (1.28 to 1.34) | 407 / 310 ns (1.29 to 1.35) | 569 / 1,750 ns (0.33) | 169 to 198 ns |

The second partials are as fast as the pin's or up to 3 times faster. The point and the first
partials are slower, by a cost that does not grow with the degree (40 to 60 ns for the point,
about 200 ns for `jet::<1>`): likely the stack scratch sized for the degree cap (`FULL = 32`)
in the basis and A2.3, initialized on each call, where the pin sizes its buffers by the
degree. That is a hypothesis, not yet measured. Under the perf rule the change does not wait for
it: the follow-up is a parallel branch that sizes the scratch by the degree, or keeps it
uninitialized behind a written-before-read proof, with before/after numbers from this loop.

Follow-up (nsurf-2b, 11-27): below degree `SMALL = 8` the basis values and A2.3 now run on
`SMALL`-wide scratch (the `FULL`-wide path stays for higher degrees; a unit test checks the two
are bit-equal for every degree below `SMALL`, both sides, inside and outside the knots). Same
loop, before / after, scx ns (scx/pin):

| Case | `point` | `jet::<1>` | `jet::<2>` | `eval_many` per point |
|---|---|---|---|---|
| bicubic, polynomial | 116 / 91 to 116 (1.43 / 1.42 to 1.78) | 365 / 195 to 208 (2.7 / 1.78) | 435 / 262 (1.0 / 0.72) | 145 / 110 to 128 |
| bicubic, rational | 128 / 86 to 104 (1.57 / 1.43 to 1.54) | 388 / 218 to 230 (1.97 / 1.35) | 545 / 343 to 354 (0.52 / 0.41) | 150 / 120 to 123 |
| biquintic, rational | 190 / 139 to 175 (1.30 / 1.20 to 1.36) | 485 / 275 to 295 (1.29 / 0.92) | 670 / 437 to 460 (0.34 / 0.28) | 208 / 167 to 190 |
| degree 9, rational | 386 / 326 to 332 (1.02 / 1.04 to 1.10) | 796 / 704 to 805 (0.86 / 0.92 to 1.03) | 0.22 / 0.22 to 0.24 | 403 / 313 to 340 |

Degree 9 takes the `FULL` path, so it is unchanged within noise (the "before" ran at a higher
machine load). `jet::<1>` lost most of its fixed cost: the hypothesis held there. The point did
not move in ratio (still about 1.4 to 1.5 at low degree), so its fixed cost is elsewhere; the
candidates (span search, moving `Result`s, the 256-byte `Basis` arrays) are open and unmeasured.
Clipping the finiteness check in `Basis::at` to the live `p + 1` values was tried and gave
nothing beyond noise, so it is not in.
`eval_many` costs about what `point` does at scattered parameters (no basis is reused there).
On a grid it evaluates one basis per new parameter value.

#### NS2c: the point's fixed cost and grid evaluation (11-27)

Branch `nsurf-2c`. Two items from the NS2 follow-ups: the point's fixed cost (1.4 to 1.8x the
pin at low degree after nsurf-2b) and 65 #2, grid evaluation with basis reuse. Every result is
the same bits as before; the counts are local (callgrind on a timing loop, not committed).

**Profile first.** Instructions per `point` on the 12 × 13 bicubic net, at 1,024 scattered
parameters (the pin's `Surface::eval` 1,575 polynomial and 1,584 rational):

| Step | Polynomial | Rational |
|---|---|---|
| main after nsurf-2b | 2,014 | 2,243 |
| finiteness check over the `p + 1` live values, not all 32 | 1,898 | 2,127 |
| A2.2 leaves `n[p+1..]` as it is (it is never read; `Basis` already starts at zero) | 1,864 | 2,093 |
| `#[inline]` on `KnotVector::basis` and `Basis::at` (non-generic, so no cross-crate inlining without it) | 1,836 | 2,065 |
| A2.2's inner loop indexed, not zipped (`right.zip(left.take(j).rev())` over `n.take(j + 1)`) | 1,510 | 1,739 |
| the `Basis` built in place, not from a `values` array moved in | 1,443 | 1,672 |
| the rational point's two tensor sums (`w P` and `w`) in one pass over the net | 1,443 | 1,548 |
| `#[inline]` on `Basis::first` and `Basis::values` | **1,412** | **1,514** |

So the fixed cost was the basis, not the span search (about 60 instructions a direction,
as the pin's `find_span`) and only partly the moves: the iterator adapters around A2.2's
inner loop cost more than its arithmetic at degree 3 (326 instructions a point), then the
32-wide finiteness check (116) and the copies of the 264-byte `Basis` (about 130, of which one
copy a direction is left: `Ok(basis)` still copies it out). The point now runs fewer
instructions than the pin's. The arithmetic and its order are unchanged in every step: the same
products, quotients and sums in the same order, so the bits are the same (the SMALL-against-FULL
test, the scx-nurbs and scx-nurbs-surface unit tests and the `t_nurbs` difftests pass as
before). The curve's `point` shares `values_into`, so it gains too; `curve.rs` is untouched.
Earlier (nsurf-2b) the clipped finiteness check showed nothing in wall time at a machine load of
about 10; the instruction count shows it, so it is in.

**Grid evaluation: a separate entry point.** `eval_grid(us: &[f64], vs: &[f64], side, out:
&mut [[f64; D]]) -> Result<()>` writes `out[i * vs.len() + j] = point([us[i], vs[j]], side)`,
`u` outer, computing each `v` basis once (kept in a `Vec<Option<Basis>>`, one allocation a
call) and each `u` basis once per row: `us.len() + vs.len()` bases for `us.len() · vs.len()`
points. The alternative was to have `eval_many` detect a grid in its `uv` list.

- *For a separate entry point.* The grid's structure is in the arguments, so nothing has to be
  detected, and a caller with a grid (tessellation, sampling, the projection's start grids)
  passes `nu + nv` numbers, not `nu · nv` pairs. `eval_many`'s contract stays simple.
- *Against.* A second function to learn, and a caller holding an arbitrary list that happens to
  be a grid gets only `eval_many`'s reuse.
- *Adversarial on detection.* Recognising a grid in a list costs a pass and a hash or sort of
  the distinct values, and still needs a store of `nv` bases. It is wrong for lists that are
  almost grids. It would also make `eval_many`'s cost depend on its input's shape in a way its
  signature does not show. Rejected.
- *Error order.* `eval_grid` errors with the first failing point in row order, as `point` at
  each would. A `v` basis that fails is kept as `None` and rebuilt when its first point is
  reached, so a degenerate point earlier in row 0 still wins. A unit test checks both a failing
  `u` (row 1) and a failing `v` (column 1).

`eval_many` keeps its previous-point reuse (it is free and covers a row or column walked in
order), now updating the two bases in place rather than moving them through an `Option` each
step.

**The 04 output shape.** Both write into a caller's `&mut [[f64; D]]` and panic on a length
mismatch (`uv.len()` for `eval_many`, `us.len() * vs.len()` for `eval_grid`, with an overflowing
product treated as a mismatch), as CU2's `Curve::eval_many` and the 04 11-27 ruling. They return
`Result<()>`: on an error `out` holds the points before the failing one and the rest is left as
it was. This changes `eval_many` from `-> Result<Vec<_>>`; its callers were the unit test and
the difftest.

**Wall time** (same loop, ns a point; the machine load was 10 before and 13 after, so the
after numbers are if anything high):

| Case | `point` scx/pin | `eval_many` scattered | `eval_many` 32 × 32 grid | `eval_grid` 32 × 32 | pin `eval` on the grid |
|---|---|---|---|---|---|
| bicubic, polynomial | 1.41 / 1.27 | 132 / 92 | 98 / 61 | 22 | 76 / 72 |
| bicubic, rational | 1.53 / 1.31 | 132 / 98 | 98 / 66 | 28 | 72 / 72 |
| biquintic, rational | 1.30 / 1.15 | 186 / 152 | 135 / 100 | 43 | 124 / 124 |
| degree 9, rational | 1.02 / 0.91 | 392 / 331 | 270 / 213 | 92 | 350 / 347 |

`eval_grid` is 0.27 to 0.39 times the pin's point-by-point grid, and 3.5 to 4.2 times faster than
scx's own point per point, above 65 #2's "about 2x" estimate: the bases dominate a point at
these degrees. The point's wall-time ratio fell less than its instruction ratio (now 0.90 and
0.96 of the pin's instructions), so what is left is likely latency rather than work (a
hypothesis, not measured): the A2.2 divisions are a dependent chain, and scx still clears and
copies a 256-byte `Basis` a direction. Shrinking `Basis` below `FULL` would need a degree-split
type; at this ratio it is not pursued.

#### NS3 as built (11-27)

Proposed for review, not yet ruled. The code is in
`crates/scx-nurbs-surface/src/edit.rs`.

- **Names follow the curve methods, not the sketch.** `insert_knot(axis, t, times)`,
  `elevate(axis, times)`, `split(axis, t)`, `reversed(axis)`, `swapped()` and `transformed(&Affine)`.
  The sketch's `with_knot_inserted`/`elevated`/`split_at` would give the surface a second
  vocabulary for the same edits as `NurbsCurve::insert_knot`/`elevate`/`split`. All take `&self`
  and return a new surface; none edits in place.
- **Each edit returns `Result`.** The only infallible one is `swapped`, which is
  `#[must_use]`, as is every edit. A curve's refusal on a line is wrapped in one context frame,
  `NurbsSurfaceError::Edit { axis, cause }`, which inherits the curve cause's kind (62 D11). So
  a parameter outside the domain, a multiplicity above the degree and elevating unclamped knots are
  `Refused`; a degree above 31 is `Exhausted`; `Unstable` elevation is `Unsupported`. A line of a
  valid surface that is refused as a curve, or edited lines that do not form one net, are
  `Defect`.
- **One edit per direction; no `Direction::UV`.** Along v each row of the net is a curve and gets
  the curve edit. Along u the surface is swapped, edited along v and swapped back. The two
  directions therefore share one path and the curves' algorithms, checks and accuracy. The edited
  rows share one knot vector, so the result is assembled without validating it again
  (`from_valid_parts`, crate-private).
- **`times == 0` still checks.** `insert_knot(axis, t, 0)` and `elevate(axis, 0)` refuse the
  arguments that the edit would refuse, then return a copy. Zero is not a way around the checks.
- **Reversal negates the parameter** (changed by SU7, §4.9, to agree with the curves'
  `C(-t)`, the 04 CU5 row). As built in NS3 it reflected the knots about the domain, `t` to
  `a + b - t`, which rounded each knot and could have no form in doubles (`Reversal`,
  removed). Now the knots of `axis` are negated in reverse order and the domain `[a, b]`
  becomes `[-b, -a]`, exactly and for every surface. The pin reflects about `U[0] + U[m]`
  (pin defect `nurbs_surface_reversed`, which now checks the negation).
- **Transform is affine only, on `NurbsSurface<3>`.** Control points map through `Affine::apply`
  and the weights are kept. A projective map does not map a NURBS surface through its control
  points with the weights kept. The pin's 4x4 accepts a projective row 3 and divides each point
  by it, which gives a surface that is not the image. A non-finite coefficient is refused as
  `AffineNotFinite`; a mapped point or `w P` that overflows is `Overflow` (`Unsupported`). main's new
  `scx_num::frame::Frame` does not fit here: it is orthonormal, and a net maps under any affine
  map, including shears and scales.
- **Accuracy is the curves'.** Insertion and splitting match the exact edits within `1e-13`,
  comparing homogeneous points relative to the largest weight. Elevation matches within `1e-12`
  below degree 9. From degree 9, the curve elevation's known error of a few `1e-12` (05,
  `scx-nurbs`) passes through unchanged, at about `1.3e-12` in points and `3e-12` in control
  points. It is not fixed here.
- **Rational control points are compared in homogeneous form.** A control point whose weight is
  orders below its neighbours' is ill-conditioned (`P = w P / w`): it differs from the exact
  point by up to about `1e-8` while the surface agrees to `1e-13`. Tests and pin-defect checks
  compare `(w P, w)` relative to the largest weight.

Difftest `t_nurbs_surface_edit`: 6 tests against `srf_tool::p1::insert_knot`/`split`,
`p2::elevate_degree`, `Surface::reversed`, `swap_uv` and `transform`. The inputs are 140
valid surfaces at degrees (1,1) to (15,1), with every knot and weight kind, clamped and
unclamped.

| Test | Inputs | Exact | Within tolerance | Pin defects | Mismatches |
|---|---|---|---|---|---|
| `nurbs_surface_insert_knot` (`times` 0 to 3; inside a span, at a knot, `1e-9` past it, at the end) | 4,416 | 4,416 | 0 | 0 | 0 |
| `nurbs_surface_split` | 1,136 | 1,136 | 0 | 0 | 0 |
| `nurbs_surface_elevate` (`times` 0 to 3, result degree at most 17) | 1,112 | 1,107 | 0 | 5 | 0 |
| `nurbs_surface_reversed` | 1,120 | 560 | 0 | 560 | 0 |
| `nurbs_surface_swapped` | 140 | 140 | 0 | 0 | 0 |
| `nurbs_surface_transformed` (identity, shear, turn and scale, far translation) | 560 | 242 | 318 | 0 | 0 |

The pin defects:

- **`nurbs_surface_elevate`.** In all 5 cases scx refuses with `Unstable`, at near-coincident
  knots, and the pin answers. Each pin answer has a weight that is not positive, or is more than
  `1e-6` from the original surface when evaluated exactly.
- **`nurbs_surface_reversed`.** These are the 560 unclamped inputs, all of them. The pin moves
  the domain (it reflects about `U[0] + U[m]`). The check confirms that scx keeps the domain,
  flips the net exactly, maps each knot to `a + b - t` within 4 ulp, and, for degrees up to 3,
  is the original surface reversed at a point evaluated exactly.

The transform's 318 inputs within tolerance are rational. The pin computes `P = w P / w`, maps
it and multiplies back, while scx maps its stored `P`, so the two differ only in the last bits.

**High-degree elevation.** This is a finding about `scx-nurbs`, and the curve elevation is not
fixed here. On the degree-15 surfaces, A5.9 gives the same answer as the pin to the bit, and it is
far from the exact elevation:

- `7e-13` to `7e-11` on uniform knots and on knots with multiplicities;
- `2e-11` to `3e-5` on random knots;
- on clustered knots, about `1e81` for the polynomial surface, which `Unstable` does not catch
  (every weight stays positive and every point finite), and `Unstable` for the rational ones,
  where the pin is `1e82` off.

At degree 9 the worst seen is `1.1e-11`. Above the pin's cap of 17, the pin refuses and scx
answers with the same error, so the difftest leaves out elevations to a degree above 17. No
check against the truth can excuse those answers. The curve elevation needs an accuracy bound,
or a refusal, that grows with the degree and the knot spacing. That belongs to `scx-nurbs`.

**Perf (local release loop, the edit alone on prebuilt surfaces, best of 3).**

| Edit | Inputs | scx | pin | scx / pin |
|---|---|---|---|---|
| insert | 4,416 | 1.61 µs | 0.61 µs | 2.66 |
| split | 1,104 | 4.62 µs | 1.34 µs | 3.45 |
| elevate | 1,080 | 2.29 µs | 1.50 µs | 1.53 |
| reverse | 1,120 | 0.47 µs | 0.22 µs | 2.12 |
| swap (the pin's on a clone) | 138 | 0.40 µs | 0.10 µs | 4.15 |
| transform (the pin's on a clone) | 552 | 0.12 µs | 0.76 µs | 0.16 |

The added work has these sources:

- Each line becomes a `NurbsCurve`, which checks it again (`from_homogeneous` and `new`) and
  clones the knot vector.
- Along u, the surface is swapped twice.
- A rational surface keeps three nets (`P`, `w`, `w P`) where the pin keeps one.

A follow-up can recover this without changing the API. It would add a crate-private unchecked
line constructor in `scx-nurbs`, or a strided curve edit, so that u is edited in place. That
would fold the swaps and the per-line checks into one pass.

**NS3b (11-27): the follow-up, done.** The strided edit is the cleaner of the two. An unchecked
constructor would have to be public in `scx-nurbs`, and then a `NurbsCurve` would no longer be
valid by construction. Instead, `scx-nurbs` has a new public type, `CompatibleCurves<D>`. It holds
curves of one knot vector, one polygon after another (Piegl and Tiller's compatible curves: a
net's lines, and later the sections that skinning takes). It is checked as `NurbsCurve` is, and
an error names the index within its curve.

Its `insert_knot`, `split` and `elevate` do the knot work once: the checks, the new knot vector,
the insertion plan and the elevation matrix. Each polygon then goes through the same private
A5.1 and A5.9 code as `NurbsCurve`, so curve `i` of the result is the edit of curve `i`, bit for
bit. A unit test checks this, results and errors, over every knot and weight kind.

The surface edits now copy the lines once, transposed along u, into one `CompatibleCurves`, and
assemble the result from its parts; there are no swaps and no per-line checks. Every surface
edit result is unchanged: a dump of 32,708 results, errors included, is byte-identical before and
after.

Ratios against the pin, measured in one session on a loaded machine (best of 5):

| Edit | before | after |
|---|---|---|
| insert | 2.70 | 1.32 |
| split | 3.49 | 1.44 |
| elevate | 1.53 | 0.90 |

What is left is one copy of the lines, and the `w P / w` and check passes on the way in and out.
Reverse (2.1) and swap (4.1) are copies of three nets and are unchanged.

#### NS4 as built (11-27)

Proposed for review, not yet ruled. The code is in `crates/scx-nurbs-surface/src/enclose.rs`.
The pin's `geom::nurbs_enclosure` encloses every partial `S^(a,b)`, `a + b <= N`, over a box of
parameters as intervals, by a centred Taylor form. NS4 ports it.

- **A method, not a cell.** The table row says "as a cell", but the enclosure depends on the
  box, the order and the point it is taken about, so a per-surface `OnceLock` has nothing to
  hold. Two alternatives were weighed:
  - A cell of each piece's Taylor coefficients. It would save the basis at the middle, but the
    middle moves with the box, so the coefficients would have to be re-centred, a
    `(p + 1)²(q + 1)²` interval pass. That is the same order of work as computing them, and it
    widens every interval. It loses on both counts.
  - A cell of enclosure tables, keyed by boxes. That is a cache across calls, and it belongs in
    the caller's `Scratch` (X4), not in the surface.

  So the API is `partials_over_piece::<N>(span, uv, about)` and `partials_over::<N>(uv,
  continuity, about)` on `&self`, returning `SurfaceJet<[Ivl; D], N>` like `jet::<N>`.
- **`About` replaces the pin's four entry points and `Shift`.** The `About` variants are
  `Origin`, `Point(o)` (the pin's `_about` forms) and `Middle`. The pin's `Shift::Middle` is
  crate-only, but it is the form its callers use, so scx keeps it. The difftest cannot reach
  it, so the unit tests check it against the exact partials. The alternative was a
  `Option<[f64; D]>` argument: it reads well, but it cannot say `Middle`.
- **The rational path is the pin's arithmetic; the polynomial path skips the quotient.** On a
  rational surface the homogeneous partials, the shift, the Taylor sums and A4.4's interval
  quotient run in the pin's order. The difftest bit-checks this: 768 rational calls, all
  bit-identical. On a polynomial surface the pin still divides by a weight function. That
  function's enclosures hold 1 and 0 but are wider than those points, and the pin shifts `o`
  by its weight interval. scx sums the numerator alone and shifts by `o`. Every operand is
  then narrower than the pin's, so by inclusion isotonicity the result lies inside the pin's,
  and it is about 20 to 40 % cheaper. The alternative was to copy the pin's weight lane: same
  bits, but wider and slower for no gain. The pin defect checks containment.
- **Errors are typed and mostly `Refused`.** `EnclosureCause` has five variants:
  - `NotAPiece`;
  - `BoxNotFinite`;
  - `BoxOutside`;
  - `BelowContinuity` (`box_crosses_rough_knot`);
  - `WeightsNotPositive`.

  All are `Refused` except `WeightsNotPositive`, which is `Unsupported`: a smaller box may prove
  the weights positive. The pin returns `Unsupported` for all of them, but they are the
  caller's data (62 D11). The pin also trusts a span index and never checks a box. scx checks
  both, because an unchecked span makes the method index outside the net.
- **The order cap is 31 (`MAX_ORDER`), not the pin's `MAX_K` = 13.** It is refused as `Basis {
  cause: DerivativeOrder }`, as `jet` refuses it. The binomials are exact to 31. The pin's
  `p!/(p-k)!` is a double, exact while it is below 2^53, which covers every degree the pin
  allows (17 or less). scx forms it in `u128` and takes an outward interval past 2^53. The pin
  has no defect here, since it refuses those degrees.
- **Box cuts use the knot vector's spans.** A box is cut at the knot lines strictly inside it,
  and each part is enclosed on its own piece. A zero-width box on an interior knot gives both
  sides' pieces. A crossed line whose continuity is below `continuity` is refused before any
  part is enclosed, u before v. The pin finds a span by its own search, which returns an empty
  span at an unclamped end. scx uses `KnotVector::span` with `Approach::Above`/`Below`, so
  every part is a real piece.
- **Research 66 §4.1 (sizing the scratch by order): adopted.** One buffer holds the basis
  rows, the A2.3 work, the homogeneous partials and the powers. It lives on the stack (512
  `Ivl`, 8 KiB) up to degree 7 each way at `D = 3`, and in a `Vec` past that. This is
  OpenNURBS's shape. It is bit-identical, because only the storage changes. The pin allocates
  several `Vec`s per piece.
- **Research 66's explicit span hint: not adopted.** A span is one binary search per direction
  per call, against `O(p²q²D)` interval products. It would be unmeasurable, and it would add
  an argument that can be wrong. `partials_over_piece` already takes the span for a caller
  that has one.
- **Not ported:**
  - The pin's crate-only `weights: Option<Ivl>` cut, which narrows the weight enclosure to a
    caller's certificate. A cut that no caller can check is unsound in a public method.
  - A public hull helper.
- **CU3's `eval_over`/`Ranged` is not used.** It is direct interval evaluation, not the
  centred form. It is wider on small boxes, and its bits are not the pin's.

Unit tests (`enclose/tests.rs`, 8): every `About`, on near and far surfaces, holds the exact
(rational) partials at sample points of the box, both sides of a knot included. They also
check that a point box is tight (within `1e-11` of the jet's largest entry), that a near `o`
is narrower, that the piece continues past its span, degrees 5, 19 and 25 (the heap path),
every refusal, a weight function not proved positive, and the cuts.

Difftest `t_nurbs_surface_enclose`: 4 tests, 1,592 inputs, against
`nurbs_partials_over_box(_about)` and `nurbs_partials_over_piece(_about)`. The inputs are the
`t_nurbs_surface` surfaces that both sides build. The boxes are a random point, the whole
domain, a narrow box, both ends, and for an interior knot the point on it, a box across it and
a box ending on it. The about points cycle through the origin, a control point and a far
point, and the continuity cycles through 0 to 2.

| Test | Inputs | Exact | Within tolerance | Pin defects | Mismatches |
|---|---|---|---|---|---|
| `nurbs_surface_enclose_over` (order 2) | 400 | 207 | 13 | 180 | 0 |
| `nurbs_surface_enclose_over4` (order 4) | 400 | 206 | 0 | 194 | 0 |
| `nurbs_surface_enclose_point` (order 0) | 392 | 198 | 175 | 19 | 0 |
| `nurbs_surface_enclose_piece` (order 1) | 400 | 200 | 75 | 125 | 0 |

Every input within tolerance and every pin defect is a polynomial surface. The pin defect
(`nurbs_surface_enclose_*`) recomputes the pin's result and requires each scx interval to lie
inside the pin's. It never applies to a rational surface. Where the pin refuses, the defect
accepts any scx answer, and on these inputs both sides refused the same 18 polynomial calls.

**Perf (local release loop, best of 5).** The test is `partials_over::<2>` about a point, on a
box `[0.31, 0.47] × [0.22, 0.61]` of a surface with uniform knots and `p + 4` points each way,
so the box crosses knot lines in both directions.

| Degrees | Rational | scx | pin | scx / pin |
|---|---|---|---|---|
| (1, 1) | no | 4.5 µs | 7.6 µs | 0.59 |
| (3, 3) | no | 41.0 µs | 49.9 µs | 0.82 |
| (3, 3) | yes | 53.3 µs | 50.5 µs | 1.05 |
| (5, 5) | yes | 242 µs | 225 µs | 1.07 |
| (7, 7) | yes | 636 µs | 636 µs | 1.00 |
| (9, 9) | yes | 1,485 µs | 1,482 µs | 1.00 |

The rational path does the pin's arithmetic, so it costs the same. The extra 5 to 7 % at
degrees 3 to 5 is the checks and the span searches. The cost is all interval products, which
round outward through `checked`. A faster form would change the bits: a Horner Taylor sum, or
products of the basis rows before the net sum. That is a separate decision, recorded here and
not taken.

## 3. `scx-curve`

The step-2 slice covers line, circle, ellipse and B-spline, in 3-D and 2-D, with `Curve` and its
dispatch.

### 3.1 Shapes found

| Shape | Where (READ) | Count (MEASURED) |
|---|---|---:|
| Evaluation trait with the kernel, two bools and a `Vec` result | `curve/curve/p2.rs:335` `trait CurveEvaluate`: `evaluate(&self, k, t, num_derivs, from_left, non_zero_tangents) -> Result<Vec<Vec3>>`, `evaluate_point`, `dim() -> u64` | `from_left` 72, `non_zero_tangents` 59, `Result<Vec<Vec3>>` 19 |
| Trait default that walks back to the base class | `is_closed_virtual` calling `p4::is_closed_base` | — |
| A downcast as a trait method | `as_crv_on_surf() -> Option<&CrvOnSurf>` (`curve/crv_on_surf.rs:2306`, `curve/tangent_field.rs:811`) | — |
| Runtime "base method not implemented" | `curve/curve/p2.rs:327` `base_err(BaseCurveMethod)`; arms at `:1598`, `:1805`, `:1919`, `:1941`, `:2008`, `:2051`, `:2097`, `:2138`, `:2209` | `Err(base_err` and kin 267 |
| Kind enum mixing free, on-surface and volume curves | `curve/curve/p2.rs:570` `enum CurveKind` (14 variants; circle inside `Ellipse`) | — |
| Owner, identity, shared memo, owner tolerance on the curve | `curve/curve/p2.rs:628` `struct Curve` | owner 435, identity 103, `owner_zone_tol` 52 |
| In-place transform with an optional scale and a depth | `transform_at_depth(&mut self, k, &Axis2Placement, Option<Vec3>, depth)` (`curve/crv_on_surf.rs:1901`, `curve/crv_in_volume.rs:1402`) | `fn set_` 14, `&mut self` 105 |
| Sentinel output arrays | `[Vec3::UNDEF; N]` at `curve/curve/p1.rs:177`, `p3.rs:175`, `p7.rs:992` | sentinels 552 |
| Degrees for conic angles; zero tolerance means "the context's" | `curve/ellipse/p2.rs:1411` `step_inversion(&self, k, point, classify: bool, zone_tol) -> (f64, Option<CurveLocationType>)` | angle degrees 204 |
| Knots with a status beside them | `KnotsWithStatus` (`curve/crv_on_surf.rs:1111`), `knots_ignoring_status` | — |
| Type test by integer | `is_kind_of(i64)` per class | 359 |

### 3.2 Proposed shapes

- **The kind set for step 2** (01 §7.3 starting points, JUDGED).
  - `Curve` (3-D): `Line`, `Circle`, `Ellipse`, `Nurbs`. `Parabola` and `Hyperbola` follow in step
    5 as separate kinds, each with the pin's parameterisation (D13).
  - Every kind keeps the pin's parameterisation (D13) [Ruled 11-27, H2]: the line's parameter as
    at the pin, the conics' angle in radians from `frame.x` (X11), and the NURBS curve's own
    parameter.
  - `Curve2` (2-D, for pcurves): `Line2`, `Circle2`, `Ellipse2`, `Nurbs2`.
  - The circle leaves the ellipse. A circle has one radius and an exact closest point, and the
    ellipse's foot needs quartic isolation (04, 11-27 testkit row).
  - Curves on surfaces and in volumes are `AnyCurve` in `scx-surface`, not here (01 §2), so
    `as_crv_on_surf` has no counterpart: `AnyCurve::OnSurface(c)` is matched.
- **Evaluation is inherent per kind, then dispatched by `match`.**
  - Every kind has `point(t)`, `jet::<D>(t, side)`, `domain() -> ParamDomain` (periodic, bounded,
    half-open or unbounded, with no number for a missing end: D1) and `eval_many`.
  - Analytic kinds ignore `side`, and the signature still takes it so the dispatch is uniform.
    Section 9 discusses whether an analytic kind should take it.
  - The scalar-generic form required by 01 (`eval<T: Ring>`) is provided for the analytic kinds and
    for NURBS through `scx-nurbs`'s generic evaluation, so interval jets can certify.
  - No `dim()`: 2-D and 3-D are different types.
- **`tangent_dir(t, side) -> Option<UnitVec3>`** replaces `non_zero_tangents` (X5). It returns
  `None` only where every derivative up to the degree vanishes, which is a degenerate curve.
- **Transforms by value.**
  - `transformed(&self, &Affine) -> Transformed`, with `enum Transformed { Same(Curve),
    Nurbs(Curve) }`, or simply `Curve`, with the doc saying which kinds change kind.
  - A circle under a non-similarity becomes an ellipse. That is exact and stays analytic: an affine
    image of an ellipse is an ellipse.
  - Every step-2 curve kind is closed under `Affine`: line to line, circle to ellipse, ellipse to
    ellipse, NURBS to NURBS. So the 11-27 transform rule's "become exact rational NURBS" path is
    not needed for curves in step 2.
  - The image of a conic under a non-similarity has new principal axes, so its angle parameter is
    the old one plus a constant. `transformed` returns that shift with the curve, and
    `scx-brep::transform` applies it to the edge's interval (JUDGED; a line under a non-uniform
    scale likewise rescales its parameter).
  - A mirror keeps the frame right-handed and reverses the kind's sense (D14).
  - The pin's `transform_at_depth`, its scale vector and its depth argument have no counterpart.
- **Inversion** [Ruled 11-27, L5: 57 F6's shape].
  - `invert` returns `Inversion<f64>`, 57 F6's `Inversion { Inside, Outside }` with each variant
    holding the `Foot { at, point, distance }`. F6's `Failed` is gone, because an inversion that
    can fail is iterative and returns `Result` (X13). The foot is always computed, so there is no
    `classify` flag; start, end and seam are read from `at` against the domain.
  - Angles are radians, and the tolerance is a `Length` argument. A zero tolerance is a zero
    tolerance.
  - Closed forms for the line and circle: bounded and infallible. The ellipse isolates a quartic's
    roots, so it takes a budget and `Stop`; its truth is the certified quartic when the testkit
    gains it (section 3.4).
- **`restrict(&self, Extent1d) -> Option<Curve>`** replaces `Trim`/`Copy` pairs. `reversed()`
  replaces `ReverseParameterization`. Both are total for the step-2 kinds.
- **Errors.** One `CurveError` with typed causes, kinded per X13: `Degenerate` (`Refused`),
  `OutsideDomain` (`Refused`), `TooLarge { cap }` (`Exhausted`), `Solve` (transparent, inherits).
  `BaseCurveMethod` has no counterpart (X2). No `String` causes (X12).

### 3.3 Sketch

```rust
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Curve { Line(Line), Circle(Circle), Ellipse(Ellipse), Nurbs(Arc<NurbsCurve>) /* step 5: Parabola, Hyperbola, Procedural */ }
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Curve2 { Line(Line2), Circle(Circle2), Ellipse(Ellipse2), Nurbs(Arc<NurbsCurve2>) }

pub struct Circle { frame: Frame, radius: Length, sense: Sense } // t in radians from frame.x; private fields (01 §7.4)
pub struct CurveJet<const D: usize> { pub point: Point3, pub d: [Vec3; D] }
pub struct Foot<P> { pub at: P, pub point: Point3, pub distance: f64 }
pub enum Inversion<P> { Inside(Foot<P>), Outside(Foot<P>) }  // 57 F6, without Failed

impl Curve {
    pub fn point(&self, t: f64) -> Point3;
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D>;
    pub fn tangent_dir(&self, t: f64, side: Approach) -> Option<UnitVec3>;
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point3]);
    pub fn domain(&self) -> ParamDomain;                    // periodic, bounded, half-open or unbounded (D1)
    pub fn invert(&self, p: Point3, near: Option<f64>, budget: &mut Budget, stop: &Stop)
        -> Result<Inversion<f64>, CurveError>;              // closed-form arms never fail
    pub fn restrict(&self, e: Extent1d) -> Option<Curve>;
    pub fn reversed(&self) -> Curve;
    pub fn transformed(&self, a: &Affine) -> (Curve, ParamShift); // D14 for mirrors
}
```

`Frame` (an origin with an orthonormal right-handed basis: x and z stored, y derived; D12) is needed
here and is not yet in `scx-num` (section 8, P1). `Budget` and `Stop` are 01 §6.3's.

### 3.4 Truth references missing

- **A certified ellipse foot**: quartic root isolation, ruled for when `scx-curve` needs it (04,
  11-27). It is needed for `Ellipse::invert`.
- **An exact NURBS curve closest point** (59 §7).
  - Until it exists, inversion is checked by the residual's orthogonality in double-double, as 59
    suggests.
  - That check proves a stationary point, not the nearest one. A second check is needed: for small
    degrees, compare the minimum distance against a dense exact-rational sample with a certified
    bound on the gap between samples. A Lipschitz bound from the derivative reference suffices.
- **Knot removal** (59 §7), checked by reinsertion.
- **Affine images of conics.** Circle to ellipse under shear and non-uniform scale, checked by
  exact rational evaluation of the image at the images of sample points. This needs the testkit's
  analytic types to accept an `Affine`.
- **Parabola and hyperbola in double-double** (step 5, listed so the slot is reserved).

### 3.5 Builder tasks

| Task | Work | Depends on |
|---|---|---|
| CU1 | `Line`, `Circle`, `Ellipse` (3-D and 2-D): construction, `point`, `jet`, generic scalar, `domain` | P0, P1 (`Frame`), P2 (`ParamDomain`) |
| CU2 | `Curve`/`Curve2` enums, dispatch, Send/Sync compile test | CU1 (after it, not in parallel: review-63 M5) |
| CU3 | `Nurbs` arm wrapping `scx-nurbs` | `scx-nurbs` landed, CU2 |
| CU4 | inversion: line and circle closed form; ellipse through certified quartic | CU1; testkit ellipse foot for the ellipse |
| CU5 | `restrict`, `reversed`, `transformed` | CU2, CU3 |
| CU6 | difftest facade adapters for the slice (facade maps `from_left` to `Approach`, `non_zero_tangents` to `tangent_dir`) | CU2 |

### 3.6 CU1 as built (11-27)

Proposed for review, not yet ruled.

- **`Line { origin, direction }`, the direction unnormalised.** The pin's point, unit vector and
  scale are one vector here: `point(t) = origin + direction t` is the pin's STEP parameter
  (D13) with one rounding fewer. `Line::through(a, b)` is `new(a, b - a)`, so `t = 0` and
  `t = 1` are the ends, as the pin's `from_points`.
- **`point(t)` takes no side.** A point is continuous on every kind of curve (NURBS curves are
  C0, ruling 2), so only `jet` takes `Approach`; analytic kinds ignore it. This narrows
  §3.2's `point(t)` nowhere, and section 9's question stays open for `jet`.
- **One evaluator per kind, `point_over<T>` and `jet_over<T, D>`** (X12): generic over
  `scx_num::ring::Ring` for lines and the new `scx_num::trig::Trig` (`Ring` plus `sin_cos`) for
  conics; `point` and `jet` are their `f64` forms. `Trig` is `fmath::sin_cos` for `f64` and an
  enclosure for `Ivl` (the end values widened past `fmath`'s half ulp, plus every
  extremum inside; a span of a turn or more gives `[-1, 1]`).
- **`scx-num` gains** generic `Vec2<T>`/`Point2<T>` (as `Vec3<T>`), `Frame2 { origin, x }`
  (y = x turned a quarter, right-handed), `trig::Trig`, `jet::CurveJet<D, T>` and `CurveJet2`
  (`jet/curve.rs`, beside NS2's `jet/surface.rs`) and `ParamDomain::TURN` (`[0, 2 pi)`
  periodic).
- **Conic jets are any order `D`.** Derivative `k` is the point's two terms rotated a quarter `k`
  times (`coeffs(k % 4)`), so there is no cap where the pin stopped at three.
- **The sense folds into `b`.** `Sense::Reversed` evaluates with `-b` on the frame's `y`, so the
  frame stays right-handed (D14) and the evaluator has no branch. `y` is recomputed per
  evaluation from the frame (D12 stores x and z), a cross product per call; caching it would
  break X4.
- **Thresholds: the pin's `EFF_ZERO`.** A line's direction shorter than `EFF_ZERO` is refused
  (the pin's `from_points` rule, which `Line::infinite` lacked). A circle's radius not above
  `EFF_ZERO` is refused (`create_canonical_circle`'s rule), and the same rule holds for each
  semi-axis of an ellipse, where the pin's `Ellipse::new` built any radii. Non-finite data is
  refused everywhere (`CurveError::NotFinite(Part)`).
- **`CurveError` has `Degenerate` and `NotFinite` only**, both `Refused`; `OutsideDomain`,
  `TooLarge` and `Solve` (§3.2) join with the first operation that needs them (CU4, CU5):
  nothing in CU1 can be outside a domain (every kind is total over `t`) or too large.
- **No `scx-algo` dependency yet**; CU4's budgeted ellipse foot adds it.
- **No `eval_many` yet.** It belongs with the dispatch (CU2), where a slice loop over a `match`
  is the saving; per kind it would be a loop over `point`.
- **No difftest for `domain()`.** The pin's domain is an `Extent1d` sentinel pair, not the
  periodic or unbounded domain; the unit tests check `Unbounded` and `TURN`.
- **NURBS and `CurveJet`.** `NurbsCurve::derivs::<K>` returns its own array. CU3's `Nurbs` arm
  should wrap it as `jet::<D>(t, side) -> CurveJet<D>` in `scx-curve`, leaving `scx-nurbs`
  as it is (`scx-nurbs` is layer 1 and has `[f64; D]` points, not `Point3`).

Truth: `scx_testkit::analytic` in double-double. Lines: point within 2 roundings, the
derivatives exact. Conics: each coordinate within 6 roundings of `|o| + |a x| + |b y|`, points
and jets to order 3, both senses, 2-D twins embedded at z = 0; `Ivl` evaluation encloses the
truth, narrow and wide.

Difftest `t_curve`, 19 tests, 5,846 inputs: against `Line::infinite`/`from_points` and
`Ellipse::new`/`create_canonical_circle`/`new_circle` (dimension 2 for the twins), read back
through `canonical`, and `evaluate_step` at `t 180 / pi` with derivative `k` times
`(180 / pi)^k`. Where acceptance or values differ past the tolerance, scx is checked against
the definition or the truth (pin defects `curve_*`):

| Input | pin | scx | Why scx is right |
|---|---|---|---|
| a zero or sub-`EFF_ZERO` direction (`Line::infinite`) | builds NaN data | refuses | no direction, no line |
| a segment shorter than `EFF_ZERO` (`from_points`) | the X axis over `[0, 0]` | refuses | the points give no direction |
| a non-finite origin or direction | builds | refuses | not a line |
| an infinite circle radius | builds | refuses | not a circle |
| an ellipse radius zero, negative, below `EFF_ZERO`, infinite or NaN | builds | refuses | not an ellipse (the pin's own circle rule) |
| a conic at a large angle or a coordinate cancelling near zero | the degree round trip rounds the angle twice | `sin_cos` of the radian angle | within 6 roundings of the double-double truth |

2,520 exact, 2,819 within tolerance, 507 pin defects passing their checks, 0 mismatches, 0
nondeterministic (0.2 s in debug, one thread). Line points and jets need no defect: none is
past the tolerance.

### 3.7 CU2 as built (11-27)

Proposed for review, not yet ruled.

```rust
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Curve { Line(Line), Circle(Circle), Ellipse(Ellipse) }
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Curve2 { Line(Line2), Circle(Circle2), Ellipse(Ellipse2) }

impl Curve {   // Curve2 the same, with Point2 and CurveJet2
    pub fn domain(&self) -> ParamDomain;
    pub fn point(&self, t: f64) -> Point3;
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D>;
    pub fn point_over<T: Trig>(&self, t: T) -> Point3<T>;
    pub fn jet_over<T: Trig, const D: usize>(&self, t: T, side: Approach) -> CurveJet<D, T>;
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point3]);  // panics if the lengths differ
}
impl From<Line> for Curve { .. }  // and every other kind into its arm
```

- **`#[non_exhaustive]`, as 01 C10 chose.** Against: a match outside the crate needs a
  wildcard arm, so CU3's `Nurbs` arm will not show as a compile error at every outside
  match. For: it is 01's choice for every kind enum, adding `Nurbs`, `Parabola`,
  `Hyperbola` and `Procedural` is then not breaking (01 §7.4), and inside the crate (where
  every dispatch is) matches stay exhaustive. Outside callers should dispatch through the
  enum's methods rather than match.
- **Not `Copy`**, although every arm is today. Against: callers must `clone`. For: the
  `Nurbs` arm holds an `Arc` (§3.3), so `Copy` would be taken away in CU3, which breaks
  every caller that relied on it.
- **Size asserted**: `Curve` at most 104 bytes (the ellipse, 96, plus the tag) and `Curve2`
  at most 64 (A7). An arm past that goes behind an `Arc`.
- **The enum's `point_over` and `jet_over` are generic over `Trig`**, the conics' bound
  (lines need only `Ring`). Against: the `Nurbs` arm has no generic evaluator yet
  (`scx-nurbs` evaluates in `f64` only), so CU3 must add one or this bound cannot hold for
  it. For: §3.2 asks for the scalar-generic form on every kind, certification (SSI, the
  bounds of 01 C10) takes a `&Curve`, and leaving it off the enum would make every
  certifying caller match the kinds itself. This is a CU3 dependency: an `Ivl` NURBS
  evaluation in `scx-nurbs` or in the wrapper.
- **The enum's `jet_over` takes `side`; the kinds' does not.** Against: an unused argument
  today. For: the `Nurbs` arm needs it at a knot exactly as `jet` does, and adding it in CU3
  would change the signature under every caller. `point_over` takes no side, as `point`
  (a point is continuous, CU1).
- **`eval_many` writes into `&mut [Point3]`, and panics on a length mismatch.** §3.3, X9
  and 01 §9.4 sketch a slice. SU1's kinds took `&mut Vec<Point3>` (cleared first), which
  cannot mismatch. Against the slice: a mismatch is a programmer error the type does not
  rule out. For: the caller's storage can be any slice (a column of a larger buffer, an
  array on the stack) with no allocation and no capacity question; a silent zip over the
  shorter would hide the error, so it panics, as `copy_from_slice` does. SU2 should pick
  one of the two for both enums; this is the one 63 drew.
- **One `match` per batch, and a conic reads its frame once.** Each conic arm calls a
  crate-private `fill` that lifts the origin, x and y (`y` a cross product, D12) once and
  then runs the same operations in the same order as `point`, so every point has
  `point`'s bits (tested). It is not cached anywhere (X4): it is a local of one call.
  `fill` stays crate-private; the kinds' public surface is unchanged.
- **The dispatch methods are `#[inline]`.** Without it the line's `jet::<2>` through the
  enum took 7.5 ns against 3.3 ns direct (the generic call was not inlined into the
  caller); with it 3.4 ns.
- **`From` each kind into its arm.** A caller builds a kind and writes `.into()`.
- **No `tangent_dir`** (§3.3's sketch) yet: CU6's facade is its first user, and it needs
  the `Nurbs` arm's degenerate case to be defined. CU6 added it (§3.11).
- **No difftest through the enum.** CU6 adds the facade adapters (§3.5); the unit tests
  prove the enum gives the kind's bits, so the pin comparison would add nothing until then.
- **"Bit for bit" treats every NaN as one value.** Rust leaves a NaN's sign and payload
  unspecified, so the tests compare bits after mapping NaN to one pattern. Signed zeros are
  kept apart.

Tests (unit, `curve.rs`): every method of both enums against the kind's own call, bit for
bit, over 60 seeded instances of each kind at three scales, both senses, both sides, at
33 parameters each (signed zero, `1e-300`, `±1e6`, an infinity, a NaN), with interval
parameters for the generic forms; `eval_many` against `point` per element; the length
panic; the conversions. Send and Sync are checked at compile time in `lib.rs`.

Timings (release, one core, a loaded build machine, best of 7 runs of 4,000 batches of
1,024 seeded parameters; outputs written to a slice and black-boxed; a throwaway example,
deleted):

| ns per call | direct | via `Curve` | `eval_many` |
|---|---:|---:|---:|
| line `point` | 1.8 | 1.9 | 0.45 |
| line `jet::<2>` | 3.4 | 3.4 | |
| circle `point` | 36.2 | 35.3 | 32.9 |
| circle `jet::<2>` | 37.9 | 37.6 | |
| ellipse `point` | 36.6 | 36.3 | 33.0 |
| ellipse `jet::<2>` | 37.2 | 39.0 | |
| mixed kinds, one parameter each | | 24.5 | |

Dispatch costs nothing measurable over a direct call (the conic rows are within the
run-to-run spread of about 2 ns). `eval_many` saves 9% on a conic (the frame read once)
and 75% on a line (the loop vectorises).

### 3.8 CU3 as built (11-27)

Proposed for review, not yet ruled.

```rust
pub enum Curve  { .., Nurbs(Arc<NurbsCurve<3>>) }
pub enum Curve2 { .., Nurbs(Arc<NurbsCurve<2>>) }
impl From<NurbsCurve<3>> for Curve { .. }       // and From<Arc<NurbsCurve<3>>>; Curve2 the same

// scx-nurbs, src/eval_over.rs (new)
impl<const D: usize> NurbsCurve<D> {
    pub fn jet<const K: usize>(&self, u: f64, side: Approach)
        -> Result<([f64; D], [[f64; D]; K])>;                          // = derivs::<{K + 1}>
    pub fn point_over<T: Ranged>(&self, u: T, side: Approach) -> [T; D];
    pub fn jet_over<T: Ranged, const K: usize>(&self, u: T, side: Approach)
        -> ([T; D], [[T; D]; K]);
}

// scx-num, trig.rs
pub trait Ranged: Ring + Neg<Output = Self> {
    fn ends(self) -> (f64, f64);
    fn clip(self, lo: f64, hi: f64) -> Self;
    fn join(self, other: Self) -> Self;
    fn undefined() -> Self;                                            // NaN, the whole line
    fn gap(right: Self, left: Self, high: f64, low: f64) -> Self;      // a basis's knot difference
}
pub trait Trig: Ranged { fn sin_cos(self) -> (Self, Self); }           // was Ring + Neg
```

- **The arm holds an `Arc`** (§3.3, A7). Against: one indirection per call, and a caller
  who wants to change the curve must clone it out. For: the enum stays within its 104-byte bound
  (asserted), cloning a `Curve` is a count, and a NURBS curve is plain data that
  never changes in place (X3), so sharing it is safe. The arm costs nothing measurable over
  the direct call (table below).
- **`jet::<D>` comes from a new `NurbsCurve::jet::<K>`, not from `derivs::<{D + 1}>`.**
  Stable Rust cannot write `D + 1` in a const argument. Against the new function: a second
  copy of the derivative kernel next to `derivs`. The alternative was a `match D` calling
  `derivs::<1>` to `derivs::<32>`, which instantiates 32 kernels for every caller and
  stops at a hand-written bound. For: the copy is the scalar-generic kernel that
  `jet_over` needs anyway, so `jet` is just its `f64` form (X12). It keeps the pin's
  arithmetic order, the point's row apart from the derivatives' rows. A test proves it is
  `derivs::<{K + 1}>` bit for bit, errors included, for K = 0..=5, 9 and 31 at random and
  knot parameters on both sides.
- **No quotient-rule step in the wrapper.** `derivs` already returns Cartesian derivatives
  (the pin's rational basis derivatives). `jet` reuses the same quotient rule, with the same
  exact binomials and the same loop order.
- **The generic evaluator lives in scx-nurbs, in a new `eval_over.rs`.** Against: scx-nurbs
  gains a dependency on `scx_num::trig`. That is a layer-0 module it already sits on. For:
  the kernels need crate-private `span_of` and `window`, and they stay next to the `f64`
  kernels they mirror. `curve.rs` and `curves.rs` are untouched.
- **`Ranged`, a new supertrait of `Trig`.** Against: one more trait in scx-num, and `Trig`
  now asks more of a ring. Only `f64` and `Ivl` implement it, so nothing breaks. For: the
  enum's `point_over<T: Trig>` keeps its ruled signature. A piecewise evaluator needs two
  things `Ring` cannot give: the range a value spans, to pick its pieces, and a knot
  difference that stays tight. Putting these on `Ring` instead would have reached the
  Taylor models (`TmN`), which have no range.
- **An interval's knot difference is exact (`gap`); an `f64`'s is the pin's sum.** In
  `f64`, `gap` is `(U[i+j] - u) + (u - U[i])`, so the values keep `point`'s and `derivs`'s
  bits. In an interval that sum is twice as wide as `u` and holds zero once `u` is a span
  wide, and every basis value then becomes the whole line. `Ivl::gap` encloses
  `U[i+j] - U[i]` itself, so the enclosure holds the true real-number recurrence however
  wide `u` is. The alternative, a Bernstein or convex-hull enclosure per piece, would be
  tighter on wide intervals. It is a second algorithm with its own proof, left for when a
  certifier needs it.
- **A wide interval is split at the knots it crosses.** Each piece it meets is evaluated on
  the part of the interval inside that piece (the end pieces continue past the domain, as
  the curve does), and the results are joined. At an interior knot inside the interval,
  both one-sided values are therefore held, whatever `side` says. At a point interval,
  `side` picks the piece, exactly as in `f64`. Against: the cost grows with the number of
  knots crossed. For: one polynomial piece per evaluation is what makes the basis
  enclosure valid, and a jet's derivatives jump at a knot.
- **The enum's `f64` calls are NaN where the curve's errs.** This happens for a parameter
  that is not finite, an overflow, or a jet past `MAX_ORDER`. It is how a conic behaves at
  an infinite angle: `point` and `jet` are infallible on the enum (CU2). Against: the
  typed cause is lost at the enum. For: a caller who needs the cause calls the curve
  directly (`Curve::Nurbs(n)` gives `n.point(t, side)?`). Making `Curve::point` fallible
  for every kind because one kind can fail was rejected. The interval forms give the
  whole line for an end that is not finite.
- **`point` and `point_over` take the piece above `t`.** A NURBS curve is C0 (ruling 2),
  so the two sides agree in value; `Above` is `NurbsCurve::point`'s convention, and in
  `f64` the bits are that call's.
- **`eval_many` loops over `NurbsCurve::point`.** Against: no shared span search across
  the batch. For: it is `point`'s bits by construction, and a sorted-batch kernel belongs
  in scx-nurbs with its own measurement. It costs 2 to 10% over a hand loop (noise on a
  loaded machine, table below).
- **No difftest.** CU6 does the facade adapters (§3.5). The arm's `f64` values are
  `NurbsCurve`'s own, which `t_nurbs` already compares with the pin.

Tests:
- **scx-nurbs `eval_over`, 5 tests.**
  - `point_over::<f64>` against `point`, bit for bit. The curves cover degrees 1 to 5 and 9,
    and every knot and weight kind. The parameters are random ones in `[-0.1, 1.1]` and
    every knot, on both sides.
  - `jet::<K>` against `derivs::<{K + 1}>`, as above.
  - `Ivl` enclosures hold the testkit's exact rational point and first two derivatives.
    The intervals have widths 0, `1e-9`, `1e-3`, 0.2, 0.7 and 1.6, inside the domain,
    across knots and past its ends. They are checked at both ends, at random interior
    parameters and at every knot inside, with both one-sided exact values inside a wide
    interval. Degree 9 is checked on polynomial curves only, because its exact rational
    derivatives take seconds.
  - Point intervals are tight: within `1e-9` relative of `point`, and holding it.
  - Undefined inputs: NaN, infinite and reversed intervals, and orders past `MAX_ORDER`.
- **scx-curve `curve.rs`.**
  - The two dispatch tests now include NURBS arms: 3-D and 2-D, degrees 1 to 4 and 9. Every method is checked against the curve's own call, bit for bit,
    including `jet::<4>` against `derivs::<5>`, the interval forms and `domain` as
    `Bounded`.
  - The `eval_many` test now covers the NURBS arms as well.
  - The conversions test covers `From` for both arms and `Arc` sharing.
  - A new test checks the NaN failures and that clones share one `Arc`.

Timings. These are release builds on one core, on a loaded build machine (load average
about 11). Each figure is the best of 7 runs of 2,000 passes over 1,024 seeded parameters
in `[0, 1]`, with the results black-boxed, from a throwaway program that has since been
deleted. The curves are random-knot curves with `p + 7` control points, degree 3 and 9,
polynomial (P) and rational (R).

| ns per call | p3 P | p3 R | p9 P | p9 R |
|---|---:|---:|---:|---:|
| pin `crv_eval` | 27.1 | 35.7 | 114.4 | 127.5 |
| `NurbsCurve::point` | 34.5 | 46.7 | 114.2 | 133.8 |
| `Curve::point` | 34.6 | 46.8 | 108.6 | 134.2 |
| `Curve::eval_many`, per point | 42.6 | 44.7 | 131.4 | 131.0 |
| a loop of `NurbsCurve::point`, per point | 40.5 | 40.8 | 130.4 | 126.8 |
| pin `crv_derivs`, order 2 | 82.2 | 142.1 | 267.5 | 503.7 |
| `NurbsCurve::derivs::<3>` | 74.2 | 92.4 | 272.4 | 294.1 |
| `NurbsCurve::jet::<2>` | 76.8 | 94.9 | 313.0 | 353.5 |
| `Curve::jet::<2>` | 76.5 | 93.1 | 324.0 | 361.9 |
| `NurbsCurve::point_over::<f64>` | 34.2 | 45.3 | 146.0 | 175.5 |
| `Curve::point_over::<Ivl>`, a point | 379 | 522 | 2,702 | 2,644 |
| `Curve::point_over::<Ivl>`, width 0.05 | 512 | 664 | 3,286 | 3,219 |
| `Curve::jet_over::<Ivl, 2>`, width 0.05 | 1,306 | 1,952 | 5,490 | 5,797 |

- **Dispatch through the enum costs nothing measurable** over the direct call: the rows
  differ by run-to-run noise of a few ns.
- **Against the pin, `point` is 7 ns slower at degree 3** (27 against 35 ns) and level at
  degree 9. That gap was already there in scx-nurbs (the `NurbsCurve::point` row), so CU3
  did not add it.
- **Jets.** At degree 3, the jet matches `derivs`. At degree 9 the generic kernel is 15 to
  20% slower than `derivs` (313 against 272 ns, 353 against 294). That is a regression
  for an `f64` jet through the enum at high degree, against the `derivs` path no caller
  had through the enum before. It is still at or below the pin (268 and 504 ns).
  - The likely cause is the kernel's extra knot arrays and its separate point row at
    scratch width 32, the same cost `point_over::<f64>` shows at degree 9 (146 against
    114 ns).
  - Recovering it is a follow-up, which perf fixes do not block. One option is to route
    `jet` through `derivs_into` once `curve.rs` is free to change. Another is to drop the
    knot arrays from the `f64` instance.
- **An interval point costs 11 to 24 times an `f64` point.** That is the outward-rounded
  products.
- **Follow-up landed (11-27).** The `f64` instances of `jet` and `point_over` now take the
  `f64` basis directly and keep no knot arrays. The kernel is still the same one, with the same
  bits; `derivs_into` and `curve.rs` are untouched. Same machine and method as above,
  degree 9, ns per call: `jet::<2>` went from 376-424 to 279-340 (`derivs::<3>` 312-375 in
  the same runs), and `point_over::<f64>` from 184-193 to 125-133. In callgrind, `point_over`
  at p9 polynomial dropped from 3,409 instructions to 3,017. The jet went from 5,466 to 12,834,
  which is mostly zeroing the scratch rows: it is faster even so. The `Ivl` forms changed
  by under 0.5% (point 38,191 to 38,308, jet 107,907 to 108,289).

### 3.9 CU5 as built (11-27)

Proposed for review, not yet ruled.

```rust
// scx-curve, src/edit.rs (new)
impl Curve {
    pub fn restrict(&self, e: Extent1d) -> Result<Self>;
    pub fn reversed(&self) -> Self;                                    // C'(t) = C(-t)
    pub fn transformed(&self, a: &Affine) -> Result<(Self, ParamShift)>;
}
impl Curve2 { pub fn restrict(..) -> Result<Self>; pub fn reversed(&self) -> Self; }
pub struct ParamShift { .. }   // NONE, by(), apply(t) = t + by, extent(e)

impl Line    { pub fn reversed(&self) -> Self; pub fn transformed(&self, a: &Affine) -> Result<Self>; }
impl Circle  { pub fn reversed(&self) -> Self;
               pub fn transformed(&self, a: &Affine) -> Result<(Curve, ParamShift)>; }
impl Ellipse { pub fn reversed(&self) -> Self;
               pub fn transformed(&self, a: &Affine) -> Result<(Self, ParamShift)>; }
// Line2, Circle2, Ellipse2: reversed only

// scx-nurbs
impl<const D: usize> NurbsCurve<D> { pub fn reversed(&self) -> Self; }   // src/reverse.rs (new)
impl KnotVector { pub fn negated(&self) -> Self; }                        // knots.rs, one hunk

// CurveError gains OutsideDomain { extent, domain } and Nurbs(NurbsError) (transparent
// kind and name); Degenerate gains Extent { at } and Plane; Part gains Map.
```

- **The edits are by value** (04, `&self -> Result<Self>`), as the brief says. An infallible
  edit returns `Self`. `reversed` is total for every kind, so it takes no `Result`.
- **Reversal is `C'(t) = C(-t)` for every kind, not the pin's `a + b - t`.**
  - A line negates its direction. A conic flips its sense (`Sense::reversed`). Both are
    bit for bit, because `fmath`'s sine is odd and its cosine even, and a test checks it
    over every analytic curve of the CU2 fixtures.
  - A NURBS curve negates its knots in reverse order (`KnotVector::negated`, exact) and
    reverses its points and weights. The domain `[a, b]` becomes `[-b, -a]`, and an
    edge's interval negates exactly with it. A second reversal is the curve bit for bit.
    The values agree to rounding only (the basis sums run the other way).
  - Against: the pin keeps the domain, `[a, b]` to `[a, b]`, and that is what scx-nurbs-surface's
    `reversed` does for a surface direction. A caller that pairs a curve with a surface
    direction must negate the interval, not keep it.
  - For: `a + b - t` rounds each knot. Two close knots can merge, or a knot can cross
    its neighbour, so the pin's reversal can fail or change the curve. Negation cannot.
    The surface's form is left alone here (outside CU5's files) and noted for SU7.
- **`restrict` keeps the parameter.** A line and a conic carry no extent, because an
  edge holds its interval (§3.2), so they return a clone for any `e`.
  - A NURBS curve is `split` at each end of `e` that lies strictly inside its domain.
    An `e` equal to the domain returns the same `Arc`, so nothing is copied.
  - It refuses `OutsideDomain` for an `e` past the domain, `Degenerate::Extent` for an
    `e` of zero length, and the split's own cause otherwise.
  - Against a no-op `restrict` on each analytic kind: the enum arm already clones, so
    the kinds would only gain a method that does nothing.
  - Against clamping `e` to the domain: a request past the domain is the caller's bug,
    and the cause says so.
- **`transformed` returns a `ParamShift` with the image.** It holds
  `image.point(shift.apply(t)) ≈ a(self.point(t))`.
  - A line maps its origin and its direction (unnormalised, so `t` keeps its meaning,
    and no rescale is needed). A NURBS curve maps its control points and keeps its knots
    and weights, then is rebuilt through `NurbsCurve::new`, which re-checks them. Both
    have a zero shift.
  - A conic's image comes from `p = |Ax|²`, `q = |Ay|²` and `m = Ax·Ay`, with `x` and `y`
    the frame's axes scaled by the radii.
    - Where the map is a similarity on the plane (`|p - q|` and `|m|` within 16
      roundings of `p + q`), the frame takes `Ax` and the shift is zero. A circle stays a
      `Circle`, with radius `½(|Ax| + |Ay|)`.
    - Otherwise the image is put on its principal axes:
      `t0 = ½ atan2(2m s, (p - q) s)` with `s = sign(p - q)`, so `t0` is in `[-π/4, π/4]`
      and the nearest axis is chosen. Then `U' = U cos t0 + V sin t0`,
      `V' = V cos t0 - U sin t0`, and the shift is `-t0`. A circle under a map that is
      not a similarity becomes an `Ellipse`. This is an exact image, so no kind becomes NURBS (§3.2).
  - Against `Option<ParamShift>` or a shift folded into the curve: the curve has no
    parameter offset field, and an edge's interval must move with the image anyway
    (`ParamShift::extent`).
  - Against a NURBS image for a non-similarity map (the pin's `Transform` refuses
    anything but a uniform scale on a conic): an affine image of a conic is a conic, and
    the closed form is exact.
- **The 16-rounding similarity threshold.** Measured: the worst misfit over 200,000 random
  computed rotations times uniform scales was 5.25 roundings. A turned circle therefore
  stays a circle, with a margin of three. Snapping costs at most twice the misfit
  relative to the size, 32 roundings. Just past the threshold the principal axes are
  ill-conditioned (the angle is `atan2` of two tiny numbers), but the image is not: every
  point still maps to a few roundings, and a test checks both sides of the threshold.
  Against 64 roundings, the first draft: a near-similar map at 1e-14 snapped and
  cost `2e-15` at size 1, which is a needless error.
- **A mirror keeps the sense.** The image frame's z is the normal of `Ax × Ay`, and the
  sense is read from the image `y` against the frame's y.
  - Under a mirror, the image runs the same way about that normal as the original
    does about its own z. That agrees with the pin's points and with the geometry.
  - D14 says "keeps the frame right-handed and reverses the kind's sense". That is the
    same curve described from the other side, with z taken as `A z`.
  - Against `Frame::transformed`: it maps z by `A`, so it would refuse a projection
    along the plane's normal. Under such a map the image of z is not used, and the circle
    still maps to a circle or an ellipse in its own plane.
  - A map that flattens the plane itself (`Ax × Ay` too short) is `Degenerate::Plane`. A
    flattened axis is the size's own `Degenerate::SemiAxis` or `Radius`.
- **`Curve2` has no `transformed`.** scx-num has no 2-D affine map, and a body's transform
  keeps its pcurves (D14). It is added when a caller needs one.
- **`NurbsCurve::reversed` is in a new file, `scx-nurbs/src/reverse.rs`.** This keeps it out of
  `curve.rs`, where elevation work is under way. Its only other hunk is
  `KnotVector::negated` in `knots.rs`. The NURBS transform needs no new scx-nurbs op.
- **`CurveError` carries `NurbsError` (`Nurbs(..)`), with its kind and name passed through.**
  Against: `CurveError` grows from 16 to 40 bytes. `Result<Line>` stays at 56, and
  `Result<(Curve, ParamShift)>` is 104, the same as `Curve`'s bound. For: a NURBS edit's
  cause reaches the caller typed (typed errors goal), not flattened to `Refused`.

Tests (scx-curve `edit.rs`, 6 tests; scx-nurbs `reverse.rs`, 1; `error.rs` extended):
- **Reversal of analytic kinds, 3-D and 2-D.** Point and jet at `-t` against `t`, bit for
  bit (odd derivatives negated), and a double reversal.
- **Reversal of NURBS, 3-D and 2-D.** Domain negated exactly. Point and first derivative
  on swapped sides within rounding, and a double reversal bit for bit. In scx-nurbs,
  `derivs::<3>` on both sides at 65 parameters, polynomial and rational.
- **Restriction.** Analytic kinds unchanged. NURBS over interior, end-touching and `1e-9`-wide
  extents against the original's points, and the same `Arc` for the full domain.
  `OutsideDomain` and `Degenerate::Extent` are checked too.
- **Transform: every kind under random affine maps** (sizes 1e-3 to 1e3, with translations and
  mirrors). Point and tangent within a bound from the map's largest column times the
  curve's reach. A line has no shift, and a circle under a non-similarity becomes an ellipse.
- **Transform: closed forms.** Rotation, uniform scale, a mirror (`x → -X`, z flipped, sense
  kept), axis scales, a shear with the golden-ratio axes, a projection along z (circle
  kept), a projection along y (`SemiAxis` Y zero), and a NaN map (`NotFinite(Map)`).
- **Transform: near similarities and degenerate images.** Maps 1e-15 to 1e-10 from a
  rotation, on both sides of the threshold. A line taken to a point, and NURBS points
  overflowing to the curve's own cause.

Work added to existing paths: none. These are new functions. The only change to shared code
is `curve.rs`'s test helpers becoming `pub(crate)` and the error enum's larger size, which
the constructors' `Result`s mostly absorb.

### 3.10 CU4 as built (11-27)

Proposed for review, not yet ruled.

```rust
// scx-num, src/invert.rs (new): shared with scx-surface; scx-curve re-exports both
pub struct Foot<A = f64, Q = Point3> { pub at: A, pub point: Q, pub distance: f64 }
impl<A, Q> Foot<A, Q> { pub fn is_on(&self, tol: Length) -> bool; }   // distance <= tol
pub enum Inversion<A = f64, Q = Point3> { Inside(Foot<A, Q>), Outside(Foot<A, Q>) }
impl<A, Q> Inversion<A, Q> { pub const fn foot(&self) -> &Foot<A, Q>; pub fn into_foot(self) -> Foot<A, Q>;
                             pub const fn is_inside(&self) -> bool; }
// scx-curve, src/invert.rs (new); 2-D kinds the same with Point2
pub enum Closest<A = f64, Q = Point3> { One(Foot<A, Q>), Two([Foot<A, Q>; 2]), Every { distance: f64 } }
pub const POLISH_STEPS: u32 = 64;

impl Line    { pub fn closest(&self, p: Point3) -> Foot;
               pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Inversion; }
impl Circle  { pub fn closest(&self, p: Point3) -> Closest;
               pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Inversion; }
impl Ellipse { /* as Circle */ }
impl Curve   { pub fn closest(&self, p: Point3) -> Result<Closest, CurveError>;
               pub fn invert(&self, p: Point3, domain: Option<&Extent1d>) -> Result<Inversion, CurveError>; }

// error.rs
pub enum CurveError { .., Unsupported(Unsupported) }      // kind Unsupported
#[non_exhaustive] pub enum Unsupported { NurbsInversion }
```

- **`closest` gives every nearest point; `invert` picks one.** Against: two calls where
  §3.3 sketched one, and a caller who only wants a foot can ignore `Closest`. For: a
  circle's axis (every point) and an ellipse's axis inside its evolute (two feet) are real
  answers; the pin returned the domain's ends there without saying so. `invert` keeps
  §3.3's one-foot shape on top. All stationary points (the testkit's `Locus`) were
  rejected: no caller needs the farther ones, and the testkit already certifies them.
- **The domain is `Option<&Extent1d>`, wrapped by whole turns for a conic.** Against: a
  conic's foot can come back past `2 pi` (`at` is moved to start at the domain's `min`).
  For: an edge's interval on a periodic curve may start anywhere (`[-pi, 0]`,
  `[4 pi, 5 pi]`), and the caller wants `at` in its own interval. `None` is the kind's own
  domain. A domain a whole turn long or more holds every angle.
- **An outside foot is not clamped to the domain's end.** Against: a caller wanting the
  nearest point of a trimmed edge must compare the ends itself. For: 57 F6's `Outside`
  says the kind's foot lies outside, and the nearest end is the edge's question, not the
  kind's (scx-brep). Clamping would hide whether the foot was inside.
- **The circle's axis is exact.** Only a query whose projection is the centre itself is
  `Every`. Against: a query `1e-20` off the axis gets an `atan2` angle that means little.
  For: its distance is right to the last bit whatever the angle, and the pin's band of
  `SM_EFF_ZERO` scaled by the model is a tolerance the kind has no business choosing. The
  difftest finds no input where the band changes the distance.
- **No budget, `Stop` or near hint** (§3.3 sketched them). Against: the sketch's
  signature, and a NURBS arm will need them. For: every analytic arm is a closed form or
  a polish of at most `POLISH_STEPS` steps (bisection alone closes the quarter turn in
  under 60), so a budget would only be threaded through and never spent. The ellipse's
  seed is the quartic in `tan(s/2)` (`scx_algo::polynomial::solve_quartic`), polished by
  Newton inside a bracket in the first quadrant, where there is exactly one stationary
  point. The centre and the axes are closed forms. The NURBS arm adds the budget when it
  is built, on its own signature.
- **NURBS is `Unsupported`**, a new `CurveError` cause of kind `Unsupported`. Against:
  `Curve::invert` is fallible for every kind because one kind is not built. For: the
  alternative, a dense sample with a local polish, would be a wrong-answer path with no
  certified truth behind it (§3.4), and a typed refusal is honest. `Closest` and
  `Inversion` are infallible on the kinds themselves.
- **The impls live in `invert.rs`, not `curve.rs`.** `curve.rs` was being edited by CU5
  at the same time. The kinds' and enums' inversions sit together, and `curve.rs` is
  untouched.
- **`Foot` and `Inversion` live in `scx_num::invert`.** Against: two plain value types
  in layer 0 that scx-num itself never returns. For: the surfaces' inversion (SU4)
  returns the same shapes with `A = (f64, f64)`, and scx-surface does not depend on
  scx-curve. `Closest` stays in scx-curve: its `Every` and `Two` are curve answers.
- **The distance is 3-D, always computed.** Against: a caller who only wants `at` pays
  for a subtraction and a length (the line's 7.4 ns against the pin's 6.3 ns below). For:
  the foot is one value (X8), and `is_on(tol)` needs it; the pin's out-parameters made
  callers recompute it.

Tests (scx-curve, 9 new): the line's foot against the testkit's exact projection at
scales `1`, `1e3`, `1e-3`; a line against its domain; the circle's foot against the
reference, and its axis as every point; the ellipse's foot against the testkit's
certified nearest (T1, research 59 §4a) at four scales, near its evolute and when thin
(`b/a` down to `1e-3`); the ellipse's two feet at its centre and on an axis inside its
evolute; whole-turn wrapping; the 2-D kinds against the 3-D ones in the plane; a
non-finite query gives a NaN foot, and NURBS gives `Unsupported`.

T1's `Ellipse::stationary` was already in the testkit; only its doc in `analytic.rs`
was corrected to say so.

Timings. Release, one core, on a loaded build machine; best of several runs over 256
seeded queries, from a throwaway bench since deleted.

| per query | scx `invert` | pin |
|---|---:|---:|
| line | 7.4 ns | 6.3 ns (`step_inversion`, the parameter only) |
| circle | 98 ns | 302 ns (`drop_point_fast`) |
| ellipse | 1.11 µs | 33.5 ns (`step_inversion`, `atan2`, the foot only on the curve) |

- **The line is 17% slower** because it also builds the foot point and distance, which
  the pin's call does not return.
- **The circle is 3.1 times faster.**
- **The ellipse has no like-for-like pin row.** The pin's `step_inversion` is one
  `atan2` and is the foot only for a point on the curve. The pin's general
  `drop_point`, seeded with it, took about 76 µs per query in the same bench, but that
  bench passed a degree interval and a degree guess to a solve that runs on the carried
  NURB's parameter, so the figure is not a fair comparison and is not claimed.

**Perf follow-up (11-27).** The ellipse foot took 1.11 us a query because every query solved the quartic for a seed. `quadrant_root` now seeds with the foot of a point on the curve, `atan2(a v, b u)`, and polishes with the same safeguarded Newton (6 steps at most); a query that does not converge in them is redone from the quartic as before. No certificate is needed: the first-quadrant root of `g` is unique and the bracket `(0, pi/2)` holds every step, so either seed ends at the global nearest foot. The fast polish also stops where `g` is within rounding of zero (`|g| <= 4 eps (|k sin cos| + a u sin + b v cos)`) and the Newton step is within `4 eps s`; the quartic path keeps its old stop. Added work per query: one `atan2`, and a `sin_cos` per step, 1.0 to 3.6 steps on or within 1e-3 of the curve (the quartic ran every query before; it now runs for 0 to 6% of those, 2 to 39% of far points). Per query, best of 3 runs of 100 passes over 2,000 points, four ellipses (5/3, 10/1, 2/1.9, 1/8): on the curve 783/980/558/439 ns before, 136/175/127/104 after; 1e-9 off 786/1001/576/450 before, 180/227/169/148 after; 1e-3 off 802/1021/558/452 before, 206/317/204/212 after; far 842/857/756/807 before, 312/421/246/503 after. Some output bits change (7,147 of 22,400 test queries, on the angle in the last ulps); against the testkit's certified nearest the new distances are equally or more correct (the mean distance error is at or below the old on each of 7 ellipses, 1.9e-16 against 2.8e-16 and 2.6e-15 against 3.4e-15 at most; worst 8.9e-14 against 9.0e-14, the same ill-conditioned 100/0.01 ellipse), and the angles are as close (worst 1.9e-13 both). The pin's 33.5 ns answers only points on the curve.

### 3.11 CU6 as built (11-27)

Proposed for review, not yet ruled.

```rust
// scx-curve, src/tangent.rs (new)
impl Curve  { pub fn tangent_dir(&self, t: f64, side: Approach) -> Option<UnitVec3>; }
impl Curve2 { pub fn tangent_dir(&self, t: f64, side: Approach) -> Option<UnitVec2>; }

// scx-difftest, tests/diff/t_curve_query.rs (new)
// curve_nurbs_jet, curve_nurbs_tangent, curve_line_tangent, curve_line_invert,
// curve_circle_invert, curve_ellipse_invert_on
```

- **`tangent_dir` keeps the pin's floor for a vanishing derivative,
  `1.1 EFF_ZERO (1 + max|C(t)|)`.** Against: a first derivative of `1e-18` is not zero,
  and exact arithmetic would follow it. For: a computed derivative that small is
  indistinguishable from the rounding of one that is zero (a doubled control point at an
  interior double knot gives about `1e-16`), and its direction would be noise. The floor
  applies to each derivative in turn, not to the second alone as at the pin; `None` only
  where every derivative to the order vanishes (a curve collapsed to a point).
- **The sign follows the side: `(-1)^(k-1)` for the k-th derivative from below.** The
  pin negated the second derivative past the middle of the domain, whichever side was
  asked. Against: results differ from the pin. For: near `t` the curve runs as
  `C^(k) h^k / k!`, so from below an even-order cusp is approached along `-C^(k)`. The
  pin's rule is right only at the domain's start from above and its end from below.
- **An analytic arm normalises its first derivative**, which never vanishes (CU1's
  construction floor). Each arm scales by its largest coordinate before normalising, so
  any finite non-zero vector has a direction.
- **`from_left = true` is `Approach::Above`** (X5). The pin's `Side::Left` is the span
  starting at the knot, which is the piece above it.
- **The conics are not compared through the pin's `Curve::evaluate`.** It runs on the
  carried NURB's parameter, not the angle; `t_curve` compares their `evaluate_step`
  instead. The line and NURBS arms are compared through it.
- **The NURBS arm is compared inside the knots' domain, on curves scx builds.** Outside
  the domain the pin clamps `t` and scx continues the end pieces (CU2). The `t_nurbs`
  pool's curves with a knot of multiplicity above the degree are refused by scx (CU3) and
  are left out.
- **A circle is compared by its foot's distance.** The pin's `drop_point_fast` returns
  the carried NURB's parameter, not an angle, so the distance is what both share. It is
  compared at `rel 1e-12, abs 1e-12`: a distance near zero carries the coordinates'
  rounding (`4e-14` a thousand units out), not its own.
- **An ellipse is compared only for points on it.** That is the one place the pin's
  `atan2` is the foot. Off the curve, the foot is held to T1's certified nearest in
  scx-curve's tests.

Pin defects, each with a truth check in `pin_defects.rs`:
- `curve_nurbs_tangent`: the pin's zero-tangent fallback signs by the domain's middle,
  not the side, and keeps a zero first derivative where the second vanishes too. Truth:
  the testkit's exact derivatives, the first above the floor, signed by the side, within
  `1e-9` (a rational curve's higher derivatives lose digits to the spread of its
  weights). 17 inputs.
- `curve_ellipse_invert_on`: the two angles are a turn apart at the seam, or differ by
  `atan2`'s rounding on a thin ellipse. Truth: the point at scx's angle is the query,
  within 64 roundings of the ellipse's size. 57 inputs.

| difftest | inputs | exact | within tolerance | pin defects | mismatches |
|---|---:|---:|---:|---:|---:|
| `curve_nurbs_jet` | 3,270 | 1,398 | 1,872 | 0 | 0 |
| `curve_nurbs_tangent` | 3,206 | 1,894 | 1,295 | 17 | 0 |
| `curve_line_tangent` | 3,026 | 2,347 | 679 | 0 | 0 |
| `curve_line_invert` | 3,406 | 3,170 | 236 | 0 | 0 |
| `curve_circle_invert` | 437 | 264 | 173 | 0 | 0 |
| `curve_ellipse_invert_on` | 323 | 148 | 118 | 57 | 0 |

The NURBS pool adds cusps: a doubled start and end, a tripled start, a rational doubled
start, a curve collapsed to a point, and a degree-2 cusp at a double knot at 0.4, 0.5 and
0.7, at their knots and `1e-9` either side. scx-curve's `tangent.rs` has 3 unit tests (an
analytic kind, the NURBS cusps on both sides, 2-D against 3-D).

`tangent_dir` adds no work to any existing call: it is new, and only the difftest calls
it. A smooth NURBS point costs one `derivs::<2>`; the higher derivatives are computed
only where the first vanishes.

### 3.12 Conic kinds as built (11-27)

Proposed for review, not yet ruled. Decided by analysis and an adversarial pass, below.

```rust
// scx-curve, src/open_conic.rs (new)
pub struct Parabola  { /* frame, focal: Length, sense */ }   // vertex + F t^2 x + 2 F t y
pub struct Hyperbola { /* frame, semi_x, semi_y, sense */ }  // centre + a cosh t x + b sinh t y
// and Parabola2, Hyperbola2; each: new(..) -> Result, point, jet::<D>(t, Approach),
// point_over / jet_over (f64, Ivl), domain() = Unbounded, reversed, transformed -> (Self, ParamShift),
// closest(p) -> Closest, invert(p, Option<&Extent1d>) -> Inversion
// Curve::{Parabola, Hyperbola}, Curve2::{Parabola, Hyperbola}: every dispatched method
// ParamShift gains a stretch: (t + by) / stretch (one but for a parabola's affine image)

// scx-ssi::analytic: a cone's section is now a parabola or a hyperbola's branch where it is one
// scx-surface: pcurve_of on a plane keeps the kind (Parabola2, Hyperbola2); on the other
//   kinds NoClosedForm { curve: Parabola | Hyperbola, .. } (Unsupported), never empty;
//   intersect_curve takes both against all five kinds
```

**The parametrisation: STEP's, the pin's.** A parabola is `O + F t^2 X + 2 F t Y` and a
hyperbola one branch `C + a cosh t X + b sinh t Y`, over every `t`.

- Alternatives weighed:
  - **the parabola by its y**, `(t^2 / 4F, t)`: the parameter is a length, so a uniform
    scale moves it, and every STEP read and write would remap it;
  - **a rational form** (a quadratic Bezier, or `sec`/`tan` for the hyperbola): the NURBS
    code could evaluate it, but `sec`/`tan` has poles inside the branch's natural range, the
    Bezier is bounded to one span, and either needs a remap to and from STEP;
  - **`e^t` for the hyperbola**, `(a (s + 1/s) / 2, b (s - 1/s) / 2)`: a polynomial after
    clearing `s`, which T2 and the clip use internally (below), but `s` runs over `(0, inf)`
    only, the reversal `t -> -t` becomes `s -> 1/s`, and the derivatives lose their
    two-term cycle.
- For STEP's: the AP242 `parabola` and `hyperbola` entities take exactly these, so a part
  reads and writes with no parameter map; the pin's `evaluate_step` is the same function, so
  the difftest compares like with like; reversal is `t -> -t` with the sense flipped, as the
  closed conics (CU5); a uniform scale or rigid motion leaves `t` alone; and the
  derivatives are closed forms of any order (the parabola's vanish past the second; the
  hyperbola's alternate between two terms).
- Against, and what was done:
  - **a hyperbola point needs `sinh` and `cosh`**: `fmath::sinh_cosh` (new in scx-num) gives
    both from one double-double exponential, with the bits of `sinh` and `cosh` (their
    rounding tests and fallbacks, research 70, so correctly rounded); a point costs about
    20 ns against the parabola's 4;
  - **`cosh` overflows past `|t| ~ 710`**: there the point is infinite, as a line's at an
    infinite `t`; an edge's interval never reaches it (the branch is then past `1e308`);
  - **a parabola's parameter is not affine-invariant**: under a map that is not a similarity
    the vertex moves and the parameter stretches. `transformed` returns the map as a
    `ParamShift` with a stretch, `(t + by) / stretch`, and an edge's interval moves with it,
    as CU5's ellipse.

**Deviations from the pin.**

- **Plain data, validated once.** A frame, the sizes as `Length` and a `Sense`, refused at
  `new` for a non-finite size or one not above `EFF_ZERO` (`Degenerate::Focal`,
  `Degenerate::SemiAxis`). The pin built any size, kept an analytic domain and a NURB
  twin, and clamped `t` to the domain inside `evaluate_step`. Here the domain is
  `Unbounded`: an arc is an edge's interval (CU5).
- **The parabola's x term is `F (t t)`**, the pin's `(F t) t`. Both are one rounding of the
  same product apart; `F (t t)` keeps `t t` exact in the interval form, so the enclosure is
  tight. The difftest agrees with the pin within the tolerance at every input.
- **The foot is the true nearest point.** The pin's `step_inversion` is the parameter of a
  point assumed on the curve (`y / 2F`, `asinh(y / b)`). `closest` and `invert` (CU4) take
  the query into the frame:
  - the parabola's stationary cubic `F s^3 + (2F - u) s - v` has one root on the query's
    side; Cardano's root (stable forms) seeds a bracketed Newton polish. On the axis, past
    the evolute's cusp (`u > 2F`), two equal feet `+-sqrt((u - 2F) / F)`;
  - the hyperbola's `k sinh t - a u tanh t - b v` (`k = a^2 + b^2`) has one root on the
    query's side, below `asinh((max(a u, 0) + b v) / k)`; the pin's `asinh(v / b)` seeds a
    bracketed Newton polish. On the axis past the cusp, `+-acosh(a u / k)`.
  - No quartic fallback: the hyperbola's equation is a quartic in `e^t`, but the bracket is
    certified (the function is convex on the side, with a sign change at its ends), so the
    bracketed Newton converges from any seed in it. A foot outside the domain is reported as
    `Outside`, never clamped (CU4, CU6). Held to the testkit's certified nearest (cubic for
    the parabola, isolated roots for the hyperbola) in scx-curve's tests.
- **Bounding** is the interval form, as for the closed conics: `point_over::<Ivl>` over an
  interval of `t` encloses the arc (tested against samples).

**Wired in.**

- **scx-surface `pcurve_of`**: on a plane the kind is kept in its affine uv (`Parabola2`,
  `Hyperbola2`); on a cylinder, cone, sphere or torus the answer is
  `PcurveError::NoClosedForm { curve, surface }` (`ErrorKind::Unsupported`), never an empty
  or approximate curve. Exact pcurves off the plane would follow §4.14's `AngleWave2`
  (an oblique cylinder section), which has no open-conic analogue yet.
- **scx-surface `intersect_curve`**: both kinds against all five surfaces. The parabola is a
  polynomial quadratic in `u` (its parameter affine in `u`, as a line's); the hyperbola is a
  rational quadratic whose middle control point is where the ends' tangents meet, with
  weight `cosh w`, and a new `Map::Hyper` takes `u` back to `t`
  (`t = m + 2 atanh(k (2u - 1))`, kept as sums of positive terms). Degrees as for a conic.
  §4.14's overlap clipping by `se` takes both: a side's plane meets the parabola where a
  quadratic in `t` vanishes and the hyperbola where `alpha cosh t + beta sinh t + gamma`
  does, a quadratic in `e^t` (only its positive roots); `overlaps_are_clipped_by_se` runs
  both in a plane and on a cone (a generator-parallel parabola, an axis-parallel branch).
- **Testkit T2** takes both curves: the parabola in `t`, the hyperbola in `s = e^t` (a new
  `Variable::Exp`), with only `s > 0` on the branch.

**The cone section (63 §5.6).** From the apex, in the plane's coordinates
`delta n + alpha e1 + beta e_t`, the section is `e alpha^2 - g alpha + beta^2 + c0 = 0`
(`e = nz^2 - k^2 s^2`, `g = 2 delta s nz (1 + k^2)`, `c0 = delta^2 (s^2 - k^2 nz^2)`).

- `e > 0` is an ellipse, `e < 0` a hyperbola and `e = 0` a parabola. A rounded `e` is never
  exactly zero, so **a parabola is taken where dropping `e alpha^2` moves no point of the
  extent's heights by more than the tolerance**: `2 |e| reach^2 <= tol |g|`, `reach` the
  largest `|alpha|` over the two rims. There the exact ellipse or hyperbola is longer than
  its precision and the parabola is within the tolerance of it.
  - Against: the true curve is an ellipse or hyperbola, and a parabola changes its kind.
  - For: 04's primitive rule (a section within the tolerance of a simpler kind is that kind)
    and the ellipse's own precision bound; outside the threshold the exact kind is kept.
- The hyperbola is the branch on the nappe the cone's extent lies on (`k z' > 0`); the
  other branch is on the far nappe and is dropped, as the generators' far halves are.
- Between the parabola threshold and the ellipse or hyperbola precision bound
  (`size * 2^-50 > tol`) the answer stays `General`. Both tests are exact statements of
  what a double can hold; the band between them stays open, and
  closing it would need a kind that is neither.
- The clip: a parabola's crossings with a linear side are a quadratic in `t`; a
  hyperbola's are a quadratic in `e^t`, roots with `e^t > 0` only. Spans are kept as a
  line's.

**The cylinder's too-long ellipse does not fall out.** A near-axial cut of a cylinder is
an ellipse longer than its precision, but it is not near a parabola: its flanks are the
cylinder's two nearly straight generators, and its ends turn with the cylinder's radius.
Where the generators are within the tolerance over the extent's height it is already two
lines (§5.6); between that and the precision bound it stays `General`.

**Tests.** scx-curve: 7 unit tests in `open_conic.rs` (points and jets to order 5 against
the double-double truth, interval enclosure, reversal to the bit, the 2-D twins, refusals,
`transformed` against closed forms, the inverse hyperbolics) and 3 in `invert.rs` (the foot
against the certified nearest over every scale and sense, the two feet past the cusp, an
outside foot reported). scx-num: `sinh_cosh` to the bit of each, and the `Ranged`
enclosures. scx-surface: `pcurve_of` on a plane and the typed refusal on a cone;
`intersect_curve` against T2 for both kinds, with the plane's overlap and the vertex touch.
Testkit: the new truths checked exactly.

| difftest | inputs | exact | within tolerance | pin defects | mismatches |
|---|---:|---:|---:|---:|---:|
| `curve_parabola_point` | 380 | 330 | 50 | 0 | 0 |
| `curve_parabola_jet` | 380 | 154 | 226 | 0 | 0 |
| `curve_parabola2_jet` | 380 | 144 | 236 | 0 | 0 |
| `curve_hyperbola_point` | 380 | 309 | 71 | 0 | 0 |
| `curve_hyperbola_jet` | 380 | 324 | 56 | 0 | 0 |
| `curve_hyperbola2_jet` | 380 | 312 | 68 | 0 | 0 |

`t_open_conic` compares against the pin's `Parabola::evaluate_step` and
`Hyperbola::evaluate_step` on `t_curve`'s conic pool, at parameters inside the pin's domain
(it clamps; scx does not). The within-tolerance rows are an ulp or two of the sum's order.

**Timings.** Side-channel, a throwaway timer, not committed; release, per call:

| call | scx | pin |
|---|---:|---:|
| parabola point | 4.2 ns | 17.4 ns |
| parabola jet, order 3 | 7.5 ns | 16.5 ns |
| hyperbola point | 20.1 ns | 24.2 ns |
| hyperbola jet, order 3 | 24.3 ns | 54.3 ns |

The pin allocates a `Vec` per call; scx's hyperbola is mostly `sinh_cosh`.

**Other kernels (research 66).** Nothing applies: 66 §8.2 records only OCCT's IntAna for
quadric pairs (docs only), and none of 66's techniques is about conic evaluation or feet.
The `e^t` substitution and the tangent-meeting control point are the textbook conic as a
rational quadratic.

**SS2 coverage (63 §5.5).** SS2 is plane-plane and plane-quadric through `intersect`.
`intersect` puts the plane second and answers by `section_by_plane`, so both pairs are
built. `plane_sections` now runs every case (fixed and 400 random, both senses) through
`intersect` both ways: the surface first equals `section_by_plane`, and the plane first gives
the same pieces run the other way (a tangent curve either way, its `n_a x n_b` vanishing),
the same points, and a coincidence's extents swapped. The difftest compares
`section_by_plane` with the pin. Still missing from SS2:

- the cylinder's too-long ellipse and the cone's band between the parabola threshold and the
  precision bound (`General`, above; §5.8 (c): not closed, a rational shape in the clipper
  is proposed);
- the torus's spiric sections (`General`; step 5's marcher);
- exact pcurves off the plane: the cone's conics, open and closed (`pcurve_of` answers
  `NoClosedForm`; the cylinder's oblique ellipse is built, §4.14);
- the difftest goes through `section_by_plane` only, not `intersect`'s plane-first order
  (the pin's `intersect_with_plane` has one order);
- ~~SS4 (classification at the tolerance)~~ built 11-27 (§5.8); SS3 (the coaxial pairs,
  §5.7) is built.

## 4. `scx-surface`

The step-2 slice covers plane, cylinder, cone, sphere, torus and B-spline, with point inversion,
closed-form analytic projection and curve-surface intersection. `AnyCurve` starts here.

### 4.1 Shapes found

| Shape | Where (READ) | Count (MEASURED) |
|---|---|---:|
| Kind enum where analytic kinds are subclasses of the B-spline and carry its NURB | `surface/surface/p2.rs:392` `enum SurfaceKind`: `Cone` "including analytic state and its NURB", `Plane` "(an `SmBSplineSurface`)", `Sphere` "(an `SmSurfOfRevolution`)" | `kind_with_nurb`/`gw_nurb` 341 |
| Lazily made NURB, reached before every dispatch | `surface/surface/p2.rs:1711` `kind_with_nurb(&self, k) -> Result<&SurfaceKind>`, used by `evaluate` (`:2190`) and `step_inversion` (`:2919`) | — |
| Owner chain and owner tolerance on the surface | `surface/surface/p2.rs:430`-`:441` (`owner`, `owner_chain_face`, `owner_zone_tol_3d`) | owner 887, `owner_zone_tol` 92 |
| Integer type test | `surface/surface/p2.rs:357` `is_kind_of(t: i64)`, `TYPE_STRING`, `CLASS_STRING` | 606 |
| Base-class failing defaults | `surface/surface/p2.rs:5195`-`5400` (X2) | 603 |
| Partials `Vec<Vec<Vec3>>` with two orders and flags | `surface/surface/p2.rs:2190` `evaluate(&self, k, uv, highest_u_deriv, highest_v_deriv, flags: EvaluateFlags)` | 59 |
| An error status that is an answer | `surface/surface/p2.rs:2902` `step_inversion_answered` | `upstream_status` 84 |
| Inversion returning `(Point2, LocationType)` | `surface/surface/p2.rs:2919`; separate `_unbounded` variants (`:3051`, `surface/cone/p1.rs:1863`) | — |
| Plane intersection as `(bool, SurfaceIntersection, Option<Warning>)`, with `use_surface_edges: [bool; 2]` and an ignored angle tolerance | `surface/surface/p2.rs:3249` | `needs_more` 83, `(bool,` 89 |
| Tolerance fallback by `Option` | `surface/sphere.rs:1170`: "the tolerance is `approx_tol_3d`, else `SM_EFF_ZERO_SQRT`" | — |
| Two result types of the same name | `SurfaceIntersection` at `surface/surface/p2.rs:1178` (curves) and `surface/surface/p11.rs:209` (a point pair) | — |
| Owned and borrowed duplicates of one routine | `intersect_with_plane`, `_owned`, `_owned_into` (`surface/torus.rs:471`, `:494`, `:515`) | — |
| `_simple` volume methods | `surface/b_spline_volume_n_lib.rs:134` and on | 352 |
| Interior mutability | — | `RefCell`/`Cell`/`Rc` 25 |

### 4.2 Proposed shapes

- **Analytic kinds are analytic only.**
  - `Plane`, `Cylinder`, `Cone`, `Sphere` and `Torus` are small value types (a `Frame` and lengths
    or angles), with no NURB inside.
  - `to_nurbs(&self, over: &Extent2d) -> NurbsSurface` is the exact rational conversion, a pure
    function of the kind and a bounded extent. Under D13 it keeps uv: the NURBS image has the same
    parameter as the kind on `over`, so a pcurve needs no mapping across the conversion.
  - An unbounded plane or cylinder has no NURB, so the extent is required. `Extent2d` is finite by
    construction (P0), so the conversion is total.
  - `kind_with_nurb` disappears. Every query on an analytic kind uses its closed form, which 01 C10
    gives as the reason for enums.
  - Section 9 tests whether the pin's carried NURB encodes a cost lesson.
- **The uv parameterisation is the pin's** (D13) [Ruled 11-27, H2. This note first switched the
  analytic kinds to natural parameters, angle periodic in 2π, without a D row; review-63 H2.]
  - At the pin, sphere, torus, revolution and extrusion evaluate through their base NURB
    (`surface/surface/p2.rs:1711`, `:2190-2240`), and the cone and cylinder "at the NURB
    parameters" (`surface/cone/p1.rs:1570`). Their uv is the rational NURB parameter.
  - scx keeps that uv, computed in closed form from the kind's frame and sizes. The kind carries no
    NURB (D4). The closed forms (inversion, normals, sections) work in the angle and map it to the
    NURB-compatible parameter and back, per span of the pin's knot layout, which SU1's builder
    reads from the pin and states in the crate docs.
  - Why (review-63 H2):
    - an oblique plane section of a cylinder stays an exact rational pcurve of degree 2p in uv
      (`intersect/surface_drop_lib/cylinder_section.rs`, WR-680), held exactly by `Curve2::Nurbs`.
      With natural parameters it would be `v = a cos u + b sin u`, which no `Curve2` kind holds, so
      it would become a fitted pcurve;
    - the pin's open item asks for more exact sections, not fewer (`docs/booleans.md:~1024`);
    - reading pin bodies needs no uv mapping, and uv-valued difftests compare directly.
  - The cost moves to STEP, whose pcurves on analytic faces use the angle: `scx-step` maps them, as
    the pin's reader does. The truth for that map is that a STEP uv and its scx uv give the same
    3-D point (section 4.4).
- **Cylinder leaves Cone.**
  - The pin's `Cone` holds both. A cone with a zero half-angle is the cylinder, but the closed
    forms differ: the apex is a pole, and v runs along the ruling against along the axis.
  - Each kind keeps the pin's uv for its own case (D13), so the split changes no parameter
    (JUDGED).
- **Evaluation.**
  - `point(uv)`, `jet::<D>(uv, [Approach; 2]) -> SurfaceJet<D>`, `normal(uv) -> Option<UnitVec3>`
    (`None` at a pole or degenerate point), `domain() -> UvDomain` (a `ParamDomain` per direction,
    D1) and `eval_many`.
  - Partials are true partials, with no flags (X5).
  - The pole and collapse lessons of `eval/collapse` and `eval/pole` become `poles()` and
    `collapse_at(uv, reach: Length) -> Collapse` on the kinds that have them.
- **Inversion has outcomes, not codes** (X8) [Ruled 11-27, L5 and D7: 57 F6's shape].
  - It returns `Inversion<Point2>`, the same type as the curve's (section 3.2): `Inside(Foot)` or
    `Outside(Foot)` of the domain, each with uv, point and distance. Being out of tolerance is
    `foot.distance > tol`, read by `Foot::is_on(tol)`. D7's three outcomes are inside and on,
    inside and off, and outside.
  - F6's `Failed` is gone: the closed forms (plane, cylinder, sphere) cannot fail, and the
    iterative arms (NURBS, and the torus and cone where they hand over to a solve) return
    `Result<_, SurfaceError>` under a budget and `Stop` (X13).
  - The `_unbounded` twins become an argument: `domain: Option<&Extent2d>`, where `None` means the
    kind's own domain, possibly unbounded. This is not a bool. Only the closed forms accept `None`
    on an unbounded kind; they do no arithmetic on the domain's ends (D1).
- **The plane section is `scx-ssi`'s** (01 §1.1: "the plane section of a surface") [Ruled 11-27,
  M3. This note first put `section_by_plane` here as SU5.] Section 5 has it as SS5. `scx-surface`
  supplies only what every query uses: the kind's frame, sizes and the uv map of D13.
- **One routine per operation.** No `_owned`/`_owned_into` triplets.
  - Where the pin passed ownership context, the caller passes the value.
  - Where it wrote into a vector, the function returns one.
- **Curve-surface intersection** returns sorted `Vec<CurveSurfaceHit>`, inside a `Result` under a
  budget and `Stop`, because root isolation iterates (X13).
  - A hit is a point `{ t, uv, point, kind: Crossing | Tangent }` or an overlap
    `{ t: Extent1d, .. }`.
  - Sorting is by `t`, then `uv`, for determinism.
  - Analytic pairs use closed forms. Line, circle and ellipse against the quadrics reduce to
    polynomials of degree up to 4 in `scx-algo`'s root isolation.
- **`AnyCurve`** (01 §2) starts here with the arms step 2 needs: `Free(Curve)` and `OnSurface`
  (a `Curve2` on an `Arc<Surface>`). The intersection curve arm follows with `scx-ssi`'s marching
  in step 5.

### 4.3 Sketch

```rust
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Surface { Plane(Plane), Cylinder(Cylinder), Cone(Cone), Sphere(Sphere), Torus(Torus), Nurbs(Arc<NurbsSurface>) }
pub struct Cone { frame: Frame, radius: Length, half_angle: f64 /* radians, (0, pi/2) */, sense: Sense } // private fields (01 §7.4)
pub enum Closed<T> { Solved(T), General }                         // shared with scx-ssi (D6)
pub enum CurveSurfaceHit { Point { t: f64, uv: Point2, point: Point3, contact: Contact }, Overlap { t: Extent1d } }

impl Surface {
    pub fn point(&self, uv: Point2) -> Point3;
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<D>;
    pub fn normal(&self, uv: Point2) -> Option<UnitVec3>;
    pub fn invert(&self, p: Point3, near: Option<Point2>, domain: Option<&Extent2d>,
                  budget: &mut Budget, stop: &Stop) -> Result<Inversion<Point2>, SurfaceError>;
    pub fn to_nurbs(&self, over: &Extent2d) -> NurbsSurface;           // exact; keeps uv (D13)
    pub fn transformed(&self, a: &Affine, over: &Extent2d) -> Surface; // D4: NURBS over `over` for a non-similarity
}
pub fn intersect_curve(c: &Curve, ce: &Extent1d, s: &Surface, se: &Extent2d, tol: Length,
                       budget: &mut Budget, stop: &Stop) -> Result<Vec<CurveSurfaceHit>, SurfaceError>;
```

- **Transforms, per 04's 11-27 row** [Ruled 11-27, D4 revised].
  - Plane, sphere, cylinder, cone and torus are closed under similarities. A sphere under a
    non-uniform scale is an ellipsoid, which is not a step-2 kind.
  - The row says such a map gives "exact rational NURBS". `transformed` therefore returns
    `Surface::Nurbs` for a non-similarity, over an extent the caller passes. `scx-brep` passes the
    face's uv box, never an infinite domain (review-63 H1).
  - A face whose surface changes kind has its pcurves refit by the step-5 fitter, with the
    accuracy recorded (D10). Until the fitter lands, `scx-brep::transform` of a body with an
    analytic face under a non-similarity returns `Unsupported` by name
    (`TransformCause::AnalyticNonSimilarity { kind }`). `Surface::transformed` itself is exact and
    available from SU7.
  - JUDGED, for SU7's builder to measure: under D13 the NURBS image keeps the kind's uv on the
    extent, so many pcurves may come through unchanged. The refit then reduces to a check for
    those, and the builder counts which need more before the step-5 work is sized.
  - A mirror keeps every frame right-handed and reverses the kind's sense (D14).

### 4.4 Truth references missing

- **Curve-surface intersection truths.** Line against plane, sphere, cylinder, cone and torus:
  - exact roots in rationals where the data are rational (line-plane, line-sphere with a rational
    discriminant);
  - otherwise the quadratic or quartic roots certified by the algo crate's isolation, checked in
    double-double.
  - Tangency cases are generated by construction: a line tangent to a sphere at a chosen point.
- **Analytic projection references.** 59 has closest points for every step-2 analytic surface,
  exact for the plane, so these are covered.
  - What is missing is a check that the returned uv is the *global* nearest, for the torus (up to
    four stationary points) and the cone near its apex.
  - The reference should enumerate all stationary points by the closed forms and pick the least
    in double-double.
- **Exact `to_nurbs` of quadrics.** The rational biquadratic image, evaluated in rationals at
  rational parameters of the rational parameterisation, lies exactly on the implicit quadric.
  This is an exact predicate.
- **Poles and normals.** Sphere poles and cone apices: `normal` returns `None` exactly there, and
  the limiting normal from either side matches the closed form.
- **Derivatives above order 2 for analytic surfaces in double-double.** 59 gives derivatives, and
  the order to which they are tested should be extended with the jets.
- **The uv map of D13** [Ruled 11-27, H2]. The testkit's analytic primitives use the angle today.
  Two truths are added:
  - the kind's NURB-compatible uv and the angle form give the same 3-D point at exact rational
    parameters (this is also the converter truth for `scx-step`);
  - `to_nurbs` keeps uv: the NURBS image and the kind agree at the same uv.
- **WR-680's exact cylinder section** (T5): an oblique plane section of a cylinder, as a rational
  uv curve of degree 2p, lifts exactly onto the 3-D conic.

### 4.5 Builder tasks

| Task | Work | Depends on |
|---|---|---|
| SU1 | analytic kinds: construction, `point`, `jet`, generic scalar, `normal`, poles, domain, the D13 uv map | P0, P1 (`Frame`), P2 (`ParamDomain`) |
| SU2 | `Surface` enum, dispatch, Send/Sync, `Nurbs` arm | NS1, SU1 names |
| SU3 | `to_nurbs` for the quadrics and torus, with the exact on-surface predicate | SU1, NS1; T4 |
| SU4 | inversion: closed forms, global-nearest for torus and cone; NURBS inversion over `scx-algo` | SU1, SU2, NS2; T3 |
| SU5 | (moved to `scx-ssi` as SS5: M3) | — |
| SU6 | curve-surface intersection, analytic pairs, budgeted | SU1, CU1, CU4; T2 |
| SU7 | `transformed` per the 11-27 row and D4 (exact; the pcurve refit is step 5's BR9) | SU3 |
| SU8 | `AnyCurve` with `Free` and `OnSurface` | CU2, SU2 |

### 4.6 SU1 as built (11-27)

Built 11-27, proposed for review: decided by analysis and an adversarial pass, not yet ruled.

- **Kinds.** `Plane`, `Cylinder`, `Cone`, `Sphere` and `Torus` hold a `Frame`, their sizes as
  `f64` millimetres (given as `Length`) and, except the plane, a `Sense`. Fields are private;
  every one is `Copy` and at most 120 bytes, checked at compile time. Each has `new`, accessors,
  `point`, `eval<T: Ring>(u, v, near)`, `eval_many`, `jet::<D>(uv, [Approach; 2])`, `normal`,
  `domain`, `to_step` and `from_step`. The cone has `pole()`, the sphere `poles()`.
- **The plane has no sense.** Its normal is the frame's z. A reversed plane is the plane of
  the mirrored frame. `Plane::new(frame)` is infallible and `const`. Its uv is STEP's.
- **Reversed is a mirror (D14).** Evaluation uses `Ze = sense * z`. A reversed kind is the
  forward one mirrored in its frame's xy plane at the same uv, so `S_u x S_v` turns and the
  normal stays along it.
- **`normal` is `Option` only on the cone.**
  - The sketch had `normal -> Option<UnitVec3>` on every kind. Plane, cylinder and ring torus
    have a normal everywhere.
  - The sphere is smooth at its poles. Where `S_u x S_v` vanishes it returns the limit
    (`-Ze` at `v = 0`, `+Ze` at `v = 1`) rather than `None`. This is a change to 4.4's "None
    exactly there", which holds only for the cone's apex.
  - The `Surface` enum (SU2) keeps `Option`.
- **The cone's half-angle is signed** as the pin's semi-angle is.
  - A negative half-angle narrows the cone along `Z`. The sketch had `(0, pi/2)`.
  - Zero is refused (`Degenerate`: a cylinder is its own kind), as is a magnitude of `pi/2` or
    more. The pin reduces the angle modulo `pi`; scx refuses it instead.
  - A zero radius is allowed: the apex is at the origin.
  - The slope `tan(half_angle)` is computed once, in `new`.
  - The `v` domain runs from the apex along the nappe that holds `O`'s circle.
- **The torus is a ring torus.** `minor >= major` (horn or spindle) is
  `SelfIntersecting`, `Unsupported`, which is not a step-2 kind. The pin builds these.
- **Errors.** `SurfaceError` has four causes, each with a kind and `name()`:
  - `NotFinite`, `Degenerate` and `Negative` of a `Size` (`Radius`, `HalfAngle`, `Major`,
    `Minor`), all `Refused`;
  - `SelfIntersecting`, `Unsupported`.
  
  Evaluation cannot fail.
- **The D13 layout**, read from the pin and stated in the crate docs:
  - every angle direction is standard rational quarter arcs, one span each;
  - cylinder `u` is over `[0, 2 pi r]` (arc length);
  - cone, sphere and torus `u` are over `[0, 1]`;
  - sphere `v` is two arcs from the south pole;
  - torus `v` is four arcs from the outer equator;
  - cylinder and cone `v` is the height.
  
  `to_step` and `from_step` give STEP's angles in radians. The cone's `to_step` angle is
  continuous, not reduced to a turn.
- **Jets at breaks.**
  - `Approach` picks the arc in each direction.
  - On a periodic seam, the side off the domain wraps to the arc at the other end, which is the
    periodic surface's true value. The pin's NURB clamps to its end span there (pin defect
    `surface_jet`).
- **Generic-scalar `eval` takes `near`.** It uses the formulas of the arcs that hold `near`. An
  argument spanning a break evaluates one arc's rational form past its span, which is a
  different curve. Callers split at the breaks (`domain`).
- **Cylinder breaks.**
  - scx puts the breaks at `k (2 pi r / 4)`. The pin's arc-length knots come from
    `reparametrize_with_arc_length`, and its period end also differs from scx's by a few ulps.
  - Both are within a few ulps of the true `k pi r / 2`, neither consistently closer. scx's are
    one rounding of a closed form.
  - The difftest evaluates the pin on its own knot when the spec is on a break.
- **The pin's unbounded cylinder and cone are not the reference.**
  - The pin's canonical unbounded forms store their height over `[-1234567, 1234567]`. They
    lose about `1e-10` in `v` (for example `S_u.z = 6e-11`).
  - The difftest uses the bounded forms (`new_cylinder_bounded`, `new_bounded`), whose `v`
    knots are `[0, h]`.
  - This is recorded here as a pin precision limit, not filed as a pin defect.
- **The jet is `scx_num::jet::SurfaceJet`** (NS2). SU1 first built on a crate-private stand-in
  with the same `from_fn`, `point` and `get`; it was swapped for the real type 11-27, with the
  truth tests and the `t_surface` difftest unchanged.

### 4.7 SU2 as built (11-27)

Decided by analysis and an adversarial pass; proposed for review, not yet ruled. The model
is CU2 (§3.7, ruled in 04).

```rust
#[non_exhaustive] #[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    Plane(Plane), Cylinder(Cylinder), Cone(Cone), Sphere(Sphere), Torus(Torus),
    Nurbs(Arc<NurbsSurface<3>>),
}
impl Surface {
    pub fn domain(&self) -> UvDomain;
    pub fn point(&self, uv: Point2) -> Point3;
    pub fn jet<const D: usize>(&self, uv: Point2, side: [Approach; 2]) -> SurfaceJet<Vec3, D>;
    pub fn normal(&self, uv: Point2, side: [Approach; 2]) -> Option<UnitVec3>;
    pub fn eval_many(&self, uv: &[Point2], out: &mut [Point3]);  // panics if the lengths differ
    pub fn eval_grid(&self, us: &[f64], vs: &[f64], out: &mut [Point3]);  // row by row
}
impl From<Plane> for Surface { .. }  // every kind, NurbsSurface<3> and Arc<NurbsSurface<3>>
```

- **`#[non_exhaustive]`, not `Copy`, `From` each kind**: as CU2, for its reasons. The
  `Nurbs` arm holds an `Arc`, so `Copy` was never available.
- **Size asserted at 128 bytes** (§4.3): the largest analytic kind (120, asserted in SU1)
  plus the tag. `Nurbs` is behind an `Arc` (A7).
- **The `Nurbs` arm cannot fail; where the surface's call errs, every coordinate is NaN.**
  Alternative: `Result` on `point` and `jet`. Against NaN: the error's cause is lost through
  the enum. For: CU3's `Curve::Nurbs` arm does the same, so both enums keep one infallible
  signature; an analytic kind's point at an infinite or NaN parameter is already NaN, so a
  caller that checks finiteness handles both; and a `Result` on the enum would make every
  analytic caller unwrap an error that cannot happen. A caller that needs the cause calls
  the `NurbsSurface` itself. The errs are a parameter that is not finite, an overflowing
  value (`Degenerate`) and a jet past `scx_nurbs::MAX_ORDER`.
- **`point` and `eval_many` take the pieces above `uv`** in both directions: the surface is
  continuous, so side changes only roundings, and `point` takes no side (CU1, CU2).
- **`normal` takes `side`; 63 §4.3's sketch did not.** Against: an argument every analytic
  kind ignores. For: a C0 NURBS surface's normal differs across a knot line (tested: a
  right-angle crease gives `+z` below and `-x` above), and adding the argument later would
  change the signature under every caller; this is CU2's reasoning for `jet_over`'s side.
- **The NURBS normal is `unit(S_u x S_v)` from `jet::<1>`, `None` where the jet errs or the
  cross product is below `normalized`'s threshold** (a collapsed edge, a cusp). Alternative:
  the limit from higher partials, as SU1's sphere gives at its poles. Against `None`: a
  NURBS sphere's pole has no normal through the enum while the analytic sphere's does.
  For: the limit needs a rule per degeneracy (which partials, which sign), which is
  inversion and SSI work (SU4, SS); the arm stays thin until a caller defines it. Recorded
  in 05 as open.
- **No scalar-generic evaluation on the enum yet.** The kinds have `eval<T: Ring>(u, v,
  near)`; `NurbsSurface` has no generic or interval form. Two shapes were weighed:
  - *Leave it off* until NS adds an interval NURBS surface evaluation, then add
    `point_over` (CU2's name) for every arm. Against: a certifying caller of an analytic
    kind matches the kinds itself until then.
  - *Restrict it*: `point_over<T: Ring>(..) -> Option<Point3<T>>`, `None` for `Nurbs`.
    Against: the `Option` goes away when NURBS gets the form, which is a breaking change
    for every caller, and meanwhile every caller needs a fallback for `None` that it could
    not write without matching anyway.
  
  Chosen: leave it off. No caller exists yet (SSI certification is SS work), adding the
  method later is not breaking, and the restricted form would ship a signature known to
  change. This is an NS dependency, recorded in 05.
- **`eval_many` writes into `&mut [Point3]` and panics on a length mismatch**, per the CU2
  ruling, and the five kinds moved from `&mut Vec<Point3>` (cleared first) to the same
  shape, with the same panic. Each kind keeps its own `eval_many` (the cylinder and cone
  read their axes once); the enum matches once and calls it.
- **The `Nurbs` arm of `eval_many` wraps NS2's slice-shaped `NurbsSurface::eval_many`**
  (nsurf-2c: `&[[f64; 2]]` in, `&mut [[f64; D]]` out, a length panic), 64 points at a time
  through two buffers on the stack. Alternative: one heap buffer of the batch's size.
  Against chunks: a basis is recomputed at each chunk's first point. For: no allocation,
  and the cost is one basis in 64. The buffers exist because `Point2` and `Point3` have no
  layout promise and `unsafe` is forbidden. A chunk errs at its first bad point, so then
  each of its points is taken alone, NaN only where its own call errs: `out[i]` is
  `point(uv[i])` bit for bit either way (tested through both paths).
- **The enum has `eval_grid(us, vs, out)`**, row by row (`out[i * vs.len() + j]`), panicking
  unless `out.len()` is `us.len() * vs.len()`, as `NurbsSurface::eval_grid`. Alternative:
  leave it off, since no analytic kind has one. Against: one more method, and the analytic
  arms gain nothing (they evaluate each point). For: a caller sampling a face's grid
  (tessellation, bounds, seeding SSI) should not have to match the `Nurbs` arm to get the
  basis computed once per row and column, which is 2.2 times faster than `eval_many` over
  the same grid (table); and an analytic kind can gain a closed-form grid later without a
  signature change. The `Nurbs` arm takes one heap buffer of the grid's size (a grid cannot
  be split into chunks without recomputing the bases of one direction); on an error, each
  point is taken alone.
- **The `Nurbs` helpers are `#[inline(never)]`; the dispatch methods `#[inline]`.** With
  the NURBS evaluation inlined beside the analytic arms, a plane's `point` took 13 ns
  through the enum against 5 ns direct (a hand-written match without the `Nurbs` arm took
  5); out of line, 5.0 against 4.0. A NURBS evaluation is over 100 ns, so its extra call
  is noise. `#[inline(always)]` on `point` changed nothing.
- **A `jet::<2>` through the enum still costs more than direct** for the plane and torus
  (table). The overhead is the five analytic jets inlined into one match: a match over the
  plane alone is as fast as direct, and over the five analytic kinds is not. Marking each
  kind's `jet` `#[inline(never)]` narrowed it on the plane and not reliably elsewhere, on
  a loaded machine, and would cost direct callers their dead-code elimination, so SU1's
  kinds are unchanged. Recorded in 05 as open: worth a look when a hot caller (SSI
  marching) dispatches jets.
- **No difftest through the enum**: as CU2, the unit tests prove the enum gives each kind's
  bits, and `t_surface` and `t_nurbs_surface_eval` already compare the kinds with the pin.
- **"Bit for bit" treats every NaN as one value**, signed zeros apart, as CU2.

Tests (unit, `surface.rs`): `domain`, `point`, `jet::<0>`, `jet::<1>`, `jet::<4>` and
`normal` against each kind's own call, bit for bit, over 60 seeded placements at three
scales and both senses as each analytic kind (300 surfaces) and 40 NURBS surfaces (degrees
1 to 4 and 9, every knot and weight kind), at 33 or more parameters each paired into uv
(signed zero, `1e-300`, `±1e6`, the arc breaks, the NURBS knots, an infinity, a NaN), at
all four side pairs: 89,760 checks, 10,560 on NURBS, 1,286 with no normal. `eval_many`
against `point` per element, through both the batch path and the per-point fallback;
`eval_grid` the same, over grids of 20 by 13 parameters (with and without non-finite ones)
and empty grids; the three length panics (enum `eval_many` and `eval_grid`, a kind); the
conversions, with `Arc` sharing; the crease normal.
Send and Sync are checked in `lib.rs`.

Timings (release, best of 3 runs of 5 passes of 4,000,000 calls, a loaded build machine,
load about 15 on 32 cores; outputs black-boxed; NURBS bicubic rational, 5 by 5 points, one
interior knot each way; a throwaway example, deleted):

| ns per call | direct | via `Surface` |
|---|---:|---:|
| plane `point` | 4.0 | 5.0 |
| plane `jet::<2>` | 17.1 | 32.8 |
| cylinder `point` | 10.4 | 9.4 |
| cylinder `jet::<2>` | 35.2 | 40.3 |
| cylinder `normal` | 16.0 | 17.5 |
| torus `point` | 19.9 | 20.2 |
| torus `jet::<2>` | 48.2 | 69.1 |
| torus `normal` | 25.2 | 26.4 |
| NURBS `point` | 104.8 | 99.9 |
| NURBS `jet::<2>` | 318.1 | 310.8 |
| NURBS `normal` | | 195.7 |

`eval_many` over a 32 by 32 grid, per point: plane 2.5, cylinder 9.3, torus 13.3.

The same NURBS surface over the 32 by 32 grid after the merge of NS2's faster point and
`eval_grid` (a quieter machine, load about 5; best of 7 runs of 2,000 grids), ns per point:

| | direct | via `Surface` |
|---|---:|---:|
| a `point` loop | | 85.5 |
| `eval_many` | 51.7 | 54.4 |
| `eval_grid` | 21.3 | 24.2 |

The enum's batches cost 5% (`eval_many`: the chunk copies) and 14% (`eval_grid`: the
buffer and its copy) over the direct calls; both are recorded in 05 as open, to go when
`Point3` gets a layout that lets the surface write into `out` directly.

### 4.8 SU3 as built (11-27)

Decided by analysis and an adversarial pass; proposed for review, not yet ruled.

```rust
impl Plane /* and Cylinder, Cone, Sphere, Torus */ {
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError>;
}
impl Surface {
    pub fn to_nurbs(&self, over: &Extent2d) -> Result<NurbsSurface<3>, SurfaceError>;
}
// new SurfaceError arms: EmptyExtent { axis, at }, OutsideDomain { axis, min, max },
// Conversion(NurbsSurfaceError), folded into SU7's Nurbs(NurbsSurfaceError) (§4.9)
```

The image is a tensor product of the kind's two directions. A length (the plane's u and v, the
height of a cylinder or cone) is degree 1, the parameter itself. An angle is the rational
quadratic arcs of SU1's `Arcs` (`Arcs::bezier`): one span per quarter arc crossed, each a
sub-arc of the pin's quarter arc written through the homogeneous blossom
`H(x, y) = (1 - xy, x + y, 1 + xy)` of SU1's half-angle form, so the span's control values are
`H(ta, ta)`, `H(ta, tb)`, `H(tb, tb)` turned by the arc's middle. The control point at `(i, j)`
is the kind's own formula `O + f D + g Z` at the directions' control values (affine in each
direction's homogeneous point, so this is exact), and the weight is `wu wv`. Plane, cylinder
and cone are degree (1, 1), (2, 1), (2, 1); sphere and torus (2, 2).

- **Keeps the pin's uv (D13), so the image is off the surface by rounding; the alternative was
  rational weights exactly on it.** In the pin's parameter a quarter arc's middle weight is
  `cos(pi/4)` of its ends' (the arc is symmetric in its parameter, so its end weights are equal,
  and that fixes the middle). No double holds `cos(pi/4)`, so the control data is rounded and
  T4's `Implicit::check` reports the image off, by a first-order distance at most 1.0e-16 of
  the case's scale (38 extents over 10 surfaces, two of each kind; the pin's difftest tolerance `ON_TOL` is 1e-14
  relative). Alternative: weights `(1, 1, 2)` with `w1^2 = w0 w2 / 2` (the testkit's exact
  `circle()`), exactly on the quadric in doubles. Against: that is a different parameter of the
  same arc, so the image and the kind would disagree at the same uv, which is what D13 and 4.2
  rule out (a pcurve would need a map across every conversion). Kept: uv. The plane, being
  polynomial, is exactly on in an integer frame over an integer extent (tested). 4.4's "lies
  exactly on" holds in the reals, and is checked to rounding.
- **`Result`, not 4.3's infallible sketch.** 4.2 argued totality from `Extent2d` being finite.
  It is finite, but not every finite extent is a domain of the kind: a zero-width side
  (`EmptyExtent`), more than one turn of a periodic direction, a sphere's v outside `[0, 1]`
  (past a pole), a cone's v past its apex onto the other nappe (`OutsideDomain`). Alternative:
  clamp or wrap the extent silently. Against: a caller asking for two turns gets one, or a
  sphere that folds back over its pole, and finds out from a wrong intersection. The NURBS
  constructors' own errors (none expected, kept so no `unwrap` sits in the crate) come back as
  `Conversion(NurbsSurfaceError)`, its kind passed through and its source exposed (since
  SU7, §4.9, the one wrapping arm `Nurbs(NurbsSurfaceError)`, named `nurbs_failed`).
- **One period, anywhere.** A turn may start at any u (the seam crossing is one more span, not a
  rewrap), and `hi - lo <= period` exactly, so at most 5 spans. Alternative: accept several turns
  as repeated arcs. Against: the image would cover the surface twice, which no B-rep face does;
  refused instead.
- **A break is placed exactly.** At a quarter break the control point is the axis itself
  (`(1, 0)`, `(0, 1)`, ...), not the formula's rounding of it, so a full turn's knots are
  `[0,0,0, L,L, 2L,2L, 3L,3L, P,P,P]` and a sphere's pole rows are its pole exactly (tested).
  Interior breaks are double knots, the arcs' own `C1` joints; a single knot vector across the
  turn (`C1` rational B-spline) was the alternative, but it gives a different parameter.
- **Every weight is positive.** Splitting at each break keeps `|tau| <= tan(pi/8)`, so
  `1 + xy > 0`; T4's check needs positive weights and NS's evaluator does not divide by zero.
- **`Surface::to_nurbs` restricts the `Nurbs` arm** (`NurbsSurface::split` at each end strictly
  inside the domain, the surface itself on its whole domain), with the same refusals.
  Alternative: no `to_nurbs` on the enum. Against: step 5's B-spline-against-analytic pairs and
  SU7 want one call for "the NURB of this face over this extent" whatever the arm.

Truth (`tests/truth.rs`, 4 new tests, about 22 s in a debug build for the exact check): per
case the whole range, a random piece, a piece from break to break, and one crossing each
seam, over 40 surfaces. The kind's `point` and the testkit's `point_at` (u wrapped into the period) agree with
the image at random uv, the extent's ends and its breaks, to 3.2e-16 of scale (bound 8e-16).
The structure, the exact plane, the refusals by kind and name, and the enum's agreement with
each kind (`==`) and its restriction of a `Nurbs` arm (at the same uv) are checked.

Cost (A3; release build, ns per call, one quiet run; `point` and NURBS `point` for scale, the
image's `point` with `Approach::Above`):

| | `point` | `to_nurbs`, whole | `to_nurbs`, one span | image `point` |
|---|---:|---:|---:|---:|
| plane | 6.5 | 125 | 122 | 63 |
| cylinder | 10.3 | 360 | 186 | 78 |
| cone | 13.3 | 378 | 196 | 78 |
| sphere | 19.8 | 586 | 232 | 84 |
| torus | 21.0 | 899 | 244 | 83 |

"Whole" is one turn each way (the plane `[0, 1]^2`); "one span" an extent inside one quarter
arc each way. So a whole torus costs about 11 image evaluations, a trimmed patch about 3; mostly the four
`Vec`s (knots, points, weights) the NURBS takes. A3's comparison against one analytic
inversion, measured 11-27 after SU4 (release build, ns per call, best of 9 timed batches in the
best of three runs, pinned to one core; the inversion is `invert(p, None)` over 64 queries
around the surface, global, closed form):

| | `to_nurbs`, whole | `to_nurbs`, one span | `invert` | whole / `invert` | span / `invert` |
|---|---:|---:|---:|---:|---:|
| plane | 94.5 | 94.9 | 8.1 | 11.7 | 11.7 |
| cylinder | 285.1 | 145.5 | 51.2 | 5.6 | 2.8 |
| cone | 297.2 | 150.8 | 50.6 | 5.9 | 3.0 |
| sphere | 464.8 | 186.7 | 60.7 | 7.7 | 3.1 |
| torus | 680.2 | 176.5 | 59.2 | 11.5 | 3.0 |

("Whole" is the kind's whole uv extent, so the sphere and torus are one turn each way and the
cone is `v` over `[0.5, 2]`; the three runs agreed within 2%, and the conversion is a little
faster than the table above, whose figures are one earlier run.) A conversion costs 3 to 12
inversions, so the verdict stands and sharpens: convert once per intersection call, never per
query; inverting a point against the analytic kind is cheaper than the NURBS image's own
`point` (63-78 ns), so nothing should convert in order to invert.

From research 66: no technique there changes this conversion. Its implicitisation (§8.4) is
an approximate implicit form to seed SSI; our kinds have exact implicit forms already, which
T4's check uses. Its exact predicates (§16) are runtime float filters for orientation tests;
T4's check is exact rationals in the testkit, test-only, so there is nothing to adopt here.
Its "keep derived data, do not rebuild" (§1 point 3) and OCCT's per-boolean intersection
context (§14) match A3: if the conversion ever shows in a profile, memoise it in the caller's
`Scratch`, keyed by the kind's bits and the extent, never on the kind. Its allocation-free
inner loops (§1 point 4) would ask for a caller-provided buffer; not taken, as the conversion
runs once per intersection call and `NurbsSurface` owns its storage.

Open from SU3: a `to_nurbs` into a caller's buffer if a profile ever asks (the inversion cost
comparison is closed above).

### 4.9 SU7 as built (11-27)

Decided by analysis and an adversarial pass; proposed for review, not yet ruled. The model
is CU5 (§3.9, ruled in 04 11-27), whose rules it follows: reversal negates the parameter,
`restrict` refuses rather than clamps, `transformed` gives the exact image with its
parameter map.

```rust
pub struct UvMap { /* m: [[f64; 2]; 2], c: [f64; 2] */ }  // uv' = m uv + c
impl UvMap {
    pub const IDENTITY: Self;
    pub const fn matrix(&self) -> [[f64; 2]; 2];
    pub const fn offset(&self) -> [f64; 2];
    pub fn apply(&self, uv: Point2) -> Point2;
}
impl Plane {     // and Cylinder, Cone, Sphere, Torus
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap);
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)>;
}
impl Surface {
    pub fn restrict(&self, e: Extent2d) -> Result<Self>;
    pub fn reversed(&self, axis: ParamAxis) -> (Self, UvMap);
    pub fn transformed(&self, a: &Affine) -> Result<(Self, UvMap)>;
}
impl NurbsSurface<D> { pub fn reversed(&self, axis: ParamAxis) -> Self; }  // was Result
impl Frame { pub fn half_turn_about_x(&self) -> Self; pub fn half_turn_about_y(&self) -> Self; }
```

- **Reversal negates the parameter, as the curves' does (the 04 CU5 row).** A NURBS
  surface's knots of `axis` are negated in reverse order (`KnotVector::negated`), so the
  domain `[a, b]` becomes `[-b, -a]`; the edit is total and reversing twice gives the same
  bits. `NurbsSurfaceError::Reversal` (`knots_not_reversible`) is removed: negation has a
  double form for every knot vector. The analytic kinds are exact, `S'(-u, v)` with `S(u,
  v)`'s bits (tested over 18,000 points of every kind):
  - a plane's frame turns half a turn about its other axis (`Frame::half_turn_about_y` for
    u, `_x` for v), negating two axes exactly;
  - a turn direction (u of the cylinder, cone, sphere and torus) turns the frame about x
    (y and z negated) and reverses the sense, so the axis `z · sense` is kept and the quarter
    arcs' odd sine gives `D(-u) = D(u)` on the new frame to the bit;
  - the cylinder's and torus's v reverse the sense; the cone's v reverses the sense and
    negates the half-angle and its slope (copied, not recomputed through `tan`), so the apex
    moves to `-v` with the parameter.

  `Frame::new` recomputes x from a hint and rounds, so the half turns are new `scx-num`
  methods that build the frame from the negated axes directly.
- **The sphere's latitude differs: `S'(u, 1 - v) = S(u, v)`.** Its domain is `[0, 1]` from
  the south pole (D13), and `-v` would leave it. The only kind-preserving reversal is the
  sense reversed (the poles swap), a reflection about the domain's middle, which rounds
  `1 - v` once. Alternatives: give the reversed sphere the domain `[-1, 0]` (against: a kind
  with two layouts, which D13 and SU1 exclude, and every caller of `domain` and the poles
  would learn both), or refuse it (against: a face on a sphere must be reversible). Because
  this one direction is not the negation, `reversed` returns the `UvMap` for every kind,
  so a caller maps pcurves through the map rather than assuming `-v`.
- **`UvMap` is the surface `ParamShift`: a 2-D affine map**, not a shift. A transform scales
  the directions measured in lengths (the plane's two, the cylinder's arc length and
  height, the cone's height) by the map's scale; angle directions over `[0, 1]` are kept.
  A plane under a map that is not a similarity needs a triangular map (below), and the
  sphere's reversal an offset, so a per-direction scale would not do. `apply` skips zero
  terms, so a negation is exact and an infinite coordinate stays infinite.
- **A plane maps to a plane under any affine map.** The image frame takes `A x` as its x
  and `A x × A y` as its z; then `A y = (A y · x') x' + (A y · y') y'`, so `u' = |A x| u +
  (A y · x') v`, `v' = (A y · y') v`. Within 16 roundings of a similarity on the plane (the
  CU5 threshold) the map is the mean scale in both directions.
- **A surface of revolution keeps its kind under a similarity only**: all three axes'
  images square and of one length to 16 roundings. The frame comes from
  `Frame::transformed` (a mirror gives a right-handed frame and reverses the sense, D14),
  the sizes scale by the mean length. Under any other map the exact image is a NURBS
  surface, and it stays `SurfaceError::NotSimilar` (`Unsupported`), refused rather than
  snapped to the nearest similarity.

  **Follow-up, open: the NURBS image under a non-similarity.** SU3's `to_nurbs` (§4.8) now
  gives every kind's exact image, and `NurbsSurface::transformed` maps it, so that arm can
  be built. What blocks it is the signature. A kind carries no extent, and `to_nurbs` needs
  one (one turn at most, inside the poles and the apex), so `transformed(&self, a)` cannot
  build it alone. The options:
  - (a) `transformed(&self, a, over: &Extent2d) -> Result<(Surface, UvMap)>`, the §4.3
    sketch: one call per face. Against: the extent is unused on every similarity, and most
    calls are similarities.
  - (b) Keep `NotSimilar` and let the caller write `to_nurbs(over)?.transformed(a)`, with the
    identity `UvMap`, which is what (a) would do inside. Against: every caller learns the
    fallback.
  - (c) A separate `transformed_over(a, over)` that tries the kind first and falls back.

  Leaning (c) or (b), so the common call keeps its signature; to be ruled with the step
  that first needs it (a non-uniform scale on import). Also open: an axial-only scale
  keeps a cylinder a cylinder and a cone a cone, and could be added without NURBS.
- **`restrict` checks the extent against the domain for every kind.** An analytic kind
  carries no extent, so it comes back unchanged; a NURBS surface is split at each end inside
  its domain by SU3's restriction (the domain itself gives the same `Arc`). An extent of a single value in a
  direction is `EmptyExtent`, one past an end of the domain (a pole, the apex, the knots) is
  `OutsideDomain`; periodic and endless directions have no end to pass. This differs from
  CU5's analytic curves, which take any extent: a line or conic has no ending domain, while
  the sphere and cone do. `restrict` and `to_nurbs` share the two extent variants (SU3's
  names, kinds and fields, `extent_empty`, `extent_outside_domain`); `to_nurbs` alone also
  refuses more than one turn of a periodic direction, since it builds the turns, while
  `restrict` keeps an analytic kind's periodic parameter whole.
- **Errors.** `SurfaceError` gains `MapNotFinite`, `MapFlattens` (both `Refused`) and
  `NotSimilar` (`Unsupported`). It shares SU3's `EmptyExtent` and `OutsideDomain`. SU3's
  `Conversion(NurbsSurfaceError)` is folded into one wrapping arm, `Nurbs(NurbsSurfaceError)`
  (`nurbs_failed`), for conversions, splits and edits alike: a context frame with the NURBS
  error's kind and `source`. It stays `Copy` at 48 bytes, the NURBS error's size, as on main
  since SU3; `Surface` stays 104 bytes, under the 128-byte budget (62 D14).
- **`to_nurbs` composes with `reversed`.** For every kind and direction, the image of the
  reversed kind over the mapped extent equals the reversal of the kind's image. Where the
  reversal negates, the knots, control points and weights match bit for bit. The sphere's
  latitude matches the NURBS reversal shifted by 1, to rounding, because its map is `1 - v`
  where the NURBS reversal's is `-v` (test `to_nurbs_of_a_reversal_is_the_reversal_of_to_nurbs`).

Measured (`std::time::Instant`, best of 7 runs of 20,000 calls, a 20 by 20 rational cubic,
reversed along u): 2298 ns before, 1573 ns after (-32%). The negation does strictly less work than the reflection:
one negation per knot instead of a subtraction and up to three comparisons, and no knot
vector validation or continuity check (each a pass over the knots). No evaluation path
changed: the kinds gained only `const` rebuild helpers, `Surface` keeps its size (104
bytes), and the difftest's `nurbs_surface_reversed` pin defect now checks negation (exact
knots, the exactly flipped net, and the point at `-t` from the other side for degrees up to 3).
### 4.10 SU8 as built (11-27)

Decided by analysis and an adversarial pass; proposed for review, not yet ruled.

```rust
#[non_exhaustive] #[derive(Debug, Clone, PartialEq)]
pub enum AnyCurve { Free(Curve), OnSurface(CurveOnSurface) }   // at most 112 bytes
pub struct CurveOnSurface { uv: Curve2, surface: Arc<Surface> } // private fields
impl CurveOnSurface {
    pub fn new(uv: Curve2, surface: impl Into<Arc<Surface>>) -> Self;
    pub fn uv(&self) -> &Curve2;  pub fn surface(&self) -> &Arc<Surface>;
    pub fn domain(&self) -> ParamDomain;  pub fn point(&self, t: f64) -> Point3;
    pub fn jet<const D: usize>(&self, t: f64, side: Approach) -> CurveJet<D>;
    pub fn eval_many(&self, ts: &[f64], out: &mut [Point3]);
}
// AnyCurve: domain, point, jet::<D>, eval_many by match; From<Curve>, From<CurveOnSurface>.
```

Why SU8 next, not SU5 or SU6: SU5 moved to `scx-ssi` as SS5 (M3). SU6 depends on CU4, which
is not on main. SU8 depends on CU2 and SU2, both done.

- **A struct, not a one-arm `SurfaceCurve` enum.** 56 §7.1 names the arm `OnSurface(SurfaceCurve)`.
  Only the parameter-space curve exists in step 2, so an enum would have one arm and nothing
  to tell apart. Against it: speculative structure. The struct can become an arm of such an
  enum when another kind of on-surface curve exists.
- **The surface behind an `Arc`, not held by value.** The curves on one face share one
  surface. By value, `Surface` (up to 128 bytes) would sit in every pcurve and push
  `AnyCurve` past the free curve's size. With the `Arc`, the arm is a `Curve2` (at most
  64 bytes) and a pointer, and `AnyCurve` stays at 112 bytes or less. `new` takes
  `impl Into<Arc<Surface>>`, so a caller can pass a `Surface` or an `Arc` it already holds.
- **None of the pin's extra state.** The pin's `CrvOnSurf` holds a uv domain of interest,
  owner flags, a break cache, an approximation cache and a curve cache. The `Arc` replaces
  the owner flags. The domain is the uv curve's, and the caller trims as it does for any
  curve. The caches are derived data, which A3 says to memoise in the caller if a profile
  asks. Against keeping them: interior mutability in a value type, and a `PartialEq` that
  would have to ignore them.
- **`new` cannot fail.** Every `Curve2` has a point at every `t`, and every surface has a
  point at every `uv`. A NURBS surface continues its end pieces past its knots, and the
  analytic kinds are defined everywhere. The pin's constructor checks only its nesting
  depth, which does not arise here. Against `Result`: no arm of it is reachable.
- **The surface's side in each direction follows the curve, not `side`** (more correct than
  the pin). The pin's `evaluate` passes `from_left` as the side in both u and v. Where the uv
  curve crosses a knot line backward (u falling as t rises), that takes the partials of the
  piece the curve is leaving. scx reads the side from the sign of the coordinate's first
  nonzero derivative, turned for an odd order when `t` moves down. When every derivative to
  order D is zero, no partial along that direction reaches the result, and `side` is used.
  This also handles a curve tangent to a knot line (`u' = 0`, `u'' < 0`), which stays below
  the line on both sides. Tested on a C0 crease, `(2u, v, 0)` meeting `(1, v, 2u - 1)`: each
  case matches the one-sided difference, and swapping in the pin's rule fails both tests.
  Where the sides agree, the result is the pin's.
- **Chain rule to any order, written out to 3; the pin caps at 3.** Orders 1 to 3 use the
  pin's `lift_second_derivative` and `lift_third_derivative` with the same grouping and sum
  order, so from the same partials they give the pin's bits. Past order 3, the Taylor series
  is used: with `du = sum u^(k) h^k / k!`, take `sum S^(i,j)/(i! j!) du^i dv^j` truncated to
  degree D, then multiply by `n!`. The alternative was Taylor for every order: one code path,
  but it cost +40 to +100 ns at order 3 against +6 ns for the written-out form (table
  below). Both paths are checked to D = 6 against the composite polynomial in exact
  rationals, with random dyadic jets, to 1e-14 of the absolute-value bound. They agree with
  each other at D ≤ 3, bit for bit at order 1.
- **No `point_over` yet.** `Surface` has no scalar-generic evaluation (open from SU2), so the
  composite has none.
- **`eval_many` goes through a 64-point stack buffer of uv points** into `Surface::eval_many`,
  so it needs no allocation and gives `point`'s bits.
- **`#[non_exhaustive]` and a size assert (≤ 112 bytes).** The intersection-curve arm (step 5)
  and the in-volume and projected arms (56 §7.1) join later without breaking callers.

Truth:

- Unit tests (3): the exact composite to D = 6; written-out against Taylor; the side rule.
- `tests/any_curve.rs` (7):
  - a uv `Circle2` on a `Plane` equals the 3-D `Circle` to order 8, at 1e-13 of scale, 40
    random frames;
  - the crease crossed backward, and the crease touched;
  - `eval_many` against `point` bit for bit, over 0 to 200 points on a sphere, a NURBS and
    the free arm, plus the length panic;
  - the `Free` arm bit for bit against `Curve`'s calls;
  - the `OnSurface` arm as `Surface::point` at `Curve2::point`.

No difftest. Orders 1 to 3 are the pin's formulas term for term. The surface jets are already
difftested in `t_surface`, the uv curve jets in CU2, and the side rule differs from the pin on
purpose.

Cost (A3; release build, ns per call, best of 7 over 1e6 calls). The box was loaded (load
average 8 to 12), so the figures are good to about 5%. "Before" is the direct calls a caller
makes today, with the pin's written-out lift added for jets.

| | before | `AnyCurve` |
|---|---:|---:|
| free circle `point` (`Curve::point`) | 29.1 | 29.9 |
| free circle `jet::<2>` | 27.4 | 27.5 |
| sphere `point` (`Curve2::point` + `Surface::point`) | 47.8 | 48.0 |
| sphere `jet::<2>` (uv jet + surface jet + lift) | 88.0 | 94.6 |
| sphere `jet::<3>` | 115.8 | 126.2 |
| bicubic NURBS `point` | 126.0 | 125.6 |
| bicubic NURBS `jet::<2>` | 321.7 | 315.3 |
| bicubic NURBS `jet::<3>` | 443.5 | 405.1 |
| sphere `eval_many`, per point (against a loop of `point`) | 50.3 | 41.9 |
| bicubic NURBS `eval_many`, per point | 123.6 | 108.1 |

The free arm and `point` cost the same as before. On the sphere, jets cost up to 9% more. The
added work is the side rule: two scans of D values each. On the NURBS surface the
differences are within noise. Before the written-out path, the Taylor-only lift cost 224 ns
at order 3 on the sphere and 588 ns on the NURBS surface.

From research 66: nothing more to take. Its "keep derived data, do not rebuild" (§1 point 3)
is A3's memoise-in-the-caller, which is why there is no cache here.

Open from SU8:
- `point_over` waits for a scalar-generic `Surface` evaluation;
- the intersection-curve arm comes in step 5;
- normals along the curve (the pin's `evaluate_surface_normal`) are left to callers, which use
  `Surface::normal` at `uv().point(t)` with the sides from the same rule. The rule would be
  exposed if a second caller needs it.
### 4.11 pcurve as built (11-27)

Ruled by the coordinator 11-27 for Brett's review (option 3 of four), built in two
commits: the unambiguous pairs with typed refusals, then the angle line.

```rust
// scx-surface
pub fn pcurve_of(curve: &Curve, surface: &Surface, over: Extent1d, tol: Length)
    -> Result<Curve2, PcurveError>;
pub enum PcurveError {            // #[non_exhaustive]; kind() and name()
    EmptyInterval { .. }, BadTolerance { .. }, NotOnSurface { .. },   // Refused
    CrossesPole { at: f64 },                                          // Refused
    NoClosedForm { curve: CurveKind, surface: SurfaceKind },          // Unsupported
    Inexact { .. },                                                   // Failed
}
// scx-curve
pub enum AngleAxis { U, V }
pub struct AngleLine2 { arcs: Arcs, axis: AngleAxis, start: f64, sense: Sense, other: f64, rate: f64 }
pub enum Curve2 { .., AngleLine(AngleLine2) }
pub mod arcs;                     // Arcs, ArcTrig, parameter_jet (moved from scx-surface)
```

| Pair | Pcurve |
| --- | --- |
| line, circle, ellipse, NURBS curve on a plane | the kind kept, through the plane's affine uv |
| line along a cylinder's or cone's ruling | `Line2`: `u` fixed, `v` affine |
| circle about the axis of a cylinder, cone, sphere or torus (a parallel) | `AngleLine2` in `u`, `v` fixed |
| sphere's great circle through the poles, on one side of them | `AngleLine2` in `v` (the half `Arcs`), `u` fixed |
| torus's tube circle | `AngleLine2` in `v`, `u` fixed |
| meridian over an interval across a pole | `CrossesPole { at }`: the caller splits there |
| anything else, any curve on a NURBS surface | `NoClosedForm` |

**The decision (option 3: an exact `Curve2` kind for an angle-direction line).** The
angle line's angle coordinate is `arcs.parameter(start ± t)`, the arc parameter of the
D13 quarter-arc map; the other coordinate is `other + rate t` (`rate` is 0 for every
case built here; it is kept for a helix-like line on a cylinder later).

- The case for:
  - It keeps the edge a `Circle` and the pcurve exact; nothing is fitted (D10).
  - The pcurve runs at the edge's own parameter (same-parameter), as STEP's
    `pcurve` and OCCT's `BRep_CurveOnSurface` expect; BR5 needs no reparametrisation.
  - `scx-step` needs the inverse map anyway, to write the surfaces' radians uv.
  - The cost is one `Curve2` arm and a small closed form: no effect on any other path.
- The case against:
  - A new kind every `Curve2` consumer must handle; inversion of it is
    `Unsupported::AngleLineInversion` for now (a closed form exists: the angle's
    `Arcs::angle`; not needed until SS or BR asks).
  - Its second derivative jumps at an arc break (the map is C^1), so jets take a side,
    as a NURBS curve's do.
  - A rational quadratic `Curve2::Nurbs` (option 1) would have been an existing kind,
    but it lifts at a different parameter from the edge (not same-parameter), and needs
    `scx-nurbs` in every pcurve; rejected, with options 2 (a radians uv, which breaks D13)
    and 4 (fit, which D10 forbids where a closed form exists).

**Arcs placement.** `Arcs` moved from `scx-surface` to `scx-curve` as `pub mod arcs`, so
both `Curve2` and the surfaces reach it without a cycle (`scx-surface` depends on
`scx-curve`). Its parameter function is pure arithmetic, so `scx-num` was possible too;
`scx-curve` keeps it beside its one 2-D user and out of the numeric layer. The struct
`Trig` became `ArcTrig` (it clashed with `scx_num::trig::Trig` once imported). The move
is code-identical; new is `parameter_jet::<T, D>(phi, side)`, the arc parameter with `D`
derivatives over a generic scalar (interval enclosures included), at most 8 arcs per call.

**The signature.** `pcurve_of` takes `tol: Length` beside the ruled
`(&Curve, &Surface, interval)`: the lifting check needs a bound, and the caller (BR5)
holds the edge and face accuracies. A deviation, recorded for review.

**How a pair is chosen.** A circle is tried as a parallel or a meridian when its axis and
centre are within `NEAR = 1e-6` (relative: radians for the axis, over the radius for the
centre) of that configuration. That only picks the candidate: every answer is checked by
lifting at 9 samples (ends included) against `tol`, so a circle near one but not in it
is `Inexact` and one far from both is `NoClosedForm`.

**The period window (07 §3 rule 5).** Where a direction is periodic, the pcurve's
coordinate at the interval's start lies in the surface's base window. `start` is moved
by whole turns, then the nearest of `start` and `start ± 2 pi` whose start coordinate is
in the window is kept, so a start one rounding off the seam neither ping-pongs nor lands
a period out (tests allow 1e-15 of the period at the seam).

**Poles.** A sphere's meridian runs on one half-plane, with latitude in `[-pi/2, pi/2]`
(slack 1e-14 for the rounded ends); pole to pole fits. Past a pole the azimuth jumps by
half a turn, so no single angle line exists: `CrossesPole { at }` gives the curve
parameter of the pole, and the caller splits the edge there. The pin instead lets the
latitude run past `pi/2` with `u` fixed, which lifts onto the right points in its radians
uv but leaves the surface's domain.

**How the pin differs** (`eval/pcurve.rs`, `exact` and `meridian`). The pin's pcurves are
degree-1 splines between the interval's ends in a uv whose angle directions are radians:
a parallel is `[u0 ± t, v]`, a meridian `[u0, alpha ± t]`, exact in that uv and at the
edge's parameter. Ours are in the NURBS uv (D13), where the same curves are angle lines;
a straight segment there would not be. The plane cases and rulings agree with the pin's
up to the kinds kept (a circle on a plane stays a `Circle2`, not a spline). The pin also
does not refuse a meridian across a pole (above).

**The SS5 perpendicular-section gap.** §5's SS5 text says plane sections' pcurves on
cylinders, cones, spheres and tori are rational `Curve2::Nurbs` in the kind's uv (WR-680).
That holds for an oblique section (a cylinder's ellipse), which `pcurve_of` refuses as
`NoClosedForm`. A section perpendicular to the axis is a parallel, whose exact pcurve at
the edge's parameter is an `AngleLine2`, not a NURBS; SS5 should return that (or call
`pcurve_of`) and keep `Curve2::Nurbs` for the oblique ones.

**Truth check.** Every case is lifted (pcurve, then surface, then compared with the
curve's point) at 101 samples to 1e-12 of size, over scales 1e-3, 1 and 1e3, both senses
of circle and surface, circle axes with and against the surface's, phases across the
seam and intervals past a turn, negative and short. `AngleLine2` is checked against the
arc-parameter identity, finite differences off the breaks, C^1 not C^2 at a break with
the side honoured, reversal, and interval enclosure.

**Added work.** Per `pcurve_of` call: 9 curve evaluations and 9 distances (the
on-surface check), the closed form (a few dot products and one or two `atan2`), then 9
pcurve and 9 surface evaluations (the lifting check). `AngleLine2::point` is one
`Arcs::parameter` (one `sin`, one `cos` and a division) per call. Existing paths gain no
work: `Curve2` gains one match arm, the `Arcs` move changes no code, and nothing yet
calls `pcurve_of` (BR5 will).

### 4.12 SU4 as built (11-27)

Inversion of a surface (63 §4.2, X8, D7; research 66), in the form CU4 uses for the
curves, in two parameters. Decided by analysis and an adversarial pass; proposed for
review, not yet ruled.

```rust
// scx-surface::invert
pub use scx_num::invert::{Foot, Inversion};  // unchanged
pub enum Closest {
    One(Foot<Point2>),
    AlongU { v: f64, distance: f64 },  // an axis query: a circle of feet, or a pole
    AlongV { u: f64, distance: f64 },  // a query on a torus's tube centre circle
    Every { distance: f64 },           // a sphere's centre
}
impl Closest { pub const fn distance(&self) -> f64; }
impl Plane {  // and Cylinder, Cone, Sphere, Torus
    pub fn closest(&self, p: Point3) -> Closest;
    pub fn invert(&self, p: Point3, domain: Option<&Extent2d>) -> Inversion<Point2>;
}
impl Surface {
    pub fn invert(&self, p: Point3, near: Option<Point2>, domain: Option<&Extent2d>,
                  budget: &mut Budget, stop: &Stop) -> Result<Inversion<Point2>, SurfaceError>;
}
pub enum SurfaceError { .., Exhausted(Exhausted), Stopped, Defect { reason: &'static str } }
```

- **Each analytic kind is a closed form, global, infallible.** Every kind is a plane or a
  surface of revolution, so the nearest points lie in the query's meridian plane: the
  projection, the radial foot, the foot on the generator of the query's half-plane or the
  apex (cone), the foot on the near meridian circle (torus). The `uv` is read by `Arcs`'
  inverse with no transcendental, so the foot's `point` is the kind's `point(at)` exactly.
  Alternative: one Newton for every kind. Against: a local answer, a budget and an error
  where none can happen.
- **`Closest` names the sets of feet; `invert` picks one.** Alternative: always one foot.
  Against: a caller building an SSI start or a degenerate-case test needs to know the answer
  is a circle, not a point that happened to be chosen. For picking in `invert`: the domain
  decides it (the foot at the domain's start in the free parameter), and periodic
  parameters wrap by whole periods to start at the domain's min, so an outside foot is
  reported `Outside`, never moved in.
- **`Surface::invert` takes `&mut Budget` and `&Stop`, which only the NURBS arm uses.**
  Alternative: a separate `Surface::closest` for the analytic arms and a NURBS-only call.
  Against: every caller of the enum would dispatch again. For: one signature; the analytic
  arms ignore both and `near`, and return `Ok`.
- **NURBS: a certified branch and bound over Bezier patches through `global_solve`**, per
  research 66: no presearch grid. Each knot span pair is one patch, its homogeneous points
  read off by blossoming (unclamped knots need nothing more). The search walks a balanced
  tree of the patches, nearer half first, pruning a node whose bound is not below the
  incumbent less `eps`; a leaf it reaches is refined best first, keyed by (bound, sequence
  index), a total order, so the bits never depend on a tie. `eps` is `GAP_REL` (1e-12) of the
  control net's diagonal plus the first incumbent's distance. The incumbent starts at the
  best patch corner, polished by Newton from it and from `near`; a corner that beats it by
  more than `eps` starts another run. Newton is on `|S - q|^2 / 2` with an active set at the
  domain's edges, Gauss-Newton where the Hessian is not positive, a gradient step where that
  is singular, and a step kept only when it lowers the distance. Alternative: Newton from a
  grid of starts (solidcore's shape). Against: research 66, and no certificate: a narrow
  valley between grid points is missed.
- **Two lower bounds per piece, the larger taken.** `N / W` with `N = D . D`, `W = w^2`,
  `D = w (S - q)` in Bernstein form (`scx_algo::bernstein`), bounded below by the least
  coefficient ratio; exact where the distance is constant (a query at a sphere's centre is
  certified in 3 units). And a hull bound: `(S - q) . n` is a positive-weighted mean of the
  `(D_k . n) / w_k`, so no less than their least, for `n` the unit direction toward the
  corners. Against `N / W` alone: next to a tiny weight, `N`'s coefficients are tiny against
  its largest and the bound stays near zero however small the piece; the hull bound takes no
  product and stays sharp there.
- **Every coefficient carries its own error bound.** Alternative: `Bounded`'s one bound for
  every coefficient. Against: it is absolute and inherited by subdivision, so beside a
  weight near `2^-40` it swamps that weight's coefficients and the bound stays useless
  however small the piece; the search did not finish on such surfaces. For per-coefficient
  errors: each step of the blossom is taken from its nearer end (`a + s (b - a)` or
  `b + r (a - b)`, `r` from the knots, so no cancellation beside clustered knots), a step
  at a knot is exact, and the error of a mix is the mix of the errors. scx-algo is unchanged: the forms are seeded with the largest error.
- **Anisotropic halving; one-ulp spans become edges.** A piece is halved by de Casteljau at
  1/2 along the parameter whose control polygon is more than twice as long, else in both.
  Against quartering only: a tiny weight crowds the surface into a thin strip that quartering
  covers with about `1 / width` pieces. A parameter whose interval holds no float strictly
  inside (a knot span of one ulp, whose patch can still be long) is not halved: the piece
  becomes its two edges, the only `uv`s that exist there. So the result is the global
  nearest over the floating point `uv`s, to `eps`.
- **The budget is the one bound.** One unit per tree node tested, piece refined and Newton
  evaluation; the stop flag is polled at the same points and by the solve before each
  branch. Both end in an error, never a partial foot, and a search that finishes gives the
  same bits whatever the limit. The solve's own branch limit is set to `u64::MAX`.
  **Recommended for later (not done here):** `global_solve::solve` should take
  `&mut Budget` and `&Stop` itself, as every other iterating call does; today the solver
  carries them inside its `BranchSolver` and maps them out through `Unfinished`, and its
  `max_branches` is a second, weaker bound to lift.
- **Errors.** `SurfaceError` gains `Exhausted(Exhausted)` (kind `Exhausted`, the budget's
  own name), `Stopped` (kind `Cancelled`) and `Defect { reason }` (kind `Defect`,
  `inversion_defect`: a patch's Bernstein form could not be built, a bug).
- **Placement.** `Closest` lives in scx-surface, not scx-num: it names the kinds' parameters
  (`AlongU`, a pole). Commit 99b897b carries only the scx-num half of c6b24fa (`Foot`,
  `Inversion`, unchanged), with `Budget` and `Stop` from c57177f.

Tests (`tests/invert.rs`, 6; `tests/nurbs_invert.rs`, 8), against truth, not the pin: plane
feet are projections; cylinder and sphere feet match double-double references; cone and
torus feet are no farther than a 121 by 121 sample of the uv domain; every set of feet
(axis, pole, apex, tube circle, centre); placement against domains, wrapped and not. NURBS:
random B-splines and NURBS (unit, moderate and near-zero weights, down to `2^-40`)
and an unclamped B-spline, each foot the
surface's own point, no farther than a 61 by 61 sample, and stationary where it is interior
and off a knot; exact NURBS spheres, cylinders and tori give the analytic distance; the
budget and the stop flag end the search, and a clear flag or a warm start change only the
work.

Work, in budget units, over the 330 random queries: median 78, p90 368, max 53045. A
sphere's centre: 3. Cold against warm (started at the cold foot) on one wavy surface, ten
queries: 78/74, 85/104, 175/202, 68/82, 111/70, 149/195, 44/50, 53/54, 71/63, 52/63; the
distance agrees to `eps` every time. A warm start buys little and sometimes costs: the
certificate, not finding the foot, is most of the work (likely too: an incumbent that
differs by rounding prunes differently; not measured). So `near` is kept as a hint, not
relied on for speed.

### 4.13 SU6 as built (11-27)

Built on SU4's `Budget` and `Stop` (§4.12), for review.

```rust
// scx-surface
pub enum Contact { Crossing, Tangent }
pub enum CurveSurfaceHit {
    Point { t: f64, uv: Point2, point: Point3, contact: Contact },
    Overlap { t: Extent1d },
}
pub fn intersect_curve(c: &Curve, ce: &Extent1d, s: &Surface, se: &Extent2d, tol: Length,
                       budget: &mut Budget, stop: &Stop) -> Result<Vec<CurveSurfaceHit>, SurfaceError>;
pub enum SurfaceError { .., NurbsIntersection /* Unsupported */, Exhausted(Exhausted), Stopped }
// scx-curve: Arcs::of_direction(x, y) is pub (the D13 parameter of a direction, no atan2)
```

| Curve | Plane | Cylinder | Cone | Sphere | Torus |
| --- | --- | --- | --- | --- | --- |
| line (one Bernstein piece) | 1 | 2 | 2 | 2 | 4 |
| circle, ellipse (two rational half turns) | 2 | 4 | 4 | 4 | 8 |

The table is the degree of `g(u)`, the surface's implicit equation on the curve's piece.
`intersect_curve` takes the curve into the surface's axes (frame z times sense), writes
each piece in homogeneous Bernstein form on `u` in `[0, 1]`, and isolates two sets of
roots with `scx-algo`'s `bernstein_roots`:

- the roots of `g`;
- the turning points of `g / H^d`, which are `g'` for a line and `H g' - d H' g` for a
  conic (degree at most 9).

Those roots and the ends of the extent are events in `t`. Each root is refined on the
surface's own equation. Each event is judged near or far by the distance at the tolerance.
A run of near events then gives one answer:

- an overlap, when every event is near;
- a `Tangent` at the nearest turning point;
- a `Crossing` at the root, or at the end when the run has only an end.

Every point gets the D13 uv of its foot, with periods wrapped into `se`, and a point
outside `se` is dropped. The results are sorted by `t`, then `u`, then `v`.

**Bernstein isolation for every degree, not the closed forms.** §4.2 said the pairs
reduce to "polynomials of degree up to 4" in `scx-algo`'s solvers. In fact a conic against
a torus has degree 8, and the turning points reach degree 9.

- The case for:
  - There is one certified path for every pair, and its answer is final. A root exists
    exactly where the Bernstein coefficients certify a sign change, so an empty answer
    needs no grid fallback (04).
  - `solve_quadratic` has an absolute `zero_tol` and cancels on a near-double root. Those
    are exactly the near tangents this query has to classify.
  - The Bernstein form of the half turns needs no `tan` substitution at the seam. The
    halves share their end control point bit for bit, so a sign change at the seam is
    seen once.
- The case against:
  - Quadratics cost more than the formula would. Measured, the plane (degree 1 and 2) is
    1.1 µs a call, 20 times faster than the pin, so no degree-2 fast path was added. It
    can be added later without changing any answer.

**The polish.** A root of the Bernstein `g` carries the rounding of its coefficients. That
rounding is large for the torus's degree 8, where a crossing came out 9.3e-10 off the
surface. The fix is up to 3 Newton steps on the surface's affine implicit, evaluated at the
curve's point. A step is kept only while it lowers `|f|` and moves `t` by less than 1e-6 of
the curve's scale, so a root cannot move to another one. With the polish the worst crossing
is 1.95e-13 from T2's root, at size 20.

- The case for:
  - The steps are deterministic, bounded and cheap (at most 3 evaluations per root).
- The case against:
  - A second equation beside `g`. The two must agree, and they are tested to.

**Tangency at the tolerance.** Between two events `g` is monotone and keeps one sign. That
is exact for the plane, cylinder and sphere, where `g` is a function of the distance. For
the cone and torus it holds at the tolerance: their `g` has a turning point near, but not
at, the distance minimum.

The rules:

- A Tangent is placed at the nearest turning point of the run, within `tol`.
- Crossings closer than `tol` can tell apart merge into that one Tangent. A segment that
  enters and leaves a sphere within one tolerance of depth is one touch, not two
  crossings.

- The case for:
  - The answer is exact: every point is on the curve and within `tol` of the surface.
  - Every T2 root is either a crossing within rounding or joined to a tangent by a stretch
    that stays within `tol`. The tests check exactly that.
- The case against:
  - A Tangent's point is where the turning point is, not the true distance minimum. For
    the cone and torus they differ by up to the tolerance.
  - A segment shorter than its distance to the surface, lying wholly within `tol`, is
    reported as an Overlap. This is the same rule as a curve lying on the surface, applied
    at the tolerance.

**Open for review.**

- **The overlap is not clipped by `se`.** An `Overlap` covers the whole `ce`. Clipping a
  curve's stretch to a uv box needs the curve's pcurve, which `pcurve_of` (§4.11) now
  gives. Until then the caller clips. (Done 11-27, §4.14: clipped into pieces, without a
  pcurve.)
- **A NaN `tol` gives no tangents.** It is not refused. `pcurve_of` refuses a bad
  tolerance (`BadTolerance`), so a typed refusal may be the consistent rule here too. (Done
  11-27, §4.14: `SurfaceError::BadTolerance`.)
- **A closed `ce`** (a turn or more) is the one turn from `ce.min()`.

**Errors.**

- A NURBS curve or surface is `NurbsIntersection` (Unsupported, `nurbs_intersection_unbuilt`),
  never an empty answer.
- `Exhausted` and `Stopped` are SU4's variants, with the same names, kinds and Display.
  SU4's `Defect` is not used: the closed forms have no inside failure to
  report.
- The budget is charged 2 units per piece (one for each isolation), and `stop` is checked
  before each piece. Neither returns a partial answer.

**A14: the pin's callers that take the first hit.** The pin's curve-surface solve returns
its hits by curve parameter, ascending (`add_sorted_solution`, by all parameters). It
re-sorts by `t` when there are coincident ranges or several gaps. So "first" means
smallest `t`, and the `t, u, v` sort serves it with no change.

Four production sites take the first hit:

- `blend/fillet_solver/p2.rs:2902`, the corner trim;
- `blend/fillet_solver/p1/rails.rs:613`, the support edge's end;
- `blend/fillet_geom/p1.rs:2009`, the cliff vertex;
- `intersect/curve_class/p2.rs:1703` and `:1875`, the seed for the refined `(t, uv)`.

Three more depend on order but do not take the first hit:

- `offset_geometry_creation.rs:1317`, which reads every hit as having one parameter;
- its other caller at `:5281`;
- the boolean's `topology_intersector/p1.rs:1002`, which processes edges in `t` order.

Every other caller uses all the hits and re-sorts or picks the nearest. That includes the
ray pick in `query/brep.rs`, which sorts by distance along the ray. Where a caller of ours
wants the hit nearest a guess, the rule stands: that is a filter at the caller.

**How the pin differs.** `SmApiIntersectCurveSurface` (`GlobalCurveIntersect`, within
0.001) agrees on every pair in the difftest but one. In that pair a segment cuts a sphere
of radius 1.65 along a chord of about 0.25, dipping 4.6e-3 inside, which is 4.6 times the
tolerance. The pin reports a coincident stretch and one point. scx reports two crossings,
which T2's roots confirm. This is the pin's defect, checked against T2 in
`pin_defects::curve_surface`.

**Truth check.**

- Unit tests (`tests/intersect.rs`, 39 s) check against testkit T2 and against exact
  checks:
  - every point is on the curve at its `t`, inside `se`, and its foot is within the
    distance;
  - every crossing is within `1e-11` of size of a T2 root;
  - every tangent is within `tol`;
  - every T2 root is found.
- The random pairs: 2,800 pairs at `tol` of 1e-6 and 1e-3, with 3,106 roots and 3,104
  crossings. The worst crossing is 1.95e-13.
- The near tangents: the curve is set at a distance `delta` from a surface point along
  the tangent plane, with `delta` in 0, ±1e-9, ±tol/2 and ±2 tol, for lines, circles and
  ellipses curving both ways.
  - A run of 30 seeds: 10,080 pairs, 7,200 touches classified Tangent, 1,624 misses with
    no hit. The worst crossing is 4.3e-11, on a cylinder at the 1e-3 tolerance.
  - The committed test runs 4 seeds.
- Further tests: curves lying on the surface (Overlap), the cone's far nappe (no hit),
  `se` dropping and wrapping, the sort, and the NURBS, budget and stop errors.
- The difftest `t_curve_surface`: 716 inputs, 279 exact, 436 within tolerance, 1 pin
  defect and 0 mismatches.

**Speed.** Measured with both sides prebuilt and the intersect call alone. scx ran at
opt-level 2 (the pin's dev profile) over the 737 pool cases, taking the best of 7 runs:

| Surface | scx | pin | ratio |
| --- | --- | --- | --- |
| plane | 1.14 µs | 22.4 µs | 20x |
| cylinder | 2.41 µs | 443 µs | 184x |
| cone | 2.51 µs | 813 µs | 323x |
| sphere | 2.27 µs | 2.56 ms | 1,129x |
| torus | 4.37 µs | 4.06 ms | 930x |
| all | 2.55 µs | 1.59 ms | 622x |

The pin solves its NURBS images with a general subdivision solver. Research 66 §5 has
nothing for this beyond 65's rows. Its candidates either do not certify isolation (Newton
from samples) or are the pin's own subdivision. The certified Bernstein isolation used
here is 65 row 1.

**Added work.** All the code is new. No existing path gains work. `Arcs::of_direction`
became `pub` because `Arcs` moved to `scx-curve`. `Axes::local` is the same as SU4's, and
`Cone::slope` is main's.

### 4.14 pcurve and SU6 follow-ups as built (11-27)

Three items were left open by §4.11, §4.13 and §5.6: WR-680's oblique cylinder section in
`pcurve_of`, the overlap left unclipped by `se`, and a bad `tol` in `intersect_curve`.

**The oblique section.** A plane that cuts a cylinder of radius `r` off square gives an
ellipse `O + a cos t X + s_e b sin t Y`. With `O` on the axis, the parts of `a X` and
`s_e b Y` across the axis are perpendicular and both `r` long, so:

- the azimuth is exactly `alpha + s t`: `alpha` is the azimuth of `a X`, and `s` is the
  sign of the turn from it to `s_e b Y`;
- the height is `v = h + (a X . z) cos t + (s_e b Y . z) sin t`, with `h` the height of
  `O`.

In D13's uv, `u` is the arcs parameter of that azimuth, the same coordinate as a
parallel's. So the exact pcurve, at the ellipse's own parameter `t`, is a new kind:

```rust
pub struct AngleWave2 { arcs, start, sense, mean, amp_cos, amp_sin }  // scx-curve
// u = arcs.parameter(start + s t), v = mean + amp_cos cos t + amp_sin sin t
pub enum Curve2 { .., AngleLine(AngleLine2), AngleWave(AngleWave2) }
AngleWave2::new(arcs, start, sense, mean, amp_cos, amp_sin) -> Result<Self, CurveError>
```

It has the same methods as `AngleLine2`: `point`, `jet`, `point_over`, `jet_over`,
`domain`, `angle`, `reversed`, and `restrict` through the enum. The `u` half shares
`AngleLine2`'s code (`angle_domain`, `angle_jet`), and `AngleLine2` stays bit for bit the
same (tested). `v`'s derivatives rotate `(cos, sin)` a quarter turn each order.
`reversed` flips the sense and negates `amp_sin`, bit for bit `C(-t)`. Inverting one is
`Unsupported::AngleWaveInversion`, as an angle line's inversion was.

`pcurve_of` builds it when the centre is within `NEAR * r` of the axis. Everything else
goes through the same on-surface and lifting checks as every other answer. A section of a
cone is still `NoClosedForm`.

**Decision: a new kind, by analysis and adversarial analysis.**

- *Option 1, a `Curve2::Nurbs` (the pin's answer).* The exact rational uv curve exists
  (testkit T5's `cylinder_section`, the pin's `cylinder_section.rs`). But its parameter is
  the section NURB's, not the ellipse's angle, and the 04 ruling is that the pcurve keeps
  the edge's own parameter. Matching the parameter would need a reparameterisation, which
  is a fit or a composition. It is rejected for that.
- *Option 2, widen `AngleLine2` with a sinusoid.* That would give one kind for both. But
  every parallel and meridian would then carry three more words and a `cos`/`sin` it
  never needs, and the name would be wrong. It is rejected.
- *Option 3, `AngleWave2` (chosen).* It is exact at the ellipse's parameter, it is plain
  data like the other kinds, and its `u` code is shared with `AngleLine2`.
- *The adversarial case against 3.*
  - "Another enum arm everyone must match." `Curve2` is `#[non_exhaustive]`, so a new arm
    costs one line per dispatch, and the dispatch test covers it bit for bit.
  - "A STEP writer wants a B-spline." It can convert at write time, exactly (T5's form
    over the same span). That conversion is a cost at export, not on the kernel's paths.
  - "A square cut now has two answers." A square cut given as a `Circle` is a parallel
    (`AngleLine2`). The same cut given as an `Ellipse` with equal semi-axes gets an
    `AngleWave2` with zero amplitudes. That is exact, only less specific.
  - None of these overturns 3.

**Truth check.**

- `cylinder_sections_are_angle_waves` (unit, 3,840 cases): scales 1, 1e3 and 1e-3, both
  senses of cylinder and ellipse, tilts from 1e-9 to 1.2, either axis first and the
  ellipse's z either way, five intervals (negative, short, past a turn). Each case is
  lifted at 101 samples to 1e-12 of size.
- `oblique_cylinder_sections_have_the_rational_uv` (`tests/truth.rs`): 24 sections, at 8
  points on each of T5's exact rational spans, give the same uv as T5's NURBS uv, scaled
  by `2 pi r / 4 pi`. The ellipse's parameter of each point is checked to 1e-12.
- Six `AngleWave2` tests in scx-curve:
  - against its definition;
  - finite differences;
  - the side at a break;
  - reversal bit for bit;
  - interval enclosure;
  - `NotFinite`.
- Difftest `t_pcurve` (new): 240 arcs of oblique sections against the pin's
  `drop_curve_to_srf`, compared at 17 stations of the ellipse's angle. On the pin side,
  bisection finds the NURB parameter whose point has that angle.
  - The result: 212 within tolerance, 28 pin defects, 0 mismatches.
  - Both uv are the same (the pin's bounded cylinder's `u` runs `[0, 2 pi r]`).
  - The defect: the pin's drop runs at a 3-D tolerance of 10. Its `DropIsoCurve` takes a
    gently tilted section for the iso line at its start height and answers a constant
    `v`. scx's stations there are held to the exact uv: `v` the height, `u` T4's arcs
    parameter of the azimuth in double-double, within 1e-9 of size
    (`pin_defects::oblique_pcurve`).
  - The pin also returns its uv curve twice, or split where scx keeps one piece. The
    fingerprint takes the first piece that holds each station.

**The overlap clipped by `se`.** An `Overlap` is now the pieces of `ce` whose uv lies in
`se`.

- *Decision: one overlap can become several pieces.* A circle on a plane crossing a
  rectangle leaves and re-enters it up to four times. A single extent would have to cover
  the outside runs or drop the inside ones. Both are wrong. The variant stays
  `Overlap { t: Extent1d }`, and one hit is returned per piece, in `t` order.
- *How.* The cuts are where the curve meets the planes, in the surface's axes, that
  carry `se`'s sides:
  - the plane's sides `x = u` and `y = v`;
  - a cylinder's or cone's `z = v`;
  - a sphere's `z = r sin(angle(v))`;
  - a torus's `z = small sin(angle(v))`, skipped when the `v` range is a turn or more;
  - a side of constant `u` on a revolution kind: the plane through the axis at that
    azimuth, skipped when the `u` range is a period or more.

  These planes hold every boundary crossing (a superset: a plane through the axis also
  meets the opposite half). A line's root is linear, and a conic's are `psi ± acos(-gamma
  / R)`. Between adjacent cuts, the middle's uv is tested with the same `uv_in` as a
  point hit, and adjacent kept pieces merge. No transcendental search, no tolerance.
- *Edges.*
  - When nothing leaves `se`, the original `ce` is returned unchanged.
  - On a closed conic, a piece across the turn's start is joined into one extent that
    runs past `ce.min() + 2 pi`. The `Overlap` doc says so.
  - A touch of no length (the curve grazes a side from inside) is dropped. It is a
    point of the curve on the surface, but not a stretch, and a point hit would claim a
    crossing that is not there.
- *Truth check.*
  - `overlaps_are_clipped_by_se`: 30 rounds of the 11 on-surface cases (every kind),
    each with a random `se` (its `u` sometimes a period or more), 200 parameters each. The
    oracle is the surface's own inversion placed against `se`: a parameter is in a piece
    exactly when its foot is inside, except within 1e-6 of a piece's end. The test asserts
    more than 10 cases in several pieces, more than 20 with none, more than 5 whole, and
    more than 50,000 parameters checked.
  - `a_circle_in_a_box_is_its_arcs`: exact arc ends.
  - `rulings_and_parallels_clip_exactly`.
  - `curves_on_the_surface_overlap_it` now has an oblique ellipse on a cylinder and
    asserts 220 overlaps.
  - `t_curve_surface` is unchanged at 0 mismatches. Its fingerprint is only whether
    there is an overlap.

**A bad `tol`.** `intersect_curve` now refuses a `tol` that is not finite or not above
zero as `SurfaceError::BadTolerance { value }` (Refused, `intersect_tolerance_invalid`).
It is checked first, before the NURBS check, so a NaN never reaches the solve
(`a_bad_tolerance_is_refused_first`). `pcurve_of` still accepts a zero `tol` (its checks
then ask for an exact lift). The two now differ on zero. That is left for review: making
`pcurve_of` refuse zero too is a one-line change if it is ruled.

**Added work.** Timed side-channel with a throwaway timer (best of 7, release, not
committed). `main` and the branch were run back to back under the same load:

| Path | main | branch |
| --- | ---: | ---: |
| `pcurve_of` of a parallel | 1365 ns | 1370 ns |
| `Curve2` angle-line `point` | 38.9 ns | 37.4 ns |
| `pcurve_of` of an oblique ellipse | 507 ns (refused) | 1573 ns (built and checked) |
| `intersect_curve` line-cylinder | 1880 ns | 1799 ns |
| overlap, parallel, `se` the whole turn | 6501 ns | 6531 ns |
| overlap, parallel, `se` part of the turn | 6497 ns | 7153 ns |
| overlap, circle on a plane in a box | 2500 ns | 3191 ns |

- Paths that do not clip are unchanged within noise. The load was 6 to 10, and earlier
  runs swung by 25% on both sides.
- An oblique `pcurve_of` now does the work every other answer does: 9 + 9 evaluations for
  the checks, one `atan2`.
- A clipped overlap adds, per side of `se`: one plane, a `hypot`, an `atan2` and an
  `acos`. Then one curve point and one `uv_in` per sub-interval (at most 9 here).
- A plain hit gains one comparison for `tol`.

## 5. `scx-ssi`, the analytic part

The pin's sources are `intersect/surface_intersector` (`p1`-`p5`) and `adv_surface_intersector`.
Step 2 needs only the analytic pairs. The marching and the B-spline patch intersector are step 5.

### 5.1 Shapes found

| Shape | Where (READ) | Count (MEASURED) |
|---|---|---:|
| Intersector object with setters before the call | `intersect/surface_intersector/p1.rs:628` `SurfaceIntersector<'s>`, `:839` `new`, `:961` `set_extension_distance`, `:966` `set_use_surface_edges(bool, bool)` | `fn set_` 3 |
| Point arena with typed ids, and mutable accessors by surface index | `p1.rs:274` `TsectPnt`, `:601` `TsectId`, `:439`-`:540` `uv_pos_mut(SrfIndex)` and so on | `&mut self` 38 |
| `assign(&mut self, &Self)` as copy-assignment | `p1.rs:349` | — |
| `Result<()>` steps that mutate the intersector | `p1.rs:1621` `add_point_to_curve`, `:1634` `add_start_point(k, id)`, `:1924` `reverse_curve_direction` | 73 |
| Kernel for tolerances | `p1.rs:1634` and on | `&Kernel` 229 |
| User surface wrapped in `Arc` | `p1.rs:306` `UserSurface(pub Arc<Surface>)` | — |
| Analytic pairs dispatched inside surface kinds, with a bool for "general intersector still needed" | `surface/sphere.rs:1178`, `surface/cone/p3.rs:401`, `surface/torus.rs:471`, `surface/plane/p2.rs:781` | `needs_more` 2 here, 83 in surface |

### 5.2 Proposed shapes

- **The analytic part is free functions over pairs, not methods on the intersector.**
  - `analytic::intersect(a: &Surface, ae: &Extent2d, b: &Surface, be: &Extent2d, tol: Length) ->
    Closed<Intersection>`.
  - It matches on the unordered pair, normalised so `(a, b)` and `(b, a)` share one routine, and
    swaps the pcurve sides back.
  - `General` hands over to the marcher in step 5. In step 2, `General` is a valid result that the
    caller handles: the difftest counts the pairs.
- **The pairs in step 2** (JUDGED from the pin's overrides):
  - plane-plane (a line, coincident or nothing);
  - plane-quadric (conics: the plane section, SS5, which lives here per 01 §1.1);
  - sphere-sphere (a circle);
  - coaxial quadric pairs (circles);
  - sphere against a cylinder, cone or torus on its axis (circles).
  - Anything else is `General`.
- **The plane section of a surface** (01 §1.1) [Ruled 11-27, M3: moved here from `scx-surface`'s
  SU5]. `section_by_plane(s, se, plane, pe, tol) -> Closed<Section>`.
  - `Section` holds the 3-D curves with their two pcurves as `Option<Curve2>` (`None` where no
    closed form exists on that side), plus warnings.
  - Under D13 the pcurves of plane sections on cylinders, and where the pin has them on cones,
    spheres and tori, are exact rational `Curve2::Nurbs` in the kind's uv (WR-680). So SS5 needs
    `scx-nurbs` (CU3).
    [11-27, §4.11: true of the oblique sections only. A section perpendicular to the axis is
    a parallel, whose exact pcurve at the edge's parameter is a `Curve2::AngleLine`
    (`pcurve_of`).]
  - The plane's face accuracy, which the pin took from `PlaneOwner`, is a `Length` argument.
  - The unused `use_surface_edges` and `ang_tol_rad` are dropped.
  - It is a closed form, so it is bounded and infallible (X13).
- **Coincidence is a result variant**, `Intersection::Coincident { region }`. It is not an empty
  curve list with a flag.
  - Its tolerance is `max(resolution, acc1, acc2)` (04, 11-27 coincidence row). The caller passes
    both accuracies, and the function does not look them up.
- **The marching intersector, when it comes in step 5,** becomes `options + one call` (04 closure
  row): `MarchOptions { extension: Option<Length>, edges: [EdgeUse; 2] }` passed to `march(..)`.
  - The point arena stays as a local `Vec<TsectPnt>` with `TsectId` indices inside the call. That
    is idiomatic, so it is kept.
  - The `_mut(SrfIndex)` accessors become `[SideState; 2]` indexed by a two-variant `Which { A, B }`
    with `Index`, the 57 M8 pattern.
  - Listed now so the step-2 types do not block it.

### 5.3 Sketch

```rust
pub mod analytic {
    pub enum Intersection {
        Curves(Vec<IntersectionCurve>),   // sorted: by kind, then by a point on each, then by direction
        Coincident { on_a: Extent2d, on_b: Extent2d },
        Touch(Vec<TouchPoint>),           // isolated tangent points
        Empty,
    }
    pub struct IntersectionCurve { pub curve: Curve, pub on_a: Option<Curve2>, pub on_b: Option<Curve2>, pub contact: Contact }
    pub fn intersect(a: &Surface, ae: &Extent2d, b: &Surface, be: &Extent2d, acc: [Length; 2], res: Resolution) -> Closed<Intersection>;
    pub struct Section { pub curves: Vec<SectionCurve>, pub warnings: Vec<SectionWarning> }
    pub struct SectionCurve { pub curve: Curve, pub on_surface: Option<Curve2>, pub on_plane: Option<Curve2> }
    pub fn section_by_plane(s: &Surface, se: &Extent2d, plane: &Plane, pe: &Extent2d, tol: Length) -> Closed<Section>;
}
```

### 5.4 Truth references missing

None of these are in the testkit (59). All are closed forms, so they are cheap.

- plane-plane: an exact line in rationals;
- plane against sphere, cylinder, cone and torus:
  - the conic's centre, axes and radii in closed form;
  - the exact rational uv pcurve on the cylinder (WR-680) in the D13 uv, lifted exactly;
  - exact in rationals where the data allow, double-double otherwise;
  - the cone's conic type by the sign of an exact predicate: ellipse, parabola or hyperbola;
- sphere-sphere: the circle's plane and radius, exact in rationals when the squared radii are
  rational;
- coaxial pairs: the circle heights, from a quadratic in the axial coordinate;
- **near-degenerate generators**: tangent planes to spheres and cylinders, parallel and coincident
  planes, nearly coaxial pairs (axes off by 1e-12 rad). The truth decides `General` against
  `Solved`. The pin is not the judge.
- **a pcurve agreement oracle**: each returned pcurve lifted onto its surface lies within tol of
  the 3-D curve. This is shared with `scx-brep` (T6), and it is a test oracle only: no library code
  calls it (D10).

### 5.5 Builder tasks

| Task | Work | Depends on |
|---|---|---|
| SS1 | `analytic::Intersection` types, pair normalisation, `Closed` reuse | SU2 |
| SS5 | the plane section of the analytic kinds (was SU5) | SS1, SU1, CU1, CU3; T5 |
| SS2 | plane-plane, plane-quadric (on SS5) | SS1, SS5; T5 |
| SS3 | sphere-sphere, coaxial pairs | SS1, SU1; T5 |
| SS4 | coincidence and tangency classification at the tolerance | SS2, SS3 |

### 5.6 SS1/SS5 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

```rust
// scx-ssi::analytic
#[non_exhaustive] pub enum Intersection {
    Pieces { curves: Vec<IntersectionCurve>, points: Vec<IntersectionPoint> },  // at least one
    Coincident { on_a: Extent2d, on_b: Extent2d },   // 11-27 §5.8: Coincident(Vec<Overlap>)
    Empty,                                                                       // final
}
pub struct IntersectionCurve { pub curve: Curve, pub span: Extent1d, pub contact: Contact }
pub struct IntersectionPoint { pub point: Point3, pub contact: Contact }
pub enum Contact { Transversal, Tangent }
pub fn intersect(a: &Surface, ae: &Extent2d, b: &Surface, be: &Extent2d,
                 acc: [Accuracy; 2], res: Resolution) -> Closed<Intersection>;
pub fn section_by_plane(s: &Surface, se: &Extent2d, plane: &Plane, pe: &Extent2d,
                        tol: Length) -> Closed<Intersection>;
// scx-surface: Closed<T> { Solved(T), General } moved here from its first user, shared
```

| Surface | Solved | `General` |
|---|---|---|
| plane | a line, coincident, or empty | never |
| cylinder | an ellipse, a circle, two lines or a tangent line, or empty | an ellipse so long that `2^-50` of its semi-axis passes the tolerance |
| cone | an ellipse, a circle, a parabola, the branch of a hyperbola on the nappe, two generators or a tangent generator, the apex, or empty (§3.12, 11-27) | an ellipse or hyperbola that long and not within the tolerance of a parabola |
| sphere | a circle, a touch point, or empty | never |
| torus | one or two circles across the axis, two meridian circles along it, or empty | every other plane (a spiric section) |

**Deviations from §5.2 and §5.3, with the case against and the case for each.**

- **One answer type, `Closed<Intersection>`, for both entry points.** §5.3 has a separate
  `Section` with warnings.
  - Against: a plane section has the plane's side to name, and 5.3 kept them apart.
  - For: a section *is* an intersection with a plane. One type means `intersect` hands
    `section_by_plane`'s answer through unchanged, and the caller matches one enum. The pin's
    warnings say that the answer is partial or near a limit. Here those cases are
    `General`, so no warning is left to carry.
- **No pcurve fields.** §5.3 has `on_a`/`on_b: Option<Curve2>`.
  - Against: callers building edges need them, and 5.2 said SS5 needs `scx-nurbs`.
  - For: D10 and §4.11 (ruled 11-27) put exact pcurves in `scx_surface::pcurve_of`, at the
    curve's own parameter. A piece carries `curve` and `span`, which is exactly what
    `pcurve_of(&piece.curve, surface, piece.span, tol)` takes. A unit test calls it on both sides of
    a square cylinder cut. Storing `Option<Curve2>` here would duplicate that function and
    compute pcurves for callers that never read them. An oblique cylinder section's pcurve is
    `NoClosedForm` in `pcurve_of` today (WR-680's rational form is not built), and that gap
    belongs to `pcurve_of`, not to this crate.
- **`Pieces { curves, points }`, not `Curves` and `Touch`.**
  - Against: 5.3 kept touch points as their own variant.
  - For: one section can have both. A cylinder cut can give a line and a separate point where
    the other line leaves the extent within the tolerance. Two variants would force one of
    them to be lost or a third variant. `Empty` stays its own variant, never an empty `Pieces`.
- **Curves in construction order, not sorted.**
  - Against: a sort makes `(a, b)` and `(b, a)` compare equal piece for piece.
  - For: the construction is deterministic (the same input gives the same bits), and
    reversing the order reverses the direction (`n_a x n_b`), so the pieces would not match
    one-to-one anyway. A sort key over floating point needs a tie rule, and no caller needs one.
    It can be added when a caller needs it.
- **`acc: [Accuracy; 2]` and `res: Resolution` on `intersect`; `section_by_plane` takes one
  `Length`.** This follows §5.2's coincidence row (`max(res, acc1, acc2)`, via
  `scx_num::units::coincident`). `section_by_plane` keeps the single plane-accuracy length that
  5.2 names.
- **`use_surface_edges` and `ang_tol` dropped** (§5.2 said so). Neither changes any pin answer
  on these kinds.
- **The cone is worked from its apex.** The cone is `rho = k z'` and the plane is
  `n . p' = delta`, so every size is `delta` times a factor of the plane's lean.
  - Against: the pin works from the frame origin.
  - For: there is no cancellation. A plane through the apex gives the apex or the generators,
    not a rounding-sized ellipse.
- **Empty from exact bounds, never from a search (04, research 68).** Two bounds come in
  beyond the pin's. Each is a support function, so it is exact and final:
  - **The cylinder or cone between its two rims.** Each generator is a segment between the rims,
    so the part lies in their convex hull. A rim at height `v` and radius `rho` spans
    `nz v - d +- |rho| s` from the plane. Both rims off one side by more than `tol` means
    `Empty`.
  - **The torus.** If the plane is farther from the centre than `s * major + minor` (the core
    circle's support plus the tube's ball), the answer is `Empty`.
  - Against: the pin answers `General` there, and a bound that is too tight would lose a curve.
  - For: both are exact supports. The difftest checks every such case on a 65x65 grid of the
    surface (all points off the plane by more than the snap), and `plane_sections` checks them
    against the truth's points. Without them, a hyperbola or spiric case that misses the extent
    would go to the marcher for nothing.
- **No seam splitting.** A conic arc that crosses the turn's start (`u = 0`) is one piece
  whose span passes `2 pi`.
  - Against: the pin cuts it in two, and a face loop built from the pieces may want the seam
    as a vertex.
  - For: the intersection is one connected curve. A seam is a property of a face's uv, and
    whoever builds edges splits there if the face needs it, via `pcurve_of`'s period window
    (07 §3 rule 5). The difftest's truth check rejects two pieces that meet end to end.
- **Primitives keep their known topology (04).** A near-square cylinder cut is a circle when
  the true ellipse lies within the tolerance of it along the surface. A near-axial plane gives
  lines when they are within the tolerance over the extent's height. Two lines or circles
  within the tolerance of each other are one tangent curve.
- **The difftest compares on forward sense only.** Sense changes only the direction
  (`plane_sections` covers both senses against the truth). The pin's reversed-sense frames
  would add a mapping to the harness and no new geometry.

**Validation.** `plane_sections` (6 tests) checks every answer against the exact sections
(testkit T5). Pieces must lie on both surfaces and inside both extents, with the truth's
kinds, and every truth point inside the extents must be covered. Crossing curves must run
along `n_s x n_plane`. It covers both senses: fixed cases per kind plus 400 random sections.
There are 5 unit tests: order and direction, coincident extents, other pairs `General`,
the span merge across the turn's start, and the `pcurve_of` seam.

The difftest `t_ssi::ssi_section_by_plane` compares against the pin's `intersect_with_plane`.
It uses 1,000 inputs aimed at the cases above. Results: 378 exact, 532 within tolerance,
90 pin defects, 0 mismatches, 0 nondeterministic. With the open conics (§3.12, 11-27): 323
exact, 532 within tolerance, 145 pin defects, 0 mismatches, 0 nondeterministic; the 55 new
defects are the last four rows below.

Each pin defect is judged against T5 by `t_ssi::truth::check`, in these classes (pool counts):

| Class | Count | Pin answer | scx answer |
|---|---:|---|---|
| Coincident planes | 23 | general | coincident |
| Cone cut through the apex, steeper than the generators | 11 | general | the two generator lines |
| Cone apex outside the extent | 15 | a tangent point off the extent | empty |
| Cylinder seam | 9 | splits the arc at `u = 0` | one piece |
| Cone seam | 10 | splits the arc at `u = 0` | one piece |
| Bounded cylinder or cone missed | 8 | general | empty |
| Torus missed | 15 | general | empty |
| Cone hyperbola | 19 + 12 | general | the branch on the nappe, in one piece or two |
| Cone parabola | 13 + 5 | general | the parabola, in one piece or two |
| Cone cut along a generator, off the apex | 4 | general | empty (the parabola on the other nappe or outside the extent) |
| Cone hyperbola clipped to nothing | 2 | general | empty |

**Timings.** These were measured side-channel with a throwaway timer, not committed. The
timer ran over the difftest pool on prebuilt surfaces, 200 repetitions, per call. Each
figure is scx against the pin:

| Kind | scx | pin |
|---|---:|---:|
| plane | 0.55 us | 1.71 us |
| cylinder | 1.81 us | 7.72 us |
| cone | 0.68 us | 2.57 us |
| sphere | 0.63 us | 5.47 us |
| torus | 0.69 us | 32.6 us |

The torus gap is mostly the pin's general fallback on the spiric cases. scx gives up on
those cases at once and the caller pays later; the comparison is on what each call does.

With the open conics (§3.12, 11-27), on the pool's cone cases, release: a hyperbola section
1.70 us (31 cases) and a parabola 0.94 us (17) against the pin's 0.70 and 0.65 us, which
answer `general` and leave the curve to the marcher; the other cone cases 0.89 us against
3.17 us. The hyperbola's cost over the ellipse's is its clip (a quadratic in `e^t`, a `ln`
per root) and `sinh_cosh` at the span's ends.

**Other kernels (research 66).** §8.2 records that OCCT's IntAna does analytic quadric pairs.
Its docs only were read, and no code. The techniques adopted here are ours:

- exact support and hull rejects before any conic is built;
- one linear side per extent edge, so clipping is the roots of `a cos t + b sin t + c`.

Both are exact and deterministic. Nothing was taken that depends on a search or a cache.

**Open.**

- WR-680's oblique cylinder section in `pcurve_of`: done, an `AngleWave2` at the
  ellipse's parameter (§4.14). An oblique section of a cone is still `NoClosedForm`.
- ~~Parabola and hyperbola kinds in `scx-curve`.~~ Built 11-27 (§3.12); a cone's
  parabola and hyperbola are solved.
- The cylinder's ellipse too long for its precision, and the cone's band between the
  parabola threshold and that bound: `General` (§3.12). Looked at in SS4 and not closed
  (§5.8 (c)).
- SS2's remaining gaps are listed in §3.12. ~~SS4~~ built 11-27 (§5.8): `Coincident`
  carries the overlap, and one depth rule decides tangency.

### 5.7 SS3 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

No public signature changes. `intersect` now answers two spheres, and any two of cylinder,
cone, sphere and torus on one axis, in `analytic/coaxial.rs`. The answer is §5.6's
`Closed<Intersection>`. Every other pair stays `General` (SS2). Inside the crate, `Clip` now
takes two `Part`s (a surface's region and its normal) instead of a surface and a plane, so
the plane sections and the coaxial circles share one clipper and one orientation rule
(`n_a x n_b`).

**The method.** Each surface of revolution is reduced to its meridian in the half-plane
(distance from the axis, height along it):

- a cylinder or cone is a line;
- a sphere or torus is a circle;
- each carries the heights its extent allows.

Two meridians meet at points, and each point is a circle about the axis, or a touch point
when it lies on the axis. The circles are then clipped to both extents by `Clip`.

The axis is chosen as follows:

- Two spheres share the line of their centres.
- A sphere shares the other surface's axis when its centre lies on it.
- Otherwise the axes must agree, and their tilt and offset, carried over the second's
  reach, become `δ`. When `δ` is past the tolerance, the answer is `General`.

| Meridians | Solved | `General` |
|---|---|---|
| two lines (cylinder, cone) | one circle; coincident over the shared heights; empty | a gap between `tol` and `tol + δ`; a crossing past both apexes that the apex distance cannot settle |
| line and circle | one or two circles, a tangent circle, or empty | the same |
| two circles | one or two circles, a tangent circle, coincident, or empty | the same |

**Choices, with the case against and the case for each.**

- **One meridian solver for every coaxial pair, not one function per pair.** The pin has
  `intersect_with_sphere` and `intersect_with_cone` per kind.
  - Against: a function per pair is easier to compare with the pin's line by line.
  - For: the geometry is the same in every case, either lines and circles meeting in a
    plane, so ten pairs need three meeting rules (`lines`, `line_circle`, `circles`).
    The torus pairs come with no extra code. The pin solves the torus pairs only through
    its general intersector.
- **Two circles by the pin's WR-79 Heron form.** The half-chord is computed as
  `sqrt` of the product of the four gaps, not by `r^2 - d^2`.
  - Against: none found. It is the pin's lesson, and it is exact up to rounding.
  - For: near a tangency the difference of squares cancels, while the product of gaps does
    not.
- **Empty only from exact bounds.** Three bounds give Empty:
  - the heights apart;
  - parallel meridians apart;
  - a crossing past both apexes, where each apex is farther than `tol + δ` from the other
    meridian.

  Each is a distance in the meridian plane that the surfaces cannot close. Between `tol`
  and `tol + δ` the answer is `General`, not a guess.
  - Against: a pair that is slightly off-axis and far apart could have been called empty by
    a looser bound.
  - For: §5.6 and 04 say a solved empty is final. `General` costs the marcher a call, and a
    false empty loses a curve.
- **Coincident when the meridians agree within `tol` over shared heights longer than `tol`.**
  Two cones that share only their apex are not coincident: they meet at that apex only. Two
  spheres with the same centre and radius are coincident, and so are two tori with the same
  tube.
  - Against: none found for these surfaces. A turned coaxial frame matters only for the
    `around` range, and the clip handles that range.
  - For: the truth (T5) agrees on every case the tests build.
- **Concentric spheres are answered without looking at their extents.** They are
  coincident when the radii agree within `tol`, and empty otherwise.
  - Against: two sphere patches on opposite sides of one sphere do not overlap, and the
    answer still says `Coincident`.
  - For: `Coincident { on_a, on_b }` carries both whole extents, as for two planes in
    §5.6. Bounding the overlap of two latitude and longitude patches is SS4's
    classification work. A caller that needs the overlap intersects `on_a` with `on_b`.
    This is a deviation, listed for review.
    [11-27, §5.8: superseded. `Coincident` carries the overlap's pieces, and two spheres
    whose axes differ past the tolerance are `General`.]
- **A torus's heights are its whole tube's** (`[-small, small]`), whatever its extent's
  tube angle. The clip then cuts the circles to the tube angle.
  - Against: a tighter height range would reject more pairs as empty before the clip runs.
  - For: the bound stays exact. The clip is exact as well, so no curve is lost, and a
    rejected pair would only have been cheaper.
- **Touch points on the axis.** Where the meridians meet at distance 0 (`|x| <= tol`), the
  answer is a tangent point, not a circle of radius 0. Examples are a sphere touching a cone's
  apex, and two spheres touching end to end.
- **No seam splitting.** This is §5.6's rule: a circle is one piece over its span, even when
  the span passes the turn's start.

**Tests and the truth extension.**

- **T5 extension.** T5's `shared_axes` now accepts frames whose x and y axes are a signed
  permutation of each other's, in `scx-testkit/src/ssi.rs`. Before, it needed equal axes.
  This lets the truth answer pairs that are turned or mirrored about the shared axis. It is
  tested in `scx-testkit/tests/ssi.rs`.
- **Shared test helpers.** The helpers of `plane_sections` moved to `tests/common`, so
  `tests/coaxial.rs` can use them too.
- **`tests/coaxial.rs`** has 6 tests that check every answer against T5, in both orders,
  with the same checks as `plane_sections`:
  - on both surfaces;
  - inside both extents;
  - the truth's circle and contact;
  - direction along `n_a x n_b`;
  - every owed truth point covered.

  The cases are:
  - fixed cases for each pair kind, including turned and mirrored frames;
  - near-axis pairs: a tilt or offset of `1e-12` is solved; `1e-3` and square axes are
    `General`;
  - 800 random pairs.
- **Unit test change.** The unit test `other_pairs_are_general` now uses a sphere off the
  cylinder's axis.

**Difftest.** `t_ssi_coaxial::ssi_coaxial` compares scx against the pin's
`intersect_with_sphere` and `intersect_with_cone` on 900 inputs: sphere, cylinder and cone
pairs on exact signed-axis frames. The pin has no closed-form torus pairs. Results:

- 501 exact;
- 129 within tolerance;
- 270 pin defects;
- 0 mismatches;
- 0 nondeterministic.

Each pin defect is judged against T5 by `t_ssi_coaxial::truth::check`:

| Class | Count | Pin answer | scx answer |
|---|---:|---|---|
| Cones crossing | 41 | general | one circle |
| Cones apart | 58 | general | empty |
| Cones coincident | 6 | general | coincident |
| Cylinder and cone crossing | 64 | general | one circle |
| Cylinder and cone apart | 28 | general | empty |
| Cylinders of one radius | 46 | coincident or tangent arcs and lines along the overlap | coincident |
| Sphere and cylinder of its radius | 27 | the equator, transversal | the equator, tangent (as T5) |

**Timings.** Side-channel and throwaway, as in §5.6: the difftest pool on prebuilt surfaces,
200 repetitions, release build, per call. Each figure is scx against the pin:

| Pair | scx | pin |
|---|---:|---:|
| sphere-sphere | 0.92 us | 1.44 us |
| sphere-cylinder | 0.72 us | 1.70 us |
| sphere-cone | 0.76 us | 1.37 us |
| cylinder-cylinder | 0.28 us | 115 us |
| cylinder-cone | 0.70 us | 0.37 us |
| cone-cone | 0.68 us | 0.39 us |

The pin is faster on cone pairs only because it answers `General` at once and leaves the
work to the marcher. scx solves these pairs. The pin's cylinder pairs are slow because it
builds arcs and lines along the overlap.

**Other kernels (research 66).** §8.2 has only OCCT's docs on IntAna, and research 66 lists
nothing for coaxial pairs. Nothing was adopted from it. The meridian reduction is the
textbook one for surfaces of revolution, and it is exact and deterministic.

**Open.**

- ~~SS4: the overlap of coincident patches, and tangency classification at the tolerance.~~
  Built 11-27 (§5.8). Concentric spheres now answer their overlap; tilted ones are
  `General`.
- SS2: non-coaxial quadric pairs.

### 5.8 SS4 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

```rust
// scx-ssi::analytic
pub struct Overlap { pub on_a: Vec<Point2>, pub on_b: Vec<Point2> }  // one piece, vertex for vertex
#[non_exhaustive] pub enum Intersection {
    Pieces { curves: Vec<IntersectionCurve>, points: Vec<IntersectionPoint> },
    Coincident(Vec<Overlap>),   // was Coincident { on_a: Extent2d, on_b: Extent2d }
    Empty,
}
```

`intersect` and `section_by_plane` keep their signatures. The new code is in
`analytic/overlap.rs`; the tangency rule is in `section.rs` and `coaxial.rs`.

**(a) The overlap of coincident patches.** `Coincident` now carries the pieces of the
overlap, each a convex polygon in both surfaces' D13 uv, with `on_a[i]` and `on_b[i]` one
point. Everywhere the closed forms find two surfaces coincident, the map from one uv to the
other is affine (§5.6 plane uv; the revolutions' turns), so the overlap is exact as
polygons:

- **Two planes.** The second's rectangle is carried into the first's uv (a rotation and a
  shift) and clipped by the first's rectangle (Sutherland and Hodgman): one convex polygon of
  up to eight vertices. The clip is exact first, with no slack. Only when that is empty is it
  taken again with the tolerance as slack, which gives a piece of no width where the patches
  touch within the tolerance. When that too is empty the planes are apart and the answer is
  `Empty`, as before.
- **Two surfaces of revolution on one axis.** Each coordinate maps alone: the angle about
  the axis turns and may reverse, a height shifts and may reverse, a latitude or tube angle
  may reverse. The overlap is a product of intervals, split where an arc wraps past either
  one's seam: up to two pieces per coordinate. An exact overlap gives pieces; a gap within
  the tolerance gives a piece of no width; a gap past `tol + δ` (in angle, `asin` of it over
  the least distance from the axis) is `Empty`; between those it is `General`, as §5.7.
- **Two spheres one within the tolerance** (concentric to within it, not only to the bit)
  share the latitude and longitude map when their axes agree within the tolerance over the
  radius. When the axes differ by more it is `General`.
- Every vertex is clamped into both extents.

Choices, with the case against and the case for each:

- **Polygons in uv, not the overlap's boundary curves in 3-D.**
  - Against: a face builder wants the boundary as edges, and the polygon has to be turned
    into them. A polygon also cannot carry a non-affine map, so a future pair with one (two
    tilted spheres, below) needs another form.
  - For: on every pair the closed forms call coincident the map is affine, so the polygon
    is exact, small (eight vertices at most) and checkable: the tests map every vertex both
    ways. The edges of the polygon are the patches' own uv lines, so the face builder already
    has their 3-D curves. A `Vec` of pieces, not one polygon, because a wrapping arc gives
    two.
- **Exact first, slack second.**
  - Against: two clips where one would do.
  - For: a clip with slack moves every vertex of a true overlap out by up to the tolerance,
    and clamping it back leaves vertices off the other patch's edge by about that (the
    first build failed a turned-octagon test by 7e-10 this way). The second clip runs only
    when the first is empty.
- **Tilted coincident spheres are `General`.** The map between their uv is a rotation of
  the sphere, and the overlap of two latitude and longitude patches is bounded by small
  circles of the other's, which are no rectangle of either's uv.
  - Against: §5.7 answered concentric spheres `Coincident` whatever their frames; this
    answers fewer cases.
  - For: §5.7's answer could not say where the overlap was (§5.7 recorded that as a
    deviation). An overlap the type cannot hold is `General`, not a wrong polygon. No
    difftest or T5 case lands here: the pin has no sphere overlap and T5 has no tilted pair.
  - This is a deviation, listed for review.

**(b) One tangency rule.** Before, each section had its own test of "tangent": a sphere's
cap radius, a cylinder's half-gap between lines, a cone's `sin * rho`, a torus's half-chord,
two meridians' half-chord. Those are widths across the crossings, which are of order
`sqrt(tol * size)` when the depth is `tol`, so a large surface kept two crossings far deeper
than the tolerance as a touch, and a small one split a tangency into two. Now one rule, as
SU6's (§4.13: one `Tangent` per run within the tolerance):

- **Two crossings whose surfaces part by no more than the tolerance between them are one
  tangent piece at the deepest point. A miss within the tolerance is the same touch.**
- The depth per case, each closed form:
  - a sphere's cap, `r - |d|`, a touch point at the cap's centre;
  - a cylinder's strip between two lines, `r - |d|`, a tangent line at its middle;
  - a torus's tube top or bottom, `small - |h|`, a tangent circle;
  - a cone's thin sector between two generators, `rho sin^2 / (1 + |cos|)` (the most the
    cone leaves the plane between them), a tangent generator at its middle;
  - a line meridian into a circle meridian, `r - dist`;
  - two circle meridians, the lens across their overlap or across the gap when one is
    nearly inside the other, a touch at the lens's middle.
- Against:
  - the pin's WR-79 kept the exact circle in the SSI and put the "cap centre within the
    tolerance" test in its face intersector. Keeping the exact curve defers the decision
    to where topology is known.
  - A tangent piece in place of two crossings moves the curve by up to the tolerance.
- For:
  - SU6's curve-surface intersection already answers one `Tangent` per run within the
    tolerance. Two layers deciding tangency differently would give an edge that SU6 calls a
    touch and SSI calls two crossings.
  - Depth is the quantity the tolerance bounds (every point within it of both surfaces);
    width is not.
  - The truth judges agree: in both difftests a truth crossing pair is matched by the
    tangent piece only when the run between them stays within the tolerance (below).

**(c) The near-threshold `General` bands.** Each was looked at for an exact classification.
None is closed; why:

- **The cylinder's too-long ellipse** (`semi * 2^-50 > tol`). The ellipse is a valid exact
  kind, but its form has an error of about its size times epsilon, which is past the
  tolerance by definition of the band. Its flanks are not near a parabola (they are the
  cylinder's straight generators), and past the two-lines threshold they are not lines. An
  exact answer needs a form whose error is relative to the local curvature, such as a
  rational quadratic arc (a conic as a weighted Bezier, held to the tolerance by its control
  points). The clipper has no rational `Shape`, so this is proposed as follow-up work, a
  `Shape::Rational` in `analytic/clip.rs`, not built here.
- **The cone's band between the parabola threshold and the precision bound.** The same
  reason. The parabola check `2 |e| reach^2 <= tol |g|` is conservative (it bounds the whole
  extent's height, not the arc inside it), so a tighter bound from the clipped arc would
  move part of the band to `Solved`. The arc is only known after clipping, which needs the
  kind. Not closed: it would need clipping twice.
- **Vertex-near cases** (a plane through a cone very near its apex, at an angle between the
  generators' and the axis's). The kind depends on the sign of `e`, whose rounding is of the
  order of `e` there; the classification is ill-conditioned in doubles, and an exact
  predicate would only move the band, since the curve itself is then shorter than its
  precision. Left `General`.

**(d) Other §5 and SS2 items.**

- The difftests check tangency and overlaps against T5, not only the pin (below).
- SS2's other gaps (§3.12) are not SS4's: the torus's spirics, pcurves off the plane, the
  difftest's single order. They stay open there.

**Tests and the truth extension.**

- Unit: `coincident_overlap_follows_order` replaces `coincident_extents_follow_order` (the
  extents it checked are no longer in the answer; it now checks that the pieces swap with the
  order). New: `coincident_planes_turned_give_an_octagon`, `coincident_planes_touching_and_apart`
  (a gap of 5e-10 is a touch of no width, 1e-6 is `Empty`).
- `tests/common`: an overlap judge (every vertex pair is one point; a 41 by 41 grid over the
  first extent, where a sample inside the second by a margin must be covered by a piece and a
  sample outside by the margin must not be strictly inside one), and a depth judge (each
  tangent piece within `tol` of both surfaces; a truth crossing absorbed by a tangent piece
  only when the run to it is shallow; any two transversal pieces, or opposite points of a
  whole conic, whose run stays within half the tolerance are flagged).
- `plane_sections`: `depth_rule`, across sphere, cylinder, torus and cone at depths
  `0.5 tol, 0, -0.5 tol, 4 tol, -4 tol`; turned plane polygons and edge to edge planes.
- `coaxial`: `depth_rule` (sphere and cylinder, two spheres, torus and cylinder at the same
  depths) and `overlaps` (wrapped cylinder arcs in two pieces, stacked heights touching,
  arcs end to end, a turned cone frame, tube arcs, sphere caps split by the second's seam, a
  tilted cap `General`).
- The difftests' T5 judges now check the overlap against the truth's patches, hold
  tangent pieces to the tolerance, and apply the depth judge.

**Difftests.** 0 mismatches and 0 nondeterministic:

| difftest | inputs | exact | within tolerance | pin defects |
|---|---:|---:|---:|---:|
| `ssi_section_by_plane` | 1,000 | 323 | 532 | 145 |
| `ssi_coaxial` | 900 | 501 | 129 | 270 |

The totals are the same as before SS4: the pin's answers are unchanged, and the stricter
T5 judges (overlap, depth) pass on every input. The pin's `Coincident` planes and cylinders (23 and 46 defects) are
now judged by their overlap's pieces, not by the whole extents.

**Added work.** Side-channel, a throwaway timer, not committed; release, per call, on the
pool's cases, before and after:

| case | before | after |
|---|---:|---:|
| plane and plane | 255 to 288 ns | 316 to 367 ns |
| coaxial, coincident | 288 to 477 ns | 658 to 690 ns |
| cylinder, cone, sphere, torus sections; coaxial crossing | — | unchanged within noise |

The plane cost is the clip and its `Vec` on the coincident half; the coaxial cost is the
rectangles and the D13 maps of their corners. Both are paid only where the answer is
`Coincident`, which before carried no overlap at all. The depth rule replaces one square
root per case by a subtraction, moving the root after the test.

**Other kernels (research 66).** Nothing applies: 66 lists no technique for coincident
overlaps or tangency depth. Sutherland and Hodgman's clip is the textbook one, exact for a
convex polygon against a rectangle and deterministic.

**Open.**

- `Shape::Rational` for the too-long conic bands ((c) above).
- Tilted coincident spheres: `General` ((a) above).
- The face intersector's use of the overlap (step 5's face work).

## 6. `scx-brep`

The pin's sources are `topology/` (without `api_files`, `part_file` and `database_io`), `heal/gap`,
`eval/pcurve`, `query/poly` and `geom/tools`. Step 3 builds the structure and its writers, not the
operations.

### 6.1 Shapes found

| Shape | Where (READ) | Count (MEASURED) |
|---|---|---:|
| The body as arenas inside the kernel, with use records at four levels | `topology.rs:761` `struct Brep`: arenas of shells, faces, loops, edges, vertices, regions, faceuses, loopuses, edgeuses and vertexuses, each `Stamped<..>` | `&mut Kernel` 418 |
| Mode flags on a long-lived body | `topology.rs:761`: `validate_only: Stamped<bool>`, `editing_enabled: Stamped<bool>` | — |
| Order lists kept beside the arenas | `face_list`, `edge_list`, `vertex_list`, `region_order`, `region_list_head` | — |
| Per-field write stamps as cache invalidation | `Stamped<..>` on every field; `topology/stamped.rs` | memo/cache 152 |
| Generic flags, marks and user slots on every entity | `topology/topology.rs:144` `struct Topology`; `:227` `set_all_flags(u64)`, `:250` `set_user_index1`, `:255` `set_user_index2`, `:260` `set_user_ptr1`, `:280` `mark(Option<&Context>, MarkType)` | marks 146, user slots 97, `&Context` 54 |
| Notification and change callbacks | `topology/topology.rs:764` `notify`, `:857` `NotifyEvent`, `:960` `TopologyChangeCallback`; `topology/edge.rs:2539`, `:2558` | identity/notify 1,919 |
| A direction as a bool | `topology.rs` `LoopEdge { edge, forward: bool }` | — |
| Tolerance 0.0 for "not set"; a tri-state as `i32` with `UNSURE` | `topology.rs:386` `Vertex::tolerance`, `:413` `Edge::tolerance`, `:458` `Face { rectangular_trim: i32, tolerance: f64 }` | sentinels 404, `SM_USE_OLDTOL` in `topology/vertex.rs` 5 |
| Geometry optional on a finished entity | `Edge { curve: Option<Curve> }`, `Face { surface: Option<Surface> }` | — |
| Interval as an extent that may be inside-out | `Edge { interval: Extent1d }` | `BIG_DOUBLE` 63 |
| Gap held as cached state | `topology/vertex.rs:1186` `VertexGap`, `:1206` `HeldGap`; `topology/edge.rs:1508` `EdgeGap`, `:1528` `MaxGap` | — |
| Options structs in some writers (already the right shape) | `topology/edge.rs:2879` `SetCurveOptions`; `topology/face/p1.rs:2090` `MoveSeamOptions` | — |
| Pcurves exact on the record path, fitted by the kernel otherwise [Ruled 11-27, D10: this row read "exact or refused, with nothing fitted"; review-63 H3] | `eval/pcurve.rs:316-320`: `exact_pcurves` returns `None` for "the uses whose pcurves the face's kernel makes ([`Trims`])", and the kernel fits them: `topology/edgeuse/p2.rs:652` `rebuild_uv_trim_curve`, `:696` `check_and_set_uv_trim_curve` (an 8-sample distance under 100× tolerance or a tenth of the edge length, else a raise or a re-drop more than twice as close); `docs/booleans.md` §7 (~924-940) | — |
| In-place knot snapping that returns a bool | `eval/pcurve.rs:1626` `snap_end_knots(&mut [f64], interval, rounding) -> bool` | — |
| Mirror that keeps ids | `topology/brep/mirror.rs` | — |
| Free functions over `(&Brep, FaceId) -> Result<Option<&Surface>>` | `topology/face/p2.rs:252` `surface`, `:285` `tolerance -> Result<f64>` | `Result<()>` 623, `Result<bool>` 202 |

### 6.2 Proposed shapes

Most of these shapes are ruled by 01 §2. The list says which ruling answers which shape, and adds
what 01 leaves open.

- **Arena in a kernel → `Brep`/`Draft` values** (01 §2.3).
  - No `&mut Kernel`, `validate_only` or `editing_enabled`. Editing is the `Draft` type, and
    validation is `finish`.
  - The order lists go: id order is the order (01 §2.2).
- **Four use levels → two sides per face and one `EdgeUse` per pass** (01 C4, ruled).
  - Faceuse, loopuse and vertexuse have no counterpart.
  - The radial order is stored on the edge's adjacency part.
- **`Stamped` fields → cells reset on write** (01 §3.2). `Draft::face_mut` is the only mutable
  accessor and empties the cells on every call.
- **`Topology` flags, marks and user slots: dropped.**
  - Marks are traversal state. They become a local `IdSet`/`IdMap` in the algorithm that marks.
  - User indices and pointers are application data in the app's `IdMap`s, carried by `carry`
    (01 §2.6).
  - The `Context` argument goes with them.
- **Notification → `Changes`.** Events are recorded by the draft as it edits (01 §2.3).
  - The C++ callbacks (`TopologyChangeCallback`) have no counterpart.
  - Named change records, per 57 §5.1: `Merge { survivor, absorbed }` and
    `Coincidence { operand, entity, onto }`.
- **`forward: bool` → `Sense`** (57 M3), on `EdgeUse`.
- **Tolerances.**
  - `tolerance: f64` with 0.0 for "unset", and `SM_USE_OLDTOL`, become `accuracy: Option<Accuracy>`,
    where `None` means within the stamped resolution (01 C4).
  - `rectangular_trim: i32` (`UNSURE`) is not stored. It is a face cell
    (`OnceLock<RectangularTrim>`) derived from the loops.
- **An edge's geometry is a curve or a collapse** (D8) [Ruled 11-27, D8 revised after review-63
  M1. This item first required a curve on every finished edge.]
  - 01 §2.1: an edge collapsed to a point has no curve, and a lone vertex (a cone apex, an
    imprinted point) is a loop of one use on a collapsed edge. A collapsed edge is valid.
  - So the geometry part is `enum EdgeShape { Curve { curve: Curve, interval: Extent1d },
    Collapsed }`. A face is never without a surface.
  - A `Draft` may hold an edge whose curve is not yet attached (A10), as a draft-only state. At
    `finish`, a non-collapsed edge with no curve is `FinishCause::MissingCurve { edge }`, kinded
    `Defect`: only our own operations build drafts that way, since the public builders and the
    record decoder check curve presence on the caller's input first and refuse there (62 D11).
  - The `EdgeUse` gains the parameter interval 01 §2.1 gives it. On a collapsed edge at a pole its
    pcurve is the pole's uv segment (Face 24); a lone vertex's use holds a uv point.
- **Intervals.** `Edge::interval: Extent1d` is always non-empty, because an empty extent is not an
  `Extent1d` (57 M3).
- **Gaps are recorded facts, not caches** (04 §4, research 20 and 21 withdrawn).
  - The gap is the recorded accuracy on the use, the edge or the vertex, written by every writer
    before `finish` (01 §2).
  - `VertexGap`, `EdgeGap` and `HeldGap` become the measurement functions those writers call. They
    return a `Length` and keep no state.
- **Pcurves: exact where a closed form exists, otherwise fitted by the writer** (D10) [Ruled
  11-27, D10 holds in direction, rewritten. The first text said the pin refuses where no closed
  form exists. Only the record path refuses; the kernel's `Trims` fit (review-63 H3).]
  - **Exact first.** Closed-form pcurves (the `eval/pcurve` cases, and SS5's sections in the D13
    uv) are exact and checked by lifting. Step 3 carries exact pcurves only.
  - **Who fits, and when.** The writer fits, eagerly, before `finish`. 04 §4 withdrew "raise
    tolerances up front, build trims lazily", and research 27 found that trims fitted on demand
    depend on the order of the reads.
  - **The target** (research 27 D2, 28, 34). Fit to the accuracy the body claims, and never raise
    it in advance. Accept when the fitted distance is within the edge's accuracy plus the face's
    accuracy plus the existing gap G (34: the pin's fitter targets the edge tolerance only, which
    is where most of its failures came from). Record an accuracy only when the result is worse
    than the resolution, or recorded accuracies creep up.
  - **The basis and its cost** (research 35, 47). The recorded accuracy says whether it is
    `Certified` or `Measured`. A certified design refused 40 trims and cost 3.3× creation time,
    76% of it in bounding G (35). A certified trim has a median of 12.8 ms against 69 µs without
    one, and 1% of trims take 73% of trim time (47). So the default is `Measured`, by the library's
    own measure in `scx-brep` or below (dense sampling of the lifted distance), and a certified
    bound is taken only where an operation asks for it. Under the hot-path rule BR9's builder
    gives before and after counts of the work added (JUDGED; for review with BR9).
  - **Poles, seams and periodicity** (the pin's `docs/validity-and-tolerances.md`): a pcurve does
    not jump across a period (Edgeuse 7), has no dog-leg at a pole (Edgeuse 20), and a pole is a
    vertex (Face 24). `finish` checks all three, and every writer, exact or fitted, keeps them.
  - **T6 is a test oracle only.** It checks the library's recorded figure in tests; no library
    code calls it. The pin never measures a pcurve against its edge (`docs/validity-and-tolerances.md`
    `:229-231`, `:441`), so T6 is a truth the pin lacks.
  - **Timing.** Curve fitting is `scx-nurbs` work in step 5 (01 §10.2). The fitted half of the old
    BR5 moves to step 5 as BR9. Until then a writer that needs a fitted pcurve returns
    `Unsupported` by name (`NeedsFit { edge }`).
  - **Records** (research 37, 41). The record stores each pcurve exactly as written, so a decoded
    body never refits. 37 found the pin loses plane trims on read, and 41 lists what its records
    cannot hold exactly; both bear on RE2 and RE3. The plane-trim carry stays held until records
    store exact forms (04, 11-25).
  - `snap_end_knots` becomes `snapped_end_knots(&Knots, Extent1d, rounding) -> Option<Knots>`,
    which returns `None` when no snap applies.
- **Free functions with `Result<Option<&Surface>>` → borrowing views** (57 §5.2):
  - `brep.face(id) -> Option<Face<'_>>`;
  - `face.surface() -> &Surface`, which is total;
  - `face.accuracy() -> Option<Accuracy>`;
  - `face.loops()`, `lp.uses()`, `edge.sides()`, `vertex.edges()` as iterators.
  - The `Result` disappears, because a missing id is `None` and a present face always has a
    surface.
- **`transform`** ports `topology/brep/mirror.rs` and the pin's `transform_matrix` (04, 11-27
  transform row):
  - ids kept;
  - every entity `Touch::Geometry`;
  - an orientation-reversing map reverses loops and keeps the quadrics' sense. Each placed kind's
    image is a right-handed frame plus a reversed kind sense (D14), so no left-handed frame is
    stored and the uv map is unchanged;
  - an analytic face under a non-similarity is `Unsupported` until step 5's refit (D4).

### 6.3 Sketch

```rust
pub struct RegionId(u32); pub struct ShellId(u32); pub struct FaceId(u32); pub struct EdgeId(u32); pub struct VertexId(u32);
// Records are #[non_exhaustive] (01 §7.4): fields readable, built only through Draft and constructors.
pub struct RegionRecord { pub solid: bool }                         // 01 §2.1; RegionId(0) is the outside by construction
pub struct FaceRecord { pub surface: Surface, pub sense: Sense, pub sides: [ShellId; 2], pub loops: Vec<LoopRecord>,
                        pub accuracy: Option<Accuracy>, cells: FaceCells }
pub struct LoopRecord { pub uses: Vec<EdgeUse> }
pub struct EdgeUse { pub edge: EdgeId, pub sense: Sense, pub geom: UseGeom, pub accuracy: Option<Accuracy> }
pub enum UseGeom { Pcurve { curve: Curve2, interval: Extent1d }, Point(Point2) }   // Point: a lone vertex
pub enum EdgeShape { Curve { curve: Curve, interval: Extent1d }, Collapsed }        // D8
pub struct EdgeGeom { pub shape: EdgeShape, pub ends: [VertexId; 2], pub accuracy: Option<Accuracy>, cells: EdgeCells }
pub struct EdgeAdj { pub radial: Vec<(FaceId, u32 /* use index */)> }   // radial order
pub struct VertexRecord { pub point: Point3, pub accuracy: Option<Accuracy> }

impl Brep {
    pub fn face(&self, f: FaceId) -> Option<Face<'_>>;
    pub fn faces(&self) -> impl ExactSizeIterator<Item = Face<'_>>;   // id order, live only
    pub fn edit(&self) -> Draft; pub fn into_draft(self) -> Draft;
    pub fn compact(&self) -> (Brep, IdMap);
}
impl Draft {
    pub fn add_face(&mut self, rec: FaceRecord) -> FaceId;
    pub fn face_mut(&mut self, f: FaceId) -> Option<&mut FaceRecord>;  // empties cells
    pub fn finish(self, doc: &Document) -> Result<(Brep, Changes), Box<Unfinished>>;
}
pub fn transform(b: &Brep, a: &Affine) -> Result<Brep, TransformError>;
```

### 6.4 Truth references missing

- **Structure invariants as executable predicates**:
  - Euler-Poincaré with genus and regions;
  - every edge's radial order lists each use exactly once;
  - each loop is closed in uv and in 3-D within its recorded accuracies;
  - the two sides of every face name existing shells, and those shells bound regions of
    consistent kind;
  - ids are dense and never reused.
  - These are the oracle for `finish`'s structure check. The check must agree with the predicate on
    generated valid and broken bodies.
- **The pcurve agreement oracle (T6).** A pcurve lifted onto its surface against the edge curve,
  with a certified bound for the analytic and NURBS cases: interval evaluation of the difference,
  through the generic-scalar evaluators (so it needs CU1, SU1 and NS2: review-63 M5).
  - This gives `finish`'s "curve and pcurve ends within vertex balls" check something to be checked
    against.
  - It checks, in tests, the fitted pcurve's recorded accuracy. It is not how the library measures
    (D10).
- **Pole and seam predicates** (T7): no jump across a period (Edgeuse 7), no dog-leg at a pole
  (Edgeuse 20), a pole is a vertex (Face 24).
- **`Changes` and `carry` property oracles.**
  - Composition: carry over two edits equals carry over the composed changes, where composition is
    defined.
  - Identity: an edit that touches nothing gives empty `Changes`.
  - Order: every list is sorted, and split pieces follow 01 §2.6's key.
  - Determinism: the same edit under two executors gives equal `Changes`.
- **Transform truth.** For a rational affine map and rational input geometry:
  - the transformed body's vertex points are the exact images;
  - the curves agree with the images at sample points;
  - the signed volume, once `scx-mass` exists, scales by `det(A)`.
  - These are exact in rationals.
- **Body generators.** Seeded generators of small valid bodies: a block, a cylinder, a sphere, a
  torus, a slab with a hole. They are built through `Draft` and used by every property above. They
  go in the testkit's `gen`.

### 6.5 Builder tasks

| Task | Work | Depends on |
|---|---|---|
| BR1 | ids, records, `Brep`/`Draft`, views and iterators, Send/Sync | SU2, CU2 (geometry types), P0, P2 |
| BR2 | `finish`: structure check, the pole and seam rules, and the cheap geometry check | BR1; T7 (T6 in its tests) |
| BR3 | `Changes`, event recording in `Draft`, `carry` | BR1 (parallel with BR2) |
| BR4 | cells: face, uv and body boxes; `FaceCell::fill`; boundary trees | BR1 |
| BR5 | pcurves, step 3: the exact cases from `eval/pcurve` and SS5's sections; `NeedsFit` elsewhere | BR1, CU3, SU2, SS5; T6 in tests |
| BR6 | `transform` (an analytic face under a non-similarity `Unsupported` until BR9) | BR1, SU7, CU5 |
| BR7 | port the provenance and history tests onto `Changes` (01 §2.6) | BR3 |
| BR8 | body generators in the testkit | BR1 |
| BR9 | step 5: fitted pcurves by the writer, eagerly, to D10's target, with the recorded basis; the D4 refit | BR5, the `scx-nurbs` fitter (step 5) |

### 6.6 BR1 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

`crates/scx-brep` (layer 5, after `scx-ssi`) holds the ids, the records, `Brep` and `Draft`,
the views and their iterators, and a `finish` that checks references only. BR2 to BR6 each
have a seam left for them:

- BR2: `check::run`, which runs after `check::references`.
- BR3: the `Changes` stub, empty and `#[non_exhaustive]`.
- BR4: `FaceCells` and `EdgeCells` with `reset()`, which hold no fields yet.
- BR5: `UseGeom`, which the writer fills.
- BR6: `Brep::edit` and `into_draft`.

**The public surface.**

```rust
pub struct RegionId(u32); /* ShellId, FaceId, EdgeId, VertexId */  // from_raw, raw, index, Display "f3"
impl RegionId { pub const OUTSIDE: Self; }
pub enum EntityKind { Region, Shell, Face, Edge, Vertex }           // name()
pub enum Entity { Region(RegionId), /* ... */ }                     // kind(), Display
pub struct IdMap { /* per kind */ }  // region(old) -> Option<RegionId>, ..., is_identity()

pub struct RegionRecord { pub solid: bool }
pub struct ShellRecord { pub region: RegionId, pub wires: Vec<EdgeId>, pub lone_vertices: Vec<VertexId> }
pub enum FaceSide { Front, Back }                                   // other(), BOTH
pub struct FaceRecord { pub surface: Surface, pub sides: [ShellId; 2], pub loops: Vec<LoopRecord>, cells }
pub struct LoopRecord { pub uses: Vec<EdgeUse> }
pub struct EdgeUse { pub edge: EdgeId, pub sense: Sense, pub geom: UseGeom, pub bound: Option<Arc<Recorded>> }
pub enum UseGeom { Pcurve { curve: Curve2, interval: Extent1d }, Point(Point2) }
pub enum EdgeShape { Curve { curve: Curve, interval: Extent1d }, Collapsed }
pub struct EdgeGeom { pub shape: EdgeShape, pub ends: [VertexId; 2], pub accuracy: Option<Arc<Recorded>>, cells }
pub struct Fin { pub face: FaceId, pub sense: Sense }
pub struct EdgeAdj { pub fins: Vec<Fin> }                           // radial order
pub struct VertexRecord { pub point: Point3, pub accuracy: Option<Arc<Recorded>> }
// every record #[non_exhaustive], built by its `new`

impl Brep {   // Clone is cheap: tables are Arc'd
    pub fn edit(&self) -> Draft;  pub fn into_draft(self) -> Draft;
    pub fn compact(&self) -> (Brep, IdMap);
    pub fn resolution(&self) -> Length;
    pub fn region(&self, RegionId) -> Option<Region<'_>>;  /* shell, face, edge, vertex */
    pub fn outside(&self) -> Region<'_>;
    pub fn faces(&self) -> impl ExactSizeIterator<Item = Face<'_>> + DoubleEndedIterator + Clone; /* each kind */
    pub fn face_slots(&self) -> usize;  /* edge_slots, vertex_slots: bitset sizes */
}
impl Draft {
    pub fn new() -> Self;
    pub fn finish(self, &Document) -> Result<(Brep, Changes), Box<Unfinished>>;
    pub fn add_region(&mut self, solid: bool) -> Result<RegionId, TableFull>;
    pub fn add_shell(&mut self, ShellRecord) -> Result<ShellId, TableFull>;
    pub fn add_face(&mut self, FaceRecord) -> Result<FaceId, TableFull>;     // appends its fins
    pub fn add_edge(&mut self, EdgeGeom) -> Result<EdgeId, TableFull>;
    pub fn add_edge_pending(&mut self, ends: [VertexId; 2]) -> Result<EdgeId, TableFull>;  // A10
    pub fn attach_curve(&mut self, EdgeId, Curve, Extent1d) -> Result<(), Absent>;
    pub fn add_vertex(&mut self, VertexRecord) -> Result<VertexId, TableFull>;
    pub fn face_mut(&mut self, FaceId) -> Option<&mut FaceRecord>;  /* region_, shell_, edge_, adjacency_, vertex_mut */
    pub fn remove_face(&mut self, FaceId) -> Option<FaceRecord>;    /* each kind; remove_region(OUTSIDE) is None */
    pub fn face(&self, FaceId) -> Option<Face<'_, Drafting<'_>>>;   /* each view, each iterator */
}
pub struct Face<'a, S: Stage = Finished>;  /* Region, Shell, Loop, Use, Edge, Vertex: Copy */
pub trait Stage: sealed { type Look<T>; }  // Finished: Look<T> = T; Drafting: Look<T> = Option<T>
pub struct Document;  // Default, with_resolution, resolution()
pub enum FinishCause { MissingCurve { edge }, Structure(StructureCause) }  // #[non_exhaustive]
pub enum StructureCause { Outside, Dangling { from: Entity, to: Entity }, FinWithoutUse { edge, fin } }
pub struct Unfinished { pub draft: Draft, pub cause: FinishCause }  // kind(), name(), Error
pub struct TableFull { pub kind: EntityKind }   // Exhausted, "table_full"
pub struct Absent { pub entity: Entity }        // Refused, "absent"
```

Every public type is `Send + Sync`, checked by a `const` assertion in `lib.rs`. The views are
checked at `'static`.

**The decisions.** For each, the case for, the case against, and what would change it.

- **The table is copy-on-write as a whole: `Arc<Vec<Option<T>>>` plus a live count.** Faces sit
  in it as `Arc<FaceRecord>`. `edit` costs one `Arc` clone per table. The first write to a table
  copies its slot vector. For faces, that copies pointers, and then only the written face is
  copied.
  - *For:* this is 01 §2.3's sharing at the least code. Reads go through one index. A face, the
    largest record and the one most often left untouched, is never copied unless it is written.
  - *Against:* the first edge or vertex write copies the whole edge or vertex table. For a body of
    1e5 edges that is about 13 MB of `EdgeGeom` per edit that touches one edge.
  - *Change if:* a profile shows edit cost growing with body size. Chunked tables (`Arc<[Chunk]>`)
    can then go behind the same `Table` type without changing its API.
- **An edge is two lockstep tables, geometry and adjacency, under one `EdgeId`.**
  - *For:* `add_face` and `remove_face` write the radial order, and doing so copies no curve.
    Most edits to topology leave the geometry alone.
  - *Against:* two tables must stay in step. `add_edge` and `remove_edge` are the only writers of
    both, and `check::references` reads them together.
  - *Change if:* never; the split costs one `Arc` per edit.
- **A fin is `(FaceId, Sense)`, not `(FaceId, u32)` with a use index (§6.3).** A fin is found
  again by scanning the face's uses for that edge and sense. `Use::fin` and `Edge::uses` do this.
  - *For:* a fin survives every loop edit that keeps the use: an insert, a split, a reorder.
    With a use index, every one of those edits must rewrite every fin after the edit point,
    because the positions shift. A seam, used twice by one face, still tells its two fins apart,
    because the two uses have opposite senses.
  - *Against:* finding a fin's use is linear in the face's uses, where an index is O(1). A face
    that uses one edge twice in the same sense would be ambiguous. That is invalid: BR2 refuses it.
  - *Change if:* profiles show `Edge::uses` hot on faces with many uses. A face cell can then map
    each fin to its use, built lazily and reset with the face.
- **Views are generic over a sealed `Stage` with a GAT.** In `Brep` they are total
  (`Look<T> = T`). In `Draft` they return `Option` (`Look<T> = Option<T>`), because a draft may
  dangle. One view type serves both stages.
  - *For:* a finished body never returns an `Option` for a reference that `finish` proved live.
    A draft never panics on one it has not proved. There is one implementation, not two.
  - *Against:* a GAT in the signatures (`S::Look<Edge<'a, S>>`) is harder to read. `Finished`
    turns an impossible `None` into `unreachable!`. That is sound only because `finish` runs
    `check::references` and no `Brep` is built any other way. `compact` and `from_tables` keep
    the invariant by construction.
  - *Change if:* a second way to build a `Brep` appears, such as a decoder that skips `finish`.
    That way must run `references` too.
- **A pending curve (A10) is the draft's `pending: BTreeSet<EdgeId>`.** The stored shape is
  `Collapsed` as a placeholder. Every view of a pending edge's shape returns `None`, and `finish`
  reports the lowest pending edge as `MissingCurve`, kinded `Defect`.
  - *For:* `EdgeShape` stays the two-case D8 enum. A finished body cannot hold a pending edge,
    because the set lives only in `Draft`.
  - *Against:* inside the draft, the record's `shape` reads `Collapsed` through `edge_mut`. A
    writer that reads the record rather than the view sees a wrong answer.
  - *Change if:* a writer trips on it. Then add an `EdgeShape::Pending` that only a draft can
    hold, refused by `finish`.
- **No stored face sense (unlike §6.3's `sense: Sense`).** The surface carries its own sense
  (D14). The face's front side is the side of the surface's natural normal.
  - *For:* one sense, not two that can disagree. Mirroring flips the kind's sense once.
  - *Against:* flipping a face means rebuilding its surface value, not toggling a bit. That costs
    a copy of up to 104 bytes.
  - *Change if:* never.
- **No stored face accuracy (unlike §6.3's `accuracy`).** It is derived from the uses' bounds
  and the edges' accuracies, so it becomes a BR4 cell.
  - *For:* 01 records accuracy where a writer measures it: on the use, the edge and the vertex.
    A stored face figure would be a cache that can go stale.
  - *Against:* if a writer ever measures a face-level figure (a surface fitted to within some
    distance), it has nowhere to go.
  - *Change if:* step 5's surface fitting records one. Then it is added as
    `FaceRecord::accuracy`, `#[non_exhaustive]` allowing.
- **Recorded accuracy is `Option<Arc<Recorded>>`, not 01's `Option<Recorded>`.** The use's
  field is named `bound`, as 01 has it. `Recorded` is 72 bytes. Inline, it makes `EdgeUse`
  152 bytes, `EdgeGeom` 192 and `VertexRecord` 96. With the `Arc` they are 88, 128 and 32.
  - *For:* most entities have no recorded accuracy, because they are within the resolution (01
    C4). Each pays 8 bytes rather than 72. A walk over uses touches about half the cache lines.
  - *Against:* a tolerant body pays a pointer chase on each read, and an allocation on each write.
  - *Change if:* profiles of tolerant bodies show accuracy reads. Then split the record into an
    inline figure plus an `Arc`'d basis.
- **`ShellRecord` stores no face list.** `Shell::faces` and `Region::shells` scan the face and
  shell tables. `Vertex::edges` scans the edges.
  - *For:* one owner per fact. A face names its shells, and no second list can disagree with it.
  - *Against:* each of those calls is O(body). An algorithm that walks shell by shell is
    quadratic.
  - *Change if:* a BR4 body cell (shell to faces, vertex to edges) is built lazily from the scan.
    That is the planned home, and the record does not change.
- **Each add returns `Result<_, TableFull>`.** A table that has given every `u32` id is
  `Exhausted`, not a panic or a wrap.
  - *For:* 01 §2.2's "never reused" stays true. Library code panics on nothing.
  - *Against:* every builder writes a `?` for a case no real body reaches.
  - *Change if:* never.
- **`attach_curve` returns `Result<(), Absent>`.** It refuses an edge that is not live. An edge
  that is live but not pending simply takes the curve.
  - *For:* a caller that names a dead id hears about it.
  - *Against:* replacing a curve that is already attached gives no signal.
  - *Change if:* BR3's event recording needs to tell an attach from a replace.
- **`add_face` appends its fins; `face_mut` does not update them.** `remove_face` drops its
  fins. A writer that edits loops through `face_mut` must fix the fins by hand, through
  `adjacency_mut`. `finish` reports `FinWithoutUse` for a fin left behind. A use without its fin
  is BR2's check.
  - *For:* the common build path, add a face, is right by construction. Editing loops in place
    is the rare writer's job, and the check catches it.
  - *Against:* half of the radial invariant is maintained, and half is checked.
  - *Change if:* BR3's event recording gives loop edits their own calls. Those calls can keep
    the fins.
- **A cell reset skips cells that are already empty.** `edge_mut` resets the cells of the faces
  that use the edge, and `vertex_mut` those of its star. Faces whose cells are empty are skipped,
  so a face is not copied just to reset nothing.
  - *For:* without the skip, every edge write would copy each adjacent face. That is the face
    copy the face `Arc` exists to avoid.
  - *Against:* until BR4 the cells are always empty, so the reset path is not exercised. The
    test `an_edge_or_vertex_write_copies_no_face` pins the skip, not the reset.
  - *Change if:* BR4 must test the reset with real cells.
- **`finish` checks references now.** It checks that the outside exists and is void, that every
  id a record names is live, and that every fin has its use. Records are walked in kind order,
  then id order, and the lowest failure is the cause.
  - *For:* the total views need it, so it cannot wait for BR2.
  - *Against:* BR2 will put its structure check next to this one, so the order of causes is
    shared ground.
  - *Change if:* BR2 needs a different order. The order is the function's.
- **How the causes are kinded.** `StructureCause` is `Refused`, because the caller built a bad
  draft. `MissingCurve` is `Defect`, per D8.
  - *Against:* a decoder's dangling id is the input's fault, not the caller's. It may want
    `Invalid`.
  - *Change if:* the record decoder (`scx-record`) maps it. This is open.
- **`Document` holds only the resolution.** 01's units and size class have no types yet.
  - *Change if:* they land. `Document` is `#[non_exhaustive]` with `Default`, so adding fields
    breaks no caller.
- **`compact` is built; `shed_cells` is not.** There are no cells yet. `compact` returns a clone
  when the `IdMap` is the identity.
- **Live iteration skips empty slots without a live list.** Research 66 §14 measured deleted
  slots costing about 3× in the pin's arena walks. The tables keep a live count, so each `len`
  is exact, and the walk skips `None` slots.
  - *For:* this is the least state to keep. `compact` is how a long edit sequence gets dense
    again.
  - *Against:* a body that has had many deletions and was never compacted walks its dead slots.
  - *Change if:* profiles show it. Then add a live bitset to `Table`, behind the same `iter`.
- **No Morton ordering** (66 §14: rejected). Id order is the order.

**Tests.** There are 19 unit tests: ids, tables, errors, sharing and copy-on-write, writes in
place on `into_draft`, no face copied by an edge or vertex write, pending and `MissingCurve`,
each structure cause and its order, the draft views, fin removal, `compact`, and record sizes.
There are three integration tests in `tests/bodies.rs`:

- A unit block is built by hand and every view and iterator is walked:
  - counts of 2 regions, 2 shells, 6 faces, 12 edges and 8 vertices;
  - Euler's formula;
  - every vertex star of 3;
  - every `next`/`prev` ring;
  - every fin to its use and back;
  - the draft walk equals the finished walk;
  - an edit and finish round trip, and `compact`;
  - the body is read from a second thread.
- A capped cylinder is walked the same way. Its wall uses the seam twice, at positions 1 and 3,
  with fins Forward then Reversed.
- A rewritten face leaves the original body alone.

There is no difftest against the pin: its topology is four use levels, and no single call
matches one view.

**Measured** (throwaway timer, not committed; release build, best of 7). The walk is A9's: all
faces, then their loops, then their uses, marking each edge in a bitset.

Record sizes, in bytes:

| scx-brep | Bytes | Pin | Bytes |
|---|---|---|---|
| `FaceRecord` | 136 | `Face` | 408 |
| `LoopRecord` | 24 | `Loop` + `Loopuse` | 40 + 40 |
| `EdgeUse` | 88 | `Edgeuse` | 472 |
| `EdgeGeom` + `EdgeAdj` | 128 + 24 | `Edge` | 392 |
| `VertexRecord` | 32 | `Vertex` | 64 |
| `ShellRecord` | 56 | | |

The walk, in ns per face:

| Body | Faces | Through `Use::record().edge` | Through `Use::edge().id()` |
|---|---|---|---|
| scx quad sheet 1×1 | 1 | 4.4 | 6.7 |
| scx quad sheet 8×8 | 64 | 5.3 | 6.4 |
| scx quad sheet 32×32 | 1,024 | 4.4 | 7.0 |
| scx quad sheet 100×100 | 10,000 | 11.3 | 17.6 |
| pin box, `Loop::edges` (port-only list) | 6 | 6.1 | |
| pin box, `loop_edgeuses` (the edge-use ring) | 6 | 38.3 | |
| pin cylinder, `Loop::edges` | 3 | 4.8 | |
| pin cylinder, `loop_edgeuses` | 3 | 33.4 | |

The pin was measured on its primitives, because it has no grid builder. The quad sheet stands
in for BR8's largest generated body, which does not exist yet.

- On equal ground the walk matches the pin's fastest path, its port-only `Loop::edges` list.
- It is about 7× faster than the pin's faithful edge-use ring, which allocates a `Vec` per loop
  and goes through a `Result` per step.
- The view path's extra cost over the record is the edge-table lookup that makes the `Edge`
  view.
- At 10,000 faces (about 1.4 MB of face records) the walk falls out of L2.
- Research 51's 4.9 ns per face is met up to 1,024 faces.

### 6.7 BR2 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

`Draft::finish` now runs, in order, and stops at the first failure:

1. every curve attached (`MissingCurve`, `Defect`);
2. `check::references` (BR1's, unchanged);
3. `check::structure`: the body invariants (`StructureCause`, `Refused`);
4. `check::geometry`: the cheap geometry check (`GeometryCause`, `Refused`).

Each pass walks records in kind order (regions, shells, faces, edges, vertices), then id order, so
with several faults the cause is the lowest and the same on every run. Both cause enums are
`Copy`, `#[non_exhaustive]`, with a stable `name()` and a `Display` that names the ids; the
geometry causes carry the gap and the allowance as `Length` where there is one.

**Structure** (the pin's `topology/brep/validation/assert_valid` rules, ported as one pass of
sorted arrays and a disjoint-set forest, no maps):

| Cause | Rule |
|---|---|
| `wire_with_fins` | a shell's wire edge has no fin |
| `face_without_loop`, `empty_loop` | a face has a loop, a loop has a use |
| `loop_not_connected` | each use starts where the one before it ends |
| `point_use_of_curve`, `pcurve_interval` | a curve edge's use is a pcurve over the edge's own interval |
| `repeated_use` | a face uses an edge at most once in each sense |
| `use_without_fin`, `repeated_fin` (and BR1's `fin_without_use`) | radial completeness: the fins are the uses, each once |
| `vertex_on_two_loops` | no vertex on two loops of one face (the pin's Face 39) |
| `collapsed_edge_ends` | a collapsed edge starts and ends at one vertex |
| `loose_edge`, `loose_vertex` | every edge has a fin or a shell, every vertex an edge or a shell |
| `sides_disagree` | the sector rule: for radially consecutive fins `a`, `b`, the shell after `a` (front for a forward fin) is the shell before `b` |
| `empty_shell`, `shell_not_connected` | a shell holds something and is one piece (face sides joined across edges by the sector rule and at shared vertices) |
| `region_without_shell` | every region but the outside has a shell; the outside has one if any region does |

**Geometry** (research 60 V1 at fixed samples, 63 D10; 8 segments per edge and per use, the ends
exact):

| Cause | Rule |
|---|---|
| `edge_end_off_vertex` | a curve's ends within its vertices' balls (the pin's Edge 16, 17) |
| `edge_too_short` | the curve longer than its own reach (Edge 7) |
| `use_end_off_vertex` | a use's lifted ends within its vertices' balls |
| `outside_domain` | every sample in the surface's domain |
| `pcurve_off_edge` | every sample lifts onto the edge's curve at the same parameter (onto the vertex, for a collapsed edge) |
| `period_jump` | no two consecutive samples 0.9 of a period apart (Edgeuse 7, the pin's threshold) |
| `dog_leg` | at a pole, a pcurve only arrives or leaves: samples at the pole near an end keep the end's `u` (Edgeuse 20); 10 probes at `2^-j` of the interval toward each pole end catch a leg shorter than a segment |
| `pole_not_vertex` | no interior sample at a pole when neither end is there (Face 24) |
| `loop_open_in_uv` | each use ends where the next starts in uv, within the two ends' budgets over the surface's larger speed there |
| `loop_orientation` | loop 0 counter-clockwise from the front, the others clockwise (shoelace on the samples) |
| `face_wraps` | the loops span at most a period in each closed direction |

Poles are the sphere's two and the cone's apex. A NURBS surface's collapsed rows are not found yet.

**T7** is `scx_testkit::topology`: a body as plain incidence data (indices, senses, sides), with
`violations` (every failed invariant, sorted), `euler` and `genus` per shell, and the pole and
seam predicates `jumps`, `dog_leg` and `passes_pole` on dense uv samples. It is written with maps
and a breadth-first search where the library uses sorted arrays and disjoint sets. The tests
adapt each draft to it and require agreement on every body: the structure is refused exactly when
the oracle finds a violation, and the cause is among them. T6 is not needed yet: every pcurve in
the tests is exact, and the lift check compares at the edge's own parameter.

Decisions, each with the case for and against:

- **The whole body is checked, not the touched part** (01 §2.3 says touched).
  - *For:* touched needs the draft's events, which are BR3's; until then the whole body is the same
    answer, and the cost is linear (measured below).
  - *Against:* an edit of one face in a large body pays for every face.
  - *Change if:* BR3's events land. Then the structure pass keeps its global parts (shells,
    regions) and the record parts run on the touched records.
- **Distances add**: an end is within `reach(edge) + reach(vertex)` of the vertex, a lifted sample
  within `reach(use)` of the curve, and a use's end within `reach(use) + reach(edge) +
  reach(vertex)`. A reach is the recorded accuracy, or the zone where none is recorded.
  - *For:* truth. The curve's end is within its reach of the true end and the vertex within its
    own, so the two are within the sum. The pin's `tau_e + tau_v` says the same.
  - *Against:* research 60 V1 wrote the maximum. The sum admits up to twice as much.
  - *Change if:* the accuracies are redefined as distances to each other rather than to truth.
- **A collapsed edge has no radial order**: its fins are not paired by the sector rule, in the
  check or in joining a shell's pieces.
  - *For:* a pole's single fin has its face on both sides; the rule would read that as front equal
    to back and refuse every closed sphere and cone.
  - *Against:* a collapsed edge with two fins from different faces is then not checked for sides.
  - *Change if:* bodies with shared collapsed edges appear (a double cone's apex); then pair them.
- **uv closure is strict**: the joint gap is allowed only the two ends' 3-D budgets over the
  surface's larger speed. So a seam must be an edge used on both sides, and a pole's line must be
  a collapsed edge's pcurve (63 §6, D8).
  - *For:* the loop's uv polygon is then the face's trim, so orientation and wrap are well defined.
  - *Against:* the pin accepts a loop that jumps a seam at a vertex, where the 3-D gap is zero. That
    body is refused here.
  - *Change if:* a writer needs it; the jump would be recorded as a uv shift on the use.
- **A pole is one collapsed edge per face, each with one fin and a pcurve along the pole's line**
  (BR8's form, asked of BR2). The pcurve's interval is its own: a collapsed edge has no
  parameter, so the `pcurve_interval` rule compares nothing there, and `[0, 1]` is a convention,
  not an assumption the check makes.
  - *For:* strict closure needs the pole's line in the loop's uv (a `UseGeom::Point` leaves a jump
    from the meridian's end to the next one's start, refused as `loop_open_in_uv`); faces round a
    pole stay independent, so editing one lune touches no record of another; the use carries the
    pcurve, so nothing needs an interval on the edge.
  - *Against, shared edge with n fins:* one edge per pole is closer to the 3-D truth (one point),
    but its fins have no radial order (see above), so their list order is a convention to keep
    deterministic and every edit round the pole rewrites it. *Against, an interval on
    `Collapsed`:* a field with no 3-D meaning, there only to repeat what the pcurve already holds.
  - *Change if:* a body needs the faces round a pole joined topologically (a double cone's apex);
    then a shared edge with its fins paired by angle, as the radial rule's change-if says.
  The check does not refuse a shared pole edge; the generators and writers use this form.
- **Face 24 by the pole check plus closure**: a pole inside a face's interior (no edge through it)
  is the case the shoelace and the domain catch together, since a loop round a pole on a sphere
  must cross the pole's line, which closure allows only along a collapsed edge's pcurve.
- **Loop 0 may turn clockwise only on a surface with an open direction** (a plane, a cylinder,
  the cone's `v`): the face is then the surface minus holes, unbounded.
  - *For:* BR1's sheet test and a half-space face are valid bodies.
  - *Against:* on a plane, a closed solid's face with its loop reversed is read as an unbounded face,
    not refused; the sides check catches the reversal only when the sides were not also swapped.
  - *Change if:* 01 rules every face bounded; then loop 0 is always counter-clockwise.
- **Point loops have no orientation** (a lone vertex, or a loop of collapsed edges): there is no
  area to sign.
- **Euler is a T7 check, not a `finish` one**: for a body that passes the record and shell checks
  the characteristic is whatever the topology is; there is no genus to compare it with. The tests
  assert genus 0 on the valid bodies.
- **`finish` does not compare region kinds across a face.** Both sides solid is a non-manifold
  interface, which 01 allows.

Deferred, with where they belong:

- the radial order's geometry (the pin's Edge 9, 10: fins counter-clockwise by the faces' normals);
- inner loops inside the outer and loops not crossing (Face 37, 38): BR4's cells hold the trim;
- a NURBS surface's poles;
- region connectivity (Brep 18, 19);
- the sample count: 8 segments catch a fault wider than an eighth of a use. A tighter check is
  T6's job in the tests, and the writer's at fit time (D10).

**Difftest against the pin's checker: not feasible yet.** The pin's `check_brep` reads its own
arena, and there is no converter from an scx body to it (RE3's record path is the first). The pin's
sphere and cone uv are in radians where ours are D13's arc parameters, so even a converter must
remap every pcurve. Recorded for RE3: once a body can be decoded on both sides, the same bodies go
to both checkers, with each disagreement judged against T7.

**Tests.** 3 new unit tests (the pole rules on hand-made pcurves, each also judged by T7's
dense-sample predicates) and 4 integration tests in `tests/finish.rs`: a block, a cylinder, a
sphere and a cone finish, with genus 0 by T7; 20 structure faults each give their cause, and 10 broken bodies give 9 of the
11 geometry causes (`dog_leg` and `pole_not_vertex` come from the unit tests); a structure fault outranks a geometry one, the lower of two ids is named, and a rerun
gives the same cause. 4 unit tests for T7 in `scx-testkit`. BR1's tests changed in two places: a
pending line ran between one vertex and itself (now two vertices), and a face turned over with its
sides left alone is now refused as `sides_disagree` (the test makes a valid rewrite instead).

**Cost.** Release build, best of 5 runs of 200 finishes of a cloned draft, before (BR1, references
only) and after:

| Body | BR1 | BR2 | of which structure |
|---|---|---|---|
| 100 blocks, 600 planar faces | 17.8 µs (0.03 µs per face) | 698 µs (1.16 µs per face) | 195 µs (0.32 µs per face) |
| block, 6 faces | 0.20 µs | 6.9 µs | 1.7 µs |
| cylinder, 3 faces | 0.07 µs | 4.0 µs | 0.5 µs |
| sphere, 1 face | 0.05 µs | 6.0 µs | 0.3 µs |
| cone, 2 faces | 0.06 µs | 3.4 µs | 0.4 µs |

The added work is about 1.1 µs per face, three quarters of it geometry: 8 samples per use, the
endpoint evaluations, and on a sphere the pole probes and trigonometry. It is linear in the body, so
a large body pays it in full on every finish until finish checks only what the draft touched
(open: BR3's touched sets, §6.8, make that possible).

**Long loops (after BR8).** BR1's fin-to-use test scanned the face's uses for each fin, quadratic
in a loop's length (BR8 saw 78 to 170 µs per face on long-loop bodies). It now sorts every use
once as `(face, edge, sense)` and finds each fin's by binary search. On an n-gon prism (two caps
of n uses), release, best of 3: n = 1,000: 1.79 to 1.74 ms; n = 4,000: 9.69 to 6.75 ms; n =
16,000: 86.7 to 26.5 ms (5.4 to 1.66 µs per face, now flat in n). The 600-face blocks body is
unchanged (1.09 µs per face).

**Merged with BR3.** Eight of BR3's `Changes` tests make bookkeeping edits that leave no valid
body (a removed face's edges left loose, a lone vertex in a shell with faces, an edge's ends
swapped), which the full check refuses. Their `done` now finishes through a test-only
`finish_references_only` (BR1's check, then the same `Changes`); the strip they start from still
goes through the full `finish`. *For:* what they test, the events and lineage, does not depend on
validity, and making each edit valid would hide that under unrelated construction. *Against:* a
path production never takes; the risk is small because the two share everything after the check.

### 6.8 BR3 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

**What was built** (`crates/scx-brep/src/changes.rs`, the recording in `draft.rs`):

- `Draft::finish` returns honest `Changes`: for faces, edges and vertices, an `EntityChanges<I>`
  with `replaced: Vec<(I, Touch)>`, `created: Vec<(I, Origins)>`, `merged: Vec<Merge<I>>`,
  `coincident: Vec<Coincidence<I>>`, `removed: Vec<I>` and `split: Vec<Split<I>>`. Every list is in
  id order; split pieces follow the key below.
- `Touch` is `Boundary` (loops, sides, an edge's ends) or `Geometry` (surface, curve, interval,
  accuracy, a vertex's point).
- `Origin` is `Face`, `Edge`, `Vertex` (part of an entity the edit started with), `Offset(Entity)`,
  `Operand(Operand, Entity)` (from a tool body) or `Free`. `Origins` is sorted, without repeats,
  never empty; `[Free]` costs no allocation.
- Draft calls added: `derive(new, origin) -> Result<(), LineageError>`,
  `coincident(operand, entity, onto) -> Result<(), Absent>`, `merge_face`, `merge_edge`,
  `merge_vertex` (remove the absorbed entity and name its survivor), and
  `edit_face(id, |&mut FaceRecord| ..) -> Option<R>`.
- `carry(changes, own, operands, rule) -> BTreeMap<I, T>` with `trait Carry<T>` (`split`, `merge`,
  `coincident`, each returning `Option<T>`). `IdMap::renumber` carries an application table across
  `compact`.
- `Tracked` (sealed; `FaceId`, `EdgeId`, `VertexId`) makes all of it generic over the kind.

**Decisions**, each with the case for and against:

1. *Touch is found by diff, not by logging each write.* The first `face_mut`, `edge_mut` or
   `vertex_mut` on an entity the draft started with keeps its record as it was (a face's `Arc`, an
   edge's geometry without cells, a vertex); `finish` compares. For: a write that changes nothing
   reports nothing (an edit that touches nothing gives empty `Changes`, 63 §6.3's identity oracle),
   and no mutation path can forget to log. Against: one copy per first write, and the touch is
   record-level only: a face around a moved vertex, or an edge whose radial order changed, is not
   listed. Change if an application needs "faces whose geometry moved through a vertex": that is
   derivable from `vertices.replaced` and the stars.
2. *Under `into_draft`, the first write to a before-face copies it once*, since the kept snapshot
   shares its `Arc`. BR1's test `into_draft_writes_in_place_when_unshared` now asserts in-place
   writes for edges and for faces the draft added. For: honest `Changes` need the old record.
   Against: `into_draft`'s "no copy" promise is weaker. Change if a profile shows it: a draft could
   take a "no changes" flag.
3. *Vertices get a full `EntityChanges`* (01 §2.6 has a smaller `VertexChanges`). For: one generic
   `carry`, and vertex merges and coincidences are real (a boolean's coincident corners). Against:
   a few unused lists. Kept.
4. *Named `Merge` and `Coincidence` records* (57 §5.1) and a `split` list. A split is read off the
   created entities that name a kept or removed entity as their part: `Split { from, pieces }` lists
   `from` itself among the pieces when it survives.
5. *Split piece order* is a total key: the lineage of the bounding entities (a face's edges, an
   edge's vertices), then the uv midpoints of its boundary (`total_cmp`), then the id. For: stable
   across executors and independent of creation order in the common case. Against: 01 §2.6 says an
   interior point; boundary midpoints are cheaper and need no point-in-face test, but two pieces
   with the same bounding lineage and the same boundary midpoints fall to the id, which is creation
   order. Change if BR7's ports show a case where that matters.
6. *`Origin::Offset`* added beyond 01, for offset and thicken (the pin's `offset_history`,
   `issue_1973_*`). It carries by the same-kind rule (an offset face takes the face's values).
7. *`derive` resolves at call time*: an origin naming a created entity stands for that entity's
   origins as they are then. For: lineage always ends at the before-body or an operand. Against: a
   later `derive` on the intermediate does not flow forward. `derive` on a before-id, or naming an
   absent origin, is refused (`LineageError::NotCreated`, `UnknownOrigin`).
8. *A created entity absorbed into a before-id drops its lineage*; one absorbed into a created id
   adds its origins. A chain of merges is followed to the end; an absorbed entity whose survivor is
   gone counts as removed.
9. *`Carry` rules return `Option`* (01 has `T`), so "no value" is expressible, as the pin's
   attribute handlers allow (`issue_1730_application_attribute_handlers`). Cross-kind origins carry
   nothing.
10. *`carry` takes `operands: &[&BTreeMap<I, T>]`*, indexed by `Operand`, for the boolean
    attribute tests (`issue_1462_boolean_*`). The tables are `BTreeMap`, as `IdMap` is taken by
    `compact`.
11. *BR1's question, loop edits keeping fins in step: yes.* `edit_face` diffs the face's
    `(edge, sense)` uses before and after the closure and fixes only the changed fins, so a seam's
    two fins stay distinct. `face_mut` is kept for edits that do not move uses. `attach_curve`
    needed nothing: its touch comes from the diff.
12. *Regions and shells are not reported.* No pin test reads them; add when one does.
13. *No difftest*: the pin has no equivalent of `Changes` to compare against (its history is
    callbacks into application code). BR7 ports the tests themselves.
14. Research 66 has nothing on provenance.

**Tests** (15 in `changes/tests.rs`): each mutation's event and touch (including a re-attach of the
same curve giving none), adds and removes, splits ordered by bound and by place, merges through
chains, refused derives, coincidences following the survivor, the same edit twice giving equal
`Changes`, carry over a split, merge, coincidence and offset, carry across an edit then `compact`,
and `edit_face` on a seam.

**Cost** (MEASURED, throwaway timer, a 100×100 quad sheet: 10,000 faces, 20,200 edges, 10,201
vertices; best of 9; ns per call or µs per finish; the host was loaded, so ±15%):

| Step | Before | After |
|---|---|---|
| `add_*`, per entity | 50–72 | 60–80 |
| `face_mut`, first write | 90–124 | 127–142 |
| `face_mut`, later write | 37–49 | 53 |
| `edge_mut`, first write | 12–18 | 35–41 |
| `vertex_mut` (the star scan dominates) | 16,000–26,000 | 16,000–29,000 |
| `remove_face` / `remove_edge` / `remove_vertex` | 106–137 / 42–58 / 10–13 | 116–152 / 52–67 / 9–11 |
| `finish`, built draft (40,401 created) | 700–1,200 | 1,800–2,000 |
| `finish`, untouched edit | 260–500 | 320–540 |
| `finish`, `face_mut` on every face | 540–950 | 940–1,400 |
| `finish`, `face_mut` on 100 faces | 300–540 | 320–540 |
| `edit_face`, no change / rotated loop | | 156–175 / 171–204 |
| split every face via `derive`, then `finish` | | 5,400–8,200 |

Snapshots are a bit per slot plus a list in first-write order, so a repeat write costs a bit test.
The remaining first-write cost is the copy itself (an edge's curve clone). `finish` on a fully
built draft pays for the 40,401-entry created lists.

**Open**:

- `Changes::then`, the composition oracle (63 §6.3), is not built; it needs a rule for a split of a
  merged entity.
- Whether `Offset` should carry by its own rule rather than the same-kind one.
- Touch of faces around moved edges and vertices (decision 1).
- The `into_draft` copy (decision 2).

### 6.10 BR8 as built (11-27)

Decided by analysis and an adversarial pass. Proposed for review; not yet ruled.

`scx_testkit::bodies`, behind the testkit's optional `bodies` feature, builds valid bodies
through `scx-brep`'s public `Draft` API only. It also builds one broken body per structure
invariant, and holds an independent oracle, `check::violations`, that names the invariants
a draft breaks. Every body is a function of `(kind, seed, size)`. Seed 0 is canonical: unit
sizes, placed about the origin along z, no jitter. Any other seed draws the sizes, a
right-handed frame and jittered splits from a SplitMix64.

**Placement.** `layers.txt` puts `scx-testkit` in `[testkit]`, which may depend on any
library, so a testkit dependency on `scx-brep` fits the layering. The open choice was
whether to use a feature or a crate.

- *For the feature* (chosen): one truth crate, as 63 §6.4 asks, and no new crate.
  - The geometry layers' tests all dev-depend on the testkit. Without the feature they do
    not build `scx-brep`, and `cargo xtask fast` is unchanged.
  - `scx-brep`'s own tests can take the generators: an integration-test dev-dependency on
    the testkit with `bodies` type-unifies.
- *Against*, adversarially:
  - A feature makes the dependency cycle `scx-brep` → (dev) testkit → `scx-brep` real for
    `scx-brep`'s tests. In its `src` unit tests the two `scx-brep`s are distinct crates and
    their types do not unify. Its integration tests are unaffected.
  - A separate `[testkit]` crate (`scx-testkit-brep`) would make the cycle explicit and need
    no feature.
  - Answer: the generators are used from integration tests and `scx-difftest`. A unit test
    that needs a body can call `generate` from `tests/`. A second crate costs a
    `layers.txt` row and a crate for 2,100 lines. Revisit this if unit tests come to need
    the bodies.
- 63 §6.4 named the module `gen`. It is `bodies`, because `gen` is a reserved keyword in
  edition 2024.

**The public surface.**

```rust
pub enum Kind { Block, Prism, QuadSheet, TriSheet, Cylinder, Cone, Sphere, Torus, Holed, Cavity, Stack }
impl Kind { pub const ALL: [Kind; 11]; pub fn name(self) -> &'static str; pub fn faces(self, size: usize) -> usize }
pub struct Counts { pub regions, shells, faces, loops, uses, edges, collapsed, vertices: usize, pub euler: i64 }
pub struct Generated { pub draft: Draft, pub counts: Counts }
#[non_exhaustive] pub enum GenError { Table(TableFull), Curve(CurveError), Surface(SurfaceError), Pcurve(PcurveError), Degenerate(&'static str) }
pub const BUILD_TOL: Length;                                    // 1e-9 mm
pub fn generate(kind: Kind, seed: u64, size: usize) -> Result<Generated, GenError>;   // size 0 is 1
pub fn broken(invariant: Invariant, seed: u64, size: usize) -> Result<Option<Draft>, GenError>;
pub mod check {
    pub enum Invariant { OutsideVoid, LiveReferences, FinWithoutUse, UseWithoutFin, RepeatedFin, SameSenseTwice,
        LoopChained, PeriodJump, PoleGap, UvGap, IntervalMismatch, PcurveOffEdge, VertexOffEdge, ShellOpen,
        Misoriented, RadialOrder, OrphanVertex }                // ALL
    pub struct Tolerance { pub space: f64, pub uv: f64 }        // Default: 1e-7, 1e-9
    pub fn violations(draft: &Draft, tol: Tolerance) -> BTreeSet<Invariant>;
}
```

**The bodies**, with `n = size`:

| Kind | Faces | What it covers |
|---|---|---|
| `Block` | `6n²` | a box, each side an `n × n` grid of planar faces, with jittered breaks |
| `Prism` | `n + 5` | an `(n+3)`-gon extruded: long planar loops |
| `QuadSheet`, `TriSheet` | `n²`, `2n²` | an open sheet with the outside on both sides, and random diagonals |
| `Cylinder` | `n + 2` | `n` wall faces between rulings, so one face crosses the seam; two caps |
| `Cone` | `n + 1` | `n` wall faces to the apex, a collapsed edge per face; a base |
| `Sphere` | `n` | `n` lunes, a collapsed edge at either pole per face |
| `Torus` | `n²` | `n × n` patches, crossing both seams; genus 1 |
| `Holed` | `n + 9` | a slab with an `(n+3)`-gon through-hole: an inner loop on two faces; genus 1 |
| `Cavity` | `12n²` | a solid with a void inside: two shells in the solid, one in the void |
| `Stack` | `5n + 6` | `n + 1` blocks sharing faces, one region each; 3-fin edges in radial order |

The pcurves come from `scx_surface::pcurve_of`, aligned along the loop by whole periods.
A pole's use is the uv segment along u from the arriving use's end to the leaving use's
start. `Counts` are closed forms, checked against the draft. Euler is
`V − (E − C) + F − (L − F)`, where C counts the collapsed edges. It gives 2 for the convex
bodies, 0 for the torus and the holed slab, 1 for the sheets, 4 for the cavity (two
spheres) and 4 for a stack of three.

**The broken bodies.** Each one writes a single defect into a valid body through the
`Draft` API, so that `violations` reports that invariant and no other:

- a solid outside;
- a removed edge;
- a fin pushed for a face that does not use its edge, and a fin popped;
- a fin repeated;
- a sheet's centre loop doubled, with its fins;
- two uses swapped;
- a cylinder wall's arc moved by a period;
- a lune's pole use removed;
- a pcurve interval halved;
- a pcurve bowed into an arc through the same ends;
- a vertex moved;
- a face removed;
- one face's loop reversed, with its fins;
- a 3-fin edge's radial order reversed;
- an orphan vertex.

`UvGap` has none, because no single defect breaks it alone: a uv gap that is not a period
or a pole also moves a pcurve off its edge, or unchains the loop. Today's `finish`
refuses three of them: `OutsideVoid`, `LiveReferences` and `FinWithoutUse`. BR2's `finish`
should refuse all sixteen, and its tests can take `broken` as is.

**Tests.** `crates/scx-testkit/tests/bodies.rs` has 8 tests:

- every kind, at seeds {0, 1, 2, 7, 0x5eed} and sizes {1, 2, 3, 5}, holds its counts, breaks
  nothing and finishes;
- the Euler values above;
- every outer loop has positive uv area;
- the convex bodies face outward;
- each broken body breaks its invariant alone;
- `finish` refuses the three it checks;
- a seed is deterministic;
- seed 0 is canonical.

The difftest `t_bodies` checks the seed-0 block, cylinder, cone, sphere and torus, at sizes
1 to 4, against the pin's `create_box`, `create_cylinder`, `create_cone`, `create_sphere`
and `create_torus`. The two sides split the solid differently, so each is checked against
the solid instead:

- the pin's face surfaces, sampled over their uv domains, lie on the solid's sheets;
- the generated vertices, edge samples and face middles lie on a sheet and in the closure;
- the generated surface kinds are the solid's own. The pin's are printed but not compared,
  because it writes a cylinder as a cone of half-angle 0;
- the box corners are the same bits.

It finds 0 mismatches.

**The walk** (BR1's A9 measurement, redone): all faces, then loops, then uses, marking each
edge in a bitset. Bodies are seed 0, finished. The numbers are release builds, best of 7,
in ns per face.

| Body | Faces | `Use::record().edge` | `Use::edge().id()` |
|---|---|---|---|
| block n=14 | 1,176 | 5.5 | 8.2 |
| block n=144 | 124,416 | 40.8 | 63.3 |
| quad sheet 33 × 33 | 1,089 | 4.4 | 7.4 |
| cylinder n=1045 | 1,047 | 6.6 | 11.1 |
| cylinder n=102908 | 102,910 | 38.6 | 67.0 |
| cone n=1045 | 1,046 | 5.2 | 9.3 |
| sphere n=1045 | 1,045 | 4.1 | 8.0 |
| sphere n=102908 | 102,908 | 29.5 | 48.4 |
| torus 33 × 33 | 1,089 | 4.0 | 7.4 |
| torus 325 × 325 | 105,625 | 28.0 | 41.3 |
| holed n=1045 | 1,054 | 7.4 | 11.5 |
| cavity n=10 | 1,200 | 5.6 | 8.3 |
| cavity n=102 | 124,848 | 41.3 | 62.6 |
| stack n=204 | 1,026 | 5.8 | 8.2 |
| stack n=22261 | 111,311 | 36.8 | 63.8 |
| pin box: `Loop::edges` / `loop_edgeuses` | 6 | 6.1 | 40.4 |
| pin cylinder | 3 | 5.1 | 34.1 |
| pin cone | 2 | 5.8 | 34.2 |
| pin sphere | 1 | 6.3 | 35.6 |
| pin torus | 1 | 8.5 | 43.5 |

- At about 1,000 faces, every kind walks in 4 to 7 ns per face. That is at or below the
  pin's port-only list (5 to 8.5) and 5 to 8× faster than its edge-use ring.
- At about 10,000 faces it is 13 to 28 ns, and at about 100,000 faces it is 28 to 53 ns. That
  is memory, not work: the face records then exceed L2, and later L3.
- The pin has no generator for a matching 100,000-face body.
- Generation costs about 1.5 to 7 µs per face. Its main costs are `pcurve_of` and the
  frames.
- **Finding for BR2: `finish` is quadratic in loop length.** `check::references` scans the
  whole face for each fin, to find its use. The prism, cylinder, cone and holed slab have a
  loop of `n` uses. At about 100,000 faces they finish in 78 to 170 µs per face, against
  0.1 to 0.4 µs for the grid bodies. A per-face `(edge, sense)` set, or a check from the use
  side, makes it linear.

**Open questions.**

1. **A collapsed edge's use.** Each pole face has its own collapsed edge with one fin. Its
   use is a `Pcurve` segment, a `Line2` over an assumed `[0, 1]`, because `EdgeShape::Collapsed`
   has no interval. The alternatives:
   - share one collapsed edge per pole, with `n` fins;
   - make the use `UseGeom::Point`, which leaves a uv gap the length of the face's u span;
   - give `Collapsed` an interval.

   BR2's pole rules, and BR5, should settle this.
2. Euler-Poincaré must leave collapsed edges out (they are not 1-cells), or count them as
   a vertex and an edge that cancel.
3. Face 24's "a pole is a vertex", and `UvGap` alone, have no broken body.
4. The dev-dependency cycle is described above: integration tests only.
5. `EdgeAdj` has only `Default`, so a builder fills its fins through `adjacency_mut`.
   `Draft::add_face` appends fins loop by loop, which is enough for the generators. No API
   was added to `scx-brep`.

## 7. `scx-record`

The pin's sources are `topology/api_files`, `topology/part_file`, `topology/database_io` and
`eval/label`. Its own `brep_data` and `brep_data_write` are the pin's format, so they go to
`scx-import-sc` (04, 11-27). They are read here only for their lessons.

### 7.1 Shapes found

| Shape | Where (READ) | Count (MEASURED) |
|---|---|---:|
| An iostream emulation with sticky state bits | `topology/database_io.rs:212`-`218` `IoState::{GOOD, BAD, EOF, FAIL}`; `:282` `good()`, `:294` `fail()`, `:299` `clear()` | `&mut self` 55 |
| Extraction returning `Option` per field, with sticky failure | `topology/database_io.rs:582` `extract_long`, `:590` `extract_ulong`, `:636` `extract_double` | — |
| Out-param readers | `topology/brep_data.rs:1204` `ext_double(s, &mut self.zone_tol_3d)` and its neighbours | — |
| Branches on the file version through the decode | `topology/database_io.rs:117` `DEFAULT_VERSION = 30`; `version > 30` and `version >= 40` | 33 in `brep_data`, 24 in `brep_data_write` |
| A bool stored as `i32`, a record kind as an integer that decides which fields mean anything | `topology/brep_data.rs:1228` `manifold: i32`; `:880` `eu_type: u64` ("0 face edge, 1 loop vertex, 2 wire edge, 3 vertex at a pole"), with `edge`, `vertex`, `shell` and `loop_` valid per type | int flag params 5 |
| Variable-length lists flattened with counts | `topology/brep_data.rs` `cfaces`, `cedges`: `[count, n0, face.., n1, ..]` | — |
| Integer codes for enums | `topology/database_io.rs:135` `code()`, `:140` `from_code(i32)` | code/from_code 3 |
| File paths in the library | `topology/api_files.rs:183` `read_brep_from_file`, `:272` `write_brep_to_file` | — |
| A kernel-wide conversion with an optional body | `topology/api_files.rs:293` `turn_to_nurbs(k: &mut Kernel, brep: Option<&mut Brep>)` | `&mut Kernel` 6 |
| Labels computed from geometry, with ±inf for unbounded | `eval/label.rs:15` `enum Natural { Periodic, Range(f64, f64) }`, `:23` `UNBOUNDED` | — |

### 7.2 Proposed shapes

- **A reader that fails with `?`, not with sticky bits.**
  - `Reader<'a> { bytes: &'a [u8], pos: usize }` with `u32()`, `f64()`, `tag::<T>()` and `list()`,
    each returning `Result<_, ReadError>`.
  - `ReadError { at: usize, cause: ReadCause }` has typed causes, each kinded per X13 (62 §10):
    `Truncated`, `BadMagic` and `BadTag { found }` are `Refused` (the bytes are not a record);
    `BadVersion { found }` is `Unsupported` (a newer record this reader does not know);
    `Invalid(FinishCause)` inherits its cause's kind. `ReadError` is a context frame: it adds the
    byte offset. [Ruled 11-27, M2.]
  - There is no state to `clear`.
- **Our own format, versioned from 1.** No branch on pin versions: that is `scx-import-sc`'s job.
  - Each top-level record starts with `(magic, version)`.
  - A reader accepts versions up to its own and refuses newer ones with `BadVersion`.
  - Version-dependent decoding is a `match version` at the record boundary, never threaded through
    field reads.
- **Kinds as enums with their own fields.**
  - `eu_type` becomes the enum of the record's use kinds. Under 01 C4 there is only the `EdgeUse`
    and the lone-vertex loop (01 §2.1), so the tag picks a variant whose fields are all
    meaningful.
  - The tag is a `u8` on the wire, mapped by an exhaustive `match`. An unknown tag is `BadTag`, not
    an `Unknown` variant (57 F2).
- **Lists are length-prefixed `Vec`s.** No flattened count streams.
- **Ids kept; deleted runs as counts** (01 §2.2): `enum Slot { Live(rec), Gap(u32) }` on the wire,
  never in memory.
- **Exact numbers.**
  - Binary: little-endian IEEE bits.
  - Text: Rust's shortest round-trip `Display` for `f64` and `str::parse` (std only, both correctly
    rounded), so text round-trips the bits.
  - Non-finite values are always refused. Under D1 no ±inf or sentinel is ever stored, so a
    `ParamDomain` goes on the wire as its variant tag with finite ends only. [Ruled 11-27, D1: this
    line allowed ±inf as a parameter bound.]
- **Bytes, not files.** `encode(&Brep) -> Vec<u8>`, `decode(&[u8]) -> Result<Brep, ReadError>`,
  and `write_text(&Brep, &mut impl fmt::Write)`. Paths and IO are the app's.
- **The decoder does not heal** (01 §1.1). It validates through `Draft::finish`, so a record that
  decodes is a valid body, and a bad one is `Invalid` with the `FinishCause`. It checks curve
  presence before `finish`, so bytes never reach the `MissingCurve` defect (D8).
- **Pcurves are stored exactly as written** (D10), so a decoded body never refits. Research 37
  (the pin loses plane trims on read) and 41 (what its records cannot hold exactly) are the
  lessons for RE2 and RE3; the plane-trim carry stays held (04, 11-25).
- **Records are a public format under semver** (01 §7.4): adding a record kind or a field is a new
  version, and fields of the Rust record types are private or `#[non_exhaustive]`.
- **Labels** (`eval/label`): `Natural` becomes `scx-curve`/`scx-surface`'s `ParamDomain`. If labels
  are stored at all, the record stores them as a check copy. Otherwise they are recomputed on read
  (JUDGED: recompute, and store nothing derived).

### 7.3 Sketch

```rust
pub const FORMAT_VERSION: u16 = 1;
pub fn encode(b: &Brep) -> Vec<u8>;
pub fn decode(bytes: &[u8], doc: &Document) -> Result<Brep, ReadError>;
pub fn write_text(b: &Brep, out: &mut impl core::fmt::Write) -> core::fmt::Result;
pub fn read_text(s: &str, doc: &Document) -> Result<Brep, ReadError>;
pub struct ReadError { pub at: usize, pub cause: ReadCause }
#[non_exhaustive] pub enum ReadCause { Truncated, BadMagic, BadVersion { found: u16 }, BadTag { record: &'static str, found: u8 }, Invalid(Box<FinishCause>) }
```

### 7.4 Truth references missing

- **The round-trip fixed point** (step 3's done test):
  - `decode(encode(b)) == b`, with structure, ids and geometry bits equal;
  - `encode(decode(x)) == x` for every `x` that decodes;
  - the same for text, and `binary → text → binary` is equal bytes.
  - This is an invariant oracle over the body generators (BR8).
- **Corruption fuzz.** Every truncation and every single-byte flip of a valid record either decodes
  to a valid body or fails with `ReadError`. It never panics and never yields a body that fails
  `finish` (std only: a seeded byte-flip loop with the testkit's `Rng`).
- **Exact procedural records** are a step-5 concern (blend definitions). The slot is reserved.

### 7.5 Builder tasks

| Task | Work | Depends on |
|---|---|---|
| RE1 | `Reader`/`Writer`, `ReadError`, tags, lists, `Slot` runs | P0 (finite extents: review-63 M5) |
| RE2 | geometry records: curves, surfaces, NURBS | RE1, CU2, CU3, SU2, NS1 |
| RE3 | body records and decode through `Draft::finish` | RE2, BR1, BR2 |
| RE4 | the text encoding | RE1 (parallel to RE2) |
| RE5 | round-trip and corruption tests | RE3, RE4, BR8 |

## 8. Prerequisites outside the six crates

- **P0. Finite extents in `scx-num`** (D1 revised; ahead of RE1, BR1, CU1 and SU1). [Ruled 11-27:
  new task, after review-63 H1. Today `Extent1d` stores ±`INFINITE_PARAMETER` (`UNBOUNDED`), so
  "finite by construction" was not true.]
  - `Extent1d::new` (and the 2-D and 3-D forms) refuses non-finite or reversed ends, so every
    extent is finite by construction.
  - Remove `UNBOUNDED`, `INFINITE_PARAMETER`, `is_infinite_param` and the infinity branches in
    `shift` and `times` across `extent1d`, `extent2d`, `extent3d`, `math.rs` and `transform.rs`.
    Their callers take an explicit working extent.
  - Add the named clamping rules that replace the pin's constants: `Clamp::Sample`
    (`BOUNDED_INFINITE_PARAM`) and `Clamp::Squared` (`INFINITE_PARAMETER_SQRT`, for parabolas).
    The bound is sized from the model, not fixed, because the pin's 1.2e6 rounds by 3.4e-11 in
    double (#18922).
  - Neither ±inf nor a sentinel is ever stored.
  - **Built 11-27 (builder K).** Decisions taken in the build, for review:
    - `Extent1d::from_bounds` is now `Extent1d::new(min, max) -> Option<Extent1d>`: `None` for
      a non-finite or reversed end. `Extent2d`/`Extent3d::from_bounds` keep their names (they
      take per-axis ends) and refuse the same.
    - Every constructor or transform whose result could leave the finite range returns
      `Option`: `point`, `spanning`, `translated`, `scaled`, `expanded`, `expanded_relative`
      on `Extent1d`; `from_point`, `spanning`, `expanded`, `expanded_vec`,
      `expanded_relative`, `scaled`, `scaled_xy` on `Extent2d`; `spanning`, `from_point`,
      `expanded`, `expanded_relative` on `Extent3d`. `None` means an end overflowed (or an
      input was not finite); no caller in the workspace reaches it on its data. `Hull`'s
      finish is `None` when a fed value is infinite.
    - `Extent1d::intersect_periodic` refuses a period that is not finite (`InvalidInput`).
      The pin answers on (NaN: no pieces; ±inf: the first interval whole, even when the two
      are apart); pin defect `e1_intersect_periodic` holds scx to the refusal.
    - Removed: `UNBOUNDED`, `BoundaryType`, `boundary_type(s)`, `is_bounded`, `any_bounds`,
      `approximate_unbounded*` (the half-size of 12 for an unbounded end; P2's
      `ParamDomain::working` replaces it with a named rule), `math::{INFINITE_PARAMETER,
      INFINITE_PARAMETER_SQRT, BOUNDED_INFINITE_PARAM, is_infinite_param}` and
      `Transform::natural_param_domain` (it answered an all-unbounded box; P2 brings it back
      as one `ParamDomain` per direction).
    - `scx_num::domain::{Clamp, Reach}`: `Clamp::Sample` (33), `Clamp::Squared(Reach)` (the
      root of the reach, rounded down so its square never exceeds the reach; at reach 1234567
      it is the pin's 1111.111 to 1e-3) and, beyond the brief, **`Clamp::Linear(Reach)`** for
      the kinds whose point is linear in the parameter (the pin's 1234567 used as a bound).
      `Reach::covering(model_box, origin)` is twice the farthest-corner distance. *For
      ruling:* whether `Linear` should exist, and whether `Reach` comes from the model box
      (proposed) or the resolution.
    - `scx-algo`: `ObjsInVoxels::map_voxel_index_to_bbox` closes the last voxel on each axis
      on the box's max where the pin's `min + n * step` rounds to infinity (a box reaching the
      end of the `f64` range); other ends are the pin's bits. `local_solve_nd`'s clamp lost
      its `is_bounded` guards (a bounded domain is always bounded).
    - Difftest: the facade maps the pin's extents with a non-finite or `±SM_INFINITE_PARAMETER`
      end (`plain_ends`) to no extent on both sides. Nine truth defects whose disagreements
      were NaN-bound extents now apply to no input and were removed (`e1_max_dimension`,
      `e1_union_intersect`, `e1_periodic_boundary`, `e2_max_dimension`, `e2_union_intersect`,
      `e3_max_length`, `e3_sphere_min_radius`, `e3_union_intersect_far`,
      `scaled_zero_of_boxes`); pin defect `ht_voxels` is new (the pin's voxel boxes at the
      `f64` range end have infinite ends); `e3_intersect_line`/`e3_line_hit_count` now cover
      only a line that is not finite through a real box. Totals after the rebase onto the
      landed `scx-nurbs` and `scx-bench`: 356 diffs, 830,314 inputs, 0 mismatches.
    - Migrations from the rebase: `scx_nurbs::KnotVector` holds its domain as one `Extent1d`
      (built once in `new`; `domain()` is now `const`); `scx-bench` calls `Extent1d::new`, and its edge intervals and boxes reach to `f64::MAX` where
      they reached to infinity.
- **P1. `Frame` (and `Rigid`) in `scx-num`.**
  - Every analytic kind is a frame plus sizes. 01 §1.1 promises frames in `scx-num`, and 57 §5.2
    lists `From<Frame> for Rigid`.
  - Today `scx-num` has `Affine` (`math.rs:127`), `Point3`, `Vec3`, `UnitVec3` and `Axis`, but no
    frame. The testkit has its own `Frame` (`scx-testkit/src/analytic.rs:38`).
  - The pin's counterpart is `Axis2Placement`.
  - Proposed (D14): `Frame { origin: Point3, x: UnitVec3, z: UnitVec3 }`, with `x ⟂ z` by
    construction (`Frame::new(origin, z, x_hint) -> Option<Frame>`), `y = z × x` derived, and
    `to_affine()`. Fields private with accessors (01 §7.4). z is always stored, since every kind's
    axis is z; y is stored as well only if CU1 or SU1 measures the cross product as a cost.
  - Right-handed by construction. A mirror maps a placed kind to a right-handed frame plus a
    reversed `sense` on the kind, so no left-handed frame is ever stored (D14). [Ruled 11-27, D12.
    The first proposal stored x and y.]
  - This is a small `scx-num` task and blocks CU1 and SU1.
  - **Built 11-27 (builder K).** Decisions taken in the build, for review:
    - `scx_num::frame::Frame { origin, x, z }` (private), `Frame::WORLD`, accessors `origin`,
      `x`, `y`, `z`, and `to_affine()` (`Affine::from_axes(x, y, z, origin)`).
    - `Frame::new(origin, z, x_hint) -> Option<Frame>`: `None` when the origin is not finite,
      when `z` or the hint has no direction (`Vec3::normalized`), or when the hint lies along
      `z`. x is the hint's part square to `z`. It is the pin's `SetSTEPCanonical` (through
      `MakeUnitOrthoVectors`) without its arbitrary x for a parallel or zero hint and without
      keeping a non-finite origin: pin defect `frame_new`.
    - `y()` is `z × x` of the two stored unit vectors, not renormalised (square within
      rounding; the property test holds lengths and dots to 8 ε). *For ruling:* whether to renormalise or
      store y if CU1/SU1 measure it.
    - `Frame::transformed(&Affine) -> Option<(Frame, Sense)>`: the image origin, x along the
      image of x, z the right-handed normal of the image xy plane, and `Sense::Reversed` when
      the image of z points against it (a mirror; D14). `None` when the map flattens the
      frame or the image is not finite. `scx_num::frame::Sense { Forward, Reversed }` with
      `reversed()` and `sign()`. *For ruling:* the name `Sense` and its home in `scx-num`
      (the kinds in `scx-curve`/`scx-surface` carry it).
    - **No `Rigid` exists** in `scx-num` (only `Affine`), so there is no `From<Frame> for
      Rigid`; 57 §5.2's conversion waits for a `Rigid` type.
    - `Vec3::make_unit_ortho_vectors` now shares its non-arbitrary path with the crate-private
      `unit_ortho_toward`; its results are unchanged.
- **P2. `ParamDomain`.** A shared type for a parameter's domain: `Periodic { period, base }`,
  `Bounded(Extent1d)`, `AtLeast(f64)`, `AtMost(f64)` or `Unbounded`. It is placed in `scx-num`
  beside `PeriodicExtent1d` so curves and surfaces share it; a `UvDomain` is one per direction.
  `ParamDomain::working(&self, rule: Clamp) -> Extent1d` gives the finite extent numeric work
  runs on. It depends on P0.
  - **Built 11-27 (builder K).** Decisions taken in the build, for review:
    - `scx_num::domain::ParamDomain`: `Periodic { period, base }`, `Bounded(Extent1d)`,
      `AtLeast { from }`, `AtMost { to }`, `Unbounded`. The variants that carry numbers are
      `#[non_exhaustive]` struct variants, so outside `scx-num` they are built only through
      `periodic(period, base)`, `at_least(from)`, `at_most(to)` (each `-> Option`: ends finite,
      period finite and above zero, `base + period` finite) and matched with `..`. *For
      ruling:* struct variants where the brief wrote `AtLeast(f64)`; a `#[non_exhaustive]`
      tuple variant cannot be matched outside its crate at all.
    - `working(&self, rule: Clamp) -> Extent1d` is total. With `h = rule.half_width()`:
      bounded gives its range, periodic `[base, base + period]`, unbounded `[-h, h]`; from
      `a`, `[a, a + 2h]` when `a > -h`, else `[-h, h]`; up to `b`, `[b - 2h, b]` when `b < h`,
      else `[-h, h]`. This is the pin's `ApproximateUnbounded(center = 0, half)`; the window
      end saturates at `±f64::MAX` where the pin's overflows to infinity. No arithmetic is done
      on a missing end, and a real end equal to `1234567` is not taken for a missing one
      (the pin swaps such an end with its marker): pin defect `domain_working`.
    - `UvDomain { u, v }` with `working(&self, u: Clamp, v: Clamp) -> Extent2d`: one rule
      per direction, since a kind can grow linearly in one and as a square in the other.
    - `Transform::natural_param_domain` is back as `[ParamDomain; 3]`, all `Unbounded`.
    - *For ruling:* `scx-algo`'s `local_solve1d::Domain1d { Line, Bounded, Periodic }` overlaps
      `ParamDomain` (a solver's search range, not a kind's domain); left as it is.
    - `period()` is the only accessor; a kind's domain is matched, not queried.
- **Testkit additions (T1-T8)**, from the sections above. Each is an independent builder task on
  `scx-testkit`:

| Id | Reference | For |
|---|---|---|
| T1 | certified ellipse foot (quartic isolation) | CU4 |
| T2 | line and conic against quadric and torus intersection truths | SU6 |
| T3 | all stationary points for torus and cone projection (global nearest) | SU4 |
| T4 | exact on-quadric predicate for rational `to_nurbs` images; the D13 uv map checked against the closed form | SU1, SU3 |
| T5 | analytic SSI truths: plane-plane, plane-quadric conics (WR-680's oblique cylinder section among them), sphere-sphere, coaxial | SS2, SS3, SS5 |
| T6 | pcurve agreement oracle with a certified bound; a test oracle only, never called by the library (D10). Needs CU1, SU1 and NS2, so not day one | tests of BR2, BR5, SS4 |
| T7 | structure invariants (Euler, radial completeness, sides) and the pole and seam rules as predicates; needs BR1's types | BR2 |
| T8 | surface derivatives of order 3 and up tested; tensor-product insert, elevate, split | NS2, NS3 |

T2, T3 and T5 are written on the testkit's own types (its `Frame` and analytic kinds, with `Affine`
for placement), so they do not wait for CU1 or SU1. A thin adapter maps them once those land.

## 9. Adversarial pass

For each proposal: the argument against it, then the verdict. "Holds" means the proposal stands.
"Revised" means it changed, and the change is folded into sections 1-8 above unless stated here.

**A1. Fixed-size jets in place of `Vec` partials (X10).**

- **Against.**
  - The pin returns `Vec<Vec3>` because callers ask for a run-time number of derivatives.
  - A const generic forces each call site to know its order at compile time.
  - Some callers, curvature analysis at a run-time continuity order for example, take the order
    from data.
- **Measured.**
  - A scratch microbenchmark (rustc `-O`, 2×10⁷ calls; not repo code) compared a circle-like curve
    jet of 3 vectors and a 3×3 surface partials block.
  - Curve: `Vec<Vec3>` 26.4-27.1 ns, `[Vec3; 3]` 17.2 ns.
  - Surface: `Vec<Vec<Vec3>>` 55.5-57.0 ns, `[[Vec3; 3]; 3]` 7.9 ns (MEASURED, but unverified:
    review-63 found the inputs were not black-boxed and the outputs not consumed whole, so the
    fixed-size figure may be partly optimised away).
  - The nested vector costs about 47 ns per surface evaluation, which is 7× the evaluation itself.
    Inversion and marching evaluate surfaces in their inner loops.
- **Verdict: holds, with a revision, pending a fair rerun.** [Ruled 11-27, D3: holds pending a
  rerun in the new `scx-bench` crate, inputs black-boxed and outputs consumed whole. The 7.9 ns is
  unverified.]
  - Run-time orders are served by the largest const order the kind supports (`MAX_PARTIAL_ORDER`
    from `eval/`), truncated.
  - Or a `jet_dyn(&self, t, order, out: &mut [Vec3])` is kept as the second batched out-param,
    which 57 allows for evaluation into caller storage.
  - Builders add `jet_dyn` only when a caller with a data-driven order is ported.

**A2. Dropping `non_zero_tangents` (X5).**

- **Against.**
  - Upstream's default is TRUE (`EvaluateFlags`, `nurbs/b_spline_surface_n_lib/p2.rs:186`). So
    almost every pin caller that normalises a derivative relies on it silently: at a pole, a
    collapsed edge or a cusp, the "tangent" is a short vector along the limit direction, never
    zero.
  - Dropping it moves the failure to the caller's normalise, which now returns `None`. Unless each
    port handles that, code that worked at the pin fails at poles in scx.
  - That is a lesson the flag encodes: degenerate points are everywhere in B-reps (sphere poles,
    cone apices, collapsed NURBS edges).
- **Verdict: revised.**
  - True derivatives stay the default, because the faked magnitude is not a derivative and would
    poison Taylor models and certification.
  - The lesson is kept as a required step at the call sites.
  - The pin's callers of `evaluate(.., non_zero_tangents = true)` that take a direction are ported
    to `tangent_dir`, which returns the limiting direction.
  - `tangent_dir`'s doc states "defined at poles and cusps where the limit exists".
  - The difftest facade maps the pin's flag-on output to `tangent_dir` scaled, so comparison stays
    meaningful.
  - Builders of CU1, SU1 and NS2 grep the pin for each caller with the flag on and list them in the
    PR, with how each is served. The count is the 59 + 92 + 5 + 2 lines above.

**A3. Analytic kinds without a carried NURB (section 4.2).**

- **Against.**
  - The pin carries the NURB because the general intersector, the box builders and the fallback
    projections ask for it repeatedly. `kind_with_nurb` is called 341 times in the surface code.
  - Rebuilding it per call could cost a hot path.
- **Answer.**
  - In scx the general intersector is not called on analytic pairs that have a closed form (SS1).
  - Boxes of analytic patches have closed forms.
  - The remaining callers are the step-5 B-spline-against-analytic pairs. There the NURB is built
    over the trimmed extent once per call, so the cost is one conversion per intersection call, not
    per evaluation.
  - For a quadric, the conversion is a few-by-three net (JUDGED, not measured: nothing to run
    yet).
- **Verdict: holds.**
  - SU3's builder records the conversion's cost against one inversion, as before and after numbers
    for the hot-path rule.
  - If it ever shows in the top costs, the NURB is memoised in the caller's `Scratch`, keyed by the
    kind's bits and the extent (01 §3.1). It is never stored on the kind.

**A4. Splitting Cylinder from Cone and Circle from Ellipse.**

- **Against.**
  - The pin merged them, so each has one inversion, one intersection set and one transform.
  - Splitting doubles the match arms, and a cone becomes a cylinder at a zero angle only through a
    kind change.
  - The pin's merge also means a cylinder under a transform stays the same kind trivially.
- **Answer.**
  - Separate kinds give each its own closed forms (keeping the pin's parameterisation, D13): the
    circle's foot is closed form, the ellipse's needs a quartic; a cylinder has no apex.
  - The pin's merged cone carries an `is_cylinder` branch internally, so the arms exist either way
    (JUDGED).
  - A cone with a zero angle is not constructible (`half_angle` in `(0, π/2)`), so no kind ever
    changes by data.
- **Verdict: holds.** [Ruled 11-27, H2: "natural parameterisation" here meant each kind's own
  closed forms. The uv is the pin's NURBS-compatible one (D13), not a natural one.]

**A5. Total matches instead of failing defaults (X2).**

- **Against.**
  - A failing default lets a new kind compile before every operation is written. A total match
    forces each kind to answer everything, or builders write `unimplemented` stubs, which are the
    same shape in disguise.
  - "Kind X cannot do Y" is also information. The pin's `base_err(Transform)` for an offset curve
    records that no exact image exists.
- **Verdict: holds, revised in how incompleteness is shown.**
  - In step 2 every slice kind is complete, so no stub is needed.
  - For later kinds, an operation not yet written for a kind returns the crate's
    `Unsupported(KindName)` variant. It is listed in the crate's README as a gap, and no
    `todo!()` or `unimplemented!()` is used (lints forbid panics).
  - An operation that *cannot* be exact (transforming an offset) records an accuracy under the
    11-26 rule, which is the lesson `base_err` was standing in for.

**A6. No owner tolerance on geometry (X3).**

- **Against.** `owner_zone_tol_3d` lets a deep geometry routine widen its box by the face's
  tolerance without any caller passing it. Removing it means every routine that needs it gains a
  parameter, and missing one silently uses a tighter tolerance than the pin. That is a correctness
  regression on bodies with recorded gaps.
- **Verdict: holds, with a check.**
  - Each routine that read the owner's tolerance at the pin (52 + 92 + 3 + 21 lines) takes an
    explicit `Length`.
  - Its builder lists each one in the PR.
  - The difftest's bodies with raised accuracies (the CTC import cases of 04, 11-27) exercise the
    path.
  - A routine that forgets is caught by the type, because the parameter is not optional.

**A7. Enum dispatch rather than `dyn` (C10, X2).**

- **Against.** A large enum makes every `Curve` value as big as its largest arm. The pin boxes
  `SurfOfRevolution` and others "because it is the largest kind".
- **Verdict: holds.**
  - NURBS and procedural payloads are behind `Arc` (01 §7.1), so the analytic arms are the largest.
  - The largest step-2 arm is the torus: a frame of 9 f64 plus 2 radii, about 88 bytes.
  - Match dispatch is a jump table with no indirection and inlines the analytic closed forms. `dyn`
    would forbid the generic-scalar evaluation 01 needs.
  - A size assertion per enum (`const _: () = assert!(size_of::<Surface>() <= 128)`) keeps it honest.

**A8. `Closed::General` as a success value (X7, SS1).**

- **Against.** The pin's bool `needs_more` is cheap and composes. A caller can merge partial
  closed-form results with the general intersector's, for example the analytic plane cut of a
  surface of revolution "special cases" plus the general rest. An enum that is either solved or
  general loses "partly solved".
- **Verdict: revised.**
  - `Closed` gains a third variant only where the pin does produce partial answers:
    `Partial { solved: T, rest: Extent2d }`, where `rest` is the uv region still to march.
  - The pin's step-2 kinds always return all or nothing: the sphere's doc says
    `rbNeedsMoreIntersections` "stays FALSE".
  - So the variant is added in step 5 with surfaces of revolution, not now. `Closed` is
    `#[non_exhaustive]`, so adding it is not breaking.

**A9. Dropping `Topology` marks and user slots (section 6.2).**

- **Against.**
  - Marks on the entity are O(1) to set and test. A local `IdSet` per traversal allocates, and
    traversals are hot in the Boolean and blend.
  - The pin's 146 mark lines are spread over many algorithms that would each grow a set.
- **Answer.** Ids are dense `u32`s, so an `IdSet` is a bitset of `len / 64` words, cleared by
  reallocation or reuse from `Scratch`. That is O(1) per test, like a mark, and needs no write to the
  shared body. A write would force copy-on-write of a shared `Arc<FaceRecord>` just to mark it,
  which is far costlier.
- **Verdict: holds.** BR1's builder measures one traversal (all faces of the largest generated
  body, marking each edge) with the bitset, as the before and after record for the hot-path rule.

**A10. `Draft::add_edge` requires the curve (section 6.2).**

- **Against.**
  - Some construction orders build topology before geometry: the stitcher, the sweeper and the
    pin's `brep_constructor` (1,588 lines) add edges and later attach curves.
  - Requiring the curve up front forces those ports to restructure.
- **Verdict: revised.** [Ruled 11-27, D8 revised further: a collapsed edge has no curve and is
  valid (01 §2.1); review-63 M1. The text below is the ruled form.]
  - `Draft` allows an edge whose curve is not yet attached, in a draft only.
  - The finished geometry is `EdgeShape { Curve { curve, interval }, Collapsed }`, so a collapsed
    edge is valid with no curve.
  - At `finish`, a non-collapsed edge with no curve is `FinishCause::MissingCurve { edge }`, kinded
    `Defect`, because only our own operations build drafts that way. The public builders and the
    decoder refuse a caller's input that lacks one before `finish`.
  - The `Brep` view's `edge.shape()` is total, because the view is built only over finished
    tables.

**A11. `PARAM_BOUND` as ±inf (X6, D1).**

- **Against.**
  - Infinities propagate silently: `inf - inf = NaN`, and a box of an unbounded line becomes NaN in
    one subtraction.
  - The pin's `1234567.0` is ugly, but every downstream computation stays finite, and the pin's
    geometry code has been debugged with it.
  - Arithmetic on a finite bound is also exact enough for extents.
- **Answer.**
  - The pin's own newer code (`eval/label.rs`) already uses infinities.
  - A finite magic bound silently truncates a model larger than 1.2 km in millimetres.
  - Both representations need clamping before evaluation. The difference is that ±inf makes a
    missing clamp produce NaN, which fails loudly in `finish`'s checks and in debug assertions,
    whereas `1234567` yields a plausible wrong answer. That is 57 M8's argument.
- **Verdict: held here; overturned by review-63 H1 and superseded by D1 revised.** [Ruled 11-27.
  Two premises were wrong: `Extent1d` is not finite by construction today (it stores
  ±`INFINITE_PARAMETER`), and NaN is not loud (comparisons with NaN are false, so a missing clamp
  passes most checks silently). The ruled form stores neither ±inf nor a sentinel: unboundedness
  is a `ParamDomain` variant, extents are finite by construction (P0), and numeric work runs on an
  explicit working extent from a named clamping rule (X6).]
  - The original text: a domain that is unbounded is `ParamDomain::Unbounded` at that end; every
    routine that needs a finite range takes an `Extent1d`/`Extent2d`; unbounded geometry never
    flows into arithmetic without passing through a constructor that rejects non-finite bounds.
    The last two survive into the ruled form.

**A12. Our own record format, without the pin's (section 7.2).**

- **Against.** The pin's format exists, is documented by its reader, and solidcore designs are
  written in it. A new format means two readers, and the converter (`scx-import-sc`) is extra work
  that a "read the pin format natively" approach avoids.
- **Verdict: holds.**
  - 04's 11-27 row already rules the converter as a helper crate that heals. The pin's format
    cannot carry two face sides, regions with kinds, or recorded accuracies as `Option`, and it
    renumbers on write (01 §2.2).
  - The lesson kept from the pin's reader is its defect list: uninitialised locals at
    `version >= 40` and 8-byte `ULONG`s (`topology/database_io.rs:58`-`61`). Every width is fixed in
    our format and stated in the spec.

**A13. Free-function analytic SSI over normalised pairs (section 5.2).**

- **Against.** The pin dispatches inside each surface kind (`Sphere::intersect_with_plane`). That
  keeps the kind's private state accessible and lets each kind add a pair without touching a
  central match.
- **Verdict: holds.**
  - Kinds in scx are plain data with accessors (fields private or `#[non_exhaustive]`, 01 §7.4), so
    nothing a pair needs is hidden. [Ruled 11-27, LOW: this said "public fields".]
  - A central match over unordered pairs makes the missing pairs visible as one table: the
    `General` arms.
  - The pin's per-kind methods duplicated the swap logic in each direction. `plane.intersect(sphere)`
    and `sphere.intersect(plane)` are separate code at the pin (`surface/plane/p2.rs:781` against
    `surface/sphere.rs:1178`).

**A14. A curve-surface hit list sorted by `t` (section 4.2).**

- **Against.** The pin's hit order may encode a preference: first hit from the curve's start, or
  from the guess. Callers may take `[0]` meaning "the one near my guess".
- **Verdict: holds.**
  - Sorting by `t` is deterministic, and "near a guess" is a separate query (`nearest_hit(guess)`)
    or a filter at the caller.
  - The builder of SU6 lists the pin's callers that take the first hit and how each is served.

## 10. Task graph across the crates

[Ruled 11-27, M5: redrawn. The `scx-nurbs` landing, NS1, NS2 and P0 are now on the critical path;
RE1 and BR1 depend on P0; SS2 depends on the plane section (SS5, was SU5); CU2 follows CU1; T6
is a test oracle that needs CU1, SU1 and NS2.]

```text
P0 Extent finite ─┬─ P2 ParamDomain ─┐
                  ├─ RE1 ─┬─ RE4 ────────────────────────────────── RE5
                  │       └─ RE2 (CU2, CU3, SU2, NS1) ─ RE3 ─────── RE5
P1 Frame ─────────┴──────────────────┴─┬─ CU1 ─ CU2 ─┬─ CU3 (scx-nurbs) ─ CU5 ─ BR6
                                       │      ├─ CU4 (T1 for ellipse)
                                       │      └─ CU6 (after CU2)
                                       └─ SU1 ─┬─ SU6 (CU1, CU4, T2)
                                               └─┐
scx-nurbs lands ─ NS1 ─┬─ SU2 (SU1) ─┬─ SS1 ─┬─ SS5 (SU1, CU1, CU3, T5) ─ SS2 (T5) ─┐
                       │             │       └─ SS3 (T5) ─────────────────────────────┴─ SS4
                       │             ├─ SU8 (CU2)
                       │             └─ BR1 (CU2, P0, P2) ─┬─ BR2 (T7) ─ RE3
                       │                                   ├─ BR3 ─ BR7
                       │                                   ├─ BR4
                       │                                   ├─ BR5 (CU3, SS5) ─ BR9 (step 5)
                       │                                   ├─ BR6 (SU7, CU5)
                       │                                   └─ BR8 ─ RE5
                       ├─ SU3 (SU1, T4) ─ SU7
                       ├─ NS2 (T8) ─┬─ SU4 (SU2, T3)
                       │            ├─ NS4
                       │            └─ T6 (CU1, SU1): test oracle for BR2, BR5, SS4
                       ├─ NS3
                       └─ NS5
```

- **Can start now, without `scx-nurbs`:** P0, P1, and the testkit truths T1, T2, T3, T4, T5 and
  T8 (T2, T3 and T5 on the testkit's own types).
- **After P0:** P2 and RE1, then RE4.
- **After P1 and P2:** CU1 and SU1, in parallel.
- **After CU1:** CU2, then CU6; CU4 (line and circle; the ellipse waits for T1); SU6 once SU1, CU4
  and T2 are in.
- **Everything else waits for `scx-nurbs`:** NS1-NS5, CU3, CU5, SU2-SU4, SU7, SU8, all of SS, BR
  (BR1 needs SU2) and RE2, RE3 and RE5. T6 waits for NS2, and T7 for BR1's types.
- **The critical path to step 2's done test:** `scx-nurbs` lands → NS1 → SU2 → SS1 → SS5 → SS2 →
  SS4, with P0 → P2 → SU1 (with P1) joining at SU2, CU1 → CU2 → CU3 joining at SS5, and NS1 → NS2
  → SU4 (T3) beside it.
- **Step 3's done test** (a round trip keeps ids and bytes; cells empty after both edit paths;
  provenance tests pass) needs BR1-BR5, BR7, BR8 and RE1-RE5. BR5 in step 3 carries exact pcurves
  only; a writer that needs a fitted one returns `NeedsFit` (`Unsupported`).
- **Step 5** adds BR9: the writer's eager pcurve fitter (D10) and the refit of pcurves after a
  non-similarity on an analytic face (D4), on `scx-nurbs`'s fitting.
- **Hot-path measurements owed** (as before and after counts of added work; local side-channel
  counts, nothing committed):
  - A1: the jet shape, rerun fairly in `scx-bench` (D3), then in NS2 and SU1;
  - A3: the NURB conversion, in SU3;
  - A9: the mark bitset, in BR1;
  - D14: whether storing `y` in `Frame` pays, in CU1 and SU1;
  - D10: the fitter's added work and the measured-basis cost, in BR9;
  - A2: the `tangent_dir` callers, listed rather than timed.

## 11. Decisions for review (proposed 04 rows)

The coordinating session ruled these on 2026-11-27 (section 12); 04 §3 carries them as rows for the
owner's review. The table gives the ruled form.

| Id | Ruled row | Grounds |
|---|---|---|
| D1 | Unboundedness is a state of the type. A shared `ParamDomain` (P2) says per direction whether a parameter is periodic, bounded, half-open or unbounded. `Extent1d` ends are finite by construction, and `UNBOUNDED`/`±INFINITE_PARAMETER` go (P0, ahead of RE1, BR1, CU1, SU1). Numeric work uses an explicit finite working extent; `BOUNDED_INFINITE_PARAM` and `INFINITE_PARAMETER_SQRT` become named `Clamp` rules (#18922). Neither ±inf nor a sentinel is ever stored. [Revised 11-27] | X6, A11; review-63 H1 |
| D2 | Derivatives are true derivatives. `non_zero_tangents` is not ported, and its callers use `tangent_dir(t, side) -> Option<UnitVec3>`, each listed in its PR | X5, A2 |
| D3 | Evaluation returns fixed-order jets (`CurveJet<D>`, triangular `SurfaceJet<D>`). A run-time order uses a batched `jet_dyn` into caller storage, added only when a caller needs it. Holds pending a fair rerun of A1 in `scx-bench` (inputs black-boxed, outputs consumed whole); the 7.9 ns is unverified | X10, A1; review-63 |
| D4 | Analytic surfaces carry no NURB. `to_nurbs(&Extent2d)` is exact, bounded and keeps uv. A non-similarity on an analytic surface gives `Surface::Nurbs`, which needs an extent; its pcurves are refit by step 5's fitter (BR9), and until then the result is `Unsupported` by name (`AnalyticNonSimilarity`). [Revised 11-27] | X4, 4.2, A3; the 11-27 transform row; review-63 |
| D5 | Step 2's kinds: `Line`, `Circle`, `Ellipse`, `Nurbs` curves (and 2-D twins); `Plane`, `Cylinder`, `Cone`, `Sphere`, `Torus`, `Nurbs` surfaces; circle separate from ellipse, cylinder separate from cone | 01 §7.3 starting points, A4 |
| D6 | Closed-form geometry answers are `Closed<T> { Solved(T), General }`, with `Partial` added in step 5 if surfaces of revolution need it. No "needs more" bool | X7, A8 |
| D7 | Inversion returns its outcome as a value, 57 F6's `Inversion<P> { Inside(Foot<P>), Outside(Foot<P>) }`; iterative arms return a `Result` under a budget and a stop, and no caller inspects an error code. [Reconciled with F6 11-27] | X8; 57 F6 |
| D8 | An edge's geometry is `EdgeShape { Curve { curve, interval }, Collapsed }`: a collapsed edge has no curve and is valid (01 §2.1). A `Draft` may hold an edge whose curve is not yet attached; a missing curve that our own operations produce is `FinishCause::MissingCurve`, kinded `Defect`; a caller's input without one is refused before `finish`. [Revised 11-27] | A10; review-63 M1 |
| D9 | Entity marks and user slots are not ported: traversal state is a dense-id bitset, and application data is the app's `IdMap` with `carry` | 6.2, A9 |
| D10 | Pcurves are exact where a closed form exists, checked by lifting. Otherwise the writer fits them eagerly before `finish`, to the body's claimed accuracy (never raised in advance), accepting within edge accuracy + face accuracy + gap G, and records the accuracy and its basis (`Measured` by default; `Certified` on request) only when worse than the resolution. `finish` checks the pole and seam rules (Edgeuse 7, 20; Face 24). T6 is a test oracle only. The fitter lands in step 5 (BR9); step 3 carries exact pcurves only, and a writer that needs a fit returns `NeedsFit`. [Rewritten 11-27: only the record path refuses; the kernel's `Trims` fit] | 6.2; research 27, 28, 34, 35, 37, 41, 47; 04 §4; review-63 H3 |
| D11 | `scx-record` has its own format from version 1: bytes in and out, typed tags, length-prefixed lists, ids kept with deleted runs as counts, shortest round-trip text for `f64`. The decoder validates through `finish` and does not heal. The format and the record types are public API under 01 §7.4 | 7.2, A12; 04 11-27 converter row; 01 §7.4 |
| D12 | `Frame` joins `scx-num` before step 2's kinds (P1): an origin, x and z stored (y derived, or stored too if measured cheaper; z always stored), right-handed by construction, built by `Frame::new -> Option`. [Revised 11-27] | P1; D14 |
| D13 | Surfaces and curves keep the pin's analytic, NURBS-compatible parameterisation, computed in closed form with no carried NURB. Oblique cylinder sections stay exact (WR-680), and converters and the difftest need no uv mapping; `scx-step` maps STEP's angle parameters. The natural-parameter switch is withdrawn everywhere. [New 11-27, H2] | review-63 H2; 4.2; WR-680 |
| D14 | A mirror gives a right-handed frame plus a reversed `sense` on the kind; no left-handed frame is ever stored. Evaluation uses the effective axis `sense · z`, so the uv map and the pcurves are unchanged. [New 11-27, from D12] | P1, 3.2, 6.2 |

## 12. Rulings (2026-11-27)

The coordinating session ruled this research on 2026-11-27, adopting the corrections of
`reviews/review-63.md`. The body above is revised in place; each changed place carries a "[Ruled
11-27: ...]" note.

- **D1, revised.** Unboundedness is a type-level state (`ParamDomain`, P2). `Extent1d` ends become
  finite by construction and the `UNBOUNDED`/`±INFINITE_PARAMETER` sentinel goes: a new `scx-num`
  task, P0, ahead of RE1, BR1, CU1 and SU1. Numeric work goes through an explicit finite working
  extent. `BOUNDED_INFINITE_PARAM` and `INFINITE_PARAMETER_SQRT` become named clamping rules,
  citing #18922. Neither ±inf nor a sentinel is ever stored. (Review-63 H1 overturned A11.)
- **H2.** Keep the pin's analytic, NURBS-compatible uv, so oblique cylinder sections stay exact
  (WR-680) and converters and the difftest need no uv mapping. Added as D13; the natural-parameter
  switch is removed everywhere.
- **D4, revised.** A non-similarity on an analytic surface gives NURBS, which needs an extent. Its
  pcurves are refit by the step-5 fitter (BR9); until then the result is `Unsupported` by name.
- **D8, revised.** A collapsed edge has no curve (01 §2.1) and is valid. A missing curve that our
  own operations produce is a `Defect`.
- **D10.** Holds in direction; the row is rewritten. Its premise was wrong: only the record path
  refuses, and the kernel's `Trims` fit (review-63 H3). Folded in: research 27, 28, 34, 35, 37, 41
  and 47 (eager fitting by the writer, the fit target with gap G, the certified-bound cost); the
  pole and seam rules (Edgeuse 7, 20; Face 24); T6 is a test oracle only. The fitter lands in step
  5, so BR5's fitted half moves to step 5 (BR9) and step 3 carries exact pcurves only.
- **D12.** `Frame` stores x and z with y derived (or whatever is cheapest, but z is stored),
  right-handed by construction. A mirror gives a right-handed frame plus a reversed sense on the
  kind; no left-handed frame is ever stored. Added as D14.
- **D3.** Holds pending a fair rerun of A1's benchmark in the new `scx-bench` crate (inputs
  black-boxed, outputs consumed whole). The 7.9 ns is marked unverified.
- **M2.** Aligned with 62 §10: every cause is kinded (`TooLarge` is `Exhausted`, `Degenerate` is
  `Refused`, `BadVersion` is `Unsupported`); each wrapper is marked as a transparent or a context
  frame; iterative sketches take a budget or a stop. The review's "X10" (errors) is fixed as X13.
- **M3.** The plane section belongs in `scx-ssi` (01 §1.1), not SU5: it is SS5.
- **M5.** The task graph is fixed: the `scx-nurbs` landing, NS1, NS2 and P0 are on the critical
  path; RE1 and BR1 depend on P0; SS2 depends on the plane section (SS5); CU2 follows CU1, and T6
  is no longer parallel from day one.
- **LOW.** Fields follow 01 §7.4 (private or `#[non_exhaustive]`, with accessors); `RegionKind` is
  replaced by 01's `solid: bool`; the clashing `EvalError` is renamed; one `Side` enum per meaning,
  named apart (`Approach`, `BoundSide`); inversion uses 57 F6's `Inversion`; D11 cites 01 §7.4.
- **Held:** D2, D5, D6, D7 (reconciled with 57 F6), D9 and D11.

### 2.7 NS5 as built: IGES 128 (11-27)

Proposed for review, not yet ruled. scx-nurbs has no
IGES 126 yet, so this sets the shape that 126 should mirror.

- **What the pin has.** `nurbs/iges.rs`: `N_Iges128Srf` takes the entity's counts, degrees,
  knots, weights, points and bounds (not the flags, not a file), clamps the knots to the bounds
  (`N_IgesClampSrf`) and reduces the multiplicities (`N_IgesCorrectSrfKnots`). Only that
  parameter-list to surface mapping is ported; the reverse direction (`to_iges`) is new.
- **Shape.** Module `scx_nurbs_surface::iges`: `Iges128 { m1, m2, s, t, weights: Net<f64>,
  points: Net<[f64; 3]>, u, v }`, `from_iges(&Iges128) -> Result<NurbsSurface<3>, IgesError>`,
  `to_iges(&NurbsSurface<3>) -> Iges128`. `K1`, `K2` are the nets' shape, so a short or ragged
  array cannot be expressed; knot arrays must have exactly `K + M + 2` values (the pin reads a
  prefix of a longer one).
- **Flags `PROP1..5` are not read** (the pin does not take them; writers set them wrongly).
  Polynomial is every weight equal to the first, as the pin; closed and periodic are not
  surface data here.
- **Checks.** Degrees `1..=31`; knot count, finite, non-decreasing; points finite; weights
  finite and positive (no `[1e-3, 100]`); `w P` finite; bounds `U[p] <= U(0) < U(1) <= U[m-p]`
  written to refuse NaN. All `Refused`; an index outside an array in the ported loops is a
  `Defect` (never reached by the difftest pool).
- **Repairs: ruled 11-27 (coordinator), for Brett's review.** scx does not port the pin's
  averaging reduction (`N_IgesCorrectSrfKnots`): it changes the surface, and a result must be
  the surface the data defines. Clamping to the bounds is the pin's, on homogeneous points, in
  its arithmetic order, and keeps the surface on the bounds.
  - An interior knot repeated `p + 1` times is a break (two surfaces): refused through the
    surface's own check (`BadKnots` over `NurbsError::Multiplicity`), Refused. Bounds that leave
    the break outside the domain are fine.
  - An end knot repeated more than `p + 1` times is dropped with its row or column of points,
    exactly: its basis function is zero on the domain, so the points cannot matter (the pin
    keeps the point whose function is zero and drops the next, which is wrong). The drop runs
    first, on the points given, so those kept are exact and rationality is decided on the
    surviving weights. The testkit's exact model takes no such knots, so the unit test
    `surplus_end_knots_drop_exactly` proves it by construction: a surface with its end knots
    repeated once more and rows and columns of arbitrary points added returns as the surface,
    equal exactly.
  - A surface that needs no clamping keeps its points and weights exactly.
- **Round trip.** `from_iges(&to_iges(s)) == s` exactly for clamped `s`. Exceptions, by
  design: a rational surface of all-equal weights returns as its polynomial form; an unclamped
  surface returns clamped to its domain (the same surface there).
- **Adversarial pass.** Porting the averaging as the pin does would make the pair total on
  malformed files, but returns a different surface than the data defines; rejected (ruling
  above). Keeping weights whenever given (so equal weights round-trip) would
  differ from the pin's stored form for no gain in the surface; rejected. `to_iges` refusing
  unclamped surfaces would make the pair total only on clamped data; rejected, since the IGES
  entity allows unclamped knots and `from_iges` defines what they mean.
- **Pin defect `iges_128_surface`** (`pin_defects.rs`): the pin refuses degrees 18 to 31 and
  weights outside `[1e-3, 100]`, reads a prefix of long knot arrays, and builds from NaN
  weights, knots, bounds and points and from overflowing `w P`, and averages away breaks and
  surplus end knots; scx answers by the definition.

