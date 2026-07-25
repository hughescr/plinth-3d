# Physics literature — extracted numbers

Five peer-reviewed papers filed in this directory, read for the specific
numbers our support-physics model is currently asserting without a source.
Companion to `lit-green-peel.md` (the earlier confidence-classed review);
this file supersedes nothing in it, it adds primary-source measurements.

**Provenance rule applied throughout:** every number below is quoted with
the resin, film/vat interface, layer height, speed and temperature it was
measured at. A peel force measured on PP film at 5 mm/min does not transfer
to nFEP at 2 mm/s, and a "green" tensile strength measured on a washed,
fully-printed dogbone is not the same quantity as the strength of one
50 µm layer three seconds after exposure. Where a paper does **not** answer
one of our questions, this file says so instead of stretching its results.

**Derived-here marker:** fits, unit conversions and pressure calculations
that *I* computed from the papers' tabulated data (rather than quoting the
authors) are marked **[derived]**. The papers themselves do not state them.

---

## 1. Separation force vs speed and cross-section (FEP / PP)

> Moayedi Manizani, S., Zamani, J., Salehi, M., & Shayesteh, M. (2023).
> *Investigating the Effect of Separation Speed and Image Cross-Section
> Geometry on The Separation Force in DLP Method using FEP and PP Polymer
> Membranes.* International Journal of Advanced Design and Manufacturing
> Technology, **16**(3), 9–17.
> DOI: [10.30486/admt.2023.1968925.1386](https://doi.org/10.30486/admt.2023.1968925.1386)
> File: `separation-force-vs-speed-and-cross-section-fep-pp.pdf`
> (the PDF's embedded `Title` metadata is wrong — it says "Towards
> optimisation of openMP codes"; the document content is correct.)

**This is the highest-value paper for us.** It is the only one of the five
that measures separation force against *both* speed and cross-section on a
fluoropolymer film.

### Test conditions (p. 12, Tables 3–4)

| Parameter | Value |
|---|---|
| Process | DLP, bottom-up, "Ultra" lab-built system (KNTU), 80 × 65 × 45 mm |
| Films | FEP and PP, **both 100 µm**, over a 4 mm acrylic backing sheet |
| Resin | Anycubic, viscosity **522 mPa·s**, 405 nm, tensile 23.4 MPa |
| Layer height | 0.05 mm |
| Exposure | 6.2 s at 0.2 mW/cm² (UV absorber-attenuated from 8 mW/cm²) |
| Separation height | 2 mm |
| Vat volume | 336 cm³ |
| Force sensor | SEWHACNM, 50 N full scale, online on the platform |
| Temperature | **not reported** |
| nFEP | **not tested** — FEP and PP only |

### ⚠ Two documentation defects in the paper, resolved against the figures

1. **Table 3's column headers are swapped relative to Figs. 5 and 6.**
   Table 3 (p. 14) labels the first two numeric columns "Separation Time (s)"
   and the last two "Separation Force (N)". Fig. 5 (p. 15, y-axis
   "Separation force (N)") plots the *first* pair, and Fig. 6 (p. 15,
   y-axis "Separation Time (s)") plots the *second* pair. The figures are
   the correct reading, and three independent checks confirm it:
   - the body text (p. 15) says "at a speed of 50 mm/hour, this force is
     2.32 newton for FEP and 3.01 newton for PP" — the first pair;
   - the claim "FEP membrane leads to less separation force" (p. 15) only
     holds for the first pair;
   - the claimed rises of **+230 % (FEP)** and **+175 % (PP)** reproduce
     exactly from the first pair (3.85/1.17 = 3.29 → +229 %;
     4.95/1.81 = 2.74 → +174 %) and not at all from the second.
   - **Independent cross-check:** Table 1's L4 (a solid 20 mm disc, same
     geometry, same 50 speed) gives 2.25 N — within 3 % of Table 3's
     2.32 N under this reading. Under the as-printed header it would have
     to be compared against 2.2 s. The reading below is correct.

2. **Speed units are self-contradictory.** Table 3 says mm/h; §4.3 (p. 13)
   and Figs. 5, 7, 8 say mm/min; Table 4 lists "average print speed
   10 mm/h". **mm/min is almost certainly right** — Figs. 7/8 x-axes run to
   10.4 s, and a 2 mm separation height at 450 mm/h would take 16 s while
   at 450 mm/min it takes 0.27 s, matching the 0.8 s measured transient.
   Treat the speed axis as **mm/min (0.083–7.5 mm/s)** and flag the residual
   risk. Our lift speeds (~1–3 mm/s) sit inside this range either way only
   under the mm/min reading.

Also unreliable: the abstract's "separation time decreased by 18.8 times /
29.6 times" and the conclusion's "8.18 / 6.29 times" do not reproduce from
Table 3 under any column assignment (the actual time ratios are 10.4× FEP
and 15.8× PP). **Do not cite the paper's time-reduction claims.** The force
data is solid and triply cross-checked; the time data is not.

### 1a. Separation force vs speed — solid 20 mm disc (A = 314.16 mm²)

Table 3 (p. 14) + Figs. 5–8 (p. 15). Corrected reading:

| Speed (mm/min) | F, FEP (N) | F, PP (N) | **P, FEP (kPa)** [derived] | **P, PP (kPa)** [derived] | t_sep FEP (s) | t_sep PP (s) |
|---:|---:|---:|---:|---:|---:|---:|
| 5   | 1.17 | 1.81 | **3.72** | 5.76 | 8.3 | 7.9 |
| 10  | 1.50 | 1.96 | **4.77** | 6.24 | 6.1 | 5.1 |
| 20  | 1.95 | 2.72 | **6.21** | 8.66 | 3.1 | 2.8 |
| 50  | 2.32 | 3.01 | **7.39** | 9.58 | 2.2 | 1.6 |
| 80  | 2.93 | 4.15 | **9.33** | 13.21 | 1.4 | 0.9 |
| 150 | 3.41 | 4.25 | **10.85** | 13.53 | 1.1 | 0.8 |
| 450 | 3.85 | 4.95 | **12.26** | 15.76 | 0.8 | 0.5 |

Pressure column **[derived]** = F / 314.16 mm². The paper never converts to
pressure; we need pressure because `peel_pressure_pa` is a stress.

**Functional form.** The paper offers Eq. (4), p. 12 — the Stefan-adhesion
/ fluid-suction model for a rigid disc:

```
F = (3 π μ V / (2 h³)) · R⁴          (Eq. 4, p. 12)
```

with μ resin viscosity, V separation speed, h gap, R disc radius. It
predicts **F ∝ V¹** and **F ∝ R⁴ = A²**. The abstract asserts "the outcomes
are consistent with the mathematical model given." **They are not.** A 90×
speed increase produces only a 3.3× force increase, not 90×. Eq. (4) is the
wrong functional form for this data and should not be used.

**What the data actually fits** — least-squares power law on ln F vs ln V
**[derived, not in the paper]**:

```
FEP:  F(V) = 0.809 · V^0.274   N      (V in mm/min, A = 314 mm²)
      P(V) = 2575  · V^0.274   Pa
PP:   F(V) = 1.231 · V^0.243   N
      P(V) = 3918  · V^0.243   Pa
```

So **peel pressure goes as roughly the fourth root of speed** — a very weak
dependence. Evaluated at realistic MSLA lift speeds:

| Lift speed | FEP peel pressure [derived] |
|---|---|
| 1 mm/s (60 mm/min) | **≈ 7.9 kPa** |
| 2 mm/s (120 mm/min) | **≈ 9.6 kPa** |
| 3 mm/s (180 mm/min) | **≈ 10.6 kPa** |

FEP is consistently *lower* force than PP at every speed (by 10–55 %),
confirming the paper's qualitative claim and its recommendation of PP only
as a cheap substitute, not an improvement.

### 1b. Separation force vs cross-sectional area

Two separate experiments, and they give **different exponents** because they
vary different things. This distinction is the single most useful structural
insight in the paper.

**(i) Area varied at *constant* outer envelope** — Table 1, p. 13. Four
20 mm-diameter lattice discs of differing porosity, FEP, speed 50:

| Part | Area (mm²) | Porosity (%) | F (N) | **P (kPa)** [derived] |
|---|---:|---:|---:|---:|
| L1 | 117.18 | 62.68 | 1.78 | **15.19** |
| L2 | 134.17 | 57.27 | 1.79 | **13.34** |
| L3 | 202.53 | 35.50 | 1.68 | **8.30** |
| L4 | 314.00 | 0 | 2.25 | **7.17** |

A **2.68× area change produces only a 1.26× force change** — power-law
exponent **n ≈ 0.24** [derived]. The paper's own conclusion (abstract, p. 9):
"changing the percentage of surface porosity has a negligible effect on the
amount of separation force." **Hollowing out a layer does not reduce its
peel force.** Note the pressure column: peel *pressure* more than doubles as
area halves.

**(ii) Area varied *with* the envelope** — Fig. 4, p. 14, and Table 2, p. 13.
Three pyramids (G1, G2, Full) printed over 33 layers, so both area and
outer diameter shrink together. Full part: slice 1 area 490.87 mm², slice 90
area 8.3 mm²; Fig. 4 shows force falling from ≈ 6.5 N to ≈ 1 N over the
33 layers. That is a **59× area reduction for a 6.5× force reduction** —
exponent **n ≈ 0.46** [derived], i.e. force tracks the *linear* dimension
(≈ √A, ≈ perimeter), not the area.

(The abstract's phrasing of this — "reducing the cross-sectional area by
1.36 % reduced the separation force by 6.5 times" — appears to mean
"reducing area *to* 1.7 % of its original value"; 8.3/490.87 = 1.69 %.
Quote the ratio, not the abstract's sentence.)

**(iii) Shape at constant area matters too.** Fig. 4: G1 (annulus + spokes,
slice-1 area 256.76 mm²) tracks Full (490.87 mm²) almost exactly, while G2
(252.76 mm² — essentially the same area as G1) sits well below both. The
paper attributes this to perimeter. This is a genuine result but the paper's
own Table 2 shows all three sharing the same "maximum diameter" per slice,
so its stated explanation is internally muddled. Treat (iii) as
"shape matters beyond area, direction confirmed, magnitude uncalibrated."

### What this paper does NOT give us

- **No nFEP.** FEP and PP only. Our nFEP films are untested here.
- **No temperature.** Peel force is strongly temperature-dependent through
  viscosity; this paper reports none.
- **No support-scale contacts.** Smallest measured cross-section is
  8.3 mm². Our support tips are 0.1–0.8 mm². Applying the n ≈ 0.46 relation
  down to tip scale is a 1–2 decade extrapolation and must be flagged as
  such wherever it is used.
- **No separation-height dependence.** Explicitly out of scope (p. 14:
  "This parameter has not been looked into in this article"), even though
  Eq. (4) says F ∝ h⁻³.
- **Resin is 522 mPa·s** — roughly 2–3× thicker than typical water-washable
  ABS-like resins. If the fluid-suction term is any part of the total, our
  resin should peel *easier* than these numbers by that ratio.

---

## 2. Green-state mechanical properties vs degree of cure

> Yang, Y., Li, L., & Zhao, J. (2019). *Mechanical property modeling of
> photosensitive liquid resin in stereolithography additive manufacturing:
> Bridging degree of cure with tensile strength and hardness.*
> Materials and Design, **162**, 418–428.
> DOI: [10.1016/j.matdes.2018.12.009](https://doi.org/10.1016/j.matdes.2018.12.009)
> File: `mechanical-property-modeling-degree-of-cure.pdf`

### Test conditions

| Parameter | Value |
|---|---|
| Process | Mask Image Projection (MIP) SLA lab prototype, free-surface (part remains submerged and keeps curing) |
| Resin | **EnvisionTec LS600** — hexane-1,6-diol diacrylate, isobornyl acrylate, acrylate monomer, TiO₂, acrylate oligomer (p. 425) |
| Layer thickness | 0.02, 0.08, 0.1, 0.14, 0.2, 0.4 mm (Table 1, p. 422) |
| Orientation | 0°, 20°, 40°, 45°, 60°, 90° |
| Specimens | ASTM D638-14 Type V, ≥ 8 replicates each |
| Tensile | Instron 3365, 5 kN, **0.5 mm/min** |
| Hardness | 560-10D Shore D durometer, 10 readings per specimen |
| Degree of cure | DSC Q2000, 5 °C/min from 30 → 300 °C |
| Post-cure | UV, 2–24 h sweep |

### The curves (the deliverable)

**Ultimate tensile strength vs degree of cure** — Eq. (25), p. 426,
R² = 88 %, α as a fraction in [0, 1], UTS in MPa:

```
UTS(α) = 0.04747 · exp(2.682 · α)  +  1.574e-14 · exp(36.47 · α)
```

**Shore D hardness vs degree of cure** — Eq. (26), p. 426, R² = 86 %:

```
H(α) = 5.148e5 · exp(-9.869 · α)  +  4.201e-9 · exp(23.89 · α)
```

Evaluated **[derived]** — the paper never tabulates the curve:

| α | UTS (MPa) | Shore D |
|---:|---:|---:|
| 0.80 | 0.5 | 190 (out of range) |
| 0.85 | 0.9 | 118 |
| 0.90 | 3.4 | 91 |
| 0.92 | 6.4 | 84 |
| 0.94 | 12.7 | 80 |
| 0.95 | 18.2 | 78.5 |
| **0.96** | **25.9** | **77.7** |
| **0.97** | **37.0** | **84.3** |

**The dominant behaviour is the `exp(36.47·α)` term.** Near α ≈ 0.96,
UTS **doubles for every Δα = ln 2 / 36.47 = 0.019**. A two-percentage-point
change in cure halves or doubles green tensile strength. Green strength is a
knife-edge function of exposure, not a material constant — which is exactly
the caveat task #111 recorded beside the 350 MPa modulus figure
("green-state modulus is not purely a resin property, it is RESIN × EXPOSURE").
This paper supplies the quantitative slope behind that caveat.

### Measured values (Tables 5 and 6, pp. 426–427)

Specimens 10–18 (layer 0.08–0.2 mm, orientation 20–60°), measured:

| | Green | Post-cured | ratio |
|---|---|---|---|
| Tensile strength (MPa) | 24.48 – 36.52, **mean ≈ 28.3** | 27.55 – 41.53, **mean ≈ 34.8** | 0.81 |
| Hardness (Shore D) | 73.28 – 79.51, mean ≈ 77.3 | 75.28 – 84.49, mean ≈ 81.1 | 0.95 |

### Degree of cure actually reached

- **DSC-measured**, Eq. (24) + Fig. 7, p. 426: ΔH_liquid = 129.0 J/g;
  specimen enthalpies span 8.502 → 28.46 J/g, so **α = 78 % … 93 %** across
  specimens 1–9 (the wide layer-thickness sweep, 0.02 / 0.1 / 0.4 mm).
- **Model-implied** for the validation specimens 10–18, back-solving
  Eqs. (25) and (26) against their measured UTS *and* hardness
  simultaneously **[derived]**: **α ≈ 0.96 green, α ≈ 0.97 post-cured.**
  Both equations agree at those values, which is a good consistency check.
- Cure-kinetics model, Eq. (8), p. 422, fitted parameters (Table 4, p. 424):
  φ = 0.3847, q = 1.369, p = 1.632, E = 18, m = 1.5, n = 1.9. Model
  prediction accuracy 93.5 % (p. 426).
- Per-layer exposure used: `tco(d) = 108.8·d² + 26.62·d + 1.744` s, d in mm
  (Eq. 23, p. 426). At d = 0.05 mm this is 3.35 s.

### ⚠ The critical caveat for our use

**Their "green" is not our green.** Their model's stated foundation (p. 421)
is that "a printed layer is still being slightly cured by the UV light that
penetrates from the new fresh layers" — a layer is cured up to ⌊D_R/d⌋
times before the part leaves the resin. Their green specimen is a
*fully-printed part*, re-exposed dozens of times, at α ≈ 0.96.

**Our load case is one layer, seconds old, at its first peel**, cured once.
That is a much lower α, and on this curve the difference is brutal: our
config's `tensile_green_mpa = 10.0` sits at **α ≈ 0.935**, which is a
perfectly plausible single-exposure cure state and squarely inside the
paper's own DSC-measured 78–93 % band. **This paper does not invalidate our
10 MPa; it supplies the first physical model under which 10 MPa is a
principled number rather than an assertion.**

### What this paper does NOT give us

- **It does not model elastic modulus at all.** Conclusions, p. 427,
  explicitly list "elastic modulus, Poisson's ratio, and stiffness" as
  *future work*. It cannot close the `modulus_green_gpa` gap. (Paper 3
  below does.)
- Fig. 11 (p. 426) plots stress-strain at different cure levels, from which
  a modulus could in principle be read, but no values are tabulated.
- LS600 is an industrial EnvisionTec resin, **not** a water-washable
  ABS-like consumer resin.

### Post-cure duration (Fig. 12, p. 427)

Tensile strength gain vs post-cure time: 2 h **+30.06 %**, 8 h **+37.33 %**,
16 h **+73.58 %**, 18 h +29.47 %, 20 h +28.68 %, 24 h **−14.03 %**.
Hardness gain: +2.71 / +4.67 / **+8.60** / +5.76 / +5.06 / +0.08 %.
**Optimum at 16 h; beyond that, over-exposure degrades both properties.**
At the 16 h optimum the green:post-cure tensile ratio is 1/1.7358 = **0.58**.

---

## 3. Curing effects — green vs post-cure across 12 resins

> Riccio, C., Civera, M., Grimaldo Ruiz, O., Pedullà, P., Rodriguez Reinoso,
> M., Tommasi, G., Vollaro, M., Burgio, V., & Surace, C. (2021). *Effects of
> Curing on Photosensitive Resins in SLA Additive Manufacturing.*
> Applied Mechanics, **2**(4), 942–955.
> DOI: [10.3390/applmech2040055](https://doi.org/10.3390/applmech2040055)
> (corrected 28 January 2022)
> File: `effects-of-curing-on-photosensitive-resins-sla.pdf`

**This paper is far more valuable to us than its title suggests.** It is a
direct, systematic, 12-resin measurement of *green-state Young's modulus and
tensile strength against post-cured*, which is precisely the "biggest
remaining data gap" recorded in task #109 and the evidentiary basis task
#111 needs.

### Test conditions

| Parameter | Value |
|---|---|
| Printer | **Formlabs Form 2** (laser SLA, top-down) |
| Resins | 12 Formlabs commercial methacrylates |
| Specimens | ISO 527 type 5A, **72 total**; 3 green + 3 post-cured per resin |
| Green state | printed, Form Wash, **not post-cured** |
| Post-cure | Form Cure, per-resin manufacturer settings (Table 1, p. 947): 15–120 min at 60–80 °C |
| Tensile | MTS Insight 2, 1000 N cell, **5 mm/min**, 50 Hz, 50 mm clamp gauge |
| Temperature | ambient, not controlled/reported |

### Table 2 (p. 947) — the money table

Load at break and Young's modulus, green vs post-cure, experimental (not
datasheet) values. Ratio columns are **[derived]**.

| Resin | Green UTS (MPa) | Post UTS (MPa) | UTS ratio | **Green E (GPa)** | Post E (GPa) | **E ratio** |
|---|---:|---:|---:|---:|---:|---:|
| Tough | 23.5 ± 1.83 | 30.4 ± 22.17 † | 0.77 | 0.3 ± 0.03 | 0.6 ± 0.08 | 0.50 |
| Standard Clear | 20.1 ± 2.17 | 42.2 ± 1.91 | 0.48 | 0.7 ± 0.09 | 1.0 ± 0.03 | 0.70 |
| Grey Pro | 32.2 ± 5.68 | 60.4 ± 1.90 | 0.53 | 0.8 ± 0.02 | 1.0 ± 0.03 | 0.80 |
| BioMed Amber | 41.2 ± 1.04 | 76.8 ± 4.09 | 0.54 | 0.8 ± 0.02 | 1.2 ± 0.02 | 0.67 |
| Dental LT Clear | 34.6 ± 1.89 | 61.8 ± 10.69 | 0.56 | 0.6 ± 0.04 | — ‡ | — |
| Tough 1500 | 30.4 ± 2.63 | 41.2 ± 0.96 | 0.74 | 0.4 ± 0.02 | 0.6 ± 0.01 | 0.67 |
| Tough 2000 | 26.5 ± 0.41 | 42.0 ± 1.43 | 0.63 | 0.4 ± 0.04 | 0.8 ± 0.02 | 0.50 |
| Custom Tray | 26.9 ± 1.98 | 84.7 ± 2.43 | 0.32 | 0.4 ± 0.01 | 1.1 ± 0.01 | 0.36 |
| High Temp | 26.2 ± 0.30 | 48.2 ± 7.90 | 0.54 | 0.3 ± 0.01 | 0.8 ± 0.00 | 0.38 |
| **Durable** | **17.0 ± 0.79** | 34.5 ± 1.34 | 0.49 | **0.2 ± 0.03** | 0.4 ± 0.01 | 0.50 |
| BioMed Clear | 30.9 ± 1.87 | 60.1 ± 0.55 | 0.51 | 0.4 ± 0.02 | 0.9 ± 0.02 | 0.44 |
| Rigid | 39.8 ± 3.07 | 74.5 ± 7.69 | 0.53 | 0.7 ± 0.02 | 1.3 ± 0.18 | 0.54 |

† The Tough post-cure σ = 22.17 MPa on a mean of 30.4 MPa is larger than
most of the other means; that one cell is not usable.
‡ Dental LT Clear became so brittle after curing that it fractured during
clamping (footnote, p. 947).

**Headline numbers [derived from Table 2]:**

- **Green Young's modulus envelope: 0.2 – 0.8 GPa** (200 – 800 MPa).
  **Median 0.4 GPa. Minimum 0.2 GPa (Durable).**
- **Green tensile strength envelope: 17.0 – 41.2 MPa.**
  **Median ≈ 28.7 MPa. Minimum 17.0 MPa (Durable).**
- **Green : post-cure modulus ratio: 0.36 – 0.80, median 0.52.**
- **Green : post-cure tensile ratio: 0.32 – 0.77, median 0.53.**

The modulus ratio result **independently confirms the "40–55 %" figure**
that tasks #109 and #111 were relying on from two indicative papers — the
median across 12 resins is 52 % — while widening the honest range to
36–80 %. That is a meaningful upgrade in confidence for the derivation
behind the 350 MPa ruling.

Paper's own summary of the gains (p. 947–948): tensile strength increase on
curing ranges **+29 % (Tough) to +215 % (Custom Tray)**; Young's modulus
increase ranges **+33 % (Grey Pro) to +172 % (Custom Tray)**.

Named in prose to two decimals (p. 948): green tensile **30.39 MPa**
(Tough 1500) and **26.46 MPa** (Tough 2000); green modulus **435.08 MPa**
(Tough 1500) and **447.79 MPa** (Tough 2000), both cured 60 min at 60 °C,
gaining +47 % and +74 % modulus respectively.

### Table 3 (p. 948) — ductility, toughness, resilience

Relevant to us because it describes **how a green part fails**, which is the
failure mode our supports actually experience.

| Resin | Ductility green → post (%) | Toughness green → post (MPa) | Resilience green → post (MPa) |
|---|---|---|---|
| Tough | 64.35 → 33.15 | 1299 → 990 | 50.4 → 100.1 |
| Tough 1500 | 89.87 → 73.23 | 2171 → 2603 | 74.9 → 130.9 |
| Tough 2000 | 79.90 → 55.11 | 1788 → 2183 | 88.9 → 134.6 |
| Durable | 88.85 → 72.14 | 1053 → 1885 | 21.5 → 67.0 |
| Standard Clear | 3.52 → 4.64 | 36.7 → 104.7 | 30.4 → 101.7 |
| Rigid | 28.55 → 11.28 | 951 → 565 | 27.4 → 98.2 |
| High Temp | 18.87 → 7.61 | 356 → 166 | 59.3 → 88.7 |

**The tough/durable class is highly ductile in the green state** (65–90 %
elongation) and loses roughly a third to a half of that on post-cure
(p. 951: "maximum elongation was decreased by more than 60 % for all four"
of Rigid / Dental LT / Custom Tray / High Temp). For our class of resin,
green failure is **ductile yielding, not brittle fracture** — which matters
for whether an axial-stress-vs-UTS check (what `tip_radius_mm` does) is the
right criterion at all, versus a yield/deflection criterion.

### Shrinkage

The paper's shrinkage content (p. 945) is **literature review, not
measurement**. It reports, citing its ref. [16], that shrinkage "can reach
up to 10 % of the total free volume" and causes internal tension, and that
low layer thickness and high laser power reduce it (ref. [18]).
**No shrinkage was measured in this study. Do not cite it as a measurement.**

### Cure kinetics

Also not measured here — the paper varies only "green vs manufacturer-
recommended post-cure", with no intermediate cure-degree sweep and no DSC.
For kinetics, use paper 2.

### Ageing note (p. 945, reviewing ref. [14])

Green specimens kept for 30 days **without** UV exposure retained a tacky
surface and unchanged hardness; green specimens left exposed to incidental
UV **gained hardness**. Relevant if we ever reason about time-between-print-
and-support-removal.

---

## 4. Traction-force-constrained support optimization

> Cao, L., Yan, X., Liu, L., Liu, J., Jiang, T., & Lu, L. (2025).
> *Auto-support: traction force constrained support structures optimization
> for bottom-up vat photopolymerization.* Virtual and Physical Prototyping,
> **20**(1), e2466189.
> DOI: [10.1080/17452759.2025.2466189](https://doi.org/10.1080/17452759.2025.2466189)
> File: `auto-support-traction-force-constrained-optimization.pdf`

**This is our exact problem domain**, published nine months ago, and it
independently arrives at the structure our model is missing.

### Test conditions

| Parameter | Value |
|---|---|
| Printer | Accufab-D1s, bottom-up DLP, 144 × 81 × 160 mm |
| **Vat interface** | **PDMS layer over rigid glass** — *not* FEP/nFEP |
| PDMS properties | E = 0.3 MPa, ν = 0.49 |
| Resin | **DM12**: ρ = 1.1 × 10³ kg/m³, tensile strength 24 MPa, **E = 1000 MPa**, ν = 0.4 |
| Lift ("pulling") speed | **2 mm/s** |
| Exposure | 1.1 s |
| Layer thickness | 0.05 mm |
| Force sensing | two load cells under a rigid steel+glass vat; validated with a 49 N weight at five positions to < 1 % deviation |
| Temperature/humidity | held constant by air conditioning (values not stated) |

### 4a. Their force model

Cohesive Zone Model (CZM) for the part/film interface, Eq. (4), p. 6:
`t = K·δ`, traction vector = stiffness matrix × separation-distance matrix.

Calibration (p. 13), from a circular specimen of radius 15 mm,
**A_c = 706.85 mm²**:

```
F_max = 31.7 N   at damage-initiation displacement δ_n⁰ = 0.4 mm
t_n⁰  = F_max / A_c = 0.045 MPa = 45 kPa      ← their peel/suction stress
K = K_nn = t_n⁰ / δ_n⁰ = 0.01125 (MPa/mm)
```

**45 kPa** is their measured normal traction at damage initiation. They note
(p. 14) that they matched their simulation "to the suction pressure
parameter of **45 KPa** in e-Stage 7.3" — i.e. **Materialise's commercial
support generator independently uses the same 45 kPa figure.** Two
independent sources converge.

⚠ **But this is a PDMS vat, not FEP.** PDMS is well known to adhere far more
strongly than fluoropolymer film — that is why FEP displaced it. Paper 1's
FEP measurement at a comparable speed gives ~9–10 kPa. The 4–5× gap between
45 kPa (PDMS) and ~9 kPa (FEP) is physically expected, not a contradiction.
**For our nFEP machines, paper 1's number is the transferable one; 45 kPa is
an upper bound for a stickier interface** — and is also what a commercial
tool ships with, so it is the number that would make us match e-Stage.

### 4b. Their failure criterion — a directly measured support fracture load

Eq. (21), p. 13:

```
F_break = σ_t · A = E · ε_f · A
```

Measured by printing the support and pulling it to fracture with a force
gauge (Fig. 14, p. 14), 7 tensile tests, high and low discarded, 5 averaged:

```
F_break = 1.2 N   for a 0.5 mm × 0.5 mm contact (A = 0.25 mm²)
```

**[derived]** effective fracture stress = 1.2 N / 0.25 mm² = **4.8 MPa**,
consistent with their stated maximum tensile strain of **0.48 %** at
E = 1000 MPa. **This is only 20 % of the resin's bulk tensile strength of
24 MPa.** An as-printed thin strut fractures at a fifth of the datasheet
number — layer interfaces, stress concentration and incomplete cure all
land in that factor.

### 4c. Geometric thresholds they measured

| Threshold | Value | How determined |
|---|---|---|
| Minimum printable support side length | **0.4 mm** square | swept 0.1 → 1.0 mm in 0.1 mm steps (Fig. 13, p. 13) |
| Chosen contact cross-section | **0.5 × 0.5 mm**, later **0.53 × 0.53 mm** with a stated **10 % engineering margin** (p. 20) | for comparability with e-Stage |
| Non-contact shaft cross-section | **0.7 × 0.7 mm** | contact deliberately thinner than shaft |
| Maximum self-supporting overhang distance `d_self-support` | **2.4 mm** | swept 0.4 → 4 mm; "significant layering effect and edge whitening" beyond 2.4 mm (Fig. 15, p. 14) |
| Overhang angle threshold | **6°** for their machine (prior work suggested 10°) | Fig. 15 |
| Stress-field width σ_x = σ_y | **0.45 × γ_i · d_self-support** | swept 0.25–0.65 (Fig. 16, p. 15) |
| Circle-covering max packing utilisation | **0.9069** | standard hexagonal packing constant |

### 4d. What they constrain, and how

Objective, Eq. (8), p. 10: **minimise support count n**, subject to two
simultaneous constraints:

1. **Force constraint** — for every support point `p_i`, the integral of the
   stress field over that point's **Voronoi cell** must not exceed
   `F_break`. Traction is mapped to a continuous stress field via a Gaussian
   kernel, Eq. (5), with an adaptive expansion coefficient
   `γ_i = max(1, √(F_si / F_break))`, Eq. (6).
2. **Overhang constraint** — the union of circles of radius
   `d_self-support` centred on the support points must fully cover the
   geometric overhang region.

Solved by gradient descent on a centroidal-Voronoi-tessellation-style energy
`E_total = λ_k·E_f + E_o` (Eq. 9), with λ_k decaying so the optimiser
weights force early and overhang late.

**Their key modelling simplification, and it is load-bearing for us:**
they establish (Fig. 5, p. 5) that two models with **identical
cross-sections but different 3D shapes** produce Z-direction separation
forces differing by only **0.17 %**, within simulation error, despite
markedly different stress distributions. Hence (p. 5): "the primary factor
affecting the separation force is the shape and region of the vacuum,
rather than the printed layers themselves." **Separation force is a function
of the slice image alone.** This justifies their MLP surrogate, which takes
a 192 × 192 binary slice image and emits a scalar Z force in < 1 ms, trained
on 18,000 images with cross-sections spanning 1–90 % of the frame; mean
relative error 0.032 train / 0.043 test.

### 4e. Magnitudes

- "In our experiments, **separation forces can reach several hundred
  newtons, whereas gravity typically does not exceed 10 newtons**" (p. 4).
  **Peel dominates gravity by 1–2 orders of magnitude.** Any model that
  sizes supports against self-weight is solving the wrong problem.
- Per-support traction, Table 2, p. 18 — max force on any one support,
  before → after their optimisation: Dragon 3.91 → 1.19 N; Cat 1.55 → 1.11;
  Armadillo 2.91 → 1.19; Sofa 7.30 → 1.20; Bone 5.05 → 1.19 N.
  Standard deviation collapses from 0.50–1.46 N to 0.08–0.21 N.
- e-Stage 7.3 for comparison leaves maxima of 1.89–3.42 N — above the
  1.2 N fracture threshold, i.e. **the commercial tool generates supports
  that its own physics says should break.**
- Results, Table 3, p. 21: e-Stage at its default 30° overhang angle
  **failed to fabricate 4 of 5 models**. e-Stage with per-model
  binary-searched critical angles (55°/30°/49°/41°/46°) succeeded but needed
  314/35/134/181/74 supports; their method succeeded with 170/23/89/159/61 —
  a **32 % overall support reduction**, at 14–71 % less compute.

### 4f. Where their approach differs from ours — explicitly

| Dimension | Cao et al. | Ours (current) |
|---|---|---|
| Peel force | per-layer, from the actual slice image, via CZM simulation + MLP surrogate | flat `peel_pressure_pa = 3000` × carried area, identical every layer |
| Speed dependence | fixed at 2 mm/s, not parameterised | none at all |
| Area dependence | implicit and non-linear, learned from the slice image | strictly linear, F ∝ A |
| Failure criterion | `F_support ≤ F_break`, F_break **measured by pulling printed supports to fracture** | axial stress ≤ `tensile_green_mpa / SF`, with SF = 8 and tensile asserted |
| Which layer governs | the **worst** layer, found by clustering slices and simulating only ~10 representatives per model (Table 1, p. 16: 802–1220 layers → 6–12 clusters → 12–42 simulations) | no per-layer analysis; no notion of a governing layer |
| Overhang rule | **two** region types — "geometric overhang" (angle/distance based) *and* "force-augmented overhang" (angle > α_c, added only where force demands it) | single geometric overhang-angle rule, no force-driven region |
| Load balancing | Voronoi/CVT redistribution to equalise per-support traction | none; contacts placed geometrically |
| Gravity | included but noted as negligible vs separation | — |

**The most transferable idea for us is 4f row 6.** Task #109 diagnoses our
two failure modes as needing *separate* fixes — under-support from the
mechanics path, over-support from a geometric overhang-angle rule where
material stiffness never enters. Cao et al. solve exactly that split by
defining two overhang region classes: geometry decides where support is
*mandatory*, and force decides where support is *additionally* required.
Supports are never added for angle alone in a mechanically quiet region.
That is a direct structural answer to the toad-warts / Morel-struts
over-support complaint, from a paper in our exact process.

The second transferable idea is 4f row 5: cluster slices by cross-section
similarity, evaluate only the worst layer per cluster. We currently have no
concept of "which layer is the peel-critical one," and Round 25's 27×
cross-section growth over 1.35 mm is precisely the situation that framing
was invented for.

### What this paper does NOT give us

- **No FEP/nFEP data.** PDMS vat only. The 45 kPa does not transfer directly.
- No speed sweep — everything at 2 mm/s.
- The 45 kPa and 1.2 N are calibrated for DM12 on their machine; the paper
  is explicit (p. 13) that "parameter calibration through physical
  experiments is an unavoidable requirement for any 3D printing system."
- No green-vs-post-cure distinction: E = 1000 MPa for DM12 is quoted flatly,
  and it is not stated whether the 1.2 N fracture test was on green or
  post-cured supports (context suggests as-printed, unwashed/uncured, but
  the paper does not say).

---

## 5. Scarless support removal

> Artemeva, M., Lopez Navas, J., Calaon, M., Basso, A., Nadimpalli, V. K., &
> Pedersen, D. B. (2024). *Towards Scarless Support Removal in Vat
> Photopolymerization Additive Manufacturing for Mass Manufacturing of
> Precision Components.* Procedia CIRP, **130**, 47–52.
> DOI: [10.1016/j.procir.2024.10.054](https://doi.org/10.1016/j.procir.2024.10.054)
> File: `towards-scarless-support-removal-vat-photopolymerization.pdf`

### Test conditions

| Parameter | Value |
|---|---|
| Printer | DTU open-architecture ultra-high-resolution VPP; **7.54 µm pixel** |
| Resin | **FTD Industrial Blend Unpigmented**, ρ = 1.05 g/cm³ |
| Layer height | 10 – 50 µm |
| Burn exposure | 800 ms at 13.091 mW/cm² |
| Normal exposure | 500 ms at 9.141 – 19.899 mW/cm² |
| Repositioning delay | 500 ms |
| Feed rate | 300 mm/min |
| Wash | IPA, ultrasonic, 30 °C, 5 min, 80 kHz |
| Post-cure | 405 nm, 5 min, no added heat |
| Test geometry | worst case — a flat bar at **0°** to the build surface, supported underneath |
| Metrology | Olympus OLS 4000 LEXT confocal; MountainsSPIP; L-filter and S-filter Gaussian ISO 16610-61, 0.025 mm |

### Support contact geometry used (p. 49)

Lychee (Mango3D) cones, **held constant for the entire study**:

| | Value |
|---|---|
| **Tip diameter** | **0.23 mm** (contact area 0.0415 mm² [derived]) |
| Tip length | 1 mm |
| Base diameter | 0.60 mm |
| Minimum XY feature | 50 µm |

### ⚠ The honest answer to our question

**This paper does not measure any relationship between contact geometry and
scarring.** Tip diameter was fixed at 0.23 mm throughout; the study varies
*support count* (via SFF) and *UV exposure*, never tip size. Its own
definition of "scar" (p. 48) is qualitative — "the valleys and/or the peaks
that are formed after the support structure was removed from the part" —
and the only scar metric reported is areal roughness Sq over the whole
supported face, not per-contact scar area or scar diameter.

**Therefore it cannot calibrate `A_ABS_MM2`, `KAPPA`, or `L_SCAR_MAX_MM`**
(`verify.py:32-34`, and `docs/specs/SPEC-EMBED-POLICY.md:42-44`). Those
three remain unexplained module constants after reading this paper. Anyone
looking for a citation to bolt onto them will not find one here. Saying so
plainly is more useful than a stretched inference.

### What it *does* give us

**Support-Free Factor (SFF)** — Eqs. (1)–(2), p. 49; results Table 2, p. 49.
SFF relates supported area to full-part area; lower support fraction = higher
SFF number.

| Sample | % pixels, support | SFF | Result |
|---|---:|---:|---|
| S1 | 3.97 | 24.19 | Success |
| S2 | 3.56 | 27.08 | Success |
| S3 | 2.99 | 32.48 | Success |
| S4 | 2.21 | 44.17 | Success |
| **S5** | **1.51** | **65.19** | **Success (minimum)** |
| S6 | 1.34 | 73.92 | **Failed** |
| S7 | 1.16 | 85.27 | Failed |
| S8 | 0.81 | 122.78 | Failed |

**The support/part projected-area ratio at which parts start detaching from
their supports is between 1.34 % (fails) and 1.51 % (succeeds).** That is a
genuinely usable minimum-support-density figure for a worst-case 0°
down-face — and it is measured, not assumed. (The body text's remark that
"parts manufactured with support equal to 1.25 % of the object succeeded
with high consistency" contradicts Table 2, where 1.34 % already fails.
Trust the table.)

**Removal timing dominates removal difficulty** (Table 3, p. 50; ranking
1 = very easy … 5 = very hard):

| Case | SFF | Timing | Rank | Z dimension (mm, nominal 2.0) |
|---|---|---|---:|---|
| A | min | after post-processing | 3 | 2.24 ± 0.02 |
| **B** | **min** | **before post-processing** | **1** | 2.21 ± 0.03 |
| C | max | after post-processing | 3 | 2.17 ± 0.01 |
| **D** | **max** | **before post-processing** | **1** | 2.18 ± 0.01 |

**Removing supports before post-cure is markedly easier at every support
density**, because post-processing hardens the support material (p. 50).
This is relevant to our "TIP is the fuse" reasoning: `physics_printability.md:62`
argues the tip snaps by hand "during **post-cure** removal, when the resin
is 4–6× stronger than the green state." This paper says the *practitioner*
answer is the opposite — remove **before** post-cure, when it is easiest and
when it also produces dimensions closest to nominal.

**Trapped-resin over-cure between support pillars is a real scarring/accuracy
mechanism** (pp. 49–50, Fig. 4): photopolymer trapped between pillars
over-cures, adding **Δ ≈ 0.4 mm** of height — a **20 % Z error** on a 2 mm
part. Raising part-layer thickness from 10 µm to 50 µm mitigated it. Pushing
irradiance within the part from 9.141 to 19.899 mW/cm² made polymer buildup
inside the support pillar visible.

**Exposure placement matters, and only inside the part:**

| Method | Z error |
|---|---|
| Under-cure within part (UP) | **6.9 %** (best) |
| Control / over-cure or under-cure within support *tip* (OT, UT) | ≈ 8.5 % |
| Over-cure within part (OP) | 11.7 % (worst) |

p. 51: "manipulations with UV intensity within the support structure **tip**
have minimal effect on dimensional changes." Tuning exposure at the tip does
**not** buy dimensional accuracy; tuning it in the part does.

**Roughness:** control Sq = **3.960 µm**; all adaptive-slicing methods lower;
low-irradiance-within-part gave the lowest roughness with the tightest
spread (σ = 0.219 µm).

**Final conclusion (p. 51):** high support count (SFF_max = 24.19) removed
before post-processing gave 8.67 % thickness error and the smoother surface.
Note this is the *opposite* trade from minimising supports — for this
worst-case 0° face, **more** supports gave better dimensional accuracy,
because the part sagged less.

---

# What this changes in our config

Ordered by how much the literature moves the number. Each row states the
supporting paper, the value, and the transfer caveat that limits it.

### `peel_pressure_pa` = 3000 Pa — **raise; strongest finding in this review**

**Current 3000 Pa is below every peel pressure measured in these papers.**

| Source | Interface | Speed | Area | Peel pressure |
|---|---|---|---|---|
| Manizani Table 3 (slowest point) | FEP 100 µm | 0.083 mm/s | 314 mm² | 3.7 kPa |
| Manizani Table 3 | FEP 100 µm | **1 mm/s** | 314 mm² | **≈ 7.9 kPa** |
| Manizani Table 3 | FEP 100 µm | **2 mm/s** | 314 mm² | **≈ 9.6 kPa** |
| Manizani Table 3 | FEP 100 µm | 7.5 mm/s | 314 mm² | 12.3 kPa |
| Manizani Table 1, L1 | FEP 100 µm | ~0.8 mm/s | **117 mm²** | 15.2 kPa |
| Cao §3.1 | **PDMS** | 2 mm/s | 707 mm² | **45 kPa** |
| Materialise e-Stage 7.3 default | (unstated) | — | — | **45 kPa** |

**Recommendation: 8000–10000 Pa** for FEP/nFEP at our lift speeds, i.e.
**2.7–3.3× the current value**, sourced to Manizani Table 3 at 60–120 mm/min.
Do not adopt 45 kPa — that is a PDMS vat, a materially stickier interface.
But note that a commercial tool ships 45 kPa, so our 3 kPa is 15× below what
Materialise assumes.

Impact, using the sensitivities already documented in
`docs/reference/physics_printability.md:87`: tip/tension-floored radii scale
as √F, deflection-floored radii as F^¼. A 3× rise in peel pressure thickens
tension-floored members by **~73 %** and deflection-floored members by
**~32 %**. This is not a small correction.

### `peel_pressure_pa` — **add a speed term** (weak, but now quantified)

**[derived]** from Manizani Table 3, FEP:

```
peel_pressure_pa(V) = 2575 · V^0.274      V in mm/min
peel_pressure_pa(V) = 8010 · V^0.274      V in mm/s   [same fit, rescaled]
```

The exponent ~0.27 (FEP) / ~0.24 (PP) means a **2× lift-speed change moves
peel pressure only ~20 %.** So the absence of a speed term is a much smaller
error than the absolute value being 3× low — but if we ever tune lift speed
for print time, this is the coefficient to reach for. It also justifies
keeping a scalar constant *within* a fixed speed regime, which is worth
recording as a deliberate decision rather than an oversight.

Note the paper's own Eq. (4) predicts F ∝ V¹ and is **contradicted by its own
data**; do not implement Stefan adhesion as written.

### `F = carried_area · peel_pressure_pa` — **the linear area law is wrong**

Three independent lines of evidence:

- Manizani Table 1: 2.68× area change → 1.26× force change → **n ≈ 0.24**
  (porosity at constant envelope).
- Manizani Fig. 4: 59× area reduction → 6.5× force reduction → **n ≈ 0.46**
  (envelope shrinking with area).
- Cao Fig. 5: identical cross-sections, different 3D shapes → separation
  force differs by **0.17 %**. Force is a function of the slice image.

Our model uses **n = 1**. Suggested form:

```
F(A) = P_ref · A_ref · (A / A_ref)^n ,  n ≈ 0.25–0.5
```

anchored at `P_ref ≈ 9 kPa`, `A_ref = 314 mm²` (Manizani, FEP, ~2 mm/s).

**Direction of the error is the dangerous one for us.** With n = 1 we
*under*-predict force on small cross-sections and *over*-predict it on large
ones. Support tips and thin emerging islands are exactly the small-area
case. Round 25's 27× cross-section growth over 1.35 mm carried by one
contact is precisely where an n = 1 law misbehaves — under a n ≈ 0.46 law
that 27× area growth produces only ~4.7× force growth, so a model tuned to
survive the top of the ramp is badly under-margined at the bottom.

⚠ **Extrapolation warning:** the smallest measured cross-section anywhere in
these papers is 8.3 mm². Support-tip areas are 0.1–0.8 mm². Any n applied
down there is a 1–2 decade extrapolation and must be labelled as such.

### `tensile_green_mpa` = 10.0 MPa — **keep, but re-provenance it**

The literature does **not** support raising this, and gives it a real
derivation for the first time.

- Riccio Table 2: green tensile across 12 resins spans **17.0–41.2 MPa**,
  minimum 17.0 (Durable). Our 10 MPa is below the entire measured envelope.
- Yang Table 5: measured green tensile 24.5–36.5 MPa (LS600).
- **But** both measure *washed, fully-printed, multiply-re-exposed
  specimens*, not a fresh single layer at first peel.
- Yang Eq. (25) puts **UTS = 10 MPa at α ≈ 0.935**, inside Yang's own
  DSC-measured 78–93 % band for as-printed SLA layers. **10 MPa is a
  defensible single-exposure green tensile**, and now has a curve behind it
  instead of an uncited design.md range.
- **Counter-evidence worth taking seriously:** Cao's *measured* fracture
  stress for an as-printed 0.5 × 0.5 mm support strut is **4.8 MPa** — half
  our assumed 10 MPa, and only 20 % of that resin's 24 MPa bulk tensile.
  Thin printed struts are much weaker than bulk coupons.

**Action: do not change the value. Replace the provenance comment.** Cite
Yang Eq. (25) with α ≈ 0.935, note the Riccio 17–41 MPa bulk-coupon envelope
as the upper bound, and record Cao's 4.8 MPa strut measurement as the
reason not to raise it. Also record that our SF = 8 gives a design stress of
1.25 MPa, an effective safety factor of **3.8 against Cao's measured strut
fracture stress** rather than the 8 the constant implies.

### `modulus_green_gpa` = 0.5 GPa — **lower; task #111's 350 MPa is now well-supported**

Riccio Table 2 is a direct 12-resin measurement of green modulus:

- **Envelope 0.2 – 0.8 GPa; median 0.4 GPa; minimum 0.2 GPa (Durable).**
- **Green : post-cure ratio 0.36 – 0.80, median 0.52** — independently
  confirming the 40–55 % figure tasks #109/#111 were leaning on, and
  widening the honest range.
- Named values: Tough 1500 **435.08 MPa** green, Tough 2000 **447.79 MPa**
  green (p. 948).

Our current **0.5 GPa sits above the median of the measured green
envelope** — optimistic, in the direction task #109 identified as causing
under-prediction of deflection.

**Task #111's 350 MPa lands between the measured minimum (200 MPa) and the
median (400 MPa).** That is a defensible conservative choice and this paper
is the citation it was missing. Derivation via 0.40 × 790 MPa (bottom of the
post-cure flexural envelope in `resins/RESIN-ENVELOPE.md`) → 316 → 350 now
has a second, independent route: the direct green-modulus measurement of
12 resins, whose 25th percentile is ≈ 0.35 GPa.

⚠ **Caveats to carry with the number:** Riccio's green specimens are Form 2
laser-SLA, IPA-washed, ISO 527 5A coupons pulled at 5 mm/min — not MSLA, not
a fresh layer, not our resin class (Formlabs engineering resins, none
water-washable). And Yang Eq. (25)'s steepness (UTS doubles per Δα = 0.019)
means the exposure caveat task #111 already recorded is quantitatively
severe, not decorative.

**No paper in this set measures modulus vs degree of cure.** Yang explicitly
defers modulus to future work (p. 427). That specific curve remains a gap.

### `A_ABS_MM2` = 2.0, `KAPPA` = 1.25, `L_SCAR_MAX_MM` = 2.5 — **unchanged; no literature support found**

Artemeva et al. is the only scar paper here and **it holds tip diameter
constant at 0.23 mm**, so it cannot calibrate a scar-area cap. It reports
scarring only as whole-face areal roughness (Sq = 3.960 µm control), never
per-contact scar area or scar dimension. These three constants remain
unexplained.

Two weak sanity anchors, offered as context only:

- Artemeva's working contact is 0.23 mm diameter = **0.0415 mm²**; Cao's is
  0.5 × 0.5 mm = **0.25 mm²**. Our `A_ABS_MM2 = 2.0 mm²` cap is **8×–48×**
  a single demonstrated-good contact, so as a *per-contact* cap it is very
  loose; as an aggregate patch cap it is unconstrained by these papers.
- Cao (p. 21): "the 32 % decrease in support count under identical contact
  radii proportionally reduces the surface area susceptible to damage" —
  total scar area scales with count × per-contact area, supporting the shape
  of a cap but supplying no value.

**Closing this gap needs our own measurement**, not another paper.

### Support contact geometry — **two independent measured floors**

| Source | Contact | Note |
|---|---|---|
| Cao §3.1 | **0.4 mm** square minimum printable; **0.5 mm** chosen; **0.53 mm** with 10 % margin | swept 0.1–1.0 mm |
| Artemeva §2.1 | **0.23 mm** tip diameter | Lychee default, worked throughout |

Our `tip_radius_mm` clamp of **[0.2, 1.0] mm** (radius → 0.4–2.0 mm
diameter) brackets both. The lower clamp of 0.2 mm radius = 0.4 mm diameter
matches Cao's measured minimum printable feature almost exactly — a useful
independent confirmation that the floor is in the right place.

### Overhang thresholds — **our 45°/60° are not contradicted, but the framing is**

- Cao measured a **6°** overhang-angle threshold for their machine (prior
  work: 10°), and a **2.4 mm** maximum self-supporting overhang distance.
- Artemeva's whole study is a **0°** flat down-face, the worst case, and it
  succeeds with 1.51 % supported area.
- **But Cao's central argument is that a pure angle threshold is the wrong
  instrument.** e-Stage at 30° failed 4 of 5 models; e-Stage at
  per-model-tuned angles (55°/30°/49°/41°/46° — note the 25° spread) needed
  85 % more supports than the force-aware method. There is no single correct
  angle.

This lands directly on **P3** in `docs/reference/physics_printability.md:102`
(θ_max doing double duty as a resolution limit and an unevaluated strength
limit) and on task #109's diagnosis that over-support comes from the
geometric path where stiffness never enters. **Cao's two-region split —
geometric overhang for mandatory support, force-augmented overhang for
mechanically-demanded support — is the structural fix**, and it comes with
a published implementation in our exact process.

### Peel dominates gravity — **worth stating explicitly somewhere**

Cao p. 4: separation forces reach **several hundred newtons** across a build;
**gravity does not exceed 10 N**. Peel is 1–2 orders of magnitude larger.
Any sizing intuition anchored on self-weight is anchored on the wrong term.

### Support removal timing — **contradicts a claim in our own docs**

`docs/reference/physics_printability.md:62` reasons about the tip as a fuse
snapped "during **post-cure** removal, when the resin is 4–6× stronger."
Artemeva Table 3 measures the opposite practice as better on both axes:
removal **before** post-processing rated 1/5 (very easy) versus 3/5 after,
at both support densities, *and* gave Z dimensions closer to nominal. The
4–6× strength ratio itself is roughly right — Riccio's measured post-cure
tensile gain is +29 % to +215 %, i.e. **1.3–3.2×**, somewhat less than 4–6× —
but the operational conclusion drawn from it should be revisited.

---

## Gaps these five papers do not close

1. **Green modulus vs degree of cure.** Yang models tensile and hardness
   only, and defers modulus explicitly. Riccio gives green modulus but at a
   single (uncured) state with no cure sweep. The curve does not exist here.
2. **nFEP.** Nothing in this set measures nFEP. We have FEP (Manizani) and
   PDMS (Cao) and nothing else.
3. **Peel force at support-tip scale.** Smallest measured cross-section is
   8.3 mm²; our tips are 0.1–0.8 mm².
4. **Temperature.** Not reported by Manizani; not stated by Cao; ambient and
   uncontrolled in Riccio. Viscosity — and therefore any fluid-suction
   component of peel — is strongly temperature-dependent.
5. **Water-washable ABS-like resins specifically.** None of the five papers
   tests one. Manizani uses generic Anycubic; Yang uses EnvisionTec LS600;
   Riccio uses 12 Formlabs engineering resins; Cao uses DM12; Artemeva uses
   FTD Industrial Blend. The supply-uncertainty argument in task #111 —
   design to the bottom of the observed envelope — applies with equal force
   to every number in this file.
6. **Scar area vs contact geometry.** Artemeva holds tip size constant.
   Unmeasured anywhere here.
7. **Separation height / lift distance.** Manizani explicitly declines to
   study it despite Eq. (4) claiming F ∝ h⁻³.
