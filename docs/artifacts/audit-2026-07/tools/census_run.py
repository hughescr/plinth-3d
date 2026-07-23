"""Exact-geometry penetration census over every live member + ball.

Members: Moller-Trumbore gives exact surface-crossing LOCATIONS that partition
each ray (axis + 8 capsule-offset lines) into sub-segments; each sub-segment is
classified inside/outside by MAJORITY of dense ParityVote samples (robust to
thin-feature flicker). Depth via exact point-to-triangle distance.

Balls: antipodal-ray span test -- a ball whose sphere is pierced by the model on
two opposite sides (local feature thinner than its diameter) pokes out both
sides = the screenshot's breaching joint balls. Pure Moller, no point-inside.
"""
import numpy as np, sys, os, json, time
from collections import Counter
np.seterr(all="ignore")
sys.path.insert(0, "/Users/craig/.claude/jobs/0ddf6d41/tmp")
import census_core as C

AUDIT = os.path.join(C.SCR, "audit")
MIN_ILLEGAL_LEN = 0.05
CROWN_Z = 35.0
STEP = 0.12          # dense sample step (mm)
CAP = 80             # max samples per sub-segment
BALL_EPS = 0.05      # mm poke-out to count a ball breach

def own(o):
    return list(o) if isinstance(o, (list, tuple)) else o

t0 = time.time()
S, mesh = C.load_all()
members = S["members"]; balls = S["balls"]; cfg = S["cfg"]
CD = cfg.contact_depth_mm; STAND = cfg.standoff_mm
MI = C.MeshIndex(mesh)
import routing
stree = routing.build_surface_tree(mesh)
PB = float(routing.surface_tree_pitch_bound_mm(stree, percentile=cfg.surface_pitch_bound_percentile)); del stree
dist_tool = C.SignedTester(mesh, MI, k=64, flip=False)
PV = C.ParityVote(mesh, h=2.0)
print("pitch_bound=%.3f ready RSS=%.2fGB" % (PB, C.rss_gb()), flush=True)

# ---------------- PASS A: crossings + dense per-subsegment samples ----------------
rec = []; pts_list = []
for mi, m in enumerate(members):
    p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
    r0 = float(m["r0"]); r1 = float(m["r1"]); rmax = max(r0, r1)
    kind = m["kind"]; embed = m.get("embed_end")
    L = float(np.linalg.norm(p1 - p0))
    if L < 1e-9:
        continue
    center = (p0 + p1) / 2.0
    R = 0.5 * L + rmax + MI.tri_extent + 0.6
    idx = MI.candidates(center, R)
    rays = C.member_rays(p0, p1, r0, r1) if kind != "stub" else [("axis", p0.copy(), (p1 - p0), L)]
    for label, O, D, seg in rays:
        if seg < 1e-9:
            continue
        ts = C.dedup_loc(C.ray_crossings(O, D, idx, MI), seg)
        b = np.concatenate([[0.0], ts, [1.0]])
        subs = []   # [t_a, t_b, start_surf, end_surf, sample_pts]
        for i in range(len(b) - 1):
            ta, tb = b[i], b[i + 1]
            Lsub = (tb - ta) * seg
            ns = min(CAP, max(3, int(Lsub / STEP)))
            tt = np.linspace(ta, tb, ns + 2)[1:-1]
            spts = O[None, :] + tt[:, None] * D[None, :]
            subs.append([ta, tb, i != 0, (i + 1) != (len(b) - 1), spts])
        rec.append(dict(mi=mi, kind=kind, embed=embed, owner=m.get("owner"),
                        label=label, O=O, D=D, seg=seg, r0=r0, r1=r1, L=L, subs=subs))
# flatten sample points, record slices
allp = []; run = 0
for r in rec:
    for sub in r["subs"]:
        sp = sub[4]; allp.append(sp)
        sub.append((run, run + len(sp)))   # sub[5] = slice
        run += len(sp)
all_pts = np.concatenate(allp) if allp else np.zeros((0, 3))
print("PASS A: %d ray-records, %d samples t=%.1fs RSS=%.2fGB" %
      (len(rec), len(all_pts), time.time() - t0, C.rss_gb()), flush=True)

# ---------------- PASS B: parity inside + distance ----------------
inside_all = np.empty(len(all_pts), bool)
dist_all = np.empty(len(all_pts))
for s in range(0, len(all_pts), 40000):
    e = min(s + 40000, len(all_pts))
    inside_all[s:e] = PV.inside(all_pts[s:e])
    assert C.rss_gb() < 32.0
for s in range(0, len(all_pts), 4000):
    e = min(s + 4000, len(all_pts))
    dist_all[s:e] = np.abs(dist_tool.signed(all_pts[s:e]))
    assert C.rss_gb() < 32.0
print("PASS B: dense inside frac=%.4f t=%.1fs RSS=%.2fGB" %
      (inside_all.mean(), time.time() - t0, C.rss_gb()), flush=True)

# ---------------- PASS C: subseg majority -> intervals -> classify ----------------
crossings = []; per_member = {}
for r in rec:
    subs = r["subs"]
    ins = []; dep = []
    for sub in subs:
        a0, a1 = sub[5]
        si = inside_all[a0:a1]
        ins.append(si.mean() >= 0.5 if len(si) else False)
        dd = dist_all[a0:a1][si] if len(si) else np.array([])
        dep.append(float(dd.max()) if len(dd) else 0.0)
    ins = np.array(ins); n = len(subs)
    # merge consecutive inside subsegs
    ivs = []; i = 0
    while i < n:
        if ins[i]:
            j = i
            while j + 1 < n and ins[j + 1]:
                j += 1
            ta = subs[i][0]; tb = subs[j][1]
            ss = subs[i][2]; es = subs[j][3]
            depth = max(dep[i:j + 1])
            ivs.append((ta, tb, ss, es, depth)); i = j + 1
        else:
            i += 1
    if not ivs:
        continue
    seg = r["seg"]; embed = r["embed"]; kind = r["kind"]
    inset0 = CD + PB + r["r0"] + STAND; inset1 = CD + PB + r["r1"] + STAND
    O = r["O"]; D = r["D"]
    for (ta, tb, ss, es, depth) in ivs:
        aa = ta * seg; bb = tb * seg
        if embed == "p0":
            lo, hi, zone = max(aa, inset0), bb, "p0"
        elif embed == "p1":
            lo, hi, zone = aa, min(bb, seg - inset1), "p1"
        else:
            lo, hi, zone = aa, bb, None
        ill_len = hi - lo
        entry = O + ta * D; exit_ = O + tb * D
        feat = float(np.linalg.norm(exit_ - entry)); through = bool(ss and es)
        loc = (entry + exit_) / 2.0
        agg = per_member.setdefault(r["mi"], dict(
            mi=r["mi"], kind=kind, embed=embed, owner=own(r["owner"]),
            illegal=False, through=False, axis_transit=False,
            max_transit=0.0, max_depth=0.0, min_feat=None, n_iv=0, worst=None, zmax=-1e9))
        if ill_len <= MIN_ILLEGAL_LEN:
            continue
        agg["illegal"] = True; agg["n_iv"] += 1
        if through: agg["through"] = True
        if r["label"] == "axis": agg["axis_transit"] = True
        transit = bb - aa
        agg["max_transit"] = max(agg["max_transit"], transit)
        agg["max_depth"] = max(agg["max_depth"], depth)
        if through:
            agg["min_feat"] = feat if agg["min_feat"] is None else min(agg["min_feat"], feat)
        agg["zmax"] = max(agg["zmax"], float(loc[2]))
        cd = dict(member_idx=r["mi"], kind=kind, owner=own(r["owner"]), embed_end=embed,
                  ray=r["label"], through_and_through=through,
                  entry=[round(float(x), 3) for x in entry], exit=[round(float(x), 3) for x in exit_],
                  location=[round(float(x), 3) for x in loc],
                  transit_len_mm=round(float(transit), 4), illegal_len_mm=round(float(ill_len), 4),
                  feature_thickness_mm=round(float(feat), 4), penetration_depth_mm=round(float(depth), 4),
                  z=round(float(loc[2]), 3), embed_zone=zone, scar_on_removal=through)
        crossings.append(cd)
        if agg["worst"] is None or transit > agg["worst"][0]:
            agg["worst"] = (transit, cd)
print("PASS C: %d illegal crossings, %d illegal members t=%.1fs" %
      (len(crossings), sum(1 for a in per_member.values() if a["illegal"]), time.time() - t0), flush=True)

# ---------------- BALLS: through-thin-feature breach ----------------
# Almost every ball whose sphere reaches the surface is a DESIGNED model-anchor
# foot (strut/rung p1) or contact tip -- a shallow embed into the surface is
# fine when the feature is thick. The DEFECT (screenshot) is a foot/joint ball
# on a feature THINNER than the ball can span: the model's far wall lies within
# the ball radius, so the ball pokes out the far side. Measure the model's local
# thickness along the inward surface normal (+/- a cone); breach if < r.
def fib_sphere(n=120):
    ga = np.pi * (3 - 5 ** 0.5); out = []
    for i in range(n):
        z = 1 - (i + 0.5) * 2.0 / n; rr = (1 - z * z) ** 0.5; th = ga * i
        out.append([rr * np.cos(th), rr * np.sin(th), z])
    return np.array(out)
SPH = fib_sphere(120)
bc = np.array([b["center"] for b in balls]); br = np.array([b["r"] for b in balls])
nd, bti, bcp, bnorm = dist_tool.nearest(bc)
nd = np.abs(nd)
b_center_in = PV.inside(bc)

def cone_dirs(axis, half_deg=25, n=8):
    axis = axis / (np.linalg.norm(axis) or 1.0)
    e1 = C.perp_basis(axis)[0]; e2 = np.cross(axis, e1)
    th = np.radians(half_deg); dirs = [axis]
    for k in range(n):
        ph = 2 * np.pi * k / n
        d = np.cos(th) * axis + np.sin(th) * (np.cos(ph) * e1 + np.sin(ph) * e2)
        dirs.append(d / np.linalg.norm(d))
    return dirs

ball_records = []
n_touch = 0
for i, b in enumerate(balls):
    r = float(br[i])
    if r - nd[i] <= BALL_EPS:
        continue    # sphere does not cross the model surface
    n_touch += 1
    c = bc[i]
    inward = -bnorm[i]        # into the model from the near wall
    idx = MI.candidates(c, r + MI.tri_extent + 0.1)
    if len(idx) == 0:
        continue
    # thickness = nearest far-wall crossing along inward-normal cone (skip near wall)
    thick = np.inf
    for d in cone_dirs(inward, 25, 8):
        tt = C.ray_crossings(c, d * r, idx, MI)
        if len(tt):
            dd = tt * r
            dd = dd[dd > nd[i] + 0.05]
            if len(dd):
                thick = min(thick, float(dd.min()))
    if thick == np.inf or thick >= r - BALL_EPS:
        continue    # feature is thicker than the ball -> shallow anchor, sanctioned
    poke = r - thick
    in_frac = float(PV.inside(c[None, :] + r * SPH).mean())
    ball_records.append(dict(ball_idx=i, owner=own(b.get("owner")),
        center=[round(float(x), 3) for x in c], r=round(r, 3),
        feature_thickness_mm=round(float(thick), 4), poke_out_mm=round(float(poke), 4),
        surface_dist_mm=round(float(nd[i]), 4), center_inside=bool(b_center_in[i]),
        buried_fraction=round(in_frac, 3), z=round(float(c[2]), 3)))
print("BALLS: %d breaching / %d (%d spheres reach surface) t=%.1fs" %
      (len(ball_records), len(balls), n_touch, time.time() - t0), flush=True)

# ---------------- CROSS-CHECK worst offenders (heavy 9-ray) ----------------
ill_members = [a for a in per_member.values() if a["illegal"]]
ill_members.sort(key=lambda a: -a["max_transit"])
def heavy_inside(P):
    v = 0
    for d in [[1,0,0],[0,1,0],[0,0,1],[1,1,1],[1,-1,1],[-1,1,1],[1,1,-1],[-1,-1,1],[2,1,.5]]:
        l = mesh.ray.intersects_location([P], [np.asarray(d, float)])[0]
        v += 1 if len(l) % 2 == 1 else 0
    return v >= 5
chk = [a["worst"][1]["location"] for a in ill_members[:30]]
try:
    agree = float(np.mean([bool(heavy_inside(np.array(p))) for p in chk])) if chk else float("nan")
except Exception as ex:
    agree = f"err:{ex}"
print("cross-check heavy-9-ray agreement on top-30 worst (want ~1.0):", agree, flush=True)

# ---------------- AGGREGATE ----------------
def hist(vals, edges):
    h = [0] * (len(edges) - 1)
    for v in vals:
        for k in range(len(edges) - 1):
            if edges[k] <= v < edges[k + 1]:
                h[k] += 1; break
    return h
illegal = ill_members
n_illegal = len(illegal); n_through = sum(1 for a in illegal if a["through"])
n_axis = sum(1 for a in illegal if a["axis_transit"])
by_kind = Counter(a["kind"] for a in illegal); by_kind_through = Counter(a["kind"] for a in illegal if a["through"])
crown = [a for a in illegal if a["zmax"] > CROWN_Z]; crown_balls = [b for b in ball_records if b["z"] > CROWN_Z]
depth_edges = [0, 0.5, 1, 2, 4, 8, 1e9]; thick_edges = [0, 0.3, 0.5, 1, 2, 4, 1e9]
worst10 = illegal[:10]
summary = dict(
    n_members_total=len(members), n_balls_total=len(balls),
    n_illegal_members=n_illegal, n_through_and_through=n_through,
    n_axis_centerline_transit=n_axis, n_capsule_clip_only=n_illegal - n_axis,
    n_illegal_crossings=len(crossings),
    illegal_by_kind=dict(by_kind), through_by_kind=dict(by_kind_through),
    n_breaching_balls=len(ball_records),
    n_balls_center_inside=sum(1 for b in ball_records if b["center_inside"]),
    n_ball_spheres_reach_surface=n_touch,
    crown_illegal_members=len(crown), crown_breaching_balls=len(crown_balls),
    transit_hist_edges=depth_edges, transit_hist=hist([a["max_transit"] for a in illegal], depth_edges),
    depth_hist_edges=depth_edges, depth_hist=hist([a["max_depth"] for a in illegal], depth_edges),
    feature_thickness_hist_edges=thick_edges,
    member_through_feat_hist=hist([a["min_feat"] for a in illegal if a["min_feat"] is not None], thick_edges),
    ball_feature_thickness_hist=hist([b["feature_thickness_mm"] for b in ball_records], thick_edges),
    ball_pokeout_edges=[0, 0.1, 0.2, 0.3, 0.5, 1, 1e9],
    ball_pokeout_hist=hist([b["poke_out_mm"] for b in ball_records], [0, 0.1, 0.2, 0.3, 0.5, 1, 1e9]),
    ball_by_owner=dict(Counter(b["owner"][0] if isinstance(b["owner"], list) else b["owner"] for b in ball_records)),
    pitch_bound_mm=round(PB, 4), contact_depth_mm=CD, standoff_mm=STAND,
    min_illegal_len_mm=MIN_ILLEGAL_LEN, ball_eps_mm=BALL_EPS,
    crosscheck_heavy9ray_agreement=agree,
    delivered_verify_clearance_violations=len(S["verify"]["clearance_violations"]),
    method="Moller crossings + dense-majority ParityVote inside per sub-segment; balls via antipodal-ray span")
worst10m = [dict(member_idx=a["mi"], kind=a["kind"], embed_end=a["embed"], owner=a["owner"],
                 max_transit_mm=round(a["max_transit"], 3), max_penetration_depth_mm=round(a["max_depth"], 3),
                 min_feature_thickness_mm=(round(a["min_feat"], 3) if a["min_feat"] else None),
                 through_and_through=a["through"], n_illegal_intervals=a["n_iv"],
                 worst_location=a["worst"][1]["location"], z=a["worst"][1]["z"]) for a in worst10]
out = dict(summary=summary, worst10_members=worst10m,
           worst10_balls=sorted(ball_records, key=lambda b: -b["poke_out_mm"])[:10],
           illegal_crossings=crossings, breaching_balls=ball_records)
json.dump(out, open(os.path.join(AUDIT, "penetration_census.json"), "w"), indent=1)
json.dump(dict(summary=summary, worst10_members=worst10m, worst10_balls=out["worst10_balls"]),
          open("/Users/craig/.claude/jobs/0ddf6d41/tmp/census_summary.json", "w"), indent=1)
print("wrote penetration_census.json DONE t=%.1fs RSS=%.2fGB" % (time.time() - t0, C.rss_gb()), flush=True)
print(json.dumps(summary, indent=1))
