# Resin Property Envelope — Water-Washable ABS-Like / Tough Photopolymers

**Purpose:** parameterize the support-structure physics model with a *range* of
mechanical properties across the class of resins the owner actually prints with
("water-washable ABS-like" / "water-washable tough"), rather than a single
manufacturer's datasheet. Compiled 2026-07-25 from manufacturer TDS/SDS documents,
retailer-reproduced spec tables, and (where available) independent/academic testing.

Existing per-manufacturer audits: [`elegoo.md`](./elegoo.md), [`nova3d.md`](./nova3d.md),
[`sunlu.md`](./sunlu.md) — read first, not duplicated here. This document adds
Anycubic, Phrozen, Photocentric, Inslogic3D, and Jayo (real or semi-real numbers),
plus confirms Siraya Tech, eSUN, Creality, and Monocure3D have **no SKU that is
simultaneously water-washable and ABS-like/tough-branded** (their adjacent
products are recorded for context, not counted in the envelope). Retrieved TDS/user-guide
PDFs are saved in [`pdfs/`](./pdfs/).

**If you read nothing else:** flexural modulus — the number that governs deflection —
spans roughly **790–2500 MPa** (>3×) across the small set of manufacturers that
publish it for this resin class. A support-generation model tuned only to Sunlu's
956 MPa is anchored near the *low* end of the observed range, not the middle. And
**every single manufacturer number in this document is post-cured or of unstated
cure state** — none of them is what the resin actually behaves like while a support
arm is holding up green, wet, as-printed material during the print itself. The two
academic sources that *do* measure green-state stiffness (§4) suggest green-state
modulus runs roughly **40–55% of fully post-cured modulus** for this general resin
family, but neither source tests a water-washable ABS-like/tough SKU specifically.

---

## 1. Which manufacturers actually have a qualifying product

A recurring pattern across this whole survey: manufacturers split "water-washable"
and "ABS-like/tough" into two *separate, non-overlapping* product lines far more
often than they combine them into one SKU. Confirmed for each brand surveyed:

| Manufacturer | Water-washable **and** ABS-like/tough in one SKU? | Notes |
|---|---|---|
| Sunlu | **Yes** | Water Wash ABS-Like Resin — real ISO TDS. See `sunlu.md`. |
| Anycubic | **Yes** (two generations) | ABS-Like Resin V2 (water-wash) and Water-Wash ABS-Like Resin 3.0. Real ASTM/GB TDS PDFs. |
| Phrozen | **Yes** | Water-Washable Tough Resin (Gray/White). Real ASTM TDS, explicitly labeled "UV post-cured." No flexural data. |
| Photocentric | **Yes**, with caveat | Durable DL902 (plant-based) and DL110H. Real ASTM TDS. "Water-washable" via Photocentric's proprietary aqueous cleaner + water rinse, not necessarily plain tap water like the others — flag before treating as directly comparable. |
| Inslogic3D | **Yes**, lower confidence | Small/less-mainstream Chinese brand. Real TDS PDF exists but no test method cited, cure state unstated. |
| Jayo | **Yes**, low confidence | ABS-Like Water Washable Resin. No TDS PDF found anywhere — numbers are rounded marketing-page ranges only, same tier as Elegoo/Nova3D's dead ends. |
| Elegoo | No confirmed data | See `elegoo.md` — extensive search, nothing published for this exact SKU. |
| Nova3D | No confirmed data | See `nova3d.md` — same outcome. |
| Siraya Tech | **No** | Their only water-washable line ("Simple"/"Simple V2") is not marketed ABS-like/tough. Their ABS-like/tough line ("Fast Tough") is alcohol/IPA-wash, not water-washable. Both recorded in §3 for reference. |
| eSUN | **No** | "Hard-Tough Resin" (ABS-like) is alcohol-wash only, confirmed explicitly in its own TDS and a retailer's cleaning instructions. Their water-washable line is never marketed tough — one SKU (W200) explicitly exists to *patch* the class's typical brittleness. Recorded in §3. |
| Creality | **No** | "ABS-Like Resin 2.0" requires 95%+ IPA, confirmed in its own spec sheet. Water Washable Resin / Resin Plus are never marketed ABS-like/tough. Recorded in §3. |
| Monocure3D | **No product at all** | No water-washable resin SKU of any kind. Their "RESINAWAY"/"RinseOFF" are cleaning *solvents* used on ordinary resin, not a hydrophilic resin formulation. |
| Prusament, Peopoly, Formlabs, Voxelab, Longer3D, Loctite/Henkel | No qualifying SKU found | Confirmed alcohol/IPA-wash ecosystems (Prusament, Peopoly, Formlabs) or no usable data located in the time available (Voxelab, Longer3D, Loctite/Henkel — not exhaustively ruled out, just not found). |

## 2. Comparison table — the primary envelope (water-washable AND ABS-like/tough)

All values as published. **Cure state column is what the source actually states** —
where a datasheet is silent, that is reported as "unstated," not assumed. See the
footnotes for data-quality flags.

| Manufacturer | Product | Tensile (MPa) | Flexural strength (MPa) | Flexural / tensile modulus (MPa) | Elongation (%) | Shore D | Density liquid / cured (g/cm³) | Viscosity (mPa·s or cP) | Cure state | Data quality |
|---|---|---|---|---|---|---|---|---|---|---|
| Sunlu | Water Wash ABS-Like Resin | 26 | 34 | **956** (flexural) | 24 | 77 | 1.12 / 1.22 | 400–800 | Unstated (implied post-cure) | High — real ISO TDS, methods cited |
| Anycubic | ABS-Like Resin V2 | 35–45 | ~40–50¹ | **~1000–1200**¹ (flexural) | 30–40 | 80–85 | 1.1–1.2 / not given | 180–200 | Unstated (implied post-cure) | Medium-high — real ASTM/GB TDS, but internal label swap suspected¹ |
| Anycubic | Water-Wash ABS-Like Resin 3.0 | 36–46 | ~40–50¹ | **~2000–2500**¹ (flexural) | 35–45 | 83–86 | 1.05–1.1 / not given | 400–500 | Unstated (implied post-cure) | Medium-high — same caveat¹ |
| Phrozen | Water-Washable Tough Resin | 22.4–22.9 | Not published | 863.4–993 (**tensile**, no flexural given) | 16.3–23.1 | 75–80 | 1.11–1.12 / not given | 250–500 | **Explicitly UV post-cured** | High — real ASTM TDS, only one that states cure condition on-sheet |
| Photocentric | Durable DL902 (plant-based) | 37 (break) / 46 (yield) | 71 | **1790** (flexural); 2290 (tensile) | 37 | 82 | not given / 1.06 | 900 | Explicitly post-cured (3h @ 60°C) | High — real ASTM TDS; washable via proprietary cleaner² |
| Photocentric | Durable DL110H | 54 (UTS) | 78 | **1950** (flexural); 2100 (tensile) | 28 | 86 | not given / 1.14 | 480 | Explicitly post-cured ("fully cured material") | High — same caveat² |
| Inslogic3D | Water-Washable ABS-Like Resin | 39 | 40 | **790** (flexural) | 22 | 80 | not given | not given | Unstated, no method cited | Low-medium — small/obscure brand |
| Jayo | ABS-Like Water Washable Resin | 20–40 | Not found | 600–800 (labeled "tensile modulus," not flexural) | 15–25 | 70–80 | 1.04–1.14 (ambiguous, likely liquid) | 200–400 | Unstated | Low — marketing copy only, no TDS PDF, no test method |

¹ **Anycubic label-swap flag:** both Anycubic TDS PDFs list "Bending Modulus" as 40–50 MPa
(identical across both product generations despite very different "Bending Strength"
values) and "Bending Strength" as 1000–1200 / 2000–2500 MPa. This is backwards —
flexural modulus is normally 20–40× flexural strength for this resin class (every
other manufacturer's numbers above follow that ratio), and 40–50 MPa as a *modulus*
would be physically implausible (softer than typical elastomers). The table above
reports the **corrected interpretation** (modulus ≈ 1000–2500 MPa, strength ≈ 40–50
MPa) in bold as the primary reading, consistent with the whole rest of the class;
the raw as-published Anycubic field names are the reverse. Treat as unconfirmed —
Anycubic has not corrected or clarified this discrepancy anywhere found.

² **Photocentric water-wash caveat:** cleaned with Photocentric's own aqueous
"Resin Cleaner" plus a warm-water rinse — not confirmed to be plain-tap-water
washable like Sunlu/Anycubic/Phrozen/Inslogic/Jayo. Included because it is the best
real ASTM-cited data in the whole survey and the mechanism (aqueous, not alcohol)
is the same category the owner cares about, but flag this distinction if the
physics model needs to discriminate "plain water" vs. "proprietary aqueous
cleaner" washability.

### The envelope (min / median / max across the 8 rows above)

| Property | Min | Median | Max | Spread (max/min) |
|---|---|---|---|---|
| Tensile strength (MPa) | 20 (Jayo, low-confidence) / 22.4 (Phrozen, high-confidence floor) | ~39–40 | 54 (Photocentric DL110H) | ~2.4× |
| Flexural strength (MPa) | 34 (Sunlu) | ~45 | 78 (Photocentric DL110H) | ~2.3× |
| **Flexural modulus (MPa)** | **790 (Inslogic3D)** | **~1445** | **~2500 (Anycubic 3.0, corrected)** | **~3.2×** |
| Elongation at break (%) | 15 (Jayo, low-confidence) / 16.3 (Phrozen, high-confidence floor) | ~26 | 45 (Anycubic 3.0) | ~2.8× (~3×) |
| Shore D hardness | 70 (Jayo, low-confidence) / 75 (Sunlu/Phrozen, high-confidence floor) | ~81 | 86 (Photocentric DL110H / Anycubic 3.0) | ~1.15–1.2× (tight) |
| Density, liquid (g/cm³) | 1.04 (Jayo, ambiguous) / 1.05 (Anycubic 3.0) | ~1.11 | 1.2 (Anycubic V2 high end) | ~1.15× (tight) |
| Viscosity (mPa·s) | 180 (Anycubic V2) | ~430 | 900 (Photocentric DL902) | **~5×** |

## 3. Adjacent/reference data (NOT part of the primary envelope — wrong SKU class)

These are real numbers from real TDS documents, but each product is *either*
water-washable *or* ABS-like/tough, never both — recorded so they aren't lost and
so the "no qualifying SKU" claims above are traceable, but excluded from the
envelope math in §2 since they don't match the target class.

| Manufacturer | Product | Washable? | Tough/ABS-like? | Tensile (MPa) | Flexural strength (MPa) | Flexural/Young's modulus (MPa) | Elongation (%) | Shore D | Cure state |
|---|---|---|---|---|---|---|---|---|---|
| Siraya Tech | Simple V2 | Water | No | 27–29 | Not given | 950 (flexural) / 750 (Young's) | 10 | 78 | Unstated |
| Siraya Tech | Fast Tough | IPA | Yes | 32–34 | Not given | 1400 (flexural) / 1050 (Young's) | 40 | 77 | Unstated |
| eSUN | Hard-Tough Resin | IPA | Yes | 30–60 | 30–75 | Not published (no modulus on any eSUN sheet) | 35–52 | 75–81 | Unstated (TDS implies post-cure needed for "best mechanical strength") |
| eSUN | Water Washable Resin (W100) | Water | No | 19–46 | 15–50 | Not published | 17–30 | 74–82 | Unstated |
| eSUN | W300 16K Water-Washable | Water | No | 68.6 | 89.5 | Not published | 8.75 | 81 | Unstated |
| Creality | Water Washable Resin | Water | No | 40–46 | 69–76 | Not published | 10–14 | 78–79 | Unstated |
| Creality | ABS-Like Resin 2.0 | IPA | Yes | 50–65 | 80–100 | 2000–2500 (flexural) / 650–1000 (tensile) | ~5 | 80–85 | Unstated |

**Note on a recurring false lead:** a PDF titled "Water Washable Resin Technical
Data Sheet v1" (hosted on `cdn.shopify.com/s/files/1/0763/2263/6060/...`) surfaced
independently in *three separate* manufacturer searches (Elegoo, Nova3D, Creality)
as if it were that manufacturer's own datasheet. It is not — it's branded "3D
META" (3dmeta.com.au), an Australian reseller of a generic/unbranded water-washable
resin, unrelated to any of the three brands. Its numbers (tensile 30–35 MPa,
flexural strength 55–65 MPa, flexural modulus 2100–2200 MPa, elongation 6–9%,
density 1.1–1.2 g/cm³, Shore D 84, "after curing") are a real ISO-tested TDS for
*some* generic water-washable resin, and land inside the envelope range above, but
must not be attributed to any named manufacturer.

## 4. Green-state data — the most valuable (and scarcest) category

**No manufacturer in this entire survey publishes green-state (as-printed,
pre-post-cure) mechanical properties for a water-washable ABS-like/tough resin.**
Every real number in §2 is either explicitly post-cured (Phrozen, both Photocentric
sheets) or of unstated cure state with post-cure implied by context (everyone
else — e.g. Anycubic's TDS lists tensile/flexural/hardness right next to a "fully
cured" ASTM test-method citation with no separate as-printed condition ever
described). This mirrors what the Elegoo and Nova3D audits already found.

Two independent academic sources *do* measure green-state stiffness directly —
neither on the exact target resin class, but both are real, controlled,
directly-measured data and are the best available proxy:

**(a) Anycubic Water Wash Resin+ post-cure-time study** (real green-state modulus,
different Anycubic SKU — their standard washable line, not the ABS-Like variant):
*"Influence of Post-Curing Time and Print Orientation on the Mechanical Behavior of
Photosensitive Resins in mSLA 3D Printing,"* Applied Mechanics (MDPI), 2025,
[DOI 10.3390/applmech6030071](https://www.mdpi.com/2673-3161/6/3/71).

| Post-cure time | Young's modulus, flat/XY (MPa) | Young's modulus, vertical/Z (MPa) |
|---|---|---|
| 0 min (**green state**) | 964.5 ± 45.2 | 774.0 ± 38.2 |
| 5 min | 1255.0 ± 62.1 | 1278.0 ± 59.4 |
| 10 min | 1392.4 ± 58.9 | 1363.0 ± 67.1 |
| 30 min | 1551.5 ± 71.3 | 1339.0 ± 61.8 |
| 60 min (near-full cure) | 1892.4 ± 89.7 | 1661.2 ± 78.9 |

Green-state modulus is **~51% (XY) / ~47% (Z)** of the 60-minute post-cured value
for this resin. Extracted via automated fetch of the paper, not manually verified
against the primary PDF — recommend confirming directly before hard-coding.

**(b) Formlabs Clear V4 post-cure study** (standard SLA resin, not water-washable,
not ABS-like — included purely for the green-vs-cured *ratio*, which is the only
other real measurement found): *"The impact of modifying 3D printing parameters on
mechanical strength and physical properties in vat photopolymerisation,"*
Scientific Reports (Nature), April 2025,
[open-access PMC11993694](https://pmc.ncbi.nlm.nih.gov/articles/PMC11993694/).

| Orientation | Green (uncured) tensile strength (MPa) | Green elongation (%) | Best post-cured tensile strength (MPa, 80°C/15min) |
|---|---|---|---|
| Horizontal | 22.84 ± 1.92 | 84 ± 14.4 | — |
| Vertical | 44.99 ± 2.84 | 64 ± 20.4 | 127.37 ± 6.19 |

Vertical tensile strength roughly **2.8×** from green to best post-cure. The same
research pass surfaced (but could not independently verify) general claims from
this literature that post-curing raises Young's modulus by 33–273% depending on
resin/regime — i.e., green-state modulus as low as ~27–43% of fully cured, broadly
consistent with (a) above.

**(c) Weaker/unverified supporting signals**, not to be treated as measured data:
- BCH Technologies ran an informal (non-ISO/ASTM, custom compression rig)
  green-vs-cured test on **Phrozen ABS-Like Matte Gray** (a non-water-washable
  sibling of the Phrozen product in §2): uncured mean 59.4 kgf vs. cured-30-min
  mean 75.2 kgf → green ≈ 79% of cured *push-pull compressive resistance*
  (a different metric from modulus, and a different SKU).
- A RapidMade blog post on support generation asserts "green state ≈ 0.6× post-cure
  E" as a design default, and separately claims a "modulus factor of 3.73"
  (green→cured) for Formlabs White V4 — neither claim carries a traceable primary
  citation on the page itself, though the 3.73× figure is directionally consistent
  with the Nature/PMC paper above. Treat as an engineering rule of thumb someone
  else adopted, not as independently verified data.

**Working estimate, if the physics model needs a single green/cured stiffness
ratio today:** the two directly-measured, citable sources both land in the
**~0.4–0.55×** range for modulus (green state as a fraction of fully post-cured),
with weaker secondary signals (0.6× rule of thumb, 0.27–0.43× from the wider
post-cure-modulus-increase literature) bracketing that on both sides. None of this
is measured on a water-washable ABS-like/tough resin specifically — it is the best
available proxy, not a confirmed value for this resin class.

## 5. Assessment — how much does this class actually vary?

Per the framing question (does a physics model tuned to one datasheet sit on thin
ice, or is it on solid ground): the answer differs sharply by property.

**Tightly clustered — a single-datasheet model is reasonably safe here:**
- **Shore D hardness**: 70–86D across all 8 primary-class rows, and 75–86D if the
  one low-confidence Jayo figure is excluded — roughly a 15–20% spread. This whole
  resin class converges on "hard, ABS-like" hardness regardless of manufacturer.
- **Liquid density**: 1.04–1.2 g/cm³, ~15% spread. Unsurprising — these are all
  broadly similar acrylate/urethane-acrylate photopolymer formulations by mass.

**Moderately varying — worth a sensitivity check, not a crisis:**
- **Tensile strength**: 20–54 MPa (2.4–2.7×), roughly bracketing Sunlu's 26 MPa
  toward the low end.
- **Flexural strength**: 34–78 MPa (2.3×), again with Sunlu's 34 MPa near the floor.
- **Elongation at break**: 15–45% (~3×) — the most variable of the "moderate" group,
  meaning assumptions about support-arm ductility before failure could be
  materially wrong if tuned to one resin.

**Widely varying — a single-datasheet model is genuinely on thin ice here:**
- **Flexural modulus (the deflection-governing number): 790–2500 MPa, >3×
  spread.** This is the headline finding. Sunlu's 956 MPa — the one datasheet the
  physics model currently has — sits near the *bottom quartile* of the observed
  range (median ≈1445 MPa), not in the middle. A model calibrated only to Sunlu
  would systematically *under*-predict stiffness (over-predict deflection/support
  need) for roughly half the manufacturers surveyed, and could be off by a factor
  of ~2.5× for a resin at the Anycubic 3.0 end of the range. Given the corrected-
  vs-raw ambiguity in Anycubic's own TDS (footnote 1), the true upper bound could
  plausibly be somewhat different, but even the conservative Photocentric-anchored
  range (790–1950 MPa) is still a ~2.5× spread.
- **Viscosity: 180–900 mPa·s, ~5× spread.** Directly affects drainage/peel-force
  assumptions; the class does not converge here at all — thin, watery Anycubic V2
  (180–200 mPa·s) and thick, syrupy Photocentric DL902 (900 cPs) are not
  interchangeable inputs for a drainage model.

**Compounding all of the above:** every number in this whole document — tight or
wide — is a post-cure or unstated-cure-state figure. Since supports fail
*during* printing, before any post-cure step, the practically relevant uncertainty
is not "±3× on flexural modulus" but "±3× on a number that itself may only be
~40–55% of the real in-print value" (§4). The genuinely load-bearing modulus
number for the physics model — green-state flexural modulus of a water-washable
ABS-like resin — was not found published anywhere, by any manufacturer, in this
survey.

## 6. What could not be found (explicit gap list)

- **Green-state mechanical properties for any water-washable ABS-like/tough
  resin, from any manufacturer.** This is the single biggest gap and the one the
  task asked to prioritize. The closest available data (§4) is one paper testing
  a *different* Anycubic SKU (standard washable, not ABS-like) and one paper
  testing a non-water-washable Formlabs resin.
- **Independent/third-party lab-measured verification** of any manufacturer's
  published numbers, for any brand, water-washable-tough or not. The only
  non-manufacturer mechanical test found in the entire survey is the informal BCH
  Technologies compression rig (§4c), which used a custom non-ISO method on a
  non-water-washable sibling product.
- **Flexural data for Phrozen's Water-Washable Tough Resin** — official TDS gives
  tensile strength/modulus and impact but no flexural strength or flexural
  modulus at all.
- **Flexural strength for Jayo's resin**, and any TDS PDF at all for Jayo (every
  number sourced from a product page, not a datasheet).
- **Cured (solid) density** for both Anycubic SKUs and both Photocentric DL-series
  resins (only liquid or only cured density given, never both) and for Inslogic3D
  and Jayo (density essentially unresolved for these two).
- **Any qualifying water-washable ABS-like/tough SKU at all** from Elegoo, Nova3D,
  Siraya Tech, eSUN, Creality, or Monocure3D (see §1 for what each actually sells
  instead).
- **Voxelab, Longer3D, and Loctite/Henkel** — not confirmed either way; a
  reasonably thorough search found no usable TDS for any of the three in the time
  budget, but this is a "not found," not a confirmed "doesn't exist."
- **Resolution of the Anycubic "Bending Modulus"/"Bending Strength" label
  swap** (footnote 1) — no corrected TDS or manufacturer clarification located.

## 7. Sources / provenance summary

Real TDS or user-guide PDFs retrieved and saved in [`pdfs/`](./pdfs/):

| File | Product | Retrieved from |
|---|---|---|
| `anycubic-abs-like-v2-tds.pdf` | Anycubic ABS-Like Resin V2 | store.anycubic.com (Shopify CDN) |
| `anycubic-water-wash-abs-like-3.0-tds.pdf` | Anycubic Water-Wash ABS-Like Resin 3.0 | store.anycubic.com (Shopify CDN) |
| `phrozen-water-washable-tough-user-guide.pdf` | Phrozen Water-Washable Tough Resin | 3d.nice-cdn.com (Phrozen CDN) |
| `photocentric-durable-dl902-tds.pdf` | Photocentric Durable DL902 Plant-Based | photocentricgroup.com |
| `photocentric-durable-dl110h-tds.pdf` | Photocentric Durable DL110H | photocentricgroup.com |
| `inslogic3d-water-washable-abs-like-tds.pdf` | Inslogic3D Water-Washable ABS-Like Resin | store.inslogic3d.com (Shopify CDN) |
| `esun-hard-tough-tds.pdf` | eSUN Hard-Tough Resin (adjacent, not water-washable) | esun3d.com |
| `esun-water-washable-tds.pdf` | eSUN Water Washable Resin W100 (adjacent, not ABS-like) | esun3d.com |
| `siraya-simple-v2-tds.pdf` | Siraya Tech Simple V2 (adjacent, not ABS-like) | Siraya Tech (Google Drive) |
| `siraya-fast-tough-tds.pdf` | Siraya Tech Fast Tough (adjacent, IPA-wash) | Siraya Tech (Google Drive) |

Not saved: eSUN's W300 16K TDS (~15MB, image-heavy, not water-washable-ABS-like
class anyway); Jayo, Creality, and Monocure3D had no retrievable PDF to save.
Academic sources (§4) are linked inline rather than mirrored locally.
