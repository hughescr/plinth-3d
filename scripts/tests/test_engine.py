import json
import os
import pickle
import dataclasses

import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import (
    config as config_mod, engine, mesh_io, phase8_driver, placement, rewire,
    tree as tm, verify,
)


def test_prepared_state_pickle_roundtrip_reuses_pool_without_recompute(
        table_mesh, cfg, monkeypatch):
    pre = engine.prepare(table_mesh, cfg)
    reloaded = pickle.loads(pickle.dumps(pre, protocol=pickle.HIGHEST_PROTOCOL))
    assert reloaded["schema"] == engine.PREPARED_STATE_SCHEMA
    assert reloaded["version"] == engine.PREPARED_STATE_SCHEMA_VERSION
    assert reloaded["mesh_fingerprint"] == engine.mesh_fingerprint(table_mesh)
    assert reloaded["config_fingerprint"] == engine.config_fingerprint(cfg)
    engine.validate_prepared_state(reloaded, table_mesh, cfg)
    pool = reloaded["contact_candidate_pool"]
    assert pool.nbytes == len(pool) * placement.CONTACT_POOL_BYTES_PER_CANDIDATE

    def _refuse_recompute(*_args, **_kwargs):
        raise AssertionError("prepared pool was recomputed")

    monkeypatch.setattr(placement, "candidate_pool", _refuse_recompute)
    regular = placement.sample_candidates(
        table_mesh, reloaded["islands"], cfg,
        candidate_pool_prepared=pool)
    assert regular
    iid = int(regular[0]["island_id"])
    repaired = engine._add_fix_contacts(
        table_mesh, reloaded["islands"], [regular[0]],
        [dict(island_id=iid, n_faces=len(reloaded["islands"][iid]))],
        candidate_pool_prepared=pool)
    assert len(repaired) == 2
    idx = phase8_driver.SurfaceSeedIndex(
        table_mesh, islands=reloaded["islands"],
        candidate_pool_prepared=pool, cfg=cfg)
    assert idx.pool_positions is pool.positions
    report = verify.self_verify(
        table_mesh, reloaded["islands"], regular, [], np.array([]), np.array([]),
        cfg, plate_z=0.0, contact_candidate_pool=pool)
    assert report["ok"] is True
    assert report["regular_contact_invariant_violations"] == []


def test_staged_consumers_reject_old_prepared_state(table_mesh, cfg):
    with pytest.raises(ValueError, match="rerun prepare"):
        engine.construct(table_mesh, {}, None, cfg)
    with pytest.raises(ValueError, match="rerun prepare"):
        engine.finish(
            table_mesh, {}, None, [], [], np.array([]), np.array([]),
            None, cfg)


def test_prepared_state_rejects_mismatched_mesh_and_semantic_config(
        table_mesh, cfg):
    pre = engine.prepare(table_mesh, cfg)
    changed_mesh = table_mesh.copy()
    changed_mesh.vertices[0, 0] = np.nextafter(
        changed_mesh.vertices[0, 0], np.inf)
    with pytest.raises(ValueError, match="mesh fingerprint mismatch"):
        engine.construct(changed_mesh, pre, None, cfg)
    with pytest.raises(ValueError, match="mesh fingerprint mismatch"):
        engine.finish(
            changed_mesh, pre, None, [], [], np.array([]), np.array([]),
            None, cfg)
    with pytest.raises(ValueError, match="mesh fingerprint mismatch"):
        engine.relocate(
            changed_mesh, None, [], [], np.array([]), np.array([]), 0.0, cfg,
            prepared_state=pre)
    changed_cfg = dataclasses.replace(
        cfg, target_spacing_mm=np.nextafter(cfg.target_spacing_mm, np.inf))
    with pytest.raises(ValueError, match="config fingerprint mismatch"):
        engine.construct(table_mesh, pre, None, changed_cfg)


def test_engine_runs_end_to_end_on_table_fixture(table_mesh, cfg):
    # The table fixture is deliberately adversarial: overhang contacts on
    # the slab underside can land directly above a leg. That's an honest
    # limitation, not a bug: coverage and load must still be clean, and
    # self_verify must still SURFACE any remaining clearance issue rather
    # than silently reporting ok.
    result = engine.run(table_mesh, cfg)
    assert len(result["contacts"]) > 0
    # v4 contract: every contact either gets a member in the forest or is
    # honestly reported unrouted -- never a through-model member.
    n_terminal_members = sum(1 for m in result["members"] if m["kind"] == "stub")
    assert n_terminal_members + len(result["unrouted"]) == len(result["contacts"])
    # v3 baseline on this fixture (fixtures/v3_baseline.json) was 2
    # unrouted; v4 Phase 2 (straight segments only, no curved descent/
    # rewiring yet) is allowed up to 4 pending Phase 3 rewiring and Phase
    # 4 curvature attacking the residual -- also visible directly as
    # result["log"]["n_unrouted"] (verifier should-fix: surfaced at the
    # top level of the log so a regression shows up without digging into
    # the unrouted list or a per-pass dict).
    assert len(result["unrouted"]) <= 4
    assert result["log"]["n_unrouted"] == len(result["unrouted"])
    assert result["verify"]["load_violations"] == []
    # the no-intersection guarantee: zero clearance violations, always
    assert result["verify"]["clearance_violations"] == [], result["verify"]
    # nothing floats -- every emitted member endpoint attaches to
    # something (another member's axis, the plate, or the model).
    assert result["verify"]["connectivity_violations"] == [], result["verify"]
    # v4 p6 (I3): the honest slice-based island-birth gate -- HARD
    # acceptance for this fixture, not report-only (that's the Treant-
    # scale run, which additionally depends on the parallel p6-routing
    # workstream).
    assert result["verify"]["uncovered_island_births"] == [], result["verify"]
    # regression tripwire only, per Craig's correctness-over-speed ruling
    # (never trade correctness/resin for speed): pre-v5 cloud-based
    # composed-engine baseline was ~10.2s (2026-07-22). The v5-geom-core
    # Python-exact swap (SPEC-V5-GEOM) measured ~19.5-20.1s and sat at a
    # < 30.0s pin. SPEC-RUST-KERNEL WIRE (2026-07-23) replaced the geometry
    # hot core with the Rust geom_kernel (parallel across all cores): the
    # SAME fixture now runs ~7.5s on this machine (M3 Max). Re-baselined to
    # < 15.0s as an honest tripwire -- ~2x measured headroom (absorbs
    # machine/thermal variance) while still tripping on any regression back
    # toward the Python-exact ~20s or worse. NOT a speed target (correctness
    # and resin never trade against wall-time); re-measure, don't guess.
    assert result["log"]["t_total"] < 15.0


def test_engine_insertion_accounting_exact(table_mesh, cfg):
    """Every contact is routed (has a stub member in the forest) or
    honestly unrouted, exactly once -- no double-counting, nothing
    silently dropped."""
    result = engine.run(table_mesh, cfg)
    n = len(result["contacts"])
    routed_idx = {owner[1] for m in result["members"]
                  for owner in [m["owner"]] if m["kind"] == "stub"}
    unrouted_idx = set(result["unrouted"])
    assert routed_idx.isdisjoint(unrouted_idx)
    assert routed_idx | unrouted_idx == set(range(n))


def test_engine_per_segment_printability_green_on_table(table_mesh, cfg):
    """v4 new gate: per-segment tangent+taper printability (design-v4.md
    #3, #11), checked here directly against the returned forest (not just
    via verify['ok'], so a regression in this specific gate is caught
    even if something else masks 'ok')."""
    result = engine.run(table_mesh, cfg)
    assert result["verify"]["printability_violations"] == [], result["verify"]


def test_engine_murray_conservation_holds_on_table(table_mesh, cfg):
    result = engine.run(table_mesh, cfg)
    assert result["verify"]["murray_violations"] == [], result["verify"]


def test_engine_prepare_keeps_lifted_box_bottom_faces_as_demand(box_mesh, cfg):
    """A lifted flat bottom is unsupported demand, not plate-grounded."""
    pre = engine.prepare(box_mesh, cfg)
    bottom = set(np.flatnonzero(np.isclose(
        box_mesh.triangles_center[:, 2], box_mesh.bounds[0][2])))
    demanded = set(np.concatenate(pre["islands"]))
    assert pre["plate_z"] == pytest.approx(
        float(box_mesh.bounds[0][2]) - cfg.lift_mm)
    assert bottom <= demanded
    assert len(pre["contacts"]) > 0


def test_engine_box_supports_bottom_plateau_demand(box_mesh, cfg):
    """The lifted box's first FEP island remains supported end to end.

    A2's regular pool lands directly on the plateau's lowest vertices, so the
    minimum demand is already satisfied and need not add a redundant
    ``minimum=True`` specialized contact.
    """
    result = engine.run(box_mesh, cfg)
    assert result["log"]["minima"] == 1
    assert len(result["contacts"]) > 0
    assert result["verify"]["unsupported_minima"] == []
    regular = [c for c in result["contacts"] if not c.get("minimum")]
    assert regular and all("face_id" in c for c in regular)
    assert result["verify"]["ok"], result["verify"]


def test_engine_supports_toe_minima_directly(toed_slab_mesh, cfg):
    """The layer-island rule end to end: both toe apexes (including the
    steep one the overhang-angle island model cannot see) end up with a
    routed contact AT the birth region, with the no-intersection
    guarantee intact.

    v4 p6 (I4, Craig's strict criterion): tightened from
    cfg.minima_contact_radius_mm (1.0mm -- that field's own docstring
    says it was never meant as a coverage radius, "only absorbs sampling
    gaps") to perception.ISLAND_ATTACH_REACH_MM (0.2mm, the same
    conservative attach-reach the new island-birth demand/gate machinery
    uses) -- strictly TIGHTER, never looser. The routed contact is placed
    directly at the apex by _ensure_minima_contacts, so this stays a
    near-zero-mm distance either way; only the STATED tolerance changed."""
    from support_placement import perception
    result = engine.run(toed_slab_mesh, cfg)
    assert result["verify"]["clearance_violations"] == [], result["verify"]
    assert result["verify"]["connectivity_violations"] == [], result["verify"]
    assert result["verify"]["unsupported_minima"] == []
    assert result["verify"]["uncovered_island_births"] == []
    routed = {m["owner"][1] for m in result["members"] if m["kind"] == "stub"}
    for apex in (np.array([8.0, 0.0, 9.0]), np.array([-8.0, 0.0, 11.0])):
        d = min(float(np.linalg.norm(
            np.asarray(result["contacts"][i]["position"]) - apex))
            for i in routed)
        assert d <= perception.ISLAND_ATTACH_REACH_MM, (apex, d)


def test_engine_wall_time_pin_on_table(table_mesh, cfg):
    result = engine.run(table_mesh, cfg)
    # regression tripwire only, per Craig's correctness-over-speed ruling
    # (never trade correctness/resin for speed): pre-v5 cloud-based
    # composed-engine baseline was ~10.2s (2026-07-22). The v5-geom-core
    # Python-exact swap (SPEC-V5-GEOM) measured ~19.5-20.1s and sat at a
    # < 30.0s pin. SPEC-RUST-KERNEL WIRE (2026-07-23) replaced the geometry
    # hot core with the Rust geom_kernel (parallel across all cores): the
    # SAME fixture now runs ~7.5s on this machine (M3 Max). Re-baselined to
    # < 15.0s as an honest tripwire -- ~2x measured headroom (absorbs
    # machine/thermal variance) while still tripping on any regression back
    # toward the Python-exact ~20s or worse. NOT a speed target (correctness
    # and resin never trade against wall-time); re-measure, don't guess.
    assert result["log"]["t_total"] < 15.0


def test_engine_construct_is_deterministic_despite_global_rng_state(table_mesh, cfg):
    """Charter: no RNG. engine.construct (via placement.load_partition)
    and its surface_tree (via routing.build_surface_tree) both draw from
    the GLOBAL numpy random state through trimesh's mesh.sample(). If
    either isn't seeded internally, a fix-pass loop or a re-run on the
    same mesh can silently produce a different forest depending on
    whatever ran before it. Perturb the global RNG differently before
    each call; the resulting forest must be bit-identical (member count,
    positions, radii)."""
    from support_placement import routing

    log = {}
    pre = engine.prepare(table_mesh, cfg, log)

    np.random.seed(12345)
    surface_tree_a = routing.build_surface_tree(table_mesh, pitch_mm=0.3, max_points=400000)
    forest_a, unrouted_a, carried_a, tip_r_a, contacts_a = engine.construct(
        table_mesh, pre, surface_tree_a, cfg)

    np.random.seed(54321)
    surface_tree_b = routing.build_surface_tree(table_mesh, pitch_mm=0.3, max_points=400000)
    forest_b, unrouted_b, carried_b, tip_r_b, contacts_b = engine.construct(
        table_mesh, pre, surface_tree_b, cfg)

    assert len(forest_a.members) == len(forest_b.members)
    assert unrouted_a == unrouted_b
    assert np.array_equal(carried_a, carried_b)
    assert np.array_equal(tip_r_a, tip_r_b)
    for ma, mb in zip(forest_a.members, forest_b.members):
        assert np.array_equal(ma.p0, mb.p0)
        assert np.array_equal(ma.p1, mb.p1)
        assert ma.r0 == mb.r0
        assert ma.r1 == mb.r1


# =================================================================
# v4 p6 (I1/I2/I3): slice-based island-birth detection, on-footprint
# contact demand, and the honest at-birth coverage gate
# =================================================================
def test_engine_prepare_demands_ladder_contacts_old_detector_misses(raking_blade_mesh, cfg):
    """TDD failing-first case (fails on master -- there is no
    _ensure_island_birth_contacts / perception.island_births on master, so
    engine.prepare only ever produces the OLD detector's single apex
    contact for this fixture, which verify.island_birth_report -- also new
    -- would then correctly flag as leaving the rest of the ladder
    uncovered): engine.prepare's new I2 step must demand and add contacts
    for the raking-blade births local_z_minima's single apex misses, and
    the resulting FULL contact set must satisfy the strict I3 gate even
    though the old-detector-only subset does not."""
    from support_placement import verify as verify_mod

    log = {}
    pre = engine.prepare(raking_blade_mesh, cfg, log)
    assert log["island_births"] >= 5
    assert log["island_birth_contacts_added"] > 0, (
        "I2 must demand extra contacts for the raking ladder beyond what "
        "local_z_minima's single apex minimum already covers")

    old_detector_only = [c for c in pre["contacts"] if not c.get("island_birth")]
    uncovered_old = verify_mod.island_birth_report(pre["island_births"], old_detector_only, cfg)
    assert len(uncovered_old) > 0, (
        "the OLD detector's contacts alone should leave real gaps in the "
        "strict ladder gate -- this is the failing-first half of the proof")

    uncovered_new = verify_mod.island_birth_report(pre["island_births"], pre["contacts"], cfg)
    assert uncovered_new == [], (
        "the FULL demanded contact set (old detector + I2 ladder) must "
        f"clear the strict gate: {uncovered_new}")


def test_engine_prepare_island_births_deterministic_despite_global_rng_state(
        raking_blade_mesh, cfg):
    """Charter requirement: two preps bit-identical. engine.prepare's new
    I1/I2 steps must not introduce any hidden RNG dependency -- perturb the
    global numpy RNG differently before each call (same technique as
    test_engine_construct_is_deterministic_despite_global_rng_state) and
    require identical island_births and contacts."""
    np.random.seed(111)
    log_a = {}
    pre_a = engine.prepare(raking_blade_mesh, cfg, log_a)

    np.random.seed(999)
    log_b = {}
    pre_b = engine.prepare(raking_blade_mesh, cfg, log_b)

    assert len(pre_a["island_births"]) == len(pre_b["island_births"])
    for ba, bb in zip(pre_a["island_births"], pre_b["island_births"]):
        assert ba["x"] == bb["x"] and ba["y"] == bb["y"] and ba["z"] == bb["z"]
        assert ba["area_mm2"] == bb["area_mm2"]

    assert len(pre_a["contacts"]) == len(pre_b["contacts"])
    for ca, cb in zip(pre_a["contacts"], pre_b["contacts"]):
        assert np.array_equal(np.asarray(ca["position"]), np.asarray(cb["position"]))
        assert bool(ca.get("island_birth")) == bool(cb.get("island_birth"))
    assert log_a["island_birth_contacts_added"] == log_b["island_birth_contacts_added"]


def test_engine_prepare_threads_specialized_reseat_domains(
        raking_blade_mesh, cfg):
    """Production birth contacts carry the dual-flag payload R2 must gate."""
    pre = engine.prepare(raking_blade_mesh, cfg, {})
    births = [c for c in pre["contacts"] if c.get("island_birth")]
    assert births
    for contact in births:
        assert contact.get("minimum") is True
        assert np.asarray(contact["birth_loop"]).ndim == 2
        assert np.asarray(contact["birth_loop"]).shape[1] == 2
        assert contact["birth_z"] == pytest.approx(
            float(contact["position"][2]), abs=cfg.layer_height_mm
            * cfg.minima_connect_layers)


def test_minimum_contact_threads_region_payload(box_mesh, cfg):
    pos = np.asarray(box_mesh.vertices[0], float)
    region = np.asarray(box_mesh.vertices[[0, 2, 4]], float)
    contacts, _islands, added = engine._ensure_minima_contacts(
        box_mesh, [], [],
        [dict(position=pos, normal=np.array([0.0, 0.0, -1.0]),
              region_pts=region, z=float(pos[2]))],
        cfg)
    assert added == 1
    assert contacts[0]["minimum"] is True
    assert np.array_equal(contacts[0]["region_pts"], region)


def test_engine_lifted_thin_box_does_not_take_no_contacts_path(cfg):
    """A lifted thin box still has unsupported bottom demand.

    This used to exercise the empty-result path only because the mesh's own
    z-min was mistaken for the plate.
    """
    import trimesh
    flat = trimesh.creation.box(extents=(10, 10, 0.5))
    m = trimesh.Trimesh(vertices=flat.vertices.copy(), faces=flat.faces.copy(),
                         process=True)
    result = engine.run(m, cfg)
    assert len(result["contacts"]) > 0
    bottom = set(np.flatnonzero(np.isclose(
        m.triangles_center[:, 2], m.bounds[0][2])))
    demanded = set(np.concatenate(result["islands"]))
    assert bottom <= demanded
    assert result["verify"]["clearance_violations"] == []
    assert result["verify"]["connectivity_violations"] == []
    assert set(result.keys()) >= {
        "islands", "contacts", "carried_area_mm2", "tip_r_mm", "forest",
        "members", "balls", "unrouted", "minima", "plate_z", "verify",
        "volume", "log"}


# =================================================================
# Phase 4c: engine.brace_and_size (SPEC-INTEGRATION #3)
# =================================================================
def _far_tree():
    return cKDTree(np.array([[1e6, 1e6, 1e6]]))


def _m(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("t", 0), embed_end=None, carried_mm2=0.0):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner,
                      embed_end=embed_end, carried_mm2=carried_mm2)


def test_brace_and_size_shrinks_a_braceable_tall_twin_column(cfg, box_mesh):
    """The 14x-lever smoke test (SPEC-INTEGRATION #4, kickoff doc: 'a
    ~14x per-column lever'): two tall (80mm), close (4mm apart) parallel
    plate columns -- far enough from any real mesh that clearance is
    trivial (a far single-point surface_tree), so the only thing that can
    move member_ml is the fixed point itself. brace_and_size must (a)
    measurably shrink member_ml, (b) actually place braces, (c) leave the
    forest gate-clean (murray/printability/certificates)."""
    from support_placement import cco, placement

    plate_z = -50.0

    def make_column(x, carried):
        L = 30.0 - plate_z
        r = max(cco._r_tension_mm(carried, cfg),
                placement._shaft_r_for_L(L, cfg, 0.0), cfg.min_trunk_r_mm)
        return tm.Member(p0=[x, 100.0, 30.0], p1=[x, 100.0, plate_z], r0=r, r1=r,
                          kind="trunk", owner=("test", int(x)), embed_end=None,
                          carried_mm2=carried)

    f = tm.Forest()
    f.add_member(make_column(0.0, 300.0))
    f.add_member(make_column(4.0, 300.0))

    log = {}
    out = engine.brace_and_size(box_mesh, f, [], _far_tree(), plate_z, cfg, log=log)

    assert out["member_ml_after"] < out["member_ml_before"], out
    assert out["member_ml_after"] < out["member_ml_before"] / 4.0, (
        f"expected the bracing lever to be dramatic, not marginal: {out}")
    assert out["rungs_placed"] > 0
    assert len(f.braces) > 0
    tm.assert_all_certified(f)
    live = [f.members[i] for i in range(len(f.members)) if i not in f.dead]
    from support_placement import verify as verify_mod
    assert verify_mod.murray_report(f, cfg) == []
    assert tm.printability_report(live, cfg) == []
    assert log["brace_and_size"] == out


def test_brace_and_size_activation_timing_integration(cfg, box_mesh):
    """SPEC-INTEGRATION #4 activation-timing bullet: a forest whose only
    restraint is a lagged Y-junction (transient.py test 6's own fixture,
    reproduced here) uses MORE total live print material than an
    otherwise-identical forest with an early-activating brace at the
    junction height -- ties transient_report's activation-aware
    semantics through the FULL brace_and_size loop (not just transient.
    resize_forest in isolation, which test_transient.py already pins).

    SPEC-PATHRUNG W3 reconciliation (independently re-traced, not just
    trusting the Phase-4d implementer's own note): the ORIGINAL
    assertion compared max(r0, r1) on the SAME member index (`trunk`)
    across both runs. That comparison is no longer sound now that
    resize_forest correctly floors on r_deflect (charter #6) IN
    ADDITION to r_transient (#8) -- NOT because r_deflect happens to
    dominate here (traced and ruled out: r_deflect(20mm, lean=0) ==
    0.8203mm, well under the observed ~1.03mm), but because of a real,
    separate structural consequence: brace_and_size's own rewire.
    optimize pass finds a genuinely cheaper topology for the UNBRACED
    forest (splitting the Y-junction's two branches onto independent
    descending connectors through a shared bifurcation point instead of
    one shared trunk) and takes it -- `trunk`'s own member_idx retires
    (tree.Forest.dead), its r0/r1 left FROZEN at whatever they were the
    instant before retirement. The BRACED forest's `trunk` is instead
    PROTECTED from that exact restructuring by rewire's own brace-
    awareness guard (module docstring: "retiring root_idx would strand
    that brace... refused outright when root_idx is [a braced host]"),
    a real, load-bearing safety rule (SPEC-INTEGRATION #2) rather than a
    defect -- this workstream does not touch it. The two `trunk`
    readings are therefore comparing a topology-divergent, one-shot-
    frozen dead member against a live, differently-supported one:
    incomparable by construction, independent of any r_deflect/
    r_transient interaction at all.

    Old (unsound) expectation: r_braced < r_unbraced, read off a single
    member index assumed stable across both runs.
    New expectation, computed from the charter formula in THIS test
    body (design-v4.md #6/#11's own accounting, tree.volume_report/
    rewire._total_volume_ml -- both already used elsewhere in this
    suite for exactly this "how much real material got printed"
    question): total LIVE print volume is topology-agnostic (survives
    the reattach divergence) and directly measures whether the
    early-activating restraint actually let the structure use less
    material overall -- the SAME physical claim the original assertion
    was reaching for, on a metric that stays well-defined regardless of
    which specific member ends up carrying which span."""
    def y_junction_forest():
        f = tm.Forest()
        trunk = f.add_member(_m([0.0, 0.0, 20.0], [0.0, 0.0, 0.0], kind="trunk"))
        a = f.add_member(_m([0.0, 0.0, 58.0], [0.0, 0.0, 20.0], kind="shoot"))
        f.set_parent(a, trunk)
        b = f.add_member(_m([0.0, 0.0, 45.0], [0.0, 0.0, 20.0], kind="shoot"))
        f.set_parent(b, trunk)
        sa = f.add_member(_m([0.0, 0.0, 60.0], [0.0, 0.0, 58.0], kind="stub", embed_end="p0"))
        f.set_parent(sa, a)
        sb = f.add_member(_m([0.0, 0.0, 47.0], [0.0, 0.0, 45.0], kind="stub", embed_end="p0"))
        f.set_parent(sb, b)
        return f, trunk

    f_u, trunk_u = y_junction_forest()
    engine.brace_and_size(box_mesh, f_u, [], _far_tree(), plate_z=0.0, cfg=cfg, log={})

    f_b, trunk_b = y_junction_forest()
    # Early-activating restraint at the SAME z as the junction (20.0):
    # a model_rung (embed_end="p1") activates the instant both its own
    # endpoints exist -- deliberately hand-placed (not searched for by
    # bracing.propose_and_place, which has no real model surface to find
    # here) to isolate exactly the activation-timing effect under test.
    f_b.add_brace(_m([0.0, 0.0, 20.0], [3.0, 0.0, 20.0], r0=cfg.brace_r_mm,
                      r1=cfg.brace_r_mm, kind="model_rung", embed_end="p1"))
    engine.brace_and_size(box_mesh, f_b, [], _far_tree(), plate_z=0.0, cfg=cfg, log={})

    # Sanity, independent of the volume claim below: the braced trunk's
    # own member_idx really does survive live (the guard this test's
    # reconciliation depends on actually fired), while the unbraced
    # one's really did retire -- confirms the traced mechanism, not an
    # assumption.
    assert trunk_b not in f_b.dead
    assert trunk_u in f_u.dead

    vol_unbraced = rewire._total_volume_ml(f_u)
    vol_braced = rewire._total_volume_ml(f_b)
    assert vol_braced < vol_unbraced, (vol_braced, vol_unbraced)


def test_run_writes_brace_and_size_log_and_stays_deterministic(cfg, toed_slab_mesh):
    """brace_and_size is wired into run() (log key present) and run() stays
    deterministic across repeated calls on the same fixture -- identical
    volume_report and member count. Uses toed_slab_mesh, not table_mesh:
    table_mesh currently CRASHES run() (see
    test_table_and_toed_slab_full_run_not_worse_than_baseline's docstring
    for the root-caused reason) -- toed_slab_mesh is the fixture this
    determinism property is actually checkable on."""
    r1 = engine.run(toed_slab_mesh, config_mod.SupportConfig())
    r2 = engine.run(toed_slab_mesh, config_mod.SupportConfig())
    assert "brace_and_size" in r1["log"]
    assert r1["volume"] == r2["volume"]
    assert len(r1["members"]) == len(r2["members"])


# =================================================================
# SPEC-RUST-KERNEL WIRE (c): production-path determinism guard.
# engine.run now builds the Rust geom_kernel (KernelSurface) as its
# surface-EXACT oracle. Determinism is a HARD requirement (house rules):
# results must be BIT-IDENTICAL across repeated runs AND across thread
# counts. This guard runs the table fixture three ways -- twice at the
# default (all-cores) thread count and once pinned to a single kernel
# thread -- and asserts every load-bearing output is byte-for-byte equal.
# =================================================================
def _tobytes(a):
    a = np.ascontiguousarray(np.asarray(a, dtype=np.float64))
    return str(a.shape).encode() + a.tobytes()


def _run_canon(result):
    """Canonical byte image of a run() result's load-bearing, determinism-
    relevant outputs: the full volume accounting, every live member's
    endpoints/radii/kind, the joint balls, the routed/unrouted split,
    carried areas + tip radii, and every verify violation list. dtype +
    shape + tobytes() (NOT np.array_equal, which treats +0.0 == -0.0 --
    Amendment A19)."""
    parts = []
    vol = result["volume"]
    for k in sorted(vol):
        v = vol[k]
        if isinstance(v, dict):
            parts.append((k + ":" + repr(sorted(v.items()))).encode())
        else:
            parts.append((k + "=" + repr(float(v))).encode())
    for m in result["members"]:
        parts.append(_tobytes(m["p0"]))
        parts.append(_tobytes(m["p1"]))
        parts.append(repr((float(m["r0"]), float(m["r1"]), m["kind"],
                           tuple(m["owner"]), m.get("embed_end"),
                           float(m.get("carried_mm2", 0.0)))).encode())
    for b in result["balls"]:
        parts.append(_tobytes(b["center"]))
        parts.append(repr(float(b["r"])).encode())
    parts.append(repr(sorted(int(i) for i in result["unrouted"])).encode())
    parts.append(_tobytes(result["carried_area_mm2"]))
    parts.append(_tobytes(result["tip_r_mm"]))
    v = result["verify"]
    for key in sorted(v):
        if key.endswith("violations") or key.startswith("uncovered") or \
                key == "unsupported_minima":
            parts.append((key + "=" + repr(v[key])).encode())
    return b"|".join(parts)


@pytest.mark.kernel
def test_run_bit_identical_across_thread_counts_on_table(table_mesh):
    """WIRE guard (c): engine.run on the table fixture is bit-identical when
    repeated AND when the kernel's effective thread count is changed. The
    real knob for THIS kernel is GEOM_KERNEL_THREADS (spec A4: the kernel
    never touches rayon's GLOBAL pool -- it keys per-call ThreadPoolBuilder
    pools on GEOM_KERNEL_THREADS, so RAYON_NUM_THREADS is inert by design and
    a RAYON_NUM_THREADS sweep would pass VACUOUSLY without ever varying the
    count). We therefore sweep the real knob and, per A4's anti-vacuous rule,
    ASSERT the effective count actually changed before trusting the equality.
    """
    from support_placement import geometry_kernel as GK

    prev = os.environ.get("GEOM_KERNEL_THREADS")
    seen = []
    canons = []
    try:
        # default (all cores), default again (repeat), then single-thread.
        for env in (None, None, "1"):
            if env is None:
                os.environ.pop("GEOM_KERNEL_THREADS", None)
            else:
                os.environ["GEOM_KERNEL_THREADS"] = env
            seen.append(GK.KernelSurface(table_mesh).current_threads())
            result = engine.run(table_mesh, config_mod.SupportConfig())
            canons.append(_run_canon(result))
    finally:
        if prev is None:
            os.environ.pop("GEOM_KERNEL_THREADS", None)
        else:
            os.environ["GEOM_KERNEL_THREADS"] = prev

    # A4 anti-vacuous: the sweep MUST have actually varied the thread count,
    # otherwise thread-invariance is untested. Default is all cores (>1 on
    # this machine); the pinned case is exactly 1.
    assert seen[2] == 1, seen
    assert seen[0] > 1, ("default kernel thread count is not >1; cannot "
                         f"demonstrate thread-count variance: {seen}")
    # The load-bearing property: byte-identical results across all three runs.
    assert len(set(canons)) == 1, "engine.run results differ across runs/threads"


_BASELINE_PATH = os.path.join(os.path.dirname(__file__), "fixtures",
                              "v5_baseline.json")


def test_table_and_toed_slab_full_run_not_worse_than_baseline(cfg):
    """SPEC-INTEGRATION #4's acceptance gate + trap detector: table and
    toed_slab through the FULL new run() (construct -> rewire ->
    brace_and_size -> verify) must stay gate-clean and not exceed
    fixtures/v5_baseline.json's total_ml.

    v5 geometry-core re-freeze (SPEC-V5-GEOM sec 4, PRE-APPROVED): evicting
    the sampled surface point cloud from the load-bearing queries legitimately
    moves the frozen volumes. The exact swap that lands in SWAP STEP 3 changes
    total_ml two ways: (1) placement.load_partition became an EXACT,
    deterministic per-face area partition (sec 2.5) -- no more Monte-Carlo
    sub.sample -- which reassigns carried areas (hence tip radii, hence member
    volumes) off the honest face geometry rather than a seed-dependent point
    draw; (2) the perception interior-face filter (sec 2.4) drops phantom
    demand seeded on faces interior to the overlapping-assembly solid union.
    The GATE FORMULA itself (not-worse-than-frozen, +1e-6 tolerance) is
    UNCHANGED; only the frozen numbers moved, re-measured on the exact engine
    and confirmed bit-identical across two independent engine.run calls per
    fixture before freezing (scripts/tests/_refreeze_v5.py: the double-run
    determinism proof; old->new values are in the SWAP-STEP-3 commit message).
    A1 plate-grounding re-freeze (2026-07-25): the old table baseline omitted
    its lifted leg bottoms because grounded_mask compared against mesh z-min
    instead of the actual plate. Restoring that mandatory support demand
    necessarily raises volume and may add unrouted demand, so the table entry
    was re-measured twice after A1 and confirmed bit-identical. Toed-slab was
    unchanged. A2 virtual-analysis-face re-freeze (2026-07-25): replacing
    arbitrary surface samples/centroids with the coverage-capable dyadic
    lowest-vertex pool intentionally changes both fixtures' contact sets.
    `_refreeze_v5.py` again ran each fixture twice; volume reports were
    exactly equal, all hard gates including regular-contact provenance were
    clean, and only `v5_baseline.json` moved. The gate formula remains
    unchanged; fixtures/v4_p4_baseline.json
    (and v4_p3, v3) are kept untouched for history/provenance."""
    baseline = json.load(open(_BASELINE_PATH))

    for name, mesh_fn in (("table", mesh_io.make_overhang_table),
                           ("toed_slab", mesh_io.make_toed_slab)):
        mesh = mesh_fn()
        result = engine.run(mesh, config_mod.SupportConfig())
        v = result["verify"]
        assert v["ok"], (name, v)
        assert v["uncovered_islands"] == [], (name, v["uncovered_islands"])
        assert v["clearance_violations"] == [], (name, v["clearance_violations"])
        assert v["ball_breach_violations"] == [], (
            name, v["ball_breach_violations"])
        assert v["connectivity_violations"] == [], (name, v["connectivity_violations"])
        assert v["printability_violations"] == [], (name, v["printability_violations"])
        assert v["murray_violations"] == [], (name, v["murray_violations"])
        assert v["regular_contact_invariant_violations"] == [], (
            name, v["regular_contact_invariant_violations"])
        # v4 p6 (I3): HARD acceptance on these small fixtures -- the honest
        # island-birth gate, with the I2-demanded contacts actually
        # generated, routed, and covering.
        assert v["uncovered_island_births"] == [], (name, v["uncovered_island_births"])
        total_ml = result["volume"]["total_ml"]
        assert total_ml <= baseline[name]["total_ml"] + 1e-6, (
            f"{name}: total_ml {total_ml:.4f} exceeds frozen baseline "
            f"{baseline[name]['total_ml']} (not-worse trap detector)")
        # Coverage pin (SPEC-V5-GEOM sec 4.5 / adversarial review point 7):
        # the volume not-worse gate ALONE would silently accept lost routes
        # (fewer routed contacts = less resin = a "better" volume that is
        # actually worse support). Pin the unrouted-contact count so a
        # coverage regression from the exact-anchor pool change is caught,
        # not rewarded.
        n_unrouted = len(result["unrouted"])
        assert n_unrouted <= baseline[name]["n_unrouted"], (
            f"{name}: n_unrouted {n_unrouted} exceeds frozen coverage baseline "
            f"{baseline[name]['n_unrouted']} (silent route-loss detector)")
