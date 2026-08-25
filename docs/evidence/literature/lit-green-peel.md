> **Evidence record — non-normative.** Relocated from `docs/engineering-journal/reference/literature/lit-green-peel.md` in the 2026-08 docs restructure; content preserved verbatim below.

# Literature Review: Print-Physics Numbers for Bottom-Up MSLA Resin Printing

Research date: 2026-07-22. Scope: peel/separation force, green-state mechanical properties, support-tip pull-off, suction/cupping forces, and ABS-like water-washable resin quirks. Every entry is tagged with a confidence class:

- **[MEASURED-PEER-REVIEW]** — published in a peer-reviewed journal, real instrumented measurement.
- **[MEASURED-VENDOR]** — manufacturer datasheet or manufacturer-run instrumented test, methodology disclosed.
- **[MEASURED-COMMUNITY]** — hobbyist/community instrumented test (load cell, printer log), methodology disclosed but not peer-reviewed, often small sample size.
- **[RULE-OF-THUMB]** — qualitative community/vendor guidance, no numbers or no disclosed measurement behind the number.
- **[DERIVED-ESTIMATE]** — my own back-of-envelope calculation combining two sourced numbers; not itself a citation of a measurement.

---

## 1. Peel/separation force during layer release

### 1.1 Peel stress order of magnitude — converges around 1–2 N/cm² for small-to-medium cured areas

**[MEASURED-COMMUNITY]** AmeraLabs (maker of resin-printer calibration tools) instrumented a Mars 4 DLP printer's built-in load cell/force log while printing a fixed test geometry through several release films, then fit peel force vs. layer area linearly; the slope ("peel stress") was reported in Pa:
- ACF (300 µm): **12 kPa = 1.2 N/cm²** (lowest of the set tested)
- 50 µm FEP: **13 kPa = 1.3 N/cm²**
- 127 µm nFEP: **13 kPa = 1.3 N/cm²**
- 127 µm FEP and 150 µm FEP: higher than the above but exact kPa values were not stated in the article text extracted (only "highest peel stresses" of the group)
- Source: https://ameralabs.com/blog/are-fep-alternatives-worth-it-acf-vs-nfep-vs-fep-films/ (AmeraLabs blog, own Mars 4 testbed, linear-regression method disclosed)
- Confidence: MEASURED-COMMUNITY (methodology disclosed — printer load-cell log + linear fit of force vs. area — but single printer, single resin, not peer reviewed, exact table not fully recovered from the page).

**[MEASURED-PEER-REVIEW]** Independent cross-check via Pan, He, Xu & Feinerman, "Study of separation force in constrained surface projection stereolithography," *Rapid Prototyping Journal* 23(2), 2017, pp. 353–361 (DOI 10.1108/RPJ-12-2015-0188), full text: https://yayuepan.lab.uic.edu/wp-content/uploads/sites/779/2021/01/8edafea83b2e3d9d65896da53bcf9ab108dc.pdf . This is a real in-situ load-cell testbed (bottom-up projection SL rig with load cell mounted directly on the build platform), not FEP but 2-mm PDMS-coated glass — same suction-force physics, comparable interface class:
- Solid discs/polygons at fixed **314 mm² (3.14 cm²)** cross-section, 1.56 mm/s lift speed: cylinder 6.16 N, hexagon 5.87 N, triangle 5.45 N, star 4.90 N → **1.56–1.96 N/cm²** depending on area/perimeter ratio (force increases linearly with A/L, the area-to-perimeter ratio).
- This **independently converges with the AmeraLabs FEP/ACF number (~1.2–1.3 N/cm²) to within the same order of magnitude (~1–2 N/cm²)** for small-to-medium (≈1–6 cm²) cured cross-sections at moderate lift speeds — reasonable convergence between an academic PDMS rig and a community FEP/ACF rig.
- Confidence: MEASURED-PEER-REVIEW for the Pan et al. numbers themselves (PDMS, not FEP/ACF — flag this material difference when applying to your engine); the cross-validation with AmeraLabs' FEP/ACF number is my own synthesis, not a claim either paper makes.

### 1.2 Dependence on lift speed — approximately linear (peer-reviewed, directly measured)

**[MEASURED-PEER-REVIEW]** Pan et al. 2017 (same paper as above), Section 3.2: five lift speeds tested (0.63, 1.00, 1.56, 2.00, 3.00 mm/s) on a fixed test geometry:
- 2-mm-thick PDMS surface: peak forces **0.72, 0.98, 1.18, 1.54, 2.44 N** respectively (roughly linear in V).
- 4-mm-thick PDMS surface: peak forces **0.35, 0.50, 0.81, 1.01, 1.40 N** respectively.
- Their analytical model derives **F ∝ V** (linear in pull-up speed) and **F ∝ R⁴** (radius to the 4th power) and **F ∝ 1/h³** (inverse cube of the gap/inhibition-layer thickness) for a solid circular cross-section, from a lubrication-flow (Navier–Stokes) model of resin being sucked into the closing gap.
- Confidence: MEASURED-PEER-REVIEW + analytically modeled and experimentally validated in the same paper.

**[MEASURED-PEER-REVIEW]** A second, independent academic source (Iranian J. Adv. Design & Manuf. Tech., 2023) "Investigating the Effect of Separation Speed and Image Cross-Section Geometry on the Separation Force in DLP Method using FEP and PP Polymer Membranes" — this one explicitly used **FEP** (not PDMS):
- "Reducing the cross-sectional area by 1.36% reduced the separation force by 6.5×" (steep, superlinear area dependence, consistent with Pan et al.'s R⁴ law).
- "The separation force rose by 230% [with] an increase of 96× the speed" on FEP; for the PP membrane tested, "the separation force rate increases by 175%" over a comparable speed range — i.e., force is much less than proportional to speed at this extreme range (sublinear at very high speed ratios, unlike the near-linear Pan et al. result at more modest 5× speed range — reasonable, since the linear regime likely breaks down at very high speed/large ratios).
- Source (abstract-level only; full text was behind an access wall so I could not pull table values): https://sanad.iau.ir/journal/admt/Article/706365 and https://www.researchgate.net/publication/374814028
- Confidence: MEASURED-PEER-REVIEW claim per abstract, but I could not independently verify exact table numbers (paywalled) — treat the percentages as reported-by-abstract, moderate confidence on exact magnitudes.

**[MEASURED-PEER-REVIEW]** Broader-context numbers from a review, "A Review of Critical Issues in High-Speed Vat Photopolymerization," PMC10302688 (https://pmc.ncbi.nlm.nih.gov/articles/PMC10302688/):
- DLP with 2-mm PDMS: separation force dropped from **27.5 N to 2.5 N** when lift speed dropped from 5.0 mm/s to 0.2 mm/s (separation time increased ~10×) — again consistent direction (slower = much lower force) but a much larger dynamic range/geometry than Pan et al.'s own numbers, so treat as a different test condition, not the same data point.
- Vibration/tilting mitigations cited in the same review: tilting the plate instead of straight pull-up cuts force ~20%; piezoelectric vibration-assist cuts force ~75%; loudspeaker vibration keeps max force under ~200 gf vs ~500 gf for normal separation (unit note: gf = gram-force ≈ 0.0098 N, so ≈2.0 N vs ≈4.9 N).
- Confidence: MEASURED-PEER-REVIEW (review article synthesizing multiple underlying primary studies; I did not chase every underlying primary citation individually, so treat the exact numbers as "as reported by the review," not independently re-verified against each original paper).

### 1.3 Dependence on cross-sectional area / shape — superlinear, and shape (perimeter) matters, not just area

**[MEASURED-PEER-REVIEW]** Pan et al. 2017: at fixed area (314 mm²), force varies **20–25%** across shapes purely from the area-to-perimeter ratio A/L (cylinder A/L=5 → 6.16 N; star A/L=2.58 → 4.90 N) — "with the same printing area and process parameters, a geometry with a smaller perimeter will cause a larger separation force." Porous/hollowed test geometries (same nominal area, increasing porosity degree p=1.00→1.56) showed force increasing from **~1.0 N to ~1.7 N**, in an "approximately polynomial" relationship with porosity — i.e., more perforated/lattice-like cross-sections at the same footprint area pull *harder*, not easier, likely because more edge/perimeter interacts with trapped resin pockets non-linearly.
- Confidence: MEASURED-PEER-REVIEW, same paper/rig as above.

**[MEASURED-COMMUNITY]** AmeraLabs and multiple vendor blogs note "a notable spike in peel force occurring around 125 mm²" cross-section (per aggregated web-search summarization of several vendor blogs); I was not able to pin this to one primary source with a verifiable number, so treat this specific "125 mm² knee point" as **RULE-OF-THUMB** pending a primary citation, not confirmed measured.

**[RULE-OF-THUMB]** Nearly every vendor troubleshooting page (Anycubic, HeyGears, Raise3D, protoresins.com, mini4speed.com) states peel/suction force scales with cross-sectional area and rises with lift speed, but none of these vendor blog posts I fetched (protoresins.com, mini4speed.com) contained actual numeric data — confirmed by direct fetch; they are qualitative-only. Treat any specific numbers from vendor marketing copy as unverified unless a methodology is disclosed (as AmeraLabs' was).

### 1.4 FEP vs ACF (and nFEP/PFA) — direction of effect is well established; magnitude estimates vary by source and are NOT all measured

- **[MEASURED-COMMUNITY]** AmeraLabs measured peel stress (see 1.1): ACF ~8% lower than thin FEP/nFEP in their test (12 vs 13 kPa) — a much *smaller* gap than marketing claims below. Their own framing: "the article distinguishes between manufacturer claims for ACF (‘almost no release force') and actual measured performance, which showed modest improvements but not dramatic differentiation from thin FEP." This is an important flag — **measured reduction (~8%) is far smaller than marketed/community-quoted reduction (~35–40%, see below).**
- **[MEASURED-COMMUNITY, low-N]** A Siraya Tech blog post cites a Discord user's own instrumented test (Athena MSLA printer's built-in load cell, 3.5 mm tall 40 mm-diameter open ring, 0.05 mm layers, 2 mm/s lift): **ACF reduced average peel force by ~40% vs. nFEP** (no absolute N values reported, only the percentage). Source: https://siraya.tech/blogs/news/a-deep-dive-into-acf-vs-nfep-films-peeling-force . Confidence: MEASURED-COMMUNITY but single anonymous tester, single geometry, no absolute numbers, and Siraya Tech (a resin vendor) republishing a third party's Discord result — treat as a plausible order-of-magnitude, not a robust value.
- **[RULE-OF-THUMB / MARKETING]** Multiple vendor pages (Phrozen help center, Anycubic, Aorita3D) repeat "ACF offers 35% less release force compared to FEP" and "ACF can reduce suction force by approximately 40% compared to nFEP" — these read as vendor marketing figures without disclosed test methodology in the pages I could access (Phrozen page itself returned 403; the "35%" figure is second-hand via search-engine summarization of vendor copy, not independently verified by me against a primary methodology). **Do not treat this 35–40% figure as equivalent in confidence to the AmeraLabs 8% measured figure.**
- **Net read**: FEP vs ACF/nFEP direction (ACF lower) is consistent across every source, but the *magnitude* of the reduction is contested — a disclosed-methodology community measurement puts it at single-digit percent (~8%), while undisclosed vendor/marketing claims and a single anonymous community data point put it at 35–40%. This is a real, material discrepancy worth flagging for the support-placement engine: don't hardcode a 35–40% ACF force discount without a better source.

### 1.5 Not usable / paywalled / no numbers recovered
- Pan & He / Liravi et al. cohesive-delamination model (referenced inside Pan et al. 2017 but not independently pulled) — mentioned only secondhand.
- MDPI "Effects of Curing on Photosensitive Resins in SLA AM" (403, could not access).
- ScienceDirect "Mechanical property modeling... degree of cure with tensile strength and hardness" (403).
- protoresins.com and mini4speed.com blogs — confirmed qualitative-only on direct fetch, no usable numbers despite promising titles.

---

## 2. Green-state mechanical properties (fraction of post-cured strength)

### 2.1 Direct manufacturer datasheet — Formlabs Tough 2000 Resin (best single data point found)

**[MEASURED-VENDOR]** Formlabs publishes side-by-side Green vs. Post-Cured mechanical data on its official TDS (ASTM-method tested, methodology and post-cure protocol disclosed: Form 2, 100 µm layers, green = "without additional treatments"; post-cured = Form Cure, 120 min @ 80°C). Source: https://formlabs-media.formlabs.com/datasheets/2001340-TDS-ENUS-0P.pdf

| Property | Green | Post-Cured | Green as % of Post-Cured |
|---|---|---|---|
| Ultimate Tensile Strength | 29 MPa | 46 MPa | **63%** |
| Tensile Modulus | 1.2 GPa | 2.2 GPa | **55%** |
| Elongation at Break | 74% | 48% | 154% (green is *more* ductile) |
| Flexural Strength | 17 MPa | 65 MPa | **26%** |
| Flexural Modulus | 0.45 GPa | 1.9 GPa | **24%** |
| Notched Izod (impact) | 79 J/m | 40 J/m | 198% (green is tougher to notched impact) |
| Unnotched Izod (impact) | 208 J/m | 715 J/m | 29% (post-cured is tougher unnotched — opposite trend from notched) |
| HDT @ 1.8 MPa | 42 °C | 53 °C | 79% |

Key takeaways for your engine: **green tensile strength/modulus is roughly half to two-thirds of final** for this resin, but **green flexural strength/modulus is only about a quarter of final** — flexural (bending) properties build far more slowly with cure than pure tensile properties do. Green parts are also *more* elongation-capable and *more* notch-tolerant (less brittle in that specific mode) than post-cured, which is somewhat counterintuitive if you assume "green = weaker in every way" — it isn't; it's weaker in load-bearing strength/stiffness but not necessarily more brittle in every failure mode.
- Confidence: MEASURED-VENDOR, high — official ASTM-method datasheet, single resin/single vendor, values will vary by resin chemistry (this is an engineering "Tough"/ABS-like acrylate resin, not a standard brittle photopolymer).

### 2.2 Academic degree-of-cure study — corroborates the same rough magnitude

**[MEASURED-PEER-REVIEW]** "Influence of the Degree of Cure in the Bulk Properties of Graphite Nanoplatelets Nanocomposites Printed via Stereolithography," PMC7285314 (https://pmc.ncbi.nlm.nih.gov/articles/PMC7285314/):
- Green (as-printed) degree of cure for the pristine (unfilled) resin: **~80–85%** (80.1% by Raman, 84.8% by DSC).
- Post-cured (60 °C, UV, 1 hr) degree of cure: **89.6–95.6%** across formulations.
- Young's modulus: **~1280 MPa (green) → ~1800 MPa (post-cured)**, i.e. green ≈ **71%** of post-cured modulus for a ~10-percentage-point gain in degree of cure — the paper explicitly states "a decrease of approximately 10% in the degree of cure had a strong influence on the loss of the mechanical properties for the green samples," implying the strength curve is steep near full cure, not flat.
- Tensile strength: **~48.8 MPa green**, "significantly increased" post-cure (exact post-cure MPa not recovered from my extraction — flagged as incomplete).
- Confidence: MEASURED-PEER-REVIEW, but note this resin already reaches 80–85% cure *in the green state* (i.e., "green" here means straight off the printer without any post-UV, but the printing process itself already drives cure quite high) — this is a materially different regime from an under-exposed/thin support tip, which may be far less cured than 80%. Don't extrapolate this 71%-modulus number down to lightly-exposed support tips.

### 2.3 Secondary, less certain data point (flag as unreliable)

**[LOW CONFIDENCE — possible extraction error]** Scientific Reports 2025 paper ("The impact of modifying 3D printing parameters on mechanical strength and physical properties in vat photopolymerisation," PMC11993694, https://pmc.ncbi.nlm.nih.gov/articles/PMC11993694/) reported no-post-cure tensile strength of 22.8 MPa (horizontal) / 45.0 MPa (vertical) for Formlabs Clear V4 resin, vs. 127.4 MPa for a vertical, 80 °C/15 min post-cure condition. **The 127 MPa figure is implausibly high** relative to Formlabs' own published Clear Resin post-cured UTS (~65 MPa) and likely reflects either a scaled/non-standard dumbbell specimen geometry (the paper used "scaled ASTM D638" specimens, which can inflate apparent UTS) or a extraction/transcription artifact on my part. **Do not use the 127 MPa number without pulling the primary paper directly** — I flag it here only so you know it exists and is suspect, not to endorse it.

### 2.4 General/rule-of-thumb color on how fast strength builds with post-cure

**[MEASURED-VENDOR]** Formlabs' own post-cure blog (https://formlabs.com/blog/how-to-post-cure-3d-prints/) gives one concrete build-rate number: for Rigid 4000 Resin, "tensile modulus increases by 116% over the first 15 minutes of post-curing at 80 °C," after which "no further improvements are observed" — i.e., most of the post-cure strength gain happens fast (first 15 min at 80 °C) and then plateaus. This is a single resin/single condition but a genuinely useful build-rate anchor.
- Confidence: MEASURED-VENDOR.

**No source found** that directly answers "how fast does green strength build up *during printing*, layer by layer" (e.g., strength of a 5-layer-thick green stack vs. a 50-layer stack) — this specific sub-question was not answered by anything I could access. If this matters for your engine, it may need a dedicated experiment rather than literature.

---

## 3. Support-tip pull-off adhesion

**No primary source with directly measured tip pull-off force (N) vs. tip cross-section was found and successfully accessed.** Several promising academic titles exist but were paywalled/blocked (403) on every attempt:
- "Auto-support: traction force constrained support structures optimization for bottom-up vat photopolymerization" (2025, Scientific Reports family / Taylor & Francis) — https://www.tandfonline.com/doi/full/10.1080/17452759.2025.2466189 and https://www.researchgate.net/publication/389116602 — both 403'd. Abstract-level only: formalizes support placement as a "Capacity-Constrained Circle Covering Problem" limiting max force per support under overhang constraints — confirms the *existence* of a force-constraint model but I could not pull the actual N or MPa thresholds used.
- "Towards Scarless Support Removal in Vat Photopolymerization..." (ScienceDirect) — 403'd, no data recovered.
- Xu & Chen "Solid Mechanics Based Design and Optimization for Support Structure Generation in SLA-Based AM" — 403'd, no data recovered.

**[RULE-OF-THUMB]** Consistent, repeated community guidance (AmeraLabs, 3DPrinterly, Dreaming3D, and others) on tip diameter vs. reliability, no force numbers attached:
- Tip diameter **<0.3 mm**: easiest to remove/least visible mark, but weakest.
- **~0.4–0.5 mm**: commonly cited as "the sweet spot" balance of strength vs. mark visibility.
- **0.6–1.5 mm**: used for heavier models/large supports, visibly stronger but leaves bigger marks.
- One source states a jump from 0.2 mm to 0.35 mm tip diameter "can be the difference between failure and success" — directionally useful, no force number.
- Confidence: RULE-OF-THUMB across all of these — consistent across many independent vendor/community sources (which raises practical confidence in the *ranking*, even with zero measured Newtons behind it).

**[DERIVED-ESTIMATE — not a citation, my own calculation]** Combining §2.1's green UTS (Formlabs Tough 2000: 29 MPa green) with typical tip diameters, as an order-of-magnitude sanity check only (pure tension, ignores stress concentration, peel-vs-tension loading mode, and that a support tip is likely far less cured than a bulk 100 µm-layer tensile bar since it's a thin, possibly under-exposed feature):
- 0.3 mm diameter tip, area ≈ 0.0707 mm² → ≈ **2.1 N** theoretical max pure-tensile pull-off at 29 MPa.
- 0.4 mm diameter tip, area ≈ 0.1257 mm² → ≈ **3.6 N**.
- 0.5 mm diameter tip, area ≈ 0.1963 mm² → ≈ **5.7 N**.
- Cross-referencing against §1's measured peel forces (~1–6 N for cm²-scale cured layers, and up to 20+ N for large/many-layer parts under oxygen-depleted conditions per Pan et al. Figure 7), a 0.3–0.5 mm tip's theoretical pure-tensile capacity is in the *same order of magnitude* as typical single-layer peel forces — which is exactly why tip failure is such a common, finely-balanced failure mode in practice, consistent with the qualitative community framing ("battle between FEP peel force and support tensile strength"). **Treat this derived number as a plausibility check, not a design value** — real failures are peel/shear/stress-concentration dominated, not pure axial tension, and actual tip cure state is unknown.

---

## 4. Suction/cupping forces for hollow or cup-shaped geometry

### 4.1 Mechanism — two distinct physical effects, confirmed by manufacturer app notes

**[MEASURED-VENDOR / authoritative, but qualitative]** Formlabs support article (https://formlabs.com/support/Cupping-Blowout) and Asiga support article (https://support.asiga.com/suction-cup-effect/) both explicitly describe cupping/blowout as **pressure-differential-driven wall buckling** (air trapped in a hollow cavity expands as the build plate retracts, dropping internal pressure, pushing walls inward) — this is described as mechanistically *distinct* from the ordinary layer-peel suction force discussed in §1 (which acts on the newly-cured layer against the film). No numeric pressure/force values were given by either vendor. Formlabs states risk depends on "resin type, size of the cup, shape of the model, and thickness of the cup wall" and recommends drainage holes + reorientation; Asiga recommends keeping the vat filled ≥75% and adding drain holes, again no numbers.
- Confidence: MEASURED-VENDOR for the mechanism description (authoritative, official troubleshooting docs of two established printer makers); no quantitative values from either.

### 4.2 Quantitative anchor from the same peer-reviewed rig as §1

**[MEASURED-PEER-REVIEW]** The "Review of Critical Issues in High-Speed Vat Photopolymerization" (PMC10302688) cites a U-shaped constant-area (625 mm²) test geometry reaching **maximum separation force ~36 N at 1 s exposure** — this is a solid-cross-section geometry test (not a true air-trapping hollow cavity) but shows how much higher force gets with larger area/awkward shape even without true suction-cup air-trapping. The same review states, from a different cited study, that **"maximum separation force is independent of the part's geometry" for a hollow vs. solid circle at constant 1600 mm² area** in an LCD-specific study — i.e., for *simple through-hole* hollow shapes (not sealed cavities), geometry mattered less than area. **This is not the same as a sealed suction-cup cavity** (bust/cup with no drain holes), which is the failure mode community sources describe as catastrophic — no paper I could access put a number on a *sealed* trapped-air cavity's blowout pressure/force.
- Confidence: MEASURED-PEER-REVIEW for the 36 N figure and the "independent of geometry" finding (as reported by the review; I did not independently chase the review's own underlying primary source for these two specific claims, so treat them as secondhand-but-peer-reviewed).

### 4.3 Rule-of-thumb magnitude/severity claims (uniformly unquantified)

**[RULE-OF-THUMB]** Every vendor/community troubleshooting source I fetched (protoresins.com, mini4speed.com, Lychee/Mango3D docs, Anycubic, HeyGears) states suction force is "directly proportional to surface area" and that sealed hollow cavities without drain holes produce forces strong enough to "rip the print in half horizontally" or cause "catastrophic failure" — but **none disclosed an actual measurement or methodology**, confirmed on direct fetch of protoresins.com and mini4speed.com (both explicitly returned "no quantitative data" on inspection). Universal, standard guidance: **minimum two drain holes**, one near the build plate (vents air, breaks vacuum) and one at the opposite end (drains liquid resin), non-negotiable for any hollow print. No source gave a minimum hole diameter with engineering justification (all just say "add drain holes," no sizing formula found).

---

## 5. ABS-like water-washable resins — specific quirks

### 5.1 Green-state brittleness — direction confirmed, magnitude thin

**[RULE-OF-THUMB, but mechanistically explained]** A vendor guide (inslogic3d.com) gives a mechanistic (not measured) explanation: hydrophilic surfactant/additive packages used to make resin water-dispersible **reduce cross-link density**, which "means cured parts have lower tensile strength and less elongation at break — both of which contribute to a higher likelihood of snapping under stress," worst in "thin, high-stress geometries: miniature weapons, fine lattice structures, small clips." Also warns **over-curing exacerbates brittleness** in both water-washable and standard resin. Source: https://www.inslogic3d.com/blogs/blog/water-washable-resin-guide-resin-properties-washing-and-post-curing
- Confidence: RULE-OF-THUMB (mechanistically plausible, vendor-published, but no measured numbers, no citation of a controlled test).

### 5.2 One directly-sourced datasheet number for a water-washable ABS-like resin

**[MEASURED-VENDOR]** "W90B" water-washable resin datasheet numbers (as found via product/amazon-hosted instruction PDF, secondhand via search extraction, not independently re-verified against the primary PDF): Shore Hardness 82D, **Tensile Strength 35.66 MPa**, **Flexural Strength 38.07 MPa**, **Elongation at Break 18%**, viscosity 139 mPa·s. For comparison, Formlabs Tough 2000 (an ABS-like *engineering* resin, not itself water-washable) has post-cured UTS 46 MPa / elongation 48% — the water-washable resin's much lower elongation (18% vs 48%) is *directionally consistent* with the "more brittle" community claim, though this is comparing two different resin families/vendors, not a controlled same-formula comparison.
- Confidence: MEASURED-VENDOR but low-verification (I could not open the primary PDF directly; number came through search-result text extraction) — treat the exact 35.66/38.07/18% figures as indicative, re-verify against the primary datasheet before hard-coding.

### 5.3 Water absorption

**[RULE-OF-THUMB / industry-standard-referenced]** One search-aggregated source states industry practice targets "water absorption less than 0.5% weight gain" per ASTM D570 for water-washable formulations, and that Anycubic markets an "optimized hydrophilic formula" claiming reduced water absorption/cracking resistance — no primary measured value with methodology was successfully pulled (Anycubic/Elegoo datasheet PDFs were not accessible in readable form during this session).
- A community instrumented test exists — Honza Mrázek's blog, "I tested how much moisture SLA printer resins absorb. How it changes them?" (https://blog.honzamrazek.cz/2021/05/i-tested-how-much-moisture-sla-printers-resins-absorb-how-it-changes-them/) — dry/weigh/soak/re-weigh methodology across multiple resins including water-washable ones. **I was blocked from fetching this page directly (401 on repeated attempts)** and could only recover a paraphrase via search snippet: the author explicitly caveats that the test "was designed to compare resins rather than get precise values, as proper measuring equipment and controlled environment were not available" — so even if recovered, treat this as **low-precision community data**, not a rigorous measurement. I was not able to pull the actual percentage numbers this session.
- **Net**: I could not obtain a solid, independently-verified quantitative water-absorption percentage specific to ABS-like water-washable resin this session. What exists in the literature I could reach is either vendor marketing (no methodology) or a self-admittedly imprecise community test I couldn't fully access. This sub-question should be treated as **open** pending a better source or a direct instrumented test.

### 5.4 Warping

**No quantitative source found or accessed** for warping specifically tied to water-washable/ABS-like resin (vs. standard resin). Every vendor page found asserts "warp-free" or "low-shrinkage" as a marketing claim with zero supporting data. Treat as **unanswered** by this literature pass.

---

## Summary table of confidence-weighted numbers you can actually use

| Question | Best number | Confidence |
|---|---|---|
| Peel stress, small-medium cured area, FEP/ACF/nFEP or PDMS | **~1–2 N/cm²** (converges from two independent sources: AmeraLabs 1.2–1.3 N/cm² on FEP/ACF/nFEP; Pan et al. 1.56–1.96 N/cm² on PDMS) | Measured, two independent methods, different interface materials — moderate-high |
| Peel force vs. lift speed | Roughly linear in the ~0.6–3 mm/s range (Pan et al.); becomes sub/super-linear at extreme speed ratios (other paper) | Measured, peer-reviewed |
| Peel force vs. area | Superlinear (~R⁴ for solid discs); shape/perimeter also matters independently of area | Measured, peer-reviewed |
| ACF vs FEP peel-force reduction | **~8%** (disclosed-methodology community measurement) vs. **35–40%** (vendor marketing / single anonymous community data point) — genuine unresolved discrepancy | Mixed — don't hardcode 35-40% |
| Green tensile strength as % of post-cured | **~55–71%** (two independent resins/sources: Formlabs Tough 2000 63% UTS/55% modulus; GNP-nanocomposite paper 71% modulus) | Measured (vendor datasheet + peer-reviewed) |
| Green flexural strength as % of post-cured | **~24–26%** (Formlabs Tough 2000) — builds much slower than tensile | Measured, vendor datasheet |
| Most post-cure strength gain happens fast | ~116% modulus gain in first 15 min @ 80°C, then plateaus (Formlabs Rigid 4000) | Measured, vendor |
| Support tip pull-off force | No direct measurement found; derived estimate ~2–6 N for 0.3–0.5mm tips in pure tension at green UTS | Derived, not measured — treat as plausibility check only |
| Suction/cupping force, sealed hollow cavity | No number found for true sealed-cavity blowout; 36 N cited for a 625mm² solid U-shape (not a sealed cavity) | Measured but not the right geometry class; sealed-cavity case remains unquantified |
| Water-washable resin elongation vs standard | 18% (W90B, low-verification) vs 48% (Tough 2000, different resin family) — directionally consistent with "more brittle" claim | Low-medium, cross-family comparison only |
| Water absorption % for water-washable resin | Not obtained this session (sources blocked/inaccessible) | Open question |
| Warping for water-washable resin | Not obtained this session (marketing claims only) | Open question |

## Sources that were promising but inaccessible this session (worth a retry with different access, e.g. institutional login or sci-hub-adjacent mirror if policy allows)
- ScienceDirect: "Mechanical property modeling of photosensitive liquid resin... Bridging degree of cure with tensile strength and hardness" — https://www.sciencedirect.com/science/article/pii/S0264127518308803 (403)
- MDPI: "Effects of Curing on Photosensitive Resins in SLA Additive Manufacturing" — https://www.mdpi.com/2673-3161/2/4/55 (403 — unusual for MDPI, which is normally open-access; worth a retry)
- Taylor & Francis: "Auto-support: traction force constrained support structures optimization for bottom-up vat photopolymerization" — https://www.tandfonline.com/doi/full/10.1080/17452759.2025.2466189 (403) — most directly relevant paper to Q3 that I found; worth prioritizing if you can get access.
- ScienceDirect: "Towards Scarless Support Removal in Vat Photopolymerization Additive Manufacturing" — https://www.sciencedirect.com/science/article/pii/S2212827124012071 (403)
- blog.honzamrazek.cz moisture-absorption post (401 on repeated direct fetch, only search-snippet paraphrase recovered) — https://blog.honzamrazek.cz/2021/05/i-tested-how-much-moisture-sla-printers-resins-absorb-how-it-changes-them/
- sanad.iau.ir / ResearchGate full text of the FEP-vs-PP separation-speed paper (redirect loop / 403) — only abstract-level percentages recovered.
