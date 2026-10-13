#!/usr/bin/env bash
# gate-full.sh: the milestone test gate, run to completion in one command.
#
#   - every workspace test binary (unit tests, integration programs, doctests) under
#     `--profile gate`, plus the ignored tests (`--include-ignored`: the slow legacy-runner
#     grinders that scripts/test-fast.sh skips), and the separate tools/plinth-audit workspace
#     (built `--release`; it has no gate profile);
#   - independent test binaries run side by side, at most -j at a time (default 3);
#   - the result is compared with scripts/known-reds.txt: PASS / NEW-RED / FIXED-RED, exit 1 on
#     any NEW-RED, exit 2 when something did not build.
#
# It is the merge gate's TEST half only. The rest of the gate (clippy, fmt, git diff --check,
# `--release` for the merge) is in docs/development.md. Wrap the call in the CPU lock:
#   cpu_lock_run <label> <cap-s> -- scripts/gate-full.sh [options]
#
# Options:
#   -j N              test binaries run side by side (default 3)
#   --all-features    also run the workspace with --all-features (a second pass)
#   --skip-ignored    do not run `#[ignore]`d tests
#   --no-audit        leave out tools/plinth-audit
#   --known FILE      known-reds list (default: scripts/known-reds.txt next to this script)
#   --out DIR         keep logs and verdict files here (default: a fresh temp dir)
export LC_ALL=C
set -o pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/test-units.sh
. "$HERE/lib/test-units.sh"
ROOT="$(cd "$HERE/.." && pwd)"
cd "$ROOT" || exit 2

JOBS=3; ALLF=0; IGNORED=(--include-ignored); AUDIT=1; KNOWN="$HERE/known-reds.txt"; OUT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -j) JOBS="$2"; shift ;;
    --all-features) ALLF=1 ;;
    --skip-ignored) IGNORED=() ;;
    --no-audit) AUDIT=0 ;;
    --known) KNOWN="$2"; shift ;;
    --out) OUT="$2"; shift ;;
    *) echo "gate-full.sh: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$OUT" ] || OUT="$(mktemp -d "${TMPDIR:-/tmp}/gate-full.XXXXXX")"
mkdir -p "$OUT/logs"

T0="$(date +%s)"
WS_ARGS=(--profile gate --offline)
AUDIT_ARGS="--release --offline --manifest-path $ROOT/tools/plinth-audit/Cargo.toml"

# --- build every test binary once -----------------------------------------------------------------
echo "== build"
cargo test --workspace "${WS_ARGS[@]}" --no-run --message-format=json \
  >"$OUT/artifacts.jsonl" 2>"$OUT/build.log" || { tail -40 "$OUT/build.log"; echo "BUILD-FAIL (workspace): $OUT/build.log"; exit 2; }
tu_enumerate "$ROOT" "$OUT/artifacts.jsonl" >"$OUT/units.unordered"

if [ "$ALLF" = 1 ]; then
  # A second pass over the workspace with every feature: units are listed again, so their ids
  # carry a "+all-features" label via a separate log directory and result files.
  cargo test --workspace "${WS_ARGS[@]}" --all-features --no-run --message-format=json \
    >"$OUT/artifacts-allf.jsonl" 2>"$OUT/build-allf.log" || { tail -40 "$OUT/build-allf.log"; echo "BUILD-FAIL (all features): $OUT/build-allf.log"; exit 2; }
fi

if [ "$AUDIT" = 1 ]; then
  # shellcheck disable=SC2086
  cargo test $AUDIT_ARGS --no-run --message-format=json \
    >"$OUT/artifacts-audit.jsonl" 2>"$OUT/build-audit.log" || { tail -40 "$OUT/build-audit.log"; echo "BUILD-FAIL (plinth-audit): $OUT/build-audit.log"; exit 2; }
  tu_enumerate "$ROOT/tools/plinth-audit" "$OUT/artifacts-audit.jsonl" \
    | awk -F'\t' -v A="$AUDIT_ARGS" '{ print $0 "\t" A }' >>"$OUT/units.unordered"
fi
T1="$(date +%s)"

# --- run -------------------------------------------------------------------------------------------
echo "== run $(wc -l <"$OUT/units.unordered" | tr -d ' ') test binaries, $JOBS at a time"
tu_order "$OUT/units.unordered" >"$OUT/units.list"
tu_run_pool "$OUT/logs" "$JOBS" "$OUT/units.list" "${WS_ARGS[@]}" -- ${IGNORED[@]+"${IGNORED[@]}"}
T2="$(date +%s)"

tu_collect "$OUT/logs" "$OUT"
sort -t "$(printf '\t')" -k3,3nr "$OUT/units.tsv" \
  | awk -F'\t' '{ printf "  %-52s rc=%s %5ss  ok %s  FAILED %s  ignored %s\n", $1, $2, $3, $4, $5, $6 }'

if [ "$ALLF" = 1 ]; then
  mkdir -p "$OUT/allf/logs"
  tu_enumerate "$ROOT" "$OUT/artifacts-allf.jsonl" >"$OUT/allf/units.unordered"
  tu_order "$OUT/allf/units.unordered" >"$OUT/allf/units.list"
  echo "== run the --all-features pass"
  tu_run_pool "$OUT/allf/logs" "$JOBS" "$OUT/allf/units.list" "${WS_ARGS[@]}" --all-features -- ${IGNORED[@]+"${IGNORED[@]}"}
  tu_collect "$OUT/allf/logs" "$OUT/allf"
  # one verdict over both passes: an id failing in either pass is a failure
  cat "$OUT/allf/failed.txt" >>"$OUT/failed.txt"; sort -u "$OUT/failed.txt" -o "$OUT/failed.txt"
  cat "$OUT/allf/passed.txt" >>"$OUT/passed.txt"; sort -u "$OUT/passed.txt" -o "$OUT/passed.txt"
  comm -13 "$OUT/failed.txt" "$OUT/passed.txt" >"$OUT/passed.only.txt" && mv "$OUT/passed.only.txt" "$OUT/passed.txt"
fi
T3="$(date +%s)"

echo "build $((T1 - T0)) s, run $((T2 - T1)) s, total $((T3 - T0)) s; logs and verdict files in $OUT"
tu_verdict "$OUT" "$KNOWN" -
