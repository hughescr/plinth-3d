"""Model-to-model strut option (design.md #6): "sometimes the model is a
better base than the plate. Price honestly: two scars + short stiff
member vs one scar + long slender shaft + braces."

This module finds a candidate model-surface anchor near a contact and
prices it against the plate route using the same scar-cost field that
drives placement and orientation scoring (design.md requirement 3: "same
cost function, applied one stage earlier" -- reused here a stage later
instead, at routing time, which is the other place the charter names it).
"""
import numpy as np


def cost(free_length_mm, contact_delicacy, anchor_delicacy, cfg):
    """Two-sided scar pricing: material cost scales with member length;
    scar cost is paid once per real touch point on the model (plate
    anchors don't scar anything)."""
    return (cfg.material_cost_per_mm * free_length_mm
            + cfg.scar_cost_weight * (contact_delicacy + anchor_delicacy))


def find_model_anchor_candidates(contact, sample_points, sample_delicacy, cfg,
                                  min_len_mm=5.0, max_delicacy=0.5, max_candidates=40):
    """Every usable model-surface anchor point to strut to, sorted nearest
    first and capped to `max_candidates` (v3.7's trunks.model_strut
    pattern -- BUG 1 fix: v4's original find_model_anchor picked ONLY the
    single globally-nearest qualifying point with no lean/cone filter at
    all, which measured 76.7deg median lean (near-horizontal) and got
    71% cone-rejected at the builder with no fallback. A candidate list
    lets cco's normal insertion trial machinery (checkpoint/build/
    revalidate/rollback, cheapest-survivor-wins) retry the next-nearest
    anchor when the nearest one fails -- on the cone OR on clearance --
    instead of the whole model-anchor route silently vanishing.

    A candidate qualifies if it is: below or level with the contact
    (keeps the member roughly load-bearing rather than fighting gravity
    sideways), far enough away to be a real strut (not a degenerate
    near-zero-length hack), not itself a delicate/detailed area (don't
    trade one scar for a worse one), within cfg.strut_max_length_mm (keep
    it local, design.md #6), and within cfg.strut_fallback_max_angle_deg
    of vertical (the REVIVED v3.7 knob -- config.py:209 was dead code in
    v4; this is the pre-filter that fixes the near-horizontal problem:
    cco's own builder-side cone check (_within_cone) is the final
    authority on any one candidate's feasibility at its actual sized
    radius, but pre-filtering here means we don't waste 39 of 40 slots on
    candidates that were never going to clear it).

    Returns a list of (anchor_point, anchor_delicacy, length_mm) tuples
    ordered by ascending length (ties broken by sample index, for
    determinism -- no RNG anywhere, design-v4.md #4), empty if nothing
    qualifies within the pool given.
    """
    pos = np.asarray(contact["position"], float)
    if len(sample_points) == 0:
        return []
    delta = sample_points - pos
    d = np.linalg.norm(delta, axis=1)
    dxy = np.linalg.norm(delta[:, :2], axis=1)
    drop = -delta[:, 2]  # positive when the candidate is below the contact
    with np.errstate(divide="ignore", invalid="ignore"):
        lean = np.degrees(np.arctan2(dxy, drop))
    max_len = getattr(cfg, "strut_max_length_mm", None)
    max_lean = getattr(cfg, "strut_fallback_max_angle_deg", 60.0)
    mask = (d >= min_len_mm) & (sample_points[:, 2] <= pos[2] + 1e-6) \
           & (sample_delicacy <= max_delicacy) & (lean <= max_lean + 1e-9)
    if max_len is not None:
        mask &= (d <= max_len)
    if not mask.any():
        return []
    idx = np.nonzero(mask)[0]
    order = idx[np.argsort(d[idx], kind="stable")][:max_candidates]
    return [(sample_points[j], float(sample_delicacy[j]), float(d[j])) for j in order]


def find_model_anchor(contact, sample_points, sample_delicacy, cfg,
                       min_len_mm=5.0, max_delicacy=0.5):
    """Nearest cone-feasible usable model-surface point to strut to (see
    find_model_anchor_candidates for the qualification rules). Thin
    single-result wrapper kept for choose_anchor's one-shot cost
    comparison; cco.py's insertion builder uses
    find_model_anchor_candidates directly so a cone- or clearance-
    infeasible nearest candidate doesn't kill the whole model-anchor
    route (BUG 1).

    Returns (anchor_point, anchor_delicacy, length_mm) or None if nothing
    qualifies within the pool given.
    """
    candidates = find_model_anchor_candidates(
        contact, sample_points, sample_delicacy, cfg,
        min_len_mm=min_len_mm, max_delicacy=max_delicacy)
    if not candidates:
        return None
    return candidates[0]


def choose_anchor(contact, plate_free_length_mm, contact_delicacy,
                   sample_points, sample_delicacy, cfg):
    """Decide plate vs model-strut for one contact. Returns dict:
    {kind: 'plate'|'model', anchor: point or None, free_length_mm,
     saved_mm}. Only switches to a model strut if it's both cheaper AND
    saves at least cfg.strut_min_length_saved_mm of material (design.md:
    don't chase tiny savings for a second scar)."""
    plate_cost = cost(plate_free_length_mm, contact_delicacy, 0.0, cfg)
    found = find_model_anchor(contact, sample_points, sample_delicacy, cfg)
    if found is None:
        return dict(kind="plate", anchor=None, free_length_mm=plate_free_length_mm, saved_mm=0.0)
    anchor_pt, anchor_delicacy, strut_len = found
    # approximate strut free length ~ straight-line length + stub (routing
    # will compute the exact articulated length; this is the pre-routing
    # cost-comparison estimate the charter calls for).
    strut_free_length = strut_len
    saved = plate_free_length_mm - strut_free_length
    strut_cost = cost(strut_free_length, contact_delicacy, anchor_delicacy, cfg)
    if strut_cost < plate_cost and saved >= cfg.strut_min_length_saved_mm:
        return dict(kind="model", anchor=anchor_pt, free_length_mm=strut_free_length, saved_mm=saved)
    return dict(kind="plate", anchor=None, free_length_mm=plate_free_length_mm, saved_mm=0.0)
