# 49: Module dependencies of the kernel crate

Rewrite, phase 0. Read-only analysis of which part of the 1.16 M-line crate refers to which, written to plan the order in
which `num`, `algo`, `nurbs`, ... are cut out (`01-architecture.md` section 2). Every statement is labelled **MEASURED** (produced by
the scripts in `49-data/`) or **JUDGED** (my reading; the evidence is given). Nothing was built or run; no repository was touched.

## 1. What was analysed (MEASURED)

- Source: a local worktree, branch `cache-leaf-split-2`, commit
  `53a7f148562b5c83c8798d6314cbda9060987fdd` ("solidcore: format the normalised evaluation's calls after the rename", 2026-11-25),
  working tree clean (`git status --short` empty). No fetch was done; this is what was checked out.
- 855 `.rs` files, all reachable from `lib.rs` through `mod` declarations (0 orphans); 32 of them are test-only (38,865 lines).
  894,071 non-test lines in 823 files; 25,141 non-test functions (the architecture document says "about 25,300").
- Sibling and app crates: the same worktree, `solidcore/crates/*` and `crates/*`.

## 2. Method and its blind spots

Scripts (all in `49-data/`, run with `python3 -I`): `depmap.py` (extraction), `graph.py` (graphs, cycles), `layers.py` (the proposed
file-to-library mapping: the one JUDGED input), `analyse.py` (cuts, library numbers), `order.py` (extraction order), `extras.py`,
`siblings.py` (crates outside the kernel), `make_report.py` (this file). Rerun: `depmap.py SRC OUT` then the others with `OUT`.

**Extraction (MEASURED).**
- Each file is read with comments, string, raw-string and char literals blanked, then tokenised. Modules come from `mod x;`
  declarations (honouring `#[path]`, raw identifiers such as `r#loop`, inline `mod x { }`), not from file names.
- A reference is one of: a leaf of a `use` tree (nested braces, `self`, `as`, globs); a `crate::`, `super::`, `self::` path in an
  expression or type (including `..crate::x` struct-update paths and `$crate::` in macros); or `alias::item` where `alias` is an
  imported module (7,202 such rows). `super::` chains are resolved to absolute module paths from the module tree.
- Every path is resolved through `pub use` chains and globs to the **defining** module and item (so `crate::math::Vec3` is an
  edge to `geom::vector3d`; 1,082 references name a module that only re-exports). Result: 0 unresolved; 408 `std`/`core` and 3 external
  `use` rows skipped. A `use x::module;` that is then used as `module::item` is dropped in favour of the `module::item`
  rows (1,122 rows), so a module import is not counted twice.
- "References" = rows (use-leaf lines + path expressions). "Name-use sites" = uses of an imported name elsewhere in the same file
  (the thing you actually edit when you cut): 178,636 of them, each tagged with a syntactic context (fn parameter, return, field,
  impl header, body ...). The one-line classifications in the cut lists come from that context and the item's kind. They are
  heuristics: a file-wide name match, no scoping.
- Items: `fn`, `struct`, `enum`, `trait`, `type`, `const`, `static`, `mod`, macro-defined ids (`typed_id!` and four similar,
  which define `FaceId` and friends). `lib.rs` re-exports (22 rows) are left out of the graphs.

**Test code (MEASURED).** Excluded: every item carrying `#[cfg(test)]` or `#[test]` at any depth (the item is skipped to its end),
every file declared by a skipped `mod x;` (31 files) and the file with an inner `#![cfg(test)]`; that is 32 files. The suggested rule
"everything after the first top-level `#[cfg(test)]`" was **not** used: 49 files carry non-test code after their first `#[cfg(test)]`
(46,414 non-blank non-comment lines), which that rule would have dropped. Features are treated as on (`wr57-replay`, `profile`).

**Blind spots (known).**
- Macros: only paths written out in a macro body are seen (`macro_rules!`: 33 in non-test code). Items a macro defines are
  known only for the five id macros plus `UpstreamFixes` (hand-added). A reference produced only by expansion is missed.
- Glob imports: one in non-test code (`profile.rs`: `enabled::*`). Names reached through a glob resolve for `pub use` chains, but a glob
  import row has no item.
- Name uses ignore shadowing and scope; a local variable with an imported function's name is filtered only by requiring `(` or `::`.
- A cross-check: of 14,090 non-test lines containing `crate::` in 774 files, 25 (all `..crate::x::default()` struct-update lines) were
  missed on a first run; they are now caught. Lines were compared as a set per file; the check does not cover `super::` or alias paths.
- Trait method calls and inherent methods are not paths: a call `x.surface()` creates no row, only the import of the type does.
  The dependency is real but invisible here, which makes counts a lower bound on coupling (type-level coupling is complete).
- Mixed areas: the library mapping is by whole file. A file that mixes layers shows its upward references as that library's cuts
  (listed in section 8) rather than being split.


## 3. The area graph (MEASURED)

26 top-level areas (`lib.rs` excluded), 252 directed edges, 23,572 cross-area references of 35,701 in all. Cell = number of references from the row area to the column area. Full table with files and distinct items: `49-data/area_edges.tsv`; second-level graphs: `mod2_edges.tsv` (all 386 second-level nodes) and `mod2_split_edges.tsv` (only the eight areas to be split expanded).

| from \ to | math | limits | geom | nurbs | eval | curve | surfac | inters | topolo | query | mass | blend | boolea | offset | sweep | primit | heal | prism | hlr | develo | cell_g | hatch | mesh_r | profil | error | kernel |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| math |  |  | 10 | 3 |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | 17 |  |
| limits |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| geom | 109 | 6 |  | 12 |  | 6 | 12 | 2 | 75 | 13 |  | 7 | 2 |  |  |  | 2 |  |  |  |  |  |  |  | 83 | 11 |
| nurbs | 48 | 18 | 198 |  |  | 52 | 56 | 4 | 11 | 8 | 1 | 10 |  | 3 |  |  |  |  |  |  |  |  |  |  | 202 | 51 |
| eval | 1 |  | 21 | 19 |  |  |  |  |  |  |  |  |  |  |  |  |  | 4 |  |  |  |  |  |  | 6 |  |
| curve | 131 | 1 | 644 | 166 | 12 |  | 44 | 2 | 45 |  | 1 | 13 |  |  |  |  |  | 15 |  |  |  |  |  |  | 166 | 45 |
| surface | 232 |  | 1082 | 444 | 21 | 195 |  | 27 | 78 | 3 | 1 | 100 |  | 9 | 9 |  |  |  |  |  |  |  |  | 4 | 352 | 74 |
| intersect | 119 |  | 586 | 118 | 2 | 121 | 197 |  | 273 | 35 | 2 | 51 |  |  | 1 |  | 6 |  |  |  |  |  |  | 3 | 211 | 40 |
| topology | 142 |  | 1039 | 225 | 2 | 178 | 241 | 147 |  | 210 | 16 | 13 | 12 | 6 |  | 2 | 63 |  |  |  |  |  |  |  | 498 | 74 |
| query | 37 | 1 | 273 | 76 | 3 | 43 | 125 | 54 | 397 |  | 4 | 57 |  | 1 |  | 1 | 28 |  |  |  |  |  |  |  | 130 | 29 |
| mass | 6 |  | 19 | 14 |  | 1 | 4 |  | 63 | 12 |  | 21 |  |  |  |  |  |  |  |  |  |  |  | 1 | 17 | 4 |
| blend | 235 |  | 918 | 382 |  | 222 | 436 | 226 | 3306 | 107 | 5 |  | 25 | 4 | 5 | 4 | 11 |  |  |  |  |  |  | 42 | 838 | 144 |
| boolean | 23 |  | 141 | 31 |  | 22 | 34 | 69 | 579 | 34 |  |  |  |  | 1 | 5 |  |  |  |  |  |  |  | 3 | 184 | 18 |
| offset | 30 |  | 225 | 83 |  | 28 | 106 | 15 | 832 | 39 | 1 | 62 | 10 |  | 5 | 4 | 14 |  |  |  |  |  |  | 2 | 115 | 23 |
| sweep | 14 |  | 138 | 83 | 1 | 45 | 33 | 6 | 313 | 14 |  | 7 | 18 |  |  | 12 | 8 | 5 |  |  |  |  |  | 5 | 87 | 26 |
| primitive | 42 | 1 | 168 | 88 |  | 77 | 33 |  | 240 | 9 | 1 | 2 | 23 |  | 36 |  | 8 |  |  |  |  |  |  |  | 108 | 26 |
| heal | 22 |  | 85 | 24 |  | 22 | 15 | 9 | 540 | 17 |  | 10 | 3 |  |  |  |  |  |  |  |  |  |  |  | 48 | 8 |
| prism | 1 |  | 40 | 15 | 7 | 8 | 3 |  | 15 | 11 |  |  |  |  | 1 |  |  |  |  |  |  |  |  |  | 13 | 5 |
| hlr | 1 |  | 9 | 4 | 29 |  |  |  |  |  |  | 1 |  |  |  |  |  | 1 |  |  |  |  |  |  |  |  |
| develop |  |  | 16 |  | 11 |  |  |  |  |  |  |  |  |  |  |  |  | 22 |  |  |  |  |  |  |  |  |
| cell_grid |  |  | 7 |  |  |  |  |  |  |  |  |  |  |  |  |  |  | 9 |  |  |  |  |  |  |  |  |
| hatch |  |  | 16 |  |  |  |  |  |  |  |  |  |  |  |  |  |  | 10 |  |  |  |  |  |  |  |  |
| mesh_reduce |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | 4 |  |
| profile |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| error |  |  | 4 | 1 |  |  | 1 | 5 | 63 |  |  | 31 |  | 3 |  |  |  |  |  |  |  |  |  |  |  |  |
| kernel | 1 |  | 1 | 2 |  |  | 5 |  | 4 |  | 1 | 4 |  |  |  |  |  |  |  |  |  |  |  | 3 | 3 |  |


The thirty heaviest edges, with the three weights asked for:

| from | to | references | referencing files | distinct items |
|---|---|---|---|---|
| blend | topology | 3306 | 111 | 306 |
| surface | geom | 1082 | 81 | 182 |
| topology | geom | 1039 | 70 | 155 |
| blend | geom | 918 | 136 | 117 |
| blend | error | 838 | 148 | 59 |
| offset | topology | 832 | 19 | 201 |
| curve | geom | 644 | 47 | 161 |
| intersect | geom | 586 | 41 | 129 |
| boolean | topology | 579 | 16 | 212 |
| heal | topology | 540 | 8 | 156 |
| topology | error | 498 | 77 | 24 |
| surface | nurbs | 444 | 68 | 70 |
| blend | surface | 436 | 104 | 83 |
| query | topology | 397 | 21 | 127 |
| blend | nurbs | 382 | 94 | 31 |
| surface | error | 352 | 81 | 22 |
| sweep | topology | 313 | 18 | 117 |
| intersect | topology | 273 | 18 | 111 |
| query | geom | 273 | 30 | 85 |
| topology | surface | 241 | 52 | 73 |
| primitive | topology | 240 | 22 | 92 |
| blend | math | 235 | 85 | 12 |
| surface | math | 232 | 57 | 18 |
| blend | intersect | 226 | 74 | 42 |
| topology | nurbs | 225 | 51 | 33 |
| offset | geom | 225 | 23 | 79 |
| blend | curve | 222 | 85 | 27 |
| intersect | error | 211 | 40 | 20 |
| topology | query | 210 | 39 | 78 |
| nurbs | error | 202 | 67 | 11 |


Reading it (JUDGED): `error` and `kernel` are referenced from nearly everything (section 5.3). Below them, the dense core is
`blend`, `topology`, `surface`, `intersect`, `offset`, `curve`, `query`, with `geom` and `nurbs` referenced by all of them. The
edges that point the wrong way for the target layering are in section 5.


## 4. Cycles (MEASURED)

**Area level.** One strongly connected component of 19 of the 26 areas: blend, boolean, curve, error, eval, geom, heal, intersect, kernel, mass, math, nurbs, offset, primitive, prism, query, surface, sweep, topology.
Outside it: `cell_grid`, `develop`, `hatch`, `hlr`, `limits`, `mesh_reduce`, `profile` (all are leaves or sit above the component).
Without `error` and `kernel` the component is 17 areas (the same list minus those two): the cycle does not come from
the umbrella types; `geom`, `nurbs`, `math` and `curve` are inside it, i.e. the "bottom" libraries today depend on the top.

**Second level, the eight areas to be split expanded** (every other area one node): one component of **183** modules out of
236 nodes, holding most second-level modules of `geom` (34 of 38), `surface` (28 of 39), `blend` (25 of 28), `curve` (25 of 32), `topology` (23 of 24), `query` (14 of 26),
`eval` (12 of 22), `intersect` (11 of 13), plus one node each for `error`, `kernel`, `math`, `heal`, `offset`, `primitive`, `sweep`, `boolean`, `mass`, `prism`, `nurbs`. Removing the `(root)`
facade nodes changes it only to 180. With all areas expanded: one component of **245** of 386 second-level
modules, plus seven small ones that are facade artefacts inside one area (`cell_grid`, `develop`, `hatch`, `hlr`, `mesh_reduce`: root
file and children refer to each other; `curve::arc` with `curve::path`) and one real two-area one
(`curve::{conic,fit,spline}_projection` with `prism::{measure,read}`).
Modules outside the large component (all areas, 141 nodes, seven of them in the small components above) are mostly leaves: most of `eval`, `nurbs::{funcs_adv, gordon,
srf_match, ...}`, `query::{contact, poly_properties, tess, ...}`, `surface::{face_*, from_parameters, recognise, ...}`, `geom::{step_matrix,
facing_trough, plane_height, axial_section}`. List: `49-data/sccs.txt`.

**Library level (using the mapping of section 6).** The libraries of the target, with the references measured between them
(excluding `error` and the umbrella pieces), form one component of 20 of the 24
libraries other than the umbrella today. Removing every reference that points upward (section 6) leaves two residual cycles, both between libraries the
architecture treats as peers:
- `curve` <-> `surface`: `surface` -> `curve` 314 references, `curve` -> `surface` 48. Cut the 48 and `curve` goes first.
- the operation crates `blend, boolean, defeature, heal, hlr, offset, primitive, sweep`: 8 crates, 38 references pointing at a crate
  extracted later in the best order (`boolean, heal, hlr, sweep, primitive, blend, defeature, offset`; found by trying all 8! orders).
  The largest couplings: `offset` -> `blend` 36 (reverse 4), `primitive` -> `sweep` 36 (reverse 12), `blend` -> `boolean` 29,
  `primitive` -> `boolean` 23, `offset` -> `heal` 18, `sweep` -> `boolean` 18, `offset` -> `boolean` 15. The architecture says these crates "depend on
  0 to 6, not on each other, except where stated"; today `blend`, `offset` and `sweep` each refer to six of the other seven, `primitive` to four.


## 5. Cuts at area level (MEASURED counts; the order of areas is JUDGED)

A cut is a reference from an area that the target layering puts **below** another to that other area. Ranks used (JUDGED, one per area;
areas that mix libraries get the rank of the group they belong to, see section 6 for the file-level answer):

`math`, `limits`, `geom`, `profile` = 0; `nurbs` = 2; `eval` = 3; `curve`, `surface` = 5 (peers); `intersect` = 6;
`topology`, `query`, `mass` = 7 (peers: they are split between `geomq`, `brep` and `topo`); the operation areas
(`blend`, `boolean`, `offset`, `sweep`, `primitive`, `heal`, `prism`, `hlr`, `develop`, `cell_grid`, `hatch`, `mesh_reduce`) = 9.
`error` and `kernel` are cross-cutting and treated apart (5.3). A reference from a lower to a higher rank is a cut; between peers both
directions are in the table of 5.1b.

**Warning on reading the area-level table.** `intersect`, `query`, `topology` and `geom` each contain code the target puts on both sides of
a rank. Their cuts include references that disappear simply by moving the right file to the right library (section 6): for example
`intersect::{curve_class, point_class}` are 227 of the 273 references of `intersect` -> `topology`, and they belong with `topo`.
The library-level list in section 6 is the authoritative cut list; this one shows where the cycle runs.

### 5.1 Pairs ranked by number of cuts

| lower -> upper | references (cuts) | files | distinct items | name-use sites | refs the other way |
|---|---|---|---|---|---|
| intersect -> topology | 273 | 18 | 111 | 155 | 147 |
| surface -> blend | 100 | 22 | 64 | 537 | 436 |
| surface -> topology | 78 | 22 | 20 | 163 | 241 |
| geom -> topology | 75 | 6 | 42 | 148 | 1039 |
| topology -> heal | 63 | 17 | 23 | 127 | 540 |
| query -> blend | 57 | 9 | 24 | 1246 | 107 |
| nurbs -> surface | 56 | 10 | 25 | 91 | 444 |
| nurbs -> curve | 52 | 9 | 37 | 96 | 166 |
| intersect -> blend | 51 | 15 | 20 | 534 | 226 |
| curve -> topology | 45 | 15 | 6 | 128 | 178 |
| intersect -> query | 35 | 8 | 20 | 29 | 54 |
| query -> heal | 28 | 3 | 20 | 153 | 17 |
| surface -> intersect | 27 | 8 | 18 | 46 | 197 |
| mass -> blend | 21 | 3 | 11 | 532 | 5 |
| curve -> prism | 15 | 3 | 10 | 50 | 8 |
| topology -> blend | 13 | 6 | 7 | 207 | 3306 |
| curve -> blend | 13 | 6 | 4 | 254 | 222 |
| geom -> query | 13 | 4 | 10 | 4 | 273 |
| geom -> nurbs | 12 | 8 | 7 | 42 | 198 |
| geom -> surface | 12 | 5 | 9 | 29 | 1082 |
| topology -> boolean | 12 | 3 | 12 | 24 | 579 |
| nurbs -> topology | 11 | 2 | 4 | 25 | 225 |
| nurbs -> blend | 10 | 2 | 9 | 144 | 382 |
| surface -> sweep | 9 | 3 | 4 | 12 | 33 |
| surface -> offset | 9 | 4 | 5 | 31 | 106 |
| nurbs -> query | 8 | 2 | 7 | 4 | 76 |
| geom -> blend | 7 | 2 | 6 | 65 | 918 |
| geom -> curve | 6 | 5 | 5 | 28 | 644 |


In all: 47 area pairs, **1,158 upward references** (excluding `error` and `kernel`). Every pair of the top 28 also has references in the other direction (last column), which is what closes the cycle. Per-site lists: `cuts_area_refs.tsv` (1,158 rows: file, line, item, kind, classification), `cuts_area_uses.tsv` (5,078 name-use sites), `cuts_area_items.tsv` (grouped by item with the dominant uses).

### 5.1b What the cut sites are, and the peer pairs

Name-use sites of the items behind the 1,158 upward references, by what the use is (MEASURED, heuristic context):

| use | sites |
|---|---|
| type associated fn/const/variant (Name::x) | 2,167 |
| type in fn parameter | 1,012 |
| function call | 546 |
| type in fn return type | 409 |
| type in a type annotation in body | 270 |
| type in struct/enum field | 186 |
| type struct literal/pattern | 160 |
| type value/pattern in body | 122 |
| constant | 63 |
| trait in fn generics/where | 54 |
| type in alias/const type | 45 |
| impl header (trait or inherent target) | 17 |


Peer pairs (same rank, both directions; these are not cuts but must be ordered). Top of `area_peer_refs.tsv`:

| pair | references | reverse |
|---|---|---|
| query -> topology | 397 | 210 |
| surface -> curve | 195 | 44 |
| geom -> math | 109 | 10 |
| mass -> topology | 63 | 16 |
| offset -> blend | 62 | 4 |
| primitive -> sweep | 36 | 12 |
| blend -> boolean | 25 | 0 |
| primitive -> boolean | 23 | 5 |
| develop -> prism | 22 | 0 |
| sweep -> boolean | 18 | 1 |
| offset -> heal | 14 | 0 |
| mass -> query | 12 | 4 |


### 5.2 The ten largest pairs, grouped by referenced item (references / name-use sites)

**intersect -> topology** (273 references in 18 files, 111 distinct items)

- `Brep` (struct, `topology`): 18 references, 64 name-use sites; mainly: type in fn parameter: 28; type in fn return type: 18; type in struct/enum field: 11
- `FaceId` (struct, `topology`): 13 references, 25 name-use sites; mainly: type in fn parameter: 17; type in fn return type: 5; type in a type annotation in body: 2
- `EdgeId` (struct, `topology`): 10 references, 10 name-use sites; mainly: type in fn return type: 4; type in struct/enum field: 3; type in fn parameter: 2
- `surface` (fn, `topology::face::p2`): 14 references, 0 name-use sites; mainly: function call
- `curve` (fn, `topology::edge`): 14 references, 0 name-use sites; mainly: function call

**surface -> blend** (100 references in 22 files, 64 distinct items)

- `Ivl` (struct, `blend::fillet_section::interval`): 5 references, 215 name-use sites; mainly: type associated fn/const/variant (Name::x): 135; type in fn parameter: 29; type in fn return type: 16
- `V3` (struct, `blend::fillet_section::series`): 1 references, 42 name-use sites; mainly: type in struct/enum field: 13; type struct literal/pattern: 9; type in fn parameter: 8
- `v_scale` (fn, `blend::fillet_section::series`): 1 references, 32 name-use sites; mainly: function call: 32
- `v_sub` (fn, `blend::fillet_section::series`): 1 references, 25 name-use sites; mainly: function call: 25
- `as_b_spline` (fn, `blend::fillet_section::station`): 16 references, 9 name-use sites; mainly: function call: 9

**surface -> topology** (78 references in 22 files, 20 distinct items)

- `DatabaseReader` (struct, `topology::database_io`): 17 references, 54 name-use sites; mainly: type in fn parameter: 32; type associated fn/const/variant (Name::x): 13; type in a type annotation in body: 6
- `FileType` (enum, `topology::database_io`): 16 references, 42 name-use sites; mainly: type associated fn/const/variant (Name::x): 38; type in a type annotation in body: 3; type in alias/const type: 1
- `InputStream` (struct, `topology::database_io`): 13 references, 18 name-use sites; mainly: type associated fn/const/variant (Name::x): 14; type in alias/const type: 2; type in fn parameter: 2
- `DatabaseWriter` (struct, `topology::database_io`): 9 references, 13 name-use sites; mainly: type in fn parameter: 11; type associated fn/const/variant (Name::x): 2
- `FaceId` (struct, `topology`): 3 references, 14 name-use sites; mainly: type in fn parameter: 8; type in struct/enum field: 2; type value/pattern in body: 2

**geom -> topology** (75 references in 6 files, 42 distinct items)

- `Brep` (struct, `topology`): 3 references, 54 name-use sites; mainly: type in fn parameter: 40; type associated fn/const/variant (Name::x): 9; type in fn return type: 3
- `BrepId` (struct, `topology`): 3 references, 20 name-use sites; mainly: type in fn parameter: 8; type associated fn/const/variant (Name::x): 7; type in alias/const type: 3
- `EdgeId` (struct, `topology`): 3 references, 18 name-use sites; mainly: type in fn parameter: 12; type in fn return type: 2; type in struct/enum field: 2
- `VertexId` (struct, `topology`): 3 references, 13 name-use sites; mainly: type in struct/enum field: 3; type in fn parameter: 3; type in a type annotation in body: 2
- `FaceId` (struct, `topology`): 3 references, 11 name-use sites; mainly: type in fn parameter: 4; type associated fn/const/variant (Name::x): 2; type in fn return type: 2

**topology -> heal** (63 references in 17 files, 23 distinct items)

- `LoopObject` (trait, `heal::face_props`): 4 references, 37 name-use sites; mainly: trait in fn generics/where: 25; impl header (trait or inherent target): 9; trait value/pattern in body: 2
- `VertexEdgeGap` (struct, `heal::gap`): 5 references, 13 name-use sites; mainly: type associated fn/const/variant (Name::x): 7; type in struct/enum field: 3; type in fn return type: 3
- `FaceProps` (struct, `heal::face_props`): 3 references, 15 name-use sites; mainly: type in fn parameter: 9; type associated fn/const/variant (Name::x): 3; type in a type annotation in body: 2
- `Tri` (enum, `heal::face_props`): 7 references, 8 name-use sites; mainly: type associated fn/const/variant (Name::x): 7; type in a type annotation in body: 1
- `EdgeEdgeGap` (struct, `heal::gap`): 5 references, 10 name-use sites; mainly: type associated fn/const/variant (Name::x): 7; type in fn return type: 2; type in struct/enum field: 1

**query -> blend** (57 references in 9 files, 24 distinct items)

- `Ivl` (struct, `blend::fillet_section::interval`): 8 references, 866 name-use sites; mainly: type associated fn/const/variant (Name::x): 532; type in fn parameter: 146; type in fn return type: 81
- `NurbsCurve` (struct, `blend::fillet_section::certificate`): 4 references, 99 name-use sites; mainly: type in fn parameter: 40; type associated fn/const/variant (Name::x): 21; type in a type annotation in body: 13
- `V3` (struct, `blend::fillet_section::series`): 3 references, 95 name-use sites; mainly: type in fn parameter: 40; type struct literal/pattern: 30; type in fn return type: 14
- `round_up` (fn, `blend::fillet_section::interval`): 6 references, 41 name-use sites; mainly: function call: 41
- `knot` (fn, `blend::fillet_section::station`): 2 references, 27 name-use sites; mainly: function call: 27

**nurbs -> surface** (56 references in 10 files, 25 distinct items)

- `Surface` (struct, `surface::surface::p2`): 23 references, 76 name-use sites; mainly: type associated fn/const/variant (Name::x): 34; type in fn parameter: 28; type in struct/enum field: 4
- `SurfaceSolveContext` (trait, `surface::surface::p1`): 3 references, 3 name-use sites; mainly: trait in fn parameter: 3
- `SurfaceKind` (enum, `surface::surface::p2`): 2 references, 2 name-use sites; mainly: type associated fn/const/variant (Name::x): 2
- `NoObject` (struct, `surface::surface::p4`): 4 references, 0 name-use sites; mainly: path use
- `SurfaceEvaluate` (trait, `surface::surface::p2`): 1 references, 3 name-use sites; mainly: trait in fn generics/where: 3

**nurbs -> curve** (52 references in 9 files, 37 distinct items)

- `Curve` (struct, `curve::curve::p2`): 8 references, 40 name-use sites; mainly: type associated fn/const/variant (Name::x): 17; type in fn parameter: 10; type in a type annotation in body: 5
- `CurveKind` (enum, `curve::curve::p2`): 5 references, 18 name-use sites; mainly: type associated fn/const/variant (Name::x): 18
- `TangentField` (struct, `curve::tangent_field`): 1 references, 4 name-use sites; mainly: type associated fn/const/variant (Name::x): 2; type in struct/enum field: 1; type in fn parameter: 1
- `Ellipse` (struct, `curve::ellipse::p2`): 1 references, 3 name-use sites; mainly: type associated fn/const/variant (Name::x): 2; type in struct/enum field: 1
- `Line` (struct, `curve::line`): 1 references, 3 name-use sites; mainly: type in struct/enum field: 2; type associated fn/const/variant (Name::x): 1

**intersect -> blend** (51 references in 15 files, 20 distinct items)

- `Ivl` (struct, `blend::fillet_section::interval`): 10 references, 350 name-use sites; mainly: type associated fn/const/variant (Name::x): 225; type in fn parameter: 64; type in fn return type: 40
- `V3` (struct, `blend::fillet_section::series`): 6 references, 61 name-use sites; mainly: type in fn parameter: 16; type struct literal/pattern: 16; type associated fn/const/variant (Name::x): 12
- `Nurbs` (struct, `blend::fillet_section::station`): 7 references, 42 name-use sites; mainly: type associated fn/const/variant (Name::x): 15; type struct literal/pattern: 11; type in fn parameter: 7
- `round_up` (fn, `blend::fillet_section::interval`): 3 references, 26 name-use sites; mainly: function call: 26
- `FilletLaw` (struct, `blend::fillet_standard_solver::p1`): 4 references, 10 name-use sites; mainly: type in fn parameter: 8; type associated fn/const/variant (Name::x): 2

**curve -> topology** (45 references in 15 files, 6 distinct items)

- `FileType` (enum, `topology::database_io`): 13 references, 46 name-use sites; mainly: type associated fn/const/variant (Name::x): 45; type in a type annotation in body: 1
- `DatabaseReader` (struct, `topology::database_io`): 14 references, 42 name-use sites; mainly: type in fn parameter: 26; type associated fn/const/variant (Name::x): 14; type struct literal/pattern: 1
- `InputStream` (struct, `topology::database_io`): 8 references, 27 name-use sites; mainly: type associated fn/const/variant (Name::x): 25; type in fn parameter: 2
- `DatabaseWriter` (struct, `topology::database_io`): 6 references, 10 name-use sites; mainly: type in fn parameter: 8; type associated fn/const/variant (Name::x): 1; type in a type annotation in body: 1
- `write_curve_at` (fn, `topology::geometry_write`): 3 references, 2 name-use sites; mainly: function call: 2

What the groups say (JUDGED, from the lists):
- `intersect` -> `topology` (273): classification against a body (`Brep` in 18 imports and 64 uses, mostly parameters and returns; entity ids).
  Mostly the two classification modules, which belong in `topo`.
- `surface` -> `blend` (100) and `query`/`intersect`/`mass`/`curve` -> `blend` (57/51/21/13): the rounded interval arithmetic
  `Ivl`, vectors `V3`, series and Taylor models in `blend/fillet_section/{interval,series,tay,taylor_model}.rs` (2,049 lines), and
  the certified NURBS enclosures in `blend/fillet_section/{certificate,station}.rs`. Lifting the first to `num` and the second to `nurbs`
  removes most of the 270 references and 3,500 name-use sites that point at `blend` (from `surface`, `query`, `intersect`, `mass`, `curve`, `nurbs`, `geom`, `topology`).
  For `query`, `intersect`, `mass` and `geomq` they are almost all interval and certified-NURBS machinery. For `surface` they are `surface::surface::exact_pieces` (20),
  `procedural_blend/*` (35 or more) and `extend/blend` (13): the blend definition evaluators (`fillet_section::definition`) that a procedural blend stores.
- `surface`/`curve` -> `topology` (78/45): not the body. The groups are the database reader and writer (`DatabaseReader`, `DatabaseWriter`, `FileType`,
  `InputStream`: 55 of the 78 `surface` references) and `geometry_write`; the virtual read/write-to-file members of each geometry kind. Only 5 `surface`
  files and 0 `curve` files name an entity or body type (`Brep`, `FaceId`, ...) directly. (The architecture document counts 26 surface and 19
  curve files that "import the body"; this count is of direct references, and does not include the indirection through `geom::object`
  and `geom::assert_array`, which carry the owner and the check state.)
- `geom` -> `topology` (75 references in 6 files): `tol.rs` 20, `tools.rs` 20, `object.rs` 13, `attribute.rs` 12, `a_object.rs` 7, `transform.rs` 3.
  These six files are the part of `geom/` that is not vector maths (section 8).
- `topology` -> `heal` (63) and `query` -> `heal` (28): the healer's property and gap records (`heal::face_props`, `heal::gap`) are used by body code; they
  sit with `topo` in the mapping.
- `nurbs` -> `surface`/`curve` (56/52): 103 of the 108 are inside `nurbs/b_spline_{curve,surface}*` (wrappers that make a B-spline a kernel curve or surface),
  which the mapping puts in `curve`/`surface`; the other 5 are in `gordon`, `certificate`, `crv_proportion`. The raw NLib part of `nurbs` has 5 upward references in all (section 6).


### 5.3 `error` and `kernel`: referenced from everywhere (MEASURED)

| from area | refs to `error` | files | refs to `kernel` | files |
|---|---|---|---|---|
| blend | 838 | 148 | 144 | 127 |
| topology | 498 | 77 | 74 | 65 |
| surface | 352 | 81 | 74 | 73 |
| intersect | 211 | 40 | 40 | 40 |
| nurbs | 202 | 67 | 51 | 16 |
| boolean | 184 | 17 | 18 | 15 |
| curve | 166 | 45 | 45 | 40 |
| query | 130 | 30 | 29 | 27 |
| offset | 115 | 25 | 23 | 23 |
| primitive | 108 | 26 | 26 | 21 |
| sweep | 87 | 24 | 26 | 20 |
| geom | 83 | 29 | 11 | 7 |
| heal | 48 | 9 | 8 | 8 |
| mass | 17 | 4 | 4 | 4 |
| math | 17 | 1 | 0 | 0 |
| prism | 13 | 6 | 5 | 5 |
| eval | 6 | 3 | 0 | 0 |
| mesh_reduce | 4 | 3 | 0 | 0 |
| kernel | 3 | 1 | 0 | 0 |


By item: `Result` 758 references (635 files), `SolidError` 575 (559), `Kernel` 548 (486), `EntityRef` 258 (209), `Failure` 198 (182),
`ignored` 188, `ignore_status` 119, the `SM_*` status codes about 275, `math::EFF_ZERO` 282 (`math` is inside the future `num`).
The `error` area itself refers upward: `error::blend` to `FaceId`/`EdgeId`/`VertexId` (36 references with `error::blend::solver`), `error.rs` to blend and offset types
(`FilletObject`, `FitKind`, `OffsetExecutive`) and to every id type. `kernel.rs` refers to `blend`, `surface`, `mass`, `nurbs`, `topology`,
`geom::Context` and `profile` because the `Kernel` struct holds their memos as fields (`StationMemo`, `ThumbweightMemo`,
`BlendFaceLog`, `BlendCounts`, `ReplaySeam`, `RecertifyMemo`, `LoopAreaMemo`, `AnalysisMemo`, `CoonsTangencyLog`, four `NotifyRecords`,
a `Profiler`). Cutting these two is the same work as "settings passed in, scratch passed in" and "one error enum per crate"; it is
counted separately in every table below because it touches every file and decides nothing about layering.


## 6. Mapping modules to target libraries (JUDGED) and what each library has today (MEASURED given the mapping)

The mapping is in `layers.py` (longest-prefix rules on file paths, each with a confidence: H doc comment and references agree, M plausible,
L uncertain), and per file in `file_map.tsv`, per second-level module in `module_map.tsv`. Libraries added to the architecture's list
because the measurements ask for them: `evalrec` (evaluators on plain records, `eval/`: depends only on `num` and `nurbs`, used by
`curve`, `surface`, `hlr`, `develop`, `prism`) and `planar` (the planar-profile core of `prism/`, used by `curve::*_projection`, `cell_grid`, `hatch`,
`develop`). `record_convert` is `record` plus `convert`; `umbrella` is `SolidError`/`Result`/`EntityRef`/status codes (`error.rs`, `error/*` root
items), the `Kernel` object and `lib.rs`. Operation crates are one library each; `defeature` is `blend/feature_executive` plus its error type;
`hlr` also takes `blend/hcr` and `topology/api_projection`.

| library | rank | files | non-test lines | non-test fns | upward refs today | of which not to `umbrella` | files holding them | name-use sites of upward items | refs to peers |
|---|---|---|---|---|---|---|---|---|---|
| num | 0 | 33 | 20,840 | 1,017 | 120 | 54 | 22 | 578 | 0 |
| profile | 0 | 1 | 560 | 19 | 0 | 0 | 0 | 0 | 0 |
| algo | 1 | 9 | 10,562 | 410 | 31 | 10 | 8 | 397 | 0 |
| nurbs | 2 | 55 | 46,200 | 1,143 | 113 | 5 | 50 | 1,479 | 0 |
| evalrec | 3 | 26 | 13,340 | 401 | 10 | 4 | 4 | 30 | 0 |
| planar | 4 | 11 | 5,520 | 243 | 23 | 10 | 5 | 50 | 0 |
| curve | 5 | 54 | 67,513 | 1,646 | 328 | 146 | 49 | 3,081 | 48 |
| surface | 5 | 99 | 140,091 | 3,554 | 767 | 349 | 87 | 6,974 | 314 |
| geomq | 6 | 38 | 43,133 | 957 | 251 | 83 | 34 | 2,123 | 0 |
| brep | 7 | 32 | 65,694 | 2,140 | 739 | 463 | 30 | 5,281 | 0 |
| topo | 8 | 88 | 104,240 | 3,040 | 460 | 60 | 80 | 5,963 | 0 |
| blend | 9 | 173 | 187,928 | 4,827 | 737 | 38 | 154 | 10,319 | 53 |
| boolean | 9 | 32 | 43,224 | 1,369 | 238 | 10 | 27 | 2,271 | 6 |
| cell_grid | 9 | 7 | 4,095 | 140 | 0 | 0 | 0 | 0 | 0 |
| defeature | 9 | 3 | 3,697 | 109 | 18 | 2 | 3 | 226 | 8 |
| develop | 9 | 12 | 6,950 | 221 | 0 | 0 | 0 | 0 | 0 |
| hatch | 9 | 7 | 2,174 | 100 | 0 | 0 | 0 | 0 | 0 |
| heal | 9 | 11 | 13,937 | 422 | 45 | 2 | 8 | 734 | 10 |
| hlr | 9 | 10 | 10,177 | 287 | 17 | 2 | 2 | 185 | 2 |
| mesh_reduce | 9 | 4 | 1,974 | 57 | 4 | 0 | 3 | 22 | 0 |
| offset | 9 | 32 | 41,254 | 1,094 | 166 | 19 | 29 | 2,522 | 83 |
| primitive | 9 | 31 | 16,610 | 434 | 136 | 17 | 27 | 1,061 | 69 |
| prism | 9 | 4 | 2,409 | 61 | 5 | 0 | 3 | 26 | 0 |
| sweep | 9 | 28 | 19,862 | 660 | 113 | 6 | 26 | 1,148 | 47 |
| record_convert | 10 | 20 | 18,690 | 598 | 75 | 0 | 19 | 1,075 | 0 |
| umbrella | 11 | 3 | 3,397 | 192 | 0 | 0 | 0 | 0 | 0 |


"Upward refs today" must be 0 to extract a library. The column "of which not to `umbrella`" removes references to `SolidError`, `Result`, `EntityRef`,
`Failure`, the status codes and `Kernel` (but `Tolerances`, `Rng`, `CancelToken`, `KernelOptions` are counted as `num`). `peer` refs are between
libraries of equal rank (`curve`/`surface`; the operation crates). Per-site lists: `lib_upward_refs.tsv` (4,396 rows incl. the umbrella), `lib_upward_uses.tsv`, grouped in
`lib_cut_groups.tsv`; matrix `lib_matrix.tsv`.

Reading the table:
- `num` (20,840 lines): 54 upward references outside the umbrella (52 not counting `error`) in 22 files, plus 66 to the umbrella in 20 files. The 54: `geom/tol.rs` 30 (20 to `brep`, 5 to `topo`, 2 to `surface`),
  `geom/transform.rs` 12 (5 to `surface::volume`, 4 to `algo`, 3 to the database reader/writer), `math.rs` 3 and `vector3d.rs` 2 (`nurbs::math::gamma`, `NOZ`), `geom_utility` 2, `series.rs` 2 (inside `blend`), 3 singles.
- `algo`: 10 (surface/curve cache types used by the global solver, one `Side`).
- `nurbs`: **5** (`geom::a_object::reverse_knots`, `SurfParamType`, `ExtendRefusal`): the architecture's claim that `nurbs` has almost no upward
  references holds once the B-spline curve/surface wrappers are counted as `curve`/`surface`. Without that reassignment `nurbs` -> `curve`/`surface`
  is 108 references in at most 19 files.
- `evalrec`: 4 (all `prism::profile`: `Seg2`, `Loop2`, two constants). `planar`: 10 (7 to `curve` types: curve-reading helpers).
- `curve`: 146 (not umbrella): `geom::object::ObjectRef` 9 refs / 59 uses, `geom::assert_array::{ObjectKey,AssertArray}` 10 / 83,
  the database reader/writer 41 / 123, `prism::read::PlanarPiece` 2 / 18.
- `surface`: 349: database reader/writer 66 / 154, `ObjectRef` 15 / 43, `ObjectKey`/`AssertArray` 8 / 72, plus the
  `blend` references of section 5.2 (82) and `geomq` 24, `sweep` 17, `offset` 9 (`Bernstein2` in `offset::construction`).
- `geomq`: 83: 44 to `blend` (certified NURBS enclosures), 22 to `brep`, 14 to `topo`.
- `brep` / `topo`: the split is the least certain. Counted separately `brep` has 463 and `topo` 60, but 416 of `brep`'s refer to `topo`.
  Counted as one library (`brep+topo`): 107 upward references outside the umbrella (`blend` 43, `record_convert` 23, `heal` 21, `boolean` 14,
  `offset` 2, `primitive` 3, `hlr` 1), 783 including it. I would plan `brep+topo` as one extraction first (JUDGED); see section 8.
- The operation crates, `record_convert`: 0 to 38 upward references each outside the umbrella (e.g. `blend` -> `record_convert` 38 (`from_parameters`
  style constructors), `offset` -> `record_convert` 19, `primitive` -> `record_convert` 17): they are leaves already, which is the
  expected shape.

**Composition of each area by library** (lines; from `file_map.tsv`):

| area | non-test lines | goes to (library lines) |
|---|---|---|
| blend | 188,856 | blend 181,198; defeature 3,520; hlr 2,089; num 2,049 |
| surface | 126,849 | surface 122,629; record_convert 3,796; offset 424 |
| topology | 116,488 | brep 58,326; topo 27,282; boolean 15,443; record_convert 12,862; offset 2,012; hlr 563 |
| nurbs | 70,613 | nurbs 45,213; surface 12,049; curve 11,070; sweep 1,559; record_convert 722 |
| intersect | 66,165 | geomq 33,734; topo 32,431 |
| curve | 59,366 | curve 55,186; surface 2,870; record_convert 1,310 |
| geom | 37,500 | num 16,917; algo 10,562; brep 5,447; topo 4,574 |
| offset | 36,876 | offset 36,876 |
| query | 36,534 | topo 26,883; geomq 8,493; surface 1,158 |
| boolean | 27,028 | boolean 27,028 |
| error | 20,790 | blend 6,730; umbrella 2,105; offset 1,942; brep 1,921; surface 1,385; curve 1,257; nurbs 987; geomq 906; num 892; boolean 753; primitive 652; sweep 628; heal 261; topo 194; defeature 177 |
| heal | 19,213 | heal 13,676; topo 5,537 |
| sweep | 17,675 | sweep 17,675 |
| primitive | 15,958 | primitive 15,958 |
| eval | 13,340 | evalrec 13,340 |
| prism | 7,929 | planar 5,520; prism 2,409 |
| hlr | 7,525 | hlr 7,525 |
| mass | 7,339 | topo 7,339 |
| develop | 6,950 | develop 6,950 |
| cell_grid | 4,095 | cell_grid 4,095 |
| hatch | 2,174 | hatch 2,174 |
| mesh_reduce | 1,974 | mesh_reduce 1,974 |
| kernel | 1,200 | umbrella 1,200 |
| math | 903 | num 903 |
| profile | 560 | profile 560 |
| lib | 92 | umbrella 92 |
| limits | 79 | num 79 |


Uncertain mappings, by library: lines at confidence L / M / H.

| library | L | M | H |
|---|---|---|---|
| num | 1,069 | 9,198 | 10,573 |
| profile | 0 | 0 | 560 |
| algo | 0 | 0 | 10,562 |
| nurbs | 0 | 0 | 46,200 |
| evalrec | 0 | 13,340 | 0 |
| planar | 0 | 5,520 | 0 |
| curve | 0 | 0 | 67,513 |
| surface | 0 | 5,107 | 134,984 |
| geomq | 0 | 43,133 | 0 |
| brep | 0 | 65,694 | 0 |
| topo | 4,574 | 99,666 | 0 |
| blend | 0 | 0 | 187,928 |
| boolean | 0 | 43,224 | 0 |
| cell_grid | 0 | 0 | 4,095 |
| defeature | 0 | 3,697 | 0 |
| develop | 0 | 0 | 6,950 |
| hatch | 0 | 0 | 2,174 |
| heal | 0 | 0 | 13,937 |
| hlr | 0 | 10,177 | 0 |
| mesh_reduce | 0 | 0 | 1,974 |
| offset | 0 | 424 | 40,830 |
| primitive | 0 | 0 | 16,610 |
| prism | 0 | 2,409 | 0 |
| sweep | 0 | 1,559 | 18,303 |
| record_convert | 0 | 18,690 | 0 |
| umbrella | 1,200 | 2,105 | 92 |


The `L` lines: `geom/{content, axial_section, facing_trough}` (unclear home), `geom/{tools, assert_array}` (to `topo`), `error/query`,
`kernel.rs` (must be split), plus the `M` bulk in `topology/` where each file is marked by its doc comment, not by its functions.


## 7. Proposed extraction order (JUDGED order; cut counts MEASURED)

Rule: a library is extracted when everything it uses is already extracted. "Cuts" = references from the library to libraries not yet extracted at that
step, so the step cannot compile on its own until they are removed (moved, inverted behind a trait, or the item moved down). The umbrella column is the
`SolidError` / `Result` / `Kernel` / `EntityRef` / status-code references that every step also carries; it is the same cut each time
(a per-crate error and "settings passed in") and is not repeated in the detail.

| step | library | cuts | umbrella refs | where the cuts go (target library: refs) |
|---|---|---|---|---|
| 1 | profile | 0 | 0 |  |
| 2 | num | 54 | 66 | brep:20; nurbs:8; surface:8; algo:7; topo:5; record_convert:3; blend:2; curve:1 |
| 3 | algo | 10 | 21 | curve:3; surface:3; brep:2; nurbs:1; blend:1 |
| 4 | nurbs | 5 | 108 | brep:3; surface:2 |
| 5 | evalrec | 4 | 6 | planar:4 |
| 6 | planar | 10 | 13 | curve:7; sweep:1; brep:1; topo:1 |
| 7 | curve | 194 | 182 | record_convert:52; surface:48; topo:46; brep:30; prism:9; blend:9 |
| 8 | surface | 349 | 418 | topo:86; blend:82; record_convert:75; brep:56; geomq:24; sweep:17; offset:9 |
| 9 | geomq | 83 | 168 | blend:44; brep:22; topo:14; record_convert:1; sweep:1; offset:1 |
| 10 | brep | 463 | 276 | topo:416; heal:19; record_convert:15; boolean:10; blend:3 |
| 11 | topo | 60 | 400 | blend:40; record_convert:8; boolean:4; primitive:3; heal:2; offset:2; hlr:1 |
| 12 | prism | 0 | 5 |  |
| 13 | develop | 0 | 0 |  |
| 14 | hatch | 0 | 0 |  |
| 15 | cell_grid | 0 | 0 |  |
| 16 | mesh_reduce | 0 | 4 |  |
| 17 | boolean | 16 | 228 | record_convert:10; primitive:5; sweep:1 |
| 18 | heal | 9 | 43 | blend:7; record_convert:2 |
| 19 | hlr | 4 | 15 | record_convert:2; blend:2 |
| 20 | sweep | 23 | 107 | primitive:12; record_convert:6; offset:3; blend:2 |
| 21 | primitive | 17 | 119 | record_convert:17 |
| 22 | blend | 44 | 699 | record_convert:38; offset:4; defeature:2 |
| 23 | defeature | 2 | 16 | record_convert:2 |
| 24 | offset | 19 | 147 | record_convert:19 |
| 25 | record_convert | 0 | 75 |  |
| 26 | umbrella | 0 | 0 |  |


Notes (JUDGED):
1. **Step 0, before any step: the umbrella.** Without a per-crate error type and without `Kernel` the first library cannot be extracted: 120 references
   from `num` alone. Do it per crate as that crate is extracted (architecture step J), starting from `num`; for `num` the 66 umbrella references
   are in 20 files.
2. `num` (step 2) is not "almost free": 54 cuts, of which 30 are `geom/tol.rs` entity-tolerance helpers (move to `brep`/`topo`), 8 `nurbs::math::gamma` and `NOZ`
   (move down to `num`), 12 in `geom/transform.rs` (the volume and database members). After those three, about 4 remain.
   Both of the first two are the file splits of section 8.1.
3. `curve` before `surface`: `curve` has 48 references to `surface`, `surface` has 314 to `curve`. The `curve` cuts are mainly `record_convert` (52: database read/write
   members, `from_parameters`), `topo` (46: `ObjectKey`/`AssertArray`, tolerance helpers), `brep` (30: `ObjectRef`, tolerances). The largest single cut for `curve`
   and `surface` together is the group `FileType`/`DatabaseReader`/`DatabaseWriter`/`InputStream` (107 references, 277 name-use sites): the virtual read/write
   members move to `record` as functions over the closed set of kinds.
4. `brep` and `topo`: plan as one step (cuts 107 outside the umbrella as one library). The 416 `brep` -> `topo` references are an artefact of my file-level split, not a layering fact.
5. The five leaf crates (`prism`, `develop`, `hatch`, `cell_grid`, `mesh_reduce`) need 0 cuts once `num`, `nurbs`, `evalrec`, `planar` exist (steps 5, 6): they can be
   extracted at any time after step 6, as practice for the mechanics. `mesh_reduce` has no dependency except `error`.
6. The operation crates: `boolean` (16), `heal` (9), `hlr` (4), `sweep` (23: `primitive` 12), `primitive` (17), `blend` (44), `defeature` (2), `offset` (19). `blend` last but one
   because it uses `boolean`, `heal` and `offset` (4); all remaining cuts are `record_convert` constructors, which are the same family as step-3 item 3.
7. Per step, the per-site lists are in `lib_upward_refs.tsv` filtered by `from_lib`; for a peer or later target the same file lists them (it contains every
   reference that points to a higher library; peers are in `lib_peer_refs.tsv`, summary only).
8. The greedy alternative (always take the library with the fewest outstanding cuts) is in `extraction_order_greedy.tsv`; it starts with the leaves.


## 8. Items that do not fit the layering (JUDGED, with measured evidence)

### 8.1 `geom/` (48 k lines, 40 files, 38 modules): maths, solvers and body-side types in one directory

| file | lines | fns | library | conf | upward refs | umbrella refs |
|---|---|---|---|---|---|---|
| `geom.rs` | 45 | 0 | num | H | 0 | 0 |
| `geom/a_object.rs` | 1,190 | 47 | brep | M | 0 | 2 |
| `geom/assert_array.rs` | 630 | 25 | topo | L | 0 | 2 |
| `geom/assert_array/labels.rs` | 3,207 | 3 | topo | L | 0 | 0 |
| `geom/attribute.rs` | 2,147 | 116 | brep | M | 6 | 2 |
| `geom/axial_section.rs` | 410 | 8 | num | L | 0 | 0 |
| `geom/axis2_placement.rs` | 533 | 31 | num | H | 0 | 1 |
| `geom/certified.rs` | 234 | 17 | num | H | 1 | 0 |
| `geom/content.rs` | 164 | 18 | num | L | 1 | 0 |
| `geom/context.rs` | 1,264 | 82 | brep | M | 0 | 3 |
| `geom/expansion.rs` | 134 | 12 | num | H | 0 | 0 |
| `geom/extent1d.rs` | 1,011 | 62 | num | H | 0 | 2 |
| `geom/extent2d.rs` | 898 | 67 | num | H | 0 | 2 |
| `geom/extent3d.rs` | 1,214 | 78 | num | H | 0 | 2 |
| `geom/facing_trough.rs` | 118 | 1 | num | L | 0 | 0 |
| `geom/geom_utility.rs` | 13 | 0 | num | M | 0 | 0 |
| `geom/geom_utility/p1.rs` | 1,973 | 54 | num | M | 2 | 2 |
| `geom/geom_utility/p2.rs` | 1,395 | 28 | num | M | 0 | 3 |
| `geom/global_solver.rs` | 2,315 | 70 | algo | H | 1 | 3 |
| `geom/grid.rs` | 1,292 | 53 | algo | H | 4 | 4 |
| `geom/hash_table.rs` | 921 | 43 | algo | H | 0 | 3 |
| `geom/interval_vector.rs` | 46 | 5 | num | H | 0 | 0 |
| `geom/krawczyk.rs` | 320 | 17 | algo | H | 1 | 1 |
| `geom/local_solve1d.rs` | 1,171 | 29 | algo | H | 0 | 3 |
| `geom/local_solve_nd.rs` | 798 | 25 | algo | H | 0 | 3 |
| `geom/matrix.rs` | 1,152 | 33 | num | M | 0 | 2 |
| `geom/object.rs` | 846 | 40 | brep | M | 8 | 5 |
| `geom/periodic_extent1d.rs` | 531 | 20 | num | H | 0 | 2 |
| `geom/plane_height.rs` | 108 | 5 | num | M | 0 | 0 |
| `geom/point_set.rs` | 803 | 23 | num | H | 0 | 1 |
| `geom/polar_box.rs` | 877 | 32 | num | H | 0 | 2 |
| `geom/polynomial.rs` | 271 | 9 | algo | H | 0 | 0 |
| `geom/pseudo_box.rs` | 1,239 | 56 | num | H | 0 | 3 |
| `geom/solution_array.rs` | 1,862 | 94 | algo | H | 1 | 2 |
| `geom/step_matrix.rs` | 144 | 5 | num | H | 0 | 0 |
| `geom/tol.rs` | 1,625 | 103 | num | M | 30 | 3 |
| `geom/tools.rs` | 737 | 21 | topo | L | 2 | 4 |
| `geom/transform.rs` | 804 | 43 | num | M | 12 | 2 |
| `geom/tree.rs` | 1,612 | 70 | algo | H | 3 | 2 |
| `geom/vector2d.rs` | 382 | 34 | num | H | 0 | 2 |
| `geom/vector3d.rs` | 1,064 | 71 | num | H | 2 | 2 |


How to divide (JUDGED, from the doc comment of each file and its measured references):
- **To `num`** (value types, no upward references of consequence): `vector2d`, `vector3d`, `extent1d/2d/3d`, `periodic_extent1d`, `pseudo_box`, `polar_box`, `point_set`,
  `axis2_placement`, `step_matrix`, `expansion`, `interval_vector`, `certified`, `matrix`, `geom_utility`, `plane_height`. With `math.rs` (which only re-exports `Vec3`/`Point3`
  from `geom::vector3d` and holds constants, `EFF_ZERO*`, `PI`, and `max2`/`min2`: 750 references name `math` for items defined in `geom`).
- **To `algo`**: `local_solve1d`, `local_solve_nd`, `global_solver`, `krawczyk`, `polynomial`, `solution_array`, `tree`, `grid`, `hash_table`. Two carry cuts to
  remove: `global_solver` names `CrvOnSurf`, `Kernel`, `CancelToken`, `UpstreamFixes` (cancellation moves to `num`; the fix switches are to be deleted); `krawczyk`
  and `interval_vector` currently use the `Ivl`/`Ring`/`TmN` of `blend::fillet_section`, which in the target sit below them.
- **`tol.rs` (1,625 lines) splits in two**: scalar comparison and constants (`are_same`, `is_zero`, `tol3d`, `in_tol`, `ZONE_TOL_3D`, ...; used by `curve` 17+17 files,
  `surface` 24+18 files, `nurbs`, `geomq`) belong to `num`; the entity-tolerance functions (`VertexGap`, `EdgeGap`, `set_tolerance` on an edge, face tolerance; 20 references
  to `topology`, 2 to `heal`) belong to `brep`. This is the largest single hidden coupling of the geometry libraries: `are_same`/`is_zero`/`tol3d` are the functions most called.
- **To `brep`**: `attribute`, `a_object`, `object`, `context`: attributes, change notification, run-time type codes and the per-session context (marks, locks, callbacks, Boolean flag).
  `curve` and `surface` hold an `ObjectRef` (24 references in 19 files, 102 uses): that is the geometry's link to the body's notification, to be removed (principle 1).
- **To `topo`**: `tools` (nearest vertex/edge selection tools, 20 refs to `topology`), `assert_array` (`AssertValid` bookkeeping: `ObjectKey` and `AssertArray` are used from `curve`
  and `surface` with 18 references, 155 name-use sites: check state needs to leave the geometry types).
- `transform.rs`: the matrix part is `num`; its volume-mapping and database read/write members (5 refs to `surface::volume`, 3 to `topology`) move out with the other virtual file members.
- Unclear (L): `content` (digest of a geometry value), `axial_section`, `facing_trough` (section helpers for revolution surfaces; likely `surface`).


### 8.2 `error/` (30 k lines in 38 files, including the test-only scanners; 3,519 lines in `error.rs`)

`error.rs` (2,105 non-test lines) is the umbrella `SolidError` with `EntityRef`, `Failure` and the `SM_*` status codes; it refers to blend, offset and every id type
(`error_outgoing_refs.tsv`: 56 rows). It stays at the top, as `ops`'s wrapper. The per-area files are the per-crate enums the architecture asks for, and map by name:
`error/geom` -> `num`, `error/nurbs` -> `nurbs`, `error/curve` -> `curve`, `error/surface` -> `surface`, `error/intersect` -> `geomq`, `error/topology` -> `brep`, `error/mass` -> `topo`,
`error/boolean`, `blend`, `offset`, `sweep`, `primitive`, `heal` -> the same-named crate, `error/defeature` -> `defeature`. `error/query` (377 lines) is used from `nurbs` (2 refs) upward,
so it must sit at `num` (L). The ids are the constraint: `error/blend` (6,730 lines) carries `FaceId`/`EdgeId`/`VertexId` (36 references with `error/blend/solver`), so an error enum that names an entity can only
live in a crate that can see `brep`; `error/surface` (1 ref to `topology`) and `error/offset` (5) are nearly clean. Four files (`bare_scan`, `phrase_sites`, `site_scan`, `stage_sites`)
are test-only source scanners (`#[cfg(test)] mod`) and are not in the counts; they scan this crate's own source and would move with the tests.


### 8.3 `kernel.rs` (1,200 non-test lines)

Eight items: `Tolerances` (l.47), `Rng` (96), `Caches` (136), `WorkCounters` (189), `UpstreamFixes` (751, to be deleted), `KernelOptions` (756), `CancelToken` (794), `Kernel`
(916). Divide by item (JUDGED): `Tolerances`, `KernelOptions`, `Rng`, `CancelToken` -> `num` (the `&Settings` and `&Cancel` of the calling convention; referenced from 8, 5, 2, 7 files);
`Caches`, `WorkCounters` and the memos currently fields of `Kernel` -> a `Scratch` that each crate defines its own part of (the struct today names types of `blend`, `surface`, `mass`,
`nurbs`, `topology`, `profile` and `geom::Context`: 20 references from `kernel.rs` to six higher areas, plus `geom::Context`); `Kernel` itself disappears. Measured use of the object: 548 references in
486 files across 15 areas, 10,350 name-use sites.


### 8.4 `query/` (52 k lines, 35 modules)

| file | lines | fns | library | conf | upward refs | umbrella refs |
|---|---|---|---|---|---|---|
| `query.rs` | 39 | 0 | topo | M | 0 | 0 |
| `query/api_queries.rs` | 2,359 | 82 | topo | M | 0 | 13 |
| `query/brep.rs` | 1,676 | 67 | topo | M | 1 | 7 |
| `query/brep/ray.rs` | 441 | 22 | topo | M | 0 | 4 |
| `query/brep/ray/live.rs` | 1,190 | 98 | topo | M | 0 | 4 |
| `query/contact.rs` | 728 | 22 | topo | M | 0 | 4 |
| `query/continuity.rs` | 821 | 26 | topo | M | 0 | 3 |
| `query/curve.rs` | 127 | 1 | geomq | M | 1 | 3 |
| `query/curve_surface_gap.rs` | 4,433 | 89 | geomq | M | 23 | 10 |
| `query/curve_surface_gap/polynomial_pieces.rs` | 962 | 24 | geomq | M | 5 | 2 |
| `query/deviation.rs` | 215 | 8 | geomq | M | 0 | 3 |
| `query/distance_bound.rs` | 626 | 16 | geomq | M | 4 | 3 |
| `query/edge.rs` | 1,457 | 26 | topo | M | 0 | 8 |
| `query/edge_turn.rs` | 249 | 8 | topo | M | 0 | 3 |
| `query/face.rs` | 22 | 0 | topo | M | 0 | 0 |
| `query/face/p1.rs` | 1,446 | 43 | topo | M | 0 | 6 |
| `query/face/p2.rs` | 2,739 | 148 | topo | M | 2 | 11 |
| `query/face/p3.rs` | 3,400 | 79 | topo | M | 0 | 7 |
| `query/face/p4.rs` | 1,043 | 25 | topo | M | 0 | 5 |
| `query/face_surface.rs` | 275 | 7 | topo | M | 0 | 3 |
| `query/faces_continue.rs` | 129 | 3 | topo | M | 0 | 2 |
| `query/flush_contact.rs` | 138 | 4 | topo | M | 0 | 2 |
| `query/line_meeting.rs` | 59 | 5 | geomq | M | 1 | 0 |
| `query/loop.rs` | 1,252 | 25 | topo | M | 0 | 4 |
| `query/member_turn.rs` | 58 | 1 | topo | M | 0 | 2 |
| `query/normal.rs` | 539 | 21 | geomq | M | 4 | 3 |
| `query/point_identity.rs` | 86 | 1 | geomq | M | 0 | 2 |
| `query/poly.rs` | 3,446 | 150 | topo | M | 0 | 6 |
| `query/poly/copy.rs` | 521 | 8 | topo | M | 0 | 3 |
| `query/poly_properties.rs` | 459 | 7 | topo | M | 0 | 3 |
| `query/seam_bound.rs` | 1,412 | 45 | geomq | M | 8 | 3 |
| `query/tangent_edge.rs` | 2,995 | 73 | topo | M | 7 | 3 |
| `query/tess.rs` | 1,158 | 42 | surface | M | 6 | 3 |
| `query/vertex_meet.rs` | 34 | 1 | geomq | M | 0 | 0 |


Divide by file (JUDGED): geometry-only queries go to `geomq` (`curve`, `deviation`, `distance_bound`, `normal`, `point_identity`, `line_meeting`, `vertex_meet`, `curve_surface_gap`, `seam_bound`);
`tess` is a trimmed-surface cache and goes to `surface`; everything that takes a face, a loop, an edge or a body goes to `topo` (`face` 8,650 lines, `brep`, `edge`, `loop`, `api_queries`,
`tangent_edge`, `contact`, `continuity`, `poly` ...). `query::curve_surface_gap` (5,395 lines, 24 refs to `blend`) and `query::normal` (6 refs) are `geomq`'s main cuts: they use the interval
and certified-NURBS machinery now in `blend`.


### 8.5 `topology/` (146 k lines), `intersect/`, `heal/`, `blend/`, `nurbs/`, `surface/`, `curve/`

- `topology/`: the 49,643-line `brep/` directory and the entity files are one body written as upstream's `SmBrep` members split across `p1..p10`; each `pN.rs` mixes data editing and
  algorithms by function, not by file. File assignment by doc comment: data and edit primitives -> `brep`; `validation/*`, `cutting`, `planar_section`, `sheet`, `side_extend`, `trim_extend`,
  `mirror`, trim creation (`face/p3`, `edgeuse/p3`), `trimming_tools`, `brep_cache`, `topology_traverser` -> `topo`; `proto_topology` (14,661 lines: the Boolean's gathered intersection
  results, 33 refs to `intersect`) -> `boolean`; `local_operation` -> `offset`; `api_projection` -> `hlr`; `database_io`, `part_file`, `api_files`, `brep_data*`, `geometry_write`, `brep/p7` (reading a
  database file) and `brep/p10` (building topology from serialized data) -> `record_convert`. Everything here except the last two groups is marked M.
- `intersect/`: `surface_*`, `adv_surface_intersector`, `ray_tracer`, `views` (about 34 k lines) -> `geomq`; `curve_class` (23,405 lines, 165 refs to `topology`), `point_class`, `topology_solver`,
  `api_intersectors` -> `topo`.
- `heal/`: `face_props` and `gap` (5.5 k lines) are used by `topology` (63 refs), `query` (28) and `intersect` (6), so they go to `topo`; `heal_data`, `stitch`, `tolerance_defects` stay in `heal`.
- `blend/`: `fillet_section/{interval,tay,series,taylor_model}.rs` (2,049 lines) -> `num`; `fillet_section/{certificate,station}.rs` (7,015 lines) hold certified-NURBS enclosures used from
  `geomq`, `topo`, `mass`, `intersect`, `query` (`NurbsCurve`, `Nurbs`, `curve_derivs_over`, `knot`, `span`, `Partials`: 44 refs from `geomq`, 40 from `topo`) and are only partly blend-specific
  (L: they need to be split by function); `hcr` -> `hlr`; `feature_executive` -> `defeature`.
- `nurbs/`: `b_spline_curve*`, `b_spline_surface*` (22 k lines, upward refs 108) are kernel geometry, not NLib: -> `curve`/`surface`. `gordon`, `n_sided` -> `surface`; `certificate` (railed loft)
  -> `sweep`; `iges` -> `record_convert`; the rest -> `nurbs` (46 k lines, 5 upward references).
- `surface/` and `curve/`: modules about a *face* rather than a surface (`face_evaluation`, `face_pair`, `face_pair_cylinder`, `face_surface`, `step_seam`, `step_window`, `cylinder_wall`,
  `from_parameters`; `curve::{from_parameters, arc_of, step_edge}`) are record conversions -> `record_convert`; `mid_joint`, `mid_sheet` -> `offset`. `procedural_blend` (7,502 lines, 47 refs to
  `blend`) and `procedural_sweep` stay in `surface` as the architecture says, but require the blend/sweep definition evaluators to be moved down.
- `eval/` (21 k lines) is a library of its own in this analysis (`evalrec`): 4 upward references to `prism::profile` (and 6 to the umbrella), and `eval::{hole_stacks, outline, plan, query, pcurve, agreement}` are
  record-level analyses that read like `convert`/`ops` code; marked M.
- `mass/` -> `topo`; its quadrature core (`integrator`) could go to `algo` but names the body 50 times (L).


## 9. Crates outside the kernel (MEASURED)

`siblings.py` counts every `solidcore::a::b` path in code (each leaf of a `use solidcore::{..}` tree, each expression path, comments and strings blanked, `#[cfg(test)]` code included, as
`xtask/src/solidcore_paths.rs` does). **It reproduces the repository's own caps exactly: `crates/smlib` 651 and `crates/geom` 20** (`xtask/solidcore-paths.txt`).
`xtask/solidcore-paths.txt` caps the count of paths into non-public solidcore crates per app crate (only `smlib` and `geom` have a line); `xtask/solidcore-seam.txt` lists the app
crates allowed to use the `solidcore` crate at all (`smlib`, `geom`); `xtask/solidcore-public.txt` lists the crates any app crate may use: `solidcore-ops`, `solidcore-tess`,
`solidcore-record`, `solidcore-step`, `solidcore-cadir`. The kernel crate `solidcore` is **not** public, so every path counted below is a reach into the kernel itself, not a facade; the
only facade is a separate crate.

Paths into the kernel crate, by crate (paths; of which in `src/` vs tests/benches/examples; root-level names such as `solidcore::Kernel` vs module paths; paths with a numbered `pN` module
or deeper than four segments; files):

| crate | paths | src | tests etc. | root names | module paths | numbered or deep | files | in `cfg(test)` tail of a src file |
|---|---|---|---|---|---|---|---|---|
| solidcore/crates/goldens | 11365 | 11064 | 301 | 1165 | 10200 | 2049 | 433 | 886 |
| solidcore/crates/solidcore-ops | 1915 | 1460 | 455 | 431 | 1484 | 208 | 128 | 82 |
| crates/smlib | 651 | 651 | 0 | 114 | 537 | 139 | 27 | 37 |
| solidcore/crates/solidcore-convert | 173 | 92 | 81 | 27 | 146 | 20 | 12 | 0 |
| solidcore/crates/solidcore-step | 96 | 46 | 50 | 5 | 91 | 9 | 12 | 7 |
| solidcore/crates/step-export | 82 | 47 | 35 | 0 | 82 | 22 | 2 | 15 |
| solidcore/crates/solidcore-tess | 75 | 75 | 0 | 17 | 58 | 0 | 19 | 18 |
| solidcore/crates/solidcore-ssi | 44 | 44 | 0 | 1 | 43 | 8 | 3 | 17 |
| crates/geom | 20 | 20 | 0 | 6 | 14 | 0 | 2 | 4 |
| solidcore/crates/diagnostics | 16 | 16 | 0 | 5 | 11 | 3 | 2 | 2 |
| solidcore/crates/solidcore-cadir | 6 | 6 | 0 | 4 | 2 | 0 | 5 | 0 |


Which kernel areas (paths; the number in brackets is how many sibling or app crates name the area): `topology` 3,229 (7), `geom` 2,478 (11), `error` 2,088 (7), `nurbs` 1,697 (8),
`surface` 1,072 (8), `curve` 825 (7), `kernel` 573 (9), `blend` 557 (5), `intersect` 376 (3), `math` 348 (6), `query` 292 (5), `primitive` 150 (6), `sweep` 132, `mass` 128, `eval` 127 (7),
`heal` 126, `boolean` 106, `offset` 57, `prism` 51, and 1 to 13 for `develop`, `cell_grid`, `hatch`, `mesh_reduce`, `profile`, `limits`, `hlr`. Per crate: `goldens` (the oracle) reaches
into 19 areas, mostly `topology`, `geom`, `error`, `nurbs`; `solidcore-ops` into 25; `smlib` (the app's kernel wrapper) into 17: `topology` 210, `error` 129, `geom` 103, `nurbs` 45,
`surface` 37, `query` 31, `kernel` 23, `curve` 20; `geom` (app) into `eval` 14 and `geom` 6; `solidcore-tess` into `eval` 57, `geom` 17 and one `nurbs` path. Per-path rows: `sibling_refs.tsv`.

Reach into internals rather than a root-level name (JUDGED reading of the data): in `smlib` 114 of 651 paths are root names (`Kernel`, `Brep`, `Vec3`, ...) and 537 name a module path;
139 contain a numbered module (`p1`..`p9`, which `lib.rs` says are public only for compatibility) or are deeper than four segments. In `goldens` it is 2,049 of 11,365 and in
`solidcore-ops` 208 of 1,915. So after the planned rearrangement, any path in `smlib` of the second kind will break when `p1..p9` disappear, and the area names in the paths
(`topology::...`, `geom::...`) will not exist as such.

App crates use facade crates heavily: `solidcore_record` 956 paths in `crates/kernel`, 1,057 in `engine`, 650 in `ui-egui`, 524 in `doc`; `solidcore_ops` 228 in `crates/kernel`, 162 in
`smlib`, 23 in `geom`; `solidcore_tess` 43 in `viewport-gpu`, 41 in `kernel`. Only `smlib` (651) and `geom` (20) reach into the kernel crate itself; no other app crate has such a path in code. The
sibling crates under `solidcore/crates/` are not subject to the caps and reach in freely: `solidcore-ops` 1,915, `solidcore-convert` 173, `step-export` 82, `solidcore-tess` 75, `solidcore-ssi` 44.
The dependency edges between them (`Cargo.toml`): `solidcore-ops` -> `solidcore`, `-record`, `-convert`; `-convert` -> `solidcore`, `-record`; `-tess` -> `solidcore`, `-record`, `-convert`,
`-ops`; `goldens` -> `solidcore`, `ssi`, `solidcore-ssi`, `solidcore-record`, `step-export`; `solidcore-record` depends on no solidcore crate.


## 10. The twenty most-referenced items across area boundaries (MEASURED)

Counted over cross-area references (use-leaf lines and path expressions); name-use sites are uses of the imported name inside referencing files. 'kind fn' with 0 name-use sites are called as `module::function(..)`. Top 60: `top_items.tsv`.

| # | item | kind | defined in | references | files | areas | name-use sites |
|---|---|---|---|---|---|---|---|
| 1 | `Result` | type | `error` | 758 | 635 | 19 | 15267 |
| 2 | `SolidError` | enum | `error` | 575 | 559 | 19 | 7657 |
| 3 | `Kernel` | struct | `kernel` | 548 | 486 | 15 | 10350 |
| 4 | `Vec3` | struct | `geom::vector3d` | 465 | 452 | 19 | 12458 |
| 5 | `Point3` | type | `geom::vector3d` | 390 | 365 | 16 | 6443 |
| 6 | `Curve` | struct | `curve::curve::p2` | 344 | 300 | 14 | 3570 |
| 7 | `Surface` | struct | `surface::surface::p2` | 289 | 262 | 14 | 2277 |
| 8 | `EFF_ZERO` | const | `math` | 282 | 264 | 14 | 1978 |
| 9 | `Extent1d` | struct | `geom::extent1d` | 280 | 253 | 14 | 2369 |
| 10 | `edge` | fn | `topology::edgeuse::p1` | 264 | 72 | 8 | 0 |
| 11 | `EntityRef` | enum | `error` | 258 | 209 | 11 | 4094 |
| 12 | `Extent2d` | struct | `geom::extent2d` | 248 | 218 | 14 | 2050 |
| 13 | `Point2` | type | `geom::vector2d` | 246 | 234 | 15 | 3585 |
| 14 | `interval` | fn | `topology::edge` | 237 | 81 | 10 | 0 |
| 15 | `Brep` | struct | `topology` | 230 | 208 | 13 | 3373 |
| 16 | `FaceId` | struct | `topology` | 221 | 190 | 13 | 1633 |
| 17 | `EdgeId` | struct | `topology` | 210 | 160 | 12 | 1436 |
| 18 | `Failure` | struct | `error` | 198 | 182 | 13 | 499 |
| 19 | `CurveKind` | enum | `curve::curve::p2` | 190 | 160 | 12 | 1134 |
| 20 | `point` | fn | `topology::vertex` | 190 | 49 | 9 | 0 |


These are the de facto shared vocabulary (JUDGED): `Vec3`, `Point3`, `Point2`, `Extent1d`, `Extent2d`, `EFF_ZERO` (the tolerance constant, in `math`) and the small `math` functions belong to `num`
and are already free of upward references; `Curve`, `CurveKind`, `Surface` are the `curve`/`surface` base types; `Brep`, `FaceId`, `EdgeId`, `VertexId` the body; `edge::interval`, `vertex::point` and
`edgeuse::edge` are accessors called by every operation crate (they are the body's read interface, `brep`). The top three, `Result`, `SolidError`, `Kernel`, are the umbrella.
`EntityRef` and `Failure` are error vocabulary. Nothing in the top 20 is defined in an operation crate: shared vocabulary is all in the foundation, the geometry kinds and the body, which is the shape the
layering needs.


## 11. What was not done, and how far to trust it

- Method calls and trait dispatch are not references here; the rewrite will find more coupling than this graph shows where code goes through traits (for instance `Context`, `AssertValid`
  virtual members). Type-level coupling is complete.
- The library mapping is judgement at file granularity. Rankings that depend on it (section 6 and 7) move with it: change `layers.py` and rerun `analyse.py`, `extras.py`, `order.py`, `make_report.py`.
- `brep` against `topo` is the weakest part: cutting `topology` by file leaves 416 references from `brep` to `topo` files; do not read that as a number to remove, but as evidence that the
  split needs to be by function (the `SmBrep` members).
- Counts of name-use sites include umbrella types; they measure edit effort, not coupling.
- The test-skip rule is by item; items under other `cfg` flags (`wr57-replay`, `profile`, `doc`) are counted as code.
- Not examined: the 26 compatibility switches' effect on the graph, `goldens`' own layering, `tests/` of the kernel crate, `step`/`ssi` internals.

## 12. Files in `49-data/`

`depmap.py`, `graph.py`, `layers.py`, `analyse.py`, `extras.py`, `order.py`, `siblings.py`, `make_report.py` (scripts); `meta.json` (modules, files, line and function counts);
`refs.tsv` (every reference row: file, line, source module, defining module, item, kind, form, flags); `area_edges.tsv`, `mod2_edges.tsv`, `mod2_split_edges.tsv`, `sccs.txt`;
`cuts_area_{refs,uses,items,pairs}.tsv` and `area_peer_refs.tsv`; `file_map.tsv`, `module_map.tsv`, `lib_summary.tsv`, `lib_matrix.tsv`, `lib_upward_refs.tsv`, `lib_upward_uses.tsv`,
`lib_cut_groups.tsv`, `lib_peer_refs.tsv`, `extraction_order_proposed.tsv`, `extraction_order_by_rank.tsv`, `extraction_order_greedy.tsv`, `extras.txt`; `umbrella_refs_by_area.tsv`,
`error_outgoing_refs.tsv`, `pub_use_cross_area.tsv` (55 cross-area `pub use` rows); `top_items.tsv`; `sibling_refs.tsv`. `name_uses.tsv` (18 MB) is regenerated by `depmap.py` and is not kept.

