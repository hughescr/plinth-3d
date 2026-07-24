"""P11 cross-examination: for every QUARANTINED (condemned-stub) contact site,
classify REAL vs ARTIFACT using the independent census-grade exact geometry
(Moller crossings + dense-majority ParityVote inside) -- NEVER the kernel.

The engine quarantines a stub when forest.revalidate finds an AXIS
penetration interval overlapping the stub's [inset0, L-inset1] span by > 0.05mm
(tree.py _stub_axis_transit; axis-only, mirrors census_run.py:52). Quarantined
stubs are EXCLUDED from the live forest, so the standard census never sees
them -- this pass reconstructs each stub's axis transit independently.

REAL     = census confirms an illegal through/into-model axis interval beyond
           the sanctioned tip embed (quarantine correct; Phase-8 relocation).
ARTIFACT = census finds NO illegal interior transit on the axis (the kernel's
           through-transit flag has no exact-geometry support -> check/kernel
           false positive).

Writes p11_crossexam.json; prints a REAL/ARTIFACT split + up to 10 exemplars.
"""
import numpy as np, sys, os, json, time
np.seterr(all="ignore")
SCR = "/private/tmp/claude-501/-Users-craig-code-hughescr-dnd-campaigns/0ddf6d41-f89e-4672-9577-2690d62b260c/scratchpad"
sys.path.insert(0, os.path.join(SCR, "tools"))
import census_core as C

MIN_ILLEGAL_LEN = 0.05
STEP = 0.12
CAP = 80

t0 = time.time()
S, mesh = C.load_all()
cfg = S["cfg"]
CD = cfg.contact_depth_mm
STAND = cfg.standoff_mm
sites = S.get("quarantine_sites", [])
print(f"loaded state: {len(S['members'])} live members, {len(sites)} quarantine sites "
      f"RSS={C.rss_gb():.2f}GB", flush=True)

MI = C.MeshIndex(mesh)
import routing
stree = routing.build_surface_tree(mesh)
PB = float(routing.surface_tree_pitch_bound_mm(stree, percentile=cfg.surface_pitch_bound_percentile))
del stree
PV = C.ParityVote(mesh, h=2.0)
dist_tool = C.SignedTester(mesh, MI, k=64, flip=False)
print(f"census index ready pitch_bound={PB:.3f} RSS={C.rss_gb():.2f}GB", flush=True)


def classify_stub(site):
    p0 = np.asarray(site["p0"], float)
    p1 = np.asarray(site["p1"], float)
    r0 = float(site["r0"]); r1 = float(site["r1"])
    embed = site.get("embed_end")
    O = p0.copy(); D = (p1 - p0)
    L = float(np.linalg.norm(D))
    res = dict(stub_member=site["stub_member"], contact_idx=site.get("contact_idx"),
               p0=p0.tolist(), p1=p1.tolist(), r0=round(r0, 4), r1=round(r1, 4),
               L=round(L, 4), embed_end=embed, z_tip=round(float(p0[2]), 3),
               z_root=round(float(p1[2]), 3))
    if L < 1e-9:
        res.update(classification="ARTIFACT", reason="degenerate zero-length stub",
                   illegal_len_mm=0.0, through=False)
        return res
    # sanctioned embed insets (census-independent formula, matches census_run)
    inset0 = CD + PB + r0 + STAND if embed == "p0" else 0.0
    inset1 = CD + PB + r1 + STAND if embed == "p1" else 0.0
    center = (p0 + p1) / 2.0
    R = 0.5 * L + max(r0, r1) + MI.tri_extent + 0.6
    idx = MI.candidates(center, R)
    ts = C.dedup_loc(C.ray_crossings(O, D, idx, MI), L)
    b = np.concatenate([[0.0], ts, [1.0]])
    # dense inside-majority per sub-segment
    subs = []
    allp = []
    for i in range(len(b) - 1):
        ta, tb = b[i], b[i + 1]
        Lsub = (tb - ta) * L
        ns = min(CAP, max(3, int(Lsub / STEP)))
        tt = np.linspace(ta, tb, ns + 2)[1:-1]
        spts = O[None, :] + tt[:, None] * D[None, :]
        subs.append([ta, tb, i != 0, (i + 1) != (len(b) - 1), len(allp), len(allp) + len(spts)])
        allp.append(spts)
    allp = np.concatenate(allp) if allp else np.zeros((0, 3))
    inside = PV.inside(allp) if len(allp) else np.zeros(0, bool)
    dist = np.abs(dist_tool.signed(allp)) if len(allp) else np.zeros(0)
    ins = []
    dep = []
    for sub in subs:
        a0, a1 = sub[4], sub[5]
        si = inside[a0:a1]
        ins.append(bool(si.mean() >= 0.5) if len(si) else False)
        dd = dist[a0:a1][si] if len(si) else np.array([])
        dep.append(float(dd.max()) if len(dd) else 0.0)
    ins = np.array(ins); n = len(subs)
    # merge consecutive inside subs into intervals
    ivs = []
    i = 0
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
    worst = None
    illegal_total = 0.0
    any_illegal = False
    through_any = False
    for (ta, tb, ss, es, depth) in ivs:
        aa = ta * L; bb = tb * L
        lo = max(aa, inset0); hi = min(bb, L - inset1)
        ill = hi - lo
        entry = O + ta * D; exit_ = O + tb * D
        through = bool(ss and es)
        feat = float(np.linalg.norm(exit_ - entry))
        transit = bb - aa
        if ill > MIN_ILLEGAL_LEN:
            any_illegal = True
            illegal_total += ill
            if through:
                through_any = True
            cand = dict(illegal_len_mm=round(float(ill), 4),
                        transit_len_mm=round(float(transit), 4),
                        through=through, feature_thickness_mm=round(feat, 4),
                        penetration_depth_mm=round(float(depth), 4),
                        entry=[round(float(x), 3) for x in entry],
                        exit=[round(float(x), 3) for x in exit_],
                        z=round(float(((entry + exit_) / 2.0)[2]), 3))
            if worst is None or ill > worst["illegal_len_mm"]:
                worst = cand
    res.update(inset0_mm=round(inset0, 4), inset1_mm=round(inset1, 4),
               n_crossings=int(len(ts)), n_inside_intervals=len(ivs),
               illegal_len_total_mm=round(illegal_total, 4),
               through=through_any, worst_interval=worst)
    if any_illegal:
        res["classification"] = "REAL"
        res["reason"] = ("axis transits through model (through-and-through)"
                         if through_any else
                         "axis penetrates model interior beyond sanctioned tip embed")
    else:
        res["classification"] = "ARTIFACT"
        res["reason"] = ("no exact interior transit on axis beyond embed inset "
                         "(kernel through-transit flag unsupported by census geometry)")
    return res


records = []
for k, site in enumerate(sites):
    r = classify_stub(site)
    records.append(r)
    if (k + 1) % 10 == 0 or k == len(sites) - 1:
        print(f"  classified {k+1}/{len(sites)} t={time.time()-t0:.1f}s", flush=True)

n_real = sum(1 for r in records if r["classification"] == "REAL")
n_art = sum(1 for r in records if r["classification"] == "ARTIFACT")
n_through = sum(1 for r in records if r.get("through"))
reals = [r for r in records if r["classification"] == "REAL"]
arts = [r for r in records if r["classification"] == "ARTIFACT"]
reals.sort(key=lambda r: -(r["illegal_len_total_mm"]))
arts.sort(key=lambda r: -(r["illegal_len_total_mm"]))
exemplars = (reals[:7] + arts[:3]) if (reals or arts) else records[:10]

summary = dict(
    n_quarantine_sites=len(sites), quarantine_real=n_real,
    quarantine_artifact=n_art, n_through_and_through=n_through,
    method="census-grade axis Moller crossings + dense-majority ParityVote inside, "
           "sanctioned tip-embed inset (CD+PB+r+standoff); REAL iff illegal interior "
           "interval > 0.05mm survives inset",
    pitch_bound_mm=round(PB, 4), contact_depth_mm=CD, standoff_mm=STAND,
    min_illegal_len_mm=MIN_ILLEGAL_LEN)
out = dict(summary=summary, exemplars=exemplars, all_sites=records)
with open(os.path.join(SCR, "p11_crossexam.json"), "w") as f:
    json.dump(out, f, indent=1, default=str)
print("P11 " + json.dumps(summary), flush=True)
print(f"wrote p11_crossexam.json  REAL={n_real} ARTIFACT={n_art} t={time.time()-t0:.1f}s "
      f"RSS={C.rss_gb():.2f}GB", flush=True)
