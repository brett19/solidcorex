#!/usr/bin/env python3
"""Module dependency map of the solidcore kernel crate (report 49), step 1: extraction.

Run:  python3 -I depmap.py SRC_DIR OUT_DIR
Reads every .rs file reachable from lib.rs through `mod` declarations (honouring #[path]),
tokenises it with comments, strings and char literals blanked, skips test-only items
(any item carrying #[cfg(test)] / #[test], at any depth; files declared under such a mod; files
with an inner #![cfg(test)]), and extracts every path reference into the crate: `use` trees
(resolved through `pub use` chains and globs to the DEFINING module), `crate::` / `super::` /
`self::` paths in expressions and types, and `alias::item` paths where `alias` is an imported
module.  Writes refs.tsv (one row per reference), name_uses.tsv (one row per use of an imported
name, with syntactic context) and meta.json.  analyse.py reads these.
"""
import os, re, sys, json, collections

SRC = os.path.abspath(sys.argv[1])
OUT = os.path.abspath(sys.argv[2])
os.makedirs(OUT, exist_ok=True)

IDENT = re.compile(r'[A-Za-z_]')
NONWORD = lambda c: not (c.isalnum() or c == '_')
CHAR_RE = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^\\'\n])'")
RAW_RE = re.compile(r'b?r(#*)"')
BLANK = re.compile(r'[^\n]')


def clean(t):
    """Blank comments, strings, char literals; keep newlines and offsets."""
    out = []
    i, n = 0, len(t)
    while i < n:
        c = t[i]
        if c == '/' and i + 1 < n and t[i + 1] == '/':
            j = t.find('\n', i); j = n if j < 0 else j
            out.append(' ' * (j - i)); i = j
        elif c == '/' and i + 1 < n and t[i + 1] == '*':
            depth, j = 1, i + 2
            while j < n and depth:
                if t.startswith('/*', j): depth += 1; j += 2
                elif t.startswith('*/', j): depth -= 1; j += 2
                else: j += 1
            out.append(BLANK.sub(' ', t[i:j])); i = j
        elif c in 'br' and (i == 0 or NONWORD(t[i - 1])) and RAW_RE.match(t, i):
            m = RAW_RE.match(t, i)
            end = '"' + m.group(1)
            j = t.find(end, m.end()); j = n if j < 0 else j + len(end)
            out.append(BLANK.sub(' ', t[i:j])); i = j
        elif c == '"' or (c == 'b' and i + 1 < n and t[i + 1] == '"' and (i == 0 or NONWORD(t[i - 1]))):
            j = i + (2 if c == 'b' else 1)
            while j < n and t[j] != '"':
                j += 2 if t[j] == '\\' else 1
            j = min(j + 1, n)
            out.append(BLANK.sub(' ', t[i:j])); i = j
        elif c == "'":
            m = CHAR_RE.match(t, i)
            if m:
                out.append(' ' * (m.end() - i)); i = m.end()
            else:
                out.append(c); i += 1
        else:
            out.append(c); i += 1
    return ''.join(out)


TOK_RE = re.compile(r"r#[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*|\d[\w]*|::|=>|->|\S")
ID_MACROS = {'typed_id', 'props_id', 'arena_id', 'proto_id', 'poly_id'}


def tokenize(ct):
    toks, lines = [], []
    line, pos = 1, 0
    for m in TOK_RE.finditer(ct):
        line += ct.count('\n', pos, m.start()); pos = m.start()
        g = m.group()
        toks.append(g[2:] if g.startswith('r#') else g); lines.append(line)
    return toks, lines


ITEM_KW = {'fn', 'struct', 'enum', 'union', 'trait', 'type', 'impl', 'mod', 'use', 'const', 'static',
           'pub', 'unsafe', 'extern', 'async', 'macro_rules', 'macro'}
NOT_PATH = {'std', 'core', 'alloc', 'f64', 'f32', 'i8', 'i16', 'i32', 'i64', 'i128', 'u8', 'u16',
            'u32', 'u64', 'u128', 'usize', 'isize', 'str', 'bool', 'char', 'mem', 'cmp', 'fmt',
            'iter', 'slice', 'vec', 'option', 'result', 'string', 'collections', 'ops', 'ptr',
            'array', 'hash', 'rc', 'sync', 'cell', 'borrow', 'convert', 'default', 'any'}
# context codes
C_BODY, C_PARAM, C_RET, C_FIELD, C_IMPL_TRAIT, C_IMPL_FOR, C_DECL, C_USE, C_SIGOTHER = 0, 1, 2, 3, 4, 5, 6, 7, 8

mods = {}
files_info = {}
raw_refs = []
name_use_cands = []


def new_mod(path, file, test):
    return mods.setdefault(path, dict(file=file, test=test, children={}, defs={}, globs=[],
                                      aliases=collections.defaultdict(list)))


def parse_use_tree(toks, i, prefix):
    leaves = []
    segs = list(prefix)
    n = len(toks)
    while i < n:
        t = toks[i]
        if t == '{':
            i += 1
            while i < n and toks[i] != '}':
                sub, i = parse_use_tree(toks, i, segs)
                leaves += sub
                if i < n and toks[i] == ',': i += 1
            return leaves, i + 1
        if t == '*':
            leaves.append((segs, None, True)); return leaves, i + 1
        if t == '::':
            i += 1; continue
        if IDENT.match(t):
            segs.append(t); i += 1
            if i < n and toks[i] == 'as':
                leaves.append((segs, toks[i + 1], False)); return leaves, i + 2
            if i < n and toks[i] == '::': continue
            leaves.append((segs, None, False)); return leaves, i
        break
    return leaves, i


def queue_child(queue, rel, fdir, child_dir, parent_path, name, path_attr, test):
    cands = []
    if path_attr:
        cands.append(os.path.normpath(os.path.join(fdir, path_attr)))
    else:
        cands += [os.path.join(child_dir, name + '.rs'), os.path.join(child_dir, name, 'mod.rs')]
    queue.append((cands, parent_path + (name,), test, rel))


def attr_end(toks, j, n):
    d = 0
    while j < n:
        if toks[j] == '[': d += 1
        elif toks[j] == ']':
            d -= 1
            if d == 0: return j
        j += 1
    return n - 1


def skip_item(toks, n, i0):
    """i0 at an attribute token; skip attributes and the item after them; return next index."""
    j = i0
    while j < n and toks[j] == '#':
        j += 1
        if j < n and toks[j] == '!': j += 1
        if j < n and toks[j] == '[': j = attr_end(toks, j, n) + 1
    k = j
    while k < n and toks[k] == 'pub':
        k += 1
        if k < n and toks[k] == '(':
            d = 0
            while k < n:
                if toks[k] == '(': d += 1
                elif toks[k] == ')':
                    d -= 1
                    if d == 0: k += 1; break
                k += 1
    itemlike = k < n and toks[k] in ITEM_KW
    d = 0
    while j < n:
        t = toks[j]
        if t in '([': d += 1
        elif t in ')]':
            d -= 1
            if d < 0: return j
        elif t == '{':
            bd = 0
            while j < n:
                if toks[j] == '{': bd += 1
                elif toks[j] == '}':
                    bd -= 1
                    if bd == 0: break
                j += 1
            if itemlike: return j + 1
        elif t == '}':
            if d == 0: return j
        elif t == ';' and d == 0:
            return j + 1
        elif t == ',' and d == 0 and not itemlike:
            return j + 1
        j += 1
    return j


def parse_file(rel, modpath, is_test_mod, queue):
    text = open(os.path.join(SRC, rel), encoding='utf-8').read()
    src_lines = text.splitlines()
    nlines = len(src_lines)
    toks, tl = tokenize(clean(text))
    n = len(toks)
    info = dict(rel=rel, lines=nlines, skipped=0, fns=0, test_file=is_test_mod, mod=modpath, after=0)
    files_info[rel] = info
    fdir = os.path.dirname(rel)
    stem = os.path.splitext(os.path.basename(rel))[0]
    child_dir = fdir if os.path.basename(rel) in ('lib.rs', 'mod.rs') else os.path.join(fdir, stem)

    for k in range(min(n - 4, 600)):
        if toks[k] == '#' and toks[k + 1] == '!' and toks[k + 2] == '[' and toks[k + 3] == 'cfg':
            a = toks[k + 4:attr_end(toks, k + 2, n)]
            if 'test' in a and 'not' not in a: info['test_file'] = True
    if info['test_file']:
        info['skipped'] = nlines
        new_mod(modpath, rel, True)
        for k in range(n - 2):
            if toks[k] == 'mod' and toks[k + 2] == ';' and IDENT.match(toks[k + 1]):
                queue_child(queue, rel, fdir, child_dir, modpath, toks[k + 1], None, True)
        return

    m_here = new_mod(modpath, rel, False)
    stack = []            # ('mod', parent_path) | ('other', kind)
    cur_path = modpath
    pending_skip = False
    pending_path = None
    skip_spans = []
    use_names = set()
    fn_sig = None
    hdr = None
    ctx = [0] * n
    first_cfgtest_line = None
    i = 0

    def top_kind():
        return stack[-1][1] if stack and stack[-1][0] == 'other' else None

    while i < n:
        t = toks[i]
        # ---- attributes
        if t == '#' and i + 1 < n and (toks[i + 1] == '[' or (toks[i + 1] == '!' and i + 2 < n and toks[i + 2] == '[')):
            j = i + 1 + (1 if toks[i + 1] == '!' else 0)
            e = attr_end(toks, j, n)
            attr = toks[j + 1:e]
            if attr and attr[0] == 'cfg' and 'test' in attr and 'not' not in attr:
                pending_skip = True
                if first_cfgtest_line is None: first_cfgtest_line = tl[i]
            elif attr == ['test']:
                pending_skip = True
            elif attr and attr[0] == 'path':
                mm = re.search(r'#\[path\s*=\s*"([^"]+)"\]', src_lines[tl[i] - 1])
                pending_path = mm.group(1) if mm else None
            if pending_skip:
                end = skip_item(toks, n, i)
                skip_spans.append((tl[i], tl[min(end, n) - 1]))
                seg = toks[e + 1:min(end, n)]
                # a path attribute may sit after the cfg(test) attribute
                pp = pending_path
                for q in range(len(seg) - 1):
                    if seg[q] == '[' and seg[q + 1] == 'path':
                        ln = tl[e + 1 + q]
                        mm = re.search(r'#\[path\s*=\s*"([^"]+)"\]', src_lines[ln - 1])
                        if mm: pp = mm.group(1)
                if 'mod' in seg[:10]:
                    mi = seg.index('mod')
                    if mi + 2 < len(seg) and seg[mi + 2] == ';':
                        queue_child(queue, rel, fdir, child_dir, cur_path, seg[mi + 1], pp, True)
                pending_skip = False; pending_path = None
                i = end
                continue
            i = e + 1
            continue
        # ---- mod
        if t == 'mod' and i + 2 < n and IDENT.match(toks[i + 1]) and (i == 0 or toks[i - 1] not in ('.', '::')):
            name = toks[i + 1]
            if toks[i + 2] == ';':
                mods[cur_path]['children'][name] = cur_path + (name,)
                queue_child(queue, rel, fdir, child_dir, cur_path, name, pending_path, False)
                pending_path = None
                i += 3; continue
            if toks[i + 2] == '{':
                cp = cur_path + (name,)
                new_mod(cp, rel, False)
                mods[cur_path]['children'][name] = cp
                stack.append(('mod', cur_path))
                cur_path = cp
                i += 3; continue
        # ---- use
        if t == 'use' and (i == 0 or toks[i - 1] in (';', '{', '}', ']', 'pub', ')')) and i + 1 < n:
            vis_pub = i > 0 and toks[i - 1] in ('pub', ')')
            ln = tl[i]
            j = i + 1
            if toks[j] == '::': j += 1
            leaves, j2 = parse_use_tree(toks, j, [])
            while j2 < n and toks[j2] != ';': j2 += 1
            for segs, alias, glob in leaves:
                segs = list(segs)
                if glob:
                    mods[cur_path]['globs'].append(segs)
                    raw_refs.append(dict(file=rel, line=ln, mod=cur_path, segs=segs, kind='use', glob=True, pub=vis_pub, local=None))
                    continue
                if segs and segs[-1] == 'self': segs = segs[:-1]
                if not segs: continue
                local = alias or segs[-1]
                if local == '_': local = None
                if local:
                    mods[cur_path]['aliases'][local].append(segs)
                    use_names.add(local)
                raw_refs.append(dict(file=rel, line=ln, mod=cur_path, segs=segs, kind='use', glob=False, pub=vis_pub, local=local))
            for k in range(i, min(j2 + 1, n)): ctx[k] = C_USE
            i = j2 + 1
            continue
        # ---- braces
        if t == '{':
            kind = 'other'
            if fn_sig is not None and fn_sig['paren'] == 0:
                kind = 'fn'; fn_sig = None
            elif hdr and hdr[0] in ('struct', 'enum', 'union', 'impl'):
                kind = hdr[0]
            if hdr and hdr[0] in ('struct', 'enum', 'union', 'impl'): hdr = None
            stack.append(('other', kind))
            ctx[i] = C_BODY
            i += 1; continue
        if t == '}':
            if stack:
                e = stack.pop()
                if e[0] == 'mod': cur_path = e[1]
            i += 1; continue
        in_mod_level = (not stack) or stack[-1][0] == 'mod'
        isid = i + 1 < n and IDENT.match(toks[i + 1]) and (i == 0 or toks[i - 1] not in ('.', '::'))
        if in_mod_level and t in ('struct', 'enum', 'union', 'trait', 'type', 'fn', 'const', 'static') and isid:
            nm = toks[i + 1]
            if not (t == 'const' and nm in ('fn', 'unsafe', 'async', 'extern')):
                mods[cur_path]['defs'].setdefault(nm, t)
        if in_mod_level and t in ID_MACROS and i + 2 < n and toks[i + 1] == '!' and toks[i + 2] in '({':
            for q in range(i + 3, min(i + 8, n)):
                if IDENT.match(toks[q]) and toks[q][0].isupper():
                    mods[cur_path]['defs'].setdefault(toks[q], 'struct'); break
        if t == 'macro_rules' and i + 2 < n and toks[i + 1] == '!':
            mods[cur_path]['defs'].setdefault(toks[i + 2], 'macro')
        if t == 'fn' and isid:
            info['fns'] += 1
            fn_sig = dict(paren=0, arrow=False)
        elif t in ('struct', 'enum', 'union') and isid:
            hdr = (t,)
        elif t == 'impl' and (i == 0 or toks[i - 1] not in ('.', '::', '(', '<', ',', '&', '->', ':', '+')):
            hdr = ('impl', False)
        elif t in ('type', 'const', 'static') and in_mod_level and isid and toks[i + 1] not in ('fn', 'unsafe', 'async', 'extern'):
            hdr = ('decl',)
        if hdr and hdr[0] == 'impl' and t == 'for': hdr = ('impl', True)
        if t == ';':
            if fn_sig is not None and fn_sig['paren'] == 0: fn_sig = None
            if hdr and hdr[0] in ('struct', 'decl', 'union'): hdr = None
        if fn_sig is not None:
            if t in '([': fn_sig['paren'] += 1
            elif t in ')]': fn_sig['paren'] -= 1
            elif t == '->' and fn_sig['paren'] == 0: fn_sig['arrow'] = True
            ctx[i] = C_RET if fn_sig['arrow'] else (C_PARAM if fn_sig['paren'] > 0 else C_SIGOTHER)
        elif hdr and hdr[0] == 'impl':
            ctx[i] = C_IMPL_FOR if hdr[1] else C_IMPL_TRAIT
        elif hdr and hdr[0] in ('struct', 'union', 'enum'):
            ctx[i] = C_FIELD
        elif hdr and hdr[0] == 'decl':
            ctx[i] = C_DECL
        elif top_kind() in ('struct', 'enum', 'union'):
            ctx[i] = C_FIELD
        else:
            ctx[i] = C_BODY
        # ---- path chains
        if (IDENT.match(t)) and (i == 0 or toks[i - 1] not in ('::', '.', '$') or (toks[i - 1] == '.' and i >= 2 and toks[i - 2] == '.')) and i + 1 < n and toks[i + 1] == '::':
            if t in ('crate', 'super', 'self') or (t[0].islower() and t not in NOT_PATH):
                segs = [t]
                j = i + 1
                while j + 1 < n and toks[j] == '::' and IDENT.match(toks[j + 1]):
                    segs.append(toks[j + 1]); j += 2
                if not (t == 'self' and len(segs) == 1):
                    raw_refs.append(dict(file=rel, line=tl[i], mod=cur_path, segs=segs, kind='expr', glob=False, pub=False,
                                         local=None, ctx=ctx[i], nxt=toks[j] if j < n else ''))
        i += 1

    for k in range(n - 3):
        if toks[k] == '$' and toks[k + 1] == 'crate' and toks[k + 2] == '::':
            segs = ['crate']; j = k + 2
            while j + 1 < n and toks[j] == '::' and IDENT.match(toks[j + 1]):
                segs.append(toks[j + 1]); j += 2
            raw_refs.append(dict(file=rel, line=tl[k], mod=modpath, segs=segs, kind='expr', glob=False, pub=False,
                                 local=None, ctx=0, nxt='', macro=True))
    for k in range(n):
        t = toks[k]
        if t in use_names and ctx[k] != C_USE and (k == 0 or toks[k - 1] not in ('::', '.', '$', 'fn')):
            name_use_cands.append((rel, modpath, t, tl[k], ctx[k], toks[k - 1] if k else '',
                                   toks[k + 1] if k + 1 < n else '', toks[k + 2] if k + 2 < n else ''))

    covered = set()
    for a, b in skip_spans:
        covered.update(range(a, b + 1))
    info['skipped'] = len(covered)
    if first_cfgtest_line is not None:
        info['after'] = sum(1 for ln in range(first_cfgtest_line, nlines + 1)
                            if ln not in covered and src_lines[ln - 1].strip() and not src_lines[ln - 1].strip().startswith('//'))


def main():
    queue = [(['lib.rs'], (), False, None)]
    seen = {}
    unresolved_files = []
    while queue:
        cands, mp, test, parent = queue.pop(0)
        rel = next((c for c in cands if os.path.isfile(os.path.join(SRC, c))), None)
        if rel is None:
            unresolved_files.append((parent, cands)); continue
        if rel in seen: continue
        seen[rel] = mp
        parse_file(rel, mp, test, queue)

    all_files = sorted(os.path.relpath(os.path.join(dp, f), SRC) for dp, dn, fn in os.walk(SRC) for f in fn if f.endswith('.rs'))
    orphans = sorted(set(all_files) - set(files_info))

    ROOT = ()
    if ('kernel',) in mods: mods[('kernel',)]['defs'].setdefault('UpstreamFixes', 'struct')  # defined by macro upstream_fixes!

    def lookup_name(M, name, visited, depth=0):
        key = (M, name)
        if key in visited or depth > 25: return None
        visited.add(key)
        m = mods.get(M)
        if m is None: return None
        if name in m['children']: return ('mod', m['children'][name])
        if name in m['defs']: return ('item', M, name, m['defs'][name])
        for segs in m['aliases'].get(name, []):
            r = resolve_raw(M, segs, visited, depth + 1)
            if r is not None: return r
        for gsegs in m['globs']:
            r = resolve_raw(M, gsegs, visited, depth + 1)
            if r is not None and r[0] == 'mod':
                r2 = lookup_name(r[1], name, visited, depth + 1)
                if r2 is not None: return r2
        return None

    def walk(cur, segs, visited, depth):
        i = 0
        while i < len(segs):
            r = lookup_name(cur, segs[i], set(visited), depth)
            if r is None: return ('partial', cur, segs[i:])
            if r[0] == 'mod': cur = r[1]; i += 1
            else: return ('item', r[1], r[2], r[3])
        return ('mod', cur)

    def resolve_raw(M, segs, visited=None, depth=0):
        visited = visited if visited is not None else set()
        if not segs: return None
        first = segs[0]
        if first == 'crate': r = walk(ROOT, segs[1:], visited, depth)
        elif first == 'self': r = walk(M, segs[1:], visited, depth)
        elif first == 'super':
            cur = M; k = 0
            while k < len(segs) and segs[k] == 'super':
                cur = cur[:-1]; k += 1
            r = walk(cur, segs[k:], visited, depth)
        else:
            if lookup_name(M, first, set(visited), depth) is None: return None
            r = walk(M, segs, visited, depth)
        if r is None or r[0] == 'partial': return None
        return r

    stats = collections.Counter()
    unresolved = []
    use_rows = [r for r in raw_refs if r['kind'] == 'use']
    expr_rows = [r for r in raw_refs if r['kind'] == 'expr']
    alias_mods = collections.defaultdict(dict)
    resolved_use = []
    for r in use_rows:
        M, segs = r['mod'], r['segs']
        first = segs[0]
        if first in ('std', 'core', 'alloc'):
            stats['std_use'] += 1; continue
        if first not in ('crate', 'self', 'super') and lookup_name(M, first, set()) is None:
            stats['extern_or_unknown_use'] += 1; continue
        res = resolve_raw(M, segs)
        if res is None:
            unresolved.append((r['file'], r['line'], '::'.join(segs)))
            if first == 'crate' and len(segs) > 1:
                res = ('mod', (segs[1],)); stats['use_textual_fallback'] += 1
            else:
                stats['use_unresolved'] += 1; continue
        r['res'] = res
        resolved_use.append(r)
        if res[0] == 'mod' and r['local'] and not r['glob']:
            alias_mods[r['file']][r['local']] = res[1]
    expansions = collections.Counter()
    resolved_expr = []
    for r in expr_rows:
        M, segs = r['mod'], r['segs']
        first = segs[0]
        if first in ('crate', 'super', 'self'):
            res = resolve_raw(M, segs)
            if res is None:
                unresolved.append((r['file'], r['line'], '::'.join(segs)))
                if first == 'crate' and len(segs) > 1:
                    res = ('mod', (segs[1],)); stats['expr_textual_fallback'] += 1
                else:
                    stats['expr_unresolved'] += 1; continue
            r['res'] = res; resolved_expr.append(r)
        elif first in alias_mods[r['file']]:
            res = walk(alias_mods[r['file']][first], segs[1:], set(), 0)
            if res[0] == 'partial': stats['alias_partial'] += 1; continue
            r['res'] = res; r['via'] = 'alias'; resolved_expr.append(r)
            expansions[(r['file'], first)] += 1
        elif first in mods[M]['children']:
            res = walk(mods[M]['children'][first], segs[1:], set(), 0)
            if res[0] == 'partial': continue
            r['res'] = res; r['via'] = 'child'; resolved_expr.append(r)
    rows = []
    for r in resolved_use + resolved_expr:
        res = r['res']
        r['dropped'] = bool(r['kind'] == 'use' and res[0] == 'mod' and r['local'] and not r['glob']
                            and expansions.get((r['file'], r['local']), 0) > 0)
        item = res[2] if res[0] == 'item' else ('*' if r['glob'] else '(mod)')
        kind = res[3] if res[0] == 'item' else ('glob' if r['glob'] else 'mod')
        rows.append([r['file'], r['line'], '::'.join(r['mod']), '::'.join(res[1]), item, kind, r['kind'], int(r['pub']),
                     int(r['glob']), int(r['dropped']), r.get('ctx', C_USE), r.get('nxt', ''), '::'.join(r['segs']),
                     r['segs'][1] if r['segs'][0] == 'crate' and len(r['segs']) > 1 else '', r.get('local') or '',
                     r.get('via', '')])
    stats['modimports_dropped_as_expanded'] = sum(1 for x in rows if x[9])
    with open(os.path.join(OUT, 'refs.tsv'), 'w') as f:
        f.write('\t'.join(['file', 'line', 'src_mod', 'tgt_mod', 'item', 'item_kind', 'ref_kind', 'pub_use', 'glob', 'dropped',
                           'ctx', 'next', 'text', 'textual_area', 'local', 'via']) + '\n')
        for x in rows: f.write('\t'.join(map(str, x)) + '\n')

    leaf_by_file = collections.defaultdict(dict)
    for r in resolved_use:
        if r['local'] and not r['glob'] and r['res'][0] == 'item':
            leaf_by_file[r['file']].setdefault(r['local'], r['res'])
    nu = 0
    with open(os.path.join(OUT, 'name_uses.tsv'), 'w') as f:
        f.write('\t'.join(['file', 'line', 'src_mod', 'name', 'tgt_mod', 'item', 'item_kind', 'ctx', 'prev', 'next', 'next2']) + '\n')
        for (rel, mp, name, line, c, pv, nx, nx2) in name_use_cands:
            res = leaf_by_file[rel].get(name)
            if not res: continue
            if res[3] == 'fn' and nx != '(' and nx != '::' : continue
            if res[3] in ('const', 'static') and not name.isupper(): continue
            f.write('\t'.join(map(str, [rel, line, '::'.join(mp), name, '::'.join(res[1]), res[2], res[3], c, pv, nx, nx2])) + '\n'); nu += 1

    meta = dict(stats=dict(stats), orphans=orphans, unresolved_mod_files=[(a, b) for a, b in unresolved_files],
                n_files_parsed=len(files_info), n_files_total=len(all_files), n_refs=len(rows), n_name_uses=nu,
                n_unresolved=len(unresolved), unresolved=unresolved[:300],
                files={rel: dict(lines=i['lines'], skipped=i['skipped'], fns=i['fns'], test_file=i['test_file'],
                                 mod='::'.join(i['mod']), after_first_cfgtest_nontest=i['after']) for rel, i in files_info.items()},
                modules={'::'.join(p): dict(file=m['file'], test=m['test'], defs=m['defs']) for p, m in mods.items()},
                globs=[dict(file=r['file'], line=r['line'], text='::'.join(r['segs']) + '::*') for r in use_rows if r['glob']])
    json.dump(meta, open(os.path.join(OUT, 'meta.json'), 'w'))
    print('files', len(files_info), 'of', len(all_files), 'refs', len(rows), 'name_uses', nu, dict(stats),
          'unresolved', len(unresolved), 'orphans', orphans[:10], 'unresolved_mod_files', unresolved_files[:5])


main()
