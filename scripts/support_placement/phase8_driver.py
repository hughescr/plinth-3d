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
from scipy.spatial import cKDTree

from . import phase8, verify, joints, p8_accept, tree, perception

# ---------------------------------------------------------------------------
# Frozen-fixture target law (SPEC-P8-RELOCATE §3.0 / §5.3) -- the TREANT law.
# ---------------------------------------------------------------------------
# The frozen `phase8_relocation_input.json` is the acceptance TARGET-SELECTION
# authority for the treant (§5.3 Layer 2, census-authoritative): 143 non-exempt
# (incl. the 6 census-only boundary members {168,376,443,445,485,519} the kernel
# robust tip-anchor merge exempts but the census counts), 50 moot (incl. mi=420,
# all-moot at full IEEE-754 precision), 77 REAL quarantine, 15 ball breaches. The
# live-forest gate under-counts these boundary members (kernel-exempt), so the
# frozen fixture -- not the live gate -- is the treant target law.
#
# It is applied ONLY when the current geometry IS the fixture's source geometry
# (the pre-relocation treant, v5e_run1). The gate is a canonical members digest:
# on the ORIGINAL geometry it matches and the fixture's 143/50 target set is used;
# once relocation MUTATES the geometry (subsequent convergence sweeps, the §6.10
# idempotence pass, or ANY other model) the digest no longer matches and target
# selection falls back to the live gate -- which reads the relocated members legal
# and yields an EMPTY snapshot, so the idempotence pass is a provable no-op by
# construction (and the fixture's v5e_run1 member INDICES, invalid on a mutated /
# re-indexed forest, are never mis-applied). Pinned from
# `p8_accept.geometry_digest(collect_members(v5e_run1.members), [])`.
TREANT_PRERELOCATE_DIGEST = (
    "354f1ec85b972c14b674167200130d397aac5f71bb58359d9b954129fd35341f")


def load_treant_frozen_snapshot():
    """Load the frozen treant target-law snapshot (§3.0), or None if the pinned
    fixture is unavailable (unit/synthetic callers on a tiny mesh). Ball breaches
    come from the LIVE gate (the JSON does not carry the 15), so this carries only
    the non_exempt / moot / quarantine target sets."""
    try:
        return phase8.load_frozen_snapshot()
    except Exception:
        return None


def _members_geometry_digest(members) -> str:
    """Canonical byte-exact digest of the current live member geometry (members
    only, balls excluded), for the frozen-fixture geometry gate. Reuses the
    amendment-G `p8_accept.geometry_digest` (hex-exact IEEE-754, no rounding) so a
    single serialization contract governs both the gate and the idempotence gate."""
    return p8_accept.geometry_digest(members, [])


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


def _contact_index_of(member: dict, contacts) -> Optional[int]:
    """The INDEX of the contact a member owns, or None. The R2 re-seat needs the
    index (not just the dict) so a committed re-seat can write the moved demand
    back into `contacts[ci]` -- coverage and island-birth accounting read contact
    POSITIONS, so a migrated demand that is not written back would leave the
    gates judging a position no stub is standing on."""
    owner = member.get("owner")
    if isinstance(owner, (tuple, list)) and len(owner) == 2 and owner[0] == "contact":
        ci = int(owner[1])
        if 0 <= ci < len(contacts):
            return ci
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
    # Amendment E (defect-1a): the strut foot ball CENTRE is itself the FIXED
    # model-surface anchor -- feet are designed ON the surface (census: all 263
    # interior struts have embed-tip surface distance d=0). The invariant contact
    # point `c` is therefore the foot point, and the foot embed depth is 0, so a
    # re-aim PIVOTS the shaft about the FIXED foot (every candidate's embed end is
    # `c - n_hat*0 == foot`, unchanged) and the embed tip's signed surface
    # distance d is preserved EXACTLY under re-aim -- verified against the mesh by
    # the shared certificate (`verify.certify_member`) on every candidate.
    #
    # The OLD `c = foot + n0*contact_depth_mm` floated the anchor contact_depth_mm
    # OFF the surface along the shaft axis, so any off-axis candidate dragged the
    # foot off the model (d 0 -> 0.4-2.05mm, A_anchor collapse): the exact
    # regression that turned 38 legal struts illegal in the round-1 run.
    delta = 0.0
    c = foot.copy()
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
    target_source: str = "live-gate"  # "frozen-fixture" iff geometry-gated fixture
    # R2 re-seat bookkeeping: a stub site's owning CONTACT index, so a committed
    # re-seat can write the moved demand back into the contact record (the demand
    # migrated -- coverage/birth accounting must see where it actually is).
    contact_of_site: dict = field(default_factory=dict)   # site_id -> contact idx


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


def frozen_target_snapshot(frozen: phase8.DefectSnapshot, gate: GateClassification,
                           real_ids, artifact_ids) -> phase8.DefectSnapshot:
    """Assemble the frozen §3.0 target-law snapshot for the treant acceptance run
    (§5.3 Layer 2): non_exempt / moot are taken VERBATIM from the frozen fixture
    (the census-authoritative 143/50, incl. the 6 census-only boundary members the
    live kernel gate exempts), the quarantine split from the live re-grade (tied to
    the forest's own quarantine records, deterministic, == the JSON's 77), and the
    ball breaches from the live gate (the JSON carries no balls). Applied ONLY under
    the geometry gate (`construct_sites`), so it never touches a mutated forest."""
    return phase8.DefectSnapshot(
        non_exempt=tuple(frozen.non_exempt), moot=tuple(frozen.moot),
        quarantine_real=tuple(sorted(real_ids)),
        quarantine_artifact=tuple(sorted(artifact_ids)),
        ball_breaches=tuple(range(len(gate.ball_breaches))),
        partition=dict(n_firm_engine=len(gate.firm_transit),
                       n_frozen_non_exempt=len(frozen.non_exempt),
                       n_moot=len(frozen.moot)),
        provenance=dict(source="frozen-fixture (treant law, geometry-gated) "
                               "+ live quarantine-regrade + live ball-gate"))


def construct_sites(forest, members, balls, contacts, carried, cfg, geom, mesh,
                    unrouted=(), quarantine_sites=None,
                    census_authority=frozenset(),
                    frozen_snapshot=None) -> DriverInput:
    """Build the full Phase-8 driver input from the engine's OWN gate output
    (task step 1). BATCH-FIRST: one `classify_gate` pass (batched clearance +
    ball-breach reports) drives every class; per-site geometry is pure algebra on
    the already-gathered member dicts (no kernel call in the site loop).

    The site_id of every stub/strut is its live member index `mi` (the stable
    §3.0 site ID = the census/gate integer); quarantine stubs keep their own
    `stub_member` id (a disjoint index space -- a quarantined stub is never in the
    live list).

    Target selection (§5.3): when `frozen_snapshot` is supplied AND the current
    member geometry IS the frozen fixture's source geometry (the pre-relocation
    treant, matched by `_members_geometry_digest == TREANT_PRERELOCATE_DIGEST`),
    the non_exempt / moot target set is the FROZEN fixture (the census-authoritative
    143/50 treant law, incl. the 6 census-only boundary members the live kernel gate
    exempts). Otherwise -- a mutated forest (a convergence sweep, the idempotence
    pass) or any non-treant model -- target selection is the live engine gate (which
    reads relocated members legal, so the idempotence pass builds an empty snapshot).
    Returns a `DriverInput`."""
    members = joints.collect_members(members)
    gate = classify_gate(mesh, members, balls, geom, cfg)
    real_ids, artifact_ids = regrade_quarantine(quarantine_sites, geom, cfg)
    use_frozen = (frozen_snapshot is not None
                  and _members_geometry_digest(members) == TREANT_PRERELOCATE_DIGEST)
    if use_frozen:
        snapshot = frozen_target_snapshot(frozen_snapshot, gate, real_ids, artifact_ids)
    else:
        snapshot = build_snapshot(gate, real_ids, artifact_ids, census_authority)

    live_fi = live_forest_index_map(forest, unrouted)

    stub_sites, strut_sites = [], []
    site_forest_idx = {}
    contact_of_site = {}
    for mi in snapshot.non_exempt:
        m = members[mi]
        contact = _contact_of(m, contacts)
        carried_mm2 = _carried_of(m, contacts, carried)
        wc = gate.worst_chord.get(mi, 0.0)
        if m["kind"] == "stub":
            stub_sites.append(stub_site_from_member(mi, m, contact, cfg, carried_mm2, wc))
            ci = _contact_index_of(m, contacts)
            if ci is not None:
                contact_of_site[mi] = ci
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
        qowner = qs.get("owner")
        if (isinstance(qowner, (tuple, list)) and len(qowner) == 2
                and qowner[0] == "contact"):
            contact_of_site[Q_SITE_OFFSET + int(qid)] = int(qowner[1])

    ball_sites = ball_sites_from_breaches(list(gate.ball_breaches), members, cfg)

    return DriverInput(snapshot=snapshot, stub_sites=stub_sites,
                       strut_sites=strut_sites, ball_sites=ball_sites,
                       live_fi=live_fi, site_forest_idx=site_forest_idx, gate=gate,
                       target_source=("frozen-fixture" if use_frozen else "live-gate"),
                       contact_of_site=contact_of_site)


# ===========================================================================
# R2 -- SURFACE-SEED SEARCH + THE INVIOLABLE-DOMAIN / COVERAGE GUARD.
# ===========================================================================
# SM-STUB R2 (§3.1) moves a DEMAND, not merely a shaft: the contact surface
# point migrates to an alternative model-surface seat. Two things therefore
# bound it, and neither is a config lever:
#
#   * THE INVIOLABLE CLASS DOMAIN (REACH §1). A contact belongs to its island;
#     a seat on a different island is a different demand, not a relocation of
#     this one. Seeds are restricted to faces of the contact's OWN island.
#   * THE NEIGHBOR-BIRTH / COVERAGE GUARD (REACH §2b, "before committing, guard
#     neighbor birth coverage"). `uncovered_islands` and `uncovered_island_births`
#     are HARD gates that read contact POSITIONS -- a re-seat that clears a
#     transit by uncovering an island has not fixed anything, it has moved the
#     failure to a gate the D6 influence-set invariant cannot see (that invariant
#     scores MEMBER geometry classes only). So the guard is applied at SEED
#     SELECTION, before any candidate is built: a seat that would regress
#     coverage or orphan a birth this contact uniquely covers is never enumerated.
#
# BATCH-FIRST: the face KD-tree is built ONCE per run and reused by every sweep;
# every site's seed neighbourhood comes from ONE `query_ball_point` over the
# whole stacked query-point array; the coverage guard evaluates all of a site's
# seats against all of its critical island faces in ONE (n_seeds x n_faces)
# array. There is no per-seed scalar mesh query anywhere below.


class SurfaceSeedIndex:
    """Model-surface seat source for R2 (and for D5 when a junction sits on the
    surface). Holds the face-centroid KD-tree, the face normals, and the
    face->island map, all built ONCE and reused across sweeps.

    `islands` (the engine's island face-id lists) is optional: without it the
    per-island domain restriction cannot be applied, so `seeds_for` refuses to
    emit ANY seed rather than silently relocating a demand out of its class
    domain -- R2 simply does not fire for a caller that did not supply the
    domain (unit/legacy paths), which is the conservative failure direction."""

    def __init__(self, mesh, islands=None, island_births=None):
        self.centers = np.asarray(mesh.triangles_center, float)
        self.normals = np.asarray(mesh.face_normals, float)
        self.tree = cKDTree(self.centers) if len(self.centers) else None
        self.islands = islands
        self.island_births = list(island_births or ())
        n_faces = len(self.centers)
        self.island_of_face = np.full(n_faces, -1, dtype=np.int64)
        if islands is not None:
            for iid, face_ids in enumerate(islands):
                fi = np.asarray(face_ids, dtype=np.int64)
                if len(fi):
                    self.island_of_face[fi] = int(iid)

    # -- coverage / birth guard ------------------------------------------------
    def _coverage_context(self, ci: int, contacts, unrouted, cfg):
        """Everything the coverage half of the guard needs for contact `ci`, in
        ONE KD query: the island's face centres, the per-face distance to the
        nearest OTHER routed contact on that island, the gate threshold, and the
        island's CURRENT worst gap. Returns None when the island is unknown (no
        domain -> no seeds)."""
        if self.islands is None or not (0 <= ci < len(contacts)):
            return None
        iid = int(contacts[ci].get("island_id", -1))
        if not (0 <= iid < len(self.islands)):
            return None
        face_ids = np.asarray(self.islands[iid], dtype=np.int64)
        if not len(face_ids):
            return None
        centers = self.centers[face_ids]
        unrouted = set(int(u) for u in unrouted)
        others = [np.asarray(c["position"], float) for k, c in enumerate(contacts)
                  if k != ci and k not in unrouted
                  and int(c.get("island_id", -1)) == iid]
        if others:
            d_other, _ = cKDTree(np.array(others)).query(centers)
            d_other = np.asarray(d_other, float)
        else:
            d_other = np.full(len(centers), np.inf)
        cur = np.linalg.norm(centers - np.asarray(contacts[ci]["position"], float),
                             axis=1)
        threshold = float(cfg.target_spacing_mm) * float(cfg.coverage_radius_factor)
        cur_worst = float(np.minimum(d_other, cur).max())
        return centers, d_other, threshold, cur_worst

    def _sole_births(self, ci: int, contacts, unrouted, cfg):
        """The island births that ONLY contact `ci` covers today -- the births a
        re-seat could orphan (REACH §2b's neighbor-birth guard). Births covered
        by some OTHER routed contact are not this move's responsibility."""
        if not self.island_births:
            return []
        z_tol = float(cfg.minima_connect_layers) * float(cfg.layer_height_mm)
        attach_r = perception.ISLAND_ATTACH_REACH_MM
        unrouted = set(int(u) for u in unrouted)

        def _covers(pos, b):
            if abs(float(pos[2]) - float(b["z"])) > z_tol:
                return False
            ok, _d = perception.island_birth_xy_ok(float(pos[0]), float(pos[1]),
                                                  b["loop"], attach_r)
            return bool(ok)

        here = np.asarray(contacts[ci]["position"], float)
        out = []
        for b in self.island_births:
            if not _covers(here, b):
                continue
            if any(_covers(np.asarray(c["position"], float), b)
                   for k, c in enumerate(contacts)
                   if k != ci and k not in unrouted):
                continue
            out.append(b)
        return out

    # -- the seed search -------------------------------------------------------
    def seeds_for(self, sites, contact_of_site, contacts, unrouted, cfg) -> dict:
        """`{site_id: [(position, outward_normal, distance_from_demanded), ...]}`,
        ordered nearest-to-demanded and capped at `cfg.p8_reseat_max_seeds`
        (REACH §2b), restricted to the contact's own island (the inviolable class
        domain) and filtered by the coverage / sole-birth guard.

        ONE stacked `query_ball_point` over every site's demanded position; the
        per-site work is then pure numpy over that site's own candidate faces."""
        out = {s.site_id: [] for s in sites}
        if (self.tree is None or self.islands is None or not sites
                or not getattr(cfg, "p8_reseat_enable", True)):
            return out
        radius = float(getattr(cfg, "p8_reseat_radius_mm", 2.0))
        cap = int(getattr(cfg, "p8_reseat_max_seeds", 24))
        if radius <= 0.0 or cap <= 0:
            return out
        pts = np.array([np.asarray(s.c, float) for s in sites])
        # ONE batched neighbourhood query for every site (batch-first).
        neigh = self.tree.query_ball_point(pts, r=radius)
        for k, site in enumerate(sites):
            ci = contact_of_site.get(site.site_id)
            if ci is None:
                continue
            faces = np.asarray(sorted(neigh[k]), dtype=np.int64)
            if not len(faces):
                continue
            iid = int(contacts[ci].get("island_id", -1))
            faces = faces[self.island_of_face[faces] == iid]      # class domain
            if not len(faces):
                continue
            c0 = np.asarray(site.c, float)
            cand = self.centers[faces]
            dist = np.linalg.norm(cand - c0[None, :], axis=1)
            keep = dist > 1e-9                    # the demanded seat already failed
            faces, cand, dist = faces[keep], cand[keep], dist[keep]
            if not len(faces):
                continue
            # nearest-to-demanded, face index as the total-order tiebreak.
            order = np.lexsort((faces, dist))
            faces, cand, dist = faces[order], cand[order], dist[order]
            faces, cand, dist = faces[:cap], cand[:cap], dist[:cap]
            ctx = self._coverage_context(ci, contacts, unrouted, cfg)
            if ctx is None:
                continue
            centers, d_other, threshold, cur_worst = ctx
            # ONE (n_seeds x n_faces) evaluation of the island's coverage with
            # THIS contact moved to each seat: the new worst gap must clear the
            # gate threshold, or at least not exceed the CURRENT worst gap (an
            # already-uncovered island must not be made worse).
            dseat = np.linalg.norm(centers[None, :, :] - cand[:, None, :], axis=2)
            new_worst = np.minimum(d_other[None, :], dseat).max(axis=1)
            ok = (new_worst <= threshold + 1e-9) | (new_worst <= cur_worst + 1e-9)
            births = self._sole_births(ci, contacts, unrouted, cfg)
            if births:
                z_tol = float(cfg.minima_connect_layers) * float(cfg.layer_height_mm)
                attach_r = perception.ISLAND_ATTACH_REACH_MM
                for j in range(len(cand)):
                    if not ok[j]:
                        continue
                    p = cand[j]
                    for b in births:
                        if abs(float(p[2]) - float(b["z"])) > z_tol:
                            ok[j] = False
                            break
                        good, _d = perception.island_birth_xy_ok(
                            float(p[0]), float(p[1]), b["loop"], attach_r)
                        if not good:
                            ok[j] = False
                            break
            sel = np.nonzero(ok)[0]
            out[site.site_id] = [
                (cand[j].copy(), phase8._unit(self.normals[faces[j]]), float(dist[j]))
                for j in sel]
        return out


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


# ---------------------------------------------------------------------------
# D2 (SPEC-P8-RELOCATE): connectivity- and printability-preserving commits.
# ---------------------------------------------------------------------------
def _live_universe(forest, unrouted):
    """The live member-dict universe the engine gate sees (`live_member_lists`'
    filter: drop honestly-unrouted stubs + rewire-dead), plus the stable
    forest<->live index maps. Built ONCE per splice batch (batch-first): the
    affected-set connectivity guard scores every subset endpoint against THIS
    full universe (so a member never reads floating merely for lack of a partner
    outside the subset). Returns `(members_dicts, live_fi, pos_of)` where
    `members_dicts[k]` is `forest.members[live_fi[k]]` and `pos_of[fi] == k`. The
    live-index SET is stable across the splice (no member is added/killed, only
    moved), so `live_fi`/`pos_of` are valid for both the pre and post reports."""
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted
                         if i in forest.terminal_of}
    dead = getattr(forest, "dead", set())
    live_fi = [fi for fi in range(len(forest.members))
               if fi not in unrouted_stub_idx and fi not in dead]
    members = joints.collect_members([forest.members[fi] for fi in live_fi])
    pos_of = {fi: k for k, fi in enumerate(live_fi)}
    return members, live_fi, pos_of


def _coincident(idx_arr, end_arr, pt_arr, point, tol, exclude_fi=None):
    """Vectorised endpoint-node coincidence query against a pre-built
    `Forest.endpoint_index` (batch-first, not a per-member forest scan). Returns
    `[(fi, end), ...]` for every indexed endpoint within `tol` of `point`,
    excluding `exclude_fi`. Same coincidence semantics as `_translate_joint`."""
    if len(idx_arr) == 0:
        return []
    d = np.linalg.norm(pt_arr - np.asarray(point, float)[None, :], axis=1)
    hit = np.nonzero(d <= tol)[0]
    return [(int(idx_arr[k]), int(end_arr[k])) for k in hit
            if exclude_fi is None or int(idx_arr[k]) != exclude_fi]


def _member_endpoints(snap):
    """The two `(point, radius)` endpoints of a member snapshot (p0/r0, p1/r1)."""
    return ((np.asarray(snap["p0"], float), float(snap["r0"])),
            (np.asarray(snap["p1"], float), float(snap["r1"])))


# ---------------------------------------------------------------------------
# D6 -- ZERO-NEW-VIOLATION COMMIT INVARIANT (orchestrator ruling, 2026-07-24).
# ---------------------------------------------------------------------------
# The relocation ladder did not converge: committing a fix for member A perturbed
# a NEIGHBOUR B (through the D2 co-move that keeps the junction welded) into a NEW
# legal violation, which the next sweep re-targeted, which perturbed C... The
# sweep tail plateaued at 1-2 commits/sweep forever ([57,14,11,9,3,3,1,2], cap 8)
# and no per-SITE retirement rule can fix a per-NEIGHBOUR cascade.
#
# The invariant that does: a candidate whose application would create ANY new
# violation -- transit, embed grade, connectivity, printability, ball breach --
# ANYWHERE in its influence set is ILLEGAL AT COMMIT, and a commit that does not
# STRICTLY DECREASE the defect count over that influence set is likewise illegal
# (it is churn: the asymptotic "worst_chord shrinks a little every sweep but never
# clears rule 4" re-commit is exactly this case). Both together make every
# surviving commit strictly decrease the GLOBAL defect count -- geometry outside
# the influence set is untouched and its defect classes are functions of that
# geometry alone -- so the sweep loop is monotone in a non-negative integer and
# reaches a fixed point (hence idempotence) BY CONSTRUCTION, not by a cap.
#
# BATCH-FIRST: the whole influence set is scored in ONE evaluation per side
# (pre/post) -- two kernel interval calls, one SCAR verdict call, one connectivity
# report, one printability report, one sphere-breach call -- never per member.


def _influence_defects(member_dicts, subset, geom, cfg, plate_z):
    """Score EVERY defect class over `subset` (positions into `member_dicts`, the
    live-universe list) in ONE batched pass. Returns `{pos: frozenset(classes)}`,
    entries only for positions carrying >= 1 defect.

    The classes, and the ONE authority each is read from -- never a second
    predicate invented here (SPEC-P8-RELOCATE §5 "one law, one authority"):

      * "transit" / "embed" -- `verify.certify_member`, the SHARED certificate the
        candidate gate (`phase8._batch_legal_sets`) and the acceptance verifier
        (`verify._clearance_report_exact`) both route through: `nonexempt` is a
        firm rule-4/PRINT-FLOOR crossing, `scar_ok=False` is an embed-GRADE
        regression (a collapsed SCAR-AREA-CAP anchor). Stubs classify AXIS-only,
        non-stubs AXIS + capsule-offset -- the same ray split the gate uses.
      * "connectivity" -- `verify.connectivity_report` (subset-restricted, full
        attachment universe).
      * "printability" -- `tree.printability_report` (subset-restricted).
      * "ball" -- `verify.ball_breach_report` over the joint balls the SUBSET's own
        members form (`joints.compute_joint_balls`) plus their embed end-caps. An
        end-cap breach carries its own `member_idx`; a joint-ball breach is
        attributed to every subset member with an endpoint at the breaching
        centre (one vectorised distance query, not a scan).

    Deterministic: pure geometry + sorted iteration, no wall-clock, no RNG."""
    out: dict = {}
    if not subset:
        return out
    subset = sorted(subset)
    sub_members = [member_dicts[k] for k in subset]

    def _add(pos, cls):
        out[pos] = out.get(pos, frozenset()) | {cls}

    # ---- transit + embed grade, through the ONE shared certificate ----------
    def _seg_len(m):
        return float(np.linalg.norm(np.asarray(m["p1"], float)
                                    - np.asarray(m["p0"], float)))
    stub_mis = [j for j, m in enumerate(sub_members)
                if m["kind"] == "stub" and _seg_len(m) >= 1e-9]
    seg_mis = [j for j, m in enumerate(sub_members)
               if m["kind"] != "stub" and _seg_len(m) >= 1e-9]
    ivmap = {**verify._batch_axis_intervals(geom, sub_members, stub_mis, rays="axis"),
             **verify._batch_axis_intervals(geom, sub_members, seg_mis, rays="all")}
    all_mis = sorted(stub_mis + seg_mis)
    verdicts = verify._embed_scar_verdicts(geom, sub_members, all_mis, ivmap, cfg)
    for j in all_mis:
        cert = verify.certify_member(ivmap.get(j, []), sub_members[j], verdicts.get(j))
        if cert["nonexempt"]:
            _add(subset[j], "transit")
        if not cert["scar_ok"]:
            _add(subset[j], "embed")

    if plate_z is None:                 # legacy/unit oracle: the rest needs a plate
        return out

    # ---- connectivity + printability (subset-restricted, full universe) -----
    for v in verify.connectivity_report(member_dicts, plate_z, cfg, geom=geom,
                                        subset=subset):
        _add(int(v["member_idx"]), "connectivity")
    for v in tree.printability_report(member_dicts, cfg, subset=subset):
        _add(int(v["member_idx"]), "printability")

    # ---- ball breaches over the subset's own joints + embed end-caps --------
    if hasattr(geom, "sphere_breach"):
        sub_balls = joints.compute_joint_balls(sub_members, plate_z, cfg)
        breaches = verify.ball_breach_report(geom, sub_members, sub_balls, cfg)
        joint_centers = [np.asarray(v["center"], float) for v in breaches
                         if v.get("source") == "ball"]
        for v in breaches:
            if v.get("source") == "endcap":
                _add(subset[int(v["member_idx"])], "ball")
        if joint_centers:
            # ONE vectorised endpoint query: (n_breach x 2*n_sub) distances.
            ep_pts = np.array([np.asarray(m[e], float)
                               for m in sub_members for e in ("p0", "p1")])
            ep_pos = [subset[j] for j in range(len(sub_members)) for _e in (0, 1)]
            d = np.linalg.norm(np.array(joint_centers)[:, None, :]
                               - ep_pts[None, :, :], axis=2)
            for k in np.unique(np.nonzero(d <= joints.COINCIDE_TOL_MM)[1]):
                _add(ep_pos[int(k)], "ball")
    return out


def _defect_count(defmap, positions) -> int:
    """Total defect count (member x class) over `positions` -- the integer the
    commit invariant requires every commit to strictly decrease."""
    return sum(len(defmap.get(k, ())) for k in positions)


def _post_members(forest, pre_members, pos_of, changed_fis):
    """The live member-dict universe AFTER a transaction, built INCREMENTALLY
    from the pre-transaction one: only `changed_fis` moved, and
    `joints.collect_members` is a per-member pure normalisation (no cross-member
    state), so replacing exactly those entries yields the bit-identical list a
    full re-collect would -- at O(|changed|) instead of O(n_members).

    That difference is not cosmetic: the D5 junction lattice and the SM-BALL
    slide search evaluate this guard once per TRIAL, and a full 1476-member
    re-collect per trial is the shape of pure-Python-per-item cost that turned
    the ladder into a 99%-CPU spin once already."""
    out = list(pre_members)
    for fi in sorted(changed_fis):
        k = pos_of.get(fi)
        if k is not None:
            out[k] = joints.collect_members([forest.members[fi]])[0]
    return out


def _ball_slide_guard_ok(forest, geom, cfg, plate_z, saved, pre_members,
                         pos_of, idx_arr, end_arr, pt_arr, pre_cache=None) -> bool:
    """D6 commit invariant for a ball-joint slide (Phase-3), the SAME law the
    shaft splice enforces (`splice_reaims`): over the affected set -- the
    translated joint members UNION their endpoint-neighbours -- the slide must
    create NO new violation of ANY class (transit, embed grade, connectivity,
    printability, ball breach) AND must STRICTLY DECREASE the defect count there.
    A slide that merely shuffles a breach around, or trades a breach for a new
    transit on a neighbour, is illegal at commit -- that is what makes the sweep
    loop monotone. `saved` is `_translate_joint`'s `(fi, pre-snapshot)` list; the
    forest currently holds the POST-slide geometry (this is called before the
    trial undo). `plate_z is None` disables the guard (legacy/unit callers).

    `pre_cache` (optional, caller-owned dict): memoises the PRE-side influence
    scoring by subset. A D5 lattice trials many deltas against ONE unchanged
    pre-geometry at ONE node, so every trial derives the SAME subset and the
    SAME `pre_def` -- re-asking the kernel for it per trial is the same
    "re-ask the same question" waste the pocket solver's cert_cache retired.
    Semantically inert: a miss computes exactly what the uncached path would."""
    if plate_z is None:
        return True
    affected = {fi for fi, _s in saved}
    for fi, snap in saved:
        for pt, r in _member_endpoints(snap):
            for (nfi, _ne) in _coincident(idx_arr, end_arr, pt_arr, pt,
                                          max(0.05, 0.5 * r), exclude_fi=fi):
                affected.add(nfi)
    subset = sorted(pos_of[fi] for fi in affected if fi in pos_of)
    if not subset:
        return False
    ckey = tuple(subset)
    if pre_cache is not None and ckey in pre_cache:
        pre_def = pre_cache[ckey]
    else:
        pre_def = _influence_defects(pre_members, subset, geom, cfg, plate_z)
        if pre_cache is not None:
            pre_cache[ckey] = pre_def
    post_members = _post_members(forest, pre_members, pos_of,
                                 {fi for fi, _s in saved})
    post_def = _influence_defects(post_members, subset, geom, cfg, plate_z)
    if any(post_def.get(k, frozenset()) - pre_def.get(k, frozenset()) for k in subset):
        return False                       # a NEW violation somewhere in reach
    return _defect_count(post_def, subset) < _defect_count(pre_def, subset)


def splice_reaims(forest, geom, cfg, accepted: list, *, plate_z=None, unrouted=(),
                  reasons_out: Optional[dict] = None):
    """Splice a batch of accepted re-aims into the forest under its
    checkpoint/revalidate protocol (deterministic, transactional). `accepted` is
    a list of `(forest_idx, cand_member, orig_embed_end)`.

    D2 (SPEC-P8-RELOCATE) makes the splice connectivity- and printability-
    PRESERVING:

      (A) CO-MOVE: a re-aim moves a member's tree-side end (a stub's p1, a strut's
          free p0); every OTHER live member whose endpoint is coincident with the
          OLD tree-side node (same test as `_translate_joint`, tol max(0.05,
          0.5*r)) rides the same delta ATOMICALLY, snapshotted into the same
          transaction, so the junction stays welded by construction -- exactly
          what the ball-slide path already does for joints.

      (B) D6 COMMIT INVARIANT (orchestrator ruling 2026-07-24, extending D2's
          guard transit- and embed-aware): after applying the batch, the affected
          set (spliced UNION co-moved UNION endpoint-neighbours -- the INFLUENCE
          set) is scored for EVERY defect class (`_influence_defects`), and a
          transaction is REVERTED AS A UNIT -- every co-moved member restored
          bit-exact -- when it either carries a NEW violation of any class or does
          not STRICTLY DECREASE the influence set's defect count. Batch-first: one
          evaluation per side over the whole affected list, never per member.

    Returns `(n_committed, reverted_forest_idxs)` (committed / reverted PRIMARY
    site count -- the caller keys the anti-churn ban on the reverted primaries).

    `reasons_out` (optional dict): filled `primary_forest_idx -> reason` for every
    revert, `"violation"` (a NEW violation somewhere in the influence set -- the
    ladder should try a DIFFERENT candidate, so the caller BANS this one) or
    `"no-progress"` (legal but sterile -- the caller RETIRES the site, since
    re-picking around a candidate that cannot reduce the defect count is exactly
    the churn that kept the sweep tail alive)."""
    if not accepted:
        return 0, []
    # pre-splice endpoint-node index + live universe (built ONCE, batch-first):
    # the co-move scan and the neighbour lookup query this index; the guard diffs
    # `pre_members` (this geometry) against the mutated post state.
    idx_arr, end_arr, pt_arr = forest.endpoint_index()
    pre_members, live_fi, pos_of = _live_universe(forest, unrouted)

    saved = {}

    def _snap(fi):
        if fi not in saved:
            saved[fi] = forest.members[fi]._snapshot()

    primary_fis = {fi for fi, _cm, _e in accepted
                   if fi is not None and 0 <= fi < len(forest.members)}
    moved_endpoints = set()          # (fi, end) already displaced -> co-move at most once
    txns = []                        # (primary_fi, members:set, responsibility:set)

    for fi, cand_member, orig_embed in accepted:
        if fi is None or not (0 <= fi < len(forest.members)):
            continue
        fm = forest.members[fi]
        embed = orig_embed if orig_embed in ("p0", "p1") else fm.embed_end
        tree_end = 0 if embed == "p1" else 1     # the NON-embedded (tree-side) end
        snap0 = fm._snapshot()
        old_tree_pt = np.asarray(snap0["p0"] if tree_end == 0 else snap0["p1"], float)
        r_tree = float(snap0["r0"] if tree_end == 0 else snap0["r1"])
        _snap(fi)
        _apply_member_geometry(fm, cand_member, orig_embed)
        new_tree_pt = np.asarray(fm.p0 if tree_end == 0 else fm.p1, float)
        delta = new_tree_pt - old_tree_pt

        members_of = {fi}
        # (A) co-move dependents coincident with the OLD tree-side node.
        tol = max(0.05, 0.5 * r_tree)
        for (cfi, cend) in _coincident(idx_arr, end_arr, pt_arr, old_tree_pt, tol,
                                       exclude_fi=fi):
            if cfi in primary_fis or (cfi, cend) in moved_endpoints:
                continue                         # its own re-aim / already displaced
            _snap(cfi)
            cm = forest.members[cfi]
            if cend == 0:
                cm.p0 = np.asarray(cm.p0, float) + delta
            else:
                cm.p1 = np.asarray(cm.p1, float) + delta
            moved_endpoints.add((cfi, cend))
            members_of.add(cfi)

        # responsibility = txn members UNION endpoint-neighbours (members a re-aim
        # could ORPHAN); a new float on any of them reverts THIS transaction.
        resp = set(members_of)
        for mfi in members_of:
            for pt, r in _member_endpoints(saved[mfi]):
                for (nfi, _ne) in _coincident(idx_arr, end_arr, pt_arr, pt,
                                              max(0.05, 0.5 * r), exclude_fi=mfi):
                    resp.add(nfi)
        txns.append((fi, members_of, resp))

    if not saved:
        return 0, []

    # merge txns that share a member (a co-moved member can land in two txns' sets)
    # into groups -- the revert UNIT, so a restore from the pre-splice snapshot is
    # bit-exact and no member is split across a partial revert. Deterministic:
    # groups keyed/sorted by min primary fi.
    n = len(txns)
    parent = list(range(n))

    def _find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def _union(a, b):
        ra, rb = _find(a), _find(b)
        if ra != rb:
            parent[max(ra, rb)] = min(ra, rb)

    owner = {}
    for ti, (_p, members_of, _r) in enumerate(txns):
        for mfi in members_of:
            if mfi in owner:
                _union(ti, owner[mfi])
            else:
                owner[mfi] = ti
    groups = {}
    for ti in range(n):
        g = groups.setdefault(_find(ti), dict(primaries=[], members=set(), resp=set()))
        g["primaries"].append(txns[ti][0])
        g["members"] |= txns[ti][1]
        g["resp"] |= txns[ti][2]

    # ---- D6 ZERO-NEW-VIOLATION COMMIT INVARIANT (see `_influence_defects`).
    # The influence set is the union of every group's responsibility set (spliced
    # UNION co-moved UNION endpoint-neighbours) -- the ONLY members whose defect
    # classes this batch can change. It is scored ONCE pre-splice; then, each
    # round, once post-splice, and every group that either (a) carries a NEW
    # violation of ANY class anywhere in its responsibility set, or (b) fails to
    # STRICTLY DECREASE the defect count over that set, is reverted as a unit.
    # Reverting can change a shared responsibility member's post state, so the
    # test is iterated to a fixed point -- each round reverts >= 1 group, so it
    # runs at most `len(groups)` rounds and always terminates.
    affected = set().union(*(g["resp"] for g in groups.values())) if groups else set()
    inf_subset = sorted(pos_of[fi] for fi in affected if fi in pos_of)
    pre_def = _influence_defects(pre_members, inf_subset, geom, cfg, plate_z)

    live_groups = dict(groups)
    reverted = []
    for _round in range(len(groups) + 1):
        violations = forest.revalidate(geom, cfg)
        bad_clearance = {v["member_idx"] for v in violations if v["member_idx"] in saved}
        # incremental (batch-first): only the snapshotted members can differ from
        # `pre_members` -- a reverted one collects back to its pre geometry, so
        # this is bit-identical to a full re-collect at O(|saved|).
        post_members = _post_members(forest, pre_members, pos_of, set(saved))
        post_def = _influence_defects(post_members, inf_subset, geom, cfg, plate_z)
        new_bad = {live_fi[k] for k in inf_subset
                   if post_def.get(k, frozenset()) - pre_def.get(k, frozenset())}
        bad_members = bad_clearance | new_bad
        illegal = []
        for root, g in sorted(live_groups.items(), key=lambda kv: min(kv[1]["primaries"])):
            if g["resp"] & bad_members:
                illegal.append((root, "violation"))
                continue
            if plate_z is None:
                continue                  # legacy/unit oracle: no progress ledger
            gsub = [pos_of[fi] for fi in g["resp"] if fi in pos_of]
            if _defect_count(post_def, gsub) >= _defect_count(pre_def, gsub):
                illegal.append((root, "no-progress"))   # churn: no strict decrease
        if not illegal:
            break
        for root, why in illegal:
            g = live_groups.pop(root)
            for mfi in sorted(g["members"]):
                if mfi in saved:
                    forest.members[mfi]._restore(saved[mfi])
            for pfi in sorted(g["primaries"]):
                reverted.append(pfi)
                if reasons_out is not None:
                    reasons_out[pfi] = why
        forest.revalidate(geom, cfg)      # re-certify the restored members
    n_committed = len(txns) - len(reverted)
    return n_committed, sorted(reverted)


# ---------------------------------------------------------------------------
# ball-slide splice (Phase 3): translate the joint node.
# ---------------------------------------------------------------------------
def _translate_joint(forest, center, delta, tol, index=None):
    """Translate every live member endpoint coincident with `center` by `delta`
    (a joint slide keeps the members meeting at the node coincident, so
    connectivity is preserved). Returns the list of `(forest_idx, snapshot)` it
    touched so the caller can revert.

    BATCH-FIRST: the coincidence test is ONE vectorised distance query against
    `Forest.endpoint_index` (the same live, dead-excluded endpoint set the old
    per-member Python scan walked), not a scan over every forest member. `index`
    lets a caller that trials MANY deltas against the SAME geometry (D5's
    junction lattice: every failed trial is undone bit-exact, so the endpoint
    index is invariant across them) build it once instead of once per trial --
    that per-trial O(n_members) rebuild is what turned the ladder into a spin.
    Passing a STALE index would silently mis-target the move, so it is only ever
    supplied by a caller that owns the geometry between rebuilds."""
    idx_arr, end_arr, pt_arr = index if index is not None else forest.endpoint_index()
    if len(idx_arr) == 0:
        return []
    center = np.asarray(center, float)
    delta = np.asarray(delta, float)
    hit = np.nonzero(np.linalg.norm(pt_arr - center[None, :], axis=1) <= tol)[0]
    if not len(hit):
        return []
    ends_by_fi: dict = {}
    for k in hit:
        ends_by_fi.setdefault(int(idx_arr[k]), []).append(int(end_arr[k]))
    saved = []
    for fi in sorted(ends_by_fi):                     # deterministic order
        fm = forest.members[fi]
        saved.append((fi, fm._snapshot()))
        for end in sorted(ends_by_fi[fi]):
            if end == 0:
                fm.p0 = np.asarray(fm.p0, float) + delta
            else:
                fm.p1 = np.asarray(fm.p1, float) + delta
    return saved


# ===========================================================================
# D5 -- PARENT-JUNCTION RE-SEAT (orchestrator ruling (5), 2026-07-24).
# ===========================================================================
# The rung BELOW R2 and above honest failure. R1 pivots the shaft about the
# demanded contact; R2 moves the demand itself. Both leave one degree of freedom
# untouched: the site's PARENT JUNCTION -- the tree-side node where the member
# meets its route. Moving THAT swings the shaft while holding the embedded tip
# (and therefore the demand, the carried area and the coverage accounting)
# exactly where it is, which is precisely the lever for a site whose contact is
# fine and whose corridor is not.
#
# The move is committed through the SAME guarded co-moving splice SM-BALL B1
# uses -- `_translate_joint` (every member meeting at the node rides the delta,
# so the junction stays welded by construction) under `_ball_slide_guard_ok`
# (the D6 zero-new-violation invariant + strict defect-count decrease over the
# influence set). D5 therefore cannot break the monotonicity the fixed point
# rests on: a junction re-seat that does not strictly reduce the defect count is
# refused exactly like a sterile re-aim.
#
# The candidate lattice is a total order (distance, then direction rank), so
# "first legal in order" IS the lexicographic minimum -- the same argument B1's
# minimal-slide search rests on, not a first-success-wins shortcut.


def junction_lattice(node, axis, cfg) -> list:
    """Ordered candidate positions for a parent-junction re-seat, nearest-first.

    Free-space lattice: +/- the member axis and the two stable local-frame
    perpendiculars (`phase8.local_frame`, the SAME frame the R1 azimuth grid is
    anchored to, so the two rungs never disagree about what "sideways" means),
    at `p8_junction_reseat_step_mm` multiples out to `p8_junction_reseat_max_mm`.
    Ordered by `(distance, direction_rank)` -- a total order, so the first legal
    candidate is the unique lexicographic minimum.

    The lattice is FREE-SPACE only, and deliberately so: a site's parent junction
    is where its member meets the ROUTE, which is in free space by construction
    -- the surface-anchored end is the embed end, and that end is R1/S1's cone
    and R2's seed search, not this rung's. A surface-seed branch here would be a
    code path no site can reach."""
    node = np.asarray(node, float)
    step = float(getattr(cfg, "p8_junction_reseat_step_mm", 0.25))
    reach = float(getattr(cfg, "p8_junction_reseat_max_mm", 1.0))
    if step <= 0.0 or reach <= 0.0:
        return []
    u = phase8._unit(np.asarray(axis, float))
    e, e2 = phase8.local_frame(u)
    dirs = [u, -u, e, -e, e2, -e2]                    # direction_rank = list index
    n_steps = int(round(reach / step))
    out = []
    for si in range(1, n_steps + 1):
        d = step * si
        for rank, dv in enumerate(dirs):
            out.append((float(d), rank, dv * d))
    out.sort(key=lambda t: (t[0], t[1]))
    return [(t[2], t[0], t[1]) for t in out]          # (delta, distance, rank)


def try_junction_reseat(forest, geom, cfg, plate_z, unrouted, fi, node_pt, axis,
                        tol, max_trials=None):
    """Trial the junction lattice for ONE site and COMMIT the first legal delta.

    Every trial is transactional: translate -> revalidate -> D6 guard -> undo,
    and the winner is left in place. Returns
    `(committed_delta or None, n_trials, n_members_moved)`.

    The pre-trial endpoint index and live universe are built ONCE for the whole
    lattice (a failed trial restores bit-exact geometry, so both are invariant
    across trials) -- the per-trial rebuild is the O(n_members) cost that made
    the naive version a spin."""
    # `fi` is ledger attribution only and is None for a BALL site (a joint node
    # is not any single member's endpoint) -- the move itself is addressed by
    # `node_pt`, never by a member index.
    if fi is not None and not (0 <= fi < len(forest.members)):
        return None, 0, 0
    index = forest.endpoint_index()
    pre_members, _live_fi, pos_of = _live_universe(forest, unrouted)
    lattice = junction_lattice(node_pt, axis, cfg)
    if max_trials is not None:
        lattice = lattice[:int(max_trials)]
    n_trials = 0
    pre_cache: dict = {}          # PRE-side influence scoring, invariant per node
    for delta, _dist, _rank in lattice:
        n_trials += 1
        saved = _translate_joint(forest, node_pt, delta, tol, index=index)
        if not saved:
            continue
        viol = forest.revalidate(geom, cfg)
        touched = {sfi for sfi, _s in saved}
        ok = (not any(v["member_idx"] in touched for v in viol)
              and _ball_slide_guard_ok(forest, geom, cfg, plate_z, saved,
                                       pre_members, pos_of, *index,
                                       pre_cache=pre_cache))
        if ok:
            return np.asarray(delta, float), n_trials, len(saved)
        for sfi, snapm in saved:                       # trial failed -> undo
            forest.members[sfi]._restore(snapm)
        forest.revalidate(geom, cfg)
    return None, n_trials, 0


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
    # ---- defect-3 (fixed point) + defect-5/6 (frozen-fixture target law) ------
    target_source: str = "live-gate"  # "frozen-fixture" iff the treant target law
    sweep_tail_monotone: bool = True  # committed_by_sweep non-increasing (anti-churn)
    n_banned: int = 0                 # collision-reverted candidates banned (anti-churn)
    # ---- D3(a) progress-based retirement (fixed point) -----------------------
    retired_sites: tuple = ()         # (report_class, site_id, last_rung) honest-unresolved
    # ---- D6 zero-new-violation commit invariant ------------------------------
    # Global defect population (non-exempt + ball breaches) at the TOP of each
    # executed sweep. The invariant makes every surviving commit strictly reduce
    # it, so this trace is the machine-checkable monotonicity evidence: it is
    # non-increasing, and strictly decreasing across any sweep that committed.
    defects_by_sweep: tuple = ()
    # ---- ruling (5): R2 surface-seed re-seat / D5 junction re-seat -----------
    reseats: tuple = ()               # (site_id, contact_idx, x, y, z, dist_mm) committed
    junction_reseats: tuple = ()      # (report_class, site_id, dx, dy, dz, n_trials)
    # ---- ruling (5): the PERSISTENT formally-unresolvable ledger -------------
    # One row per site the ladder could not resolve, CARRIED ACROSS SWEEPS (and,
    # via the caller's `carry_ledger`, across passes) rather than rebuilt from the
    # last sweep's leftovers -- so a site that fell at R1 in sweep 1, was retried
    # at R2 in sweep 2 and finally refused at D5 in sweep 3 is ONE row with its
    # whole history, not three disconnected observations. Rows are
    # `(class, site_id, last_rung, reason, first_sweep, last_sweep, n_attempts,
    # measure_mm)`, sorted -- the honest-failure report §6.7 gates on and the
    # thing an owner actually reads to decide what to do about a site.
    unresolvable_ledger: tuple = ()


# ---------------------------------------------------------------------------
# the PERSISTENT formally-unresolvable ledger (ruling (5)).
# ---------------------------------------------------------------------------
_LEDGER_FIELDS = ("cls", "site_id", "rung", "reason", "first_sweep", "last_sweep",
                  "n_attempts", "measure_mm")


def ledger_touch(ledger: dict, cls: str, site_id: int, rung: str, reason: str,
                 sweep: int, measure: Optional[float] = None) -> None:
    """Record (or update) one formally-unresolvable site in `ledger`, keyed by
    `(class, site_id)`. A repeat observation UPDATES the row -- last rung, last
    reason, last sweep, attempt count, latest defect measure -- it never appends
    a duplicate, which is what makes the ledger a persistent register rather than
    a log. `ledger` is caller-owned so it survives sweeps AND passes.

    `measure` is the site's live defect measure (transit chord for a stub/strut,
    poke depth for a ball) at the moment of the observation; None leaves the
    previously recorded value in place (an observation with no measure must not
    erase a good one) and is reported as None, never 0.0 -- "no measure recorded"
    and "this site has no defect" are different statements, and the row is
    JSON-serialisable either way (NaN is not valid JSON).

    `cls` NAMESPACES the row: "STUB"/"STRUT"/"BALL" for live sites addressed by
    their live member index, "QSTUB" for a Q-REAL condemned stub addressed by its
    forest member index -- two DISJOINT id spaces that can collide on the same
    integer, which is exactly why the class is part of the key."""
    key = (str(cls), int(site_id))
    prev = ledger.get(key)
    if prev is None:
        ledger[key] = dict(cls=str(cls), site_id=int(site_id), rung=str(rung),
                           reason=str(reason), first_sweep=int(sweep),
                           last_sweep=int(sweep), n_attempts=1,
                           measure_mm=(None if measure is None else float(measure)))
        return
    prev["rung"] = str(rung)
    prev["reason"] = str(reason)
    prev["last_sweep"] = int(sweep)
    prev["n_attempts"] = int(prev["n_attempts"]) + 1
    if measure is not None:
        prev["measure_mm"] = float(measure)


def unresolvable_rows(ledger: dict) -> tuple:
    """The ledger as a deterministic tuple of `_LEDGER_FIELDS` tuples, sorted by
    (class, site_id) -- the form the report and the engine log carry."""
    return tuple(tuple(ledger[k][f] for f in _LEDGER_FIELDS)
                 for k in sorted(ledger))


def _cand_geom_key(member: dict) -> tuple:
    """A hashable, revert-stable identity of a candidate's spliced geometry
    (round-9 p0/p1/r0/r1). A splice REVERT restores the member to bit-exact
    pre-splice geometry (`Member._restore`), so the reverted site re-derives the
    SAME candidate grid and the SAME lex-min winner on the next sweep -- keyed
    here so the cross-sweep ban recognises and skips it (defect-3 anti-churn)."""
    p0 = np.asarray(member["p0"], float); p1 = np.asarray(member["p1"], float)
    return (tuple(np.round(p0, 9).tolist()), tuple(np.round(p1, 9).tolist()),
            round(float(member["r0"]), 9), round(float(member["r1"]), 9))


def _ban_revalidate_fn(banned: dict, site_forest_idx: dict):
    """A ladder-selection `revalidate_fn` (returns True == candidate legal) that
    REJECTS any candidate whose (forest_idx, spliced-geometry) was reverted for a
    §5-law collision on an EARLIER sweep. This is the monotone anti-churn guard
    (defect-3): a revert restores exact geometry, so a naive loop re-picks the
    same lex-min winner and reverts it again forever ([...,1,1] churn + net-zero
    non-monotone sweeps). Banning the reverted candidate forces the ladder to the
    NEXT lex-min candidate or to an honest R2/R3 fall; the ban set only GROWS over
    finite candidate grids, so the sweep loop provably reaches a fixed point.
    None / empty ban => every candidate passes (behaviour-preserving)."""
    if not banned:
        return None

    def _ok(cand):
        fi = site_forest_idx.get(cand.site_id)
        if fi is None:
            return True
        return _cand_geom_key(cand.member) not in banned.get(fi, ())
    return _ok


def _forest_member_geom_key(fm) -> tuple:
    """The `_cand_geom_key` of a live `tree.Member`'s CURRENT stored geometry
    (p0/p1/r0/r1 as held on the forest, un-canonicalised)."""
    return _cand_geom_key(dict(p0=fm.p0, p1=fm.p1, r0=fm.r0, r1=fm.r1))


def _noop_skip_fn(forest, site_forest_idx: dict):
    """A ladder-selection `revalidate_fn` (D3(a) part 2) that REJECTS any candidate
    whose SPLICED geometry equals the member's CURRENT forest geometry -- a re-pick
    of already-applied geometry is a no-op churn commit, not progress. The splice
    (`_apply_member_geometry`) writes a candidate (canonical embed-at-p0) onto the
    forest member, swapping ends back when the member embeds at p1 (SM-STRUT feet);
    this predicate reconstructs that would-be forest geometry from the candidate and
    compares it to the member's current stored geometry, so the no-op test is
    invariant to the canonicalisation. Deterministic: reads only the live member
    geometry + rounded (round-9) geom keys -- no wall-clock, no iteration order."""
    def _ok(cand):
        fi = site_forest_idx.get(cand.site_id)
        if fi is None or not (0 <= fi < len(forest.members)):
            return True
        fm = forest.members[fi]
        m = cand.member
        p0 = np.asarray(m["p0"], float); p1 = np.asarray(m["p1"], float)
        if fm.embed_end == "p1":                 # candidate embeds p0; forest embeds p1
            would = _cand_geom_key(dict(p0=p1, p1=p0, r0=m["r1"], r1=m["r0"]))
        else:
            would = _cand_geom_key(dict(p0=p0, p1=p1, r0=m["r0"], r1=m["r1"]))
        return would != _forest_member_geom_key(fm)
    return _ok


def _compose_fns(*fns):
    """Compose ladder-selection `revalidate_fn`s: a candidate must pass EVERY
    non-None fn. Returns None when none are active (behaviour-preserving -- no
    filter installed, identical to the pre-compose contract)."""
    active = [f for f in fns if f is not None]
    if not active:
        return None

    def _ok(cand):
        return all(f(cand) for f in active)
    return _ok


# D3(a) per-site commit budget (fixed-point backstop). Progress-based retirement
# on the defect measure alone does NOT terminate on the treant: a re-aim can make
# tiny asymptotic progress -- `worst_chord` strictly shrinks every sweep without the
# member ever clearing rule-4 -- so the measure test (which permits any strict
# improvement) re-commits such a site every sweep, and neighbour co-moves re-flag a
# resolved site a sweep later (a gapped oscillation). A site that has been committed
# `_SITE_COMMIT_BUDGET` times without resolving has demonstrably stalled: its
# relocation is not converging, so it is retired (honest-unresolved). This BOUNDS
# per-site commits, which -- with the persistent measure baseline below -- makes the
# committable population strictly shrink and drives `committed_by_sweep` to zero
# (converged=true) inside the sweep cap. Calibrated on the archived treant (the
# smallest budget that yields a monotone trace converging before cap 8).
_SITE_COMMIT_BUDGET = 2


def _should_retire(now, pre, count, canon_mm=0.0) -> bool:
    """D3(a) progress-based retirement predicate. A site that was committed on an
    earlier sweep is RETIRED (honest-unresolved) when it REAPPEARS in the current
    snapshot (`now is not None`) and it is NOT making acceptable progress -- either:

      * it has already been committed `_SITE_COMMIT_BUDGET` times without resolving
        (a stalled site whose re-aims never clear it), OR
      * its defect measure did not shrink by at least the canonical chord/poke
        granularity `canon_mm` since its last commit (`not (now < pre - canon_mm)`)
        -- no meaningful progress, so re-committing it is churn.

    A site that RESOLVED (`now is None` -- it left the snapshot) is never retired.
    Pure float/int compare against snapshot-derived measures (deterministic; no
    wall-clock, no iteration-order dependence)."""
    if now is None:
        return False
    if count >= _SITE_COMMIT_BUDGET:
        return True
    return not (now < pre - canon_mm)


def _site_defect_measure(di: "DriverInput", key: tuple, non_exempt_set) -> Optional[float]:
    """The defect measure that TARGETS a site in `di`'s snapshot, or None if the
    site is no longer a live target there (resolved / absent). `key` is the
    namespaced retirement key:

      * ('M', mi) -- a class-T stub/strut member: the worst counted TRANSIT chord
        (mm, `di.gate.worst_chord`) iff `mi` is still non-exempt this sweep; None
        otherwise (the member left the non-exempt set -> genuinely resolved).
      * ('B', bid) -- a joint-ball breach: the ball's POKE-OUT depth (mm,
        `poke_out_mm`) iff `bid` still indexes a live breach; None otherwise.

    Derived only from snapshot integers + gate floats (no wall-clock, no
    iteration-order dependence) so the retirement decision is deterministic."""
    cls, sid = key
    if cls == "M":
        if sid in non_exempt_set:
            return float(di.gate.worst_chord.get(sid, 0.0))
        return None
    breaches = di.gate.ball_breaches
    if 0 <= sid < len(breaches):
        return float(breaches[sid].get("poke_out_mm", 0.0))
    return None


def _strut_shaft_printability_ok(cand, cfg) -> bool:
    """S1 strut candidate legality (D2, SPEC-P8-RELOCATE): reject a re-aim whose
    SHAFT tilts past the cfg overhang cone -- the SAME drift+taper printability
    predicate the commit gate enforces (`tree.printability_report`), reused
    verbatim on the single candidate member so an illegal candidate is pruned at
    SELECTION, not discovered at commit (cheaper, and keeps the honest-fall
    accounting right). Stubs are printability-exempt, so this is composed into the
    strut selection filter only. Candidate sets are <=75/site, so this per-
    candidate check is not a hot-path batch violation (it is the existing
    revalidate_fn contract's own per-candidate hook)."""
    return not tree.printability_report([cand.member], cfg)


def _compose_strut_revalidate_fn(ban_fn, noop_fn, cfg):
    """Compose the cross-sweep collision-revert ban (defect-3 anti-churn), the
    D3(a) no-op-candidate skip, and the D2 strut shaft-angle legality filter for
    the Phase-2 struts selection. Any subset may be active; a candidate must pass
    ALL active predicates."""
    def _ok(cand):
        if ban_fn is not None and not ban_fn(cand):
            return False
        if noop_fn is not None and not noop_fn(cand):
            return False
        return _strut_shaft_printability_ok(cand, cfg)
    return _ok


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
    newly_banned: list = field(default_factory=list)  # (forest_idx, geom_key) reverts
    # D3(a) part 1: sites SUCCESSFULLY committed this sweep (survived revalidate),
    # namespaced key -> (report_class, rung) -- the caller records their pre-commit
    # defect measure and retires any that reappear next sweep without improving.
    committed_sites: dict = field(default_factory=dict)
    # D6: sites whose splice was legal but STERILE (no strict defect-count decrease
    # over its influence set). Re-picking around a sterile candidate is the churn
    # that kept the sweep tail alive, so the caller RETIRES these outright --
    # namespaced key -> (report_class, rung), same shape as `committed_sites`.
    sterile_sites: dict = field(default_factory=dict)
    # ---- ruling (5) escalation rungs ---------------------------------------
    reseats: list = field(default_factory=list)            # (sid, ci, seat_c, dist)
    junction_reseats: list = field(default_factory=list)   # (cls, sid, delta, n_trials)
    # last rung actually ATTEMPTED per still-unresolved site, keyed
    # (report_class, site_id) -- what the persistent ledger records as the
    # honest end of the ladder for that site this sweep.
    final_rung: dict = field(default_factory=dict)
    # LEDGER-READY honest failures: (ledger_class, site_id, measure_key). Distinct
    # from `unresolved_items` because the ledger NAMESPACES a Q-REAL stub as
    # "QSTUB" -- its site id is a forest member index, a different id space from a
    # class-T stub's live member index, and the two can collide on one integer.
    # `measure_key` is the `_site_defect_measure` key, or None when the site has
    # no live measure (a Q-REAL stub is not in the live member list at all).
    ledger_obs: list = field(default_factory=list)


def _relocation_sweep(di: DriverInput, forest, geom, cfg,
                      banned: Optional[dict] = None,
                      plate_z=None, unrouted=(), seed_index=None,
                      contacts=None) -> _SweepResult:
    """Run ONE Phase 1 (stubs+Q-REAL) -> Phase 2 (struts) -> Phase 3 (balls) pass
    over the FROZEN sites in `di`, splicing accepted re-aims/slides into `forest`
    under the transactional revalidate protocol. Pure single-sweep mechanics --
    the caller (`run_phase8`) re-derives `di` from the mutated forest for the next
    sweep, which is the F1 convergence: the ladder runs on its OWN output until a
    sweep changes nothing (`n_committed == 0`, the true-no-op fixed point).

    LADDER ORDER (§3.1, never re-ordered; ruling (5) adds the last two rungs):
    a rung is entered ONLY when the rung above it yields ZERO legal candidates
    for that site.

        stubs   R1 tip re-aim -> R2 surface-seed contact re-seat -> D5 parent-
                junction re-seat -> R3 honest failure
        struts  S1 foot re-aim -> D5 parent-junction re-seat -> S3 honest failure
        balls   B1 axial slide -> D5 junction re-seat -> B3 honest failure

    Every rung commits through the SAME guarded transaction (`splice_reaims` for
    shaft geometry, `_translate_joint` + `_ball_slide_guard_ok` for a junction),
    so the D6 zero-new-violation invariant and its strict defect-count decrease
    bind the new rungs exactly as they bind R1 -- the escalation buys resolution
    COMPLETENESS without touching the monotonicity the fixed point rests on.

    `seed_index` (a `SurfaceSeedIndex`) and `contacts` enable R2; without them
    (unit/legacy callers, or a caller that supplied no island domain) the ladder
    is exactly the R1/S1/B1 ladder it was.

    Records a per-site provenance row (F6): (cls, site_id, forest_idx, disposition,
    rung) so the 34-member-class collateral is attributable from artifacts."""
    unresolved_items: list = []
    reverted: list = []
    pocket_labels: list = []
    ledger_rows: list = []
    n_committed = 0
    newly_banned: list = []
    committed_sites: dict = {}            # D3(a): key -> (report_class, rung), committed
    sterile_sites: dict = {}              # D6: key -> (report_class, rung), no-progress
    reseats: list = []                    # ruling (5) R2 commits
    junction_reseats: list = []           # ruling (5) D5 commits
    final_rung: dict = {}                 # (ledger_cls, sid) -> last rung ATTEMPTED
    ledger_obs: list = []                 # (ledger_cls, sid, measure_key)
    # anti-churn ban filter (defect-3): any candidate in `banned` is skipped at
    # selection, so the ladder picks a DIFFERENT one or falls honestly. `run_phase8`
    # owns that set's lifetime -- see its comment for why D6 makes it sweep-local.
    ban_fn = _ban_revalidate_fn(banned, di.site_forest_idx)
    # D3(a) part 2 no-op-candidate skip: a candidate whose spliced geometry equals
    # the member's CURRENT forest geometry is a no-op churn commit -- reject it at
    # selection so a re-pick of already-applied geometry is never "committed".
    noop_fn = _noop_skip_fn(forest, di.site_forest_idx)
    stub_sel_fn = _compose_fns(ban_fn, noop_fn)

    # ---- shared escalation bookkeeping --------------------------------------
    def _drop_unresolved(cls, sid):
        """Retract an honest-failure row a LOWER rung has since resolved. The R1
        disposal records the failure the moment R1 fails (that is what makes the
        provenance ledger complete); when R2 or D5 then resolves the site the row
        must go, or the report would count one site as both resolved and
        unresolved."""
        hit = False
        for i, it in enumerate(unresolved_items):
            if it[0] == cls and it[1] == sid:
                del unresolved_items[i]
                hit = True
                break
        for i, ob in enumerate(ledger_obs):
            # only ever called for LIVE sites, so the class match is exact and a
            # Q-REAL row ("QSTUB") sharing the integer is never touched.
            if ob[0] == cls and ob[1] == sid:
                del ledger_obs[i]
                break
        return hit

    def _site_node_and_axis(fi):
        """The parent-JUNCTION node of a live member and the shaft axis pointing
        at it: the TREE-SIDE endpoint (the non-embedded end -- a stub's p1, a
        strut foot's p0), its radius, and the unit axis from the embedded end
        toward it. This is the node D5 re-seats; the embedded end (and therefore
        the demand, the carried area and every coverage number) does not move."""
        fm = forest.members[fi]
        embed = fm.embed_end
        if embed == "p1":
            node = np.asarray(fm.p0, float); other = np.asarray(fm.p1, float)
            r_node = float(fm.r0)
        else:
            node = np.asarray(fm.p1, float); other = np.asarray(fm.p0, float)
            r_node = float(fm.r1)
        return node, r_node, phase8._unit(node - other)

    def _try_d5_at(cls, sid, fi, node, axis, tol):
        """D5 at one junction NODE. Returns True iff the re-seat committed (and
        therefore strictly reduced the influence set's defect count -- the guard
        refuses anything else). `fi` is only the ledger's member attribution;
        the move is a NODE translation and rides every member meeting there."""
        nonlocal n_committed
        if not getattr(cfg, "p8_junction_reseat_enable", True):
            return False
        delta, n_trials, n_touched = try_junction_reseat(
            forest, geom, cfg, plate_z, unrouted, fi, node, axis, tol=tol)
        final_rung[(cls, sid)] = "D5"
        if delta is None:
            ledger_rows.append((cls, sid, fi, "D5-EXHAUSTED", "D5"))
            return False
        n_committed += n_touched
        junction_reseats.append((cls, sid, np.asarray(delta, float), int(n_trials)))
        ledger_rows.append((cls, sid, fi, "D5", "D5"))
        return True

    def _try_d5(cls, sid, fi):
        """D5 for one MEMBER site (stub / strut): re-seat its parent junction --
        the tree-side node -- holding the embedded end fixed."""
        if fi is None or not (0 <= fi < len(forest.members)):
            return False
        node, r_node, axis = _site_node_and_axis(fi)
        return _try_d5_at(cls, sid, fi, node, axis, max(0.05, 0.5 * r_node))

    # ---- Phase 1: stubs (class-T stubs UNION Q-REAL), pocket solver + isolated.
    stub_resolved = stub_unresolved = 0
    qreal_resolved = qreal_unresolved = 0
    r1_failed: list = []                  # live stub site_ids with an EMPTY R1 set

    def _dispose_stub(mid, resolved, rung):
        nonlocal stub_resolved, stub_unresolved, qreal_resolved, qreal_unresolved
        if mid >= Q_SITE_OFFSET:
            if resolved:
                qreal_resolved += 1
            else:
                qreal_unresolved += 1
                unresolved_items.append(("STUB", mid - Q_SITE_OFFSET, rung))
                # a Q-REAL stub is NOT in the live member list, so it has no live
                # defect measure -- ledgered as "QSTUB" with measure None.
                final_rung[("QSTUB", mid - Q_SITE_OFFSET)] = rung
                ledger_obs.append(("QSTUB", mid - Q_SITE_OFFSET, None))
            return None                       # Q-REAL: never spliced (see run_phase8)
        if resolved:
            stub_resolved += 1
            return di.site_forest_idx.get(mid)
        stub_unresolved += 1
        unresolved_items.append(("STUB", mid, rung))
        final_rung[("STUB", mid)] = rung
        ledger_obs.append(("STUB", mid, ("M", mid)))
        r1_failed.append(mid)                 # escalate: R2, then D5
        return None

    if di.stub_sites:
        report = phase8.run_pocket_solver(di.stub_sites, geom, cfg,
                                          revalidate_fn=stub_sel_fn)
        pocket_labels = [pk.label for pk in report.pockets]
        accepted = []
        acc_site = {}                     # fi -> (site_id, rung) for committed tracking
        for pk in report.pockets:
            for mid, mr in pk.members.items():
                ok = mr.status == phase8.PocketStatus.COMMITTED and bool(mr.committed_cand)
                fi = _dispose_stub(mid, ok, mr.escalate_rung or "R2")
                if fi is not None:
                    accepted.append((fi, mr.committed_cand.member, _orig_embed(forest, fi)))
                    acc_site[fi] = (mid, mr.escalate_rung or "R1")
                ledger_rows.append(("STUB", mid, fi, "POCKET",
                                    mr.escalate_rung or ("R1" if ok else "R2")))
        for mid, r1res in report.isolated.items():
            ok = r1res.winner is not None
            fi = _dispose_stub(mid, ok, "R2")
            if fi is not None:
                accepted.append((fi, r1res.winner.member, _orig_embed(forest, fi)))
                acc_site[fi] = (mid, "R1")
            ledger_rows.append(("STUB", mid, fi, "ISOLATED", "R1" if ok else "R2"))
        acc_by_fi = {fi: cm for fi, cm, _emb in accepted}
        reasons: dict = {}
        n_ok, rv = splice_reaims(forest, geom, cfg, accepted,
                                 plate_z=plate_z, unrouted=unrouted,
                                 reasons_out=reasons)
        n_committed += n_ok
        reverted.extend(("STUB", fi) for fi in rv)
        stub_resolved -= len(rv); stub_unresolved += len(rv)
        rv_set = set(rv)
        for fi, (mid, rung) in acc_site.items():
            if fi not in rv_set:          # spliced AND survived revalidate -> committed
                committed_sites[("M", mid)] = ("STUB", rung)
            elif reasons.get(fi) == "no-progress":       # D6: sterile -> retire
                sterile_sites[("M", mid)] = ("STUB", "D6-sterile")
        for fi in rv:
            why = reasons.get(fi, "violation")
            ledger_rows.append(("STUB", None, fi, "REVERTED", why))
            # only a NEW-VIOLATION revert bans the candidate (try a different one);
            # a sterile one retires the SITE instead -- banning there would just
            # walk the ladder down its candidate list, sweep after sweep.
            if why == "violation" and fi in acc_by_fi:
                newly_banned.append((fi, _cand_geom_key(acc_by_fi[fi])))

    # ---- Phase 1 rung R2: SURFACE-SEED CONTACT RE-SEAT (ruling (5)) ---------
    # Entered ONLY for live stub sites whose R1 legal set was EMPTY (Q-REAL sites
    # are never spliced by this driver, so they never escalate here). ONE batched
    # seed search over every escalated site, ONE batched candidate evaluation
    # through the shared certificate, then ONE guarded splice batch -- so a
    # re-seat is judged by exactly the law and the invariant a re-aim is.
    r2_pending = list(r1_failed)
    if (r2_pending and seed_index is not None and contacts is not None
            and getattr(cfg, "p8_reseat_enable", True)):
        want = set(r2_pending)
        r2_sites = [s for s in di.stub_sites if s.site_id in want]
        seeds = seed_index.seeds_for(r2_sites, di.contact_of_site, contacts,
                                     unrouted, cfg)
        r2res = phase8.evaluate_r2(r2_sites, seeds, geom, cfg,
                                   revalidate_fn=stub_sel_fn)
        accepted = []
        acc_site = {}                       # fi -> (site_id, winner Candidate)
        for sid in sorted(r2res):
            final_rung[("STUB", sid)] = "R2"
            res = r2res[sid]
            if res.winner is None:
                continue
            fi = di.site_forest_idx.get(sid)
            if fi is None:
                continue
            accepted.append((fi, res.winner.member, _orig_embed(forest, fi)))
            acc_site[fi] = (sid, res.winner)
        acc_by_fi = {fi: cm for fi, cm, _emb in accepted}
        reasons = {}
        n_ok, rv = splice_reaims(forest, geom, cfg, accepted,
                                 plate_z=plate_z, unrouted=unrouted,
                                 reasons_out=reasons)
        n_committed += n_ok
        rv_set = set(rv)
        for fi, (sid, cand) in sorted(acc_site.items()):
            if fi in rv_set:
                reverted.append(("STUB", fi))
                why = reasons.get(fi, "violation")
                ledger_rows.append(("STUB", sid, fi, "REVERTED", "R2:" + why))
                if why == "violation" and fi in acc_by_fi:
                    newly_banned.append((fi, _cand_geom_key(acc_by_fi[fi])))
                elif why == "no-progress":
                    sterile_sites[("M", sid)] = ("STUB", "D6-sterile")
                continue
            # COMMITTED. The demand MIGRATED: write the new seat back into the
            # contact record, or the coverage / island-birth gates (which read
            # contact POSITIONS, not member geometry) would keep judging a point
            # no stub is standing on. The seat was already cleared by the
            # domain + coverage/birth guard at seed selection, so this write can
            # never be the thing that fails a coverage gate.
            ci = di.contact_of_site.get(sid)
            if ci is not None and 0 <= ci < len(contacts):
                rec = contacts[ci]
                as_list = isinstance(rec.get("position"), list)
                pos = np.asarray(cand.seat_c, float)
                nrm = np.asarray(cand.seat_n, float)
                rec["position"] = pos.tolist() if as_list else pos
                rec["normal"] = nrm.tolist() if as_list else nrm
            reseats.append((sid, ci, np.asarray(cand.seat_c, float),
                            float(cand.reseat_dist_mm)))
            stub_resolved += 1
            stub_unresolved -= 1
            _drop_unresolved("STUB", sid)
            r2_pending.remove(sid)
            committed_sites[("M", sid)] = ("STUB", "R2")
            ledger_rows.append(("STUB", sid, fi, "R2", "R2"))

    # ---- Phase 1 rung D5: PARENT-JUNCTION RE-SEAT (ruling (5)) --------------
    # The last rung before R3 honest failure, for stubs neither re-aim nor
    # re-seat could place. Sites are taken in the pocket solver's own total
    # commit-priority order (worst frozen chord desc, site id asc) so the
    # sequence is deterministic and worst-first.
    if r2_pending and getattr(cfg, "p8_junction_reseat_enable", True):
        wfc = di.gate.worst_chord
        for sid in sorted(r2_pending, key=lambda i: (-float(wfc.get(i, 0.0)), i)):
            if _try_d5("STUB", sid, di.site_forest_idx.get(sid)):
                stub_resolved += 1
                stub_unresolved -= 1
                _drop_unresolved("STUB", sid)
                committed_sites[("M", sid)] = ("STUB", "D5")

    # ---- Phase 2: interior struts (SM-STRUT S1), spliced AFTER stubs.
    strut_resolved = strut_unresolved = 0
    s1_failed: list = []                  # strut site_ids with an EMPTY S1 set
    if di.strut_sites:
        sres = phase8.run_phase2_struts(
            di.strut_sites, geom, cfg,
            revalidate_fn=_compose_strut_revalidate_fn(ban_fn, noop_fn, cfg))
        accepted = []
        acc_site = {}                     # fi -> (site_id, rung) for committed tracking
        for sid, sr in sres.items():
            if sr.rung == "S1" and sr.winner is not None:
                fi = di.site_forest_idx.get(sid)
                accepted.append((fi, sr.winner.member, _orig_embed(forest, fi)))
                if fi is not None:
                    acc_site[fi] = (sid, "S1")
                strut_resolved += 1
                ledger_rows.append(("STRUT", sid, fi, "S1", "S1"))
            else:
                strut_unresolved += 1
                unresolved_items.append(("STRUT", sid, sr.rung))
                final_rung[("STRUT", sid)] = sr.rung
                ledger_obs.append(("STRUT", sid, ("M", sid)))
                s1_failed.append(sid)         # escalate: D5 (see below)
                ledger_rows.append(("STRUT", sid, None, "UNRESOLVED", sr.rung))
        acc_by_fi = {fi: cm for fi, cm, _emb in accepted}
        reasons = {}
        n_ok, rv = splice_reaims(forest, geom, cfg, accepted,
                                 plate_z=plate_z, unrouted=unrouted,
                                 reasons_out=reasons)
        n_committed += n_ok
        reverted.extend(("STRUT", fi) for fi in rv)
        strut_resolved -= len(rv); strut_unresolved += len(rv)
        rv_set = set(rv)
        for fi, (sid, rung) in acc_site.items():
            if fi not in rv_set:          # spliced AND survived revalidate -> committed
                committed_sites[("M", sid)] = ("STRUT", rung)
            elif reasons.get(fi) == "no-progress":       # D6: sterile -> retire
                sterile_sites[("M", sid)] = ("STRUT", "D6-sterile")
        for fi in rv:
            why = reasons.get(fi, "violation")
            ledger_rows.append(("STRUT", None, fi, "REVERTED", why))
            if why == "violation" and fi in acc_by_fi:
                newly_banned.append((fi, _cand_geom_key(acc_by_fi[fi])))

    # ---- Phase 2 rung D5: PARENT-JUNCTION RE-SEAT for struts (ruling (5)).
    # A strut foot is a FIXED model-surface anchor (amendment E), so S1 can only
    # pivot the shaft about it; when that cone is empty the remaining freedom is
    # the other end -- the junction where the strut meets the tree. Same guarded
    # co-moving transaction, so both adjoining segments are re-judged by the §5
    # law and the D6 invariant, and connectivity is preserved by construction.
    if s1_failed and getattr(cfg, "p8_junction_reseat_enable", True):
        wfc = di.gate.worst_chord
        for sid in sorted(s1_failed, key=lambda i: (-float(wfc.get(i, 0.0)), i)):
            if _try_d5("STRUT", sid, di.site_forest_idx.get(sid)):
                strut_resolved += 1
                strut_unresolved -= 1
                _drop_unresolved("STRUT", sid)
                committed_sites[("M", sid)] = ("STRUT", "D5")

    # ---- Phase 3: ball breaches (SM-BALL B1), LAST -- reads FINAL shaft geometry.
    ball_resolved = ball_unresolved = 0
    b1_failed: list = []                  # ball ids B1 could not place
    if di.ball_sites:
        def _slide_revalidate(site, sl):
            center = np.asarray(site.center, float)
            delta = np.asarray(sl.center, float) - center
            tol = max(0.05, 0.5 * site.r)
            # pre-slide index + universe (D2 guard baseline), captured BEFORE move.
            idx_arr, end_arr, pt_arr = forest.endpoint_index()
            pre_members, _live_fi, pos_of = _live_universe(forest, unrouted)
            saved = _translate_joint(forest, center, delta, tol=tol)
            if not saved:
                return False
            viol = forest.revalidate(geom, cfg)
            touched = {fi for fi, _s in saved}
            ok = (not any(v["member_idx"] in touched for v in viol)
                  and _ball_slide_guard_ok(forest, geom, cfg, plate_z, saved,
                                           pre_members, pos_of,
                                           idx_arr, end_arr, pt_arr))
            for fi, snapm in saved:              # trial only -- always undo here
                forest.members[fi]._restore(snapm)
            forest.revalidate(geom, cfg)
            return ok
        bres = phase8.run_phase3_balls(di.ball_sites, geom, cfg,
                                       slide_revalidate_fn=_slide_revalidate)
        for bid, br in bres.items():
            site = next((s for s in di.ball_sites if s.ball_id == bid), None)
            if br.rung == "B1" and br.slide is not None and site is not None:
                center = np.asarray(site.center, float)
                delta = np.asarray(br.slide.center, float) - center
                tol = max(0.05, 0.5 * site.r)
                idx_arr, end_arr, pt_arr = forest.endpoint_index()
                pre_members, _live_fi, pos_of = _live_universe(forest, unrouted)
                saved = _translate_joint(forest, center, delta, tol=tol)
                viol = forest.revalidate(geom, cfg)
                touched = {fi for fi, _s in saved}
                clean = (not any(v["member_idx"] in touched for v in viol)
                         and _ball_slide_guard_ok(forest, geom, cfg, plate_z, saved,
                                                  pre_members, pos_of,
                                                  idx_arr, end_arr, pt_arr))
                if not clean:
                    # D3(a) part 3 ball-ban parity: bar re-attempting the SAME slide
                    # on the touched joint members next sweep -- append each touched
                    # member's POST-slide geometry key (captured BEFORE the restore)
                    # to newly_banned, exactly as the STUB/STRUT revert paths do.
                    for fi, snapm in saved:
                        newly_banned.append((fi, _cand_geom_key(forest.members[fi]._snapshot())))
                        forest.members[fi]._restore(snapm)
                    forest.revalidate(geom, cfg)
                    ball_unresolved += 1
                    unresolved_items.append(("BALL", bid, "B1-reverted"))
                    final_rung[("BALL", bid)] = "B1-reverted"
                    ledger_obs.append(("BALL", bid, ("B", bid)))
                    b1_failed.append(bid)
                    ledger_rows.append(("BALL", bid, None, "REVERTED", "B1"))
                else:
                    ball_resolved += 1
                    n_committed += len(touched)
                    committed_sites[("B", bid)] = ("BALL", "B1")
                    ledger_rows.append(("BALL", bid, None, "B1", "B1"))
            else:
                ball_unresolved += 1
                unresolved_items.append(("BALL", bid, br.rung))
                final_rung[("BALL", bid)] = br.rung
                ledger_obs.append(("BALL", bid, ("B", bid)))
                b1_failed.append(bid)
                ledger_rows.append(("BALL", bid, None, "UNRESOLVED", br.rung))

    # ---- Phase 3 rung D5: JUNCTION RE-SEAT for unresolved ball breaches.
    # B1's candidate space is the member AXIS only -- a strict 1-D subset of the
    # D5 lattice, which adds the two local-frame perpendiculars. So D5 on a ball
    # is not a retry of B1's question, it is the rest of the question, and that
    # is why a B1-REVERTED ball escalates here as well as an empty-B1 one: the
    # revert says "no clean position ALONG THE AXIS", not "no clean position".
    if b1_failed and getattr(cfg, "p8_junction_reseat_enable", True):
        site_by_bid = {s.ball_id: s for s in di.ball_sites}
        for bid in sorted(b1_failed):
            site = site_by_bid.get(bid)
            if site is None:
                continue
            if _try_d5_at("BALL", bid, None, np.asarray(site.center, float),
                          np.asarray(site.root_dir, float),
                          max(0.05, 0.5 * float(site.r))):
                ball_resolved += 1
                ball_unresolved -= 1
                _drop_unresolved("BALL", bid)
                committed_sites[("B", bid)] = ("BALL", "D5")

    return _SweepResult(
        stub_resolved=stub_resolved, stub_unresolved=stub_unresolved,
        qreal_resolved=qreal_resolved, qreal_unresolved=qreal_unresolved,
        strut_resolved=strut_resolved, strut_unresolved=strut_unresolved,
        ball_resolved=ball_resolved, ball_unresolved=ball_unresolved,
        reverted=reverted, pocket_labels=pocket_labels,
        unresolved_items=unresolved_items, ledger_rows=ledger_rows,
        n_committed=n_committed, newly_banned=newly_banned,
        committed_sites=committed_sites, sterile_sites=sterile_sites,
        reseats=reseats, junction_reseats=junction_reseats, final_rung=final_rung,
        ledger_obs=ledger_obs)


def run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z, geom,
               cfg, mesh, unrouted=(), quarantine_sites=None,
               census_authority=frozenset(), log=None,
               member_list_fn=None, frozen_snapshot=None,
               progress=None, islands=None, island_births=None,
               carry_ledger=None) -> Phase8Report:
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

    CONVERGENCE (fix-round F1 + D3(a)): the three phases run as ONE sweep, then the
    sweep repeats -- re-deriving the frozen defect snapshot from the MUTATED forest
    (`member_list_fn` re-reads the live member/ball lists) -- until a sweep commits
    ZERO splices (the true-no-op fixed point) or every remaining target is retired,
    bounded by `cfg.p8_max_relocate_sweeps` (default 8). PROGRESS-BASED RETIREMENT
    (D3(a)) is what guarantees termination at a HONEST fixed point: a site committed
    one sweep that reappears the next without its defect measure (transit chord for
    stubs/struts, poke depth for balls) strictly shrinking made no progress and is
    RETIRED for the rest of the run (filtered out of every later sweep), so the
    committable population strictly shrinks and `converged` is reached before the
    cap instead of churning to it. This converts the previously-discarded first
    relocation pass into cumulative progress and makes the §6.10 idempotence gate
    real: a subsequent `run_phase8` over the fixed-point forest builds the same
    snapshot, commits zero splices in its first sweep, and returns a byte-identical
    no-op. Deterministic: no RNG, no wall-clock in the loop body; termination is
    driven by integer commit counts + snapshot-derived defect measures only, and
    the retired set is built from sorted keys (no iteration-order dependence).

    `member_list_fn` (the engine passes `lambda: live_member_lists(forest, ...)`):
    re-reads the live member + ball lists from the current forest state between
    sweeps. When None (unit/legacy callers), the loop runs EXACTLY ONE sweep on the
    passed-in `members`/`balls` -- byte-identical to the pre-convergence driver.

    `frozen_snapshot` (the treant target law, §5.3 / defect-5-6): when supplied it
    is applied by `construct_sites` ONLY on the sweep whose geometry IS the fixture's
    source (the pre-relocation treant, digest-gated) -- so the FIRST sweep targets
    the census-authoritative 143/50 (incl. the 6 census-only boundary members the
    live kernel gate exempts), while every subsequent (mutated-geometry) sweep and
    the §6.10 idempotence pass fall back to the live gate.

    Fixed point (defect-3): a collision-revert ban carried across sweeps forbids
    re-picking a candidate that a §5-law splice already reverted, so the per-sweep
    commit count is non-increasing (`sweep_tail_monotone`) and the loop reaches a
    true fixed point -- a second `run_phase8` over the accepted geometry re-derives
    an empty snapshot and is a provable no-op."""
    t0 = time.time()
    log = log if log is not None else {}
    if quarantine_sites is None:
        quarantine_sites = quarantine_sites_from_forest(forest, contacts)

    # ruling (5): the R2/D5 surface-seed source, built ONCE for the whole run
    # (the mesh does not move) and reused by every sweep -- rebuilding the face
    # KD-tree per sweep would be exactly the per-item cost the batch-first rule
    # forbids. Without `islands` there is no inviolable class domain to keep a
    # re-seated demand inside, so R2 is simply not offered (see SurfaceSeedIndex).
    seed_index = None
    if (mesh is not None and islands is not None
            and getattr(cfg, "p8_reseat_enable", True)):
        seed_index = SurfaceSeedIndex(mesh, islands=islands,
                                      island_births=island_births)
    # ruling (5): the PERSISTENT formally-unresolvable ledger. Caller-owned when
    # supplied (`carry_ledger`), so its rows survive not just sweeps but PASSES --
    # a site the ladder gave up on in an earlier relocation call keeps its
    # first-seen sweep and accumulates attempts instead of being re-reported as a
    # fresh discovery.
    unresolvable = carry_ledger if carry_ledger is not None else {}
    reseats_all: list = []
    junction_reseats_all: list = []

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
    sweeps_run = 0
    reached_fp = False                          # True iff a further sweep is a no-op
    first_target_source = "live-gate"
    # Collision-revert ban (defect-3 anti-churn): forest_idx -> set of refused
    # candidate geom-keys, consulted at ladder SELECTION. Under D6 it is no longer
    # a cross-sweep set: it is cleared on every committing sweep (see there) and the
    # loop stops at the first non-committing one, so it never survives into a later
    # sweep -- which is exactly what makes every sweep, and in particular the
    # terminating one, a pure function of the geometry (idempotence). It stays as
    # the refused-candidate provenance counter and as the selection hook the
    # within-sweep ladder-depth retry (the identified next lever for resolution
    # COMPLETENESS) will use. `n_banned_total` is the cumulative count for the
    # report, since `banned` itself no longer accumulates.
    banned: dict = {}
    n_banned_total = 0
    # D3(a) progress-based retirement (fixed point). `retired` GROWS monotonically:
    # a committed site that reappears WITHOUT acceptable progress -- its defect
    # measure not shrinking by the canonical granularity since its last commit, or
    # it has exhausted its per-site commit budget -- is retired for the rest of the
    # run (filtered out of every later sweep's site lists), so the committable
    # population strictly shrinks and the loop reaches an honest no-op fixed point.
    # `commit_measure` is a PERSISTENT per-site ledger (key -> (report_class, rung,
    # measure_at_last_commit, commit_count)), checked at the TOP of every sweep so a
    # site that reappears after a GAP (a neighbour co-move re-flagged a momentarily-
    # resolved site) is caught too, not only on the immediately-next sweep. All keys
    # are namespaced -- ('M', mi) for a stub/strut member, ('B', bid) for a ball --
    # so a stub mi and a ball bid that share an integer never conflate.
    retired: set = set()
    retired_info: dict = {}                      # key -> (report_class, last_rung)
    commit_measure: dict = {}                    # key -> (cls, rung, measure, count)
    canon_mm = float(getattr(cfg, "p8_chord_canon_micron", 1)) * 1e-3
    defects_by_sweep: list = []                  # D6 monotonicity trace (evidence)

    for _sweep in range(max_sweeps):
        if _sweep > 0:
            members, balls = member_list_fn()   # re-read the mutated forest
        di = construct_sites(forest, members, balls, contacts, carried, cfg, geom,
                             mesh, unrouted=unrouted, quarantine_sites=quarantine_sites,
                             census_authority=census_authority,
                             frozen_snapshot=frozen_snapshot)
        ne_set = frozenset(di.snapshot.non_exempt)
        # ---- D3(a) part 1: progress-based retirement. Any previously-committed site
        # that REAPPEARS in THIS sweep's snapshot without acceptable progress (measure
        # not shrunk by the canonical granularity since its last commit, OR its commit
        # budget is exhausted) is retired (honest-unresolved). Iterate the ledger keys
        # in sorted order so the retired set is iteration-order-free.
        for key in sorted(commit_measure):
            if key in retired:
                continue
            rcls, rung, pre, count = commit_measure[key]
            if _should_retire(_site_defect_measure(di, key, ne_set), pre, count, canon_mm):
                retired.add(key)
                retired_info[key] = (rcls, rung)
        # filter construct_sites' output against the retired set (do NOT touch
        # construct_sites itself) so a retired site never re-enters a sweep.
        if retired:
            di.stub_sites = [s for s in di.stub_sites
                             if ("M", s.site_id) not in retired]
            di.strut_sites = [s for s in di.strut_sites
                              if ("M", s.site_id) not in retired]
            di.ball_sites = [s for s in di.ball_sites
                             if ("B", s.ball_id) not in retired]
        if first_summary is None:
            first_summary = di.snapshot.summary()
            first_sites = (len(di.stub_sites), len(di.strut_sites), len(di.ball_sites))
            first_moot = tuple(di.snapshot.moot)
            first_target_source = di.target_source
        if not (di.stub_sites or di.strut_sites or di.ball_sites):
            reached_fp = True                   # snapshot emptied / all retired -> done
            break                               # nothing left to work -> fixed point
        # D6 monotonicity evidence: the GLOBAL defect population this sweep starts
        # from. The commit invariant makes every surviving commit strictly decrease
        # it, so this trace is non-increasing and strictly decreasing on any sweep
        # that committed -- the fixed point is reached, not merely capped.
        defects_by_sweep.append(len(di.snapshot.non_exempt)
                                + len(di.snapshot.ball_breaches))
        entered_any = True

        sw = _relocation_sweep(di, forest, geom, cfg, banned=banned,
                               plate_z=plate_z, unrouted=unrouted,
                               seed_index=seed_index, contacts=contacts)
        reseats_all.extend((int(sid), (None if ci is None else int(ci)),
                            float(pos[0]), float(pos[1]), float(pos[2]), float(d))
                           for sid, ci, pos, d in sw.reseats)
        junction_reseats_all.extend(
            (str(cls), int(sid), float(dv[0]), float(dv[1]), float(dv[2]), int(nt))
            for cls, sid, dv, nt in sw.junction_reseats)
        # ruling (5): fold THIS sweep's honest failures into the persistent
        # ledger -- one row per site, updated (never duplicated), carrying the
        # last rung the ladder actually reached and the live defect measure.
        for cls, sid, mkey in sw.ledger_obs:
            ledger_touch(unresolvable, cls, sid,
                         sw.final_rung.get((cls, sid), "?"),
                         "ladder-exhausted", _sweep,
                         None if mkey is None
                         else _site_defect_measure(di, mkey, ne_set))
        # accumulate this sweep's collision-reverts into the monotone ban set so
        # the NEXT sweep cannot re-pick the reverting winner (defect-3 anti-churn).
        for fi, gk in sw.newly_banned:
            banned.setdefault(fi, set()).add(gk)
        n_banned_total += len(sw.newly_banned)
        if sw.n_committed:
            # D6 IDEMPOTENCE: a ban records "this candidate was illegal against THAT
            # geometry". A commit MOVES the geometry, so every such record is stale;
            # carrying it forward lets a stale ban HIDE a candidate that is now legal
            # and progressing -- which is precisely how the loop stopped one commit
            # early and failed the §6.10 strict-no-op gate (the acceptance run's
            # second pass found the hidden commit: committed_by_sweep=[1,0]).
            # Clearing here also means every sweep -- in particular the TERMINATING
            # zero-commit one -- runs with an EMPTY ban set, so it is bit-for-bit the
            # sweep a FRESH `run_phase8` executes on that geometry. Combined with the
            # commit invariant (a sweep that commits nothing cannot commit anything
            # on a re-run either, the sweep being a pure function of the geometry),
            # that makes the fixed point IDEMPOTENT BY CONSTRUCTION, not by
            # measurement. Termination is unaffected: it rests on the invariant's
            # strict global-defect decrease, never on the ban.
            banned.clear()
        # D6: a site whose splice was LEGAL but STERILE (no strict defect-count
        # decrease over its influence set) is retired immediately -- honest-
        # unresolved. This is the fix for the neighbour cascade: re-picking around
        # a sterile candidate every sweep IS the churn, and the site cannot be
        # relocated to a better place by re-aim alone (that is directives 3/4's
        # R2 surface-seed / D5 junction re-seat territory, not this ladder's).
        for key in sorted(sw.sterile_sites):
            if key not in retired:
                retired.add(key)
                retired_info[key] = sw.sterile_sites[key]
        # update the persistent commit ledger for every site committed THIS sweep:
        # record its pre-commit defect measure (from THIS sweep's pre-mutation gate,
        # `di.gate`) as the new progress baseline and bump its commit count, so the
        # next sweep's retirement pass measures progress since this commit + enforces
        # the per-site budget.
        for key, (rcls, rung) in sw.committed_sites.items():
            m = _site_defect_measure(di, key, ne_set)
            prev = commit_measure.get(key)
            count = (prev[3] + 1) if prev is not None else 1
            commit_measure[key] = (rcls, rung, m if m is not None else float("inf"), count)
        sweeps_run += 1
        if progress is not None:      # timer-read ticker cell (SPEC-V5-PERF §4):
            progress.n = sweeps_run   # a monotone int write, no timing on this path
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
            provenance_ledger=(), moot_member_ids=tuple(first_moot),
            target_source=first_target_source, sweep_tail_monotone=True, n_banned=0,
            # a no-op pass resolves nothing and un-resolves nothing: the carried
            # ledger is reported VERBATIM, never cleared. That is what makes it a
            # persistent register across passes rather than a per-call snapshot.
            unresolvable_ledger=unresolvable_rows(unresolvable))

    # D3(a) part 1: retired sites are honest-unresolved at their last rung. Move
    # each out of the cumulative resolved tally into the unresolved tally + trace
    # (a retired site was committed once then reappeared without progress, so it is
    # not a genuine resolution). Sorted keys -> deterministic ordering.
    retired_sites: list = []
    for key in sorted(retired):
        rcls, rung = retired_info[key]
        _, sid = key
        # A D6-STERILE site was REVERTED, so its sweep already netted it out of the
        # cumulative `*_resolved` tally (+1 at disposal, -1 at revert) -- it is
        # honest-unresolved, but decrementing `*_resolved` again would double-count.
        # A D3(a) PROGRESS retirement did commit, so it does need the move.
        if rung != "D6-sterile":
            if rcls == "STUB":
                stub_resolved -= 1
            elif rcls == "STRUT":
                strut_resolved -= 1
            elif rcls == "BALL":
                ball_resolved -= 1
        if rcls == "STUB":
            stub_unresolved += 1
        elif rcls == "STRUT":
            strut_unresolved += 1
        elif rcls == "BALL":
            ball_unresolved += 1
        unresolved_items.append((rcls, sid, rung))
        retired_sites.append((rcls, sid, rung))
        # a retirement IS a formally-unresolvable outcome (the site was given up
        # on, honestly) -- it belongs in the persistent ledger with its reason,
        # not only in the retirement trace.
        ledger_touch(unresolvable, rcls, sid, rung,
                     "sterile" if rung == "D6-sterile" else "retired-no-progress",
                     sweeps_run)

    # converged iff a further sweep is provably a no-op: either the snapshot
    # emptied / every remaining target retired, or the last executed sweep
    # committed nothing. A sweep-cap exhaustion leaves `reached_fp` False -- the
    # ladder had NOT reached its fixed point, and idempotence is not claimed
    # (reported honestly).
    converged = reached_fp
    # anti-churn invariant (defect-3 + D3(a) + D6): with the no-op-candidate skip,
    # progress-based retirement, and above all the commit invariant's strict
    # global-defect decrease, the per-sweep commit count is non-increasing -- the
    # [116,57,9,2,3,2,1,1] revert/re-commit churn (a 2->3 rebound, a [...,1,1] tail)
    # can no longer occur.
    sweep_tail_monotone = all(a >= b for a, b in
                              zip(committed_by_sweep, committed_by_sweep[1:]))
    # cumulative over the run: `banned` itself is cleared after every
    # committing sweep (see there), so its live size is not the total.
    n_banned = n_banned_total

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
        provenance_ledger=tuple(ledger), moot_member_ids=tuple(first_moot),
        target_source=first_target_source,
        sweep_tail_monotone=sweep_tail_monotone, n_banned=n_banned,
        retired_sites=tuple(retired_sites),
        defects_by_sweep=tuple(defects_by_sweep),
        reseats=tuple(reseats_all),
        junction_reseats=tuple(junction_reseats_all),
        unresolvable_ledger=unresolvable_rows(unresolvable))


def _orig_embed(forest, fi):
    if fi is None or not (0 <= fi < len(forest.members)):
        return None
    return forest.members[fi].embed_end
