#!/usr/bin/env bash
# build-pgo.sh: profile-guided `plinth` build (opt-in; never the default build).
#
#   scripts/build-pgo.sh --train DIR [--train DIR ...] [options]
#
#   --train DIR       a model directory (an --input-models-dir) to train on; repeatable; required.
#                     It is only READ: training output goes to a scratch directory of the script.
#   --profile NAME    cargo profile for both builds (default release-max; `release` also works)
#   --target-dir DIR  cargo target directory (default: $CARGO_TARGET_DIR, else <repo>/target-pgo)
#   --pgo-data DIR    parent directory for profile data (default: <target-dir>/pgo).  Each run makes
#                     its own fresh `run.XXXXXX` subdirectory there (raw profiles, merged profile,
#                     training logs) and deletes nothing it did not create in this run.  The
#                     `run.*` directories of earlier runs stay; delete them yourself.
#   --out FILE        copy the finished binary here (default: <target-dir>/plinth-pgo)
#   --dry-run         validate inputs, print the steps and stop
#
# Four steps: (1) instrumented build, (2) run the training models through it, (3) merge the
# raw profiles, (4) rebuild with `-Cprofile-use`.  Output bytes do not change: PGO moves code
# and inlining only, and the compiler never reassociates or fuses floating-point operations.
# Check the three reference fingerprints on the result before using it.
#
# Training must succeed.  `plinth prepare` exits 0 (every model reached an artifact) or 3 (the
# pre-write physics gates refused at least one model; the search still ran, so the profile is
# still valid; the script prints a note).  Any other status (1 input, 2 hollow, 4 emit, 5 supports,
# a usage error, a crash or a signal) stops the script, and so does a training run that leaves no
# new .profraw file.  The exit codes are PipelineExitCode in crates/plinth-pipeline/src/lib.rs.
#
# Train on models that look like the work.  A binary tuned on two models is slower on a model
# unlike them only if the training was narrow; the lane measurement (docs/development.md,
# "Build flags and allocator") held a model outside the training set.  Rebuild after the hot
# code changes much: a stale profile for moved code is ignored function by function, not an error.
#
# Needs `llvm-profdata` at least as new as rustc's LLVM: the rustup component
# (`rustup component add llvm-tools`) or Homebrew LLVM (`brew install llvm`).
#
# Compiler flags.  Both builds pass `--target <host triple>`, so the flags reach the plinth crates
# and not build scripts or proc macros.  The base flags follow cargo's own rule: if RUSTFLAGS is
# set in the caller's environment it is used as given (so `RUSTFLAGS="-C target-cpu=apple-m1"
# scripts/build-pgo.sh ...` builds a PGO binary for an older Mac); otherwise the `-C target-cpu=`
# value from `.cargo/config.toml` for the host triple is used.  The script appends its own
# `-Cprofile-generate=...` or `-Cprofile-use=...` (absolute paths) to that.  The config's other
# rustflags, if any are ever added, are not read by the script: put them in RUSTFLAGS.
# CARGO_ENCODED_RUSTFLAGS is honoured the same way.  sccache is switched off: its cache key does
# not cover the contents of a .profdata file.
#
# The script does not take the CPU lock.  Wrap it:
#   cpu_lock_run --shared <label> <cap-seconds> -- scripts/build-pgo.sh --train ...
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

die() { echo "build-pgo.sh: $*" >&2; exit 1; }
# absdir DIR: create DIR if needed and print its absolute path.
absdir() { mkdir -p "$1" && (cd "$1" && pwd -P); }

profile=release-max
target_dir="${CARGO_TARGET_DIR:-$REPO/target-pgo}"
pgo_base="" out="" dry=0
train=()
while [ $# -gt 0 ]; do
  case "$1" in
    --train) [ $# -ge 2 ] || { echo "build-pgo.sh: --train needs a value" >&2; exit 2; }
             train+=("$2"); shift ;;
    --profile) [ $# -ge 2 ] || { echo "build-pgo.sh: --profile needs a value" >&2; exit 2; }
               profile="$2"; shift ;;
    --target-dir) [ $# -ge 2 ] || { echo "build-pgo.sh: --target-dir needs a value" >&2; exit 2; }
                  target_dir="$2"; shift ;;
    --pgo-data) [ $# -ge 2 ] || { echo "build-pgo.sh: --pgo-data needs a value" >&2; exit 2; }
                pgo_base="$2"; shift ;;
    --out) [ $# -ge 2 ] || { echo "build-pgo.sh: --out needs a value" >&2; exit 2; }
           out="$2"; shift ;;
    --dry-run) dry=1 ;;
    -h|--help) sed -n '2,/^set -euo/{/^set -euo/!p;}' "$0"; exit 0 ;;
    *) echo "build-pgo.sh: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
if [ ${#train[@]} -eq 0 ]; then
  echo "build-pgo.sh: need at least one --train DIR" >&2; exit 2
fi

# Training directories: must exist; made absolute; only ever read.
train_abs=()
for d in "${train[@]}"; do
  [ -d "$d" ] || die "training directory not found: $d"
  [ -r "$d" ] || die "training directory not readable: $d"
  train_abs+=("$(cd "$d" && pwd -P)")
done

target_dir="$(absdir "$target_dir")"
pgo_base="$(absdir "${pgo_base:-$target_dir/pgo}")"
out="${out:-$target_dir/plinth-pgo}"
mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd -P)/$(basename "$out")"

host="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$host" ] || die "cannot read the host triple from rustc -vV"

# Base flags: the caller's RUSTFLAGS as given, else the config's target-cpu for this host.
if [ -n "${CARGO_ENCODED_RUSTFLAGS:-}" ]; then
  base_flags="$(printf '%s' "$CARGO_ENCODED_RUSTFLAGS" | tr '\037' ' ')"
  flags_from="CARGO_ENCODED_RUSTFLAGS"
elif [ -n "${RUSTFLAGS:-}" ]; then
  base_flags="$RUSTFLAGS"
  flags_from="RUSTFLAGS"
else
  base_flags=""
  flags_from=".cargo/config.toml"
  if [ -f .cargo/config.toml ]; then
    cpu="$(sed -n "/^\[target\.$host\]/,/^\[/{s/.*target-cpu=\([A-Za-z0-9._-]*\).*/\1/p;}" .cargo/config.toml | head -n1)"
    [ -n "$cpu" ] && base_flags="-C target-cpu=$cpu"
  fi
fi

profdata=""
for c in "$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata" \
         "$(command -v llvm-profdata || true)" \
         "$(brew --prefix llvm 2>/dev/null || true)/bin/llvm-profdata"; do
  if [ -n "$c" ] && [ -x "$c" ]; then profdata="$c"; break; fi
done
[ -n "$profdata" ] || die "llvm-profdata not found; run: rustup component add llvm-tools"

bin="$target_dir/$host/$profile/plinth"

if [ "$dry" = 1 ]; then
  echo "host $host; base flags '$base_flags' (from $flags_from); profdata $profdata; profile $profile"
  echo "target dir $target_dir; profile data parent $pgo_base; out $out"
  echo "train (read only): ${train_abs[*]}"
  exit 0
fi

export CARGO_TARGET_DIR="$target_dir"
export RUSTC_WRAPPER="" CARGO_BUILD_RUSTC_WRAPPER=""
unset CARGO_ENCODED_RUSTFLAGS "CARGO_TARGET_$(echo "$host" | tr 'a-z-' 'A-Z_')_RUSTFLAGS" PLINTH_PERF_DIR

build() { # $1 = extra rustflags
  RUSTFLAGS="$base_flags $1" cargo build --locked --profile "$profile" --target "$host" -p plinth-cli
}

# A fresh directory that this run owns.  Nothing outside it is ever deleted.
work="$(mktemp -d "$pgo_base/run.XXXXXX")"
work="$(cd "$work" && pwd -P)"
raw="$work/raw"
mkdir -p "$raw"
echo "profile data for this run: $work"

echo "== PGO 1/4: instrumented build"
build "-Cprofile-generate=$raw"
[ -x "$bin" ] || die "instrumented binary missing: $bin"
"$bin" --version >/dev/null || die "the instrumented binary does not run: $bin"
# The smoke run above wrote a profile too; clear it so only training counts.  A run of the same
# binary updates the SAME .profraw file (its name carries the binary's hash, not the run), so
# "a training run produced a profile" is tested by a file newer than a marker, not by a count.
find "$raw" -maxdepth 1 -name '*.profraw' -delete

echo "== PGO 2/4: training"
n=0
for d in "${train_abs[@]}"; do
  n=$((n + 1))
  echo "  training on $d"
  run_out="$work/out-$n"
  mkdir -p "$run_out"
  log="$work/train-$n-$(basename "$d").log"
  marker="$work/marker-$n"
  : > "$marker"
  rc=0
  "$bin" prepare --input-models-dir "$d" --output saturn4-ultra --out-dir "$run_out" \
    --orient dense --placement sweep > "$log" 2>&1 || rc=$?
  case "$rc" in
    0) ;;
    3) echo "  note: some models were refused by the pre-write gates (exit 3); the profile is still valid" ;;
    *) tail -n 20 "$log" >&2
       die "training on $d failed with exit status $rc (see $log)" ;;
  esac
  [ -n "$(find "$raw" -maxdepth 1 -name '*.profraw' -newer "$marker" | head -n1)" ] \
    || die "training on $d wrote no .profraw file under $raw (see $log)"
  rm -rf "$run_out"
done

echo "== PGO 3/4: merge"
"$profdata" merge -o "$work/merged.profdata" "$raw"
[ -s "$work/merged.profdata" ] || die "merged profile is empty: $work/merged.profdata"

echo "== PGO 4/4: optimised build"
build "-Cprofile-use=$work/merged.profdata"
cp "$bin" "$out"
rm -rf "$raw"   # created by this run, inside $work
echo "built $out ($(shasum -a 256 "$out" | cut -c1-16)); profile $work/merged.profdata"
