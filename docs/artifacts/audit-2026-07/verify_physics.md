# Adversarial verification of the physics-model audit

Verifier role: independent, did not write the four reviewer reports or the
engine. Goal: break the audit. Method: (1) reproduce every load-bearing
reviewer claim at its cited file:line in the worktree
`.../scratchpad/wt-emit-final/scripts/support_placement/`; (2) run the
safety-critical formulas end-to-end with real numbers from the resin/lit
research; (3) cross the resin-sensitivity items against measured data and
state conservative-vs-optimistic + factor; (4) classify.

TL;DR verdict: **No BLOCKER-for-print found. All hard gates (0) and every
load-bearing code claim reproduce exactly.** The audit's factual spine is
sound. BUT the reviewers collectively mis-framed the single most important
resin-sensitivity lever: `peel_pressure_pa = 3000 Pa` is not the mid-point of
a "plausible 1-5 kPa range" — the peer-reviewed + community-measured peel
stress is **12-20 kPa**, so the engine's load assumption is **4-7x too low
(optimistic)**, which quietly collapses the advertised tip SF=8 to a real
~1.7-3.0 for formula-governed tips. That is the top should-fix. A second
sign error: `modulus_green_gpa = 0.5` is optimistic, not "conservative", for
its bending use. Craig's volume double-count hypothesis is confirmed
(necessarily >= 2.85 ml / >= 22%).

---

## 1. Spot-check of every load-bearing claim (all reproduce)

| Claim (reviewer) | Cited loc | Reproduced? | Verified value |
|---|---|---|---|
| `peel_pressure_pa=3000.0` | config.py:23 | YES | `peel_pressure_pa: float = 3000.0` |
| `tensile_green_mpa=10.0` | config.py:29 | YES | line 29 |
| `modulus_green_gpa=0.5` | config.py:30 | YES | line 30 |
| `tip_safety_factor=8.0` | config.py:34 | YES | line 34 |
| `tip_r_min/max=0.2/1.0` | config.py:39-40 | YES | lines 39-40 |
| `shaft_drag_force_n=0.01` | config.py:50 | YES | line 50 |
| `max_deflection_mm=0.15` | config.py:54 | YES | line 54 |
| `max_unbraced_shaft_r_mm=0.9`,`min_shaft_r_mm=0.3` | config.py:60-61 | YES | lines 60-61 |
| `min_trunk_r_mm/max=0.8/2.4`,`min_shoot_r_mm=0.35` | config.py:220-223 | YES | lines 220,221,223 |
| `minima_connect_layers=2.0`,`minima_eps_mm=0.02`,`minima_contact_radius_mm=1.0` | config.py:351,354,359 | YES | lines 351,354,359 |
| tip fuse `r=sqrt(F*SF/(pi*sigma))`, `F=area*peel` clamped [0.2,1.0] | placement.py:113-122 | YES | exact |
| `_r_tension_mm` same, unclamped | cco.py:118-128 | YES | exact |
| `_shaft_r_for_L` `r=(4FL^3/3E pi delta)^.25`, angle term `+peel*pi*r^2*tan`, 6-iter | placement.py:130-153 | YES | exact; returns `max(.,min_shaft_r_mm)` |
| `max_unbraced_len_mm` closed-form inverse | placement.py:156-168 | YES | exact |
| Murray propagate `r=(m.r0^4+r_up^4)^.25` then `max(r_murray,r_defl,r_tension,kind_floor)` | cco.py:654,660 | YES | exact; B2 fix (r0 not max(r0,r1)) present cco.py:626-645 |
| `murray_report` `r_parent^4>=sum(child^4)*(1-tol)`, tol=0.05, only >=2 children | verify.py:143-169 | YES | uses parent.r0, child.r1 |
| `load_report` `sigma=F/(pi r^2)`,`SF_real=sigma_t/sigma`, flag<SF | verify.py:172-189 | YES | exact; uses SAME peel=3kPa |
| `minima_report` flat 3D gap `if gap>minima_contact_radius_mm` | verify.py:214 | YES | 1.0mm 3D Euclidean |
| `r_transient_mm=_shaft_r_for_L(worst_span,lean)*max(N,1)^0.125` | transient.py:454-461 | YES | exact |
| `resize_forest r_req=max(r_murray,r_tension,r_deflect,r_trans,kind_floor)` NO r_bend | transient.py:598-610 | YES | exact; r_deflect uses worst_gap_3d cantilever |
| `volume_report` frustum `pi/3*L*(r0^2+r0r1+r1^2)/1000`, ball `4/3 pi r^3/1000` | tree.py:515-537 | YES | exact |
| `MODEL_FOOT_FACTOR=1.4` | joints.py:50 | YES | line 50 |
| `ENCLOSURE_TOL_MM=0.02`, pixel-pitch ~0.05mm comment-only | joints.py:68-77 | YES | no config knob exists |
| `r_bend` absent (only docstring mention) | transient.py:520 | YES | grep: no computed r_bend |
| no density/viscosity constant anywhere | (grep) | YES | confirmed |

Dimensional checks re-derived by hand and confirmed: tension
`N/(N/m^2)=m^2 -> sqrt -> m`; deflection `N*m^3/((N/m^2)*m)=m^4 -> ^.25 -> m`;
inverse-length `m^4*(N/m^2)*m/N=m^3 -> ^(1/3) -> m`. All consistent.
**No reviewer factual claim failed to reproduce -> no BLOCKER on the audit
itself.** Internal consistency check on the authoritative numbers:
by_kind sums to 11.4901 = member_ml exactly; +ball 1.3495 = 12.8396. OK.

---

## 2. Safety-critical formulas end-to-end with real numbers

Real numbers substituted (all cited):
- **Peel stress (areal suction load):** AmeraLabs Mars-4 load-cell, FEP 13 kPa,
  nFEP 13 kPa, ACF 12 kPa (lit_green_peel 1.1); Pan et al. 2017 RPJ, PDMS rig,
  1.56-1.96 N/cm^2 = **15.6-19.6 kPa** (lit 1.1). Both at ~1.5-2 mm/s lift,
  which matches this resin class's recommended lift 60-120 mm/min = 1-2 mm/s
  (resin_sunlu). **Working measured band: 13-20 kPa.** This is exactly the
  quantity `peel_pressure_pa` represents (force per newly-cured cross-section
  during film release), so the comparison is apples-to-apples.
- **Green tensile:** no green datasheet exists for any target resin (all three
  resin_*.md say NOT PUBLISHED). Best estimate = SUNLU water-wash post-cure
  tensile **26 MPa** (resin_sunlu, official TDS) x green fraction **55-63%**
  (Formlabs Tough 2000 measured green/post UTS 63%, modulus 55%, lit 2.1)
  = **~14.3-16.4 MPa green**.
- **Green FLEXURAL modulus** (the property the bending/deflection solve
  actually needs): Formlabs Tough 2000 green flexural modulus **0.45 GPa**
  measured (lit 2.1); SUNLU post-cure flexural modulus 956 MPa x 24-26%
  green-flex ratio (Formlabs) = **0.23-0.25 GPa**. Working band **0.23-0.45 GPa**.

### 2a. Tip-fuse SF, formula-governed tips (arithmetic shown)
Engine sizes so design stress = sigma/SF = 10/8 = **1.25 MPa**. Substituting
measured peel (force scales x peel-ratio) and realistic green tensile:

| peel (Pa) | green tensile | real stress at engine's r | **real SF** |
|---|---|---|---|
| 13000 | 14.3 MPa | 5.42 MPa | **2.64** |
| 13000 | 16.4 MPa | 5.42 MPa | **3.02** |
| 20000 | 14.3 MPa | 8.33 MPa | **1.72** |
| 20000 | 16.4 MPa | 8.33 MPa | **1.97** |

Equivalently the required radius must be **1.63-2.16x fatter** than the engine
computes (`r_true/r_eng = sqrt(peel_ratio * sigma_eng/sigma_green)`). The
"SF=8" headline is real only against the assumed 3 kPa / 10 MPa; against
measured peel it is **~1.7-3.0**. Still >1, so not a predicted failure, but the
margin is largely illusory. **`load_report` (verify.py:172-189) cannot catch
this — it uses the same 3 kPa, so its 0 violations is blind to peel
under-estimation by construction.**

### 2b. Why this is NOT catastrophic: the 0.2mm floor
Most interior contacts are floor-clamped to r=0.2mm (formula only lifts off the
floor above the carried area below). A 0.2mm tip (cross-section 0.1257 mm^2) on
a typical 4mm-spacing Voronoi cell (~13.9 mm^2):

| green tensile | F_fail | peel | F_real | physical SF |
|---|---|---|---|---|
| 10 MPa | 1.257 N | 13 kPa | 0.181 N | 6.9 |
| 10 MPa | 1.257 N | 20 kPa | 0.278 N | 4.5 |
| 14.3 MPa | 1.798 N | 20 kPa | 0.278 N | 6.5 |
| 16.4 MPa | 2.059 N | 20 kPa | 0.278 N | 7.4 |

Floor-clamped ordinary tips keep 4.5-11x even under measured peel — the 0.2mm
removal floor is generously oversized for a ~14 mm^2 cell. **The risk is
concentrated in large-area / off-floor tips**: sparse islands, minima and
island-birth contacts that carry a whole island, and `min_contacts_per_island=1`
cases. Those are governed by the formula and hence by the 4-7x peel optimism.

### 2c. Lift-off area threshold — the contacts_tips reassurance is peel-contingent
`contacts_tips` argued "most contacts floor-clamped, so peel error doesn't
bite." Confirmed at 3 kPa (lift-off area = 52.4 mm^2, well above a 14 mm^2
cell). **But that reassurance is itself a function of the underestimated peel:**

| peel | lift-off area (leaves 0.2mm floor) |
|---|---|
| 3 kPa (engine) | 52.4 mm^2 |
| 13 kPa (measured) | 12.1 mm^2 |
| 20 kPa (measured) | 7.9 mm^2 |

At measured peel the threshold (8-12 mm^2) falls **at or below** the typical
cell (13.9 mm^2 at 4mm; 5.4 mm^2 at the 2.5mm floor) — so a materially larger
fraction of contacts would leave the floor and inherit the 1.7-3.0 real SF.
The "floor saves us" argument is only true because peel is assumed 4-7x low.

### 2d. Deflection: green flexural modulus is optimistic, not conservative
Reviewers (load_sizing #3, transient_peel #1, printability #3) called
`modulus_green_gpa=0.5` the "conservative weak end of 0.5-1.5 GPa", damped by a
4th root. **Sign is wrong for the bending use.** The deflection solve needs
green *flexural* modulus, measured at 0.23-0.45 GPa. Engine 0.5 GPa is
1.1-2.2x *stiffer* than reality -> shafts under-radiused by up to
`2.18^0.25 = 1.22x` (~18% thin). 4th-root damps it, so modest, but it stacks
with the peel optimism on leaning members and is in the unsafe direction.

---

## 3. Resin-sensitivity cross-check (conservative vs optimistic, with factor)

| Constant | Engine | Measured/estimated | Direction | Factor |
|---|---|---|---|---|
| `peel_pressure_pa` | 3000 Pa | 13000-20000 Pa (lit 1.1) | **OPTIMISTIC (undersizes)** | **force 4.3-6.7x low; radius 2.1-2.6x low** |
| `tensile_green_mpa` | 10 MPa | 14.3-16.4 MPa (Sunlu 26 x 55-63%) | conservative | ~1.5x safe (partially offsets peel) |
| `modulus_green_gpa` (bending) | 0.5 GPa | 0.23-0.45 GPa (green flexural) | **OPTIMISTIC** | 1.1-2.2x too stiff -> r 1.03-1.22x low |
| `tip_safety_factor` net effect | nominal 8 | vs measured loads | eroded | **real SF 1.7-3.0 off-floor; 4.5-11 on-floor** |
| `shaft_drag_force_n` | 0.01 N | no measured lateral-whip figure exists (lit: none) | unknown | uncalibratable from resin data — reviewers correct |
| density / viscosity | absent | 1.12 g/cm3 liquid, 400-800 mPa.s (Sunlu) | zero code path | cannot move any output — reviewers correct |

Net for the tip fuse: the ~1.5x conservatism in tensile is swamped by the
4.3-6.7x optimism in peel; product leaves off-floor tips at real SF < 3.
Density (Sunlu 1.12 liquid / 1.22 cured) still has **zero** consumer in the
code, confirming every reviewer's "density is inert as-built" and the
"gravity ~cancelled by buoyancy" charter assumption remains untested.

---

## 4. Verified issue list

### BLOCKER-for-print
**None.** Hard gates (clearance/connectivity/printability/murray/load) all 0
and reproduce; every off-floor real SF computed above stays > 1; the
floor-clamped majority keeps 4.5-11x. Nothing here predicts an in-print
failure at delivered geometry. The items below are margin/quality/accounting.

### should-fix (ranked)
1. **`peel_pressure_pa=3000` is 4-7x below measured 12-20 kPa** (config.py:23;
   lit_green_peel 1.1). Highest-priority calibration lever. Collapses tip SF=8
   to ~1.7-3.0 for formula-governed (large-area) tips (§2a) and drops the
   floor lift-off area from 52 to 8-12 mm^2 (§2c). **This re-frames all four
   reviewers**, who called 3 kPa the mid/geo-mean of a "plausible 1-5 kPa
   range" and rated peel "high sensitivity but unmeasured" — it is now
   effectively measured and the assumed band is itself 4-7x low. Raise
   `peel_pressure_pa` toward ~13-15 kPa and re-run; expect large-area tips and
   tension-floored trunks/shoots to fatten and `member_ml` to rise.
2. **`modulus_green_gpa=0.5` is optimistic for bending, not conservative**
   (config.py:30; lit 2.1/2.2 + Sunlu flex 956 MPa). Real green flexural
   modulus 0.23-0.45 GPa. Sign-corrects reviewer framing; ~18% thin shafts
   worst case, 4th-root-damped. Lower to ~0.35 GPa for the deflection solve.
3. **`verify.minima_report` still uses the loose 1.0mm 3D gap the codebase's
   OWN comments call "too-loose / self-certifying"** (verify.py:214 vs the
   already-fixed demand side engine.py:350-363 which uses the strict
   `ISLAND_ATTACH_REACH_MM=0.2mm`; perception.py:414 warns against reverting).
   The verify gate was left on the old loose criterion, so the reported
   `unsupported_minima=28` is an **undercount** vs the strict standard the
   engine now adds contacts against. Confirms contacts_tips Issue 1 — and it is
   sharper than framed (the codebase itself already ruled 1.0mm too loose).
   Verification-honesty fix; not a print blocker (demand side already strict).
4. **Cantilever formula applied to both-ends-restrained free_gaps**
   (transient.py:602-607). Over-radiuses interior braced spans up to
   64^0.25 = 2.83x vs a fixed-fixed model. Conservative (safe) but inflates
   `member_ml`. Confirms transient_peel Issue B.

### note
- **Volume double-count CONFIRMED (Craig's hypothesis right).** `volume_report`
  (tree.py:515-537) sums each frustum + ball as independent solids, never
  subtracting rod-rod (junction) or ball-rod (joint) overlap. Naive
  members+balls = 12.84 ml; ChituBox slices the union of members+balls+53
  pads+52 raft bridges = 9.99 ml. Since pads+bridges add real union volume,
  the primitive-sum inflation on members+balls alone is **necessarily
  >= 12.84 - 9.99 = 2.85 ml (>= 22%)**, and strictly more once the pads/bridges
  inside 9.99 are netted out. The gap is over-counting, not under-building.
- **r_bend not implemented** (transient.py:610 computes only 4 of charter §6's
  terms; only a docstring mention at transient.py:520). Confirmed dormant — the
  run has no curved members (kinds: stub/trunk/shoot/strut/brace/model_rung).
  Correctly deferred to Phase 5. All three reviewers agree; verified.
- **Three un-reconciled load->radius exponents** max()'d together: tension
  r∝load^0.5 (cco.py:127), deflection sized by span not carried load
  (placement.py:130-153), transient r∝N^0.125 (transient.py:461). Factually
  confirmed (load_sizing Issue 2). Architecture/theory concern, not
  print-safety; whichever term wins locally sets the effective exponent.
- **`murray_report` is a self-consistency check, not independent physics**
  (verify.py:143-169 re-checks the same r^4 convention `_propagate_murray`
  enforces at cco.py:654). Confirmed (load_sizing Issue 3). Documentation note;
  a passing murray_report validates bookkeeping, not a conservation law.
- **`shaft_drag_force_n=0.01N`** single global constant, no size/orientation
  term (config.py:50). Confirmed. No measured lateral-whip drag figure exists
  in the literature, so it cannot be calibrated from resin data — reviewers
  correct. Governs deflection for near-vertical members (peel term -> 0).
- **geometric-mean docstring error**: sqrt(1*5)=2.236 != 3.0 (3.0 is the
  arithmetic mean). Confirmed (load_sizing Issue 4) but **moot** — the entire
  assumed 1-5 kPa band is 4-7x below measured, so which mean was intended is
  irrelevant.
- **No printer XY pixel-pitch config / formability gate** (joints.py:69-77
  comment-only ~0.05mm; Saturn 4 Ultra actual XY resolution not sourced).
  Confirmed (printability P1). At ~0.05mm the 0.35mm strut and 1.334mm max
  trunk are formable, but nothing gates it.
- **`minima_eps_mm` and joints.py tolerances are flat-mm, not layer-derived**,
  against design-v4.md's 2026-07-22 amendment (config.py:354 "Sub-layer scale";
  joints.py:51-60). Confirmed (printability P6, contacts_tips Issue 2).
  No-op at today's 0.05mm; latent at other layer heights.

### Reviewer claims re-framed (none refuted as false; all reproduced)
Every load-bearing factual claim reproduced at its file:line. Three *framings*
are corrected against the measured data:
- (a) All four reports treat `peel_pressure_pa=3000` as a mid-range plausible
  value with "unknown/unmeasured" sensitivity. **Re-framed:** measured peel is
  12-20 kPa; 3 kPa is 4-7x low and optimistic — the top correctness lever, not
  an unknowable one.
- (b) load_sizing/transient_peel/printability call `modulus_green_gpa=0.5`
  "conservative weak end." **Sign-corrected:** for the bending/deflection use
  the relevant green flexural modulus is 0.23-0.45 GPa, so 0.5 is optimistic.
- (c) contacts_tips' "most tips floor-clamped so peel error is harmless."
  **Qualified:** only true at the underestimated 3 kPa; at measured peel the
  lift-off area (8-12 mm^2) meets the typical cell, so the reassurance is
  contingent on the very number that is wrong.
