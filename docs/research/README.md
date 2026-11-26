# Research index

Every investigation made on solidcore between 2026-11-22 and 2026-11-26, copied here unchanged. Each
report labels its claims MEASURED, READ (from code) or INFERRED. **Read the "Trust" column first:**
several reports were corrected by later ones, and one run was contaminated.

Conventions used in the reports: "main" is tangency-labs/solidcraft `main` at the commit the report
names. Timings are indicative unless the report says otherwise (shared server, often single runs);
counts are exact. Raw data, probes and logs are NOT copied here: they are under
`reports/cache-arch/` (`count-kit/`, `scratch*/`, `perf-*`, `logs/`), 200 MB.

## First investigation: where caches and hidden writes are

| Report | Subject | Main finding | Trust |
|---|---|---|---|
| 01 | Geometry-side caches | Surface and curve caches, slots, what holds them | Code reading |
| 02 | Lazy writes on the body | Which reads write to a body; eight "write leaves" | Code reading; superseded in detail by 42 |
| 03 | Kernel ambient state | What the `Kernel` object carries | Code reading |
| 04 | Cache holders | Who holds which cache | Code reading |
| 05 | Candidate read phases | Where independent work exists inside operations | Code reading |
| 06 | History probes | Answers that depended on what a kernel had done before | Measured; all four fixed and merged |
| 07 | Boolean as gather/apply | What restructuring one operation costs | Code reading |
| 08 | Stale gap memos | Whether an operation reaches a stale gap | Measured |
| 09 | Whole-cache copies | One edge fillet made 260 copies | Measured; fixed (#21019) |
| 10 | Face-level trim pass | Idempotence and callers | Partly superseded by 30, 44, 45 |
| 11–13, 16, 17 | Per-operation studies: reads, validity check, fillet, offset/sweep, heal/stitch/import | What each needs to become prepare/read/apply | Code reading; some assumptions later disproved (see 36, 42) |
| 14 | Boolean coupling probes | Coupled writes fired 0 times in 47 Booleans | Measured |
| 15 | Signature-flip sizing | The compiler lists the writes under a read | Method; reused in 42 |
| 18 | One stale edge-to-edge gap | Real, low impact, cause not traced | Code reading |

## Performance of what was merged

| Report | Subject | Main finding | Trust |
|---|---|---|---|
| 19, 23 | Before/after counts of six merged changes | No avoidable work added; one rise is work wrongly skipped before | Measured |

## The validity check and trims

| Report | Subject | Main finding | Trust |
|---|---|---|---|
| 20 | Check stores no gaps | Removing gap storage made later passes recompute; approach parked | Measured |
| 21 | Repeated vertex-to-face gap solves | Real but worth 1.3 ms of 32 ms; closed | Measured; first explanation of cause was wrong, corrected inside |
| 22 | First check takes 3.7 s | It built each trim three times; 95–99% is the certified bound | Measured; one build removed (#21135) |
| 27 | Could a trim be a pure cache | Not while creating one raises tolerances | Code reading; its recommended design superseded by 34, 35 |
| 28 | How many trims already fit | Almost all; the obstacle was "no proof", not "too large" | Measured |
| 34 | Why trims fail to fit | 24 of 5,485; 1 true floor; the fitter aims at the wrong target | Measured |
| 35 | Trims fitted to the existing tolerance | Sound, order-independent, no corner welding needed; 3.3× creation cost; 40 refusals from missing proofs | Measured; prototype only |
| 47 | Exact cost of a trim | 2.7 µs to 95 µs without a proof; 12.8 ms median with one. Proofs are 38% of app regeneration, mostly not from creating trims | Measured, release build, ±3% |

## Reads, trees and "prepared" bodies

| Report | Subject | Main finding | Trust |
|---|---|---|---|
| 24 | Classification and rays, design | Items independent once a body is prepared | Code reading |
| 25 | Classification counts | The body copy is 0.3–1.6% of a call; trees are rebuilt every call | Measured |
| 26 | Why trees are rebuilt | Solves never read the face's slot | Measured |
| 29 | Lazy trims and lazy trees, prototype | Works; **stage 2's "70,337 answers match" came from bodies whose corner pass had not run, so it does not show trees matched** | Stage 1 sound; stage 2 weaker than stated |
| 30 | What a tree build writes | An edit on first run (72 of 2,644), a fixed point after. **Section 5's first numbers were contaminated by a process-wide switch and were replaced; section 5's "settle misses a trim" was wrong and is corrected in 6** | Use sections 6.0 onward |
| 33 | Answers that change when a body is settled first | 50 rays, 19 mass, 4 checks: almost all rounding ties; main is order-dependent on raw bodies | Measured |
| 42 | 32 reads on the edit door | None can take `&Brep` today; no real edit under any | Measured |
| 43 | Splitting trim-making leaves | Two leaves split with all 283 golden units unchanged. **First version missed a 2e-6 divergence; see section 0 and the review** | Measured |
| 44 | The containment walk and the face routine | The walk splits exactly; the face routine cannot | Measured |
| 45 | Is corner welding needed | No consumer needs it | Measured; **54 rows became 80 on a later main** (see 46 and the status file) |
| 46 | What depends on the tolerance raise | One fillet "works" through a tolerance of 53 on a 20 mm box; no valid body needs the raise | Measured; filed as #22387 |

## Tolerances, the app, and records

| Report | Subject | Main finding | Trust |
|---|---|---|---|
| 31 | Where tolerances are read from | About 1,000 reads from the entity, 70 from the kernel, 64 constants; the kernel-level settings are near-vestigial | Pattern counts, ±10% |
| 32 | Tight bounding box | Wrong by 10–13 mm and 14–89 s on bodies with offset faces | Measured; filed as #21660 |
| 36 | Intermediate bodies in the app | 61% of bodies are intermediate. **"104 Boolean results only queried" is wrong: they are borrowed union operands (38)** | Measured |
| 37 | Why reloaded bodies rebuild trims | The app's import drops plane trims the record holds | Measured |
| 38 | Why intermediates exist | 402 history copies, 663 API shape, 210 app storage; 515 ms of 39 s | Measured |
| 39 | Mass across a reload | Rounding-level, invisible to users; the record round trip is not a fixed point | Measured; filed as #21927 |
| 40 | One body at 5e-7 | The record stores a spline stand-in for an offset surface | Measured |
| 41 | What records cannot hold | Offset and some procedural kinds; an unfilled gap, not a design rule | Read and measured; filed as #21926 |

## Rewrite, phase 0

- [49-module-dependencies.md](49-module-dependencies.md): who refers to whom in the 1.16 M-line crate (commit 53a7f148, measured by script, data in `49-data/`): the area graph, one 19-area cycle (17 without `error`/`kernel`), 1,158 upward references between areas with the items behind them, a proposed file-to-library mapping with upward references per library and an extraction order with the cuts at each step, the files that mix layers (`geom/`, `error/`, `kernel.rs`, `query/`), the crates outside the kernel (smlib 651 and geom 20 paths into it, as the xtask caps say), and the twenty most-shared items. Trust: counts MEASURED, mapping and order JUDGED.
- [51-body-structure.md](51-body-structure.md): how an immutable body should be laid out (solidcore e3b1b2dbb6 probed on server2; std-only prototype in `../../prototypes/body-structure/`; data in `51-data/`). Bodies are 9-21 KB per face, and face caches are 5-160 times the body. Local operations leave 58-98.6% of faces and 94-100% of edges untouched, but a fillet rewrites 93% of faces (re-parameterised) and offset and transform keep nothing. Recommendation: per-face `Arc` records with shared geometry and cells, behind one table trait; whole-table copy-on-write is enough for app bodies (p99 94 faces), and chunks of 64 are needed only at 10k faces and up (51 µs against 14.5 ms per edit at 102k, reads 1.1-2.6 times slower). Invalidation goes through the draft, and fates are sparse. Trust: MEASURED, with the recommendation INFERRED; no large STEP part was measured.

## Not produced

- Report 48 (why the certified bound is 38% of regeneration: what each caller proves, how often a proof
  changes anything, whether a proved bound is kept). Started, stopped before it reported.

## Other folders

- `reviews/`: independent reviews of changes, published and unpublished.
- `issues-as-filed/`: the text of each issue and comment filed, including superseded drafts.
- `plan-v3.md` … `plan-v5.md`: earlier plans for the cache work, kept for history. Superseded by
  `../01-architecture.md` and `../04-decisions.md`.
