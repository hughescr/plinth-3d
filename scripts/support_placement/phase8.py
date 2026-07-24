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
    frozen_worst_chord_mm: float = 0.0   # §4.3 pocket commit-priority key (frozen)


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


def _batch_legal_sets(all_cands: list, site_by_id: dict, sids: list, geom, cfg, *,
                      rays: str, guard_fn: Callable, moot_fn: Callable,
                      revalidate_fn: Optional[Callable] = None,
                      reroute_fn: Optional[Callable] = None,
                      pocket_penalty_fn: Optional[Callable] = None):
    """The shared BATCH-FIRST legality evaluator: one flat candidate matrix ->
    a handful of batched kernel interval + SCAR calls -> scatter -> per-site
    legal set, each SORTED by the §3.1 score tuple (lexicographic). Returns
    `(per_site_legal, per_site_count)` where `per_site_legal[sid]` is the FULL
    sorted `[(score, cand), ...]` list (never first-legal-truncated -- amendment
    B). `rays` selects the kernel ray set (stubs axis-only; interior struts axis
    + capsule-offset, so a shaft whose axis is clean but whose capsule grazes a
    thin wall -- census mi=1138 -- is caught, §3.1s); `guard_fn` is the member-
    kind's amendment-H legality guard; `moot_fn` counts moot crossings for the
    score tuple's n_moot slot. A per-candidate scalar kernel loop here would be a
    defect (batch-first rule).

    Both consumers read this ONE evaluator: the ladder rungs (`_run_rung` -> the
    unique global lex-min) and the pocket solver (§4, which needs the whole
    sorted list per member for its atomic-commit walk)."""
    per_site_legal = {sid: [] for sid in sids}
    per_site_count = {sid: 0 for sid in sids}
    if not all_cands:
        return per_site_legal, per_site_count

    members = [c.member for c in all_cands]
    mis = list(range(len(members)))

    # --- one batched kernel call for intervals; one for SCAR verdicts ---
    ivmap = verify._batch_axis_intervals(geom, members, mis, rays=rays)
    verdicts = verify._embed_scar_verdicts(geom, members, mis, ivmap, cfg)

    # --- scatter: classify each candidate, assemble per-site legal sets ---
    for j, cand in enumerate(all_cands):
        per_site_count[cand.site_id] += 1
        m = cand.member
        verdict_legal = verdicts.get(j)
        # (a)+(b) SCAR-AREA-CAP verdict LEGAL *and* zero NON-EXEMPT rule-4
        # crossings under PRINT-FLOOR (all rays) -- evaluated through the ONE
        # shared certificate the acceptance verifier also calls
        # (verify.certify_member, defect-1 b): a candidate can never be certified
        # legal here and flagged by the gate, nor vice-versa.
        if not verify.certify_member(ivmap.get(j, []), m, verdict_legal)["legal"]:
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
        # GLOBAL lexicographic order over the full legal set -- a total order
        # because the score tuple ends in the total-order candidate index.
        legal.sort(key=lambda sc: sc[0])
    return per_site_legal, per_site_count


def _run_rung(all_cands: list, site_by_id: dict, sids: list, geom, cfg, *,
              rays: str, guard_fn: Callable, moot_fn: Callable,
              revalidate_fn: Optional[Callable] = None,
              reroute_fn: Optional[Callable] = None,
              pocket_penalty_fn: Optional[Callable] = None) -> dict:
    """The shared BATCH-FIRST ladder-rung evaluator (SM-STUB R1 / SM-STRUT S1):
    `_batch_legal_sets` -> per-site UNIQUE global lexicographic minimum. A rung
    is NOT first-legal-wins (amendment B): the FULL legal set is enumerated and
    the lex-min of the score tuple is selected."""
    results = {sid: R1Result(site_id=sid, winner=None, score=None,
                             n_candidates=0, n_legal=0, needs_r2=True)
               for sid in sids}
    per_site_legal, per_site_count = _batch_legal_sets(
        all_cands, site_by_id, sids, geom, cfg, rays=rays, guard_fn=guard_fn,
        moot_fn=moot_fn, revalidate_fn=revalidate_fn, reroute_fn=reroute_fn,
        pocket_penalty_fn=pocket_penalty_fn)
    for sid, legal in per_site_legal.items():
        res = results[sid]
        res.n_candidates = per_site_count[sid]
        res.n_legal = len(legal)
        if not legal:
            res.needs_r2 = True
            continue
        # GLOBAL lexicographic minimum over the full legal set -- unique because
        # the tuple ends in the total-order candidate index.
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
    frozen_worst_chord_mm: float = 0.0  # §4.3 pocket commit-priority key (frozen)


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


# ===========================================================================
# §4 POCKET SOLVER -- atomic-commit loop with full dependency-set invalidation.
# ===========================================================================
# The convergence-critical piece for the three dense defect clusters (canopy P1
# x6-12/y-12..-6/z48-54 ~37, canopy P2 x18-36/y6-18/z48-60 ~22, lower-trunk P3
# x-36..-30/y12-18/z-24..-18 17). Above CROWN_Z=35 in dense foliage a per-site
# greedy re-aim THRASHES: a re-aimed shaft can create a NEW crossing through a
# neighbour's corridor, and a naive sequential pass can oscillate. The draft's
# unary scorer against a stale snapshot, its "two sweeps" cutoff, and its
# candidate-ray-only invalidation are REJECTED (amendment F). This is an atomic
# per-member commit loop with a MONOTONE resolved-count objective and FULL
# dependency-set invalidation, driven by a precomputed conflict graph. It
# terminates in <= |pocket| commits, never oscillates, and is bit-deterministic
# (§4.5): world-origin-pinned voxels + lexicographic component labels + a total
# commit-priority order + total-order score tuples => every choice is unique.
#
# One law, one authority: legality against the MODEL SURFACE reuses the SAME
# batched verify primitives (`_batch_legal_sets`) the ladder rungs and the
# independent census use -- the pocket solver never invents a second predicate.
# Support/support interaction (the "no NEW firm transit into a committed
# neighbour" mutual-legality guard, §4.3-3-ii / §6.9) is a pure geometric
# capsule predicate applied at COMMIT time over the bounded committed subset of
# a member's conflict neighbourhood; the engine may inject its own §5-law check.


# --- pure-geometry segment primitives (deterministic; no RNG, no sampling) ---
def _pt_seg_dist(pt, q0, q1) -> float:
    """Distance from point `pt` to segment (q0,q1)."""
    pt = np.asarray(pt, float); q0 = np.asarray(q0, float); q1 = np.asarray(q1, float)
    d = q1 - q0
    L2 = float(np.dot(d, d))
    if L2 < 1e-18:
        return float(np.linalg.norm(pt - q0))
    t = float(np.dot(pt - q0, d) / L2)
    t = min(1.0, max(0.0, t))
    return float(np.linalg.norm(pt - (q0 + t * d)))


def _seg_seg_dist_batch(p0, p1, Q0, Q1):
    """Minimum distance between segment P=(p0,p1) and EACH segment
    (Q0[k],Q1[k]) -- one vectorised clamped closest-approach (Ericson RTCD) over
    the K neighbour corridors, not a per-neighbour scalar loop. Returns (K,)."""
    p0 = np.asarray(p0, float); p1 = np.asarray(p1, float)
    Q0 = np.atleast_2d(np.asarray(Q0, float)); Q1 = np.atleast_2d(np.asarray(Q1, float))
    d1 = p1 - p0                             # (3,)  the SAME segment for all K
    d2 = Q1 - Q0                             # (K,3)
    r = p0[None, :] - Q0                     # (K,3)
    a = float(np.dot(d1, d1))                # scalar  = |P|^2
    e = np.sum(d2 * d2, axis=1)              # (K,)    = |Q_k|^2
    f = np.sum(d2 * r, axis=1)               # (K,) dot(d2_k, r_k)
    c = r @ d1                               # (K,) dot(d1, r_k)
    b = d2 @ d1                              # (K,) dot(d1, d2_k)
    K = e.shape[0]
    e_safe = np.where(e < 1e-18, 1.0, e)
    if a < 1e-18:                            # P degenerate -> a point at p0
        s = np.zeros(K)
        t = np.where(e < 1e-18, 0.0, np.clip(f / e_safe, 0.0, 1.0))
    else:
        denom = a * e - b * b                # (K,)
        denom_safe = np.where(denom > 1e-18, denom, 1.0)
        s = np.where(denom > 1e-18,
                     np.clip((b * f - c * e) / denom_safe, 0.0, 1.0), 0.0)
        t = np.where(e < 1e-18, 0.0, (b * s + f) / e_safe)
        recl = (t < 0.0) | (t > 1.0)
        t = np.clip(t, 0.0, 1.0)
        # Ericson: where t clamped, recompute s = clip((b*t - c)/a, 0, 1)
        s = np.where(recl, np.clip((b * t - c) / a, 0.0, 1.0), s)
    cp_P = p0[None, :] + s[:, None] * d1[None, :]
    cp_Q = Q0 + t[:, None] * d2
    return np.linalg.norm(cp_P - cp_Q, axis=1)


def _seg_capsule_chord(p0, p1, q0, q1, rad) -> float:
    """Length of the sub-interval of segment P=(p0,p1) whose centreline lies
    within `rad` of segment Q -- i.e. the chord along which P pierces the capsule
    (Q, rad). distance-to-segment is CONVEX along P (P->P(t) affine composed with
    distance to a convex set), so {t : d(t) < rad} is a single interval; found by
    a golden-section min + two deterministic bisections (no RNG, no sampling)."""
    p0 = np.asarray(p0, float); p1 = np.asarray(p1, float)
    d1 = p1 - p0
    L = float(np.linalg.norm(d1))
    if L < 1e-9:
        return 0.0

    def dist(t):
        return _pt_seg_dist(p0 + t * d1, q0, q1)

    gr = (np.sqrt(5.0) - 1.0) / 2.0
    a, b = 0.0, 1.0
    x1 = b - gr * (b - a); x2 = a + gr * (b - a)
    f1 = dist(x1); f2 = dist(x2)
    for _ in range(64):
        if f1 < f2:
            b, x2, f2 = x2, x1, f1
            x1 = b - gr * (b - a); f1 = dist(x1)
        else:
            a, x1, f1 = x1, x2, f2
            x2 = a + gr * (b - a); f2 = dist(x2)
    tstar = 0.5 * (a + b)
    if dist(tstar) >= rad:
        return 0.0
    # left crossing on [0, tstar]
    if dist(0.0) <= rad:
        tlo = 0.0
    else:
        lo, hi = 0.0, tstar
        for _ in range(64):
            mid = 0.5 * (lo + hi)
            if dist(mid) > rad:
                lo = mid
            else:
                hi = mid
        tlo = hi
    # right crossing on [tstar, 1]
    if dist(1.0) <= rad:
        thi = 1.0
    else:
        lo, hi = tstar, 1.0
        for _ in range(64):
            mid = 0.5 * (lo + hi)
            if dist(mid) > rad:
                hi = mid
            else:
                lo = mid
        thi = lo
    return float(max(0.0, (thi - tlo) * L))


# --- member helpers: tip, original placement, candidates, envelope ----------
def _site_length(site) -> float:
    return float(site.strut_length_mm if isinstance(site, StrutSite)
                 else site.stub_length_mm)


def _site_tip(site) -> np.ndarray:
    """The frozen free-end (tip) position of a member at its ORIGINAL placement
    (§4.1): the point the pocket-detection voxel grid buckets."""
    n = _unit(np.asarray(site.n0, float))
    return np.asarray(site.c, float) + n * _site_length(site)


def _original_member(site) -> dict:
    """The member dict at the site's original (polar-0, full-length) placement --
    the corridor a member occupies before it commits (§4.2 density baseline)."""
    if isinstance(site, StrutSite):
        return reaim_foot(site.c, site.n0, site.foot_depth_mm, site.strut_length_mm,
                          site.r_foot, site.r_root, site.owner, site.carried_mm2)
    r_shoot_floor = max(float(site.r_tip), 0.0)
    return reaim_stub(site.c, site.n0, site.contact_depth_mm, site.stub_length_mm,
                      site.r_tip, r_shoot_floor, site.owner, site.carried_mm2)


def _member_kindinfo(site):
    """(kind, rays, guard_fn, moot_fn) for a pocket member -- a strut is judged
    over axis+capsule rays with the foot growth guard, a stub axis-only."""
    if isinstance(site, StrutSite):
        return "strut", "all", _strut_guards_ok, _count_moot
    return "stub", "axis", _resin_guards_ok, _count_moot_axis


def _member_candidates(site, cfg) -> list:
    if isinstance(site, StrutSite):
        return strut_candidates(site, cfg)
    return stub_candidates(site, cfg)


def _corridor_of(member: dict):
    """(q0, q1, rad) of a member's shaft corridor -- endpoints + max capsule
    radius (the corridor half-width used by both the mutual guard and density)."""
    return (np.asarray(member["p0"], float), np.asarray(member["p1"], float),
            float(max(member["r0"], member["r1"])))


# --- pocket detection (§4.1): pinned world-origin voxels + detection halo ----
def voxel_key(pos, vox: float) -> tuple:
    """World-origin-pinned voxel key `(floor(x/vox), floor(y/vox), floor(z/vox))`
    (§4.1). NOT bbox-relative -- amendment F: a bbox grid shifts under any move
    and would make pocket identity unstable."""
    p = np.asarray(pos, float)
    return (int(np.floor(p[0] / vox)), int(np.floor(p[1] / vox)),
            int(np.floor(p[2] / vox)))


def _vox_neighbors6(v):
    """The 6 face neighbours -- the connectivity used to build a pocket's
    voxel component (§4.1: a pocket is a 6-connected component)."""
    x, y, z = v
    return ((x - 1, y, z), (x + 1, y, z), (x, y - 1, z),
            (x, y + 1, z), (x, y, z - 1), (x, y, z + 1))


def _vox_halo26(v):
    """The 26 neighbours (full 3x3x3 minus centre) -- the 1-voxel dilation
    structuring element of the detection halo (§4.1). Face-adjacent occupied
    voxels are already 6-connected into the component; the halo's job is to
    absorb a straggler whose tip voxel only touches the pocket at an edge or
    corner, so a sub-voxel re-aim can never flip its membership."""
    x, y, z = v
    return tuple((x + dx, y + dy, z + dz)
                 for dx in (-1, 0, 1) for dy in (-1, 0, 1) for dz in (-1, 0, 1)
                 if not (dx == 0 and dy == 0 and dz == 0))


@dataclass(frozen=True)
class Pocket:
    label: int                    # deterministic (lexicographic component order)
    voxels: frozenset             # the qualifying 6-connected component's voxels
    member_ids: tuple             # sorted mi of members (incl. detection halo)


def detect_pockets(sites: list, cfg) -> list:
    """Pocket detection (§4.1). Bucket frozen tip positions into world-origin
    `p8_pocket_voxel_mm` voxels; a pocket is a 6-connected occupied-voxel
    component with >= `p8_pocket_min_members` tips. Components are discovered in
    lexicographically-sorted voxel order, so labels are DETERMINISTIC. Membership
    is then tested with a 1-voxel detection halo (dilation): a member whose tip
    voxel lies in a qualifying component OR within its halo joins it (ties broken
    to the lowest label), so a tip on a voxel face does not flip membership under
    a sub-voxel re-aim. Returns the qualifying pockets only; members in none are
    isolated (§4.6)."""
    vox = float(cfg.p8_pocket_voxel_mm)
    tip_vox = {s.site_id: voxel_key(_site_tip(s), vox) for s in sites}
    occ = {}
    for v in tip_vox.values():
        occ[v] = occ.get(v, 0) + 1

    seen = set(); comps = []
    for v in sorted(occ.keys()):                     # lexicographic discovery
        if v in seen:
            continue
        stack = [v]; seen.add(v); comp = []
        while stack:
            u = stack.pop(); comp.append(u)
            for nb in _vox_neighbors6(u):
                if nb in occ and nb not in seen:
                    seen.add(nb); stack.append(nb)
        comps.append(comp)

    qualifying = []                                   # (label, voxelset, dilated)
    for comp in comps:
        tips = sum(occ[v] for v in comp)
        if tips >= int(cfg.p8_pocket_min_members):
            vset = frozenset(comp)
            dilated = set(vset)
            for u in vset:
                dilated.update(_vox_halo26(u))       # 1-voxel dilation halo
            qualifying.append((len(qualifying), vset, dilated))

    # assign members (halo, lowest-label tie-break), in mi order for determinism
    members = {lab: [] for lab, _v, _d in qualifying}
    for sid in sorted(tip_vox):
        tv = tip_vox[sid]
        for lab, _vset, dilated in qualifying:
            if tv in dilated:
                members[lab].append(sid)
                break
    return [Pocket(label=lab, voxels=vset, member_ids=tuple(sorted(members[lab])))
            for lab, vset, _d in qualifying if members[lab]]


# --- conflict graph (§4.2): reachable-envelope AABB broad-phase --------------
def _member_envelope(site, cfg):
    """Reachable envelope E_m (§4.2): the AABB enclosing the capsule of EVERY
    candidate shaft in C_m, padded by the detection halo
    `p8_pocket_corridor_pad_mm`. (lo[3], hi[3])."""
    pad = float(cfg.p8_pocket_corridor_pad_mm)
    lo = np.full(3, np.inf); hi = np.full(3, -np.inf)
    for cand in _member_candidates(site, cfg):
        m = cand.member
        r = float(max(m["r0"], m["r1"]))
        p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
        lo = np.minimum(lo, np.minimum(p0, p1) - r)
        hi = np.maximum(hi, np.maximum(p0, p1) + r)
    return lo - pad, hi + pad


def build_conflict_graph(sites: list, cfg):
    """Conflict graph G (§4.2): one node per pocket member; an undirected edge
    (a,b) iff the reachable envelopes E_a, E_b overlap (SOME candidate of a could
    geometrically interact with SOME candidate of b). One vectorised AABB
    broad-phase over the envelopes (not a scalar O(n^2) probe loop); the envelope
    IS an AABB so the broad-phase test is exact. N(m) is the COMPLETE dependency
    set the §4.4 invalidation rule uses. Returns (env, adjacency, edges) with
    edges as sorted (min mid, max mid) pairs and adjacency mid -> set(neighbours)."""
    ids = sorted(s.site_id for s in sites)
    site_by_id = {s.site_id: s for s in sites}
    env = {i: _member_envelope(site_by_id[i], cfg) for i in ids}
    adj = {i: set() for i in ids}
    edges = []
    if len(ids) >= 2:
        LO = np.array([env[i][0] for i in ids])       # (n,3)
        HI = np.array([env[i][1] for i in ids])
        # overlap[a,b] iff LO_a <= HI_b (all axes) and LO_b <= HI_a (all axes)
        ov = (np.all(LO[:, None, :] <= HI[None, :, :], axis=2)
              & np.all(LO[None, :, :] <= HI[:, None, :], axis=2))
        n = len(ids)
        for ai in range(n):
            for bi in range(ai + 1, n):
                if ov[ai, bi]:
                    a, b = ids[ai], ids[bi]
                    adj[a].add(b); adj[b].add(a)
                    edges.append((a, b))
    return env, adj, tuple(edges)


# --- density penalty (§4.2) feeding the §3.1 score tuple --------------------
def _density_count(sid, cand, corridors, cfg) -> int:
    """density_penalty(cand, m) (§4.2): the integer count of OTHER pocket members
    whose current corridor the candidate shaft of m passes within
    `p8_pocket_corridor_pad_mm` of (surface-to-surface). `corridors` is
    {mid: (q0,q1,rad)} at the CURRENT state (committed geometry if COMMITTED,
    else original placement). Vectorised over the neighbour corridors."""
    others = [c for k, c in corridors.items() if k != sid]
    if not others:
        return 0
    m = cand.member
    p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
    rp = float(max(m["r0"], m["r1"]))
    Q0 = np.array([o[0] for o in others]); Q1 = np.array([o[1] for o in others])
    RQ = np.array([o[2] for o in others])
    d = _seg_seg_dist_batch(p0, p1, Q0, Q1)
    surf = d - rp - RQ                       # surface-to-surface gap
    return int(np.count_nonzero(surf < float(cfg.p8_pocket_corridor_pad_mm)))


def _default_mutual_legal(cand_member: dict, committed_members: list, cfg) -> bool:
    """The mutual-legality guard (§4.3-3-ii / §6.9), default pure-geometry
    implementation: the candidate introduces NO new firm transit into any
    already-COMMITTED neighbour. A firm transit = a shaft pierces the other's
    capsule with an interior chord >= PRINT_FLOOR_CHORD_MM (0.08mm); checked
    BOTH directions (m through k and k through m). Vacuously True when no
    committed neighbour. The engine may inject its own §5-law equivalent."""
    p0, p1, rp = _corridor_of(cand_member)
    for km in committed_members:
        q0, q1, rq = _corridor_of(km)
        if _seg_capsule_chord(p0, p1, q0, q1, rq) >= verify.PRINT_FLOOR_CHORD_MM:
            return False
        if _seg_capsule_chord(q0, q1, p0, p1, rp) >= verify.PRINT_FLOOR_CHORD_MM:
            return False
    return True


# --- pocket-solver result records -------------------------------------------
class PocketStatus(str, Enum):
    UNRESOLVED = "unresolved"
    COMMITTED = "committed"
    FALLEN = "fallen"          # empty/exhausted -> leaves to R2/R3 (S2/S3)


@dataclass
class PocketMemberResult:
    mid: int
    status: PocketStatus
    kind: str                              # "stub" | "strut"
    committed_cand: Optional[Candidate] = None
    score: Optional[tuple] = None
    escalate_rung: Optional[str] = None    # "R2" (stub) | "S2" (strut) when FALLEN
    n_legal_seed: int = 0


@dataclass
class PocketSolveResult:
    label: int
    member_ids: tuple
    members: dict                          # mid -> PocketMemberResult
    commit_order: tuple                    # mids in loop-processing order
    objective_trace: tuple                 # UNRESOLVED count after each iteration
    invalidations: tuple                   # (committed_mid, (dirty_mid, ...)) per commit
    n_iterations: int
    n_committed: int
    n_fallen: int
    edges: tuple


@dataclass
class PocketSolverReport:
    pockets: list                          # PocketSolveResult, in label order
    isolated: dict                         # mid -> R1Result (§4.6 plain ladder)
    dispositions: dict                     # mid -> str


def solve_pocket(members: list, geom, cfg, *,
                 revalidate_fn: Optional[Callable] = None,
                 reroute_fn: Optional[Callable] = None,
                 mutual_legal_fn: Optional[Callable] = None,
                 label: int = 0) -> PocketSolveResult:
    """The atomic-commit loop (§4.3, normative) for ONE pocket's members.

    Batch-first throughout: the seed evaluates the FULL candidate matrix in ONE
    batched kernel query per ray-kind (gather -> batched call -> scatter); every
    dirty re-evaluation is likewise ONE batched query over the dirty members'
    candidates. Per iteration EXACTLY one UNRESOLVED member becomes COMMITTED or
    FALLEN, so the UNRESOLVED count strictly decreases -- the loop runs <=
    |pocket| iterations and halts, with no sweep cap and no oscillation (§4.5).

    Soundness (§4.4): a member commits its lex-min legal candidate that also (i)
    is legal against all committed geometry (surface law + injected revalidate)
    AND (ii) introduces no new firm transit into any COMMITTED neighbour
    (`mutual_legal_fn`). Committed geometry only grows and every addition
    preserves mutual legality, so the committed set is fully mutually legal at
    every step -- the §6.9 zero-NEW-firm-transit gate enforced locally."""
    if mutual_legal_fn is None:
        mutual_legal_fn = _default_mutual_legal
    site_by_id = {s.site_id: s for s in members}
    ids = sorted(site_by_id)
    _env, adj, edges = build_conflict_graph(members, cfg)

    # candidate sets are FIXED grids, computed once (§4.5 determinism).
    cand_map = {i: _member_candidates(site_by_id[i], cfg) for i in ids}
    kind_of = {i: _member_kindinfo(site_by_id[i])[0] for i in ids}
    orig_member = {i: _original_member(site_by_id[i]) for i in ids}
    wfc = {i: float(getattr(site_by_id[i], "frozen_worst_chord_mm", 0.0)) for i in ids}

    status = {i: PocketStatus.UNRESOLVED for i in ids}
    committed_member = {i: None for i in ids}
    committed_cand = {i: None for i in ids}
    score_of = {i: None for i in ids}
    legal_sorted = {i: [] for i in ids}
    n_legal_seed = {i: 0 for i in ids}

    def _corridors():
        """Current corridor of every member: committed geometry if COMMITTED,
        else the original placement (§4.2)."""
        out = {}
        for i in ids:
            m = committed_member[i] if status[i] == PocketStatus.COMMITTED else orig_member[i]
            out[i] = _corridor_of(m)
        return out

    def _batched_eval(eval_ids):
        """ONE batched legality evaluation (grouped by ray-kind) over eval_ids;
        returns {mid: sorted legal [(score, cand), ...]}. density_penalty is
        recomputed against the CURRENT committed corridors (§4.3-5)."""
        corridors = _corridors()

        def density_fn(sid, cand):
            return _density_count(sid, cand, corridors, cfg)

        out = {}
        for kind in ("stub", "strut"):
            grp = [i for i in eval_ids if kind_of[i] == kind]
            if not grp:
                continue
            rays, guard, moot = (("axis", _resin_guards_ok, _count_moot_axis)
                                 if kind == "stub"
                                 else ("all", _strut_guards_ok, _count_moot))
            all_cands = [c for i in grp for c in cand_map[i]]
            sub = {i: site_by_id[i] for i in grp}
            per_legal, _cnt = _batch_legal_sets(
                all_cands, sub, grp, geom, cfg, rays=rays, guard_fn=guard,
                moot_fn=moot, revalidate_fn=revalidate_fn, reroute_fn=reroute_fn,
                pocket_penalty_fn=density_fn)
            out.update(per_legal)
        return out

    # 1. seed -- one batched call over ALL members (§4.3-1).
    seed = _batched_eval(ids)
    for i in ids:
        legal_sorted[i] = seed.get(i, [])
        n_legal_seed[i] = len(legal_sorted[i])

    commit_order = []
    objective_trace = []
    invalidations = []

    # 2-6. the atomic-commit loop.
    while True:
        ready = [i for i in ids
                 if status[i] == PocketStatus.UNRESOLVED and legal_sorted[i]]
        if not ready:
            break
        # commit priority (§4.3-2): worst-firm-chord desc, mid asc -- a total order.
        m = min(ready, key=lambda i: (-wfc[i], i))
        committed_neighbors = [committed_member[k] for k in sorted(adj[m])
                               if status[k] == PocketStatus.COMMITTED]
        # 3. walk lex-min-first to the first candidate passing the mutual guard.
        chosen = None
        for score, cand in legal_sorted[m]:
            if mutual_legal_fn(cand.member, committed_neighbors, cfg):
                chosen = (score, cand)
                break
        if chosen is None:                            # exhausted -> FALLEN
            status[m] = PocketStatus.FALLEN
        else:
            committed_cand[m] = chosen[1]
            committed_member[m] = chosen[1].member
            score_of[m] = chosen[0]
            status[m] = PocketStatus.COMMITTED        # terminal, never re-evaluated
            # 4. FULL dependency-set invalidation: every UNRESOLVED graph
            #    neighbour (amendment F: NOT merely candidate-ray-overlapping).
            dirty = [k for k in sorted(adj[m])
                     if status[k] == PocketStatus.UNRESOLVED]
            invalidations.append((m, tuple(dirty)))
            # 5. re-evaluate ALL dirty members in ONE batched call.
            if dirty:
                re = _batched_eval(dirty)
                for k in dirty:
                    legal_sorted[k] = re.get(k, [])
        commit_order.append(m)
        # monotone objective (§4.5): UNRESOLVED count strictly decreases by one.
        objective_trace.append(sum(1 for i in ids
                                   if status[i] == PocketStatus.UNRESOLVED))

    # any member still UNRESOLVED here has an empty legal set -> honest FALLEN
    # (leaves to SM-STUB R2/R3 resp. SM-STRUT S2/S3).
    for i in ids:
        if status[i] == PocketStatus.UNRESOLVED:
            status[i] = PocketStatus.FALLEN

    out_members = {}
    for i in ids:
        rung = None
        if status[i] == PocketStatus.FALLEN:
            rung = "R2" if kind_of[i] == "stub" else "S2"
        out_members[i] = PocketMemberResult(
            mid=i, status=status[i], kind=kind_of[i],
            committed_cand=committed_cand[i], score=score_of[i],
            escalate_rung=rung, n_legal_seed=n_legal_seed[i])
    n_committed = sum(1 for i in ids if status[i] == PocketStatus.COMMITTED)
    return PocketSolveResult(
        label=label, member_ids=tuple(ids), members=out_members,
        commit_order=tuple(commit_order), objective_trace=tuple(objective_trace),
        invalidations=tuple(invalidations),
        n_iterations=len(commit_order), n_committed=n_committed,
        n_fallen=len(ids) - n_committed, edges=edges)


def run_pocket_solver(sites: list, geom, cfg,
                      revalidate_fn: Optional[Callable] = None,
                      reroute_fn: Optional[Callable] = None,
                      mutual_legal_fn: Optional[Callable] = None) -> PocketSolverReport:
    """The atomic-commit pocket solver with full dependency-set invalidation
    (§4) -- conflict-graph-driven, monotone resolved-count objective, terminates
    in <= |pocket| commits, bit-deterministic (§4.5).

    Detects pockets among `sites` (§4.1, world-origin voxels + detection halo),
    solves each pocket's members with `solve_pocket` (the atomic-commit loop),
    and routes members in NO pocket component through the plain per-site ladder
    (§4.6: `density_penalty` is 0 for them, so their §3.1 score tuple reduces
    exactly to the sparse ordering -- the interaction machinery is pocket-only,
    sparse sites pay nothing). Returns a `PocketSolverReport`."""
    site_by_id = {s.site_id: s for s in sites}
    pockets = detect_pockets(sites, cfg)
    pocketed = set()
    solved = []
    for pk in pockets:
        pmembers = [site_by_id[i] for i in pk.member_ids]
        solved.append(solve_pocket(pmembers, geom, cfg, revalidate_fn=revalidate_fn,
                                   reroute_fn=reroute_fn, mutual_legal_fn=mutual_legal_fn,
                                   label=pk.label))
        pocketed.update(pk.member_ids)

    # §4.6 isolated members: plain per-site ladder, density 0 (pocket_penalty None).
    iso_ids = [s.site_id for s in sites if s.site_id not in pocketed]
    iso_stubs = [site_by_id[i] for i in iso_ids if not isinstance(site_by_id[i], StrutSite)]
    iso_struts = [site_by_id[i] for i in iso_ids if isinstance(site_by_id[i], StrutSite)]
    isolated = {}
    if iso_stubs:
        isolated.update(evaluate_r1(iso_stubs, geom, cfg, revalidate_fn=revalidate_fn,
                                    reroute_fn=reroute_fn))
    if iso_struts:
        isolated.update(evaluate_s1(iso_struts, geom, cfg, revalidate_fn=revalidate_fn,
                                    reroute_fn=reroute_fn))

    dispositions = {}
    for res in solved:
        for mid, mr in res.members.items():
            dispositions[mid] = ("POCKET_COMMITTED" if mr.status == PocketStatus.COMMITTED
                                 else "POCKET_FALLEN_" + (mr.escalate_rung or "R2"))
    for mid, r in isolated.items():
        dispositions[mid] = "ISOLATED_RESOLVED" if r.winner is not None else "ISOLATED_NEEDS_R2"
    return PocketSolverReport(pockets=solved, isolated=isolated, dispositions=dispositions)
