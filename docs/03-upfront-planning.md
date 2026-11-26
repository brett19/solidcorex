# solidcorex: what to plan before moving code

If solidcore's code is moved over a piece at a time, from the bottom up, with testing and correctness
checks built as it goes, these are the things to settle first. They are grouped by how expensive they
are to get wrong: each is baked into every file moved, so changing it later means touching everything
again. This is judgement, informed by the measurements in `research/`.

## 1. Decisions every moved line depends on

- **Conventions.** Units, angle measure, parameter domains, orientation of faces and loops, the meaning
  of a surface's natural normal. solidcore converts degrees and radians and re-derives plane frames at
  its boundaries; that is where its record round trip loses bits (research 39, 41).
- **The tolerance model.** One number, or accuracy per entity plus resolution per operation
  (`01-architecture.md` section 5). It shapes the type of nearly every geometric function.
- **The set of curve and surface kinds,** and whether each is exact. Adding a kind later touches every
  pairing.
- **Determinism rules.** Fixed summation order, no fused multiply-add, no iteration over hash maps,
  seeded randomness. Cheap to hold from the start, very hard to retrofit: reversing face order moved
  mass in 380 of 474 bodies (research 39).
- **Identity.** How entities are named and whether ids survive an edit. History and persistent naming
  depend on it.
- **Platform constraints.** The browser build, minimum compiler version, no `unsafe`, allowed
  dependencies and licences.

## 2. The body's data structure

Immutable finished bodies with per-face sharing is the hinge of the design and cannot be added later.

- How sharing is chunked, and what an edit costs in memory on a large model.
- Where cache and proof slots live, and what "untouched" means for carrying them forward.
- What an operation returns about its inputs: which faces survived, split, merged, were created. The
  app's naming depends on this; today it tags copies of operands (research 38).

Prototype this one piece and measure it on the largest models before anything is moved onto it.

## 3. The relationship with the code being moved from

- **A moving source.** solidcore's main took 1,145 commits between two branch bases a day or two apart.
  Choose: pin a base and port from that, or track it; and decide how fixes made upstream to
  already-moved code are carried across.
- **What "moved" means.** If old code stays alive beside new, every fix is made twice. If it is deleted
  on move, the app must switch piece by piece, which needs a shim presenting the old interface on top
  of the new crates. Design the shim first.
- **Do not move dead code.** Run coverage on solidcore before starting.
- **The 26 compatibility switches.** Decide each one's fate before its code moves.
- **Provenance and licensing.** Answered in `06-licence-and-source.md`: solidcore is Apache-2.0 and
  derives from NVIDIA's Apache-2.0 SMLib. One question remains open there.

## 4. The oracle

New code is proved right by comparison with old from the first day.

- **A differential harness:** the same input through both kernels, compared in a canonical form that
  ignores storage order and compares within tolerance. It needs a neutral input format.
- **A written definition of a valid body,** and a checker built to it. solidcore's checker edits the
  body it checks and can excuse a fault by widening a tolerance (research 45, 46), so it cannot be the
  oracle as it stands.
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
- Three test tiers, each with a time budget (`02-testing.md` section 8).
- Deterministic work counters, built in, with no global state.
- A benchmark harness and recorded baselines.
- Fuzzing targets for each parser and public entry point.
- A fast way to run only what a change can affect.

## 7. Process

- **Order of work and a definition of done per crate:** ported, property-tested, differentially tested
  against the old kernel, documented, benchmarked.
- **A decision record** (`04-decisions.md`). The recommended design changed several times in one week as
  measurements came in; written decisions with their evidence stop that being relitigated.
- **A vocabulary.** solidcore and the app have five types called Brep or Body. Fix the names first.
- **Mandatory independent review** for anything that claims "no result moves". A review found a 2e-6
  result change that a 49-unit run reported as clean (research 43).
- **Checkpoints with stop conditions.** For example: if the first operation crate cannot match the old
  kernel on the corpus within a set effort, reassess before moving the next.
- **Machine discipline.** Build output filled a disk quota and blocked all work once; scratch build
  directories need an owner and a clean-up rule.

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
