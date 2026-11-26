"""Report 49, step 3: cuts, library mapping, extraction order, shared vocabulary.
python3 -I analyse.py DATA_DIR      (needs refs.tsv, name_uses.tsv, meta.json from depmap.py; layers.py alongside)
Writes TSVs into DATA_DIR and prints the numbers the report quotes.
"""
import sys, os, csv, json, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import layers

D = sys.argv[1]
meta = json.load(open(os.path.join(D, 'meta.json')))
FILES, MODS = meta['files'], meta['modules']


def rd(name):
    with open(os.path.join(D, name)) as f:
        return list(csv.DictReader(f, delimiter='\t', quoting=csv.QUOTE_NONE))


REFS = [r for r in rd('refs.tsv') if r['dropped'] == '0' and r['src_mod'] != '']   # lib.rs facade re-exports excluded
USES = rd('name_uses.tsv')
CTX = {'0': 'body', '1': 'param', '2': 'ret', '3': 'field', '4': 'impl_hdr', '5': 'impl_for', '6': 'decl', '7': 'use', '8': 'sig_other'}


def area(m): return m.split('::')[0]


def classify(kind, ctx, prev, nxt, tgt_mod):
    err = ' (error/outcome type)' if area(tgt_mod) == 'error' else ''
    if kind == 'fn': return 'function call' + err
    if kind in ('const', 'static'): return 'constant' + err
    if kind == 'macro': return 'macro'
    if kind == 'mod': return 'module path'
    if kind == 'glob': return 'glob import'
    what = {'struct': 'type', 'enum': 'type', 'union': 'type', 'type': 'type alias', 'trait': 'trait'}.get(kind, kind)
    c = CTX.get(str(ctx), 'body')
    if c == 'use': return 'import of ' + what + err
    if c == 'field': return what + ' in struct/enum field' + err
    if c == 'param': return what + ' in fn parameter' + err
    if c == 'ret': return what + ' in fn return type' + err
    if c == 'sig_other': return what + ' in fn generics/where' + err
    if c == 'impl_hdr': return 'impl header (trait or inherent target)' + err
    if c == 'impl_for': return 'trait impl for ' + what + err
    if c == 'decl': return what + ' in alias/const type' + err
    if nxt == '::': return what + ' associated fn/const/variant (Name::x)' + err
    if nxt == '(': return what + ' constructor/variant call' + err
    if nxt == '{': return what + ' struct literal/pattern' + err
    if prev in (':', '<', '&', 'as', 'mut'): return what + ' in a type annotation in body' + err
    return what + ' value/pattern in body' + err


# ------------------------------------------------------------------ area ranks from the library mapping
fl = {}     # file -> (lib, conf, note)
for f, i in FILES.items():
    if not i['test_file']:
        fl[f] = layers.lib_of(f)
mod_file = {m: v['file'] for m, v in MODS.items()}


def lib_of_mod(m):
    return fl[mod_file[m]][0]


# area rank: lines-weighted majority library rank
arl = collections.defaultdict(collections.Counter)
for f, i in FILES.items():
    if i['test_file']: continue
    a = i['mod'].split('::')[0] if i['mod'] else '(lib)'
    arl[a][fl[f][0]] += i['lines'] - i['skipped']
AREA_LIB = {a: c.most_common(1)[0][0] for a, c in arl.items()}
# Area-level ranks for the "A is below B" test (JUDGED; areas that mix libraries get the rank of the group they are part of).
# error and kernel are cross-cutting umbrella areas: reported separately, not ranked against the rest.
AREA_RANK = {'math': 0, 'limits': 0, 'geom': 0, 'nurbs': 2, 'eval': 3, 'curve': 5, 'surface': 5, 'intersect': 6,
             'topology': 7, 'query': 7, 'mass': 7,
             'blend': 9, 'boolean': 9, 'offset': 9, 'sweep': 9, 'primitive': 9, 'heal': 9, 'hlr': 9, 'develop': 9,
             'prism': 9, 'cell_grid': 9, 'hatch': 9, 'mesh_reduce': 9, 'profile': 0}
UMB_AREAS = {'error', 'kernel'}
json.dump(dict(AREA_LIB=AREA_LIB, AREA_RANK=AREA_RANK), open(os.path.join(D, 'area_ranks.json'), 'w'), indent=1)

# ------------------------------------------------------------------ area-level cuts
uses_by = collections.defaultdict(list)
for u in USES:
    uses_by[(u['file'], u['name'], u['tgt_mod'], u['item'])].append(u)

edge_refs = collections.defaultdict(list)
for r in REFS:
    a, b = area(r['src_mod']), area(r['tgt_mod'])
    if a != b: edge_refs[(a, b)].append(r)

cuts = []          # upward rows
for (a, b), rs in edge_refs.items():
    if a in UMB_AREAS or b in UMB_AREAS: continue
    if AREA_RANK[a] < AREA_RANK[b]:
        cuts += rs
with open(os.path.join(D, 'cuts_area_refs.tsv'), 'w') as f:
    f.write('file\tline\tfrom_area\tto_area\ttarget_module\titem\titem_kind\tref_kind\tclassification\n')
    for r in sorted(cuts, key=lambda r: (area(r['src_mod']), area(r['tgt_mod']), r['file'], int(r['line']))):
        f.write('\t'.join([r['file'], r['line'], area(r['src_mod']), area(r['tgt_mod']), r['tgt_mod'], r['item'], r['item_kind'], r['ref_kind'],
                           classify(r['item_kind'], r['ctx'], '', r['next'], r['tgt_mod'])]) + '\n')
# name uses for upward items
upward_uses = []
for u in USES:
    a, b = area(u['src_mod']), area(u['tgt_mod'])
    if a != b and a not in UMB_AREAS and b not in UMB_AREAS and AREA_RANK[a] < AREA_RANK[b]:
        upward_uses.append(u)
with open(os.path.join(D, 'cuts_area_uses.tsv'), 'w') as f:
    f.write('file\tline\tfrom_area\tto_area\ttarget_module\titem\titem_kind\tclassification\n')
    for u in sorted(upward_uses, key=lambda u: (area(u['src_mod']), area(u['tgt_mod']), u['file'], int(u['line']))):
        f.write('\t'.join([u['file'], u['line'], area(u['src_mod']), area(u['tgt_mod']), u['tgt_mod'], u['item'], u['item_kind'],
                           classify(u['item_kind'], u['ctx'], u['prev'], u['next'], u['tgt_mod'])]) + '\n')

pair = collections.defaultdict(lambda: dict(refs=0, uses=0, files=set(), items=collections.Counter(), cls=collections.Counter()))
for r in cuts:
    p = pair[(area(r['src_mod']), area(r['tgt_mod']))]
    p['refs'] += 1; p['files'].add(r['file']); p['items'][(r['tgt_mod'], r['item'], r['item_kind'])] += 1
for u in upward_uses:
    p = pair[(area(u['src_mod']), area(u['tgt_mod']))]
    p['uses'] += 1
    p['items'][(u['tgt_mod'], u['item'], u['item_kind'])] += 0
    p['cls'][classify(u['item_kind'], u['ctx'], u['prev'], u['next'], u['tgt_mod'])] += 1
with open(os.path.join(D, 'cuts_area_pairs.tsv'), 'w') as f:
    f.write('lower_area\tupper_area\treferences\tfiles\tdistinct_items\tname_use_sites\tin_downward_cycle\n')
    for (a, b), p in sorted(pair.items(), key=lambda kv: -(kv[1]['refs'])):
        down = len(edge_refs.get((b, a), []))
        f.write(f"{a}\t{b}\t{p['refs']}\t{len(p['files'])}\t{len({(k[0], k[1]) for k in p['items']})}\t{p['uses']}\t{down}\n")
# items per pair with uses
iu = collections.Counter(); ucls = collections.defaultdict(collections.Counter)
for u in upward_uses:
    k = (area(u['src_mod']), area(u['tgt_mod']), u['tgt_mod'], u['item'], u['item_kind'])
    iu[k] += 1
    ucls[k][classify(u['item_kind'], u['ctx'], u['prev'], u['next'], u['tgt_mod'])] += 1
ir = collections.Counter()
for r in cuts:
    ir[(area(r['src_mod']), area(r['tgt_mod']), r['tgt_mod'], r['item'], r['item_kind'])] += 1
keys = set(iu) | set(ir)
with open(os.path.join(D, 'cuts_area_items.tsv'), 'w') as f:
    f.write('lower_area\tupper_area\ttarget_module\titem\titem_kind\treferences\tname_use_sites\tmain_use_classes\n')
    for k in sorted(keys, key=lambda k: (k[0], k[1], -(ir[k] + iu[k]))):
        top = '; '.join(f'{c}: {n}' for c, n in ucls[k].most_common(3))
        f.write('\t'.join(map(str, list(k) + [ir[k], iu[k], top])) + '\n')

# ------------------------------------------------------------------ library-level
KERNEL_LOW = {'Tolerances', 'Rng', 'CancelToken', 'KernelOptions', 'UpstreamFixes'}   # Settings-like parts of Kernel (JUDGED)


def tgt_lib(tgt_mod, item, tf):
    if tgt_mod == 'kernel' and item in KERNEL_LOW: return 'num'
    return fl[tf][0]


def lib_pair_rows():
    rows = []
    for r in REFS:
        sf = r['file']
        tf = mod_file.get(r['tgt_mod'])
        if tf is None or tf not in fl: continue     # target inside a test-only module cannot happen for non-test refs
        sl, tl = fl[sf][0], tgt_lib(r['tgt_mod'], r['item'], tf)
        if sl != tl: rows.append((r, sl, tl, tf))
    return rows


LP = lib_pair_rows()
RANK = layers.RANK
UMB_ITEMS = {('error', 'Result'), ('error', 'SolidError'), ('error', 'EntityRef'), ('kernel', 'Kernel')}


def is_umbrella(r):
    return area(r['tgt_mod']) in UMB_AREAS


with open(os.path.join(D, 'lib_upward_refs.tsv'), 'w') as f:
    f.write('file\tline\tfrom_lib\tto_lib\ttarget_file\ttarget_module\titem\titem_kind\tref_kind\tclassification\n')
    for r, sl, tl, tf in sorted(LP, key=lambda x: (RANK[x[1]], x[1], RANK[x[2]], x[2], x[0]['file'], int(x[0]['line']))):
        if RANK[sl] < RANK[tl]:
            f.write('\t'.join([r['file'], r['line'], sl, tl, tf, r['tgt_mod'], r['item'], r['item_kind'], r['ref_kind'],
                               classify(r['item_kind'], r['ctx'], '', r['next'], r['tgt_mod'])]) + '\n')
LPU = []
for u in USES:
    sf = u['file']; tf = mod_file.get(u['tgt_mod'])
    if tf is None or tf not in fl: continue
    sl, tl = fl[sf][0], tgt_lib(u['tgt_mod'], u['item'], tf)
    if sl != tl and RANK[sl] < RANK[tl]: LPU.append((u, sl, tl))
with open(os.path.join(D, 'lib_upward_uses.tsv'), 'w') as f:
    f.write('file\tline\tfrom_lib\tto_lib\ttarget_module\titem\titem_kind\tclassification\n')
    for u, sl, tl in sorted(LPU, key=lambda x: (RANK[x[1]], x[1], RANK[x[2]], x[2], x[0]['file'], int(x[0]['line']))):
        f.write('\t'.join([u['file'], u['line'], sl, tl, u['tgt_mod'], u['item'], u['item_kind'],
                           classify(u['item_kind'], u['ctx'], u['prev'], u['next'], u['tgt_mod'])]) + '\n')

with open(os.path.join(D, 'lib_cut_groups.tsv'), 'w') as f:
    g = collections.defaultdict(lambda: dict(refs=0, files=set(), uses=0))
    for r, sl, tl, tf in LP:
        if RANK[sl] < RANK[tl] and tl != 'umbrella':
            x = g[(sl, tl, r['tgt_mod'], r['item'], r['item_kind'])]; x['refs'] += 1; x['files'].add(r['file'])
    for u, sl, tl in LPU:
        if tl == 'umbrella': continue
        g[(sl, tl, u['tgt_mod'], u['item'], u['item_kind'])]['uses'] += 1
    f.write('from_lib\tto_lib\ttarget_module\titem\titem_kind\treferences\tfiles\tname_use_sites\n')
    for k, x in sorted(g.items(), key=lambda kv: (RANK[kv[0][0]], kv[0][0], -(kv[1]['refs'] + kv[1]['uses']))):
        f.write('\t'.join(map(str, list(k) + [x['refs'], len(x['files']), x['uses']])) + '\n')

# per library statistics
stat = {l: dict(files=0, lines=0, fns=0, up=0, up_noumb=0, up_noerr=0, peer=0, down=0, up_files=set(), uses_up=0) for l in layers.ORDER}
for f, i in FILES.items():
    if i['test_file']: continue
    s = stat[fl[f][0]]
    s['files'] += 1; s['lines'] += i['lines'] - i['skipped']; s['fns'] += i['fns']
libmat = collections.defaultdict(collections.Counter)
for r, sl, tl, tf in LP:
    libmat[sl][tl] += 1
    s = stat[sl]
    if RANK[sl] < RANK[tl]:
        s['up'] += 1; s['up_files'].add(r['file'])
        if tl != 'umbrella': s['up_noumb'] += 1
        if tl != 'umbrella' and area(r['tgt_mod']) != 'error': s['up_noerr'] += 1
    elif RANK[sl] == RANK[tl]: s['peer'] += 1
    else: s['down'] += 1
for u, sl, tl in LPU: stat[sl]['uses_up'] += 1
with open(os.path.join(D, 'lib_summary.tsv'), 'w') as f:
    f.write('library\trank\tfiles\tnontest_lines\tnontest_fns\tupward_refs\tupward_refs_excl_umbrella_lib\tupward_refs_excl_umbrella_lib_and_error_area\tupward_files\tupward_name_use_sites\tpeer_refs\tdownward_refs\n')
    for l in layers.ORDER:
        s = stat[l]
        f.write(f"{l}\t{RANK[l]}\t{s['files']}\t{s['lines']}\t{s['fns']}\t{s['up']}\t{s['up_noumb']}\t{s['up_noerr']}\t{len(s['up_files'])}\t{s['uses_up']}\t{s['peer']}\t{s['down']}\n")
with open(os.path.join(D, 'lib_matrix.tsv'), 'w') as f:
    f.write('from\\to\t' + '\t'.join(layers.ORDER) + '\n')
    for a in layers.ORDER:
        f.write(a + '\t' + '\t'.join(str(libmat[a][b] or '') for b in layers.ORDER) + '\n')
with open(os.path.join(D, 'file_map.tsv'), 'w') as f:
    f.write('file\tlibrary\tconfidence\tnontest_lines\tnontest_fns\tnote\n')
    for fn, (l, c, n) in sorted(fl.items()):
        i = FILES[fn]
        f.write(f"{fn}\t{l}\t{c}\t{i['lines'] - i['skipped']}\t{i['fns']}\t{n}\n")
# module map (second level) with library split
m2 = collections.defaultdict(collections.Counter); m2c = collections.defaultdict(set)
for fn, (l, c, n) in fl.items():
    p = FILES[fn]['mod'].split('::') if FILES[fn]['mod'] else ['(lib)']
    k = p[0] + '::(root)' if len(p) == 1 else '::'.join(p[:2])
    m2[k][l] += FILES[fn]['lines'] - FILES[fn]['skipped']; m2c[k].add(c)
with open(os.path.join(D, 'module_map.tsv'), 'w') as f:
    f.write('module\tlines\tlibraries(lines)\tconfidence\n')
    for k in sorted(m2):
        f.write(f"{k}\t{sum(m2[k].values())}\t{'; '.join(f'{l}:{n}' for l, n in m2[k].most_common())}\t{''.join(sorted(m2c[k]))}\n")

# ------------------------------------------------------------------ extraction order
def cuts_for(lib, done):
    """refs from `lib` to libraries not in `done` and not lib itself; umbrella counted apart"""
    c = collections.Counter()
    for r, sl, tl, tf in LP:
        if sl == lib and tl not in done:
            c[tl] += 1
    return c


def fmt(c):
    umb = c.get('umbrella', 0)
    rest = [(k, v) for k, v in c.most_common() if k != 'umbrella']
    return sum(v for k, v in rest), umb, '; '.join(f'{k}:{v}' for k, v in rest)


LIBS_PRESENT = [l for l in layers.ORDER if stat[l]['files']]
done = []; remaining = list(LIBS_PRESENT)
with open(os.path.join(D, 'extraction_order_greedy.tsv'), 'w') as f:
    f.write('step\tlibrary\tstructural_refs_to_not_yet_extracted\tumbrella_refs\tdetail\n')
    n = 0
    while remaining:
        best = None
        for l in remaining:
            if l == 'umbrella': continue
            c = cuts_for(l, set(done))
            tot = fmt(c)[0]
            key = (tot, RANK[l])
            if best is None or key < best[0]: best = (key, l, c)
        if best is None: break
        _, l, c = best
        n += 1
        t, u, det = fmt(c)
        f.write(f"{n}\t{l}\t{t}\t{u}\t{det}\n")
        done.append(l); remaining.remove(l)
done2 = set(); strict = []
for l in LIBS_PRESENT:
    c = cuts_for(l, done2 | {l})
    strict.append((l,) + fmt(c))
    done2.add(l)
with open(os.path.join(D, 'extraction_order_by_rank.tsv'), 'w') as f:
    f.write('step\tlibrary\tstructural_refs_to_not_yet_extracted\tumbrella_refs\tdetail\n')
    for n, (l, t, u, det) in enumerate(strict, 1):
        f.write(f"{n}\t{l}\t{t}\t{u}\t{det}\n")

# peers: references between libraries of equal rank (both directions)
peer = collections.Counter()
for r, sl, tl, tf in LP:
    if RANK[sl] == RANK[tl]: peer[(sl, tl)] += 1
with open(os.path.join(D, 'lib_peer_refs.tsv'), 'w') as f:
    f.write('from\tto\trefs\treverse_refs\n')
    for (a_, b_), n in sorted(peer.items(), key=lambda kv: -kv[1]):
        f.write(f"{a_}\t{b_}\t{n}\t{peer.get((b_, a_), 0)}\n")

# residual cycles at area level after removing the upward cuts (peers stay)
def scc(nodes, adj):
    sys.setrecursionlimit(10000)
    idx, low, on, st, out = {}, {}, set(), [], []
    c = [0]
    def go(v):
        idx[v] = low[v] = c[0]; c[0] += 1; st.append(v); on.add(v)
        for w in adj.get(v, ()):
            if w not in idx: go(w); low[v] = min(low[v], low[w])
            elif w in on: low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = st.pop(); on.discard(w); comp.append(w)
                if w == v: break
            out.append(sorted(comp))
    for v in sorted(nodes):
        if v not in idx: go(v)
    return out


adj = collections.defaultdict(set); nodes = set()
for (a_, b_), rs in edge_refs.items():
    if a_ in UMB_AREAS or b_ in UMB_AREAS: continue
    nodes |= {a_, b_}
    if AREA_RANK[a_] >= AREA_RANK[b_]: adj[a_].add(b_)
resid = [c for c in scc(nodes, adj) if len(c) > 1]
# peers of equal rank: both directions counts
peer_area = collections.Counter()
for (a_, b_), rs in edge_refs.items():
    if a_ in UMB_AREAS or b_ in UMB_AREAS: continue
    if AREA_RANK[a_] == AREA_RANK[b_]: peer_area[(a_, b_)] = len(rs)
with open(os.path.join(D, 'area_peer_refs.tsv'), 'w') as f:
    f.write('from\tto\trefs\n')
    for (a_, b_), n in sorted(peer_area.items(), key=lambda kv: -kv[1]): f.write(f"{a_}\t{b_}\t{n}\n")
# umbrella (error/kernel) references per source area
umb = collections.defaultdict(lambda: collections.Counter())
umbfiles = collections.defaultdict(lambda: collections.defaultdict(set))
for r in REFS:
    b_ = area(r['tgt_mod']); a_ = area(r['src_mod'])
    if b_ in UMB_AREAS and a_ != b_:
        umb[a_][b_] += 1; umbfiles[a_][b_].add(r['file'])
with open(os.path.join(D, 'umbrella_refs_by_area.tsv'), 'w') as f:
    f.write('from_area\terror_refs\terror_files\tkernel_refs\tkernel_files\n')
    for a_ in sorted(umb):
        f.write(f"{a_}\t{umb[a_]['error']}\t{len(umbfiles[a_]['error'])}\t{umb[a_]['kernel']}\t{len(umbfiles[a_]['kernel'])}\n")
# what the error area itself references (upward from the point of view of a per-area error type)
errout = collections.Counter()
for r in REFS:
    if area(r['src_mod']) == 'error' and area(r['tgt_mod']) != 'error':
        errout[(r['src_mod'], area(r['tgt_mod']), r['item'])] += 1
with open(os.path.join(D, 'error_outgoing_refs.tsv'), 'w') as f:
    f.write('error_module\ttarget_area\titem\trefs\n')
    for k, n in sorted(errout.items(), key=lambda kv: -kv[1]): f.write('\t'.join(map(str, k + (n,))) + '\n')
json.dump(dict(residual_area_sccs=resid), open(os.path.join(D, 'residual_sccs.json'), 'w'))

# ------------------------------------------------------------------ top shared vocabulary (area boundaries)
item = collections.defaultdict(lambda: dict(refs=0, files=set(), areas=set(), uses=0))
for r in REFS:
    if area(r['src_mod']) == area(r['tgt_mod']) or r['item'] in ('(mod)', '*'): continue
    i = item[(r['tgt_mod'], r['item'], r['item_kind'])]
    i['refs'] += 1; i['files'].add(r['file']); i['areas'].add(area(r['src_mod']))
for u in USES:
    if area(u['src_mod']) == area(u['tgt_mod']): continue
    k = (u['tgt_mod'], u['item'], u['item_kind'])
    if k in item: item[k]['uses'] += 1
with open(os.path.join(D, 'top_items.tsv'), 'w') as f:
    f.write('rank\tdefined_in\titem\tkind\treferences\tfiles\tsource_areas\tname_use_sites\n')
    for n, (k, i) in enumerate(sorted(item.items(), key=lambda kv: -kv[1]['refs'])[:60], 1):
        f.write(f"{n}\t{k[0]}\t{k[1]}\t{k[2]}\t{i['refs']}\t{len(i['files'])}\t{len(i['areas'])}\t{i['uses']}\n")

# ------------------------------------------------------------------ pub use re-exports across areas
pu = [r for r in REFS if r['pub_use'] == '1' and area(r['src_mod']) != area(r['tgt_mod'])]
with open(os.path.join(D, 'pub_use_cross_area.tsv'), 'w') as f:
    f.write('file\tline\tfrom_area\tto_area\ttarget_module\titem\n')
    for r in pu: f.write('\t'.join([r['file'], r['line'], area(r['src_mod']), area(r['tgt_mod']), r['tgt_mod'], r['item']]) + '\n')

# re-export through: textual target area differs from defining area
thru = collections.Counter()
for r in REFS:
    if r['textual_area'] and r['textual_area'] != area(r['tgt_mod']): thru[(r['textual_area'], area(r['tgt_mod']))] += 1

# ------------------------------------------------------------------ summary print
print('REFS', len(REFS), 'cross-area', sum(len(v) for v in edge_refs.values()), 'uses', len(USES))
print('re-export-through (textual area != defining area):', sum(thru.values()), thru.most_common(8))
print('pub use cross-area rows', len(pu))
print('--- area pairs, upward cuts (lower, upper, refs, files, items, uses, downward refs)')
for (a, b), p in sorted(pair.items(), key=lambda kv: -kv[1]['refs'])[:40]:
    print(a, b, p['refs'], len(p['files']), len({(k[0], k[1]) for k in p['items']}), p['uses'], len(edge_refs.get((b, a), [])))
print('--- libs')
print(open(os.path.join(D, 'lib_summary.tsv')).read())
print(open(os.path.join(D, 'extraction_order_greedy.tsv')).read())
print('residual area SCCs after removing upward cuts:', resid)
print(open(os.path.join(D, 'extraction_order_by_rank.tsv')).read())
print(open(os.path.join(D, 'top_items.tsv')).read()[:3000])
