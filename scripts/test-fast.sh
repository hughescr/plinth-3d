#!/usr/bin/env bash
# test-fast.sh: the lane test loop. `cargo test --profile gate` for the crates you name (default:
# the crates changed against develop, plus every workspace crate that depends on them), then a
# verdict against scripts/known-reds.txt:
#
#   PASS       no failing test outside the known-reds list
#   NEW-RED    a failing test that is not listed: a regression, exit status 1
#   FIXED-RED  a listed test that passes now: delete its line from known-reds.txt
#
# Slow legacy-runner tests are `#[ignore]`d and do not run here; scripts/gate-full.sh runs them.
# Wrap the call in the CPU lock like every other cargo command:
#   cpu_lock_run <label> <cap-s> -- scripts/test-fast.sh [options] [crate ...] [-- libtest-args]
#
# Options:
#   --no-dependents   test only the named / changed crates
#   --all-features    add --all-features to the build and the test run
#   --base REF        the ref "changed" is measured against (default: develop)
#   -j N              test binaries run side by side (default 3)
#   --known FILE      known-reds list (default: scripts/known-reds.txt next to this script)
#   -- ARGS           passed to every test binary (a test-name filter, for instance)
export LC_ALL=C
set -o pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/test-units.sh
. "$HERE/lib/test-units.sh"
ROOT="$(cd "$HERE/.." && pwd)"
cd "$ROOT" || exit 2

DEPENDENTS=1; FEATURES=(); BASE=develop; JOBS=3; KNOWN="$HERE/known-reds.txt"
CRATES=(); LIBTEST=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-dependents) DEPENDENTS=0 ;;
    --all-features) FEATURES=(--all-features) ;;
    --base) BASE="$2"; shift ;;
    -j) JOBS="$2"; shift ;;
    --known) KNOWN="$2"; shift ;;
    --) shift; LIBTEST=("$@"); break ;;
    -*) echo "test-fast.sh: unknown option $1" >&2; exit 2 ;;
    *) CRATES+=("$1") ;;
  esac
  shift
done

OUT="${TEST_FAST_OUT:-$(mktemp -d "${TMPDIR:-/tmp}/test-fast.XXXXXX")}"
mkdir -p "$OUT/logs"

# --- which crates ------------------------------------------------------------------------------
META="$OUT/metadata.json"
cargo metadata --no-deps --format-version 1 --offline >"$META" || exit 2

if [ "${#CRATES[@]}" = 0 ]; then
  MB="$(git merge-base HEAD "$BASE" 2>/dev/null || echo "$BASE")"
  { git diff --name-only "$MB"; git ls-files -o --exclude-standard; } | sort -u >"$OUT/changed-files.txt"
  if grep -qE '^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml)$' "$OUT/changed-files.txt"; then
    CHANGED="$(jq -r '.packages[].name' "$META")"
  else
    CHANGED="$(jq -r --arg root "$ROOT/" --rawfile files "$OUT/changed-files.txt" '
        ($files | split("\n") | map(select(length > 0))) as $f
        | .packages[]
        | (.manifest_path | sub("^" + $root; "") | sub("Cargo.toml$"; "")) as $dir
        | select(any($f[]; startswith($dir)))
        | .name' "$META")"
  fi
  # The audit tool is its own workspace; a change under it is tested by gate-full.sh.
  if grep -q '^tools/plinth-audit/' "$OUT/changed-files.txt"; then
    echo "note: tools/plinth-audit changed; its tests run in scripts/gate-full.sh (own workspace)" >&2
  fi
  while IFS= read -r c; do [ -n "$c" ] && CRATES+=("$c"); done <<<"$CHANGED"
  if [ "${#CRATES[@]}" = 0 ]; then
    echo "PASS: no workspace crate changed against $BASE; nothing to test"
    exit 0
  fi
fi

if [ "$DEPENDENTS" = 1 ]; then
  SEED="$(printf '%s\n' "${CRATES[@]}" | jq -R . | jq -sc .)"
  CLOSURE="$(jq -r --argjson seed "$SEED" '
      [.packages[] | {name, deps: [.dependencies[].name]}] as $m
      | def grow($s): ($s + [$m[] | select(any(.deps[]; . as $d | $s | index($d))) | .name] | unique);
        def fix($s): grow($s) as $n | if ($n | length) == ($s | length) then $s else fix($n) end;
      fix($seed | unique) | .[]' "$META")"
  CRATES=()
  while IFS= read -r c; do [ -n "$c" ] && CRATES+=("$c"); done <<<"$CLOSURE"
fi
echo "crates under test (${#CRATES[@]}): ${CRATES[*]}"

PKG_ARGS=()
for c in "${CRATES[@]}"; do PKG_ARGS+=(-p "$c"); done
CARGO_ARGS=(--profile gate --offline ${FEATURES[@]+"${FEATURES[@]}"})

# --- build, then run the binaries side by side --------------------------------------------------
START="$(date +%s)"
if ! cargo test "${CARGO_ARGS[@]}" "${PKG_ARGS[@]}" --no-run --message-format=json \
      >"$OUT/artifacts.jsonl" 2>"$OUT/build.log"; then
  tail -40 "$OUT/build.log"
  echo "BUILD-FAIL: see $OUT/build.log"
  exit 2
fi
BUILT="$(date +%s)"
tu_enumerate "$ROOT" "$OUT/artifacts.jsonl" >"$OUT/units.unordered"
tu_order "$OUT/units.unordered" >"$OUT/units.list"

tu_run_pool "$OUT/logs" "$JOBS" "$OUT/units.list" "${CARGO_ARGS[@]}" -- ${LIBTEST[@]+"${LIBTEST[@]}"}
END="$(date +%s)"

tu_collect "$OUT/logs" "$OUT"
awk -F'\t' '{ printf "  %-52s rc=%s %5ss  ok %s  FAILED %s  ignored %s\n", $1, $2, $3, $4, $5, $6 }' "$OUT/units.tsv"
awk -F'\t' '{ print $1 }' "$OUT/units.tsv" >"$OUT/ran.txt"
echo "build $((BUILT - START)) s, tests $((END - BUILT)) s; logs in $OUT"
tu_verdict "$OUT" "$KNOWN" "$OUT/ran.txt"
