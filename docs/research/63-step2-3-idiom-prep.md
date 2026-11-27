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
`Vec`s (knots, points, weights) the NURBS takes. 9 A3's comparison against one inversion is
deferred to SU4, which builds the inversion; A3's verdict (one conversion per intersection
call, not per evaluation) stands on these numbers.

From research 66: no technique there changes this conversion. Its implicitisation (§8.4) is
an approximate implicit form to seed SSI; our kinds have exact implicit forms already, which
T4's check uses. Its exact predicates (§16) are runtime float filters for orientation tests;
T4's check is exact rationals in the testkit, test-only, so there is nothing to adopt here.
Its "keep derived data, do not rebuild" (§1 point 3) and OCCT's per-boolean intersection
context (§14) match A3: if the conversion ever shows in a profile, memoise it in the caller's
`Scratch`, keyed by the kind's bits and the extent, never on the kind. Its allocation-free
inner loops (§1 point 4) would ask for a caller-provided buffer; not taken, as the conversion
runs once per intersection call and `NurbsSurface` owns its storage.

Open from SU3: the inversion cost comparison (SU4); a `to_nurbs` into a caller's buffer if a
profile ever asks.

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

