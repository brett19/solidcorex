# body-structure: prototype for research report 51

A design-study prototype of an immutable B-rep body with cheap edits. It is written from
scratch with std only. No solidcore code is in it: solidcore was read for its structure and
measured separately (see `docs/research/51-body-structure.md`).

## What is here

- `src/table.rs`: four entity tables behind one trait.
  - `Plain`: a `Vec`, the mutable baseline.
  - `WholeCow`: `Arc<Vec>`; the first write copies the whole table. This is what solidcore's
    `Shared<Arena>` gives.
  - `Chunked<T, C>`: `C` items per `Arc` chunk; the first write copies one chunk.
  - `Trie`: a 32-way persistent vector; the first write copies a root-to-leaf path.
- `src/body.rs`: two body layouts, each generic over the table kind.
  - `TableBody`: one table per kind (faces, loops, edges, vertices).
  - `FaceBody`: one `Arc<FaceRecord>` per face. The record holds the face's loops, edge uses,
    trims, surface and cells. Edges and vertices stay in tables.
- `src/body.rs`, edit types:
  - `Draft`: an edit of either layout.
  - `Draft::finish`: gives touched faces fresh cells and reports each one's `FaceFate` in an
    `Outcome`. The fates are sparse: a face that is absent is unchanged and keeps its id.
- `src/model.rs`: the synthetic body. It is a closed torus quad mesh with geometry held by
  `Arc`. It also holds `FaceCells`, the fill-once cells (`OnceLock`).
- `src/ids.rs`: generational ids over any table. A freed slot is reused, and a stale id is
  refused.
- `src/counting.rs`: a counting global allocator. It records bytes allocated, bytes live and
  the number of allocations.
- `src/bin/body_bench.rs`: the measurements, printed as TSV sections (`memory`, `edit`,
  `transient`, `read`).
- `tests/structure.rs`: tests for the behaviour the report relies on:
  - every layout gives the same body;
  - untouched faces carry their filled cells by pointer, with no recomputation;
  - a cell filled through a new body is visible through the old one;
  - changing an edge's tolerance resets the cells of its two faces and no others;
  - eight racing readers get one value;
  - a transient draft writes in place;
  - stale generational ids are refused.
- `src/lib.rs`: a compile-time `Send + Sync` assertion for every layout.

## Running

Never build on the workstation: build on a build server with a scratch target directory.

    rsync -a --exclude target ./ server:sc-trash/scx-body/body-structure/
    cargo test  --release --manifest-path .../body-structure/Cargo.toml --target-dir .../target
    cargo run   --release --manifest-path .../body-structure/Cargo.toml --target-dir .../target \
                --bin body-bench -- all > bench.tsv

A full run takes about two minutes.
