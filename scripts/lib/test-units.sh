#!/usr/bin/env bash
# Shared helpers for scripts/test-fast.sh and scripts/gate-full.sh. Source it; do not run it.
#
# A "unit" is one test binary: a library's unit tests, one integration-test program, a
# binary's own tests. Units are what the scripts run side by side and what the known-reds list
# is keyed by. A test's id is `<package>::<target>::<test path>`:
#
#   plinth-orient::lib::tests::z_spin_is_a_null_direction_of_every_term
#   plinth-sweep::it::prefilter_no_brace::some_test          (target = the test program's name)
#   plinth-support::doc::...                                 (doctests of the whole run)
#
# bash 3.2 (macOS /bin/bash) is the floor: no associative arrays, no `wait -n`.

TU_SLOW_FIRST="plinth-support::support_runner_integration plinth-audit::compare plinth-audit::fixtures plinth-sweep::it plinth-pipeline::it plinth-support::ratchet_harness plinth-cli::process plinth-audit::real_artifacts plinth-sweep::lib"

tu_root() { git rev-parse --show-toplevel; }

# tu_enumerate <manifest-dir> <artifacts.jsonl>
#   Prints one tab-separated line per test binary: package, cargo flag, target name.
#   <artifacts.jsonl> is `cargo test --no-run --message-format=json` stdout for that manifest.
tu_enumerate() {
  local dir="$1" artifacts="$2" md
  md="$(mktemp)"
  (cd "$dir" && cargo metadata --no-deps --format-version 1 --offline) >"$md" || return 1
  jq -r --slurpfile md "$md" '
      ($md[0].packages | map({key: .manifest_path, value: .name}) | from_entries) as $names
      | select(.reason == "compiler-artifact" and .profile.test == true and .executable != null)
      | (.target.kind) as $k
      | [ $names[.manifest_path],
          (if ($k | index("test")) then "--test"
           elif ($k | index("bin")) then "--bin"
           elif ($k | index("example")) then "--example"
           else "--lib" end),
          .target.name ] | @tsv' "$artifacts" | sort -u >"$md.units"
  cat "$md.units"
  # Doctests build no test artifact; every package with a library has some to run.
  awk -F'\t' '$2 == "--lib" { print $1 "\t--doc\tdoc" }' "$md.units"
  rm -f "$md" "$md.units"
}

# tu_unit_label <package> <flag> <name>  ->  the target part of a test id.
tu_unit_label() {
  case "$2" in
    --lib) echo "lib" ;;
    --doc) echo "doc" ;;
    *) echo "$3" ;;
  esac
}

# tu_order <units-file>  ->  the same lines, known-slow units first (longest-job-first keeps the
# wall time near the longest unit instead of the sum).
tu_order() {
  local f="$1" key line
  for key in $TU_SLOW_FIRST; do
    while IFS= read -r line; do
      [ -n "$line" ] || continue
      if [ "$(printf '%s' "$line" | awk -F'\t' '{ t = ($2 == "--lib") ? "lib" : $3; print $1 "::" t }')" = "$key" ]; then
        printf '%s\n' "$line"
      fi
    done <"$f"
  done
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    local k
    k="$(printf '%s' "$line" | awk -F'\t' '{ t = ($2 == "--lib") ? "lib" : $3; print $1 "::" t }')"
    case " $TU_SLOW_FIRST " in *" $k "*) ;; *) printf '%s\n' "$line" ;; esac
  done <"$f"
}

# tu_run_unit <logdir> <idx> <package> <flag> <name> <cargo-test-args...> -- <libtest-args...>
#   Runs one unit; writes <logdir>/<idx>.log and <logdir>/<idx>.meta (label, rc, seconds).
tu_run_unit() {
  local logdir="$1" idx="$2" pkg="$3" flag="$4" name="$5" override="$6"
  shift 6
  local cargo_args=() libtest_args=() seen=0 a
  for a in "$@"; do
    if [ "$a" = "--" ]; then seen=1; continue; fi
    if [ "$seen" = 0 ]; then cargo_args+=("$a"); else libtest_args+=("$a"); fi
  done
  # a unit line may carry its own cargo arguments (the audit tool is a separate workspace)
  # shellcheck disable=SC2206
  if [ -n "$override" ]; then cargo_args=($override); fi
  local target_args=("$flag")
  case "$flag" in --lib|--doc) ;; *) target_args+=("$name") ;; esac
  local start rc
  start="$(date +%s)"
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" \
    cargo test "${cargo_args[@]}" -p "$pkg" "${target_args[@]}" --no-fail-fast \
    -- "${libtest_args[@]}" >"$logdir/$idx.log" 2>&1
  rc=$?
  printf '%s\t%s\t%s\n' "$pkg::$(tu_unit_label "$pkg" "$flag" "$name")" "$rc" "$(( $(date +%s) - start ))" \
    >"$logdir/$idx.meta"
  return 0
}

# tu_run_pool <logdir> <jobs> <units-file> <cargo-test-args...> -- <libtest-args...>
#   Runs every unit in <units-file>, at most <jobs> at a time.
tu_run_pool() {
  local logdir="$1" jobs_cap="$2" units="$3"
  shift 3
  local idx=0 pkg flag name override
  while IFS="$(printf '\t')" read -r pkg flag name override; do
    [ -n "$pkg" ] || continue
    idx=$((idx + 1))
    while [ "$(jobs -rp | wc -l | tr -d ' ')" -ge "$jobs_cap" ]; do sleep 1; done
    tu_run_unit "$logdir" "$idx" "$pkg" "$flag" "$name" "$override" "$@" </dev/null &
  done <"$units"
  wait
}

# tu_collect <logdir> <out-dir>
#   Reads every unit log and writes <out-dir>/failed.txt, passed.txt and units.tsv
#   (label, exit status, seconds, passed, failed, ignored). Ids are sorted and unique.
tu_collect() {
  local logdir="$1" out="$2" meta label rc secs idx p f i
  : >"$out/failed.txt"; : >"$out/passed.txt"; : >"$out/units.tsv"
  for meta in "$logdir"/*.meta; do
    [ -e "$meta" ] || continue
    idx="$(basename "$meta" .meta)"
    IFS="$(printf '\t')" read -r label rc secs <"$meta"
    awk -v L="$label" '/^test .* \.\.\. FAILED/ { sub(/^test /, ""); sub(/ \.\.\. FAILED.*$/, ""); print L "::" $0 }' \
      "$logdir/$idx.log" >>"$out/failed.txt"
    awk -v L="$label" '/^test .* \.\.\. ok/ { sub(/^test /, ""); sub(/ \.\.\. ok.*$/, ""); print L "::" $0 }' \
      "$logdir/$idx.log" >>"$out/passed.txt"
    p="$(grep -c '^test .* \.\.\. ok' "$logdir/$idx.log")"
    f="$(grep -c '^test .* \.\.\. FAILED' "$logdir/$idx.log")"
    i="$(grep -c '^test .* \.\.\. ignored' "$logdir/$idx.log")"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$rc" "$secs" "$p" "$f" "$i" >>"$out/units.tsv"
    # A unit that exited non-zero with no FAILED test line did not finish its tests (build
    # error, crash, killed): record it as a failed id of its own so it can never pass silently.
    if [ "$rc" != 0 ] && ! grep -q '^test .* \.\.\. FAILED' "$logdir/$idx.log"; then
      printf '%s::<UNIT-DID-NOT-FINISH>\n' "$label" >>"$out/failed.txt"
    fi
  done
  sort -u "$out/failed.txt" -o "$out/failed.txt"
  sort -u "$out/passed.txt" -o "$out/passed.txt"
}

# tu_known <known-reds-file>  ->  sorted ids on stdout (comments and blanks dropped).
tu_known() {
  [ -f "$1" ] || return 0
  sed -e 's/[[:space:]]*#.*$//' -e 's/[[:space:]]*$//' "$1" | grep -v '^$' | sort -u
}

# tu_verdict <out-dir> <known-reds-file> <scope-file|->
#   Prints PASS / NEW-RED / FIXED-RED lines and returns 1 on any NEW-RED.
#   <scope-file> (or "-" for all units) lists unit labels that ran; a known red outside the scope
#   is neither fixed nor red, just not run.
tu_verdict() {
  local out="$1" known="$2" scope="$3" rc=0
  tu_known "$known" >"$out/known.txt"
  if [ "$scope" != "-" ]; then
    # keep only the known reds of units that ran
    awk -F'::' 'NR==FNR { run[$0]=1; next } { u=$1 "::" $2; if (u in run) print }' "$scope" "$out/known.txt" \
      >"$out/known.scoped.txt"
  else
    cp "$out/known.txt" "$out/known.scoped.txt"
  fi
  comm -23 "$out/failed.txt" "$out/known.scoped.txt" >"$out/new-red.txt"
  comm -12 "$out/passed.txt" "$out/known.scoped.txt" >"$out/fixed-red.txt"
  comm -12 "$out/failed.txt" "$out/known.scoped.txt" >"$out/still-red.txt"
  local n_new n_fixed n_still
  n_new="$(wc -l <"$out/new-red.txt" | tr -d ' ')"
  n_fixed="$(wc -l <"$out/fixed-red.txt" | tr -d ' ')"
  n_still="$(wc -l <"$out/still-red.txt" | tr -d ' ')"
  [ "$n_still" = 0 ] || { echo "KNOWN-RED ($n_still, still red as listed):"; sed 's/^/  /' "$out/still-red.txt"; }
  if [ "$n_fixed" != 0 ]; then
    echo "FIXED-RED ($n_fixed listed in known-reds.txt but passing now; delete them from the list):"
    sed 's/^/  /' "$out/fixed-red.txt"
  fi
  if [ "$n_new" != 0 ]; then
    echo "NEW-RED ($n_new failing and not in known-reds.txt):"
    sed 's/^/  /' "$out/new-red.txt"
    rc=1
  fi
  if [ "$rc" = 0 ]; then echo "PASS: no failure outside the known-reds list"; fi
  return "$rc"
}
