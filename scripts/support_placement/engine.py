"""v4 slim orchestrator (design-v4.md #2, #14 phase 2): perception ->
sampling -> load partition -> tip sizing -> delicacy field -> cco.build
-> verify.self_verify, with a bounded (<=2 pass) fix loop for islands
that came up short on coverage. NO stability loop, NO route/trunk
machinery -- those retire with trunks.py/braces.py's policy role
(design-v4.md #2); the CCO builder's own transactional certificate rule
(tree.py) replaces v3's repeated post-hoc gate/repair passes.

See references/design-v4.md for the charter and __init__.py for the
module map.
"""
import contextlib
import sys
import threading
import time
import numpy as np

from . import perception, placement, routing, verify, joints, cco, tree, rewire
from . import transient, bracing
from .geometry_kernel import KernelSurface


@contextlib.contextmanager
def _stage(log, name, run_t0):
    """Per-stage progress + timing (SPEC-RUST-KERNEL sec 7). Owner-mandated so
    a full-scale run is never a heartbeat-only black box: on enter prints
    ``[v5] >> {name}``; on exit prints
    ``[v5] << {name} dt={dt}s t=+{total}s`` -- BOTH to stderr (stdout stays
    clean for result piping), flush=True. ``dt`` is this stage's own duration;
    ``total`` is cumulative since the run's ``run_t0``. Records
    ``log[f"t_{name}"]`` = cumulative time (round 2), which preserves today's
    key semantics verbatim for the stages that already had them (t_geom_build,
    t_surface_tree were both ``time.time() - run_t0`` cumulative) and adds the
    rest (t_prepare, t_construct[p{i}], t_rewire[p{i}], t_brace_and_size[p{i}],
    t_verify[p{i}], t_finish)."""
    t_stage = time.time()
    print(f"[v5] >> {name}", file=sys.stderr, flush=True)
    try:
        yield
    finally:
        now = time.time()
        print(f"[v5] << {name} dt={now - t_stage:.1f}s t=+{now - run_t0:.1f}s",
              file=sys.stderr, flush=True)
        log[f"t_{name}"] = round(now - run_t0, 2)


class _ProgressCell:
    """SPEC-V5-PERF sec 4: a single monotonic int cell, written by the compute
    thread (cco.build) after each insertion, read by the ticker thread. CPython
    int assignment/read cannot tear, so no lock is needed for a single writer +
    reader-only observer."""
    __slots__ = ("n",)

    def __init__(self):
        self.n = 0


@contextlib.contextmanager
def _ticker(label, get_n, total, interval=5.0):
    """SPEC-V5-PERF sec 4 (owner-mandated, task #17): a daemon thread that
    prints ``[v5] .. {label} {n}/{total} contacts`` to stderr every `interval`
    seconds while the wrapped stage runs, so a full-scale construct is never a
    heartbeat-only black box. ALL timing lives in the thread's
    ``Event.wait(interval)``; the compute path only advances the counter
    get_n() reads (no kernel calls, no allocation), so output is byte-identical
    with the ticker on or off. Event-signaled shutdown + join in finally
    (never leaves a dangling thread, even on exception -- risk P7)."""
    stop = threading.Event()

    def _run():
        while not stop.wait(interval):
            print(f"[v5] .. {label} {get_n()}/{total} contacts",
                  file=sys.stderr, flush=True)

    t = threading.Thread(target=_run, name=f"ticker-{label}", daemon=True)
    t.start()
    try:
        yield
    finally:
        stop.set()
        t.join()


def prepare(mesh, cfg, log=None, geom=None):
    """Stage 1 of the pipeline: everything that depends only on the mesh.
    Returns a picklable dict (no KDTree/KernelSurface -- rebuild per
    process with routing.build_surface_tree / KernelSurface(mesh)):
    islands, minima, island_births, contacts, plate_z, sample_points,
    sample_delicacy, log. Extracted from run() so Treant-scale runs can
    checkpoint between bounded foreground calls; run() remains the
    canonical composition.

    `geom` (v5, SPEC-V5-GEOM sec 2.3, optional): when supplied (a
    surface-EXACT oracle, NOT stored in the returned dict -- it is only
    used here to build sample_points/sample_delicacy), the model-anchor
    candidate pool becomes deterministic exterior-face centroids instead
    of delicacy_field's seeded sample cloud (perception.exterior_anchor_
    pool). Callers that omit it (unit tests / legacy) get the v4 pool
    verbatim, unchanged."""
    log = log if log is not None else {}
    t0 = time.time()

    islands = perception.overhang_islands(mesh, cfg, geom=geom)
    log["islands"] = len(islands)
    log["t_islands"] = round(time.time() - t0, 2)

    # v3.6 (Craig, top priority, carried forward unchanged): island-birth
    # minima. Detected before sampling so every minimum the overhang-angle
    # model missed (steep flanks) still ends up with a contact AT its
    # birth region.
    minima = perception.local_z_minima(mesh, cfg)
    log["minima"] = len(minima)
    log["t_minima"] = round(time.time() - t0, 2)

    # v4 p6 (I1, Craig's strict criterion): ground-truth slice-based
    # island-birth detection at the real print layer height, computed ONCE
    # here and carried in `pre` for both contact demand (below) and the
    # honest verify gate (verify.island_birth_report) -- see
    # perception.island_births' module-level comment for why this is a
    # necessary cross-check on local_z_minima above, not a duplicate of it.
    island_births = perception.island_births(mesh, cfg)
    log["island_births"] = len(island_births)
    log["t_island_births"] = round(time.time() - t0, 2)

    # SPEC-V5-PERF B3 (perf_seed_spacing_factor, default OFF): multiply the
    # SEEDING target spacing by f > 1 -- fewer seeds into a smaller forest.
    # SEEDING ONLY: the coverage gate consumes cfg.target_spacing_mm unchanged
    # (a scaled cfg is passed only to sample_candidates via dataclasses.replace,
    # so coverage itself never loosens). QUALITY lever: higher per-support
    # carried load, coverage margin thins toward thresholds (5.3 measures it).
    seed_f = getattr(cfg, "perf_seed_spacing_factor", None)
    seed_cfg = cfg
    if seed_f is not None:
        import dataclasses
        seed_cfg = dataclasses.replace(
            cfg, target_spacing_mm=cfg.target_spacing_mm * float(seed_f))
    contacts = placement.sample_candidates(mesh, islands, seed_cfg)
    log["contacts_initial"] = len(contacts)
    contacts, islands, n_min_added = _ensure_minima_contacts(
        mesh, islands, contacts, minima, cfg, geom=geom)
    log["minima_contacts_added"] = n_min_added

    # I2: demand one contact (or a short "ladder" for a raking/migrating
    # cluster) per island-birth footprint not already satisfied by an
    # existing contact under the strict at-birth criterion. Runs AFTER
    # the minima pass so births local_z_minima already covers produce no
    # duplicate demand (dedup is by the strict criterion here, not the
    # old 1mm minima_contact_radius_mm -- see _ensure_island_birth_contacts).
    contacts, islands, n_birth_added = _ensure_island_birth_contacts(
        mesh, islands, contacts, island_births, cfg, geom=geom)
    log["island_birth_contacts_added"] = n_birth_added

    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    sample_points, sample_delicacy = perception.exterior_anchor_pool(mesh, geom, cfg)
    log["t_delicacy"] = round(time.time() - t0, 2)
    return dict(islands=islands, minima=minima, island_births=island_births,
                contacts=contacts, plate_z=plate_z, sample_points=sample_points,
                sample_delicacy=sample_delicacy, log=log)


def construct(mesh, pre, surface_tree, cfg, log=None, limit_contacts=None, geom=None,
              progress=None):
    """Stage 2: load partition + tip sizing + cco.build (NO rewiring --
    the caller owns rewire.optimize so its wall budget is its own).
    `limit_contacts` truncates the contact list for profiling only.
    `geom` (v5 exact oracle, SPEC sec 2.1) drives cco's per-candidate
    clearance revalidation when supplied; without it, the legacy cKDTree
    `surface_tree` does (tests/staged-legacy). Returns (forest, unrouted,
    carried, tip_r, contacts)."""
    log = log if log is not None else {}
    oracle = geom if geom is not None else surface_tree
    contacts = pre["contacts"][:limit_contacts] if limit_contacts else pre["contacts"]
    carried = placement.load_partition(mesh, pre["islands"], contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    minima_flags = [bool(c.get("minimum")) for c in contacts]
    forest, unrouted = cco.build(mesh, contacts, tip_r, carried, minima_flags,
                                  pre["plate_z"], oracle, cfg,
                                  sample_points=pre["sample_points"],
                                  sample_delicacy=pre["sample_delicacy"],
                                  progress=progress)
    tree.assert_all_certified(forest)
    # SPEC-V5-PERF A0b: surface the quarantined (condemned-stub) contacts and
    # their carried surface area in the run log so the census cross-check
    # (SPEC sec 0/2.5.5) can rule on each -- the load these contacts would
    # have carried is declared, accepted QUALITY loss, not silently dropped.
    if forest.quarantined:
        stub_of = {idx: i for i, idx in forest.terminal_of.items()}
        q = sorted((stub_of[si], si, float(carried[stub_of[si]]))
                   for si in forest.quarantined if si in stub_of)
        log["quarantined_contacts"] = [
            dict(contact=ci, stub_member=si, carried_mm2=round(area, 4))
            for ci, si, area in q]
        log["n_quarantined"] = len(q)
        print(f"[v5] .. construct quarantined {len(q)} condemned-stub contact(s), "
              f"carried_mm2 total={sum(a for _, _, a in q):.2f}",
              file=sys.stderr, flush=True)
    return forest, unrouted, carried, tip_r, contacts


def live_member_lists(forest, unrouted, plate_z, cfg):
    """Stage 3a: the verify/emission-facing member + ball lists. An
    honestly-unrouted terminal's never-attached stub, and rewire-retired
    (Forest.dead) members, are excluded -- `unrouted` reports the former
    honestly and the latter were folded into survivors."""
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted}
    live = [m for mi, m in enumerate(forest.members)
            if mi not in unrouted_stub_idx and mi not in forest.dead]
    members = joints.collect_members(live)
    balls = joints.compute_joint_balls(live, plate_z, cfg)
    return members, balls


def _live_member_ml(forest, unrouted):
    """member_ml over exactly the same live set live_member_lists uses
    (honestly-unrouted stubs and rewire-dead members excluded), without
    paying for compute_joint_balls -- brace_and_size only needs the member
    total for its before/after log, not the ball list."""
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted}
    live = [m for mi, m in enumerate(forest.members)
            if mi not in unrouted_stub_idx and mi not in forest.dead]
    return tree.volume_report(live, [])["member_ml"]


def brace_and_size(mesh, forest, unrouted, surface_tree, plate_z, cfg, log=None, geom=None):
    """design-v4.md #8/#9 fixed point (SPEC-INTEGRATION #3): bounded
    propose-braces / resize-down-and-up / batch-revalidate loop (<=3
    iterations), then a SECOND rewire.optimize pass against the now-braced
    reality (design-v4.md #7: rewire after ANY structural pass -- braces
    just changed which spans are restrained), then a final re-fix so the
    forest handed back to the caller is clearance-clean at its final
    (braced) radii. Mutates `forest` in place; returns the log dict this
    call wrote to log['brace_and_size'].

    A member that GREW past clearance during the loop is never silently
    shrunk back -- its need (a real transient span) and its violation (a
    real clearance conflict) are both honest; the fix is more restraint
    next iteration (another brace), not corruption-by-omission.
    """
    log = log if log is not None else {}
    oracle = geom if geom is not None else surface_tree
    member_ml_before = _live_member_ml(forest, unrouted)

    iterations = 0
    rungs_placed = 0
    rungs_unplaceable = 0
    n_resized_down = 0
    n_resized_up = 0
    brace_iters = 3
    if cfg.perf_fix_iter_caps is not None:      # SPEC-V5-PERF B4
        brace_iters = int(cfg.perf_fix_iter_caps[1])
    for it in range(brace_iters):
        iterations = it + 1
        report = transient.transient_report(forest, plate_z, cfg)
        placed = bracing.propose_and_place(forest, surface_tree, plate_z, cfg, report,
                                            log=log, geom=geom)
        resize = transient.resize_forest(forest, plate_z, cfg)
        violations = forest.revalidate(oracle, cfg)
        rungs_placed += placed["rungs_placed"]
        rungs_unplaceable += placed["rungs_unplaceable"]
        for _idx, r_old, r_new in resize["resized"]:
            if r_new < r_old - 1e-9:
                n_resized_down += 1
            elif r_new > r_old + 1e-9:
                n_resized_up += 1
        if not placed["placed"] and not resize["resized"] and not violations:
            break

    # Design-v4.md #7: rewire again after ANY structural pass -- the
    # braces/resizes above just changed which spans are restrained and how
    # fat every member is, exactly the kind of neighborhood change that can
    # newly expose a parallel-column merge or a now-degenerate twig.
    rewire.optimize(forest, mesh, oracle, cfg, log=log)

    # Re-fix: rewire may have created or moved unbraced members (a fresh
    # reattach connector, a slid junction) -- one more propose/resize pass
    # against that post-rewire reality before certifying.
    report = transient.transient_report(forest, plate_z, cfg)
    bracing.propose_and_place(forest, surface_tree, plate_z, cfg, report, log=log, geom=geom)
    transient.resize_forest(forest, plate_z, cfg)
    violations = forest.revalidate(oracle, cfg)
    assert not violations, (
        f"brace_and_size fixed point ended with {len(violations)} "
        f"clearance-dirty member(s) after re-fix (an honest engine failure, "
        f"not a warning) -- member_idx: radius_mm = " + ", ".join(
            f"{v['member_idx']}: "
            f"{round(max(forest.members[v['member_idx']].r0, forest.members[v['member_idx']].r1), 4)}"
            for v in violations))
    tree.assert_all_certified(forest)

    member_ml_after = _live_member_ml(forest, unrouted)
    log["brace_and_size"] = dict(
        iterations=iterations, rungs_placed=rungs_placed,
        rungs_unplaceable=rungs_unplaceable, n_resized_down=n_resized_down,
        n_resized_up=n_resized_up, member_ml_before=member_ml_before,
        member_ml_after=member_ml_after)
    return log["brace_and_size"]


def finish(mesh, pre, forest, unrouted, contacts, carried, tip_r,
           surface_tree, cfg, log=None, geom=None):
    """Stage 3b: verify gates + volume report on the final forest."""
    log = log if log is not None else {}
    plate_z = pre["plate_z"]
    tree.assert_all_certified(forest)
    members, balls = live_member_lists(forest, unrouted, plate_z, cfg)
    routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
    report = verify.self_verify(mesh, pre["islands"], routed_contacts, members,
                                 carried, tip_r, cfg, plate_z,
                                 surface_tree=surface_tree, minima=pre["minima"],
                                 forest=forest, unrouted=unrouted,
                                 island_births=pre["island_births"],
                                 geom=geom, balls=balls)
    volume = tree.volume_report(members, balls)
    log["n_unrouted"] = len(unrouted)
    return dict(members=members, balls=balls, verify=report, volume=volume,
                plate_z=plate_z, log=log)


def run(mesh, cfg, log=None):
    """Run the full v4 pipeline on an already-loaded trimesh.Trimesh.
    Returns a result dict: islands, contacts, carried_area_mm2, tip_r_mm,
    forest, members, balls, unrouted, minima, plate_z, verify, volume,
    log."""
    log = log if log is not None else {}
    t0 = time.time()

    # v5 geometry core: the surface-EXACT oracle, now the Rust geom_kernel
    # (SPEC-RUST-KERNEL, de-Python stage 1) -- a drop-in for the frozen
    # Python ExactSurface oracle, same public API/semantics/dtypes, parallel
    # across all cores (GEOM_KERNEL_THREADS), bit-deterministic run-to-run and
    # thread-count invariant. Built FIRST (ahead of prepare()) so prepare()'s
    # own model-anchor pool (SPEC-V5-GEOM sec 2.3) can already be exterior-face
    # centroids rather than a seeded cloud -- prepare() no longer has a cheaper
    # legacy path to fall back to once geom exists, so this pays the kernel
    # build cost even on a mesh that turns out to have zero contacts; that
    # build is sub-second (BVH + parity CSR over the soup) and never the
    # dominant cost. Every load-bearing clearance/anchor decision downstream
    # (both clearance implementations, model anchors, perception seeding +
    # interior-face filter) consults the true triangles via this kernel, not
    # the sampled cloud. The Python ExactSurface survives ONLY as the frozen
    # differential-test oracle (scripts/tests/test_kernel_parity.py).
    with _stage(log, "geom_build", t0):
        geom = KernelSurface(mesh)

    with _stage(log, "prepare", t0):
        pre = prepare(mesh, cfg, log, geom=geom)
    islands, minima, contacts = pre["islands"], pre["minima"], pre["contacts"]
    island_births = pre["island_births"]
    plate_z = pre["plate_z"]

    if not contacts:
        log["t_total"] = round(time.time() - t0, 2)
        empty_forest = tree.Forest()
        return dict(
            islands=islands, contacts=[], carried_area_mm2=np.array([]),
            tip_r_mm=np.array([]), forest=empty_forest, members=[], balls=[],
            unrouted=[], minima=minima, plate_z=plate_z,
            verify=dict(ok=True, uncovered_islands=[], clearance_violations=[],
                        moot_transit_members=[],
                        ball_breach_violations=[], endcap_breach_diagnostic=[],
                        load_violations=[], unsupported_minima=[],
                        connectivity_violations=[], printability_violations=[],
                        murray_violations=[], uncovered_island_births=[],
                        removal_report=dict(ok=True, n_trees=0, trees=[],
                                             removal_order=[], removal_effort={},
                                             blocked_trees=[]),
                        removal_violations=[]),
            volume=dict(member_ml=0.0, ball_ml=0.0, total_ml=0.0, by_kind_ml={}),
            log=log)

    # Built once, reused for every clearance query this run makes (see
    # routing.build_surface_tree's docstring). Tighter sample than the
    # default: every no-intersection guarantee in this engine rides on
    # this tree, so keep its approximation error small against
    # standoff_mm. Kept alive for the not-yet-migrated consumers
    # (connectivity/removal diagnostics -- SPEC step 3); model anchors
    # (this step) and clearance (step 1) no longer read it.
    with _stage(log, "surface_tree", t0):
        surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)

    forest = members = balls = report = carried = tip_r = unrouted = None
    # SPEC-V5-PERF B4 (perf_fix_iter_caps=(engine_passes, brace_iters), default
    # OFF): lower the fix-pass and brace loop bounds. Can leave residual
    # UNCOVERED ISLANDS -- a hard-gate failure, not a resin trade -- so it is
    # gated by the mechanical emission gate on verify.ok below
    # (perf_gate_emit_on_verify). QUALITY/coverage lever (5.3).
    max_fix_passes = 2
    if cfg.perf_fix_iter_caps is not None:
        max_fix_passes = int(cfg.perf_fix_iter_caps[0])
    for fix_pass in range(max_fix_passes):
        pre["contacts"] = contacts  # fix-pass additions feed back in
        with _stage(log, f"construct[p{fix_pass}]", t0):
            _prog = _ProgressCell()
            with _ticker(f"construct[p{fix_pass}]", lambda: _prog.n, len(pre["contacts"])):
                forest, unrouted, carried, tip_r, contacts = construct(
                    mesh, pre, surface_tree, cfg, log=log, geom=geom, progress=_prog)
        # Phase 3 (design-v4.md #7): topology mutation runs AFTER
        # insertion, BEFORE verify -- rewire.optimize mutates `forest` in
        # place (subtree reattach / junction slide / twig prune), each
        # move already transactional+certificate-checked internally;
        # re-asserting after is a cheap belt-and-suspenders check.
        with _stage(log, f"rewire[p{fix_pass}]", t0):
            rewire.optimize(forest, mesh, geom, cfg, log=log)
        # Phase 4c (design-v4.md #8/#9): build-time-honest transient sizing
        # + bracing fixed point, then its own second rewire pass against the
        # braced reality -- runs AFTER topology settles, BEFORE the gates
        # that decide whether this fix-pass is done.
        with _stage(log, f"brace_and_size[p{fix_pass}]", t0):
            brace_and_size(mesh, forest, unrouted, surface_tree, plate_z, cfg,
                           log=log, geom=geom)
        tree.assert_all_certified(forest)
        members, balls = live_member_lists(forest, unrouted, plate_z, cfg)
        log[f"forest_pass{fix_pass}"] = dict(
            n_members=len(members), n_balls=len(balls), n_unrouted=len(unrouted))
        # Verifier should-fix: surface n_unrouted top-level so regressions
        # vs the frozen baselines show up without digging.
        log["n_unrouted"] = len(unrouted)

        routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
        with _stage(log, f"verify[p{fix_pass}]", t0):
            report = verify.self_verify(mesh, islands, routed_contacts, members, carried, tip_r,
                                        cfg, plate_z, surface_tree=surface_tree, minima=minima,
                                        forest=forest, unrouted=unrouted,
                                        island_births=island_births, geom=geom, balls=balls)
        log[f"verify_pass{fix_pass}"] = dict(
            ok=report["ok"],
            n_uncovered=len(report["uncovered_islands"]),
            n_clearance_violations=len(report["clearance_violations"]),
            n_moot_transit_members=len(report["moot_transit_members"]),
            n_load_violations=len(report["load_violations"]),
            n_unsupported_minima=len(report["unsupported_minima"]),
            n_connectivity_violations=len(report["connectivity_violations"]),
            n_printability_violations=len(report["printability_violations"]),
            n_murray_violations=len(report["murray_violations"]),
            n_uncovered_island_births=len(report["uncovered_island_births"]),
            # Phase 5 (design-v4.md #10, amended 2026-07-22): MEASUREMENT
            # ONLY -- self_verify's default gate_on_removal=False means
            # this never affects `ok`. Per Craig's 2026-07-22 ruling
            # (design-v4.md #10 amendment, superseding the earlier
            # "BLOCKER gate" charter language), removal_report is
            # diagnostic/report-only and removability is the least
            # important optimization axis -- it must never block a
            # layout that's otherwise superior on structural/physics
            # quality or resin use, so gate_on_removal stays an inert
            # (unused-by-any-caller) toggle rather than something wired
            # to engine.run acceptance.
            n_removal_blocked_trees=len(report["removal_violations"]),
        )
        if report["ok"] or not report["uncovered_islands"]:
            break
        # Only add fix contacts on islands that are uncovered because
        # they lacked contacts -- islands whose contacts exist but proved
        # UNROUTABLE would just accumulate more unroutable contacts.
        unroutable_islands = {contacts[i]["island_id"] for i in unrouted}
        fixable = [u for u in report["uncovered_islands"]
                   if u["island_id"] not in unroutable_islands]
        if not fixable:
            break
        contacts = _add_fix_contacts(mesh, islands, contacts, fixable)

    with _stage(log, "finish", t0):
        volume = tree.volume_report(members, balls)
    log["t_total"] = round(time.time() - t0, 2)
    # SPEC-V5-PERF B4/P14 mechanical emission gate: a lever that can leave
    # verify.ok False (B4's capped fix loop) must not let the pipeline emit a
    # gate-failing layout. When perf_gate_emit_on_verify is set, refuse the run
    # (fatal) if verify failed; default OFF keeps today's behavior (report
    # returned, caller decides). Recorded in the log either way.
    log["gates_failed"] = bool(report is not None and not report["ok"])
    if getattr(cfg, "perf_gate_emit_on_verify", False) and log["gates_failed"]:
        raise RuntimeError(
            "GATES-FAILED: verify.ok is False and perf_gate_emit_on_verify is "
            "set -- refusing to emit a gate-failing layout (SPEC-V5-PERF B4/P14)")
    return dict(
        islands=islands, contacts=contacts, carried_area_mm2=carried, tip_r_mm=tip_r,
        forest=forest, members=members, balls=balls, unrouted=unrouted, minima=minima,
        plate_z=plate_z, verify=report, volume=volume, log=log,
    )


def _exterior_face_ids(mesh, geom):
    """Original face indices to draw a contact's owning face from (SPEC-V5-GEOM
    sec 2.4): exterior faces only when a surface-EXACT `geom` is supplied, so a
    minimum/birth contact never snaps its owning face (and thus its normal and
    island) to a triangle INTERIOR to the overlapping-assembly solid union.
    Fail-open: no oracle, or a degenerate all-interior classification, falls
    back to every face (the v4 behavior)."""
    if geom is not None and hasattr(geom, "exterior_face_mask"):
        ids = np.nonzero(geom.exterior_face_mask())[0]
        if len(ids):
            return ids
    return np.arange(len(mesh.faces))


def _ensure_minima_contacts(mesh, islands, contacts, minima, cfg, geom=None):
    """Ported verbatim from v3 engine.py (design-v4.md #4 orchestration
    step list): every island-birth minimum (perception.local_z_minima)
    gets a contact touching its birth region unless one already does. A
    contact merely NEAR the minimum does not count -- the satisfaction
    test is distance to the birth-region vertices, which only spans the
    first couple of layers of the island (Craig: what matters is how many
    layers pass before the island connects, not lateral proximity).

    Minima outside every overhang island (flanks steeper than the
    down-facing test -- exactly the class the island model misses) get a
    small synthetic island appended so load partition and coverage treat
    them like any other region. Mutates neither input list in place;
    returns (contacts, islands, n_added)."""
    from scipy.spatial import cKDTree
    if not minima:
        return contacts, islands, 0
    contacts = list(contacts)
    islands = list(islands)
    have = np.array([np.asarray(c["position"], float) for c in contacts]) \
        if contacts else np.zeros((0, 3))
    tree_have = cKDTree(have) if len(have) else None
    face_island = -np.ones(len(mesh.faces), dtype=np.int64)
    for iid, fids in enumerate(islands):
        face_island[fids] = iid
    ext_ids = _exterior_face_ids(mesh, geom)
    centers_tree = cKDTree(mesh.triangles_center[ext_ids])
    added = 0
    for m in minima:
        if tree_have is not None:
            d, _ = tree_have.query(m["region_pts"],
                                    workers=routing.query_workers(len(m["region_pts"])))
            # I4 (Craig's strict criterion, v4 p6): this used to skip on
            # cfg.minima_contact_radius_mm (1.0mm) -- but that field's own
            # docstring says it is "NOT a coverage radius... only absorbs
            # sampling gaps" between face-sampled contact points and
            # vertex-sampled birth regions. 1.0mm let a contact 20 real
            # layers away from a birth (Craig: "that's 20 layers... a lump
            # floating in the resin tank") silently count as already
            # covering it, so this pass would under-add. Use the same
            # conservative attach-reach as the new island-birth demand
            # pass (perception.ISLAND_ATTACH_REACH_MM, ~tip_r_min_mm) --
            # a plain module constant, not a config.py knob (see that
            # constant's own docstring for why: config.py belongs to a
            # parallel workstream on this branch).
            if float(np.min(d)) <= perception.ISLAND_ATTACH_REACH_MM:
                continue  # an existing contact already holds this birth island
        local = centers_tree.query_ball_point(m["position"], r=1.5)
        if not local:
            _, ln = centers_tree.query(m["position"])
            local = [int(ln)]
        near = [int(ext_ids[k]) for k in local]  # map exterior-local -> mesh face id
        best = min(near, key=lambda f: float(
            np.linalg.norm(mesh.triangles_center[f] - m["position"])))
        if face_island[best] >= 0:
            iid = int(face_island[best])
        else:
            fids = np.array(sorted(near), dtype=np.int64)
            islands.append(fids)
            iid = len(islands) - 1
        area = float(mesh.area_faces[islands[iid]].sum())
        contacts.append(dict(position=np.asarray(m["position"], float),
                             normal=np.asarray(m["normal"], float),
                             island_id=iid, area_mm2=area, minimum=True))
        added += 1
    return contacts, islands, added


# I2 (v4 p6, Craig's strict criterion): "distinct physical footprint"
# clustering radius for raw real-layer island births -- see
# _ensure_island_birth_contacts. Not a config.py knob (see
# perception.ISLAND_ATTACH_REACH_MM's docstring for why: implementation
# geometry constant, config.py belongs to a parallel workstream on this
# branch). Measured against the real Weeping Treant corpus (design
# scratchpad's island_contact_demand_result.json): bridges the
# surface-texture-scale gap between consecutive 0.05mm-layer birth events
# of the same raking/migrating thin feature into one physical footprint,
# without merging genuinely separate overhangs (kept well under
# cfg.target_spacing_mm).
#
# This is a 3D-EUCLIDEAN xy/z BOOKKEEPING radius -- "are these two raw
# birth records the same physical feature, or two different ones" -- not
# a coverage-correctness tolerance; it never decides whether a birth
# counts as covered (island_birth_report's derived z_tol_mm and
# perception.ISLAND_ATTACH_REACH_MM alone decide that). Deliberately NOT
# derived from cfg.layer_height_mm under design-v4.md section 11's
# amendment: this radius groups birth RECORDS regardless of how many real
# layers separate them, a different question from "is layer N close
# enough in time to layer N-1" (the z_tol question). It was tuned once
# against the 0.05mm-layer Treant corpus, though, so a future
# dynamic-layer-height pass should empirically re-check whether 0.6mm
# still groups sensibly at much finer local layer heights (more, closer-
# spaced raw births per unit height could plausibly want a smaller link
# radius to avoid over-merging distinct nearby features) -- not required
# for this branch, flagged for whoever lands that pass.
_ISLAND_BIRTH_LINK_R_MM = 0.6


def _cluster_island_births_3d(uncovered, link_r_mm):
    """Union-find over 3D euclidean distance <= link_r_mm -> distinct
    physical footprints. `uncovered` is a list of birth dicts (x, y, z, ...).
    Returns a list of index-lists into `uncovered`, ported from the
    design scratchpad's contact_demand.py measurement script (same
    algorithm, now production code)."""
    n = len(uncovered)
    pts = np.array([[b["x"], b["y"], b["z"]] for b in uncovered])
    parent = list(range(n))

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    def union(i, j):
        ri, rj = find(i), find(j)
        if ri != rj:
            parent[ri] = rj

    for i in range(n):
        if i + 1 >= n:
            continue
        d = np.linalg.norm(pts[i + 1:] - pts[i], axis=1)
        for k, dd in enumerate(d):
            if dd <= link_r_mm:
                union(i, i + 1 + k)

    groups = {}
    for i in range(n):
        groups.setdefault(find(i), []).append(i)
    return list(groups.values())


def _greedy_ladder_positions(cluster_idx, uncovered, attach_r_mm, z_tol_mm):
    """Greedy minimum-contact cover for one footprint cluster (I2's
    "ladder" for a raking/migrating cluster, 1 contact for an isolated
    birth): sort the cluster's raw births by z, then repeatedly take the
    lowest still-uncovered birth as a placement and drop every later birth
    within attach_r_mm (xy) AND z_tol_mm (z) of THAT placement's own
    position -- each placement is a real contact that will itself be
    checked against exactly this xy/z criterion by
    verify.island_birth_report, so the cover this loop computes is exactly
    the ladder needed to satisfy that gate. Not necessarily the global
    minimum cover (greedy), but a simple, defensible, honest-by-
    construction one -- ported from the design scratchpad's
    contact_demand.py measurement script. Returns the list of chosen birth
    dicts (subset of `uncovered`)."""
    items = sorted((uncovered[i] for i in cluster_idx), key=lambda b: b["z"])
    remaining = list(items)
    placements = []
    while remaining:
        anchor = remaining.pop(0)
        placements.append(anchor)
        keep = []
        for b in remaining:
            xy_d = float(np.hypot(b["x"] - anchor["x"], b["y"] - anchor["y"]))
            z_d = float(abs(b["z"] - anchor["z"]))
            if xy_d <= attach_r_mm and z_d <= z_tol_mm:
                continue  # covered by this placement
            keep.append(b)
        remaining = keep
    return placements


def _ensure_island_birth_contacts(mesh, islands, contacts, island_births, cfg, geom=None):
    """I2: contact demand from perception.island_births (I1) -- the
    ground-truth slice-based birth detection, independent of
    local_z_minima's vertex-graph model. For every birth not already held
    by an existing contact under Craig's strict at-birth criterion (within
    cfg.minima_connect_layers real print layers of z, landing on/within
    perception.ISLAND_ATTACH_REACH_MM of the birth footprint's own
    boundary -- NOT the old 1mm minima_contact_radius_mm, see I4/
    _ensure_minima_contacts), clusters the raw uncovered births into
    distinct physical footprints (_ISLAND_BIRTH_LINK_R_MM) and demands the
    minimum "ladder" of contacts (_greedy_ladder_positions) needed to
    satisfy the gate, placed ON the footprint (xy-exact at the birth
    centroid, z at the birth layer).

    Runs AFTER _ensure_minima_contacts so births that detector already
    covers (the common case: a sharp toe apex is both a vertex-graph
    local minimum AND a slice-birth) produce no duplicate demand -- this
    pass only fills the GAP: raking/migrating features and any other
    birth event the vertex-graph model's per-vertex 1-ring can't express
    (see perception.island_births' module docstring).

    Mirrors _ensure_minima_contacts' island bookkeeping: a birth whose
    footprint isn't already inside a known overhang island gets a small
    synthetic island appended (load partition / coverage_report then
    treat it like any other region). Unlike _ensure_minima_contacts, a
    newly-created synthetic island's owning face IS marked in the local
    face_island lookup immediately (a strict local improvement, not a
    behavior change to the sibling function this is ported alongside) --
    ladder placements from the same cluster are likely to share a nearest
    face, and this avoids spawning one redundant synthetic island per
    ladder rung for what is physically one small footprint.

    Mutates neither input list; returns (contacts, islands, n_added)."""
    if not island_births:
        return contacts, islands, 0
    from scipy.spatial import cKDTree
    contacts = list(contacts)
    islands = list(islands)

    z_tol_mm = cfg.minima_connect_layers * cfg.layer_height_mm
    attach_r = perception.ISLAND_ATTACH_REACH_MM

    have = np.array([np.asarray(c["position"], float) for c in contacts]) \
        if contacts else np.zeros((0, 3))

    def already_covered(b):
        if len(have) == 0:
            return False
        z_ok = np.abs(have[:, 2] - b["z"]) <= z_tol_mm
        if not z_ok.any():
            return False
        for cx, cy, _cz in have[z_ok]:
            ok, _d = perception.island_birth_xy_ok(float(cx), float(cy), b["loop"], attach_r)
            if ok:
                return True
        return False

    uncovered = [b for b in island_births if not already_covered(b)]
    if not uncovered:
        return contacts, islands, 0

    clusters = _cluster_island_births_3d(uncovered, _ISLAND_BIRTH_LINK_R_MM)

    face_island = -np.ones(len(mesh.faces), dtype=np.int64)
    for iid, fids in enumerate(islands):
        face_island[fids] = iid
    ext_ids = _exterior_face_ids(mesh, geom)
    centers_tree = cKDTree(mesh.triangles_center[ext_ids])

    added = 0
    for cluster_idx in clusters:
        for b in _greedy_ladder_positions(cluster_idx, uncovered, attach_r, z_tol_mm):
            pos = np.array([b["x"], b["y"], b["z"]], float)
            local = centers_tree.query_ball_point(pos, r=1.5)
            if not local:
                _, ln = centers_tree.query(pos)
                local = [int(ln)]
            near = [int(ext_ids[k]) for k in local]  # exterior-local -> mesh face id
            best = min(near, key=lambda f: float(
                np.linalg.norm(mesh.triangles_center[f] - pos)))
            normal = np.asarray(mesh.face_normals[best], float)
            if face_island[best] >= 0:
                iid = int(face_island[best])
            else:
                fids = np.array(sorted(near), dtype=np.int64)
                islands.append(fids)
                iid = len(islands) - 1
                face_island[fids] = iid
            area = float(mesh.area_faces[islands[iid]].sum())
            contacts.append(dict(position=pos, normal=normal, island_id=iid,
                                 area_mm2=area, minimum=True, island_birth=True))
            added += 1
    return contacts, islands, added


def _add_fix_contacts(mesh, islands, contacts, uncovered):
    """Ported verbatim from v3 engine.py: one extra contact at the
    worst-covered point of each still-uncovered island."""
    centers = mesh.triangles_center
    normals = mesh.face_normals
    by_island = {}
    for c in contacts:
        by_island.setdefault(c["island_id"], []).append(np.asarray(c["position"]))

    extra = list(contacts)
    for u in uncovered:
        iid = u["island_id"]
        face_ids = islands[iid]
        pts = centers[face_ids]
        existing = np.array(by_island.get(iid, []))
        if len(existing):
            from scipy.spatial import cKDTree
            tree_existing = cKDTree(existing)
            dist, _ = tree_existing.query(pts, workers=routing.query_workers(len(pts)))
        else:
            dist = np.full(len(pts), np.inf)
        worst = int(np.argmax(dist))
        extra.append(dict(position=pts[worst], normal=normals[face_ids[worst]],
                           island_id=iid, area_mm2=u["n_faces"]))
    return extra
