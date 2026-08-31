//! The declared inputs and the shared per-model context every check reads.
//!
//! **Everything expensive is built once and shared.** The rasteriser, the
//! demand set, the ownership partition and the seat reconstruction are lazily
//! built here and handed to every check by reference. A check that recomputes
//! one of them is grading a different solid from its neighbours, which is
//! exactly the class of defect this audit exists to find.
//!
//! Every lazily-built product returns a `Result`. **An error is never absorbed
//! into a pass**: the caller must report the named reason and fail.

use crate::consts as c;
use crate::graph::SupportGraph;
use crate::json::Json;
use crate::mesh::Mesh;
use crate::ownership::{self, ContactSite, DemandSet, Ownership, SampleBudget};
use crate::primitives::{GeomError, Primitive};
use crate::raster::{Budget, RasterError, RasterGrid, RunRaster};
use crate::reader::{ChituboxFile, ModelEntry};
use crate::seat::{self, Seat};
use crate::sidecar::Sidecar;
use std::cell::OnceCell;
use std::path::Path;

// ---------------------------------------------------------------------------
// Declared inputs
// ---------------------------------------------------------------------------

/// Where a declared input came from. Echoed in every output document, because
/// a result that does not say what constants produced it cannot be re-read in
/// six months (`audit-invariants.md` §5 rule 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    Measured,
    Derived,
    Placeholder,
    OwnerRuling,
}

impl Provenance {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "Measured",
            Self::Derived => "Derived",
            Self::Placeholder => "Placeholder",
            Self::OwnerRuling => "OwnerRuling",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DeclaredInput {
    pub name: &'static str,
    pub value: f64,
    pub units: &'static str,
    pub provenance: Provenance,
    pub source: &'static str,
}

/// The eight declared inputs of `audit-invariants.md` §1.3. The settings blob
/// is unmapped and all-zero in this corpus, so the print process parameters
/// are not in the file: they are supplied here, recorded verbatim in the
/// output, and **an absent one fails the file** -- the audit never substitutes
/// a guess.
#[derive(Clone, Debug)]
pub struct DeclaredInputs {
    pub pixel_pitch_x_mm: f64,
    pub pixel_pitch_y_mm: f64,
    pub layer_height_mm: f64,
    pub default_contact_depth_mm: f64,
    pub ball_factor_stub: f64,
    pub x_allowed: f64,
    pub tip_radius_min_allowed_mm: f64,
    pub tip_radius_max_mm: f64,
}

impl Default for DeclaredInputs {
    fn default() -> Self {
        Self {
            pixel_pitch_x_mm: c::PIXEL_PITCH_X_MM,
            pixel_pitch_y_mm: c::PIXEL_PITCH_Y_MM,
            layer_height_mm: c::LAYER_HEIGHT_DEFAULT_MM,
            default_contact_depth_mm: c::DEFAULT_CONTACT_DEPTH_MM,
            ball_factor_stub: c::BALL_FACTOR_STUB,
            x_allowed: c::X_ALLOWED,
            tip_radius_min_allowed_mm: c::TIP_RADIUS_MIN_ALLOWED_MM,
            tip_radius_max_mm: c::TIP_RADIUS_MAX_MM,
        }
    }
}

/// Why a declared input was refused. Every variant maps to
/// `FILE_DECLARED_INPUT_MISSING`.
#[derive(Clone, Debug)]
pub struct InputRejection {
    pub name: &'static str,
    pub reason: &'static str,
    pub value: f64,
}

impl DeclaredInputs {
    /// The rows of the output's `declared_inputs` object, with provenance.
    #[must_use]
    pub fn rows(&self) -> Vec<DeclaredInput> {
        vec![
            DeclaredInput {
                name: "pixel_pitch_x_mm",
                value: self.pixel_pitch_x_mm,
                units: "mm",
                provenance: Provenance::Measured,
                source: "PIXEL_PITCH_X_UM; evidence.rs:261-266; saturn-4-ultra-16k datasheet",
            },
            DeclaredInput {
                name: "pixel_pitch_y_mm",
                value: self.pixel_pitch_y_mm,
                units: "mm",
                provenance: Provenance::Measured,
                source: "PIXEL_PITCH_Y_UM; evidence.rs:154-159; saturn-4-ultra-16k datasheet",
            },
            DeclaredInput {
                name: "layer_height_mm",
                value: self.layer_height_mm,
                units: "mm",
                provenance: Provenance::Measured,
                source: "LAYER_HEIGHT_DEFAULT_MM; evidence.rs:210-220; project record, \
                         plate-06-results.md §1 -- NOT a vendor default",
            },
            DeclaredInput {
                name: "default_contact_depth_mm",
                value: self.default_contact_depth_mm,
                units: "mm",
                provenance: Provenance::OwnerRuling,
                source: "Owner ruling 2026-08-30: burial depth is not an adhesion requirement; \
                         down from the retired 0.300 mm. Bound to I2 -- contact area still \
                         governs and depth never overrides it",
            },
            DeclaredInput {
                name: "ball_factor_stub",
                value: self.ball_factor_stub,
                units: "1",
                provenance: Provenance::Placeholder,
                source: "BALL_FACTOR_STUB",
            },
            DeclaredInput {
                name: "x_allowed",
                value: self.x_allowed,
                units: "1",
                provenance: Provenance::Derived,
                source: "X_ALLOWED = floor(25.00/2); arm snaps at area ratio ~25, design at half",
            },
            DeclaredInput {
                name: "tip_radius_min_allowed_mm",
                value: self.tip_radius_min_allowed_mm,
                units: "mm",
                provenance: Provenance::Derived,
                source: "TIP_RADIUS_MIN_ALLOWED_MM; evidence.rs:354-358 (RADIUS, not diameter)",
            },
            DeclaredInput {
                name: "tip_radius_max_mm",
                value: self.tip_radius_max_mm,
                units: "mm",
                provenance: Provenance::Placeholder,
                source: "TIP_RADIUS_MAX_LEGACY_MM (RADIUS, not diameter)",
            },
        ]
    }

    /// Validate. `audit-invariants.md` §1.3 rejects a
    /// `default_contact_depth_mm` above 0.300 mm or below `layer_height_mm`
    /// with reason `out_of_ruled_range`: below one layer the seat is not
    /// resolvable, above the retired value the caller is loosening a ruling
    /// rather than configuring an instrument.
    pub fn validate(&self) -> Result<(), Vec<InputRejection>> {
        let mut bad = Vec::new();
        for r in self.rows() {
            if !r.value.is_finite() || r.value <= 0.0 {
                bad.push(InputRejection {
                    name: r.name,
                    reason: "absent_or_nonpositive",
                    value: r.value,
                });
            }
        }
        if self.default_contact_depth_mm > c::DEFAULT_CONTACT_DEPTH_MAX_MM
            || self.default_contact_depth_mm < self.layer_height_mm
        {
            bad.push(InputRejection {
                name: "default_contact_depth_mm",
                reason: "out_of_ruled_range",
                value: self.default_contact_depth_mm,
            });
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(bad)
        }
    }

    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        for r in self.rows() {
            let mut e = Json::obj();
            e.set("value", Json::num(r.value));
            e.set("units", Json::str(r.units));
            e.set("provenance", Json::str(r.provenance.as_str()));
            e.set("source", Json::str(r.source));
            o.set(r.name, e);
        }
        o
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct AuditOptions {
    pub inputs: DeclaredInputs,
    pub raster_budget: Budget,
    pub sample_budget: SampleBudget,
    /// Override the reconstructed file class's endpoint tolerance. Leave
    /// `None`: a hand-picked tolerance is how a wrong graph gets built quietly.
    pub join_tol_override_mm: Option<f64>,
}

// ---------------------------------------------------------------------------
// File-level context
// ---------------------------------------------------------------------------

pub struct AuditContext<'a> {
    pub file: &'a ChituboxFile,
    pub options: &'a AuditOptions,
    pub sidecar: Option<&'a Sidecar>,
    /// Why the sidecar is absent, when it is. **Absence is never a pass**:
    /// a check that needs it must report `sidecar_absent` and fail.
    pub sidecar_absence: Option<String>,
}

impl<'a> AuditContext<'a> {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.file.path
    }
    /// The path as it appears in every failure line.
    #[must_use]
    pub fn path_str(&self) -> String {
        self.file.path.display().to_string()
    }
    #[must_use]
    pub const fn inputs(&self) -> &DeclaredInputs {
        &self.options.inputs
    }
}

// ---------------------------------------------------------------------------
// Per-model context
// ---------------------------------------------------------------------------

/// Everything a check gets. Cheap products are eager; expensive ones are built
/// on first use and then shared.
pub struct ModelContext<'a> {
    pub audit: &'a AuditContext<'a>,
    pub model_index: usize,
    pub model: &'a ModelEntry,
    /// The welded model mesh, exact-`f32`-bit welding per §1.4.
    pub model_mesh: Mesh,
    /// The plate in the model-centred frame, resolved by [`FrameCheck`] --
    /// RECONSTRUCTED from raft geometry where it exists. **Not simply
    /// `-translate_z`**: see [`FrameCheck`] for why the two specification
    /// documents disagree and which one the bytes support.
    pub z_plate_mm: f64,
    pub z_plate_source: PlateSource,
    /// The printer lattice for this model.
    pub grid: RasterGrid,
    /// One entry per support record, parallel to `model.records`. An `Err` is
    /// a degenerate record and is I8's raw material.
    pub primitives: Vec<Result<Primitive, GeomError>>,
    /// The reconstructed support graph.
    pub graph: SupportGraph,
    /// The seat reconstruction, one per type-7 record, in record order.
    pub seats: Vec<Seat>,

    demand: OnceCell<DemandSet>,
    ownership: OnceCell<Ownership>,
    support_raster: OnceCell<Result<RunRaster, RasterError>>,
    model_raster: OnceCell<Result<RunRaster, RasterError>>,
}

impl<'a> ModelContext<'a> {
    /// Build the context for one model. Cheap: welding, primitives, the graph
    /// and the seats. Rasters and the ownership partition wait until asked.
    #[must_use]
    pub fn build(audit: &'a AuditContext<'a>, model_index: usize) -> Self {
        let model = &audit.file.models[model_index];
        let model_mesh = Mesh::weld_exact(&model.model_soup);
        let primitives_early = model
            .records
            .iter()
            .map(Primitive::from_record)
            .collect::<Vec<_>>();
        let (z_plate_mm, z_plate_source) = resolve_plate(model, &model_mesh, &primitives_early);
        let grid = RasterGrid {
            pitch_x_mm: audit.inputs().pixel_pitch_x_mm,
            pitch_y_mm: audit.inputs().pixel_pitch_y_mm,
            layer_height_mm: audit.inputs().layer_height_mm,
            z_plate_mm,
        };
        let primitives = primitives_early;
        let join_tol = audit
            .options
            .join_tol_override_mm
            .unwrap_or_else(|| audit.file.file_class.join_tol_mm());
        let graph = SupportGraph::build(model, join_tol, audit.file.file_class.label());
        let seats = seat::reconstruct_seats(
            model,
            &model_mesh,
            &graph,
            audit.inputs().ball_factor_stub,
            audit.inputs().layer_height_mm,
            audit.inputs().default_contact_depth_mm,
        );
        Self {
            audit,
            model_index,
            model,
            model_mesh,
            z_plate_mm,
            grid,
            primitives,
            graph,
            seats,
            z_plate_source,
            demand: OnceCell::new(),
            ownership: OnceCell::new(),
            support_raster: OnceCell::new(),
            model_raster: OnceCell::new(),
        }
    }

    #[must_use]
    pub const fn inputs(&self) -> &DeclaredInputs {
        &self.audit.options.inputs
    }
    /// The file path as every failure line spells it.
    #[must_use]
    pub fn file(&self) -> String {
        self.audit.path_str()
    }
    /// The seat of a contact by its `contact_id` (its TIP record index).
    #[must_use]
    pub fn seat(&self, contact_id: usize) -> Option<&Seat> {
        self.seats.iter().find(|s| s.contact_id == contact_id)
    }
    /// The analytic solid of a record, or the reason it has none.
    pub fn primitive(&self, record: usize) -> Result<&Primitive, GeomError> {
        self.primitives[record].as_ref().map_err(|e| *e)
    }
    /// Every record that yielded a usable solid, with its index.
    #[must_use]
    pub fn live_primitives(&self) -> Vec<(usize, Primitive)> {
        self.primitives
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().ok().map(|p| (i, *p)))
            .collect()
    }

    /// The demand set `D` of §2.1. Built once.
    pub fn demand(&self) -> &DemandSet {
        self.demand
            .get_or_init(|| DemandSet::build(&self.model_mesh))
    }

    /// The contacts as the ownership partition needs them, routed to the
    /// surface component of their own seat entry triangle.
    #[must_use]
    pub fn contact_sites(&self) -> Vec<ContactSite> {
        self.seats
            .iter()
            .map(|s| ContactSite {
                contact_id: s.contact_id,
                p0: s.p0,
                component: s.entry_component,
            })
            .collect()
    }

    /// The complete Voronoi partition of §2.2. Built once, shared by I2, I3
    /// and I4. Check `Ownership::budget_exceeded` before using the numbers:
    /// a `Some` there is `sample_budget_exceeded` and FAILS.
    pub fn ownership(&self) -> &Ownership {
        self.ownership.get_or_init(|| {
            let d = self.demand();
            ownership::partition(
                &self.model_mesh,
                d,
                &self.contact_sites(),
                &self.audit.options.sample_budget,
            )
        })
    }

    /// `A_owned(c)` for a contact id, mm^2. `None` when the contact is not in
    /// the site list at all.
    #[must_use]
    pub fn a_owned_mm2(&self, contact_id: usize) -> Option<f64> {
        let sites = self.contact_sites();
        let idx = sites.iter().position(|s| s.contact_id == contact_id)?;
        self.ownership().a_owned_mm2.get(idx).copied()
    }

    /// The cured raster of the support solid `S`, built from the ANALYTIC
    /// primitives (§1.4) on the printer lattice, full containment.
    pub fn support_raster(&self) -> Result<&RunRaster, &RasterError> {
        self.support_raster
            .get_or_init(|| {
                let prims: Vec<Primitive> =
                    self.live_primitives().into_iter().map(|(_, p)| p).collect();
                RunRaster::from_primitives(&prims, self.grid, &self.audit.options.raster_budget)
            })
            .as_ref()
    }

    /// The cured raster of the model solid `M`.
    pub fn model_raster(&self) -> Result<&RunRaster, &RasterError> {
        self.model_raster
            .get_or_init(|| {
                RunRaster::from_mesh(
                    &self.model_mesh,
                    self.grid,
                    &self.audit.options.raster_budget,
                )
            })
            .as_ref()
    }

    /// The cured raster of one record's own solid. **Not cached**: it is a
    /// per-record product and a check that needs many should batch them.
    pub fn record_raster(&self, record: usize) -> Result<RunRaster, RasterError> {
        match self.primitives[record] {
            Ok(p) => RunRaster::from_primitives(&[p], self.grid, &self.audit.options.raster_budget),
            Err(_) => Ok(RunRaster::empty(self.grid)),
        }
    }

    /// The frame cross-check of `audit-invariants.md` §1.2, which must run
    /// before anything else. It is the one check that catches a mis-parsed
    /// lift, and on this corpus it caught a disagreement between the two
    /// specification documents themselves. See [`FrameCheck`].
    #[must_use]
    pub fn frame_check(&self) -> FrameCheck {
        let mut min_raft_z = f64::INFINITY;
        let mut min_support_z = f64::INFINITY;
        let mut lowest_support_record = None;
        for (i, p) in self.primitives.iter().enumerate() {
            let Ok(p) = p else { continue };
            let z = p.aabb().min[2];
            if self.model.records[i].kind == crate::reader::RecordKind::RaftPadOrBridge {
                min_raft_z = min_raft_z.min(z);
            } else if z < min_support_z {
                min_support_z = z;
                lowest_support_record = Some(i);
            }
        }
        let min_raft_z_mm = min_raft_z.is_finite().then_some(min_raft_z);
        let min_support_z_mm = min_support_z.is_finite().then_some(min_support_z);
        let mag = [
            min_raft_z_mm.unwrap_or(0.0).abs(),
            min_support_z_mm.unwrap_or(0.0).abs(),
            self.z_plate_mm.abs(),
        ]
        .into_iter()
        .fold(1.0f64, f64::max);
        let tolerance_mm = c::tol_geom_mag(mag);
        FrameCheck {
            z_plate_mm: self.z_plate_mm,
            source: self.z_plate_source,
            neg_translate_mm: -self.model.translate_mm[2],
            contract_relation_mm: (!self.model_mesh.is_empty())
                .then(|| self.model_mesh.bbox.min[2] - self.model.translate_mm[2]),
            min_raft_z_mm,
            raft_agrees: min_raft_z_mm.map(|z| (z - self.z_plate_mm).abs() <= tolerance_mm),
            min_support_z_mm,
            lowest_support_record,
            support_below_plate_mm: min_support_z_mm
                .map_or(0.0, |z| (self.z_plate_mm - z).max(0.0)),
            tolerance_mm,
        }
    }
}

/// Where `z_plate` came from. It is RECONSTRUCTED whenever the file carries
/// raft geometry, and only falls back to a declared relation when it does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateSource {
    /// `model_bbox.min.z - translate_z`, the relation `audit-contract.md`
    /// §1.5 measured on three files. DECLARED-derived.
    ContractRelation,
    /// `-translate_z`, the formula `audit-invariants.md` §1.2 states.
    /// DECLARED. Used only when neither of the above is available.
    NegativeTranslate,
}

impl PlateSource {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ContractRelation => "contract_relation_declared",
            Self::NegativeTranslate => "negative_translate_declared",
        }
    }
}

/// The plate-frame reconstruction and the two declared formulas it is checked
/// against.
///
/// **The two specification documents disagree here, and the audit reports the
/// disagreement rather than choosing quietly.**
///
/// * `audit-invariants.md` §1.2 says `z_plate = -translate_z`.
/// * `audit-contract.md` §1.5 measured, on three files, that the lowest
///   support `z` equals `model_min_z - lift`, which makes the plate sit at
///   `model_bbox.min.z - translate_z`.
///
/// On `exports/sphere-first-artifact.chitubox` the raft bottoms out at
/// `-14.9054537` mm, `-translate_z` is `-5`, and `bbox.min.z - translate_z` is
/// `-14.9054537`. The contract's relation is the one the bytes support, so the
/// audit uses raft geometry where it exists and the contract relation
/// otherwise, and it FAILS the file only when the raft and the contract
/// relation disagree -- which is the genuine mis-parsed-lift signal §1.2 was
/// reaching for.
#[derive(Clone, Copy, Debug)]
pub struct FrameCheck {
    /// The plate the audit used, in the model-centred frame.
    pub z_plate_mm: f64,
    pub source: PlateSource,
    /// `-translate_z`, the `audit-invariants.md` §1.2 formula. DECLARED.
    /// Measured on this corpus: it does NOT reproduce the plate.
    pub neg_translate_mm: f64,
    /// `model_bbox.min.z - translate_z`, the `audit-contract.md` §1.5
    /// relation, and the one the bytes support.
    pub contract_relation_mm: Option<f64>,
    /// Minimum `z` over type-6 raft geometry.
    pub min_raft_z_mm: Option<f64>,
    /// Does the raft sit on the plate? True on emitted files; FALSE on native
    /// ones, where the raft records carry `z = 0` while the rest of the
    /// support tree is in the model-centred frame. Reported as a FINDING, not
    /// gated, because the deviation is the file class's and not a defect in
    /// this model.
    pub raft_agrees: Option<bool>,
    /// Minimum `z` over every NON-raft support solid.
    pub min_support_z_mm: Option<f64>,
    pub lowest_support_record: Option<usize>,
    /// How far the lowest non-raft support reaches BELOW the plate, mm. Any
    /// positive value beyond tolerance is physically impossible and is the
    /// mis-parsed-lift signal this check exists to catch.
    pub support_below_plate_mm: f64,
    pub tolerance_mm: f64,
}

impl FrameCheck {
    /// `true` when the frame is established.
    ///
    /// The gate is physics, not a formula: **no support solid may reach below
    /// the plate.** That is what catches a mis-parsed lift, and unlike a
    /// raft-position identity it holds for native and emitted files alike.
    #[must_use]
    pub fn is_established(&self) -> bool {
        self.support_below_plate_mm <= self.tolerance_mm
    }
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("z_plate_mm", Json::num(self.z_plate_mm));
        o.set("source", Json::str(self.source.label()));
        o.set("neg_translate_mm", Json::num(self.neg_translate_mm));
        o.set(
            "contract_relation_mm",
            Json::num_or_null(self.contract_relation_mm),
        );
        o.set("min_raft_z_mm", Json::num_or_null(self.min_raft_z_mm));
        o.set(
            "raft_agrees",
            self.raft_agrees.map_or(Json::Null, Json::Bool),
        );
        o.set("min_support_z_mm", Json::num_or_null(self.min_support_z_mm));
        o.set(
            "lowest_support_record",
            self.lowest_support_record.map_or(Json::Null, Json::usize),
        );
        o.set(
            "support_below_plate_mm",
            Json::num(self.support_below_plate_mm),
        );
        o.set("tolerance_mm", Json::num(self.tolerance_mm));
        o.set("established", Json::Bool(self.is_established()));
        o
    }
}

/// Resolve the plate height in the model-centred frame.
///
/// Preference order, and each step says which of RECONSTRUCTED or DECLARED it
/// is: raft geometry (RECONSTRUCTED), then the contract's measured relation
/// `bbox.min.z - translate_z` (DECLARED-derived), then `-translate_z`
/// (DECLARED). The audit never silently averages them; [`FrameCheck`] reports
/// all three and fails when the reconstruction and the relation disagree.
fn resolve_plate(
    model: &ModelEntry,
    mesh: &Mesh,
    _primitives: &[Result<Primitive, GeomError>],
) -> (f64, PlateSource) {
    if mesh.is_empty() {
        return (-model.translate_mm[2], PlateSource::NegativeTranslate);
    }
    (
        mesh.bbox.min[2] - model.translate_mm[2],
        PlateSource::ContractRelation,
    )
}
