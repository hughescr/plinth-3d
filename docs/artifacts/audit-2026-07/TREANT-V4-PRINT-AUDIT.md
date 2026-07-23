# TREANT V4 — FINAL PRE-PRINT AUDIT REPORT

**Subject:** `treant_v4_supports.chitubox` (Weeping Treant v1.2, engine master f02b7af, lift_mm=3.0)
**Date:** 2026-07-22
**Inputs:** 3 resin research reports, 1 literature review, 4 physics audits + adversarial verifier, volume audit + verifier, unrouted-contact forensics + verifier, performance/port plan. One expected input — the Codex review of the port plan — never arrived (see Section 8).

A note on language used throughout: **"green state"** means the resin as it comes off the printer, before you put it in the UV cure box — it is much softer and weaker than the final cured part. **"Peel force"** is the suction-like pull the print feels every single layer, when the build plate lifts and the fresh layer separates from the film at the bottom of the vat. It is the force most likely to snap a support mid-print.

---

## 1. VERDICT: YELLOW — print it, but do the short checklist in Section 6 first

No auditor or verifier found anything that predicts this print will fail: every hard gate in the engine passes, and even when we re-run the support-sizing math with real measured peel forces (which are much harsher than the engine assumed), every support still comes out with strength to spare — just far less spare than advertised. The three things keeping this at YELLOW instead of GREEN: **(1)** the engine assumed a peel force 4–7× lower than published measurements, so the advertised safety factor of 8 on support tips is really about 1.7–3 on the large-area tips (still above 1, but the margin is mostly illusory — mitigate by printing with a slow lift speed); **(2)** two of your three resins (Elegoo and Nova3D) have **zero published mechanical data anywhere**, so for them we are flying on assumptions — start with the SUNLU, which has a real datasheet; **(3)** there are 2–4 genuinely under-supported spots on the model (one stranded overhang on an upper-right branch, plus a crown-tip cluster) that deserve manual supports in ChituBox before you press print. RED was never on the table: nothing measured predicts an in-print failure at the delivered geometry.

---

## 2. Resin data — three resins side by side

Bottom line first: **only SUNLU published real numbers.** Elegoo and Nova3D publish nothing measurable for these exact products, and — trap for the unwary — web searches confidently return numbers for *different* sibling products under nearly identical names.

| Property | SUNLU Water-Wash ABS-Like | Elegoo Water-Washable ABS-Like | Nova3D Tough Washable (ABS-like) |
|---|---|---|---|
| Source | Official TDS (ISO methods), doc SL-TE-WI-096 | **NOT PUBLISHED** — nothing found | **NOT PUBLISHED** — nothing found |
| Tensile strength (pull-apart), post-cured | 26 MPa | NOT FOUND | NOT FOUND |
| Elongation at break (stretch before snap) | 24% | NOT FOUND | NOT FOUND |
| Flexural strength / modulus (bending) | 34 MPa / 956 MPa | NOT FOUND | NOT FOUND |
| Impact (Izod, notched) | 120 J/m | NOT FOUND | NOT FOUND |
| Hardness | Shore 77D | NOT FOUND | NOT FOUND |
| Density liquid / cured | 1.12 / 1.22 g/cm³ | NOT FOUND | NOT FOUND |
| Viscosity | 400–800 mPa·s @25°C | NOT FOUND | NOT FOUND (page says "low viscosity", no number) |
| Heat deflection / glass transition | 47°C / 45°C (low — soft in a hot car) | NOT FOUND | NOT FOUND |
| Volumetric shrinkage | 8.8% (ISO 294) | NOT FOUND | unverified "2.5%" Amazon bullet, may belong to a different SKU — do not trust |
| Recommended layer / exposure | 0.05mm; mono LCD ~2–4s (TDS shows "24s", almost certainly a lost hyphen); bottom 10–60s ×5–10 layers | NOT FOUND | NOT FOUND |
| Lift speed | 60–120 mm/min | NOT FOUND | NOT FOUND |
| Post-cure recipe | "3–5 min in a curing chamber" — no wavelength/power given | NOT FOUND | NOT FOUND |
| **Green-state (pre-cure) properties** | **NOT PUBLISHED** | **NOT PUBLISHED** | **NOT PUBLISHED** |
| Independent third-party test data | None found | None found | One qualitative drop-test review (survived 62" drops; beat SUNLU ABS-like in that test) — no numbers |

Things that could NOT be found for any of the three, anywhere: green-state strength (what our support math actually needs), water absorption numbers, warping data, and any independent lab verification of manufacturer claims.

Do-not-confuse warnings (all three research files hit these):
- Elegoo's **non-water-washable** "ABS-Like V3.0" has a widely-circulated spec sheet (tensile 30–52 MPa, Shore 75D, etc.). Those numbers are for a different resin chemistry. Every AI search summary encountered conflated the two.
- SUNLU sells a non-water-wash "ABS-Like Resin" sibling with different numbers (tensile 28, HDT 50°C). Also not your resin.
- A "Water Washable Resin TDS" PDF floating on Shopify's CDN is from **3D META**, an Australian reseller — not Elegoo, not Nova3D, not ABS-like.
- The SUNLU TDS itself contradicts its own marketing (its "Cleaning method" field says to use ≥95% alcohol — almost certainly a copy-paste template error), and its PDF metadata says "KINGFA" (a large resin OEM), suggesting a white-label formulation.

**Practical consequence:** because green-state data does not exist for any of the three, the best estimate we can build is: SUNLU post-cure tensile 26 MPa × the measured green-to-cured ratio from the one vendor who publishes both (Formlabs Tough 2000: green is 55–63% of cured) ≈ **14–16 MPa green tensile**. The engine assumed 10 MPa — comfortably conservative. The same exercise for bending stiffness gives ~0.23–0.45 GPa green, and there the engine's 0.5 GPa is slightly **optimistic** (see Section 3).

---

## 3. Physics model assessment

### What the engine actually models
One force only: **peel**. A flat pressure (3,000 Pa) over each support's share of the overhanging area, converted into (a) pull-apart tension for sizing support tips and trunks, and (b) a sideways bending load for slender shafts (a "cantilever" — a beam fixed at one end, free at the other, like a diving board). On top of that: a pipe-network bookkeeping rule (parent radius⁴ ≥ sum of children radius⁴) and a "worst unsupported span during the build" check. **Not modeled at all:** gravity/self-weight (assumed cancelled by buoyancy — never checked), resin density (no constant exists in the code), viscosity, and suction from sealed hollow cavities.

### What held up under adversarial verification
Everything factual. The verifier reproduced **every single load-bearing claim** from all four physics audits at its cited file and line — formulas, constants, dimensional consistency — with zero refutations. The hard verification gates on the delivered file (clearance, connectivity, printability, Murray, load) all report 0 violations and reproduce exactly. The audit's factual spine is sound. What the verifier *overturned* was framing, not facts — three items below.

### Confirmed issues, ranked

1. **The assumed peel force is 4–7× too low.** `config.py:23` sets `peel_pressure_pa = 3000` and calls it the middle of a plausible range. It is not: two independent instrumented measurements — a community load-cell test on FEP/ACF films (12–13 kPa) and a peer-reviewed lab rig (15.6–19.6 kPa) — converge at **13–20 kPa**, and both were taken at the same 1–2 mm/s lift speed your resin class recommends. What it means for the print: support tips sized by the formula carry a real safety factor of **1.7–3.0, not 8** (i.e., they'd need to see roughly double-to-triple the measured worst-case force before snapping — safe, but not by the margin the engine reports). Severity: HIGH for honesty/margin, not a predicted failure — every computed case stays above 1.
2. **The "most tips are safe because they're floor-clamped" reassurance is itself peel-dependent.** Most interior tips are clamped to the 0.2mm minimum radius, which is genuinely oversized for their small load share (real safety factor 4.5–11 even at measured peel). But the area threshold at which a tip outgrows the floor drops from 52 mm² (at the assumed 3 kPa) to **8–12 mm² at measured peel** — right at the typical per-tip area. So the risk concentrates in **large-area, formula-governed tips**: sparse islands, minima contacts, and single-contact islands. `verify.py:172-189`'s load gate cannot catch this, because it checks against the same 3 kPa — its "0 violations" is blind to peel underestimation by construction. Severity: HIGH (same root cause as #1).
3. **Green bending stiffness is assumed slightly too stiff — in the unsafe direction.** `config.py:30` sets 0.5 GPa and the audits called it "conservative." The verifier sign-corrected this: the bending math needs green *flexural* modulus, which measures 0.23–0.45 GPa. Result: slender shafts up to ~18–22% thinner than the model's own logic would demand, softened by a fourth-root. Severity: MEDIUM; stacks with #1 on leaning members.
4. **The verifier gate for unsupported minima uses a criterion the codebase itself already ruled "too loose."** `verify.py:214` passes any low-point with a support within a flat 1.0mm ball, while the engine's own demand side (`engine.py:350-363`) was already fixed to a strict 0.2mm reach. The reported "28 unsupported minima" is therefore an **undercount** against the code's own standard. Severity: MEDIUM — verification honesty, not print safety (the strict side already adds the contacts).
5. **The beam formula over-builds interior braced spans.** `transient.py:602-607` applies the diving-board (cantilever) formula even to spans braced at both ends, which are 8–64× stiffer. Over-radiuses those members by up to 2.8×. Severity: LOW for safety (it errs fat), but it inflates resin volume.
6. **The lateral drag force on shafts is a pure guess.** `config.py:50`, 0.01 N flat for every member regardless of size. It is the *only* sideways load on vertical members (most trunks). No literature figure exists to calibrate it — this needs a real-print measurement, not resin data. Severity: unknown direction; flagged for calibration.
7. Housekeeping items, all confirmed and all harmless today: three un-reconciled load→radius exponents get max()'d together (cco.py:127, placement.py:130-153, transient.py:461); the Murray check validates its own bookkeeping, not independent physics (verify.py:143-169); the "geometric mean" docstring is wrong arithmetic (moot — the whole band was 4–7× low anyway); no printer pixel-pitch constant or thin-feature gate exists (joints.py:69-77, comment only); several tolerances are flat millimetres instead of layer-derived, against the charter's own 2026-07-22 ruling (config.py:354, joints.py:51-60); and `r_bend` (curved-member sizing) is specified but unimplemented — correctly dormant, since this run has no curved members.

### Where real resin numbers make our assumptions conservative vs optimistic

| Constant | Engine assumes | Best real estimate | Direction | Factor |
|---|---|---|---|---|
| Peel pressure | 3,000 Pa | 13,000–20,000 Pa (measured, two independent rigs) | **OPTIMISTIC** (undersizes) | force 4.3–6.7× low → radii 2.1–2.6× low where formula governs |
| Green tensile strength | 10 MPa | ~14.3–16.4 MPa (SUNLU 26 × 55–63% green ratio) | conservative | ~1.5× safe — partially offsets the peel optimism |
| Green bending modulus | 0.5 GPa | 0.23–0.45 GPa (green flexural) | **OPTIMISTIC** | shafts 1.03–1.22× thin (4th-root damped) |
| Net tip safety factor | nominal 8 | vs measured loads | eroded | real ~1.7–3.0 off-floor; 4.5–11 on-floor |
| Shaft drag | 0.01 N | no measurement exists | unknown | uncalibratable from resin data |
| Density / viscosity | absent | 1.12 g/cm³ / 400–800 mPa·s (SUNLU) | inert | zero code path — cannot move any output |

---

## 4. The 12.84 vs 9.99 ml question — answered by measurement

**Your hypothesis was right, both halves of it, and it's the dominant effect.** The engine's 12.84 ml is a sum of every rod and ball counted as if it stood alone (`tree.py:515-537` — no overlap check of any kind). Where rods meet at a junction, or a ball sits on a rod, the shared space gets counted twice or more.

Measured (voxel rasterization of the exact 1,625 members + 1,102 balls behind the 12.8396 ml figure, converged across five grid resolutions and **reproduced by a second, independently written voxelizer** — seven runs total, all within 0.4%):

| Quantity | ml | Notes |
|---|---|---|
| Engine's naive sum | 12.840 | reproduced bit-for-bit from the run pickle |
| **True union (actual printed solid)** | **8.737** | measured, converged |
| → double-counted overlap | 4.103 (**32.0%**) | |
| → of which rod-rod junctions | 3.080 (75% of overlap) | **no mitigation exists in code** |
| → of which ball-rod | 1.023 (25%) | balls already have a 72.5% "enclosure diet" filter; even survivors are 76% redundant |
| + foot pads (53) and raft bridges (52), measured from the delivered file | +0.206 | never included in 12.84 at all |
| = composite true-volume estimate | ≈ 8.94 | |
| ChituBox reports | 9.99 | |
| **Residual, unexplained** | **≈ 1.05 (11.7%)** | honest open item |

So: overlap double-counting explains **~63% of the 12.84→9.99 gap**; ChituBox sits between our naive sum and our measured union, much closer to the union — exactly the signature you'd expect if ChituBox slices the geometry into its true union (an inference from matching numbers, not something verified inside ChituBox). The remaining ~1.05 ml is genuinely unexplained; candidates (untested, listed only as candidates): ChituBox counting the raft/base, slicer quantization, and support geometry embedded inside the model that we didn't clip.

**Honest-reporting recommendation:** stop presenting the sum-of-parts as "the" volume. Report the voxelized union as the headline (it costs 3.5 seconds for this entire forest), keep the fast analytic sum as a clearly-labeled internal number (`sum_of_parts_ml`), and include the pads/bridges the emit stage adds — today they are invisible to the engine's own report. Budget ~10 ml of resin for this print, not 12.8.

---

## 5. Unrouted contacts — your three sub-questions, answered

Counts first: **195 unrouted contacts** total. The quality-bearing subset is **49 residual points** = 28 unsupported minima (low-points the router left uncovered) + 21 uncovered island births (moments where a piece of geometry first appears mid-air on a layer). Plus 144 "uncovered coverage islands," which are the face-count view of the same under-supported regions. Every number below was independently recomputed by an adversarial verifier with **zero numeric disagreement**.

One caveat up front: the source STL became unreadable to every process in the session (macOS Documents permission — confirmed real by two independent agents, with and without sandbox), so the airtight mesh-based tests (ray-visibility for "internal", groove clearance, reach re-runs) could not run. Everything below is pickle-derived and proxy-based, honestly labeled.

**Q1 — Are they weird internal parts?** No evidence of that; unlikely. The 21 island births have footprints of 0.001–0.054 mm² (median 0.0045 mm²) — specks 0.03–0.23 mm across, consistent with bark texture and fine canopy detail, not enclosed internal structure. (A true internal-cavity contact would not show up as a sub-0.05 mm² birth.) Pending the ray test for airtightness.

**Q2 — Are they nested in grooves / already held by a neighbor?** Yes, most of them. 19/49 residuals have a live support within 0.5 mm, 28/49 within 1 mm, 43/49 within 2 mm. For the births specifically: 15/21 within 0.5 mm. The router declined a dedicated support because one is already essentially touching the spot (and/or a second strut can't fit without colliding). Effectively redundant, low real risk — with the disclosed caveat that "a member within 0.5mm" is a proximity proxy, not mesh-confirmed support of that exact face.

**Q3 — Genuine reach gaps?** Yes, a small tail: **6 minima have no support within 2 mm**, led by a 10.65 mm stranded overhang at the top-right branch. These are the real work items.

### Top-10 risk spots (coordinates are in the engine's centered frame: (0,0,0) is the model's bounding-box center; +x right, +y front, z −64 bottom → +64 crown tip; "height" = mm above the support plate)

| # | Type | Where (x, y, z) | Height | Nearest support | Plain-language location | Worth a manual support? |
|---|---|---|---|---|---|---|
| 1 | Birth | (−37.6, 12.4, −20.0) | 47 mm | 1.03 mm | Lower trunk, left side, front — the largest floating speck (0.054 mm², 28 layers before it merges) | **Optional — yes if easy** |
| 2 | Minimum | (32.1, −12.1, 43.7) | 111 mm | **10.09 mm** | Upper branch, right side, back — **the one genuinely stranded overhang** | **YES — priority 1** |
| 3 | Birth | (17.7, 12.6, 23.6) | 90 mm | 1.48 mm | Mid-canopy branch, right, front — small speck, 40-layer float | Optional |
| 4 | Minimum | (36.2, 3.9, 52.7) | 120 mm | 5.57 mm | Crown tip, right — unsupported near the very top | **YES — priority 2 (one support covers #4+#5)** |
| 5 | Minimum | (36.3, 3.8, 52.7) | 120 mm | 5.50 mm | Twin of #4, adjacent | (covered by #4's support) |
| 6 | Birth | (−7.6, 10.0, 52.6) | 120 mm | 1.87 mm | Crown tip, center-front — tiny speck but longest float (49 layers) | No — speck |
| 7 | Birth | (35.5, −1.2, −61.6) | 5 mm | 1.09 mm | Base/roots, right, just above the raft | No |
| 8 | Minimum | (−16.0, −1.0, −18.2) | 49 mm | 4.82 mm | Lower trunk, left flank | Optional |
| 9 | Minimum | (11.6, −19.0, 32.2) | 99 mm | 3.80 mm | Upper branch, right, back | Optional |
| 10 | Birth | (9.3, −12.9, 1.9) | 69 mm | 0.39 mm | Mid-canopy, right, back — a support is already practically touching it | No |

**What this means for pressing print:** the residuals are dominated by micro-detail that either merges within 1–2 mm of build height or already has a neighbor touching it. Worst realistic outcome for the specks is a small surface blemish, not a chunk detaching into the vat. Add **2 manual supports minimum** (the #2 stranded overhang, and one in the #4/#5 crown-tip cluster), ideally 2 more (#1, #3), and this dimension is closed. One honest unknown: a few of the 144 coverage islands are large (up to 22,467 faces) and *could* be broad genuine overhangs rather than grown-from-speck detail — distinguishing them needs the currently-unreadable mesh. If the STL becomes readable, that's the first check to run.

---

## 6. Recommended pre-print actions (in order)

1. **Use the SUNLU Water-Wash ABS-Like for this first print.** It is the only one of your three resins with a real datasheet; the whole physics cross-check in this audit is calibrated against its numbers. Save Elegoo/Nova3D for after a validated benchmark print.
2. **Set lift speed to the slow end: 60 mm/min (1 mm/s), lift height 6–10 mm.** Measured peel force scales roughly linearly with lift speed — this is your single biggest practical lever against the thin real safety margins in Section 3. Retract 120–180 mm/min per the TDS.
3. **Layer height 0.05 mm** (matches both the TDS recommendation and the engine profile the supports were sized for — do not change it without re-running the engine). Bottom layers 5–10; normal exposure: start ~2.5–3s on a mono LCD and calibrate; treat the TDS's "24s" as the misprint it almost certainly is.
4. **Add manual supports in ChituBox** (medium tip ~0.4 mm): one at the stranded overhang, upper-right-back branch, ~111 mm above the plate (centered coords 32.1, −12.1, 43.7); one at the right crown tip ~120 mm up (36.2, 3.9, 52.7 — covers its twin); optionally two light supports at the lower-left-front trunk speck (−37.6, 12.4, −20.0) and the right-front mid-canopy speck (17.7, 12.6, 23.6). To locate them: they are positions relative to the model's bounding-box center — half-way point of each axis — which is what ChituBox's view navigates by; heights above plate are given for each.
5. **Do not re-orient the model and do not re-run auto-supports** — the delivered support forest passes all gates in the orientation it was built for; add supports only.
6. **If the model is hollowed, confirm at least two drain holes** (one near the plate, one opposite). No sealed-cavity analysis was in this audit's scope, and literature confirms sealed cavities are the one catastrophic-failure mode nobody has good numbers for. If it's solid, skip.
7. **Environment per TDS:** 25–30°C ambient, humidity < 60%. Cold resin = higher viscosity = higher peel force.
8. **Budget ~10 ml of resin, not 12.8** — the engine's number double-counts overlap (Section 4). ChituBox's 9.99 is the closer figure.
9. **Watch the first 20–30 mm of build height** (raft, roots, lower trunk births), and after the print, expect tiny blemish-scale imperfections at the speck locations in the Top-10 table — cosmetic, not structural.
10. **Wash fast with water, post-cure 3–5 min** in your cure chamber per the TDS. Note the resin's heat tolerance is low (HDT 47°C) — keep the finished mini out of hot cars.

---

## 7. Config/code change PROPOSALS (nothing was changed during this audit), ranked by print-safety impact

1. **Raise `peel_pressure_pa` from 3,000 to ~13,000–15,000 Pa** (config.py:23) and re-run the engine. This is the top correctness lever: it re-aligns the load model with two independent measurements. Expect large-area tips and tension-floored trunks to fatten and reported volume to rise. (Pair with #3 so the honest volume number doesn't inflate misleadingly.)
2. **Lower `modulus_green_gpa` from 0.5 to ~0.35** (config.py:30) — the bending solve needs green *flexural* modulus (measured 0.23–0.45 GPa), and the current value is on the stiff/optimistic side.
3. **Report true union volume as the headline** (tree.py:515-537): relabel the current figure `sum_of_parts_ml`, add the 3.5-second voxel union, and include emit-stage pads/bridges (invisible to the engine's report today).
4. **Tighten `verify.minima_report` to the strict criterion the engine already uses** — replace the flat 1.0mm 3-D ball at verify.py:214 with the 0.2mm-xy + layer-derived-z test its sibling gate uses. The verifier gate should never be looser than the builder.
5. **Add a printer XY pixel-pitch config constant and a formability gate** (today: comment-only ~0.05mm at joints.py:69-77, unsourced). Source the Saturn 4 Ultra's actual pitch; nothing currently catches an unresolvable-thin feature.
6. **Size island-birth contacts from the birth's own measured area** (`b["area_mm2"]`, perception.py:468) instead of the 1.5mm face-ball proxy (engine.py:551-570) — the code computes the right number and then discards it.
7. **Use a both-ends-restrained beam model for braced interior spans** (transient.py:602-607) — pure resin-volume saving (up to 2.8× over-radius today), no safety cost; do after #1 lands.
8. **Express resolution-class tolerances in layers, not mm** (config.py:354 `minima_eps_mm`; joints.py:51-60 tolerance block), per the charter's own 2026-07-22 ruling. No-op at 0.05mm; prevents silent breakage at other layer heights.
9. **Add a resin density constant and an explicit buoyancy check** — today "gravity ~cancelled by buoyancy" is prose, and density (1.12/1.22 g/cm³ for SUNLU) has zero code path. Low urgency; enables mass reporting and validates the charter assumption.
10. Calibration wishlist (needs prints, not code): measure `shaft_drag_force_n` from observed ribbing; log which sizing term wins per member to quantify how much of the forest is floor-bound; re-verify the resin's green strength once any manufacturer publishes it.

---

## 8. Task-2 roadmap — parallelization/port plan (execute only after the print is validated)

Current state: 193.8s single-core run on the 16-core M3 Max (~15% CPU). Measured split: ~30% mesh preprocessing (of which layer-slicing for island births alone is 23.5% and embarrassingly parallel), ~70% the fix-pass loop, which profiling shows is dominated by a genuine algorithmic bug-shaped hotspot: the rewire optimizer re-sums the volume of the whole forest for every candidate move (47% of wall on the profile fixture), likely causing it to hit its 30s time cap and leave better layouts unfound — meaning this fix may improve print quality, not just speed.

The plan, in brief (full detail in `port_plan.md`):
- **Phase 0 (0.5d):** byte-identical hash harness — every later change gated on producing the exact same output file.
- **Phase 1 (4–5d, biggest ROI):** single-core fixes — incremental volume accounting, de-numpy the 3-vector hot loops, memoize the delicacy field, dirty-set revalidation. Target ≥2.5× overall.
- **Phase 2 (5–6d):** process-parallel the independent stages (island births across z-chunks, prepare tasks, per-island load partition with deterministic per-island seeds). Target prepare ≥4×; combined run ≲45s.
- **Phase 3 (3–12d/tactic, highest risk):** parallelize the greedy core via speculative candidate evaluation with fixed-order commit. Target fix-pass ≥3×.
- **Phase 4 (optional, 3–5wk):** Rust kernel (PyO3) for the arithmetic hotspots only — not a full port. Full Rust/C++ rewrites are explicitly recommended against while the engine is still under R&D churn.
- Realistic end-to-end ceiling: **~6–8× (~31s)** without Phase 4.
- Determinism is the load-bearing design constraint: the plan commits to "Reading B" — parallel output must be byte-identical to the shipped engine's own serial mode, with any baseline-changing improvement re-passing your print-approval gate once. Sub-rules: processes not threads (the code seeds the global random generator), all reductions in fixed index order, and any incremental-volume optimization validated byte-for-byte because a last-decimal-place drift can flip a rewire accept/reject and change the whole topology.

**Codex's dissent: NOT AVAILABLE.** The handoff said all chains returned, but `port_plan_codex_review.md` does not exist in the audit directory (verified twice, plus a scratchpad-wide search). I am not going to invent its contents. The port plan pre-registered the three questions it asked Codex to attack — (1) Reading A vs B of "bit-identical", (2) whether one-time print re-approvals of a changed baseline are acceptable, (3) whether the engine is feature-frozen enough for a native kernel — and those, plus the Amdahl estimate and the "hot-kernel vs full-port" call, are exactly where an independent challenge would land. Recommend re-running the Codex review before committing Phase 3+ effort; Phases 0–2 are low-risk enough to start without it.

---

## Appendix: source documents (all in this directory)

resin_sunlu.md, resin_elegoo.md, resin_nova3d.md — resin research · lit_green_peel.md — literature review · physics_load_sizing.md, physics_contacts_tips.md, physics_printability.md, physics_transient_peel.md — physics audits · verify_physics.md — adversarial physics verification · volume_audit.md + verify_volume.md + voxel_result_pitch*.json + pads_bridges_result.json — volume measurement · unrouted_forensics.md/.json + verify_forensics.md — contact forensics · port_plan.md — Task-2 plan.
