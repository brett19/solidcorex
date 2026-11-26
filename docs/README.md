# solidcorex docs

Everything known and decided about rearranging solidcore into separate libraries: the plan, the
decisions, the state of the work it grew from, and the research behind it. Written for whoever picks
this up next, human or agent. There is no code in this directory yet.

**If you are an agent starting here: read `04-decisions.md` and `05-status.md` before proposing or
doing anything, then the index in `research/README.md`. Keep these files current as you work:** new
findings go in `research/` as a numbered report with an index line; decisions go in `04-decisions.md`
with the date, who decided, and the evidence; status changes go in `05-status.md`.

| File | What it is |
|---|---|
| [01-architecture.md](01-architecture.md) | The restructuring: what is wrong today, principles, the libraries, immutable finished bodies, caches, proofs and tolerances, the calling convention, errors, what to delete, speed, order of work, risks |
| [02-testing.md](02-testing.md) | How to prove it works quickly: an oracle per layer, properties for operations, work counts as tests, what remains of the goldens |
| [03-upfront-planning.md](03-upfront-planning.md) | What to settle before moving any code |
| [04-decisions.md](04-decisions.md) | What was decided, by whom, on what evidence; designs withdrawn; what is still open |
| [06-licence-and-source.md](06-licence-and-source.md) | The licence decision and what it allows; the pinned solidcore commit |
| [07-conventions.md](07-conventions.md) | Units, angles, parameter domains, orientation, frames, numbers, determinism, ids and naming: what solidcore does, where it disagrees with itself, one rule each |
| [08-glossary.md](08-glossary.md) | One meaning per word: the body types, entities and uses (with the radial-edge picture), today's loose terms and the architecture's terms |
| [09-tolerances.md](09-tolerances.md) | Tolerances today and against solidcore's own spec; accuracy per entity and resolution per operation; certificates as data; legitimate raises; migration and what moves |
| [05-status.md](05-status.md) | Merged work, parked drafts, issues, local branches, clean-up owed, how to resume |
| [research/](research/README.md) | 47 investigation reports with an index that says which to trust, plus reviews, issue texts and earlier plans |

## The plan in eight lines

1. Dependencies point one way: geometry never names a face, an edge or a body.
2. A signature tells the truth: a shared reference changes nothing observable.
3. A body is finished when an operation returns it, and never changes afterwards. An edit makes a new
   body that shares every untouched face, with its caches and proofs.
4. A proof is made once and kept with what it proves.
5. No ambient state: settings and scratch space are passed in.
6. The same input gives the same bits, on any platform and with any number of workers.
7. The stored form is exact, and importing it copies.
8. An error says who is at fault: the input or the kernel.

## Rules that apply to anything published from this work

- The wish to use threads inside the kernel is never named in an issue, PR, commit or code comment.
  Each public change stands on a defect it fixes. (These documents are private and may say it.)
- Investigate and build locally first; publish only objective improvements.
- Every change is checked for added work, with counts, before it is proposed.
- No result may move without a derivation; a claim that nothing moves needs an independent review.

## Where things are on disk

- This directory: `solidcorex/docs`. It is **not under version control**.
- Raw data, probes, logs and builder briefs: `reports/cache-arch/` (the reports
  here are copies of the ones there; the data is not copied).
- Worktrees: `sc-wt/`.
