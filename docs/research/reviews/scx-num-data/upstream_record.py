#!/usr/bin/env python3
# Written for solidcorex (review of scx-num). Checks that each moved file's header quotes
# its pin file's upstream record word for word (whitespace and comment markers ignored).
# Usage: upstream_record.py PIN_SRC_DIR SCX_SRC_DIR
import re, sys, os
pin, scx = sys.argv[1], sys.argv[2]
MAP = {'math.rs': ['math.rs'], 'limits.rs': ['limits.rs'],
       'small_matrix.rs': ['geom/geom_utility/p1.rs', 'geom/geom_utility/p2.rs']}
for f in sorted(os.listdir(scx)):
    if not f.endswith('.rs'): continue
    text = open(os.path.join(scx, f)).read()
    head = []
    for l in text.splitlines():
        if l.startswith('//') and not l.startswith('//!') and not l.startswith('///'): head.append(l[2:])
        else: break
    h = ' '.join(head)
    if 'Moved from solidcore' not in h:
        print(f'{f}: written here'); continue
    m = re.search(r'Upstream record \(verbatim from th(?:at file|ose files)\):(.*?)Changes:', h, re.S)
    quoted = ' '.join(m.group(1).split()) if m else None
    srcs = MAP.get(f, ['geom/' + f])
    ok = []
    for s in srcs:
        p = open(os.path.join(pin, s)).read()
        docs = [l[3:].strip() if l.startswith('//!') else None for l in p.splitlines()]
        # the record: from the line starting "Upstream" (or "Not upstream") to the next blank doc line
        rec = []; on = False
        for d in docs:
            if d is None:
                if on: break
                continue
            if not on and re.match(r'(Upstream|Not upstream)', d): on = True
            if on:
                if d == '': break
                rec.append(d)
        r = ' '.join(' '.join(rec).split())
        ok.append((s, r == quoted, r))
    for s, same, r in ok:
        print(f'{f} <- {s}: {"VERBATIM" if same else "DIFFERS"}')
        if not same:
            print('   pin :', r[:300]); print('   here:', (quoted or '')[:300])
