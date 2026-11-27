#!/usr/bin/env python3
"""Check research 56's crate graph against a complete reference graph of the pin.

Usage:
    prove.py --src PIN/solidcore/crates/solidcore/src --occ occurrences.tsv [--assign assignment.py]
             [--report flows|units|bad|all]

Inputs:
  --src     the kernel crate's src/ directory at the pin (03e16c9ee7).
  --occ     scip_occurrences.py's output for the same pin (see README.md).
  --assign  the file and item assignment (default: assignment.py beside this script).

Method (56 section 9.1):
  1. Units. Each file is cut into top-level items: functions (including methods), structs, enums,
     consts, statics, type aliases, traits, and trait impl blocks (one unit each). Test code is
     dropped: files under a `#[cfg(test)] mod x;`, and the lines from a file's first inline
     `#[cfg(test)] mod x {` to its end. A `mod x;` declaration does not end a file's live code.
  2. Edges. Every SCIP reference inside a unit, to a symbol defined in the kernel crate, is an
     edge from that unit to the unit that holds the definition. SCIP resolves method calls,
     trait methods, fields, variants and same-module calls, so nothing is matched by name.
  3. Crates. Files get crates by longest prefix; then the item moves apply.
  4. Removals. The edges that a design change in 56 deletes are dropped (assignment.REMOVE).
  5. Sinking. For each (A, B) in assignment.SINK_WHOLE, the part of B that A's references reach,
     through B's own references, moves down to A whole, provided it needs nothing above A. Then a
     unit called from below moves down to its caller's crate with the part of its own crate that it
     reaches, when that part needs nothing above the caller and is at most SINK_CAP non-test lines
     (crates in NO_SINK keep their code; types outside HUBS may go with SINK_TYPES); a struct,
     enum or union sinks only with its impl blocks in the same module.
  6. Rising. Until nothing changes, a function or trait-impl unit whose edges leave its crate's
     closure moves to the lowest crate whose closure holds its crate and all its targets. A
     dependent never rises merely because its target rose (no cascade), except into the crates
     in RISE_WHOLE, whose families may take their callers along.
  7. Orphan rule. A trait impl block left in neither its trait's nor its self type's crate moves,
     with its methods, to whichever of the two sees all its targets and is seen by all its callers;
     failing that, the self type comes up to the impl's crate when everything that names it sees
     that crate. The impls still away from both, and the inherent methods moved without their
     type (which become free functions at the port), are counted.
  8. Check. Every edge must lie in the declared closure, and the crate graph must be acyclic.
     Units that cannot rise (types, traits, consts) and still leave the closure are listed.
"""
import argparse
import collections
import importlib.util
import os
import re
import sys

ITEMRE = re.compile(
    r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+|async\s+|unsafe\s+|extern\s+"C"\s+)*'
    r'(fn|struct|enum|union|const|static|type|trait|impl|mod|macro_rules!)\b\s*([A-Za-z_][A-Za-z0-9_]*)?')
KEEP = ('fn', 'struct', 'enum', 'union', 'trait', 'const', 'static', 'type', 'macro_rules!', 'impl')


def scan(text):
    """Top-level units (start, end, kind, name), 1-based inclusive. Methods inside impl and trait
    blocks are fn units; a trait impl block is also a unit named `impl:Trait`."""
    lines = text.split('\n')
    units, depth, stack, pending, inblk = [], 0, [], None, False
    for i, l in enumerate(lines, 1):
        t = re.sub(r'"(\\.|[^"\\])*"', '""', l)
        t = re.sub(r"'(\\.|[^'\\])'", "''", t)
        if inblk:
            if '*/' in t:
                t, inblk = t.split('*/', 1)[1], False
            else:
                continue
        t = re.sub(r'/\*.*?\*/', '', t)
        if '/*' in t:
            t, inblk = t.split('/*')[0], True
        t = t.split('//')[0]
        m = ITEMRE.match(t)
        infn = any(k == 'fn' for k, _, _, _ in stack)
        if m and not infn:
            kind, name = m.group(1), m.group(2) or ''
            if kind == 'impl':
                mm = re.match(r'^\s*(?:unsafe\s+)?impl\s*(?:<.*?>)?\s*(.*?)\s+for\s+', t)
                name = 'impl:' + re.sub(r'<.*', '', mm.group(1)).split('::')[-1].strip() if mm else 'impl'
            pending = (kind, name, i, depth)
        for ch in t:
            if ch == '{':
                if pending and pending[3] == depth:
                    stack.append(pending)
                    pending = None
                else:
                    stack.append(('blk', '', i, depth))
                depth += 1
            elif ch == '}':
                depth -= 1
                if stack:
                    k, n, s, _ = stack.pop()
                    if k in KEEP and not any(kk == 'fn' for kk, _, _, _ in stack):
                        units.append((s, i, k, n))
        if pending and t.rstrip().endswith(';') and pending[3] == depth:
            units.append((pending[2], i, pending[0], pending[1]))
            pending = None
    return units


def impl_self(line):
    m = re.match(r'^\s*(?:unsafe\s+)?impl\s*(<[^{]*?>)?\s*([A-Za-z_][A-Za-z0-9_:]*)', line)
    return m.group(2).split('::')[-1] if m else None


def mapper(rules):
    rules = sorted(rules, key=lambda x: -len(x[0]))
    cache = {}

    def g(f):
        if f not in cache:
            b = f[:-3] if f.endswith('.rs') else f
            res = None
            for p, c in rules:
                if p.endswith('/'):
                    hit = f.startswith(p)
                elif p.endswith('*'):
                    hit = b.startswith(p[:-1])
                elif p.endswith('.rs'):
                    hit = f == p
                else:
                    hit = b == p or b.startswith(p + '/')
                if hit:
                    res = c
                    break
            cache[f] = res
        return cache[f]
    return g


class Kernel:
    def __init__(self, src):
        self.src = src
        self.files = sorted(os.path.relpath(os.path.join(d, n), src)
                            for d, _, ns in os.walk(src) for n in ns if n.endswith('.rs'))
        self.text = {f: open(os.path.join(src, f)).read().split('\n') for f in self.files}
        self.cut = {}       # first test line (exclusive end of non-test code), 1-based
        testmods = set()
        for f, L in self.text.items():
            t = len(L) + 1
            for i, l in enumerate(L):
                if l.strip().startswith('#[cfg(test)]'):
                    nxt = ' '.join(L[i:i + 3])
                    m = re.search(r'\bmod\s+(\w+)\s*;', nxt)
                    if m:
                        p = re.search(r'#\[path\s*=\s*"([^"]+)"\]', nxt)
                        base = os.path.dirname(f)
                        stem = f[:-3]
                        if os.path.basename(f) in ('mod.rs', 'lib.rs'):
                            stem = base
                        if p:
                            testmods.add(os.path.normpath(os.path.join(base, p.group(1))))
                        else:
                            testmods.add(os.path.normpath(os.path.join(stem, m.group(1) + '.rs')))
                            testmods.add(os.path.normpath(os.path.join(stem, m.group(1), 'mod.rs')))
                    # only an inline test module starts the test tail; a `mod x;` declaration
                    # names a separate file (marked test above) and code after it is still live
                    if re.search(r'\bmod\s+\w+\s*\{', nxt) and t == len(L) + 1:
                        t = i + 1
            self.cut[f] = t
        self.test = set()
        for f in self.files:
            b = f[:-3]
            for tm in testmods:
                tb = tm[:-3] if not tm.endswith('mod.rs') else os.path.dirname(tm)
                if f == tm or b.startswith(tb + '/'):
                    self.test.add(f)
                    break
        for f in self.test:
            self.cut[f] = 1
        self.units = {f: scan('\n'.join(L)) for f, L in self.text.items()}
        self.byname = collections.defaultdict(dict)
        for f, us in self.units.items():
            for u in us:
                self.byname[f].setdefault(u[3], u)
        # inherent methods: fn unit -> self type of the enclosing inherent impl
        self.inherent = {}
        for f, us in self.units.items():
            imps = [u for u in us if u[2] == 'impl' and u[3] == 'impl']
            for u in us:
                if u[2] == 'fn':
                    for im in imps:
                        if im[0] < u[0] and u[1] <= im[1]:
                            self.inherent[(f, u[0])] = impl_self(self.text[f][im[0] - 1])
                            break
        self.trait_impl_of = {}
        self._starts = None
        for f, us in self.units.items():
            for u in us:
                if u[3].startswith('impl:'):
                    m = re.search(r'\bfor\s+&?(?:mut\s+)?([A-Za-z_][A-Za-z0-9_:]*)', self.text[f][u[0] - 1])
                    self.trait_impl_of[(f, u[0])] = (u[3][5:], m.group(1).split('::')[-1] if m else None)

    def nontest(self, f, s, e):
        c = self.cut[f] - 1
        return max(0, min(e, c) - s + 1) if s <= c else 0

    def unit_at(self, f, line):
        best = timpl = None
        for u in self.units.get(f, ()):
            if u[0] <= line <= u[1]:
                if u[3].startswith('impl:'):
                    if timpl is None or u[1] - u[0] > timpl[1] - timpl[0]:
                        timpl = u
                elif u[2] != 'impl':
                    if best is None or u[1] - u[0] < best[1] - best[0]:
                        best = u
        return timpl or best


def _units_in(self, f, a, b):
    return [(f, u[0]) for u in self.units[f] if a <= u[0] <= b]


Kernel.units_in = _units_in


def load_assign(path):
    spec = importlib.util.spec_from_file_location('assignment', path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def build_edges(K, occ):
    defs = {}
    refs = []
    for l in open(occ):
        f, line, role, sym = l.rstrip('\n').split('\t', 3)
        line = int(line)
        if role == 'def':
            defs.setdefault(sym, (f, line))
        else:
            refs.append((f, line, sym))
    E = collections.Counter()   # (src unit key, dst unit key) -> count
    sym_of = {}
    for f, line, sym in refs:
        if f not in K.text or f in K.test or line >= K.cut[f]:
            continue
        d = defs.get(sym)
        if d is None or d[0] not in K.text or d[0] in K.test or d[1] >= K.cut[d[0]]:
            continue
        su = K.unit_at(f, line)
        tu = K.unit_at(*d)
        if su is None or tu is None:
            continue
        a, b = (f, su[0]), (d[0], tu[0])
        if a == b:
            continue
        E[(a, b)] += 1
        sym_of.setdefault((a, b), sym)
    return E, sym_of


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--src', required=True)
    ap.add_argument('--occ', required=True)
    ap.add_argument('--assign', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'assignment.py'))
    ap.add_argument('--report', default='flows')
    ap.add_argument('--no-rise', action='store_true')
    ap.add_argument('--explain', default='')
    ap.add_argument('--needs', default='', help='PREFIX, the crates the units under it reach')
    ap.add_argument('--chain', default='', help='FILE:LINE, print the cause chain of that unit')
    ap.add_argument('--limit', type=int, default=40)
    ap.add_argument('--reach', default='', help='A:B, the part of B that the A -> B outside edges reach')
    args = ap.parse_args()
    A = load_assign(args.assign)
    K = Kernel(args.src)
    G = mapper(A.FILES)
    U = {(f, u[0]): u for f, us in K.units.items() for u in us}
    unassigned = [f for f in K.files if f not in K.test and G(f) is None]

    DEPS = {k: set(v.split()) for k, v in A.DEPS.items()}
    NP = A.NOT_PORTED
    for f in K.files:
        c = G(f)
        if c and c != NP:
            DEPS.setdefault(c, set())
    clo, lay = {}, {}

    def closure(c):
        if c not in clo:
            s = set()
            for d in DEPS.get(c, ()):
                s |= {d} | closure(d)
            clo[c] = s
        return clo[c]

    def layer(c):
        if c not in lay:
            lay[c] = 1 + max((layer(d) for d in DEPS.get(c, ())), default=-1)
        return lay[c]

    def lowest(cs):
        cands = [c for c in DEPS if cs <= closure(c) | {c}]
        return min(cands, key=lambda c: (layer(c), c)) if cands else None

    moved, why, cause = {}, {}, {}

    def find(f, n):
        u = K.byname[f].get(n)
        if u is None:
            sys.exit(f'assignment names a missing item: {f} {n}')
        return u

    # a type takes its inherent methods; an ITEMS entry for one of those methods wins
    for (f, n), c in A.TYPES.items():
        u = find(f, n)
        moved[(f, u[0])] = c
        why[(f, u[0])] = 'type'
        for (ff, s0), st in K.inherent.items():
            if ff == f and st == n:
                moved[(f, s0)] = c
                why[(f, s0)] = 'type'
    for (f, n), c in A.ITEMS.items():
        u = find(f, n)
        moved[(f, u[0])] = c
        why[(f, u[0])] = 'item'

    def crate(k):
        return moved.get(k, G(k[0]))

    E, sym_of = build_edges(K, args.occ)
    # design-change removals: (source file prefix or None, target file prefix, target name or None)
    def removed(a, b):
        sf, tf = a[0], b[0]
        sn, tn = U[a][3], U[b][3]
        for rs, rsn, rt, rtn in A.REMOVE:
            if rs and not (sf == rs or sf.startswith(rs)):
                continue
            if rsn and sn != rsn:
                continue
            if not (tf == rt or tf.startswith(rt)):
                continue
            if rtn and tn != rtn:
                continue
            return True
        return False
    live = {e: n for e, n in E.items() if not removed(*e)}
    dropped = sum(E.values()) - sum(live.values())
    out = collections.defaultdict(set)
    for (a, b) in live:
        out[a].add(b)

    # Whole sinks (step 5a, decided in 56): for each (A, B) in SINK_WHOLE, the part of B that
    # A's outside references reach, through B's own references, moves down to A. It must need
    # nothing above A; a unit it would take that needs more is reported and the sink refused.
    for ra, rb in getattr(A, 'SINK_WHOLE', ()):
        todo = [b for a, bs in out.items() if crate(a) == ra for b in bs
                if crate(b) == rb and rb not in closure(ra)]
        D = set()
        while todo:
            x = todo.pop()
            if x in D:
                continue
            D.add(x)
            todo += [y for y in out.get(x, ()) if crate(y) == rb]
        over = sorted({(x, y) for x in D for y in out.get(x, ())
                       if crate(y) not in closure(ra) | {ra, rb, None, NP}})
        if over:
            for x, y in over[:args.limit]:
                print(f'SINK_WHOLE {ra}<-{rb} refused: {x[0]}:{x[1]} {U[x][3]} needs '
                      f'{crate(y)} {y[0]}:{y[1]} {U[y][3]}')
            continue
        for x in D:
            moved[x] = ra
            why[x] = 'sink'
        print(f'sink whole {rb} -> {ra}: {len(D)} units, '
              f'{sum(K.nontest(x[0], U[x][0], U[x][1]) for x in D)} lines')

    # Sinking (step 5a): a helper called from below moves down to its caller's crate with the
    # part of its own crate that it reaches, when that part is functions and constants only,
    # needs nothing above the caller and is at most SINK_CAP non-test lines. Crates in NO_SINK keep their code (record: 7.3).
    # Functions and constants sink; a type sinks too when SINK_TYPES is set (see below).
    SINKABLE = ('fn', 'const', 'static')
    # a type sinks only together with every impl of it (inherent methods, trait impl blocks and
    # their methods) defined in its own crate: Rust keeps inherent impls in the type's crate
    impls_of = collections.defaultdict(list)
    blocks = {}
    for k, (tr, st_) in K.trait_impl_of.items():
        if k in U:
            blocks[k] = [x for x in K.units_in(k[0], U[k][0], U[k][1])]
            impls_of[st_].extend(blocks[k])
    for k, st_ in K.inherent.items():
        impls_of[st_].append(k)

    def companions(x):
        if U[x][2] not in ('struct', 'enum', 'union'):
            return []
        cx, stem, d = crate(x), x[0][:-3] + '/', os.path.dirname(x[0])
        return [y for y in impls_of.get(U[x][3], ()) if crate(y) == cx and (
            y[0] == x[0] or y[0].startswith(stem) or os.path.dirname(y[0]) == d)]

    cap = getattr(A, 'SINK_CAP', 0)
    nosink = set(getattr(A, 'NO_SINK', ()))
    # SINK_TYPES lets a type go down with the functions that need it, except types defined under HUBS
    sinkt = getattr(A, 'SINK_TYPES', False)
    hubs = tuple(getattr(A, 'HUBS', ()))
    sunk = 0
    while cap:
        ch = 0
        for a in list(out):
            ca = crate(a)
            if ca in (None, NP):
                continue
            for b in list(out[a]):
                cb = crate(b)
                if cb in (None, NP) or cb == ca or cb in closure(ca) or ca not in closure(cb) or cb in nosink:
                    continue
                if why.get(b) in ('item', 'type'):
                    continue
                D, todo, lines, okk = set(), [b], 0, True
                while todo and okk:
                    x = todo.pop()
                    if x in D:
                        continue
                    D.add(x)
                    todo += companions(x)
                    lines += K.nontest(x[0], U[x][0], U[x][1])
                    if lines > cap or why.get(x) in ('item', 'type') or (
                            U[x][2] not in SINKABLE and not (sinkt and not x[0].startswith(hubs))):
                        okk = False
                        break
                    for y in out.get(x, ()):
                        cy = crate(y)
                        if cy == cb:
                            todo.append(y)
                        elif cy not in closure(ca) | {ca, None, NP}:
                            okk = False
                            break
                if okk:
                    for x in D:
                        moved[x] = ca
                        why[x] = 'sink'
                        cause[x] = a
                    ch += 1
                    sunk += len(D)
        if not ch:
            break

    # a family rising into a crate in RISE_WHOLE (record: the codecs) may take its callers along
    cascade = set(getattr(A, 'RISE_WHOLE', ()))
    inn = collections.defaultdict(set)
    for a, bs in out.items():
        for b in bs:
            inn[b].add(a)
    rounds = 0
    while not args.no_rise:
        rounds += 1
        ch = 0
        for a, bs in out.items():
            u = U[a]
            if not (u[2] == 'fn' or u[3].startswith('impl:')) or why.get(a) in ('type', 'item'):
                continue
            ca = crate(a)
            if ca in (None, NP):
                continue
            tc = {crate(b) for b in bs} - {None, NP}
            if tc <= closure(ca) | {ca}:
                continue
            j = lowest(tc | {ca})
            # no cascade: a unit rises only when every caller can still see it there
            if j and j not in cascade and any(crate(c) not in (None, NP, j) and j not in closure(crate(c))
                         for c in inn.get(a, ())):
                continue
            if j and j != ca:
                cause[a] = next(b for b in bs if crate(b) not in closure(ca) | {ca, None, NP})
                moved[a] = j
                why.setdefault(a, 'rise')
                ch += 1
        if not ch or rounds > 100:
            break

    # Orphan rule (step 5b): a trait impl must sit in the crate of its trait or of its self type.
    # One that the moves left elsewhere goes, with the methods inside it, to whichever of those
    # homes sees all their targets and is seen by all their callers (the type's crate first).
    # One with no such home stays where it is and is counted below.
    tdef0 = {}
    for f, us in K.units.items():
        for u in us:
            if u[2] in ('trait', 'struct', 'enum', 'union', 'type'):
                tdef0.setdefault(u[3], set()).add(crate((f, u[0])))
    rehomed = 0
    for k, (tr, st_) in K.trait_impl_of.items():
        c = crate(k)
        if c in (None, NP) or k not in U:
            continue
        ht, hs = tdef0.get(tr, set()) - {None, NP}, tdef0.get(st_, set()) - {None, NP}
        if not hs or c in ht | hs:
            continue
        blk = [x for x in U if x[0] == k[0] and U[k][0] <= U[x][0] <= U[k][1]]
        tc = {crate(b) for x in blk for b in out.get(x, ()) if b not in blk} - {None, NP}
        cc = {crate(a) for x in blk for a in inn.get(x, ()) if a not in blk} - {None, NP}
        for h in sorted(hs) + sorted(ht):
            if (tc | hs) <= closure(h) | {h} and all(h == d or h in closure(d) for d in cc):
                for x in blk:
                    moved[x] = h
                    why[x] = 'orphan'
                rehomed += 1
                break
        else:
            # or the self type comes up to the impl's crate, when everything that names it sees it
            ts = [x for x in U if U[x][3] == st_ and U[x][2] in ('struct', 'enum', 'union')
                  and crate(x) in hs and (x[0] == k[0] or os.path.dirname(x[0]) == os.path.dirname(k[0]))]
            if len(ts) == 1:
                t = ts[0]
                grp = [t] + [y for y in impls_of.get(st_, ()) if crate(y) == crate(t) and y not in blk]
                users = {crate(a) for y in grp for a in inn.get(y, ()) if a not in grp} - {None, NP}
                needs = {crate(b) for y in grp for b in out.get(y, ()) if b not in grp} - {None, NP}
                if all(c == d or c in closure(d) for d in users) and needs <= closure(c) | {c}:
                    for y in grp:
                        moved[y] = c
                        why[y] = 'orphan'
                    rehomed += 1

    CE = collections.Counter()
    bad = collections.defaultdict(list)

    for (a, b), n in live.items():
        ca, cb = crate(a), crate(b)
        if ca in (None, NP) or cb in (None, NP) or ca == cb:
            continue
        CE[(ca, cb)] += n
        if cb not in closure(ca):
            bad[(ca, cb)].append((a, b, n))
    adj = collections.defaultdict(set)
    for (a, b) in CE:
        adj[a].add(b)
    # Tarjan SCCs
    idx, low, st, on, sccs, counter = {}, {}, [], set(), [], [0]
    sys.setrecursionlimit(10000)

    def strong(v):
        idx[v] = low[v] = counter[0]
        counter[0] += 1
        st.append(v)
        on.add(v)
        for w in adj[v]:
            if w not in idx:
                strong(w)
                low[v] = min(low[v], low[w])
            elif w in on:
                low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = st.pop()
                on.discard(w)
                comp.append(w)
                if w == v:
                    break
            if len(comp) > 1:
                sccs.append(sorted(comp))
    for v in list(adj):
        if v not in idx:
            strong(v)

    # sizes and moved lines
    size = collections.Counter()
    for f in K.files:
        size[G(f)] += K.nontest(f, 1, len(K.text[f]))
    flows = collections.defaultdict(lambda: [0, 0])
    inh = 0
    orphan = []
    for k, c in moved.items():
        u = U[k]
        n = K.nontest(k[0], u[0], u[1])
        if c == G(k[0]):
            continue
        size[G(k[0])] -= n
        size[c] += n
        if n:
            flows[(G(k[0]), c)][0] += 1
            flows[(G(k[0]), c)][1] += n
        if why.get(k) != 'type' and k in K.inherent:
            inh += 1
    # orphan rule: a trait impl must sit with its trait or its self type
    tdef = {}
    for f, us in K.units.items():
        for u in us:
            if u[2] in ('trait', 'struct', 'enum', 'union', 'type'):
                tdef.setdefault(u[3], set()).add(crate((f, u[0])))
    for k, (tr, st_) in K.trait_impl_of.items():
        c = crate(k)
        if c in (None, NP):
            continue
        homes = tdef.get(tr, set()) | tdef.get(st_, set())
        if not tdef.get(st_) and not tdef.get(tr):
            continue
        if c not in homes:
            orphan.append((k, tr, st_, c))

    total_edges = sum(E.values())
    print(f'files {len(K.files)} (test {len(K.test)}), units {len(U)}, unit edges {len(E)} '
          f'({total_edges} references), removed by design changes {dropped}')
    print(f'unassigned files: {unassigned or "none"}')
    print(f'units sunk: {sunk}; rising rounds: {rounds}')
    print(f'references outside the declared graph: {sum(n for v in bad.values() for _, _, n in v)} '
          f'in {sum(len(v) for v in bad.values())} unit edges')
    print(f'cycles (strongly connected crate sets): {sccs or "none"}')
    print(f'trait impls re-homed to their type or trait: {rehomed}; '
          f'left away from both: {len(orphan)}')
    used = collections.defaultdict(set)
    for (a, b) in CE:
        used[a].add(b)
    print('\ncrate layer lines declared-deps used-direct-deps')
    for c in sorted(DEPS, key=lambda c: (layer(c), c)):
        red = sorted(d for d in used[c] if not any(d in closure(e) for e in used[c] if e != d))
        print(f'{c:13} L{layer(c):<2} {size[c]:7}  {" ".join(sorted(DEPS[c])) or "-":28} {" ".join(red)}')
    print(f'not ported {size[NP]}; ported {sum(v for k, v in size.items() if k != NP)}')
    mv = sum(v[1] for v in flows.values())
    print(f'\nmoved non-test lines {mv} in {sum(v[0] for v in flows.values())} units; '
          f'inherent methods moved without their type {inh}')
    if args.report in ('flows', 'all'):
        for k, v in sorted(flows.items(), key=lambda x: -x[1][1]):
            print(f'  {k[0]:>13} -> {k[1]:13} {v[0]:5} {v[1]:7}')
    if args.report in ('bad', 'all'):
        for (ca, cb), lst in sorted(bad.items()):
            print(f'OUTSIDE {ca} -> {cb}: {sum(n for _, _, n in lst)}')
            for a, b, n in lst[:args.limit]:
                print(f'   {a[0]}:{a[1]} {U[a][2]} {U[a][3]} -> {b[0]}:{b[1]} {U[b][3]} x{n}')
        for k, tr, st_, c in orphan[:40]:
            print(f'ORPHAN {k[0]}:{k[1]} impl {tr} for {st_} in {c}')
    if args.explain:
        fa, fb = args.explain.split(':')
        roots = collections.Counter()
        for k, c in moved.items():
            if G(k[0]) == fa and c == fb:
                seen = set()
                x = k
                while x in cause and x not in seen:
                    seen.add(x)
                    x = cause[x]
                roots[(x, crate(x))] += K.nontest(k[0], U[k][0], U[k][1])
        for (x, c), n in roots.most_common(30):
            print(f'ROOT {n:6} {x[0]}:{x[1]} {U[x][2]} {U[x][3]} [{G(x[0])}->{c}]')
    if args.needs:
        tot = collections.Counter()
        for k in U:
            if k[0].startswith(args.needs) and k[0] not in K.test:
                for b in out.get(k, ()):
                    if crate(b) not in (None, NP) and not b[0].startswith(args.needs):
                        tot[(crate(k), crate(b), b[0])] += 1
        lines = sum(K.nontest(k[0], U[k][0], U[k][1]) for k in U if k[0].startswith(args.needs))
        print(f'NEEDS {args.needs}: {lines} lines')
        for (c, d, f), n in sorted(tot.items()):
            print(f'  {c} -> {d} {f} x{n}')
    if args.chain:
        f, n = args.chain.rsplit(':', 1)
        x, seen = (f, int(n)), set()
        while x and x not in seen:
            seen.add(x)
            print(f'CHAIN {x[0]}:{x[1]} {U[x][2]} {U[x][3]} {G(x[0])}->{crate(x)} {why.get(x)}')
            x = cause.get(x)
    if args.reach:
        ra, rb = args.reach.split(':')
        todo = [b for _, b, _ in bad.get((ra, rb), [])]
        D = set()
        while todo:
            x = todo.pop()
            if x in D:
                continue
            D.add(x)
            todo += [y for y in out.get(x, ()) if crate(y) == rb]
        byf = collections.Counter()
        for x in D:
            byf[x[0]] += K.nontest(x[0], U[x][0], U[x][1])
        print(f'REACH {ra} -> {rb}: {len(D)} units, {sum(byf.values())} lines of {size[rb]}')
        for f, n in byf.most_common(args.limit):
            print(f'  {n:6} {f}')
    if args.report in ('units', 'all'):
        for k, c in sorted(moved.items()):
            if c != G(k[0]):
                u = U[k]
                tg = sorted({crate(b) for b in out.get(k, ())} - {c, None})
                print(f'MOVED {G(k[0])}->{c} {why.get(k)} {k[0]}:{u[0]} {u[2]} {u[3]} '
                      f'{K.nontest(k[0], u[0], u[1])} targets={",".join(tg)}')


if __name__ == '__main__':
    main()
