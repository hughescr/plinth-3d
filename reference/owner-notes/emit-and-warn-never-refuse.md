---
name: emit-and-warn-never-refuse
description: "Craig 2026-09-01 ruling — a shortfall is a WARNING on an emitted file, never a refusal; an unsupported region emits anyway with a loud, locatable warning, because a refusal hands him nothing to inspect or fix"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-01T21:03:31.956Z
---

Craig overruled a design on 2026-09-01. I had specified that if no support
placement for a region could be decided, the region cannot be supported and
that is a legitimate refusal, reported as a named coverage failure. His ruling:

> "if a region can't be supported, then EMIT THE MODEL ANYWAYS with the supports
> that *do* work in place, and a loud warning about the fact some area(s) are
> unsupported."

**Why, and there are TWO reasons — the second is the stronger one.** First, a
refusal hands him nothing he can inspect, judge, or repair; a file with a named
gap he can open in ChituBox and support by hand, which he has said he would
consider doing. Second, and this is the one that generalises:

> "Partial success will let me then look at the model and fix it manually to
> print, or have a chance at refining the algorithm understanding where it
> failed. With *nothing* emitted, I can't see what's working and what isn't
> working."

**The emitted file is EVIDENCE ABOUT THE ENGINE, not only a deliverable.** A bad
file says which parts of the placement logic worked and which did not. A refusal
says nothing at all — which is exactly why support quality has been impossible
to make progress on, since there has been almost nothing to look at. The
engine's job is to do the work it can and be honest about the rest, not to
withhold output until it is perfect. See [[print-candidate-metric]] — he has
asked repeatedly for "a functional base so that I COULD give you feedback".

The same session showed the cost of the opposite disposition: correcting a real
geometry bug produced ZERO improved files across the corpus. Every model either
emitted exactly the bytes it always had, or emitted nothing. The corrected
knowledge became a refusal instead of a better answer — a wrong answer replaced
by no answer.

**How to apply:**

- A quality or coverage shortfall is a warning on an emitted file. Never a gate.
- **The warning must be ACTIONABLE**, which is the whole value of it: where the
  gap is (coordinates or a named region), how large, and how many candidates
  were rejected there. "Coverage incomplete" is useless; "unsupported area
  ~4 mm² near (12.3, -8.1, 22.0), 6 placements rejected" is what he can act on.
- This does NOT license going quiet. A dropped placement nobody hears about is
  still a silent deletion, which is its own well-founded fear in this codebase.
  Loud and local, never silent and never total.
- Related and same shape: an undecidable result at one support is a property of
  ONE ray at ONE placement, not of the model — move the support and the
  ambiguity is gone. Craig's own words: *"if a single support is bad or
  uncertain, then it should just get moved, no? Not abandon the entire model?"*
- Treat this as a general disposition of the engine, not a rule about geometry.
  When you find another refuse-on-shortfall path, name it for him rather than
  silently widening scope.

Related: [[supports-may-seat-on-model]], [[challenge-loops-not-halts]],
[[design-framing-optimism-bias]].
