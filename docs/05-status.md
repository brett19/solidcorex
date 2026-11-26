# Status of the solidcore work this plan grew from

As of 2026-11-26. The stop of 11-25 was lifted for rewrite work when Brett assigned ownership of the
rewrite on 11-26 (`04-decisions.md`). The solidcore branches below are still as they were left. Check every
item against the repository before relying on it: solidcore's main moves by hundreds of commits a day.

Repository: tangency-labs/solidcraft (kernel under `solidcore/`). Local worktrees are under
`sc-wt/`. The build server is server2 (the build server); rules for using it are in
`coord/sc-agent-rules.md` and `reports/cache-arch/briefs/`.

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

## 4. Local branches, unpublished

All clean (nothing uncommitted) on 11-26. None is pushed to the fork.

| Worktree / branch | Head | What | State |
|---|---|---|---|
| `cache-leaf-split` | a2bbe62eed | Binormal evaluation takes `&Brep`; one mutable helper preserves main's order | Reviewed, fixed; all 283 golden units unchanged. Recommended for publishing; not decided |
| `cache-leaf-split-2` | 53a7f14856 | Same split for normalised evaluation, stacked on the first | All 283 unchanged; **not reviewed** |
| `cache-leaf3-walk` | 2c7fe37f57 | The containment walk as a resumable sequence | All 283 unchanged; **not reviewed**; based on `cache-leaf-split-2` |
| `cache-tangent-field-gap` | fb43804db0 | Fix for #22387: a curve without positions is skipped in two gap measurements | Test strengthened, fails on main; full golden run started on server2 and **its results were never read** |
| `cache-tree-no-gapfix` | f011958b61 | The tree build no longer welds corners; merges the fix above | **Unfinished, nothing blessed.** 80 rows in 10 units on its old base; main has since moved 1,145 commits and deleted some pinned tests, so the table must be recounted |
| `cache-plane-trims` | c4c600450f | The app's import keeps plane trims the record holds | Reviewed and ready. **Held by Brett** until records are exact |
| `cache-settle` | 3229ce995c | A "prepare" operation and mark in the kernel; ops layer calls it | Works, but the mark does not survive a proof or a tree build on main. Superseded as a design |
| `cache-c2-bounds`, `cache-trim-raise-one-rule` | | The two parked drafts | |

Prototypes (scratch, with temporary switches and counters; for reading, not for merging):
`cache-lazytrim-a` (branch `zz-lazytrim`: lazy trims and trees), `cache-tree-slot-probe`
(`zz-tree-slot-s`: explicit settle and read-only trees), `cache-trimsplit-a` and its fit-to-budget
successor, `cache-nogapfix` (the welding experiment), `cache-tanfield`, and the measurement trees
`cache-bodies-m`, `cache-rectrims-m`, `cache-massreload-m`, `cache-finsvol-m`, `cache-trimcost-m`,
`cache-reads23-*`, `cache-leaf*-m`.

## 5. Left to clean up

- About 54 `cache-*` worktrees on this machine; most are scratch.
- On server2: scratch worktrees and side refs `push/cache-*` in `sc-main`; logs under
  `sc-trash/`; three build directories the removal tool refused because they are outside its
  allowed locations (`sc-wt/cache-tree-no-gapfix/solidcore/bin-exp`, `bin-test-branch`,
  `bin-test-main`, about 17 GB).
- Remove worktrees only with `cargo xtask worktree-rm` (see the agent rules). Never `rm -rf`.
- This machine hit a disk quota on 11-26, which blocked every command until space was released. Builders
  had run cargo locally against instructions.

## 6. If work resumes, in this order

1. Read `04-decisions.md`.
2. Re-base before anything else: every local branch is days behind a main that moves fast.
3. Read the unread golden results for `cache-tangent-field-gap`, or rerun them.
4. Get reviews for `cache-leaf-split-2` and `cache-leaf3-walk`.
5. Recount `cache-tree-no-gapfix`'s moved rows on current main; finish the derivations.
6. Produce report 48 (why proofs are 38% of regeneration).
7. Check whether the four issues closed by others are really fixed, using our reproductions.

## 7. The rewrite: phase 0 (started 2026-11-26)

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
| Layer 0 moved: `scx-num` | `crates/scx-num`, `research/52-num-move.md` | done (to 429af2c): 128 unit and 10 property tests; difftest 267 tests, 616,874 inputs, 0 mismatches against the pin; builds for wasm32 |
| Interval arithmetic into `scx-num`; review fixes | `crates/scx-num`, `research/53-interval-and-algo.md` | done 11-26: 148 unit, 11 enclosure, 10 property, 1 libm-table tests; difftest 303 tests, 0 mismatches, 97.11% of regions; one non-enclosure found (overflow to NaN), not changed |
| Layer 1 started: `scx-algo` | `crates/scx-algo`, `research/53-interval-and-algo.md` sections 5 and 8 | `polynomial` moved 11-26, bit-identical; the local solvers 11-26 with generic errors (owner's design), 0 mismatches with failures injected at every call; Krawczyk (with `inverse4`) and `hash_table` (over `Tolerances`) 11-26, 0 mismatches. Step 2 done |

Builds and tests run on server2, never on the editing machine. The clone is `solidcorex`, the git remote is `server2`, and pushes use updateInstead, so the server tree must be clean. Every cargo command goes through `coord/sc-slot.sh <name> nice -n 10 cargo ...`. Format fixes are made by `cargo fmt` there, brought back as a patch and committed here. `Cargo.lock` is committed. The pinned solidcore that the difftest builds against is `sc-wt/scx-pin` on both machines.
