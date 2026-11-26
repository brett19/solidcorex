# solidcorex: testing

How a fork would prove it works as fast as possible. A design proposal. The time the current suites
take end to end has not been measured, so claims about speed here are about structure, not numbers.

## 1. What the golden runs cost today

solidcore pins behaviour with bit-for-bit dumps across 283 golden units (the goldens crate is about
430,000 lines), plus about 8,650 test cases in and beside the kernel.

The dumps pin more than answers. Seen in one week of work:

- rows moved because a cache flag changed from filled to empty;
- rows moved because one edge landed in a different storage slot, shifting every index-based digest;
- rows moved by 4e-11 in a tolerance and 2e-12 in a mass.

Each needed a derivation, a ledger digest, and edits to other modules pinning the same rows. Most of the
effort on every results-changing change went into rows where nothing a user could observe had changed.

They are also coarse about meaning: a run of 49 chosen units reported "nothing moved" for a change that
did move a point by 2e-6. It took all 283 to see it.

## 2. Principle

Test each layer against an oracle that is independent of the kernel, at the level where the property is
cheap to state. Keep whole-body comparison for a small set.

## 3. By layer

| Layer | What is tested | Oracle |
|---|---|---|
| `num` | arithmetic, transforms, interval operations | exact identities; for random inputs an interval result contains the true value |
| `algo` | solvers, quadrature, bisection | functions with known roots and integrals |
| `nurbs` | evaluation, derivatives, knot insertion, fitting | a slow reference evaluator; derivatives against finite differences; insertion leaves the curve unchanged; batched equals single, bit for bit |
| `curve`, `surface` | each kind's evaluation and closed forms | analytic formulas; evaluate then project returns the parameter |
| `geomq` | projection, intersection, certified bounds | a sampled distance never exceeds a bound; analytic pairs have known intersections; the ratio of bound to sampled distance is tracked so looseness is visible |
| `brep` | construction, edits, sharing | invariants: adjacency consistent, Euler count, every trim certified, tolerances cover real gaps, an edit leaves untouched faces identical |
| `topo` | trims, face trees, validity, mass | the checker's findings on bodies broken on purpose; mass against analytic values |
| operation crates | Boolean, blend, offset, sweep, heal | observable properties (section 4) |
| `record`, `convert` | storage | export then import gives the same body; export of that gives the same record |
| `ops` | feature compositions | the same properties, per feature |

## 4. Operations: properties, not dumps

- **Validity.** Every result passes the checker. The checker must first be read-only and never excuse a
  fault by widening a tolerance.
- **Measures against truth.** Volume, area, centroid against analytic values where they exist; otherwise
  against an independent estimate (a mesh at several resolutions with extrapolation; point sampling).
- **Algebra.** vol(A ∪ B) = vol(A) + vol(B) − vol(A ∩ B). A ∪ B equals B ∪ A. Cutting then joining
  restores the volume. A ∪ A equals A.
- **Geometry of the result.** A fillet face is tangent to its neighbours along its edges and has the
  stated radius at sampled points. An offset face is the stated distance from its source. A sweep's
  sections match its profile.
- **Metamorphic.** Moving the inputs then operating equals operating then moving. Scaling by s scales
  volume by s³ and area by s².
- **Determinism.** The same input twice gives identical bits. Reads in any order give identical answers.
  One worker and many give identical bits.

None of these mentions storage order or cache state, so they survive refactoring.

## 5. Work as a test

Deterministic counters, asserted in tests. Counts are exact and fast; timings are neither. Each of these
would have caught a defect found by measurement in solidcore:

- a repeated query builds no tree;
- a validity check builds each trim once;
- a read makes no body copy and changes no value;
- a proof is not recomputed for identical inputs;
- an edit of three faces leaves the caches of the others in place.

## 6. What remains of the goldens

1. **Bit-exact, where bit-exactness is the contract:** `num`, `algo`, `nurbs`. Small and fast. They guard
   determinism across platforms.
2. **A small characterisation set of whole bodies:** a few dozen. Compared in canonical form: entities
   matched geometrically, values within tolerance, no cache flags, no storage order.
3. **One regression case per fixed defect:** minimal, asserting the property that was violated.

## 7. Before deleting anything

The existing goldens encode behaviour nobody wrote a property for. Convert, do not drop:

1. For every golden case, generate property assertions from the present kernel's output: validity,
   entity counts, volume, area, bounds, within tolerance.
2. Run old and new suites side by side until the new one has caught everything the old one catches
   over a period that includes real regressions.
3. Retire the bit dumps for every layer above `nurbs`.

## 8. Speed

- **Split by library.** A change to blending builds and tests blending and what depends on it.
- **Three tiers.**
  - Seconds: the touched crate's own tests.
  - Minutes: every crate's property tests and the characterisation set. This is the gate for a change.
  - Nightly: the corpus, random operation sequences, comparison with the pre-fork kernel, coverage.
- **Fixtures built in code,** not read from large files.
- **Fixtures shared.** A finished body is immutable, so one can be built once and used by many tests.

## 9. Nightly only

- **Differential.** The pre-fork kernel as a reference, compared by properties within tolerance. The
  best oracle there is for the hard cases, and too slow for every change.
- **Fuzzing.** Hostile input never panics. Random sequences of operations keep every invariant.
- **The full proof as an audit.** Every certificate on every body in the corpus is re-proved; a failure
  is a kernel defect.
- **Coverage.** What no test reaches.

## 10. Where this is weaker

- **Tolerance-based comparison can hide a small regression** a bit dump would catch. The determinism
  check and the bit-exact lower layers are the counterweight. It is still a real loss.
- **Independent oracles for blends are hard.** Tangency and radius can be checked; whether a corner patch
  is the right one largely cannot. That area leans on the differential run.
- **Properties have to be written.** A dump costs nothing to produce; this costs design effort per
  operation.
- **The checker becomes load-bearing.** It must be fixed first, or everything built on it is unsound.

## 11. Not known

- How long the current suites take, by tier.
- How many cases the 283 units hold, and how many have an analytic answer.
- How much of the kernel no test reaches.
