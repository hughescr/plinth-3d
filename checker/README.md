# Independent checker

This is a separate Cargo workspace. It does not import producer crates, meshes, hole lists,
inside labels or support plans. `../scripts/check-independence` checks that boundary.

## Hollow applicability and limits

The new geometric hollow search runs **only with `--declared-hollow`**. The declaration
sets applicability, not geometry or a drainage certificate. The library equivalent is
`Scene::with_declared_hollow()` or `report::build_posed_declared(..., true)`. The producer
passes the flag exactly when it hollowed the model in a `--hollow-fixture` review run.
Without the declaration H5 keeps its old topology-only test, H1/H2/H6 keep their outside
rules, and no hollow grid or inside-classification search runs.

For declared scenes, `hollow` reads the decoded, emitted f32 model and every native support
mesh. H1, H2, H5 and H6 reuse one bounded search per scene. The checker-specific
`limits.toml` supplies the 3 mm nominal clear diameter and 5 mm local mouth-clearance radius.
A `[hollow]` section in the `--limits` file must contain both `drain_min_mm` and
`mouth_clearance_mm`; unknown/malformed section headers and duplicate/malformed values
are errors. A file with no `[hollow]` section uses the documented 3 mm / 5 mm compatibility
defaults. A declared-hollow report then explicitly sets
`settings.hollow_limits_defaults_used: true` (and reports the effective numeric limits).
The producer's five-key limits file stays unchanged.

The effective-width rule is **3 mm clear emitted geometry**, not the approximately 2 mm
wet-resin gap described by the owner. A measured narrower mouth is an H5 violation by
itself, even if point routes remain open. A keep-out is local to each actual mouth, not a
strip to the plate. The bore cylinder spans the measured axial wall only; a lower leg or
other remote bounding-box extreme must not extend it through outside air.

## Bounded hollow measurements

The fixed search uses 0.10 mm cells, triangle/cell separating-axis tests and batched column
rays. It retains definite interior components before width tests. Surface cells stay unknown.
Both optimistic and strict directed floods account for all support parts, raft and plate;
hanging gravity is +Z. Definite support-filled cells obstruct flow but are not resin cells
that must drain. Unknown support boundary cells stay open only in the optimistic failure
search; they block a strict proof. The report gives component counts, mouth coordinates,
swept axial inscribed diameters, boundary-volume uncertainty, failed directed routes and
support intersections. Every complete support mesh is tested against each detected throat
and local mouth ball, including containment as well as surface crossings. Zero intersections
is not a drainage certificate.

The detector is bounded to straight axial throats along X, Y or Z, with a wider chamber
behind them and resolvable axial wall triangles. Separate connected openings on one extreme
layer are measured separately, never averaged into a wall-centred mouth. Zero mouths,
a straight through-bore without a wider chamber, unresolved throat ends, or an opening
away from the chamber's extreme layer produce an explicit unverified discovery result.
For declared scenes, genus zero does not exempt a one-hole hollow. Declared scenes never
receive H5 not applicable: an exactly convex body is instead unverified with an explicit
"declared hollow but no cavity was found" inconsistency. Sealed inward shells still fail.

**H5 Pass and the inside exemption are currently unreachable by construction.** Every
surface triangle leaves unresolved grid cells; a contact-bearing part always touches the
surface. This is deliberate fail-closed behaviour, not implemented relaxation of FR-19 or
FR-23. H1/H2/H6 therefore retain their outside rules, including for visibly interior contacts.
`wholly_inside` would also require the complete part's bounding box in definite void cells
of one proved drained chamber; vertex-only containment or a native foot role is not proof.
Positive drainage/inside certification is future work, not a completed hollow checker.

## H3 model-standing feet

When feet touch the model, H3 tries to slice its material into convex tiles without filling
holes. Those tiles and support sections enter the same print-order root graph. Model material
is not a source: it needs rooted support or earlier rooted material. Same-layer cycles cannot
root themselves. Foot starts and model vertex-height events are included in the plane batch.
A missing foot section at its first thick height is explicitly unverified.

If the model is open, slicing is unresolved, or a scene-local model-slicing budget is exceeded,
H3 falls back to the old support-only walk: model-standing feet and dependent parts remain
unverified/reliant. Floating columns and measurable unsupported islands still produce located
violations; model uncertainty must not discard them. The report retains unverified items even
when a separate violation determines H3. Rooting grants no load capacity and waives no H5/H9
requirement. The load audit has not been replaced by an inside load ledger.

## Known blind spots and pending decisions

- An **undeclared hollow receives the old topology-only H5**, as before this change. A
  one-hole hollow can therefore be called not applicable. This applicability blind spot must
  be made sound before hollow files may become print candidates.
- Off-extreme openings, oblique/non-axial throats, straight through-bores without wider
  chambers, and unresolved chamber components remain unverified. The detector does not
  certify a complete general mouth inventory or the absence of undeclared pockets.
- Fixed surface uncertainty, positive general drainage certification and 3 mm bulk
  connecting-route certification remain unresolved. No interior exemption is granted.
- Independent FR-17a exposure-prefix checks, H7 permitted hollow changes, native hollow
  consumer samples and physical resin-flow/washing evidence are separate missing proofs.
- The 5 mm mouth keep-out ball remains an **H5 violation**, even for a thin rim contact.
  Its conflict with rim contacts permitted by FR-18 is an open owner decision; this checker
  does not silently resolve it or reduce the ball.

## Bounds and checks

The hollow search caps storage at 64 million cells and raster work at 120 million
triangle/cell tests. Dense fields, component lists and flood queues are bounded below 2 GiB.
No coarser fallback can produce a pass. Model slices cap segments at 8,192, material tiles
at 16,384, strip/segment tests at four million per plane and stored material/support sections
at two million. A refusal falls back only for model slicing, never to positive hollow proof.

Run `cargo test -p plinth-check`, `cargo clippy -p plinth-check --all-targets -- -D warnings`
and `cargo fmt --all -- --check` here, then `../scripts/check-independence` from this folder.
Generated hollow controls in `src/hollow/tests.rs` use no sibling-path fixture reads. They
exercise single/two mouths, two openings on one layer, off-extreme and straight-through
refusals, 2 mm versus 3 mm widths, recessed mouths with a lower leg, complete ball/brace/raft
intrusions, the unclear-topology intrusion branch, scene-local settings, blocked routes and
unchanged undeclared behaviour. H3 controls force each scene-local slicing budget and pair
an uncertain model-standing foot with a separately floating column.
