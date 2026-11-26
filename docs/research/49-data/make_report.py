"""Assemble 49-module-dependencies.md from the TSVs (numbers) and the prose below (judgements).
python3 -I make_report.py DATA_DIR OUT_MD"""
import sys, os, csv, json, collections, ast
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import layers

D, OUT = sys.argv[1], sys.argv[2]


def tsv(name):
    with open(os.path.join(D, name)) as f:
        return list(csv.DictReader(f, delimiter='\t', quoting=csv.QUOTE_NONE))


def md(headers, rows):
    s = '| ' + ' | '.join(headers) + ' |\n|' + '|'.join('---' for _ in headers) + '|\n'
    for r in rows: s += '| ' + ' | '.join(str(x) for x in r) + ' |\n'
    return s


def n(x): return f"{int(x):,}"


meta = json.load(open(os.path.join(D, 'meta.json'))); F = meta['files']
nt = {f: i for f, i in F.items() if not i['test_file']}
lines_nt = sum(i['lines'] - i['skipped'] for i in nt.values()); fns_nt = sum(i['fns'] for i in nt.values())
edges = tsv('area_edges.tsv'); E = {(r['from'], r['to']): r for r in edges}
pairs = tsv('cuts_area_pairs.tsv'); items = tsv('cuts_area_items.tsv')
libsum = tsv('lib_summary.tsv'); fmap = tsv('file_map.tsv')
sc = open(os.path.join(D, 'sccs.txt')).read().split('\n')
def sccs(line): return ast.literal_eval(line[line.index(':') + 1:].strip())
sc_area, sc_area_x, sc_split, sc_all = sccs(sc[0]), sccs(sc[1]), sccs(sc[2]), sccs(sc[4])
extras = open(os.path.join(D, 'extras.txt')).read()
peer = tsv('lib_peer_refs.tsv'); order = tsv('extraction_order_proposed.tsv')
top = tsv('top_items.tsv'); umb = tsv('umbrella_refs_by_area.tsv')
areas_order = ['math', 'limits', 'geom', 'nurbs', 'eval', 'curve', 'surface', 'intersect', 'topology', 'query', 'mass', 'blend', 'boolean',
               'offset', 'sweep', 'primitive', 'heal', 'prism', 'hlr', 'develop', 'cell_grid', 'hatch', 'mesh_reduce', 'profile', 'error', 'kernel']
tot_refs = sum(int(r['refs']) for r in edges)
tot_cross = tot_refs
up_pairs = [p for p in pairs]
tot_up = sum(int(p['references']) for p in up_pairs)

o = []
w = o.append
w("""# 49: Module dependencies of the kernel crate

Rewrite, phase 0. Read-only analysis of which part of the 1.16 M-line crate refers to which, written to plan the order in
which `num`, `algo`, `nurbs`, ... are cut out (`01-architecture.md` section 2). Every statement is labelled **MEASURED** (produced by
the scripts in `49-data/`) or **JUDGED** (my reading; the evidence is given). Nothing was built or run; no repository was touched.

## 1. What was analysed (MEASURED)

- Source: `sc-wt/cache-leaf-split-2/solidcore/crates/solidcore/src`, branch `cache-leaf-split-2`, commit
  `53a7f148562b5c83c8798d6314cbda9060987fdd` ("solidcore: format the normalised evaluation's calls after the rename", 2026-11-25),
  working tree clean (`git status --short` empty). No fetch was done; this is what was checked out.
- %s `.rs` files, all reachable from `lib.rs` through `mod` declarations (0 orphans); %s of them are test-only (38,865 lines).
  %s non-test lines in %s files; %s non-test functions (the architecture document says "about 25,300").
- Sibling and app crates: the same worktree, `solidcore/crates/*` and `crates/*`.
""" % (n(len(F)), len(F) - len(nt), n(lines_nt), len(nt), n(fns_nt)))

w("""## 2. Method and its blind spots

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

""")

# ---------------- section 3
w("## 3. The area graph (MEASURED)\n")
w(f"26 top-level areas (`lib.rs` excluded), {len(edges)} directed edges, {n(tot_refs)} cross-area references of {n(35701)} in all. "
  "Cell = number of references from the row area to the column area. Full table with files and distinct items: `49-data/area_edges.tsv`; "
  "second-level graphs: `mod2_edges.tsv` (all 386 second-level nodes) and `mod2_split_edges.tsv` (only the eight areas to be split expanded).\n")
hdr = ['from \\ to'] + [a[:6] for a in areas_order]
rows = []
for a in areas_order:
    rows.append([a] + [(E[(a, b)]['refs'] if (a, b) in E else '') for b in areas_order])
w(md(hdr, rows))
w("\nThe thirty heaviest edges, with the three weights asked for:\n")
rows = [[r['from'], r['to'], r['refs'], r['files'], r['distinct_items']] for r in edges[:30]]
w(md(['from', 'to', 'references', 'referencing files', 'distinct items'], rows))
w("""
Reading it (JUDGED): `error` and `kernel` are referenced from nearly everything (section 5.3). Below them, the dense core is
`blend`, `topology`, `surface`, `intersect`, `offset`, `curve`, `query`, with `geom` and `nurbs` referenced by all of them. The
edges that point the wrong way for the target layering are in section 5.

""")

# ---------------- section 4
w("## 4. Cycles (MEASURED)\n")
w(f"""**Area level.** One strongly connected component of {sc_area[0][0]} of the 26 areas: {', '.join(sc_area[0][1])}.
Outside it: `cell_grid`, `develop`, `hatch`, `hlr`, `limits`, `mesh_reduce`, `profile` (all are leaves or sit above the component).
Without `error` and `kernel` the component is {sc_area_x[0][0]} areas (the same list minus those two): the cycle does not come from
the umbrella types; `geom`, `nurbs`, `math` and `curve` are inside it, i.e. the "bottom" libraries today depend on the top.

**Second level, the eight areas to be split expanded** (every other area one node): one component of **{sc_split[0][0]}** modules out of
236 nodes, holding most second-level modules of `geom` (34 of 38), `surface` (28 of 39), `blend` (25 of 28), `curve` (25 of 32), `topology` (23 of 24), `query` (14 of 26),
`eval` (12 of 22), `intersect` (11 of 13), plus one node each for `error`, `kernel`, `math`, `heal`, `offset`, `primitive`, `sweep`, `boolean`, `mass`, `prism`, `nurbs`. Removing the `(root)`
facade nodes changes it only to {sccs(sc[3])[0][0]}. With all areas expanded: one component of **{max(s for s, c in sc_all)}** of 386 second-level
modules, plus seven small ones that are facade artefacts inside one area (`cell_grid`, `develop`, `hatch`, `hlr`, `mesh_reduce`: root
file and children refer to each other; `curve::arc` with `curve::path`) and one real two-area one
(`curve::{{conic,fit,spline}}_projection` with `prism::{{measure,read}}`).
Modules outside the large component (all areas, {386 - max(s for s, c in sc_all)} nodes, seven of them in the small components above) are mostly leaves: most of `eval`, `nurbs::{{funcs_adv, gordon,
srf_match, ...}}`, `query::{{contact, poly_properties, tess, ...}}`, `surface::{{face_*, from_parameters, recognise, ...}}`, `geom::{{step_matrix,
facing_trough, plane_height, axial_section}}`. List: `49-data/sccs.txt`.

**Library level (using the mapping of section 6).** The libraries of the target, with the references measured between them
(excluding `error` and the umbrella pieces), form one component of {len(ast.literal_eval(extras.split(chr(10))[0].split(': ', 1)[1])[0])} of the 24
libraries other than the umbrella today. Removing every reference that points upward (section 6) leaves two residual cycles, both between libraries the
architecture treats as peers:
- `curve` <-> `surface`: `surface` -> `curve` 314 references, `curve` -> `surface` 48. Cut the 48 and `curve` goes first.
- the operation crates `blend, boolean, defeature, heal, hlr, offset, primitive, sweep`: 8 crates, 38 references pointing at a crate
  extracted later in the best order (`boolean, heal, hlr, sweep, primitive, blend, defeature, offset`; found by trying all 8! orders).
  The largest couplings: `offset` -> `blend` 36 (reverse 4), `primitive` -> `sweep` 36 (reverse 12), `blend` -> `boolean` 29,
  `primitive` -> `boolean` 23, `offset` -> `heal` 18, `sweep` -> `boolean` 18, `offset` -> `boolean` 15. The architecture says these crates "depend on
  0 to 6, not on each other, except where stated"; today `blend`, `offset` and `sweep` each refer to six of the other seven, `primitive` to four.

""")

# ---------------- section 5
w("## 5. Cuts at area level (MEASURED counts; the order of areas is JUDGED)\n")
w("""A cut is a reference from an area that the target layering puts **below** another to that other area. Ranks used (JUDGED, one per area;
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
""")
w("### 5.1 Pairs ranked by number of cuts\n")
rows = []
for p in up_pairs[:28]:
    rows.append([f"{p['lower_area']} -> {p['upper_area']}", p['references'], p['files'], p['distinct_items'], p['name_use_sites'], p['in_downward_cycle']])
w(md(['lower -> upper', 'references (cuts)', 'files', 'distinct items', 'name-use sites', 'refs the other way'], rows))
w(f"\nIn all: {len(up_pairs)} area pairs, **{n(tot_up)} upward references** (excluding `error` and `kernel`). Every pair of the top 28 also has "
  "references in the other direction (last column), which is what closes the cycle. Per-site lists: `cuts_area_refs.tsv` "
  f"({n(len(tsv('cuts_area_refs.tsv')))} rows: file, line, item, kind, classification), `cuts_area_uses.tsv` ({n(len(tsv('cuts_area_uses.tsv')))} name-use sites), "
  "`cuts_area_items.tsv` (grouped by item with the dominant uses).\n")
w("### 5.1b What the cut sites are, and the peer pairs\n")
cl = collections.Counter(); 
for r in tsv('cuts_area_uses.tsv'): cl[r['classification'].replace(' (error/outcome type)', '')] += 1
w("Name-use sites of the items behind the %s upward references, by what the use is (MEASURED, heuristic context):\n" % n(tot_up))
w(md(['use', 'sites'], [[k, n(v)] for k, v in cl.most_common(12)]))
w("\nPeer pairs (same rank, both directions; these are not cuts but must be ordered). Top of `area_peer_refs.tsv`:\n")
ap = tsv('area_peer_refs.tsv'); apd = {(r['from'], r['to']): int(r['refs']) for r in ap}
seen = set(); rows = []
for r in ap:
    k = tuple(sorted((r['from'], r['to'])))
    if k in seen: continue
    seen.add(k); rows.append([f"{r['from']} -> {r['to']}", r['refs'], apd.get((r['to'], r['from']), 0)])
w(md(['pair', 'references', 'reverse'], rows[:12]))
w("")
w("### 5.2 The ten largest pairs, grouped by referenced item (references / name-use sites)\n")
for p in up_pairs[:10]:
    a, b = p['lower_area'], p['upper_area']
    rs = [r for r in items if r['lower_area'] == a and r['upper_area'] == b]
    rs.sort(key=lambda r: -(int(r['references']) + int(r['name_use_sites'])))
    w(f"**{a} -> {b}** ({p['references']} references in {p['files']} files, {p['distinct_items']} distinct items)\n")
    for r in rs[:5]:
        cls = r['main_use_classes'] if r['main_use_classes'] else (
            'function call' if r['item_kind'] == 'fn' else 'path use')
        w(f"- `{r['item']}` ({r['item_kind']}, `{r['target_module']}`): {r['references']} references, {r['name_use_sites']} name-use sites; mainly: {cls}")
    w("")
w("""What the groups say (JUDGED, from the lists):
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

""")
w("### 5.3 `error` and `kernel`: referenced from everywhere (MEASURED)\n")
rows = [[r['from_area'], r['error_refs'], r['error_files'], r['kernel_refs'], r['kernel_files']] for r in umb]
rows.sort(key=lambda r: -int(r[1]))
w(md(['from area', 'refs to `error`', 'files', 'refs to `kernel`', 'files'], rows))
w("""
By item: `Result` 758 references (635 files), `SolidError` 575 (559), `Kernel` 548 (486), `EntityRef` 258 (209), `Failure` 198 (182),
`ignored` 188, `ignore_status` 119, the `SM_*` status codes about 275, `math::EFF_ZERO` 282 (`math` is inside the future `num`).
The `error` area itself refers upward: `error::blend` to `FaceId`/`EdgeId`/`VertexId` (36 references with `error::blend::solver`), `error.rs` to blend and offset types
(`FilletObject`, `FitKind`, `OffsetExecutive`) and to every id type. `kernel.rs` refers to `blend`, `surface`, `mass`, `nurbs`, `topology`,
`geom::Context` and `profile` because the `Kernel` struct holds their memos as fields (`StationMemo`, `ThumbweightMemo`,
`BlendFaceLog`, `BlendCounts`, `ReplaySeam`, `RecertifyMemo`, `LoopAreaMemo`, `AnalysisMemo`, `CoonsTangencyLog`, four `NotifyRecords`,
a `Profiler`). Cutting these two is the same work as "settings passed in, scratch passed in" and "one error enum per crate"; it is
counted separately in every table below because it touches every file and decides nothing about layering.

""")

# ---------------- section 6
w("## 6. Mapping modules to target libraries (JUDGED) and what each library has today (MEASURED given the mapping)\n")
w("""The mapping is in `layers.py` (longest-prefix rules on file paths, each with a confidence: H doc comment and references agree, M plausible,
L uncertain), and per file in `file_map.tsv`, per second-level module in `module_map.tsv`. Libraries added to the architecture's list
because the measurements ask for them: `evalrec` (evaluators on plain records, `eval/`: depends only on `num` and `nurbs`, used by
`curve`, `surface`, `hlr`, `develop`, `prism`) and `planar` (the planar-profile core of `prism/`, used by `curve::*_projection`, `cell_grid`, `hatch`,
`develop`). `record_convert` is `record` plus `convert`; `umbrella` is `SolidError`/`Result`/`EntityRef`/status codes (`error.rs`, `error/*` root
items), the `Kernel` object and `lib.rs`. Operation crates are one library each; `defeature` is `blend/feature_executive` plus its error type;
`hlr` also takes `blend/hcr` and `topology/api_projection`.
""")
rank_note = {l: layers.RANK[l] for l in layers.RANK}
rows = []
for r in libsum:
    rows.append([r['library'], r['rank'], r['files'], n(r['nontest_lines']), n(r['nontest_fns']), n(r['upward_refs']),
                 n(r['upward_refs_excl_umbrella_lib']), r['upward_files'], n(r['upward_name_use_sites']), r['peer_refs']])
w(md(['library', 'rank', 'files', 'non-test lines', 'non-test fns', 'upward refs today', 'of which not to `umbrella`', 'files holding them',
      'name-use sites of upward items', 'refs to peers'], rows))
w("""
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
""")
comp = collections.defaultdict(collections.Counter); conf = collections.defaultdict(collections.Counter)
for r in fmap:
    f = r['file']; ar = f.split('/')[0][:-3] if '/' not in f else f.split('/')[0]
    comp[ar][r['library']] += int(r['nontest_lines']); conf[r['library']][r['confidence']] += int(r['nontest_lines'])
rows = []
for a in areas_order + ['lib', 'topology']:
    pass
for a in sorted(comp, key=lambda a: -sum(comp[a].values())):
    rows.append([a, n(sum(comp[a].values())), '; '.join(f"{l} {n(v)}" for l, v in comp[a].most_common())])
w(md(['area', 'non-test lines', 'goes to (library lines)'], rows))
w("\nUncertain mappings, by library: lines at confidence L / M / H.\n")
rows = [[l, n(conf[l]['L']), n(conf[l]['M']), n(conf[l]['H'])] for l in layers.ORDER if sum(conf[l].values())]
w(md(['library', 'L', 'M', 'H'], rows))
w("""
The `L` lines: `geom/{content, axial_section, facing_trough}` (unclear home), `geom/{tools, assert_array}` (to `topo`), `error/query`,
`kernel.rs` (must be split), plus the `M` bulk in `topology/` where each file is marked by its doc comment, not by its functions.

""")

# ---------------- section 7
w("## 7. Proposed extraction order (JUDGED order; cut counts MEASURED)\n")
w("""Rule: a library is extracted when everything it uses is already extracted. "Cuts" = references from the library to libraries not yet extracted at that
step, so the step cannot compile on its own until they are removed (moved, inverted behind a trait, or the item moved down). The umbrella column is the
`SolidError` / `Result` / `Kernel` / `EntityRef` / status-code references that every step also carries; it is the same cut each time
(a per-crate error and "settings passed in") and is not repeated in the detail.
""")
rows = [[r['step'], r['library'], r['structural_cuts(refs to libs not yet extracted, excl umbrella)'], r['umbrella_refs'], r['structural_detail']] for r in order]
w(md(['step', 'library', 'cuts', 'umbrella refs', 'where the cuts go (target library: refs)'], rows))
w("""
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

""")

# ---------------- section 8
w("## 8. Items that do not fit the layering (JUDGED, with measured evidence)\n")
fm = {r['file']: r for r in fmap}
up = collections.defaultdict(collections.Counter)
for r in tsv('lib_upward_refs.tsv'):
    up[r['file']]['umb' if r['to_lib'] == 'umbrella' else 'up'] += 1


def filetable(prefixes, notes=None):
    rows = []
    for f in sorted(fm):
        if any(f == p or f.startswith(p) for p in prefixes):
            r = fm[f]
            rows.append([f"`{f}`", n(r['nontest_lines']), r['nontest_fns'], r['library'], r['confidence'], up[f]['up'], up[f]['umb']])
    return md(['file', 'lines', 'fns', 'library', 'conf', 'upward refs', 'umbrella refs'], rows)


w("### 8.1 `geom/` (48 k lines, 40 files, 38 modules): maths, solvers and body-side types in one directory\n")
w(filetable(['geom.rs', 'geom/']).replace('geom/', 'geom/'))
w("""
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

""")
w("### 8.2 `error/` (30 k lines in 38 files, including the test-only scanners; 3,519 lines in `error.rs`)\n")
w("""`error.rs` (2,105 non-test lines) is the umbrella `SolidError` with `EntityRef`, `Failure` and the `SM_*` status codes; it refers to blend, offset and every id type
(`error_outgoing_refs.tsv`: 56 rows). It stays at the top, as `ops`'s wrapper. The per-area files are the per-crate enums the architecture asks for, and map by name:
`error/geom` -> `num`, `error/nurbs` -> `nurbs`, `error/curve` -> `curve`, `error/surface` -> `surface`, `error/intersect` -> `geomq`, `error/topology` -> `brep`, `error/mass` -> `topo`,
`error/boolean`, `blend`, `offset`, `sweep`, `primitive`, `heal` -> the same-named crate, `error/defeature` -> `defeature`. `error/query` (377 lines) is used from `nurbs` (2 refs) upward,
so it must sit at `num` (L). The ids are the constraint: `error/blend` (6,730 lines) carries `FaceId`/`EdgeId`/`VertexId` (36 references with `error/blend/solver`), so an error enum that names an entity can only
live in a crate that can see `brep`; `error/surface` (1 ref to `topology`) and `error/offset` (5) are nearly clean. Four files (`bare_scan`, `phrase_sites`, `site_scan`, `stage_sites`)
are test-only source scanners (`#[cfg(test)] mod`) and are not in the counts; they scan this crate's own source and would move with the tests.

""")
w("### 8.3 `kernel.rs` (1,200 non-test lines)\n")
w("""Eight items: `Tolerances` (l.47), `Rng` (96), `Caches` (136), `WorkCounters` (189), `UpstreamFixes` (751, to be deleted), `KernelOptions` (756), `CancelToken` (794), `Kernel`
(916). Divide by item (JUDGED): `Tolerances`, `KernelOptions`, `Rng`, `CancelToken` -> `num` (the `&Settings` and `&Cancel` of the calling convention; referenced from 8, 5, 2, 7 files);
`Caches`, `WorkCounters` and the memos currently fields of `Kernel` -> a `Scratch` that each crate defines its own part of (the struct today names types of `blend`, `surface`, `mass`,
`nurbs`, `topology`, `profile` and `geom::Context`: 20 references from `kernel.rs` to six higher areas, plus `geom::Context`); `Kernel` itself disappears. Measured use of the object: 548 references in
486 files across 15 areas, 10,350 name-use sites.

""")
w("### 8.4 `query/` (52 k lines, 35 modules)\n")
w(filetable(['query.rs', 'query/']).replace('query/', 'query/'))
w("""
Divide by file (JUDGED): geometry-only queries go to `geomq` (`curve`, `deviation`, `distance_bound`, `normal`, `point_identity`, `line_meeting`, `vertex_meet`, `curve_surface_gap`, `seam_bound`);
`tess` is a trimmed-surface cache and goes to `surface`; everything that takes a face, a loop, an edge or a body goes to `topo` (`face` 8,650 lines, `brep`, `edge`, `loop`, `api_queries`,
`tangent_edge`, `contact`, `continuity`, `poly` ...). `query::curve_surface_gap` (5,395 lines, 24 refs to `blend`) and `query::normal` (6 refs) are `geomq`'s main cuts: they use the interval
and certified-NURBS machinery now in `blend`.

""")
w("### 8.5 `topology/` (146 k lines), `intersect/`, `heal/`, `blend/`, `nurbs/`, `surface/`, `curve/`\n")
w("""- `topology/`: the 49,643-line `brep/` directory and the entity files are one body written as upstream's `SmBrep` members split across `p1..p10`; each `pN.rs` mixes data editing and
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

""")

# ---------------- section 9
w("## 9. Crates outside the kernel (MEASURED)\n")
sib = open(os.path.join(D, 'siblings_summary.txt')).read()
w("""`siblings.py` counts every `solidcore::a::b` path in code (each leaf of a `use solidcore::{..}` tree, each expression path, comments and strings blanked, `#[cfg(test)]` code included, as
`xtask/src/solidcore_paths.rs` does). **It reproduces the repository's own caps exactly: `crates/smlib` 651 and `crates/geom` 20** (`xtask/solidcore-paths.txt`).
`xtask/solidcore-paths.txt` caps the count of paths into non-public solidcore crates per app crate (only `smlib` and `geom` have a line); `xtask/solidcore-seam.txt` lists the app
crates allowed to use the `solidcore` crate at all (`smlib`, `geom`); `xtask/solidcore-public.txt` lists the crates any app crate may use: `solidcore-ops`, `solidcore-tess`,
`solidcore-record`, `solidcore-step`, `solidcore-cadir`. The kernel crate `solidcore` is **not** public, so every path counted below is a reach into the kernel itself, not a facade; the
only facade is a separate crate.

Paths into the kernel crate, by crate (paths; of which in `src/` vs tests/benches/examples; root-level names such as `solidcore::Kernel` vs module paths; paths with a numbered `pN` module
or deeper than four segments; files):
""")
sibrows = []
for line in sib.split('\n'):
    if line.startswith('solidcore/crates/') or line.startswith('crates/'):
        if ' | ' in line:
            p = [x.strip() for x in line.split(' | ')]
            sibrows.append(p)
w(md(['crate', 'paths', 'src', 'tests etc.', 'root names', 'module paths', 'numbered or deep', 'files', 'in `cfg(test)` tail of a src file'], sibrows))
w("""
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

""")

# ---------------- section 10
w("## 10. The twenty most-referenced items across area boundaries (MEASURED)\n")
w("Counted over cross-area references (use-leaf lines and path expressions); name-use sites are uses of the imported name inside referencing files. 'kind fn' with 0 name-use sites are called as `module::function(..)`. Top 60: `top_items.tsv`.\n")
rows = [[r['rank'], f"`{r['item']}`", r['kind'], f"`{r['defined_in']}`", r['references'], r['files'], r['source_areas'], r['name_use_sites']] for r in top[:20]]
w(md(['#', 'item', 'kind', 'defined in', 'references', 'files', 'areas', 'name-use sites'], rows))
w("""
These are the de facto shared vocabulary (JUDGED): `Vec3`, `Point3`, `Point2`, `Extent1d`, `Extent2d`, `EFF_ZERO` (the tolerance constant, in `math`) and the small `math` functions belong to `num`
and are already free of upward references; `Curve`, `CurveKind`, `Surface` are the `curve`/`surface` base types; `Brep`, `FaceId`, `EdgeId`, `VertexId` the body; `edge::interval`, `vertex::point` and
`edgeuse::edge` are accessors called by every operation crate (they are the body's read interface, `brep`). The top three, `Result`, `SolidError`, `Kernel`, are the umbrella.
`EntityRef` and `Failure` are error vocabulary. Nothing in the top 20 is defined in an operation crate: shared vocabulary is all in the foundation, the geometry kinds and the body, which is the shape the
layering needs.

""")

# ---------------- section 11
w("""## 11. What was not done, and how far to trust it

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
""")
open(OUT, 'w').write('\n'.join(o) + '\n')
print('written', OUT, sum(len(x) for x in o))
