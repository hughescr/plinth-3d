"""P11 condemned-stub cross-examination -- ruling 1A (settled) regression tests.

Guards the SETTLED P11 fixture and its two load-bearing findings against silent
drift:

  * Ruling 1A ground truth: the 9 curated exemplars carry fixed labels
    {ARTIFACT: 3; REAL: 8, 223, 232, 253, 297, 316, 323, 325, 333}. mi=3 is kept
    ARTIFACT as a DOCUMENTED known rule-4 shallow-graze false positive (a
    0.406mm-arc surface graze only 0.0574mm deep); its predicate fix is deferred
    to the placement round. 223/232 FLIP to REAL (buried-root re-entry) vs the
    retired arc-inset tool's ARTIFACT call.

  * Three-oracle agreement: on every buried-root re-entry interior point the
    three INDEPENDENT inside-oracles agree -- the kernel (ExactSurface parity),
    the census dense-majority ParityVote, and an independent 16-ray exact
    trimesh majority. The real-mesh verdicts are frozen in fixtures_p11.json;
    this file additionally proves the three METHODS agree on controlled
    synthetic geometry (method-independence, hermetic -- no real STL needed).

  * mi=3 print-floor interaction (the finding this task settled): the PRINT-FLOOR
    moot rule is CHORD-based (feature_thickness < 0.08mm) and is ORTHOGONAL to
    mi=3's shallow-DEPTH disqualifier. mi=3 has 5 unanchored illegal crossings;
    only ONE chord is sub-floor, so the member is NOT all-moot and stays
    NON-EXEMPT (raw predicate = REAL, i.e. IN the Phase-8 relocation set). The
    print-floor does NOT mechanically express the owner's depth intuition; mi=3
    is ARTIFACT only under the depth-aware ground truth.

Rule-4 illegality is asserted on the DEEP re-entry class (223/232/8) by replaying
their recorded interior intervals through verify._transit_illegal -- NEVER on
mi=3 (whose ARTIFACT status is a deferred depth-predicate question, not a
mechanical rule-4 flag).

The predicate MECHANICS (moot floor, SCAR-AREA-CAP, capsule graze) are covered in
test_v5_gates.py; this file is the FIXTURE-anchored consumer guard.
"""
import json
import os

import numpy as np
import trimesh
import pytest

from support_placement import verify
from support_placement.config import SupportConfig
from support_placement.geometry_exact import ExactSurface

HERE = os.path.dirname(__file__)
FIXTURE = os.path.join(HERE, "fixtures_p11.json")
PHASE8 = os.path.join(HERE, "..", "..", "docs", "artifacts", "audit-2026-07",
                      "acceptance-run-2026-07-23", "phase8_relocation_input.json")


@pytest.fixture(scope="module")
def fx():
    with open(FIXTURE) as f:
        return json.load(f)


@pytest.fixture(scope="module")
def phase8():
    with open(PHASE8) as f:
        return json.load(f)


@pytest.fixture
def cfg():
    return SupportConfig()


# --------------------------------------------------------------------------
# Ruling 1A: labels + provenance integrity
# --------------------------------------------------------------------------
def test_ruling_1a_labels_and_counts(fx):
    assert fx["ruling"] == "1A"
    assert fx["labels"]["ARTIFACT"] == [3]
    assert sorted(fx["labels"]["REAL"]) == [8, 223, 232, 253, 297, 316, 323, 325, 333]
    assert fx["label_counts"] == {"ARTIFACT": 1, "REAL": 9}
    # 223/232 flipped to REAL (the ratified reversal of the retired arc-inset tool).
    for mi in (223, 232):
        assert fx["exemplars"][str(mi)]["ground_truth"] == "REAL"


def test_fixture_predicate_is_ratified_not_stale(fx):
    p = fx["provenance"]["predicate"]
    assert "SCAR-AREA-CAP" in p and "rule-4" in p and "PRINT-FLOOR" in p
    assert fx["provenance"]["print_floor_chord_mm"] == verify.PRINT_FLOOR_CHORD_MM


def test_mi3_kept_artifact_as_documented_shallow_graze_fp(fx):
    """mi=3 label stays ARTIFACT (depth-aware GT) even though the raw ratified
    predicate grades it REAL -- the deferral must remain visible."""
    ex = fx["exemplars"]["3"]
    assert ex["ground_truth"] == "ARTIFACT"
    assert ex["raw_predicate"] == "REAL"      # raw predicate disagrees -> documented FP
    assert "GROUND TRUTH" in fx["_doc"] and "shallow-graze false positive" in fx["_doc"]


# --------------------------------------------------------------------------
# mi=3 print-floor interaction (the settled finding)
# --------------------------------------------------------------------------
def test_mi3_printfloor_is_orthogonal_to_shallow_depth(fx):
    """The chord-based moot floor does NOT catch mi=3: only 1 of its 5 unanchored
    crossings is sub-floor, so it is not all-moot and stays non-exempt (raw REAL).
    Its disqualifier is DEPTH (0.0574mm), orthogonal to the 0.08mm CHORD floor."""
    m = fx["mi3_print_floor_interaction"]
    assert m["moot"] is False
    assert m["nonexempt"] is True
    assert m["all_chords_below_floor"] is False
    assert m["floor_mm"] == verify.PRINT_FLOOR_CHORD_MM == 0.08
    # exactly one crossing dips below the floor; the long graze arc is firm.
    below = [c for c in m["unanchored_illegal_crossings"] if c["below_floor"]]
    assert len(below) == 1
    assert m["max_arc_chord_mm"] > verify.PRINT_FLOOR_CHORD_MM     # 0.406mm graze is firm
    assert m["depth_mm"] < verify.PRINT_FLOOR_CHORD_MM             # shallow, but that's depth


# --------------------------------------------------------------------------
# Three-oracle agreement (frozen real-mesh verdicts)
# --------------------------------------------------------------------------
@pytest.mark.parametrize("mi", [223, 232, 8])
def test_three_oracle_agreement_frozen_buried_root(fx, mi):
    """Every recorded buried-root re-entry interior point has kernel == census
    ParityVote == 16-ray exact, all inside."""
    pts = fx["exemplars"][str(mi)]["three_oracle"]
    assert pts, f"mi={mi} must record at least one unanchored interior point"
    for p in pts:
        assert p["kernel_inside"] is True
        assert p["census_parityvote_inside"] is True
        assert p["ray16_inside"] is True
        assert p["agree"] is True


# --------------------------------------------------------------------------
# Rule-4 illegality replayed on the DEEP re-entry class (223/232/8), NOT mi=3
# --------------------------------------------------------------------------
@pytest.mark.parametrize("mi", [223, 232, 8])
def test_rule4_illegal_on_deep_reentry_replayed(fx, mi):
    """Replay each deep-re-entry exemplar's RECORDED axis intervals through the
    ratified predicate: the sanctioned tip embed is exempt, the buried-root
    re-entry interval is an unanchored transit -> the member is NON-EXEMPT
    (hard rule-4 violation). Dropping the re-entry interval (tip only) makes the
    legal verdict exempt the member entirely -- proving the flag is driven by the
    re-entry, not the embed."""
    ex = fx["exemplars"][str(mi)]
    m = dict(p0=np.asarray(ex["p0"], float), p1=np.asarray(ex["p1"], float),
             r0=float(ex["r0"]), r1=float(ex["r1"]), kind="stub",
             owner=("contact", 0), embed_end=ex["embed_end"])
    ivs = [dict(s_a=c["s_a"], s_b=c["s_b"], ray="axis") for c in ex["crossings"]]
    vlegal = ex["verdict_legal"]
    assert verify._transit_illegal(ivs, m, vlegal) is True
    # tip-anchored interval alone, under a legal verdict, must NOT flag.
    tip = [iv for iv, c in zip(ivs, ex["crossings"]) if c["tip_anchored"]]
    if tip and vlegal is True:
        assert verify._transit_illegal(tip, m, True) is False


def test_mi3_not_asserted_illegal_by_rule4_here(fx):
    """Contract guard: this suite deliberately does NOT assert a rule-4 flag on
    mi=3. Its ARTIFACT status is a DEFERRED depth-predicate question; under the
    current (chord-only) ratified predicate mi=3 is raw=REAL, and that is exactly
    what the Phase-8 relocation input records."""
    ex = fx["exemplars"]["3"]
    assert ex["ground_truth"] == "ARTIFACT" and ex["raw_predicate"] == "REAL"


# --------------------------------------------------------------------------
# Hermetic three-oracle METHOD independence + rule-4 on a synthetic
# buried-root re-entry soup (no real STL required)
# --------------------------------------------------------------------------
def _fib_dirs(n):
    ga = np.pi * (3 - 5 ** 0.5)
    out = []
    for i in range(n):
        z = 1 - (i + 0.5) * 2.0 / n
        rr = (1 - z * z) ** 0.5
        th = ga * i
        out.append([rr * np.cos(th), rr * np.sin(th), z])
    return np.asarray(out, float)


def _ray_parity_inside(mesh, P, dirs):
    """Independent parity-majority inside test: odd surface crossings along a
    ray => inside; majority vote over a deterministic direction set."""
    votes = 0
    for d in dirs:
        loc = mesh.ray.intersects_location([P], [d])[0]
        votes += 1 if (len(loc) % 2 == 1) else 0
    return votes * 2 >= len(dirs)


def test_hermetic_three_oracle_and_rule4_on_buried_reentry(cfg):
    """A stub tip-embedded in an UPPER slab that re-enters and buries its free end
    in a SEPARATE lower slab -- the buried-root re-entry geometry class of
    223/232/8. On the deep interior midpoint the three INDEPENDENT methods agree
    (kernel ExactSurface.inside == 32-ray parity majority == 16-ray majority), a
    clearly-exterior point agrees False, and the ratified predicate flags the
    re-entry as a NON-EXEMPT rule-4 transit."""
    upper = trimesh.creation.box(extents=(20, 20, 4)); upper.apply_translation((0, 0, 6))
    lower = trimesh.creation.box(extents=(20, 20, 4)); lower.apply_translation((0, 0, -6))
    soup = trimesh.util.concatenate([upper, lower])
    body = trimesh.Trimesh(vertices=soup.vertices.copy(), faces=soup.faces.copy(),
                           process=False)
    geom = ExactSurface(body)

    inside_pt = np.array([0.0, 0.0, -6.0])      # centroid of the lower slab
    outside_pt = np.array([0.0, 0.0, 0.0])      # the air gap between the slabs
    d32, d16 = _fib_dirs(32), _fib_dirs(16)
    for P, expect in ((inside_pt, True), (outside_pt, False)):
        k = bool(geom.inside(np.array([P]))[0])
        pv = _ray_parity_inside(body, P, d32)
        r16 = _ray_parity_inside(body, P, d16)
        assert k == pv == r16 == expect, (P.tolist(), k, pv, r16, expect)

    # A stub: tip embedded in the UPPER slab (p0 just inside its underside z=4),
    # shaft crosses the air gap, free end buried deep in the LOWER slab.
    m = dict(p0=np.array([0.0, 0.0, 4.0]), p1=np.array([0.0, 0.0, -6.0]),
             r0=0.2, r1=0.35, kind="stub", owner=("contact", 0), embed_end="p0")
    assert bool(geom.inside(np.array([m["p1"]]))[0])        # free end interior
    ivs = geom.penetration_intervals(m["p0"], m["p1"], m["r0"], m["r1"],
                                     rays="axis", min_illegal_len_mm=0.05)
    verdicts = verify._embed_scar_verdicts(geom, [m], [0], {0: ivs}, cfg)
    # buried-root re-entry => non-exempt hard rule-4 violation, regardless of the
    # sanctioned tip embed's SCAR verdict.
    assert verify._transit_illegal(ivs, m, verdicts.get(0)) is True
    # verify's independent clearance gate flags it too (empty moot bucket -- these
    # are firm >>0.08mm chords, not knife-edges).
    moot = []
    rep = verify.clearance_report(None, [m], cfg, geom=geom, moot_transit_out=moot)
    assert [v["member_idx"] for v in rep] == [0]
    assert moot == []


# --------------------------------------------------------------------------
# Phase-8 relocation input + 193 print-floor partition integrity
# --------------------------------------------------------------------------
def test_phase8_relocation_targets_raw_regrade(phase8, fx):
    rt = phase8["relocation_targets"]
    assert rt["n_sites"] == 80
    assert rt["n_real"] == 77 and rt["n_artifact"] == 3
    assert sorted(rt["artifact_stub_members"]) == [437, 440, 444]
    assert len(rt["real_stub_members"]) == 77
    # mi=3 is REAL under the raw predicate -> IN the relocation set (the fixture's
    # depth-aware ARTIFACT override does NOT remove it from Phase-8's input).
    assert 3 in rt["real_stub_members"]


def test_phase8_printfloor_193_partition(phase8):
    p = phase8["printfloor_193"]
    assert p["n_illegal_members"] == 193
    # Corrected partition 2026-07-24 (amendment A): mi=420 is ALL-MOOT at full
    # IEEE-754 precision; the earlier 144/49/band-26 was a rounding artifact.
    assert p["n_non_exempt"] == 143 and p["n_moot"] == 50
    assert p["partition"] == {"firm_ge_0_10": 118, "band_0_08_to_0_10": 25,
                              "moot_all_lt_0_08": 50}
    assert len(p["non_exempt_member_idxs"]) == 143
    assert len(p["moot_member_idxs"]) == 50
    # mi=420 lives in moot, not non-exempt.
    assert 420 in p["moot_member_idxs"] and 420 not in p["non_exempt_member_idxs"]
    # disjoint partition of exactly the 193.
    ne = set(p["non_exempt_member_idxs"]); mo = set(p["moot_member_idxs"])
    assert not (ne & mo) and len(ne | mo) == 193
