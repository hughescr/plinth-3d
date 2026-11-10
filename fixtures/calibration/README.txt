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

Replacement plate (R-C4, 2026-11-02): fewer than six robust exposed-neck
ratios means A1 becomes a second A0 per band, four A0 total. C4 is deferred.
Archive the current project and cal1.goo in superseded-A1b/ before writing;
retain copies of plate-check.json and predictions.json there. Then run:
  cargo run -p plinth-calibration --bin cal1-replicates -- <plate-directory>
The command reads that archive, refuses to overwrite an existing project/GOO,
clones the same band's A0 into the former A1 XY slot, preserves pixel phase,
proves the other 14 whole serialized model records unchanged, and checks all
midpoint layers under both boundary conventions. It writes a new map, project,
predictions and README; render plate-map.svg with rsvg-convert --unlimited.
The two original A0 names stay unchanged; the added copies are front-A0-f2 and
back-A0-b2. The archived native slice is not validation of the new project.

Native slice measurement:
  cargo run -p plinth-calibration --bin cal1-slice -- <project> <goo> <new-dir>
This read-only decoder writes decoded.json and narrowest-sections.json. R-S324
uses the narrowest sampled waist per contact across 4.40 through 4.65 mm,
separately for lit-any and weighted area. R-C4 scans all isolated neck layers,
not a station window, and rejects a section still shrinking into a body merge.
Tests sabotage the windowed minimum, omit the late waist minimum, and supply
a descending last-isolated section before body merge. These are sampled checks,
not certified continuous sections or physical-strength evidence.

Frozen CAL-1 evidence (2026-11-02) is in cal1-frozen/: FROZEN.json identifies
both emitted inputs, predictions.json is the compact 16-row preregistration,
and measurements.json matches the review folder's slice-check.json byte-for-byte.
No owner project or GOO binary is stored here. replay-maxima.json is a small
snapshot of output from cal1-replay rebuilt with serde_json float_roundtrip;
its source project/output hashes are recorded. These files are the record of the
predictions made with the load model at CAL-1 release (1a7600c); CAL-1 is closed
and later load models do not reproduce them (rigid-carrier replaced the rigid-lever
solve; a lone contact is now "not assessed"). Tests compare all frozen rows
bit-for-bit with those output maxima (the record agrees with itself), pin the
sha256 of every file in cal1-frozen/, and replay the specimen types from cal1.json
on the current model for finite outputs only. A0's owner mesh is not a repository
fixture; its snapshot arithmetic was independently rerun on the hashed source
outside the repo. A consistent one-ULP drift across index, maxima and call still
fails the frozen-row test.
The native-raster differences were accepted under R-REP on 2026-11-02; the
plate was released for the print request.

Run tests and lint through scripts/t L1 calibration and scripts/t L0 calibration.
The unit tests cover both waist sizes on every written coupon path, the controls'
arm sections/cap area/heights, and an adversarial mesh-only waist change with
unchanged radius fields. The latter must fail the +/-10% section criterion.
The section union also has a disconnected-parts test: a hull must not fill a gap.

The separate checker must be run on each written project. Supply the matching
body-only STL with --stl for coupons and controls. Comparison copies use the
A-reference STL. W is a closed solid body, not an empty support-only model.
Retain raw checker verdicts: W fails H4 by the labelled calibration exception;
its checker H3/H8 remain Unverified. Native pixel-overlap measurements and the
midpoint witness are separate evidence, not a production checker pass.

K's 48-sided base is a diameter 8 mm pedestal ending at plate height 4.50 mm [S].
The 4 mm free arm and the flare/cap heights follow the design's explicit plate-6
reconstruction. Coupon/W native pads are diameter 2.2 x 0.2 mm 20-gons with
2.2 mm centre-to-centre strips along a deterministic minimum spanning tree.
All native support meshes are built directly in their model-local frame.

See docs/format/calibration-light-attrs.md for the measured 0xc01 observations,
field-by-field A0/A1 evidence and the remaining ChituBox representation question.
ChituBox opening, saving, slicing, waist pixel areas, physical loads, full-plate
layout, resin/time and replay predictions are NOT verified by this program.
