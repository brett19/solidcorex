# 08. Glossary

One meaning per word. Each entry gives:

- **Definition**: the meaning solidcorex uses (JUDGED).
- **solidcore**: what the word or type is today, cited at the pin (READ).
- **Not**: what the word must not be used for.
- **solidcorex**: the name a builder writes.

Path prefixes are as in [07-conventions.md](07-conventions.md): `k/` is the kernel, `rec/` the
record, `cv/` convert, `ops/` solidcore-ops, and `app/` the application's `crates/`.

---

## 1. The five body types

Today one model is represented by five types, across three repositories' worth of layers.

| # | Type today | Where | What it is |
|---|---|---|---|
| 1 | `solidcore::Brep` | `k/topology.rs:761` | The kernel's mutable B-rep: arenas of entities and uses, a body tolerance (`m_sThisZoneTol3d`, ~l.775), an `ObjectKey`, model size estimate, edit flags, attributes, trim-cover raises. Not `Clone` (#2715). |
| 2 | `solidcore_record::brep::Brep` | `rec/brep.rs:39` | The stored form: plain lists of regions, shells, faces, edges and vertices in canonical order, plus a `tolerance` documented as the *intersection* tolerance (`XSectTol3d`). Serde. |
| 3 | `solidcore_ops::Body` | `ops/body.rs:44` | `Mutex<Held>`: a kernel `Brep` with its lock, plus an optional `Outward` proof and the primitive `Template` it came from, dropped on any mutable access. |
| 4 | `solidcraft_smlib::Brep` (the bridge) | `app/smlib/src/lib.rs:310` | "Immutable: operations return new bodies." It wraps an ops body (`core::Body`), a proven-shell-order flag and a trace id. |
| 5 | `kernel::Body` (the app) | `app/kernel/src/body.rs:172` | The app's body. It holds its record (the identity), and lazily built cells for the bridge `Brep`, the ops `Body`, the outline, size, volume, mass and bounds. It may also be a mesh. "Cheap to clone; never mutated in place." |

Others share the names but are not bodies of the model:

- `BrepData` (`k/topology/brep_data.rs:1228`, upstream's serialised `SmBrepData`)
- `hlr::Brep` (`k/hlr/body.rs:85`, a hidden-line view)
- `step::model::Body` (`rec/step/model.rs:269`, a parsed STEP body)
- `teeth::mesh::Body` (`k/teeth/mesh.rs:31`)
- `diagnostics::Body` (`solidcore/crates/diagnostics/src/body.rs:18`)

**JUDGED. Three names in solidcorex, each one thing:**

| solidcorex | Crate | Is | Replaces |
|---|---|---|---|
| `Brep` | `brep` | A **finished body**: immutable, shared per face (`Arc<FaceRecord>`), every derived cell fillable once. | 1, and the immutability promise of 4 and 5 |
| `Draft` | `brep` | The only mutable body. Made from a `Brep` (`Brep::edit`) or empty, and ended by `Draft::finish(&Settings) -> Result<Brep>`. | the mutating half of 1 |
| `Record` | `record` | The stored form, exact, with no maths. Import copies it into a `Draft`. | 2 (renamed so it is never confused with a `Brep`) |

`ops::Body` survives only as the session's handle (a `BodyId` plus an `Arc<Brep>`), with no lock and
no proof field: the proof lives in the `Brep`'s cells. The bridge `Brep` and the app's lazily built
kernel form disappear once the app keeps live bodies (01, step G and I). The app's `Body` then
holds an `ops::Body`, not a record.

Hidden-line, STEP and mesh types are renamed so that none is called `Brep` or `Body`: `HlrView`,
`StepBody` and `ToothMesh`.

## 2. Entity kinds

For each entity: what it is in solidcorex, the solidcore type, what it is not, and the solidcorex
type name. Ids are `VertexId`, `EdgeId` and so on, as today.

| Term | Definition | solidcore | Not | solidcorex |
|---|---|---|---|---|
| **vertex** | A point of the body, with an accuracy (a ball). | `Vertex`, `k/topology.rs:386`: point, tolerance (default 0 there, `ZONE_TOL_3D` in the builder), its uses. | Not a curve end parameter. Not a UV point (that is on the vertex use). | `Vertex` |
| **edge** | A bounded piece of one 3D curve between two vertices (equal for a closed edge), with an accuracy (a tube). It is shared by every face that meets along it. | `Edge`, `k/topology.rs:413`: `vertices`, `edgeuses` in radial order (the first is primary), `interval`, `tolerance`, `curve`. | Not a trim. Not a side of one face; that is an edge use. | `Edge` |
| **loop** | A closed chain bounding one face: one outer loop, then any holes. | `Loop`, `k/topology.rs:443`. `LoopEdge` (`:434`) is an oriented edge in it. | Not a wire. Not a loop *use*: a loop has two uses, one per face side. | `Loop` |
| **face** | A bounded piece of one surface: its loops, a side bit, an accuracy. | `Face`, `k/topology.rs:458`: shell, loops, surface, primary face use, `uv_domain` (the surface's domain), `rectangular_trim`, tolerance (default 0). | Not a surface. Not one side of itself; that is a face use. | `Face` (in a `FaceRecord`, shared by `Arc`) |
| **shell** | A connected set of faces (or wire edges, or a lone vertex) forming one boundary between regions. | `Shell`, `k/topology.rs:502`. A closed boundary is two runtime shells, one per side; the record makes them one (`cv/brep/record.rs:20-22`). | Not a solid. Not a region. | `Shell` |
| **region** | A connected volume of space: the infinite region, or a bounded one that is material or a void. | `Region`, `k/topology.rs:517`, with `is_void` as `i32`. In the record, `regions[0]` is the infinite region (`rec/brep.rs` `Region`). | Not a lump of material; a void is a region too. | `Region`, with `is_void: bool` |

### The four uses

A **use** is an entity seen from one side, or by one owner. Uses carry the orientation and the
links. The entities carry geometry and accuracy.

| Use | Definition | solidcore | Not | solidcorex |
|---|---|---|---|---|
| **face use** | A face seen from one of its two sides. `Same` when its normal is the surface's natural normal. The two uses are mates. | `Faceuse`, `k/topology.rs:529`: shell, face, mate, orientation, loop uses (outer first). | Not a face. Not the material side; which use faces material is a property of the shell. | `FaceSide` (two per face, held inside `FaceRecord`) |
| **loop use** | A loop as used by one face use. `Same` for outer, `Opposite` for inner. | `Loopuse`, `k/topology.rs:560`: face use, loop, mate, start, orientation. | Not a direction of travel. | `LoopSide` |
| **edge use** | An edge as used by one loop use: a direction along the edge, links to the next and previous uses in the loop (`ccw` / `cw`), its start vertex use, and on the `Same` member of a mate pair, the UV trim (pcurve). | `Edgeuse`, `k/topology.rs:589`, with inline gap caches (`max_edge_face_gap`, `edge_ccw_edge_gap`) and `drop_curve_fail`. | Not an edge. Not a trim, though it holds one. | `EdgeUse` (in tables, per research 51) |
| **vertex use** | A vertex as used at the start of one edge use, or as a vertex loop, or alone in a shell. Holds the UV point on that face. | `Vertexuse`, `k/topology.rs:650`: vertex, owner, vertex/edge and vertex/face gap caches, `uv_point` (default `BIG_DOUBLE`). | Not a vertex. | `VertexUse` |

Research 51 measured use records at 71 to 80% of a body's bytes. Their layout is open (04 §6).

### The radial-edge structure

An edge in a manifold solid is shared by two faces. Each face has two sides, so the edge has
**four edge uses**. Each pair of faces meeting at the edge contributes two uses, one on each side of
each face. The edge keeps its uses in **radial order** around it, starting with the primary use
(`Edge::edgeuses`, `k/topology.rs` ~l.421).

Two links connect the uses:

- **Mate.** `mate` gives "the oppositely oriented edge use on the face's other side": the same face,
  flipped.
- **Radial.** `radial` gives "the oppositely oriented edge use on the adjacent face": across the
  edge, in the same shell.

Both are read off that radial list, by whether a use's orientation matches the primary's
(`k/topology/edgeuse/p1.rs:504`, `:521`). Orientations alternate `Same`, `Opposite` around the edge
(validity doc §9 invariant 3). A non-manifold edge where k faces meet has 2k uses, in the same
alternating order.

```
                          outside (empty region)
                                   ^
                       A_out       |       B_out
   face A  ==================>  [edge e]  <==================  face B
           <==================     |     ==================>
                       A_in        |       B_in
                                   v
                          inside (material)

   e is seen end-on; curve C(t) runs out of the page.
   A_out : A's outside side, runs with C (Same)      <- holds A's pcurve
   A_in  : A's material side, runs against C (Opposite)
   B_in  : B's material side, runs with C (Same)     <- holds B's pcurve
   B_out : B's outside side, runs against C (Opposite)

   radial order round e : A_out -> A_in -> B_in -> B_out -> (A_out)
                          Same     Opp     Same    Opp       alternating
   mate   : A_out <-> A_in,  B_in <-> B_out     (one face, its two sides)
   radial : A_out <-> B_out, A_in <-> B_in      (adjacent faces, one shell)
```

Read the picture this way. The boundary between material and empty space is two shells, one seen
from each side. `A_out` and `B_out` lie in the shell facing the empty region; `A_in` and `B_in` lie
in the shell facing the material. A loop on face A's outer side runs along `A_out`. Crossing the
edge within the same shell reaches `B_out` by the radial link, and that use runs the other way, as
adjacent loops always do. The `Same` member of each mate pair holds its face's pcurve (`A_out` for
A, `B_in` for B), so an edge between two faces has exactly two pcurves. The record collapses the two
runtime shells of a closed boundary into one (`cv/brep/record.rs:20-22`) and keeps one oriented edge
per face, with `upper = !forward`.

## 3. Loose terms in today's code

These words are each used for several things in solidcore. solidcorex gives each one meaning, or
retires the word.

| Term | solidcore uses it for | Not (in solidcorex) | solidcorex |
|---|---|---|---|
| **cache** | Memos on `&Kernel`, such as `curve_analysis`, `blend_station_memo`, `loop_area_memo` and `check_memo` (`k/kernel.rs:136` on, ~1315 on). Derived fields on entities (gap caches on uses, `Edge::vertices`). Face caches 5–160× the body (research 51). Keyed by `k.tolerances` (`FaceCacheKey`, `CurveMemoKey`; research 31). | Never state that changes a result. Never something a reader writes and another reads as truth. | Two words. **Cell**: a fill-once derived value stored with a `Brep` entity, a pure function of what it is declared to read. **Memo**: an entry in `Scratch`, discarded freely, keyed by input content. |
| **trim** | (a) A UV trim curve or pcurve on an edge use (`Edgeuse::uv_trim_curve`). (b) A face's loops as a whole (`rectangular_trim`, "trimmed surface"). (c) `Curve::trim`, cutting a curve's interval. | Not the loops. Not an interval cut. | **pcurve**: the 2D curve of an edge use in its face's parameters. **boundary**: a face's loops. `Curve::restrict` cuts an interval. |
| **tolerance** | Body zone, entity zone, xsect, approx, angle, contact, coincidence, fit tolerance (see [09-tolerances.md](09-tolerances.md) §1). | Not one number. | **accuracy** (per entity) or **resolution** (per operation). There is no bare `tolerance` identifier. |
| **gap** | A measured distance between an entity and one it bounds (edge/face, vertex/edge, edge/edge), sampled at 20 points (`calc_max_edge_face_gap`; validity doc §5.2). Cached on uses. | Not a bound, and not an accuracy. | **gap**: a measured distance, as a diagnostic only. A proved one is a **bound** (below). |
| **use** | A topology use (above), and also ordinary English. | Not a verb in identifiers (`uses_face`). | Only `FaceSide`, `LoopSide`, `EdgeUse`, `VertexUse`. |
| **settled / prepared / finished** | "Prepared" bodies have their UV trims made (`create_missing_uv_trim_curves`, `PreparationRequired` error, `k/topology/edgeuse/p3.rs` ~l.3960). "Settled" tolerances follow the tolerance pass. "Finished" is used for `Assembly::finish` in the app. | Not three states of one type. | **finished** only: a `Brep`, as returned by `Draft::finish`. There is no unprepared `Brep`, so "prepared" and "settled" are retired. |
| **certificate / proof / bound** | Certificate: ε with a proof that a net is within ε of its definition (solidcore `docs/definitions-and-certificates.md` §4.1). Proof: ops' `Outward`, a proof the body is valid and faces out (`ops/body.rs:62`). Bound: a certified number, `Verdict` answers (`k/geom/certified.rs:56`). | Not a sampled check. | **bound**: an `f64` proved to be above a quantity. **certificate**: a stored bound together with what it is about and the inputs it depends on (09 §5). **proof**: a certificate whose claim is a predicate (valid, outward) rather than a distance. |
| **record** | (a) The stored form `solidcore_record::brep::Brep`. (b) The kernel's "notify records" (`k/kernel.rs` `notify_records`). (c) An arena slot ("face record"). | Not a notification log. | **Record**: the stored form only. Arena slots are **entries**. Notify maps become `Outcome`. |
| **draft** | (a) A draft-angle feature (`Draft` blend definitions, research 41). (b) The architecture's mutable body (01). | | **`Draft`**: the mutable body. The feature is **taper** (`TaperDef`, `ops::taper`). |

## 4. Architecture terms

These are defined in [01-architecture.md](01-architecture.md). They are restated here so that one
page holds every meaning.

| Term | Definition | solidcore today | Not | solidcorex |
|---|---|---|---|---|
| **finished body** | A `Brep`. All pcurves exist, accuracies are final, every invariant of validity doc §9 holds, and every later computation on it is a pure function of it. | No such type. `Brep` is mutated by readers (research 45: the tree build writes in 393 builds). | Not "valid and outward", which is a proof stored in a cell. | `brep::Brep` |
| **draft** | The only mutable body. Operations prepare, compute, then apply onto it. `finish` checks invariants, fits what remains, and seals. | `&mut Brep` everywhere. | Not a copy that is thrown away on error. A failed `finish` returns the draft's error and no body. | `brep::Draft` |
| **fill-once cell** | A slot on a `Brep` entity holding a derived value: a box, a tree, a pcurve's bound, a proof. Filled at most once, by a pure function of declared inputs. Shared reads, no invalidation (a changed entity is a new entity). | Lazy caches cleared by notification (`notify_records`, research 51). | Not a memo; it is never evicted. Not a field a draft writes. | `brep::Cell<T>` (`OnceLock`-like, with the declared reads in its type) |
| **certificate** | See §3 and 09 §5. | ε on procedural faces; `TrimCoverRaises` (`k/topology.rs:869`) records some raises. | | `brep::Cert` |
| **accuracy** | Per entity: a proved bound on how far the stored entity is from the exact one it stands for. | Entity `tolerance` fields, which also serve as resolutions. | Not a coincidence threshold. | `num::Accuracy` |
| **resolution** | Per operation: how close two things must be to count as the same. A parameter, from `Settings`, scaled to the model. | `zone_tol_3d`, `contact_tol_3d`, `coincidence_tol_3d` (`k/geom/tol.rs:450`, `:519`). | Not a property of an entity. Never stored on a body. | `num::Resolution` |
| **refusal** | An operation's typed answer that the input cannot be handled, with a cause from a closed list and the entities involved. | `SolidError` variants plus upstream status numbers (research 35: status 0→1001). | Not a kernel bug. | `Refused<Cause>` |
| **defect** | An internal invariant failed: a kernel bug. | Mixed into `SolidError` with refusals. | Never shown as the user's fault. Never a reason to widen an accuracy (validity doc §8). | `Defect` |
| **settings** | The immutable per-session value: resolution policy, angle policy, seed. No fix switches. | `Kernel.tolerances`, `Kernel.context`, `Kernel.fixes` (26 switches), `Kernel.seed` (`k/kernel.rs:1315` on). | Not mutable mid-operation; nothing is set and restored. | `Settings` (in `num` for the numeric part, assembled in `ops`) |
| **scratch** | Per-worker reusable buffers and memos. Never changes a result. | The `Cell`/`RefCell` memos on `Kernel`, the ops kernel pool, and the ops per-call `thread_local!`s ([07](07-conventions.md) §7). | Not a place for a value that changes results (`blend_net_tolerance`). | `Scratch` |

## 5. Smaller terms a builder meets

| Term | Meaning in solidcorex | solidcore |
|---|---|---|
| **natural normal** | ∂S/∂u × ∂S/∂v. Loops are stored about it ([07](07-conventions.md) §4). | Same (`k/topology/face/p1.rs:1383`) |
| **primary** | The first use in an edge's radial list. The face's `Same` face use. | `GetPrimaryEdgeuse` |
| **seam** | An edge where a periodic surface's parameter wraps. Always an edge. | Validity doc §6 |
| **pole** | A point where a surface's parameter line collapses. Always a vertex. | Validity doc §6 |
| **net** | The NURBS approximation stored for a procedural face, with its certificate. | Definitions doc §3 |
| **definition** | The exact construction a procedural face stands for (offset, blend, sweep, corner). | Definitions doc §3 |
| **stored form** | The `Record`. | "record", "native brep" |
| **outcome** | What an operation reports became of each input face and edge (kept, modified, split, deleted, new). | Notify records; open in 04 §6 |

---

## Questions for the owner

1. **Names.** Accept the three body names: `Brep` (finished), `Draft` (mutable), `Record` (stored)?
   Should the app's `Body` keep its name, since users and app code already say "body"?
2. **Uses.** Rename `Faceuse`/`Loopuse` to `FaceSide`/`LoopSide`, or keep the radial-edge
   literature's "use" for all four? Keeping it is more familiar to readers of the papers. Renaming
   reads better in code.
3. **Taper.** Rename the draft-angle feature to "taper" internally, so that `Draft` is free, even
   though users will say "draft"?
4. **Trim.** Retire "trim" as an identifier in favour of "pcurve" and "boundary"? Golden unit names
   and many docs use it.
5. **Region voids.** Is a void region a first-class region (as today), or should solidcorex model a
   void as a shell attribute?
