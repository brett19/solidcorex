#!/usr/bin/env python3
"""Compare the SCIP reference graph with research 49's path references.

Usage: crosscheck.py --src PIN/solidcore/crates/solidcore/src --occ occurrences.tsv --refs49 refs.tsv

refs.tsv is 49's depmap output for the same pin. Both graphs are reduced to file-to-file edges
between distinct kernel files (non-test code only), and the counts of each, of the overlap and of
each side's extra edges are printed, with a sample of each side's extras classified by kind.
"""
import argparse
import collections
import csv
import os
import random
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from prove import Kernel  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--src', required=True)
    ap.add_argument('--occ', required=True)
    ap.add_argument('--refs49', required=True)
    ap.add_argument('--sample', type=int, default=40)
    a = ap.parse_args()
    K = Kernel(a.src)
    defs, refs = {}, []
    for l in open(a.occ):
        f, line, role, sym = l.rstrip('\n').split('\t', 3)
        if role == 'def':
            defs.setdefault(sym, (f, int(line)))
        else:
            refs.append((f, int(line), sym))
    ok = lambda f, n: f in K.text and f not in K.test and n < K.cut[f]  # noqa: E731
    scip = collections.defaultdict(set)
    for f, n, sym in refs:
        d = defs.get(sym)
        if d and ok(f, n) and ok(*d) and d[0] != f:
            scip[(f, d[0])].add(sym)
    # 49: module path -> file
    mod2file = {}
    for f in K.files:
        m = f[:-3]
        if m.endswith('/mod'):
            m = m[:-4]
        mod2file['crate::' + m.replace('/', '::') if m != 'lib' else 'crate'] = f
        mod2file[m.replace('/', '::')] = f

    def file_of(m):
        m = m.replace('crate::', '')
        while m and m not in mod2file:
            m = m.rsplit('::', 1)[0] if '::' in m else ''
        return mod2file.get(m)
    csv.field_size_limit(10 ** 9)
    old = collections.defaultdict(set)
    with open(a.refs49) as fh:
        r = csv.reader(fh, delimiter='\t', quoting=csv.QUOTE_NONE)
        h = next(r)
        for row in r:
            d = dict(zip(h, row))
            if d.get('dropped') == '1' or d.get('pub_use') == '1':
                continue
            tf = file_of(d['tgt_mod'])
            n = int(d['line'] or 0)
            if tf and tf != d['file'] and ok(d['file'], n) and ok(tf, 1):
                old[(d['file'], tf)].add(d['item'])
    s, o = set(scip), set(old)
    print(f'file-to-file edges: SCIP {len(s)}, 49 {len(o)}, both {len(s & o)}, '
          f'SCIP only {len(s - o)}, 49 only {len(o - s)}')
    # a SCIP symbol ending in '/' names a module (a path segment such as `curve::` in
    # `curve::Curve`); those edges point at the module's own file, not at an item in it
    si = {e for e, v in scip.items() if any(not x.endswith('/') for x in v)}
    print(f'without module-path segments: SCIP {len(si)}, both {len(si & o)}, '
          f'SCIP only {len(si - o)}, 49 only {len(o - si)}')
    print(f'distinct (file pair, item) uses: SCIP {sum(len(v) for v in scip.values())}, '
          f'49 {sum(len(v) for v in old.values())}')
    random.seed(56)
    for name, extra in (('SCIP only', sorted(s - o)), ('49 only', sorted(o - s))):
        print(f'\nsample of {name}:')
        for e in random.sample(extra, min(a.sample, len(extra))):
            items = sorted(scip.get(e) or old.get(e))[:3]
            print(f'  {e[0]} -> {e[1]}  {" ".join(items)}')


if __name__ == '__main__':
    main()
