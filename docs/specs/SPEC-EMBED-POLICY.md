# SPEC-EMBED-POLICY — Sanctioned embed at designed embed ends: scar-area bound

Status: PANEL DECISION 2026-07-23 (judge ruling; pending owner ratification).
Supersedes: the same-day ARC-SLACK depth-inset ruling (committed 3920a0e),
overtaken before ratification by the owner's binding reframing: union is
idempotent, interior overlap is physically meaningless, and the embed's
physical consequences live at the SURFACE-CROSSING PATCH, whose area sets
adhesion strength and removal-scar size. Depth is a proxy that this panel's
measurements show points the WRONG WAY on the real population
(corr(depth, patch area) = -0.44 to -0.53): deep near-perpendicular embeds
have the smallest patches; shallow grazing chords have the largest. Both
incumbent depth formulas (engine arc, census perpendicular) enforce not one
area bound but a ~25-30x scatter of effective bounds, unbounded at grazing
angles. They are retired as sanctions.

Scope: legality of DESIGNED embed ends (contact-stub tips, strut/rung feet,
model-rung ends). Non-embed ends receive no exemption. Interval interior
classification follows the FINDING-2 robust dense-majority semantics
(spec'd separately). Engine (Rust/Python) and census (independent Python)
MUST implement the formulas below identically from this text alone; the
census never calls the kernel — alignment means both independently
implement the same spec'd formulas on exact triangle geometry.

## Decision

Formula name: **SCAR-AREA-CAP** (capped-ellipse shaft term + ball-disc
term, per-class cap, projected-area anchor floor, strict crossing
topology).

The sanction is an area bound on a conservative closed-form upper bound of
the surface-crossing patch, plus a scar max-dimension bound, plus a
minimum PROJECTED (angle-free) anchor area — adopting the physics brief's
area lens and the Codex corrections: raw patch area must not be credited
as adhesion strength (the grazing ellipse grows as 1/cos while the axial
section stays pi*r^2), area alone permits a long thin gash (hence the
length bound), and only declared embed endpoints get an exception
(undeclared crossings remain collisions).

## Normative constants (single source of truth; literal in both implementations)

    A_ABS_MM2     = 2.0     # absolute scar-area allowance (1.60 mm equiv. scar dia)
    KAPPA         = 1.25    # headroom multiplier on the designed crossing disc
    L_SCAR_MAX_MM = 2.5     # max scar patch dimension
    EPS_COS       = 1e-9    # divide-by-zero guard only; NOT a physics floor
    TOL_MM        = 0.05    # interval anchoring / detection tolerance
    BALL_FACTOR   = 1.5 for contact-stub tips; 1.4 for strut/rung feet
                            # (config.py / joints.py MODEL_FOOT_FACTOR — designed ball radii)

The legacy COS_FLOOR = 0.5 (60 deg clamp) is ABOLISHED for area purposes:
clamping cos conceals exactly the grazing growth this policy exists to
catch. pitch_bound is RETIRED entirely: with exact oracles on both sides
there is no sampled cloud in the comparison, and PB is measured process
noise (+/-63% with tree parameters). No placement-slack constant is
needed either — this policy measures the ACTUAL crossing, so builder
placement slack moves the crossing location but never requires a budget
term.

## Normative quantities (per designed embed end)

All geometry exact (Moller ray-triangle; exact signed distance). IEEE-754
double throughout; identical formulas give identical bits in Rust and
Python.

- `p0`   designed embed endpoint (tip / foot ball center).
- `u`    unit axis vector from the member's other end toward `p0`.
- `r_end` end radius after the config tip-radius clamp (min 0.2 mm).
- `r_root`, `L_mem` the member's other-end radius and axis length;
  frustum radius `r(s) = r_end + (r_root - r_end) * clamp(s / L_mem, 0, 1)`
  with `s` measured along the axis from `p0`.
- `T`    length of the TIP-ANCHORED interior interval: the solid interval
  along the axis whose start lies within TOL_MM of `p0` (under FINDING-2
  robust classification). `T = 0` if no such interval exists.
- `x_c`  the exit crossing of that interval (axis point at `s = T`);
  `n` the outward unit normal of the crossing triangle at `x_c`
  (tie-break for edge/vertex hits: lowest triangle index).
  `cosang = abs(dot(u, n))`.
- `r_x = r(T)` frustum radius at the exit crossing.
- `R_b = BALL_FACTOR * r_end` designed tip/foot ball radius.
- `d`    absolute exact signed distance from `p0` to the surface.

## Normative formulas

Shaft (capped ellipse; the transit cap 2*r_x*T is the true grazing-chord
bound and replaces any cos floor):

    A_shaft = 0                                                if T <= TOL_MM
    A_shaft = min( pi * r_x^2 / max(cosang, EPS_COS),
                   2.0 * r_x * T )                             otherwise

Ball disc (angle-independent):

    A_ball  = pi * max(R_b^2 - d^2, 0.0)

Patch-area upper bound and designed reference:

    A_patch  = A_shaft + A_ball
    A_design = pi * R_b^2
    A_max    = max(A_ABS_MM2, KAPPA * A_design)

Scar max dimension (ellipse major diameter, capped by the chord extent):

    L_scar = 0                                                 if T <= TOL_MM
    L_scar = min( 2.0 * r_x / max(cosang, EPS_COS),
                  T + 2.0 * r_x )                              otherwise

Anchor area (PROJECTED, angle-free — never credit the 1/cos ellipse
inflation as strength):

    A_anchor = A_ball + ( pi * r_x^2  if T > TOL_MM else 0.0 )

## Legality predicate (per designed embed end)

LEGAL iff ALL of:

1. **Scar cap:**        `A_patch <= A_max`
2. **Scar dimension:**  `L_scar <= L_SCAR_MAX_MM`
3. **Anchor floor:**    `A_anchor >= pi * r_end^2`
   (the fuse/neck floor: the break must happen at the tip neck; also
   catches detached tips — `T = 0` and `d >= R_b` gives `A_anchor = 0`,
   ILLEGAL. Depth checks were structurally blind in this direction.)
4. **Crossing topology (retained interval checks, unchanged):** only the
   tip-anchored interval is exempt-eligible. ANY interior interval not
   anchored to a designed embed end (mid-span graze, re-entry after the
   tip interval exits, capsule-edge crossing) is a transit and ILLEGAL
   regardless of its size. Both-ends-embedded members apply this per end
   independently. The thin-feature-span check remains a separate,
   unchanged check.

There is NO depth cap on the tip-anchored interval itself: a deep
perpendicular plug into thick solid has a small crossing patch and, per
the owner's idempotence ruling, no further physical consequence; thin
walls are protected by rule 4 and the thin-feature-span check, and long
grazing chords are bounded by rules 1-2 (A_shaft grows as 2*r_x*T).

## Rationale (measured on the 853-member run1 designed-embed population)

- Actual patch areas span 34x (stubs 0.126-4.23 mm^2); the depth formulas'
  implicit area bounds span ~25-30x and diverge at grazing. Area is the
  quantity the owner's concession ("slight model damage fixed in post")
  is actually about; A_ABS = 2.0 mm^2 caps the scar at 1.60 mm equivalent
  diameter, between the designed stub scar (0.6 mm) and foot discs
  (0.98 mm median dia), inside the buff/fill/paint class.
- KAPPA * A_design keeps every DESIGNED foot ball legal by construction
  (feet are pi*(1.4 r)^2 >= 0.754 mm^2 by design, up to 8.99 mm^2 at
  large strut radii — a flat cap below that would outlaw the sanctioned
  design; a flat cap above it would let small tips gouge).
- L_SCAR_MAX = 2.5 mm accepts the known-benign grazing rib class
  (mi=415: 2.26 mm) while rejecting the pathological long-gash geometry
  (Codex's 85 deg example: 4.59 mm footprint).
- Anchor floor: zero members violate it today, but the pipeline
  demonstrably produces near-tangent contacts (156 stub centers outside
  the surface; 1 beyond its own ball radius) — the failure mode exists
  and only an area-style check can see it.
- P11 REAL vs ARTIFACT actual-area distributions overlap almost
  completely: the depth/interval classification never separated sites by
  physical scar; this policy does.

## Impact

- Crown stub mi=415 (79.8 deg graze): A_patch <= 0.76 mm^2, L_scar
  2.26 mm, anchor pi*r_x^2 >= pi*r_end^2 -> LEGAL (mid-pack under the
  area lens; smaller than the median designed foot disc).
- Strut mi=1138: the foot disc itself is legal (0.754 <= 2.0), but its
  capsule-edge graze is an unanchored crossing -> remains ILLEGAL under
  rule 4. Census-real illegal members 2 -> 1.
- New flags concentrate exactly where physical scars are largest:
  measured actual areas exceed 2.0 mm^2 on ~23 stubs (plus any feet whose
  actual patch exceeds KAPPA times their designed disc), and grazing
  sites with footprint > 2.5 mm fail rule 2. These are genuine
  >1.6-mm-dia scars the depth formulas silently sanctioned. All ~80
  quarantine candidates are re-graded under this predicate AFTER the
  FINDING-2 robust interior classification lands; the P11 labeled set
  (ARTIFACT: 3, 223, 232; REAL: 8, 253, 297, 316, 323, 325, 333) is the
  regression fixture.
- Ratification levers (the only tunables): A_ABS_MM2, KAPPA,
  L_SCAR_MAX_MM.

## Implementation notes

- Engine: the R4 path already computes every input (r_x via the frustum,
  exit crossing + normal via exact projection, d via exact signed
  distance, T via kernel intervals under FINDING-2 semantics).
  `_embed_inset_arc` and its inset window are removed from the sanction
  path; intervals feed only rule 4 and thin-feature detection.
- Census: replaces `CD + PB + r + standoff` with this predicate, computing
  cosang/d/T from its own independent exact machinery (dense ParityVote +
  signed distance; it never calls the kernel). No PB, no cloud-derived
  constant anywhere.
- Known approximation (accepted): patch areas use a locally-planar
  surface model; the 2*r_x*T and T+2*r_x caps bound the curvature error.
  A future exact patch extraction may tighten, never loosen, A_patch.
- Site-derived regression tests required per the binding rules, minimum
  set: mi=415 LEGAL; mi=1138 ILLEGAL (unanchored crossing); one detached
  tip (d > R_b, T = 0) ILLEGAL via anchor floor; one >2.0 mm^2 grazing
  stub ILLEGAL via scar cap; one large-radius designed foot LEGAL via
  KAPPA * A_design; one mid-span transit ILLEGAL. Suite + clippy green;
  determinism preserved (all inputs exact, all constants pinned).

## Evidence

docs/artifacts/audit-2026-07/acceptance-run-2026-07-23/ (census_core.py,
census_run.py, p11_crossexam.py, validate_r4_embed_inset.py, results/),
panel area analysis (853-member run1 forest, scratchpad
panel_area_analysis.py), docs/artifacts/audit-2026-07/
{physics_contacts_tips.md, resin_sunlu.md, TREANT-V4-PRINT-AUDIT.md},
scripts/support_placement/config.py, scripts/support_placement/joints.py
(MODEL_FOOT_FACTOR), scripts/support_placement/chitubox_emit.py (tip ball
synthesis).
