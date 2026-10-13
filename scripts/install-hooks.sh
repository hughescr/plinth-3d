#!/bin/sh
# install-hooks.sh: copy scripts/hooks/* into the shared git hooks directory of this repository
# (the main checkout's .git/hooks, which every linked worktree uses). Run it once after cloning
# and again whenever a file under scripts/hooks changes. It replaces an existing hook of the same
# name, so the file in the repository stays the single source of truth.
set -eu
ROOT="$(git rev-parse --show-toplevel)"
COMMON="$(cd "$(git rev-parse --git-common-dir)" && pwd)"
mkdir -p "$COMMON/hooks"
for hook in "$ROOT"/scripts/hooks/*; do
  [ -f "$hook" ] || continue
  name="$(basename "$hook")"
  cp "$hook" "$COMMON/hooks/$name"
  chmod +x "$COMMON/hooks/$name"
  echo "installed $COMMON/hooks/$name"
done
