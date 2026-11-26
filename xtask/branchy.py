#!/usr/bin/env python3
# Written for solidcorex. Apache-2.0. Adopted from the scx-num review's `branchy.py`
# (docs/research/reviews/scx-num-data/). From `llvm-cov export -format=text` JSON of the
# difftest run, rank the library functions by their code-region count (a proxy for
# branches), say which have regions the difftest never executed, then list every function
# with an unexecuted region and its lines. Monomorphisations are merged by (file, first
# line). Usage: branchy.py cov.json CRATES_DIR [N]
import json, sys, re, os
d = json.load(open(sys.argv[1])); crates = os.path.abspath(sys.argv[2])
n = int(sys.argv[3]) if len(sys.argv) > 3 else 30
rows = {}
for fn in d['data'][0]['functions']:
    fs = fn['filenames']
    if not fs or not os.path.abspath(fs[0]).startswith(crates + '/') or '/src/' not in fs[0]:
        continue
    regs = [r for r in fn['regions'] if r[7] == 0 and r[5] == 0]  # code regions in the fn's own file
    if not regs:
        continue
    key = (os.path.relpath(fs[0], crates), regs[0][0])
    hit = {(r[0], r[1], r[2], r[3]): r[4] > 0 for r in regs}
    if key in rows:
        for k, v in hit.items():
            rows[key][k] = rows[key].get(k, False) or v
    else:
        rows[key] = hit
lines = {}
def sig(f, l):
    if f not in lines:
        lines[f] = open(os.path.join(crates, f)).read().splitlines()
    L = lines[f]
    for i in range(l - 1, max(l - 40, 0), -1):
        m = re.search(r'\bfn\s+(\w+)', L[i])
        if m:
            return m.group(1)
    return '?'
out = []
for (f, l), hit in rows.items():
    miss = sorted(k[0] for k, v in hit.items() if not v)
    out.append((len(hit), len(miss), f, l, sig(f, l), miss))
out.sort(key=lambda r: (-r[0], r[2], r[3]))
for tot, miss, f, l, name, ml in out[:n]:
    print(f'{tot:4d} regions {miss:3d} unexecuted  {f}:{l} {name}' + (f'  (lines {sorted(set(ml))[:10]})' if miss else ''))
print(f'top {n}: {sum(1 for r in out[:n] if r[1])} of {n} have an unexecuted region')
tr = sum(r[0] for r in out); tm = sum(r[1] for r in out)
print(f'all functions: {len(out)}; with an unexecuted region {sum(1 for r in out if r[1])}; '
      f'never entered {sum(1 for r in out if r[1] == r[0])}; regions {tr - tm} of {tr} executed '
      f'({100.0 * (tr - tm) / max(tr, 1):.2f}%)')
print()
print('every function with an unexecuted region:')
for tot, miss, f, l, name, ml in sorted(out, key=lambda r: (r[2], r[3])):
    if miss:
        print(f'  {f}:{l} {name}: {miss} of {tot} unexecuted, lines {sorted(set(ml))}')
