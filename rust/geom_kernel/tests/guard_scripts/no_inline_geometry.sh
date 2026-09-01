#!/bin/sh
# Guard 2 extraction pass: find every raw dot-product / cross-product /
# squared-length spelling under the given roots, via ast-grep structural
# patterns (not text grep -- these match the AST shape regardless of the
# metavariable names the author picked).
#
# This script only EXTRACTS candidate hits as "<pattern label>\t<file>\t<0
# indexed start line>" rows, one per line, plus one final
# "SCANNED_FILES\t<n>" row. It does NOT decide exemptions (marker check,
# test-code MAY-STAY) -- that lives in the calling Rust test, in one place,
# so both the real guard and its own self-test share exactly one
# implementation of the classification rule.
#
# Usage: no_inline_geometry.sh <root-dir> [<root-dir> ...]
#
# Implementation note (verified on a known case, per the brief -- see
# kn_no_inline_geometry_guard.rs's self-test for the executable proof):
# ast-grep's `--json=compact` reports 0-INDEXED line numbers. This script
# passes that 0-indexed number through UNCHANGED; the caller is responsible
# for the +1 conversion when it wants a 1-indexed source line, and for
# reading the line ABOVE a match (which is exactly the 0-indexed number
# itself, per `sed -n "${l}p"` where $l is the 0-indexed match line).

set -eu

if ! command -v ast-grep >/dev/null 2>&1; then
    echo "no_inline_geometry.sh: ast-grep is not on PATH -- install it \
(e.g. 'brew install ast-grep') before running this guard. A missing tool is \
not the same as a clean scan; refusing to report false-green." >&2
    exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
    echo "no_inline_geometry.sh: jq is not on PATH -- install it \
(e.g. 'brew install jq') before running this guard." >&2
    exit 2
fi

if [ "$#" -eq 0 ]; then
    echo "no_inline_geometry.sh: no scan roots given" >&2
    exit 2
fi

for root in "$@"; do
    if [ ! -d "$root" ]; then
        echo "no_inline_geometry.sh: scan root '$root' is not a directory" >&2
        exit 2
    fi
done

# Independent file count, gated on separately from the pattern matches below
# (the project's own xargs-incident rule: a matcher that silently touched
# zero files must not be read as "clean").
scanned=$(find "$@" -type f -name '*.rs' | wc -l | tr -d ' ')
if [ "$scanned" -eq 0 ]; then
    echo "no_inline_geometry.sh: scanned ZERO .rs files under: $* -- this is \
a broken walker, not a clean workspace" >&2
    exit 2
fi

DOT3D='$A[0] * $B[0] + $A[1] * $B[1] + $A[2] * $B[2]'
CROSS3D='[$A[1] * $B[2] - $A[2] * $B[1], $A[2] * $B[0] - $A[0] * $B[2], $A[0] * $B[1] - $A[1] * $B[0]]'
SQLEN3D='$A[0] * $A[0] + $A[1] * $A[1] + $A[2] * $A[2]'
SQLEN2D='$A[0] * $A[0] + $A[1] * $A[1]'

emit_pattern_hits() {
    label="$1"
    pattern="$2"
    shift 2
    ast-grep run -p "$pattern" -l rust "$@" --json=compact 2>/dev/null \
        | jq -r --arg label "$label" \
            '.[] | [$label, .file, (.range.start.line | tostring)] | @tsv'
}

emit_pattern_hits "dot-product-3d" "$DOT3D" "$@"
emit_pattern_hits "cross-product-3d" "$CROSS3D" "$@"
emit_pattern_hits "squared-length-3d" "$SQLEN3D" "$@"
emit_pattern_hits "squared-length-2d" "$SQLEN2D" "$@"

printf 'SCANNED_FILES\t%s\n' "$scanned"
