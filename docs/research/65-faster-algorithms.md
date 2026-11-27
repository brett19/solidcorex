# 65 - Faster algorithms: what the pin does, and what is known to beat it

Scope: the algorithms solidcorex has ported or will port in steps 2 to 5 (63's NS, CU, SU, SS, BR
tasks, and the step-5 marcher and fitter), each held against the published alternatives. The bar
for an alternative is CAD-sized inputs: degrees 1 to 15, a few hundred control points, tolerances
1e-9 to 1e-12. Under 04 §1 ("truth over pin") an alternative may change results if it is equally or
more correct against the truths, and it must stay bit-identical across x86-64, aarch64 and wasm32
(no fast-math, no platform intrinsics in result paths).

Labels: READ (from the pin's or scx's code; pin paths are relative to the pin's `solidcore/src`
unless another crate is named), MEASURED (figures already in 05 or the research it cites; nothing
was measured for this report but section 2.6, added with the change it records), INFERRED
(operation counts and estimates made here). Every gain below is INFERRED unless marked otherwise.
"Bit-identical" means the change leaves every result bit unchanged, so it needs no truth check
beyond the difftest.

## 1. Ranked summary

Ranked by expected gain times confidence, for work solidcorex will actually run.

| # | Algorithm | Current (pin and scx) | Alternative | Expected gain | Confidence | Verdict |
|---|---|---|---|---|---|---|
| 1 | Bernstein root refinement | Descartes-rule midpoint subdivision, then bisection to rounding: up to 64 de Casteljau evaluations, each O(n²), per simple root | Keep the isolation; refine each isolated simple root by safeguarded Newton (or ITP) on the bracket, evaluating in O(n) | 3-8x on `bernstein_roots` for simple roots | high | adopt |
| 2 | Grid and batched NURBS evaluation | One parameter per call; basis recomputed per point (A2.2, O(p²) with p divisions) | Basis per distinct u and per distinct v, reused across a tensor grid; spans found once per row | about 2x on bicubic grids, more at higher degree; bit-identical | high | adopt in NS2 `eval_many` |
| 3 | Box tree build and self-join | Full sort of each node on its spread axis (O(n log² n)); `self_pairs` runs `pairs(self, self)` and drops half | `select_nth_unstable_by` with the same total key (O(n log n)); a true self-join traversal | build 2-4x for n ≥ 1,000; `self_pairs` about 2x; bit-identical | high | adopt |
| 4 | Curve fit refinement loop | One knot added per round (the worst span halved), a full refit each round; banded LU without pivoting on the normal equations | Add a knot to every failing span per round; banded Cholesky, or banded Givens QR where the normal equations are ill conditioned | rounds O(m) to O(log m) for m spans; Cholesky about 2x the LU | medium-high | prototype and measure |
| 5 | Surface-surface intersection | Port: seeds from tessellation-leaf global solve and 5°/2° angle thresholds, Hermite-checked marching; misses small loops | The pin's own `ssi` crate (normal-cone loop-free subdivision, boundary seeds, proved field and algebraic paths), with its all-pairs piece test replaced by box-tree pairs | correctness first (118 of 119 corpus cases against 78, MEASURED in the pin's docs); pair culling O(N_A N_B) to O((N_A + N_B) log N + k) | medium (speed unmeasured) | adopt the design for step 5; measure |
| 6 | Global solver (subdivision branch and bound) | Depth-first over prebuilt Bezier-leaf trees, box and pseudo-box culls, Newton at leaves, nothing certified; up to 3.4M branches at about 8 µs each | Krawczyk uniqueness at leaf pairs (stop branching once a box holds a proved unique root); Bezier or fat-line clipping inside a surviving leaf pair; first, profile the 8 µs | unknown; the 8 µs per branch suggests a constant factor of 10x is available before any algorithm change | low-medium | prototype and measure |
| 7 | Point projection (NURBS) | Control-net nearest, then nested 7×7, 5×5, 3×3 grids, Newton (35 steps), fallback to the tree solve; no certificate | Best-first branch and bound with distance lower bounds over Bezier patches, seeded by Newton (the pin's `ssi::surface3::closest`); warm start along sequences | fewer evaluations when the incumbent prunes (likely 2x), certified nearest; sequences 5-10x by warm start | medium | prototype and measure |
| 8 | Quadrature in `scx-algo::ode` | `Quadrature` integrates `y' = g(t)` with Cash-Karp RK4(5), 6 evaluations a step | Adaptive Gauss-Kronrod (G7K15, QUADPACK), as the pin's mass integrator already uses | 5-20x fewer evaluations on smooth integrands at 1e-9 | medium-high | prototype and measure |
| 9 | Cubic and quartic closed forms | Graphics Gems: Cardano or Viète (cubic), Ferrari (quartic); no polishing; quartic errors up to 3e-3 on clustered roots (MEASURED, 53 §9) | Flocke (TOMS 954) or Orellana-De Michele (TOMS 1010) quartic, Kahan's trig-free cubic, each with one or two Newton polishes | correctness; the trig-free cubic also avoids correctly rounded trig (scx cubic 1,021 instructions against the pin's 448, MEASURED) | medium | prototype and measure |
| 10 | Enclosures for culling | Control-net AABB and oriented pseudo box per leaf; centred interval Taylor form for partials (`geom/nurbs_enclosure.rs`) | Slefes (Lutterkort-Peters) for culling; Taylor models (Makino-Berz) for the certificates near tangency | tighter boxes mean fewer surviving leaf pairs; size of the gain unknown | low-medium | prototype after 5 and 6 |
| 11 | Span-cached evaluation | None: every evaluation finds the span and rebuilds the basis | Per-span Bernstein (Bezier) cache, evaluated in O(p) (Volk-Schumaker, Woźny-Chudy); not the power basis | 2-4x for repeated evaluation on one span (Newton, marching), at last-bit result changes | medium | prototype and measure |
| 12 | ODE integration | Cash-Karp RK4(5), Numerical Recipes step control | Dormand-Prince 5(4) with FSAL and Hairer's error scale | 10-30% fewer evaluations; fixes the vanishing-start give-up (53 §13) | medium | prototype (low priority) |
| 13 | Bracketed 1-D root finding | Regula falsi clamped to [0.15, 0.85]; damped Newton (step ≤ 0.08 of the interval) | ITP (Oliveira-Takahashi 2020) or Chandrupatla | 1.2-2x fewer evaluations on bracketed cases | low-medium | prototype (low priority) |
| 14 | Degree elevation | Decompose to Bezier, elevate, remove knots (A5.9); far from exact at degree 15 (63 §2.6) | Direct elevation (Prautzsch 1984; Huang, Hu, Martin 2005) | about 2x, and no knot removal, so more accurate | medium | prototype for accuracy |
| 15 | Correctly rounded `fmath` (`cos`, `acos`, `cbrt`) | Double-double evaluation on every call, rounded once | Ziv's fast path: plain doubles with a proved bound, a rounding test, the double-double path on failure (under 2% of inputs) | `cos` -21%, `acos` -13%, `cbrt` -38% instructions; cubic -12%, quartic -6% (MEASURED, 2.6); bit-identical | high | adopted (2.6) |
| - | Krawczyk operator | Classical, midpoint inverse, ε-inflation, N ≤ 4 | Hansen-Sengupta | small | high | keep |
| - | Local N-D Newton | Newton, LU with partial pivoting, step scaling | Levenberg-Marquardt or dogleg | none in speed | high | keep |
| - | Coincident points (`hash_table`) | Uniform voxels, all pairs in a bucket | Sort-and-sweep | only on clustered inputs | medium | keep |
| - | `sort` | std stable merge sort with `total_cmp` | Radix sort on transformed bits | none at n < 10⁴ | high | keep |
| - | Best-fit plane | Scatter matrix, cyclic Jacobi 3×3 | Closed-form 3×3 eigensolver (Smith; Kopp) | small, and less accurate for near-degenerate fits | high | keep |
| - | Knot insertion | Boehm (A5.1), refinement (A5.4) | Oslo algorithm | none | high | keep |

Top recommendations, in order:

1. **Bernstein roots: Newton refinement on the isolated bracket** (section 2.2). The largest cheap
   win, certification kept by a final sign check.
2. **Batched NURBS evaluation over grids in NS2** (section 3.1). Bit-identical, so free to adopt,
   and it is what tessellation, sampling and the fitter's deviation checks call.
3. **Box tree: `select_nth` build and a true self-join** (section 6.1). Bit-identical; it lands
   before the box tree gets its first production caller.
4. **The `ssi` crate's design for step 5's surface-surface intersection, with tree culling**
   (section 5.3). This is first a correctness choice; the speed has to be measured.
5. **Profile the global solver before changing its algorithm** (section 2.1): 8 µs per branch is
   far above the cost of the box tests a branch performs.

## 2. Solvers and root finding

### 2.1 The global solver (subdivision branch and bound)

**(a) Current.** READ: `geom/global_solver.rs` (`solve_branch_body` :1792, `branch_may_contain_answers`
:839, `find_largest_non_leaf` :1330, `identical_solutions_in` :1494); in scx, `scx-algo`
`global_solve.rs`, `split_tree.rs`, `solution.rs`, the same algorithm. It runs a depth-first branch
and bound over one to three trees that already exist: Bezier spans or patches subdivided until the
tessellation tolerances pass (`curve/curve_cache.rs:922` `build_tree`). The trees are not refined
during the solve. A node is culled by its tolerance-expanded AABB (intersection), by box distance
against the incumbent (minimum or maximum), or by bounding spheres (rays). The split takes the
non-leaf node with the largest diagonal and descends to its two children. When every node is a leaf,
the caller's `local_solve` runs Newton from `local_solve1d`/`local_solve_nd`. Duplicates merge by
value within `EFF_ZERO_SQRT` and parameter within `max(100 ε (1 + |s|), 1e-4 L)`. Nothing is
certified. Caps are 1000 summed depth and 2^23 branches. MEASURED (pin doc comment :85-96): fixtures
need at most 56,753 branches, a tiny-scale box-torus boolean 3,408,795, at about 8 µs a branch. scx's
port runs in 0.49 to 0.55 of the pin's time (MEASURED, 05).

**(b) Alternatives.**
- *Bezier clipping* (Sederberg and Nishita 1990, CAD 22(9)) and its quadratic and cubic variants
  (Bartoň and Jüttler 2007, CAGD 24(2); Liu et al. 2009). Inside a pair of leaves these clip the
  parameter interval by the convex hull of a distance function. They converge quadratically to a
  transversal root (proved by Schulz 2009, CAGD 26(1)), against bisection's linear rate. For
  curve-surface pairs the analogue is Nishita, Sederberg and Kakimoto (1990, SIGGRAPH).
- *Interval Newton or Krawczyk as an exclusion and uniqueness test* (Moore; Neumaier 1990,
  "Interval Methods for Systems of Equations"). Once a box is proved to hold exactly one root, branching
  stops there. The pin already has a Krawczyk operator (2.3); it is not wired into this solver.
- *Projected polyhedron* (Sherbrooke and Patrikalakis 1993, CAGD 10(5)) and the preconditioned
  subdivision of Mourrain and Pavone (2009, J. Symb. Comp. 44(3)). Both solve the whole polynomial
  system in Bernstein form, clipping each variable's range by the projected convex hulls.
- *Best-first ordering* for the minimum and maximum modes: a priority queue on the lower bound in
  place of depth-first. The pin already orders children by midpoint distance, which gets most of the
  benefit.

**(c) Gain.** At fixture sizes the trees are shallow (leaves come at the tessellation tolerance), so
clipping gains little: Newton at the leaves already converges quadratically. The deep cases (3.4M
branches) are near-tangent or small-feature cases, where clipping degrades to linear (Schulz) and box
culls fail for the same reason. Krawczyk uniqueness would end branches early only at transversal
roots, which are not the costly cases. The figure that stands out is about 8 µs a branch: a box
overlap test costs nanoseconds, so most of a branch is something else (local solves, the solution
list, allocation). That is a constant factor, likely 5-10x, and it is not an algorithm change.
Confidence: low on the algorithms, medium that the constant factor exists.

**(d) Risks.** Krawczyk exclusion adds certification and changes which leaf pairs reach Newton, so
which duplicate wins a merge can change; results stay within tolerance. Clipping needs the leaf in
Bernstein form, which the curve cache has. Determinism holds if the clip uses only the convex hull
of floats in a fixed order.

**(e) Verdict.** Prototype and measure, in this order: profile the scx solver on the fixtures and
the tiny-torus case; then try Krawczyk uniqueness at leaf pairs; clipping last.

### 2.2 Bernstein roots

**(a) Current.** READ: `scx-algo` `bernstein.rs` (`bernstein_roots`, `roots_in`, :859-930), from the
pin's `query/tangent_edge.rs:1241`. It subdivides at the midpoint by de Casteljau, O(n²) a level, and
uses Descartes' rule on the coefficients. No sign change means no root. One sign change with nonzero
ends means the interval is isolated, and it is then bisected to rounding: up to 64 steps, each a
de Casteljau evaluation of O(n²). Several changes mean halve again. Limits: depth 48 and 4,096
subdivisions. MEASURED (53 §14): about 4,000 ns per quintic in scx, against about 7,000 at the pin.

**(b) Alternatives.**
- *Refinement.* Keep the isolation, but refine an isolated simple root by safeguarded Newton on the
  bracket, or by ITP (2.4). Evaluate in O(n): Bernstein-Horner (Schumaker and Volk 1986, CAGD 3(2)),
  or Woźny and Chudy's geometric scheme (2020, CAD 118, arXiv:1803.06843), which uses only convex
  combinations. The derivative is a Bernstein polynomial of degree n − 1, evaluated the same way.
- *Isolation.* Bezier clipping or quadratic clipping (2.1) converge quadratically. Continued-fraction
  methods (Akritas; Tsigaridas and Emiris) are asymptotically faster than Descartes bisection. At
  degree ≤ 15, Descartes bisection (Collins and Akritas 1976) needs only a few levels, so isolation
  is not where the time goes.

**(c) Gain.** Per simple root, about 52 bisection steps × n(n+1)/2 multiply-adds (n = 5: about 780)
against about 5 Newton steps × 2n (n = 5: about 50). The isolation stage is at most a few levels of
O(n²). So for low-degree inputs with simple roots, bisection to rounding dominates, and the gain on
`bernstein_roots` is 3-8x. High confidence on the count; the wall-clock share is INFERRED and should
be confirmed with the existing `bernstein_roots` bench.

**(d) Risks.** The truth (53 §14) is "found within 1e-12, with an exact sign change within four
ulps". Newton keeps that if it ends with a bracket check: a sign change between `x` and
`x.next_up()` or within four ulps, falling back to bisection when a step leaves the bracket. Last
bits change against the pin, which 04 §1 allows; the testkit's exact dyadic roots are the judge.
Close and double roots keep the current path (`isolated = false`). Determinism: plain f64, no
intrinsics.

**(e) Verdict.** Adopt the refinement. Keep the isolation.

### 2.3 Krawczyk and interval Newton

**(a) Current.** READ: `geom/krawczyk.rs`, `scx-algo` `krawczyk.rs`. This is the classical Krawczyk
operator with the point inverse of the midpoint Jacobian, ε-inflation (×1.5, up to 8 tries), and
closed-form inverses for N ≤ 4. It is generic over `Ivl` and the univariate Taylor model `TmN`
(degree 10). It certifies existence and uniqueness when `K(Y) ⊂ int Y`.

**(b) Alternatives.** The Hansen-Sengupta operator (interval Gauss-Seidel after preconditioning)
gives boxes at least as tight (Neumaier 1990, ch. 5). Rump's `verifynlss` scheme (INTLAB) works on
the residual with slopes.

**(c) Gain.** For N ≤ 4 the operator costs O(N³) on a handful of numbers. A tighter box saves at
most an inflation retry. Small.

**(d) Risks.** None to certification. Results move only in the returned boxes.

**(e) Verdict.** Keep. Try Hansen-Sengupta only if retry counts show up in a profile.

### 2.4 Local solvers

**(a) Current.** READ: `geom/local_solve1d.rs`, `local_solve_nd.rs` and the scx ports.
- 1-D: damped Newton, with the step capped at 0.08 of the interval, an oscillation queue, and a
  bisection fallback; regula falsi with the fraction clamped to [0.15, 0.85]; derivative Brent
  minimisation (Numerical Recipes `dbrent`).
- N-D: Newton with an analytic or central-difference Jacobian, LU with partial pivoting, step clamped
  to the box, and the step scale divided by 10 when the residual grows.
- MEASURED (05): `solve1d` at 161 µs against the pin's 129-133 µs on the bench, after the subnormal
  fix.

**(b) Alternatives.**
- 1-D bracketed: Brent's `zeroin` (Brent 1973), Chandrupatla (1997, Adv. Eng. Softw. 28(3)), ITP
  (Oliveira and Takahashi 2020, ACM TOMS 47(1)). ITP keeps bisection's worst-case count (log₂ of
  width over tolerance, plus a constant) with superlinear average convergence. Clamped regula falsi
  has neither guarantee: its worst case shrinks by 0.85 a step, about 4.3 times as many steps as
  bisection.
- N-D: Levenberg-Marquardt, or Powell's dogleg (MINPACK `hybrj`; Moré 1978). These are more robust
  far from the root, and no faster near it.

**(c) Gain.** 1-D bracketed: 1.2-2x fewer evaluations on the cases where regula falsi stalls, and
none where Newton already converges. The 0.08 step cap slows convergence from distant starts (up to
about 12 capped steps across an interval), but it is a robustness choice tied to periodic domains.
Low-medium confidence.

**(d) Risks.** Every change alters iteration paths, so last bits change. The difftest's call-log
comparison (53 §8) would have to become a truth comparison for those inputs.

**(e) Verdict.** Prototype ITP for the bracketed path, at low priority. Keep N-D Newton.

### 2.5 Closed-form cubic and quartic

**(a) Current.** READ: `scx-algo` `polynomial.rs`, from the pin's `geom/polynomial.rs` (Graphics Gems
I): discriminant quadratic, Cardano with `cbrt` or Viète's `acos` form, Ferrari via the resolvent
cubic. No polishing. MEASURED (53 §9): forward errors up to 3e-3 on clustered quartic roots; the
pin's one production caller filters and drops, never polishes. MEASURED (05): scx cubic 1,021
instructions against the pin's 448; the gap is correctly rounded `fmath` trig.

**(b) Alternatives.**
- Flocke (2015, ACM TOMS 41(4), Algorithm 954): cubic and quartic, accurate by rescaling and Newton
  polishing.
- Orellana and De Michele (2020, ACM TOMS 46(2), Algorithm 1010): quartic by an LDLᵀ-type
  factorisation, faster than Flocke at equal accuracy (their tables).
- Kahan's cubic ("To solve a real cubic equation", 1986 notes): finds one real root by Newton from
  a bracketed start, then deflates to a quadratic. It needs no trig.

**(c) Gain.** Mainly correctness: the clustered-root defect goes away. The trig-free cubic should
also take the cubic below the current correctly-rounded-trig form, likely 1.5-3x. Medium confidence.

**(d) Risks.** Results change wherever the new roots are closer to the truth. The testkit's `poly`
module (exact real-root isolation) is the judge. Fully deterministic in f64 with `fmath`.

**(e) Verdict.** Prototype and measure, together with 05's open item on Newton polishing.

### 2.6 Correctly rounded `fmath`: a fast path before the double-double one

**(a) Current.** READ: every `fmath` function evaluates in double-double and rounds once
(`scx-num`'s `fmath` module docs). The cubic's trig form calls `acos`, `cos` and `cbrt`, and the
quartic's resolvent calls them as well: MEASURED (05), the scx cubic costs 1,021 instructions
against the pin's 448, and the gap is these calls.

**(b) Alternative, adopted for `cos`, `acos` and `cbrt`.** Ziv's strategy (Ziv, ACM TOMS 17, 1991),
in the form CRlibm uses (de Dinechin, Lauter and Muller, RAIRO-ITA 41, 2007), and the one CORE-MATH
uses for its binary64 functions. First evaluate in plain doubles with a known error bound, as an
unevaluated `hi + lo`. If a rounding test proves that every real within the bound rounds to `hi`,
return `hi`. Otherwise run the existing double-double path. `dd::rounds_to_hi` holds the test,
`hi + lo e == hi`, with the derivation of its constant: the test is valid when
`eps < 2^-54 (1 - (1 + 2^-53)/e)`. Here `eps` covers the fast path's error and the double-double
path's error together, both measured against the same exact value. A pass therefore means the
double-double path rounds to the same double, so no result bit can change. No code was copied:
CRlibm and CORE-MATH were cited, not read for code, and the worst-case input lists used in the
check are CORE-MATH's (MIT).

| Function | Fast path | Bound `eps` | Test constant `e` | Valid to | Fallback (MEASURED) |
|---|---|---|---|---|---|
| `cos` | the existing reduction's double-double `y`; table point plus a short Taylor correction in doubles, one exact product | `2^-62` fast + `2^-63` kernel | `1 + 2^-6` | `2^-60.02` | 1.5% of [-4, 4] |
| `acos` | the double-double formulas, one exact product per step, low parts summed plainly and never renormalised | `2^-64` + `2^-64` | `1 + 2^-7` | `2^-61.01` | 0.8% of [-1, 1] |
| `cbrt` | a degree-4 Chebyshev start rounded to 17 bits (so its cube is exact), then a binomial series to `t^4`; no fma | `2^-65` + `2^-95` | `1 + 2^-9` | `2^-63.01` | 0.2% |

**(c) Gain.** MEASURED, 2026-11-27, off the repository: callgrind instructions per call over 4,096
inputs (cos uniform in [-4, 4], acos uniform in [-1, 1], cbrt log-uniform over `2^±60` with either
sign), before and after: `cos` 297 to 234, `acos` 316 to 274, `cbrt` 195 to 119. Back to back on one
pinned core: `cos` about 9% faster, `acos` about 30%, `cbrt` about 12% (as ratios to the platform
libm in the same run, the machine being loaded). The instruction counts understate the `acos` gain,
because the fast path also shortens the dependency chain. In the benchmarks: `algo/cubic` 1,719,599
to 1,512,679 instructions (-12%; the pin 458,535), `algo/quartic` 2,015,626 to 1,893,421 (-6%; the
pin 761,284), the edge groups within 2.3%. New `num_ic` cases `num/cos`, `num/acos`, `num/cbrt`
(against the platform libm): 308,192 to 244,478 (-21%), 327,980 to 286,574 (-13%), 203,659 to
125,647 (-38%, now below libm's 175,949).

**(d) The decisions, each against its alternative.**
- *Reuse the reduction against reducing again in doubles.* The fast `cos` takes `reduce`'s
  double-double `y`, the same one the slow kernel uses. Reduction error is therefore common to both
  paths and drops out of `eps`. A plain-double reduction would save about 50 instructions, but it
  needs its own error bound near multiples of π/2. That bound would also have to cover the slow
  path's different reduction, which would tighten `e` and raise the fallback rate.
- *Exact products only where the leading term needs them.* Rounding the largest product, as a
  plain-double kernel does, costs `2^-53`, which no test can absorb. One exact product per leading
  term (`two_prod`, one `fma` call of about 8 instructions on the default target) keeps it. Dekker's
  split product (about 17 instructions) would avoid the call, but it costs more.
- *The constants `e`.* A larger `e` accepts fewer inputs, and a smaller one leaves less margin over
  the bound. Each constant leaves a margin of 2.6 to 4 over the declared `eps`, and a factor of 7 to
  12 over the error budgets as summed. The measured largest fast-to-double-double difference sits
  below each bound (`cos` `2^-65.4`, `acos` `2^-65.4`, `cbrt` `2^-67.0`).
- *`cbrt`: a polynomial start against a Halley step.* A degree-4 start is good to `2^-14`, and
  rounding it to the `2^-16` grid makes `y0^3` exact in doubles, so the residual `t` is exact up to
  one division. A series to `t^4` then gives `2^-65`. A Halley step from a coarser start would need
  two more divisions. A longer series would need a better start.
- *`acos`: a lean mirror of the double-double formulas against a new polynomial.* An `asin` series
  near 1 needs many terms, and fitting minimax polynomials needs tools (Sollya, mpmath) not
  available here. The lean mirror reuses the table and the arguments for its formulas, and drops
  the renormalisations. The check found two errors in a first version, and both are fixed and
  recorded in `fast_acos`'s doc. One was a root of `1 - x^2`'s high part alone, which left a
  second-order term of `2^-59` near `|x| = 1`. The other was an unnormalised low part, which the
  series, seeing only the high part, would have lost.
- *Which functions.* These three are the ones the closed forms call. `sin`, `tan`, `asin` and the
  rest could take the same fast path, but they are left for when a caller needs them.

**(e) Check (MEASURED, 2026-11-27).** Each function was compared bit for bit with its double-double
path alone, and the long run is `#[ignore]`d in each module's tests:
- 10^8 seeded random inputs per function, in release. The inputs are dense at the hard places:
  arguments near multiples of π/2 and very large ones for `cos`; near ±1, near `x^2 = 1/2`, and
  near 0 for `acos`; random bits including subnormals, near exact cubes, and near binade edges for
  `cbrt`.
- CORE-MATH's worst-case lists (`cos` 1,107,744 inputs, `acos` 264,099, `cbrt` 106,254).

No bit differed, and the 20,000-input default run passes on x86-64, on x86-64 with `+fma`, on
aarch64 under qemu, and on wasm32 under wasmtime. Acceptance on that mixed sample: `cos` 98.13%,
`acos` 99.17%, `cbrt` 99.85%. The fmath and xplat pins, and the full scx-difftest, are unchanged.

**(f) Verdict.** Adopted. The rest of the `fmath` functions can follow the same pattern when a
benchmark shows them.

## 3. NURBS evaluation and edits

### 3.1 Point and derivative evaluation

**(a) Current.** READ:
- Span: `nurbs/knots.rs:320` binary search, O(log m).
- Basis: Cox-de Boor triangular (Piegl and Tiller A2.2), about p² multiplies and p(p+1)/2
  divisions. Derivatives by A2.3, O(k p²).
- Curves by A4.1/A3.2/A4.2.
- Surface points by A3.5/A4.3: both bases, then (p+1)(q+1) blends.
- Surface derivatives (`nurbs/srf_nurb.rs:968`) through a full tensor table,
  O((K+1)(L+1)(p+1)(q+1)).
- No batching, no span cache, no power basis anywhere.
- scx-nurbs matches the pin bit for bit with stack scratch; scx-nurbs-surface has no evaluator yet
  (NS2).

**(b) Alternatives.**
- *Grid evaluation.* On a tensor grid of nu × nv parameters, the u basis depends only on u. Compute
  nu bases and nv bases once, then blend, as in de Boor's "A Practical Guide to Splines" (2001
  edition) and the grid evaluators of SISL and Open CASCADE. Cost becomes
  nu·p² + nv·q² + nu·nv·(p+1)(q+1)·4 multiply-adds against nu·nv·(p² + q² + (p+1)(q+1)·4). Along one
  row the v basis and span are fixed.
- *Batch layout.* `eval_many` over structure-of-arrays control points (x, y, z, w in separate
  arrays) lets the compiler vectorise the blend. The f64 lanes are 2 on SSE2 and wasm simd128, 4 on
  AVX2.
- *Divisions.* The basis divides by knot differences. Reciprocals cached per span would trade p
  divisions for multiplies, but a·(1/b) is not a/b bit for bit.

**(c) Gain.** Bicubic grid: the basis is about 9 multiplies plus 6 divisions per direction (a
division costs about 4-5 multiplies' latency), and the blend 16 × 4 multiply-adds. Reusing the bases
saves about half the work at p = q = 3, and more at higher degree, where the basis grows as p² and
the blend as p·q. High confidence. SoA vectorisation: up to 2x more on 2-lane targets for the blend
only. Medium confidence.

**(d) Risks.** Grid reuse is bit-identical: the same basis code runs on the same u, and the blend
order is unchanged. Vectorising the blend is bit-identical only if each lane keeps the scalar
summation order, which a lane-per-point layout does and a lane-per-term reduction does not. Rust does
not contract a·b + c into FMA, so x86, aarch64 and wasm agree. Reciprocals change bits; that would be
a truth-checked change.

**(e) Verdict.** Adopt grid reuse in NS2 (`eval_many` over a grid, plus a row form for tessellation
and sampling). Prototype the lane-per-point SoA blend after NS2 lands. Reject cached reciprocals
unless a profile shows the divisions.

### 3.2 Span caches and O(p) evaluation

**(a) Current.** READ: none for evaluation. The pin's `curve/curve_cache.rs` caches Bezier spans for
bounding and the global solver only. Newton at a leaf, marching and projection re-find the span and
rebuild the basis on every call.

**(b) Alternatives.**
- Convert each span to Bezier form once (A5.6, already present) and evaluate in O(p) per
  coordinate. The candidates are Bernstein-Horner (Schumaker and Volk 1986) and Woźny-Chudy (2020;
  derivatives in their follow-up papers).
- de Casteljau is O(p²) and the most stable.
- The power basis with Horner is O(p) too, but its condition grows quickly with degree (Farouki and
  Rajan 1987, CAGD 4(3), "On the numerical condition of polynomials in Bernstein form"). At degree
  15 it is unacceptable for 1e-12 tolerances.

**(c) Gain.** For p = 3, O(p) Bernstein evaluation of x, y, z, w costs about 16 multiply-adds plus
the powers, against a basis of 9 multiplies and 6 divisions plus a 16-term blend. The saving is about
2x at cubic and about p/2 at higher degree. Repeated-span workloads (Newton: 3-10 calls; marching:
many) see it fully. Medium confidence.

**(d) Risks.** Results change in the last bits against A2.2. Volk-Schumaker forms `t/(1-t)` powers
and loses accuracy near t = 1, which the usual fix handles by evaluating from the other end past
t = ½. Woźny-Chudy report de Casteljau-level accuracy. The testkit's exact rational evaluation
judges. The memory is a `OnceLock` cell per curve or surface (01 §3), at the cost of (p+1) points
per span.

**(e) Verdict.** Prototype and measure after NS2 and SU4, on the projection and marching workloads.

### 3.3 Knot insertion, splitting, elevation

**(a) Current.** READ:
- Insertion and splitting: Boehm (A5.1), refinement (A5.4), split by full-multiplicity insertion,
  decomposition (A5.6).
- Elevation (A5.9): decompose, elevate each Bezier piece, remove knots. scx-nurbs ports the same
  algorithms. 63 §2.6 records elevation far from exact at degree 15, and 57 §8 a known gap across
  near-coincident knots.

**(b) Alternatives.**
- The Oslo algorithm (Cohen, Lyche and Riesenfeld 1980) for refinement. A5.4 is already the
  efficient form.
- Direct elevation without decomposition: Prautzsch (1984, CAGD 1(2)) and Huang, Hu and Martin
  (2005, CAGD 22(2), "Fast degree elevation and knot insertion for B-spline curves").

**(c) Gain.** Elevation: about 2x by skipping decomposition and removal, and removal (the
ill-conditioned step, `scx-nurbs` `curve.rs:879`) disappears. Edits are not hot, so the value is
accuracy. Medium confidence.

**(d) Risks.** The control points change in the last bits; the testkit's exact elevation judges.

**(e) Verdict.** Keep insertion. Prototype direct elevation for its accuracy at high degree.

## 4. Enclosures

**(a) Current.** READ:
- Culling (curve cache leaves, `nurbs/b_spline_curve/p2.rs:451`, `nurbs/b_spline_surface/p1.rs:1382`):
  the control-point AABB, plus an oriented pseudo box built from the control-point triangle of
  largest area, plus a polar box of normals.
- Certificates (`geom/nurbs_enclosure.rs:68`): a centred interval Taylor form of the partials over
  one polynomial piece, with the rational quotient taken in intervals.
- Procedural curves get cubic Hermite spans per knot span in the curve cache. The hull of a Hermite
  span bounds the approximant, not the true curve, unless an error term is added.

**(b) Alternatives.**
- *Slefes* (Lutterkort and Peters 2001, CAGD 18(9); Peters and Wu): piecewise linear two-sided
  bounds from tabulated second differences. They cost roughly what a convex hull costs, are
  "typically much" narrower (their figures), and narrow by a factor 4 per midpoint subdivision.
- *The control-polygon distance bound* (Nairn, Peters and Lutterkort 1999, CAGD 16(7)), for
  deciding when a leaf is flat enough.
- *Taylor models* (Makino and Berz 2003, Int. J. Pure Appl. Math. 4(4)): enclosures of order n+1,
  for certificates near tangency, where an interval's dependency problem grows. The pin's `TmN` is
  univariate.

**(c) Gain.** Narrower boxes leave fewer leaf pairs surviving near an intersection curve, roughly
in proportion to the width. Without counts of surviving pairs from 2.1 and 5.3 the size cannot be
stated. Low-medium confidence.

**(d) Risks.** Slefes are certified only with outward rounding of the tabulated sums. The tables are
constants, so determinism holds. Porting cost is moderate (tables per degree up to 15).

**(e) Verdict.** Prototype after the global solver and SSI have pair counts to compare against.
Before step 5, add an error term to Hermite spans that are used as enclosures. That is a correctness
item, not a speed one.

## 5. Projection and intersection

### 5.1 Point inversion and projection

**(a) Current.** READ:
- Surfaces (`nurbs/b_spline_surface/p1.rs:2156`, `surface/surface_point_solve/p2.rs:129`, `:1643`):
  - a hint gives a local Newton solve first;
  - otherwise the nearest control-net point plus a polygon refinement, then nested 7×7, 5×5 and
    3×3 grids (83 evaluations);
  - Newton on (S−P)·Su = (S−P)·Sv = 0 with line search, capped at 35 steps;
  - on a wrong minimum, the tree branch and bound (2.1). Not certified.
- Curves: the global solver over the cache tree, then Newton.
- The pin's separate `ssi` crate (`ssi/src/surface3.rs:890`) has a certified closest point:
  best-first, a min-heap over a span quadtree, lower bounds from box distance and support planes, and
  12 Newton steps per popped patch.
- Pcurve checks (`eval/uv.rs:285`) use 16 starts plus 2 per span, up to 32 Newton starts.

**(b) Alternatives.**
- Bezier-based elimination: Selimovic (2006, CAGD 23(5), "Improved algorithms for the projection of
  points on NURBS curves and surfaces"); Chen et al. (2008, CAD 40(10), "Computing the minimum
  distance between a point and a NURBS curve"), which corrects the control-polygon test of Ma and
  Hewitt (2003, CAGD 20(2)).
- The best-first certified branch and bound in `ssi::surface3::closest` is the same idea with
  proved bounds.
- For sequences (sampling an edge for a pcurve, fitting, deviation checks), warm-start Newton from
  the previous foot. This is standard practice (Piegl and Tiller ch. 6) and needs 2-3 iterations per
  sample.

**(c) Gain.**
- Single queries: with a Newton incumbent, the lower bounds prune most patches, so the visits are
  likely tens of patches against 83 grid evaluations plus the fallback. About 2x, and the answer is
  the certified global nearest rather than a good local one.
- Sequences: 5-10x by skipping the starts.
- Medium confidence; the `ssi` crate can be timed directly.

**(d) Risks.** Certification needs outward-rounded distance bounds, which `ssi` has. The answer
changes where the pin returns a farther local minimum; 05 records two spindle tori where it does, and
T3 is the judge. Determinism: the heap needs a total key (bound, then patch index).

**(e) Verdict.** Prototype for SU4: the certified best-first search as the global path, the grids
dropped if it wins, and warm starts for every sequence caller.

### 5.2 Curve-surface and curve-curve intersection

**(a) Current.** READ: `intersect/api_intersectors.rs:1020` → `surface/surface/p8.rs:2668`, and
`curve/curve/p8.rs:841`. Closed forms for analytic pairs; otherwise the global solver over both
cache trees, then Newton; ranges for curves lying on the surface. The ray tracer uses a voxel grid
with a 3-D DDA walk. Krawczyk certification exists for ray-surface (`surface/surface/certified_line.rs`).
Step 2 (SU6) needs only analytic pairs, which reduce to quadratics and quartics (2.5).

**(b) Alternatives.** Bezier clipping for curve-curve (Sederberg and Nishita 1990). Ray-patch
clipping (Nishita, Sederberg and Kakimoto 1990). Projected polyhedron for curve-surface as a system
of three equations in three unknowns (Sherbrooke and Patrikalakis 1993).

**(c) Gain.** As in 2.1: little at fixture sizes, where leaves are already flat; the costly cases are
near-tangent. Low confidence.

**(d) Risks.** As in 2.1.

**(e) Verdict.** Keep the pin's structure for step 5. Use 2.5's polished closed forms for SU6. Fold
any clipping prototype into 2.1's.

### 5.3 Surface-surface intersection

**(a) Current.** READ:
- *The port* (`intersect/surface_intersector/p1`-`p5`, `adv_surface_intersector`):
  - seeds from a global solve over both leaf trees, boundary curves against the other surface, and
    one candidate per intersecting leaf pair;
  - a Gauss-map loop test (Sederberg-Meyers polar boxes);
  - seeds traced only where the normals differ by 5° at the guess and 2° at the solution, so small
    loops between nearly tangent surfaces can be missed (pin `docs/ssi.md` §4.2);
  - marching by tangent prediction with a Newton corrector; each Hermite span checked at 5 interior
    samples on both surfaces;
  - step ÷1.5 on rejection and ×1.5 when easy.
- *The `ssi` crate* (a separate method in the pin, not yet called by solidcore), dispatching in
  order:
  - closed forms;
  - a field path, where g = f_S(P(s,t)) is a Bernstein polynomial and cells are proved by sign,
    monotonicity or degree theory;
  - an exact implicit form up to degree 10;
  - coincidence;
  - subdivision and marching.
- In the `ssi` subdivision, pieces are Bezier patches by knot insertion. All N_A × N_B pairs are box
  tested, then halved depth first until "apart" or "loop-free" (normal cones).
- `ssi` seeds come from piece boundaries against the other piece. Marching uses turn and step
  limits and a corrector held to a plane, and Hermite densifies with no extra solves.
- MEASURED in the pin's own docs: 118 of the 119 corpus cases pass, against 81 for the C++ original
  and 78 for the port.

**(b) Alternatives in the literature.**
- Patrikalakis and Maekawa, "Shape Interrogation for Computer Aided Design and Manufacturing"
  (2002), ch. 5: lattice, subdivision, marching and projected-polyhedron start points.
- Loop detection by normal cones (Sederberg and Meyers 1988, CAGD 5(2); Sederberg, Christiansen and
  Katz 1989, CAGD 6(2)) and by collinear-normal points (Kriezis, Patrikalakis and Wolter 1992,
  CAGD 9(2)).
- Marching step control by curvature (Barnhill and Kersey 1990, CAGD 7(1)).
- Topology by a boundary-value formulation (Grandine and Klein 1997, CAGD 14(2)).
- The `ssi` crate already combines the loop-free subdivision with boundary seeds and proofs; it is
  the strongest of these in the corpus figures.

**(c) Gain.**
- Correctness is the main gain: 118 of 119 against 78, MEASURED.
- Speed: the all-pairs test is O(N_A N_B) box tests. For 400 pieces a side that is 160,000 tests,
  about a millisecond; box-tree pairs (6.1) bring it to O((N_A + N_B) log N + k).
- `ssi`'s Taylor fallback costs about 17 ms a use (pin docs). That is where its time will be, and
  it is the number to measure.
- Medium confidence overall; the speed is unmeasured.

**(d) Risks.**
- Its refusal (`Error::Tangent` below a resolution) is a typed outcome, which fits 62.
- Determinism: depth-first halving on a stack in a fixed order, and the tree's sorted pairs.
- Porting cost is high but planned anyway for step 5. The licence follows 06 like every pin file.

**(e) Verdict.** Adopt the `ssi` crate's design as the base of step 5's SSI, with the port's
analytic pairs (SS1-SS5) in front. Replace its all-pairs piece test with `BoxTree::pairs`. Measure
time on the corpus against the port before deciding whether any of the port's marcher survives.

## 6. Spatial indexing, sorting, small fits

### 6.1 The box tree

**(a) Current.** READ: `scx-algo` `box_tree.rs`.
- Build: top-down and iterative. The split axis is the widest spread of box centres, the node is
  fully sorted with `sort_unstable_by(centre.total_cmp, then index)` (:214), and the cut is at the
  count median. Leaves hold 4 items; nodes are an array of structs; boxes are `Extent3d`. Cost is
  O(n log² n).
- Queries: overlap by stack DFS. `pairs` is a dual traversal descending the larger side.
  `self_pairs` (:343) runs `pairs(self, self)` and keeps `i < j`, so it visits every pair twice and
  every item against itself.
- Output: sorted, so it does not depend on the tree's shape.
- The pin's `SpatialIndex` (`topology/brep/validation/spatial.rs`) is the same kind of tree, with
  the axis cycled by depth and 8 items a leaf. The pin's mesh `Bvh` (`mesh_reduce/collision.rs:148`)
  already uses `select_nth_unstable_by`.

**(b) Alternatives.**
- Partition by `select_nth_unstable_by` (introselect) with the same key: O(n log n).
- Binned SAH (Wald 2007, IEEE Symp. Interactive Ray Tracing, "On fast construction of SAH-based
  bounding volume hierarchies").
- LBVH by Morton codes (Lauterbach et al. 2009; Karras 2012, HPG).
- PLOC (Meister and Bittner 2018, IEEE TVCG).
- A self-join that visits (A, A) as (left, left), (right, right) and (left, right) once.

**(c) Gain.**
- Build: removes a log n factor, 2-4x at n ≥ 1,000 faces and noise below 100.
- `self_pairs`: about 2x on the traversal and no self-pair filtering.
- SAH's published gains are 10-30% fewer node visits for rays. For box self-joins over CAD faces,
  whose boxes are few, large and overlapping, there is no evidence it pays, and builds at these
  sizes take microseconds. LBVH trades quality for parallel build speed, which a CPU tree of 10⁴
  boxes does not need.
- High confidence for the first two.

**(d) Risks.**
- The key (centre, then index) is a strict total order, so `select_nth` yields the same partition as
  the full sort. The same items land in each child; only their order inside a node differs, and the
  sorted output hides that. Bit-identical results.
- The self-join must emit exactly the pairs `i < j` that `pairs` would, and a difftest against the
  current form proves it.

**(e) Verdict.** Adopt both now, before the first production caller (BR4's cells). Keep the median
split. Reject LBVH. Revisit SAH only if a profile shows tree queries.

### 6.2 Coincident points, sorting, best-fit plane

- **`hash_table`** (READ, `scx-algo` `hash_table.rs`). This is uniform voxels with all pairs
  compared in a bucket, O(Σ bucket²). It is quadratic only when points cluster in one voxel, which
  the voxel size prevents for the healing inputs it serves. Sort-and-sweep (sort by x, sweep a
  window of the tolerance) is O(n log n + k) regardless. Keep; switch only if a clustered input
  shows up.
- **`sort`** (READ): `sort_by(f64::total_cmp)`, a stable merge sort, 0.31 of the pin's time
  (MEASURED, 05). A radix sort on transformed bits pays only for n ≫ 10⁴. Keep.
- **Best-fit plane** (READ, `scx-algo` `geom/fit.rs`): the scatter matrix, then cyclic Jacobi on a
  3×3. The closed-form symmetric 3×3 eigensolvers (Smith 1961, CACM 4(4); Kopp 2008, Int. J. Mod.
  Phys. C 19(3)) are faster, but Kopp shows them losing accuracy on nearly degenerate spectra. A
  nearly flat point set is the case a plane fit is for. The 1.22x gap against the pin (MEASURED,
  05) is a constant factor to find in a profile. Keep Jacobi.

## 7. Integration

### 7.1 ODE integration

**(a) Current.** READ: `scx-algo` `ode.rs` from `mass/integrator.rs`. Adaptive Cash-Karp RK4(5),
6 evaluations a step, Numerical Recipes step control (`0.9 h err^-0.2`, cap 5h; on rejection
`err^-0.25`, floor 0.1h). Elsewhere in the pin: a fixed-step RK4 with doubling for geodesics, and
the same integrator for face iso-u trim integrals.

**(b) Alternatives.** Dormand-Prince 5(4) (Dormand and Prince 1980, J. Comput. Appl. Math. 6(1)) in
Hairer, Nørsett and Wanner's form (`DOPRI5`, "Solving Ordinary Differential Equations I", 1993):
7 stages with first-same-as-last, so 6 new evaluations a step. It propagates the fifth-order
solution, uses a PI step controller, and scales the error by `atol + rtol·max(|y0|, |y1|)`.

**(c) Gain.** Hairer et al.'s comparisons put DOPRI5 ahead of the older embedded pairs by roughly
10-30% in evaluations at equal accuracy. The error scale also removes the vanishing-start give-up
that 53 §13 records. Medium confidence.

**(d) Risks.** Every step size changes, so results move within the accuracy. The truths (closed
forms) judge; the `pinf` oracle no longer applies. Deterministic with `fmath::powf`, or with a
fixed-exponent power, which 05 already lists as a follow-up.

**(e) Verdict.** Prototype at low priority, together with the 53 §13 error-scale fix.

### 7.2 Quadrature

**(a) Current.** READ: `scx-algo` `ode.rs` `Quadrature::integrate` treats ∫g as an ODE with
Cash-Karp, plus a fifth-divided-difference estimate. The pin's mass properties use nested adaptive
7-point Gauss-Kronrod with an embedded 3-point Gauss rule (`mass/integrator/properties.rs:1002`).

**(b) Alternatives.** Adaptive Gauss-Kronrod: G7K15 or G10K21 (Piessens et al. 1983, QUADPACK
`QAG`). Clenshaw-Curtis is an option for very smooth integrands.

**(c) Gain.** For an analytic integrand at 1e-9, a G7K15 panel's error falls like h^22. A handful of
panels (15 evaluations each) replaces hundreds of RK steps at 6 evaluations each (MEASURED in 53
§14 only as time: about 10 µs for `1/(1 + c t²)` on [-1, 1]). The gain is likely 5-20x in
evaluations. Medium-high confidence; it depends on who calls `Quadrature` once the mass drivers move.

**(d) Risks.** Results change within the accuracy. Error estimation in Gauss-Kronrod is heuristic
(as is RK's), and the certified volume (`mass/enclosure.rs`) is separate and unaffected.
Deterministic: fixed nodes and weights as literals, and a panel queue in a fixed order.

**(e) Verdict.** Prototype and measure when the mass drivers move. If it wins, use one adaptive
Gauss-Kronrod for every quadrature caller and keep the ODE integrator for true ODEs.

## 8. Fitting and the pcurve fitter

### 8.1 Curve and surface fitting

**(a) Current.** READ:
- Setup: `nurbs/crv_fit/p1.rs`, `p2.rs`, `within.rs` and `srf_fit.rs`. Parameters are uniform,
  chord or centripetal. Knots are averaged (Piegl and Tiller eq. 9.8 for interpolation, 9.68-style
  for least squares).
- Solve: banded normal equations NᵀN solved by Doolittle LU without pivoting
  (`nurbs/math.rs:1552`). Constraints go through Lagrange multipliers and matrix inverses. Surfaces
  are fitted in two stages, columns then rows.
- Refinement (`within.rs` `refit_within`): start with 1 span and 2(p+1) samples a span. Each round
  is a full least-squares fit plus a certified distance per span, then the worst span is halved by
  one knot.
- Deviation is certified (`crv_deviation.rs`: Bezier difference to a common degree with rounding
  terms).

**(b) Alternatives.**
- *Refinement rounds.* Insert a knot in every span over the tolerance each round, as in Piegl and
  Tiller's tolerance-driven approximation (ch. 9.4). With m final spans the rounds drop from m to
  about log₂ m.
- *Solve.* NᵀN is symmetric positive definite for well-posed data, so banded Cholesky costs half of
  LU and is stable without pivoting. Where the parameterisation makes NᵀN ill conditioned, a banded
  Givens QR on N avoids squaring the condition number: Cox (1981, IMA J. Appl. Math. 27); de Boor's
  `L2APPR` ("A Practical Guide to Splines").
- *Knot placement.* Dominant points or adaptive refinement (Park and Lee 2007, CAD 39(6)) reach a
  tolerance with fewer control points.

**(c) Gain.**
- Rounds: a 64-span result needs 64 fits and certified checks one knot at a time, against about 7
  rounds. The fits grow with the spans, so the total falls from O(m²·p) to O(m·p·log m).
- High confidence on the count, medium on the size of the curves: inserting everywhere can add a
  few more spans than the minimum.
- Cholesky: about 2x on the solve, which is a small part of a fit; QR is 1.5-2x slower than the
  normal equations but more accurate.

**(d) Risks.**
- Different knots mean different curves, all within the certified tolerance. The judge is the
  certified deviation, which is unchanged.
- LU without pivoting on NᵀN is safe for SPD matrices, but Cholesky makes the SPD assumption
  explicit and fails loudly when it breaks: a typed `Unstable`.
- Deterministic: fixed loop order.

**(e) Verdict.** Prototype and measure for step 5's fitter: time against the control-point count on
the pin's fitting fixtures. Banded Cholesky by default, with QR as the fallback on an unstable
factorisation.

### 8.2 The pcurve fitter (BR9)

**(a) Current.** READ: the pin has no pcurve fitter. `eval/pcurve.rs` produces exact forms only
(lines and splines on planes, axis circles on revolved kinds, rulings, straight uv segments on spline
faces) and otherwise returns `Failed`. Acceptance lifts 17 parameters plus each span midpoint. BR5
in step 3 carries only exact pcurves; BR9 adds the fitter.

**(b) Design inputs from this survey, for BR9.**
- Sample the 3-D edge, project each sample with Newton warm-started from the previous foot (5.1):
  2-3 iterations a sample, against the pin's grid starts.
- Fit as in 8.1: rounds that refine every failing span, banded Cholesky.
- Certify with the T6 oracle and 8.1's certified deviation.
- Evaluate the surface along the pcurve through the span cache (3.2), since every check
  re-evaluates on the same spans.

**(c) Gain.** Not comparable: there is no pin fitter to beat. Against a naive sample, grid-project
and fit, warm starts and multi-knot rounds are where the time goes.

**(d) Risks.** As in 5.1 and 8.1.

**(e) Verdict.** Design BR9 on these inputs; measure D10's added work as 63 §10 already requires.

## 9. What this report did not do

- Nothing was timed for this report but section 2.6. Every other gain is an operation count or a
  literature figure, and each "prototype and measure" item needs the `scx-bench` harness (02 §8.1)
  with before and after counts.
- The global solver's 8 µs per branch was not profiled. It is the first measurement to make.
- The `ssi` crate's run time on the corpus was not measured, only its pass counts quoted from the
  pin's docs.
- Literature gains quoted for SAH, DOPRI5 and slefes come from their authors' benchmarks, not from
  CAD workloads.
