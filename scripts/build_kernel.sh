#!/usr/bin/env bash
# Temporary one-command build of the ChituBox extension into the venv.
#
#   scripts/build_kernel.sh             # --release (default)
#   scripts/build_kernel.sh dev-opt     # opt-level 2 + debug assertions
#
# maturin is pinned (spec sec 1.2). If it is missing, install it with:
#   .venv/bin/python -m pip install "maturin==1.8.3"
# cargo fetch / pip may need the sandbox disabled (expected — network access).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_PY="$REPO/.venv/bin/python"
MATURIN="$REPO/.venv/bin/maturin"

if [ ! -x "$VENV_PY" ]; then
  echo "ERROR: venv python not found at $VENV_PY" >&2
  exit 1
fi
if [ ! -x "$MATURIN" ]; then
  echo "ERROR: maturin not installed in the venv. Install with:" >&2
  echo "  \"$VENV_PY\" -m pip install \"maturin==1.8.3\"" >&2
  exit 1
fi

# Accept one optional profile word.
PROFILE_ARGS=(--release)
for arg in "$@"; do
  case "$arg" in
    dev-opt)  PROFILE_ARGS=(--profile dev-opt) ;;
    release)  PROFILE_ARGS=(--release) ;;
    *)
      echo "ERROR: unknown argument '$arg'." >&2
      echo "Usage: build_kernel.sh [release|dev-opt]" >&2
      exit 1
      ;;
  esac
done

# Keep the ChituBox extension build pinned to this repository's interpreter.
export PYO3_PYTHON="$VENV_PY"

echo "==> building chitubox_io"
cd "$REPO/rust/chitubox_io"
"$MATURIN" develop "${PROFILE_ARGS[@]}"
echo "built and installed into the venv: chitubox_io"
