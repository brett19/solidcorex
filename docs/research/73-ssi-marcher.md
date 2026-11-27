# 73 - Step 5: the general surface-surface intersection

Written for solidcorex, 2026-11-27. Trust: pin READ (both of its SSI implementations, the corpus
report and the crate's history table), design JUDGED, costs INFERRED from the pin's recorded
timings (no new pin measurement was run for this note; row SM11 runs it). Proposed for review.

## 1. The question

`scx-boolean` refuses every pair of faces whose surfaces have no closed-form intersection here:

- stage 4 (72 §14.24, BO2) answers `Closed::General` from `scx_ssi::analytic::intersect` with
  `BooleanUnsupported::SsiPair { a, b, kinds }`, after BO2b's proof that the faces are apart has
  failed (72 §14.30). Any NURBS face is `Unsupported(Nurbs)` before that;
- stage 5 (72 §14.26, BO3) answers `BooleanUnsupported::Pcurve { operand, face }` when a new
  loop needs a pcurve that `scx_surface::pcurve_of` has no closed form for. Two unit spheres at
  the origin and at `(1, 0, 0)` already stop here: their circle `x = 1/2` is exact in 3D, but
  it is neither a parallel nor a meridian of either sphere (`two_spheres_need_a_fitted_pcurve`);
- BO2b's apartness proof is first order (1-Lipschitz level functions bisected to depth 8), so an
  oblique pair that misses by less than about half its smallest box is refused, not dropped.

This note designs step 5's general SSI for NURBS, quadrics and tori. It covers seeds and loop
detection, tracing, the tangent and singular cases, exact pcurves, termination and determinism,
typed errors, and how the result enters SS4's tangency rule and the BO stages. It ends with a
recommendation, an adversarial pass, builder rows, costs against the pin, test truths, open
questions and a proposed 04 row.

## 2. What exists

### 2.1 In solidcorex

| Piece | Where | What it gives step 5 |
|---|---|---|
| analytic SSI | `scx-ssi::analytic` (63 §5, SS1 to SS5) | plane sections of every analytic kind, coaxial sections, two spheres; one depth rule for tangency (SS4); `Intersection { Pieces { curves, points }, Coincident, Empty }`, curves along `n_a x n_b`, `Contact { Transversal, Tangent }` |
| exact pcurves | `scx_surface::pcurve_of` | lines and conics on a plane, rulings, parallels, the cylinder's ellipse (`AngleWave2`), meridians and tube circles (`AngleLine2`); `NoClosedForm` and `CrossesPole` otherwise |
| Bernstein tools | `scx-algo::bernstein` (`Bernstein2`, `bernstein_roots`, `Bounded`) | sign and range enclosures of tensor Bernstein forms |
| interval Newton | `scx-algo::krawczyk` (`krawczyk`, `krawczyk_region`, `ProvedRoot`) | existence and uniqueness proofs in boxes |
| branch and bound | `scx-algo::global_solve` (with `&mut Budget`, `&Stop`) | all roots of a square system over a box, refusing on budget |
| pair search | `scx-algo::box_tree` (`BoxTree::pairs`) | candidate pairs without the all-pairs loop |
| Bézier split | `scx-nurbs::bezier` | pieces by knot insertion, subdivision, degree elevation |
| surface tools | `scx-surface` (`invert`, `nurbs_invert`, `to_nurbs` with the D13 quarter-arc uv, curve-surface `intersect`), `scx-nurbs-surface::enclose` | seeds on boundaries, rational forms of analytic surfaces |
| accuracy records | `scx-brep::record` (`Recorded { acc, basis: Measured, Certified(Cert), Unproven(Cert), Stated }`) | where a certified curve's bound is kept |

What is missing: any curve that is not a line or a conic, any pcurve that is not a closed form,
and any proof that a pair has no loop inside it.

### 2.2 The pin's two implementations

The pin has two SSI codes (`solidcore/docs/ssi.md`).

**The port of SMLib's marcher**, which solidcore calls. Its seeds come from a leaf-pair global
solve, boundary curves against the other surface, and interior seeds only where the normals differ
by at least 5 degrees at the guess and 2 at the solution. Its tracing is a predictor along
`n1 x n2` and a corrector (`refine_point`) solved against a plane through the guess. The span test
looks at 5 interior samples. The step halves on rejection and grows by 1.5 when easy, the first
step is 100 times the approximation tolerance, and there is a 5000-point cap. The output is a C0
cubic B-spline in 3D and both uvs on one knot vector. Seven fixes (WR-74 to 80, 550) patched
tangent contacts, a tangent-curve tracer by Levenberg-Marquardt, singular splits and the pole
tangent. Corpus: 78 of 119 pass (SMLib's own C++: 81). It misses small loops and touch points,
because a seed needs a boundary crossing or a large normal angle.

**The `ssi` crate** (MIT OR Apache-2.0, relicensed by its author, not called by solidcore),
10,861 lines. Corpus: 118 of 119 pass. Median 0.60 ms per case against SMLib 1.0.1's 1.08 ms
(`crates/ssi/README.md`, history table, last row). Its design:

1. Bézier pieces of each patch, made once.
2. Piece pairs halved until they are apart (boxes, or slabs across a normal-cone axis) or
   loop-free (normal cones disjoint, by `acos` with a 1e-9 slack).
3. In a loop-free pair, every curve meets the pair's boundary, so the seeds are the boundary
   curves of each piece against the other piece.
4. The march: predictor along `nA x nB`, a Newton corrector held to a plane across the curve.
   Tolerance 1e-11 times the sine of the angle, maximum step 0.02 of the size, maximum turn
   0.06 rad, link 1e-7, resolution 1e-5, a pair budget of 2,000,000, `Unfinished` after 100,000
   steps, a pad of `1024 eps |x|`, a link floor `8 sqrt(eps |x|)` for grazing seeds.
5. The field path (`trace2d.rs`, the largest file): when one side has an implicit form `f`
   (analytic, or an exact algebraic form to degree 10 found from a composition-matrix null vector
   and proved by composition), `g = f(P(s, t))` is a Bernstein polynomial over the other side's
   piece. Cells are proved by sign, by monotone sweeps, or as holding one critical point (Hessian
   sign plus gradient winding: a touch or a branch point). Tangent curves are valleys of `g`
   (contact order up to 4). Poles of the other surface are divided out. A clear band of 1e-14.
6. Local Taylor fields with proved error bounds (`taylor.rs`) when neither side has an implicit
   form: 17 ms each, tried last.
7. Coincidence from all-zero coefficients (`coincide.rs`), the plane-sphere circle radius as
   `sqrt((R - |d|)(R + |d|))` with a compensated `d` (`quadric.rs`), densification by Hermite with
   Fritsch-Carlson limits (`densify.rs`).
8. Errors `Tangent { at, detail: String }`, `BadPatch(String)`, `Stalled(V3)`, `ExitDiverged`,
   `Unfinished`.

The unsolved case is `y23_fig1d_two_quads` (refused, not wrong).

Neither implementation certifies the B-spline between its points; both test samples. The pin's
documented target (§8 of `docs/ssi.md`) is an interpolation remainder with derivatives enclosed
over the span box, plus a Krawczyk existence and uniqueness proof. Its invariants 1 to 15 hold
here too:

- closed forms first;
- no thresholded samples;
- drop only by proof;
- one knot vector;
- splits at seams and branch points;
- touch points as points;
- no duplicates;
- refusal by name;
- bit-identical output.

### 2.3 Lessons taken from the pin

1. Seeding decides correctness. The marcher scores 78 of 119 and the `ssi` crate 118, and they
   march in much the same way. The difference is that the crate proves where curves can be
   (apart, loop-free) before it marches.
2. The field path is the workhorse. Every analytic surface has an implicit form, and so does a
   torus (a quartic), so every pair with one analytic side reduces to a 2D zero set. A 2D zero
   set can be proved cell by cell, and a 3D march cannot.
3. Closed forms in front pay twice: the plane and sphere pairs got 3 to 4 times faster, and two
   1e-7 loops that the field read as rounding came out right.
4. The tolerances are idiom to remove: `acos` with a 1e-9 slack, `1024 eps` pads, the
   `8 sqrt(eps)` link floor, the 100,000-step cap, the `/64` budget for B-spline pairs, string
   details. Each one becomes outward rounding, a `Budget`, or a typed cause.
5. The Taylor fields cost 17 ms each. Step 5 refuses that case by name first and builds the
   fields only if the corpus or the Boolean cases need them.
6. The pin's patent note (`docs/ssi.md`): US 12,229,885 claims choosing intersection points by
   thresholding distances at sample points. Here a point comes from a Newton or Krawczyk root,
   or from a bracketed root of a Bernstein form, and never from a sample under a threshold.

## 3. Literature and other kernels

- **Sederberg and Meyers 1988** (CAGD 5(2), "Loop detection in surface patch intersections"):
  if the Gauss maps of two patches are disjoint (no normal of one parallel to a normal of the
  other), the intersection inside the pair has no closed loop and no singular point. Each
  component then meets the pair's boundary. Their test uses separating planes of the tangent
  cones and is tighter than disjoint normal cones. **Adopted** as the loop-free test, with cones
  first because they are cheap.
- **Sinha, Klassen and Wang 1985; Kriezis, Patrikalakis and Wolter 1992**: a closed loop inside
  a pair contains a point where the two normals are collinear (a critical point of the distance
  along a common normal). Solving for those points with interval methods finds every small loop
  and touch, and gives each one a seed. **Adopted** as the fallback when the cone test fails, in
  place of halving down to the resolution.
- **Hohmeyer 1992**: Gauss-map tests on the sphere of directions, tighter than cones. Noted as a
  later refinement if cones split too often.
- **Barnhill and Kersey 1990; Grandine and Klein 1997**: marching and its step control, and
  the formulation as a differential-algebraic system. They give the step rules of §4.4.
- **Hoffmann 1990** (the dimensionality paradigm) and **Patrikalakis and Maekawa 2002** (the
  survey): the cases of tangency and singularity; the second fundamental form difference at a
  singular point.
- **Miller and Goldman 1995**: quadric pairs whose intersection degenerates into conics and
  lines. Those are the cases where a marcher meets tangent lines and branch points. **Adopted**
  as closed forms (§4.2).
- **Dupont, Lazard, Lazard and Petitjean 2008** (research 69 I2): exact parametrisation of
  every quadric pair. It is **truth only**: it needs exact arithmetic and radicals, and its code
  is not permissive.
- **Yang, Jia and Yan 2023** (TOG): guaranteed topology for free-form pairs. It is the source of
  the field and critical-cell approach. The corpus's six cases built from its data stay
  excluded until their licence is confirmed (04).
- **Dokken**, approximate implicitisation (GoTools, docs only): it could serve for seeding. It
  is not adopted, because the exact implicit forms of degree 4 and below cover what the corpus
  needs.
- **Moore 1977; Krawczyk**: interval Newton existence and uniqueness, already in `scx-algo`.
- **Fritsch and Carlson 1980**: monotone Hermite limits, used for densifying output.

Other kernels (research 66 §8):

- truck and verb mesh both surfaces and then refine. That is **rejected**, because a loop
  smaller than the mesh is missed.
- OCCT's IntPatch and IntWalk walk with a step control like the pin port's (docs only).
- SISL subdivides with loop detection, which is the shape this note adopts (65 row 5).

65 §5.3's verdict stands: adopt the `ssi` crate's design as the base, keep analytic pairs in
front, and use `BoxTree::pairs` instead of all-pairs tests.

## 4. Design

### 4.1 Pipeline

```
intersect_surfaces(a, ae, b, be, acc, res, budget, stop)
  1. analytic::intersect            closed forms (today's, plus §4.2)        -> done
  2. pieces + pairs                 Bézier pieces, BoxTree pairs, proved apart -> dropped
  3. coincidence                    all-zero field / shared form               -> Coincident
  4. per remaining pair, in order:
       loop-free?  cones, then Sederberg-Meyers, then the critical-point solve
       field path  if either side has an implicit form (every analytic kind)
       march       otherwise
  5. link arcs across pairs, split at seams, poles and singular points
  6. certify each span; emit curves, points and contacts
```

Each stage either answers or hands on a typed reason. No stage returns an empty result in place
of an error.

### 4.2 Closed forms added in front

These pairs are where a general method struggles, because their exact answers have tangent lines
or branch points. Their inputs are common in modelling (fillets, pipes, saddles), so they get
closed forms in `scx-ssi::analytic`, with the existing depth rule deciding tangency:

| pair | answer |
|---|---|
| two cylinders with parallel axes, distance `e` | two lines, one tangent line (`e = r1 + r2` or `|r1 - r2|`), coincident (`e = 0`, equal radii), or empty |
| two cylinders of equal radius whose axes meet | two ellipses in the bisecting planes, crossing at two points (branch points as points) |
| cylinder or cone pairs sharing an inscribed sphere (Miller-Goldman) | two conics in planes through the tangent circle's chord, meeting at two points |
| a torus and a bitangent plane through its centre at `sin t = r/R` | two Villarceau circles of radius `R`, meeting at the two tangent points |
| a sphere and a cylinder, cone or torus with its centre on the axis | already coaxial (SS3) |

These are found by tests on the frames and radii, within the tolerance, as SS3's axis test is.
Each must also say `General` when its near case lies past the tolerance, so the general path sees
the near-degenerate pair.

### 4.3 Pieces, pairs and the proof of apartness

- A NURBS face splits into Bézier pieces once (`scx-nurbs::bezier`). An analytic face uses its
  rational form from `to_nurbs` (D13 quarter-arc uv) only for enclosures. Points are always
  evaluated and refined on the exact analytic surface.
- `BoxTree::pairs` gives the candidate pairs of the two faces' piece boxes, grown by the
  coincidence.
- A pair is dropped only by proof:
  1. its boxes are disjoint;
  2. it is apart across the slab of a normal-cone axis;
  3. with an implicit side, the field `g = f(S_B)` has one strict sign over the piece (every
     Bernstein coefficient the same sign, with the rounding bound of the coefficients' formation
     added outward).

  This replaces the pin's `1024 eps |x|` pads with bounds computed from the arithmetic.
- A pair that is neither apart nor loop-free is halved on its longer parameter side. If a piece
  falls below the resolution without being decided, the pair is refused as `Unresolved { at,
  size }`. That is the one place where the resolution enters as a size.
- BO2b stays in front as stage 4's cheap filter (3 to 7 µs on pairs that touch). It never gives
  answers, only drops pairs that are proved apart.

### 4.4 Loop-free pairs and seeds

**Normal cones**:

- Each piece's normal cone is the axis and half-angle of an enclosure of its normals, from the
  Bernstein form of `S_u x S_v`, rounded outward.
- The pair is loop-free when the cones of `nA` and of both `nB` and `-nB` are disjoint.
- The test compares cosines, `axisA . axisB` against `cos(alphaA + alphaB)`, with outward
  bounds. It does not use `acos` with a slack.

**Sederberg-Meyers**: when the cones overlap, this tighter sufficient test runs before any split.
It looks for two planes, one through each cone, that separate the tangent planes.

**The critical-point solve**:

- When a pair is still undecided after two or three halvings, `global_solve` runs on the system
  `nA(u, v) x nB(s, w) = 0`, `(SA - SB) x nA = 0` (four independent equations in four unknowns
  after choosing two components).
- No root proves the pair loop-free, and seeds come from its boundary.
- Each root proved by Krawczyk is a collinear-normal point at a signed distance `d`:
  - `|d|` greater than the coincidence: it rules out loops through that point only;
  - `|d|` within the coincidence: a touch point or a singular point of the curve, classified in
    §4.6;
  - otherwise: a loop around the point, seeded by marching out from it along the minor axis of
    the difference of the second fundamental forms.
- This finds a 1e-7 loop without halving the pair 20 times, and it is how the pin crate's
  `c7_tiny_loop_*` cases are met without a closed form.

**Seeds in a loop-free pair**:

- Each piece's four boundary curves are intersected with the other surface by the curve-surface
  intersection in `scx-surface`.
- Each root is a Krawczyk-proved point, never a thresholded sample.
- Seeds are kept in a fixed order: by pair, then boundary side, then parameter.
- A seed on a boundary shared by two pairs is the same seed, found once by the lower pair index,
  so arcs link exactly.

### 4.5 Tracing

**The field path** applies when either side is analytic or has a proved implicit form. It is the
default for quadrics and tori.

- `g(s, w) = f_A(S_B(s, w))` in rational Bernstein form over B's piece. The degree is
  `deg f_A x deg S_B` per variable: 4 for two quadrics, 8 for a torus against a quadric. Which
  side is `f_A` is decided by the lower degree and then by operand order, so the result is fixed.
- Cells are proved by sign (no curve), by a monotone gradient component (the curve is a graph
  crossing the cell once per branch), or as holding one critical point. A critical point is
  classified by the sign of the Hessian: definite with `g` clear of zero means no curve; definite
  with `g` within the band means a touch point; indefinite with `g` within the band means a node,
  so two branches crossing.
- The zero set is traced in B's uv with Newton on `g` alone (one equation in two unknowns, with a
  cross line). The 3D curve is `S_B(q(t))`, and the pcurve on A is A's chart inverse (§4.7).
- Tangent curves are valleys: `g >= 0` everywhere with `min g` inside the band along a curve where
  `grad g = 0`. Contact of order 2 comes first; orders 3 and 4 (the pin's later rows) are a
  follow-up.
- Poles of B (a sphere or cone's apex in its NURBS form) are divided out of `g` as the pin does.

**The march** is for NURBS against NURBS with no implicit form (or one of degree above 4):

- Predictor along `t = nA x nB / |nA x nB|`. The corrector is Newton on the 4-unknown system
  `SA(u, v) - SB(s, w) = 0` plus `(x - x0) . t0 = h`, the plane across the curve at the step
  length.
- Step control by three bounds, each an analytic estimate and not a sample count:
  1. turn: `h kappa <= theta_max`, with `kappa` from the curve's second derivative (the normal
     curvatures of both surfaces along `t`);
  2. chord deviation: `kappa h^2 / 8 <= acc`;
  3. conditioning: the corrector's tolerance scaled by `sin(angle(nA, nB))`.

  A rejected step halves. If `h` falls below `res` the march refuses as `Stalled { at, step }`.
- An arc ends where it leaves the pair through a seed (linked), returns to its own start (a
  loop, only possible in a pair proved to contain one), or reaches a singular point found in
  §4.4.
- Arc length inside a pair is bounded by the pair's box diagonal times a factor from the
  normal-cone separation, so the step count is bounded. The `Budget` is charged one unit per
  step and per pair, with no fixed cap.

### 4.6 Tangent and singular cases

| case | found by | answer |
|---|---|---|
| isolated touch (spheres tangent, plane on a torus top) | a critical point with a definite relative form, `|d|` within coincidence | `IntersectionPoint`, `Contact::Tangent`, at the deepest point (SS4's rule) |
| node: two branches crossing (Steinmetz, Viviani, Villarceau) | a critical point with an indefinite form `II_A - II_B` and `|d|` within coincidence | a point that is a vertex; four arcs leave along the null directions of the form |
| tangent curve (cylinder on cylinder along a line, sphere in a cylinder) | a field valley, or a closed form | `IntersectionCurve` with `Contact::Tangent` |
| contact of higher order (tacnode, cusp, a degenerate form) | the form is singular within its bound | refused as `Singular { at, kind }` |
| coincident patches | all-zero field coefficients, or a shared form | `Intersection::Coincident` (SS4) |

The topology claim is made **up to the coincidence**, as SS4's depth rule already makes it. Two
loops passing within the tolerance of each other are a node, and a gap within it is a touch.
Truth for the closed-form tests is exact. A test perturbed below the tolerance expects the
tolerance's answer, and the truth tables say so.

At every node the three classic cases have indefinite relative forms:

- Steinmetz at `(0, R, 0)`: `diag(1/R, -1/R)`;
- Viviani at `(2a, 0, 0)`: `diag(1/(2a), -1/(2a))`;
- Villarceau at the tangent points: the torus's own indefinite form.

So all three are ordinary nodes, not refusals.

### 4.7 Pcurves

Two different needs:

**(a) Exact 3D curves on analytic surfaces.** This covers the two spheres, an oblique cone
section, an off-axis circle on a sphere and a plane's ellipse on a torus. A new exact kind is
proposed:

```rust
/// The uv of a 3D curve on an analytic surface, by that surface's closed-form chart
/// inverse: exact, at the edge's own parameter (04 11-27).
Curve2::OnChart(OnChart { curve: Curve, chart: Chart, turn: [i32; 2] })
```

- `Chart` is a small value in `scx-curve` with a kind, a frame and radii (plane, cylinder, cone,
  sphere, torus).
- The uv is `chart.inverse(curve.eval(t))`. Its derivatives come by the chain rule from the
  curve's.
- The angle branch is the integer turn count at the span's start, unwrapped by continuity. A
  curve through a pole, an apex or a torus's inner seam is split there first (`CrossesPole`
  stays the rule).
- It is exact, cheap (one `atan2` and one `asin` or `hypot` a point), keeps the edge's
  parameter, and removes the two-sphere `Pcurve` refusal outright.

**(b) Marched and field curves.**

- The march and the field trace already produce both uvs. The curve is stored as three B-splines
  on one knot vector: 3D, uv on A, uv on B (pin invariant 4).
- On an analytic side, the uv B-spline can instead be `OnChart` of the 3D B-spline, which is
  exact against the 3D curve and removes one certificate. This is chosen when the side is
  analytic.
- On a NURBS side, the uv B-spline is certified (§4.8).

(c) Reframing a sphere so its circle becomes a parallel is rejected: it rewrites geometry the
Boolean did not change.

### 4.8 Certification

A marched or field curve claims, on every knot span `[t_i, t_{i+1}]`:

1. `|c(t) - S_A(p(t))| <= eps` and `|c(t) - S_B(q(t))| <= eps`, for all `t`;
2. a unique true intersection point lies within `eps / sin(theta_min)` of `c(t)` on the cross
   plane at `t`.

The proof:

- the residuals are enclosed by interval Bernstein or mean-value forms of the compositions over
  the span, split until they are under `eps`;
- **parametric Krawczyk** proves the second claim: Krawczyk on the cross-plane system with `t`
  as an interval over the span and the unknowns in a box of that radius about `(p(t), q(t))`.
  One proof covers the whole span, not samples.

At the span's end points the Krawczyk roots are the knots' points.

The proof is recorded as `EdgeGeom::accuracy = Recorded { acc: eps, basis: Certified(cert) }` and
likewise on each use. A span that cannot be certified is split. If it cannot be split further it
is refused as `Uncertified { at }`; it is never kept with an `Unproven` basis by default.

### 4.9 Termination and determinism

**Termination.** Every loop is bounded:

- subdivision stops at the resolution (then `Unresolved`);
- `global_solve` uses the caller's `Budget`;
- marching steps are at least `res` long, inside a bounded arc;
- certification splits stop at `res`.

Every unit of work is charged to `&mut Budget` and checks `&Stop` per pair. There are no fixed
caps, so the pin's `Unfinished` after 100,000 steps and the `/64` budget for B-spline pairs go.

**Determinism.** Bit-identical across runs and platforms:

- pairs in `BoxTree::pairs` order, re-sorted by piece indices;
- seeds sorted by (pair, side, parameter);
- arcs linked by seed identity, not by a distance search;
- no hash-order iteration;
- every function is `fmath` (correctly rounded) and there is no fused contraction.

The output is independent of the order in which pairs are examined, because each pair's answer
depends only on the pair.

### 4.10 Typed errors

```rust
#[non_exhaustive]
pub enum SsiError {
    /// A pair neither proved apart nor loop-free at the resolution.
    Unresolved { at: Point3, size: Length },
    /// The march could not step at the resolution.
    Stalled { at: Point3, step: Length },
    /// A contact this method has not built: a tacnode, a cusp, contact of order above 2.
    Singular { at: Point3, kind: SingularKind },
    /// A span whose certificate could not be closed at the resolution.
    Uncertified { at: Point3 },
    /// The caller's budget ran out, or it was stopped.
    Budget(Exhausted),
    Stopped,
}
```

- A surface that cannot be read is already typed upstream (`scx-nurbs`), so the pin's
  `BadPatch(String)` has no arm here.
- `ExitDiverged` does not exist, because exits are seeds proved in advance.
- `scx-boolean` maps each cause into `SsiPair { a, b, kinds, cause }`, so a refusal names its
  reason.
- There is no `Option` where failure is meant, and no empty answer in place of a refusal.

### 4.11 The API and the plug-in points

- `scx_ssi::intersect(a, ae, b, be, [Accuracy; 2], Resolution, &mut Budget, &Stop) ->
  Result<Intersection, SsiError>`. It calls `analytic::intersect` first and the general path on
  `General`. `IntersectionCurve.curve` gains a kind for the certified curve (three B-splines and
  the certificate), next to the exact `Curve`.
- **SS4**: the depth rule and `Contact` carry over unchanged. Touch points and tangent curves from
  §4.6 are given at the deepest point, as the analytic pairs give them.
- **Stage 4 (BO2)**: `General` no longer refuses. It calls the general path. BO2b stays as the
  filter in front. The splitting of curves at merged points through both faces works on any
  curve, because it uses the curve's evaluation and the face tests.
- **Stage 5 (BO3)**: `exact_loops` accepts `OnChart` and the certified uv B-splines. The
  `Pcurve` refusal remains only for a NURBS face crossed by an exact curve that no march
  produced. That case gets its uv by marching the curve against the face, the same corrector
  with one surface.
- **The brep**: `EdgeShape::Curve` holds `Curve::Nurbs` for the certified 3D B-spline, with the
  certified accuracy recorded. The definition (the two faces) is recoverable from the edge. 01
  C10's procedural `AnyCurve` arm is not needed for step 5 (open question 1).
- **BO5**: the expected-`Unsupported` tests (tpipe, cylinder and sphere on and off the seam,
  torus and sphere, torus and box) turn into results, judged against the truths of §9 and the
  pin's volumes. The 04 row for BO5 changes with them.

## 5. Recommendation

1. Add the Miller-Goldman degenerate pairs and the Villarceau planes as closed forms (§4.2).
2. Add the exact `Curve2::OnChart` pcurve (§4.7a). This removes every `Pcurve` refusal for exact
   curves on analytic surfaces, two spheres included, before any marching exists.
3. Port the `ssi` crate's design as idiomatic Rust in `scx-ssi::general`:
   - Bézier pieces through `BoxTree::pairs`;
   - proofs of apartness with outward rounding;
   - loop-free by certified cones, then Sederberg-Meyers, then the critical-point solve;
   - the field path whenever a side is analytic (every BO pair today);
   - the march for NURBS against NURBS;
   - Taylor fields deferred, refused as `Unresolved` for now.
4. Certify every span by residual enclosure plus parametric Krawczyk, recorded as
   `Basis::Certified`.
5. Typed `SsiError`, `Budget` and `Stop` termination, fixed orders. BO2b stays as the filter in
   front.

Order: rows SM0 to SM2 unblock two spheres and the degenerate quadric cases at once. SM6 (field
path) unblocks every analytic Boolean in BO5. SM7 (march) is needed only once NURBS faces enter
the Boolean.

## 6. Adversarial pass

| Case against | Weight | Answer |
|---|---|---|
| Port the SMLib marcher, as solidcore did: it is the pin and has years of fixes | strong for parity | It scores 78 of 119 and misses small loops and touches by design (interior seeds need a 5 degree normal angle). Truth over pin: the crate's 118 is the better base. The marcher's step control still informs §4.5. |
| Mesh then refine (truck, verb): simple, fast, robust on big curves | medium | It misses loops below the mesh size, with no way to know. Rejected (research 66). |
| The field path alone, never a march: one method, provable | medium | NURBS against NURBS without an implicit form of degree 4 or below has no field. The pin needed exact forms to degree 10 or 17 ms Taylor fields for those. The march stays for them. |
| Exact algebraic parametrisation for every quadric pair (Dupont 2008): exact topology | medium | It needs exact arithmetic on floating input, radicals, and code that is not permissive. Used as the truth for tests, not at run time. |
| `OnChart` is not a B-spline: tessellation, export and STEP want NURBS pcurves | real | Tessellation evaluates it like any `Curve2`. STEP export fits a B-spline at export with a certified bound, paid only on export. The alternative (fit at the Boolean) pays on every Boolean and loses exactness. |
| A new `Curve2` kind needs chart maths in `scx-curve`, below `scx-surface` | real | The chart inverse is closed-form trigonometry on a frame (a few dozen lines). Putting a `Chart` value in `scx-curve` keeps the layering. The other choice, an `AnyCurve2` in `scx-surface`, is open question 2. |
| Certifying every span costs time the pin never spends | real | Estimated at 20 to 40 percent over the uncertified march (§8). The record is the contract (Certified versus Unproven). A measured cost above 2x the uncertified path reopens this. |
| A node claimed within the tolerance may truly be two loops 1e-17 apart in the input doubles | real, small | The same rule SS4 applies to tangency. The claim is up to the coincidence, and the truth tables state the tolerance's answer for perturbed cases. |
| The critical-point system is singular at tangent contacts, so Krawczyk cannot prove its roots | real | Such pairs fall back to halving, then to `Unresolved` or `Singular`, named. Nothing is dropped. |
| The closed forms of §4.2 multiply special cases | medium | Each is a frame-and-radius test of the kind SS3 already does. Without them the general path meets its hardest inputs (branch points and tangent lines) on the most common modelling shapes. |
| BO2b and SM4's apartness proofs overlap | low | BO2b costs microseconds and drops most far pairs before pieces are built. SM4 proves what BO2b cannot (second order, field sign). Both stay, and BO2b gains nothing new. |
| Deferring the Taylor fields leaves `y23_fig1a`-class cases refused | low | Those are free-form pairs of degree 8 whose data is excluded. They are refused by name until the corpus or a model needs them. |

## 7. Builder rows (§11 style)

```
SM0 (truths) ──┬─> SM2 (closed forms)
               │
SM1 (OnChart) ─┼──────────────────────────────────────┐
SM3 (API, errors) ─> SM4 (pieces, apart) ─┬─> SM5 (loop-free) ─> SM7 (march) ─┐
                                          └─> SM6a (field) ─> SM6b (critical) ─┤
                                                                SM8 (certify) <┘
SM10 (corpus scorer, after SM3) ............................. runs as rows land
SM9 (BO integration: after SM1, SM6b, SM8) ─> SM11 (cost against the pin)
SM12 (deferred: Taylor fields, implicit forms of NURBS)
```

| Task | Crate | What | Depends on | Model |
|---|---|---|---|---|
| SM0 | testkit | the closed-form truths of §9: curves with residual and topology, Boolean volumes and areas, perturbed variants with the tolerance's expected answer | — | Sonnet |
| SM1 | curve, surface | `Curve2::OnChart` with `Chart`, its evaluation and derivatives, turn tracking; `pcurve_of` returns it for every exact curve on an analytic surface that does not cross a pole or apex; BO3's two-sphere test turns into a result | — | Opus |
| SM2 | ssi (`analytic`) | §4.2's closed forms: parallel cylinders, equal cylinders with meeting axes, Miller-Goldman conic pairs, Villarceau planes; branch points as points; `General` past the tolerance | SM0 | Opus |
| SM3 | ssi | the `intersect` API with `Budget` and `Stop`, `SsiError`, the certified-curve kind in `IntersectionCurve`, stubs that refuse by name | — | Opus |
| SM4 | ssi (`general`) | Bézier pieces, `BoxTree::pairs`, apartness by boxes, slabs and field sign with outward rounding, halving to `Unresolved`, coincidence from zero fields | SM3 | Sonnet |
| SM5 | ssi | certified normal cones, the Sederberg-Meyers test, the critical-point solve with classification; boundary seeds by curve-surface roots in a fixed order | SM4 | Opus |
| SM6a | ssi | the field path: implicit forms of the analytic kinds (torus quartic included), `g` in rational Bernstein form, sign and monotone cells, the trace in uv, the chart inverse for the other side | SM4 | Opus |
| SM6b | ssi | critical cells (touch, node), valleys of order 2 (tangent curves), poles divided out | SM6a, SM5 | Opus |
| SM7 | ssi | the march: corrector, step control by turn, deviation and conditioning, linking, nodes from the relative form, `Stalled` | SM5 | Opus |
| SM8 | ssi | output on one knot vector (Hermite, Fritsch-Carlson), residual enclosures, parametric Krawczyk per span, `Basis::Certified`, `Uncertified` | SM6a, SM7 | Opus |
| SM9 | boolean | stage 4 calls `scx_ssi::intersect` on `General`, `SsiPair` carries the cause; stage 5 takes `OnChart` and certified uv; the BO5 tests turn into results judged on SM0's truths; 04 BO5 updated | SM1, SM6b, SM8 | Opus |
| SM10 | ssi (tests) | the ssi-corpus runner and scorer on the permissive cases (the six built from Yang data excluded), verdicts against truth | SM3 | Sonnet |
| SM11 | bench | per-pair cost against the pin crate and the pin's Boolean, with side-channel timers, on the corpus and the BO5 cases; no committed counters | SM9, SM10 | Sonnet |
| SM12 | ssi | deferred: Taylor fields, exact implicit forms of NURBS patches of degree 4 or below, contact of order 3 and 4 | SM8 | Opus |

SM0, SM1 and SM3 can start at once. SM1 alone clears the two-sphere refusal.

## 8. Expected cost per pair against the pin

From the pin's recorded timings (the crate's README history and research 72 §14.26, §14.30). The
estimates are labelled as estimates. Row SM11 measures them.

| pair class | pin | expected here | basis |
|---|---|---|---|
| closed-form analytic pair (today's and §4.2) | the crate's `quadric.rs` and plane pairs, about 0.15 to 0.2 ms (3 to 4 times under its 0.60 ms median); the port marches them | 1 to 10 µs (SS1 to SS5 are closed forms; BO2b 3 to 7 µs) | measured for SS1-SS5 and BO2b; the new forms are the same shape |
| exact curve's pcurve on an analytic face (two spheres) | the port fits one in the march | under 1 µs per evaluation (`OnChart`), nothing up front | inferred |
| quadric against quadric, general (sphere-cylinder, tpipe) | crate median 0.60 ms per case | 0.2 to 0.6 ms: degree-4 field, the same cells, plus certification | inferred from the crate plus §4.8 |
| torus against quadric or plane (spiric, torus-sphere) | inside the crate's 0.60 ms median; field of degree 8 | 0.5 to 2 ms | inferred |
| NURBS against NURBS, marched | SMLib 1.08 ms median, crate 0.6 to 2 ms | 1 to 3 ms with certification (20 to 40 percent over uncertified) | inferred |
| free-form with no implicit form | crate Taylor fields, 17 ms each (3.8 s on `y23_fig1a`) | refused (`Unresolved`) until SM12 | by design |
| the two-sphere union, whole Boolean | pin 31.8 ms (22.0 ms in `intersect_insert_relate`) | SSI part microseconds; the Boolean bounded by stages 5 to 7, not by SSI | measured pin; inferred here |

Watch: the field path's degree 8 for tori is the expected hot spot. If SM11 measures more than
2 times the crate on torus pairs, the torus's coaxial and plane closed forms should take more of
them, and the field should be built on the lower-degree side.

## 9. Test truths

All of these are exact. A test checks:

- the residual of each output point on both surfaces;
- the Hausdorff distance to the closed form in both directions;
- the topology (number of curves, closed or open, the points);
- for a Boolean, the volume and area.

**Sphere and sphere.** Centres `c1`, `c2`, radii `r1`, `r2`, `d = |c2 - c1|`.

- `a = (d^2 + r1^2 - r2^2) / (2d)` from `c1` along the axis; the circle's radius is
  `rho = sqrt((r1 - a)(r1 + a))`.
- It meets when `|r1 - r2| < d < r1 + r2`. Equality gives a touch point; `d = 0` with
  `r1 = r2` gives coincidence.
- Unit spheres at `0` and `(1, 0, 0)`: the plane `x = 1/2`, `rho = sqrt(3)/2`. The circle stays
  clear of both spheres' poles, so `OnChart` serves on both faces.

**Perpendicular cylinders** (`x^2 + y^2 = r^2`, `y^2 + z^2 = R^2`, `r <= R`):

- `x = r cos t`, `y = r sin t`, `z = +/- sqrt(R^2 - r^2 sin^2 t)`: two closed loops for `r < R`.
- At `r = R` (Steinmetz) the loops become two ellipses in the planes `z = x` and `z = -x`,
  crossing at `(0, +/-R, 0)`: two nodes.
- Bicylinder volume with modulus `k = r/R`: `V = (8/3) R [ (R^2 + r^2) E(k) - (R^2 - r^2) K(k) ]`.
  At `r = R` this is `16 R^3 / 3`, and the area is `16 R^2`.
- At `r = 1.5`, `R = 2` (the tpipe radii): `V = 26.1131656` (checked by quadrature), and each
  loop is 9.887974 long.
- The pin's tpipe union volume is 308.9567. It is the pin's figure, to be re-derived in SM0 from
  the lengths and the bicylinder volume.

**Skew perpendicular cylinders** (the second axis offset by `e` across both axes,
`(y - e)^2 + z^2 = R^2`, `r < R`): `z = +/- sqrt(R^2 - (r sin t - e)^2)`. There are two loops
when `e < R - r`, a node at `e = R - r`, one loop when `R - r < e < R + r`, and a touch at
`e = R + r`.

**Parallel cylinders**, axes `e` apart:

- two lines at `x = (e^2 + r1^2 - r2^2) / (2e)` along the axes;
- one tangent line at `e = r1 + r2` (`Contact::Tangent`);
- coincident at `e = 0` with equal radii.

**Sphere and cylinder** (sphere radius `rho` at distance `e` from the axis of a cylinder of radius
`r`): `x = r cos t`, `y = r sin t`, `z = +/- sqrt(rho^2 - r^2 - e^2 + 2 r e cos t)`, with the
sphere centre on the x axis.

- Viviani: `e = r = a`, `rho = 2a`. The curve is `x = a(1 + cos t)`, `y = a sin t`,
  `z = 2a sin(t/2)` for `t` in `[0, 4 pi)`, a figure eight with a node at `(2a, 0, 0)`.
- The pin's BO5 case has the sphere centre 5 from the axis.

**Torus and plane.** Torus `(x^2 + y^2 + z^2 + R^2 - r^2)^2 = 4 R^2 (x^2 + y^2)`.

- `z = h`: circles of radius `R +/- sqrt(r^2 - h^2)`, already coaxial; a tangent circle at
  `|h| = r`.
- Plane `y = d`, parallel to the axis (spiric sections,
  `(x^2 + d^2 + z^2 + R^2 - r^2)^2 = 4 R^2 (x^2 + d^2)`):
  - `d = 0`: two tube circles;
  - `0 < d < R - r`: two ovals;
  - `d = R - r`: a figure eight with a node at `(0, R - r, 0)`;
  - `R - r < d < R + r`: one oval;
  - `d = R + r`: a touch point.
- A bitangent plane through the centre at `sin t = r/R`: two Villarceau circles of radius `R`,
  centres `r` from the torus centre, meeting at two nodes.

**Coaxial pairs.** Already solved and tested (SS3). They stay as regression truth.

**Tiny loops.** A plane at depth `delta` into a sphere of radius `R` gives a circle of radius
`sqrt(delta (2R - delta))`. With `delta = 1e-7` it is reached by the critical-point solve on a
NURBS sphere (the corpus's `c7_tiny_loop_*`) and by the closed form on an analytic one.

**Corpus.** ssi-corpus (MIT OR Apache-2.0): 119 cases, the six built from Yang et al. data
excluded per 04. The scorer's verdicts are kept (CRASH, ERROR, INCOMPLETE, WRONG TOPOLOGY,
INACCURATE, PASS, `acc_tol = 10 tol`), but judged against the truth stored with each case, not
against the pin's output.

## 10. Open questions

1. **`EdgeShape` against 01 C10.** C10 puts the intersection curve as a procedural `AnyCurve`
   arm with its evaluator. The brep's `EdgeShape::Curve` holds a plain `Curve`. Proposed: step 5
   stores the certified 3D B-spline as `Curve::Nurbs` with a `Certified` record. The definition
   is recoverable from the edge's two faces, and the procedural arm waits until refinement past
   the recorded accuracy is needed. This needs a ruling that amends C10.
2. **Where `OnChart` lives.** One choice is a `Chart` value in `scx-curve` (keeps `Curve2` whole,
   adds a little trigonometry below `scx-surface`). The other is an `AnyCurve2` in `scx-surface`
   wrapping `Curve2` (keeps layers pure, and every pcurve consumer changes type). Proposed: the
   first.
3. **Topology up to the coincidence** at nodes and touches (§4.6). Proposed as SS4's rule
   extended. It needs a ruling because it makes a topology claim, not only a geometric one.
4. **01 D10's eager fitter.** With `OnChart` and co-marched uvs, the fitter is needed only for
   STEP export and for exact curves on NURBS faces. Proposed: D10 narrows to that.
5. **Porting the pin crate's code itself.** The licence allows it (MIT OR Apache-2.0). Each file
   still needs the idiom hunt: string errors, fixed caps, ad hoc pads, `Option` returns. The rows
   above assume a port with rewrites, not a copy.
6. **The six corpus cases from Yang data.** They stay excluded until the licence is confirmed
   (04). SM10 is sized without them.

## 11. Proposed 04 row

| 11-27 | Proposed (R73, research 73), for review. Step 5's SSI has five layers, in this order. (1) Closed forms first: today's SS1 to SS5, plus parallel cylinders, equal cylinders with meeting axes, Miller-Goldman conic pairs and Villarceau planes, each `General` past the tolerance. (2) Exact pcurves for exact curves on analytic surfaces: `Curve2::OnChart`, the closed-form chart inverse at the edge's own parameter, split at poles. (3) Then `scx-ssi::general`, the pin `ssi` crate's design ported as idiomatic Rust: Bézier pieces through `BoxTree::pairs`; pairs dropped only by proof with outward rounding; loop-free by certified normal cones, Sederberg-Meyers, then a Krawczyk critical-point solve that seeds loops and classifies touches and nodes by `II_A - II_B`; the field path whenever a side has an implicit form, the march otherwise; Taylor fields deferred and refused. (4) Each span is certified by residual enclosure plus parametric Krawczyk and recorded as `Basis::Certified`; three B-splines share one knot vector. (5) Refusals are typed `SsiError` (`Unresolved`, `Stalled`, `Singular`, `Uncertified`, budget, stop), carried in `SsiPair`; there are no fixed caps, and order is fixed. Topology is claimed up to the coincidence, as SS4 does. BO2b stays as the filter in front. Change if: SM11 measures certification above 2 times the uncertified path, the torus field path above 2 times the pin crate, or C10's procedural arm is ruled needed for step 5. |

## 14. SM3 as built (11-27)

SM3 lays down the public API of `scx_ssi::general`, the typed `SsiError` and its plumbing into
`scx-boolean`. The body refuses every pair. This section records what was built, where it departs
from §4.10 and §4.11, and the case for and against each choice.

### 14.1 What exists

- `scx_ssi::SsiError` (`crates/scx-ssi/src/error.rs`) is §4.10's enum, `#[non_exhaustive]`:
  `Unresolved { at, size }`, `Stalled { at, step }`, `Singular { at, kind: SingularKind }`,
  `Uncertified { at }`, `Budget(Exhausted)` and `Stopped`.
  - `kind()` is `Unsupported` for the four refusals, `Exhausted` for the budget and `Cancelled`
    for the stop (62).
  - `name()` gives the stable names `ssi_unresolved`, `ssi_stalled`, `ssi_singular` and
    `ssi_uncertified`, and the budget's and stop's own names.
  - `at()` gives the named point, or `None` for the budget and the stop.
  - The one-line Display names the point and the size or step in mm. `From<Exhausted>` and
    `From<Stopped>` exist, so `budget.charge(n)?` and `stop.check()?` work.
- `SingularKind` (`#[non_exhaustive]`) has two variants. `DegenerateForm` is a collinear-normal
  point whose relative form is singular within its bound: a tacnode, a cusp or a touch of order
  above 2, not told apart. `HighOrderTangency` is a tangent curve of order above 2, which is
  SM12's.
- `scx_ssi::general::intersect(a, ae, b, be, [Accuracy; 2], Resolution, &mut Budget, &Stop) ->
  Result<Traced, SsiError>` charges one unit, then polls the stop, as `global_solve` orders them.
  It then refuses with `Unresolved`:
  - at the middle of the overlap of the two certified boxes `Surface::box_over(extent)`, or of
    their union when they do not overlap;
  - with `size` set to that box's longest side;
  - or, if a box cannot be enclosed, with an infinite size at the middle of the two extents'
    middle points.
- `scx_ssi::intersect` takes the same arguments and returns `Result<Answer, SsiError>`, where
  `Answer` is either `Closed(analytic::Intersection)` or `Traced(general::Traced)`. It tries the
  closed forms first. The closed forms charge nothing and do not poll the stop.
- The answer types (`general/traced.rs`):
  - `Traced { branches, nodes, touches }` (`#[non_exhaustive]`, `Default` = no meeting, proved).
  - `Branch { curve: Arc<NurbsCurve<3>>, pcurves: [Curve2; 2], contact: Contact, ends: Ends,
    certificate: Certificate }`.
  - `Ends::{Closed, Open { start, end }}`, `End::{Rim(Rim), Node(usize)}` and `Rim::{A, B, Both}`.
  - `Node { point, uv: [Point2; 2], kind: NodeKind }`, with `NodeKind::{Crossing, Pole, Seam}`
    (`#[non_exhaustive]`).
  - `Touch { point, uv: [Point2; 2] }`.
  - `Certificate { to_intersection: Length, on_surface: [OnSurface; 2] }`
    (`#[non_exhaustive]`, so only this crate builds one), and `OnSurface::{Exact, Within(Length)}`.
  - `Certificate::curve_cert(reads)` and `OnSurface::cert(reads)` give the `scx_num::cert::Cert`
    records (`CurveToIntersection` by `Krawczyk`, `PcurveOnSurface` by `Interval`). The caller
    supplies the keys. An exact pcurve has no record.
- In `scx-boolean`:
  - `BooleanUnsupported::SsiPair` gains `cause: SsiError`, and its Display appends the cause's
    line.
  - `BooleanError::of_ssi(cause, a, b, kinds)` maps the budget to `Exhausted`, the stop to
    `Cancelled`, and any refusal to `SsiPair` with it as the cause.
  - Stage 4 does not call the general method yet; that is SM9. Its pre-check `apart` now returns
    the first region its bisection could not clear (`Option<Extent3d>`, `None` = proved apart)
    instead of `false`. The refusal is `Unresolved` at that region's middle, of its longest
    side.
  - A face with no box at this point is now `Defect(NoFaceBox)`, which matches that variant's
    contract. It was a bare `SsiPair` before. It cannot happen, because pairs come from the box
    tree.

### 14.2 Decisions, for and against

1. **A separate `Traced`, not a certified kind in `IntersectionCurve` (departs from §4.11 and
   §7's SM3 row).**
   - For:
     - `IntersectionCurve` is read field by field in stages 4 and 5, which BO4 is editing now. A
       new kind there forces a match arm in every reader before any of them can use it.
     - A traced branch carries what a closed-form piece does not: two pcurves, its ends, and its
       certificate. A closed-form piece carries its exact span and overlap flags instead.
     - `Answer` keeps the two apart where they differ, and SM9 reads both.
   - Against:
     - Stage 4 has two answer shapes to split at merged points.
     - §4.11's single `Intersection` would let `exact_loops` read one list.
   - Held: SM9 can convert a `Traced` into stage 4's records in one place. Folding the types
     later is cheap, but splitting them after readers depend on a merged type is not.
2. **`Answer` at the top level, not `Result<Intersection, SsiError>`.**
   - For: it follows from decision 1, and the caller sees which method answered.
   - Against: a caller that wants "just the curves" matches twice. No such caller exists yet.
3. **The certificate as bounds plus `Cert` builders, with no keys stored.**
   - For: `scx-ssi` sits below `scx-brep` and knows no entity keys. `Cert` already lives in
     `scx-num`, so the records the body stores are built from this type with no new type.
   - Against: a branch's certificate holds one bound per claim, not one per span. SM8 may want
     per-span bounds, which is why `Certificate` is `#[non_exhaustive]`.
4. **`OnSurface::Exact`.** SM1's `OnChart` pcurve is exact and carries no record.
   - Against: a seam or pole split still needs the chart's turn tracking. That is SM1's, not a
     bound.
5. **`End::Node(usize)` indexes `Traced::nodes`.**
   - For: there is one owner for each node, and no `Rc` graph.
   - Against: an index can be stale if a caller edits the lists. The fields are public for
     reading, and the crate builds the lists. No setters are offered.
6. **`Rim::Both`** names a corner where both extents' boundaries meet within the coincidence.
   The alternative was two ends, which would double-count the exit.
7. **Coincident pieces (all-zero fields, SM4) are not a field yet.** `Traced` is
   `#[non_exhaustive]` so SM4 can add one without breaking callers.
   - Against: SM4 must decide the shape (a uv region on each side) before SM9 reads it.
8. **`SsiPair` holds a full `SsiError`, though `Budget` and `Stopped` never go there**
   (`of_ssi` routes them to the Boolean's own variants, and the field's doc says so).
   - Against: a narrower `SsiRefusal` type would make the bad state unrepresentable.
   - Held: it would duplicate four variants and their names for a state that one constructor
     already rules out. `BooleanUnsupported` is `#[non_exhaustive]`, so tightening the field
     later breaks only this crate.
9. **`BooleanError` and `BooleanUnsupported` lose `Eq`**, because the cause holds `Point3` and
   `Length`, which are `f64`. Nothing outside `scx-boolean` names either type, so `PartialEq` is
   enough for the tests.
10. **Stage 4's cause comes from BO2b's bisection, not from the stub.**
    - For: the region it names really is the undecided one. On the torus and block it is a
      0.14 mm piece holding the touch, where the stub would name the faces' whole overlap.
    - Against: this is a touch to `faceface.rs` beyond the error type. BO4 does not edit that
      file, and the change is only `apart`'s return type and the construction site.
11. **The stub's region is the box overlap.**
    - For: it is deterministic, symmetric in the operands, and lies inside both surfaces' boxes.
    - Against: it says nothing about where the surfaces really meet. It is replaced by SM4's
      first undecided pair.

### 14.3 Tests

- `scx-ssi` `general::tests` has 10 tests:
  - a general pair (tpipe; a NURBS sphere against a plane) is refused as `Unresolved` inside
    both boxes, with one unit spent;
  - the budget is charged before the stop is polled, and the budget answers when both apply;
  - the closed forms answer first with no budget and the flag set, and spend nothing;
  - the same bits on every run and under any budget that suffices;
  - swapped operands name the same place;
  - every cause's kind, name, Display and `at()`;
  - the `?` conversions;
  - the certificate's records;
  - the empty trace.
- `scx-boolean`:
  - `expect_ssi_pair` now checks that the cause is `Unresolved` and is the same in either operand
    order;
  - the torus-and-block test checks that the named region holds the touch and is small.
- No test was removed.

### 14.4 Added cost

- The general stub is not on any Boolean path.
- `apart` returns a box it already held instead of a `bool`. The refusal path builds one
  `SsiError` (a midpoint and a longest side) once per refused Boolean.
- No loop gained work.

### 14.5 Ruling (11-27)

The coordinator accepted SM3 as built.

- `Answer` and `Traced` stay separate from `IntersectionCurve` for now, as a stepping stone. SM9
  merges them.
- SM9 also narrows `SsiPair`'s cause to a refusal-only type, so that `Budget` and `Stopped`
  cannot be represented there (§14.2 items 1 and 8).
