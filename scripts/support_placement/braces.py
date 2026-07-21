"""Brace graph construction (design.md #5).

"Brace graph (neighbor shafts within cross-width; ladder spacing from
config; slenderness-triggered)." A shaft that needs bracing (per
placement.shaft_radius_for_length_mm's n_braces > 0) gets horizontal
braces to its nearest qualifying neighbor(s) at cfg.brace_ladder_z_mm
intervals, capped at the number of braces the slenderness calc actually
asked for.
"""
import numpy as np
import networkx as nx


def _shaft_column(route):
    """(xy, z0, z1) for a route's vertical descent segment, or None if it
    has no shaft segment (e.g. contact anchors directly at the plate with
    zero shaft length)."""
    for seg in route.segments:
        if seg.kind == "shaft":
            xy = (seg.p0[:2] + seg.p1[:2]) / 2.0
            z0, z1 = sorted([seg.p0[2], seg.p1[2]])
            return xy, z0, z1
    return None


def build_trunk_braces(trunks, plate_z, cfg):
    """v3 bracing between arterial trunks: DIAGONAL X pairs, never
    horizontal (in-Chitubox review: floating horizontal rods print as
    unsupported bridge islands; every brace member obeys the same
    <=45-degrees-from-vertical rule as everything else). Each rung of the
    ladder between two trunks is a crossed pair of rods whose vertical
    rise equals their lateral span (45 degrees).

    Returns (braces, graph): braces are {i, j, p0, p1, r} dicts in TRUNK
    indices."""
    # Height-driven requirement (second review): during the build a trunk
    # is a raft-rooted cantilever until restrained -- any trunk taller
    # than brace_free_span_mm needs bracing at that cadence, whatever the
    # static slenderness calc said. Trunks that can't get braced are
    # thickened afterwards by trunks.resize_for_spans (the debt is paid
    # either way).
    span_need = {i: int((t.z_top - plate_z) / cfg.brace_free_span_mm)
                 for i, t in enumerate(trunks)}
    pairs = []
    for i in range(len(trunks)):
        for j in range(i + 1, len(trunks)):
            if max(trunks[i].n_braces, span_need[i]) == 0 \
                    and max(trunks[j].n_braces, span_need[j]) == 0:
                continue
            d = float(np.linalg.norm(trunks[i].xy - trunks[j].xy))
            if d > cfg.brace_neighbor_max_xy_mm or d < 1e-6:
                continue
            zlo = max(trunks[i].base_z(plate_z),
                       trunks[j].base_z(plate_z)) + cfg.flare_len_mm + 1.0
            zhi = min(trunks[i].z_top, trunks[j].z_top) - 0.5
            rise = d  # 45-degree diagonal needs this much vertical room
            if zhi - zlo < rise:
                continue
            pairs.append((d, i, j, zlo, zhi, rise))
    pairs.sort(key=lambda t: t[0])

    needs = {i: max(t.n_braces, span_need[i]) for i, t in enumerate(trunks)}
    given = {i: 0 for i in range(len(trunks))}
    graph = nx.Graph()
    braces = []
    for d, i, j, zlo, zhi, rise in pairs:
        if given[i] >= needs[i] and given[j] >= needs[j]:
            continue
        span = zhi - zlo
        n_rungs = max(1, int(span / cfg.brace_ladder_z_mm))
        n_rungs = min(n_rungs, max(needs[i], needs[j]))
        # A brace rod is itself a transient cantilever while it grows
        # diagonally from one trunk to the other (the far end doesn't
        # exist until its layers print) -- size it for its own full rod
        # length; the crossed pair shares the final lateral load, hence
        # the (1/2)^(1/4) on the pair-load term.
        from . import placement
        rod_len = rise * np.sqrt(2.0)
        r_rod = max(cfg.brace_r_mm,
                     placement._shaft_r_for_L(
                         rod_len, cfg,
                         angle_from_vertical_deg=cfg.max_segment_angle_deg)
                     * 0.5 ** 0.25)
        placed = 0
        for k in range(n_rungs):
            if given[i] >= needs[i] and given[j] >= needs[j]:
                break
            z = zlo + (k + 0.5) * (span - rise) / max(n_rungs, 1)
            xi, xj = trunks[i].xy, trunks[j].xy
            # crossed pair: up-right and up-left
            braces.append(dict(i=i, j=j, r=r_rod,
                                p0=np.array([xi[0], xi[1], z]),
                                p1=np.array([xj[0], xj[1], z + rise])))
            braces.append(dict(i=i, j=j, r=r_rod,
                                p0=np.array([xj[0], xj[1], z]),
                                p1=np.array([xi[0], xi[1], z + rise])))
            given[i] += 1
            given[j] += 1
            placed += 1
        if placed:
            graph.add_edge(i, j, n=placed)
    return braces, graph


def build_model_braces(trunks, placed_braces, junctions_by_trunk, ground_points,
                        surface_tree, plate_z, cfg):
    """Rungs from a trunk to nearby MODEL surface where no trunk neighbor
    was reachable (third review: without this, isolated canopy trunks
    either stayed wobbly-slender or fattened until they clipped the
    model). Both ends sit on structure already printed at that layer, so
    these are short two-end-supported bridges (like the raft bridges),
    capped at cfg.model_brace_max_len_mm. Returns brace dicts with
    j=-1 (model side)."""
    from . import trunks as trunks_mod
    attach = {i: [] for i in range(len(trunks))}
    for b in placed_braces:
        if b["i"] in attach:
            attach[b["i"]].append(float(b["p0"][2]))
        if b.get("j") in attach:
            attach[b["j"]].append(float(b["p1"][2]))
    out = []
    for ti, t in enumerate(trunks):
        junc_pts = [float(j[0]) if np.ndim(j) else float(j)
                    for j in junctions_by_trunk.get(ti, [])]
        zs = sorted([t.base_z(plate_z)] + attach[ti] + junc_pts + [t.z_top])
        for a, b in zip(zs[:-1], zs[1:]):
            gap = b - a
            if gap <= cfg.brace_free_span_mm:
                continue
            n = int(gap / cfg.brace_free_span_mm)
            for k in range(1, n + 1):
                z = a + k * gap / (n + 1)
                p_t = np.array([t.xy[0], t.xy[1], z])
                d = ground_points - p_t
                L = np.linalg.norm(d, axis=1)
                mask = (L >= 1.5) & (L <= cfg.model_brace_max_len_mm) \
                    & (d[:, 2] <= 1.0)  # model point at or below the rung
                if not mask.any():
                    continue
                for idx in np.nonzero(mask)[0][np.argsort(L[mask])][:6]:
                    anchor = ground_points[idx]
                    Lseg = float(np.linalg.norm(anchor - p_t))
                    # clear along the rod, excluding the deliberate
                    # model-end run-in
                    probe = p_t + (anchor - p_t) * max(0.0, 1 - 1.2 / Lseg)
                    if trunks_mod._segment_clear(surface_tree, p_t, probe,
                                                  cfg.brace_r_mm, cfg):
                        out.append(dict(i=ti, j=-1, r=cfg.brace_r_mm,
                                         p0=p_t, p1=anchor))
                        break
    return out


def brace_and_thin_shoots(routes, trunks, plate_z, ground_points, surface_tree, cfg):
    """v3.4 (Craig): cross-brace long diagonals so they can be THINNER.

    Deflection goes as L^3/r^4: halving a shoot's free span lets its
    radius drop ~40% and its resin cross-section ~65%, far more than the
    rung costs. A shoot grows from its junction (the low end, which
    prints first) as a free cantilever; a rung from a point on the shoot
    to a trunk, to grounded model below, or to a SIBLING shoot is
    two-end-supported the moment its layer prints (everything it touches
    exists at that z), so it restrains immediately under the transient
    rules. Bracing then thinning is monotone-safe for clearance: a
    thinner member always clears where its fat version cleared, so
    members are only ever shrunk here, never grown.

    Mutates shoot/stub segment radii in place; returns the rung braces
    (i=j=-3 markers, geometry absolute)."""
    from . import placement, trunks as trunks_mod
    out = []
    # sibling-shoot targets: every shoot segment with its z range
    all_shoots = []
    for ri, rt in enumerate(routes):
        for seg in rt.segments:
            if seg.kind == "shoot":
                all_shoots.append((ri, seg))

    def _shoot_point_at_z(seg, z):
        z0, z1 = seg.p1[2], seg.p0[2]  # p1 = root (low), p0 = stub end
        if not (min(z0, z1) + 0.5 <= z <= max(z0, z1) - 0.5) or abs(z1 - z0) < 1e-6:
            return None
        t = (z - z0) / (z1 - z0)
        return seg.p1 + t * (seg.p0 - seg.p1)

    for ri, rt in enumerate(routes):
        shoot = next((s for s in rt.segments if s.kind == "shoot"), None)
        if shoot is None:
            continue
        stub = rt.segments[0]
        root, tip_end = shoot.p1, shoot.p0  # build grows root -> tip_end
        L = float(np.linalg.norm(tip_end - root))
        d_seg = tip_end - root
        lean_deg = float(np.degrees(np.arctan2(
            np.linalg.norm(d_seg[:2]), abs(d_seg[2])))) if L > 1e-9 else 0.0
        r_target = max(stub.r0, cfg.min_shoot_r_mm)
        span_ok = placement.max_unbraced_len_mm(
            r_target, cfg, angle_from_vertical_deg=lean_deg)
        if L <= span_ok or L < 4.0:
            continue
        dirv = (tip_end - root) / L
        n_rungs = int(L / span_ok)
        restraints = [0.0]
        member_rungs = []
        for k in range(1, n_rungs + 1):
            s = k * L / (n_rungs + 1)
            p = root + dirv * s
            z = p[2]
            best = None
            # (a) trunk columns existing at this z
            for t in trunks:
                if not (plate_z + cfg.flare_len_mm <= z <= t.z_top):
                    continue
                q = np.array([t.xy[0], t.xy[1], z])
                d = float(np.linalg.norm(q - p))
                if 1.0 <= d <= cfg.model_brace_max_len_mm and (best is None or d < best[0]):
                    best = (d, q, "trunk")
            # (b) sibling shoots crossing this z
            for rj, seg in all_shoots:
                if rj == ri:
                    continue
                q = _shoot_point_at_z(seg, z)
                if q is None:
                    continue
                d = float(np.linalg.norm(q - p))
                if 1.0 <= d <= cfg.model_brace_max_len_mm and (best is None or d < best[0]):
                    best = (d, q, "shoot")
            # (c) grounded model below (scar, so last preference)
            if best is None and len(ground_points):
                dd = ground_points - p
                Ld = np.linalg.norm(dd, axis=1)
                mask = (Ld >= 1.5) & (Ld <= cfg.model_brace_max_len_mm) & (dd[:, 2] <= 1.0)
                for idx in np.nonzero(mask)[0][np.argsort(Ld[mask])][:4]:
                    best = (float(Ld[idx]), ground_points[idx], "model")
                    break
            if best is None:
                continue
            d, q, kind = best
            probe = q if kind != "model" else p + (q - p) * max(0.0, 1 - 1.2 / d)
            if not trunks_mod._segment_clear(surface_tree, p, probe,
                                              cfg.brace_r_mm, cfg):
                continue
            member_rungs.append(dict(i=-3, j=-3, r=cfg.brace_r_mm, p0=p, p1=q))
            restraints.append(s)
        # worst remaining free span along the member (root..rungs..tip;
        # the tip contact only exists at the very end, so the final gap
        # counts in full)
        ss = sorted(restraints) + [L]
        worst = max(b - a for a, b in zip(ss[:-1], ss[1:]))
        r_new = max(stub.r0,
                     placement._shaft_r_for_L(
                         worst, cfg, angle_from_vertical_deg=lean_deg),
                     cfg.min_shoot_r_mm)
        # ECONOMIC gate (Craig): these members already met the deflection
        # budget at full-length sizing, so rungs here are pure resin
        # arbitrage -- keep the set only if the thinned member + rung
        # material beats the fat unbraced member.
        vol_before = np.pi * shoot.r0 ** 2 * L
        vol_rungs = sum(np.pi * b["r"] ** 2
                         * float(np.linalg.norm(np.asarray(b["p1"]) - np.asarray(b["p0"])))
                         for b in member_rungs)
        vol_after = np.pi * r_new ** 2 * L + vol_rungs
        if member_rungs and r_new < shoot.r0 - 1e-9 and vol_after < vol_before:
            shoot.r0 = shoot.r1 = float(r_new)
            stub.r1 = float(r_new)
            out.extend(member_rungs)
    return out


def build_braces(routes, cfg):
    """Returns list of dicts: {i, j, p0, p1, r} -- brace cylinders between
    route i and route j's shaft columns. Greedy nearest-neighbor-first so
    scarce brace budget goes to the closest, most structurally useful
    connections first.
    """
    columns = {i: _shaft_column(r) for i, r in enumerate(routes)}
    needs = {i: r.n_braces for i, r in enumerate(routes)}
    given = {i: 0 for i in range(len(routes))}

    pairs = []
    ids = [i for i, c in columns.items() if c is not None]
    for a_pos, i in enumerate(ids):
        xy_i, z0_i, z1_i = columns[i]
        for j in ids[a_pos + 1:]:
            xy_j, z0_j, z1_j = columns[j]
            if needs[i] == 0 and needs[j] == 0:
                continue
            d = float(np.linalg.norm(xy_i - xy_j))
            if d > cfg.brace_neighbor_max_xy_mm:
                continue
            overlap = min(z1_i, z1_j) - max(z0_i, z0_j)
            if overlap <= 0:
                continue
            pairs.append((d, i, j, max(z0_i, z0_j), min(z1_i, z1_j)))
    pairs.sort(key=lambda t: t[0])

    graph = nx.Graph()
    braces = []
    for d, i, j, zlo, zhi in pairs:
        target = max(needs[i], needs[j])
        if target == 0:
            continue
        n_rungs = max(1, int((zhi - zlo) / cfg.brace_ladder_z_mm))
        n_rungs = min(n_rungs, target)
        placed_here = 0
        for k in range(n_rungs):
            if given[i] >= needs[i] and given[j] >= needs[j]:
                break
            z = zlo + (k + 0.5) * (zhi - zlo) / n_rungs
            xy_i, *_ = columns[i]
            xy_j, *_ = columns[j]
            p0 = np.array([xy_i[0], xy_i[1], z])
            p1 = np.array([xy_j[0], xy_j[1], z])
            braces.append(dict(i=i, j=j, p0=p0, p1=p1, r=cfg.brace_r_mm))
            given[i] += 1
            given[j] += 1
            placed_here += 1
        if placed_here:
            graph.add_edge(i, j, n=placed_here)
    return braces, graph
