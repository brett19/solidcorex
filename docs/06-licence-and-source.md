# Licence and source

## Licence (decided by Brett, 2026-11-26)

"This Library will be MIT or Apache 2, so we should only publish things allowed under that license."

What that allows, from reading solidcore's own notices (pinned commit, `solidcore/NOTICE`,
`solidcore/LICENSE`, `solidcore/ATTRIBUTION.md`). This is a reading of the files, not legal advice.

- **solidcore is licensed Apache-2.0** (`solidcore/Cargo.toml`). Its NOTICE states it is a derivative
  work: a Rust port of the SMLib and SM_API components of NVIDIA's USD BRep (usd-brep v1.0.1, commit
  3fd5b95), which is Apache-2.0. Each ported Rust file names the upstream file it derives from.
  An earlier statement in these documents that solidcore is "a port of a commercial library" was
  imprecise: the upstream is NVIDIA's, and it is published under Apache-2.0.
- **Code moved from solidcore can be published under Apache-2.0**, keeping: the Apache-2.0 licence text,
  the NVIDIA copyright and notice, solidcore's NOTICE, a statement of changes, and the per-file record
  of what each file derives from.
- **Code moved from solidcore cannot be offered under MIT by us.** A dual "MIT OR Apache-2.0" grant can
  be made only by whoever holds the rights. For the ported parts that includes NVIDIA, whose grant is
  Apache-2.0 only.
- **Code written here from scratch can be MIT OR Apache-2.0.**

So there are two workable shapes, and the choice is Brett's:

1. **The library is Apache-2.0.** Everything may be moved from solidcore. This matches solidcore today.
2. **The library is MIT OR Apache-2.0, and only original code goes in.** Nothing derived from solidcore
   or SMLib may be copied; algorithms would have to be reimplemented from public descriptions, not
   translated. That is a different and much larger project than moving code "bits at a time".

A mixture is possible (original crates dual-licensed, derived crates Apache-2.0 only) if every crate
states which it is. SolidCraft itself is MIT OR Apache-2.0 at its root and vendors solidcore as
Apache-2.0, which is that mixture.

Rules that follow, until Brett says otherwise:

- Every file brought from solidcore keeps its derivation header and goes in a crate marked Apache-2.0.
- Every crate's manifest states its licence, and a check fails the build if a file with a derivation
  header sits in a crate offering MIT.
- Test data: permissive sources only, each with a recorded origin (standing rule).
- Dependencies: MIT, Apache-2.0 or compatible; nothing copyleft.
- Nothing is published until the notices are in place.
- `prototypes/body-structure/` was written from scratch with nothing copied.

## The pinned source (decided by Brett, 2026-11-26)

"We should also pin the solid core right now, we can iteratively increase that pin later."

| | |
|---|---|
| Repository | tangency-labs/solidcraft, kernel under `solidcore/` |
| Pinned commit | `03e16c9ee766de5b9b1ced845e5149d62ff493b8` (main, 2026-11-26T08:27:17Z) |
| Read-only checkout | `../sc-wt/scx-pin` (detached; never edit it) |

All porting reads from the pin. Moving the pin is a recorded event: the new commit, the date, and the
list of upstream changes that touch code already moved, each either carried across or declined with a
reason.

Research made before the pin used other commits, and says which:

| Report | Commit |
|---|---|
| 49 (dependency map) | 53a7f14856, a local branch about a day behind the pin |
| 50 (coverage, in progress) | e3b1b2dbb6, main a few hours before the pin |
| 51 (body structure) | e3b1b2dbb6 |

Their counts will have drifted slightly. Anything that drives a decision is re-measured at the pin
before the code it concerns is moved.
