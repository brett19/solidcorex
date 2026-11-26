#!/usr/bin/env bash
# Written for solidcorex. Apache-2.0. `cargo xtask cov`: region coverage of the moved
# libraries while the differential suite runs (the recipe of the scx-num review,
# docs/research/reviews/scx-num-data). Instruments the libraries named (crate names,
# default scx_num) and the difftest binary `diff`, runs the suite once in the dev profile,
# and writes under target/cov/: cov.json (llvm-cov export), report.txt (per file),
# branchy.txt (the 30 functions with the most regions, and every function with a region
# the suite never ran, with its lines).
# Usage: cov.sh REPO_ROOT [crate ...]
set -eu
ROOT=$1; shift
CRATES="${*:-scx_num}"
OUT=$ROOT/target/cov
mkdir -p "$OUT/prof"
rm -f "$OUT"/prof/*.profraw
cd "$ROOT"
CARGO=${CARGO:-cargo}
SYSROOT=$(rustc --print sysroot)
HOST=$(rustc -vV | sed -n 's/^host: //p')
BIN_DIR=$SYSROOT/lib/rustlib/$HOST/bin
export CARGO_TARGET_DIR=$OUT/target RUSTC_WRAPPER=$ROOT/xtask/covwrap.sh SCX_COV_CRATES="$CRATES diff"
BIN=$("$CARGO" test -p scx-difftest --no-run --message-format=json 2>"$OUT/build.log" \
  | grep -o '"executable":"[^"]*/diff-[^"]*"' | tail -1 | sed 's/"executable":"//; s/"$//')
[ -n "$BIN" ] || { echo "cov: no difftest binary (see $OUT/build.log)"; exit 1; }
start=$(date +%s.%N)
LLVM_PROFILE_FILE=$OUT/prof/diff-%p.profraw "$BIN" > "$OUT/run.log" 2>&1 || { echo "cov: the difftest failed (see $OUT/run.log)"; exit 1; }
end=$(date +%s.%N)
"$BIN_DIR/llvm-profdata" merge -sparse "$OUT"/prof/*.profraw -o "$OUT/diff.profdata"
IGN='(registry|/rustc/|sc-wt|scx-difftest|xtask)'
"$BIN_DIR/llvm-cov" export -format=text -instr-profile="$OUT/diff.profdata" "$BIN" --ignore-filename-regex="$IGN" > "$OUT/cov.json"
"$BIN_DIR/llvm-cov" report -instr-profile="$OUT/diff.profdata" "$BIN" --ignore-filename-regex="$IGN" > "$OUT/report.txt"
python3 -I "$ROOT/xtask/branchy.py" "$OUT/cov.json" "$ROOT/crates" 30 > "$OUT/branchy.txt"
awk -v a="$start" -v b="$end" 'BEGIN { printf "cov: difftest run %.1f s under instrumentation\n", b - a }'
grep -E '^(Filename|TOTAL)' "$OUT/report.txt"
head -32 "$OUT/branchy.txt"
echo "cov: target/cov/report.txt and target/cov/branchy.txt (every unexecuted region)"
