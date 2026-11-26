"""Report 49: proposed extraction order and the cuts at each step.  python3 -I order.py DATA_DIR
Order of the coupled operation crates is the permutation with the fewest references pointing at a crate extracted later (brute force over 8!)."""
import sys, os, csv, json, itertools, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import layers
D = sys.argv[1]
meta = json.load(open(os.path.join(D, 'meta.json'))); FILES = meta['files']; MODS = meta['modules']
fl = {f: layers.lib_of(f)[0] for f, i in FILES.items() if not i['test_file']}
mf = {m: v['file'] for m, v in MODS.items()}
KL = {'Tolerances', 'Rng', 'CancelToken', 'KernelOptions', 'UpstreamFixes'}
def area(m): return m.split('::')[0]
rows = [r for r in csv.DictReader(open(os.path.join(D, 'refs.tsv')), delimiter='\t', quoting=csv.QUOTE_NONE) if r['dropped'] == '0' and r['src_mod'] != '']
E = collections.Counter()
for r in rows:
    sl = fl[r['file']]; tl = 'num' if (r['tgt_mod'] == 'kernel' and r['item'] in KL) else fl[mf[r['tgt_mod']]]
    if sl != tl: E[(sl, tl)] += 1
coupled = ['blend', 'boolean', 'defeature', 'heal', 'hlr', 'offset', 'primitive', 'sweep']
best = None
for p in itertools.permutations(coupled):
    pos = {l: i for i, l in enumerate(p)}
    c = sum(E[(a, b)] for a in coupled for b in coupled if a != b and pos[b] > pos[a])
    if best is None or c < best[0]: best = (c, p)
print('best coupled order', best)
leaves = ['prism', 'develop', 'hatch', 'cell_grid', 'mesh_reduce']
ORDER = ['profile', 'num', 'algo', 'nurbs', 'evalrec', 'planar', 'curve', 'surface', 'geomq', 'brep', 'topo'] + leaves + list(best[1]) + ['record_convert', 'umbrella']
out = ['step\tlibrary\tstructural_cuts(refs to libs not yet extracted, excl umbrella)\tumbrella_refs\tstructural_detail']
done = set()
for n, l in enumerate(ORDER, 1):
    c = collections.Counter()
    for (a, b), k in E.items():
        if a == l and b not in done and b != l: c[b] += k
    umb = c.pop('umbrella', 0)
    out.append(f"{n}\t{l}\t{sum(c.values())}\t{umb}\t" + '; '.join(f'{k}:{v}' for k, v in c.most_common()))
    done.add(l)
open(os.path.join(D, 'extraction_order_proposed.tsv'), 'w').write('\n'.join(out) + '\n')
print('\n'.join(out))
