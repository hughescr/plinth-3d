# Constants Registry

| ID | Value | Unit | Shape | Provenance |
| --- | --- | --- | --- | --- |
| A&#95;ABS&#95;MM2 | 2.0 | mm² | scalar | placeholder(note="Placeholder; non-AcceptSafe. Owner-ratified scar-area gate with no literature measurement.") |
| BALL&#95;BREACH&#95;EPS&#95;MM | 0.05 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy ball-breach gate tolerance.") |
| BALL&#95;FACTOR&#95;FOOT | 1.4 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-foot joint-ball geometry factor.") |
| BALL&#95;FACTOR&#95;STUB | 1.5 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Tip stub ball-radius factor.") |
| BRACE&#95;LADDER&#95;Z&#95;MM | 8.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Brace ladder vertical spacing tuning.") |
| BRACE&#95;NEIGHBOR&#95;MAX&#95;XY&#95;MM | 15.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Brace-neighbor search radius.") |
| BRACE&#95;REACH&#95;MAX&#95;FACTOR&#95;X | 1.5 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Hard widened cross-link reach factor.") |
| BRACE&#95;R&#95;MM | 0.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Brace member radius tuning.") |
| BRACING&#95;FALLBACK&#95;FRACTIONS | &#91;0.5,0.25,0.75,0.1,0.9&#93; | 1 | list&#91;5&#93; | placeholder(note="Placeholder; non-AcceptSafe. Ordered deterministic fallback-fraction policy.") |
| BRACING&#95;MAX&#95;STACKED&#95;FALLBACKS | 6 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Result-changing stacked-fallback search cap.") |
| BRACING&#95;MIN&#95;CROSS&#95;LINK&#95;XY&#95;MM | 0.7071067811865476 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum cross-link XY distance, exactly one over square-root two.") |
| BRACING&#95;MODEL&#95;RUNG&#95;MAX&#95;CANDIDATES | 6 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-rung shortlist cap.") |
| BRACING&#95;MODEL&#95;RUNG&#95;MIN&#95;LENGTH&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-rung length floor.") |
| BRACING&#95;MODEL&#95;RUNG&#95;Z&#95;TOL&#95;MM | 0.75 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-rung candidate Z band.") |
| BRACING&#95;REACH&#95;TIER&#95;BASE&#95;FACTOR | 1.25 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Base factor for deterministic brace reach tiers.") |
| BRANCH&#95;ANGLE&#95;DEG | 60.0 | ° | scalar | placeholder(note="Placeholder; non-AcceptSafe. Widened branch cone is valid only with the angle-aware load term.") |
| BRIDGE&#95;FREE&#95;GAP&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Local unsupported-gap perception default, distinct from sag span.") |
| BRIDGE&#95;SAG&#95;SPAN&#95;APPROX&#95;MM | {"section&#95;mm":0.3,"span&#95;mm":3.0} | mm | record{section&#95;mm,span&#95;mm} | placeholder(note="Placeholder; non-AcceptSafe. Approximate sag span inseparably conditioned on a 0.300 mm section; distinct from free gap.") |
| CCO&#95;BIFURCATION&#95;HOSTS | 5 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Bifurcation host shortlist.") |
| CCO&#95;CONE&#95;TOLERANCE&#95;DEG | 0.000001 | ° | scalar | placeholder(note="Placeholder; non-AcceptSafe. Cone comparison tolerance, not a physics floor.") |
| CCO&#95;FERMAT&#95;MAX&#95;ITERATIONS | 12 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Bounded Weiszfeld iteration cap.") |
| CCO&#95;MIN&#95;SPLIT&#95;LENGTH&#95;MM | 0.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Bifurcation split-piece length floor.") |
| CCO&#95;SPLIT&#95;SLIDE&#95;POLICY | {"cap":40,"end":0.97,"start":0.03,"step":0.03} | 1 | record{cap,end,start,step} | placeholder(note="Placeholder; non-AcceptSafe. Structured deterministic split-slide grid.") |
| CCO&#95;XIA&#95;ALPHA | 0.5 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy topology-search heuristic whose conditional equations do not derive alpha.") |
| CCO&#95;X&#95;PAIR&#95;LOAD&#95;DISCOUNT | 0.8408964152537145 | 1 | scalar | derived(from=&#91;&#93;,note="Proof-only fourth-root relation: (F/2)^0.25/F^0.25 = 0.5^0.25. The proof is in docs/reference/physics&#95;transient&#95;peel.md and the relation is implemented in both cco.py and bracing.py.") |
| CHITUBOX&#95;DEFAULT&#95;PLACEMENT&#95;XY&#95;MM | {"x":0.0,"y":0.0} | mm | record{x,y} | placeholder(note="Placeholder; non-AcceptSafe. Project placement default with signed-zero-preserving coordinates.") |
| CHITUBOX&#95;EMIT&#95;N&#95;SIDES | 24 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. ChituBox emission geometry contract.") |
| CHITUBOX&#95;PAD&#95;DIAMETER&#95;MM | 2.2 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Generated support-pad diameter.") |
| CHITUBOX&#95;PAD&#95;N&#95;SIDES | 20 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Generated support-pad tessellation count.") |
| CHITUBOX&#95;PLATE&#95;MESH&#95;TOL&#95;MM | 0.00001 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Plate mesh geometry tolerance, separate from binary offsets.") |
| CHITUBOX&#95;RAFT&#95;HEIGHT&#95;MM | 0.2 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Generated raft height.") |
| CHITUBOX&#95;ROOT&#95;HEIGHT&#95;MM | 0.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Root no-bend height.") |
| CHITUBOX&#95;ROOT&#95;LAYERS | 10 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum vertical root layers.") |
| CHITUBOX&#95;SPHERE&#95;MESH&#95;TESSELLATION&#95;POLICY | {"n&#95;lat":8,"n&#95;lon":16,"triangles":224} | count | record{n&#95;lat,n&#95;lon,triangles} | placeholder(note="Placeholder; non-AcceptSafe. Default UV-sphere geometry policy for emitted tip, contact, and joint spheres.") |
| CLEARANCE&#95;SAMPLE&#95;STEP&#95;MM | 0.3 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy sampled-path diagnostic only; production clearance is exact.") |
| COINCIDE&#95;TOL&#95;MM | 0.15 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Joint coincidence tolerance.") |
| COLINEAR&#95;ANGLE&#95;DEG | 10.0 | ° | scalar | placeholder(note="Placeholder; non-AcceptSafe. Joint colinearity angle tolerance.") |
| COLINEAR&#95;RADIUS&#95;FRAC | 0.2 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Joint colinearity radius fraction.") |
| CONNECTIVITY&#95;AXIS&#95;ATTACH&#95;TOL&#95;MM | 0.25 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Connectivity axis-attachment gate tolerance.") |
| CONNECTIVITY&#95;EMBED&#95;PAD&#95;MM | 0.45 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Connectivity embed pad added to contact depth.") |
| COVERAGE&#95;RADIUS&#95;FACTOR | 1.5 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Coverage gate radius factor selected without measurement.") |
| COVERAGE&#95;REPAIR&#95;MAX&#95;ATTEMPTS | 8 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Result-changing coverage-repair cap, not a resource exclusion.") |
| DECIMATION&#95;ADAPTATION&#95;POLICY | {"adaptations":3,"error&#95;multiplier":0.6,"feature&#95;multiplier":2.0} | 1 | record{adaptations,error&#95;multiplier,feature&#95;multiplier} | placeholder(note="Placeholder; non-AcceptSafe. Structured feature-protection adaptation policy.") |
| DECIMATION&#95;FEATURE&#95;HALO&#95;MM | 0.4 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Feature-protection halo.") |
| DECIMATION&#95;FEATURE&#95;PROBE&#95;RATIO | 0.2 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Aggressive feature-probe ratio.") |
| DECIMATION&#95;P999&#95;FACTOR | 0.001 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Selected pixel-pitch divisor fraction; arithmetic does not establish authority.") |
| DECIMATION&#95;P999&#95;SAMPLE&#95;POLICY | {"cap":150000,"seed":1} | count | record{cap,seed} | placeholder(note="Placeholder; non-AcceptSafe. Forward percentile sampling seed and cap.") |
| DECIMATION&#95;P99&#95;FACTOR | 0.0005 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Selected pixel-pitch divisor fraction; arithmetic does not establish authority.") |
| DECIMATION&#95;P99&#95;SAMPLE&#95;POLICY | {"cap":120000,"seed":42} | count | record{cap,seed} | placeholder(note="Placeholder; non-AcceptSafe. Backward percentile sampling seed and cap.") |
| DECIMATION&#95;PROTECTED&#95;CAP&#95;FRACTION | 0.12 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Protected-vertex fraction cap.") |
| DECIMATION&#95;RATIO&#95;SEARCH | {"bisections":5,"lower":0.05,"upper":1.0} | 1 | record{bisections,lower,upper} | placeholder(note="Placeholder; non-AcceptSafe. Structured bounded ratio-search policy.") |
| DECIMATION&#95;RECHECK&#95;SLACK | 1.1 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Selected full-population recheck slack, not measured authority.") |
| DECIMATION&#95;SCALE&#95;ACCEPTANCE&#95;MM | interval(lower=10.0,lower&#95;bound=inclusive,upper=300.0,upper&#95;bound=inclusive) | mm | interval | placeholder(note="Placeholder; non-AcceptSafe. Inclusive scale-sanity acceptance interval.") |
| DECIMATION&#95;SEED&#95;ERROR&#95;MM | 0.05 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Feature seed error threshold.") |
| DECIMATION&#95;VERTEX&#95;GROUP&#95;FACTOR | 10.0 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Protected vertex-group sizing factor.") |
| DEFAULT&#95;CONTACT&#95;DEPTH&#95;MM | 0.3 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Generation/profile default, not ChituBox format authority.") |
| DELICACY&#95;CURVATURE&#95;WEIGHT | 0.6 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Curvature proxy blend weight.") |
| DELICACY&#95;THINNESS&#95;WEIGHT | 0.4 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Thinness proxy blend weight.") |
| EMIT&#95;REQUIRE&#95;PASSED&#95;GATES&#95;DEFAULT | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Emission authorization default; blocking defects always refuse emission.") |
| ENCLOSURE&#95;PROOF&#95;TOL&#95;MM | 0.018 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Arithmetic from enclosure tolerance does not improve Placeholder authority.") |
| ENCLOSURE&#95;TOL&#95;MM | 0.02 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Enclosure tolerance whose old pixel derivation is rejected.") |
| ENGINE&#95;SURFACE&#95;TREE&#95;MAX&#95;POINTS | 400000 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Live engine sampled-surface override.") |
| ENGINE&#95;SURFACE&#95;TREE&#95;PITCH&#95;MM | 0.3 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Live engine sampled-surface pitch override.") |
| EPS&#95;COS | 0.000000001 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Selected divide-by-zero guard, explicitly not a physics floor.") |
| EXPECTED&#95;CHITUBOX&#95;API&#95;VERSION | 1 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Native writer compatibility version checked before emission.") |
| FLARE&#95;LEN&#95;MM | 1.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Conical flare length, not the missing fast-flare term.") |
| FLARE&#95;R&#95;FACTOR | 1.6 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Conical flare radius factor.") |
| GATE&#95;ON&#95;BALL&#95;BREACH&#95;DEFAULT | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Independent ball-breach verification default.") |
| GATE&#95;ON&#95;REMOVAL&#95;DEFAULT | false | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Removal metric remains opt-in by default.") |
| GRIP&#95;CLEAR&#95;MM | 5.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Uncalibrated opt-in removal clearance.") |
| GRIP&#95;HANDLE&#95;LEN&#95;MM | 10.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Uncalibrated opt-in graspable length.") |
| GRIP&#95;HANDLE&#95;R&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Uncalibrated opt-in graspable radius.") |
| HOLLOW&#95;ALLOW&#95;OUTPUT&#95;DEFAULT | false | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Hollow output remains disabled without a dedicated verifier.") |
| HOLLOW&#95;BREATHING&#95;VOXEL&#95;PITCH&#95;MM | 0.4 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Final cavity-breathing verification voxel pitch.") |
| HOLLOW&#95;BULK&#95;MIN&#95;MM | 10.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Hollowing bulk-size policy.") |
| HOLLOW&#95;CONTAINMENT&#95;MIN&#95;FRACTION | 0.9 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Internal-component containment threshold.") |
| HOLLOW&#95;CONTAINMENT&#95;SAMPLES | 32 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Internal-component containment sample count.") |
| HOLLOW&#95;DRAIN&#95;DIRECTIONS | &#91;{"x":0.0,"y":0.0,"z":-1.0},{"x":0.5,"y":0.0,"z":-1.0},{"x":-0.5,"y":0.0,"z":-1.0},{"x":0.0,"y":0.5,"z":-1.0},{"x":0.0,"y":-0.5,"z":-1.0}&#93; | 1 | list&#91;5&#93;&lt;record{x,y,z}&gt; | placeholder(note="Placeholder; non-AcceptSafe. One ordered structured set of five downward drain-search directions.") |
| HOLLOW&#95;DRAIN&#95;R&#95;MM | 1.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Drain-hole radius policy.") |
| HOLLOW&#95;EXIT&#95;RAY&#95;MAX&#95;MM | 45.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Drain and vent exit-search ceiling.") |
| HOLLOW&#95;EXTRA&#95;DRAIN&#95;POLICY | {"cylinder&#95;sides":24,"exit&#95;max&#95;mm":8.0,"extensions&#95;mm":&#91;4.0,8.0,9.0&#93;,"max&#95;extra&#95;drains":3,"min&#95;pocket&#95;ml":0.5} | mixed | record{cylinder&#95;sides,exit&#95;max&#95;mm,extensions&#95;mm,max&#95;extra&#95;drains,min&#95;pocket&#95;ml} | placeholder(note="Placeholder; non-AcceptSafe. Structured trapped-pocket repair policy; all conditions travel together.") |
| HOLLOW&#95;LOBE&#95;BAND&#95;MM | 15.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Hollow lobe spatial band.") |
| HOLLOW&#95;LOBE&#95;MIN&#95;ML | 0.5 | mL | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum hollow lobe volume.") |
| HOLLOW&#95;ORIENT&#95;ENABLE&#95;DEFAULT | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Independent hollow-orientation enable default.") |
| HOLLOW&#95;PITCH&#95;MM | 0.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Hollowing pitch policy.") |
| HOLLOW&#95;SAVE&#95;MIN&#95;FRAC | 0.25 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum fractional resin saving.") |
| HOLLOW&#95;SAVE&#95;MIN&#95;ML | 15.0 | mL | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum absolute resin saving.") |
| HOLLOW&#95;SHARD&#95;MAX&#95;DIM&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Component-triage dimension ceiling.") |
| HOLLOW&#95;SHARD&#95;MAX&#95;FACES | 64 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Component-triage face ceiling.") |
| HOLLOW&#95;SHARD&#95;MAX&#95;VOLUME&#95;MM3 | 0.01 | mm³ | scalar | placeholder(note="Placeholder; non-AcceptSafe. Component-triage volume ceiling.") |
| HOLLOW&#95;VENT&#95;R&#95;MM | 1.25 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Vent-hole radius policy.") |
| HOLLOW&#95;VOLUME&#95;BALANCE&#95;REL&#95;TOL | 0.02 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Final volume-balance tolerance relative to solid volume.") |
| HOLLOW&#95;WALL&#95;DEFAULT&#95;MM | 2.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Hollow generation and tool wall default.") |
| ISLAND&#95;ATTACH&#95;REACH&#95;MM | 0.2 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Tuning tied to the legacy Placeholder tip default.") |
| ISLAND&#95;BIRTH&#95;LINK&#95;R&#95;MM | 0.6 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Tuned link radius invalidated by dynamic layer height.") |
| ISLAND&#95;MERGE&#95;RADIUS&#95;MM | 2.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Geometric closing radius, not physics.") |
| JOINT&#95;BALL&#95;FACTOR | 1.0 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Joint-ball geometry factor.") |
| KAPPA | 1.25 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Owner-ratified crossing-disc headroom multiplier.") |
| LAYER&#95;HEIGHT&#95;DEFAULT&#95;MM | 0.05 | mm | scalar | measured(source=project&#95;record(id="saturn4-ultra-layer-profile-plate6",record="docs/reference/plate-06-results.md",locator="§1 Layer height",date=exact("2026-07-30")),err=null,note="Plate 6 project profile layer height; this is not a vendor default.") |
| LIFT&#95;MM | 5.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Canonical local printer-profile default; retired 3.0 pipeline overrides are not aliases.") |
| LIFT&#95;TOL&#95;MM | 0.000001 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Output frame-invariant tolerance for centered Z minus plate Z.") |
| LOAD&#95;SHARING | additive(n&#95;eff&#95;cap=6.0) | 1 | load&#95;sharing | measured(source=calibration&#95;plate(plate="COUPON-PLATE-6-support-arm-capacity",record="docs/reference/plate-06-results.md",locator="§5 Finding 1",date=exact("2026-07-31")),err=null,note="Plate 6 validated one cap area (30 mm²), one arm diameter (0.798 mm), and one arm length (4.00 mm). No strip failed, so there is no measured upper bracket; N &gt; 6 is extrapolation. The fin root contributes about 19.75% section bonus, so the result is at least N× and never evidence for super-additive sharing. X&#95;tot is algebraically degenerate with total arm area, and the ratio rests on plate 5. The tight N=2 flip, X&#95;tot 30.077 alive versus 30.481 dead, is a 1.33% window and does not prove exact additivity. Plate 6 §5 Finding 3 records survival on both axes at X&#95;arm=60.15, excluding zipper collapse at that load while leaving zero-variance and near-boundary axis effects untested; the four X-axis cells were not zipper tests. docs/reference/plate-06-results.md §6 records the B6 override: re-scoping a preregistered void after seeing data is exactly what preregistration blocks, making these points post-hoc and leaving that counterargument unanswered.") |
| L&#95;SCAR&#95;MAX&#95;MM | 2.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Owner-ratified maximum scar-patch dimension.") |
| MATERIAL&#95;COST&#95;PER&#95;MM | 0.05 | per-mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Strut-selection material cost weight.") |
| MAX&#95;DEFLECTION&#95;MM | 0.15 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Subjective visual-ribbing serviceability threshold.") |
| MAX&#95;SEGMENT&#95;ANGLE&#95;DEG | 45.0 | ° | scalar | placeholder(note="Placeholder; non-AcceptSafe. Qualitative printability policy, not measured authority.") |
| MAX&#95;TRUNK&#95;R&#95;MM | 2.4 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Live trunk-radius ceiling.") |
| MAX&#95;UNBRACED&#95;SHAFT&#95;R&#95;MM | 0.9 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Corpus rationale does not derive the unbraced-shaft ceiling.") |
| MINIMA&#95;BIRTH&#95;MATCH&#95;LAYERS | 2.0 | layers | scalar | placeholder(note="Placeholder; non-AcceptSafe. Corpus rule with selected headroom, invalidated by per-layer heights.") |
| MINIMA&#95;CONNECT&#95;LAYERS | 2.0 | layers | scalar | placeholder(note="Placeholder; non-AcceptSafe. Corpus rule with selected headroom, invalidated by per-layer heights.") |
| MINIMA&#95;CONTACT&#95;RADIUS&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minima contact discretization slack, not coverage radius.") |
| MINIMA&#95;DROP&#95;INTERIOR&#95;FACES | true | 1 | scalar | measured(source=project&#95;record(id="minima-contact-fix-2026-07-24",record="docs/reference/acceptance-run-2026-07-24-contactfix.md",locator="Provenance and What moved",date=exact("2026-07-24")),err=null,note="Measured contact-fix disposition; see §3.7 for the old-rule false-contact rates and per-layer propagation context.") |
| MINIMA&#95;EPS&#95;MM | 0.02 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minima-stage sub-layer tie tolerance.") |
| MINIMA&#95;REQUIRE&#95;ISLAND&#95;BIRTH | true | 1 | scalar | measured(source=project&#95;record(id="minima-contact-fix-2026-07-24",record="docs/reference/acceptance-run-2026-07-24-contactfix.md",locator="Provenance and What moved",date=exact("2026-07-24")),err=null,note="Measured contact-fix disposition; see §3.7 for the old-rule false-contact rates and per-layer propagation context.") |
| MINIMA&#95;SUPPORT&#95;AREA&#95;TIPS | 1.0 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Selected tuning in a flat sensitivity region, not a measured boundary.") |
| MIN&#95;CONTACTS&#95;PER&#95;ISLAND | 1 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum placement contacts per island.") |
| MIN&#95;ILLEGAL&#95;LEN&#95;MM | 0.05 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Post-anchoring crossing sliver cutoff.") |
| MIN&#95;SHAFT&#95;R&#95;MM | 0.3 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum shaft radius.") |
| MIN&#95;SHOOT&#95;R&#95;MM | 0.35 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum routed shoot/member radius, distinct from tip diameter.") |
| MIN&#95;TRUNK&#95;R&#95;MM | 0.8 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum trunk radius.") |
| MODEL&#95;BRACE&#95;MAX&#95;LEN&#95;MM | 5.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Maximum model-brace length tuning.") |
| MODULUS&#95;GREEN&#95;GPA | 0.5 | GPa | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy diagnostic modulus with no plate-5 measurement.") |
| MURRAY&#95;TOLERANCE | 0.05 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Murray gate tolerance.") |
| NEAR&#95;PLATE&#95;MM | 1.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Near-plate perception band.") |
| N&#95;EFF&#95;CAP | 6.0 | count | scalar | derived(from=&#91;"LOAD&#95;SHARING"&#93;,note="Reads the measured Additive payload's largest tested group size, 6.0; it is not a separate measurement.") |
| ON&#95;AXIS&#95;TOL&#95;MM | 0.2 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Joint on-axis tolerance.") |
| ORIENT&#95;SCAR&#95;WEIGHT | 0.4 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Opt-in additive scar weight; zero reproduces the pre-scar objective.") |
| OVERHANG&#95;ANGLE&#95;DEG | 45.0 | ° | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy tool convention lacking the required geometric-extent criterion.") |
| P8&#95;BALL&#95;SLIDE&#95;MAX&#95;MM | 2.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Phase-8 ball-slide search reach.") |
| P8&#95;BALL&#95;SLIDE&#95;STEP&#95;MM | 0.25 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Phase-8 ball-slide search step.") |
| P8&#95;CHORD&#95;CANON&#95;MICRON | 1 | µm | scalar | measured(source=corpus&#95;study(id="phase8-micron-chord-canonicalization-2026-07-24",record="docs/specs/SPEC-P8-RELOCATE.md",locator="§5.3 Layer 3/amendment G",date=exact("2026-07-24")),err=null,note="f64 chords canonicalize to integer microns at one-micron comparison resolution.") |
| P8&#95;JUNCTION&#95;RESEAT&#95;ENABLE | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Independent junction reseat enable.") |
| P8&#95;JUNCTION&#95;RESEAT&#95;MAX&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Junction reseat reach.") |
| P8&#95;JUNCTION&#95;RESEAT&#95;STEP&#95;MM | 0.25 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Junction reseat grid step.") |
| P8&#95;MAX&#95;RELOCATE&#95;ROUNDS | 4 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Relocation backstop whose live fallbacks agree.") |
| P8&#95;MAX&#95;RELOCATE&#95;SWEEPS | 8 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Relocation sweep backstop whose live fallbacks agree.") |
| P8&#95;MAX&#95;UNRESOLVED | 10 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Honest-failure unresolved-site bound.") |
| P8&#95;MAX&#95;UNRESOLVED&#95;WEIGHT | 3.0 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Honest-failure unresolved-weight bound.") |
| P8&#95;POCKET&#95;CORRIDOR&#95;PAD&#95;MM | 0.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Pocket detection halo and density-penalty radius.") |
| P8&#95;POCKET&#95;MIN&#95;MEMBERS | 8 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum defect members in a pocket component.") |
| P8&#95;POCKET&#95;VOXEL&#95;MM | 6.0 | mm | scalar | measured(source=owner&#95;decision(id="p8-pocket-voxel-owner-decision",record="docs/specs/SPEC-P8-RELOCATE.md",locator="§4.1",date=exact("2026-07-25")),err=null,note="Owner-approved Phase-8 pocket voxel size.") |
| P8&#95;REAIM&#95;AZIMUTH&#95;DEG | &#91;0,45,90,135,180,225,270,315&#93; | ° | list&#91;8&#93; | placeholder(note="Placeholder; non-AcceptSafe. Pinned deterministic azimuth enumeration order.") |
| P8&#95;REAIM&#95;LENGTH&#95;FACTORS | &#91;1.0,0.75,0.5&#93; | 1 | list&#91;3&#93; | placeholder(note="Placeholder; non-AcceptSafe. Pinned shortening-length ladder.") |
| P8&#95;REAIM&#95;POLAR&#95;DEG | &#91;0,10,20,30&#93; | ° | list&#91;4&#93; | placeholder(note="Placeholder; non-AcceptSafe. Pinned polar cone grid.") |
| P8&#95;REAIM&#95;ROUTE&#95;GROWTH&#95;MAX&#95;FACTOR | 1.5 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Phase-8 route-growth honest-failure bound.") |
| P8&#95;REAIM&#95;VOLUME&#95;DELTA&#95;MAX&#95;ML | 0.02 | mL | scalar | placeholder(note="Placeholder; non-AcceptSafe. Phase-8 volume-delta honest-failure bound.") |
| P8&#95;RELOCATE&#95;ENABLE | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Canonical migration target true; conflicting false compatibility fallback is not ported.") |
| P8&#95;RESEAT&#95;ENABLE | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Surface reseat enable with consistent live true fallbacks.") |
| P8&#95;RESEAT&#95;MAX&#95;SEEDS | 24 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Surface reseat candidate cap.") |
| P8&#95;RESEAT&#95;RADIUS&#95;MM | 2.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Surface reseat search radius.") |
| P8&#95;RESIN&#95;DELTA&#95;MAX | 0.03 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Phase-8 resin-delta honest-failure bound.") |
| P8&#95;RESIZE&#95;FIXED&#95;POINT&#95;MAX&#95;ROUNDS | 4 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Resize fixed-point backstop whose live fallback agrees.") |
| P8&#95;SITE&#95;COMMIT&#95;BUDGET | 2 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Per-site Phase-8 commit budget.") |
| P8&#95;STRUT&#95;GROWTH&#95;MAX&#95;FACTOR | 1.25 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Phase-8 strut-growth honest-failure bound.") |
| PEEL&#95;ELEVATIONS&#95;DEG | &#91;0,25,45,65,90&#93; | ° | list&#91;5&#93; | placeholder(note="Placeholder; non-AcceptSafe. Uncalibrated ordered elevations for the opt-in removal metric.") |
| PEEL&#95;PRESSURE&#95;PA | 3000.0 | Pa | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy diagnostic split, potentially about four times low.") |
| PERF&#95;ANCHOR&#95;CAP | {"state":"disabled"} | count | trial&#95;setting&lt;anchor&#95;candidate&#95;cap&gt; | placeholder(note="Placeholder; non-AcceptSafe. Experimental B2 remains Disabled; no enabled candidate is selected.") |
| PERF&#95;BEAM&#95;K | {"state":"disabled"} | count | trial&#95;setting&lt;beam&#95;width&gt; | placeholder(note="Placeholder; non-AcceptSafe. Experimental B1 remains Disabled; no enabled candidate is selected.") |
| PERF&#95;FIX&#95;ITER&#95;CAPS | {"state":"disabled"} | count | trial&#95;setting&lt;brace&#95;iteration&#95;cap&gt; | placeholder(note="Placeholder; non-AcceptSafe. Experimental B4 models only the consumed brace-iteration cap and remains Disabled.") |
| PERF&#95;SEED&#95;SPACING&#95;FACTOR | {"state":"disabled"} | 1 | trial&#95;setting&lt;seed&#95;spacing&#95;factor&gt; | placeholder(note="Placeholder; non-AcceptSafe. Experimental B3 remains Disabled and cannot loosen the coverage gate.") |
| PIPELINE&#95;HOLLOW&#95;ENABLE&#95;DEFAULT | true | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Pipeline hollow-stage interface default.") |
| PIPELINE&#95;P8&#95;RELOCATE&#95;DEFAULT | false | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. CLI pipeline default distinct from the canonical Phase-8 config enable.") |
| PIXEL&#95;PITCH&#95;X&#95;UM | 14.0 | µm | scalar | measured(source=vendor&#95;document(id="elegoo-saturn4-ultra-16k-press-2025-01-22",record="docs/datasheets/printers/saturn-4-ultra-16k.md",locator="Physical parameters plus project axis mapping",date=exact("2025-01-22")),err=null,note="The vendor's first pitch maps to project X; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.") |
| PIXEL&#95;PITCH&#95;Y&#95;UM | 19.0 | µm | scalar | measured(source=vendor&#95;document(id="elegoo-saturn4-ultra-16k-press-2025-01-22",record="docs/datasheets/printers/saturn-4-ultra-16k.md",locator="Physical parameters plus project axis mapping",date=exact("2025-01-22")),err=null,note="The vendor's second pitch maps to project Y; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.") |
| PLATE&#95;BAND&#95;MM | 0.2 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. One owner for joint and connectivity plate-band tolerance.") |
| PRINT&#95;FLOOR&#95;CHORD&#95;MM | 0.08 | mm | scalar | measured(source=owner&#95;decision(id="print-floor-owner-decision-2026-07-24",record="docs/specs/SPEC-EMBED-POLICY.md",locator="PRINT-FLOOR amendment and Normative constants",date=exact("2026-07-24")),err=null,note="Owner-ratified print-floor chord below which a rule-4 unanchored transit is moot.") |
| PULL&#95;DIST&#95;MM | 15.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Uncalibrated opt-in removal translation distance.") |
| REWIRE&#95;DEGENERATE&#95;LEN&#95;MM | 0.0001 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Result-changing degenerate-span threshold.") |
| REWIRE&#95;MAX&#95;SLIDE&#95;EVALS | 50 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Deterministic slide-evaluation cap.") |
| REWIRE&#95;ROLL&#95;EPS | 0.0025 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Rolling-savings convergence fraction.") |
| REWIRE&#95;ROLL&#95;WINDOW | 20 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Rolling-savings convergence window.") |
| REWIRE&#95;TWIG&#95;MERGE&#95;LEN&#95;MM | 0.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Result-changing twig-merge geometry threshold.") |
| REWIRE&#95;WORK&#95;BUDGET | {"state":"disabled"} | count | trial&#95;setting&lt;nonzero&#95;u64&gt; | placeholder(note="Placeholder; non-AcceptSafe. B5 exact-search baseline is Disabled with no wall-clock fallback and no selected enabled budget.") |
| SATURN&#95;BUILD&#95;VOLUME&#95;MM | {"x":211.68,"y":118.37,"z":220.0} | mm | record{x,y,z} | measured(source=vendor&#95;document(id="elegoo-saturn4-ultra-16k-press-2025-01-22",record="docs/datasheets/printers/saturn-4-ultra-16k.md",locator="Physical parameters plus project axis mapping",date=exact("2025-01-22")),err=null,note="The vendor's first, second, and third dimensions map to project X, Y, and Z; retrieval date 2026-07-31 is metadata, while 2025-01-22 is the evidence date.") |
| SATURN&#95;LEADING&#95;RELEASE&#95;EDGE | "negative&#95;y" | enum | y&#95;direction | measured(source=project&#95;record(id="saturn4-ultra-release-front-plate6",record="docs/reference/plate-06-results.md",locator="§1 Printer",date=exact("2026-07-31")),err=null,note="Project leading release edge is NegativeY.") |
| SATURN&#95;RELEASE&#95;FILM&#95;THICKNESS&#95;MM | 0.127 | mm | scalar | measured(source=installed&#95;hardware(id="saturn4-ultra-pfa-install-2026-07-25",record="docs/rust-migration/03-physics-and-constants.md",locator="§3.7 release film",date=exact("2026-07-25")),err=null,note="Nominal thickness identifies the installed official Elegoo PFA and is not peel evidence.") |
| SATURN&#95;RELEASE&#95;FRONT&#95;SWEEP&#95;DIRECTION | "positive&#95;y" | enum | y&#95;direction | measured(source=project&#95;record(id="saturn4-ultra-release-front-plate6",record="docs/reference/plate-06-results.md",locator="§1 Printer",date=exact("2026-07-31")),err=null,note="Project release-front sweep direction is PositiveY: the front moves -Y → +Y.") |
| SCAR&#95;COST&#95;WEIGHT | 1.0 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Strut-selection scar cost weight.") |
| SPACING&#95;FLOOR&#95;MM | 2.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Removal-ergonomics spacing floor selected without measurement.") |
| STANDOFF&#95;MM | 0.6 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy clearance policy whose corpus use does not derive the distance.") |
| STRUT&#95;FALLBACK&#95;MAX&#95;ANGLE&#95;DEG | 60.0 | ° | scalar | placeholder(note="Placeholder; non-AcceptSafe. Fallback strut routing angle.") |
| STRUT&#95;MAX&#95;LENGTH&#95;MM | 20.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Corpus outcomes do not establish a safe strut maximum.") |
| STRUT&#95;MIN&#95;LENGTH&#95;SAVED&#95;MM | 8.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Minimum material saving that justifies a second scar.") |
| STRUT&#95;MODEL&#95;ANCHOR&#95;MAX&#95;CANDIDATES | 40 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-anchor candidate-selection cap.") |
| STRUT&#95;MODEL&#95;ANCHOR&#95;MAX&#95;DELICACY | 0.5 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-anchor delicacy ceiling.") |
| STRUT&#95;MODEL&#95;ANCHOR&#95;MIN&#95;LEN&#95;MM | 5.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Model-anchor candidate length floor.") |
| STUB&#95;LENGTH&#95;MM | 2.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Support stub length.") |
| SURFACE&#95;TREE&#95;MAX&#95;POINTS | 150000 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Result-changing sampled-surface cap, not a resource exclusion.") |
| SURFACE&#95;TREE&#95;PITCH&#95;MM | 0.4 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Result-changing sampled-surface pitch.") |
| SURFACE&#95;TREE&#95;SEED | 0 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Deterministic sampled-surface seed; zero is a real seed, not a sentinel.") |
| TARGET&#95;SPACING&#95;MM | 4.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Observed sufficiency does not derive an optimum spacing boundary.") |
| TENSILE&#95;GREEN&#95;MPA | 0.288 | MPa | scalar | placeholder(note="Placeholder; non-AcceptSafe. Algebraic legacy diagnostic split, not a measured material property.") |
| THIN&#95;FEATURE&#95;MM | 1.5 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Delicacy thinness saturation threshold.") |
| TIP&#95;DIAMETER&#95;MIN&#95;ALLOWED&#95;MM | 0.15 | mm | scalar | measured(source=owner&#95;decision(id="tip-diameter-floor-owner-decision",record="docs/rust-migration/03-physics-and-constants.md",locator="§3.2",date=on&#95;or&#95;before("2026-07-31")),err=null,note="Owner-authorized hard validator floor for this machine and resin; it is not a target or a cap.") |
| TIP&#95;RADIUS&#95;DEFAULT&#95;MM | 0.2 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Generation starting radius, distinct from the measured validator minimum.") |
| TIP&#95;RADIUS&#95;MAX&#95;LEGACY&#95;MM | 1.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Live output-affecting placement clamp retained as legacy policy.") |
| TIP&#95;RADIUS&#95;MIN&#95;ALLOWED&#95;MM | 0.075 | mm | scalar | derived(from=&#91;"TIP&#95;DIAMETER&#95;MIN&#95;ALLOWED&#95;MM"&#93;,note="Exact diameter-to-radius conversion: TIP&#95;DIAMETER&#95;MIN&#95;ALLOWED&#95;MM / 2 = 0.075 mm; do not substitute the 0.200 mm default.") |
| TIP&#95;SAFETY&#95;FACTOR | 8.0 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy diagnostic load multiplier selected as policy.") |
| TOL&#95;MM | 0.05 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Interval anchoring and detection tolerance, distinct from the sliver cutoff.") |
| TRANSIENT&#95;SHOOT&#95;COUNT&#95;EXPONENT | 0.125 | 1 | scalar | placeholder(note="Placeholder; non-AcceptSafe. Inherited N-to-the-one-eighth heuristic without physical authority.") |
| TRUNK&#95;SEARCH&#95;DIRECTIONS | 12 | count | scalar | placeholder(note="Placeholder; non-AcceptSafe. Trunk search direction count.") |
| TRUNK&#95;SEARCH&#95;RADII&#95;MM | &#91;0.0,2.0,4.0,7.0,10.0,14.0,19.0,25.0&#93; | mm | list&#91;8&#93; | placeholder(note="Placeholder; non-AcceptSafe. Ordered trunk-search radius ladder including the real zero-radius candidate.") |
| TWIN&#95;SINGLE&#95;BRACE&#95;XY&#95;MM | 3.0 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Twin-to-single brace XY threshold.") |
| VOXEL&#95;PITCH&#95;MM | 0.4 | mm | scalar | placeholder(note="Placeholder; non-AcceptSafe. Legacy perception working scale inferred from one corpus run.") |
| X&#95;ALLOWED | 12.0 | 1 | scalar | derived(from=&#91;"X&#95;STAR&#95;CONE"&#93;,note="Binding commitment: floor(lower(X&#95;STAR&#95;CONE)/2) = floor(25.00/2) = 12. X&#95;STAR&#95;CYLINDER supplies only the margin check 19.91/12 = 1.66× and is not a derivation input.") |
| X&#95;STAR&#95;CONE | interval(lower=25.0,lower&#95;bound=inclusive,upper=25.34,upper&#95;bound=inclusive) | 1 | interval | measured(source=calibration&#95;plate(plate="COUPON-PLATE-5-compliant-vs-stiff",record="docs/reference/plate-05-results.md",locator="§1 and §5",date=on&#95;or&#95;before("2026-07-30")),err=null,note="Plate 5 conical-arm bracket with one boundary inversion at 25.00; retain the interval rather than a midpoint.") |
| X&#95;STAR&#95;CYLINDER | interval(lower=19.91,lower&#95;bound=exclusive,upper=30.48,upper&#95;bound=exclusive) | 1 | interval | measured(source=calibration&#95;plate(plate="COUPON-PLATE-6-support-arm-capacity",record="docs/reference/plate-06-results.md",locator="§5 Finding 2",date=exact("2026-07-31")),err=null,note="Cylindrical-arm bracket with n = 1 per bracket edge, conditional on 4.00 mm arms and a 30 mm² cap; retain the interval and never its midpoint.") |
