# Material Data Audit: Elegoo Water-Washable ABS-Like Resin

**Target product:** Elegoo Water-Washable ABS-Like Resin
**Product page:** https://us.elegoo.com/products/elegoo-water-washable-abs-like-resin
**Date researched:** 2026-07-22

## CRITICAL CAVEAT — read before using any numbers below

Elegoo sells (at least) two distinct, similarly-named resins:

1. **"ABS-Like Resin" / "ABS-Like Resin V2.0 / V3.0"** — the original, NOT water-washable
   (requires IPA/alcohol cleanup). This product's spec sheet IS published and easy to
   find (protomont.com reseller page, 3dsourced.com guide).
2. **"Water-Washable ABS-Like Resin"** — the TARGET of this audit, a newer/different
   formulation that cleans up with plain water.

Every AI-generated web-search summary encountered during this research **conflated
these two products** and attributed product #1's numbers (Shore 75D, tensile 30-52 MPa,
flexural modulus 1.882-2.385 MPa, notch impact 41-48 J/m, etc.) to product #2. I
traced those numbers to their source and confirmed twice, independently, that they
belong to product #1, not the water-washable variant:
- protomont.com explicitly identifies its own page as describing "the standard ELEGOO
  ABS-Like Photopolymer 3D Printer Resin (non-water-washable variant)"
  (https://protomont.com/elegoo-abs-like-photopolymer-resin/)
- 3dsourced.com's guide likewise only carries specs for "the standard Elegoo ABS-Like
  Resin (non-washable version)" and states it "does not mention a water-washable
  variant" (https://www.3dsourced.com/3d-printer-materials/abs-like-tough-resins/)

**None of those numbers appear below as Water-Washable ABS-Like Resin data.** Do not
backfill this table with them — they are for a different SKU.

## Bottom line

**No manufacturer TDS or independently-measured mechanical-property numbers for
Elegoo Water-Washable ABS-Like Resin specifically could be located**, despite an
extensive search (official Elegoo storefronts in 4 regions, Elegoo's MSDS/SDS blog
page, Elegoo's resin-settings wiki, Amazon/3DJake/eBay listings, and ~6 independent
review sites). This is itself a finding, not an oversight — see "Search process and
dead ends" below. Every numeric value in the table is marked accordingly.

---

## Data table

| Property | Value | Post-cure or green? | Source | Status |
|---|---|---|---|---|
| Density, liquid (uncured) | NOT PUBLISHED | — | — | Not found |
| Density, cured | NOT PUBLISHED | — | — | Not found |
| Viscosity (cP, at temp) | NOT PUBLISHED (see note) | — | — | Not found / unverified |
| Shore hardness | NOT PUBLISHED | — | — | Not found |
| Tensile strength (MPa) | NOT PUBLISHED | — | — | Not found |
| Elongation at break (%) | NOT PUBLISHED | — | — | Not found |
| Flexural strength (MPa) | NOT PUBLISHED | — | — | Not found |
| Flexural modulus (MPa) | NOT PUBLISHED | — | — | Not found |
| Izod/notch impact strength | NOT PUBLISHED | — | — | Not found |
| Volumetric shrinkage (%) | NOT PUBLISHED | — | — | Not found |
| Curing wavelength | 405 nm (photoinitiator peak, marketing copy only, no band given) | Print-time (UV) | Amazon listing titles (e.g. "405nm Rapid UV-Curing") | Weak / marketing claim only |
| Recommended layer thickness | 0.05 mm cited by one third-party review as a "common starting point" for Elegoo water-washable resins generally — **not resin-specific, not from Elegoo** | Print-time setting | acciyo.com review (https://www.acciyo.com/elegoo-water-washable-resin-review/) | Unverified / generic |
| Recommended exposure time | 2.5–3.5 s normal, 25–45 s bottom layer — same review, described as "common starting points," explicitly NOT resin-specific or manufacturer-sourced | Print-time setting | acciyo.com review | Unverified / generic |
| Measured exposure time (independent test) | 3.6 s normal exposure measured by reviewer on an Elegoo Mars 5 Ultra for "Elegoo Water Washable (White)" — **this is the plain Water Washable resin, not the ABS-Like variant** | Print-time setting | tweakreviews.com (https://tweakreviews.com/printer/elegoo-resin-test) | Found, but wrong SKU |
| Green-state (pre-postcure) properties | NOT PUBLISHED anywhere found — no source discusses green vs. post-cured mechanical properties for this resin at all | — | — | Not found |
| Glass transition temp | NOT PUBLISHED | — | — | Not found |

### Adjacent/reference data points found (NOT this product — do not use as substitutes)

These surfaced during the search and are recorded here only so the numbers are not
mistaken for the target product later. All are for **different SKUs**.

| Property | Value | Product it actually belongs to | Source |
|---|---|---|---|
| Viscosity | 150–250 mPa·s | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Density | 1.05–1.25 g/cm³ | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Shore hardness | 75D | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Tensile strength | 30–52 MPa | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Tensile modulus | 1.779–2.385 GPa (page showed "Mpa" but this is almost certainly a units typo for GPa given the magnitude) | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Flexural modulus | 1.882–2.385 GPa (same units caveat) | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Bending/flexural strength | 40–70 MPa | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Notch impact strength | 41–48 J/m | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Volume shrinkage | 3.72–4.24% | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Linear shrinkage | 1.05–1.35% | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Glass transition temp | 100°C | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Thermal deformation temp | 80°C | Elegoo ABS-Like Resin V3.0 (non-water-washable) | protomont.com |
| Density | 1.1–1.2 g/cm³ | Third-party (3D META, an Australian reseller — NOT Elegoo) generic "Water Washable 3D Printing Resin," ISO 1183 | See "3D META TDS" section below |
| Viscosity @25°C | 100–200 cP, Rotameter Viscometer | Same 3D META third-party document | See below |
| Molding/volumetric shrinkage | 0.2–0.7%, ISO 294 | Same 3D META third-party document | See below |
| Tensile strength | 30–35 MPa, ISO 527 | Same 3D META third-party document | See below |
| Elongation at break | 6–9%, ISO 527 | Same 3D META third-party document | See below |
| Flexural modulus | 2100–2200 MPa, ISO 527 | Same 3D META third-party document | See below |
| Flexural strength | 55–65 MPa, ISO 178 | Same 3D META third-party document | See below |
| Hardness (after curing) | 84 Shore D, ISO 868 | Same 3D META third-party document | See below |
| Curing wavelength | 355–410 nm | Same 3D META third-party document | See below |
| Layer thickness | 0.05 mm | Same 3D META third-party document | See below |
| Exposure time | 1.5–2.5 s (normal), 20–25 s (bottom, ×4–6 layers) | Same 3D META third-party document | See below |

#### On the "3D META TDS" (found via search for Elegoo TDS PDFs on Shopify CDN)

A web search for Elegoo TDS PDFs on `cdn.shopify.com` returned:
`https://cdn.shopify.com/s/files/1/0763/2263/6060/files/Water_Washable_Resin_Technical_Data_Sheet_v1.pdf`

I fetched and read this PDF directly (WebFetch's text summarizer couldn't parse the
binary PDF; I re-read the saved file with the PDF-capable Read tool). **It is branded
"3D META" (info@3dmeta.com.au, an Australian 3D-printing reseller), not Elegoo**, and
covers a generic "Water Washable 3D Printing Resin" — no "ABS-Like" designation
anywhere in the document, no Elegoo branding, no mention of Elegoo. It is plausible
this is 3D META's own TDS for resin they resell (possibly a private-label or the same
OEM base resin Elegoo also sources), but there is no evidence connecting it to
Elegoo's own formulation, and it is explicitly NOT the ABS-Like variant. **I am not
using these numbers as Elegoo Water-Washable ABS-Like Resin data** — they're recorded
above purely as a nearby-adjacent, differently-sourced reference point, in case it's
useful as a rough proxy for "a generic water-washable resin" but it must not be
attributed to Elegoo or to the ABS-Like SKU in any downstream table.

---

## Search process and dead ends (why the table above is mostly "not found")

1. **Primary product page** (`us.elegoo.com/products/elegoo-water-washable-abs-like-resin`)
   — fetched multiple times. It is a Shopify SPA; WebFetch retrieves only navigation
   chrome, product images, and color-variant names. No numeric spec table, no TDS
   link, appears in the fetched HTML/markdown. This was consistent across the
   product's US, EU, and UK regional URLs, and the 12kg-bundle variant page.
2. **Elegoo's central MSDS/SDS blog page**
   (`https://eu.elegoo.com/en-at/blogs/3d-printing/elegoo-resin-sds`, fetched
   successfully — the `www.elegoo.com` version 403'd) lists MSDS/REACH/ROHS reports
   for: Water Washable Resin (standard), Standard Photopolymer Resin, ABS-Like
   Photopolymer Resin, and Plant-based Photopolymer Resin. **There is no separate
   entry for "Water-Washable ABS-Like Resin."** This is a direct, confirmed finding
   that Elegoo has not published a distinct compliance/safety doc for this SKU on
   that page, at least not one that's linked there.
3. **Elegoo's resin-settings wiki** (`wiki.elegoo.com/resin-settings`) — WebFetch
   could not retrieve usable content (repeatedly returned as truncated with no
   table data recovered, across three different prompt phrasings). Could not confirm
   or deny an entry for this resin.
4. **Elegoo's general download center** (`www.elegoo.com/pages/download`) — 403
   Forbidden on every attempt.
5. **Scribd MSDS document** ("ELEGOO-ABS-Like-Photopolymer-Resin-MSDS-Report") —
   title suggests it's for the non-water-washable ABS-Like resin (matches the
   Elegoo SDS page's "ABS-Like Photopolymer Resin" naming); Scribd's viewer blocked
   content extraction (paywalled/JS-gated), so this could not be confirmed either way
   and no numbers were retrieved.
6. **Retailer listings** (Amazon x2, 3DJake, eBay, Newegg, Adorama) — Amazon fetches
   returned 500 errors on both attempted ASINs (bot-blocked); 3DJake carries the
   plain "Water Washable Resin" (not ABS-Like) product line, not this SKU; none of
   the retailer search snippets surfaced numeric specs for this exact product.
7. **Independent reviews searched**: essentialgrade.com (403 Forbidden, twice),
   tweakreviews.com (tested plain "Water Washable" white resin, not ABS-Like — no
   quantitative mechanical data, only a qualitative flex/drop test and one measured
   exposure time of 3.6s for the wrong SKU), acciyo.com (no independent measurements,
   only generic "common starting point" print settings sourced from community
   consensus, explicitly not resin- or manufacturer-specific), 3dsourced.com and
   openelab.io buying guides (both explicitly state they have no separate
   water-washable ABS-Like entry and direct readers back to the product's own
   page/SDS/TDS — which, per points 1–4, doesn't surface anything).
8. **Wayback Machine** — WebFetch is blocked from `web.archive.org` in this
   environment, so no archived snapshot of the product page could be checked.
9. Browser automation (`claude-in-safari` MCP tools) was available but non-functional
   in this session — every `navigate` call failed with `tab_not_found` immediately
   after tab creation, so I could not load the JS-rendered product page in an actual
   browser to bypass WebFetch's static-HTML limitation.

## Sources consulted (full list)

- https://us.elegoo.com/products/elegoo-water-washable-abs-like-resin (target page — no specs surfaced)
- https://eu.elegoo.com/en-es/products/elegoo-water-washable-abs-like-resin (no specs surfaced)
- https://www.elegoo.com/collections/water-washable-resins/products/water-washable-abs-like-resin (no specs surfaced)
- https://www.elegoo.com/products/water-washable-abs-like-resin (403 Forbidden)
- https://us.elegoo.com/products/water-washable-abs-like-resin-colored-12kg (no specs surfaced)
- https://uk.elegoo.com/products/water-washable-abs-like-resin-colored-5-kg (no specs surfaced)
- https://eu.elegoo.com/en-pl/products/water-washable-abs-like-resin-colored-12-kg (no specs surfaced)
- https://eu.elegoo.com/en-at/blogs/3d-printing/elegoo-resin-sds (confirmed: no distinct MSDS entry for this SKU)
- https://www.elegoo.com/blogs/3d-printing/elegoo-resin-sds (403 Forbidden)
- https://wiki.elegoo.com/resin-settings (could not retrieve table content)
- https://www.elegoo.com/pages/download (403 Forbidden)
- https://protomont.com/elegoo-abs-like-photopolymer-resin/ (confirmed OFF-TARGET product — non-water-washable ABS-Like V3.0 specs)
- https://www.3dsourced.com/3d-printer-materials/abs-like-tough-resins/ (confirms off-target specs belong to non-washable resin; no water-washable data)
- https://openelab.io/blogs/learn/elegoo-resin-buying-guide (no water-washable ABS-Like specs; defers to product page/SDS/TDS)
- https://the3dprinterbee.com/elegoo-resin-settings/ (qualitative note only: "particularly thin... requires longer curing time"; no numbers for this SKU)
- https://tweakreviews.com/printer/elegoo-resin-test (tested plain Water Washable resin, not ABS-Like; 3.6s exposure measured, drop-test qualitative results)
- https://www.acciyo.com/elegoo-water-washable-resin-review/ (generic community exposure-time ranges, not resin-specific; no mechanical data)
- https://www.essentialgrade.com/elegoo-water-washable-abs-like-resin-2000g-review-the-ultimate-game-changer-for-clean-and-durable-3d-prints/ (403 Forbidden, could not access)
- https://www.scribd.com/document/813298073/ELEGOO-ABS-Like-Photopolymer-Resin-MSDS-Report (content blocked/paywalled; title suggests non-water-washable variant)
- https://mydatasheet.b-cdn.net/posts/elegoo-water-washable-resin-datasheet/ (low-quality SEO page, describes what a datasheet "should" contain, zero actual numbers)
- https://cdn.shopify.com/s/files/1/0763/2263/6060/files/Water_Washable_Resin_Technical_Data_Sheet_v1.pdf (real TDS but branded 3D META, an Australian reseller — not Elegoo, not ABS-Like; recorded as adjacent reference only, see above)
- https://www.amazon.com/ELEGOO-Water-Washable-ABS-Like-Printer/dp/B0DFWBHH85 (500 error)
- https://www.amazon.com/ELEGOO-Water-Washable-ABS-Resin/dp/B0D5MBZLC8 (500 error — this ASIN is the 8K variant, also not confirmed identical to target SKU)

## Recommendation for the support-generation engine

Treat Elegoo Water-Washable ABS-Like Resin as **unquantified** in this dataset.
Do not substitute the non-water-washable ABS-Like V3.0 numbers (Shore 75D, tensile
30-52 MPa, etc.) — that is a materially different resin chemistry (different
cleaning solvent requirement is itself evidence of a different monomer/oligomer
system) and using its numbers would misrepresent the target resin's real mechanical
behavior, particularly if the water-washable formulation trades some toughness for
water-solubility of the uncured resin (a common formulation trade-off in this resin
class, though I found no source confirming or quantifying that trade-off for this
specific product). If real numbers are required to seed the support-generation
model, the closest available proxy is the 3D META third-party TDS above (density
1.1-1.2 g/cm³, tensile 30-35 MPa, elongation 6-9%, flexural modulus 2100-2200 MPa,
flexural strength 55-65 MPa, 84 Shore D) for a *generic* water-washable resin — but
flag it in the engine as unverified/non-Elegoo-sourced, not as this product's TDS.
