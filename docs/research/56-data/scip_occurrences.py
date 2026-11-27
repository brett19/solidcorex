#!/usr/bin/env python3
"""Flatten a SCIP index into a TSV of occurrences, with no protobuf dependency.

Usage: scip_occurrences.py index.scip PREFIX > occurrences.tsv

Only documents whose relative path starts with PREFIX are written (for the kernel crate,
PREFIX is `crates/solidcore/src/`; the prefix is stripped from the output path). Each output
row is: path, 1-based line, role (`def` or `ref`), symbol. Local symbols are skipped.

The fields read are those of scip.proto: Index.documents = 2; Document.relative_path = 1,
Document.occurrences = 2; Occurrence.range = 1 (packed int32), Occurrence.symbol = 2,
Occurrence.symbol_roles = 3 (bit 1 = Definition).
"""
import sys


def varint(b, i):
    r = s = 0
    while True:
        c = b[i]
        i += 1
        r |= (c & 0x7F) << s
        if c < 0x80:
            return r, i
        s += 7


def fields(b, i=0, end=None):
    end = len(b) if end is None else end
    while i < end:
        key, i = varint(b, i)
        f, w = key >> 3, key & 7
        if w == 0:
            v, i = varint(b, i)
            yield f, w, v
        elif w == 2:
            n, i = varint(b, i)
            yield f, w, (i, i + n)
            i += n
        elif w == 1:
            i += 8
        elif w == 5:
            i += 4
        else:
            raise ValueError(f"wire type {w}")


def main():
    path, prefix = sys.argv[1], sys.argv[2]
    b = open(path, "rb").read()
    out = sys.stdout
    for f, w, v in fields(b):
        if f != 2 or w != 2:
            continue
        ds, de = v
        rel = None
        occs = []
        for g, gw, gv in fields(b, ds, de):
            if g == 1 and gw == 2:
                rel = b[gv[0]:gv[1]].decode()
                if not rel.startswith(prefix):
                    break
            elif g == 2 and gw == 2:
                occs.append(gv)
        if rel is None or not rel.startswith(prefix):
            continue
        rel = rel[len(prefix):]
        for os_, oe in occs:
            line = None
            sym = ""
            roles = 0
            for h, hw, hv in fields(b, os_, oe):
                if h == 1 and hw == 2 and line is None:
                    line, _ = varint(b, hv[0])
                elif h == 1 and hw == 0 and line is None:
                    line = hv
                elif h == 2 and hw == 2:
                    sym = b[hv[0]:hv[1]].decode()
                elif h == 3 and hw == 0:
                    roles = hv
            if not sym or sym.startswith("local ") or line is None:
                continue
            out.write(f"{rel}\t{line + 1}\t{'def' if roles & 1 else 'ref'}\t{sym}\n")


if __name__ == "__main__":
    main()
