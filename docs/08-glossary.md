# 08. Glossary

One meaning per word. Each entry gives:

- **Definition**: the meaning solidcorex uses (JUDGED).
- **solidcore**: what the word or type is today, cited at the pin (READ).
- **Not**: what the word must not be used for.
- **solidcorex**: the name a builder writes.

Path prefixes are as in [07-conventions.md](07-conventions.md): `k/` is the kernel, `rec/` the
record, `cv/` convert, `ops/` solidcore-ops, and `app/` the application's `crates/`.

Brought in line 11-26 with `01-architecture.md` v2 and `04-decisions.md`. Entries that v2 overrides are
edited and marked "(changed 11-26, see 01 §N)". Crate names are v2's: `scx-brep`, `scx-bodyq`,
`scx-record`, `scx-fates` and so on.

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
| `Brep` | `scx-brep` | A **finished body**: immutable, shared per face (`Arc<FaceRecord>`), every derived cell fillable once, carrying a stamp of the document's resolution (01 C7). | 1, and the immutability promise of 4 and 5 |
| `Draft` | `scx-brep` | The only mutable body. Made from a `Brep` (`Brep::edit`, `into_draft`) or empty, and ended by `Draft::finish(self, &Document) -> Result<(Brep, Changes), Box<Unfinished>>` (changed 11-26, see 01 §2.3). | the mutating half of 1 |
| `Record` | `scx-record` | The stored form, exact, with no maths. Import copies it into a `Draft`. | 2 (renamed so it is never confused with a `Brep`) |

`ops::Body` does not survive (changed 11-26, see 01 §2.2 and §6.2): there is no session and no
`BodyId`. A body is a value, and the app names its bodies as it likes; the proof lives in the `Brep`'s
cells. The bridge `Brep` and the app's lazily built kernel form disappear at the app switch (01 §10.2
step 13). The app's `Body` then holds a `Brep`, not a record.

Hidden-line, STEP and mesh types are renamed so that none is called `Brep` or `Body`: `HlrView`,
`StepBody` and `ToothMesh`.

## 2. Entity kinds

For each entity: what it is in solidcorex, the solidcore type, what it is not, and the solidcorex
type name. Ids are `VertexId`, `EdgeId` and so on, as today.

| Term | Definition | solidcore | Not | solidcorex |
|---|---|---|---|---|
| **vertex** | A point of the body, with an accuracy (a ball). | `Vertex`, `k/topology.rs:386`: point, tolerance (default 0 there, `ZONE_TOL_3D` in the builder), its uses. | Not a curve end parameter. Not a UV point (that is on the vertex use). | `Vertex` |
| **edge** | A bounded piece of one 3D curve between two vertices (equal for a closed edge), with an accuracy (a tube). It is shared by every face that meets along it. It has a geometry part and an adjacency part (the face sides around it in radial order; 01 §2.1). | `Edge`, `k/topology.rs:413`: `vertices`, `edgeuses` in radial order (the first is primary), `interval`, `tolerance`, `curve`. | Not a trim. Not a side of one face; that is an edge use. | `Edge` |
| **loop** | A closed chain bounding one face: one outer loop, then any holes. Changed 11-26 (01 §2.1): a list of `EdgeUse`s inside the face record. | `Loop`, `k/topology.rs:443`. `LoopEdge` (`:434`) is an oriented edge in it. | Not a wire. | a loop list in `FaceRecord` (no type of its own is needed) |
| **face** | A bounded piece of one surface: its loops and its two sides (`sides: [ShellId; 2]`). Its accuracy is derived, not stored (01 §4.1). | `Face`, `k/topology.rs:458`: shell, loops, surface, primary face use, `uv_domain` (the surface's domain), `rectangular_trim`, tolerance (default 0). | Not a surface. Not one side of itself; that is a face side. | `Face` (in a `FaceRecord`, shared by `Arc`) |
| **shell** | A connected set of face sides (or wire edges, or a lone vertex) bounding one region. Changed 11-26 (01 §2.1): a closed boundary is two shells, one per side, as at the pin's runtime. | `Shell`, `k/topology.rs:502`. A closed boundary is two runtime shells, one per side; the record makes them one (`cv/brep/record.rs:20-22`). | Not a solid. Not a region. | `Shell` |
| **region** | A connected volume of space: the infinite region, or a bounded one that is material or a void. | `Region`, `k/topology.rs:517`, with `is_void` as `i32`. In the record, `regions[0]` is the infinite region (`rec/brep.rs` `Region`). | Not a lump of material; a void is a region too. | `Region`, with `solid: bool`; `RegionId(0)` is the infinite region (01 §2.1). A sheet or lamina face has both sides in one region |

### The uses and the radial order

Changed 11-26 (01 §2.1 and choice C4; this replaces the four-uses picture and the use table). At the pin
a manifold edge has four edge uses, and use records are 71 to 80% of a body's bytes (research 51).
solidcorex keeps loop lists inside the face record, two sides per face, and the radial order on the
edge. A manifold edge then has two uses, one per face per pass of the edge, and no pointers span
records.

| Use | Definition | solidcore | solidcorex |
|---|---|---|---|
| **face side** | A face seen from one of its two sides, the front or back taken about the natural normal. Each side names the shell, and so the region, on it. Not the material side; which side faces material is a property of the region. | `Faceuse`, `k/topology.rs:529` | `FaceSide`, an index into the face's `sides` (01 §12 item 12). No record of its own |
| **loop use** | not needed | `Loopuse`, `k/topology.rs:560` | none (`LoopSide` dropped) |
| **edge use** | An edge as used by one face in one pass of its loop: the edge, the sense, the pcurve (a 2-D curve and interval on the face's surface) and the pcurve's recorded bound. Not an edge. Not a trim, though it holds one. | `Edgeuse`, `k/topology.rs:589`, with inline gap caches | `EdgeUse`, inside its face's loop |
| **vertex use** | not needed: a vertex is named by id, and the uv point comes from the pcurve | `Vertexuse`, `k/topology.rs:650` | none (`VertexUse` dropped) |

The edge's **adjacency part** lists the face sides around it in radial order. A seam edge appears twice
in one face's loop and lists the face twice, told apart by sense. A wire edge has no face sides and
belongs to its shell's wire list. A lone vertex inside a face (a cone apex) is a loop of one `EdgeUse`
on an edge collapsed to that vertex. Mates (the same edge's use on the other face) are found by scanning
the other face's loops, not by a stored link.

Each face has two sides, and each side names a shell and so a region. A solid's boundary face has its
outer side in `RegionId(0)` or a void and its inner side in the solid. A face between two solid regions
has a region on each side, which the Boolean's cellular merge needs. A lamina has both sides in one
region.

Research 51's layout question is answered by C4. The old end-on picture of `A_out`, `A_in`, `B_in` and
`B_out` is withdrawn with the four uses.

## 3. Loose terms in today's code

These words are each used for several things in solidcore. solidcorex gives each one meaning, or
retires the word.

| Term | solidcore uses it for | Not (in solidcorex) | solidcorex |
|---|---|---|---|
| **cache** | Memos on `&Kernel`, such as `curve_analysis`, `blend_station_memo`, `loop_area_memo` and `check_memo` (`k/kernel.rs:136` on, ~1315 on). Derived fields on entities (gap caches on uses, `Edge::vertices`). Face caches 5–160× the body (research 51). Keyed by `k.tolerances` (`FaceCacheKey`, `CurveMemoKey`; research 31). | Never state that changes a result. Never something a reader writes and another reads as truth. | Two words. **Cell**: a fill-once derived value stored with a `Brep` entity, a pure function of what it is declared to read. **Memo**: an entry in `Scratch`, discarded freely, keyed by input content. |
| **trim** | (a) A UV trim curve or pcurve on an edge use (`Edgeuse::uv_trim_curve`). (b) A face's loops as a whole (`rectangular_trim`, "trimmed surface"). (c) `Curve::trim`, cutting a curve's interval. | Not the loops. Not an interval cut. | **pcurve**: the 2D curve of an edge use in its face's parameters. **boundary**: a face's loops. `Curve::restrict` cuts an interval. |
| **tolerance** | Body zone, entity zone, xsect, approx, angle, contact, coincidence, fit tolerance (see [09-tolerances.md](09-tolerances.md) §1). | Not one number. | **accuracy** (per entity) or **resolution** (per operation). There is no bare `tolerance` identifier. |
| **gap** | A measured distance between an entity and one it bounds (edge/face, vertex/edge, edge/edge), sampled at 20 points (`calc_max_edge_face_gap`; validity doc §5.2). Cached on uses. | Not a bound, and not an accuracy. | **gap**: a measured distance, as a diagnostic only. A proved one is a **bound** (below). |
| **use** | A topology use (above), and also ordinary English. | Not a verb in identifiers (`uses_face`). | Only `FaceSide` and `EdgeUse`. |
| **settled / prepared / finished** | "Prepared" bodies have their UV trims made (`create_missing_uv_trim_curves`, `PreparationRequired` error, `k/topology/edgeuse/p3.rs` ~l.3960). "Settled" tolerances follow the tolerance pass. "Finished" is used for `Assembly::finish` in the app. | Not three states of one type. | **finished** only: a `Brep`, as returned by `Draft::finish`. There is no unprepared `Brep`, so "prepared" and "settled" are retired. |
| **certificate / proof / bound** | Certificate: ε with a proof that a net is within ε of its definition (solidcore `docs/definitions-and-certificates.md` §4.1). Proof: ops' `Outward`, a proof the body is valid and faces out (`ops/body.rs:62`). Bound: a certified number, `Verdict` answers (`k/geom/certified.rs:56`). | Not a sampled check. | **bound**: an `f64` proved to be above a quantity. **certificate**: a stored bound together with what it is about and the inputs it depends on (09 §5). **proof**: a certificate whose claim is a predicate (valid, outward) rather than a distance. |
| **record** | (a) The stored form `solidcore_record::brep::Brep`. (b) The kernel's "notify records" (`k/kernel.rs` `notify_records`). (c) An arena slot ("face record"). | Not a notification log. | **Record**: the stored form only. Arena slots are **entries**. Notify maps become `Changes` (01 §2.6). |
| **draft** | (a) A draft-angle feature (`Draft` blend definitions, research 41). (b) The architecture's mutable body (01 §2.3). | | **`Draft`**: the mutable body. The feature is **taper** (`TaperDef`, `ops::taper`). |

## 4. Architecture terms

These are defined in [01-architecture.md](01-architecture.md). They are restated here so that one
page holds every meaning.

| Term | Definition | solidcore today | Not | solidcorex |
|---|---|---|---|---|
| **finished body** | A `Brep`. All pcurves exist, accuracies are final, every invariant of validity doc §9 holds, and every later computation on it is a pure function of it. | No such type. `Brep` is mutated by readers (research 45: the tree build writes in 393 builds). | Not "valid and outward", which is a proof stored in a cell. | `brep::Brep` |
| **draft** | The only mutable body. Operations prepare, compute, then apply onto it. `finish` checks the structure of what the draft touched, stamps the document's resolution, rebuilds the body cells, and seals. Geometric validity is `scx-bodyq::check`, which only reads. | `&mut Brep` everywhere. | Not a copy that is thrown away on error. A failed `finish` returns the draft's error and no body. | `brep::Draft` |
| **fill-once cell** | A slot holding a derived value: a box, a tree, a derived face accuracy. Filled at most once, by a pure function of declared inputs. Changed 11-26 (01 §3): cells are emptied by every write path (a new or rewritten record starts empty), and a write to an edge's geometry part or a vertex replaces the records of the faces that use it. | Lazy caches cleared by notification (`notify_records`, research 51). | Not a memo; it is never evicted. Not a field a draft writes. | geometry cells: `OnceLock` in the geometry payload; face cells: `FaceCells` in `FaceRecord`, filled by `FaceCell::fill` (its read set is in its type); body cells: `Brep::cells` |
| **certificate** | See §3 and 09 §5. | ε on procedural faces; `TrimCoverRaises` (`k/topology.rs:869`) records some raises. | | `scx_num::Cert` (01 §4.1) |
| **accuracy** | Per entity: a proved bound on how far the stored entity is from the exact one it stands for. | Entity `tolerance` fields, which also serve as resolutions. | Not a coincidence threshold. | `num::Accuracy` |
| **resolution** | Per operation: how close two things must be to count as the same. One per document, chosen at creation from units and a size class. Changed 11-26 (01 §4.3): not a per-operation parameter. | `zone_tol_3d`, `contact_tol_3d`, `coincidence_tol_3d` (`k/geom/tol.rs:450`, `:519`). | Not a property of an entity. Not a tolerance a body sets: a `Brep` carries only a check stamp of it, and a mismatch is `Refused(ResolutionMismatch)`. | `num::Resolution` |
| **refusal** | An operation's typed answer that the input is not valid, with a cause from a closed list and the entities involved. `Unsupported` is the separate answer for valid input the method cannot complete (01 §5.1). | `SolidError` variants plus upstream status numbers (research 35: status 0→1001). | Not a kernel bug. | `Refused<Cause>` |
| **defect** | An internal invariant failed: a kernel bug. | Mixed into `SolidError` with refusals. | Never shown as the user's fault. Never a reason to widen an accuracy (validity doc §8). | `Defect` |
| **settings** | A value passed in `Cx`, immutable for a call: algorithm choices, the random seed, the work budget and the `Executor`. Changed 11-26 (01 §6.3, §12 item 8): there is no session, and the resolution is in the `Document`, not here. No angle policy, no fix switches. | `Kernel.tolerances`, `Kernel.context`, `Kernel.fixes` (26 switches), `Kernel.seed` (`k/kernel.rs:1315` on). | Not mutable mid-operation; nothing is set and restored. | `Settings` (`scx-algo`) |
| **document** | A value describing the space bodies live in: resolution, units, size class. It holds no bodies (01 §6.2). | `Kernel` and `Context` tolerances. | Not a session; not the app's design file. | `Document` (`scx-brep`) |
| **cx** | The calling convention of operations: the document, settings, `&mut Scratch`, `&Stop` and a trace (01 §6.3). Layers 0 to 4 take only what they read. | The `&mut Kernel` passed through 89% of the time. | | `Cx<'_>` |
| **changes** | The events a `Draft` records as it edits (replaced, created with origins, merged, coincident, removed) and returns from `finish`. | Notify records, provenance hooks. | Not a fates record. | `Changes` (`scx-brep`; 01 §2.6) |
| **carry** | Propagating application data held outside the body (names, colours) through `Changes` by a rule for split, merge and coincidence. | `solidcore-ops/src/provenance.rs`. | | `carry` and the `Carry` trait (`scx-brep`) |
| **placed** | An operand given with a rigid frame, so a move of a tool body costs nothing. A `Brep` has no placement (01 C14). | A transform that leaves 0% of faces untouched. | | `Placed<'_> { body, frame }` |
| **scratch** | Per-worker reusable buffers and memos. Never changes a result. | The `Cell`/`RefCell` memos on `Kernel`, the ops kernel pool, and the ops per-call `thread_local!`s ([07](07-conventions.md) §7). | Not a place for a value that changes results (`blend_net_tolerance`). | `Scratch` |

## 5. Smaller terms a builder meets

| Term | Meaning in solidcorex | solidcore |
|---|---|---|
| **natural normal** | ∂S/∂u × ∂S/∂v. Loops are stored about it ([07](07-conventions.md) §4). | Same (`k/topology/face/p1.rs:1383`) |
| **primary** | The first face side in an edge's radial list. | `GetPrimaryEdgeuse` |
| **seam** | An edge where a periodic surface's parameter wraps. Always an edge. | Validity doc §6 |
| **pole** | A point where a surface's parameter line collapses. Always a vertex. | Validity doc §6 |
| **net** | The NURBS approximation stored for a procedural face, with its certificate. | Definitions doc §3 |
| **definition** | The exact construction a procedural face stands for (offset, blend, sweep, corner). | Definitions doc §3 |
| **stored form** | The `Record`. | "record", "native brep" |
| **outcome** | What an operation reports became of each input face and edge (kept, modified, split, deleted, new). Changed 11-26 (01 §2.6): `Changes` and `carry` in the core, the typed record in `scx-fates`. | Notify records |

---

## Questions for the owner

1. **Names.** Accept the three body names: `Brep` (finished), `Draft` (mutable), `Record` (stored)?
   Should the app's `Body` keep its name, since users and app code already say "body"?
2. **Uses.** Answered by v2 (01 §12 item 12): `FaceSide` is an index into a face's sides, `EdgeUse` is
   kept, and `LoopSide` and `VertexUse` are not needed.
3. **Taper.** Rename the draft-angle feature to "taper" internally, so that `Draft` is free, even
   though users will say "draft"?
4. **Trim.** Retire "trim" as an identifier in favour of "pcurve" and "boundary"? Golden unit names
   and many docs use it.
5. **Region voids.** Answered by v2 (01 §2.1): a region is first-class, with `solid: bool`.
