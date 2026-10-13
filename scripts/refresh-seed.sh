#!/bin/sh
# refresh-seed.sh [seed-dir]: rebuild the target SEED that new worktrees start from.
#
# Run it from a worktree on develop (or any worktree whose Cargo.lock is the one you want warm),
# inside the CPU lock. Do it when Cargo.lock changes (a new dependency is a cold compile in every
# new worktree until the seed has it) and now and then otherwise; a stale seed is still safe, it
# only warms less.
#
#   cpu_lock_run seed-refresh 1800 -- scripts/refresh-seed.sh
#
# It builds every profile a lane uses, in THIS worktree's own target/ (release build, release and
# gate test builds, gate with --all-features, clippy), then makes the seed from the result: the
# dependency artifacts stay; every artifact of a workspace crate, its incremental state and its
# fingerprints go (scripts/seed-target.sh explains why only registry dependencies are reusable).
# The default seed is the directory above the main checkout, ../.plinth-target. The old seed is
# replaced only after the new one is complete.
set -eu
ROOT="$(git rev-parse --show-toplevel)"
COMMON="$(cd "$(git rev-parse --git-common-dir)" && pwd)"
SEED="${1:-$(dirname "$(dirname "$COMMON")")/.plinth-target}"
cd "$ROOT"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/refresh-seed.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

echo "== building every profile in $ROOT/target"
: >"$TMP/artifacts.jsonl"
cb() { # cb <cargo subcommand and arguments>: keep the artifact list, show the progress
  cargo "$@" --offline --message-format=json-render-diagnostics >>"$TMP/artifacts.jsonl"
}
cb build --release --workspace
cb test --release --workspace --no-run
cb test --profile gate --workspace --no-run
cb test --profile gate --workspace --all-features --no-run
cb clippy --workspace --all-targets --all-features

echo "== making the seed"
cargo metadata --no-deps --format-version 1 --offline >"$TMP/md.json"
NEW="$SEED.new"
rm -rf "$NEW"
/bin/cp -cR "$ROOT/target" "$NEW"
# 1. every artifact a workspace crate produced (library, rmeta, test and example executables)
jq -r --slurpfile md "$TMP/md.json" '
    ($md[0].packages | map(.manifest_path)) as $ws
    | select(.reason == "compiler-artifact" and (.manifest_path as $m | any($ws[]; . == $m)))
    | (.filenames // [])[], (.executable // empty)' "$TMP/artifacts.jsonl" \
  | sort -u | while IFS= read -r f; do
      case "$f" in "$ROOT/target/"*) rm -f "$NEW/${f#"$ROOT"/target/}" ;; esac
    done
# 2. workspace fingerprints, incremental state, timing reports, scratch
for pkg in $(jq -r '.packages[].name' "$TMP/md.json"); do
  rm -rf "$NEW"/*/.fingerprint/"$pkg"-*
done
rm -rf "$NEW"/*/incremental "$NEW"/cargo-timings "$NEW"/tmp "$NEW"/tests "$NEW"/ratchet_harness_test_scratch \
       "$NEW"/plinth-decimate-corpus-budgets "$NEW"/.rustc_info.json
# 3. provenance (the hook reads the Cargo.lock hash to say when the seed is stale)
{
  echo "Seed for per-worktree target dirs (scripts/seed-target.sh). Built by scripts/refresh-seed.sh at $(git rev-parse --short HEAD), $(date '+%Y-%m-%d %H:%M:%S')."
  echo "Cargo.lock sha256: $(shasum -a 256 Cargo.lock | cut -d' ' -f1)"
  echo "Registry dependencies only; the workspace fingerprints and artifacts are removed. Clone with scripts/seed-target.sh; never point CARGO_TARGET_DIR here."
} >"$NEW/SEED-PROVENANCE.txt"

OLD="$SEED.old"
rm -rf "$OLD"
[ -d "$SEED" ] && mv "$SEED" "$OLD"
mv "$NEW" "$SEED"
rm -rf "$OLD"
echo "seed refreshed: $SEED ($(du -sh "$SEED" | cut -f1))"
