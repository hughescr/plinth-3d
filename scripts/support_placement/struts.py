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


def find_model_anchor(contact, sample_points, sample_delicacy, cfg,
                       min_len_mm=5.0, max_delicacy=0.5):
    """Nearest usable model-surface point to strut to: below or level with
    the contact (keeps the member roughly load-bearing rather than
    fighting gravity sideways), far enough away to be a real strut (not a
    degenerate near-zero-length hack), and not itself a delicate/detailed
    area (don't trade one scar for a worse one).

    Returns (anchor_point, anchor_delicacy, length_mm) or None if nothing
    qualifies within the pool given.
    """
    pos = np.asarray(contact["position"], float)
    d = np.linalg.norm(sample_points - pos, axis=1)
    max_len = getattr(cfg, "strut_max_length_mm", None)
    mask = (d >= min_len_mm) & (sample_points[:, 2] <= pos[2] + 1e-6) \
           & (sample_delicacy <= max_delicacy)
    if max_len is not None:
        mask &= (d <= max_len)
    if not mask.any():
        return None
    idx = np.nonzero(mask)[0]
    best = idx[np.argmin(d[idx])]
    return sample_points[best], float(sample_delicacy[best]), float(d[best])


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
