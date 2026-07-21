"""v4 CCO data model (design-v4.md #2, #3, #4): the Member/Forest schema
the insertion builder (cco.py) produces natively, replacing v3's
Trunk/Route/riser/brace collections and joints.collect_members's
from-parts reconstruction.

Certificate rule (design-v4.md #4, "too foundational for implementer
improvisation"): every member carries a clearance certificate
(geom_version, cert_version, cert_radius). All geometry/radius mutation
goes through Member's controlled __setattr__, which bumps geom_version.
A certificate is valid iff `cert_version == geom_version` (nothing
changed since the last certified check) OR the member's current radius
envelope is no fatter than what was certified (`max(r0, r1) <=
cert_radius`) -- "checked fat covers thin" for a resize-down.

Implementation choice (recorded -- the charter names exactly these three
fields and describes the two rules in prose, leaving the interaction
under-specified): a POSITION change (p0 or p1) resets cert_radius to -1,
so the radius-decrease escape hatch can never fire after a geometry
change even though geom_version alone can't distinguish "which field
changed" after the fact. This makes the two stated rules
("any radius increase or geometry change voids it" / "radius decrease
with unchanged geometry keeps the certificate") both hold exactly,
with no fields beyond the three the charter specifies.
"""
from dataclasses import dataclass
import numpy as np

_POS_FIELDS = ("p0", "p1")
_R_FIELDS = ("r0", "r1")
_TRACKED_FIELDS = _POS_FIELDS + _R_FIELDS


class Member:
    """One straight tapered frustum segment of the v4 support forest.

    p0, p1: (3,) float64 endpoints. r0, r1: radii at those endpoints.
    kind: semantic role string (e.g. "stub", "trunk", "strut", "shoot").
    owner: identifying tuple, e.g. ("contact", i).
    embed_end: "p0" | "p1" | None -- which end (if any) is DESIGNED to
        embed into the model surface (a stub's tip contact, or a
        model-anchor strut's foot). Replaces v3's kind/sentinel logic.
    carried_mm2: cumulative carried peel area flowing through this
        member (own terminal's share, plus everything merged onto it
        from downstream via bifurcation).
    """
    __slots__ = ("p0", "p1", "r0", "r1", "kind", "owner", "embed_end",
                 "carried_mm2", "geom_version", "cert_version", "cert_radius",
                 "_forest", "_idx")

    def __init__(self, p0, p1, r0, r1, kind, owner, embed_end=None, carried_mm2=0.0):
        object.__setattr__(self, "_forest", None)
        object.__setattr__(self, "_idx", None)
        object.__setattr__(self, "geom_version", 0)
        object.__setattr__(self, "cert_version", -1)
        object.__setattr__(self, "cert_radius", -1.0)
        object.__setattr__(self, "kind", kind)
        object.__setattr__(self, "owner", owner)
        object.__setattr__(self, "embed_end", embed_end)
        object.__setattr__(self, "carried_mm2", float(carried_mm2))
        object.__setattr__(self, "p0", np.asarray(p0, dtype=np.float64).copy())
        object.__setattr__(self, "p1", np.asarray(p1, dtype=np.float64).copy())
        object.__setattr__(self, "r0", float(r0))
        object.__setattr__(self, "r1", float(r1))

    def __setattr__(self, name, value):
        if name in _TRACKED_FIELDS:
            forest = object.__getattribute__(self, "_forest")
            idx = object.__getattribute__(self, "_idx")
            if forest is not None and idx is not None:
                forest._touch_member(idx)
            if name in _POS_FIELDS:
                value = np.asarray(value, dtype=np.float64).copy()
            else:
                value = float(value)
            object.__setattr__(self, name, value)
            object.__setattr__(self, "geom_version",
                                object.__getattribute__(self, "geom_version") + 1)
            if name in _POS_FIELDS:
                object.__setattr__(self, "cert_radius", -1.0)
        else:
            object.__setattr__(self, name, value)

    @property
    def cert_valid(self):
        if self.cert_version == self.geom_version:
            return True
        return max(self.r0, self.r1) <= self.cert_radius

    def _snapshot(self):
        return dict(p0=self.p0.copy(), p1=self.p1.copy(), r0=self.r0, r1=self.r1,
                    kind=self.kind, owner=self.owner, embed_end=self.embed_end,
                    carried_mm2=self.carried_mm2, geom_version=self.geom_version,
                    cert_version=self.cert_version, cert_radius=self.cert_radius)

    def _restore(self, snap):
        for k, v in snap.items():
            object.__setattr__(self, k, v.copy() if isinstance(v, np.ndarray) else v)

    def __repr__(self):
        return (f"Member(kind={self.kind!r}, owner={self.owner!r}, "
                f"p0={self.p0.tolist()}, p1={self.p1.tolist()}, "
                f"r0={self.r0:.3f}, r1={self.r1:.3f}, embed_end={self.embed_end!r})")


def _get(m, name):
    """Field access that works for either a Member object or a plain dict
    with the same schema (joints.collect_members's downstream consumers
    and hand-built test fixtures use dicts)."""
    return m[name] if isinstance(m, dict) else getattr(m, name)


class Forest:
    """A forest of Members rooted on the plate or model anchors, plus tree
    topology (parent/children over member indices) and the terminal
    registry. See module docstring for the certificate rule; see
    checkpoint()/rollback() for the transactional mutation protocol
    cco.py's candidate acceptance relies on.

    Convention: for member i, `parent[i]` is the index of the member whose
    p0 endpoint coincides with member i's p1 endpoint (i.e. member i flows
    INTO its parent at the parent's p0 -- p1 is always the root-ward end
    of a member, p0 the terminal-ward end, matching embed_end="p0" for a
    stub's contact tip and embed_end="p1" for a model-anchor strut's
    foot). `parent[i] is None` means member i is a root: its p1 lands
    directly on the plate or a model anchor point (no member there).
    """

    def __init__(self):
        self.members = []          # list[Member]
        self.parent = []           # list[Optional[int]]
        self.children = []         # list[list[int]]
        self.roots = []            # list[int] -- member indices with parent None
        self.terminal_of = {}      # contact_idx -> member idx (that terminal's stub)
        self._cp_stack = []

    # ---- construction / topology ----------------------------------
    def add_member(self, member):
        idx = len(self.members)
        object.__setattr__(member, "_forest", self)
        object.__setattr__(member, "_idx", idx)
        self.members.append(member)
        self.parent.append(None)
        self.children.append([])
        self.roots.append(idx)
        if self._cp_stack:
            self._cp_stack[-1]["new_since"] = min(
                self._cp_stack[-1].get("new_since", idx), idx)
        return idx

    def set_parent(self, child_idx, parent_idx):
        """Attach member `child_idx` to member `parent_idx` at the
        parent's p0 (or detach if parent_idx is None)."""
        self._touch_topology(child_idx)
        old_parent = self.parent[child_idx]
        if old_parent is not None:
            self._touch_topology(old_parent)
            if child_idx in self.children[old_parent]:
                self.children[old_parent].remove(child_idx)
        if parent_idx is not None:
            self._touch_topology(parent_idx)
        self.parent[child_idx] = parent_idx
        if parent_idx is not None:
            if child_idx in self.roots:
                self.roots.remove(child_idx)
            if child_idx not in self.children[parent_idx]:
                self.children[parent_idx].append(child_idx)
        else:
            if child_idx not in self.roots:
                self.roots.append(child_idx)

    # ---- certificates ------------------------------------------------
    def dirty_members(self):
        return [i for i, m in enumerate(self.members) if not m.cert_valid]

    def revalidate(self, surface_tree, cfg):
        """ONE batched clearance re-check over every currently-dirty
        member: sample at cfg.clearance_sample_step_mm, linear radius
        taper, skip the first contact_depth_mm+0.2 of a designed-embed
        end. clearance = kdtree_distance - radius - standoff_mm. Members
        with no violation get their certificate (re)issued; members with
        a violation stay dirty. Returns the violation list.

        "stub" members are skipped entirely (design decision, recorded --
        mirrors routing.sample_route_envelope's existing, unchanged
        precedent: "the stub is rooted against the surface it springs
        from"). The charter's contact_depth_mm+0.2 inset is enough to
        exclude the EMBEDDED point itself, but a stub travels along the
        surface normal at a near-grazing angle to the local surface for
        its first couple of mm by construction -- an inset that size
        still leaves samples well under standoff_mm from the surface,
        which would flag every ordinary stub as a false violation. Since
        v3 already established (and design-v4.md #2 keeps routing.py,
        including this exemption, unchanged as substrate) that the stub
        is the model's own emergence point and not swept, the same rule
        carries over here rather than reintroducing the false-positive
        class routing.py already solved."""
        from . import routing
        dirty = self.dirty_members()
        if not dirty:
            return []
        inset = cfg.contact_depth_mm + 0.2
        all_pts, all_r, all_mi = [], [], []
        for mi in dirty:
            m = self.members[mi]
            if m.kind == "stub":
                continue
            p0, p1 = np.asarray(m.p0, float), np.asarray(m.p1, float)
            length = float(np.linalg.norm(p1 - p0))
            if length < 1e-9:
                pts = p0[None, :]
                radii = np.array([max(m.r0, m.r1)])
            else:
                n = max(int(length / cfg.clearance_sample_step_mm), 1)
                t = np.linspace(0.0, 1.0, n + 1)
                if m.embed_end == "p0":
                    t = t[t * length >= inset]
                if m.embed_end == "p1":
                    t = t[(1 - t) * length >= inset]
                if len(t) == 0:
                    continue
                pts = p0[None, :] + t[:, None] * (p1 - p0)[None, :]
                radii = m.r0 + t * (m.r1 - m.r0)
            all_pts.append(pts)
            all_r.append(radii)
            all_mi.append(np.full(len(radii), mi, dtype=int))

        violations = []
        bad = set()
        if all_pts:
            pts_all = np.concatenate(all_pts)
            r_all = np.concatenate(all_r)
            mi_all = np.concatenate(all_mi)
            ub = float(r_all.max()) + cfg.standoff_mm
            dist, _ = surface_tree.query(pts_all, workers=routing.query_workers(len(pts_all)),
                                          distance_upper_bound=ub)
            clearance = dist - r_all - cfg.standoff_mm
            for mi in dirty:
                mask = mi_all == mi
                if not mask.any():
                    continue
                worst = float(clearance[mask].min())
                if worst < 0:
                    bad.add(mi)
                    widx = np.nonzero(mask)[0][int(np.argmin(clearance[mask]))]
                    violations.append(dict(member_idx=mi, min_clearance_mm=worst,
                                            at_point=pts_all[widx].tolist()))
        for mi in dirty:
            if mi in bad:
                continue
            m = self.members[mi]
            m.cert_version = m.geom_version
            m.cert_radius = max(m.r0, m.r1)
        return violations

    # ---- transactions --------------------------------------------------
    def _touch_member(self, idx):
        if not self._cp_stack:
            return
        top = self._cp_stack[-1]
        if idx < top["n_members"] and idx not in top["members"]:
            top["members"][idx] = self.members[idx]._snapshot()

    def _touch_topology(self, idx):
        if not self._cp_stack:
            return
        top = self._cp_stack[-1]
        if idx < top["n_members"] and idx not in top["topo"]:
            top["topo"][idx] = (self.parent[idx], list(self.children[idx]))

    def checkpoint(self):
        self._cp_stack.append(dict(
            n_members=len(self.members), members={}, topo={},
            roots=list(self.roots), terminal_of=dict(self.terminal_of)))
        return len(self._cp_stack) - 1

    def rollback(self, cp):
        assert cp == len(self._cp_stack) - 1, "rollback must target the innermost checkpoint"
        top = self._cp_stack.pop()
        for idx, snap in top["members"].items():
            self.members[idx]._restore(snap)
        for idx, (p, ch) in top["topo"].items():
            self.parent[idx] = p
            self.children[idx] = ch
        n0 = top["n_members"]
        del self.members[n0:]
        del self.parent[n0:]
        del self.children[n0:]
        self.roots = top["roots"]
        self.terminal_of = top["terminal_of"]

    def commit(self, cp):
        assert cp == len(self._cp_stack) - 1, "commit must target the innermost checkpoint"
        self._cp_stack.pop()


def assert_all_certified(forest):
    """The certificate-rule invariant (design-v4.md #4): nothing is
    accepted or emitted with a void certificate. Crash, not a warning --
    a caller (engine.run stands in for "emission" until Phase 7 wires a
    real emitter) must call this before trusting a forest's geometry.
    Raises AssertionError naming the uncertified members if any exist."""
    dirty = forest.dirty_members()
    assert not dirty, (
        f"forest has {len(dirty)} uncertified member(s) -- crash over "
        f"corruption, not a silent pass: {dirty}")


def volume_report(members, balls):
    """Frustum + ball volume accounting, ml, EXACTLY the accounting that
    froze the Treant v3.7 baseline (design-v4.md #11): frustum
    pi/3*L*(r0^2+r0*r1+r1^2)/1000, ball 4/3*pi*r^3/1000. Works over either
    Member objects or plain member dicts (see _get)."""
    by_kind = {}
    member_ml = 0.0
    for m in members:
        p0 = np.asarray(_get(m, "p0"), float)
        p1 = np.asarray(_get(m, "p1"), float)
        r0 = float(_get(m, "r0"))
        r1 = float(_get(m, "r1"))
        L = float(np.linalg.norm(p1 - p0))
        v = np.pi / 3.0 * L * (r0 ** 2 + r0 * r1 + r1 ** 2) / 1000.0
        member_ml += v
        kind = _get(m, "kind")
        by_kind[kind] = by_kind.get(kind, 0.0) + v
    ball_ml = 0.0
    for b in balls:
        r = float(b["r"] if isinstance(b, dict) else b.r)
        ball_ml += 4.0 / 3.0 * np.pi * r ** 3 / 1000.0
    return dict(member_ml=member_ml, ball_ml=ball_ml, total_ml=member_ml + ball_ml,
                by_kind_ml=by_kind)


def printability_report(members, cfg):
    """Per-segment rim-overhang printability rule (design-v4.md #3): with
    z the build (up) axis, drift = |dc_xy|/dz, taper = (r_top-r_bot)/dz
    (signed -- an upward taper is a credit); violation if
    max(0, drift+taper) > tan(theta_max(r)). theta_max(r) is
    cfg.branch_angle_deg if min(r0,r1) >= cfg.min_shoot_r_mm, else
    cfg.max_segment_angle_deg. Near-horizontal members (dz < 1e-6) are a
    violation unless kind == "model_rung" and length <=
    cfg.model_brace_max_len_mm (v3's length-capped rung exemption).

    Design decision (recorded, ambiguity in the charter): "stub" members
    are exempt, mirroring routing.sample_route_envelope's existing stub
    exemption -- a contact stub follows the model's surface normal by
    physical necessity (not a choice the builder makes), so subjecting it
    to the same cone rule as a free-standing shaft would flag nearly
    every contact on any non-vertical surface. Balls are exempt by
    construction (this function only ever sees frustum members)."""
    violations = []
    for mi, m in enumerate(members):
        kind = _get(m, "kind")
        if kind == "stub":
            continue
        p0 = np.asarray(_get(m, "p0"), float)
        p1 = np.asarray(_get(m, "p1"), float)
        r0 = float(_get(m, "r0"))
        r1 = float(_get(m, "r1"))
        length = float(np.linalg.norm(p1 - p0))
        if p0[2] <= p1[2]:
            bottom, top, r_bot, r_top = p0, p1, r0, r1
        else:
            bottom, top, r_bot, r_top = p1, p0, r1, r0
        dz = float(top[2] - bottom[2])
        if dz < 1e-6:
            if kind == "model_rung" and length <= cfg.model_brace_max_len_mm + 1e-9:
                continue
            violations.append(dict(member_idx=mi, kind=kind, reason="near_horizontal",
                                    length_mm=length))
            continue
        drift = float(np.linalg.norm(top[:2] - bottom[:2])) / dz
        taper = (r_top - r_bot) / dz
        lhs = max(0.0, drift + taper)
        theta_max = (cfg.branch_angle_deg if min(r0, r1) >= cfg.min_shoot_r_mm
                     else cfg.max_segment_angle_deg)
        rhs = float(np.tan(np.radians(theta_max)))
        if lhs > rhs + 1e-9:
            violations.append(dict(member_idx=mi, kind=kind,
                                    drift_rate=drift, taper_rate=taper,
                                    limit_deg=theta_max,
                                    actual_deg=float(np.degrees(np.arctan(lhs)))))
    return violations
