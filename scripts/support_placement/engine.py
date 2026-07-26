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
import copy
import dataclasses
import hashlib
import json
import sys
import threading
import time
import numpy as np

from . import perception, placement, routing, verify, joints, cco, tree, rewire
from . import transient, bracing, phase8_driver
from . import topology, coverage, prepared_validation
from .geometry_exact import closest_on_tri
from .geometry_kernel import KernelSurface


PREPARED_STATE_SCHEMA = "support-placement/prepare"
PREPARED_STATE_SCHEMA_VERSION = 2


def mesh_fingerprint(mesh):
    """Deterministic identity of staged mesh geometry (stdlib SHA-256)."""
    return topology.mesh_fingerprint(mesh)


def _canonical_config_value(value):
    if isinstance(value, dict):
        return {
            str(k): _canonical_config_value(v)
            for k, v in sorted(value.items(), key=lambda kv: str(kv[0]))
        }
    if isinstance(value, (tuple, list)):
        return [_canonical_config_value(v) for v in value]
    if isinstance(value, np.generic):
        return value.item()
    return value


def canonical_config_json(cfg):
    """Stable semantic SupportConfig serialization; no repr/default coupling."""
    if not dataclasses.is_dataclass(cfg):
        raise TypeError("prepared state config must be a dataclass instance")
    payload = _canonical_config_value(dataclasses.asdict(cfg))
    return json.dumps(
        payload, sort_keys=True, separators=(",", ":"), allow_nan=False)


def config_fingerprint(cfg):
    return hashlib.sha256(canonical_config_json(cfg).encode("utf-8")).hexdigest()


def _prepared_value_exact_equal(left, right):
    """Exact recursive equality for caller-supplied prepared domains."""
    if left is right:
        return True
    if isinstance(left, np.ndarray) or isinstance(right, np.ndarray):
        return (
            isinstance(left, np.ndarray)
            and isinstance(right, np.ndarray)
            and left.dtype == right.dtype
            and left.shape == right.shape
            and np.array_equal(left, right))
    if isinstance(left, dict) or isinstance(right, dict):
        return (
            isinstance(left, dict)
            and isinstance(right, dict)
            and left.keys() == right.keys()
            and all(_prepared_value_exact_equal(left[k], right[k])
                    for k in left))
    if isinstance(left, (list, tuple)) or isinstance(right, (list, tuple)):
        return (
            isinstance(left, (list, tuple))
            and isinstance(right, (list, tuple))
            and len(left) == len(right)
            and all(_prepared_value_exact_equal(a, b)
                    for a, b in zip(left, right)))
    return type(left) is type(right) and left == right


def validate_prepared_state(pre, mesh=None, cfg=None):
    """Fail clearly on checkpoints created before the A2 prepared-pool schema."""
    if (not isinstance(pre, dict)
            or pre.get("schema") != PREPARED_STATE_SCHEMA
            or pre.get("version") != PREPARED_STATE_SCHEMA_VERSION):
        raise ValueError(
            "prepared state schema/version is missing or unsupported; rerun prepare")
    if "contact_candidate_pool" not in pre:
        raise ValueError(
            "prepared state has no A2 contact candidate pool; rerun prepare")
    if "face_topology" not in pre or "coverage_samples" not in pre:
        raise ValueError(
            "prepared state has no A3 topology/coverage sidecars; rerun prepare")
    if not isinstance(pre.get("mesh_fingerprint"), str):
        raise ValueError(
            "prepared state has no mesh fingerprint; rerun prepare")
    if not isinstance(pre.get("config_fingerprint"), str):
        raise ValueError(
            "prepared state has no config fingerprint; rerun prepare")
    if mesh is not None and pre["mesh_fingerprint"] != mesh_fingerprint(mesh):
        raise ValueError(
            "prepared state mesh fingerprint mismatch; rerun prepare")
    if cfg is not None and pre["config_fingerprint"] != config_fingerprint(cfg):
        raise ValueError(
            "prepared state config fingerprint mismatch; rerun prepare")
    placement.validate_candidate_pool(
        pre["contact_candidate_pool"], mesh=mesh,
        islands=pre.get("islands"))
    topology.validate_face_topology(pre["face_topology"], mesh=mesh)
    coverage.validate_coverage_samples(
        pre["coverage_samples"], mesh=mesh, islands=pre.get("islands"))
    return pre


_require_prepared_state = validate_prepared_state


def _validated_prepared_token(pre, mesh, cfg, token=None):
    """Return a run-local token, deep-validating once when not authorized."""
    if prepared_validation._authorizes_prepared(token, pre, mesh, cfg):
        return token
    validate_prepared_state(pre, mesh=mesh, cfg=cfg)
    return prepared_validation._issue_prepared_validation_token(
        pre, mesh, cfg)


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
def _ticker(label, get_n, total, interval=5.0, unit="contacts"):
    """SPEC-V5-PERF sec 4 (owner-mandated, task #17): a daemon thread that
    prints ``[v5] .. {label} {n}/{total} {unit}`` to stderr every `interval`
    seconds while the wrapped stage runs, so a full-scale construct is never a
    heartbeat-only black box. ALL timing lives in the thread's
    ``Event.wait(interval)``; the compute path only advances the counter
    get_n() reads (no kernel calls, no allocation), so output is byte-identical
    with the ticker on or off. Event-signaled shutdown + join in finally
    (never leaves a dangling thread, even on exception -- risk P7)."""
    stop = threading.Event()

    def _run():
        while not stop.wait(interval):
            print(f"[v5] .. {label} {get_n()}/{total} {unit}",
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
    ``schema``, ``version``, islands, minima, island_births, contacts, plate_z,
    sample_points, sample_delicacy, the compact schema-versioned
    ``contact_candidate_pool``, exact-bit ``face_topology``, area-weighted
    ``coverage_samples``, and log. Extracted from run() so Treant-scale
    runs can checkpoint between bounded foreground calls; run() remains the
    canonical composition. Staged consumers reject older checkpoints with an
    explicit "rerun prepare" error.

    `geom` (v5, SPEC-V5-GEOM sec 2.3, optional): when supplied (a
    surface-EXACT oracle, NOT stored in the returned dict -- it is only
    used here to build sample_points/sample_delicacy), the model-anchor
    candidate pool becomes deterministic exterior-face centroids instead
    of delicacy_field's seeded sample cloud (perception.exterior_anchor_
    pool). Callers that omit it (unit tests / legacy) get the v4 pool
    verbatim, unchanged."""
    log = log if log is not None else {}
    t0 = time.time()

    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    topology_stats = {}
    face_topology = topology.build_face_topology(
        mesh, cfg=cfg, stats_out=topology_stats)
    log["face_topology"] = topology_stats
    islands = perception.overhang_islands(
        mesh, cfg, plate_z, geom=geom, face_topology=face_topology)
    log["islands"] = len(islands)
    log["t_islands"] = round(time.time() - t0, 2)

    # v4 p6 (I1, Craig's strict criterion): ground-truth slice-based
    # island-birth detection at the real print layer height, computed ONCE
    # here and carried in `pre` for both contact demand (below) and the
    # honest verify gate (verify.island_birth_report) -- see
    # perception.island_births' module-level comment for why this is a
    # necessary cross-check on local_z_minima below, not a duplicate of it.
    #
    # Ordered BEFORE local_z_minima (Craig's 2026-07-24 supportive-
    # connectivity ruling) so the single slicing pass also feeds that
    # detector's cfg.minima_require_island_birth filter -- the model is
    # sliced once per run, not twice.
    island_births = perception.island_births(mesh, cfg)
    log["island_births"] = len(island_births)
    log["t_island_births"] = round(time.time() - t0, 2)

    # v3.6 (Craig, top priority): island-birth minima. Detected before
    # sampling so every minimum the overhang-angle model missed (steep
    # flanks) still ends up with a contact AT its birth region -- but
    # filtered by the two independent passes in local_z_minima's docstring:
    # (A) cfg.minima_drop_interior_faces drops inner-wall phantoms (needs
    # `geom`), (B) cfg.minima_require_island_birth drops exterior surface
    # that already rests on solid connected below.
    minima = perception.local_z_minima(mesh, cfg, births=island_births,
                                       geom=geom)
    log["minima"] = len(minima)
    log["t_minima"] = round(time.time() - t0, 2)

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
    # A2 prepared regular-contact pool: one picklable source of truth reused by
    # initial placement, coverage repair, Phase 8, and final verification.
    # Virtual refinement is derived from the UNCHANGED coverage contract, not
    # the optional B3 seed-spacing quality lever.
    pool_stats = {}
    # Install the live stats dict before candidate_pool runs so a refusal still
    # leaves its complete planned counts and winning-cap reasons in caller log.
    log["contact_candidate_pool"] = pool_stats
    contact_candidate_pool = placement.candidate_pool(
        mesh, islands, cfg, stats_out=pool_stats)
    n_pool_islands = len(islands)
    contacts = placement.sample_candidates(
        mesh, islands, seed_cfg,
        candidate_pool_prepared=contact_candidate_pool)
    log["contacts_initial"] = len(contacts)
    contacts, islands, n_min_added = _ensure_minima_contacts(
        mesh, islands, contacts, minima, cfg, geom=geom,
        face_topology=face_topology, _face_topology_authorized=True)
    log["minima_contacts_added"] = n_min_added

    # I2: demand one contact (or a short "ladder" for a raking/migrating
    # cluster) per island-birth footprint not already satisfied by an
    # existing contact under the strict at-birth criterion. Runs AFTER
    # the minima pass so births local_z_minima already covers produce no
    # duplicate demand (dedup is by the strict criterion here, not the
    # old 1mm minima_contact_radius_mm -- see _ensure_island_birth_contacts).
    contacts, islands, n_birth_added = _ensure_island_birth_contacts(
        mesh, islands, contacts, island_births, cfg, geom=geom,
        face_topology=face_topology, _face_topology_authorized=True)
    log["island_birth_contacts_added"] = n_birth_added
    if len(islands) > n_pool_islands:
        # Synthetic minimum/birth islands remain part of shared coverage and
        # later A4 repair. Extend the same compact pool under the COMBINED
        # aggregate preflight; candidate_pool refuses before generating the new
        # virtual cells or allocating the concatenated arrays.
        contact_candidate_pool = placement.candidate_pool(
            mesh, islands[n_pool_islands:], cfg,
            island_id_offset=n_pool_islands, stats_out=pool_stats,
            existing_pool=contact_candidate_pool)
    coverage_stats = {}
    log["coverage_samples"] = coverage_stats
    coverage_samples = coverage.build_coverage_samples(
        mesh, islands, cfg, stats_out=coverage_stats)
    if len(islands) > n_pool_islands:
        # Synthetic islands already contain immutable specialized
        # minimum/birth contacts. Add only the deterministic
        # minimum-cardinality regular pool rows needed to cover every prepared
        # A3 sample, while retaining the real target-spacing exclusion around
        # every specialized and selected seat.
        selection_stats = {}
        log["synthetic_regular_coverage_selection"] = selection_stats
        selected_rows = _select_synthetic_coverage_rows(
            contact_candidate_pool, coverage_samples, face_topology,
            contacts, n_pool_islands, len(islands), cfg,
            stats_out=selection_stats)
        synthetic_regular_contacts = [
            _regular_contact_from_pool_row(
                contact_candidate_pool, row, islands, mesh)
            for row in selected_rows
        ]
        contacts.extend(synthetic_regular_contacts)
        log["synthetic_regular_contacts_added"] = len(
            synthetic_regular_contacts)
    else:
        log["synthetic_regular_contacts_added"] = 0
        log["synthetic_regular_coverage_selection"] = dict(
            selected_count=0, selected_rows=(), components=(),
            searched_cardinality=0, pairs_examined=0,
            max_matrix_entries=0, exact=True)
    sample_points, sample_delicacy = perception.exterior_anchor_pool(mesh, geom, cfg)
    log["t_delicacy"] = round(time.time() - t0, 2)
    return dict(schema=PREPARED_STATE_SCHEMA,
                version=PREPARED_STATE_SCHEMA_VERSION,
                mesh_fingerprint=mesh_fingerprint(mesh),
                config_fingerprint=config_fingerprint(cfg),
                islands=islands, minima=minima, island_births=island_births,
                contacts=contacts, plate_z=plate_z, sample_points=sample_points,
                sample_delicacy=sample_delicacy,
                contact_candidate_pool=contact_candidate_pool,
                face_topology=face_topology,
                coverage_samples=coverage_samples, log=log)


def construct(mesh, pre, surface_tree, cfg, log=None, limit_contacts=None, geom=None,
              progress=None, contacts_override=None,
              _prepared_validation_token=None):
    """Stage 2: load partition + tip sizing + cco.build (NO rewiring --
    the caller owns rewire.optimize so its wall budget is its own).
    `limit_contacts` truncates the contact list for profiling only.
    `geom` (v5 exact oracle, SPEC sec 2.1) drives cco's per-candidate
    clearance revalidation when supplied; without it, the legacy cKDTree
    `surface_tree` does (tests/staged-legacy). Returns (forest, unrouted,
    carried, tip_r, contacts)."""
    _validated_prepared_token(
        pre, mesh, cfg, _prepared_validation_token)
    log = log if log is not None else {}
    oracle = geom if geom is not None else surface_tree
    source_contacts = (
        pre["contacts"] if contacts_override is None else contacts_override)
    contacts = (
        source_contacts[:limit_contacts] if limit_contacts
        else source_contacts)
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


def relocate(mesh, forest, unrouted, contacts, carried, tip_r, plate_z, cfg,
             geom=None, log=None, census_authority=frozenset(), pass_idx=0,
             progress=None, islands=None, island_births=None, carry_ledger=None,
             candidate_pool_prepared=None, prepared_state=None,
             failed_candidate_pool_rows=(),
             _prepared_validation_token=None):
    """Phase-8 RELOCATION STAGE (SPEC-P8-RELOCATE §3.0), wired post-brace /
    pre-final-verify. Constructs the frozen defect snapshot + the SM-STUB /
    SM-STRUT / SM-BALL sites from the engine's OWN gate output, runs the state
    machines in the normative order, and splices accepted relocations back into
    a clone deterministically (`phase8_driver.run_phase8`). Phase 8 plus
    structural resize repeat to one combined fixed point, then commit the clone
    while preserving the caller's Forest/list identities. An exception or cap
    exhaustion leaves the caller's forest, contacts, and carry ledger untouched.
    Returns the `Phase8Report` dict this call wrote to `log['relocate']`.

    IDEMPOTENT BY CONSTRUCTION: a forest with zero non-exempt transits / zero
    breaches builds an empty snapshot, enters no ladder, and mutates nothing, so
    re-invoking the stage over already-fixed geometry is a byte-exact no-op
    (§6.10). Requires an exact `geom` oracle; a legacy cKDTree caller is a no-op
    (the transit/ball predicates need the exact kernel)."""
    log = log if log is not None else {}
    if prepared_state is not None:
        _prepared_validation_token = _validated_prepared_token(
            prepared_state, mesh, cfg, _prepared_validation_token)
        prepared_islands = prepared_state["islands"]
        prepared_births = prepared_state["island_births"]
        if (islands is not None
                and not _prepared_value_exact_equal(
                    islands, prepared_islands)):
            raise ValueError(
                "Phase-8 supplied islands mismatch prepared-state authority")
        if (island_births is not None
                and not _prepared_value_exact_equal(
                    island_births, prepared_births)):
            raise ValueError(
                "Phase-8 supplied island_births mismatch prepared-state "
                "authority")
        islands = prepared_islands
        island_births = prepared_births
    if geom is None or not hasattr(geom, "penetration_intervals"):
        log["relocate"] = log[f"relocate_p{pass_idx}"] = dict(skipped="no exact geom oracle")
        return log["relocate"]
    r2_can_run = (
        islands is not None and getattr(cfg, "p8_reseat_enable", True))
    if r2_can_run:
        if prepared_state is None:
            raise ValueError(
                "Phase-8 R2 requires prepared state; rerun prepare and pass "
                "prepared_state")
        authoritative_pool = prepared_state["contact_candidate_pool"]
        if (candidate_pool_prepared is not None
                and candidate_pool_prepared is not authoritative_pool):
            raise ValueError(
                "Phase-8 R2 candidate pool is not the prepared-state authority")
        candidate_pool_prepared = authoritative_pool
    max_combined_rounds = int(getattr(
        cfg, "p8_resize_fixed_point_max_rounds", 4))
    if max_combined_rounds <= 0:
        raise ValueError("p8_resize_fixed_point_max_rounds must be positive")
    working_forest = copy.deepcopy(forest)
    working_contacts = copy.deepcopy(contacts)
    working_ledger = copy.deepcopy(
        carry_ledger) if carry_ledger is not None else {}
    transaction_log = {}
    combined_trace = []
    reports = []
    transaction_committed = False
    frozen_snapshot = phase8_driver.load_treant_frozen_snapshot()
    try:
        for combined_round in range(max_combined_rounds):
            members, balls = live_member_lists(
                working_forest, unrouted, plate_z, cfg)
            member_list_fn = lambda: live_member_lists(  # noqa: E731
                working_forest, unrouted, plate_z, cfg)
            report = phase8_driver.run_phase8(
                working_forest, members, balls, working_contacts,
                carried, tip_r, plate_z, geom, cfg, mesh,
                unrouted=unrouted, census_authority=census_authority,
                log=transaction_log, member_list_fn=member_list_fn,
                frozen_snapshot=frozen_snapshot, progress=progress,
                islands=islands, island_births=island_births,
                candidate_pool_prepared=candidate_pool_prepared,
                coverage_samples=(
                    None if prepared_state is None
                    else prepared_state["coverage_samples"]),
                face_topology=(
                    None if prepared_state is None
                    else prepared_state["face_topology"]),
                failed_candidate_pool_rows=failed_candidate_pool_rows,
                carry_ledger=working_ledger,
                _prepared_validation_token=_prepared_validation_token)
            reports.append(report)
            resize = transient.resize_forest(
                working_forest, plate_z, cfg)
            resized = list(resize.get("resized", ()))
            resize_violations = working_forest.revalidate(geom, cfg)
            # A stable round may still report an honest, formally-unresolvable
            # clearance defect. What must not escape is a resize mutation that
            # Phase 8 has not yet seen: any resize forces another combined
            # round; the following no-resize Phase-8 fixed point is authoritative.
            stable = bool(report.converged and not resized)
            combined_trace.append(dict(
                combined_round=int(combined_round),
                p8_entered=bool(report.entered_ladder),
                p8_converged=bool(report.converged),
                n_resized=int(len(resized)),
                n_resize_clearance_violations=int(
                    len(resize_violations)),
                stable=stable))
            if stable:
                transaction_committed = True
                break
        if not transaction_committed:
            raise RuntimeError(
                "Phase-8 + structural-resize combined fixed point exhausted "
                f"{max_combined_rounds} round(s); original forest was not "
                "modified")
        tree.assert_all_certified(working_forest)
    except Exception as exc:
        log["relocate_transaction"] = dict(
            committed=False, combined_rounds=len(combined_trace),
            max_combined_rounds=max_combined_rounds,
            trace=combined_trace, error=f"{type(exc).__name__}: {exc}")
        raise

    # Commit the clone as one stage-wide transaction. Preserve the caller's
    # Forest and contacts-list identities; only now expose the accepted state.
    forest.__dict__.clear()
    forest.__dict__.update(working_forest.__dict__)
    for member_idx, member in enumerate(forest.members):
        object.__setattr__(member, "_forest", forest)
        object.__setattr__(member, "_idx", member_idx)
    contacts[:] = working_contacts
    if carry_ledger is not None:
        carry_ledger.clear()
        carry_ledger.update(working_ledger)
    log.update(transaction_log)
    log["relocate_transaction"] = dict(
        committed=True, combined_rounds=len(combined_trace),
        max_combined_rounds=max_combined_rounds,
        trace=combined_trace)
    first_report = reports[0]
    last_report = reports[-1]
    rep = dict(
        entered_ladder=any(r.entered_ladder for r in reports),
        dt_s=round(sum(float(r.dt_s) for r in reports), 3),
        n_stub_sites=sum(r.n_stub_sites for r in reports),
        n_strut_sites=sum(r.n_strut_sites for r in reports),
        n_ball_sites=sum(r.n_ball_sites for r in reports),
        stub_resolved=sum(r.stub_resolved for r in reports),
        stub_unresolved=last_report.stub_unresolved,
        qreal_resolved=sum(r.qreal_resolved for r in reports),
        qreal_unresolved=last_report.qreal_unresolved,
        strut_resolved=sum(r.strut_resolved for r in reports),
        strut_unresolved=last_report.strut_unresolved,
        ball_resolved=sum(r.ball_resolved for r in reports),
        ball_unresolved=last_report.ball_unresolved,
        n_reverted=sum(len(r.reverted_splices) for r in reports),
        n_pockets=sum(len(r.pocket_labels) for r in reports),
        sweeps=sum(r.sweeps for r in reports), converged=True,
        # residual round 2: the OUTER fixed-point round trace. `converged` is now
        # "a FRESH-STATE round committed nothing", so committed_by_round ending
        # in 0 is the §6.10 strict-no-op evidence.
        rounds=sum(r.rounds for r in reports),
        committed_by_round=[
            value for r in reports for value in r.committed_by_round],
        committed_by_sweep=[
            value for r in reports for value in r.committed_by_sweep],
        target_source=first_report.target_source,
        sweep_tail_monotone=all(
            r.sweep_tail_monotone for r in reports),
        n_banned=sum(r.n_banned for r in reports),
        n_moot=len(last_report.moot_member_ids),
        n_ledger=sum(len(r.provenance_ledger) for r in reports),
        # D6 commit-invariant evidence: the global defect population at the top of
        # each sweep (monotone by construction) + the honest-unresolved retirements.
        defects_by_sweep=[
            value for r in reports for value in r.defects_by_sweep],
        n_retired=sum(len(r.retired_sites) for r in reports),
        retired_sites=[
            list(row) for r in reports for row in r.retired_sites],
        # ruling (5): escalation-rung evidence + the persistent honest-failure
        # register. `unresolvable_ledger` rows are
        # (class, site_id, last_rung, reason, first_sweep, last_sweep,
        #  n_attempts, measure_mm) -- see phase8_driver._LEDGER_FIELDS.
        n_reseats=sum(len(r.reseats) for r in reports),
        reseats=[list(row) for r in reports for row in r.reseats],
        n_junction_reseats=sum(
            len(r.junction_reseats) for r in reports),
        junction_reseats=[
            list(row) for r in reports for row in r.junction_reseats],
        n_unresolvable=len(last_report.unresolvable_ledger),
        unresolvable_ledger=[
            list(row) for row in last_report.unresolvable_ledger],
        snapshot=first_report.snapshot_summary,
        combined_rounds=len(combined_trace),
        combined_trace=combined_trace)
    # defect-5: DISTINCT per-fix-pass ledger keys (relocate_p0, relocate_p1, ...)
    # so a later fix-pass's report never overwrites an earlier one; `relocate`
    # keeps the latest for back-compat.
    log["relocate"] = rep
    log[f"relocate_p{pass_idx}"] = rep
    # ruling (5): the persistent formally-unresolvable register is surfaced at the
    # TOP of the log, not only inside the per-pass relocate block -- it is the
    # honest-failure statement for the whole run and must be readable without
    # knowing which pass last touched it.
    log["p8_unresolvable"] = rep["unresolvable_ledger"]
    log["p8_n_unresolvable"] = rep["n_unresolvable"]
    if rep["entered_ladder"]:
        print(f"[v5] .. relocate sites stub={rep['n_stub_sites']} "
              f"strut={rep['n_strut_sites']} ball={rep['n_ball_sites']} -> "
              f"resolved stub={rep['stub_resolved']} strut={rep['strut_resolved']} "
              f"ball={rep['ball_resolved']} (reverted {rep['n_reverted']}) "
              f"reseat={rep['n_reseats']} d5={rep['n_junction_reseats']} "
              f"unresolvable={rep['n_unresolvable']} "
              f"sweeps={rep['sweeps']} rounds={rep['rounds']} "
              f"combined={rep['combined_rounds']} "
              f"commits/round={rep['committed_by_round']} "
              f"converged={rep['converged']}",
              file=sys.stderr, flush=True)
    return rep


def _unrouted_specialized_contact_report(contacts, forest, unrouted):
    """Return specialized demands absent from the final routed forest graph."""
    explicitly_unrouted = {int(i) for i in unrouted}
    routed = set()
    if forest is not None:
        for contact_idx, member_idx in forest.terminal_of.items():
            contact_idx = int(contact_idx)
            member_idx = int(member_idx)
            if (contact_idx in explicitly_unrouted
                    or member_idx in forest.dead
                    or member_idx in forest.quarantined
                    or not (0 <= member_idx < len(forest.members))):
                continue
            member = forest.members[member_idx]
            if (member.kind == "stub"
                    and tuple(member.owner) == ("contact", contact_idx)):
                routed.add(contact_idx)
    return [
        dict(contact_idx=int(contact_idx), island_id=int(
            contact.get("island_id", -1)))
        for contact_idx, contact in enumerate(contacts)
        if (contact.get("minimum") or contact.get("island_birth"))
        and contact_idx not in routed
    ]


def _apply_unrouted_specialized_gate(report, contacts, forest, unrouted):
    """Surface the final specialized-routing hard gate on any verify report."""
    violations = _unrouted_specialized_contact_report(
        contacts, forest, unrouted)
    report["unrouted_specialized_contacts"] = violations
    if violations:
        report["ok"] = False
    return violations


def finish(mesh, pre, forest, unrouted, contacts, carried, tip_r,
           surface_tree, cfg, log=None, geom=None,
           _prepared_validation_token=None):
    """Stage 3b: verify gates + volume report on the final forest."""
    _prepared_validation_token = _validated_prepared_token(
        pre, mesh, cfg, _prepared_validation_token)
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
                                 geom=geom, balls=balls,
                                 contact_candidate_pool=pre["contact_candidate_pool"],
                                 provenance_contacts=contacts,
                                 coverage_samples=pre["coverage_samples"],
                                 face_topology=pre["face_topology"],
                                 _prepared_validation_token=(
                                     _prepared_validation_token))
    _apply_unrouted_specialized_gate(
        report, contacts, forest, unrouted)
    volume = tree.volume_report(members, balls)
    log["n_unrouted"] = len(unrouted)
    return dict(members=members, balls=balls, verify=report, volume=volume,
                plate_z=plate_z, log=log)


def _canonical_empty_geometry_state(
        mesh, pre, contacts, cfg, geom=None, surface_tree=None,
        _prepared_validation_token=None):
    """One honest representation for zero emitted support geometry.

    Regular demands are removed before this helper is called. Specialized
    demands remain explicit, unrouted hard failures with zero carried load;
    A3/minimum/birth verification still runs against the empty routed set.
    """
    contacts = [dict(contact) for contact in contacts]
    specialized_unrouted = list(range(len(contacts)))
    carried = np.zeros(len(contacts), dtype=np.float64)
    tip_r = np.zeros(len(contacts), dtype=np.float64)
    forest = tree.Forest()
    members, balls = [], []
    assessment_out = []
    report = verify.self_verify(
        mesh, pre["islands"], [], members, carried, tip_r, cfg,
        pre["plate_z"], surface_tree=surface_tree, minima=pre["minima"],
        forest=forest, unrouted=specialized_unrouted,
        island_births=pre["island_births"], geom=geom, balls=balls,
        contact_candidate_pool=pre["contact_candidate_pool"],
        provenance_contacts=contacts,
        coverage_samples=pre["coverage_samples"],
        face_topology=pre["face_topology"],
        coverage_assessment_out=assessment_out,
        _prepared_validation_token=_prepared_validation_token)
    _apply_unrouted_specialized_gate(
        report, contacts, forest, specialized_unrouted)
    return dict(
        forest=forest, members=members, balls=balls, report=report,
        carried=carried, tip_r=tip_r, unrouted=specialized_unrouted,
        contacts=contacts,
        coverage_assessment=assessment_out[0])


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
    # One deep boundary proof per run. Every later stage receives this
    # unpicklable, identity-bound capability instead of rescanning A2/A3's
    # potentially multi-million-row immutable sidecars.
    validate_prepared_state(pre, mesh=mesh, cfg=cfg)
    _prepared_validation_token = (
        prepared_validation._issue_prepared_validation_token(
            pre, mesh, cfg))
    islands, minima, contacts = pre["islands"], pre["minima"], pre["contacts"]
    island_births = pre["island_births"]
    plate_z = pre["plate_z"]

    if not contacts:
        state = _canonical_empty_geometry_state(
            mesh, pre, [], cfg, geom=geom,
            _prepared_validation_token=_prepared_validation_token)
        volume = tree.volume_report([], [])
        log["n_unrouted"] = 0
        log["gates_failed"] = not state["report"]["ok"]
        log["t_total"] = round(time.time() - t0, 2)
        return dict(
            islands=islands, contacts=state["contacts"],
            carried_area_mm2=state["carried"], tip_r_mm=state["tip_r"],
            forest=state["forest"], members=[], balls=[],
            unrouted=state["unrouted"], minima=minima, plate_z=plate_z,
            verify=state["report"], volume=volume,
            island_births=pre["island_births"],
            coverage_failed_candidate_pool_rows=(), log=log)

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
    max_repair_attempts = int(cfg.coverage_repair_max_attempts)
    if max_repair_attempts < 0:
        raise ValueError("coverage_repair_max_attempts must be >= 0")
    failed_rows = set()
    repair_ledger = []
    repair_attempts = 0
    trial_row = None
    accepted = None
    build_pass = 0
    # A baseline build plus at most one full build per A4 attempt. Baseline
    # regular contacts that prove unroutable are removed and rebuilt before
    # coverage is judged; each such normalization strictly shrinks contacts.
    while True:
        fix_pass = build_pass
        build_pass += 1
        with _stage(log, f"construct[p{fix_pass}]", t0):
            _prog = _ProgressCell()
            with _ticker(f"construct[p{fix_pass}]", lambda: _prog.n, len(contacts)):
                forest, unrouted, carried, tip_r, contacts = construct(
                    mesh, pre, surface_tree, cfg, log=log, geom=geom,
                    progress=_prog, contacts_override=contacts,
                    _prepared_validation_token=_prepared_validation_token)
        regular_unrouted = []
        specialized_unrouted = []
        for contact_idx in sorted(int(i) for i in unrouted):
            contact = contacts[contact_idx]
            if contact.get("minimum") or contact.get("island_birth"):
                specialized_unrouted.append(contact_idx)
            else:
                regular_unrouted.append(contact_idx)
        if regular_unrouted:
            rows = sorted({
                int(contacts[i]["candidate_pool_row"])
                for i in regular_unrouted
                if "candidate_pool_row" in contacts[i]})
            for row in rows:
                failed_rows.add(row)
                repair_ledger.append(dict(
                    attempt=int(repair_attempts), candidate_pool_row=row,
                    reason="unrouted", accepted=False))
            if trial_row is not None:
                repair_attempts += 1
                # A candidate trial is one build. Any unrouted regular demand
                # rejects it before topology mutation and restores the accepted
                # normalized authority.
                contacts = [dict(c) for c in accepted["contacts"]]
                trial_row = None
                if repair_attempts >= max_repair_attempts:
                    break
                occupied_rows = {
                    int(c["candidate_pool_row"]) for c in contacts
                    if not (c.get("minimum") or c.get("island_birth"))
                    and "candidate_pool_row" in c}
                rank_stats = {}
                next_row = _rank_coverage_repair_rows(
                    pre, accepted["coverage_assessment"],
                    occupied_rows, failed_rows, cfg,
                    stats_out=rank_stats)
                log.setdefault("coverage_repair_rank", []).append(
                    dict(rank_stats))
                if next_row is None:
                    repair_ledger.append(dict(
                        attempt=int(repair_attempts),
                        candidate_pool_row=None,
                        reason="eligible-pool-exhausted", accepted=False))
                    break
                trial_row = int(next_row)
                contacts.append(_regular_contact_from_pool_row(
                    pre["contact_candidate_pool"], trial_row, islands, mesh))
                continue
            # Mandatory baseline normalization happens before rewire/bracing:
            # discard this forest, remove exact failed rows, reconstruct and
            # repartition until no new regular routing failure appears.
            drop = set(regular_unrouted)
            contacts = [c for i, c in enumerate(contacts) if i not in drop]
            if not contacts:
                accepted = _canonical_empty_geometry_state(
                    mesh, pre, [], cfg, geom=geom,
                    surface_tree=surface_tree,
                    _prepared_validation_token=(
                        _prepared_validation_token))
                break
            continue
        if specialized_unrouted:
            carried = np.asarray(carried, dtype=np.float64).copy()
            tip_r = np.asarray(tip_r, dtype=np.float64).copy()
            carried[specialized_unrouted] = 0.0
            tip_r[specialized_unrouted] = 0.0
        if contacts and len(unrouted) == len(contacts):
            accepted = _canonical_empty_geometry_state(
                mesh, pre, contacts, cfg, geom=geom,
                surface_tree=surface_tree,
                _prepared_validation_token=_prepared_validation_token)
            break
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
        coverage_assessments = []
        with _stage(log, f"verify[p{fix_pass}]", t0):
            report = verify.self_verify(mesh, islands, routed_contacts, members, carried, tip_r,
                                        cfg, plate_z, surface_tree=surface_tree, minima=minima,
                                        forest=forest, unrouted=unrouted,
                                        island_births=island_births, geom=geom, balls=balls,
                                        contact_candidate_pool=pre[
                                            "contact_candidate_pool"],
                                        provenance_contacts=contacts,
                                        coverage_samples=pre["coverage_samples"],
                                        face_topology=pre["face_topology"],
                                        coverage_assessment_out=(
                                            coverage_assessments),
                                        _prepared_validation_token=(
                                            _prepared_validation_token))
        coverage_assessment = coverage_assessments[0]
        unrouted_specialized_contacts = _apply_unrouted_specialized_gate(
            report, contacts, forest, unrouted)
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
            n_regular_contact_invariant_violations=len(
                report["regular_contact_invariant_violations"]),
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
            n_unrouted_specialized=len(unrouted_specialized_contacts),
        )
        state = dict(
            forest=forest, members=members, balls=balls, report=report,
            carried=carried, tip_r=tip_r, unrouted=list(unrouted),
            contacts=[dict(c) for c in contacts],
            coverage_assessment=coverage_assessment)
        if accepted is None:
            accepted = state
        else:
            repair_attempts += 1
            (trial_accepted, reason, old_objective, new_objective,
             old_hard, new_hard) = _coverage_repair_trial_verdict(
                accepted["report"], report)
            if trial_accepted:
                repair_ledger.append(dict(
                    attempt=int(repair_attempts),
                    candidate_pool_row=int(trial_row),
                    reason="accepted", accepted=True,
                    objective_before=old_objective,
                    objective_after=new_objective))
                accepted = state
            else:
                failed_rows.add(int(trial_row))
                repair_ledger.append(dict(
                    attempt=int(repair_attempts),
                    candidate_pool_row=int(trial_row),
                    reason=reason, accepted=False,
                    objective_before=old_objective,
                    objective_after=new_objective,
                    noncoverage_before=old_hard,
                    noncoverage_after=new_hard))
                state = accepted
            trial_row = None

        # Restore the accepted authority after either acceptance or rollback.
        forest = accepted["forest"]
        members = accepted["members"]
        balls = accepted["balls"]
        report = accepted["report"]
        carried = accepted["carried"]
        tip_r = accepted["tip_r"]
        unrouted = accepted["unrouted"]
        contacts = [dict(c) for c in accepted["contacts"]]
        if not report["uncovered_islands"]:
            break
        if repair_attempts >= max_repair_attempts:
            break
        occupied_rows = {
            int(c["candidate_pool_row"]) for c in contacts
            if not (c.get("minimum") or c.get("island_birth"))
            and "candidate_pool_row" in c}
        rank_stats = {}
        row = _rank_coverage_repair_rows(
            pre, accepted["coverage_assessment"],
            occupied_rows, failed_rows, cfg,
            stats_out=rank_stats)
        log.setdefault("coverage_repair_rank", []).append(
            dict(rank_stats))
        if row is None:
            repair_ledger.append(dict(
                attempt=int(repair_attempts), candidate_pool_row=None,
                reason="eligible-pool-exhausted", accepted=False))
            break
        trial_row = int(row)
        contacts.append(_regular_contact_from_pool_row(
            pre["contact_candidate_pool"], trial_row, islands, mesh))

    if accepted is not None:
        forest = accepted["forest"]
        members = accepted["members"]
        balls = accepted["balls"]
        report = accepted["report"]
        carried = accepted["carried"]
        tip_r = accepted["tip_r"]
        unrouted = accepted["unrouted"]
        contacts = [dict(c) for c in accepted["contacts"]]
    termination = (
        "covered" if report is not None and not report["uncovered_islands"]
        else "attempt-cap" if repair_attempts >= max_repair_attempts
        else "eligible-pool-exhausted")
    log["coverage_repair"] = dict(
        schema="support-placement/coverage-repair-log",
        version=1,
        attempts=int(repair_attempts),
        max_attempts=int(max_repair_attempts),
        termination=termination,
        failed_candidate_pool_rows=sorted(int(r) for r in failed_rows),
        ledger=repair_ledger)
    # ---- Phase-8 RELOCATION: SINGLE FINAL-ONLY STAGE ------------------------
    # SPEC-P8-RELOCATE §3.0, implementation-consequence amendment (orchestrator-
    # approved 2026-07-24). Relocation used to run INSIDE the fix-pass loop, once
    # per pass. That was wrong twice over:
    #
    #   * CORRECTNESS. Pass-1's construct/rewire/brace_and_size run AFTER pass-0's
    #     relocation and MUTATE the geometry relocation had just certified -- the
    #     acceptance run's 3 embed regressions (the mi 920/991/1010 class) were
    #     brace_and_size drift on endpoints pass-0 relocation had already settled.
    #     §3.0's own phase order ("shaft geometry is finalized before joint
    #     geometry") generalises: NOTHING may mutate geometry after relocation
    #     except the verify that judges it.
    #   * COST. Two full relocation ladders per run, the second re-deriving from a
    #     forest the first had already relocated -- roughly double the wall cost
    #     for no additional resolution.
    #
    # So the ladder now runs ONCE, after every fix pass' construct/rewire/brace has
    # settled, immediately before the FINAL verify -- which is re-run over the
    # relocated forest so the returned report judges what is actually emitted.
    # ruling (5): the persistent formally-unresolvable ledger is owned HERE, above
    # the relocation call, so its rows are carried across passes -- a run that
    # relocates more than once (or a future caller that re-enters the stage) keeps
    # one register per site instead of a fresh per-call snapshot.
    p8_ledger: dict = {}
    if (getattr(cfg, "p8_relocate_enable", False)
            and forest is not None and members):
        with _stage(log, "relocate", t0):
            _prog_r = _ProgressCell()
            # ticker total spans the OUTER round loop too -- `progress.n` counts
            # sweeps globally, so the denominator is sweeps x rounds.
            max_sweeps = (int(getattr(cfg, "p8_max_relocate_sweeps", 8))
                          * int(getattr(cfg, "p8_max_relocate_rounds", 4)))
            with _ticker("relocate", lambda: _prog_r.n, max_sweeps,
                         unit="sweeps"):
                relocate(mesh, forest, unrouted, contacts, carried, tip_r,
                         plate_z, cfg, geom=geom, log=log, progress=_prog_r,
                         islands=islands, island_births=island_births,
                         prepared_state=pre,
                         failed_candidate_pool_rows=failed_rows,
                         carry_ledger=p8_ledger,
                         _prepared_validation_token=(
                             _prepared_validation_token))
        tree.assert_all_certified(forest)
        members, balls = live_member_lists(forest, unrouted, plate_z, cfg)
        log["forest_relocated"] = dict(
            n_members=len(members), n_balls=len(balls), n_unrouted=len(unrouted))
        log["n_unrouted"] = len(unrouted)
        routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
        with _stage(log, "verify[final]", t0):
            report = verify.self_verify(mesh, islands, routed_contacts, members,
                                        carried, tip_r, cfg, plate_z,
                                        surface_tree=surface_tree, minima=minima,
                                        forest=forest, unrouted=unrouted,
                                        island_births=island_births, geom=geom,
                                        balls=balls,
                                        contact_candidate_pool=pre["contact_candidate_pool"],
                                        provenance_contacts=contacts,
                                        coverage_samples=pre["coverage_samples"],
                                        face_topology=pre["face_topology"],
                                        _prepared_validation_token=(
                                            _prepared_validation_token))
        log["verify_final"] = dict(
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
            n_regular_contact_invariant_violations=len(
                report["regular_contact_invariant_violations"]),
            n_removal_blocked_trees=len(report["removal_violations"]),
            # ruling (5): the honest-failure count belongs next to the gate
            # numbers -- a reader of verify[final] must see what relocation gave
            # up on, not just what the gates still flag.
            n_unresolvable=int(log.get("p8_n_unresolvable", 0)),
        )

    unrouted_specialized_contacts = _apply_unrouted_specialized_gate(
        report, contacts, forest, unrouted)
    if "verify_final" in log:
        log["verify_final"]["ok"] = report["ok"]
        log["verify_final"]["n_unrouted_specialized"] = len(
            unrouted_specialized_contacts)

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
        # ruling (5): the R2 re-seat's domain + birth-guard inputs. Returned so a
        # caller re-entering `relocate` over this result (the §6.10 idempotence
        # pass, an acceptance harness) can hand the stage the SAME domain the run
        # itself used -- a second pass offered a DIFFERENT ladder would not be
        # testing idempotence of anything.
        island_births=island_births,
        coverage_failed_candidate_pool_rows=tuple(
            sorted(int(r) for r in failed_rows)),
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


def _exact_specialized_face_id(
        mesh, point, allowed_face_ids, geom=None, block_size=65_536):
    """Exact nearest source triangle, with lowest-face-id deterministic ties."""
    point = np.asarray(point, dtype=np.float64)
    allowed = np.asarray(allowed_face_ids, dtype=np.int64)
    if not len(allowed):
        raise ValueError("specialized contact has no legal source face")
    if geom is not None and hasattr(geom, "project"):
        _distance, _closest, projected_face, _normal = geom.project(point)
        projected_face = int(projected_face)
        if np.any(allowed == projected_face):
            return projected_face
    triangles = np.asarray(mesh.triangles, dtype=np.float64)
    best_face = None
    best_distance_sq = np.inf
    for start in range(0, len(allowed), block_size):
        face_ids = allowed[start:start + block_size]
        tri = triangles[face_ids]
        points = np.broadcast_to(point, (len(face_ids), 3))
        closest = closest_on_tri(
            points, tri[:, 0], tri[:, 1], tri[:, 2])
        distance_sq = np.einsum(
            "ij,ij->i", closest - points, closest - points,
            optimize=False)
        block_best = float(np.min(distance_sq))
        tied_faces = face_ids[distance_sq == block_best]
        face_id = int(np.min(tied_faces))
        if (block_best < best_distance_sq
                or (block_best == best_distance_sq
                    and (best_face is None or face_id < best_face))):
            best_distance_sq = block_best
            best_face = face_id
    return int(best_face)


class _FaceIslandOwnership:
    """Grouped many-to-many face ownership with deterministic synthetic adds.

    Existing memberships live in one face-sorted CSR-like pair of arrays.
    Synthetic islands are rare and append monotonically increasing island ids
    into a small per-face overlay.  A face therefore retains *all* named
    owners; no scalar face->island overwrite can collapse overlapping islands.
    """

    def __init__(self, islands, n_faces):
        self.n_faces = int(n_faces)
        counts = np.fromiter(
            (len(np.unique(np.asarray(fids, dtype=np.int64)))
             for fids in islands),
            dtype=np.int64, count=len(islands))
        if int(counts.sum()) == 0:
            self.offsets = np.zeros(self.n_faces + 1, dtype=np.int64)
            self.island_ids = np.empty(0, dtype=np.int64)
        else:
            faces = np.concatenate([
                np.unique(np.asarray(fids, dtype=np.int64))
                for fids in islands])
            owners = np.repeat(
                np.arange(len(islands), dtype=np.int64), counts)
            if np.any((faces < 0) | (faces >= self.n_faces)):
                raise ValueError("island face id is outside the mesh")
            order = np.lexsort((owners, faces))
            faces = faces[order]
            self.island_ids = owners[order]
            face_counts = np.bincount(faces, minlength=self.n_faces)
            self.offsets = np.r_[
                0, np.cumsum(face_counts, dtype=np.int64)]
        self.synthetic = {}

    def owners(self, face_id):
        face_id = int(face_id)
        start = int(self.offsets[face_id])
        stop = int(self.offsets[face_id + 1])
        base = self.island_ids[start:stop]
        added = self.synthetic.get(face_id, ())
        if not added:
            return tuple(int(iid) for iid in base)
        return tuple(int(iid) for iid in base) + tuple(added)

    def add_synthetic_island(self, face_ids, island_id):
        island_id = int(island_id)
        for face_id in np.unique(np.asarray(face_ids, dtype=np.int64)):
            key = int(face_id)
            prior = self.synthetic.get(key, ())
            if not prior or prior[-1] != island_id:
                self.synthetic[key] = prior + (island_id,)


def _component_local_synthetic_face_ids(
        position, best, ext_ids, centers_tree, face_topology):
    """Return the existing 1.5mm local patch, clipped to ``best``'s component."""
    best = int(best)
    ext_ids = np.asarray(ext_ids, dtype=np.int64)
    local = centers_tree.query_ball_point(position, r=1.5)
    if not local:
        _, nearest = centers_tree.query(position)
        local = [int(nearest)]
    near = ext_ids[np.asarray(local, dtype=np.int64)]
    component_ids = face_topology.component_ids
    best_component = int(component_ids[best])
    near = near[component_ids[near] == best_component]
    face_ids = np.unique(np.r_[near, np.int64(best)]).astype(
        np.int64, copy=False)
    if (len(face_ids) == 0
            or not np.all(component_ids[face_ids] == best_component)):
        raise AssertionError(
            "synthetic island must contain exactly one topology component")
    return face_ids


def _authoritative_specialized_face_topology(
        mesh, cfg, face_topology, authorized):
    """Build legacy authority or validate a caller-supplied sidecar once."""
    if face_topology is None:
        return topology.build_face_topology(mesh, cfg=cfg)
    if authorized:
        return face_topology
    return topology.validate_face_topology(face_topology, mesh=mesh)


def _contact_positions_by_island(contacts, n_islands):
    """Stack contact positions once per named island for domain queries."""
    grouped = [[] for _ in range(n_islands)]
    for contact in contacts:
        island_id = int(contact.get("island_id", -1))
        if 0 <= island_id < n_islands:
            grouped[island_id].append(
                np.asarray(contact["position"], dtype=np.float64))
    return [
        (np.asarray(rows, dtype=np.float64)
         if rows else np.empty((0, 3), dtype=np.float64))
        for rows in grouped]


def _ensure_minima_contacts(
        mesh, islands, contacts, minima, cfg, geom=None, face_topology=None,
        _face_topology_authorized=False):
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
    them like any other region. When the exact owning face belongs to
    overlapping named islands, each island gets its own route demand unless
    a same-island contact already satisfies this minimum. Mutates neither
    input list in place; returns (contacts, islands, n_added)."""
    from scipy.spatial import cKDTree
    if not minima:
        return contacts, islands, 0
    contacts = list(contacts)
    islands = list(islands)
    face_topology = _authoritative_specialized_face_topology(
        mesh, cfg, face_topology, _face_topology_authorized)
    ownership = _FaceIslandOwnership(islands, len(mesh.faces))
    base_positions = _contact_positions_by_island(contacts, len(islands))
    base_trees = [
        cKDTree(rows) if len(rows) else None
        for rows in base_positions]
    added_positions = [[] for _ in islands]

    def island_satisfied(island_id, region_pts):
        tree = base_trees[island_id]
        if tree is not None:
            distances, _nearest = tree.query(
                region_pts, workers=routing.query_workers(len(region_pts)))
            if float(np.min(distances)) <= perception.ISLAND_ATTACH_REACH_MM:
                return True
        if added_positions[island_id]:
            added = np.asarray(
                added_positions[island_id], dtype=np.float64)
            distances = np.linalg.norm(
                region_pts[:, None, :] - added[None, :, :], axis=2)
            if float(np.min(distances)) <= perception.ISLAND_ATTACH_REACH_MM:
                return True
        return False

    ext_ids = _exterior_face_ids(mesh, geom)
    centers_tree = cKDTree(mesh.triangles_center[ext_ids])
    added = 0
    for m in minima:
        region_pts = np.asarray(m["region_pts"], dtype=np.float64)
        best = _exact_specialized_face_id(
            mesh, m["position"], ext_ids, geom=geom)
        owners = ownership.owners(best)
        if not owners:
            fids = _component_local_synthetic_face_ids(
                m["position"], best, ext_ids, centers_tree, face_topology)
            islands.append(fids)
            owners = (len(islands) - 1,)
            ownership.add_synthetic_island(fids, owners[0])
            base_trees.append(None)
            added_positions.append([])
        for iid in owners:
            if island_satisfied(iid, region_pts):
                continue
            position = np.asarray(m["position"], dtype=np.float64)
            area = float(mesh.area_faces[islands[iid]].sum())
            contacts.append(dict(
                position=position.copy(),
                normal=np.asarray(m["normal"], dtype=np.float64),
                island_id=iid, area_mm2=area, minimum=True,
                coverage_face_id=int(best),
                coverage_component_id=int(
                    face_topology.component_ids[int(best)]),
                region_pts=region_pts.copy()))
            added_positions[iid].append(position.copy())
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


def _ensure_island_birth_contacts(
        mesh, islands, contacts, island_births, cfg, geom=None,
        face_topology=None, _face_topology_authorized=False):
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

    Mirrors _ensure_minima_contacts' island bookkeeping. Every named island
    containing the exact owning face receives its own route demand unless an
    existing contact in that same island already covers the birth. A birth
    outside all known overhang islands gets one small synthetic island appended
    (load partition / coverage_report then treat it like any other region).
    The synthetic island's complete local face set is registered immediately,
    so later ladder placements reuse it instead of spawning one redundant
    synthetic island per rung.

    Mutates neither input list; returns (contacts, islands, n_added)."""
    if not island_births:
        return contacts, islands, 0
    from scipy.spatial import cKDTree
    contacts = list(contacts)
    islands = list(islands)
    face_topology = _authoritative_specialized_face_topology(
        mesh, cfg, face_topology, _face_topology_authorized)

    z_tol_mm = cfg.minima_connect_layers * cfg.layer_height_mm
    attach_r = perception.ISLAND_ATTACH_REACH_MM

    ownership = _FaceIslandOwnership(islands, len(mesh.faces))
    base_positions = _contact_positions_by_island(contacts, len(islands))
    added_positions = [[] for _ in islands]

    def already_covered(b, island_id):
        rows = base_positions[island_id]
        if added_positions[island_id]:
            added = np.asarray(
                added_positions[island_id], dtype=np.float64)
            rows = np.vstack((rows, added)) if len(rows) else added
        if len(rows) == 0:
            return False
        z_ok = np.abs(rows[:, 2] - b["z"]) <= z_tol_mm
        if not z_ok.any():
            return False
        for cx, cy, _cz in rows[z_ok]:
            ok, _d = perception.island_birth_xy_ok(float(cx), float(cy), b["loop"], attach_r)
            if ok:
                return True
        return False

    ext_ids = _exterior_face_ids(mesh, geom)
    centers_tree = cKDTree(mesh.triangles_center[ext_ids])
    births_by_owner = {iid: [] for iid in range(len(islands))}
    unowned = []
    for birth in island_births:
        position = np.array(
            [birth["x"], birth["y"], birth["z"]], dtype=np.float64)
        best = _exact_specialized_face_id(
            mesh, position, ext_ids, geom=geom)
        owners = ownership.owners(best)
        if not owners:
            unowned.append(birth)
            continue
        for island_id in owners:
            if not already_covered(birth, island_id):
                births_by_owner[island_id].append(birth)

    added = 0
    for island_id in sorted(births_by_owner):
        uncovered = births_by_owner[island_id]
        if not uncovered:
            continue
        clusters = _cluster_island_births_3d(
            uncovered, _ISLAND_BIRTH_LINK_R_MM)
        for cluster_idx in clusters:
            placements = _greedy_ladder_positions(
                cluster_idx, uncovered, attach_r, z_tol_mm)
            for b in placements:
                if already_covered(b, island_id):
                    continue
                pos = np.array([b["x"], b["y"], b["z"]], float)
                best = _exact_specialized_face_id(
                    mesh, pos, ext_ids, geom=geom)
                normal = np.asarray(mesh.face_normals[best], float)
                area = float(mesh.area_faces[islands[island_id]].sum())
                contacts.append(dict(
                    position=pos, normal=normal, island_id=island_id,
                    area_mm2=area, minimum=True, island_birth=True,
                    coverage_face_id=int(best),
                    coverage_component_id=int(
                        face_topology.component_ids[int(best)]),
                    birth_loop=np.asarray(b["loop"], float).copy(),
                    birth_z=float(b["z"])))
                added_positions[island_id].append(pos.copy())
                added += 1

    if unowned:
        clusters = _cluster_island_births_3d(
            unowned, _ISLAND_BIRTH_LINK_R_MM)
        for cluster_idx in clusters:
            placements = _greedy_ladder_positions(
                cluster_idx, unowned, attach_r, z_tol_mm)
            for b in placements:
                pos = np.array([b["x"], b["y"], b["z"]], float)
                best = _exact_specialized_face_id(
                    mesh, pos, ext_ids, geom=geom)
                owners = ownership.owners(best)
                if not owners:
                    fids = _component_local_synthetic_face_ids(
                        pos, best, ext_ids, centers_tree, face_topology)
                    islands.append(fids)
                    owners = (len(islands) - 1,)
                    ownership.add_synthetic_island(fids, owners[0])
                    base_positions.append(
                        np.empty((0, 3), dtype=np.float64))
                    added_positions.append([])
                normal = np.asarray(mesh.face_normals[best], float)
                for island_id in owners:
                    if already_covered(b, island_id):
                        continue
                    area = float(
                        mesh.area_faces[islands[island_id]].sum())
                    contacts.append(dict(
                        position=pos.copy(), normal=normal.copy(),
                        island_id=island_id, area_mm2=area, minimum=True,
                        island_birth=True, coverage_face_id=int(best),
                        coverage_component_id=int(
                            face_topology.component_ids[int(best)]),
                        birth_loop=np.asarray(
                            b["loop"], float).copy(),
                        birth_z=float(b["z"])))
                    added_positions[island_id].append(pos.copy())
                    added += 1
    return contacts, islands, added


def _add_fix_contacts(mesh, islands, contacts, uncovered,
                      candidate_pool_prepared=None, log=None, cfg=None):
    """Add one regular contact at the worst-covered LEGAL pool seat.

    The coverage-distance ranking is unchanged, but a repair may no longer
    invent a face-centroid seat: selection is restricted to the same
    lowest-vertex pool as initial regular placement.
    """
    by_island = {}
    for c in contacts:
        by_island.setdefault(c["island_id"], []).append(np.asarray(c["position"]))
    if candidate_pool_prepared is None:
        if cfg is None:
            raise ValueError("cfg is required when no prepared candidate pool is supplied")
        prepared = placement.candidate_pool(mesh, islands, cfg)
    else:
        prepared = candidate_pool_prepared
    placement.validate_candidate_pool(
        prepared, mesh=mesh, islands=islands)

    extra = list(contacts)
    for u in uncovered:
        iid = u["island_id"]
        occupied = {tuple(float(x) for x in p)
                    for p in by_island.get(iid, ())}
        island_slice = prepared.island_slice(iid)
        pool_rows = np.arange(island_slice.start, island_slice.stop,
                              dtype=np.int64)
        available = np.fromiter(
            (tuple(float(x) for x in prepared.positions[row]) not in occupied
             for row in pool_rows),
            dtype=bool, count=len(pool_rows))
        candidate_rows = pool_rows[available]
        if not len(candidate_rows):
            if log is not None:
                log.setdefault("contact_pool_exhausted", []).append(
                    dict(island_id=int(iid),
                         n_pool=len(pool_rows),
                         n_occupied=len(occupied)))
            continue
        pts = prepared.positions[candidate_rows]
        existing = np.array(by_island.get(iid, []))
        if len(existing):
            from scipy.spatial import cKDTree
            tree_existing = cKDTree(existing)
            dist, _ = tree_existing.query(pts, workers=routing.query_workers(len(pts)))
        else:
            dist = np.full(len(pts), np.inf)
        worst = int(np.argmax(dist))
        row = int(candidate_rows[worst])
        extra.append(dict(
            position=prepared.positions[row].copy(),
            normal=prepared.normals[row].copy(),
            island_id=iid, face_id=int(prepared.face_ids[row]),
            candidate_pool_row=int(row),
            area_mm2=u["n_faces"]))
    return extra


def _coverage_objective(uncovered):
    """Pinned A3 lexicographic objective; lower is better."""
    return (
        float(sum(
            u.get("failing_uncovered_area_mm2", 0.0) for u in uncovered)),
        float(max(
            (u.get("max_patch_area_mm2", 0.0) for u in uncovered),
            default=0.0)),
        int(sum(u.get("n_failing_patches", 0) for u in uncovered)),
        float(max(
            (u.get("worst_gap_mm", 0.0) for u in uncovered), default=0.0)),
    )


_NONCOVERAGE_GATE_KEYS = (
    "clearance_violations",
    "load_violations",
    "unsupported_minima",
    "uncovered_island_births",
    "regular_contact_invariant_violations",
    "connectivity_violations",
    "printability_violations",
    "murray_violations",
    "ball_breach_violations",
    "unrouted_specialized_contacts",
)


def _noncoverage_hard_counts(report):
    return tuple(len(report.get(key, ())) for key in _NONCOVERAGE_GATE_KEYS)


def _coverage_repair_trial_verdict(accepted_report, trial_report):
    before = _coverage_objective(
        accepted_report.get("uncovered_islands", ()))
    after = _coverage_objective(
        trial_report.get("uncovered_islands", ()))
    hard_before = _noncoverage_hard_counts(accepted_report)
    hard_after = _noncoverage_hard_counts(trial_report)
    if not after < before:
        return False, "coverage-not-improved", before, after, hard_before, hard_after
    if not all(new <= old for new, old in zip(hard_after, hard_before)):
        return (
            False, "noncoverage-hard-gate-regression",
            before, after, hard_before, hard_after)
    return True, "accepted", before, after, hard_before, hard_after


def _rank_coverage_repair_rows(
        pre, assessment, occupied_rows, failed_rows, cfg, stats_out=None):
    """Rank eligible A4 rows by marginal currently-uncovered sample area.

    Work is grouped by island/topology component and evaluated in bounded
    candidate-by-sample matrix blocks. No candidate performs an individual
    geometry query. Equal marginal area breaks toward the lower pool row.
    """
    pool = pre["contact_candidate_pool"]
    samples = pre["coverage_samples"]
    face_topology = pre["face_topology"]
    radius = float(cfg.target_spacing_mm) * float(cfg.coverage_radius_factor)
    matrix_budget = int(getattr(
        cfg, "coverage_score_matrix_max_entries", 65_536))
    if (not isinstance(assessment, verify.CoverageAssessment)
            or len(assessment.failing_sample_mask) != len(samples)
            or len(assessment.sample_components) != len(samples)):
        raise ValueError("A4 requires the matching run-local A3 assessment")
    if matrix_budget <= 0:
        raise ValueError("coverage score matrix budget must be positive")
    blocked_rows = np.asarray(sorted(
        {int(r) for r in occupied_rows} | {int(r) for r in failed_rows}),
        dtype=np.int64)
    pool_components = face_topology.component_ids[pool.face_ids]
    best_row = None
    best_score = -np.inf
    max_matrix_entries = 0
    candidate_rows_scanned = 0
    eligible_rows = 0
    sample_rows_ranked = 0
    max_candidate_tile = 0
    max_sample_tile = 0
    for key in assessment.failing_route_keys:
        iid, component_id = key
        pool_slice = pool.island_slice(iid)
        pool_rows = np.arange(
            pool_slice.start, pool_slice.stop, dtype=np.int64)
        candidate_rows_scanned += len(pool_rows)
        candidate_rows = pool_rows[
            pool_components[pool_rows] == component_id]
        if len(blocked_rows):
            candidate_rows = candidate_rows[
                ~np.isin(candidate_rows, blocked_rows)]
        sample_slice = samples.island_slice(iid)
        island_sample_rows = np.arange(
            sample_slice.start, sample_slice.stop, dtype=np.int64)
        sample_rows = island_sample_rows[
            assessment.failing_sample_mask[island_sample_rows]
            & (assessment.sample_components[island_sample_rows]
               == component_id)]
        eligible_rows += len(candidate_rows)
        sample_rows_ranked += len(sample_rows)
        if not len(candidate_rows) or not len(sample_rows):
            continue
        sample_tile = min(
            len(sample_rows), max(1, int(np.sqrt(matrix_budget))))
        candidate_tile = max(1, matrix_budget // sample_tile)
        max_candidate_tile = max(max_candidate_tile, candidate_tile)
        max_sample_tile = max(max_sample_tile, sample_tile)
        for candidate_start in range(
                0, len(candidate_rows), candidate_tile):
            rows = candidate_rows[
                candidate_start:candidate_start + candidate_tile]
            marginal = np.zeros(len(rows), dtype=np.float64)
            for sample_start in range(0, len(sample_rows), sample_tile):
                srows = sample_rows[
                    sample_start:sample_start + sample_tile]
                delta = (
                    pool.positions[rows, None, :]
                    - samples.positions[srows][None, :, :])
                distance_sq = np.einsum(
                    "ijk,ijk->ij", delta, delta, optimize=False)
                max_matrix_entries = max(
                    max_matrix_entries, int(distance_sq.size))
                marginal += (
                    distance_sq <= radius * radius
                ) @ samples.area_weights_mm2[srows]
            block_max = float(np.max(marginal))
            row = int(rows[np.flatnonzero(marginal == block_max)[0]])
            if (block_max > best_score
                    or (block_max == best_score
                        and (best_row is None or row < best_row))):
                best_score = block_max
                best_row = row
    if stats_out is not None:
        stats_out.update(
            n_candidate_rows_scanned=int(candidate_rows_scanned),
            n_eligible_rows=int(eligible_rows),
            n_failing_samples_ranked=int(sample_rows_ranked),
            max_score_matrix_entries=int(max_matrix_entries),
            max_candidate_tile_rows=int(max_candidate_tile),
            max_sample_tile_rows=int(max_sample_tile),
        )
    return best_row


def _synthetic_matrix_budget(cfg):
    budget = int(getattr(
        cfg, "coverage_score_matrix_max_entries", 65_536))
    if budget <= 0:
        raise ValueError("synthetic coverage matrix budget must be positive")
    return budget


def _synthetic_mask_word_budget(cfg):
    budget = int(getattr(
        cfg, "synthetic_coverage_mask_max_words", 131_072))
    if budget <= 0:
        raise ValueError("synthetic coverage mask word budget must be positive")
    return budget


def _synthetic_note_matrix(stats, n_entries):
    stats["max_matrix_entries"] = max(
        int(stats.get("max_matrix_entries", 0)), int(n_entries))


_SYNTHETIC_SELECTOR_MAX_PEAK_BYTES = 512 * 1024 * 1024


def _synthetic_selector_resource_plan(
        candidate_count, needed_sample_count, matrix_budget,
        retained_prepared_authority_bytes=0):
    """Conservative selector peak plan, including Python container overhead."""
    candidates = int(candidate_count)
    samples = int(needed_sample_count)
    words_per_candidate = (samples + 63) // 64
    requested_words = candidates * words_per_candidate

    # Deliberately conservative and implementation-independent: each retained
    # Python integer is charged 128 bytes plus twice its dense uint64 payload,
    # and the owning list is charged 64 bytes plus one 8-byte pointer per row.
    retained_mask_bytes = (
        64 + 8 * candidates
        + candidates * (128 + 16 * words_per_candidate))
    # The live vectorized tile retains xyz deltas and squared distances. Boolean
    # comparison plus packed output are charged as two additional bytes/entry.
    distance_tile_bytes = 32 * int(matrix_budget)
    packing_scratch_bytes = 2 * int(matrix_budget)

    # Pair batching deliberately remains unchanged. Charge the largest possible
    # Python list/tuple batch and its dense subset/position/metric work arrays;
    # higher-k batches are no larger after their cardinality-aware bound.
    pair_batch_count = int(matrix_budget)
    python_pair_batch_bytes = (
        64 + pair_batch_count * (8 + 80))
    dense_pair_batch_bytes = pair_batch_count * 2 * (8 + 24)
    metric_vector_bytes = pair_batch_count * 16
    exact_search_scratch_bytes = (
        python_pair_batch_bytes
        + dense_pair_batch_bytes
        + metric_vector_bytes)
    planned_peak_bytes = (
        int(retained_prepared_authority_bytes)
        + retained_mask_bytes
        + distance_tile_bytes
        + packing_scratch_bytes
        + exact_search_scratch_bytes)
    return dict(
        candidate_count=candidates,
        needed_sample_count=samples,
        requested_mask_words=int(requested_words),
        estimated_retained_mask_bytes=int(retained_mask_bytes),
        retained_prepared_authority_bytes=int(
            retained_prepared_authority_bytes),
        bounded_distance_tile_bytes=int(distance_tile_bytes),
        packing_scratch_bytes=int(packing_scratch_bytes),
        exact_search_scratch_bytes=int(exact_search_scratch_bytes),
        planned_selector_peak_bytes=int(planned_peak_bytes),
        selector_max_peak_bytes=int(_SYNTHETIC_SELECTOR_MAX_PEAK_BYTES),
    )


def _synthetic_allocate_masks(candidate_count):
    """Single allocation seam kept behind the selector resource preflight."""
    return [0] * int(candidate_count)


def _synthetic_covered_mask(
        contact_positions, sample_positions, radius_mm, matrix_budget, stats):
    """A3 inclusive-radius cover, tiled on both axes."""
    contacts = np.asarray(contact_positions, dtype=np.float64).reshape((-1, 3))
    samples = np.asarray(sample_positions, dtype=np.float64).reshape((-1, 3))
    covered = np.zeros(len(samples), dtype=bool)
    if not len(contacts) or not len(samples):
        return covered
    contact_tile = min(
        len(contacts), max(1, int(np.sqrt(matrix_budget))))
    sample_tile = max(1, matrix_budget // contact_tile)
    radius_sq = float(radius_mm) ** 2
    for sample_start in range(0, len(samples), sample_tile):
        sample_stop = min(len(samples), sample_start + sample_tile)
        nearest_sq = np.full(
            sample_stop - sample_start, np.inf, dtype=np.float64)
        for contact_start in range(0, len(contacts), contact_tile):
            block = contacts[
                contact_start:contact_start + contact_tile, None, :]
            delta = block - samples[None, sample_start:sample_stop, :]
            distance_sq = np.einsum(
                "ijk,ijk->ij", delta, delta, optimize=False)
            _synthetic_note_matrix(stats, distance_sq.size)
            nearest_sq = np.minimum(nearest_sq, distance_sq.min(axis=0))
        covered[sample_start:sample_stop] = nearest_sq <= radius_sq
    return covered


def _synthetic_spacing_eligible(
        candidate_positions, blocker_positions, spacing_mm,
        matrix_budget, stats):
    """Return candidates inclusively at least ``spacing_mm`` from blockers."""
    candidates = np.asarray(
        candidate_positions, dtype=np.float64).reshape((-1, 3))
    blockers = np.asarray(
        blocker_positions, dtype=np.float64).reshape((-1, 3))
    eligible = np.ones(len(candidates), dtype=bool)
    if not len(candidates) or not len(blockers):
        return eligible
    blocker_tile = min(
        len(blockers), max(1, int(np.sqrt(matrix_budget))))
    candidate_tile = max(1, matrix_budget // blocker_tile)
    spacing_sq = float(spacing_mm) ** 2
    for candidate_start in range(0, len(candidates), candidate_tile):
        candidate_stop = min(
            len(candidates), candidate_start + candidate_tile)
        minimum_sq = np.full(
            candidate_stop - candidate_start, np.inf, dtype=np.float64)
        for blocker_start in range(0, len(blockers), blocker_tile):
            delta = (
                candidates[candidate_start:candidate_stop, None, :]
                - blockers[
                    None, blocker_start:blocker_start + blocker_tile, :])
            distance_sq = np.einsum(
                "ijk,ijk->ij", delta, delta, optimize=False)
            _synthetic_note_matrix(stats, distance_sq.size)
            minimum_sq = np.minimum(
                minimum_sq, distance_sq.min(axis=1))
        eligible[candidate_start:candidate_stop] = (
            minimum_sq >= spacing_sq)
    return eligible


def _synthetic_candidate_masks(
        candidate_positions, needed_sample_positions, radius_mm,
        matrix_budget, mask_word_budget, stats):
    """Build packed Python-int masks without an unbounded C-by-S matrix.

    The retained masks are admitted only when their dense uint64-equivalent
    payload is at most ``synthetic_coverage_mask_max_words``. Every distance
    tile is independently bounded by ``coverage_score_matrix_max_entries``.
    Both the word bound and aggregate physical peak are checked before any
    mask, packbits, or search allocation.
    """
    candidate_count = len(candidate_positions)
    sample_count = len(needed_sample_positions)
    plan = _synthetic_selector_resource_plan(
        candidate_count, sample_count, matrix_budget,
        stats.get("retained_prepared_authority_bytes", 0))
    words = int(plan["requested_mask_words"])
    stats.update(plan)
    stats["configured_mask_word_limit"] = int(mask_word_budget)
    stats["packed_mask_words"] = (
        int(stats.get("packed_mask_words", 0)) + int(words))
    if words > int(mask_word_budget):
        stats["mask_preflight_passed"] = False
        stats["mask_refusal_reason"] = "requested-mask-words"
        return None
    if (int(plan["planned_selector_peak_bytes"])
            > _SYNTHETIC_SELECTOR_MAX_PEAK_BYTES):
        stats["mask_preflight_passed"] = False
        stats["mask_refusal_reason"] = "selector-aggregate-peak"
        return None
    stats["mask_preflight_passed"] = True
    stats["mask_refusal_reason"] = None
    candidates = np.asarray(
        candidate_positions, dtype=np.float64).reshape((-1, 3))
    samples = np.asarray(
        needed_sample_positions, dtype=np.float64).reshape((-1, 3))
    masks = _synthetic_allocate_masks(candidate_count)
    if not candidate_count or not sample_count:
        return masks
    sample_tile = min(
        len(samples), max(1, int(np.sqrt(matrix_budget))))
    candidate_tile = max(1, matrix_budget // sample_tile)
    radius_sq = float(radius_mm) ** 2
    for candidate_start in range(0, len(candidates), candidate_tile):
        candidate_stop = min(
            len(candidates), candidate_start + candidate_tile)
        for sample_start in range(0, len(samples), sample_tile):
            sample_stop = min(len(samples), sample_start + sample_tile)
            delta = (
                candidates[candidate_start:candidate_stop, None, :]
                - samples[None, sample_start:sample_stop, :])
            distance_sq = np.einsum(
                "ijk,ijk->ij", delta, delta, optimize=False)
            _synthetic_note_matrix(stats, distance_sq.size)
            packed = np.packbits(
                distance_sq <= radius_sq, axis=1, bitorder="little")
            for local_row, payload in enumerate(packed):
                masks[candidate_start + local_row] |= (
                    int.from_bytes(payload.tobytes(), "little")
                    << sample_start)
    return masks


def _synthetic_selection_metrics(
        component_contact_positions, all_contact_positions,
        sample_positions, matrix_budget, stats):
    """Return (worst A3 sample gap, minimum all-contact pair spacing)."""
    component_contacts = np.asarray(
        component_contact_positions, dtype=np.float64).reshape((-1, 3))
    samples = np.asarray(
        sample_positions, dtype=np.float64).reshape((-1, 3))
    worst_gap = 0.0
    if len(samples):
        if not len(component_contacts):
            worst_gap = np.inf
        else:
            contact_tile = min(
                len(component_contacts),
                max(1, int(np.sqrt(matrix_budget))))
            sample_tile = max(1, matrix_budget // contact_tile)
            worst_sq = 0.0
            for sample_start in range(0, len(samples), sample_tile):
                sample_stop = min(
                    len(samples), sample_start + sample_tile)
                nearest_sq = np.full(
                    sample_stop - sample_start, np.inf, dtype=np.float64)
                for contact_start in range(
                        0, len(component_contacts), contact_tile):
                    delta = (
                        component_contacts[
                            contact_start:contact_start + contact_tile,
                            None, :]
                        - samples[None, sample_start:sample_stop, :])
                    distance_sq = np.einsum(
                        "ijk,ijk->ij", delta, delta, optimize=False)
                    _synthetic_note_matrix(stats, distance_sq.size)
                    nearest_sq = np.minimum(
                        nearest_sq, distance_sq.min(axis=0))
                worst_sq = max(
                    worst_sq, float(nearest_sq.max(initial=0.0)))
            worst_gap = float(np.sqrt(worst_sq))

    all_positions = np.asarray(
        all_contact_positions, dtype=np.float64).reshape((-1, 3))
    minimum_pair_sq = np.inf
    for left in range(len(all_positions) - 1):
        for start in range(left + 1, len(all_positions), matrix_budget):
            delta = all_positions[
                start:start + matrix_budget] - all_positions[left]
            distance_sq = np.einsum(
                "ij,ij->i", delta, delta, optimize=False)
            _synthetic_note_matrix(stats, distance_sq.size)
            minimum_pair_sq = min(
                minimum_pair_sq, float(distance_sq.min(initial=np.inf)))
    return worst_gap, float(np.sqrt(minimum_pair_sq))


def _synthetic_greedy_cover(
        masks, positions, rows, full_mask, fixed_positions, spacing_mm):
    """Deterministic valid-cover upper bound; returns None rather than a hole."""
    selected = []
    covered = 0
    spacing_sq = float(spacing_mm) ** 2
    while covered != full_mask:
        best = None
        best_gain = 0
        blockers = (
            np.vstack((
                fixed_positions,
                positions[selected],
            ))
            if selected else fixed_positions)
        for idx, mask in enumerate(masks):
            if idx in selected:
                continue
            if len(blockers):
                delta = blockers - positions[idx]
                if np.any(np.einsum(
                        "ij,ij->i", delta, delta, optimize=False)
                        < spacing_sq):
                    continue
            gain = int((mask & ~covered).bit_count())
            if (gain > best_gain
                    or (gain == best_gain and gain > 0
                        and (best is None or int(rows[idx]) < int(rows[best])))):
                best = idx
                best_gain = gain
        if best is None or best_gain == 0:
            return None
        selected.append(best)
        covered |= masks[best]
    return tuple(selected)


def _synthetic_best_subset_batch(
        subset_indices, positions, rows, specialized_component_positions,
        fixed_island_positions, sample_positions, matrix_budget, stats):
    """Vectorized fixed-cardinality tie metrics under the matrix cap."""
    subsets = np.asarray(subset_indices, dtype=np.int64)
    if not len(subsets):
        return None, None
    if subsets.ndim != 2 or subsets.shape[1] <= 0:
        raise ValueError("synthetic subset batch must be nonempty and 2D")
    cardinality = int(subsets.shape[1])
    samples = np.asarray(
        sample_positions, dtype=np.float64).reshape((-1, 3))
    specialized = np.asarray(
        specialized_component_positions,
        dtype=np.float64).reshape((-1, 3))
    base_nearest_sq = np.full(len(samples), np.inf, dtype=np.float64)
    if len(specialized):
        contact_tile = min(
            len(specialized), max(1, int(np.sqrt(matrix_budget))))
        sample_tile = max(1, matrix_budget // contact_tile)
        for sample_start in range(0, len(samples), sample_tile):
            sample_stop = min(len(samples), sample_start + sample_tile)
            nearest = np.full(
                sample_stop - sample_start, np.inf, dtype=np.float64)
            for contact_start in range(0, len(specialized), contact_tile):
                delta = (
                    specialized[
                        contact_start:contact_start + contact_tile, None, :]
                    - samples[None, sample_start:sample_stop, :])
                distance_sq = np.einsum(
                    "ijk,ijk->ij", delta, delta, optimize=False)
                _synthetic_note_matrix(stats, distance_sq.size)
                nearest = np.minimum(nearest, distance_sq.min(axis=0))
            base_nearest_sq[sample_start:sample_stop] = nearest

    subset_tile = min(
        len(subsets),
        max(1, int(np.sqrt(matrix_budget // cardinality))))
    sample_tile = max(
        1, matrix_budget // (cardinality * subset_tile))
    worst_sq = np.zeros(len(subsets), dtype=np.float64)
    for subset_start in range(0, len(subsets), subset_tile):
        subset_stop = min(
            len(subsets), subset_start + subset_tile)
        subset_positions = positions[
            subsets[subset_start:subset_stop]]
        block_worst = np.zeros(
            subset_stop - subset_start, dtype=np.float64)
        for sample_start in range(0, len(samples), sample_tile):
            sample_stop = min(len(samples), sample_start + sample_tile)
            delta = (
                subset_positions[:, :, None, :]
                - samples[None, None, sample_start:sample_stop, :])
            distance_sq = np.einsum(
                "bksd,bksd->bks", delta, delta, optimize=False)
            _synthetic_note_matrix(stats, distance_sq.size)
            nearest_sq = np.minimum(
                distance_sq.min(axis=1),
                base_nearest_sq[None, sample_start:sample_stop])
            block_worst = np.maximum(
                block_worst, nearest_sq.max(axis=1, initial=0.0))
        worst_sq[subset_start:subset_stop] = block_worst

    fixed = np.asarray(
        fixed_island_positions, dtype=np.float64).reshape((-1, 3))
    fixed_min_sq = np.inf
    for left in range(len(fixed) - 1):
        delta = fixed[left + 1:] - fixed[left]
        distance_sq = np.einsum(
            "ij,ij->i", delta, delta, optimize=False)
        _synthetic_note_matrix(stats, distance_sq.size)
        fixed_min_sq = min(
            fixed_min_sq, float(distance_sq.min(initial=np.inf)))
    candidate_fixed_min_sq = np.full(
        len(positions), np.inf, dtype=np.float64)
    if len(fixed):
        fixed_tile = min(
            len(fixed), max(1, int(np.sqrt(matrix_budget))))
        candidate_tile = max(1, matrix_budget // fixed_tile)
        for candidate_start in range(0, len(positions), candidate_tile):
            candidate_stop = min(
                len(positions), candidate_start + candidate_tile)
            nearest = np.full(
                candidate_stop - candidate_start,
                np.inf, dtype=np.float64)
            for fixed_start in range(0, len(fixed), fixed_tile):
                delta = (
                    positions[candidate_start:candidate_stop, None, :]
                    - fixed[None, fixed_start:fixed_start + fixed_tile, :])
                distance_sq = np.einsum(
                    "ijk,ijk->ij", delta, delta, optimize=False)
                _synthetic_note_matrix(stats, distance_sq.size)
                nearest = np.minimum(
                    nearest, distance_sq.min(axis=1))
            candidate_fixed_min_sq[
                candidate_start:candidate_stop] = nearest
    subset_spacing_sq = np.full(
        len(subsets), fixed_min_sq, dtype=np.float64)
    for column in range(cardinality):
        subset_spacing_sq = np.minimum(
            subset_spacing_sq,
            candidate_fixed_min_sq[subsets[:, column]])
        for right_column in range(column + 1, cardinality):
            delta = (
                positions[subsets[:, column]]
                - positions[subsets[:, right_column]])
            distance_sq = np.einsum(
                "ij,ij->i", delta, delta, optimize=False)
            _synthetic_note_matrix(stats, distance_sq.size)
            subset_spacing_sq = np.minimum(
                subset_spacing_sq, distance_sq)
    minimum_spacing = np.sqrt(subset_spacing_sq)

    best = None
    best_key = None
    for subset_row, subset in enumerate(subsets):
        row_tuple = tuple(sorted(
            int(rows[idx]) for idx in subset))
        key = (
            float(np.sqrt(worst_sq[subset_row])),
            -float(minimum_spacing[subset_row]), row_tuple)
        if best_key is None or key < best_key:
            best = tuple(int(idx) for idx in subset)
            best_key = key
    return best, best_key


def _synthetic_exact_k_cover(
        masks, positions, rows, full_mask, fixed_positions, spacing_mm,
        cardinality, node_budget, metric_batch_for, batch_size):
    """Bounded exact set-cover enumeration for cardinalities three and above."""
    spacing_sq = float(spacing_mm) ** 2
    best_indices = None
    best_key = None
    nodes = 0
    aborted = False
    feasible_batch = []
    cardinality_batch_size = max(1, int(batch_size) // int(cardinality))

    def compatible(idx, selected):
        point = positions[idx]
        for other in selected:
            delta = point - positions[other]
            if float(np.dot(delta, delta)) < spacing_sq:
                return False
        return True

    def flush():
        nonlocal best_indices, best_key, feasible_batch
        batch_best, batch_key = metric_batch_for(feasible_batch)
        if batch_key is not None and (best_key is None or batch_key < best_key):
            best_indices = batch_best
            best_key = batch_key
        feasible_batch = []

    def visit(selected, remaining, covered):
        nonlocal nodes, aborted
        nodes += 1
        if nodes > node_budget:
            aborted = True
            return
        slots = cardinality - len(selected)
        if covered == full_mask:
            if slots == 0:
                ordered = tuple(sorted(selected, key=lambda i: int(rows[i])))
                feasible_batch.append(ordered)
                if len(feasible_batch) >= cardinality_batch_size:
                    flush()
            return
        if slots <= 0 or len(remaining) < slots:
            return
        possible = covered
        for idx in remaining:
            possible |= masks[idx]
        if possible != full_mask:
            return
        uncovered = full_mask & ~covered
        pivot = uncovered & -uncovered
        coverers = tuple(
            idx for idx in remaining
            if masks[idx] & pivot and compatible(idx, selected))
        if not coverers:
            return
        coverer_set = set()
        for idx in coverers:
            if aborted:
                return
            coverer_set.add(idx)
            next_remaining = tuple(
                other for other in remaining
                if other != idx and other not in coverer_set
                and compatible(other, selected + (idx,)))
            visit(
                selected + (idx,), next_remaining,
                covered | masks[idx])

    visit((), tuple(range(len(masks))), 0)
    if feasible_batch:
        flush()
    return best_indices, best_key, nodes, not aborted


def _select_synthetic_component_rows(
        candidate_rows, sample_rows, specialized_component_positions,
        fixed_island_positions, pool, coverage_samples, cfg, stats):
    """Minimum regular A3 cover for one synthetic island/component."""
    matrix_budget = _synthetic_matrix_budget(cfg)
    mask_word_budget = _synthetic_mask_word_budget(cfg)
    radius = (
        float(cfg.target_spacing_mm)
        * float(cfg.coverage_radius_factor))
    spacing = float(cfg.target_spacing_mm)
    sample_positions = coverage_samples.positions[sample_rows]
    base_covered = _synthetic_covered_mask(
        specialized_component_positions, sample_positions,
        radius, matrix_budget, stats)
    if bool(np.all(base_covered)):
        metrics = _synthetic_selection_metrics(
            specialized_component_positions, fixed_island_positions,
            sample_positions, matrix_budget, stats)
        return (), metrics, 0, True

    rows = np.asarray(candidate_rows, dtype=np.int64)
    positions = pool.positions[rows]
    eligible = _synthetic_spacing_eligible(
        positions, fixed_island_positions, spacing, matrix_budget, stats)
    rows = rows[eligible]
    positions = positions[eligible]
    stats["eligible_rows"] = int(len(rows))
    if not len(rows):
        raise RuntimeError(
            "synthetic A3 coverage has no target-spaced candidate rows")

    needed_positions = sample_positions[~base_covered]
    masks = _synthetic_candidate_masks(
        positions, needed_positions, radius, matrix_budget,
        mask_word_budget, stats)
    if masks is None:
        stats["failure"] = "packed-mask-resource-bound"
        raise RuntimeError(
            "synthetic A3 coverage exact search refused by packed-mask "
            "resource bound; refusing an under-covered initial construct")
    useful = np.fromiter(
        (mask != 0 for mask in masks), dtype=bool, count=len(masks))
    rows = rows[useful]
    positions = positions[useful]
    masks = [mask for mask, keep in zip(masks, useful) if keep]
    full_mask = (1 << len(needed_positions)) - 1
    if not masks or int(np.bitwise_or.reduce(
            np.asarray(masks, dtype=object), initial=0)) != full_mask:
        raise RuntimeError(
            "synthetic A3 candidate pool cannot cover every prepared sample")

    def metrics_for(indices):
        selected_positions = positions[list(indices)]
        component_positions = (
            np.vstack((
                specialized_component_positions,
                selected_positions,
            ))
            if len(specialized_component_positions)
            else selected_positions)
        all_positions = (
            np.vstack((fixed_island_positions, selected_positions))
            if len(fixed_island_positions)
            else selected_positions)
        return _synthetic_selection_metrics(
            component_positions, all_positions, sample_positions,
            matrix_budget, stats)

    def metric_batch_for(index_sets):
        if index_sets:
            cardinality = len(index_sets[0])
            if cardinality >= 3:
                stats["max_higher_k_batch_entries"] = max(
                    int(stats.get("max_higher_k_batch_entries", 0)),
                    int(len(index_sets) * cardinality))
        return _synthetic_best_subset_batch(
            index_sets, positions, rows,
            specialized_component_positions, fixed_island_positions,
            sample_positions, matrix_budget, stats)

    node_budget = max(65_536, matrix_budget * 16)
    stats["search_node_budget"] = int(node_budget)

    # Cardinality one is exhaustive.
    stats["searched_cardinality"] = 1
    best = None
    best_key = None
    for idx, mask in enumerate(masks):
        if mask != full_mask:
            continue
        metrics = metrics_for((idx,))
        key = (float(metrics[0]), -float(metrics[1]), (int(rows[idx]),))
        if best_key is None or key < best_key:
            best = (idx,)
            best_key = key
    if best is not None:
        return (
            tuple(int(rows[i]) for i in best),
            (best_key[0], -best_key[1]), 1, True)

    # The common two-face synthetic-island case searches every pair exactly.
    stats["searched_cardinality"] = 2
    pair_trials = 0
    pair_complete = True
    best = None
    best_key = None
    spacing_sq = spacing ** 2
    feasible_pair_batch = []

    def flush_pair_batch():
        nonlocal best, best_key, feasible_pair_batch
        batch_best, batch_key = _synthetic_best_subset_batch(
            feasible_pair_batch, positions, rows,
            specialized_component_positions, fixed_island_positions,
            sample_positions, matrix_budget, stats)
        if batch_key is not None and (best_key is None or batch_key < best_key):
            best = batch_best
            best_key = batch_key
        feasible_pair_batch = []

    for left in range(len(masks) - 1):
        for right in range(left + 1, len(masks)):
            pair_trials += 1
            if pair_trials > node_budget:
                pair_complete = False
                break
            delta = positions[left] - positions[right]
            if float(np.dot(delta, delta)) < spacing_sq:
                continue
            if (masks[left] | masks[right]) != full_mask:
                continue
            feasible_pair_batch.append((left, right))
            if len(feasible_pair_batch) >= matrix_budget:
                flush_pair_batch()
        if not pair_complete:
            break
    if feasible_pair_batch:
        flush_pair_batch()
    stats["pairs_examined"] = int(pair_trials)
    if pair_complete and best is not None:
        selected_rows = tuple(sorted(int(rows[i]) for i in best))
        return (
            selected_rows, (best_key[0], -best_key[1]), 2, True)

    # For higher cardinalities, deterministic pivoted branch-and-bound is
    # exact until the explicit node cap. Greedy is computed only now: it is an
    # upper bound/fallback for this higher-k search, never a precondition for
    # the exhaustive cardinality-one/two rungs above. If greedy itself falls
    # into a spacing trap, bounded exact search may still prove a higher-k
    # solution; hitting the cap without a valid upper bound fails closed.
    greedy = _synthetic_greedy_cover(
        masks, positions, rows, full_mask, fixed_island_positions, spacing)
    greedy_metrics = metrics_for(greedy) if greedy is not None else None
    if not pair_complete:
        stats["fallback"] = "pair-search-node-bound"
        if greedy is None:
            raise RuntimeError(
                "synthetic A3 pair search hit its node bound without a "
                "valid target-spaced fallback")
        return (
            tuple(sorted(int(rows[i]) for i in greedy)),
            greedy_metrics, len(greedy), False)
    maximum_cardinality = len(greedy) if greedy is not None else len(masks)
    total_nodes = 0
    for cardinality in range(3, maximum_cardinality + 1):
        stats["searched_cardinality"] = int(cardinality)
        remaining_node_budget = node_budget - total_nodes
        if remaining_node_budget <= 0:
            stats["search_nodes"] = int(total_nodes)
            stats["fallback"] = "higher-cardinality-node-bound"
            if greedy is None:
                raise RuntimeError(
                    "synthetic A3 bounded exact search exhausted without a "
                    "valid target-spaced upper bound")
            return (
                tuple(sorted(int(rows[i]) for i in greedy)),
                greedy_metrics, len(greedy), False)
        exact, key, nodes, complete = _synthetic_exact_k_cover(
            masks, positions, rows, full_mask, fixed_island_positions,
            spacing, cardinality, remaining_node_budget,
            metric_batch_for, matrix_budget)
        total_nodes += int(nodes)
        if not complete:
            stats["search_nodes"] = int(total_nodes)
            stats["fallback"] = "higher-cardinality-node-bound"
            if greedy is None:
                raise RuntimeError(
                    "synthetic A3 bounded exact search exhausted without a "
                    "valid target-spaced upper bound")
            return (
                tuple(sorted(int(rows[i]) for i in greedy)),
                greedy_metrics, len(greedy), False)
        if exact is not None:
            stats["search_nodes"] = int(total_nodes)
            selected_rows = tuple(int(rows[i]) for i in exact)
            return (
                selected_rows, (key[0], -key[1]),
                cardinality, True)
    raise RuntimeError(
        "synthetic A3 exact search found no valid target-spaced cover")


def _select_synthetic_coverage_rows(
        pool, coverage_samples, face_topology, contacts,
        first_island_id, n_islands, cfg, stats_out=None):
    """Select deterministic, specialized-aware regular rows for new islands.

    Each named island/topology component is solved in increasing cardinality
    against every prepared A3 sample. Singles and the common pair case are
    exhaustive. Higher cardinalities use bounded exact branch-and-bound; its
    default cap is 1,048,576 nodes/component. If that cap is reached, only an
    already-proven fully covering, target-spaced greedy upper bound may be
    emitted. Packed masks and every distance tile have separate semantic
    bounds; mask refusal fails closed.
    """
    retained_authority_bytes = (
        int(pool.nbytes)
        + int(coverage_samples.compact_retained_bytes)
        + int(face_topology.compact_retained_bytes))
    stats = stats_out if stats_out is not None else {}
    stats.update(
        selected_count=0, selected_rows=(), components=[],
        searched_cardinality=0, pairs_examined=0,
        max_matrix_entries=0, packed_mask_words=0,
        requested_mask_words=0, candidate_count=0,
        needed_sample_count=0,
        configured_mask_word_limit=int(_synthetic_mask_word_budget(cfg)),
        retained_prepared_authority_bytes=retained_authority_bytes,
        estimated_retained_mask_bytes=0,
        bounded_distance_tile_bytes=0, packing_scratch_bytes=0,
        exact_search_scratch_bytes=0, planned_selector_peak_bytes=0,
        selector_max_peak_bytes=int(_SYNTHETIC_SELECTOR_MAX_PEAK_BYTES),
        max_higher_k_batch_entries=0, search_nodes=0, exact=True)

    def merge_route_stats(route_stats):
        stats["searched_cardinality"] = max(
            int(stats["searched_cardinality"]),
            int(route_stats["searched_cardinality"]))
        stats["pairs_examined"] += int(route_stats["pairs_examined"])
        stats["max_matrix_entries"] = max(
            int(stats["max_matrix_entries"]),
            int(route_stats["max_matrix_entries"]))
        stats["packed_mask_words"] += int(
            route_stats["packed_mask_words"])
        stats["search_nodes"] += int(route_stats["search_nodes"])
        stats["max_higher_k_batch_entries"] = max(
            int(stats["max_higher_k_batch_entries"]),
            int(route_stats.get("max_higher_k_batch_entries", 0)))
        requested = int(route_stats.get("requested_mask_words", 0))
        if requested >= int(stats["requested_mask_words"]):
            for name in (
                    "requested_mask_words", "candidate_count",
                    "needed_sample_count", "configured_mask_word_limit",
                    "estimated_retained_mask_bytes",
                    "bounded_distance_tile_bytes", "packing_scratch_bytes",
                    "exact_search_scratch_bytes",
                    "planned_selector_peak_bytes",
                    "selector_max_peak_bytes", "mask_preflight_passed",
                    "mask_refusal_reason"):
                if name in route_stats:
                    stats[name] = route_stats[name]

    component_ids = face_topology.component_ids
    selected_rows = []
    specialized_by_island = {}
    for contact in contacts:
        iid = int(contact.get("island_id", -1))
        if (first_island_id <= iid < n_islands
                and (contact.get("minimum")
                     or contact.get("island_birth"))):
            specialized_by_island.setdefault(iid, []).append(contact)

    for island_id in range(int(first_island_id), int(n_islands)):
        specialized = specialized_by_island.get(island_id, ())
        specialized_positions = (
            np.asarray(
                [contact["position"] for contact in specialized],
                dtype=np.float64)
            if specialized else np.empty((0, 3), dtype=np.float64))
        fixed_positions = specialized_positions.copy()
        specialized_components = {}
        for contact in specialized:
            face_id = contact.get("coverage_face_id")
            if face_id is None:
                continue
            component_id = int(component_ids[int(face_id)])
            specialized_components.setdefault(component_id, []).append(
                np.asarray(contact["position"], dtype=np.float64))

        pool_slice = pool.island_slice(island_id)
        island_pool_rows = np.arange(
            pool_slice.start, pool_slice.stop, dtype=np.int64)
        sample_slice = coverage_samples.island_slice(island_id)
        island_sample_rows = np.arange(
            sample_slice.start, sample_slice.stop, dtype=np.int64)
        sample_components = component_ids[
            coverage_samples.face_ids[island_sample_rows]]
        pool_components = component_ids[pool.face_ids[island_pool_rows]]
        for component_id in np.unique(sample_components):
            component_id = int(component_id)
            route_stats = dict(
                island_id=int(island_id),
                component_id=component_id,
                candidate_rows=int(np.count_nonzero(
                    pool_components == component_id)),
                sample_rows=int(np.count_nonzero(
                    sample_components == component_id)),
                eligible_rows=0, packed_mask_words=0,
                searched_cardinality=0, pairs_examined=0,
                max_matrix_entries=0, search_nodes=0,
                max_higher_k_batch_entries=0,
                retained_prepared_authority_bytes=retained_authority_bytes)
            stats["components"].append(route_stats)
            candidate_rows = island_pool_rows[
                pool_components == component_id]
            sample_rows = island_sample_rows[
                sample_components == component_id]
            component_specialized = np.asarray(
                specialized_components.get(component_id, ()),
                dtype=np.float64).reshape((-1, 3))
            try:
                rows, metrics, cardinality, exact = (
                    _select_synthetic_component_rows(
                    candidate_rows, sample_rows, component_specialized,
                    fixed_positions, pool, coverage_samples, cfg,
                    route_stats))
            finally:
                merge_route_stats(route_stats)
            route_stats.update(
                selected_rows=tuple(int(row) for row in rows),
                selected_count=int(len(rows)),
                selected_cardinality=int(cardinality),
                worst_sample_gap_mm=float(metrics[0]),
                min_selection_pair_spacing_mm=float(metrics[1]),
                exact=bool(exact))
            selected_rows.extend(rows)
            if rows:
                added_positions = pool.positions[
                    np.asarray(rows, dtype=np.int64)]
                fixed_positions = (
                    np.vstack((fixed_positions, added_positions))
                    if len(fixed_positions) else added_positions.copy())
            stats["exact"] = bool(stats["exact"] and exact)

    selected_rows = tuple(sorted(int(row) for row in selected_rows))
    stats["components"] = tuple(stats["components"])
    stats["selected_rows"] = selected_rows
    stats["selected_count"] = int(len(selected_rows))
    return selected_rows


def _regular_contact_from_pool_row(pool, row, islands, mesh):
    row = int(row)
    iid = int(pool.island_ids[row])
    return dict(
        position=pool.positions[row].copy(),
        normal=pool.normals[row].copy(),
        island_id=iid,
        face_id=int(pool.face_ids[row]),
        candidate_pool_row=row,
        area_mm2=float(np.asarray(mesh.area_faces, float)[
            np.asarray(islands[iid], dtype=np.int64)].sum()),
    )
