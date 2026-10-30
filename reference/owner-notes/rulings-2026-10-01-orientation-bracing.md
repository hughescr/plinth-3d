---
name: rulings-2026-10-01-orientation-bracing
description: "Craig 2026-10-01: no heuristic can tell a functional flat face (table top) from an undetailed underside — pose is the user's call via 'as drawn' (Finder tag plinth-no-rotate / Linux user.xdg.tags / --orient none / <stem>.asdrawn marker); slenderness S 50→30 interim pending a coupon; keep one-direction crosses (never prune); fat tips accepted"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-08T20:28:06.383Z
---

Craig, 2026-10-01:

1. **Flat faces:** "some flat faces are important to keep clean and some are
   'nobody added any detail here cos it's the underside' … I can't think of a
   heuristic way to detect this — it's really more of a subjective/analytic
   question which requires understanding of what the model is." Examples: a
   conic base (big flat face DOWN wants supports) vs a table (big flat top must
   stay clean). → Do NOT add a flat-face heuristic. The user decides pose for
   such models.
2. **"Use as drawn" triggers** (implements issue #1): his Finder tag
   `plinth-no-rotate` (xattr com.apple.metadata:_kMDItemUserTags, bplist; he
   tagged /Users/craig/code/hughescr/dnd-campaigns/cube.stl as the example);
   Linux `user.xdg.tags`; `--orient none` for a whole run; `<stem>.asdrawn`
   marker as the cross-platform (Windows) fallback. Tags are lost via git/zip/
   some shares — repo fixtures use the marker.
2b. **Also `--no-orient=file,file,…`** (Craig 2026-10-01, after round 10 launched): comma-separated model names to use as drawn; match with or without `.stl`, simple globs relative to the input dir; combines (OR) with tag/marker/--orient none; an unmatched name WARNS, never refuses; log names the reason. QUEUED as a follow-up lane after round 10 (not in the running orient-as-drawn lane).
3. **Slenderness S = 30** now (was 50, no field provenance), "30 now, coupon
   later" — design a slenderness coupon plate for him to print.
4. **Keep one-direction crosses** (ChituBox practice): never prune an |X| tying
   two columns just because it fails the every-direction test.
5. **Fat tips OK:** Round_50 "looks great", Sphere "very good/great" with tips
   up to ~1.6 mm across → no 0.8 mm cap.

**2026-10-02 follow-ups:** S 30 → **40** interim (Cylinder/Sphere "heavily over-braced" at 30; stilt better). Test-set models to use AS DRAWN via repo marker files (git drops Finder tags): Round_25/50/100, make_overhang_table (legs down), make_raking_blade (blade UP — authored blade down, so a flipped repo copy + marker). Everything else stays auto-oriented. Finder tags are for Craig's own local model folders. Stilt plate: thick posts with one late X = consistent with the radius-scaled limit (he accepts). Morel_base defects reported: brace end gap at its post; long plate-to-mushroom posts that should land on the lower model; ≥2 such posts clip through the model (clearance guard missed arm shafts).

**2026-10-02 evening, bracing (Craig):** thickening-only left long skinny posts unbraced ("unlikely to work") — keep SOME bracing; but "every direction" as implemented is overkill. For 4 posts in a square (a-b over c-d) you don't need all 4 sides AND the middle X — probably just the diagonals a-d and b-c; "you'll have to do some analysis on the forces". Also: **braces DO carry load** — long thick 45° braces exert significant torque on the posts they hang from; he has been worried about brace length/thickness. → Next step is a real force analysis of bracing (brace self-load while printing as a cantilever, torque on posts, frame layout perimeter vs diagonals vs ChituBox, rung order, junction sizing) BEFORE changing bracing rules; the brace-joint "raise to plate" is NOT to be removed blindly.

**2026-10-03 (after bracing-mechanics.md, revision 2):** Craig approved ALL FIVE interim bracing rules: (1) R2 brace never thicker than the posts it joins (deletes junction raise-to-plate); (2) R6 band count from a multi-band frame model; (3) R7 brace only in a window ≈1.5×–3.3× the slenderness limit, thicken outside (partly reverses the 2026-09-05 crosses-first order); (4) R9 warning-only construction-stage shake check, p_b stated; (5) D9 two-post ceiling: past ≈3.3× thicken or brace to a wider frame. Brace angle (57.5°) and "every direction before seating" unchanged until a coupon. ALLOWED brace-to-MODEL ties (D8) where nothing else works: the tie lands on the model with a ROUNDED TIP like any support→model contact (easy removal), counts as a contact scar, and the support math must account for it (load path, sizing). Slenderness test plate POSTPONED — "not about to roll out the printer"; don't design/build it further until he asks.

**2026-10-05:** the 0.3 mm final clearance does NOT apply inside a hidden (sealed/interior) cavity — supports there are never removed or cleaned (consistent with the 2026-08-31 ruling); outside cavities the rule is unchanged.

**2026-10-06, articulated heads:** GO on head-first heads, skipping Phase 1 (no standalone drawn-field/validator). Head length = physics minimum (joint ball held a gap off the surface; visible neck ≈ gap + tip ball, ~0.3–0.6 mm) — "short's fine, if it will help". Implementer must REPORT the built neck length; Craig will judge in ChituBox whether the neck is distinct, reachable with cutters and breaks at the tip; if too stubby, raise the minimum (one number, e.g. ChituBox's 1.5 mm). NOTE: v17 has no heads — the footprint normal only changes seat depth, trim plane, disc grading and lip-seat angle, not the arm direction.

**2026-10-07, head geometry (after v18):** each part is sized for ITS OWN requirement — column for the column, head for the head, joint for the joint, "not mix-and-match" (never carry the joint ball's size down the column). Junction between two members at an angle = CONVEX HULL of the two end circles (a thin member buried in a thick post is the same hull hidden inside; same resin). Joint spheres: triangles only, so match facet count/azimuth with incoming members and make the sphere's inscribed radius an epsilon larger than every incoming radius.

**2026-10-08, heads on leaning arms (Craig, after v20 showed Round 25 +50% resin, ~2 mm joint balls):** a head ONLY WHERE NEEDED — a leaning straight arm whose shaft stays ≥0.3 mm clear of the model stays straight; it gets a head only where its shaft would graze/clip the model (outside hidden cavities). Replaces the 2026-10-07 orchestrator ruling "head on every off-normal contact". Steep-face heads as in v19 unchanged.

**2026-10-08, contact-size cap REJECTED (Craig, after the for-craig-v21-tip04 experiment):** capping contact radius at 0.4 mm by inserting more same-level contacts was "a shit-show of overlapping contacts and hard-to-remove supports", plus long, nearly horizontal stretched supports on the Sphere; +22–28% resin. "Not the right approach"; branch discarded. Fat bottom-row tips on degenerate models like the Cylinder are accepted for now ("rarely get something as degenerate as this cylinder"). Do not re-propose contact multiplication as the fix for fat tips.

Orchestrator rulings same day: scar fold must be tessellation-invariant (a
score must not depend on triangulation); lip-seat blame only for load a move
ADDED, longest offending move put back first; a landing branch counts as a
brace tie only once its seat has printed.

Related: [[rulings-2026-09-30-oversupport]], [[sweep-field-feedback-2026-09-29]],
[[plinth-state-2026-09-28]].
