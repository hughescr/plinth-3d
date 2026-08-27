#!/bin/zsh
# Per-binary test baseline with a per-binary timeout, so one runaway test cannot
# hide every binary queued behind it. Records pass/fail/ignore/elapsed per binary.
set -u
cd /Users/craig/code/hughescr/dnd-campaigns/plinth-3d || exit 1
SP=/private/tmp/claude-501/-Users-craig-code-hughescr-dnd-campaigns/310c4c58-ad81-435c-8000-7c11d508a02a/scratchpad/review
OUT=$SP/baseline-per-binary.txt
RAW=$SP/baseline-raw.txt
: > "$OUT"; : > "$RAW"

TIMEOUT=${TIMEOUT:-240}

# Build test binaries and list them.
cargo test --workspace --no-run --message-format=json 2>/dev/null \
  | jq -r 'select(.profile.test == true) | .executable // empty' \
  | sort -u > "$SP/test-binaries.txt"

n=$(wc -l < "$SP/test-binaries.txt" | tr -d ' ')
echo "test binaries: $n  (per-binary timeout ${TIMEOUT}s)" | tee -a "$OUT"
echo "" >> "$OUT"

tp=0; tf=0; ti=0; to=0
while IFS= read -r bin; do
  name=$(basename "$bin" | sed 's/-[0-9a-f]\{16\}$//')
  start=$(date +%s)
  # gtimeout comes from coreutils; fall back to plain run if absent
  if command -v gtimeout >/dev/null; then
    gtimeout "$TIMEOUT" "$bin" --test-threads=4 > "$SP/.bin.out" 2>&1
  else
    "$bin" --test-threads=4 > "$SP/.bin.out" 2>&1
  fi
  rc=$?
  el=$(( $(date +%s) - start ))
  cat "$SP/.bin.out" >> "$RAW"

  line=$(grep -E "^test result:" "$SP/.bin.out" | tail -1)
  p=$(echo "$line" | sed -n 's/.* \([0-9]*\) passed.*/\1/p'); p=${p:-0}
  f=$(echo "$line" | sed -n 's/.* \([0-9]*\) failed.*/\1/p'); f=${f:-0}
  i=$(echo "$line" | sed -n 's/.* \([0-9]*\) ignored.*/\1/p'); i=${i:-0}

  # NOTE: `status` is a read-only variable in zsh (it aliases $?). Do not use it.
  if [ $rc -eq 124 ]; then
    verdict="TIMEOUT after ${TIMEOUT}s"
    to=$((to+1))
  elif [ "$f" -gt 0 ]; then
    verdict="FAILED ${f}"
    tf=$((tf+f))
  else
    verdict="ok"
  fi
  tp=$((tp+p)); ti=$((ti+i))

  printf '%-52s %5ds  %-18s pass=%-5s fail=%-3s ign=%s\n' "$name" "$el" "$verdict" "$p" "$f" "$i" | tee -a "$OUT"

  if [ "$f" -gt 0 ] || [ $rc -eq 124 ]; then
    grep -E "\.\.\. FAILED" "$SP/.bin.out" | sed 's/^/      /' | tee -a "$OUT"
  fi
done < "$SP/test-binaries.txt"

{
  echo ""
  echo "=== BASELINE TOTALS ==="
  echo "binaries=$n  passed=$tp  failed=$tf  ignored=$ti  timed_out=$to"
} | tee -a "$OUT"
rm -f "$SP/.bin.out"
