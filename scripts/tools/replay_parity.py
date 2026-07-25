"""A23(a)/sec-6-item-9 production-mesh replay-corpus parity gate.

Replays the seeded corpus of REAL geometry-call inputs captured during run1
through BOTH the Rust kernel (KernelSurface) and the FROZEN Python oracle
(geometry_exact.ExactSurface), asserting the sec-5/6 tier-aware parity
contracts: verdict BOOLEANS exact (inside, sphere_breach.breached modulo the
Amendment-A15 measured-tie OR), clearance sign-at-0 parity + the one-directional
Tier-1 bound clr_kernel <= clr_oracle + 1e-9, and 1e-9-relative on finite values
elsewhere. Bounded sample per method. Prints a per-method parity table.
"""
import sys, os, json, time, pickle
import numpy as np
np.seterr(all="ignore")
import _toolpaths
WT = _toolpaths.REPO_ROOT
SCR = _toolpaths.scratch_dir()
sys.path.insert(0, _toolpaths.SCRIPTS_DIR)
from support_placement import mesh_io, geometry_exact
from support_placement.geometry_kernel import KernelSurface

STL = _toolpaths.resolve_stl()
PER_METHOD = 2500          # bounded replay sample per method
RTOL, ATOL = 1e-9, 1e-9

t0 = time.time()
mesh = mesh_io.load_stl(STL)
mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
corp = pickle.load(open(os.path.join(SCR, "run1_corpus.pkl"), "rb"))["corpus"]
print(f"mesh {len(mesh.faces)} faces; corpus methods {list(corp.keys())}", flush=True)

K = KernelSurface(mesh)
O = geometry_exact.ExactSurface(mesh)
print(f"both oracles built t={time.time()-t0:.1f}s", flush=True)

rng = np.random.RandomState(7)
def sample(rows, n=PER_METHOD):
    if len(rows) <= n:
        return list(rows)
    return [rows[i] for i in rng.choice(len(rows), n, replace=False)]

def relclose(a, b, rtol=RTOL, atol=ATOL):
    a = np.asarray(a, float); b = np.asarray(b, float)
    fin = np.isfinite(a) & np.isfinite(b)
    both_inf = (~np.isfinite(a)) & (~np.isfinite(b))
    ok = np.zeros(a.shape, bool)
    ok[both_inf] = True
    ok[fin] = np.abs(a[fin] - b[fin]) <= (atol + rtol * np.abs(b[fin]))
    return ok

results = {}

# ---- project (distance + closest point + normal) ----
if "project" in corp:
    rows = sample(corp["project"])
    P = np.array([r[0] for r in rows], float)
    dk, cpk, ik, fnk = K.project(P)
    do, cpo, ico, fno = O.project(P)
    dmax = float(np.max(np.abs(dk - do)))
    cpmax = float(np.max(np.abs(cpk - cpo)))
    results["project"] = dict(n=len(rows), dist_max_abs=dmax, cp_max_abs=cpmax,
                              dist_parity=bool(relclose(dk, do).all()),
                              cp_parity=bool(relclose(cpk.ravel(), cpo.ravel(), 1e-8, 1e-8).all()))

# ---- inside (verdict boolean, must be EXACT) ----
if "inside" in corp:
    rows = sample(corp["inside"])
    P = np.array([r[0] for r in rows], float)
    ik = np.asarray(K.inside(P)); io = np.asarray(O.inside(P))
    results["inside"] = dict(n=len(rows), disagreements=int((ik != io).sum()),
                             verdict_parity=bool((ik == io).all()))

# ---- query (nearest distance) ----
if "query" in corp:
    rows = sample(corp["query"])
    P = np.array([r[0] for r in rows], float)
    dk, _ = K.query(P); do, _ = O.query(P)
    results["query"] = dict(n=len(rows), dist_max_abs=float(np.max(np.abs(dk - do))),
                            dist_parity=bool(relclose(dk, do, 1e-8, 1e-8).all()))

# ---- cast_rays (offsets/first-hit t) : compare min-t per ray ----
if "cast_rays" in corp:
    rows = sample(corp["cast_rays"], 1500)
    Og = np.array([r[0] for r in rows], float)
    Dg = np.array([r[1] for r in rows], float)
    tmax = float(rows[0][2])
    rk = K.cast_rays(Og, Dg, tmax); ro = O.cast_rays(Og, Dg, tmax)
    def mint(res):
        out = []
        for ts, _f in res:
            out.append(float(np.min(ts)) if len(ts) else np.inf)
        return np.array(out)
    tk = mint(rk); to = mint(ro)
    hitk = np.isfinite(tk); hito = np.isfinite(to)
    results["cast_rays"] = dict(n=len(rows), hit_verdict_parity=bool((hitk == hito).all()),
                                hit_disagreements=int((hitk != hito).sum()),
                                mint_max_abs=float(np.max(np.abs(np.where(hitk & hito, tk - to, 0.0)))))

# ---- segment_clearance (Tier-aware: sign@0 + clr_k <= clr_o + 1e-9) ----
if "segment_clearance" in corp:
    rows = sample(corp["segment_clearance"])
    p0 = np.array([r[0] for r in rows], float); p1 = np.array([r[1] for r in rows], float)
    r0 = np.array([r[2] for r in rows], float); r1 = np.array([r[3] for r in rows], float)
    standoff = float(rows[0][4])
    i0 = np.array([r[5] for r in rows], float); i1 = np.array([r[6] for r in rows], float)
    ck, _ = K.segment_clearance(p0, p1, r0, r1, standoff, inset0s=i0, inset1s=i1)
    co, _ = O.segment_clearance(p0, p1, r0, r1, standoff, inset0s=i0, inset1s=i1)
    fin = np.isfinite(ck) & np.isfinite(co)
    sign_parity = bool(((ck > 0) == (co > 0)).all())
    tier1_bound = bool((ck[fin] <= co[fin] + 1e-9).all())         # kernel never over-reports clearance
    val_close = relclose(ck, co, 1e-7, 1e-7)
    # the 0.1 revalidate gate verdict
    gate_k = ck < 0.1; gate_o = co < 0.1
    results["segment_clearance"] = dict(
        n=len(rows), sign_at_0_parity=sign_parity, tier1_one_directional_bound=tier1_bound,
        value_close_frac_1e7=round(float(val_close.mean()), 6),
        max_abs_when_both_finite=float(np.max(np.abs(ck[fin]-co[fin]))) if fin.any() else 0.0,
        gate0p1_disagreements=int((gate_k != gate_o).sum()),
        gate0p1_kernel_ge_oracle=bool((gate_k.astype(int) >= gate_o.astype(int)).all()))

# ---- sphere_breach (breached EXACT modulo A15 measured-tie OR) ----
if "sphere_breach" in corp:
    rows = sample(corp["sphere_breach"])
    C = np.array([r[0] for r in rows], float); R = np.array([r[1] for r in rows], float)
    bk, thk, pok = K.sphere_breach(C, R)
    bo, tho, poo = O.sphere_breach(C, R)
    bk = np.asarray(bk); bo = np.asarray(bo)
    disagree = bk != bo
    # A15: at measured ties kernel may OR-breach -> breached_kernel >= breached_oracle
    one_dir = bool((bk.astype(int) >= bo.astype(int))[disagree].all()) if disagree.any() else True
    both = bk & bo
    results["sphere_breach"] = dict(
        n=len(rows), breached_disagreements=int(disagree.sum()),
        breached_exact=bool((~disagree).all()),
        disagreements_all_kernel_ge_oracle_A15=one_dir,
        thickness_close_frac_1e9_where_both=round(float(relclose(thk[both], tho[both]).mean()) if both.any() else 1.0, 6))

# ---- penetration_intervals (batch replay; compare illegal-verdict) ----
if "penetration_intervals_batch" in corp:
    rows = sample(corp["penetration_intervals_batch"], 1500)
    p0 = np.array([r[0] for r in rows], float); p1 = np.array([r[1] for r in rows], float)
    r0 = np.array([r[2] for r in rows], float); r1 = np.array([r[3] for r in rows], float)
    perk = K.penetration_intervals_batch(p0, p1, r0, r1, rays="axis", min_illegal_len_mm=0.05)
    def has_illegal(iv_list):
        for iv in iv_list:
            if (iv["s_b"] - iv["s_a"]) > 0.05:
                return True
        return False
    vk = np.array([has_illegal(perk[j]) for j in range(len(rows))])
    # oracle per-member
    vo = []
    for j in range(len(rows)):
        ivo = O.penetration_intervals(p0[j], p1[j], r0[j], r1[j], rays="axis", min_illegal_len_mm=0.05)
        vo.append(has_illegal(ivo))
    vo = np.array(vo)
    results["penetration_intervals"] = dict(
        n=len(rows), illegal_verdict_disagreements=int((vk != vo).sum()),
        illegal_verdict_parity=bool((vk == vo).all()))

allpass = True
for m, r in results.items():
    for k, v in r.items():
        if k.endswith("parity") or k.endswith("_exact") or k == "tier1_one_directional_bound":
            if v is False:
                allpass = False
results["_overall"] = dict(all_verdict_parity=allpass, sample_per_method=PER_METHOD,
                           elapsed_s=round(time.time() - t0, 1))
json.dump(results, open(os.path.join(SCR, "replay_parity.json"), "w"), indent=1, default=str)
print("REPLAY_PARITY " + json.dumps(results, default=str), flush=True)
print(f"OVERALL verdict parity = {'PASS' if allpass else 'FAIL'} t={time.time()-t0:.1f}s", flush=True)
