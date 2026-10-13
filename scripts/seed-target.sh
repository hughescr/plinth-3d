#!/bin/sh
# seed-target.sh <seed-target-dir> <worktree>
#
# Gives a NEW worktree a warm target dir without sharing one: <worktree>/target becomes an APFS
# copy-on-write clone of <seed-target-dir> (no extra disk until a file is rewritten, under a
# second), and every WORKSPACE package's fingerprint is deleted, so cargo rebuilds all of the
# worktree's own crates from its own sources and reuses only the registry dependencies.
#
# Why not one shared CARGO_TARGET_DIR: cargo decides freshness by source mtime, so a worktree
# whose sources are OLDER than another worktree's last build is judged up to date and silently
# links the other branch's code (measured 2026-09-28; plinth-evidence RUN-RECIPE.md, "Shared
# build directory"). Registry dependencies are named by exact version and feature set and cannot
# differ between worktrees, so reusing THEM is safe; deleting the workspace fingerprints removes
# the only unsafe part of the seed. Never point CARGO_TARGET_DIR at the seed.
#
# The seed is made by scripts/refresh-seed.sh and installed into new worktrees by the git
# post-checkout hook (scripts/hooks/post-checkout, installed by scripts/install-hooks.sh).
set -eu
[ "$#" -eq 2 ] || { echo "usage: $0 <seed-target-dir> <worktree>" >&2; exit 2; }
SEED="$(cd "$1" && pwd)"
W="$(cd "$2" && pwd)"
[ -e "$W/target" ] && { echo "refused: $W/target already exists" >&2; exit 1; }
# /bin/cp, not `cp`: GNU coreutils shadow the macOS tools on this machine, and only the macOS cp
# has -c (clonefile(2), copy-on-write on APFS).
/bin/cp -cR "$SEED" "$W/target"
cd "$W"
removed=0
for pkg in $(cargo metadata --no-deps --format-version 1 --offline | jq -r '.packages[].name'); do
  for dir in "$W"/target/*/.fingerprint/"$pkg"-*; do
    [ -d "$dir" ] || continue
    rm -rf "$dir"; removed=$((removed+1))
  done
done
echo "seeded $W/target from $SEED; removed $removed workspace fingerprint dirs"
if [ -f "$SEED/SEED-PROVENANCE.txt" ] && [ -f "$W/Cargo.lock" ]; then
  want="$(shasum -a 256 "$W/Cargo.lock" | cut -d' ' -f1)"
  if ! grep -q "$want" "$SEED/SEED-PROVENANCE.txt"; then
    echo "note: the seed was built from a different Cargo.lock; dependencies added or changed since will build cold (scripts/refresh-seed.sh renews it)"
  fi
fi
