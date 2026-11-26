`IsInnerShellOf` and `GetPointsInFace` take their random numbers from the kernel's generator, so their samples depend on how many draws that kernel made earlier. The same call on the same untouched body can then give a different answer, and the validity check of a body can pass or fail depending on what ran before.

## Reproduction

On main at 782ef9afc, default kernel. The body is a cube with a spherical cavity (one face) and a few box notches whose vertical corner edges stand in the path of some of the cavity's sample rays.

| Call, repeated on one kernel and one untouched body | Answers |
|---|---|
| `topology::shell::is_inner_shell_of(k, b, cavity, Some(outer))`, 40 times | `TRUE` 30 times, `UNSURE` 10 times |
| `check_brep(k, b, CheckLevel::Level0)`, 16 times | valid 13 times; 3 times invalid with "SmBrep::Test #12 Bad Inner shells: NonInfiniteRegion Inner shell is not geometrically inside its containing outerShell" |

The same holds across bodies: on a fresh kernel an untouched copy of the body answers `TRUE` on its first call, and `UNSURE` when that kernel first made two `points_in_face` calls on an unrelated box.

`topology::face::p2::points_in_face(k, b, f, 7)` itself returns different points on a second identical call: all 7 UV points differ, on every face tried (box, cylinder, a box joined to a cylinder). For example on one box face the first sample is `(9.333333333333334, 7.018051275293367)` on a fresh kernel and `(9.333333333333334, 4.159047463129141)` after one earlier call.

## Cause

- `points_in_face` dithers each sample's V by a draw from `kernel.rng` when asked for more than one point (`rand_dither`, `topology/face/p2.rs`).
- `is_inner_shell_of` casts three rays per face use from `points_in_face(.., 3)`, and draws ray directions from `kernel.rng` for wire and vertex shells (`topology/shell.rs`). A ray that hits an edge is refused; when every ray is refused the answer is `UNSURE`.
- `check_brep`'s inner-shell test fails unless the answer is `TRUE`.
- Each draw moves the generator, so which samples a call gets depends on every earlier draw on that kernel.

Other callers of the two routines: `topology/brep/enclosing.rs`, `topology/brep/p3.rs`, `topology/topology_traverser.rs`, and `crates/smlib/src/core/convert.rs`.

`solidcore-ops` resets the generator at the start of each call, so one ops call is repeatable. Its answer still depends on how many draws came earlier within that call, and kernel users without that reset see the dependence directly. #13672 names the generator as one of the things that make a result depend on call history.

## Proposed change

Each call makes its own generator from the kernel's seed, so the samples are a function of the call's inputs alone. The dither keeps its purpose (samples off the symmetric lines of a face). The kernel's generator then has no reader and can go, with its save and restore in the Boolean's all-or-none glue, the copies made into scratch kernels, and the per-call resets in the two kernel pools.

## Expected result changes

Goldens and tests whose draws were not the first on their kernel get different sample points. The inner-shell answer for a given body becomes one fixed value. Each moved row needs its reason.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
