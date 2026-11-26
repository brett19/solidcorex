# Decisions

What has been decided, by whom, and on what evidence; what is only recommended; what is open.
Add to this file; do not rewrite history in it. A changed decision gets a new entry that names the one
it replaces.

**Read this before proposing a design.** Several designs were recommended and then withdrawn during
2026-11-22 to 26 as measurements came in (section 4).

## 1. Decided by Brett

| Date | Decision | His words or the substance | Why |
|---|---|---|---|
| 11-22 | Threading is never named in an issue, PR, commit or code comment | "The desire to do threading is intentionally not an issue and is meant to be worked on separately" | Each public change must stand on a defect it fixes |
| 11-22 | We may file and claim our own issues | "As long as it doesn't call out threading explicitly, and the issue can be justified outside that scope" | |
| 11-22 | Investigate instead of asking him to choose | "Rather than asking me for decisions, we should perform investigations to identify the best path forwards" | |
| 11-22 | Builders run on the model that fits the task, not the largest | | Cost |
| 11-22 | Local checks are the basics; CI runs full suites | | Speed |
| 11-22 | Count with side-channel instrumentation; nothing added to the repository for counting | | We own the process end to end |
| 11-23 | Every change is checked for added work, with counts | "be sensitive to performance regressions" | |
| 11-23 | The goal is signatures that tell the truth | "architect the types so that it's clear from the code what operations read (&Brep) and which ones write (&mut Brep), which would then later lead to the ability to use multithreading" | Correctness, performance, maintainability; threads later |
| 11-23 | Local first; publish only objective improvements | "do as much investigation and building locally first, and then only push PRs for things that are objective improvements for now" | |
| 11-25 | The plane-trim carry is held | "wait until we make records analytics" | It should land with exact records, not alone |
| 11-25 | The offset stand-in defect is filed on its own, with a linked comment on #13789 | "Decision 2: b" | A correctness defect should not sit inside a feature request |
| 11-25 | The record round trip defect is filed | "Decision 3: file it" | |
| 11-25 | Stop work | "Stop working for now." | Not lifted as of 11-26; only questions answered and documents written since |
| 11-26 | Research, decisions and analysis are kept in `solidcorex/docs` for future agents | "We should keep all our research, decisions and analysis in docs for future agents" | |

| 11-26 | The coordinating session (Claude) owns the rewrite | "You will own the rewrite, what's next?" | Taken as lifting the stop of 11-25 for rewrite work: local investigation, design and scaffolding. Nothing is published and no solidcore code is copied until the licence question (section 6) is answered |
| 11-26 | The library is licensed MIT or Apache-2.0; only what that licence allows is published | "This Library will be MIT or Apache 2, so we should only publish things allowed under that license." | solidcore is Apache-2.0 and derived from NVIDIA's Apache-2.0 SMLib, so code moved from it can be published under Apache-2.0 but not offered under MIT by us: see `06-licence-and-source.md`. Which of the two shapes applies is open (section 6) |
| 11-26 | solidcore is pinned; the pin moves only deliberately | "pin the solid core right now, we can iteratively increase that pin later" | Pinned at 03e16c9ee7 (main, 2026-11-26); `06-licence-and-source.md` |
| 11-26 | Apache-2.0 throughout | "Apache 2 throughout" | Code may be moved from solidcore with its notices and derivation headers; no MIT offer. `06-licence-and-source.md` |
| 11-26 | Profiling tools, new Rust versions and Rust tooling may be installed | "You can install tools for profiling, or new rust versions or it's tooling." | Read as: on the machines already in use; not general system packages |

## 2. Decided elsewhere, and binding on this work

| Date | Decision | Where | Note |
|---|---|---|---|
| 11-23 | A validity check builds missing trims on a private copy and leaves the body as it was | tangency-labs/solidcraft #21105, answer posted on Brett's behalf by the swarm coordinator | Condition posted by us: the outward-solid proof checks the body itself, so the ops layer must create trims once before the proof or every proof rebuilds them |

## 3. Rulings made by the coordinating session (Claude), on evidence

These were within its remit as coordinator. Each can be overturned.

| Date | Ruling | Evidence |
|---|---|---|
| 11-23 | Gap caches stay; the check's writes of meaning are what should go | Removing gap storage made later passes recompute (research 20) |
| 11-23 | Do not build the "remembered gap" memo | 1.3 ms saved of a 32 ms repeat check (research 21) |
| 11-24 | Hold the kept tight bounding box out of the design | The box is wrong on offset faces; keeping it would preserve the wrong answer (research 32) |
| 11-25 | A test is not edited to make a change pass | The tangent-field fillet test passed on main only through a tolerance of 53 on a 20 mm box (research 46) |
| 11-25 | A fuzzed, already-invalid body's moved row is described as "findings now reported, not hidden", not as "still covered" | research 46 section C |
| 11-25 | The mutable path of a split leaf must preserve main's order of read and write exactly | A naive ensure-then-read moved a point by 2e-6 (research 43, review) |
| 11-26 | (As owner of the rewrite) The body is represented as per-face shared records (a face with its loops, uses, trims, surface and fill-once cells behind one reference count), edges and vertices in tables, dense ids never reused, all behind one table interface; whole-table copy-on-write first, chunked tables as a drop-in change when bodies exceed about 10,000 faces. Invalidation goes through the draft, which marks the faces on any changed edge or vertex. | research 51: app bodies are small (p99 94 faces); the real cost today is that every copy drops face caches 5 to 160 times the body's size; chunking only pays at about 10,000 faces; validating a cell by content costs 13 to 34 times a plain read |
| 11-26 | (As owner) Ported operations must not rewrite what they do not change | A one-rim fillet on a 70-face plate left 7% of faces untouched because it re-parameterised 63 faces it did not alter; a shell renumbers every edge and vertex. Sharing is worthless if operations churn (research 51) |
| 11-26 | (As the owner's delegate) The rewrite's crates are named `scx-*` (`scx-num`, `scx-difftest`, ...) | Short, does not collide with solidcore's own crates, which the difftest links beside them |
| 11-26 | Layer 0's errors are a `NumError` (kind, operation, typed `NumFailure`, cause) whose strings and variants match solidcore's one for one | The difftest compares errors on kind, operation, failure and cause, with 0 mismatches (research 52) |
| 11-26 | Tolerance functions take a `Tolerances` value, not the kernel | Every read was `k.tolerances.<field>`; same reads, same bits (research 52 section 3) |
| 11-26 | `geom/transform.rs` is split: the matrix part moves to layer 0, and the pseudo-box, volume and point-solve members wait for their layer | They need bodies and volumes; the four transform tests on them move with them (research 52) |
| 11-26 | Tolerance quantities get newtypes (`Length`, `Accuracy`, `Resolution`, `AngleTol` in radians, `ParamTol`), and each constant has one definition in `scx-num::consts`; values are unchanged | docs 07 and 09; the other zone definitions at the pin are listed in research 52 section 4, to be removed as their layers move |
| 11-26 | `size_estimate_to_zone_tol_3d` is renamed `zone_for_size`; the half-open rewrite of 09 section 1.2 waits | Bit identity first; a test pins today's answer at every boundary |
| 11-26 | `math::det3` is public in `scx-num` (it was `pub(crate)`) | Eval and blend use it; there is no `pub(crate)` across crates |
| 11-26 | The interval arithmetic (`Ivl`, the ring traits, Taylor models, series, interval vectors) is layer 0; `certificate.rs` and `station.rs` stay in solidcore for `nurbs`/`blend` | They need only `f64`; the certificate takes B-splines, the kernel and faces (research 53 sections 1, 11) |
| 11-26 | Defects found while moving are pinned by a test and reported, not fixed in the move | The overflow-to-NaN interval and the quartic's clustered roots are both the pin's behaviour bit for bit (research 53 section 9) |
| 11-26 | Layer 1 is `scx-algo`, depending only on `scx-num`; its first file is `polynomial` | Pure, no error type, 6 callers; 0 mismatches over 27,984 root-built polynomials at every pool tolerance (research 53 section 8) |
| 11-26 | (Decided by the owner, 2026-11-26) The local solvers are generic over the evaluator's error through `scx_algo::error::EvalError`, which classifies it: `ends_solve` (unported, cancelled, out of fuel: stops at once), `passes_through` (those and preparation-required: returned unchanged), `never_ignored` (those, missing face trims and an uncertified blend: never skipped as a failed sample). The solver's error `SolveError<E>` is its own failure (`InvalidInput`, `Failed`, `Num`) or a failed call carrying the evaluator's error as cause (`CallFailed`), or that error unchanged (`Eval`), mapped one for one onto solidcore's errors in the difftest facade. A bad Jacobian is read from the matrix solve's `NumError::Failed`, not from the evaluator. Evaluators keep `&mut self` | `scx-algo` cannot name `SolidError`; the solvers act on its classes; 13 of the 27 pin evaluators keep state between calls (research 53 sections 5 and 12) |

## 4. Designs recommended and then withdrawn

So nobody proposes them again without new evidence.

| Design | Withdrawn because | Research |
|---|---|---|
| A check computes gaps as values and stores none | Later passes relied on the stored gaps | 20 |
| Stop clearing the mate side's gap cache | The tolerance pass reads the cleared state on closed edges | 21 |
| "Raise tolerances up front, build trims lazily" | The size of a raise comes from the built trim; no a-priori bound exists with today's fitter | 27 |
| A "prepared" mark keyed on the body's write stamps, with reads refusing unprepared bodies | The proof and every tree build move the stamps; the mark does not survive a read on main | 30, local build `cache-settle` |
| Every operation ends by preparing its result (ops layer) | 61% of bodies are intermediate; later softened: the cost is small in time, and intermediates are mostly copies | 36, 38, 47 |
| Reads refuse a missing trim, done from the read side | The kernel's own edit operations create trims through the same helpers (tens of thousands of times) | 42 and the hand-back after it |
| Welding corners on the tree's own copy of the trims | Not established: the prototype was faulty | 45 |
| Read-form-plus-fallback pairs (`*_prepared`, `read_else_edit`) | They leave two entry points per operation; on hold, to be collapsed. One pair merged (#21107) | Decision of 11-23 |

## 5. Recommended, not yet decided

| Recommendation | Evidence | Depends on |
|---|---|---|
| Operations finish the trims of what they create; imports and records carry the trims they were given | A lazily trimmed body has no definite value (order and caller dependence); eager cost is small except on two STEP parts | Exact records |
| Publish the first leaf split (binormal evaluation takes `&Brep`) | Reviewed; all 283 golden units unchanged; research 43 | Brett's word |
| Remove corner welding from the tree build | No consumer needs it; removes a behaviour that masks faults | The tangent-field fix (#22387) first; 80 golden rows to derive on a current main |
| Prove once and keep the proof | Proofs are 38% of regeneration; 22.6% of the work repeats identical inputs | Report 48, not produced |
| The restructuring in `01-architecture.md` and the testing in `02-testing.md` | Design proposals | Everything in `03-upfront-planning.md` |

## 6. Open questions

- Is a per-body angle tolerance wanted (today the angle tolerance exists only at kernel level)?
- Should records store offset surfaces and blend definitions exactly (#13789 is open and unscheduled)?
- Do bodies carry a placement, so that a rigid move changes no geometry (today a transform leaves 0% of faces untouched)?
- Which edge and vertex fields may a face's cached values read? That set defines invalidation.
- The layout of the use records (71 to 80% of a body's bytes today) and where gap data lives.
- The type an operation returns to say what became of each input face and edge.
