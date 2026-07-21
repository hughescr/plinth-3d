from support_placement import engine


def test_engine_runs_end_to_end_on_table_fixture(table_mesh, cfg):
    # The table fixture is deliberately adversarial: overhang contacts on
    # the slab underside can land directly above a leg, and v2's router is
    # a fixed stub+elbow+descent kink (design.md #4) with a bounded local
    # detour search (engine._repair_clearance_with_detours) -- not a
    # general path planner. When a leg sits exactly under the stub's own
    # exit point, no lateral direction clears it in the first few mm
    # (the descent would have to pass through the leg's own footprint to
    # leave it). That's an honest limitation, not a bug: coverage and load
    # must still be clean, and self_verify must still SURFACE any
    # remaining clearance issue rather than silently reporting ok.
    result = engine.run(table_mesh, cfg)
    assert len(result["contacts"]) > 0
    # v3.2 contract: every contact either gets a CLEAR route or is
    # honestly reported unrouted -- never a through-model member. The
    # leg-blocked contacts (planted right above a leg, v2's known-hard
    # case) are the expected unrouted ones.
    assert len(result["routes"]) + len(result["unrouted"]) == len(result["contacts"])
    assert len(result["unrouted"]) <= 4
    assert result["verify"]["load_violations"] == []
    # the no-intersection guarantee: zero clearance violations, always
    assert result["verify"]["clearance_violations"] == [], result["verify"]
    assert result["log"]["t_total"] < 10.0


def test_engine_reports_trunk_plan(table_mesh, cfg):
    result = engine.run(table_mesh, cfg)
    info = result["log"]["trunks_pass0"]
    assert info["n_trunks"] == len(result["trunks"]) > 0
    # every contact is attached to exactly one trunk OR a model strut
    n_struts = sum(1 for r in result["routes"] if r.anchor_kind == "model")
    assert sum(len(t.shoots) for t in result["trunks"]) + n_struts \
        + len(result["unrouted"]) == len(result["contacts"])
    assert all(len(t.shoots) <= cfg.trunk_max_shoots for t in result["trunks"])


def test_engine_no_member_leans_past_printability_angle(table_mesh, cfg):
    """v3 invariant from the in-Chitubox review: no floating horizontal
    runs -- every non-stub member (shoots, trunk columns, braces) leans at
    most cfg.max_segment_angle_deg from vertical."""
    import numpy as np
    from support_placement import trunks as trunks_mod
    result = engine.run(table_mesh, cfg)
    for route in result["routes"]:
        # model-anchored fallback struts are rooted on already-printed
        # model, so they get the (steeper) strut angle budget instead
        max_deg = (cfg.strut_fallback_max_angle_deg
                   if route.anchor_kind == "model" else cfg.max_segment_angle_deg)
        for seg in route.segments:
            if seg.kind == "stub":
                continue  # stubs follow the surface normal by design
            d = np.asarray(seg.p1, float) - np.asarray(seg.p0, float)
            L = np.linalg.norm(d)
            lean = np.degrees(np.arccos(min(1.0, abs(d[2]) / L))) if L > 1e-9 else 0.0
            assert lean <= max_deg + 0.5, (route.anchor_kind, seg.kind, lean)
    for b in result["braces"]:
        if b["j"] in (-1, -3):
            # rungs (trunk-to-model, or shoot thinning rungs to trunks/
            # sibling shoots/model): short two-end-supported bridges --
            # both ends sit on structure already printed at that layer.
            # The 45-degree rule is for members that would otherwise
            # dangle from the raft.
            L = float(np.linalg.norm(np.asarray(b["p1"]) - np.asarray(b["p0"])))
            assert L <= cfg.model_brace_max_len_mm + 1e-6
        else:
            assert trunks_mod._angle_ok(b["p0"], b["p1"], cfg)
    for t in result["trunks"]:
        assert t.r >= cfg.min_trunk_r_mm


def test_engine_trunk_radius_meets_load_and_span_requirements(table_mesh, cfg):
    """Members must actually be sized by need (review finding #5). Since
    v3.2 the driver is the TRANSIENT worst span (build-time cantilever,
    restraint-activation-aware), so more load does not imply thicker --
    a better-braced trunk is legitimately thinner. The real invariant:
    every trunk meets BOTH its axial (load) floor and the deflection
    requirement for its actual worst span, within the clamp band."""
    from support_placement import placement
    result = engine.run(table_mesh, cfg)
    for t in result["trunks"]:
        assert cfg.min_trunk_r_mm <= t.r <= cfg.max_trunk_r_mm
        if t.r < cfg.max_trunk_r_mm:  # not clamped down by the cap
            scale = max(len(t.shoots), 1) ** 0.125
            r_need = max(t.r_axial,
                          placement._shaft_r_for_L(t.eff_len, cfg) * scale)
            assert t.r >= min(r_need, cfg.max_trunk_r_mm) - 1e-6


def test_engine_box_alone_needs_no_supports(box_mesh, cfg):
    result = engine.run(box_mesh, cfg)
    assert result["contacts"] == []
    assert result["verify"]["ok"]
