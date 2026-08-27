# plinth-3d code review

207 verified findings across 25 reading lanes. All paths are relative to
`/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`.

---

## 1. Verdict

This is a well-built codebase with a specific and repeating structural failure. The
foundations are genuinely strong: `rust/geom_kernel` counts and charges every
allocation before it makes it, `rust/chitubox_io` re-parses and audits every file it
writes, `plinth-orient` reads every one of its weights from the constants registry,
and the registry itself (`crates/plinth-core/src/constants/`) carries dated, checkable
derivations for its measured values. Error handling in the large support engine is
better than its size suggests — the 11,000-line `cco.rs` contains no `unwrap` or
`panic` outside its tests. The failure is at the seams: a number, a predicate, or a
contract is established in one place and then restated by hand somewhere else, and the
two copies disagree or will. That single pattern produces most of what follows —
drilling literals that duplicate the registered drain-keepout ladder, two functions
that both claim to define "the regular contact set", a printable-envelope constant that
ignores the printer profile the same function just validated, and a hard-typed `12.0`
standing in for the derived `X_ALLOWED`.

The one thing most worth fixing is the pre-write gate
(`crates/plinth-pipeline/src/gates.rs:303-313`): eleven of its twelve named safety gates
return `NotYetPorted` and cannot refuse anything, the twelfth delegates both its
measurement and its accept/reject predicate back to the crate it is meant to be
judging, and the list of what did not run is dropped one hop before the run report so
no operator ever sees it. Everything else on this list is a defect you can find by
reading; this one is a safety property the architecture claims and does not have, and
it is what makes every producer-side bug below unrecoverable at the boundary. The
strongest *concrete* defect is a different one and is second on the list: a global face
id used as a slot index in `rust/geom_kernel`, which panics or silently exempts the wrong
faces from a clearance check, has live production callers, and is a one-token fix. Start
there, because it costs an hour. Most of the remaining 195 findings are cleanliness
rather than correctness — vocabulary drift, duplicated three-line helpers, stale
`#[allow(dead_code)]` reasons — and should be read as a backlog, not an emergency.

---

## 2. The short list

Ranked by consequence times reachability. Effort is `trivial` (under an hour),
`contained` (a session), `substantial` (a focused piece of work), `project`.

### 1. The pre-write gate implements one of its own twelve gates, and hides that it did not run the other eleven
**Where:** `crates/plinth-pipeline/src/gates.rs:303-313`, disclosure dropped at
`crates/plinth-pipeline/src/prepare.rs:2057-2087`; the one implemented gate delegates its
predicate at `crates/plinth-verify/src/lib.rs:330`.
**Problem:** `Clearance`, `Load`, `BallBreach`, `Murray`, `Printability`,
`Connectivity` and five more return `Ok(GateVerdict::NotYetPorted)`; the gate reports
them in `pending_gates`, `ChainOutcome` carries the list, and `artifact_outcome` then
builds `ModelPreparationOutcome::Artifact` without it. `crates/plinth-cli/src/execute.rs:222`
cannot print what it is not given.
**Consequence:** Emission is authorized on one gate of twelve. Every physics check that
decides whether printed supports hold is enforced only inside the producer, with no
independent re-check before a file is written, and the run report says nothing about it.
The guard test that claims to protect this hop sets `pending_gates: Vec::new()`
(`prepare.rs:2715`), so it is structurally blind to the loss.
**Do:** Add `pending_gates` to the `Artifact` variant and print it (trivial, do today).
Then port the predicates — not the measurements — for `Load` and `BallBreach` first,
judging evidence recorded on `SupportVerificationSnapshot` rather than re-running
producer code.
**Effort:** trivial for the disclosure; project for the gates.

### 2. A BVH global face id is used as a slot index, so a subset surface panics or exempts the wrong faces
**Where:** `rust/geom_kernel/src/queries.rs:1827` and `rust/geom_kernel/src/queries.rs:2142`.
**Problem:** `bvh.rs:76-77` states and implements that leaf order carries *global* face
ids even for a narrowed view, and `mesh.rs:387-400` makes `face_ids()` a slot→global map
of length `nfaces()`. Both sites do `self.mesh.face_ids()[tid as usize]` with `tid`
already global. On a full soup the map is the identity and nothing shows; on a sparse
subset it indexes out of bounds.
**Consequence:** `faces_near_segments_batch`
(reached from `crates/plinth-support/src/seat_zone.rs:477`) can panic inside a rayon
worker, and `segment_clearance_excluding` (reached from
`crates/plinth-support/src/brace.rs:1832`) can exclude the wrong faces from a clearance
safety check — the mechanism whose whole doc argues that naming faces is what makes the
exemption honest. The "every native query family" subset test
(`rust/geom_kernel/tests/native_api.rs:1378-1445`) reaches neither API. This is the
review's most direct defect: a wrong index, live production callers, and two bad
outcomes, one of them a silently weakened safety check.
**Do:** Replace both expressions with `tid`. Extend the subset test to cover
`segment_clearance_excluding` with a non-empty per-row exclusion set and
`faces_near_segments_batch`.
**Effort:** trivial.

### 3. Drain sites are searched only around each chamber's single global low point
**Where:** `crates/plinth-hollow/src/drains.rs:1288`; the refill that follows at
`crates/plinth-hollow/src/drilling.rs:1025-1039`.
**Problem:** `low_lobes` admits a voxel only when its Z is below
`chambers.low_z_mm[chamber] + band_mm` (15 mm). A "chamber" is a 6-connected region of
the whole selected cavity, so a treant's trunk, limbs and legs are one chamber and
`low_z_mm` is the single lowest voxel in the model. A raised foot or a limb whose own
local minimum sits higher than that band never becomes a lobe and never gets a planned
drain. Drilling's gravity map uses a different, downhill partition
(`drilling.rs:1682-1707`), finds that basin undrainable, and refills it to solid.
**Consequence:** A limb whose own local minimum sits above that band gets no planned
drain, is found undrainable at drilling, and is refilled to solid: the trunk prints
hollow and the limb prints solid. The mechanism is certain from the code. The
attribution to the "it used to hollow body and both legs" regression is a hypothesis,
not a result — the committed Weeping Treant tests stop before drilling, the corpus STL
is not in git, and no run of that model reached the refill during this review. Treat the
chamber-versus-basin defect as established and the named regression as the leading
explanation for it.
**Do:** Band the search per drainage basin, not per 6-connected chamber — reuse the
downhill/union-find partition `drilling::drainability_map_admitted` already builds, and
emit one lobe per basin. Correct the doc at `drains.rs:1272-1276`, which currently
asserts behaviour the code does not have.
**Effort:** substantial.

### 4. The trapped-cavity refill is one relaxation pass; drilling then demands the fixpoint
**Where:** `crates/plinth-hollow/src/drilling.rs:1035-1040` (one pass) versus
`crates/plinth-hollow/src/drilling.rs:712-722` (equality required).
**Problem:** Drainability is level-restricted, so refilling voxel W can trap voxel A
that drained through it. `prepare_audit_plan` marks one round of non-draining voxels;
`drill` recomputes and returns `DrillingRefusal::TrappedCavity` if the counts differ. A
two-step cascade in a branched cavity — trunk plus limb plus limb is exactly that
shape — refuses the whole model.
**Consequence:** A perfectly valid shell is discarded; `prepare.rs:1427-1455` stamps
`SolidDecisionReason::HollowStageRefused` and the model prints solid. No test exercises
more than one refill step.
**Do:** Loop the mark step to a fixpoint (monotone on a finite set; 2-3 rounds in
practice), keep the check in `drill` as the assertion it was meant to be, and add a
cascading-pocket regression test. Cache the whole-grid drainability scan first or with it
(`crates/plinth-hollow/src/drilling.rs:1642`, theme 3.5) — iterating multiplies that cost
directly.
**Effort:** contained.

### 5. Two different functions define "the regular contact set", and a production validator compares one against the other
**Where:** `crates/plinth-support/src/runner.rs:7320` builds it with
`select_contacts_baseline` (repair off); `crates/plinth-support/src/specialized.rs:2405`
replays it with `crate::select_contacts` (repair on) and demands digest and value
equality.
**Problem:** `validate_regular` runs unconditionally on the production path
(`specialized.rs:2014`). The repair-enabled selector reads
`config.coverage_repair_max_attempts` and can push extra contacts (`lib.rs:1425`).
The repository already contains a fixture proving the two selectors diverge
(`specialized.rs:11055-11118`).
**Consequence:** On any model where the legacy pre-routing loop finds one failing
coverage sample, the run is refused as `StaleRegularContacts` — an error text that
misstates the cause, on a coverage verdict the production design deliberately does not
use. If the loop exhausts its attempts instead, `UnsatisfiedCoverage` propagates and
kills the run. The module doc at `coverage_closure.rs:3-8` claiming the legacy constant
has no production reader is what makes this invisible.
**Do:** Replay through the same selector that produced the value —
`select_contacts_baseline` — then delete or mode-gate the legacy pre-routing loop and
correct that module doc.
**Effort:** contained.

### 6. The law that sets every printed support tip radius is built from four "legacy diagnostic" placeholders
**Where:** `crates/plinth-support/src/perception.rs:626-642`; the checks it drives at
`perception.rs:535-542` and `crates/plinth-support/src/contact.rs:485-488`.
**Problem:** `FrozenTipLoadPolicy::from_placeholders` reads `peel_pressure_pa`
("Legacy diagnostic split, potentially about four times low"), `tensile_green_mpa`
("not a measured material property"), `tip_safety_factor` and two radius clamps, then
sizes tips by `tensile/(pressure * safety_factor)`. `docs/physics.md:21-33` says the
engine sizes to the measured area law `A_arm >= A_supported / X_ALLOWED`. It does not.
The two agree at 12 only because `0.288e6/(3000*8)` happens to; the code's own comment
at `perception.rs:581-586` says so, and the only cross-check
(`implied_area_ratio() <= x_allowed`) lives inside `#[cfg(test)]`.
**Consequence:** Correcting `PEEL_PRESSURE_PA` in the direction its own registry note
says it is wrong silently re-sizes every arm on every model against a ratio with no
relation to the measured snap boundary, and nothing in production notices.
**Do:** Either size from `X_ALLOWED` directly, or state plainly in the registry note and
in `docs/physics.md` that the shipped law is the legacy stress form and its agreement is
coincidental. Move the one-sided anchor assertion out of the test module into
`FrozenTipLoadPolicy`'s constructor.
**Effort:** substantial.

### 7. The island-birth overlap test ignores holes, so a solid born inside a cavity is never a birth
**Where:** `crates/plinth-support/src/perception.rs:2164-2184`; the hole-aware predicate
it should be using is 70 lines above at `perception.rs:2090`.
**Problem:** `solids_overlap` samples only `outer` contours. On a hollowed model — the
normal pipeline for this engine — the previous layer is an annulus, so a new solid
appearing inside the cavity reads as *overlapping* the prior layer and `find_births`
records no birth. The behaviour is pinned as deliberate frozen-Python parity
(`perception.rs:7989`), while the neighbouring reach criterion records the identical
physical error being fixed (`perception.rs:8057`: "Physically there is nothing under it").
**Consequence:** An unconditional support demand is silently removed and the public birth
plan is wrong. The minima/reach path may still recover ordinary cavity geometry, so this
is a lost demand rather than a guaranteed unsupported island — but the two filters now
disagree about what "carried" means.
**Do:** Have `solids_overlap` ask the question `solid_contains` asks, and include hole
boundaries in the edge pass. This moves frozen slice digests, so re-pin
`frozen_birth_overlap_ignores_retained_negative_winding_holes_like_python` with a
recorded rationale — the reach-criterion note is the precedent. Fold in the sampling fix
(overlap-sampling, theme 4) so the digests move once.
**Effort:** substantial.

### 8. `Placeholder` values reach acceptance paths, including the emission-blocking coverage gate
**Where:** `crates/plinth-core/src/provenance.rs:1373-1390` (`diagnostic()` is public and
unconditional); `crates/plinth-core/src/constants.rs:295-300` claims the wrapper
"structurally bars those settings from acceptance"; used as gates at
`crates/plinth-support/src/coverage_closure.rs:203`, `crates/plinth-hollow/src/lib.rs:1227`
and `:1236`.
**Problem:** `AcceptSafe` is a sealed trait with a compile-fail test proving `Placeholder`
cannot satisfy it — and it is used as a bound in exactly one production signature
(`crates/plinth-core/src/layer.rs:28`). Everywhere else the value is taken with
`.diagnostic()`; there are 414 such calls. Three of them decide whether a model may be
written: the coverage radius is `1.5 * 4.0` from two values whose registry notes say
"selected without measurement", the censor area is `voxel_pitch_mm` squared "inferred
from one corpus run", and the hollow savings and minimum-wall gates read the same way.
**Consequence:** The project's central doctrine control is a naming convention with one
enforced instance, and a reviewer reading the type signatures gets a false assurance.
**Do:** Reclassify `HOLLOW_SAVE_MIN_ML` and `HOLLOW_WALL_MIN_MM` as `Measured` with
`EvidenceSource::owner_decision` (the pattern `TIP_DIAMETER_MIN_ALLOWED_MM` already
uses). Take `&impl AcceptSafe<f64>` on the signatures that produce a verdict, and require
a named `sanctioned_search_read()` wrapper for placeholder reads that only prune, so
every acceptance-path placeholder read is visible at its call site.
**Effort:** contained for the reclassification; project for the enforcement.

### 9. Drilling re-derives the registered drain keepout ladder as literals, in the exact drift class its own comment condemns
**Where:** `crates/plinth-hollow/src/drilling.rs:1416-1424` (validator),
`crates/plinth-hollow/src/drilling.rs:1251-1252` (audit hole),
`crates/plinth-hollow/src/drilling.rs:35-41` (search directions) — against
`crates/plinth-hollow/src/drains.rs:403-423`, which reads all of it live.
**Problem:** `HOLLOW_EXTRA_DRAIN_POLICY.extensions_mm` is `[4.0, 8.0, 9.0]`, giving
before 4.0, drain-after 4.0, vent-after 5.0 — numerically equal to the literals today,
so the duplication is silent. `HOLLOW_DRAIN_DIRECTIONS` is likewise duplicated as
`AUDIT_DIRECTIONS`. Twelve lines above the audit-hole constructor the file documents this
exact hazard for the bore radius: "It used to be the literal 1.5 written here … and
silently stopped being it the moment the owner set the bore to 2.5 mm."
**Consequence:** A valid registry edit — one that keeps the ladder increasing — makes
`validate_frozen_hole_geometry` refuse every hole the planner just produced, and the
entire model goes solid. Two lanes found this independently.
**Do:** Read the ladder once in `drill`/`prepare_audit_plan` and pass `before_mm`/`after_mm`
into `audit_hole` and `validate_frozen_hole_geometry`, the way `drains::plan` already
does; read `hollow_drain_directions()` for the audit, or add a pin test shaped like
`cylinder_sections_matches_the_registered_hollow_extra_drain_policy`.
**Effort:** contained.

### 10. Frozen, uncalibrated compatibility constants decide brace geometry today, behind dead-code labels saying they do not
**Where:** `crates/plinth-support/src/legacy_compat.rs:38-42`, `:77-81`, `:157-161`; live
callers at `crates/plinth-support/src/cco.rs:5607-5619`, `:5665-5669`,
`crates/plinth-support/src/brace.rs:1068-1089`, `:2096-2108`,
`crates/plinth-support/src/rewire.rs:842-873`.
**Problem:** Three methods carry `#[allow(dead_code, reason = "the future current-support
controller is the sole O-10 caller")]` while the production `run_support` path calls all
three. The frozen 0.01 N shaft-drag load and 18 mm brace-free span — explicitly
`Placeholder`, explicitly not measured — set the priced sizing span, the brace rung
count, the brace rod radius and rewire radii.
**Consequence:** Two doctrine failures at once: uncalibrated numbers deciding emitted
geometry, and labels that would lead a maintainer doing dead-code cleanup to treat live
sizing as inert scaffolding. The same stale-label pattern recurs in `transient.rs:424`,
`legalization.rs:412` and `coverage_closure.rs:600`.
**Do:** Delete the false allows (the items compile without them), and open a tracked item
to replace 0.01 N / 18 mm with a derived load. Do the label sweep across the crate before
any structural work — an accurate reachability signal is what makes a later split
verifiable.
**Effort:** trivial for the labels; substantial for the physics.

### 11. Two kernel tolerances decide support safety, and their entire recorded provenance is a deleted Python file
**Where:** `rust/geom_kernel/src/consts.rs:22` (`ZDEDUP = 5e-3`, consumed at
`rust/geom_kernel/src/parity.rs:624-631`) and `consts.rs:35` (`NEAR_WALL_SKIP = 0.05`,
consumed at `rust/geom_kernel/src/queries.rs:2961-2966`); the cone that samples the same
predicate is two more unregistered literals at
`crates/plinth-support/src/anchor.rs:31-36`.
**Problem, certain:** both numbers are outcome-deciding and neither has provenance.
`ZDEDUP` merges crossing clusters closer than 5 µm, which changes the parity count and
therefore flips a point between inside and outside — the oracle the whole hollow and
support stack rests on. `NEAR_WALL_SKIP` discards any second crossing within 0.05 mm of
the first. The cone half-angle (25°) and ray count (8) that sample the predicate are
unregistered, carry a stale `allow(dead_code)`, and sit under a comment asserting that
only the radius and epsilon are result-changing. `find` over the repo returns only
`geometry_exact.cpython-311.pyc` — there is no readable source at any of the nine cited
line numbers.
**Consequence, plausible not demonstrated:** reading the code, a suppressed second
crossing makes `cone_thickness` report the *next* surface, so `breach_one` compares an
inflated thickness and reports no breach — a support foot ball admitted through a
sub-0.05 mm feature such as a cape edge or a fin. That chain also depends on the
surrounding `r - nd` precondition and on the cone directions, and no witness geometry was
constructed or run to produce the outcome. Rank it as a safety defect worth a fixture,
not as an observed failure.
**Do:** Register the geom_kernel tolerance set with honest "inherited from the retired
Python oracle, no measurement of record" notes and fix the header, which promises a link
that does not resolve. Build the thin-wall fixture first: it either produces the breach
or bounds it. Then make the near-wall skip a same-face (or same-normal-sign) test rather
than an absolute distance, and record the smallest wall the predicate can resolve.
Delete the false comment and allows on the cone constants.
**Effort:** contained.

### 12. The per-trial coverage measurement re-validates and re-scans superlinearly, inside a loop with no trial cap
**Where:** `crates/plinth-support/src/runner.rs:7724` (`validate_named_islands` inside a
per-contact helper) and `crates/plinth-support/src/runner.rs:3412-3488`
(`measurement_preflight`).
**Problem:** For N routed contacts, one controller measurement fully re-validates the
island sidecar 2N+1 times — N during `verification_snapshot`, then once directly plus N
through `validate_routed_memberships`. `measurement_preflight` separately runs an
O(M² + K(M+S)) set of scans, where S is the coverage-sample count (roughly 4x10⁵ on a
flagship model, per the file's own note). The repair controller takes a full measurement
per trial and the trial count is deliberately uncapped.
**Consequence:** The verification path is superlinear in exactly the loop the design
removed the cap from. The fix pattern already exists in this file: `IslandFaceIndex`
(`runner.rs:7661`) answers membership in two binary searches, and the D-37 comment 90
lines below the preflight explains why the neighbouring scans were removed. The
complexity is read from the loops; nothing was profiled, so this is the last item on the
list rather than a measured hot spot.
**Do:** Validate the sidecar once per snapshot and pass a pre-built `IslandFaceIndex`
into `routed_logical_routes`; group memberships and bucket coverage samples by
`(island_id, component_id)` once in the preflight. Return the index from the preflight
rather than rebuilding it 200 lines later (`runner.rs:3749`).
**Effort:** contained.

---

## 3. By theme

Findings already in the short list are not repeated. Severity is consequence, not
effort.

### 3.1 Architecture and layering

The crate names carve the problem well, and `plinth-core` really is a
geometry-free vocabulary crate. Two things spoil it. First, the stage handoff types are
each stage's own newtypes, so the dependency chain is hard and linear and the crates at
the end of it compile everything before them: `plinth-emit` uses four types from
`plinth-support` and gets 100,000 lines plus 18 vendored C++ translation units.
Second, the verifier depends on the producer — for coverage it calls the producer's
measurement *and* its accept/reject predicate — so "independent verification" is a
consistency guarantee, not an independence one. A third, smaller pattern: general
facilities have landed wherever they were first needed (progress reporting inside a
coverage-repair module, a contour engine inside the support crate), and lower crates
then copy them because they cannot reach up.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-verify/src/lib.rs:330` | The one implemented pre-write gate delegates both its measurement and its blocking predicate to `plinth-support`, the crate it judges. | substantial |
| medium | `crates/plinth-emit/Cargo.toml:13` | Emit's four semantic handoff types live in the implementation crate, so the emission boundary inherits the whole engine's build closure. | substantial |
| medium | `crates/plinth-metrics/Cargo.toml:1` | The 9,176-line audit crate has no production consumer — the `plinth score` subcommand it served was removed, and `docs/architecture.md:137` still presents it as live. | contained |
| medium | `crates/plinth-pipeline/src/lib.rs:364` | `PrepareRequest` is a union of stage-crate types, so the CLI depends directly on `plinth-hollow` and `plinth-orient` and stage request changes reach the binary. | substantial |
| medium | `crates/plinth-pipeline/src/coverage_evidence.rs:354` | Coverage evidence recovers `plinth-verify`'s private field shape by round-tripping JSON and replaces any parse failure with empty patch evidence. | contained |
| medium | `crates/plinth-support/src/perception.rs:1581` | A general 2D contour stitcher and its primitives sit inside the support crate; `plinth-layers`, one crate below, copied `signed_area` verbatim because it cannot reach them. | project |
| medium | `crates/plinth-support/src/runner.rs:194` | `SupportPreparationLimits::production` reads four caps from the registry and restates 34 more as literals whose registry notes name that function. | contained |
| low | `crates/plinth-support/src/coverage_repair.rs:987` | `StageClock`, a pipeline-wide progress facility, is defined inside the coverage-repair module, so no crate can use it without depending on the support engine. | trivial |

### 3.2 Design flaws, and constants that decide outcomes

This is the largest theme and the one the doctrine speaks to directly. The pattern is
consistent: a threshold with real authority — it decides accept or reject, or how much
material gets printed — is written as a bare millimetre literal at the point of use,
often a few lines from a neighbouring value that *is* read from the registry. Two
variants make it worse than plain debt. A search bound returned as if it were a
solution (`slender_rungs` returns 50 whether or not 50 rungs satisfied the deflection
predicate), and a comment that denies the number is result-changing when it is (the
anchor foot-breach cone). The registry itself is excellent; it is the boundary between
registry and use that leaks.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| high | `crates/plinth-support/src/brace.rs:2941` | `slender_rungs` searches 0..=50 and returns `Ok(50)` even when the deflection predicate never passed — an unsatisfied search is indistinguishable from a solution. | contained |
| high | `crates/plinth-mesh/src/triage.rs:16` | `TriageSettings` are documented as diagnostic-only placeholders, yet they decide which components are silently deleted as dust shards and which models are hard-refused as `PreHollowed`. | project |
| high | `crates/plinth-hollow/src/lib.rs:2178` | `PRINTABLE_MAX_EXTENT_MM = 300.0` gates all three axes while the registered Saturn build volume is 211.68 x 118.37 x 220; the profile is in scope at the call site. Found by two lanes. | contained |
| high | `rust/geom_kernel/src/queries.rs:3099` | `exterior_face_mask` decides which faces are legal support seats from two inline probe offsets (0.05, 0.15 mm) and a 4-of-6 vote, with no physical input. Found by two lanes. | contained |
| high | `crates/plinth-support/src/perception.rs:2338` | A bare `0.2` mm decides whether a local minimum is an island birth, beside a registry-read reach allowance of 0.0707 mm. | contained |
| high | `crates/plinth-metrics/src/score.rs:775` | The `max_x` hard bound is `value > 12.0` instead of the derived `X_ALLOWED`; `plinth-metrics` never reads the anchor and its tests pin the literal. Latent only because the crate has no production caller. | contained |
| high | `crates/plinth-hollow/src/drilling.rs:665` | H0 preserves authored extra bodies and has a passing test for it; H1b refuses any shell with more than one positive component, in three places. | contained |
| medium | `crates/plinth-support/src/specialized.rs:34` | `SYNTHETIC_PATCH_RADIUS_MM = 1.5` is an unregistered island-extent authority; `BIRTH_LINK_RADIUS_MM = 0.6` duplicates and bypasses the registered `island_birth_link_r_mm`. | contained |
| medium | `crates/plinth-support/src/brace.rs:1940` | A bare `0.1` selects which rods get the exact penetration screen and which get only a midpoint inside/outside test. | contained |
| medium | `crates/plinth-support/src/joints.rs:16` | Joint existence, coincidence merging and plate-band classification are decided by three fixed millimetre tolerances unrelated to member radii or process resolution. | substantial |
| medium | `crates/plinth-support/src/cco.rs:4527` | An unnamed `+ 1.0` mm decides which of the 84 spiral plate-descent candidates exist at all, in a function where every other threshold comes from `Config`. | trivial |
| medium | `crates/plinth-support/src/cco.rs:5550` | `candidate_price` returns three incommensurate quantities (new-strut volume, whole-forest delta, a synthetic plate estimate with hypothetical braces) and ranks them with one `total_cmp`. | contained |
| medium | `crates/plinth-support/src/brace.rs:3008` | `rungs_that_fit` removes requested cross-braces using an unexplained universal `- 1.0` mm. | trivial |
| medium | `crates/plinth-support/src/rewire.rs:163` | `Config::current` reads five registered values and then refuses any value but a hard-coded literal, so the registry cannot move the behaviour — only break it. | contained |
| medium | `crates/plinth-support/src/runner.rs:209` | ~20 further production resource caps are unregistered literals that can each end a run as a refusal. | contained |
| medium | `crates/plinth-support/src/lib.rs:436` | `SupportPrimitive` encodes variant state as public tagged optional fields; invalid combinations are representable and caught only at `SupportedModel::from_parts`. | substantial |
| medium | `crates/plinth-support/src/perception.rs:2557` | A minimum's exterior/interior verdict uses the nearest *face centroid* although the vertex's exact incident-face set was built 350 lines earlier and discarded. | contained |
| medium | `crates/plinth-pipeline/src/prepare.rs:1787` | The layer plan every support run is judged against adds a bare `1.0` mm headroom and a literal 1,000-layer minimum, provenance recorded only as "carried over verbatim". | contained |
| medium | `crates/plinth-metrics/src/topology.rs:17` | The mandated-cavity exemption from the zero-bound `negative_shells` metric is decided by a 9-of-10 vote over the first 32 vertices in face order. | substantial |
| medium | `crates/plinth-metrics/src/scar.rs:1049` | Scar exterior classification uses fixed 0.05/0.15 mm probes and a fixed 0.005 mm crossing merge, neither scale-relative nor material-derived. | substantial |
| medium | `crates/plinth-metrics/src/scar.rs:183` | `scar_weighted` is a normative accept/reject row driven by an admittedly-assumed 0.6/0.4 proxy blend read through `Placeholder::diagnostic()`. | substantial |
| medium | `rust/geom_kernel/src/queries.rs:1891` | `segment_clearance` returns `INFINITY` as an in-band "not measured" sentinel; `crates/plinth-support/src/rewire.rs:1088` reads it as a pass. | contained |
| medium | `rust/geom_kernel/src/segment_batch.rs:225` | `point_segment_distances_batch` can only return the dense P x S matrix, so the caller that wants a per-point nearest hand-rolled a scalar loop and a duplicate degeneracy epsilon. | contained |
| low | `crates/plinth-decimate/src/lib.rs:2082` | A self-declared non-derivable 32,768-face threshold alone refuses otherwise-safe reductions on the two known corpus models with eligible candidates. | contained |
| low | `crates/plinth-emit/src/lib.rs:588` | Raft pad-versus-bridge is dispatched on exact `f64` endpoint equality; correct today only because one producer passes the same value twice. | contained |

### 3.3 Correctness

Reachable wrong behaviour outside the short list is bounded and mostly local. The
recurring shape is a check that is not re-run after the value it checked has changed
(the anchor seat after ray snapping), or an accumulator whose recorded meaning drifts
from what it holds (the rewire retire path). One entry is about the write itself: the
artifact reaches its final name through two chained renames, and neither of them syncs
the directory that holds the name, so "the file is on disk" is weaker than the emit doc
claims.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| high | `crates/plinth-support/src/anchor.rs:1193` | Anchor qualification is not repeated after the ray snaps to a nearer face: the minimum-length prune and, more seriously, the `max_delicacy` screen are never applied to the face actually seated on, and the seat records the source face's delicacy under the snapped face's id. | contained |
| high | `crates/plinth-support/src/cco.rs:3528` | `HostCache` ranks cached members by a stale three-sample minimum and newer members by current midpoint only, so a 25-terminal rebuild cadence — a pure performance knob — decides which bifurcation hosts the tournament may weigh. | contained |
| medium | `crates/plinth-support/src/brace.rs:2140` | Model-rung seats are generated only from face centroids, so a coarsely triangulated wall straddling the rung band yields no candidate and an avoidable `Unplaceable`. | substantial |
| medium | `crates/plinth-support/src/rewire.rs:837` | `try_retire` commits unconditionally and clamps a volume *increase* to a zero saving; 20 such repairs in a row satisfy `RollingSavingsConverged` and stop the sweep, and the disclosed curve stops matching the recomputed final volume. | contained |
| medium | `crates/plinth-support/src/perception.rs:1955` | `IslandBirth::centroid_xy_mm` is the unweighted boundary-vertex mean, not an area centroid — sampling-density dependent, not guaranteed inside a concave footprint — and is used directly as the support seed position. | contained |
| medium | `crates/plinth-support/src/designed_embed.rs:1758` | The seat attestation digest binds outcomes but no seat geometry, so translated-but-different refused candidates produce identical transcripts. | contained |
| medium | `crates/plinth-support/src/specialized.rs:8333` | Three hand-maintained hash lists over `SpecializedLimits` already disagree; the two batch-work coefficients that materialize the gate and load operation caps are in none of them, so differing authorities share a digest. | contained |
| medium | `rust/chitubox_io/src/project.rs:618` and `crates/plinth-emit/src/lib.rs:81` | Neither rename syncs the directory that now holds the artifact's name. `save` calls `sync_all` on the file bytes and then renames; emit renames that result a second time. On a filesystem where a rename needs an explicit directory sync to be durable, a power loss after a successful return can leave the final name absent or pointing at the previous artifact, even though both calls said `Ok`. Reopening and auditing before the rename proves content, not persistence of the name. | contained |
| medium | `rust/geom_kernel/src/mesh.rs:244` | `Mesh::from_indexed` divides by `length.max(1e-30)` and has no degenerate-face error, so a repeated-index or collinear face is stored with a `[0,0,0]` "normal" that consumers treat as a direction. | contained |
| low | `rust/chitubox_io/src/project.rs:821` | The Append support path uses unchecked `as u32` for mesh length and offset where the sibling Replace path uses the crate's checked `u32_of`. | trivial |

### 3.4 Numerics and units

The float-equality picture is better than the raw clippy counts suggest — two lanes
cleared the census's top-ranked equality sites by reading and found them to be
exact-copy comparisons. What is real: the same ray/triangle question is answered four
ways with four different guards, so the "independent" auditor cannot falsify the
producer on grazing cases; two "degenerate" gates test an accumulated volume for exact
`== 0.0` and so pick winding from the sign of the noise; and one interval type promises
an exact enclosure while rounding to nearest. The units trap is live in exactly one
place, and it is a public type.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-support/src/raft.rs:335` | Raft pads pass a *diameter* in `start_radius_mm` and a *height* in `end_radius_mm` on a public struct; only one consumer special-cases it, and `is_semantically_valid` constrains neither. | contained |
| medium | `rust/geom_kernel/src/moller.rs:16` | Four Möller ray/triangle implementations across three crates with four determinant guards and three barycentric slacks; a triangle within 1e-6 of an edge is a hit for the kernel and a miss for triage and the audit. | substantial |
| medium | `crates/plinth-metrics/src/topology.rs:578` | The degenerate-shell gate is `corrected == 0.0` on an accumulated tetrahedron volume, so it effectively never fires and winding falls to the sign of the residual; same shape at `crates/plinth-hollow/src/lib.rs:2293`. | contained |
| medium | `crates/plinth-metrics/src/scar.rs:2122` | `RealInterval` documents itself as an exact real preimage but `norm_interval` and `interval_product` round to nearest, so the enclosure is not sound and the "proof" is approximate. | contained |
| medium | `crates/plinth-support/src/perception.rs:2165` | Birth overlap decimates contours to ~40 sampled vertices and ~60 *chords* (`i + step`, not `i + 1`), so the verdict changes with tessellation density. Fix with short-list item 7. | contained |
| medium | `crates/plinth-support/src/specialized.rs:8213` | Demand ordering tie-breaks on `to_ne_bytes()`, which is neither numeric order nor target-independent, and that order reaches the sealed plan digest. | trivial |
| medium | `crates/plinth-support/src/brace.rs:1936` | Transit acceptance passes literal `8`, `0.12` and `80` to the deciding penetration query, unrelated to member scale or process resolution. | substantial |
| low | `crates/plinth-support/src/runner.rs:5427` | A NaN-filled `ArmRefusalPhysics` is minted as the "no measurement" sentinel and travels into a disclosure whose contract is measured millimetres. Plausible, not confirmed reachable. | contained |
| low | `crates/plinth-support/src/perception.rs:897` | `GenericContactPlan::digest` is not prefix-free across layers, loops and minima, unlike the two length-prefixed fingerprints beside it. Plausible. | trivial |
| low | `crates/plinth-mesh/src/topology.rs:969` | `vertex_count` uses a bare `as u32` in all three branches while every id assigned two lines above is `u32::try_from`-checked. | trivial |
| low | `crates/plinth-support/src/member_sizing.rs:610` | `frustum_volume_ml` divides mm³ by `MILLIMETRES_PER_METRE`; right by coincidence, wrong by dimension, and a correctly named twin exists in `plinth-metrics`. The naming is the small half of the duplication entry in theme 3.7. | trivial |

### 3.5 Efficiency

The batch-first doctrine is being followed where it is most visible: one lane checked
all 83 non-test call sites of the kernel's slice-taking APIs and found zero
scalar-in-batch defects. The waste has migrated one level in. Three shapes dominate:
containers of containers where a flat CSR belongs (1.29M `BTreeSet` allocations for one
face-incidence build, called ~15 times per model); loop-invariant structures rebuilt per
iteration (the unchanged original mesh's BVH and weld map rebuilt on every decimation
probe); and whole subsystems that opted out of parallelism entirely — `plinth-hollow`,
`plinth-mesh`, `plinth-decimate` and `plinth-pipeline` have no rayon dependency, so the
voxel rasterizer's serial sweep over 429k triangles runs on one core while the rest of
the pipeline saturates the machine.

Read the whole theme with one caveat in front of it. Every figure here is analytic —
read off the loop bounds and the allocation sites — and nothing was profiled,
benchmarked, or traced. The *existence* of each complexity defect is supported by the
code. Its magnitude, and therefore the order of these rows, is not measured. Nothing in
this theme is rated high for that reason: high would need a demonstrated resource
refusal, a corpus-scale bound that crosses a stated limit, or a development cost you can
point at, and no lane produced one. The missing parallelism in `plinth-hollow`,
`plinth-mesh`, `plinth-decimate` and `plinth-pipeline` is an opportunity worth measuring,
not an established loss: whether it pays depends on how the work partitions and on
memory bandwidth, neither of which was tested.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-support/src/specialized.rs:744` | `RepairPool::build` takes no `limits` at all in a module whose contract is that every batch is priced first, and is O(islands x candidates + rows²) with linear `id_index`/`resolve_route` lookups on top. The unmetered work is the certain half: a module that prices every batch has one entry point that does not. | contained |
| medium | `crates/plinth-mesh/src/triage.rs:1015` | `classify_batch` is `points.iter().map(classify)`: three rays per point, each a full linear scan over every container face with no BVH, in a crate whose manifest says it does not get its own spatial index. A per-item call in a batch entry point, which the doctrine names as a defect regardless of measured cost. | contained |
| medium | `crates/plinth-mesh/src/topology.rs:527` | Face incidence is `BTreeMap<u32, BTreeMap<usize, BTreeSet<usize>>>` — one heap allocation per (vertex, face) pair, ~1.29M on a 429k-face model, ~15 calls per model. | contained |
| medium | `crates/plinth-hollow/src/lib.rs:2655` | The voxel rasterizer is a serial per-triangle sweep run twice (count pass, then mark pass) and the crate has no rayon dependency. | contained |
| medium | `crates/plinth-decimate/src/lib.rs:2795` | Every `probe_two_sided` rebuilds the *unchanged* original mesh's BVH and parity grid; the crate's own comment measures half its surface builds as redundant. | substantial |
| medium | `crates/plinth-decimate/src/lib.rs:3069` | Same shape for the exact-bit weld view and quantized positions, rebuilt per candidate ratio. | substantial |
| medium | `crates/plinth-support/src/cco.rs:5644` | `o10_uniform_radius_for_span` builds a whole `Forest` and heap-allocates an owner `String` per radius query, inside the per-ancestor sizing walk; a scalar hook already exists at `legacy_compat.rs:128`. | contained |
| medium | `crates/plinth-support/src/cco.rs:4619` | The tournament deep-clones the entire forest once per descriptor (~90 per terminal) and each survivor retains its clone for the rest of the round. | substantial |
| medium | `crates/plinth-support/src/cco.rs:4055` | A fresh rayon `ThreadPool` is built per terminal and per rewire sweep; `geom_kernel::par` already caches pools but keeps `mod par` private, so four crates copy the workaround. | trivial |
| medium | `crates/plinth-support/src/cco.rs:3528` | `HostCache::nearest_excluding` is a full linear scan plus a second full walk of live members plus two allocations, to return five nearest hosts, once per terminal. | contained |
| medium | `crates/plinth-support/src/cco.rs:2179` | The whole-forest volume delta is computed three times per rewire candidate where one would do; for bifurcations the second value is bit-identical to one already in hand. | contained |
| medium | `crates/plinth-support/src/rewire.rs:496` | Each viable slide/prune/retire clones the forest and recomputes total volume before and after, though `optimize` already tracks the running total. | substantial |
| medium | `crates/plinth-support/src/forest.rs:536` | `add_member` scans every slot for a reusable one (Θ(N²) to build a forest) and every geometry mutation linearly scans the terminal ledger. | contained |
| medium | `crates/plinth-support/src/coverage_closure.rs:688` | The scoring matrix's "tiling" permutes visit order without reusing anything, and after the clamp the registered 65,536 ceiling bounds only a chunk size, never total work. | contained |
| medium | `crates/plinth-support/src/specialized.rs:2896` | The island-birth satisfaction scan meters only its owner-matching subset, so the full O(births x routes) traversal is charged nothing. | contained |
| medium | `crates/plinth-support/src/specialized.rs:1580` | `derive_child_from` attributes load rows with `iter_mut().find` where the sibling site indexes directly — O(rows x contacts) on the repair edit seam. | trivial |
| medium | `crates/plinth-support/src/perception.rs:2205` | `find_local_minima` allocates one `BTreeSet` and one `Vec` per welded vertex for an adjacency it only iterates; the file's own `LoadEligibilityTranspose` is a hand-rolled CSR. | contained |
| medium | `crates/plinth-support/src/perception.rs:946` | `validate_binding` rebuilds the whole `NativeSurface` and re-hashes the whole mesh to answer a staleness question, at seven stage boundaries. | contained |
| medium | `crates/plinth-support/src/brace.rs:1316` | Every rung site rescans and re-sorts the full exterior face set. | substantial |
| medium | `crates/plinth-support/src/brace.rs:489` | Winner scattering restarts `candidates.iter().find` from index zero for every need/site pair. | contained |
| medium | `crates/plinth-support/src/raft.rs:343` | `euclidean_mst_edges` re-scans the full visited x unvisited cross product each iteration — O(n³) where dense Prim is O(n²). | contained |
| medium | `crates/plinth-support/src/transit_measurement.rs:1011` | `measure_scar` resolves every anchored member by linear search over the snapshot slice, three times, and runs the scalar preflight twice. | contained |
| medium | `crates/plinth-support/src/runner.rs:3967` | `validate_integrity` cross-checks contacts against memberships with a nested scan although both vectors are pushed in lockstep. | trivial |
| medium | `crates/plinth-support/src/runner.rs:4656` | `validate_initial_cco_preflight` scans the whole terminal and contact lists per terminal, with a non-short-circuiting `filter().count()`. | trivial |
| medium | `crates/plinth-support/src/perception.rs:2336` | Birth matching re-allocates a 65-point sample vector inside the inner `births.iter()` loop. | trivial |
| medium | `crates/plinth-hollow/src/drilling.rs:1642` | Each drill rebuilds the whole-grid drainability structure several times, each a full 5.3M-cell scan plus a 42 MB throwaway allocation, with `sort_by_key` recomputing the index per comparison. | contained |
| medium | `crates/plinth-metrics/src/scar.rs:272` | Support-record reconstruction forms the complete tips x tapers and members x balls Cartesian products. | substantial |
| medium | `crates/plinth-metrics/src/score.rs:271` | Support metric rows re-derive canonical meshes and shell topology that `topology_metric_rows` computed moments earlier. | substantial |
| medium | `rust/geom_kernel/src/point_index.rs:505` | The only query module in the kernel with no parallelism, in a module documented as batch-only. | contained |
| medium | `rust/geom_kernel/src/parity.rs:641` | Three independent axis grids built serially, each walking every face twice and ending in a serial sort of up to 2³² references, on the construction path every stage blocks on. | contained |
| medium | `rust/geom_kernel/src/slicing.rs:213` | Slicing descends the BVH twice per plane and allocates five fresh `Vec`s per plane, ~1,000 planes for a 50 mm model. | contained |
| low | `crates/plinth-support/src/perception.rs:1084` | The no-legal-candidate check is an islands x candidates scan where one pass over candidates answers every island. | trivial |
| low | `crates/plinth-support/src/runner.rs:3023` | Blocked-cause diagnosis brute-forces rows x uncovered samples; refusal-path only, so it makes refusals slow, not runs. | contained |
| low | `crates/plinth-support/src/seat_zone.rs:487` | The only rayon global-pool site in the workspace, on a per-terminal workload of a handful of tiny flood fills — and it defeats the `PLINTH_THREADS` sweep the explicit-pool convention exists for. | trivial |
| low | `crates/plinth-pipeline/src/prepare.rs:1823` | Every model pays a full mesh copy (~12 MB on a 496k-face shell) held through the peak-allocation phase, to serve an SVG written only on refusal. Disclosed, not hidden. | substantial |
| low | `crates/plinth-pipeline/src/prepare.rs:860` | The folder loop is strictly serial and the crate has no rayon; a real trade-off against oversubscription, not a free win. Plausible. | contained |
| low | `crates/plinth-decimate/src/lib.rs:2351` | `SimplifyRequest` owns its lock vector, so an unchanged `protected` slice is cloned per search candidate. | trivial |
| low | `crates/plinth-orient/src/score.rs:728` | A fresh thread pool per scoring tier, three per model — same root cause as the CCO pool churn. | contained |
| low | `rust/geom_kernel/src/queries.rs:2960` | `cone_thickness` allocates a hit vector and a direction vector per cone ray, though the allocation-free traversal is six lines away. | trivial |

### 3.6 Dead code

Most of what was sampled is not unreachable code; it is *mislabelled* code. Eleven
module-level `#[allow(dead_code)]` attributes in `plinth-support` carry reasons saying
their consumer has not landed yet, and the consumers landed. That has two costs: the
compiler's unused signal is off across roughly 57,000 production lines, and a maintainer
doing cleanup is told that live sizing code is scaffolding.

Genuinely unreachable items do exist, and two below are the honest kind of obsolete
shim — a kernel method kept for a Python surface this workspace no longer has, and a
vendored-source checksum inventory that nothing reads. State this as a claim about what
was sampled: no lane cross-referenced every public item in the largest crates, so "the
workspace has little real dead code" is not something this review can support.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-support/src/runner.rs:3372` | The 3,788-line `transit_measurement` module has no production caller — one wrapper is used only by unit tests, two by nothing. It is under an explicit reasoned allow, so this is a decision to make, not a lint to fix. | substantial |
| medium | `crates/plinth-support/src/cco.rs:4163` | `mask_inset0`/`mask_inset1` are allocated to row count and written per contact on the hottest batched path, and never read; the comment justifying them names a consumer that uses a different accumulator. | trivial |
| medium | `rust/geom_kernel/src/lib.rs:25` | `pub mod testkit;` is ungated, so every release build ships synthetic mesh generators, brute-force oracles and two public constructors that `unwrap` instead of returning `SurfaceBuildError`. | contained |
| low | `rust/geom_kernel/src/queries.rs:2872` | `penetration_intervals_batch` is `pub(crate)` and kept "solely for the inline PyO3 frozen-oracle surface… delete it with that Python surface in M9". There is no PyO3 surface in the crate, no `pyo3` dependency in its manifest, and no caller anywhere in the workspace — the only textual matches are string literals in a test. A real obsolete shim behind a retention reason that no longer describes anything. | trivial |
| low | `crates/plinth-solid/build.rs:60` and `crates/plinth-solid/vendor/manifold/FILES.sha256:1` | The vendored Manifold checksum inventory is inert. `FILES.sha256` holds 51 per-file hashes, and the build script's only use of it is `cargo:rerun-if-changed`; it never reads or compares a hash, and no other file in the repository mentions the manifest. Editing a vendored source triggers a rebuild and passes. The file reads as a supply-chain and reproducibility control and is documentation. | contained |
| low | `crates/plinth-support/src/transient.rs:424` | `resize_forest_current_parity` and its exclusive helpers carry stale "integration follows" dead-code allows; `brace.rs` calls it in two live paths. | trivial |
| low | `crates/plinth-support/src/legalization.rs:412` | `LegalizationSteering::round()` is marked dead one commit ahead of its reader; `runner.rs` reads it at four production sites. | trivial |
| low | `crates/plinth-support/src/coverage_closure.rs:600` | Seven allows on the ranking machinery say the one-winner controller "lands in the next commit"; it landed. | trivial |
| low | `crates/plinth-support/src/cco.rs:1353` | `cco_xia_alpha` is validated as a hard precondition and never used — the exponent is a literal `.sqrt()` — and the only function naming alpha is `#[cfg(test)]` with a test-only caller. | contained |
| low | `crates/plinth-support/src/designed_embed.rs:201` | The whole-forest designed-mask mint (`DesignedMaskTable`, `validate_forest`, `validate_members_batch`) has no production consumer; production uses the seat-attestation path instead. | substantial |
| low | `crates/plinth-support/src/cco.rs:4607` | The `cached_hosts` `None` fallback is unreachable *and* would rank hosts by a different metric if it were reached. | trivial |
| low | `crates/plinth-support/src/specialized.rs:2531` | `add_synthetic_islands` takes a `Mesh` it silences with `let _ = mesh;`, and re-sorts and dedups an already-sorted owner vector after every push. | trivial |
| low | `crates/plinth-verify/src/lib.rs:505` | `verify_artifact`, the structural format gate, has no production caller; `plinth-emit`'s own disk audit is the only format check that runs, and `docs/architecture.md` says otherwise. | contained |
| low | `crates/plinth-core/src/constants/placeholders.rs:1342` | Four `TrialSetting` perf-experiment constants have no production reader; their only test asserts they are disabled. | trivial |
| low | `crates/plinth-core/src/provenance.rs:1393` | The `Pending<T>` provenance category is fully implemented, tested, and used by none of the 306 registered constants. | contained |
| low | `crates/plinth-pipeline/src/lib.rs:373` and `crates/plinth-cli/src/execute.rs:84` | `--no-cache` is parsed, threaded and reported, and controls nothing because no cache exists. Honestly disclosed at runtime; still surface describing unbuilt machinery. | trivial |

### 3.7 Duplication

Two kinds. The benign kind is short helpers repeated because there is nowhere shared to
put them: `vec3` arithmetic appears four times inside `plinth-mesh` alone and again in
three `plinth-support` modules, because `geom_kernel` exports the `V3` type alias but
keeps `mod vec3` private. The harmful kind is where the copies have *drifted*: four
point-segment distance implementations in `plinth-support` with three different
degeneracy treatments (`== 0.0`, `1e-15`, a named `1e-18`), applied to the same
`SliceSolid` data by two engines that must agree. Where the project has already solved a
duplication once, the lesson usually did not propagate to the value beside it.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| high | `crates/plinth-mesh/src/triage.rs:1043` | A third hand-written Möller implementation plus a per-point full-face scan, where `delicacy_eval.rs` two files away does the same class of query in one batched kernel call. Same site as the efficiency finding above. | substantial |
| medium | `crates/plinth-metrics/src/geometry.rs:276` and `:300` | The auditor's ray/triangle copy still has the behind-origin defect that was fixed in `plinth-mesh`; `docs/status.md` lists it as open (#50). Auditor independence justifies a second implementation, not an unpinned divergent one. | contained |
| medium | `crates/plinth-support/src/perception.rs:2604` | Four 2D primitives copied across three files with drifted epsilons; `plinth-layers` documents its `signed_area` as "identical to the perception stitcher's". | contained |
| medium | `crates/plinth-support/src/member_sizing.rs:601`, `crates/plinth-support/src/cco.rs:5672`, `crates/plinth-support/src/rewire.rs:1186` | One tapered-member volume law, three live owners inside one crate. `frustum_volume_ml` takes a typed `MemberSpan` whose length is `dx.hypot(dy).hypot(dz)`; `member_volume` re-derives the length as `(dx*dx+dy*dy+dz*dz).sqrt()` — deliberately, for frozen NumPy parity — and repeats the formula; `frustum_raw` repeats it a third time with a bare `1000.0` divisor and no validation. The plain sum-of-squares can overflow or underflow where `hypot` stays finite, and ordinary inputs differ by rounding. These three feed candidate pricing and rewire savings, so a tie-sensitive decision can be made on a different number than the sizing path used. The divisor is spelled four ways across the workspace (`MILLIMETRES_PER_METRE`, `MILLIMETRES_PER_MILLILITRE`, `1000.0`, and the correct `CUBIC_MILLIMETRES_PER_MILLILITRE` in `plinth-metrics`). | contained |
| medium | `crates/plinth-support/src/joints.rs:15` | Seven module literals duplicate registry policy entries and decide ball removal, clustering and plate-band classification; the colinear cosine is a pre-evaluated conversion of a registered angle with nothing linking them. | contained |
| medium | `crates/plinth-support/src/specialized.rs:3001` | Two structurally identical restart-from-scratch merge loops with a byte-identical commit tail written three times, each restarting an O(n²) coincidence scan after every single merge. | contained |
| medium | `crates/plinth-support/src/specialized.rs:6227` | The gate's plan and fill gather the same route points from two hand-kept copies; disagreement surfaces as a fingerprint mismatch, not a compile error. | trivial |
| medium | `crates/plinth-support/src/perception.rs:4117` | `finalize_retained_round` re-implements `validated_contact_order` and `hash_physical_contacts` inline, in the two places that set the refusal order and the fingerprint the fast path must match. | trivial |
| medium | `crates/plinth-support/src/brace.rs:39` | `CROSS_LINK_DUTY_FACTOR` duplicates the registered, proof-backed `cco_x_pair_load_discount`, whose note cites two deleted Python files and never names the live reader. | trivial |
| medium | `rust/geom_kernel/src/queries.rs:1307` | Two ways to project onto a face subset; production picked `subset()` (fresh BVH plus three parity grids) for the exact use case `project_allowed_faces_batch` exists to serve — and that is the path that reaches short-list item 2. | contained |
| low | `crates/plinth-support/src/specialized.rs:8128` | `point_in_polygon` verbatim duplicated from `perception.rs`, with a different segment-degeneracy rule. | contained |
| low | `crates/plinth-support/src/cco.rs:5299` | The plate-topology predicate is written twice in two forms, both exact `f64` equality, in functions that must agree to size and price the same shape. | trivial |
| low | `crates/plinth-support/src/cco.rs:3021` | The FNV fold is hand-inlined three more times after `FnvWords` was factored out precisely to prevent that (the fourth copy, the oracle, is correct and should stay). | trivial |
| low | `crates/plinth-support/src/census.rs:23` | The census hard-codes a rung length band transcribed from `brace.rs`, so the instrument would lie about the very change it exists to measure. | trivial |
| low | `crates/plinth-hollow/src/drilling.rs:1737` | Three ties-to-even rounding implementations in one crate, two hand-rolled, used by the two functions whose results are compared against each other; `f64::round_ties_even` has been in std since 1.77. | trivial |
| low | `crates/plinth-hollow/examples/raster_delta.rs:271` | The ruling diagnostic copies the *current* production rasterizer, not just the retired one, against the crate's own stated rule about diagnostics. | contained |
| low | `crates/plinth-mesh/src/delicacy_eval.rs:389` and `crates/plinth-mesh/src/lib.rs:234` | Vec3 arithmetic four times, and triangle unit-normal twice with different degeneracy paths, inside one crate. | contained |
| low | `crates/plinth-layers/src/lib.rs:278` | `signed_area` duplicated across a crate boundary, with the doc comment asserting an identity nothing enforces. | trivial |
| low | `crates/plinth-orient/src/score.rs:252` | The down-facing predicate is copied between crates, protected by a source-line cross-reference in a doc comment that is already stale by 57 lines. | trivial |
| low | `crates/plinth-metrics/src/scar.rs:1282` | One axis-crossing cluster tolerance, two independent owners in two crates (`AXIS_CROSSING_CLUSTER_MM` and the kernel's `ZDEDUP`). | trivial |

### 3.8 Tests

This theme is a sample, not an audit, and the difference matters. The workspace holds
1,585 test functions and roughly 32,500 lines under `tests/` directories, plus inline
`#[cfg(test)]` modules on top of that. Lanes enumerated test names and read a few dozen
each. So the right reading of the table below is "these test shapes were proven bad",
not "the rest of the suite is sound" — most tests were never read closely enough to tell
an independent oracle from a restatement of the production code.

What was read shows real strengths: compile-fail tests enforce ownership boundaries,
geometry tests check cap-minus-one and thread identity, and the orientation rederivation
gate compares production output against independently committed artifacts. The proven
failures cluster in two shapes the doctrine names. Oracles that re-implement the
production formula (so a synchronized misunderstanding stays green), and assertions on
telemetry that is never written (so they compare a defaulted zero against zero). The
most consequential gap is not a bad test but a missing one: nothing runs the flagship
model through the full hollow chain, which is exactly where short-list items 3 and 4
live.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-hollow/src/drains.rs:3079` | No test drives the committed treant (or any multi-chamber corpus model) past `plan_drains`; the refill, the audit rescue and the `TrappedCavity` check are all downstream of the last assertion. | contained |
| medium | `crates/plinth-support/src/specialized.rs:3885` | `GatePreflightTelemetry` is never written — the parameter is bound `_telemetry` — so six assertions, including a test named for allocation-free refusal, compare 0 against 0. | trivial |
| medium | `crates/plinth-pipeline/src/lib.rs:2027` | A test states that it must use the production layer plan and then guarantees it by copying the derivation, magic numbers included. | trivial |
| medium | `crates/plinth-decimate/tests/execution_budget_refusal.rs:77` | The malformed-budget oracle reproduces production's three predicates and their precedence. | contained |
| medium | `rust/geom_kernel/tests/native_api.rs:889` | The sample-budget test recomputes the production `min(cap*sub, 3*sub + ceil(seg/step))` formula rather than pinning a reviewed expected value. | contained |
| medium | `crates/plinth-hollow/src/lib.rs:4868` and `crates/plinth-decimate/tests/local_minis_repair.rs:79` | Two named corpus regressions are guarded only by `#[ignore]`d tests reading local STLs that are not in git, so a clean checkout and the normal suite cannot exercise them. | substantial |
| low | `crates/plinth-support/src/cco.rs:7770` | The digest-equivalence helper's second assertion is a tautology given its first; its failure message promises a full-recompute check that is not performed. | trivial |
| low | `crates/plinth-support/src/rewire.rs:1509` | Five assertions restate constraints the constructor on the line above already refuses to violate. | trivial |
| low | `crates/plinth-hollow/src/drilling.rs:1121` | `qualifies_audit_drain`'s `added` parameter is always 0 in production, so the "three drain cap" test covers a dead limb while the cap that binds has no boundary test. | trivial |
| low | `crates/plinth-pipeline/src/lib.rs:1224` | A test named for thread settings discards `[1, 4, 16]` with `|_|` and asserts three identical constructions are equal. | trivial |
| low | `crates/plinth-layers/tests/registry_pin.rs:30` | The test claiming to pin the slice quantum to the stitch key pins it to a literal; the kernel constant it should compare against is public and already a dependency. | trivial |
| low | `crates/plinth-pipeline/src/lib.rs:2008` | A 400-line fixture whose oracle is error-message string equality, re-recorded eight times, once because a member key moved from slot 40 to slot 32. | contained |

### 3.9 Errors and diagnostics

Error *design* is good — typed variants carrying measurements, `thiserror` composition,
refusals that name what they measured. The defect is selective destruction at crate
boundaries: `map_err(|_| ...)` collapses distinct causes onto one variant, and in three
places the surviving variant misstates what happened. The worst is in `plinth-emit`,
where a structural verification failure on bytes the crate just generated is reported as
`cannot create ChituBox target {path}` — naming a path that was not even the file being
written.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-emit/src/lib.rs:81` | Staging save, rename, reopen and metadata failures all collapse into `TargetUnavailable`/`DiskAuditRejected` with no `#[source]`; `EmitError` derives `Clone + PartialEq`, so adding sources means relaxing those derives. | contained |
| medium | `crates/plinth-support/src/runner.rs:969` | Nine `map_err(\|_\| AuthorityMismatch)` / `ResourceLimit` sites on the verification and measurement paths destroy the cause — the exact defect this file's own `SupportRunError::Failed` doc says was fixed. | contained |
| medium | `crates/plinth-support/src/perception.rs:1203` | A registry-lookup failure is reported as `GenericPerceptionError::Topology`, and mesh/kernel/point-index/slice errors are stringified so no source chain survives. | substantial |
| medium | `crates/plinth-pipeline/src/gates.rs:448` | The documented "a refused model leaves no path on disk" invariant is enforced by `let _ = std::fs::remove_file(...)`, while `clear_stale_artifact` treats the same failure as fatal. | trivial |
| medium | `crates/plinth-pipeline/src/prepare.rs:1923` | The sealed repair ledger is written before the gate, and its receipt (including `write_error`) is dropped on every refusal path. | contained |
| medium | `crates/plinth-support/src/specialized.rs:1465` | `DuplicateChildPhysicalSeat` carries a candidate id in a field named `contact_id`, and `UnsatisfiedDemand` carries two different index namespaces under one message. | trivial |
| medium | `rust/geom_kernel/src/par.rs:42` | `PLINTH_THREADS` is a documented runtime contract with no validation, and pool construction failure hits `.expect("failed to build rayon pool")` in library code, holding the pool mutex. | substantial |
| low | `crates/plinth-pipeline/src/coverage_evidence.rs:354` | Serialization failures become empty recovered patches or omitted report fields rather than a recorded `write_error`. | contained |
| low | `crates/plinth-hollow/src/lib.rs:3854` | `IneligibleSavings` says "the largest cavity" for a value summed over every chamber, and `IneligibleBelowBulk` bakes the constant's current value into the text while carrying neither measurement. | trivial |

### 3.10 Naming and API surface

One rule accounts for most of this theme. "Floor" is used for "smallest allowed value"
in nine modules across five crates, including public `plinth-core` accessors, a
serialized enum variant, an owner-ratified constant name, and an emitted census string —
in a codebase where "floor" also means the bottom of a printed part and the build plate.
The ruling is quoted verbatim *inside* the codebase, in `HOLLOW_WALL_MIN_MM`'s own
registry note, and was applied to that one constant and nothing else. The single worst
instance is `patch_area_floor_mm2`, which is not even a minimum: it is a censor
threshold, an upper bound on ignorable area, so the public name points the wrong way
across the comparison.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| high | `crates/plinth-hollow/src/drilling.rs:418` | The shipped hollow report collapses every chamber into one `CavityRecord` with `cavity_id: 1` and stamps every drain with the same id, discarding the per-lobe identity the planner computed. | contained |
| medium | `crates/plinth-support/src/coverage_measurement.rs:124` | `patch_area_floor_mm2` is a public accessor crossing into `plinth-verify` and `plinth-pipeline` for a value the runner immediately renames `censor_area_mm2`. | contained |
| medium | `crates/plinth-core/src/constants.rs:145` | The whole `SupportResourceCapPolicy` accessor family, plus registry ids (`SUPPORT_KNN_MAX_WORK_FLOOR`, `SPACING_FLOOR_MM`, `PRINT_FLOOR_CHORD_MM`), use "floor" for a minimum. | substantial |
| medium | `crates/plinth-support/src/contact.rs:210` | `floor_depth_mm` is the worst-placed instance: a seat depth, in the one context where "floor" also has its legitimate printed-part meaning. | contained |
| medium | `crates/plinth-support/src/runner.rs:183` | Same vocabulary through the runner's cap helpers. | contained |
| medium | `crates/plinth-orient/src/search.rs:275` | `BelowFaceFloor { faces, floor }` is a *serialized* enum variant, so the word is now in the persisted orientation-report schema. | contained |
| medium | `rust/geom_kernel/src/queries.rs:1908` | Radii parameters are bare `r0s`/`r1s`/`radii` with no unit suffix and no radius-versus-diameter statement, in signatures that suffix `eps_mm` two arguments later — at the boundary every support crate crosses with those numbers. | trivial |
| low | `crates/plinth-support/src/specialized.rs:60`, `crates/plinth-support/src/perception.rs:99`, `crates/plinth-support/src/cco.rs:491`, `crates/plinth-mesh/src/delicacy.rs:119`, `crates/plinth-pipeline/src/coverage_evidence.rs:298` | The same "floor" drift in five more modules; `cco.rs:491` renames `own_floor` to `own_minimum_mm` on the next line, which is the drift caught in the act. | contained |
| low | `crates/plinth-core/src/constants/placeholders.rs:179` and `crates/plinth-support/src/cco.rs:1` | "CCO" reaches the `plinth-core` crate root as a public type name and is expanded nowhere in 178,000 lines or the docs tree. One glossary line would fix it. | trivial |
| low | `crates/plinth-support/src/anchor.rs:1045` | `AnchorSeatDiscovery` and `AnchorFootValidation` are public with public accessors while every function that can produce one is crate-private. | contained |
| low | `crates/plinth-solid/src/lib.rs:204` | The most discoverable public entry (`SolidBoolean` trait) and `BooleanLimits::default()` are both unbounded, while every production caller uses the explicitly limited inherent methods. Plausible; the compatibility path is documented. | contained |
| low | `rust/geom_kernel/src/lib.rs:22` | `V3` is exported but `mod vec3`'s operations are not, which is why three `plinth-support` modules re-implement them. Plausible as a parity risk; certain as duplication pressure. | trivial |
| low | `crates/plinth-core/src/printer.rs:173` | `pixel_pitch_x_um` and `pixel_pitch_y_um` are adjacent same-typed positional parameters with no transposition protection. Plausible. | contained |
| low | `crates/plinth-cli/src/lib.rs:29` | Three required flags and no positional argument, against the recorded target `plinth prepare <folder> --output <printer>`. | contained |
| low | `crates/plinth-support/src/specialized.rs:1` | "specialized" only has meaning as the other half of a contrast with "generic"; read alone it does not say what is specialized. Plausible, and a judgement call. | substantial |

### 3.11 Documentation drift

Comments in this codebase are unusually substantial and unusually honest — superseded
claims are retracted in place rather than deleted. That discipline has one failure mode:
an append-only journal inside a doc comment, where the current state can only be
recovered by reading four dated retractions in order and one of the intermediate claims
reads as the opposite of the truth. Elsewhere the drift is ordinary: crate docs
describing an architecture two refactors old, and citations into files a docs
consolidation deleted.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-support/src/lib.rs:11` | The crate doc says "A future CCO controller is the authority… There is no public `run` stub" 160 lines above `pub use runner::run_support`, with an 11,094-line `cco.rs` already live. | contained |
| medium | `rust/geom_kernel/src/consts.rs:1` | The header promises each tolerance is linked to its `geometry_exact.py` source line; only a `.pyc` remains, so nine outcome-affecting numbers have no readable provenance and no registry entry. | contained |
| medium | `crates/plinth-support/src/specialized.rs:141` | 170 lines of doc on a 6-line `const fn`, four paragraphs of which are explicitly retracted by later ones, including "the treant still refuses at this gate" — asserted, then retracted 60 lines down. | contained |
| medium | `crates/plinth-core/src/constants/evidence.rs:187` and `:301` | Two citations into files deleted by the docs retarget, missed by the commit that repointed ~20 other files. | trivial |
| low | `crates/plinth-hollow/src/lib.rs:404` | The traversal cost of a re-selection pass is stated as 13 here and 12 in `well_composed.rs`; `flow.rs` enumerates 11 and is right, so the two public-facing numbers are off by one. | trivial |
| low | `crates/plinth-core/src/constants.rs:352` | The registry's own publication doc and its `Vec::with_capacity` hint say 287 entries; the test-verified count is 306. | trivial |
| low | `docs/architecture.md:148` | "Every public query takes a slice and returns a slice" is false for `knn_single_source_planned_into` among others, and overstating the rule masks the genuine gaps. | trivial |

### 3.12 Code smells

This theme is short partly because the one systematic search that ran came back mostly
empty. A dedicated lane did triage all 3,817 pedantic and nursery clippy warnings into
categories (`census-clippy.md`), and the result is that the great majority are style
noise rather than defect signal: the 534 `mul_add` suggestions, the ~519 documentation
lints, the ~230 `pub(crate)`-in-private-module lints and the ~250 assorted style lints
are all noise; the 28 "redundant clone" and 36 "passed by value, not consumed" hits are
every one of them inside `mod tests`; the 19 "manual midpoint which can overflow" hits
are all `(a+b)/2.0` on millimetre-scale coordinates, where overflow is physically
unreachable; and all ten "this sequence of operators looks suspiciously like a bug"
warnings were read individually and all ten are false positives (determinant and
frustum-volume formulas, and chained boolean logic). Two classes carry real signal and
fed themes 3.4 and 3.5: the 586 exact float comparisons and the ~600 lossy casts. That
lane also proposed a `[lints.clippy]` block for the workspace manifest that would leave
only the signal classes at `warn`, which is worth adopting — the project currently
carries no lint configuration at all, so this material is invisible in normal builds.

What the search did *not* cover is the rest of the smell surface: no lane took a
structured pass at function cohesion, boolean and `Option` state machines that permit
invalid combinations, the scope of `allow` attributes, or the examples as maintained
tooling. What follows is that residue — what individual lanes tripped over while reading
for something else. Read it as a starting point.

The one worth acting on is the name: `HostCache` is documented throughout as a kd-tree,
with a `KDTREE_REBUILD_EVERY` constant, and there is no kd-tree anywhere in
`plinth-support` — which is what makes the 25-terminal staleness in the `HostCache`
finding (theme 3.3) read as benign amortization rather than a correctness hazard.

| Sev | Location | Finding | Effort |
|---|---|---|---|
| medium | `crates/plinth-support/src/cco.rs:40` | The "host kd-tree cache" is a linearly scanned `Vec`; no kd-tree exists in the crate, and the cache removes no term from the cost. | contained |
| low | `crates/plinth-support/src/runner.rs:3748` | A comment says the island index is reused from the preflight; the code rebuilds it. | trivial |
| low | `crates/plinth-support/src/forest.rs:1281` | Subtree walks recurse without a depth bound, and `validate()` runs one per candidate move, so a deep chain aborts on stack overflow instead of returning `ForestError`. | contained |
| low | `crates/plinth-pipeline/src/coverage_evidence.rs:91` | `COVERAGE_RENDER_MAX_FACES`' stated size guarantee names the sidecar, which contains only filenames, and is roughly 10x low for the SVGs it actually bounds. | trivial |
| low | `rust/geom_kernel/src/bvh.rs:297` | The index-median fallback sorts a range the comment two lines above says is already sorted, and reserves `2 * count` for a partition totalling `count`. | trivial |
| low | `rust/geom_kernel/src/queries.rs:762` | Penetration scratch accounting bills a stand-in tuple instead of the real `CapsuleRay`; same size today by coincidence, in a routine whose stated purpose is that the two charges cannot drift. | trivial |
| low | `rust/chitubox_io/tests/common/mod.rs:26` | `optional_treant_dir` falls back to a hard-coded personal absolute path when `CHITUBOX_CORPUS_DIR` is unset. This is untidy, not a doctrine-3 defect: the fallback serves one optional oversized specimen, the module documents that, `required_fixture_path` asserts the name is not that specimen and fails by path when a committed fixture is missing, and the eight required round-trip fixtures are in the repository. A clean checkout runs the suite; it skips one file. | trivial |

---

## 4. What is good

**`rust/geom_kernel`'s resource accounting is the strongest in the workspace.**
Every batch entry point preflights its own memory, allocations go through
`try_reserve_exact` with named failures, there is no `unsafe` and no FFI, and
parallelism is confined to a single sanctioned index-parallel map-collect under an
explicit pool. The determinism story is enforced rather than asserted: `winding.rs`,
`slicing.rs`, `segment_batch.rs` and `native_api.rs` all carry cross-process
thread-sweep tests. The penetration path's count-before-charge-before-allocate
discipline, with a `CensusDrift` refusal when the counting walk and the executing walk
disagree, is exactly the right shape for a resource-bounded kernel.
`project_allowed_faces_batch` is a model of index reuse — which is why it is a shame
production takes the other path. Note the contrast rather than the accolade: this is the
crate that also holds short-list item 2, the review's most direct bug. Strong resource
discipline did not catch a wrong index.

**`rust/chitubox_io` is the strongest crate in the workspace on input handling.** Every
offset and length in the hot parse path is `checked_*` through a small codec module,
malformed-input tests exist for the exact panic classes that matter (offsets inside the
index quad, truncated bodies, aliased blobs, NaN/zero-triangle STL), and the writer keeps
an independent structural re-parse plus a from-scratch interval audit that never trusts
the parser's own code path. Round-trip fidelity is enforced both by a `CleanRoundtripDrift`
tripwire inside `to_bytes` and by byte-identical corpus regression tests.
`#[forbid(unsafe_code)]` holds.

**The M0 constants registry is a real engineering artifact, not a ledger.** 306
declarations, each with a dated evidence source or an explicit "no derivation is
claimed"; a four-category provenance sum type with a sealed `AcceptSafe` trait and a
compile-fail test proving `Placeholder` cannot satisfy it; deny-unknown-fields wire
formats. One lane re-computed several derivations by hand
(`next_power_of_two(2 x 696) = 2048`, `floor(25.00/2) = 12`, the `0.5 * sqrt(n)`
drain-lattice argument) and they check out. The problem is entirely at the boundary
between the registry and its consumers; the registry itself is right.

**`plinth-orient` is the cleanest crate on the physics-first and batch-first doctrines.**
Every weight and ladder count is registered, scoring is genuinely batch-first (one tiled
N x U product per candidate tile under rayon), and the search space is stated and
*enforced* as S² — a machine-checked test proves no objective term is sensitive to
z-spin. The lane looked specifically for dead Y-position optimization code, per the
settled front-to-back ruling, and found none. It does not clear every doctrine: the
serialized `BelowFaceFloor { faces, floor }` variant (`search.rs:275`) puts the
minimum-versus-floor drift into a persisted schema, and `score.rs:728` builds a fresh
rayon pool per scoring tier, three per model.

**`plinth-emit` and `plinth-decimate` have strong local invariants.** Emit writes to
staging, renames, then re-opens the file from disk and audits it against the values it
thought it wrote, with staging and target both removed on every failure path — the "no
file left behind on a refusal" invariant is intact, subject to the directory-sync gap in
theme 3.3. Decimate is a thin, disciplined adapter around meshopt with no
`#[allow(dead_code)]` anywhere and no production `unwrap` that is not proven safe at its
call site; the one non-meshopt algorithm in it is a small, exhaustively documented
topological repair for a specific meshoptimizer defect. Neither crate is clean overall,
and this review should not be read as saying so: emit collapses four distinct error
sources onto two variants and dispatches raft geometry on exact `f64` equality, and
decimate refuses reductions on an admittedly non-derivable face-count threshold and
rebuilds the unchanged mesh's indices on every probe.

**Error design in the support engine is better than its size predicts.** The 5,791
production lines of `cco.rs` contain zero `unwrap`, zero `expect` and zero `panic!`, with
typed, value-carrying `CcoError` variants throughout — the census's "356 unwrap in
cco.rs" is entirely the test module. `plinth-pipeline`'s per-model dispositions genuinely
contain one bad model without aborting the folder, and both substitution seams are
classified by exhaustive matches on typed errors with no `_` arm.

**Nobody calls the kernel one item at a time.** One lane checked all 83 non-test call
sites of the kernel's slice-taking APIs (`query`, `project`, `inside`, `cast_rays`,
`segment_clearance`, `penetration_intervals_batch_*`, `slice_z_batch`) and found zero
cases of a one-element slice in a loop. The FU-8b/8d/8f rewrites in `specialized.rs` and
`perception.rs` are real batch conversions verified against retained reference
implementations rather than restatements. That is the narrow claim and it holds. The
broader one does not: theme 3.5 lists a scalar per-point classifier at a batch entry
point, a serial voxel rasterizer, whole-forest clones per candidate, and superlinear
coverage work in an uncapped loop. The kernel boundary is batch-first; the callers above
it are not consistently so.

**Two specific pieces of design are worth naming.** The typestate seam in
`runner.rs` hands the context back inside every `Err`, so a refusal does not kill the
model — that is what makes the repair loop possible at all. And `coverage_closure.rs`
keeps one definition of the censor, the objective and the accept clauses, with the
anti-gaming clause carrying its own revert proof; it is the best-tested thing in the
support crate, and it is the pattern the rest of the engine should copy.

**Where comments record history, they usually record it honestly.** Superseded
measurements are retracted in place rather than quietly deleted, rejected alternatives
are recorded with reasons, and known gaps are disclosed instead of hidden
(`NotYetPorted`/`pending` exists precisely to make an omission a reported value). Theme
3.11's complaint is mostly about the *form* that honesty has taken.

The statement does not generalize, and themes 3.6 and 3.11 are the counter-evidence:
eleven module-level allows say a consumer has not landed when it has, a kernel method
cites a PyO3 surface that does not exist in the crate, `docs/architecture.md` presents an
unused gate and a consumerless crate as live, and one `const fn` carries four paragraphs
of doc that later paragraphs retract. Retrospective comments are candid; comments about
the *current* state of the code drift like anyone else's.

---

## 5. Recommended sequence

### Fix now

These are cheap, unblock other work, or stop a valid change from breaking the build.

1. **`queries.rs:1827` and `:2142` — replace the lookup with `tid`** (short list 2). One
   token each. Then extend the subset test so it can never regress. Do this first: it is
   a live panic and it costs an hour.
2. **Add `pending_gates` to `ModelPreparationOutcome::Artifact` and print it** (short
   list 1, disclosure half). Also fix the guard test at `prepare.rs:2715` so it sets a
   non-empty value. This does not make the engine safer, but it stops the run report
   claiming more than it knows, which is a precondition for reasoning about everything
   else.
3. **Read the drain keepout ladder in the validator and the audit-hole constructor**
   (short list 9). Until this lands, a legitimate registry edit turns every hollow into a
   solid, so it blocks any calibration work on `HOLLOW_EXTRA_DRAIN_POLICY`.
4. **Replay `validate_regular` through `select_contacts_baseline`** (short list 5), then
   delete or mode-gate the legacy pre-routing repair loop and correct the
   `coverage_closure` module doc that hides it.
5. **Delete the stale `#[allow(dead_code)]` attributes across `plinth-support`**
   (short list 10, `transient.rs`, `legalization.rs`, `coverage_closure.rs`, and the
   eleven module-level ones at `lib.rs:33-142`), then `cargo check -p plinth-support` and
   re-add only per-item allows with true reasons. **This is a prerequisite for anything
   structural**: while eleven modules are blanket-suppressed, the compiler's reachability
   signal is off across ~57,000 lines, so neither a crate split nor a dead-code cleanup
   can be verified.
6. **Iterate the trapped-cavity refill to a fixpoint** (short list 4) — but cache the
   member list and z-order first (`drilling.rs:1642`, theme 3.5), because iterating
   multiplies that whole-grid scan directly. Add the cascading-pocket test.

### Fix before the next feature

7. **Drain basins** (short list 3). This is the substantial one and it is the product's
   actual purpose. Do it *after* item 6: the fixpoint refill is what makes a basin-aware
   plan safe to evaluate, and the current single-pass refill would mask whether the new
   lobes drain. Expect to move the committed drain-count assertions; pin
   `refilled_mm3` and `refilled_pockets` at the same time, since nothing asserts them
   today and they are the number that actually matters.
8. **The island-birth predicate** (short list 7) together with the overlap sampling fix
   (theme 3.4) and the exterior-face filter (theme 3.2, `perception.rs:2557`). All three
   move frozen slice digests, so do them as one change and re-pin once, with the recorded
   rationale the reach-criterion note already models. Doing them separately means three
   digest migrations.
9. **The `AcceptSafe` boundary** (short list 8). Reclassify the two hollow gates as
   `Measured` with an owner decision — that part is contained — then take
   `&impl AcceptSafe<f64>` on the coverage request, the tip load law and the clearance
   predicate. **This conflicts with item 10 if done in the wrong order**: fixing the tip
   law changes which values those signatures take, so land the tip-law decision first.
10. **Decide the tip sizing law** (short list 6). Either derive from `X_ALLOWED` or
    document that the shipped law is the legacy stress form and its agreement is
    coincidental. Either way, move the one-sided anchor assertion out of `#[cfg(test)]`
    into the constructor. This is a decision more than a code change; make it explicitly.
11. **Port `Load` and `BallBreach` as real gates** (short list 1, second half). Both
    already have producer-side implementations that record their measurements, so the
    port is predicate-only against `SupportVerificationSnapshot`. This is the payoff item
    for item 2.
12. **The per-trial measurement cost** (short list 12) and the triage classifier
    (theme 3.5, `triage.rs:1015`). Both are contained, both are on paths that run on
    every model, and the fix patterns (`IslandFaceIndex`, `NativeSurface` + `cast_rays`)
    already exist in the same crate.
13. **Register the geom_kernel tolerance set and build the thin-wall fixture** (short
    list 11). The registration is contained and independently worth doing. The fixture
    comes before the fix: it is what turns the near-wall breach from a mechanism read off
    the code into a measured yes or no, and it is also the test that would keep a
    same-face replacement honest.
14. **Sync the directory after both renames** (theme 3.3). Open the parent directory and
    `sync_all` it after `rename` in `chitubox_io::save` and in `plinth-emit::write`. Two
    calls; it is the difference between "the bytes are on disk" and "the name is on
    disk". Do it with item 13 or on its own.

### Standing cleanup backlog

Batched, not urgent, and best done as sweeps so each touches the tree once.

- **The vocabulary sweep** (theme 3.10). One pass renaming "floor" to "minimum" across
  `plinth-core`'s accessors, the registry ids, `patch_area_floor_mm2` (which should
  become `censor_area_mm2`), the serialized `BelowFaceFloor` variant, and the census
  output keys. **Order matters against `constants-doctrine-01`**: make the production
  constructors read their caps from the registry *first*, then rename the registry ids
  once — otherwise you rename two copies of every id.
- **The duplication consolidation.** Publish `geom_kernel::vec3` and the Möller
  primitive; move the 2D contour primitives (`signed_area`, `point_in_polygon`,
  `segment_distance`, `point_segment_distance`) to one home with one named degeneracy
  tolerance; close `docs/status.md` #50 by porting the symmetric origin band into
  `plinth-metrics` and adding a cross-crate differential test over grazing fixtures. The
  differential test is the part that stops this recurring. Include the frustum-volume
  law in this sweep: one function, taking a length and two radii, with one named
  cubic-millimetres-per-millilitre divisor, and — if the NumPy-parity length is genuinely
  required — one explicit parameter selecting it, rather than three functions that differ
  by accident.
- **Verify the vendored Manifold checksums, or delete the file.** `FILES.sha256` is
  either a supply-chain control, in which case the build script should read it and refuse
  a mismatch, or it is a comment. Today it is the second thing wearing the first thing's
  name.
- **The registry read-through** for the 34 support caps, the seven `joints.rs` literals,
  the cross-link duty factor and the `plinth-metrics` `12.0`, using the equality-pin
  pattern `crates/plinth-layers/tests/registry_pin.rs` already demonstrates where a full
  read is too invasive.
- **The test tautologies** (theme 3.8) — all trivial, all mechanical, and each one
  currently buys false confidence.
- **Docs drift** (theme 3.11) — the crate doc for `plinth-support`, the geom_kernel
  consts header, the two dead citations, the 287/306 count, and the
  `docs/architecture.md` batch-contract sentence.
- **Decide `plinth-metrics`' status before spending anything on it** (theme 3.1). It has
  no production consumer, so its seven findings — including the hard-typed `12.0` and the
  unsound interval type — are latent by construction. Either restore a `plinth score`
  subcommand, which gives the workspace back the independent post-write audit the
  architecture claims, or move the crate out of the default members and say in
  `docs/architecture.md` that it is an offline tool. Do not fix its internals until that
  is settled.
- **The two `#[ignore]`d corpus regressions** (theme 3.8) need committed, minimized
  fixtures. This is the "works on my machine" doctrine applied to the test suite.
- **`arch-01`/`arch-11` (the stage handoff types) is real but should wait.** It is the
  right long-term shape, it depends on the dead-code labels being honest first, and none
  of the correctness work above is blocked by it.

---

## 6. Method and limits

**How this was done.** Twenty-five reading lanes each took a scoped set of files and read
them, mostly in full: the lane summaries record what each opened and what it did not. The
lanes produced 239 raw findings; deduplication left 224; 44 were verified individually
and 180 in batches, by a cross-family verifier that re-read the cited code. 203 survived;
21 were refuted and dropped. A later targeted pass over the native build subtree, the
kernel's compatibility surface, the durability of both rename paths and the frustum
copies added four more, each confirmed by reading the cited lines. Five findings at
critical or high severity were verified in a batch rather than individually because the
individual-verification budget was exhausted — those are not weaker findings, but they
received less scrutiny than the 44, and that is the review's own limit, not the code's. Where a verifier corrected a claim,
the corrected version is what appears above. Several corrections were material and are
worth stating: the 0.6 mm birth-link radius **is** registered and documented (the defect
is that live code duplicates and bypasses the entry, not that provenance is missing); the
`transit_measurement` module **is** under an explicit reasoned dead-code allow rather than
silently dead; the treant hollow regression is a certain code path but an **inferred**
attribution; and `constants-doctrine-12`'s count of `.diagnostic()` calls is 414, not the
273 originally claimed.

**Convergence.** Several defects were found independently by lanes with different reading
assignments, which raises confidence in them: the 300 mm printable-envelope literal
(hollow and constants lanes); the drilling keepout-ladder duplication (hollow and
constants); the triage containment scanner (mesh and architecture); `exterior_face_mask`'s
probe offsets (geom_kernel and numerics); the `plinth-metrics` `12.0` (metrics and
constants); the thread-pool churn and the decimation index rebuilds (three lanes each);
the `Placeholder`-on-acceptance-paths boundary (core, constants, and the coverage lane,
from three different entry points); the pre-write gate facade (architecture, pipeline, and
the verify lane); and the floor/minimum vocabulary drift, which nine lanes reported
independently.

**What this review did not cover.** Every Rust library crate in the workspace had an
owning lane — the 13 `plinth-*` crates, `rust/geom_kernel` and `rust/chitubox_io` — and
no production `src/*.rs` subtree fell outside all of them. That is the honest ceiling of
the completeness claim. Assignment is not the same as reading, and several categories of
workspace code were assigned to nobody.

- **The native half of `plinth-solid` fell outside every lane's scope and was reviewed
  separately afterwards.** The combined lane scoped that crate at roughly 1,645 lines,
  which is its Rust portion; `manifold_bridge.cc` (267 lines), `manifold_bridge.h` (30)
  and `build.rs` (63) were read in a follow-up pass and are no longer a gap. The result
  belongs in section 4 rather than here: the bridge is one of the better-defended pieces
  of code in the workspace. Both boundary-crossing functions are `noexcept` and route
  through one `execute` wrapper whose catch ladder ends in `catch (...)`, so no exception
  can cross into Rust; fifteen `static_assert`s pin the scalar format, the index width and
  every one of Manifold's fifteen error-enum values, so an upstream ABI change fails the
  build rather than corrupting a result; both output limits are enforced *before* the
  result is copied out; the serial backend and the exclusion of the cross-section code are
  enforced by `#error` rather than by convention; and malformed offset arrays and
  out-of-range triangle indices are both rejected with distinct typed codes (the latter
  via Manifold's own `Status()`, which is checked per input mesh). Two small things are
  worth noting and neither is a defect: the overflow guard at `manifold_bridge.cc:178`
  covers `num_vertices * 3` but the loop two lines below multiplies by `mesh.numProp`,
  which is only bounded below — unreachable today because the output stride is always 3,
  but the guard and the multiply should match; and `get_mesh_gl64_calls`
  (`manifold_bridge.cc:70`) is a test-observability counter incremented on every
  production export. The 18 vendored Manifold translation units remain deliberately out
  of scope as third-party code, which is defensible — though see the `FILES.sha256`
  finding: that exclusion rests on a per-file hash manifest that exists in the tree and
  that nothing verifies. `build.rs:60` names it only as a rebuild trigger.
- **The example programs were largely unassigned.** There are 4,556 lines under
  `*/examples/`, including `crates/plinth-pipeline/examples/fu61_bore_penetration.rs`
  (1,090 lines), `crates/plinth-hollow/examples/raster_delta.rs` (836),
  `crates/plinth-pipeline/examples/shared/first_artifact_chain.rs` (417),
  `crates/plinth-mesh/examples/corpus_gate.rs` (324) and
  `rust/chitubox_io/examples/regen_corpus.rs` (302). A few were hit incidentally by
  duplication and documentation searches; none had a full-read owner. These are not
  demos — they are evidence generators, corpus gates and the workflow articles the
  project reasons from, so a defect in one corrupts a ruling rather than a screenshot.
- **Nothing was executed.** No lane ran the test suite, a build, an example, a corpus
  gate, a mutation run, a profiler, or the pipeline. Every efficiency finding is a
  complexity or allocation claim derived from reading the loop, and no wall-clock figure
  appears anywhere. The per-model magnitudes (429,120 faces, 482 contacts, 5.3M voxel
  cells) come from figures already recorded in `corpus/baseline/` and in the code's own
  comments, not from measurement. This weakens four classes of claim in particular:
  reachability, performance magnitude and ordering, end-to-end hollowing behaviour, and
  anything about filesystem behaviour on a platform other than this one.
- **The corpus is not present.** The large STLs are local and not in git, so the two
  `#[ignore]`d regression harnesses could not be exercised, and the hollow lane could not
  run the release-gated treant tests. Short-list item 3's attribution to the treant is
  therefore inference from the code plus the committed assertions, not an observed
  refusal.
- **Test modules were sampled, not audited, and the test lane's scope said otherwise.**
  That lane's stated scope was all integration tests and all inline test modules; what it
  delivered was a sample. The workspace has 1,585 test functions and about 32,500 lines
  under `tests/` directories, plus inline `#[cfg(test)]` modules — the support crate alone
  carries roughly 26,000 lines of tests. Lanes enumerated test names and read a dozen or
  two each, chosen by relevance to their findings. Sampling a corpus that size is a
  reasonable choice; calling the result an audit would not be. A weak test outside those
  samples would not appear here, and no lane systematically separated independent oracles
  from restatements of production logic. `crates/plinth-support/tests/` was not read by
  any lane. One lane's mechanical `#[cfg(test)]` mask means production code appearing
  textually *after* an inline test module in the same file was excluded from two sweeps
  (the comparison-literal sweep and the float-equality classification), and `cco.rs` and
  `specialized.rs` are both large enough for that to matter.
- **Code smells were only partly searched for.** The 3,817 pedantic and nursery clippy
  warnings *were* triaged, by class, with the priority classes sampled by reading the
  code at the sites — see theme 3.12 and `census-clippy.md`. What no lane took was a
  structured pass at function cohesion, boolean and `Option` state machines that allow
  invalid combinations, the scope of `allow` attributes, the examples as maintained
  tooling, or cross-platform filesystem assumptions. Theme 3.12's shortness is now a
  fact about the code for the lint-detectable classes, and a fact about the search for
  the rest.
- **Some large files had exactly one real reader.** `plinth-decimate`'s internals, the
  `plinth-hollow` drain/drill bodies, and most of `plinth-support`'s modules were read by
  the lane that owned them but only grepped by the cross-cutting lanes, so the
  cross-cutting claims (architecture, constants, performance) over those files rest on
  import graphs and targeted line ranges rather than full reads. Where a finding in those
  areas is described as cross-checked, that means two lanes agreed on a cited line range,
  not that two people read the module.
- **Absence from this report is not evidence a file was clean.** Most of
  `rust/chitubox_io`'s source files are not cited anywhere above even though its parse and
  write design was read closely enough to praise. A file with no entry may have been read
  and found sound, read partially, or not read at all; only the gaps named in this section
  distinguish those cases.
- **Ten findings are marked PLAUSIBLE, not CONFIRMED** — the verifier could reproduce the
  mechanism but not a reachable consequence: `perf-08` (serial folder loop),
  `numerics-01` (the four Möller variants — the guards differ, but the metrics routine
  rescales its inputs so the determinant magnitudes are not directly comparable),
  `support-runner-14` (NaN sentinel), `perception-digest-prefix-15` (digest not
  prefix-free), `solid-unbounded-default`, `geom-kernel-12` (private `vec3`),
  `emit-raft-pad-bit-equality-coupling`, `api-naming-06` (transposable pitch arguments),
  `api-naming-07` and `support-cco-16` (naming judgements). Treat those as questions, not
  conclusions. Two short-list items are mixed: item 3 pairs a certain mechanism with an
  inferred attribution, and item 11 pairs a certain provenance gap with an unwitnessed
  breach. Both say so in place.
- **Three things verification could not settle.** Whether the drain-banding defect is the
  cause of the specific treant regression (only a full-chain run on that fixture will
  say); whether the eleven pending gates all have producer-side implementations ready to
  port — only `Load` and `BallBreach` were confirmed to have them; and whether
  `NEAR_WALL_SKIP` actually admits a foot ball through a thin feature, which needs a
  constructed geometry satisfying the surrounding precondition and was not built.
- **Severity is a judgement.** The rubric used was: critical = wrong printed parts or
  corrupt output; high = wrong behaviour in a reachable case, or architecture actively
  costing development time; medium = real but bounded; low = cleanliness. Nothing here
  was rated critical. Several findings that a lane submitted as high were lowered because
  the demonstrated consequence was a lost optimization or a refusal rather than a bad
  artifact; where that happened, the lower rating is what appears above. No efficiency
  finding is rated high, for the reason theme 3.5 states: without a measurement, "this
  loop is O(n²)" establishes a defect but not its cost.
