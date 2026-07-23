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
# carried_mm2 is transactionally snapshotted (Phase 3 fix -- see __setattr__)
# but is NOT a certificate field: it doesn't affect swept geometry, so a
# carried_mm2-only change must not bump geom_version/void cert_radius the
# way a real p0/p1/r0/r1 mutation does.
_SNAPSHOT_ONLY_FIELDS = ("carried_mm2",)


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

    def __getstate__(self):
        """Pickle support (Treant-scale staged runs checkpoint the whole
        Forest): dump slots raw; the Forest<->Member cycle is handled by
        pickle's memo, and __setstate__ bypasses the guarded setattr
        (which would otherwise fire before _forest exists)."""
        return {s: object.__getattribute__(self, s) for s in self.__slots__}

    def __setstate__(self, state):
        for k, v in state.items():
            object.__setattr__(self, k, v)

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
        elif name in _SNAPSHOT_ONLY_FIELDS:
            # Phase 3 fix (verifier-caught corruption): cco._propagate_
            # murray/rewire._slim_ancestors always set carried_mm2 BEFORE
            # r0/r1 on the same member. Before this fix, carried_mm2 was
            # invisible to the checkpoint/rollback protocol -- the r0/r1
            # write's _touch_member call snapshotted the member AFTER
            # carried_mm2 had already changed, so a rolled-back candidate
            # permanently baked in that carried_mm2 delta even though its
            # geometry correctly reverted (measured repro: a rejected
            # rewire.py reattach candidate left a sibling member's
            # carried_mm2 drifting across every subsequent trial in the
            # same sweep, eventually undersizing it against its own
            # load). Touching BEFORE the mutation (matching the tracked-
            # field branch above) captures the true pre-transaction value.
            forest = object.__getattribute__(self, "_forest")
            idx = object.__getattribute__(self, "_idx")
            if forest is not None and idx is not None:
                forest._touch_member(idx)
            object.__setattr__(self, name, float(value))
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
        self.braces = []           # list[int] -- member idx of NON-TREE Forest members
                                    # (design-v4.md #9): graph edges (X-pair cross-links,
                                    # model rungs), not parent/child topology. Append-only
                                    # alongside `members`, mirroring add_member's own
                                    # invariant -- see add_brace. Braces ride certificates/
                                    # volume/printability/checkpoint generically BY BEING
                                    # MEMBERS; they never enter roots/children/terminal_of
                                    # because they carry no tree-topology role (Murray
                                    # conservation and rewire's own move candidates exclude
                                    # `forest.braces` from what they treat as real topology).
        self.dead = set()          # member idx retired by rewire.py's twig prune (Phase
                                    # 3, design-v4.md #7): the checkpoint/rollback
                                    # protocol is append-only (rollback truncates the
                                    # member list from the tail, see rollback() below),
                                    # so an interior member can never be physically
                                    # removed -- a pruned member is instead marked dead
                                    # (its volume/carried load already folded into its
                                    # surviving neighbor) and every downstream consumer
                                    # (volume_report, verify, emission) is fed a member
                                    # list that already excludes `dead` indices, the same
                                    # pattern already used for honestly-unrouted stubs.
        self._cp_stack = []
        self._pitch_bound_cache = None   # (surface_tree, pitch_bound_mm) -- see
                                          # _surface_pitch_bound_mm below.

    def __getstate__(self):
        """Pickle support (Treant-scale staged runs checkpoint the whole
        Forest, see Member.__getstate__): drop _pitch_bound_cache, not
        keep it. It holds a strong reference to a full cKDTree (measured:
        17.5MB pickled for a 400k-point tree) that every OTHER part of
        this checkpoint protocol deliberately does NOT persist (surface_
        tree is always rebuilt fresh from the mesh per stage/process, see
        manual_treant_v4.py's own _tree() helper) -- keeping this one
        derived, purely-a-perf-cache field would silently reintroduce
        exactly the bloat that convention avoids, for a value that's
        cheap (~0.16s/400k pts, measured) to recompute on first use after
        unpickling anyway."""
        state = self.__dict__.copy()
        state["_pitch_bound_cache"] = None
        return state

    def __setstate__(self, state):
        self.__dict__.update(state)

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

    def add_brace(self, member):
        """Register a NON-TREE Forest member (design-v4.md #9, SPEC-BRACING
        Part 1): mirrors add_member EXACTLY (forest/idx wiring, members/
        parent/children append, checkpoint frame's new_since bump) except
        it does NOT enter `roots` -- instead its index is appended to
        `self.braces`. A brace is a graph edge (X-pair cross-link, model
        rung), never tree topology: it never gets a parent, never gets
        children attached under it, and is never a terminal's stub."""
        idx = len(self.members)
        object.__setattr__(member, "_forest", self)
        object.__setattr__(member, "_idx", idx)
        self.members.append(member)
        self.parent.append(None)
        self.children.append([])
        self.braces.append(idx)
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

    # ---- retirement (rewire.py twig prune, design-v4.md #7) -----------
    def mark_dead(self, idx):
        """Retire member `idx` (see `dead` field docstring): the caller
        (rewire._try_twig_prune) must already have folded its geometry/
        load into a surviving member and detached it (set_parent(idx,
        None)) before calling this -- mark_dead only records that idx is
        no longer live, it does not itself touch topology."""
        self.dead.add(idx)

    # ---- certificates ------------------------------------------------
    def dirty_members(self):
        return [i for i, m in enumerate(self.members) if not m.cert_valid]

    def revalidate(self, geom, cfg):
        """ONE batched clearance re-check over every currently-dirty member.

        v5 geometry core (SPEC-V5-GEOM sec 2.1): when `geom` is a surface-
        EXACT oracle (geometry_exact.ExactSurface -- duck-typed on
        `.segment_clearance`), clearance is measured against the ACTUAL
        triangles via exact swept-capsule distance, sanctioned end-embedding
        is honored by an inset that excludes the designed embed zone
        (contact_depth_mm + r_end + standoff_mm -- the pitch term is
        identically 0 for an exact oracle: there is no approximating cloud to
        budget for, so the whole surface_tree_pitch_bound_mm derivation of the
        sampled path below collapses), mid-path penetration is caught by a
        gated axis penetration-interval test, and stub members get a
        through-transit interval check instead of the old blanket skip (the
        census found illegal through-and-through stubs the skip waved past).
        When `geom` is a plain cKDTree (unit tests / legacy), the historical
        SAMPLED path runs unchanged -- SPEC sec 2.6 keeps build_surface_tree
        alive for tests/legacy; production (engine.run) always builds and
        threads an ExactSurface, so every load-bearing production clearance
        query is exact. Members with no violation get their certificate
        (re)issued; violators stay dirty. Returns the violation list."""
        if hasattr(geom, "segment_clearance"):
            return self._revalidate_exact(geom, cfg)
        return self._revalidate_sampled(geom, cfg)

    @staticmethod
    def _embed_inset_arc(geom, m, cfg):
        """Angle-aware sanctioned-embed inset (arc length excluded from the
        exact clearance / interval checks) at each designed embed_end.

        A member whose foot is DESIGNED to sit on the model surface grazes
        the standoff envelope as its shaft dives toward that foot; the arc
        length of this SANCTIONED approach depends on the approach angle -- a
        perpendicular dive clears the envelope in contact_depth_mm + r_end +
        standoff_mm, but an ANGLED dive (a strut/model-rung foot leans up to
        the 60deg branch/strut family) legitimately stays within the envelope
        proportionally longer. Project the embedded end to the true surface,
        take the angle between the member axis and the local surface normal,
        and divide the perpendicular reach by its cosine (floored at 60deg so
        a near-grazing approach cannot inflate the exempt zone without bound).
        Non-embed ends get 0.

        This is the exact-oracle replacement for the sampled path's
        contact_depth + PITCH_BOUND + r_end + standoff: the pitch term
        (sampling uncertainty) is correctly 0 for an exact oracle (SPEC sec
        2.1), but the geometry of an angled designed approach that the bare
        perpendicular formula under-derives is restored from the TRUE surface
        normal, not from a guessed cloud-density budget. It matches what the
        pitch term was empirically providing for embed approaches (measured
        ~2.04 vs ~2.02mm on the table) while forbidding a mid-path clip, whose
        argmin sits at the full perpendicular distance far from any anchor."""
        p0 = np.asarray(m.p0, float)
        p1 = np.asarray(m.p1, float)
        L = float(np.linalg.norm(p1 - p0))

        def _arc(p_embed, u_out, r_end):
            if L < 1e-9:
                return cfg.contact_depth_mm + r_end + cfg.standoff_mm
            _d, _cp, _ti, n = geom.project(p_embed)
            nn = float(np.linalg.norm(n))
            if nn < 1e-9:
                return cfg.contact_depth_mm + r_end + cfg.standoff_mm
            cosang = max(abs(float(np.dot(u_out, n / nn))), 0.5)  # 60deg floor
            return cfg.contact_depth_mm + (r_end + cfg.standoff_mm) / cosang

        inset0 = _arc(p0, (p1 - p0) / L, m.r0) if (m.embed_end == "p0" and L >= 1e-9) else 0.0
        inset1 = _arc(p1, (p0 - p1) / L, m.r1) if (m.embed_end == "p1" and L >= 1e-9) else 0.0
        return inset0, inset1

    @staticmethod
    def _exact_interval_illegal(geom, m, cfg, inset0, inset1):
        """True iff the member's AXIS has an inside-the-solid run longer than
        the census MIN_ILLEGAL_LEN (0.05mm) OUTSIDE its sanctioned embed zones
        -- the mid-path-penetration / stub through-transit test. This is a
        parity-crossing test (exact boundaries, majority-vote inside), never a
        distance test, so it cannot false-positive on an ordinary surface-
        hugging stub. Axis-only, matching the census (census_run.py:52)."""
        p0 = np.asarray(m.p0, float)
        p1 = np.asarray(m.p1, float)
        L = float(np.linalg.norm(p1 - p0))
        if L < 1e-9:
            return False
        lo = inset0
        hi = L - inset1
        if hi - lo <= 1e-9:
            return False
        intervals = geom.penetration_intervals(p0, p1, m.r0, m.r1, rays="axis",
                                                min_illegal_len_mm=0.05)
        for iv in intervals:
            if min(iv["s_b"], hi) - max(iv["s_a"], lo) > 0.05:
                return True
        return False

    def _revalidate_exact(self, geom, cfg):
        """Exact swept-capsule clearance path (SPEC sec 2.1). Non-stub dirty
        members: exact geom.segment_clearance over the non-embed sub-segment
        (< 0 => violation), plus a GATED axis penetration-interval belt for
        the census through-transit class (runs only when a cheap trigger
        fires: clearance within 0.1mm of 0, or the member midpoint parity-
        votes inside -- the latter catches a segment routed wholly inside a
        solid region an unsigned distance test cannot see).

        STUB handling (SPEC sec 2.1): stubs keep the blanket standoff
        exemption -- they are NOT distance-checked (a stub hugs its own
        emergence surface at a grazing angle by construction, so any swept-
        distance check false-positives) -- but they are NO LONGER blanket-
        skipped. Each dirty stub gets the axis through-transit interval
        check: it fails certification iff penetration_intervals on its axis
        finds an inside run > 0.05mm (census MIN_ILLEGAL_LEN) OUTSIDE its
        sanctioned p0 embed zone (the census's illegal through-and-through
        stubs, members 403/357/227). A parity-crossing test, never a distance
        test, so an ordinary surface-hugging stub cannot false-positive. A
        stub's geometry is fixed at creation, so it is dirty (hence checked)
        only until its first revalidate, then certified once and for all --
        the check is O(n_contacts) across a whole run, not per-trial.

        The step-1 deferral -- which depended on step 2.4's interior-face
        demand filter -- has EXPIRED: commit 6aba0f6 landed that filter, so
        contacts seed only on exterior faces. A flagged stub is therefore a
        real through-transit defect, not an interior phantom; letting it stay
        dirty (so assert_all_certified crashes rather than silently emitting
        a support that welds through a fin) is the mandated hard gate."""
        dirty = self.dirty_members()
        if not dirty:
            return []
        seg_mi, p0s, p1s, r0s, r1s, ins0, ins1 = [], [], [], [], [], [], []
        stub_mi = []
        for mi in dirty:
            m = self.members[mi]
            if m.kind == "stub":
                stub_mi.append(mi)  # standoff-exempt, but gets the interval check below
                continue
            i0, i1 = self._embed_inset_arc(geom, m, cfg)
            seg_mi.append(mi)
            p0s.append(np.asarray(m.p0, float))
            p1s.append(np.asarray(m.p1, float))
            r0s.append(m.r0)
            r1s.append(m.r1)
            ins0.append(i0)
            ins1.append(i1)

        violations = []
        bad = set()
        # Stub through-transit rule (SPEC sec 2.1): axis-only interval check,
        # standoff exemption preserved (no segment_clearance for stubs).
        #
        # A printable stub TERMINATES IN OPEN AIR: its stub_end (p1) is where
        # the support column begins. Only such a stub can weld THROUGH a thin
        # feature (census member-357 class: tip embedded, exits, transits a
        # fin, continues in open air). If p1 is INSIDE the solid, the contact
        # is unroutable-by-construction -- its outward normal points into the
        # model body or an overlapping shell (the sec-2.4 interior class),
        # so every candidate support would run through solid and the contact
        # ends up honestly unrouted (excluded from live_member_lists). Such a
        # wholly-buried stub is NOT a printed transit; flagging it would only
        # crash assert_all_certified on geometry that never prints. Gate the
        # interval test on p1 being exterior (one cheap parity vote), so the
        # rule catches real through-transits and leaves interior phantoms to
        # the unrouted path -- exactly the sequencing the step-1 deferral
        # named, now that sec 2.4 keeps genuine exterior overhangs.
        for mi in stub_mi:
            m = self.members[mi]
            if bool(geom.inside(np.asarray(m.p1, float)[None, :])[0]):
                continue
            si0, si1 = self._embed_inset_arc(geom, m, cfg)
            if self._exact_interval_illegal(geom, m, cfg, si0, si1):
                bad.add(mi)
                violations.append(dict(member_idx=mi, min_clearance_mm=0.0,
                                       at_point=None))
        if seg_mi:
            clr, argp = geom.segment_clearance(
                np.array(p0s), np.array(p1s), np.array(r0s), np.array(r1s),
                cfg.standoff_mm, inset0s=np.array(ins0), inset1s=np.array(ins1))
            for k, mi in enumerate(seg_mi):
                m = self.members[mi]
                worst = float(clr[k])
                if worst < 0:
                    bad.add(mi)
                    ap = argp[k]
                    violations.append(dict(
                        member_idx=mi, min_clearance_mm=worst,
                        at_point=(ap.tolist() if np.all(np.isfinite(ap)) else None)))
                    continue
                triggered = worst < 0.1
                if not triggered:
                    mid = 0.5 * (np.asarray(m.p0, float) + np.asarray(m.p1, float))
                    triggered = bool(geom.inside(mid))
                if triggered and self._exact_interval_illegal(geom, m, cfg, ins0[k], ins1[k]):
                    bad.add(mi)
                    violations.append(dict(member_idx=mi, min_clearance_mm=worst,
                                           at_point=None))
        for mi in dirty:
            if mi in bad:
                continue
            m = self.members[mi]
            m.cert_version = m.geom_version
            m.cert_radius = max(m.r0, m.r1)
        return violations

    def _revalidate_sampled(self, surface_tree, cfg):
        """SAMPLED clearance path (legacy / unit-test cKDTree oracle).
        Preserved verbatim from the v4 engine (SPEC sec 2.6 keeps this alive
        for tests/legacy): sample at cfg.clearance_sample_step_mm, linear radius
        taper, skip the embed-end inset described below. clearance =
        kdtree_distance - radius - standoff_mm. Members with no violation
        get their certificate (re)issued; members with a violation stay
        dirty. Returns the violation list.

        "stub" members are skipped entirely (design decision, recorded --
        mirrors routing.sample_route_envelope's existing, unchanged
        precedent: "the stub is rooted against the surface it springs
        from"). A stub travels along the surface normal at a near-grazing
        angle to the local surface for its first couple of mm by
        construction -- ANY fixed inset still leaves samples well under
        standoff_mm from the surface, which would flag every ordinary
        stub as a false violation. Since v3 already established (and
        design-v4.md #2 keeps routing.py, including this exemption,
        unchanged as substrate) that the stub is the model's own
        emergence point and not swept, the same rule carries over here
        rather than reintroducing the false-positive class routing.py
        already solved.

        Embed-end exemption (BUG 2 fix, radius-principled -- p6-routing):
        for a NON-stub member whose embed_end is p0 or p1, that endpoint
        (call it A) is DESIGNED to sit on/at the model surface (a strut's
        anchor foot, a model_rung's foot -- both land at a raw sample
        point off the surface, offset by nothing, the same way a stub's
        contact tip is defined as sitting cfg.contact_depth_mm INSIDE the
        surface -- format.md's "0.3mm INSIDE surface" convention is
        reused here as the general embedding-slop budget for ANY
        designed-embed point, not just the stub tip). A's own sample was
        drawn from a DIFFERENT point set than `surface_tree` (struts.py's
        candidates come from perception.delicacy_field's own mesh.sample()
        call; model_rung's foot IS a surface_tree point, but a strut's
        anchor is not), so a second, independent uncertainty term applies:
        pitch_bound_mm, `routing.surface_tree_pitch_bound_mm`'s
        cfg.surface_pitch_bound_percentile-th percentile of THIS tree's
        own nearest-neighbor gaps (see that function's docstring for the
        full derivation and the real-Treant numbers that motivated it) --
        a deterministic, per-mesh, measured bound on how far A could
        legitimately sit from surface_tree's nearest actual point, not a
        guessed constant.

        A sample P on the member's own centerline at arc-length s from A
        can therefore have its true nearest-surface distance bounded
        below: dist(P) >= s - contact_depth_mm - pitch_bound_mm (triangle
        inequality through the true surface point within contact_depth_mm
        of A, then through surface_tree's nearest actual point to THAT,
        within pitch_bound_mm of it). The member's own radius at P, r(s),
        is <= r_end (r0 if embed_end is p0, r1 if embed_end is p1: the
        radius AT the embedded end) for every s on this endpoint's side,
        because both a strut's foot (its FATTER end, by construction:
        r_strut = max(r_i, r_load, r_defl, min_shoot_r_mm) >= r_i) and a
        model_rung's uniform-radius rod taper monotonically toward, or
        stay flat at, r_end moving away from A. So requiring the WORST
        case -- dist(P) - r(s) - standoff >= (s - contact_depth_mm -
        pitch_bound_mm) - r_end - standoff -- to be non-negative reduces
        to s >= contact_depth_mm + pitch_bound_mm + r_end + standoff_mm:
        exactly `inset` below. Any sample beyond that inset cannot have
        its clearance explained by proximity to A alone -- a violation
        there is real (caused by OTHER geometry the member's straight
        line happens to clip), not a false positive from the intentional
        anchor. This replaces the OLD fixed inset (contact_depth_mm +
        0.2, radius- and standoff-independent), which was proven too
        small for every real radius (r >= min_shoot_r_mm = 0.35mm): its
        own worst-case bound, inset + sample_step - r - standoff, is
        NEGATIVE for r >= 0.2mm at the default cfg -- measured 179/179
        model-anchor cone-survivors and 215/215 model_rung members
        failing clearance by exactly that formula. The 3-term-only
        version (no pitch_bound_mm) closed that but left a SECOND,
        smaller gap at real mesh scale (measured on the Weeping Treant:
        6 members, -0.003..-0.05mm, after the sampling-set mismatch above)
        -- pitch_bound_mm closes it with a measured, not guessed, term.
        The inset grows only as far as THIS member's own end radius, the
        standoff margin, and the measured cloud density require, never
        further (test_tree.py's masking tests cover both directions: an
        anchored member with nothing else nearby now passes; one that
        clips other geometry beyond its own anchor region, or whose
        embedded end is NOT on the model surface, still gets caught)."""
        from . import routing
        dirty = self.dirty_members()
        if not dirty:
            return []
        pitch_bound_mm = self._surface_pitch_bound_mm(surface_tree, cfg)
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
                    inset = cfg.contact_depth_mm + pitch_bound_mm + m.r0 + cfg.standoff_mm
                    t = t[t * length >= inset]
                if m.embed_end == "p1":
                    inset = cfg.contact_depth_mm + pitch_bound_mm + m.r1 + cfg.standoff_mm
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

    def _surface_pitch_bound_mm(self, surface_tree, cfg):
        """Cache routing.surface_tree_pitch_bound_mm's result, keyed by
        `surface_tree` OBJECT IDENTITY (`is`, not `==`) -- cKDTree
        supports neither attribute assignment nor weakrefs (both measured
        directly: TypeError on both), so the value can't be stashed ON
        the tree itself the way its own docstring's "carry it alongside
        the tree" phrasing might first suggest; caching it here instead,
        on the Forest that's the actual `self` at every real call site in
        this codebase, gets the same effect with zero signature changes
        to any of revalidate's ~10 production call sites (cco.py,
        rewire.py, bracing.py, engine.py all keep calling
        `forest.revalidate(surface_tree, cfg)` verbatim).

        Held as a STRONG reference (self._pitch_bound_cache stores the
        tree object itself, not its id()) specifically to rule out an
        id-reuse hazard: holding the reference keeps the object alive, so
        a later id() collision with a DIFFERENT, unrelated tree object
        cannot occur while this cache entry exists. Correctness over
        performance for the (rare, mostly test-only) case where the same
        Forest is later revalidated against a genuinely different
        surface_tree: the `is` check below recomputes rather than
        silently reusing a stale value from the wrong cloud. In every
        real pipeline run (engine.py builds surface_tree ONCE and reuses
        the SAME object across construct/rewire/brace_and_size), this is
        the fast path -- measured 0.16s for a 400k-point query, paid
        once, not once per candidate trial (revalidate is called from
        cco.py's insertion loop up to tens of thousands of times per
        Treant-scale run; recomputing per-call would have been a real
        regression on top of the wall-clock budget this fix already has
        to mind)."""
        cached = self._pitch_bound_cache
        if cached is not None and cached[0] is surface_tree:
            return cached[1]
        from . import routing
        value = routing.surface_tree_pitch_bound_mm(
            surface_tree, percentile=cfg.surface_pitch_bound_percentile)
        self._pitch_bound_cache = (surface_tree, value)
        return value

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
            roots=list(self.roots), terminal_of=dict(self.terminal_of),
            dead=set(self.dead), braces=list(self.braces)))
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
        self.dead = top["dead"]
        self.braces = top["braces"]

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
