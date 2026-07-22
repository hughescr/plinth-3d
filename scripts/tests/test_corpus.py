"""Regression test: cross-validate corpus.analyze() against the figures
already documented (by independent earlier analysis) in
skills/chitubox-support-injection/references/chitubox-format.md's "Real-
scale specimen" section -- 469 supports, 261 raft bridges, ~3.1mm median
tip spacing, bottom quarter carrying ~40% of tips. If this drifts, either
the corpus file changed or corpus.py is misreading the format -- both are
worth knowing loudly, hence asserting instead of just printing.

Skips (does not fail) in two cases:
  - the corpus file isn't present at all -- it's real user data (56MB),
    not fixture data committed to the plugin repo.
  - the corpus file IS present but isn't a parseable calibration specimen
    (too few support groups for corpus.analyze() to calibrate from). This
    is the current state of supported-treant.chitubox as of 2026-07-22:
    the original real-user-data specimen (469 T_TIP-style support groups,
    documented above) was permanently deleted that day, and the path now
    holds a substitute -- our own engine's emitted snapshot, approved by
    Craig as the emit-stage container/template source (see
    test_chitubox_emit.py, which legitimately reads it for that purpose).
    The substitute contains no support groups, so it cannot stand in for
    the calibration corpus; the figures asserted below live on as
    baked-in config.py constants (with docstring provenance) regardless.
"""
import os
import pytest
from support_placement import corpus

CANDIDATE_PATHS = [
    os.environ.get("CALIBRATION_CORPUS_PATH", ""),
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "supported-treant.chitubox"),
]

SUBSTITUTE_REASON = (
    "supported-treant.chitubox is present but is not a parseable calibration "
    "specimen (fewer than 2 support groups found -- corpus.analyze() would "
    "raise ValueError). The original real-user-data specimen documented in "
    "chitubox-format.md's 'Real-scale specimen' section (469 supports, 261 "
    "raft bridges) was permanently deleted on 2026-07-22 and is unrecoverable "
    "(a regenerated file would not be byte-identical). The file at this path "
    "is now a substitute: our own engine's emitted snapshot, approved by "
    "Craig as the emit-stage container/template source for "
    "test_chitubox_emit.py -- not calibration-specimen data. The documented "
    "figures this test cross-checks against remain baked into config.py "
    "(with docstring provenance) and are unaffected at runtime.")


def _find_corpus():
    for p in CANDIDATE_PATHS:
        if p and os.path.isfile(p):
            return os.path.abspath(p)
    return None


def _is_calibration_specimen(path):
    """Cheap pre-check: does this file have enough support groups for
    corpus.analyze() to calibrate from? Mirrors corpus.analyze()'s own
    n < 2 guard without duplicating its fuller analysis."""
    import chitubox_inject as ci
    proj = ci.Project(path)
    groups, _bridges = proj.supports()
    return len(groups) >= 2


CORPUS_PATH = _find_corpus()

_skip_reason = None
if CORPUS_PATH is None:
    _skip_reason = ("calibration corpus (supported-treant.chitubox) not found; "
                     "set CALIBRATION_CORPUS_PATH to run this cross-check")
elif not _is_calibration_specimen(CORPUS_PATH):
    _skip_reason = SUBSTITUTE_REASON

pytestmark = pytest.mark.skipif(_skip_reason is not None, reason=_skip_reason or "")


@pytest.fixture(scope="module")
def stats():
    return corpus.analyze(CORPUS_PATH)


def test_support_and_bridge_counts_match_documented_specimen(stats):
    assert stats["n_supports"] == 469
    assert stats["n_bridges"] == 261


def test_median_tip_spacing_matches_documented_figure(stats):
    median = stats["tip_spacing_mm"]["median"]
    assert 2.8 <= median <= 3.4, f"median spacing {median} outside documented ~3.1mm ballpark"


def test_bottom_quartile_carries_roughly_documented_share(stats):
    share = stats["z_quartile_tip_share"][0]
    assert 0.30 <= share <= 0.50, f"bottom-quartile share {share} outside documented ~40% ballpark"


def test_target_spacing_upper_bound_is_the_corpus_median(stats):
    ub = corpus.target_spacing_upper_bound(stats)
    assert ub == stats["tip_spacing_mm"]["median"]
