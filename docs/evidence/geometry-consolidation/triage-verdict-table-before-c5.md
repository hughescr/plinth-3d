<!--
Captured by `cargo run --release -p plinth-mesh --example triage_verdict_table`
BEFORE plan item C5 (docs/specs/geometry-canon.md §1 OP-2, ray vs triangle).
This is the C5 hard prerequisite named in docs/plans/geometry-consolidation.md §4.
Do not edit by hand; re-run the example and replace the whole file.
-->

# Triage verdict table — BEFORE C5 (baseline)

Binary: `target/release/examples/triage_verdict_table`, built from
`feature/stage0-bytes-audit` with `crates/plinth-mesh/src/triage.rs` still
carrying its private `ray_triangle_distance()`.

**Read the probe section first.** Measured: every plain corpus row has
`classifiers = 0`. No corpus model contains a nested watertight component, so
triage never reaches the containment ray caster on the corpus as shipped, and
"no plain row moved across C5" is a true negative that is evidence of nothing.
The probe section forces the path onto the same models' real triangles.

# Triage verdict table

One row per corpus model. `verdict` is `triage_components(production limits, ExactBits)`; `digest` is SHA-256 over the triaged mesh's vertex and triangle bits (`-` when triage refused). `ray_tests`/`hits`/`misses`/`ambig` are measured counts from the containment ray caster, and `classifiers` is how many nested-component containment queries ran at all: a row with `classifiers=0` did not exercise the ray/triangle path, so its unchanged verdict is NOT evidence about C5.

| model | faces | verdict | components | main | stripped_comp | stripped_faces | digest | classifiers | rays | ray_tests | hits | misses | ambig | on_tri |
|---|--:|---|--:|--:|--:|--:|---|--:|--:|--:|--:|--:|--:|--:|
| Sphere | 4760 | Ok | 1 | 0 | 0 | 0 | f3865cb7b399329202b2b1bdfd8640402d2c629653cb373e3afbf585a155d489 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Cylinder | 244 | Ok | 1 | 0 | 0 | 0 | 270228cd89501768b2a5ba0bfb8f0ddea915cec95775c331fe4088ed2a245657 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_box | 12 | Ok | 1 | 0 | 0 | 0 | 0daf9f4a0eb471fd0dfa4c560ebee8e17546620eb1b113aef960043251339149 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_overhang_table | 60 | Ok | 5 | 4 | 0 | 0 | 433471c80763d7dc1a4b00cc7842ec2adb985f534f969609fbab90a672ecab25 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_toed_slab | 140 | Ok | 3 | 0 | 0 | 0 | b4b4cd13f8d9c398ebb1a6de35374dc4536449b7dbaef22abdc5deba9775ad2f | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_raking_blade | 24 | Ok | 2 | 0 | 0 | 0 | af0354bfc163c5dbaf54d974613625839bf91ed4d370f0a133fc87371a9f06a4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_thin_fin | 24 | Ok | 2 | 0 | 0 | 0 | 4aa7968f4bf7e9a495d9a8ae0b60dcaf4117adc5c435114bcda6712c51736503 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Weeping Treant v1.2 | 429120 | Ok | 1 | 0 | 0 | 0 | 389ed8646fac1dfbcd29f33ba43f5185f7765644c20b19d98c3aaecfabe4e733 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

rows: 8 (manifest names 8)

## Nested-shell probes on real corpus geometry

A 2 mm box nested at each model's bounding-box centre, so the ray/triangle path runs against that model's own triangles. `ray_tests` is the measured number of ray-vs-triangle evaluations; `hits`/`misses`/`ambig` are their outcomes.

| model | verdict | classifiers | rays | ray_tests | hits | misses | ambig | on_tri |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| Sphere | PreHollowed(1) | 1 | 24 | 15330 | 24 | 15306 | 0 | 0 |
| Cylinder | PreHollowed(1) | 1 | 24 | 1404 | 24 | 1380 | 0 | 0 |
| make_box | PreHollowed(1) | 1 | 24 | 144 | 24 | 120 | 0 | 0 |
| make_overhang_table | Ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_toed_slab | Ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_raking_blade | PreHollowed(1) | 1 | 24 | 144 | 24 | 120 | 0 | 0 |
| make_thin_fin | Ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Weeping Treant v1.2 | PreHollowed(1) | 1 | 24 | 1642722 | 24 | 1642698 | 0 | 0 |

probe rows: 8

probe ray/triangle face tests, all rows: 1659744
