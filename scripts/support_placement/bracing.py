"""Bracing-as-restraint (design-v4.md #9, SPEC-BRACING workstream B).

Braces are NON-TREE Forest members (tree.Forest.add_brace): graph edges,
not topology. Two shapes carry over from v3's retired braces.py (read,
never imported, see that module + scripts/tests/test_braces.py for the
geometry authority this reimplements against tree.Forest):

- Cross-link (preferred): a rod (or a pair, "X-pair") between the member
  needing a restraint (M) and a nearby neighbor member (N), climbing at
  v3.6's hardcoded 45-degree construction (rise == the xy distance
  crossed, ALWAYS -- never the printability angle). Pairs wider than
  cfg.twin_single_brace_xy_mm get a crossed X-pair (both rods discounted
  0.5**0.25 for load-sharing); pairs at or under that spacing get ONE
  rod per rung instead, alternating which member it starts from by rung
  parity, sized WITHOUT the discount (v3.6's twin rule -- a second rod
  between columns millimetres apart is cosmetic waste, not a load-sharing
  partner).
- Model rung (fallback, only when no cross-link candidate clears): a
  short, EXACTLY HORIZONTAL rod from M's axis to a nearby model surface
  point, capped at cfg.model_brace_max_len_mm -- the Chitubox-native
  mini-brace, embed_end="p1" so tree.printability_report's length-capped
  near-horizontal exemption applies (kind "model_rung" only -- the gate
  is not weakened for anything else).

This module only proposes and transactionally places braces for ONE
snapshot of a forest against a caller-supplied spans_report (transient.
transient_report's output shape -- another workstream; tests here hand-
build the dicts, that is the decoupling contract per SPEC-BRACING). No
resizing of host members, no engine wiring, no restraint/span computation
beyond consuming spans_report.

SPEC-ARM-FIX fix A (Phase 4d, design-v4.md #9's literal rule): compute_
needs ALSO triggers on spans_report's worst_span_mm (the activation-aware
moment-arm value, transient.transient_report's own per-member field) via
transient.r_transient_mm -- not just the static free_gaps trigger. A
pass-through chain can have tiny per-member static gaps (no internal
restraint points at all) while its worst_span_mm runs 20mm+ unrestrained
during the actual build (a lagged sibling elsewhere on the path). The
static trigger is blind to that danger class; this one isn't. Only
`transient.r_transient_mm` is imported from that module (a pure function,
no forest/report coupling) -- the decoupling contract above stays intact:
this module still never COMPUTES a span itself, only consumes the two
fields (worst_span_mm, free_gaps) spans_report already carries.
"""
import math

import numpy as np

from . import cco, placement, transient, tree

# Half of the connectivity-restraint-dedup tolerance lives in cfg
# (brace_ladder_z_mm/2); see propose_and_place's restraint_zs skip.


def _n_slender(gap_mm, cfg, lean_deg):
    """Smallest n >= 0 (capped at 50, v3's runaway guard) such that
    splitting `gap_mm` into n+1 equal segments brings the deflection-
    required shaft radius for one segment back under
    cfg.max_unbraced_shaft_r_mm at the member's own lean."""
    for n in range(51):
        seg = gap_mm / (n + 1)
        r = placement._shaft_r_for_L(seg, cfg, lean_deg)
        if r <= cfg.max_unbraced_shaft_r_mm:
            return n
    return 50


def _rungs_that_fit(gap_span, n_rungs):
    """Cap n_rungs so that, laid out evenly across gap_span at n_rungs+1
    equal sub-spacings (propose_and_place's own layout formula:
    z = z_lo + (k+1) * gap_span / (n_rungs+1)), every sub-spacing is
    >= 1mm. gap_span / (n+1) >= 1  <=>  n <= gap_span - 1."""
    if n_rungs <= 0:
        return 0
    max_by_spacing = int(math.floor(gap_span - 1.0 + 1e-9))
    return max(0, min(n_rungs, max_by_spacing))


def compute_needs(forest, spans_report, cfg):
    """For each member and each free gap (z_lo, z_hi) in spans_report,
    decide how many rungs that gap needs (SPEC-BRACING Part 2): the max
    of the slenderness-triggered count (n_slender, deflection budget) and
    the build-time trigger (n_span, v3's brace_free_span_mm cadence) --
    this is the STATIC trigger, blind to activation timing by design.

    SPEC-ARM-FIX fix A (Phase 4d, design-v4.md #9): ALSO trigger on the
    member's own worst_span_mm (S), the activation-aware moment-arm value
    threaded straight from transient_report -- a pass-through member can
    have a tiny static free_gap (no internal restraint points on its OWN
    extent) while S, inherited from a lagged restraint elsewhere on its
    root->terminal path, is large enough that either the resulting
    r_transient shaft radius exceeds cfg.max_unbraced_shaft_r_mm or S
    itself exceeds cfg.brace_free_span_mm. When that fires, request
    rungs (n_rungs = max(n_slender(S, lean), int(S / brace_free_span_mm))
    -- the SAME two-trigger formula as the static path, just fed S
    instead of a gap's own span) in the member's LARGEST free gap (the
    only concrete height band this trigger has to work with -- spans_
    report doesn't say WHERE along the member the moment-arm empirically
    bites), capped by what physically fits in that gap at >= 1mm rung
    spacing (_rungs_that_fit).

    Dedup (never double-request the same height band): when the
    activation trigger's target gap is the SAME (z_lo, z_hi) the static
    trigger already produced a request for on this member, the two are
    MERGED (max of the two counts) into ONE entry, not appended as a
    second request -- propose_and_place's rung-height formula divides a
    gap by (n_rungs+1), so two separate requests against the identical
    band would place overlapping/duplicate rungs at different offsets.

    Returns an ordered list of (member_idx, z_lo, z_hi, n_rungs) requests
    for n_rungs >= 1, sorted by (member_idx, z_lo) -- deterministic
    regardless of spans_report's own iteration/dict order."""
    requests = []
    for member_idx, info in spans_report.items():
        member = forest.members[member_idx]
        lean = cco._lean_deg(member.p0, member.p1)
        gaps = info.get("free_gaps", [])
        member_reqs = {}  # (z_lo, z_hi) -> n_rungs, static-then-merged

        for gap in gaps:
            z_lo, z_hi = float(gap[0]), float(gap[1])
            span = z_hi - z_lo
            if span <= 0:
                # Design decision (recorded): a degenerate/zero-length
                # gap needs no rung -- defensive, spans_report is another
                # workstream's output.
                continue
            n_slender = _n_slender(span, cfg, lean)
            n_span = int(span / cfg.brace_free_span_mm)
            n_rungs = max(n_slender, n_span)
            if n_rungs >= 1:
                member_reqs[(z_lo, z_hi)] = n_rungs

        worst_span_mm = float(info.get("worst_span_mm", 0.0))
        if worst_span_mm > 0 and gaps:
            n_terminals = info.get("n_terminals", 0)
            r_trans = transient.r_transient_mm(worst_span_mm, lean, n_terminals, cfg)
            if r_trans > cfg.max_unbraced_shaft_r_mm or worst_span_mm > cfg.brace_free_span_mm:
                n_slender_s = _n_slender(worst_span_mm, cfg, lean)
                n_span_s = int(worst_span_mm / cfg.brace_free_span_mm)
                n_rungs_s = max(n_slender_s, n_span_s)
                if n_rungs_s >= 1:
                    z_lo, z_hi = max(
                        ((float(g[0]), float(g[1])) for g in gaps),
                        key=lambda t: t[1] - t[0])
                    gap_span = z_hi - z_lo
                    n_rungs_s = _rungs_that_fit(gap_span, n_rungs_s)
                    if n_rungs_s >= 1:
                        key = (z_lo, z_hi)
                        member_reqs[key] = max(member_reqs.get(key, 0), n_rungs_s)

        for (z_lo, z_hi), n_rungs in member_reqs.items():
            requests.append((member_idx, z_lo, z_hi, n_rungs))
    requests.sort(key=lambda r: (r[0], r[1]))
    return requests


# ---------------------------------------------------------------- geometry --
def _z_extent(m):
    z0, z1 = float(m.p0[2]), float(m.p1[2])
    return (z0, z1) if z0 <= z1 else (z1, z0)


def _axis_xy_at_z(m, z):
    p0, p1 = m.p0, m.p1
    dz = float(p1[2] - p0[2])
    if abs(dz) < 1e-9:
        return np.array([p0[0], p0[1]], dtype=float)
    t = (z - p0[2]) / dz
    return p0[:2] + t * (p1[:2] - p0[:2])


def _axis_point_at_z(m, z):
    xy = _axis_xy_at_z(m, z)
    return np.array([xy[0], xy[1], z], dtype=float)


def _live_neighbor_indices(forest):
    braces_set = set(forest.braces)
    return [i for i, m in enumerate(forest.members)
            if i not in forest.dead and i not in braces_set and m.kind != "stub"]


def _find_cross_link_candidates(forest, mi, z, cfg, k):
    """Neighbor candidates for member `mi` at rung height z (SPEC-BRACING
    rule 1): live non-brace non-stub members N != M whose z-extent covers
    [z, z+rise] (rise = d, the xy distance between the axes at z -- v3.6's
    hardcoded 45-degree construction), sorted by (d, idx). `k` is the
    rung's own index within its request (SAME meaning as
    _cross_link_rods' k, which parity picks the rod direction) --
    required, not defaulted, because which of M's endpoints a rod
    actually reads depends on it (see the should-fix note below).

    Fix (verifier blocker #1, floating brace endpoint): rule 1's
    endpoints are "ON THE AXES (centerline to centerline)" for BOTH
    members -- the X-pair's mirrored rod and the twin rule's odd-parity
    rung both read M's own axis AT z+rise (_cross_link_rods), and
    _axis_xy_at_z is an unclamped linear parametrization that silently
    extrapolates past a member's real endpoint rather than failing. A
    candidate whose rise would put z+rise beyond M's own physical
    z-extent is therefore not "on M's axis" at all -- it must be
    rejected here, exactly like N's own z-extent is already rejected
    below, or the rod gets certified while floating disconnected from
    M's real geometry (the 'stale-brace-leak' class connectivity_report
    exists to catch).

    Should-fix (verifier round 2, twin k-even over-rejection): the fix
    above made M's [z, z+rise] coverage unconditional, but a rod only
    ever LANDS on M at z+rise for an X-pair (rod2 mirrors onto M) or a
    twin ODD-parity rung (`k` odd, N -> M climbing) -- see
    _cross_link_rods. An X-pair's OTHER rod (rod1) and a twin
    EVEN-parity rung both read M at z (its own real point, never
    extrapolated) and only read N at z+rise; requiring M to also cover
    z+rise in that case over-rejects a perfectly valid candidate.
    X-pairs are unaffected (both rods always required, so M needs the
    full range regardless of k) -- N's own [z, z+rise] requirement below
    stays unconditional in every case, unchanged."""
    M = forest.members[mi]
    z0_M, z1_M = _z_extent(M)
    if not (z0_M - 1e-9 <= z <= z1_M + 1e-9):
        # M's own axis doesn't even reach the requested rung height --
        # no candidate at this z can ever be valid.
        return []
    p_M = _axis_point_at_z(M, z)
    cands = []
    for ni in _live_neighbor_indices(forest):
        if ni == mi:
            continue
        N = forest.members[ni]
        z0, z1 = _z_extent(N)
        if not (z0 - 1e-9 <= z <= z1 + 1e-9):
            continue
        p_N = _axis_point_at_z(N, z)
        d = float(np.linalg.norm(p_N[:2] - p_M[:2]))
        if not (1e-6 < d <= cfg.brace_neighbor_max_xy_mm):
            continue
        rise = d
        if not (z0 - 1e-9 <= z + rise <= z1 + 1e-9):
            continue
        # M's own [z, z+rise] coverage is only required when a rod
        # actually lands ON M at z+rise: always true for an X-pair
        # (d > twin_single_brace_xy_mm, both rods placed -- rod2 mirrors
        # onto M), and true for a twin (d <= threshold) ONLY on an odd
        # (N -> M climbing) rung. A twin even rung never reads M above z.
        is_twin = d <= cfg.twin_single_brace_xy_mm
        m_needs_full_extent = (not is_twin) or (k % 2 != 0)
        if m_needs_full_extent and not (z0_M - 1e-9 <= z + rise <= z1_M + 1e-9):
            continue
        cands.append((d, ni))
    cands.sort(key=lambda t: (t[0], t[1]))
    return cands


def _cross_link_rods(forest, mi, ni, z, d, k, cfg):
    """Rod geometry for one candidate (SPEC-BRACING rule 1): X-pair for
    d > cfg.twin_single_brace_xy_mm (both rods discounted), else a single
    alternating rod (twin rule, undiscounted). rise = d always."""
    M, N = forest.members[mi], forest.members[ni]
    rise = d
    p_M_z, p_N_z = _axis_point_at_z(M, z), _axis_point_at_z(N, z)
    p_M_hi, p_N_hi = _axis_point_at_z(M, z + rise), _axis_point_at_z(N, z + rise)
    rod_len = d * np.sqrt(2.0)
    r_raw = placement._shaft_r_for_L(rod_len, cfg, cfg.max_segment_angle_deg)
    if d <= cfg.twin_single_brace_xy_mm:
        r = max(cfg.brace_r_mm, r_raw)
        if k % 2 == 0:
            return [(p_M_z, p_N_hi, r)]      # even: M -> N climbing
        return [(p_N_z, p_M_hi, r)]          # odd: N -> M climbing
    r = max(cfg.brace_r_mm, r_raw * 0.5 ** 0.25)
    return [(p_M_z, p_N_hi, r), (p_N_z, p_M_hi, r)]


def _find_model_rung_anchors(forest, surface_tree, mi, z, cfg, limit=6):
    """Rule 2 fallback candidates: surface points within cfg.model_brace_
    max_len_mm of M's axis at z, with |anchor_z - z| <= 0.75, sorted by
    (distance, index) -- nearest first, up to `limit` (v3's cadence,
    braces.py's ground_points[mask][argsort][:6]). Returns a list of
    (p0, p1) with p0 = M's axis at the ANCHOR's z (so the rod is exactly
    horizontal by construction) and p1 = the anchor; empty if nothing
    qualifies.

    Should-fix (verifier round 2, model-rung retry): the original
    single-candidate version gave up entirely if the ONE nearest anchor
    failed the caller's length floor or clearance, even when a perfectly
    good next-nearest anchor was right there. Returning the whole
    ordered shortlist lets the caller retry deterministically instead."""
    if surface_tree is None:
        return []
    M = forest.members[mi]
    p_M_z = _axis_point_at_z(M, z)
    pts = np.asarray(surface_tree.data, dtype=float)
    if pts.size == 0:
        return []
    dist = np.linalg.norm(pts - p_M_z[None, :], axis=1)
    anchor_z = pts[:, 2]
    mask = (dist <= cfg.model_brace_max_len_mm + 1e-9) & (np.abs(anchor_z - z) <= 0.75 + 1e-9)
    idxs = np.nonzero(mask)[0]
    if len(idxs) == 0:
        return []
    order = idxs[np.argsort(dist[idxs], kind="stable")]
    out = []
    for i in order[:limit]:
        anchor = pts[int(i)]
        p0 = _axis_point_at_z(M, float(anchor[2]))
        p1 = np.array(anchor, dtype=float)
        out.append((p0, p1))
    return out


# --------------------------------------------------------------- placement --
def _try_place_rods(forest, surface_tree, cfg, rods, kind, embed_end, counters):
    """Transactional placement (SPEC-BRACING rule 3): checkpoint, add all
    rods of this ONE candidate (both X-pair rods together, atomically),
    revalidate, rollback on any violation (returning the kind's owner
    counter to where it was so placed members keep a contiguous sequence
    number), else commit. Returns the new member indices or None."""
    cp = forest.checkpoint()
    idxs = []
    for p0, p1, r in rods:
        owner = (kind, counters[kind])
        counters[kind] += 1
        m = tree.Member(p0=p0, p1=p1, r0=r, r1=r, kind=kind, owner=owner,
                         embed_end=embed_end)
        idxs.append(forest.add_brace(m))
    violations = forest.revalidate(surface_tree, cfg)
    if violations:
        forest.rollback(cp)
        counters[kind] -= len(rods)
        return None
    forest.commit(cp)
    return idxs


def _place_one_rung(forest, surface_tree, cfg, mi, z, k, counters):
    """One rung request at height z on member mi: try cross-link
    candidates nearest-first, then the model-rung fallback -- itself
    retried nearest-first (should-fix, verifier round 2: up to
    cfg-implied v3 cadence of 6 candidates, deterministic (distance,
    index) order) rather than giving up after the single nearest anchor.
    Returns a list of (member_idx, kind) placed, or None if
    unplaceable."""
    for d, ni in _find_cross_link_candidates(forest, mi, z, cfg, k):
        rods = _cross_link_rods(forest, mi, ni, z, d, k, cfg)
        placed = _try_place_rods(forest, surface_tree, cfg, rods, "brace", None, counters)
        if placed is not None:
            return [(idx, "brace") for idx in placed]

    for p0, p1 in _find_model_rung_anchors(forest, surface_tree, mi, z, cfg):
        length = float(np.linalg.norm(p1 - p0))
        if not (1.0 - 1e-9 <= length <= cfg.model_brace_max_len_mm + 1e-9):
            continue
        placed = _try_place_rods(forest, surface_tree, cfg,
                                  [(p0, p1, cfg.brace_r_mm)], "model_rung", "p1", counters)
        if placed is not None:
            return [(idx, "model_rung") for idx in placed]
    return None


def propose_and_place(forest, surface_tree, plate_z, cfg, spans_report, log=None):
    """Places braces transactionally against ONE snapshot of spans_report
    (SPEC-BRACING Part 2). `plate_z` is accepted per the interface
    contract (engine wiring depends on this exact signature) but unused
    here -- bracing never roots a member at the plate, only cross-links
    or model rungs; plate-relative placement decisions belong to cco.py.

    Returns dict(placed=[brace idx...], rungs_requested, rungs_placed,
    rungs_unplaceable, by_kind={"brace": n, "model_rung": m}).

    Design decision (recorded): a rung height skipped because an
    existing restraint (spans_report[...]["restraint_zs"]) already sits
    within cfg.brace_ladder_z_mm/2 of it is neither "placed" nor
    "unplaceable" -- it wasn't attempted because it wasn't needed, which
    is a different outcome than "tried and couldn't clear". It still
    counts toward rungs_requested (the raw ask from compute_needs, before
    any placement-time skip/failure)."""
    _ = plate_z  # accepted for interface parity; unused, see docstring
    if log is None:
        log = {}
    requests = compute_needs(forest, spans_report, cfg)
    placed = []
    rungs_requested = 0
    rungs_placed = 0
    rungs_unplaceable = 0
    by_kind = {}
    counters = {"brace": 0, "model_rung": 0}

    for member_idx, z_lo, z_hi, n_rungs in requests:
        rungs_requested += n_rungs
        restraint_zs = (spans_report.get(member_idx, {}).get("restraint_zs") or [])
        for k in range(n_rungs):
            z = z_lo + (k + 1) * (z_hi - z_lo) / (n_rungs + 1)
            if any(abs(z - rz) <= cfg.brace_ladder_z_mm / 2.0 for rz in restraint_zs):
                continue
            result = _place_one_rung(forest, surface_tree, cfg, member_idx, z, k, counters)
            if result is None:
                rungs_unplaceable += 1
                continue
            rungs_placed += 1
            for idx, kind in result:
                placed.append(idx)
                by_kind[kind] = by_kind.get(kind, 0) + 1

    log["bracing"] = dict(rungs_requested=rungs_requested, rungs_placed=rungs_placed,
                           rungs_unplaceable=rungs_unplaceable, by_kind=dict(by_kind))
    return dict(placed=placed, rungs_requested=rungs_requested, rungs_placed=rungs_placed,
                rungs_unplaceable=rungs_unplaceable, by_kind=by_kind)
