# Step 11 design: Round 25 supports built inside the limits, and the independent checks that must accept them

Status: design for dev-loop §10 step 11 (feeds step 12). Merged from two independent proposals (2026-10-30). Where they differed, §1 states the choice and why.

Governing documents: `docs/PRD.md` (§§4.4–4.6, 6, 7, 8.1, 8.3) and `docs/design/dev-loop.md` (§4, §7, §8, §10). Old evidence informs assumptions only. Nothing here changes a hard limit.

**Labels on numbers:**

- **[O]** owner ruling or owner practice (his profile);
- **[S]** orchestrator starting value: a choice, not evidence; tuned from prints;
- **[M]** measured (on this printer, or on the approved STL);
- **[C]** ChituBox convention;
- **[U]** unmeasured estimate; never decides alone.

---

## 0. Measured facts about the approved Round 25

Input: `fixtures/round/approved/Round 25.keepup.stl`, SHA-256 `853709e4063de2e3c9dc7a646f13d5ede5a2f26c4a4372ab220cfe153480750a`. It is the original turned +115° about X, so the disc is 65° from flat **[O]**. Do not turn it again. The underside normal is about (0, −0.906, −0.423).

Two throwaway scratch programs measured these. They are for design sizing only. They are not checker passes or support-count targets. Lane B re-measures them with production code; agreement within ±2% is a test.

| Quantity | Value **[M]** |
|---|---|
| Triangles; signed volume | 2,996; 851.9 mm³ (0.85 mL). No weight is claimed: resin density is not an input. |
| Bounding box after the tilt | 25.00 (x) × 13.31 (y) × 23.23 (z) mm. The lowest point is on the lip edge, at y ≈ +5.3. |
| Height on the plate | 28.23 mm at 5.0 mm lift. |
| Layers at 0.05 mm | 465 model layers; plate layers 100–564 at 5.0 mm lift. |
| Layer area profile (model height) | 0.04 mm² (z 0) → 4.3 (0.5) → 11.6 (1.0) → 20.6 (1.5) → 30.3 (2.0) → 36.3 (2.5) → 33–37 (3–8) → 50 (9.0) → **51.4 max at z 14.5** (plate layer 390, both scratch programs agree) → 36 (16–20) → 47 (21.0) → 0 (23.2) |
| New down-facing area per layer | 0.5–1.8 mm². This is how far a slab-envelope area can differ from a mid-plane slice: 1–4%. |
| Island 1 | Model layer 0, 0.04 mm², x −0.7…0.7 mm: the lowest point of the lip edge. |
| Island 2 | Model layer 152 (z 7.6 mm): the lowest point of the central annular rib. It stays separate for 21 layers and grows to 10.6 mm² before it joins the recess at layer 173. It is far over the P18 size (more than 25 px across, more than 3 layers). |
| Down-facing faces flatter than 45° | Inner lip wall, top band: 26.3 mm² (<30°) + 12.4 (30–45°). Rib hole wall, top band: 3.4 + 3.9. Rib outer wall, bottom band: 5.5 + 6.3. Outer rim wall, bottom band: 21.4 (30–45°, none <30°). Total 79 mm². |
| Faces at 65° | Lip end face, recess face and rib end face. A 65° face advances only 0.05·cot 65° ≈ 0.023 mm per layer. |

The underside features are raised annular ribs, not long straight ribs. All underside faces are hidden when the base stands on a table. The outer rim wall is partly visible. The top face is the decorated top.

---

## 1. Decisions where the proposals differed

| Topic | Decision | Why (one line) |
|---|---|---|
| Load sharing | Owner-approved 2026-10-31: contacts and downstream sections are sized with a rigid-carrier, tension-only spring-sharing hypothesis (stiffness ∝ section area) together with the additive group requirement, using only rooted contacts and certified areas available at each layer. Equilibrium and conditioning are assessed separately. Nearest-contact tributary is kept as a labelled sensitivity diagnostic. All results are classified under §4.8, and its unprintable conditions take precedence: a confirmed absent rooted material route, or an additive area ratio > 25. Any other additive, equal-share or valid rigid-lever result above 12, and any load response the model cannot balance or assess, is a located intervention (§4.3). | Tributary sharing gave a high ratio on the earlier plan. The new hypothesis is a change of sharing model, not proof that the old high ratio meant no risk. The spring rule is an unmeasured approximation. The first print tests it. |
| Distance for the tributary diagnostic | Euclidean 3D distance, only to contacts on the same printed component. A segment that leaves printed material triggers a route assessment (§4.3); it does not prove the route absent. | On Round 25 the components are simple strips; a path-through-material metric costs a 3D search that changes no decision here. Revisit on the first model where the flag fires. |
| Peel area convention | Slab envelope: the XY projection of all material in the layer slab, as vector polygons. The mid-plane slice is reported beside it. | Union of the top and bottom slices misses a bump inside the slab; the envelope bounds any slicing plane ChituBox picks. |
| H1 admission bound | Generator admits a patch only if its upper bound G ≥ max(T_mesh, T_analytic) is ≤ 0.48 mm² **[S]**. | The checker's per-representation excess (0.010) gives U ≤ 0.49; the second 0.010 is a construction reserve. It does not bound an unknown ChituBox substitute shape: the post-output sample settles that, and until then a substitute is unverified. Contact-count estimates must use the certified capacity of the selected native tip family (§5.4); an analytic-patch capacity such as 5.76 mm² per capped contact is not a mesh-capacity bound. |
| H6 build gap | Build ≥ 0.32 mm **[S]** outside contact regions; the joint ball is always outside, checked by `probe` at contact admission. | 0.02 margin over the checker's 0.005 error lets the checker use a simple, proven method; it costs nothing measurable on Round 25. |
| H4 lift | Lowest emitted model point ≥ 5.0 mm, proved by exact arithmetic on the written f32 numbers. No 0.002 reserve. | A vertex minimum has no measurement error; both sides compute it exactly. |
| H2 separation | Patch envelopes ≥ 0.012 mm apart **[S]**, plus no overlap of reserved head regions (dev-loop §7.1 item 6). | 0.010 resolution + 0.002 reserve; the owner declined any larger gap, and the region rule already spaces heads further. |
| 45° overhangs on the hidden underside | Covered by contacts within 3.0 mm **[S]** (P11 **[C]**, owner baseline). | FR-30: stay at the owner baseline where the evidence is thin. |
| 45° band on the partly visible rim | Automatic contacts **[O]** (owner ruling 2026-10-30). They obey every construction rule: the H1 area bound, H2, H6, H9 and load sizing. Use the smallest tips the load allows, and only as many as 3.0 mm coverage needs. | The owner chose support over a warning. The rim is partly visible, so marks stay small and few. |
| P18 "either direction" | Both directions: X ≤ 25 px **and** Y ≤ 25 px **[O]** (owner ruling 2026-10-30). Joined within 3 layers **[S]**. | The owner's reading. Changes nothing on Round 25. |
| Admission flow | `probe` (pure, parallel, per-candidate quotes and rejects, intra-batch conflicts included), then `commit` of a whole batch: all or nothing. | Per-candidate results let the planner re-plan; atomic commit keeps load assignments consistent with what was admitted. |
| Shape kernel location | Move the pure f32 native-shape mesh kernel from `emit/src/shapes` into `geom`. `emit` keeps record building. | `support` must build and check the exact written vertices, and `support → emit` would be a dependency cycle (dev-loop §4.3). |
| Bracing | Owner's Light profile rule (§3.6). | FR-30 and P12: his view is that long thin unbraced posts fail; there is no measured slenderness rule to use instead. |
| H3 sampling in the checker | Slice at the bottom, middle and top of each layer **and** at every vertex height inside the layer. | Three planes alone can miss a piece that starts and ends between them. |

---

## 2. Prerequisites that must not be called settled

1. **Which solid ChituBox slices.** `docs/format/chitubox-v1.4.0.md` §19 shows we reproduce native fields and meshes within 2 ULP. It does not show whether ChituBox slices the mesh, rebuilds from fields, or substitutes a shape. Step 11 uses only whole, unmodified native balls with matching fields and meshes. No clipped balls (they return `Unverified`). The native mesh is inscribed in the analytic solid from the fields, so analytic upper patch bounds and analytic clearance lower bounds (H1 upper, H2, H6) hold for both readings. An analytic lower patch bound does **not** hold for the mesh: a 16-gon section has at most 0.9745 of the analytic area, so at 0.48 mm² the difference is ≥ 0.0122 mm². The checker keeps separate intervals per representation (§6.3). A substitute shape is not bounded. PRD §8.1 requires the post-output sample (PRD §12) to settle this before the file is offered for printing. Do not drive ChituBox without asking.
2. **Solid status.** Prove a closed, consistently oriented manifold with no self-intersection and no enclosed void (§5.6). Signed volume alone proves none of these.

---

## 3. What the evidence supports and what it does not

| Claim used | Evidence | Supports | Does not support |
|---|---|---|---|
| Supports snap near area ratio 25 | P4 **[M]**: cones [25.00, 25.34], cylinders (19.9, 30.5), H2 plate (25, 36). Single specimens at range ends; 3 cap areas. | Flat horizontal caps 2–12.5 mm across, vertical arms 4–6 mm, one resin, failure at the narrowest section above the contact (P7). | Thin tilted strips (Round 25 layers are 1–4 mm × up to 25 mm). Angled, bent or merged members. Off-axis loads. |
| Design ratio 12 | P5 **[O]** "start with 12 and see". | The policy for this milestone. | Any claim that 12 is safe or needed. Heads were lost at 9 on one plate; another way of measuring gives 11–14. |
| Sharing is at least additive | P6 **[M]**: 2–6 arms under one 30 mm² cap, pitch 2.0–2.8 mm (group span ≤ 14 mm). | Sharing among ≤ 6 arms within about 14 mm under a stiff part. | More than 6 arms; wider spans; off-centre loads; any bonus above simple addition (that plate had a fin-root bonus of about 20%). |
| 45° faces need support | P11 **[C]**, low confidence. | The owner baseline (FR-30); what ChituBox's overhang check flags. | Physics. |
| Flat-shelf reach | P8 **[M]**: ≤ 2.13 mm no loss; 2.59 first loss; reading spread ±0.8 mm. | Single flat layers. | Thick sloped wedges. |
| Owner's spacing | P15 **[O]** settings: tip distance 4 / 6 / 8 / 8 mm. Light tip 0.3 mm. | The baseline style. | That such layouts print reliably here. |

**Consequence.** At full layer area, Round 25 needs at least 51.4 / 12 = 4.3 mm² of total support section. A Light-sized tip (neck radius 0.10, section 0.031 mm²) holds 0.37 mm² of layer at ratio 12. A Light-like layout at 6 mm spacing scores a ratio of about 100–300 **[U]** at the lowest layers. If such layouts print reliably on tilted bases (his experience), the full-area convention overstates strip load by up to ten times. That is a hypothesis, not evidence. The design applies P5 = 12 as ruled and reports the gap. The PRD §8.4 side-by-side print is the print that settles P5 versus 18 and whether the cap law carries over to strips.

---

## 4. The FR-20a load model (milestone version)

### 4.1 Index and peel area

- **Index:** X(s, k) = Q(s, k) / B(s, k).
  - k is a layer (one exposure).
  - Q(s, k) is the peeling area, mm², assigned to support section s in that layer.
  - B(s, k) is a lower bound on the load-bearing section area of s, in geometry printed by layer k.
  - Requirement: max over s and k of X ≤ 12 (P5 **[O]**).
- Never divide by contact patch area. Patch area and section area are different numbers.
- **Layer grid.** Plate z = 0. Layer k covers [k·h, (k+1)·h), h = 0.05 mm **[O]** (owner ruling 2026-10-30: his default). ChituBox applies the layer height only when it slices, so the native files do not store it. Record the profile and layer origin with the candidate's hash. The model's lowest point sits at 5.0 mm, a layer boundary.
- **Peel area of layer k.** The area of the slab envelope: bottom slice ∪ top slice ∪ the XY projection of every surface triangle clipped to the slab. Exact vector polygons, f64. Material in overlapping solids counts once. Model, support and raft area are labelled separately, with an overlap term so the parts add up to the union. The mid-plane slice area is reported beside it.
- **What peels.** All material cured in layer k: model and supports. One layer peels at a time. The load on a section is its largest single-layer load. Layer loads are never summed over the print.
- **Raft layers** are a separate, earlier load case. They are not added to model layers.
- **Not modelled.**
  - Weight (P3 **[O]**).
  - Plate position: multiplier 1 (P21 **[M]**, a bound, not proof of no effect).
  - Suction, except a print-order cup (FR-17a, §4.6).

### 4.2 Print-order structure

- **Raster for structure only.** Each slab is also rasterised on the printer pixel grid (0.014 × 0.019 mm, P1 **[C]**). The raster gives connectivity, island sizes and cups. Areas come from the vector polygons.
- **Components.** Union-find over layers: pixels linked by 4-connectivity in a layer, and to overlapping pixels in layer k−1. C(k) is the set of model components that exist after layer k. Components merge when they first touch (island 2 merges at model layer 173).
- **A contact becomes active** on component C at the first layer where its ball and C overlap in pixels. Its capacity at layer k uses only the patch printed by then: A_p(≤k), a lower bound from the patch geometry.
- **Islands.** A model component that first appears in layer k must overlap a contact ball in layer k or k−1. Otherwise it is an unsupported island.
  - P18 **[O]** exempts an island that is ≤ 25 px in X **and** ≤ 25 px in Y (0.350 × 0.475 mm), stays inside that size, and joins rooted material within 3 layers **[S]** (0.15 mm at 0.05 mm layers).
  - Such an island is reported (FR-29) with its sizes and growth, never only a yes or no.
  - An omitted island never carries load, and cannot carry earlier loads after it joins.
- **Nothing later carries anything earlier.** A support or model part that exists only above layer k is unavailable at layer k.

### 4.3 How load reaches the contacts

**Sizing hypothesis [U].** For each layer k and each printed component C, the contacts and every downstream support section are sized with two rules together. Both must hold.

1. **Rigid-carrier, tension-only spring sharing.** The printed part of C at layer k is a rigid body. Each active rooted contact path is an axial spring. Its stiffness is proportional to its limiting load-bearing section area B_i(k). The layer's peel area A_k(C) acts as one resultant at the centroid of the layer's source elements. Springs can pull but cannot push. Solve for the rigid-body motion (one translation along the peel direction and two tilts) so that the spring forces balance the resultant in force and in moment. A spring that would push is removed and the solve is repeated. The reaction share R_i(k) is the area-equivalent force in spring i. The sharing ratio of contact path i is R_i(k) / B_i(k). Downstream sections use these shares (§4.4).
2. **Additive group requirement.** X_group(k,C) = A_k(C) / Σ_i B_i(k) ≤ 12 (definition below).

Conditions on both rules:

- Only rooted contacts, and only certified section areas available at layer k, count. A contact or section that exists only above layer k does not count.
- The stiffness rule (stiffness ∝ section area) is an unmeasured approximation. It is not a physical upper bound on any reaction.
- Sharing depends on the sizes. After any change of size or placement, recompute the sharing, then replay the complete emitted geometry (all layers, all components).
- A source has a positive-width material route to a rooted contact, or it does not. A straight segment from a source to a contact is not evidence of a route. A straight segment leaving printed material triggers a route assessment; it does not by itself prove that no route exists. If a source has no positive-width material route, available by that layer, to an active rooted contact, classify it as **unprintable as is**. If a route exists but its bending, stiffness or load sharing cannot be assessed, classify it as **intervention: load path not assessed**. If route existence itself remains unresolved, report that uncertainty explicitly rather than claiming a supported path. None of these sources is assigned zero load or silently omitted.
- A component with peel area and no active contact has no sharing result. That is an unsatisfied placement constraint (§5.6), never a zero load.
- **Islands keep their own load groups until merger.** A separate island (for example the rib before it joins the disc) is sized only against its own rooted contacts. Pooling the load of the whole part is never used to claim that newly printed local material is held. Reach, first-layer anchoring and print-order coverage stay mandatory (§5.3).
- **Partial patches are layer-specific.** A contact's capacity at layer k uses only the patch printed by k (§4.2). Future patch area never holds an earlier layer.

**Equilibrium and conditioning are assessed separately, for every layer and component.**

- Equilibrium: the residual force and the residual moment of the spring solution, compared with the layer's resultant; whether the load centroid lies inside the spread of the tension springs; how far outside if not.
- Conditioning: whether the stiffness system is singular, near-singular or ill-conditioned (for example a near-straight row of contacts), and how sensitive the shares are to a small change of position or stiffness. The thresholds are set before the run and stated in the report **[S]**.
- A singular, ill-conditioned, unsupported or materially unbalanced response is **unresolved**. It is never a low-ratio pass.
- A negative reaction is not clipped and the shares renormalised, unless the moment balance is re-established and checked. Removing a pushing spring and solving again is allowed because it re-solves force and moment. There is no silent fallback to the additive result that clears the condition.
- For an unresolved case, the planner first tries legal contacts that improve the reaction spread (§5.6). If none does, the report gives a located load-path intervention (§4.8).

The route, bending and stability checks stay in force: narrow material routes, angled heads (§4.5 A9), long columns, and brace growth and redistribution. A load-carrying neck whose bending is not assessed produces a located **intervention: load path not assessed**. Ignoring brace assistance does not prove an upper bound for individual column reactions. Every growing brace's own peel is included, and its unassessed bending or redistribution is reported as an intervention. The report names the affected contacts and layers and states what must be changed or what physical uncertainty the owner would be accepting. No result is called a per-section load bound without stating and establishing the conditions under which it is a bound.

**Thin growing edges and narrow connections.** Where new material grows as a thin edge, or joins existing material through a narrow connection, and transfer of its load through the existing material is not assessed, report a located **local-load-path intervention**. This applies even when every global ratio is below 12.

Other sharing results, computed beside the sizing result. Each of the first three can raise an intervention as in §4.8:

- **Additive group ratio** (also a sizing rule, above). For component C at layer k, let B_i(k) be each active contact path's limiting load-bearing section lower bound in mm². The additive **area ratio** is X_group(k,C) = A_k(C) / Σ_i B_i(k). The additive **design-capacity utilisation** is u_group(k,C) = A_k(C) / Σ_i [12 B_i(k)] = X_group(k,C) / 12. Report these under distinct names. Apply the P4 breaking comparison to X_group, not to utilisation: X_group > 25 is unprintable as is (§4.8). Generator and checker use the same definitions, implemented independently. A positive area with zero active section is unprintable, never zero or an omitted result. This is the P6 form; evidence covers it only up to 6 contacts within 14 mm.
- **Equal-share result** on the same contacts.
- **Rigid-lever result:** the spring-sharing solution above, reported by its own name. It is valid only when equilibrium and conditioning are assessed and pass. It is very sensitive to a near-straight row of contacts. The bottom cluster on Round 25 is such a row, so the result there may be large or unresolved, and that is reported as an intervention.
- **Tributary result (sensitivity diagnostic).** Every source area element of C's peel area goes to the nearest active contact on C (Euclidean 3D, from the element at the layer height to the contact centre; exact ties split equally, in stable contact order), over the whole component. It is labelled a sensitivity diagnostic. It does not size anything. A tributary value above 12 is not, by its value alone, an intervention. A tributary result never downgrades a local response that is unassessed. The change from tributary sizing to the sharing hypothesis above is a change of hypothesis. It is not proof that the old high ratio meant no risk.
- Per load group: number of contacts, their height range and path-length range; whether the layer's centroid lies inside the spread of its active contacts, and how far outside if not; the narrowest model connection the load passes through.

Do not switch to a smaller result silently. More than 6 contacts in one group is reported as beyond the tested group size. It is not forbidden.

**The part as a carrier.** Load passes through the printed disc as a stiff body. That is an approximation, and it is the rigid-carrier hypothesis above. Every route must pass through printed material of positive width. Narrow rib or rim connections are named in the report.

### 4.4 Merged loads and sections

- **Merged loads.** For section s and layer k: load = Σ shares of the contacts upstream of s + the support's own material in layer k upstream of s. Loads add at joints (statics **[U]**). A source ledger stops one area counting twice.
- **Milestone shape.** One column per contact above the raft. No merges above the raft. Braces are given no share of model load; this is an assumption, not a bound for individual columns in a coupled frame. Each brace's own peel, including while it grows before it joins its partner, is loaded onto its carrier column; its bending and any redistribution it causes are reported as an intervention (§4.3). Merging of model load happens only in the raft, which is fixed to the plate and is not a load section.
- **Section areas.** The smallest perpendicular cross-section of each native 16-gon member: B16 = 8r² sin(π/8) = 3.0615 r² (2.55% below πr²). Its smallest width across is 2r·cos(π/16), not 2r.
  - The tip cone's section is its **neck**: the cone radius where it leaves the ball.
  - The **contact patch** is also a load section.
  - So contact capacity is cap(k) = 12 · min(A_p(≤k), A_neck).
- **Every section on the path is checked:** patch, neck, tip cone, joint throat, column, flare.
- **Worst layer per section** = the layer that maximises its ratio. It may differ from the largest peeling layer. Both are reported, with section id, native record index, layer number, height and picture marker.
- **Prediction to verify [U]:** the model's worst layer is one of model layers 40–60 (z 2.0–3.0), before contacts on the recess face and rib become active.

### 4.5 Assumptions and their checks

| # | Assumption | Check |
|---|---|---|
| A1 | Peel is the only load (P3) | Owner experience; the first print, per region. |
| A2 | Load ∝ full cured layer area, carried over from caps to strips | Not testable from the file. The §8.4 side-by-side print. The report shows the ratio a Light-sized tip would have at each site. |
| A3 | One layer peels at a time | Matches how the plates were designed. |
| A4 | Area = slab envelope | Mid-plane result reported beside it. Native slicing (H8 record) can test it. |
| A5 | Failure at the narrowest perpendicular section; the patch is a section (P7) | Owner reads the breaks on the first print. |
| A6 | Rigid-carrier, tension-only spring sharing (stiffness ∝ section area) plus the additive group requirement size contacts and sections; the spring rule is an unmeasured approximation, not a reaction bound | All four sharing results (rigid-lever, additive, equal-share, tributary), with separate equilibrium and conditioning assessments, per layer and component, classified under §4.8. Tributary is a sensitivity diagnostic only. Unprintable conditions take precedence: a confirmed absent rooted material route, or an additive area ratio > 25. Any other additive, equal-share or valid rigid-lever result above 12, and any singular, ill-conditioned, unsupported, unbalanced or unassessed response, is a located intervention. The owner reports which contacts failed, if any. |
| A7 | A contact carries only its patch printed so far | Geometric; tested on synthetic shapes. |
| A8 | Loads add at joints; braces take no model load | Not a bound for individual columns. Brace self-peel is counted; brace bending and redistribution are an intervention. |
| A9 | Angled heads (25°–60° below horizontal) behave like vertical arms | Unmeasured. Each head's angle, length and offset is reported. |
| A10 | 65° faces hold themselves | ChituBox's overhang check on the output; the print. |
| A11 | No print-order cups on Round 25 | Computed (§4.6). |
| A12 | Contacts ≤ 3.0 mm **[S]** apart cover a sub-45° region, hidden or on the rim band (half the Light tip distance of 6 mm **[O]**) | The print; ChituBox's overhang check. |

### 4.6 Print-order cups (FR-17a)

A cup is an uncured region enclosed in layer k whose resin, over layers ≤ k, has no path to the outside. It is computed on the same raster, in print order, separately from the finished-cavity test (H5). None is expected on Round 25 (the recess opens toward −y and down), but it must be computed. A cup found gets no invented suction factor and no undeclared hole: it is a located intervention or unprintable warning.

### 4.7 What the report states (FR-21)

- Candidate identity: file hash, layer origin and layer height, pose, slicing profile.
- Parameter values: P5 = 12, the H1 admission bound, the 3.0 mm coverage reach, the 45° rule, layer height, pixel pitch.
- The sharing model, with the change stated: from nearest-contact tributary sizing (old) to rigid-carrier tension-only spring sharing plus the additive group requirement (new). The stiffness rule is stated as an unmeasured approximation. The tributary result is labelled a sensitivity diagnostic.
- The largest ratio over all layers for **each** model separately: rigid-lever, additive, equal-share, tributary. No model's maximum stands in for another's. Beside each: section id, layer and picture marker, and whether equilibrium and conditioning were assessed there.
- The 10 largest area ratios in the emitted file, as a comparison table. For each row: section id, part type, worst layer and height, all four model values side by side, and a picture marker. Never coordinates alone. Rows are ordered by the larger of the rigid-lever and additive values.
- At each of three places (the bottom lip, the rib, and the worst newly active patch): picture marker; layer and height; the printed patch and the limiting section; the number of available contacts; the assigned load; all four model values; whether load transfer and equilibrium were assessed.
- The history from contact activation to patch growth at each critical location, layer by layer.
- Limitations, stated plainly: the stiffness rule, the moving peel front, angled heads, and brace growth and redistribution.
- The largest peeling layer, with its area and the total capacity active then.
- Areas treated as needing support (islands; sub-45° hidden faces, with mm²). Areas judged self-supporting, with the reason (≥ 45°, P8 reach).
- The uncertainty statement (§3).
- Comparison with the owner baseline: the ratio a Light tip would have at each site. Our tips depart from Light; FR-30 requires citing the reason (P4/P5).
- Sensitivity: the largest ratio with the mid-plane convention; the contact count the same planner would need at P5 = 18 (not emitted).
- Contact count, size distribution, spacing, coverage, and bracing (actual braces and remaining unbraced length), before and after the sharing-model change. These are measured on the new emitted file. No count is a target.
- Print observation plan, per region: lost lower layers, torn edges, rib loss, neck or patch breaks, column movement. Damage from removing the supports is recorded separately.

### 4.8 Warnings

| Trigger | Class |
|---|---|
| A rigid-lever or additive sizing ratio > 12 remains after the planner has tried every legal non-touching site in the region and a full replay shows no improvement | **Intervention**: "load not held at design ratio", with ratio, region, the search done (exhaustion, not infeasibility) and the easy fix (a ChituBox support at the marked place). |
| A tributary ratio > 12, with no other trigger in this table | **Not an intervention.** Shown as a labelled sensitivity diagnostic (§4.3). It never downgrades an unassessed local response. |
| A thin growing edge or narrow connection whose load transfer through existing material is not assessed, even when every global ratio is below 12 | **Intervention: local load path not assessed**, located at the edge or connection |
| A singular, ill-conditioned, unsupported or materially unbalanced reaction response (equilibrium or conditioning), left after legal contacts that improve the reaction spread were tried | **Intervention: load path not assessed**, located, naming the condition. Never counted as a low-ratio pass. |
| Additive area ratio X_group > 25 (beyond P4 breaking) on a component, or positive area with zero active section | **Unprintable as is** |
| A non-exempt island has no rooted first-layer anchor (search budget exhausted, or proved no legal anchor in the stated domain; the report says which), or a source has no material route to an active rooted contact by its exposure layer | **Unprintable as is**, with a hint where splitting would help, if any. Risk acceptance alone cannot clear this status; changed geometry must be checked again. |
| A sub-45° point, hidden or on the rim band, with no contact within 3.0 mm printed no later than that point | **Intervention**, with the easy fix |
| Additive, equal-share or valid rigid-lever result moves a section past 12 (tributary is excluded); the stiff-part route passes a narrow connection; an angled head or long column whose stability cannot be judged | **Intervention** |
| A material route exists but its mechanical response is unassessed, or route existence remains unresolved: load centroid outside the active-contact spread, uncertain bending (neck or brace), sharing, or brace growth or redistribution | **Intervention: load path not assessed**, naming exactly what is unknown. A proven absent route belongs to the unprintable row above. |
| A load path of a shape the model cannot assess | **Intervention** |
| A print-order cup | **Intervention** or **unprintable**, by size |
| A P18 island left unsupported | **Information** (FR-29) |

Separate islands keep their own load groups until they merge. A warning about an island is judged against that island's own contacts, never against the pooled load group of a part it has not yet joined.

When a case matches both an unprintable row and an intervention row, it is unprintable as is.

Interventions name where to add a support. They never suggest moving one (FR-34).

### 4.9 Prediction for Round 25 [U] (step 12 measures it; not a target)

There is **no contact-count target**. The statements below are predictions **[U]**. Step 12 measures count, size distribution, spacing, braces, remaining unbraced length, coverage and every model's maximum on the new emitted file. A prediction that the new file has fewer contacts, with more room for braces, than a tributary-sized plan is also **[U]** until measured.

Estimates use the certified capacity of the selected native tip family (§5.4): the in-sphere-certified cap capacity of about 5.45 mm² **[U]**. An analytic-patch capacity (5.76 mm²) is not a mesh-capacity bound. Recompute after the lower-bound-aware sizing test (§8.2 item 10) passes.

- **Lowest 2.5 mm:** the additive requirement alone needs active capacity of A(z)/5.45 contacts at the cap: at least 3 by z 1.0, 4 by 1.5, 6 by 2.0, 7 by 2.5. These are minimum counts, on the lip end face along the bottom arc (|x| ≤ 7.7 mm). The spring-sharing result may need more, or may stay unresolved there. No upper count is predicted.
- **Rib island:** 10.6 / 5.45 means at least 2 at the cap by the additive requirement, on the rib end face near its lowest point. The rib is sized against its own contacts until it joins the disc.
- **Coverage:** the inner lip wall top band from contacts on the inner edge of the lip end face (≤ 2.5 mm away); the rib bands from the rib contacts.
- **Rim band:** the band (21.4 mm²) lies along the bottom arc, beside the bottom cluster. Bottom-cluster contacts within 3.0 mm, printed no later, may cover part of it; rim contacts fill only the rest, at the smallest tips the load allows.
- **Bottom cluster:** a near-straight row, so the rigid-lever solution is expected to be ill-conditioned or unbalanced at the lowest layers. Expect "load path not assessed" interventions there unless legal contacts improve the spread. They are shown, not dismissed.
- **Top layers** (z 20–22, 37–47 mm²): more contacts on the top arc of the lip end face. The number is not predicted.

This is the "12 may over-support" case the owner foresaw (P5). It is shown openly, not tuned away.

---

## 5. Placement and sizing

### 5.1 Freeze the model first

Centre the approved STL, convert to the f32 form that will be written, recompute bounds, and set the translation so the lowest written vertex is ≥ 5.0 mm (§1). Freeze these bytes with the input hash, pose, limits and process settings before any contact is placed. No decimation is needed for this fixture.

### 5.2 Site classes (FR-11a)

Site class comes from **batched ray casts in the resting pose**: the base standing underside-down on a table, from the recorded keep-up rotation.

- A sample point is **hidden** if every ray in the upper hemisphere hits the model or the table: 64 directions **[S]**, sample spacing 0.25 mm **[S]**.
- **Class 1, hidden underside:** preferred. The lip end face and the rib end face come first. Ribs are support sites, not detail (FR-11a).
- **Class 2, partly visible rim:** used for two jobs. It covers the rim band below 45° where no class 1 contact does (owner ruling 2026-10-30) **[O]**. It also anchors an island that no class 1 site can hold. Class 2 contacts use the smallest tip whose certified capacity carries the load. If a rim contact's load needs a larger tip, first try added class 1 contacts nearby to take load from it. Each class 2 contact is reported with a picture.
- **Class 3, decorated top:** never.

### 5.3 What needs support

1. **Islands** (§4.2): an anchor present by the island's first layer.
2. **Reach:** new unsupported material whose horizontal reach from rooted material exceeds 2.13 mm (P8 **[M]**, no-loss value).
3. **Overhangs:** points flatter than 45° (P11 **[C]**) on hidden faces and on the rim band **[O]**, on a 0.25 mm grid **[S]**. Each needs a contact within 3.0 mm **[S]** whose patch prints no later than that point.
4. **Load:** at every section and layer, the rigid-carrier spring-sharing ratio and the additive group ratio are both ≤ 12, with equilibrium and conditioning assessed and passing (§4.3). The tributary result is a labelled diagnostic, not a constraint. Islands are held by their own contacts until merger. Items 1 to 3 stay mandatory whatever the load result is.

### 5.4 Tip family and sizing (H1 and H9 built in)

On a flat face: ball radius R, standoff s ≥ 0 (centre outside the surface), patch A_p = π(R² − s²).

- **Neck:** radius r_e where the tip cone's boundary leaves the ball (t² + r(t)² = R² for a coaxial cone, t measured from the ball centre), not where its centreline does. A_n = 3.0615 r_e². Admission also requires the minimum load-bearing width of the ball/cone union through the whole transition ≥ 0.102 mm, measured as in §7.2 H9.
- **Required section:** for every layer k assigned to this contact, require `B_lower(k) ≥ Q(k) / 12`, where `B_lower(k)` is the minimum certified area of the patch printed by k, the exposed neck and every other section on the path available by k. Q(k) is the contact's reaction share under the §4.3 sharing hypothesis, counting only rooted contacts and certified areas available at layer k. The additive group requirement (§4.3) must also hold for the contact's group. A share depends on every stiffness in the group, so it is recomputed after any size or placement change. Patch bounds must be valid for every still-possible native representation (§6.3: the face in-sphere bound, not the analytic patch).
- **Initial proposal:** the analytic planar formulas may propose dimensions, but do not establish capacity. They are: A_req = L / 12 (L the contact's current load: its equal share of the component's layer area in the first round, then its §4.3 reaction share); r_e = max(0.052, √(A_req / 3.0615)); R = 1.06 r_e **[S]**; s = √(R² − max(A_req, A_n)/π). 0.052 mm is the H9 build minimum: width across flats 2r·cos 11.25° = 0.102 mm ≥ 0.1 + 0.002 reserve; a native r = 0.05 is only 0.098 mm across flats. Round the proposed fields, construct the actual native mesh, and calculate its certified capacity before selecting the dimensions. Example: r_e 0.2, R 0.212, s 0.0772 gives an analytic patch of 0.1225 mm², a native-mesh patch of about 0.1189 and an in-sphere certified bound of about 0.1158 mm²; at a load sized for ratio 12 by the analytic area, the certified ratio is 12.69.
- **Sizing:** within each private planning batch, search radius, standoff and permitted head dimensions for the smallest contact satisfying the layer-specific capacity constraints, H9 and every clearance constraint, while keeping the H1 upper bound G ≤ 0.48 mm². Use the certified lower bounds in this search, not the analytic patch area. Recompute the sharing after changes to the private geometry.
- **Cap:** a contact is capped when no larger certified capacity can be obtained within its admissible geometry and H1 bound. Report that capacity from the rounded native geometry; there is no universal per-contact capacity. At the largest-tip scale (analytic patch 0.48 mm²) the in-sphere-certified capacity is about 5.45 mm² **[U]**, not the analytic 5.76. When it is insufficient, try legal non-touching contacts (FR-22) under the keep rule of §5.6, or issue the located unresolved-load warning. The planner never makes an admitted contact bigger.
- **Smallest tip:** the across-flats construction minimum remains 0.102 mm (r_e 0.052). Its usable capacity is measured from the rounded geometry and the patch printed at each layer, not inferred from the radius alone.
- **Head:**
  - Tip cone from the ball centre, flaring fast (P7) to the joint radius r_j = max(1.5 r_e, r_col) over 1.5 mm **[S]** (native Ultralight arm 1.5, Light 2.0 **[O]**).
  - Joint ball r = 1.04 · max(r_j, r_col) (native ratio).
  - The complete head, including the joint ball, has a target reach of 2.6 mm from the contact centre and a construction maximum of 2.95 mm. This head envelope is not the H6 contact-surrounding region. That region contains only the contact ball, neck and tip cone, ends before the joint, and extends no farther than 3.0 mm from the contact centre **[O]**. The complete joint ball remains outside the region and must satisfy the 0.32 mm admission gap.
  - Whole balls only.
- **Curved faces:** the formula only proposes R and s. Admission computes the true patch bound on the frozen mesh (§6.3). Minimisation tolerance target: ≤ 0.001 mm in member sizes **[S]**. The report says when geometric uncertainty, not load, sets the smallest certifiable contact.

### 5.5 Approach direction (FR-26)

The head axis is the **most vertical** direction in a small fixed fan between the outward normal and vertical that meets all of these, proved at admission:

- (a) Only the contact ball touches the model. Inside the contact-surrounding region, the neck and tip cone keep a gap greater than 0.02 mm **[S]**. The contact-surrounding region contains only the contact ball, neck and tip cone, ends before the joint, and extends no farther than 3.0 mm from the contact centre. The complete joint ball, column and every other part outside that region must have a certified model-clearance lower bound of at least 0.32 mm at admission, including clearance from other model sheets. Being inside the head envelope does not grant a clearance exemption. For a fat neck on a 65° face this gives at most about 35° from the normal, so the head axis is 25°–60° below horizontal.
- (b) **Departure (capped):** along the head beyond the ball, the gap to the model never decreases while it is below 0.35 mm **[S]**. After the gap first reaches 0.35 mm it may vary, but it stays ≥ 0.32 mm for the rest of the region. At the region end it is ≥ 0.32 mm, and nothing after it comes closer. Reports call this "capped departure (cap 0.35 mm)", never "never decreases".
- (c) **H3 at the head:** the ball's lowest point lies inside the cone or joint: R·cos α ≤ r(R·sin α), α = axis angle below horizontal. Otherwise the bottom cap of the ball prints as a floating piece. For r_e ≈ 0.94 R this needs α ≥ about 22°.
- (d) All other model sheets are tested. A large region is not permission to touch the far side of the recess.

### 5.6 Planner: solve before commit

All sizing happens in a private proposal. Admitted geometry is never moved or grown.

1. **Seed islands before assigning their load.** For every non-exempt island, batch-search contact proposals whose actual supported native geometry overlaps the island on its first printed layer and whose support path is rooted by that layer. Search feasible class-1 sites first; if none can be certified within the search budget, search class-2 sites under the island exception and report each selected rim contact. "Nearest site" alone is not an anchor certificate. An island with no active contact is an explicit unsatisfied placement constraint, never a zero load or a nearest-contact query on an empty set. Rejections trigger alternative position, size and approach proposals within the private plan before admission. Then add a hexagonal sample of class 1 sites over the needs regions at spacing 3.0·√3 mm. Then add class 2 sites, at the same spacing, only on rim-band parts that no class 1 site covers. All start at the smallest tip.
2. **Iterate** (at most 8 rounds **[S]**, each one batch):
   - run the load model (§4.3) on the proposal, including the supports' own layer areas. Islands keep their own load groups until merger;
   - size each tip and column to its load;
   - for each unresolved load or equilibrium condition (a capped tip still over 12, or a singular, ill-conditioned, unsupported or materially unbalanced response), propose **load-sharing candidates**: legal sites (region and separation rules kept) chosen for their expected effect on the reaction spread. There is no mandatory neighbour ring and no required distance from the overloaded tip. Candidate generation is a search setting, not a spacing law;
   - **keep rule.** Replay the complete geometry with the candidate. Keep a candidate added solely for load sharing only when the replay shows improvement in the relevant unresolved load or equilibrium condition, and introduces no worse violation elsewhere. Otherwise drop it. A contact needed independently for island anchoring or coverage is kept even when it does not lower a parent's overload;
   - for each component with an unsatisfied island constraint, or a load-path intervention a further contact could remove, propose further anchors as in step 1;
   - shrink tips with spare certified capacity to the smallest whose certified lower bounds (§5.4) carry their load, recomputing the sharing after each size change;
   - stop when every sizing ratio ≤ 12, every equilibrium and conditioning assessment passes, and every island is anchored, or the search ends (then warn, §4.8, naming the search done).
   - Column self-peel makes load depend on size. Size columns from a conservative envelope, then replay the final geometry once before admission. A fixed point that does not settle in 8 rounds is reported, not hidden.
3. **Admit:** `probe` the whole layout. A rejected candidate blocks its site; re-plan the rest as a proposal, including alternative anchors for any island that lost its anchor. Then `commit` the whole batch at once.
4. **Termination.** Distinguish **satisfied**, **search budget exhausted**, and **proved no legal anchor within the stated search domain**. Eight rounds or exhausted samples establish only search exhaustion. It is reported as search exhaustion, never called infeasibility. Recompute first-layer anchoring, P18 exemptions, print-order coverage, sharing and load on the complete final proposal, then replay them on the emitted file. An unresolved non-exempt island remains unprintable as is. No downstream step moves or grows an admitted contact.

Search resolution is a search setting, not a spacing law. The owner's 4 / 6 / 8 / 8 mm distances stay comparison values. If emitted spacing is much denser, the report names the region and the capacity or island need that caused it.

### 5.7 Columns, bracing, bases, raft

- **Columns:** vertical rods from the joint to the base. A vertical drop from a 65° underside moves away from it (the gap grows 0.423 mm per mm of drop). r_col = max(r_j, √(load / 12 / 3.0615), 0.25). 0.25 mm is the pillar radius of his Light and Ultralight profiles **[O]**.
- **Bracing** (his Light profile **[O]**; P12 unmeasured): a column with more than 4.0 mm free length gets 45° X-pairs to the nearest column that is at least 4.0 mm away (plan distance) and within 15 mm. Columns closer than 4.0 mm to each other are allowed (they keep the 0.32 mm member gap) and are not braced to each other; each still needs its own brace partner, or the planner reports the column as unbraced (located warning). Brace radius 0.25. Nodes at least 4.0 mm apart, the first at least 4.0 mm above the raft. Each brace starts on a column already printed (H3).
- **Joints:** rod ends reach the joint ball centre; the joint ball radius ≥ the rod end radius. No tangent or thin-lens joins.
- **Bases:** flare from r_col to 1.1 mm over 1.0 mm (P17 **[O]**, native). Column end 0.002 mm below the flare top; flare bottom 0.002 mm below the raft top (native overlap **[M]**).
- **Raft:** connected style only **[O]**. A pad 2.2 mm across × 0.2 mm thick at each base; bridges 2.2 mm wide × 0.2 mm thick along the minimum spanning tree of pad centres (native rule). Native records with attr `0x1e00` and the real raft Z. Check that the solid union is one piece; a bridge list is not proof. Pads may overlap; supports may merge near the plate (FR-27). Never the default mat or hex mat.
- **Feet on the model:** none on Round 25 (plate-rooted supports first). If routing ever lands one, it uses the same contact constructor as a tip (H1 includes feet).

### 5.8 Each limit and where it is built

| Limit | Built in by | Build value vs limit |
|---|---|---|
| H1 | `probe`: conservative upper bound G of every connected patch the whole head makes (ball, cone, joint, any second sheet), on the frozen f32 mesh | G ≤ 0.48 vs 0.50 **[O]** (§6.3) |
| H2 | `probe` and `commit`: patch envelopes ≥ 0.012 mm apart (lower bound \|c1 − c2\| − R1 − R2); reserved head regions do not overlap, inside the batch and against the plan | 0.012 vs fusion at < 0.010 |
| H3 | `Anchor` typestate: carrier printed no later than the member's lowest point, overlap ≥ 0.05 mm **[S]**; head pole cover (§5.5c); braces start on columns | 0.05 vs the checker's 0.01 |
| H4 | `PosedModel::new`: Z from the written f32 vertices; raft and base admission keeps ≥ 0.32 mm from the model | exact ≥ 5.0 **[O]** |
| H6 | `probe` checks the complete head, including the joint; `probe_members` checks all later members. Both require a certified gap lower bound ≥ 0.32 mm for every part outside its own contact-surrounding region. Departure is non-decreasing inside the region. The joint is always outside the region. | 0.32 vs 0.30 **[O]** |
| H9 | `Section::new(r)` refuses across-flats < 0.102; neck at the cone boundary exit; minimum ball/cone union width through the transition ≥ 0.102; joins only at ball centres with ball r ≥ end r | 0.102 vs 0.10 **[O]** |

### 5.9 Data flow, batch first

STL → `PosedModel` (f32 mesh, BVH) → **one** slicing pass over all layers (triangles bucketed by layer span; per-band vector slabs and run-length rasters; per-band union-find, then bands joined) → per-triangle batches: site class (ray batch) and overhang class → sample points (batch) → planner rounds (one `probe` batch and one load batch per round; layer bands in parallel, ≥ 1 band per task) → `commit` (sorted by candidate id) → member probes in one batch → raft → freeze → emit.

Debug counters assert ≥ 64 items per call (dev-loop §8). Targets on Round 25 **[S]**: generator ≤ 3 s, checker ≤ 5 s at 8 threads, job peak memory ≤ 1 GB (dev-loop T2). No dense 3D grid; no subprocess per candidate.

---

## 6. Construction API (`crates/support`), sealed

### 6.1 Types

```rust
// plinth_support::construct — all fields private; no Clone on Quote, MemberQuote or plan types
pub struct Limits { /* parsed from limits.toml by support's own parser */ }
pub struct PosedModel { /* f32 written mesh, BVH, hash, translate */ }
impl PosedModel {
    pub fn new(input: &Mesh, keep_up: KeepUpRecord, limits: &Limits) -> Result<Self, PoseError>; // Z from written vertices
}

pub struct PatchBound { lo: f64, hi: f64 }   // only made by construct::patch (exact disc∩triangle sums)
pub struct CappedPatch(PatchBound);          // only via PatchBound::cap(self, &Limits) -> Result<_, Reject>
pub struct Section { r: f32 }                // Section::new(r) -> Result<Section, TooThin> (across flats ≥ 0.102)
pub struct NativeBall { start: [f32; 3], end: [f32; 3], r: f32 }   // exactly the written fields
pub struct NativeFrustum { start: [f32; 3], end: [f32; 3], r1: Section, r2: Section, tail: [f32; 2] }

pub struct ContactCandidate { pub id: CandId, pub site: SurfacePoint, pub ball_r: f32, pub standoff: f32,
                              pub axis: [f32; 3], pub neck: f32, pub joint_r: f32, pub head_len: f32 }
pub struct Quote { plan: PlanId, rev: u64, cand: CandId, ball: NativeBall, cone: NativeFrustum,
                   joint: NativeBall, patch: CappedPatch, envelope: HeadEnvelope, clear: HeadClearance,
                   digest: FieldDigest }                       // digest of the exact written fields and vertices
pub struct QuoteBatch { /* Vec<Quote>, plan, rev, limits id, process id; not Clone */ }

pub struct PlanBuilder<S> { /* PlanId, PosedModel, Limits, process record, reservations, rev */ }
impl PlanBuilder<Contacts> {
    pub fn new(model: PosedModel, limits: Limits, process: ProcessRecord) -> Self;
    pub fn probe(&self, c: &[ContactCandidate], out: &mut Vec<Result<Quote, Reject>>); // parallel, pure; checks the batch against itself too
    pub fn commit(&mut self, q: QuoteBatch) -> Result<Vec<ContactId>, BatchReject>;     // all or nothing; stale → re-check pairwise or Stale
    pub fn get(&self, id: ContactId) -> Result<&Contact, ForeignHandle>;
    pub fn into_routing(self) -> PlanBuilder<Routed>;
}
pub enum Anchor { Plate, Column(ColumnId, LayerZ), Contact(ContactId) }
impl PlanBuilder<Routed> {
    pub fn probe_members(&self, m: &[MemberProposal], out: &mut Vec<Result<MemberQuote, Reject>>);
    pub fn commit_members(&mut self, q: MemberQuoteBatch) -> Result<Vec<MemberId>, BatchReject>;
    pub fn raft_connected(&mut self) -> Result<RaftId, Reject>;   // pads + MST bridges, admitted like members
    pub fn freeze(self) -> Result<SupportPlan, IncompletePlan>;   // & accessors only
}
pub enum Reject { AreaOverLimit { hi, max }, PatchUnverified, SecondSheet, WouldFuse { with }, RegionBlocked { by },
    HeadTouchesModel, DepartureNotMonotone, PoleUncovered, GapTooSmall { gap }, PrintOrder { layer },
    TooThin { across }, NarrowJoin, RaftTouchesModel, InsufficientCapacity, NonFinite, Stale, ForeignHandle,
    UnknownShape, MeasureExhausted, ResourceExhausted }
```

The typestate (`Contacts` → `Routed` → frozen `SupportPlan`) is dev-loop §7.2's. There is no contact admission after routing, and no edit after freeze.

### 6.2 How the guards work

- **Written numbers are the checked numbers.** Every check runs on the f32 values after rounding, converted to f64. The native-shape kernel (moved to `geom`, §10) builds the exact vertices; `plinth-emit` copies fields and vertices bit for bit and invents nothing. Test: emit → parse → fields and vertex bytes equal the plan's.
- **A quote is bound to one plan state:** plan id, revision, limits, process record and a field digest. If the revision changed, `commit` re-runs the pairwise tests (H2, regions, H6 against new members) for the whole batch, or returns `Stale`. A failed commit changes nothing. A quote cannot be edited, copied or used in another plan.
- **Additions keep earlier guarantees.** A member is admitted only if its full native shape keeps every admitted patch, region and gap.
- **Load is not an admission rule.** Ratios are sized in the planner and verified on the frozen plan. A miss gives a warning, never a resize (FR-21, FR-22).
- **No bypass.** A model foot uses the same contact constructor. There is no `inside_cavity: true` input; this milestone has no cavity exemptions.

### 6.3 H1 geometry and margins

Let `T_mesh` be the patch area of the written, validated closed support mesh and `T_analytic` the patch area of the reconstructed native analytic solid. Until the consumer experiment establishes which representation ChituBox slices, keep these as separate quantities. A validated enclosing analytic solid may provide `G ≥ max(T_mesh, T_analytic)`, but analytic inside cells do not provide a lower bound for `T_mesh`. The checker computes separate certified intervals for each supported representation, or reports the combined interval `[min(L_mesh, L_analytic), max(U_mesh, U_analytic)]`; it never requires that this combined interval have a width smaller than the actual representation difference. Capacity uses a lower bound valid for every still-possible representation. Establish mesh closure, orientation, native topology and the required inner and outer containment independently; vertex inscription alone is insufficient.

A 0.010 mm² checker excess allowance applies only after establishing `U_i − T_i ≤ 0.010 mm²` for each representation `i`. With `G ≥ T_i` and admission at `G ≤ 0.48 mm²`, this implies `U_i ≤ 0.49 mm²`. The additional 0.010 mm² is a construction reserve, not a bound on an unknown ChituBox substitute. An unbounded representation remains unverified until the consumer experiment settles it. Numerical slack and oracle error must be derived or enclosed; agreement with a finer point grid alone is not a certified error bound.

- **Generator bound G:** for the analytic sphere from the f32 fields, an exact f64 sum over model triangles within R of area(triangle ∩ disc in that triangle's plane). This is exact for a flat-faceted surface. Add derived fp slack (≤ 1e-9 mm² per triangle) and a 1e-5 mm radius inflation for the ≤ 2-ULP field-to-mesh reproduction difference (format doc §19): about 2.5e-5 mm². The mesh lies inside the inflated analytic solid (checked by containment, not vertex inscription alone), so G ≥ max(T_mesh, T_analytic).
- **Capacity lower bound:** the in-sphere of the native 242-vertex ball is contained in both the closed mesh and the analytic sphere, so its patch is a lower bound for every supported representation. Used for capacity only, never for H1.
- **Admit iff G ≤ 0.48 mm²** **[S]**. Recompute the reserve, never the limit, if the checker's bound changes.
- The largest admitted mark on a flat face is 0.78 mm across, not 0.80.

### 6.4 Compile-fail doctests (T1c, at most 10)

1. `PatchBound { lo: 0.1, hi: 0.3 }` outside its module.
2. `CappedPatch(b)` outside its module.
3. `Section { r: 0.04 }` literal.
4. `plan.get_mut(id)`.
5. `ContactId { plan, ix }` literal.
6. `quote.clone()`.
7. `PlanBuilder<Routed>::probe` (no contact admission after routing).
8. `probe_members` given a bare `f32` instead of a `Section`.
9. Changing a `SupportPlan` after `freeze`.
10. `PosedModel { translate_z: 4.0, .. }` literal.

Runtime tests cover foreign-plan handles, wrong model or process identity, stale revision, and committing a consumed batch twice. Compile-fail tests prove access rules, not geometry.

---

## 7. Independent checker (`checker/` only, no shared code)

### 7.1 Common ground

- Its own reader and STL reader already exist. It reads the exact candidate bytes and every support mesh.
- **Analytic solids from record fields:** spheres from the poles; frustums from start, end, r1 and r2 (type 5 reversal handled); raft pads and bridges from the mesh.
- **Mesh validity and containment:** each record mesh is closed, consistently oriented and has the native topology; it lies inside its analytic solid inflated by 1e-5 mm; and it contains the inner shape the checker uses for any lower bound (for a ball, its face in-sphere). Vertex inscription alone is not enough: an inscribed mesh with missing faces fails. If any part fails: `unverified: mesh does not match fields` for every geometric rule.
- **Contact chains, found independently.** There is no native parent id, and producer ids are not trusted. Tip and foot chains are identified from record kinds, end-point relations and geometry. A shared trunk below several heads does not join their patches. An unclear ownership split is `unverified`, never the favourable grouping. Part roles come from the native samples and are stated in the report.
- **All support–model intersections** are searched: joints, feet, braces and raft too. An unclaimed touch is an incidental contact and still counts for H1 and H2.
- Its own f64 BVH. Native f32 coordinates are exact inputs; arithmetic error, reconstruction envelope and unresolved classification are separate report fields.
- Anything it cannot measure is `unverified`, never `pass`.

### 7.2 Measurements

| Rule | Method | Bound | Validated on |
|---|---|---|---|
| **H1** | Done separately for each supported representation i (analytic solid from the fields; validated closed record mesh). For each part near the model: a square grid in the plane of each nearby model triangle, base cell 0.010 mm **[S]** (PRD §6.2). A cell is **inside** if all 4 corners are inside one convex part of representation i (exact for convex solid ∩ plane); **outside** if the 3D signed distance to representation i at its centre > its half-diagonal. Otherwise split, down to 0.00125 mm. L_i = inside cells; U_i = all cells not proven outside. Analytic inside cells never count toward L_mesh. A patch is a connected set of U cells across triangle seams. Parts of one chain count once (cell union). Tangencies are recorded even at zero area. Reported: each [L_i, U_i] and the combined [min L_i, max U_i]. | U_i − L_i ≤ 0.010 mm² per patch and representation, else `unverified` (never rounded down); the combined width is not required to be smaller than the true representation difference. Pass iff max_i U_i ≤ 0.50. A substitute shape is unverified until the consumer sample settles it. | Sphere on a plane, R ∈ {0.05, 0.1, 0.2, 0.39, 0.42, 1.0}, s/R ∈ [0, 0.99], truth π(R² − s²), planes tessellated at random (1,000 triangles). Convex 65°/90°/115° edges (exact sums of circular segments). Two parallel sheets; a concave recess. Frustum on a plane at an angle (ellipse). Sphere on a 64-gon cylinder (oracle: 0.0005 mm grid). Sphere–cone overlap counted once. 10,000 random cases: largest excess recorded, must be ≤ 0.010. T = 0.48 and 0.49 pass, 0.505 fails. A 0.48 mm² analytic patch with the plane normal to the native ball axis, checked separately against the written mesh (difference ≥ 0.0122 mm²); an inscribed mesh missing faces (`unverified`, no lower bound and no capacity claim). Oracle error derived or enclosed, not taken from grid agreement. **Native:** all 97 tip balls in samples 03, 04c, 04h, 05 and 07 against the native Round 25 mesh, vs a 0.0005 mm oracle. Expected on a flat underside: Heavy ≈ 0.471, Middle ≈ 0.188, Light ≈ 0.063, Ultralight ≈ 0.031 mm². |
| **H2** | Smallest distance between every pair of distinct patches, with bounds, including patches on different model sheets. Fused if a pair is < 0.010 mm apart or one patch touches ≥ 2 chains. | ± 0.0018 mm (leaf cell diagonal); a pair straddling 0.010 is `unverified` | Two balls at gap 0 and 0.005 (fused), 0.05 (pass); two 0.20 mm² patches touching (neither oversized); native 03 heads |
| **H6** | Region(P) = the chain's ball, neck and tip cone within 3.0 mm of P's centre, ending before the joint and column. Every other support surface is tested against every model surface. Sphere: exact. Frustum: axis-distance minus maximum radius is used only as a broad-phase conservative rejection of distant pairs. For unresolved finite frustum/model pairs, compute bounds on the distance between the actual finite convex solid, including its caps, and each model triangle, or use independently validated emitted-mesh distance bounds. Refinement stops successfully only when the full geometric distance interval meets the required tolerance; radius variation alone is not an error bound. A piece crossing the region boundary counts as outside. Reports departure inside each region, and any region that is false or longer than 3.0 mm. | Maximum interval width 0.005 mm established on the supported shapes, otherwise `unverified`. Pass iff the lower bound ≥ 0.300 and zero incidental contacts | Cylinder parallel to a plane at 0.28 / 0.30 / 0.33; a radius-0.25 cylinder cap 0.33 mm from a perpendicular plane (must certify without exhausting refinement); oblique end-cap and joint cases; a tilted frustum near a 90° edge; a column touching 3.5 mm from a contact; a joint touch; a far-wall touch inside a claimed region |
| **H9** | Measure the minimum load-bearing width of the actual union through every ball/frustum and member/member join. Do not locate exposure by centreline–sphere intersection. For a coaxial analytic sphere and frustum, exposure starts when the frustum boundary leaves the sphere; a boundary point satisfies t² + r(t)² = R², and the minimum union width must still be evaluated through the transition. Use the emitted mesh's section polygons for mesh geometry and retain certified lower and upper width bounds for every supported representation. For polygon sections, evaluate the exact candidate directions for minimum width or include a proven angular-sampling error. Pass H9 only when the lower width bound is at least 0.1 mm; unresolved joins are unverified. Away from joins, a member's width across flats is 2r·cos(π/16) at its narrowest section. Raft: thickness 0.2. Ball tips, mesh edges and one-layer slivers are excluded (PRD). Unknown member or join kind is `unverified`. | Certified lower and upper bounds; exact to f32 (1e-6 mm) where the candidate directions are exact | r = 0.050 (0.098, violation), 0.052 (pass); two r 0.3 rods joined by a thin offset lens (violation); ball R 0.2 with coaxial frustum r(t) = 0.012 + 0.2t from the ball centre (centreline exit width 0.1020, union minimum 0.0995 at t ≈ 0.1937: violation). Native "forked small" r 0.05 arms will show 0.098: a finding to record, not a limit change. |
| **H3** | Its own slicer. Plate z = support minimum, which must equal the model's local minimum − Z within 1e-4 mm, else `unverified`. Each layer is sliced at its bottom, middle and top and at every vertex height inside it. Polygons overlapping by ≥ 0.01 mm depth are joined. A component is rooted if it is on the raft at layer 0, or overlaps a rooted component of layer k−1 or a rooted model component (itself rooted in print order) by ≥ 0.01 mm. Model is not seeded as rooted. Lists model islands to match the generator's FR-29 list. | 0.01 mm overlap; result must not change across the sample planes | Brace starting one layer before its carrier; ball pole floating under a too-shallow head; a false carrier made from an omitted island (violations); native 07 braces (pass) |
| **H4** | Model world z = v.z − local min z + Z, exact. Lowest model point ≥ 5.0. Raft bottom at world 0. Model to (raft ∪ plate flares ∪ plate bases) gap > 0 by the H6 method. | Exact on f32 | Z = 4.95 (violation); 5.0 (pass); raft grown under the model; model touching a raised plate foot |
| **H7** | Its own STL reader. Input = approved keep-up STL (hash) + recorded pose: identity rotation plus bounding-box centring in f32. The chain original → +115° about X → approved is checked against the recorded hashes. Triangle by triangle, triangle order free, cyclic vertex rotation allowed; reversed winding and missing duplicates are not. Count, volume and area equal. Allowed edits: none. | Vertex tolerance 1e-5 mm | One vertex moved 0.001 mm; a triangle dropped; scale 1.001; reversed winding (violations); a cyclic rotation (pass) |
| **H5** | §7.4 | Combinatorial, exact | §7.4 |
| *Load audit (report only)* | From the file: model layer areas per component, contact heads and B_i = min(patch lower bound over all still-possible representations, neck). Gives the largest additive area ratio X_group = A_k(C) / Σ B_i and the utilisation X_group / 12, under those names, with the same definitions as §4.3, implemented independently. The generator's X_group must agree. Where the generator's rigid-lever result is valid, its largest contact-path ratio must be ≥ X_group (a weighted mean cannot exceed its largest term). X_group > 25, or positive area with zero section, is reported as unprintable. | — | Catches a generator that drops a component or a layer; 60 mm² over 2 mm² of section reports ratio 30, utilisation 2.5 and the unprintable trigger |

### 7.3 Where the checker's bounds come from

Native examples prove decoding and native-shape handling. They do not give exact contact truth by themselves. Truth for analytic and restricted native cases comes from a separate, finer oracle in test code. Never calibrate the checker against generator output. Each analytic case records containment, the widest interval and the refinement work.

### 7.4 H5 "not applicable" proof

Weld only vertices that are exactly equal in f32. Every edge must be used twice with opposite direction, and there must be no self-intersection; otherwise `unverified`. Count shells, give each a signed volume, test nesting. Count enclosed voids in the **model only**, not the support scene. "Not applicable" only with exactly 1 shell, volume > 0, 0 enclosed voids and 0 drain bores:

`H5: not applicable — emitted model has 0 enclosed cavities and 0 drain bores; topology verified.`

The open underside recess is not a cavity. A bad shell makes H5 `unverified`, never not applicable. The FR-17a cup result (§4.6) stays separate. Round 25: 1 shell, +851.9 mm³ **[M]**. A cube with an inward inner shell (the existing broken file) makes H5 apply.

### 7.5 Geometry acceptance is separate from H8

The current checker turns any unverified rule, H8 included, into an unverified overall result. Step 11 splits the status:

- **Geometry accepted:** zero violations and zero unverified for H1–H4, H6, H7, H9, and the proved H5 not-applicable.
- **H8 pending** until an external record names the exact hash, app version, what was seen and the slice result (dev-loop §7.5).
- `ready_for_owner_review` is not `credible_to_print`. Print eligibility needs geometry accepted, interventions resolved, H8 on the exact print file, and the owner's judgement (PRD §8.3).

No CLI path may turn "all checks built so far passed" into full acceptance.

---

## 8. Tests

### 8.1 Per rule

Every negative is built through the **production low-level writer**, not byte patching. For every row, the normal-CLI test runs the real `plinth-check` subprocess through `plinth_cli::check::run` and the publish step, via an internal test seam (no public bypass flag). Assert: no print candidate, no geometry acceptance, the rule id and location in the report, nonzero exit. Repeat once with an unverified measurement and once with a stale file hash. Other failures may also appear; the intended rule must fire.

| Rule | Positive control | Production-writer negative control | Smallest defect seen |
|---|---|---|---|
| H1 | Native 03 Heavy tip on Round 25; ball with patch 0.48 (analytic and mesh intervals reported separately, plane normal to the ball axis); generated Round 25 | Ball R 0.45, s 0 (0.636 mm²); patch 0.505; oversized model foot; ball + cone union over the limit when the ball alone is not; inscribed mesh missing faces (`unverified`, no capacity claim) | > 0.50 − (U − T), at most 0.51 |
| H2 | Two patches 0.012 mm apart | Overlapping patches; gap 0.005 | < 0.010 |
| H3 | Native 07 braces; plate-rooted head, column and brace; generated Round 25 | Brace one layer early; floating ball pole; false carrier from an omitted island | 1 layer |
| H4 | Lowest point 5.0 | 4.95; model on the raft; model on a raised plate foot | any < 5.0 |
| H5 | Round 25 → not applicable | Inner void with no drain (existing); malformed shell → `unverified`, not N/A | 1 shell |
| H6 | Rod at 0.33; r 0.25 cylinder cap 0.33 from a perpendicular plane (certified without exhausting refinement) | Rod at 0.28; clear centreline but surface at 0.29; joint touch; incidental touch 3.5 mm from a contact; region longer than 3 mm | 0.295 |
| H7 | Generated Round 25 vs the approved STL | Moved vertex, dropped triangle, scale 1.001, reversed winding | 1e-5 mm |
| H9 | r 0.052 | r 0.050; 0.09 mm neck; thick rods joined by a sub-0.1 mm lens; ball R 0.2 with coaxial frustum r(t) = 0.012 + 0.2t (union minimum 0.0995; H9 itself must reject it) | 0.1 − 1e-6 |

`MEASURED` in `broken.rs` gains each rule in the commit that adds its measurement. The H5 proof is its own test. Each sabotage test documents what smaller or different defect it cannot see.

### 8.2 Properties (T0p)

64 fixed-seed cases in the fast suite; 1,024 at the checkpoint. Cases: candidate batches on Round 25 and on synthetic shapes: plates at 0 / 30 / 45 / 65 / 90°, a ridge, a V-groove, a 0.3 mm wall of two sheets, a 0.5 mm cylinder, a sphere, f32 rounding edges, simultaneous conflicts in one batch.

1. **0 admitted patches above 0.48 + oracle error**, by a brute-force 0.0005 mm oracle in test code only.
2. **0 fused admitted pairs** (oracle distance > 0.010), including pairs within one batch.
3. **0 admitted H6 or H9 violations** by the oracle.
4. **Every `Reject` variant hit** by a targeted generator, with per-variant counters asserted > 0. Random cases alone are not a witness.
5. A failed commit leaves the builder unchanged. Every admitted quote's digest matches the emitted fields and vertex bytes.
6. Batched kernels (disc ∩ triangle area, segment–mesh distance, rays, slicing) equal their scalar reference.
7. Load model: shares sum to the layer area; spring shares are tension-only and balance force and moment (a negative reaction is never clipped and renormalised without a moment check); no sum across layers; no share to a contact not yet printed; an omitted island never carries; a separate island is sized only against its own contacts until merger; a bottom-step worst layer is found; support self-peel is counted; merged loads add. A singular, ill-conditioned or unbalanced layer is unresolved, never a low-ratio pass. A layer whose centroid lies outside a near-collinear contact cluster gets a "load path not assessed" intervention, never an information-only result, even when its additive ratio is below 12; a tributary value above 12 alone is not an intervention; a brace growing before it joins its partner has its self-peel counted and its bending reported as an intervention. 60 mm² over 2 mm² of section reports X_group 30, utilisation 2.5 and the unprintable trigger; positive area over zero section is unprintable.
8. Same plan and same bytes at 1 thread and the slot's allocation (bench epoch: 3 and 8).
9. Planner anchoring: both Round 25 islands have recorded first-layer rooted anchors after all admission rejections. A synthetic island whose only feasible early anchor is class 2 gets a reported class-2 anchor. A case where the nearest class-1 proposal fails but a different class-1 proposal succeeds is anchored. A case with no anchor reports "search budget exhausted" or "proved no legal anchor" and is unprintable.
10. **Certified sizing.** The planar contact proposed at r_e 0.2 mm, R 0.212 mm, s 0.0772 mm (analytic patch 0.1225 mm², certified in-sphere ratio 12.69 at its analytic-sized load) must either be resized within the private proposal until its certified ratio is ≤ 12, or be reported as constrained with additional contacts requested. Repeating the analytic dimensions for eight rounds must not be classified as a satisfied plan.
11. **Joint clearance at admission.** A joint with a 0.10 mm gap to a second model sheet, while the ball and tip cone meet all their own constraints, is rejected by `probe` as `GapTooSmall`. It never reaches commit or emission.
12. **Absent route stays blocking.** A source with a confirmed absent rooted material route remains unprintable through report generation and candidate-status evaluation, even if an intervention-acceptance record is supplied. A curved but connected route is not classified as absent merely because its straight nearest-contact segment leaves the material.

### 8.3 Step 12 checks on the written Round 25

1. **Rim-band coverage.** Measured on the written file. Every point of the rim band below 45°, on the 0.25 mm grid, has a contact within 3.0 mm whose patch prints no later than that point. The report states the number of rim-band points, the number covered, and the number of class 2 contacts with their patch areas.

---

## 9. Review pictures and report

Render the **written file**, not the plan. ChituBox-like colours: model grey-blue, supports a distinct colour, patches red, warnings orange. Outlines and labels as well as colour. Captions and legend on the image. Two views per warning (FR-32). No coordinates without a picture.

| # | Picture |
|---|---|
| 1 | **Before / after, same camera:** the approved 65° base at its lift, then supported with the raft. Labels: hidden underside, top, low edge, 65°, "lift 5.0 mm". |
| 2 | Underside view from the low side (same view as the approval picture). Contacts numbered, coloured by site class. The 5 largest patches labelled in mm². Smallest spacing marked. |
| 3 | Close-up: the bottom cluster on the lip edge, with the first island. Count, largest patch, head angles. |
| 4 | Close-up: the lowest point of the rib (island 2) and its contacts. |
| 5 | Close-up: the inner lip top band and the lip end-face contacts covering it. |
| 6 | **Worst contact:** patch outline, its area interval, head departure, the 3.0 mm region, smallest outside gap. |
| 7 | **Worst load:** (a) layer area against height, with the "12 × active capacity" line and the worst layer marked; (b) the peeling layer, its source region, assigned contact and full path to the plate, labelled "assigned area / section = ratio" (not patch area); (c) side view with supports coloured by ratio (< 8 green, 8–12 yellow, > 12 red). |
| 8 | The two islands: first-layer slice with the island in red, plus a 3D arrow, to match ChituBox's island report. |
| 9 | Raft from below: pads, bridges, base joins; one connected piece. |
| 10 | Model coloured by need: needs support / self-supporting (≥ 45° or by reach) / covered. |
| 11 | Each warning region, two views. |
| 12 | **Rim band:** the outer rim band below 45° and its contacts, two views. Each rim contact numbered, with its patch area. Rim-band points not covered within 3.0 mm, if any, in orange. |

**Report contents:**

- **Identity:** original, approved, generated (and any edited) file hashes; pose and keep-up record; FR-12 pose line.
- **Process:** layer height and its source, all physics values, P5 = 12 shown prominently.
- **Checker:** each rule with value, interval and location; H5 proof; cup result; H8 status as a separate record; geometry status vs `ready_for_owner_review`.
- **Load:** the §4.7 block, the assumptions table (§4.5), the evidence table (§3).
- **Supports:** contact count by site class (not native record count); largest patch (checker U); smallest patch spacing; support density per cm² of the hidden underside (not the whole surface); thinnest section; smallest gap outside regions; actual spacing beside his profile settings, with the reason for any departure (FR-30).
- **Volumes and height:** model, supports and raft (mL), overlap treatment stated; print height.
- **Run data:** time per step, batch counts, rejection reasons, job peak memory.
- **Warnings** by class, and the FR-35 risk summary.

Every number states its unit, what it counts, what it does not, its denominator and the file it came from. "0 fused pairs" does not prove tool access everywhere. "Geometry accepted" does not mean a good print.

After writing, replay the load model on geometry decoded from the written file (measured sections, actual contact places). Label it "generator load convention applied to the written file", not independent physics. No planned radius or stale load table is a headline number.

---

## 10. Work split (3 lanes) and merge order

**Step 0 (one commit, before the lanes; the orchestrator or one agent):**

- Move the pure f32 native-shape mesh kernel from `crates/emit/src/shapes/` to `crates/geom/src/shapes/`. `emit` keeps record building and calls the kernel. Existing native-shape tests pass unchanged in meaning.
- Module stubs and `lib.rs` exports for every new module below, with signatures. Bodies return `Unverified` or an empty result. No `todo!`.
- The load model's data API, `plinth_support::load::{LayerStack, LayoutView, LoadReport}`: plain data (contact centres, patch lower bound, neck area, part chains), not sealed types.
- The checker JSON fields for the geometry / H8 split, and the rejection list.

After this no lane edits a shared file.

| Lane | Owns (exclusive write set) | Delivers |
|---|---|---|
| **C: independent checker** | `checker/**`; `crates/acceptance/tests/it/{broken.rs, rule_blocks.rs}` | §7 measurements, analytic and native validation, near-limit negatives through the production writer, per-rule normal-CLI tests, the H5 proof, the geometry / H8 split, the load audit |
| **B: layers, load model, report** | `crates/geom/src/{slice.rs, raster.rs, label.rs}`; `crates/support/src/load/**`; `crates/support/src/report_data.rs`; `crates/cli/src/report.rs`; new `crates/render/**` | Slab envelopes, rasters and labelling (batched, with scalar references); components, islands, cups, reach, overhang and site needs maps; rigid-carrier spring-sharing loads with equilibrium and conditioning assessments, and the additive, equal-share and tributary results; section loads; worst layers; warning data; the FR-21 report and pictures 1–12; §0 numbers reproduced within 2% |
| **A: construction** | `crates/geom/src/{shapes/**, bvh.rs, patch.rs, clearance.rs, ray.rs}`; `crates/support/src/{construct,place,route,raft}/**`; `crates/support/tests/it/**`; `crates/emit/src/**`; `crates/cli/src/{run.rs, check.rs}`; `crates/cli/tests/it/e2e.rs` | Sealed types, `probe` / `commit`, tip family, heads, planner loop (on B's API), columns, braces, bases, connected raft, freeze → emit, CLI wiring and status, T1c and T0p tests, the go/no-go loop timings redone with real construction edits, L1 for support within budget (5 s target) |

**Merge order:**

1. **Step 0.**
2. **C first.** It needs no other lane. With it, the Round 25 transport file reports H4 pass, H7 pass and H5 not applicable. The orchestrator updates `fixtures/round/stage-manifest.tsv` in the same merge.
3. **B second.** It changes `geom`, which rebuilds most crates, so it lands early.
4. **A last, rebased on B.** A builds admission and routing at fixed sizes while B works, then wires in the planner.

Then the orchestrator switches the Round 25 stage row to "supported": expected pass {H1, H2, H3, H4, H6, H7, H9}, H5 not applicable, H8 pending. Then step 12.

At most 3 lanes write at once. One cross-family reviewer covers lanes A and C (limit code). Lane B gets no reviewer beyond its tests.

---

## 11. Step 11 is complete when

- Analytic and native controls show the checker's interval bounds (H1 width ≤ 0.010 mm² per representation, H6 interval width ≤ 0.005 mm including caps, H9 certified union width).
- Every positive, production-writer negative, normal-CLI and compile-fail test passes; every `Reject` path has a measured hit.
- Independence check, format and lint are clean.
- L1 for support within the dev-loop L1 budget (15 s) on a rebuilt edit, with 5 s as the reported target (compile alone is about 2.5–4.5 s); the go/no-go edit-loop budgets hold with real construction edits (dev-loop §3.3, §10).
- Memory and threads stay within dev-loop §5.6 and §8; no dense whole-printer 3D grid; no per-candidate subprocess.
- Output is written atomically; a checker failure cannot produce candidate status.

Step 12 then generates the supported Round 25 with zero violated or unverified applicable rules, passes the §8.3 checks, checks L3 ≤ 90 s, and the orchestrator inspects the written file and pictures. Then the H8 record and the owner's judgement. Ask before printing. Neither a passing checker nor an accepted warning is the milestone; only the owner's accepted print and removal is.

**Risk and rollback.** The main risks are a false geometric pass, an under-modelled load path, and a status mistake that labels a file printable. All fail closed to a labelled diagnostic. There is no data migration; the approved STL, native samples and hard-limit values do not change. Reverting the supported-output path restores the labelled transport result, never an unsupported file called printable.

---

## 12. Open points

**For the owner:**

1. **The post-output ChituBox sample** (PRD §12): at step 12, he opens, saves and slices a plinth-written file so we learn which solid ChituBox slices. Required before printing (PRD §8.1). The checker's hard-limit verdicts are measured on the written canonical support triangles only (ruling 2026-10-31, gpt-astra accepted with conditions), so this test must discriminate: a differential pair (one project with native supports, one with the same triangles as an ordinary mesh at identical coordinates), including an oblique tapered start and a sphere/contact boundary placed at layer and pixel phases where the true-circle and polygon predictions differ visibly. Compare decoded layer masks, not screenshots, before and after a native resave; record files, hashes, ChituBox version and slicing profile. The earlier load test (format spec §18) covered model-only files, so supported files are still untested.

**For the orchestrator (decide from results, no owner input needed):**

2. P5 = 12 at full layer area may give many contacts at the cap **[U]**; no count is predicted or targeted (§4.9). That may look over-supported next to his Light practice. The PRD §8.4 side-by-side print settles P5 vs 18 and whether A2 holds for strips. Ask before printing.
3. The tributary result does not size contacts; it is a labelled sensitivity diagnostic. All sharing results are classified under §4.8, and its unprintable conditions take precedence: a confirmed absent rooted material route, or an additive area ratio > 25. Any other additive, equal-share or valid rigid-lever result above 12, and any unbalanced or unassessed load response, is a located intervention. If the first print loses the lowest contacts, the moment imbalance of the near-straight bottom cluster is the first candidate cause.
4. ChituBox's own thin native parts (r 0.05) measure 0.098 mm across flats, under H9. Record as a finding; the limit does not change.
