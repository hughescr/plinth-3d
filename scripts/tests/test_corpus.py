"""Regression test: cross-validate corpus.analyze() against the figures
already documented (by independent earlier analysis) in
skills/chitubox-support-injection/references/chitubox-format.md's "Real-
scale specimen" section -- 469 supports, 261 raft bridges, ~3.1mm median
tip spacing, bottom quarter carrying ~40% of tips. If this drifts, either
the corpus file changed or corpus.py is misreading the format -- both are
worth knowing loudly, hence asserting instead of just printing.

Skips (does not fail) when the corpus file isn't present -- it's real
user data (56MB), not fixture data committed to the plugin repo.
"""
import os
import pytest
from support_placement import corpus

CANDIDATE_PATHS = [
    os.environ.get("CALIBRATION_CORPUS_PATH", ""),
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "supported-treant.chitubox"),
]


def _find_corpus():
    for p in CANDIDATE_PATHS:
        if p and os.path.isfile(p):
            return os.path.abspath(p)
    return None


CORPUS_PATH = _find_corpus()

pytestmark = pytest.mark.skipif(
    CORPUS_PATH is None,
    reason="calibration corpus (supported-treant.chitubox) not found; "
           "set CALIBRATION_CORPUS_PATH to run this cross-check")


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
