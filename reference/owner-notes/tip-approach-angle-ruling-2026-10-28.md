---
name: tip-approach-angle-ruling-2026-10-28
description: "Craig 2026-10-28 — a support cone that pokes into the model (TR) is a placement fault, fixed by the tip's approach angle; trimming the overlap is correct; unavoidable cases minimise overlap within the physics"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-28T01:45:07.313Z
---

Craig, after seeing side-view diagrams of a slanted tip whose cone rises through the model surface: "The problem isn't the overlap removal, it's the positioning of the support on its approach to the model! You need to make its approach angle not suck to minimize the impact, not worry about cutting the overlap area. There could be some tight situations in valleys or something where there's no way to avoid incidental contact, but those will probably be pretty rare and in that situation you should just angle to minimize the overlap while maintaining structural support per the physics model."

Follow-up, same day: "'straight head drop' is almost always going to be wrong. You want an angled head where it's angle is chosen to (1) avoid hitting the model as much as possible and (2) provide maximum support to the contact – that latter part might encourage the thing to be vertical, but it must be withing the constraint of #1, *and* it's possible that better transfer of the support might be achieved with a head that's orthogonal to the surface or something rather than vertical?"
→ Every tip considers an angled head. Its angle is chosen by: (1) the constraint, no or least model overlap; (2) the objective, best support of the contact per the physics model. Vertical lines up with the pull, so the head bends least. The face normal seats the ball squarely, which gives a round patch with no shear smear. The physics model decides between them; vertical is not a default. A plain straight drop is just one candidate, and it wins only when it scores best.

**Why:** the fused scar is exactly the surface that the cone crosses. Trimming the overlap (support minus model) is correct and changes nothing. The cause is the approach angle.
**How to apply:**
- Treat TR as a placement fault. Fix it by setting the tip's approach angle (the cone near the normal, within the derived limit angle), not by trim or reader tricks.
- Where no clean approach exists (valleys, tight spots), pick the angle that MINIMISES the overlap and still meets the load and slenderness physics. Count the residue as a named warning.
- When explaining geometry to Craig, draw real side-view diagrams (an SVG artifact). ASCII art failed twice, and he got angry at the vague wording "wider mark".
- Never call a cut post "thinner": the trimmed part was never support.

Related: [[steep-face-contacts-reland-2026-10-23]], [[departure-rule-2026-10-21]], [[measure-from-emitted-file]], [[review-pictures-self-explanatory]].
