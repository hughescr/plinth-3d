"""Phase-8 ACCEPTANCE HARNESS -- amendment G additions (SPEC-P8-RELOCATE.md
sec 6 items 3, 7, 9, 10). These are the aggregate, run-level gates layered ON
TOP of the existing per-site state-machine guards (SM-STUB/SM-STRUT/SM-BALL,
`phase8.py`) and the existing `verify.self_verify` reporting surface --
nothing here replaces those; each function here consumes their OUTPUT
(a `self_verify` report dict, a `phase8.DefectSnapshot`, a `tree.Forest`) and
adds one more normative check:

  * `final_geometry_verify`      -- sec 6 item 9: reload the *serialized*
                                     forest (pickle round-trip, catches
                                     serialize/round-trip drift the in-memory
                                     pass would miss) and re-run the complete
                                     gate over it, PLUS the two additional
                                     item-9 sub-gates below.
  * `zero_new_firm_transit_gate` -- sec 6 item 9 sub-gate: the output's
                                     firm-transit member set must be a SUBSET
                                     of the frozen relocation-target set.
  * `raft_connectivity_report`   -- sec 6 item 9 sub-gate: every live member
                                     must trace back to a registered root
                                     (the raft) with no dead ancestor.
  * `criticality_weighted_unresolved_gate` -- sec 6 item 7: honest-failure
                                     items bounded BOTH by raw count
                                     (`p8_max_unresolved`) and by
                                     criticality-weighted sum
                                     (`p8_max_unresolved_weight`).
  * `geometry_digest` / `idempotence_gate` -- sec 6 item 10: a second Phase-8
                                     pass over already-fixed geometry must
                                     re-derive an EMPTY frozen defect snapshot
                                     and emit byte-identical geometry.

Chord canonicalization itself (sec 6 item 3, sec 5.3 Layer 3) lives in the
dependency-free `chord_canon` module (imported here, and importable standalone
by an external census-comparison script) -- see that module's docstring for
why it is not defined here.
"""
from __future__ import annotations

import hashlib
import os
import pickle
import tempfile

from . import engine
from .chord_canon import canon_chord_micron, chord_parity_report  # re-exported, see below

__all__ = [
    "canon_chord_micron", "chord_parity_report",
    "final_geometry_verify", "zero_new_firm_transit_gate",
    "raft_connectivity_report", "criticality_weighted_unresolved_gate",
    "geometry_digest", "idempotence_gate",
]


# ---------------------------------------------------------------------------
# sec 6 item 9 -- final-geometry verifier pass over the SERIALIZED output.
# ---------------------------------------------------------------------------
def final_geometry_verify(mesh, pre, forest, unrouted, contacts, carried, tip_r,
                          surface_tree, cfg, geom=None, target_set=frozenset()):
    """Amendment G sec 6 item 9. Round-trips `forest` through `pickle` (a real
    serialize + reload, not just reusing the in-memory object graph -- this is
    what catches drift a purely in-memory re-verify would miss: a field that
    doesn't survive `__getstate__`/`__setstate__`, a stale cached verdict that
    silently rides along in memory but would be absent after a real reload,
    etc.) and re-derives `members`/`balls` + runs the complete
    `verify.self_verify` gate (`engine.finish`) over the RELOADED forest.

    Bundles the two sec-6-item-9 sub-gates named alongside the serialized
    re-verify in the spec text:
      - zero-NEW-firm-transit (`zero_new_firm_transit_gate`) against
        `target_set` (the frozen relocation-target set, e.g.
        `phase8.DefectSnapshot._target_set` / `is_relocation_target`'s
        backing set) -- pass an empty set (the default) to skip this
        sub-gate (e.g. when calling on a forest that predates any Phase-8
        target-set computation).
      - raft-connectivity / orphan check (`raft_connectivity_report`) on the
        RELOADED forest.

    Returns a report dict: `ok` is True iff the reloaded self_verify report is
    ok AND both sub-gates are ok. `verify` carries the full reloaded
    self_verify report (same shape `engine.finish` / `engine.run` return);
    `zero_new_firm_transit` and `raft_connectivity` carry their own sub-reports
    (see those functions)."""
    fd, tmp_path = tempfile.mkstemp(suffix=".p8accept.pkl")
    os.close(fd)
    try:
        with open(tmp_path, "wb") as fh:
            pickle.dump(forest, fh, protocol=pickle.HIGHEST_PROTOCOL)
        with open(tmp_path, "rb") as fh:
            reloaded_forest = pickle.load(fh)
    finally:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)

    result = engine.finish(mesh, pre, reloaded_forest, unrouted, contacts,
                            carried, tip_r, surface_tree, cfg, geom=geom)
    report = result["verify"]

    zero_new = zero_new_firm_transit_gate(report["clearance_violations"], target_set)
    raft = raft_connectivity_report(reloaded_forest)

    ok = bool(report["ok"]) and zero_new["ok"] and raft["ok"]
    return dict(ok=ok, verify=report, zero_new_firm_transit=zero_new,
                raft_connectivity=raft, members=result["members"],
                balls=result["balls"])


def zero_new_firm_transit_gate(clearance_violations, target_set):
    """Amendment G sec 6 item 9 sub-gate: "the set of firm-transit members in
    the output SUBSET-OF the frozen target set -- re-aiming must NEVER
    introduce a firm transit into a member that was legal in `v5e_run1.pkl`."
    `clearance_violations` is `verify.self_verify`'s (or `clearance_report`'s)
    hard-gate list -- each entry's `source` field (added alongside this gate)
    distinguishes "transit" (rule-4/SCAR-AREA-CAP) from "distance" (plain
    swept-capsule collision); only "transit" entries are in scope here (a
    distance collision is a different, already-covered failure class, and
    folding it in would make this gate reject a run for a reason it isn't
    named for). `target_set` is any set/frozenset of member indices (e.g.
    `phase8.DefectSnapshot._target_set`, the union of the frozen non-exempt +
    Q-REAL targets minus the moot 50).

    Returns `dict(ok, new_firm_transit_members)` -- `ok` True iff every firm-
    transit member index is IN `target_set`; `new_firm_transit_members` lists
    the offenders (member indices the rebuild introduced or left behind
    outside the frozen target population)."""
    target_set = frozenset(target_set)
    firm = {v["member_idx"] for v in clearance_violations
            if v.get("source", "transit") == "transit"}
    new_firm = sorted(firm - target_set)
    return dict(ok=len(new_firm) == 0, new_firm_transit_members=new_firm,
                n_firm_transit=len(firm), n_target=len(target_set))


# ---------------------------------------------------------------------------
# sec 6 item 9 -- raft-connectivity / orphan check.
# ---------------------------------------------------------------------------
def raft_connectivity_report(forest):
    """Amendment G sec 6 item 9 sub-gate: "every emitted support member is
    connected to the raft; no subtree was orphaned by a `mark_dead` (SM-STRUT
    S3 / SM-BALL B3). Any orphan fails acceptance."

    Walks EVERY live (non-dead, non-brace) member's `forest.parent` chain.
    `tree.Forest.roots` is the set of member indices with `parent is None`
    (plate/raft-anchored trunk bases by construction, see `tree.Forest.
    set_parent`'s docstring); a live member is connected iff its ancestor
    chain reaches a registered root WITHOUT passing through a `forest.dead`
    member first (the exact bug class `mark_dead` -- used by SM-STRUT S3 and,
    per its own contract, never by SM-BALL B3 which never disposes via
    mark_dead at all, see `phase8.py`'s `_STRUT_CONTRACT` -- must never
    introduce: retiring a member whose children were not first reattached).
    A cycle (a topology bug, not a real forest) is flagged the same way, not
    infinite-looped.

    Braces (`forest.braces`, SPEC-BRACING Part 1 graph edges -- never tree
    topology, never a parent, never a root, see `tree.Forest.add_brace`'s
    docstring) are excluded: they are not part of the parent/children tree at
    all, so a parent-chain walk does not apply to them; their structural
    attachment is to their HOST members (both of which, if live, are walked
    in their own right).

    Returns `dict(ok, orphans)`; `orphans` is a list of
    `{member_idx, reason, at}` (`reason` one of "dead_ancestor", "cycle",
    "unregistered_root"; `at` the member index the walk was standing on when
    the problem was found, `member_idx` the ORIGINAL live member the walk
    started from)."""
    dead = getattr(forest, "dead", set())
    braces = set(getattr(forest, "braces", ()))
    roots = set(getattr(forest, "roots", ()))
    parent = forest.parent
    orphans = []
    for idx in range(len(forest.members)):
        if idx in dead or idx in braces:
            continue
        seen = set()
        cur = idx
        while True:
            if cur in seen:
                orphans.append(dict(member_idx=idx, reason="cycle", at=cur))
                break
            seen.add(cur)
            if cur in dead:
                orphans.append(dict(member_idx=idx, reason="dead_ancestor", at=cur))
                break
            p = parent[cur]
            if p is None:
                if cur not in roots:
                    orphans.append(dict(member_idx=idx, reason="unregistered_root", at=cur))
                break
            cur = p
    return dict(ok=len(orphans) == 0, orphans=orphans)


# ---------------------------------------------------------------------------
# sec 6 item 7 -- criticality-weighted honest-failure bound.
# ---------------------------------------------------------------------------
def _unresolved_weight_items(results, position_lookup=None):
    """Normalizes a mixed iterable of `phase8.StrutResult` / `phase8.
    BallResult` (or any object/dict exposing `.unresolved`/`.criticality`, or
    a caller-pre-filtered list of already-unresolved items lacking that
    attribute entirely, treated as unresolved) into `(item, criticality,
    summary_dict)` triples, keeping only the unresolved ones."""
    out = []
    for r in results:
        is_dict = isinstance(r, dict)
        def _get(key, default=None):
            if is_dict:
                return r.get(key, default)
            return getattr(r, key, default)
        unresolved = _get("unresolved", True)
        if not unresolved:
            continue
        crit = float(_get("criticality", 0.0) or 0.0)
        site_id = _get("site_id", _get("ball_id"))
        rung = _get("rung") or ""
        # StrutResult.rung in {"S1","S2","S3"}, BallResult.rung in
        # {"B1","B2","B3"}; a future SM-STUB R3 honest-failure result would
        # use "R3" -- duck-typed on the rung LETTER so this gate needs no
        # import of phase8's dataclasses (no circular-import risk) and picks
        # up a future STUB result with zero changes here.
        klass = dict(R="STUB", S="STRUT", B="BALL").get(rung[:1], "UNKNOWN")
        summary = dict(
            site_id=site_id, klass=klass, criticality=crit,
            disposition=str(_get("disposition")), rung=_get("rung"),
            trace=tuple(_get("trace", ()) or ()),
            position=(position_lookup.get(site_id) if position_lookup else None),
        )
        out.append((r, crit, summary))
    return out


def criticality_weighted_unresolved_gate(results, cfg, position_lookup=None):
    """Amendment G sec 6 item 7: "items ending at the honest-failure rung
    (SM-STUB R3 / SM-STRUT S3 / SM-BALL B3) <= `p8_max_unresolved` by count,
    AND their criticality-weighted sum <= `p8_max_unresolved_weight` --
    so the bound cannot be satisfied by abandoning many high-criticality sites
    under a raw count." Exceeding EITHER bound fails.

    `results` is any iterable of `phase8.StrutResult`/`phase8.BallResult` (a
    future STUB-ladder result once SM-STUB grows an R3 rung would plug in the
    same way -- this gate is duck-typed on `.unresolved`/`.criticality`, not
    on a specific dataclass) -- typically the `.values()` of
    `run_phase2_struts`/`run_phase3_balls`'s return dicts, pre-filtering to
    `unresolved=True` items is done HERE, so callers may pass the FULL result
    dict values, not just the unresolved subset. `position_lookup`, if given,
    maps `site_id`/`ball_id` -> a position (e.g. `(x, y, z)`) for the sec-6.7
    "reports position" requirement; omitted when the caller has no cheap
    site-position lookup at hand (the count/weight bound itself never depends
    on it).

    Returns `dict(ok, count, weight, max_count, max_weight, items)` --
    `items` is the per-unresolved-item summary list (`site_id`/`ball_id`,
    `klass` in {"STUB","STRUT","BALL","UNKNOWN"}, `criticality`,
    `disposition`, `rung`, `trace`, `position`)."""
    triples = _unresolved_weight_items(results, position_lookup=position_lookup)
    count = len(triples)
    weight = sum(c for _, c, _ in triples)
    max_count = int(cfg.p8_max_unresolved)
    max_weight = float(cfg.p8_max_unresolved_weight)
    ok = count <= max_count and weight <= max_weight
    return dict(ok=ok, count=count, weight=weight, max_count=max_count,
                max_weight=max_weight, items=[s for _, _, s in triples])


# ---------------------------------------------------------------------------
# sec 6 item 10 -- idempotence / second-run no-op.
# ---------------------------------------------------------------------------
def geometry_digest(members, balls) -> str:
    """A canonical, byte-exact digest of the emitted geometry (member p0/p1/
    r0/r1/kind/owner/embed_end, ball center/r), for the sec-6-item-10
    idempotence gate's "byte-identical geometry (same digest)" requirement.
    Every float goes through `.hex()` (exact IEEE-754 round-trip, no decimal
    rounding) exactly like the existing SPEC-V5-PERF bit-identity fingerprint
    (`scripts/tests/_perf_fingerprint.py`) this mirrors -- kept as a small,
    self-contained copy here rather than an import so this module has no
    dependency on `scripts/tests/`."""
    def _f(x):
        return float(x).hex()

    def _pt(a):
        return [float(v).hex() for v in (a.tolist() if hasattr(a, "tolist") else list(a))]

    parts = []
    for m in members:
        p0 = m["p0"] if isinstance(m, dict) else m.p0
        p1 = m["p1"] if isinstance(m, dict) else m.p1
        r0 = m["r0"] if isinstance(m, dict) else m.r0
        r1 = m["r1"] if isinstance(m, dict) else m.r1
        kind = m["kind"] if isinstance(m, dict) else m.kind
        owner = m["owner"] if isinstance(m, dict) else m.owner
        embed_end = (m.get("embed_end") if isinstance(m, dict)
                    else getattr(m, "embed_end", None))
        parts.append(repr((_pt(p0), _pt(p1), _f(r0), _f(r1), kind,
                           list(owner) if isinstance(owner, (list, tuple)) else owner,
                           embed_end)))
    for b in balls:
        c = b["center"] if isinstance(b, dict) else b.center
        r = b["r"] if isinstance(b, dict) else b.r
        parts.append(repr((_pt(c), _f(r))))
    blob = "\n".join(parts).encode("utf-8")
    return hashlib.sha256(blob).hexdigest()


def idempotence_gate(second_pass_snapshot, first_digest: str, second_digest: str):
    """Amendment G sec 6 item 10: "a second Phase-8 pass over the ... output
    re-derives an EMPTY frozen defect snapshot (0 non-exempt, 0 breaches, 0
    REAL-quarantine), enters no ladder, and emits byte-identical geometry
    (same digest). A non-empty second-pass snapshot or any geometry delta
    fails acceptance."

    `second_pass_snapshot` is the `phase8.DefectSnapshot` a second Phase-8 run
    computes over the FIRST run's serialized output. `first_digest` /
    `second_digest` are `geometry_digest(members, balls)` computed before and
    after the second pass -- equal iff the second pass was a true no-op (it
    entered no ladder, so nothing should have moved).

    Returns `dict(ok, empty_snapshot, digest_match, n_non_exempt,
    n_ball_breaches, n_quarantine_real, first_digest, second_digest)`."""
    n_non_exempt = len(second_pass_snapshot.non_exempt)
    n_ball_breaches = len(second_pass_snapshot.ball_breaches)
    n_quarantine_real = len(second_pass_snapshot.quarantine_real)
    empty = (n_non_exempt == 0 and n_ball_breaches == 0 and n_quarantine_real == 0)
    digest_match = (first_digest == second_digest)
    return dict(ok=empty and digest_match, empty_snapshot=empty,
                digest_match=digest_match, n_non_exempt=n_non_exempt,
                n_ball_breaches=n_ball_breaches, n_quarantine_real=n_quarantine_real,
                first_digest=first_digest, second_digest=second_digest)
