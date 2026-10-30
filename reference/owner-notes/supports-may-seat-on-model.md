---
name: supports-may-seat-on-model
description: "Craig's ruling 2026-08-13: a support is valid if it lands on the model, the base plate, or another support, provided the whole is adequately supported per physics. Cavity interiors MUST be supported, by supports seated on the lower interior of the cavity."
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-13T21:07:09.125Z
---

Craig, 2026-08-13, verbatim in substance: *"We DO want supports inside the model. The hollow must be
supported. But those supports will likely land on the lower part of the interior of the cavity, not
directly to the base plate. The connection to the base plate would be via the model itself and its
outer supports. In general supports should count as valid if they land on the model, on the base
plate, or on other supports as long as the entirety is adequately supported per physics."*

**The rule:** a support seat is valid on any of three targets —
1. the base plate,
2. the model's own surface (outside OR inside a cavity),
3. another support —
and the test of the whole assembly is the recorded arm physics ([[support-arm-load-physics]]:
supported_area / arm_section ≤ 12 by design, snaps at ≈ 25), not the seat's identity.

**What this corrects:** I had assumed a sealed cavity must not contain supports and nearly built a
change that censored cavity-interior islands entirely. Wrong. The cavity ceiling is a real down-facing
surface and it gets real supports; they stand on the cavity's lower interior wall and the load path to
the plate runs through the model itself and its exterior supports.

**How to apply:**
- The support router's validity question is "does the load path close, within physics?", never "does
  this support reach the plate?".
- An island inside a cavity is routable — its candidates seat on the cavity's lower interior.
- Interior supports are unremovable after printing; that is accepted. They stay inside the model.
  Their scars are invisible. Weight added is small compared to resin saved; if it matters it shows up
  in the savings arithmetic, not as a validity rule.
- See [[treant-hollowing-is-a-regression]] for the measured cavity-ceiling island this first surfaced
  on (Giant Toad, 135.58 mm² ceiling island on the drilled shell).
