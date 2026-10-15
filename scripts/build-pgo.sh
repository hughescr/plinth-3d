#!/usr/bin/env bash
# build-pgo.sh: profile-guided `plinth` build (opt-in; never the default build).
#
#   scripts/build-pgo.sh --train DIR [--train DIR ...] [options]
#
#   --train DIR       a model directory (an --input-models-dir) to train on; repeatable; required
#   --profile NAME    cargo profile for both builds (default release-max; `release` also works)
#   --target-dir DIR  cargo target directory (default: $CARGO_TARGET_DIR, else <repo>/target-pgo)
#   --pgo-data DIR    raw and merged profiles (default: <target-dir>/pgo)
#   --out FILE        copy the finished binary here (default: <target-dir>/plinth-pgo)
#   --dry-run         print the steps and stop
#
# Four steps: (1) instrumented build, (2) run the training models through it, (3) merge the
# raw profiles, (4) rebuild with `-Cprofile-use`.  Output bytes do not change: PGO moves code
# and inlining only, and the compiler never reassociates or fuses floating-point operations.
# Check the three reference fingerprints on the result before using it.
#
# Train on models that look like the work.  A binary tuned on two models is slower on a model
# unlike them only if the training was narrow; the lane measurement (docs/development.md,
# "Build flags and allocator") held a model outside the training set.  Rebuild after the hot
# code changes much: a stale profile for moved code is ignored function by function, not an error.
#
# Needs `llvm-profdata` at least as new as rustc's LLVM: the rustup component
# (`rustup component add llvm-tools`) or Homebrew LLVM (`brew install llvm`).
#
# `--target <host triple>` keeps the instrumentation flags off build scripts and proc macros.
# They go in through the target's rustflags environment variable, which REPLACES the rustflags in
# `.cargo/config.toml`; the script copies the `-C target-cpu=...` pair from that file so the two
# agree.  sccache is switched off: its cache key does not cover the contents of a .profdata file.
#
# The script does not take the CPU lock.  Wrap it:
#   cpu_lock_run --shared <label> <cap-seconds> -- scripts/build-pgo.sh --train ...
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

profile=release-max
target_dir="${CARGO_TARGET_DIR:-$REPO/target-pgo}"
pgo_data="" out="" dry=0
train=()
while [ $# -gt 0 ]; do
  case "$1" in
    --train) train+=("$2"); shift ;;
    --profile) profile="$2"; shift ;;
    --target-dir) target_dir="$2"; shift ;;
    --pgo-data) pgo_data="$2"; shift ;;
    --out) out="$2"; shift ;;
    --dry-run) dry=1 ;;
    -h|--help) sed -n '2,31p' "$0"; exit 0 ;;
    *) echo "build-pgo.sh: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
if [ ${#train[@]} -eq 0 ]; then
  echo "build-pgo.sh: need at least one --train DIR" >&2; exit 2
fi
pgo_data="${pgo_data:-$target_dir/pgo}"
out="${out:-$target_dir/plinth-pgo}"

host="$(rustc -vV | sed -n 's/^host: //p')"
triple_env="CARGO_TARGET_$(echo "$host" | tr 'a-z-' 'A-Z_')_RUSTFLAGS"
cpu_flags=""
if [ -f .cargo/config.toml ]; then
  cpu="$(sed -n "/^\[target\.$host\]/,/^\[/{s/.*target-cpu=\([A-Za-z0-9._-]*\).*/\1/p;}" .cargo/config.toml | head -n1)"
  [ -n "$cpu" ] && cpu_flags="-C target-cpu=$cpu"
fi

profdata=""
for c in "$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata" \
         "$(command -v llvm-profdata || true)" \
         "$(brew --prefix llvm 2>/dev/null || true)/bin/llvm-profdata"; do
  if [ -n "$c" ] && [ -x "$c" ]; then profdata="$c"; break; fi
done
if [ -z "$profdata" ]; then
  echo "build-pgo.sh: llvm-profdata not found; run: rustup component add llvm-tools" >&2; exit 1
fi

export CARGO_TARGET_DIR="$target_dir"
export RUSTC_WRAPPER="" CARGO_BUILD_RUSTC_WRAPPER=""
unset RUSTFLAGS CARGO_ENCODED_RUSTFLAGS
bin="$target_dir/$host/$profile/plinth"

build() { # $1 = extra rustflags
  env "$triple_env=$cpu_flags $1" cargo build --locked --profile "$profile" --target "$host" -p plinth-cli
}

if [ "$dry" = 1 ]; then
  echo "host $host; flags '$cpu_flags'; profdata $profdata; profile $profile; train: ${train[*]}"
  exit 0
fi

rm -rf "$pgo_data"; mkdir -p "$pgo_data/raw"
echo "== PGO 1/4: instrumented build"
build "-Cprofile-generate=$pgo_data/raw"
echo "== PGO 2/4: training"
for d in "${train[@]}"; do
  echo "  training on $d"
  run_out="$(mktemp -d "${TMPDIR:-/tmp}/build-pgo-train.XXXXXX")"
  "$bin" prepare --input-models-dir "$d" --output saturn4-ultra --out-dir "$run_out" \
    --orient dense --placement sweep > "$pgo_data/train-$(basename "$d").log" 2>&1 || true
  rm -rf "$run_out"
done
echo "== PGO 3/4: merge"
"$profdata" merge -o "$pgo_data/merged.profdata" "$pgo_data/raw"
echo "== PGO 4/4: optimised build"
build "-Cprofile-use=$pgo_data/merged.profdata"
cp "$bin" "$out"
echo "built $out ($(shasum -a 256 "$out" | cut -c1-16))"
