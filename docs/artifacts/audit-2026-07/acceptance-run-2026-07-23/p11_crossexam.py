"""P11 condemned-stub cross-examination -- RATIFIED predicate (refreshed 2026-07-24).

RETIREMENT NOTE: this tool previously classified quarantine sites with the
RETIRED CD+PB arc-inset window on a dense ParityVote at h=2.0 (contact_depth +
pitch_bound + r + standoff), which graded the 80 sites 55 REAL / 25 ARTIFACT.
That predicate was superseded by the owner-ratified SCAR-AREA-CAP embed policy
(SPEC-EMBED-POLICY.md, panel ruling cd989fe) plus the rule-4 through-transit test
and the PRINT-FLOOR moot amendment (OWNER RATIFIED 2026-07-24). The arc-inset
outputs (former results/p11_crossexam.json, results/rerun_p11_crossexam.json) were
stale-tool artifacts and have been removed -- see RETIRED-TOOLS.md.

This refreshed tool re-grades every quarantine site through the ENGINE's ratified
verify path -- geometry_exact.penetration_intervals (kernel axis Moller crossings)
+ verify._embed_scar_verdicts (SCAR-AREA-CAP) + verify._transit_classify (rule-4 +
PRINT-FLOOR chord<0.08mm moot floor) -- and cross-checks three INDEPENDENT
inside-oracles (kernel ExactSurface parity, census dense-majority ParityVote,
independent 16-ray exact trimesh) on the buried-root re-entry interior points.

Result under the ratified predicate: 77 REAL / 3 ARTIFACT (ARTIFACTs 437/440/444 =
single tip-anchored LEGAL deep plugs). Emits results/p11_regrade_ratified.json.
The settled ruling-1A ground-truth fixture derived from this run is committed at
scripts/tests/fixtures_p11.json and guarded by scripts/tests/test_p11_fixture.py
(hermetic -- no real STL required).

env:
  CENSUS_STATE  archived forest pickle (default: v5e_run1.pkl in this scratchpad)
  TREANT_STL    source STL (default: repo-root Weeping Treant v1.2.stl)
"""
import numpy as np, sys, os, json, time, hashlib
np.seterr(all="ignore")

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
STATE = os.environ.get("CENSUS_STATE",
    "/private/tmp/claude-501/-Users-craig-code-hughescr-dnd-campaigns/"
    "0ddf6d41-f89e-4672-9577-2690d62b260c/scratchpad/v5e_run1.pkl")
def _resolve_stl():
    if os.environ.get("TREANT_STL"):
        return os.environ["TREANT_STL"]
    name = "Weeping Treant v1.2.stl"
    # worktrees share the model with the main working tree; probe a few roots.
    for cand in (os.path.join(ROOT, name),
                 os.path.join(ROOT, "..", "..", name),   # dnd-campaigns/ (main repo)
                 os.path.join(ROOT, "..", name)):
        if os.path.exists(cand):
            return os.path.abspath(cand)
    return os.path.join(ROOT, name)


STL = _resolve_stl()
os.environ["CENSUS_STATE"] = STATE

from support_placement.geometry_exact import ExactSurface
from support_placement import verify
from support_placement.config import SupportConfig  # noqa: F401 (asserts cfg parity)

sys.path.insert(0, os.path.join(ROOT, "docs/artifacts/audit-2026-07/tools"))
import census_core as C
C.STATE = STATE
import trimesh, pickle

FLOOR = verify.PRINT_FLOOR_CHORD_MM
MIN_ILL = verify.MIN_ILLEGAL_LEN_MM
LABELS = {3: "ARTIFACT", 8: "REAL", 223: "REAL", 232: "REAL", 253: "REAL",
          297: "REAL", 316: "REAL", 323: "REAL", 325: "REAL", 333: "REAL"}

t0 = time.time()
S = pickle.load(open(STATE, "rb"))
mesh = trimesh.load(STL, file_type="stl", process=False)
mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
cfg = S["cfg"]
sites = S["quarantine_sites"]
state_md5 = hashlib.md5(open(STATE, "rb").read()).hexdigest()
print(f"loaded {len(sites)} sites floor={FLOOR} min_ill={MIN_ILL} md5={state_md5}", flush=True)

MI = C.MeshIndex(mesh)
PV = C.ParityVote(mesh, h=2.0)
geom = ExactSurface(mesh)
print(f"indices ready t={time.time()-t0:.1f}s RSS={C.rss_gb():.2f}GB", flush=True)


def ray16_inside(P):
    ga = np.pi * (3 - 5 ** 0.5)
    votes = 0
    for i in range(16):
        z = 1 - (i + 0.5) * 2.0 / 16
        rr = (1 - z * z) ** 0.5
        th = ga * i
        d = np.array([rr * np.cos(th), rr * np.sin(th), z], float)
        loc = mesh.ray.intersects_location([P], [d])[0]
        votes += 1 if (len(loc) % 2 == 1) else 0
    return votes >= 8


def make_m(site):
    return dict(p0=np.asarray(site["p0"], float), p1=np.asarray(site["p1"], float),
                r0=float(site["r0"]), r1=float(site["r1"]), kind=site["kind"],
                owner=site.get("owner"), embed_end=site["embed_end"])


def classify(site):
    m = make_m(site)
    ivs = geom.penetration_intervals(m["p0"], m["p1"], m["r0"], m["r1"],
                                     rays="axis", min_illegal_len_mm=MIN_ILL)
    verdicts = verify._embed_scar_verdicts(geom, [m], [0], {0: ivs}, cfg)
    vlegal = verdicts.get(0)
    nonexempt, moot = verify._transit_classify(ivs, m, vlegal)
    embed = m["embed_end"]
    seg = float(np.linalg.norm(m["p1"] - m["p0"]))
    axis_ivs = sorted([iv for iv in ivs if iv["ray"] == "axis"], key=lambda x: x["s_a"])
    tip_idx = (verify._tip_iv_index(axis_ivs, seg, embed)
               if embed in ("p0", "p1") else None)
    O = m["p0"].copy(); D = m["p1"] - m["p0"]
    crossings = []
    for i, iv in enumerate(axis_ivs):
        chord = iv["s_b"] - iv["s_a"]
        entry = O + (iv["s_a"] / seg) * D
        exit_ = O + (iv["s_b"] / seg) * D
        crossings.append(dict(s_a=round(float(iv["s_a"]), 4), s_b=round(float(iv["s_b"]), 4),
                              chord_mm=round(float(chord), 4), tip_anchored=(i == tip_idx),
                              feature_thickness_mm=round(float(np.linalg.norm(exit_ - entry)), 4),
                              below_floor=bool(chord < FLOOR),
                              counts_illegal=bool(chord > MIN_ILL)))
    return dict(mi=site["stub_member"], embed_end=embed, L=round(seg, 4),
                p0=[round(float(x), 4) for x in m["p0"]],
                p1=[round(float(x), 4) for x in m["p1"]],
                r0=round(m["r0"], 4), r1=round(m["r1"], 4),
                verdict_legal=(None if vlegal is None else bool(vlegal)),
                n_axis_intervals=len(axis_ivs), tip_idx=tip_idx,
                nonexempt=bool(nonexempt), moot=bool(moot),
                classification=("REAL" if nonexempt else "ARTIFACT"),
                crossings=crossings)


records = [classify(s) for s in sites]
by_mi = {r["mi"]: r for r in records}
reals = sorted(r["mi"] for r in records if r["nonexempt"])
arts = sorted(r["mi"] for r in records if not r["nonexempt"])
moots = sorted(r["mi"] for r in records if r["moot"])
print(f"\n80-site RATIFIED regrade: REAL={len(reals)} ARTIFACT={len(arts)} "
      f"(moot bucket={len(moots)}) ARTIFACT mis={arts}", flush=True)

# three-oracle agreement on buried-root re-entry exemplars.
# The exemplar member indices below are keyed to the PRE-contact-fix numbering
# (state v5e_run1.pkl). Removing phantom/adequately-carried minima reindexes the
# forest, so on a later state these ids need not exist -- skip what is absent
# rather than crashing; the 80-site regrade above is the deliverable.
oracle = {}
for mi in (223, 232, 8, 3):
    rec = by_mi.get(mi)
    if rec is None:
        continue
    O = np.asarray(rec["p0"], float); D = np.asarray(rec["p1"], float) - O
    seg = rec["L"]
    pts = []
    for cr in rec["crossings"]:
        if cr["tip_anchored"]:
            continue
        smid = 0.5 * (cr["s_a"] + cr["s_b"])
        P = O + (smid / seg) * D
        k_in = bool(geom.inside(np.array([P]))[0])
        pv_in = bool(PV.inside(np.array([P]))[0])
        r16 = bool(ray16_inside(P))
        pts.append(dict(s_mid=round(float(smid), 4), point=[round(float(x), 3) for x in P],
                        kernel_inside=k_in, census_parityvote_inside=pv_in,
                        ray16_inside=r16, agree=bool(k_in == pv_in == r16)))
    oracle[mi] = pts

r3 = by_mi.get(3)
if r3 is not None:
    unanch = [cr for cr in r3["crossings"] if not cr["tip_anchored"] and cr["counts_illegal"]]
    mi3 = dict(mi=3, classification_raw=r3["classification"], nonexempt=r3["nonexempt"],
               moot=r3["moot"], unanchored_illegal_crossings=unanch,
               all_chords_below_floor=bool(unanch and all(c["below_floor"] for c in unanch)),
               min_chord_mm=(min(c["chord_mm"] for c in unanch) if unanch else None),
               max_chord_mm=(max(c["chord_mm"] for c in unanch) if unanch else None),
               floor_mm=FLOOR)
else:
    mi3 = dict(mi=3, absent=True, floor_mm=FLOOR)

out = dict(
    predicate="SCAR-AREA-CAP + rule-4 (through-transit) + PRINT-FLOOR moot floor",
    print_floor_chord_mm=FLOOR, min_illegal_len_mm=MIN_ILL,
    state_pkl=os.path.basename(STATE), state_md5=state_md5,
    regrade_80=dict(n_sites=len(sites), n_real=len(reals), n_artifact=len(arts),
                    n_moot=len(moots), artifact_mis=arts, moot_mis=moots, real_mis=reals),
    ruling_1a_labels=LABELS,
    exemplars={str(mi): by_mi[mi] for mi in sorted(LABELS) if mi in by_mi},
    three_oracle={str(k): v for k, v in oracle.items()}, mi3_print_floor=mi3)
OUT = os.environ.get("P11_OUT") or os.path.join(
    os.path.dirname(__file__), "results", "p11_regrade_ratified.json")
os.makedirs(os.path.dirname(OUT), exist_ok=True)
json.dump(out, open(OUT, "w"), indent=1)
print(f"wrote {OUT}", flush=True)
for mi in sorted(LABELS):
    r = by_mi.get(mi)
    if r is None:
        print(f"  mi={mi}: GT={LABELS[mi]:8s} ABSENT from this state's quarantine set",
              flush=True)
        continue
    print(f"  mi={mi}: GT={LABELS[mi]:8s} raw={r['classification']:8s} "
          f"moot={r['moot']} verdict_legal={r['verdict_legal']}", flush=True)
for mi in (223, 232, 8):
    if mi not in oracle:
        continue
    ag = all(p["agree"] for p in oracle[mi]) if oracle[mi] else None
    print(f"  three-oracle mi={mi}: {len(oracle[mi])} pts all-agree={ag}", flush=True)
if not mi3.get("absent"):
    print(f"mi=3 print-floor all_chords_below_floor={mi3['all_chords_below_floor']} "
          f"(min={mi3['min_chord_mm']} max_arc={mi3['max_chord_mm']} floor={FLOOR}) "
          f"-> moot={mi3['moot']}", flush=True)
print(f"done t={time.time()-t0:.1f}s RSS={C.rss_gb():.2f}GB", flush=True)
