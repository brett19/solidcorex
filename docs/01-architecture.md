# solidcorex: architecture

A design proposal for a fork of solidcore, rearranged into separate libraries. Not a schedule; nothing
here is started. Numbers are measurements of solidcore as of 2026-11-26 (private reports 19 to 47 under
the raw data (kept outside this repository)) unless marked as judgement.

## 0. What is wrong today, in one place

- One crate of 1.16 M lines (about 25,300 non-test functions). Everything can reach everything.
- Geometry depends on topology, though less directly than first counted: 26 surface and 19 curve files
  import the topology module, but only 5 surface files and no curve file name the body or an entity id;
  the rest reach the database reader and writer, which live there (research 49). A surface does hold a
  link to its owning face (set at 175 sites) and can build that face's trims when asked for a cache.
- A read can edit: no read entry point of 32 sized had a real edit under it, yet none could take
  `&Brep`. 1,667 parameters take the body writable against 1,473 shared.
- A body is not finished when an operation returns it: trims are made lazily, making one raises
  tolerances, and the result depends on who asks first and in what order.
- Proofs are recomputed, not kept: the certified bound is 38% of app regeneration, mostly re-checks.
- Caches are per call or keyed on whole-table stamps: trees are rebuilt on every query.
- Ambient state: a `Kernel` object carries settings, mode flags set mid-flight, memos and counters.
- The stored form cannot hold every kind exactly and its round trip is not a fixed point, so the app
  rebuilds a body from its record before every step (61% of bodies are intermediate).
- 26 switches each keep a defect of the original kernel alive beside its fix.

## 1. Principles

1. **Dependencies point one way.** Geometry never names a face, an edge or a body.
2. **A signature tells the truth.** `&T` changes nothing observable; `&mut T` is a write of meaning;
   one signature per operation.
3. **A body is finished when it is returned.** Trims exist, tolerances are final. Whatever is computed
   later is a pure function of the body.
4. **Prove once, keep the proof.** A certificate lives with what it certifies and is invalidated only by
   a change to its inputs.
5. **No ambient state.** Settings are an immutable value passed in; scratch space is passed in; nothing
   is set and restored.
6. **Same input, same bits**, on every platform and for any number of workers.
7. **The stored form is exact** and importing it copies, never re-derives.
8. **Errors are types that say who is at fault**: the input, or the kernel.

## 2. The libraries

Bottom to top. A crate may depend only on crates above it in this list.

| # | Crate | Holds | From today's |
|---|---|---|---|
| 0 | `num` | scalars, vectors, points, matrices, rigid and affine maps, boxes, parameter ranges, **interval arithmetic with sound rounding**, tolerance newtypes | `math`, most of `geom` (48 k) |
| 1 | `algo` | root finding, Newton, minimisation, quadrature, polynomial tools, linear solves, a generic branch-and-bound / bisection driver, a generic bounding-volume tree | scattered through `eval`, `geom`, `query` |
| 2 | `nurbs` | B-spline basis, curve and surface evaluation with derivatives (single and **batched**), knot operations, fitting, enclosures over a span | `nurbs` (90 k) |
| 3 | `curve`, `surface` | every curve and surface kind (analytic, spline, procedural: offset, blend, sweep) as closed sets; evaluation, natural domain, exact closed forms for analytic pairs. **No owner, no topology.** | `curve` (77 k), `surface` (164 k) |
| 4 | `geomq` | point-to-curve and point-to-surface projection, curve/surface and surface/surface intersection, certified curve-to-surface distance bounds, the plain surface tree | `intersect` (83 k), parts of `query`, `eval` |
| 5 | `brep` | the body: entities, ids, adjacency, per-entity tolerances, attributes, per-face change tracking, the declared pure-cache slots, the builder. **Data and invariants only; small.** | `topology.rs` (3 k) and its tables |
| 6 | `topo` | algorithms on bodies that belong to no one operation: traversal, classification, trim construction, the face (trimmed) tree, validity checking, mass and bounds, tolerance settlement | `topology` (146 k), `query`, `mass` |
| 7 | `boolean`, `blend`, `offset`, `sweep`, `primitive`, `heal`, `stitch`, `hlr`, `develop` | one modelling operation each. They depend on 0 to 6, not on each other, except where stated (shell and fillet insertion use `boolean`) | same names; `blend` is 230 k and splits again inside: section, solver, corner, executive |
| 8 | `record`, `convert`, `step` | geometry as data; exact conversion both ways; exchange formats | same |
| 9 | `ops` | feature-level compositions, sessions, history and naming. **The only crate an application sees.** | `solidcore-ops`, plus the 87 k lines of composition now in the app |
| – | `goldens`, `testkit` | the oracle and shared fixtures | same |

Why these cuts: 0 to 2 are reusable outside CAD and have almost no upward references today (2 spline
files import topology). 3 and 4 are where the cycle is and where the vectorisable arithmetic is. 5 is a
privacy boundary: only `brep` can touch a body's fields, so nobody can add a hidden writable field from
an algorithm. 7's crates compile and test independently.

## 3. The body

**Operations take bodies by shared reference and return new ones.** Inside an operation there is a
mutable *draft*; `finish()` turns it into a `Brep`, which is immutable from then on.

- `Draft`: the working form. Mutable, no caches, trims may be missing.
- `Draft::finish(&Settings) -> Result<Brep>`: creates the missing trims of what the operation made, in
  one fixed order; settles tolerances; attaches certificates; checks invariants in debug and test
  builds. This is the one place tolerances are set.
- `Brep`: finished. Shared by reference count. Edits go `brep.edit() -> Draft` with structural
  sharing: entity tables are chunked or per-face, so a draft that changes three faces of three hundred
  shares the other 297, and **their caches and certificates come with them**.

What this removes, with today's measured cost in the app: operand copies made to keep the original
(236 bodies), copies made so a read can run (427), rollback snapshots on every call (492), and "the
callee consumed my body".

Imports are the other way in: `convert::from_record` builds a `Draft` from stored data, keeps the trims
and certificates the record holds, verifies what is not certified, and finishes.

## 4. Caches

Three kinds, each with exactly one home and one rule.

| Kind | Example | Home | Invalidation |
|---|---|---|---|
| Intrinsic to a geometry object | a surface's plain tree, a curve's analysis | a fill-once cell in the surface or curve | never: the object is immutable |
| Intrinsic to a finished body | a face's trimmed tree, exact box, gap records | fill-once cells in `brep`, per face or per entity | never: a `Brep` is immutable; an edit makes a new one and shares the cells of faces it did not touch |
| Shared across bodies, value-keyed | a proof for identical inputs | an explicit, bounded memo in the caller's workspace | by key; dropped with the workspace |

Because a finished body cannot change, the invalidation problem this session kept hitting (stale gap
caches through three different writes; a prepared mark voided by a flag toggle) does not exist. There is
no per-call cache table, no whole-table stamp, no lock held for the length of a solve.

## 5. Proofs and tolerances

- A trim carries its certificate: the bound and what it was proved against. Analytic pairs (line on
  plane, circle on cylinder) are exact by construction and carry a zero bound with no runtime proof.
  Fitted trims are proved once, by the operation that makes them. Imports are verified once on entry.
- An operation states what it preserves: a rigid move keeps every certificate; a scale multiplies
  them; a Boolean keeps those of untouched faces.
- The full proof runs across the test suite and corpus as an audit. A violation there is a kernel
  defect, never a reason to widen a tolerance.
- Two numbers, not one: an **accuracy** per entity (derived from its certificate; as tight as is true)
  and a **resolution** per operation (how close counts as coincident; a parameter, scaled to the model).
  Creating a trim never raises anything. A raise happens only for imported data whose stated tolerance
  is false, and it is recorded.
- Trim ends are fitted to the vertex's position to begin with; there is no corner-welding pass.

## 6. Calling convention

No `Kernel` object. Three things are passed, explicitly:

- `&Settings`: immutable for the life of a session: resolution policy, angle policy, the random seed.
  No fix switches (section 8).
- `&mut Scratch`: per-worker reusable buffers and the bounded memo. Never affects a result.
- `&Cancel`: cooperative cancellation; and `&mut Report` for warnings and counts, separate from errors.

Every operation has the same shape: **prepare** (on the draft: what must exist) → **compute** (reads
finished inputs and the draft by shared reference; independent items; returns plain values) → **apply**
(writes the draft in a fixed order). The compute phase is where work can later be spread over workers
with no change to results, because nothing in it writes.

Mode flags become arguments. A function that needs a face's boundary is given the face.

## 7. Errors

Today: about 30 k lines of error types, with status numbers from the original still carried in places.

- Each crate defines one error enum for its own failures. Upper crates wrap lower ones with the
  operation and entity as context. No strings are parsed and no numbers compared to decide anything.
- Three top-level outcomes besides success, and the type says which:
  - `Refused`: the input cannot be handled. Carries a typed cause and the entities involved. Stable,
    documented, user-presentable.
  - `Defect`: an internal invariant failed. This is a kernel bug. Carries where and what; never shown
    as the user's fault.
  - `Cancelled`.
- "Valid input never becomes a refusal" becomes checkable: the set of `Refused` causes is a closed
  list per operation, and tests pin which inputs produce which.
- No panics, no `unsafe`, as now.

## 8. What I would delete

- The 26 compatibility switches: choose the fixed behaviour, keep comparison with the original only in
  an external harness.
- The surface's link to its face, and the 175 sites that maintain it.
- Mutable/read-only twins of the same computation.
- Settings nothing reads (the intersection and approximation tolerance fields, the size ratio).
- Per-call cache tables; the corner-welding step; raise-on-creation; the app's bridge shaped like the
  original C++ API.
- Whatever coverage over the goldens and the app corpus shows is never reached, after review.

## 9. Speed

In order of expected payoff (judgement, informed by the measurements):

1. Stop repeating work: keep proofs (38% of regeneration), keep trees (rebuilt per query), stop
   rebuilding bodies from records.
2. Exact closed forms wherever a pairing is analytic (a circle on a cylinder mostly takes the general
   path today).
3. Batched evaluation in `nurbs` and batched interval pieces in `geomq`: lane-wise, so results are
   bit-identical to scalar. No fused multiply-add, no cross-lane sums. A scalar fallback for wasm that
   gives the same bits.
4. Workers over the compute phase of operations and over independent reads.

## 10. How to get there

Not a rewrite. The kernel stays working and the goldens stay green at every step; each step is
mechanical or small, and states whether any result moves.

| Step | What | Results move? |
|---|---|---|
| A | Enforce the layering *inside* the current crate first: a check that module X may not import module Y, starting permissive and only tightening. No code moves. | No |
| B | Extract `num`, `algo`, `nurbs`. Almost no upward references. | No |
| C | Cut geometry's references to topology: callers pass the face; the cache getter splits into "plain" and "for this face". Then extract `curve`, `surface`, `geomq`. | No |
| D | Lift writes out of read paths (the leaf splits: three done locally with no row moved). Remove the corner welding from the tree build. | A few dozen rows, each derived |
| E | Introduce `Draft` / `finish` / immutable `Brep` in a `brep` crate; operations end by finishing. Per-face sharing and cache cells. | Where trim order mattered: few |
| F | Certificates kept with trims; operations declare what they preserve; the audit in tests. | No (work removed) |
| G | Records exact for every kind; import copies; round trip a fixed point. The app keeps live bodies. | Imported and reloaded bodies |
| H | Extract the operation crates one at a time, smallest first; `blend` last. | No |
| I | Move feature compositions from the app into `ops`; retire the bridge. | Per feature |
| J | Errors, settings and the calling convention, crate by crate as each is extracted. | No |
| K | Separate accuracy from resolution; fit-to-budget trims; delete the switches. | Yes, widely; last |

A to D and F are worth doing even if nothing else is. E is the hinge: it is what makes caches never
invalidate and reads provably pure. K is the only step that changes results across the board, so it
goes last and on its own.

## 11. Risks

- **Size and pace.** Main took 1,145 commits between two of this session's branch bases. Large moves
  must be scripted and landed fast, or they will never merge. Step A exists so that the boundaries are
  enforced before anything moves.
- **The oracle.** 283 golden units pin bits. Mechanical steps must show zero moved rows; steps that move
  rows need a derivation each. The budget for that is what limits the pace of D, E, G and K.
- **Fidelity to the original.** Part of the kernel's value is that it behaves as a known kernel does.
  Each deliberate departure is recorded with its reason.
- **Immutable bodies and memory.** Sharing must be per face or per chunk, or every edit copies a body.
  Needs measuring on the largest models before E is committed to.
- **The app.** It depends on the bridge and on rebuilding from records; G and I change how it works
  day to day.

## 12. What I do not know

- How much of the 1.16 M lines is never executed (a coverage run would say).
- How many call sites hold a surface without its face in scope (12 use the face-less cache getter; the
  owner link is read about 250 times).
- What fraction of a body an operation changes (2.8% of edge uses after a Boolean, by proxy).
- Whether batched evaluation pays, without a profile of the release build by leaf routine.
- The effort. A to D are weeks of builder time; E, G and H are months; I would not put a number on K.

## 13. Corrections from the dependency map (research 49, 2026-11-26)

Measured on the module graph; these change the plan above.

- **Everything is one cycle today.** 19 of 26 top-level areas form a single strongly connected
  component, including `geom`, `math`, `nurbs` and `curve`. No library can be extracted without cuts.
  There are 1,158 upward references across 47 pairs of areas, not counting errors and the kernel object.
- **Interval arithmetic and the certified spline enclosures live inside blending**
  (`blend/fillet_section`: about 9,000 lines) and are used by surface, query, intersect and mass. They
  belong in `num` and `nurbs`. Moving them removes most references to blending from below.
- **`geom/tol.rs` must be split.** Its scalar comparisons are the most-called functions in the kernel;
  the same file holds entity-tolerance helpers that name bodies.
- **`brep` and `topo` should be planned as one library first.** Split by file they have 416 references
  the wrong way; the data structure can be separated from its algorithms later.
- **Two more libraries are needed:** one for the evaluation records (`eval/`, 13,000 lines) and one for
  the planar core of prisms (5,500 lines).
- **`curve` and `surface` are peers, not layers** (surface refers to curve 314 times, curve to surface
  48), and the operation crates refer to each other 38 times in the best order found.
- **Three items are a cut in every step:** `Result`, `SolidError` and `Kernel` are referenced from 635,
  559 and 486 files. The error and calling-convention work (sections 6, 7) cannot wait until last.
- **Proposed order,** with cuts per step: `num` (54), `algo` (10), `nurbs` (5), evaluation records (4),
  planar (10), `curve` (194), `surface` (349), `geomq` (83), `brep` with `topo`, then leaf operation
  crates, then Boolean, heal, hidden-line, sweep, primitives, blending, offset, conversion.

An earlier count in this session treated everything after a file's first `#[cfg(test)]` as test code.
49 files have 46,000 lines of non-test code after that point, so the earlier test-share and
function counts are slightly too low for non-test code.
