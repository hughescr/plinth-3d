#!/usr/bin/env bash
# alloc-profile.sh: where does a run allocate?  Builds the counting binary, or turns
# its sampled call-site table into a ranked report.  Apple Silicon macOS only (frame
# pointer walk, `atos`).  Never time the counting binary: it is slower than release.
#
#   scripts/alloc-profile.sh build [--target-dir DIR]     -> DIR/release/plinth (default <repo>/target-alloc)
#   scripts/alloc-profile.sh run   BIN OUTDIR -- ARGS...   run BIN with ARGS..., write OUTDIR/{counters,sites}.tsv
#   scripts/alloc-profile.sh report BIN SITES [FIRST_WINDOW [LAST_WINDOW]] [--top N]
#
# `report` prints, for the sample windows chosen (10 s each, counted from process start;
# default: all), the estimated allocation count and bytes per call site, ranked by count
# and by bytes.  A call site is "leaf <- owner": the nearest frame that is not allocator
# plumbing (Vec/String/Box/format internals), and the nearest frame in a plinth, geom_kernel,
# chitubox_io or dependency crate.  Names come from the symbol table (the release binary
# keeps it; inlined callers are folded into their parent).  Install `rustfilt`
# (`cargo install rustfilt`) for readable names; without it names stay mangled.
#
# The counting build is `--features alloc-count` of plinth-cli (crates/plinth-cli/src/alloc_count.rs).
# Estimated counts are samples x stride (1 in 256 allocations up to 1 KiB, 1 in 16 above), so a
# site is trustworthy from about 20 samples (5,000 allocations); small sites are noise.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cmd="${1:-}"; shift || true
case "$cmd" in
  build)
    target_dir="$REPO/target-alloc"
    if [ "${1:-}" = "--target-dir" ]; then target_dir="$2"; fi
    cd "$REPO"
    CARGO_TARGET_DIR="$target_dir" cargo build --release -p plinth-cli --features alloc-count
    echo "built $target_dir/release/plinth"
    ;;
  run)
    bin="$1"; out="$2"; shift 2
    [ "${1:-}" = "--" ] && shift
    mkdir -p "$out"
    PLINTH_ALLOC_LOG="$out/counters.tsv" PLINTH_ALLOC_SITES="$out/sites.tsv" "$bin" "$@"
    ;;
  report)
    bin="$1"; sites="$2"; shift 2
    first=0; last=999999; top=25; pos=0
    while [ $# -gt 0 ]; do
      case "$1" in
        --top) top="$2"; shift ;;
        *) if [ "$pos" = 0 ]; then first="$1"; else last="$1"; fi; pos=$((pos + 1)) ;;
      esac
      shift
    done
    slide="$(head -n1 "$sites" | cut -f2)"
    load="$(printf '0x%x' $((0x100000000 + slide)))"
    work="$(mktemp -d "${TMPDIR:-/tmp}/alloc-profile.XXXXXX")"
    trap 'rm -rf "$work"' EXIT
    tail -n +2 "$sites" | cut -f4 | tr ' ' '\n' | sort -u | grep -v '^$' > "$work/pcs"
    sed 's/^/0x/' "$work/pcs" | xargs atos -o "$bin" -arch arm64 -l "$load" > "$work/names.raw"
    if command -v rustfilt > /dev/null; then rustfilt < "$work/names.raw" > "$work/names"; else cp "$work/names.raw" "$work/names"; fi
    paste "$work/pcs" "$work/names" > "$work/map"
    awk -F'\t' -v first="$first" -v last="$last" -v top="$top" '
      function clean(n) {
        sub(/ \(in [^)]*\).*$/, "", n); gsub(/::h[0-9a-f]{16}/, "", n); gsub(/\[[0-9a-f]+\]/, "", n)
        return n
      }
      function plumbing(n) {
        if (owner(n)) return 0
        return n ~ /^(alloc::(alloc|raw_vec|vec|boxed|string|fmt|slice|borrow|collections|sync|rc)|core::(ptr|iter|ops|result|option|slice|clone)|std::|hashbrown|<alloc::|<core::|<std::|<hashbrown|_?_rust|rayon_core|rayon::iter|rayon::slice|rayon::vec|<rayon|<\[|<Vec|<&|<I as|<F as|core::fmt|_malloc|_calloc|_realloc|_posix)/
      }
      function owner(n) {
        return n ~ /^[<&]*(plinth|geom_kernel|chitubox_io|i_overlay|kiddo|meshopt|scirs2|nalgebra|robust|fast_surface_nets)/
      }
      FNR == NR { name[$1] = clean($2); next }
      /^#/ { next }
      {
        w = $1 + 0
        if (w < first || w > last) next
        n = split($4, pc, " ")
        leaf = ""; own = ""
        for (i = 1; i <= n; i++) {
          nm = (pc[i] in name) ? name[pc[i]] : pc[i]
          if (leaf == "" && !plumbing(nm)) leaf = nm
          if (own == "" && owner(nm)) own = nm
        }
        if (own == "") {
          # No project frame within the sampled depth: show the innermost distinct frames.
          own = "[no owner frame in depth]"; shown = 0; prev = ""
          for (i = 1; i <= n && shown < 3; i++) {
            nm = (pc[i] in name) ? name[pc[i]] : pc[i]
            if (nm != prev) { own = own " " nm " |"; shown++ }
            prev = nm
          }
        }
        if (leaf == "") leaf = "(all frames are allocator plumbing)"
        key = leaf " <- " own
        cnt[key] += $2; byt[key] += $3; tc += $2; tb += $3
        wc[w] += $2; wb[w] += $3
      }
      END {
        printf "estimated: %.1f M allocations, %.2f GB, mean %.0f B\n", tc / 1e6, tb / 1e9, (tc ? tb / tc : 0)
        print "\nper 10 s window (allocs M, GB):"
        fflush()
        wcmd = "sort -t\"\t\" -k1,1n | cut -f2"
        for (w in wc) printf "%d\t  window %d: %.1f M, %.2f GB\n", w, w, wc[w] / 1e6, wb[w] / 1e9 | wcmd
        close(wcmd)
        tcmd = "sort -t\"\t\" -k1,1nr | head -n " top
        print "\ntop by count:   M allocs   share  mean B   site"
        fflush()
        for (k in cnt) printf "%.2f\t%.1f%%\t%.0f\t%s\n", cnt[k] / 1e6, 100 * cnt[k] / tc, byt[k] / cnt[k], k | tcmd
        close(tcmd)
        print "\ntop by bytes:   GB   share  mean B   site"
        fflush()
        for (k in cnt) printf "%.3f\t%.1f%%\t%.0f\t%s\n", byt[k] / 1e9, 100 * byt[k] / tb, byt[k] / cnt[k], k | tcmd
        close(tcmd)
      }' "$work/map" "$sites"
    ;;
  *)
    sed -n '2,19p' "$0"; exit 2 ;;
esac
