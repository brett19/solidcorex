# solidcorex: what to plan before moving code

If solidcore's code is moved over a piece at a time, from the bottom up, with testing and correctness
checks built as it goes, these are the things to settle first. They are grouped by how expensive they
are to get wrong: each is baked into every file moved, so changing it later means touching everything
again. This is judgement, informed by the measurements in `research/`.

Changed 11-26: this list was written before the design. Most items are now settled by
`01-architecture.md` (v2) and `04-decisions.md`, and each says where. Read it as the checklist the design
had to answer. The "moved a piece at a time" picture is superseded: solidcorex rewrites the algorithms
in its own structure (04, 11-26), and the app switches once at the end (01 §10).

## 1. Decisions every moved line depends on

- **Conventions.** Units, angle measure, parameter domains, orientation of faces and loops, the meaning
  of a surface's natural normal. solidcore converts degrees and radians and re-derives plane frames at
  its boundaries; that is where its record round trip loses bits (research 39, 41).
- **The tolerance model.** Settled 11-26: one resolution per document, accuracy recorded per entity
  where worse (`01-architecture.md` section 4, `09-tolerances.md`). It shapes the type of nearly every
  geometric function.
- **The set of curve and surface kinds,** and whether each is exact. Adding a kind later touches every
  pairing. Settled in 01 §7.3 (closed enums, certified approximation for user kinds).
- **Determinism rules.** Fixed summation order, no fused multiply-add, no iteration over hash maps,
  seeded randomness (settled: 01 §8, with `fmath` for transcendental functions). Cheap to hold from the start, very hard to retrofit: reversing face order moved
  mass in 380 of 474 bodies (research 39).
- **Identity.** How entities are named and whether ids survive an edit. History and persistent naming
  depend on it. Settled: ids are dense, never reused and kept in the stored form (01 §2.2); names across
  edits come from `Changes` and `carry` (01 §2.6).
- **Platform constraints.** The browser build, minimum compiler version, no `unsafe`, allowed
  dependencies and licences. Every platform is a target, verified late (04, 11-26; 01 §7.4, §8).

## 2. The body's data structure

Immutable finished bodies with per-face sharing is the hinge of the design and cannot be added later.

- How sharing is chunked, and what an edit costs in memory on a large model. Settled: whole-table
  copy-on-write first, chunks only past about 10,000 faces (01 C5, §2.5, §3.4).
- Where cache and proof slots live, and what "untouched" means for carrying them forward. Settled:
  cells in the face record, emptied by every write path; memos in `Scratch` (01 §3).
- What an operation returns about its inputs: which faces survived, split, merged, were created. The
  app's naming depends on this; today it tags copies of operands (research 38). Settled: `Changes` and
  `carry` in the core, fates in a helper (01 §2.6).

The vertical slice (01 §10.2 steps 2 to 4) is where this is built and measured before the wider crates
are written.

## 3. The relationship with the code being moved from

- **A moving source.** solidcore's main took 1,145 commits between two branch bases a day or two apart.
  Settled 11-26: the pin is fixed and moves only deliberately (04, `06-licence-and-source.md`).
- **What "moved" means.** Settled 11-26: algorithms and lessons are kept and the code is rewritten, with
  transliteration only where solidcore is already idiomatic (04). The app does not switch piece by
  piece and no shim or adapter is built: it switches once, when solidcorex is built (01 §10.2 step 13).
- **Do not move dead code.** Run coverage on solidcore before starting.
- **The 26 compatibility switches.** Settled: none is carried over; each fix is either right (always
  on) or wrong (absent) (01 §6.3).
- **Provenance and licensing.** Answered in `06-licence-and-source.md`: solidcore is Apache-2.0 and
  derives from NVIDIA's Apache-2.0 SMLib. One question remains open there.

## 4. The oracle

Changed 11-27 (04 §1): new code is proved right against known truths, and compared with the old as a
smoke detector from the first day. A result may change whenever the new one is equally or more
correct.

- **Truth checks:** exact or higher-precision references, closed forms, and properties that need no
  reference (`02-testing.md` sections 2, 4 and 5).
- **A differential harness:** `scx-difftest`. Layers 0 to 4 are compared function by function and bodies
  by outcome, within a stated tolerance of the pin, not bit for bit (04, 11-26). A disagreement is
  settled against the truth, and a pin defect is excused by a predicate and checked against the truth
  (`02-testing.md` section 3).
- **A written definition of a valid body,** and a checker built to it: `scx-bodyq::check` takes `&Brep`
  and only reads (01 §2.3). solidcore's checker edits the body it checks and can excuse a fault by
  widening a tolerance (research 45, 46), so it cannot be the oracle as it stands.
- **Fixtures and corpus:** where test bodies come from, in what format, under what licence. Standing
  rule: permissive sources only.

## 5. The stored form

Design it with the body, not after: every kind storable exactly, import that copies, a round trip that
is a fixed point, proofs carried with the data. It is also the fixture format for tests and the bridge
to the app, so it is needed by the first crate that has anything to store.

Note: solidcore's records need no backwards compatibility. A design file's authority is its feature
timeline and original import files; the record in the file is a cache, and an unsupported version is a
cache miss that is re-evaluated (solidcore `docs/native-brep.md`).

## 6. Tooling that should exist at the first commit

- The layering check: which crate may depend on which.
- Test tiers, each with a time budget (`02-testing.md` section 8).
- Deterministic work figures: `Report::work` is an output the library returns; counters beyond that are
  side-channel instrumentation, not added to the repository (04, 11-22; 01 §9.4). No global state.
- A benchmark harness and recorded baselines.
- Fuzzing targets for each parser and public entry point.
- A fast way to run only what a change can affect.

## 7. Process

- **Order of work and a definition of done per crate:** the order is 01 §10 (a vertical slice first,
  then wider layers; 13 steps). Done is a row of the "Done when" column there: rewritten,
  property-tested, differentially tested against the pin, benchmarked from the slice on (01 §9.4).
- **A decision record** (`04-decisions.md`). The recommended design changed several times in one week as
  measurements came in; written decisions with their evidence stop that being relitigated.
- **A vocabulary.** solidcore and the app have five types called Brep or Body. Fix the names first.
- **Mandatory independent review** for anything that claims "no result moves". A review found a 2e-6
  result change that a 49-unit run reported as clean (research 43).
- **Checkpoints with stop conditions.** For example: if the first operation crate cannot match the old
  kernel on the corpus within a set effort, reassess before writing the next. The vertical slice (01
  §10.2 step 4) is the first such checkpoint.
- **Machine discipline.** Scratch build directories need an owner and a clean-up rule; unowned build
  output once filled a disk and blocked all work.

## 8. Worth measuring before committing

- How much of solidcore never executes (coverage over the goldens and the app corpus).
- How many cases the 283 golden units hold, and how many have analytic answers.
- A release-build profile by leaf routine (decides whether batched evaluation pays).
- Memory per body under the sharing design.
- What fraction of a body each operation changes (2.8% of edge uses after a Boolean, by proxy only).
- Why the certified bound is 38% of regeneration and whether a proved bound is kept (research 47; the
  follow-up was not finished).

## The two to do first

The body's data structure, because everything above depends on it and it is the least certain; and the
policy for the moving source, because without it the port never converges.
