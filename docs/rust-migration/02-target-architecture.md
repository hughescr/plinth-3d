# 02 — Target Rust Architecture

**Status: NORMATIVE.** This is the architecture chapter of the Rust migration guide. Where it
conflicts with any pre-existing document in this repo, **this document wins** — see §13 for the
explicit supersession list. Read `README.md` for the map of the guide,
`01-current-system.md` for what exists in Python today, `03-physics-and-constants.md` for the
measured physics, `04-migration-plan.md` for milestone ordering and dispositions,
`05-chitubox-and-corpus.md` for the format contract and test corpus, and `06-conventions.md`
for git and code conventions.

Everything you need is in this directory. Repo docs linked below are supplemental depth, never
prerequisites.

---

## 1. Mission and rationale

Eliminate **all** Python from plinth-3d and re-implement in Rust. This is **not primarily a
performance decision.** The owner's operative rationale, near-verbatim:

> "eliminate all python because it cripples Claude's ability to think well… every exposure to
> python actively destroys productivity."

The public Python corpus is dominated by unprofiled notebook and glue code, which trains coding
agents into scalar-loop, unbatched habits. Removing Python is a **guardrail on all future agent
work in this repo**, not a benchmark chase. TypeScript/bun is permitted only if a concrete
blocker is named in writing; the expectation is pure Rust. Exactly one exception is granted in
this guide: the boolean-solid kernel binds C++ `manifold` through FFI (§8, override O-1 in §14) —
C++ FFI does not carry the agent-productivity cost that motivates the migration.

**Batch-first is law, in every language.** Data flow is gather → one batched call → scatter. A
per-item call in a hot path is a defect, not a style choice. It applies to every crate below and
is the deeper reason a GPU backend can bolt on later (§11).

**The frozen Python engine stays runnable as the measuring stick** until the Rust core beats it.
The standing rule is *no new Python*. The migration is additive until the final milestone, which
deletes all Python. The acceptance criterion is **not** byte-identical output; it is *"rust is
producing as-good-as or better output in chitubox files"*, measured by the harness in §12, with a
named owner-eyeball checkpoint per milestone for the subjective part. Byte-level identity applies
only where a real format contract exists: the `.chitubox` record structure must round-trip and
load in ChituBox.

**Hard-won lesson, encoded structurally:** gates measure the file **as written to disk**, with no
repair pass. This project once validated an in-memory mesh while shipping a different one. Every
accepting audit in this architecture re-reads the artifact from disk (§9, §12).

---

## 2. Workspace layout

The root `Cargo.toml` today is a hybrid: a `[package] name = "plinth-3d"` with `[[bin]] name =
"plinth", path = "src/main.rs"` (a 4-line stub that prints "not yet implemented") plus a
`[workspace]` with `members = ["rust/chitubox_io", "rust/geom_kernel"]`.

Convert it to a **virtual workspace manifest**: delete the root `[package]` and `[[bin]]` sections
and delete `src/main.rs`; the `plinth` binary moves to `crates/plinth-cli`. Existing crates stay
at `rust/chitubox_io` and `rust/geom_kernel` — **no path churn**. New crates live under `crates/`
with kebab-case package names.

```
plinth-3d/
  Cargo.toml            # virtual workspace: members = ["crates/*", "rust/chitubox_io", "rust/geom_kernel"]
  corpus/
    models/             # corpus STLs COPIED INTO THE REPO (repo is local-only git; large binaries OK)
    baseline/           # frozen Python outputs: *.chitubox, *.prepped.stl, *.pipeline.json — frozen ONCE
    manifest.json       # model list, per-model Python exit code, metric directions (pre-committed)
  crates/
    plinth-core/        # ids, frames, units, provenance-typed constants + registry, DefectReport,
                        #   ViolationKind, passive exhaustive FixerId, validated tagged LoadSharing,
                        #   Termination, LayerPlan, passive cost/report/metric DTOs,
                        #   Stage trait + typestate marks, digests, chord canonicalization,
                        #   deterministic reduction helpers. Chord parity is diagnostic only:
                        #   `DefectReport` remains the sole acceptance authority.
    plinth-mesh/        # f64 Mesh, STL IO with explicit WeldPolicy, face topology + cut flags,
                        #   canonical fingerprint, curvature/thinness/delicacy FaceTable,
                        #   exact z-slicing to 2D loops with winding, ProxyLadder container, triage
    plinth-decimate/    # error-bounded QEM edge collapse — decimate_pipeline.py's logic, native
    plinth-voxel/       # voxelize/fill, signed EDT, 3D connected labels, morphology, marching cubes,
                        #   priority-flood drainability
    plinth-solid/       # trait SolidBoolean + manifold (C++ FFI) backend; watertight/winding/shell audit
    plinth-cost/        # CostProvider, CandidateBatch, Evaluator (rayon now, wgpu later),
                        #   Pareto/scalarization algorithms and per-search memoization; its passive
                        #   CostVector/Fidelity/Pareto DTOs come from plinth-core
    plinth-orient/      # candidate generation (coarse-to-fine SO3), search driver, orientation cost
                        #   providers. ITS OWN STAGE — never a side effect of hollowing
    plinth-hollow/      # hollow eligibility, cavity carve, drains/vents, drainability audit,
                        #   internal-cavity support planning (A5)
    plinth-support/     # ONE crate around the whole solver: reexports core FixerId/LoadSharing;
                        #   owns Response/response_for/fixer, ArmGroup/effective_x;
                        #   perception (islands/minima/births),
                        #   placement (pool, hex/CVT lattice, X-rule sizing incl. multi-arm sharing),
                        #   Forest arena, CCO growth, rewire, bracing, transient sizing,
                        #   repair dispatcher (exhaustive ViolationKind match) + relocation ladder
    plinth-verify/      # every gate; the sole producer of Defects; SCAR-AREA-CAP / PRINT-FLOOR
                        #   certificate; the ONE authoritative interval/tip-anchor algorithm;
                        #   removal report as OPT-IN diagnostic (never on the accept path)
    plinth-emit/        # support-solid synthesis (frustum/sphere/pad/bridge/raft-MST),
                        #   BOOLEAN UNION before write, .chitubox write via chitubox_io,
                        #   then re-read-FROM-DISK audit
    plinth-metrics/     # the parity harness. Links NEITHER plinth-verify NOR geom_kernel (own slow
                        #   brute-force f64 primitives). MAY link chitubox_io's reader. Disk only;
                        #   consumes Metric DTOs from plinth-core while owning scoring behaviour.
    plinth-pipeline/    # stage sequencing over typestate artifacts, content-addressed cache,
                        #   progress reporting, report writing
    plinth-cli/         # bin `plinth`. THE only entry point. Subcommands: prepare, score, calibrate
  rust/
    geom_kernel/        # UNCHANGED role: deterministic BVH + surface-exact batched queries
    chitubox_io/        # UNCHANGED role: .chitubox container read/write + independent structural audit
```

Two crates deliberately do **not** exist: no python-shim crate, and no crate that both produces
and grades an artifact.

### 2.1 Where each Python cluster lands

| Python cluster | Fate in Rust |
|---|---|
| engine core (`scripts/support_placement/engine.py`, `pipeline.py`, `corpus.py`, `prepared_validation.py`) | `plinth-pipeline` + `plinth-support` (fix loop) + `plinth-cli`; `prepared_validation.py` deleted (replaced by typestate ownership); `corpus.py` → `plinth calibrate` |
| phase 8 (`phase8.py`, `phase8_driver.py`, `p8_accept.py`) | **Redesigned** in `plinth-support::repair` from stated invariants (strict-decrease rule, global lexicographic minimum over the full legal set, deterministic total-order tie-breaks, batch-first, shaft-before-joint ordering); NOT transliterated. Frozen-treant snapshot special-casing not ported |
| placement fields (`config.py`, `perception.py`, `placement.py`, `coverage.py`, `specialized_contacts.py`) | `plinth-core` (config → typed constants), `plinth-mesh` (delicacy), `plinth-support` (perception/placement/coverage/specialized contacts) |
| structure + growth (`tree.py`, `cco.py`, `rewire.py`, `bracing.py`, `joints.py`, `struts.py`, `routing.py`, `topology.py`) | `plinth-support`; `trunks.py` and `braces.py` are **deleted, not ported** (dead v3); the sampled clearance path is not ported |
| verify + gates (`verify.py`, `geometry_exact.py`, `transient.py`, `internal.py`, `removal.py`) | `plinth-verify` (gates, certificates), `plinth-mesh` (topology), `plinth-support` (`transient.py` — it is sizing, not verification), `plinth-hollow` (`internal.py`), `geom_kernel` (`geometry_exact.py` already ported) |
| hollow/orient/mesh (`hollow.py`, `mesh_io.py`, `geometry_kernel.py`, `scripts/tools/decimate_pipeline.py`) | `plinth-mesh` (mesh IO, triage), `plinth-orient`, `plinth-hollow`, `plinth-decimate`; the `geometry_kernel.py` wrapper is deleted |
| chitubox emit (`chitubox_emit.py`, `chitubox_inject.py`, `emit.py`) | `plinth-emit` (all geometry synthesis) + existing `chitubox_io`; `chitubox_inject.py` **not ported** |
| tools + census (`scripts/tools/*`) | `plinth-metrics` + `plinth-cli` subcommands; all bypass entry points deleted. `scripts/tools/geometry_sweep.py` is **deleted, not ported** — it is a Blender-only manual degenerate-geometry repair sweep; triage plus topology cut flags refuse what it repaired, and repairing pathological source meshes is upstream of this pipeline |
| prior-art docs | docs only; superseded where §13 states |

Smart decimation is not re-invented: `scripts/tools/decimate_pipeline.py` and
`skills/decimate-for-print/SKILL.md` describe a Blender-scripted pseudo-implementation. Extract
its parameters, stage order and feature-preservation choices and mirror them in native Rust
(error-bounded quadric edge collapse). Blender is retired in the end state — its scripting is
Python and it segfaults headless in sandboxes.

---

## 3. Core data model

### 3.1 Mesh — f64 throughout, one welding policy per purpose, canonical fingerprint

```rust
// plinth-mesh
pub struct Mesh { pub verts: Vec<[f64; 3]>, pub tris: Vec<[u32; 3]> }  // f64 end to end
pub enum WeldPolicy { None, ExactBits, Epsilon(f64) }
```

Commitment: **float64 geometry throughout**. A float32 trimesh wrapper caused real bugs. `f32`
appears in exactly one place — `chitubox_io`'s record encoder, because the wire format is f32
(STL is likewise f32 on disk; loading is lossless into f64).

The two divergent Python loading policies (`scripts/support_placement/mesh_io.py::load_stl`, which
loads with `process=False`, versus `hollow.py`'s `merge_vertices()` path) are preserved as
**explicit** `WeldPolicy` choices recorded on the loaded mesh — never unified by accident, never
implicit.

**Fingerprint correction (fix, do not reproduce):** the mesh fingerprint canonicalizes `-0.0 →
0.0` and **rejects non-finite values as a `Defect`**. The Python raw-bit weld made `-0.0` and
`0.0` distinct vertices, and a float32 round-trip could then silently fragment adjacency into
boundary cut flags that hard-failed coverage patches. The topology content digest is redefined
over the topology arrays only, with a schema version bump; the Python digest's NumPy
allocator-arithmetic counters are **not** reproduced. Byte-parity with Python digests is
explicitly abandoned (§14 O-2) — it does not touch the one place byte-identity is a real
contract, the `.chitubox` record structure.

That canonicalization is specific to the **future mesh-fingerprint boundary**. Generic numeric
content digests reject non-finite inputs and hash finite `f64` raw bits, preserving the distinction
between `-0.0` and `+0.0`; they must not silently inherit the mesh policy.

### 3.2 Frames are types, stages are typestate — the bypass-emit defect dies here

```rust
// plinth-core
pub struct ModelFrame; pub struct BuildFrame;      // Orientation is the ONLY converter
pub struct Model<S: StageMark> { /* mesh, tables, provenance, PhantomData<S> */ }
pub enum Raw {} pub enum Triaged {} pub enum Decimated {}
pub enum Oriented {} pub enum Hollowed {} pub enum Supported {} pub enum Emitted {}

pub trait Stage {
    type Input;
    type Output;
    type Context: ?Sized;
    const ID: StageId;

    fn run(&self, input: Self::Input, ctx: &Self::Context) -> Result<Self::Output, DefectReport>;
}
```

`Context` is deliberately an associated `?Sized` type: a stage may use a concrete shared context
or a trait-object context without smuggling an untyped global through the pipeline. `Stage::ID`
is the stable report-stage identity. Operational failure returns the sole validated
`DefectReport` authority; concrete `thiserror`-style error enums remain required for lower-level
library APIs that cannot produce a report.

There is no constructor for `Model<Oriented>` outside `plinth-orient`. `plinth-emit` takes
`&Model<Supported>`, which can only have come through `Oriented` and `Hollowed`. **A second entry
point that skips orientation and hollowing cannot compile** — this is the structural fix for the
Python defect where a bypass emit path skipped orientation, hollowing and drainage entirely
because two entry points existed.

`StageProvenance` records which stages ran with which config digests and is written into the
report. Stages return **owned** values; the on-disk artifact is written once, at the end, and then
audited — the Python A5 mid-run `shutil.copyfile` artifact rewrite is unrepresentable.

### 3.3 The orientation-invariant FaceTable — the load-bearing performance decision

An orientation candidate does not change the mesh; it changes one unit vector, the build direction
`g`. **Keep the mesh in model frame and rotate the gravity vector instead.** Therefore the BVH,
face adjacency, curvature/thinness (delicacy) field, EDT wall-thickness field and cavity volume are
built **once per model per level of detail** and are orientation-invariant:

```rust
// plinth-mesh
pub struct FaceTable {   // built once per (mesh, LOD); orientation-invariant
    pub area: Vec<f64>, pub centroid: Vec<[f64; 3]>, pub normal: Vec<[f64; 3]>,
    pub delicacy: Vec<f64>,    // curvature + thinness scar-cost field, [0,1]
    pub thickness: Vec<f64>,   // local wall thickness from the EDT
}
pub struct Proxy { pub mesh: Arc<Mesh>, pub faces: FaceTable, pub bvh: Bvh,
                   pub level: Lod, pub max_deviation_mm: f64 }
pub struct ProxyLadder { pub exact: Arc<Proxy>, pub l1: Arc<Proxy>, pub l2: Arc<Proxy> } // ~10%, ~2%
```

Down-face masks, z-height, scar weighting and support demand become masked reductions over this
table — microseconds per candidate on a 20k-face proxy. This is what turns "dense sweep on a
decimated proxy" from aspiration into arithmetic.

Delicacy is a property of the FaceTable **at the LOD being scored**. The Python defect where the
delicacy field came from the full mesh while face areas came from a 90%-decimated proxy (making
scar magnitudes decimation-dependent) cannot recur: field and evaluation mesh are one object.

### 3.4 One validated DefectReport, one violation enum, and a responder table that must be total

```rust
// plinth-core — passive, plain exhaustive vocabulary only. NOT #[non_exhaustive]: that attribute
// forces `_` arms in downstream crates and would structurally defeat the exhaustive-responder
// commitment, because the M7 responder table lives in plinth-support, a different crate.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ViolationKind {
    UncoveredIsland, UncoveredIslandBirth, UnsupportedMinimum,
    LoadExceeded, ArrangementInstability,
    ClearanceTransit, BallBreach, ScarCapExceeded,
    Disconnected, Unprintable, MurrayViolation,
    UnroutedContact, ContactProvenance,
    CavityNotDrainable,
    EmitBoundaryEdge, EmitNegativeShell, EmitInconsistentWinding,
    ChituboxLoadability,
    RepairCapExhausted,
}

// Passive identity only. It is exhaustive for the support responder table, but plinth-core
// neither selects nor executes a fixer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FixerId {
    AddSeatAtMarginalArea, AddBirthLadderSeat, AddMinimumSeat, SplitLoadAcrossArms,
    RelocateForLateralSpread, ReaimReseatOrQuarantine, SlideOrReseatJoint,
    ReaimReduceEmbed, RerouteToPlateOrAnchor, SteepenOrBrace, ResizeAncestors,
    RerouteOrDropRegular, AddAuditDrainOrRefill,
}

pub enum SiteId { Member(u32), Quarantine(u32), Ball(u32), Contact(u32), Face(u32), Cavity(u32) }
// A sum type — the phase8_driver.py `Q_SITE_OFFSET = 1_000_000` integer-space hack is dead.

// JSON-visible floating values are finite signed numbers. This transparent wrapper preserves the
// sign bit of -0.0 on the wire while rejecting NaN and both infinities in its fallible constructor
// and custom Deserialize implementation.
#[repr(transparent)]
#[derive(serde::Serialize)]
#[serde(transparent)]
pub struct Finite(f64);

pub struct ProvenanceManifest(
    // Canonical serialized order: DecisionId -> ConstantId -> ProvenanceRef.
    std::collections::BTreeMap<DecisionId,
        std::collections::BTreeMap<ConstantId, ProvenanceRef>>,
);

pub struct Defect {
    pub kind: ViolationKind, pub site: SiteId, pub severity: Severity,
    pub measure: Finite,         // signed finite, in the kind's own units
    pub detail: DefectDetail,
}

pub struct DefectReport {
    subject: ModelId,
    defects: Vec<Defect>,
    metrics: MetricSet,
    stage: StageId,
    provenance: ProvenanceManifest,
    termination: Termination,
    search: SearchEvidence,
}
impl DefectReport {
    pub fn new(/* all fields */) -> Result<Self, ReportError> { /* validate below */ }
    pub fn accepted(&self) -> bool { !self.defects.iter().any(|d| d.severity == Severity::Blocking) }
    // Read-only accessors expose report data; callers cannot construct or mutate an unchecked report.
}
// Do not derive Deserialize for DefectReport. Deserialize a private wire DTO, then call new();
// map ReportError to the serde error. This makes malformed reports unrepresentable in memory.
impl<'de> serde::Deserialize<'de> for DefectReport { /* private wire DTO -> new */ }
```

`Finite` is the one transparent wire representation for report floats: it is signed and
finite, including `-0.0`. `MetricValue`, `Defect::measure`, and the before/after values in a
termination therefore use it and may be negative where their semantics require it. Physical
`CostVector`'s height/time, every resin/scar quantity, `max_effective_x`, and lateral spread are
private `Finite` fields whose owning fallible constructors reject negative values (zero is valid).
All validation failures use concrete error enums (`ReportError`, `FiniteError`, and the owning
crate's error enum), not
string errors or erased trait objects.

`ProvenanceManifest` is required, not a best-effort manifest: every acceptance decision records
the ordered `DecisionId → ConstantId → ProvenanceRef` path. At the later constants-registry
checkpoint, decision-scoped recorder access must make an accept-path read impossible unless its
provenance is recorded; the finalized report must contain this complete ordered map.
`ProvenanceManifest::new` is fallible: repeated decision batches merge, and repeating an identical
decision/constant/provenance record is idempotent. `record` is likewise idempotent for an identical
record, but a different provenance value for an existing pair fails with
`ProvenanceError::ConflictingRecord`; neither API overwrites it. JSON is deliberately stricter:
custom deserialization always rejects duplicate outer `DecisionId` keys with
`ProvenanceError::DuplicateDecisionKey` and duplicate inner `ConstantId` keys with
`ProvenanceError::DuplicateConstantKey`, even if their values are identical, rather than accepting
serde's usual last-write-wins result.

`DefectReport::new` validates termination against the defects before the report exists. A
`CapExhausted` termination must name a non-`RepairCapExhausted` original kind and have a matching
blocking `RepairCapExhausted` defect at the same site. A `PoolExhausted` termination must have a
blocking defect with its same kind and site. `Converged`, `CapExhausted`, and `PoolExhausted` are
the complete termination set; `accepted()` remains **only** the no-blocking-defects predicate
shown above. In particular, `Converged` with a blocking defect is rejected solely by that blocker
scan; termination neither adds another acceptance predicate nor overrides the scan.

`DefectReport` is the **sole acceptance authority**. No `assert!`, no exception, no bare exit code,
no stringly log key participates in acceptance. The CLI exit code is a *projection* of the report.
The `p8_accept.py` gates that are unwired in Python today (orphan/dead-ancestor, raft connectivity,
idempotence, re-verify-after-serialize) become ordinary kinds and tests, actually wired into the
shipped accept path.

**A gate that can fail with no responder is a compile error.** This is M7 work: `plinth-core`
contains passive vocabulary and the exhaustive `FixerId` identity only. `plinth-support::repair`
reexports `FixerId` and owns `Response`, `response_for`, and `fixer`, where the concrete fixers
exist; `plinth-support::load` reexports `LoadSharing`:

```rust
// plinth-support::repair
pub use plinth_core::FixerId;
pub enum Response { Repair(FixerId), Terminal(&'static str) }
pub fn fixer(id: FixerId) -> &'static dyn Fixer { /* support-owned concrete implementation */ }

pub const fn response_for(k: ViolationKind) -> Response {
    match k {                              // NO `_` ARM. Adding a kind breaks the build.
        ViolationKind::UncoveredIsland        => Response::Repair(FixerId::AddSeatAtMarginalArea),
        ViolationKind::UncoveredIslandBirth   => Response::Repair(FixerId::AddBirthLadderSeat),
        ViolationKind::UnsupportedMinimum     => Response::Repair(FixerId::AddMinimumSeat),
        ViolationKind::LoadExceeded           => Response::Repair(FixerId::SplitLoadAcrossArms),
        ViolationKind::ArrangementInstability => Response::Repair(FixerId::RelocateForLateralSpread),
        ViolationKind::ClearanceTransit       => Response::Repair(FixerId::ReaimReseatOrQuarantine),
        ViolationKind::BallBreach             => Response::Repair(FixerId::SlideOrReseatJoint),
        ViolationKind::ScarCapExceeded        => Response::Repair(FixerId::ReaimReduceEmbed),
        ViolationKind::Disconnected           => Response::Repair(FixerId::RerouteToPlateOrAnchor),
        ViolationKind::Unprintable            => Response::Repair(FixerId::SteepenOrBrace),
        ViolationKind::MurrayViolation        => Response::Repair(FixerId::ResizeAncestors),
        ViolationKind::UnroutedContact        => Response::Repair(FixerId::RerouteOrDropRegular),
        ViolationKind::CavityNotDrainable     => Response::Repair(FixerId::AddAuditDrainOrRefill),
        ViolationKind::ContactProvenance      => Response::Terminal("pool integrity broken; re-run prepare"),
        ViolationKind::EmitBoundaryEdge       => Response::Terminal("union backend defect; artifact rejected, never repaired"),
        ViolationKind::EmitNegativeShell      => Response::Terminal("union backend defect; artifact rejected, never repaired"),
        ViolationKind::EmitInconsistentWinding=> Response::Terminal("union backend defect; artifact rejected, never repaired"),
        ViolationKind::ChituboxLoadability    => Response::Terminal("format contract; not repairable in-loop"),
        ViolationKind::RepairCapExhausted     => Response::Terminal("named in Termination; not re-dispatched"),
    }
}
```

`Terminal` is load-bearing: a Terminal kind is never silently ANDed into accept — the pipeline
reports *"unsatisfiable by construction for this input"* with the kind named. The Emit\* kinds are
Terminal on purpose: their only correct fix is fixing `plinth-solid`; repairing the file would
violate the audit-as-written rule.

`LoadExceeded → SplitLoadAcrossArms` is the exact fixer whose absence made the Python load gate
**unsatisfiable by construction**: `verify.load_report` (`scripts/support_placement/verify.py`,
called from `verify.py` line ~1636 and counted by `engine._load_gate_count`, engine.py:195) is
ANDed into the accept decision, but `engine._add_fix_contacts` reacts only to coverage gaps —
nothing anywhere adds contacts in response to a load violation.

Tests enforce that the table is not a lie: every `Repair` mapping has a fixture on which the fixer
**measurably reduces** that kind's measure; every `Terminal` mapping has a fixture proving genuine
in-loop unrepairability. A kind with neither is a failing test.

**Cap exhaustion names WHICH defect failed to improve:**

```rust
pub enum Termination {
    Converged,
    CapExhausted { kind: ViolationKind, fixer: FixerId, site: SiteId,
                   attempts: u32, before: Finite, after: Finite, note: String },
    PoolExhausted { kind: ViolationKind, site: SiteId },
}
// These are exactly the three variants. CapExhausted rejects kind == RepairCapExhausted and
// requires one matching Blocking Defect { kind: RepairCapExhausted, site }. PoolExhausted requires
// a Blocking Defect of its same `kind` and `site`. Never a bare exit code, never an anonymous
// "attempt cap reached".
```

### 3.5 Constants with typed provenance — the build fails on a reachable Placeholder

Mechanism: distinct wrapper types plus an `AcceptSafe` trait that `Placeholder` deliberately does
not implement.

```rust
// plinth-core::consts
pub struct Measured<T>    { value: T, plate: PlateId, err: Option<T>, date: String, note: String }
pub struct Derived<T>     { v: T, from: Vec<String>, note: String }
pub struct Placeholder<T> { v: T, note: String }
pub struct Pending<T>     { conservative: T, optimistic: T, resolves_on: PlateId, question: String }

pub trait MeasuredPayload: sealed::Sealed { /* core-owned payload validation */ }
impl<T: MeasuredPayload> Measured<T> {
    pub fn new(value: T, plate: PlateId, err: Option<T>, date: impl Into<String>,
               note: impl Into<String>) -> Result<Self, ProvenanceError>;
}
impl<'de, T: serde::Deserialize<'de> + MeasuredPayload> serde::Deserialize<'de> for Measured<T> {
    /* custom map visitor: `err` is required; JSON null becomes None */
}

pub trait AcceptSafe<T: Copy> { fn get(&self) -> T; fn provenance(&self) -> ProvenanceRef; }
impl<T: Copy + MeasuredPayload> AcceptSafe<T> for Measured<T> { /* ... */ }
impl<T: Copy> AcceptSafe<T> for Derived<T>  { /* ... */ }
// Placeholder<T> does NOT implement AcceptSafe. Every accept-path signature takes
// `&impl AcceptSafe<f64>`. Passing a Placeholder is a COMPILE ERROR, not a lint.
impl<T> Placeholder<T> { pub fn diagnostic(&self) -> &T { &self.v } }   // reading stays loud

// `resolve()` yields an AcceptSafe wrapper, NOT a bare `T` — a bare `T` could not be passed to an
// `&impl AcceptSafe<f64>` signature, which would make 06-conventions.md §5's "only via explicit
// .resolve()" row type-illegal. The returned Derived carries the pending provenance, so the
// ProvenanceManifest still names the open question that fed the accept decision.
impl<T: Copy> Pending<T> {
    pub fn resolve(&self) -> Derived<T>;     // the conservative branch — the default
    pub fn optimistic(&self) -> Derived<T>;  // A/B only; `plinth calibrate` reports the corpus delta
}
```

`Measured::date` is a required `String` carried into the corresponding `ProvenanceRef`; it is never
optional or silently dropped. `err: None` remains visible on the wire as `"err": null` for
categorical values.

Measurement payloads are a core-sealed approved set: at minimum `f64`, `Finite`, and
`LoadSharing`; an interval joins the set only if and when core has a current interval type. Numeric
payloads (`f64` and `Finite`) may use either `Some(error)` or `None`; they are not forced to invent
an error bar. Categorical `LoadSharing` requires `None`. For every approved payload the `err` key
is required on the wire: a missing key is rejected, while explicit `"err": null` means `None`.

Enforcement is **proven, not asserted**:

- a `trybuild` compile-fail test in `plinth-core` contains a file that passes a `Placeholder` to an
  accept-path signature and asserts the build fails;
- a golden test renders every constant into `docs/rust-migration/constants.md` and fails if the
  table and the code disagree, so provenance cannot rot;
- every run emits a `ProvenanceManifest` naming which constants fed each accept decision.

The single most important constant-modelling ruling: the Python triple in
`scripts/support_placement/config.py` — `peel_pressure_pa = 3000.0` (line 63),
`tensile_green_mpa = 0.288` (line 91), `tip_safety_factor = 8.0` (line 106) — is **not** ported as
three tunables. Only the ratio is measured; the Pa/MPa split is a modelling convenience. Define
`X = A_supported / A_arm_cross_section` (both mm²):

```rust
pub static X_STAR_CONE: Measured<f64> = /* 25.0 — plate 5, 30 specimens: every X <= 25.3 survived,
    every X >= 25.0 failed, one boundary inversion. Load scales with supported AREA (not perimeter,
    not a squeeze-film law). Failure is tensile snap of the NARROWEST cross-section just above the
    contact, so a long slow taper is a liability: supports must flare fast above the tip. */;
pub static X_ALLOWED: Derived<f64> = /* 12.0, from X_STAR_CONE with a 2x safety factor;
    margin-checked against X_STAR_CYLINDER's binding edge at 19.91/12 = 1.66x */;
// tensile_green_mpa / peel_pressure_pa / tip_safety_factor survive ONLY as Placeholder
// diagnostics; they can never reach accept.
pub static X_STAR_CYLINDER: Measured<f64> = /* (19.91, 30.48) — plate 6, scored 2026-07-31.
    Cylindrical arms; n = 1 at each bracket edge, conditional on 4.00 mm arms and a 30 mm^2
    cap. It CONTAINS the conical bracket, so X* ~ 25 was not an artifact of conical arms.
    Store the interval, never the 24.6 centre. Was Pending<f64> until 2026-07-31. */;
```

`X_STAR_CYLINDER` was the worked example of `Pending` in this document until plate 6 scored;
it is kept here **as a `Measured`** so the declaration matches reality. The `Pending` machinery
above stays — it is the mechanism for the next unmeasured constant, and `Pending::resolve()`
is still what keeps an open question traceable into an accept decision.

Other accept-path physics carried with provenance (full detail in `03-physics-and-constants.md`):
bridge/sag limit ~3 mm at 0.300 mm section (Measured, approximate); squeeze-film suction 40–60 kPa
at 30–40 °C (Measured envelope, diagnostic); and a `PrinterProfile` for the Elegoo Saturn 4 Ultra
(build volume, 14×19 µm pixels, tilt-release peel front sweeping **front-to-back in −Y**, layer
0.050 mm) which lives in `plinth-core::consts` because the X-rule, arrangement risk, decimation
budget and the future plater all need it.

### 3.6 Multi-arm load sharing — first-class, measured by plate 6

`X ≤ 12` is **necessary, not sufficient**: it is blind to arrangement (three collinear arms have
zero lateral stability perpendicular to their line). The peel front sweeps −Y, which predicted that
Y-strung arms meet it one at a time (zipper conditions) while X-strung arms meet it together
(sharing conditions). **Plate 6 scored 2026-07-31 and refuted the zipper branch:** strips along
both axes survived at `X_arm = 60.15` while the matched single arm died, so sharing is **at least
additive** and there is **no measured axis dependence**. Both variants stay code-complete:

```rust
// plinth-core::load — passive, tagged, and validated. Its manual Deserialize calls additive(),
// which rejects a non-finite cap or an Additive cap below 1.0.
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadSharing { Zipper, Additive { n_eff_cap: EffectiveArmCount } }
pub struct EffectiveArmCount(Finite);
impl LoadSharing { pub fn additive(n_eff_cap: f64) -> Result<Self, LoadSharingError>; }

// plinth-support::load — behaviour over the core vocabulary.
pub use plinth_core::LoadSharing;
pub struct ArmGroup { /* co-carrying arms and their contiguous rigid region */ }
/// Effective X for an arm group. `peel_dir` is currently UNUSED BY MEASUREMENT — plate 6 found
/// no axis dependence — and is retained so an orientation term can be added later without
/// changing this signature. Do not add one until plate 6 is contradicted.
pub fn effective_x(sharing: &LoadSharing, group: &ArmGroup, peel_dir: [f64; 3]) -> f64 { /* ... */ }

// Later M0 constants work, in plinth-core::constants, declares the normative static:
pub static LOAD_SHARING: Measured<LoadSharing> = /* Additive { n_eff_cap: 6.0 } — plate 6,
    2026-07-31. AT LEAST additive: N arms co-carry, but never credit more than N. n_eff_cap = 6.0
    is the largest group measured. Was Pending<LoadSharing> until 2026-07-31; `Zipper` is kept in
    the enum as the strictly-conservative fallback and the A/B arm. All five caveats in
    `03-physics-and-constants.md` §4.2 belong verbatim in `note`. */;
```

`LoadSharing`'s custom tagged-wire deserializer denies unknown fields for every variant; variant
typos or surplus legacy fields are errors, never ignored input.

The passive tagged `LoadSharing` type belongs to this core checkpoint; the `LOAD_SHARING`
declaration shown above is normative for the later M0 constants work, not a current declaration in
this bounded checkpoint.

`Measured<T>` carries `err: Option<T>`. A **categorical** payload has no error bar, so
`Measured<LoadSharing>` uses `err: None`; its serialized record carries an explicit `"err": null`
and an explicitly tagged `LoadSharing` variant. Do not invent a numeric error, omit the field, or
widen the enum to carry uncertainty. The uncertainty is conditional-scope, and it lives in `note`.

The lattice generator takes `effective_x` as an **input** — density is never baked in — so the
2026-07-31 switch to `Additive` was one constant plus a re-run, and `plinth calibrate` still reports
the corpus-wide A/B delta under both branches so the size of that switch is measured, not assumed.

The `Measured<LoadSharing>::note` serializes all five `03-physics-and-constants.md` §4.2 caveats,
not merely a pointer to them: **(1)** one cap area, arm diameter, and arm length were validated;
**(2)** no strip failed, so sharing has no measured upper bracket; **(3)** strip fin roots have a
roughly 20% section bonus, so never design on super-additive margin; **(4)** plate 6's one cap area
makes `X_tot` algebraically degenerate, leaving the ratio form dependent on plate 5's cap-area
spread; and **(5)** no axis dependence was observed at this load, while a near-boundary orientation
effect remains untested. The reference is retained with the payload: see `03-physics-and-constants.md`
§4.2 caveats 1–5.

This removes the root cause of the unsatisfiable Python gate: `config.py`'s `tip_r_max_mm = 1.0`
(line 118) caps one tip at ~3.14 mm² of cross-section, i.e. ~37.7 mm² of supported area at X = 12,
so a large flat down-face needs **N arms as a group** — which the Python data model could not
express. `ArrangementInstability` is a real `ViolationKind` with a real fixer.

**Placement:** hex/triangular lattice (~23% fewer points than a square grid for the same worst-case
gap), with CVT/Lloyd relaxation weighted by the delicacy scar-cost field. Each arm's Voronoi cell
area **is** its supported area, which gives per-arm sizing directly. Total scar area is invariant at
`A_face / X` — 8.3% of face area at X = 12, regardless of thin-dense versus thick-sparse, because
the d² cancels between capacity and cell area. The only levers exposed to any optimizer are **X**
and **where the scars land**, and the cost function says exactly that.

### 3.7 Layer height is an array on day one

```rust
// plinth-core. The wire DTO has exactly z0_mm and heights_mm; cum is derived, never serialized.
pub struct LayerPlan { z0_mm: f64, heights_mm: Vec<f64>, cum: Vec<f64> }
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerPlanWire { z0_mm: f64, heights_mm: Vec<f64> }
impl LayerPlan {
    pub fn new(z0_mm: f64, heights_mm: Vec<f64>) -> Result<Self, LayerPlanError>;
    pub fn constant(h: &impl AcceptSafe<f64>, n: usize) -> Result<Self, LayerPlanError>; // ships constant(0.050, n)
    pub fn z_of(&self, i: usize) -> Option<f64>;       // lower boundary only when i < count()
    pub fn index_at(&self, z: f64) -> Option<usize>;   // binary search on cum
    pub fn count(&self) -> usize;
}
```

`LayerPlan` custom-serializes and deserializes through `LayerPlanWire` and `new`: its raw numeric
fields stay private; `z0_mm` is a signed finite value, every height is finite and strictly
positive, and each cumulative addition must remain finite **and strictly advance the boundary**.
A finite positive height whose addition rounds away is rejected with `LayerPlanError`. An empty
`heights_mm` list is valid (zero layers); `cum` is always recomputed from the two wire fields and
cannot be supplied or trusted by a caller. `z_of(i)` returns a lower boundary only for an existing
layer (`i < count()`), so an empty plan and `i == count()` return `None`. Numerical comparisons
treat `+0.0` and `-0.0` coherently and never panic.

**No `layer_height_mm: f64` exists anywhere.** Every consumer takes `&LayerPlan`: island-birth z
tolerance, minima connect layers, the `_ISLAND_BIRTH_LINK_R_MM = 0.6` tuning in
`scripts/support_placement/engine.py:1789` (whose own comment flags dynamic-height invalidation),
transient activation, and the future slicer. The variable-height slicer later replaces
`constant()` with an optimizer output and nothing downstream changes shape.

### 3.8 Cost — the type the search is arranged around

```rust
// plinth-core::cost — passive vocabulary shared by reports, search, and future plating.
#[derive(Clone, Copy)]
pub struct CostVector {
    z_height_mm: Finite,
    print_time_s: Finite,    // Σ layers (exposure + lift) over the LayerPlan — NOT z alone
    resin: Resin,            // private Finite magnitudes: { model_ml, support_ml, trapped_ml }
    scar: Scar,              // private Finite magnitudes: { area_mm2, delicacy_weighted, worst_patch_mm2 }
    risk: Risk,              // private validated fields: { max_effective_x, zipper_arms, min_lateral_spread_mm }
    fidelity: Fidelity,      // Surrogate | ProxyRefined | Exact — a cost always knows how much it lied
}
pub struct OrientationCost { orientation: OrientationId, cost: CostVector }
pub struct ParetoPoint { orientation: OrientationId, cost: CostVector }
pub struct OptimizationSummary {
    selected: OrientationCost,
    pareto: Vec<ParetoPoint>,
    candidates_examined: u64,
}
impl OptimizationSummary { pub fn new(/* all fields */) -> Result<Self, OptimizationError> { /* validate below */ } }
pub enum SearchEvidence {
    NotEvaluated,
    Evaluated(OptimizationSummary),
}

// plinth-cost — behaviour over the shared vocabulary.
pub trait CostProvider {
    fn surrogate(&self, batch: &CandidateBatch, ctx: &Ctx) -> Vec<PartialCost>;  // batched, no mutation
    fn realize(&self, cand: &Candidate, ctx: &Ctx) -> Result<(PartialCost, StageOutput), Defect>;
}
pub trait Evaluator { /* ordered map-collect; CpuEvaluator (rayon) now, WgpuEvaluator later */ }
```

`CostVector`, `Resin`, `Scar`, and `Risk` keep their numeric fields private. Their fallible
constructors reject non-finite values and reject negative physical quantities; their custom
deserializers route through those constructors. Read-only accessors expose the validated values.

`OptimizationSummary::new` (also used by custom deserialization) rejects an empty Pareto set,
zero `candidates_examined`, or a candidate count smaller than `pareto.len()`. Pareto orientation
IDs must be unique, and the selected `(OrientationId, CostVector)` pair must match exactly one
Pareto point — not just the same ID or an approximately equal cost. This prevents a report from
claiming an unevaluated or internally inconsistent winner.

Print time is `Σ layers (exposure + lift)` over the `LayerPlan`, not z-height — it reduces to
z-height only under a constant plan, which is precisely why `LayerPlan` is in the CostVector's path
from day one.

`SearchEvidence::NotEvaluated` is the only valid report state before orientation evaluates a
candidate. Never fabricate a sentinel cost or an empty-but-claimed Pareto result; after evaluation,
`SearchEvidence::Evaluated` carries an `OptimizationSummary` with a mandatory selected typed
orientation, Pareto points, and candidate count.

**Scalarization weights are `Placeholder`** — and because they are Placeholder they *physically
cannot* reach an accept decision. Search weights select; they never accept. Acceptance is the
DefectReport, never a score.

**Rule of two** governs abstraction: no trait exists without two real implementors
(`Surrogate`/`Exact` for `CostProvider`; `Zipper`/`Additive`; CPU/pinned for search). Named
exemptions: `SolidBoolean` and the future `Sink`, whose second implementors are roadmap.

---

## 4. Control flow: `plinth prepare --input-models-dir DIR --output PRINTER`

**One entry point.** `plinth-cli` has no second path that reaches a stage directly. There is no
flag that silently disables orientation (the Python `hollow=False` path that skipped orientation is
dead), no `getattr`-style config defaults (typed config only), and no signature reflection.

```
for each model in DIR (rayon across models at the outer level):
 0. LOAD + TRIAGE   Model<Raw> -> Model<Triaged>       plinth-mesh (explicit WeldPolicy; shard strip;
                                                        pre-hollowed refusal; canonical fingerprint)
 1. BUILD ONCE      BVH(exact) · FaceTable · topology + cut flags · EDT/thickness · delicacy
                    · ProxyLadder (QEM, error-bounded)  — ALL orientation-invariant; cached
 2. DECIMATE        Model<Triaged> -> Model<Decimated>  plinth-decimate (production decimation)
 3. ORIENT (search) Model<Decimated> -> Model<Oriented> plinth-orient — §6
 4. HOLLOW + DRILL  Model<Oriented> -> Model<Hollowed>  plinth-hollow + plinth-voxel + plinth-solid
                    (eligibility, cavity, drains/vents, priority-flood drainability, internal supports)
 5. SUPPORT         Model<Hollowed> -> Model<Supported> plinth-support (internal fixed-point solver)
 6. EMIT            synthesize solids -> BOOLEAN UNION -> write .chitubox     plinth-emit
 7. AUDIT           re-read the file FROM DISK; structural + geometric gates; NO repair pass
 8. REPORT          one DefectReport; exit code is a projection of it
```

**Caching** (`plinth-pipeline`): a content-addressed store keyed by
`H(stage_id ‖ input_digest ‖ config_digest ‖ code_digest)`. Generic numeric digests reject
non-finite inputs and hash finite raw `f64` bit patterns, including the sign of zero; only the
future mesh-fingerprint boundary applies the §3.1 `-0.0 → 0.0` policy. A cache hit is by
construction identical to a miss; CI runs the corpus cache-disabled and requires identical reports.

**Rayon placement** — exactly one level active at a time, chosen by batch size: across models;
across orientation candidates in the coarse sweep; or across faces/rays/members inside a kernel
batch call. Inside the support solver the only sound pattern (adjudicated in
[../reference/port_plan_codex_review.md](../reference/port_plan_codex_review.md), binding) is:
**serial generate → parallel evaluate against one immutable snapshot → serial commit of exactly one
winner.** Independent-tree parallelism and speculative whole-sweep replay were ruled unsound and are
not resurrected.

**Determinism rules (all binding):**

- ordered map-collect only; reductions fold serially in index order; never `par_iter().sum()`;
- explicit tie-break keys everywhere (lowest pool row, lowest face id, sorted row tuples);
- arithmetic whose form is pinned by parity stays pinned (`x*x + y*y + z*z`, never a reassociating
  reduction — the einsum lesson: 3,275 divergences in 240,000);
- **no wall-clock budgets anywhere**; every cap is a deterministic work-unit budget
  (`config.py`'s `rewire_time_cap_s: float = 30.0`, line 580, dies with no legacy timing mode
  retained — a wall-clock cap makes the baseline itself non-reproducible);
- CI runs the corpus at 1, 4 and 16 threads and diffs artifact digests.

**The repair loop, redesigned.** Python rebuilds the entire forest per repair attempt (up to 9 full
construct + rewire + brace passes) — the largest term in the 86–872 s per-model runtime (median
~402 s) and the direct reason orientation search was cut to 13 candidates. In Rust:

1. gather all Defects → group by kind;
2. `response_for(kind)` proposes a **batch** of candidate edits;
3. evaluate all against one immutable snapshot (batched, parallel);
4. commit the single lexicographic-best **serially**, under a strict-decrease rule;
5. invalidate only the affected members' certificates;
6. incrementally re-verify the dirty set plus its clearance neighbourhood.

The certificate/dirty-member design in `scripts/support_placement/tree.py::Forest.revalidate`
(line 389) is the right shape — keep it. Incremental re-verify is checked against full re-verify on
a fixture in CI.

Phase-8 relocation remains a distinct final-only pass invoked once after the main loop reaches its
fixed point (joint-ball legality depends on final shaft geometry), but unified across stub and strut
via a `MemberKind` parameter and driven by the responder table. The geometric **proofs are ported as
coded** — the joints enclosure certificate with its star-shapedness restriction and direct-witness
short-circuit, the frustum signed-distance reduction, and the interval/tip-anchor ordering law —
because the code contains post-review corrections the prose lacks. Only the orchestration
(sweep/round/retirement/ban bookkeeping) is re-derived from invariants.

**The interval ordering law, made structural:** the kernel offers **no pre-anchoring length filter
at all**. Penetration intervals are always full resolution; `MIN_ILLEGAL_LEN` is applied per
crossing strictly *after* tip anchoring, in exactly one function in `plinth-verify`. The
dual-certificate/boundary-divergence class (census item mi=445) cannot exist because there is one
implementation. Stubs get axis-only rays; non-stubs get axis plus 8 capsule-offset rays; tip
anchoring is per ray against that ray's own arc length.

---

## 5. How the optimization triad is served

| Goal | Mechanism in this architecture |
|---|---|
| **fast** | Orientation minimizes `print_time_s = Σ layers (exposure + lift)` over the `LayerPlan`; the future variable-height slicer thickens layers where the per-z detail field is sparse, using the same histogram the orientation surrogate already builds |
| **minimum resin** | Orientation drives hollowing quality and support count; `Resin { model_ml, support_ml, trapped_ml }` makes trapped (undrainable) resin a first-class cost term, so an orientation that traps resin loses |
| **high quality** | Orientation feeds dynamic layer thickness; the delicacy field in the `FaceTable` weights the CVT lattice so scars land on tolerant surfaces; total scar area is fixed at `A_face / X`, so the optimizer is told the truth — only X and scar *placement* are levers |

Every stage couples through **orientation**, which is why orientation is its own stage
(`plinth-orient`) and never a side effect of hollowing, and why the FaceTable's
orientation-invariance is the load-bearing performance decision (§3.3).

---

## 6. Orientation search

Replaces the Python 13 hand-picked rotations and its ad-hoc weighted objective
(`0.5·support_area + 3·peak_xsec + 0.3·height`). Three tiers, all batched:

1. **Coarse.** ~512 Fibonacci-sphere `g` directions + convex-hull stable-pose candidates + **the
   legacy 13 candidates** (so the sweep can never do worse than the old search by construction),
   scored on the L2 proxy by masked reductions over the FaceTable → `CostVector{Surrogate}`.
   Per-candidate work: rotate `g`; masked dot products for down-face area, scar and z-extent; one
   prefix-sum re-bin of the per-z detail histogram. The support-demand surrogate needs **no layout
   at all**: total scar is invariant at `A_face / X`, so demand = down-face area ÷ (A_face per arm
   at effective X), plus the delicacy-weighted placement penalty. The resin surrogate includes the
   **trapped-resin** term (cavity volume unreachable from a drain under `g`).
2. **Refine.** Top-K = 24 on L1 with a cheap real pass — coarse hollow + hex-lattice seat + X-rule
   size, no routing → `Fidelity::ProxyRefined`.
3. **Commit.** Top-3 realized at `Fidelity::Exact` (full hollow + seat + size); commit the
   scalarized best; record the **Pareto front** over `(z_height, print_time, resin, scar, risk)` in
   the report. The front costs nothing now and is what lets plating bolt on — plate print time is a
   max over models, so the plater needs the front, not a winner.

**Guard rails.** Commitment happens only at `Fidelity::Exact`. `--orient pinned:<baseline>` exists
from the vertical-slice milestone so orientation can be frozen to the Python baseline's choice while
other stages are compared — isolating "is the port correct" from "is the search better".
`plinth calibrate` publishes surrogate-vs-exact Spearman rank correlation and regret on the corpus.
Gates (initial thresholds are Placeholder provenance): the surrogate must place the exact-best
candidate inside its top-24 or K doubles; Spearman ≥ 0.7 or the sweep is demoted.

**How far joint optimization goes now: one-way coupling only.** Orientation is the single free
global variable; everything downstream is scored at surrogate fidelity as a function of it, and the
committed orientation flows forward once at full fidelity. There is **no inner-loop
orientation↔support co-optimization** in this migration. The seam for later is already in place
(`CostProvider`, Pareto output, cache, `Fidelity`), and the measured decision point is written down:
if calibration at the search-quality milestone shows the surrogate cannot rank (exact-best outside
top-48 after one K doubling), the fallback ladder is **(a)** a fixed candidate set (one
`impl Search`, architecture unharmed), then **(b)** couple orientation into the support solver's
outer loop — in that order.

---

## 7. Hollowing and drainability

`plinth-voxel` is the voxel authority at explicit, provenance-typed pitch constants: voxelize/fill,
signed EDT for wall thickness, largest-component cavity, per-lobe drains and a top vent,
**priority-flood drainability** with audit drains or refill-to-solid (`CavityNotDrainable` has a
real fixer). Marching-cubes builds the interior shell; the cavity difference goes through
`plinth-solid`.

The Python step-8 self-checks (single body, watertight, winding, volume balance, cavity breathes)
become `plinth-verify` gates evaluated on the artifact **as re-read from disk**, and the flood-fill
outside seed is **asserted exterior** rather than assumed to be at grid corner `[0,0,0]`.

Internal-cavity support planning (A5, `scripts/support_placement/internal.py`) lives here, batched:
no per-keepout dilation loops, no per-member clearance loops.

---

## 8. Boolean solids

`plinth-solid` defines `trait SolidBoolean` (union, difference) plus a watertight/winding/shell
audit of any solid. The first backend is vendored C++ **manifold** through `cxx` FFI. This is the
**named-in-writing blocker** to the pure-Rust expectation: no mature pure-Rust f64 manifold-grade
boolean kernel exists, and this project was burned four times by coincident coplanar faces produced
by naive triangle-soup concatenation of touching primitives (fixed four times at individual sites
before being fixed at source).

Bounds on the override: C++ FFI carries none of the agent-productivity cost that motivates Python
elimination; the backend sits behind the trait so a native Rust kernel can replace it without
touching `plinth-emit`; and the disk re-read audit gates the output regardless of backend, so a bad
kernel is detectable, never silent. Spike the FFI in the first milestone — it is the single largest
schedule risk.

---

## 9. Emit and the on-disk audit

All support-solid synthesis (frustum, sphere, pad, bridge, MST raft topology, root-boss sizing)
moves from `scripts/support_placement/chitubox_emit.py` into `plinth-emit`, computed in f64 and cast
to f32 only at the record encoder.

Winding is decided by a native f64 signed-tetrahedron-volume sum, validated against the
**native ChituBox ground-truth** fixtures in `scripts/tests/fixtures/chitubox/`: the three
UI-authored specimens `basic-cube-supports-raft.chitubox`,
`cylinder-supported-bottom-connected.chitubox` and `two-supports.chitubox`, plus the original
`morel_mudskipper_supports.chitubox` (243 records). **Only the first two carry native PAD/BRIDGE
(type-6) geometry**, so they are the only valid oracle for pad and bridge synthesis. The
regenerated-by-this-library fixtures in that directory — the five `sphere-*`/`cylinder-*`
specimens **and `pad_bridge_donor.chitubox`** — are **circular for writer bugs** and serve only as
round-trip tests, never as ground truth. See
[../../scripts/tests/fixtures/chitubox/README.md](../../scripts/tests/fixtures/chitubox/README.md)
and `05-chitubox-and-corpus.md`.

**Emitted solids are boolean-unioned before write.** The audit then re-reads the artifact from disk
and fails on boundary edges, negative shells, inconsistent winding, and container-structure
violations (`chitubox_io`'s independent interval/pointer audit in `rust/chitubox_io/src/verify.rs`).

The frame invariant `zmin_centered − plate_z == lift_mm` to 1e-6 mm, recomputed from a freshly
reloaded STL, is preserved verbatim. The two conflicting `lift_mm` defaults die — `config.py` line
235 defaults `lift_mm = 5.0` while `scripts/tools/prep_one.py` line 71 defaults `--lift-mm` to
`3.0`; the Rust code has one typed constant and one consumer.
`scripts/support_placement/chitubox_inject.py`'s write/write_replace path is not ported.

---

## 10. The support solver

Artifact-boundary purity is right **around** the solver and wrong **through** it. Seat, size, route,
rewire, brace and repair are one fixed-point solver over one arena: a `Forest` with generational
indices, and checkpoint/rollback represented as data (the Python append-only constraint is lifted).

- Radius-end conventions (`r0` = entry / terminal-ward, `r1` = exit / root-ward) are **distinct
  newtypes** — the Murray B2-class bug (wrong end, 2.3× oversize) becomes unrepresentable.
- Murray verification filters dead members. `verify.murray_report` in Python sums dead children's
  radii — a live bug; the Rust gate fixes it, with a named regression test.
- Xia angles, Weiszfeld polish, Murray propagation, CCO cheapest-first insertion, rewire moves, and
  activation-aware transient sizing (sorted by `active_z`, clamped, overlap gate required, N^0.125
  exponent) port with their pinned semantics. Sizing returns **new radius arrays** rather than
  mutating what verify measures.
- One generic dyadic sample-builder replaces the two near-duplicate Python implementations
  (candidate pool + coverage samples).
- Specialized-contact consolidation is redesigned batched, with **one** acceptance predicate.
- Concession supports (the P8-REACH Feature B standoff ladder) are **not ported** initially — the
  port aims to need fewer concessions. If the parity gate then shows a corpus model unpassable
  without them, that is an owner decision point recorded in the gate table (add the ladder as a
  fixer, or accept the model's failure), never a silent re-implementation.

---

## 11. Concurrency now, GPU later

**Now: rayon.** One parallel level active at a time (§4), ordered map-collect only, serial folds,
deterministic tie-breaks, work-unit budgets instead of wall clocks, thread-count invariance proven
in CI at 1/4/16 threads.

**Later: wgpu.** Add `WgpuEvaluator: Evaluator` and a wgpu `SurfaceOracle` implementation; both must
pass the same differential suites as the CPU path. Determinism policy: **GPU proposes, CPU
disposes** — the GPU may rank at `Fidelity::Surrogate`, but anything committed is re-scored on the
deterministic CPU path. Bit-determinism of the accept path is never delegated to a driver.
Batch-first is exactly why this is a drop-in rather than a rewrite.

---

## 12. Roadmap bolt-on points (no restructuring)

| Future capability | Bolt-on point |
|---|---|
| **Plating** | New `plinth-plate` crate: a search over `(model → Pareto point, position, z-spin)` under a plate-level CostVector where print time is a **max**. Consumes the orientation Pareto front §6 already records, plus `chitubox_io`'s existing placement/rotate APIs. The only new geometry is 2D footprint packing |
| **Dynamic-height slicing** | New `plinth-slice` plus a `CostProvider` that produces a `LayerPlan` from the per-z detail field the orientation surrogate already builds. `LayerPlan::constant` simply stops being the only constructor |
| **Sending to the printer** | `trait Sink { fn deliver(&self, artifact: &Path) -> Result<(), DefectReport> }`; disk write is today's only implementation, and `--output PRINTER` already resolves a `PrinterProfile` |
| **GPU** | §11 |

The end-state CLI is `plinth --input-models-dir DIR --output PRINTER` doing prep → decimate →
orient → hollow/drill → support → plate → slice → send. This migration's scope stops at emitting
per-model `.chitubox` files with supports and rafts; the table above is why the remainder needs no
restructuring.

---

## 13. Relationship to the existing Rust crates

Both existing crates keep their role and their paths. Neither is rewritten.

**`rust/geom_kernel`** — deterministic BVH and surface-exact batched queries; already the ported
`geometry_exact.py`. Its `Cargo.toml` already declares `crate-type = ["cdylib", "rlib"]`, so linking
it from workspace crates needs no manifest change. Existing batched entry points on
`NativeSurface` (`rust/geom_kernel/src/queries.rs`) include `query`, `project`, `inside`,
`cast_rays`, `segment_clearance`, `penetration_intervals_batch`, `sphere_breach` and
`exterior_face_mask` — all batch-in/batch-out, which is the shape the new crates consume.

Changes to make: add a `trait SurfaceOracle` so the wgpu backend and the CPU backend are
interchangeable, and add point-kNN and radius-pair queries for the lattice/CVT work. Its
determinism checklist (index-parallel map-collect only, no rayon float reductions, no HashMap
iteration feeding results, no wall-clock reads in the kernel) at the top of
`rust/geom_kernel/src/lib.rs` is **binding on all new code**.

The PyO3 surface lives **entirely inline in `rust/geom_kernel/src/lib.rs`** — there is no `py.rs`
in this crate. It is the `use numpy::…`/`use pyo3::…` imports (lines 31-35), the `#[pyclass]`
`NativeSurface` wrapper and its `#[pymethods]` block (line 70ff), the `#[pymodule]` for
`geom_kernel._native` (line 347) and the `#[pyfunction] current_threads` (line 356). `queries.rs`
holds the pure-Rust `NativeSurface` methods (`query`, `project`, `inside`, `cast_rays`,
`segment_clearance`, `penetration_intervals_batch`, `sphere_breach`, `exterior_face_mask`) and
carries no PyO3 attributes at all. Note also that `rust/geom_kernel/Cargo.toml` has **no
`[features]` block today**: `numpy` and `pyo3` are unconditional dependencies, which is why M2
makes them optional before M9 can strip the code. All of it — the inline surface and the
`rust/geom_kernel/python/` shim directory — is deleted in the final milestone, along with the
`numpy`/`pyo3` dependencies and the `cdylib` crate type.

**`rust/chitubox_io`** — `.chitubox` container read/write plus an independent structural audit
(`src/verify.rs`). `plinth-emit` writes through it; `plinth-metrics` may link its **reader** (§14
D-19). Its PyO3 layer is `rust/chitubox_io/src/py.rs` plus the `rust/chitubox_io/python/` shim
directory; both are deleted in the final milestone.

Removal of PyO3 happens only once nothing consumes it — i.e. in the same final milestone that
deletes `scripts/` Python, `pyproject.toml`, `uv.lock` and `.venv`, after the parity gates pass.

---

## 14. Quality-gate harness (summary)

Operational detail lives in `04-migration-plan.md` and `05-chitubox-and-corpus.md`. The shape:

`plinth-metrics` + `plinth score` reads a `.chitubox` and its source/prepped STL **from disk**
(never in-memory state, never a repair pass) and computes the pre-committed metric set: island
count, closed cavities, watertightness / boundary-edge count, negative shells, winding consistency,
X-rule load compliance (arm-group effective X under the resolved `LoadSharing`), support count,
resin volume (model / support / total), scar area + worst patch + delicacy-weighted placement,
z-height, and ChituBox loadability (`chitubox_io` round-trip plus the record-structure byte
contract).

Directions of goodness are pre-committed **before anyone knows which side wins**. The gate: Rust is
as-good-or-better on **every** metric, per model, with a written waiver protocol (the owner may sign
a named regression into the gate table with the reason recorded).

**Independence is mandatory:** `plinth-metrics` links neither `plinth-verify` nor `geom_kernel` —
its geometry is its own slow brute-force f64 implementation. A kernel bug must not corrupt producer
and judge together; this independence is the property that caught census item mi=445. Slow is fine.
The one exception is `chitubox_io`'s reader, because reimplementing the container parser would be
waste and the native ChituBox fixtures (§9) anchor it non-circularly.

Per-milestone owner-eyeball checkpoints are **named artifacts, not vibes**: Pareto sheets, scar heat
maps, per-model before/after tables (support count, resin, z-height), and ChituBox load tests.

---

## 15. Supersessions — read these before trusting older repo docs

You will read the old documents cold. These clauses are dead:

| Document | Superseded clause | Still binding |
|---|---|---|
| [../reference/port_plan.md](../reference/port_plan.md) | its "stay Python" verdict (owner ruling 2026-07-23 and this guide) | its determinism/parallelism discipline |
| [../reference/port_plan_codex_review.md](../reference/port_plan_codex_review.md) | — | all of it, especially the solver-parallelism ruling (§4) and the wall-clock-budget finding (§4) |
| [../specs/SPEC-RUST-KERNEL.md](../specs/SPEC-RUST-KERNEL.md) | §6's claim that the Python `ExactSurface` stays "permanently" as differential oracle; its PyO3-hybrid scope | its numerical-hazard notes (tie-breaking, consequence-based parity) |
| The Python CLI exit-code/JSONL contract (`scripts/tools/prep_one.py` et al.) | entirely — replaced by the DefectReport-projection contract; the Python tests that pin it retire with their subject | — |
| `skills/*/SKILL.md` | they describe the CURRENT interactive Blender/Python workflow, not the target; marked superseded at the final milestone | the algorithms they document, notably decimate-for-print's error-budget QEM staging |
| The Python topology/geometry digests | byte-parity with them is explicitly not attempted; schema-bumped and redefined over topology arrays | the `.chitubox` record-structure byte contract, which is unaffected |

The differential-oracle role that `SPEC-RUST-KERNEL.md` §6 assigned to Python transfers to the
frozen baseline artifacts plus `plinth-metrics`' independent implementation.

---

## 16. Decisions and why (condensed)

This architecture was synthesized from two adversarial proposals — **A**, a staged-pipeline lens,
and **B**, a global-optimization lens. Every disagreement was adjudicated; the rulings below are
recorded so they are not re-litigated.

| # | Question | Ruling and reason |
|---|---|---|
| D-1 | Pipeline of staged functions (A) vs one optimizer with orientation as decision variable (B) | **A's staged spine with B's data model.** The spine is cheap to reason about; search-shaped types (CostVector, Fidelity, Pareto output, orientation-invariant FaceTable, per-layer LayerPlan) are expensive to retrofit. Take both |
| D-2 | Crate layout | Constants folded into `plinth-core`; `plinth-decimate` separate (maps 1:1 to a milestone and to "convert, don't reinvent"); perception split between `plinth-mesh` and `plinth-support`; `plinth-cost` and `plinth-pipeline` adopted; no `plinth-calib` crate (`plinth calibrate` is a subcommand); no `xtask` crate; one independent implementation in `plinth-metrics`, not two |
| D-3 | Bypass-path prevention | **Typestate `Model<Stage>`** (A) — the Python bypass-emit defect becomes a compile error, not a convention — plus B's owned-value discipline and `Validated<T>` newtype replacing `prepared_validation.py` |
| D-4 | Orientation-invariant FaceTable | **Adopted from B verbatim.** It makes the dense sweep arithmetic rather than aspiration, and structurally fixes scoring scar on a different mesh than the delicacy field |
| D-5 | Source of the surrogate's support-demand term | **B**: total scar invariance (`A_face / X`) needs no layout. A's per-seat volume estimate appears at the refine tier, where a layout exists |
| D-6 | Provenance mechanism | **B's wrapper types + `AcceptSafe`.** A's const-fn gating depends on const-eval panics being reached and degrades to a runtime panic for non-const construction; trait-absence is unconditionally compile-time. Kept from A: the registry golden test. Added: a `trybuild` compile-fail test — a mechanism without a test of the mechanism is a promise |
| D-7 | `Pending` shape | **B**: carries both conservative and optimistic values, with `.resolve()` defaulting conservative. Forces both plate-6 outcomes to be written down |
| D-8 | LoadSharing representation | **Enum** (A) — two variants decided by a measurement, exhaustively matchable, serializable. B's own rule-of-two would flag a two-impl trait as ceremony. B's operational points kept: `effective_x` is an input to lattice density; `plinth calibrate` reports the A/B delta before any switch |
| D-9 | ViolationKind set | Union of both, plus `ArrangementInstability` and `CavityNotDrainable` (B), keeping A's `Emit*`/`ChituboxLoadability` kinds; `ProvenanceViolation` dropped (provenance is compile-time; a runtime kind would imply the compile-time story is incomplete); `RepairCapExhausted` double-booked — named in `Termination` **and** injected as a Defect |
| D-10 | `#[non_exhaustive]` on ViolationKind | **Rejected.** It forces `_` arms in other crates, and the responder table lives in `plinth-support` — the attribute would structurally defeat the exhaustive-match commitment |
| D-11 | Responder table shape | **A's `Repair`/`Terminal`.** Some kinds genuinely have no sound in-loop fixer; B's shape would force fake fixers. Terminal abuse is guarded by the fixture discipline in §3.4 |
| D-12 | Orientation funnel | **B's three tiers** with fixed numbers (512 Fibonacci + hull poses + the legacy 13; top-24; top-3), because the third cheap tier is what the FaceTable makes nearly free. From A: rank-correlation tracked with K-widening. Added: including the legacy 13 makes the sweep never-worse by construction. Pareto output and `--orient pinned:` adopted |
| D-13 | How far joint optimization goes now | One-way coupling, with a written measured ladder: exact-best outside top-48 after one K doubling → (a) fixed candidate set, then (b) couple orientation into the support solver. Both proposals' concessions become the fallback order |
| D-14 | Milestone order | **B's ordering** — solid-FFI spike first (largest schedule risk), metrics before any porting, thin vertical slice early (it closes the format contract, the audit, the DefectReport and the one-CLI invariant at once). From A: emit's native-fixture validation regime, and verify-before-repair as a hard dependency |
| D-15 | Phase-8 fidelity | Redesign authority covers **orchestration only**; the geometric proofs are ported **as coded**, because the code carries post-review corrections the prose lacks. Strengthened: the kernel offers no pre-anchoring length filter at all |
| D-16 | Where `transient.py` lives | `plinth-support` — it mutates radii, so it is sizing, not verification. Added rule: sizing returns new arrays and never mutates what verify measures |
| D-17 | `removal.py` | Ported as an **opt-in diagnostic**, never on the accept path (it is inert-by-charter in Python yet computed on every verify — dead compute the port drops) |
| D-18 | Mesh fingerprint semantics | **Canonicalize `-0.0`, reject non-finite as a Defect** — the raw-bit weld is a documented hazard chain. A's digest schema bump (topology arrays only, no allocator archaeology) also adopted |
| D-19 | Metrics-harness dependencies | Stricter than either proposal: **no `plinth-verify` and no `geom_kernel`** — a kernel bug must not corrupt producer and judge together. Exception: `chitubox_io`'s reader, anchored non-circularly by the native fixtures (§9) |
| D-20 | CLI contract | Exit code is a **projection of the DefectReport**; the pinned Python exit-code/JSONL contract dies with the Python, and the tests that pin it retire with their subject |
| D-21 | Corpus location | Corpus STLs are **copied into the repo** at `corpus/models/`, baselines at `corpus/baseline/`, both committed. The source models currently live in the repo's parent directory (`scripts/tools/_toolpaths.py` probes `REPO_ROOT/..`), which the executing agent cannot reach; without the copy, the baseline freeze and every gate are unexecutable |
| D-22 | `scripts/tools/geometry_sweep.py` | **Deleted, not ported** — Blender-only manual degenerate-geometry repair; triage plus cut flags refuse what it repaired. Recorded so it is not treated as unported scope |
| D-23 | Cache ownership | Cache in `plinth-pipeline`; per-search memoization in `plinth-cost`; a cache-disabled CI run must produce identical reports |
| D-24 | Print-time term | `Σ layers (exposure + lift)` over the LayerPlan, not z-height — it reduces to z-height only under a constant plan, which is why LayerPlan is on the cost path from day one |
| D-25 | Scalarization weights | **Placeholder**, therefore structurally barred from accept. Typing tuning weights as accept-safe is precisely what the provenance commitment exists to prevent |
| D-26 | Concession supports (P8-REACH Feature B) | **Not ported** initially. If the parity gate shows a model unpassable without them, that is a recorded owner decision point, not a silent re-implementation |
| D-27 | Murray dead-member filter | `verify.murray_report` sums dead children's radii — a live Python bug. The Rust gate filters dead members, with a named regression test |

**Conscious overrides of standing commitments:**

- **O-1 — pure-Rust expectation:** C++ `manifold` via FFI in `plinth-solid` (§8). The named-in-writing
  blocker the mission clause requires, bounded by the trait, the audit and the FFI's lack of
  agent-productivity cost.
- **O-2 — byte-parity with Python digests/fingerprints:** abandoned (§3.1). Does not touch the
  `.chitubox` record-structure contract.
- **O-3 — `SPEC-RUST-KERNEL.md` §6 "the Python `ExactSurface` stays permanently":** overridden (§15).
- **O-4 — wall-clock budgets:** banned outright (§4), with no legacy timing mode retained. A
  wall-clock cap makes the baseline itself non-reproducible.
