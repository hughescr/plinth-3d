---
name: lift-always-elephants-foot
description: "Craig 2026-09-02 ruling — never rest a model on the plate; every model is lifted, because base layers get long exposure for adhesion and swell (elephant's foot), giving rough edges on any surface touching the plate"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-02T08:37:17.713Z
---

On 2026-09-02 I recommended letting a large flat face (the round bases) rest ON
the plate with zero lift as a genuine zero-support pose. Craig rejected it:

> "Resting on the plate means having to deal with elephant's foot though — the
> base layers on the plate are exposed longer for better adhesion to the plate
> and that makes them swell slightly, leading to rough edges and ugly prints for
> surface on the plate; for that reason we always lift everything off the plate."

**Why:** the first layers use a much longer exposure so the print sticks to the
plate; the over-exposed resin spreads sideways ("elephant's foot"), so any model
surface printed directly on the plate comes out swollen and rough. Lifting puts
that swelling on the support raft, which is discarded.

**How to apply:**
- `lift_mm` is always applied; a zero-lift pose is never a candidate. Do not
  propose it again, for bases or anything else.
- A flat base therefore ALWAYS needs supports under its whole underside (or a
  tilt that trades support area for peel area). Flat-underside-on-supports is a
  normal, correct outcome for a base, not a defect.
- The orientation objective must weigh flat-lifted against tilted honestly with
  a continuous overhang term (see [[round-base-diagnosis-2026-09-02]]); the
  45° step is what let the disc escape the support term at 44.9°.

Related: [[round-base-diagnosis-2026-09-02]], [[saturn4-tilt-peel-geometry]],
[[physics-first-constants-doctrine]].
