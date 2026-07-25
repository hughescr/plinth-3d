# Nova3D Water-Washable ABS-Like Tough Resin — Materials Data Audit

**Target product:** Nova3D Water Washable 3D Printer Resin, "Tough Washable Resin" (ABS-like, water-washable)
**Product page:** https://store.nova3dp.com/products/nova3d-water-washable-abs-like-3d-printer-resin-tough-washable-resin (mirrored at https://www.nova3dp.com/products/nova3d-water-washable-abs-like-3d-printer-resin-tough-washable-resin)
**Audit date:** 2026-07-22
**Bottom line:** Nova3D does **not** publish a numeric TDS/spec table for this product anywhere we could find — on their own site, on retailer listings, or in independent reviews. Every quantitative mechanical/physical property below is **NOT FOUND**. This is the headline finding, not a gap to be filled with estimates.

---

## 1. Property table

All rows are POST-CURED unless noted (Nova3D publishes no green-state data at all — see §4).

| Property | Value | Units | Source | Status |
|---|---|---|---|---|
| Density — liquid (uncured) | — | g/cm³ | — | **NOT FOUND** |
| Density — cured | — | g/cm³ | — | **NOT FOUND** |
| Viscosity | — | cP (at stated temp) | — | **NOT FOUND** (page claims qualitative "low viscosity" only, no number) |
| Shore hardness | — | Shore A/D | — | **NOT FOUND** |
| Tensile strength | — | MPa | — | **NOT FOUND** |
| Elongation at break | — | % | — | **NOT FOUND** |
| Flexural strength | — | MPa | — | **NOT FOUND** |
| Flexural modulus | — | MPa | — | **NOT FOUND** |
| Izod / impact strength | — | J/m or kJ/m² | — | **NOT FOUND** |
| Volumetric shrinkage | — | % | — | **NOT FOUND** (see §3 re: an unverifiable "2.5%" figure) |
| Recommended layer exposure / print settings | — | s, mm | — | **NOT FOUND** (no settings table published for this specific SKU; see §3 for a settings-matrix dead end) |
| Cure wavelength | 405 nm (compatibility spec, not a cure-quality parameter) | nm | Amazon listing title, https://www.amazon.com/NOVA3D-Washable-ABS-Like-Toughness-Photopolymer/dp/B0FR4SPGMF (indexed via search; direct fetch failed, see §2) | Found, but only as a printer-compatibility bullet ("405nm UV Fast Cure for 12K/14K/LCD/DLP/SLA Printers"), not a cure-parameter spec |
| Green-state (pre-post-cure) properties | — | — | — | **NOT FOUND** — no green-state data of any kind published |

---

## 2. Sources checked and what each one actually had

| # | Source | URL | What it had |
|---|---|---|---|
| 1 | Nova3D US store, product page | https://store.nova3dp.com/products/nova3d-water-washable-abs-like-3d-printer-resin-tough-washable-resin | Only marketing bullets (water-washable, "exceptional toughness," "accurate dimensions," impact-absorbing, low-odor). Product currently shows **sold out**, price $59.99 (sale $32.99). Confirmed by two independent full-page fetches, one specifically grepping for "%", "shrinkage", "density", "viscosity", "cps", "g/cm", "hardness", "MPa", "cm3" — **none present anywhere in the page text**. No PDF/TDS/SDS link on the page. |
| 2 | Nova3D CN site, same product | https://www.nova3dp.com/products/nova3d-water-washable-abs-like-3d-printer-resin-tough-washable-resin | Renders as nav/menu shell only when fetched — no spec content reachable this way. |
| 3 | Nova3D water-washable resin category page (CN) | https://www.nova3dp.com/product/cate_29.html | Lists 4 washable-resin SKUs (incl. 水洗高韧树脂 = Tough Washable Resin) with **no spec tables**; purchase links route to Tmall (`detail.tmall.com/item.htm?id=617290162702`), which typically renders specs as images, not fetchable text. |
| 4 | Nova3D download center | https://www.nova3dp.com/support/download.html | Page shell only fetchable; couldn't enumerate actual file listing through WebFetch (JS-rendered). Printer *user manuals* (PDF) are confirmed to exist on nova3dp.com's CDN (e.g. GEM3/Whale4 manuals), so the CDN does host real PDFs — but no resin TDS/SDS filename surfaced in search or direct navigation. |
| 5 | "specs" URL guessed from a stray search hit | https://www.nova3dp.com/products/specs-1498.html | **404** — dead link, not a real specs page. |
| 6 | Amazon listing (primary US SKU, 1kg grey) | https://www.amazon.com/NOVA3D-Washable-ABS-Like-Toughness-Photopolymer/dp/B0FR4SPGMF | WebFetch returned **HTTP 500** on 3 separate attempts — page not directly fetchable. WebSearch's indexed snippet of this page surfaced only qualitative bullets ("Toughness & Impact Resistance," "Water-Washable & Easy Post-Processing," "High Precision & High Success Rate") plus **one numeric claim, "2.5% Low Shrinkage"** — see caveat in §3, this could not be independently confirmed and may belong to a different Nova3D SKU. |
| 7 | Amazon listing (500g grey SKU) | https://www.amazon.com/NOVA3D-Washable-Toughness-Photopolymer-Precision/dp/B0CQJPN4SC | HTTP 500 on fetch — not retrievable. |
| 8 | WoodArtSupply retailer listing | https://woodartsupply.com/products/nova3d-water-washable-abs-like-tough-resin-high-toughness-3d-printing-photopolymer-resin-405nm-uv-fast-cure-for-12k-14k-lcd-dlp-sla-printers-low-odor-grey-1kg | Marketing copy only ("low viscosity" qualitative, no number). No spec table. |
| 9 | Newegg listing | https://www.newegg.com/p/298-00MF-00098 | **404** — not a valid listing for this product. |
| 10 | Gray Scalp Miniatures independent review | https://www.grayscalpminiatures.com/blog-3/nova3d-tough-washable-resin-review-for-3d-printing-miniatures | Real independent test data, but **qualitative only**: drop tests from 20", 34", 62" onto miniatures — no damage at any height, all fragile parts (thin legs/tails/weapon connections) survived. Reviewer's comparison: this resin beat Sunlu ABS-Like (12/15 drop-test pass rate) and was comparable to FauxHammer WarGamer resin. **No density/viscosity/hardness/tensile/elongation/shrinkage/exposure numbers reported.** |
| 11 | YouTube reviews (2 videos found) | https://www.youtube.com/watch?v=ptuE1SQ29i4 , https://www.youtube.com/watch?v=ZnwkNBmaQws | Descriptions not extractable via WebFetch (truncated/nav-only content returned). Not used as a source. |
| 12 | makertrainer.com resin settings wiki | https://makertrainer.com/wiki/Nova3D_materials_resin_settings_matrix | **Could not fetch** — TLS certificate error ("unable to get local issuer certificate") on both https and http attempts. Unverified; flagging as a lead for manual follow-up, not used as a source. |
| 13 | web.archive.org (Wayback Machine) | — | WebFetch explicitly refuses this host in this environment ("unable to fetch from web.archive.org") — could not check for older cached versions of the product page that might have carried more spec content pre-"sold out." |

---

## 3. The "2.5% shrinkage" and third-party TDS dead ends (documented so they aren't re-chased)

Two numeric leads surfaced during the search that ultimately **do not qualify** as verified data for this exact product, and are excluded from the table in §1 for that reason:

1. **"2.5% Low Shrinkage" (Amazon bullet).** A WebSearch snippet of the target Amazon listing (B0FR4SPGMF, Tough Washable Resin) surfaced the phrase "2.5% Low Shrinkage." However: (a) the underlying page could not be directly fetched to confirm exact wording/context (persistent HTTP 500), and (b) a separate search also attributed a "2.5% low shrinkage" claim specifically to Nova3D's **Mecha Washable Resin** (a different SKU, store.nova3dp.com/products/nova3d-washable-mecha-resin-...), whose own store page — directly fetched and grepped — only says "Low Shrinkage" qualitatively with **no percentage in the page text**. Given the conflicting attribution and inability to verify against a primary source, this number is **not** reported as a confirmed spec. It is also not stated whether this would be linear or volumetric shrinkage.

2. **"Water Washable Resin Technical Data Sheet v1" PDF** (https://cdn.shopify.com/s/files/1/0763/2263/6060/files/Water_Washable_Resin_Technical_Data_Sheet_v1.pdf) — found via search and read in full. This PDF **is not a Nova3D document**. It's branded "3D META" (info@3dmeta.com.au, an Australian 3D-printing retailer that sells its own house-brand resins). Cross-checked against 3D META's own catalog (https://www.3dmeta.com.au/collections/printing-resins): they sell a "Water Washable 3D Printing Resin" and a separate "Tough 3D Printing Resin" as distinct house-brand SKUs, with no Nova3D-branded products listed. So this datasheet is for a different manufacturer's plain water-washable resin, not Nova3D's ABS-like Tough variant — it is **not used** as source data for this report. For reference only (not attributed to Nova3D), that unrelated document states: density 1.1–1.2 g/cm³ (ISO 1183), viscosity 100–200 cP @ 25°C (rotational viscometer), molding shrinkage 0.2–0.7% (ISO 294), tensile strength 30–35 MPa (ISO 527), elongation at break 6–9% (ISO 527), flexural modulus 2100–2200 MPa (ISO 527), flexural strength 55–65 MPa (ISO 178), hardness 84 Shore D (ISO 868, after curing), exposure 1.5–2.5s/layer at 0.05mm, bottom exposure 20–25s ×4–6 layers, post-cure 1–5 min under 405nm. These numbers are plausible as an order-of-magnitude reference for "what a water-washable UV resin in this class typically looks like," but must not be presented as Nova3D Tough Washable Resin's actual specs.

---

## 4. Green-state vs. post-cured

Nova3D publishes **zero** quantitative data of either kind for this product — there is nothing to distinguish. No source (manufacturer or independent) reported green-state (as-printed, pre-post-cure) mechanical properties, dimensions, or hardness. The manufacturer's marketing copy ("exceptional toughness... resistant to breaking... withstand repeated bending and impacts") is presented undifferentiated, with no indication of whether it describes green or post-cured state, and carries no numbers regardless.

## 5. Verdict for the support-generation engine

Do not seed any material-property default for this resin from a "Nova3D-specific" source — none exists publicly. If the engine needs a working number, it will have to come from either (a) direct in-house testing of this exact resin, or (b) an explicitly-labeled generic "tough water-washable resin" class assumption (e.g., the unrelated 3D META datasheet in §3, or general tough-resin ranges: tensile ~30–55 MPa, elongation ~6–25%, Shore ~84D per broader market data) — but any such substitution must be flagged in the engine's data provenance as an estimate, not a Nova3D-verified value.
