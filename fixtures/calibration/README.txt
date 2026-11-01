CAL-1: calibration piece, not a print candidate for any model.

cal1.json is the specimen specification for six individual feasibility pieces,
not a shuffled plate. It names body, XY translation (pose), lift, contact sites,
waist diameters, the fixed head dimensions, band and shuffle seed. Units are mm.
The program rejects changes to the preregistered bodies, sites, lifts and rungs
rather than writing a different experiment under the same specimen name.

Entry point (separate from the production plinth command):
  plinth-calibration <spec.json> <read-only-owner.chitubox> <new-empty-directory>

In a built slot the executable is target/debug/plinth-calibration. The source is
crates/calibration. The output directory must be empty. The source is only read.
The program extracts the owner's zero-based model #2, checks its numeric Light
signature, and writes A0/A1 in addition to the six spec pieces. Each output is an
individual project. It never assembles or approves a plate.

Measurements always parse bytes re-read from the files on disk. The main table
covers every coupon/W head path and both K arms. A separate A0-A1-tips.tsv and
A0-A1-field-diff.json cover the ten exposed Light necks. Sections are polygon
unions of plane/triangle intersections. Station resolution is reported; these
are sampled minima, not certified continuous lower bounds. The production
planner and load classifier are not used. These are not checker verdicts.

Run tests and lint through scripts/t L1 calibration and scripts/t L0 calibration.
The unit tests cover both waist sizes on every written coupon path, the controls'
arm sections/cap area/heights, and an adversarial mesh-only waist change with
unchanged radius fields. The latter must fail the +/-10% section criterion.
The section union also has a disconnected-parts test: a hull must not fill a gap.

The separate checker must be run on each written project. Supply the matching
body-only STL with --stl for the coupons and controls. Both A0 and A1 use the
A-reference STL. W has no dummy model mesh or reference STL. Retain the raw
checker verdicts: this version cannot prove the coupon head start's continuity,
handle W's empty model, or apply the K calibration exceptions. Do not change the
checker or relabel an Unverified rule as a pass.

K's 48-sided base is a diameter 8 mm pedestal ending at plate height 4.50 mm [S].
The 4 mm free arm and the flare/cap heights follow the design's explicit plate-6
reconstruction. Coupon/W native pads are diameter 2.2 x 0.2 mm 20-gons with
2.2 mm centre-to-centre strips along a deterministic minimum spanning tree.
All native support meshes are built directly in their model-local frame.

See docs/format/calibration-light-attrs.md for the measured 0xc01 observations,
field-by-field A0/A1 evidence and the remaining ChituBox representation question.
ChituBox opening, saving, slicing, waist pixel areas, physical loads, full-plate
layout, resin/time and replay predictions are NOT verified by this program.
