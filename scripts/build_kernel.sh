#!/usr/bin/env bash
# One-command build of the geom_kernel PyO3 extension into the venv (spec sec 1.2).
#
#   scripts/build_kernel.sh            # --release (default; 10-50x faster)
#   scripts/build_kernel.sh dev-opt    # opt-level 2 + debug assertions
#
# maturin is pinned (spec sec 1.2). If it is missing, install it with:
#   .venv/bin/python -m pip install "maturin==1.8.3"
# cargo fetch / pip may need the sandbox disabled (expected — network access).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_PY="$REPO/.venv/bin/python"
MATURIN="$REPO/.venv/bin/maturin"
CRATE="$REPO/rust/geom_kernel"

if [ ! -x "$VENV_PY" ]; then
  echo "ERROR: venv python not found at $VENV_PY" >&2
  exit 1
fi
if [ ! -x "$MATURIN" ]; then
  echo "ERROR: maturin not installed in the venv. Install with:" >&2
  echo "  \"$VENV_PY\" -m pip install \"maturin==1.8.3\"" >&2
  exit 1
fi

# PyO3 needs to know which interpreter to build against for `cargo test`; for
# maturin it auto-detects the venv it runs from.
export PYO3_PYTHON="$VENV_PY"

PROFILE_ARGS=(--release)
if [ "${1:-}" = "dev-opt" ]; then
  PROFILE_ARGS=(--profile dev-opt)
fi

cd "$CRATE"
"$MATURIN" develop "${PROFILE_ARGS[@]}"
echo "geom_kernel built and installed into the venv."
