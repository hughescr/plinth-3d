"""From-scratch segment mesh synthesis + injection (design.md #8).

Cross-validated (not guessed): the capped-frustum TOPOLOGY here was
reverse-engineered directly from spike_SYNTH2.chitubox's already-injected
support -- 24-sided capped frustums for tip/taper/shaft/flare/joiner,
2 side triangles per facet, both ends capped with a center-point fan.
Winding is enforced outward via a signed-volume check rather than
byte-matching the specimen's exact vertex order (the specimen's chirality
depends on a local basis choice this module doesn't need to replicate --
outward-normal-consistent is the actual requirement). Trailing 72-byte-record fields
(bytes 48:72, mostly unmapped) were checked against the full
supported-treant.chitubox corpus: types 7/9/3/4 are uniformly zero across
every real record of that type; type 1 (taper) carries the contact-depth
float in slot 0 (varies with profile, matches design's own
cfg.contact_depth_mm concept) -- both choices below are evidenced, not
invented.

PAD and BRIDGE (type 6) are NOT synthesized from scratch here -- per the
sibling chitubox-support-injection skill's doctrine ("clone + patch,
never author from scratch") and its own code, only clone-based pad/bridge
placement is validated. This module's pad/bridge helpers therefore
translate a template record from the base project, same as
chitubox_inject.clone_support -- see place_pad().
"""
import sys
import os
import struct
import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
import chitubox_inject as ci  # noqa: E402

N_SIDES = 24
CONTACT_DEPTH_TRAILING_TYPE = ci.T_TAPER


def frustum_mesh(start, end, r0, r1, n_sides=N_SIDES):
    """Capped frustum triangle soup from start (radius r0) to end (radius
    r1), matching the validated Chitubox synthesized-segment TOPOLOGY
    (reverse-engineered from spike_SYNTH2.chitubox): two rings of n_sides
    perimeter points + one center point per ring, side walls as 2
    triangles per side, capped both ends. Winding is self-corrected to be
    outward/positive-volume via a signed-volume check rather than
    hand-tracking handedness through the local basis -- robust regardless
    of axis direction.

    Returns float32 array shape (n_tri, 3, 3): (n_tri, vertex, xyz).
    """
    start = np.asarray(start, float)
    end = np.asarray(end, float)
    axis = end - start
    L = np.linalg.norm(axis)
    axis_n = axis / L if L > 1e-9 else np.array([0.0, 0.0, 1.0])
    # any vector not parallel to axis_n, for a stable perpendicular basis
    ref = np.array([1.0, 0.0, 0.0]) if abs(axis_n[0]) < 0.9 else np.array([0.0, 1.0, 0.0])
    u = np.cross(axis_n, ref)
    u /= np.linalg.norm(u)
    v = np.cross(axis_n, u)

    ang = 2 * np.pi * np.arange(n_sides) / n_sides
    ring0 = start[None, :] + r0 * (np.cos(ang)[:, None] * u + np.sin(ang)[:, None] * v)
    ring1 = end[None, :] + r1 * (np.cos(ang)[:, None] * u + np.sin(ang)[:, None] * v)

    tris = []
    for i in range(n_sides):
        j = (i + 1) % n_sides
        tris.append([ring0[i], ring1[i], ring1[j]])
        tris.append([ring0[i], ring1[j], ring0[j]])

    # cap winding: pick the order whose normal points away from the solid
    # (start cap -> -axis_n side, end cap -> +axis_n side), verified
    # against the real specimen (opposite fan direction at each end).
    def fan(center, ring, outward_sign):
        out = []
        for i in range(n_sides):
            j = (i + 1) % n_sides
            tri = [center, ring[i], ring[j]]
            n = np.cross(ring[i] - center, ring[j] - center)
            if np.dot(n, axis_n) * outward_sign < 0:
                tri = [center, ring[j], ring[i]]
            out.append(tri)
        return out

    if r0 > 1e-9:
        tris += fan(start, ring0, outward_sign=-1.0)
    if r1 > 1e-9:
        tris += fan(end, ring1, outward_sign=+1.0)

    out = np.array(tris, dtype="<f4")
    # Self-correct overall winding to outward/positive-volume: the basis
    # (u, v) chosen above can come out either chirality depending on
    # axis_n, which would otherwise silently flip every normal. Delegate
    # the signed-volume check to trimesh's mass_properties (Mirtich's
    # algorithm) rather than a hand-rolled origin-referenced formula --
    # verified during development that a naive sum(v0.(v1xv2))/6 can
    # disagree with it on an unwelded triangle-soup frustum even though it
    # matches on a simple closed box; trimesh's is the one every
    # downstream consumer (including this package's own tests) actually
    # trusts, so check against the same implementation being validated
    # against.
    import trimesh
    signed_vol = trimesh.triangles.mass_properties(
        triangles=out.astype(np.float64), skip_inertia=True).volume
    if signed_vol < 0:
        out = out[:, [0, 2, 1], :]
    return out


def sphere_mesh(center, r, n_lat=8, n_lon=16):
    """UV-sphere triangle soup (float32 (n_tri,3,3)), winding corrected
    the same way frustum_mesh does it. Used for joint balls, tip contact
    spheres, and trunk-top domes (v3 review findings #2/#5: angled
    frustum-to-frustum joints leave lens gaps; cylinder-into-surface
    contacts scar worse and remove harder than sphere contacts)."""
    center = np.asarray(center, float)
    lat = np.linspace(0.0, np.pi, n_lat + 1)
    lon = np.linspace(0.0, 2 * np.pi, n_lon, endpoint=False)

    def pt(a, l):
        return center + r * np.array([np.sin(a) * np.cos(l),
                                       np.sin(a) * np.sin(l),
                                       np.cos(a)])

    tris = []
    for a0, a1 in zip(lat[:-1], lat[1:]):
        for k in range(n_lon):
            l0, l1 = lon[k], lon[(k + 1) % n_lon]
            p00, p01 = pt(a0, l0), pt(a0, l1)
            p10, p11 = pt(a1, l0), pt(a1, l1)
            # quad (p00,p01,p10,p11) -> two triangles; the one that uses
            # both TOP vertices degenerates at the north pole (a0=0) and
            # the one using both BOTTOM vertices at the south pole (a1=pi)
            if a1 < np.pi - 1e-9:
                tris.append([p00, p10, p11])
            if a0 > 1e-9:
                tris.append([p00, p11, p01])
    out = np.array(tris, dtype="<f4")
    import trimesh
    signed_vol = trimesh.triangles.mass_properties(
        triangles=out.astype(np.float64), skip_inertia=True).volume
    if signed_vol < 0:
        out = out[:, [0, 2, 1], :]
    return out


def _ball_meshes(extra_balls):
    return [sphere_mesh(c, r) for c, r in (extra_balls or [])]


def synth_v3_support(route, r_tip, r_shoot, cfg, next_off, extra_balls=()):
    """v3 per-contact chain: spherical CONTACT BALL merged into the tip
    cone blob (removal-friendly point contact) + diagonal shoot rod.

    v3.6: joint balls (elbow, junction, hub, strut foot) are no longer
    embedded here -- the global joint pass (joints.compute_joint_balls)
    sizes ONE ball per joint at final radii and hands this route's share
    in via `extra_balls` [(center, r), ...]; they are appended to the
    last segment record's blob. Returns (record, blob) pairs."""
    out = []
    off = next_off
    segs = route.segments
    stub = segs[0]
    ball_r = cfg.tip_ball_r_factor * r_tip
    tip_ball = sphere_mesh(stub.p0, ball_r)
    tip_cone = frustum_mesh(stub.p0, stub.p1, r_tip, r_shoot)
    parts = [np.concatenate([tip_ball, tip_cone])]
    recs_meta = [(ci.T_TIP, stub.p0, stub.p1, r_tip, r_shoot)]
    for seg in segs[1:]:
        if seg.kind != "shoot":
            continue
        parts.append(frustum_mesh(seg.p0, seg.p1, seg.r0, seg.r1))
        recs_meta.append((ci.T_SHAFT, seg.p0, seg.p1, seg.r0, seg.r1))
    balls = _ball_meshes(extra_balls)
    if balls:
        parts[-1] = np.concatenate([parts[-1]] + balls)
    for (tid, p0, p1, r0, r1), blob in zip(recs_meta, parts):
        out.append((build_record(tid, p0, p1, r0, r1,
                                  off, blob.nbytes, cfg), blob.tobytes()))
        off += blob.nbytes
    return out


def synth_v3_trunk(trunk, plate_z, cfg, next_off, parent_xy=None, extra_balls=()):
    """v3 trunk: conical foot flare at the pad + constant-radius column.
    v3.5 branch trunks instead sprout from their parent via a 45-degree
    Y diagonal, no flare/pad.

    v3.6: the top dome and the Y-split ball are no longer embedded here
    -- they are joints, owned by the global joint pass, which passes this
    trunk's balls (top joint, mid-column junction balls, brace-end balls
    on this axis...) via `extra_balls`; appended to the column blob.
    Returns (record, blob) pairs; plate pads are cloned separately."""
    out = []
    off = next_off
    xy = trunk.xy
    p_top = np.array([xy[0], xy[1], trunk.z_top])
    if trunk.foot_kind == "branch" and parent_xy is not None:
        p_split = np.array([parent_xy[0], parent_xy[1], trunk.z_split])
        p_start = np.array([xy[0], xy[1], trunk.z_start])
        diag = frustum_mesh(p_split, p_start, trunk.r, trunk.r)
        out.append((build_record(ci.T_JOINER, p_split, p_start, trunk.r, trunk.r,
                                  off, diag.nbytes, cfg), diag.tobytes()))
        off += diag.nbytes
        p_col0 = p_start
    else:
        p_plate = np.array([xy[0], xy[1], plate_z])
        p_col0 = np.array([xy[0], xy[1], plate_z + cfg.flare_len_mm])
        flare = frustum_mesh(p_plate, p_col0, cfg.flare_r_factor * trunk.r, trunk.r)
        out.append((build_record(ci.T_FLARE, p_plate, p_col0,
                                  cfg.flare_r_factor * trunk.r, trunk.r,
                                  off, flare.nbytes, cfg), flare.tobytes()))
        off += flare.nbytes
    parts = [frustum_mesh(p_col0, p_top, trunk.r, trunk.r)]
    parts += _ball_meshes(extra_balls)
    blob = np.concatenate(parts)
    out.append((build_record(ci.T_SHAFT, p_col0, p_top, trunk.r, trunk.r,
                              off, blob.nbytes, cfg), blob.tobytes()))
    off += blob.nbytes
    return out


def _trailing_bytes(type_id, cfg):
    trailing = bytearray(24)
    if type_id == CONTACT_DEPTH_TRAILING_TYPE:
        struct.pack_into("<f", trailing, 0, float(cfg.contact_depth_mm))
    return bytes(trailing)


def build_record(type_id, start, end, r1, r2, mesh_off, mesh_len, cfg):
    """Pack a 72-byte segment record for a from-scratch-synthesized
    segment. Known fields are computed; the unmapped trailing 24 bytes
    use the corpus-evidenced convention (see module docstring)."""
    raw = bytearray(72)
    struct.pack_into("<II", raw, 0, ci.SEG_MAGIC, type_id)
    struct.pack_into("<3f", raw, 8, *start)
    struct.pack_into("<3f", raw, 20, *end)
    struct.pack_into("<2f", raw, 32, float(r1), float(r2))
    struct.pack_into("<II", raw, 40, mesh_off, mesh_len)
    raw[48:72] = _trailing_bytes(type_id, cfg)
    return bytes(raw)


def synth_chain_records(route, r_tip, r_shaft, next_off, cfg):
    """Build (record_bytes, mesh_bytes) pairs for one support's tip,
    taper, shaft (one per shaft/foot Segment in the route -- a route can
    have more than one shaft-kind segment when it kinks to a model
    anchor), and a joiner into the flare -- everything except the
    pad/bridge foot, which place_pad() below handles via cloning.

    `route` is a routing.Route. Segment kinds map:
      "stub"  -> tip (constant r_tip) then taper (r_tip -> r_shaft)
      "shaft" -> constant-radius shaft cylinder
      "foot"  -> joiner (short constant-radius coupler) leading to the pad
    """
    out = []
    off = next_off
    segs = route.segments
    for seg in segs:
        if seg.kind == "stub":
            mid = seg.p0 + (seg.p1 - seg.p0) * 0.4
            tip_tri = frustum_mesh(seg.p0, mid, r_tip, r_tip)
            out.append((build_record(ci.T_TIP, seg.p0, mid, r_tip, r_tip,
                                       off, tip_tri.nbytes, cfg), tip_tri.tobytes()))
            off += tip_tri.nbytes
            taper_tri = frustum_mesh(mid, seg.p1, r_tip, r_shaft)
            out.append((build_record(ci.T_TAPER, mid, seg.p1, r_tip, r_shaft,
                                       off, taper_tri.nbytes, cfg), taper_tri.tobytes()))
            off += taper_tri.nbytes
        elif seg.kind in ("shaft", "detour"):
            tri = frustum_mesh(seg.p0, seg.p1, r_shaft, r_shaft)
            out.append((build_record(ci.T_SHAFT, seg.p0, seg.p1, r_shaft, r_shaft,
                                       off, tri.nbytes, cfg), tri.tobytes()))
            off += tri.nbytes
        elif seg.kind == "foot":
            tri = frustum_mesh(seg.p0, seg.p1, r_shaft, r_shaft)
            out.append((build_record(ci.T_JOINER, seg.p0, seg.p1, r_shaft, r_shaft,
                                       off, tri.nbytes, cfg), tri.tobytes()))
            off += tri.nbytes
    return out


def brace_record(brace, next_off, cfg, extra_balls=()):
    """A brace is a constant-radius joiner rod between two trunk columns
    (v3: 45-degree diagonals in crossed X pairs, never horizontal).

    v3.6: end balls come from the global joint pass via `extra_balls`
    (one deduped ball per joint, sized to the fattest member there --
    the old per-rod pair doubled spheres at shared endpoints and
    undersized them against fattened trunks)."""
    parts = [frustum_mesh(brace["p0"], brace["p1"], brace["r"], brace["r"])]
    parts += _ball_meshes(extra_balls)
    blob = np.concatenate(parts)
    rec = build_record(ci.T_JOINER, brace["p0"], brace["p1"], brace["r"], brace["r"],
                        next_off, blob.nbytes, cfg)
    return rec, blob.tobytes()


def place_pad(proj, template_pad_record, at_xy, next_off):
    """Foot pad at a new xy, via VALIDATED clone+translate (not
    from-scratch synthesis -- see module docstring)."""
    dx = at_xy[0] - template_pad_record.start[0]
    dy = at_xy[1] - template_pad_record.start[1]
    out = ci.clone_support(proj, [template_pad_record], dx, dy)
    (rec_bytes, blob) = out[0]
    # clone_support already assigned mesh_off from proj's current end; the
    # caller here is appending many new records in one batch, so patch the
    # offset to the caller's running `next_off` instead.
    rec = ci.Record(rec_bytes)
    patched = rec.packed(mesh_off=next_off)
    return patched, blob


def _field(m, name, default=None):
    """Read `name` off a v4 tree.Member OR a plain dict with the same
    schema (joints.collect_members's output, or a hand-built test
    fixture) -- same duality joints.py's _field/tree.py's _get already
    rely on, duplicated here rather than importing (this module stays
    self-contained per its existing style)."""
    if isinstance(m, dict):
        return m.get(name, default)
    return getattr(m, name, default)


def _point_seg_dist(p, a, b):
    """Distance from point p (3,) to segment a->b (3,)."""
    d = b - a
    L2 = float(d @ d)
    if L2 < 1e-18:
        return float(np.linalg.norm(p - a))
    t = float(np.clip((p - a) @ d / L2, 0.0, 1.0))
    return float(np.linalg.norm(p - (a + t * d)))


def _assign_balls_to_members(members, balls, tol=0.25):
    """One member index per ball: "the fattest member touching it"
    (design-v4.md joint-ball doctrine, joints.compute_joint_balls) --
    re-derived here against member INDEX rather than the `owner` tuple
    joints.py hands back, because in v4 several Members legitimately
    share the same owner (e.g. a contact's stub, trunk diagonal, and
    trunk column are all owner=("contact", i)) so owner alone can't
    identify which single record should carry the ball. A joint ball's
    center is always literally an endpoint of at least one touching
    member (COINCIDE_TOL_MM=0.15) or within ON_AXIS_TOL_MM=0.2 of a
    through-member's axis (joints.py's own tolerances) -- tol=0.25
    covers both with margin. Asserts every ball resolves to exactly one
    member; a ball that lands on nothing is a bug upstream, not a
    silent drop."""
    assigned = []
    for b in balls:
        center = np.asarray(_field(b, "center"), float)
        best_idx, best_r = None, -1.0
        for mi, m in enumerate(members):
            p0 = np.asarray(_field(m, "p0"), float)
            p1 = np.asarray(_field(m, "p1"), float)
            if _point_seg_dist(center, p0, p1) <= tol:
                mr = max(float(_field(m, "r0")), float(_field(m, "r1")))
                if mr > best_r:
                    best_r, best_idx = mr, mi
        assert best_idx is not None, (
            f"joint ball at {center.tolist()} r={_field(b, 'r')} did not land "
            f"on any member's axis -- upstream bug, not a silent drop")
        assigned.append(best_idx)
    return assigned


def synth_member_chain(members, balls, cfg, next_off):
    """v4 emission (design-v4.md #14 Phase-review/interim emitter): one
    record per Member (tree.Member or joints.collect_members's dict
    schema), each a frustum_mesh blob. Type id mirrors v3's evidenced
    T_TAPER-carries-contact_depth convention: T_TAPER when the member is
    both tapered (r0 != r1) AND has a designed embed_end (a stub's
    contact tip, or a strut's model-anchor foot -- the cases v3's
    tip->taper transition and struts.py's anchor foot correspond to);
    T_SHAFT otherwise (constant-radius trunk columns, shoots, diagonals).

    A stub's contact ball (cfg.tip_ball_r_factor * r0 sphere at its
    embedded p0) is merged into ITS OWN record's blob -- same
    "removal-friendly point contact, not a joint" rule v3.6 established
    (joints.py module docstring); a model-anchor strut's embedded foot
    is instead handled by the global joint pass's MODEL_FOOT_FACTOR ball
    (it has no separate contact-ball convention of its own).

    Every entry in `balls` (joints.compute_joint_balls output: dicts or
    objects with center/r/owner) is appended into exactly ONE record's
    blob, chosen by _assign_balls_to_members; the assert there is the
    "every ball emitted exactly once" guarantee the caller can rely on.
    Returns (record_bytes, blob_bytes) pairs, offsets contiguous from
    next_off."""
    members = list(members)
    ball_owner_idx = _assign_balls_to_members(members, balls)
    balls_by_member = {}
    for bi, mi in enumerate(ball_owner_idx):
        balls_by_member.setdefault(mi, []).append(bi)

    out = []
    off = next_off
    n_balls_emitted = 0
    for mi, m in enumerate(members):
        p0 = np.asarray(_field(m, "p0"), float)
        p1 = np.asarray(_field(m, "p1"), float)
        r0 = float(_field(m, "r0"))
        r1 = float(_field(m, "r1"))
        kind = _field(m, "kind")
        embed_end = _field(m, "embed_end", None)

        parts = [frustum_mesh(p0, p1, r0, r1)]
        if kind == "stub":
            ball_r = cfg.tip_ball_r_factor * r0
            parts.append(sphere_mesh(p0, ball_r))
        for bi in balls_by_member.get(mi, []):
            b = balls[bi]
            parts.append(sphere_mesh(_field(b, "center"), float(_field(b, "r"))))
            n_balls_emitted += 1
        blob = np.concatenate(parts)

        tapered = abs(r0 - r1) > 1e-9
        type_id = ci.T_TAPER if (tapered and embed_end is not None) else ci.T_SHAFT
        out.append((build_record(type_id, p0, p1, r0, r1, off, blob.nbytes, cfg),
                    blob.tobytes()))
        off += blob.nbytes

    assert n_balls_emitted == len(balls), (
        f"ball emission count mismatch: emitted {n_balls_emitted}, expected {len(balls)}")
    return out


def find_template_records(proj):
    """First real record of each type this module needs a trailing-bytes
    or pad/bridge template for, from an already-loaded base Project."""
    templates = {}
    for r in proj.records:
        templates.setdefault(r.type, r)
    return templates
