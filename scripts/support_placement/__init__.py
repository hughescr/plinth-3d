"""Support placement v2 engine.

Physics-driven resin-print support generation: engineered tip/shaft sizing
from green-state cured-resin mechanics (not fixed palettes), scar-cost-aware
placement and orientation scoring, articulated/braced routing, clearance-
envelope self-verification, and native emission via CHITUBOX injection.

See ../../skills/support-placement/references/design.md for the design
charter this package implements (the "v0 -> v2 work list").

Package layout:
  config.py        physics constants + tunable defaults (SupportConfig)
  mesh_io.py        STL load/save, light mesh helpers
  perception.py     overhang/island detection, scar-cost (delicacy) field
  placement.py       candidate sampling, load partition, tip/shaft sizing
  routing.py          articulated routing, clearance envelope, brace graph,
                       model-to-model struts
  verify.py            self-verify (islands/clearance/load), orientation
                       scar-cost scorer
  chitubox_emit.py     from-scratch segment mesh synthesis + injection glue
  corpus.py            calibration analyzer over a real supported project
  engine.py            orchestrates the full pipeline
"""
