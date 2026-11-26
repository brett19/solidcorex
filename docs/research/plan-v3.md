# solidcore: caches as values, bodies stable under reads (plan, version 3)

Draft, 2026-11-22, on `origin/main` 782ef9afc. Supersedes plan v2 ("reads that do not write"), which it
contains. Sources: the five investigation reports beside this file (01 geometry caches, 02 body lazy
writes, 03 kernel ambient state, 04 cache holders, 05 parallel sites). The reports are code readings:
nothing was built or run. Claims I re-checked myself are marked (checked).

Private note: the purpose is to make the kernel compatible with threading later. No threading is added
by this plan, and threading is not named in any issue, PR or commit. Each step goes up on its own merits.

## 1. What "compatible" means (the acceptance criteria)

1. **Reads take `&Brep`.** No read creates a trim, raises a tolerance or fills a field of the body.
2. **Shared values are `Sync`,** with compile-time assertions: body, surface, curve, their caches.
3. **A cache is an immutable value with an explicit key and one home.** No mode flags in it, no
   take-out and put-back, no answer that depends on what ran before.
4. **Ambient state is separated:** shareable configuration, per-call work state, and an apply context
   for topology edits.
5. **Results do not depend on evaluation order.**

## 2. Rules that bind the plan (solidcore/AGENTS.md, checked)

- "Same result, any thread count ... no shared mutable state, no order-dependent reductions, no lazily
  filled caches without synchronisation. State that latches on first use ... goes ... nothing new may
  be added like it" (301-305). This is the rule most steps can cite.
- "No global or static/thread_local state. Everything lives in Kernel" (103); "no threads in library
  code for now" (104-105).
- "Clearing must never change results, only speed" (kernel.rs:134).
- "Anything not reported waits": a PR names the issue it fixes (45-46). Speed-ups need a count on a
  case slow for users (42-44).
- "No compatibility flags": a fix changes the default result (234-238). Existing flags are retired.
- A moved golden needs old and new values and why the new one is right (144-148).

## 3. What the investigation found (one paragraph each; details in the reports)

**The body (02).** About 207 functions in the read modules take `&mut Brep`; about 15 are plumbing only.
All real writes go through eight leaves: the trim setters, the two trim-creation routines, the gap
calculators in caching mode, the owner-cleared scope, the mode setters, the tolerance setters, and the
surface cache slot. Trim creation makes 13 writes, 6 crossing faces, and has no measure-only form.
`validate_only` suppresses one. 23 kernel sites copy a body so a read can scribble; the ops measure
path copies a body per call. The body has no version; one protocol compares its address. Gap memos go
stale (`edge::set_interval` notifies nothing; one memo is never reset).

**Geometry caches (01).** No query refines a tree after build (checked: every subdivide caller is build,
tess or grid code), so a built tree can be immutable. But the build reads `doing_boolean`, the body's
editing flag, the blend net scope and the face tolerance; and cache presence changes answers. A shared
`&Surface` is mutable: offset distance (24 sites) and two B-spline scope flags (about 60 sites) are set
through `&self`. Five kinds of copy; `Clone` changes geometry (knot fix-up). The four value-keyed memos
(curve cache, boundary, singularities, continuity) are the pattern to converge on.

**Holders (04).** A face's trimmed cache has four homes that never exchange caches; three always build
fresh. Only the surface slot sees `face_was_modified`; the others see no edits. Every consumer works on
a deep copy (`map_handles`). Eight call sequences give the same face different caches. A long list of
setters does not invalidate the trimmed cache. Mutable uses of a cache by solvers are lazy extras, two
mode flags flipped per solve, and build guards: "immutable cache plus per-query scratch" is feasible.

**Kernel (03).** 35 leaf fields: 4 configuration, 10 work state, 5 memos, 7 dynamic scope, 7 history.
Of about 1,232 `&mut Kernel` functions, about 89% only pass it through; the direct causes belong to
topology editing (notify context and identities, mark locks, `doing_boolean`) except two RNG draw
sites. History: the RNG (shell classification dither), the fillet step latch (bypassed when its fix
flag is on), and context size scalars that `Brep` setters write into the kernel. No kernel body or
kernel is shared across threads anywhere today; the mesher and hidden lines run on the record.

**Fork points (05).** Used here only to define a *prepared body*: every edge use has its trim or a
recorded failure; tolerances final; gap memos filled; each face's trimmed cache built; analytic NURB
forms made; spatial cache built; not mid-edit; kernel latches settled. Preparing is a fixpoint (a trim
build fetches a cache that can attach another trim). Inside a Boolean, curve classification raises
tolerances and can rewrite an edge curve; that is an edit inside an operation, not a read.

## 4. Target shape

| Level | Holds | Key | Replaces |
|---|---|---|---|
| Curve value | curve cache | kernel tolerances + fixes, owner tolerance | (exists; make hits zero-copy, multi-entry) |
| Surface value | face-free decomposition; continuities; singularities; boundary curves; made NURB | kernel tolerances + fixes, domain | per-solve rebuilds, `PoleMemo`, cache-held duplicates, two NURB slot kinds |
| Face of a body | implanted boundary + node classes, or the build failure | face version, kernel tolerances + fixes | surface slot and the three operation maps |
| Body | spatial trees, box, seam flags, gap table | body version | `OnceCell` holders, gap fields on records |
| Operation context | handle views; editing/owner-cleared/validate modes | n/a | `map_handles` copies; mode fields on the body |
| Per-call work state | solve mode flags, visit lists, counters, logs | n/a | flags inside caches; kernel `Cell`s |

Three rules: (a) every lazily written field is classified meaning, memo or scratch, and only meaning is
written through `&mut` by operations; (b) a memo's key lists every input its build reads; (c) an
operation returns a completed body, so nothing downstream needs to create anything.

## 5. Steps

Size: S under a day, M days, L a week or more, XL many PRs. "Moves results" means goldens can change
and each change needs its proof.

### Tier 0. Foundations (no result change; start now, independent)

| # | Step | Size |
|---|---|---|
| 0.1 | Compile-time `Send + Sync` assertions for what already passes (`Curve` is asserted; add `Surface`, `Tree`, the three caches, `Brep` and records). What fails becomes the list. | S |
| 0.2 | The read-purity guard (v2 step 2) at the eight write leaves: a per-body lazy-write counter that read entry points compare, plus digests in the goldens runner and kernel rig; an offender list that only shrinks. | M |
| 0.3 | Remove dead cache machinery: `BrepCacheSlot` (never constructed, checked), the Boolean's no-op `delete_stale_caches`. The ported `CacheStore` is still used by five goldens units and three kernel signatures (checked), so it goes only with a decision on those units. | S |
| 0.4 | Commit the field classification (meaning / memo / mode / derived topology) as a kernel doc, from report 02 section 1 and 03 section 1. The guard's digest is defined by it. | S |

### Tier 1. Defects that stand alone (each is or becomes a reported issue; moves results)

| # | Step | Size |
|---|---|---|
| 1.1 | Land #20694 (fast-drop bit, #20179). Watch #20785 (another contributor's draft retiring the stale-trims flag); do not collide. | in flight |
| 1.2 | Standing copies get their own cache slot (v2 step 1). Still live: `topology/brep/mirror.rs:349` transforms a standing copy that shares the original's slot (checked that the call is there; the effect is from report 02). | S |
| 1.3 | The validity check is pure (v2 step 3). | M |
| 1.4 | Stale gap memos: reproduce a wrong answer after `set_interval`, file it, fix by reset. Superseded later by 3.3. | S |
| 1.5 | History-dependent trimmed cache: file the eight sequences in report 04 section 3.6 that are reachable from a real operation, with repros. These are the issues Tier 3 and 4 fix. | M (measurement) |

### Tier 2. Enablers (independent of each other; can run in parallel sub-sessions)

| # | Step | Moves results | Size |
|---|---|---|---|
| 2.1 | Surfaces stop being mutable through `&self`: offset distance, the two B-spline scope flags, `pending_extension`. Each becomes an evaluation argument or an explicit modified copy. | no | L |
| 2.2 | Solve modes leave the cache: `point_test_enabled` and `process_boundary_curves` become solve arguments. Removes a whole-cache clone per global point solve. | no | M |
| 2.3 | One memo type for value-keyed memos: complete key, several entries, `Arc` hits with no deep copy. Move the four existing memos onto it; extend past the five whitelisted surface kinds where the kind is a pure value. | no | M |
| 2.4 | Body and face versions (v2 step 6): fields the caches read become private, setters bump the face version; replaces the address comparison in ray preparation. | no | L |
| 2.5 | Kernel history removed: retire the fillet latch with its flag (the flag-on path is what SolidCraft runs); move the context size scalars onto the body; make the two shell-classification draws a function of the shell or face, not of the generator's position. | yes (RNG, maybe latch) | M |
| 2.6 | Dynamic scope made explicit: the blend net tolerance becomes an evaluation argument or a key input; mark epochs move from `Context` into each traversal; delete `doing_boolean` (one behavioural reader found, `surface/surface/p1.rs:1045`; the other reads look like save and restore, not yet confirmed one by one). | yes (`doing_boolean`) | M |

### Tier 3. The body is stable under reads (the crux)

| # | Step | Moves results | Size |
|---|---|---|---|
| 3.1 | A kernel-level completion pass: all trims, in a fixed per-edge order, raises settled, run to a fixpoint. `to_record` in `solidcore-convert` already walks edge uses in a fixed order and is the starting point. Every public operation calls it before returning (v2 step 5). | yes | L |
| 3.2 | Mode state leaves the body: `editing_enabled` (223 set sites), the owner-cleared scope, `validate_only`, the locked box become operation context. | no | L |
| 3.3 | Gap memos leave the records: filled by completion into a body-level table keyed by version, or computed on demand without storing. | maybe | M |
| 3.4 | Reads that write back on purpose (`CurveOnClassifyCache::apply_*`) and the Boolean's mid-operation raises and edge rewrites are routed through the existing named-edit adapters, so every write inside an operation is an explicit edit. | no | M |

### Tier 4. One home per cache

| # | Step | Moves results | Size |
|---|---|---|---|
| 4.1 | Face-free surface decomposition as a memo on the surface value. Open point: the face tolerance grows node boxes and sets the singularity tolerance today, so it is either a key input or moves to the face overlay. | maybe | L |
| 4.2 | The face's trimmed cache becomes a keyed value held with the face (key: face version, kernel inputs). Needs 2.2, 2.4, 3.1, 3.2. Open point: implanting trims subdivides the base tree, so the overlay is not a pure annotation of 4.1's tree. | yes | XL |
| 4.3 | Handle views replace `map_handles` copies: caches store local ids, consumers supply the mapping and the solution-object facts. | no | L |
| 4.4 | Delete the surface slot (`Stored`/`CheckedOut`, `cleared_for`, `face_modified_while_out`), the three operation maps, `alt_caches`, and the keyless held slots. | no | M |
| 4.5 | Spatial cache, box and seam flags held with the body under its version. | no | M |

### Tier 5. Signatures follow the facts

| # | Step | Size |
|---|---|---|
| 5.1 | Flip the read stack to `&Brep`, bottom-up from the write leaves, as the guard's offender list empties. Remove the 23 scribble copies and the ops measure copy. | XL (mechanical) |
| 5.2 | Remove the restart protocol (`PreparationRequired`, ray preparation, the query-topology modes) and the side tables report 04 section 7 lists. | L |
| 5.3 | Split `Kernel`: shared configuration; per-call work state (counters, logs, memos); apply context (callbacks, identities, record scratch). Then drop `&mut Kernel` from the pass-through functions. Unify the two pools' reset rules (`solidcore-ops` and `crates/smlib`). | XL (mechanical) |
| 5.4 | `Sync` bounds on the identity environments (`&dyn Fn(EntityRef) -> ObjectRef`, about 100). | M |

### Tier 6. Later, only if still wanted

- Owner data off geometry values; fewer kinds of copy; an explicit "same surface" identity.
- Identifiers that are numbered by visit order (geometry keys, handles, node ids, lazily issued identity).
- The Boolean's order coupling between face pairs (tolerance raises seen by later pairs). This is an
  algorithm change, not a cache change, and is outside "compatible".

## 6. Dependencies

```
0.1 0.2 0.3 0.4          (now)
1.2 1.3 1.4              (now)           1.5 -> issues for 3.x, 4.x
2.1 2.2 2.3 2.5 2.6      (independent)
2.4 ---------------------------+
3.1 -> 3.3                     |
3.2 ---------------------------+-> 4.2 -> 4.4 -> 5.1 -> 5.2
2.2 ---------------------------+
2.3 -> 4.1 --------------------+
4.3 -> 4.4
2.5 2.6 3.2 -> 5.3 -> 5.4
```

The critical path is 2.4 and 3.1 into 4.2. Everything in Tier 0 to 2 is worth having even if the plan
stopped there.

## 7. Measurements needed (each needs a build; coordinate with the swarm session)

| # | Question | How |
|---|---|---|
| M1 | Do repeated cache builds cost real time? | Count cache and tree builds per face in a Boolean on kernel cases and on #14453's case. |
| M2 | Does completing trims at the end of each operation change tolerances or results? | Prototype 3.1 behind a local switch; run the goldens; classify what moves. |
| M3 | Which reads write today? | Run 0.2's guard over goldens and kernel rig; that is the offender list. |
| M4 | What is not `Sync` today? | `cargo check` with 0.1's assertions. |
| M5 | Do app-facing reads give history-dependent answers (v2 step 0)? | Rig sequences on the ops layer. |
| M6 | Memory of holding a trimmed cache per face for a body's life. | Size counters on a few large bodies. |

## 8. Decisions needed

1. **Completion is a kernel property** (every kernel operation returns a completed body), not only an
   ops-layer one. Recommended: yes. Cost for kernel users who never convert to a record: M2 measures it.
2. **Version granularity:** face-level with private fields and setters. Recommended over a body-wide
   counter (too coarse for 4.2) and per-entity counters (edges are shared copy-on-write).
3. **How steps are reported,** given "anything not reported waits" and that threading is not to be an
   issue. Recommended: steps that fix a wrong or history-dependent result cite their own defect issue
   (1.5 produces them); refactor-only steps cite the existing rule text at AGENTS.md 301-305 and a hub
   issue about results depending on cache history.
4. **The ported `CacheStore`** and the five goldens units that exercise it: retire them, or leave.
5. **The Boolean's order coupling** stays out of scope. Recommended: yes.
6. **RNG:** make the two draws position-independent (moves goldens once), or remove the dither.

## 9. Known gaps in the evidence

- Nothing was compiled: every auto-trait statement is from reading field types.
- `Kernel::with_sampled_trim_raise` does have a production caller (`crates/smlib/src/core/convert.rs:303`,
  checked); report 02 says it has none because app crates are outside the sparse checkout.
- Whether `doing_boolean` has exactly one behavioural reader is not confirmed line by line.
- Report 02 lists six items it could not verify (whether `Brep` is `Sync`, what `merge_brep` copies
  share, and four others).
- How many goldens move at 3.1 and 4.2 is unknown until M2.
- No reviewer has attacked this version yet.
