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

Builds and tests run on a build server, never on the editing machine. The difftest builds against a
read-only checkout of solidcore at the pin, `../sc-wt/scx-pin` beside this repository (its build script
refuses any other commit). `Cargo.lock` is committed.
