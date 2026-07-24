"""Global joint-ball pass (v3.6, Craig's design -- queue item 1).

Replaces the scattered per-call ball sizing that lived inside
synth_v3_support / synth_v3_trunk / brace_record. Those embedded balls
were sized at EMISSION time from whatever radius the local call happened
to know, which created a whole bug class (gapped undersized balls when a
later pass fattened a member; doubled spheres where two members shared an
endpoint and each brought its own ball).

This pass runs after ALL geometry and radii are final:

  1. collect every member (stubs, shoots, trunk columns, Y-diagonals,
     risers, braces, rungs) with its final endpoint radii;
  2. cluster joint points: endpoint coincidence within a small tolerance,
     PLUS on-axis proximity for balls landing mid-member (a shoot
     junction on a trunk column, a rung end on a sibling shoot);
  3. emit exactly ONE ball per joint, sized to the FATTEST member
     touching it (joint_ball_factor * max radius at that point).

Balls are assigned an OWNER record (the fattest touching member's
entity) so emission can append each sphere into an existing record's
mesh blob -- record count and Chitubox-side grouping stay identical to
the ball-free structure.

Deliberate conventions preserved from the per-member era:
  - the tip CONTACT ball is not a joint (it is the removal-friendly
    point contact, still emitted by synth_v3_support);
  - a model-embedded end (strut anchor foot) keeps its 1.4x adhesion
    factor -- there is no second member to size against, the ball is
    the adhesion fillet against the model surface.

Enclosure diet (Craig's volume-forensics review): measured on the
full-scale Treant, 72.5% of joint balls (6.54ml of 7.60ml total ball
volume) are geometrically ENCLOSED -- every point of the ball's sphere
already lies inside the union of the touching members' own frustums,
overwhelmingly the case where a brace/rung endpoint lands mid-span on a
fatter through-member and the ball is sized to (and centered on the axis
of) that fatter member, well clear of its own ends. Emitting a sphere
there prints nothing a viewer or the slicer doesn't already have from
the frustum alone; it's pure wasted volume/resin. This pass drops those
balls from the LIST before emission (see compute_joint_balls's tail) --
every other exclusion (plate-band, radius-aware plate crossing, colinear
pass-through) and the sizing/ownership logic above it are unchanged;
emission still just consumes whatever ball list it's handed.
"""
import numpy as np

_MISSING = object()

MODEL_FOOT_FACTOR = 1.4     # adsorbed strut foot ball vs its rod radius
COINCIDE_TOL_MM = 0.15      # endpoint clustering: true coincidences only
ON_AXIS_TOL_MM = 0.2        # node-to-member-axis distance for sizing
PLATE_BAND_MM = 0.2         # same plate-band rule verify.connectivity_report
                             # rule (b) uses -- a node this close to plate_z
                             # is plate-rooted, owned by the foot flare/pad,
                             # not a joint (F1: no balls sunk into the plate)
COLINEAR_ANGLE_DEG = 10.0   # F3(a): two-member pass-through anti-parallel
                             # tolerance (straight continuation, no ball)
COLINEAR_RADIUS_FRAC = 0.20  # F3(a): end-radius match tolerance for the
                              # same pass-through rule
_COLINEAR_COS_MAX = float(np.cos(np.radians(180.0 - COLINEAR_ANGLE_DEG)))

# --- enclosure diet: drop a ball whose sphere is provably already
# inside the union of its touching members' own frustums (see module
# docstring). Conservative by construction: a ball is dropped only when
# a genuine mathematical CERTIFICATE proves the whole sphere covered;
# any patch of the sphere the certificate can't clear keeps the ball.
ENCLOSURE_TOL_MM = 0.02
# Physical argument for the tolerance: mSLA resin printers resolve
# roughly ~0.05mm per feature (typical layer height and XY pixel pitch
# both sit around there). A protrusion under 0.02mm -- well under half
# that feature scale -- cannot be resolved as a distinct bump by the
# layer stack or the pixel grid; it does not print as anything the
# frustum union wasn't already going to produce. Dropping a ball whose
# only "escape" from the union is smaller than this changes nothing that
# actually resolves on the plate. This is a fixed constant, not a config
# knob -- it is a printer-physics fact, not a per-model tuning choice.
#
# Adversarial-review history (this filter has been through three rounds
# of independent review; the first two rounds' fixes are still load-
# bearing and summarized here, but the actual enclosure TEST below this
# comment is a full replacement of what those rounds patched):
#
# Round 1 found the filter sampled coverage with a fixed N=256 Fibonacci
# grid and used an unsound "clamp-then-radial" distance formula. Round 2
# found and fixed two more issues that are still true today and still
# matter: (a) star-shapedness -- "every sampled point covered implies
# the solid sphere is covered" only holds when the ball's CENTER lies
# inside every frustum credited for that coverage (a convex frustum
# containing both the center and a covered point contains the whole
# segment between them); with members meeting under sub-COINCIDE_TOL_MM
# jitter this can fail, so only CREDITED members (center verified
# inside their own frustum, checked in _ball_enclosed) may contribute
# coverage. (b) the containment test needs the TRUE nearest-point
# distance to the solid, not an approximation that disagrees with
# itself at region boundaries.
#
# Round 3 found that ANY finite, ANALYTIC prediction of where a
# member's coverage boundary crosses the sphere -- however carefully
# reasoned -- can be wrong for compound geometry (two tapered, non-
# coplanar members at a shallow bend produced a real ~0.035mm gap at a
# boundary location the analytic prediction missed by 24-45 degrees).
# No amount of sampling density fixes an analytic model that is itself
# wrong. The fix below stops predicting where boundaries are and
# instead builds a genuine mathematical CERTIFICATE that needs no
# boundary-shape assumption at all: recursive spherical-patch
# subdivision with a rigorous Lipschitz bound on the exact signed
# distance to the union (see _signed_dist_to_frustum and
# _ball_enclosed's docstring for the full argument). This is sound for
# ANY member geometry -- tapered, non-coplanar, degenerate -- by
# construction, not by anticipating shapes.
ENCLOSURE_PROOF_TOL_MM = 0.9 * ENCLOSURE_TOL_MM
# 10% safety margin for the certificate: guards floating-point drift
# across many recursive re-normalizations and the depth-limit fallback
# below, on top of (not instead of) the rigorous bound itself. A ball
# actually dropped has a CERTIFIED true worst-case gap under 0.018mm,
# safely inside the stated 0.02mm feature-scale argument.
_CERT_MAX_DEPTH = 22
# Each level roughly halves the patch's angular radius (icosahedron
# faces start around 37 degrees); depth 22 reaches well under 0.0001
# degrees -- far finer than anything physically meaningful at
# millimeter ball radii. A patch that still can't certify at that
# resolution is not a numerical-precision problem; the doctrine (keep
# when the certificate can't be produced) applies and the ball is kept.
_CERT_MAX_PATCHES = 2_000_000
# A circuit breaker, not a correctness requirement: bounds worst-case
# work for any input, however pathological, without changing the
# soundness argument (still: uncertified anywhere -> KEEP).


def _seg_dist_2d(qa, qr, a0, a1, b0, b1):
    """2D Euclidean distance from points (qa, qr) (both (N,) arrays) to
    the single segment (a0, a1)-(b0, b1) in that same 2D plane."""
    dx, dy = b0 - a0, b1 - a1
    seg_L2 = dx * dx + dy * dy
    if seg_L2 < 1e-18:
        return np.hypot(qa - a0, qr - a1)
    t = np.clip(((qa - a0) * dx + (qr - a1) * dy) / seg_L2, 0.0, 1.0)
    px, py = a0 + t * dx, a1 + t * dy
    return np.hypot(qa - px, qr - py)


def _signed_dist_to_frustum(points, p0, p1, r0, r1):
    """EXACT signed Euclidean distance (mm) from each of `points` (N,3)
    to the boundary of the solid, flat-capped frustum p0->p1 (radius r0
    at p0, r1 at p1) -- negative when the point is inside (magnitude =
    true depth), positive outside. Exact, not an approximation: this is
    what makes it (a) correct as a containment test on its own, and (b)
    -- the property the certification in _ball_enclosed actually needs
    -- exactly 1-Lipschitz in ordinary Euclidean distance, a basic real-
    analysis fact true of the distance function to ANY nonempty closed
    set, no case-by-case argument required (|d(x,S)-d(y,S)| <= |x-y| by
    the triangle inequality; the signed version inherits the same bound
    on each side of the boundary). No per-member "how tapered is this"
    adjustment needed, unlike an earlier draft of this function that
    tried to shortcut the geometry and got the direction of its own
    safety margin backwards for tapered members (verified by hand
    against this exact formula before landing it).

    Computed by exploiting the frustum's rotational symmetry: in the
    2D (axial, radial) half-plane obtained by measuring each point's
    position along the member's own axis and its perpendicular distance
    from that axis, the frustum's cross-section is a trapezoid (or
    triangle, r0/r1=0) with three real edges -- the p0 cap, the p1 cap,
    and the tapered lateral side -- plus the axis line itself, which
    isn't a boundary of the 3-D solid (the frustum is this trapezoid's
    solid of revolution). The true 3-D distance to the solid equals the
    2-D distance to those three edges (never the axis line, since every
    query point's radial coordinate is already the distance FROM that
    axis, i.e. exactly where the point sits on its own meridian half-
    plane) -- minimized over the three, then sign-flipped by an
    ordinary inside/outside test."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        return np.linalg.norm(points - p0[None, :], axis=1) - r0
    L = float(np.sqrt(L2))
    d_hat = d / L
    qa = (points - p0[None, :]) @ d_hat            # axial coordinate, mm
    rel = points - p0[None, :] - qa[:, None] * d_hat[None, :]
    qr = np.linalg.norm(rel, axis=1)                # radial coordinate, mm (>=0)

    d_p0cap = _seg_dist_2d(qa, qr, 0.0, 0.0, 0.0, r0)
    d_slant = _seg_dist_2d(qa, qr, 0.0, r0, L, r1)
    d_p1cap = _seg_dist_2d(qa, qr, L, r1, L, 0.0)
    dist = np.minimum(np.minimum(d_p0cap, d_slant), d_p1cap)

    local_r = r0 + (r1 - r0) * np.clip(qa, 0.0, L) / L
    inside = (qa >= 0.0) & (qa <= L) & (qr <= local_r)
    return np.where(inside, -dist, dist)


def _in_frustum_mask(points, p0, p1, r0, r1, tol):
    """Boolean (N,) containment of `points` within `tol` mm of the
    frustum p0->p1 (radius r0 at p0, r1 at p1) -- see
    _signed_dist_to_frustum for the exact distance this thresholds."""
    return _signed_dist_to_frustum(points, p0, p1, r0, r1) <= tol


def _build_icosahedron():
    """12 unit vertices + 20 triangular faces (vertex-index triples) of
    a regular icosahedron -- the starting partition for the enclosure
    certificate's recursive subdivision. Deterministic, seedless, pure
    geometry (golden-ratio vertex construction, then faces recovered by
    adjacency: each vertex's 5 nearest neighbors, by construction, are
    exactly its edge-connected neighbors, since the next-nearest
    vertices -- across a face diagonal -- sit strictly farther away)."""
    phi = (1.0 + np.sqrt(5.0)) / 2.0
    raw = []
    for a in (-1.0, 1.0):
        for b in (-1.0, 1.0):
            raw.append((0.0, a, b * phi))
            raw.append((a, b * phi, 0.0))
            raw.append((b * phi, 0.0, a))
    verts = np.array(raw, dtype=float)
    verts /= np.linalg.norm(verts, axis=1, keepdims=True)
    n = len(verts)
    d = np.linalg.norm(verts[:, None, :] - verts[None, :, :], axis=2)
    np.fill_diagonal(d, np.inf)
    edge_len = np.sort(d, axis=1)[:, :5].max()
    adj = d <= edge_len * 1.01
    faces = []
    for i in range(n):
        neighbors_i = [j for j in range(i + 1, n) if adj[i, j]]
        for jx, j in enumerate(neighbors_i):
            for k in neighbors_i[jx + 1:]:
                if adj[j, k]:
                    faces.append((i, j, k))
    assert len(faces) == 20, f"icosahedron construction bug: {len(faces)} faces"
    return verts, faces


_ICOSA_VERTS, _ICOSA_FACES = _build_icosahedron()


def _certify_sphere_enclosed(center, r, credited, tol):
    """The enclosure certificate. Returns True only when a recursive
    spherical-patch subdivision PROVES every direction on the ball's
    sphere is within `tol` of some credited member's frustum -- not
    sampled-and-looked-fine, an actual bound.

    Mathematical argument: for each credited member m, let
    h_m(u) = tol - signed_dist_to_frustum_m(center + r*u) for a
    direction u (unit vector). h_m(u) >= 0 exactly when direction u is
    covered (within tol) by member m -- signed_dist_to_frustum is EXACT
    (see its own docstring), so this is not an approximation. Let
    F(u) = max over credited m of h_m(u); F(u) >= 0 exactly when u is
    covered by the UNION. Since each signed_dist_to_frustum_m is
    exactly 1-Lipschitz in Euclidean distance (real-analysis fact for
    any distance-to-a-closed-set function -- true for ANY member
    geometry, no taper/coplanarity assumption needed), each h_m is too,
    hence so is F (max of 1-Lipschitz functions with the same modulus
    is 1-Lipschitz with that modulus). Moving direction by angle theta
    moves the corresponding sphere point by chord length 2r.sin(theta/2)
    <= r.theta, so F is (r)-Lipschitz in ANGULAR distance.

    For a spherical-triangle patch (v1, v2, v3) with reference direction
    u0 = normalize(v1+v2+v3) and angular radius
    theta_patch = max(angle(u0,v1), angle(u0,v2), angle(u0,v3)): angular
    distance to u0 is a convex function of position on the sphere within
    a hemisphere of u0 (true here -- these patches, even at depth 0, are
    well under 90 degrees across), so its maximum over the (geodesically
    convex) triangle is attained at a vertex -- theta_patch is the EXACT
    bound, not a loose one. So for every u in the patch,
    F(u) >= F(u0) - r.theta_patch. If F(u0) >= r.theta_patch, EVERY
    point in the patch is certified covered, no further work needed.

    Otherwise the patch is split into 4 children (bisecting each edge,
    normalized back to the sphere -- standard geodesic subdivision) and
    the same test applied to each, up to _CERT_MAX_DEPTH; a patch that
    still can't certify by then (or if the total patch budget
    _CERT_MAX_PATCHES is exhausted) makes the whole ball NOT certified
    -- conservative by construction, per doctrine.

    Implementation note: iterative and batched (one round processes
    every currently-uncertified patch together, vectorized across
    patches and members) rather than recursive-per-patch, both for
    speed and to avoid Python recursion-depth limits at _CERT_MAX_DEPTH."""
    v1 = _ICOSA_VERTS[[f[0] for f in _ICOSA_FACES]]
    v2 = _ICOSA_VERTS[[f[1] for f in _ICOSA_FACES]]
    v3 = _ICOSA_VERTS[[f[2] for f in _ICOSA_FACES]]
    depth = np.zeros(len(_ICOSA_FACES), dtype=np.int64)
    total_patches = len(_ICOSA_FACES)

    while len(v1) > 0:
        u0 = v1 + v2 + v3
        u0 /= np.linalg.norm(u0, axis=1, keepdims=True)
        cos1 = np.clip(np.sum(u0 * v1, axis=1), -1.0, 1.0)
        cos2 = np.clip(np.sum(u0 * v2, axis=1), -1.0, 1.0)
        cos3 = np.clip(np.sum(u0 * v3, axis=1), -1.0, 1.0)
        theta = np.maximum(np.arccos(cos1), np.maximum(np.arccos(cos2), np.arccos(cos3)))
        pts = center[None, :] + r * u0

        best = np.full(len(pts), -np.inf)
        for m in credited:
            signed = _signed_dist_to_frustum(pts, m["p0"], m["p1"], m["r0"], m["r1"])
            best = np.maximum(best, tol - signed)

        # Direct witness short-circuit: best(u0) < 0 means the CENTROID
        # direction itself -- one single, concrete point -- is not
        # covered by any credited member, full stop. That alone already
        # proves the ball isn't fully enclosed; no Lipschitz reasoning
        # or further subdivision needed. This is what keeps a genuinely
        # large gap (an elbow, a real junction) cheap: the very first
        # icosahedron face landing in it exits immediately, instead of
        # subdividing all the way to _CERT_MAX_DEPTH before giving up --
        # that exhaustive path is reserved for genuinely borderline
        # patches (near the tolerance line), where it's unavoidable.
        if (best < 0.0).any():
            return False

        uncertified = best < r * theta
        if not uncertified.any():
            return True

        if (depth[uncertified] >= _CERT_MAX_DEPTH).any():
            return False

        idx = np.nonzero(uncertified)[0]
        total_patches += 3 * len(idx)
        if total_patches > _CERT_MAX_PATCHES:
            return False

        sv1, sv2, sv3, sdepth = v1[idx], v2[idx], v3[idx], depth[idx]
        m12 = sv1 + sv2
        m12 /= np.linalg.norm(m12, axis=1, keepdims=True)
        m23 = sv2 + sv3
        m23 /= np.linalg.norm(m23, axis=1, keepdims=True)
        m31 = sv3 + sv1
        m31 /= np.linalg.norm(m31, axis=1, keepdims=True)

        v1 = np.concatenate([sv1, sv2, sv3, m12])
        v2 = np.concatenate([m12, m23, m31, m23])
        v3 = np.concatenate([m31, m12, m23, m31])
        depth = np.concatenate([sdepth + 1] * 4)
    return True


def _ball_enclosed(center, r, touching_members, tol):
    """True iff the ball's solid sphere is CERTIFIED (proof, not
    sampling -- see _certify_sphere_enclosed's docstring for the
    argument) contained in the union of its touching members' own
    frustums. Conservative by construction (an empty credited set, a
    non-positive radius, or any patch the certificate can't clear all
    mean "not enclosed") and deterministic (no RNG anywhere in this
    pipeline).

    Two stages:

    1. Star-shaped credit restriction: only members whose frustum
       contains the ball's CENTER are credited -- this is what makes
       "a sampled/patch point is covered by member m" a valid stand-in
       for "the whole solid ray from center to that point is covered by
       m" (a convex frustum containing both endpoints of a segment
       contains the whole segment). Without this, a member whose own
       endpoint is offset from the ball's chosen center by ordinary
       clustering jitter can appear to cover surface points without
       actually backing up the interior between them (an adversarial-
       review finding from an earlier round of this filter).
    2. The recursive certificate over the credited set."""
    if r <= 0.0 or not touching_members:
        return False
    center_pt = center[None, :]
    credited = [m for m in touching_members
                if bool(_in_frustum_mask(center_pt, m["p0"], m["p1"], m["r0"], m["r1"], tol)[0])]
    if not credited:
        return False
    return _certify_sphere_enclosed(center, r, credited, tol)


def _point_segment_dist(points, p0, p1):
    """Distances from (N,3) points to one segment p0->p1."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        return np.linalg.norm(points - p0, axis=1)
    t = np.clip((points - p0) @ d / L2, 0.0, 1.0)
    proj = p0[None, :] + t[:, None] * d[None, :]
    return np.linalg.norm(points - proj, axis=1)


def _dist_and_local_radius(center, p0, p1, r0, r1):
    """Distance from `center` to segment p0->p1, plus the segment's OWN
    radius at the nearest point (linear taper r0->r1 evaluated at the
    same clamped t used for the distance) -- F3(c): the radius a joint
    ball should be sized against is the member's radius AT THE POINT the
    joint actually lands on, not blindly max(r0, r1) (which lets an
    irrelevant far taper end dominate a joint sitting at the near end)."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        return float(np.linalg.norm(center - p0)), float(r0)
    t = float(np.clip((center - p0) @ d / L2, 0.0, 1.0))
    proj = p0 + t * d
    dist = float(np.linalg.norm(center - proj))
    local_r = float(r0) + (float(r1) - float(r0)) * t
    return dist, local_r


# Rows per chunk of the batched joint<->axis-member scan below. Bounds the
# transient (chunk x n_axis_members x 3) working set; purely a memory knob, it
# cannot change the result (each row is independent).
_AXIS_SCAN_CHUNK = 256


def _batch_dist_and_local_radius(centers, p0s, p1s, r0s, r1s):
    """BATCH-FIRST (binding house rule) vectorised `_dist_and_local_radius` over
    the full (n_centers x n_segments) cross product. Returns `(dist, local_r)`,
    both `(n_centers, n_segments)` float arrays.

    BIT-IDENTICAL to calling the scalar helper in a double loop -- every
    arithmetic expression below is written in the SAME operand order the scalar
    form uses (explicit component-wise `x*x + y*y + z*z` rather than `@`/`einsum`,
    whose small-vector reduction order is NOT guaranteed to match `np.dot`; the
    einsum form was measured to differ from the scalar reference in 3275/240000
    distances, the explicit form in 0/240000). This branch's determinism contract
    is FLOAT-EXACT, so "close enough" is not a legal substitute -- see
    `test_joints.py::test_batch_dist_and_local_radius_bit_identical_to_scalar`.

    Zero-length segments follow the scalar branch exactly: distance to `p0`, and
    the segment's `r0` (the scalar helper returns `t`-free values there)."""
    centers = np.asarray(centers, float)
    p0s = np.asarray(p0s, float); p1s = np.asarray(p1s, float)
    r0s = np.asarray(r0s, float); r1s = np.asarray(r1s, float)
    n_c = len(centers); n_m = len(p0s)
    dist = np.empty((n_c, n_m), float)
    local_r = np.empty((n_c, n_m), float)
    if n_c == 0 or n_m == 0:
        return dist, local_r
    d = p1s - p0s
    L2 = d[:, 0] * d[:, 0] + d[:, 1] * d[:, 1] + d[:, 2] * d[:, 2]
    degenerate = L2 < 1e-18
    L2safe = np.where(degenerate, 1.0, L2)
    for start in range(0, n_c, _AXIS_SCAN_CHUNK):
        end = min(start + _AXIS_SCAN_CHUNK, n_c)
        c = centers[start:end]                                   # (k,3)
        relx = c[:, None, 0] - p0s[None, :, 0]
        rely = c[:, None, 1] - p0s[None, :, 1]
        relz = c[:, None, 2] - p0s[None, :, 2]
        num = (relx * d[None, :, 0] + rely * d[None, :, 1] + relz * d[None, :, 2])
        t = np.clip(num / L2safe[None, :], 0.0, 1.0)
        # `proj = p0 + t*d` THEN `center - proj` -- the scalar helper's exact
        # association; `(center - p0) - t*d` is a different rounding and does
        # NOT reproduce it bit-for-bit.
        ex = c[:, None, 0] - (p0s[None, :, 0] + t * d[None, :, 0])
        ey = c[:, None, 1] - (p0s[None, :, 1] + t * d[None, :, 1])
        ez = c[:, None, 2] - (p0s[None, :, 2] + t * d[None, :, 2])
        dchunk = np.sqrt(ex * ex + ey * ey + ez * ez)
        rchunk = r0s[None, :] + (r1s[None, :] - r0s[None, :]) * t
        if degenerate.any():
            # scalar branch: |center - p0| and r0, with no t involved at all.
            d0 = np.sqrt(relx * relx + rely * rely + relz * relz)
            dchunk = np.where(degenerate[None, :], d0, dchunk)
            rchunk = np.where(degenerate[None, :],
                              np.broadcast_to(r0s[None, :], rchunk.shape), rchunk)
        dist[start:end] = dchunk
        local_r[start:end] = rchunk
    return dist, local_r


def collect_members(members):
    """v4 (design-v4.md #2): the CCO builder (cco.build) produces the
    member list NATIVELY (tree.Member objects, or plain dicts with the
    same schema) instead of v3's four separate routes/trunks/risers/
    braces collections. This function inverts from a from-parts
    CONSTRUCTOR to a validation/identity layer: assert the schema, and
    normalize every member (Member object or dict) to the dict schema
    {p0, p1, r0, r1, owner, kind, embed_end} that compute_joint_balls,
    verify.connectivity_report, and emission consume.

    embed_end replaces v3's kind/sentinel logic (j == -1 model rung,
    i == j == -3 thinning rung) for embed-eligibility; a plain dict
    fixture that omits it is treated as None (no designed embed), same
    as a freshly-built tree.Member's default.
    """
    out = []
    for m in members:
        p0 = np.asarray(_field(m, "p0"), float)
        p1 = np.asarray(_field(m, "p1"), float)
        r0 = float(_field(m, "r0"))
        r1 = float(_field(m, "r1"))
        kind = _field(m, "kind")
        owner = _field(m, "owner")
        assert p0.shape == (3,) and p1.shape == (3,), f"member p0/p1 must be 3-vectors: {m}"
        assert isinstance(kind, str), f"member kind must be a str: {m}"
        assert isinstance(owner, tuple) and len(owner) == 2, f"member owner must be a 2-tuple: {m}"
        embed_end = _field(m, "embed_end", default=None)
        assert embed_end in ("p0", "p1", None), f"member embed_end must be p0/p1/None: {m}"
        out.append(dict(p0=p0, p1=p1, r0=r0, r1=r1, owner=owner, kind=kind,
                         embed_end=embed_end))
    return out


def _field(m, name, default=_MISSING):
    if isinstance(m, dict):
        if name in m:
            return m[name]
        if default is not _MISSING:
            return default
        raise KeyError(f"member dict missing required field {name!r}: {m}")
    if hasattr(m, name):
        return getattr(m, name)
    if default is not _MISSING:
        return default
    raise AttributeError(f"member object missing required field {name!r}: {m}")


def compute_joint_balls(members, plate_z, cfg):
    """One ball per joint, sized to the fattest member touching it.
    Returns list of dicts {center (3,), r, owner} with owner as in
    collect_members. See module docstring for the rules.

    v4: joint-ness is now determined structurally instead of by a
    per-kind branch table -- every member contributes a node at p1, and
    at p0 UNLESS its kind is "stub" (a stub's p0 is the tip CONTACT, the
    removal-friendly point contact, never a joint -- same rule v3 had,
    now expressed once instead of per-kind). An endpoint is the
    model-adsorbed-foot case (MODEL_FOOT_FACTOR) iff it is that member's
    embed_end AND it isn't the stub-tip case already excluded above --
    replaces v3's kind-specific "strut"/"model_rung" embedded flags.

    F1 (Craig's Chitubox review): a node within PLATE_BAND_MM of plate_z
    is plate-rooted -- excluded here, same plate-band rule
    verify.connectivity_report rule (b) uses. The foot flare + pad
    (chitubox_emit's per-member plate_z handling) own that region
    instead; a ball there would sink half into the build plate.

    F3(a): a cluster of exactly two endpoints whose members are
    anti-parallel through the point (within COLINEAR_ANGLE_DEG) AND
    whose end radii match within COLINEAR_RADIUS_FRAC is a straight
    continuation (a bifurcation split or rewire seam, not a real
    junction) -- v3 merged these cosmetically; no ball."""
    members = collect_members(members)
    if not members:
        return []

    # --- joint NODES: member endpoints that are articulation points.
    # outward_dir points AWAY from the node, along the member, toward its
    # other end -- used by F3(a)'s colinear-pass-through test. The
    # trailing `m` (the owning member's own dict) is carried so the
    # enclosure diet below can test the ball against the actual member(s)
    # that meet at this node, without re-deriving them from owner tuples
    # (several members can legitimately share one owner, design-v4.md #2).
    nodes = []  # (point, radius_at_end, owner, embedded_model, outward_dir, member)
    for m in members:
        d = m["p1"] - m["p0"]
        L = float(np.linalg.norm(d))
        d_unit = d / L if L > 1e-9 else np.zeros(3)
        if m["kind"] != "stub":
            if abs(float(m["p0"][2]) - plate_z) > PLATE_BAND_MM:
                nodes.append((m["p0"], m["r0"], m["owner"], m["embed_end"] == "p0", d_unit, m))
        if abs(float(m["p1"][2]) - plate_z) > PLATE_BAND_MM:
            nodes.append((m["p1"], m["r1"], m["owner"], m["embed_end"] == "p1", -d_unit, m))

    if not nodes:
        return []
    pts = np.array([n[0] for n in nodes])

    # --- cluster coincident endpoints (doubled X-brace ends, fan hubs)
    from scipy.spatial import cKDTree
    tree = cKDTree(pts)
    pairs = tree.query_pairs(r=COINCIDE_TOL_MM, output_type="ndarray")
    parent = np.arange(len(nodes))

    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    for a, b in pairs:
        ra, rb = find(int(a)), find(int(b))
        if ra != rb:
            parent[rb] = ra
    clusters = {}
    for i in range(len(nodes)):
        clusters.setdefault(find(i), []).append(i)

    # --- axis members for mid-member landings (sized by what the ball
    # actually sits on, not just who ends there). v4: a bifurcation SPLITS
    # its host member at the junction, so "mid-member" landings are now
    # the exception rather than v3's routine case (a rung/riser floating
    # onto an unsplit column's surface) -- kept as a safety net for any
    # near-coincident-but-not-exactly-split geometry. Every non-stub
    # member is eligible (stubs are excluded: their p0 is the tip contact
    # domain, not structural axis a joint could land on).
    axis_members = [m for m in members if m["kind"] != "stub"]

    # BATCH-FIRST (SPEC-V5-PERF, binding house rule): the joint <-> axis-member
    # proximity scan is ONE vectorised (n_clusters x n_axis_members) evaluation,
    # NOT a per-cluster scalar loop. The scalar form was the dominant pure-Python
    # cost of the whole Phase-8 relocation stage -- 6.6M `_dist_and_local_radius`
    # calls / 6.6s per `compute_joint_balls`, and the relocation loop re-derives
    # the live member+ball lists once per sweep, so it paid that 8x (46s of a
    # 204s profiled stage). `_batch_dist_and_local_radius` is bit-identical to the
    # scalar helper (see its docstring), so this is a pure cost fix: the emitted
    # ball list is byte-for-byte what the scalar loop produced.
    #
    # Every cluster's center is derived FIRST (the same `argmax` on end radius the
    # per-cluster body used), so one batched call covers them all; the body below
    # then reads its own row. Cluster iteration order, the F3(a)/plate/enclosure
    # skips, and the `>=` (last-wins) tie rule over `axis_members` order are all
    # preserved verbatim.
    cluster_idxs = list(clusters.values())
    cluster_centers = np.array(
        [nodes[idxs[int(np.argmax([nodes[i][1] for i in idxs]))]][0]
         for idxs in cluster_idxs], dtype=float) if cluster_idxs else np.zeros((0, 3))
    if axis_members:
        ax_dist, ax_local_r = _batch_dist_and_local_radius(
            cluster_centers,
            np.array([m["p0"] for m in axis_members], dtype=float),
            np.array([m["p1"] for m in axis_members], dtype=float),
            np.array([m["r0"] for m in axis_members], dtype=float),
            np.array([m["r1"] for m in axis_members], dtype=float))
        ax_on_axis = ax_dist <= ON_AXIS_TOL_MM
    else:
        ax_local_r = np.zeros((len(cluster_idxs), 0))
        ax_on_axis = np.zeros((len(cluster_idxs), 0), bool)

    balls = []
    for ci, idxs in enumerate(cluster_idxs):
        cluster_nodes = [nodes[i] for i in idxs]
        if len(cluster_nodes) == 2:
            # F3(a): exactly two endpoints, anti-parallel through the point
            # and near-equal radii -- a straight continuation (bifurcation
            # split / rewire seam), not a real junction. No ball.
            (_, ra, _, _, dir_a, _), (_, rb, _, _, dir_b, _) = cluster_nodes
            cosang = float(np.dot(dir_a, dir_b))
            r_hi = max(ra, rb)
            radii_close = r_hi <= 1e-12 or (r_hi - min(ra, rb)) <= COLINEAR_RADIUS_FRAC * r_hi
            if cosang <= _COLINEAR_COS_MAX and radii_close:
                continue
        r_end = max(n[1] for n in cluster_nodes)
        center = cluster_nodes[int(np.argmax([n[1] for n in cluster_nodes]))][0]
        embedded = any(n[3] for n in cluster_nodes)
        owner = None
        r_max = r_end
        # fattest member whose axis passes through this joint. F3(c): size
        # by the radius AT THE CLUSTER POINT (linear taper evaluated at the
        # nearest point on the member's own axis), not the member's overall
        # max end radius -- a tapered member's FAR end (irrelevant to this
        # joint) must not dominate a ball sized for its near end.
        #
        # Enclosure diet: every axis member within ON_AXIS_TOL_MM (not just
        # the fattest) is also a TOUCHING member for the containment test
        # below -- a through-member's own solid genuinely occupies this
        # point even when it isn't the one sizing the ball.
        touching_members = {id(n[5]): n[5] for n in cluster_nodes}
        # ascending indices == the original `for m in axis_members` order, so the
        # `>=` last-wins tie rule for `owner`/`r_max` is preserved exactly.
        for j in np.nonzero(ax_on_axis[ci])[0]:
            m = axis_members[int(j)]
            touching_members[id(m)] = m
            local_r = float(ax_local_r[ci, j])
            if local_r >= r_max:
                r_max = local_r
                owner = m["owner"]
        if owner is None:
            # no fatter through-member: owned by the fattest endpoint
            owner = cluster_nodes[int(np.argmax([n[1] for n in cluster_nodes]))][2]
        r_ball = cfg.joint_ball_factor * r_max
        if embedded and len(cluster_nodes) == 1:
            # adsorbed model foot: adhesion fillet, no second member
            r_ball = MODEL_FOOT_FACTOR * r_end
        # F1 addendum (verifier should-fix): the node-band exclusion above
        # only tests the NODE z against PLATE_BAND_MM, BEFORE the ball's
        # radius is known -- a node comfortably outside the 0.2mm band can
        # still size up into a sphere whose BOTTOM crosses the plate
        # (real-data specimen: node 0.806mm above plate_z, r=0.8268,
        # sphere bottom 0.0211mm below plate_z). Now that r_ball is final,
        # judge the actual sphere geometry: center_z - r < plate_z means
        # the sphere would poke through the build plate.
        #
        # Design decision: DROP the ball rather than clamp its radius or
        # lift its center. These near-plate balls sit on structure whose
        # reinforcement the foot flare/pad already provides -- the same
        # F1 rationale as the node-band exclusion just above -- so a
        # dropped ball loses no real support. Clamping would silently
        # shrink or reposition a ball sized against a real member's
        # radius and make the emitted geometry lie about what was
        # actually decided; a missing entry is honest and cheap to audit.
        if center[2] - r_ball < plate_z:
            continue
        # Enclosure diet (module docstring): the ball's sphere already
        # lies entirely inside material the touching members' own
        # frustums provide -- emitting it adds print volume without
        # adding print geometry. Dropped from the LIST, not clamped or
        # repositioned, same honesty argument as the plate-crossing drop
        # just above: every other ball here keeps the exact center/radius
        # its sizing decided.
        if _ball_enclosed(np.asarray(center, float), float(r_ball),
                           list(touching_members.values()), ENCLOSURE_PROOF_TOL_MM):
            continue
        balls.append(dict(center=np.asarray(center, float), r=float(r_ball),
                          owner=owner))
    return balls


def balls_by_owner(balls):
    """Group compute_joint_balls output for the emitter:
    {("trunk", i): [(center, r), ...], ...}"""
    out = {}
    for b in balls:
        out.setdefault(b["owner"], []).append((b["center"], b["r"]))
    return out
