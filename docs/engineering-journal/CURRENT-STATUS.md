# Rust migration current status

Durable handoff recorded 2026-08-03; updated after the 2026-08-04 integration pass, and
extended 2026-08-06 with the FU-8b resolution and owner ruling R6. This
file distinguishes integrated work from reviewed-but-branch-only work and from work still
awaiting review. A review result is not evidence that a commit has been merged.

## State of the system (2026-08-12)

*Summary added 2026-08-12 (orchestrator-sanctioned addition; see decisions journal).*

Slimmed 2026-08-12. Full dated history on and before 2026-08-09 (and the closed FU-8/8c-8g,
FU-10, FU-25/26, FU-31, FU-32 chains) moved verbatim to docs/engineering-journal/archive/.
Reviewed != merged still applies; see the preamble above.

**Where things stand:** the pipeline runs end to end (`plinth prepare`) with dense
orientation search, an automatic mesh-repair pre-pass, and a hollow stage that as of
2026-08-13 aims at the requested wall as a TARGET and guarantees a measured 2.0 mm minimum
(`HOLLOW_WALL_MIN_MM`; owner ruling, [the dated section](#wall-target-and-minimum-2026-08-13)
— it shipped `>=` the requested wall by construction from 2026-08-12 to 2026-08-13, which cost
the Weeping Treant more than half its cavity), with
a bounded repair pass (FU-54), the requested/realized wall persisted to the report
(FU-37 landed), flow-based cavity connectivity (FU-47 landed) and a coverage-closure
controller that carries per-refusal evidence (FU-56/FU-57 landed). **All twelve rows
worked in the 2026-08-12 ruled batch are on `develop`**, and the 37-mini rerun against
that tip emits **8 artifacts where the baseline emitted 1**, with **zero**
`PreWriteGate::UncoveredIslands` refusals; see
[the batch section](#ruled-batch-landed-2026-08-12) and
[`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md).

Ten owner rulings from the 2026-08-12 batch are recorded in
[`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md).
**Nine of the ten now have their implementation on `develop`** — FU-36, FU-37, FU-38,
FU-47, FU-50, FU-51, FU-54, FU-55 and FU-57 — and the tenth, **FU-46, required none**: the
ruling put deletion out of scope, so only `repair.rs`'s comments needed correcting
(`4d5de5e`), and that row stays open solely as the refusal class it names.
**Landed is not closed.** FU-56 and FU-57 built the mechanism their rulings named and
**19 of 37 models still cannot be supported**; FU-47 and FU-51 each carry questions the
landing did not answer, recorded in the batch record rather than guessed at.

**The live blocker set**, from the 37-mini rerun and residual-refusal analysis
(2026-08-12): every one of the 8 artifacts is `Solid { BelowThreshold }`, so **no mini has
reached the drilling stage** and the FU-53/FU-55/FU-47/FU-61 guarantees are unexercised end
to end (FU-62); failure causes are still discarded at the error boundary (FU-58, now 10 of
37 stage failures); the drain planner can blow its byte cap on dense minis (FU-59); folder
runs are all-or-nothing with no resume (FU-60); the closure controller's own repair plan
can be refused by the router (FU-64); and `Crawler Zombie` is the one model that moved
backwards across the batch (FU-65). **FU-61 is no longer open** — every drain and vent bore
is confirmed to fully penetrate, 5.20× headroom, on the two corpus fixtures it was measured
on (that coverage caveat is itself FU-62). **Eight new rows, FU-62 through FU-69, were
filed by the batch record**; no existing row was renumbered, edited or displaced.

**A synthetic now hollows end to end, and it is on `develop`.** *[This paragraph read "on a
branch" / "`feature/dual-cavity-fixture`, **not on `develop`**" when it was written on that
branch; **corrected 2026-08-12** — `9e5dbb0` is the merge commit, the branch and its worktree
are deleted, and `develop` is this work. See
[`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md)
§4.6.]* The dual-cavity pawn fixture
(**merged to `develop` in `9e5dbb0`**) is the first geometry in the project to go
hollow → drain plan → drill → written-and-re-read shell through the production entry point under
unmodified `H0AdmissionLimits::production()`: 34.7 % savings, realized wall **3.176 mm** against
a requested 2.5 mm, one drain and one vent, with FU-47's flow rule deciding both sides of its own
boundary on real geometry. It **partly** answers FU-62 and does not close it — zero real minis
have hollowed — and it surfaced four new rows, **FU-79 through FU-82**, including the finding
that `Solid { BelowThreshold }` is a catch-all that also swallows resource-cap and support-stage
refusals (so the 37-mini rerun's headline needs re-reading) and that FU-47's connectivity verdict
depends on the model's pose. See [the fixture section](#dual-cavity-fixture-2026-08-12).

**Standing doctrine still in force:** FU-8b's lesson (never raise a cap to admit a
defect — fix the defect), the git-flow convention (main+develop, feature/* branches,
never push without instruction), and the owner's 2026-08-03 clearance-mask architecture
decision. See the Follow-ups ledger below for the full open-row list.

## 2026-08-13 — the requested wall becomes a TARGET and a 2.0 mm minimum wall becomes the guarantee: the Weeping Treant hollows for the first time {#wall-target-and-minimum-2026-08-13}

<a id="wall-target-and-minimum-2026-08-13"></a>

**Owner ruling, landed.** Two rulings, verbatim in substance:

> **Wall:** "matching what Chitubox does is fine — allowing walls down to 2mm but aiming
> for 2.5mm seems correct. A 2mm thick wall in spots will still be entirely structurally
> stable, there should be no problem there; the 2.5mm target is playing it quite safe."

> **Gate:** "it looks like the threshold is maybe around 10-15% savings, not 25% — and
> really it's whether the savings is worth it versus the damage cost of the drain holes.
> So perhaps the threshold is not PERCENTAGE but TOTAL VOLUME. Larger models will suffer
> less from the drain holes (since they tend to have more surface area), and benefit more
> from smaller percentages of total savings."

**What was wrong.** FU-53 (2026-08-12, `b71ddbc`) closed a real eggshell defect — shipped
walls of 0.27–0.70 mm against a requested 2.5 mm — by adding
`HOLLOW_CAVITY_MARCH_MARGIN_MM`, one voxel body diagonal, to the requested wall before
cavity selection. That made the realized wall `>=` the requested wall **by construction**,
and it cost the Weeping Treant more than half its cavity: the engine cut at **3.366 mm**
when 2.5 mm was asked for, delivering **1,556 mm³** where ChituBox delivers 6,060 mm³ at
the same requested wall. The owner weighed the trade and chose the target.

**What landed.**

| constant | from | to |
|---|---|---|
| `HOLLOW_CAVITY_MARCH_MARGIN_MM` | 0.8660254037844386 mm | **0.0 mm** |
| `HOLLOW_SAVE_MIN_ML` | 15.0 mL | **1.5 mL** |
| `HOLLOW_WALL_MIN_MM` | *(did not exist)* | **2.0 mm** (new) |

`HOLLOW_SAVE_MIN_FRAC` was **not** moved and stays at 0.25; the savings decision moved onto
total volume, which is the axis the owner named, and leaving the fraction where it is keeps
the small-model end of the OR exactly as strict as it was. `HOLLOW_WALL_DEFAULT_MM` (2.5 mm)
is untouched. **Nothing else was loosened.**

**The guarantee moved from construction to measurement; it did not go away.** The cavity is
now cut at exactly the wall the caller states, so the surface-nets extractor can land the
delivered boundary up to one voxel body diagonal inside it — as ChituBox's own accepted
output does. What is guaranteed instead is `HOLLOW_WALL_MIN_MM`, enforced by measuring the
exact distance from every vertex of the delivered cavity surface to the source surface and
discarding the shell if any of them falls under 2.0 mm, **before the Boolean**. FU-53's exact
distance-to-surface computation is untouched: what changed is what is done with the number.

**FU-55 was retargeted, not deleted.** As written it discarded any shell whose realized wall
measured below the *requested* wall with no epsilon. That check would have thrown away every
shell this ruling produces — and it would have thrown away **ChituBox's own treant**, which
the owner judged good and 45.4 % of whose cavity surface measures below the 2.500 mm it was
asked for. `checked_realized_wall_mm` now tests against the registered minimum. Removing the
margin without moving this check would have changed nothing at all.

**Measured, through the shipped `create_shell` path, at the registered 2.5 mm wall** (the new
`plinth-hollow` example `wall_distribution`, area-weighted over the delivered cavity surface):

| model | min | p5 | median | p95 | max | cavity | ChituBox |
|---|---|---|---|---|---|---|---|
| Weeping Treant v1.2 | **2.285** | 2.385 | **2.515** | 3.011 | 4.278 | 3,300.6 mm³ (13.4 %) | 6,060 mm³ (24.7 %) |
| Giant Toad | **2.274** | 2.386 | **2.518** | 2.750 | 4.021 | 12,126.6 mm³ (48.8 %) | 12,390 mm³ (49.8 %) |

Zero area under 2.20 mm on either model; the 2.0 mm minimum is respected with 0.27 mm to
spare. The toad lands **within 2 %** of the owner's own hand-hollowed result. The treant is
short because the FU-47 flow rule still severs its two leg chambers — **out of scope by
instruction, unchanged, and reported rather than worked around.** 43.5 % (treant) and 42.3 %
(toad) of the delivered cavity surface sits below 2.5 mm, against ChituBox's 45.4 % and
30.8 %: the same distribution shape, which is the point.

**The headline consequence: the committed Weeping Treant now hollows.** It previously reached
a shell and then lost it — the margin shrank the cavity under the savings gate and it took the
solid reserve (`IneligibleSavingsAfterCavityRepair` → `BelowThresholdAfterRepair`). The FU-47
finding that model witnessed is untouched: still thirteen interiors, still only the largest
retained, same voxel counts. What changed is that it keeps its cavity.

**Re-pins, each measured and attributed in place, none edited silently:** the dual-cavity pawn's
two throat literals re-derived through their own unchanged formulas (`DUAL_CAVITY_THROAT_ABOVE_BORE_MM`
5.816 → 4.95 = `t + 2.45`; `DUAL_CAVITY_THROAT_SEPARATED_MM` 3.0 → 2.0, which must sit under the
erosion and no longer did — **its B7 guard caught exactly that, which is what it was built for**);
`corpus/baseline/Sphere.pipeline.json`'s drain and vent, a second deliberate recorded re-record
under FU-51's protocol; the Sphere and cube cavity ranges; four drilling/refill counts; and the
`Sphere.prepped.stl` row of the zero-field divergence table, false → true. That last one was
**verified against `develop` (`7f8e4dd`) before being re-recorded**: the Sphere did not seal
there either — it refused under both fields with two *different* messages, and now refuses under
both with the same one, so the row flips for a reason that is not a regression. It stops at
Lowering where it used to reach Routing, which is recorded as an open question for whoever owns
the Sphere's coverage closure.

**Naming.** The new bound is `HOLLOW_WALL_MIN_MM`, never "floor": the owner ruled on the same day
that "floor" already means the bottom surface of a printed part and must never also mean a
smallest allowed value. Pre-existing "floor" names elsewhere are left for that dedicated pass.

**Gates**, foreground, on this tip: `cargo test --workspace --release` **1,355 passed, 0 failed,
2 ignored, 104 binaries**; `cargo clippy --workspace --all-targets -- -D warnings` **clean**.

**The minimum-wall check is proved load-bearing, not merely present.** Disabling it in a scratch
copy (`raw_realized_wall_mm < 0.0`) turns `the_check_has_no_epsilon` and
`a_shell_cut_at_the_minimum_wall_is_refused_before_the_starved_boolean_stage` red; raising the
registered minimum above the target turns `the_registered_minimum_wall_is_below_the_registered_wall_target`
and `the_committed_sphere_realizes_at_least_the_registered_minimum_wall` red. Both breakages
were reverted and the gates re-run.

**Files**: `crates/plinth-core/src/constants/placeholders.rs`,
`crates/plinth-core/tests/constants_golden.rs`,
`crates/plinth-core/tests/constants_placeholders.rs`, `crates/plinth-hollow/src/lib.rs`,
`crates/plinth-hollow/src/drilling.rs`, `crates/plinth-hollow/src/drains.rs`,
`crates/plinth-hollow/examples/wall_distribution.rs` (new),
`crates/plinth-mesh/src/fixtures.rs`, `crates/plinth-pipeline/src/lib.rs`,
`crates/plinth-pipeline/src/prepare.rs`,
`crates/plinth-pipeline/tests/dual_cavity_real_pipeline.rs`,
`corpus/baseline/Sphere.pipeline.json`, `docs/engineering-journal/constants.md`,
`docs/engineering-journal/m0-constants-source-ledger.json`.


## 2026-08-12 — a synthetic that actually hollows: the dual-cavity pawn witnesses FU-53/FU-55/FU-47 end to end, and the FU-47 doctrine turns out to be pose-dependent {#dual-cavity-fixture-2026-08-12}

<a id="dual-cavity-fixture-2026-08-12"></a>

> **CORRECTED 2026-08-12 — this work IS on `develop`.** The paragraph below was written on the
> branch and is left standing rather than rewritten. At `9e5dbb0` `develop` **is** this work:
> `9e5dbb0` is the merge commit (parents `87f07c3` and `0fe4c65`), and both the
> `feature/dual-cavity-fixture` branch and its worktree are gone — the ref store holds only
> `develop` and `main`. Read every "on a branch" and "not on `develop`" below as **merged**.
> Recorded in
> [`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md)
> §4.6.

**Branch state first, because it changes how everything below should be read: this work is on
`feature/dual-cavity-fixture` and is NOT on `develop`.** Three commits — `21cdba5` (the
fixture, its mesh-side tests, the printable article), `76e4988` (a real-pipeline measurement
harness, deleted again by the next commit), `1ea01ec` (the fix pass answering two independent
reviews: the resize, the real-pipeline *test* suite, and every review finding). `cargo test
--workspace` and `cargo clippy --workspace --all-targets` are clean at `1ea01ec`. **Nothing was
pushed, and no cap, gate, floor, tolerance or registered physics constant moved** — two
*added* admission bounds only, which [FU-8b](#follow-ups-2026-08-04) permits. Reviewed !=
merged still applies; see this page's preamble.

### Why it exists

The 37-mini rerun recorded in [the ruled-batch section](#ruled-batch-landed-2026-08-12) emitted
**8 artifacts and every one of them was `Solid { BelowThreshold }`**. So the hollowing work that
batch landed — FU-53/FU-55's realized-wall guarantee, FU-47's resin-flow connectivity rule,
FU-61's bore penetration — had **never once been exercised on a shipped shell**; that gap is
[FU-62](#follow-ups-2026-08-04). Waiting for a real mini to hollow was not a plan. The dual-cavity
pawn is a synthetic built for the sole purpose of *being hollowable*: sized to clear the
registered savings gate on its own arithmetic, and shaped so the FU-47 doctrine is a property of
its geometry rather than of a hand-built voxel mask.

### What it is

`plinth_mesh::make_dual_cavity_pawn()` / `make_dual_cavity_pawn_with_throat_radius(r)`, in the
canonical fixture home `crates/plinth-mesh/src/fixtures.rs`. A **coaxial lathe** — two silhouette
spheres joined by a cylindrical throat, revolved in pure f64, manifold and watertight by
construction, no boolean kernel, f32-snapped so the strict STL writer accepts it. **As shipped
at `1ea01ec`:** lower sphere R 12.5 mm centred 9.5 mm above the foot plane and flat-truncated by
it, upper sphere R 8.0 mm, centres 20.5 mm apart (exactly `R1 + R2`), 64 slices, 48/60 arc steps
— a **25 × 38 mm** article with **13,952** source faces. The axis sits at `x = y = 0` on whole
multiples of the 0.5 mm pitch, so `grid_spec_from_bounds` anchors a lattice sample exactly on the
axis; that phase is what makes the throat-clearance numbers below *exact equalities* rather than
approximations, and it is a property of the bench, not of production.

**One parameter, three registered settings**, all cavity-side radii measured against the live
registry erosion `t = HOLLOW_WALL_DEFAULT_MM + HOLLOW_CAVITY_MARCH_MARGIN_MM = 3.3660254 mm`
(read from `HollowSettings`, never restated as a literal in `plinth-mesh`):

> **Annotated 2026-08-13, and the numbers in this subsection have MOVED.** The owner ruling on
> the wall ([the dated section](#wall-target-and-minimum-2026-08-13)) set
> `HOLLOW_CAVITY_MARCH_MARGIN_MM` to zero, so the live erosion `t` is now the requested wall
> itself, **2.5 mm**, not 3.3660254 mm. Both physical throat literals were re-derived through
> their own unchanged formulas: `DUAL_CAVITY_THROAT_ABOVE_BORE_MM` 5.816025403784439 → **4.95**
> (`t + 2.45`), and `DUAL_CAVITY_THROAT_SEPARATED_MM` 3.0 → **2.0**, which must sit *under* `t`
> to hold no cavity voxel and at 3.0 mm no longer did. **The B7 guard named below caught exactly
> that**, by name, before any other test could silently become a different experiment — which is
> what it was built for. The cavity-side radii (1.45 and 2.45) and every doctrine claim in this
> section are unchanged. This subsection is left standing as written rather than rewritten, per
> the journal's own protocol; read the `t` and the two physical radii through this annotation.

- **SEPARATED**, physical throat r = 3.0 mm — *below* `t`, so the throat holds no cavity voxel at
  all and the two interiors are not even 6-connected. The control.
- **AT_BORE**, r = `t + 1.45` — the throat's axis clearance is **exactly 1.5 mm =
  `HOLLOW_DRAIN_R_MM`**, and is **refused** by FU-47's corrected core threshold
  (`HOLLOW_DRAIN_R_MM + pitch/2` = **1.75 mm**). Plain 6-connectivity says **one** component here;
  the flow rule says **two**. That disagreement, on real geometry, is the fixture's crown jewel:
  it is the only configuration in which FU-47's rule provably does work that 6-connectivity does
  not, and a comparison written against the raw 1.5 mm bore would wrongly join the chambers.
- **ABOVE_BORE** (the default), r = `t + 2.45` — clearance exactly **2.5 mm**, admitted; one
  drainable interior spanning both chambers.

**Exact equality with the 1.75 mm threshold is unreachable**, and the fixture proves that by
computation rather than asserting it: 1.75 = 0.5·3.5 needs a lattice offset with `i² + j² =
12.25`, which is not an integer. The fixture therefore pins the three-voxel-step case on the
**refused** side, which is the strongest available on-geometry witness of FU-47's half-voxel
tightening.

### What it witnessed — measured, not claimed

Reproduced by this scribe pass at `1ea01ec`, release build,
`cargo run -p plinth-hollow --example dual_cavity_article`, direct hollow chain
(`create_shell` → `plan_drains` → `drill_planned_shell`) under **genuinely unmodified**
`H0AdmissionLimits::production()`, registered 2.5 mm wall. Output went to a scratch directory
outside the repository and **is not committed**.

| Setting | throat r (mm) | source mm³ | cavity mm³ | savings | realized wall (mm) | drains | vents | drilled |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SEPARATED | 3.0 | 9,993.2 | 2,974.2 | 29.8 % | 3.176138 | 1 | 1 | refused (see FU-81) |
| AT_BORE | 4.8160254 | 10,071.8 | 3,013.9 | 29.9 % | 3.176335 | 1 | 1 | refused (see FU-81) |
| ABOVE_BORE | 5.8160254 | 10,180.6 | 3,530.9 | 34.7 % | 3.176951 | 1 | 1 | **yes** |

**What each column is evidence of.**

- **It hollows at all.** `create_shell` returns `Ok` at all three settings under production
  limits, clearing the registered 25 % fraction limb by recomputation from the candidate's own
  evidence — never against a literal. The 15 mL absolute limb is not relied on. This is the
  first shell in the project's history that a *test* watches go all the way through the
  production entry point and come out drillable.
- **FU-53/FU-55.** Requested wall **2.5 mm**; realized wall **3.176 mm** at every setting, with
  its `RealizedWallClaim` recorded. The shipped wall is *thicker* than requested, which is the
  guaranteed direction.
- **FU-47.** The **shipped** cavity field — post-flow-selection, post-well-composedness repair,
  the field `plan_drains` actually receives — occupies **2** chambers at ABOVE_BORE and **1** at
  AT_BORE and SEPARATED, measured by a new rotation- and translation-invariant
  `CavityField::deep_lobe_count`. AT_BORE's `label3d(cavity_mask, 6)` is **1** while its
  `flow_labels` is **2**, in the same test, so the two rules cannot drift apart unnoticed.
- **The drainage experiment the owner asked for.** Exactly **one drain** — origin `[0, 0, 3.5]`,
  in the lower chamber, below the `HOLLOW_LOBE_BAND_MM` band — and exactly **one vent**, at the
  cavity crown, at every setting. The upper chamber never gets a drain of its own, so at
  ABOVE_BORE resin from the far chamber **must traverse the throat**. That falls out of the
  production planner with no special pleading.
- **The bytes.** The real-pipeline suite writes the drilled shell, **re-reads it from disk**,
  builds topology on what came off the disk, requires zero boundary edges, certifies the closed
  components, and asserts the certified volume equals source − cavity. The reported savings are
  bound to the STL bytes, not to an in-memory candidate.
- **In-test pins** (machine-measured on the reference toolchain, house style): ABOVE_BORE source
  **10,180.584 mm³**, cavity **3,530.875 mm³**.

Test homes: `crates/plinth-mesh/tests/fixtures.rs` (geometry invariants A1–A7),
`crates/plinth-hollow/src/lib.rs`'s `dual_cavity_doctrine` module (B1–B7, in-crate because the
flow and distance-transform entry points are `pub(crate)`), and
`crates/plinth-pipeline/tests/dual_cavity_real_pipeline.rs` (the real chain — re-run by this
scribe pass at `1ea01ec`: **5 passed, 0 failed, 1.32 s**, release). Article generator:
`crates/plinth-hollow/examples/dual_cavity_article.rs`, output directory from `argv[1]`, refusing
to run without one.

### Four things the fixture caught that nothing else had

**1. FU-47's connectivity verdict is POSE-DEPENDENT, and the first version of this fixture was
measurably wrong because of it.** The doctrine tests run on the fixture's own exact lattice
phase; production runs `repair → decimate → dense orientation sweep` first, which lands the
throat on an arbitrary phase. Measured, same process, same settings: at the original
cavity-side throat of **1.90 mm**, ABOVE_BORE's cavity was **11,999.1 mm³** on the raw mesh but
**10,038.8 mm³** after decimate + orient — **0.45 %** from the SEPARATED control's own 9,993.4
mm³, i.e. the far chamber had silently dropped out and only the lower chamber hollowed. The
mechanism: the worst-case in-plane distance from an arbitrary axis to the nearest 0.5 mm sample
is **0.5/√2 ≈ 0.354 mm**, so an off-phase throat reads `1.90 − 0.354 = 1.546 mm`, under the
1.75 mm threshold. The fix was to the **fixture**, not the threshold: the cavity-side ABOVE_BORE
throat moved 1.90 → **2.45 mm** (worst case 2.096 mm, still inside the
`(0.5·√20, 0.5·√25)` bracket that reads exactly 2.5 mm on the fixture's own phase), and a new
fail-closed suite asserts the doctrine on the real chain **and** on a deliberate quarter-pitch
translation on all three axes. That suite has teeth: reverting the throat to 1.90 mm makes it
fail by name. **The general finding outlives the fixture and is filed as
[FU-82](#follow-ups-2026-08-04):** any real cavity whose throat sits within 0.354 mm of the core
threshold can flip between one and two flow components purely on the model's pose.

**2. The 1 GiB drain-planning ceiling refuses a 25 mm pawn, and the fix was the fixture.** The
first fixture (17 mm/11 mm silhouette, 53 mm tall, 55,552 source faces, 82k–92k shell faces)
needed **2,978,398,904 / 2,294,880,640 / 2,266,190,592 bytes** in `plan_drains`'s preflight
against the shipped `drain_planning_bytes` = **1,073,741,824** — so the printable article
existed only under an 8× raised, caller-owned budget, which means *no configuration a user could
run produced the hollow*. The dominant preflight term is `candidate_capacity × shell_triangles ×
96`. The resize to the 12.5 mm/8.0 mm silhouette brings the preflight to **578–632 MB, ~42 %
under the shipped ceiling** at all three settings, and a named test
(`every_setting_fits_under_the_shipped_drain_planning_ceiling`) now fails if a future geometry
change reinflates it. **This is the third and fourth independent confirmation of
[FU-59](#follow-ups-2026-08-04)** — after `Warhorse` at 4.06× and `Giant Frog` at 8.30× — and the
first on a model small enough that the cap is clearly the defect rather than the model. The old
sizing note's claim that the savings gate forces chamber radii ≥ 16.3 mm was simply wrong
arithmetic; the lower chamber alone clears the 25 % fraction limb at 29.8 %.

**3. A model can hollow, drain and drill, and still ship solid — because the SUPPORT stage
refuses the drilled shell.** An end-to-end `plinth prepare` on `above-bore.source.stl` at
`1ea01ec` exits **0 and emits**. The hollow succeeds, the drain plan succeeds, the drilling
succeeds — and then the support stage refuses the **30,290-face** drilled shell during
`Lowering` (`max_grid_bytes=67108864`, `knn_max_work=100000000`), and `prepare` substitutes the
oriented **solid**. Filed as [FU-80](#follow-ups-2026-08-04). Worse, the substitution is recorded
as `SolidDecisionReason::BelowThreshold` — a false statement about what the hollow stage decided
— and the same catch-all swallows a `HollowError::DrainPlanning(ResourceLimitExceeded)` too, so a
memory-cap hit and a savings verdict are indistinguishable on the wire. That is
[FU-79](#follow-ups-2026-08-04). Note what this means for
[FU-62](#follow-ups-2026-08-04): **`Solid { BelowThreshold }` in the 37-mini rerun cannot be
trusted to mean what it says**, and some of those 8 may not have been savings refusals at all.

**4. The vent planner can choose a site the driller then refuses.** At SEPARATED and AT_BORE, on
the fixture's own axis-aligned pose, `drill_planned_shell` refuses with `planned opening 1 is not
a clear cavity-to-exterior passage` — reproduced by this scribe pass, both settings, in the run
tabulated above. ABOVE_BORE drills cleanly on every pose, and all three settings drill cleanly on
every rotated pose the real chain produces. The apparent cause is a vent site on the spherical
crown's own axis, where the horizontal search ray is tangent to the dome. **This is a refusal, so
it is safe** — the article example reports it and still emits the undrilled shell and its cutaway
rather than papering over it — but it is a planner/driller disagreement inside a stage FU-70 is
about to rebuild. Filed as [FU-81](#follow-ups-2026-08-04).

### Its role in the drainage-architecture work

**This fixture is the canonical article for [FU-70](#follow-ups-2026-08-04)** (the multi-cavity
drainage architecture the owner ruled in on 2026-08-12 — see
[the doctrine section](#drainage-doctrine-and-orchestrator-rulings-2026-08-12)), and it was built
to be exactly that. Every claim FU-70 has to make has a home here:

- **Multi-cavity retention.** SEPARATED is two admissible chambers that today's engine hollows as
  one and leaves the far one solid. When FU-70 retains both, the AT_BORE and SEPARATED rows in the
  table above are the regression that must flip, by name, with before/after numbers.
- **Per-cavity vents.** Today the fixture gets exactly **one** vent regardless of how many
  chambers are hollow. FU-70's per-cavity vent obligation is directly measurable here.
- **Cavity-to-cavity drainage and the throat rule.** AT_BORE is a sub-bore neck between two
  chambers — precisely the link FU-70's chain/DAG rule has to reason about — and its clearance is
  pinned to the bit, so a throat-widening repair's effect is measurable rather than argued.
- **Throat-widening behind the wall gate.** The realized wall is already 3.176 mm against a
  2.5 mm request at every setting, so there is measured headroom to widen into, and
  [FU-71](#follow-ups-2026-08-04)'s refusal counter has a fixture that can actually exercise it.
- **Orientation-aware drainage.** Finding 1 above means the fixture is *already* the sharpest
  available probe of what re-orientation does to a cavity's connectivity, with a fail-closed test
  that measures it.

**What it is not.** It is not a corpus member: `corpus/manifest.json`, `corpus/models/` and
`crates/plinth-mesh/examples/corpus_gate.rs`'s frozen `EXPECTED`/`[Builder; 5]` lists are
untouched, and adding a ninth member would break that gate. It does not replace
[FU-62](#follow-ups-2026-08-04): **zero real minis have hollowed**, and a synthetic sized to
clear the gate is evidence that the machinery works, not that the corpus will reach it. And its
exact clearance equalities are a property of one lattice phase — the fixture's doc comment says
so loudly, because translating it off the 0.5 mm lattice is the easiest way for a future edit to
make it quietly stop testing what it claims.

**Four new rows, [FU-79 through FU-82](#follow-ups-2026-08-04), were filed by this pass** at the
next free numbers under the recorded collision protocol; no existing row was renumbered, reused
or displaced. Three existing rows were **annotated rather than duplicated** — FU-59 (the
drain-planning cap, now with two more measurements), FU-62 (the end-to-end gap, now partially
answered on a synthetic) and FU-58 (which is why finding 3 can only say "during `Lowering`" and
not which of the seven `LoweringError` variants fired). The corpus-provisioning friction this
work hit again is already [FU-4](#follow-ups-2026-08-04) and is not re-filed.

## 2026-08-12 — drainage becomes a doctrine: the owner rules multi-cavity in, and twenty-three lane questions are ruled under co-owner authority {#drainage-doctrine-and-orchestrator-rulings-2026-08-12}

<a id="drainage-doctrine-and-orchestrator-rulings-2026-08-12"></a>

**Two classes of decision landed on 2026-08-12, and they do not carry the same weight.**
Both are recorded, with their reasoning, in
[`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md).

**Owner rulings (Craig, drainage physics).** **Multi-cavity hollowing is allowed and
encouraged** — *"as long as each cavity is large enough and properly drainable"* — lifting the
engine's one-cavity hard-coding. **Cavity-to-cavity drainage is legitimate**: drainage is a
chain that must terminate at the exterior. **Boring into a neighbouring cavity is preferred
over scarring the visible surface** (soft preference). **Throat-widening is an admissible
repair**, behind a wall check and a structural/peel check. **Drainage is orientation-aware**:
drains at each cavity's low point in the print orientation, no pooling in local minima, an air
inlet above for replacement air — and the throat rule extends to the drilling-side
drainability and trapped-pocket floods. **Two tiers of wall**: the normal spec serves
green-state self-support, while a strut-carried neck needs only a cured-strength minimum
(**~1.5 mm, PROVISIONAL and NOT registered** — it owes coupon-plate provenance first).

**Orchestrator rulings (co-owner authority, granted the same day).** The twenty-three open
questions the four lane planners filed rather than guessed at are resolved — five RULED into
code work, the rest confirming the lanes' conservative choices as permanent answers — plus the
staging of the two-tier wall doctrine into two phases and the journal-hygiene dispositions.
**The authority came with an escalation boundary that still holds:** physical-doctrine
expansions, reliability-vs-savings tradeoffs, new-scope spend, and anything cap-adjacent go to
Craig.

**An engine reality check is on the record with the rulings**, so the implementer starts from
facts: the mesh is already hollowed in print orientation, drains already sit at each low
lobe's own min-z, the drainability proof is already monotone-in-z with a hard `TrappedCavity`
refusal, and a vent is already mandatory — but there is **one vent per model rather than per
cavity, one cavity ever retained, no geometric pocket detector, and no cavity-to-cavity,
interior-preference or throat-widening machinery at all**.

**Nine rows, FU-70 through FU-78**, were filed by this pass at the next free numbers; three
further items were folded into existing rows (FU-66, FU-68, FU-17) rather than duplicated. See
[the Follow-ups ledger](#follow-ups-2026-08-04). **This record is documentation only: no gate,
cap, floor, tolerance, constant or registry entry moved, and nothing in it loosens anything.**

## 2026-08-12 — the ruled batch is on `develop`, and the 37-mini corpus is re-run against it: 1 artifact becomes 8 {#ruled-batch-landed-2026-08-12}

<a id="ruled-batch-landed-2026-08-12"></a>

**All twelve rows worked in the 2026-08-12 ruled batch are on `develop`**, and the full
record — what landed per row, the FU-61 verdict, the corpus rerun, and every question the
lanes filed rather than guessed at — is in
[`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md).
This section carries the measurement; that record carries the reasoning.

**The four lanes merged in this order**, each no-ff, each with `cargo test --workspace`
green and `cargo clippy --workspace --all-targets` clean at the merge commit:
`b71ddbc` (`feature/hollow-walls` — FU-50, FU-53, FU-55, FU-54, FU-37), `b9decd5`
(`feature/coverage-closure` — FU-56, FU-57), `b9c8a6b` (`feature/connectivity-doctrine` —
FU-47), `647c991` (`feature/sphere-generator` — FU-36). Four further commits landed
directly on `develop`: `44b8df1` (FU-61's verification and FU-51's one re-record),
`4d5de5e` (FU-46's ruled scope, corrected in `repair.rs`'s comments), `4084c40` (the dated
record the coverage lane never wrote), `fbbc099` (FU-38). **Nothing was pushed**; the
lane branch tips are unchanged and no worktree was touched.

**FU-8b compliance across the whole batch, checked against the pre-batch base `0ee2814`
rather than trusted from any lane's own claim:** the constants diff is **additions only** —
`HOLLOW_CAVITY_MARCH_MARGIN_MM` (derived, with a cross-constant test that fails if
`HOLLOW_PITCH_MM` moves and it does not), `DEFAULT_MAX_CAVITY_REPAIR_PASSES` = 8,
`DEFAULT_CAVITY_DISTANCE_INDEX_BYTES`, `TOPOLOGY_CUT_CLOSED_SOLID_MASK` — plus one
**tightening**, the emit closure bound `1e-12` → `1e-15`. `max_raster_work_items`,
`HOLLOW_WALL_DEFAULT_MM`, the support gate, the 6.0 mm contact radius, the 0.16 mm²
island floor and the zero uncovered-area tolerance are byte-unchanged. No Python anywhere.

### The corpus rerun

One `plinth prepare` run over all 37 minis in `minis-to-process/`, release binary built
from `develop` at `647c991` (all four lane merges in), default dense orientation sweep,
registered 2.5 mm wall, no cache, single instance, run to completion. Output went to a
scratch directory outside the repository and **is not committed**.

**Why this measures HEAD even though it was built at `647c991`:** the four commits since
touch `crates/*/src/` in three files only, and the diff on all three is confined to a
`#[cfg(test)]` module (`plinth-hollow/src/drains.rs`) or to comments
(`plinth-hollow/src/drilling.rs`, `plinth-mesh/src/repair.rs`) — checked by filtering the
diff to non-comment, non-test lines, which is empty for two of the three and inside the
test module for the third.

```
SUMMARY  emitted=8  gate-refused=0  failed=29  stopped=0
```

| Class | Baseline (2026-08-11/12) | This run | Delta |
| --- | ---: | ---: | ---: |
| Artifacts emitted | 1 | **8** | **+7** |
| `PreWriteGate::UncoveredIslands` refusals | 21 | **0** | **−21** |
| Coverage-closure controller refusals (FU-56; did not exist) | 0 | **19** | +19 |
| Rasterizer work-cap refusals on flat bases (FU-50) | 2 | **0** | **−2** |
| Sealed-support stage failures (`Failed(phase)`, FU-58) | 6 | 10 | +4 |
| Models with **no disposition at all** (the baseline run was killed) | 7 | **0** | **−7** |

The baseline's 7 no-disposition models make a strict per-class subtraction unsound for the
two failure classes, so the sound comparison is **per model over the 21 the baseline names
by name** — and that comparison is stated below rather than inferred.

**The eight artifacts:** `Bat Cat Owl`, `Plague Zombie`, `Round 25`, `Round 50`,
`Round 100`, `Skulker Zombie`, `Tavey Badger`, `zombie-3-a`.

- **`Round 50` and `Round 100` close FU-50's last open question by measurement.** FU-50's
  implementer recorded that the two flat bases had been proved to pass *rasterization* but
  never run end to end. They now emit artifacts.
- **Four of the 21 gate-refused minis now emit** — `Bat Cat Owl` (1 blocking defect at
  baseline), `Plague Zombie` (3), `Skulker Zombie` (1), `Tavey Badger` (1). This is the
  first time in this project's record that a real mini has produced an artifact.
- **Every one of the eight is `Solid { reason: BelowThreshold }`.** Not one shell shipped.
  FU-53's wall fix, FU-55's realized-wall check, FU-47's flow connectivity and FU-61's bore
  guarantee are **unexercised end to end on the mini corpus** — filed as
  [FU-62](#follow-ups-2026-08-04).

**The 29 failures**, by class: `CoverageClosure { NoEligibleRepairRows }` **10**
(`Cat (wild shape - Cat Lord)`, `Deinonychus`, `Morel Mudskipper`, `Myconid Adult`,
`Tavey`, `Tavey Base`, `Warhorse`, `zombie-4-a`, `zombie-8-a`, `zombie-9-b`);
`CoverageClosure { AttemptsExhausted }` **5** (`Decomposing Zombie`, `Giant Wolf Spider`,
`Skeleton_1`, `Zombie Cat`, `zombie-7-a`);
`CoverageClosure { RepairPlanRefused { Routing, Cco(InvalidGeometry) } }` **4**
(`Giant Toad`, `Skeleton_3`, `Thorny Vegepygmy`, `zombie-5-a`); `Failed(Routing)` **7**
(`Awakened Tree`, `Crawler Zombie`, `Giant Toad (swallowing)`, `Morel base`,
`Rotting Warhorse`, `Skeleton_2`, `zombie-6-a`); `Failed(Brace)` **3** (`Giant Bat`,
`zombie-1-a`, `zombie-2-a`).

**Regression check against the 21 the baseline names:** 4 now emit, 16 still refuse on
coverage (now through the controller instead of the gate), and **exactly one moved
backwards** — `Crawler Zombie`, from a 2-blocking-defect gate refusal to `Failed(Routing)`.
It is the only model in the 37 that lost ground across this batch, and it is filed as
[FU-65](#follow-ups-2026-08-04). No refusal class present in the baseline grew other than
the stage-failure class, whose growth is not separable from the 7 models the baseline
never dispositioned.

**FU-56's own prescribed first action, answered by measurement.** The row asked that the
three cheapest refusals — `Tavey Badger` (1 defect), `Thorny Vegepygmy` (1),
`Myconid Adult` (1) — be measured to decide whether the controller faces *a placement
problem or a candidate-pool problem*. The answer is **both, model by model, plus a third
thing neither option anticipated**: `Tavey Badger` **closes and emits**; `Myconid Adult`
refuses at attempt **zero** with `NoEligibleRepairRows` on one 0.0092 mm² island — the pool
offered nothing eligible, a candidate-pool problem; `Thorny Vegepygmy` had rows, appended
them over 2 attempts, and the **router then refused the densified plan outright**
(`Cco(InvalidGeometry)`) on a 0.4528 mm² island — filed as [FU-64](#follow-ups-2026-08-04).

**FU-57's evidence is complete on this run, verified by enumeration, not by sampling.**
All **19** coverage refusals carry a `<stem>.coverage.json` sidecar (schema
`plinth.coverage-refusal.v1`) — 19 of 19. **53** island SVGs were written, 1 to 5 per
model, capped exactly at `COVERAGE_RENDER_MAX_ISLANDS` = 5 on the seven models with more
islands than that. Every card ranks its worst islands worst-first with uncovered mm² and
worst-gap kind, shows 3, and points the remainder at the sidecar. Nothing was written at
any `.chitubox` target path.

**What this rerun does NOT claim.** It does not claim any of the 29 refusals is closable,
that the four newly-emitted minis print well, or that a hollowed shell would survive the
pipeline if one were ever produced. It claims what it measured.

### FU-61's verdict, in one line

**Every drain and vent bore fully penetrates at full bore width**, worst-case outer margin
**4.5000 mm** against a 0.86603 mm voxel body diagonal — **5.20×** headroom — and the
overshoot the ruling hypothesised already exists as the registered
`HOLLOW_EXTRA_DRAIN_POLICY` ladder. **No new margin constant was needed and none was
registered.** The full measurement table and method are in
[the 2026-08-12 FU-61/FU-51 section below](#fu61-confirmed-fu51-re-record-2026-08-12); the
coverage caveat — two corpus fixtures, zero real minis — is [FU-62](#follow-ups-2026-08-04).

### Eight new rows

Filed at the next free numbers under the recorded collision protocol; **no existing row was
renumbered, edited or displaced**. [FU-62](#follow-ups-2026-08-04) (no mini reaches
drilling), FU-63 (the treant no longer seals in the OD-8 census), FU-64 (the closure
controller's own repair plan can be refused by the router), FU-65 (`Crawler Zombie`'s
regression), FU-66 (the drain/vent extension ladder is restated as bare literals), FU-67
(`exit_code()` masks a `Supports` refusal behind an `Input` failure), FU-68
(`Refused { gate: UncoveredIslands }` is now structurally unreachable), FU-69 (repo-wide
rustfmt drift).

## 2026-08-12 — FU-38 lands: all four decision-relevant instruments move to the real delicacy field, and every affected digest is re-pinned with its from→to {#fu38-real-delicacy-repin-2026-08-12}

<a id="fu38-real-delicacy-repin-2026-08-12"></a>

**FU-38 lands**, per the owner's 2026-08-12 ruling
([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§5): *"Update **ALL FOUR** instruments to the real delicacy field, and **re-pin
every affected digest with from→to attribution**."* The four instruments now
compute delicacy exactly as `plinth_pipeline::prepare::prepared_delicacy_field`
computes it in production — same LOD name (`"prepared"`), same registered
`DelicacySettings::production()`, same scaled
`DelicacyBuildConfig::production(faces)` — inlined rather than imported because
neither `plinth-support` nor `plinth-verify` may depend on `plinth-pipeline`
(the dependency runs the other way). **The fifth, length-only feed
(`resource_cap_policy.rs`, `plinth-pipeline/src/lib.rs`) is not
decision-relevant and was not touched**, exactly as the ruling directs.

**No digest is claimed wrong.** Every value on the left of an arrow below
remains exactly reproducible by the harness that produced it, against the
zero-field engine that was the engine at the time. Re-pinning records that the
engine changed, not that the measurement was bad.

**Nothing else moved.** No cap, gate, floor, radius, bound or tolerance is
touched by this landing: the coverage radius is still the registered
`COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm, the patch-area floor is
still `VOXEL_PITCH_MM²` = 0.16 mm², the uncovered-area tolerance is still zero,
the closure bound is still 8, and the registry is untouched. The only code
outside a doc comment that changed is the delicacy feed in three harness files.

### 1. The OD-8 census (`crates/plinth-support/examples/embed_incidence_census.rs`)

**Both columns were measured at this tip**, not copied from a prior record: the
harness was built twice from the same working tree — once with the `vec![0.0;
faces]` feed restored verbatim from `44b8df1`, once with the real field — and
both binaries were run over the same locally provisioned eight-model corpus.
**That matters, because the "from" column is *not* the FU-32 J4 septet any
more:** the coverage-closure lane
([`#coverage-closure-and-evidence-2026-08-12`](#coverage-closure-and-evidence-2026-08-12))
had already moved `make_overhang_table` and taken `make_raking_blade` off the
sealed list before FU-38 touched anything. Attributing FU-38's movement against
J4's numbers would have charged it for another lane's work.

| model | zero field (measured at `44b8df1`) | real field | `routed_contacts` | `final_live_members` |
| --- | --- | --- | --- | --- |
| `Sphere` | `2297157889062365577` | **`686822247060573262`** | 7 → **9** | 15 → **21** |
| `Cylinder` | `6853793710942395608` | **`12410021645461158056`** | 54 → **45** | 119 → **101** |
| `make_box` | `8709076999423814412` | **`5356317665317762409`** | 11 (unmoved) | 25 (unmoved) |
| `make_overhang_table` | `11652461737312715963` | **`14600685200423759332`** | 74 (unmoved) | 261 (unmoved) |
| `make_toed_slab` | `133000971864625681` | **`16855162491340010017`** | 64 (unmoved) | 170 (unmoved) |
| `make_thin_fin` | `14656364680512457190` | **`14718557305548010128`** | 17 (unmoved) | 41 (unmoved) |
| `make_raking_blade` | UNREACHABLE — coverage closure refused | UNREACHABLE — **byte-identical refusal** | — | — |
| `Weeping Treant v1.2` | UNREACHABLE — `Preparation` | UNREACHABLE — `Preparation` | — | — |

**Six digests move; two models do not seal under either field.** Unlike the
2026-08-09 J4 movement — where all seven digests moved and *not one count
moved* — **this one moves counts on two models**, and that is the finding, not
a defect: `Sphere` gains two routed contacts and six live members, `Cylinder`
loses nine and eighteen. The delicacy field is a real input to contact
selection (it breaks exact-Z ties, `plinth-support/src/lib.rs`), so a harness
fed zeros was selecting different contacts from the ones production selects.
That is precisely what this row claimed and what the ruling ordered corrected.

`make_raking_blade`'s refusal is **identical under both fields**, field by
field: `attempts` 8, reason `AttemptsExhausted`, island 3
(`uncovered_area_mm2` 0.060000776135069545, `nominated_repair_rows` 3,
`eligible_repair_rows` 0) and island 5 (5.13500164395471, 8, 11), both with
`worst_gap = NoRoutedContact`, and the same eight-attempt history. **This is
the `make_raking_blade` measurement the row deferred as "named but
unmeasured", landed as part of the re-pin rather than ahead of it, and its
answer is that the blade does not move.**

`Weeping Treant v1.2` fails `SupportRunError::Failed(Preparation)` under **both
fields**, so FU-38 neither caused nor cured it. It is recorded here as a
measured fact rather than argued away: the treant sealed in the FU-32 J6 census
of 2026-08-09 and does not seal in this one, the cause is not this row's, and
the one-word phase is exactly the opacity [FU-58](#follow-ups-2026-08-04) was
opened against. **No count, digest or claim about the treant is pinned by this
section.**

### 2. The J4 snapshot digests (`crates/plinth-support/tests/support_runner_integration.rs`)

`make_toed_slab` (procedural fixture, this test's own layer plan):
**`2198459883570957390` → `15556275517430246244`**. **The counts did not move
and must not**: 64 physical contact identities, 170 final live members, 69
distinct seats. `coverage_repair_rounds()` is still **0** — the model still
closes on its baseline layout, so the coverage-closure controller still costs
it nothing.

`make_raking_blade` has **no J4 snapshot digest left to re-pin on this
fixture**. `74030269957898085` was retired by the FU-56 coverage-closure
landing — a refusal has no sealed authority and therefore no digest — not by
FU-38. Under the real field the procedural blade refuses with a byte-identical
typed refusal (attempts 8, `AttemptsExhausted`, history length 9, islands 3 and
5 with the same component ids, representative faces and `NoRoutedContact`
gaps), which is the same answer the corpus blade gave in §1 and discharges the
row's "measure it, then re-pin".

### 3. The coverage-gate golden (`crates/plinth-verify/tests/coverage_gate.rs`)

**`snapshot_digest` `2198459883570957390` → `15556275517430246244`.**

**The from→to spans the coverage lane's rename of this golden.** FU-38's ruling
names `coverage_gate_overhang_table.json`; that file no longer exists —
`1d407a0` re-pinned this test off `make_overhang_table` onto `make_toed_slab`
and renamed the golden to `coverage_gate_toed_slab.json` (merged `b9decd5`).
The arrow above is therefore **the toed-slab golden's zero-field digest moving
to its real-field one**, not a value carried over from the retired
overhang-table file, and it is the same number §2 pins because it is the same
sealed snapshot.

**The golden was regenerated through this file's own producer path** — the
`THREAD_CHILD` arm of
`public_coverage_report_is_frozen_without_evidence_loss`, whose
`PLINTH_VERIFY_COVERAGE_JSON=` stdout line was captured verbatim — **never
hand-edited**, exactly as all four earlier refreshes were. The two JSON
documents were then diffed leaf by leaf: **181 scalar leaves, EXACTLY 1
moved**. Every `request` bound, all 11 `resources` counters, all 9 islands with
their face ids, all 9 patches (`uncovered_area_mm2`, `cut_ambiguous`,
`assessment_failed`, `worst_gap`) and all 9 defects are byte-identical. **No
evidence was lost and no verdict changed** — the report still refuses with 9
blocking defects.

### 4. The structural asserts and hand-counted coverage resource plan

Item 4 of the ruling is
`public_runner_measures_sealed_coverage_with_frozen_resources_and_evidence`,
re-derived under the real field. **Exactly one number in it moved:
`snapshot_digest`, to the same value as §2 and §3.** The 11-tuple resource plan
— `(71, 30, 69, 30, 50 688, 1 033 728, 12 624 152, 5 879 808, 3 902 976,
8 312 832, 12 215 808)` — is **unchanged**, and so are all 9 frozen islands and
all 9 frozen patches with every one of their bits. That is a **measured**
result, not an assumption: re-deriving it was the work, and finding it still
true is the finding. It says the delicacy field reordered exact-Z tie-breaks
inside routing on this fixture without moving a single seat, sample, island or
square millimetre the measurement can see — which is exactly why the *digest*
is the instrument that catches it and a count is not.

### What this landing does not do

It does not touch the length-only feeds, does not add or move any constant,
does not change any engine code, and does not claim the treant. Every from→to
above is reproducible from this tip: the census by building the harness with
and without the feed, §2–§4 by running their own tests.

## 2026-08-12 — production support planning gets a coverage-closure controller, and a coverage refusal stops being an integer {#coverage-closure-and-evidence-2026-08-12}

<a id="coverage-closure-and-evidence-2026-08-12"></a>

**FU-56 and FU-57 land together**, per the owner's 2026-08-12 ruling
([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§2 for FU-57; FU-56 under the standing doctrine that every refusal the pipeline
still emits is an automation work item). Three commits on
`feature/coverage-closure`, merged at `b9decd5`: `1d407a0` (the controller),
`cc891a8` (the evidence), `6ddf051` (eight adversarially-confirmed review
findings, one of which changed the outcome claim). **This section is written
after the fact, on 2026-08-12, because the lane shipped without one** — every
other lane in this batch journaled itself, and the audit that caught the FU-38
gap caught this too.

**FU-56 — the controller the repair pool was built for.** Production selection
was `select_contacts_baseline`, which returned one line before the
coverage-repair loop; `COVERAGE_REPAIR_MAX_ATTEMPTS` (registered value **8**)
had **no production reader** and the lineage-validated, contact-validated
`RepairPool` had no production consumer. `run_support` is now that controller
(`crates/plinth-support/src/coverage_closure.rs`, new): it prepares the sealed
context once, then iterates to the registered 8-attempt bound, each round
measuring **routed** coverage under **the pre-write gate's own registered
radius and floor** and nominating repair-pool rows for every still-blocking
patch — batch-first, one gathered occupancy/pool/sample pass per attempt with a
deterministic tie-break, appended through a new batched
`SpecializedContactPlanEdit::append_regular_repair_rows`. A model that already
covers costs exactly one pass and is byte-identical to the prior single-shot
runner. A model that cannot close refuses with the new typed
`SupportRunError::CoverageClosure`, carrying per-attempt history and per-island
evidence.

**The gate did not move and was not allowed to.** The coverage radius is still
the registered `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = **6.0 mm**, the
patch-area floor is still `VOXEL_PITCH_MM²` = **0.16 mm²**, the uncovered-area
tolerance is still **zero**, and the attempt bound is still **8**. Closure is
achieved **only** by appending immutable repair-pool contacts, never by
widening what counts as covered. `plinth_support::registered_coverage_request`
and `plinth_support::coverage_patch_is_blocking` became the one shared
implementation of the gate's request and its blocking predicate;
`plinth_pipeline::gates::registered_coverage_request` and
`plinth_verify::defects_for_patches` now delegate to them rather than restating
them — a refactor over identical registered numbers, so the controller
provably measures what the gate measures.

**A coverage refusal can no longer launder itself into a printed solid.**
`ChainStop::is_coverage_closure_refusal` is excluded from `is_support_refusal`,
so the oriented-solid reserve is not substituted for a model that failed to
cover; and `PrepareReport::exit_code` makes a coverage refusal **sticky at
second priority behind `Gates`** so an unrelated earlier `Input` failure cannot
hide the exact class FU-57 exists to surface. Both are bounds **added**;
nothing was loosened, and the remaining exit codes keep their order.

**The finding that changed the outcome (`6ddf051`, review finding 1).**
`nominate_repair_rows` scored every candidate against **all** coverage samples
on a blocking patch's faces, because the exact uncovered sample identities were
discarded when the patch was projected to a face-id list. A blocking patch's
faces are usually *mostly covered* with a residual gap, so the score summed
already-covered area and ranked rows beside the covered region above the row
that closes the gap — and since that score set never changes between attempts,
the controller walked a fixed ranking of eight useless rows and then declared
the island unclosable. `assess_coverage_with_mode` already computes the
uncovered set; it is now retained on
`CoverageMeasurement::uncovered_sample_indices` and is what nomination scores
against. **It costs no new allocation and no new charge** — the retained list is
the traversal's own scratch vector, moved rather than copied, already counted in
`scratch_bytes` and therefore in `phase_peak_bytes`.

**Measured consequence, and it is the first of its kind in this project's
record: an appended repair contact demonstrably reduces measured uncovered
area.** `make_overhang_table`, this suite's "genuinely unclosable island"
fixture, **closes and emits** — it had a closing candidate all along. Its
integration test is re-pinned from a refusal to
`public_runner_closes_overhang_table_only_after_appending_repair_contacts`,
which asserts ≥ 1 repair round, a strictly falling blocking-patch count and a
clean closing measurement. `Sphere.prepped.stl`'s blocking patch count measures
174 → 174 → **109** across attempts under both delicacy fields.
`make_raking_blade` still refuses from its own pool at the registered radius,
with both islands' `worst_gap` = `NoRoutedContact` on every attempt, and that
refusal is now *recorded rather than forced* — **not** answered by touching the
radius, the floor or the tolerance. `FinalSupportAuthority` gains
`coverage_closure_history()` / `coverage_repair_rounds()`, without which a
successful run cannot be told apart from one in which the controller did
nothing.

**FU-57 — all three evidence forms the ruling named, and only evidence.**
(1) A **sidecar `<model>.coverage.json`**, written beside the would-be artifact
path (`coverage_evidence::sidecar_path` = `target.with_extension("coverage.json")`),
schema `plinth.coverage-refusal.v1` from day one, with the full
`CoverageReport` or `CoverageClosureRefusal` embedded verbatim so it round-trips
losslessly with no second format. (2) The **CLI card ranks the worst islands**
with their uncovered mm² and worst gap — `COVERAGE_CARD_MAX_ISLANDS_SHOWN` = 3
shown worst-first, the remainder pointed at the sidecar — so a run's refusals
can be ranked without a rebuild. (3) **Deterministic pure-Rust SVG renders** of
the worst islands: no Blender, no Python, no new dependency; three orthographic
panels (plate view down −Z, +X elevation, and the island's own area-weighted
normal so the patch is always shown at true size), painter's-ordered by a total
depth comparator, with the island's uncovered-patch faces drawn after every
neutral face. Both evidence sources — the pre-write gate's `CoverageReport` and
the controller's `CoverageClosureRefusal` — reach one `EvidenceSource`
discriminant and one schema.

**Evidence is never a gate input, and that is enforced rather than promised.**
`judge_pre_write_gates` stays private and unchanged, and
`gates.rs`'s own `gates_module_source_never_imports_the_evidence_module` test
`include_str!`s this file's source and fails if `gates.rs` ever names the
evidence module. **Nothing is ever written at the `.chitubox` target path** —
the sidecar and renders are a second file class beside it, written on the
refusal path only, and `prepare_folder.rs` asserts the target path stays absent
(a coverage refusal now also clears a stale artifact at its target, which
`judge_then_emit` used to own and a coverage refusal no longer reaches).

**Bounds added by FU-57, none loosened:** `COVERAGE_RENDER_MAX_FACES` = 20,000
(over-bound meshes degrade to a bounding-box silhouette plus the highlighted
faces, and the degradation is **recorded in `render_skipped`, never silently
dropped**), `COVERAGE_RENDER_MAX_ISLANDS` = 5, `MAX_HIGHLIGHT_FACES_PER_ISLAND`
= 5,000, and `COVERAGE_CARD_MAX_ISLANDS_SHOWN` = 3. These are pipeline-crate
constants and **this implementation's own choice, not owner-derived** — flagged
as such in the lane's own openQuestions and repeated here so no reader mistakes
them for registered physics. A `measurement_error` field is carried on the
sidecar and the card so that "0 island(s)" can never read as "nothing was
uncovered": an *unmeasurable* coverage gate now persists a reason rather than
complete silence.

**Recorded rather than fixed:** the eager per-model `RenderMesh` copy in
`prepare.rs`'s `seal_and_emit`. The prepared mesh's verts/tris are snapshotted
*before* `run_support` consumes the `HollowedModel`, because a coverage-closure
refusal is only knowable from that call's `Err`, by which point the mesh is
otherwise gone. Both lazier remedies were checked and neither works today — the
mesh is not in hand at the `RepairPlanRefused` exits (the typestate moves the
context into each stage precisely to avoid duplicating the `NativeSurface`
grid), and the sealed snapshot is unavailable on the gate path — and both dead
ends are written into `RenderMesh`'s doc so the follow-up starts from what is
already ruled out.

**What this does NOT claim.** It does not claim that any of FU-56's 21 refused
minis is now printable: the fixtures that close here are procedural, and the
production re-run that would score the 21 is not part of this lane. FU-56's own
warning stands verbatim — **do not respond to a residual coverage refusal by
raising `COVERAGE_RADIUS_FACTOR`, lowering the patch-area floor, or admitting a
non-zero uncovered-area tolerance.** No cap, gate, floor or tolerance moved in
any of the three commits.

## 2026-08-12 — FU-61 confirms the bore fully penetrates, and FU-51's Sphere baseline gets its one deliberate re-record {#fu61-confirmed-fu51-re-record-2026-08-12}

<a id="fu61-confirmed-fu51-re-record-2026-08-12"></a>

**FU-61, the verification the 2026-08-12 owner ruling required before FU-51
could touch anything** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §9), **is CONFIRMED.**
`crates/plinth-pipeline/examples/fu61_bore_penetration.rs` runs the production
chain (`load -> repair -> decimate -> orient -> create_shell -> plan_drains ->
drill`) and then measures the drilled `Sphere` with an independent ray caster
sharing no code with the casts `plinth-hollow::drilling` already uses:
clear-radius probes on the drilled mesh (does the opening reach the cutter's
full nominal radius, not just its 24-gon inradius?) and margin probes on the
undrilled shell (how much of the extension ladder's overshoot the voxel
stepping actually consumes). Measured on the `Sphere`, both holes:

| | drain | vent |
| --- | --- | --- |
| `clear_radius / nominal` | 0.991 | 0.991 |
| `clear_at_cutter_inradius` | true | true |
| `min_outer_margin_mm` | 4.5001 | 4.8351 |
| `penetrates` | true | true |

**Every hole penetrates.** The realized clear radius reaches the cutter's own
24-gon inradius (0.9914 of nominal — a polygon-inscribed-circle fact, not a
lattice defect) on both holes, so the shortfall from 1.0 is the drilling
cutter's own polygon, not a narrowed or blocked opening. The worst-case
overshoot margin measured is **4.50 mm**, against the registered extension
ladder's **4.0 mm** minimum (`HOLLOW_EXTRA_DRAIN_POLICY`, the "before"/"drain
after" rungs) and the voxel body diagonal the ruling asked the margin to be
compared against, `0.5 * sqrt(3) = 0.8660254 mm` — **5.2x** that diagonal.
**No new margin constant is needed**: the existing ladder already clears the
worst measured case with headroom to spare, so this row's likely remedy
("the bore may have to overshoot... on the order of a voxel body diagonal")
did not have to be taken. No cap, floor or constant moves by this
confirmation.

**FU-51: the Sphere baseline gets its one deliberate, recorded regeneration.**
Per the ruling, only the drain/vent placement fields the row's own claim
named — `corpus/baseline/Sphere.pipeline.json`'s hollow-stage `drains[0]` and
`vent`, plus the `cavity_descriptor.drain_keepouts` that must stay
self-consistent with them — are rewritten to what the FU-61-verified engine
produces today, running the same `create_shell` + `plan_drains` composition
`crates/plinth-hollow/src/drains.rs`'s test harness already exercises against
the committed `Sphere.prepped.oriented-solid.stl`:

| field | old (Python-era) | new (this record) |
| --- | --- | --- |
| `drains[0].xyz` | `[0, 0, 0.5]` | `[0, 0, 3.5]` |
| `drains[0].wall_mm` | `1.0` | `4.0` |
| `vent.xyz` | `[0, 0, 19.5]` | `[-1.5, -4.0, 15.0]` |
| `vent.wall_mm` | `4.0` | `5.0` |
| `drain_keepouts[0]` (`p0`/`p1`) | `[0,0,4.5]` / `[0,0,-4.5]` | `[0,0,7.5]` / `[0,0,-4.5]` |
| `drain_keepouts[1]` (`axis`, `p0`/`p1`) | `+x`, `[-4,0,19.5]` / `[9,0,19.5]` | `-y`, `[-1.5,0,15]` / `[-1.5,-14,15]` |

**Everything else in the JSON is untouched.** The Python-era `a5` algorithm
fields (`solid_ml`, `final_ml`, `decision`, `hollow_fallback_reason`, the
cavity-field fingerprints and transform) are not read by any Rust code — the
frozen artifact's only production consumers are `plinth_orient`'s pinned
orientation replay (reads `stages[hollow].report.orientation`/`oriented`,
untouched by this change) and `plinth-hollow`'s own tests (which read exactly
the fields moved above, plus `bodies`/`watertight`/`winding`/
`cavity_breathes`, also untouched). Reproducing the rest would mean
re-running the retired Python pipeline, which this project does not do, and
FU-51's own claim was never about those fields.

**`frozen_shell_field_has_an_enclosed_drain_and_vent_plan`** (`drains.rs`) is
rewritten from a divergence check (baseline vs. engine, opposite signs) to a
straight regression pin: the re-recorded baseline is now asserted **equal**
to what the engine produces, whole, both holes, with the old numbers kept
only in the doc comment for provenance. The keepout self-consistency
assertion (`keepouts[0]["p1"]` derived from the baseline's own recorded
`xyz`/`wall_mm`) holds unchanged — it is a property of the record, and the
newly recorded numbers satisfy it by construction, the same way the old ones
did. A comment in `drilling.rs`'s
`frozen_sphere_is_drilled_once_and_every_voxel_drains` that said the frozen
baseline was "deliberately NOT regenerated until FU-61" is corrected: that
test reads only the untouched `bodies`/`watertight`/`winding`/
`cavity_breathes` fields, so it was never affected either way, and the
correction says so precisely rather than leaving a now-false blanket claim
standing.

**What this confirms:** the bore-penetration guarantee FU-61 asked for, on
the one model where the analytic answer is checkable; and that FU-51's
baseline no longer describes a defective field — it describes the current,
verified engine. **What it does NOT claim:** that any other frozen baseline
(`Cylinder`, `make_box`, etc.) was touched (none was), that the Python-era
volume/algorithm fields on `Sphere.pipeline.json` are now trustworthy records
of anything the Rust engine does (they are not, and are not claimed to be),
or that FU-61's one-model measurement generalizes to branched minis without
its own further measurement — the row's "first action" also named one
branched mini, not measured here. **No cap, floor, constant or registry
entry moves.** `cargo test --workspace` passes with these changes.

## 2026-08-12 — the wall a run asked for and the wall it got reach the run's own output, and a bound stops reading as a measurement {#hollow-wall-reaches-run-output-2026-08-12}

<a id="hollow-wall-reaches-run-output-2026-08-12"></a>

**Two review findings against the FU-37 and FU-55 landings below, fixed
together because the second is what the first has to carry.**

### FU-37's measurement was discarded by every real pipeline run

`HollowReport` gained `wall` (FU-37) and `realized_wall_mm` (FU-55), populated
on every production path — and then **`seal_and_emit` read only
`disposition()` and `cavity_repair()` off that report** before `run_support`
consumed the model and the report went with it. `ChainOutcome` had no wall
field; `ModelPreparationOutcome::Artifact` serialized `emit`, `geometry`,
`hollow_disposition` and `cavity_repair` and nothing else;
`plinth_emit::write` embeds geometry, placement and support records only; the
CLI printed no wall. So `plinth prepare --wall-mm 3.0` and `--wall-mm 2.5`
produced runs whose **stdout report, serialized report and `.chitubox`
artifact were all metadata-indistinguishable** — the exact claim FU-37 was
filed to invert — while the in-crate tests, which serialize a `HollowReport`
in isolation, passed. The row was marked LANDED with its motivating
end-to-end claim unmet.

**The seam is closed.** `ChainOutcome` carries `wall` and `realized_wall`,
read in `seal_and_emit` beside `cavity_repair` and for the same stated reason
(that is the last place the hollow report is in hand);
`ModelPreparationOutcome::Artifact` serializes `wall_mm` and `realized_wall`,
both `#[serde(default)]` so an older report still parses; and the CLI prints
`wall  requested 2.5 mm, realized 2.71 mm` under the disposition. The
2026-08-12 ruling (§8) decided the field's *home*, not that pipeline output
may omit it, so nothing here departs from it.

**Stated precisely, because the row's own wording is broader than this fix.**
FU-37 said "byte-indistinguishable in metadata in **every file** the pipeline
writes". What this closes is the **prepare report** — printed and serialized —
and the in-process record behind it. **The `.chitubox` itself still carries no
wall**: `plinth_emit::write` embeds geometry, placement and support records
only, and adding a field to the artifact wire format is an emit-format change
with its own byte-movement pass, not something to fold into a plumbing fix
that no ruling asked to widen. Recorded as a remaining gap rather than
claimed closed. §3.10.3, which had inherited the broader claim from the field
landing, is corrected to say the same.

**Proven end to end, not in isolation:**
`two_prepare_runs_at_different_walls_are_distinguishable_in_the_serialized_report`
runs the whole executor twice on one committed fixture, changing nothing but
the wall, and asserts the serialized artifact records differ and carry the
right figures; `prepare_folder`'s load-bearing artifact test now asserts the
requested wall reaches the record on the ordinary path.

`ModelPreparationOutcome` loses its derived `Eq` — `Finite` wraps an `f64` and
is `PartialEq` only. Nothing keys a map or a set on an outcome; the derive was
reachable only because every field happened to be integral. Only
`Artifact` gained the fields: `Refused` and `Failed` have no `ChainOutcome`
behind them, and inventing a wall for them would be manufacturing a record.

**What deliberately still reports `None`, and why.** The two solid-reserve
substitutions carry the *requested* wall (they already did) and **not** a
realized one: the artifact those paths write is the oriented solid, and a
realized-wall figure beside it would describe a shell that is not in the file.
FU-55's `ShellUnderWalled` decision is the one exception and stays one — there
the measurement is the *reason* for the decision, not a description of the
artifact. No committed fixture reaches an `Artifact` by the
hollowed-and-drilled route through `prepare` (the toed slab decides solid, the
Sphere takes the support-refusal reserve), so the drilled path's realized
figure is pinned where it is produced instead:
`drilling.rs`'s `production_drilling_succeeds_with_exact_refill_exclusion_and_public_report`
now asserts the report carries a `RealizedWall` at or above the requested wall
and that it is a measurement rather than a bound.

### A clamped query bound was indistinguishable from a measurement

`checked_realized_wall_mm` clamps an over-bound measurement (every cavity
vertex beyond `2 × (wall + margin)`, so the query returns `INFINITY`) to the
query bound and stored **that number** as `realized_wall_mm`. Its rustdoc
called it a "truthful lower bound", but the record carried no marker: a reader
of the FU-37/FU-55 figure could not tell **"measured exactly 6.73 mm"** from
**"at least 6.73 mm"**, and those are different claims about the same shell —
now doubly so, since that record reaches the serialized artifact.

**The number and its claim travel as one value.** `RealizedWall { wall_mm,
claim }` with `RealizedWallClaim::{Measured, AtLeast}` replaces the bare
`Option<Finite>` on `HollowReport` and the bare `Finite` on `CavityEvidence`,
so the two cannot be serialized, copied or read apart; `Display` renders them
as `6.73 mm` and `at least 6.73 mm`. The refusal path's own number is
`Measured` by construction — it is strictly below the requested wall, hence
strictly below the query bound, hence never the clamp.

**Nothing about the check moves.** The refusal still compares the **raw**
measurement, strictly, with no epsilon; a clamped record still means the shell
passed with room to spare. `an_over_bound_measurement_is_recorded_as_a_clamped_lower_bound`
is extended to pin that the two claims serialize differently at the same
number and that a distance exactly at the bound is a measurement, not a bound.

**Nothing loosened**: no cap, gate, floor or tolerance is touched by either
half.

**Files**: `crates/plinth-hollow/src/lib.rs`,
`crates/plinth-hollow/src/drilling.rs`, `crates/plinth-hollow/src/drains.rs`,
`crates/plinth-pipeline/src/lib.rs`, `crates/plinth-pipeline/src/prepare.rs`,
`crates/plinth-pipeline/tests/prepare_folder.rs`,
`crates/plinth-cli/src/execute.rs`.

---

## 2026-08-12 — `HOLLOW_CAVITY_MARCH_MARGIN_MM`'s derivation from the pitch stops being prose and becomes a test {#hollow-margin-derivation-2026-08-12}

<a id="hollow-margin-derivation-2026-08-12"></a>

**A review finding against the FU-53 landing, fixed.** The margin is declared
in the registry as the literal `0.5 * 1.732_050_807_568_877_2`, where the
`0.5` *duplicates* `HOLLOW_PITCH_MM`'s value rather than reading it, and its
own prose says it is DERIVED and must be re-derived if the pitch ever moves.
Nothing mechanical held them together: `constants_placeholders.rs` pins both
as independent literals with no cross-assertion, `constants_golden.rs` lists
only the name, and `HollowSettings::with_wall_mm` reads
`hollow_cavity_march_margin_mm()` and `hollow_pitch_mm()` from the registry
independently. **Bump the pitch to 0.6 mm and every constants test stays
green** while the margin stays 0.866 mm and a surface-nets vertex can sit
0.6·√3 ≈ 1.039 mm from a selected cavity centre — the realised wall
undershooting the requested wall by ~0.17 mm, silently reopening the exact
FU-53 shortfall the margin exists to close.

**The fix.** `the_cavity_march_margin_is_one_body_diagonal_of_the_registered_pitch`
(in `plinth-hollow`'s `fu53_exact_cavity_distance` module) asserts
`margin == pitch * sqrt(3)` **exactly**, off the same `HollowSettings` the
engine runs on, so both values come from the registry the runtime reads rather
than from restated literals. Exact equality, no tolerance: the registered
literal is the f64 nearest `pitch * sqrt(3)`, and a re-derivation that is not
is a re-derivation that was not done. The constant's registry description now
names that test, so an editor moving the pitch is told where the obligation is
enforced rather than only that one exists.

**No value moves.** `HOLLOW_PITCH_MM` (0.5 mm) and
`HOLLOW_CAVITY_MARCH_MARGIN_MM` (0.8660254037844386 mm) are byte-identical;
`docs/engineering-journal/constants.md` is regenerated by its own producer
(`render_constants_golden`) for the description text alone. This adds a guard
against a documented future hazard; it loosens nothing.

**Files**: `crates/plinth-hollow/src/lib.rs` (the test),
`crates/plinth-core/src/constants/placeholders.rs` (description),
`docs/engineering-journal/constants.md` (regenerated golden).

---

## 2026-08-12 — the rasterizer's shape-conditioned quadratic tail is removed, not narrowed: candidate generation follows the triangle instead of its bounding box {#hollow-raster-slab-clip-2026-08-12}

<a id="hollow-raster-slab-clip-2026-08-12"></a>

**A review finding against the FU-50 landing below, fixed.** The
area-proportional rasterizer that landed with FU-50 still enumerated
candidates by sweeping **every column of the triangle's projected bounding
box** before the exact test rejected them. That is `Θ(projected-AABB area)`
per triangle, not `Θ(triangle area)`, so a long thin sliver with vanishing
area kept paying quadratically in its longest edge whenever it lay diagonal
to the axes — the same growth law that produced the `Round 50`/`Round 100`
cap failures, surviving in the rotated direction. Measured before the fix, on
a 45° sliver of index length 200 and width 0.02: **20,736 work items for 358
covered cells (57.9× waste), 34× the identical axis-aligned sliver**. The
degenerate fallback was worse: one exactly-collinear zero-area triangle
spanning a 204³ grid diagonal walked its whole dilated bounding box for
**8,242,408 work items — 0.49× the unchanged cap, from a single triangle**.
FU-50's ruling ordered an area-proportional rasterizer and rested its
headroom argument on that property; the implementation's own header claimed
area-proportionality it did not have in the diagonal regime.

**The fix: clip, do not sweep.** `walk_triangle_candidates` now orders the
three axes by descending clamped index span and, for each cell column on the
outermost axis, clips the triangle to that cell slab, reads the *triangle's
own* extent there to bound the middle axis, and for each `(i, j)` reads the
triangle's extent inside that column to bound the innermost axis. Work
therefore tracks the cells the triangle actually covers **in every
orientation**. Clipping a degenerate triangle yields a degenerate polygon
whose extents are still exact, so the zero-area/collinear special case and
its `Θ(L³)` dilated-AABB fallback (`DEGENERATE_NORMAL_SQ`,
`scan_dilated_aabb`) are **deleted** rather than narrowed. The inner extent
is read off the column polygon's boundary cycle one segment at a time — an
extremum of a linear coordinate over a convex set intersected with a slab is
attained on that boundary — so no second polygon is materialized per cell.

**Completeness, proven and mechanized.** If cell `(i, j, k)` overlaps the
triangle then some triangle point lies in its cube; that point is in
`triangle ∩ slab(i)`, so the clip is non-empty; its middle coordinate is
within half a cell of `j`, so `j` is in the span read off that clip; and it
lies in the `(i, j)` column, so `k` is in the span read off that. The new
test `the_candidate_walk_enumerates_every_cell_an_exhaustive_aabb_scan_marks`
checks this against the **maximal** candidate set (every cell of every
triangle's dilated bounding box, exactly tested) on seven committed fixtures
and five adversarial ones, asserting set *equality*, not a subset relation.

**A real completeness defect was found by that test and fixed.** Clipped
extents come from floating-point segment/plane intersections while the exact
test reads the original triangle, so a triangle edge grazing a cell boundary
exactly (`x + y == 210` touching the corner of the cell above it) rounded a
hair inside and the walk dropped that cell — **6 of 20,501 cells on a flat
fan-base triangle**. Every clip slab and every clipped extent is now widened
by `CANDIDATE_EXTENT_SLACK` (relative, 1e-9, floored at one voxel). **This is
not a tolerance on any decision**: the overlap verdict is still the exact
13-axis separating-axis test, and a widening can only add a candidate the
exact test then rejects for free.

**The count pass stops paying for a verdict it discards.** The work-item
count is candidate enumeration alone, and candidate enumeration never
consults the overlap verdict — but both passes evaluated the full 13-axis SAT
per candidate, and the count pass threw it away (verified surviving in the
release build's post-optimization IR, which cannot delete the call). The
verdict now travels **unevaluated**, as `RasterCandidate::overlaps()`; the
count pass never calls it. The two-pass `RasterWorkMismatch` contract is
untouched — the candidate set still depends only on `(mesh, spec)`.

**The marked-voxel set does not move — measured, not argued.** The exact test
decides marking, and both the old and new candidate sets contain every truly
overlapping cell, so the marked set is identical. `raster_delta` re-run on the
eight committed models plus `Round 50`/`Round 100` reproduces
**`|new marked|` byte-for-byte identical to the table below** on all ten
(20,058 / 2,402 / 18,363 / 5,866 / 7,457 / 12,515 / 7,584 / 79,974 / 21,794 /
74,698), with `|old ∖ new| = 0` throughout. **No shell, cavity, drain, vent or
artifact changes.** Nothing downstream was re-pinned, and nothing needed to be.

**Work items, from → to** (same harness, same pitch, same unchanged cap
16,777,216):

| Model | FU-50 landing | this fix | ratio |
|---|---|---|---|
| Cylinder | 153,765 (0.0092x) | 34,579 (0.0021x) | 4.4× less |
| make_box | 5,292 (0.0003x) | 3,012 (0.0002x) | 1.8× less |
| make_overhang_table | 61,661 (0.0037x) | 22,446 (0.0013x) | 2.7× less |
| make_raking_blade | 15,923 (0.0009x) | 6,933 (0.0004x) | 2.3× less |
| make_thin_fin | 22,015 (0.0013x) | 8,972 (0.0005x) | 2.5× less |
| make_toed_slab | 43,472 (0.0026x) | 15,690 (0.0009x) | 2.8× less |
| Sphere | 161,491 (0.0096x) | 30,802 (0.0018x) | 5.2× less |
| **Weeping Treant v1.2** | **9,406,259 (0.5607x)** | **1,173,406 (0.0699x)** | **8.0× less** |
| Round 50 | 418,730 (0.0250x) | 99,910 (0.0060x) | 4.2× less |
| Round 100 | 443,767 (0.0265x) | 120,417 (0.0072x) | 3.7× less |

The `Weeping Treant` row is the one the FU-50 landing recorded honestly as a
regression against the frozen walker (9,406,259 vs 4,454,823 old items,
0.561x of cap). It is now **3.8× cheaper than the frozen walker it replaced**,
so that regression is gone rather than merely tolerated. FU-50's acceptance
test still passes and its margin widens: `Round 50` **1.150x → 0.0060x**,
`Round 100` **1.340x → 0.0072x** of the cap that did not move.

**Re-pinned, downward only.** `raster_work_profile_is_area_proportional_and_pinned`:
`Sphere` **161,491 → 30,802** work items (work/area-cells **32.2 → 6.14**),
`make_box` **5,292 → 3,012** (**2.21 → 1.26**). Its ratio bound is
**tightened** with them, from a shared 50.0 to per-fixture 8.0 and 2.0 — a
bound that no longer bites is not a bound. New coverage:
`candidate_work_tracks_covered_cells_at_every_aspect_ratio` pins work against
covered cells on the diagonal sliver, its axis-aligned twin, the collinear
grid diagonal, a single-point triangle and a fan base, and asserts the
rotated sliver cannot cost more than 3× its axis-aligned twin.

**Nothing loosened.** `max_raster_work_items` (16,777,216),
`DEFAULT_MAX_VOXEL_CELLS`, the savings floors, the support gate, the 6.0 mm
contact radius, the 0.16 mm² island floor and the zero coverage tolerance are
all untouched. The only bound that moved is a test bound, and it moved
tighter.

**Files**: `crates/plinth-hollow/src/lib.rs` (rasterizer, lazy verdict,
re-pinned and new tests), `crates/plinth-hollow/examples/raster_delta.rs`
(its verbatim copy of the production walker, kept verbatim).

---

## 2026-08-12 — the requested wall is persisted on the top-level `HollowReport`, on every production path {#hollow-wall-field-2026-08-12}

<a id="hollow-wall-field-2026-08-12"></a>

**FU-37 lands**, per the owner's 2026-08-12 ruling
([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§8): the requested wall goes on the **top-level `HollowReport`**, as a `wall`
field — not on the drilling evidence, which records holes rather than the
shell's defining parameter.

**The row's own claim, before this change.** `CavityEvidence::wall_thickness_mm`
recorded the operator's stated wall, but only in-process: `CavityEvidence` is
not `Serialize` and hangs off `HollowedModel::cavity_authority`, so it died
with the process. The serialized `HollowReport`, `CavityRecord` and
`DrillingEvidence` carried no wall field at all, so two artifacts prepared at
2.5 mm and 3.0 mm were byte-indistinguishable in metadata in every file the
pipeline wrote — the geometry differed, the record did not say why.

**The field.** `HollowReport::wall: Option<Finite>`, `#[serde(default)]` —
the same forward-compatibility pattern `cavity_repair` and `realized_wall_mm`
already established, so a report frozen before this field existed keeps
deserializing, reading a missing key as `None`. `Option` rather than required
because `HollowReport::solid_decision` is a public hand-built constructor
(used as a test stand-in by `plinth-emit`, `plinth-support` and
`plinth-verify`) with no request behind it to state a wall for. A new
`pub const fn wall(&self) -> Option<Finite>` accessor reads it.

**Populated on every production path.** `HollowedModel::solid` (the private
constructor behind every route that builds a `HollowReport` outside the test
stand-in) now takes a `wall: Option<Finite>` parameter alongside
`cavity_repair`, threaded to every caller:

- the hollowed-and-drilled path — `drilling::into_hollowed_model` already
  holds the `CavityEvidence` that built the shell, so `wall:
  Some(cavity.wall_thickness_mm())`;
- every typed `SolidDecisionReason` `run`'s own `convert_shell_failure_to_solid`
  converts a hollow-stage failure into (`NoCavity`, `BelowThreshold`,
  `NoCavityAfterRepair`, `BelowThresholdAfterRepair`, `ShellUnderWalled`) —
  `run` now passes `settings.wall_thickness_mm()`, in scope at every one of
  its typed decisions, down through the new parameter on
  `HollowedModel::solid_decision_recording_repair`;
- `HollowedWithSolidReserve::into_solid_fallback` — the drilled shell's own
  report already carries `Some(wall)` from `into_hollowed_model`, and the
  substitution reads it off that report the same way it already reads
  `cavity_repair` off it, rather than restating a number of its own;
- `plinth_pipeline::prepare::run_from_oriented`'s two solid-decision fallback
  sites — the pre-hollow refusal reads the wall straight off
  `policy.hollow.wall_mm` (the request that failed), the post-support-refusal
  site reads it off the drilled shell's own report, matching the
  `reserve_cavity_repair` pattern immediately above it in the same function.

**Distinct from `realized_wall_mm` (FU-53/FU-55).** `wall` states what the
caller asked for; `realized_wall_mm` states what the shell actually measured.
The 2026-08-12 ruling names `wall`'s field as the natural future home for the
realized figure recorded beside it — which is exactly what FU-55 already did,
landing first — without this change scheduling anything further.

**No cap, gate, floor or tolerance moved; no existing constant changed.**
This is a pure evidence addition: a new optional field, threaded to every
constructor that already existed, changing no decision any of them makes.

**Re-pinning.** `cargo test --workspace --offline --no-fail-fast` and the same
with `--all-features` were run before and after, workspace-wide: **nothing on
disk serializes a `HollowReport`** (no committed JSON carries `disposition`,
`cavity_repair` or `cavity_record`), so no golden file needed re-pinning. The
only call sites that moved were in-crate: two `HollowedModel::solid` test call
sites (an added `None` argument), two `HollowedModel::solid_decision_recording_repair`
test call sites in `plinth-hollow` and two production call sites in
`plinth-pipeline::prepare::run_from_oriented` (an added `wall` argument, wired
to a live value rather than `None`, since both are real production paths), and
`convert_shell_failure_to_solid`'s one direct unit test (an added
`requested_wall_mm` argument). None of these are goldens; all are call-site
signature updates, none of which changed what any existing test asserts about
disposition, cavity volume, drain geometry or any other previously-pinned
number.

**Tests.** New `crates/plinth-hollow/src/lib.rs::fu37_wall_field` module,
four tests: (1) **the row's own claim, inverted** — the committed Sphere's
report built at the registered default (2.5 mm) and handling-heavy (3.0 mm)
walls through `run`'s own production wiring, replicated call-for-call (this
crate has no way to construct a `plinth_orient::OrientedModel` directly to
call `run` itself), asserting the two reports and their serialized JSON now
differ, with the raw `"wall":2.5` / `"wall":3.0` byte pattern each is asserted
to contain; (2) every typed `SolidDecisionReason` `convert_shell_failure_to_solid`
produces carries `Some(requested_wall)`, table-driven over all six typed
`HollowError` arms, with the hand-built `HollowReport::solid_decision` alone
asserted `None`; (3) backward compatibility — a `HollowReport` JSON fixture
predating this field (and `cavity_repair`/`realized_wall_mm`) still
deserializes, reading all three as `None`; (4) round trip — serialize,
deserialize, exact `Finite` bit-pattern preserved. Existing tests extended in
place rather than duplicated: `drilling.rs`'s two `into_hollowed_model`
production tests (`sphere_plan` and the exact-refill-exclusion fixture) each
gained a `report.wall()` assertion; `lib.rs`'s repair-caused-solid-decision
test, the support-refusal-substitution test and the `ShellUnderWalled`
conversion test each gained a `wall` assertion alongside their existing
`cavity_repair`/`realized_wall_mm` ones. `cargo test --workspace` (all
binaries, including the release-gated 429,120-face Weeping Treant fixture,
re-run separately under `--release`) and `cargo clippy --workspace
--all-targets` are both clean.

**Module doc and §3.10.3 updated.** `crates/plinth-hollow/src/lib.rs`'s
module doc, which stated "no serialized artifact carries it yet (FU-37)", now
names `HollowReport::wall` as the artifact that does.
[§3.10.3](03-physics-and-constants.md#3103-closed-source-ledger-additions--hollowing-and-drainage)
is updated the same way, and this row is annotated LANDED in the follow-ups
ledger below, in place, per the established pattern.

## 2026-08-12 — the cavity-repair re-selection loop is bounded at eight, with a typed refusal {#hollow-cavity-repair-pass-limit-2026-08-12}

<a id="hollow-cavity-repair-pass-limit-2026-08-12"></a>

**FU-54 lands**, per the owner's 2026-08-12 ruling
([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§7): "bound it." `well_composed::well_composed_cavity_component`'s whole-grid
re-selection loop — the second `O(cells)` domain the module doc already named
and counted as [`WellComposedRepair::full_grid_passes`], but never bounded —
now refuses rather than running unboundedly.

**The bound.** A new, independent H0 admission domain,
`H0AdmissionLimits::max_cavity_repair_passes` (production default **8**),
validated in `H0AdmissionLimits::new` exactly like its six siblings (rejects
`0` and `usize::MAX`). `well_composed_cavity_component` now takes the limit as
a parameter and checks it at the top of every re-selection round, **strictly
before** that round's `label3d` call, so a round that would exceed the bound
is refused in place of the traversal it would have cost rather than after it.

**The refusal.** A new `HollowError::CavityRepairPassLimitExceeded { passes,
max_passes, critical_cells, repaired_voxels, cell_visits }`, carrying the same
repair evidence `CavityRepairRemovedCavity` and
`IneligibleSavingsAfterCavityRepair` already carry — a partial repair still
ran, and the record must say what it did.
`create_shell_recording_repair` lifts it onto `HollowStageFailure` the same
way it already does for `CavityRepairRemovedCavity`.
`plinth_pipeline::prepare::hollow_error_refuses_this_geometry` classifies it
in the **resource-ceilings** group, returning `false`: a cap that was reached
is a finding to report (`PipelineError::Hollow`, no artifact), never a route
to a solid substitution — the same FU-8b rule as every other admission limit
in that group.

**The number, and who owns it.** The owner's, not derived from anything.
Measured real models use 1–2 passes: 2 on the committed Weeping Treant v1.2
fixture, the raw Warhorse, the raw Giant Toad, the raw Awakened Tree and the
synthetic pinched dumbbell; 1 on every zero-critical-cell mask. Eight is 4×
headroom over the worst measured case, and it is a ceiling that is never meant
to bite real geometry — if a real model ever reaches it, the correct response
is to investigate the mask, never to raise the number.

**This ADDS a bound; it loosens nothing.** No existing cap, gate, floor or
tolerance moved. The well-composedness repair's removal-only mechanism, its
cell-visit accounting, and every other repair outcome are untouched.

**Tests.** The bound firing, with the right evidence, on the synthetic mask
directly (`well_composed::tests::the_pass_limit_refuses_instead_of_taking_the_next_reselection`)
and through the real production entry point on the pinched-dumbbell fixture
(`corpus_cavity_repair::the_pass_limit_refuses_a_real_shell_candidate_through_the_production_entry_point`,
both the `create_shell_with_admission_limits` and `create_shell_recording_repair`
surfaces). Admission-limit validation boundary
(`tests::cavity_repair_pass_admission_rejects_zero_and_usize_max`). Pipeline
classification (`hollow_error_refuses_this_geometry`'s existing exhaustive-match
test, extended). The existing well-composedness convergence tests and the
release-gated committed-Treant test continue to assert `full_grid_passes()`
is `1` or exactly `2` — unaffected by the new bound because production's
default of 8 is never approached — which is the evidence behind the 4×
headroom claim above. `cargo test --workspace` (all binaries, including the
release-gated 429,120-face Treant fixture, re-run separately under
`--release`) and `cargo clippy --workspace --all-targets` are both clean.

## 2026-08-12 — an under-walled shell never ships: the realized wall is measured against the source surface before the Boolean, and a shell that misses it is discarded for the solid reserve {#hollow-realized-wall-check-2026-08-12}

<a id="hollow-realized-wall-check-2026-08-12"></a>

**FU-55 lands**, the interim safety net the owner's 2026-08-12 ruling required
beside FU-53
([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§1): "an under-walled hollow shell must never ship." FU-53 made the realized
wall `>=` the requested wall *by construction* (a proven lemma over the
selection field); this change adds the independent, measured check the ruling
asked for on top of that proof — belt and suspenders, not a substitute for
either half.

**Where, and in what order.** Inside `create_shell_recording_repair`'s sealed
block, immediately after `marching_cavity` extracts the cavity surface `inner`
and strictly before the Manifold difference: the minimum, over every vertex of
`inner`, of the exact distance to the source surface, batched through the same
`geom_kernel::NativeSurface` FU-53's `exact_cavity_distance` already built for
selection (returned out and reused, not rebuilt — one BVH per shell, not two).
The reduction is index-parallel map (`NativeSurface::query`'s own
`par_iter().map().collect()`), **serial** min-fold across batches and across
each batch's own results — the repo forbids a float reduction across parallel
work items. Proved to run before the Boolean, not just believed to: a test
starves the Boolean stage's own admission limits to zero on the same
zero-margin fixture below and gets `RealizedWallBelowSpec` back, not a Boolean
resource refusal.

**The check.** Strict `realized_wall_mm < settings.wall_thickness_mm`, no
epsilon — FU-53's own margin (`HOLLOW_CAVITY_MARCH_MARGIN_MM`, one voxel body
diagonal) already makes floating-point noise at the boundary irrelevant, and
an epsilon here would itself be a new tolerance. The comparison is against the
wall the caller stated, which in production is bounded below by
`checked_wall_mm`, never against a number chosen to let a shell through.

**The refusal, and the fallback.** A new `HollowError::RealizedWallBelowSpec {
realized_wall_mm, requested_wall_mm }`; `run` converts it into
`HollowStageOutput::Solid` with a new `SolidDecisionReason::ShellUnderWalled`,
through the same `solid_decision_recording_repair` machinery every other
shell-stage refusal already uses — a hollow refusal, not a model rejection, as
the ruling requires. `hollow_error_refuses_this_geometry`
(`plinth-pipeline/src/prepare.rs`) gained the matching arm so that if the
error ever did propagate rather than convert, "keep the solid" is still what
it means; the match is exhaustive with no `_` arm, so this was a compile
error until it was added.

**The evidence, in three places.** `realized_wall_mm: Finite` on
`CavityEvidence` (populated only on a shell that passed the check — a shell
that misses it never gets a `CavityEvidence` at all); `realized_wall_mm:
Option<Finite>` on the top-level `HollowReport`, `#[serde(default)]` for the
same forward-compatibility `cavity_repair` established, populated by
`drilling::into_hollowed_model` on the hollowed path and by `run`'s
`ShellUnderWalled` arm on the discarded path; and both numbers named in the
error's own text. This field is distinct from FU-37's still-unbuilt `wall`
(the requested figure) — FU-37 lands next, beside this one on the same
`HollowReport`.

**Measured on the committed Sphere.** At the registered 2.5 mm wall,
`realized_wall_mm >= 2.5` — consistent with FU-53's own analytic measurement
of the same fixture. **The check firing on real geometry, not just on a
hand-built error:** a locally-built `HollowSettings` with
`cavity_march_margin_mm` forced to `0.0` (never a change to the registered
constant — `create_shell`'s own doc names a local settings value the
sanctioned way to exercise this in tests) reproduces the exact defect FU-53
fixed — the surface-nets extractor placing the cavity boundary vertex outside
the selected voxel's own measured distance — and the Sphere's shell refuses
with `RealizedWallBelowSpec` before the (deliberately starved) Boolean stage
is ever reached.

**What this change does NOT do.** It does not relax, widen or epsilon the
comparison; it does not touch FU-53's selection math, `frozen_distance_transform`,
`marching_cavity`'s geometry, or the Boolean/volume-balance validation path.
It does not extend to bore penetration — the check runs on the pre-drill
shell, and FU-61 owns whether a drain or vent bore fully opens. It is
vertex-sampled, not a continuous minimum over the cavity surface — dense at
the frozen 0.5 mm voxel pitch, and documented as a methodology rather than
claimed as a proof. No existing cap, gate, floor or tolerance moved; no
constant changed. `cargo test --workspace` (all binaries, including the
release-gated 429,120-face Treant fixture, run separately under `--release`)
and `cargo clippy --workspace --all-targets` are both clean.

## 2026-08-12 — the shipped wall becomes the requested wall: cavity selection moves from the EDT's conservative overestimate to the exact nearest-surface distance, plus a provable marching margin {#hollow-exact-distance-wall-fix-2026-08-12}

<a id="hollow-exact-distance-wall-fix-2026-08-12"></a>

**FU-53 lands**, mandated and prioritized by the owner's 2026-08-12 ruling on
FU-55 ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§1): "the objective is hollowing properly, not refusing gracefully." This is
the fix, not a bounded interim gate — FU-55's realized-wall check is separate,
unbuilt, and not part of this change.

**The defect, restated precisely.** `create_shell_recording_repair` selected
the cavity as `edt(v) > wall`, where `edt` is a voxel-lattice Euclidean
distance transform computed on the *conservatively rasterized* occupancy — a
voxelization that marks every voxel a triangle passes through, so `edt`
measures to background voxel *centres* that sit outside the true surface by
construction. Two independent overestimates stacked: the EDT's own
conservative bias, and the surface-nets extractor emitting the cavity boundary
half a voxel outside the outermost selected voxel centre. On smooth primitives
this cost 0.55–0.65 mm; on oblique, concave, branched organic geometry it cost
1.2–2.2 mm, and past its limit it opened the cavity straight through the wall
(`InvalidShellTopology` in 13 of 15 synthetic thickness/direction
configurations, non-monotonically — the signature of lattice aliasing).

**The fix, in the three parts the design called for.**

1. **`frozen_distance_transform` is demoted from selection authority to a
   sound prefilter.** It is otherwise completely unchanged — same padding,
   same crop, same SciPy-compatible transform. The module doc above it now
   states and the lemma
   `frozen_oracle_edt_never_understates_the_exact_distance` proves
   mechanically, on `Sphere`, `Cylinder`, a box and a synthetic ramped-face
   box: **`edt(v) >= d_true(v)` for every material voxel**, because any
   straight segment from an interior point to a background voxel centre must
   cross the true surface, and a straight segment is the shortest path
   between its endpoints.
2. **Exact distance, batched, on the candidate set the lemma licenses.** New
   `exact_cavity_distance` gathers every voxel whose EDT exceeds
   `min(wall + margin, bulk_min / 2)` — the smaller of the two thresholds this
   field's two consumers need, which is the more inclusive filter and
   therefore sound for both — into one flat point buffer, builds one
   `geom_kernel::NativeSurface` over the triaged source mesh, and queries it
   in caller-sized batches (`query_working_bytes / QUERY_BYTES_PER_POINT`
   points per call) through `NativeSurface::query`'s own index-parallel
   `par_iter().map().collect()`. No per-voxel call anywhere. A single scalar
   `upper_bound` — the largest EDT value among the candidates — bounds the
   whole batch, sound by the same lemma (no candidate's true distance can
   exceed its own EDT value).
3. **Selection and the marching margin.** The cavity is now
   `material[v] && d_true(v) > wall + HOLLOW_CAVITY_MARCH_MARGIN_MM`, where
   the new registered constant is one `HOLLOW_PITCH_MM` voxel body diagonal —
   `0.5 * sqrt(3) = 0.8660254037844386 mm` — the maximum distance a
   surface-nets boundary vertex can sit from the nearest selected voxel
   centre. Adding it makes the realized wall `>=` the requested wall **by
   construction**, not by measurement after the fact.

**`bulk_mm` is honest too, for free.** The same hybrid field (exact wherever a
voxel could plausibly be the true interior maximum, a sound upper bound
elsewhere) feeds `bulk_mm = 2 * edt.max()`. It is a TIGHTENING of a gate that
was previously fed the EDT's own overestimate — some models that used to clear
`HOLLOW_BULK_MIN_MM` on a falsely generous number may now correctly not; none
in the committed corpus did.

**Registered.** `HOLLOW_CAVITY_MARCH_MARGIN_MM` is a new `Placeholder`
constant (`crates/plinth-core/src/constants/placeholders.rs`), registered in
the ledger, `constants.md` (regenerated, not hand-edited), and the descriptor
count pins (`272 -> 273` placeholders, `303 -> 304` total). `HOLLOW_WALL_DEFAULT_MM`
was **not** touched — FU-53's own standing instruction, reaffirmed by the
FU-55 ruling.

**A new, independent H0 admission domain.** The exact-distance `NativeSurface`
is a new BVH + parity-grid work domain, so `H0AdmissionLimits` grew a caller-owned
`max_cavity_distance_index_bytes` (validated in `new`, rejecting 0 and
`usize::MAX` like every sibling limit; defaulted in `production()` to 512 MiB),
refused with a new typed `HollowError::CavityDistanceIndexLimitExceeded` —
no fallback to the EDT-only selection this domain exists to move H0 off of.
`H0ResourceDiagnostics` gained `cavity_distance_index_bytes` and
`cavity_distance_exact_query_points` so the new work is visible in the record.
This is a bound ADDED, not a bound loosened.

### The measured before/after

Realized wall (brute-force minimum distance from every cavity-shell vertex to
the source surface), against the registered 2.5 mm default wall:

| model | committed? | realized wall before FU-53 | realized wall after FU-53 | cavity volume before | cavity volume after |
| --- | --- | --- | --- | --- | --- |
| `Sphere` | yes | 1.85 mm (74%) | **>= 2.5 mm, proven by construction; measured minimum >= requested** ([`the_analytic_sphere_reaches_its_requested_wall`](../../crates/plinth-hollow/src/lib.rs)) | ~2,087 mm3 (~42% of source) | **1,221.375 mm3 (29.3% of 4,174.75 mm3 source)** |
| `Cylinder` | yes | 1.95 mm (78%) | not re-measured by brute force; the lemma + margin guarantee applies identically | — | 9,958.125 mm3 (47.0% of 21,175.76 mm3 source) — still clears savings |
| `Weeping Treant v1.2` (committed fixture) | yes | 0.70 mm (28%) | the honest cavity no longer clears the registered savings floor: `IneligibleSavingsAfterCavityRepair` at production settings; the pipeline substitutes the solid reserve (mandated by FU-55's ruling, not a defect) | 8,196.5 mm3 | 3,118.75 mm3 (measured with `save_min` relaxed, to exercise the repair through the real production entry point independent of the unrelated savings gate — see the repin note on `the_committed_treant_hollows_and_its_repair_is_the_one_the_record_states`) |
| `Warhorse.stl` (raw mini) | no | 1.27 mm (51%) | **not re-measured** — `minis-to-process/` is not present in the worktree this landed from; open question below | — | — |
| `Giant Toad.stl` (raw mini) | no | 0.27 mm (11%) | **not re-measured**, same reason | — | — |
| `Awakened Tree.stl` (raw mini) | no | 0.49 mm (19%) | **not re-measured**, same reason | — | — |
| synthetic ramped-face box (this fix's own reconstruction of the undocumented breach fixture) | procedural | breach (`InvalidShellTopology`) in 13/15 configs | **zero breaches across a 10-rise x 2-axis, 20-configuration sweep** (`the_ramped_box_breach_class_is_gone`) | 0 (breached) | varies by configuration; every configuration either hollows cleanly (0 cut faces) or refuses for a named, different reason |

**Open question, recorded rather than guessed at:** the three raw minis
(`Warhorse.stl`, `Giant Toad.stl`, `Awakened Tree.stl`) live under
`minis-to-process/`, which is read-only project input and was not present in
the worktree this change landed from. Their post-fix realized wall is
therefore **not re-measured** here. The mechanism guarantee (lemma +
`wall + margin` selection) applies to them identically to every other model —
there is no model-specific carve-out in `exact_cavity_distance` — but the
concrete numbers are unmeasured and should not be assumed. Re-running the
FU-53 diagnosis harness against them, when they are next available, closes
this row.

**Test coverage landed with the fix**, all debug-runnable except the
release-gated corpus row:

1. **The lemma**, mechanically proven on four fixtures (`Sphere`, `Cylinder`,
   a box, a synthetic ramped-face box) —
   `fu53_exact_cavity_distance::frozen_oracle_edt_never_understates_the_exact_distance`.
2. **The analytic Sphere** — cavity radius within one pitch of the analytic
   `R - (wall + margin)`, and the measured realized minimum wall `>=` the
   requested wall — `the_analytic_sphere_reaches_its_requested_wall`.
3. **The breach class** — a 20-configuration sweep of the reconstructed
   ramped-face box, asserting `InvalidShellTopology` never fires again —
   `the_ramped_box_breach_class_is_gone`.
4. **The new admission limit** — a 1-byte `max_cavity_distance_index_bytes`
   refuses typed, with no fallback —
   `a_too_small_cavity_distance_index_limit_is_a_typed_refusal_not_a_fallback`.
5. **Thread sweep** — `shell_digest_is_bit_identical_across_one_four_and_sixteen_kernel_workers`
   already parametrizes over `query_working_bytes` (127 and 4,096-point
   batches) as well as 1/4/16 `PLINTH_THREADS`, and continues to pass
   unmodified: the new batched exact-distance query is covered by the same
   bit-identity assertion the shell digest already carried.
6. **The committed Weeping Treant v1.2**, release-gated
   (`the_committed_treant_hollows_and_its_repair_is_the_one_the_record_states`):
   re-pinned critical cells 52 -> 116, repaired voxels 67 -> 170, cell visits
   5,405,496 -> 5,406,320, cavity volume 8,196.5 -> 3,118.75 mm3, all measured
   with `save_min_mm3`/`save_min_fraction` relaxed to isolate the
   well-composedness repair (this test's own subject, FU-49) from the
   unrelated savings-gate consequence recorded in the table above.

**Frozen numbers re-pinned, every one with a `from -> to` note at its call
site**, because the cavity moved everywhere a wall is selected against it:
`drains.rs`'s `frozen_shell_field_has_an_enclosed_drain_and_vent_plan` (drain
`z` 2.5 -> 3.5 mm, vent centre and realized exit wall 3.0 -> 4.0 / 3.5 -> 5.0
mm), three `drilling.rs` fixtures keyed to the Sphere's cavity shape
(`frozen_sphere_is_drilled_once_and_every_voxel_drains`,
`frozen_trapped_pocket_policy_refills_a_long_exit`,
`production_drilling_succeeds_with_exact_refill_exclusion_and_public_report`),
the synthetic pinched-dumbbell fixture (rod radius retuned 3.375 -> 3.875 mm
to keep reproducing its lattice-diagonal defect at the new selection
threshold — the defect is non-monotonic in rod radius, the same aliasing
signature the breach class shows), and two `plinth-pipeline` integration
tests whose two-model folder scenario (`mini` + the doubly-processed
`Sphere.prepped.stl` fixture) now crosses the sealed run's registered support
routing capacity before the gate is reached, converting a terminal
`UncoveredIslands` gate refusal into a `DrilledShellRefusedBySupport` reserve
substitution — a real, measured, cap-untouched consequence of the geometry
moving, attributed in full at both test sites
(`crates/plinth-pipeline/tests/prepare_folder.rs`).

**What this change does NOT do.** It does not touch
`distance_transform_edt`, `crop_margin_in_place`, `marching_cavity`,
`label3d`, the well-composedness repair, or the Boolean/volume-balance
validation path — the marching path and the voxel-count volume accounting
stay structurally untouched, per the design's risk note, so
`hollow_volume_balance_rel_tol` keeps its present character. It does not
lower `HOLLOW_WALL_DEFAULT_MM`. It does not build FU-55's realized-wall gate
— that row stays open, in a different lane, as interim work the ruling
separately mandated. No cap, floor, gate or tolerance moved; one bound was
added (`max_cavity_distance_index_bytes`).

## 2026-08-12 — the frozen rasterizer is replaced with an area-proportional conservative one, and `Round 50`/`Round 100` rasterize under the unchanged cap {#hollow-raster-rewrite-2026-08-12}

<a id="hollow-raster-rewrite-2026-08-12"></a>

**FU-50 lands.** Per the owner's ruling
([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md),
§3), `walk_raster_leaves` — the frozen midpoint-quartering rasterizer that
reproduced `trimesh.voxelized(pitch).fill()`'s default `subdivide` voxelizer —
is replaced by `walk_raster_candidates`, an area-proportional conservative
rasterizer: per triangle, candidate voxel cells are enumerated by walking the
columns of the triangle's dilated index-space bounding box along the two axes
orthogonal to its dominant normal axis, narrowing each column to the plane's
own reachable span on the third axis, then each candidate is exactly tested
against the voxel's cube with the 13-axis separating-axis test (Akenine-Moller
2001 / Ericson RTCD §5.2.9). **WORK ITEM = one candidate cell tested**, not one
cell marked, so the two-pass count/mark contract (`raster_work_item_count`,
`voxelize_filled`, `RasterWorkMismatch`) is untouched, and `max_raster_work_items`
(16,777,216) is not touched, raised or reinterpreted.

**The ruling's precondition — proof before the flip.** OLD's marked set is a
strict subset of NEW's: every old leaf vertex is a convex combination of the
original triangle's vertices, so it lies on the triangle, and rounding it to
the nearest grid point (ties-to-even) puts it within 0.5 of that voxel's
center on every axis — inside the voxel's own cube — which the new exact SAT
test (boundary-inclusive) always marks too. This is checked mechanically two
ways: the in-crate test
`frozen_walker_marks_a_subset_of_the_area_proportional_rasterizer` (a retained
`#[cfg(test)]`-only copy of the old walker, run against the real production
`walk_raster_candidates`, on `make_box`, `Cylinder`, `Sphere`, `make_thin_fin`,
`make_raking_blade`), and the read-only measurement harness
`crates/plinth-hollow/examples/raster_delta.rs` below, which independently
reproduces both walkers and reports the full delta on the eight committed
corpus models plus `Round 50.stl`/`Round 100.stl`. `|old \ new|` is **0** on
every one of the ten models measured.

**The measured delta** (`cargo run --release --offline --example raster_delta
-p plinth-hollow -- --model ".../Round 50.stl" --model ".../Round 100.stl"`;
pitch 0.5 mm; cap 16,777,216; `Round 50`/`Round 100` measured on the raw,
uncommitted `minis-to-process/` STLs — not the pipeline's triaged/oriented
form, since the point of this table is the rasterizer's own work, which does
not depend on orientation):

| Model | old items | old / cap | new items | new / cap | \|old marked\| | \|new marked\| | \|new∖old\| | \|old∖new\| |
|---|---|---|---|---|---|---|---|---|
| Cylinder | 3,503,104 | 0.2088x | 153,765 | 0.0092x | 20,058 | 20,058 | 0 | 0 |
| make_box | 49,152 | 0.0029x | 5,292 | 0.0003x | 2,402 | 2,402 | 0 | 0 |
| make_overhang_table | 933,888 | 0.0557x | 61,661 | 0.0037x | 17,405 | 18,363 | 958 | 0 |
| make_raking_blade | 205,824 | 0.0123x | 15,923 | 0.0009x | 5,264 | 5,866 | 602 | 0 |
| make_thin_fin | 294,912 | 0.0176x | 22,015 | 0.0013x | 6,774 | 7,457 | 683 | 0 |
| make_toed_slab | 468,992 | 0.0280x | 43,472 | 0.0026x | 11,667 | 12,515 | 848 | 0 |
| Sphere | 230,720 | 0.0138x | 161,491 | 0.0096x | 6,964 | 7,584 | 620 | 0 |
| Weeping Treant v1.2 | 4,454,823 | 0.2655x | 9,406,259 | 0.5607x | 74,219 | 79,974 | 5,755 | 0 |
| **Round 50** | **19,291,472** | **1.1499x (REFUSED)** | **418,730** | **0.0250x** | 21,304 | 21,794 | 490 | 0 |
| **Round 100** | **22,486,336** | **1.3403x (REFUSED)** | **443,767** | **0.0265x** | 73,466 | 74,698 | 1,232 | 0 |

`old items` for `Round 50`/`Round 100` reproduce the FU-50 row's own
opening-section figures (19,291,472 / 22,486,336) exactly, cross-confirming
the harness's retained OLD walker against the earlier measurement. **This is
the ruling's acceptance test, satisfied**: both models now rasterize at
~2.5–2.6% of the unchanged cap, comfortably clear of the 1.15x/1.34x that used
to refuse them with `RasterWorkLimitExceeded`, and the in-crate loud-skip test
`round_50_and_round_100_rasterize_under_the_unchanged_production_cap`
(`#[ignore]`d by default, same pattern as `plinth-decimate`'s
`local_minis_repair`; run with `cargo test -p plinth-hollow --release --lib
round_50_and_round_100 -- --ignored --nocapture`) asserts it directly against
the real production `raster_work_item_count`.

**What moved, and which direction.** `|new marked| ≥ |old marked|` on every
row (equal only on `Cylinder` and `make_box`, whose triangles are large,
axis-aligned and land exactly on grid points either way). Post-fill material
therefore grows too, the EDT reads marginally deeper, and the selected cavity
gains voxels — recorded here, as the ruling requires, in the direction it
actually moved rather than presented as a wall improvement: **occupancy grows,
so the realized wall gets marginally thinner** until FU-53 lands. Two frozen
numbers move for exactly this reason and are re-pinned with `from -> to` in
the commit and here:

- `sphere.cavity_voxels` (the frozen-oracle characterization test):
  **16,491 -> 16,697**.
- The committed sphere's realized drain/vent geometry (`drains.rs`'s
  `frozen_shell_field_has_an_enclosed_drain_and_vent_plan`): the drain's
  centre `y` moves **-0.0299 mm -> 0.0 mm** (the old rasterizer's sampled-
  vertex marking carried a stray asymmetry the new exact marking does not —
  a second-order benefit, not a regression), the vent's centre moves
  **(-2.5, -4.5, 16.0) -> (-0.5, -5.0, 16.0)** and its realized exit wall
  **4.0 mm -> 3.5 mm** (a different equal-cost lateral direction on this body
  of revolution, sampling a thinner wall).
- `Weeping Treant v1.2`'s well-composedness repair
  (`the_committed_treant_hollows_and_its_repair_is_the_one_the_record_states`,
  release-gated): `cavity_critical_cells` **70 -> 52**,
  `cavity_repaired_voxels` **115 -> 67**, `cavity_repair_cell_visits`
  **5,405,880 -> 5,405,496**, `cavity_volume_mm3` **8,043.875 -> 8,196.5**
  (64,351 -> 65,572 voxels). The critical-cell count is a property of the
  *local pinch pattern* the exact marking produces, not a monotone function of
  the (monotonically growing) marked set, so a fall here and a rise on the
  synthetic pinched-dumbbell fixture below are both consistent with the
  superset proof.
- The synthetic pinched-dumbbell fixture
  (`a_synthetic_pinched_dumbbell_needs_the_repair_and_hollows_because_of_it`):
  the unrepaired mask's non-manifold/point-only face counts move
  **(0, 8, 26) -> (0, 16, 50)**, `cavity_critical_cells`/`cavity_repaired_voxels`
  **4 -> 8**, `cavity_repair_cell_visits` **411,674 -> 411,706**.
- Two Sphere-fixture drilling numbers move with the same widened cavity:
  `frozen_trapped_pocket_policy_refills_a_long_exit`'s `refilled_voxels`
  **7,856 -> 7,957** (and its refill span's far end **59,841 -> 59,842**);
  `production_drilling_succeeds_with_exact_refill_exclusion_and_public_report`'s
  `refilled_voxels` **490 -> 499**.

**`corpus/baseline/Sphere.pipeline.json` is untouched** — FU-51 governs its
regeneration and it is blocked on FU-61 (bore-penetration verification), not
opened by this change. Every re-pinned number above is the *engine's own*
measurement (via `create_shell`/`plan_drains`/`drill_planned_shell` on the
committed STL), never the frozen artifact.
**Superseded by the later, same-day FU-61/FU-51 section above:** FU-61 has
since confirmed, and the baseline's drain/vent placement fields (only —
`cavity_voxels` and every other field this section's numbers describe are
untouched by that re-record too) have had their one deliberate regeneration.
The "is untouched" claim above described this landing's own moment, not the
day's final state.

**Dead code removed with the recursion**: `RasterNode`, `raster_leaf`,
`edge_too_long`, `midpoint`, `mark_voxel_vertex`, `walk_raster_leaves`,
`MAX_RASTER_SUBDIVISION_ROUNDS`, `RASTER_STACK_CAPACITY`, and the now-
unreachable `HollowError::RasterSubdivisionLimitExceeded` — including its arm
in `plinth_pipeline::prepare::hollow_error_refuses_this_geometry` and the
exhaustive-match test that pinned it. Removing an unreachable variant loosens
nothing; the cap that binds (`max_raster_work_items`) is untouched. The
`voxelize_filled` doc block's claim of `trimesh.voxelized(pitch).fill()`
`subdivide`-voxelizer parity as a frozen source is corrected: the owner lifted
that freeze in this ruling, and the doc now cites the decision record instead.

**Perf, in passing.** The new rasterizer is not uniformly cheaper in raw work
items — `Weeping Treant v1.2` (429,120 triangles, most of them small) costs
**more** new work items than old (9,406,259 vs 4,454,823, 0.561x vs 0.266x of
cap), because a very high triangle count carries a small per-triangle fixed
candidate-generation cost that the old algorithm's per-triangle recursion did
not. It is still comfortably under cap and the whole point — `Round
50`/`Round 100`'s huge slivers, the actual failure mode — is fixed by roughly
two orders of magnitude. This is recorded honestly rather than claimed away:
the ruling's own text anticipated demand is not perfectly uniform per-model,
only that the *worst-case-dominating* pathology (a single huge, thin triangle)
stops being quadratic in edge length.

**Files**: `crates/plinth-hollow/src/lib.rs` (rasterizer, dead-code removal,
re-pinned tests, new tests), `crates/plinth-hollow/src/drains.rs` (re-pinned
sphere drain/vent test), `crates/plinth-hollow/src/drilling.rs` (re-pinned
refill tests), `crates/plinth-hollow/examples/raster_delta.rs` (new
measurement harness), `crates/plinth-pipeline/src/prepare.rs` (dead match arm
removed).

---

## 2026-08-11 — the cavity mask is made well-composed, FU-49 closes, and the wall the shells actually ship is measured for the first time {#hollow-well-composed-2026-08-11}

<a id="hollow-well-composed-2026-08-11"></a>

**FU-49 is fixed here, exactly along the route FU-49 itself prescribed; a fourth defect
is found, measured and filed.** FU-49's recorded first action was: *count the critical
cells on the `Warhorse` and the treant — if it is tens, remedy (a) is bounded and cheap
and should be measured to a fixpoint; if it is thousands, remedy (b) is the honest
answer.* Counted: **44** on the raw `Warhorse.stl` and **70** on the committed
`Weeping Treant v1.2` fixture. Tens. Remedy (a) is therefore the sanctioned path and it is
what landed. (**Corrected 2026-08-11 on review.** This sentence first said *59* for the
treant, which is the raw `Awakened Tree.stl` in its delivered pose — the same provenance
slip the tables below already correct, restated at the top of the entry and missed by that
sweep. The committed fixture's figure is 70, asserted by the release-gated test. Either
number is tens, so the decision the sentence records is unchanged.)

### DEFECT — FIXED: one vertex per lattice cell is not enough where the mask is not well-composed

`fast_surface_nets` emits **at most one vertex per lattice cell**. That suffices only when
the mask is *well-composed* — when no 2×2×2 cell holds a face-diagonal foreground pair
(Latecki's C1) or an antipodal foreground/background pair (C2). Where it does not, two
sheets of the surface are forced to share the cell's single vertex and the edges around it
carry four faces instead of two. The cavity comes out watertight, consistently oriented,
and **non-manifold**; the Boolean inherits it; `validate_shell_topology` correctly refuses
its own shell with `InvalidShellTopology`.

`crates/plinth-hollow/src/well_composed.rs` repairs the selected cavity component to
well-composedness **by removing cavity voxels only, never adding them**. That direction is
the whole safety argument: removing cavity turns void back into material, so the wall can
only get *thicker* and a repair can never breach the registered wall thickness. Adding
cavity would break the same configurations while thinning the wall at precisely the pinch
points where it is already thinnest — a wall violation dressed as a topology fix. The wall
constant is neither read nor written by the module.

**Termination needs no new budget.** Removal is monotone on a finite set: every round that
does not stop removes at least one voxel and the foreground only shrinks.

**Work and memory, corrected 2026-08-11 to what the code actually guarantees.** The first
version of this section claimed a single bound, `cells + 8 × changed` cell visits, and that
claim was *narrower than the algorithm*. It is now stated as the two domains it really is,
and both are counted rather than asserted:

- **Cell visits ≤ `cells + 8 × changed`.** One full scan of `(nz+1)(ny+1)(nx+1)` cells, after
  which only the eight cells incident to each *changed* voxel are re-examined. Reported as
  `H0ResourceDiagnostics::cavity_repair_cell_visits`, and pinned end to end on the committed
  treant, where it is exactly `5,404,960 + 8 × 115 = 5,405,880`.
- **Whole-grid passes**, which the first bound did not cover at all. A round that removes
  anything re-selects the component with a `label3d` plus a whole-grid sweep, and the number
  of such rounds is bounded only by the removals, so total work is
  `cells × (2 × passes − 1) + 8 × changed`, not `cells + 8 × changed`. A re-selection either
  drops a voxel or ends the loop, so `passes ≤ 2 + dropped`; **measured, it is 2 on every
  model available**, which is why nothing observable moved. Reported as
  `cavity_repair_full_grid_passes` and pinned at 2 on the treant and on the synthetic
  dumbbell. (**Corrected 2026-08-11 on review, twice over.** The factor: each counted pass is
  *two* whole-grid traversals — the `label3d` and the `indexed_iter` sweep that applies its
  verdict — so the first form, `cells × passes + 8 × changed`, undercounted the re-selection
  domain by about 2×. And the bound: `passes ≤ 2 + dropped` is `O(N)` and is **not an
  admitted limit**, which makes this the one work domain in H0 a caller cannot cap. That gap
  is **FU-54**, filed with the measured profile rather than closed with a number of this
  change's own invention.)
- **Transient memory ≤ ~6 bytes per admitted voxel.** The first version held the eight
  incident cells of every changed voxel as a sorted `Vec<[usize; 3]>` — 192 bytes per changed
  voxel, ~3.2 GB at the admitted cap of 16,777,216 cells, uncounted and with no admitted
  limit of its own, in a stage where every other work domain has one. That set is now
  enumerated lazily and never materialised; what is held is a reused one-byte-per-voxel dedup
  flag array and the changed voxels themselves as `u32` **linear indices**, four bytes each.
  Both are the same order as the `Array3<i32>` label array `label3d` already builds over the
  same grid for the caller. Duplicate incident cells are visited instead of deduplicated,
  which is free because the removal rule is a pure function of the cell's eight bits — and it
  is what makes the cell-visit count exactly `8 × changed` per incident round.

**No constant, cap, floor, gate threshold or registry entry is introduced, raised,
reinterpreted or bypassed.** Bounding the transient is a *tightening*; nothing was relaxed to
accommodate it.

The removal rule is a **pure function of the cell's 8-bit configuration**, so it is
order-free and needs no tie-break; a `0..256` test asserts that, and asserts no background
corner is ever marked.

### What it bought, measured on every model available — **and on which mesh, in which pose**

**Corrected 2026-08-11.** The first version of this table recorded the treant's row under
the name of the **committed** fixture, `Weeping Treant v1.2`, and asserted it was the same
thing as `Awakened Tree.stl`. It was not measured on the committed fixture, and the row it
stated is arithmetically impossible for that fixture: the repair's opening full scan alone
is `166 × 176 × 185 = 5,404,960` cell visits on that grid, so the recorded 3,561,128 is
*below one scan* of it. The identity is nearly true and that is exactly what made the error
easy to make — `Awakened Tree.stl` and the committed fixture have the same 429,120 faces and
are the same source mesh — but they are **different poses**, so they have different bounding
boxes (`165×109×194` vs `165×175×184`), different grids (3,489,090 vs 5,313,000 cells) and
therefore different cavities. Every row now names the file it was measured on and whether
that file is committed evidence or an uncommitted raw mini.

Release, `create_shell` internals. Cost is the fraction of the selected cavity the repair
hands back as material:

| mesh, as measured | committed? | grid | critical cells | voxels removed | cavity voxels | cost | inner surface before → after |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `corpus/baseline/Weeping Treant v1.2.prepped.oriented-solid.stl` | **yes** — committed evidence, oriented-solid pose | 165×175×184 | 70 | 115 | 64,351 | **0.18 %** | 132 non-manifold / 483 point-only → **0 / 0** |
| `Warhorse.stl` | no — raw mini, as delivered, unoriented | 39×121×109 | 44 | 51 | 27,033 | **0.19 %** | 92 / 276 → **0 / 0** |
| `Giant Toad.stl` | no — raw mini, as delivered, unoriented | 101×97×62 | 59 | 63 | 114,990 | **0.055 %** | 122 / 380 → **0 / 0** |
| `Awakened Tree.stl` | no — raw mini, the treant's source mesh in its delivered pose | 165×109×194 | 59 | 66 | 63,958 | **0.10 %** | 124 / 404 → **0 / 0** |

Cavity volumes: committed treant **8,043.875 mm³**, `Warhorse` 3,379.125 mm³, `Giant Toad`
14,373.75 mm³, raw `Awakened Tree` 7,994.75 mm³. Cell visits 5,405,880 / 537,208 / 630,252 /
3,561,228 — in every case exactly the grid's own `(n+1)³` opening scan plus `8 × changed`,
which is the bound, arrived at rather than remembered. Whole-grid passes: **2** on all four.

**The committed row is now a test, not a recollection.**
`plinth-hollow`'s `the_committed_treant_hollows_and_its_repair_is_the_one_the_record_states`
runs `create_shell` on that fixture under `cargo test --release` and asserts every number in
its row, plus that the shell it produces carries zero cut flags of any kind. The two raw-mini
rows cannot be tested, because those files are not in the repository and never will be; they
are recorded here as what they are — local verification material.

**Independent corroboration of the committed row.** The 2026-08-10 entry's own table records
the treant's pre-repair cavity as 8,058.25 mm³, which is 64,466 voxels at the frozen 0.5 mm
pitch. `8,058.25 − 115 × 0.125 = 8,043.875`, exactly. The two entries now agree on the same
mesh; before this correction they described two.

**Corpus-wide, all 37 minis plus the 8 committed corpus models: `InvalidShellTopology` is
gone — zero refusals in that class remain.** Every remaining refusal is a different and
already-owned class: `NotClosedOrOriented` (raw input defects, the repair-pre-pass work),
`IneligibleBelowBulk`/`IneligibleSavings` (the savings gates doing their job), and
`RasterWorkLimitExceeded` on `Round 50`/`Round 100` (FU-50, untouched). `Sphere`,
`Cylinder` and every simple primitive report **0 critical cells** and are byte-for-byte
unaffected — the repair is inert where the mask was already well-composed, which is what
makes the attribution clean.

### The repair is now reportable, and a repair-caused solid decision is distinguishable from a shape that has no cavity

Added 2026-08-11 on review. The first version of this change measured the repair and then
**threw the measurement away exactly where it decided how a model was treated.** Three
problems, all fixed here, none of them touching a threshold:

1. **The diagnostics were unreachable.** `cavity_critical_cells`,
   `cavity_repaired_voxels` and `cavity_repair_cell_visits` lived on
   `H0ResourceDiagnostics` and had **no consumer anywhere outside `plinth-hollow`**, so no
   emitted artifact and no report could say a repair had run. They now travel the whole way
   out: `HollowReport::cavity_repair` on the drilled path and on the refusal paths alike,
   `ChainOutcome::cavity_repair` and `ModelPreparationOutcome::Artifact { cavity_repair }` in
   the prepare report, and one `cavity repair` line from `plinth prepare` when a repair
   actually took something — joined by the newly counted `cavity_repair_full_grid_passes`.
2. **A repair that ate the cavity was reported as geometry.** When the fixpoint removed
   everything, the module returned `IneligibleNoCavity`, which `plinth_hollow::run` converts
   to `SolidDecisionReason::NoCavity` — *"this shape has no room for a cavity"*. It is not the
   same finding, and the `prepare.rs` caveat about `NoCavity`/`BelowThreshold` imprecision
   predates the repair and does not cover it. There are now two typed refusals,
   `CavityRepairRemovedCavity` and `IneligibleSavingsAfterCavityRepair`, carrying the repair's
   own numbers, and two matching reasons, `NoCavityAfterRepair` and
   `BelowThresholdAfterRepair`. A cavity that fell under the savings gate *after* the repair
   shrank it no longer reads as a cavity that was always too small.
3. **The measurement FU-49's decision procedure turns on was `#[cfg(test)]`.** Counting the
   critical cells is what chose remedy (a) over remedy (b); a number nothing in production
   reports is a number nobody can check, which is how the treant row above came to state a
   figure no committed input can produce. `critical_cell_count` remains the tests'
   *independent* oracle, and the count production reports is the repair's own — now on the
   record and now pinned end to end by the release-gated corpus test.

### Second review round, 2026-08-11: the evidence seam, the default test run, and the transient nobody counted

The round above carried the measurement to the record on *five* named refusal codes and left
every other exit bare. Four findings, all fixed here; a fifth is filed and a sixth is an
owner decision this branch must not make for itself. **No threshold, cap or bound moved.**

1. **The refusal record was still repair-blind on most paths.** `plinth_hollow::run`
   converted five `create_shell` errors into typed outputs and propagated everything else
   naked — `InvalidShellTopology` (FU-53's finned-block class is live in exactly this path),
   `DrainPlanning`, `Drilling`, `Solid`, `InvalidProductionMesh` — and `prepare.rs` then built
   `SupportedGeometry::HollowStageRefused` through a constructor that hard-coded
   `cavity_repair: None`. A repair that removed cavity voxels was invisible in the one record
   that decided the model's treatment, which is the failure mode this measurement exists to
   prevent. `create_shell_recording_repair` now returns a typed **`HollowStageFailure`** —
   the error plus what the repair had already measured — written as one fallible block with a
   single exit that attaches the evidence, so the fix is not "one `map_err` per call site"
   and cannot be defeated by a future call site nobody remembered. `run` and
   `prepare::run_from_oriented` carry it onto the substituted record. `create_shell` itself
   keeps its `HollowError` signature and every existing caller is untouched.
2. **The support-refusal substitution had the same hole**, and two comments asserted it did
   not. `prepare.rs` built the oriented-solid reserve with `cavity_repair: None` *after*
   `create_shell` had succeeded — so a repair may well have run — while
   `ModelPreparationOutcome::Artifact`'s doc said `None` means "a stage earlier than cavity
   selection decided it" and `into_solid_fallback`'s comment said "the repair's measurement
   already travelled out with that shell's own report". Both were false on that branch,
   because the shell's report was discarded with the `seal_and_emit` that failed. The
   substitution now carries the shell's measurement and both comments say what the code does.
   `None` is once again a claim: nothing reached cavity selection.
3. **A default `cargo test` ran no end-to-end coverage of the fix at all.** The only test
   exercising the repair through `create_shell` is release-gated on a 429,120-face fixture;
   `well_composed.rs`'s twelve unit tests stop at the mask or at `marching_cavity`; and both
   debug-runnable `create_shell` tests run masks with **0 critical cells**, in which the
   repair is inert. The spec asked for a small synthetic *solid* and only a synthetic mask was
   built. `a_synthetic_pinched_dumbbell_needs_the_repair_and_hollows_because_of_it` closes it
   in **0.9 s**: two 10 mm lobes joined by a 3.375 mm waist at **45° in the xy plane**, where
   the diagonal is the point — at a 2.5 mm wall the waist's cavity is a sub-voxel thread, so
   the mask crosses it as a chain of lattice-diagonal voxels, which is Latecki's C1/C2
   exactly. Every constant is exact in binary. It asserts the chain in order: the unrepaired
   cavity surface is watertight but **non-manifold (0 boundary / 8 non-manifold / 26
   point-only)**, which is the FU-49 refusal; the repair is 4 critical cells, 4 voxels,
   411,674 cell visits = the 99×99×42 opening scan + 8×4, 2 whole-grid passes; and the shell
   that emerges from the marching, the Boolean and `validate_shell_topology` carries **0 cut
   faces** and exactly **one negative contained cavity**. The middle assertion is what keeps
   it a regression test rather than a tautology.
4. **`frozen_distance_transform`'s transient peak was ~2× what the 2026-08-10 entry
   recorded** — see the correction appended to that entry. Fixed by cropping in place.
5. **The repair's whole-grid domain has no admitted bound, and the work formula undercounted
   it ~2×.** `full_grid_passes` is bounded only by `2 + reselection_dropped_voxels`, and
   `dropped` can be `O(N)`, so an admitted 16,777,216-cell mask has a worst-case `O(N²)` work
   profile with no typed refusal — in a stage where every other work domain has an admitted
   limit and FU-50 exists precisely because a work cap was *respected* rather than raised.
   Separately, each counted pass performs **two** whole-grid traversals (the `label3d` and the
   sweep that applies its verdict), so `p` passes are `2p − 1` traversals and the documented
   `cells × p + 8 × changed` was low by about a factor of two. **The formula is corrected in
   the module doc, the accessor docs and the test that asserts the bound.** The missing cap is
   **filed as FU-54, not invented here**: choosing that number is an acceptance-bound decision,
   the hard constraint on this work is that a fix which seems to need a bound is filed rather
   than improvised, and the measured profile is 2 passes on every model available.
6. **An owner ruling is outstanding and this branch should not merge without it — see
   FU-55.** The branch knowingly converts three previously fail-closed models into emitted
   `.chitubox` artifacts whose measured realised wall is 11–51 % of the registered 2.5 mm,
   while `PreWriteGate::Printability` is unimplemented and no production stage measures or
   gates realised wall. The journal itself says hollowed shells are not printable. Filing
   FU-53 records the defect; it does not authorise shipping under it.
   **[Superseded 2026-08-12, no line above rewritten: the branch merged (`17ae61e`) with the
   decision still unmade, and the owner has since ruled on FU-55 — under-walled shells never
   ship, a bounded realized-wall check gates hollow emission with a SOLID fallback so nothing
   becomes unprintable, and FU-53 is mandated and prioritized. The precondition this item
   states was violated; the ruling regularizes the state after the fact and the interim check
   is owed on `develop`. See
   [`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md)
   §1 and the FU-55 row.]**

### DEFECT — DIAGNOSED, FILED AS FU-53: the wall the engine actually ships is far thinner than the wall it is told to leave

Fixing the extractor let three models through to a shell, and the obvious next question had
never been asked of any hollowed model: **how thick is the wall that actually came out?**
Measured directly — brute-force minimum distance from every vertex of the marching cavity
surface to the source surface, source triangles bucketed into 3 mm cells and the answer
capped at 3 mm, which is all that matters against a 2.5 mm wall:

| mesh, as measured | committed? | realised wall | fraction of the 2.5 mm registered wall | critical cells |
| --- | --- | --- | --- | --- |
| `Cylinder` | **yes** | **1.95 mm** | 78 % | 0 — *untouched by this change* |
| `Sphere` | **yes** | **1.85 mm** | 74 % | 0 — *untouched by this change* |
| `corpus/baseline/Weeping Treant v1.2.prepped.oriented-solid.stl` | **yes** | **0.70 mm** | 28 % | 70 |
| `Warhorse.stl` | no — raw mini | **1.27 mm** | 51 % | 44 |
| `Giant Toad.stl` | no — raw mini | **0.27 mm** | 11 % | 59 |
| `Awakened Tree.stl` | no — raw mini, treant's source in its delivered pose | **0.49 mm** | 19 % | 59 |
| synthetic finned block | procedural | **breach** | 0 % | 0 |

The treant row carried the same provenance defect as the table above and is corrected the
same way: **0.49 mm was the raw `Awakened Tree.stl` in its delivered pose; the committed
fixture ships 0.70 mm.** Both are far under the stated wall and neither changes FU-53's
finding — the correction moves the treant from 19 % to 28 %, and the worst model in the
corpus is still `Giant Toad` at 11 %. Pose matters here for the same reason it mattered
above: the lattice is axis-aligned, so a rotated mesh is a differently aliased mesh.

**This is pre-existing and is not caused by the well-composedness repair.** The `Sphere` and
the `Cylinder` have **zero critical cells**, so the repair does not touch either of them, and
they already ship at 74–78 % of the stated wall. The repair only ever *removes* cavity, so it
can only ever move these numbers **up**. What this change does is newly expose three more
models to a defect that was already there — and at far worse magnitudes, because the error
grows with local surface obliquity and concavity, which is exactly what separates a smooth
primitive from a branched organic mini.

**One sentence here was wrong and is corrected.** It read "the `Sphere` and the `Cylinder` …
hollow today on `develop`". The `Sphere` does. **The `Cylinder` does not** — `develop`
(`dba1bc9`) refuses it, as the 2026-08-10 table below records in this same file, and it
hollows only as of `10725ff` on this branch, the cavity-field exterior fix. That does not
weaken the attribution, which never depended on it: what makes the repair non-causal for
these two is that both report **0 critical cells**, so no voxel of either mask is touched by
it. The under-walled `Cylinder` is `10725ff`'s to own, not this change's.

**The same defect, taken to its limit, is a second refusal cause that has nothing to do with
well-composedness.** A synthetic 20 mm block with a 20 mm fin leaving one face at an angle
refuses with `InvalidShellTopology` in 13 of 15 thickness/direction configurations — while
reporting **0 critical cells and a perfectly manifold cavity surface** (0 boundary edges, 0
non-manifold, 0 point-only). The Boolean output is watertight and manifold; what it is not is
*hollow*. Triage returns `Ok` — a plain solid with **zero enclosed cavities** — because the
cavity was cut so close to the fin's surface that it broke through and opened the void to the
outside. And the failure is **non-monotonic in fin thickness** (5.4, 5.6, 5.8, 6.2 mm refuse;
6.0 mm hollows), which is the signature of lattice aliasing rather than a clean threshold.

FU-53 carries the full evidence chain and the mechanism. It is filed rather than fixed
because the honest remedy is in the EDT/rasterization semantics — the cavity is selected by a
distance transform measured against the *conservatively rasterized* occupancy, which is
fatter than the true solid — and changing that moves every hollowed model's cavity. That is
architecturally larger than this change and it must be measured on the committed corpus
before it is chosen, not improvised beside a topology fix.

**Product consequence, stated plainly:** hollowed shells are currently under-walled, the
three models this change newly hollows are the worst affected, and a 0.27 mm wall is not a
printable wall. FU-53 is the blocker on trusting a hollowed shell to a printer, and it now
gates three more models than it did yesterday.

## 2026-08-10 — the cavity field had no exterior, and the three hollow refusal classes separate {#hollow-cavity-field-exterior-2026-08-10}

<a id="hollow-cavity-field-exterior-2026-08-10"></a>

**One defect is fixed here; two are diagnosed and filed.** The task was FU-34's first
action, asked of the `Rotting Warhorse`/`Warhorse` refusal rather than the treant's: dump
what the hollow stage actually produces and count it, then decide between "the check is
right about bad geometry" and "the Boolean/marching path is producing a broken shell". The
answer turned out to be **both, for different models**, plus a third defect that has
nothing to do with either.

### The measurement that separated them

Every number below is from `create_shell`'s own internals, release build, at `dba1bc9` +
this change. `inner` is the marching-cubes cavity surface before the Boolean; `shell` is
`from_solid(difference(source, inner))`.

| model | occupancy | EDT max | cavity | inner surface | shell |
| --- | --- | --- | --- | --- | --- |
| `make_box` 10 mm cube | **9,261/9,261** | **17.61 mm** | **1,148.5 mm³ on a 1,000 mm³ source** | closed, 1 component | REFUSED |
| `Cylinder` | 179,889/226,981 | 15.30 mm | 16,294.6 mm³, spanning z −0.25…30.25 on a 0…30 source | closed, 1 component | REFUSED |
| `Sphere` | 36,987/68,921 | 10.30 mm | 2,070.4 mm³ | closed, 1 component | OK |
| `Warhorse.stl` (raw) | 101,874/514,371 | 7.92 mm | 3,385.5 mm³, wholly interior | **0 boundary edges, 92 faces `TOPOLOGY_CUT_NONMANIFOLD`, 276 `TOPOLOGY_CUT_POINT_ONLY`** | REFUSED |
| `Weeping Treant v1.2` | 236,668/5,313,000 | 8.17 mm | 8,058.3 mm³, wholly interior | **0 boundary edges, 132 non-manifold, 483 point-only** | REFUSED |

Two rows have a cavity that is not inside the solid. Two rows have a cavity that is
perfectly placed and an inner *surface* that is not a manifold. They are different
defects and they were hiding behind one refusal string.

### DEFECT 1 — FIXED: the distance transform had no exterior to measure to

`grid_spec` spans **exactly** the source's bounding box. A Euclidean distance transform
can only measure to background samples the array contains. So every material voxel lying
on the array boundary had no background neighbour at all, and the field answered with
whatever it could reach laterally — or, when the solid filled its box completely, with a
compatibility branch that measured from an implicit sample at `(-1, 0, 0)` and returned
**17.6 mm of interior on a 10 mm cube**.

The consequence is not subtle. `edt > wall` then selects a cavity that reaches the
surface, or passes through it:

- **A 10 mm cube's "cavity" was 1,148.5 mm³ — larger than the 1,000 mm³ solid it was cut
  from.** The marching surface enclosed the source instead of sitting inside it, so the
  difference was not one negative void in positive material and H0 refused its own shell.
- **The `Cylinder`'s cavity ran from z = −0.25 to z = 30.25 through both end caps**, which
  lie exactly on the z = 0 and z = 30 grid planes. Same refusal.
- **The `Sphere` hollowed, and shipped a breached wall.** It is tangent to its bounding box
  at six points; those 72 voxels read as deep interior and were selected as cavity. The
  frozen Python artifact records the consequence in plain sight:
  `corpus/baseline/Sphere.pipeline.json` puts the drain at `z = 0.5` with `wall_mm = 1.0`
  on a solid whose bottom is `z = 0`, **under a stated 2.5 mm wall**.

The fix is one function. `frozen_distance_transform` now pads the occupancy grid with one
background layer on every side, runs the transform, and crops back to the caller's
dimensions. The margin is a transient derived from the already-admitted grid: **no cap,
threshold, floor or constant is raised, loosened or reinterpreted.** `max_voxel_cells`
still binds on the unpadded `GridSpec::count` exactly as before, the wall stays 2.5 mm,
the pitch stays 0.5 mm, and `H0ResourceDiagnostics::voxel_count` reports the same number
it always did. The all-foreground compatibility branch is deleted rather than left
standing, because with a margin the padded array can never be all-foreground.

**What it costs:** `(n+2)³` instead of `n³` f64 for the transform — +33 % on the 21³
`make_box` grid, +4 % on the treant's 165×175×184, both transient.

**Corrected 2026-08-11 on review: that was the padding's cost, not the function's peak.**
The first version cropped with `Array3::from_shape_fn` over the caller's dimensions, which
holds the padded `(n+2)³` f64 array alive — it is the source being read — while allocating
the `n³` f64 result. The true transient peak was therefore **both at once**: ~141 MB beside
~134 MB at the admitted 16,777,216-cell grid, roughly **2×** the unpadded field rather than
+4 %. The crop now compacts the interior into the front of the padded array's own buffer and
truncates it — every source index is at or above its destination and both walk forward, so
one forward sweep of whole contiguous rows is safe — so the peak is the padded array alone
and the stated +4 %/+33 % is now the whole of it. On a branch whose companion module audits
its transients to four bytes per changed voxel, this one was uncounted and misstated. **No
cap moved:** `max_voxel_cells` still binds on the unpadded `GridSpec::count`.

**What it bought, measured on the committed corpus:** `Cylinder` hollows for the first
time, and `make_box` reaches a **typed** verdict (`IneligibleSavings` — its true 166.4 mm³
cavity is not worth cutting from a 1,000 mm³ cube) instead of a hard error. Of the eight
corpus models the hollow-refusal count goes **3 → 1**.

### The frozen movements, attributed

Five pinned values move. Every one of them traces to the same cause and every one moves in
the direction of the geometry being more correct, not less.

| pin | from | to | why |
| --- | --- | --- | --- |
| `Sphere` cavity voxels | 16,563 | 16,491 | the six polar tangent points leave the cavity |
| `make_box` raster bulk | 35.2 mm | 11.0 mm | a 10 mm cube does not have 35 mm of bulk |
| `make_box` cavity | 1.1 (mm³ ×10³) | 0.166 | a cavity can never exceed its source |
| `Sphere` refill fixtures | 7,880 / 502 voxels | 7,856 / 490 | the severed slab is cut from a smaller cavity |
| `Sphere` drain / vent | z = 0.5 / 19.5 | z = 2.5 / 16.0 | the drain now sits **on** the stated wall, not inside it |

**`corpus/baseline` is NOT regenerated.** It is evidence, and the frozen Python artifact is
the record of what the old field produced — including the 1.0 mm drain exit. The drains
test now pins **both** numbers and the difference between them, so the divergence is loud
and attributed rather than silently re-pinned. Whether the artifact is ever re-frozen is
the owner's call: **FU-51**.
**The owner's call was made, and carried out, later the same day:** see
[the FU-61/FU-51 section above](#fu61-confirmed-fu51-re-record-2026-08-12).
`Sphere.pipeline.json`'s drain/vent fields now read `z = 3.5` / `-1.5,-4.0,15.0`
(FU-53 moved the cavity floor again after this fix landed, so the re-record's
numbers are not this section's `z = 2.5` / `16.0` either) — the divergence
this paragraph describes is the state as it stood on 2026-08-10 through
2026-08-12, not the current one.

**The `Sphere`'s route through the pipeline moved, and it now stops at the gate.** Until
this fix the sealed support run refused the `Sphere`'s drilled shell — a shell whose wall
was breached at both poles — and the chain substituted the oriented solid, so `plinth
prepare` wrote a file made of the reserve. With the wall intact the drilled shell **seals**
(`SupportedGeometry::DrilledShellSealed`), reaches the pre-write gate on its own merits,
and `PreWriteGate::UncoveredIslands` refuses it with three blocking defects — terminally,
because a gate refusal is a physics verdict on the plan the chain actually planned and is
never answered by substituting different geometry. That is the **same verdict**
`corpus_sphere_drilled_plan_is_refused_before_any_file_exists` already pins; the `Sphere`
joins it instead of escaping it through a fallback. **No gate, cap or bound moved.** The
product consequence is real and is stated rather than buried: one corpus model that used
to produce a (substituted) artifact now produces none, and the uncovered islands that stop
it are a support-planning finding that predates this change. One consequence is a **lost
gate**, filed as FU-52: that `Sphere` was the only committed fixture reaching
`SupportedGeometry::DrilledShellRefusedBySupport`, so that production branch now has no
test on it.

The same movement re-records one FU-38 fixture verdict, `true → false`: the `Sphere`'s
zero-field-versus-measured-field comparison used to read *neutral* only because **both**
configurations refused the breached shell identically. Both now seal, with differing
digests (`6219979372923930719` against `1114015346968956405`), so the `Sphere` says exactly
what `make_toed_slab` and `make_overhang_table` already said. FU-38's substance is
unchanged and nothing pinned is re-frozen by it.

`crates/plinth-pipeline/tests/first_artifact.rs`'s hollow-error fallback gate was pinned to
`make_box`, which no longer takes that branch. It is retargeted to a **procedural
pre-hollowed solid** (two nested outward-wound boxes), whose `TriageError::PreHollowed`
refusal is a *designed* one — the engine declines to hollow what is already hollow — so
unlike `InvalidShellTopology` it cannot evaporate the next time a geometry defect is
fixed, and the branch keeps a permanent gate.

### DEFECT 2 — DIAGNOSED, FILED AS FU-49: the marching cavity surface is not a manifold

The `Warhorse` and the treant are **not** in defect 1's class. Their EDT is unchanged by
the margin, byte for byte — their cavities never touched the array boundary. Their inner
surface is watertight (0 boundary edges, 0 same-direction edges) and **non-manifold**: 92
faces carrying `TOPOLOGY_CUT_NONMANIFOLD` and 276 carrying `TOPOLOGY_CUT_POINT_ONLY` on
the `Warhorse`; 132 and 483 on the treant; **zero of either** on the `Sphere`, `Cylinder`
and cube, which is what makes the attribution clean.

The mechanism is `fast_surface_nets`' one-vertex-per-cell rule meeting a cavity mask that
is not *well-composed*. Where a 2×2 face of a lattice cell holds the checkerboard pattern,
or a 2×2×2 cell's foreground is an antipodal pair, two sheets of the surface must share
that cell's single vertex — and the edges around it end up used by four faces instead of
two. The Boolean inherits it; `certify_closed_components` refuses; `orient_boolean_cavity`
and `validate_shell_topology` both demand `PreHollowed` with exactly one cavity component
and ≥ 2 certified closed components with exactly one negative, get neither, and return
`InvalidShellTopology`.

This is the same `TOPOLOGY_CUT_POINT_ONLY` self-contact class the corpus census counts on
*input* meshes — except here the hollow stage manufactures it on a pristine one.
`Warhorse.stl` is clean at raw and after decimation; the pinch is born inside hollowing.

**Not fixed here** because the honest remedy is a well-composedness repair on the cavity
mask (or a manifold-guaranteeing extractor), and both are new algorithms with their own
work bound and their own convergence argument — architecturally larger than this change,
and not something to improvise beside a one-function boundary-condition fix. FU-49 carries
the evidence chain and the two candidate remedies.

### DEFECT 3 — DIAGNOSED, FILED AS FU-50: raster subdivision is quadratic in the longest edge

`Round 50.stl` and `Round 100.stl` — plain flat display bases — fail with `frozen voxel
subdivision exceeded the caller work cap 16777216`. **The cap is not the problem.** The
rasterizer quarters each triangle by midpoints until every edge is ≤ pitch/2; midpoint
quartering is shape-preserving, so the recursion runs to a uniform depth
`d = ⌈log₂(L_max / (pitch/2))⌉` and a triangle's leaf count is `4^d` — **growing as the
square of its longest edge, in power-of-four steps, and completely blind to its area.** A
fan-triangulated round base is made of slivers that span the whole disc.

Measured over all 37 minis (`raster_work_item_count` with a diagnostic ceiling; the
production cap was not touched), against the area-proportional demand the same
rasterization actually needs (`Σ area / pitch²`):

| model | work items | cap ratio | area cells | waste |
| --- | --- | --- | --- | --- |
| `Round 100` | 22,486,336 | **1.340×** | 70,493 | 319× |
| `Round 50` | 19,291,472 | **1.150×** | 19,698 | 979× |
| `Rotting Warhorse` | 8,724,485 | 0.520× | 41,084 | 212× |
| `Awakened Tree` | 4,454,778 | 0.266× | 67,901 | 66× |
| `Round 25` | 3,489,488 | 0.208× | 5,637 | 619× |
| median of 37 | — | 0.026× | — | ~80× |

Two of 37 models exceed the cap, both by less than 1.4×. **The cap is already ~240× larger
than the honest need of the worst model in the corpus.** `Round 100`'s single worst
triangle is 97.96 mm long and 144.70 mm² in area: `d = ⌈log₂(97.96/0.25)⌉ = 9`, so 4⁹ =
262,144 leaves for about 579 voxel cells, a 452× overshoot on one triangle.

**The cap is not raised and must not be.** Raising it to admit `Round 100` (≥ 22,486,336,
or **67,108,864** under the repo's own `next_power_of_two(2 × worst case)` rule; 33,554,432
only if the doubling is dropped) would enshrine 300–1,000× waste as a
budget — FU-8b's standing lesson — and buy nothing durable, because the demand grows as
`L_max²`: the next larger base blows it again. FU-50 carries the full 37-model profile and
the cap-vs-need curve for the owner to rule on.

## 2026-08-10 — the decimator stops manufacturing duplicate faces; meshoptimizer's collapse is not link-condition checked {#decimate-dupfaces-2026-08-10}

<a id="decimate-dupfaces-2026-08-10"></a>

FU-26 landed the counter that could see duplicate faces. This section is what the counter
was pointed at. **No gate, cap, work bound or acceptance threshold is raised, loosened or
bypassed here**; one deterministic edit is added to `compact_result`, and every counter it
moves, it moves toward zero.

**The mechanism, and it is not ours.** An edge collapse `x → y` is topologically admissible
only when `link(x) ∩ link(y)` is exactly `link(xy)`. meshoptimizer does not test that: it
classifies vertices and rejects collapses that flip a triangle normal, which is a weaker
condition. So whenever the mesh contains an edge `(a, b)` whose two opposite vertices happen
to be `x` and `y`, collapsing `x → y` rewrites the two faces `(a, b, x)` and `(b, a, y)` into
`(a, b, y)` and `(b, a, y)` — **one triangle, stored twice, wound in opposite directions**.
Every dense organic mesh has thousands of such configurations wherever a surface nearly
touches itself: a limb against a body, a wing membrane, an eye socket. The same collapse
also produces non-manifold edges and point-only pinches; the coincident pair is the
manifestation this section fixes.

**Four candidate root causes were tested and eliminated before this one.** (1) *Dirty
input:* the `Cat (wild shape - Cat Lord)` source is one vertex-connected component of 82,564
faces with **0** boundary edges, **0** non-manifold edges and **0** duplicate faces — the
duplicates appear only after decimation. (2) *Dust shards collapsing to pancakes:* the
duplicate pair sits inside the 53,170-face main body, not in a detached component. (3) *An
f32 collision in our own adapter:* measured on five models including the Weeping Treant v1.2
— `f32_collision_groups = 0` on every one, 214,562 welded vertices at 214,562 distinct f32
positions, and **0** faces degenerate under f32. (4) *A lock artifact:* the pattern
reproduces with an all-false lock vector. What does reproduce it is a **torus**, at every
size tried, and never a uv-sphere at any target down to four faces — exactly the
sheet-fusing neighbourhood the link condition exists to forbid.

**The fix is a deletion, and it is the identity on the represented solid.** The two faces of
such a pair have equal area and exactly opposite normals: they contribute nothing to the
divergence-theorem volume, and a ray crossing them crosses twice, so they are not part of the
boundary of the solid at all. They are the bookkeeping residue of a collapse that already
happened. Because they span the same triple with opposite winding, they contribute exactly
one forward and one backward directed occurrence to each of their three undirected edges, so
removing both decrements every affected edge **in both directions**: an edge whose only faces
were the pair drops to zero occurrences and ceases to exist, every other keeps its balance,
and the only way a boundary could open is an edge that already carried an odd incidence.
That case is measured, not assumed, and refused by name
(`ZeroVolumePairOpensBoundary` — it fired nowhere in the corpus).

Groups are reduced by opposite-winding **pairs**, `min(even, odd)` of them, because two
coincident faces with the *same* winding are two copies of real surface and deleting either
would change the solid; anything surviving that is `CoincidentFacesRemain` and is not
repaired.

**Only groups this reduction wrote are touched, and "touched" is all-or-nothing.**
meshoptimizer's own input is passed to the removal and decides *which groups are eligible*:
a group whose occupancy the source already fully accounts for is not something this
reduction made, so it is neither removed nor refused — repairing an input stays a pre-pass
decision (D-22). That is what keeps the census property ("where the decimator declines to
reduce, the mesh passes through unedited") true of a no-op *in any face order*, and it is
why a source that carries its own coincident faces — `Giant Bat` 4 in 2 groups, `Plague
Zombie` 2 in 1 — does not turn every reducing ratio into a blanket refusal. What the source
must **not** decide is how much of an eligible group pairs off; see the correction below.

**What it buys, measured, reproducibly.** The removal runs on meshoptimizer's welded index
space before compaction, so the emitted mesh, its fingerprint, its probes and its topology
counts all describe the repaired candidate. The table below is the committed harness
`local_minis_zero_volume_repair` (`crates/plinth-decimate/tests/local_minis_repair.rs`,
`#[ignore]`d because its inputs are local read-only STLs), run at the branch tip over the
seven models the census named clean-raw-but-damaged-by-decimation. Every column is
production `plinth_decimate::run` output read through `build_face_topology(ExactBits)` — the
same call `validate_closed_mesh` makes.

Each model's *every* cut class is read — boundary, non-manifold, same-direction, duplicate,
point-only and degenerate — and the harness fails on any class the decimation manufactured.
The "cut faces, all six classes" column is therefore the whole of what
`validate_closed_mesh` will refuse, not a sample of it; see the second review pass below for
why reading four `TopologyCounts` numbers instead was not enough. "refused ratios" counts
ratios the emitted-candidate gate declined because a count rose, after which the search
stepped on and verified elsewhere.

| model | disposition | faces in → out | cut faces, all six classes, in → out | refused ratios |
|---|---|---|---|---|
| `Cat (wild shape - Cat Lord)` | verified | 82,564 → 53,148 | 0 → **0** | 0 |
| `Morel base` | verified | 153,494 → 71,466 | 0 → **0** | 0 |
| `Rotting Warhorse` | verified | 277,382 → 260,906 | 0 → **0** | 1 |
| `Tavey Base` | verified | 73,582 → 38,630 | 0 → **0** | 2 |
| `Thorny Vegepygmy` | verified | 76,188 → 60,352 | 0 → **0** | 0 |
| `zombie-8-a` | verified | 289,388 → 83,194 | 0 → **0** | 0 |
| `zombie-9-b` | verified | 373,748 → 51,966 | 0 → **0** | 0 |

Boundary and non-manifold edges are 0 → 0 on all seven, duplicate faces and duplicate-face
groups are 0 → 0 on all seven, and neither `CoincidentFacesRemain` nor
`ZeroVolumePairOpensBoundary` reaches the operator on any of them. The harness fails
the run if either happens, so it is a discriminator rather than a formality — see the
correction below, where the same harness fails all but one model under the code this branch
originally proposed.

The cut-face collapse is the second-order win and it is why the fix is worth more than its
own counter: `build_face_topology` severs adjacency at every edge touching a duplicate, so
one manufactured pair fragments the whole vertex fan around it into point-only cuts. The
per-ratio cut-face figures recorded on 2026-08-10 (`Weeping Treant v1.2` 30,724 → 3,828 at
r = 0.525, `Giant Bat` 1,780 → 481, `Crawler Zombie` 570 → 27, `Cat` 86 → 0) were **scratch
measurements** taken against an instrument that was never committed; they are left here as
the direction of travel and are explicitly *not* re-derivable from a clean checkout. The
table above is what a clean checkout reproduces.

**What it does not fix, stated plainly.** `Rotting Warhorse` still manufactures 8 non-manifold
edges at r = 0.525 from clean input, and the Treant 199. Those are the same meshoptimizer
defect in its unrepairable manifestation — two sheets genuinely fused, not a zero-volume
residue — and `topology_not_increased` refuses them, which is the gate working. **One
consequence to be honest about:** removing the pairs *reveals* non-manifold edges that
FU-26's duplicate-face exclusion was hiding (Treant 192 → 199, `Giant Bat` 31 → 32), so the
gate now refuses on a slightly truer number. It can only ever refuse more.

**Three tests were reading zero-volume artifacts and now say so.** `captured_treant_fan_\
closes_a_bare_lock_over_the_minimum_incident_face` asserted that a `[0, 1, 2]` face survived
the closure — it was one half of a coincident pair over a fan that cannot reduce at all.
`feature_locks_are_hard_constraints_in_simplification` inspected an eight-face octahedron
halved to a pancake, and now uses a fixture dense enough for a real reduction to be what
proves the lock. `production_feature_selection_pins_the_ratio_twenty_probe` reached its cap
refusal by scoring `make_box`'s two-face probe pancake as though it were a surface: a
twelve-face model has no ratio-0.20 candidate, that is now named
(`FeatureProbeHasNoCandidate`), and it is still a refusal on the audited identity path — the
corpus keeps its `already_print_scale`.

**The lock is checked twice, against two different meshes, because it is two different
statements.** Against meshoptimizer's own result it asks whether the simplifier honoured the
constraint it was given; a `no` is a fact about the machinery and stays loud and fatal
(`LockedVertexRemoved`). Against the **repaired** face list it asks the question that
actually binds, because a hard lock is a constraint on the mesh this stage *emits*: a
protected vertex whose entire surviving incidence sits inside removed pairs would otherwise
be deleted with them, silently, with no error and nothing in the trace — an unbounded
relaxation of a hard constraint. That candidate is refused by name
(`LockedVertexLostToZeroVolumeRepair`), and because the refusal is a fact about *this ratio*
rather than about the engine, the search records it and steps on. Measured on the Weeping
Treant v1.2: the ratios that would have surrendered 27, 4 and 1 protected vertices are
refused, r = 0.940625 surrenders none and is still measured, and the corpus disposition is
unchanged. `ZeroVolumeRepairRecord::locked_vertices_dropped` is therefore structurally zero
on every emitted candidate, and the corpus gate asserts it there
(`audit_no_lock_was_surrendered`) so the field is a standing witness rather than a constant.

**Report schema** goes to `plinth/m3-decimation-corpus-budget-report/v9`: v7 turned
`zero_volume_pairs_removed` into the full `zero_volume_repair` record on every candidate the
search describes (each bisection step, each adaptive probe, each final attempt, plus the
mandatory ratio-0.20 feature probe that decides the entire protected set), v8 adds
`search_candidate_free_ratios`, and v9 adds the three cut-class face counts that now gate.
The thread-determinism sweep's report goes to v3 for the
same reason — the one edit this crate makes now travels in the artifact whose byte-identity
across 1/4/16 threads *is* the determinism claim, and the sweep asserts the fixture really
exercises it (12 pairs removed, 12 vertices dropped, 0 locked). Corpus dispositions are
unchanged — all eight `already_print_scale`, the Treant still refused by `TopologyIncreased`
before the witness is consulted.

### 2026-08-11 — the review's eight, applied in place

The branch above was reviewed before merge and did not survive first contact: it **did not
compile** (`remove_zero_volume_face_pairs` had grown a second parameter and eight unit-test
call sites had not), so *nothing* in the working tree had ever been executed. Everything
above stands except where corrected here. No gate, cap, work bound or acceptance threshold
is raised, loosened or bypassed by any of it.

1. **The repair was disabled in the primary case.** Source occupancy was subtracted from each
   winding *before* pairing. Run that against the mechanism this whole section is about:
   collapsing `x → y` turns `(a, b, x)` into `(a, b, y)`, whose partner `(b, a, y)` is a
   **source** face meshoptimizer retained. The group is `even = 1, odd = 1` against a source
   occupancy of `even = 0, odd = 1`, so subtraction scores `min(1, 0) = 0` and pairs nothing.
   A pair is a pair: deleting the manufactured face *requires* deleting its retained partner,
   and doing so is still the identity on the represented solid. The source now decides only
   **which groups are eligible** (any group carrying a face the source did not) and never how
   much of an eligible group pairs off. Measured with the committed harness, the subtraction
   version costs six of the seven models their reduction outright — `Cat` and `zombie-9-b`
   refuse at the ratio-0.20 feature probe and ship at 82,564 and 373,748 faces instead of
   53,148 and 51,966; `Rotting Warhorse` ships unreduced; `Morel base`, `Thorny Vegepygmy`
   and `zombie-8-a` verify at 121,594 / 69,402 / 186,292 instead of 71,466 / 60,352 / 83,194
   — all of them on `CoincidentFacesRemain` refusals over coincidence the reduction itself
   manufactured. The same asymmetry is fixed in the repeated-index degeneracy guard, which
   read raw group size and so blanket-refused a source that already carried a duplicated
   degenerate triple.
2. **The hard-lock re-check is committed with the rest.** The version originally committed
   checked the lock only against meshoptimizer's pre-repair result and then deleted pair
   faces with no re-check, so a protected vertex could be dropped in silence with the only
   witness a field nothing audited. See the paragraph above for the semantics that shipped.
3. **The per-cause bisection direction is wired, not dead.** `CandidateFreeCause::
   bisection_response` existed, was never called, and the arm that ran instead took
   `high = midpoint` unconditionally — which that dead code's own analysis calls
   self-reinforcing for five of its six members: a lower midpoint is *more* aggressive, so
   one candidate-free hit burns the remaining bisection budget and the model falls out as
   `already_print_scale`. Only `DegenerateOutputTriangle` has its candidates on the denser
   side. Both directions are now pinned by test, including that a refused ratio moves the
   search **away** from the wall.
4. **The corpus gate's step-count audit accounts for refused ratios.** It rejected any
   `Verified` model with `trace.steps.len() != 6`, but a candidate-free midpoint pushes a
   refusal and no step, so one unusable ratio out of five would have failed the gate on a
   model that verified perfectly well — latent only because all eight corpus members are
   `already_print_scale`. The trace now counts them (`candidate_free_ratios`) and the audit
   checks `steps + candidate-free ratios`.
5. **The verification instrument now exists.** The doc comment and the table above cited
   `local_minis_zero_volume_repair` as committed; the `tests/` directory contained no such
   file. It is committed, it fails on the pre-correction code, and the table above is its
   output.
6. **The feature-probe downgrade carries its cause.** Any candidate-free outcome at the
   mandatory r = 0.20 probe becomes `already_print_scale`. That is right for "the model is
   too coarse" (`EmptyResult`, `AllFacesWereZeroVolume` — `make_box` is twelve faces) and
   indistinguishable-from-fine for the four *defect* causes, which were loud `ProbeSimplify`
   failures on `develop`. `FeatureProbeHasNoCandidate` now carries the `CandidateFreeCause`,
   so the report can tell "too coarse" from "the probe hit a defect".

**FU-43 part (1) landed here; part (2) is still filed, not fixed.** A candidate-level failure
inside the ratio search no longer ends the whole run. The six outcomes that are facts about
*one ratio* rather than failures of the machinery are collected in `CandidateFreeCause`,
recorded as `ReductionRefusalReason::CandidateFree`, and stepped past — in the direction the
cause implies, per cause (see the review's item 3 above). `LockedVertexRemoved` is
deliberately not a member: it says meshoptimizer violated a hard constraint it was given, and
that stays loud. `zombie-2-a`'s r = 0.525 output triangle — three *distinct* f64 vertices
that are *exactly* collinear, a needle meshoptimizer cannot see because it measured the
collapse in f32 — is named (`DegenerateOutputTriangle`), refused at that ratio, and the
search continues to the ratios where the same model is clean. **Part (2), the exact
geometry-preserving repair for the needle** (delete it and split the single face across its
long edge at the on-segment midpoint: no vertex moves, no surface point changes, zero net
faces) is a separate design decision and is still open in the FU-43 row with the coordinates.

### 2026-08-11, second pass — the gate was asymmetric, and the instrument could not see it

A second review executed the branch independently rather than reading it, and found that
the section above was true and incomplete: the decimator had stopped manufacturing
*duplicate* faces and was still manufacturing *pinches*, and shipping them as verified. Two
findings, both applied in place. **No gate, cap, work bound or acceptance threshold is
raised, loosened or bypassed by either; both refuse strictly more than before.**

1. **The gate compared two numbers out of seven.** `topology_not_increased` read
   `boundary_edges` and `nonmanifold_edges`, so a manufactured non-manifold edge was refused
   by name — the honest Warhorse/Treant caveat above — while a manufactured **point-only
   cut** was not part of the comparison at all. `Tavey Base.stl` is clean at raw in all six
   cut classes and production `plinth_decimate::run` emitted a *verified* 73,582 → 29,890
   candidate carrying 10 faces flagged `TOPOLOGY_CUT_POINT_ONLY`, deterministically, at the
   ratified budget. `validate_closed_mesh` (`plinth-hollow/src/lib.rs:1021`) refuses **any**
   nonzero cut flag, so that model stayed pipeline-refused end to end *because of* the stage
   that was supposed to have fixed it, and the branch's own acceptance criterion —
   previously-clean models come through decimation with zero manufactured cut faces — was
   not met for a named mini. The predicate is now the whole of `TopologyCounts`: no
   canonical count may rise. `TopologyCounts` gains the three cut-class face counts no edge
   count covers (`point_only_cut_faces`, `same_direction_cut_faces`,
   `degenerate_cut_faces`), taken in one pass over the finished `cut_flags` array in the same
   canonical build, and the gate destructures the struct exhaustively so that a count added
   later is a **compile error** until it is given a verdict. `duplicate_faces` and
   `duplicate_face_groups` join the gate at the same time: FU-26 landed the counter and
   deferred the decision "with its own corpus consequences", and the consequences are now
   measured to be none. The refusal is the shape that already existed — the ratio is
   recorded as `TopologyIncreased` and the search steps on — so `Tavey Base` verifies at a
   *different* ratio (73,582 → 38,630) rather than being surrendered, after the gate refuses
   r = 0.40625 (10 pinch faces) and r = 0.3765625 (9). `Rotting Warhorse` refuses
   r = 0.9109375 (9 pinch faces) and verifies unchanged at 260,906. The other five models are
   byte-for-byte where they were.
2. **The committed instrument was structurally blind to the class.** `local_minis_repair.rs`
   read `TopologyCounts` and nothing else — four numbers, none of them a cut-flag count — so
   it passed, green, while `Tavey Base` emitted those 10 manufactured pinch faces. It now
   counts **all six** cut classes per stage straight off `FaceTopology::cut_flags()` and
   fails on any class the decimation manufactured, not duplicate faces alone; its report
   (schema v2) carries every class both sides plus the refused ratios in full, and the table
   above is regenerated from it. A harness that can only fail on the defect you already fixed
   is not evidence about the defect you did not.

The synthetic regression fixture is two closed boxes welded at one corner: watertight,
consistently oriented, no duplicates — **every count the old gate read is identical to a
single clean box's** — and the surface pinches to a point at the shared vertex. That is the
shape `Tavey Base`'s candidate had, and the fixture pins that the old comparison admits it
and the new one refuses it, without needing a 73,582-face STL to say so.

## 2026-08-10 — the orientation-search review's twelve, applied in place {#orient-search-review-2026-08-10}

<a id="orient-search-review-2026-08-10"></a>

Two independent reviews of the dense-orientation-search landing returned twelve findings.
All twelve are applied on `feature/orient-search` before merge. **No gate, cap, acceptance
bound or funnel width was raised, loosened or bypassed**, and no pinned digest outside this
unmerged branch was re-pinned. Two findings turned out to be worse than reported once
measured, and those measurements are the most important thing in this section.

**The record was wrong about why nothing moved (BLOCKING).** The delicacy-producer decision
record claimed the anchor cap was the support stage's only delicacy-driven decision, and
named exactly one surviving `vec![0.0]` feed. Both are false: contact selection breaks
exact-Z ties on delicacy, and there are five surviving feeds of which four reach a support
run. The re-pin protocol's own instrument was then run rather than argued —
`make_toed_slab` (`13640335649674000245` → `6989898566632288617`) and `make_overhang_table`
(`3706765241747593242` → `3116329269026579872`) **both move** under the real field, and
neither crosses the cap. Nothing committed broke, because every harness that pins a support
digest feeds the zero field itself and so remains internally reproducible — but those pinned
digests describe support runs production no longer performs. Full attribution is in
[`decisions/2026-08-10-delicacy-producer.md`](decisions/2026-08-10-delicacy-producer.md);
the re-pin question is FU-38's and stays with the owner.

**The neutrality guard was measuring the wrong solid.** It built all three fixtures'
prepared meshes through one pinned rotation (the `Sphere`'s). Hollowing and drain drilling
are rotation-dependent, so for two of three it asserted about geometry the pipeline never
makes and could have stayed green while covered geometry crossed the cap. Each fixture now
orients by its own pinned baseline.

**The orientation work budget could never fire.** Every cap in `OrientSearchLimits` was
derived from the very counts it was then compared against — `faces × candidates` against
`max(2^28, faces × 2048)` with `candidates ≤ 653` — so every comparison was structurally
`n·a ≤ n·b` with `a < b`, and orientation was the one stage in the pipeline with **no
effective work bound at all**. The caps are now absolute, derived once by the repo's own
`next_power_of_two(2 × worst case)` rule from the largest model this workspace can run end
to end (the treant, 429,120 faces / 1,287,360 verts), and **`PrepareResourcePolicy` gains
the `orient` field the design's §6 asked for**, so the deviation is removed rather than
recorded. The byte guard also undercharged: it omitted the section solver's sort
permutation, breakpoint list and quadratic coefficient array — 36 bytes per vertex, 46 MB
on a mesh whose whole charge was 34 MB.

**A fabricated measurement in a provenance record.** With no L1 rung there is no cascade
source for L2, and the code synthesized `RungError::BelowFloor { faces: 0 }`, publishing
"0 faces is below the 256-face search-proxy floor" about a proxy that was never built.
That is precisely the class this landing's own decision record forbids — "there is no
substitute value for a measurement that did not happen". It is now
`ProxyFallbackReason::NoCascadeSource`, carrying no count, and the covering test asserts
the detail does **not** claim one.

**The determinism digest could degrade to a constant.** `serde_json::to_vec(record)
.unwrap_or_default()` made any serialization failure yield the SHA-256 of the empty string
— the same value for every record. `search_digest` is the one string T4, T7, the thread
sweep and cache-equivalence compare on, so a path under which two different searches agree
is the exact defect the digest exists to detect. It is now a typed refusal.

**`decision_resolved` compared two instruments.** An exact-tier margin against a ~2 % proxy
spread; on the `Sphere` that spread is 674.08 score points against a committed margin of
0.0059. Both sides are now measured at the coarse tier, the tier is stamped in the record
(`margin_tier`, `spread_tier`, `winner_runner_up_margin_at_spread_tier`), and the committed
exact margin is still reported beside it. **Re-measured on all eight after the change, the
flag still reads `false` on every one**, so 03 §3.8's finding stands — as a statement about
the coarse grid rather than about two instruments disagreeing.

**Near-antipode rotations were not rotations.** The closed form divides by `1 + c`, and
near `−ẑ` that quantity is at the scale of `c`'s own rounding error: the normalized
`[2e-8, 0, −1]` produced a row-1 diagonal of ≈ 0.8 instead of 1. Only the *exact* antipode
took the pinned branch. Rest poses are area-weighted `−n` aggregates and can land
arbitrarily close to `−ẑ` on a real mesh, and the winner's matrix is applied to production
geometry at emit. `1 + c` is now read off `x² + y²` on the negative hemisphere, where it is
well-conditioned, and a near-antipode test covers six offsets down to exact.

**The charge-beside-meter receipt was vacuous.** Two of three counters were assigned from
the plan they were meant to audit, so `ChargeDrift` could only ever fire on breakpoints and
the covering test asserted `plan == meter`. All three are now counted where the work
executes — one increment per product written — and the test checks them against counts
derived from the mesh rather than from the plan.

**A zero scar weight now reproduces the scar-free search structurally.** Every rung built
the delicacy field unconditionally and carried its digest into `search_digest`, so a
scar-free run's digest depended on delicacy bytes and a delicacy failure refused a search
that never wanted the number — contradicting a requirement the producer ruling explicitly
kept in force. At a structurally zero weight the field is now not built, and `LodRecord`
carries `delicacy_digest: null` rather than a hash of zeros.

**A widened precondition, recorded.** The closed-boundary refusal is enforced on the Exact
tier, where the LOD ladder cannot answer it, so a not-perfectly-closed welded mesh is
unpreparable under the default policy while `--orient pinned` still prepares it. That is a
new refusal class; it is journalled as FU-42 with the reachability measurement — all eight
corpus models weld closed, `boundary_edges = 0` and `nonmanifold_edges = 0` on every one.

**Nine ledger locators named types that do not exist.** `search.rs:SearchLadder` is
`LadderSettings`; the three grid constants named `grid.rs:SweepGrid`, which does not read
them, and `search.rs`, which does not either. `constants_ledger_consistency.rs` compares id
sets only, so nothing caught it. All nine now name the type that performs the registry
read.

## 2026-08-10 — `plinth prepare` orients, and the standing product blocker is closed

<a id="dense-orientation-search-2026-08-10"></a>

**Read this before the 2026-08-09 section below.** That section's headline blocker —
"the CLI can prepare nothing today" — is **closed**. Every sentence in it about
`LegacyCoarse`, the missing `--orient` flag and the per-model orientation refusal
describes a state that no longer exists; it is left standing as the record of what was
true rather than rewritten.

`plinth-orient` now searches. `plinth_orient::search_and_orient` runs the D-12 funnel
over the recorded candidate grid and commits a winner at the production mesh;
`OrientationPolicy::DenseSweep` is the pipeline default; `plinth prepare` takes
`--orient dense|pinned:DIR` and defaults to `dense`. **`OrientationPolicy::LegacyCoarse`
and `PipelineError::OrientationPolicyUnavailable` are deleted**, not left standing beside
the search — a persisted policy carrying `"kind":"legacy_coarse"` now fails to
deserialize by name, which is the intended loud failure.

**What it is.** Three candidate blocks in one contract order — the legacy 13 first (so
identity is `CandidateId` 0 and the legacy identity-wins-ties semantics two committed
baselines depend on is reproduced), then ≤128 face-aligned rest poses, then the 512-point
Fibonacci lattice on S², deduplicated at 1.0° keeping the **earlier** index. Three tiers:
the whole grid on the ~2 % proxy, the top 24 on the ~10 % proxy, the top 3 **plus the
escorted legacy block** at the production mesh, where the committed score is computed and
only there. Nine new `Placeholder` constants, registry 293 → 302.

**Measured** (release, committed baselines, all four objective terms): Weeping Treant
v1.2, 429,120 faces, 641 candidates, whole sweep **3.07 s** — about 0.65 % of that
model's ~470 s support run. Every mini is single-digit milliseconds. The full table,
including the two divergences worth reading, is the second 2026-08-10 addendum to
[`03-physics-and-constants.md`](03-physics-and-constants.md) §3.8.

**One recorded deviation, with its measurement.** The legacy 13 are escorted to the exact
tier rather than merely enumerated at the coarse one, because D-12's "never worse by
construction" is a property of the candidate set at the tier that *decides*. On the
committed `Sphere` the L1 proxy ranks the winning legacy X −20 pose 478th of 643, so
unescorted the sweep committed to 16.3173 against that pose's 16.3009 — worse than the
search it replaces, on a corpus model. Escorted, it reproduces the pose the `Sphere`
baseline itself records. Neither funnel width moved; no gate, cap or bound moved.
[`decisions/2026-08-10-legacy-block-escorted-to-the-exact-tier.md`](decisions/2026-08-10-legacy-block-escorted-to-the-exact-tier.md).

**Still open, and named rather than implied.** Three pieces of the design are
deliberately not in this landing: the `<stem>.orientation.json` sidecar and the native
pin format that would let a searched winner be replayed by fingerprint (the report
carries the full `DenseSweepRecord` provenance today, but nothing persists it to disk);
the legacy-13 comparison **driver** and its `LegacyStepped` section method, which is what
T1/T2's reproduction fixtures need; and `plinth calibrate` with the recorded
Spearman/K-doubling gates. The `decision_resolved` flag reads `false` on all eight corpus
models, which is precisely the signal those calibration gates exist to quantify.

## 2026-08-05 — the line of record moved to `develop`

Everything below this section was written while the engine line lived on a topic branch in
a temporary worktree. That is no longer true, and where the older text names that branch or
that path as "the integration authority", read it as history rather than as instruction.

**The single line of record is now `develop` in the definitive checkout
`/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`.** The repository follows git flow:
`main` and `develop` are the long-lived branches, `main` is release-only and moves only when
the owner releases `develop` into it, and every new piece of work starts as a `feature/*`
branch cut from `develop` and merges back into `develop` with `--no-ff`.

`codex/rust-pipeline-integration` was merged into `develop` in full — a true two-parent
merge of its tip `cef27a1`, no squash and no rebase, so the whole engine arc including the
caps landing (`3d91390`, `e3670ab`, `ef88ab0`) and owner rulings R1–R5 is preserved commit
for commit in `develop`'s history. The README overhaul (`feature/docs-readme-overhaul`,
tip `207f84d`) was merged immediately after so its documentation links resolve against the
engine tree. **With those merges recorded, the integration branch and its worktree
`/private/tmp/plinth-3d-pipeline-integration` are retired and deleted as part of this
commit.** Containment is the merge itself: `cef27a1` is an ancestor of `develop`.

The five gates were run on the merged `develop` tip and are green. The authority records for
the caps landing are this file and
[`support-resource-caps-2026-08-05.md`](support-resource-caps-2026-08-05.md), both now on
`develop`.

Practical consequences for anyone resuming:

- Work in `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, not in `/private/tmp`.
- Read the live head with `git -C /Users/craig/code/hughescr/dnd-campaigns/plinth-3d log`,
  not from the retired integration worktree, which no longer exists.
- "Integrated" now means **an ancestor of `develop`**. The status vocabulary below still
  says "the integration branch"; substitute `develop` throughout.
- Start new work on `feature/<topic>` off `develop`. Never push to the `github` remote
  without the owner's explicit instruction.

## Status vocabulary

- **Integrated:** the commit is an ancestor of the integration branch.
- **Accepted, branch-only:** an independent review accepted the commit, but it is not
  an ancestor of the integration branch. Integrate the named commit deliberately after
  inspecting its patch; do not merge the branch head blindly.
- **Blocked:** review found required changes. Do not integrate it.
- **Pending review:** implementation exists, but acceptance has not been recorded.
- **Uncommitted:** worktree-only state. It may be lost and is never integration evidence.

## Owner caps-landing rulings R1–R5 (2026-08-05, late) {#owner-caps-rulings-2026-08-05}

The support resource-cap landing (`3d91390`, `e3670ab`, `ef88ab0`) was reviewed **ACCEPT
with findings**. The following are **binding owner decisions of 2026-08-05**, delegated by
the owner with veto retained, adjudicating those findings. The measurement record they act
on is `docs/rust-migration/support-resource-caps-2026-08-05.md`, which carries the long
form of R1–R4 at the exact numbers they correct; this section is the index.

- **R1 — the undisclosed third scaled cap is PINNED BACK.** `SupportPreparationLimits::input`
  scaled **three** caps where the doc comment, the caps record and the M0 registry all
  declared two: `SpecializedLimits::coverage_route_knn.max_work` rode the same
  `2048 × faces` rule purely for sharing the `KnnBatchLimits` shape. That coefficient was
  measured on the **minima-to-face** batch alone; the coverage-route batch has no
  measurement of record, no accessor and no test. The scaling is **removed** — the cap stays
  at the flat `SUPPORT_KNN_MAX_WORK_FLOOR` (100,000,000) until someone measures that batch.
  Behaviorally inert on every model: the treant's instrumented coverage-route demand is
  **15,282,619**, 6.5× under the floor, and the treant stops at the specialized gate before
  coverage routing runs at all. Landed `2aec85a`, pinned across 0…10⁸ faces by
  `the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count`.
- **R2 — 512 B/face is RATIFIED.** The landed parity-grid coefficient is 512 where the
  proposal said 256. Ratified as written, **no code change**: the deviation buys **one**
  uniform `next_power_of_two(2 × observed)` rule for both coefficients, and the wider margin
  it leaves (4.2× admitted-over-observed refs/face against kNN's 1.7×) is earned by the
  corpus showing refs/face varying 526 ↔ 2.83. Recorded `354c27d`.
- **R3 — the census headline is CORRECTED.** The caps record claimed the before/after census
  "differs in exactly one line out of 3,399". The true diff is **9 lines**: 8 additive
  harness fields on the `MODEL` rows plus the treant's own stage-error row. What the
  measurement supports — **all 3,390 instrumented `CENSUS` body lines byte-identical, all 7
  reachable digests unmoved** — is the zero-behavioral-delta claim, and it survives intact.
  Written as a **visible dated addendum** below the original per this project's
  honest-addendum convention, not a silent rewrite. Recorded `354c27d`.
- **R4 — both scaled coefficients are MARKED n = 1.** The two Measured observations and the
  two Derived coefficients above them rest on a single specimen: **Weeping Treant v1.2,
  429,120 faces, 2026-08-05**, the only corpus model above 4,760 faces. All four M0 registry
  notes now say so and say the consequence: **a second large-model measurement triggers
  re-derivation**, never silent inheritance of a treant-shaped constant. The `Measured`
  classification of the refs-per-face observation **stands** — the inversion through
  `axis_memory_bounds` is disclosed in its own note. Notes changed at the producer
  (`constants/evidence.rs`) and `constants.md` regenerated; the generated file was never
  hand-edited. Recorded `354c27d`.
- **R5 — the orphan execution caps are REGISTERED.** The support pipeline's remaining
  execution authority had **no registry entry at all**. Every cap in the four named
  authority types is now registered — **42** of them, not the ruling's estimated ~29, the
  count reported as enumerated rather than trimmed to the estimate — all
  `runtime_resource_cap`, all wrapped `Placeholder`, each with `source_locators` and a
  `provenance_detail` naming the Codex-era commit that introduced it (`73e7c13` / `a8acfb9`
  / `c2b3b98`, no measurement of record). **Registration is disclosure, not ratification**,
  and the `Placeholder` wrapper is what keeps them off any accept path. The four gate/load
  caps measured to be refusing algorithm defects carry that prohibition in their own notes:
  they must **NOT** be raised (see FU-8b). Nested authorities under
  `SupportPreparationLimits::production` — `CandidatePayloadConfig`,
  `ContactConstructionLimits`, `PenetrationBatchLimits`, `RaftLimits` — are explicitly **out
  of scope** and need their own pass; the boundary is stated in
  `03-physics-and-constants.md` §3.10.9. Landed `e6994f9`; registry moved **218 → 260**
  constants, classes `[18 Measured, 6 Derived, 194 Placeholder, 0 Pending]` →
  `[18, 6, 236, 0]`, `constants.md` regenerated via the producer example.

**OD-8 delta discipline.** The expected behavioral delta across all five rulings is
**ZERO**, and it was measured, not assumed: R1 removes a scaling no model reaches, R2 is a
no-op, and R3/R4/R5 move note text and registry membership, which are not accept-path
inputs. See *Caps-rulings OD-8 delta* immediately below.

### Caps-rulings OD-8 delta — MEASURED ZERO {#caps-rulings-od8-delta-2026-08-05}

Re-run at the rulings tip over all 8 corpus models:

```
PLINTH_CENSUS=1 cargo run --release --offline --features census \
  --example embed_incidence_census -p plinth-support
```

The transcript is **3,399 lines**, the same length as the caps-landing run, and **all seven
reachable digests are byte-identical to the values recorded in
`support-resource-caps-2026-08-05.md` §4**:

| Model | faces | `snapshot_digest` at rulings tip | vs recorded |
|---|---|---|---|
| `Sphere` | 4,760 | `17667468971446251870` | unmoved |
| `Cylinder` | 244 | `13310185350867878190` | unmoved |
| `make_box` | 12 | `9657051231007502658` | unmoved |
| `make_overhang_table` | 60 | `6686599816688114065` | unmoved |
| `make_toed_slab` | 140 | `11199296506741562956` | unmoved |
| `make_raking_blade` | 24 | `13278215294653054002` | unmoved |
| `make_thin_fin` | 24 | `7016743535301716631` | unmoved |

The treant still refuses at the same place, with the same typed error:

```
MODEL  Weeping Treant v1.2  UNREACHABLE  faces=429120
       max_grid_bytes=219709440  knn_max_work=878837760
       support: sealed support run failed during Preparation
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 2000000001, limit: 2000000000 })
```

Note that `knn_max_work=878837760` on that row is the **minima** batch, which R1 did not
touch and which stays scaled. R1 removed the scaling on the *coverage-route* batch, which
is not printed and which the treant never reaches. **Behavioral delta: zero, measured.**

**The R-numbered ledger continues.** Owner ruling **R6** (2026-08-06) — the decimation-first
premise is falsified, FU-8b runs first, decimation is demoted — is recorded in *FU-8b
resolved; the treant's stopping point moved*, immediately below.

## Owner architecture decision {#owner-architecture-2026-08-03}

The owner approved this binding clearance-mask architecture on 2026-08-03:

1. Each member end may independently carry an optional, finite, designed clearance-mask
   extent. Neither, either, or both ends may have one.
2. A supplied extent is valid only after construction proves that its mask is connected
   to the corresponding endpoint. This is construction validation, not an inference from
   later penetration observations.
3. Observed penetration thickness `T` is separate evidence. It must not be substituted
   for, or used to synthesize, a designed mask extent.
4. Any mutation that could change the member or mask invalidates the prior construction
   validation. The mutated value cannot enter verification until freshly validated.
5. A path that requires a designed extent rejects when that extent is absent. It must not
   guess or recover one from observed geometry.
6. The retired `_embed_inset_arc_exact` `cos.max(0.5)`/60-degree-floor formula is deleted.
   It is not ported, retained as fallback, or used to weaken the new design.

This decision authorizes the embed-zone implementation. It does not accept any particular
implementation in advance.

## Follow-ups ledger {#follow-ups-2026-08-04}

Items raised by the 2026-08-04 and 2026-08-05 reviews. Each should be scheduled
deliberately. FU-18, FU-19 and FU-20 — the three that made the workspace gate red and the
inventory stale — were **resolved on 2026-08-05 (late)**; see *Post-merge interaction delta*.
Everything else here is non-blocking.

**Ten rows were ruled by the owner on 2026-08-12** — FU-36, FU-37, FU-38, FU-46, FU-47,
FU-50, FU-51, FU-54, FU-55 and FU-57 — and each carries its ruling at the head of its Note
cell, above the row as first written. The reasoning for all ten is in one place:
[`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md).
A ruling decides a question; it does not close a row, and every one of the ten stays open
until the work it authorizes is done. **FU-61 was filed by that batch** (from FU-51), at the
next free number under the recorded collision protocol.

**All twelve rows that batch worked — FU-36, FU-37, FU-38, FU-47, FU-50, FU-51, FU-53,
FU-54, FU-55, FU-56, FU-57 and FU-61 — are now on `develop`**, and each row below carries
its landing state at the head of its Note cell. What landed per row, the corpus rerun that
measured it, the FU-61 verdict and the twenty-three questions the lanes filed rather than
guessed at are all in one place:
[`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md),
with the measurement in [the dated section](#ruled-batch-landed-2026-08-12). A landed row
is not necessarily a *closed* row: FU-56 and FU-57 built the mechanism their rulings named
and 19 of 37 models still cannot be supported. **Eight new rows, FU-62 through FU-69, were
filed by that record** at the next free numbers; no existing row was renumbered, edited or
displaced.

**Nine further rows, FU-70 through FU-78, were filed on 2026-08-12** by the drainage-doctrine
scribe pass, from the owner's drainage-physics rulings and the twenty-three orchestrator
rulings that resolved the lane planners' open questions. The reasoning for all of them is in
one place:
[`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md),
summarized in [the dated section](#drainage-doctrine-and-orchestrator-rulings-2026-08-12).
Three items that pass ruled on were **already covered by existing rows and were annotated
there rather than duplicated** — FU-66 (the unregistered `drilling.rs` drain/vent literals),
FU-68 (the fragmented refusal class, whose reporting half FU-76 now answers) and FU-17 (the
archived model-anchor bundle, re-named so it is findable). **No existing row was renumbered,
reused or displaced, and no cap, gate, floor or tolerance moved.**

**Four further rows, FU-79 through FU-82, were filed on 2026-08-12** by the dual-cavity
hollowing fixture's scribe pass, every one of them from a measurement rather than a review
opinion: the `Solid { BelowThreshold }` catch-all (FU-79), a drilled shell losing its hollow at
the support stage (FU-80), a vent site the driller then refuses (FU-81), and FU-47's
core-clearance verdict turning out to depend on the model's pose (FU-82). The measurements are
in [the dated section](#dual-cavity-fixture-2026-08-12). **Three items were annotated on
existing rows rather than duplicated** — FU-58 (the discarded support-refusal reason, hit again
by FU-80), FU-59 (the drain-planning byte cap, re-measured on a 25 mm synthetic) and FU-62 (the
end-to-end gap, now answered on a synthetic and still open on the corpus). **No existing row was
renumbered, reused or displaced, and no cap, gate, floor or tolerance moved.**

**Three further rows, FU-83 through FU-85, were filed on 2026-08-12** by the reconciliation pass
over the batch record's "23 open owner questions", whose full reasoning is in
[`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md).
**Of the twenty-three, 13 were already answered** (the record was stale, not the question open),
**10 are ruled there** — one by the owner, nine as orchestrator rulings under co-owner authority —
and **1 genuine owner decision** was extracted from the residue of questions 2, 3 and 16 and is
**open, with Craig**: *what "wide enough for resin to flow" means in the shipped engine*, where the
doctrine cites a 3.0 mm bore, the inscribed cutter realizes 2.974 mm, and `flow.rs` requires a
3.5 mm passage. The new rows are the discarded `FlowStats` that gates FU-82's census (FU-83), the
digest chain-of-custody pass (FU-84) and the Sphere baseline's reader's note (FU-85). **Six existing
rows were annotated rather than duplicated** — FU-47 (the 1.5 vs 1.75 mm discrepancy and the open
owner decision), FU-56 (the previously unattributed `make_overhang_table` census movement, now
recorded with its commits), FU-57 (questions 10 and 11 ruled), FU-75 (sequenced behind FU-82's
census), FU-76 (the dead `evidence` field dies in the same commit) and FU-82 (gated on FU-83).
**No existing row was renumbered, reused or displaced, and no cap, gate, floor, tolerance or
registered constant moved.**

| # | Follow-up | Note |
| --- | --- | --- |
| FU-1 | Mechanize the class-(b) cap-table tie to `SupportPreparationInput::validate` | Currently the cap table and the validating boundary agree by inspection only. Needs a small production affordance to make the tie mechanical. |
| FU-2 | Decide `validate()` width vs per-type constructor contracts | Defer until the execution runner lands, when the real call sites exist to judge against. |
| FU-3 | Six sites still construct `NativeSurfaceConfig`/`ProbeConfig` with `usize::MAX` outside the validating boundary | The three latent fixture defects on the merged path were fixed; these six remain outside it. |
| FU-4 | Commit a corpus provisioning script, or make the corpus-dependent targets loud-skip | Today a bare checkout hard-fails two test targets with no explanation. See `corpus-provisioning.md`. |
| FU-5 | Pin the `scar.rs` duplicate registry-reader (`crates/plinth-metrics/src/scar.rs:183-186`) the way the new delicacy M0 test pins the `plinth-mesh` reader | The duplication is now documented in the delicacy test comment but is still unpinned on the `plinth-metrics` side. |
| FU-6 | `chitubox-native-preview` fix `9ffc829` still awaits review and cherry-pick | Sole live carrier of the unlanded embed-native-preview fallback. |
| FU-7 | `make_toed_slab` parity investigation (vertex vs. interior containment probe) | The digest-moving containment-probe hardening was deferred; the current `make_toed_slab` required-table freeze (digest `11_859_964_443_161_510_103`) is **still PROVISIONAL** until this lands, and so is the 2026-08-05 seal re-pin (`4826202191472851835`) — the freeze is now provisional **on the new baseline**. See *Corpus defect fixes landed (2026-08-04, late)* and the cross-reference in *Post-merge interaction delta*. **CLOSED 2026-08-08, solved digit for digit, by the FU-10 phase-C re-pin** ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). The old vertex-parity probe was evaluated at the loop's minimum-`PointKey` vertex — for an abutting toe, exactly the touching point — at a measured distance of ~1e-15 mm from the neighbour's boundary; it was deciding SOLID vs. HOLE by f64 round-off, coin-flipping between adjacent 0.05 mm layers on a continuously-growing feature (evidence: [`attribution-slicer-census-2026-08-07.md`](attribution-slicer-census-2026-08-07.md) §1.3). FU-7 proposed to *harden* that probe; phase C instead *retires* it, replacing parity with the union engine's structural shape hierarchy. Both land on the same number: `plan.contacts().len()` **72 → 42**, digit for digit. This closes only the freeze-blocking, digest-moving containment-probe half of the row; the OD-9 `BuriedSurface` 3D `surface.inside` question that shares this row is a different probe on a different question, its code untouched by the cutover, and remains open — split out rather than closed by association. |
| FU-8b | ~~Treant support gate/load stage is quadratic~~ | **RESOLVED 2026-08-06.** All three defects fixed and the deferred cap bound then set on post-fix measurement. **(a)** (`0bdec0c`) `unique_island_face_count` / `visit_served_owner_faces` found distinct face ids by repeated-minimum selection sort; they now gather each island once and `sort_unstable` + `dedup` in one batch. **(b)** (`1706c5d`) every consumer that had to predict the extended load row count took the `(owner, face)` **pair** count instead of the distinct **face** count — 7,988,078 against 134,368, 59.5× inflated; the pairs are now grouped by face once (`GroupedOwnerFaces`) and the two counts are stated separately (`SpecializedLoadCounts`). **(c)** (`e0a4bed`) `finalize_specialized_load_partition` rescanned the pair list per base row and per pair (~6e13 uncharged comparisons, the 16-hour hang, 4,127/4,127 stack samples in that one frame); it now joins base rows to the existing grouping in one linear merge, proved against a verbatim copy of the replaced algorithm over a 4×5 cross product of shapes. **Then measured, not assumed:** the treant's honest post-fix demands are 12,714,828,544 gate operations on its proposal-free batch and 5,402,535,619 load operations over 134,368 rows **[corrected 2026-08-06 (record fixes F1, F2): 130,548 rows — the demand is right, the row count paired with it was not; and in (b) above, 7,988,078 → 130,548 is 61.2×, and it was a *prediction* that was inflated, not a row set]**, with load scratch (144,753,256 B) and gate peak (215,061,920 B) comfortably under their unchanged caps **[corrected 2026-08-06 (record fix F7): the gate peak is **216,203,088 B** = 215,061,920 + 1,141,168 face-scratch as of `23d929d`, which charged the batch's shared distinct-face buffer against the bound; 215,061,920 was the peak at `d82c895` only. The 1,073,741,824 B cap is unchanged and the batch is still at 20.1 % of it]**, at 2.84 GB peak RSS and 81.5 s to refusal. Both operation caps therefore became `max(floor, 64 × batch_product)` on measurement — **not** on face count, since 24-face `make_raking_blade` charges 612,887,212 gate operations against the 429,120-face treant's 12,714,828,544. Every corpus model stays bit-identical at the floor and the census prints each materialization. See `03-physics-and-constants.md` §3.10.10 and `support-resource-caps-2026-08-05.md` §7. The treant now clears the gate's proposal-free batch and the whole load stage; its remaining refusal is a **new** defect, FU-8c. |
| FU-9 | Hollow EDT margin defect | `grid_spec_from_bounds`'s no-padding path plus its all-foreground special case fabricate a distance ramp; the committed Sphere hollow goldens are wrong today. Est 8/16 pts; regenerates four `plinth-hollow` oracles. |
| FU-11 | Opt-in SDF/remesh repair mode | Roadmap item. |
| FU-13 | `DuplicatePhysicalSeat` error payload should carry position+routes instead of post-sort indices | Fold into any future `SpecializedError` change. |
| FU-14 | `coalesced_seat_count` is deliberately NOT in the plan digest | Kept fixtures byte-identical. Fold it in at the next authorized digest-moving change. |
| FU-15 | Corpus-gated end-to-end test tier, with loud skip for gitignored STLs | So the Cylinder and `make_box` sealed runs (digests `13310185350867878190` / `9657051231007502658`), currently test-uncovered, get pinned. |
| FU-17 | Contact-area deficits: 12 stub + 554 CCO model-anchor | RECORDED as data by the 2026-08-05 unified census, not adjudicated. These contacts attach over measurably less interface than their tip radius claims. This is the print-quality question OQ-8 asks the owner; the 554 CCO figure is moot unless PR-7 revives that family. **[Annotated 2026-08-12 by the drainage-doctrine scribe pass, per the journal-hygiene ruling that the archived work items must stay findable by name.** This row is the **live remnant of the archived model-anchor bundle** — critical-path item 6 of the 2026-08-05 *Ordered resumption checklist*, in [`archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md`](archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md), section *Critical path (2026-08-05) — the next five items, in order*. That bundle is three things treated as one unit, because landing any alone is incoherent: **(a) the tournament-bias investigation** (0 of 3,429 attempted model anchors survives corpus-wide, and all 3,186 attested anchor seats are `centreline = false` — find out whether the CCO tournament is *biased* against model anchors or whether they are correctly losing), **(b) PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT`** so a revived anchor has a proven interface rather than a voxel-rounding kiss, and **(c) retiring the commit-time `NotAttached` tripwire (`6a35d45`)**, which exists precisely to hold the line until (b). The bundle is **gated on owner ruling OQ-10** (revive the CCO model-anchor family with PR-7 seating, or leave it dead) — reviving it deliberately scars the mini. **The tournament-bias measurement comes first and needs no ruling**: it is the evidence the owner needs to answer OQ-10. **OQ-10 is queued for Craig's next briefing session** alongside OQ-8 and OQ-9. No claim in this row changes.**]** |
| FU-21 | Measure the **coverage-route kNN batch** | Opened by owner ruling R1 (2026-08-05). `SpecializedLimits::coverage_route_knn.max_work` now sits at the flat `SUPPORT_KNN_MAX_WORK_FLOOR` (100,000,000) precisely because **nothing has measured it** — the 696 work-units/face figure is the minima-to-face batch's, and borrowing it was the defect R1 removed. The treant's instrumented coverage-route demand is 15,282,619, but that is one point from a model that never reaches the stage in production. Measure the batch properly (it needs the FU-8b gate fix first, so the treant can get there), then either scale it on its **own** coefficient or ratify the floor on evidence. Until then the floor stands and `the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count` pins it. |
| FU-22 | Classify the **nested support authorities** R5 left out of scope | `CandidatePayloadConfig`, `ContactConstructionLimits`, `PenetrationBatchLimits` and `RaftLimits` — ~19 further values reachable from `SupportPreparationLimits::production` — still have **no registry entry**. They were excluded from R5 deliberately, not by oversight: `CandidatePayloadConfig` in particular mixes **result-changing refinement policy** with execution caps, so a blanket `runtime_resource_cap` sweep would misclassify them. This needs a per-field pass that decides, for each value, whether it can alter an admitted plan. See `03-physics-and-constants.md` §3.10.9. |
| FU-23 | Re-derive the two scaled coefficients when a **second large model** arrives | Standing trigger from owner ruling R4 (2026-08-05). `SUPPORT_KNN_MAX_WORK_PER_FACE` (2048) and `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` (512) rest on **n = 1**, the Weeping Treant v1.2 alone; the next largest corpus model is 90× smaller and sits under both floors. When a second model above ~4,760 faces is measured, re-run `next_power_of_two(2 × observed)` over **both** specimens rather than declaring the new model an outlier. The refs-per-face input is the one measured to vary 526 ↔ 2.83, so it is the likelier of the two to move. |
| FU-27 | An **unidentified intermittent failure** in `plinth-support`'s lib tests under machine load | **Opened 2026-08-07 by FU-8f phase G4.** Two separate `cargo test --workspace --offline --no-fail-fast` passes exited non-zero with `error: 1 target failed:` `-p plinth-support --lib`, both while the machine was still hot from G4's two 600 s treant runs. **Neither run's `failures:` block was retained, so the failing test is UNKNOWN**, and G4 deliberately does not guess. Four consecutive full-workspace runs afterwards are clean at **989 / 0**, the `--all-features` gate has never failed, and `specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads` — the 1/4/16-thread determinism test the FU-8f G2 section already recorded flaking under load, and the only known load-sensitive test in that binary — passes in isolation. **It is the obvious suspect and is NOT identified as the cause.** Two sightings in one session is past the point where "flake, re-ran, green" is an adequate record: a determinism test that only fails under load is either a real ordering dependence or a real resource sensitivity, and both matter. **First action is procedural, not diagnostic** — every gate run should tee to a file so the `failures:` block survives the next sighting; only then is there evidence to name the test. Until then **no claim is made that the two G4 sightings and the G2 sighting are the same defect.** See *FU-8f phase G4 measured 2026-08-07* §G4.8. **[Datum 2026-08-07 (FU-8f G3–G5 close-out verification):** five-gate workspace test runs came back clean **twice on a quiet machine AND twice while a concurrent `cargo test --workspace` ran in another worktree** — a non-reproduction under load, recorded as a datum, **not** as evidence the flake is gone.**]** **[CAPTURED 2026-08-08 (`feature/slicer-union` close-out) — the `failures:` block this row asked for, plus a `develop` control. The procedural first action is discharged and the flake is caught with its assertion intact rather than inferred.** **The failing test is `specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads`** — the obvious suspect this row named. Parent panic at `crates/plinth-support/src/specialized.rs:13692` (*"specialized 4-thread child failed"*); child panic at `specialized.rs:13654`, the guard `assert!(effective > 1 && participating > 1)`: **`required fixture corpus must exercise more than one worker at 4 threads`** (line numbers are `develop` `0f24b79`'s). **It reproduces ON `develop` `0f24b79` ITSELF**, in a detached scratch control worktree: **1 failure in 8 iterations** under 48 busy-loop CPU burners plus a concurrent `cargo build -p plinth-support` churn on a 16-core machine, where the same binary on the same tree passes **solo in 0.54 s**. The flake is therefore **pre-existing and environmental — NOT introduced by `feature/slicer-union`**, which does not touch this test at all (the branch's only `specialized.rs` change is the seat-coalescing gate's re-pinned expectations). **Load-sensitivity mechanism, stated as the hypothesis the capture is consistent with, not as proof:** the child asserts on `generic_slice_thread_evidence`'s *participating* worker count, and under CPU starvation an N-thread `rayon` pool can effectively serialize — the spawning thread drains every work item before any other worker steals one — so `participating` returns 1 and the guard fires. **The capture refines this row's assumed shape in one respect: the failure landed on the 4-thread child, not the 16-thread one**, so starvation does not need a 16-wide pool to produce it. **This capture is the LIKELY IDENTITY of FU-27 — hypothesis, not proof.** It is the same binary (`-p plinth-support --lib`), the same load-only failure, and the same cleanliness when quiet; but the two G4 sightings' `failures:` blocks remain lost, so they **cannot** be matched to this one directly, and the claim that they are the same defect is still **not** made. **At the `feature/slicer-union` tip (`d219d7b`), gate 2 (`--all-features`) ran TWICE on a quiet machine and was fully green both times — 1041 / 0 / 0, exit 0, no `failures:` block** — and the `develop` control's own gate 2 is **1012 / 0**, both consistent with the G3–G5 datum above. **No code change is made and none is proposed here**; the remedy (make the guard tolerate a starved pool, or pin the pool size the evidence is measured against) is a follow-up. **This row stays OPEN.**]** **[Sighting appended 2026-08-12 during the ruled batch.** `specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads` failed once under full parallel load with *"required fixture corpus must exercise more than one worker at 16 threads"* — its 16-thread child scheduled all work on one rayon worker — and passed in isolation, in a quiet `-p plinth-support --lib` run (475/475) and in two quiet full-workspace runs (1334/1334, exit 0). It was seen twice more across the batch's merge gates in the same shape. **This is now the third recorded session in which this one test is the only load-sensitive failure**, which strengthens the suspect without identifying a cause; nothing in the batch's diff can reach it (the batch's edits in that crate are in `tests/` and `examples/`, and the test lives in `src/`). No code change made or proposed here. **The row stays OPEN**, and the batch's own scribe deliberately did not open a duplicate row for it — see [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md) §5.1.]** |
| FU-28 | The **drilled hollow is producible but not supportable**: `run_support` refuses `plinth_hollow::run`'s own output | **Opened 2026-08-07 by export phase E2** (`export-path-design-2026-08-07.md`, *E2 LANDED* section), which was the first code anywhere to run the real hollow stage and the sealed support run back to back on a corpus model. The export design's OQ-1 asked whether `plinth_hollow::run` *completes* on corpus models and planned a typed solid-decision fallback for a refusal. It completes: the corpus Sphere hollows, plans drains and drills, returning `AwaitingInternalSupport`. **The refusal lands one stage later** — `run_support` rejects that 14,526-face drilled shell during `Preparation` (against `max_grid_bytes=67108864`, `knn_max_work=100000000`, so this is not obviously a resource-cap stop; the phase is `Preparation`, and the underlying refusal code was not extracted because doing so needs the `census` feature and E2 must not move engine behaviour). The oriented solid is 4,760 faces and seals fine — digest `17667468971446251870`, unmoved. **Consequence today:** every artifact the export path can emit for the Sphere is the *solid*, so hollowing buys the corpus nothing downstream, and the whole 7-of-8 sealed-run evidence base remains what F14 already said it was — `solid_decision` handoffs. **Consequence for E4:** `prepare()` cannot simply chain `hollow_stage` into `run_support` and expect the hollow to survive; it needs this diagnosed first or an explicit, recorded policy for which geometry ships. **First action is diagnostic, not a fix:** re-run the drilled shell under `--features census` and read `stage_errors` to name the actual `Preparation` refusal, then decide whether it is a mesh-property refusal (the drilled shell has an interior surface the oriented solid does not) or a resource stop. Pinned by `crates/plinth-pipeline/tests/first_artifact.rs`, which asserts the `DrilledShellRefusedBySupport` variant by equality, so the day this changes the gate fails and says so. Note also that **A5 internal-support planning does not exist anywhere in this workspace**, so even a supportable drilled shell would ship today with no internal supports. |
| FU-29 | The **OQ-3 adequacy stitch site has zero corpus coverage**, and its `max_loop_vertices` cap is an unexercised refusal path | **Opened 2026-08-08 as ruling condition (2) of the FU-10 phase-C re-pin** ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). §8.5 measurement (3) said the adequacy-probe path "moves zero rows on all seven models"; instrumentation at both stitch sites shows it **ran** zero rows: 2,956 print-layer rows against **0** adequacy rows, because `find_local_minima` always reaches `normalized_slice_layers(SliceStitchSpace::AdequacyProbe, …)` with an empty `probe_heights` — every local minimum matched a birth, so `suspects` is empty and the batch carries zero planes. "Moves zero rows" and "ran zero rows" are different claims and the record did not distinguish them. Consequence: the adequacy site's normalization has **no corpus-level evidence at all** — unit tests only — and the `max_loop_vertices` cap that phase-C deviation 2 adds there (a site that previously had no loop-vertex cap) is a **new refusal path that has never executed on real data**. This is a coverage gap, not a known defect. **First action:** construct a model, or a synthetic layer set, whose local minima do *not* all match births, so `probe_heights` is non-empty and the site executes on real geometry; then decide whether the cap belongs there at the value it has. |
| FU-30 | Corpus coverage of `coalesce_exact_regular_seats` **halved** at the FU-10 phase-C cutover | **Opened 2026-08-08 by the re-pin** ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). The specialized golden asserted `coalesced_seat_count == 1` for both corpus models; measured at the re-pinned tip it is `toed slab` **0** and `raking blade` **1**. With the corrected slice geometry `make_toed_slab` no longer produces a co-located regular seat pair at all, so the terminal coalescing pass is a measured no-op there and the count is now pinned per model. The pass is still exercised — by `make_raking_blade` and by `specialized`'s unit tests — but by one corpus model instead of two, and the `DuplicatePhysicalSeat` failure mode it was built to prevent now has a single corpus witness. The disappearance is *consistent with* the four phantom island births removed from `make_toed_slab` (print layers 49, 52, 226, 228) but that causal link was **not** independently proven; only the measurement is on the record. **First action:** decide whether one corpus witness plus unit tests is adequate, or whether a fixture should pin the duplicate-seat path deliberately rather than incidentally. |
| FU-33 | `rewire::optimize` is an **unbounded loop**: `REWIRE_WORK_BUDGET` is `TrialSetting::disabled()` and there is no wall-clock deadline anywhere in `plinth-support` for rewire | **Opened 2026-08-09 by FU-32 phase J5** ([`#fu32-j5-2026-08-09`](#fu32-j5-2026-08-09)), which charged routing and **deliberately did not charge this**. The shape is named from source in the design's §A2 #1 and ranked the **most likely next binder at ~55 %**: `rewire.rs`'s `'sweeps: loop` iterates `sweep_order` (one descriptor per live non-stub member, up to four kinds each) and **per move** does `RewireCcoBinding::bind` (a `forest.clone()` plus **two full `Forest::digest()` walks**), `derive_brace_attachments`, `total_volume_ml`, a whole-forest `validate()` per candidate, `changed_forest_volume_delta` per candidate, and `discover_rewire_anchor_seats` over **the entire 364,223-row anchor pool**. `REWIRE_WORK_BUDGET` is `Disabled` (`crates/plinth-core/src/constants/placeholders.rs`), so the loop has no budget at all and the only thing that has ever stopped it is an external `SIGKILL`. **Why J5 did not fix it, stated rather than assumed:** a work budget that stops a search early is **result-changing** — it changes which forest is returned and therefore every downstream digest — whereas J5's routing charge is result-inert by construction (proved: all seven OD-8 digests byte-identical at its tip). Landing both in one phase would put a value-moving ruling inside a resource-charging phase, which is the mixing FU-10 protocol exists to prevent. **What this row owes:** measure `rewire::optimize`'s real demand on the treant first (it has never been instrumented — the treant reached rewire for the first time at FU-32 J2), then decide between a charged preflight on a knowable worst case (J5's shape, result-inert) and an enabled `REWIRE_WORK_BUDGET` (result-changing, and therefore owner-ruled under FU-10 protocol before it is built). **Do NOT enable the budget without that measurement and that ruling.** No cap, floor or constant is touched by opening this row. |
| FU-34 | `plinth_hollow::run` **refuses the Weeping Treant's shell outright** — `InvalidShellTopology`, so the project's flagship model cannot be hollowed and gets no resin saving at all | **Opened 2026-08-09 by the treant `.chitubox` checkpoint** ([`#checkpoint-treant-artifact-2026-08-09`](#checkpoint-treant-artifact-2026-08-09)). **Number note: this row was adjudicated as "FU-33", which was already taken** by the `rewire::optimize` row directly above (opened 2026-08-09 by FU-32 J5); it is filed at the next free number instead and **no existing row was renumbered, edited or displaced**. **The measurement.** `target/release/examples/first_artifact "Weeping Treant v1.2"` at `063115f`, twice, unsandboxed with the 90 GB RSS watchdog armed: both attempts exit 1 at **22.29 s / 22.42 s** (peak RSS 1.69 / 1.72 GiB, watchdog never fired, 1200 s timebox never approached) with the byte-identical line `hollow: native shell did not contain one negative cavity enclosed by positive outer material`. The typed value is `plinth_hollow::HollowError::InvalidShellTopology` (`crates/plinth-hollow/src/lib.rs:1866`, category `Shell`), raised from the post-Boolean shell check — `orient_boolean_cavity` (defined `:995`, called `:863`) and `validate_shell_topology` (`:949`, called `:864`) — both of which require the shell to triage as `TriageError::PreHollowed` with **exactly one** cavity component and to certify ≥2 closed components with **exactly one** of negative signed volume. Which of the two sites fires is **not** distinguishable from outside; both carry the same message and separating them needs instrumentation, which the checkpoint run was forbidden to add. **This is a hard error, not an ineligibility:** `plinth_hollow::run` (`:351–370`) converts only `IneligibleNoCavity`, `IneligibleBelowBulk` and `IneligibleSavings` into a typed `HollowStageOutput::Solid`; everything else propagates, and this propagates. **Why it was never seen before, stated precisely:** the OD-8 census and every sealed-run gate bypass the hollow stage entirely — `embed_incidence_census.rs:142` builds `HollowedModel::solid_decision(mesh, orientation, BelowThreshold)` straight from `oriented.into_parts()` — so **FU-32 J6's 470 s seal is on the oriented solid and the treant had never entered `plinth_hollow::run` in project history** until this checkpoint. **The treant is not alone, and that was already on the record:** `export-path-design-2026-08-07.md`'s corpus-sweep addendum filed `Cylinder` and `make_box` as a "Refusal finding" with **this same message**, unfixed. So the count is **three of eight corpus models refused by the cavity check**, and the largest, most branched model is one of them — consistent with (but **not proved by**) the reading that a branching form yields several disjoint interior voids where the check demands one. **What this row does NOT claim:** that the check is wrong. It may be correctly refusing a shell that is genuinely not printable; nobody has looked at the geometry. **First action:** dump the treant's post-Boolean shell and count its cavity components and its negative-volume closed components, which decides between "the model produces a multi-cavity shell and the check is right" and "the Boolean/marching path is producing a broken shell". Only then choose a remedy. **Product consequence, stated because it is the reason this row is not cosmetic:** hollowing is where the resin saving lives, and the flagship model currently gets none. **The checkpoint artifact is NOT blocked by this** — it emits from the oriented solid through the `HollowStageRefused` fallback, with the refusal recorded verbatim in the artifact's own `SupportedGeometry` — and no cap, floor, constant or registry entry is touched by opening this row. |
| FU-35 | **FU-8f phase G6c's "a retained round executes no more than it is charged" invariant is FALSE on the `Cylinder`** — executed 23,203 against a charge of 4,146, a 5.6× overrun | **Opened 2026-08-09 by the treant `.chitubox` checkpoint** ([`#checkpoint-treant-artifact-2026-08-09`](#checkpoint-treant-artifact-2026-08-09)), found incidentally while gating the FU-34 fallback and **not caused by it**. `crates/plinth-support/src/specialized.rs:4858` carries `debug_assert!(executed_operations <= baseline_resource.operations)` under a comment stating the property is **"provable, not merely hoped"** and that it is "checked here every round in debug, and pinned by `fu8f_g6c_retained_round_executes_no_more_than_it_is_charged` in release". On the `Cylinder` it is false: the assertion fires with `a retained round executed 23203 load operations against a charge of 4146`. **Independent of the checkpoint's change, and proved so rather than asserted:** `examples/embed_incidence_census` shares none of the export-seam code and takes the same `HollowedModel::solid_decision(BelowThreshold)` handoff, and a **debug** build of it at this tip panics identically — `PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Cylinder" ./target/debug/examples/embed_incidence_census`. The census's own last flushed row before the panic reads `retained_load_charged_operations=4146 retained_load_executed_operations=4146`, so the overrun belongs to a **later** round than the one the meter had flushed. **Why nobody had seen it:** every routine consumer of this path is a **release** build, where `debug_assert!` is compiled out, and the release pin named in the comment is a unit fixture rather than a corpus run — so no gate in the workspace has ever carried the `Cylinder` into `run_support` in a debug profile. `make_box` on the same path is clean (executed 532 ≤ charged 716), which is why the checkpoint's new test uses `make_box`; the checkpoint deliberately did **not** pick a model that hides this. **What this row does NOT claim:** that the charge is too low, or that the meter is wrong, or that any cap should move — the direction of the defect is exactly what has not been established. It could be an under-charge (a real accounting gap) or an over-count in `state.executed_operations` (a metering bug costing nothing real); those have opposite remedies and **the evidence does not yet separate them**. **First action:** run the `Cylinder` in debug, capture the round index and the per-round charge/executed pair for every round rather than the flushed subset, and determine which of the two figures is wrong before touching either. **Do NOT raise the charge to make the assertion pass** — that would enshrine the defect as a budget, which is FU-8b's whole lesson. **No cap, floor, constant or registry entry is touched by opening this row**, and the treant's own artifact is unaffected: the treant does not trip this assertion and the checkpoint's release runs are unaffected either way. |
| FU-36 | **Every emitted sphere carries a 16-sliver south-pole seam**: `sphere_triangles` closes its north cap on `sin(0) == 0` but its south cap on `sin(PI) == 1.2246e-16`, so the emitted sphere is not exactly closed — and the one slackened bound in the emit closure test exists solely to accommodate it | **LANDED 2026-08-12 and MERGED TO `develop` in `647c991`** — commits `53d70d2` (fix), `18f30e0` (the mandated A/B record), `7becf8e` (two record overclaims retracted). Batch record: [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md); measurement: [the dated section](#ruled-batch-landed-2026-08-12). One open question survives the landing and is carried there: the longitude quadrants were deliberately **not** snapped (`cos(π/2)` = 6.12e-17), because they are needed for neither manifoldness nor north–south symmetry and the ruling does not mention them — a reader of "properly manifold object" might expect the same exactness in longitude. **The landing as first recorded on the branch follows.** `sphere_triangles`/`sphere_point` in `crates/plinth-emit/src/lib.rs` now read two `LazyLock` tables (`LATITUDE_TABLE`, `LONGITUDE_TABLE`) built once per process: the latitude table pins both poles and (`SPHERE_LATITUDES=8` being even) the equator to their exact values instead of raw `sin`/`cos`, and fills every southern entry as the EXACT mirror of the already-computed northern entry (`(sin, cos) -> (sin, -cos)`) rather than a second, independently-rounded `sin_cos()` call. The triangle emission loop, its two skip conditions and the 224-triangle-per-sphere count are untouched. **Five new invariant tests** land in `plinth-emit`'s existing test module, called directly against `sphere_triangles` over 6 centres (including the origin — the seam is bit-invisible at an off-axis centre) × 3 radii: `every_emitted_sphere_is_a_closed_two_manifold` (every directed edge pairs exactly once with its reverse, `V-E+F==2`, no degenerate face); `both_pole_caps_are_exact_single_apex_fans` (`center ± [0,0,r]` bitwise-exact, no stray vertex within `r*1e-12`); `the_generated_sphere_is_exactly_north_south_symmetric` (at the origin: full vertex set closed under negating z alone, AND the two pole-cap fans — 32 of 224 triangles — closed under "negate z, reverse winding" [the full 224-triangle multiset is deliberately NOT asserted mirror-closed: every band away from the poles is split on a fixed diagonal regardless of hemisphere, a pre-existing, out-of-scope tessellation property, not a symmetry defect — measured, not assumed]; at every centre: the shared latitude table itself proven exactly mirrored, which by determinism implies every ring pair is congruent everywhere); `the_sphere_triangle_count_and_soup_length_are_unchanged`; `sphere_triangles_still_refuses_a_non_positive_or_non_finite_radius`. **THREE of the five were verified to FAIL on the pre-fix generator; the other two pass on both sides by construction.** *Corrected 2026-08-12 on review* — this row first said "**Each** was verified to FAIL on the pre-fix generator (stash/test/unstash)", which is **false for two of the five** and is retracted rather than quietly reworded. **Procedure, re-run 2026-08-12:** a whole-fix stash does not compile (the symmetry test dereferences `LATITUDE_TABLE`, a static the fix introduces), so `build_latitude_table` **alone** is reverted to the pre-fix rule (plain `theta.sin()`/`theta.cos()` at `PI*k/SPHERE_LATITUDES`), which reproduces `develop`'s latitude values bit-for-bit, then `cargo test -p plinth-emit --lib`, then restore. **FAILED (the three that exercise the repair):** `both_pole_caps` with south apex `[3.96e-17, 1.64e-17, -r]` instead of `[0,0,-r]`; the manifold test with unmatched directed edges; the symmetry test because `(0,0,-r)` has no exact mirror in the pre-fix vertex set. **PASSED, and necessarily so (the two regression pins):** `the_sphere_triangle_count_and_soup_length_are_unchanged` — the fix moves vertex coordinates only, and the capacity formula plus both skip conditions are byte-identical to `develop`, so 224 was never in question — and `sphere_triangles_still_refuses_a_non_positive_or_non_finite_radius`, whose `!radius.is_finite() || radius <= 0.0` guard is likewise byte-identical. Those two are **pins against a future regression, not evidence this defect is caught**; the three above are the regression tests the ruling asked for. **Correction to this row's and `05-chitubox-and-corpus.md`'s prior claim** that the `1e-12` bound exists "solely" for the seam: measured in-tree (both before and after this fix, on the crate's own closure-test fixture, f32 wire), the seam's actual contribution was at most **1.52e-17** — five orders below the bound. The residual is f32 wire-quantization noise, present both before (1.52e-17 max) and after (1.5200e-17 max, record 2/JointSphere/origin frame) this fix. **The bound is tightened to `1e-15`** (~65x headroom over the measured maximum, three orders tighter than `1e-12`) and the docstring is rewritten to name the real residual. **Never widened**, per FU-8b. **A/B pass, measured not assumed** (`cargo build --release --offline --example first_artifact -p plinth-pipeline`; `--all` plus `-- "Weeping Treant v1.2"`; BEFORE = this lane's parent commit via `git stash`, AFTER = the fix). Of the seven sweep models, **`Sphere`, `make_overhang_table` and `make_raking_blade` gate-refuse on `UncoveredIslands` identically on both sides** (no artifact, unrelated to this fix). **The Weeping Treant gate-refuses on both sides too**, on an unrelated, pre-existing cause (`HollowStageRefused`: drain-planning memory `10493918112` exceeds the retained H0 cap `1073741824`) — not a sphere or winding defect. Four models are byte-comparable: `Cylinder` (298/298 records, 1,324,536/1,324,536 bytes, TIP(7) 19/90720 scalars moved across 3 records, SHAFT(9) 12/72576 across 2), `make_box` (69/69, 343,392/343,392, TIP(7) 30/22176 across 2, SHAFT(9) 15/20160 across 1), `make_toed_slab` (402/402, 2,110,344/2,110,344, TIP(7) 59/129024 across 4, SHAFT(9) 45/195552 across 3), `make_thin_fin` (109/109, 560,688/560,688, TIP(7) 45/34272 across 3, SHAFT(9) 45/44352 across 3) — full hashes and the per-model table are in `05-chitubox-and-corpus.md`. **Movement is confined to TIP(7)/SHAFT(9) on every comparable model, zero on TAPER(1)/JOINER(3)/FLARE(4)/PAD_OR_BRIDGE(6)**, record count and byte length unchanged everywhere — matching the ruling's stated expectation exactly. `cargo test --workspace` green, `cargo clippy --workspace --all-targets` clean, both re-run after landing; `exports/` left clean, nothing from the A/B pass committed. **Row as first written, and the owner's ruling that authorized this work, both follow, left standing.** **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §10) — **FIX NOW; the row stays open as the work of fixing it.** Not because the seam is harmful — it is ~1e-16 of the sphere's area and changes no winding decision — but **as principle: a generator we own must produce a properly manifold object, symmetric north pole to south pole.** This is our own code emitting geometry we would refuse from anyone else's. **How it lands:** the fix gets **its own measured byte-moving pass**, under the **A/B process recorded in [`05-chitubox-and-corpus.md`](05-chitubox-and-corpus.md)** — corpus artifacts re-measured against the recorded baseline, per-record byte movement measured rather than assumed, exactly as this row's "first action" set out. **Afterwards the `1e-12` closure bound is TIGHTENED**, because the accommodation it exists for is gone with the seam. **The bound is never widened** — if a future closure defect exceeds it, the geometry is wrong, not the bound. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-09 by the `wind_outward` ruling execution** (`05-chitubox-and-corpus.md` §2, the *Executed (2026-08-09)* record's second measured item, where the full derivation lives). **Number note: filed at the next free number under the recorded collision protocol** — FU-34 was appended to this same table earlier on 2026-08-09 by concurrent checkpoint work, which is precedent, not an obstacle — and **no existing row was renumbered, edited or displaced**. **The measurement.** `crates/plinth-emit/src/lib.rs`'s `sphere_triangles` walks its rings on `sin(theta)` with `theta` spanning `0..=PI`; `f64::sin(PI)` is `1.2246467991473532e-16`, not `0.0`. With `SPHERE_LATITUDES = 8` and `SPHERE_LONGITUDES = 16`, the north cap therefore closes on a single true point while the south cap's 16 fan apices land on a **16-gon of radius `r * 1.2246e-16`** instead of one point — so each sphere soup is a closed surface with a 16-sided sliver seam of about `1e-16` of its area. Every `ContactSphere` (type 7) and `JointSphere` (type 9) record this crate writes carries it; on the seven-model corpus sweep that is **624 of 1,728 records** — 257 type-7 contact spheres and 367 type-9 joint spheres. **Why this is a ledger item and not just a curiosity:** the closure bound in `every_emitted_soup_is_closed_which_the_winding_decision_requires` is `1e-12` rather than exactly `0.0` **solely** to accommodate this seam — the seam clears it by four orders and a real leak would not — so the single loosened bound on the emit side has, until this row, had no tracked item to trace to, and the implementer's own test comment asks for an owner ruling that a topic-journal note alone would never prompt. **What this row does NOT claim:** that any emitted byte is wrong, or that any artifact already shipped is unsound. The area defect is ~`1e-16` of the sphere and it changes no winding decision — the signed-volume sign is stable across it, proved on the same seven-model sweep (1,728 soups, every one reading outward, 257 correctly reversed). **This is not a bound to raise:** `1e-12` is the accommodation already made and it must not be widened; if a future closure defect exceeds it, the geometry is wrong, not the bound (FU-8b's standing lesson). **Why it was not repaired when found:** the `wind_outward` change was a relocation with a bit-identity acceptance condition, and **repairing the seam moves emitted geometry**: the 16 south apices move in x and y from `r*1.2246e-16` to `0.0` in every sphere record the crate writes. Whether that reaches the wire is per-record and must be measured rather than assumed — the apices are `center + r*sin*cos`, so for a sphere centred well away from the model origin the offset rounds away in f32 and the bytes hold, while near the origin, or if the now-degenerate slivers are dropped rather than kept (which changes the record's triangle count), they do not. Either way it is a numerical change needing its own phase, and the relocation's acceptance condition was bit-identity, so it could not be made there. **First action:** an owner ruling on whether the seam is repaired at all (it costs nothing today), and if so, that the repair lands in a change permitted to move emitted bytes, with the corpus artifacts re-measured against the A/B baseline recorded in `05-chitubox-and-corpus.md` and the `1e-12` bound **tightened** rather than left slack once the seam is gone. |
| FU-37 | **The selected hollow wall is recorded in-process only — no persisted artifact says which wall built it** | **MERGED TO `develop` in `b71ddbc`** (commits `679b78e` + plumbing `790fed7`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). The requested-vs-realized question the lane filed is **resolved by what landed**: two distinct fields, `wall` (requested, FU-37) and `realized_wall_mm` (FU-55), living side by side on the top-level report and both reaching `ChainOutcome`/`ModelPreparationOutcome::Artifact` and the CLI. The "goldens re-pin together as ONE change" clause is **vacuously satisfied and verified empirically** — nothing on disk serializes a `HollowReport`, so the affected-golden set was empty. **LANDED 2026-08-12** ([`#hollow-wall-field-2026-08-12`](#hollow-wall-field-2026-08-12)) — `HollowReport::wall: Option<Finite>` (`#[serde(default)]`, the `cavity_repair`/`realized_wall_mm` forward-compatibility pattern), populated on every production path: the hollowed-and-drilled path, every typed `SolidDecisionReason` `run` converts a hollow-stage failure into, and the internal-support refusal substitution. Only the hand-built `HollowReport::solid_decision` test stand-in carries `None`. Nothing on disk serialized a `HollowReport`, so no golden needed re-pinning; the in-crate call sites the new parameter touched are documented in the journal section. **COMPLETED 2026-08-12 by the pipeline plumbing** ([`#hollow-wall-reaches-run-output-2026-08-12`](#hollow-wall-reaches-run-output-2026-08-12)) — a review finding caught that the field landed but the report DIED inside `seal_and_emit`, so the row's own motivating claim ("two artifacts prepared at 2.5 mm and 3.0 mm are byte-indistinguishable in metadata in every file the pipeline writes") stayed true of real runs while the isolated `HollowReport` tests passed. `ChainOutcome` and `ModelPreparationOutcome::Artifact` now carry the requested wall and FU-55's realized wall, and the CLI prints both. Remaining gap, recorded rather than claimed closed: the `.chitubox` wire format itself still carries no wall. No cap, gate, floor or tolerance moved. **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §8) — **the field's home is decided; the row stays open as the work of adding it.** **The requested wall goes on the TOP-LEVEL `HollowReport`, as a `wall` field** — not on the drilling evidence, which records holes rather than the shell's defining parameter. **The affected frozen goldens re-pin together with the field addition, as ONE gated, documented change**, exactly as this row's deferral anticipated. **Note only, binding nothing and scheduling nothing:** that field is the natural future home for FU-53/FU-55's **realized** wall figure recorded beside the requested one, so an artifact could say both what wall it was asked for and what wall it got. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-09 by the wall-knob restoration review.** **Number note: this row was opened as "FU-36" on `feature/hollow-wall`, and "FU-36" was already taken** by the sphere south-pole-seam row directly above (opened 2026-08-09 by the `wind_outward` ruling execution, landed on `develop` first); it is filed at the next free number instead and **no existing row was renumbered, edited or displaced** — the same collision protocol FU-34 records. `CavityEvidence::wall_thickness_mm` (`crates/plinth-hollow/src/lib.rs`) does record the operator's stated wall, and that is what §3.10.3 leans on, but `CavityEvidence` is **not `Serialize`**: it hangs off `HollowedModel::cavity_authority` and dies with the process. The serialized `HollowReport`, `CavityRecord` and `DrillingEvidence` carry **no wall field**, so two artifacts prepared at 2.5 mm and 3.0 mm are byte-indistinguishable in metadata in every file the pipeline writes — the geometry differs, the record does not say why. **What this row does NOT claim:** that the wall is unrecorded (it is, in-process, and the CLI refuses walls the lattice cannot produce), or that any gate is wrong. **Why it is deferred rather than fixed in the restoration commit:** adding a wall field to `HollowReport` changes serialized report bytes and therefore moves frozen goldens, which is a separate, gated change with its own re-pin. **First action:** decide whether the wall belongs on `HollowReport` or on the drilling evidence, then land the field and the golden re-pin together. Until then §3.10.3 states the in-process limitation explicitly rather than claiming persisted auditability. **No cap, floor, constant or registry entry is touched by opening this row.** |
| FU-38 | **Four harnesses still feed an all-zeros delicacy field, so they measure a support run production no longer performs** — and correcting any of them moves pinned digests, which is an owner decision, not an implementer's | **LANDED 2026-08-12 directly on `develop` in `fbbc099`**; batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **This row is the batch's one recovered non-compliance and the record says so rather than smoothing it over:** the adversarial critic found FU-38 present only as *uncommitted working-tree changes* that moved the delicacy feed without re-pinning a single digest, leaving `develop`'s tree red on three tests. It was completed at `fbbc099` with every digest re-pinned and **both** columns re-measured at that tip (the harness was built twice, once with the `vec![0.0; faces]` feed restored verbatim), and the section its own code comments promised now exists. `cargo test --workspace` is green. **Two questions survive and are carried in the batch record:** the ruling's item 2 named two J4 digests but `make_raking_blade`'s was retired by FU-56 (a refusal produces no sealed authority) — the conservative path taken was to prove the blade's refusal byte-identical under both fields rather than manufacture a digest; and `make_overhang_table`'s census movement (73 → 74 routed) belongs to FU-56 and has no FU-56-side attribution anywhere. **LANDED 2026-08-12** ([`#fu38-real-delicacy-repin-2026-08-12`](#fu38-real-delicacy-repin-2026-08-12)) — **all four decision-relevant instruments now compute the production delicacy field** (same `"prepared"` LOD name, same registered `DelicacySettings::production()`, same scaled `DelicacyBuildConfig::production(faces)` as `plinth_pipeline::prepare::prepared_delicacy_field`; inlined because neither `plinth-support` nor `plinth-verify` may depend on `plinth-pipeline`), and **every affected digest carries its from→to**. **THE FULL SEPTET-AND-MORE IS IN THE DATED SECTION**, measured at this tip rather than copied from FU-32 J4 — the coverage-closure lane had already moved `make_overhang_table` and taken `make_raking_blade` off the sealed list, and charging FU-38 for another lane's movement would have been a false attribution. **Six OD-8 census digests move:** `Sphere` `2297157889062365577` → `686822247060573262` (routed 7→9, live 15→21), `Cylinder` `6853793710942395608` → `12410021645461158056` (54→45, 119→101), `make_box` `8709076999423814412` → `5356317665317762409`, `make_overhang_table` `11652461737312715963` → `14600685200423759332`, `make_toed_slab` `133000971864625681` → `16855162491340010017`, `make_thin_fin` `14656364680512457190` → `14718557305548010128`. **Unlike J4, counts DO move on two models, and that is the finding** — a harness fed zeros was selecting different contacts from the ones production selects, which is exactly what this row claimed. **J4 digests:** the procedural `make_toed_slab` `2198459883570957390` → `15556275517430246244`, counts unmoved at 64/170/69 seats and `coverage_repair_rounds()` still 0. **Coverage-gate golden:** the same `2198459883570957390` → `15556275517430246244`, and the from→to deliberately **spans the coverage lane's rename** (`coverage_gate_overhang_table.json` → `coverage_gate_toed_slab.json`, `1d407a0`/`b9decd5`) rather than citing the retired file; regenerated through the test's own `THREAD_CHILD` producer path, never hand-edited, then diffed leaf by leaf — **181 scalar leaves, EXACTLY 1 moved**, no evidence lost, still 9 blocking defects. **Item 4 (structural asserts + hand-counted resource plan) was re-derived and is UNCHANGED** — the 11-tuple plan, all 9 islands, all 9 patches — measured, not assumed. **The deferred `make_raking_blade` measurement landed as part of the re-pin and moved nothing:** byte-identical `AttemptsExhausted` refusals under both fields, on both the corpus and procedural fixtures, so `74030269957898085`'s retirement is FU-56's doing and not this row's. The fifth, length-only feed was not touched. **NO cap, gate, floor, radius, bound, tolerance or registry entry raised, lowered or touched; no engine code changed.** **Recorded, not claimed:** `Weeping Treant v1.2` fails `Failed(Preparation)` in the census under **both** fields, so FU-38 neither caused nor cured it — see [FU-58](#follow-ups-2026-08-04). **The row as first written follows, left standing rather than substituted.** **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §5) — **the harnesses move to the real field; the row stays open as the work of moving them.** **Update ALL FOUR instruments** — the **OD-8 census digests** (`embed_incidence_census.rs`), **both J4 snapshot digests** (`support_runner_integration.rs`: `make_toed_slab` and `make_raking_blade`), the **coverage-gate golden** (`coverage_gate.rs` / `coverage_gate_overhang_table.json`) and that geometry's **structural asserts** plus hand-counted coverage resource plan — **re-pinning every affected digest with from→to attribution**. The fifth, length-only feed is not decision-relevant and is not touched. **No digest is claimed wrong:** each remains exactly reproducible by the harness that produced it, and each measured the zero-field engine, which is what the engine then was — re-pinning records that the engine changed, not that the measurement was bad. **Nothing else moves:** no cap, gate, floor or bound, and the deferred `make_raking_blade` measurement lands as part of the re-pin rather than ahead of it. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-10 by the delicacy-producer landing; widened the same day after review** ([`decisions/2026-08-10-delicacy-producer.md`](decisions/2026-08-10-delicacy-producer.md)). The 2026-08-10 owner ruling built the producer-side delicacy evaluator and replaced `prepare.rs`'s `vec![0.0; prepared_faces]` feed to the support stage with the measured field. **This row first named exactly one surviving feed. There are five, four of which reach a support run.** The full enumeration, with the per-harness verdict, is the decision record's "Every surviving zero-field feed, and whether it matters" table; in summary: (1) `crates/plinth-support/examples/embed_incidence_census.rs:171` — the seven pinned OD-8 census digests over the eight-model corpus, **decision-relevant**; (2) `crates/plinth-support/tests/support_runner_integration.rs:167` — the pinned FU-32 J4 snapshot digests for `make_toed_slab` (`2198459883570957390`) and `make_raking_blade` (`74030269957898085`), **decision-relevant** (`make_toed_slab` is *measured* to move; `make_raking_blade` carries 5 of 24 faces above the 0.5 anchor cap and is the one corpus model where both delicacy channels are live, and it is **not** covered by the prepare-fixture neutrality guard); (3) `crates/plinth-verify/tests/coverage_gate.rs:88` — the frozen golden `coverage_gate_overhang_table.json` with its pinned `snapshot_digest`, **decision-relevant** (`make_overhang_table` is *measured* to move); (4) `crates/plinth-support/tests/support_runner_integration.rs:222,257` — the same geometry's structural asserts and hand-counted coverage resource plan, **decision-relevant**; (5) `crates/plinth-support/tests/resource_cap_policy.rs:44,53` and `crates/plinth-pipeline/src/lib.rs:816` — **not** decision-relevant: those slices are read for their **length** only (`SupportPreparationLimits::input` treats `delicacy_by_face.len()` as the face count) and no run consumes their values. **The measurement that widened this row.** The recorded neutrality argument rested on a false premise — that `strut_model_anchor_max_delicacy` is the support stage's only delicacy-driven decision. It is not: contact selection breaks exact-Z ties on delicacy (`crates/plinth-support/src/lib.rs:1281`), and exact Z ties are ordinary on a flat underside. `the_zero_field_divergence_is_measured_per_committed_fixture` now seals each committed prepare fixture twice and compares: `make_toed_slab` `13640335649674000245` → `6989898566632288617` and `make_overhang_table` `3706765241747593242` → `3116329269026579872` both **move**, neither crosses the cap, and `Sphere` refuses identically under both fields. **What this row claims:** four harnesses and production have diverged in one named input; the divergence is measured for three of them and named-but-unmeasured for `make_raking_blade` (measuring it means re-pinning, and re-pinning is the owner's). **What it does NOT claim:** that any pinned digest is wrong (each remains exactly reproducible by the harness that produced it), that past measurements are invalidated (they measured the zero-field engine, which is what the engine then was), or that any gate, cap or bound should move — none did. **The question before the owner:** re-point these harnesses at the real field and re-pin every affected digest with a from→to attribution, or leave them pinned to the zero field as frozen historical instruments and say so in the record. **First action:** the owner ruling; the remaining attribution measurements are cheap and come after it, not before. |
| FU-39 | **A searched orientation cannot be pinned yet — nothing persists it to disk** | **Opened 2026-08-10 by the dense-orientation-search landing.** The winner arrives with its full evidence attached: `OrientationProvenance::DenseSweep(Box<DenseSweepRecord>)` carries the grid, the LOD manifest, every tier's complete ranking, the fidelity ledger, the work counters and a `search_digest`. All of it lives **in process only**, exactly as the pre-search `OrientationReport` did. The design's §5.3 sidecar (`<out-dir>/<stem>.orientation.json`, written only on the success path inside the emit branch so a gate refusal still leaves no files), `PinnedBaseline::format: BaselineFormat{LegacyPython, PlinthOrientation}`, and the `model_fingerprint` check that would close the recorded hazard "the pinned path cannot check that the report and the mesh describe the same object" are **all unbuilt**. **What this row claims:** a run's orientation decision is reproducible by re-running the search (it is deterministic, and `search_digest` proves it), but it is not *replayable from an artifact*, so design test T13's round-trip — `--orient dense`, take the sidecar, rerun `--orient pinned:DIR`, byte-identical `.chitubox` — cannot be written. **What it does NOT claim:** that `--orient pinned:DIR` is missing; it exists and replays the legacy `<stem>.pipeline.json` format today. **First action:** land the sidecar write plus the native-format reader together, since a writer without a reader is an artifact nothing consumes. |
| FU-40 | **The legacy-13 comparison driver and `plinth calibrate` are unbuilt, so the sweep's fidelity is recorded but not gated** | **Opened 2026-08-10 by the dense-orientation-search landing.** Two recorded pieces are deliberately absent. (1) The legacy-compat **driver** — block-0 candidates only, `PeakSectionMethod::LegacyStepped{margin_mm: 0.1, step_mm: 4.0}`, the literal `-0.707` cutoff, the L1 proxy, scar off — which `04-migration-plan.md:1009-1012` records as a driver rather than a policy, and which is what design tests T1 (reproduce the recorded argmin for all 8 baselines) and T2 (the six recorded scores as a one-sided bound) need. (2) `plinth calibrate` and D-12's own safety valves: Spearman(coarse, exact) ≥ 0.7 or the sweep is demoted, exact-best inside the top-24 or K doubles, outside the top-48 after one doubling → the recorded fallback ladder (02:856-857). **What this row claims:** the sweep measures and records its own fidelity honestly — `coarse_rank_of_final_winner`, `refine_rank_of_final_winner`, `proxy_agreed_with_exact`, `decision_resolved`, the winner–runner-up angle beside the grid spacing — but **nothing acts on any of it**, so a proxy that degrades is visible and not stopped. Two concrete cases are already waiting for those gates: the committed `Sphere`, whose winning pose the L1 proxy ranks **478th of 643** (which is why the legacy block is escorted to the exact tier — [`decisions/2026-08-10-legacy-block-escorted-to-the-exact-tier.md`](decisions/2026-08-10-legacy-block-escorted-to-the-exact-tier.md)), and the fact that `decision_resolved` reads **false on all eight** corpus models — re-measured 2026-08-10 after the flag was corrected to compare margin and spread at the **same** tier (it first compared an exact-tier margin against a ~2 % proxy spread, which biased it toward `false` for reasons unrelated to the grid; on the `Sphere` that proxy spread is 674.08 score points against a committed margin of 0.0059). The verdict now carries its tier in the record (`margin_tier`, `spread_tier`) and reads as "did the coarse grid separate these two directions", never as "did the pipeline resolve the choice". **What it does NOT claim:** that any orientation shipped so far is wrong. **First action:** the driver, because T1 is the cheapest evidence that the objective did not change meaning, and it needs no new constant. |
| FU-41 | **Rest poses are an area-binned substitute for convex-hull face-aligned poses, and three-point rest poses are not covered** | **Opened 2026-08-10 by the dense-orientation-search landing.** D-12 names convex-hull **face-aligned** poses. This workspace has no 3-D convex hull kernel in any crate or in the lockfile, and the lockfile is frozen, so the shipped substitute bins each face's down direction `-n_f` into a 2048-point reference Fibonacci lattice, accumulates area, and emits the area-weighted mean direction of the heaviest `ORIENT_REST_POSE_MAX` bins — O(F), no new dependency, and it finds a cube's six axis poses exactly. **What this row claims:** a genuine three-point rest pose — a treant balanced on three toes, a figure on a narrow base — has no single large facet and this substitute will not propose it as its own candidate. **What it does NOT claim:** that such a pose is unreachable; every direction on S² is within one grid spacing (~9.0° at `ORIENT_FIBONACCI_COUNT` = 512) of a block-2 Fibonacci candidate, so the loss is bounded by the grid resolution rather than unbounded. **First action:** decide whether a hull kernel is worth a lockfile change at all — the honest prior is that it is not until calibration (FU-40) shows a model whose best pose the grid misses by more than the spacing. |
| FU-42 | **The dense sweep's closed-boundary precondition is enforced on the Exact tier, which makes a not-perfectly-closed mesh unpreparable under the default policy** | **Opened 2026-08-10 by the dense-orientation-search review.** The exact section solver's shoelace identity assumes each section is a union of closed loops, so `OrientFaceTable` refuses any LOD with `boundary_edges > 0` after `WeldPolicy::ExactBits` welding. Design §2.1 put that floor on the **proxies**, where the LOD ladder catches it and answers with a finer rung. Enforced on Exact there is no finer rung: it becomes `SearchError::ExactUnscorable`, and since `--orient dense` is the default, such a model is unpreparable — while `--orient pinned` still prepares it, because the pinned path never builds a section profile. **What this row claims:** this is a **new refusal class** the record did not previously carry, and it is now recorded rather than latent. **What it does NOT claim:** that the refusal is wrong. It is arguably necessary — a hole makes the identity produce a plausible *wrong* area rather than a slow one, and a silently wrong peak cross-section would be scored against 653 candidates. **Reachability, measured 2026-08-10:** all eight corpus models weld closed — `boundary_edges = 0` on `make_box` (12 faces), `make_raking_blade` (24), `make_thin_fin` (24), `make_overhang_table` (60), `make_toed_slab` (140), `Cylinder` (244), `Sphere` (4,760) and the Weeping Treant (429,120), and `nonmanifold_edges = 0` on every one. No input class in the corpus reaches it. **First action:** decide whether a hole should refuse the *model* or degrade the *term* — the honest options are a typed per-model refusal (today's behaviour, now named), or a section method that tolerates open boundaries and is recorded as a different, weaker measurement. Do not widen it silently in either direction. |
| FU-43 | **A candidate-level failure inside the ratio search still ends the whole run, and the exactly-collinear output triangle is the case that fires it** | **Opened 2026-08-10 by the zero-volume-pair landing** (`feature/decimate-dupfaces`), which fixed the duplicate-face half of meshoptimizer's non-link-condition collapse and deliberately did **not** touch the search's control flow. **The defect.** `simplify_locked` can fail on one candidate for reasons that are facts about *that ratio*, not failures of the machinery — `DegenerateOutputTriangle`, `AllFacesWereZeroVolume`, `EmptyResult`, `LockedVertexRemoved`, `CoincidentFacesRemain`, `ZeroVolumePairOpensBoundary`. Every one of them propagates out of `sampled_step` / the final-verification loop / `run_adaptive_diagnostics` as `BudgetSearchError::SearchExecution`, so one unusable ratio ends the entire decimation instead of being recorded as a refusal and stepped past. **The live instance: `zombie-2-a.stl` (397,226 faces) fails production decimation outright at r = 0.525**, on output triangle 63,345, whose three surviving vertices are `(-1.0828016996383667, -2.585498571395874, 15.263912200927734)`, `(-1.0678989887237549, -2.5856692790985107, 15.246969223022461)` and `(-1.052996277809143, -2.5858399868011475, 15.230026245117188)` — three *distinct* f64 points that are exactly collinear and evenly spaced (the middle one is the exact midpoint), i.e. three consecutive vertices of a straight source edge that a collapse flattened into a needle. `robust::orient2d` returns exactly 0 on all three projections, so `audit_strict` refuses it; meshoptimizer measured the same collapse in **f32**, where the cross product rounds to a small non-zero value with an arbitrary sign, and cannot see it. Model coordinates come from binary STL and are f32-exact, which is why exact collinearity is common enough to hit rather than a measure-zero curiosity. The same model is clean at r = 0.2875, 0.2 and 0.16875, so a search that recorded the refusal and continued would have found a candidate. **Two parts, in order.** **(1)** Make a candidate-free `simplify_locked` outcome a recorded `ReductionRefusalReason` and a bisection step (`high = midpoint`, the branch a meshoptimizer no-op already takes, since only a lower ratio can produce a candidate), not a `SearchExecution` error. Note the constraint this must respect: the corpus gate's independent audit asserts `trace.steps.len() == 6` for a `Verified` model, so a refused step has to be counted somewhere it can still see. **(2)** Only then decide whether the needle itself is repairable. There is an *exact* repair available and it is worth measuring before anything approximate is considered: the needle `(p0, p1, p2)` has `p1` exactly on segment `p0 p2`, so deleting the needle and splitting the single face across its long edge `(p0, p2)` at `p1` is geometry-preserving to the bit — it moves no vertex and changes no surface point — costs zero net faces, and closes the slit that plain deletion would open. It needs the long edge to have exactly two incident faces and both replacement triangles to pass `audit_strict`, both of which are checkable rather than assumed. **Scope note:** the corpus-wide census behind this row (38 models × 7 search ratios, local `minis-to-process` plus `corpus/models`) found the collinear failure exactly **once**. What is *not* rare is part (1)'s exposure: `make_box`, `make_overhang_table`, `make_raking_blade` and `make_thin_fin` all reach `AllFacesWereZeroVolume` or `EmptyResult` at the deeper search ratios, and today they are saved only by refusing earlier — `make_box` on the new `FeatureProbeHasNoCandidate` path and the other three on `ProtectionCap`. A model that reaches those ratios through the bisection would hard-fail. **Nothing here is a gate change and no bound may be raised to close it.** **[Part (1) LANDED 2026-08-11 on `feature/decimate-dupfaces`, in the same amended commit as the zero-volume repair; part (2) remains open and nothing above is retracted.** The six candidate-free outcomes are collected in `CandidateFreeCause`, recorded as `ReductionRefusalReason::CandidateFree`, and stepped past at every site that could end the run — the bisection, the final-verification loop and the adaptive dense sweep. `LockedVertexRemoved` is deliberately **not** a member: it says meshoptimizer violated a hard constraint it was given, which is a fact about the machinery, not about the ratio; its sibling `LockedVertexLostToZeroVolumeRepair` *is* a member, because there the simplifier honoured the lock and this crate's own pair removal is what would have dropped the vertex. **Two corrections to this row's own prescription, both found in review before merge.** (a) The direction is **not** `high = midpoint` uniformly. That is right only for `DegenerateOutputTriangle`, whose candidates are on the denser side (`zombie-2-a` is clean at r = 0.2875, 0.2, 0.16875); for the other five a lower midpoint is strictly *more* aggressive than the one that just failed, so a single refusal would burn the whole remaining bisection budget and a model with real candidates further up would fall out as `already_print_scale`. `CandidateFreeCause::bisection_response` answers per cause and is wired into the arm. (b) The `trace.steps.len() == 6` constraint this row names was **not** in fact respected by the first implementation: the arm pushed a refusal and no step, so any model that hit one unusable ratio and still verified would have failed the corpus gate on `M3SearchStepCount`. Latent only because all eight corpus members are `already_print_scale`. `BudgetSearchTrace::candidate_free_ratios` counts them and the gate now audits `steps + candidate-free ratios`, pinned by `the_step_count_audit_counts_a_candidate_free_midpoint_as_a_bisection`. Report schema `plinth/m3-decimation-corpus-budget-report/v8`.]** |
| FU-44 | **The decimator manufactures duplicate faces: 7 models that are CLEAN on disk are refused only after decimation, and the repair pre-pass runs upstream of it so it cannot reach them** | **Opened 2026-08-10 by the automatic-repair pre-pass landing**, under the standing doctrine of the same ruling — every refusal the pipeline still emits is an automation work item. **The measurement.** Of the 21 refused/suspect models, 13 trip the closed-solid gate at RAW. The other **7 — Cat wild shape, Morel base, Rotting Warhorse, Tavey Base, Thorny Vegepygmy, zombie-8-a, zombie-9-b — are clean raw**, and acquire their defects in `plinth-decimate`: Cat wild shape 82,564 → 53,150 faces **gains 2 duplicate faces and 31 point-only-cut faces**; zombie-9-b 373,748 → 51,974 **gains 8 duplicate faces**. Crawler Zombie's pre-existing damage is **amplified 50 → 634 cut faces** by decimation. Where the decimator declines to reduce, meshes pass through byte-identical, which is the control: the damage tracks reduction, not transit. **Re-measured at the pre-pass's own tip, 2026-08-10**, by `cargo run --release -p plinth-pipeline --example repair_probe -- minis-to-process` over all 37 STLs: **14 models refused raw and all 14 are rescued** by the pre-pass, and **11 — not 7 — are clean raw but refused after decimation**, so the class is larger than the census first counted. **Re-measured 2026-08-11**, after the review removed the duplicate action's keep-none disposition entirely (FU-46): the raw column is **unchanged** — 14 refused, 14 rescued, 0 pinches capped — and **14 models remain refused on the probe's decimated column** (the 11, plus Crawler Zombie, Giant Wolf Spider and zombie-4-a). The earlier "0 remain refused at either stage" reading came from those models passing *by deletion*: the decimated duplicates were coincident pairs removed **whole, with no survivor** (`dup-faces = 2 × dup-groups`), which is the deletion FU-46 reverted and puts to the owner. This row's subject is unaffected either way — the producer is still the decimator — but the "a second pass after decimation would fix it" reading is now known to be false under the landed rule. Adjacent observation from the same run, recorded because it is a refusal and the doctrine says refusals get recorded: **`zombie-2-a` (397,226 faces) errors out of `plinth-decimate` entirely**, reported as `n/a` in the probe's decimated columns. That is a decimation-stage failure of its own and is **not** claimed here to have the same cause as the duplicate-face manufacture; it wants its own measurement before it gets its own row. **Why the pre-pass does not fix it.** `plinth_mesh::repair_mesh` runs in `prepare_one` on the **loaded input**, immediately after `load_binary_stl`. That is the correct place for the raw-damaged 13 and it is, by construction, upstream of the stage that damages these 7. The `repair_probe` example's four-column output (raw / raw+repair / decimated / decimated+repair) exists precisely to keep this distinction measurable rather than inferred. **What this row does NOT claim:** that a second repair pass after decimation is the answer. It might be — but a decimator that emits duplicate faces is emitting a defect, and O-8 makes its output the sole production mesh, so the first question is whether the QEM path can be made not to produce them. Bolting a repair onto the output would hide a producer defect behind a consumer fix, which is the shape of mistake `06-conventions.md` §6's "the fix is always in the producer" rule exists to prevent. **What it must NOT become:** a reason to weaken the gate for decimated meshes, or to raise any decimation error bound. **First action:** on one model (Cat wild shape is the smallest at 82,564 faces), identify which meshopt simplification step produces the coincident triples — collapse-to-existing-vertex is the obvious suspect — and determine whether it is preventable at source. Only then decide between a producer fix and a second, post-decimation pre-pass. **No cap, floor, constant or registry entry is touched by opening this row.** |
| FU-45 | **A raw input larger than the triage admission ceiling gets no repair at all — the pre-pass declines every one of its three actions rather than refusing the model, and that decline is invisible except in the run report** | **Opened 2026-08-11 by the repair pre-pass review; corrected the same day by its follow-up review, which found the bound was defeated one line after it fired.** **The shape.** `plinth_mesh::repair::run_repair` needs a resource bound before it starts input-proportional work, and it takes the absolute one — `TriageTopologyLimits::production()`: 8,388,608 faces / 16,777,216 vertices / 512 MB retained. **What this row said first, and what was wrong with it.** The bound originally guarded only action 1, the dust-shard strip. One line after it declined that action, `build_face_topology(.., WeldPolicy::ExactBits)` ran **unconditionally on the same unbounded mesh** — a `BTreeMap` edge index over ~3× the face count, a per-welded-vertex incidence map, and materialized pinch fans, which is the *heavier* of the two operations — and the `WeldedEdges` indexes for actions 2 and 3 were unbounded with it. So a >8,388,608-face raw STL skipped the cheap triage and performed the expensive topology build anyway, and this row's claim that the bound protected the stage was false as written. **What is landed now:** admission is proven once, ahead of every action, and the capability it returns is what the strip consumes — so the shape is unrepresentable rather than merely avoided. On refusal *nothing* runs: no strip, no topology, no duplicate index, no pinch fan. **The trap that was avoided, stated so it is not re-introduced:** `prepare_one` runs load → repair → decimate → hollow, so `H0AdmissionLimits::production()` has only ever been applied to the **decimated** (~50k face) mesh on this chain, never to the raw file, and `plinth-decimate` has no comparable raw face-count admission. A >8M-face raw STL therefore used to decimate and prepare successfully. Had this bound been a `RepairError`, the mandatory, unconditional repair pre-pass would have created a **new hard-refusal class on the production `prepare` path** — a repair pass refusing a model it was added to help — which is the exact inversion of the 2026-08-10 ruling. It does not, and the follow-up review closed the wider version of the same hole: `repair_mesh` no longer returns a `Result` at all and `PipelineError` no longer has a `Repair` variant, so admission failure **and every other stage-internal failure** — the topology build, the triage's arithmetic and overflow paths, the constants registry — are recorded in `RepairEvidence::actions_skipped` with the caller's mesh handed back bit for bit. (Reachability, for the record: `load_binary_stl` enforces finite coordinates, so the ray-arithmetic and non-finite paths are unreachable for loadable inputs and the count-overflow paths need >1.4 B faces — no corpus input trips any of them today. The class was closed anyway, because it is the new-refusal-class inversion the ruling forbids on the production path, not because it fired.) Every stage below runs exactly as it did before the pre-pass existed, and the hollow stage's own H0 boundary still applies where it always did. **What this row claims:** an over-large raw input silently gets *less repair* than a small one — now *no* repair rather than two actions out of three. Its dust shards, if it has any, reach the gate unstripped and it may be refused there for a defect the pre-pass would have fixed at 8M faces minus one. That is a capability gap recorded rather than a refusal, and it is invisible unless someone reads the `repair skipped` line on the run card. **What it does NOT claim:** that the ceiling is wrong or should be raised. It is a resource boundary with an owner; raising it is a resource decision, never a fix for a refused model. **First action:** measure whether any real input approaches it — the largest corpus model is 459,072 faces, 5.5 % of the face ceiling — and if none does, this row stays open at low priority as a known asymmetry rather than as work. **No cap, floor, constant or registry entry is touched by opening this row, and the correction above raised nothing: it removed work from the over-large path, it did not admit more of it.** |
| FU-46 | **Duplicate groups the pre-pass cannot resolve stay refusals: a doubled (double-sided) sheet halves to a one-sided sheet with an open boundary, and a model made only of removable faces is handed back untouched. OWNER RULING REQUESTED on whether wider coincident-flap deletion is in scope** | **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §6) — **the scope question is answered: DELETION IS OUT OF SCOPE for automatic repair.** A model carrying zero-volume doubled geometry — a flap, a doubled sheet, a cape, a shell modelled inside-out and doubled — **stays refused until a real fix exists**, and **thickening is named as the natural future candidate** (give the zero-volume sheet a printable thickness rather than silently delete the owner's geometry). **Keep-none stays forbidden in every form**; the conservative keep-first rule stands exactly as landed. **Recorded as fact, because it sizes the class:** **no real corpus model exhibits the doubled-sheet case** — the reproductions that motivated this row were **synthetic** (a box plus an 8 × 2 mm double-sided sheet; a box plus a coincident flap) — and the **raw corpus's 3 coincident-face groups are single-pair slips already handled by the conservative keep-first rule** (in production the shard strip removes them before the duplicate action sees them). The row stays open only as the recorded refusal class it names; the deletion question is closed. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-11 by the repair pre-pass review, which found the alternative empirically.** **What was measured.** The duplicate action's keep-none disposition was originally admitted by the same local-edge-cleanliness test as the keep-one candidates. On a doubled sheet that test **self-propagates**: every group's edge-neighbours are themselves duplicate groups, so each shared edge reads one-up/one-down from *outside* the group and each rim edge reads zero, making keep-none "locally clean" for every group independently. Reproduced against the worktree: a box plus a free-floating **8 × 2 mm double-sided sheet (16 faces, 8 groups)** lost **the entire sheet**, the residual came out clean, and a model the gate **refused** became one it **passed** with the owner's geometry silently gone. The shard strip does not catch it — the sheet is 8 mm across against a 1.0 mm dust ceiling, so `shard-faces=0` — and the scale is unbounded: a 100 mm doubled cape deleted identically. **What was landed first, and why it was reverted.** The first attempt at a bound kept keep-none but narrowed it to a group **no face outside it shares any edge with**, which is unsatisfiable for any group with an edge-neighbour and so cannot cascade across a sheet. The follow-up review rejected that too, and correctly: it is still a rule the scope never authorized, it still deletes a whole group with **no survivor**, and narrowing the *shape* it applies to gave it no **size** bound at all — a 60 mm free-floating doubled triangle is edge-isolated exactly as a 0.2 mm one is, and goes the same way. Reproduced on a box+flap model: `validate_closed_mesh` went from `Err` to `Ok` with the owner's geometry gone. A pre-pass able to turn a gate refusal into a gate pass by deleting the geometry being refused is not upstream of the gate; it is a hole in it. **What is landed now:** the one rule the scope authorized, and nothing else — drop faces sharing an identical vertex triple, keep the first by deterministic order. There is no disposition that keeps no survivor, and there is none that promotes a **later, opposite-wound** member either (that second draft was an orientation repair for a defect class the census counted zero of: 0 same-direction edges across all 21 models). `DuplicateDisposition` no longer chooses which face lives; it records whether keeping the first left the group's three edges walked once in each direction (`KeepOne`) or not (`KeptFirstUnresolved`), which is evidence about the result and the count this row's class is measured by. Separately, the empty-mesh guard — a plan that would leave the mesh with **no triangles at all** is rolled back whole, the input is returned bit-identical, and `EmptyResultRefusal` says so on the card — is now **unreachable by construction** and is kept anyway: the strip never strips the main component, the duplicate rule always keeps a survivor, and the split only appends, so "no action can empty a model" is an argument about today's three actions, and the fourth this row asks about is exactly the one that would break it. Regressions: `plinth-mesh/tests/repair.rs`'s `a_doubled_sheet_is_halved_rather_than_deleted` and `a_model_that_is_only_a_coincident_flap_keeps_a_face_and_is_left_to_the_gate`, plus `repair.rs`'s `a_free_floating_coincident_group_keeps_the_first_and_goes_to_the_gate` and `a_later_member_is_not_promoted_for_having_the_better_winding`. **What the corpus says**, re-measured 2026-08-11 by `repair_probe` over all 37 STLs after this revert: on **raw** input — the only place the pre-pass runs in production — the conservative rule changes **nothing**: no duplicate group reaches action 2 at all (the shard strip removes the census's three groups first), and all 14 raw-refused models are still rescued. On the probe's **decimated** column, which is diagnostic and not production (FU-44), 14 models carry 1–14 decimator-manufactured groups each; every one keeps a survivor and is refused. Nowhere in the corpus, at either stage, is a group deleted outright — because no rule that could do so exists any more. **What this row claims:** three input shapes reach the unchanged gate and are refused — a doubled sheet (halved, so genuinely open), a model that is nothing but coincident faces, and the decimated-column class above. Under the standing doctrine every remaining refusal is an automation work item, so they are recorded here. **The question that is filed, not decided:** the spec authorized "drop faces sharing an identical vertex triple, keeping the first by deterministic order", and both wider rules — deleting a coincident group with no survivor, and preferring a later member for its winding — are fourth actions. Whether the pipeline *should* delete zero-volume double-sided geometry at all — a flap, a sheet, a cape, a shell modelled inside-out and doubled — is an owner call about what may be silently removed from a customer's model rather than a technical one, and if it is ruled in it needs a **size bound** and report visibility, neither of which the reverted drafts had. **What it must NOT become:** a reason to reinstate keep-none in any form ahead of the ruling. **First action:** an owner ruling on the scope question; until then the conservative rule stands and the gate refuses. **No cap, floor, constant or registry entry is touched by opening this row.** |
| FU-47 | **The closed-solid gate folds `TOPOLOGY_CUT_POINT_ONLY` into a refusal whose message claims only "closed, consistently oriented solid" — which the census proves these meshes already are. OWNER RULING REQUESTED; this row decides nothing** | **ANNOTATED 2026-08-12 by the reconciliation pass — THE DOCTRINE TEXT AND THE SHIPPED COMPARISON STATE DIFFERENT NUMBERS, AND AN OWNER DECISION ON IT IS OPEN** ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §4.2, §6). The doctrine sentence — and the batch record's question 2, and orchestrator ruling B.3 #8 — all say the throat rule is **radius >= `HOLLOW_DRAIN_R_MM` = 1.5 mm, i.e. the passage admits the 3.0 mm bore**. **The shipped comparison is 1.75 mm.** `core_clearance_threshold_mm(throat_r_mm, pitch_mm) = throat_r_mm + 0.5 * pitch_mm` (`crates/plinth-hollow/src/flow.rs:147-149`), applied as `clearance >= core_threshold_mm`, is **1.75 mm of clearance radius — a 3.5 mm passage** at the registered `HOLLOW_DRAIN_R_MM` 1.5 mm and `HOLLOW_PITCH_MM` 0.5 mm. **The half-voxel correction is right and is not in question**: it is an adversarial-review **tightening**, it moves no registered constant, and this row's own note below documents it. **What is in question is the record**: ruling B.3 #8 was made against a description of the code that was already out of date, so three numbers now describe one doctrine — the 3.0 mm nominal bore the doctrine cites, the **2.974335 mm** the inscribed 24-gon cutter realizes (owner ruling, [`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §2), and the 3.5 mm passage the engine requires. **Escalated to Craig as ONE decision and OPEN.** **Nothing may be loosened while it is open**, and no lane is blocked by it. **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §4) — **doctrine established; the row stays open as the work of carrying it out.** **LANDED 2026-08-12, all three phases, MERGED TO `develop` in `b9c8a6b`** (implemented on `feature/connectivity-doctrine`: `9cb57df`, `e93a4e1`, review fixes `9e4317f`/`c90a996`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **The merge resolved by measuring, not text-merging:** two `plinth-hollow` integration tests and FU-54's dedicated pass-limit integration test lost their *premise* — flow-based selection now excludes the far lobe and the non-well-composed pinch before `well_composed_cavity_component` ever runs — so they were replaced with doctrine-correct successors and the treant fixture was fully re-measured (its cavity splits into **13** flow interiors, not 9, every derived number re-pinned `from → to`). The FU-54 bound is still proved directly by `well_composed.rs`'s own unit test. **Five questions survive the landing and are carried in the batch record:** the throat rule as radius ≥ 1.5 mm vs the brief's "3.0 mm"; `clearance >= throat` vs the pre-existing strict `>`; the drilling-side floods still 6-connected; multi-cavity drainage unbuilt (largest flow component cut, the rest left solid — never a trap, savings left on the table); and `REPAIR_PINCH_SPLIT_MAX_MM` still registered and now read by nothing. **The branch record as first written follows.** Phase 1: `TOPOLOGY_CUT_CLOSED_SOLID_MASK` (`plinth-mesh/src/topology.rs`) replaces the bare "any nonzero cut flag" in `validate_closed_mesh`, written out class by class rather than as `KNOWN & !POINT_ONLY`. Phase 2: the pinch-split repair action, its evidence types (`PinchSplitEvidence`/`PinchSplit`/`PinchFanMove`/`UnrepairedPinch`) and its fan-separation machinery are deleted from `plinth-mesh::repair`; the pass is now shard-strip and duplicate-removal only. `REPAIR_PINCH_SPLIT_MAX_MM` stays registered, unread — see openQuestions. **Deviation from the planner's design, found by running the fixtures rather than assumed:** `make_dust_shard_box()`'s only defect was the point-touch, so under the new mask the raw fixture *already* passes the gate; the shard-strip test was rewritten to prove the strip still runs (its own triage criteria, independent of the gate) rather than asserting a refusal that no longer fires. Phase 3: new `plinth-hollow/src/flow.rs` — `registered_throat_radius_mm()` reads `HOLLOW_DRAIN_R_MM` fresh (never restated), `flow_labels`/`select_largest_flow_component` implement the core-plus-wave-synchronous-flood algorithm the design specified, `O(cells)` and batch-first. Wired at both `label3d`+`largest_label` selection sites (the production selection in `create_shell_recording_repair` and the well-composedness repair's own re-selection loop in `well_composed.rs`, which now threads `pitch_mm`/`throat_r_mm` down from the caller rather than reading the registry per pass) and at the two `#[cfg(test)]` production mirrors (`characterize`, `unrepaired_cavity_mask`). New `HollowError::IneligibleCavityBelowDrainThroat { throat_radius_mm }` and `SolidDecisionReason::CavityBelowDrainThroat` land a cavity that nowhere admits the bore as the SOLID reserve, never a hard refusal — wired through `plinth_hollow::run`'s own match (not just `plinth-pipeline`'s `hollow_error_refuses_this_geometry`, which the design named but which is not actually where production decides; `run`'s inner match is, and needed its own new arm or the new variant would have propagated as a hard failure instead of a solid). **Corpus effect, measured:** `Sphere`/`Cylinder`/the committed corpus fixtures were measured for cavity-voxel movement before landing per the mandated discipline; see the commit for the from→to figures. The synthetic pinched-dumbbell fixture (`corpus_cavity_repair`), purpose-built to be barely-viable after the well-composedness repair, is exactly the shape the doctrine targets: its 40 mm rod-derived waist never admits the 1.5 mm throat, so its two lobes are now separate resin-flow interiors and the larger one alone falls under the registered savings threshold — re-pinned from *hollows* to *stays solid*, with the interaction proven end-to-end rather than assumed. **Re-pinned again 2026-08-12 by the adversarial review of this branch** (below): once the flood stopped annexing the waist, the selected lobe stopped carrying its share of the diagonal thread, the selection became well-composed on its own, and the fixture now returns plain `IneligibleSavings` without ever reaching the repair — the census the test asserts on the marched selection went `(0, 8, 26)` → `(0, 0, 0)`, which is the proof that the excluded thread was exactly the non-manifoldness. A committed Weeping Treant re-measurement is in the commit history for this branch. **Not done:** `H0ResourceDiagnostics` was not extended with the new EDT/label/flood transient-byte fields the design asked for (see openQuestions) — the struct's committed-golden blast radius and its collision with FU-54/FU-55's concurrent edits to the same selection code made that the conservative call under the time available, not an oversight. **Still not done after the review below, and now with a named consequence:** `FlowStats { flow_components, core_voxels, unreached_voxels }` is computed at both production call sites (`create_shell_recording_repair`, `well_composed`'s re-selection) and discarded at both, so a model whose cavity shrank because whole sub-throat regions were left unselected leaves **no runtime trace** beyond a smaller `cavity_volume_mm3`. The evidence exists and is thrown away; wiring it is owed to whichever of FU-54/FU-55 lands last on `H0ResourceDiagnostics`. **Four fixes from the adversarial review of this branch landed 2026-08-12** on the same branch: (1) the flood is now **downhill-only in clearance**, with a stranding fixpoint and a re-flood behind it, so a coreless pocket behind a sub-throat neck is no longer annexed into a cored interior, selected, cut, and falsely certified drainable by `drilling`'s union — the defect that made the ruling's own separation rule not hold for a region with no core of its own; (2) the throat comparison now carries the **half-voxel surface offset** (`core_clearance_threshold_mm` = `throat_r_mm + pitch/2`), because the EDT is centre-to-centre while `surface_nets` puts the realized wall half a voxel inside the background centre — at the 0.5 mm pitch the raw comparison admitted a 2.5 mm realized aperture for the registered 3.0 mm bore; a tightening, no registered constant moved; (3) `certify_closed_components` (`plinth-mesh`) and `validate_perception_input` (`plinth-support`) now refuse on `TOPOLOGY_CUT_CLOSED_SOLID_MASK` rather than any nonzero cut flag — without this the ruling's own outcome was undelivered, since a point-touch model passed the relaxed gate and was refused one line later as `Topology(CertificateNotClosedOrOriented)`, and again at A5; (4) the frozen-oracle mirror's `label_count` is `FlowStats::flow_components` again rather than a has-cavity boolean, and the flow tests now pin the exact `>=` boundary, the five-sample neck, and the coreless-pocket case. **The product consequence of (2), measured on the committed Weeping Treant and NOT hidden: the treant no longer hollows.** Its `edt > wall` field holds 64,718 cavity voxels (plain 6-connectivity: 64,466 as one cavity). Against the *uncorrected* threshold it reads as one dominant interior — 10 flow components, largest zone 63,219 voxels, 97.7 % — and it hollowed. Against the corrected threshold it is **9 interiors and the largest takes 40,982 (63 %)**: the trunk and its limbs are joined by passages whose realized aperture is between 2.5 and 3.0 mm, wide enough for the raw centre-to-centre EDT to call them one cavity and too narrow for the 3.0 mm bore the pipeline drills. FU-47 says those are separate cavities and **each needs its own drain**; H0 retains exactly one (`select_largest_flow_component`), so the retained interior falls under the registered savings fraction and the model takes the SOLID reserve (`IneligibleSavingsAfterCavityRepair` → `BelowThresholdAfterRepair`) — **it still prints, it is not refused**, and no cap, gate or constant moved to get there. The release-gated corpus test was re-pinned from *hollows* to *nine interiors, solid reserve* with all of those figures as equalities. **What would give the model its cavity back is the ruling's own other half — retain and drain every qualifying interior rather than only the largest — which is an H0+H1b feature (per-cavity drain planning and per-cavity drainability proof), is not implemented, and is filed as an open question for the owner rather than guessed at here.** **A point-touch is NOT a defect.** Two interior volumes are **"connected" only through a passage wide enough for resin to flow**, and the threshold is **the drain bore diameter the pipeline already drills** — the implementation pulls the **registered drill-diameter constant** (`HOLLOW_DRAIN_R_MM` = 1.5 mm radius, a 3.0 mm bore) as the **minimum-throat rule in cavity connectivity**, rather than restating a number. **A zero-width point never connects two interiors:** each side is its own cavity, and each needs its own drain. **The owner's rationale, paraphrased faithfully:** a mini pressing two fingertips together is legitimate sculpting, not a modelling defect; the interiors on either side do not connect because **resin cannot flow through a point**. **What follows mechanically:** `TOPOLOGY_CUT_POINT_ONLY` **comes out of the closed-solid gate's `any`**; the **pinch-split repair action becomes unnecessary** (it exists to pass a check that is going away); and the **false "not closed" refusal message dies with the check** that produced it — the census had already proved these 21 models closed and consistently oriented. **This removes work rather than adding a waiver**, which is the outcome this row predicted for this ruling, and it is **not** a gate loosening in the D-22 sense: nothing about closure or orientation is relaxed and no threshold moves — a check whose premise was wrong is retired and a physical rule about resin flow, backed by an already-registered constant, takes its place. **Not ruled here:** [FU-48](#follow-ups-2026-08-04), whose refusal premise changes once the flag leaves this gate but whose "which stage writes the pinch" measurement is untouched; that row is left standing. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-10 by the automatic-repair pre-pass landing** (D-22 revision, `docs/engineering-journal/decisions/2026-08-10-d22-revision-automatic-repair.md`). **Number note: this row was opened as "FU-43" on `feature/repair-prepass`, and "FU-43" was already taken** by the candidate-free-ratio row above (opened 2026-08-10 by the zero-volume-pair landing on `feature/decimate-dupfaces`, landed on `develop` first); it is filed at the next free number instead and **no existing row was renumbered, edited or displaced** — the same collision protocol FU-34 and FU-37 record. **The mismatch, stated exactly.** `plinth_hollow::validate_closed_mesh` (`crates/plinth-hollow/src/lib.rs`) refuses on `topology.cut_flags().iter().any(\|flags\| *flags != 0)` and raises `HollowError::NotClosedOrOriented`, whose `Display` is about closure and orientation. `TOPOLOGY_CUT_POINT_ONLY` is one of the six flags in that `any`, and it is **neither**: it says two locally-disconnected face fans meet at one vertex. **The census measurement that makes this concrete** (verified harness reproducing this gate 21/21 with `build_face_topology(WeldPolicy::ExactBits)`, the exact call the gate makes): all 21 refused/suspect models are **watertight — 0 boundary edges**, **consistently oriented — 0 same-direction edges**, **0 non-manifold edges**, and each has **one positive-signed-volume shell**; an epsilon weld at 1e-4 mm changes none of it. The whole corpus pinch surface is **113 vertices**, at most 25 in one model. So every one of these models satisfies, on the measurement, the property the refusal message names. **The question, which belongs to the hollowing owner and not to this row:** do pinch vertices actually harm voxelization / marching cubes / the Boolean? A self-contact point is a real geometric fact and may well be a legitimate refusal for the voxel path — the point is that **nobody has measured whether it is**, and the message currently asserts a different reason than the one that fired. **What this row does NOT claim:** that the gate is wrong, that the flag should be dropped from the `any`, or that the message should be reworded to make the refusal easier to accept. **What it must NOT become:** a reason to loosen the gate. The 2026-08-10 ruling is explicit that automation lands upstream of gates and never by relaxing them, and the repair pre-pass already splits these pinches upstream — so a ruling that pinches are harmless would **remove work**, not add a waiver. **First action:** an owner ruling on whether `TOPOLOGY_CUT_POINT_ONLY` is a voxelization hazard. If it is, the refusal stays and its message is corrected to name self-contact as its own reason. If it is not, the flag comes out of this one gate's `any` with a recorded rationale, and the pinch-split action becomes optional rather than required. **No cap, floor, constant or registry entry is touched by opening this row.** |
| FU-48 | **The manufactured point-only pinch is refused but not attributed: the collapse and the pair removal that follows it are both candidates, and neither is exonerated** | **Opened 2026-08-11 by the second review pass on `feature/decimate-dupfaces`.** **Number note: this row was opened as "FU-44" on `feature/decimate-dupfaces`, and "FU-44" was already taken** by the duplicate-face-manufacture row above (opened 2026-08-10 by the automatic-repair pre-pass landing on `feature/repair-prepass`, landed on `develop` first, together with FU-45, FU-46 and FU-47); it is filed at the next free number instead and **no existing row was renumbered, edited or displaced** — the same collision protocol FU-34, FU-37 and FU-47 record. The review that opened it strengthened the emitted-candidate gate to refuse any increase in any canonical count and deliberately did **not** change what the reduction produces. **What is known.** Production decimation manufactures `TOPOLOGY_CUT_POINT_ONLY` faces from sources clean in all six cut classes: `Tavey Base` 10 at r = 0.40625 and 9 at r = 0.3765625, `Rotting Warhorse` 9 at r = 0.9109375, `Weeping Treant v1.2` 298 at r = 0.940625 (alongside its 16 non-manifold edges). All are refused now, and every affected model still verifies at another ratio, so this is a *reduction-quality* follow-up and not a correctness hole. **What is not known: which stage writes the pinch.** Two mechanisms can produce one and the evidence does not separate them. (a) meshoptimizer's collapse is not link-condition checked — the same root cause as the duplicate pair — so it can fuse a neighbourhood into a single-vertex contact directly. (b) `remove_zero_volume_face_pairs` checks that removing a pair does not open a **boundary** (edge incidence parity) and does **not** check that the vertex fans it touches stay connected, so deleting the pair that was the only link across a fan pinches that vertex by construction. The Treant candidate removed 67 pairs and carries 298 pinch faces, which is consistent with either. **How to settle it.** Count `TOPOLOGY_CUT_POINT_ONLY` on meshoptimizer's own compacted result versus the repaired one at a refused ratio; the difference is (b) and the remainder is (a). If (b) contributes, the fix is a fan-connectivity clause in the removal, refusing the *pair* rather than the candidate — strictly narrower than the gate refusal it replaces, never wider. Do not weaken the gate for it. |
| FU-49 | ~~**The hollow stage manufactures a non-manifold cavity surface on branched models — `Warhorse` and the Weeping Treant refuse their own shells with pristine input and a perfectly placed cavity**~~ | **RESOLVED 2026-08-11** by the well-composedness repair ([`#hollow-well-composed-2026-08-11`](#hollow-well-composed-2026-08-11)); the finding as first written is left standing below and the resolution is appended to it, not substituted for it. **The prescribed first action was carried out and it decided the remedy:** the critical cells were counted — **44** on the raw `Warhorse.stl`, **70** on the committed treant fixture. Tens, not thousands, so remedy (a) was the sanctioned path and remedy (b) (a new extractor) was not needed. `crates/plinth-hollow/src/well_composed.rs` repairs the selected cavity mask to well-composedness by **removing cavity voxels only**, iterated to a fixpoint. Removal-only is the safety argument: it turns void back into material, so the wall can only get thicker and the repair can never breach the registered wall — the wall constant is neither read nor written by the module. Termination needs no budget (removal is monotone on a finite set). **Work is two counted domains, not one:** cell visits are bounded by the already-admitted grid at `cells + 8 × changed`, and the whole-grid `label3d` re-selections are counted separately as `cavity_repair_full_grid_passes` (measured: **2** on every model available, bounded only by `2 + dropped`) because the cell-visit bound does not cover them. **Corrected 2026-08-11 on review:** each counted pass is *two* whole-grid traversals, so `p` passes are `2p − 1` traversals and the total is `cells × (2p − 1) + 8 × changed`; and that domain has no admitted bound of its own, which is filed as **FU-54**. The transient workset is the changed voxels themselves as four-byte linear indices plus a reused one-byte-per-voxel flag array — the incident cells are enumerated lazily and never materialised, which is what keeps the working set at the same order as the label array `label3d` already builds. The removal rule is a pure function of the cell's 8-bit configuration, so it is order-free and deterministic. **Measured cost: 0.055–0.19 % of the cavity** (51/27,033 voxels on the raw `Warhorse`, **115/64,351 on the committed treant fixture**, 63/114,990 on the raw `Giant Toad`), and the inner surface goes 92 non-manifold + 276 point-only → **0 + 0** on the `Warhorse`, 132 + 483 → **0 + 0** on the treant. All three models hollow for the first time (3,379.1 / **8,043.9** / 14,373.8 mm³). **Every measurement here names the mesh and the pose it was taken in** — the table at [`#hollow-well-composed-2026-08-11`](#hollow-well-composed-2026-08-11) carries the full provenance, and the committed row is asserted by a release-gated test rather than recalled. **A first version of this row recorded the treant's numbers from the raw, uncommitted `Awakened Tree.stl` under the committed fixture's name; those were 59/66/63,958 and 3,561,128 cell visits, which is below one full scan of the committed fixture's grid and could not have been it.** The two files are the same 429,120-face mesh in **different poses**, which is exactly why the numbers differ. **Corpus-wide across all 37 minis and the 8 committed corpus models, zero `InvalidShellTopology` refusals remain**; `Sphere`, `Cylinder` and every simple primitive report 0 critical cells and are unaffected. **A repair-caused solid decision is distinguishable in the record**: `CavityRepairRemovedCavity` / `IneligibleSavingsAfterCavityRepair` and the matching `NoCavityAfterRepair` / `BelowThresholdAfterRepair` reasons carry the repair's measurement rather than presenting it as a property of the geometry, and `HollowReport::cavity_repair` puts the three diagnostics on the report for the first time. **No cap, floor, constant or registry entry was touched.** **What the resolution does NOT claim:** that the resulting shells are printable. They are not — the wall they actually ship is 11–78 % of the registered 2.5 mm, which is the separate and pre-existing defect filed as **FU-53**, and this fix newly exposes three models to it. Closing FU-49 closes the *topology* refusal only. **Original finding follows.** **Opened 2026-08-10 by the cavity-field exterior work** ([`#hollow-cavity-field-exterior-2026-08-10`](#hollow-cavity-field-exterior-2026-08-10)), which is FU-34's recorded first action carried out. **This is the residue of FU-34, not a duplicate of it:** FU-34 filed one refusal string over three corpus models; two of those three (`Cylinder`, `make_box`) were the boundary-condition defect and are fixed, and this row is what is left. **The measurement**, release, `create_shell` internals: on `Warhorse.stl` (raw, 197,936 faces) the cavity is 3,385.5 mm³ and lies **wholly interior** — its EDT is byte-identical before and after the exterior margin, so it is provably not in defect 1's class — and the marching cavity surface is watertight (**0** boundary edges, **0** same-direction edges) and **non-manifold**: **92** faces carry `TOPOLOGY_CUT_NONMANIFOLD` and **276** carry `TOPOLOGY_CUT_POINT_ONLY` out of 20,272. The Weeping Treant: **132** and **483** out of 55,084. The `Sphere`, `Cylinder` and a 30 mm cube carry **zero of either**, which is what makes the attribution clean rather than assumed. **The mechanism, stated as a mechanism and not a guess:** `fast_surface_nets` emits at most **one vertex per lattice cell**. Where the cavity mask is not *well-composed* — a 2×2 face of a cell holding the checkerboard pattern, or a 2×2×2 cell whose foreground is an antipodal pair — two sheets of the surface must share that cell's single vertex, and the edges around it are used by four faces instead of two. `ManifoldBackend::difference_limited` inherits it, `certify_closed_components` refuses the result, and both `orient_boolean_cavity` (`crates/plinth-hollow/src/lib.rs`) and `validate_shell_topology` demand `TriageError::PreHollowed` with **exactly one** cavity component and ≥ 2 certified closed components with **exactly one** negative — they get neither, and return `HollowError::InvalidShellTopology`. **This is the same defect class the corpus census counts on inputs** (113 `TOPOLOGY_CUT_POINT_ONLY` pinch vertices across 21 models) — except here a pristine input acquires it inside the hollow stage: `Warhorse.stl` is clean at raw and after decimation. **What this row does NOT claim:** that the shell check is wrong. It is right — that shell is genuinely not one negative void in positive material, and printing it would be a defect, not a saving. **Why it was not fixed beside the boundary-condition fix:** the two candidate remedies are both new algorithms. (a) Make the mask well-composed by removing the foreground voxels that participate in a critical configuration, iterating to a fixpoint — deterministic and monotone, but it needs its own work bound, a convergence argument, and a re-label pass, and the 6-foreground/2-antipodal-background case is not repaired in one round. (b) Replace the extractor with one that guarantees manifold output (Marching Cubes 33, or a multi-vertex-per-cell Surface Nets), which is a dependency or a new extractor and moves every hollowed model's geometry. Neither belongs improvised beside a one-function boundary-condition fix. **First action:** count the critical cells on the `Warhorse` and the treant — if it is tens, remedy (a) is bounded and cheap (0.07 % of a 27,084-voxel cavity) and should be measured to a fixpoint before anything else is considered; if it is thousands, remedy (b) is the honest answer. **No cap, floor, constant or registry entry is touched by opening this row**, and both models still reach an artifact through the `HollowStageRefused` fallback, with the refusal recorded verbatim. **Product consequence:** hollowing is where the resin saving lives, and the project's two largest models still get none. **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-43" on `feature/hollow-warhorse`, and "FU-43" was already taken on `develop` by the ratio search's candidate-level failure ending the whole run (opened by `feature/decimate-dupfaces`) — the two branches numbered independently off the same merge base (`dba1bc9`, which ended at FU-42). Per the FU-34/FU-36/FU-37 precedent the incoming row is refiled at the next free number and **no existing row was renumbered, edited or displaced**. Every reference to "FU-43" in this branch's code, tests, module docs and journal sections was rewritten to "FU-49" in the merge commit; the `develop` rows keep FU-43 through FU-48 exactly as they were merged. |
| FU-50 | **Raster subdivision work is quadratic in a triangle's longest edge and blind to its area, so plain flat display bases blow a work cap that is already ~240× larger than the honest need** | **MERGED TO `develop` in `b71ddbc`** (commits `6b4ed20` + tail removal `1ad34e6`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **The ruling's acceptance criterion is now closed end to end, by measurement rather than by inference:** the implementer recorded as an open question that `Round 50` and `Round 100` had been proved to pass *rasterization* but never run through the full pipeline. In the 2026-08-12 corpus rerun ([the dated section](#ruled-batch-landed-2026-08-12)) **both emit artifacts**. **One finding survives and is recorded as measured rather than engineered to fit:** the ruling asserted "roughly three orders of magnitude of headroom" under an area-proportional rasterizer; the landed rasterizer measures the two flat bases at **~2.5 % of the unchanged cap**, i.e. **~40×**, not ~1000×. The counting rule was **not** adjusted to reach the ruling's figure — that would be the redefinition of "work item" the ruling forbids. Also carried in the batch record: `raster_delta.rs`'s "approx min wall" column is a locally-defined proxy, not FU-55's landed measurement primitive, and is labelled as such. **LANDED 2026-08-12** ([`#hollow-raster-rewrite-2026-08-12`](#hollow-raster-rewrite-2026-08-12)) — the area-proportional conservative rasterizer replaced the frozen midpoint-quartering one, the superset property (`old marked ⊆ new marked`) is proven mechanically and holds with `|old∖new| = 0` on all ten measured models, `Round 50.stl` and `Round 100.stl` now rasterize at ~2.5% of the unchanged cap (down from 1.15×/1.34× that refused them), and every affected frozen number is re-pinned with `from -> to` in the journal section. `max_raster_work_items` was not touched. **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §3) — **the replacement is authorized and the row stays open as the work of building it.** **The ruling: replace the frozen midpoint-quartering rasterizer with an area-proportional conservative rasterizer.** The **new marked-voxel set must be proven on the committed corpus — the delta measured and recorded — BEFORE it ships**, because that set is the EDT's input and therefore moves every hollowed model's cavity by an amount that is measurable on eight committed models rather than inferable from the argument. **The work cap does not move and "work item" is not redefined:** both prohibitions this row wrote against itself stand, and under an area-proportional rasterizer the existing cap keeps ~3 orders of magnitude of headroom. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-10 by the cavity-field exterior work** ([`#hollow-cavity-field-exterior-2026-08-10`](#hollow-cavity-field-exterior-2026-08-10)). `Round 50.stl` and `Round 100.stl` refuse with `frozen voxel subdivision exceeded the caller work cap 16777216`. **The cap is not the defect.** `walk_raster_leaves` reproduces trimesh's `subdivide` voxelizer: quarter each triangle at its edge midpoints until every edge is ≤ pitch/2. Midpoint quartering is **shape-preserving**, so every child has its parent's aspect ratio, the recursion runs to a uniform depth `d = ⌈log₂(L_max/(pitch/2))⌉`, and a triangle's leaf count is exactly `4^d` — **growing as the square of the longest edge, in power-of-four steps, and completely independent of the triangle's area**. (This row first stated the leaf count as `(L_max/(pitch/2))²`, which is the same growth law but the wrong number: its own worst-triangle example, 97.96 mm at pitch 0.5 mm, gives `d = 9` and `4⁹ = 262,144` leaves, not `391.84² = 153,538`.) A fan-triangulated round base is built from slivers that span the whole disc, which is the worst possible input for that rule. **The measured work profile**, all 37 minis, release, `raster_work_item_count` called with a diagnostic ceiling (**the production cap was not raised, reinterpreted or bypassed — the ceiling exists only so the true demand could be counted**), against the area-proportional demand the same rasterization actually needs (`Σ area / pitch²`, pitch 0.5 mm): `Round 100` **22,486,336** items = **1.340× cap** against 70,493 area cells (**319× waste**); `Round 50` **19,291,472** = **1.150× cap** against 19,698 cells (**979× waste**); then the whole rest of the corpus is under the cap — `Rotting Warhorse` 8,724,485 (0.520×, 212× waste), `Awakened Tree` 4,454,778 (0.266×, 66×), `Giant Bat` 4,274,299 (0.255×, 81×), `Round 25` 3,489,488 (0.208×, **619×**), median 0.026× of cap, minimum waste 45× (`Tavey Badger`, `Myconid Adult`). **The cap-vs-need curve:** the honest demand of the *worst* model in the corpus is 70,493 cells — **0.42 % of the cap**, i.e. the cap already carries ~240× headroom over a correct rasterizer. `Round 100`'s single worst triangle is 97.96 mm long and 144.70 mm² in area and costs **262,144 leaves for about 579 voxel cells**, a 452× overshoot on one triangle; its depth histogram puts 58 triangles at depth 9 (15.2 M items) and 36 at depth 8 (2.4 M) out of 1,966 triangles total. **DO NOT RAISE THE CAP.** Admitting `Round 100` needs ≥ 22,486,336, which is **67,108,864** under the repo's own `next_power_of_two(2 × worst case)` rule (33,554,432 only if the doubling is dropped; this row first stated the smaller figure while citing the doubling rule) — either of which would enshrine 300–1,000× waste as a budget (FU-8b's standing lesson) and buy nothing durable, because demand grows as `L_max²`: the next larger base blows it again. **What this row claims:** the remedy is a rasterizer whose work is proportional to the surface area it covers rather than to the square of the longest edge, and under such a rasterizer the existing cap is untouched with three orders of magnitude of headroom. **What it does NOT claim:** that a cheap in-place fix exists. It does not. The frozen marked-voxel set is *defined* by the leaf vertices of this exact recursion, so any cheaper enumeration must be proved to mark the identical voxels — and the counted "work item" is the leaf, so a change that reduces marking work without reducing leaf count does not move the cap comparison at all, and redefining what a work item *is* would be reinterpreting the cap, which is forbidden. **First action:** an owner ruling on whether the frozen trimesh rasterization semantics may be replaced by an area-proportional conservative rasterizer (which moves the marked voxel set, and therefore every hollowed model's cavity, by a small amount that must be measured on the committed corpus first). Until that ruling, large flat bases are refused rather than admitted. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-44" on `feature/hollow-warhorse`, and "FU-44" was already taken on `develop` by the decimator manufacturing duplicate faces (opened by the automatic-repair pre-pass landing) — the two branches numbered independently off the same merge base (`dba1bc9`, which ended at FU-42). Per the FU-34/FU-36/FU-37 precedent the incoming row is refiled at the next free number and **no existing row was renumbered, edited or displaced**. Every reference to "FU-44" in this branch's code, tests, module docs and journal sections was rewritten to "FU-50" in the merge commit; the `develop` rows keep FU-43 through FU-48 exactly as they were merged. |
| FU-51 | ~~**The `Sphere`'s frozen Python hollow artifact and the engine have diverged by an attributed amount — the recorded drain sits inside the wall it is drilled through**~~ | **RESOLVED 2026-08-12** on `develop` in `44b8df1`; batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **Two scoping notes carried there rather than left implicit:** the retired-Python-era fields (`solid_ml`, `final_ml`, `decision`, fingerprints, transform — no Rust reader exists for any of them) were deliberately left untouched, a recorded deviation from a literal whole-file regeneration, because reproducing them would require re-running the retired Python pipeline; and the record's `drains[0].wall_mm` / `vent.wall_mm` fields are **sampled exit distances**, not wall thicknesses (Rust's `HoleGeometry::sampled_exit_mm` carries the same quantity), so the record arguably ought to say "sampled exit" — an open naming question, not a value error. **RESOLVED 2026-08-12** — FU-61 confirmed and `corpus/baseline/Sphere.pipeline.json`'s drain/vent placement got its one deliberate, recorded regeneration; old and new values, the touched tests and what was deliberately left untouched are all recorded in [the 2026-08-12 FU-61/FU-51 section above](#fu61-confirmed-fu51-re-record-2026-08-12). **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §9) — **VERIFY FIRST, THEN RE-RECORD; the row stays open, blocked on the verification.** **The verification is filed as a new row, [FU-61](#follow-ups-2026-08-04)**: prove on the corrected engine that **every drain and vent bore fully penetrates — outer surface to cavity void, at full bore width** — against **worst-case placement on the 0.5 mm voxel-stepped cavity surface**. **The owner's observation that motivates it:** the cavity interior is voxel-stepped at `HOLLOW_PITCH_MM` = 0.5 mm while the outer wall carries near-full model resolution, an asymmetry that **may require the bore to overshoot the nominal wall — on the order of a voxel body diagonal, 0.5 × √3 ≈ 0.87 mm ("~0.9 mm")** — to guarantee a clean full-width opening; if so, **the overshoot margin is established and registered**. **Only after that verification passes — with the fix if one is needed — does `corpus/baseline/Sphere.pipeline.json` get its ONE deliberate, recorded regeneration**, never as a side effect of another change. Re-freezing now would only replace one unverified record with another. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-10 by the cavity-field exterior work** ([`#hollow-cavity-field-exterior-2026-08-10`](#hollow-cavity-field-exterior-2026-08-10)). `corpus/baseline/Sphere.pipeline.json`'s hollow report records a drain at `xyz = [0, 0, 0.5]` with `wall_mm = 1.0` on a solid whose bottom is `z = 0`, under a **stated 2.5 mm wall**, and a vent at `z = 19.5` on a solid whose top is `z = 20`. Those positions are a faithful record of the pre-2026-08-10 cavity field, which had no exterior: the `Sphere` is tangent to its own bounding box at six points, those 72 voxels read as deep interior, and the selected cavity therefore reached to within a fraction of a millimetre of the outer surface at each pole. With the exterior margin the drain sits at `z = 2.5` — **on** the stated wall — and the vent at `z = 16.0`; the vent's lateral exit axis also moves from `+x` to `−y`, because it is a tie among equal-cost directions on a body of revolution and the smaller cavity breaks that tie elsewhere. **`corpus/baseline` was deliberately NOT regenerated:** it is evidence, and `development.md` is explicit that it is not regenerated as a side effect of another change. `frozen_shell_field_has_an_enclosed_drain_and_vent_plan` therefore now pins **both** figures and the sign of the difference (`drain_z_new > drain_z_frozen`, `vent_z_new < vent_z_frozen`), so the divergence is loud and attributed rather than quietly re-pinned. **What this row claims:** exactly one corpus artifact's recorded hollow report no longer describes what the engine produces, and the reason is known and one-line. **What it does NOT claim:** that any other frozen artifact moved (the support and emit baselines are built from the oriented solid and are untouched), or that the old numbers were wrong *as a record* — they are a correct record of a defective field. **First action:** the owner decides whether `Sphere.pipeline.json`'s hollow stage is re-frozen against the native engine, or left standing as the Python-era record with this row as its footnote. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-45" on `feature/hollow-warhorse`, and "FU-45" was already taken on `develop` by the oversize raw input that gets no repair at all (opened by the repair pre-pass review) — the two branches numbered independently off the same merge base (`dba1bc9`, which ended at FU-42). Per the FU-34/FU-36/FU-37 precedent the incoming row is refiled at the next free number and **no existing row was renumbered, edited or displaced**. Every reference to "FU-45" in this branch's code, tests, module docs and journal sections was rewritten to "FU-51" in the merge commit; the `develop` rows keep FU-43 through FU-48 exactly as they were merged. |
| FU-52 | **`SupportedGeometry::DrilledShellRefusedBySupport` is now an ungated production branch — no committed fixture reaches it** | **Opened 2026-08-10 by the cavity-field exterior work** ([`#hollow-cavity-field-exterior-2026-08-10`](#hollow-cavity-field-exterior-2026-08-10)). The support-refusal substitution — the chain hollows, drains and drills, the sealed support run refuses *that* shell, and the oriented solid carries the model instead — had exactly one gate in the workspace: `prepare_folder.rs`'s per-model-baseline test, which asserted the `Sphere` took it. It no longer does. Until this fix the `Sphere`'s cavity reached to within 0.5 mm of its outer surface at the six points where it is tangent to its own bounding box; the support run refused the shell that produced, and the reserve stood in. With the wall intact the shell **seals** and the `Sphere` is refused by `PreWriteGate::UncoveredIslands` instead — a terminal gate stop, which is a different variant on a different branch. The production code path is untouched and still correct (`crates/plinth-pipeline/src/prepare.rs`, consumed by `crates/plinth-cli/src/execute.rs:278`); what is gone is the fixture that proved it. **What this row claims:** a live branch of the shared composition now has no test on it, which is the precise condition the retired gate's own comment names as what drifts. **What it does NOT claim:** that the branch is unreachable — the `Cylinder` hollows for the first time as of this change and its drilled shell's support verdict has never been measured (FU-35 blocks it in debug profiles), so a corpus fixture may well still exist. **First action:** measure the `Cylinder`'s drilled-shell support verdict in release; if it takes this branch, re-gate on it. If no corpus model does, construct a fixture whose drilled shell is deliberately unsealable rather than leaving the branch bare. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-46" on `feature/hollow-warhorse`, and "FU-46" was already taken on `develop` by the duplicate groups the pre-pass cannot resolve (opened by the repair pre-pass review) — the two branches numbered independently off the same merge base (`dba1bc9`, which ended at FU-42). Per the FU-34/FU-36/FU-37 precedent the incoming row is refiled at the next free number and **no existing row was renumbered, edited or displaced**. Every reference to "FU-46" in this branch's code, tests, module docs and journal sections was rewritten to "FU-52" in the merge commit; the `develop` rows keep FU-43 through FU-48 exactly as they were merged. |
| FU-53 | **Every hollowed shell is under-walled — the engine ships 11–78 % of the 2.5 mm wall it is told to leave, and where the error exceeds the local half-thickness the cavity breaks through and the shell has no void at all** | **MERGED TO `develop` in `b71ddbc`** (commits `c07f5a9`, `3a9b98b`, cross-constant test `385eaf9`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **The raw-mini re-measurement this row's own test item 3 asked for is still unmeasured, and the 2026-08-12 corpus rerun says why:** all 37 minis were run end to end and **not one reaches the drilling stage** — every artifact emitted is `Solid { reason: BelowThreshold }` — so the realized-wall fix has no real-geometry witness at all. Filed as [FU-62](#follow-ups-2026-08-04), not left as a note. **One question survives:** `HOLLOW_CAVITY_MARCH_MARGIN_MM` = 0.866 mm is derived and provably safe but costs real cavity volume on every model, enough to push some under the savings gate into a solid decision; a worst-case construction suggests ~0.44 mm may suffice but could not be proved airtight, and trading proof for savings is a ruling rather than an implementer's call. **LANDED 2026-08-12** ([`#hollow-exact-distance-wall-fix-2026-08-12`](#hollow-exact-distance-wall-fix-2026-08-12)) — cavity selection moved from the EDT's conservative overestimate to the exact nearest-surface distance (`geom_kernel::NativeSurface`, batched, index-parallel, no per-voxel call), prefiltered by the unchanged EDT under a mechanically proven soundness lemma (`edt(v) >= d_true(v)`), plus a new registered marching margin (`HOLLOW_CAVITY_MARCH_MARGIN_MM` = one voxel body diagonal, 0.866 mm) that makes the realized wall `>=` the requested wall **by construction**. The Sphere's realized wall moves from a measured 1.85 mm (74 %) to a proven-by-construction `>= 2.5 mm`; the synthetic breach class (13/15 configurations refusing `InvalidShellTopology`) is gone across a 20-configuration reconstructed sweep. The committed Weeping Treant v1.2 now correctly fails the registered savings floor after its honest wall shrinks its cavity (`IneligibleSavingsAfterCavityRepair`), converting to the mandated solid-reserve substitution per the FU-55 ruling rather than shipping under-walled — measured and recorded, not a regression. The raw minis (`Warhorse.stl`, `Giant Toad.stl`, `Awakened Tree.stl`) are **not re-measured**, `minis-to-process/` not being present in the worktree this landed from; that is filed as an open question in the journal section, not silently assumed. `HOLLOW_WALL_DEFAULT_MM` was not touched; one new admission bound (`max_cavity_distance_index_bytes`) was added, none loosened. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-11 by the well-composedness work** ([`#hollow-well-composed-2026-08-11`](#hollow-well-composed-2026-08-11)). Fixing the extractor let three models reach a shell, and the obvious question had never been asked of *any* hollowed model: how thick is the wall that actually came out? **The measurement**, release, brute-force minimum distance from every vertex of the marching cavity surface to the source surface (source triangles bucketed into 3 mm cells, answer capped at 3 mm — all that matters against a 2.5 mm wall): `Cylinder` **1.95 mm** (78 %), `Sphere` **1.85 mm** (74 %), `Warhorse.stl` **1.27 mm** (51 %), the committed `Weeping Treant v1.2` fixture **0.70 mm** (28 %), `Giant Toad.stl` **0.27 mm** (11 %), and the raw `Awakened Tree.stl` — the treant's source mesh in its *delivered* pose, not the committed one — **0.49 mm** (19 %). **This is pre-existing and is NOT caused by the well-composedness repair:** the `Sphere` and `Cylinder` have **0 critical cells**, are not modified by that change in any way, and already ship at 74–78 %. (**Corrected 2026-08-11 on review:** this row first said both "hollow today on `develop`". The `Sphere` does; the **`Cylinder` does not** — `develop` (`dba1bc9`) refuses it and it hollows only as of `10725ff`, the cavity-field exterior fix on the same branch as the repair. The attribution never depended on it and is unchanged: what makes the repair non-causal for these two is that both report **0 critical cells**, so no voxel of either mask is touched by it. The under-walled `Cylinder` is `10725ff`'s to own.) The repair only ever *removes* cavity, so it can only move these numbers up. What it did was newly expose three more models to a defect that was already there. **The error grows with local surface obliquity and concavity**, which is what separates a smooth primitive (loses ~0.55–0.65 mm) from a branched organic mini (loses 1.2–2.2 mm). **The same defect at its limit is a second, independent refusal cause.** A synthetic 20 mm block with a 20 mm fin leaving one face at an angle refuses with `InvalidShellTopology` in **13 of 15** thickness/direction configurations while reporting **0 critical cells and a perfectly manifold cavity surface** (0 boundary edges, 0 non-manifold, 0 point-only). The Boolean output is watertight and manifold; what it is not is *hollow* — triage returns `Ok`, a plain solid with **zero enclosed cavities**, because the cavity was cut so close to the fin's surface that it broke through and opened the void to the outside. The failure is **non-monotonic in fin thickness** (5.4, 5.6, 5.8, 6.2 mm refuse; 6.0 mm hollows), the signature of lattice aliasing rather than a clean threshold. **The mechanism, as far as it is proven:** the cavity is selected by `edt > wall`, where the EDT is a voxel-lattice Euclidean distance transform computed on the **conservatively rasterized** occupancy — a voxelization that marks every voxel the surface passes through, so the boundary the EDT measures from lies *outside* the true surface — and the marching surface then sits up to half a voxel outside the outermost cavity voxel centres. **What this row claims:** the realised wall numbers above, which are measured, not inferred; that the defect predates the well-composedness fix; and that the breach case is the same defect past its limit. **What it does NOT claim:** a precise decomposition of the 0.55–2.2 mm error into rasterization dilation versus marching offset versus lattice quantization — that apportionment is unmeasured and is the first thing the remedy needs. **Why it is not fixed here:** the honest remedy is in the EDT/rasterization semantics, and changing them moves every hollowed model's cavity, so it must be measured on the committed corpus before it is chosen — architecturally larger than a topology fix and not improvisable beside one. Candidate remedies to weigh: measure the EDT against the true surface rather than the rasterized occupancy; erode the selected cavity by the measured worst-case lattice error; or select the cavity from a signed distance field sampled from the mesh instead of from a binary voxelization. **First action:** apportion the error — measure, on the `Sphere` (where the analytic answer is known exactly), how much of the 0.65 mm loss is rasterization dilation, how much is the marching half-voxel, and how much is EDT quantization. That decides which of the three remedies is the cheap one. **Do NOT respond to this row by lowering the registered wall constant to match what the engine produces** — the constant is the physical requirement and the engine is what is wrong. **Product consequence:** hollowed shells are currently not printable, a 0.27 mm wall being roughly five layers of resin, and this row is the blocker on trusting any hollowed shell to a printer. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-47" on `feature/hollow-warhorse`, and "FU-47" was already taken on `develop` by the closed-solid gate's `TOPOLOGY_CUT_POINT_ONLY` refusal message (opened by the automatic-repair pre-pass landing) — the two branches numbered independently off the same merge base (`dba1bc9`, which ended at FU-42). Per the FU-34/FU-36/FU-37 precedent the incoming row is refiled at the next free number and **no existing row was renumbered, edited or displaced**. Every reference to "FU-47" in this branch's code, tests, module docs and journal sections was rewritten to "FU-53" in the merge commit; the `develop` rows keep FU-43 through FU-48 exactly as they were merged. |
| FU-54 | **The well-composedness repair's whole-grid re-selection domain has no admitted bound — it is the one work domain in H0 that a caller cannot cap** | **MERGED TO `develop` in `b71ddbc`** (commit `13f68c4`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **One question survives the landing:** the ruling said the limit is "registered with a proper provenance class like every other admission limit", and what landed is a private const plus a caller-owned `H0AdmissionLimits` field — the sibling pattern, matching `DEFAULT_MAX_VOXEL_CELLS`, `DEFAULT_MAX_RASTER_WORK_ITEMS`, `DEFAULT_DRAIN_PLANNING_BYTES` and the Boolean limits, **none** of which is a registered M0 constant. That satisfies "like every other admission limit" but not `docs/development.md`'s "resource caps are registered too"; the planner recommended the M0-registry route, and the sibling route is what landed. **Also recorded:** the connectivity merge (`b9c8a6b`) retired this row's dedicated full-pipeline pass-limit test because flow-based cavity selection removed its premise — the bound itself is still proved directly by `well_composed.rs`'s own unit test. **LANDED 2026-08-12** ([`#hollow-cavity-repair-pass-limit-2026-08-12`](#hollow-cavity-repair-pass-limit-2026-08-12)) — a new, independent H0 admission domain (`H0AdmissionLimits::max_cavity_repair_passes`, production default 8) bounds the loop; the check runs before each re-selection's `label3d` call, so a round that would exceed the bound is refused (`HollowError::CavityRepairPassLimitExceeded`, carrying the same repair evidence the other two repair-caused refusals carry) instead of taken. `plinth-pipeline` classifies it as a resource ceiling (no artifact, no solid substitution), per FU-8b. Adds a bound; loosens nothing. **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §7) — **BOUND IT; the row stays open as the work of landing the bound.** A **new admitted limit on whole-grid re-selection passes, value EIGHT (8)**, with a **typed refusal of the `CavityRepairPassLimitExceeded` shape this row proposed**, carried as a **caller-owned `H0AdmissionLimits` field** and **registered with a proper provenance class like every other admission limit**. **The number:** measured real models use **1–2** passes, so 8 is **4× headroom** over the worst measured case; **the owner owns the number 8** — it is not derived from anything, and the change that registers it says so in its provenance rather than manufacturing a derivation. **Why bounded rather than accepted:** the alternative on the table (accept a mask-shaped `O(N²)` tail because the mask comes from an already-admitted grid) is **rejected** — an unbounded work domain is the one thing a stage whose design principle is admitted work domains must not contain. **Eight is a ceiling that never bites real geometry, not a budget to grow into**, which is the FU-8b lesson applied in the direction that costs nothing. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-11 by the second review round of the well-composedness work** ([`#hollow-well-composed-2026-08-11`](#hollow-well-composed-2026-08-11)). Every other work domain in `create_shell` is admitted against a caller-owned limit: `max_voxel_cells`, `max_raster_work_items`, `max_edge_occurrences`, `max_containment_probe_slots`, the four Boolean limits, `drain_planning_bytes`. The repair's *cell visits* are bounded by the already-admitted grid at `cells + 8 × changed`, and that is counted. Its **whole-grid passes are not bounded by anything a caller states.** `well_composed_cavity_component` re-selects the component with a `label3d` after any round that removed a voxel, and the loop continues while the re-selection drops anything, so `full_grid_passes ≤ 2 + reselection_dropped_voxels` and `dropped` can be `O(N)`. On an admitted 16,777,216-cell mask the worst case is therefore `O(N²)` cell reads **with no typed refusal at all** — the run would simply take arbitrarily long. **The measured profile, every model available:** `full_grid_passes` = **2** on the committed `Weeping Treant v1.2` fixture, the raw `Warhorse.stl`, the raw `Giant Toad.stl`, the raw `Awakened Tree.stl`, and the synthetic pinched dumbbell; **1** (no re-selection at all) on `Sphere`, `Cylinder`, `make_box` and every mask with zero critical cells. **The cap-vs-need curve is flat:** the need is 2 passes, and 2 passes is `2p − 1 = 3` whole-grid traversals — at the admitted cell cap, ~50 M cell reads against a worst case with no ceiling. There is more than enough room between the measured need and any plausible bound that the bound would never bite on real geometry; what is missing is the *number*, and choosing it is an acceptance-bound decision. **What this row claims:** an unbounded work domain exists in a stage whose entire design principle is that work domains are admitted, and FU-50 exists precisely because a work cap was respected rather than raised — so inventing one here without a ruling would be the same error in the opposite direction. **What it does NOT claim:** that any real input reaches it. None does; the profile above is the whole of the evidence. **First action:** the owner rules on the bound — either a new caller-owned `H0AdmissionLimits` field with a typed `CavityRepairPassLimitExceeded` refusal (in which case the value, its provenance class and its registry entry are the owner's to set), or an explicit ruling that a mask-shaped `O(N²)` tail is accepted because the mask is derived from an already-admitted grid. **The work formula was also wrong and is corrected on this branch, independently of the bound:** each counted pass is *two* whole-grid traversals (the `label3d` and the `indexed_iter` sweep applying its verdict), so total work is `cells × (2p − 1) + 8 × changed`, not `cells × p + 8 × changed` — the module doc, the accessor docs and the bound test now say so. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-48" on `feature/hollow-warhorse`, and "FU-48" was already taken on `develop` by the manufactured point-only pinch that is refused but not attributed (opened by the second `feature/decimate-dupfaces` review) — the two branches numbered independently off the same merge base (`dba1bc9`, which ended at FU-42). Per the FU-34/FU-36/FU-37 precedent the incoming row is refiled at the next free number and **no existing row was renumbered, edited or displaced**. Every reference to "FU-48" in this branch's code, tests, module docs and journal sections was rewritten to "FU-54" in the merge commit; the `develop` rows keep FU-43 through FU-48 exactly as they were merged. |
| FU-55 | **OWNER RULING REQUIRED BEFORE MERGE: `feature/hollow-warhorse` converts three fail-closed models into emitted artifacts whose measured wall is 11–51 % of the registered 2.5 mm, with the `Printability` gate unimplemented and no production stage measuring realised wall** | **MERGED TO `develop` in `b71ddbc`** (commit `a3ef816`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **The interim mechanism this ruling required is built, and its realized figure now reaches the run's own output** — `realized_wall_mm` rides on the top-level `HollowReport` beside FU-37's requested `wall`, through `ChainOutcome` and `ModelPreparationOutcome::Artifact` to the CLI, after a review finding caught both dying inside `seal_and_emit`. **What the corpus rerun says about it:** the check has no real-geometry witness either, because no mini reaches drilling at all ([FU-62](#follow-ups-2026-08-04)) — the fallback has never had to catch anything in production. **LANDED 2026-08-12** ([`#hollow-realized-wall-check-2026-08-12`](#hollow-realized-wall-check-2026-08-12)) — the realized minimum wall is now measured, batched, against the source surface before the Boolean; a shell that measures strictly below the requested wall (no epsilon) is discarded and `run` converts the refusal into `SolidDecisionReason::ShellUnderWalled` through the existing solid-reserve machinery, so a hollow refusal never becomes a model rejection. Proved on real geometry (a locally-built zero-margin `HollowSettings` on the committed Sphere reproduces FU-53's own diagnosed defect and refuses correctly) and proved to run before the Boolean (a starved-Boolean-limits variant of the same fixture still refuses with `RealizedWallBelowSpec`, not a Boolean resource error). No cap, gate or tolerance moved; no constant changed. **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §1) — **DECISION MADE; THE INTERIM MECHANISM IS NOT YET BUILT, so this row stays open as work.** **The ruling: an under-walled hollow shell must NEVER ship.** In the interim a **bounded realized-wall check gates hollow emission**: a shell that fails it is **discarded**, the model **falls back to SOLID**, and it **still prints** — no model becomes "not printable" because of this check. That is this row's option **(b)**, a tightening that moves no threshold, with the existing solid reserve as the floor under it so the tightening never costs a model its artifact; option **(a)**, accepting interim under-walled emission, is **rejected**. **The fallback is explicitly interim and is not the goal:** **FU-53 is MANDATED and prioritized** — the objective is hollowing properly, not refusing gracefully. **The 2.5 mm constant does not move** (`HOLLOW_WALL_DEFAULT_MM` is the physical requirement and the engine is what is wrong; the interim check is written against the registered wall, never against a value chosen to let today's shells through). **Recorded because the record must be true:** `feature/hollow-warhorse` was merged to `develop` (`17ae61e`) while this row read *"Do not merge with the decision unmade"* — **that precondition was violated**, and this ruling **regularizes the state after the fact** rather than pretending it held: the three models emit shells at 11–51 % of the registered wall on `develop` today, so the interim check is owed on `develop` as work, not on a branch as a merge condition. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-11 by the second review round of the well-composedness work** ([`#hollow-well-composed-2026-08-11`](#hollow-well-composed-2026-08-11)). This is a decision, not a defect: the defect is FU-53 and it is already recorded. **The state of affairs.** Before this branch, the committed `Weeping Treant v1.2`, `Warhorse.stl` and `Giant Toad.stl` refused with `InvalidShellTopology` and reached an artifact through the solid-reserve substitution — a *solid* model, correctly walled by construction. After it they hollow, and `plinth prepare` writes a `.chitubox` of the **shell**, whose measured realised wall is **0.70 mm (28 %)**, **1.27 mm (51 %)** and **0.27 mm (11 %)** of the registered 2.5 mm respectively — the `Giant Toad` at roughly **five resin layers**. Meanwhile `PreWriteGate::Printability` returns `GateVerdict::NotYetPorted` (`crates/plinth-pipeline/src/gates.rs`), the only implemented pre-write gate is `UncoveredIslands`, and **no production stage anywhere measures or gates realised wall** — the 11–51 % figures come from a diagnostic harness, not from the pipeline. The journal states in its own words that hollowed shells are currently not printable. **What this row claims:** filing FU-53 records the defect; it does not authorise shipping under it, and no recorded ruling authorises emitting under-walled shells in the interim. A model that used to fail closed and now emits a plausible-looking artifact it cannot print is a *regression in what the owner can trust*, even though every individual change on the branch is correct. **What it does NOT claim:** that the well-composedness repair is wrong (it is right, and it only ever thickens the wall), or that FU-53 must be fixed first. **The two admissible outcomes, and this branch must take one before it merges:** (a) the owner records an explicit ruling accepting interim under-walled emission, with FU-53's ordering stated — in which case this row closes as ruled and the branch merges unchanged; or (b) hollow emission is held behind a bounded realised-wall check that refuses rather than emits until FU-53 is resolved, which is a *tightening* and moves no threshold. **Do not merge with the decision unmade.** **No cap, floor, constant or registry entry is touched by opening this row**, and nothing on the branch was changed in response to it — an agent choosing between (a) and (b) on the owner's behalf is exactly what this row exists to prevent. **Number note: filed at the next free number under the recorded collision protocol.** This row was authored as "FU-49" on `feature/hollow-warhorse`. "FU-49" is not occupied on `develop`, but FU-49 through FU-54 are consumed by the six sibling rows refiled above it in this same merge, so it is filed here at the next free number rather than renumbering any of them. **No existing row was renumbered, edited or displaced**; every reference to "FU-49" in this branch's code, tests and journal sections was rewritten to "FU-55" in the merge commit. |
| FU-56 | **Production support planning has no coverage-closure step at all, and the gate that measures coverage refuses 21 of the 22 models that reach it — the one that passes is a 25 mm flat disc** | **ATTRIBUTION ADDED 2026-08-12 — THIS LANE MOVED `make_overhang_table`'s OD-8 CENSUS DIGEST AND NEVER RE-PINNED THE CENSUS RECORD.** Ruled by the reconciliation pass ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.7) on §5.2 question 19, and recorded here because FU-38 **correctly** refused to charge another lane's movement to itself, which left the movement charged to nobody. **The movement, with its commits:** `make_overhang_table` OD-8 census digest under the **zero** delicacy field, **`16396858890798947253` (73 routed contacts) → `11652461737312715963` (74 routed contacts)**, charged to **this row** — commits **`1d407a0`** (the coverage-closure controller) and **`6ddf051`** (the review fixes that made the repairs actually close coverage), merged to `develop` in **`b9decd5`**. The "to" value is the one measured at HEAD in [the FU-38 section](#fu38-real-delicacy-repin-2026-08-12); the "from" value is the FU-32 J4 pin, still quoted as a model identifier in `crates/plinth-support/tests/penetration_census_equivalence.rs` and `crates/plinth-verify/tests/coverage_gate.rs`. **This annotation re-pins nothing and moves no digest** — it closes a hole in the chain of custody. The remaining record-keeping is [FU-84](#follow-ups-2026-08-04). **MERGED TO `develop` in `b9decd5`** (commits `1d407a0`, fix `6ddf051`; the dated record the lane never wrote landed at `4084c40`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **THE ROW IS LANDED BUT NOT CLOSED, and the 2026-08-12 corpus rerun is why:** 37 models in, **8 artifacts out** (up from 1), **0 gate refusals** (down from 21) — and **19 models now refuse at the controller instead**, so the residual is the same size and has merely changed reporter. Four of the 21 gate-refused minis now emit (`Bat Cat Owl`, `Plague Zombie`, `Skulker Zombie`, `Tavey Badger`), the first real minis this project has ever produced an artifact for. **This row's own prescribed first action is answered, by measurement, and the answer is "both, plus a third thing":** `Tavey Badger` closes and emits; `Myconid Adult` refuses at attempt **zero** with `NoEligibleRepairRows` on one 0.0092 mm² island — a candidate-pool problem; `Thorny Vegepygmy` had rows, appended them, and the router then refused the densified plan outright ([FU-64](#follow-ups-2026-08-04)). **Two questions survive the landing.** (1) A closure refusal lands as `ModelPreparationOutcome::Failed` / `PipelineExitCode::Supports` (5), **not** as `Refused` — the planner's conservative choice was `Refused { gate: None }` at the same exit code precisely so the residual stayed one class, and the measured consequence is that the run summary now reads `gate-refused=0` where the baseline read 21, so the two runs' summary lines are not directly comparable. No ruling covers it. (2) The blocking-patch predicate landed as **one implementation** (`plinth_verify::defects_for_patches` now *calls* `plinth_support::coverage_patch_is_blocking`) rather than the planner's conservative mirror-plus-equivalence-test — worth the owner's eye because it edits the gate crate the ruling said "does not move one byte"; the bytes that moved are a delegation, not a rule, and no registered number changed. **LANDED 2026-08-12** ([`#coverage-closure-and-evidence-2026-08-12`](#coverage-closure-and-evidence-2026-08-12)) — the post-routing coverage-closure controller this row named is built and is production's path: `run_support` now measures **routed** coverage under the pre-write gate's own registered radius and floor, nominates immutable repair-pool rows for every still-blocking patch (batch-first, one gathered occupancy/pool/sample pass per attempt), appends them through the new batched `SpecializedContactPlanEdit::append_regular_repair_rows`, and iterates to the registered `COVERAGE_REPAIR_MAX_ATTEMPTS` = 8 — a constant that had **no production reader** when this row was written. It refuses with the new typed `SupportRunError::CoverageClosure` only when it cannot close, and that refusal is excluded from `is_support_refusal` so it can never be answered by substituting the oriented-solid reserve. **NOTHING WAS LOOSENED, which this row demanded in capitals:** radius still 6.0 mm, patch-area floor still `VOXEL_PITCH_MM²`, uncovered-area tolerance still zero, and `plinth_verify::defects_for_patches` now *calls* `plinth_support::coverage_patch_is_blocking` instead of restating it, so the controller provably measures what the gate measures. **First evidence in this project's record that an appended repair contact reduces measured uncovered area at all:** `make_overhang_table` — this suite's "genuinely unclosable island" fixture — **closes and emits**, after review finding 1 stopped nomination scoring candidates against area it had already covered. `make_raking_blade` still cannot close from its own pool and now says so with per-attempt, per-island evidence. **What is NOT claimed: not one of the 21 refused minis has been shown to be printable** — the fixtures that close are procedural, the production re-run that would score the 21 is not part of this landing, and this row's ban on raising the radius or admitting a tolerance stands verbatim. Commits `1d407a0`, `6ddf051`, merged `b9decd5`. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-12 by the residual-refusal analysis of the 37-mini `plinth prepare` run** (dispositions recorded 2026-08-11/12; no journal section covers that run, so its own report is the evidence), under the standing owner doctrine that every refusal the pipeline still emits is an automation work item. **The count.** 37 models in, **one artifact out** — `Round 25`, a 3.5 mm-tall plain display base, emitted solid by the hollow stage's own `BelowThreshold` decision. Of the **22** models that reached the pre-write gate at all, **21 are refused by `UncoveredIslands`**, with 1–34 blocking defects each: `Bat Cat Owl` 1, `Cat (wild shape - Cat Lord)` 13, `Crawler Zombie` 2, `Decomposing Zombie` 8, `Deinonychus` 2, `Giant Toad` 34, `Giant Wolf Spider` 2, `Morel Mudskipper` 8, `Myconid Adult` 1, `Plague Zombie` 3, `Skeleton_1` 5, `Skeleton_3` 6, `Skulker Zombie` 1, `Tavey` 3, `Tavey Badger` 1, `Tavey Base` 7, `Thorny Vegepygmy` 1, `Warhorse` 22, `Zombie Cat` 2, `zombie-8-a` 1, `zombie-9-b` 6. **Nineteen of the 21 refused `HollowStageDecidedSolid` geometry** — these are refusals about supporting a *solid* mini, not about a hollowed shell, so they are not FU-53's problem and not the well-composedness work's. (The two exceptions, `Giant Toad` and `Warhorse`, carry `HollowStageRefused` from FU-59 and were gate-judged on the oriented-solid reserve.) Against the prior run's 6 gate-refusals of 7 reachable models the *rate* is essentially unchanged; what changed is that three times as many models now reach the gate, so this class is the dominant residual and no mini has ever passed it. **The mechanism, from source, and it is a staging gap rather than a physics one.** Production selection is `select_contacts_baseline` (`crates/plinth-support/src/lib.rs:1162`; its sole non-test caller is `crates/plinth-support/src/runner.rs:3357`), which calls `select_contacts_with_mode(..., legacy_pre_routing_repair = false)` and **returns at `lib.rs:1304`, immediately before the coverage-repair loop**. That loop — measure the failing coverage samples, rank repair rows, add contacts, iterate to `COVERAGE_REPAIR_MAX_ATTEMPTS` (registered value **8**), and refuse `ContactSelectionError::UnsatisfiedCoverage` if it cannot close — exists complete at `lib.rs:1313-1359` and is reachable **only** through the public `select_contacts`, whose every remaining caller in the workspace is a unit test. `COVERAGE_REPAIR_MAX_ATTEMPTS` is therefore a registered constant with **no production reader**: it is hashed into the selection digest (`crates/plinth-support/src/specialized.rs:8050`) and decides nothing. **What was meant to replace it was never built.** The S1 note at `lib.rs:1154` says "New production routing consumes `select_contacts_baseline` plus the immutable repair pool"; `RepairPool` is duly built (`specialized.rs:1906`), lineage-validated, contact-validated and hashed — and `specialized.rs:1891` states the gap in its own words: rows enter the pool "**before any repair controller exists**". No repair controller exists anywhere in the workspace; `RepairPool::occupied_for_contacts` (`specialized.rs:964`) is documented as returning "stable-order rows eligible for a repair controller" and has no production consumer. **So what production actually promises each island is `MIN_CONTACTS_PER_ISLAND` = 1 contact placed by the `TARGET_SPACING_MM` = 4.0 mm spacing rule, and nothing re-measures whether those contacts cover the island's area at the `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm coverage radius until the pre-write gate does — at which point the only available answer is refusal.** **The gate is zero-tolerance, and that is correct.** `defects_for_patches` (`crates/plinth-verify/src/lib.rs:290`) raises a `Blocking` defect for any patch with `uncovered_area_mm2 > 0.0`, `cut_ambiguous`, or a failed assessment; sub-floor patches are already dropped upstream by the `patch_area_floor_mm2` = `VOXEL_PITCH_MM²` = 0.16 mm² filter, so every surviving square millimetre is a real one. **The automation this row names:** the post-routing coverage-closure controller the repair pool was built for — measure the *final routed* layout with the gate's own `measure_coverage`, nominate repair-pool rows for each uncovered patch, re-route, iterate under an admitted attempt bound, and refuse `UnsatisfiedCoverage` only when it cannot close. It lands **upstream of the gate**, which does not move one byte. **What this row claims:** production has no coverage-closure step, by staging rather than by decision, and roughly nine tenths of the machinery for one is present and disconnected. **What it does NOT claim:** that reviving the legacy pre-routing loop is the answer (it repairs *before* routing, and routing is what moves contacts), nor that any particular one of the 21 becomes printable once a controller lands — not one of them has been shown to be closable, and the first action is what would show it. **DO NOT respond to this row by raising `COVERAGE_RADIUS_FACTOR`, lowering the patch-area floor, or admitting a non-zero uncovered-area tolerance.** The gate measures the promise the planner made; a wider radius forges the promise instead of keeping it, and this is the FU-8b lesson in the gate's own domain. **First action:** on the three cheapest refusals (`Tavey Badger` 1 defect, `Thorny Vegepygmy` 1, `Myconid Adult` 1) measure how much uncovered area each defect actually is and whether a repair-pool row exists that would have covered it — that one measurement decides whether the controller is a placement problem or a candidate-pool problem. It needs FU-57 first, or it needs the pipeline re-instrumented to ask a question the pipeline already answered. **Inventory note:** `codex/normalization-controller`, listed in the worktree table above as "parked for independent review as candidate coverage-repair machinery", **no longer exists** — neither the branch (`git branch -a` at `17ae61e` lists only `develop`, `main` and the three `feature/*`) nor the worktree at `/private/tmp/plinth-normalization-controller`. The controller must be built rather than revived, and that inventory row is stale. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol.** FU-56 is unoccupied on `develop` and on every live branch (`feature/hollow-warhorse` ends at FU-49, `feature/repair-prepass` at FU-46, `feature/decimate-dupfaces` at FU-44), so no collision arose and **no existing row was renumbered, edited or displaced**. |
| FU-57 | **A coverage refusal reaches its reader as an integer: the `CoverageReport` naming the islands, faces, uncovered areas and worst gaps is built, used for the verdict, and then dropped at the report boundary** | **ANNOTATED 2026-08-12 by the reconciliation pass** ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.2, §3.3), resolving two of the questions this row's landing left open. **§5.2 question 10 — the renders omit routed contact points: STANDS.** The trim was anticipated by the plan and is documented in code. The render answers *"where is the gap and how big"*; the sidecar answers the rest. **Revisit only** if a human reading the renders reports they cannot separate a real overhang from a modelling artifact — which is the job the owner's FU-57 ruling assigned them. No row. **§5.2 question 11 — `RenderMesh::from_mesh` clones the prepared mesh on every `seal_and_emit`: STANDS, PENDING MEASUREMENT.** **Append the measured bytes and milliseconds for the largest corpus model to this row, then decide.** Refactoring costs a `run_support` signature change that was deliberately settled during the batch, and that is not spent on an unmeasured cost. Measure first is the whole ruling; **do not refactor before the number is in this cell.** **MERGED TO `develop` in `b9decd5`** (commits `cc891a8`, fix `6ddf051`; dated record `4084c40`); batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md). **Proved complete on a real 37-model run, by enumeration rather than by sampling** ([the dated section](#ruled-batch-landed-2026-08-12)): all **19** coverage refusals carry a `<stem>.coverage.json` sidecar — 19 of 19, no gaps — **53** island SVGs were written, 1 to 5 per model, capped exactly at `COVERAGE_RENDER_MAX_ISLANDS` = 5 on the seven models with more islands than that, every card ranks its worst islands worst-first with uncovered mm² and worst-gap kind, and nothing was written at any `.chitubox` target path. **Four questions survive the landing:** the four evidence bounds (`COVERAGE_RENDER_MAX_FACES` = 20,000, `COVERAGE_RENDER_MAX_ISLANDS` = 5, `MAX_HIGHLIGHT_FACES_PER_ISLAND` = 5,000, `COVERAGE_CARD_MAX_ISLANDS_SHOWN` = 3) are the implementation's own presentation bounds, not owner-derived and not in the M0 registry, and the rerun exercised all four; the evidence field rides on **both** `Refused` and `Failed` because ground truth is that the closure refusal is `Failed`, which is an interpretation of the ruling's plain text rather than a ruling; island face ids are recovered by walking the serialized report rather than through new `CoverageReport` accessors, so `plinth-verify/src` is untouched; and `RenderMesh::from_mesh` clones the prepared mesh on **every** `seal_and_emit`, success path included — a real per-model cost, stated rather than hidden. The renders also omit routed contact points, so a render shows where the gap is but not what the router did near it. **LANDED 2026-08-12** ([`#coverage-closure-and-evidence-2026-08-12`](#coverage-closure-and-evidence-2026-08-12)) — **all three evidence forms the ruling named exist.** (1) The sidecar is `<model>.coverage.json` beside the would-be artifact path (`coverage_evidence::sidecar_path` = `target.with_extension("coverage.json")`), schema `plinth.coverage-refusal.v1` from day one, embedding the full `CoverageReport` or `CoverageClosureRefusal` verbatim so it round-trips losslessly with no second format. (2) The CLI card ranks the worst islands with their uncovered mm² and worst gap (3 shown worst-first, the remainder pointed at the sidecar), so a run's refusals can be ranked without a rebuild. (3) The renders are deterministic pure-Rust SVG — no Blender, no Python, no new dependency — three orthographic panels including one along the island's own area-weighted normal so the patch is shown at true size. **Evidence, never a gate input, enforced rather than promised:** `judge_pre_write_gates` stays private and unchanged and `gates.rs` carries a test that `include_str!`s its own source and fails if that file ever names the evidence module. **Nothing is written at the `.chitubox` target path** — asserted in `prepare_folder.rs` — and a coverage refusal now also clears a stale artifact there. **Bounds ADDED, none loosened:** `COVERAGE_RENDER_MAX_FACES` = 20,000 (an over-bound mesh degrades to a silhouette plus highlighted faces and the degradation is recorded in `render_skipped`, never silently dropped), `COVERAGE_RENDER_MAX_ISLANDS` = 5, `MAX_HIGHLIGHT_FACES_PER_ISLAND` = 5,000, `COVERAGE_CARD_MAX_ISLANDS_SHOWN` = 3 — **all four are the implementation's own choice, not owner-derived, and are recorded as such rather than passed off as registered physics.** A `measurement_error` field means an *unmeasurable* gate persists a reason, so "0 island(s)" can never read as "nothing was uncovered". **Recorded rather than fixed:** the eager per-model `RenderMesh` copy in `seal_and_emit` (both lazier remedies were checked and neither works today; both dead ends are written into `RenderMesh`'s doc). Commits `cc891a8`, `6ddf051`, merged `b9decd5`. **The row as first written follows, left standing rather than substituted.** **RULED 2026-08-12 by the owner** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §2) — **the evidence is kept, in three forms, and the row stays open as the work of building them.** (1) A **sidecar `<model>.coverage.json`**, written **beside the would-be artifact path**; (2) the **CLI card summarizes the worst islands with their uncovered mm²**, so a run's refusals can be ranked without a rebuild; (3) **visualizations — renders — of the worst uncovered islands**, as part of the refusal evidence rather than as a separate diagnostic errand. All three are **evidence only, never a gate input**: `judge_pre_write_gates` stays private and unchanged, and the verdict is decided exactly as it is today and only then explained. **Nothing is ever written at the `.chitubox` target path** — the sidecar and the renders are a second file class beside it, written on the refusal path only, so the 2026-08-09 gate landing's "no file at the target path" claim is untouched. **The row as first written follows, left standing rather than substituted.** **Opened 2026-08-12 by the residual-refusal analysis of the 37-mini run**, as the measurement blocker on [FU-56](#follow-ups-2026-08-04)'s first action. **The seam, exactly.** `failure()` (`crates/plinth-pipeline/src/prepare.rs:787-793`) projects a gate stop into `ModelPreparationOutcome::Refused { gate, geometry, violation }`, where `violation` is `ChainStop::detail()` — a **`String`** — and that string is `GateRefusal::Refused`'s own `Display` (`crates/plinth-pipeline/src/gates.rs:170`): `"gate {gate:?} refused emission with {} blocking defect(s)"`. The CLI card prints exactly that and nothing more (`crates/plinth-cli/src/execute.rs:256-272`). Everything else the gate measured — per-island `sorted_face_ids`, per-patch `uncovered_area_mm2`, `cut_ambiguous`, `assessment_failed`, `worst_gap` as either `Finite(mm)` or `NoRoutedContact`, plus the full resource plan — lives in `CoverageReport` (`crates/plinth-verify/src/lib.rs`), which is **already `Serialize`/`Deserialize`**, and is discarded when the `ChainStop` is projected. **The measured consequence:** the 21 coverage refusals of the 2026-08-11/12 run are, to their reader, 21 integers between 1 and 34. Which islands, where on the model, how many square millimetres, and whether the worst gap was a distance or "no routed contact at all" — all measured, none retained. FU-56's first action cannot be taken from the run report; it requires re-running the pipeline with new instrumentation to recover an answer the pipeline already computed. **This is the FU-37/FU-39 shape a third time** — the evidence exists in process, nothing persists it — and it is the shape that makes a refusal class un-worked. **The automation:** persist the refusing model's `CoverageReport` as a per-model sidecar (it serializes today), and print the worst few islands on the card with their uncovered area, so a reader can rank 21 refusals without a rebuild. **What this row claims:** only that measured evidence is dropped at a seam that already has a serializable copy of it. **What it does NOT claim:** that a sidecar is or becomes a gate input — it is evidence, never an input to a verdict, and `judge_pre_write_gates` stays private and unchanged; nor that the file's name, format or location is settled, since `.chitubox` is the only artifact contract this pipeline has and a second file class beside it is the owner's to name. **A gate-refused model still writes no artifact:** the sidecar is written on the refusal path only, and "no file at the target path" (the 2026-08-09 gate landing's own claim) means the `.chitubox` target, which this must not touch. **First action:** the owner names the sidecar's path and format; the write itself is a few lines once named. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-57 is unoccupied on `develop` and on all three live `feature/*` branches, and no existing row was renumbered, edited or displaced.** |
| FU-58 | **`SupportRunError::Failed(SupportRunPhase)` keeps the phase and throws the reason away — 6 of 37 models fail with one word standing for fourteen possible causes, and the only capture is `#[cfg(feature = "census")]` code that a production build is forbidden to contain** | **Opened 2026-08-12 by the residual-refusal analysis of the 37-mini run.** **The six, with the caps their refusal was measured against:** `Awakened Tree` Routing (faces 429,120), `Rotting Warhorse` Routing (260,906), `Giant Bat` **Brace** (159,336), `Giant Toad (swallowing)` Routing (149,982), `Skeleton_2` Routing (82,304), `Morel base` Routing (71,466). Every one of them exits 5 with the whole of its explanation being `sealed support run failed during Routing` (or `Brace`). **The seam, from source.** `run_support` (`crates/plinth-support/src/runner.rs:542`) maps each stage's typed error through `note_stage_failure(stage, &error)` and then returns `SupportRunError::Failed(SupportRunPhase::Routing)`. `note_stage_failure` (`runner.rs:484`) is `crate::census_record!`, whose expansion is `#[cfg(feature = "census")] { … }` (`crates/plinth-support/src/lib.rs:21-28`), and `census` is documented in `crates/plinth-support/Cargo.toml` as throwaway instrumentation "quarantined so it cannot reach a production build (MAJOR-8)". **So in every build the operator can run, the typed error is constructed, borrowed by a no-op, and dropped.** What is dropped is not small: `SupportRoutingError` (`runner.rs:3001`) has **14 variants**, among them `Limit { resource, required, limit }` — the FU-32 J5 typed cap refusal, the first resource refusal routing ever had — plus `ChargeDrift`, `ResourceOverflow`, four digest-staleness variants (each of which would be a *bug*, not a capacity finding), and three transparent wrappers (`Specialized`, `Anchor`, `Cco`) that each carry their own error trees. It is `pub(crate)`, so a caller could not read it even if it were returned. **The consequence, stated precisely:** for all six models it is unknown whether a registered cap refused the run, whether the CCO could not route the geometry, or whether an authority digest went stale — three findings with three different owners, reported identically. **What the run report does carry** is the model-scaled caps, printed on the failure path by `crates/plinth-pipeline/src/prepare.rs:1222` (`faces=… max_grid_bytes=… knn_max_work=…`) — the FU-8b discipline of never reporting a refusal without its bounds. Those bounds check out as the per-face scaled values, not floors: `Giant Bat` at 159,336 faces gives 159,336 × 512 B = **81,580,032** and 159,336 × 2,048 = **326,320,128**, exactly `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` and `SUPPORT_KNN_MAX_WORK_PER_FACE`; `Morel base` at 71,466 faces sits at the **67,108,864 grid floor** with a scaled kNN of 146,362,368. Whether either bound actually bound anything is precisely what was discarded. **This blocks FU-23.** Its standing trigger — "re-derive the two scaled coefficients when a second large model arrives" — has now fired six times over on models 15–90× larger than the corpus's second-largest, and the re-derivation it asks for cannot be attributed while the refusal reason is thrown away. **The automation:** a public typed projection of each stage's failure carried on `SupportRunError`, so a phase arrives with its cause and, when the cause is a `Limit`, with the resource name, the demand and the bound — the same shape `prepare` already uses for the caps line. **DO NOT respond by enabling `census` in a production build** (MAJOR-8 forbids it, and instrumentation is not a substitute for a typed error), and **do not raise any cap**: nothing here has yet shown a cap binding. **What this row claims:** six refusals are undiagnosable by construction, in a pipeline whose whole discipline is that a refusal states what it measured. **What it does NOT claim:** that any of the six is a cap refusal, a bug, or a physics limit — that is exactly the distinction the discarded value carried. **First action:** land the projection, re-run the six, and only then read the caps question. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-58 is unoccupied on `develop` and on all three live `feature/*` branches, and no existing row was renumbered, edited or displaced.** **[ANNOTATED 2026-08-12 by the dual-cavity fixture pass ([the dated section](#dual-cavity-fixture-2026-08-12)): a seventh model hit this, and it is the first one where the discarded reason blocked a diagnosis rather than merely annoying one. `SupportRunError::Failed(SupportRunPhase::Lowering)` is all the journal can say about why a fully drilled hollow shell lost its cavity — see [FU-80](#follow-ups-2026-08-04) — so which of `LoweringError`'s seven variants fired is unrecorded, and the row's evidence is a cap pair (`max_grid_bytes=67108864`, `knn_max_work=100000000`) printed by the error's own `Display` rather than by the error itself. No cap moved; this is an annotation only.]** |
| FU-59 | **The hollow drain planner charges every candidate ray against every triangle in the mesh — a rays × faces product demanding 4.06 GB and 8.30 GB against the 1 GiB retained-H0 cap on two models whose planner refuses anything but two crossings per ray** | **Opened 2026-08-12 by the residual-refusal analysis of the 37-mini run.** **The two refusals.** `Warhorse`: `drain planning refused: H1a planning memory 4,357,463,912 exceeds retained H0 cap 1,073,741,824` (**4.06×**), bbox 18.893 × 59.656 × 54.008 mm. `Giant Toad`: **8,916,665,110** against the same cap (**8.30×**). Both then fell back to the oriented-solid reserve and were refused one stage later by the coverage gate ([FU-56](#follow-ups-2026-08-04)), so this refusal never reaches the exit code — it survives only inside `SupportedGeometry::HollowStageRefused { refusal }`, which is why the run report carries it and the summary does not. **The charge, from source** (`crates/plinth-hollow/src/drains.rs:960-1000`): `hit_values = candidate_capacity × candidate.mesh().tris().len()`, and **six** buffers are charged at that count — one `(Vec<f64>, Vec<i64>)` header pair per ray plus `(f64, usize)`, `f64`, `i64`, `f64`, `i64` payload arrays, ≈48 B per hit value before allocator slack — and that product dominates the `requested` sum against `DEFAULT_DRAIN_PLANNING_BYTES` (`crates/plinth-hollow/src/lib.rs:788`, 1 GiB). The comment states its premise honestly: "every deduplicated crossing can originate on a distinct face, so all ragged, transient and flattened payloads are charged with allocator/capacity slack". **The premise is a true worst case and a useless bound.** A drain or vent ray through a mini crosses the cavity wall and then the outer shell — which is not an assumption, it is the planner's own admission rule: `DrainPlanningRefusal::CrossingCount` refuses any candidate whose deduplicated crossings are not **exactly two** (`drains.rs:176`), and `ShellBreach` refuses any ray that does not cross cavity wall then outer shell. Charging `T` retained hits per ray in a planner that rejects everything but 2 is charging on the order of `T/2` times the demand — roughly 7×10⁴ on a 150,000-triangle mini. **This is the FU-8b/FU-50 shape a third time: the cap is not the defect, the charge is.** **DO NOT RAISE THE CAP.** The demand grows as candidates × faces, so any raised value is blown by the next larger mini, and admitting these two would enshrine a four-orders-of-magnitude overcharge as a budget. **The automation:** charge what the query can retain rather than the product — either bound each ray's retained hit list by the crossing count the planner already enforces plus a stated margin, refusing a ray that exceeds it, or gather rays in bounded batches so the retained peak is one batch's rather than the whole candidate set's (batch-first, with the charge following the batch). Either way `DEFAULT_DRAIN_PLANNING_BYTES` is untouched and gains orders of magnitude of headroom. **What this row claims:** the charge formula and the two measured demands, and that the formula's own worst case is excluded by the planner's own admission rules. **What it does NOT claim:** that the true retained peak has been measured (it has not), nor that either model plans successfully once the charge is honest — `NoExit`, `CrossingCount`, `ExitTooLong` and `ShellBreach` all remain reachable, and this row predicts none of them. **First action:** instrument `cast_rays`'s actual retained hit count on the `Warhorse` and the `Giant Toad` and put it beside `candidate_capacity × tris`; that single ratio is the whole case, and it is also the number any bounded-batch design has to be sized from. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-59 is unoccupied on `develop` and on all three live `feature/*` branches, and no existing row was renumbered, edited or displaced.** **[Second site identified 2026-08-12 by the FU-61 verification, appended here rather than opened as a new row because it is the same over-charge upstream of the one this row measures.** The `candidate_capacity` factor in `hit_values = candidate_capacity × tris` comes from `planner_preflight` (`crates/plinth-hollow/src/drains.rs:804`), which computes `lobe_capacity = low_members / minimum_lobe_voxels` — a **partition** bound that ignores cavity connectivity and therefore assumes every ~4,000-voxel chunk of the low band is a separate lobe. So the preflight inflates the candidate count and the hit charge then multiplies it by the triangle count: the same defect twice in series. FU-61's stress runs re-measured the same two refusals under thinner walls (`Warhorse` **4.20 GB**, `Giant Toad` **9.12 GB**, `Weeping Treant` **12.10 GB** against the unchanged 1 GiB cap), confirming this row's own numbers (4.06 GB / 8.92 GB at the registered wall) are the same wall, not a different one. **This is why zero real minis reach the drilling stage** at any wall thin enough to clear the savings floor — see [FU-62](#follow-ups-2026-08-04). **THE CAP WAS NOT TOUCHED and must not be**: any bounded-batch fix must fix the preflight bound and the hit charge together, and this row's DO-NOT-RAISE-THE-CAP instruction covers both sites.]** **[ANNOTATED 2026-08-12 by the dual-cavity fixture pass ([the dated section](#dual-cavity-fixture-2026-08-12)) — two more measurements, on a model far smaller than `Warhorse` or `Giant Frog`. The fixture's first sizing (a 34 × 53 mm pawn, 55,552 source faces, 82k–92k shell faces) needed **2,978,398,904 / 2,294,880,640 / 2,266,190,592 bytes** in the preflight at its three throat settings against the shipped 1,073,741,824 — **2.11×–2.77×** — so the printable article existed only under an 8× raised, caller-owned budget and no user-runnable configuration produced the hollow. **The response was to shrink the FIXTURE, not the cap** (12.5 mm/8.0 mm silhouette, 25 × 38 mm, 13,952 source faces, 25.7k–29.8k shell faces), which brings the preflight to **578–632 MB, ~42 % under the shipped ceiling**, pinned by a named test. **The dominant preflight term was measured, not guessed: `candidate_capacity × shell_triangles × 96` bytes**, i.e. the charge scales with the *shell's* marching-surface triangle count at the frozen 0.5 mm pitch, which is a function of cavity surface area — so a bounded-batch fix has a concrete target. THE CAP STILL HAS NOT MOVED.]** |
| FU-60 | **A folder run is all-or-nothing: nothing is reported until every model finishes, there is no resume, and two processes may drive the same folder into the same output directory without either noticing — 7 of 37 models produced no disposition at all** | **Opened 2026-08-12 by the residual-refusal analysis of the 37-mini run.** **What happened.** One `plinth prepare` folder run (dense orient, 7 models: `zombie-1-a.stl` … `zombie-7-a.stl`) passed 11 minutes, was SIGTERMed to keep it inside its turn, and **produced nothing whatsoever** — an empty log, no exit code, no artifact, and no disposition for any of the seven, including any model that may already have completed. Separately, an accidental duplicate process from an earlier launch attempt was found running **concurrently against the same input directory** as the intended one; both were killed. Seven of the run's 37 models are therefore unmeasured, and that is the disposition. **From source.** `prepare` (`crates/plinth-pipeline/src/prepare.rs:508-560`) enumerates the folder in sorted order, loops `prepare_one` sequentially, accumulates a `Vec<ModelPreparationReport>`, and returns; `execute` (`crates/plinth-cli/src/execute.rs:41-53`) prints only once `prepare` has returned. A killed run therefore yields nothing at all. There is no skip-existing or resume behaviour — a re-run redoes every model — and `clear_stale_artifact` (`prepare.rs:778`) deletes the artifact at a model's target before that model's new attempt, which is right within one run and destructive across an interrupted one. The single collision guard, `claimed_targets` (`prepare.rs:531`), is **per process**: it refuses two sources in one folder that claim one artifact path, and nothing stops a second process claiming the same paths — where `plinth_emit::write`'s staging-then-rename makes the loser's replacement silent by design. **The constraint the automation must respect, stated because it forbids the obvious fix.** `crates/plinth-cli/src/execute.rs:11-14` records that the report "is deterministic by construction … nothing reads a clock. Two runs over the same folder print the same bytes." **So the automation is NOT a wall-clock per-model timeout**: a clock-shaped budget makes the report non-deterministic and invents a refusal class with no admitted work bound behind it, which is exactly what the registered-cap discipline exists to prevent. What *is* deterministic is streaming — the loop is already sequential over a sorted enumeration, so printing each model's card as it completes changes neither the content nor the order of a single reported byte. **The automation, three independently landable pieces:** (1) stream each per-model card at completion, summary last, so an interrupted run's finished models are recoverable; (2) take an exclusive lock on the output directory for the run, so a second process refuses with the holder's identity instead of racing; (3) an explicit skip-if-artifact-present mode so an interrupted 37-model run resumes rather than restarts. **What this row claims:** an interrupted folder run is today indistinguishable from one that never started, and a concurrent second run is invisible to both. **What it does NOT claim:** that any of the seven models would have emitted, or where the run was spending its time — the run was never instrumented and (1) is what makes the next one evidence. **First action:** (1). **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-60 is unoccupied on `develop` and on all three live `feature/*` branches, and no existing row was renumbered, edited or displaced.** |
| FU-61 | ~~**Every drain and vent bore must be proven to fully penetrate — outer surface to cavity void at full bore width — against worst-case placement on a cavity surface that is voxel-stepped at 0.5 mm while the outer wall carries near-full model resolution**~~ | **CONFIRMED 2026-08-12**; harness committed on `develop` in `44b8df1`; batch record [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md) §4. **The ruling's premise does not describe this engine, and that is the finding:** nothing is drilled to the nominal wall depth — every hole is built over `[origin − before·dir, origin + (sampled_exit + after)·dir]` from the registered `HOLLOW_EXTRA_DRAIN_POLICY` ladder (`before` 4.0 mm, drain `after` 4.0 mm, vent `after` 5.0 mm), cut as a full-radius prism in one Boolean, with `sampled_exit` measured against source-solid occupancy and `validate_frozen_hole_geometry` enforcing `length_mm <= sampled_exit_mm`, so the far cap can only move further out. The hypothesised ~0.87 mm overshoot is already there and already registered. **The lattice takes nothing off the opening:** measured clear radius is 0.99100 of nominal on all four holes and the 24-gon cutter's own inradius is cos(π/24) = 0.9914449, bracketed exactly by the scan. **The worst case was exercised, not argued:** 20,563 of 45,316 probes (45 %) started already inside shell material at full bore offset and every one still opened at full width. **Three questions survive the confirmation.** (1) **Coverage is two corpus fixtures, not two real minis** — zero of 37 minis reaches drilling on `develop`, so this row's own first action ("the Sphere and one branched mini") was not satisfiable; filed as [FU-62](#follow-ups-2026-08-04), with the structural argument (the ladder is unconditional and geometry-independent) recorded as what carries the verdict beyond the fixtures. (2) The cutter is **inscribed**, so a registered 3.0 mm drain realizes 2.9743 mm and a 2.5 mm vent 2.4786 mm — 0.026 mm under the number FU-47's throat doctrine cites; circumscribing it is exactly the widening this row forbids, so nothing was changed. (3) The 4.0/4.0/5.0 ladder is restated as bare literals in `drilling.rs` and cross-asserted nowhere — filed as [FU-66](#follow-ups-2026-08-04), a *new* pinning test, which FU-8b permits. **CONFIRMED 2026-08-12** — `crates/plinth-pipeline/examples/fu61_bore_penetration.rs` measured the `Sphere`: every hole penetrates, worst-case overshoot margin 4.50 mm measured against the registered extension ladder's 4.0 mm minimum, 5.2x the 0.8660 mm voxel body diagonal. **No new margin constant is needed.** Full measurement table and method in [the 2026-08-12 FU-61/FU-51 section above](#fu61-confirmed-fu51-re-record-2026-08-12); this unblocked FU-51's re-record. **Opened 2026-08-12 by the owner's FU-51 ruling** ([`decisions/2026-08-12-owner-rulings-fu-batch.md`](decisions/2026-08-12-owner-rulings-fu-batch.md), §9), which is **VERIFY FIRST, THEN RE-RECORD**: this row is the verification, and [FU-51](#follow-ups-2026-08-04)'s single deliberate regeneration of `corpus/baseline/Sphere.pipeline.json` is blocked on it. **The verification.** On the corrected engine, prove that **every** drain and vent bore **fully penetrates** — from the outer surface through to the cavity void, **at full bore width** — under **worst-case placement** on the cavity surface, not merely on the nominal geometry. **The asymmetry that motivates it, in the owner's observation:** the cavity interior is a marching surface over a lattice at `HOLLOW_PITCH_MM` = **0.5 mm**, so it is **voxel-stepped**, while the **outer wall carries near-full model resolution**. A bore drilled to the *nominal* wall depth can therefore land on a step and open a partial-width hole — or, at the limit, no opening into the void at all — while every nominal figure in the record reads correct. **The likely remedy, stated as a magnitude and not yet as a number to register:** the bore may have to **overshoot the nominal wall on the order of a voxel body diagonal (0.5 × √3 ≈ 0.87 mm, "~0.9 mm")** to guarantee a clean full-width opening; **if the verification shows an overshoot is needed, the margin is established and registered** with the provenance any depended-on number owes. **What this row claims:** only that the guarantee is unproven today — `plinth-hollow::drilling` proves drainability with exact ray casts against the frozen cavity field, which is a statement about rays and crossings, not about the *width* of the resulting opening at the stepped cavity boundary. **What it does NOT claim:** that any shipped bore is in fact blocked or narrowed (none has been measured), that the overshoot is needed, or that `HOLLOW_DRAIN_R_MM` should move — the bore radius is the physical requirement and any margin is a separate, additional number. **DO NOT respond to this row by widening the bore or by lowering the wall**; an overshoot margin is a depth allowance for a lattice artifact, not a licence to enlarge the hole or thin the shell. **First action:** measure, on the `Sphere` (where the analytic answer is known exactly) and on one branched mini, the realised opening width at the cavity boundary for every planned drain and vent, against the nominal bore — that ratio decides whether a margin is needed and, if so, sizes it. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-61 is unoccupied on `develop` and on `main`, and at this tip (`d90310b`) those are the only branches in the ref store — the three `feature/*` branches were merged and retired — so no collision arose and no existing row was renumbered, edited or displaced.** |
| FU-62 | **Zero of 37 real minis reaches the drilling stage — every guarantee the hollow lanes landed is proved on corpus fixtures only** | **Opened 2026-08-12 by the ruled batch's scribe pass**, from two independent measurements that agree ([`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md) §2 and §4). **The measurement, from the 37-mini rerun on the fully merged engine:** 8 artifacts emitted and **every one of them is `Solid { reason: BelowThreshold }`**. Not one shell shipped. **The FU-61 verification sweep says the same thing from the other side:** 45 models attempted (37 minis + 8 corpus models), exactly **2** reached drilling — `Sphere` and `Cylinder`, both corpus fixtures. **The two causes, both measured, neither of them a cap to raise.** (a) **30 minis stop at "no interior beyond the 10 mm bulk threshold"** and **5** more (`Warhorse`, `Rotting Warhorse`, `Giant Bat`, `Weeping Treant v1.2`/`Awakened Tree` — byte-identical files, md5 `04df0834885d8188fd1bc14c1ad9d777`) clear that but fail the registered resin-saving floor `HOLLOW_SAVE_MIN_FRAC` = 0.25 at the registered 2.5 mm wall, which FU-53's honest cavity made harder to clear rather than easier. (b) **At any wall thin enough to clear the floor, the drain planner refuses on its own preflight** — [FU-59](#follow-ups-2026-08-04)'s over-charge, now identified at a second site upstream (`planner_preflight`'s `lobe_capacity = low_members / minimum_lobe_voxels`, a partition bound blind to cavity connectivity): `Warhorse` 4.20 GB, `Giant Toad` 9.12 GB, `Weeping Treant` 12.10 GB against the unchanged 1 GiB cap. **What this row claims:** FU-53's exact-distance wall fix, FU-55's realized-wall check, FU-47's flow-based connectivity and FU-61's full-penetration guarantee are all **landed and all unwitnessed on real geometry** — the whole hollow path is exercised end to end by two procedural corpus fixtures and nothing else, so a defect that only shows on branched, thin-limbed geometry cannot currently be seen at all. FU-53's own test item 3 (re-measure the raw minis' realized wall) has been outstanding since that landing for exactly this reason. **What it does NOT claim:** that any of the four landings is wrong — FU-61's verdict rests on a structural argument (the extension ladder is unconditional and geometry-independent) that does not depend on which model is drilled, and FU-53's margin makes the realized wall correct by construction. It claims the *evidence base* is two fixtures. **DO NOT respond to this row by lowering `HOLLOW_SAVE_MIN_FRAC`, raising `DEFAULT_DRAIN_PLANNING_BYTES`, or lowering the bulk threshold.** Cause (b) is a wrong estimate and belongs to FU-59; cause (a) is either correct behaviour (a 30 mm mini with thin limbs genuinely has little to hollow) or a question about what the threshold should be, and that is a ruling, not an implementer's call. **First action:** fix FU-59's two charge sites, then re-run the sweep and count how many minis reach drilling at the **registered** wall — that number decides whether cause (a) needs a ruling at all. Until then, treat every hollow-path guarantee as fixture-proved. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-62 is unoccupied and no existing row was renumbered, edited or displaced.** **[ANNOTATED 2026-08-12 by the dual-cavity fixture pass ([the dated section](#dual-cavity-fixture-2026-08-12)). **Partly answered, and the row stays open.** A purpose-built synthetic now reaches the drilling stage through the production entry point under unmodified `H0AdmissionLimits::production()` — hollow, drain plan, drill, write, re-read, certify — with the realized wall at **3.176 mm** against a requested 2.5 mm and FU-47's flow rule deciding both sides of its own boundary. So the machinery is no longer unwitnessed. **What is still unanswered is this row's actual claim:** zero *real* minis have hollowed, and a synthetic sized to clear the savings gate proves the engine works, not that the corpus reaches it. **Two findings make the row's own baseline shakier, not stronger:** `Solid { BelowThreshold }` is a catch-all that also swallows resource-cap and support-stage refusals ([FU-79](#follow-ups-2026-08-04)), so the "all 8 are `BelowThreshold`" measurement needs re-reading once dispositions are distinguishable; and even a perfect hollow can be discarded downstream by the support stage ([FU-80](#follow-ups-2026-08-04)), so reaching the drilling stage is not the same as shipping a hollow artifact. The fixture is on `feature/dual-cavity-fixture` and is **not** on `develop`.]** |
| FU-63 | **`Weeping Treant v1.2` no longer seals in the OD-8 census — `Failed(Preparation)` where it sealed at FU-32 J6, and nothing in this batch is charged with it** | **Opened 2026-08-12 by FU-38's census re-measurement.** Building the OD-8 embed-incidence census harness twice at `develop` HEAD — once with the retired `vec![0.0; faces]` feed restored verbatim, once with the real production delicacy field — the treant fails `SupportRunError::Failed(Preparation)` under **both** fields. **FU-38 neither caused nor cured it**, which is why it is a row rather than a note in FU-38's from→to table. **The prior datum:** the treant sealed in the FU-32 J6 census on 2026-08-09 with routed 306, live 1,383, digest `423276951048565625`. **What this row claims:** the one large, genuinely branched model in the corpus stopped sealing somewhere between 2026-08-09 and 2026-08-12, the census can no longer measure it, and the failure names a one-word phase and throws the reason away — which is precisely [FU-58](#follow-ups-2026-08-04)'s opacity, met here on the model that matters most. **What it does NOT claim:** any attribution. The window contains the repair pre-pass, the rasterizer rewrite, the exact-distance wall fix, the flow-connectivity change and the coverage-closure controller, and guessing which is a fabrication. **First action:** it needs FU-58 first, or the census re-instrumented to print the refusal behind `Preparation` — the same shape as FU-56's own first action, and the same answer: the record has the phase and not the fact. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-63 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-64 | **The coverage-closure controller's own repair plan can be refused by the router: `Cco(InvalidGeometry)` on 4 of 37 minis, and densifying contacts near a gap is what produces it** | **Opened 2026-08-12 by the ruled batch's corpus rerun** ([the dated section](#ruled-batch-landed-2026-08-12)). **The measurement.** Of the 19 models FU-56's controller refuses, **4** refuse with `CoverageClosureRefusal { reason: RepairPlanRefused { phase: Routing, detail: "Cco(InvalidGeometry)" } }` — `Giant Toad`, `Skeleton_3`, `Thorny Vegepygmy`, `zombie-5-a` — at attempts 1, 1, 2 and 1 respectively. The controller nominated eligible repair-pool rows, appended them through `append_regular_repair_rows`, and the CCO router then refused the resulting plan **outright** rather than routing it worse. **This was predicted before it was seen**, on a different fixture: the FU-56 implementer recorded that on the highly symmetric `Sphere.prepped.stl` repeated nomination produced a CCO-invalid plan at attempts 4–5, noted that the existing oriented-solid reserve absorbed it correctly there, and wrote that "nobody has audited whether repeatedly densifying contacts near a coverage gap can produce CCO-invalid geometry more broadly (e.g. on the 21 real minis)". It does, on four of them, and on three of the four it happens on the **first** repair attempt rather than the fifth. **What this row claims:** the closure controller's nomination and the router's geometric admissibility rule are not reconciled — the controller can propose a plan the router will not accept, and when it does the model's refusal reason becomes the router's complaint rather than the coverage gap that caused it, which is a strictly worse record than the gate refusal it replaced. `Thorny Vegepygmy` is the clearest case: one blocking defect at baseline, 0.4528 mm² of uncovered area, and its refusal now says `InvalidGeometry`. **What it does NOT claim:** that the router is wrong to refuse, that the nominated rows are individually invalid (every one is lineage- and contact-validated before nomination), or that raising `COVERAGE_REPAIR_MAX_ATTEMPTS` would help — three of the four fail on their first attempt, so more attempts is the wrong axis. **DO NOT respond to this row by loosening the router's geometry admission, by raising the attempt bound, or by widening the coverage radius so the repair is not needed.** **First action:** on `Thorny Vegepygmy` — the cheapest of the four, one island, two attempts — dump the appended rows and the router's own `InvalidGeometry` predicate and determine whether the invalidity is co-location, an arm-angle violation, or a seat collision; that one measurement decides whether nomination needs a router-admissibility filter or the router needs to tolerate a legal-but-dense plan. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-64 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-65 | **`Crawler Zombie` regressed from a pre-write-gate refusal to `Failed(Routing)` — the only model in 37 that lost ground across the ruled batch** | **Opened 2026-08-12 by the ruled batch's corpus rerun** ([the dated section](#ruled-batch-landed-2026-08-12)). **The measurement, per model against the 21 the baseline names by name:** 4 now emit, 16 still refuse on coverage, and **exactly one moved backwards**. `Crawler Zombie` reached the pre-write gate in the baseline and was refused there with **2 blocking defects**; it now fails earlier, at `sealed support run failed during Routing [faces=45972 max_grid_bytes=67108864 knn_max_work=100000000]`. **This is a genuine stage regression, not a re-classification:** the 19 coverage-closure refusals are the same physical fact reported by a new mechanism, but a `Failed(Routing)` at attempt 0 happens *before* the closure controller ever looks at the model, so this is not FU-56's refusal wearing a different name. **What this row claims:** one model that used to get far enough to be told what was wrong with it now stops earlier and says only "Routing". **What it does NOT claim:** any attribution. The batch changed the rasterizer (FU-50, which marks a superset of material voxels and moves every downstream EDT), cavity selection (FU-53, FU-47) and the support planner (FU-56); `Crawler Zombie` is `HollowStageDecidedSolid` in both runs, which *narrows* the field to the support side but does not settle it, and guessing further would be a fabrication. **First action:** it needs [FU-58](#follow-ups-2026-08-04) first — the phase is all the record carries — or a bisect of the four merge commits against this one model, which is cheap (45,972 faces) and is the only model for which a bisect is warranted. **DO NOT respond to this row by raising `knn_max_work` or any routing cap**; nothing in the record says a cap was hit, and the FU-8b lesson is that a stated demand is evidence and a stopped stage is not. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-65 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-66 | **The drain/vent keepout extension ladder is restated as bare literals in `drilling.rs` and cross-asserted against the registry nowhere — move the ladder and every model refuses while every constants test stays green** | **Opened 2026-08-12 by the FU-61 verification.** FU-61's whole verdict — every bore fully penetrates, worst-case margin 4.50 mm — rests on the registered `HOLLOW_EXTRA_DRAIN_POLICY.extensions_mm` ladder `[4.0, 8.0, 9.0]` yielding `before` 4.0 mm, drain `after` 4.0 mm, vent `after` 5.0 mm. **Only `drains.rs` reads that registry entry.** `drilling.rs` restates the same numbers as literals in two places — `audit_hole` (`crates/plinth-hollow/src/drilling.rs:1124`) hardcodes keepout `-4.0`, `sampled + 4.0` and `radius_mm 1.5`, and `validate_frozen_hole_geometry` (`drilling.rs:1314`) hardcodes `before == 4.0` and `after == sampled + 4.0`/`+ 5.0` — and nothing asserts the two agree. Likewise `HOLLOW_DRAIN_R_MM` = 1.5 mm is known to `drains.rs` and re-typed in `drilling.rs`. **The failure mode, stated concretely:** change the registered ladder to `[5.0, 9.0, 10.0]` and every model refuses drilling with `InvalidHoleGeometry` while the entire constants suite stays green — the engine would be refusing every hole because two files disagree, and no test would say so. **This is the exact class the 2026-08-12 pitch/margin fix closed** for `HOLLOW_CAVITY_MARCH_MARGIN_MM` with an exact-equality cross-constant test, and the class `CYLINDER_SECTIONS` already has a pinning test for; these do not. **The remedy is an ADDED bound, which FU-8b permits explicitly** — a pinning test asserting `drilling.rs`'s literals equal the registered ladder and `HOLLOW_DRAIN_R_MM`, or better, deleting the literals and reading the registry at both sites. **Nothing is loosened either way.** **What this row does NOT claim:** that the values currently disagree — they were measured to agree, which is why FU-61's verdict holds. It claims the agreement is unenforced. **First action:** add the exact-equality test; it is small, it fails loudly if either side moves, and it should land before anything touches the ladder. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-66 is unoccupied and no existing row was renumbered, edited or displaced.** **[Annotated 2026-08-12 by the drainage-doctrine scribe pass.** The owner's drainage rulings' engine reality check names the same defect from the other side — `AUDIT_DIRECTIONS` and the drill-through extensions exist in `drilling.rs` as **unregistered literals duplicating registry values** — and deliberately does **not** open a second row for it: it is this one. [FU-70](#follow-ups-2026-08-04) (the drainage architecture) will move the drain/vent geometry those literals restate, so **FU-66's exact-equality test should land before FU-70 touches the ladder**, not after. See [`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §A.7. No claim in this row changes.**]** |
| FU-67 | **`PrepareReport::exit_code()` masks a coverage-closure refusal behind an earlier load failure — a folder with both reports exit 1 and never mentions the support refusal at all** | **Opened 2026-08-12 by the FU-56/FU-57 landing**, discovered by the implementer and deliberately left as a measured finding rather than silently repaired. `PrepareReport::exit_code()`'s masking-priority rule special-cases only `PipelineExitCode::Gates`. So when a folder run contains both an earlier, unrelated `Input` failure and a later coverage-closure refusal (`Supports`, exit 5), the **`Supports` refusal is masked** and the run reports exit **1**. The behaviour is pinned by a test renamed to say so: `a_coverage_closure_refusal_behind_an_earlier_load_failure_is_masked_pending_a_priority_ruling`. **What this row claims:** the exit code is a summary of a folder, and the current priority rule can drop the most actionable class in it. With FU-56 landed, `Supports` is now the dominant residual class (19 of 37 models in the 2026-08-12 rerun), so the masking is far more likely to bite than when the rule was written. **What it does NOT claim:** that the priority rule is wrong — a load failure genuinely is upstream of everything, and an argument that the *earliest* failure should own the exit code is coherent. It claims nobody has decided, and that the implementer correctly declined to decide for the owner because changing the priority could affect other `Supports`-class failures nobody has audited. **First action:** an owner ruling on the priority order across `Input`, `Supports` and `Gates`; the implementation is small once the order is stated. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-67 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-68 | **`Refused { gate: UncoveredIslands }` is now structurally unreachable in production, and the 37-mini rerun measured it: 21 gate refusals became 0** | **Opened 2026-08-12 by the FU-56 landing and confirmed by the corpus rerun.** `UncoveredIslands` is the only ported pre-write gate, and FU-56's controller now guarantees zero blocking patches — **under the identical registered request, radius and floor** — before the gate is ever reached on any sealed run. So `ModelPreparationOutcome::Refused { gate: UncoveredIslands, .. }` appears to be structurally unreachable going forward. **Nothing was changed to force this**; no gate logic was touched, and the shared blocking predicate makes the controller measure exactly what the gate measures. It is an emergent consequence. **The measurement:** the baseline run refused **21** models at that gate; the 2026-08-12 rerun refuses **0**, with `gate-refused=0` on the summary line. **What this row claims, and it is two things.** (1) A live production branch that 21 models took last week now has no production reachability — which is the same condition [FU-52](#follow-ups-2026-08-04) exists for on a different branch, and the same drift risk: anything downstream still branching on that outcome shape is now dead code that nothing exercises. (2) **The refusal class fragmented in the run summary**, which is a reporting regression even though every individual model's disposition is at least as informative: a reader comparing the two runs' summary lines sees 21 gate refusals become 0 and 8 artifacts appear, with no line saying that 19 models refused for the same physical reason one stage earlier. This is the same question as the exit-code projection recorded against FU-56. **What it does NOT claim:** that the gate should be removed, weakened, or made unconditional — the gate is the promise-keeper and it stays exactly as it is; nor that the controller's guarantee is unconditional in principle (a `Failed(phase)` before routing still skips both, as 10 models did). **First action:** decide whether the gate branch keeps a fixture (as FU-52 asked for its own branch) or is explicitly recorded as controller-shadowed, and whether the summary line should name the closure-refusal class so two runs remain comparable. **No cap, floor, constant, gate or registry entry is touched by opening this row — and specifically, nothing about `UncoveredIslands` was loosened, moved or bypassed.** **Number note: filed at the next free number under the recorded collision protocol; FU-68 is unoccupied and no existing row was renumbered, edited or displaced.** **[Annotated 2026-08-12 by the drainage-doctrine scribe pass.** The second half of this row's first action — whether the summary line should name the closure-refusal class so two runs stay comparable — now has a **ruled answer**: orchestrator ruling R2 projects the controller's refusal onto `Refused { gate: Some(UncoveredIslands) }` with exit **3**, which re-unifies the class end to end without moving the gate one byte. That migration is filed as **[FU-76](#follow-ups-2026-08-04)**; see [`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.1 #2. **The first half is untouched** — whether the gate branch keeps a fixture or is recorded as controller-shadowed is still undecided, and this row stays OPEN for it. Nothing about `UncoveredIslands` is loosened, moved or bypassed by either.**]** |
| FU-69 | **Repo-wide rustfmt drift: 19 files are not `rustfmt`-clean under this machine's toolchain, including files no recent change touched, and no decision says which rustfmt is canonical** | **Opened 2026-08-12 by the ruled batch's gap-closing pass**, which found it and deliberately did not run a blanket format. `cargo fmt --all -- --check` fails on 19 files spanning the batch and beyond — including `crates/plinth-cli/src/execute.rs`, `crates/plinth-hollow/src/lib.rs`, `crates/plinth-pipeline/src/coverage_evidence.rs`, `crates/plinth-support/src/runner.rs`, `crates/plinth-verify/src/lib.rs` and files no lane in this batch touched. The FU-37 implementer independently reported the same thing from a separate worktree (`crates/plinth-hollow/examples/raster_delta.rs`, `crates/plinth-hollow/src/well_composed.rs`, `crates/plinth-pipeline/tests/first_artifact.rs` and untouched regions of `plinth-hollow/src/lib.rs`, all predating that change per hunk boundaries). **Some differences look like a rustfmt version or style-edition mismatch** rather than genuine drift, so a blanket `cargo fmt --all` would produce a large unrelated diff and might fight whichever toolchain the lanes used. **What this row claims:** the workspace has no enforced formatting baseline, `cargo fmt --check` is therefore not usable as a gate, and each lane has been left to hand-format its own hunks — which is what every lane in this batch actually did. **What it does NOT claim:** that any current formatting is wrong. **First action, and it is a decision before it is a change:** pin the canonical rustfmt (toolchain version and `style_edition`) in `rust-toolchain.toml`/`rustfmt.toml`, *then* run one deliberate whole-repo format as its own commit that touches nothing else, *then* add `cargo fmt --check` to the gate. Doing the format before pinning the toolchain guarantees the next machine re-drifts. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-69 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-70 | **The hollow stage retains exactly one cavity, plans exactly one vent per model, and floods the drilling side without the throat rule — the drainage architecture the owner ruled on 2026-08-12 is unbuilt** | **Opened 2026-08-12 by the owner's drainage rulings** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §A). **This is one architecture item, not seven**, because the owner ruled the pieces as one physics: E1 and E2 are *"two halves of the same physics"* and must be built together. **What the ruling authorizes.** (1) **Multi-cavity hollowing is allowed and encouraged** — verbatim: *"Multi-cavity is allowed (and encouraged!) as long as each cavity is large enough and properly drainable."* Every flow-separated interior component becomes a candidate; a candidate is admitted only if it is **large enough** (per-cavity admission against the **existing registered** savings/admission machinery — **no new physics constant without escalation to Craig**) **and properly drainable** (its own drain and vent bores satisfying the FU-47 throat doctrine). **Candidates failing either test are left solid — never a trap.** (2) **Per-cavity drains AND vents**: every admitted cavity needs **both** a downward resin outflow **and** an air inlet above it for replacement air. (3) **Cavity-to-cavity drainage is legitimate** — *"red can drain to green or to the exterior"* — so drainage is a **chain/DAG that must terminate at the exterior**, every link obeying both the throat rule and the gravity rule. (4) **Interior-bore preference**: when drilling a drain, **prefer boring into a neighbouring cavity over boring through to the outside**, to avoid scarring the visible surface. **Soft preference** — an exterior bore stays legal when no interior route works. (5) **Orientation-aware drainage**, in the **print orientation** (the orientation in which the model lifts off the film): a passage counts only if resin **gravity-flows** through it, drain bores sit at **each cavity's local low point**, and **any local pocket that would pool — a local minimum with no downward path to a drain — is a defect**, refilled solid or split into its own cavity. **Drains are therefore planned after orientation is chosen, and re-run if orientation changes.** (6) **The drilling-side drainability and trapped-pocket floods obey the `>= bore` throat rule** — no more face-adjacency-only floods, which claim drainage through necks the resin cannot pass. (7) **Phase-1 throat-widening**: a sub-bore neck between cavities may be drilled out to full bore diameter, admitted **ONLY** behind two checks — the **realized wall at the widened neck stays at or above spec** (FU-53/FU-55 doctrine) **and** a **structural check that the model, including its supports, survives printing and peel**. If either fails, fall back to treating the far side as its own cavity, or refill solid. **In phase 1 the wall gate is the NORMAL wall spec, and the implementation MUST COUNT how often that conservative gate refuses a widening** — that counter is the demand signal [FU-71](#follow-ups-2026-08-04) is gated on, and shipping the widening without the counter forecloses phase 2. **What already exists on `develop`, measured 2026-08-12 — start from these facts, not from the doctrine.** The mesh is **already in final print orientation before hollowing** (`crates/plinth-pipeline/src/prepare.rs:811`/`:830`), so item (5)'s frame is already the hollow stage's frame; drains are placed **per low lobe at each lobe's own min-z** with **downward-only search rays**; the **drainability proof is already monotone-in-z** (`crates/plinth-hollow/src/drilling.rs` ~`1497-1563` — a voxel counts as drainable only via a path that never rises) with a **hard `TrappedCavity` refusal**; **gravity-trapped pockets already get rescue drains** at their own low points (capped at **3**, each **>= 0.5 mL**) with the remainder **refilled solid**; and **a vent is already MANDATORY** (refusal without one) at the global cavity top. **The gaps this row closes:** exactly **one vent per model, not per cavity**, and **none for rescue drains**; **vent-above-drain true by construction but never asserted as an invariant**; **only one cavity ever retained** (`cavity_id` hard-coded `1` in `drilling.rs:419`/`:429`, plus `validate_shell_topology`'s single-negative-shell requirement — **the code already documents this gap**, `crates/plinth-metrics/src/topology.rs` ~`332`/`351`/`711-717` state that every cavity beyond the first is outside what the validator proves); **no geometric pocket detector** (pockets are found only as residue *after* drain placement, so "would pool" is never predicted); **the rescue-drain cap of 3 means overflow pockets silently refill**; **no drainage term in orientation scoring** (`HOLLOW_ORIENT_ENABLE_DEFAULT` is registered but unread); and **no cavity-to-cavity bores, no interior-target preference, no throat-widening repair at all**. **Constants in play:** `HOLLOW_DRAIN_R_MM` 1.5 mm, `HOLLOW_VENT_R_MM` 1.25 mm, lobe band 15 mm, lobe minimum 0.5 mL, vent top band 2 mm. The unregistered `drilling.rs` literals that duplicate registry values are **[FU-66](#follow-ups-2026-08-04)**, not re-filed here; **FU-66 should land before this row moves the ladder**. **DO NOT respond to this row by loosening the throat rule, lowering `HOLLOW_DRAIN_R_MM` or `HOLLOW_VENT_R_MM`, raising the rescue-drain cap to paper over undetected pockets, or introducing a per-cavity size constant without an owner ruling.** The FU-47 interim — largest interior component only — **stands until this row is built**. **First action:** build the **geometric pocket detector** and the **per-cavity vent obligation** first, because both are testable against today's single-cavity engine and both are preconditions for admitting a second cavity safely; multi-cavity retention without pocket detection would manufacture exactly the traps the doctrine forbids. **This row is measured work: it owes before/after corpus evidence.** **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-70 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-71 | **Phase 2 of the two-tier wall doctrine: a cured-strength floor at externally-supported necks — BLOCKED on FU-70's refusal counter and on coupon-plate provenance for the PROVISIONAL ~1.5 mm** | **Opened 2026-08-12 by the owner's two-tier wall ruling, staged by orchestrator ruling** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §A.5). **The owner's ruling.** The normal wall spec serves **GREEN-state self-support during printing**; a neck **carried by adequate external struts** only needs a **CURED-strength minimum** — **~1.5 mm proposed** — so the model does not break at support removal. Two tiers, two different physical jobs. **The staging, which is the orchestrator's and not the owner's.** Phase 1 ships inside [FU-70](#follow-ups-2026-08-04) with throat-widening gated by the **normal** wall spec only, **counting** how often that gate refuses a widening. **This row is built ONLY if that counter says the demand is material** — if the conservative gate almost never refuses, the machinery is not worth its risk and this row closes unbuilt. **What it would build:** region-tagged neck sections get the lower cured-minimum floor, admitted via a **drilling → support obligation channel verified at the gate** — a neck may only take the lower floor if the support planner has actually committed the struts that carry it, and the gate must be able to check that commitment. **This is new machinery of the same class as FU-56's coverage-closure controller**, and it is the reason this is a separate row rather than a flag. **THE 1.5 mm FIGURE IS PROVISIONAL AND IS NOT REGISTERED.** It must not be registered, read by any code, or relied on in any argument until it has **coupon-plate provenance**: neck test articles at **1.0 / 1.25 / 1.5 / 2.0 mm**, **supported vs bare**, **print + peel survival** first, then a **post-cure break test**. It then becomes a **measured owner constant, never an intuition** — the same standard every other physics constant in the registry is held to. **What this row claims:** there is a real physical distinction between the wall a shell needs to hold itself up wet and the wall a strut-carried neck needs to survive support removal, and today the engine only knows the first. **What it does NOT claim:** that 1.5 mm is right, that the demand exists, or that any current wall is too conservative. **DO NOT respond to this row by lowering `HOLLOW_WALL_DEFAULT_MM`, by applying a cured-strength floor anywhere outside a verified support obligation, or by registering 1.5 mm on the strength of the decisions record — the record explicitly forbids that.** **Nothing in phase 1 loosens any existing check, and nothing here may loosen one before the coupon plate is measured.** **First action, in order:** (1) read FU-70's refusal counter once it exists; (2) if material, design the coupon plate and get it printed — provenance before machinery. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-71 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-72 | **Prose citations of `CURRENT-STATUS.md` by line number are stale, and most were already wrong BEFORE the archive split — repair by git-history bisection, never by guessing** | **Opened 2026-08-12 by the journal-hygiene ruling** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.7). **The measurement at `9bdd165`:** **36 `CURRENT-STATUS.md:NNN` citations across 5 files** — `fu32-design-2026-08-09.md` **20**, `slicer-union-design-2026-08-07.md` **9**, `decimation-design-2026-08-06.md` **3**, `fu8g-design-2026-08-08.md` **2**, `archive/STATUS-ARCHIVE-2026-08-09.md` **2**. **The 2026-08-12 archive split (8,782 → 2,112 lines) invalidated every one of them, but it is not the cause:** the hygiene fixer **verified that most were already wrong before the split** — CURRENT-STATUS.md has had dated sections prepended continuously since 2026-08-03, so each citation drifted the moment anything was added above it. **Why they were left dangling rather than repaired in the split:** a re-pointed line number that happens to land on unrelated content is a **silently wrong** citation, which is strictly worse than a visibly stale one. Guessing was refused deliberately. **The repair method, and it is the whole point of this row:** for each citation, **bisect the git history of `CURRENT-STATUS.md` to the commit at which the citing document was written**, read what stood at that line **then**, and replace the citation with a **section anchor** naming that content. Anything cheaper produces fiction. **The convention is already in force:** `06-conventions.md` §12 ("Journal hygiene") now says **never cite `CURRENT-STATUS.md` by line number — cite the section anchor instead**, because the file churns. This row cleans up the citations written before that rule existed; it does not create the rule. **Scope note:** the 2 citations inside `archive/STATUS-ARCHIVE-2026-08-09.md` are **historical text and must not be edited** — archives are verbatim. They are counted here for completeness and should be dispositioned as "left as written, archive is verbatim" rather than repaired. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-72 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-73 | **Stale `docs/rust-migration/` path references repo-wide: 134 occurrences in 17 files pointing at a directory that no longer exists** | **Opened 2026-08-12 by the journal-hygiene ruling** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.7), which ruled it a **separate mechanical follow-up** rather than scope creep on the hygiene branch. **The measurement at `9bdd165`:** **134 occurrences of `docs/rust-migration/` across 17 files** — concentrated in `06-conventions.md` (**58**), `m0-constants-source-ledger.json` (**30**) and `archive/STATUS-ARCHIVE-2026-08-09.md` (**19**), with the remaining 27 spread across `04-migration-plan.md`, `03-physics-and-constants.md`, `02-target-architecture.md`, the design documents, `CURRENT-STATUS.md` itself and three further archive files. **This is pre-existing drift**, not caused by the archive split: the journal moved to `docs/engineering-journal/` and the old path was never swept. **What this row claims:** a reader or agent following any of these paths gets nothing, and `m0-constants-source-ledger.json`'s 30 occurrences mean a **machine-readable** artifact carries dead paths. **What it does NOT claim:** that any of the referenced *content* is missing — in nearly every case the file exists at the new path under the same name. **The fix is mechanical but not blind:** archive files are **verbatim history and must not be rewritten** (the same rule [FU-72](#follow-ups-2026-08-04) applies to its own archive sites), so the ~24 archive occurrences are dispositioned as historical text, and the ~110 live occurrences are swept. **First action:** confirm the target path for each distinct referenced file exists before rewriting anything — a sweep that repoints to an also-missing path buys nothing. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-73 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-74 | **The constants registry cannot record a RETIRED constant: `REPAIR_PINCH_SPLIT_MAX_MM` is registered with its only reader deleted, and the inventory has no way to say so without surgery** | **Opened 2026-08-12 by orchestrator ruling R11** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.3 #11). **The concrete case.** FU-47's landing removed the last production reader of `REPAIR_PINCH_SPLIT_MAX_MM`. The constant is still registered — `crates/plinth-core/src/constants/placeholders.rs:1359`, with its full Placeholder provenance — and still pinned by `crates/plinth-core/tests/constants_golden.rs:201` and `constants_placeholders.rs:191`/`:480`. **The ruling: leave it registered this batch.** Deleting it would destroy the provenance paragraph, which is the only record of why a 5e-5 mm repair ceiling was ever chosen and what it bounds. **What this row asks for:** a **retirement mechanism** — a retired class in the inventory carrying **provenance, not deletion** — so the 303-name inventory can state *"this constant's reader was removed by FU-47"* as a first-class fact, checkable by the constants tests, **without surgery on the entry or on the goldens**. **What this row claims:** the registry currently models only two states, live and absent, and the absent state loses the reasoning. A constant whose reader disappears is a real and recurring event, and the record should be able to hold it. **What it does NOT claim:** that `REPAIR_PINCH_SPLIT_MAX_MM` should be deleted, or that any registered value is wrong. **DO NOT respond to this row by deleting the entry, by loosening the golden, or by re-pointing the constant at a synthetic reader to make it look live.** **First action:** decide the shape — a `retired_by` field on the Placeholder entry, or a separate retired section with its own golden — before writing any code; the goldens are the load-bearing part and the shape decides how they move. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-74 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-75 | **`HOLLOW_CAVITY_MARCH_MARGIN_MM` is a conservative 0.866 mm and a worked construction suggests ~0.44 mm may be provable — only an airtight PROOF may tighten it** | **ANNOTATED 2026-08-12 — SEQUENCED BEHIND [FU-82](#follow-ups-2026-08-04)'s CENSUS.** Ruled by the reconciliation pass ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §5). **FU-82's census must run first.** `HOLLOW_CAVITY_MARCH_MARGIN_MM` sets where the cavity boundary falls, so tightening it **moves the pose-ambiguity band** FU-82 counts throats inside — run the census against today's margin and it measures the shipped engine; run it after a tightening and it measures a moving target, with no way to separate the two effects. FU-82's census is itself gated on [FU-83](#follow-ups-2026-08-04), so the order is **FU-83 → FU-82 → FU-75**. This changes nothing about this row's bar: **only a proof, not an argument, may tighten the constant.** **Opened 2026-08-12 by orchestrator ruling R21** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.5 #21). **The registered value and its derivation.** `HOLLOW_CAVITY_MARCH_MARGIN_MM` = **0.8660254037844386 mm** = one `HOLLOW_PITCH_MM` (0.5 mm) voxel **body diagonal**, pitch x sqrt(3), derived by an exact geometric identity and **enforced** by `the_cavity_march_margin_is_one_body_diagonal_of_the_registered_pitch`, which asserts the equality off the same registry both values come from. It exists because FU-53 measured shipped walls at **11-78 % of the requested wall**; adding this margin before cavity selection makes the realized wall **>= the requested wall by construction**. **The bound is a worst case, not a measurement.** A surface-nets vertex for a boundary cavity cell lies inside the closed cell whose corners include at least one selected cavity voxel centre, so it is **at most** one body diagonal from that centre — but a worked construction suggests the attainable displacement is closer to **~0.44 mm**, roughly half. **The ruling: KEEP 0.866 mm now.** A conservative margin **only thickens walls, and can never be a safety risk**, so there is no urgency and no risk in leaving it. **The bar for changing it, stated exactly:** produce an **airtight proof** of the tighter bound — a Fable-grade proof task — and **only a proof, not an argument, may tighten the registered constant.** A plausible construction, a numerical sweep, or a measured worst case over the corpus are all **insufficient**; the current value is safe precisely because it is proved. **What this costs meanwhile, recorded for the owner:** the margin **eats cavity volume**, which **will push some marginal models to solid** — it is one of the mechanisms behind [FU-62](#follow-ups-2026-08-04)'s finding that 5 minis clear the bulk threshold and then fail `HOLLOW_SAVE_MIN_FRAC`. **If the savings loss shows up in practice, THIS PROOF TASK IS THE LEVER — not the constant, and not the savings floor.** **DO NOT respond to this row, or to a savings shortfall, by lowering the margin, lowering `HOLLOW_SAVE_MIN_FRAC`, or relaxing the requested wall.** **First action:** state the exact geometric claim to be proved (attainable maximum displacement of a surface-nets vertex from the nearest selected cavity voxel centre, over all mask configurations) before attempting a proof; a proof of the wrong statement is worse than none. **No cap, floor, constant or registry entry is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-75 is unoccupied and no existing row was renumbered, edited or displaced.** **ANNOTATED 2026-08-13** — [FU-82](#follow-ups-2026-08-04) is **CLOSED without measurement** by owner ruling ([`decisions/2026-08-13-fu82-closed-without-measurement.md`](decisions/2026-08-13-fu82-closed-without-measurement.md)), so the **FU-83 → FU-82 → FU-75** sequencing above no longer applies — the census it named will not run. **This row's bar is unchanged:** only an airtight proof, not a cancelled census, may tighten `HOLLOW_CAVITY_MARCH_MARGIN_MM`. **CLOSED 2026-08-13 — OWNER RULING. Read this before acting on anything above.** The owner ruled directly on the wall ([the dated section](#wall-target-and-minimum-2026-08-13)) and `HOLLOW_CAVITY_MARCH_MARGIN_MM` is now **0.0 mm**. This row demanded a PROOF because the margin was the only thing standing between the engine and an under-walled shell; it no longer is. The requested wall is a **target** the cavity is cut at exactly, and the guarantee moved to a new registered bound — **`HOLLOW_WALL_MIN_MM` = 2.0 mm**, enforced by MEASURING the delivered cavity surface rather than by inflating the selection threshold. The proof task this row names is therefore **moot, not skipped**: there is no lattice margin left to prove a tighter bound for. **Every instruction above still binds if the margin ever returns:** a non-zero margin is a lattice quantity, must be re-derived from `HOLLOW_PITCH_MM` as `pitch x sqrt(3)`, and an intermediate "seems safe enough" value is mechanically refused by `the_cavity_march_margin_is_zero_or_one_body_diagonal_of_the_registered_pitch`, which admits only the ruled zero and the derived body diagonal. **This row's savings instruction is superseded on exactly one point.** It said DO NOT respond to a savings shortfall by lowering `HOLLOW_SAVE_MIN_FRAC` or relaxing the requested wall. The owner did **neither**: `HOLLOW_SAVE_MIN_FRAC` is untouched at 0.25 and `HOLLOW_WALL_DEFAULT_MM` is untouched at 2.5 mm. He moved the savings decision onto total volume instead, lowering `HOLLOW_SAVE_MIN_ML` 15.0 -> 1.5 mL against his own hand-hollowed acceptance suite. **DO NOT lower `HOLLOW_WALL_MIN_MM` for any reason without a fresh owner ruling — it is now the whole guarantee.** |
| FU-76 | **The coverage-closure refusal ships as the interim `Refused { gate: None }` / exit 5 — ruling R2 requires `Refused { gate: Some(UncoveredIslands) }` / exit 3** | **ANNOTATED 2026-08-12 — THE DEAD `evidence` FIELD DIES IN THE SAME COMMIT, NOT IN A ROW OF ITS OWN.** Ruled by the reconciliation pass ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.1, §5) on §5.2 question 8. FU-57 added its evidence field to **both** `Refused` **and** `Failed`, because ground truth at implementation time was that the closure refusal arrives as `Failed`. **Once this row projects the refusal to `Refused { gate: Some(UncoveredIslands) }`, the `Failed`-side `evidence` field is unreachable** — and a field that advertises evidence a variant can no longer carry is precisely the misleading surface [FU-58](#follow-ups-2026-08-04) condemns. **Delete it in this row's own commit.** Two commits would leave a window in which the type says something the code cannot make true, which is the defect, not the cleanup. **Opened 2026-08-12 by orchestrator ruling R2** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.1 #2). The FU-56 lane filed the disposition question rather than guessing, and shipped interim option **(b)** — `Refused { gate: None }`, exit **5** — pending the ruling. **The ruling is option (c)**, and this row migrates the shipped code to it. **The reasoning, because the migration is only correct if the reasoning holds.** The physical fact the controller reports — **coverage is not closable** — is the **same fact the gate refused on** before the controller existed, and the controller applies **the gate's own predicate** (mirrored, and pinned by an equivalence test per ruling R1/B.1 #1). So projecting the refusal to `Refused { gate: Some(UncoveredIslands) }` with exit **3** is **honest rather than cosmetic**: it keeps the residual **one class end to end**, keeps the CLI saying **GATE-REFUSED for a gate-shaped fact**, and keeps **rerun metrics comparable across batches** — which the current split does not, and which is exactly the reporting regression [FU-68](#follow-ups-2026-08-04) records as its second claim. **What this row does NOT do, and the distinction is load-bearing:** **the gate itself does not move one byte.** No gate predicate, radius, floor or verdict changes; this is a **projection of the controller's refusal onto the gate's existing vocabulary**, admissible only because the equivalence test proves the two predicates agree. **Interaction to check before landing:** exit 3 vs exit 5 changes which class `PrepareReport::exit_code()`'s masking-priority rule sees, which is **[FU-67](#follow-ups-2026-08-04)**'s open question — this row must not silently resolve FU-67's priority ruling as a side effect, and the test FU-67 named (`a_coverage_closure_refusal_behind_an_earlier_load_failure_is_masked_pending_a_priority_ruling`) must be re-read, not deleted. **First action:** land the equivalence test as a precondition if it is not already green, then migrate the projection and re-run the 37-mini sweep so the summary line is comparable with the 2026-08-12 baseline. **No cap, floor, constant, gate or registry entry is touched by this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-76 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-77 | **`CoverageReport`'s face ids are read through a `serde_json::to_value` round trip into a private-field mirror struct — ruling R6 replaces it with a read-only accessor** | **Opened 2026-08-12 by orchestrator ruling R6** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.2 #6). **The workaround as shipped.** `crates/plinth-pipeline/src/coverage_evidence.rs` serializes the whole `CoverageReport` with `serde_json::to_value` and deserializes it into a local `WireReport` that **mirrors `plinth-verify`'s private field shape** (`coverage_evidence.rs:301-303`, with the module doc at `:33` stating plainly that it *"reads the same private-field JSON shape"*). It then reads `sorted_face_ids` off that mirror. **Why the lane did it:** it read *"the gate does not move"* as forbidding any change to `plinth-verify`'s public surface. **The ruling: that reading is too broad.** *"The gate does not move"* is about **verdict behaviour**, not a freeze on **additive read-only API**. A public `sorted_face_ids` accessor on `CoverageReport` (`crates/plinth-verify/src/lib.rs:24`) is **additive, testable, and cannot change any verdict**. **Reverse the workaround.** **What this row claims:** the current path is fragile in a way tests will not catch — a field rename, a `#[serde(rename)]`, or a shape change inside `plinth-verify` breaks the mirror **at runtime with a silent `None`**, not at compile time, and the mirror duplicates a private layout across a crate boundary where nothing enforces the duplication. **What it does NOT claim:** that the current code is wrong today — it was measured working; it claims the coupling is unenforced, the same class as [FU-66](#follow-ups-2026-08-04). **First action:** add the accessor with a test that it returns the same ids the JSON walk returns, then delete the walk and the `WireReport` mirror in the same change, so the two paths are never both live. **No cap, floor, constant, gate or registry entry is touched by this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-77 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-78 | **The cavity-repair pass limit is an unregistered production default: `H0AdmissionLimits::max_cavity_repair_passes` = 8 lives only in code, and `docs/development.md` says resource caps are registered** | **Opened 2026-08-12 by orchestrator ruling R20** ([`decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](decisions/2026-08-12-drainage-doctrine-and-orchestrator-rulings.md) §B.5 #20). The FU-54 lane filed the registration question rather than guessing, and **shipped route (a)** — a private production default of **8** on `H0AdmissionLimits`, validated in `H0AdmissionLimits::new` like its six siblings, with the typed `HollowError::CavityRepairPassLimitExceeded` refusal ([the dated section](#hollow-cavity-repair-pass-limit-2026-08-12)). **The ruling is route (b)**, so the follow-up applies: the limit becomes a **registered M0 Placeholder** (`HOLLOW_CAVITY_REPAIR_MAX_PASSES`) as the **single source of truth**, `production()` stays a `const fn` via a **private const mirror**, and a **mechanical test pins mirror == registry** — the same pattern `HOLLOW_CAVITY_MARCH_MARGIN_MM` got on 2026-08-12 and the pattern [FU-66](#follow-ups-2026-08-04) asks for on the drill ladder. **Registering the same threshold once and reading it twice is not double registration** — that objection was considered and rejected in the ruling. **What this row claims:** an admission limit that can refuse a real model is a resource cap, the project's own documented rule is that resource caps are registered with provenance, and this one is not — so the owner's *"eight, and it is a ceiling never meant to bite real geometry"* reasoning lives in a journal section instead of in the inventory where every other cap's reasoning lives. **What it does NOT claim:** that 8 is wrong. **The value does not move**: registration carries the owner's number and the owner's provenance verbatim (measured real models use 1-2 passes; 8 is 4x headroom over the worst measured case; if a real model ever reaches it, **investigate the mask, never raise the number**). **DO NOT respond to this row by changing 8, and do not let the registration commit move the value.** **First action:** write the Placeholder provenance paragraph from the recorded ruling before touching code, so the registered entry carries the reasoning rather than a bare number. **No cap, floor, constant or registry entry is loosened by this row; it adds a registry entry and a pinning test.** **Number note: filed at the next free number under the recorded collision protocol; FU-78 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-79 | **`SolidDecisionReason::BelowThreshold` is a catch-all: a drain-planning resource-cap hit and a support-stage refusal of an already-drilled shell are both recorded as a savings verdict** | **Opened 2026-08-12 by the dual-cavity fixture's end-to-end runs** ([the dated section](#dual-cavity-fixture-2026-08-12)). **Two distinct measured paths reach the same false record.** (1) `hollow_error_refuses_this_geometry` in `crates/plinth-pipeline/src/prepare.rs` classifies **every** `HollowError::DrainPlanning` variant — including `ResourceLimitExceeded`, which is a memory-budget outcome and not a geometry verdict — as "this geometry is refused", and the caller substitutes `HollowDisposition::Solid { reason: SolidDecisionReason::BelowThreshold }`. Measured on the pre-resize fixture: `plan_drains` needed 2.27–2.98 GB against the shipped 1,073,741,824-byte ceiling, and a plain `plinth prepare` reported **nothing at all** about the hollow stage, then died much later at an unrelated support cap. (2) The support-refusal path substitutes the oriented solid under the **same** `solid_decision(BelowThreshold)` handoff, so a model whose hollow, drain plan **and** drilling all succeeded — see [FU-80](#follow-ups-2026-08-04) — is recorded as having failed the savings gate. **Why this row matters beyond tidiness:** [FU-62](#follow-ups-2026-08-04)'s headline measurement is "all 8 artifacts of the 37-mini rerun are `Solid { BelowThreshold }`", and this row means that string **cannot be trusted to mean what it says**; some of those 8 may be resource-cap or support refusals wearing a savings label. **What the row asks for:** distinguishable reasons — at minimum separating a drain-planning resource-cap refusal and a support-stage refusal of a drilled shell from a genuine below-threshold verdict — so the disposition names the stage that actually decided. **This is a record/serialization semantics change**: it touches `SolidDecisionReason`, which is serialized in `HollowReport` and pinned by round-trip tests, so **it owes a re-pin plan before code**. **DO NOT respond to this row by widening `hollow_error_refuses_this_geometry` to swallow more error classes, and do not raise any cap to make the misclassification stop reproducing.** **First action:** re-read the 37-mini rerun's 8 dispositions against the new classification once it exists — that re-read, not the enum change, is the deliverable. **No cap, gate, floor, tolerance or constant is touched by this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-79 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-80 | **A model can hollow, drain and drill successfully and still ship SOLID, because the support stage refuses the drilled shell during `Lowering`** | **Opened 2026-08-12 by the dual-cavity fixture's end-to-end `plinth prepare` run** ([the dated section](#dual-cavity-fixture-2026-08-12)). **The measurement.** `plinth prepare` on the fixture's `above-bore` source at `1ea01ec` (`feature/dual-cavity-fixture`) exits **0 and emits**: `create_shell` succeeds, `plan_drains` succeeds under the shipped ceiling, `drill_planned_shell` succeeds — and then the sealed support run refuses the **30,290-face** drilled shell during `SupportRunPhase::Lowering` (`max_grid_bytes=67108864`, `knn_max_work=100000000`), so `prepare` substitutes the **oriented solid** and the hollow is thrown away at the last stage. **This is the first model in the project to get all the way through hollowing and lose it downstream**, and it means [FU-62](#follow-ups-2026-08-04)'s end-to-end gap is not only a hollow-stage problem: even when hollowing works perfectly, the artifact that ships can still be solid. **Why the drilled shell is harder than the source:** hollowing replaces a 13,952-face closed solid with a 30,290-face two-shell body whose inner cavity skin is a 0.5 mm-stepped marching surface, so the support stage's spatial-acceleration structures see roughly twice the faces at much finer local spacing — the drilled shell is the *worst* input the support stage receives, and nothing measures that anywhere. **What this row owes:** a measurement of the support stage's cost on hollowed-and-drilled shells specifically (face count, grid bytes, kNN work) across the corpus, before anyone proposes a number. **DO NOT respond to this row by raising `max_grid_bytes` or `knn_max_work` — [FU-8b](#follow-ups-2026-08-04) is exactly this situation, and a cap that refuses a legitimate input is a defect in what is fed to it or in how it is fed, until a measurement says otherwise.** **Related but distinct:** the refusal's own reason is discarded before it reaches the reader, which is [FU-58](#follow-ups-2026-08-04), and the substitution is mislabelled a savings verdict, which is [FU-79](#follow-ups-2026-08-04). **No cap, gate, floor, tolerance or constant is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-80 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-81 | **The vent planner can choose a site the driller then refuses: a crown-axis vent whose horizontal search ray is tangent to the dome fails as `DrillingRefusal::BlockedOpening`** | **Opened 2026-08-12 by the dual-cavity fixture's article run** ([the dated section](#dual-cavity-fixture-2026-08-12)), reproduced by the scribe pass at `1ea01ec`. **The measurement.** On the fixture's own axis-aligned pose, at the **SEPARATED** and **AT_BORE** throat settings, `plan_drains` returns `Ok` with one drain and one vent, and `drill_planned_shell` then refuses: `planned opening 1 is not a clear cavity-to-exterior passage`. The **ABOVE_BORE** setting drills cleanly on the same pose, and **all three settings drill cleanly on every rotated pose the real chain produces** (`crates/plinth-pipeline/tests/dual_cavity_real_pipeline.rs`, 5/5 green). The apparent cause is a vent site placed on the spherical cavity crown's own axis, where the vent's horizontal search ray is tangent to the dome and so never clears a full-bore passage. **This is a refusal, so the behaviour is SAFE** — the planner proposes, the driller vetoes, nothing unprintable ships, and [FU-61](#follow-ups-2026-08-04)'s penetration guarantee is exactly this veto doing its job. **What is wrong is the disagreement**, not the veto: the planner has no notion that a site the driller cannot reach is not a site, so a legitimate cavity can be denied its mandatory vent and the whole model falls back to solid for a reason no operator can read. **The question this row asks:** is a crown-axis vent site a planner defect (the planner should evaluate reachability, or nudge off-axis, before committing a site), or correct conservatism on a dome? **It is scoped for [FU-70](#follow-ups-2026-08-04)**, which is rebuilding vent planning to be per-cavity anyway and will have to answer reachability for every cavity rather than for one. **DO NOT respond to this row by loosening the driller's clear-passage check, lowering `HOLLOW_VENT_R_MM`, or making the vent optional** — a cavity without a vent does not drain. **First action:** confirm the tangency diagnosis by instrumenting the refused ray on the fixture, which is cheap and reproducible, before touching the planner. **No cap, gate, floor, tolerance or constant is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-81 is unoccupied and no existing row was renumbered, edited or displaced.** |
| FU-82 | **FU-47's core-clearance verdict is POSE-DEPENDENT: a cavity throat within 0.354 mm of the 1.75 mm threshold flips between one and two resin-flow components on the model's orientation alone** | **Opened 2026-08-12 by the dual-cavity fixture's real-pipeline measurement** ([the dated section](#dual-cavity-fixture-2026-08-12)), where it was found as a **measured failure of the fixture's own doctrine claim**, not as a theoretical worry. **The mechanism.** `flow.rs`'s core test compares a cavity voxel's EDT clearance against `core_clearance_threshold_mm = HOLLOW_DRAIN_R_MM + pitch/2 = 1.75 mm`, and that clearance is **quantized onto the 0.5 mm lattice**. The lattice is anchored globally by `grid_spec_from_bounds`, not to the mesh, so where a throat's axis falls between samples depends on the model's pose. The worst-case in-plane distance from an arbitrary axis to the nearest sample is **0.5/√2 ≈ 0.354 mm**, so a throat whose true cavity radius is `r` can read as little as `r − 0.354 mm`. **The measurement, same process, same settings:** at a cavity-side throat of 1.90 mm the fixture's cavity was **11,999.1 mm³** on its own exact lattice phase but **10,038.8 mm³** after `repair → decimate → dense orientation sweep` — within **0.45 %** of the separated control's 9,993.4 mm³, i.e. the second chamber had silently stopped being part of the drainable interior. `1.90 − 0.354 = 1.546 < 1.75`. **The fixture was fixed by moving the FIXTURE** (cavity-side throat 1.90 → 2.45 mm, worst case 2.096 mm), never the threshold, and a fail-closed test now asserts the doctrine on the real chain and on a deliberate quarter-pitch translation. **The general finding is unfixed and belongs to real geometry:** any cavity throat in the band `[1.75, 1.75 + 0.354]` mm of true cavity radius — i.e. a physical passage radius within about 0.35 mm of `t + HOLLOW_DRAIN_R_MM + pitch/2` — may be admitted or refused depending on nothing but which way the model was turned, and the orientation sweep chooses the pose for reasons that have nothing to do with drainage. **This is directly load-bearing for [FU-70](#follow-ups-2026-08-04)**, whose whole architecture reasons about throats between cavities, and it interacts with [FU-75](#follow-ups-2026-08-04) (any tightening of `HOLLOW_CAVITY_MARCH_MARGIN_MM` moves the band). **DO NOT respond to this row by lowering the core threshold, deleting the half-voxel correction, or special-casing the sweep — the correction is FU-47's whole point and the ambiguity is a sampling fact, not a bug in the rule.** **Candidate directions to MEASURE, not to assume:** an exact (non-quantized) clearance query at throat candidates, a phase-aware bound that admits only when `clearance − 0.354 mm` still clears, or a drainage term in orientation scoring (`HOLLOW_ORIENT_ENABLE_DEFAULT` is registered and unread, per [FU-70](#follow-ups-2026-08-04)). A phase-aware bound would be a **tightening**, which is permitted; nothing here may loosen the rule. **First action:** measure how many real cavity throats in the 37-mini corpus fall inside the ambiguous band — if the answer is zero the row is cheap, and if it is not, it reframes every FU-47 result on that corpus. **No cap, gate, floor, tolerance or constant is touched by opening this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-82 is unoccupied and no existing row was renumbered, edited or displaced.** **ANNOTATED 2026-08-12 by the reconciliation pass** ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.4, §5): **this row's first action is GATED on [FU-83](#follow-ups-2026-08-04) and cannot be started before it.** The census asked for here — how many real cavity throats fall inside the ambiguous band — **cannot be produced by a pipeline that computes `FlowStats` and drops it**, which is what ships at `9e5dbb0`: both production call sites of `flow::select_largest_flow_component` discard the stats (`Some((selected, _stats))` in `plinth-hollow/src/lib.rs`, `_flow_stats` in `plinth-hollow/src/well_composed.rs`), and `H0ResourceDiagnostics` carries no flow-selection field. **Build FU-83 first.** The stated coupling with [FU-75](#follow-ups-2026-08-04) is also given an order by that pass: **census first, then any margin proof**, because tightening `HOLLOW_CAVITY_MARCH_MARGIN_MM` moves the band this row counts. **CLOSED 2026-08-13 — owner ruling, not measured.** ([`decisions/2026-08-13-fu82-closed-without-measurement.md`](decisions/2026-08-13-fu82-closed-without-measurement.md).) The pose-ambiguity question is ruled irrelevant: it may move verdicts at the margins, but the margins don't matter and the resin savings at stake do not justify further spend chasing it. The owner separately ruled the corpus's high refusal rate is correct behaviour, not a defect this row's census was needed to explain — hollowing targets Large-size creatures and bigger, and Medium minis genuinely have too little interior to be worth the wall risk. **No gate, cap, floor or threshold is loosened by this closure.** |
| FU-83 | **`H0ResourceDiagnostics` carries no flow-selection evidence: FU-47's `FlowStats` is computed on every production hollow and discarded at both call sites — and that GATES [FU-82](#follow-ups-2026-08-04)'s census** | **Opened 2026-08-12 by orchestrator ruling on §5.2 question 12** ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.4; the question as first filed is [`decisions/2026-08-12-ruled-batch-landed.md`](decisions/2026-08-12-ruled-batch-landed.md) §5.2 #12). **The FU-47 lane deferred this deliberately and said so**, to avoid a committed-golden blast radius colliding with FU-54/FU-55's concurrent edits to the same code. **That justification expired when those lanes merged**, and the ruling is: **extend it, and treat it as a prerequisite rather than a nicety.** **The measured state at `9e5dbb0`.** `flow::select_largest_flow_component` returns `(Array3<bool>, FlowStats)`, where `FlowStats` carries `flow_components`, `core_voxels` and `unreached_voxels` — the three numbers that say how many resin-flow-connected interiors a model has, how much of its cavity admits the bore outright, and how much was left unselected because it nowhere does. **Both production call sites throw the struct away**: `Some((selected, _stats))` in `crates/plinth-hollow/src/lib.rs`'s shell-creation path and `let Some((selected, _flow_stats))` in `crates/plinth-hollow/src/well_composed.rs`'s re-selection loop. Every read of those three counters anywhere in the workspace is inside `plinth-hollow`'s own `#[cfg(test)]` modules. `H0ResourceDiagnostics` has **sixteen** fields and not one of them is a flow-selection field, so **no production run has ever reported how FU-47's rule decided.** **Why this is a prerequisite and not tidiness:** [FU-82](#follow-ups-2026-08-04)'s own first action is a corpus census of cavity throats inside the pose-ambiguity band, and that census is exactly the quantity being discarded. FU-82 cannot start until this lands. **What the row asks for:** the flow-selection fields on `H0ResourceDiagnostics` — at minimum the three `FlowStats` counters, plus the transient-byte figures FU-47 named (the `f64` EDT clearance array and the `i32` label array, whose per-voxel costs `flow.rs`'s module doc already states) — plumbed from both call sites, with the same accessor pattern the fourteen existing fields use. **This is a serialized-diagnostics change and owes a re-pin plan before code**, which is the blast radius the lane correctly feared and which is now payable in isolation. **DO NOT respond to this row by raising any H0 admission limit, and do not let the plumbing commit change what `flow_labels` computes** — the numbers must be reported exactly as the selection already produces them, or the census measures the reporting instead of the geometry. **First action:** enumerate the goldens that move before touching `H0ResourceDiagnostics`, and land the plumbing and the re-pin as one attributed change. **No cap, gate, floor, tolerance or registered constant is touched by this row.** **Number note: filed at the next free number under the recorded collision protocol; the highest occupied number was FU-82 and no existing row was renumbered, edited or displaced.** **ANNOTATED 2026-08-13** — [FU-82](#follow-ups-2026-08-04) is **CLOSED without measurement** by owner ruling ([`decisions/2026-08-13-fu82-closed-without-measurement.md`](decisions/2026-08-13-fu82-closed-without-measurement.md)), so the census this row gated will not run. **This row stays OPEN on its own merits, not on that gate:** no production run has ever reported how FU-47's connectivity rule decided, and that observability gap exists independent of the pose-ambiguity question. |
| FU-84 | **The digest chain of custody has a hole: `make_overhang_table`'s OD-8 census movement is charged to no lane, FU-38's substituted blade evidence is unannotated, and FU-53's cross-lane verdict flips are unclosed** | **Opened 2026-08-12 by three orchestrator rulings on §5.2 questions 18, 19 and 20** ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.6–§3.8), which that pass ruled are **one record-keeping pass and must not be worked apart**. **This row moves no digest and re-pins nothing** — every item is prose the record already owes. **(1) The unattributed movement (question 19).** `make_overhang_table`'s OD-8 census digest under the zero field reads `11652461737312715963` with **74** routed contacts at `develop` HEAD ([the FU-38 section](#fu38-real-delicacy-repin-2026-08-12)), while the J4-recorded value still quoted in `crates/plinth-support/tests/penetration_census_equivalence.rs` and `crates/plinth-verify/tests/coverage_gate.rs` is `16396858890798947253` with **73**. FU-38 **correctly** refused to charge another lane's movement to itself, which left it charged to nobody. **The ruling: record it**, as one from→to line with its commits, in the [FU-56](#follow-ups-2026-08-04) row — done at the head of that row by this pass; this row carries any further placement (the two test comments above still cite the retired value as a model identifier). **(2) FU-38's substituted evidence (question 18).** The FU-38 ruling asked for two J4 digests; only one existed to re-pin, because `make_raking_blade`'s `74030269957898085` was retired by FU-56's landing — a refusal produces no sealed authority. The lane measured the blade under the real field and proved the refusal **byte-identical under both fields** instead. **STANDS: that is strictly better evidence than manufacturing a digest or reviving a retired pin, and a literal instruction overtaken by events is not a ruling violated.** What this row owes is **one annotation on the FU-38 record naming what was substituted and why**. **(3) FU-53's cross-lane verdict flips (question 20).** `the_zero_field_divergence_is_measured_per_committed_fixture` (`crates/plinth-pipeline/src/lib.rs`) had recorded verdicts flipped by the coverage lane rather than by FU-38's own. **RULED: correct as done — a cross-lane flip applied with full attribution under the file's own re-pin protocol is the protocol working, not a breach of it. Close it.** **Correction the closure carries:** the §5.2 framing of this question is stale — the `false → true` flips it describes were reverted `true → false` by the FU-56 review fixes, and **all three recorded verdicts read `false` at `9e5dbb0`**, so the sub-question "is identical refusal acceptable?" is moot. **What this row claims:** a digest chain of custody with an unattributed movement in it is not a chain, and the cost of closing this one is a handful of sentences. **What it does NOT claim:** that any digest is wrong, that any pin should move, or that FU-38 or FU-56 did anything incorrectly. **DO NOT respond to this row by re-pinning, regenerating or re-freezing any digest, and do not edit a recorded verdict without a from→to attribution.** **First action:** write the three annotations; nothing here needs a build. **No cap, gate, floor, tolerance or registered constant is touched by this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-83 was filed by the same pass immediately above and no existing row was renumbered, edited or displaced.** |
| FU-85 | **`corpus/baseline/Sphere.pipeline.json`'s surviving Python-era fields are unnamed and unattributed: `wall_mm` is a sampled exit distance whose value coincides exactly with a registered keepout extension** | **Opened 2026-08-12 by orchestrator ruling on §5.2 question 22** ([`decisions/2026-08-12-reconciliation-and-rulings.md`](decisions/2026-08-12-reconciliation-and-rulings.md) §3.10). **The ruling: do not re-cut the file; add a reader's note.** Regenerating the baseline to rename a field would violate the owner's FU-51 ruling that it gets **one** deliberate recorded regeneration — which it already had, at `44b8df1` ([the dated section](#fu61-confirmed-fu51-re-record-2026-08-12)). **The finding that makes the note worth writing**, verified against the file at `9e5dbb0`: `drains[0].wall_mm` reads **4.0** and `vent.wall_mm` reads **5.0**, which are **exactly** the registered drain/vent `after` keepout-extension values (`HoleRole::Drain => 4.0`, `HoleRole::Vent => 5.0` in `crates/plinth-hollow/src/drilling.rs`, the ladder [FU-66](#follow-ups-2026-08-04) is opened against). The field actually carries the quantity Rust's `HoleGeometry::sampled_exit_mm` carries. So **three different quantities — a sampled exit distance, a wall, and a keepout extension — coincide numerically on this one file**, and a reader has no way to tell which is meant. (The §5.1 answer that named this quantity quoted the *pre*-FU-51 figures `1.0`/`4.0`; that quote is corrected in place in the batch record.) **What this row asks for:** name each surviving field of the baseline in the corpus documentation ([`corpus-provisioning.md`](corpus-provisioning.md) / [`05-chitubox-and-corpus.md`](05-chitubox-and-corpus.md)) — what the quantity is, and **which fields have no Rust reader at all** (`solid_ml`, `final_ml`, `decision`, the fingerprints and the transform were deliberately left untouched by the re-record because reproducing them would require re-running the retired Python pipeline). **What it does NOT claim:** that any value in the file is wrong. **DO NOT respond to this row by regenerating, renaming inside, or hand-editing `corpus/baseline/Sphere.pipeline.json`** — the file's one authorized regeneration is spent, and hand-editing a baseline is worse than a confusing name. **First action:** write the field table from the file as it stands; no build, no run, no regeneration. **No cap, gate, floor, tolerance or registered constant is touched by this row.** **Number note: filed at the next free number under the recorded collision protocol; FU-83 and FU-84 were filed by the same pass immediately above and no existing row was renumbered, edited or displaced.** |

## Test-count counting recipe (2026-08-07) {#test-count-recipe-2026-08-07}

Certified gate test counts have been re-litigated across agents more than once — 946/947 and
947/948 historically, 970/971 immediately before this section was written. **The pair is real,
not noise, and both halves are correct — they are just two different, both-valid, invocations.**
The gap is caused by exactly one feature-gated unit test:
`chitubox_io::py::tests::every_format_gate_error_has_a_stable_python_kind` in
`rust/chitubox_io/src/py.rs`, compiled only when `chitubox_io`'s sole optional feature (`python`,
the PyO3 binding) is enabled. `python` is not in chitubox_io's `default = []` feature set, so:

- `cargo test --workspace --offline --no-fail-fast` (gate 1, default features) compiles without
  it and passed **971** as this section was written.
- `cargo test --workspace --offline --all-features --no-fail-fast` (gate 2) turns `python` on
  and passed **972** — one more, always that one test, verified by diffing the two runs'
  per-binary `running N tests` lines: every other of the 82 test-result sections (65 unit /
  integration binaries plus 14 per-crate doc-test batches, one per workspace member — the
  workspace has 14 members, but `plinth-cli` alone contributes two unit-test binaries,
  `src/lib.rs` and `src/main.rs`, which is why unit-test binaries outnumber doc-test batches by
  exactly one crate's worth) reports an identical count in both logs.

So: **"the test count" is not one number**, it is a pair, one per gate invocation, and quoting
either half without saying which gate produced it is what has been re-litigated as drift. Prior
disagreements (946/947, 947/948) are almost certainly the same ±1 default/all-features pair
observed at earlier points in the workspace's growth, not a flaky suite.

**A second, independent trap for a naive count**: do not grep the raw log text for lines that
look like individual test results and sum them. `crates/plinth-core/tests/compile_fail.rs` and
`crates/plinth-support/tests/compile_fail.rs` are `trybuild` harnesses — each is exactly **one**
`#[test]` fn as far as cargo's own libtest accounting is concerned (each contributes 1 to its
binary's own `running N tests` / `test result: ok. N passed`), but internally `trybuild` prints
its own `test tests/compile_fail/<fixture>.rs ... ok` line per fixture file it compile-checks —
23 such lines across the two harnesses in this workspace (3 + 20), none of which is a libtest
test and none of which appears in any `test result:` summary. A log-wide
`grep -c '\.\.\. ok$'` can therefore overcount by up to 23, depending on colour/tty: `trybuild`
colours its own output independently of libtest, so with ANSI colour on, those 23 fixture lines
do not end in a bare `ok` and the naive grep only *coincidentally* matches the libtest lines
alone — do not rely on that coincidence, because with colour off the same grep counts all 23.
Only a sum keyed off the `test result: ok. N passed` summary lines — the number libtest itself
already computed per binary — is authoritative.

**Canonical counting command** (run whichever gate's count you need to state, and say which):

```sh
cargo test --workspace --offline --all-features --no-fail-fast 2>&1 \
  | tee "${TMPDIR:-/tmp}/plinth-test-run.log" \
  | tail -5
grep -oE '^test result: ok\. [0-9]+ passed' "${TMPDIR:-/tmp}/plinth-test-run.log" \
  | grep -oE '[0-9]+' \
  | awk '{s+=$1} END{print s}'
```

Swap in `--offline --no-fail-fast` (no `--all-features`) for the default-feature count. Report
the number **with** the invocation it came from (e.g. "971 default-feature / 972 all-features"),
never a bare figure — that is the one canonical command and the one canonical report shape;
future verifiers should stop re-deriving this from scratch.

### Longer-horizon milestones

(Items 1-9 of this list's original "Ordered resumption checklist" — the 2026-08-05 critical
path, including the execution runner referenced as "item 9 above" below — moved to
[archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md](archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md)
with the rest of that section.)

10. Complete former M7 in its documented gate order:

   ```text
   format [integrated]
   -> coverage [integrated]
   -> clearance/transit [in progress]
   -> load
   -> connectivity
   -> printability
   -> murray
   -> ball breach
   -> response_for exhaustive table
   -> fixer-efficacy and Terminal-proof suites
   -> relocation ladder
   -> incremental re-verify == full re-verify
   -> remaining typed-registry conversion
   ```

   The preparation interface and CLI parser are integrated; the execution runner itself is
   item 9 above. The blocked resource patch is not authorization to bypass its tests.

11. Run the M7 acceptance commands in `04-migration-plan.md` only after every slice above is
   integrated. Record full-corpus score evidence and owner ChituBox review separately; the
   two GUI checkpoints in this file do not by themselves satisfy all M7 acceptance.

12. Continue M8: dense L2/L1/exact orientation, Pareto records, `plinth calibrate`,
   LoadSharing A/B, exact-byte 1/4/16-thread determinism, cache-on/off equality, and kernel
   benchmarks. O-6 makes redesign/calibration post-Python backlog, while determinism of the
   minimally ported pipeline remains a deletion gate.

13. Complete M9/native-handoff deletion under O-6/O-7: eliminate every remaining Python,
   PyO3, maturin, shim, and Python-only control; update docs/skills; run workspace test,
   Clippy, format, zero-Python searches, prepare/score, and the documented clean-checkout
   re-fire. The owner-print checkpoint remains binding. Never claim M9 from an existing
   dirty or artifact-bearing worktree.

14. Before any final merge, re-run:

   ```sh
   git status --short --branch
   git diff --check
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace
   ```

   Use an independent reviewer on the exact candidate diff. Cherry-pick reviewed commits
   into the clean integration worktree and merge only after verifying ancestry and the
   resulting diff. An agent's clean branch, passing focused test, or ACCEPT label is not a
   substitute for integration verification.

## Archived section anchors

The eight anchors below were referenced by anchor — from other documents, from
code comments, or from surviving content on this page — before their sections
were moved to `archive/` on 2026-08-12. Each stub keeps the anchor live so
existing links still resolve; the full section is at the linked archive location.

### Owner ruling: the FU-10 phase-C census is RE-PINNED, all four (2026-08-08) {#fu10-repin-2026-08-08}

`fu10-repin-2026-08-08` moved to [archive/STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08](archive/STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08).

### FU-8g phase H2 landed 2026-08-09 (2026-08-09) {#fu8g-h2-2026-08-09}

`fu8g-h2-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#fu8g-h2-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#fu8g-h2-2026-08-09).

### FU-8g phase H6 measured 2026-08-09 (2026-08-09) {#fu8g-h6-2026-08-09}

`fu8g-h6-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#fu8g-h6-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#fu8g-h6-2026-08-09).

### FU-32 phase J3 attributed 2026-08-09 (2026-08-09) {#fu32-j3-2026-08-09}

`fu32-j3-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j3-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j3-2026-08-09).

### Owner ruling: the FU-32 J2 controller-digest movement is RE-PINNED, all seven (2026-08-09) {#fu32-j4-2026-08-09}

`fu32-j4-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j4-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j4-2026-08-09).

### FU-32 phase J5 landed 2026-08-09 (2026-08-09) {#fu32-j5-2026-08-09}

`fu32-j5-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j5-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j5-2026-08-09).

### FU-32 phase J6 measured 2026-08-09 (2026-08-09) {#fu32-j6-2026-08-09}

`fu32-j6-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j6-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#fu32-j6-2026-08-09).

### CHECKPOINT 2026-08-09 — THE WEEPING TREANT'S FIRST `.chitubox`, TWICE, BYTE-IDENTICAL {#checkpoint-treant-artifact-2026-08-09}

`checkpoint-treant-artifact-2026-08-09` moved to [archive/STATUS-ARCHIVE-2026-08-09.md#checkpoint-treant-artifact-2026-08-09](archive/STATUS-ARCHIVE-2026-08-09.md#checkpoint-treant-artifact-2026-08-09).

Two sections named (not anchor-linked) by kept rows on this page also moved whole:

- *Post-merge interaction delta — ATTRIBUTED and re-pinned (2026-08-05, late)*, named by the
  ledger preamble and the FU-7 row, moved to
  [archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md#post-merge-delta-2026-08-05](archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md#post-merge-delta-2026-08-05).
- *Corpus defect fixes landed (2026-08-04, late)*, named by the FU-7 row, moved to
  [archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md#corpus-fixes-2026-08-04](archive/STATUS-ARCHIVE-2026-08-04-2026-08-05.md#corpus-fixes-2026-08-04).
