# 56-data: the crate-graph proof

These are the scripts behind section 9 of research 56. They check the crate graph against a
complete reference graph of the kernel at the pin. The pin is solidcore 03e16c9ee7.

| File | What it is |
|---|---|
| `assignment.py` | The input listing (appendix B). It gives each file and moved item a crate, plus the crate dependencies, the design-change edge removals and the sink settings. |
| `prove.py` | The proof. It builds the item graph, applies the assignment, sinks and raises units, and checks closure and acyclicity. |
| `scip_occurrences.py` | Flattens a SCIP index into a TSV of definitions and references. It needs no protobuf library. |
| `crosscheck.py` | Compares the SCIP graph with research 49's path references (`../49-data/refs.tsv`). |

## Regenerating

The index is large, so it is not kept in the repository. To rebuild it:

1. Check out solidcore at 03e16c9ee7. In its root, run `rust-analyzer scip .`, which writes
   `index.scip`.
2. `python3 scip_occurrences.py index.scip crates/solidcore/src/ > occurrences.tsv`
3. `python3 prove.py --src <checkout>/crates/solidcore/src --occ occurrences.tsv --report all`
4. `python3 crosscheck.py --src <checkout>/crates/solidcore/src --occ occurrences.tsv --refs49 ../49-data/refs.tsv`

## Expected results

`prove.py` should report, for the graph as changed by Re-review 2 (56 appendix C):

- 918 files (50 of them test files) and 48,982 units;
- 222,584 unit edges carrying 545,129 references;
- 323 references removed by design changes;
- no unassigned files;
- 16 references outside the declared graph, in 12 unit edges, all in four edges the ruling
  dropped: bodyq → ssi (5), check → mass (2), local → mass (8) and mesh → mass (1);
- no cycles;
- 10 trait impls moved to their trait's or type's crate and 7 left in neither.

With those four edges added to `DEPS`, it reports 0 references outside and no cycles. The crate
table it prints is the one in 56 appendix C.4.

`crosscheck.py` counts file-to-file edges as follows:

| Count | SCIP | 49 | Both | SCIP only | 49 only |
|---|---:|---:|---:|---:|---:|
| File-to-file edges | 24,299 | 13,054 | 13,047 | 11,252 | 7 |
| Without module-path segments | 16,655 | 13,054 | 13,000 | 3,655 | 54 |
