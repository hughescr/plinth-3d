# SUNLU Water-Wash ABS-Like Resin — Materials Data Audit

Researched: 2026-07-22
Target product: **SUNLU Water Wash ABS-Like Resin** (product page: https://www.sunlu.com/products/water-wash-abs-like-resin)

## Primary source (authoritative)

**Official manufacturer TDS**, Document No. `SL-TE-WI-096`, "LCD TDS ISO", Version A/0, titled "Water Wash ABS-Like Resin".
PDF URL: https://www.sunlu.com/public/upload/file/20260324/34917e9c-8d07-4464-a4d3-390f8d2417d6.pdf?filename=TDS(English)
(linked from the Downloads section of the product page above; product page itself is live and duplicates the headline numbers below but the PDF is the full ISO-test-method document and is treated as ground truth here.)

All "Typical Values" in the TDS are lab averages per the stated ISO method; the document does **not** state the cure schedule (UV time/power) used to prepare the test specimens, and does **not** distinguish green-state vs. post-cured — see Gaps section.

## Full property table

| Property | Value | Unit | Test Method / Condition | Source |
|---|---|---|---|---|
| **Mechanical** | | | | |
| Tensile strength | 26 | MPa | ISO 527/2, 50 mm/min | TDS PDF |
| Elongation at break | 24 | % | ISO 527/2, 50 mm/min | TDS PDF |
| Flexural strength | 34 | MPa | ISO 178, 10 mm/min | TDS PDF |
| Flexural modulus | 956 | MPa | ISO 178, 10 mm/min | TDS PDF |
| Izod impact, notched | 120 | J/m | ISO 180, 23°C | TDS PDF |
| Izod impact, unnotched | **NOT PUBLISHED** (shown as "/" in TDS) | J/m | ISO 180, 23°C | TDS PDF |
| Shore hardness | 77 | HD (Shore D) | ISO 868, 23°C | TDS PDF |
| **Thermal** | | | | |
| Heat distortion temp (HDT) | 47 | °C | ISO 75, 0.45 MPa | TDS PDF |
| Glass transition (Tg) | 45 | °C | ISO 11357-2, 10°C/min | TDS PDF |
| Melting temperature | 200 | °C | ISO 11357-1, 10°C/min | TDS PDF |
| 5% decomposition temp | 300 | °C | ISO 11358, 20°C/min | TDS PDF |
| Volumetric/mold shrinkage | 8.8 | % | ISO 294, 23°C | TDS PDF |
| Coefficient of thermal expansion | 101 × 10⁻⁶ | µm/(m·°C) | ISO 11359-2 | TDS PDF |
| **Physical (resin)** | | | | |
| Liquid density | 1.12 ± 0.02 | g/cm³ | ISO 1675, 23°C | TDS PDF |
| Solid (cured) density | 1.22 ± 0.02 | g/cm³ | ISO 1183, 23°C | TDS PDF |
| Viscosity | 400–800 | mPa·s | ISO 2555, **25°C** | TDS PDF |
| Curing wavelength | 365–405 | nm | — | TDS PDF |
| **Print settings (manufacturer-recommended)** | | | | |
| Ambient print temp | 25–30 | °C, RH<60% | — | TDS PDF |
| Recommended layer thickness | 0.05 | mm | — | TDS PDF |
| Bottom layer count | 5–10 | layers | — | TDS PDF |
| Bottom exposure time | 20–80 (color LCD) / 10–60 (mono LCD) | s | — | TDS PDF |
| Layer exposure time | 4–15 (color LCD) / **"24s"** (mono LCD — see note) | s | — | TDS PDF |
| Lifting height | 6–10 | mm | — | TDS PDF |
| Lifting speed | 60–120 | mm/min | — | TDS PDF |
| Retract/descent speed | 120–180 | mm/min | — | TDS PDF |
| Light-off delay | 0.5–1 | s | — | TDS PDF |
| Storage conditions | 15–35°C, RH<60%, sealed, dark | — | — | TDS PDF |
| Shelf life | 2 | years | — | TDS PDF |
| Post-cure | 3–5 min in a "curing chamber" | min | **No wavelength/power/temperature spec given** | TDS PDF |

Note on mono-LCD layer exposure: the TDS PDF text extracts as "24s" with no dash, which is very likely a lost hyphen for "2-4s" — an earlier scrape of the live product page (same manufacturer, same numbers elsewhere) reported "Monochrome LCD: 2–4s" for this field. Flagging as a probable OCR/formatting artifact in the PDF rather than a real value of 24 seconds; treat as **2–4s, low confidence** until cross-checked against a second clean copy of the PDF.

## Green-state (pre-post-cure) properties

**NOT PUBLISHED.** No manufacturer document or third-party source found gives separate mechanical/physical properties for the as-printed (green) part before post-curing. The TDS's post-cure instructions are limited to "after cleaning, perform post-curing in a curing chamber for 3–5 minutes" with no light source, wavelength, or power spec — so even the post-cure condition that produced the quoted "typical values" is not fully reproducible from the document. Treat all mechanical/thermal numbers above as post-cured, but with an under-specified cure recipe.

## Discrepancy / do-not-confuse flags

1. **A different, non-water-wash sibling product exists**: SUNLU "ABS-Like Resin" (no "Water Wash" in the name), product page https://www.sunlu.com/products/283, with its own TDS PDFs (https://media.sunlu.com/prod/20260331/cdd6ab82-85d8-49ea-b432-12c6c9c5c005.pdf?filename=TDS and a newer https://media.sunlu.com/prod/20260709/2594071783581670859.pdf?filename=TDS). Its typical values are **different** from the target product: Tensile 28 MPa, Flexural 36 MPa, Elongation 25%, Shore 80 HA/HD, Izod notched 70 J/m, Viscosity 300–700 mPa·s, Mold shrinkage 8%, HDT 50°C. Several early web searches returned this sibling's numbers when queried generically for "SUNLU ABS-like resin" — **do not use these for the water-wash product**.
2. One WebSearch AI-summarized result (source URL not traceable to a single page — appears to be a blended/aggregator answer) reported: solid density 1.16–1.22 g/cm³, liquid density 1.04–1.14 g/cm³, viscosity 200–400 mPa·s @25°C, Tg 44°C, tensile 34 MPa, Young's modulus 1020 MPa, elongation 20%, flexural strength/modulus 36/1070 MPa, Izod 60 J/m, shrinkage 6–8%. These numbers match **neither** the target product's official TDS **nor** the sibling product's TDS exactly. Treated as **unverified/unreliable** and excluded from the table above; flagged here only so it isn't mistaken for a corroborating independent source later.
3. **Internal inconsistency inside the official TDS itself**: the product's whole value proposition and "Product features" text is that it's cleanable with plain tap water ("water-washable... uses tap water instead of alcohol for cleaning... does not require alcohol"), but the same document's "Cleaning method" field under "Others" instructs: *"Use ≥95% clean ethanol or isopropyl alcohol for ultrasonic cleaning for 5–6 minutes."* This directly contradicts the product's core marketing claim and most likely reflects a copy-paste template error (this TDS template — "LCD TDS ISO" — is probably shared across SUNLU's whole resin line and someone failed to swap this one field for the water-wash variant). Flagging rather than silently reconciling; a support-planning engine should not assume this field is reliable for this SKU.
4. **PDF metadata provenance note**: the underlying PDF's embedded document metadata identifies Title = "KINGFA" and Author = "Rao X" (visible in raw PDF properties, not in the rendered text). Kingfa Sci. & Tech. Co. is a large Chinese engineering-plastics/resin manufacturer known to white-label materials for multiple 3D-printing brands. This is circumstantial (metadata only, not a stated OEM relationship anywhere in the visible document) but is worth knowing if cross-referencing against other brands' "ABS-like" resins that may share the same underlying formulation/Kingfa TDS template.

## Independent / third-party test data

**NOT FOUND.** Searched for independent lab reviews (All3DP, general "independent test" / "measured tensile strength" queries). Results found only:
- Manufacturer marketing copy (repeated across sunlu.com, store.sunlu.com, uk.store.sunlu.com blog, retailer listings on Amazon/eBay/Newegg).
- One YouTube video, "Testing SUNLU resin Toughness, ABS, Standard and water washable through the testing gauntlet #3 #RTG" (https://www.youtube.com/watch?v=pZ-Yb5-R4KM) — an informal comparative demo (pull/push/drill-tap tests across SUNLU resin types) with no numeric published tensile/flexural/impact data extracted from the search snippet. Not fetched in full video form (no transcript tool used); if numeric data from this video is needed, it would require watching/transcribing the video directly, which was out of scope here.
- No peer-reviewed, lab-certification, or third-party materials-testing publication was found for this specific SKU.

Amazon listing (https://www.amazon.com/SUNLU-Water-Wash-ABS-Resin-Precision/dp/B0CG5WSGJY) was checked but the fetch only returned page-framework/JS content, not the rendered spec table or review text — inconclusive, not a source for any value above.

## Summary of found vs. not-found

**Found (manufacturer TDS, high confidence):** liquid density, solid/cured density, viscosity @25°C, Shore hardness, tensile strength, elongation at break, flexural strength, flexural modulus, Izod notched impact, HDT, Tg, melting point, decomposition temp, volumetric/mold shrinkage, CTE, curing wavelength, full print-settings block (layer thickness, exposure times, lift height/speed, retract speed, light-off delay), storage conditions, shelf life.

**Not found / not published anywhere:**
- Izod unnotched impact strength
- Any green-state (pre-post-cure) mechanical or physical property
- Post-cure light specification (wavelength/power) — only "3–5 min in a curing chamber," chamber unspecified
- Any independent/third-party lab-measured verification of any manufacturer value
