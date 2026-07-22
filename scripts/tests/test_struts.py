import numpy as np
import pytest
from support_placement import struts


def test_nearby_low_delicacy_anchor_wins_over_long_plate_route(cfg):
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    plate_free_length = 55.0  # a long way down to the plate
    sample_points = np.array([[3.0, 0.0, 45.0], [0.0, 0.0, -5.0]])
    sample_delicacy = np.array([0.1, 0.0])
    decision = struts.choose_anchor(contact, plate_free_length, contact_delicacy=0.2,
                                     sample_points=sample_points,
                                     sample_delicacy=sample_delicacy, cfg=cfg)
    assert decision["kind"] == "model"
    assert decision["saved_mm"] >= cfg.strut_min_length_saved_mm


def test_no_candidates_falls_back_to_plate(cfg):
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    decision = struts.choose_anchor(contact, 55.0, 0.2,
                                     sample_points=np.zeros((0, 3)),
                                     sample_delicacy=np.zeros(0), cfg=cfg)
    assert decision["kind"] == "plate"
    assert decision["anchor"] is None


def test_insufficient_savings_stays_on_plate(cfg):
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    plate_free_length = 20.0
    # candidate saves only ~2mm -- below strut_min_length_saved_mm default (8mm)
    sample_points = np.array([[18.0, 0.0, 50.0]])
    sample_delicacy = np.array([0.0])
    decision = struts.choose_anchor(contact, plate_free_length, 0.0,
                                     sample_points=sample_points,
                                     sample_delicacy=sample_delicacy, cfg=cfg)
    assert decision["kind"] == "plate"


def test_delicate_candidate_anchor_is_excluded(cfg):
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    sample_points = np.array([[3.0, 0.0, 45.0]])
    sample_delicacy = np.array([0.9])  # too delicate to strut into
    found = struts.find_model_anchor(contact, sample_points, sample_delicacy, cfg)
    assert found is None


# ------------------------------------------------------ BUG 1 regression --
def test_find_model_anchor_excludes_near_horizontal_prefers_cone_feasible(cfg):
    """BUG 1 regression (struts.py:22, forensics-diagnosed): the pre-fix
    find_model_anchor picked the single globally-nearest qualifying
    point with NO lean/cone filter at all. Here the nearest point is
    near-horizontal (~89deg lean -- exactly the failure mode measured on
    the Treant: median 76.7deg, 71% cone-rejected at the builder with no
    retry). The fix must skip it in favor of the farther, cone-feasible
    candidate rather than handing the builder a doomed anchor."""
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    near_horizontal = np.array([5.0, 0.0, 49.9])   # d ~5.00mm, lean ~88.9deg
    far_cone_ok = np.array([3.0, 0.0, 40.0])       # d ~10.44mm, lean ~16.7deg
    sample_points = np.array([near_horizontal, far_cone_ok])
    sample_delicacy = np.array([0.0, 0.0])
    found = struts.find_model_anchor(contact, sample_points, sample_delicacy, cfg)
    assert found is not None
    anchor_pt, _delicacy, _len = found
    assert np.allclose(anchor_pt, far_cone_ok)


def test_find_model_anchor_candidates_excludes_cone_infeasible_points(cfg):
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    near_horizontal = np.array([5.0, 0.0, 49.9])
    far_cone_ok = np.array([3.0, 0.0, 40.0])
    sample_points = np.array([near_horizontal, far_cone_ok])
    sample_delicacy = np.array([0.0, 0.0])
    candidates = struts.find_model_anchor_candidates(contact, sample_points, sample_delicacy, cfg)
    assert len(candidates) == 1
    assert np.allclose(candidates[0][0], far_cone_ok)


def test_find_model_anchor_candidates_empty_when_all_cone_infeasible(cfg):
    contact = dict(position=np.array([0.0, 0.0, 50.0]))
    # both essentially level with the contact -- near-90deg lean
    sample_points = np.array([[10.0, 0.0, 50.0], [8.0, 0.0, 49.99]])
    sample_delicacy = np.array([0.0, 0.0])
    candidates = struts.find_model_anchor_candidates(contact, sample_points, sample_delicacy, cfg)
    assert candidates == []


def test_find_model_anchor_candidates_sorted_ascending_and_capped_at_40(cfg):
    """design.md's revived strut_fallback_max_angle_deg pre-filter feeds
    cco's insertion trial machinery a candidate LIST, not a single point
    (BUG 1 fix): must be nearest-first and capped at ~40 (v3.7's
    trunks.model_strut cadence), deterministically (no RNG anywhere)."""
    contact = dict(position=np.array([0.0, 0.0, 100.0]))
    # 60 straight-down points (lean 0deg, always cone-feasible), fed in
    # DESCENDING order to prove the function does real sorting, not just
    # echoing input order.
    depths = 5.05 + 0.1 * np.arange(60)  # lengths 5.05 .. 10.95mm
    shuffled = depths[::-1].copy()
    sample_points = np.stack(
        [np.zeros_like(shuffled), np.zeros_like(shuffled), 100.0 - shuffled], axis=1)
    sample_delicacy = np.zeros(len(shuffled))
    candidates = struts.find_model_anchor_candidates(contact, sample_points, sample_delicacy, cfg)
    assert len(candidates) == 40
    lengths = [c[2] for c in candidates]
    assert lengths == sorted(lengths)
    assert lengths[0] == pytest.approx(5.05)
    assert lengths[-1] == pytest.approx(5.05 + 0.1 * 39)
