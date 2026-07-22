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
# EVERY sampled point on its sphere resolves as contained; any single
# uncovered sample keeps it.
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
ENCLOSURE_SAMPLE_N = 256  # coarse global sample count, see _fibonacci_sphere_dirs
# Adversarial-review findings (independent verifier, two blockers found
# on the first cut of this filter, both fixed here -- see
# _ball_enclosed's docstring for the full pipeline these motivate):
#
# BLOCKER 1 (sampling gap): a fixed N=256 Fibonacci grid has a MEASURED
# covering radius of ~9.7 degrees (worst point on the sphere is that far
# from every sample direction) -- a real but narrow uncovered sliver
# (e.g. two members meeting at a shallow bend just past
# COLINEAR_ANGLE_DEG) can dodge every one of the 256 samples and get
# wrongly certified "fully covered". A fixed-N global sampler alone
# cannot promise catching every gap down to ENCLOSURE_TOL_MM. Fixed by
# three targeted, ADAPTIVE follow-up passes that only run when the
# coarse grid reports full coverage -- _seams_confirm_enclosed (dense
# resampling in a band straddling every credited member's own coverage
# boundary -- flat cap OR, for a member thinner than the ball, the cone
# around its own axis; any real gap is proven to border one of these),
# _corners_confirm_enclosed (dense resampling around the exact points
# where two members' boundaries cross -- multi-member "corner" gaps a
# single member's own azimuthal sweep can straddle without landing on),
# and _meridian_confirm_enclosed (analytically solves, per meridian,
# exactly where every OTHER member's boundary crosses it, for hand-offs
# that don't happen to fall through a true 3-D corner). Also fixed:
# _in_frustum_mask's distance-to-solid math had its own bug in the
# "past an end AND radially outside that end's disk" corner (it measured
# distance to the endpoint instead of to the disk's rim, silently over-
# crediting coverage there) -- see that function's docstring.
#
# BLOCKER 2 (union non-star-shapedness): "every surface sample covered
# implies the solid sphere is covered" is only valid when the ball's
# CENTER lies inside every frustum being credited for that coverage --
# a convex frustum containing both the center and a covered surface
# point contains the whole segment between them (star-shaped-from-
# center), which is what actually licenses the surface-sampling
# shortcut. With 3+ members meeting under sub-COINCIDE_TOL_MM jitter --
# exactly what that tolerance exists to permit -- a member whose own
# endpoint is offset from the ball's chosen center can cover some
# surface points without its frustum containing the center itself,
# silently breaking that inference. Fixed in _ball_enclosed: only
# CREDITED members (center verified inside their own frustum) may
# contribute coverage.
#
# The enclosure PROOF (the whole pipeline above) runs against a tighter
# internal tolerance than the stated ENCLOSURE_TOL_MM feature-scale
# bound -- a 10% safety margin against the fact that any FINITE
# sampling/densification scheme, however dense, cannot certify an exact
# real-valued threshold with zero residual risk at the boundary itself
# (adversarial review found real cases within ~0.0003mm of the raw
# 0.02mm line that a still-finite densification pass could miss).
# Proving containment at 0.018mm instead means any ball actually
# dropped has a true worst-case gap safely under the stated 0.02mm
# argument, with margin to spare -- this doesn't change what the
# tolerance MEANS (0.02mm is still the printable-feature-scale fact
# motivating the diet), only how confidently the proof has to clear it
# before trusting a drop.
ENCLOSURE_PROOF_TOL_MM = 0.9 * ENCLOSURE_TOL_MM
_SEAM_N_THETA = 720  # 0.5 degree azimuthal resolution around each seam
# Deliberately swept well past the coarse grid's own ~10 degree measured
# covering radius: an uncovered region bordering a seam is not bounded
# by that number (a real repro measured 13.6 degrees deep from the
# nearest seam and was NOT caught by the 256-point coarse grid either --
# "the coarse grid backstops anything beyond ~10 degrees" is false).
# Swept out to 60 degrees so only a gap that consumes most of a
# credited member's own hemisphere could still be missed here, and a
# gap that large is caught by the coarse 256-point grid directly
# (it is no longer a thin sliver).
_SEAM_PHI_BANDS_DEG = (0.005, 0.01, 0.02, 0.05, 0.12, 0.3, 0.7, 1.5, 3.0, 6.0,
                        11.0, 20.0, 35.0, 50.0, 65.0)
_SEAM_THETAS = np.linspace(0.0, 2.0 * np.pi, _SEAM_N_THETA, endpoint=False)
_SEAM_COS_T = np.cos(_SEAM_THETAS)
_SEAM_SIN_T = np.sin(_SEAM_THETAS)


def _fibonacci_sphere_dirs(n):
    """n unit vectors (n, 3) evenly covering the sphere via the
    Fibonacci-sphere construction. Fully deterministic and seedless --
    no RNG anywhere, just a fixed closed-form point-index -> angle map --
    so the same n always produces bit-identical directions across calls,
    processes, and runs (the determinism the enclosure test, and its
    test_joints pin, need)."""
    i = np.arange(n, dtype=float)
    z = 1.0 - 2.0 * (i + 0.5) / n
    r_xy = np.sqrt(np.clip(1.0 - z * z, 0.0, None))
    golden_angle = np.pi * (3.0 - np.sqrt(5.0))
    theta = golden_angle * i
    return np.stack([r_xy * np.cos(theta), r_xy * np.sin(theta), z], axis=1)


_ENCLOSURE_SAMPLE_DIRS = _fibonacci_sphere_dirs(ENCLOSURE_SAMPLE_N)


def _in_frustum_mask(points, p0, p1, r0, r1, tol):
    """Boolean (N,) containment of `points` (N,3) in the frustum p0->p1
    (radius r0 at p0, r1 at p1) -- true Euclidean distance to the
    solid, flat-capped frustum (a cone with two flat end DISKS, not a
    rounded-cap capsule), tolerant by `tol` mm.

    A point's position relative to the member decomposes into an axial
    coordinate (mm from p0, unclamped) and a radial (perpendicular-to-
    axis) distance. Three regions, each with its own correct nearest-
    point-on-solid distance (this is NOT "clamp axial, then take radial
    distance from the clamped point" -- an earlier version of this
    function did exactly that, which collapses to measuring distance to
    the ENDPOINT rather than to the flat cap's disk/rim whenever a
    point is both past an end and radially outside that end's radius;
    that silently OVER-credited coverage in that corner region, an
    adversarial-review finding -- see BLOCKER comments above
    ENCLOSURE_SAMPLE_N):

    - axial position within [0, L]: distance is the radial overshoot
      past the taper-interpolated radius there (0 if inside).
    - axial position past an end, radially INSIDE that end's disk:
      distance is just the axial overshoot (nearest point is straight
      "below" on the flat cap).
    - axial position past an end, radially OUTSIDE that end's disk:
      distance is to the disk's RIM circle (axial and radial overshoot
      combined via Pythagoras) -- the case the old formula got wrong."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        dist = np.linalg.norm(points - p0[None, :], axis=1)
        return dist <= r0 + tol
    L = float(np.sqrt(L2))
    d_hat = d / L
    s = (points - p0[None, :]) @ d_hat        # axial coordinate, mm, from p0
    rel = points - p0[None, :] - s[:, None] * d_hat[None, :]
    radial = np.linalg.norm(rel, axis=1)
    t = np.clip(s, 0.0, L) / L
    local_r = r0 + (r1 - r0) * t

    dist = np.empty(len(points))
    inside_span = (s >= 0.0) & (s <= L)
    dist[inside_span] = np.maximum(0.0, radial[inside_span] - local_r[inside_span])
    below = s < 0.0
    if below.any():
        axial_over = -s[below]
        rad = radial[below]
        on_disk = rad <= r0
        dist[below] = np.where(on_disk, axial_over, np.hypot(axial_over, rad - r0))
    above = s > L
    if above.any():
        axial_over = s[above] - L
        rad = radial[above]
        on_disk = rad <= r1
        dist[above] = np.where(on_disk, axial_over, np.hypot(axial_over, rad - r1))
    return dist <= tol


def _perp_basis(axis):
    """Two unit vectors e1, e2 completing `axis` (unit) to an orthonormal
    basis -- deterministic (fixed reference vector choice, no RNG)."""
    ref = np.array([1.0, 0.0, 0.0]) if abs(axis[0]) < 0.9 else np.array([0.0, 1.0, 0.0])
    e1 = ref - axis * (ref @ axis)
    e1 = e1 / np.linalg.norm(e1)
    e2 = np.cross(axis, e1)
    return e1, e2


def _seam_boundaries(center, r, members, tol):
    """For every member, yield (axis, phi0) for each of its two sides
    (toward p1, toward p0) whenever that side's coverage is NOT
    unconditional -- axis/phi0 describe the boundary in an elevation
    convention: this member covers a sample direction u near this seam
    iff u.axis >= sin(phi0).

    A member's coverage on one side is bounded by TWO independent
    conditions, and the real boundary is whichever is more restrictive
    (thresholds combine by max, since both are of the form u.axis>=k):

    - axial: the ball's sphere reaches past that end's flat-cap plane
      (a thin/short member -- the case the module docstring's elbow
      story is about).
    - radial: the member is simply THINNER than this ball (local
      radius < r) -- even mid-span, comfortably clear of both its own
      ends, such a member only covers a CONE around its own axis (half-
      angle arcsin(local_r/r)), never a full hemisphere. Missing this
      was a real bug found by adversarial review: a thin credited
      member was silently treated as covering as much as a same-radius
      one would, understating how little of the sphere it actually
      backs up.

    A side contributes no boundary only when BOTH conditions are slack
    (member reaches well past this ball in that direction AND is at
    least as fat as the ball there) -- the "mid-span fat through-trunk"
    case, which needs no seam care at all."""
    out = []
    for m in members:
        p0, p1 = m["p0"], m["p1"]
        d = p1 - p0
        L2 = float(d @ d)
        if L2 < 1e-18:
            continue
        L = float(np.sqrt(L2))
        d_hat = d / L
        s0 = float((center - p0) @ d_hat)  # center's axial offset from p0
        r_min = min(float(m["r0"]), float(m["r1"]))  # conservative: never overstate radial reach
        radial_ratio = (r_min + tol) / r
        k_radial = float(np.sqrt(max(0.0, 1.0 - radial_ratio * radial_ratio))) if radial_ratio < 1.0 else None

        c1 = (L + tol - s0) / r   # near-p1 axial plane, in units of r from center
        k_p1 = -c1
        if k_radial is not None:
            k_p1 = max(k_p1, k_radial)
        if abs(k_p1) < 1.0:
            out.append((-d_hat, float(np.arcsin(np.clip(k_p1, -1.0, 1.0)))))

        c0 = (tol - s0) / r       # near-p0 axial plane, in units of r from center
        k_p0 = c0
        if k_radial is not None:
            k_p0 = max(k_p0, k_radial)
        if abs(k_p0) < 1.0:
            out.append((d_hat, float(np.arcsin(np.clip(k_p0, -1.0, 1.0)))))
    return out


_CORNER_PATCH_N_THETA = 48
_CORNER_PATCH_RADII_DEG = (0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 4.0,
                            8.0, 15.0, 25.0, 40.0)
_CORNER_THETAS = np.linspace(0.0, 2.0 * np.pi, _CORNER_PATCH_N_THETA, endpoint=False)
_CORNER_COS_T = np.cos(_CORNER_THETAS)
_CORNER_SIN_T = np.sin(_CORNER_THETAS)


def _circle_circle_intersections(n1, k1, n2, k2):
    """Unit-vector solutions of {u.n1=k1, u.n2=k2, |u|=1} -- the (up to
    two) directions where two members' own seam circles cross. n1,n2
    are the members' seam axes (unit), k1=sin(phi0_1), k2=sin(phi0_2)
    the corresponding boundary thresholds from _seam_boundaries.
    Standard basis expansion u=a.n1+b.n2+c'.w (w = unit n1 x n2);
    returns [] if the axes are (near-)parallel or the circles don't
    meet."""
    m = float(n1 @ n2)
    denom = 1.0 - m * m
    if denom < 1e-10:
        return []
    a = (k1 - k2 * m) / denom
    b = (k2 - k1 * m) / denom
    rem = 1.0 - a * a - b * b - 2.0 * a * b * m
    if rem < 0.0:
        return []
    cross = np.cross(n1, n2)
    w = cross / np.sqrt(denom)
    base = a * n1 + b * n2
    cprime = float(np.sqrt(max(rem, 0.0)))
    if cprime < 1e-9:
        norm = np.linalg.norm(base)
        return [base / norm] if norm > 1e-9 else []
    return [base + cprime * w, base - cprime * w]


def _corners_confirm_enclosed(center, r, credited, tol):
    """Third-opinion check (multi-member "corner" case): where 3+ near-
    seam members meet, the deepest residual gap can sit at a point where
    TWO members' own seam circles cross -- close to both boundaries at
    once -- which a single member's own 1-D azimuthal seam sweep
    (_seams_confirm_enclosed) is not guaranteed to land exactly on
    (its theta step is a fixed global grid, not centered on that
    corner). For every PAIR of near-seam boundaries, compute their
    (up to two) exact crossing directions and densely resample a small
    disk of directions around each -- catches gaps localized at these
    boundary intersections regardless of azimuthal alignment."""
    boundaries = _seam_boundaries(center, r, credited, tol)
    if len(boundaries) < 2:
        return True
    for i in range(len(boundaries)):
        axis_i, phi0_i = boundaries[i]
        k_i = float(np.sin(phi0_i))
        for j in range(i + 1, len(boundaries)):
            axis_j, phi0_j = boundaries[j]
            k_j = float(np.sin(phi0_j))
            for u0 in _circle_circle_intersections(axis_i, k_i, axis_j, k_j):
                e1, e2 = _perp_basis(u0)
                for rad_deg in _CORNER_PATCH_RADII_DEG:
                    rad = np.radians(rad_deg)
                    cos_r, sin_r = float(np.cos(rad)), float(np.sin(rad))
                    dirs = (cos_r * u0[None, :]
                            + sin_r * (_CORNER_COS_T[:, None] * e1[None, :]
                                        + _CORNER_SIN_T[:, None] * e2[None, :]))
                    pts = center[None, :] + r * dirs
                    covered = np.zeros(len(pts), dtype=bool)
                    for m in credited:
                        covered |= _in_frustum_mask(pts, m["p0"], m["p1"], m["r0"], m["r1"], tol)
                        if covered.all():
                            break
                    if not covered.all():
                        return False
    return True


_MERIDIAN_EPS_DEG = np.radians(np.array([0.003, 0.01, 0.04, 0.15, 0.6]))


def _meridian_confirm_enclosed(center, r, credited, tol):
    """Fourth-opinion check: restricted to any ONE near-seam member's
    azimuthal sweep (a meridian through its axis), every OTHER credited
    member's own coverage is a single contiguous elevation interval
    (its boundary crosses that meridian at 0, 1, or 2 exact,
    analytically solvable points). The union's hand-off point on that
    meridian therefore always sits at one of these crossings -- not
    necessarily at any of _seams_confirm_enclosed's hand-picked global
    bands, and not necessarily at one of _corners_confirm_enclosed's
    exact circle-circle vertices either (two members can hand off
    coverage to each other along a meridian that does not pass through
    their true 3-D intersection point). Solves u(phi).axis_m = k_m
    (A.sin(phi)+B.cos(phi)=C) in closed form per theta, then samples
    immediately on both sides of every solution."""
    boundaries = _seam_boundaries(center, r, credited, tol)
    if not boundaries:
        return True
    cos_t, sin_t = _SEAM_COS_T, _SEAM_SIN_T
    for axis0, _phi0 in boundaries:
        e1, e2 = _perp_basis(axis0)
        e0_arr = cos_t[:, None] * e1[None, :] + sin_t[:, None] * e2[None, :]
        for axis_m, phi0_m in boundaries:
            A = float(axis0 @ axis_m)
            B_arr = e0_arr @ axis_m
            C = float(np.sin(phi0_m))
            R_arr = np.hypot(A, B_arr)
            valid = R_arr >= abs(C)
            if not valid.any():
                continue
            ratio = np.clip(np.where(valid, C / np.where(valid, R_arr, 1.0), 0.0), -1.0, 1.0)
            delta_arr = np.arctan2(B_arr, A)
            base_arr = np.arcsin(ratio)
            for cand_arr in (base_arr - delta_arr, np.pi - base_arr - delta_arr):
                wrapped = (cand_arr + np.pi) % (2.0 * np.pi) - np.pi
                keep = valid & (wrapped >= -np.pi / 2.0) & (wrapped <= np.pi / 2.0)
                if not keep.any():
                    continue
                idx = np.nonzero(keep)[0]
                phis_c = wrapped[idx]
                e0_k = e0_arr[idx]
                for eps in _MERIDIAN_EPS_DEG:
                    for sign in (-1.0, 1.0):
                        phi = phis_c + sign * eps
                        in_range = (phi >= -np.pi / 2.0) & (phi <= np.pi / 2.0)
                        if not in_range.any():
                            continue
                        phi_k = phi[in_range]
                        e0_kk = e0_k[in_range]
                        pts = (center[None, :]
                               + r * np.sin(phi_k)[:, None] * axis0[None, :]
                               + r * np.cos(phi_k)[:, None] * e0_kk)
                        covered = np.zeros(len(pts), dtype=bool)
                        for mm in credited:
                            covered |= _in_frustum_mask(pts, mm["p0"], mm["p1"], mm["r0"], mm["r1"], tol)
                            if covered.all():
                                break
                        if not covered.all():
                            return False
    return True


def _seams_confirm_enclosed(center, r, credited, tol):
    """Second-opinion check, run only after the coarse global grid
    already found every one of its 256 samples covered: densely
    resample a thin band straddling every credited member's own seam
    (its flat-cap plane, wherever it crosses this ball's sphere).

    Why a real gap must border a seam: the union of the credited
    members' frustums is a union of convex sets; the boundary of that
    union (where covered turns to uncovered) is contained in the union
    of the INDIVIDUAL members' own boundaries (a union can only gain
    area, never lose it, by adding a set -- so its edge can't cut
    through a region two sets both already agree on). So if any surface
    point is genuinely uncovered, walking from it toward covered
    territory crosses some member's own seam -- meaning a thin enough
    band around every seam, swept through the full 360 degrees, will
    graze any such gap regardless of how narrow it is, INCLUDING gaps
    the coarse 256-point grid's ~10 degree covering radius (measured)
    dodges entirely. Each band sample is deliberately placed strictly on
    the uncovered side of its own seam (phi0 - band, never at phi0
    itself, where the member trivially covers by construction and would
    tell us nothing)."""
    for axis, phi0 in _seam_boundaries(center, r, credited, tol):
        e1, e2 = _perp_basis(axis)
        for band_deg in _SEAM_PHI_BANDS_DEG:
            phi = phi0 - np.radians(band_deg)
            if phi <= -np.pi / 2.0:
                continue
            cos_phi, sin_phi = float(np.cos(phi)), float(np.sin(phi))
            pts = (center[None, :]
                   + r * sin_phi * axis[None, :]
                   + r * cos_phi * (_SEAM_COS_T[:, None] * e1[None, :]
                                     + _SEAM_SIN_T[:, None] * e2[None, :]))
            covered = np.zeros(len(pts), dtype=bool)
            for m in credited:
                covered |= _in_frustum_mask(pts, m["p0"], m["p1"], m["r0"], m["r1"], tol)
                if covered.all():
                    break
            if not covered.all():
                return False
    return True


def _ball_enclosed(center, r, touching_members, tol):
    """True iff the ball's solid sphere is provably contained in the
    union of its touching members' own frustums. Conservative by
    construction at every stage (a single uncovered sample, anywhere,
    at any stage, keeps the ball; an empty credited set or non-positive
    radius never counts as enclosed) and deterministic (fixed sample
    directions throughout, no RNG).

    Three stages, addressing the soundness gaps a from-scratch adversarial
    review found in the first cut of this filter (see the BLOCKER
    comments above ENCLOSURE_SAMPLE_N):

    1. Star-shaped credit restriction (blocker 2): only members whose
       frustum contains the ball's CENTER are credited -- this is what
       makes "surface sample covered" a valid stand-in for "the solid
       ray from center to that sample is covered" (convexity).
    2. Coarse global check, then a seam-targeted second opinion
       (blocker 1): the fixed 256-point grid is cheap and catches large
       gaps immediately; only if it reports full coverage do we pay for
       _seams_confirm_enclosed's dense, TARGETED resampling near every
       credited member's own flat-cap plane, which is where any gap the
       coarse grid could have missed is guaranteed to border.
    3. Corner second-opinion (multi-member follow-up on blocker 1): with
       3+ near-seam members, the deepest residual gap can sit exactly
       where two members' own seam circles cross -- close to both
       boundaries simultaneously, which a single member's own azimuthal
       sweep isn't guaranteed to land on. _corners_confirm_enclosed
       resamples densely around every such exact crossing.
    4. Meridian hand-off check (multi-member follow-up on blocker 1,
       general case): a hand-off between two members' coverage can also
       occur along a meridian that does NOT pass through their true
       circle-circle vertex -- _meridian_confirm_enclosed analytically
       solves, per meridian, exactly where every other member's own
       boundary crosses it, and samples both sides of every crossing."""
    if r <= 0.0 or not touching_members:
        return False
    center_pt = center[None, :]
    credited = [m for m in touching_members
                if bool(_in_frustum_mask(center_pt, m["p0"], m["p1"], m["r0"], m["r1"], tol)[0])]
    if not credited:
        return False

    samples = center[None, :] + r * _ENCLOSURE_SAMPLE_DIRS
    covered = np.zeros(len(samples), dtype=bool)
    for m in credited:
        covered |= _in_frustum_mask(samples, m["p0"], m["p1"], m["r0"], m["r1"], tol)
        if covered.all():
            break
    if not covered.all():
        return False

    if not _seams_confirm_enclosed(center, r, credited, tol):
        return False
    if not _corners_confirm_enclosed(center, r, credited, tol):
        return False
    return _meridian_confirm_enclosed(center, r, credited, tol)


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

    balls = []
    for idxs in clusters.values():
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
        for m in axis_members:
            dist, local_r = _dist_and_local_radius(center, m["p0"], m["p1"], m["r0"], m["r1"])
            if dist <= ON_AXIS_TOL_MM:
                touching_members[id(m)] = m
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
