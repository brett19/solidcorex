#!/usr/bin/env python3
"""Report 49, step 2: graphs and cycles.  python3 -I graph.py DATA_DIR
Reads refs.tsv + meta.json written by depmap.py; writes area_edges.tsv, mod2_edges.tsv, sccs.txt.
"""
import sys, os, json, csv, collections

D = sys.argv[1]
meta = json.load(open(os.path.join(D, 'meta.json')))
rows = []
with open(os.path.join(D, 'refs.tsv')) as f:
    rd = csv.DictReader(f, delimiter='\t', quoting=csv.QUOTE_NONE)
    for r in rd:
        r['line'] = int(r['line'])
        rows.append(r)
LIVE = [r for r in rows if r['dropped'] == '0']


def area(m):
    return m.split('::')[0] if m else '(lib)'


def mod2(m):
    p = m.split('::') if m else []
    if not p: return '(lib)'
    if len(p) == 1: return p[0] + '::(root)'
    return '::'.join(p[:2])


def edges(keyf):
    E = collections.defaultdict(lambda: dict(refs=0, files=set(), items=set(), glob=0, mods=0))
    for r in LIVE:
        a, b = keyf(r['src_mod']), keyf(r['tgt_mod'])
        if a == b or a == '(lib)': continue
        e = E[(a, b)]
        e['refs'] += 1; e['files'].add(r['file'])
        if r['item'] in ('*',): e['glob'] += 1
        elif r['item'] == '(mod)': e['mods'] += 1
        else: e['items'].add((r['tgt_mod'], r['item']))
    return E


def tarjan(nodes, adj):
    idx, low, on, st, out = {}, {}, set(), [], []
    c = [0]

    def sc(v):
        idx[v] = low[v] = c[0]; c[0] += 1; st.append(v); on.add(v)
        for w in adj.get(v, ()):
            if w not in idx:
                sc(w); low[v] = min(low[v], low[w])
            elif w in on:
                low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = st.pop(); on.discard(w); comp.append(w)
                if w == v: break
            out.append(sorted(comp))
    sys.setrecursionlimit(10000)
    for v in sorted(nodes):
        if v not in idx: sc(v)
    return out


def write(E, name):
    with open(os.path.join(D, name), 'w') as f:
        f.write('from\tto\trefs\tfiles\tdistinct_items\tglob_imports\tmodule_imports\n')
        for (a, b), e in sorted(E.items(), key=lambda kv: -kv[1]['refs']):
            f.write(f"{a}\t{b}\t{e['refs']}\t{len(e['files'])}\t{len(e['items'])}\t{e['glob']}\t{e['mods']}\n")


EA = edges(area)
E2 = edges(mod2)
write(EA, 'area_edges.tsv'); write(E2, 'mod2_edges.tsv')
areas = sorted({a for a, b in EA} | {b for a, b in EA})


def adj_of(E):
    adj = collections.defaultdict(set)
    for a, b in E: adj[a].add(b)
    return adj


out = []
sc_a = [c for c in tarjan(areas, adj_of(EA)) if len(c) > 1]
out.append('AREA-LEVEL SCCs: ' + str([(len(c), c) for c in sc_a]))
EAx = {k: v for k, v in EA.items() if k[0] not in ('error', 'kernel') and k[1] not in ('error', 'kernel')}
ax_nodes = sorted({a for a, b in EAx} | {b for a, b in EAx})
sc_ax = [c for c in tarjan(ax_nodes, adj_of(EAx)) if len(c) > 1]
out.append('AREA-LEVEL SCCs without error and kernel: ' + str([(len(c), c) for c in sc_ax]))
# split areas for mod2 graph (second-level only for the areas that will be split); other areas collapse to one node
SPLIT = {'geom', 'surface', 'curve', 'topology', 'query', 'intersect', 'eval', 'blend'}


def mod2_split(m):
    a = area(m)
    return mod2(m) if a in SPLIT else a


E2s = edges(mod2_split)
write(E2s, 'mod2_split_edges.tsv')
nodes2 = sorted({a for a, b in E2s} | {b for a, b in E2s})
sc2 = [c for c in tarjan(nodes2, adj_of(E2s)) if len(c) > 1]
out.append('MOD2 (split areas expanded) SCCs: ' + str([(len(c), c) for c in sc2]))
# without root facade nodes
E2n = {k: v for k, v in E2s.items() if not k[0].endswith('::(root)') and not k[1].endswith('::(root)')}
nodes2n = sorted({a for a, b in E2n} | {b for a, b in E2n})
sc2n = [c for c in tarjan(nodes2n, adj_of(E2n)) if len(c) > 1]
out.append('MOD2 without (root) nodes SCCs: ' + str([(len(c), c) for c in sc2n]))
# full second-level for all areas
nodes_all = sorted({a for a, b in E2} | {b for a, b in E2})
sc_all = [c for c in tarjan(nodes_all, adj_of(E2)) if len(c) > 1]
out.append('MOD2 all areas expanded SCCs: ' + str([(len(c), c) for c in sc_all]))
open(os.path.join(D, 'sccs.txt'), 'w').write('\n'.join(out) + '\n')
print('\n'.join(out))
print(len(areas), 'areas;', len(EA), 'area edges;', len(E2s), 'mod2 split edges;', len(E2), 'mod2 edges')
