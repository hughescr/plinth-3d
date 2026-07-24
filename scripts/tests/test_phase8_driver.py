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
