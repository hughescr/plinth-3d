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
  # written first with status -1, so a unit that is killed before it ends (the run hit its cap,
  # the machine slept) is still found by tu_collect and judged unfinished, never silently absent
  printf '%s\t%s\t%s\n' "$pkg::$(tu_unit_label "$pkg" "$flag" "$name")" "-1" "0" >"$logdir/$idx.meta"
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
  local logdir="$1" out="$2" meta label rc secs idx p f i nsec=0 summary_failed unfinished
  : >"$out/failed.txt"; : >"$out/passed.txt"; : >"$out/units.tsv"; : >"$out/unfinished.txt"
  rm -rf "$out/sections" "$out/sections.idx"; mkdir -p "$out/sections"; : >"$out/sections.idx"
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
    # The failure text of each failed test, for the known-reds signature check: the libtest
    # section "---- NAME stdout ----" up to the next section, "failures:" or the summary.
    awk -v L="$label" -v dir="$out/sections" -v idx="$out/sections.idx" -v n="$nsec" '
      /^---- .* stdout ----$/ {
        name = $0; sub(/^---- /, "", name); sub(/ stdout ----$/, "", name)
        if (cur != "") close(cur)
        n++; file = dir "/" n; print L "::" name "\t" file >> idx; close(idx)
        printf "" > file; close(file); cur = file; next }
      /^---- .* ----$/ || /^failures:$/ || /^test result:/ { if (cur != "") close(cur); cur = "" }
      cur != "" { print >> cur }
      END { print n > (dir "/.count") }' "$logdir/$idx.log"
    nsec="$(cat "$out/sections/.count" 2>/dev/null || echo "$nsec")"
    # A unit is finished only when libtest printed its final "test result:" line AND the exit
    # status is the one a normal test failure gives (0 passes, 101 fails). A unit that was killed
    # (status -1: never ended), crashed after printing a FAILED line, hit a build error or aborted
    # has no summary or another status: record it as a failed id of its own so it can never pass
    # silently, whatever it printed before it stopped. The failure count in the summary must
    # also equal the FAILED lines seen.
    unfinished=""
    summary_failed="$(sed -n 's/^test result: [A-Z]*\. [0-9]* passed; \([0-9]*\) failed;.*/\1/p' "$logdir/$idx.log" | awk '{s+=$1} END {print s+0}')"
    if ! grep -q '^test result:' "$logdir/$idx.log"; then unfinished="no libtest summary"
    elif [ "$rc" != 0 ] && [ "$rc" != 101 ]; then unfinished="exit status $rc"
    elif [ "$rc" = 0 ] && [ "$f" != 0 ]; then unfinished="exit status 0 with FAILED lines"
    elif [ "$rc" = 101 ] && [ "$f" = 0 ]; then unfinished="exit status 101 with no FAILED line"
    elif [ "$summary_failed" != "$f" ]; then unfinished="summary says $summary_failed failed, $f FAILED lines"
    fi
    if [ -n "$unfinished" ]; then
      printf '%s::<UNIT-DID-NOT-FINISH>\n' "$label" >>"$out/failed.txt"
      printf '%s::<UNIT-DID-NOT-FINISH>\t%s\n' "$label" "$unfinished" >>"$out/unfinished.txt"
    fi
  done
  sort -u "$out/failed.txt" -o "$out/failed.txt"
  sort -u "$out/passed.txt" -o "$out/passed.txt"
}

# tu_known <known-reds-file>  ->  on stdout, sorted by id, one line per known red:
#   <id> TAB <signatures>
# Source lines are `<id> => <sig> ;; <sig>   # reason`. A signature is a fixed string that the
# failure text of the test must contain; several (separated by ` ;; `) must all be present.
# A line with no `=> ` has no signature and matches on id alone. Comments (whitespace then `#`)
# and blank lines are dropped, so a signature must not contain ` #`.
tu_known() {
  [ -f "$1" ] || return 0
  sed -e 's/^[[:space:]]*#.*$//' -e 's/[[:space:]][[:space:]]*#.*$//' -e 's/[[:space:]]*$//' "$1" \
    | grep -v '^$' \
    | awk '{ i = index($0, " => "); if (i) { id = substr($0, 1, i - 1); sig = substr($0, i + 4) } else { id = $0; sig = "" }
             print id "\t" sig }' \
    | sort -t "$(printf '\t')" -k1,1
}

# tu_signature_ok <section-file> <signatures>  ->  0 if every ` ;; `-separated signature is a
# substring of the section; a missing section file counts as empty text.
tu_signature_ok() {
  local section="$1" sigs="$2" sig rest
  [ -n "$sigs" ] || return 0
  [ -f "$section" ] || return 1
  rest="$sigs"
  while [ -n "$rest" ]; do
    case "$rest" in
      *' ;; '*) sig="${rest%% ;; *}"; rest="${rest#* ;; }" ;;
      *) sig="$rest"; rest="" ;;
    esac
    grep -F -q -- "$sig" "$section" || return 1
  done
  return 0
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
    awk -F'\t' 'NR==FNR { run[$0]=1; next } { split($1, p, "::"); u = p[1] "::" p[2]; if (u in run) print }' \
      "$scope" "$out/known.txt" >"$out/known.scoped.tsv"
  else
    cp "$out/known.txt" "$out/known.scoped.tsv"
  fi
  cut -f1 "$out/known.scoped.tsv" | sort -u >"$out/known.scoped.txt"
  comm -23 "$out/failed.txt" "$out/known.scoped.txt" >"$out/new-red.txt"
  comm -12 "$out/passed.txt" "$out/known.scoped.txt" >"$out/fixed-red.txt"
  comm -12 "$out/failed.txt" "$out/known.scoped.txt" >"$out/red-listed.txt"
  # A listed red must fail the way it is listed: its failure text has to contain the signature.
  # Failing some other way is a different defect hiding behind a known name.
  : >"$out/still-red.txt"; : >"$out/changed-red.txt"
  local id sigs section
  while IFS= read -r id; do
    sigs="$(awk -F'\t' -v id="$id" '$1 == id { print $2; exit }' "$out/known.scoped.tsv")"
    section="$(awk -F'\t' -v id="$id" '$1 == id { print $2; exit }' "$out/sections.idx" 2>/dev/null)"
    if tu_signature_ok "$section" "$sigs"; then echo "$id" >>"$out/still-red.txt"
    else printf '%s\t%s\n' "$id" "$sigs" >>"$out/changed-red.txt"; fi
  done <"$out/red-listed.txt"
  local n_new n_fixed n_still n_changed
  n_new="$(wc -l <"$out/new-red.txt" | tr -d ' ')"
  n_fixed="$(wc -l <"$out/fixed-red.txt" | tr -d ' ')"
  n_still="$(wc -l <"$out/still-red.txt" | tr -d ' ')"
  n_changed="$(wc -l <"$out/changed-red.txt" | tr -d ' ')"
  [ "$n_still" = 0 ] || { echo "KNOWN-RED ($n_still, still red as listed):"; sed 's/^/  /' "$out/still-red.txt"; }
  if [ "$n_changed" != 0 ]; then
    echo "CHANGED-RED ($n_changed listed in known-reds.txt but failing differently; read the log, then fix the test or update the signature):"
    awk -F'\t' '{ print "  " $1 "\n      expected: " $2 }' "$out/changed-red.txt"
    rc=1
  fi
  if [ -s "$out/unfinished.txt" ]; then
    echo "UNFINISHED (a test binary did not complete; its results are not trusted):"
    awk -F'\t' '{ print "  " $1 " (" $2 ")" }' "$out/unfinished.txt"
  fi
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
