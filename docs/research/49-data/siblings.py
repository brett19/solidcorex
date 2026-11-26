#!/usr/bin/env python3
"""Report 49, part 5: which crates outside the kernel crate name which kernel areas.
python3 -I siblings.py REPO_ROOT DATA_DIR
Counts every `solidcore::a::b` path in code (each leaf of a `use solidcore::{..}` tree and each expression path),
with comments and strings blanked, as xtask/src/solidcore_paths.rs does.  Writes sibling_refs.tsv (one row per path)
and prints per-crate, per-area tables.  Root-level names (`solidcore::Kernel`) are attributed to the area
that defines them, using lib.rs's `pub use` rows from refs.tsv.
"""
import os, re, sys, csv, collections, json

REPO, D = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
BLANK = re.compile(r'[^\n]')
CHAR_RE = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^\\'\n])'")
RAW_RE = re.compile(r'b?r(#*)"')
NONWORD = lambda c: not (c.isalnum() or c == '_')


def clean(t):
    out = []; i, n = 0, len(t)
    while i < n:
        c = t[i]
        if c == '/' and t.startswith('//', i):
            j = t.find('\n', i); j = n if j < 0 else j; out.append(' ' * (j - i)); i = j
        elif c == '/' and t.startswith('/*', i):
            d, j = 1, i + 2
            while j < n and d:
                if t.startswith('/*', j): d += 1; j += 2
                elif t.startswith('*/', j): d -= 1; j += 2
                else: j += 1
            out.append(BLANK.sub(' ', t[i:j])); i = j
        elif c in 'br' and (i == 0 or NONWORD(t[i - 1])) and RAW_RE.match(t, i):
            m = RAW_RE.match(t, i); end = '"' + m.group(1)
            j = t.find(end, m.end()); j = n if j < 0 else j + len(end)
            out.append(BLANK.sub(' ', t[i:j])); i = j
        elif c == '"' or (c == 'b' and t.startswith('b"', i) and (i == 0 or NONWORD(t[i - 1]))):
            j = i + (2 if c == 'b' else 1)
            while j < n and t[j] != '"': j += 2 if t[j] == '\\' else 1
            j = min(j + 1, n); out.append(BLANK.sub(' ', t[i:j])); i = j
        elif c == "'":
            m = CHAR_RE.match(t, i)
            if m: out.append(' ' * (m.end() - i)); i = m.end()
            else: out.append(c); i += 1
        else:
            out.append(c); i += 1
    return ''.join(out)


TOK = re.compile(r"r#[A-Za-z_]\w*|[A-Za-z_]\w*|\d\w*|::|\S")


def toks_of(t):
    ts, ls = [], []; line = 1; pos = 0
    for m in TOK.finditer(t):
        line += t.count('\n', pos, m.start()); pos = m.start()
        g = m.group(); ts.append(g[2:] if g.startswith('r#') else g); ls.append(line)
    return ts, ls


def use_tree(ts, i, prefix):
    leaves = []; segs = list(prefix); n = len(ts)
    while i < n:
        t = ts[i]
        if t == '{':
            i += 1
            while i < n and ts[i] != '}':
                sub, i = use_tree(ts, i, segs); leaves += sub
                if i < n and ts[i] == ',': i += 1
            return leaves, i + 1
        if t == '*': leaves.append((segs + ['*'])); return leaves, i + 1
        if t == '::': i += 1; continue
        if re.match(r'[A-Za-z_]', t):
            segs.append(t); i += 1
            if i < n and ts[i] == 'as': leaves.append(list(segs)); return leaves, i + 2
            if i < n and ts[i] == '::': continue
            leaves.append(list(segs)); return leaves, i
        break
    return leaves, i


def paths_in(text, idents):
    ct = clean(text); ts, ls = toks_of(ct); n = len(ts); out = []; i = 0
    while i < n:
        t = ts[i]
        if t == 'use' and (i == 0 or ts[i - 1] in (';', '{', '}', ']', 'pub', ')')):
            j = i + 1
            if j < n and ts[j] == '::': j += 1
            if j < n and ts[j] in idents:
                leaves, j2 = use_tree(ts, j, [])
                for lf in leaves:
                    if lf[-1] == 'self': lf = lf[:-1]
                    out.append((ls[i], lf, 'use'))
            while i < n and ts[i] != ';': i += 1
            i += 1; continue
        if t in idents and (i == 0 or ts[i - 1] not in ('::', '.')) and i + 1 < n and ts[i + 1] == '::':
            segs = [t]; j = i + 1
            while j + 1 < n and ts[j] == '::' and re.match(r'[A-Za-z_]', ts[j + 1]): segs.append(ts[j + 1]); j += 2
            out.append((ls[i], segs, 'expr')); i = j; continue
        i += 1
    return out


def main():
    # root-name -> (area, item) from lib.rs rows
    rootmap = {}
    for r in csv.DictReader(open(os.path.join(D, 'refs.tsv')), delimiter='\t', quoting=csv.QUOTE_NONE):
        if r['file'] == 'lib.rs' and r['ref_kind'] == 'use' and r['local']:
            rootmap[r['local']] = (r['tgt_mod'].split('::')[0], r['item'])
    meta = json.load(open(os.path.join(D, 'meta.json')))
    areas = {m.split('::')[0] for m in meta['modules'] if m}
    crates = []
    for base in ('solidcore/crates', 'crates'):
        for name in sorted(os.listdir(os.path.join(REPO, base))):
            p = os.path.join(base, name)
            if p == 'solidcore/crates/solidcore' or not os.path.isdir(os.path.join(REPO, p)): continue
            crates.append(p)
    rows = []
    for c in crates:
        for dp, dn, fn in os.walk(os.path.join(REPO, c)):
            dn[:] = [d for d in dn if d not in ('target', '.git', 'node_modules')]
            for f in fn:
                if not f.endswith('.rs'): continue
                full = os.path.join(dp, f); rel = os.path.relpath(full, REPO)
                text = open(full, encoding='utf-8', errors='replace').read()
                if 'solidcore' not in text: continue
                part = rel.split('/')
                sub = part[part.index(os.path.basename(c)) + 1] if len(part) > part.index(os.path.basename(c)) + 1 else ''
                kind = 'src' if sub == 'src' else ('tests/benches/examples' if sub in ('tests', 'benches', 'examples') else sub or 'other')
                # in src, drop everything after a column-0 #[cfg(test)] (approximation)
                cutline = 10**9
                if kind == 'src':
                    k = text.find('\n#[cfg(test)]')
                    if k >= 0: cutline = text.count('\n', 0, k) + 2
                for line, segs, how in paths_in(text, {'solidcore'}):
                    rest = segs[1:]
                    if not rest: continue
                    if rest[0] in rootmap: area, item = rootmap[rest[0]]; depth_class = 'root re-export'
                    elif rest[0] in areas: area, item = rest[0], '::'.join(rest[1:]) or '(module)'; depth_class = 'module path'
                    elif len(rest) == 1: area, item = '(root)', rest[0]; depth_class = 'root re-export'
                    else: area, item = '(unknown)', '::'.join(rest); depth_class = 'module path'
                    internal = any(re.fullmatch(r'p\d+', s) for s in rest) or len(rest) > 4
                    rows.append((c, rel, line, kind, how, area, '::'.join(rest), depth_class, int(internal), int(line >= cutline)))
    with open(os.path.join(D, 'sibling_refs.tsv'), 'w') as f:
        f.write('crate\tfile\tline\tpart\tform\tarea\tpath_after_solidcore\tdepth_class\tnumbered_or_deep\tin_cfgtest_tail\n')
        for r in rows: f.write('\t'.join(map(str, r)) + '\n')
    # tables
    by = collections.defaultdict(lambda: collections.Counter())
    files = collections.defaultdict(set); items = collections.defaultdict(set)
    tot = collections.defaultdict(lambda: collections.Counter())
    for c, rel, line, kind, how, area, path, dc, internal, tail in rows:
        by[c][area] += 1; files[c].add(rel); items[(c, area)].add(path)
        tot[c]['all'] += 1; tot[c][kind] += 1; tot[c]['tail'] += tail
        tot[c]['root re-export' if dc == 'root re-export' else 'module path'] += 1
        tot[c]['internal'] += internal
    print('crate | paths | src | tests/benches/examples | root-name | module-path | numbered-or-deep | files | in cfg(test) tail')
    for c in sorted(tot, key=lambda c: -tot[c]['all']):
        t = tot[c]
        print(f"{c} | {t['all']} | {t['src']} | {t['tests/benches/examples']} | {t['root re-export']} | {t['module path']} | {t['internal']} | {len(files[c])} | {t['tail']}")
    print()
    for c in sorted(tot, key=lambda c: -tot[c]['all']):
        print(c, 'areas:', ', '.join(f"{a} {n}({len(items[(c, a)])})" for a, n in by[c].most_common()))
    # area totals
    print()
    ar = collections.Counter(); arc = collections.defaultdict(set)
    for c, rel, line, kind, how, area, path, dc, internal, tail in rows:
        ar[area] += 1; arc[area].add(c)
    print('kernel area | paths | crates')
    for a, n in ar.most_common(): print(f"{a} | {n} | {len(arc[a])}")


main()


def facade_counts():
    ids = ['solidcore_ops', 'solidcore_tess', 'solidcore_record', 'solidcore_step', 'solidcore_cadir', 'solidcore_convert']
    res = collections.defaultdict(collections.Counter)
    for name in sorted(os.listdir(os.path.join(REPO, 'crates'))):
        base = os.path.join(REPO, 'crates', name)
        for dp, dn, fn in os.walk(base):
            dn[:] = [d for d in dn if d not in ('target',)]
            for f in fn:
                if f.endswith('.rs'):
                    text = open(os.path.join(dp, f), encoding='utf-8', errors='replace').read()
                    if 'solidcore_' not in text: continue
                    for line, segs, how in paths_in(text, set(ids)):
                        if len(segs) > 1: res[name][segs[0]] += 1
    print('\napp crates: paths into facade crates')
    for c, cn in sorted(res.items(), key=lambda kv: -sum(kv[1].values())):
        print(c, dict(cn))


facade_counts()
