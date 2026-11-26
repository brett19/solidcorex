# Status of the solidcore work this plan grew from

As of 2026-11-26. The stop of 11-25 was lifted for rewrite work when Brett assigned ownership of the
rewrite on 11-26 (`04-decisions.md`). The solidcore branches below are still as they were left. Check every
item against the repository before relying on it: solidcore's main moves by hundreds of commits a day.

Repository: tangency-labs/solidcraft (kernel under `solidcore/`). Machines, paths, local branches and
raw data are kept outside this repository.

## 1. Merged

| PR | What |
|---|---|
| #20941, #20942, #20943, #20944, #20945 | Stage A: an answer no longer depends on what a kernel did before (fillet latch, random draws, trim loop, size estimate, stale gap caches) |
| #21019 | A kept surface cache is lent to a solve, not deep-copied (one edge fillet: 260 whole-cache copies to 2) |
| #21072 | 22 ops-layer reads use the read door |
| #21107 | Mass, area and face measures read a body that has its trims without a snapshot. **Introduced a read-form-plus-fallback pair that is to be collapsed** (`04-decisions.md` section 4) |
| #21135 | A first validity check no longer repeats its pointer pass (three trim builds to two) |

## 2. Open and parked (drafts)

| PR | What | Why parked |
|---|---|---|
| #21268 | Tight box without a body copy | Another read-form-plus-fallback pair; the copy is 0.3% of a call |
| #21284 | One rule for a trim's tolerance raise (#21161) | Results-changing, 30 rows undecomposed; may be moot if creating a trim stops raising |

## 3. Issues filed

| Issue | Subject | State on 11-26 |
|---|---|---|
| #21104 | Near-tangent union returns an invalid body | Closed as completed 11-23 (others took it: see #21193, #21194) |
| #21105 | A validity check changes the body it checks | Closed as completed 11-24 (see #21494) |
| #21118 | Repeated vertex-to-face gap solves | Closed by us: measured, not worth doing |
| #21131 | A first check builds each trim three times | Open; one build removed by #21135 |
| #21161 | A trim's raise depends on which routine creates it | Open; draft #21284 |
| #21660 | Tight box wrong and slow on offset faces | Closed as completed 11-26 (see #22076) |
| #21926 | A record's stand-in for an offset face can be 21× tolerance away | Closed as completed 11-25 (see #22180) |
| #21927 | Record round trip is not a fixed point | Open |
| #22387 | A vertex on a tangent-field edge gets a tolerance of twice its distance from the origin | Open; fix built locally (below) |
| #13789 (not ours) | Store an offset face as its definition | Open, unscheduled; we added measurements |

Four of ours were closed by other contributors' work within days. **Nobody in this session has verified
those fixes against our reproductions.**

## 4. Local branches and clean-up

Local, unpublished branches of solidcore (the leaf splits, the #22387 fix, the tree-build change, the
plane-trim carry held by Brett, and prototypes) and the clean-up they owe are tracked outside this
repository, with the machines they live on.

## 5. If work on solidcore resumes, in this order

1. Read `04-decisions.md`.
2. Re-base every local branch: main moves fast.
3. Read or rerun the goldens for the #22387 fix.
4. Get reviews for the second leaf split and the containment walk.
5. Recount the moved rows of the tree-build change on current main; finish the derivations.
6. Produce report 48 (why proofs are 38% of regeneration).
7. Check whether the four issues closed by others are really fixed, using our reproductions.

## 6. The rewrite: phase 0 (started 2026-11-26)

Local only (no GitHub remote). Licence: Apache-2.0 throughout, decided 11-26; code is moved from the
pinned solidcore with derivation headers (`06-licence-and-source.md`).

| Item | Output | State |
|---|---|---|
| Module dependency and cycle map of solidcore | `research/49-module-dependencies.md` | done; corrections folded into `01-architecture.md` section 13 |
| Coverage of solidcore over its goldens, tests and the app corpus | `research/50-coverage.md` | started |
| The body's data structure: design and a measured prototype | `research/51-body-structure.md`, `prototypes/body-structure/` | done; representation decided (`04-decisions.md` section 3); not measured on a large STEP part |
| Conventions, glossary, tolerance model | `07-conventions.md`, `08-glossary.md`, `09-tolerances.md` | written 11-26; owner questions listed at the end of each |
| Error taxonomy | to be written | not started |
| Workspace skeleton, layering check, test tiers | `Cargo.toml`, `layers.txt`, `xtask/` (`cargo xtask fast`: fmt, clippy, layers, headers, tier-1 tests), `crates/scx-difftest` (tier 2) | done; tier 1 passes in 1.4 s cold, 0.2 s warm |
| Layer 0 moved: `scx-num` | `crates/scx-num`, `research/52-num-move.md` | done (to 273768e): 128 unit and 10 property tests; difftest 267 tests, 616,874 inputs, 0 mismatches against the pin; builds for wasm32 |
| Interval arithmetic into `scx-num`; review fixes | `crates/scx-num`, `research/53-interval-and-algo.md` | done 11-26: 148 unit, 11 enclosure, 10 property, 1 libm-table tests; difftest 303 tests, 0 mismatches, 97.11% of regions; one non-enclosure found (overflow to NaN), not changed |
| Layer 1 started: `scx-algo` | `crates/scx-algo`, `research/53-interval-and-algo.md` sections 5 and 8 | `polynomial` moved 11-26, bit-identical; the local solvers 11-26 with generic errors (owner's design), 0 mismatches with failures injected at every call; Krawczyk (with `inverse4`) and `hash_table` (over `Tolerances`) 11-26, 0 mismatches. Step 2 done |
| Difftest: deterministic, and within tolerance of the pin | `crates/scx-difftest/tests/diff/harness.rs` | done 11-26 (1cab726): every case runs scx twice (must match to the bit) and is compared with the pin within `Tol` (1e-12 relative by default; same shape exactly). 317 tests, 766,857 inputs: 766,754 exact, 10 within tolerance (`t_extent::e3_line` 8, `t_rest::tf_inv_evaluate_guess_point_simple` 2; all differ only in a NaN's sign or payload), 0 mismatches, 0 nondeterministic. Whole difftest 2.2 s |
| Architecture redone as solidcorex's own design | `01-architecture.md` | v2 committed 11-26 after adversarial review (`research/reviews/review-architecture-v2.md`); ruling in 04 §3; 02, 03, 07, 08, 09 brought in line 11-26 |
| Step 1 of the order of work (`01-architecture.md` §10.1 items 1 to 8) | `crates/scx-num` (`units`, `cert`, `hash`, `eft`, `fmath`), `crates/scx-algo` (`local_solve1d`, `hash_table`, `box_tree`, `executor`), `clippy.toml` | done 11-27 apart from two difftest mismatches, below: `Tolerances` is now `Resolution` plus constants, the fields are private, the sentinels are gone except `INFINITE_PARAMETER` (step 2), there are certificates, a fixed hash and `Rng`, `LocalSolve1d` is an options value with a pure call, `hash_table` iterates in a fixed order, and the box tree and `Executor` exist. `fmath` (item 4) is the transcendental functions in plain `f64` and double-double: every one correctly rounded on the 5,000 inputs per function measured against Python `decimal` at 90 digits, 1.1 to 3.7 times the platform libm's time on the default x86-64 target (tables in `fmath/mod.rs` docs). `tests/fmath.rs` checks them to 0.501 ulp against in-test double-double references (random inputs, 335 hard cases, the C99 special values); `tests/fmath_pin.rs` pins their hashes, the same on every platform. Every call site uses `fmath`, and `clippy.toml` bans the 20 `f64` transcendental methods.
Difftest 323 tests, 767,061 inputs: before `fmath` 767,051 exact, 10 within tolerance, 0 mismatches; after 761,413 exact, 5,491 within tolerance, 157 mismatches, 0 nondeterministic. The 157 are two cases where the pin is wrong, left failing for the owner's ruling rather than tolerated:
- `tr_is_regular`, 156 inputs, all a matrix with an entry `f64::MAX`: scx says `No` (correct: the rows' lengths differ by `2^1024`), the pin `Undecided`. The pin scales by `f64::powi(2, -1024)`, which is `1/2^1024 = 1/inf = 0`, and the zero matrix decides nothing; `fmath::powi` gives the subnormal `2^-1024`. (scx now reads the exponent from the bits, not `log2(..).floor()`.)
- `three_point_circle_2d`, 1 input, points `(1, 0)`, `(1e-13, 1)`, `(1e-6, 1e-6)`: the sweep is `2 asin(q)` with `q` within `1e-12` of 1, which multiplies the radius's last bit by about `10^6`. `fmath::hypot` rounds the radius correctly, libm's does not; scx's sweep is off the true one by `4.8e-11`, the pin's by `1.7e-10`, and the two differ by `7e-11` relative, past the `1e-12` tolerance. A sweep from `atan2(cross, dot)` would be well conditioned. |
| Certified trim-gap bounds for offset and procedural-blend surfaces | `research/55-certified-trim-gap.md`, `prototypes/trim-gap-bound/` | reviewed (`research/reviews/review-55.md`); rulings in 04 §3; prototype fixed for patch lines, offsets and domain-touching trims (report 55 "Corrections after review") 11-26 |

Builds and tests run on a build server, never on the editing machine. The difftest builds against a
read-only checkout of solidcore at the pin, `../sc-wt/scx-pin` beside this repository (its build script
refuses any other commit). `Cargo.lock` is committed.
