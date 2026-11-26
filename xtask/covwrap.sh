#!/usr/bin/env bash
# Written for solidcorex. Apache-2.0. A rustc wrapper for `cargo xtask cov`: instruments for
# coverage only the crates named in SCX_COV_CRATES (crate names, space-separated), so the
# pinned solidcore and the registry stay uninstrumented and fast.
rustc=$1; shift
prev=
for a in "$@"; do
  if [ "$prev" = "--crate-name" ]; then
    for c in $SCX_COV_CRATES; do
      if [ "$a" = "$c" ]; then exec "$rustc" "$@" -C instrument-coverage; fi
    done
  fi
  prev=$a
done
exec "$rustc" "$@"
