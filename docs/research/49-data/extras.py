"""Report 49 extras: library-graph SCCs, merged brep+topo, nodes outside the second-level SCC.  python3 -I extras.py DATA_DIR"""
import sys, os, csv, json, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import layers
D = sys.argv[1]
meta = json.load(open(os.path.join(D, 'meta.json'))); FILES = meta['files']; MODS = meta['modules']
fl = {f: layers.lib_of(f)[0] for f, i in FILES.items() if not i['test_file']}
mf = {m: v['file'] for m, v in MODS.items()}
KL = {'Tolerances', 'Rng', 'CancelToken', 'KernelOptions', 'UpstreamFixes'}
def area(m): return m.split('::')[0]
rows = [r for r in csv.DictReader(open(os.path.join(D, 'refs.tsv')), delimiter='\t', quoting=csv.QUOTE_NONE) if r['dropped'] == '0' and r['src_mod'] != '']
E = collections.Counter(); out = []
def L(r):
    sl = fl[r['file']]; tf = mf[r['tgt_mod']]
    tl = 'num' if (r['tgt_mod'] == 'kernel' and r['item'] in KL) else fl[tf]
    return sl, tl
for r in rows:
    sl, tl = L(r)
    if sl != tl and 'umbrella' not in (sl, tl) and area(r['tgt_mod']) != 'error': E[(sl, tl)] += 1
def scc(nodes, adj):
    sys.setrecursionlimit(10000); idx, low, on, st, res = {}, {}, set(), [], []; c = [0]
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
            res.append(sorted(comp))
    for v in sorted(nodes):
        if v not in idx: go(v)
    return res
nodes = {a for a, b in E} | {b for a, b in E}
adj = collections.defaultdict(set)
for a, b in E: adj[a].add(b)
out.append('LIB-LEVEL SCCs today (error area and umbrella excluded): %s' % [c for c in scc(nodes, adj) if len(c) > 1])
adj2 = collections.defaultdict(set)
for (a, b) in E:
    if layers.RANK[a] >= layers.RANK[b]: adj2[a].add(b)
out.append('after removing upward edges: %s' % [c for c in scc(nodes, adj2) if len(c) > 1])
# merged brep+topo upward refs
R = dict(layers.RANK); R['brep'] = R['topo'] = 7
up = collections.Counter(); upn = 0
for r in rows:
    sl, tl = L(r)
    if sl in ('brep', 'topo') and R[sl] < R[tl]:
        up[tl] += 1
out.append('brep+topo merged: upward refs by target lib (all, incl umbrella): %s total %d' % (dict(up), sum(up.values())))
out.append('brep+topo merged upward excl umbrella: %d' % (sum(v for k, v in up.items() if k != 'umbrella')))
# refs between brep and topo
bt = collections.Counter()
for r in rows:
    sl, tl = L(r)
    if {sl, tl} == {'brep', 'topo'}: bt[(sl, tl)] += 1
out.append('brep<->topo refs: %s' % dict(bt))
# lib matrix excluding umbrella
libs = [l for l in layers.ORDER if l not in ('umbrella',)]
out.append('LIB MATRIX (refs, error area and umbrella excluded)')
out.append('from\\to\t' + '\t'.join(libs))
for a in libs: out.append(a + '\t' + '\t'.join(str(E[(a, b)] or '') for b in libs))
# second-level nodes outside the big SCC
sc = open(os.path.join(D, 'sccs.txt')).read().split('\n')
open(os.path.join(D, 'extras.txt'), 'w').write('\n'.join(out) + '\n')
print('\n'.join(out))
