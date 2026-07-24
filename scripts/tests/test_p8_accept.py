"""Phase-8 ACCEPTANCE HARNESS tests (amendment G, SPEC-P8-RELOCATE.md sec 6
items 3, 7, 9, 10): `p8_accept.py` + the dependency-free `chord_canon.py` it
shares with any external census-comparison script. Also pins the additive
`verify.py` wiring these rest on: `source` tagging on clearance_report/
self_verify output, and the new `transit_chords_mm` report field.

Site-derived (task binding rule): a chord at EXACTLY 0.08mm (the strict-`<`
PRINT-FLOOR boundary) canonicalizes to a firm, non-exempt member end-to-end
through `clearance_report` -> `transit_chords_mm` -> `canon_chord_micron`.
"""
import numpy as np
import trimesh
import pytest

from support_placement import engine, p8_accept, phase8, tree as tree_mod, verify
from support_placement.chord_canon import (canon_chord_micron, chord_parity_report,
                                           chords_match_micron)
from support_placement.geometry_exact import ExactSurface
from support_placement.config import SupportConfig


@pytest.fixture
def cfg():
    return SupportConfig()


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(), process=False)


def _soup(*meshes):
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(),
                           process=False)


def _fin_mesh(fin_z):
    """Slab (top face z=0) + a floating 10x10 fin of thickness `fin_z`
    starting at z=1.0 -- mirrors test_v5_gates.py's PRINT-FLOOR fixture."""
    slab = _box((20, 20, 5), center=(0, 0, -2.5))
    fin = _box((10, 10, fin_z), center=(0, 0, 1.0 + fin_z / 2))
    return _soup(slab, fin)


def _fin_stub():
    return dict(p0=np.array([0.0, 0.0, -0.3]), p1=np.array([0.0, 0.0, 2.5]),
               r0=0.2, r1=0.35, kind="stub", owner=("contact", 0), embed_end="p0")


def _member(mi, p0, p1, r0=0.3, r1=0.3, kind="strut", owner=("route", 0)):
    return dict(p0=np.array(p0, float), p1=np.array(p1, float), r0=r0, r1=r1,
               kind=kind, owner=owner)


# ==========================================================================
# chord_canon.py -- dependency-free micron canonicalization (sec 6 item 3).
# ==========================================================================
def test_canon_chord_micron_rounds_to_nearest_micron():
    assert canon_chord_micron(0.0800000) == 80
    assert canon_chord_micron(0.0800004) == 80    # 0.4um noise, same bucket
    assert canon_chord_micron(0.0806) == 81       # clearly past the half-micron mark
    assert canon_chord_micron(0.0794) == 79


def test_canon_chord_micron_coarser_granularity():
    # micron=5 buckets to nearest 5-micron step.
    assert canon_chord_micron(0.0800, micron=5) == 80
    assert canon_chord_micron(0.0813, micron=5) == 80
    assert canon_chord_micron(0.0824, micron=5) == 80
    assert canon_chord_micron(0.0826, micron=5) == 85


def test_canon_chord_micron_rejects_nonpositive_granularity():
    with pytest.raises(ValueError):
        canon_chord_micron(0.08, micron=0)


def test_chords_match_micron_sub_micron_noise_matches():
    """The exact case the amendment exists for: two independent
    implementations' raw floats differ by sub-micron noise (the mi=445/474/575
    symdiff class) but canonicalize to the SAME integer micron."""
    assert chords_match_micron(0.25310, 0.25314) is True
    assert chords_match_micron(0.25310, 0.25360) is False


def test_chord_parity_report_matching_population_and_chords_ok():
    eng = {100: [0.25, 0.06], 200: [0.12]}
    census = {100: [0.2501, 0.0599], 200: [0.1204]}
    rep = chord_parity_report(eng, census)
    assert rep["ok"] is True
    assert rep["only_engine"] == [] and rep["only_census"] == []
    assert rep["mismatches"] == []
    assert rep["n_shared"] == 2


def test_chord_parity_report_population_mismatch():
    eng = {100: [0.25], 445: [0.2531]}
    census = {100: [0.25], 474: [0.30]}
    rep = chord_parity_report(eng, census)
    assert rep["ok"] is False
    assert rep["only_engine"] == [445]
    assert rep["only_census"] == [474]


def test_chord_parity_report_chord_value_mismatch():
    eng = {100: [0.25]}
    census = {100: [0.40]}      # same member, genuinely different reading
    rep = chord_parity_report(eng, census)
    assert rep["ok"] is False
    assert rep["only_engine"] == [] and rep["only_census"] == []
    assert rep["mismatches"] == [dict(member_idx=100, engine_micron=(250,),
                                      census_micron=(400,))]


# ==========================================================================
# ==0.08 boundary fixture, end-to-end through clearance_report's new
# `transit_chords_mm` / `source` wiring (amendment G sec 6 item 3 + item 8).
# ==========================================================================
def test_boundary_chord_exactly_0_08_is_firm_and_canonicalizes_to_80(cfg):
    geom = ExactSurface(_fin_mesh(0.08))
    moot_out = []
    chords_out = {}
    rep = verify.clearance_report(None, [_fin_stub()], cfg, geom=geom,
                                  moot_transit_out=moot_out,
                                  transit_chords_out=chords_out)
    assert [v["member_idx"] for v in rep] == [0]          # NON-exempt: hard gate
    assert rep[0]["source"] == "transit"
    assert moot_out == []                                  # never in the moot bucket
    assert 0 in chords_out
    micron_readings = [canon_chord_micron(c) for c in chords_out[0]]
    assert 80 in micron_readings                            # the 0.08mm crossing itself


def test_boundary_chord_just_under_0_08_is_moot(cfg):
    geom = ExactSurface(_fin_mesh(0.0796))
    moot_out = []
    chords_out = {}
    rep = verify.clearance_report(None, [_fin_stub()], cfg, geom=geom,
                                  moot_transit_out=moot_out,
                                  transit_chords_out=chords_out)
    assert rep == []
    assert [v["member_idx"] for v in moot_out] == [0]
    assert moot_out[0]["source"] == "transit"
    assert 0 in chords_out


def test_self_verify_carries_transit_chords_mm_field(cfg):
    """self_verify's new report key is always present (empty dict on a clean
    mesh), so callers/tests can rely on the shape without a getattr guard."""
    geom = ExactSurface(_fin_mesh(0.08))
    members = [_fin_stub()]
    report = verify.self_verify(None, [], [], members, np.array([1.0]),
                                np.array([0.2]), cfg, plate_z=0.0, geom=geom)
    assert "transit_chords_mm" in report
    assert 0 in report["transit_chords_mm"]


# ==========================================================================
# zero_new_firm_transit_gate (sec 6 item 9 sub-gate).
# ==========================================================================
def test_zero_new_firm_transit_gate_ok_when_subset_of_target_set():
    violations = [dict(member_idx=5, source="transit"),
                  dict(member_idx=7, source="transit")]
    rep = p8_accept.zero_new_firm_transit_gate(violations, target_set={5, 7, 9})
    assert rep["ok"] is True and rep["new_firm_transit_members"] == []


def test_zero_new_firm_transit_gate_flags_member_outside_target_set():
    violations = [dict(member_idx=5, source="transit"),
                  dict(member_idx=42, source="transit")]
    rep = p8_accept.zero_new_firm_transit_gate(violations, target_set={5})
    assert rep["ok"] is False
    assert rep["new_firm_transit_members"] == [42]


def test_zero_new_firm_transit_gate_ignores_distance_source():
    """A distance collision outside the target set is a DIFFERENT failure
    class (already covered by the general clearance gate) -- this gate is
    named for TRANSIT regressions specifically and must not flag it."""
    violations = [dict(member_idx=99, source="distance")]
    rep = p8_accept.zero_new_firm_transit_gate(violations, target_set=set())
    assert rep["ok"] is True and rep["new_firm_transit_members"] == []


# ==========================================================================
# raft_connectivity_report (sec 6 item 9 sub-gate).
# ==========================================================================
def _add(forest, mi, parent=None):
    m = tree_mod.Member(p0=np.array([0.0, 0.0, float(mi)]),
                        p1=np.array([0.0, 0.0, float(mi) + 1.0]),
                        r0=0.3, r1=0.3, kind="strut", owner=("route", mi))
    idx = forest.add_member(m)
    if parent is not None:
        forest.set_parent(idx, parent)
    return idx


def test_raft_connectivity_clean_forest_has_no_orphans():
    f = tree_mod.Forest()
    root = _add(f, 0)
    child = _add(f, 1, parent=root)
    _add(f, 2, parent=child)
    rep = p8_accept.raft_connectivity_report(f)
    assert rep["ok"] is True and rep["orphans"] == []


def test_raft_connectivity_flags_dead_ancestor_orphan():
    """The exact bug class the guard exists for: a member is retired via
    `mark_dead` WITHOUT first reattaching its children (the caller contract
    `mark_dead`'s own docstring demands, but does not itself enforce) --
    the child's parent chain now dead-ends on a dead member instead of
    reaching a live root."""
    f = tree_mod.Forest()
    root = _add(f, 0)
    child = _add(f, 1, parent=root)
    grandchild = _add(f, 2, parent=child)
    f.mark_dead(child)                     # child retired, grandchild left dangling
    rep = p8_accept.raft_connectivity_report(f)
    assert rep["ok"] is False
    offenders = {o["member_idx"] for o in rep["orphans"]}
    assert grandchild in offenders
    assert root not in offenders and child not in offenders  # root/dead-itself not flagged


def test_raft_connectivity_flags_cycle_without_infinite_loop():
    f = tree_mod.Forest()
    a = _add(f, 0)
    b = _add(f, 1, parent=a)
    f.parent[a] = b                        # force a 2-cycle a<->b (topology bug)
    if a in f.roots:
        f.roots.remove(a)
    rep = p8_accept.raft_connectivity_report(f)   # must terminate, not hang
    assert rep["ok"] is False
    assert any(o["reason"] == "cycle" for o in rep["orphans"])


def test_raft_connectivity_skips_dead_and_brace_members():
    f = tree_mod.Forest()
    root = _add(f, 0)
    dead_root = _add(f, 1)
    f.mark_dead(dead_root)                 # dead root itself: not walked, not flagged
    brace = tree_mod.Member(p0=np.array([1.0, 0.0, 0.0]), p1=np.array([2.0, 0.0, 0.0]),
                            r0=0.2, r1=0.2, kind="brace", owner=("rung", 0))
    f.add_brace(brace)
    rep = p8_accept.raft_connectivity_report(f)
    assert rep["ok"] is True


# ==========================================================================
# criticality_weighted_unresolved_gate (sec 6 item 7).
# ==========================================================================
def _strut_res(sid, unresolved=True, crit=1.0, rung="S3"):
    return phase8.StrutResult(site_id=sid, disposition=phase8.Disposition.STRUT_S3_DEAD,
                              contract="mark_dead", rung=rung, unresolved=unresolved,
                              criticality=crit, trace=("S1", "S2", "S3"))


def _ball_res(bid, unresolved=True, crit=1.0, rung="B3"):
    return phase8.BallResult(ball_id=bid, disposition=phase8.Disposition.BALL_B3_UNRESOLVED,
                             rung=rung, unresolved=unresolved, criticality=crit,
                             trace=("B1", "B2", "B3"))


def test_criticality_gate_ok_under_both_bounds(cfg):
    results = [_strut_res(1, crit=1.0), _ball_res(2, crit=1.5),
              _strut_res(3, unresolved=False, crit=99.0)]   # resolved: excluded
    rep = p8_accept.criticality_weighted_unresolved_gate(results, cfg)
    assert rep["ok"] is True
    assert rep["count"] == 2
    assert rep["weight"] == pytest.approx(2.5)
    klasses = {it["klass"] for it in rep["items"]}
    assert klasses == {"STRUT", "BALL"}


def test_criticality_gate_fails_on_raw_count_bound(cfg):
    results = [_strut_res(i, crit=0.01) for i in range(cfg.p8_max_unresolved + 1)]
    rep = p8_accept.criticality_weighted_unresolved_gate(results, cfg)
    assert rep["ok"] is False
    assert rep["count"] == cfg.p8_max_unresolved + 1


def test_criticality_gate_fails_on_weighted_sum_even_under_count_bound():
    """The bound cannot be satisfied by abandoning many high-criticality sites
    under a raw count (sec 6 item 7): 2 unresolved items, well under
    p8_max_unresolved=10, but their combined criticality exceeds
    p8_max_unresolved_weight=3.0."""
    cfg = SupportConfig()
    results = [_strut_res(1, crit=2.0), _ball_res(2, crit=1.5)]
    rep = p8_accept.criticality_weighted_unresolved_gate(results, cfg)
    assert rep["count"] == 2 and rep["count"] <= cfg.p8_max_unresolved
    assert rep["weight"] == pytest.approx(3.5)
    assert rep["weight"] > cfg.p8_max_unresolved_weight
    assert rep["ok"] is False


def test_criticality_gate_position_lookup_surfaces_in_items():
    cfg = SupportConfig()
    results = [_strut_res(7, crit=1.0)]
    rep = p8_accept.criticality_weighted_unresolved_gate(
        results, cfg, position_lookup={7: (1.0, 2.0, 3.0)})
    assert rep["items"][0]["position"] == (1.0, 2.0, 3.0)


# ==========================================================================
# geometry_digest / idempotence_gate (sec 6 item 10).
# ==========================================================================
def test_geometry_digest_stable_across_equal_but_distinct_objects():
    m1 = [_member(0, [0, 0, 0], [0, 0, 1])]
    m2 = [_member(0, [0, 0, 0], [0, 0, 1])]   # distinct dict instances, same values
    assert p8_accept.geometry_digest(m1, []) == p8_accept.geometry_digest(m2, [])


def test_geometry_digest_changes_when_a_member_moves():
    m1 = [_member(0, [0, 0, 0], [0, 0, 1])]
    m2 = [_member(0, [0, 0, 0], [0, 0, 1.001])]
    assert p8_accept.geometry_digest(m1, []) != p8_accept.geometry_digest(m2, [])


def test_idempotence_gate_ok_on_empty_snapshot_and_matching_digest():
    snap = phase8.DefectSnapshot(non_exempt=(), moot=(), quarantine_real=(),
                                 quarantine_artifact=(), ball_breaches=(),
                                 partition={}, provenance={})
    rep = p8_accept.idempotence_gate(snap, "abc123", "abc123")
    assert rep["ok"] is True


def test_idempotence_gate_fails_on_nonempty_second_pass_snapshot():
    snap = phase8.DefectSnapshot(non_exempt=(17,), moot=(), quarantine_real=(),
                                 quarantine_artifact=(), ball_breaches=(),
                                 partition={}, provenance={})
    rep = p8_accept.idempotence_gate(snap, "abc123", "abc123")
    assert rep["ok"] is False and rep["empty_snapshot"] is False


def test_idempotence_gate_fails_on_digest_mismatch():
    snap = phase8.DefectSnapshot(non_exempt=(), moot=(), quarantine_real=(),
                                 quarantine_artifact=(), ball_breaches=(),
                                 partition={}, provenance={})
    rep = p8_accept.idempotence_gate(snap, "abc123", "def456")
    assert rep["ok"] is False and rep["digest_match"] is False


# ==========================================================================
# final_geometry_verify -- integration: pickle round-trip + engine.finish +
# both sub-gates, on a real (small) mesh through the real construct pipeline.
# ==========================================================================
def test_final_geometry_verify_round_trips_and_gates_clean_forest(table_mesh, cfg):
    from support_placement import routing
    from support_placement.geometry_kernel import KernelSurface

    log = {}
    geom = KernelSurface(table_mesh)
    pre = engine.prepare(table_mesh, cfg, log, geom=geom)
    surface_tree = routing.build_surface_tree(table_mesh, pitch_mm=0.3, max_points=400000)
    forest, unrouted, carried, tip_r, contacts = engine.construct(
        table_mesh, pre, surface_tree, cfg, geom=geom)

    rep = p8_accept.final_geometry_verify(
        table_mesh, pre, forest, unrouted, contacts, carried, tip_r,
        surface_tree, cfg, geom=geom)

    assert set(["ok", "verify", "zero_new_firm_transit", "raft_connectivity",
               "members", "balls"]) <= set(rep)
    assert rep["raft_connectivity"]["ok"] is True    # a real constructed forest is connected
    # empty target_set (default): every transit-violating member, if any, is
    # necessarily "outside" it -- so this sub-gate's `ok` reduces exactly to
    # "the run has zero transit violations".
    znt = rep["zero_new_firm_transit"]
    assert znt["ok"] == (znt["n_firm_transit"] == 0)
