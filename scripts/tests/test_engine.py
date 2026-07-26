import json
import os
import pickle
import dataclasses

import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import (
    config as config_mod, coverage, engine, mesh_io, phase8_driver, placement, rewire,
    topology,
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
    topology.validate_face_topology(
        reloaded["face_topology"], mesh=table_mesh)
    coverage.validate_coverage_samples(
        reloaded["coverage_samples"], mesh=table_mesh,
        islands=reloaded["islands"])

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


def _count_prepared_deep_validators(monkeypatch):
    counts = dict(candidate=0, topology=0, coverage=0)
    originals = (
        placement.validate_candidate_pool,
        topology.validate_face_topology,
        coverage.validate_coverage_samples,
    )

    def candidate(*args, **kwargs):
        counts["candidate"] += 1
        return originals[0](*args, **kwargs)

    def face_topology(*args, **kwargs):
        counts["topology"] += 1
        return originals[1](*args, **kwargs)

    def coverage_samples(*args, **kwargs):
        counts["coverage"] += 1
        return originals[2](*args, **kwargs)

    monkeypatch.setattr(placement, "validate_candidate_pool", candidate)
    monkeypatch.setattr(topology, "validate_face_topology", face_topology)
    monkeypatch.setattr(coverage, "validate_coverage_samples", coverage_samples)
    return counts


def test_external_staged_entries_deep_validate_prepared_authority_once(
        table_mesh, cfg, monkeypatch):
    pre = engine.prepare(table_mesh, cfg)
    counts = _count_prepared_deep_validators(monkeypatch)
    monkeypatch.setattr(
        engine.cco, "build", lambda *_args, **_kwargs: (tm.Forest(), []))

    engine.construct(table_mesh, pre, None, cfg)
    assert counts == dict(candidate=1, topology=1, coverage=1)

    counts.update(candidate=0, topology=0, coverage=0)
    monkeypatch.setattr(engine.verify, "self_verify",
                        lambda *_args, **_kwargs: {})
    engine.finish(
        table_mesh, pre, tm.Forest(), [], [], np.array([]), np.array([]),
        None, cfg)
    assert counts == dict(candidate=1, topology=1, coverage=1)

    counts.update(candidate=0, topology=0, coverage=0)
    result = engine.relocate(
        table_mesh, None, [], [], np.array([]), np.array([]), 0.0, cfg,
        prepared_state=pre)
    assert result["skipped"] == "no exact geom oracle"
    assert counts == dict(candidate=1, topology=1, coverage=1)


def test_finish_hard_gates_unrouted_specialized_from_final_graph(
        table_mesh, cfg, monkeypatch):
    pre = engine.prepare(table_mesh, cfg)
    contact = next(
        dict(row) for row in pre["contacts"]
        if row.get("minimum") or row.get("island_birth"))
    forest = tm.Forest()
    member_idx = forest.add_member(tm.Member(
        p0=np.asarray(contact["position"], dtype=np.float64),
        p1=np.asarray(contact["position"], dtype=np.float64)
        + np.array([0.0, 0.0, 1.0]),
        r0=0.2, r1=0.3, kind="stub", owner=("contact", 0),
        embed_end="p0"))
    forest.terminal_of[0] = member_idx
    monkeypatch.setattr(
        engine.tree, "assert_all_certified", lambda *_args: None)
    monkeypatch.setattr(
        engine.verify, "self_verify",
        lambda *_args, **_kwargs: {"ok": True})

    failed = engine.finish(
        table_mesh, pre, forest, [0], [contact],
        np.array([0.0]), np.array([0.0]), None, cfg)
    assert failed["verify"]["unrouted_specialized_contacts"] == [
        dict(contact_idx=0, island_id=int(contact["island_id"]))]
    assert failed["verify"]["ok"] is False

    clean = engine.finish(
        table_mesh, pre, forest, [], [contact],
        np.array([1.0]), np.array([0.2]), None, cfg)
    assert clean["verify"]["unrouted_specialized_contacts"] == []
    assert clean["verify"]["ok"] is True


def test_prepared_validation_token_is_run_local_and_rejects_replaced_authority(
        table_mesh, cfg, monkeypatch):
    import copy

    pre = engine.prepare(table_mesh, cfg)
    engine.validate_prepared_state(pre, table_mesh, cfg)
    token = engine.prepared_validation._issue_prepared_validation_token(
        pre, table_mesh, cfg)
    with pytest.raises(TypeError, match="run-local"):
        pickle.dumps(token, protocol=pickle.HIGHEST_PROTOCOL)

    counts = _count_prepared_deep_validators(monkeypatch)
    monkeypatch.setattr(
        engine.cco, "build", lambda *_args, **_kwargs: (tm.Forest(), []))

    # A copied prepared dict is valid, but the old dict-identity capability
    # cannot authorize it; the staged boundary therefore proves it afresh.
    copied = copy.copy(pre)
    engine.construct(
        table_mesh, copied, None, cfg,
        _prepared_validation_token=token)
    assert counts == dict(candidate=1, topology=1, coverage=1)

    # Replacing a sidecar on the original dict also invalidates the old token.
    replacement_topology = copy.deepcopy(pre["face_topology"])
    counts.update(candidate=0, topology=0, coverage=0)
    pre["face_topology"] = replacement_topology
    engine.construct(
        table_mesh, pre, None, cfg,
        _prepared_validation_token=token)
    assert counts == dict(candidate=1, topology=1, coverage=1)


def test_unpickled_corrupt_prepared_state_without_token_is_rejected(
        table_mesh, cfg, monkeypatch):
    pre = pickle.loads(pickle.dumps(
        engine.prepare(table_mesh, cfg),
        protocol=pickle.HIGHEST_PROTOCOL))
    object.__setattr__(
        pre["coverage_samples"], "content_digest", "0" * 64)
    counts = _count_prepared_deep_validators(monkeypatch)

    with pytest.raises(ValueError, match="content digest mismatch"):
        engine.construct(table_mesh, pre, None, cfg)

    assert counts == dict(candidate=1, topology=1, coverage=1)


def test_staged_consumers_reject_old_prepared_state(table_mesh, cfg):
    with pytest.raises(ValueError, match="rerun prepare"):
        engine.construct(table_mesh, {}, None, cfg)
    with pytest.raises(ValueError, match="rerun prepare"):
        engine.finish(
            table_mesh, {}, None, [], [], np.array([]), np.array([]),
            None, cfg)


def test_a4_ranking_uses_marginal_area_lower_row_tie_and_exact_ledger(cfg):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    islands = [np.array([0], dtype=np.int64)]
    pool = placement.candidate_pool(mesh, islands, cfg)
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    assessments = []
    coverage_rows = verify.coverage_report(
        mesh, islands, [], cfg, coverage_samples=samples,
        face_topology=sidecar, assessment_out=assessments)
    report = dict(
        uncovered_islands=[row for row in coverage_rows if not row["covered"]])
    pre = dict(
        contact_candidate_pool=pool, coverage_samples=samples,
        face_topology=sidecar)

    stats = {}
    first = engine._rank_coverage_repair_rows(
        pre, assessments[0], occupied_rows=set(), failed_rows=set(), cfg=cfg,
        stats_out=stats)
    assert first == 0
    assert stats["n_eligible_rows"] == len(pool)

    # Occupancy and failure are exact row identities, never an island-wide or
    # spatial-radius veto. The next legal row remains available.
    stats = {}
    second = engine._rank_coverage_repair_rows(
        pre, assessments[0], occupied_rows={0}, failed_rows=set(), cfg=cfg,
        stats_out=stats)
    assert second == 1
    assert stats["n_eligible_rows"] == len(pool) - 1
    assert engine._rank_coverage_repair_rows(
        pre, assessments[0], occupied_rows=set(), failed_rows={0}, cfg=cfg) == 1


def test_run_multi_attempt_a4_deep_validates_once_and_threads_phase8_token(
        table_mesh, cfg, monkeypatch):
    from types import SimpleNamespace

    local_cfg = dataclasses.replace(
        cfg, coverage_repair_max_attempts=2, p8_relocate_enable=True)
    pre = engine.prepare(table_mesh, local_cfg)
    # Keep one valid demand so the real A3/A4 loop must add multiple prepared
    # rows to address the other named islands.
    pre["contacts"] = [dict(pre["contacts"][0])]
    counts = _count_prepared_deep_validators(monkeypatch)
    original_self_verify = verify.self_verify
    verify_calls = 0
    phase8_contexts = 0

    monkeypatch.setattr(engine, "prepare", lambda *_args, **_kwargs: pre)
    monkeypatch.setattr(
        engine, "KernelSurface",
        lambda _mesh: SimpleNamespace(penetration_intervals=lambda *_a: ()))
    monkeypatch.setattr(
        engine.routing, "build_surface_tree",
        lambda *_args, **_kwargs: None)
    monkeypatch.setattr(
        engine.cco, "build", lambda *_args, **_kwargs: (tm.Forest(), []))
    monkeypatch.setattr(
        engine.rewire, "optimize", lambda *_args, **_kwargs: None)
    monkeypatch.setattr(
        engine, "brace_and_size", lambda *_args, **_kwargs: {})
    monkeypatch.setattr(
        engine, "live_member_lists",
        lambda *_args, **_kwargs: ([dict(
            p0=np.zeros(3), p1=np.array([0.0, 0.0, 1.0]),
            r0=0.2, r1=0.2, kind="stub")], []))
    monkeypatch.setattr(
        engine.tree, "volume_report",
        lambda *_args, **_kwargs: dict(member_ml=0.0, ball_ml=0.0))
    monkeypatch.setattr(
        engine.transient, "resize_forest",
        lambda *_args, **_kwargs: {"resized": []})

    def token_aware_verify(
            mesh, islands, contacts, _members, carried, tip_r, run_cfg,
            plate_z, **kwargs):
        nonlocal verify_calls
        verify_calls += 1
        # Exercise the real coverage and A2 provenance validators while
        # discarding the synthetic member used only to enter Phase 8.
        kwargs["forest"] = tm.Forest()
        return original_self_verify(
            mesh, islands, contacts, [], carried, tip_r, run_cfg, plate_z,
            **kwargs)

    monkeypatch.setattr(engine.verify, "self_verify", token_aware_verify)

    def token_aware_phase8(*args, **kwargs):
        nonlocal phase8_contexts
        phase8_contexts += 1
        phase8_driver.SurfaceSeedIndex(
            args[9], islands=kwargs["islands"],
            island_births=kwargs["island_births"],
            candidate_pool_prepared=kwargs["candidate_pool_prepared"],
            coverage_samples=kwargs["coverage_samples"],
            face_topology=kwargs["face_topology"], cfg=args[8],
            _prepared_validation_token=kwargs[
                "_prepared_validation_token"])
        return phase8_driver.Phase8Report(
            snapshot_summary={}, n_stub_sites=0, n_strut_sites=0,
            n_ball_sites=0, stub_resolved=0, stub_unresolved=0,
            qreal_resolved=0, qreal_unresolved=0,
            strut_resolved=0, strut_unresolved=0,
            ball_resolved=0, ball_unresolved=0,
            reverted_splices=(), unresolved_items=(), pocket_labels=(),
            dt_s=0.0, entered_ladder=False, sweeps=0, converged=True,
            rounds=1, committed_by_round=(0,), committed_by_sweep=(0,))

    monkeypatch.setattr(
        engine.phase8_driver, "run_phase8", token_aware_phase8)
    result = engine.run(table_mesh, local_cfg)

    assert result["log"]["coverage_repair"]["attempts"] == 2
    assert verify_calls == 4
    assert phase8_contexts == 1
    assert counts == dict(candidate=1, topology=1, coverage=1)


def test_a4_scoring_tiles_both_axes_and_matches_full_matrix(cfg):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [12.0, 0.0, 0.0], [0.0, 12.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    islands = [np.array([0], dtype=np.int64)]
    local_cfg = dataclasses.replace(
        cfg, coverage_score_matrix_max_entries=9)
    pool = placement.candidate_pool(mesh, islands, local_cfg)
    samples = coverage.build_coverage_samples(mesh, islands, local_cfg)
    sidecar = topology.build_face_topology(mesh, cfg=local_cfg)
    assessments = []
    verify.coverage_report(
        mesh, islands, [], local_cfg, coverage_samples=samples,
        face_topology=sidecar, assessment_out=assessments)
    pre = dict(
        contact_candidate_pool=pool, coverage_samples=samples,
        face_topology=sidecar)
    stats = {}
    selected = engine._rank_coverage_repair_rows(
        pre, assessments[0], occupied_rows=set(), failed_rows=set(),
        cfg=local_cfg, stats_out=stats)

    distance_sq = np.sum(
        (pool.positions[:, None, :] - samples.positions[None, :, :]) ** 2,
        axis=2)
    radius = local_cfg.target_spacing_mm * local_cfg.coverage_radius_factor
    score = (distance_sq <= radius * radius) @ samples.area_weights_mm2
    expected = int(np.flatnonzero(score == np.max(score))[0])
    assert selected == expected
    assert len(pool) > stats["max_candidate_tile_rows"]
    assert len(samples) > stats["max_sample_tile_rows"]
    assert stats["max_score_matrix_entries"] <= 9


def test_a4_objective_and_noncoverage_guard_are_pinned():
    old = [dict(
        failing_uncovered_area_mm2=0.4, max_patch_area_mm2=0.3,
        n_failing_patches=2, worst_gap_mm=8.0)]
    better = [dict(
        failing_uncovered_area_mm2=0.3, max_patch_area_mm2=0.29,
        n_failing_patches=3, worst_gap_mm=9.0)]
    assert engine._coverage_objective(better) < engine._coverage_objective(old)
    report = {key: [] for key in engine._NONCOVERAGE_GATE_KEYS}
    assert engine._noncoverage_hard_counts(report) == (
        0,) * len(engine._NONCOVERAGE_GATE_KEYS)
    report["clearance_violations"] = [{}, {}]
    assert engine._noncoverage_hard_counts(report)[0] == 2

    better_report = {key: [] for key in engine._NONCOVERAGE_GATE_KEYS}
    better_report["uncovered_islands"] = better
    old_report = {key: [] for key in engine._NONCOVERAGE_GATE_KEYS}
    old_report["uncovered_islands"] = old
    assert engine._coverage_repair_trial_verdict(
        old_report, better_report)[:2] == (True, "accepted")

    same_report = dict(old_report)
    assert engine._coverage_repair_trial_verdict(
        old_report, same_report)[:2] == (
            False, "coverage-not-improved")

    regressed = dict(better_report)
    regressed["clearance_violations"] = [{}]
    assert engine._coverage_repair_trial_verdict(
        old_report, regressed)[:2] == (
            False, "noncoverage-hard-gate-regression")


def test_a4_full_trial_rolls_back_reranks_and_accepts_one_winner(
        cfg, monkeypatch):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [12.0, 0.0, 0.0], [0.0, 12.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    islands = [np.array([0], dtype=np.int64)]
    pool = placement.candidate_pool(mesh, islands, cfg)
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    initial = engine._regular_contact_from_pool_row(pool, 0, islands, mesh)
    pre = dict(
        schema=engine.PREPARED_STATE_SCHEMA,
        version=engine.PREPARED_STATE_SCHEMA_VERSION,
        mesh_fingerprint=engine.mesh_fingerprint(mesh),
        config_fingerprint=engine.config_fingerprint(cfg),
        islands=islands, minima=[], island_births=[], contacts=[initial],
        plate_z=-3.0, sample_points=np.empty((0, 3)),
        sample_delicacy=np.empty(0), contact_candidate_pool=pool,
        coverage_samples=samples, face_topology=sidecar, log={})
    base_assessments = []
    base_coverage = verify.coverage_report(
        mesh, islands, [initial], cfg, coverage_samples=samples,
        face_topology=sidecar, assessment_out=base_assessments)
    base_uncovered = [r for r in base_coverage if not r["covered"]]
    assert base_uncovered

    monkeypatch.setattr(engine, "KernelSurface", lambda _mesh: object())
    monkeypatch.setattr(engine, "prepare", lambda *_args, **_kwargs: pre)
    monkeypatch.setattr(
        engine.routing, "build_surface_tree", lambda *_args, **_kwargs: object())
    monkeypatch.setattr(engine.rewire, "optimize", lambda *_args, **_kwargs: None)
    monkeypatch.setattr(engine, "brace_and_size", lambda *_args, **_kwargs: None)
    monkeypatch.setattr(engine.tree, "assert_all_certified", lambda *_args: None)
    monkeypatch.setattr(engine, "live_member_lists", lambda *_args: ([], []))
    monkeypatch.setattr(
        engine.tree, "volume_report",
        lambda *_args: dict(
            member_ml=0.0, ball_ml=0.0, total_ml=0.0, by_kind_ml={}))

    constructed_rows = []

    def fake_construct(_mesh, prepared, *_args, **_kwargs):
        active_contacts = _kwargs.get(
            "contacts_override", prepared["contacts"])
        rows = tuple(
            int(c["candidate_pool_row"]) for c in active_contacts)
        constructed_rows.append(rows)
        n = len(active_contacts)
        return object(), [], np.ones(n), np.ones(n), list(active_contacts)

    monkeypatch.setattr(engine, "construct", fake_construct)
    verify_calls = 0

    def blank_report(uncovered, clearance=()):
        report = {key: [] for key in engine._NONCOVERAGE_GATE_KEYS}
        report.update(
            ok=not uncovered and not clearance,
            uncovered_islands=uncovered,
            clearance_violations=list(clearance),
            moot_transit_members=[], removal_violations=[],
            removal_report=None, endcap_breach_diagnostic=[],
            transit_chords_mm={})
        return report

    def fake_verify(*_args, **_kwargs):
        nonlocal verify_calls
        verify_calls += 1
        _kwargs["coverage_assessment_out"].append(base_assessments[0])
        if verify_calls == 1:
            return blank_report(base_uncovered)
        if verify_calls == 2:
            trial = [dict(base_uncovered[0])]
            trial[0]["failing_uncovered_area_mm2"] *= 0.5
            return blank_report(trial, clearance=[dict(member_idx=0)])
        return blank_report([])

    monkeypatch.setattr(engine.verify, "self_verify", fake_verify)
    local_cfg = dataclasses.replace(
        cfg, p8_relocate_enable=False, coverage_repair_max_attempts=2)
    # prepare is a test authority; align its semantic fingerprint to local cfg.
    pre["config_fingerprint"] = engine.config_fingerprint(local_cfg)
    result = engine.run(mesh, local_cfg)
    repair = result["log"]["coverage_repair"]
    assert len(constructed_rows) == 3
    assert constructed_rows[0] == (0,)
    first_trial_row = constructed_rows[1][-1]
    second_trial_row = constructed_rows[2][-1]
    assert first_trial_row != second_trial_row
    assert tuple(
        int(c["candidate_pool_row"]) for c in result["contacts"]) == (
            0, second_trial_row)
    assert repair["attempts"] == 2
    assert repair["termination"] == "covered"
    assert repair["ledger"][-2]["reason"] == (
        "noncoverage-hard-gate-regression")
    assert repair["ledger"][-1]["reason"] == "accepted"
    assert first_trial_row in repair["failed_candidate_pool_rows"]

    # The same first rejection with a one-attempt budget restores the baseline
    # and terminates honestly at the workload cap.
    constructed_rows.clear()
    verify_calls = 0
    pre["contacts"] = [initial]
    capped_cfg = dataclasses.replace(
        cfg, p8_relocate_enable=False, coverage_repair_max_attempts=1)
    pre["config_fingerprint"] = engine.config_fingerprint(capped_cfg)
    capped = engine.run(mesh, capped_cfg)
    capped_repair = capped["log"]["coverage_repair"]
    assert len(constructed_rows) == 2
    assert tuple(
        int(c["candidate_pool_row"]) for c in capped["contacts"]) == (0,)
    assert capped_repair["attempts"] == 1
    assert capped_repair["termination"] == "attempt-cap"
    assert capped_repair["ledger"][-1]["reason"] == (
        "noncoverage-hard-gate-regression")
    assert capped["verify"]["uncovered_islands"]


@pytest.mark.parametrize("p8_enabled", [False, True])
@pytest.mark.parametrize("mixed_specialized", [False, True])
def test_all_unrouted_normalizes_to_canonical_empty_geometry_without_mutating_prepared(
        cfg, monkeypatch, p8_enabled, mixed_specialized):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    islands = [np.array([0], dtype=np.int64)]
    pool = placement.candidate_pool(mesh, islands, cfg)
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    regular = engine._regular_contact_from_pool_row(
        pool, 0, islands, mesh)
    contacts = [regular]
    if mixed_specialized:
        contacts.append(dict(
            position=np.asarray(mesh.triangles_center[0]).copy(),
            normal=np.asarray(mesh.face_normals[0]).copy(),
            island_id=0, minimum=True, coverage_face_id=0,
            region_pts=np.asarray(mesh.triangles_center).copy(),
            area_mm2=float(mesh.area)))
    local_cfg = dataclasses.replace(
        cfg, p8_relocate_enable=p8_enabled)
    pre = dict(
        schema=engine.PREPARED_STATE_SCHEMA,
        version=engine.PREPARED_STATE_SCHEMA_VERSION,
        mesh_fingerprint=engine.mesh_fingerprint(mesh),
        config_fingerprint=engine.config_fingerprint(local_cfg),
        islands=islands, minima=[], island_births=[],
        contacts=contacts, plate_z=-3.0,
        sample_points=np.empty((0, 3)),
        sample_delicacy=np.empty(0),
        contact_candidate_pool=pool, coverage_samples=samples,
        face_topology=sidecar, log={})
    prepared_contacts = pre["contacts"]
    prepared_contact_ids = tuple(map(id, prepared_contacts))
    active_history = []

    def fake_construct(_mesh, _pre, *_args, **kwargs):
        active = list(kwargs["contacts_override"])
        active_history.append(tuple(
            "specialized" if c.get("minimum") else "regular"
            for c in active))
        n = len(active)
        return (
            tm.Forest(), list(range(n)), np.ones(n), np.full(n, 0.2),
            active)

    monkeypatch.setattr(engine, "KernelSurface", lambda _mesh: object())
    monkeypatch.setattr(engine, "prepare", lambda *_args, **_kwargs: pre)
    monkeypatch.setattr(
        engine.routing, "build_surface_tree",
        lambda *_args, **_kwargs: object())
    monkeypatch.setattr(engine, "construct", fake_construct)
    monkeypatch.setattr(
        engine, "relocate",
        lambda *_args, **_kwargs: pytest.fail(
            "P8 must not run for canonical empty geometry"))

    result = engine.run(mesh, local_cfg)
    assert result["members"] == [] and result["balls"] == []
    assert result["forest"].members == []
    assert result["volume"]["total_ml"] == 0.0
    assert result["verify"]["ok"] is False
    assert result["verify"]["uncovered_islands"]
    assert pre["contacts"] is prepared_contacts
    assert tuple(map(id, pre["contacts"])) == prepared_contact_ids
    assert "coverage_failed_candidate_pool_rows" not in pre
    if mixed_specialized:
        assert active_history == [
            ("regular", "specialized"), ("specialized",)]
        assert len(result["contacts"]) == 1
        assert result["contacts"][0]["minimum"] is True
        assert result["unrouted"] == [0]
        assert result["carried_area_mm2"].tolist() == [0.0]
        assert result["tip_r_mm"].tolist() == [0.0]
        assert result["verify"]["unrouted_specialized_contacts"] == [
            dict(contact_idx=0, island_id=0)]
    else:
        assert active_history == [("regular",)]
        assert result["contacts"] == []
        assert result["unrouted"] == []
        assert result["carried_area_mm2"].size == 0
        assert result["tip_r_mm"].size == 0


def test_specialized_contact_face_provenance_uses_exact_triangle_not_centroid(
        cfg):
    import trimesh
    triangles = np.array([
        [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]],
        [[0.9, 0.9, 0.1], [1.1, 0.9, 0.1], [0.9, 1.1, 0.1]],
    ])
    mesh = trimesh.Trimesh(
        vertices=triangles.reshape((-1, 3)),
        faces=np.arange(6, dtype=np.int64).reshape((-1, 3)),
        process=False)
    islands = [np.array([0, 1], dtype=np.int64)]
    point = np.array([1.0, 1.0, 0.0])
    assert np.linalg.norm(
        mesh.triangles_center[1] - point) < np.linalg.norm(
            mesh.triangles_center[0] - point)
    assert engine._exact_specialized_face_id(
        mesh, point, np.array([0, 1])) == 0

    minima = [dict(
        position=point.copy(), normal=np.array([0.0, 0.0, 1.0]),
        region_pts=point[None, :])]
    minima_contacts, _islands, n_added = engine._ensure_minima_contacts(
        mesh, islands, [], minima, cfg)
    assert n_added == 1
    assert minima_contacts[0]["coverage_face_id"] == 0

    birth = dict(
        x=1.0, y=1.0, z=0.0, area_mm2=1.0,
        loop=np.array([
            [0.9, 0.9], [1.1, 0.9], [1.1, 1.1], [0.9, 1.1],
        ]))
    birth_contacts, _islands, n_added = (
        engine._ensure_island_birth_contacts(
            mesh, islands, [], [birth], cfg))
    assert n_added == 1
    assert birth_contacts[0]["coverage_face_id"] == 0


def _nearby_disconnected_face_mesh():
    """Two edge-connected faces beside a disconnected face within 1.5mm."""
    import trimesh
    return trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [0.6, 0.0, 0.0],
            [0.0, 0.6, 0.0],
            [0.6, 0.6, 0.0],
            [0.0, 0.0, 0.2],
            [0.6, 0.0, 0.2],
            [0.0, 0.6, 0.2],
        ], dtype=np.float64),
        faces=np.array([
            [0, 1, 2],
            [1, 3, 2],
            [4, 5, 6],
        ], dtype=np.int64),
        process=False)


def test_minima_synthetic_islands_clip_local_faces_to_owning_component(cfg):
    mesh = _nearby_disconnected_face_mesh()
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    assert tuple(sidecar.component_ids) == (0, 0, 1)
    centers = np.asarray(mesh.triangles_center, dtype=np.float64)
    minima = [
        dict(
            position=centers[face_id].copy(),
            normal=mesh.face_normals[face_id].copy(),
            region_pts=centers[face_id][None, :])
        for face_id in (0, 1, 2)
    ]

    contacts, islands, added = engine._ensure_minima_contacts(
        mesh, [], [], minima, cfg, face_topology=sidecar)
    repeated = engine._ensure_minima_contacts(
        mesh, [], [], minima, cfg, face_topology=sidecar)

    assert added == 3
    assert [island.tolist() for island in islands] == [[0, 1], [2]]
    assert [contact["island_id"] for contact in contacts] == [0, 0, 1]
    assert [contact["coverage_face_id"] for contact in contacts] == [0, 1, 2]
    assert [island.tolist() for island in repeated[1]] == [
        island.tolist() for island in islands]
    assert [
        (contact["island_id"], contact["coverage_face_id"])
        for contact in repeated[0]
    ] == [
        (contact["island_id"], contact["coverage_face_id"])
        for contact in contacts
    ]
    for island_id, island in enumerate(islands):
        components = np.unique(sidecar.component_ids[island])
        assert components.tolist() == [island_id]
        assert all(
            int(sidecar.component_ids[contact["coverage_face_id"]])
            == int(components[0])
            for contact in contacts
            if contact["island_id"] == island_id)


def test_birth_synthetic_island_clips_local_faces_to_owning_component(cfg):
    mesh = _nearby_disconnected_face_mesh()
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    point = np.asarray(mesh.triangles_center[0], dtype=np.float64)
    birth = dict(
        x=float(point[0]), y=float(point[1]), z=float(point[2]),
        area_mm2=0.1,
        loop=np.array([
            [0.1, 0.1], [0.3, 0.1], [0.3, 0.3], [0.1, 0.3],
        ], dtype=np.float64))

    contacts, islands, added = engine._ensure_island_birth_contacts(
        mesh, [], [], [birth], cfg, face_topology=sidecar)

    assert added == 1
    assert [island.tolist() for island in islands] == [[0, 1]]
    assert contacts[0]["island_id"] == 0
    assert contacts[0]["coverage_face_id"] == 0
    assert np.unique(sidecar.component_ids[islands[0]]).tolist() == [0]


def test_prepare_reuses_one_topology_build_for_specialized_contact_passes(
        box_mesh, cfg, monkeypatch):
    original = topology.build_face_topology
    calls = []

    def counted(*args, **kwargs):
        calls.append(True)
        return original(*args, **kwargs)

    monkeypatch.setattr(topology, "build_face_topology", counted)
    prepared = engine.prepare(box_mesh, cfg)

    assert calls == [True]
    assert prepared["face_topology"].n_faces == len(box_mesh.faces)


def test_minima_contacts_preserve_each_named_owner_of_a_shared_face(cfg):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.0, 0.2, 0.0],
        ]),
        faces=np.array([[0, 1, 2]], dtype=np.int64),
        process=False)
    islands = [
        np.array([0], dtype=np.int64),
        np.array([0], dtype=np.int64),
    ]
    point = np.asarray(mesh.triangles_center[0], dtype=np.float64)
    minimum = dict(
        position=point.copy(), normal=mesh.face_normals[0].copy(),
        region_pts=point[None, :])

    contacts, returned_islands, added = engine._ensure_minima_contacts(
        mesh, islands, [], [minimum], cfg)

    assert added == 2
    assert returned_islands == islands
    assert [contact["island_id"] for contact in contacts] == [0, 1]
    assert [contact["coverage_face_id"] for contact in contacts] == [0, 0]

    existing = [dict(contacts[0])]
    contacts, returned_islands, added = engine._ensure_minima_contacts(
        mesh, islands, existing, [minimum], cfg)
    assert added == 1
    assert returned_islands == islands
    assert [contact["island_id"] for contact in contacts] == [0, 1]


def test_birth_contacts_preserve_each_named_owner_of_a_shared_face(cfg):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.0, 0.2, 0.0],
        ]),
        faces=np.array([[0, 1, 2]], dtype=np.int64),
        process=False)
    islands = [
        np.array([0], dtype=np.int64),
        np.array([0], dtype=np.int64),
    ]
    point = np.asarray(mesh.triangles_center[0], dtype=np.float64)
    birth = dict(
        x=float(point[0]), y=float(point[1]), z=float(point[2]),
        area_mm2=float(mesh.area),
        loop=np.array([
            [0.0, 0.0], [0.2, 0.0], [0.2, 0.2], [0.0, 0.2],
        ]))

    contacts, returned_islands, added = (
        engine._ensure_island_birth_contacts(
            mesh, islands, [], [birth], cfg))

    assert added == 2
    assert returned_islands == islands
    assert [contact["island_id"] for contact in contacts] == [0, 1]
    assert [contact["coverage_face_id"] for contact in contacts] == [0, 0]

    existing = [dict(contacts[0])]
    contacts, returned_islands, added = (
        engine._ensure_island_birth_contacts(
            mesh, islands, existing, [birth], cfg))
    assert added == 1
    assert returned_islands == islands
    assert [contact["island_id"] for contact in contacts] == [0, 1]


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
    changed_mask_cap = dataclasses.replace(
        cfg, synthetic_coverage_mask_max_words=(
            cfg.synthetic_coverage_mask_max_words - 1))
    assert engine.config_fingerprint(changed_mask_cap) != pre[
        "config_fingerprint"]
    with pytest.raises(ValueError, match="config fingerprint mismatch"):
        engine.validate_prepared_state(pre, table_mesh, changed_mask_cap)


def test_synthetic_mask_cap_exact_boundary_matches_dense_oracle():
    candidates = np.asarray([
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
    ])
    samples = np.column_stack((
        np.linspace(0.0, 10.0, 65),
        np.zeros(65),
        np.zeros(65),
    ))
    stats = {}
    masks = engine._synthetic_candidate_masks(
        candidates, samples, 2.0, 8, 4, stats)

    dense = np.linalg.norm(
        candidates[:, None, :] - samples[None, :, :], axis=2) <= 2.0
    expected = [
        sum(int(covered) << sample_index
            for sample_index, covered in enumerate(row))
        for row in dense
    ]
    assert masks == expected
    assert stats["requested_mask_words"] == 4
    assert stats["configured_mask_word_limit"] == 4
    assert stats["mask_preflight_passed"] is True
    assert stats["max_matrix_entries"] <= 8


def test_synthetic_mask_cap_refuses_before_pack_or_mask_allocation(monkeypatch):
    candidates = np.zeros((217, 3), dtype=np.float64)
    samples = np.zeros((21_778, 3), dtype=np.float64)
    allocation_calls = []

    def refuse_mask_allocation(_candidate_count):
        allocation_calls.append(True)
        raise AssertionError("mask allocation crossed failed preflight")

    def refuse_packbits(*_args, **_kwargs):
        raise AssertionError("packbits crossed failed preflight")

    monkeypatch.setattr(
        engine, "_synthetic_allocate_masks", refuse_mask_allocation)
    monkeypatch.setattr(engine.np, "packbits", refuse_packbits)
    stats = {}
    masks = engine._synthetic_candidate_masks(
        candidates, samples, 1.0, 65_536, 73_996, stats)

    assert masks is None
    assert allocation_calls == []
    assert stats["candidate_count"] == 217
    assert stats["needed_sample_count"] == 21_778
    assert stats["requested_mask_words"] == 73_997
    assert stats["configured_mask_word_limit"] == 73_996
    assert stats["mask_preflight_passed"] is False
    assert stats["mask_refusal_reason"] == "requested-mask-words"


def test_toed_mask_cap_73996_refuses_without_partial_contacts(
        toed_slab_mesh, cfg, monkeypatch):
    capped_cfg = dataclasses.replace(
        cfg, synthetic_coverage_mask_max_words=73_996)
    original_allocate = engine._synthetic_allocate_masks
    allocation_counts = []

    def observe_mask_allocation(candidate_count):
        allocation_counts.append(int(candidate_count))
        if int(candidate_count) == 217:
            raise AssertionError("large mask allocation crossed preflight")
        return original_allocate(candidate_count)

    def refuse_partial_contact(*_args, **_kwargs):
        raise AssertionError("selector emitted a partial regular contact")

    monkeypatch.setattr(
        engine, "_synthetic_allocate_masks", observe_mask_allocation)
    monkeypatch.setattr(
        engine, "_regular_contact_from_pool_row", refuse_partial_contact)
    log = {}
    with pytest.raises(
            RuntimeError, match="packed-mask resource bound"):
        engine.prepare(toed_slab_mesh, capped_cfg, log=log)

    selection = log["synthetic_regular_coverage_selection"]
    refused = max(
        selection["components"],
        key=lambda component: component.get("requested_mask_words", 0))
    assert allocation_counts == [4]
    assert selection["selected_count"] == 0
    assert selection["selected_rows"] == ()
    assert refused["requested_mask_words"] == 73_997
    assert refused["configured_mask_word_limit"] == 73_996
    assert refused["candidate_count"] == 217
    assert refused["needed_sample_count"] == 21_778
    assert refused["mask_preflight_passed"] is False
    assert refused["mask_refusal_reason"] == "requested-mask-words"


def test_synthetic_mask_cap_adversarial_refusal_is_deterministic(monkeypatch):
    candidates = np.zeros((513, 3), dtype=np.float64)
    samples = np.zeros((16_384, 3), dtype=np.float64)

    def refuse_mask_allocation(_candidate_count):
        raise AssertionError("mask allocation crossed failed preflight")

    monkeypatch.setattr(
        engine, "_synthetic_allocate_masks", refuse_mask_allocation)
    outcomes = []
    for _ in range(2):
        stats = {}
        masks = engine._synthetic_candidate_masks(
            candidates, samples, 1.0, 65_536, 131_072, stats)
        outcomes.append((masks, stats))

    assert outcomes[0] == outcomes[1]
    assert outcomes[0][0] is None
    assert outcomes[0][1]["requested_mask_words"] == 131_328
    assert outcomes[0][1]["mask_refusal_reason"] == "requested-mask-words"


def test_synthetic_higher_k_batches_bound_rows_times_cardinality():
    cardinality = 3
    matrix_budget = 10
    masks = [1] * 6 + [2] * 6 + [4] * 6
    positions = np.column_stack((
        np.arange(len(masks), dtype=np.float64) * 10.0,
        np.zeros(len(masks)),
        np.zeros(len(masks)),
    ))
    rows = np.arange(len(masks), dtype=np.int64)
    batch_shapes = []

    def observe_batch(index_sets):
        batch_shapes.append((len(index_sets), len(index_sets[0])))
        return None, None

    selected, key, _nodes, complete = engine._synthetic_exact_k_cover(
        masks, positions, rows, 0b111, np.empty((0, 3)),
        1.0, cardinality, 100_000, observe_batch, matrix_budget)

    assert selected is None
    assert key is None
    assert complete is True
    assert len(batch_shapes) > 1
    assert max(batch_count * k for batch_count, k in batch_shapes) <= (
        matrix_budget)


def test_synthetic_exact_pair_survives_greedy_spacing_trap(cfg, monkeypatch):
    """A highest-gain greedy first seat must not preclude the exact pair rung."""
    from types import SimpleNamespace

    pool = SimpleNamespace(positions=np.array([
        [0.0, 0.0, 0.0],
        [-0.75, 0.0, 0.0],
        [0.75, 0.0, 0.0],
        [-0.75, 0.0, 0.0],
    ], dtype=np.float64))
    samples = SimpleNamespace(positions=np.array([
        [-0.75, 0.0, 0.0],
        [-0.75, 0.0, 0.0],
        [0.75, 0.0, 0.0],
        [0.75, 0.0, 0.0],
    ], dtype=np.float64))
    # Row 0 has the unique highest gain (three bits) but is within the
    # 1 mm spacing floor of row 2, the only remaining-gain seat. Rows 1+2
    # and 3+2 are compatible exact pairs with identical geometry; the
    # existing lower-row tuple tie break must choose 1+2.
    monkeypatch.setattr(
        engine, "_synthetic_candidate_masks",
        lambda *_args, **_kwargs: [0b0111, 0b0011, 0b1100, 0b0011])
    stats = {}

    selected, _metrics, cardinality, exact = (
        engine._select_synthetic_component_rows(
            np.array([0, 1, 2, 3], dtype=np.int64),
            np.arange(4, dtype=np.int64),
            np.empty((0, 3), dtype=np.float64),
            np.empty((0, 3), dtype=np.float64),
            pool, samples,
            dataclasses.replace(cfg, target_spacing_mm=1.0),
            stats))

    assert selected == (1, 2)
    assert cardinality == 2
    assert exact is True
    assert stats["searched_cardinality"] == 2
    assert stats["pairs_examined"] == 6


def test_toed_prepare_mask_cap_metrics_and_determinism(toed_slab_mesh, cfg):
    first = engine.prepare(toed_slab_mesh, cfg)
    second = engine.prepare(toed_slab_mesh, cfg)
    first_selection = first["log"]["synthetic_regular_coverage_selection"]
    second_selection = second["log"]["synthetic_regular_coverage_selection"]

    assert first_selection == second_selection
    assert first_selection["requested_mask_words"] == 73_997
    assert first_selection["configured_mask_word_limit"] == 131_072
    assert first_selection["candidate_count"] == 217
    assert first_selection["needed_sample_count"] == 21_778
    assert first_selection["mask_preflight_passed"] is True
    assert first_selection["max_matrix_entries"] <= 65_536
    assert first_selection["selector_max_peak_bytes"] == 512 * 1024 * 1024
    assert first_selection["planned_selector_peak_bytes"] <= (
        first_selection["selector_max_peak_bytes"])
    assert first_selection["estimated_retained_mask_bytes"] > (
        first_selection["requested_mask_words"] * 8)
    assert all(
        component["max_matrix_entries"] <= 65_536
        for component in first_selection["components"])


def test_prepare_seeds_synthetic_minima_islands_with_exact_minimum_cover(
        table_mesh, cfg):
    pre = engine.prepare(table_mesh, cfg)
    repeat = engine.prepare(table_mesh, cfg)
    n_original_islands = pre["log"]["islands"]
    synthetic_ids = set(range(n_original_islands, len(pre["islands"])))
    assert synthetic_ids

    # Existing/original named islands retain ordinary placement's exact rows.
    expected_regular = placement.sample_candidates(
        table_mesh, pre["islands"], cfg,
        candidate_pool_prepared=pre["contact_candidate_pool"])
    expected_original_rows = [
        contact["candidate_pool_row"] for contact in expected_regular
        if contact["island_id"] < n_original_islands
    ]
    prepared_original_rows = [
        contact["candidate_pool_row"] for contact in pre["contacts"]
        if contact["island_id"] < n_original_islands
        and not contact.get("minimum") and not contact.get("island_birth")
    ]
    assert prepared_original_rows == expected_original_rows

    # Specialized minima/birth contacts are not moved, replaced, or decorated
    # by the regular selector, and the whole selection is deterministic.
    def specialized_snapshot(prepared):
        return [
            (
                tuple(sorted(contact.keys())),
                tuple(np.asarray(contact["position"], dtype=np.float64)),
                tuple(np.asarray(contact["normal"], dtype=np.float64)),
                int(contact["island_id"]),
                int(contact["coverage_face_id"]),
                bool(contact.get("minimum")),
                bool(contact.get("island_birth")),
            )
            for contact in prepared["contacts"]
            if contact.get("minimum") or contact.get("island_birth")
        ]

    assert specialized_snapshot(pre) == specialized_snapshot(repeat)
    synthetic_regular = [
        contact for contact in pre["contacts"]
        if contact["island_id"] in synthetic_ids
        and not contact.get("minimum") and not contact.get("island_birth")
    ]
    assert synthetic_regular
    assert {contact["island_id"] for contact in synthetic_regular} == synthetic_ids
    assert pre["log"]["synthetic_regular_contacts_added"] == len(
        synthetic_regular)
    pool = pre["contact_candidate_pool"]
    for contact in synthetic_regular:
        row = int(contact["candidate_pool_row"])
        assert int(pool.island_ids[row]) == contact["island_id"]
        assert int(pool.face_ids[row]) == contact["face_id"]
        assert np.array_equal(pool.positions[row], contact["position"])
        assert np.array_equal(pool.normals[row], contact["normal"])

    selection = pre["log"]["synthetic_regular_coverage_selection"]
    repeat_selection = repeat["log"]["synthetic_regular_coverage_selection"]
    assert selection == repeat_selection
    assert selection["exact"] is True
    assert selection["selected_rows"] == tuple(
        sorted(int(c["candidate_pool_row"]) for c in synthetic_regular))
    assert selection["selected_count"] == len(synthetic_regular)
    assert selection["max_matrix_entries"] <= (
        cfg.coverage_score_matrix_max_entries)
    assert selection["packed_mask_words"] <= (
        len(selection["components"])
        * cfg.coverage_score_matrix_max_entries)
    assert all(route["exact"] for route in selection["components"])

    by_island = {
        island_id: [
            c for c in synthetic_regular if c["island_id"] == island_id]
        for island_id in sorted(synthetic_ids)
    }
    # Four leg-bottom two-face islands are the common exact pair case.
    assert {
        island_id: len(by_island[island_id])
        for island_id in range(n_original_islands, n_original_islands + 4)
    } == {
        island_id: 2
        for island_id in range(n_original_islands, n_original_islands + 4)
    }
    assert all(
        route["searched_cardinality"] >= route["selected_cardinality"]
        for route in selection["components"])

    # Every regular seat stays at least the real 4 mm target from specialized
    # and other selected contacts in the same named island.
    target_sq = float(cfg.target_spacing_mm) ** 2
    for island_id in synthetic_ids:
        island_contacts = [
            contact for contact in pre["contacts"]
            if int(contact["island_id"]) == island_id]
        for contact in by_island[island_id]:
            others = [
                other for other in island_contacts if other is not contact]
            delta = (
                np.asarray([other["position"] for other in others], float)
                - np.asarray(contact["position"], float))
            assert np.all(np.einsum(
                "ij,ij->i", delta, delta, optimize=False) >= target_sq)

    # Pin exact A3 semantics: every prepared sample, not merely every
    # area-censored report patch, is within the component-local radius.
    assessments = []
    coverage_rows = verify.coverage_report(
        table_mesh, pre["islands"], pre["contacts"], cfg,
        coverage_samples=pre["coverage_samples"],
        face_topology=pre["face_topology"],
        assessment_out=assessments)
    assert [row for row in coverage_rows if not row["covered"]] == []
    assert np.all(assessments[0].covered_mask)

    # Minimum-cardinality sets cannot contain a blanket of redundant
    # zero-exclusive regulars. Also recompute and pin both deterministic tie
    # metrics advertised for every route key.
    radius = (
        float(cfg.target_spacing_mm)
        * float(cfg.coverage_radius_factor))
    component_ids = pre["face_topology"].component_ids
    samples = pre["coverage_samples"]
    running_island_positions = {
        island_id: [
            np.asarray(contact["position"], dtype=np.float64)
            for contact in pre["contacts"]
            if (int(contact.get("island_id", -1)) == island_id
                and (contact.get("minimum")
                     or contact.get("island_birth")))
        ]
        for island_id in synthetic_ids
    }
    for route in selection["components"]:
        island_id = int(route["island_id"])
        component_id = int(route["component_id"])
        sample_slice = samples.island_slice(island_id)
        sample_rows = np.arange(
            sample_slice.start, sample_slice.stop, dtype=np.int64)
        sample_rows = sample_rows[
            component_ids[samples.face_ids[sample_rows]] == component_id]
        route_contacts = []
        for contact in pre["contacts"]:
            if int(contact.get("island_id", -1)) != island_id:
                continue
            face_id = contact.get(
                "coverage_face_id", contact.get("face_id"))
            if (face_id is not None
                    and int(component_ids[int(face_id)]) == component_id):
                route_contacts.append(contact)
        distances = np.linalg.norm(
            np.asarray(
                [contact["position"] for contact in route_contacts],
                dtype=np.float64)[:, None, :]
            - samples.positions[sample_rows][None, :, :],
            axis=2)
        assert float(distances.min(axis=0).max()) == pytest.approx(
            route["worst_sample_gap_mm"])
        selected_route_indices = [
            idx for idx, contact in enumerate(route_contacts)
            if "candidate_pool_row" in contact]
        zero_exclusive = 0
        for idx in selected_route_indices:
            other = np.delete(distances, idx, axis=0)
            exclusive = (
                (distances[idx] <= radius)
                & np.all(other > radius, axis=0))
            zero_exclusive += int(not np.any(exclusive))
        assert zero_exclusive <= 1

        running_island_positions[island_id].extend(
            np.asarray(pool.positions[int(row)], dtype=np.float64)
            for row in route["selected_rows"])
        all_island_positions = np.asarray(
            running_island_positions[island_id], dtype=np.float64)
        pair_min = np.inf
        for left in range(len(all_island_positions) - 1):
            pair_min = min(
                pair_min,
                float(np.linalg.norm(
                    all_island_positions[left + 1:]
                    - all_island_positions[left], axis=1).min()))
        assert pair_min == pytest.approx(
            route["min_selection_pair_spacing_mm"])


def test_prepared_synthetic_regular_rows_are_individually_required_by_actual_a3(
        table_mesh, cfg):
    pre = engine.prepare(table_mesh, cfg)
    engine.validate_prepared_state(pre, table_mesh, cfg)
    token = engine.prepared_validation._issue_prepared_validation_token(
        pre, table_mesh, cfg)
    n_original_islands = int(pre["log"]["islands"])
    selected_rows = tuple(
        int(row)
        for row in pre["log"]["synthetic_regular_coverage_selection"][
            "selected_rows"])
    selected = [
        (contact_index, contact)
        for contact_index, contact in enumerate(pre["contacts"])
        if ("candidate_pool_row" in contact
            and int(contact["candidate_pool_row"]) in selected_rows)
    ]

    # Selection provenance, rather than a frozen fixture row list, identifies
    # exactly the synthetic regular contacts whose A3 necessity is under test.
    assert tuple(sorted(
        int(contact["candidate_pool_row"])
        for _contact_index, contact in selected)) == selected_rows
    assert selected
    assert all(
        int(contact["island_id"]) >= n_original_islands
        and not contact.get("minimum")
        and not contact.get("island_birth")
        for _contact_index, contact in selected)

    diagnostics = []
    for contact_index, contact in selected:
        remaining_contacts = (
            pre["contacts"][:contact_index]
            + pre["contacts"][contact_index + 1:])
        coverage_rows = verify.coverage_report(
            table_mesh, pre["islands"], remaining_contacts, cfg,
            coverage_samples=pre["coverage_samples"],
            face_topology=pre["face_topology"],
            _prepared_validation_token=token)
        island_id = int(contact["island_id"])
        island_row = coverage_rows[island_id]
        failing_patches = [
            patch for patch in island_row["patches"] if patch["fails"]]
        diagnostic = dict(
            candidate_pool_row=int(contact["candidate_pool_row"]),
            island_id=island_id,
            face_id=int(contact["face_id"]),
            covered=bool(island_row["covered"]),
            patch_area_floor_mm2=float(
                island_row["patch_area_floor_mm2"]),
            failing_uncovered_area_mm2=float(
                island_row["failing_uncovered_area_mm2"]),
            max_patch_area_mm2=float(
                island_row["max_patch_area_mm2"]),
            n_failing_patches=int(
                island_row["n_failing_patches"]),
            failing_patches=failing_patches)
        diagnostics.append(diagnostic)

        assert island_row["covered"] is False, diagnostic
        assert failing_patches, diagnostic
        assert any(
            patch["ambiguous_topology_cut"]
            or patch["uncovered_area_mm2"]
            > island_row["patch_area_floor_mm2"]
            for patch in failing_patches), diagnostic

    assert len(selected_rows) == len(diagnostics) == 11
    maximum_patch_areas = [
        diagnostic["max_patch_area_mm2"] for diagnostic in diagnostics]
    assert min(maximum_patch_areas) == pytest.approx(3.453369140625)
    assert max(maximum_patch_areas) == pytest.approx(18.798828125)
    assert {
        diagnostic["patch_area_floor_mm2"]
        for diagnostic in diagnostics
    } == {0.16000000000000003}


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


def test_normalized_table_is_clean_and_default_phase8_is_a_noop():
    default = config_mod.SupportConfig()
    baseline = json.load(open(_BASELINE_PATH))
    assert default.p8_relocate_enable is True
    clean = engine.run(mesh_io.make_overhang_table(), default)
    assert clean["verify"]["ok"] is True
    assert clean["verify"]["ball_breach_violations"] == []
    assert clean["log"]["p8_n_unresolvable"] == 0
    assert clean["log"]["relocate"]["entered_ladder"] is False
    assert clean["log"]["relocate_transaction"]["committed"] is True
    assert clean["log"]["coverage_repair"]["attempts"] == 0
    assert clean["volume"]["total_ml"] <= baseline["table"]["total_ml"]

    explicit_off = engine.run(
        mesh_io.make_overhang_table(),
        dataclasses.replace(default, p8_relocate_enable=False))
    assert explicit_off["verify"]["ok"] is True
    assert explicit_off["verify"]["ball_breach_violations"] == []
    assert explicit_off["verify"]["uncovered_islands"] == []
    assert "t_relocate" not in explicit_off["log"]
    assert "relocate" not in explicit_off["log"]


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


def _refreeze_fake_result():
    import _refreeze_v5 as refreeze

    verify_result = {
        "ok": True,
        **{field: [] for field in refreeze.HARD_GATE_FIELDS},
        "removal_report": {"blocked_trees": []},
        "removal_violations": [],
        "moot_transit_members": [],
        "transit_chords_mm": {},
        "endcap_breach_diagnostic": [],
    }
    return {
        "volume": {
            "total_ml": 1.0, "member_ml": 0.75, "ball_ml": 0.25,
            "by_kind_ml": {"stub": 0.75},
        },
        "verify": verify_result,
        "unrouted": [],
        "contacts": [{
            "position": np.array([1.0, 2.0, 3.0]),
            "normal": np.array([0.0, 0.0, -1.0]),
            "candidate_pool_row": 7,
        }],
        "members": [{
            "p0": np.array([1.0, 2.0, 3.0]),
            "p1": np.array([1.0, 2.0, 0.0]),
            "r0": 0.2, "r1": 0.4, "kind": "stub",
            "owner": ("contact", 0),
        }],
        "balls": [],
        "carried_area_mm2": np.array([1.0]),
        "tip_r_mm": np.array([0.2]),
        "islands": [np.array([0], dtype=np.int64)],
        "minima": [],
        "island_births": [],
        "coverage_failed_candidate_pool_rows": (),
        "log": {
            "gates_failed": False,
            "rewire": {"accepted": 1, "rejected": 0},
            "synthetic_regular_coverage_selection": {
                "selected_rows": (7,), "exact": True},
            "relocate_transaction": {"committed": True, "dt_s": 0.01},
            "t_total": 1.2,
            "measured_total_rss_high_water_delta_bytes": 123,
        },
    }


def test_refreeze_signature_ignores_telemetry_but_covers_provenance():
    import _refreeze_v5 as refreeze

    first = _refreeze_fake_result()
    second = pickle.loads(pickle.dumps(first))
    second["log"]["t_total"] = 99.0
    second["log"]["relocate_transaction"]["dt_s"] = 2.5
    second["log"]["measured_total_rss_high_water_delta_bytes"] = 999
    assert refreeze.canonical_signature(first) == (
        refreeze.canonical_signature(second))

    second["contacts"][0]["candidate_pool_row"] = 8
    assert refreeze.canonical_signature(first) != (
        refreeze.canonical_signature(second))


def test_refreeze_pair_requires_two_gate_clean_full_signatures(monkeypatch):
    import _refreeze_v5 as refreeze

    first = _refreeze_fake_result()
    second = pickle.loads(pickle.dumps(first))
    calls = []
    queue = [first, second]

    def fake_run_once(_mesh_fn):
        calls.append(True)
        return queue.pop(0)

    monkeypatch.setattr(refreeze, "run_once", fake_run_once)
    result, signature = refreeze.run_pair("fixture", lambda: None)
    assert result is first
    assert signature == refreeze.canonical_signature(first)
    assert calls == [True, True]

    bad_gate = pickle.loads(pickle.dumps(first))
    bad_gate["verify"]["clearance_violations"] = [{"member_idx": 1}]
    with pytest.raises(RuntimeError, match="clearance_violations"):
        refreeze.require_gate_clean("fixture", bad_gate)

    divergent = pickle.loads(pickle.dumps(first))
    divergent["log"]["relocate_transaction"]["committed"] = False
    queue[:] = [first, divergent]
    with pytest.raises(RuntimeError, match="NON-DETERMINISTIC"):
        refreeze.run_pair("fixture", lambda: None)


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
