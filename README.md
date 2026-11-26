# solidcorex

solidcore, the Rust B-rep kernel, rearranged into layered libraries. Code is moved
from solidcore at one pinned commit (`03e16c9ee766de5b9b1ced845e5149d62ff493b8`),
bottom layer first, without changing a result: every moved function is checked bit
for bit against the pinned solidcore. Apache-2.0 (`LICENSE`, `NOTICE`).

## Layers

`layers.txt` lists the libraries bottom to top; a crate may depend only on crates
listed above it (`cargo xtask layers` checks it, dev-dependencies included).

| Crate | Layer | Holds |
|---|---|---|
| `scx-num` | 0 | constants, vectors, small matrices, the two transforms, extents, scalar tolerances |
| `scx-difftest` | outside the order | differential tests against the pinned solidcore |
| `xtask` | tool | the repository checks |

The plan for the layers above is `docs/01-architecture.md`.

## Build and test

Toolchain: `rust-toolchain.toml` (Rust 1.99.1, as solidcore).

- **Tier 1**, one crate, seconds: `cargo test -p scx-num`.
- **Tier 2**, every library, minutes: `cargo test` (the default members).
- **Differential** (needs the pinned solidcore as a clean worktree at
  `../sc-wt/scx-pin`, beside this repository; its build script refuses anything else):
  `cargo test -p scx-difftest`, in the dev profile, without `--release`: dev keeps the
  overflow checks and debug assertions on both sides, and builds faster than it runs.
- **Before a commit**: `cargo xtask fast` (format check, clippy with warnings denied on
  the default members and on `scx-difftest`, `layers`, `headers`, tier 1, the xtask's own
  tests).
- **Before a move is reviewed**: `cargo xtask movecheck`, the standing steps below.
- Wasm: `cargo build --target wasm32-unknown-unknown -p scx-num`.

Every `.rs` file opens with a header: either where it was moved from (the solidcore
file and commit, that file's own record of its upstream SMLib source, and what changed)
or that it was written for solidcorex. `cargo xtask headers` checks it against the pin.

## Standing steps for a move

`cargo xtask movecheck [crate...]` runs them in order and stops at the first failure:

1. `cargo xtask pin`: the pinned checkout is at the pin commit and clean.
2. `cargo xtask layers`: the layer order of `layers.txt`, and no library depends on a
   crate outside the workspace unless `[external]` lists it (solidcore never: only
   `[difftest]` crates may link the pin).
3. `cargo xtask headers`: every header names pin files that exist, and quotes each
   file's upstream record as the pin file has it.
4. `cargo xtask bodydiff` (`xtask/bodydiff.py`): every fn body, tests included, against
   the same-named fn at the pin after normalising the mechanical renames; each one that
   differs, or is on one side only, must be named in the header's `Changes:`. A file
   written for solidcorex may hold no fn body of the pin's solidcore crate.
5. `cargo xtask cov` (`xtask/cov.sh`): the differential tests under coverage
   instrumentation; `target/cov/branchy.txt` lists the 30 most branchy functions and every
   unexecuted region. Each unexecuted region is either reached by a new input or
   explained in the move's report (unreachable, and why).

Rules for the differential tests (`crates/scx-difftest/tests/diff/harness.rs`):

- Every public function has a `diff!` test; an error is compared by class, operation,
  reason or typed fields (floats by bits), text and cause.
- **Thresholds**: every tolerance or numerical zero a moved function compares with is in
  `thresholds()`, and the pools hold it and one ulp either side; branches random draws
  miss get a fixed case list indexed by a `usize`.

## Review checklist

1. The header names the pin files and commit, quotes their upstream records, and lists
   the changes (`headers`).
2. The crate sits in `layers.txt` and depends only on crates above it (`layers`).
3. Every moved fn is bit-identical to the pin, or named in `Changes:` (`bodydiff`).
4. Every public function has a differential test, with 0 mismatches.
5. The threshold pools hold the crate's thresholds; coverage gaps are reached or
   explained (`cov`).
6. Errors map one for one, and every variant's text is the pin's.
7. No `unwrap`, `expect`, `panic` or indexing outside tests (clippy).
8. `cargo xtask fast` and the wasm32 build pass.
9. Files written for solidcorex are diffed against the pin (`bodydiff`): none holds a
   pin body under a "written for" header.
10. The report records what moved, the cuts, the mappings, test counts and timings,
    the differential results and what was found and not changed.

## Docs

`docs/README.md` indexes the plan, decisions (`docs/04-decisions.md`), status
(`docs/05-status.md`) and research; `docs/research/52-num-move.md` records the move of
layer 0.
