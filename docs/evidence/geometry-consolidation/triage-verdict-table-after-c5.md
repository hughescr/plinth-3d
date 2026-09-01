<!--
Captured by `cargo run --release -p plinth-mesh --example triage_verdict_table`
AFTER plan item C5 (docs/specs/geometry-canon.md §1 OP-2, ray vs triangle) and
the plinth-mesh C8a / OP-6 / OP-4 consolidations.
Do not edit by hand; re-run the example and replace the whole file.
-->

# Triage verdict table — AFTER C5

Binary: `target/release/examples/triage_verdict_table`, built with
`crates/plinth-mesh/src/triage.rs::ray_triangle_distance` reduced to triage
call-site policy over `geom_kernel::ray_triangle`.

**Result: byte-identical to
`triage-verdict-table-before-c5.md`. Every row, every digest, every count.**

That is a real negative, not an empty one, and the probe section is why. The
plain corpus rows all show `classifiers = 0` — no corpus model contains a
nested watertight component, so the corpus as shipped never reaches the ray
caster and its unchanged verdicts prove nothing. The probe rows nest a 2 mm box
in each model and drove **1,659,744 measured ray/triangle evaluations against
real corpus triangles** (1,642,722 of them on the Weeping Treant), and the
verdicts, hit counts and miss counts across all of them are unchanged.

What that does and does not establish:

* It establishes that on this corpus, at this geometry, the four acceptance
  rules C5 changed (barycentric slack, `t` slack, the `[1e-14, 1e-12)`
  determinant band, and the retired `EDGE_EPS` band) do not flip a single one
  of 1.66 M face verdicts. No model changes admission status.
* It does NOT establish that the changed bands are unreachable in general.
  They are narrow by construction, and the corpus's triangles are far from all
  of them. The rules themselves are pinned by the unit tests in
  `src/triage.rs` (`a_ray_within_the_barycentric_slack_of_an_edge_registers_a_crossing`
  and its three siblings), which is where a regression in them would show.

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
