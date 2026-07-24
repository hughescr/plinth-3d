"""Phase-8 ORCHESTRATION DRIVER tests (phase8_driver.py).

SPEC: docs/specs/SPEC-P8-RELOCATE.md (the amended DRAFT at HEAD is the law).

These pin the DRIVER surfaces the task makes normative -- the integration that
wires the committed component machinery (phase8.py state machines + pocket
solver, p8_accept.py acceptance gates) to the engine's own forest + gate:

  * SITE CONSTRUCTION from the engine's OWN gate output (§3.0 / §5.3): the frozen
    snapshot cross-check on the archived forest v5e_run1.pkl must reproduce the
    acceptance targets pinned in phase8_relocation_input.json -- 143 non-exempt
    (incl. mi=445 per the §5.3 Layer-2 census-authority ruling), 77 REAL
    quarantine, 15 true ball breaches -- exactly;
  * the live-member <-> forest-member index bijection (splice addressing);
  * END-TO-END synthetic: a stub that welds through a fin is resolved by the
    driver (R1 re-aim spliced back), the gate goes clean, and a SECOND pass over
    the fixed geometry is a byte-identical no-op (the §6.10 idempotence gate);
  * DETERMINISM: two driver invocations over identical input emit bit-identical
    geometry (no RNG, no wall-clock).

The archived cross-check is @pytest.mark.kernel and skips gracefully when the
session scratchpad pickle / source STL is absent (CI without the artifacts); the
synthetic + determinism tests are self-contained on a tiny triangle-soup mesh via
the frozen Python ExactSurface oracle, so they run everywhere.
"""
import os
import json

import numpy as np
import trimesh
import pytest

from support_placement import phase8_driver as pd
from support_placement import phase8, verify, engine, tree, joints, p8_accept
from support_placement.geometry_exact import ExactSurface
from support_placement.config import SupportConfig


# --------------------------------------------------------------------------
# archived-artifact locations (session scratchpad, pinned by the spec).
# --------------------------------------------------------------------------
_SCR = ("/private/tmp/claude-501/-Users-craig-code-hughescr-dnd-campaigns/"
        "0ddf6d41-f89e-4672-9577-2690d62b260c/scratchpad")
_PKL = os.path.join(_SCR, "v5e_run1.pkl")
_STL = "/Users/craig/code/hughescr/dnd-campaigns/Weeping Treant v1.2.stl"
_INPUT_JSON = os.path.join(
    os.path.dirname(__file__), "..", "..", "docs", "artifacts",
    "audit-2026-07", "acceptance-run-2026-07-23", "phase8_relocation_input.json")


# --------------------------------------------------------------------------
# tiny deterministic fixtures (mirror test_phase8._box/_soup/_fin_mesh).
# --------------------------------------------------------------------------
def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _soup(*meshes):
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)


def _fin_mesh(fin_z=0.25, fin_z0=1.1):
    """A slab (top face z=0) plus a wide floating fin (underside `fin_z0`,
    thickness `fin_z`). A vertical stub embedded 0.3mm in the slab welds THROUGH
    the fin at full length -- the only unanchored rule-4 transit -- so only
    shortening the stub (R1's 0.5 length factor) clears it (the fin is 10x10 so
    no lean escapes it)."""
    slab = _box((20, 20, 5), center=(0, 0, -2.5))
    fin = _box((10, 10, fin_z), center=(0, 0, fin_z0 + fin_z / 2))
    return _soup(slab, fin)


@pytest.fixture
def cfg():
    return SupportConfig()


def _fin_forest(cfg, stub_len=2.0):
    """Build a minimal `tree.Forest` with ONE live contact stub that welds
    through the fin (a real transit defect), plus its contact list. Returns
    `(forest, contacts, carried, tip_r)`."""
    c = np.array([0.0, 0.0, 0.0])
    n0 = np.array([0.0, 0.0, 1.0])
    delta = cfg.contact_depth_mm
    p0 = c - n0 * delta                     # embedded tip (in the slab)
    p1 = c + n0 * stub_len                  # free end -- welds through the fin
    forest = tree.Forest()
    m = tree.Member(p0=p0, p1=p1, r0=0.2, r1=0.35, kind="stub",
                    owner=("contact", 0), embed_end="p0")
    mi = forest.add_member(m)
    forest.terminal_of[0] = mi
    contacts = [dict(position=c.copy(), normal=n0.copy(), island_id=0, area_mm2=5.0)]
    return forest, contacts, [5.0], [0.2]


# ==========================================================================
# 1. FROZEN-SNAPSHOT CROSS-CHECK (task step 3) -- the pinned acceptance targets.
# ==========================================================================
@pytest.mark.kernel
def test_snapshot_cross_check_archived_forest():
    """On the archived forest v5e_run1.pkl the driver's SITE CONSTRUCTION must
    reproduce the acceptance targets pinned in phase8_relocation_input.json --
    143 non-exempt (incl. mi=445 per §5.3 Layer 2), 77 REAL quarantine, 15 true
    ball breaches -- EXACTLY (task step 3). Also pins the §5.3 reconciliation the
    ruling rests on: the engine gate alone finds 142 firm-transit members and the
    SOLE census-only addition is mi=445 (the kernel's robust tip-anchor merge
    exempts it; the census counts it; the acceptance treats it as a target)."""
    import pickle
    if not (os.path.exists(_PKL) and os.path.exists(_STL) and os.path.exists(_INPUT_JSON)):
        pytest.skip("archived v5e_run1.pkl / source STL / input JSON not present")
    from support_placement.geometry_kernel import KernelSurface

    S = pickle.load(open(_PKL, "rb"))
    forest, cfg = S["forest"], S["cfg"]
    members, balls = S["members"], S["balls"]
    contacts, unrouted = S["contacts"], S["unrouted"]
    quarantine_sites = S["quarantine_sites"]

    mesh = trimesh.load(_STL, file_type="stl", process=False)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    geom = KernelSurface(mesh)

    d = json.load(open(_INPUT_JSON))
    census_ne = frozenset(d["printfloor_193"]["non_exempt_member_idxs"])

    di = pd.construct_sites(forest, members, balls, contacts, None, cfg, geom, mesh,
                            unrouted=unrouted, quarantine_sites=quarantine_sites,
                            census_authority=census_ne)
    snap = di.snapshot

    # --- the pinned acceptance targets, EXACTLY ---
    assert len(snap.non_exempt) == 143, len(snap.non_exempt)
    assert 445 in snap.non_exempt
    assert len(snap.quarantine_real) == 77
    assert len(snap.quarantine_artifact) == 3
    assert sorted(snap.quarantine_artifact) == [437, 440, 444]
    assert len(snap.ball_breaches) == 15

    # --- §5.3 reconciliation: engine gate 142 firm, sole census-only is 445 ---
    assert len(di.gate.firm_transit) == 142
    assert sorted(census_ne - di.gate.firm_transit) == [445]
    assert di.gate.firm_transit.issubset(census_ne)   # no engine-only firm member

    # --- site objects built for every target, correctly typed ---
    #   129 class-T contact stubs + 77 Q-REAL = 206 stub sites; 14 struts; 15 balls
    assert len(di.stub_sites) == 206
    assert len(di.strut_sites) == 14
    assert len(di.ball_sites) == 15
    assert all(isinstance(s, phase8.StubSite) for s in di.stub_sites)
    assert all(isinstance(s, phase8.StrutSite) for s in di.strut_sites)
    assert all(isinstance(s, phase8.BallSite) for s in di.ball_sites)


@pytest.mark.kernel
def test_quarantine_regrade_matches_json():
    """The quarantine re-grade under the ratified law (§3.3) reproduces the
    pinned 77 REAL / 3 ARTIFACT split from the ENGINE's own gate primitives --
    no external census, no frozen list read for the split itself."""
    import pickle
    if not (os.path.exists(_PKL) and os.path.exists(_STL) and os.path.exists(_INPUT_JSON)):
        pytest.skip("archived artifacts not present")
    from support_placement.geometry_kernel import KernelSurface
    S = pickle.load(open(_PKL, "rb"))
    mesh = trimesh.load(_STL, file_type="stl", process=False)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    geom = KernelSurface(mesh)
    real, artifact = pd.regrade_quarantine(S["quarantine_sites"], geom, S["cfg"])
    d = json.load(open(_INPUT_JSON))
    assert set(real) == set(d["relocation_targets"]["real_stub_members"])
    assert set(artifact) == set(d["relocation_targets"]["artifact_stub_members"])
    assert len(real) == 77 and len(artifact) == 3


# ==========================================================================
# 2. live <-> forest index bijection (splice addressing).
# ==========================================================================
def test_live_forest_index_map_excludes_unrouted_and_dead(cfg):
    """`live_forest_index_map` reproduces `engine.live_member_lists`' filter (drop
    honestly-unrouted stubs + `forest.dead`), so `members[i]` is exactly
    `forest.members[live_fi[i]]` -- the address a splice writes back to."""
    forest = tree.Forest()
    ids = []
    for k in range(5):
        m = tree.Member(p0=[k, 0, -0.3], p1=[k, 0, 1.0], r0=0.2, r1=0.3,
                        kind="stub", owner=("contact", k), embed_end="p0")
        ids.append(forest.add_member(m))
        forest.terminal_of[k] = ids[-1]
    forest.dead.add(ids[3])                 # member 3 retired
    unrouted = [1]                          # contact 1 honestly unrouted
    live_fi = pd.live_forest_index_map(forest, unrouted)
    assert live_fi == [ids[0], ids[2], ids[4]]
    members, _ = engine.live_member_lists(forest, unrouted, plate_z=-5.0, cfg=cfg)
    assert len(members) == len(live_fi)
    for i, fi in enumerate(live_fi):
        assert np.allclose(members[i]["p0"], forest.members[fi].p0)


# ==========================================================================
# 3. END-TO-END synthetic: resolve a transit defect + idempotence (§6.10).
# ==========================================================================
def test_end_to_end_resolve_and_idempotent(cfg):
    """A stub that welds through the fin is a real rule-4 transit defect. The
    driver constructs its site from the gate, R1 re-aims it (shortening past the
    fin), and SPLICES the fix back -- the gate goes clean. A SECOND driver pass
    over the fixed geometry re-derives an EMPTY snapshot, enters NO ladder, and
    emits BYTE-IDENTICAL geometry (the §6.10 idempotence gate). The spliced
    forest stays fully certified (assert_all_certified)."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    plate_z = -5.0

    members, balls = engine.live_member_lists(forest, [], plate_z, cfg)
    before = pd.classify_gate(_fin_mesh(), members, balls, geom, cfg)
    assert len(before.firm_transit) == 1, "fixture must start with 1 firm transit"

    rep1 = pd.run_phase8(forest, members, balls, contacts, carried, tip_r,
                         plate_z, geom, cfg, _fin_mesh(), unrouted=[])
    assert rep1.entered_ladder
    assert rep1.stub_resolved == 1 and rep1.stub_unresolved == 0
    tree.assert_all_certified(forest)       # splice leaves the forest certified

    members1, balls1 = engine.live_member_lists(forest, [], plate_z, cfg)
    after = pd.classify_gate(_fin_mesh(), members1, balls1, geom, cfg)
    assert len(after.firm_transit) == 0, "the transit must be resolved"
    dig1 = p8_accept.geometry_digest(members1, balls1)

    # second pass over already-fixed geometry: empty snapshot -> no-op.
    rep2 = pd.run_phase8(forest, members1, balls1, contacts, carried, tip_r,
                         plate_z, geom, cfg, _fin_mesh(), unrouted=[])
    assert rep2.entered_ladder is False, "idempotence: empty snapshot enters no ladder"
    members2, balls2 = engine.live_member_lists(forest, [], plate_z, cfg)
    dig2 = p8_accept.geometry_digest(members2, balls2)
    assert dig1 == dig2, "idempotence: second pass must emit byte-identical geometry"


# ==========================================================================
# 3b. FIX-ROUND F1 (convergence loop) + F6 (provenance ledger).
# ==========================================================================
def test_convergence_loop_reaches_fixed_point_and_ledger(cfg):
    """Fix-round F1: with a `member_list_fn` the driver runs the ladder as a
    CONVERGENCE loop -- it re-reads the mutated forest between sweeps until a sweep
    is a true no-op. On the single-transit fin fixture it resolves the stub in
    sweep 0, then confirms an EMPTY snapshot in sweep 1 and stops, so `converged`
    is True and the last sweep committed the one splice. F6: the report carries a
    per-sweep provenance ledger (site-ID -> forest-idx -> disposition) so a
    collateral regression is attributable from artifacts."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    plate_z = -5.0
    members, balls = engine.live_member_lists(forest, [], plate_z, cfg)

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                        geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    assert rep.entered_ladder
    assert rep.stub_resolved == 1 and rep.stub_unresolved == 0
    assert rep.converged is True, "loop must terminate at a provable no-op"
    assert rep.sweeps >= 1
    assert rep.committed_by_sweep and rep.committed_by_sweep[0] == 1
    # F6 ledger: a resolved isolated stub row is present, sweep-tagged.
    assert rep.provenance_ledger, "F6: ledger must be populated"
    assert any(row[1] == "STUB" and row[4] == "ISOLATED" and row[5] == "R1"
               for row in rep.provenance_ledger), rep.provenance_ledger
    assert all(isinstance(row[0], int) for row in rep.provenance_ledger)
    tree.assert_all_certified(forest)
    # the transit is gone.
    m1, b1 = engine.live_member_lists(forest, [], plate_z, cfg)
    assert len(pd.classify_gate(_fin_mesh(), m1, b1, geom, cfg).firm_transit) == 0


def test_convergence_second_call_is_byte_identical_noop(cfg):
    """Fix-round F1 idempotence made real: after the convergence loop reaches its
    fixed point, a SECOND `run_phase8` (with the re-read closure) builds an empty
    snapshot, enters no ladder, and emits byte-identical geometry -- the §6.10
    idempotence gate the round-1 verdict could not claim (a third relocate() call
    used to keep making progress)."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    plate_z = -5.0

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    members, balls = _mlf()
    pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                  geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    m1, b1 = _mlf()
    dig1 = p8_accept.geometry_digest(m1, b1)
    rep2 = pd.run_phase8(forest, m1, b1, contacts, carried, tip_r, plate_z,
                         geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    assert rep2.entered_ladder is False
    assert rep2.sweeps == 0 and rep2.converged is True
    m2, b2 = _mlf()
    assert p8_accept.geometry_digest(m2, b2) == dig1


def test_convergence_loop_is_deterministic(cfg):
    """The convergence loop keeps the binding determinism doctrine: two full
    invocations (each with its own forest + re-read closure) emit bit-identical
    geometry AND the same per-sweep commit trace (no RNG, no wall-clock in the
    loop body; termination on integer counts only)."""
    def _run():
        geom = ExactSurface(_fin_mesh())
        forest, contacts, carried, tip_r = _fin_forest(cfg)

        def _mlf():
            return engine.live_member_lists(forest, [], -5.0, cfg)

        members, balls = _mlf()
        rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, -5.0,
                            geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
        m, b = _mlf()
        return p8_accept.geometry_digest(m, b), tuple(rep.committed_by_sweep)

    a = _run(); b = _run()
    assert a == b


def test_convergence_no_member_list_fn_is_single_sweep(cfg):
    """Back-compat: with `member_list_fn=None` (unit / legacy callers) the loop
    runs EXACTLY ONE sweep -- no forest re-read -- so the pre-convergence driver
    contract is preserved byte-for-byte."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    members, balls = engine.live_member_lists(forest, [], -5.0, cfg)
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, -5.0,
                        geom, cfg, _fin_mesh(), unrouted=[])
    assert rep.sweeps == 1
    assert rep.stub_resolved == 1


# ==========================================================================
# 4. DETERMINISM: two invocations bit-identical.
# ==========================================================================
def test_determinism_two_invocations_bit_identical(cfg):
    """Two driver invocations over identical input emit bit-identical geometry
    (no RNG, no wall-clock dependence -- the binding determinism doctrine). Built
    from two SEPARATE forests so nothing is shared between the runs."""
    def _run():
        geom = ExactSurface(_fin_mesh())
        forest, contacts, carried, tip_r = _fin_forest(cfg)
        members, balls = engine.live_member_lists(forest, [], -5.0, cfg)
        pd.run_phase8(forest, members, balls, contacts, carried, tip_r, -5.0,
                      geom, cfg, _fin_mesh(), unrouted=[])
        m, b = engine.live_member_lists(forest, [], -5.0, cfg)
        return p8_accept.geometry_digest(m, b)

    assert _run() == _run()


def test_empty_snapshot_is_noop(cfg):
    """A forest with no defect enters no ladder and mutates nothing (the
    idempotence invariant at its base case) -- `entered_ladder` is False and the
    geometry digest is unchanged."""
    geom = ExactSurface(_fin_mesh())
    # a SHORT stub (length 1.0) never reaches the fin -> no transit, no defect.
    forest, contacts, carried, tip_r = _fin_forest(cfg, stub_len=1.0)
    members, balls = engine.live_member_lists(forest, [], -5.0, cfg)
    dig_before = p8_accept.geometry_digest(members, balls)
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, -5.0,
                        geom, cfg, _fin_mesh(), unrouted=[])
    assert rep.entered_ladder is False
    m, b = engine.live_member_lists(forest, [], -5.0, cfg)
    assert p8_accept.geometry_digest(m, b) == dig_before


# ==========================================================================
# 5. engine integration: the relocate stage is wired + gated + idempotent.
# ==========================================================================
def test_engine_relocate_stage_gated_and_idempotent(cfg):
    """`engine.relocate` is the wired stage (post-brace / pre-final-verify). It
    resolves the fin transit when run, writes a `log['relocate']` ledger entry,
    and is a no-op on already-clean geometry (idempotent)."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    log = {}
    engine.relocate(_fin_mesh(), forest, [], contacts, carried, tip_r, -5.0, cfg,
                    geom=geom, log=log)
    assert "relocate" in log
    assert log["relocate"]["entered_ladder"] is True
    assert log["relocate"]["stub_resolved"] == 1
    members, balls = engine.live_member_lists(forest, [], -5.0, cfg)
    assert len(pd.classify_gate(_fin_mesh(), members, balls, geom, cfg).firm_transit) == 0
    # second call: clean geometry -> no-op ledger entry.
    log2 = {}
    engine.relocate(_fin_mesh(), forest, [], contacts, carried, tip_r, -5.0, cfg,
                    geom=geom, log=log2)
    assert log2["relocate"]["entered_ladder"] is False


# ==========================================================================
# 6. DEFECT-1 (amendment E): re-aim keeps the FIXED model-surface anchor, and
#    ONE shared certificate gates candidate legality AND acceptance (no
#    divergence). SPEC-P8-RELOCATE.md sec 3.1s + defect-1 (a)/(b).
# ==========================================================================
def _foot_d(geom, pt):
    """Unsigned surface distance of a point (embed-tip anchor check)."""
    d, _ = geom.query(np.asarray([pt], float))
    return abs(float(np.atleast_1d(d)[0]))


def test_strut_reaim_preserves_surface_anchor_synthetic(cfg):
    """Amendment E (defect-1a): a strut foot re-aim PIVOTS the shaft about the
    FIXED foot anchor -- every candidate's embed tip stays coincident with the
    on-surface foot, so its surface distance d is preserved (=0). The pre-fix
    `c = foot + n0*contact_depth_mm` floated the anchor off the surface, so any
    off-axis candidate dragged the foot off the model (d 0 -> >0): the exact
    regression that turned 38 legal interior struts illegal."""
    mesh = _box((20, 20, 5), center=(0, 0, -2.5))         # top face at z=0
    geom = ExactSurface(mesh)
    member = dict(p0=np.array([1.0, 0.5, 2.0]), p1=np.array([0.0, 0.0, 0.0]),
                  r0=0.35, r1=0.35, kind="strut", owner=("contact", 0),
                  embed_end="p1")                          # p1 == foot, ON surface
    assert _foot_d(geom, member["p1"]) < 1e-9

    site = pd.strut_site_from_member(0, member, cfg, 0.0, 0.0)
    assert site.foot_depth_mm == 0.0                       # foot IS the anchor
    assert _foot_d(geom, site.c) < 1e-9                    # c is ON the surface
    assert np.allclose(site.c, member["p1"])

    assert phase8.strut_candidates(site, cfg)              # cone yields candidates
    # EVERY candidate over the full cone x length grid keeps the foot on-surface.
    for pol, az, nh in phase8.reaim_directions(site.n0, cfg):
        for f in cfg.p8_reaim_length_factors:
            m = phase8.reaim_foot(site.c, nh, site.foot_depth_mm,
                                  site.strut_length_mm * f, site.r_foot,
                                  site.r_root, site.owner, 0.0)
            assert np.allclose(m["p0"], member["p1"]), (pol, az, f)   # foot fixed
            assert _foot_d(geom, m["p0"]) < 1e-9, (pol, az, f)        # d preserved


def test_offsurface_foot_certificate_matches_acceptance(cfg):
    """Defect-1b: an embed anchor driven OFF the surface (collapsed SCAR anchor,
    NO through-transit interval -- the amendment-E regression class) must be
    rejected by the SINGLE shared certificate (`verify.certify_member`) AND
    surfaced by the INDEPENDENT acceptance verifier
    (`clearance_report(scar_illegal_out=...)`). One source of truth, no
    divergence: the candidate gate can never accept what acceptance flags."""
    mesh = _box((20, 20, 5), center=(0, 0, -2.5))         # top face at z=0
    geom = ExactSurface(mesh)
    # foot 0.5mm ABOVE the surface -> d=0.5 > sqrt(R_b^2 - r_end^2)=0.343, anchor
    # collapses; shaft points further up so there is NO transit interval.
    bad = dict(p0=np.array([0.6, 0.0, 2.0]), p1=np.array([0.0, 0.0, 0.5]),
               r0=0.35, r1=0.35, kind="strut", owner=("contact", 0),
               embed_end="p1")
    assert _foot_d(geom, bad["p1"]) > 0.343
    iv = verify._batch_axis_intervals(geom, [bad], [0], rays="all")
    vd = verify._embed_scar_verdicts(geom, [bad], [0], iv, cfg)
    assert vd.get(0) is False                              # SCAR anchor collapsed
    cert = verify.certify_member(iv.get(0, []), bad, vd.get(0))
    assert cert["legal"] is False and cert["scar_ok"] is False
    assert cert["nonexempt"] is False                      # illegal but NO transit
    scar_out = []
    verify.clearance_report(mesh, [bad], cfg, geom=geom, scar_illegal_out=scar_out)
    assert [e["member_idx"] for e in scar_out] == [0]      # acceptance agrees
    assert scar_out[0]["source"] == "scar"

    # a GOOD on-surface foot: legal by BOTH, absent from the SCAR census.
    good = dict(p0=np.array([0.6, 0.0, 2.0]), p1=np.array([0.0, 0.0, 0.0]),
                r0=0.35, r1=0.35, kind="strut", owner=("contact", 0),
                embed_end="p1")
    ivg = verify._batch_axis_intervals(geom, [good], [0], rays="all")
    vdg = verify._embed_scar_verdicts(geom, [good], [0], ivg, cfg)
    assert vdg.get(0) is True
    assert verify.certify_member(ivg.get(0, []), good, vdg.get(0))["legal"] is True
    scar_out2 = []
    verify.clearance_report(mesh, [good], cfg, geom=geom, scar_illegal_out=scar_out2)
    assert scar_out2 == []


def test_certify_member_is_single_source_predicate():
    """`verify.certify_member` is the ONE legality predicate (defect-1b): SCAR
    verdict LEGAL *and* zero non-exempt transit. A collapsed embed anchor with no
    transit interval (verdict False, empty intervals) is illegal -- the exact
    class the transit sweep alone (`_transit_classify` -> (False,False)) would
    miss but the candidate gate rejects."""
    m_embed = dict(kind="strut", embed_end="p1", p0=[0, 0, 1], p1=[0, 0, 0],
                   r0=0.35, r1=0.35)
    # collapsed anchor, no transit -> illegal via scar, not via transit.
    c = verify.certify_member([], m_embed, False)
    assert c == dict(legal=False, scar_ok=False, nonexempt=False, moot=False)
    # legal verdict, no transit -> legal.
    c = verify.certify_member([], m_embed, True)
    assert c["legal"] is True and c["scar_ok"] is True
    # no designed embed end -> scar constraint N/A, legal when no transit.
    m_plain = dict(kind="shoot", p0=[0, 0, 1], p1=[0, 0, 0], r0=0.35, r1=0.35)
    assert verify.certify_member([], m_plain, None)["legal"] is True


@pytest.mark.kernel
def test_reaim_keeps_archived_strut_feet_on_surface():
    """Defect-1a regression on the ACTUAL members amendment E broke: interior
    struts mi 814, 945, 1471 -- three of the 38 legal struts the round-1 run
    drove off the model (d 0 -> 0.4-2.05mm, A_anchor collapse). Reconstructed
    from v5e_run1.pkl (foot ON surface, d=0), a re-aim over the FULL cone x
    length grid must keep EVERY candidate's embed tip on the surface (d
    preserved = 0) -- the fixed anchor never moves."""
    import pickle
    if not (os.path.exists(_PKL) and os.path.exists(_STL)):
        pytest.skip("archived v5e_run1.pkl / source STL not present")
    from support_placement.geometry_kernel import KernelSurface
    S = pickle.load(open(_PKL, "rb"))
    kcfg = S["cfg"]
    members = joints.collect_members(S["members"])
    mesh = trimesh.load(_STL, file_type="stl", process=False)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    geom = KernelSurface(mesh)
    for mi in (814, 945, 1471):
        m = members[mi]
        assert m["kind"] == "strut" and m.get("embed_end") == "p1"
        assert _foot_d(geom, m["p1"]) < 1e-6              # baseline foot ON surface
        site = pd.strut_site_from_member(mi, m, kcfg, 0.0, 0.0)
        assert site.foot_depth_mm == 0.0
        worst = 0.0
        for pol, az, nh in phase8.reaim_directions(site.n0, kcfg):
            for f in kcfg.p8_reaim_length_factors:
                cand = phase8.reaim_foot(site.c, nh, site.foot_depth_mm,
                                         site.strut_length_mm * f, site.r_foot,
                                         site.r_root, site.owner, 0.0)
                worst = max(worst, _foot_d(geom, cand["p0"]))
        assert worst < 1e-6, (mi, worst)


# ==========================================================================
# 7. DEFECT-3 (true fixed point): collision-revert ban -> monotone sweep tail
#    + provable second-pass no-op. SPEC-P8-RELOCATE §4.4-§4.5 / §6.10.
# ==========================================================================
def test_revert_ban_key_and_filter():
    """The anti-churn ban's two primitives. `_cand_geom_key` is a stable, hashable
    identity of a candidate's spliced geometry (round-9 p0/p1/r0/r1), equal for
    bit-identical geometry (a revert restores exactly, so the re-derived winner
    re-keys the same). `_ban_revalidate_fn` returns a selection guard that REJECTS
    a banned (forest_idx, geometry) and passes everything else; empty / None ban =>
    None (behaviour-preserving, no filter installed)."""
    m = dict(p0=np.array([0., 0., 0.]), p1=np.array([0., 0., 1.]), r0=0.2, r1=0.3)
    k = pd._cand_geom_key(m)
    assert k == pd._cand_geom_key(dict(p0=[0, 0, 0], p1=[0, 0, 1.0], r0=0.2, r1=0.3))
    assert isinstance(hash(k), int)                          # hashable
    cand = phase8.Candidate(site_id=7, cand_index=0, length_step=0, f=1.0,
                            polar_deg=0.0, azimuth_deg=0.0,
                            n_hat=np.array([0, 0, 1.0]), member=m)
    fn = pd._ban_revalidate_fn({42: {k}}, {7: 42})
    assert fn(cand) is False                                 # this (fi, geom) banned
    m2 = dict(p0=[0, 0, 0], p1=[0, 0, 0.75], r0=0.2, r1=0.3)  # DIFFERENT geometry
    cand2 = phase8.Candidate(site_id=7, cand_index=1, length_step=1, f=0.75,
                             polar_deg=0.0, azimuth_deg=0.0,
                             n_hat=np.array([0, 0, 1.0]), member=m2)
    assert fn(cand2) is True                                 # not banned -> passes
    # a site with no forest_idx mapping is never banned; empty/None ban installs no filter.
    assert pd._ban_revalidate_fn({42: {k}}, {})(cand) is True
    assert pd._ban_revalidate_fn({}, {7: 42}) is None
    assert pd._ban_revalidate_fn(None, {7: 42}) is None


def test_ladder_ban_forces_alternate_or_falls(cfg):
    """The ban changes ladder selection: banning R1's lex-min winner for a site
    forces the NEXT lex-min candidate (or an honest R2 fall) -- it can NEVER re-pick
    the banned candidate. This is the mechanism that turns the round-1 revert/
    re-commit churn ([...,1,1] tail, 32 reverts) into monotone progress: a reverted
    candidate is banned, so the site advances or falls instead of oscillating."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    members, balls = engine.live_member_lists(forest, [], -5.0, cfg)
    di = pd.construct_sites(forest, members, balls, contacts, carried, cfg, geom,
                            _fin_mesh(), unrouted=[])
    assert di.stub_sites
    site = di.stub_sites[0]
    base = phase8.evaluate_r1([site], geom, cfg)[site.site_id]
    assert base.winner is not None
    w0 = pd._cand_geom_key(base.winner.member)
    banned = {di.site_forest_idx[site.site_id]: {w0}}
    fn = pd._ban_revalidate_fn(banned, di.site_forest_idx)
    res = phase8.evaluate_r1([site], geom, cfg, revalidate_fn=fn)[site.site_id]
    # the banned winner is never re-selected: either a different winner or an R2 fall.
    if res.winner is not None:
        assert pd._cand_geom_key(res.winner.member) != w0
    else:
        assert res.needs_r2 is True


def test_convergence_sweep_tail_monotone_and_second_pass_noop(cfg):
    """Defect-3 end-to-end: the convergence loop's per-sweep commit trace is
    NON-INCREASING (`sweep_tail_monotone` -- the [116,57,9,2,3,2,1,1] revert
    rebound can no longer occur) and it terminates at a true fixed point, so a
    SECOND `run_phase8` over the ACCEPTED geometry re-derives an empty snapshot and
    emits byte-identical geometry -- a provable no-op (§6.10)."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    plate_z = -5.0

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    members, balls = _mlf()
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                        geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    cbs = rep.committed_by_sweep
    assert all(a >= b for a, b in zip(cbs, cbs[1:])), cbs      # monotone non-increasing
    assert rep.sweep_tail_monotone is True
    assert rep.converged is True

    m1, b1 = _mlf()
    dig1 = p8_accept.geometry_digest(m1, b1)
    rep2 = pd.run_phase8(forest, m1, b1, contacts, carried, tip_r, plate_z, geom,
                         cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    assert rep2.entered_ladder is False and rep2.sweeps == 0 and rep2.converged is True
    assert rep2.sweep_tail_monotone is True
    m2, b2 = _mlf()
    assert p8_accept.geometry_digest(m2, b2) == dig1


def test_per_pass_relocate_ledger_distinct_keys(cfg):
    """Defect-5 (ledger): each `relocate` INVOCATION writes a DISTINCT
    `relocate_p{N}` key, so one never overwrites another's report (the round-1
    single `relocate` key hid pass-0 under pass-1). `relocate` keeps the latest for
    back-compat.

    Since the §3.0 final-only amendment `engine.run` invokes the stage exactly once
    (writing `relocate_p0`), but the per-invocation key contract still governs any
    direct caller -- the acceptance harness and the §6.10 idempotence pass both
    invoke it a second time over the finished forest -- so it stays pinned here."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    log = {}
    engine.relocate(_fin_mesh(), forest, [], contacts, carried, tip_r, -5.0, cfg,
                    geom=geom, log=log, pass_idx=0)
    engine.relocate(_fin_mesh(), forest, [], contacts, carried, tip_r, -5.0, cfg,
                    geom=geom, log=log, pass_idx=1)
    assert "relocate_p0" in log and "relocate_p1" in log
    assert log["relocate_p0"]["entered_ladder"] is True       # pass 0 resolves the stub
    assert log["relocate_p1"]["entered_ladder"] is False      # pass 1 over fixed geometry
    assert log["relocate_p0"] is not log["relocate_p1"]       # NOT overwritten
    assert log["relocate"] is log["relocate_p1"]              # 'relocate' == latest


# ==========================================================================
# 8. DEFECT-5/6 (frozen-fixture treant target law, geometry-gated). §3.0 / §5.3.
# ==========================================================================
@pytest.mark.kernel
def test_frozen_fixture_is_treant_target_law_geometry_gated():
    """On the pre-relocation treant (v5e_run1) the driver's target set is the
    FROZEN fixture (the census-authoritative treant law, §5.3 Layer 2): 143
    non-exempt / 50 moot / 77 REAL-quarantine / 15 balls, INCLUDING the 6
    census-only boundary members {168,376,443,445,485,519} the live kernel gate
    exempts (defect-6), with mi=420 all-moot (never a target). The gate is the
    canonical geometry digest: a mutated forest falls back to the live gate, so the
    fixture's v5e_run1 indices are never mis-applied to relocated geometry and the
    §6.10 idempotence pass is a no-op by construction (defect-5)."""
    import pickle
    if not (os.path.exists(_PKL) and os.path.exists(_STL) and os.path.exists(_INPUT_JSON)):
        pytest.skip("archived artifacts not present")
    from support_placement.geometry_kernel import KernelSurface
    S = pickle.load(open(_PKL, "rb"))
    mesh = trimesh.load(_STL, file_type="stl", process=False)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    geom = KernelSurface(mesh)
    frozen = pd.load_treant_frozen_snapshot()
    assert frozen is not None

    di = pd.construct_sites(S["forest"], S["members"], S["balls"], S["contacts"],
                            None, S["cfg"], geom, mesh, unrouted=S["unrouted"],
                            quarantine_sites=S["quarantine_sites"],
                            frozen_snapshot=frozen)
    assert di.target_source == "frozen-fixture"
    snap = di.snapshot
    assert len(snap.non_exempt) == 143 and len(snap.moot) == 50
    assert len(snap.quarantine_real) == 77 and len(snap.ball_breaches) == 15
    # defect-6: every census-only boundary member is a target (485 also quarantine-routed).
    assert {168, 376, 443, 445, 485, 519}.issubset(set(snap.non_exempt))
    # mi=420 is all-moot at full precision -> recorded moot, NEVER remediated.
    assert 420 in snap.moot and 420 not in snap.non_exempt

    # geometry gate: perturb ONE member -> digest no longer matches -> live gate.
    m2 = [dict(x) for x in S["members"]]
    m2[0] = dict(m2[0])
    m2[0]["p1"] = np.asarray(m2[0]["p1"], float) + np.array([0.002, 0.0, 0.0])
    di2 = pd.construct_sites(S["forest"], m2, S["balls"], S["contacts"], None,
                             S["cfg"], geom, mesh, unrouted=S["unrouted"],
                             quarantine_sites=S["quarantine_sites"],
                             frozen_snapshot=frozen)
    assert di2.target_source == "live-gate"


def test_frozen_fixture_absent_falls_back_to_live_gate(cfg):
    """A synthetic (non-treant) forest never matches the fixture geometry digest,
    so target selection stays the live gate whether or not a frozen snapshot is
    passed -- the frozen fixture is treant-only, and passing it is always safe."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    members, balls = engine.live_member_lists(forest, [], -5.0, cfg)
    frozen = pd.load_treant_frozen_snapshot()                 # real fixture, wrong geometry
    di = pd.construct_sites(forest, members, balls, contacts, carried, cfg, geom,
                            _fin_mesh(), unrouted=[], frozen_snapshot=frozen)
    assert di.target_source == "live-gate"
    assert len(di.stub_sites) == 1                            # the fin transit only


# ==========================================================================
# 9. DIRECTIVE 1 / D2 -- connectivity- & printability-preserving splice commits.
#    (SPEC-P8-RELOCATE; site-derived from the p8f_run1 130-connectivity /
#     9-printability offenders, encoded as deterministic synthetic fixtures so the
#     regression runs everywhere -- the offender CLASSES, not the pinned indices.)
# ==========================================================================
def _faraway_box_geom():
    """An ExactSurface whose mesh is far from the origin -- clearance is trivially
    clean and nothing embeds, so a splice's ONLY guard trigger is connectivity /
    printability (the axis we mean to test), never a mesh collision."""
    return ExactSurface(_box((5, 5, 5), center=(0, 0, -50)))


def _stub_with_child(tree_node=(0.0, 0.0, 0.0), child_p1=(0.0, 0.0, -0.5),
                     r_child=(0.3, 0.3), child_kind="trunk"):
    """A class-T stub S (fi0) whose tree-side end p1 IS `tree_node`, plus a child
    member R (fi1) whose p0 is COINCIDENT with that node (R.p1 lands on the plate
    at z = child_p1[2]). This is the 130-connectivity offender shape: a re-aim that
    moves S.p1 must drag R.p0 with it or R is orphaned."""
    forest = tree.Forest()
    s = tree.Member(p0=np.array([0.0, 0.0, 0.5]), p1=np.asarray(tree_node, float),
                    r0=0.2, r1=0.3, kind="stub", owner=("contact", 0), embed_end="p0")
    forest.add_member(s)
    r = tree.Member(p0=np.asarray(tree_node, float), p1=np.asarray(child_p1, float),
                    r0=float(r_child[0]), r1=float(r_child[1]), kind=child_kind,
                    owner=("contact", 1), embed_end=None)
    forest.add_member(r)
    return forest


def test_d2_reaim_comoves_child_junction_and_preserves_connectivity(cfg):
    """D2 part A (site-derived, the 130 class-T connectivity offenders): a re-aim
    that moves a stub's tree-side p1 CO-MOVES every live member endpoint coincident
    with the OLD node, so the child junction rides p1 and the junction stays welded
    -- connectivity is preserved BY CONSTRUCTION and the splice commits.

    Under the D6 commit invariant a splice must ALSO strictly reduce the influence
    set's defect count, so the fixture carries a real defect for the re-aim to
    clear: the child R starts near-HORIZONTAL (a printability violation), and the
    node move the stub's re-aim drags it through stands it upright."""
    geom = _faraway_box_geom()
    plate_z = -0.5
    forest = _stub_with_child(tree_node=(0.0, 0.0, -0.499), child_p1=(0.5, 0.0, -0.5))
    assert tree.printability_report([forest.members[1]], cfg)   # R illegal PRE
    # a re-aim whose node move stands the child upright (defect count 1 -> 0).
    new_p1 = np.array([0.5, 0.0, 0.5])
    cand = dict(p0=np.array([0.0, 0.0, 0.5]), p1=new_p1, r0=0.2, r1=0.3,
                kind="stub", owner=("contact", 0), embed_end="p0")
    n_ok, reverted = pd.splice_reaims(forest, geom, cfg, [(0, cand, "p0")],
                                      plate_z=plate_z, unrouted=[])
    assert reverted == [] and n_ok == 1                        # committed, not reverted
    # the child junction R.p0 rode the SAME delta -> still coincident with S.p1.
    assert np.allclose(forest.members[1].p0, new_p1)           # co-moved
    assert np.allclose(forest.members[0].p1, forest.members[1].p0)   # still welded
    assert not tree.printability_report([forest.members[1]], cfg)    # R legal POST
    # affected-set connectivity has no NEW float (the junction survived the re-aim).
    members = joints.collect_members([forest.members[0], forest.members[1]])
    conn = verify.connectivity_report(members, plate_z, cfg, geom=geom, subset=[0, 1])
    assert not any(v["member_idx"] == 1 for v in conn)         # R never floats


def test_d6_no_progress_reaim_is_illegal_at_commit(cfg):
    """D6 ZERO-NEW-VIOLATION COMMIT INVARIANT, half 2 (STRICT PROGRESS). The exact
    non-convergence mechanism the archived treant trace shows -- a re-aim that
    neither breaks anything nor CLEARS anything, re-committed every sweep while its
    worst_chord shrinks asymptotically ([57,14,11,9,3,3,1,2], never reaching the
    fixed point) -- is now ILLEGAL AT COMMIT: with no defect anywhere in the
    influence set to remove, the splice cannot strictly decrease the defect count,
    so the whole transaction is reverted bit-exact and the sweep is a no-op."""
    geom = _faraway_box_geom()
    plate_z = -0.5
    forest = _stub_with_child(tree_node=(0.0, 0.0, 0.0), child_p1=(0.0, 0.0, -0.5))
    snaps = [forest.members[fi]._snapshot() for fi in (0, 1)]
    cand = dict(p0=np.array([0.0, 0.0, 0.5]), p1=np.array([0.1, 0.0, 0.0]),
                r0=0.2, r1=0.3, kind="stub", owner=("contact", 0), embed_end="p0")
    n_ok, reverted = pd.splice_reaims(forest, geom, cfg, [(0, cand, "p0")],
                                      plate_z=plate_z, unrouted=[])
    assert n_ok == 0 and reverted == [0]           # churn commit refused
    for snap, fi in zip(snaps, (0, 1)):            # restored BIT-EXACT
        cur = forest.members[fi]._snapshot()
        assert np.array_equal(cur["p0"], snap["p0"]) and np.array_equal(cur["p1"], snap["p1"])
        assert cur["r0"] == snap["r0"] and cur["r1"] == snap["r1"]


def test_d6_influence_defects_scores_every_class_batched(cfg):
    """D6: `_influence_defects` reads EVERY defect class the invariant governs, and
    reads each from the ONE authority (`verify.certify_member` for transit + embed
    grade, `connectivity_report`, `printability_report`, `ball_breach_report`) --
    never a second predicate. Site-derived shapes: a near-horizontal shaft
    (printability), a detached rod (connectivity), a stub welded through a thin fin
    (transit)."""
    geom = ExactSurface(_fin_mesh())
    plate_z = -5.0
    forest, _c, _ca, _tr = _fin_forest(cfg)
    members = joints.collect_members(list(forest.members))
    allpos = list(range(len(members)))
    defmap = pd._influence_defects(members, allpos, geom, cfg, plate_z)
    assert any("transit" in v for v in defmap.values()), defmap
    # the count the invariant compares is (member x class), monotone in the map.
    assert pd._defect_count(defmap, allpos) >= len(defmap)
    # an empty subset is a free no-op (the guard must never do work it needn't).
    assert pd._influence_defects(members, [], geom, cfg, plate_z) == {}
    # a flat shaft is scored "printability"; a far-from-everything rod
    # "connectivity" -- both from the shared reports, not a local re-derivation.
    flat = dict(p0=np.array([0.0, 0.0, 20.0]), p1=np.array([5.0, 0.0, 20.001]),
                r0=0.3, r1=0.3, kind="trunk", owner=("contact", 9), embed_end=None)
    probe = pd._influence_defects([flat], [0], geom, cfg, plate_z)
    assert "printability" in probe.get(0, frozenset()), probe
    assert "connectivity" in probe.get(0, frozenset()), probe


def test_d2_new_printability_violation_reverts_transaction_as_a_unit(cfg):
    """D2 part B (site-derived, the 9 printability offenders): a re-aim whose
    CO-MOVE tilts a dependent shaft past the overhang cone introduces a NEW
    printability violation on the co-moved member; the whole transaction (primary
    UNION co-moved) is reverted AS A UNIT -- every member restored bit-exact
    (verified by _snapshot equality), the contact never stranded."""
    geom = _faraway_box_geom()
    plate_z = -0.5
    # R is a near-vertical trunk (legal); a big HORIZONTAL node move tilts it flat.
    forest = _stub_with_child(tree_node=(0.0, 0.0, 0.0), child_p1=(0.0, 0.0, -0.5),
                              r_child=(0.4, 0.4), child_kind="trunk")
    snap_s0 = forest.members[0]._snapshot()
    snap_r0 = forest.members[1]._snapshot()
    assert not tree.printability_report([forest.members[1]], cfg)   # R legal PRE
    new_p1 = np.array([10.0, 0.0, 0.0])                        # drags R.p0 to (10,0,0)
    cand = dict(p0=np.array([0.0, 0.0, 0.5]), p1=new_p1, r0=0.2, r1=0.3,
                kind="stub", owner=("contact", 0), embed_end="p0")
    n_ok, reverted = pd.splice_reaims(forest, geom, cfg, [(0, cand, "p0")],
                                      plate_z=plate_z, unrouted=[])
    assert reverted == [0] and n_ok == 0                       # whole txn reverted
    # both the primary AND the co-moved child restored BIT-EXACT (_snapshot equality).
    for snap, fi in ((snap_s0, 0), (snap_r0, 1)):
        cur = forest.members[fi]._snapshot()
        assert np.array_equal(cur["p0"], snap["p0"]) and np.array_equal(cur["p1"], snap["p1"])
        assert cur["r0"] == snap["r0"] and cur["r1"] == snap["r1"]


def test_d2_strut_shaft_angle_candidate_pruned_at_selection(cfg):
    """D2 (site-derived, the strut-825 printability class): a strut re-aim whose
    shaft tilts past the cfg overhang cone is pruned at S1 SELECTION by the
    printability filter (the SAME drift+taper predicate the commit gate enforces),
    not discovered at commit -- and the filter composes with the anti-churn ban."""
    flat = phase8.Candidate(site_id=1, cand_index=0, length_step=0, f=1.0,
                            polar_deg=80.0, azimuth_deg=0.0, n_hat=np.array([1., 0, 0.1]),
                            member=dict(p0=np.array([0., 0, 0]), p1=np.array([5., 0, 0.1]),
                                        r0=0.3, r1=0.3, kind="strut", embed_end="p0",
                                        owner=("contact", 0)))
    upright = phase8.Candidate(site_id=1, cand_index=1, length_step=0, f=1.0,
                               polar_deg=0.0, azimuth_deg=0.0, n_hat=np.array([0., 0, 1.]),
                               member=dict(p0=np.array([0., 0, 0]), p1=np.array([0., 0, 5.]),
                                           r0=0.3, r1=0.3, kind="strut", embed_end="p0",
                                           owner=("contact", 0)))
    assert pd._strut_shaft_printability_ok(flat, cfg) is False       # pruned
    assert pd._strut_shaft_printability_ok(upright, cfg) is True      # kept
    # composition with the ban: an upright but BANNED candidate is also rejected.
    ban = {7: {pd._cand_geom_key(upright.member)}}
    fn = pd._compose_strut_revalidate_fn(pd._ban_revalidate_fn(ban, {1: 7}), None, cfg)
    assert fn(flat) is False                                          # printability
    assert fn(upright) is False                                       # banned
    # no ban / no-op filter installed -> only the printability filter binds.
    fn2 = pd._compose_strut_revalidate_fn(None, None, cfg)
    assert fn2(flat) is False and fn2(upright) is True


# ==========================================================================
# 9. DIRECTIVE 2 -- D3(a) progress-based retirement (true fixed point).
# ==========================================================================
def _const_gate_for_first_transit(cfg, forest, geom, plate_z=-5.0, chord=1.0):
    """Helper: the mi of the fin fixture's single firm transit + a constant
    `GateClassification` that ALWAYS reports it non-exempt with a fixed worst
    chord. Monkeypatching `pd.classify_gate` to this makes the site regenerate
    every sweep and its commit never clear it -- the exact non-improving
    reappearance D3(a) retires."""
    members0, balls0 = engine.live_member_lists(forest, [], plate_z, cfg)
    real = pd.classify_gate(_fin_mesh(), members0, balls0, geom, cfg)
    assert len(real.firm_transit) == 1, "fixture must start with 1 firm transit"
    mi = next(iter(real.firm_transit))
    const = pd.GateClassification(firm_transit=frozenset({mi}), moot=frozenset(),
                                  worst_chord={mi: float(chord)}, ball_breaches=())
    return mi, const


def test_d3a_synthetic_site_committed_once_then_retired(cfg, monkeypatch):
    """D3(a) part 1 (regression test 1): a synthetic site whose commit never clears
    its transit (a constant gate that re-flags it every sweep) is committed AT MOST
    ONCE -- retired on the next sweep because its defect measure did not strictly
    shrink -- so `committed_by_sweep` is monotone, `converged` is True, and the site
    is reported as honest-unresolved (in `retired_sites` AND `unresolved_items`)."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    plate_z = -5.0
    mi, const_gate = _const_gate_for_first_transit(cfg, forest, geom, plate_z)
    monkeypatch.setattr(pd, "classify_gate", lambda *a, **k: const_gate)

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    members, balls = _mlf()
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                        geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    # committed AT MOST ONCE, and the trace is non-increasing + a true fixed point.
    assert rep.committed_by_sweep == (1,), rep.committed_by_sweep
    assert all(a >= b for a, b in zip(rep.committed_by_sweep,
                                      rep.committed_by_sweep[1:]))
    assert rep.sweep_tail_monotone is True
    assert rep.converged is True, "retirement must reach an honest fixed point"
    # retired thereafter, and counted honest-unresolved at its last rung.
    assert any(r[0] == "STUB" and r[1] == mi for r in rep.retired_sites), rep.retired_sites
    assert any(it[0] == "STUB" and it[1] == mi for it in rep.unresolved_items), \
        rep.unresolved_items
    # a retired site is NOT double-counted as resolved (moved out of the tally).
    assert rep.stub_unresolved >= 1


def test_d3a_noop_candidate_is_skipped():
    """D3(a) part 2 (regression test 2): `_noop_skip_fn` REJECTS a candidate whose
    spliced geometry equals the member's CURRENT forest geometry (a no-op churn
    re-pick) and PASSES a genuinely different candidate -- for both an embed-p0 stub
    and an embed-p1 strut foot (the candidate is canonical embed-at-p0; the no-op is
    the end-swapped current geometry)."""
    forest = tree.Forest()
    m = tree.Member(p0=np.array([0., 0, 0]), p1=np.array([0., 0, 1.]), r0=0.2, r1=0.3,
                    kind="stub", owner=("contact", 0), embed_end="p0")
    fi = forest.add_member(m)
    fn = pd._noop_skip_fn(forest, {7: fi})
    same = phase8.Candidate(site_id=7, cand_index=0, length_step=0, f=1.0,
                            polar_deg=0.0, azimuth_deg=0.0, n_hat=np.array([0., 0, 1.]),
                            member=dict(p0=[0., 0, 0], p1=[0., 0, 1.], r0=0.2, r1=0.3))
    assert fn(same) is False, "a candidate == current geometry is a no-op churn commit"
    diff = phase8.Candidate(site_id=7, cand_index=1, length_step=1, f=0.5,
                            polar_deg=0.0, azimuth_deg=0.0, n_hat=np.array([0., 0, 1.]),
                            member=dict(p0=[0., 0, 0], p1=[0., 0, 0.5], r0=0.2, r1=0.3))
    assert fn(diff) is True, "a genuinely different candidate is progress"
    # embed-p1 strut foot: forest stores free end at p0, foot at p1; the candidate
    # is canonical (foot at p0), so the no-op is the end-swapped current geometry.
    m2 = tree.Member(p0=np.array([0., 0, 1.]), p1=np.array([0., 0, 0]), r0=0.3, r1=0.2,
                     kind="strut", owner=("contact", 0), embed_end="p1")
    fi2 = forest.add_member(m2)
    fn2 = pd._noop_skip_fn(forest, {9: fi2})
    noop2 = phase8.Candidate(site_id=9, cand_index=0, length_step=0, f=1.0,
                             polar_deg=0.0, azimuth_deg=0.0, n_hat=np.array([0., 0, 1.]),
                             member=dict(p0=[0., 0, 0], p1=[0., 0, 1.], r0=0.2, r1=0.3))
    assert fn2(noop2) is False, "canonical re-pick of the applied foot geometry is a no-op"
    # a filter with no forest mapping never rejects (behaviour-preserving).
    assert pd._noop_skip_fn(forest, {})(same) is True


def test_d3a_p8f_run1_churn_pair_is_single_commit_then_retire(cfg, monkeypatch):
    """D3(a) (regression test 3, SITE-DERIVED): the archived p8f_run1 relocation
    trace `committed_by_sweep=[117,56,9,2,3,2,1,1]` is NON-monotone (a 2->3 rebound,
    a [...,1,1] tail) and commits 191 splices over ~128 targets -- proof that some
    target was committed in >=2 sweeps (revert/re-commit churn). The D3(a)
    retirement predicate turns that exact non-improving-reappearance pattern into
    single-commit-then-retire, so the new trace is monotone and converged. Skips if
    the archived artifact is absent."""
    p = os.path.join(_SCR, "p8f_run1_results.json")
    if not os.path.exists(p):
        pytest.skip("p8f_run1 artifact absent")
    with open(p) as f:
        cbs_old = json.load(f)["relocate_reports"]["relocate"]["committed_by_sweep"]
    assert cbs_old == [117, 56, 9, 2, 3, 2, 1, 1], cbs_old
    assert not all(a >= b for a, b in zip(cbs_old, cbs_old[1:])), \
        "the archived trace must be the non-monotone churn this fixes"
    assert sum(cbs_old) > cbs_old[0], "re-commits across sweeps => a site committed >=2 sweeps"
    # the retirement predicate that breaks it: an equal-measure reappearance retires;
    # genuine (canonical-granularity) progress keeps trying; a resolved site (now is
    # None) is not retired; and a budget-exhausted site retires even while improving.
    assert pd._should_retire(0.8, 0.8, 0) is True                 # no progress -> retire
    assert pd._should_retire(0.79, 0.8, 0) is False               # progress -> keep
    assert pd._should_retire(None, 0.8, 0) is False               # resolved -> keep
    assert pd._should_retire(0.5, 0.9, pd._SITE_COMMIT_BUDGET) is True   # budget spent
    assert pd._should_retire(0.5, 0.9, 0) is False                # improving, budget left
    # end-to-end on a churn analogue (a commit that never clears its gate): single
    # commit, then retire, MONOTONE + converged -- the fix the trace above needed.
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    plate_z = -5.0
    mi, const_gate = _const_gate_for_first_transit(cfg, forest, geom, plate_z)
    monkeypatch.setattr(pd, "classify_gate", lambda *a, **k: const_gate)

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    members, balls = _mlf()
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                        geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    assert rep.committed_by_sweep == (1,), rep.committed_by_sweep
    assert rep.sweep_tail_monotone is True and rep.converged is True
    assert any(r[1] == mi for r in rep.retired_sites), rep.retired_sites


@pytest.mark.kernel
def test_full_run_converges_before_cap_and_is_strictly_idempotent():
    """FULL-RUN acceptance evidence for the D6 commit invariant + the final-only
    relocation stage, read from the fresh treant artifact (skips without it).

      * `converged is True` with the terminating sweep committing ZERO, reached
        BEFORE the sweep cap -- not at it;
      * `committed_by_sweep` non-increasing and ending in 0;
      * `defects_by_sweep` strictly decreasing across every committing sweep --
        the monotonicity the invariant guarantees;
      * a SECOND relocation over the final forest is a STRICT no-op (§6.10);
      * exactly ONE relocation stage in the ledger (`relocate_p0`, no `relocate_p1`).
    """
    p = os.path.join(_SCR, "core2_results.json")
    if not os.path.exists(p):
        pytest.skip("CORE full-run artifact absent")
    with open(p) as f:
        art = json.load(f)
    reports = art["relocate_reports"]
    assert set(reports) == {"relocate", "relocate_p0"}, sorted(reports)
    rep = reports["relocate_p0"]
    cbs = rep["committed_by_sweep"]
    dbs = rep["defects_by_sweep"]
    assert rep["converged"] is True, cbs
    assert cbs[-1] == 0, cbs                       # a true no-op sweep, not a cap stop
    assert all(a >= b for a, b in zip(cbs, cbs[1:])), cbs
    assert rep["sweeps"] < 8, (rep["sweeps"], cbs)
    for k, n_c in enumerate(cbs):
        if n_c > 0 and k + 1 < len(dbs):
            assert dbs[k + 1] < dbs[k], (k, dbs, cbs)
    idem = art["idempotence"]
    assert idem["strict_no_op"] is True, idem
    assert idem["exact_geometry_equal"] is True, idem
    assert sum(idem["second_pass_report"]["committed_by_sweep"] or [0]) == 0, idem


@pytest.mark.kernel
def test_relocate_perf_bound_on_archived_treant_input():
    """PERF REGRESSION on the archived p8r_run2.pkl-class input (skips without it).

    The relocation stage's dominant pure-Python cost was `compute_joint_balls`'
    O(clusters x axis_members) SCALAR double loop -- 6.6M `_dist_and_local_radius`
    calls, 6.6s PER CALL on this forest, and the sweep loop re-derives the live
    member+ball lists once per sweep, so a single relocation pass paid it 8x (46s
    of a 204s profiled stage; the stack samples caught it mid-loop, which is what
    read as a `relocate[p0] 0/1 contacts` hang).

    Two guards, one machine-independent and one wall-clock:
      * the scalar per-pair helper is NEVER called from `compute_joint_balls` --
        the batch-first property itself, which no amount of faster hardware can
        satisfy for a scalar loop;
      * and the call stays under a generous wall bound (measured 0.28s batched vs
        4.71s scalar on this exact forest).
    """
    import pickle
    import time as _time
    pkl = os.path.join(_SCR, "p8r_run2.pkl")
    if not os.path.exists(pkl):
        pytest.skip("archived p8r_run2.pkl not present")
    with open(pkl, "rb") as f:
        st = pickle.load(f)
    forest, c, plate_z = st["forest"], st["cfg"], st["plate_z"]
    unrouted_stub_idx = {forest.terminal_of[i] for i in st["unrouted"]
                         if i in forest.terminal_of}
    live = [m for mi, m in enumerate(forest.members)
            if mi not in unrouted_stub_idx and mi not in forest.dead]
    assert len(live) > 1000, len(live)             # a real treant-scale forest

    calls = {"n": 0}
    orig = joints._dist_and_local_radius

    def counting(*a, **k):
        calls["n"] += 1
        return orig(*a, **k)

    joints._dist_and_local_radius = counting
    try:
        t0 = _time.perf_counter()
        balls = joints.compute_joint_balls(live, plate_z, c)
        dt = _time.perf_counter() - t0
    finally:
        joints._dist_and_local_radius = orig
    assert balls, "the archived forest must produce joint balls"
    assert calls["n"] == 0, \
        f"{calls['n']} scalar per-pair calls -- the batch-first scan regressed"
    assert dt < 2.0, f"compute_joint_balls took {dt:.2f}s on the archived forest"


# ==========================================================================
# 10. D6 -- ZERO-NEW-VIOLATION COMMIT INVARIANT (orchestrator ruling 2026-07-24).
# ==========================================================================
def _cascade_forest(cfg, stub_len=2.0, tip_z=4.0):
    """The NEIGHBOUR-CASCADE shape, site-derived from the treant's own failure
    mode: a contact stub P that welds through the fin (its own real transit
    defect), with a trunk N welded to P's tree-side node and rising clear above
    the fin. Every R1 candidate that clears P's transit SHORTENS it, which drags
    N's foot DOWN through the fin -- so fixing P creates a brand-new transit on
    its neighbour N. That is exactly the cascade the archived trace shows
    (committed_by_sweep=[57,14,11,9,3,3,1,2], never converging): the next sweep
    re-targets N, whose fix perturbs the next member, forever."""
    forest, contacts, carried, tip_r = _fin_forest(cfg, stub_len=stub_len)
    node = np.array([0.0, 0.0, stub_len])
    forest.add_member(tree.Member(p0=node.copy(), p1=np.array([0.0, 0.0, tip_z]),
                                  r0=0.35, r1=0.35, kind="trunk",
                                  owner=("contact", 0), embed_end=None))
    return forest, contacts, carried, tip_r


def test_d6_neighbour_cascade_commit_is_illegal_and_loop_converges(cfg):
    """D6 half 1 (ZERO NEW VIOLATION) end-to-end through the driver: on the
    cascade fixture, every candidate that clears the stub's own transit pushes its
    welded neighbour through the fin. That is a NEW violation inside the commit's
    INFLUENCE SET, so the transaction is illegal at commit and reverted as a unit.

    The loop therefore commits ZERO and stops at a TRUE fixed point on sweep 1 --
    instead of committing, re-flagging the neighbour, re-targeting it next sweep,
    and grinding to the cap. The stub is reported HONEST-unresolved (it needs the
    R2 surface-seed / D5 junction re-seat levers, not another re-aim)."""
    geom = ExactSurface(_fin_mesh())
    plate_z = -5.0
    forest, contacts, carried, tip_r = _cascade_forest(cfg)

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    members, balls = _mlf()
    pre = p8_accept.geometry_digest(members, balls)
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                        geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    assert rep.entered_ladder is True
    assert rep.committed_by_sweep == (0,), rep.committed_by_sweep   # nothing legal
    assert rep.converged is True and rep.sweep_tail_monotone is True
    assert rep.sweeps == 1, "a zero-commit sweep IS the fixed point -- never the cap"
    # geometry is untouched: an illegal batch is reverted BIT-exact.
    m1, b1 = _mlf()
    assert p8_accept.geometry_digest(m1, b1) == pre
    # and the neighbour was never left perturbed.
    assert np.array_equal(forest.members[1].p0, np.array([0.0, 0.0, 2.0]))
    # the guard's own evidence: applying the winning re-aim by hand CLEARS the
    # stub's transit and hands a brand-new TRANSIT to the co-moved neighbour --
    # a class the pre-D6 guard (clearance + connectivity + printability only) was
    # blind to. This is the transit-aware extension the ruling calls for.
    pre_def = pd._influence_defects(joints.collect_members(list(forest.members)),
                                    [0, 1], geom, cfg, plate_z)
    assert "transit" in pre_def[0] and "transit" not in pre_def.get(1, frozenset())
    forest.members[0].p1 = np.array([0.0, 0.0, 1.0])
    forest.members[1].p0 = np.array([0.0, 0.0, 1.0])
    post_def = pd._influence_defects(joints.collect_members(list(forest.members)),
                                     [0, 1], geom, cfg, plate_z)
    assert "transit" not in post_def.get(0, frozenset())      # primary fixed ...
    assert "transit" in post_def[1]                            # ... neighbour broken


def test_d6_second_pass_is_a_strict_no_op_on_the_cascade_fixture(cfg):
    """§6.10 IDEMPOTENCE on the synthetic that PREVIOUSLY cascaded: because the
    first pass ends at a real fixed point (D6 refused every illegal commit rather
    than committing one and re-opening a neighbour), a SECOND `run_phase8` over the
    same forest re-derives the same snapshot, commits zero, and emits BYTE-identical
    geometry. Before D6 the first pass committed, the neighbour re-flagged, and the
    second pass had fresh work to do -- the gate could not be met."""
    geom = ExactSurface(_fin_mesh())
    plate_z = -5.0
    forest, contacts, carried, tip_r = _cascade_forest(cfg)

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    m0, b0 = _mlf()
    pd.run_phase8(forest, m0, b0, contacts, carried, tip_r, plate_z, geom, cfg,
                  _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    m1, b1 = _mlf()
    dig1 = p8_accept.geometry_digest(m1, b1)
    rep2 = pd.run_phase8(forest, m1, b1, contacts, carried, tip_r, plate_z, geom,
                         cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    assert rep2.converged is True and sum(rep2.committed_by_sweep) == 0
    m2, b2 = _mlf()
    assert p8_accept.geometry_digest(m2, b2) == dig1     # STRICT no-op


def test_d6_defects_by_sweep_is_monotone_and_strict_when_committing(cfg, monkeypatch):
    """D6 monotonicity, machine-checked: `defects_by_sweep` records the GLOBAL
    defect population at the top of every executed sweep. The invariant makes each
    surviving commit strictly decrease it, so the trace is non-increasing -- and
    strictly decreasing across any sweep that committed. That, plus the trace being
    a non-negative integer, is the termination proof: the loop reaches its fixed
    point BY CONSTRUCTION, never by exhausting the sweep cap."""
    geom = ExactSurface(_fin_mesh())
    plate_z = -5.0
    forest, contacts, carried, tip_r = _fin_forest(cfg)     # resolvable, no neighbour

    def _mlf():
        return engine.live_member_lists(forest, [], plate_z, cfg)

    members, balls = _mlf()
    rep = pd.run_phase8(forest, members, balls, contacts, carried, tip_r, plate_z,
                        geom, cfg, _fin_mesh(), unrouted=[], member_list_fn=_mlf)
    d = rep.defects_by_sweep
    assert d and d[0] >= 1, d
    assert all(a >= b for a, b in zip(d, d[1:])), d              # non-increasing
    for k, n_c in enumerate(rep.committed_by_sweep):
        if n_c > 0 and k + 1 < len(d):
            assert d[k + 1] < d[k], (k, d, rep.committed_by_sweep)   # STRICT
    assert rep.converged is True
    assert rep.sweeps < int(getattr(cfg, "p8_max_relocate_sweeps", 8)), \
        "convergence must be reached BEFORE the cap, not at it"


def test_d6_report_and_engine_ledger_carry_the_invariant_evidence(cfg):
    """The invariant's evidence is auditable from the artifacts, not only from a
    test: `Phase8Report.defects_by_sweep` reaches `log['relocate']` alongside the
    retirement rows, so an acceptance run's JSON shows the monotone trace and every
    honest-unresolved site (`D6-sterile` rows included) without re-running."""
    geom = ExactSurface(_fin_mesh())
    forest, contacts, carried, tip_r = _fin_forest(cfg)
    log = {}
    rep = engine.relocate(_fin_mesh(), forest, [], contacts, carried, tip_r, -5.0,
                          cfg, geom=geom, log=log)
    assert "defects_by_sweep" in rep and isinstance(rep["defects_by_sweep"], list)
    assert "n_retired" in rep and "retired_sites" in rep
    assert log["relocate"] is rep and log["relocate_p0"] is rep


# ==========================================================================
# 11. FINAL-ONLY RELOCATION STAGE (SPEC-P8-RELOCATE §3.0 amendment 2026-07-24).
# ==========================================================================
def test_relocation_is_a_single_final_only_stage(cfg, monkeypatch):
    """SPEC-P8-RELOCATE §3.0 amendment: relocation runs ONCE per engine run, AFTER
    every fix pass' construct/rewire/brace_and_size, immediately before the FINAL
    verify -- nothing mutates geometry after it except the verify that judges it.

    Pinned by the stage ledger: exactly one `relocate` stage, a `verify[final]`
    after it, no per-pass `relocate[pN]` stage at all, and the relocate timestamp
    strictly after the LAST brace_and_size. (Before the amendment, pass-1's
    rewire/brace ran after pass-0's relocation and drifted 3 embed ends back out of
    grade -- the mi 920/991/1010 class.)"""
    from support_placement import mesh_io, config as config_mod
    mesh = mesh_io.make_overhang_table()
    c = config_mod.SupportConfig(p8_relocate_enable=True)
    log = {}
    engine.run(mesh, c, log=log)
    stages = [k for k in log if k.startswith("t_")]
    assert "t_relocate" in stages, stages
    assert not [k for k in stages if k.startswith("t_relocate[p")], stages
    assert "t_verify[final]" in stages, stages
    braces = [log[k] for k in stages if k.startswith("t_brace_and_size[")]
    assert braces and log["t_relocate"] >= max(braces), (log["t_relocate"], braces)
    assert log["t_verify[final]"] >= log["t_relocate"]
    assert "relocate" in log and "relocate_p0" in log
    assert "verify_final" in log
