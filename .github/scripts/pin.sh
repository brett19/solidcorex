#!/usr/bin/env bash
# Written for solidcorex. Apache-2.0.
# Builds the pinned solidcore checkout the difftest crate links, at <parent>/sc-wt/scx-pin.
# The pin is solidcraft commit 03e16c9ee7, whose solidcore/ directory is the public mirror
# tangency-labs/solidcore at MIRROR below (the same tree, c5c43059c9). The checkout is a fresh
# one-commit repository holding that tree under solidcore/, as the build script expects.
set -euo pipefail
MIRROR=9f0f9f31e61ad1df0baa26170eb507fbf1867e7b
TREE=c5c43059c99ed6b4d36e5ddf2c76dcb1992848ab
dest=${1:?usage: pin.sh <checkout dir>}
mkdir -p "$dest"
cd "$dest"
git init -q .
git fetch -q --depth 1 "${MIRROR_URL:-https://github.com/tangency-labs/solidcore.git}" "$MIRROR"
git read-tree --prefix=solidcore/ FETCH_HEAD
git checkout-index -a
git -c user.name=ci -c user.email=ci@invalid commit -q -m "pin: solidcore at the mirror tree"
test "$(git rev-parse HEAD:solidcore)" = "$TREE" || { echo "pin tree mismatch" >&2; exit 1; }
# Cargo fingerprints a path dependency by its files' mtimes, and a fresh checkout stamps them with
# the clock, so a cached build of the pin would be thrown away every run. A fixed stamp keeps it.
find solidcore -exec touch -h -d '2020-01-01 00:00:00 UTC' {} +
