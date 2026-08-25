# Embed policy — sanctioned embed at designed embed ends (SCAR-AREA-CAP)

Status: OWNER RATIFIED (decision id `SCAR-AREA-CAP`). PRINT-FLOOR amendment
owner-ratified 2026-07-24. TRUNCATION amendment owner-ruled 2026-08-22 (amends
the no-depth-cap paragraph only; every other rule and constant below is
unamended by it).

Scope: legality of DESIGNED embed ends (contact-stub tips, strut/rung feet,
model-rung ends). Non-embed ends receive no exemption. The engine (Rust) and
the independent census (Python) implement the formulas below identically from
this text alone — the census never calls the engine kernel; agreement between
the two independent implementations is the alignment check.

The sanction is an area bound on a conservative closed-form upper bound of the
surface-crossing patch, plus a scar max-dimension bound, plus a minimum
PROJECTED (angle-free) anchor area, with strict crossing topology. Raw patch
area is never credited as adhesion strength on its own (the grazing ellipse
grows as `1/cos` while the axial section stays `pi*r^2`); area alone would
permit a long thin gash (hence the length bound); only declared embed
endpoints get the exemption (an undeclared crossing remains a collision).

## Normative constants (single source of truth; literal in both implementations)

    A_ABS_MM2     = 2.0     # absolute scar-area allowance (1.60 mm equiv. scar dia)
    KAPPA         = 1.25    # headroom multiplier on the designed crossing disc
    L_SCAR_MAX_MM = 2.5     # max scar patch dimension
    EPS_COS       = 1e-9    # divide-by-zero guard only; NOT a physics floor
    TOL_MM        = 0.05    # interval anchoring / detection tolerance
    PRINT_FLOOR_CHORD_MM = 0.08  # print-floor moot threshold (owner-ratified 2026-07-24)
    BALL_FACTOR   = 1.5 for contact-stub tips; 1.4 for strut/rung feet
                            # (designed ball radii, registered as
                            # BALL_FACTOR_STUB / BALL_FACTOR_FOOT)

There is no `COS_FLOOR` clamp: clamping cosine would conceal the exact grazing
growth this policy exists to catch. There is no `pitch_bound` term and no
placement-slack budget term — this policy measures the actual crossing, so
builder placement slack moves the crossing location but never requires a
budget term.

## Normative quantities (per designed embed end)

All geometry exact (Möller ray–triangle; exact signed distance). IEEE-754
double throughout; identical formulas give identical bits in Rust and Python.

- `p0` — designed embed endpoint (tip / foot ball center).
- `u` — unit axis vector from the member's other end toward `p0`.
- `r_end` — end radius after the config tip-radius clamp (minimum 0.2 mm).
- `r_root`, `L_mem` — the member's other-end radius and axis length; frustum
  radius `r(s) = r_end + (r_root - r_end) * clamp(s / L_mem, 0, 1)`, `s`
  measured along the axis from `p0`.
- `T` — length of the TIP-ANCHORED interior interval: the solid interval along
  the axis whose start lies within `TOL_MM` of `p0`. `T = 0` if no such
  interval exists.
- `x_c` — the exit crossing of that interval (axis point at `s = T`); `n` —
  the outward unit normal of the crossing triangle at `x_c` (tie-break for
  edge/vertex hits: lowest triangle index); `cosang = abs(dot(u, n))`.
- `r_x = r(T)` — frustum radius at the exit crossing.
- `R_b = BALL_FACTOR * r_end` — designed tip/foot ball radius.
- `d` — absolute exact signed distance from `p0` to the surface.

## Normative formulas

Shaft (capped ellipse; the transit cap `2*r_x*T` is the true grazing-chord
bound and replaces any cosine floor):

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

Anchor area (PROJECTED, angle-free — never credit the `1/cos` ellipse
inflation as strength):

    A_anchor = A_ball + ( pi * r_x^2  if T > TOL_MM else 0.0 )

## Legality predicate (per designed embed end)

LEGAL iff ALL of:

1. **Scar cap:** `A_patch <= A_max`.
2. **Scar dimension:** `L_scar <= L_SCAR_MAX_MM`.
3. **Anchor floor:** `A_anchor >= pi * r_end^2` — the fuse/neck floor: the
   break must happen at the tip neck. Also catches a detached tip (`T = 0`
   and `d >= R_b` gives `A_anchor = 0`, ILLEGAL).
4. **Crossing topology (PRINT-FLOOR amended below):** only the tip-anchored
   interval is exemption-eligible. Any interior interval not anchored to a
   designed embed end (mid-span graze, re-entry after the tip interval exits,
   capsule-edge crossing) is a transit and ILLEGAL, subject to the
   PRINT-FLOOR moot downgrade. Both-ends-embedded members apply this per end
   independently.

There is no depth cap on the tip-anchored interval itself (TRUNCATION
amendment, below): depth is unlimited and the interval is truncated at the
opposite face. A designed embed end never exits the model and never creates a
patch on a face other than the one it entered — there is no backside scar.
Long grazing chords stay bounded by rules 1–2 (`A_shaft` grows as
`2*r_x*T`).

## PRINT-FLOOR amendment (owner-ratified 2026-07-24)

Amends rule 4 only; rules 1–3 are untouched.

A rule-4 unanchored transit is downgraded from illegal to **moot** iff its
measured antipodal crossing chord (`feature_thickness_mm`: the entry-to-exit
distance along the offending ray) is `< PRINT_FLOOR_CHORD_MM` (0.08 mm). No
new geometry is computed for the moot test — it reads a quantity both
implementations already produce, so engine/census parity stays bit-testable.

Member-level rule (both implementations, identical):

- A member is **exempt (moot)** iff it has at least one rule-4 crossing and
  ALL of its rule-4 crossings are moot.
- A member is **non-exempt** iff it has at least one FIRM illegal crossing: a
  rule-4 unanchored transit whose chord `>= PRINT_FLOOR_CHORD_MM`, OR a
  SCAR-illegal tip interval (rules 1–3 failure — never moot-eligible).

Reporting and gating: moot transits are still counted and reported in a
separate auditable bucket alongside the intact raw per-crossing records; the
hard gate fails only on the non-exempt population.

Rationale (owner-ratified): chords `< 0.08 mm` are 2.6–4.1 px foliage
knife-edges that cannot print as coherent material; the crossing chord is
`>=` the true wall thickness, so exempting on the chord is conservative; 0.08
mm sits a 20 µm guard band above the measured population minimum (0.0502 mm).

## TRUNCATION amendment (owner-ruled 2026-08-22)

Amends the no-depth-cap paragraph only. Rules 1–4, the PRINT-FLOOR amendment,
and every normative constant and formula above are untouched.

Authority — the owner's ruling, quoted verbatim, is the whole of this
amendment:

> "A seat should be allowed to go as deep into the model as it likes, but it
> should not come out the other side of the model so if the seat wants to go
> deeper than the thickness of the model, it should get truncated at the
> opposite face — it should not create a scar on the backside because that
> provides no additional support so there's no need for it to extend beyond
> the model anyway."

Normative statement (both implementations, identical):

- Depth on the tip-anchored interval stays unlimited. No cap, no threshold,
  no new constant.
- The interval is clamped at the model's opposite face along the seat's own
  line: `depth <= interval`, where the interval is the measured penetration
  interval the implementations already produce. The clamp's right operand is
  a measurement, so there is no tolerance term.
- A designed embed end never exits. There is no backside patch and no
  backside scar, so rules 1–3 are graded at the entry face only.
- Punch-through along the seat's own line is therefore impossible by
  construction, not by check.

**What the clamp does not bound.** The clamp is scalar and acts along one
direction — the seat's own line. It does not bound the buried ball's lateral
reach at oblique incidence, at either end of the member: at entry incidence
`cosang`, a ball of radius `R_b` driven to depth `h` reaches
`h + R_b * (1 - cosang)` below the entry face, and where that exceeds the
perpendicular wall thickness the anchor spans the wall rather than capping a
half-space. Rules 1–3 price the joint at the entry plane only; nothing in
this spec prices the wall around such a plug. This is open follow-on work
(see `docs/plans/`); the amendment does not close it.

## Implementation

Both implementations must apply every rule and constant above identically:

- **Engine** (`crates/plinth-metrics/src/scar.rs`): computes every input via
  exact projection and exact signed distance, and never applies an area cap
  outside this predicate.
- **Census** (independent Python): computes `cosang`/`d`/`T` from its own
  independent exact machinery; it never calls the engine kernel.

Known approximation (accepted): patch areas use a locally-planar surface
model; the `2*r_x*T` and `T+2*r_x` caps bound the curvature error. A future
exact patch extraction may tighten, never loosen, `A_patch`.

## Evidence

Derivation history, the panel area analysis, and the population measurements
this policy's constants were tuned against live under
[`docs/evidence/`](../evidence/) and are supporting rationale, not a second
source of law — if evidence and this contract differ, this contract governs.
