import numpy as np
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
