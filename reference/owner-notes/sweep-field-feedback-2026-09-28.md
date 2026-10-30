---
name: sweep-field-feedback-2026-09-28
description: "Craig's ChituBox verdicts 2026-09-28 on d103c48 files (burden-arms-only/for-craig + lane3 fixtures-pinned): Round 25 good bar tip clipping; Round 50 UPSIDE DOWN + unclipped tips + spurious rim arms; dangling 'porcupine quill' arms on Cylinder/Sphere; trunk thinner than branch; duplicate X braces; masts glued to their arm; drain now clear; Sphere underside good"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-09-28T22:20:38.628Z
---

Craig in ChituBox, 2026-09-28, on `slice-sweep-2026-09-02/burden-arms-only/for-craig/`
and `lane3-congestion/for-craig/fixtures-pinned/` (HEAD d103c48). 14 screenshots.

**Good:** Round 25 "looks good" apart from tip clipping; Cylinder drain clear
inside and out ("should drain well"); cross-bracing on Cylinder looks good;
Sphere underside "much better", well spaced, no melted-together roots.

**Defects he reported:**
1. Tip clipping (Round 25 rim): the ball is cut by a HORIZONTAL plane (top
   half removed), not by the model surface — where the tip overhangs the model
   edge in X/Y the ball still sticks out, and the SHAFT is never clipped either,
   so a chunk of post pokes out past the rim. "Not a huge issue."
2. Round 50: tips NOT truncated at all (Round 25's are). Also emitted UPSIDE
   DOWN — supports land on the clean top; he guesses the three underside
   stiffener cylinders read as "detail" (ties to his 2026-09-05 ruling that
   detail = WIDE distribution of normal changes; uniform ribs are "change").
   He doubts it can be got right automatically. Also thin arms on the gently
   sloped outer edge "the model itself should adequately support".
3. Round 25 "brace to nowhere": a thin post with X braces whose top ends free
   under the model — that is a slenderness MAST (by design), but it looks odd.
4. make_tall_stilt_plate: similar-looking posts only SOMETIMES braced; one X
   for a whole post length; one X DUPLICATED (two Xs overlaid, slightly shifted
   in Z) — also on make_brace_pair and Cylinder exterior.
5. TRUNK THINNER THAN BRANCH at merge joints (stilt plate, lonely peak,
   brace pair): the descending post below the junction is thinner than the
   short post rising from it.
6. make_lonely_peak: mast placed so close to its arm the two fuse — he suggests
   a thicker-based tapering main post instead. make_brace_pair: mast slightly
   apart with a tiny X between near-touching posts — "very odd".
7. Cylinder underside still massively dense, thick roots merging; one fat
   diagonal arm "rules-compliant but very odd".
8. "PORCUPINE QUILLS": arms on the Cylinder's upper flanks (outside and inside)
   and on the Sphere seated on the model but connected to NOTHING below —
   dangling, themselves unsupported. Physically unprintable (each is an island).
9. Long thin arms high on the Cylinder exterior where the model should
   self-support; some appear to pass THROUGH model material (maybe rendering).

**How to apply:** Round 50 is a flat-disc failure → gate per
[[round-bases-first-goal]]. Unprintable geometry (quills, through-model,
trunk<branch) outranks cosmetic (clip shape, duplicate X). He did NOT rule on
S = 50 vs 70; his complaints are about brace/mast placement, not the ratio.

Related: [[sweep-field-feedback-2026-09-05]], [[plinth-state-2026-09-04]],
[[rulings-2026-09-05-merging-detail]], [[contact-area-over-seat-depth]].
