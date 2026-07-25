#!/usr/bin/env bash
# One-command build of this repo's PyO3 extensions into the venv (spec sec 1.2).
#
#   scripts/build_kernel.sh                 # both crates, --release (default)
#   scripts/build_kernel.sh dev-opt         # both crates, opt-level 2 + debug assertions
#   scripts/build_kernel.sh geom_kernel     # one crate only
#   scripts/build_kernel.sh chitubox_io dev-opt
#
# Crates (each is a maturin mixed-layout project under rust/):
#   geom_kernel  -> geom_kernel._native   (ExactSurface hot core, spec sec 1.2)
#   chitubox_io  -> chitubox_io._native   (.chitubox container read/write)
#
# maturin is pinned (spec sec 1.2). If it is missing, install it with:
#   .venv/bin/python -m pip install "maturin==1.8.3"
# cargo fetch / pip may need the sandbox disabled (expected — network access).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_PY="$REPO/.venv/bin/python"
MATURIN="$REPO/.venv/bin/maturin"
ALL_CRATES=(geom_kernel chitubox_io)

if [ ! -x "$VENV_PY" ]; then
  echo "ERROR: venv python not found at $VENV_PY" >&2
  exit 1
fi
if [ ! -x "$MATURIN" ]; then
  echo "ERROR: maturin not installed in the venv. Install with:" >&2
  echo "  \"$VENV_PY\" -m pip install \"maturin==1.8.3\"" >&2
  exit 1
fi

# Args in any order: a profile word and/or one or more crate names.
PROFILE_ARGS=(--release)
CRATES=()
for arg in "$@"; do
  case "$arg" in
    dev-opt)  PROFILE_ARGS=(--profile dev-opt) ;;
    release)  PROFILE_ARGS=(--release) ;;
    geom_kernel|chitubox_io) CRATES+=("$arg") ;;
    *)
      echo "ERROR: unknown argument '$arg'." >&2
      echo "Usage: build_kernel.sh [geom_kernel|chitubox_io ...] [release|dev-opt]" >&2
      exit 1
      ;;
  esac
done
if [ ${#CRATES[@]} -eq 0 ]; then
  CRATES=("${ALL_CRATES[@]}")
fi

# PyO3 needs to know which interpreter to build against for `cargo test`; for
# maturin it auto-detects the venv it runs from.
export PYO3_PYTHON="$VENV_PY"

for crate in "${CRATES[@]}"; do
  echo "==> building $crate"
  cd "$REPO/rust/$crate"
  "$MATURIN" develop "${PROFILE_ARGS[@]}"
done
echo "built and installed into the venv: ${CRATES[*]}"
