"""Arterial trunk consolidation (v3, from first in-Chitubox review).

v2 gave every contact its own plate column -- a dense forest of
near-identical rods, barely braced, running the full model height. v3
replaces that with a two-level structure: shared vertical TRUNKS from the
plate, and thin diagonal SHOOTS from each contact's stub down to a trunk.
A solo column is just the degenerate case (a trunk with one shoot of
zero lateral reach), so one code path serves both.

Rules (all from the review + the design charter's own mechanics):
  - every member leans <= cfg.max_segment_angle_deg from vertical; no
    floating horizontal runs, ever (printability: near-horizontal spans
    print as unsupported bridge islands until both ends merge).
  - trunk placement searches for a CLEAR column up front (spiral offsets,
    checked against the surface tree) instead of v2's route-then-repair.
  - trunk radius accumulates load: peel tension sizing over the summed
    carried areas of its shoots plus slenderness sizing over its height;
    shoots are sized for their own load and length. Members genuinely
    vary -- identical-thickness forests were review finding #5.
"""
import numpy as np


def _angle_ok(p0, p1, cfg, tol_deg=0.5):
    """True if segment p0->p1 leans no more than max_segment_angle_deg
    from vertical (or is a pure point)."""
    d = np.asarray(p1, float) - np.asarray(p0, float)
    L = np.linalg.norm(d)
    if L < 1e-9:
        return True
    lean = np.degrees(np.arccos(min(1.0, abs(d[2]) / L)))
    return lean <= cfg.max_segment_angle_deg + tol_deg


def _column_clear(surface_tree, xy, z_lo, z_hi, r, cfg, step=None):
    """Fast envelope check for a vertical column at `xy` spanning
    [z_lo, z_hi] with radius r: every sample must keep r + standoff from
    the model surface."""
    step = step or cfg.clearance_sample_step_mm * 2
    n = max(int((z_hi - z_lo) / step), 1)
    z = np.linspace(z_lo, z_hi, n + 1)
    pts = np.column_stack([np.full(n + 1, xy[0]), np.full(n + 1, xy[1]), z])
    ub = r + cfg.standoff_mm
    dist, _ = surface_tree.query(pts, distance_upper_bound=ub)
    return bool(np.all(np.isinf(dist)))


def _segment_clear(surface_tree, p0, p1, r, cfg, skip_from_p0=0.0):
    """Envelope check for an arbitrary segment (used for shoots; the end
    at the trunk is open space, the end at the stub is near the model, so
    an inset can skip the deliberate near-surface start)."""
    p0 = np.asarray(p0, float)
    p1 = np.asarray(p1, float)
    L = float(np.linalg.norm(p1 - p0))
    if L < 1e-9:
        return True
    n = max(int(L / (cfg.clearance_sample_step_mm * 2)), 1)
    t = np.linspace(0, 1, n + 1)
    if skip_from_p0 > 0:
        t = t[t * L >= skip_from_p0]
        if len(t) == 0:
            return True
    pts = p0[None, :] + t[:, None] * (p1 - p0)[None, :]
    ub = r + cfg.standoff_mm
    dist, _ = surface_tree.query(pts, distance_upper_bound=ub)
    return bool(np.all(np.isinf(dist)))


class Trunk:
    __slots__ = ("xy", "z_top", "r", "r_axial", "shoots", "carried_mm2",
                 "n_braces", "eff_len", "foot_kind", "parent_idx", "z_split",
                 "z_start", "extra_shoots")

    def __init__(self, xy, z_top):
        self.xy = np.asarray(xy, float)
        self.z_top = float(z_top)
        self.r = 0.0          # sized after assignment (accumulated load)
        self.r_axial = 0.0     # tension-only floor (load, no slenderness)
        self.shoots = []       # list of contact indices
        self.carried_mm2 = 0.0
        self.n_braces = 0
        self.eff_len = 0.0
        # v3.5 Y-branching: 'plate' trunks root on the raft; 'branch'
        # trunks sprout from a parent trunk at z_split via a 45-degree
        # diagonal, and their column exists only from z_start upward.
        self.foot_kind = "plate"
        self.parent_idx = None
        self.z_split = None
        self.z_start = None
        self.extra_shoots = 0  # children's shoots, for parent load sizing

    def base_z(self, plate_z):
        return self.z_start if self.foot_kind == "branch" else plate_z


def _junction_z(stub_end, trunk_xy, cfg):
    """z where a shoot from stub_end meets a trunk at trunk_xy, descending
    at the printability angle (45deg default: drop == lateral reach)."""
    dxy = float(np.linalg.norm(np.asarray(trunk_xy) - np.asarray(stub_end[:2])))
    drop = dxy / max(np.tan(np.radians(cfg.max_segment_angle_deg)), 1e-6)
    return stub_end[2] - drop, dxy


def plan(contacts, tip_r, carried, stub_ends, plate_z, surface_tree, cfg):
    """Assign every contact to a trunk. Returns (trunks, assignment) where
    assignment[i] = (trunk_idx, junction_z, clear:bool). Contacts are
    processed lowest-stub-first so trunks get established near the plate
    and taller contacts join existing arteries.

    A contact joins the nearest trunk whose junction works (reach,
    angle, shoot clearance, trunk-extension clearance, shoot budget);
    otherwise it founds a new trunk at the first clear spiral offset from
    its own stub xy. If nothing clears, the straight-down column is kept
    and the violation surfaces in self_verify (honest best effort)."""
    order = np.argsort([se[2] for se in stub_ends])
    trunks = []
    assignment = [None] * len(contacts)
    tan_a = np.tan(np.radians(cfg.max_segment_angle_deg))

    for i in order:
        se = stub_ends[i]
        r_shoot = max(float(tip_r[i]), cfg.min_shoot_r_mm)
        chosen = None
        # -- try joining an existing trunk: nearest first, but prefer
        # trunks AHEAD of the stub's outward direction -- a trunk behind
        # it forces the shoot to double back over the contact (fourth
        # review: 115/242 routes zigzagged for exactly this reason)
        out_xy = se[:2] - np.asarray(contacts[i]["position"], float)[:2]
        n_out = np.linalg.norm(out_xy)
        out_xy = out_xy / n_out if n_out > 1e-6 else None

        def _join_cost(t):
            d = float(np.linalg.norm(trunks[t].xy - se[:2]))
            if out_xy is not None and d > 1e-6:
                behind = np.dot(trunks[t].xy - se[:2], out_xy) < 0
                if behind:
                    d += 6.0  # try ahead-side trunks first
            return d

        cand = sorted(range(len(trunks)), key=_join_cost)
        for t in cand:
            trunk = trunks[t]
            if len(trunk.shoots) >= cfg.trunk_max_shoots:
                continue
            jz, dxy = _junction_z(se, trunk.xy, cfg)
            if dxy > cfg.shoot_max_xy_mm or jz <= plate_z + 1.0:
                continue
            jp = np.array([trunk.xy[0], trunk.xy[1], jz])
            if not _segment_clear(surface_tree, se, jp, r_shoot, cfg,
                                   skip_from_p0=cfg.standoff_mm):
                continue
            # trunk may need to grow upward to reach this junction
            if jz > trunk.z_top and not _column_clear(
                    surface_tree, trunk.xy, trunk.z_top, jz,
                    max(trunk.r, cfg.min_trunk_r_mm), cfg):
                continue
            chosen = (t, jz, True)
            break
        # -- found a new trunk: spiral search for a clear column. The
        # JOIN cap (shoot_max_xy_mm) is an economy limit; when FOUNDING,
        # the shoot may reach as far as the search spiral itself goes --
        # a longer one-off diagonal beats an unclear straight-down column.
        if chosen is None:
            found = None
            found_max_xy = max(cfg.trunk_search_radii_mm)
            for radius in cfg.trunk_search_radii_mm:
                dirs = 1 if radius == 0 else cfg.trunk_search_directions
                for k in range(dirs):
                    ang = 2 * np.pi * k / dirs
                    xy = se[:2] + radius * np.array([np.cos(ang), np.sin(ang)])
                    jz, dxy = _junction_z(se, xy, cfg)
                    if dxy > found_max_xy or jz <= plate_z + 1.0:
                        continue
                    jp = np.array([xy[0], xy[1], jz])
                    shoot_ok = _segment_clear(surface_tree, se, jp, r_shoot, cfg,
                                               skip_from_p0=cfg.standoff_mm)
                    col_ok = _column_clear(surface_tree, xy, plate_z, jz,
                                            cfg.min_trunk_r_mm, cfg)
                    if shoot_ok and col_ok:
                        found = (xy, jz)
                        break
                if found:
                    break
            if found is None:
                # honest fallback: straight column under the stub; verify
                # will surface whatever it clips.
                xy = se[:2].copy()
                jz = se[2]
                clear = False
            else:
                xy, jz = found
                clear = True
            trunk = Trunk(xy, jz)
            trunks.append(trunk)
            chosen = (len(trunks) - 1, jz, clear)

        t, jz, clear = chosen
        trunk = trunks[t]
        trunk.shoots.append(i)
        trunk.carried_mm2 += float(carried[i])
        trunk.z_top = max(trunk.z_top, jz)
        assignment[i] = chosen
    return trunks, assignment


def consolidate(trunks, assignment, stub_ends, tip_r, surface_tree, cfg):
    """Merge trunks closer than cfg.trunk_merge_radius_mm in xy (second
    review: parallel twin/triplet columns mm apart -- resin cost, no
    stiffness). Shoots of the dropped trunk move to the survivor only if
    every moved shoot's new diagonal clears; otherwise the pair stays.
    Returns (trunks, assignment) with indices rewritten."""
    keep = list(range(len(trunks)))
    merged_into = {}
    for a in range(len(trunks)):
        if a in merged_into:
            continue
        for b in range(a + 1, len(trunks)):
            if b in merged_into:
                continue
            ta, tb = trunks[a], trunks[b]
            if np.linalg.norm(ta.xy - tb.xy) > cfg.trunk_merge_radius_mm:
                continue
            # survivor = the one with more shoots (less re-checking)
            surv, gone = (a, b) if len(ta.shoots) >= len(tb.shoots) else (b, a)
            sv, gn = trunks[surv], trunks[gone]
            if len(sv.shoots) + len(gn.shoots) > cfg.trunk_max_shoots:
                continue
            moves = []
            ok = True
            for i in gn.shoots:
                se = stub_ends[i]
                jz, dxy = _junction_z(se, sv.xy, cfg)
                r_shoot = max(float(tip_r[i]), cfg.min_shoot_r_mm)
                jp = np.array([sv.xy[0], sv.xy[1], jz])
                if not _segment_clear(surface_tree, se, jp, r_shoot, cfg,
                                       skip_from_p0=cfg.standoff_mm):
                    ok = False
                    break
                moves.append((i, jz))
            if not ok:
                continue
            for i, jz in moves:
                sv.shoots.append(i)
                sv.z_top = max(sv.z_top, jz)
                assignment[i] = (surv, jz, assignment[i][2])
            sv.carried_mm2 += gn.carried_mm2
            gn.shoots = []          # dead trunk must not accumulate more
            merged_into[gone] = surv
            if gone == a:
                break               # a is no longer a valid merge target
    if merged_into:
        keep = [i for i in range(len(trunks)) if i not in merged_into]
        remap = {old: new for new, old in enumerate(keep)}
        trunks = [trunks[i] for i in keep]
        assignment = [(remap[t], jz, c) for (t, jz, c) in assignment]
    return trunks, assignment


def model_strut(contact, stub_end, r_shoot, ground_points, surface_tree, cfg):
    """Fallback for contacts whose trunk column can't clear: a strut
    rooted ON the model below (adsorbed foot, like the corpus's own
    model-adsorbed supports). Rooted on already-printed model rather
    than dangling from the raft, so a steeper lean is allowed
    (cfg.strut_fallback_max_angle_deg). Returns anchor point or None."""
    se = np.asarray(stub_end, float)
    d = ground_points - se
    dxy = np.linalg.norm(d[:, :2], axis=1)
    drop = -d[:, 2]  # positive when anchor is below the stub end
    with np.errstate(divide="ignore", invalid="ignore"):
        lean = np.degrees(np.arctan2(dxy, drop))
    L = np.linalg.norm(d, axis=1)
    # fallback struts may reach further than the economy cap on
    # optional struts -- the alternative is a through-model column
    max_len = cfg.strut_max_length_mm * 1.6
    mask = (drop > 1.0) & (L >= 3.0) & (L <= max_len) \
        & (lean <= cfg.strut_fallback_max_angle_deg)
    if not mask.any():
        return None
    for idx in np.nonzero(mask)[0][np.argsort(L[mask])][:40]:
        anchor = ground_points[idx]
        if _segment_clear(surface_tree, se, anchor, r_shoot, cfg,
                           skip_from_p0=cfg.standoff_mm):
            # near-anchor closeness is intentional (adsorbed foot) --
            # _segment_clear's uniform sweep would flag it, so re-check
            # with the last bit excluded and accept if only the final
            # approach is close.
            return anchor
        L_seg = float(np.linalg.norm(anchor - se))
        if L_seg > 2.0 and _segment_clear(
                surface_tree, se, se + (anchor - se) * (1 - 2.0 / L_seg),
                r_shoot, cfg, skip_from_p0=cfg.standoff_mm):
            return anchor
    return None


def transient_worst_span(restraints, plate_z, z_top):
    """Worst free-cantilever length this trunk EVER has during the build.

    Key rule (fourth review, Craig): at any layer, structure ABOVE the
    current build height does not exist yet. A restraint is therefore a
    pair (point_z, active_z): where it grips the trunk, and the build
    height at which it actually starts existing. A brace to a neighbor
    trunk activates the moment its own layer prints (both columns
    already exist there): active_z == point_z. A shoot junction only
    restrains the trunk once the shoot has GROWN all the way to its
    model contact, so its active_z is the contact's stub height -- until
    then the trunk above the junction is still free (and carrying the
    half-built shoot).

    Simulates the build sweep and returns the max of
    (build_z - highest_active_restraint_point)."""
    events = sorted(restraints, key=lambda t: t[1])
    worst = 0.0
    highest_active = plate_z
    for point_z, active_z in events:
        a = min(max(active_z, plate_z), z_top)
        worst = max(worst, a - highest_active)
        highest_active = max(highest_active, min(point_z, z_top))
    worst = max(worst, z_top - highest_active)
    return worst


def resize_for_spans(trunks, braces, junctions_by_trunk, plate_z, cfg):
    """Second-review reconciliation: after braces are actually PLACED,
    re-size each trunk for its real worst build-time span, with no
    add-braces-instead cap (the debt has to be paid somewhere: if no
    neighbor was in reach to brace against, the trunk gets thicker,
    clamped only by the removal-friendly max)."""
    from . import placement
    attach = {i: [] for i in range(len(trunks))}
    for b in braces:
        # a brace exists as soon as its layer prints: active == point
        if b["i"] in attach:
            z = float(b["p0"][2])
            attach[b["i"]].append((z, z))
        if b["j"] in attach:
            z = float(b["p1"][2])
            attach[b["j"]].append((z, z))
    for i, trunk in enumerate(trunks):
        # junctions arrive as (point_z, active_z): the shoot restrains
        # the trunk only once it has grown to its model contact
        zs = attach[i] + [tuple(j) if np.ndim(j) else (float(j), float(j))
                          for j in junctions_by_trunk.get(i, [])]
        span = transient_worst_span(zs, trunk.base_z(plate_z), trunk.z_top)
        scale = max(len(trunk.shoots) + trunk.extra_shoots, 1) ** 0.125
        r_span = placement._shaft_r_for_L(span, cfg) * scale
        # AUTHORITATIVE: axial floor + actual-span slenderness only. Do
        # NOT max() with the pre-brace estimate -- a well-braced trunk
        # must be allowed to come back down to slender (a slender column
        # clears the model where a span-fattened one clips it).
        trunk.r = float(np.clip(max(trunk.r_axial, r_span),
                                 cfg.min_trunk_r_mm, cfg.max_trunk_r_mm))
        trunk.eff_len = span
    return trunks


def trunk_column_ok(trunk, plate_z, surface_tree, cfg):
    """Final-radius column check, INCLUDING the dome ball on top --
    placement-time checks ran at min_trunk_r, but resize_for_spans can
    fatten a trunk well past that (third review: the fattened columns
    were eating the check's whole margin and shaving through the
    model). Branch trunks are checked from their Y-split upward."""
    ok = _column_clear(surface_tree, trunk.xy, trunk.base_z(plate_z),
                        trunk.z_top + trunk.r, trunk.r, cfg)
    if ok and trunk.foot_kind == "branch" and trunk.z_split is not None:
        # also the Y diagonal itself (its far end sits on the parent)
        p1 = np.array([trunk.xy[0], trunk.xy[1], trunk.z_start])
        # p0 is filled in by the engine via parent lookup; here we only
        # re-check the column. Diagonal was checked at merge time.
        pass
    return ok


def coalesce_columns(trunks, junctions_by_trunk, plate_z, surface_tree, cfg):
    """v3.5 (sixth review): near-parallel plate columns whose feet land
    close together merge into a tree -- the child keeps only its upper
    run, sprouting from the parent at a 45-degree Y-split; its lower
    column and pad are deleted. Build-order-safe: the Y root sits on the
    parent (exists), the diagonal climbs at 45 degrees, the child column
    continues from its end -- every layer supported. Each merge passes a
    resin gate (saved child column vs added diagonal at child radius).

    Single-level trees only (a parent cannot itself be a branch).
    Mutates trunks in place; returns list of merge dicts for the log."""
    merges = []
    order = sorted(range(len(trunks)), key=lambda t: trunks[t].z_top)
    for ci in order:
        child = trunks[ci]
        if child.foot_kind != "plate" or not child.shoots:
            continue
        lowest_j = min((jz for jz, _az in junctions_by_trunk.get(ci, [(child.z_top, 0)])),
                        default=child.z_top)
        cand = sorted((pi for pi in range(len(trunks)) if pi != ci
                        and trunks[pi].foot_kind == "plate"
                        and trunks[pi].parent_idx is None),
                       key=lambda pi: np.linalg.norm(trunks[pi].xy - child.xy))
        for pi in cand:
            parent = trunks[pi]
            if parent.foot_kind != "plate":
                continue
            d = float(np.linalg.norm(parent.xy - child.xy))
            if not (1.0 <= d <= cfg.trunk_branch_merge_xy_mm):
                continue
            z_split = min(parent.z_top, lowest_j - d) - 0.5
            if z_split <= plate_z + cfg.flare_len_mm + 2.0:
                continue
            z_start = z_split + d
            saved_len = (z_start - plate_z) - d * np.sqrt(2.0)
            if saved_len <= cfg.flare_len_mm:
                continue  # not worth a scar-free but pointless merge
            p_split = np.array([parent.xy[0], parent.xy[1], z_split])
            p_start = np.array([child.xy[0], child.xy[1], z_start])
            if not _segment_clear(surface_tree, p_split, p_start,
                                   max(child.r, cfg.min_trunk_r_mm), cfg):
                continue
            child.foot_kind = "branch"
            child.parent_idx = pi
            child.z_split = float(z_split)
            child.z_start = float(z_start)
            parent.carried_mm2 += child.carried_mm2
            parent.extra_shoots += len(child.shoots) + child.extra_shoots
            merges.append(dict(child=ci, parent=pi, z_split=float(z_split),
                                saved_mm3=float(np.pi * max(child.r, cfg.min_trunk_r_mm) ** 2
                                                 * saved_len)))
            break
    return merges


def bundle_shoots(routes, trunks, tip_r, plate_z, surface_tree, cfg):
    """v3.5b (sixth review, the green-pair case): contacts CLOSE ON THE
    MODEL whose shoots run long and parallel get a collector hub -- short
    shoots from each stub to the hub, then ONE shared riser from the hub
    down to the trunk junction, instead of N parallel diagonals.

    The riser grows from the trunk (exists) at <=45deg; the hub and the
    short fan-out shoots print on top of it. Riser is sized for the
    combined load and its own transient cantilever. Per-cluster resin
    gate. Mutates routes' shoot segments; returns riser member dicts
    {p0, p1, r, trunk_idx} for emission/verify."""
    from . import placement
    risers = []
    by_trunk = {}
    for ri, rt in enumerate(routes):
        if rt.anchor_kind != "trunk" or rt.trunk_idx is None:
            continue
        shoot = next((s for s in rt.segments if s.kind == "shoot"), None)
        if shoot is None:
            continue
        L = float(np.linalg.norm(shoot.p1 - shoot.p0))
        if L < 8.0:
            continue  # bundling only pays on long runs
        by_trunk.setdefault(rt.trunk_idx, []).append((ri, shoot, L))

    for t_idx, members in by_trunk.items():
        if len(members) < 2:
            continue
        trunk = trunks[t_idx]
        used = set()
        for a in range(len(members)):
            if a in used:
                continue
            ria, sa, La = members[a]
            cluster = [(ria, sa, La)]
            for b in range(a + 1, len(members)):
                if b in used:
                    continue
                rib, sb, Lb = members[b]
                if np.linalg.norm(sa.p0 - sb.p0) <= 7.0:  # stubs close on model
                    cluster.append((rib, sb, Lb))
                    used.add(b)
            if len(cluster) < 2:
                continue
            used.add(a)
            stubs = np.array([s.p0 for _, s, _ in cluster])
            hub_xy = stubs[:, :2].mean(axis=0)
            reach = max(np.linalg.norm(stubs[:, :2] - hub_xy, axis=1).max(), 0.5)
            hub_z = stubs[:, 2].min() - reach  # every fan shoot <=45deg
            hub = np.array([hub_xy[0], hub_xy[1], hub_z])
            # riser hub -> trunk at <=45deg. A hub nearly ABOVE the trunk
            # would make the riser coincide with the trunk column itself
            # (seventh review: redundant parallel verticals) -- skip.
            d_tr = float(np.linalg.norm(trunk.xy - hub_xy))
            if d_tr < 2.0:
                continue
            jz = hub_z - d_tr
            if jz <= trunk.base_z(plate_z) + 1.0 or jz > trunk.z_top + 1e-6:
                continue
            jp = np.array([trunk.xy[0], trunk.xy[1], jz])
            L_riser = float(np.linalg.norm(jp - hub))
            r_members = [s.r0 for _, s, _ in cluster]
            r_riser = max(max(r_members),
                           placement._shaft_r_for_L(L_riser, cfg)
                           * len(cluster) ** 0.125)
            # clearance: riser, and each short fan shoot
            if not _segment_clear(surface_tree, hub, jp, r_riser, cfg):
                continue
            fans_ok = all(_segment_clear(surface_tree, s.p0, hub, s.r0, cfg,
                                          skip_from_p0=cfg.standoff_mm)
                          for _, s, _ in cluster)
            if not fans_ok:
                continue
            # resin gate: old parallel diagonals vs fan + shared riser
            vol_old = sum(np.pi * s.r0 ** 2 * L for _, s, L in cluster)
            vol_new = (sum(np.pi * s.r0 ** 2 * float(np.linalg.norm(s.p0 - hub))
                            for _, s, _ in cluster)
                        + np.pi * r_riser ** 2 * L_riser)
            if vol_new >= vol_old:
                continue
            for ri2, s, _ in cluster:
                s.p1 = hub.copy()  # short fan shoot to the collector
                # hub ball must match the FATTEST member meeting there
                # (seventh review: sizing it by the distant trunk left a
                # ball smaller than the riser rod -- a gapped joint)
                routes[ri2].hub_ball_r = float(max(r_riser, s.r0))
            risers.append(dict(p0=hub, p1=jp, r=float(r_riser), trunk_idx=t_idx,
                                saved_mm3=float(vol_old - vol_new)))
    # bundled trunks: junctions moved to the riser attach point, so the
    # column must not keep running to the OLD junction heights (seventh
    # review: dead domed verticals above the riser). Recompute z_top from
    # what actually attaches now.
    affected = {rs["trunk_idx"] for rs in risers}
    for t_idx in affected:
        trunk = trunks[t_idx]
        tops = [rs["p1"][2] for rs in risers if rs["trunk_idx"] == t_idx]
        for rt in routes:
            if getattr(rt, "trunk_idx", None) != t_idx:
                continue
            for sg in rt.segments:
                if sg.kind == "shoot" and np.linalg.norm(sg.p1[:2] - trunk.xy) < 0.5:
                    tops.append(float(sg.p1[2]))
        if tops:
            trunk.z_top = max(tops)
    return risers


def relocate_trunk(trunk, stub_ends, tip_r, plate_z, surface_tree, cfg):
    """Re-run the spiral search for a trunk AT ITS FINAL RADIUS, centered
    on the mean of its shoots' stub xy. Returns (xy, {contact: jz}) for
    the first position where the column AND every shoot clear, or None."""
    center = np.mean([stub_ends[i][:2] for i in trunk.shoots], axis=0)
    found_max = max(cfg.trunk_search_radii_mm)
    for radius in cfg.trunk_search_radii_mm:
        dirs = 1 if radius == 0 else cfg.trunk_search_directions
        for k in range(dirs):
            ang = 2 * np.pi * k / dirs
            xy = center + radius * np.array([np.cos(ang), np.sin(ang)])
            juncs = {}
            ok = True
            for i in trunk.shoots:
                se = stub_ends[i]
                jz, dxy = _junction_z(se, xy, cfg)
                if dxy > found_max or jz <= plate_z + 1.0:
                    ok = False
                    break
                jp = np.array([xy[0], xy[1], jz])
                r_shoot = max(float(tip_r[i]), cfg.min_shoot_r_mm)
                if not _segment_clear(surface_tree, se, jp, r_shoot, cfg,
                                       skip_from_p0=cfg.standoff_mm):
                    ok = False
                    break
                juncs[i] = jz
            if not ok:
                continue
            z_top = max(juncs.values())
            if _column_clear(surface_tree, xy, plate_z, z_top + trunk.r,
                              trunk.r, cfg):
                return xy, juncs
    return None


def size_trunks(trunks, plate_z, cfg):
    """Physics sizing per trunk: peel-tension radius over ACCUMULATED
    carried area (same fuse formula as tips, without the tip's removal
    clamp) combined with slenderness sizing over the trunk's height with
    per-shoot drag load; clamped to the removal-friendly trunk band.
    Fills trunk.r, trunk.n_braces, trunk.eff_len in place."""
    from . import placement
    for trunk in trunks:
        area_m2 = trunk.carried_mm2 * 1e-6
        force_n = area_m2 * cfg.peel_pressure_pa
        tensile_pa = cfg.tensile_green_mpa * 1e6
        r_axial = np.sqrt(force_n * cfg.tip_safety_factor
                           / (np.pi * tensile_pa)) * 1000.0
        L = trunk.z_top - trunk.base_z(plate_z)
        # lateral drag scales with how much structure hangs off this
        # artery: one shoot's worth of drag per attached shoot.
        scale = max(len(trunk.shoots) + trunk.extra_shoots, 1) ** 0.5
        r_defl, n_braces, eff_len = placement.shaft_radius_for_length_mm(L, cfg)
        r_defl = r_defl * scale ** 0.25  # F^(1/4) in the deflection solve
        trunk.r_axial = float(min(r_axial, cfg.max_trunk_r_mm))
        trunk.r = float(np.clip(max(r_axial, r_defl),
                                 cfg.min_trunk_r_mm, cfg.max_trunk_r_mm))
        trunk.n_braces = n_braces
        trunk.eff_len = eff_len
    return trunks


def shoot_radius(tip_r_i, shoot_len, carried_i, cfg):
    """A shoot's worst moment is DURING the build: it grows outward from
    its trunk junction as a free cantilever of its full length until the
    final layers connect it to the model, so it gets the full deflection
    sizing for that length -- no root-stiffness discount. (A 0.7 fudge
    here previously put every length-sized shoot at exactly (1/0.7)^4 =
    4.2x the deflection budget; the fourth review's audit caught it.)"""
    from . import placement
    r_defl = placement._shaft_r_for_L(shoot_len, cfg)
    return float(max(tip_r_i, r_defl, cfg.min_shoot_r_mm))
