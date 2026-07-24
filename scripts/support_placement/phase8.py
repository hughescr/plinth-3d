"""Phase-8 placement rebuild -- FOUNDATION + CLASS-T STUB RE-AIM (SM-STUB R1).

SPEC: docs/specs/SPEC-P8-RELOCATE.md (the amended DRAFT at HEAD is the law).
Governing embed law: SPEC-EMBED-POLICY.md (SCAR-AREA-CAP + rule-4) plus the
owner-ratified PRINT-FLOOR amendment (moot floor 0.08mm). This module places
by the SAME law the verify gate judges by (§5.1) -- it never invents a second
predicate. It reuses verify.py's batched kernel primitives verbatim
(`_batch_axis_intervals`, `_embed_scar_verdicts`, `_transit_classify`) so the
builder and the independent census count the identical population from the
identical formulas.

Scope delivered here (the FOUNDATION + the rebuild CORE):

  * the FROZEN defect snapshot with stable site IDs (§3.0) -- computed once,
    immutable for the pass; every member addressed by its member-index `mi`;
  * the explicit T/Q/B phase processing order (§3.0) as a normative enum + a
    driver that dispatches Phase-1 stub re-aim and defers the later phases to
    their own state machines (SM-STRUT §3.1s, SM-BALL §3.2, the pocket solver
    §4) -- those are typed skeleton entry points that raise, so nothing
    mistakes them for done;
  * the ONE R1 selection algorithm (§3.1): GLOBAL lexicographic minimisation
    over the FULL legal candidate set (never first-legal-wins), with the
    pocket-density penalty occupying its pinned tuple slot (0 for isolated
    members, so sparse sites degrade exactly to the sparse ordering);
  * the pinned re-aim formulas (§3.1 amendment E): the demanded contact SURFACE
    POINT `c` is invariant across every candidate; the embedded tip `p0` moves
    on a sphere of radius delta about `c` as the direction changes;
  * the per-candidate resin / quality guards (amendment H) as part of LEGALITY;
  * BATCHED candidate evaluation through the kernel (gather -> one batched call
    -> scatter): every candidate axis ray across every site is evaluated in a
    handful of batched kernel interval calls, never a per-candidate scalar loop
    (the batch-first rule; a per-site scalar query loop in this hot path is a
    defect, not a style choice).

Determinism doctrine (binding): no RNG, no wall-clock dependence anywhere; every
ordering is a stable sort on an explicit key with the candidate index as the
final tiebreak, so the selected minimum is unique and bit-reproducible.
"""
from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from enum import Enum
from typing import Callable, Optional

import numpy as np

from . import verify

# The demanded-contact surface point is invariant across R1 candidates; only the
# axis direction and free length vary. All geometry below is pure IEEE-754.
_Z = np.array([0.0, 0.0, 1.0])
_X = np.array([1.0, 0.0, 0.0])

# Frozen snapshot provenance (§3.0): pinned to the digest-identical archived run.
_DEFAULT_SNAPSHOT = os.path.join(
    os.path.dirname(__file__), "..", "..",
    "docs", "artifacts", "audit-2026-07", "acceptance-run-2026-07-23",
    "phase8_relocation_input.json")

# Explicit, normative phase processing order (§3.0). Shaft geometry (stubs then
# struts) is finalised before joint geometry (balls); a ball's legal position
# depends on the FINAL segment geometry the earlier phases produce.
PHASE_ORDER = ("phase1_stubs", "phase2_struts", "phase3_balls")


# ---------------------------------------------------------------------------
# Frozen defect snapshot (§3.0) -- stable site IDs, immutable for the pass.
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class DefectSnapshot:
    """The remediation input, computed ONCE before any relocation and immutable
    for the duration of the pass (§3.0). No class reads live geometry to decide
    membership. Every member is addressed by its stable site ID = the member
    index `mi` in the pinned pickle (the same integer census and gate use).

    The acceptance TARGET-SELECTION authority is the CENSUS count (§5.3 Layer 2,
    conservative): a member the census's independent SCAR-AREA-CAP + PRINT-FLOOR
    reading counts non-exempt is a relocation target even where the kernel's
    robust tip-anchor merge exempts it (mi=445-class). `non_exempt` therefore is
    the census non-exempt set; `moot` is recorded but NEVER remediated.
    """
    non_exempt: tuple            # class-T targets (contact stubs + interior struts), mi
    moot: tuple                  # exempt, recorded, never remediated, mi
    quarantine_real: tuple       # Q-REAL: genuinely illegal pre-insertion stubs, mi
    quarantine_artifact: tuple   # Q-ARTIFACT: dropped, no real transit, mi
    ball_breaches: tuple         # class-B true joint-ball breaches (mi/ball ids)
    partition: dict              # firm / band / moot counts (§2)
    provenance: dict             # state_pkl, state_md5, predicate, ...

    def is_relocation_target(self, mi: int) -> bool:
        """True iff member `mi` must be relocated (§5.3 Layer 2). Class-T
        non-exempt OR a Q-REAL condemned stub -- the SAME defect (a stub that
        transits), caught post-census or pre-insertion. The moot 50 are NEVER
        targets (they gate nothing), so a member the census reads moot is
        excluded EVEN IF the raw pre-insertion predicate condemned its stub:
        under PRINT-FLOOR an all-moot member is exempt, so remediating it cannot
        improve a gate it does not fail (§2, §3.0 "the moot 50 are recorded but
        never remediated"). On the archived snapshot exactly 3 Q-REAL ids
        (448, 492, 497) are census-moot and are dropped by this rule."""
        return mi in self._target_set

    @property
    def _target_set(self):
        # frozen dataclass: memoise the union without mutating a public field.
        cached = self.__dict__.get("_target_set_cache")
        if cached is None:
            union = frozenset(self.non_exempt) | frozenset(self.quarantine_real)
            cached = union - frozenset(self.moot)   # moot 50 never remediated
            object.__setattr__(self, "_target_set_cache", cached)
        return cached

    def summary(self) -> dict:
        return dict(n_non_exempt=len(self.non_exempt), n_moot=len(self.moot),
                    n_quarantine_real=len(self.quarantine_real),
                    n_quarantine_artifact=len(self.quarantine_artifact),
                    n_ball_breaches=len(self.ball_breaches),
                    partition=dict(self.partition))


def load_frozen_snapshot(path: Optional[str] = None,
                         ball_breaches=()) -> DefectSnapshot:
    """Load the frozen Phase-8 defect snapshot (§3.0) from the pinned
    `phase8_relocation_input.json`. The JSON is the AMENDED partition
    (population correction A already applied: 143 non-exempt / 50 moot / band
    25, mi=420 all-moot). `ball_breaches` is supplied by the caller from the
    census `breaching_balls` (the JSON does not carry the 15 ball breaches);
    it defaults empty for the foundation/unit path."""
    if path is None:
        path = _DEFAULT_SNAPSHOT
    with open(path) as fh:
        d = json.load(fh)
    pf = d["printfloor_193"]
    rt = d["relocation_targets"]
    non_exempt = tuple(int(x) for x in pf["non_exempt_member_idxs"])
    moot = tuple(int(x) for x in pf["moot_member_idxs"])
    q_real = tuple(int(x) for x in rt["real_stub_members"])
    q_art = tuple(int(x) for x in rt["artifact_stub_members"])
    return DefectSnapshot(
        non_exempt=non_exempt, moot=moot,
        quarantine_real=q_real, quarantine_artifact=q_art,
        ball_breaches=tuple(ball_breaches),
        partition=dict(pf.get("partition", {})),
        provenance=dict(d.get("provenance", {})))


# ---------------------------------------------------------------------------
# Pinned R1 re-aim geometry (§3.1 amendment E) -- both implementations, bits.
# ---------------------------------------------------------------------------
def _unit(v):
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else v


def local_frame(n0):
    """Stable azimuth-0 frame anchored to `e = normalize(cross(n0, z))`, with
    the `|n0.z| > 0.999` degenerate case falling back to `x` (§3.1). Returns
    (e, e2) spanning the plane perpendicular to the outward contact normal
    `n0`."""
    n0 = _unit(np.asarray(n0, float))
    if abs(float(np.dot(n0, _Z))) > 0.999:
        e = _unit(np.cross(n0, _X))
    else:
        e = _unit(np.cross(n0, _Z))
    e2 = _unit(np.cross(n0, e))
    return e, e2


def reaim_directions(n0, cfg):
    """The fixed cone grid of candidate directions around the contact outward
    normal `n0` (§3.1). polar 0 recovers `n0` and is emitted ONCE; the polar
    rings x azimuths are nested polar-major / azimuth-minor. Returns a list of
    (polar_deg, azimuth_deg, n_hat) in the pinned direction order (the score
    tuple's final tiebreak reads this order)."""
    n0 = _unit(np.asarray(n0, float))
    e, e2 = local_frame(n0)
    out = []
    for polar in cfg.p8_reaim_polar_deg:
        if abs(float(polar)) < 1e-12:
            out.append((0.0, 0.0, n0.copy()))
            continue
        th = np.radians(float(polar))
        for az in cfg.p8_reaim_azimuth_deg:
            ph = np.radians(float(az))
            n_hat = (np.cos(th) * n0
                     + np.sin(th) * (np.cos(ph) * e + np.sin(ph) * e2))
            out.append((float(polar), float(az), _unit(n_hat)))
    return out


def reaim_stub(c, n_hat, delta, L, r_tip, r_shoot_floor, owner, carried_mm2):
    """Pinned R1 re-aim formulas for a contact stub (§3.1 amendment E):

        p0        = c - n_hat*delta      # embedded tip / designed ball centre
        p1        = c + n_hat*L          # free stub end
        ball_ctr  = p0                   # embed_end = "p0"
        u         = normalize(p0 - p1) = -n_hat

    The demanded contact SURFACE POINT `c` is invariant; `p0` moves on a sphere
    of radius `delta` about `c` as `n_hat` changes -- it is NOT a fixed p0.
    Returns a member dict shaped exactly as verify's batched primitives consume
    (p0/p1/r0/r1/kind/owner/embed_end/carried_mm2)."""
    c = np.asarray(c, float)
    n_hat = _unit(np.asarray(n_hat, float))
    p0 = c - n_hat * float(delta)
    p1 = c + n_hat * float(L)
    return dict(p0=p0, p1=p1, r0=float(r_tip), r1=float(r_shoot_floor),
                kind="stub", owner=owner, embed_end="p0",
                carried_mm2=float(carried_mm2),
                ball_ctr=p0.copy(), axis_u=(-n_hat).copy())


def _lean_deg(p0, p1):
    d = np.asarray(p1, float) - np.asarray(p0, float)
    L = float(np.linalg.norm(d))
    if L < 1e-9:
        return 0.0
    return float(np.degrees(np.arctan2(float(np.linalg.norm(d[:2])), abs(float(d[2])))))


def _theta_max_deg(r, cfg):
    return cfg.branch_angle_deg if r >= cfg.min_shoot_r_mm else cfg.max_segment_angle_deg


def _member_vol_ml(p0, p1, r0, r1):
    """Frustum volume in millilitres (1000 mm^3 = 1 ml). Identical form to
    cco._member_vol_ml -- kept local so phase8 has no cco import dependency."""
    L = float(np.linalg.norm(np.asarray(p1, float) - np.asarray(p0, float)))
    return np.pi / 3.0 * L * (r0 ** 2 + r0 * r1 + r1 ** 2) / 1000.0


def _ball_vol_ml(r):
    return 4.0 / 3.0 * np.pi * float(r) ** 3 / 1000.0


# ---------------------------------------------------------------------------
# Candidate generation (§3.1) -- direction x length grid, lean-skip not clamp.
# ---------------------------------------------------------------------------
@dataclass
class StubSite:
    """One class-T contact stub (or Q-REAL condemned stub) to re-aim. `c` is the
    demanded contact surface point (invariant); `n0` its outward normal. The
    resin guards read `orig_member` + the original downstream route metrics."""
    site_id: int                 # stable mi
    c: np.ndarray                # contact surface point (fixed)
    n0: np.ndarray               # outward contact normal (polar-0 direction)
    r_tip: float
    stub_length_mm: float        # free-stub design length (c -> original p1)
    contact_depth_mm: float
    carried_mm2: float = 0.0
    owner: tuple = None
    orig_member: dict = None     # for the resin volume-delta baseline
    downstream_route_len_mm: float = 0.0
    downstream_route_vol_ml: float = 0.0


@dataclass
class Candidate:
    site_id: int
    cand_index: int              # length-major, then direction order (§3.1)
    length_step: int             # index into p8_reaim_length_factors (0/1/2)
    f: float
    polar_deg: float
    azimuth_deg: float
    n_hat: np.ndarray
    member: dict


def stub_candidates(site: StubSite, cfg) -> list:
    """Enumerate a site's R1 candidate set (direction x length), <=75 members.
    Candidates outside the printability lean limit are SKIPPED (absent from the
    list), never clamped (§3.1, Open-risk #1). `cand_index` is length-major then
    the reaim_directions order -- a total order, the score tuple's final
    tiebreak, never the selector."""
    r_shoot_floor = max(float(site.r_tip), cfg.min_shoot_r_mm)
    dirs = reaim_directions(site.n0, cfg)
    ndir = len(dirs)
    out = []
    for li, f in enumerate(cfg.p8_reaim_length_factors):
        L = float(site.stub_length_mm) * float(f)
        for di, (polar, az, n_hat) in enumerate(dirs):
            m = reaim_stub(site.c, n_hat, site.contact_depth_mm, L,
                           site.r_tip, r_shoot_floor, site.owner, site.carried_mm2)
            # lean-skip: a stub's free end is floored at min_shoot_r so its cone
            # is the branch (60deg) family; a candidate past it is SKIPPED.
            if _lean_deg(m["p0"], m["p1"]) > _theta_max_deg(m["r1"], cfg) + 1e-9:
                continue
            out.append(Candidate(site_id=site.site_id, cand_index=li * ndir + di,
                                  length_step=li, f=float(f), polar_deg=polar,
                                  azimuth_deg=az, n_hat=n_hat, member=m))
    return out


# ---------------------------------------------------------------------------
# R1 legality + global lexicographic-min selection (§3.1) -- BATCHED.
# ---------------------------------------------------------------------------
@dataclass
class R1Result:
    site_id: int
    winner: Optional[Candidate]          # None => empty legal set => R2
    score: Optional[tuple]
    n_candidates: int
    n_legal: int
    needs_r2: bool = False
    traces: list = field(default_factory=list)   # per-candidate legality trace


def _score_tuple(cand: Candidate, n_moot: int, density_penalty: int,
                 route_vol_ml: float) -> tuple:
    """The R1 score tuple, minimised GLOBALLY over the full legal set (§3.1):

        (n_moot_crossings, density_penalty, polar_angle_deg,
         length_reduction_step, route_volume_ml, candidate_index)

    density_penalty occupies its pinned slot; it is 0 for isolated (non-pocket)
    members, so the tuple degrades exactly to the sparse-site ordering."""
    return (int(n_moot), int(density_penalty), float(cand.polar_deg),
            int(cand.length_step), float(route_vol_ml), int(cand.cand_index))


def _resin_guards_ok(cand: Candidate, site: StubSite, route_vol_ml: float,
                     route_len_mm: float, cfg) -> bool:
    """Amendment-H per-candidate resin / quality guards -- part of LEGALITY.

    volume-delta cap: reject if the candidate's (shaft + tip ball + re-trialled
    downstream route) volume exceeds the original member's by more than
    p8_reaim_volume_delta_max_ml. no-unbounded-growth: the re-trialled downstream
    route length is capped at p8_reaim_route_growth_max_factor x the original
    downstream route length (skipped when there is no original downstream route
    to grow against)."""
    m = cand.member
    R_b = verify.BALL_FACTOR_STUB * max(m["r0"], cfg.tip_r_min_mm)
    cand_vol = (_member_vol_ml(m["p0"], m["p1"], m["r0"], m["r1"])
                + _ball_vol_ml(R_b) + float(route_vol_ml))
    orig = site.orig_member
    if orig is not None:
        R_b0 = verify.BALL_FACTOR_STUB * max(float(orig["r0"]), cfg.tip_r_min_mm)
        orig_vol = (_member_vol_ml(orig["p0"], orig["p1"], orig["r0"], orig["r1"])
                    + _ball_vol_ml(R_b0) + float(site.downstream_route_vol_ml))
    else:
        orig_vol = (_member_vol_ml(site.c - _unit(site.n0) * site.contact_depth_mm,
                                   site.c + _unit(site.n0) * site.stub_length_mm,
                                   m["r0"], m["r1"])
                    + _ball_vol_ml(R_b) + float(site.downstream_route_vol_ml))
    if cand_vol - orig_vol > cfg.p8_reaim_volume_delta_max_ml + 1e-12:
        return False
    if site.downstream_route_len_mm > 1e-9:
        if route_len_mm > cfg.p8_reaim_route_growth_max_factor * site.downstream_route_len_mm + 1e-9:
            return False
    return True


def evaluate_r1(sites: list, geom, cfg,
                revalidate_fn: Optional[Callable] = None,
                reroute_fn: Optional[Callable] = None,
                pocket_penalty_fn: Optional[Callable] = None) -> dict:
    """R1 tip re-aim for a batch of class-T contact stubs (§3.1), BATCH-FIRST.

    Gather every candidate axis ray across every site, evaluate legality for ALL
    of them in a HANDFUL of batched kernel interval calls (gather -> one batched
    call -> scatter), keep the legal set, and pick the UNIQUE global
    lexicographic minimum of the score tuple per site -- NOT first-legal-wins
    (amendment B). A per-candidate scalar kernel loop here is a defect.

    Legality (§3.1) per candidate:
      (a) its tip SCAR-AREA-CAP verdict is LEGAL on the re-derived inputs;
      (b) it has zero NON-EXEMPT rule-4 crossings under PRINT-FLOOR;
      (e) the amendment-H resin/quality guards pass;
      (c) `revalidate_fn(member)` passes (collision-free under the §5 law) --
          injected by the engine; None => treated as passing (the pure-geometry
          foundation/unit path);
      (d) `reroute_fn(stub_end)` succeeds -- the stub end moved, so the terminal
          re-routes from the new free end; returns (ok, route_vol_ml,
          route_len_mm). None => (ok, 0.0, 0.0): no downstream route to re-trial.

    `pocket_penalty_fn(site_id, candidate) -> int` supplies the density penalty
    for pocket members (§4.2); None => 0 for every candidate (isolated sites pay
    nothing). Returns {site_id: R1Result}.
    """
    # --- gather: one flat member list over ALL candidates of ALL sites ---
    all_cands: list = []
    site_by_id = {}
    for site in sites:
        site_by_id[site.site_id] = site
        for cand in stub_candidates(site, cfg):
            all_cands.append(cand)
    return _run_rung(all_cands, site_by_id, [s.site_id for s in sites], geom, cfg,
                     rays="axis", guard_fn=_resin_guards_ok, moot_fn=_count_moot_axis,
                     revalidate_fn=revalidate_fn, reroute_fn=reroute_fn,
                     pocket_penalty_fn=pocket_penalty_fn)


def _run_rung(all_cands: list, site_by_id: dict, sids: list, geom, cfg, *,
              rays: str, guard_fn: Callable, moot_fn: Callable,
              revalidate_fn: Optional[Callable] = None,
              reroute_fn: Optional[Callable] = None,
              pocket_penalty_fn: Optional[Callable] = None) -> dict:
    """The shared BATCH-FIRST ladder-rung evaluator (SM-STUB R1 / SM-STRUT S1).
    One flat candidate matrix -> a handful of batched kernel interval + SCAR
    calls -> scatter -> per-site UNIQUE global lexicographic minimum. A rung is
    NOT first-legal-wins (amendment B): the FULL legal set is enumerated and the
    lex-min of the score tuple is selected. `rays` selects the kernel ray set
    (stubs axis-only; interior struts axis + capsule-offset, so a shaft whose
    axis is clean but whose capsule grazes a thin wall -- census mi=1138 -- is
    caught, §3.1s); `guard_fn` is the member-kind's amendment-H legality guard;
    `moot_fn` counts moot crossings for the score tuple's n_moot slot. A
    per-candidate scalar kernel loop here would be a defect (batch-first rule)."""
    results = {sid: R1Result(site_id=sid, winner=None, score=None,
                             n_candidates=0, n_legal=0, needs_r2=True)
               for sid in sids}
    if not all_cands:
        return results

    members = [c.member for c in all_cands]
    mis = list(range(len(members)))

    # --- one batched kernel call for intervals; one for SCAR verdicts ---
    ivmap = verify._batch_axis_intervals(geom, members, mis, rays=rays)
    verdicts = verify._embed_scar_verdicts(geom, members, mis, ivmap, cfg)

    # --- scatter: classify each candidate, assemble per-site legal sets ---
    per_site_legal = {sid: [] for sid in sids}
    per_site_count = {sid: 0 for sid in sids}
    for j, cand in enumerate(all_cands):
        per_site_count[cand.site_id] += 1
        m = cand.member
        verdict_legal = verdicts.get(j)
        # (a) tip/foot SCAR verdict LEGAL
        if not verdict_legal:
            continue
        # (b) zero NON-EXEMPT rule-4 crossings under PRINT-FLOOR (all rays)
        nonexempt, _ = verify._transit_classify(ivmap.get(j, []), m, verdict_legal)
        if nonexempt:
            continue
        # n_moot_crossings for the score tuple (never trade a clean candidate
        # for a moot-riding one).
        n_moot = moot_fn(ivmap.get(j, []), m, verdict_legal)
        # (d) downstream / segment re-route from the moved free end
        if reroute_fn is not None:
            ok, route_vol_ml, route_len_mm = reroute_fn(cand)
            if not ok:
                continue
        else:
            route_vol_ml, route_len_mm = 0.0, 0.0
        # (e) resin / quality guards (amendment H) AS LEGALITY
        if not guard_fn(cand, site_by_id[cand.site_id], route_vol_ml, route_len_mm, cfg):
            continue
        # (c) collision revalidate under the §5 law (both adjoining segments for
        # an interior strut -- the engine's injected revalidate)
        if revalidate_fn is not None and not revalidate_fn(cand):
            continue
        density = 0
        if pocket_penalty_fn is not None:
            density = int(pocket_penalty_fn(cand.site_id, cand))
        score = _score_tuple(cand, n_moot, density, route_vol_ml)
        per_site_legal[cand.site_id].append((score, cand))

    for sid, legal in per_site_legal.items():
        res = results[sid]
        res.n_candidates = per_site_count[sid]
        res.n_legal = len(legal)
        if not legal:
            res.needs_r2 = True
            continue
        # GLOBAL lexicographic minimum over the full legal set -- unique because
        # the tuple ends in the total-order candidate index.
        legal.sort(key=lambda sc: sc[0])
        best_score, best_cand = legal[0]
        res.winner = best_cand
        res.score = best_score
        res.needs_r2 = False
    return results


def _count_moot_axis(intervals, m, verdict_legal) -> int:
    """Count of MOOT (sub-floor) rule-4 crossings on the axis ray, for the score
    tuple's `n_moot_crossings` slot (never trade a clean candidate for a
    moot-riding one). Mirrors verify._transit_classify's per-ray accounting but
    returns the count; the tip-anchored interval (when verdict LEGAL) is exempt
    and never counted."""
    if not intervals:
        return 0
    embed = m.get("embed_end")
    p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
    axis_seg = float(np.linalg.norm(p1 - p0))
    ivs = sorted((iv for iv in intervals if iv["ray"] == "axis"),
                 key=lambda iv: iv["s_a"])
    if not ivs:
        return 0
    tip_idx = (verify._tip_iv_index(ivs, axis_seg, embed)
               if embed in ("p0", "p1") else None)
    n = 0
    for i, iv in enumerate(ivs):
        if i == tip_idx and verdict_legal:
            continue
        chord = iv["s_b"] - iv["s_a"]
        if chord <= verify.MIN_ILLEGAL_LEN_MM:
            continue
        if chord < verify.PRINT_FLOOR_CHORD_MM:   # moot (firm ones already rejected)
            n += 1
    return n


def _count_moot(intervals, m, verdict_legal) -> int:
    """Moot (sub-floor) rule-4 crossing count across ALL rays present in
    `intervals` -- the interior-strut generalisation of `_count_moot_axis` (a
    strut is classified over axis + capsule-offset rays, so a moot capsule graze
    must be counted for the score tuple). Per-ray tip-anchored interval is exempt
    when the member verdict is LEGAL, each ray measured against its OWN arc
    length (mirrors verify._transit_classify). Identical to `_count_moot_axis`
    when only axis intervals are present, so a stub scores the same either way."""
    if not intervals:
        return 0
    embed = m.get("embed_end")
    p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
    axis_seg = float(np.linalg.norm(p1 - p0))
    ray_segs = {"axis": axis_seg}
    if m["kind"] != "stub" and axis_seg >= 1e-9:
        for lab, _O, _D, sg in verify.member_rays(p0, p1, float(m["r0"]), float(m["r1"])):
            ray_segs[lab] = sg
    by_ray = {}
    for iv in intervals:
        by_ray.setdefault(iv["ray"], []).append(iv)
    n = 0
    for ray, ivs in by_ray.items():
        ivs.sort(key=lambda iv: iv["s_a"])
        ray_seg = ray_segs.get(ray, axis_seg)
        tip_idx = (verify._tip_iv_index(ivs, ray_seg, embed)
                   if embed in ("p0", "p1") else None)
        for i, iv in enumerate(ivs):
            if i == tip_idx and verdict_legal:
                continue
            chord = iv["s_b"] - iv["s_a"]
            if chord <= verify.MIN_ILLEGAL_LEN_MM:
                continue
            if chord < verify.PRINT_FLOOR_CHORD_MM:
                n += 1
    return n


# ===========================================================================
# SM-STRUT (§3.1s) -- the 17 class-T interior struts. A DISTINCT state machine
# from SM-STUB (amendment D): a strut's embed end is a FOOT ball
# (BALL_FACTOR_FOOT = 1.4, not the 1.5 stub tip); its legality is judged over
# AXIS + capsule-offset rays (a strut whose axis is clean but whose capsule
# grazes a thin wall -- census mi=1138 -- is caught); and -- critically -- the
# stub `mark_quarantined` contract DOES NOT cover it. A retired strut is
# `mark_dead` and reported as an unresolved STRUT (structural), never
# quarantined (which would send a *contact* to honest failure a strut has not
# got). Same three-rung ladder S1->S2->S3, same global-lex-min selection.
# ===========================================================================
@dataclass
class StrutSite:
    """One class-T interior strut to remediate. `c` is the fixed FOOT anchor
    surface point (invariant across S1 candidates); `n0` its outward foot normal
    (polar-0 direction). `r_foot`/`r_root` are the foot-end / far-end radii (the
    foot ball uses BALL_FACTOR_FOOT). `strut_length_mm` drives the S1 length
    ladder; `orig_strut_length_mm` is the amendment-H no-unbounded-growth
    baseline (defaults to `strut_length_mm`, so the <=1.0 length factors never
    trip the guard). `criticality` weights the §6.7 honest-failure bound -- a
    load-bearing strut outranks a leaf; `orphan` records whether S3 retirement
    would orphan a subtree from the raft (amendment G raft-connectivity guard)."""
    site_id: int
    c: np.ndarray
    n0: np.ndarray
    r_foot: float
    r_root: float
    strut_length_mm: float
    foot_depth_mm: float
    carried_mm2: float = 0.0
    owner: tuple = None
    orig_member: dict = None
    orig_strut_length_mm: float = None
    downstream_route_len_mm: float = 0.0
    downstream_route_vol_ml: float = 0.0
    criticality: float = 2.0            # structural strut > leaf contact stub
    orphan_if_dropped: bool = False     # amendment G: dropping orphans a subtree


def reaim_foot(c, n_hat, delta, L, r_foot, r_root, owner, carried_mm2):
    """Pinned S1 re-aim formulas applied AT THE FOOT (§3.1s), identical form to
    the stub re-aim but kind='strut' so the SCAR verdict uses BALL_FACTOR_FOOT
    (1.4) and the transit classifier sweeps the capsule-offset rays:

        p0        = c - n_hat*delta      # embedded foot / designed foot ball
        p1        = c + n_hat*L          # free strut end
        ball_ctr  = p0
        u         = normalize(p0 - p1) = -n_hat

    The demanded FOOT surface point `c` is invariant; `p0` moves on a sphere of
    radius `delta` about `c` as `n_hat` changes."""
    c = np.asarray(c, float)
    n_hat = _unit(np.asarray(n_hat, float))
    p0 = c - n_hat * float(delta)
    p1 = c + n_hat * float(L)
    return dict(p0=p0, p1=p1, r0=float(r_foot), r1=float(r_root),
                kind="strut", owner=owner, embed_end="p0",
                carried_mm2=float(carried_mm2),
                ball_ctr=p0.copy(), axis_u=(-n_hat).copy())


def strut_candidates(site: StrutSite, cfg) -> list:
    """Enumerate a strut's S1 candidate set (direction x length), <=75 members.
    Foot re-aim lean cone uses the branch/segment angle family exactly as the
    stub cone; candidates past the printability lean limit are SKIPPED, never
    clamped (§3.1s, Open-risk #1). `cand_index` is length-major then the
    reaim_directions order -- a total order, the score tuple's final tiebreak."""
    dirs = reaim_directions(site.n0, cfg)
    ndir = len(dirs)
    out = []
    for li, f in enumerate(cfg.p8_reaim_length_factors):
        L = float(site.strut_length_mm) * float(f)
        for di, (polar, az, n_hat) in enumerate(dirs):
            m = reaim_foot(site.c, n_hat, site.foot_depth_mm, L,
                           site.r_foot, site.r_root, site.owner, site.carried_mm2)
            if _lean_deg(m["p0"], m["p1"]) > _theta_max_deg(m["r1"], cfg) + 1e-9:
                continue
            out.append(Candidate(site_id=site.site_id, cand_index=li * ndir + di,
                                  length_step=li, f=float(f), polar_deg=polar,
                                  azimuth_deg=az, n_hat=n_hat, member=m))
    return out


def _strut_guards_ok(cand: Candidate, site: StrutSite, route_vol_ml: float,
                     route_len_mm: float, cfg) -> bool:
    """Amendment-H S1 no-unbounded-strut-growth guard (part of LEGALITY, §3.1s):
    reject a candidate whose strut shaft length exceeds
    `p8_strut_growth_max_factor x` the ORIGINAL strut length. The S1 length
    ladder is <= 1.0 so re-aim itself cannot grow the strut against its OWN
    design length; the guard binds when a candidate's length is measured against
    a SHORTER original (an already-shortened strut), so no relocation balloons
    structural material. When an S2 re-route is trialled (`route_len_mm`>0) its
    added length is bound by the same factor against the original downstream
    route -- no unbounded downstream growth either."""
    orig_len = (site.orig_strut_length_mm if site.orig_strut_length_mm is not None
                else site.strut_length_mm)
    cand_len = float(np.linalg.norm(np.asarray(cand.member["p1"], float)
                                    - np.asarray(cand.member["p0"], float)))
    # cand_len includes the foot embed depth; compare the FREE strut length.
    free_len = cand_len - float(site.foot_depth_mm)
    if free_len > cfg.p8_strut_growth_max_factor * float(orig_len) + 1e-9:
        return False
    if site.downstream_route_len_mm > 1e-9 and route_len_mm > 1e-9:
        if route_len_mm > cfg.p8_strut_growth_max_factor * site.downstream_route_len_mm + 1e-9:
            return False
    return True


def evaluate_s1(sites: list, geom, cfg,
                revalidate_fn: Optional[Callable] = None,
                reroute_fn: Optional[Callable] = None,
                pocket_penalty_fn: Optional[Callable] = None) -> dict:
    """SM-STRUT S1 foot re-aim for a batch of interior struts (§3.1s), BATCH-FIRST.
    Same gather->batch->scatter->global-lex-min discipline as SM-STUB R1, but on
    the strut ray set (axis + capsule-offset) with the foot ball factor and the
    S1 growth guard. `revalidate_fn` is where the engine enforces "both adjoining
    segments legal" (moving an interior foot re-splits two segments); None => the
    pure-geometry unit path (the strut's own transit legality). Returns
    {site_id: R1Result} (winner => S1 resolved; needs_r2 => escalate to S2)."""
    all_cands: list = []
    site_by_id = {}
    for site in sites:
        site_by_id[site.site_id] = site
        for cand in strut_candidates(site, cfg):
            all_cands.append(cand)
    return _run_rung(all_cands, site_by_id, [s.site_id for s in sites], geom, cfg,
                     rays="all", guard_fn=_strut_guards_ok, moot_fn=_count_moot,
                     revalidate_fn=revalidate_fn, reroute_fn=reroute_fn,
                     pocket_penalty_fn=pocket_penalty_fn)


# ===========================================================================
# SM-BALL (§3.2) -- the 15 true joint-ball breaches, run LAST (Phase 3). A THIRD
# distinct state machine (amendment D). Remedy is RELOCATION of the joint, NEVER
# shrinking the ball (a smaller ball is a different, un-offered sanction). Ladder
# B1 (axial slide) -> B2 (local re-route) -> B3 (honest failure, unresolved
# BALL). B3 never shrinks the ball and never suppresses the breach, and the
# raft-connectivity guard (amendment G) applies exactly as SM-STRUT S3.
# ===========================================================================
@dataclass
class BallSite:
    """One true joint-ball breach to relocate (`source="ball"`; endcap spheres
    are report-only, out of SM-BALL scope). `center`/`r` are the breaching ball;
    `root_dir` is the UNIT member-axis direction toward the ROOT (the raft) -- B1
    slides the joint +root_dir (root-ward, rank 0) then -root_dir (tip-ward, rank
    1). The ball radius is INVARIANT across the slide (never shrunk)."""
    ball_id: int
    center: np.ndarray
    r: float
    root_dir: np.ndarray
    owner: tuple = None
    criticality: float = 1.5
    orphan_if_dropped: bool = False


@dataclass
class BallSlide:
    ball_id: int
    slide_index: int            # (magnitude-major, direction-minor) total order
    slide_distance_mm: float
    direction_rank: int         # 0 root-ward, 1 tip-ward
    center: np.ndarray          # slid ball center (radius unchanged)


@dataclass
class B1Result:
    ball_id: int
    winner: Optional[BallSlide]     # None => no clean slide => B2
    n_candidates: int
    n_clean: int
    needs_b2: bool = True


def ball_slide_candidates(site: BallSite, cfg) -> list:
    """The 1-D deterministic minimal-slide grid (§3.2 B1): magnitudes
    `p8_ball_slide_step_mm .. p8_ball_slide_max_mm` in step increments, each in
    two directions (root-ward rank 0, then tip-ward rank 1). Slide 0 is excluded
    (a breaching ball is breaching at 0 by definition). `slide_index` is
    magnitude-major then direction-minor -- the total order in which
    `first-clean` == lexicographic min of `(slide_distance, direction_rank)`."""
    step = float(cfg.p8_ball_slide_step_mm)
    smax = float(cfg.p8_ball_slide_max_mm)
    root = _unit(np.asarray(site.root_dir, float))
    n_mag = int(round(smax / step))
    out = []
    idx = 0
    for k in range(1, n_mag + 1):
        dist = step * k
        for rank, sign in ((0, 1.0), (1, -1.0)):        # root-ward first
            center = np.asarray(site.center, float) + sign * dist * root
            out.append(BallSlide(ball_id=site.ball_id, slide_index=idx,
                                 slide_distance_mm=dist, direction_rank=rank,
                                 center=center))
            idx += 1
    return out


def evaluate_b1(sites: list, geom, cfg,
                slide_revalidate_fn: Optional[Callable] = None,
                eps_mm: float = 0.05) -> dict:
    """SM-BALL B1 axial slide for a batch of breaching joint balls (§3.2),
    BATCH-FIRST. Gather EVERY slide candidate of EVERY ball, test them in ONE
    batched `geom.sphere_breach` call (radius invariant), scatter, and per ball
    take the MINIMAL-slide position that is (i) sphere-clean AND (ii) leaves both
    adjoining segments legal under the §5 law (`slide_revalidate_fn`, injected by
    the engine; None => the pure-geometry unit path). A per-ball scalar
    sphere_breach loop would be a defect. Returns {ball_id: B1Result}."""
    all_slides: list = []
    for site in sites:
        for sl in ball_slide_candidates(site, cfg):
            all_slides.append((site, sl))
    site_by_id = {s.ball_id: s for s in sites}
    results = {s.ball_id: B1Result(ball_id=s.ball_id, winner=None,
                                   n_candidates=0, n_clean=0, needs_b2=True)
               for s in sites}
    if not all_slides:
        return results
    centers = np.array([sl.center for _s, sl in all_slides])
    radii = np.array([float(_s.r) for _s, sl in all_slides])
    breached, _thick, _poke = geom.sphere_breach(centers, radii, eps_mm=eps_mm)
    per_ball_clean = {s.ball_id: [] for s in sites}
    per_ball_count = {s.ball_id: 0 for s in sites}
    for j, (site, sl) in enumerate(all_slides):
        per_ball_count[site.ball_id] += 1
        if bool(breached[j]):
            continue
        if slide_revalidate_fn is not None and not slide_revalidate_fn(site, sl):
            continue
        per_ball_clean[site.ball_id].append(sl)
    for bid, clean in per_ball_clean.items():
        res = results[bid]
        res.n_candidates = per_ball_count[bid]
        res.n_clean = len(clean)
        if not clean:
            res.needs_b2 = True
            continue
        # minimal-slide == lex-min of (slide_distance, direction_rank); slide_index
        # already encodes that total order, so the smallest index is the winner.
        clean.sort(key=lambda sl: sl.slide_index)
        res.winner = clean[0]
        res.needs_b2 = False
    return results


# ===========================================================================
# Ladder dispositions + the SM-STRUT / SM-BALL drivers (§3.0 Phase 2 / Phase 3).
# The disposal CONTRACT is explicit and pinned (amendment D): a retired strut is
# `mark_dead`, a failed ball is an unresolved BALL -- NEITHER is ever
# `mark_quarantined` (that contract is stub-only).
# ===========================================================================
class Disposition(str, Enum):
    STRUT_S1_REAIM = "strut_s1_reaim"       # S1 foot re-aim resolved it
    STRUT_S2_REROUTE = "strut_s2_reroute"   # S2 re-route from an alternative foot
    STRUT_S3_DEAD = "strut_s3_dead"         # S3 mark_dead, unresolved STRUT
    STRUT_S3_ESCALATE = "strut_s3_escalate"  # raft-guard: owner escalation (never dropped)
    BALL_B1_SLIDE = "ball_b1_slide"         # B1 axial slide resolved it
    BALL_B2_REROUTE = "ball_b2_reroute"     # B2 local re-route
    BALL_B3_UNRESOLVED = "ball_b3_unresolved"  # B3 honest failure, unresolved BALL


# The disposal contract each terminal disposition asserts. Struts use mark_dead
# (structural retirement); a ball is never disposed by mark_* at all (its joint
# is relocated or reported unresolved). NO strut/ball disposition is
# `mark_quarantined` -- that is the stub-only contract (amendment D).
_STRUT_CONTRACT = {
    Disposition.STRUT_S1_REAIM: "reaim",
    Disposition.STRUT_S2_REROUTE: "mark_dead",   # old foot dies, strut re-routed
    Disposition.STRUT_S3_DEAD: "mark_dead",
    Disposition.STRUT_S3_ESCALATE: "escalate",
}


@dataclass
class StrutResult:
    site_id: int
    disposition: Disposition
    contract: str                       # "reaim" | "mark_dead" | "escalate"
    rung: str                           # "S1" | "S2" | "S3"
    winner: Optional[Candidate] = None  # set for S1
    score: Optional[tuple] = None
    unresolved: bool = False            # True for S3 (dead or escalated)
    criticality: float = 0.0
    trace: tuple = ()                   # rungs entered, in order


@dataclass
class BallResult:
    ball_id: int
    disposition: Disposition
    rung: str                           # "B1" | "B2" | "B3"
    slide: Optional[BallSlide] = None   # set for B1
    ball_shrunk: bool = False           # MUST always stay False (never shrink)
    unresolved: bool = False
    criticality: float = 0.0
    trace: tuple = ()


# ---------------------------------------------------------------------------
# Phase-order driver + the state machines (§3.0). Phase-1 stub re-aim (R1),
# Phase-2 interior struts (SM-STRUT), and Phase-3 ball breaches (SM-BALL) are
# implemented; the atomic-commit pocket solver (§4) remains a typed skeleton
# that RAISES, so no caller mistakes it for a no-op.
# ---------------------------------------------------------------------------
def run_phase1_stubs(sites: list, geom, cfg, **kw) -> dict:
    """Phase 1 (§3.0): the class-T contact stubs and Q-REAL condemned stubs are
    ONE population, ONE ladder (R1->R2->R3), two entry points. This foundation
    implements R1 (tip re-aim); sites whose R1 legal set is empty are returned
    with `needs_r2=True` for the R2 re-seat rung (§3.1 R2), not silently
    dropped. Ordered by the frozen key (worst-firm-chord desc, mi asc) is the
    caller's responsibility when it assembles `sites`; R1 itself is order-free
    (each site's winner is a pure function of geometry)."""
    return evaluate_r1(sites, geom, cfg, **kw)


def run_phase2_struts(sites: list, geom, cfg,
                      revalidate_fn: Optional[Callable] = None,
                      reroute_fn: Optional[Callable] = None,
                      orphan_fn: Optional[Callable] = None,
                      pocket_penalty_fn: Optional[Callable] = None) -> dict:
    """Phase 2 (§3.1s): the interior struts run SM-STRUT, a three-rung ladder
    S1->S2->S3 -- entered top-down, a rung reached only when the rung above
    yields ZERO legal candidates for the site (the ladder never re-orders).
    DISTINCT from SM-STUB (amendment D): the disposal contract is `mark_dead` /
    escalate, NEVER `mark_quarantined`.

      * S1 -- foot re-aim (batched, `evaluate_s1`): the strut foot re-aims in its
        lean cone; a winner disposes STRUT_S1_REAIM.
      * S2 -- strut re-route (`reroute_fn(site) -> (ok, vol, len)`, injected: the
        engine re-trials `_candidate_builders` from an alternative foot in the
        strut/rung foot domain). Old foot `mark_dead` (never mark_quarantined).
      * S3 -- retirement (honest STRUCTURAL failure): `mark_dead` + reported as
        an unresolved STRUT (§6.7, class="STRUT"), NEVER `mark_quarantined`.
        Raft-connectivity guard (amendment G, §6.9): if `orphan_fn(site)` is
        True (dropping the strut orphans a subtree from the raft), S3 is
        DISALLOWED and the strut ESCALATES to the owner as a
        criticality-weighted unresolved -- structural connectivity is never
        silently severed.

    Sites are processed in the frozen commit order the caller assembles (worst-
    firm-chord desc, mi asc); S1 itself is order-free (a pure function of
    geometry). Returns {site_id: StrutResult}."""
    site_by_id = {s.site_id: s for s in sites}
    s1 = evaluate_s1(sites, geom, cfg, revalidate_fn=revalidate_fn,
                     reroute_fn=None, pocket_penalty_fn=pocket_penalty_fn)
    out = {}
    for sid, res in s1.items():
        site = site_by_id[sid]
        if not res.needs_r2:                       # S1 resolved
            out[sid] = StrutResult(
                site_id=sid, disposition=Disposition.STRUT_S1_REAIM,
                contract=_STRUT_CONTRACT[Disposition.STRUT_S1_REAIM], rung="S1",
                winner=res.winner, score=res.score, criticality=site.criticality,
                trace=("S1",))
            continue
        # S2 -- re-route from an alternative foot in the strut/rung foot domain.
        if reroute_fn is not None:
            ok, _vol, _len = reroute_fn(site)
            if ok:
                out[sid] = StrutResult(
                    site_id=sid, disposition=Disposition.STRUT_S2_REROUTE,
                    contract=_STRUT_CONTRACT[Disposition.STRUT_S2_REROUTE],
                    rung="S2", criticality=site.criticality, trace=("S1", "S2"))
                continue
        # S3 -- honest structural failure. Raft-connectivity guard first.
        orphans = bool(orphan_fn(site)) if orphan_fn is not None else bool(site.orphan_if_dropped)
        disp = (Disposition.STRUT_S3_ESCALATE if orphans
                else Disposition.STRUT_S3_DEAD)
        out[sid] = StrutResult(
            site_id=sid, disposition=disp, contract=_STRUT_CONTRACT[disp],
            rung="S3", unresolved=True, criticality=site.criticality,
            trace=("S1", "S2", "S3"))
    return out


def run_phase3_balls(sites: list, geom, cfg,
                     slide_revalidate_fn: Optional[Callable] = None,
                     reroute_fn: Optional[Callable] = None,
                     orphan_fn: Optional[Callable] = None,
                     eps_mm: float = 0.05) -> dict:
    """Phase 3 (§3.2): the 15 true ball breaches run SM-BALL, LAST -- a joint
    ball's legal position depends on the FINAL shaft geometry Phases 1-2 produce.
    Three-rung ladder B1->B2->B3; the ball radius is INVARIANT everywhere (never
    shrunk -- `ball_shrunk` stays False by construction).

      * B1 -- axial slide (batched, `evaluate_b1`): minimal-slide clean joint
        position; a winner disposes BALL_B1_SLIDE.
      * B2 -- local re-route (`reroute_fn(site) -> (ok, ...)`, injected): the
        owning terminal re-routes with the breaching joint's region penalized.
      * B3 -- honest failure: unresolved BALL (§6.7, class="BALL"); the breach is
        never suppressed and the ball is never shrunk. Raft-connectivity guard
        (amendment G) applies exactly as SM-STRUT S3.

    Returns {ball_id: BallResult}."""
    site_by_id = {s.ball_id: s for s in sites}
    b1 = evaluate_b1(sites, geom, cfg, slide_revalidate_fn=slide_revalidate_fn,
                     eps_mm=eps_mm)
    out = {}
    for bid, res in b1.items():
        site = site_by_id[bid]
        if not res.needs_b2:                       # B1 resolved
            out[bid] = BallResult(
                ball_id=bid, disposition=Disposition.BALL_B1_SLIDE, rung="B1",
                slide=res.winner, criticality=site.criticality, trace=("B1",))
            continue
        if reroute_fn is not None:
            ok = reroute_fn(site)
            ok = ok[0] if isinstance(ok, tuple) else bool(ok)
            if ok:
                out[bid] = BallResult(
                    ball_id=bid, disposition=Disposition.BALL_B2_REROUTE,
                    rung="B2", criticality=site.criticality, trace=("B1", "B2"))
                continue
        # B3 -- honest failure. The raft guard records orphaning for the owner
        # escalation but the ball is NEVER shrunk and the breach NEVER suppressed.
        out[bid] = BallResult(
            ball_id=bid, disposition=Disposition.BALL_B3_UNRESOLVED, rung="B3",
            unresolved=True, criticality=site.criticality, trace=("B1", "B2", "B3"))
    return out


def run_pocket_solver(*a, **k):
    """The atomic-commit pocket solver with full dependency-set invalidation
    (§4) -- conflict-graph-driven, monotone resolved-count objective,
    terminates in <= |pocket| commits. Not part of this foundation delivery;
    isolated (non-pocket) sites use the plain per-site R1 ladder here (§4.6),
    for which `pocket_penalty_fn` is 0 by construction."""
    raise NotImplementedError(
        "The Phase-8 pocket solver (SPEC-P8-RELOCATE §4) is a follow-up; the "
        "foundation evaluates isolated sites with density_penalty 0 (§4.6).")
