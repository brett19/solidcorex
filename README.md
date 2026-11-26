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
- **Differential** (needs the pinned solidcore as a worktree at
  `../sc-wt/scx-pin`, beside this repository): `cargo test -p scx-difftest --release`.
- **Before a commit**: `cargo xtask fast` (format check, clippy with warnings denied,
  `layers`, `headers`, tier 1).
- Wasm: `cargo build --target wasm32-unknown-unknown -p scx-num`.

Every `.rs` file opens with a header: either where it was moved from (the solidcore
file and commit, that file's own record of its upstream SMLib source, and what changed)
or that it was written for solidcorex. `cargo xtask headers` checks it.

## Docs

`docs/README.md` indexes the plan, decisions (`docs/04-decisions.md`), status
(`docs/05-status.md`) and research; `docs/research/52-num-move.md` records the move of
layer 0.
