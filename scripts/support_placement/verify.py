"""Self-verify (design.md #9): "zero unsupported islands, clearance check,
plus per-region load check against green-strength SF." Mandatory, not
optional -- engine.py treats a non-empty report as a fix-pass trigger, not
just a warning.

Also hosts the reusable scar-cost orientation scorer (design.md
requirement 3: "same cost function, applied one stage earlier" -- rotate
delicate regions away from support-facing surfaces). This is exposed for
prep-mini's orient stage to call; the support-placement engine itself
doesn't choose orientation, it evaluates candidates handed to it.
"""
import numpy as np
from scipy.spatial import cKDTree

from . import perception, routing, joints, tree as tree_mod, removal as removal_mod
from .geometry_exact import member_rays


# --- SCAR-AREA-CAP embed policy (SPEC-EMBED-POLICY.md, owner-ratified cd989fe) -
# The ratified sanction for DESIGNED embed ends: an area bound on a conservative
# closed-form upper bound of the surface-crossing patch, a scar max-dimension
# bound, a projected (angle-free) anchor-area floor, and strict crossing
# topology (rule 4). SUPERSEDES the retired depth/arc-inset sanctions
# (`_embed_inset_arc_exact` below survives only as a distance-gate mask, not a
# sanction). These constants are literal from the spec text and MUST be
# bit-identical to the independent census implementation
# (docs/artifacts/audit-2026-07/acceptance-run-2026-07-23/census_run.py) -- the
# engine gate and the census then count the SAME population from the SAME
# formulas, neither sharing code with the other. TIP_R_MIN is read from cfg
# (cfg.tip_r_min_mm); the BALL factors are the designed ball radii
# (config.tip_ball_r_factor=1.5 stub tips, joints.MODEL_FOOT_FACTOR=1.4 feet).
A_ABS_MM2 = 2.0            # absolute scar-area allowance (1.60mm equiv. scar dia)
KAPPA = 1.25              # headroom multiplier on the designed crossing disc
L_SCAR_MAX_MM = 2.5      # max scar patch dimension
EPS_COS = 1e-9           # divide-by-zero guard only; NOT a physics floor
TOL_MM = 0.05            # interval anchoring / detection tolerance
MIN_ILLEGAL_LEN_MM = 0.05  # census MIN_ILLEGAL_LEN: a longer solid run is a transit
BALL_FACTOR_STUB = 1.5   # contact-stub tips  (config.tip_ball_r_factor)
BALL_FACTOR_FOOT = 1.4   # strut / model-rung feet  (joints.MODEL_FOOT_FACTOR)


def coverage_report(mesh, islands, contacts, cfg):
    """Independent re-check (not just trusting sample_candidates by
    construction): for each island, the worst-covered face's distance to
    its nearest assigned contact. Flags islands whose worst point exceeds
    cfg.target_spacing_mm * cfg.coverage_radius_factor."""
    threshold = cfg.target_spacing_mm * cfg.coverage_radius_factor
    by_island = {}
    for c in contacts:
        by_island.setdefault(c["island_id"], []).append(c["position"])

    report = []
    for island_id, face_ids in enumerate(islands):
        positions = by_island.get(island_id)
        centers = mesh.triangles_center[face_ids]
        if not positions:
            report.append(dict(island_id=island_id, covered=False,
                                worst_gap_mm=float("inf"), n_faces=len(face_ids)))
            continue
        tree = cKDTree(np.array(positions))
        dist, _ = tree.query(centers, workers=routing.query_workers(len(centers)))
        worst = float(dist.max())
        report.append(dict(island_id=island_id, covered=worst <= threshold,
                            worst_gap_mm=worst, n_faces=len(face_ids)))
    return report


def _embed_inset_arc_exact(geom, m, cfg):
    """DISTANCE-GATE MASK ONLY -- no longer an embed sanction (SPEC-EMBED-POLICY,
    ratified cd989fe: the arc/depth inset is RETIRED as a sanction, replaced by
    SCAR-AREA-CAP + rule 4, see `_embed_scar_verdicts` / `_transit_illegal`).
    This survives solely so the swept-capsule DISTANCE check (segment_clearance)
    does not self-trip on a member's OWN designed embed: a foot is embedded in
    the surface by construction, so the min-distance test masks that zone with a
    geometric inset. Retained verbatim (angle-aware arc, pitch term 0 for an
    exact oracle) to keep the distance gate's calibration unchanged this round;
    it is slated for replacement in the placement-rebuild round alongside
    tree.Forest._embed_inset_arc. Non-embed ends get 0.

    Angle-aware inset: a designed foot on the surface grazes the standoff
    envelope as the shaft dives toward it; the arc length scales with the
    approach angle (perpendicular reach / cos(axis-vs-surface-normal), floored
    at the 60deg strut/branch family)."""
    p0 = np.asarray(m["p0"], float)
    p1 = np.asarray(m["p1"], float)
    L = float(np.linalg.norm(p1 - p0))

    def _arc(p_embed, u_out, r_end):
        if L < 1e-9:
            return cfg.contact_depth_mm + r_end + cfg.standoff_mm
        _d, _cp, _ti, n = geom.project(p_embed)
        nn = float(np.linalg.norm(n))
        if nn < 1e-9:
            return cfg.contact_depth_mm + r_end + cfg.standoff_mm
        cosang = max(abs(float(np.dot(u_out, n / nn))), 0.5)
        return cfg.contact_depth_mm + (r_end + cfg.standoff_mm) / cosang

    embed = m.get("embed_end")
    inset0 = _arc(p0, (p1 - p0) / L, float(m["r0"])) if (embed == "p0" and L >= 1e-9) else 0.0
    inset1 = _arc(p1, (p0 - p1) / L, float(m["r1"])) if (embed == "p1" and L >= 1e-9) else 0.0
    return inset0, inset1


def _tip_iv_index(ivs, ray_seg, embed, tol=TOL_MM):
    """Index into `ivs` (sorted ascending by s_a, arc-length mm along the ray)
    of the TIP-ANCHORED interval for a designed embed end, per SPEC-EMBED-POLICY:
    'the solid interval whose start lies within TOL_MM of the embed point' (p1
    case: symmetric, measured back from the ray end). DISTANCE-based (a tip
    sitting a hair outside the surface still anchors as long as the gap is within
    tolerance), independent verify-side copy of census tip_iv_index. Returns None
    if no interval anchors (a mid-span-only transit) or `ivs` is empty."""
    if not ivs:
        return None
    if embed == "p0":
        return 0 if ivs[0]["s_a"] <= tol else None
    if embed == "p1":
        return (len(ivs) - 1) if (ray_seg - ivs[-1]["s_b"]) <= tol else None
    return None


def _transit_illegal(intervals, m, verdict_legal):
    """SPEC-EMBED-POLICY rule 4 (through-transit) -- True iff the member has an
    inside-the-solid run longer than MIN_ILLEGAL_LEN_MM that is NOT the
    sanctioned tip-anchored exemption. ONLY the tip-anchored interval of a
    designed embed end is exempt-eligible, and ONLY when its SCAR-AREA-CAP
    verdict is LEGAL; an illegal verdict flags its full raw length, and EVERY
    unanchored interval (mid-span graze, re-entry after the tip interval exits,
    capsule-edge crossing) is a transit and illegal regardless of its size. A
    parity-crossing test on exact boundaries -- never a distance test -- so an
    ordinary surface-hugging stub cannot false-positive. Independent verify-side
    copy of census PASS C3 (shares only geom's generic primitives).

    Ray set is chosen by the CALLER via which intervals it passes: stubs get
    AXIS-only (a stub hugs its own emergence surface, so a capsule-offset ray
    grazes it and false-positives -- census restricts offset rays to non-stubs);
    non-stubs get AXIS + the capsule-offset rays, so a strut whose axis is clean
    but whose capsule surface grazes a thin wall (census member 1138) is caught.
    Tip anchoring is evaluated per RAY, each against its OWN arc length (an
    offset ray's length differs from the axis by O(|r1-r0|) -- material at the
    0.05mm tolerance for a foot graze reaching the designed embed end), matching
    the census's per-ray tip_iv_index. `verdict_legal` is the member's SINGLE
    axis-derived SCAR-AREA-CAP verdict, applied to exempt the tip interval on any
    ray (None => no exemption)."""
    if not intervals:
        return False
    embed = m.get("embed_end")
    p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
    axis_seg = float(np.linalg.norm(p1 - p0))
    ray_segs = {"axis": axis_seg}
    if m["kind"] != "stub" and axis_seg >= 1e-9:
        for lab, _O, _D, sg in member_rays(p0, p1, float(m["r0"]), float(m["r1"])):
            ray_segs[lab] = sg
    by_ray = {}
    for iv in intervals:
        by_ray.setdefault(iv["ray"], []).append(iv)
    for ray, ivs in by_ray.items():
        ivs.sort(key=lambda iv: iv["s_a"])
        ray_seg = ray_segs.get(ray, axis_seg)
        tip_idx = (_tip_iv_index(ivs, ray_seg, embed)
                   if embed in ("p0", "p1") else None)
        for i, iv in enumerate(ivs):
            if i == tip_idx and verdict_legal:
                continue
            if (iv["s_b"] - iv["s_a"]) > MIN_ILLEGAL_LEN_MM:
                return True
    return False


def _batch_axis_intervals(geom, members, mis, rays="axis"):
    """ONE native penetration_intervals call for ALL members in `mis`
    (Amendment A7/A18); returns {member_idx: interval-list}. Batch SoA entry
    point when the geom exposes it (the Rust kernel does); per-member fallback
    only for a legacy cKDTree/oracle with none. Independent verify-side copy.

    `rays` selects the ray set the kernel returns intervals for: "axis" (the
    single centerline, the load-bearing stub rule) or "all" (axis + the n
    capsule-offset rays, R2's non-stub capsule-graze evidence)."""
    if not mis:
        return {}
    p0s = np.array([np.asarray(members[mi]["p0"], float) for mi in mis])
    p1s = np.array([np.asarray(members[mi]["p1"], float) for mi in mis])
    r0s = np.array([float(members[mi]["r0"]) for mi in mis])
    r1s = np.array([float(members[mi]["r1"]) for mi in mis])
    batch = getattr(geom, "penetration_intervals_batch", None)
    if batch is not None:
        per = batch(p0s, p1s, r0s, r1s, rays=rays, min_illegal_len_mm=0.05)
    else:
        per = [geom.penetration_intervals(p0s[j], p1s[j], r0s[j], r1s[j],
                                          rays=rays, min_illegal_len_mm=0.05)
               for j in range(len(mis))]
    return {mi: per[j] for j, mi in enumerate(mis)}


def _scar_area_cap_legal(T, d, cosang, r_x, r_end, R_b):
    """Pure SCAR-AREA-CAP legality predicate -- the SPEC-EMBED-POLICY normative
    closed forms, given one designed embed end's geometry. Split out so the math
    is unit-pinnable against the spec's documented numbers with no mesh. Returns
    (legal_bool, metrics_dict). `cosang` is ignored when T <= TOL_MM (no shaft
    term). LITERAL-IDENTICAL to census_run.py PASS C2.

        A_shaft  = 0                                          if T <= TOL_MM
                 = min(pi*r_x^2 / max(cosang, EPS_COS), 2*r_x*T)   otherwise
        A_ball   = pi * max(R_b^2 - d^2, 0)
        A_patch  = A_shaft + A_ball
        A_max    = max(A_ABS_MM2, KAPPA * pi * R_b^2)
        L_scar   = 0                                          if T <= TOL_MM
                 = min(2*r_x / max(cosang, EPS_COS), T + 2*r_x)    otherwise
        A_anchor = A_ball + (pi*r_x^2 if T > TOL_MM else 0)
        LEGAL iff A_patch <= A_max AND L_scar <= L_SCAR_MAX_MM
                  AND A_anchor >= pi*r_end^2      (scar cap; scar dim; anchor floor)
    """
    if T > TOL_MM:
        A_shaft = min(np.pi * r_x ** 2 / max(cosang, EPS_COS), 2.0 * r_x * T)
        L_scar = min(2.0 * r_x / max(cosang, EPS_COS), T + 2.0 * r_x)
        A_anchor_shaft = np.pi * r_x ** 2
    else:
        A_shaft = 0.0; L_scar = 0.0; A_anchor_shaft = 0.0
    A_ball = np.pi * max(R_b ** 2 - d ** 2, 0.0)
    A_patch = A_shaft + A_ball
    A_max = max(A_ABS_MM2, KAPPA * np.pi * R_b ** 2)
    A_anchor = A_ball + A_anchor_shaft
    legal = ((A_patch <= A_max) and (L_scar <= L_SCAR_MAX_MM)
             and (A_anchor >= np.pi * r_end ** 2))
    return bool(legal), dict(A_patch=float(A_patch), A_max=float(A_max),
                             L_scar=float(L_scar), A_anchor=float(A_anchor),
                             A_ball=float(A_ball), A_shaft=float(A_shaft))


def _embed_scar_verdicts(geom, members, mis, ivmap, cfg):
    """SCAR-AREA-CAP legality per designed embed end (SPEC-EMBED-POLICY.md),
    AXIS geometry only. Returns {mi: legal_bool} for every mi in `mis` whose
    member carries a designed embed_end (p0/p1). The verdict gates the rule-4 tip
    exemption in `_transit_illegal` (a LEGAL tip is fully exempt; an ILLEGAL one
    is flagged at full raw length). Formulas and constants are LITERAL-IDENTICAL
    to the independent census (census_run.py PASS C2) -- the engine never shares
    code with it, only the same spec text, so both count the same population.

    Inputs read exactly as the census does: `T` = length of the tip-anchored
    axis interval; `x_c` = its exit crossing (surface normal via geom.project ->
    cosang); `d` = |signed distance| p_end->surface (geom.query); `r_x` = frustum
    radius at x_c; `R_b` = BALL_FACTOR * r_end. Batched: one geom.query for all
    embed points, one geom.project for all tip-interval exits."""
    embed_mis = [mi for mi in mis if members[mi].get("embed_end") in ("p0", "p1")]
    if not embed_mis:
        return {}
    p_ends = []; metas = []; xc_pts = []; xc_owner = []
    for mi in embed_mis:
        m = members[mi]; embed = m["embed_end"]
        p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
        D = p1 - p0; seg = float(np.linalg.norm(D))
        axis_ivs = sorted((iv for iv in ivmap.get(mi, []) if iv["ray"] == "axis"),
                          key=lambda iv: iv["s_a"])
        if embed == "p0":
            p_end = p0; r_end_raw = float(m["r0"]); r_root = float(m["r1"])
            u = (-D / seg) if seg > 1e-9 else D
        else:
            p_end = p1; r_end_raw = float(m["r1"]); r_root = float(m["r0"])
            u = (D / seg) if seg > 1e-9 else D
        ti = _tip_iv_index(axis_ivs, seg, embed) if seg > 1e-9 else None
        T = 0.0; xc = None
        if ti is not None:
            iv = axis_ivs[ti]; T = iv["s_b"] - iv["s_a"]
            if T > TOL_MM:
                s_x = iv["s_b"] if embed == "p0" else iv["s_a"]
                xc = p0 + (s_x / seg) * D
        r_end = max(r_end_raw, cfg.tip_r_min_mm)
        ball_factor = BALL_FACTOR_STUB if m["kind"] == "stub" else BALL_FACTOR_FOOT
        R_b = ball_factor * r_end
        k = len(metas)
        metas.append(dict(mi=mi, T=T, seg=seg, r_end=r_end, r_root=r_root,
                          R_b=R_b, u=u, has_xc=xc is not None))
        p_ends.append(p_end)
        if xc is not None:
            xc_pts.append(xc); xc_owner.append(k)
    d_all, _ = geom.query(np.array(p_ends))
    d_all = np.abs(np.atleast_1d(np.asarray(d_all, float)))
    if xc_pts:
        _, _, _, xc_fn = geom.project(np.array(xc_pts))
        xc_fn = np.atleast_2d(np.asarray(xc_fn, float))
    else:
        xc_fn = np.zeros((0, 3))
    norm_by_k = {kk: xc_fn[i] for i, kk in enumerate(xc_owner)}
    verdicts = {}
    for k, mt in enumerate(metas):
        T = mt["T"]; d = float(d_all[k]); R_b = mt["R_b"]; r_end = mt["r_end"]
        if T > TOL_MM and mt["has_xc"]:
            n = norm_by_k[k]; nn = float(np.linalg.norm(n))
            n = n / nn if nn > 1e-12 else n
            cosang = abs(float(np.dot(mt["u"], n)))
            frac = min(max(T / mt["seg"], 0.0), 1.0) if mt["seg"] > 1e-9 else 0.0
            r_x = r_end + (mt["r_root"] - r_end) * frac
        else:
            cosang = 1.0; r_x = r_end
        legal, _ = _scar_area_cap_legal(T, d, cosang, r_x, r_end, R_b)
        verdicts[mt["mi"]] = legal
    return verdicts


def _clearance_report_exact(members, cfg, geom):
    """Independent EXACT clearance + embed/transit re-check (SPEC-V5-GEOM
    sec 2.2, SPEC-EMBED-POLICY rule 4). Deliberately NOT sharing tree.Forest's
    exact helpers -- the independence covenant (a builder bug that mis-certifies
    a member must not also fool the verify gate): it calls only geom's GENERIC
    primitives (segment_clearance, penetration_intervals, query, project),
    deriving its own verdict/interval logic here.

    Two independent checks, each flagging a member at most once (distance
    collision reported with its min clearance; a rule-4 transit with no negative
    distance reported at min_clearance_mm 0.0 / the clearance value):

    1. THROUGH-TRANSIT / EMBED (SPEC-EMBED-POLICY, ratified cd989fe): the ported
       SCAR-AREA-CAP + rule-4 predicate REPLACES the retired arc-inset window
       sanction (`_axis_interval_illegal` is gone). Per-member SCAR-AREA-CAP
       verdicts (`_embed_scar_verdicts`) gate the tip exemption; `_transit_illegal`
       flags any member with a non-exempt inside-solid run > MIN_ILLEGAL_LEN_MM.
       Swept over the SAME population the census does: EVERY member (ungated by
       clearance sign), stubs AXIS-only (they hug their emergence surface, so an
       offset ray false-positives), non-stubs AXIS + capsule-offset (a strut
       whose axis is clean but whose capsule grazes a thin wall -- census 1138 --
       is caught). This makes the engine gate count exactly the census's illegal
       population from identical formulas.

    2. SWEPT-CAPSULE DISTANCE (SPEC-V5-GEOM sec 2.2): non-stubs get the
       segment_clearance min-distance test. A designed foot is embedded by
       construction, so the DISTANCE test masks that zone with the
       `_embed_inset_arc_exact` geometric inset -- a distance-gate mask ONLY, no
       longer an embed sanction (rule 4 above owns embed legality). Stubs are
       skipped here (they hug their own emergence surface, so any swept-distance
       check false-positives); their legality is entirely the rule-4 test.

    Append order is deterministic: stub transit violations in member order, then
    non-stub violations in member order."""
    out = []

    def _len(mi):
        return float(np.linalg.norm(np.asarray(members[mi]["p1"], float)
                                    - np.asarray(members[mi]["p0"], float)))

    # (1) gather kernel penetration intervals ONCE, over the SAME population the
    # census sweeps: EVERY member with real length, stubs AXIS-only, non-stubs
    # AXIS + capsule-offset (rays="all"). Ungated by clearance sign -- the census
    # has no distance test, so a rule-4 transit on a member whose capsule also
    # collides is still counted here (that member is then flagged once, below).
    stub_iv_mi = [mi for mi, m in enumerate(members)
                  if m["kind"] == "stub" and _len(mi) >= 1e-9]
    seg_iv_mi = [mi for mi, m in enumerate(members)
                 if m["kind"] != "stub" and _len(mi) >= 1e-9]
    stub_intervals = _batch_axis_intervals(geom, members, stub_iv_mi, rays="axis")
    seg_intervals = _batch_axis_intervals(geom, members, seg_iv_mi, rays="all")
    ivmap = {**stub_intervals, **seg_intervals}

    # (2) SCAR-AREA-CAP embed verdicts (designed embed ends, axis ray) -- gate the
    # rule-4 tip exemption. Only members with intervals need a verdict (a member
    # with no tip-anchored interval is flagged, or not, independent of it).
    verdicts = _embed_scar_verdicts(geom, members, list(ivmap.keys()), ivmap, cfg)

    # (3) non-stub swept-capsule DISTANCE re-check (segment_clearance), masking
    # each member's OWN designed embed zone with the _embed_inset_arc_exact
    # geometric inset (distance-gate mask only; SCAR-AREA-CAP rule 4 owns embed
    # legality). Flags a member whose SHAFT grazes OTHER geometry (worst < 0).
    seg_mi = [mi for mi, m in enumerate(members) if m["kind"] != "stub"]
    clr_by_mi = {}
    if seg_mi:
        p0s, p1s, r0s, r1s, ins0, ins1 = [], [], [], [], [], []
        for mi in seg_mi:
            m = members[mi]
            i0, i1 = _embed_inset_arc_exact(geom, m, cfg)
            p0s.append(np.asarray(m["p0"], float)); p1s.append(np.asarray(m["p1"], float))
            r0s.append(float(m["r0"])); r1s.append(float(m["r1"]))
            ins0.append(i0); ins1.append(i1)
        clr, argp = geom.segment_clearance(
            np.array(p0s), np.array(p1s), np.array(r0s), np.array(r1s),
            cfg.standoff_mm, inset0s=np.array(ins0), inset1s=np.array(ins1))
        clr_by_mi = {mi: (float(clr[k]), argp[k]) for k, mi in enumerate(seg_mi)}

    # (4) verdict assembly -- stub transit violations first (member order), then
    # non-stub violations (member order). Each member appears at most once.
    for mi in stub_iv_mi:
        if _transit_illegal(ivmap[mi], members[mi], verdicts.get(mi)):
            out.append(dict(member_idx=mi, kind="stub", min_clearance_mm=0.0,
                            at_point=None))
    for mi in seg_mi:
        m = members[mi]
        worst, ap = clr_by_mi.get(mi, (0.0, None))
        if worst < 0:
            out.append(dict(member_idx=mi, kind=m["kind"], min_clearance_mm=worst,
                            at_point=(ap.tolist() if ap is not None
                                      and np.all(np.isfinite(ap)) else None)))
        elif mi in ivmap and _transit_illegal(ivmap[mi], m, verdicts.get(mi)):
            out.append(dict(member_idx=mi, kind=m["kind"], min_clearance_mm=worst,
                            at_point=None))
    return out


def ball_breach_report(geom, members, balls, cfg, eps_mm=0.05):
    """SPEC-V5-GEOM sec 2.2: the census's headline defect class (44 ball
    breaches) was checked by NOTHING before v5. For every joint ball AND
    every embedded member end (a synthesized end-cap sphere -- an embedded
    tip transiting a sub-mm fin AT its end is exempt from BOTH clearance
    paths by design, so it escaped every census-era test), run
    geom.sphere_breach: a sphere spanning a feature thinner than its radius
    (poke-out > eps_mm) is a violation. Sanctioned shallow anchors into
    thick surface pass by construction (the test measures far-wall
    thickness, not surface contact). No-op unless `geom` is an exact oracle
    (has `.sphere_breach`) -- legacy/test callers with a cKDTree get [].

    Returns the COMBINED list (source="ball" and source="endcap" entries
    intermixed) -- this function's own contract is unchanged by R3. The
    caller (self_verify) is where the two categories get split for gating
    purposes; see self_verify's docstring / inline note for why."""
    if geom is None or not hasattr(geom, "sphere_breach"):
        return []
    members = joints.collect_members(members)
    centers, radii, tags = [], [], []
    for b in (balls or []):
        c = b["center"] if isinstance(b, dict) else b.center
        r = b["r"] if isinstance(b, dict) else b.r
        owner = b.get("owner") if isinstance(b, dict) else getattr(b, "owner", None)
        centers.append(np.asarray(c, float))
        radii.append(float(r))
        tags.append(dict(source="ball", owner=owner))
    for mi, m in enumerate(members):
        embed = m.get("embed_end")
        if embed == "p0":
            centers.append(np.asarray(m["p0"], float))
            radii.append(float(m["r0"]))
            tags.append(dict(source="endcap", member_idx=mi, end="p0", kind=m["kind"]))
        elif embed == "p1":
            centers.append(np.asarray(m["p1"], float))
            radii.append(float(m["r1"]))
            tags.append(dict(source="endcap", member_idx=mi, end="p1", kind=m["kind"]))
    if not centers:
        return []
    breached, thick, poke = geom.sphere_breach(np.array(centers), np.array(radii),
                                               eps_mm=eps_mm)
    out = []
    for i in range(len(centers)):
        if breached[i]:
            t = dict(tags[i])
            t.update(center=centers[i].tolist(), r=float(radii[i]),
                     feature_thickness_mm=float(thick[i]), poke_out_mm=float(poke[i]))
            out.append(t)
    return out


def clearance_report(mesh, members, cfg, surface_tree=None, geom=None):
    """v4/v5: independent full-envelope clearance re-check over the FINAL
    member list. Deliberately does not reuse tree.Forest.revalidate's
    certificate bookkeeping (a builder bug that mis-certifies a member
    must not also fool the verify gate) -- same sampling math (sample at
    clearance_sample_step_mm, linear radius taper, skip the embed-end
    inset described below), applied unconditionally to every member
    regardless of cert status, as an honest independent check.
    `surface_tree` (see routing.build_surface_tree), if given, uses the
    fast approximate path real runs always use; None falls back to exact
    trimesh.proximity (routing._surface_distances) the same way
    routing.check_clearance's legacy path does -- fine for a handful of
    members in a test, not for a real mesh (see build_surface_tree's own
    docstring for why). No surface_tree also means no pitch_bound_mm
    (routing.surface_tree_pitch_bound_mm needs a tree to measure) -- the
    exact-trimesh path is inherently pitch-uncertainty-free (it queries
    the true continuous surface, not an approximating point cloud), so
    pitch_bound_mm is 0.0 in that branch, correctly (nothing to budget
    for).

    "stub" members are skipped entirely, same as routing.
    sample_route_envelope's existing precedent and tree.Forest.
    revalidate's identical rule (see that docstring for why a fixed inset
    alone isn't enough for a member that travels along the surface normal
    at a near-grazing angle).

    Embed-end exemption (p6-routing, Findings A/B): a SECOND, INDEPENDENT
    implementation of the SAME radius- and pitch-principled inset
    tree.Forest.revalidate derives (see that function's docstring for the
    full derivation) -- inset = contact_depth_mm + pitch_bound_mm + r_end
    + standoff_mm. Deliberately reimplemented here rather than calling
    into tree.py (preserving this function's own stated independence:
    "a builder bug... must not also fool the verify gate" applies just as
    much to a clearance-gate bug as a certificate bug) -- only the
    GENERIC point-cloud-density primitive (routing.surface_tree_
    pitch_bound_mm, exactly the same kind of shared low-level utility
    this function already calls via routing._surface_distances /
    routing.build_surface_tree) is shared, never the masking/exemption
    logic itself. This was ALSO the OLD fixed contact_depth_mm+0.2 inset
    (byte-identical to the pre-fix tree.py formula, independently proven
    stale on the real Weeping Treant model: 26 false-positive clearance_
    violations on table_mesh alone, all traced to this exact formula and
    confirmed to vanish when this same fix is applied).

    Returns list of dicts {member_idx, kind, min_clearance_mm, at_point}.
    """
    members = joints.collect_members(members)
    out = []
    if not members:
        return out
    # v5 exact swap (SPEC sec 2.2): a surface-EXACT oracle routes to the
    # independent exact re-check; a plain cKDTree (tests/legacy) keeps the
    # sampled path below.
    if geom is not None and hasattr(geom, "segment_clearance"):
        return _clearance_report_exact(members, cfg, geom)
    pitch_bound_mm = (routing.surface_tree_pitch_bound_mm(
        surface_tree, percentile=cfg.surface_pitch_bound_percentile)
        if surface_tree is not None else 0.0)
    all_pts, all_r, all_mi = [], [], []
    for mi, m in enumerate(members):
        if m["kind"] == "stub":
            continue
        p0, p1 = np.asarray(m["p0"], float), np.asarray(m["p1"], float)
        r0, r1 = float(m["r0"]), float(m["r1"])
        length = float(np.linalg.norm(p1 - p0))
        if length < 1e-9:
            pts = p0[None, :]
            radii = np.array([max(r0, r1)])
        else:
            n = max(int(length / cfg.clearance_sample_step_mm), 1)
            t = np.linspace(0.0, 1.0, n + 1)
            if m.get("embed_end") == "p0":
                inset = cfg.contact_depth_mm + pitch_bound_mm + r0 + cfg.standoff_mm
                t = t[t * length >= inset]
            if m.get("embed_end") == "p1":
                inset = cfg.contact_depth_mm + pitch_bound_mm + r1 + cfg.standoff_mm
                t = t[(1 - t) * length >= inset]
            if len(t) == 0:
                continue
            pts = p0[None, :] + t[:, None] * (p1 - p0)[None, :]
            radii = r0 + t * (r1 - r0)
        all_pts.append(pts)
        all_r.append(radii)
        all_mi.append(np.full(len(radii), mi, dtype=int))
    if not all_pts:
        return out
    pts_all = np.concatenate(all_pts)
    r_all = np.concatenate(all_r)
    mi_all = np.concatenate(all_mi)
    ub = float(r_all.max()) + cfg.standoff_mm
    dist = routing._surface_distances(mesh, pts_all, surface_tree, upper_bound=ub)
    clearance = dist - r_all - cfg.standoff_mm
    for mi, m in enumerate(members):
        mask = mi_all == mi
        if not mask.any():
            continue
        worst = float(clearance[mask].min())
        if worst < 0:
            widx = np.nonzero(mask)[0][int(np.argmin(clearance[mask]))]
            out.append(dict(member_idx=mi, kind=m["kind"], min_clearance_mm=worst,
                             at_point=pts_all[widx].tolist()))
    return out


def murray_report(forest, cfg, tolerance=0.05):
    """New (design-v4.md #5, #11): per-junction Murray-law conservation
    sanity check, r_parent^4 >= sum(r_child^4) * (1 - tolerance). A
    parent member's radius at its p0 (the junction end -- children attach
    there, see tree.Forest's parent/child convention) may legitimately
    sit ABOVE conservation (physics floors -- min radius, transient spans
    -- may push it up), but never meaningfully below: that would mean an
    upstream sizing bug. Only evaluated at true branch junctions (>= 2
    children) -- a single pass-through attachment is topology, not a
    merge, and has nothing to conserve against. Honestly-unrouted
    terminals never enter the forest, so they are excluded by
    construction (nothing to sum).

    Returns list of dicts {member_idx, r_parent, r_child_equiv, deficit}.
    """
    violations = []
    for pi, children in enumerate(forest.children):
        if len(children) < 2:
            continue
        parent = forest.members[pi]
        r_parent4 = float(parent.r0) ** 4
        sum_child4 = sum(float(forest.members[ci].r1) ** 4 for ci in children)
        if r_parent4 < sum_child4 * (1.0 - tolerance):
            violations.append(dict(member_idx=pi, r_parent=float(parent.r0),
                                    r_child_equiv=sum_child4 ** 0.25,
                                    deficit=sum_child4 - r_parent4))
    return violations


def load_report(carried_area_mm2, tip_r_mm, cfg):
    """Per-contact realized safety factor against green tensile strength;
    flags any contact whose tip radius got CLAMPED (design.md's 0.2-1.0mm
    removal-friendly band) below what its carried load actually needs --
    those are real capacity shortfalls, not just cosmetic clamping, and
    should drive adding a neighboring contact to split the load."""
    area_m2 = np.asarray(carried_area_mm2) * 1e-6
    force_n = area_m2 * cfg.peel_pressure_pa
    r_m = np.asarray(tip_r_mm) / 1000.0
    tensile_pa = cfg.tensile_green_mpa * 1e6
    stress_pa = force_n / (np.pi * np.maximum(r_m, 1e-9) ** 2)
    realized_sf = tensile_pa / np.maximum(stress_pa, 1e-9)
    out = []
    for i, sf in enumerate(realized_sf):
        if sf < cfg.tip_safety_factor - 1e-6:
            out.append(dict(contact_idx=i, realized_sf=float(sf),
                             required_sf=cfg.tip_safety_factor))
    return out


def minima_report(minima, routed_contacts, cfg):
    """Layer-island check (v3.6, Craig's rule): every island-birth
    minimum (perception.local_z_minima -- already filtered to the ones
    that do NOT connect to earlier-printed material within the layer
    tolerance) must have a ROUTED contact touching its birth region.
    Proximity elsewhere on the model does not count: a contact 1mm away
    laterally on a shallow toe is 10+ layers above the island's birth.
    Returns the unsupported minima as dicts {z, position, min_gap_mm}."""
    if not minima:
        return []
    out = []
    positions = np.array([np.asarray(c["position"], float)
                          for c in routed_contacts]) if routed_contacts else None
    tree = cKDTree(positions) if positions is not None and len(positions) else None
    for m in minima:
        if tree is None:
            out.append(dict(z=m["z"], position=np.asarray(m["position"]).tolist(),
                            min_gap_mm=float("inf")))
            continue
        dist, _ = tree.query(m["region_pts"],
                             workers=routing.query_workers(len(m["region_pts"])))
        gap = float(np.min(dist))
        if gap > cfg.minima_contact_radius_mm:
            out.append(dict(z=m["z"], position=np.asarray(m["position"]).tolist(),
                            min_gap_mm=gap))
    return out


def island_birth_report(island_births, routed_contacts, cfg):
    """I3: the HONEST island-birth coverage gate (v4 p6, Craig's strict
    criterion, verbatim: "every island birth ... must have a support
    contact within ~2 real print layers (0.1mm z) landing ON the birth
    component's footprint (or within that contact's own attach radius of
    its boundary)"). Consumes perception.island_births (I1's ground-truth
    slice-based birth detection at the real print layer height), NOT
    local_z_minima's birth regions -- minima_report (above) already gates
    those independently; this is a second, geometrically-independent
    check, not a stricter version of the same one.

    A birth is covered iff some ROUTED contact (proximity elsewhere,
    or an unrouted stub, does not count -- an unrouted contact provides
    zero physical support) satisfies BOTH:
      - z: |contact.z - birth.z| <= cfg.minima_connect_layers *
        cfg.layer_height_mm (the ~2-real-print-layer / 0.1mm budget);
      - xy: on the birth polygon, or within
        perception.ISLAND_ATTACH_REACH_MM of its boundary.
    This REPLACES minima_report's cfg.minima_contact_radius_mm as the
    coverage radius for THIS data -- that field's own docstring says it
    was "NOT a coverage radius... only absorbs sampling gaps", which is
    exactly how the pre-p6 engine could self-certify while a full-model
    ground-truth audit found real uncovered births (design scratchpad:
    island_audit_fullmodel_0.05mm_result.json, 77/211 uncovered against a
    gate that reported 51).

    Returns list of uncovered-birth dicts {x, y, z, area_mm2, gap_mm,
    gap_layers}, gap_mm being the true nearest-in-3D-space routed contact
    (an honest number even when nothing anywhere qualifies on xy) -- an
    empty list means every slice-detected birth is genuinely supported at
    birth, not merely "close to something.\""""
    if not island_births:
        return []
    z_tol_mm = cfg.minima_connect_layers * cfg.layer_height_mm
    attach_r = perception.ISLAND_ATTACH_REACH_MM
    positions = np.array([np.asarray(c["position"], float)
                          for c in routed_contacts]) if routed_contacts else np.zeros((0, 3))
    out = []
    for b in island_births:
        bz = b["z"]
        covered = False
        if len(positions):
            z_ok = np.nonzero(np.abs(positions[:, 2] - bz) <= z_tol_mm)[0]
            for k in z_ok:
                cx, cy, _cz = positions[k]
                ok, _d = perception.island_birth_xy_ok(float(cx), float(cy), b["loop"], attach_r)
                if ok:
                    covered = True
                    break
        if covered:
            continue
        if len(positions):
            d3 = np.linalg.norm(
                positions - np.array([b["x"], b["y"], bz]), axis=1)
            gap = float(d3.min())
        else:
            gap = float("inf")
        out.append(dict(x=b["x"], y=b["y"], z=bz, area_mm2=b["area_mm2"],
                        gap_mm=gap,
                        gap_layers=(gap / cfg.layer_height_mm
                                    if np.isfinite(gap) else float("inf"))))
    return out


def connectivity_report(members, plate_z, cfg, surface_tree=None, geom=None):
    """v3.7 hard gate (Craig's Chitubox review), signature-migrated for v4
    (design-v4.md #2): the stale-brace-leak and chained-Y-branch defects
    both manifested the same way in the emitted file -- a member with an
    endpoint touching NOTHING (a floating rod). This makes "nothing
    floats" a checked invariant instead of a hope.

    Every member (joints.collect_members) has two endpoints; each must be
    ATTACHED by at least one of:
      (a) within 0.25mm of ANOTHER member's axis segment (point-to-segment
          distance, self excluded) -- the general "something is really
          there" rule, covering junctions and any mid-member landing;
      (b) plate level (abs(z - plate_z) <= 0.2mm) -- a plate-rooted
          member's base. (v4 simplification, recorded: v3 widened this
          band by flare_len_mm to cover an emission-only flare/pad detail
          below a column's modeled base; v4's plate-descent candidate has
          no such offset -- its root member's p1 lands exactly at
          plate_z -- so the widening is dropped rather than carried
          forward as unexplained slack.)
      (c) embedded in the model surface (within cfg.contact_depth_mm+0.45)
          -- ONLY for the endpoint that is that member's DESIGNED
          embed_end (replaces v3's kind/sentinel table: a stub's tip
          (p0), a model-anchor strut's foot (p1), or any other member
          explicitly built with an embed_end). Any OTHER endpoint merely
          being near the model is explicitly NOT attachment -- that would
          silently wave through an unbraced floating rod that happens to
          pass close to the mesh. Preserved verbatim: with no
          surface_tree, an embed-eligible endpoint is honestly flagged
          rather than silently passed (the fixture pinning this case
          stays a guard on engine.py always supplying one for real runs).

    Returns a list of violation dicts {kind, endpoint (xyz list), gap_mm}
    -- gap_mm is the distance to the nearest legitimate attachment of the
    allowed kinds for that endpoint. Vectorized (member-axis distances
    computed in point-chunks against all member axes at once) to stay well
    under a second at real-model scale.
    """
    members = joints.collect_members(members)
    if not members:
        return []

    m_p0 = np.array([m["p0"] for m in members], dtype=float)
    m_p1 = np.array([m["p1"] for m in members], dtype=float)
    d_axis = m_p1 - m_p0
    L2 = np.einsum("ij,ij->i", d_axis, d_axis)
    L2safe = np.where(L2 < 1e-18, 1.0, L2)
    zero_len = L2 < 1e-18

    # endpoints: (member_idx, point, embed_eligible)
    ep_mi, ep_pts, ep_embed = [], [], []
    for mi, m in enumerate(members):
        embed0 = m.get("embed_end") == "p0"
        embed1 = m.get("embed_end") == "p1"
        ep_mi.append(mi); ep_pts.append(m["p0"]); ep_embed.append(embed0)
        ep_mi.append(mi); ep_pts.append(m["p1"]); ep_embed.append(embed1)

    ep_mi = np.array(ep_mi)
    pts = np.array(ep_pts, dtype=float)
    embed_ok = np.array(ep_embed, dtype=bool)

    n_ep = len(ep_mi)
    best_axis_dist = np.full(n_ep, np.inf)
    CHUNK = 256
    for start in range(0, n_ep, CHUNK):
        end = min(start + CHUNK, n_ep)
        chunk_pts = pts[start:end]                              # (c,3)
        chunk_mi = ep_mi[start:end]                              # (c,)
        rel = chunk_pts[:, None, :] - m_p0[None, :, :]           # (c,n,3)
        t = np.einsum("cnk,nk->cn", rel, d_axis) / L2safe[None, :]
        t = np.clip(t, 0.0, 1.0)
        proj = m_p0[None, :, :] + t[:, :, None] * d_axis[None, :, :]
        dist = np.linalg.norm(chunk_pts[:, None, :] - proj, axis=2)  # (c,n)
        if zero_len.any():
            dist[:, zero_len] = np.linalg.norm(
                chunk_pts[:, None, :] - m_p0[None, zero_len, :], axis=2)
        rows = np.arange(end - start)
        dist[rows, chunk_mi] = np.inf  # exclude the endpoint's own member
        best_axis_dist[start:end] = dist.min(axis=1)

    attached_a = best_axis_dist <= 0.25

    z = pts[:, 2]
    attached_b = np.abs(z - plate_z) <= 0.2

    # v5 (SPEC-V5-GEOM sec 2.6, review point 9): prefer the surface-EXACT
    # oracle for the embed distance when supplied (geom.query is signature-
    # compatible with cKDTree.query). DIRECTIONAL effect, intended and
    # documented: an exact point-to-surface distance is <= the nearest
    # sampled-cloud distance, so surf_dist can only SHRINK -> strictly MORE
    # embed-eligible endpoints clear attached_c (the cloud over-estimated
    # detachment). This loosens the gate only in that one direction; a real
    # floating rod (far from the surface) still fails. Falls back to the
    # sampled `surface_tree` when no exact oracle is passed (tests/legacy).
    oracle = geom if geom is not None else surface_tree
    surf_dist = np.full(n_ep, np.inf)
    if oracle is not None and embed_ok.any():
        idx = np.nonzero(embed_ok)[0]
        dd, _ = oracle.query(pts[idx])
        surf_dist[idx] = dd
    attached_c = embed_ok & (surf_dist <= cfg.contact_depth_mm + 0.45)

    attached = attached_a | attached_b | attached_c
    plate_gap = np.abs(z - plate_z)
    gap_candidates = np.stack([best_axis_dist, plate_gap,
                                np.where(embed_ok, surf_dist, np.inf)], axis=1)
    gap_mm_arr = gap_candidates.min(axis=1)

    violations = []
    for k in np.nonzero(~attached)[0]:
        mi = int(ep_mi[k])
        gap = float(gap_mm_arr[k])
        violations.append(dict(kind=members[mi]["kind"],
                                endpoint=np.asarray(pts[k], float).tolist(),
                                gap_mm=(gap if np.isfinite(gap) else None)))
    return violations


def self_verify(mesh, islands, contacts, members, carried_area_mm2, tip_r_mm, cfg,
                 plate_z, surface_tree=None, minima=None, forest=None, unrouted=(),
                 gate_on_removal=False, island_births=None, geom=None, balls=None,
                 gate_on_ball_breach=True):
    """Full design-v4.md #11 gate. Returns a report dict; `ok` is True
    only if all checks are clean: coverage, clearance, load, layer-island
    minima (only when the caller supplies `minima` from
    perception.local_z_minima; engine.run always does), the v4 p6
    slice-based island-birth gate (only when the caller supplies
    `island_births` from perception.island_births; engine.run always
    does), connectivity, and -- new in v4 -- per-segment printability and
    per-junction Murray conservation.

    `members` replaces v3's conn_parts=(routes,trunks,risers,braces,
    plate_z): one flat member list (tree.Member objects or dicts) drives
    clearance_report, connectivity_report, AND printability_report (p6-
    reach consistency fix -- see below). `forest`, if given (a tree.
    Forest -- engine.run always supplies one in real runs, same optional
    pattern as `minima`/`island_births`/`surface_tree`), additionally
    gates printability and Murray conservation, and computes the
    design-v4.md #10 removal_report (`unrouted`, the caller's honestly-
    unrouted contact-index list, feeds forest_trees so a never-built stub
    isn't reported as its own unremovable singleton tree); omitting
    `forest` skips all three (reported as empty/None) rather than gating
    on them.

    p6-reach consistency fix (verifier-authorized, one line): printability
    used to run over the RAW `forest.members` (every member this Forest
    has ever appended, including tree.Forest.dead retirees -- rewire.py's
    twig-prune/degenerate-retire moves mark a member dead rather than
    delete it, by design, so checkpoint/rollback indices stay stable) --
    the ONE check in this function not built on the caller-filtered,
    dead-excluding `members` list every sibling check (clearance,
    connectivity) already uses. That was a consistency bug, not extra
    gate strictness: a dead member is never emitted or printed (emission
    consumes engine.live_member_lists' own filtered output, the same
    live set `members` is built from), so a printability rule checking
    dead geometry was gating on something that will never actually print
    -- the gate's own referent is what gets printed. Fixed by running
    printability_report over `members` (the SAME filtered basis as its
    sibling checks) instead of `forest.members` -- `forest` is still
    required to gate this sub-report at all (per the docstring above),
    just no longer the SOURCE of what gets checked.

    `gate_on_removal` (Phase 5 wiring decision, see the p5-removal commit
    message; amended 2026-07-22): the charter (#10) originally called the
    removal gate's hard-failure case -- a residual set no removal order
    clears -- "a BLOCKER gate, wired into verify ok". Craig's 2026-07-22
    ruling (design-v4.md #10 amendment) supersedes that language:
    removal_report is diagnostic/report-only, removability is the least
    important optimization axis, and nothing it finds -- caged sets
    included -- may block an otherwise-superior layout. Per that ruling,
    removal_report/removal_violations are ALWAYS computed and surfaced
    (measurement only); gate_on_removal stays an inert toggle no shipped
    caller sets (setting it True would still AND removal_violations into
    `ok` mechanically, but doing so now runs against the charter, not
    with it). Defaults False so no existing caller's acceptance behavior
    changes by importing this function. `island_births`'s gate is NOT
    inert: per Craig's strict I3 ruling (see island_birth_report), an
    uncovered slice-detected island birth DOES fail `ok` -- removal is a
    soft preference axis, island-birth coverage is not.

    `ball_breach_violations` (R3, 2026-07-23): census-parity scope -- ONLY
    actual joint balls (antipodal-span semantics, matching the independent
    census's reference implementation both algorithmically and in scope).
    Gated (ANDed into `ok` when `gate_on_ball_breach`). A member's own
    embedded end-cap sphere (SPEC-V5-GEOM sec 2.2 amendment 5) is a
    DIFFERENT, report-only category returned as
    `embed_end_breach_violations` -- always computed and surfaced, never
    ANDed into `ok`. See the inline note above `ball_breach_all` in this
    function body for the full finding and rationale."""
    coverage = coverage_report(mesh, islands, contacts, cfg)
    clearance = clearance_report(mesh, members, cfg, surface_tree=surface_tree, geom=geom)
    # R3 (ball-gate semantics, 2026-07-23): ball_breach_report synthesizes
    # and tests TWO categories against geom.sphere_breach -- actual joint
    # balls (source="ball") AND synthesized embedded-end-cap spheres
    # (source="endcap", SPEC-V5-GEOM sec 2.2 amendment 5). The full-scale
    # acceptance run exposed a semantics gap between this gate and the
    # independent census's reference "antipodal-span" ball test: census's
    # test -- and its 15-true-breach headline count -- runs ONLY over
    # actual joint balls (S["balls"]); it never evaluates a member's own
    # embedded tip as a sphere at all (that geometry is instead the axis/
    # capsule through-transit predicate's job, R1/R2's territory). On the
    # real Weeping Treant forest the combined ball_breach_report list was
    # 199 (15 ball + 184 endcap) while the census's independently computed
    # antipodal-span count for the SAME 1049-ball set was exactly 15 --
    # i.e. the two implementations agree EXACTLY on the ball category (same
    # algorithm, same points: geometry_exact.py's sphere_breach is
    # literally docstringed "Census ball breach test", and the kernel's
    # breach_one is the same math) and the entire 184-count gap is the
    # endcap category the census never touches. Those 184 endcap flags
    # cluster at the measurement floor (feature_thickness ~0.05-0.11mm,
    # r=0.2mm stub tips) concentrated in dense crown foliage -- consistent
    # with the endcap check's local "wall thickness" reasoning being
    # unreliable on this non-watertight, double-walled mesh at typical
    # embed depths, not with 184 genuine visible-defect breaches.
    #
    # FIX: the HARD GATE and the reported `ball_breach_violations` --
    # this gate's namesake -- are restricted to the census's own reference
    # scope: actual joint balls only (antipodal-span semantics, verbatim
    # algorithm, now also verbatim SCOPE). The embedded end-cap check
    # itself is NOT deleted (amendment 5's blind spot closure is still
    # real, still computed, still reported for Phase-8 relocation triage)
    # -- it is surfaced separately, as `embed_end_breach_violations`, a
    # REPORT-ONLY diagnostic that does not gate `ok`. This is deliberately
    # scoped as R3-only: aligning the embed-end check's own thickness/inset
    # accounting with the census (so its flags stop clustering at the
    # measurement floor) is R4's job, not this one.
    ball_breach_all = ball_breach_report(geom, members, balls, cfg)
    ball_breach_violations = [v for v in ball_breach_all if v.get("source") == "ball"]
    embed_end_breach_violations = [v for v in ball_breach_all if v.get("source") == "endcap"]
    load = load_report(carried_area_mm2, tip_r_mm, cfg)
    unsupported_minima = minima_report(minima, contacts, cfg) if minima is not None else []
    uncovered_island_births = island_birth_report(island_births, contacts, cfg) \
        if island_births is not None else []
    connectivity_violations = connectivity_report(members, plate_z, cfg,
                                                   surface_tree=surface_tree, geom=geom)
    if forest is not None:
        printability_violations = tree_mod.printability_report(members, cfg)
        murray_violations = murray_report(forest, cfg)
        removal = removal_mod.removal_report(forest, mesh, surface_tree, cfg, unrouted=unrouted)
    else:
        printability_violations = []
        murray_violations = []
        removal = None
    removal_violations = list(removal["blocked_trees"]) if removal is not None else []
    uncovered = [c for c in coverage if not c["covered"]]
    ok = (len(uncovered) == 0 and len(clearance) == 0 and len(load) == 0
          and len(unsupported_minima) == 0 and len(uncovered_island_births) == 0
          and len(connectivity_violations) == 0
          and len(printability_violations) == 0 and len(murray_violations) == 0)
    if gate_on_removal:
        ok = ok and len(removal_violations) == 0
    # v5 (SPEC sec 2.2, line 280; scope pinned to census parity by R3,
    # 2026-07-23): ball_breach_violations are a HARD GATE, same class as
    # clearance -- ANDed into `ok`. This is the census's headline defect
    # class (44/48 crossings), previously checked by nothing (the sampled
    # engine reported zero of the 12 the exact core finds on the table).
    # The gate is now satisfiable because the builder side stops CREATING
    # breaching anchors: cco model-anchor and bracing model-rung candidate
    # scoring reject a thin-twig anchor via geom.sphere_breach on the
    # anchor-end ball (SPEC sec 2.2/2.3), and the exact ray-cast anchor pool
    # (exterior-face centroids) lands feet on true surface. verify's gate is
    # the independent re-check behind that. `gate_on_ball_breach` defaults
    # True; it is a no-op for non-exact callers (a plain cKDTree / None geom
    # makes ball_breach_report return [] by construction, so both
    # ball_breach_violations and embed_end_breach_violations are empty), so
    # tests/legacy paths are unaffected. embed_end_breach_violations is
    # NEVER gated (see the R3 note above `ball_breach_all` for why) --
    # always surfaced, not ANDed into `ok` under any flag.
    if gate_on_ball_breach:
        ok = ok and len(ball_breach_violations) == 0
    return dict(
        ok=ok,
        uncovered_islands=uncovered,
        clearance_violations=clearance,
        ball_breach_violations=ball_breach_violations,
        embed_end_breach_violations=embed_end_breach_violations,
        load_violations=load,
        unsupported_minima=unsupported_minima,
        uncovered_island_births=uncovered_island_births,
        connectivity_violations=connectivity_violations,
        printability_violations=printability_violations,
        murray_violations=murray_violations,
        removal_report=removal,
        removal_violations=removal_violations,
    )


def orientation_scar_cost(mesh, cfg, rotations, delicacy=None):
    """Reusable scar-cost orientation scorer (design.md requirement 3).
    `rotations` is a list of 3x3 rotation matrices (candidate print
    orientations, applied to the AS-IS mesh frame). For each, scores
    sum(face_area * delicacy) over faces that would be down-facing under
    that rotation -- lower is better (less delicate surface exposed to
    support-facing down/side orientation).

    `delicacy` may be a precomputed (points, normals, weights, query) tuple
    from perception.delicacy_field(); computed fresh if not given (a
    caller evaluating many rotations should compute it once and pass it
    in -- it does not depend on rotation).
    """
    if delicacy is None:
        delicacy = perception.delicacy_field(mesh, cfg)
    _, _, _, query = delicacy
    centers = mesh.triangles_center
    areas = mesh.area_faces
    face_delicacy = query(centers)
    normals = mesh.face_normals
    scores = []
    thresh = -np.sin(np.radians(cfg.overhang_angle_deg))
    for R in rotations:
        rn = normals @ np.asarray(R).T
        down = rn[:, 2] < thresh
        scores.append(float(np.sum(areas[down] * face_delicacy[down])))
    return scores
