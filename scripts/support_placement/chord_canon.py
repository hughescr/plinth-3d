"""Amendment G / SPEC-P8-RELOCATE.md sec 5.3 Layer 3, sec 6 item 3, sec 8
(`p8_chord_canon_micron`) -- canonicalized integer-micron chord comparison.

engine==census cross-implementation equality is asserted on crossing chords
rounded to an integer number of microns, NOT float-bit-identity: the two
independent implementations' raw float chords only ever differed below
micron granularity (the mi=445/474/575 symdiff, SPEC-P8-RELOCATE.md sec 5.3),
so bit-identity is the wrong bar and micron-canonical equality is the right
one.

Deliberately dependency-free (no numpy, no package-relative imports, no
`config.SupportConfig`): this module is the ONE shared primitive the engine's
own P8 acceptance harness (`p8_accept.py`) AND an external, independent
census-comparison script both import, so the canonicalization rule itself
never drifts between the two callers. It is a COMPARISON primitive only --
the firm/moot classification itself (`verify._transit_classify`) stays on
raw-float `>=` against `PRINT_FLOOR_CHORD_MM` (SPEC-P8-RELOCATE.md sec 5.3
Layer 1: "the kernel robust oracle is authoritative, and is NEVER
de-robustified"). Nothing here ever feeds back into that predicate.
"""
from __future__ import annotations

__all__ = ["canon_chord_micron", "chords_match_micron", "chord_parity_report"]


def canon_chord_micron(chord_mm: float, micron: int = 1) -> int:
    """Canonicalize a crossing chord (mm) to an integer count of `micron`-sized
    buckets (default granularity 1 micron = 0.001mm, `p8_chord_canon_micron`
    normative default). Plain round-half-away-from-zero on the micron count --
    deterministic, no RNG, no platform-dependent banker's-rounding surprise
    (Python's builtin `round` on a `float` does round-half-to-even, which is
    fine here too since both callers get the SAME rule from the SAME function;
    the only requirement is that engine and census canonicalize identically,
    not which convention they share)."""
    micron = int(micron)
    if micron <= 0:
        raise ValueError(f"micron granularity must be a positive int, got {micron!r}")
    microns_raw = float(chord_mm) * 1000.0
    buckets = int(round(microns_raw / micron))
    return buckets * micron


def chords_match_micron(a_mm: float, b_mm: float, micron: int = 1) -> bool:
    """True iff two raw chord readings (mm) canonicalize to the SAME integer
    micron value -- the amendment-G equality test, applied pairwise."""
    return canon_chord_micron(a_mm, micron) == canon_chord_micron(b_mm, micron)


def chord_parity_report(engine_chords_by_mi: dict, census_chords_by_mi: dict,
                        micron: int = 1) -> dict:
    """SPEC-P8-RELOCATE.md sec 6 item 3: "engine gate == census exactly, on
    canonicalized integer-micron chords ... every chord rounded to integer
    microns before the comparison". Each argument maps a stable member id
    (`mi`) to the list of raw crossing chords (mm) that member reported --
    unordered, one entry per rule-4 crossing (firm or moot; a member with no
    illegal crossing at all is simply absent from the map, matching
    `verify._transit_classify`'s own "no illegal crossing -> (False, False)"
    contract).

    Returns a report dict; `ok` is True iff BOTH member-id sets are IDENTICAL
    (the symdiff is empty -- the amendment-G target: "after relocation they
    agree member-for-member with the symdiff empty") AND every shared member's
    sorted canonicalized chord multiset matches exactly. `only_engine` /
    `only_census` surface any id present on only one side (a population
    mismatch, the more serious failure); `mismatches` surfaces shared ids
    whose canonicalized chords disagree (a granularity/reading mismatch).

    This ONE function is called by both the engine's own P8 acceptance
    harness (`p8_accept.final_geometry_verify` et al.) and any external,
    independently-written census-comparison script -- neither side
    reimplements the canonicalization rule, so there is no way for the two
    callers' rounding to silently diverge."""
    engine_ids = set(engine_chords_by_mi)
    census_ids = set(census_chords_by_mi)
    only_engine = sorted(engine_ids - census_ids)
    only_census = sorted(census_ids - engine_ids)
    mismatches = []
    for mi in sorted(engine_ids & census_ids):
        e = tuple(sorted(canon_chord_micron(c, micron) for c in engine_chords_by_mi[mi]))
        c = tuple(sorted(canon_chord_micron(c, micron) for c in census_chords_by_mi[mi]))
        if e != c:
            mismatches.append(dict(member_idx=mi, engine_micron=e, census_micron=c))
    ok = not only_engine and not only_census and not mismatches
    return dict(ok=ok, only_engine=only_engine, only_census=only_census,
                mismatches=mismatches, micron=int(micron),
                n_engine=len(engine_ids), n_census=len(census_ids),
                n_shared=len(engine_ids & census_ids))
