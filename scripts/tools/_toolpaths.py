"""Path plumbing shared by the operator/analysis tools in this directory.

These tools used to hardcode absolute paths into whichever session scratchpad
and whichever worktree they happened to be written in, which is why they got
COPIED next to each run instead of edited in place. Everything here is derived
from the module's own location or overridable by environment variable, so a
tool keeps working from any checkout.

env:
  CENSUS_SCRATCH  scratch/output directory (default: the system temp dir)
  TREANT_STL      absolute path to the source STL (default: probed, see below)
"""
import os
import tempfile

# scripts/tools/_toolpaths.py -> scripts/tools -> scripts -> repo root
TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
SCRIPTS_DIR = os.path.dirname(TOOLS_DIR)
REPO_ROOT = os.path.dirname(SCRIPTS_DIR)

STL_NAME = "Weeping Treant v1.2.stl"


def scratch_dir():
    """Directory for run pickles and tool outputs. Never inside the repo."""
    return os.environ.get("CENSUS_SCRATCH") or tempfile.gettempdir()


def resolve_stl(name=STL_NAME):
    """Absolute path to the source STL.

    $TREANT_STL wins; otherwise probe the repo root and the two enclosing
    directories -- worktrees under <main>/.worktrees/<wt> share the model with
    the main working tree rather than carrying a copy.
    """
    if os.environ.get("TREANT_STL"):
        return os.environ["TREANT_STL"]
    for cand in (os.path.join(REPO_ROOT, name),
                 os.path.join(REPO_ROOT, "..", "..", name),   # main working tree
                 os.path.join(REPO_ROOT, "..", name)):
        if os.path.exists(cand):
            return os.path.abspath(cand)
    return os.path.join(REPO_ROOT, name)
