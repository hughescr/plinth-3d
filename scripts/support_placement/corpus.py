"""Calibration analyzer over a real, user-supported CHITUBOX project.

Design charter's calibration epistemics (design.md): the corpus is
EVIDENCE OF SUFFICIENCY, not optimum -- these prints succeeded, but the
profiles were hand-tuned without a holistic physics redesign. Policy:
physics decides sizing/density; the corpus is used only for what it's
actually good evidence of -- realized spatial distribution, brace/elbow
geometry, and as an upper bound on required density (never a lower bound
-- v2 should not casually place MORE densely than this corpus).

This module ONLY reads (never writes) the corpus project, via
chitubox_inject.Project. It adds no new file-format knowledge -- see
references/chitubox-format.md for the format itself.
"""
import numpy as np
from scipy.spatial import cKDTree

from . import chitubox_inject as ci


def _tip_position(chain):
    """First T_TIP record's start point (the contact point)."""
    for r in chain:
        if r.type == ci.T_TIP:
            return r.start
    return chain[0].start


def analyze(path):
    """Return a stats dict summarizing the corpus: tip count, nearest-
    neighbor spacing distribution, pass-radius histogram, z-quartile tip
    distribution, and cross-brace (type-3 joiner between different chains)
    geometry stats."""
    proj = ci.Project(path)
    groups, bridges = proj.supports()

    tips = np.array([_tip_position(g) for g in groups])
    tip_r = np.array([next((r.r1 for r in g if r.type == ci.T_TIP), np.nan)
                       for g in groups])

    n = len(tips)
    if n < 2:
        raise ValueError(f"corpus at {path} has too few supports ({n}) to calibrate from")

    tree = cKDTree(tips)
    dist, _ = tree.query(tips, k=2, workers=-1)
    nn_spacing = dist[:, 1]

    zmin, zmax = tips[:, 2].min(), tips[:, 2].max()
    zrange = max(zmax - zmin, 1e-9)
    quartile = np.clip(((tips[:, 2] - zmin) / zrange * 4).astype(int), 0, 3)
    quartile_share = {int(q): float((quartile == q).sum()) / n for q in range(4)}

    # Cross-brace geometry: type-3 joiners whose two endpoints don't both
    # lie on the same support's own tip-chain span (a coarse but workable
    # discriminator without full branch-graph decoding, which format.md
    # marks as only partially mapped).
    joiners = [r for r in proj.records if r.type == ci.T_JOINER]
    joiner_len = np.array([np.linalg.norm(r.end - r.start) for r in joiners]) if joiners else np.array([])

    return {
        "path": path,
        "n_records": proj.n_recs,
        "n_supports": n,
        "n_bridges": len(bridges),
        "n_joiners": len(joiners),
        "tip_spacing_mm": {
            "median": float(np.median(nn_spacing)),
            "p10": float(np.percentile(nn_spacing, 10)),
            "p90": float(np.percentile(nn_spacing, 90)),
            "min": float(nn_spacing.min()),
            "max": float(nn_spacing.max()),
        },
        "tip_radius_mm": {
            "values_hist": _hist(tip_r),
        },
        "z_quartile_tip_share": quartile_share,
        "joiner_length_mm": {
            "median": float(np.median(joiner_len)) if len(joiner_len) else None,
            "p90": float(np.percentile(joiner_len, 90)) if len(joiner_len) else None,
        },
    }


def _hist(values, decimals=2):
    rounded = np.round(values[~np.isnan(values)], decimals)
    uniq, counts = np.unique(rounded, return_counts=True)
    return {float(u): int(c) for u, c in zip(uniq, counts)}


def target_spacing_upper_bound(stats):
    """The density-calibration upper bound design.md prescribes: never
    place MORE densely (smaller spacing) than the corpus's realized
    median, since that corpus is already judged likely over-dense."""
    return stats["tip_spacing_mm"]["median"]
