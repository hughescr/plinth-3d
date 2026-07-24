"""Phase-8 ORCHESTRATION DRIVER -- the end-to-end integration that was MISSING.

SPEC: docs/specs/SPEC-P8-RELOCATE.md (the amended DRAFT at HEAD is the law).

`phase8.py` implements every component STATE MACHINE (SM-STUB R1 tip re-aim,
SM-STRUT S1 foot re-aim, SM-BALL B1 axial slide, the atomic-commit pocket solver
§4) and `p8_accept.py` implements the amendment-G acceptance gates -- both are
committed and unit-green. What was MISSING, and what THIS module supplies, is the
DRIVER that wires them to the engine's own forest + gate:

  1. SITE CONSTRUCTION (batch-first, §3.0 / §5.3): given the engine's Forest +
     kernel surface + config, build the FROZEN defect snapshot and the concrete
     `phase8.StubSite` / `phase8.StrutSite` / `phase8.BallSite` objects FROM THE
     ENGINE'S OWN GATE OUTPUT -- non-exempt transit members via verify's
     SCAR-AREA-CAP + rule-4 + PRINT-FLOOR classification (`verify.clearance_report`),
     the REAL-graded quarantine pool (re-graded under the ratified law, §3.3), and
     the true joint-ball breaches (`verify.ball_breach_report`, source="ball").
     The census target-selection authority (§5.3 Layer 2, conservative) is honored
     by UNIONing the census-only non-exempt members (the mi=445-class the kernel's
     robust tip-anchor merge exempts but the census counts) into the target set.

  2. PHASE ORCHESTRATION (§3.0 normative order): predicate-move quarantine
     re-grade -> Phase-1 stubs (Q-REAL union class-T stubs, one ladder) -> Phase-2
     interior struts (SM-STRUT) -> Phase-3 ball breaches (SM-BALL, LAST because a
     joint's legal position depends on the FINAL shaft geometry). Pocket clusters
     (§4.1 world-origin-pinned voxels + halo) route through the batched pocket
     solver; isolated members through the plain per-site ladder.

  3. SPLICE (deterministic, transactional): accepted relocations are written back
     into `tree.Forest` under its checkpoint/revalidate protocol; a spliced member
     that fails the §5-law revalidate is reverted (never strands the contact), so
     the forest handed back is never worse than the one handed in.

  4. IDEMPOTENT BY CONSTRUCTION: an empty frozen snapshot enters no ladder and
     splices nothing, so a second pass over already-fixed geometry emits
     byte-identical geometry (the §6.10 gate). Determinism is inherited verbatim
     from the component machinery: no RNG, no wall-clock, stable sorts on explicit
     keys with the candidate index as final tiebreak.

BATCH-FIRST throughout (the binding house rule): site construction gathers every
member into ONE `verify.clearance_report` / `ball_breach_report` call; candidate
legality is the component machinery's own gather -> one batched kernel call ->
scatter. There is no per-site scalar kernel loop anywhere in this module.
"""
from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Optional

import numpy as np

from . import phase8, verify, joints

# Q-REAL quarantine site IDs live in a DISJOINT namespace from class-T live
# member indices (a quarantine `stub_member` is a `forest.members` index that
# numerically overlaps the live collect_members indices class-T sites use). A
# large offset keeps the two populations' site_ids unique so the ladder / pocket
# solver never conflates a Q-REAL stub with a class-T stub that happens to share
# the integer. The offset is stripped back off for the forest splice + reporting.
Q_SITE_OFFSET = 1_000_000


# ---------------------------------------------------------------------------
# live-member <-> forest-member index map.
# ---------------------------------------------------------------------------
# The engine's gate + the census address a member by its index `mi` in the LIVE
# member list (`engine.live_member_lists` -> `joints.collect_members`, which
# excludes honestly-unrouted stubs and rewire-dead members and is 1:1 with the
# surviving `forest.members`). To SPLICE a relocation back we must map that live
# index to the underlying `forest.members` index; this builds the bijection
# exactly as `live_member_lists` derives the live set, so the two never drift.
def live_forest_index_map(forest, unrouted) -> list:
    """Return `live_fi` where `members[i]` (the collect_members live list) is
    `forest.members[live_fi[i]]`. Mirrors `engine.live_member_lists`' filter
    (drop unrouted stubs + `forest.dead`) so the live ordering is identical."""
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted
                         if i in forest.terminal_of}
    dead = getattr(forest, "dead", set())
    return [fi for fi in range(len(forest.members))
            if fi not in unrouted_stub_idx and fi not in dead]


# ---------------------------------------------------------------------------
# contact lookup (owner -> contact dict).
# ---------------------------------------------------------------------------
def _contact_of(member: dict, contacts) -> Optional[dict]:
    """The contact dict a member owns, or None. Every class-T / Q-REAL target is
    a `('contact', ci)`-owned member (verified on the archived geometry: all 143
    non-exempt + all 80 quarantine sites own a contact); `ci` indexes the full
    contact list the engine threads through `carried`/`tip_r`."""
    owner = member.get("owner")
    if isinstance(owner, (tuple, list)) and len(owner) == 2 and owner[0] == "contact":
        ci = int(owner[1])
        if 0 <= ci < len(contacts):
            return contacts[ci]
    return None


def _carried_of(member: dict, contacts, carried) -> float:
    owner = member.get("owner")
    if (carried is not None and isinstance(owner, (tuple, list)) and len(owner) == 2
            and owner[0] == "contact"):
        ci = int(owner[1])
        if 0 <= ci < len(carried):
            return float(carried[ci])
    c = _contact_of(member, contacts)
    return float(c.get("area_mm2", 0.0)) if c else 0.0


# ---------------------------------------------------------------------------
# per-member site geometry (§3.1 / §3.1s).
# ---------------------------------------------------------------------------
def stub_site_from_member(mi: int, member: dict, contact: Optional[dict], cfg,
                          carried_mm2: float, worst_chord: float) -> phase8.StubSite:
    """Build an SM-STUB `StubSite` from a placed contact stub (§3.1). The contact
    SURFACE POINT `c` and its outward normal `n0` are the demanded (invariant)
    contact -- taken from the owning contact when available (the demanded anchor),
    else re-derived from the member geometry (`c = p0 + axis*contact_depth`,
    the surface point delta above the embedded tip p0). `n0` at polar 0 recovers
    the original contact normal, so the re-aim cone is centered on it."""
    p0 = np.asarray(member["p0"], float); p1 = np.asarray(member["p1"], float)
    axis = phase8._unit(p1 - p0)
    delta = float(cfg.contact_depth_mm)
    if contact is not None:
        c = np.asarray(contact["position"], float)
        n0 = phase8._unit(np.asarray(contact["normal"], float))
    else:
        c = p0 + axis * delta
        n0 = axis
    stub_len = float(np.linalg.norm(p1 - c))
    return phase8.StubSite(
        site_id=int(mi), c=c, n0=n0, r_tip=float(member["r0"]),
        stub_length_mm=stub_len, contact_depth_mm=delta,
        carried_mm2=float(carried_mm2), owner=member.get("owner"),
        orig_member=dict(member), frozen_worst_chord_mm=float(worst_chord))


def strut_site_from_member(mi: int, member: dict, cfg, carried_mm2: float,
                           worst_chord: float) -> phase8.StrutSite:
    """Build an SM-STRUT `StrutSite` from an interior strut (§3.1s). The FOOT is
    the member's designed embed end (p1 for these struts); the re-aim
    canonicalizes the embedded foot as the candidate's p0, so the site records
    the foot surface point `c` and outward foot normal `n0 = unit(free - foot)`.
    `orig_member` carries the ORIGINAL embed_end so the splice can undo the
    canonicalization (§ splice)."""
    p0 = np.asarray(member["p0"], float); p1 = np.asarray(member["p1"], float)
    embed = member.get("embed_end")
    if embed == "p1":
        foot = p1; free = p0; r_foot = float(member["r1"]); r_root = float(member["r0"])
    else:                                   # embed p0 (defensive; not seen in T-struts)
        foot = p0; free = p1; r_foot = float(member["r0"]); r_root = float(member["r1"])
    n0 = phase8._unit(free - foot)
    delta = float(cfg.contact_depth_mm)
    c = foot + n0 * delta
    strut_len = float(np.linalg.norm(free - c))
    return phase8.StrutSite(
        site_id=int(mi), c=c, n0=n0, r_foot=r_foot, r_root=r_root,
        strut_length_mm=strut_len, foot_depth_mm=delta,
        carried_mm2=float(carried_mm2), owner=member.get("owner"),
        orig_member=dict(member), orig_strut_length_mm=strut_len,
        frozen_worst_chord_mm=float(worst_chord))


def quarantine_stub_site(qs: dict, cfg, carried_mm2: float) -> phase8.StubSite:
    """Build a `StubSite` for a Q-REAL condemned stub from its quarantine record
    (§3.3). The record carries the demanded contact (`contact.position/normal`)
    and the raw stub geometry -- the SAME shape a live stub yields, so Q-REAL and
    class-T stubs enter ONE ladder (§3.3: 'one predicate, one ladder, two entry
    points')."""
    member = dict(p0=np.asarray(qs["p0"], float), p1=np.asarray(qs["p1"], float),
                  r0=float(qs["r0"]), r1=float(qs["r1"]), kind=qs["kind"],
                  owner=tuple(qs["owner"]) if isinstance(qs["owner"], (list, tuple)) else qs["owner"],
                  embed_end=qs.get("embed_end"))
    contact = qs.get("contact")
    return stub_site_from_member(int(qs["stub_member"]), member, contact, cfg,
                                 carried_mm2, worst_chord=0.0)


def quarantine_sites_from_forest(forest, contacts) -> list:
    """Reconstruct the A0b quarantine-site records the driver re-grades (§3.3)
    from a live `tree.Forest`: `forest.quarantined` is a set of `forest.members`
    indices (the condemned stubs stay in the member list, just excluded from the
    live/emitted set), so each record is that member's raw geometry plus its
    demanded contact. Matches the `quarantine_sites` schema the archived staged
    runs pickle, so the engine path and the archived-pickle path build identical
    sites. Sorted by member index (deterministic)."""
    out = []
    for qidx in sorted(getattr(forest, "quarantined", ())):
        if not (0 <= qidx < len(forest.members)):
            continue
        m = forest.members[qidx]
        owner = m.owner
        contact = None
        if (isinstance(owner, (tuple, list)) and len(owner) == 2
                and owner[0] == "contact" and 0 <= int(owner[1]) < len(contacts)):
            c = contacts[int(owner[1])]
            contact = dict(position=np.asarray(c["position"], float).tolist(),
                           normal=np.asarray(c["normal"], float).tolist(),
                           area_mm2=float(c.get("area_mm2", 0.0)))
        out.append(dict(stub_member=int(qidx), p0=np.asarray(m.p0, float),
                        p1=np.asarray(m.p1, float), r0=float(m.r0), r1=float(m.r1),
                        kind=m.kind, owner=owner, embed_end=m.embed_end,
                        contact=contact))
    return out


def ball_sites_from_breaches(breaches: list, members: list, cfg) -> list:
    """Build SM-BALL `BallSite`s from the true joint-ball breaches
    (`source="ball"`, §3.2). `root_dir` is the owning member's axis oriented
    toward the raft (lower z on a bottom-plate print) -- B1 slides root-ward
    (rank 0) then tip-ward (rank 1); the ball radius is INVARIANT (never shrunk).
    The owning member is the collect_members member whose endpoint coincides with
    the ball center (fattest wins ties), found by a single vectorized endpoint
    distance scan (batch-first; no per-ball mesh query)."""
    members = joints.collect_members(members)
    if not members:
        return []
    ep = []                                    # (idx, point, axis_unit)
    for m in members:
        p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
        ax = phase8._unit(p1 - p0)
        ep.append((p0, ax, float(max(m["r0"], m["r1"]))))
        ep.append((p1, ax, float(max(m["r0"], m["r1"]))))
    EP = np.array([e[0] for e in ep])          # (2M,3)
    out = []
    for bi, b in enumerate(breaches):
        center = np.asarray(b["center"], float)
        r = float(b["r"])
        d = np.linalg.norm(EP - center[None, :], axis=1)
        near = np.nonzero(d <= max(0.05, 0.5 * r))[0]
        if len(near):
            # fattest owning member's axis among coincident endpoints (stable).
            k = int(near[np.argmax([ep[j][2] for j in near])])
            axis = ep[k][1]; anchor = ep[k][0]
        else:
            k = int(np.argmin(d)); axis = ep[k][1]; anchor = ep[k][0]
        # orient axis root-ward: point toward the endpoint of lower z.
        p_other = anchor + axis  # arbitrary downstream point along axis
        root_dir = axis if p_other[2] <= anchor[2] else -axis
        owner = b.get("owner")
        out.append(phase8.BallSite(
            ball_id=int(bi), center=center, r=r, root_dir=root_dir,
            owner=owner, criticality=1.5, orphan_if_dropped=False))
    return out


# ---------------------------------------------------------------------------
# gate classification (batch-first) -- the ENGINE'S OWN gate output.
# ---------------------------------------------------------------------------
@dataclass
class GateClassification:
    firm_transit: frozenset          # engine non-exempt firm-transit member idxs
    moot: frozenset                  # engine moot member idxs (report-only)
    worst_chord: dict                # mi -> worst counted crossing chord (mm)
    ball_breaches: tuple             # source="ball" breach dicts (15 on treant)


def classify_gate(mesh, members, balls, geom, cfg) -> GateClassification:
    """Run the ENGINE gate over the live member + ball lists in ONE batched pass
    and split the output into the Phase-8 defect classes (§3): firm-transit
    (non-exempt hard-gate), moot (report-only), and true joint-ball breaches.
    `worst_chord` (per member, from `transit_chords_mm`) is the frozen §4.3
    pocket commit-priority key. Purely read-only -- classification never mutates
    the forest or the members."""
    moot_out = []
    chords_out = {}
    clr = verify.clearance_report(mesh, members, cfg, geom=geom,
                                  moot_transit_out=moot_out,
                                  transit_chords_out=chords_out)
    firm = frozenset(v["member_idx"] for v in clr
                     if v.get("source", "transit") == "transit")
    moot = frozenset(v["member_idx"] for v in moot_out)
    worst = {int(mi): (max(ch) if ch else 0.0) for mi, ch in chords_out.items()}
    breaches = tuple(v for v in verify.ball_breach_report(geom, members, balls, cfg)
                     if v.get("source") == "ball")
    return GateClassification(firm_transit=firm, moot=moot, worst_chord=worst,
                              ball_breaches=breaches)


def regrade_quarantine(quarantine_sites, geom, cfg):
    """Re-grade the A0b quarantine pool under the ratified law (§3.3): a
    condemned stub is Q-REAL iff, placed at its demanded position, it is genuinely
    illegal (a firm OR moot rule-4 crossing) under SCAR-AREA-CAP + PRINT-FLOOR;
    Q-ARTIFACT iff it is actually LEGAL (a false condemnation -- a buried deep
    plug the old predicate wrongly quarantined, §5.2). BATCH-FIRST: one batched
    interval + SCAR call over the whole pool. Returns `(real_ids, artifact_ids)`
    (sorted, the stub_member id of each)."""
    if not quarantine_sites:
        return [], []
    qmembers = []
    qids = []
    for qs in quarantine_sites:
        qmembers.append(dict(
            p0=np.asarray(qs["p0"], float), p1=np.asarray(qs["p1"], float),
            r0=float(qs["r0"]), r1=float(qs["r1"]), kind=qs["kind"],
            owner=tuple(qs["owner"]) if isinstance(qs["owner"], (list, tuple)) else qs["owner"],
            embed_end=qs.get("embed_end")))
        qids.append(int(qs["stub_member"]))
    mis = list(range(len(qmembers)))
    # stubs classify AXIS-only; a strut-kind quarantine (none on treant) all-rays.
    ivmap = verify._batch_axis_intervals(geom, qmembers, mis, rays="axis")
    verd = verify._embed_scar_verdicts(geom, qmembers, mis, ivmap, cfg)
    real, artifact = [], []
    for j, m in enumerate(qmembers):
        ne, mo = verify._transit_classify(ivmap.get(j, []), m, verd.get(j))
        (real if (ne or mo) else artifact).append(qids[j])
    return sorted(real), sorted(artifact)


# ---------------------------------------------------------------------------
# defect snapshot (§3.0) + full site construction.
# ---------------------------------------------------------------------------
@dataclass
class DriverInput:
    snapshot: phase8.DefectSnapshot
    stub_sites: list                 # class-T stubs union Q-REAL (Phase-1 pop)
    strut_sites: list                # class-T interior struts (Phase-2 pop)
    ball_sites: list                 # class-B breaches (Phase-3 pop)
    live_fi: list                    # live index -> forest member index
    site_forest_idx: dict            # site_id (live mi) -> forest member index
    gate: GateClassification


def build_snapshot(gate: GateClassification, real_ids, artifact_ids,
                   census_authority=frozenset()) -> phase8.DefectSnapshot:
    """Assemble the frozen `phase8.DefectSnapshot` (§3.0) from the engine gate +
    the quarantine re-grade + the census target-selection authority.

    non_exempt = engine firm-transit UNION `census_authority` (the census-only
    non-exempt members the kernel exempts via its robust tip-anchor merge -- the
    mi=445-class, §5.3 Layer 2 conservative ruling: the census counts it, so
    relocation treats it as a target) MINUS the engine moot set. `moot` is the
    engine's own report-only moot bucket (48 on the archived treant; the census's
    50 additionally counts the 474/575-class the kernel fully exempts, but moot
    members are NEVER remediated so the difference is immaterial to targeting and
    the honest engine reading is kept). The partition dict is reported for
    auditability. `census_authority` is the extra NON-EXEMPT set only (the caller
    derives it from the census non-exempt list)."""
    census_authority = frozenset(int(x) for x in census_authority)
    non_exempt = (gate.firm_transit | census_authority) - gate.moot
    non_exempt = tuple(sorted(non_exempt))
    moot = tuple(sorted(gate.moot))
    return phase8.DefectSnapshot(
        non_exempt=non_exempt, moot=moot,
        quarantine_real=tuple(sorted(real_ids)),
        quarantine_artifact=tuple(sorted(artifact_ids)),
        ball_breaches=tuple(range(len(gate.ball_breaches))),
        partition=dict(n_firm_engine=len(gate.firm_transit),
                       n_census_only=len(census_authority),
                       n_moot=len(gate.moot)),
        provenance=dict(source="engine-gate+quarantine-regrade+census-authority"))


def construct_sites(forest, members, balls, contacts, carried, cfg, geom, mesh,
                    unrouted=(), quarantine_sites=None,
                    census_authority=frozenset()) -> DriverInput:
    """Build the full Phase-8 driver input from the engine's OWN gate output
    (task step 1). BATCH-FIRST: one `classify_gate` pass (batched clearance +
    ball-breach reports) drives every class; per-site geometry is pure algebra on
    the already-gathered member dicts (no kernel call in the site loop).

    The site_id of every stub/strut is its live member index `mi` (the stable
    §3.0 site ID = the census/gate integer); quarantine stubs keep their own
    `stub_member` id (a disjoint index space -- a quarantined stub is never in the
    live list). Returns a `DriverInput`."""
    members = joints.collect_members(members)
    gate = classify_gate(mesh, members, balls, geom, cfg)
    real_ids, artifact_ids = regrade_quarantine(quarantine_sites, geom, cfg)
    snapshot = build_snapshot(gate, real_ids, artifact_ids, census_authority)

    live_fi = live_forest_index_map(forest, unrouted)

    stub_sites, strut_sites = [], []
    site_forest_idx = {}
    for mi in snapshot.non_exempt:
        m = members[mi]
        contact = _contact_of(m, contacts)
        carried_mm2 = _carried_of(m, contacts, carried)
        wc = gate.worst_chord.get(mi, 0.0)
        if m["kind"] == "stub":
            stub_sites.append(stub_site_from_member(mi, m, contact, cfg, carried_mm2, wc))
        else:
            strut_sites.append(strut_site_from_member(mi, m, cfg, carried_mm2, wc))
        if mi < len(live_fi):
            site_forest_idx[mi] = live_fi[mi]

    # Q-REAL stubs join the Phase-1 population (same ladder, §3.3). Their id space
    # is the quarantine stub_member (forest member idx of the condemned stub);
    # recorded so the splice targets the right forest member.
    q_by_id = {int(qs["stub_member"]): qs for qs in (quarantine_sites or [])}
    for qid in snapshot.quarantine_real:
        qs = q_by_id.get(qid)
        if qs is None:
            continue
        carried_mm2 = float((qs.get("contact") or {}).get("area_mm2", 0.0))
        site = quarantine_stub_site(qs, cfg, carried_mm2)
        site.site_id = Q_SITE_OFFSET + int(qid)   # disjoint namespace (see top)
        stub_sites.append(site)
        site_forest_idx[Q_SITE_OFFSET + int(qid)] = int(qid)  # -> forest member idx

    ball_sites = ball_sites_from_breaches(list(gate.ball_breaches), members, cfg)

    return DriverInput(snapshot=snapshot, stub_sites=stub_sites,
                       strut_sites=strut_sites, ball_sites=ball_sites,
                       live_fi=live_fi, site_forest_idx=site_forest_idx, gate=gate)


# ---------------------------------------------------------------------------
# splice: write an accepted re-aim back into the forest (transactional).
# ---------------------------------------------------------------------------
def _apply_member_geometry(fmember, cand_member: dict, orig_embed_end):
    """Write a winner candidate's canonical (embed-at-p0) geometry onto a live
    `tree.Member`, undoing the re-aim canonicalization when the original member
    embedded at p1 (SM-STRUT feet). The write goes through Member.__setattr__ so
    the certificate is voided and the forest dirty-index is touched -- the caller
    then revalidates."""
    p0 = np.asarray(cand_member["p0"], float)
    p1 = np.asarray(cand_member["p1"], float)
    r0 = float(cand_member["r0"]); r1 = float(cand_member["r1"])
    if orig_embed_end == "p1":
        # candidate embeds at p0; original embeds at p1 -> swap ends back.
        fmember.p0 = p1; fmember.p1 = p0
        fmember.r0 = r1; fmember.r1 = r0
    else:
        fmember.p0 = p0; fmember.p1 = p1
        fmember.r0 = r0; fmember.r1 = r1


def splice_reaims(forest, geom, cfg, accepted: list):
    """Splice a batch of accepted re-aims into the forest under its
    checkpoint/revalidate protocol (deterministic, transactional). `accepted` is
    a list of `(forest_idx, cand_member, orig_embed_end)`. Applies every winner,
    runs ONE batched `forest.revalidate` (the §5-law re-check), and REVERTS any
    member that comes back a violation to its pre-splice geometry (never strands
    the contact -- a candidate that fails the collision law is dropped, its site
    reported unresolved). Returns `(n_committed, reverted_forest_idxs)`."""
    if not accepted:
        return 0, []
    saved = {}
    for fi, cand_member, orig_embed in accepted:
        if fi is None or not (0 <= fi < len(forest.members)):
            continue
        fm = forest.members[fi]
        saved[fi] = fm._snapshot()
        _apply_member_geometry(fm, cand_member, orig_embed)
    if not saved:
        return 0, []
    violations = forest.revalidate(geom, cfg)
    reverted = []
    bad = {v["member_idx"] for v in violations if v["member_idx"] in saved}
    for fi in sorted(bad):
        forest.members[fi]._restore(saved[fi])
        reverted.append(fi)
    if reverted:
        forest.revalidate(geom, cfg)   # re-certify the restored (already-legal) members
    return len(saved) - len(reverted), reverted


# ---------------------------------------------------------------------------
# ball-slide splice (Phase 3): translate the joint node.
# ---------------------------------------------------------------------------
def _translate_joint(forest, center, delta, tol):
    """Translate every live member endpoint coincident with `center` by `delta`
    (a joint slide keeps the members meeting at the node coincident, so
    connectivity is preserved). Returns the list of `(forest_idx, snapshot)` it
    touched so the caller can revert."""
    saved = []
    for fi, fm in enumerate(forest.members):
        if fi in getattr(forest, "dead", set()):
            continue
        moved = False
        snap = fm._snapshot()
        if np.linalg.norm(np.asarray(fm.p0, float) - center) <= tol:
            fm.p0 = np.asarray(fm.p0, float) + delta; moved = True
        if np.linalg.norm(np.asarray(fm.p1, float) - center) <= tol:
            fm.p1 = np.asarray(fm.p1, float) + delta; moved = True
        if moved:
            saved.append((fi, snap))
    return saved


# ---------------------------------------------------------------------------
# the driver.
# ---------------------------------------------------------------------------
@dataclass
class Phase8Report:
    snapshot_summary: dict
    n_stub_sites: int
    n_strut_sites: int
    n_ball_sites: int
    stub_resolved: int
    stub_unresolved: int
    qreal_resolved: int
    qreal_unresolved: int
    strut_resolved: int
    strut_unresolved: int
    ball_resolved: int
    ball_unresolved: int
    reverted_splices: tuple
    unresolved_items: tuple          # (class, site_id, rung) honest-failure trace
    pocket_labels: tuple
    dt_s: float
    entered_ladder: bool             # False => idempotent no-op (empty snapshot)
    # ---- fix-round F1 convergence + F6 instrumentation -----------------------
    sweeps: int = 0                  # convergence sweeps run (F1); 0 => no-op
    converged: bool = True           # True => final sweep committed 0 splices
    committed_by_sweep: tuple = ()   # per-sweep splice-commit count (fixed-pt trace)
    provenance_ledger: tuple = ()    # (sweep, cls, site_id, forest_idx, disp, rung)
    moot_member_ids: tuple = ()      # census/gate moot member idxs (report-only, F6)


def _stub_revalidate_fn(site_by_id):
    """No injected support-support revalidate at ladder-selection time: the
    §5-law model-surface legality (SCAR-AREA-CAP + rule-4 + PRINT-FLOOR) is
    already enforced inside `phase8._batch_legal_sets`, and support/support
    collision is caught deterministically at SPLICE time by `forest.revalidate`
    (a spliced member that collides is reverted). Stubs carry no swept-distance
    gate (the gate skips them), so the model law IS their full legality."""
    return None


@dataclass
class _SweepResult:
    """One Phase 1->2->3 relocation sweep's outcome (fix-round F1). `n_committed`
    is the number of splices that survived revalidate this sweep -- the geometry-
    change count that drives the fixed-point / idempotence termination test."""
    stub_resolved: int
    stub_unresolved: int
    qreal_resolved: int
    qreal_unresolved: int
    strut_resolved: int
    strut_unresolved: int
    ball_resolved: int
    ball_unresolved: int
    reverted: list
    pocket_labels: list
    unresolved_items: list
    ledger_rows: list
    n_committed: int


def _relocation_sweep(di: DriverInput, forest, geom, cfg) -> _SweepResult:
    """Run ONE Phase 1 (stubs+Q-REAL) -> Phase 2 (struts) -> Phase 3 (balls) pass
    over the FROZEN sites in `di`, splicing accepted re-aims/slides into `forest`
    under the transactional revalidate protocol. Pure single-sweep mechanics --
    the caller (`run_phase8`) re-derives `di` from the mutated forest for the next
    sweep, which is the F1 convergence: the ladder runs on its OWN output until a
    sweep changes nothing (`n_committed == 0`, the true-no-op fixed point).

    Records a per-site provenance row (F6): (cls, site_id, forest_idx, disposition,
    rung) so the 34-member-class collateral is attributable from artifacts."""
    unresolved_items: list = []
    reverted: list = []
    pocket_labels: list = []
    ledger_rows: list = []
    n_committed = 0

    # ---- Phase 1: stubs (class-T stubs UNION Q-REAL), pocket solver + isolated.
    stub_resolved = stub_unresolved = 0
    qreal_resolved = qreal_unresolved = 0

    def _dispose_stub(mid, resolved, rung):
        nonlocal stub_resolved, stub_unresolved, qreal_resolved, qreal_unresolved
        if mid >= Q_SITE_OFFSET:
            if resolved:
                qreal_resolved += 1
            else:
                qreal_unresolved += 1
                unresolved_items.append(("STUB", mid - Q_SITE_OFFSET, rung))
            return None                       # Q-REAL: never spliced (see run_phase8)
        if resolved:
            stub_resolved += 1
            return di.site_forest_idx.get(mid)
        stub_unresolved += 1
        unresolved_items.append(("STUB", mid, rung))
        return None

    if di.stub_sites:
        report = phase8.run_pocket_solver(di.stub_sites, geom, cfg)
        pocket_labels = [pk.label for pk in report.pockets]
        accepted = []
        for pk in report.pockets:
            for mid, mr in pk.members.items():
                ok = mr.status == phase8.PocketStatus.COMMITTED and bool(mr.committed_cand)
                fi = _dispose_stub(mid, ok, mr.escalate_rung or "R2")
                if fi is not None:
                    accepted.append((fi, mr.committed_cand.member, _orig_embed(forest, fi)))
                ledger_rows.append(("STUB", mid, fi, "POCKET",
                                    mr.escalate_rung or ("R1" if ok else "R2")))
        for mid, r1res in report.isolated.items():
            ok = r1res.winner is not None
            fi = _dispose_stub(mid, ok, "R2")
            if fi is not None:
                accepted.append((fi, r1res.winner.member, _orig_embed(forest, fi)))
            ledger_rows.append(("STUB", mid, fi, "ISOLATED", "R1" if ok else "R2"))
        n_ok, rv = splice_reaims(forest, geom, cfg, accepted)
        n_committed += n_ok
        reverted.extend(("STUB", fi) for fi in rv)
        stub_resolved -= len(rv); stub_unresolved += len(rv)
        for fi in rv:
            ledger_rows.append(("STUB", None, fi, "REVERTED", "splice"))

    # ---- Phase 2: interior struts (SM-STRUT S1), spliced AFTER stubs.
    strut_resolved = strut_unresolved = 0
    if di.strut_sites:
        sres = phase8.run_phase2_struts(di.strut_sites, geom, cfg)
        accepted = []
        for sid, sr in sres.items():
            if sr.rung == "S1" and sr.winner is not None:
                fi = di.site_forest_idx.get(sid)
                accepted.append((fi, sr.winner.member, _orig_embed(forest, fi)))
                strut_resolved += 1
                ledger_rows.append(("STRUT", sid, fi, "S1", "S1"))
            else:
                strut_unresolved += 1
                unresolved_items.append(("STRUT", sid, sr.rung))
                ledger_rows.append(("STRUT", sid, None, "UNRESOLVED", sr.rung))
        n_ok, rv = splice_reaims(forest, geom, cfg, accepted)
        n_committed += n_ok
        reverted.extend(("STRUT", fi) for fi in rv)
        strut_resolved -= len(rv); strut_unresolved += len(rv)
        for fi in rv:
            ledger_rows.append(("STRUT", None, fi, "REVERTED", "splice"))

    # ---- Phase 3: ball breaches (SM-BALL B1), LAST -- reads FINAL shaft geometry.
    ball_resolved = ball_unresolved = 0
    if di.ball_sites:
        def _slide_revalidate(site, sl):
            delta = np.asarray(sl.center, float) - np.asarray(site.center, float)
            saved = _translate_joint(forest, np.asarray(site.center, float), delta,
                                     tol=max(0.05, 0.5 * site.r))
            if not saved:
                return False
            viol = forest.revalidate(geom, cfg)
            ok = not any(v["member_idx"] in {fi for fi, _s in saved} for v in viol)
            for fi, snapm in saved:              # trial only -- always undo here
                forest.members[fi]._restore(snapm)
            forest.revalidate(geom, cfg)
            return ok
        bres = phase8.run_phase3_balls(di.ball_sites, geom, cfg,
                                       slide_revalidate_fn=_slide_revalidate)
        for bid, br in bres.items():
            site = next((s for s in di.ball_sites if s.ball_id == bid), None)
            if br.rung == "B1" and br.slide is not None and site is not None:
                delta = np.asarray(br.slide.center, float) - np.asarray(site.center, float)
                saved = _translate_joint(forest, np.asarray(site.center, float), delta,
                                         tol=max(0.05, 0.5 * site.r))
                viol = forest.revalidate(geom, cfg)
                touched = {fi for fi, _s in saved}
                if any(v["member_idx"] in touched for v in viol):
                    for fi, snapm in saved:
                        forest.members[fi]._restore(snapm)
                    forest.revalidate(geom, cfg)
                    ball_unresolved += 1
                    unresolved_items.append(("BALL", bid, "B1-reverted"))
                    ledger_rows.append(("BALL", bid, None, "REVERTED", "B1"))
                else:
                    ball_resolved += 1
                    n_committed += len(touched)
                    ledger_rows.append(("BALL", bid, None, "B1", "B1"))
            else:
                ball_unresolved += 1
                unresolved_items.append(("BALL", bid, br.rung))
                ledger_rows.append(("BALL", bid, None, "UNRESOLVED", br.rung))

    return _SweepResult(
        stub_resolved=stub_resolved, stub_unresolved=stub_unresolved,
        qreal_resolved=qreal_resolved, qreal_unresolved=qreal_unresolved,
        strut_resolved=strut_resolved, strut_unresolved=strut_unresolved,
        ball_resolved=ball_resolved, ball_unresolved=ball_unresolved,
        reverted=reverted, pocket_labels=pocket_labels,
        unresolved_items=unresolved_items, ledger_rows=ledger_rows,
        n_committed=n_committed)


def run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z, geom,
               cfg, mesh, unrouted=(), quarantine_sites=None,
               census_authority=frozenset(), log=None,
               member_list_fn=None) -> Phase8Report:
    """The Phase-8 orchestration driver (task step 1-2-3): construct sites from
    the engine gate, run the state machines in the §3.0 normative order, and
    splice accepted relocations back into `forest`. Idempotent by construction --
    an empty frozen snapshot enters no ladder and mutates nothing.

    Phase order (§3.0, normative): the quarantine re-grade is folded into site
    construction (Phase-0 predicate move); Phase-1 runs the stub population
    (class-T stubs UNION Q-REAL) through the pocket solver (dense clusters) +
    isolated ladder; Phase-2 runs the interior struts (SM-STRUT S1); Phase-3 runs
    the ball breaches (SM-BALL B1) LAST, against the now-final shaft geometry.
    Shaft geometry (stubs, then struts) is spliced and revalidated before the ball
    phase reads it.

    CONVERGENCE (fix-round F1): the three phases run as ONE sweep, then the sweep
    repeats -- re-deriving the frozen defect snapshot from the MUTATED forest
    (`member_list_fn` re-reads the live member/ball lists) -- until a sweep commits
    ZERO splices (the true-no-op fixed point) or the defect population stops
    strictly decreasing (monotone guard against re-aim/slide oscillation), bounded
    by `cfg.p8_max_relocate_sweeps` (default 8). This converts the previously-
    discarded first relocation pass into cumulative progress and makes the §6.10
    idempotence gate real: a subsequent `run_phase8` over the fixed-point forest
    builds the same snapshot, commits zero splices in its first sweep, and returns
    a byte-identical no-op. Deterministic: no RNG, no wall-clock in the loop body;
    termination is driven by integer commit / defect counts only.

    `member_list_fn` (the engine passes `lambda: live_member_lists(forest, ...)`):
    re-reads the live member + ball lists from the current forest state between
    sweeps. When None (unit/legacy callers), the loop runs EXACTLY ONE sweep on the
    passed-in `members`/`balls` -- byte-identical to the pre-convergence driver."""
    t0 = time.time()
    log = log if log is not None else {}
    if quarantine_sites is None:
        quarantine_sites = quarantine_sites_from_forest(forest, contacts)

    max_sweeps = int(getattr(cfg, "p8_max_relocate_sweeps", 8))
    if member_list_fn is None:
        max_sweeps = 1                      # legacy/unit: single sweep, no re-read

    # accumulators: resolved counts are CUMULATIVE across sweeps (a site resolved
    # in sweep k is gone from sweep k+1's snapshot); unresolved counts are the
    # FINAL sweep's remaining population. n_*_sites / snapshot_summary are frozen
    # to the FIRST sweep (the audit record of the round's input, §3.0).
    stub_resolved = qreal_resolved = strut_resolved = ball_resolved = 0
    stub_unresolved = qreal_unresolved = strut_unresolved = ball_unresolved = 0
    reverted: list = []
    unresolved_items: list = []
    pocket_labels: list = []
    ledger: list = []
    committed_by_sweep: list = []
    first_summary = None
    first_sites = (0, 0, 0)
    first_moot = ()
    entered_any = False
    prev_defect = None
    sweeps_run = 0
    reached_fp = False                          # True iff a further sweep is a no-op

    for _sweep in range(max_sweeps):
        if _sweep > 0:
            members, balls = member_list_fn()   # re-read the mutated forest
        di = construct_sites(forest, members, balls, contacts, carried, cfg, geom,
                             mesh, unrouted=unrouted, quarantine_sites=quarantine_sites,
                             census_authority=census_authority)
        if first_summary is None:
            first_summary = di.snapshot.summary()
            first_sites = (len(di.stub_sites), len(di.strut_sites), len(di.ball_sites))
            first_moot = tuple(di.snapshot.moot)
        if not (di.stub_sites or di.strut_sites or di.ball_sites):
            reached_fp = True                   # snapshot emptied -> truly done
            break                               # empty snapshot -> nothing left to do
        # monotone guard: this sweep's starting defect population must be strictly
        # below the previous sweep's, else stop (an oscillation / net-zero sweep).
        defect = len(di.snapshot.non_exempt) + len(di.snapshot.ball_breaches)
        if prev_defect is not None and defect >= prev_defect:
            break
        prev_defect = defect
        entered_any = True

        sw = _relocation_sweep(di, forest, geom, cfg)
        sweeps_run += 1
        committed_by_sweep.append(sw.n_committed)
        stub_resolved += sw.stub_resolved
        qreal_resolved += sw.qreal_resolved
        strut_resolved += sw.strut_resolved
        ball_resolved += sw.ball_resolved
        stub_unresolved = sw.stub_unresolved
        qreal_unresolved = sw.qreal_unresolved
        strut_unresolved = sw.strut_unresolved
        ball_unresolved = sw.ball_unresolved
        reverted.extend(sw.reverted)
        unresolved_items = list(sw.unresolved_items)
        pocket_labels = sw.pocket_labels
        for row in sw.ledger_rows:
            ledger.append((_sweep,) + tuple(row))

        if sw.n_committed == 0:
            reached_fp = True                   # committed nothing -> fixed point
            break                               # true no-op: fixed point reached

    if not entered_any:
        # §6.10 idempotence: empty snapshot -> no ladder, no splice, no-op.
        return Phase8Report(
            snapshot_summary=first_summary or {}, n_stub_sites=0, n_strut_sites=0,
            n_ball_sites=0, stub_resolved=0, stub_unresolved=0, qreal_resolved=0,
            qreal_unresolved=0, strut_resolved=0, strut_unresolved=0,
            ball_resolved=0, ball_unresolved=0,
            reverted_splices=(), unresolved_items=(), pocket_labels=(),
            dt_s=round(time.time() - t0, 3), entered_ladder=False,
            sweeps=0, converged=True, committed_by_sweep=(),
            provenance_ledger=(), moot_member_ids=tuple(first_moot))

    # converged iff a further sweep is provably a no-op: either the snapshot
    # emptied or the last executed sweep committed nothing. A monotone-guard stop
    # or a sweep-cap exhaustion leaves `reached_fp` False -- the ladder had NOT
    # reached its fixed point, and idempotence is not claimed (reported honestly).
    converged = reached_fp

    return Phase8Report(
        snapshot_summary=first_summary,
        n_stub_sites=first_sites[0], n_strut_sites=first_sites[1],
        n_ball_sites=first_sites[2],
        stub_resolved=stub_resolved, stub_unresolved=stub_unresolved,
        qreal_resolved=qreal_resolved, qreal_unresolved=qreal_unresolved,
        strut_resolved=strut_resolved, strut_unresolved=strut_unresolved,
        ball_resolved=ball_resolved, ball_unresolved=ball_unresolved,
        reverted_splices=tuple(reverted), unresolved_items=tuple(unresolved_items),
        pocket_labels=tuple(pocket_labels),
        dt_s=round(time.time() - t0, 3), entered_ladder=True,
        sweeps=sweeps_run, converged=converged,
        committed_by_sweep=tuple(committed_by_sweep),
        provenance_ledger=tuple(ledger), moot_member_ids=tuple(first_moot))


def _orig_embed(forest, fi):
    if fi is None or not (0 <= fi < len(forest.members)):
        return None
    return forest.members[fi].embed_end
