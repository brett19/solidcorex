# 77 - Step 5 widening: the fitter, the procedural kinds, surface construction, the volume maps and the ray against NURBS

Written for solidcorex, 2026-11-28. 75 Q1 asked for one note that gives task rows to the five
step-5 pieces about 20 step-7 rows wait on, and 74's SP5 and SP6 wait on two of them. This is that
note. It is a plan: nothing here was built.

The five pieces:

1. **The fitter**: certified approximation of curves (and of pcurves, as the writer's BR9 needs).
2. **The procedural kinds**: revolution, extrusion, offset, sweep and in-volume surfaces.
3. **Surface construction**: compatibility, iso-curves, interpolation, skinning.
4. **The volume maps**: bend, twist and unbend.
5. **The ray against NURBS**: curve against surface where one side is a NURBS.

For each piece the note gives what the pin does (sizes, hazards with file and line), where it goes
in solidcorex, the rules that make its answers certified, and its rows. §7 is the task table, §7.1
the order, §8 the truths, §9 the work counts against the pin, §10 the case against this plan, §11
the open questions and §12 the proposed 04 rows.

- **Labels** (as in 72, 74 and 75): READ (read from the pin's code or ours), MEASURED (counted
  here, method given), JUDGED (a design judgement).
- **Pin paths.** `K/` is the pin kernel's `src/` (the `solidcore` crate). Line numbers are at the
  pin checkout 75 used.
- **Nothing was built or run.** The pin and solidcorex were read with grep, sed and wc only. Every
  cost in §9 is a count of work read from the code, or a figure quoted from an earlier note; none
  is a new timing.

## 1. What exists, and the rulings this plan follows

### 1.1 What solidcorex has at main (READ)

| Crate | Has | Missing for step 5 |
|---|---|---|
| `scx-nurbs` | `KnotVector`, `NurbsCurve<D>` (rational, any dimension) with `point`, `derivs`, `insert_knot`, `split`, `elevate`; `CompatibleCurves` (curves already on one knot vector, edited together, NS3b); `eval_over` over `f64` and `Ivl` | making curves compatible; interpolation; least squares; any fit; a curve-to-curve certificate |
| `scx-nurbs-surface` | `NurbsSurface<D>`, `Net`; evaluation and grids; `insert_knot`, `elevate`, `split`, `reversed`, `swapped`, `transformed` (NS3); enclosures `partials_over_piece`, `partials_over` (NS4); IGES 128 (NS5) | compatibility of surfaces; iso-curve extraction; tensor interpolation; skinning |
| `scx-curve` | `Curve { Line, Circle, Ellipse, Parabola, Hyperbola, Nurbs }`, `Curve2` (with `OnChart`, `AngleLine2`, `AngleWave2`), both `#[non_exhaustive]` | none of its own; the curve side of the procedural kinds is §3.4's question |
| `scx-surface` | `Surface { Plane, Cylinder, Cone, Sphere, Torus, Nurbs }` (`#[non_exhaustive]`, at most 128 bytes); `AnyCurve`; exact `pcurve_of` (never fits); certified `gap` (SU6) on analytic surfaces; `hull` (certified Bézier hulls); `nurbs_invert` (certified branch and bound through `scx_algo::global_solve`); `intersect_curve` for analytic pairs | a `Procedural` arm; the gap on a NURBS surface (`GapError::Pending(NurbsSurface)`, `gap.rs:1027, :1459`); any intersection with a NURBS side (`SurfaceError::NurbsIntersection`, `intersect.rs:158, :161`); the volume maps |
| `scx-bodyq` | the ray cast (`rays.rs`): 13 fixed directions, the tube test of edges | NURBS faces and edges: `Unsupported(Nurbs)` at `rays.rs:274` |

Two things already done make two of the five pieces smaller than 75 assumed:

- **Knot insertion and degree elevation exist** on curves and surfaces (NS3, NS3b). MK11, MK15b and
  MK17 need only compatibility and iso-curves on top (§4).
- **The certified gap exists** for curves against analytic surfaces and for pcurves (SU6). The
  fitter's certificate is that gap at the same parameter; only its NURBS-surface arm is missing
  (§2.4, row FT4).

Two private copies of one second-order interval jet exist (`scx-surface/src/gap.rs:308`, in one
variable; `scx-ssi/src/general/patch.rs:230`, in two). The fitter needs the one-variable jet in
`scx-nurbs`, below both, so FT3 moves it to `scx-num` (READ; JUDGED).

### 1.2 Rulings this plan follows

From 75 §1.2: R2 (no retry), R3 (no tolerance ladders: coincidence is `max(res, acc1, acc2)`), R4
(build exactly, never build then recognise), R5 (choose before building, by a predicate on the
input), R6 (caps become a fixed per-operation budget, and an unproved part is recorded, not
refused), R7 (`Made<R>`/`Done<C>`, no out-params), R8 (no global modes), R10 (no sampled
acceptance). From 04: D10 (row 130: pcurves exact where a closed form exists, else fitted by the
writer before finish, to edge accuracy + face accuracy + G), row 128 (a non-similarity on an
analytic surface gives NURBS whose pcurves step 5 refits), rows 43 and 93 (offset and blend bounds
`Unproven` until certified; a span bound carries its uv region), row 225 (the certified SSI curve
is `Curve::Nurbs` with a `Certified` record; "change if a step-5 case needs C10's procedural arm").
From 01 §4.1: a surface's accuracy lives only in its procedural payload; a span bound may be used
only for a trim whose uv box lies inside the span's region; any edit to the net certifies again.

### 1.3 The certification rule for every row (JUDGED)

Every approximation this note plans returns, for each span it made, a `SpanBound`:

```rust
pub struct SpanBound { pub span: Extent1d /* or Extent2d */, pub bound: Length, pub basis: Basis }
```

- `basis` is `Basis::Certified(Cert)` when the bound is proved (rounding included), and
  `Basis::Unproven(Cert)` when the fixed per-operation cap was reached first: the bound is then the
  one the search reached, and the `Cert` is the one that would prove it. It is never `Measured`.
- No acceptance is decided by samples. A sample may seed a fit (choose where knots go); it never
  decides that a fit is good.
- The cap is a constant of the operation (a span count, a cell count), not the caller's remaining
  budget, so the answer is a function of the input (as 04's loft pairing row). `Budget` and `Stop`
  still bound the work and stop it: running out is `Exhausted`, never a partial answer.
- The caller decides what an `Unproven` span means: the writer records it (`Raise`, R6), a loft
  records it as the face's accuracy, a refusal is never the fitter's choice.

## 2. The fitter

### 2.1 What the pin does (READ)

The pin has three fitters, of very different quality.

- **The solidcore-native certified refit** (`K/nurbs/crv_fit/within.rs`, 956 lines,
  `refit_within`): a clamped spline of a given degree through the source's ends, optionally along
  given end directions, with the fewest spans a halving search finds whose certified distance from
  the source is within the tolerance. Each round fits samples by least squares at their parameters
  (`approx_end_directions`) and certifies the fit span by span
  (`K/nurbs/crv_deviation.rs`, 1,213 lines, `certified_distance_by_span`: both curves cut into
  Bézier pieces over their common breaks, raised to one degree, the bound the largest control-point
  difference, every rounding counted). The span with the largest bound is halved. Its knot choice
  does not depend on the tolerance, so a looser tolerance's knots are among a tighter one's. It
  stops at `most` control points with the last fit past the tolerance. Non-rational 3D B-spline
  sources only. This is the design to port.
- **The SMLib fitting library** (`K/nurbs/crv_fit/` p1-p4 and `end_directions.rs`, 5,185 lines;
  `K/nurbs/srf_fit.rs`, 938; `K/nurbs/crv_approx.rs`, 746; `K/nurbs/fit_arrays.rs`, 304):
  parameterisation (uniform, chord, centripetal), knot vectors (averaging, Piegl and Tiller's
  approximation knots), interpolation with given parameters, with end derivatives, with tangents,
  least squares, weighted least squares, the tensor versions. The algorithms are the NURBS Book's;
  the shapes are C's.
- **The pcurve fitter** (`K/intersect/surface_drop_lib/`, 12,764 lines, `drop_curve`; called by
  `create_uv_trim_curve_fitted`, `K/topology/edgeuse/p3.rs:518`, at `fit_tolerance`, :426): it
  drops the curve's start to the surface, walks each span of the curve's cache with
  `get_next_point`, tests each span with `test_span_accuracy`, subdivides until it passes, and joins
  the passing spans into uv B-splines. A plane section of a cylinder gets its exact rational uv
  curve first (WR-680).

### 2.2 Idiom hazards (READ)

| Where | Hazard | Kind |
|---|---|---|
| `K/intersect/surface_drop_lib/p1.rs:516` | `test_span_accuracy` accepts a span on **three interior samples** lying on surface normal lines through the curve | sampled acceptance (R10) |
| `p1.rs:499-503` | a span shorter than a thousandth of its interval **always passes** | failing default |
| `p1.rs:574-576` | a failed `is_point_on_normal` becomes `(false, 0.0, 0.0)` | swallowed error |
| `p1.rs:446` | `angle_between(..).unwrap_or(0.0)` | failing default |
| `p3.rs:3631-3822` | literal `1e-4` acceptance tolerances in the end-point re-drops | tolerance literal |
| `K/topology/edgeuse/p3.rs:426-470` | `fit_tolerance` reads the edge tolerance, then a cached or freshly computed gap, then caps; the fit's target depends on what set the tolerance before | order-dependent target (research 34) |
| `p3.rs:518` | `opt_drop_curve_fail: Option<&mut DropCurveFail>` | out-param |
| `K/nurbs/crv_approx.rs` (module docs) | upstream leaves a control point uninitialised on parallel end tangents; `MAX_APPROX_SAMPLES` | cap; undefined upstream |
| `K/nurbs/crv_fit/p1.rs:162, :183, :203`, `p2.rs:121`, `p3.rs:105, :126` | `flag() -> i32` on the option enums: integer modes kept for the C tables | integer modes |
| `K/nurbs/crv_fit/within.rs:134` | `most: usize` caps the search; the last fit past the tolerance is returned with no record of which spans are past it beyond the result's fields | cap without a recorded basis |

### 2.3 Where it goes (JUDGED)

| Pin | solidcorex | Row |
|---|---|---|
| `crv_deviation` (`certified_distance_by_span`, `certified_distance_floor`, `interpolation_bound`) | `scx_nurbs::deviation` | FT1 |
| `crv_fit` p1-p4, `end_directions` (interpolation, least squares, end conditions, knot vectors) | `scx_nurbs::fit` | FT2 |
| `refit_within`, generalised to any source curve | `scx_nurbs::fit::within`, with the source a `CurveFn<D>` | FT3 |
| (none: `gap.rs` returns `Pending`) | the NURBS arm of `scx_surface::gap` | FT4 |
| `drop_curve`, `create_uv_trim_curve_fitted` | `scx_surface::fit_pcurve` | FT5 |
| `fit_tolerance`, the writer's eager fit | `scx_brep` writer (BR9), the target D10 states | FT6 |
| `srf_fit` (tensor interpolation and least squares) | `scx_nurbs_surface::fit` | CN3 (§4) |
| (none) `SurfaceFn` approximation | `scx_surface::approximate` (01 §1.1, §7.3) | FT7 |

### 2.4 Proposed shape (JUDGED)

```rust
// scx-nurbs
pub trait CurveFn<const D: usize> {
    fn eval<T: Ranged>(&self, t: T) -> [T; D];      // generic over f64, Ivl and the jets
    fn domain(&self) -> Extent1d;
    fn breaks(&self) -> &[f64];                      // where the source may not be smooth
}
pub struct FitAim { pub within: Length, pub degree: usize, pub ends: Ends }
pub enum Ends { Free, Points, Directions { start: Option<UnitVec>, end: Option<UnitVec> } }
pub struct Fitted<const D: usize> { pub curve: NurbsCurve<D>, pub spans: Vec<SpanBound> }
pub fn fit_within<const D: usize>(source: &impl CurveFn<D>, aim: &FitAim,
    budget: &mut Budget, stop: &Stop) -> Result<Fitted<D>, FitError>;

// scx-surface
pub fn fit_pcurve(edge: &Curve, ce: &Extent1d, s: &Surface, se: &Extent2d, aim: Length,
    budget: &mut Budget, stop: &Stop) -> Result<Fitted<2>, FitError>;
```

- **The certificate** of a span is the same-parameter distance `sup_t |F(t) - C(t)|` over it,
  bracketed by SU6's branch and bound (the chord bound with the jet's second derivative), and, when
  the source is itself a non-rational B-spline, FT1's control-point bound, whichever is smaller.
  Same parameter is stronger than Hausdorff and cheaper (no foot points).
- **A pcurve's certificate** is `pcurve_edge_gap` (SU6): `sup_t |S(p(t)) - C(t)|` at the edge's own
  parameter, which bounds the edge's gap from the face too. The fit is made in uv, but accepted in
  3D, so the aim is a length (D10's sum), never a uv tolerance.
- **The uv source** is `t -> invert(S, C(t))`. On an analytic surface the inverse is the closed
  form (`chart_invert`, periodic unwrap as `pcurve_of` does it, poles through `OnChart`). On a NURBS
  surface it is `nurbs_invert`, seeded by the previous sample, and its certified foot makes the
  sampled source exact enough to seed; the certificate is still the 3D gap.
- **Exact first** (R4, R5): `fit_pcurve` is called only where `pcurve_of` has no closed form. The
  pin's exact cylinder section (WR-680) is already `OnChart`'s.
- **The cap** is a constant span count per call (the pin's `most`); a span past the aim at the cap
  is `Unproven` with its bound (§1.3).
- **`FitError`**: `Refused(BadAim | DirectionAgainstTangent | DomainEmpty)`, `Exhausted`,
  `Stopped`, `Defect(..)`. No `i32` flags; the option enums are plain Rust enums.

## 3. The procedural kinds

### 3.1 What the pin does (READ)

| Kind | Pin file | Lines | How it is stored |
|---|---|---|---|
| Revolution | `K/surface/surf_of_revolution.rs` + p1-p3 | 6,204 | its own kind, generator + axis frame; `make_nurb` builds the exact rational net |
| Extrusion | `K/surface/surf_of_extrusion.rs` + p1, p2, `plane_patch` | 9,733 | its own kind; `make_nurb` |
| Offset | `K/surface/offset_surface.rs` | 4,974 | its own kind, `Base(uv) + d N(uv)` from the base's derivatives to third order; step-off at poles; extension of a B-spline base |
| Screw sweep | `K/surface/procedural_sweep.rs` | 1,363 | **a B-spline face with the definition riding on it** (#2572); the net's bound in closed form (`lift_error_bound`, `offset_error_bound`, rounded intervals, never sampled) |
| Path sweep | `K/surface/procedural_sweep/path.rs` | 1,353 | the same, the profile carried in the path's rotation-minimising frame; the bound an enclosure over up to `MAX_CELLS = 400_000` cells |
| Blend | `K/surface/procedural_blend.rs` | 2,558 | a B-spline net with its definition (step 9's, not this note's) |
| In-volume | `K/surface/srf_in_volume.rs`, `K/curve/crv_in_volume.rs` | 3,348 | its own kind; never constructed outside tests (75 §5.1) |
| Offset curve | `K/curve/offset_curve.rs` | 1,174 | its own kind, base moved along `C' x N` |

The swept and blend surfaces choose between the net and the definition through `NetScope`
(`K/surface/procedural_blend.rs:158`): a scope on the kernel, with a thread-local depth in the fork,
that makes a query read the net when the scope's tolerance is at or above the net's bound and the
definition otherwise.

### 3.2 Idiom hazards (READ)

| Where | Hazard | Kind |
|---|---|---|
| `K/surface/procedural_blend.rs:158` (`NetScope`) | which geometry a query reads depends on a scope entered elsewhere on the kernel | global mode (R8) |
| `K/surface/procedural_sweep.rs` module docs | "an extension, a mirror, an offset and any other new surface keep none" of the definition: it is silently dropped | silent loss of exactness |
| `K/surface/surf_of_revolution/p1.rs:1152, :1155` | `unwrap_or(0.0)` on derived angles | failing default |
| `p1.rs:1458` | `pv_eval.get(i).copied().unwrap_or(Vec3::UNDEF)` | failing default |
| `p1.rs:1172`, `K/surface/surf_of_extrusion/p2.rs:1865` | `let _ = ..set_min_max(..)`: a failed domain set is dropped | dropped status |
| `K/surface/surf_of_extrusion/p2.rs:683, :1000` | `unwrap_or(f64::INFINITY)`, `unwrap_or(0.0)` | failing default |
| `K/surface/offset_surface.rs:2729-2739` | knot access `unwrap_or(f64::NAN)` | failing default |
| `offset_surface.rs:171` | `SS_UNKNOWN: u32 = 16` singularity flags as integers | integer modes |
| `K/surface/procedural_sweep/path.rs:74, :81` | `FITTED_GOAL = 1e-7`, `EXACT_GOAL = 1e-8` absolute goals for the enclosure, tied to `ZONE_TOL_3D = 1e-5` and "parts up to about a hundred mm" | tolerance literals |
| `K/surface/procedural_sweep.rs:98, :101` | `SUP_REL = 1e-3`, `SUP_STEPS = 4096` | caps |

### 3.3 Proposed shape (JUDGED)

```rust
// scx-surface
pub enum Surface { Plane(..), Cylinder(..), Cone(..), Sphere(..), Torus(..),
                   Nurbs(Arc<NurbsSurface<3>>), Procedural(Arc<Procedural>) }
pub struct Procedural { def: Definition, net: Arc<NurbsSurface<3>>, spans: Vec<SpanBound /* uv */> }
#[non_exhaustive]
pub enum Definition {
    Revolution { generator: Curve, axis: Frame },        // PK1
    Extrusion { generator: Curve, along: Vec3 },         // PK1
    Offset { base: Surface, distance: Length },          // PK2
    Sweep { profile: Curve, motion: Motion },            // PK3a (Screw), PK3b (Path)
    InVolume { base: Surface, map: VolumeMap },          // PK4
}
```

- **One arm, one mechanism** (01 C10). The definition is evaluated generically over the scalar
  (`f64`, `Ivl`, the jets), so a query that wants the exact surface gets it with interval
  enclosures for free. The net is a B-spline whose per-span bounds against the definition are
  certified, each with its uv region.
- **No `NetScope`** (R8). A query states its accuracy, as every query already does through
  `Resolution`. The rule: a query reads the net, with the span's bound folded into its coincidence
  (R3: `max(res, acc1, acc2, bound)`), when the bound is within the query's zone; otherwise it reads
  the definition. The choice is per call, from the call's own arguments.
- **The net is a cache that is part of the value.** It is built once when the kind is made and
  never edited; any edit makes a new `Procedural` with new bounds (01 §4.1, 04 row 93).
- **Revolution and extrusion nets are exact.** With the NURBS-compatible parameterisation (01 C10,
  63 D13), revolving or extruding a B-spline or conic generator is a rational tensor product, the
  definition point for point: the bound is the rounding of forming the net only. These two kinds
  exist for what the net loses: the axis or direction (coaxial and parallel tests, the STEP
  `SURFACE_OF_REVOLUTION` and `SURFACE_OF_LINEAR_EXTRUSION` round trip of SP6, the plane-snap and
  pole rules of MK4).
- **Analytic first** (R4, R5). A constructor builds the analytic kind wherever one exists (MK4
  builds the cone, sphere and torus from lines and arcs). `Procedural` holds only what has no
  analytic kind: a revolution of a NURBS generator, or of a line skew to the axis (a hyperboloid).
- **Edits keep exactness.** A rigid motion or a mirror moves the definition and the net together
  (both exactly). A non-similarity is the same rule as 04 row 105: the definition is dropped only
  where the map does not keep it, and the result is a new `Procedural` or a `Nurbs` with a
  certified record, never a silent loss.
- **Size.** The arm is one `Arc`, so `Surface` stays within its 128-byte limit.
- **Offset** (PK2) follows research 55: the interval Taylor model with adaptive bisection, centred
  second order within a C² patch of the base, through the offset formula; where 55's bound does not
  yet hold, the span is `Unproven` (04 rows 43, 93). An offset past the base's least radius of
  curvature on a span is refused with that span (`Refused(SelfIntersecting { at })`), decided by an
  enclosure of the principal curvatures, never by sampling.
- **Sweep** (PK3a, PK3b) ports the pin's two motions with their certificates: the screw's
  closed-form bound and the path's enclosure. The cell cap becomes a constant per operation; the
  absolute goals become the document's resolution.

### 3.4 Row 225 and the curve side (JUDGED)

Row 225 says no procedural arm is needed for the SSI curve in step 5, and asks to change that if a
step-5 case needs it. For **surfaces** three cases do: SP6 (revolution, extrusion and offset kinds
from STEP kept as themselves), MK4b (a revolution of a NURBS generator that must stay coaxial for
later Booleans), and DF2 (faces with no exact image under a map, whose pcurves stay the source's
only if the surface is the exact image). For **curves** none does yet: the SSI curve stays
`Curve::Nurbs` with its `Certified` record (225), and the in-volume and offset curve images are
fitted by FT3 with their record. Q1 and Q2 ask the coordinator to confirm both halves.

## 4. Surface construction

### 4.1 What the pin does (READ)

| Work | Pin | Lines |
|---|---|---|
| Curve compatibility | `K/nurbs/crv_tool/p2.rs:1437-1555` (`make_compatible`, `_knot_tol`, `_adj_knots`) | about 150 of 3,154 |
| Surface knot work, iso-curves, patches | `K/nurbs/srf_tool/p1.rs` (`insert_knot` :237, `extract_iso_crv` :918, `insert_knots` :1069, `extract_patch` :1275) | 1,613 |
| Surface compatibility, join | `K/nurbs/srf_tool/p3.rs:817` (`make_compatible`), `p4.rs:68` (`make_compatible_uv`) | 1,594 |
| Skinning | `K/nurbs/srf_adv/p3.rs:1294` (`create_skin_srf`), `p4.rs:377` (boundary derivatives), `p5.rs:315` (boundary continuity) | 4,881 |
| Coons, Gordon, n-sided | `K/nurbs/srf_adv/p4.rs:258` (`create_coons_srf`), `K/nurbs/gordon.rs` (1,294), `K/nurbs/n_sided.rs` (311) | |
| Swung, translational, scaled sweeps | `srf_adv/p3.rs:1208, :1246`, `p6.rs:260-334` | |
| Loft certificate | `K/nurbs/certificate.rs` | 2,021 |

### 4.2 Idiom hazards (READ)

| Where | Hazard | Kind |
|---|---|---|
| `K/nurbs/srf_tool/p3.rs:810-817` | `make_compatible(sur: &mut [Surface], ..)`: knots of different surfaces within a knot-merge tolerance are **moved to their average** (the geometry changes, unmeasured); on an error "the surfaces are left as far as upstream got" | in-place mutation; partial result; unmeasured edit |
| `K/nurbs/crv_tool/p2.rs:1437, :1555` | `make_compatible_knot_tol(curves: &mut [Curve], knot_tol)`, `make_compatible_adj_knots(.., tol)`: the same knot moves on curves | as above |
| `K/nurbs/srf_adv/p3.rs:1294-1300` | `create_skin_srf(cur: &mut [Curve], cmp: bool, cld: bool, deg, dir, knot_tol)`: two behaviour bools; the input curves are mutated | bool modes; in-place mutation |
| `srf_adv/p3.rs:1311-1326` | closure decided by `curves_are_coincident(.., MODEL_TOL)` on the first and last curves | tolerance test deciding topology |
| `srf_adv/p3.rs:1330` | `Direction::UV` is a run-time `CallFailed` | a type that admits a wrong value |
| `srf_adv/p3.rs:89` | `KNOT_TOL = 1e-6` absolute | tolerance literal |
| `K/primitive/primitive_creation/p2/skin.rs:98` (75 §4.2) | `tol * 1000` | tolerance ladder |
| `K/nurbs/gordon.rs:536, :538` | `REFIT_MOST = 64`, `REFIT_FIRST = 8` crossing refits, doubling | cap |
| `K/nurbs/srf_adv/p4.rs:258` | `create_coons_srf(cur_u: &mut [Curve; 2], cur_v: &mut [Curve; 2])`: corners averaged (`combine2_pts(0.5, ..)`, :268) when the boundaries do not meet | in-place mutation; unmeasured repair |

### 4.3 Proposed shape (JUDGED)

```rust
// scx-nurbs
impl<const D: usize> CompatibleCurves<D> {
    pub fn from_curves(curves: &[NurbsCurve<D>]) -> Result<Self, NurbsError>;   // CN1
}
// scx-nurbs-surface
impl<const D: usize> NurbsSurface<D> {
    pub fn iso(&self, axis: ParamAxis, t: f64) -> Result<NurbsCurve<D>, NurbsError>;   // CN2
    pub fn restrict(&self, uv: &Extent2d) -> Result<Self, NurbsError>;                // CN2
}
pub struct CompatibleSurfaces<const D: usize> { /* one axis's knots and degree shared */ } // CN2
pub fn interpolate(rows: &CompatibleCurves<D>, params: &[f64], degree: usize, ends: &[End])
    -> Result<NurbsSurface<D>, NurbsError>;                                            // CN3
pub fn skin(sections: &CompatibleCurves<3>, params: &[f64], degree: usize, how: Skin)
    -> Result<Skinned, NurbsError>;                                                    // CN4
pub enum Skin { Open { start: End, end: End }, Closed }
pub struct Skinned { pub surface: NurbsSurface<3>, pub through: Length /* certified */ }
```

- **Compatibility is exact.** Domains are mapped affinely to the first curve's (an exact relabelling
  of knots), degrees raised to the highest (`elevate`), the union of knots inserted, and
  non-rational curves given unit weights. No knot is moved: two knots a tolerance apart stay two
  knots. The inputs are borrowed and the result is new, so an error changes nothing.
- **Iso-curves are exact**: insertion to full multiplicity, then the row of the net (the pin's
  `extract_iso_crv` method, not its shape).
- **Skinning interpolates exactly** through the compatible sections at the given parameters, so the
  sections lie on the surface up to the rounding of the solve. `through` is that rounding, bounded
  by FT1's `interpolation_bound` on the net's columns; the surface is the definition (a loft's
  between-section shape is a choice, not an approximation of something else). MK10's span
  certificate bounds the rest.
- **Closure is input** (R5). `Skin::Closed` is chosen by the caller from what it knows (a loft's
  `closed` option, a revolve-like section ring). A `Closed` skin whose first and last sections are
  farther apart than the document's coincidence is `Refused(NotClosed { gap })`, measured by FT1's
  certified distance, never a silent open skin.
- **End conditions** are `End` values (free, tangent, curvature, the pin's boundary derivative and
  continuity skins), one type for curves and surfaces, so MK12's Hermite ends G0-G3 use the same
  solver.
- **Coons, Gordon and n-sided are MK11's** (75), built on CN1-CN3: corners that do not meet are
  refused with the gap, never averaged.

## 5. The volume maps

### 5.1 What the pin does, and its hazards (READ)

75 §5.1-§5.4 cover them: `K/surface/bend_volume.rs` (1,973), `twist_volume.rs` (1,208),
`unbend_volume.rs` (1,671), `srf_in_volume.rs` (1,370), `K/curve/crv_in_volume.rs` (1,978), the
generic `Volume` chain (`surface/volume/p1.rs`, 3,278; `p2.rs`, 2,057); `BSplineVolume` a stub. The
hazards: twelve `unwrap_or` in `volume/p2.rs`, mostly `Vec3::UNDEF`; `bend_volume.rs:178` and
`unbend_volume.rs:177` map an error to `(Vec3::Z, false)`; dropped statuses at
`bend_volume.rs:648, :741, :847`; `EvalOptions` with four behaviour bools (`p1.rs:202`); exactness
decided by a 6×6 sample; inversion by a guess and a point solve. Lessons WR-45, WR-46, WR-59, WR-60.

### 5.2 Proposed shape (JUDGED)

```rust
// scx-surface
pub enum VolumeMap { Bend(Bend), Twist(Twist), Unbend(Bend), Chain(Arc<[VolumeMap]>) }
impl VolumeMap {
    pub fn map<T: Ranged>(&self, p: Point3<T>) -> Point3<T>;            // closed form
    pub fn inverse(&self, q: Point3) -> Result<Point3, MapError>;       // closed form
    pub fn image(&self, s: &Surface) -> Image;                          // decided by kind and frame
}
pub enum Image { Exact(Surface), Procedural /* PK4 */, Refused(MapRefusal) }
```

- **The maps are closed forms and so are their inverses.** Bend sends `(u, v, w)` to
  `(u cos(v/d), u sin(v/d), w)`; its inverse is `(r, d atan2(y, x), w)` with `r = |(x, y)|`, valid
  off the axis and within the bent angle. Twist's inverse is the opposite rotation at the same
  height. No guess and no point solve (the pin's inversion) is needed.
- **The derivatives** are those of the closed form, evaluated over the jets, so WR-45 (third-order
  terms) and WR-46 (the sign of Duw) cannot recur: the chain rule is the scalar type's.
- **Exactness is a predicate** on the face's kind and its frame relative to the map (R4, R10): the
  pin's three bend cases and their unbend inverses, and the twist's (a plane normal to the axis; a
  cylinder or cone coaxial with the axis maps to itself, which the pin misses). Every other face is
  PK4's in-volume kind.
- **Curves** map exactly by the same kind of rule (a line along the bend axis stays a line; a
  radial line maps to a line; a circumferential line to an arc); any other curve image (a line
  parallel to a twist axis is a helix) is fitted by FT3 with its certificate (§3.4).
- **Boxes** are the closed form over intervals, so WR-59 and WR-60 (box bounds and trimming across
  the period) are the interval arithmetic's, not hand-written cases.
- A body meeting the bend axis is DF1's `Refused(MeetsAxis)`; `inverse` on the axis is
  `MapError::OnAxis`.

## 6. The ray against NURBS

### 6.1 What the pin does (READ)

- **The ray tracer** (`K/intersect/ray_tracer.rs`, 2,132 lines): a voxel grid of surface
  subdivision leaves, a 3D DDA walk along the ray, and a local Newton solver per leaf
  (`Lsi3LocalSolver::local_solve`), whose status `fire_ray` ignores while using its outputs
  (module docs, "Out-params"). `set_solver_tolerance` is a setter; tolerances `1e-4`, `1e-8` are
  the tests' constructor arguments.
- **Classification** (`K/topology/shell.rs:1205` `ray_fire`, `:1556` `ray_brep_solve`;
  `K/topology/brep/p9.rs`): rays from three sample points per face use along its normal, or random
  directions from the kernel's seed (ND-1), with a retry on any failed status ("infinite region
  shell retry exceeded its single retry", `p9.rs` near :405); a NURBS hit is the topology solver's
  curve solve against the B-rep.

### 6.2 Idiom hazards (READ)

| Where | Hazard | Kind |
|---|---|---|
| `K/intersect/ray_tracer.rs` (`fire_ray`) | the local solve's status is ignored, its outputs used | swallowed error |
| `ray_tracer.rs` (`set_solver_tolerance`) | a setter on the tracer | setter |
| `ray_tracer.rs:1611` | `point + 0.0001 * shadow_vec` | tolerance literal |
| `K/topology/shell.rs` (`is_inner_shell_of` docs) | random ray directions and dithers from the kernel's seed | non-determinism by seed |
| `K/topology/brep/p9.rs:380-405` | retry on any failed ray status | retry (R2) |

solidcorex's `rays.rs` already fixes the last three for analytic bodies: 13 fixed directions in
general position, no random draws, a ray that decides nothing is `Unclean` and the next direction is
tried. Only the NURBS arms are missing.

### 6.3 Proposed shape (JUDGED)

`intersect_curve` keeps its signature and its `CurveSurfaceHit` answer; its two `NurbsIntersection`
returns are replaced by two arms.

- **RY1, a NURBS curve against an analytic kind.** Each Bézier span of the curve, substituted into
  the kind's implicit form in its own frame, is a polynomial in the span's parameter: degree `2p`
  for a plane's square-free height (degree `p`), a sphere, cylinder or cone (`2p`), and `4p` for a
  torus; a rational curve gives the same degrees in homogeneous form (the weight's powers multiply
  through). Roots are isolated in Bernstein form with a certified bound (the same isolation the
  analytic arms use); a span whose polynomial is zero within its coefficients' bound is an
  `Overlap` piece. Contact (crossing or tangent) is the certified sign of the polynomial's
  derivative at the root's enclosure. This is what `rays.rs`' tube test needs for a NURBS edge
  (the tube is a cylinder).
- **RY2, a line (and any analytic curve) against a NURBS surface.** The surface's Bézier patches
  (`hull`, certified) are culled by the ray's slab, then each surviving patch is solved by Krawczyk
  on `(u, v, t)` through `scx_algo::global_solve` (as `nurbs_invert` is), with `Budget` charged per
  box. A box the solve cannot decide within its fixed depth is returned as undecided, which the
  caller reads as `Unclean` (the ray decides nothing; the next direction is tried). Tangency is the
  certified sign of `n . d`, the same `GRAZE` rule as the analytic hits.
- **RY3** lifts `Unsupported(Nurbs)` in `scx-bodyq`'s cast and classification. It is a small row,
  kept separate because ST3, BW5, HL2 and BO23 all go through it.
- **Deferred: a NURBS curve against a NURBS surface.** No step-6 or step-7 row reaches it outside
  the SSI (SM9 handles faces against faces), so it stays `NurbsIntersection` (Unsupported) by name
  (Q5).

## 7. Task table

Model: Opus where a certificate or a new kind is the work; Sonnet where the work is a port with
closed-form tests. "Unblocks" names the step-6 and step-7 rows (74 §8, 75 §7) each row lifts,
directly or through the rows after it. No row adds a Cargo edge that layers.txt forbids: every row
lives in a crate that already depends on what it calls.

| Id | Crate | Work | Needs | Unblocks | Model |
|---|---|---|---|---|---|
| FT1 | nurbs | `scx_nurbs::deviation`: the same-parameter certified distance of two B-spline curves, by span and whole, its floor, and `interpolation_bound` (port of the pin's native `crv_deviation`, its 20 tests); rational curves through homogeneous points | — | FT3, CN4 (closure gap, `through`), PK1 (net rounding) | Sonnet |
| FT2 | nurbs | `scx_nurbs::fit`: parameterisations, knot vectors (averaging, approximation), interpolation at given parameters with `End` conditions (point, tangent, curvature, G0-G3 Hermite), least squares and weighted least squares, banded solves; enums, not `i32` flags | — | FT3, CN3, MK12 (Hermite ends) | Sonnet |
| FT3 | nurbs, num | `fit_within` on a `CurveFn<D>` source: the pin's halving search, each span certified by SU6's same-parameter branch and bound (or FT1 on a B-spline source), constant span cap, `Unproven` past it; `Jet2` (one variable) moved from `scx-surface` to `scx-num` | FT1, FT2 | FT5, VM2, SF1 (the rest), and through FT5 all below | Opus |
| FT4 | surface | The NURBS arm of `gap`: `curve_surface_gap` and `pcurve_edge_gap` on a NURBS surface (lift `Pending::NurbsSurface`): pieces cut at the surface's knots and the curve's, the bound through `partials_over_piece` (NS4) by the chain rule | — | FT5 on NURBS faces, `cap.rs` NURBS targets (bodyq), CF4's gaps, BO23 | Opus |
| FT5 | surface | `fit_pcurve`: uv source by the closed-form inverse (analytic) or `nurbs_invert` (NURBS), periodic unwrap and poles as `pcurve_of`, fit by FT3 in uv, accepted by `pcurve_edge_gap` in 3D at the edge's parameter; exact pcurves first. Analytic faces with FT3 alone; NURBS faces after FT4 | FT3; FT4 for NURBS faces | BO24, BO13b, SF1, BO23 (stage 5), SP5, FT6 | Opus |
| FT6 | brep | The writer (BR9): fit each missing pcurve before finish to D10's aim (edge accuracy + face accuracy + G), record `Certified` or `Unproven` (`Raise`); the refit after a non-similarity (04 row 128) lifting `Unsupported` under that map | FT5 | SP5, BO23, 04 row 128's transforms, every step-7 constructor that writes a NURBS face | Opus |
| FT7 | surface | `SurfaceFn` and `approximate(f, aim)`: tensor interpolation (CN3) at chosen parameters, each span's bound by research 55's interval Taylor model with its uv region, halving per direction, constant cap; an `f64`-only evaluator gets `Unproven` (01 C10) | CN3, FT3 (the halving rule) | PK2, PK3b, PK4 | Opus |
| CN1 | nurbs | `CompatibleCurves::from_curves`: exact compatibility (affine domains, elevation, knot union, unit weights), borrowed inputs, no knot snapping | — | MK17, CN4, MK11 (Coons) | Sonnet |
| CN2 | nurbs-surface | `iso` (exact, by insertion to full multiplicity), `restrict` to a uv box, `CompatibleSurfaces` along one axis | — | MK15b, MK11 (Gordon) and through it RP6 | Sonnet |
| CN3 | nurbs-surface | Tensor interpolation and least squares at given parameters (rows by FT2 on `CompatibleCurves`, then columns); Bernstein restriction and products for MK10's certificate | FT2 | CN4, FT7, MK10 | Sonnet |
| CN4 | nurbs-surface | `skin(sections, params, degree, Skin)` exact through the sections, `through` certified, `Skin::{Open { start, end }, Closed}`, `Refused(NotClosed)` by FT1's distance; boundary derivative and continuity ends as `End` | CN1, CN3, FT1 | MK10 (and MK10b), MK12 (and MK12b), MK11 (Gordon's network) | Opus |
| PK1 | surface | `Surface::Procedural` with `Definition::{Revolution, Extrusion}`: exact rational nets (rounding bound only), definition over `Ranged`, the per-call net-or-definition rule (§3.3), every `Surface` query's arm (eval, jets, boxes, hulls, invert, gap, intersect, to_nurbs, edit, transform); no `NetScope` | — | MK4b, SP6 (revolution, extrusion), PK2-PK4 | Opus |
| PK2 | surface | `Definition::Offset`: research 55's certified bound, `Unproven` where 55 does not hold yet (04 rows 43, 93), `Refused(SelfIntersecting)` by a curvature enclosure | PK1, FT7 | SP6 (offset), step 10's offset | Opus |
| PK3a | surface | `Definition::Sweep` with `Motion::Screw`: the pin's closed-form bound (`lift_error_bound`, `offset_error_bound`) and radial law ported; its 5 tests | PK1 | MK6 | Opus |
| PK3b | surface | `Motion::Path`: the profile in the path's rotation-minimising frame (MK7a's `frames`), twist and end scale as motion data, the net's bound by enclosure with a constant cell cap and the resolution as goal | PK1, FT7 | MK7b, and through it MK8 | Opus |
| PK4 | surface | `Definition::InVolume { base, map }`: definition through VM1's closed form, net by FT7; the face's pcurves unchanged | PK1, VM1, FT7 | DF2 (and DF3's in-volume truths) | Sonnet |
| VM1 | surface | `VolumeMap::{Bend, Twist, Unbend, Chain}`: closed forms over `Ranged`, closed-form inverses, interval boxes, `image` by kind and frame (§5.2); the pin's map truths | — | DF1, and through it DF2, DF3 | Opus |
| VM2 | surface | Curve images: exact by rule (lines, arcs), else FT3's certified fit | VM1, FT3 | DF1's curved edges, DF2 | Sonnet |
| RY1 | surface | `intersect_curve` for a NURBS curve against the analytic kinds: Bernstein substitution per span, certified roots, contact and overlaps | — | `rays.rs`' tube test on NURBS edges (RY3), BW5, ST3, BO23 | Opus |
| RY2 | surface | `intersect_curve` for an analytic curve against a NURBS surface: hull culling, Krawczyk on `(u, v, t)` through `global_solve`, undecided as `Unclean` | — | RY3, BO23 (stage 6), BW5, ST3, HL2 | Opus |
| RY3 | bodyq | Lift `Unsupported(Nurbs)` in the cast and the classification (`rays.rs:274` and the face hits) on RY1 and RY2 | RY1, RY2 | BO23, ST3, BW5, HL2, `classify_point` on any NURBS body | Sonnet |

21 rows: 13 Opus, 8 Sonnet.

### 7.1 Order and what can start now

**Can start now** (no step-5 dependency): FT1, FT2, FT4, CN1, CN2, PK1, VM1, RY1, RY2.

Ordered by how much waits on them (step-6 and step-7 rows lifted, directly or through later rows):

| Row | Lifts | Count |
|---|---|---|
| FT2 → FT3 → FT5 → FT6 | BO13b, BO23, BO24, SF1, SP5, VM2, FT7 (and so PK2, PK3b, PK4), 04 row 128 | 8 rows + 3 kinds |
| FT1 | FT3's chain above, CN4 | (as FT3) |
| PK1 | MK4b, SP6, MK6 (via PK3a), MK7b and MK8 (via PK3b), DF2 (via PK4) | 6 |
| RY2, RY1 → RY3 | BO23, BW5, ST3, HL2 | 4 |
| CN1, FT2 → CN3 → CN4 | MK10, MK10b, MK12, MK12b, MK11 (Gordon), MK17 | 6 |
| CN2 | MK15b, MK11, RP6 | 3 |
| VM1 → VM2 | DF1, DF2, DF3 | 3 |
| FT4 | FT5 on NURBS faces, CF4, `cap.rs` | 2 + FT5 |

So the first wave, in this order of priority if builders are short: **FT2, FT1, PK1, RY2, CN1, CN2,
RY1, FT4, VM1**. The second wave is FT3, CN3, PK3a, RY3; the third FT5, CN4, FT7, VM2; then FT6,
PK2, PK3b, PK4.

```
FT1 ─┬──────────────┐
FT2 ─┼─> FT3 ─┬─> FT5 ─> FT6 ──> SP5, BO23, BO24, BO13b, SF1
     │        ├─> VM2 ───────────> DF1 (curved edges), DF2
     │        └─────┐
     └─> CN3 ─┬─────┴─> FT7 ─┬─> PK2 ──> SP6 (offset)
CN1 ──────────┼─> CN4 ──────│──> MK10, MK10b, MK12, MK12b, MK11 (Gordon)
              │              ├─> PK3b ─> MK7b ─> MK8
CN2 ──────────│──────────────│──> MK15b, MK11 ─> RP6
FT4 ──────────│──> FT5 (NURBS faces), CF4
PK1 ──────────│──────────────┼─> PK3a ─> MK6
              │              └─> PK4 ──> DF2      PK1 ──> MK4b, SP6
VM1 ──────────┴──────────────────> DF1 ─> DF2, DF3     (VM1 also ─> PK4)
RY1 ─┬─> RY3 ──> BO23 (stage 6), BW5, ST3, HL2
RY2 ─┘
```

The critical path to the most step-7 rows is FT2 → FT3 → FT5 → FT6 (four rows, two of them
Opus-sized certificates). MK17 needs only CN1 and can follow it within a day; MK15b needs only CN2.

### 7.2 Amendments to 01 this plan asks for (not made in this branch)

- §1.1 and §7.3: "`Procedural` lives in `scx-curve`" becomes "the surface `Procedural` arm lives in
  `scx-surface` (its definitions hold surfaces and volume maps); the curve side has no procedural arm
  in step 5" (§3.4, Q1, Q2).
- §7.3 C10's `JetScalar` is `scx_num::trig::Ranged` as built.

## 8. Truths

Every row is judged against closed forms, never against the pin's output alone.

| Row | Truths | Pin tests to port |
|---|---|---|
| FT1 | a curve is no distance from itself, its refinements and its elevations (bound at most the floor); a moved control point bounds the distance by its move; a circle's quadratic rational form against a cubic non-rational fit: the bound at least the closed-form radial error `max ||C(t)| - r|` and within a small factor of it | `crv_deviation.rs:562, :577, :598, :615, :637, :654` |
| FT2 | interpolation of samples of a polynomial of degree at most `p` gives it back exactly (to rounding); end tangents and curvatures met to rounding; least squares of exact data on its own knots gives the curve back | the SMLib fit tests that assert a reproduced curve |
| FT3 | a source of one cubic span is given back in one span; a circle, an ellipse, a helix (closed form) refitted at a ladder of aims: each certified bound holds against the closed-form distance and is within the aim; looser aims' knots are subsets of tighter ones'; two halves of an arc with a shared tangent meet G1 | `within.rs:554, :595, :639, :674, :689, :711` |
| FT4 | a bilinear NURBS patch that is a plane, against a line at height `h`: gap `h`; the rational NURBS sphere against a circle on it: 0 within rounding, against a circle moved out radially by `h`: `h`; a NURBS cylinder's pcurve gap equal to the analytic cylinder's | none (new) |
| FT5 | on a torus, the oblique-plane section (a spiric quartic) fitted to the aim and its gap below it, checked against the closed-form torus distance; a helix on a cylinder fits a degree-1 pcurve in one span (the helix is a line in uv); a circle on a NURBS-written cylinder; a curve through a pole of a sphere | `surface_drop_lib/p3.rs` tests that assert geometry (the straight line 1e-7 above a plane, :4130) |
| FT6 | after a non-similar scale of a body with a cylinder face, every pcurve's certified gap within D10's sum and the mass properties equal the scaled closed form | the pin's transform tests asserting volumes |
| FT7 | `approximate` of a plane, a sphere and a torus written as `SurfaceFn`s: each span's bound holds against the closed-form distance; an `f64`-only evaluator returns `Unproven` | none (new) |
| CN1 | compatible curves evaluate as their sources to rounding at 1,000 parameters each; knot vectors equal; degrees equal | `crv_tool/p2.rs` compatibility tests asserting geometry |
| CN2 | the iso-curve of a NURBS cylinder at constant height is its circle; of a bilinear patch a line; `restrict` agrees with the source on its box | `srf_tool/p1.rs:1550, :1561` |
| CN3 | tensor interpolation of a bilinear or biquadratic polynomial surface's samples gives it back | `srf_fit.rs` interpolation tests |
| CN4 | skin of equal circles at heights on degree 1 is the cylinder; of circles with radius linear in height a cone; of three circles with radii on a parabola law, degree 2, the closed-form surface of revolution; `Closed` with ends apart is refused with the gap | `srf_adv/p4.rs:1445` (Coons of the unit square) for MK11 |
| PK1 | revolution of a line is the cone point for point; of an off-axis circle the torus; of a skew line the hyperboloid `x² + y² - (z tan a)² = r²`; extrusion of an ellipse the elliptic cylinder; net against definition within its rounding bound | `surf_of_revolution/p1.rs:2349, :2398, :2427` |
| PK2 | offsets of a plane, sphere (radius `r + d`), cylinder and torus (minor radius `r + d`) against their closed forms; the bound holds; an offset past the least radius of curvature refused | `offset_surface.rs:3658, :3694` |
| PK3a | the screw of a point is the helix; of a line along the axis the cylinder; the pin's `lift_error_bound` maximum and `offset_error_bound` tightness | `procedural_sweep.rs:1237, :1251, :1272, :1290, :1328` |
| PK3b | a circle swept along a circle in its rotation-minimising frame is the torus; along a line the cylinder | `procedural_sweep/path.rs` tests asserting geometry |
| PK4, VM1, VM2 | the closed-form maps and their inverses round trip; derivatives against finite differences in exact arithmetic; bend ∘ unbend = identity; a plane normal to the axis bends to a plane; a cylinder coaxial with a twist axis maps to itself; a line parallel to a twist axis maps to the closed-form helix | `bend:1567, :1580, :1775`, `twist:1078, :1089`, `unbend:1488, :1500`, `p1:2660, :2918, :3053` (75 §5.3) |
| RY1 | a rational NURBS circle against a plane through its centre: two points at the closed-form angles; a NURBS line tangent to a cylinder: one `Tangent` hit; a NURBS-written circle lying on a sphere: one `Overlap` | none (new) |
| RY2 | a line through the rational NURBS sphere: hits at `|p| = r`; a line tangent to a NURBS cylinder: one `Tangent`; a line along a ruling of a bilinear patch: an `Overlap`; a line through a knot line and a seam: no doubled or lost hit | `ray_tracer.rs` tests asserting hits on surfaces |
| RY3 | `classify_point` on a NURBS-written box, cylinder and sphere body gives the analytic body's answers at the same points, including points within coincidence of faces | none (new) |

## 9. Work counts against the pin

Read from the code (READ) or quoted, with each row's measurement to make before it goes ready (the
rule of measuring added work before a change is ready).

| Row | Pin's work | Ours | Measure before ready |
|---|---|---|---|
| FT5, FT6 | per span: 3 curve evaluations and 3 normal-line tests (`p1.rs:516`), subdivision until pass; spans under 1/1000 skipped | per added span: one least-squares fit and one same-parameter certificate (branch and bound pieces × jet evaluations); research 47 measured a certified trim at 12.8 ms against 69 µs uncertified (185×), 1% of trims taking 73% of the time; SU6 measured its pcurve bound at 12 µs per planar edge and 0.3-32 ms per curved edge | the BO24 torus quartic and SP5's NURBS pcurves on the STEP corpus: fits per edge, certificate pieces per span, time against `drop_curve`; the tail's share |
| FT3 | `refit_within`: one fit and one certificate per added span (READ, module docs) | the same, plus the jet branch and bound on a non-B-spline source | spans and pieces per fit on the FT3 truth curves |
| CN1, CN4 | `make_compatible` plus interpolation; knot snapping can drop knots | the same insertions with no snapping (more knots when sections' knots nearly coincide), plus one `interpolation_bound` (linear in the net) | knots added on MK10's lofts against the pin's snapped count |
| PK1 | `make_nurb` once per surface | the same net once per value, kept with it; queries read it | none beyond the size check (one `Arc`) |
| PK2-PK4 | offset: evaluation from base jets to third order per point; no certificate | the net for every query within its bound (one NURBS evaluation); the definition only for tighter queries; FT7's Taylor model once per surface | FT7's time per span on the PK2 truths; how many queries fall through to the definition on SP6's corpus |
| VM1, VM2 | 36 samples per exactness test; a guess and a point solve per inversion; the domain test copies the volume and builds a curve in it (75 §5.4) | a predicate per face; a closed-form inverse per point | none (strictly less work) |
| RY2 | a voxel grid of subdivision leaves built once per tracer, a 3D DDA walk and a Newton solve per leaf crossed | per ray: a slab test per patch hull, Krawczyk on the survivors; no grid | rays per body and boxes per ray on HL2's and ST3's inputs; if many rays per body lose to the pin's grid, a per-body box tree over patch hulls (scx-algo's) built once per query batch |
| RY1 | a topology-solver curve solve per edge | one Bernstein isolation per curve span, degree at most `4p` | isolations per cast on the RY3 truths |

## 10. Against this plan

| Case against | Weight | Answer |
|---|---|---|
| A `Procedural` arm adds a case to every `match` on `Surface` in every crate | medium | One arm, not five; every query that does not know the definition delegates to the net with the bound folded into its coincidence, a one-line arm. The alternative (five arms, or the pin's attachment) costs more or loses exactness |
| Row 225 ruled no procedural arm in step 5; this reverses it | medium | Only for surfaces, and through 225's own "change if": SP6, MK4b and DF2 need the arm. Curves stay as 225 rules (§3.4) |
| Revolution and extrusion nets are exact, so the kind adds nothing a `Nurbs` with a record lacks | medium | The axis or direction is lost in the net: coaxial tests, STEP kinds and MK4's rules need it. The pin's way (a definition riding on a B-spline) drops it silently on edits (§3.2) |
| Certified fitting is about 185× the uncertified cost (research 47); FT6's eager fit could dominate a STEP import | high | Exact pcurves first, so the fitter runs only where none exists; the fixed cap bounds the tail and records `Unproven` instead of searching on; FT5 and FT6 are measured on SP5's corpus before ready (§9). If the tail stays, the cap is the knob, not the certificate |
| A constant span cap gives `Unproven` spans on hard inputs, which every caller must handle | medium | They already must (R6, 04 rows 43, 93); the answer stays a function of the input |
| FT4 (the NURBS gap) lengthens the path to FT5 on NURBS faces | low | FT5 lands analytic faces first (BO24 is a torus); FT4 starts now in parallel |
| RY2's Krawczyk cannot decide near tangency, so rays come back `Unclean` | low | `Unclean` already sends the cast to the next of 13 directions; all 13 tangent is a measure-zero input, and running out of directions is the existing typed answer |
| Exact compatibility without knot snapping makes near-duplicate knots and ill-conditioned skins | medium | A near-duplicate knot is a cost, not a wrong answer; snapping moves the geometry unmeasured (§4.2). §9 counts the knots added; Q4 asks whether a certified knot removal should follow |
| Closure as input lets a caller skin a ring as `Open` and leave a crack | low | MK10 decides `Closed` from the loft's own option before building (R5); an open skin whose ends meet is a correct open surface, and the Brep's topology (not the skin) closes the loft |
| 21 rows, 13 on Opus, is a lot of step-5 work while step 7 waits | medium | Nine start now; the order in §7.1 puts the four-row critical path first, and MK15b and MK17 follow single small rows |
| `fit_pcurve` in `scx-surface` duplicates `scx-ssi`'s per-span certification of uv curves | low | The SSI certifies curves it traced on two surfaces; the fitter certifies a curve against one. Both use the same gap (SU6); neither calls the other |

## 11. Open questions

**Q1. Does `Surface` gain one `Procedural` arm in step 5?**

- Recommend: yes, `Surface::Procedural(Arc<Procedural>)` with `Definition::{Revolution, Extrusion,
  Offset, Sweep, InVolume}` (blend in step 9), the net and its span bounds; no `NetScope` (the
  per-call rule of §3.3). This amends row 225 for surfaces only.
- Against: store certified `Nurbs` with the definition as an attachment, as the pin does, and add
  no arm: fewer match arms, but edits drop the definition silently and SP6 cannot round-trip the
  STEP kinds.

**Q2. Does `Curve` gain a procedural arm in step 5?**

- Recommend: no. In-volume and offset curve images, and projected curves, are FT3 fits with their
  certified records; the SSI curve stays as row 225 rules.
- Against: DF2's edges and STEP's `OFFSET_CURVE_3D` would then be approximations, not the exact
  images; if SP6's corpus has offset curves on edges that later operations must keep exact, the arm
  is needed. SP6 counts them.

**Q3. Where do `SurfaceFn` and `approximate` live?**

- Recommend: `scx-surface` (as 01 §1.1 and §7.3 say), with the tensor fit itself (CN3) in
  `scx-nurbs-surface`.
- Against: `scx-nurbs-surface` for both, so a NURBS-only user needs no surface kinds; but the
  definitions that implement the trait are all in `scx-surface`.

**Q4. Should a certified knot removal follow compatibility?**

- Recommend: not in step 5. CN1 adds knots exactly; a removal with a certified bound (the pin's
  `remove_knots` with `crv_deviation`) is a later row if §9's count shows ill-conditioned skins.
- Against: MK10's lofts of imported sections may carry many near-duplicate knots, and the pin's
  snapping kept those nets small.

**Q5. Is a NURBS curve against a NURBS surface needed in step 5?**

- Recommend: no; it stays `Unsupported` by name until a row needs it. No step-6 or step-7 row does
  outside the SSI, which SM9 covers.
- Against: `imprint_curves` (BO13b) of a NURBS curve onto a NURBS face may reach it; BO13b goes
  through `fit_pcurve` (FT5) instead, which needs only the inverse.

**Q6. What is the constant span cap?**

- Recommend: one constant per fitter, named and documented, equal to the pin's `most` default
  where it has one, so the corpus behaves as the pin's at the cap; the result records which spans
  are `Unproven`.
- Against: a cap tied to the document's resolution (more spans for finer resolutions) fits large
  parts better; but then two documents' fits of the same curve differ, which the run-twice and
  determinism tests would have to allow.

## 12. Proposed 04 rows (for the coordinator's ruling)

| date | status | decision | why/against | ref |
|---|---|---|---|---|
| 11-28 | Proposed | **Step 5 widening plan.** Rows FT1-FT7 (fitter), CN1-CN4 (construction), PK1-PK4 with PK3a/PK3b (procedural kinds), VM1-VM2 (volume maps), RY1-RY3 (ray against NURBS), 77 §7; first wave FT2, FT1, PK1, RY2, CN1, CN2, RY1, FT4, VM1 (no step-5 dependency); critical path FT2 → FT3 → FT5 → FT6 | Gives 75 Q1's five pieces their rows, ordered by the step-6 and step-7 rows they lift. Against: 21 rows, 13 Opus, while step 7 waits; nine start at once | 77 §7, §7.1 |
| 11-28 | Proposed | **One `Surface::Procedural` arm** holding a `Definition` (revolution, extrusion, offset, sweep, in-volume), its net and per-span certified bounds with uv regions; a query reads the net within its bound and the definition otherwise, decided per call (no `NetScope`, R8); edits make a new value with new bounds. Amends row 225 for surfaces; curves stay `Nurbs` with a record in step 5 | SP6, MK4b and DF2 need exact surfaces; the pin's attachment drops them silently. Against: one more arm in every surface match | 77 §3.3, §3.4, Q1, Q2 |
| 11-28 | Proposed | **Fits carry per-span certificates, never sampled acceptance.** Every fit (curves, pcurves, `approximate`) returns each span's `SpanBound` (`Certified`, or `Unproven` at a constant per-operation cap); the pin's three-sample `test_span_accuracy`, its always-pass short span and its `1e-4` literals are not ported; the pcurve fit is accepted in 3D at the edge's parameter to D10's aim | R10 and D10. Against: the certified path costs about 185× the sampled one on trims (research 47); FT5 and FT6 are measured on SP5's corpus before ready | 77 §2, §1.3, §9 |
| 11-28 | Proposed | **Compatibility and skinning are exact and pure.** No knot snapping or averaging, no in-place mutation, no partial result on error; skin closure is the caller's `Skin::Closed`, refused with the gap if the ends are apart; Coons corners that miss are refused, not averaged | The pin's snapping and averaging move geometry unmeasured. Against: more knots on imported sections (counted, Q4) | 77 §4.3 |
| 11-28 | Proposed | **Volume maps are closed forms with closed-form inverses**; exact images by a predicate on kind and frame (adds the twist's coaxial cylinder and cone); other faces the in-volume procedural kind, other curves certified fits | No point solve and no 6×6 sample. Against: none found beyond the narrow step-7 scope 04 already rules | 77 §5.2 |
| 11-28 | Proposed | **The ray against NURBS**: a NURBS curve against an analytic kind by Bernstein substitution per span; an analytic curve against a NURBS surface by hull culling and Krawczyk through `global_solve`, undecided read as `Unclean`; a NURBS curve against a NURBS surface stays `Unsupported` by name | Certified and deterministic like the analytic arms. Against: many rays per body may want the pin's voxel grid; RY2 measures it on HL2 and ST3 | 77 §6.3, Q5 |
