# solidcorex: testing

How solidcorex is proved to work, as fast as possible. Section 1 is about solidcore's goldens, which
solidcorex does not carry; it was measured on 2026-11-25 (section 1.1). The rest is the plan, brought in
line on 11-26 with the decisions in `04-decisions.md` section 1 and with `01-architecture.md` sections 8
to 10 (changed 11-26, see 01 §9). The other suites' times were not measured.

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

### 1.1 Measured, 2026-11-25 (solidcore main d476017e2)

- **Size.** 283 units, 591,833 golden rows (168 MB in `expected/`), plus 302 known-failing cases in 12
  units that run but are not compared. About 381k rows (64%) are seeded generated cases. 3,356 rows record
  an error as the expected result and 189 a rejected input.
- **Mostly hashed.** About 206M numbers are recorded; about 200M (97%) only as per-256-entry block sums,
  which a non-finite entry makes uninformative (solidcraft #17001: 117 of 141 real changes invisible).
- **Time.** CI (16 vCPU): 4 min 15 s for every unit, after about 3 min building the binary from cache.
  Serial: 74 to 81 CPU-minutes (`reports/goldens/times.tsv` from two worktrees). 163 units take under a
  second; the 20 slowest take 70% (`blend.fillet_executive_p3` 350 to 550 s, `intersect.curve_class_p4`
  190 to 340 s, `blend.feature_executive` about 300 s).
- **Churn.** In the month to 11-25, 265 of 2,921 commits changed `expected/` (+646k / -53k lines). The
  cost is the bless, justify and review cycle and the rebase conflicts, not the run.
- **No oracle.** The goldens were seeded from Rust's output on 2026-11-06, when the C++ comparison was
  removed (#2816). They bless invalid bodies (#14676, #17716), and some generators redraw inputs until
  the kernel accepts them (#15395). The cases with real expected answers (`--solidcraft-cases`,
  `--ssi-corpus`) are report-only, not gated.

## 2. Principle

Changed 11-27 (04 §1): results are validated against known truths, not against solidcore's outputs.
A result may change whenever the new one is equally or more correct. This replaces the 11-26 text,
which made the pin the oracle.

- **Against the truth: the oracle.** Exact or higher-precision references, closed forms, known roots,
  integrals and measures, and properties that need no reference: validity, algebra, metamorphic
  relations, determinism.
- **Against solidcore** (the pin, `06-licence-and-source.md`): the comparison harness, `scx-difftest`.
  It is a smoke detector. It finds inputs where behaviour changed, which are then settled against the
  truth (section 3). Agreement with the pin is evidence, not proof, and a disagreement is a question,
  not a failure of scx.

The goal is the fastest answer that does not give up much checking (04, 11-26). Whole-body comparison
is by outcome, never by dump.

## 3. The pin comparison (`scx-difftest`)

Changed 11-26 (04: results need not match solidcore bit for bit; they must be correct and
deterministic; see 01 §9.1).

- **Within tolerance of the pin, not bit-exact.** The default is 1e-12 relative (1e-300 absolute),
  with exact shape: variants, errors, integers, lengths and panics must agree. A test may widen the
  tolerance with `run_tol` and must say why. Exact agreement is still counted and printed. When
  `fmath` replaces the platform's transcendental functions (01 §8.3), layer 0's agreement becomes
  tolerance agreement too.
- **Determinism is separate and exact.** The same input twice gives the same bits (section 6). Being
  near the pin and being repeatable are two checks.
- **Layers 0 to 4 are compared function by function.** Today that covers `scx-num` and `scx-algo`
  (324 tests, 0 mismatches; 01 §9.1).
- **A disagreement is settled against the truth** (changed 11-27). Both sides are checked against an
  exact or higher-precision reference, or a known invariant. If scx is within tolerance of the truth
  and at least as close as the pin, the input is a pin defect. It is excused by a predicate over the
  input, never by a list of fingerprints, and every excused input is checked against the truth on each
  run; a defect that matches no input fails (`tests/diff/pin_defects.rs`; 04, 11-27). Otherwise the
  fault is ours and is fixed. A pin defect may be fixed in scx as soon as it is found; scx does not
  reproduce a known wrong answer to keep agreement.
- **Bodies are compared by outcome**, through the pin-record converter, from the vertical slice
  (01 §10.2 steps 2 to 4) on:
  - topology up to isomorphism;
  - area and volume within tolerance;
  - the two-way distance between results, within the resolution plus the recorded accuracy;
  - validity.
  - There is no comparison of storage order, ids, cache state or dump hashes. Bit-for-bit agreement
    is not a goal (01 §9.1, "past layer 2").
- **Error strings** stay matched to the pin while the difftest compares them, then are free
  (01 §5.3, §12 item 2).
- A test is not edited to make a change pass (04, 11-25). A widened tolerance names its reason.

## 4. By layer

| Layer | What is tested | Oracle |
|---|---|---|
| `scx-num` | arithmetic, transforms, interval operations, `fmath`, hash and `Rng` | exact identities; `fmath` against a high-precision reference, and a table of its values per target; for random inputs an interval result contains the true value; the pin as a smoke check |
| `scx-algo` | solvers, quadrature, bisection, box tree | functions with known roots and integrals; exact references where cheap; the pin as a smoke check |
| `scx-nurbs` | evaluation, derivatives, knot insertion, fitting | a slow reference evaluator; derivatives against finite differences; insertion leaves the curve unchanged; the pin as a smoke check |
| `scx-curve`, `scx-surface` | each kind's evaluation, closed forms, projection, intersection, certified bounds | analytic formulas; evaluate then project returns the parameter; a sampled distance never exceeds a bound; the ratio of bound to sampled distance is tracked so looseness is visible; the pin as a smoke check |
| `scx-brep` | construction, edits, sharing, `Changes` and `carry` | invariants: adjacency and sides consistent, every cell empty after both edit paths, an edit leaves untouched faces shared (`assert_kept`); the pin's provenance and history tests, ported |
| `scx-record` | the stored form | round trip keeps ids and bytes; a second trip changes nothing |
| `scx-bodyq` | mass, classification, validity check, contact | mass against analytic values; the checker's findings on bodies broken on purpose; outcome against the pin as a smoke check |
| operation crates | Boolean, blend, offset, sweep, repair, ... | the properties of section 5; outcome against the pin as a smoke check |
| helpers (`scx-fates`, `scx-features`, ...) | what they add | the same properties, per feature, over the design corpus |

Batched evaluation is out of the first design (01 §9.4), so there is no "batched equals single" test
yet. If `eval_many` is added later, it gets one.

## 5. Operations: properties, not dumps

- **Validity.** Every result passes the checker. The checker only reads (`check` takes `&Brep`) and
  never excuses a fault by widening a tolerance.
- **Measures against truth.** Volume, area, centroid against analytic values where they exist; otherwise
  against an independent estimate (a mesh at several resolutions with extrapolation; point sampling).
- **Algebra.** vol(A ∪ B) = vol(A) + vol(B) − vol(A ∩ B). A ∪ B equals B ∪ A. Cutting then joining
  restores the volume. A ∪ A equals A.
- **Geometry of the result.** A fillet face is tangent to its neighbours along its edges and has the
  stated radius at sampled points. An offset face is the stated distance from its source. A sweep's
  sections match its profile.
- **Metamorphic.** A rigid move or uniform scale of the inputs gives the moved or scaled result,
  within the resolution plus the recorded accuracy. Scaling by s scales volume by s³ and area by s².
- **Kept faces.** The kept-face count of an edit is as stated (01 §2.5).
- **Provenance.** `carry` against the ported provenance tests (01 §2.6).
- **Accuracy.** A result's recorded accuracies are at least the measured distances, and an accuracy past
  the resolution gives the warning, never a refusal (01 §4.2).

None of these mentions storage order or cache state, so they survive refactoring. One regression test is
added for each defect fixed.

## 6. Determinism checks

Changed 11-26 (01 §8.3). The rule is the same bits for any thread count, cache state, call order, or
save and load; across platforms, verified late.

- **Run twice.** Every case runs twice and the stored bytes are compared.
- **Cold and warm.** Every operation test runs cold (a fresh decode of the stored form, an empty
  scratch) and warm (cells and memos filled), and compares the stored bytes.
- **Thread count.** The fast tier runs one case per parallel site at 1 and at 3 workers. The full tier
  runs every case at both counts.
- **Cells.** Each cell has a test that fills it on two threads and compares the bits (01 §3.2).
- **Save and load.** Every case round-trips through the stored form and repeats, comparing bytes.
- **Lints and scans** (clippy `disallowed_methods`, an xtask scan) keep out what would break the above:
  hash iteration, `mul_add` outside `eft`, platform transcendentals outside `fmath`, thread-locals.
- **Across platforms, late:** a golden table of record hashes checked on x86_64, aarch64 and wasm32.
  Until then, `fmath_pin` (every `fmath` function) and `scx-algo`'s `xplat_pin` (expansions, `Affine`,
  extents, polynomial roots, local solvers) pin today's values; both run in `cargo xtask fast`.

## 7. Work as a test, and the performance gate

Deterministic counts are asserted in tests. Counts are exact and fast; timings are neither. Each of
these would have caught a defect found by measurement in solidcore:

- a repeated query builds no tree;
- a validity check builds each trim once;
- a read makes no body copy and changes no value;
- a proof is not recomputed for identical inputs;
- an edit of three faces leaves the caches of the others in place.

**Gate in CI** (01 §9.4, new 11-26). Each difftest and property case records `Report::work` and the
count of cells filled, as reported by the test harness's own `FaceCell` wrappers.

- These are compared with a committed baseline. A rise of more than 10% fails unless the change states
  why.
- The figures are outputs the library already returns, or test-side wrappers. The library keeps no
  counters of its own (04, 11-22). Comparison of work against the pin uses temporary local
  instrumentation.
- **Target (local, not a gate).** At the app switch, the design corpus regenerates in no more total time
  than on the pin, and no single case is more than 1.25 times slower. It is measured at each layer's
  completion from the vertical slice on, and a miss is a finding in `05-status.md`.

## 8. Speed

- **Split by crate.** A change to blending builds and tests blending and what depends on it.
- **Tiers** (changed 11-26: 04 asks for a fast tier on every change, and corpora and the full difftest
  in CI).
  - **Tier 1, seconds:** the touched crate's own tests, the difftest cases for it, one case per
    parallel site at 1 and 3 workers, a handful of property cases. `cargo xtask fast` takes about
    2.2 s warm today (01 §9.3). Every change runs this locally.
  - **Tier 2, CI:** every crate's tests, the full difftest, every case at both worker counts, the
    corpora, the work gate of section 7.
  - **Nightly:** property tests with thousands of cases from the same generators, random operation
    sequences, fuzzing, full audit of every certificate, coverage.
- **Fixtures built in code** by `scx-testkit`, not read from large files, and shared: a finished body is
  immutable, so one can be built once and used by many tests. Corpus data is permissive-licence only.

## 9. What replaces the goldens

solidcorex has no dumps. What remains of solidcore's goldens' job:

1. **The pin comparison** (section 3) for behaviour, function by function and by outcome.
2. **A small characterisation set of whole bodies:** a few dozen, compared by outcome and in the
   stored form, with no cache flags and no storage order.
3. **Record hashes** per target, late (section 6).
4. **One regression case per fixed defect,** asserting the property that was violated.

The pin's goldens are not ported. Their cases are useful as inputs: a property assertion generated from
the pin's output (validity, entity counts, volume, area, bounds, within tolerance) is a cheap way to
use them.

## 10. Where this is weaker

- **Tolerance-based comparison can hide a small regression** a bit dump would catch. The run-twice
  checks and the 1e-12 default are the counterweight. It is still a real loss.
- **The pin is not always right.** It blesses invalid bodies, and where it is wrong the difference is
  recorded as a pin defect, not matched.
- **Independent oracles for blends are hard.** Tangency and radius can be checked; whether a corner patch
  is the right one largely cannot. That area leans on the pin comparison.
- **Properties have to be written.** A dump costs nothing to produce; this costs design effort per
  operation.
- **The checker becomes load-bearing.** It must be right first, or everything built on it is unsound.

## 11. Not known

- How long the suites other than the goldens take, by tier (the goldens: section 1.1).
- How many of the 591,833 golden rows have an analytic answer.
- How much of solidcore no test reaches.
