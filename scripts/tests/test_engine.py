import json
import os

import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import config as config_mod, engine, mesh_io, rewire, tree as tm


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
    # v3 baseline on this fixture (fixtures_v3_baseline.json) was 2
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
    # composed-engine baseline was ~10.2s (2026-07-22). v5-geom-core swap
    # steps 1+2 (exact clearance + exact model anchors, SPEC-V5-GEOM secs
    # 2.1/2.2/2.3) measure ~19.5-20.1s on this machine -- real geometry
    # work costs real time; re-baselined here (not tightened) with wide
    # headroom since the table fixture sat flaky right at the old 20.0s
    # pin (a coin flip, not a regression). Step 3 (perception/load-
    # partition swap) may move this again; re-measure, don't guess.
    assert result["log"]["t_total"] < 30.0


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


def test_engine_box_gets_exactly_the_bottom_plateau_minimum(box_mesh, cfg):
    """v3.6 rule, unchanged in v4: under lift a flat bottom is the
    print's FIRST FEP island, so the honest answer is one bottom-plateau
    minimum with one contact pinning it -- and nothing else."""
    result = engine.run(box_mesh, cfg)
    assert result["log"]["minima"] == 1
    assert len(result["contacts"]) == 1
    assert result["contacts"][0].get("minimum") is True
    assert result["verify"]["unsupported_minima"] == []
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
    # composed-engine baseline was ~10.2s (2026-07-22). v5-geom-core swap
    # steps 1+2 (exact clearance + exact model anchors, SPEC-V5-GEOM secs
    # 2.1/2.2/2.3) measure ~19.5-20.1s on this machine -- real geometry
    # work costs real time; re-baselined here (not tightened) with wide
    # headroom since the table fixture sat flaky right at the old 20.0s
    # pin (a coin flip, not a regression). Step 3 (perception/load-
    # partition swap) may move this again; re-measure, don't guess.
    assert result["log"]["t_total"] < 30.0


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


def test_engine_no_contacts_returns_honest_empty_result(cfg):
    """Degenerate case: a mesh with essentially no down-facing overhang
    beyond near_plate_mm still returns a complete, well-formed result
    (the empty-contacts early-return path) instead of crashing."""
    import trimesh
    flat = trimesh.creation.box(extents=(10, 10, 0.5))
    m = trimesh.Trimesh(vertices=flat.vertices.copy(), faces=flat.faces.copy(),
                         process=True)
    result = engine.run(m, cfg)
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


_BASELINE_PATH = os.path.join(os.path.dirname(__file__), "fixtures_v4_p4_baseline.json")


def test_table_and_toed_slab_full_run_not_worse_than_baseline(cfg):
    """SPEC-INTEGRATION #4's acceptance gate + trap detector: table and
    toed_slab through the FULL new run() (construct -> rewire ->
    brace_and_size -> verify) must stay gate-clean and not exceed
    fixtures_v4_p4_baseline.json's total_ml.

    Phase-4-final re-freeze (Craig-approved, honest physics): the prior
    fixtures_v4_p3_baseline.json numbers predate the r_deflect floor,
    design-v4.md #8's transient honest spans, cco.py's braced-cost pricing
    + bracing, the joint-ball diet, X-pair own-floor sizing, and seeded
    deterministic sampling -- all of which legitimately move total_ml/
    member_ml/rewire_accepted versus that old freeze. The GATE FORMULA
    itself (not-worse-than-frozen, +1e-6 tolerance) is unchanged; only the
    frozen numbers moved, re-measured on this engine and confirmed
    bit-identical across two independent engine.run calls per fixture
    before freezing (see the p4-refreeze commit message for old-vs-new
    values and the double-run proof). fixtures_v4_p3_baseline.json is kept
    untouched for history."""
    baseline = json.load(open(_BASELINE_PATH))

    for name, mesh_fn in (("table", mesh_io.make_overhang_table),
                           ("toed_slab", mesh_io.make_toed_slab)):
        mesh = mesh_fn()
        result = engine.run(mesh, config_mod.SupportConfig())
        v = result["verify"]
        assert v["clearance_violations"] == [], (name, v["clearance_violations"])
        assert v["connectivity_violations"] == [], (name, v["connectivity_violations"])
        assert v["printability_violations"] == [], (name, v["printability_violations"])
        assert v["murray_violations"] == [], (name, v["murray_violations"])
        # v4 p6 (I3): HARD acceptance on these small fixtures -- the honest
        # island-birth gate, with the I2-demanded contacts actually
        # generated, routed, and covering.
        assert v["uncovered_island_births"] == [], (name, v["uncovered_island_births"])
        total_ml = result["volume"]["total_ml"]
        assert total_ml <= baseline[name]["total_ml"] + 1e-6, (
            f"{name}: total_ml {total_ml:.4f} exceeds frozen baseline "
            f"{baseline[name]['total_ml']} (not-worse trap detector)")
