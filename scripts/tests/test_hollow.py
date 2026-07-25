"""The hollow/drill stage and its exit-code contract.

The thing under test is NOT the hollowing geometry (that is validated on real
prints, not in CI): it is the promise that a run which writes no file cannot
look like a run that did. Before the split, a failed self-verification set a
``status`` key inside a JSON blob on stdout and the tool exited 0.
"""
import json
import os
import subprocess
import sys

import numpy as np
import pytest
import trimesh

from support_placement import hollow, mesh_io

TOOLS = os.path.join(os.path.dirname(__file__), "..", "tools")
HOLLOW_DRILL = os.path.abspath(os.path.join(TOOLS, "hollow_drill.py"))
FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures", "chitubox")


def _export(mesh, path):
    mesh.export(str(path))
    return str(path)


@pytest.fixture(scope="module")
def solid_stl(tmp_path_factory):
    """A mini small enough that hollowing is not worth it: the standard
    overhang table at 0.7 scale measures 8.1mm of bulk, under the 10mm floor,
    so the decision is SOLID_below_threshold -- a SUCCESS that exports."""
    m = mesh_io.make_overhang_table()
    m.apply_scale(0.7)
    return _export(m, tmp_path_factory.mktemp("hollow") / "small_table.stl")


@pytest.fixture(scope="module")
def verify_failing_stl(tmp_path_factory):
    """A shape the hollower mangles: the toed slab hollows (10.8mm bulk) and
    then fails step 8. Same failure class as the Cylinder fixture, ~4x faster.
    If the algorithm ever learns to handle this shape, this fixture must be
    re-picked -- the point is to exercise the FAILURE PATH, not to freeze this
    particular mesh's fate."""
    return _export(mesh_io.make_toed_slab(),
                   tmp_path_factory.mktemp("hollow") / "toed_slab.stl")


# ---------------------------------------------------------------------------
# step 0: load-time component triage
# ---------------------------------------------------------------------------

def _one_body():
    """A single-body ball: 8.6mm of bulk, under the 10mm floor, so the
    decision is SOLID and the triaged mesh is exported -- which lets a test
    read the result back out of the tool's own output. (A box is no good
    here: it voxelizes with no background, so the EDT reads it as unbounded
    bulk and it hollows.)"""
    return trimesh.creation.icosphere(radius=4.0)


def _pinched_tetra(apex, edge, inward):
    """A 4-face tetrahedron hanging off ``apex`` by that ONE vertex.

    This is the shape of the real defect (measured on 8 of 37 production
    models): it touches the body at a bit-identical vertex position and
    shares no edge with it, so a bounding-box or vertex-distance test reads
    it as attached while an edge test reads it as separable. ``inward``
    flips the winding, making it a void rather than a solid -- the closed
    shards in the corpus are wound that way.
    """
    v = np.array([apex,
                  apex + np.array([edge, 0.0, 0.0]),
                  apex + np.array([0.0, edge, 0.0]),
                  apex + np.array([0.0, 0.0, edge])])
    f = np.array([[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]])
    t = trimesh.Trimesh(vertices=v, faces=f)
    trimesh.repair.fix_normals(t)
    if inward:
        t.invert()
    return t


def _body_plus(edge, inward, path):
    """The slab with a tetrahedron pinched onto its top-far corner."""
    body = _one_body()
    apex = body.vertices[np.lexsort(body.vertices.T)[-1]]
    tetra = _pinched_tetra(apex, -edge if inward else edge, inward)
    return _export(trimesh.util.concatenate([body, tetra]), path)


@pytest.fixture(scope="module")
def shard_stl(tmp_path_factory):
    """Body + a bowtie-pinched degenerate shard that the predicate drops.

    0.2mm edges put it squarely inside the measured envelope of the real
    thing: the 13 shards in the 37-model corpus run 0.0385-0.2561mm across
    and enclose at most 0.00012mm3, and this one is 0.2mm enclosing
    0.0013mm3. It was 0.5mm/0.021mm3 while the volume clause was one-sided
    -- deliberately OVER the 0.01mm3 cap, to pin the fact that an
    inward-wound shell was dropped whatever its size. That clause is gone
    (it dropped a 1.53mm gem enclosing 1.85mm3), so the fixture is now sized
    like the defect it stands for instead of like the loophole."""
    return _body_plus(0.2, inward=True,
                      path=tmp_path_factory.mktemp("hollow") / "shard.stl")


@pytest.fixture(scope="module")
def small_solid_extra_stl(tmp_path_factory):
    """Body + a small pinched component that is NOT junk: a real
    outward-wound solid of 0.167mm3, 17x the 0.01mm3 cap, and 1mm across
    where the largest real shard is 0.256mm. Four faces and one shared
    vertex are all it has in common with the junk profile, so this pins that
    a genuine small part survives triage."""
    return _body_plus(1.0, inward=False,
                      path=tmp_path_factory.mktemp("hollow") / "extra.stl")


@pytest.fixture(scope="module")
def one_body_stl(tmp_path_factory):
    return _export(_one_body(), tmp_path_factory.mktemp("hollow") / "slab.stl")


@pytest.fixture(scope="module")
def pre_hollowed_stl(tmp_path_factory):
    """A model that arrived already hollowed: r=20 sphere with an r=12
    spherical void. Watertight, is_volume True -- it sails through the only
    input gate that existed before, and step 2's voxel fill then reads the
    void as bulk and carves a replacement cavity."""
    outer = trimesh.creation.icosphere(subdivisions=2, radius=20.0)
    inner = trimesh.creation.icosphere(subdivisions=2, radius=12.0)
    inner.invert()
    m = trimesh.util.concatenate([outer, inner])
    return _export(m, tmp_path_factory.mktemp("hollow") / "pre_hollowed.stl")


def test_bowtie_shard_is_dropped_and_the_drop_is_reported(shard_stl, tmp_path):
    out = tmp_path / "clean.stl"
    log = hollow.run(shard_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["components"] == 2
    assert log["dropped_shard_faces"] == 4
    assert len(log["dropped_shards"]) == 1
    (shard,) = log["dropped_shards"]
    assert shard["faces"] == 4
    assert max(shard["dims_mm"]) == pytest.approx(0.2, abs=1e-3)

    result = trimesh.load(str(out))
    assert len(result.split(only_watertight=False)) == 1
    assert result.is_watertight, "dropping an edge-disjoint component cannot open a hole"
    assert result.is_volume
    body = _one_body()
    assert len(result.faces) == len(body.faces)
    assert result.volume == pytest.approx(body.volume, rel=1e-6)
    assert np.allclose(result.bounds, body.bounds, atol=1e-4)


def test_a_small_real_solid_is_not_a_shard(small_solid_extra_stl, tmp_path):
    """Failing any one clause keeps the component. The predicate is a DROP
    test on named evidence, not "keep the largest component"."""
    out = tmp_path / "kept.stl"
    log = hollow.run(small_solid_extra_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["components"] == 2
    assert "dropped_shards" not in log
    assert len(trimesh.load(str(out)).split(only_watertight=False)) == 2


def test_pre_hollowed_input_is_refused_before_anything_is_rehollowed(
        pre_hollowed_stl, tmp_path):
    out = tmp_path / "must_not_exist.stl"
    log = {}
    with pytest.raises(hollow.PreHollowedError) as ei:
        hollow.run(pre_hollowed_stl, str(out), orient=False, log=log)
    assert not out.exists()
    assert "already hollowed" in str(ei.value)
    # refused, NOT re-hollowed: the hollow decision was never reached
    assert "decision" not in log and "bulk_mm" not in log
    # and the cavity that triggered it is named, not quietly discarded
    assert log["cavity_components"] and log["cavity_components"][0]["faces"] > 64
    assert "dropped_shards" not in log


def test_pre_hollowed_refusal_is_a_bad_input_not_a_status(pre_hollowed_stl, tmp_path):
    """Per the module's status contract, bad INPUTS raise rather than
    reporting a status, so callers that only know ``HollowError`` still
    handle this one correctly."""
    assert issubclass(hollow.PreHollowedError, hollow.HollowError)
    with pytest.raises(hollow.HollowError):
        hollow.run(pre_hollowed_stl, str(tmp_path / "x.stl"), orient=False)


def test_a_sound_single_body_mesh_is_left_alone(one_body_stl, tmp_path):
    log = hollow.run(one_body_stl, str(tmp_path / "s.stl"), orient=False)
    assert log["components"] == 1
    assert "dropped_shards" not in log and "cavity_components" not in log


def test_multi_body_input_is_neither_stripped_nor_refused(solid_stl, tmp_path):
    """The overhang table is five overlapping boxes. Detached bodies are not
    this stage's business and never were -- only inward-wound shells (real
    cavities) and degenerate shards are."""
    out = tmp_path / "table.stl"
    log = hollow.run(solid_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["components"] == 5
    assert len(log["extra_bodies"]) == 4
    assert "dropped_shards" not in log and "cavity_components" not in log
    assert len(trimesh.load(str(out)).split(only_watertight=False)) == 5


# ---------------------------------------------------------------------------
# step 0, part 2: what makes a component a CAVITY is containment, not winding
#
# The first cut of triage asked "is this a closed, consistently-wound shell of
# negative volume?" and picked the main body by face count. Both halves of
# that are wrong, and an adversarial review produced working inputs for each
# failure. Every scenario below is one of those inputs.
# ---------------------------------------------------------------------------

def _nested(outer_r, outer_sub, inner_r, inner_sub, flip_one=False, offset=(0, 0, 0)):
    """An already-hollowed model: an inward-wound shell inside a closed body.
    ``flip_one`` reverses a single cavity triangle; ``offset`` moves the void
    off-centre so the symmetry that lets ``volume_balance`` notice is gone."""
    outer = trimesh.creation.icosphere(subdivisions=outer_sub, radius=outer_r)
    inner = trimesh.creation.icosphere(subdivisions=inner_sub, radius=inner_r)
    inner.apply_translation(offset)
    inner.invert()
    if flip_one:
        f = inner.faces.copy()
        f[0] = f[0][::-1]
        inner = trimesh.Trimesh(vertices=inner.vertices, faces=f, process=False)
    m = trimesh.util.concatenate([outer, inner])
    m.merge_vertices()
    return m


def test_one_flipped_triangle_does_not_hide_a_cavity(tmp_path):
    """Reversing a SINGLE triangle on the cavity shell made
    ``is_winding_consistent`` False, the cavity branch missed, and the shell
    was filed under ``extra_bodies``. Measured end to end on this exact
    input: 0.506ml of real void destroyed, ``volume_balance`` PASSED at 0.52
    vs a 0.67ml tolerance, file written, status OK. That tolerance is 2% of
    filled volume, so on a 20ml mini any pre-existing void under ~0.4ml (a
    9mm sphere) is invisible from end to end -- which is why this has to be
    caught here rather than backstopped in step 8."""
    m = _nested(20.0, 3, 5.0, 2, flip_one=True, offset=(3, 2, 4))
    void = [c for c in m.split(only_watertight=False) if len(c.faces) == 320][0]
    assert not void.is_winding_consistent, "the evasion is a winding evasion"
    assert abs(void.volume) / 1000.0 == pytest.approx(0.506, abs=0.01)

    out = tmp_path / "must_not_exist.stl"
    log = {}
    with pytest.raises(hollow.PreHollowedError):
        hollow.run(_export(m, tmp_path / "flipped.stl"), str(out), orient=False)
    assert not out.exists()
    assert log is not None
    assert "decision" not in log


def test_a_cavity_with_more_faces_than_the_body_is_still_a_cavity(tmp_path):
    """``main = argmax(face_count)`` crowned the VOID as the body whenever the
    cavity was tessellated more finely than the wall around it -- routine, and
    here an r=20 shell at 320 faces around an r=12 void at 1280. The real body
    became an "extra body", the cavity was never tested, and the model was
    hollowed over with status OK. Extent cannot invert like that: a contained
    shell has the smaller bounding box, by construction."""
    m = _nested(20.0, 2, 12.0, 3)
    out = tmp_path / "must_not_exist.stl"
    log = {}
    with pytest.raises(hollow.PreHollowedError):
        hollow.run(_export(m, tmp_path / "fine_cavity.stl"), str(out),
                   orient=False, log=log)
    assert not out.exists()
    # the CAVITY is what got named, so main really was the 320-face body
    assert [c["faces"] for c in log["cavity_components"]] == [1280]
    assert "extra_bodies" not in log


def test_an_off_centre_cavity_is_refused_where_volume_balance_could_not_help(tmp_path):
    """The asymmetric case the step-8 balance check cannot be relied on for:
    an off-centre void in an ellipsoid, finer than its shell."""
    m = _nested(20.0, 2, 12.0, 3, offset=(6.0, 0, 0))
    m.vertices[:, 0] *= 1.6
    out = tmp_path / "must_not_exist.stl"
    with pytest.raises(hollow.PreHollowedError):
        hollow.run(_export(m, tmp_path / "asym.stl"), str(out), orient=False)
    assert not out.exists()


def test_a_fully_inverted_multi_body_export_is_repaired_not_refused(tmp_path):
    """A whole file exported inside-out -- a mini plus a separately modelled
    base -- is a classic bad export, and ``fix_normals`` handled it correctly
    before triage existed. The winding-sign test read the inverted base as an
    "internal cavity shell" and raised, turning a repairable file into a
    batch-stopper. Neither body is inside the other, so containment agrees
    with ``fix_normals``: there is no void here."""
    base = trimesh.creation.cylinder(radius=8.0, height=3.0, sections=48)
    base.apply_translation([0, 0, -10])
    mini = trimesh.creation.icosphere(radius=6.0, subdivisions=3)
    whole = trimesh.util.concatenate([mini, base])
    whole.invert()
    whole.merge_vertices()
    assert not whole.is_volume, "the input really is inside-out"

    out = tmp_path / "repaired.stl"
    log = hollow.run(_export(whole, tmp_path / "inverted.stl"), str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert "cavity_components" not in log and "dropped_shards" not in log
    result = trimesh.load(str(out))
    assert len(result.split(only_watertight=False)) == 2, "both bodies survive"
    assert result.volume > 0, "fix_normals got its chance after all"


def test_an_inward_wound_shell_far_from_the_body_is_not_a_cavity():
    """Containment was never checked, so ANY inward-wound closed shell was
    reported as an internal cavity -- including a box 200mm away from the
    model, which is not inside anything."""
    mini = trimesh.creation.icosphere(subdivisions=3, radius=10.0)
    far = trimesh.creation.box(extents=[5, 5, 5])
    far.apply_translation([200, 0, 0])
    far.invert()
    m = trimesh.util.concatenate([mini, far])
    m.merge_vertices()
    log = {}
    hollow.triage_components(m, log)
    assert "cavity_components" not in log
    assert [b["faces"] for b in log["extra_bodies"]] == [12]


def test_an_inward_wound_bead_outside_the_body_is_not_a_cavity():
    """The near version of the same thing: a separately modelled eye/gem
    exported inward-wound, sitting proud of the surface. Too big for the
    shard predicate, not inside the body, so it is neither dropped nor
    refused -- it is a body, and ``fix_normals`` deals with it."""
    mini = trimesh.creation.icosphere(subdivisions=3, radius=10.0)
    eye = trimesh.creation.icosphere(subdivisions=2, radius=1.5)
    eye.apply_translation([0, 0, 12.0])
    eye.invert()
    m = trimesh.util.concatenate([mini, eye])
    m.merge_vertices()
    log = {}
    hollow.triage_components(m, log)
    assert "cavity_components" not in log
    assert [b["faces"] for b in log["extra_bodies"]] == [320]


# ---------------------------------------------------------------------------
# step 0, part 3: the drop predicate must not eat real parts
# ---------------------------------------------------------------------------

def _gem(scale=0.9, translate=(0, 0, 0)):
    """A ~1.53mm faceted gem -- small, but a real modelled feature."""
    g = trimesh.creation.icosahedron()
    g.apply_scale(scale)
    g.apply_translation(translate)
    return g


def test_an_open_shell_part_is_never_dropped():
    """An open patch is not a shard. It was being dropped because the volume
    clause only protected components that ``is_volume`` called solids, so
    "not closed" read as "encloses nothing worth keeping" whatever its actual
    size: this 1.53mm gem, modelled without its bottom cap, went with it.
    Openness now keeps a component rather than condemning it -- trimesh's
    ``volume`` on an open patch is an origin-dependent divergence sum, not a
    size, so it cannot be the evidence for a deletion."""
    body = trimesh.creation.icosphere(radius=4.0)
    apex = body.vertices[np.lexsort(body.vertices.T)[-1]]
    gem = _gem(translate=apex + np.array([0.4, 0.4, 0.4]))
    gem.update_faces(gem.face_normals[:, 2] > -0.5)  # cut the bottom cap off
    gem.remove_unreferenced_vertices()
    assert not gem.is_watertight and max(gem.extents) < hollow.SHARD_MAX_DIM_MM * 2

    m = trimesh.util.concatenate([body, gem])
    m.merge_vertices()
    log = {}
    r = hollow.triage_components(m, log)
    assert "dropped_shards" not in log
    assert len(r.faces) == len(body.faces) + len(gem.faces)


def test_an_inverted_small_solid_is_kept_for_fix_normals_to_repair(tmp_path):
    """Triage runs BEFORE ``fix_normals``, so a component it deletes never
    gets the chance to be repaired. This gem is a genuine solid enclosing
    1.85mm3 -- 185x the 0.01mm3 cap -- and the one-sided volume clause
    dropped it anyway, because inverted normals make ``is_volume`` False.
    Magnitude is the question, so magnitude is what is measured now."""
    body = trimesh.creation.icosphere(radius=4.0)
    apex = body.vertices[np.lexsort(body.vertices.T)[-1]]
    gem = _gem(translate=apex + np.array([0.4, 0.4, 0.4]))
    gem.invert()
    assert abs(gem.volume) == pytest.approx(1.85, abs=0.05)
    assert max(gem.extents) == pytest.approx(1.53, abs=0.01)

    m = trimesh.util.concatenate([body, gem])
    m.merge_vertices()
    out = tmp_path / "kept.stl"
    log = hollow.run(_export(m, tmp_path / "inv_gem.stl"), str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert "dropped_shards" not in log
    assert len(trimesh.load(str(out)).split(only_watertight=False)) == 2


def test_a_spray_of_sub_millimetre_open_shells_survives():
    """Twenty sub-millimetre open half-shells -- a spell effect, a spray of
    sparks -- were ALL deleted: each one is under the size caps, has few
    faces, shares no edge, and is not a positively-wound solid. Nothing about
    them says "modelling artefact of the body": they are not pinched to it,
    which is the evidence the predicate now requires."""
    mini = trimesh.creation.icosphere(subdivisions=3, radius=10.0)
    rng = np.random.default_rng(0)
    parts = [mini]
    for _ in range(20):
        s = trimesh.creation.icosahedron()
        s.apply_scale(0.55)
        s.update_faces(s.face_normals[:, 2] > 0)
        s.remove_unreferenced_vertices()
        s.apply_translation(rng.uniform(-14, 14, 3))
        parts.append(s)
    m = trimesh.util.concatenate(parts)
    m.merge_vertices()
    n_faces = len(m.faces)
    log = {}
    r = hollow.triage_components(m, log)
    assert log["components"] == 21
    assert "dropped_shards" not in log and "cavity_components" not in log
    assert len(r.faces) == n_faces


# ---------------------------------------------------------------------------
# step 0, part 4: hole-safety, and the precondition
# ---------------------------------------------------------------------------

def _flap_on_shortest_edge(mesh, height=0.15):
    """A one-triangle flap standing on ``mesh``'s shortest edge. The edge
    becomes 3-valent, so trimesh -- which builds face adjacency only across
    2-valent edges -- files the flap as its own component even though it is
    genuinely welded to that mesh along a whole edge. Dropping it would open
    a hole."""
    eu = mesh.edges_unique
    lens = np.linalg.norm(mesh.vertices[eu[:, 0]] - mesh.vertices[eu[:, 1]], axis=1)
    v0, v1 = mesh.vertices[eu[int(np.argmin(lens))]]
    apex = (v0 + v1) / 2 + np.array([0.0, 0.0, height])
    return trimesh.Trimesh(vertices=np.array([v0, v1, apex]),
                           faces=np.array([[0, 1, 2]]), process=False)


def test_a_flap_joined_to_the_body_survives_a_denser_accessory():
    """Hole-safety was measured against ``main``, and ``main`` was whichever
    component had the most faces. A 1600-face body next to a 5120-face
    accessory made the ACCESSORY main, so a flap genuinely edge-joined to the
    body read as edge-disjoint and was dropped -- opening a hole in the thing
    it was welded to. Extent picks the body, and the edge test now asks about
    every component rather than only main."""
    body = trimesh.creation.cylinder(radius=10.0, height=1.0, sections=400)
    acc = trimesh.creation.icosphere(subdivisions=4, radius=2.5)
    acc.apply_translation([30, 0, 0])
    flap = _flap_on_shortest_edge(body)
    m = trimesh.util.concatenate([body, acc, flap])
    m.merge_vertices()
    n_faces = len(m.faces)
    log = {}
    r = hollow.triage_components(m, log)
    assert log["components"] == 3
    assert "dropped_shards" not in log
    assert len(r.faces) == n_faces


def test_a_shard_edge_joined_to_a_DETACHED_body_is_also_safe():
    """Edge-disjointness was only ever measured against ``main``, but the
    main body is not the only thing this stage keeps, so it is not the only
    thing a removal can hole.

    The component here is the corpus's own junk shape -- a 2-face
    double-sided triangle, watertight, enclosing exactly nothing, pinched to
    a main-body vertex, well inside every size cap -- so it matches the drop
    predicate on every clause except one: its third edge is bit-identical
    with an edge of a detached accessory that is far too big to drop.
    Removing it would leave that accessory with a boundary edge."""
    main = trimesh.creation.icosphere(radius=4.0)
    n = np.ones(3) / np.sqrt(3.0)
    w = main.vertices[int(np.argmax(main.vertices @ n))]  # a main-body vertex
    acc = trimesh.creation.icosphere(subdivisions=2, radius=0.8)
    acc.apply_translation(w + n * 0.9)  # perched just off the surface at w
    eu = acc.edges_unique
    near = int(np.argmin(np.linalg.norm(acc.vertices[eu].mean(axis=1) - w, axis=1)))
    v0, v1 = acc.vertices[eu[near]]
    # two coincident triangles = closed, zero-volume, and joined to `acc`
    # along (v0, v1), which that shared edge makes 4-valent -- so trimesh
    # files this as its own component while it is genuinely welded on
    shard = trimesh.Trimesh(vertices=np.array([v0, v1, w]),
                            faces=np.array([[0, 1, 2], [0, 2, 1]]), process=False)
    m = trimesh.util.concatenate([main, acc, shard])
    m.merge_vertices()
    assert len(m.split(only_watertight=False)) == 3
    n_faces = len(m.faces)

    log = {}
    r = hollow.triage_components(m, log)
    assert "dropped_shards" not in log
    assert len(r.faces) == n_faces
    # and the accessory still has no boundary edge
    (kept_acc,) = [c for c in r.split(only_watertight=False)
                   if len(c.faces) == len(acc.faces)]
    assert kept_acc.is_watertight


def test_an_unwelded_mesh_is_welded_rather_than_shredded(tmp_path):
    """Components come from face adjacency, which is built on shared vertex
    INDICES. An unwelded mesh -- what ``process=False`` gives you, and what a
    raw STL's triangle soup is -- has none, so every triangle was its own
    component: 1280 faces in, 1 face out, no error, nothing logged. The
    precondition is now enforced instead of assumed."""
    mini = trimesh.creation.icosphere(subdivisions=3, radius=5.0)
    raw = trimesh.load(_export(mini, tmp_path / "raw.stl"), process=False)
    assert len(raw.vertices) == 3 * len(raw.faces), "genuinely unwelded"

    log = {}
    r = hollow.triage_components(raw, log)
    assert log["components"] == 1
    assert len(r.faces) == len(mini.faces)
    assert "dropped_shards" not in log
    assert r.is_watertight


# ---------------------------------------------------------------------------
# the library: every return path sets a status
# ---------------------------------------------------------------------------

def test_solid_decision_is_a_success_and_exports(solid_stl, tmp_path):
    out = tmp_path / "solid.stl"
    log = hollow.run(solid_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["decision"].startswith("SOLID")
    assert log["exported"] == str(out)
    assert out.is_file() and out.stat().st_size > 0
    assert hollow.failed_checks(log) == []


def test_verify_failure_writes_nothing_and_names_the_checks(verify_failing_stl, tmp_path):
    out = tmp_path / "must_not_exist.stl"
    log = hollow.run(verify_failing_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_VERIFY_FAILED
    assert not out.exists(), "a failed verification must not export"
    failed = hollow.failed_checks(log)
    assert failed, "a failed run must name at least one check"
    assert set(failed) <= set(hollow.CHECK_DESCRIPTIONS)
    assert log["failed_checks"] == failed
    lines = hollow.describe_failure(log)
    assert len(lines) == len(failed)
    for name, line in zip(failed, lines):
        assert line.startswith(name + ":")
        assert "measured" in line


def test_check_detail_covers_every_documented_check(verify_failing_stl, tmp_path):
    log = hollow.run(verify_failing_stl, str(tmp_path / "x.stl"), orient=False)
    assert set(log["check_detail"]) == set(hollow.CHECK_DESCRIPTIONS)


def test_non_volume_input_raises_rather_than_reporting(tmp_path):
    """A torn mesh is a bad input, not an outcome of the algorithm."""
    m = trimesh.Trimesh(vertices=[[0, 0, 0], [1, 0, 0], [0, 1, 0]], faces=[[0, 1, 2]])
    p = _export(m, tmp_path / "sheet.stl")
    with pytest.raises(hollow.HollowError):
        hollow.run(p, str(tmp_path / "out.stl"), orient=False)


def test_helpers_are_quiet_on_a_log_with_no_checks():
    assert hollow.failed_checks({}) == []
    assert hollow.describe_failure({"decision": "SOLID_no_cavity"}) == []


# ---------------------------------------------------------------------------
# the CLI: the exit code is the contract
# ---------------------------------------------------------------------------

def _run_cli(*argv):
    return subprocess.run([sys.executable, HOLLOW_DRILL, *argv],
                          capture_output=True, text=True)


def test_cli_exits_zero_on_a_solid_decision(solid_stl, tmp_path):
    out = tmp_path / "solid.stl"
    r = _run_cli(solid_stl, str(out), "--no-orient")
    assert r.returncode == 0, r.stderr
    assert out.is_file()
    assert json.loads(r.stdout)["status"] == hollow.STATUS_OK


def test_cli_exits_two_on_a_verify_failure(verify_failing_stl, tmp_path):
    out = tmp_path / "nope.stl"
    report = tmp_path / "r.json"
    r = _run_cli(verify_failing_stl, str(out), "--no-orient", "--report", str(report))
    assert r.returncode == 2, r.stdout
    assert not out.exists()
    # the operator must be told WHICH check failed, on stderr, unmissably
    assert "FAILED self-verification" in r.stderr
    assert "FAILED CHECK" in r.stderr
    payload = json.loads(report.read_text())
    assert payload["status"] == hollow.STATUS_VERIFY_FAILED
    assert payload["failed_checks"]
    for name in payload["failed_checks"]:
        assert name in r.stderr
    # the machine-readable record survives the failure on stdout too
    assert json.loads(r.stdout)["failed_checks"] == payload["failed_checks"]


def test_cli_exits_one_on_pre_hollowed_input(pre_hollowed_stl, tmp_path):
    """Already-hollowed is a bad input (exit 1), not a verification failure
    (exit 2): nothing about the algorithm went wrong, the file is for the
    drill-only path. ``error_type`` is what tells a caller which."""
    out = tmp_path / "nope.stl"
    r = _run_cli(pre_hollowed_stl, str(out), "--no-orient")
    assert r.returncode == 1, r.stderr
    assert not out.exists()
    assert "already hollowed" in r.stderr
    assert "drill-only" in r.stderr
    payload = json.loads(r.stdout)
    assert payload["status"] == "ERROR"
    assert payload["error_type"] == "PreHollowedError"


def test_cli_exits_one_on_a_missing_input(tmp_path):
    r = _run_cli(str(tmp_path / "no_such.stl"), str(tmp_path / "o.stl"))
    assert r.returncode == 1
    assert "no such STL" in r.stderr
    assert json.loads(r.stdout)["status"] == "ERROR"


@pytest.mark.parametrize("name", ["Cylinder.stl"])
def test_cli_exit_code_on_the_committed_cylinder_fixture(name, tmp_path):
    """The originally-reported case (task #79): the 30mm cylinder hollows into
    4 bodies with a sealed cavity. Kept as a fixture-backed regression so the
    reported bug has a test that names it."""
    out = tmp_path / "cyl.stl"
    r = _run_cli(os.path.join(FIXTURES, name), str(out), "--no-orient", "--quiet")
    assert r.returncode == 2
    assert not out.exists()
    assert "single_body" in r.stderr or "cavity_breathes" in r.stderr
