#!/usr/bin/env python3
# Written for solidcorex. Apache-2.0. Adopted from the scx-num review's `fndiff.py`
# (docs/research/reviews/scx-num-data/): the normalised body diff, run by
# `cargo xtask bodydiff` over every file under `crates/`.
#
# For each file with a derivation header ("Moved from" / "Derived from solidcore"), every
# fn body (tests included) is compared with the same-named fn in the pin files the header
# names, after normalising the changes a move makes mechanically: comments, whitespace,
# `use` lines, module paths, the error type names, `&Kernel` -> `&Tolerances`, visibility
# and `#[inline]`. Every fn that then differs, or is only here, must be named in the
# header's `Changes:`; so must every fn only at the pin, unless `Changes:` says the move
# took only part of the file (the word "only").
#
# For each file "written for solidcorex", no fn body of 200 or more normalised characters
# may equal a fn body anywhere in the pin's solidcore crate: such a file is derived and
# needs a derivation header.
#
# Usage: bodydiff.py REPO_ROOT PIN_CHECKOUT  (the checkout must be at the pin and clean;
# `cargo xtask pin` checks it). Exit status 1 on any finding.
import os
import re
import sys
import difflib


def strip_comments(s):
    # string literals first (a '//' or a brace inside a string), then char literals, then comments
    s = re.sub(r'(?<![A-Za-z0-9_])r(#*)"(?:.|\n)*?"\1', '""', s)
    s = re.sub(r'"(?:[^"\\]|\\.)*"', lambda m: '"' + re.sub(r'[{}]', '', m.group(0)[1:-1]) + '"', s)
    s = re.sub(r"'(?:[^'\\\n]|\\.)'", "'c'", s)
    s = re.sub(r'//[^\n]*', '', s)
    s = re.sub(r'/\*(?:.|\n)*?\*/', '', s)
    return s


def items(src, out=None):
    """(qualified name -> body text) for every fn, keyed by enclosing impl/mod and name."""
    out = {} if out is None else out
    src_nc = strip_comments(src)
    ctx = []  # stack of (name, depth)
    depth = 0
    i = 0
    n = len(src_nc)
    pending = None
    while i < n:
        c = src_nc[i]
        if c == '{':
            depth += 1
            if pending is not None:
                ctx.append((pending, depth))
                pending = None
            i += 1
            continue
        if c == '}':
            if ctx and ctx[-1][1] == depth:
                ctx.pop()
            depth -= 1
            i += 1
            continue
        m = re.match(r'(impl(?:<[^>]*>)?\s+[^{;]*?|mod\s+\w+)\s*\{', src_nc[i:])
        if m and (i == 0 or not (src_nc[i - 1].isalnum() or src_nc[i - 1] == '_')):
            pending = re.sub(r'\s+', ' ', m.group(1)).strip()
            i += m.end() - 1
            continue
        m = re.match(r'fn\s+(\w+)', src_nc[i:])
        if m and (i == 0 or not (src_nc[i - 1].isalnum() or src_nc[i - 1] == '_')):
            j = i + m.end()
            k = j
            br = 0  # `;` inside `[f64; 3]` or `(..)` does not end a signature
            while k < n and not (br == 0 and src_nc[k] in '{;'):
                if src_nc[k] in '([':
                    br += 1
                elif src_nc[k] in ')]':
                    br -= 1
                k += 1
            if k < n and src_nc[k] == '{':
                d = 0
                e = k
                while e < n:
                    if src_nc[e] == '{':
                        d += 1
                    elif src_nc[e] == '}':
                        d -= 1
                        if d == 0:
                            break
                    e += 1
                key = '/'.join(x[0] for x in ctx) + '::' + m.group(1)
                base = key
                t = 2
                while key in out:
                    key = f'{base}#{t}'
                    t += 1
                out[key] = src_nc[i:e + 1]
                i = e + 1
                continue
        i += 1
    return out


RULES = [
    (r'(?m)^\s*use\s[^;]*;', ''),  # imports are paths
    (r'\b(?:crate|super)::(?:\w+::)*', ''),  # module paths
    (r'\b[a-z_][a-z0-9_]*::(?=[A-Za-z_])', ''),  # a module prefix (`p1::`, `math::`)
    (r'\bSolidError\b', 'E'), (r'\bNumError\b', 'E'),
    (r'\bGeomFailure\b', 'F'), (r'\bNumFailure\b', 'F'),
    (r'\bE::geom_call\b', 'E::call'), (r'\bE::geom\(', 'E::failed('), (r'\bE::blend\(', 'E::failed('),
    # the series' blend failures, as `NumFailure` (series.rs's Changes)
    (r'\bBlendFailure::SectionDegenerate\(SectionDegeneracy::VectorVanishes\)', 'F::VectorVanishes'),
    (r'\bBlendFailure::Defect\b', 'F::Defect'),
    (r'let\s+t\s*:\s*&Tolerances\s*=\s*&k\.tolerances\s*;', ''),
    (r'pub\(crate\)\s+', 'pub '), (r'pub\(super\)\s+', 'pub '),
    (r'\bk\s*:\s*&Kernel\b', 'T'), (r'\bt\s*:\s*&Tolerances\b', 'T'),
    (r'\bk\.tolerances\.', 't.'), (r'\bk\.tolerances\b', 't'),
    (r'\bKernel::default\(\)', 'Tolerances::default()'),
    (r'\bsize_estimate_to_zone_tol_3d\b', 'zone_for_size'),  # tol.rs's declared rename
    (r'#\[inline[^\]]*\]', ''),
    (r'#\[must_use[^\]]*\]', ''),
]


def norm(s):
    if re.search(r'\bk\s*:\s*&Kernel\b', s):
        # `k: &Kernel` became `t: &Tolerances`: the parameter's name with it
        s = re.sub(r'\bk\s*:\s*&Kernel\b', 't: &Tolerances', s)
        s = re.sub(r'let\s+t\s*:\s*&Tolerances\s*=\s*&k\.tolerances\s*;', '', s)
        s = re.sub(r'\bk\.tolerances\b', 't', s)
        s = re.sub(r'\bk\b', 't', s)
    for a, b in RULES:
        s = re.sub(a, b, s)
    s = re.sub(r'\s+', '', s)
    return s.replace(',)', ')').replace(',]', ']').replace(',}', '}')


def opening_comment(text):
    head = []
    for line in text.splitlines():
        if line.startswith('//') and not line.startswith('//!') and not line.startswith('///'):
            head.append(line[2:])
        else:
            break
    return '\n'.join(head)


def rust_files(root):
    for top in ('crates',):
        for d, dirs, files in os.walk(os.path.join(root, top)):
            dirs[:] = [x for x in dirs if x != 'target' and not x.startswith('.')]
            for f in sorted(files):
                if f.endswith('.rs'):
                    yield os.path.join(d, f)


def pin_index(pin):
    """Every fn body of the pin's solidcore crate of 200+ normalised characters."""
    idx = {}
    base = os.path.join(pin, 'solidcore', 'crates', 'solidcore', 'src')
    for d, _, files in os.walk(base):
        for f in files:
            if not f.endswith('.rs'):
                continue
            p = os.path.join(d, f)
            for k, body in items(open(p, encoding='utf-8').read()).items():
                nb = norm(body)
                if len(nb) >= 200:
                    idx.setdefault(nb, f'{os.path.relpath(p, pin)} {k}')
    return idx


def named(changes, key):
    """`Changes:` names the fn in backquotes: `name` or `Path::name`."""
    name = key.split('::')[-1].split('#')[0]
    return re.search(r'`(?:[^`]*::)?' + re.escape(name) + r'`', changes) is not None


def main():
    root, pin = sys.argv[1], sys.argv[2]
    findings = []
    summary = []
    written = []
    for f in rust_files(root):
        rel = os.path.relpath(f, root)
        text = open(f, encoding='utf-8').read()
        head = opening_comment(text)
        flat = ' '.join(head.split())
        if 'Moved from solidcore' not in flat and 'Derived from solidcore' not in flat:
            written.append((rel, text))
            continue
        before = flat.split('Upstream record')[0]
        sources = [p for p in before.split('`')[1::2] if p.startswith('solidcore/') and p.endswith('.rs')]
        changes = flat.split('Changes:', 1)[1] if 'Changes:' in flat else ''
        partial = re.search(r'\bonly\b', changes) is not None
        a = {}
        for s in sources:
            items(open(os.path.join(pin, s), encoding='utf-8').read(), a)
        b = items(text)
        only_a = [k for k in a if k not in b]
        only_b = [k for k in b if k not in a]
        differ = [k for k in a if k in b and norm(a[k]) != norm(b[k])]
        same = sum(1 for k in a if k in b and norm(a[k]) == norm(b[k]))
        summary.append(f'{rel}: {same} same, {len(differ)} differ, {len(only_a)} only at the pin, '
                       f'{len(only_b)} only here')
        for k in differ:
            if not named(changes, k):
                findings.append(f'{rel}: `{k}` differs from the pin and Changes does not name it')
                for line in difflib.unified_diff(a[k].splitlines(), b[k].splitlines(), lineterm='', n=0):
                    if not line.startswith(('---', '+++', '@@')):
                        findings.append('    ' + line)
        for k in only_b:
            if not named(changes, k):
                findings.append(f'{rel}: `{k}` is not at the pin and Changes does not name it')
        if not partial:
            for k in only_a:
                if not named(changes, k):
                    findings.append(f'{rel}: `{k}` is at the pin, not here, and Changes does not name it')
    idx = pin_index(pin)
    for rel, text in written:
        for k, body in items(text).items():
            hit = idx.get(norm(body))
            if hit:
                findings.append(f'{rel}: written for solidcorex, but `{k}` is the pin\'s {hit}')
    print('\n'.join(summary))
    print(f'written-for files scanned against the pin: {len(written)} ({len(idx)} pin bodies)')
    if findings:
        print('\n'.join(findings))
        sys.exit(1)
    print('bodydiff: every difference is named in Changes')


main()
