"""chitubox_io — the Rust `.chitubox` container reader/writer.

The compiled extension is injected by maturin as ``chitubox_io._native``.
Production code goes through ``support_placement.emit``; this package is the
raw boundary.

Every project this binding writes starts from the crate's embedded EMPTY
template — there is no template parameter — so a write can never carry a
foreign model mesh into its output.
"""
from . import _native  # noqa: F401  (re-export so `import chitubox_io` warms the ext)

#: Typed exception for every ``ChituboxError`` the Rust crate raises. Carries
#: ``.kind`` (the variant name, e.g. ``"VerifyFailed"``) and the crate's own
#: message as ``str(e)``.
ChituboxError = _native.ChituboxError

API_VERSION = _native.API_VERSION
SOURCE_DIGEST = _native.SOURCE_DIGEST

write_project = _native.write_project
read_project = _native.read_project

__all__ = [
    "_native",
    "ChituboxError",
    "API_VERSION",
    "SOURCE_DIGEST",
    "write_project",
    "read_project",
]
