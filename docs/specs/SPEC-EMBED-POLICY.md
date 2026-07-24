# SPEC-EMBED-POLICY — Sanctioned embed inset at designed embed ends

Status: PANEL DECISION 2026-07-23 (judge ruling; pending owner ratification).
Scope: the axis-interval legality check at DESIGNED embed ends (contacts,
strut/rung feet). Non-embed ends get inset 0. Interval interior
classification is per the FINDING-2 robust dense-majority semantics (spec'd
separately); this document defines only the exempt-inset formula and the
legality predicate, which engine (Rust/Python) and census (independent
Python) MUST implement identically from this text. The census never calls
the kernel; alignment means both implement the formulas below.

## Decision

Formula name: **arc-plus-slack** (ARC-SLACK).

Neither incumbent formula survives on its own. The engine's arc formula is
geometrically correct about *where a designed approach legitimately runs
near the surface* (angle-aware envelope-clearance arc) but wrongly dropped
the pitch term entirely: pitch_bound conflates two roles, and only one is
obsolete. Checker-side sampling slack IS obsolete (the oracle is exact).
Builder-side placement slack is NOT: contacts and strut/rung feet are still
chosen from a sampled surface cloud (0.3 mm / 400k points, measured p99
nearest-neighbor gap 0.3510 mm; config.py surface_pitch_bound_percentile),
so a designed embed point legitimately sits up to ~PB off the true surface,
translating the whole entry region along the axis. The evidence signature is
unambiguous: 17–24 shallow-angle quarantine sites overshoot the bare arc
inset by 0.05–0.35 mm — exactly PB's magnitude — i.e. the arc-only formula
over-flags placement slack as illegality. The census's angle-blind formula
meanwhile permits the deepest perpendicular plugs of any variant (up to
1.72 mm at its unpinned PB=0.5706) while condemning a grazing designed
approach (mi=415) whose residual never exceeds the designed contact depth —
a policy misfire on designed geometry, not a real deep transit.

The normative formula is therefore the arc term plus a *pinned* placement-
slack constant, with the exemption restricted to the tip-anchored interval.

## Normative formula

Constants (single source of truth; both implementations use these literals):

    CONTACT_DEPTH_MM = 0.3      # designed embed depth (config.contact_depth_mm)
    STANDOFF_MM      = 0.6      # config.standoff_mm
    EMBED_SLACK_MM   = 0.36     # builder placement slack; see "Slack pinning"
    COS_FLOOR        = 0.5      # 60 deg design-family limit
    TOL_MM           = 0.05     # detection/measurement tolerance

Per designed embed end with tip radius `r_end` (site radius after the
config tip-radius clamp, min 0.2 mm):

1. Let `p_embed` be the designed embed endpoint and `u_out` the unit vector
   along the member axis from `p_embed` toward the member's other end.
2. Project `p_embed` to the TRUE (exact triangle) surface; let `n` be the
   outward unit surface normal at the projection. If the member length
   < 1e-9 or |n| < 1e-9, use `cosang = 1.0`.
3. `cosang = max(abs(dot(u_out, n)), COS_FLOOR)`.
4. **`inset_mm = CONTACT_DEPTH_MM + EMBED_SLACK_MM + (r_end + STANDOFF_MM) / cosang`**

Legality predicate (per member, over its interior intervals along the axis,
member length L, insets inset0/inset1 at the respective ends):

- The ONLY exempt solid material is the single interior interval that
  contains the embedded endpoint (s = 0 for that end, i.e. interval start
  <= TOL_MM from the end), and only up to `inset_mm`: the member is ILLEGAL
  if that interval extends past `inset_mm + TOL_MM` from its embed end.
- Any interior interval NOT anchored to an embed end (a mid-span or
  re-entry interval) is a transit and ILLEGAL regardless of where it falls,
  including inside [0, inset_mm]. A later re-entry after the tip interval
  exits is never exempt.
- Both ends embedded: apply the rule independently per end; an interval
  anchored to neither end is illegal.

Numerical instantiation at r_end = 0.2: 1.46 mm perpendicular, 1.79 mm at
45 deg, 2.26 mm at/beyond 60 deg. Normal-depth self-cap: axis run times
cos(theta) never exceeds CONTACT_DEPTH + EMBED_SLACK + r_end + STANDOFF
(1.46 mm at r=0.2), monotone decreasing with angle — the formula cannot
sanction a deep plug by way of the angle term.

## Slack pinning and retirement path

EMBED_SLACK_MM = 0.36 is the builder cloud's measured p99 nn-gap (0.3510,
0.3 mm / 400k tree) rounded up to a stable literal. It is a PINNED spec
constant, not a runtime-derived value — the 0.351-vs-0.5706 cloud
dependence swung 7 verdicts and is exactly the non-determinism this spec
removes. It MUST be re-ratified if the builder cloud pitch/count changes.
It drops to 0.0 if/when the builder snaps designed embed points to the
exact surface at placement time (recommended follow-up; retires the term).
Known residual (accepted): historical placement outliers beyond p99 exist
(max 0.61 mm; the recorded 0.537 mm brace_and_size anchor) — those sites
fail legality and are correctly quarantined/relocated, not exempted.

## Why not the alternatives

- **Arc-only (engine current):** over-flags 17–24 shallow sites whose only
  defect is placement slack; forces relocations (and removal scars) the
  geometry does not require. Rejected.
- **Census perpendicular (CD + PB + r + standoff):** angle-blind — permits
  the deepest perpendicular plugs of any variant while condemning
  design-depth grazing approaches (mi=415, 79.8 deg, max penetration
  0.316 mm <= designed 0.3 mm contact depth, removal is a peel of a
  ~1.76 x 0.4–0.6 mm surface rib squarely inside the owner's "slight model
  damage fixed in post" concession). PB unpinned. Rejected.
- **Codex split (exempt only s_exit ≈ CD + coupon-calibrated e_scar;
  reject >60 deg sites):** conceptually right that penetration and
  clearance are distinct — this spec adopts its structural guard (the
  first-interval-only / transits-always-illegal rule) — but e_scar has no
  calibration data today and rejecting the 21+ existing >60 deg crown
  sites relocates the whole grazing population for no demonstrated
  structural benefit (tip capacity is governed by the neck fuse; extra
  embed is scar, not margin — physics_contacts_tips.md sec 2.1). Coupon
  calibration is endorsed as follow-up work that may later tighten
  EMBED_SLACK_MM or the arc term; the 60 deg floor stays as the cap on
  the exempt zone rather than a site-rejection rule.

## Impact (measured, this session's panel analysis)

Of the 81 formula-sensitive sites (80 archived P11 quarantine + crown stub
mi=415): illegal ~50 under ARC-SLACK vs 80 (arc-only) vs 57–64 (census,
PB-dependent). mi=415 becomes LEGAL (grazing at design depth). Census-real
illegal members drop 2 -> 1 (strut mi=1138 has no embed end and remains
illegal under every formula). No archived-REAL member was rescued by the
arc term alone; all quarantine candidates are re-graded under this formula
AFTER the FINDING-2 robust interior classification lands, with the P11
labeled set (ARTIFACT: 3, 223, 232; REAL: 8, 253, 297, 316, 323, 325, 333)
as the regression fixture. Worst legal scars: perpendicular plug 0.4 mm dia
x <=1.46 mm; grazing surface rib <=~2.3 mm long x ~0.4–0.6 mm wide at
<=~0.4 mm depth — both inside the ratified concession class (buff/fill/
paint; Sunlu ABS-like tears rather than shatters, resin_sunlu.md).

## Implementation notes

- Engine: `_embed_inset_arc` (scripts/support_placement/tree.py) adds
  `EMBED_SLACK_MM` to its return and the interval gate adopts the
  tip-anchored-interval-only rule; kernel unchanged except interval
  semantics per FINDING-2.
- Census: replaces `CD + PB + r + standoff` with this formula, computing
  cosang from its own independent exact projection (it never calls the
  kernel), and pins EMBED_SLACK_MM = 0.36 instead of deriving PB from any
  cloud.
- Both use TOL_MM = 0.05 identically; site-derived regression tests
  required per the binding rules (include mi=415 legal, mi=1138 illegal,
  one shallow placement-slack flip site legal, one transit-interval
  illegal case).

## Evidence

docs/artifacts/audit-2026-07/acceptance-run-2026-07-23/ (census_core.py,
census_run.py, p11_crossexam.py, validate_r4_embed_inset.py, results/),
docs/artifacts/audit-2026-07/{physics_contacts_tips.md, resin_sunlu.md,
TREANT-V4-PRINT-AUDIT.md}, scripts/support_placement/config.py,
scripts/support_placement/tree.py (_embed_inset_arc docstring).
