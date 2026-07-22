"""Bracing-as-restraint (design-v4.md #9, SPEC-BRACING workstream B).

Braces are NON-TREE Forest members (tree.Forest.add_brace): graph edges,
not topology. Two shapes carry over from v3's retired braces.py (read,
never imported, see that module + scripts/tests/test_braces.py for the
geometry authority this reimplements against tree.Forest):

- Cross-link (preferred): a rod (or a pair, "X-pair") between the member
  needing a restraint (M) and a nearby neighbor member (N), climbing at
  v3.6's hardcoded 45-degree construction (rise == the xy distance
  crossed, ALWAYS -- never the printability angle). Pairs wider than
  cfg.twin_single_brace_xy_mm get a crossed X-pair (both rods discounted
  0.5**0.25 for load-sharing); pairs at or under that spacing get ONE
  rod per rung instead, alternating which member it starts from by rung
  parity, sized WITHOUT the discount (v3.6's twin rule -- a second rod
  between columns millimetres apart is cosmetic waste, not a load-sharing
  partner).
- Model rung (fallback, only when no cross-link candidate clears): a
  short, EXACTLY HORIZONTAL rod from M's axis to a nearby model surface
  point, capped at cfg.model_brace_max_len_mm -- the Chitubox-native
  mini-brace, embed_end="p1" so tree.printability_report's length-capped
  near-horizontal exemption applies (kind "model_rung" only -- the gate
  is not weakened for anything else).

This module only proposes and transactionally places braces for ONE
snapshot of a forest against a caller-supplied spans_report (transient.
transient_report's output shape -- another workstream; tests here hand-
build the dicts, that is the decoupling contract per SPEC-BRACING). No
resizing of host members, no engine wiring, no restraint/span computation
beyond consuming spans_report.

SPEC-ARM-FIX fix A (Phase 4d, design-v4.md #9's literal rule): compute_
needs ALSO triggers on spans_report's worst_span_mm (the activation-aware
moment-arm value, transient.transient_report's own per-member field) via
transient.r_transient_mm -- not just the static free_gaps trigger. A
pass-through chain can have tiny per-member static gaps (no internal
restraint points at all) while its worst_span_mm runs 20mm+ unrestrained
during the actual build (a lagged sibling elsewhere on the path). The
static trigger is blind to that danger class; this one isn't. Only
`transient.r_transient_mm` is imported from that module (a pure function,
no forest/report coupling) -- the decoupling contract above stays intact:
this module still never COMPUTES a span itself, only consumes the two
fields (worst_span_mm, free_gaps) spans_report already carries.

SPEC-PATHRUNG W6 (Phase 4e, "the table unblocker"): the activation
trigger's rung heights are no longer anchored to M's OWN largest free
gap alone -- they are spread across the FULL spans_report[...]
["worst_span_interval"] (transient.transient_report's own field, Phase
4e) and each height is mapped to whichever member on spans_report[...]
["worst_span_path"] physically covers it (transient.py's own docstring:
"root->terminal path... whose z-extents intersect" the winning
interval). This is v3's whole-trunk-ladder rung placement, generalized
to a v4 path: a member whose OWN gap is congested (nothing clears
locally) can still get relief from a rung placed on an UPSTREAM path
member sitting in open air within the SAME offending span -- the exact
member-150 shape this workstream exists to fix (own-gap placement
impossible, upstream placement viable). Still only CONSUMES fields
transient.transient_report already computes -- no span math happens
here, only the height-spread arithmetic (identical n_rungs formula) and
the height->host mapping.

SPEC-151-SCOPING round 2 (the table unblocker, layer 2 -- shared-interval
trigger drop): Pass 2's dedup (below) can find that a JUST-COMPUTED
trigger's own (target, z) height collides with an ALREADY-CLAIMED height
on the same target, genuinely inside its own winning interval (unlike the
round-1/P2-polish case, b7222de, where the colliding claim sat OUTSIDE
the second trigger's own interval and was rightly still attempted). The
pre-round-2 code treated same-target/in-interval collision as proof of
relief and silently DROPPED the second trigger's request -- but a shared
rung height does not shrink two different members' arms by the same
amount (each member's own contribution is `hi - max(z_min(M), R)`, and
z_min(M) differs per member), so "the first trigger's claim relieves the
second" is an assumption, not a fact. THE INVARIANT: a member that
triggers in the current spans_report is by definition not relieved --
dedup may merge PLACEMENTS, it must never skip VERIFICATION. Fix: such a
collision now MERGES (never drops) -- the second trigger's own member_idx
is appended to the existing request's carried-trigger list (sorted,
deterministic), and its own independently-computed candidate positions
(_path_fallback_candidates over ITS OWN path/interval) are folded into
the SAME request's fallback pool, so propose_and_place's stacking loop
(see its own docstring) tries every carried trigger's own path candidates
and re-verifies EVERY carried trigger with the real transient math before
it may stop stacking -- never just the first (dict-order) one.

Backward-compatible degenerate case (recorded design decision): when
`worst_span_path` is absent/empty or has length <= 1 (either a
legacy/hand-built spans_report that never set the new fields -- every
existing test in this file predates W6 and does exactly this -- or a
genuinely single-member path, i.e. no upstream relief exists), this
reduces to a SINGLE merged (member_idx, z_lo, z_hi, n_rungs) request
against M's own largest free gap, bit-identical to the pre-W6 formula
and output shape (so every pre-W6 test keeps its exact pinned
expectations). The generalized (multi-member path) case instead emits
one (host_idx, z, z, 1) request PER individual mapped height -- "an
equivalent" propose_and_place's existing z_lo+(k+1)(z_hi-z_lo)/(n+1)
formula already consumes correctly for n_rungs==1, z_lo==z_hi==z (no
change needed to propose_and_place itself, per spec)."""
import math

import numpy as np

from . import cco, placement, transient, tree

# Half of the connectivity-restraint-dedup tolerance lives in cfg
# (brace_ladder_z_mm/2); see propose_and_place's restraint_zs skip.

# Should-fix (SPEC-PATHRUNG, _find_cross_link_candidates): a cross-link
# rod's length is ALWAYS d*sqrt(2) (rise == d, v3.6's hardcoded
# 45-degree construction) -- floor it at the SAME 1.0mm minimum
# model_rung already enforces on its own rod length (_place_one_rung),
# so two near-coincident member axes can never produce a practically
# zero-length "brace" that gets credited as a real restraint by
# transient.py's activation sweep. d*sqrt(2) >= 1.0  <=>  d >= 1/sqrt(2).
_MIN_CROSS_LINK_XY_MM = 1.0 / math.sqrt(2.0)

# Should-fix (SPEC-PATHRUNG, propose_and_place's "stack more fallback
# rungs"): bound the blast radius of the trial-clearance retry -- at
# most this many REAL braces get stacked for ONE logical rung request
# before accepting whatever relief was achieved and moving on, so an
# unusually stubborn span can't quietly balloon into a dense cluster of
# extra material (and new incidental clearance pressure on UNRELATED
# nearby members) while chasing full relief. Small values cover "the
# ideal split wasn't placeable, a real fallback mostly-but-not-fully
# compensates" without unbounded escalation.
_MAX_STACKED_FALLBACKS = 6

# p6-reach (the pillar fix, config.py's own brace_reach_max_factor_x
# docstring has the full forensics): widened-reach tier MULTIPLIERS tried,
# in ascending order, for an activation-triggered request whose STANDARD
# (cfg.brace_neighbor_max_xy_mm) cross-link search comes back with ZERO
# candidates at a given height. 1.25x first (comfortably covers the one
# measured case -- the pillar's useful neighbor sat at ~1.15x the base
# cap), cfg.brace_reach_max_factor_x (default 1.5x) as the hard ceiling
# tried only if 1.25x ALSO finds nothing. Deliberately just two tiers,
# not a smooth/iterative widen: deterministic, bounded, and matches the
# spec's own "widen ... in tiers" wording.
_REACH_TIER_BASE_FACTOR = 1.25


def _reach_tiers_mm(cfg):
    """Ascending, deduped widened-reach xy caps (absolute mm, already
    scaled by cfg.brace_neighbor_max_xy_mm) to try after the standard cap
    finds nothing -- see _REACH_TIER_BASE_FACTOR and config.py's
    brace_reach_max_factor_x docstring. cfg.brace_reach_max_factor_x is a
    HARD ceiling: the intermediate 1.25x tier is only included when it
    sits AT or under that ceiling (a caller configuring a tighter ceiling
    than 1.25x, e.g. 1.1x, must never see a 1.25x attempt in between --
    that would silently exceed the ceiling they set). Factors <= 1.0 (a
    misconfigured ceiling at or under the base cap) are dropped
    entirely -- they would never find anything the standard search
    didn't already try."""
    base = cfg.brace_neighbor_max_xy_mm
    hard = float(cfg.brace_reach_max_factor_x)
    factors = sorted({f for f in (_REACH_TIER_BASE_FACTOR, hard)
                       if 1.0 + 1e-9 < f <= hard + 1e-9})
    return tuple(base * f for f in factors)


def _n_slender(gap_mm, cfg, lean_deg):
    """Smallest n >= 0 (capped at 50, v3's runaway guard) such that
    splitting `gap_mm` into n+1 equal segments brings the deflection-
    required shaft radius for one segment back under
    cfg.max_unbraced_shaft_r_mm at the member's own lean."""
    for n in range(51):
        seg = gap_mm / (n + 1)
        r = placement._shaft_r_for_L(seg, cfg, lean_deg)
        if r <= cfg.max_unbraced_shaft_r_mm:
            return n
    return 50


def _rungs_that_fit(gap_span, n_rungs):
    """Cap n_rungs so that, laid out evenly across gap_span at n_rungs+1
    equal sub-spacings (propose_and_place's own layout formula:
    z = z_lo + (k+1) * gap_span / (n_rungs+1)), every sub-spacing is
    >= 1mm. gap_span / (n+1) >= 1  <=>  n <= gap_span - 1."""
    if n_rungs <= 0:
        return 0
    max_by_spacing = int(math.floor(gap_span - 1.0 + 1e-9))
    return max(0, min(n_rungs, max_by_spacing))


def compute_needs(forest, spans_report, cfg):
    """For each member and each free gap (z_lo, z_hi) in spans_report,
    decide how many rungs that gap needs (SPEC-BRACING Part 2): the max
    of the slenderness-triggered count (n_slender, deflection budget) and
    the build-time trigger (n_span, v3's brace_free_span_mm cadence) --
    this is the STATIC trigger, blind to activation timing by design.

    SPEC-ARM-FIX fix A (Phase 4d, design-v4.md #9): ALSO trigger on the
    member's own worst_span_mm (S), the activation-aware moment-arm value
    threaded straight from transient_report -- a pass-through member can
    have a tiny static free_gap (no internal restraint points on its OWN
    extent) while S, inherited from a lagged restraint elsewhere on its
    root->terminal path, is large enough that either the resulting
    r_transient shaft radius exceeds cfg.max_unbraced_shaft_r_mm or S
    itself exceeds cfg.brace_free_span_mm. When that fires, request
    rungs (n_rungs = max(n_slender(S, lean), int(S / brace_free_span_mm))
    -- the SAME two-trigger formula as the static path, just fed S
    instead of a gap's own span) in the member's LARGEST free gap (the
    only concrete height band this trigger has to work with -- spans_
    report doesn't say WHERE along the member the moment-arm empirically
    bites), capped by what physically fits in that gap at >= 1mm rung
    spacing (_rungs_that_fit).

    Dedup (never double-request the same height band): when the
    activation trigger's target gap is the SAME (z_lo, z_hi) the static
    trigger already produced a request for on this member, the two are
    MERGED (max of the two counts) into ONE entry, not appended as a
    second request -- propose_and_place's rung-height formula divides a
    gap by (n_rungs+1), so two separate requests against the identical
    band would place overlapping/duplicate rungs at different offsets.

    Path-level dedup (SPEC-151-SCOPING round 2, see module docstring for
    the full invariant): when a generalized path-mapped trigger's own
    computed (target, z) collides (within cfg.brace_ladder_z_mm/2) with
    an existing claim on the SAME target that already falls inside THIS
    trigger's own winning interval, the two triggers are MERGED into ONE
    physical request -- carrying BOTH member indices (sorted, in the
    request's trailing element, a tuple when 2+, a bare int when exactly
    1, `None` for a static/degenerate request -- unchanged shape for
    every pre-round-2 caller) and the UNION of both triggers' own
    candidate positions, never a silent drop. A collision against a
    STATIC (Pass 1) claim, which carries no verifiable trigger of its
    own, is unaffected (still dropped, as before b7222de already
    established -- that claim becomes a real restraint once actually
    placed, and the engine's own fixed-point re-iteration picks up any
    residual need next pass).

    Returns an ordered list of (member_idx, z_lo, z_hi, n_rungs,
    fallbacks, trigger_idx_or_idxs) requests for n_rungs >= 1, sorted by
    (member_idx, z_lo) -- deterministic regardless of spans_report's own
    iteration/dict order."""
    # Pass 1: static per-gap requests (UNCHANGED formula/output shape from
    # pre-W6). Kept in a per-member dict (rather than appended straight to
    # `requests`) so the activation trigger below can MERGE into the same
    # (z_lo, z_hi) key in the degenerate case, exactly as before.
    static_reqs = {}          # member_idx -> {(z_lo, z_hi): n_rungs}
    # member_idx -> [(z, request_ref), ...] already claimed (any trigger).
    # request_ref is None for a Pass-1 static claim (not independently
    # re-verifiable, see module docstring); for a Pass-2 path-mapped claim
    # it is that request's own index into `path_entries` below, so a LATER
    # trigger colliding with it (SPEC-151-SCOPING round 2) can be MERGED
    # into the same live entry instead of silently dropped.
    placed_heights = {}
    for member_idx, info in spans_report.items():
        member = forest.members[member_idx]
        lean = cco._lean_deg(member.p0, member.p1)
        gaps = info.get("free_gaps", [])
        member_reqs = {}
        for gap in gaps:
            z_lo, z_hi = float(gap[0]), float(gap[1])
            span = z_hi - z_lo
            if span <= 0:
                # Design decision (recorded): a degenerate/zero-length
                # gap needs no rung -- defensive, spans_report is another
                # workstream's output.
                continue
            n_slender = _n_slender(span, cfg, lean)
            n_span = int(span / cfg.brace_free_span_mm)
            n_rungs = max(n_slender, n_span)
            if n_rungs >= 1:
                member_reqs[(z_lo, z_hi)] = n_rungs
        static_reqs[member_idx] = member_reqs
        for (z_lo, z_hi), n_rungs in member_reqs.items():
            for k in range(n_rungs):
                z = z_lo + (k + 1) * (z_hi - z_lo) / (n_rungs + 1)
                placed_heights.setdefault(member_idx, []).append((z, None))

    # Pass 2: activation trigger (SPEC-ARM-FIX fix A gate, SPEC-PATHRUNG
    # W6 placement). See module docstring for the degenerate-vs-
    # generalized split.
    # [{target, z, fallbacks, triggers: [member_idx, ...]}, ...] -- one
    # live, MUTABLE entry per generalized-case physical rung request
    # (generalized case only; the degenerate case still merges straight
    # into static_reqs above, unchanged). SPEC-151-SCOPING round 2: a
    # later trigger colliding with (and genuinely inside) an existing
    # entry's own winning interval APPENDS to that entry's own
    # `triggers`/`fallbacks` instead of being dropped -- see module
    # docstring for the invariant.
    path_entries = []
    for member_idx, info in spans_report.items():
        member = forest.members[member_idx]
        lean = cco._lean_deg(member.p0, member.p1)
        gaps = info.get("free_gaps", [])
        worst_span_mm = float(info.get("worst_span_mm", 0.0))
        if worst_span_mm <= 0 or not gaps:
            continue
        n_terminals = info.get("n_terminals", 0)
        r_trans = transient.r_transient_mm(worst_span_mm, lean, n_terminals, cfg)
        if not (r_trans > cfg.max_unbraced_shaft_r_mm or worst_span_mm > cfg.brace_free_span_mm):
            continue
        n_slender_s = _n_slender(worst_span_mm, cfg, lean)
        n_span_s = int(worst_span_mm / cfg.brace_free_span_mm)
        n_rungs_s = max(n_slender_s, n_span_s)
        if n_rungs_s < 1:
            continue

        path = info.get("worst_span_path") or [member_idx]
        interval = info.get("worst_span_interval")
        if len(path) <= 1 or interval is None:
            # Degenerate / legacy shape: reproduce the pre-W6 own-gap
            # targeting EXACTLY (bit-identical formula and output shape --
            # every test predating W6 pins this).
            z_lo, z_hi = max(
                ((float(g[0]), float(g[1])) for g in gaps),
                key=lambda t: t[1] - t[0])
            gap_span = z_hi - z_lo
            n_fit = _rungs_that_fit(gap_span, n_rungs_s)
            if n_fit < 1:
                continue
            key = (z_lo, z_hi)
            member_reqs = static_reqs.setdefault(member_idx, {})
            member_reqs[key] = max(member_reqs.get(key, 0), n_fit)
            continue

        # Generalized path-level distribution (SPEC-PATHRUNG W6): spread
        # n_fit heights evenly across the FULL winning interval (same
        # layout formula as propose_and_place uses per-band), map each to
        # whichever path member covers it, dedupe against every height
        # already claimed (by static OR a previously-mapped activation
        # height) on that TARGET member.
        lo, hi = float(interval[0]), float(interval[1])
        gap_span = hi - lo
        n_fit = _rungs_that_fit(gap_span, n_rungs_s)
        if n_fit < 1:
            continue
        for k in range(n_fit):
            z = lo + (k + 1) * (hi - lo) / (n_fit + 1)
            target = _map_height_to_path_member(forest, path, z)
            if target is None:
                continue  # no path member covers this height -- skip it, honestly
            existing = placed_heights.get(target, [])
            colliding = [(ez, ref) for (ez, ref) in existing
                         if abs(z - ez) <= cfg.brace_ladder_z_mm / 2.0]
            in_range = [(ez, ref) for (ez, ref) in colliding if lo <= ez <= hi]
            if in_range:
                # Geometric relief check (verifier H2), not a bare
                # proximity assumption: a surviving nearby claim on the
                # SAME target only even CANDIDATES as relief for THIS
                # trigger's request if it falls within THIS trigger's own
                # winning interval [lo, hi] -- consistent with
                # transient.py's closed-interval overlap convention (a
                # restraint anywhere inside the offending span caps it,
                # regardless of which trigger's own height arithmetic
                # produced it). A same-trigger repeat always satisfies
                # this (its own prior heights are, by construction,
                # inside its own [lo, hi]), so intra-trigger dedup is
                # unaffected.
                mergeable = [(ez, ref) for (ez, ref) in in_range if ref is not None]
                if mergeable:
                    # SPEC-151-SCOPING round 2 (module docstring, THE
                    # INVARIANT): candidacy for relief is NOT proof of
                    # relief -- the SAME physical rung shrinks two
                    # different members' arms by DIFFERENT amounts (each
                    # member's own contribution is `hi - max(z_min(M),
                    # R)`, and z_min(M) differs per member; table_mesh
                    # members 117/152: identical winning interval,
                    # different z_min anchors, one rung relieves 117 but
                    # not 152). MERGE into the live entry instead of
                    # dropping: carry this trigger for independent
                    # relief-verification (propose_and_place's stacking
                    # loop re-checks EVERY carried trigger, never just the
                    # first), and fold this trigger's OWN independently-
                    # computed candidate positions (its own path, its own
                    # interval -- may differ from the entry's primary
                    # trigger's own path) into the entry's fallback pool,
                    # deterministic, dedup'd, sorted -- "its own
                    # independent request attempt" is thereby already
                    # part of the SAME stacking pass, not a separate one.
                    mergeable.sort(key=lambda t: (t[0], t[1]))
                    _, ridx = mergeable[0]
                    entry = path_entries[ridx]
                    if member_idx not in entry["triggers"]:
                        entry["triggers"].append(member_idx)
                    own_fallbacks = _path_fallback_candidates(forest, path, target, z, lo, hi)
                    entry["fallbacks"] = _merge_fallback_lists(
                        entry["fallbacks"], own_fallbacks, ((z, target),))
                    continue
                # Every in-range colliding claim is a Pass-1 STATIC claim
                # (no verifiable trigger of its own to merge into) --
                # unaffected by round 2, unchanged since b7222de: drop.
                # That claim becomes a real restraint once actually
                # placed; the engine's own brace_and_size fixed point
                # picks up any residual need on its next iteration.
                continue
            # Either no nearby claim exists at all, or the nearby
            # claim(s) sit OUTSIDE this trigger's own winning span (a
            # different trigger's own interval happened to land a claim
            # on the same TARGET member close in raw z, but not actually
            # within the span THIS trigger needs relieved). Dropping on
            # proximity alone was unsound (verifier H2: member117's claim
            # silently swallowing member151's own trigger without ever
            # attempting or relief-checking it) -- still attempt it.
            ridx = len(path_entries)
            placed_heights.setdefault(target, []).append((z, ridx))
            fallbacks = _path_fallback_candidates(forest, path, target, z, lo, hi)
            path_entries.append(dict(target=target, z=z, fallbacks=fallbacks,
                                      triggers=[member_idx]))

    requests = []
    for entry in path_entries:
        triggers = sorted(entry["triggers"])
        # trigger_idx_or_idxs (this request's carried trigger(s) -- see
        # propose_and_place's "stack more rungs" should-fix, SPEC-151-
        # SCOPING round 2) rides along as a 6th element so propose_and_
        # place can re-check WHOSE arm(s) this request exists to relieve,
        # not just place geometry blindly. A bare int when exactly one
        # trigger shares this physical request (every pre-round-2 shape,
        # bit-identical), a sorted tuple when 2+ are merged (new).
        trigger_repr = triggers[0] if len(triggers) == 1 else tuple(triggers)
        requests.append((entry["target"], entry["z"], entry["z"], 1,
                          entry["fallbacks"], trigger_repr))
    for member_idx, member_reqs in static_reqs.items():
        for (z_lo, z_hi), n_rungs in member_reqs.items():
            requests.append((member_idx, z_lo, z_hi, n_rungs, (), None))
    requests.sort(key=lambda r: (r[0], r[1]))
    return requests


def _merge_fallback_lists(*lists):
    """SPEC-151-SCOPING round 2: union (order-preserving-then-resorted,
    duplicate-safe) of any number of (z_alt, host_alt) fallback tuples --
    used to fold a newly-merged trigger's OWN candidate positions into an
    existing request's fallback pool. Re-sorted by the SAME (z_alt,
    host_alt) key _path_fallback_candidates itself already sorts by (that
    function's own docstring: z_ideal does not drive the order, only the
    tuple's own values do), so the merged pool stays in that identical,
    deterministic convention regardless of which trigger's own candidates
    contributed which entry or the order they were merged in."""
    seen = set()
    out = []
    for lst in lists:
        for item in lst:
            if item not in seen:
                seen.add(item)
                out.append(item)
    out.sort(key=lambda t: (t[0], t[1]))
    return tuple(out)


def _path_fallback_candidates(forest, path, target, z_ideal, lo, hi):
    """SPEC-PATHRUNG W6 should-fix (table_mesh forensics): the formula-
    pinned height z_ideal (dead center of the winning interval, for the
    common n_fit==1 case) is the mathematically OPTIMAL single-rung split
    point (equalizes the two resulting segments, matching _n_slender's
    own equal-split assumption) -- but on the real Treant table it can
    land exactly on a densely-congested junction cluster while every
    OTHER member physically present in the same offending span sits in
    genuinely open air a few mm away (the exact "own-gap congested,
    upstream viable" shape this workstream exists to fix, one level
    removed: the WHOLE PATH can be congested only in a narrow sub-band).
    _place_one_rung already searches nearest-first WITHIN one height;
    this is the same idea one level up -- if the ideal height's own
    target member offers nothing, don't give up on the whole path, try
    every OTHER path member's own open middle before calling it honestly
    unplaceable.

    Returns an ordered (tightest-cut-for-the-TRIGGERING-member first,
    then member_idx, then fraction) tuple of (z_alt, host_alt) -- up to
    _FALLBACK_FRACTIONS candidates per OTHER live path member (target
    excluded, already the primary) whose own z-extent intersects the
    winning interval [lo, hi]: each z_alt is a fixed fraction of that
    member's own (extent ∩ [lo, hi]) sub-window. Bounded (at most
    len(path) - 1 times len(_FALLBACK_FRACTIONS) entries) and entirely
    deterministic (a pure function of the path, the winning interval,
    and the primary target/height -- no placement-time state).

    Should-fix (table_mesh forensics, more than one sample per member):
    a single member's own real clearance envelope is NOT uniform across
    its length (a nearby cross-link neighbor's own z-extent only
    partially overlaps it) -- the exact MIDPOINT can be blocked while a
    position nearer one end of the SAME member is wide open (confirmed:
    the table's real 64/61 pair clears only near 64's own TOP and 61's
    own BOTTOM, not either member's center). One candidate per member
    is not enough to reliably find that; multiple fixed-fraction samples
    are.

    Ordering (recorded design decision, table_mesh forensics): NOT
    nearest-to-z_ideal. The triggering member M's own worst_span
    contribution is `hi_final - max(z_min(M), R)` with z_min(M) FIXED --
    so whichever live candidate actually gets built, the smaller its
    OWN z, the smaller M's resulting arm, full stop (z_ideal's exact
    midpoint is merely the OPTIMAL split IF it were placeable at all --
    once it isn't, the split is no longer perfectly balanced regardless
    of which fallback is chosen, so the tiebreak that actually serves
    M -- the member whose gate fired -- is "cut as close to M's own top
    as the real geometry allows", not "closest to the abstract ideal
    point"). z_ideal is still threaded through purely to keep the
    signature/call-site stable; it no longer drives the sort.

    Should-fix (table_mesh forensics, member 104's own shape): TARGET
    itself is now ALSO sampled at every other fraction (only its exact
    z_ideal formula height is guaranteed tried already, as the primary,
    before this list is ever consulted -- excluding it entirely from its
    OWN fallback set threw away every OTHER position along what can be
    the single BEST (longest, most open) member on the whole path, just
    because the one formula-picked height on it happened to land on a
    locally-congested spot (e.g. dead on a neighboring member's own
    boundary). A real repro: member 104's own path is [149, 104, 105];
    z_ideal (the FULL interval's midpoint) maps onto 105 (an 8mm+-long,
    otherwise wide-open member) at a height sitting RIGHT on a
    neighbor's boundary and failing there, while positions ~1mm further
    into 105's own body clear cleanly -- 149 (a sub-mm sliver) offers
    nothing, so without re-sampling 105 itself this request had no real
    fallback at all."""
    out = []
    for p in path:
        pz0, pz1 = _z_extent(forest.members[p])
        ilo, ihi = max(pz0, lo), min(pz1, hi)
        if ihi <= ilo:
            continue  # this path member doesn't reach into the winning interval at all
        span = ihi - ilo
        for frac in _FALLBACK_FRACTIONS:
            z_alt = ilo + frac * span
            if p == target and abs(z_alt - z_ideal) <= 1e-9:
                continue  # that exact point IS the primary, already tried first
            out.append((z_alt, p))
    out.sort(key=lambda t: (t[0], t[1]))
    return tuple(out)


def _trial_resize_r(forest, member, member_idx, info, lean, cfg):
    """SPEC-PATHRUNG W6 should-fix (propose_and_place's trial-clearance
    lookahead): the SAME r_req = max(r_murray, r_tension, r_deflect,
    r_transient, kind_floor) formula transient.resize_forest itself uses
    (design-v4.md #6) -- re-derived here (read-only, no mutation) so the
    lookahead can't under-estimate the member's REAL final radius by
    checking only r_transient. r_deflect's own worst-sub-gap term can be
    the larger of the two whenever this specific member ISN'T the one
    whose bracing shrank its own free_gaps (the common case bracing
    exists to shrink is r_deflect < r_transient; this covers the
    opposite)."""
    children = [c for c in forest.children[member_idx] if c not in forest.dead]
    r_murray = (sum(forest.members[c].r1 ** 4 for c in children)) ** 0.25 if children else 0.0
    r_tension = cco._r_tension_mm(member.carried_mm2, cfg)
    worst_gap_mm = max(hi - lo for lo, hi in info["free_gaps"])
    own_length_mm = float(np.linalg.norm(
        np.asarray(member.p1, dtype=float) - np.asarray(member.p0, dtype=float)))
    z_extent_mm = max(abs(float(member.p0[2]) - float(member.p1[2])), 1e-9)
    worst_gap_3d_mm = worst_gap_mm * (own_length_mm / z_extent_mm)
    r_deflect = placement._shaft_r_for_L(worst_gap_3d_mm, cfg, lean)
    r_trans = transient.r_transient_mm(info["worst_span_mm"], lean, info["n_terminals"], cfg)
    kind_floor = cfg.min_trunk_r_mm if member.kind == "trunk" else cfg.min_shoot_r_mm
    return max(r_murray, r_tension, r_deflect, r_trans, kind_floor)


# Should-fix (SPEC-PATHRUNG, _path_fallback_candidates): fixed, ordered
# sample fractions within each candidate path member's own (extent ∩
# winning-interval) sub-window -- the midpoint alone can sit exactly on
# a locally-congested spot while a position nearer one end of the SAME
# member is wide open (a real cross-link neighbor's own z-extent often
# only partially overlaps the candidate member). Deterministic, bounded.
_FALLBACK_FRACTIONS = (0.5, 0.25, 0.75, 0.1, 0.9)


def _map_height_to_path_member(forest, path, z):
    """SPEC-PATHRUNG W6: which path member (root-side ordered list of
    live member indices) physically covers height z -- closed-interval
    containment on each member's own z-extent. Several covering the same
    z (a junction boundary, shared by parent and child) -> the LOWEST
    member index wins (deterministic, arbitrary tiebreak matching the
    spec's literal wording); none -> None (skip that height, honestly)."""
    candidates = [idx for idx in path
                  if _z_extent(forest.members[idx])[0] - 1e-9 <= z
                  <= _z_extent(forest.members[idx])[1] + 1e-9]
    return min(candidates) if candidates else None


# ---------------------------------------------------------------- geometry --
def _z_extent(m):
    z0, z1 = float(m.p0[2]), float(m.p1[2])
    return (z0, z1) if z0 <= z1 else (z1, z0)


def _axis_xy_at_z(m, z):
    p0, p1 = m.p0, m.p1
    dz = float(p1[2] - p0[2])
    if abs(dz) < 1e-9:
        return np.array([p0[0], p0[1]], dtype=float)
    t = (z - p0[2]) / dz
    return p0[:2] + t * (p1[:2] - p0[:2])


def _axis_point_at_z(m, z):
    xy = _axis_xy_at_z(m, z)
    return np.array([xy[0], xy[1], z], dtype=float)


def _live_neighbor_indices(forest):
    braces_set = set(forest.braces)
    return [i for i, m in enumerate(forest.members)
            if i not in forest.dead and i not in braces_set and m.kind != "stub"]


def _find_cross_link_candidates(forest, mi, z, cfg, k, max_xy_mm=None):
    """Neighbor candidates for member `mi` at rung height z (SPEC-BRACING
    rule 1): live non-brace non-stub members N != M whose z-extent covers
    [z, z+rise] (rise = d, the xy distance between the axes at z -- v3.6's
    hardcoded 45-degree construction), sorted by (d, idx). `k` is the
    rung's own index within its request (SAME meaning as
    _cross_link_rods' k, which parity picks the rod direction) --
    required, not defaulted, because which of M's endpoints a rod
    actually reads depends on it (see the should-fix note below).

    p6-reach: `max_xy_mm` overrides cfg.brace_neighbor_max_xy_mm as the
    upper distance cap when given (None, the default, preserves the
    EXACT prior behavior/signature for every existing caller) -- lets
    _place_one_rung retry a WIDENED search (see _reach_tiers_mm) without
    duplicating this function's own candidate-scan/rise-gate logic.

    Fix (verifier blocker #1, floating brace endpoint): rule 1's
    endpoints are "ON THE AXES (centerline to centerline)" for BOTH
    members -- the X-pair's mirrored rod and the twin rule's odd-parity
    rung both read M's own axis AT z+rise (_cross_link_rods), and
    _axis_xy_at_z is an unclamped linear parametrization that silently
    extrapolates past a member's real endpoint rather than failing. A
    candidate whose rise would put z+rise beyond M's own physical
    z-extent is therefore not "on M's axis" at all -- it must be
    rejected here, exactly like N's own z-extent is already rejected
    below, or the rod gets certified while floating disconnected from
    M's real geometry (the 'stale-brace-leak' class connectivity_report
    exists to catch).

    Should-fix (verifier round 2, twin k-even over-rejection): the fix
    above made M's [z, z+rise] coverage unconditional, but a rod only
    ever LANDS on M at z+rise for an X-pair (rod2 mirrors onto M) or a
    twin ODD-parity rung (`k` odd, N -> M climbing) -- see
    _cross_link_rods. An X-pair's OTHER rod (rod1) and a twin
    EVEN-parity rung both read M at z (its own real point, never
    extrapolated) and only read N at z+rise; requiring M to also cover
    z+rise in that case over-rejects a perfectly valid candidate.
    X-pairs are unaffected (both rods always required, so M needs the
    full range regardless of k) -- N's own [z, z+rise] requirement below
    stays unconditional in every case, unchanged.

    Should-fix (SPEC-PATHRUNG, table_mesh forensics -- pre-existing
    latent bug, never before exercised since zero braces ever placed on
    table_mesh pre-W6): the lower bound on `d` was `1e-6` -- "nonzero",
    not "physically meaningful". Two near-COINCIDENT members (real
    table_mesh case: two shoot members ~3 MICRONS apart in xy, an
    upstream bifurcation-split near-duplicate) satisfy that trivially,
    producing a rod of length d*sqrt(2) ~= 4e-6mm -- a zero-length brace
    in every practical sense (model_rung already floors its OWN rod
    length at 1.0mm, exactly to rule out this class -- cross-link never
    had the equivalent floor). Such a rod passes clearance/revalidate
    for free (no real geometry to intersect) but STILL gets recorded by
    derive_brace_attachments and, via _static_restraint_zs, is credited
    as a genuine restraint at that height for every future transient_
    report/resize_forest call -- inflating BOTH neighbors' apparent
    bracing (shrinking their r_deflect/r_transient floors) for restraint
    that does not mechanically exist. Caught by
    test_engine_every_member_meets_its_own_load_and_deflection_floor
    [table_mesh] (test_cco.py, not this module's ownership, but the
    defect is squarely bracing's to fix): two real shoot members came
    back undersized relative to their own honest per-segment floor,
    traced to exactly this degenerate pair. Fixed by requiring the
    resulting ROD length (d*sqrt(2), the same v3.6 45-degree geometry
    _cross_link_rods always uses) to clear the SAME 1.0mm floor
    model_rung already enforces -- d >= 1.0/sqrt(2) -- rather than merely
    excluding the single ill-defined d==0 point."""
    M = forest.members[mi]
    z0_M, z1_M = _z_extent(M)
    if not (z0_M - 1e-9 <= z <= z1_M + 1e-9):
        # M's own axis doesn't even reach the requested rung height --
        # no candidate at this z can ever be valid.
        return []
    cap = cfg.brace_neighbor_max_xy_mm if max_xy_mm is None else float(max_xy_mm)
    p_M = _axis_point_at_z(M, z)
    cands = []
    for ni in _live_neighbor_indices(forest):
        if ni == mi:
            continue
        N = forest.members[ni]
        z0, z1 = _z_extent(N)
        if not (z0 - 1e-9 <= z <= z1 + 1e-9):
            continue
        p_N = _axis_point_at_z(N, z)
        d = float(np.linalg.norm(p_N[:2] - p_M[:2]))
        # Should-fix (SPEC-PATHRUNG): d's lower bound is a REAL rod-length
        # floor (rise == d always, rod_len == d*sqrt(2)), not merely
        # "nonzero" -- see this function's own docstring.
        if not (_MIN_CROSS_LINK_XY_MM < d <= cap):
            continue
        rise = d
        if not (z0 - 1e-9 <= z + rise <= z1 + 1e-9):
            continue
        # M's own [z, z+rise] coverage is only required when a rod
        # actually lands ON M at z+rise: always true for an X-pair
        # (d > twin_single_brace_xy_mm, both rods placed -- rod2 mirrors
        # onto M), and true for a twin (d <= threshold) ONLY on an odd
        # (N -> M climbing) rung. A twin even rung never reads M above z.
        is_twin = d <= cfg.twin_single_brace_xy_mm
        m_needs_full_extent = (not is_twin) or (k % 2 != 0)
        if m_needs_full_extent and not (z0_M - 1e-9 <= z + rise <= z1_M + 1e-9):
            continue
        cands.append((d, ni))
    cands.sort(key=lambda t: (t[0], t[1]))
    return cands


def _cross_link_rods(forest, mi, ni, z, d, k, cfg):
    """Rod geometry for one candidate (SPEC-BRACING rule 1): X-pair for
    d > cfg.twin_single_brace_xy_mm (both rods discounted), else a single
    alternating rod (twin rule, undiscounted). rise = d always.

    ORCHESTRATOR RULING (Phase 4 floor-fix, design-v4.md #9's
    load-sharing discount vs #6's per-member deflection floor): the
    charter #9 X-pair discount (0.5**0.25) legitimately shares the
    LATERAL-RESTRAINT duty between the two rods -- but each rod's own
    r_deflect floor encodes ITS OWN survivability under print peel
    forces, which pairing cannot share (each rod is peeled
    individually). The discount may therefore never push a rod below
    its own floor. That floor must be keyed off the rod's REAL final
    endpoints (length AND lean), not the d*sqrt(2)/max_segment_angle_deg
    45-degree-construction approximation r_raw uses -- on a LEANING
    host, a rod's true endpoint-to-endpoint geometry drifts from that
    approximation (verified: measured table_mesh brace members 152/153,
    two mirrored rods of the SAME X-pair, own floors 0.6765mm and
    0.6508mm -- different, because the two hosts' own leans differ),
    exactly the same real-geometry-over-construction-estimate principle
    _try_place_rods already applies for the printability cone check
    (this function's own module docstring, SPEC-PATHRUNG W1). Twin
    single rods carry no discount and (measured) already clear this same
    per-rod floor under the fixed r_raw approximation in practice, so
    they are left as-is."""
    M, N = forest.members[mi], forest.members[ni]
    rise = d
    p_M_z, p_N_z = _axis_point_at_z(M, z), _axis_point_at_z(N, z)
    p_M_hi, p_N_hi = _axis_point_at_z(M, z + rise), _axis_point_at_z(N, z + rise)
    rod_len = d * np.sqrt(2.0)
    r_raw = placement._shaft_r_for_L(rod_len, cfg, cfg.max_segment_angle_deg)
    if d <= cfg.twin_single_brace_xy_mm:
        r = max(cfg.brace_r_mm, r_raw)
        if k % 2 == 0:
            return [(p_M_z, p_N_hi, r)]      # even: M -> N climbing
        return [(p_N_z, p_M_hi, r)]          # odd: N -> M climbing
    duty_r = r_raw * 0.5 ** 0.25
    rods = []
    for p0, p1 in ((p_M_z, p_N_hi), (p_N_z, p_M_hi)):
        own_len = float(np.linalg.norm(np.asarray(p1, float) - np.asarray(p0, float)))
        own_lean = cco._lean_deg(p0, p1)
        own_floor = placement._shaft_r_for_L(own_len, cfg, own_lean)
        rods.append((p0, p1, max(cfg.brace_r_mm, own_floor, duty_r)))
    return rods


def _find_model_rung_anchors(forest, surface_tree, mi, z, cfg, limit=6):
    """Rule 2 fallback candidates: surface points within cfg.model_brace_
    max_len_mm of M's axis at z, with |anchor_z - z| <= 0.75, sorted by
    (distance, index) -- nearest first, up to `limit` (v3's cadence,
    braces.py's ground_points[mask][argsort][:6]). Returns a list of
    (p0, p1) with p0 = M's axis at the ANCHOR's z (so the rod is exactly
    horizontal by construction) and p1 = the anchor; empty if nothing
    qualifies.

    Should-fix (verifier round 2, model-rung retry): the original
    single-candidate version gave up entirely if the ONE nearest anchor
    failed the caller's length floor or clearance, even when a perfectly
    good next-nearest anchor was right there. Returning the whole
    ordered shortlist lets the caller retry deterministically instead."""
    if surface_tree is None:
        return []
    M = forest.members[mi]
    p_M_z = _axis_point_at_z(M, z)
    pts = np.asarray(surface_tree.data, dtype=float)
    if pts.size == 0:
        return []
    dist = np.linalg.norm(pts - p_M_z[None, :], axis=1)
    anchor_z = pts[:, 2]
    mask = (dist <= cfg.model_brace_max_len_mm + 1e-9) & (np.abs(anchor_z - z) <= 0.75 + 1e-9)
    idxs = np.nonzero(mask)[0]
    if len(idxs) == 0:
        return []
    order = idxs[np.argsort(dist[idxs], kind="stable")]
    out = []
    for i in order[:limit]:
        anchor = pts[int(i)]
        p0 = _axis_point_at_z(M, float(anchor[2]))
        p1 = np.array(anchor, dtype=float)
        out.append((p0, p1))
    return out


# --------------------------------------------------------------- placement --
def _try_place_rods(forest, surface_tree, cfg, rods, kind, embed_end, counters):
    """Transactional placement (SPEC-BRACING rule 3): checkpoint, add all
    rods of this ONE candidate (both X-pair rods together, atomically),
    revalidate, rollback on any violation (returning the kind's owner
    counter to where it was so placed members keep a contiguous sequence
    number), else commit. Returns the new member indices or None.

    SPEC-PATHRUNG W1 (Treant build:200, brace rod at 69.8deg vs limit
    60): Forest.revalidate checks clearance/connectivity/murray only --
    nothing checked a brace's OWN printability until the FINAL gate,
    which is too late to reject-and-try-next. On a LEANING host, rise=d
    is evaluated at height z (v3.6's hardcoded 45-degree construction),
    but the rod's actual endpoint-to-endpoint drift is measured on the
    REAL geometry -- a leaning host's axis at z+rise sits somewhere other
    than "straight up d from the z point", so the rod's true cone angle
    can exceed theta_max(r) even though its CONSTRUCTION was textbook
    45-degrees. Design freedom (recorded): single-pass reject, not an
    iterative rise recompute -- after the ACTUAL endpoints are known
    (rods already carries them), run tree.printability_report -- the
    SAME per-segment rule (drift+taper <= tan(theta_max(r)), model_rung's
    horizontal exemption intact since it is literally the same function
    the final gate uses) -- over just the newly-added members (not the
    whole forest; the gate itself is untouched) and reject exactly like a
    clearance failure would (rollback, return None, caller tries the
    next candidate nearest-first) -- deterministic, no RNG, no retry
    loop needed since the reject is authoritative on the REAL geometry.

    SPEC-151-SCOPING fix (the table unblocker, dirty-member self-
    poisoning): forest.revalidate is ONE BATCHED re-check over EVERY
    currently-dirty member, not scoped to the just-added candidate. A
    member left dirty by an earlier stage (e.g. resize_forest growing it
    past its certified radius) that ALSO genuinely violates clearance
    stays in that dirty set on every subsequent revalidate() call in this
    same brace_and_size pass -- its own persistent violation rides along
    in the return value regardless of what candidate is being judged, so
    `if violations: reject` would kill a candidate for a defect that
    predates it and has nothing to do with its own geometry (measured:
    table_mesh member 151's own pre-existing debt false-rejected 10 of 26
    candidates that would otherwise have relieved it). Fix: snapshot the
    dirty set BEFORE adding this candidate's rod member(s), then treat as
    grounds for rejection ONLY violations whose member_idx is NOT in that
    pre-add snapshot.

    Safety argument (independently verified against add_brace/
    Forest.add_brace and Member.__setattr__ in tree.py): add_brace only
    ever APPENDS a new Member (fresh geom_version=0/cert_version=-1, its
    OWN dirty entry) to forest.members/braces -- it never assigns to any
    p0/p1/r0/r1 of an EXISTING member, and nothing else runs between
    checkpoint() and this revalidate() call. So no pre-existing member's
    geom_version can change here, meaning no pre-existing member can
    newly enter the dirty set as a result of adding this candidate.
    Therefore every member_idx in the pre-add snapshot that still appears
    in revalidate()'s violations is reporting the SAME pre-existing
    defect, not a new one this candidate caused -- filtering those out
    can never mask a genuinely new problem. Every violation on an index
    NOT in the snapshot is a real, unmasked rejection. revalidate()'s
    certificate side effects (re-issuing certs to clean dirty members)
    are untouched; only the accept/reject DECISION here is rescoped."""
    cp = forest.checkpoint()
    pre_dirty = set(forest.dirty_members())
    idxs = []
    for p0, p1, r in rods:
        owner = (kind, counters[kind])
        counters[kind] += 1
        m = tree.Member(p0=p0, p1=p1, r0=r, r1=r, kind=kind, owner=owner,
                         embed_end=embed_end)
        idxs.append(forest.add_brace(m))
    violations = forest.revalidate(surface_tree, cfg)
    violations = [v for v in violations if v["member_idx"] not in pre_dirty]
    if not violations:
        violations = tree.printability_report([forest.members[i] for i in idxs], cfg)
    if violations:
        forest.rollback(cp)
        counters[kind] -= len(rods)
        return None
    forest.commit(cp)
    return idxs


def _place_one_rung(forest, surface_tree, cfg, mi, z, k, counters, reach_tiers=None):
    """One rung request at height z on member mi: try cross-link
    candidates nearest-first, then the model-rung fallback -- itself
    retried nearest-first (should-fix, verifier round 2: up to
    cfg-implied v3 cadence of 6 candidates, deterministic (distance,
    index) order) rather than giving up after the single nearest anchor.
    Returns a list of (member_idx, kind) placed, or None if
    unplaceable.

    p6-reach (the pillar fix): `reach_tiers` (an ascending tuple of
    widened xy caps, see _reach_tiers_mm) is None by default -- every
    existing caller/test is bit-identical. When given AND the standard-
    cap search above found ZERO candidates at this exact height, retry
    with each tier's wider cap in turn (nearest-first within a tier,
    smallest tier first), skipping any neighbor index already tried by a
    smaller tier (a larger tier's candidate list is always a superset of
    a smaller one's, so nothing is retried twice). Only ever reached when
    the standard search was EMPTY -- a height where standard-cap
    candidates already exist never even evaluates `reach_tiers`, so this
    can only ADD placements the base mechanism could never have found,
    never change one that already worked."""
    std_candidates = _find_cross_link_candidates(forest, mi, z, cfg, k)
    for d, ni in std_candidates:
        rods = _cross_link_rods(forest, mi, ni, z, d, k, cfg)
        placed = _try_place_rods(forest, surface_tree, cfg, rods, "brace", None, counters)
        if placed is not None:
            return [(idx, "brace") for idx in placed]

    if not std_candidates and reach_tiers:
        tried_ni = set()
        for tier_cap in reach_tiers:
            wide_candidates = _find_cross_link_candidates(forest, mi, z, cfg, k,
                                                            max_xy_mm=tier_cap)
            for d, ni in wide_candidates:
                if ni in tried_ni:
                    continue
                tried_ni.add(ni)
                rods = _cross_link_rods(forest, mi, ni, z, d, k, cfg)
                placed = _try_place_rods(forest, surface_tree, cfg, rods, "brace", None, counters)
                if placed is not None:
                    return [(idx, "brace") for idx in placed]

    for p0, p1 in _find_model_rung_anchors(forest, surface_tree, mi, z, cfg):
        length = float(np.linalg.norm(p1 - p0))
        if not (1.0 - 1e-9 <= length <= cfg.model_brace_max_len_mm + 1e-9):
            continue
        placed = _try_place_rods(forest, surface_tree, cfg,
                                  [(p0, p1, cfg.brace_r_mm)], "model_rung", "p1", counters)
        if placed is not None:
            return [(idx, "model_rung") for idx in placed]
    return None


def _trigger_relieved(forest, surface_tree, cfg, fresh_report, trigger_idx):
    """SPEC-151-SCOPING round 2: is `trigger_idx` genuinely relieved right
    now, per the SAME real transient math (r_transient_mm / _trial_
    resize_r / a trial-revalidate probe) propose_and_place's stacking
    loop has always used for its single-trigger relief check -- extracted
    verbatim so a merged (shared-interval) request can re-verify EVERY
    carried trigger, not just one, after each stacked placement.
    `fresh_report` is ONE transient.transient_report(...) call the caller
    shares across every trigger checked at this stacking step (not
    recomputed per trigger -- the forest doesn't change between them).

    `trigger_idx` absent from `fresh_report` (no longer a live/eligible
    member) counts as relieved -- nothing left to relieve, matching the
    pre-round-2 single-trigger code's own `if t_info is None: break`."""
    t_info = fresh_report.get(trigger_idx)
    if t_info is None:
        return True
    t_member = forest.members[trigger_idx]
    t_lean = cco._lean_deg(t_member.p0, t_member.p1)
    t_span = t_info["worst_span_mm"]
    t_r = transient.r_transient_mm(t_span, t_lean, t_info["n_terminals"], cfg)
    # SPEC-PATHRUNG W6 should-fix (table_mesh forensics, trial-clearance
    # not just the deflection threshold): the (r_transient > max_
    # unbraced_shaft_r_mm) gate is compute_needs's OWN "does this need
    # bracing AT ALL" trigger -- by definition it is satisfied the instant
    # a SINGLE (mathematically ideal, equal-split) rung would suffice,
    # which says nothing about whether THIS REAL, geometry-constrained
    # split (unbalanced by whatever candidate actually cleared) also
    # clears LOCAL clearance at the RESIZE_FOREST-FINAL radius --
    # r_transient alone understates it whenever r_deflect's own worst-
    # sub-gap term (design-v4.md #6, transient.resize_forest's SAME
    # max(...) formula) is the larger of the two for this specific member
    # (unlike the common case where bracing shrinks r_deflect well below
    # r_transient). Trial the FULL resize_forest formula, not just its
    # r_transient term, so this lookahead can't declare victory on a
    # radius resize_forest itself would never actually pick.
    t_r_req = _trial_resize_r(forest, t_member, trigger_idx, t_info, t_lean, cfg)
    relieved = t_span <= cfg.brace_free_span_mm and t_r <= cfg.max_unbraced_shaft_r_mm
    if relieved:
        trial_cp = forest.checkpoint()
        if abs(t_member.r0 - t_r_req) > 1e-9:
            t_member.r0 = t_r_req
        if abs(t_member.r1 - t_r_req) > 1e-9:
            t_member.r1 = t_r_req
        trial_violations = forest.revalidate(surface_tree, cfg)
        forest.rollback(trial_cp)
        relieved = not any(v["member_idx"] == trigger_idx for v in trial_violations)
    return relieved


def _clearance_violated_members(forest, surface_tree, cfg):
    """p6-reach Case 2 fix ("more restraint next iteration", engine.
    brace_and_size's own docstring): live, non-brace, non-stub member
    indices with a REAL clearance violation RIGHT NOW -- the exact SAME
    check (forest.revalidate against surface_tree/cfg) engine.
    brace_and_size's own loop already performs after transient.
    resize_forest, just read one call earlier (propose_and_place runs
    BEFORE resize_forest each brace_and_size iteration, so this sees
    whatever the PRIOR iteration's resize already grew into a violation
    -- the mechanism that actually needs another iteration to close).
    Deduped (a member can appear in more than one violation entry) and
    sorted ascending by member_idx for determinism -- forest.revalidate's
    own return order is not a documented contract."""
    violations = forest.revalidate(surface_tree, cfg)
    brace_set = set(getattr(forest, "braces", []))
    out = set()
    for v in violations:
        idx = v["member_idx"]
        if idx in forest.dead or idx in brace_set:
            continue
        if forest.members[idx].kind == "stub":
            continue
        out.add(idx)
    return sorted(out)


def propose_and_place(forest, surface_tree, plate_z, cfg, spans_report, log=None):
    """Places braces transactionally against ONE snapshot of spans_report
    (SPEC-BRACING Part 2). `plate_z` is accepted per the interface
    contract (engine wiring depends on this exact signature); bracing
    never roots a member at the plate, only cross-links or model rungs
    (plate-relative placement decisions belong to cco.py) -- BUT
    SPEC-PATHRUNG W6's "stack more fallback rungs" should-fix (below)
    threads it straight into a mid-call transient.transient_report
    re-check, so it is no longer decoratively unused.

    Returns dict(placed=[brace idx...], rungs_requested, rungs_placed,
    rungs_unplaceable, by_kind={"brace": n, "model_rung": m}).

    Design decision (recorded): a rung height skipped because an
    existing restraint (spans_report[...]["restraint_zs"]) already sits
    within cfg.brace_ladder_z_mm/2 of it is neither "placed" nor
    "unplaceable" -- it wasn't attempted because it wasn't needed, which
    is a different outcome than "tried and couldn't clear". It still
    counts toward rungs_requested (the raw ask from compute_needs, before
    any placement-time skip/failure).

    SPEC-PATHRUNG W6 scoping (recorded design decision, table_mesh
    forensics): the restraint_zs skip above stays EXACTLY as it was for
    every static/degenerate request (z_lo != z_hi, host == the member
    that owns the gap) -- unchanged, per spec's "keep the existing
    near-existing-restraint skip". It does NOT apply to a generalized
    path-mapped single-height rung (compute_needs's path_requests,
    identifiable by z_lo == z_hi -- no static/degenerate request is ever
    a zero-length band, span > 0 is enforced at construction). Reason:
    spans_report[host]["restraint_zs"] is HOST's own STRUCTURAL branch-
    point presence (transient._static_restraint_zs: p0 when host itself
    forks, p1 when host's parent forks) -- topologically real but
    ACTIVATION-LAGGED, which is exactly the defect class this workstream
    exists to fix (member 150's own two nominal restraints at 8.4/11.1
    did not prevent its 22.28mm worst_span_mm in the first place). On a
    densely-branched forest a path member's entire short extent can sit
    within brace_ladder_z_mm/2 of BOTH its own structural boundaries,
    which would blanket-reject every path-mapped candidate and leave the
    triggering member's own arm honestly uncuttable even though open-air
    placement is geometrically viable -- the exact member-150 shape.
    compute_needs's OWN placed_heights dedup (never two requests within
    brace_ladder_z_mm/2 of the same height on the SAME host, across every
    trigger) still fully applies and is the correct redundancy guard for
    this request class; an ALREADY-PLACED brace from a prior
    brace_and_size iteration is instead caught structurally -- once
    placed it becomes part of derive_brace_attachments and the next
    transient_report call reflects the now-earlier-activating restraint,
    shrinking worst_span_mm (and usually clearing the trigger) rather
    than needing a second, separately-tracked dedup source here.

    SPEC-PATHRUNG W6 should-fix (table_mesh forensics, _path_fallback_
    candidates): when a generalized path-mapped request's PRIMARY (ideal,
    formula-computed) height fails placement outright, its request tuple
    carries a bounded, deterministic fallback list (compute_needs's own
    output, not recomputed here) of (z_alt, host_alt) candidates on every
    OTHER path member physically present in the same winning interval --
    tried nearest-to-ideal first, same "honest unplaceable" outcome only
    once every one of them has ALSO failed. A local `tried_zs` tracker
    (host -> heights attempted THIS call, primary and fallback alike)
    keeps a fallback from ever landing within brace_ladder_z_mm/2 of
    anything already attempted on that host in this same pass -- static/
    degenerate requests carry no fallbacks (empty tuple) and are
    completely unaffected, bit-identical to pre-W6 behavior.

    SPEC-PATHRUNG W6 should-fix (table_mesh forensics, "stack more
    fallback rungs"): a geometrically-successful placement is not always
    a SUFFICIENT one -- a real cross-link's mandatory 45-degree rise
    (rise == the xy distance crossed, ALWAYS, v3.6's construction) can
    put its actual activation height well above the z it was requested
    at, leaving the triggering member's arm still over budget even
    though a rung genuinely got built. For a path-mapped request, after
    each successful placement (primary or fallback) this re-checks
    spans_report's OWN triggering member (trigger_idx, compute_needs's
    6th tuple element -- NOT `host`, which may be a different path
    member entirely) against the identical two-part gate compute_needs
    used to decide bracing was needed (r_transient_mm > cfg.
    max_unbraced_shaft_r_mm or worst_span_mm > cfg.brace_free_span_mm),
    via a fresh transient.transient_report call. Relieved -> stop.
    Still over budget -> keep stacking the NEXT fallback (a genuinely
    different, real restraint further along the SAME offending span)
    instead of declaring victory on geometry-only success. Static/
    degenerate requests (trigger_idx is None) never enter this branch,
    matching every pre-W6 test's exact pinned single-placement-per-
    request behavior.

    SPEC-151-SCOPING round 2 (compute_needs's own docstring has the full
    invariant): trigger_idx is no longer always a lone int -- a request
    compute_needs merged from a shared winning interval carries a sorted
    TUPLE of every member_idx relying on it. The stacking loop above
    generalizes accordingly: after each successful placement it re-
    verifies (the SAME fresh transient_report, SAME per-member relief
    formula, one call shared across every trigger checked at that step,
    not one call per trigger) EVERY still-pending carried trigger, drops
    each one that is now genuinely relieved from the pending set, and
    only stops stacking once the pending set is EMPTY (every carried
    trigger relieved) or a candidate/stack-depth bound is hit -- never on
    the first (dict-order) trigger's relief alone. A trigger's own
    independently-computed candidate positions were already folded into
    this SAME request's fallback pool by compute_needs (its own path may
    differ from the request's primary trigger's path), so "its own
    independent request attempt" happens automatically within this one
    pass -- not as a second, separately-tracked call. A single-trigger
    request (the overwhelmingly common case, and every pre-round-2 test)
    is bit-identical: pending starts as a 1-element list, first relief
    empties it, same as the old bare `if relieved: break`. A static/
    degenerate request (trigger_idx None) starts with an EMPTY pending
    set, so the very first successful placement's pending-empty check
    stops it immediately -- bit-identical to the old `if trigger_idx is
    None: break`.

    p6-reach Case 2 fix (forensics: table_mesh member 88/contact 34, Treant
    members 713/719/1104/1183): a SEPARATE, additional pass runs after the
    compute_needs-driven loop above, entirely independent of it (does not
    touch requests/trigger_idx/fallbacks at all). Every one of the 5
    forensic members shares ONE mechanism, and it is NOT reach-limited
    (Case 1's mechanism) -- in all 5, compute_needs's own activation
    trigger (r_transient > cfg.max_unbraced_shaft_r_mm or worst_span_mm >
    cfg.brace_free_span_mm) never fires at all (each member's r_transient/
    worst_span_mm sit comfortably UNDER both ceilings), so NO cross-link
    search is ever attempted, by design -- yet transient.resize_forest's
    r_req = max(r_murray, r_tension, r_deflect, r_transient, kind_floor)
    formula (design-v4.md #6) still grows the member's radius (r_transient
    is the binding term in every forensic case measured), and that growth
    alone -- still well inside the "acceptable to leave unbraced" zone by
    compute_needs's own criteria -- is enough to intrude on real nearby
    geometry that had zero margin to spare. engine.brace_and_size's own
    docstring names the intended remedy verbatim: "the fix is more
    restraint next iteration (another brace)" -- but nothing before this
    fix ever THREADED a real clearance violation into a relief attempt;
    compute_needs's only notion of "needs bracing" was slenderness/span,
    never clearance, so a member in exactly this shape got NO relief
    attempt across all 3 of brace_and_size's iterations (confirmed:
    identical violation, byte for byte, on every iteration -- a dead fixed
    point, not a converging one). This pass closes that gap, self-
    contained within bracing.py (forest.revalidate is a read-only call
    this module already makes elsewhere, e.g. _try_place_rods) -- no
    engine.py wiring change needed: for every member CURRENTLY showing a
    real violation (_clearance_violated_members, the SAME check engine.
    brace_and_size's own loop performs one step later) that owns at least
    one free_gap in spans_report, attempt ONE relief rung in its own
    largest gap via the SAME _place_one_rung primitive (cross-link
    nearest-first -- NOT reach-tiered, see below -- then model_rung
    fallback). Deliberately NOT reach-tiered: the p6-reach widening is
    scoped to genuine activation-trigger requests (is_path_mapped above)
    per spec ("the activation trigger fired") -- these members' activation
    trigger did NOT fire, so extending reach here would misapply Case 1's
    mechanism to a class Case-1 forensics never established it helps
    with (measured: 3 of the 4 Treant members are blocked by the
    rise/extent gate at EVERY candidate distance, not by cfg.
    brace_neighbor_max_xy_mm -- widening reach would not have placed
    anything different for them; only member 713 happens to have a
    same-cap candidate, which the UNWIDENED search already finds).
    Deliberately only ONE attempt per member per call, not a stacking
    loop: a single relief rung cannot itself shrink the violating
    member's OWN radius within this same call (that only happens on the
    CALLER's next transient.resize_forest pass, after propose_and_place
    returns) -- re-checking "is it fixed yet" here would only ever see
    the SAME still-fat radius, so there is nothing a same-call stack
    would learn that the caller's own 3-iteration outer loop does not
    already provide. Height dedup reuses the SAME `tried_zs` map as the
    main loop (never re-attempts a height already tried this call, by
    either mechanism). A placement here can only ever HELP or be a
    harmless no-op: if the member's radius growth genuinely came from
    r_transient/r_deflect (span-driven terms a brace can shrink), the
    next resize_forest call brings the radius back down and the violation
    clears; if the growth came from a span-independent term (r_murray,
    r_tension, a kind floor -- unaffected by bracing), this pass attempts
    relief, it does nothing to fix the actual radius, and the outer loop
    still terminates honestly (the existing "clearance-dirty member"
    assert fires exactly as before -- no silent corruption, matching this
    module's whole design philosophy)."""
    if log is None:
        log = {}
    requests = compute_needs(forest, spans_report, cfg)
    placed = []
    rungs_requested = 0
    rungs_placed = 0
    rungs_unplaceable = 0
    by_kind = {}
    counters = {"brace": 0, "model_rung": 0}
    tried_zs = {}  # host -> [z, ...] attempted this call (primary + fallback)

    for member_idx, z_lo, z_hi, n_rungs, fallbacks, trigger_idx in requests:
        rungs_requested += n_rungs
        is_path_mapped = (z_lo == z_hi)  # SPEC-PATHRUNG W6, see docstring
        restraint_zs = ([] if is_path_mapped else
                         (spans_report.get(member_idx, {}).get("restraint_zs") or []))
        # SPEC-151-SCOPING round 2: normalize trigger_idx to the set of
        # triggers THIS request must relieve before the stacking loop may
        # stop -- empty for a static/degenerate request (trigger_idx is
        # None), a 1-tuple for the overwhelmingly common single-trigger
        # case (bit-identical old behavior), 2+ for a merged shared-
        # interval request (compute_needs's own docstring).
        if trigger_idx is None:
            trigger_idxs = ()
        elif isinstance(trigger_idx, tuple):
            trigger_idxs = trigger_idx
        else:
            trigger_idxs = (trigger_idx,)
        # p6-reach (the pillar fix): widened reach is offered ONLY to a
        # request the activation trigger itself produced (is_path_mapped
        # == compute_needs's own generalized-case tag, carrying a real
        # trigger_idx for later relief-verification) -- a static/
        # degenerate request (trigger_idx None) never receives it, so a
        # gap that merely LOOKS big enough per the static cadence trigger
        # (v3's brace_free_span_mm cadence, blind to activation timing)
        # cannot reach past the standard neighbor cap. See
        # _reach_tiers_mm / _place_one_rung's own docstring for the
        # "only reached when the standard search was EMPTY" guarantee.
        reach_tiers = _reach_tiers_mm(cfg) if is_path_mapped else None
        for k in range(n_rungs):
            z = z_lo + (k + 1) * (z_hi - z_lo) / (n_rungs + 1)
            if any(abs(z - rz) <= cfg.brace_ladder_z_mm / 2.0 for rz in restraint_zs):
                continue
            candidates = [(member_idx, z)]
            if is_path_mapped:
                candidates += [(host, zc) for zc, host in fallbacks]
            placed_any = None
            n_stacked = 0
            pending = list(trigger_idxs)  # triggers not yet confirmed relieved
            for host, zc in candidates:
                already = tried_zs.get(host, [])
                if any(abs(zc - ez) <= cfg.brace_ladder_z_mm / 2.0 for ez in already):
                    continue
                tried_zs.setdefault(host, []).append(zc)
                result = _place_one_rung(forest, surface_tree, cfg, host, zc, k, counters,
                                          reach_tiers=reach_tiers)
                if result is None:
                    continue
                placed_any = (placed_any or []) + result
                n_stacked += 1
                if n_stacked >= _MAX_STACKED_FALLBACKS:
                    break  # SPEC-PATHRUNG W6 should-fix: bounded blast radius,
                    # see _MAX_STACKED_FALLBACKS -- accept whatever this got
                    # us rather than keep placing material indefinitely.
                # SPEC-PATHRUNG W6 should-fix (table_mesh forensics): the
                # single formula-ideal split is only PROVABLY sufficient
                # when it's actually placeable there; a REAL fallback
                # candidate elsewhere on the path can leave a less-than-
                # ideal (imbalanced) split -- e.g. a cross-link's own
                # 45-degree rise pushes its ACTUAL activation height well
                # above the z it was requested at (v3.6's construction,
                # never negotiable). Re-check EVERY still-pending carried
                # trigger (SPEC-151-SCOPING round 2 -- NOT just the first,
                # dict-order one) against the SAME two-part threshold
                # compute_needs used to decide bracing was needed at all;
                # if any remain over budget, keep stacking the NEXT
                # candidate (another real restraint further along the
                # same offending span) rather than declaring victory on
                # geometry alone. Static/degenerate requests (pending
                # starts empty) never enter this branch -- unaffected,
                # bit-identical to pre-round-2.
                if not pending:
                    break
                fresh = transient.transient_report(forest, plate_z, cfg)
                pending = [t_idx for t_idx in pending
                           if not _trigger_relieved(forest, surface_tree, cfg, fresh, t_idx)]
                if not pending:
                    break  # every carried trigger relieved -- stop stacking
                # else: at least one trigger not relieved yet, fall
                # through and try the NEXT candidate too
            if placed_any is None:
                rungs_unplaceable += 1
                continue
            rungs_placed += 1
            for idx, kind in placed_any:
                placed.append(idx)
                by_kind[kind] = by_kind.get(kind, 0) + 1

    # p6-reach Case 2 fix ("more restraint next iteration" -- see this
    # function's own docstring for the full mechanism/forensics). Runs
    # AFTER the compute_needs loop above so it sees this call's own
    # placements too (a member the loop above just relieved is no longer
    # violated, and one it just placed a brace NEAR is unaffected either
    # way -- _clearance_violated_members reads live forest state).
    for member_idx in _clearance_violated_members(forest, surface_tree, cfg):
        info = spans_report.get(member_idx)
        if not info or not info.get("free_gaps"):
            continue
        z_lo, z_hi = max(((float(g[0]), float(g[1])) for g in info["free_gaps"]),
                          key=lambda t: t[1] - t[0])
        z = (z_lo + z_hi) / 2.0
        already = tried_zs.get(member_idx, [])
        if any(abs(z - ez) <= cfg.brace_ladder_z_mm / 2.0 for ez in already):
            continue
        tried_zs.setdefault(member_idx, []).append(z)
        rungs_requested += 1
        result = _place_one_rung(forest, surface_tree, cfg, member_idx, z, 0, counters)
        if result is None:
            rungs_unplaceable += 1
            continue
        rungs_placed += 1
        for idx, kind in result:
            placed.append(idx)
            by_kind[kind] = by_kind.get(kind, 0) + 1

    log["bracing"] = dict(rungs_requested=rungs_requested, rungs_placed=rungs_placed,
                           rungs_unplaceable=rungs_unplaceable, by_kind=dict(by_kind))
    return dict(placed=placed, rungs_requested=rungs_requested, rungs_placed=rungs_placed,
                rungs_unplaceable=rungs_unplaceable, by_kind=by_kind)
