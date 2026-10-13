#!/usr/bin/env bash
# test-units-selftest.sh: checks the verdict logic of scripts/lib/test-units.sh on synthetic unit
# logs, with no cargo and no lock. Run it after any change to that library. Exits 0 when every
# case gives the verdict listed below, 1 otherwise.
#
#   case                                        expected
#   a known red failing with its signature      PASS (exit 0)
#   a known red failing with another message    CHANGED-RED (exit 1)
#   a known red, no panic text at all           CHANGED-RED (exit 1)
#   a known red printed, then the binary aborts NEW-RED UNIT-DID-NOT-FINISH (exit 1)
#   a unit killed before it ended (status -1)   NEW-RED UNIT-DID-NOT-FINISH (exit 1)
#   a build error (status 101, no test lines)   NEW-RED UNIT-DID-NOT-FINISH (exit 1)
#   a summary that disagrees with the FAILED lines  NEW-RED UNIT-DID-NOT-FINISH (exit 1)
#   an unlisted failure                         NEW-RED (exit 1)
#   a listed red that passes                    FIXED-RED, exit 0
export LC_ALL=C
HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/test-units.sh
. "$HERE/lib/test-units.sh"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/tu-selftest.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
FAILS=0
ID="plinth-sweep::it::prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie"
cat >"$WORK/known.txt" <<EOF
# synthetic known-reds list
$ID => the full coarse ladder placed 0 braces, 5 ties ;; prefilter_no_brace.rs  # reason
EOF

# case <name> <expected-exit> <expected-text> <rc> <log-body>
case_run() {
  local name="$1" want_rc="$2" want_text="$3" unit_rc="$4" body="$5" dir out rc
  dir="$WORK/$name"; mkdir -p "$dir/logs" "$dir/out"
  printf '%s' "$body" >"$dir/logs/1.log"
  printf 'plinth-sweep::it\t%s\t1\n' "$unit_rc" >"$dir/logs/1.meta"
  tu_collect "$dir/logs" "$dir/out"
  out="$(tu_verdict "$dir/out" "$WORK/known.txt" - 2>&1)"; rc=$?
  if [ "$rc" = "$want_rc" ] && printf '%s' "$out" | grep -q -- "$want_text"; then
    echo "ok   $name"
  else
    echo "FAIL $name: wanted exit $want_rc and '$want_text', got exit $rc:"; printf '%s\n' "$out" | sed 's/^/       /'
    FAILS=$((FAILS + 1))
  fi
}

PANIC="
failures:

---- prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie stdout ----

thread 'prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie' (1) panicked at crates/plinth-sweep/tests/it/prefilter_no_brace.rs:133:5:
path hit: the full coarse ladder placed 0 braces, 5 ties


failures:
    prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
"
RED_LINE="running 1 test
test prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie ... FAILED
"

case_run known-red-same-signature 0 "PASS" 101 "$RED_LINE$PANIC"
case_run known-red-other-message 1 "CHANGED-RED" 101 "running 1 test
test prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie ... FAILED

failures:

---- prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie stdout ----

thread 'x' (1) panicked at head_join.rs:1:1:
bad new geometry invariant


failures:
    prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
"
case_run known-red-no-panic-text 1 "CHANGED-RED" 101 "running 1 test
test prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
"
case_run abort-after-known-red 1 "UNIT-DID-NOT-FINISH" 134 "running 2 tests
test prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie ... FAILED
test a_new_unlisted_test ... error: process did not exit successfully (signal: 6, SIGABRT)
"
case_run abort-with-exit-101 1 "UNIT-DID-NOT-FINISH" 101 "running 2 tests
test prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie ... FAILED
test a_new_unlisted_test ...
"
case_run killed-before-the-end 1 "UNIT-DID-NOT-FINISH" -1 "running 3 tests
test a ... ok
"
case_run build-error 1 "UNIT-DID-NOT-FINISH" 101 "error[E0425]: cannot find value \`x\` in this scope
error: could not compile \`plinth-sweep\` (test \"it\") due to 1 previous error
"
case_run summary-disagrees 1 "UNIT-DID-NOT-FINISH" 101 "running 2 tests
test a ... FAILED
test b ... ok

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
"
case_run unlisted-failure 1 "NEW-RED" 101 "running 1 test
test some_other_test ... FAILED

failures:

---- some_other_test stdout ----

thread 'some_other_test' (1) panicked at x.rs:1:1:
boom

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
"
case_run listed-red-now-passes 0 "FIXED-RED" 0 "running 1 test
test prefilter_no_brace::the_prefilter_measures_no_brace_mast_or_tie ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s
"

if [ "$FAILS" = 0 ]; then echo "test-units selftest: all cases pass"; exit 0; fi
echo "test-units selftest: $FAILS case(s) FAILED"; exit 1
