#!/usr/bin/env python3
# Written for solidcorex (review of scx-num). Compares every fn body of a moved file with the
# same-named fn at the pin after normalising the changes the move states: paths, error type
# names, &Kernel -> &Tolerances, visibility, comments and whitespace.
# Usage: fndiff.py PIN_FILE SCX_FILE
import re, sys

def strip_comments(s):
    # string literals first (a '//' or a brace inside a string), then char literals, then comments
    s = re.sub(r'(?<![A-Za-z0-9_])r(#*)"(?:.|\n)*?"\1', '""', s)
    s = re.sub(r'"(?:[^"\\]|\\.)*"', lambda m: '"' + re.sub(r'[{}]', '', m.group(0)[1:-1]) + '"', s)
    s = re.sub(r"'(?:[^'\\\n]|\\.)'", "'c'", s)
    s = re.sub(r'//[^\n]*', '', s)
    s = re.sub(r'/\*(?:.|\n)*?\*/', '', s)
    return s

def items(src):
    """Yield (qualified name, body text) for every fn, keyed by enclosing impl/mod and name."""
    out = {}
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
            i += 1; continue
        if c == '}':
            if ctx and ctx[-1][1] == depth:
                ctx.pop()
            depth -= 1
            i += 1; continue
        m = re.match(r'(impl(?:<[^>]*>)?\s+[^{;]*?|mod\s+\w+)\s*\{', src_nc[i:])
        if m and (i == 0 or not (src_nc[i-1].isalnum() or src_nc[i-1] == '_')):
            pending = re.sub(r'\s+', ' ', m.group(1)).strip()
            i += m.end() - 1; continue
        m = re.match(r'fn\s+(\w+)', src_nc[i:])
        if m and (i == 0 or not (src_nc[i-1].isalnum() or src_nc[i-1] == '_')):
            # find body
            j = i + m.end()
            k = j
            br = 0  # `;` inside `[f64; 3]` or `(..)` does not end a signature
            while k < n and not (br == 0 and src_nc[k] in '{;'):
                if src_nc[k] in '([': br += 1
                elif src_nc[k] in ')]': br -= 1
                k += 1
            if k < n and src_nc[k] == '{':
                d = 0; e = k
                while e < n:
                    if src_nc[e] == '{': d += 1
                    elif src_nc[e] == '}':
                        d -= 1
                        if d == 0: break
                    e += 1
                key = '/'.join(x[0] for x in ctx) + '::' + m.group(1)
                base = key; t = 2
                while key in out:
                    key = f'{base}#{t}'; t += 1
                out[key] = src_nc[i:e+1]
                i = e + 1; continue
        i += 1
    return out

RULES = [
    (r'crate::geom::geom_utility::p[12]::', 'crate::'),
    (r'crate::geom::', 'crate::'),
    (r'crate::nurbs::math::', 'crate::'),
    (r'\bSolidError\b', 'E'), (r'\bNumError\b', 'E'),
    (r'\bGeomFailure\b', 'F'), (r'\bNumFailure\b', 'F'),
    (r'crate::error::geom::', 'crate::error::'),
    (r'\bE::geom_call\b', 'E::call'), (r'\bE::geom\(', 'E::failed('),
    (r'crate::Result\b', 'crate::error::Result'),
    (r'crate::E\b', 'crate::error::E'),
    (r'pub\(crate\)\s+', 'pub '),
    (r'\bk\s*:\s*&Kernel\b', 'T'), (r'\bt\s*:\s*&Tolerances\b', 'T'),
    (r'\bk\.tolerances\.', 't.'), (r'\bk\.tolerances\b', 't'),
    (r'#\[inline[^\]]*\]', ''),
]

def norm(s):
    for a, b in RULES:
        s = re.sub(a, b, s)
    s = re.sub(r'\s+', '', s)
    s = s.replace(',)', ')').replace(',]', ']').replace(',}', '}')
    return s

a = items(open(sys.argv[1]).read())
b = items(open(sys.argv[2]).read())
only_a = [k for k in a if k not in b]
only_b = [k for k in b if k not in a]
diff = [k for k in a if k in b and norm(a[k]) != norm(b[k])]
same = [k for k in a if k in b and norm(a[k]) == norm(b[k])]
print(f'# {sys.argv[2]}: {len(same)} same, {len(diff)} differ, {len(only_a)} only at pin, {len(only_b)} only here')
for k in only_a: print('PIN-ONLY', k)
for k in only_b: print('SCX-ONLY', k)
for k in diff:
    print('DIFFER', k)
    import difflib
    for l in difflib.unified_diff(a[k].splitlines(), b[k].splitlines(), lineterm='', n=0):
        if l.startswith(('---', '+++', '@@')): continue
        print('   ', l)
