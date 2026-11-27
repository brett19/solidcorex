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

## 12. SM0 as built (11-27)

Branch `sm0`. New files only: `scx-testkit/src/ssi_truth.rs` (one `mod` line in `lib.rs`) and
`scx-testkit/tests/ssi_truth.rs`. No `scx-ssi` or `scx-boolean` source is touched. The module
states §9's truths as data a test can ask: each `Truth` holds the two surfaces (`Surf`: signed
distance and normal, so a residual is a distance), the curves (`TruthCurve`: a pure function of
one parameter, its domain, `Loop` or `Line`, and the length where known), the nodes (point plus
every `(curve, parameter)` that reaches it), isolated touches, and `tangent` and `coincident`.
`Truth::placed(&Rigid)` moves the whole answer; `Truth::hausdorff(candidate, n)` gives the two
one-sided distances; `Truth::residual`, `Truth::nearest` are the per-point checks.

**Cases.** `sphere_sphere` (circle, touch, coincident, empty; `sphere_sphere_frame` gives the
circle's frame), `tiny_loop`, `cylinders_perpendicular(r, R, e)` (two loops, Steinmetz with two
nodes, a figure eight at `e = R - r`, one loop, a touch), `cylinders_parallel` (two lines,
tangent line, coincident), `sphere_cylinder` and `viviani(a)`, `torus_plane_parallel` (the five
spiric sections with the figure eight and the touch), `torus_villarceau`. Boolean figures:
`bicylinder_volume`, `bicylinder_area`, `t_union_volume`, `sphere_lens_volume`,
`sphere_union_area`, with `ellip_k` and `ellip_e`.

**Decisions, with the cases for and against.**

1. *Analytic parameter through a square-root end.* A loop with `z = sqrt(g(t))` has vertical
   tangents where `g = 0`; a plain `sqrt` there is not smooth, and the ends are where a marcher
   is tested. For an oval the truth uses `phi = phi0 cos(th)`: `cos phi - cos phi0 = f sin^2(th) / 2`
   with `f` analytic and positive, so the signed `z = sin(th) sqrt(K f / 2)` is analytic through
   both ends. For a figure eight it uses the half angle (`z = 2 sqrt(e r) sin(t/2)`, Viviani's own
   form). For: exact points and derivatives at the ends, one parameter for the whole loop. Against:
   the parameter is not the cylinder's angle for the one-loop skew case (documented per case).
   The tests therefore also compare against naive two-family parametrisations (by angle, and by
   height `z`) that share no code.
2. *Lengths.* `length` is `Some` for circles, the Steinmetz ellipses (`4 sqrt(2) R E(1/2)`) and
   Viviani (`8 sqrt(2) a E(1/2)`) in closed form, and for the concentric perpendicular loops
   (`e = 0`, the tpipe's 9.887974) by trapezoid of the exact speed (geometric convergence, 4096
   nodes). Every `Some` is checked in the tests against Romberg-extrapolated chord lengths, which
   use no derivative. `None` elsewhere (skew loops, spiric ovals): no closed form was derived.
3. *Nodes are tangent points.* In every case here the surfaces are tangent at a node (Steinmetz,
   Viviani, the skew figure eight, the spiric figure eight, the Villarceau pair); the tests assert
   parallel normals at every node and touch. A transversal node does not occur in this set.
4. *Exact comparisons.* Touch, node and tangent classes compare on exact `f64` (on `d^2` for
   spheres and parallel cylinders), as the constructors' arguments are the caller's exact numbers;
   the tests also straddle each critical value by 1e-9 to show the class flips.
5. *Elliptic integrals by AGM, with `S`.* `E = K (1 - S)`; the bicylinder's `(R^2 + r^2) E - (R^2 - r^2) K`
   is written `r^2 K (2 - S - S/m)` and the area's `E - (1 - m) K` as `K (m - S)`. The first form
   lost 5e-12 relative at `r/R = 0.003` against quadrature; the second does not cancel. (A first
   AGM loop with the tolerance `1e-17 a` never ended: `a - b` stalls at one ulp.)
6. *Villarceau and the figure eight come from the quartic's own roots* in the tests (a quadratic
   in `x^2` per height), not from the tube-circle picture the truth uses.

**Verified.**
- Bicylinder volume `16 R^3 / 3` at `r = R` (and the limit `r -> R(1 - 1e-9)`), `26.1131656` at
  `r = 1.5`, `R = 2` (to 5e-8), against two independent quadratures (slices across `y`, slices
  across `x`) at five radius pairs to 1e-12 relative; the area `16 R^2` and the elliptic area
  against a direct sum of the two cylinders' parts.
- The tpipe union volume is `102.5 pi - V/2 = 308.956664169885` (the branch is inside the pipe
  over half the bicylinder; it equals the recipe table's value to 1e-9). The pin's figure
  308.956697417 (308.9567) is 3.3e-5 above it: the pin's error, not the truth's. Each tpipe loop is
  9.887974 long.
- Every curve sample has residual below 1e-14 R on both surfaces; loops close; nodes sit on the
  claimed parameters; `placed` keeps all of it under a random rigid motion.
- Tiny loop `delta = 1e-7`: radius `sqrt(delta (2R - delta))`, residual below 3e-15; the cap area
  `2 pi R delta` from two routes. The checker's `acos` loses digits at small `delta`; its tolerance
  scales as `8e-16 / delta`.
- Sphere lens and union area against slice integrals and a bisected-boundary polar integral.

**Not done here.** The perturbed variants with the tolerance's expected answer (§7 SM0) wait for
the tolerance model of SM3; the NURBS-sphere form of `c7_tiny_loop_*` is SM5's test. No truth for
sphere-cylinder with the sphere centre off the plane of the axes (not a closed form of this kind).
`nearest` samples 512 parameters and narrows the six lowest local minima by golden section (about
800 evaluations; a crossing needs more than one minimum: a first version with one missed a point
near a node by 2.6e-3). Test time: the first version took 78 s wall (3 CPU minutes) because
`nearest` (4096 samples) was called about 12,000 times per case; now the default run is 1.1 s
(8 s with the build) and the `_full` sweeps (`-- --ignored`: all five radius pairs, 40 sphere
pairs, 8 rigid motions, every naive vertex and 600 truth samples per comparison) are 12.7 s. The
default run checks every 16th naive vertex against the same tolerance and 150 truth samples. CI
runs the ignored sweeps next to the nurbs ones.

## 13. SM1 as built (11-27)

SM1 adds `Curve2::OnChart`, `scx_curve::{Chart, ChartKind, OnChart}` (the 04 R73 ruling: `Chart`
lives in `scx-curve`), `Surface::chart`, the `pcurve_of` fallback that makes it, its record form,
and the stage-5 and mass wiring. The two-sphere Boolean is now a result: the imprint's counts and
`check`, and union, subtract and intersect each pass `scx_check::contract` and match the
closed-form volume to `1e-9`.

### 13.1 The value: `OnChart { curve, chart, over, reference }`

```rust
pub struct Chart { /* frame, sense, kind */ }      // Chart::new(frame, sense, kind) -> Result
pub enum ChartKind { Plane, Cylinder { radius }, Cone, Sphere, Torus { major } }  // non_exhaustive
pub struct OnChart { /* curve, chart, over, reference */ }
// OnChart::new(curve, chart, over: Extent1d, reference: [Point2; 2]) -> Result
// point, jet::<D>, point_over::<T>, jet_over::<T, D>, domain, reversed, restrict, shifted
```

- **The chart is the surface's own uv** (63 D13), not an angle. Each periodic direction is
  `Arcs::of_direction`: per quarter (or half) arc, `u = (q + 1/2) L + tau / a` with
  `tau = b' / (|(a, b)| + a')`, `(a', b')` turned back by the arc's middle. So there is no
  `atan2` or `asin`: only ring operations and one square root per direction (73 §4.2 expected one
  `atan2` and one `asin` a point).
- **Derivatives are Taylor arithmetic through that formula**: the 3-D curve's jet gives the local
  coordinates' series, a square-root series gives `|(a, b)|`, a series quotient gives `tau`
  (`Arcs::direction_series`, new). The value and every derivative are the closed form's, in `f64`
  and in `Ivl` (generic over `Trig`, `TERMS = 16`). An interval box takes every arc it meets, each
  clipped to its quarter, and joins them.
- **The uv is only C1 across an arc join.** The NURBS-compatible parameter of a quarter arc is not
  the angle, so `u''` jumps at each join (on the two-sphere circle, from `-0.234` to `+0.234`).
  This is the surface's own uv, so it is correct, not a defect; it is why the mass enclosure needed
  §13.5's split rule.
- **A point within `NEAR = 4 eps` of a break** takes the arc the curve is moving onto, from the
  sign of the zero component's first nonzero later coefficient (`Approach` flips odd orders), and
  the break's exact value. A point exactly on a break whose side cannot be decided joins both.
- **The cone** needs no size: on the surface `v = z`, and the nappe is the one with `r + k v > 0`,
  as `Cone::uv_in`; a curve on the other nappe fails the lifting check. **The sphere's `v`** is the
  half arc at `(rho, z)`, in `[0, 1]`; at an exact pole the value is undefined, which `pcurve_of`
  never produces (§13.3).

### 13.2 The branch: `over` and a reference line, not `turn: [i32; 2]` (deviation from §4.2)

A turn count at the span's start cannot place a closed curve's lift: its two ends are the same
point with lifts a period apart, so the branch at `t` needs the span as well. `OnChart` keeps the
span `over` and a reference line, given by its uv at `over`'s two ends. At `t` each periodic
coordinate is the principal value plus the whole periods that bring it nearest the line at `t`.

- **The line is centred on the curve.** The maker takes the chord between the curve's lifted ends
  and moves it to the middle of the band the lifted values span about it, then accepts while the
  band is under half a period wide. The nearest lift is the curve's own while every value is under
  half a period from the line; centring leaves a quarter period to spare. Built first with the
  chord itself (a quarter period either side of it), the accepted set depended on where the span
  started: a tilted circle whose chord sat off-centre was refused though its band was narrow.
  Adversarially: the centred line accepts strictly more curves than the chord rule with the same
  margin, and the band is measured on the same samples as before (§13.3).
- In intervals the lift is applied to **each arc's value before arcs are joined**. Built first the
  other way (lift after the join), a box straddling the period break held the whole period, never
  narrowed, and the two-sphere union's contract failed `SignUndecided`. A test now asserts that the
  interval width shrinks with the box.
- `reversed` negates `over` and swaps the reference's ends; `restrict` keeps both (the lift is a
  property of the maker's span, not of the restricted one); `shifted` moves the reference by whole
  periods. Both
  `scx-brep` users of `shifted` (`pcurve`, `draft::transform::aligned`) take the new arm; the
  transform's uv map still answers `None` for `OnChart` (it rebuilds through `pcurve_of`).
- **Edges are not split at seams.** 73 §4.2 and the brief expected a split at seams; the lift makes
  it unnecessary, and the two-sphere edge is one full-circle use crossing the seam in its interior.
  Poles and apexes still split (`CrossesPole`).

### 13.3 `pcurve_of`: OnChart after the closed forms, never instead of them

`pcurve_of` tries the closed forms first, unchanged (`Line2`, `AngleLine`, `AngleWave`, ...). Only
where they answered `NoClosedForm` does it call `on_chart`, so pairs that had a closed form pay
nothing new. `on_chart`:

1. Takes `surface.chart()`; a NURBS surface has none, so it stays `NoClosedForm`.
2. Off the plane and cylinder, checks the axis at `DENSE = 64` samples. An end within `tol` of the
   axis returns `None` (so `NoClosedForm`; the uv there is a whole line). An interior approach (a
   local minimum of `rho` no larger than the chord between its neighbours, refined by golden
   section, `GOLDEN_STEPS = 120`) within `tol` is `CrossesPole { at }`, so the caller splits there.
3. Unwraps each periodic coordinate along the samples, halving a step while it turns by an eighth
   of a period or more (`MAX_HALVINGS = 40`). The start is placed in the surface's window.
4. Measures, per periodic coordinate, the band of the lifted values about the chord between the
   lifted ends. A band half a period wide or more is refused (`None`); otherwise the reference line
   is the chord moved to the band's middle (§13.2). Then it makes the `OnChart` and runs the
   existing 9-sample lifting check.

Adversarial notes: the pole check and the deviation check are sampled, not certified. A curve that
dips to the axis between samples, within `tol` and narrower than `1/63` of the span, is missed, and
its pcurve is then undefined near the dip. This is the same exposure as the 9-sample lifting check
the closed forms already carry. A certified version (an interval bound of `rho` over each sample
gap) is an open question below, not done here.

### 13.4 Record form

`Curve2Kind::OnChart = 8` (`"on_chart"`): the 3-D curve, the chart, then `over` and the
reference as six floats. `Chart` records its tag (`ChartTag`, `"chart"`: plane 0, cylinder 1,
cone 2, sphere 3, torus 4), frame, sense, and the one size a kind's uv depends on (the
cylinder's radius, the torus's major radius).
Reading revalidates through `Chart::new` and `OnChart::new`. The round-trip property test draws
OnChart as its ninth kind.

### 13.5 Mass and stage 5

- Stage 5 (`scx-boolean`) needed no code change: once `pcurve_of` answers, the two-sphere
  imprint goes through. Only the tests changed.
- `scx-mass` `curve_breaks`: an OnChart's own breaks are its 3-D curve's (a NURBS curve's knots).
  Its arc joins lie on the surface's break lines and are found as crossings, as for the angle
  kinds.
- `scx-mass` enclosure, one rule changed. A cell on a `v` break line was always split along `s`.
  When the line is crossed by the trim itself (the span's end `v(t)` moves across it along `t`),
  splitting `s` never takes the cell clear: the two-sphere lens sat at 131,072 cells (the cap) with
  its enclosure 18 wide. Now such a cell splits along whichever of `t` or `s` contributes more to
  its `v` range. Before OnChart no trim whose `v` varies crossed a `v` join (sphere trims were
  latitudes and meridians), so this was latent. The lens now settles in 662 cells.

### 13.6 Tests

- `scx-curve` `chart` (8 tests): the value is the surfaces' inverse bit for bit; jets of
  off-axis curves against the chain rule; the angle kinds' pcurves reproduced; a break takes the
  side asked for; the lift follows the span past a turn; intervals enclose the `f64` values and
  narrow with the box; the axis and a pole are undefined; bad data is refused.
- `scx-surface` `pcurve`: `other_curves_go_through_the_chart` (40 random tilted small circles on
  spheres: lifting, start in the window, first derivative by the chain rule within `1e-11 r`);
  `a_curve_through_a_pole_is_split_there` (a circle through the pole: `CrossesPole` within
  `1e-7`; one ending on it: `NoClosedForm`; one clear of it: `OnChart`).
- **Changed on purpose**, in `refusals_are_typed`: the ellipse on a cone, the hyperbola on a cone,
  and the small tilted circle on a sphere were asserted `NoClosedForm` (the hyperbola case also
  that the message names "hyperbola"). They now assert `Ok(Curve2::OnChart(_))`, because these are
  exactly the pairs OnChart is for. The refusal and its message are still covered by the NURBS
  surface case (`NoClosedForm { Line, Nurbs }`, "NURBS surface", `Unsupported`).
- **Changed on purpose**, in `scx-brep` `refusals_by_cause`: an oblique section of a sphere and an
  ellipse section of a cone, each alone in a loop, were asserted `NeedsFit` from the exact pcurve
  writer. `pcurve_of` now gives them `OnChart` (asserted), and the writer reaches the cause that
  `NeedsFit` hid: each goes round the axis, so alone it winds (`Winds`, as a lone parallel does).
  `NeedsFit` (`Unsupported`, `pcurve_needs_fit`) is still covered, by a circle on a plane given as
  a NURBS surface.
- `scx-record` `geometry`: OnChart in the round-trip pool.
- `scx-boolean`: `two_spheres_need_a_fitted_pcurve` (proved stage 5 refused
  `Unsupported(Pcurve)`) is replaced by
  `two_spheres_cut_each_other_along_a_circle_through_the_chart` (counts `[3]` and `[2]`,
  `check`), and `two_unit_spheres_a_radius_apart_unite_subtract_and_intersect` is new (contract,
  and volumes `8 pi/3 - 5 pi/12`, `4 pi/3 - 5 pi/12`, `5 pi/12` to `1e-9`).

### 13.7 Cost

Evaluation, a release build, mean of 2,000,000 parameters across the span (throwaway timer, not
committed; the main box, 11-27):

| curve on surface | kind | `point` | `jet::<2>` |
| --- | --- | --- | --- |
| sphere latitude | `AngleLine` | 29–31 ns | 31 ns |
| sphere latitude | `OnChart` | 45–47 ns | 59–60 ns |
| cylinder oblique section | `AngleWave` | 47 ns | 49 ns |
| cylinder oblique section | `OnChart` | 42 ns | 46 ns |
| tilted sphere circle | `OnChart` (no other kind) | 45 ns | 59 ns |
| plane line | `Line2` | 1.5 ns | 1.4 ns |
| plane line | `OnChart` | 10 ns | 14 ns |

`OnChart` costs about what the angle kinds cost (one square root and a quotient per periodic
direction, against their own arc parameter) and is never chosen where they apply (§13.3); against
`Line2` it is 7–10 times dearer, which is why the plane keeps `Line2`. Making one costs the 64
samples, their unwrapping and the lifting check, only on pairs that had no closed form before.

Mass: the two-sphere lens went from the cell cap (131,072 cells, enclosure 18 wide, `CellCap`) to
662 cells (`Reached`). Over the scx-mass and scx-boolean suites, 468 enclosures took 401,984 cells
before the split rule and 401,935 after (49 fewer), counted with a side-channel counter that
was not committed, before the latest merge of `main`.

### 13.8 Open questions

1. **Inverting a point onto an OnChart pcurve** (`Curve2` inversion) answers `Unsupported`, as for
   other non-line kinds. Callers that need it go through the 3-D curve.
2. **`draft::transform`'s uv map** answers `None` for OnChart and rebuilds through `pcurve_of`.
   A rigid motion that keeps the surface's frame could carry it exactly.
3. **Certified pole and deviation checks** (§13.3) in place of sampling.
4. **01 D10's fitter** narrows further: exact curves on analytic faces never need it now. It
   remains for NURBS faces and STEP export (§10 item 4).

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

## 15. SM4 as built (11-27)

SM4 cuts each surface into pieces with certified boxes, pairs them with `BoxTree::pairs`, and
drops a pair only by proof. `general::intersect` now answers a proved-empty `Traced` for pairs
that are truly apart, and refuses the rest as `Unresolved` at the first piece pair it could not
clear. The Bézier-level proofs that need implicit forms (field sign) wait for SM6a.

### 15.1 What exists

- `general::Candidates` (`general/pieces.rs`), with `Piece { uv, bbox }` and `PiecePair`:
  - `Candidates::new(a, ae, b, be, acc, res, budget, stop)` builds each surface's cells (the
    knot-span rectangles of `Surface::to_nurbs`, u-major; the whole extent as one cell where the
    form cannot be built), each with `Surface::box_over`. It pairs them through `BoxTree` and
    keeps the pairs the slab proof does not drop. `pairs()` is the survivors, in ascending piece
    order, for SM5 and SM6a.
  - `apart(&pair)` is the proof (boxes, then slabs). `halve(&pair, budget, stop)` splits one
    piece in two and returns the child pairs that survive; `settle(pair, front, ..)` halves
    round by round until nothing survives (`None`: proved) or it must give up (`Some(pair)`).
  - A pair at the resolution, or whose pieces cannot be split in `f64`, is `Unresolved` where it
    lies. That ends every halving, so no fixed depth exists.
- `Surface::range_along(dir, uv) -> Option<Ivl>` (`scx-surface`, `enclose.rs`): a certified
  range of `dir . S` over a box. Analytic kinds use the mean-value form with the partials
  enclosed by their own formulas in a ring of value-with-partials (`Grad`, intervals), met with
  the natural range; NURBS uses `partials_over_piece` per knot span. A flat piece seen along its
  normal has a width second order in its size.
- Proofs and their rounding:
  1. Boxes. `BoxTree` tests `a.min > b.max + t`, so it is given `t = tol + 4 eps (reach + tol)`
     (rounded up, `reach` the largest coordinate of any box): that exceeds the rounding of the
     sum, so a dropped pair is apart by more than `tol` in the reals. Child pairs use interval
     subtraction.
  2. Slabs. Along the unit normal `n` of either piece at its middle, the two ranges are apart
     by more than `tol |n|`, with `|n|` an interval upper bound. No `acos`, no pad.
- `Traced::coincident: Vec<analytic::Overlap>` (see 15.2 item 3).
- The Cargo graph gained `scx-algo` and `scx-nurbs-surface` for `scx-ssi`.

### 15.2 Decisions, for and against

1. **Halving is a method for SM5, with a width limit in `intersect`.**
   - For: with no loop-free test yet, a pair that the surfaces cross can never be proved apart
     and would be halved down to the resolution. `intersect` therefore gives a piece pair up
     when the surviving front of its descendants holds more than `FRONT = 24` pairs. A
     near-miss keeps a narrow front at the nearest points. A crossing widens it every round.
   - Against: `FRONT` is a fixed number, though a policy of effort and not of proof (it never
     drops a pair; only the refusal depends on it). SM5's loop-free test replaces it as the
     stop. 8, 12 and 24 were measured (15.4); 24 is what proves two unit spheres `2^-10` and
     `2^-20` apart.
2. **Field-sign proof deferred to SM6a** (§4.3 item 3). It needs the implicit forms and the
   rational Bernstein `g` that SM6a builds. For a plane side the slab is that proof.
3. **Coincident pieces.** `Traced` gains `coincident: Vec<analytic::Overlap>`, the closed
   forms' own type (convex polygons in both surfaces' uv, vertex for vertex).
   - For: SM9 reads one shape for `Closed::Coincident` and the general method.
   - Against: a NURBS-on-NURBS overlap may not be a convex polygon. Held: it is then split into
     convex pieces by whoever proves it.
   - Nothing fills it in SM4. The proof is an all-zero field (SM6a), and a slab alone cannot tell
     a coincident patch from one that only lies in the same plane beside it.
4. **Analytic ranges are not taken from the rational NURBS image.** `to_nurbs` rounds its
   control data (the quarter arc's middle weight is `cos(pi/4)`), so a hull range from it is
   not certified. Ranges use each kind's own formula instead.
   - Against: the kinds' interval boxes are loose (a unit sphere's whole box is 20 percent
     wide), so near-touching analytic pairs cost many halvings (15.4).
5. **Region of a surviving pair.** The overlap of its boxes, or their union when rounding leaves
   none (the pair survived by the coincidence alone). `Unresolved` names the middle of that
   region.
6. **Halving rule.** The piece to split comes from the axis along which the ranges overlap
   least (the wider piece), and its split side from the narrower range along that axis (box
   length without an axis). A split by box length alone stalled on cylinders (it halved their
   height forever along the tangent line).

### 15.3 Tests

`general::tests::apart` (9 tests): separated pairs (sphere over and under a plane, two
spheres, a cylinder beside a plane, spheres also as NURBS) at gaps `4` to `2^-20` are proved
empty where `testkit` says the whole surfaces do not meet; the SM0 truths (`sphere_sphere`,
`cylinders_parallel`) decide spheres and parallel cylinders through and past the touch;
gaps at, inside and past the coincidence, and crossings, are never dropped; a refusal at a touch
names it; halving proves what the cells cannot; a pair dropped by `apart` is far apart by
sampling; the same bits every run, swapped, and under the exact budget (one unit less is
`Budget`); a set stop; the tolerance is the coincidence. `scx-surface` has two tests of
`range_along`. In `general::tests`, three assertions changed (the refusal names a piece pair,
not the whole overlap, and spends more than one unit); none was removed.

### 15.4 Measured (release, throwaway harness, not committed)

Pairing at the cell level against the pin's `ssi` crate, on all 119 corpus cases as exact
NURBS patches (pin: its Bézier pieces and all pairs of `pieces_apart`, pad 1e-9; here:
`Candidates::new`, tol 1e-9):

| | pin | here |
|---|---|---|
| median per case | 12.7 us | 211 us (2.3 to 68 times) |
| surviving cell pairs, total | 2,083 | 2,736 (more in 22 cases, same in 97, fewer in none) |

- The cost is the certified enclosures (`box_over`, `partials_over_piece` at each slab), not the
  tree. The pin takes its boxes and slabs from control points, uncertified.
  `nurbs_invert.rs` already extracts Bézier nets with error bounds, so a hull range along `n`
  would be certified and as cheap as the pin's. That is the next step if SM11 finds this
  matters.
- `intersect` end to end (cells and halving to the refusal): 116 of the 119 cases (they cross)
  are `Unresolved`, 3 are proved empty. Median 11.9 ms, total 5.2 s with `FRONT = 24`; with 12
  10.6 ms and 4.3 s; with 8 4.2 ms and 1.35 s. This is the refusal path only, and SM5's
  loop-free stop replaces it.
- Near-touching analytic pairs: two unit spheres `2^-10` apart, 4,305 pair tests and about
  100 ms; `2^-20`, 9,089 and 214 ms. The same pairs as NURBS are cleared at the cells (the NURBS
  boxes are tight at the contact), in about 0.1 ms.
- Not on any Boolean path yet (the stage 4 stub does not call it), so no existing path gained
  work.

### 15.5 Open

- Hull-based certified ranges for NURBS and for the analytic kinds' exact images (15.4).
- The analytic kinds' loose interval boxes.
- A shared per-piece cache of the own-normal range (each slab computes it again per pair).

## 16. SM10 as built (11-27)

SM10 is the corpus scorer: a test target, `crates/scx-ssi/tests/corpus/` (`cargo test -p scx-ssi
--test corpus`), in new files only. It does not touch the algorithm. It runs `scx_ssi::intersect` on
each case of the ssi-corpus and judges the answer against the truth stored with the case. SM5
onward measure progress with it.

### 16.1 What it does

- **Data.** The corpus (jyviko/ssi-corpus, MIT OR Apache-2.0) is not in the repository.
  `SCX_SSI_CORPUS` names the folder that holds `cases/` (each case `.json` or `.json.gz`, read
  through `gzip -dc`). The six cases built from Yang et al.'s published data (`y23_appendix_ex2`,
  `y23_t3_ex1` to `ex5`) are excluded by id (04 R73 row); 113 cases run.
- **Surfaces.** Each case surface becomes a `Surface` over an `Extent2d`: the analytic kinds
  through their frame and `from_step` (degrees to radians; the cone's frame at its apex with
  radius 0, so `v` is the height from the apex), a NURBS surface through `KnotVector` and
  `Net` over its knot domain. Every case checks itself: the `surf` column is the largest distance
  from a few of the truth's points to either built surface, by `Surface::invert`. It is at most
  3e-9 over the 113 cases, so the loader's conventions are right.
- **Run.** One call per case at a fixed tolerance, `min(1e-6 mm, feature_size / 10)` for the
  resolution and both accuracies, and a fixed `Budget` of 2,000,000 units. `acc_tol` is ten
  times the tolerance; the coverage tolerance is the corpus's rule,
  `max(cov_tol, min(0.5, 2 acc_tol ill_cond))`.
- **Scoring.** Verdicts, most severe first: `REFUSED <name>`, `INCOMPLETE`, `WRONG TOPOLOGY`,
  `BAD CERT`, `INACCURATE`, `PASS`.
  - Refusals are their own verdict, counted by the error's stable name (`ssi_unresolved`, ...),
    and are not failures. The total reads `pass P of N; refused R (by kind); failed F`.
  - `INCOMPLETE`: a truth vertex (branch, isolated point, region boundary) farther than the
    coverage tolerance from the output. `h_truth` is that distance.
  - `WRONG TOPOLOGY`: output farther than the coverage from the truth (`h_out`); or the
    components differ (curves joined at ends within the coverage, points joined to what they lie
    on, compared as a sorted list of free-end counts); or a truth singular point has no output
    curve, node or touch within the coverage; or a non-finite sample.
  - `BAD CERT`: a traced branch's certificate is not borne out. Its `to_intersection` must be at
    least the observed distance from its samples to the truth (less the truth polyline's own chord
    error, measured from its turning angles); each pcurve's observed residual `|S(p(t)) - c(t)|`
    must not exceed its `Within` bound, and an `Exact` pcurve's must be under 1e-9.
  - `INACCURATE`: `h_out` above `acc_tol` (plus the chord error).
  - Topology is reported as truth and output counts `branches/nodes/touches`.
  - A coincident answer's pieces are drawn on the first surface (edges in 32 steps). Where the
    truth has a region, its boundary must be covered; where it has none, the pieces are held to
    the truth like any curve (a piece of no width along a seam is then a curve). The check is
    one-sided for regions (the cuts between pieces are not on the truth's boundary).
- **Hausdorff** is both ways, point to polyline segment: output samples (720 a closed-form
  conic, 32 a knot span of a traced branch) to the truth's polylines, and the truth's vertices to
  the output's segments.
- **SM0's truths.** Where a case has a closed form in `scx_testkit::ssi_truth` the scorer
  uses its `to_truth` and `from_truth` (exact curve, 720 samples) in place of the corpus
  polylines for the curve distances. Today that is two-sphere pairs (`sphere_sphere`); no
  corpus case with a circle is one (the touches have no curve), so it is exercised by an
  inline test, where the corpus's coarse polygon would have failed the exact circle. The other
  constructors (perpendicular cylinders, Viviani) need the case's placed axes mapped to a
  `Rigid`: open, for the cases `c1_cyl_cyl_two_loops`, `c4_steinmetz`, `c4_viviani_figure8`.
- **Time.** Measured around the call and printed after the table, apart from it, because a time
  is not deterministic. `SCX_SSI_TIMES=1` adds a time column. The table has no time otherwise, so
  two runs print the same bytes.

Running it: `SCX_SSI_CORPUS=<dir> cargo test -p scx-ssi --test corpus -- --ignored --nocapture
score_corpus` (the full run, about 11 s in a debug build; `SCX_SSI_CASES=a,b` limits it,
`SCX_SSI_OUT=<file>` also writes the table). Without `SCX_SSI_CORPUS` the tests that run by
default are the scorer's own, on inline cases (no corpus file): verdicts for each kind of answer,
a bad certificate, a refusal scored by kind, a deterministic table. With it, a smoke subset
(four cases, one of them refused) also runs, and the surfaces are checked against the truth.

### 16.2 Today's score (113 cases, closed forms only)

```
pass 35 of 113; refused 72 (ssi_unresolved 72); failed 6
  (incomplete 1, wrong topology 5, bad cert 0, inaccurate 0, setup 0)
for comparison, over all 119: the pin crate passes 118, the SMLib port 78
```

Not "almost every case Unresolved": the closed forms answer every pair of a plane with an
analytic surface, the coaxial pairs and the coincident ones, which is 41 of the 113. They score 35;
the other 72 are refused `ssi_unresolved` (every NURBS pair, and the analytic pairs with no closed
form: two cylinders, a sphere and a cylinder, the tori).

| class | pass | refused | failed |
|---|---|---|---|
| c1 transversal (8) | 6 | 2 | 0 |
| c1n (6) | 0 | 6 | 0 |
| c2, c2n (10) | 3 | 7 | 0 |
| c3, c3n (7) | 3 | 4 | 0 |
| c4, c4n (6) | 1 | 5 | 0 |
| c5, c5n (8) | 6 | 2 | 0 |
| c6, c6n (7) | 4 | 3 | 0 |
| c7 (24) | 5 | 13 | 6 |
| c8, c8n (12) | 7 | 5 | 0 |
| c9n (6) | 0 | 6 | 0 |
| y23, the 19 reconstructions | 0 | 19 | 0 |
| total (113) | 35 | 72 | 6 |

Total time of the run, in a debug build: about 150 ms.

### 16.3 What the six failures say (not scorer faults)

All six are class 7, and all six are the closed forms' tolerance rule, not the scorer: the truth
points lie on both built surfaces, and the answer is a legitimate one at the tolerance used.

- `c7_sunk_cyl_1e-3` to `1e-6` (truth: two lines): the plane cuts the cylinder so shallowly
  (the first case: 5e-7 deep, the lines 2e-3 apart) that the strip between the lines is no
  deeper than the tolerance. The rule of 63 (such a strip is one tangent line) answers one line,
  the case's own size from the truth. The corpus counts the lines' separation as the feature.
- `c7_tiny_loop_sphere_1e-4`, `1e-5` (truth: a closed loop of that radius): the cap is
  about 5e-9 deep, so the rule answers a touch point.

These are the case 73 §4.6 and §10 already name (depth against width). They are SM2's to settle:
either the corpus's width is the measure for a closed form's answer, or these cases are N/A at
the tolerance used. Change the tolerance rule only with a decision on that, not to move the score.

### 16.4 Decisions, for and against

1. **The corpus is read from a folder, not vendored.**
   - For: 9 MB of gzip would sit in a repository the other corpora do not weigh; the licence is
     permissive, so nothing stops a later copy; the table is the same either way.
   - Against: a clean checkout runs only the inline tests until `SCX_SSI_CORPUS` is set, so CI
     does not run the full score. Held: SM11 and the status row name the variable.
2. **Decompress with `gzip -dc`, not a built-in inflate.**
   - For: no external crate (layers.txt `[external]` is empty) and no 300-line inflate to get
     wrong.
   - Against: a tool outside the build (every machine here has it). A `.json` folder needs none.
3. **The tolerance is per case, `min(1e-6, feature_size / 10)`.**
   - For: a feature below the tolerance is not resolvable by any method, so the corpus's tiny
     cases would be N/A; this resolves them and leaves the others at the pin's default zone.
   - Against: a case-dependent knob. Held: it is one line (`Case::tol`) and the cases that
     change are the 24 with a feature size, all in class 7.
4. **Component comparison joins at ends, not along curves.**
   - For: it is the corpus's "free-end count" notion and cheap.
   - Against: two curves that cross mid-way without a node are two components in both truth and
     output, which agrees, but a truth curve that touches another mid-way would not join.
     Not met in the corpus.
5. **A coincident answer's pieces are drawn on the first surface and held to the truth only
   where the truth has no region.**
   - For: it scores a piece of no width along a seam (the closed forms' answer for
     `c5_cyl_quilt_seam`) as the curve it is, and checks a region's boundary is covered.
   - Against: the cuts between pieces are not checked, so extra area outside the truth's region
     passes. An overlap's area test needs the polygon in both uv and is SM9's.
6. **Certificate soundness is judged against the truth polylines, with their chord error.**
   - For: it needs no implicit form for each truth kind (`param`, `heightfield`).
   - Against: a truth polyline is itself an approximation, so a claim tighter than its chord
     error cannot be refuted; the chord error is measured and subtracted, and is at most the
     coverage. Nothing here is a certificate until SM8, so this path is exercised only by the
     inline tests.
7. **Zero cost on `xtask fast`.** The full run is `#[ignore]`d. The inline tests take 0.7 s
   in a debug build; with a corpus the smoke subset adds four cases and the surface checks.

## 17. SM2 as built (11-27)

Branch `sm2`. §4.2's closed forms are in `scx_ssi::analytic`, in front of the general path:
each runs only where the dispatch answered `General` before (two curved surfaces off one axis,
and a torus's plane past its circles), so no pair that was solved changes its path.

### 17.1 What exists

- `analytic/quadric.rs` (new): `pair` for two curved surfaces whose axes `coaxial` found apart.
  - **Parallel cylinders.** `b`'s axis turned onto `a`'s direction moves no point of its extent
    by more than `delta = tilt * reach`; within the tolerance the pair is two circles in `a`'s
    cross-section, solved by `coaxial::circles` (the meridian meet) with slack `tol + delta`, and
    each meeting point is a line along the axes: two lines, one tangent line, or empty. The
    coincident pair stays `coaxial`'s.
  - **Miller-Goldman.** A cylinder or cone tangent to the sphere `S = |x - O|^2 - rho^2` along a
    circle is `S = L^2`, `L` linear (a cylinder's `L = (x - O) . z`, a cone's
    `((x - O) . V - rho^2) / sqrt(|V|^2 - rho^2)`, `V` its apex from `O`). Two of them meet on
    the planes `L_a = +- L_b`, so the answer is `a`'s plane sections (`section::cylinder`,
    `section::cone`, unchanged), each lying on `b` too. Two cylinders of one radius whose axes
    meet (the Steinmetz pair, and any angle between the axes) are this case: the bisecting
    planes, two ellipses of semi-axes `r / sin(phi/2)` and `r / cos(phi/2)`.
  - The branch points are where both circles of contact meet (`L_a = L_b = 0` on the sphere): the
    two points `y0 +- along * half` on the line of the planes.
- `section.rs`: `torus` adds the **Villarceau** planes. A plane through the centre at
  `sin t = r / R` from the axis (within the tolerance: `|d| + |turn| (R + r)`) gives two circles
  of radius `R` about `+- r e_t`, crossing at the two points of tangency `+- R cv e1`. `clipped`
  (new, shared) cuts the curves at branch points and adds them as points.
- `clip.rs`: `Shape::param` (a point's parameter on a line, conic, parabola or hyperbola), and
  `Raw::branch_at`. A curve through a branch point is cut there, and each piece is oriented
  along its own `n_a x n_b`, which turns over across the node.
- **Branch points are `IntersectionPoint`s with `Contact::Tangent`**, beside the curves through
  them (the surfaces are tangent there).
- `tests/corpus/score.rs`: the component signature joins both sides' curves through the case's
  singular points (decision 6).

### 17.2 Decisions, for and against

1. **The forms run only after `shared()` fails, and at the torus's last `None`.**
   - For: the plane sections and coaxial pairs keep their path and their answers bit for bit;
     the added work there is a branch-list check per raw (measured: none).
   - Against: a pair that is both coaxial-near and Miller-Goldman-near goes to `coaxial` first,
     which answers `General` past its own tolerance. That is the right answer for both.
2. **One test, "moving `b` onto the closed form's shape moves no point by more than the
   tolerance", decides each form, as SS3's axis test.** Parallel: the tilt times the reach.
   Miller-Goldman: the axes' distance plus the two spheres' radii apart (`b` moved through `a`'s
   nearest point and given its radius). Villarceau: the plane's offset plus its turn times
   `R + r`. Between the tolerance and that slack the answer is `General`.
   - For: one measure across the forms and the coaxial pairs; an `Empty` is answered only where
     every point is apart by more than the widened tolerance.
   - Against: the radii term for cones measures the sphere, not the surface; a cone's surface
     moves by at most that over the reach, so it is no looser than the cylinder's.
3. **A Miller-Goldman pair whose sections hold a tangent piece or a touch is `General`, and so
   is one whose circles of contact touch (one node within the tolerance).**
   - For: there one plane runs along a generator of `a`, and the surfaces touch along a line the
     sections' depth rule does not measure on `b`; answering it would decide on `a` alone what is
     a contact of both. The general path (and SM5's tangent tracing) is the right owner.
   - Against: a real case (cone and cylinder whose contact circles touch) stays unsolved by a
     closed form. Test `miller_goldman_branch_points_merge_and_part` pins it as `General`, with
     two nodes and none on either side.
4. **Villarceau only on the whole tube.** A tube extent that is part of the turn is `General`.
   - For: the circles' parametrisation in the tube angle is not linear, so the clipper's
     half-line sides do not hold; the whole tube is the modelling case (pipes, rings).
   - Against: a torus face split at the tube seam into halves loses the form. The clipper's
     tube-angle side would need the circles' own sides; left open.
5. **The depth rule is unchanged; SM10's six class-7 failures are not the closed forms' fault.**
   The cases' depth (a 5e-7 strip, a 5e-9 cap) is under the tolerance the corpus runs at, so no
   method can tell the two lines or the loop from one tangent piece there; it is the truth at
   that tolerance, and these are N/A until the corpus's tolerance resolves them.
   `the_depth_rule_resolves_at_a_finer_tolerance` shows the same pairs give two lines and a
   circle at a tolerance under their depth.
   - For: truth over pin; the corpus's width is not a measure of separation along the surface.
   - Against: the corpus counts them as failures; its per-case tolerance (`min(1e-6,
     feature / 10)`, 16.4 decision 3) uses width, not depth. Changing that is SM10's, open.
6. **The scorer joins both sides through the singular points.** The truth's two Steinmetz
   ellipses are closed and the answer's four arcs end at the nodes; joining only at free ends
   made the truth two components and the answer one.
   - For: the union is one connected set at a node, however a method cuts it; applied to both
     sides, and `singular_ok` still holds the answer to passing through each node.
   - Against: a scorer change in a form's own PR. It moves no other case (the table below).
7. **Each piece across a node is oriented on its own.** `n_a x n_b` vanishes and turns over at
   a node, so one orientation per conic is wrong on half of it.

### 17.3 Tests

`tests/quadrics.rs`, 16 tests, each against SM0's `ssi_truth` (curves, nodes, touches) where it
has the pair and always against the implicit forms (every answer point on both surfaces;
every generator of `a` meeting `b` inside both extents covered, at least 100 points):
parallel cylinders (two lines, across both tangencies, tilted past the tolerance), Steinmetz
(four arcs and two nodes; clipped to one node), equal cylinders at five angles and across the
tolerance, Miller-Goldman cylinder and cone, two cones, the nodes' merge and parting, across the
tolerance, other crossed cylinders `General`, Villarceau against the truth, across the
tolerance and on part of the tube, and the depth rule at a finer tolerance. No test removed.
`coaxial.rs`'s `near_axes` changed on purpose: its cylinder tilted by 1e-3 still meets the
first's axis (at `z = -1`), so it is now the Miller-Goldman pair (solved) rather than `General`;
the same tilt moved off the meeting keeps the `General` check; its square cylinders of one
radius are now the Steinmetz pair (solved), and two radii keep the `General` check.

Corpus (`score_corpus`, 113 cases): before `pass 38; refused 69; failed 6`, after
`pass 40; refused 67; failed 6`. `c2_cyl_cyl_external` (refused to pass) and `c4_steinmetz`
(refused to pass, with decision 6; wrong topology without it). No case changed otherwise.

### 17.4 Added cost (release, throwaway harness, best of 5 x 200,000 calls, not committed)

| path | before | after |
|---|---|---|
| plane x cylinder, ellipse | 575 ns | 569 to 574 ns |
| plane x half cylinder | 905 to 910 ns | 900 to 910 ns |
| plane x torus, circles | 952 to 968 ns | 960 to 963 ns |
| plane x cone | 600 to 613 ns | 603 to 612 ns |
| coaxial cylinder x sphere | 1443 to 1463 ns | 1445 to 1460 ns |
| skew cylinders (`General` both) | 190 to 193 ns | 192 ns |
| cylinder x sphere off axis (`General` both) | 267 to 270 ns | 270 ns |

Within noise on every existing path.

### 17.5 Open

- Contact-circle tangency (decision 3) and part-tube Villarceau (decision 4).
- The corpus's per-case tolerance by depth rather than width (decision 5), SM10's.
- `scx-boolean` takes the branch points as touches; the pairs were `General` before, so no
  built case changes, but a face pair through a node now reaches D5's contacts.

## 20. SM4b as built (11-27)

The perf follow-up of SM4 (§15.4, §15.5): cell-level pairing within a small factor of the pin, and
the near-touching analytic pairs cleared at the cells. Results only equal or tighter; no pair is
dropped without proof.

### 20.1 What changed

- **Hull ranges for NURBS pieces** (`scx-surface`, `hull.rs`). `Surface::hulls()` takes the
  Bézier patches of `nurbs_invert` (each homogeneous control coordinate with its error bound),
  divides each point by its weight in interval arithmetic (the weight's interval must be
  positive, else no hulls), and keeps the points as boxes. `Hull::range_along(dir)` is the
  least and greatest interval dot product, outward rounded: certified by the convex-hull
  property of a rational Bézier patch with positive weights, and as cheap as the control points.
  `Hull::bbox()` is the box of the points. `Hull::axis()` is the mean of the unit normals of
  the net's quadrilaterals (floating-point: any direction proves, a good one proves more).
  `Hulls::containing(uv)` finds the patch by its rectangle. Only a NURBS surface has hulls: an
  analytic kind's rational image rounds its data (§15.2 item 4), so the kinds keep their own
  formulas.
- **Piece cache** (`general/pieces.rs`). A `Piece` now carries two slabs, each an axis, the
  piece's own certified range along it and the coincidence scaled by an upper bound of the axis's
  length, computed once when the piece is made (before: twice per pair, per piece). The first
  axis is the hull's mean normal for a whole patch (else the surface's normal at the middle), the
  second the surface's normal at the middle where that differs. A pair is dropped when any of
  up to four slabs separates it; each costs one range of the other piece (a hull range for a
  whole patch, `range_along` for a half). A whole patch takes its box from the hull; a half
  made by halving takes `box_over`. `Piece` has private fields now: `Candidates::piece_of_a` and
  `piece_of_b` make a piece for a uv rectangle.
- **Tight cos and sin on an arc** (`scx-curve`, `Arcs::cos_sin`; `scx-num`, `Ring`). A range
  that lies on one quarter arc has monotone `cos` and `sin`, so its result is the hull of the
  formula at the two ends, widened by `8 eps` (the formula's constants are rounded, which can
  put its turning point a few ulps inside the arc: a second-order excess). The interval form of
  the same formula was about 20 percent wide, because `tau` appears in the numerator and the
  denominator. `Ring` gains two provided methods (`range_ends`, `join_padded`) that only `Ivl`
  overrides; a ring of points evaluates as before. `range_along` takes its natural range from
  the interval formula (now tight) in place of the ring-of-values one.

### 20.2 Decisions, for and against

1. **Hull and surface box are not met for a whole patch; the hull stands.**
   - For: cheaper (no `partials_over_piece` per cell), and the pairs came out no more than the
     met form's in every case (below).
   - Against: for a strongly curved patch the Taylor-form enclosure can be tighter than the
     control hull; a half patch still takes that form.
2. **Two axes.** One axis (the hull's mean normal) gave 2,168 pairs over the corpus; adding the
   surface's normal at the middle gave 2,081 (the pin: 2,083) for about a third more time.
   Against: the extra range per pair; it is cheap on a hull.
3. **Where a hull is not valid it is not used**: a piece that is part of a patch (halving) or
   whose rectangle is not exactly a patch's. The rectangle is compared as stored, so a clipped
   extent falls to the surface's own enclosure, never to a looser hull.
4. **Tight arcs by a hook on `Ring`, not by a new interval type.** The kinds' `eval` is generic
   in the ring, so the hook is what reaches it without a second formula per kind.
   - Against: `scx-num` gets two methods only `Ivl` uses, and the monotone claim rests on the
     caller passing a range on the arc (checked in `cos_sin`, which falls back outside it).
5. **The axis hint of halving may now be a hull axis.** The halving rule picks the axis from the
   best slab; with four candidates the hint differs on some pairs, so a refusing NURBS pair spends
   a few units more or fewer (two unit NURBS spheres on a diagonal at `2^-4`: 239 against 221
   units, both refused). No pair is dropped that was not before.

### 20.3 Tests

`apart` tests all pass, with changes of premise in two other places that assumed the loose boxes:
`scx-boolean`'s `a_block_just_clear_of_a_torus_is_dropped_pair_by_pair` no longer finds the pair with
the block's `x = 8` face (the torus's box is exact, `|x| <= 6.5`, so stage 1 drops it; five of six
faces stay), and `scx-surface`'s `nurbs_boxes_hold_the_surface` asserted the analytic torus box within
5 percent of the NURBS one; now it is within the NURBS one's width, which is up to 12 percent more
(the quarter arcs' control hull). In `apart`, two changes of premise: `halving_proves_what_the_cells_cannot` now
uses spheres `2^-4` apart along a diagonal (axis-aligned near-touching spheres are cleared at
the cells now, so the old premise, that the cells do not prove them, no longer holds), and
`a_dropped_pair_is_apart_by_sampling...` builds its pieces with `piece_of_a` and `piece_of_b`.
New: `near_touching_analytic_spheres_are_cleared_at_the_cells` (`2^-10`, `2^-20`, `2^-29`: one
unit, no halving), `a_tilted_near_touch_is_still_cheap`; in `scx-surface` `hull.rs`
(`hulls_hold_dense_samples`: every patch of a torus and a sphere as NURBS, 25 by 25 samples,
four directions, and the box; `a_bilinear_patch_has_its_exact_range`: the range along four
directions is the corners' to `1e-14`; `analytic_kinds_have_none`; `hulls_are_deterministic`) and
`enclose.rs` (`a_spheres_extrema_are_exact`: whole sphere `[-r, r]`, octant `[0, r]`, ranges
along z and x, to `1e-13`; `a_cylinder_quarter_is_exact`); in `scx-curve`
`a_range_on_an_arc_has_the_tight_cos_and_sin` (holds 40 samples per range, width within `1e-13` of
the ends' difference, and across a break still holds).

### 20.4 Measured (release, throwaway harness, tolerance 1e-9)

119 corpus cases as exact NURBS patches, pairing at the cell level (`Candidates::new`), the pin's
pieces and `pieces_apart` for comparison (the same run, so the machine is the same):

| | pin | SM4 | SM4b |
|---|---|---|---|
| median per case | 7.3 to 8.2 us | 125 us (17.7 times the pin's) | 8.4 us (1.4 times the pin's) |
| total, 119 cases | 10.9 ms | 190.9 ms | 8.5 ms |
| surviving pairs | 2,083 | 2,736 | 2,081 |

Run to run on a shared machine the medians move by 20 percent (the pin's own between 7.2 and 10.9 us);
the ratio and the totals are what hold. SM4b never keeps a pair SM4 dropped, and keeps fewer in 22
cases; it keeps more than the pin's uncertified control points in 2 (`c9n_many_spans_plane_sphere`,
582 against 580; `y23_t3_ex2_torus_ellipsoid`, 72 against 68). The results of the full `intersect`
(refusal or proof) are unchanged in all 119. With one axis only (the hull's mean normal) the totals
were 6.9 ms, median 8.7 us, and 2,168 pairs.

Two unit spheres `2^-10` and `2^-20` apart (analytic kinds, `intersect` end to end):

| | SM4 | SM4b |
|---|---|---|
| `2^-10` | 4,305 units, 63 ms | 1 unit, 0.14 ms |
| `2^-20` | 9,089 units, 134 ms | 1 unit, 0.14 ms |

The same on a diagonal (boxes overlap at the nearest points) stays hard: `2^-4` 1,025 units
before and 429 after (both proved), `2^-6` refused (the `FRONT` policy) before and 813 units proved after,
`2^-10` refused by both. Halving of NURBS pairs still uses `partials_over_piece`; its cost is not changed here.

### 20.5 Open

- Hulls for the halves of a patch (de Casteljau of the homogeneous interval points) would make the
  halving rounds as cheap as the cells.
- Exact extrema over a uv box for a kind in a general frame (the sphere in a tilted frame keeps the
  interval form's dependency between the axes' terms).
- SM5's loop-free stop replaces `FRONT`; the diagonal near-touch is the case to try it on.

