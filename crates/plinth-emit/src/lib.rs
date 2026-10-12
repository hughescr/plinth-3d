//! The single final ChituBox emission boundary.
//!
//! This crate is deliberately only a conversion boundary.  It turns the f64
//! [`plinth_support::SupportedModel`] into `chitubox_io`'s f32 wire records,
//! writes one fresh project, then audits a *newly opened* copy of that file.
//! It neither plans supports nor uses a model-bearing template.
#![forbid(unsafe_code)]

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, PlacementUpdate, Project, SegmentType, SupportBatch,
    SupportMode, SupportRecord, TriangleSoup,
};
use geom_kernel::vec3::{add, cross, dot, norm, scale, sub};
use geom_kernel::SoupTriangle;
use plinth_core::{m0_constants, PositiveFinite, PrinterProfile};
use plinth_mesh::Mesh;
use plinth_support::{FrustumRingNormal, SupportPrimitive, SupportSegmentKind, SupportedModel};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use thiserror::Error;

const MODEL_NAME: &str = "plinth-model";
const SPHERE_LATITUDES: usize = 8;
const SPHERE_LONGITUDES: usize = 16;

const _: () = assert!(SPHERE_LATITUDES >= 2 && SPHERE_LONGITUDES >= 3);

#[derive(Clone, Debug, PartialEq)]
pub struct EmitRequest {
    pub target_path: PathBuf,
    pub printer: PrinterProfile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitReport {
    pub artifact_path: PathBuf,
    pub support_record_count: usize,
    pub artifact_bytes: u64,
    /// What the seat trim did to the records that carry one. Defaulted when
    /// reading a report written before the trim existed. Boxed so the report
    /// stays small inside the per-model outcome enum.
    #[serde(default)]
    pub seat_trim: Box<SeatTrimReport>,
    /// **What the post-write disk audit found wrong with a file that reads
    /// back** (fix/round100-emit, 2026-09-29). EMIT AND WARN, NEVER REFUSE
    /// (Craig, 2026-09-01): a failed check is a WARNING on the emitted file,
    /// which stays at `artifact_path` as evidence. Empty when every check
    /// passed. Only a file that cannot be read back at all fails the write
    /// ([`EmitError::DiskAuditUnreadable`]).
    #[serde(default)]
    pub audit_findings: Vec<AuditFinding>,
}

/// One failed check of the post-write disk audit: WHICH check, WHICH record,
/// and WHAT value failed against what expectation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditFinding {
    pub check: AuditCheck,
    /// The 0-based support record in the file the check failed on (the first
    /// one, when `failing` is more than 1). `None` when the check is about the
    /// file or the model, or when the model mesh itself holds the value.
    pub record: Option<usize>,
    /// How many records (or values) failed this check. `record` and `detail`
    /// name the first of them; a build-volume finding names the extreme.
    pub failing: usize,
    /// The failing value and what it was checked against, in numbers.
    pub detail: String,
}

impl std::fmt::Display for AuditFinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "check {:?}", self.check)?;
        match self.record {
            Some(record) => write!(formatter, ", record {record}")?,
            None => write!(formatter, ", no single record")?,
        }
        if self.failing > 1 {
            write!(formatter, " (first of {} failing)", self.failing)?;
        }
        write!(formatter, ": {}", self.detail)
    }
}

/// The checks of the post-write disk audit, by name. The first six make a
/// file UNREADABLE (it cannot be decoded back); the rest are findings on a
/// file that reads back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditCheck {
    /// `Project::open` refused the bytes.
    Reopen,
    /// The file does not hold exactly one model.
    ModelCount,
    /// The model's header could not be decoded.
    ModelRead,
    /// The model's triangles could not be decoded.
    ModelTrianglesRead,
    /// The support records could not be decoded.
    SupportsRead,
    /// The file's size could not be read after the audit.
    ArtifactMetadata,
    /// The model's name or placement differs from what was written.
    ModelNameOrPlacement,
    /// The model's triangles differ, bit for bit, from what was written.
    ModelTriangleBits,
    /// A support record differs, bit for bit, from what was written.
    SupportRecordBits,
    /// The file holds a different number of records than primitives written.
    RecordPrimitiveCount,
    /// A sphere record's native fields differ from those derived from its
    /// primitive's authority.
    SphereRecordFields,
    /// The model has no finite lowest point.
    ModelMinimumZ,
    /// The reopened lift and plate do not describe the planned frame.
    PlateFrame,
    /// A record carries a non-finite end point or radius.
    NonFiniteRecord,
    /// A record's (or the model's) mesh is empty, ragged or non-finite.
    MeshShape,
    /// A raft pad or bridge carries an end point z other than the native 0.0.
    RaftRecordFrame,
    /// Geometry lies outside the printer's build volume (below the plate,
    /// above the height, or beyond an X/Y half-width, or spans too far).
    BuildVolume,
}

impl AuditCheck {
    /// Whether a failure of this check means the file cannot be read back.
    #[must_use]
    pub fn makes_the_file_unreadable(self) -> bool {
        matches!(
            self,
            Self::Reopen
                | Self::ModelCount
                | Self::ModelRead
                | Self::ModelTrianglesRead
                | Self::SupportsRead
                | Self::ArtifactMetadata
        )
    }
}

/// **THE SEAT TRIM, as the emitter carried it out** (fix/tip-trim, 2026-09-28;
/// cut to the model mesh, Craig 2026-10-12).
///
/// Every tip ball, the first shaft segment of every seated arm and every
/// further span that carries the trim keep `primitive ∖ model solid`, done for
/// the whole field in ONE batched native call, and of that only the connected
/// piece that holds the shaft end ([`plinth_support::SeatTrim`]). No plane
/// cuts anything. These are its path hits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatTrimReport {
    /// Records that carried a seat trim and went through the batched call.
    pub records_trimmed: usize,
    /// Of those, records the model-solid difference removed material from.
    pub records_cut_by_model: usize,
    /// Of those, records the model cut into more than one piece. Each keeps
    /// the piece that holds its shaft end; the others are
    /// [`Self::far_side_pieces`].
    #[serde(default)]
    pub records_split: usize,
    /// **Every piece dropped because it does not hold the shaft end**: the far
    /// side of a thin wall the ball straddles, a sliver past a ridge. It
    /// would print as a loose bit. Each is a WARNING with its volume; the
    /// record keeps its other piece.
    #[serde(default)]
    pub far_side_pieces: Vec<FarSidePiece>,
    /// Records that lie WHOLLY INSIDE the model: nothing of them is outside
    /// the model solid, so nothing of them can print as support. They are
    /// left out of the file (fix/member-clearance, 2026-09-29: "the seat trim
    /// must never keep a record wholly inside the model"), named here as a
    /// WARNING of a seat that failed.
    #[serde(default)]
    pub records_dropped_wholly_inside_model: usize,
    /// The primitive indices of those dropped records.
    #[serde(default)]
    pub dropped_record_indices: Vec<usize>,
    /// Solid removed by the trim, in thousandths of a mm³, over all records.
    pub removed_volume_milli_mm3: u64,
    /// The batched call itself failed (a model solid the boolean backend
    /// rejects, say). Emit and warn: every record then keeps its untrimmed
    /// geometry, and this names why.
    #[serde(default)]
    pub batch_error: Option<String>,
    /// **THE TRIM MUST NEVER SILENTLY HIDE AN INTRUSION** (heads v19 fixes,
    /// 2026-10-08). Every `Member`, joint-sphere and junction-hull record the
    /// model solid removed material from, with the volume removed: what a
    /// trimmed shaft span loses to the seat plane and the model, and, measured
    /// without cutting anything, what a joint sphere or a hull stands inside
    /// the model by. Tapers and tips are not listed: cutting them to the seat
    /// is what the trim is for.
    #[serde(default)]
    pub member_cuts: Vec<MemberCut>,
    /// One pixel-cube of resin in millionths of a mm³: the registered XY pixel
    /// pitch in x times in y times the layer height. A cut above it is
    /// reported as a warning ([`Self::intrusions`]).
    #[serde(default)]
    pub pixel_cube_micro_mm3: u64,
}

impl SeatTrimReport {
    /// The records whose cut is above one pixel-cube of resin: the trim
    /// removed a visible volume from a part that is not a tip.
    pub fn intrusions(&self) -> impl Iterator<Item = &MemberCut> {
        self.member_cuts
            .iter()
            .filter(|cut| cut.removed_micro_mm3 > self.pixel_cube_micro_mm3)
    }
}

/// One piece of a cut record that does not hold the record's shaft end, and so
/// was left out of the file ([`SeatTrimReport::far_side_pieces`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FarSidePiece {
    /// The primitive index (before any record is left out).
    pub record: usize,
    /// `"tip"`, `"taper"`, `"member"` (or `"hull"`, `"joint"`).
    pub kind: String,
    /// The piece's volume, in millionths of a mm³.
    pub volume_micro_mm3: u64,
    /// How far the piece stands from the record's shaft end, in microns:
    /// the gap it would print across as a loose bit.
    pub distance_from_shaft_um: u64,
}

/// One `Member`, joint or hull record the model solid removed material from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberCut {
    /// The primitive index (before any record is left out).
    pub record: usize,
    /// `"member"`, `"joint"` or `"hull"`.
    pub kind: String,
    /// The volume removed, in millionths of a mm³.
    pub removed_micro_mm3: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EmitError {
    #[error("cannot create ChituBox target {path}")]
    TargetUnavailable { path: PathBuf },
    #[error("ChituBox record encoding rejected the supported scene")]
    RecordEncodingRejected,
    /// The freshly written file could not be READ BACK: our own reader could
    /// not open it, find its one model, or decode its triangles or records.
    /// This is the only audit outcome that fails the write, and even then the
    /// bytes are kept, at `kept_at`, beside the target (never AT the target,
    /// which only ever holds a file that reads back). Every other audit
    /// finding is a WARNING on an emitted file: [`EmitReport::audit_findings`].
    #[error(
        "the freshly reopened ChituBox artifact could not be read back ({finding}); its bytes \
         are kept at {}",
        kept_at.display()
    )]
    DiskAuditUnreadable {
        finding: AuditFinding,
        kept_at: PathBuf,
    },
    /// The rename onto `target_path` itself succeeded and its bytes are
    /// correct, but the directory sync that makes that rename survive a
    /// power loss could not be confirmed. Unlike every other variant, this
    /// one is raised AFTER a valid artifact is already in place at `path` --
    /// see `write`'s doc for why that artifact is deliberately not removed.
    #[error("could not confirm the emitted artifact's name is durable at {path}")]
    DurabilitySyncFailed { path: PathBuf },
    #[error("M0 constants publication failed")]
    Constants,
}

/// Write one newly-created project and derive the report from a fresh disk read.
///
/// `SupportedModel` is the only source of model and support geometry.  Its retained lift is
/// assigned to the ChituBox model placement exactly once; support vertices remain in their
/// model-local frame, where that placement applies to the complete assembly.
///
/// **The disk audit warns; it refuses only a file that cannot be read back**
/// (fix/round100-emit, 2026-09-29; EMIT AND WARN, NEVER REFUSE, Craig 2026-09-01). The
/// project is saved to a sibling staging path and audited there.
/// - A file that reads back reaches `target_path` by a rename, whatever the other checks
///   found. Each failed check is an [`AuditFinding`] in [`EmitReport::audit_findings`]: the
///   check, the record and the value. The file is the diagnostic evidence; deleting it would
///   destroy it.
/// - A file that cannot be read back ([`AuditCheck::makes_the_file_unreadable`]) never
///   reaches `target_path`, which only ever holds a file that reads back. Its bytes are KEPT
///   at [`failed_audit_path`] beside the target, and [`EmitError::DiskAuditUnreadable`]
///   names the check and that path.
///
/// **A failed write leaves no stale artifact at `target_path`.** On every failure path *up
/// to and including the rename* `target_path` is removed, which is what keeps an artifact
/// an earlier run left there from passing for this run's answer. A stale `.failed-audit`
/// sibling from an earlier run is removed at the start, for the same reason.
///
/// **Durability of the accepted name.**  `chitubox_io::Project::save` (used by
/// `write_staged`) fsyncs its bytes before its own rename and then fsyncs the directory that
/// rename lands in, so that first rename survives a crash.  This function performs a
/// *second* rename -- staging path to `target_path` -- and that rename needs the same
/// directory fsync to survive a crash in turn; it is not a side effect of the first one, and
/// this function performs it.  If that final sync cannot be confirmed, the artifact at
/// `target_path` is genuinely correct (the rename itself already succeeded) but its
/// survival across a power loss is not, and [`EmitError::DurabilitySyncFailed`] is returned
/// *without* deleting `target_path` -- unlike every other failure path, deleting a correct
/// artifact here would trade an open durability question for certain data loss.
///
/// # Errors
///
/// - [`EmitError::TargetUnavailable`] if the target or its staging sibling cannot be created,
///   or if the accepted staging file cannot be renamed into place.
/// - [`EmitError::DurabilitySyncFailed`] if the rename onto `target_path` succeeded but the
///   directory sync that makes it survive a crash could not be confirmed. `target_path`
///   holds the correct artifact regardless.
/// - [`EmitError::RecordEncodingRejected`] if the scene does not convert to wire records.
/// - [`EmitError::DiskAuditUnreadable`] if the freshly written file cannot be read back. Its
///   bytes are kept at [`failed_audit_path`].
pub fn write(model: &SupportedModel, request: &EmitRequest) -> Result<EmitReport, EmitError> {
    validate_target(&request.target_path)?;
    let staging = staging_path(&request.target_path);
    let failed_audit = failed_audit_path(&request.target_path);
    // A leftover staging file from a killed run, or an earlier run's kept
    // unreadable bytes, is not evidence about this one.
    let _ = std::fs::remove_file(&staging);
    let _ = std::fs::remove_file(&failed_audit);

    let staged = write_staged(model, request, &staging);
    settle_staged(&staging, &request.target_path, staged)
}

/// What [`write`] does with the staging file once the audit has spoken: a
/// file that reads back is renamed onto `target`; an unreadable one is kept
/// at [`failed_audit_path`]; any other failure removes both.
fn settle_staged(
    staging: &Path,
    target: &Path,
    staged: Result<EmitReport, EmitError>,
) -> Result<EmitReport, EmitError> {
    match staged {
        Ok(mut report) => {
            if std::fs::rename(staging, target).is_err() {
                let _ = std::fs::remove_file(staging);
                let _ = std::fs::remove_file(target);
                return Err(EmitError::TargetUnavailable {
                    path: target.to_path_buf(),
                });
            }
            // See "Durability of the accepted name" above: this rename can
            // return `Ok` while the directory entry it just repointed is
            // still only in the filesystem's in-memory page cache, so a
            // crash right after this point could leave `target_path`
            // absent or pointing at the previous artifact even though the
            // rename syscall itself succeeded.
            if sync_dir(target).is_err() {
                return Err(EmitError::DurabilitySyncFailed {
                    path: target.to_path_buf(),
                });
            }
            report.artifact_path = target.to_path_buf();
            Ok(report)
        }
        Err(EmitError::DiskAuditUnreadable { finding, .. }) => {
            // Keep the bytes: they are the only evidence of what the writer
            // produced. Never AT the target, which holds only a file that reads
            // back; beside it. If even the rename fails they stay at staging.
            let _ = std::fs::remove_file(target);
            let failed_audit = failed_audit_path(target);
            let kept_at = if std::fs::rename(staging, &failed_audit).is_ok() {
                failed_audit
            } else {
                staging.to_path_buf()
            };
            Err(EmitError::DiskAuditUnreadable { finding, kept_at })
        }
        Err(error) => {
            let _ = std::fs::remove_file(staging);
            let _ = std::fs::remove_file(target);
            Err(error)
        }
    }
}

/// Fsync the directory containing `path`, so a prior rename onto `path`
/// survives a crash. Content durability is a separate concern already
/// handled before any rename; this is name durability only.
fn sync_dir(path: &Path) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::File::open(dir)?.sync_all()
}

/// The staging sibling `write` builds before the audit decides.
///
/// Deterministic and derived only from the target name — no clock, no counter,
/// no randomness — because two concurrent writes to one target path are
/// already a caller defect, and a nondeterministic name would trade that
/// defect for an unremovable litter of partial files.
fn staging_path(target: &Path) -> PathBuf {
    const STAGING_SUFFIX: &str = ".plinth-partial";
    let mut name = target
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(STAGING_SUFFIX);
    target.with_file_name(name)
}

/// Where [`write`] keeps the bytes of a file that could not be read back:
/// `<target>.failed-audit`, beside the target and never at it.
#[must_use]
pub fn failed_audit_path(target: &Path) -> PathBuf {
    const FAILED_AUDIT_SUFFIX: &str = ".failed-audit";
    let mut name = target
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(FAILED_AUDIT_SUFFIX);
    target.with_file_name(name)
}

/// One pixel-cube of resin in millionths of a mm³: the printer's XY pixel pitch
/// in x times in y times its layer height.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a registered pixel-cube is a small positive volume, far below u64::MAX millionths"
)]
fn pixel_cube_micro_mm3(printer: &PrinterProfile) -> u64 {
    let pitch_x_mm = printer.pixel_pitch_x_um().value() / 1000.0;
    let pitch_y_mm = printer.pixel_pitch_y_um().value() / 1000.0;
    let layer_mm = *printer.default_layer_height_mm().value();
    (pitch_x_mm * pitch_y_mm * layer_mm * 1.0e6).round().max(0.0) as u64
}

fn write_staged(
    model: &SupportedModel,
    request: &EmitRequest,
    staging: &Path,
) -> Result<EmitReport, EmitError> {
    // Write-only stage marks (X0, 2026-10-10): where the Emit phase spends its
    // time. Inert unless `--perf` is on.
    let soup_mark = geom_kernel::perf::stage("emit.model_soup");
    let center = mesh_bbox_center(model.mesh()).ok_or(EmitError::RecordEncodingRejected)?;
    let geometry = model_triangle_soup(model.mesh(), center)?;
    drop(soup_mark);
    let lift = model.lift_mm().get();
    let lift = f32_of(lift)?;
    let records_mark = geom_kernel::perf::stage("emit.build_records");
    let (records, mut seat_trim) =
        build_records_trimmed(model.primitives(), center, Some(model.mesh()))?;
    drop(records_mark);
    seat_trim.pixel_cube_micro_mm3 = pixel_cube_micro_mm3(&request.printer);
    // The audit reopens the file and pairs every record with its primitive, so
    // it is given exactly the primitives whose records were written.
    let written: std::borrow::Cow<'_, [SupportPrimitive]> =
        if seat_trim.dropped_record_indices.is_empty() {
            std::borrow::Cow::Borrowed(model.primitives())
        } else {
            std::borrow::Cow::Owned(
                model
                    .primitives()
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !seat_trim.dropped_record_indices.contains(index))
                    .map(|(_, primitive)| primitive.clone())
                    .collect(),
            )
        };

    let project_mark = geom_kernel::perf::stage("emit.project");
    let mut project = Project::new_empty();
    let index = project
        .add_model(ModelSource {
            name: MODEL_NAME.to_owned(),
            geometry: ModelGeometry::Triangles(geometry.clone()),
            placement: Placement::default(),
        })
        .map_err(|_| EmitError::RecordEncodingRejected)?;
    debug_assert_eq!(index, 0);

    // `attach_supports` is the only format-side operation which applies a lift to a model with
    // records.  For the empty support case, use the analogous placement update explicitly.
    if records.is_empty() {
        project
            .set_placements(&[PlacementUpdate {
                model_index: index,
                placement: Placement {
                    x: 0.0,
                    y: 0.0,
                    z_lift: lift,
                },
            }])
            .map_err(|_| EmitError::RecordEncodingRejected)?;
    } else {
        project
            .attach_supports(&[SupportBatch {
                model_index: index,
                records: records.clone(),
                mode: SupportMode::Replace,
                lift_mm: Some(lift),
            }])
            .map_err(|_| EmitError::RecordEncodingRejected)?;
    }
    drop(project_mark);
    let save_mark = geom_kernel::perf::stage("emit.save");
    project
        .save(staging)
        .map_err(|_| EmitError::TargetUnavailable {
            path: request.target_path.clone(),
        })?;
    drop(save_mark);

    let audit_mark = geom_kernel::perf::stage("emit.audit_disk");
    let mut report = audit_disk(
        staging,
        &request.printer,
        AuditExpected {
            lift,
            plate_z: f32_of(model.plate_z_mm().get() - center[2])?,
            model: &geometry,
            records: &records,
            primitives: &written,
            model_center: center,
        },
    )
    .map_err(|finding| EmitError::DiskAuditUnreadable {
        finding,
        kept_at: staging.to_path_buf(),
    })?;
    drop(audit_mark);
    report.seat_trim = Box::new(seat_trim);
    Ok(report)
}

fn validate_target(path: &Path) -> Result<(), EmitError> {
    if path.is_dir() || !path.parent().unwrap_or_else(|| Path::new(".")).is_dir() {
        return Err(EmitError::TargetUnavailable {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

struct AuditExpected<'a> {
    lift: f32,
    plate_z: f32,
    model: &'a TriangleSoup,
    records: &'a [SupportRecord],
    primitives: &'a [SupportPrimitive],
    model_center: [f64; 3],
}

fn audit_disk(
    path: &Path,
    printer: &PrinterProfile,
    expected: AuditExpected<'_>,
) -> Result<EmitReport, AuditFinding> {
    // UNREADABLE: the file cannot be decoded back. Only these fail the write.
    let unreadable = |check: AuditCheck, detail: String| AuditFinding {
        check,
        record: None,
        failing: 1,
        detail,
    };
    let project = Project::open(path)
        .map_err(|error| unreadable(AuditCheck::Reopen, format!("open refused: {error:?}")))?;
    if project.model_count() != 1 {
        return Err(unreadable(
            AuditCheck::ModelCount,
            format!("{} models, 1 written", project.model_count()),
        ));
    }
    let info = project
        .model(0)
        .map_err(|error| unreadable(AuditCheck::ModelRead, format!("{error:?}")))?;
    let model_triangles = project
        .model_triangles(0)
        .map_err(|error| unreadable(AuditCheck::ModelTrianglesRead, format!("{error:?}")))?;
    let records = project
        .supports(0)
        .map_err(|error| unreadable(AuditCheck::SupportsRead, format!("{error:?}")))?;

    // READABLE: every other check is a WARNING on the emitted file.
    let mut findings = Findings::default();
    let written_placement = Placement {
        x: 0.0,
        y: 0.0,
        z_lift: expected.lift,
    };
    if info.name != MODEL_NAME || !same_placement_bits(info.placement, written_placement) {
        findings.fail(
            AuditCheck::ModelNameOrPlacement,
            None,
            format!(
                "read name {:?} placement {:?}, wrote name {MODEL_NAME:?} placement {:?}",
                info.name, info.placement, written_placement
            ),
        );
    }
    if let Some(index) = first_differing_f32(&model_triangles, &expected.model.xyz) {
        findings.fail(
            AuditCheck::ModelTriangleBits,
            None,
            format!(
                "model coordinate {index} differs (read {} values, wrote {})",
                model_triangles.len(),
                expected.model.xyz.len()
            ),
        );
    }
    if records.len() != expected.records.len() {
        findings.fail(
            AuditCheck::SupportRecordBits,
            None,
            format!(
                "read {} records, wrote {}",
                records.len(),
                expected.records.len()
            ),
        );
    }
    for (index, (actual, wrote)) in records.iter().zip(expected.records).enumerate() {
        if let Some(field) = differing_record_field(actual, wrote) {
            findings.fail(
                AuditCheck::SupportRecordBits,
                Some(index),
                format!("field {field} differs from the record written"),
            );
        }
    }
    if records.len() != expected.primitives.len() {
        findings.fail(
            AuditCheck::RecordPrimitiveCount,
            None,
            format!(
                "{} records, {} primitives written",
                records.len(),
                expected.primitives.len()
            ),
        );
    }
    for (index, (record, primitive)) in records.iter().zip(expected.primitives).enumerate() {
        if let Err(detail) = audit_sphere_record_fields(record, primitive, expected.model_center) {
            findings.fail(AuditCheck::SphereRecordFields, Some(index), detail);
        }
    }

    match min_z(&model_triangles) {
        None => findings.fail(
            AuditCheck::ModelMinimumZ,
            None,
            format!(
                "no finite lowest point in {} model values",
                model_triangles.len()
            ),
        ),
        Some(model_local_min_z) => {
            // This is the physical frame invariant frozen by the Python emitter:
            // centring moves the model and the planned plate together, while the sole
            // placement lift separates their local Z values by exactly `lift_mm`.
            let reopened_plate_z = model_local_min_z - info.placement.z_lift;
            if !same_f32_frame_value(reopened_plate_z, expected.plate_z)
                || !same_f32_frame_value(
                    model_local_min_z - reopened_plate_z,
                    info.placement.z_lift,
                )
            {
                findings.fail(
                    AuditCheck::PlateFrame,
                    None,
                    format!(
                        "reopened plate z {reopened_plate_z} (model minimum {model_local_min_z} \
                         minus lift {}) against the planned plate z {}",
                        info.placement.z_lift, expected.plate_z
                    ),
                );
            }
        }
    }

    // The world datum is `expected.plate_z` -- the one binary32 the raft pads
    // were actually emitted at -- and deliberately NOT `reopened_plate_z`.
    // Both name the same plate, but they are separately rounded derivations of
    // it: the pads carry `f32(plate_z_mm - center_z)`, while `reopened_plate_z`
    // is an f32 subtraction of the reopened lift from the reopened model
    // minimum. When `lift` and `lift + center_z` straddle a binade boundary the
    // second derivation double-rounds and lands 1-2 ulp above the first, so a
    // pad sitting exactly on the plate measures a negative world Z and the
    // `min[2] < 0.0` floor below refuses a perfect layout. Measuring against
    // the pads' own datum makes a pad on the plate read exactly 0.0. The frame
    // check above still binds the two derivations together, so this cannot
    // hide a genuinely wrong plate or a second lift.
    let plate_datum = expected.plate_z;
    let mut bounds = WorldBounds::empty();
    if let Err(detail) = bounds.extend_f32(
        &model_triangles,
        info.placement,
        plate_datum,
        GeometrySource::Model,
    ) {
        findings.fail(AuditCheck::MeshShape, None, format!("model mesh: {detail}"));
    }
    for (index, record) in records.iter().enumerate() {
        if !record.r_start.is_finite()
            || !record.r_end.is_finite()
            || !record
                .start
                .iter()
                .chain(record.end.iter())
                .all(|value| value.is_finite())
        {
            findings.fail(
                AuditCheck::NonFiniteRecord,
                Some(index),
                format!(
                    "start {:?} end {:?} r_start {} r_end {}",
                    record.start, record.end, record.r_start, record.r_end
                ),
            );
            continue;
        }
        if let Err(detail) = bounds.extend_f32(
            &record.mesh.xyz,
            info.placement,
            plate_datum,
            GeometrySource::Record(index),
        ) {
            findings.fail(
                AuditCheck::MeshShape,
                Some(index),
                format!("record mesh: {detail}"),
            );
        }
        if record.segment_type == SegmentType::PAD_OR_BRIDGE {
            // A raft record's end points are in the NATIVE raft frame: z is
            // exactly 0.0, not a model-frame height (`encode_record`), so they
            // are not geometry to bound. Its mesh, bounded above, is. Any
            // other z on a pad or bridge is a frame error.
            if record.start[2].to_bits() != 0.0_f32.to_bits()
                || record.end[2].to_bits() != 0.0_f32.to_bits()
            {
                findings.fail(
                    AuditCheck::RaftRecordFrame,
                    Some(index),
                    format!(
                        "raft end point z {} and {}, native 0.0",
                        record.start[2], record.end[2]
                    ),
                );
            }
            continue;
        }
        bounds.extend_point(
            record.start,
            info.placement,
            plate_datum,
            GeometrySource::Record(index),
        );
        bounds.extend_point(
            record.end,
            info.placement,
            plate_datum,
            GeometrySource::Record(index),
        );
    }
    for finding in bounds.violations(printer) {
        findings.push(finding);
    }

    let artifact_bytes = std::fs::metadata(path)
        .map_err(|error| unreadable(AuditCheck::ArtifactMetadata, format!("{error:?}")))?
        .len();
    Ok(EmitReport {
        artifact_path: path.to_path_buf(),
        support_record_count: records.len(),
        artifact_bytes,
        seat_trim: Box::default(),
        audit_findings: findings.into_vec(),
    })
}

/// The audit's findings, ONE per check for per-record checks: the first
/// failing record and value are kept, and every further failure of the same
/// check only counts. Aggregate findings (the build volume's sides) are
/// pushed whole.
#[derive(Default)]
struct Findings(Vec<AuditFinding>);

impl Findings {
    fn fail(&mut self, check: AuditCheck, record: Option<usize>, detail: String) {
        if let Some(existing) = self.0.iter_mut().find(|finding| finding.check == check) {
            existing.failing += 1;
            return;
        }
        self.0.push(AuditFinding {
            check,
            record,
            failing: 1,
            detail,
        });
    }

    fn push(&mut self, finding: AuditFinding) {
        self.0.push(finding);
    }

    fn into_vec(self) -> Vec<AuditFinding> {
        self.0
    }
}

fn first_differing_f32(actual: &[f32], expected: &[f32]) -> Option<usize> {
    if let Some(index) = actual
        .iter()
        .zip(expected)
        .position(|(actual, expected)| !same_f32_bits(*actual, *expected))
    {
        return Some(index);
    }
    (actual.len() != expected.len()).then_some(actual.len().min(expected.len()))
}

/// The first field (by name) in which a reopened record differs, bit for bit,
/// from the record written; `None` when they are identical.
fn differing_record_field(
    actual: &SupportRecord,
    expected: &SupportRecord,
) -> Option<&'static str> {
    if actual.segment_type != expected.segment_type {
        return Some("segment_type");
    }
    if !same_f32_slice_bits(&actual.start, &expected.start) {
        return Some("start");
    }
    if !same_f32_slice_bits(&actual.end, &expected.end) {
        return Some("end");
    }
    if !same_f32_bits(actual.r_start, expected.r_start) {
        return Some("r_start");
    }
    if !same_f32_bits(actual.r_end, expected.r_end) {
        return Some("r_end");
    }
    if !same_f32_slice_bits(&actual.mesh.xyz, &expected.mesh.xyz) {
        return Some("mesh");
    }
    // This covers type-1's depth and every other f32-bearing trailing field
    // without assuming unwritten bytes are zero.
    if actual.trailing != expected.trailing {
        return Some("trailing");
    }
    None
}

/// Independent verifier for the native sphere-record field projection.  Keep
/// this separate from the producer's `SphereGeometry::canonical_poles`: it
/// derives the expected binary32 fields from immutable semantic authority
/// after reopening the file.
fn audit_sphere_record_fields(
    record: &SupportRecord,
    primitive: &SupportPrimitive,
    model_center: [f64; 3],
) -> Result<(), String> {
    let Some(sphere) = &primitive.sphere_geometry else {
        return Ok(());
    };
    // The record carries the radius the sphere is DRAWN at (a joint sphere is
    // circumscribed, see `drawn_sphere_radius`), poles included.
    let radius = drawn_sphere_radius(primitive.segment_kind, primitive.start_radius_mm.get());
    let (start, end) = if let Some((center_mm, outward_axis_unit)) = sphere.tip_authority() {
        if primitive.segment_kind != SupportSegmentKind::ContactSphere {
            return Err(format!(
                "tip sphere authority on a {:?} primitive",
                primitive.segment_kind
            ));
        }
        let center = finite_point(center_mm);
        let axis = finite_point(outward_axis_unit);
        (
            std::array::from_fn(|index| center[index] - radius * axis[index]),
            std::array::from_fn(|index| center[index] + radius * axis[index]),
        )
    } else if let Some(center_mm) = sphere.joint_center() {
        if primitive.segment_kind != SupportSegmentKind::JointSphere {
            return Err(format!(
                "joint sphere authority on a {:?} primitive",
                primitive.segment_kind
            ));
        }
        let center = finite_point(center_mm);
        (
            [center[0], center[1], center[2] + radius],
            [center[0], center[1], center[2] - radius],
        )
    } else {
        return Err("sphere geometry with neither tip nor joint authority".to_owned());
    };
    let not_finite = |_| "sphere authority is not finite in binary32".to_owned();
    let expected_start = point_f32(sub_center(start, model_center)).map_err(not_finite)?;
    let expected_end = point_f32(sub_center(end, model_center)).map_err(not_finite)?;
    let expected_radius = f32_of(radius).map_err(not_finite)?;
    if !same_f32_slice_bits(&record.start, &expected_start)
        || !same_f32_slice_bits(&record.end, &expected_end)
        || !same_f32_bits(record.r_start, expected_radius)
        || !same_f32_bits(record.r_end, expected_radius)
    {
        return Err(format!(
            "record start {:?} end {:?} r {} / {}, authority start {expected_start:?} end \
             {expected_end:?} r {expected_radius}",
            record.start, record.end, record.r_start, record.r_end
        ));
    }
    Ok(())
}

/// Compare format fields, never numeric values: `PartialEq` treats +0.0 and
/// -0.0 as identical and treats all NaN payloads as unequal.  The disk audit
/// is a wire-fidelity check, so binary32 payloads are the authority.
fn same_f32_bits(actual: f32, expected: f32) -> bool {
    actual.to_bits() == expected.to_bits()
}

fn same_f32_slice_bits(actual: &[f32], expected: &[f32]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| same_f32_bits(*actual, *expected))
}

fn same_placement_bits(actual: Placement, expected: Placement) -> bool {
    same_f32_bits(actual.x, expected.x)
        && same_f32_bits(actual.y, expected.y)
        && same_f32_bits(actual.z_lift, expected.z_lift)
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

fn min_z(values: &[f32]) -> Option<f32> {
    if values.is_empty() || !values.len().is_multiple_of(3) {
        return None;
    }
    values
        .chunks_exact(3)
        .map(|point| point[2])
        .filter(|value| value.is_finite())
        .min_by(f32::total_cmp)
}

/// Which piece of the file set a bound: the model mesh or one record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeometrySource {
    Model,
    Record(usize),
}

impl GeometrySource {
    fn record(self) -> Option<usize> {
        match self {
            Self::Model => None,
            Self::Record(index) => Some(index),
        }
    }
}

/// Bounds in printer coordinates, never merely local mesh coordinates, with
/// the piece of the file that set each extreme, so a build-volume finding can
/// name the record that breaks it.
#[derive(Clone, Copy)]
struct WorldBounds {
    bounds: Bounds,
    min_at: [Option<GeometrySource>; 3],
    max_at: [Option<GeometrySource>; 3],
}

impl WorldBounds {
    fn empty() -> Self {
        Self {
            bounds: Bounds::empty(),
            min_at: [None; 3],
            max_at: [None; 3],
        }
    }

    /// Extend by a mesh; `Err` (with nothing added) for an empty, ragged or
    /// non-finite mesh.
    fn extend_f32(
        &mut self,
        values: &[f32],
        placement: Placement,
        plate_z: f32,
        source: GeometrySource,
    ) -> Result<(), String> {
        if values.is_empty() || !values.len().is_multiple_of(3) {
            return Err(format!(
                "{} values, not a positive multiple of 3",
                values.len()
            ));
        }
        if let Some(index) = values.iter().position(|value| !value.is_finite()) {
            return Err(format!("value {index} is {}", values[index]));
        }
        for point in values.chunks_exact(3) {
            self.extend_point([point[0], point[1], point[2]], placement, plate_z, source);
        }
        Ok(())
    }

    fn extend_point(
        &mut self,
        point: [f32; 3],
        placement: Placement,
        plate_z: f32,
        source: GeometrySource,
    ) {
        let world = [
            point[0] + placement.x,
            point[1] + placement.y,
            point[2] - plate_z,
        ];
        for (axis, value) in world.iter().enumerate() {
            let value = f64::from(*value);
            // A non-finite world value (an f32 overflow) must still be seen:
            // NaN fails every comparison, so it is taken as a new extreme.
            if value.is_nan() || value < self.bounds.min[axis] {
                self.bounds.min[axis] = value;
                self.min_at[axis] = Some(source);
            }
            if value.is_nan() || value > self.bounds.max[axis] {
                self.bounds.max[axis] = value;
                self.max_at[axis] = Some(source);
            }
        }
    }

    /// One finding per violated side of the printer's build volume, naming
    /// the extreme value, the limit and the piece that holds the extreme.
    /// (Spans need no check of their own: inside every side means the span
    /// fits.)
    fn violations(&self, printer: &PrinterProfile) -> Vec<AuditFinding> {
        let volume = printer.build_volume_mm().value();
        let half = [volume.x() * 0.5, volume.y() * 0.5];
        let limits: [(usize, &str, f64, bool); 6] = [
            (0, "x", -half[0], false),
            (0, "x", half[0], true),
            (1, "y", -half[1], false),
            (1, "y", half[1], true),
            (2, "z above the plate", 0.0, false),
            (2, "z above the plate", volume.z(), true),
        ];
        let mut out = Vec::new();
        for (axis, name, limit, is_max) in limits {
            let (value, source) = if is_max {
                (self.bounds.max[axis], self.max_at[axis])
            } else {
                (self.bounds.min[axis], self.min_at[axis])
            };
            let Some(source) = source else {
                out.push(AuditFinding {
                    check: AuditCheck::BuildVolume,
                    record: None,
                    failing: 1,
                    detail: format!("no geometry bounds {name}"),
                });
                continue;
            };
            // NaN is outside: it compares as neither inside bound.
            let inside = if is_max {
                value <= limit
            } else {
                value >= limit
            };
            let outside = value.is_nan() || !inside;
            if outside {
                let side = if is_max { "maximum" } else { "minimum" };
                let bound = if is_max { "at most" } else { "at least" };
                let holder = match source {
                    GeometrySource::Model => "the model mesh".to_owned(),
                    GeometrySource::Record(index) => format!("record {index}"),
                };
                out.push(AuditFinding {
                    check: AuditCheck::BuildVolume,
                    record: source.record(),
                    failing: 1,
                    detail: format!(
                        "{side} {name} {value:.6} mm (held by {holder}), the printer allows \
                         {bound} {limit:.6} mm"
                    ),
                });
            }
        }
        out
    }
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    #[cfg(test)]
    fn extend_f32(&mut self, values: &[f32]) -> Result<(), EmitError> {
        if values.is_empty() || !values.len().is_multiple_of(3) {
            return Err(EmitError::RecordEncodingRejected);
        }
        for point in values.chunks_exact(3) {
            for (axis, value) in point.iter().enumerate() {
                if !value.is_finite() {
                    return Err(EmitError::RecordEncodingRejected);
                }
                let value = f64::from(*value);
                self.min[axis] = self.min[axis].min(value);
                self.max[axis] = self.max[axis].max(value);
            }
        }
        Ok(())
    }
}

fn mesh_bbox_center(mesh: &Mesh) -> Option<[f64; 3]> {
    let mut bounds = Bounds::empty();
    for vertex in mesh.verts() {
        for (axis, value) in vertex.iter().enumerate() {
            if !value.is_finite() {
                return None;
            }
            bounds.min[axis] = bounds.min[axis].min(*value);
            bounds.max[axis] = bounds.max[axis].max(*value);
        }
    }
    Some([
        (bounds.min[0] + bounds.max[0]) * 0.5,
        (bounds.min[1] + bounds.max[1]) * 0.5,
        (bounds.min[2] + bounds.max[2]) * 0.5,
    ])
}

fn model_triangle_soup(mesh: &Mesh, center: [f64; 3]) -> Result<TriangleSoup, EmitError> {
    let mut triangles = Vec::with_capacity(mesh.tris().len());
    for indices in mesh.tris() {
        let mut triangle = [[0.0; 3]; 3];
        for (corner, index) in indices.iter().enumerate() {
            let source = mesh
                .verts()
                .get(*index as usize)
                .ok_or(EmitError::RecordEncodingRejected)?;
            triangle[corner] = sub_center(*source, center);
        }
        triangles.push(triangle);
    }
    let mut soups = [triangles];
    wind_outward(&mut soups)?;
    soup_from_triangles(&soups[0])
}

/// Build every support record for a model with no seat trim applied — the
/// single-primitive test path.
#[cfg(test)]
fn build_records(
    primitives: &[SupportPrimitive],
    center: [f64; 3],
) -> Result<Vec<SupportRecord>, EmitError> {
    build_records_trimmed(primitives, center, None).map(|(records, _)| records)
}

/// Build every support record for a model in one pass, with every record that
/// carries a seat trim cut to it.
///
/// Batch-first (D-37): gather every primitive's frame and raw soup, hand the
/// whole forest to one kernel winding call, then scatter into wire records.
/// There is deliberately no per-primitive winding call in this path. The
/// registered frustum/raft side counts are likewise read exactly once, here,
/// rather than once per primitive.
///
/// **THE SEAT TRIM** (fix/tip-trim, 2026-09-28; cut to the model mesh, Craig
/// 2026-10-12). A seated tip ball and the shaft spans that carry its trim keep
/// `primitive ∖ model solid`, and of that only the connected piece holding the
/// shaft end ([`trim_to_seats`]). ChituBox renders exactly this mesh, so this
/// is where the rule has to hold. The whole field goes through ONE batched
/// native call ([`plinth_solid::ManifoldBackend::trim_and_subtract_each`], the
/// same Manifold backend `plinth-hollow` drills with): every piece welded once,
/// the model imported once, no per-record crossing. The record's own
/// `start`/`end`/radii are untouched; only its soup is cut.
///
/// Without a model (`model: None`, the unit tests' single-record path) no
/// record is trimmed. Emit and warn throughout: a batch the backend rejects
/// leaves every record untrimmed and says why in the report, a piece that
/// does not hold the shaft end is left out with a warning, and a record with
/// nothing outside the model is left out of the file; all are counted.
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn build_records_trimmed(
    primitives: &[SupportPrimitive],
    center: [f64; 3],
    model: Option<&Mesh>,
) -> Result<(Vec<SupportRecord>, SeatTrimReport), EmitError> {
    let policy = m0_constants()
        .map_err(|_| EmitError::Constants)?
        .placeholders();
    let frustum_sides = policy.chitubox_emit_n_sides().diagnostic().get() as usize;
    let raft_sides = policy.chitubox_pad_n_sides().diagnostic().get() as usize;
    let mut frames = Vec::with_capacity(primitives.len());
    let mut soups = Vec::with_capacity(primitives.len());
    for primitive in primitives {
        let (frame, triangles) =
            record_frame_and_soup(primitive, center, frustum_sides, raft_sides)?;
        frames.push(frame);
        soups.push(triangles);
    }
    // Outward first: the boolean backend reads orientation as inside/outside.
    wind_outward(&mut soups)?;
    let report = match model {
        Some(mesh) => {
            let mut report = trim_to_seats(primitives, &mut soups, mesh, center);
            measure_untrimmed_intrusions(primitives, &soups, mesh, center, &mut report);
            report
        }
        None => SeatTrimReport::default(),
    };
    // A record wholly inside the model is left out (`trim_to_seats`); every
    // other record is encoded in primitive order, as before.
    let records = frames
        .iter()
        .zip(&soups)
        .enumerate()
        .filter(|(index, _)| !report.dropped_record_indices.contains(index))
        .map(|(_, (frame, triangles))| encode_record(frame, triangles))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((records, report))
}

/// **The seat trim over a whole field, in place on the wound soups: cut to the
/// model MESH, not to a plane** (Craig 2026-10-12).
///
/// Every record that carries a seat trim keeps `primitive ∖ model solid`, from
/// ONE batched native call (the half-space each piece is handed keeps all of
/// it). The model can cut a record into several pieces: the ball straddles a
/// thin wall, or reaches past a ridge. The piece that holds the record's SHAFT
/// END (the ball's pole along the shaft, a frustum's end further along it) is
/// what the support carries; every other piece stands past the model's far
/// side or beyond a ridge, touches nothing of the support, and would print as
/// a loose bit. It is left out and WARNED with its volume
/// ([`SeatTrimReport::far_side_pieces`]). A record with nothing outside the
/// model is left out of the file, as before.
///
/// Until 2026-10-12 each piece was also cut by the half-space on the support
/// side of a plane through ONE seat point. On a curved or tilted underside
/// that plane removed ball the model never touched: the file got 32 % of the
/// planned contact on Bat_Cat_Owl and 30 % on Deinonychus. The model
/// difference alone already removes everything inside the model; the
/// connected-piece rule does the far-side protection the plane used to.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a vertex index of a mesh the backend itself validated fits usize on every supported \
              target, and the removed volume is clamped non-negative and far below u64::MAX \
              thousandths of a mm3"
)]
fn trim_to_seats(
    primitives: &[SupportPrimitive],
    soups: &mut [Vec<SoupTriangle>],
    mesh: &Mesh,
    center: [f64; 3],
) -> SeatTrimReport {
    let mut report = SeatTrimReport::default();
    let trimmed: Vec<(usize, plinth_support::SeatTrim)> = primitives
        .iter()
        .enumerate()
        .filter_map(|(index, primitive)| primitive.seat_trim.map(|trim| (index, trim)))
        .collect();
    if trimmed.is_empty() {
        return report;
    }
    let mut pieces = Vec::with_capacity(trimmed.len());
    let mut keep = Vec::with_capacity(trimmed.len());
    for &(index, _) in &trimmed {
        let Some(piece) = weld(&soups[index]) else {
            report.batch_error = Some(format!("record {index}: its soup does not weld closed"));
            return report;
        };
        // NO PLANE: a half-space below every vertex of the piece keeps all of
        // it, so the native call is the model difference alone.
        let lowest = piece
            .vertices()
            .iter()
            .map(|vertex| vertex[2])
            .fold(f64::INFINITY, f64::min);
        keep.push(plinth_solid::KeepHalfSpace {
            normal: [0.0, 0.0, 1.0],
            offset: lowest - 1.0,
        });
        pieces.push(piece);
    }
    // The model as ONE indexed solid, welded by point identity: a mesh can be
    // closed by position while carrying the same corner under two indices (a
    // fixture's separate caps, a decimation seam), and the boolean backend
    // reads connectivity by index only.
    let model_soup: Vec<SoupTriangle> = mesh
        .tris()
        .iter()
        .map(|triangle| triangle.map(|corner| sub_center(mesh.verts()[corner as usize], center)))
        .collect();
    let Some(solid) = weld(&model_soup) else {
        report.batch_error = Some("the model mesh does not weld into a valid solid".to_owned());
        return report;
    };
    let results = match plinth_solid::ManifoldBackend.trim_and_subtract_each(
        &solid,
        &pieces,
        &keep,
        plinth_solid::BooleanLimits::UNLIMITED,
    ) {
        Ok(results) => results,
        Err(error) => {
            report.batch_error = Some(error.to_string());
            return report;
        }
    };
    // Each cut record's pieces, split and chosen: pure Rust over the meshes
    // the one native call returned, linear in their triangles, no further
    // native crossing.
    let chosen: Vec<Option<KeptPiece>> = trimmed
        .iter()
        .zip(&results)
        .map(|(&(index, trim), result)| {
            result
                .mesh
                .as_ref()
                .map(|kept| keep_shaft_piece(kept, shaft_end(&primitives[index], trim, center)))
        })
        .collect();
    let mut removed_mm3 = 0.0_f64;
    for ((&(index, _), result), kept) in trimmed.iter().zip(&results).zip(chosen) {
        report.records_trimmed += 1;
        // "Removed material" means more than the backend's own rounding: a
        // relative part in a billion of the piece.
        let noise = 1e-9 * result.whole_volume.abs();
        report.records_cut_by_model +=
            usize::from(result.whole_volume - result.after_subtraction_volume > noise);
        // A RECORD WITH NOTHING OUTSIDE THE MODEL is never kept whole inside
        // it (fix/member-clearance, 2026-09-29): it prints as model and nothing
        // else, and is left out of the file with a named warning.
        let Some(kept) = kept else {
            report.records_dropped_wholly_inside_model += 1;
            report.dropped_record_indices.push(index);
            note_member_cut(&mut report, &primitives[index], index, result.whole_volume);
            continue;
        };
        let kind = record_kind(&primitives[index]);
        report.records_split += usize::from(!kept.dropped.is_empty());
        let mut dropped_mm3 = 0.0_f64;
        for &(volume_mm3, distance_mm) in &kept.dropped {
            dropped_mm3 += volume_mm3.max(0.0);
            report.far_side_pieces.push(FarSidePiece {
                record: index,
                kind: kind.to_owned(),
                volume_micro_mm3: (volume_mm3.max(0.0) * 1.0e6).round() as u64,
                distance_from_shaft_um: (distance_mm.max(0.0) * 1.0e3).round() as u64,
            });
        }
        let removed = result.whole_volume - result.after_subtraction_volume + dropped_mm3;
        removed_mm3 += removed.max(0.0);
        note_member_cut(&mut report, &primitives[index], index, removed);
        soups[index] = kept.soup;
    }
    report.removed_volume_milli_mm3 = (removed_mm3 * 1000.0).round() as u64;
    report
}

/// A cut record's kept piece, and the pieces left out.
struct KeptPiece {
    /// The piece that holds the shaft end, as a soup.
    soup: Vec<SoupTriangle>,
    /// Every other piece: `(volume mm³, distance from the shaft end mm)`.
    dropped: Vec<(f64, f64)>,
}

/// The kind a far-side piece is named by.
fn record_kind(primitive: &SupportPrimitive) -> &'static str {
    match (primitive.segment_kind, &primitive.ring_normal) {
        (SupportSegmentKind::ContactSphere, _) => "tip",
        (SupportSegmentKind::ContactTaper, _) => "taper",
        (SupportSegmentKind::Member, FrustumRingNormal::Junction(_)) => "hull",
        (SupportSegmentKind::JointSphere, _) => "joint",
        _ => "member",
    }
}

/// **The point a cut record's kept piece must hold: its SHAFT END**, in the
/// record's model-centred frame. For the tip ball, its pole along the shaft
/// direction (where the taper leaves it); for a frustum, its end further along
/// the shaft (toward the base). The model sits on the other side.
fn shaft_end(
    primitive: &SupportPrimitive,
    trim: plinth_support::SeatTrim,
    center: [f64; 3],
) -> [f64; 3] {
    let shaft = trim.shaft_unit();
    let start = sub_center(finite_point(primitive.start_mm), center);
    let end = sub_center(finite_point(primitive.end_mm), center);
    match primitive.segment_kind {
        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => {
            let middle = midpoint(start, end);
            add(middle, scale(shaft, primitive.start_radius_mm.get()))
        }
        _ => {
            if dot(sub(end, start), shaft) >= 0.0 {
                end
            } else {
                start
            }
        }
    }
}

/// Split a cut record into its connected pieces (triangles that share a
/// corner, by index or by the same point to the bit) and keep the one nearest
/// `anchor`. Every other piece is returned with its volume and its distance
/// from `anchor`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a vertex index of a mesh the backend itself validated fits usize on every supported \
              target"
)]
fn keep_shaft_piece(kept: &plinth_solid::Mesh, anchor: [f64; 3]) -> KeptPiece {
    let vertices = kept.vertices();
    let triangles = kept.triangles();
    let mut parent: Vec<usize> = (0..vertices.len()).collect();
    // Corners at the same point are one corner, whatever their index.
    let mut first_at: std::collections::HashMap<[u64; 3], usize> =
        std::collections::HashMap::with_capacity(vertices.len());
    for (index, &vertex) in vertices.iter().enumerate() {
        let first = *first_at.entry(edge_key(vertex)).or_insert(index);
        parent[index] = first;
    }
    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    for triangle in triangles {
        let [a, b, c] = triangle.map(|corner| corner as usize);
        for (p, q) in [(a, b), (b, c)] {
            let (x, y) = (find(&mut parent, p), find(&mut parent, q));
            if x != y {
                parent[x.max(y)] = x.min(y);
            }
        }
    }
    // Pieces in order of their first triangle, so the answer never depends on
    // a hash order.
    let mut piece_of_root: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    let mut piece_of_triangle = Vec::with_capacity(triangles.len());
    for triangle in triangles {
        let root = find(&mut parent, triangle[0] as usize);
        let next = piece_of_root.len();
        piece_of_triangle.push(*piece_of_root.entry(root).or_insert(next));
    }
    let pieces = piece_of_root.len();
    let corners = |triangle: &[u64; 3]| triangle.map(|corner| vertices[corner as usize]);
    if pieces <= 1 {
        return KeptPiece {
            soup: triangles.iter().map(corners).collect(),
            dropped: Vec::new(),
        };
    }
    let mut volume = vec![0.0_f64; pieces];
    let mut distance = vec![f64::INFINITY; pieces];
    for (triangle, &piece) in triangles.iter().zip(&piece_of_triangle) {
        let [a, b, c] = corners(triangle);
        volume[piece] += dot(a, cross(b, c)) / 6.0;
        let closest = geom_kernel::closest_on_tri(anchor, a, b, c);
        distance[piece] = distance[piece].min(norm(sub(anchor, closest)));
    }
    // The nearest piece holds the shaft end; a tie keeps the first.
    let keep = (0..pieces)
        .min_by(|&x, &y| distance[x].total_cmp(&distance[y]).then(x.cmp(&y)))
        .unwrap_or(0);
    KeptPiece {
        soup: triangles
            .iter()
            .zip(&piece_of_triangle)
            .filter(|&(_, &piece)| piece == keep)
            .map(|(triangle, _)| corners(triangle))
            .collect(),
        dropped: (0..pieces)
            .filter(|&piece| piece != keep)
            .map(|piece| (volume[piece], distance[piece]))
            .collect(),
    }
}

/// Note the volume the model solid removed from one `Member`, joint or hull
/// record, when it is at least a millionth of a mm³ (below that is the boolean
/// backend's rounding). Tapers and tips are not noted: cutting them to the seat
/// is what the trim is for.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a removed volume is clamped non-negative and far below u64::MAX millionths of a mm3"
)]
fn note_member_cut(
    report: &mut SeatTrimReport,
    primitive: &SupportPrimitive,
    index: usize,
    removed_mm3: f64,
) {
    let kind = match (primitive.segment_kind, &primitive.ring_normal) {
        (SupportSegmentKind::Member, FrustumRingNormal::Junction(_)) => "hull",
        (SupportSegmentKind::Member, _) => "member",
        (SupportSegmentKind::JointSphere, _) => "joint",
        _ => return,
    };
    let micro = (removed_mm3.max(0.0) * 1.0e6).round();
    if micro >= 1.0 {
        report.member_cuts.push(MemberCut {
            record: index,
            kind: kind.to_owned(),
            removed_micro_mm3: micro as u64,
        });
    }
}

/// **Measure, without cutting, what every joint sphere and junction hull
/// stands inside the model by.** Neither carries a seat trim, so the trim never
/// cuts them; but a trim that reports only what it cut would hide an intrusion
/// it did not touch. ONE more batched call asks the boolean backend for each
/// piece's volume before and after the model is subtracted, with a half-space
/// that keeps everything; the cut meshes are thrown away. A call that fails is
/// named in `batch_error` and measures nothing.
fn measure_untrimmed_intrusions(
    primitives: &[SupportPrimitive],
    soups: &[Vec<SoupTriangle>],
    mesh: &Mesh,
    center: [f64; 3],
    report: &mut SeatTrimReport,
) {
    let wanted: Vec<usize> = primitives
        .iter()
        .enumerate()
        .filter(|(_, primitive)| {
            primitive.seat_trim.is_none()
                && match primitive.segment_kind {
                    SupportSegmentKind::JointSphere => true,
                    SupportSegmentKind::Member => {
                        matches!(primitive.ring_normal, FrustumRingNormal::Junction(_))
                    }
                    _ => false,
                }
        })
        .map(|(index, _)| index)
        .collect();
    let mut kept_indices = Vec::with_capacity(wanted.len());
    let mut pieces = Vec::with_capacity(wanted.len());
    let mut keep = Vec::with_capacity(wanted.len());
    for index in wanted {
        let Some(piece) = weld(&soups[index]) else {
            continue;
        };
        let lowest = piece
            .vertices()
            .iter()
            .map(|vertex| vertex[2])
            .fold(f64::INFINITY, f64::min);
        keep.push(plinth_solid::KeepHalfSpace {
            normal: [0.0, 0.0, 1.0],
            offset: lowest - 1.0,
        });
        pieces.push(piece);
        kept_indices.push(index);
    }
    if pieces.is_empty() {
        return;
    }
    let model_soup: Vec<SoupTriangle> = mesh
        .tris()
        .iter()
        .map(|triangle| triangle.map(|corner| sub_center(mesh.verts()[corner as usize], center)))
        .collect();
    let Some(solid) = weld(&model_soup) else {
        return;
    };
    match plinth_solid::ManifoldBackend.trim_and_subtract_each(
        &solid,
        &pieces,
        &keep,
        plinth_solid::BooleanLimits::UNLIMITED,
    ) {
        Ok(results) => {
            for (&index, result) in kept_indices.iter().zip(&results) {
                note_member_cut(
                    report,
                    &primitives[index],
                    index,
                    result.whole_volume - result.after_subtraction_volume,
                );
            }
        }
        Err(error) => {
            report.batch_error.get_or_insert_with(|| {
                format!("the intrusion measurement of joints and hulls did not run: {error}")
            });
        }
    }
}

/// One closed soup as an indexed mesh: corners that are the same point to the
/// bit become one vertex (the generators emit shared corners bit-identically;
/// `edge_key` folds the sign of zero), and a triangle a weld collapses is
/// dropped.
fn weld(soup: &[SoupTriangle]) -> Option<plinth_solid::Mesh> {
    let mut index: std::collections::HashMap<[u64; 3], u64> =
        std::collections::HashMap::with_capacity(soup.len());
    let mut vertices: Vec<[f64; 3]> = Vec::with_capacity(soup.len());
    let mut triangles: Vec<[u64; 3]> = Vec::with_capacity(soup.len());
    for triangle in soup {
        let corners = triangle.map(|point| {
            *index.entry(edge_key(point)).or_insert_with(|| {
                vertices.push(point);
                (vertices.len() - 1) as u64
            })
        });
        if corners[0] != corners[1] && corners[1] != corners[2] && corners[0] != corners[2] {
            triangles.push(corners);
        }
    }
    plinth_solid::Mesh::new(vertices, triangles).ok()
}

/// Everything a wire record needs besides its wound triangle soup.
#[derive(Clone, Copy, Debug)]
struct RecordFrame {
    segment_kind: SupportSegmentKind,
    start: [f64; 3],
    end: [f64; 3],
    r_start: f64,
    r_end: f64,
    contact_depth_mm: Option<f64>,
}

fn record_frame_and_soup(
    primitive: &SupportPrimitive,
    center: [f64; 3],
    frustum_sides: usize,
    raft_sides: usize,
) -> Result<(RecordFrame, Vec<SoupTriangle>), EmitError> {
    let mut r_start = primitive.start_radius_mm.get();
    let mut r_end = primitive.end_radius_mm.get();
    let (source_start, source_end) = match primitive.segment_kind {
        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => {
            // A joint sphere is drawn, and its record is written, at the
            // circumscribed vertex radius (`drawn_sphere_radius`), so the
            // poles, both record radii and the mesh agree.
            let drawn = PositiveFinite::new(drawn_sphere_radius(
                primitive.segment_kind,
                r_start,
            ))
            .map_err(|_| EmitError::RecordEncodingRejected)?;
            if primitive.segment_kind == SupportSegmentKind::JointSphere {
                r_start = drawn.get();
                r_end = drawn.get();
            }
            primitive
                .sphere_geometry
                .as_ref()
                .ok_or(EmitError::RecordEncodingRejected)?
                .canonical_poles(drawn)
                .map_err(|_| EmitError::RecordEncodingRejected)?
        }
        _ => (primitive.start_mm, primitive.end_mm),
    };
    let start = sub_center(finite_point(source_start), center);
    let end = sub_center(finite_point(source_end), center);
    let triangles = match primitive.segment_kind {
        // Frozen semantics: type 7 owns the contact sphere; type 9 owns a
        // standalone joint sphere; types 1 and 3 are frusta. A seated tip is
        // generated WHOLE here and cut to its seat by `trim_to_seats`, for the
        // whole field at once.
        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => {
            sphere_triangles(midpoint(start, end), r_start)?
        }
        SupportSegmentKind::RaftPad => raft_pad_triangles(start, r_start, r_end, raft_sides)?,
        SupportSegmentKind::RaftBridge => raft_bridge_triangles(start, end, r_start, r_end)?,
        SupportSegmentKind::Member
            if matches!(primitive.ring_normal, FrustumRingNormal::Junction(_)) =>
        {
            match &primitive.ring_normal {
                FrustumRingNormal::Junction(rings) => {
                    junction_hull_triangles(rings, center, frustum_sides)?
                }
                _ => return Err(EmitError::RecordEncodingRejected),
            }
        }
        SupportSegmentKind::ContactTaper
        | SupportSegmentKind::Member
        | SupportSegmentKind::RootFlare => frustum_triangles(
            start,
            end,
            r_start,
            r_end,
            frustum_sides,
            primitive.ring_normal,
        )?,
    };
    let contact_depth_mm = match primitive.segment_kind {
        SupportSegmentKind::ContactTaper => Some(
            primitive
                .contact_depth_mm
                .ok_or(EmitError::RecordEncodingRejected)?
                .get(),
        ),
        _ => None,
    };
    Ok((
        RecordFrame {
            segment_kind: primitive.segment_kind,
            start,
            end,
            r_start,
            r_end,
            contact_depth_mm,
        },
        triangles,
    ))
}

fn encode_record(
    frame: &RecordFrame,
    triangles: &[SoupTriangle],
) -> Result<SupportRecord, EmitError> {
    let mut record = SupportRecord::new(
        segment_type(frame.segment_kind),
        point_f32(frame.start)?,
        point_f32(frame.end)?,
        f32_of(frame.r_start)?,
        f32_of(frame.r_end)?,
        soup_from_triangles(triangles)?,
    );
    if let Some(depth) = frame.contact_depth_mm {
        record = record.with_contact_depth(f32_of(depth)?);
    }
    if matches!(
        frame.segment_kind,
        SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge
    ) {
        // THE NATIVE RAFT FRAME (fix/sweep-raft, 2026-09-29). Every type-6
        // record in every ChituBox-authored file we hold (5 files, every pad
        // and bridge) writes start/end z = 0.0, while its MESH sits at the
        // plate in the model-centred frame (`raft-diagnosis.md` §4.2). Until
        // this change the record carried the plate's model-frame z, which no
        // native file does; if ChituBox ever rebuilds a pad from its record it
        // would land that far under the plate. The mesh is unchanged: it was
        // built from the true plate height above.
        record.start[2] = 0.0;
        record.end[2] = 0.0;
        let is_bridge = frame.segment_kind == SupportSegmentKind::RaftBridge;
        record.trailing[0..4]
            .copy_from_slice(&(if is_bridge { 1.0_f32 } else { 0.0_f32 }).to_le_bytes());
        record.trailing[4..8].copy_from_slice(&4.0_f32.to_le_bytes());
    }
    Ok(record)
}

/// The sole exhaustive semantic-to-wire mapping.
const fn segment_type(kind: SupportSegmentKind) -> SegmentType {
    match kind {
        SupportSegmentKind::ContactSphere => SegmentType::TIP,
        SupportSegmentKind::ContactTaper => SegmentType::TAPER,
        SupportSegmentKind::JointSphere => SegmentType::SHAFT,
        SupportSegmentKind::Member => SegmentType::JOINER,
        SupportSegmentKind::RootFlare => SegmentType::FLARE,
        // Two distinct producer-side shapes, one wire identity: the format
        // itself does not distinguish a raft pad from a raft bridge (that is
        // what the `is_bridge` trailing flag in `encode_record` is for).
        SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge => SegmentType::PAD_OR_BRIDGE,
    }
}

fn finite_point(point: [plinth_core::Finite; 3]) -> [f64; 3] {
    point.map(plinth_core::Finite::get)
}

fn sub_center(point: [f64; 3], center: [f64; 3]) -> [f64; 3] {
    [
        point[0] - center[0],
        point[1] - center[1],
        point[2] - center[2],
    ]
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}

/// The disk format stores every frame coordinate as binary32.  This permits
/// only the bounded roundoff introduced by separately rounded subtraction,
/// not a changed plate or a second lift.
fn same_f32_frame_value(actual: f32, expected: f32) -> bool {
    let scale = actual.abs().max(expected.abs()).max(1.0);
    (actual - expected).abs() <= 32.0 * f32::EPSILON * scale
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "deliberate narrowing to the binary32 the printer file format stores. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn f32_of(value: f64) -> Result<f32, EmitError> {
    let value = value as f32;
    value
        .is_finite()
        .then_some(value)
        .ok_or(EmitError::RecordEncodingRejected)
}

fn point_f32(point: [f64; 3]) -> Result<[f32; 3], EmitError> {
    Ok([f32_of(point[0])?, f32_of(point[1])?, f32_of(point[2])?])
}

fn soup_from_triangles(triangles: &[SoupTriangle]) -> Result<TriangleSoup, EmitError> {
    let mut xyz = Vec::with_capacity(triangles.len() * 9);
    for triangle in triangles {
        for point in triangle {
            xyz.extend(point_f32(*point)?);
        }
    }
    Ok(TriangleSoup::new(xyz))
}

fn frustum_triangles(
    start: [f64; 3],
    end: [f64; 3],
    r_start: f64,
    r_end: f64,
    sides: usize,
    ring_normal: FrustumRingNormal,
) -> Result<Vec<SoupTriangle>, EmitError> {
    let plate_z = match ring_normal {
        FrustumRingNormal::SegmentAxis => false,
        FrustumRingNormal::PlateZ => true,
        // A junction hull is not a frustum; `junction_hull_triangles` draws it.
        FrustumRingNormal::Junction(_) => return Err(EmitError::RecordEncodingRejected),
    };
    // THE ONE RING ROUTINE (`plinth_support::frustum_ring`): the same code
    // makes a junction hull's rings from this frustum's own `start`, `end` and
    // radius, so a hull ring equals this frustum's ring bit for bit.
    let ring_start = plinth_support::frustum_ring(start, end, r_start, true, plate_z, sides)
        .ok_or(EmitError::RecordEncodingRejected)?;
    let ring_end = plinth_support::frustum_ring(start, end, r_end, false, plate_z, sides)
        .ok_or(EmitError::RecordEncodingRejected)?;
    let mut triangles = Vec::with_capacity(sides * 4);
    for index in 0..sides {
        let next = (index + 1) % sides;
        triangles.push([ring_start[index], ring_end[next], ring_end[index]]);
        triangles.push([ring_start[index], ring_start[next], ring_end[next]]);
        triangles.push([start, ring_start[next], ring_start[index]]);
        triangles.push([end, ring_end[index], ring_end[next]]);
    }
    Ok(triangles)
}

/// `(sin, cos)` per latitude index `0..=SPHERE_LATITUDES`, built once and
/// shared by every sphere this crate emits.
///
/// The poles (`k == 0`, `k == SPHERE_LATITUDES`) and the equator (when
/// `SPHERE_LATITUDES` is even) are pinned to their exact values rather than
/// read off `sin`/`cos`, because `sin(PI)` is `1.2246e-16`, not `0.0`, and
/// `cos(PI/2)` is `6.1232e-17`, not `0.0` — see FU-36. Everything past the
/// equator is filled by an exact mirror of the value already computed for
/// the matching northern index (`(sin, cos) -> (sin, -cos)`), rather than by
/// a second, independently-rounded `sin_cos()` call, so the table is closed
/// under negation by construction and not by luck of the rounding.
static LATITUDE_TABLE: LazyLock<[(f64, f64); SPHERE_LATITUDES + 1]> =
    LazyLock::new(build_latitude_table);

/// `(sin, cos)` per longitude index `0..SPHERE_LONGITUDES`, built once and
/// shared by every sphere this crate emits. Indexed with `% SPHERE_LONGITUDES`
/// at the wrap, exactly as the per-vertex trig did, so index `SPHERE_LONGITUDES`
/// reuses index `0` bit-for-bit.
static LONGITUDE_TABLE: LazyLock<[(f64, f64); SPHERE_LONGITUDES]> =
    LazyLock::new(build_longitude_table);

fn build_latitude_table() -> [(f64, f64); SPHERE_LATITUDES + 1] {
    let mut table = [(0.0_f64, 0.0_f64); SPHERE_LATITUDES + 1];
    for k in 0..=SPHERE_LATITUDES {
        table[k] = if k == 0 {
            (0.0, 1.0)
        } else if k == SPHERE_LATITUDES {
            (0.0, -1.0)
        } else if 2 * k == SPHERE_LATITUDES {
            (1.0, 0.0)
        } else if 2 * k < SPHERE_LATITUDES {
            (std::f64::consts::PI * k as f64 / SPHERE_LATITUDES as f64).sin_cos()
        } else {
            // 2*k > SPHERE_LATITUDES: exact mirror of the northern index
            // already filled earlier in this same loop (SPHERE_LATITUDES - k < k here).
            let (sin, cos) = table[SPHERE_LATITUDES - k];
            (sin, -cos)
        };
    }
    table
}

fn build_longitude_table() -> [(f64, f64); SPHERE_LONGITUDES] {
    let mut table = [(0.0_f64, 0.0_f64); SPHERE_LONGITUDES];
    for (j, slot) in table.iter_mut().enumerate() {
        *slot = (std::f64::consts::TAU * j as f64 / SPHERE_LONGITUDES as f64).sin_cos();
    }
    table
}

fn sphere_triangles(center: [f64; 3], radius: f64) -> Result<Vec<SoupTriangle>, EmitError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(EmitError::RecordEncodingRejected);
    }
    Ok(sphere_mesh(center, radius))
}

/// The joint sphere's circumscribe factor `k`: the smallest distance from the
/// sphere's centre to any triangle PLANE of the 8x16 sphere mesh, over that
/// triangle's vertex radius. The mesh's flat faces sit inside its vertex
/// sphere, up to `1 - k` of the radius, so a round rim of radius `r` centred
/// on the joint (a head's base, a post's top) pokes through the faces of a
/// mesh drawn at vertex radius `r`. Drawn at vertex radius `r / k`, every
/// face plane stands at least `r` from the centre and the mesh contains the
/// true sphere of radius `r`, and so every such rim. Derived from the mesh's
/// own triangles, never typed.
///
/// The mesh is convex (a latitude/longitude cell's four corners are a planar
/// isosceles trapezoid, so its two triangles share one plane; the poles are
/// single triangles), so the planes' minimum distance is the inradius.
static JOINT_CIRCUMSCRIBE: LazyLock<f64> = LazyLock::new(joint_circumscribe_factor);

fn joint_circumscribe_factor() -> f64 {
    let origin = [0.0; 3];
    sphere_mesh(origin, 1.0)
        .iter()
        .filter_map(|[a, b, c]| {
            let normal = cross(sub(*b, *a), sub(*c, *a));
            let length = norm(normal);
            let vertex_radius = norm(*a).max(norm(*b)).max(norm(*c));
            (length > 0.0 && vertex_radius > 0.0)
                .then(|| geom_kernel::vec3::dot(normal, *a).abs() / length / vertex_radius)
        })
        .fold(f64::INFINITY, f64::min)
}

/// The vertex radius a sphere record is DRAWN at, and written with, for a
/// primitive of kind `kind` and registered radius `radius`: a joint sphere is
/// circumscribed ([`JOINT_CIRCUMSCRIBE`]) so its flat faces contain the true
/// sphere; a contact sphere is drawn at its registered radius, as the seat
/// trim and the contact area read it.
fn drawn_sphere_radius(kind: SupportSegmentKind, radius: f64) -> f64 {
    match kind {
        SupportSegmentKind::JointSphere => radius / *JOINT_CIRCUMSCRIBE,
        _ => radius,
    }
}

fn sphere_mesh(center: [f64; 3], radius: f64) -> Vec<SoupTriangle> {
    let lat_table = &*LATITUDE_TABLE;
    let lon_table = &*LONGITUDE_TABLE;
    let mut triangles = Vec::with_capacity(2 * SPHERE_LONGITUDES * (SPHERE_LATITUDES - 1));
    for latitude in 0..SPHERE_LATITUDES {
        let (sin_a0, cos_a0) = lat_table[latitude];
        let (sin_a1, cos_a1) = lat_table[latitude + 1];
        for longitude in 0..SPHERE_LONGITUDES {
            let (sin_l0, cos_l0) = lon_table[longitude];
            let (sin_l1, cos_l1) = lon_table[(longitude + 1) % SPHERE_LONGITUDES];
            let p00 = sphere_point(center, radius, sin_a0, cos_a0, sin_l0, cos_l0);
            let p01 = sphere_point(center, radius, sin_a0, cos_a0, sin_l1, cos_l1);
            let p10 = sphere_point(center, radius, sin_a1, cos_a1, sin_l0, cos_l0);
            let p11 = sphere_point(center, radius, sin_a1, cos_a1, sin_l1, cos_l1);
            if latitude + 1 < SPHERE_LATITUDES {
                triangles.push([p00, p10, p11]);
            }
            if latitude > 0 {
                triangles.push([p00, p11, p01]);
            }
        }
    }
    triangles
}

/// A point's identity for edge pairing: bit patterns, with the sign of zero
/// normalized because `0.0` and `-0.0` are the same point and the sphere
/// generator produces both.
fn edge_key(point: [f64; 3]) -> [u64; 3] {
    point.map(|value| if value == 0.0 { 0.0 } else { value }.to_bits())
}

/// Same multiply associativity as the original per-vertex trig
/// (`center + (radius*sin)*cos`), so no ring vertex moves for a reason *this
/// function* introduces. All movement versus the pre-fix generator comes from
/// `LATITUDE_TABLE`, and it is **not** confined to the poles and the equator.
/// Measured against the pre-fix rule (raw `theta.sin()`/`theta.cos()` at
/// `theta = PI*k/SPHERE_LATITUDES`): `k = 0..=3` — the north pole and all three
/// northern rings — are bit-identical, while every entry from the equator
/// southward moves. `k = 4` cos (pinned `0.0` against `6.1232e-17`) and `k = 8`
/// sin (pinned `0.0` against `1.2246e-16`) are the intended pinnings; `k = 5`
/// cos, `k = 6` sin *and* cos, and `k = 7` sin each move by 1 ulp because the
/// exact northern mirror replaces a second, independently-rounded `sin_cos()`.
/// So southern mid-band ring vertices can move on the wire too, not only the
/// pole and equator rings. Per-record byte movement was measured rather than
/// inferred from this: see the FU-36 A/B table in
/// `docs/specs/chitubox-format.md`.
fn sphere_point(
    center: [f64; 3],
    radius: f64,
    sin_lat: f64,
    cos_lat: f64,
    sin_lon: f64,
    cos_lon: f64,
) -> [f64; 3] {
    [
        center[0] + radius * sin_lat * cos_lon,
        center[1] + radius * sin_lat * sin_lon,
        center[2] + radius * cos_lat,
    ]
}

fn raft_pad_triangles(
    start: [f64; 3],
    diameter: f64,
    height: f64,
    raft_sides: usize,
) -> Result<Vec<SoupTriangle>, EmitError> {
    if !diameter.is_finite() || !height.is_finite() || height <= 0.0 {
        return Err(EmitError::RecordEncodingRejected);
    }
    frustum_triangles(
        start,
        add(start, [0.0, 0.0, height]),
        diameter * 0.5,
        diameter * 0.5,
        raft_sides,
        FrustumRingNormal::SegmentAxis,
    )
}

fn raft_bridge_triangles(
    start: [f64; 3],
    end: [f64; 3],
    width: f64,
    height: f64,
) -> Result<Vec<SoupTriangle>, EmitError> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(EmitError::RecordEncodingRejected);
    }
    let axis = [end[0] - start[0], end[1] - start[1], 0.0];
    let length = norm(axis);
    if !length.is_finite() || length <= f64::EPSILON {
        return Err(EmitError::RecordEncodingRejected);
    }
    let across = [
        -axis[1] / length * width * 0.5,
        axis[0] / length * width * 0.5,
        0.0,
    ];
    let lo = [
        sub(start, across),
        add(start, across),
        add(end, across),
        sub(end, across),
    ];
    let hi = lo.map(|point| add(point, [0.0, 0.0, height]));
    let vertices = [lo[0], lo[1], lo[2], lo[3], hi[0], hi[1], hi[2], hi[3]];
    const FACES: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Ok(FACES
        .map(|face| [vertices[face[0]], vertices[face[1]], vertices[face[2]]])
        .to_vec())
}

/// The triangles of a junction hull: the convex hull of the end rings it names.
/// Each ring is made by the one ring routine from its neighbour frustum's own
/// start and end (centre-subtracted exactly as that frustum's record is), so the
/// hull's ring corners are the neighbour's ring corners bit for bit and are
/// vertices of the hull.
fn junction_hull_triangles(
    rings: &plinth_support::JunctionRings,
    center: [f64; 3],
    sides: usize,
) -> Result<Vec<SoupTriangle>, EmitError> {
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(3 * sides);
    for ring in rings.rings() {
        let corners = plinth_support::frustum_ring(
            sub_center(finite_point(ring.frustum_start_mm), center),
            sub_center(finite_point(ring.frustum_end_mm), center),
            ring.radius_mm.get(),
            ring.at_start,
            ring.plate_z,
            sides,
        )
        .ok_or(EmitError::RecordEncodingRejected)?;
        points.extend(corners);
    }
    plinth_support::convex_hull(&points)
        .map(|hull| hull.soup())
        .ok_or(EmitError::RecordEncodingRejected)
}

/// Wind a whole gathered batch of soups outward in one kernel call.
///
/// The f64 tetrahedron sum itself lives in `geom_kernel` (O-5: one
/// implementation per capability; the docs/specs/chitubox-format.md ruling that
/// replaced Python's trimesh `mass_properties` winding check).  This crate
/// keeps only the boundary mapping onto its own error enum, because a soup a
/// signed volume cannot be stated for is exactly a record that cannot be
/// encoded.
fn wind_outward(soups: &mut [Vec<SoupTriangle>]) -> Result<(), EmitError> {
    geom_kernel::wind_soups_outward(soups)
        .map(|_| ())
        .map_err(|_| EmitError::RecordEncodingRejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{m0_constants, Finite, PositiveFinite};
    use plinth_decimate::{run as run_decimation, DecimationAudit, DecimationBudget, ProbeConfig};
    use plinth_hollow::{HollowHistory, HollowReport, SolidDecisionReason};
    use plinth_orient::{
        BaselineSelection, OrientationHistory, OrientationProvenance, OrientationReport,
        RigidTransform,
    };
    use plinth_support::{
        BuildFrame, FrustumRingNormal, SphereGeometry, SupportHistory, SupportReport,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Single-primitive convenience over the batched builder.  Only tests hold
    /// exactly one primitive; production always hands over a whole forest.
    fn build_record(
        primitive: &SupportPrimitive,
        center: [f64; 3],
    ) -> Result<SupportRecord, EmitError> {
        build_records(std::slice::from_ref(primitive), center)?
            .into_iter()
            .next()
            .ok_or(EmitError::RecordEncodingRejected)
    }

    /// Triangles as f64, read back out of an already-encoded wire soup.
    fn wire_triangles(xyz: &[f32]) -> Vec<SoupTriangle> {
        xyz.chunks_exact(9)
            .map(|t| {
                [
                    [t[0] as f64, t[1] as f64, t[2] as f64],
                    [t[3] as f64, t[4] as f64, t[5] as f64],
                    [t[6] as f64, t[7] as f64, t[8] as f64],
                ]
            })
            .collect()
    }

    /// `0.0` and `-0.0` are the same real number and the same point in
    /// space; the sphere generator's own arithmetic produces both signs at
    /// the equator and at the poles depending on longitude (see
    /// `sphere_point`'s doc comment). Bit-key helpers below normalize the
    /// sign of zero so a vertex-identity or symmetry check never reports a
    /// false mismatch on that alone — every non-zero value is still
    /// compared bit-for-bit, which is where an actual defect would show up.
    fn normalize_zero(value: f64) -> f64 {
        if value == 0.0 {
            0.0
        } else {
            value
        }
    }

    /// Bit-exact identity key for a point, modulo the sign of zero (see
    /// `normalize_zero`).
    fn point_bits(point: [f64; 3]) -> [u64; 3] {
        point.map(|value| normalize_zero(value).to_bits())
    }

    /// Rotate a triangle's three vertices, preserving winding, so the
    /// lexicographically smallest bit-keyed vertex comes first. Two
    /// triangles that are the same cycle starting at a different vertex
    /// canonicalize to the same result, which is what makes a triangle
    /// multiset comparable by sorting.
    fn canonicalize_triangle(triangle: SoupTriangle) -> SoupTriangle {
        let keys = triangle.map(point_bits);
        let min_index = (0..3).min_by_key(|&index| keys[index]).expect("3 corners");
        [
            triangle[min_index],
            triangle[(min_index + 1) % 3],
            triangle[(min_index + 2) % 3],
        ]
    }

    /// Reflect a triangle across the `z == 0` plane and reverse its winding,
    /// which is what a correctly-wound outward triangle on one side of a
    /// mirror-symmetric solid must become on the other side (mirroring
    /// reverses chirality; winding must reverse to stay outward-facing).
    fn mirror_z_reverse_winding(triangle: SoupTriangle) -> SoupTriangle {
        let flip = |point: [f64; 3]| [point[0], point[1], -point[2]];
        [flip(triangle[0]), flip(triangle[2]), flip(triangle[1])]
    }

    /// Independent orientation oracle.  Every primitive this crate synthesizes
    /// — frustum, sphere, raft pad, raft bridge — is a convex body, and every
    /// face of a convex body wound outward has its geometric normal pointing
    /// away from the body's own centroid.  This decides orientation from local
    /// face normals alone and never forms a tetrahedron sum, so it does not
    /// restate the algorithm under test.
    ///
    /// Returns `(faces_checked, faces_pointing_outward)`; degenerate faces
    /// carry no orientation and are excluded from both counts.
    fn faces_pointing_away_from_centroid(triangles: &[SoupTriangle]) -> (usize, usize) {
        let mut centroid = [0.0_f64; 3];
        let mut corners = 0.0;
        for triangle in triangles {
            for point in triangle {
                for (axis, value) in point.iter().enumerate() {
                    centroid[axis] += value;
                }
                corners += 1.0;
            }
        }
        if corners == 0.0 {
            return (0, 0);
        }
        for value in &mut centroid {
            *value /= corners;
        }
        let mut checked = 0;
        let mut outward = 0;
        for triangle in triangles {
            let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
            if norm(normal) <= 0.0 {
                continue;
            }
            checked += 1;
            let face_centroid = scale(add(add(triangle[0], triangle[1]), triangle[2]), 1.0 / 3.0);
            if dot(normal, sub(face_centroid, centroid)) > 0.0 {
                outward += 1;
            }
        }
        (checked, outward)
    }

    fn assert_convex_soup_is_wound_outward(triangles: &[SoupTriangle], what: &str) {
        let (checked, outward) = faces_pointing_away_from_centroid(triangles);
        assert!(checked > 0, "{what} carries no non-degenerate face");
        assert_eq!(
            checked,
            outward,
            "{what} has {} of {checked} faces wound inward",
            checked - outward
        );
    }

    fn fixture_path(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "plinth-emit-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn printer() -> PrinterProfile {
        m0_constants()
            .expect("fixture constants")
            .saturn4_ultra()
            .clone()
    }

    fn fixture_mesh() -> Mesh {
        Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .expect("fixture mesh")
    }

    fn fixture_decimation_audit() -> DecimationAudit {
        let mesh = plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered fixture budget");
        let outcome = run_decimation(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        audit
    }

    fn fixture_history(primitives: &[SupportPrimitive], lift_mm: f64) -> SupportHistory {
        let decimation = fixture_decimation_audit();
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("fixture.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        let hollow = HollowHistory::from_reports(
            orientation,
            HollowReport::solid_decision(SolidDecisionReason::BelowThreshold),
        );
        let raft_count = primitives
            .iter()
            .filter(|primitive| {
                matches!(
                    primitive.segment_kind,
                    SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge
                )
            })
            .count();
        let frame = BuildFrame::new(
            // Fixture model minimum is 0: its sole lift establishes the
            // planned plate in the same un-centred support frame.
            Finite::new(-lift_mm).unwrap(),
            PositiveFinite::new(lift_mm).unwrap(),
        );
        SupportHistory::from_reports(
            hollow,
            SupportReport {
                support_primitive_count: primitives.len() - raft_count,
                raft_primitive_count: raft_count,
            },
            frame,
        )
    }

    fn fixture_model(primitives: Vec<SupportPrimitive>, lift_mm: f64) -> SupportedModel {
        let history = fixture_history(&primitives, lift_mm);
        SupportedModel::from_parts(fixture_mesh(), primitives, history).expect("fixture scene")
    }

    fn primitive(
        segment_kind: SupportSegmentKind,
        start: [f64; 3],
        end: [f64; 3],
    ) -> SupportPrimitive {
        let start_radius_mm = PositiveFinite::new(2.2).unwrap();
        let sphere_geometry = match segment_kind {
            SupportSegmentKind::ContactSphere => {
                let midpoint = midpoint(start, end);
                Some(
                    SphereGeometry::tip(
                        midpoint.map(|value| Finite::new(value).unwrap()),
                        sub(end, start).map(|value| Finite::new(value).unwrap()),
                    )
                    .unwrap(),
                )
            }
            SupportSegmentKind::JointSphere => Some(SphereGeometry::joint(
                midpoint(start, end).map(|value| Finite::new(value).unwrap()),
            )),
            _ => None,
        };
        let (start_mm, end_mm) = sphere_geometry.as_ref().map_or_else(
            || {
                (
                    start.map(|value| Finite::new(value).unwrap()),
                    end.map(|value| Finite::new(value).unwrap()),
                )
            },
            |geometry| {
                geometry
                    .canonical_poles(start_radius_mm)
                    .expect("fixture sphere geometry remains finite")
            },
        );
        SupportPrimitive {
            segment_kind,
            start_mm,
            end_mm,
            start_radius_mm,
            end_radius_mm: PositiveFinite::new(
                if matches!(
                    segment_kind,
                    SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
                ) {
                    2.2
                } else {
                    0.2
                },
            )
            .unwrap(),
            contact_depth_mm: (segment_kind == SupportSegmentKind::ContactTaper).then(|| {
                m0_constants()
                    .unwrap()
                    .placeholders()
                    .default_contact_depth_mm()
                    .diagnostic()
                    .to_owned()
            }),
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry,
            seat_trim: None,
        }
    }

    fn primitive_with_radii(
        segment_kind: SupportSegmentKind,
        start: [f64; 3],
        end: [f64; 3],
        start_radius: f64,
        end_radius: f64,
    ) -> SupportPrimitive {
        let mut primitive = primitive(segment_kind, start, end);
        primitive.start_radius_mm = PositiveFinite::new(start_radius).unwrap();
        primitive.end_radius_mm = PositiveFinite::new(end_radius).unwrap();
        if let Some(geometry) = &primitive.sphere_geometry {
            (primitive.start_mm, primitive.end_mm) = geometry
                .canonical_poles(primitive.start_radius_mm)
                .expect("fixture sphere geometry remains finite");
        }
        primitive
    }

    fn record_bounds(record: &SupportRecord) -> Bounds {
        let mut bounds = Bounds::empty();
        bounds.extend_f32(&record.mesh.xyz).unwrap();
        bounds
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn frozen_contact_joiner_and_joint_ball_records_keep_their_distinct_roles() {
        // This is the frozen `member_chain_specs` vocabulary: a type-7
        // contact sphere, adjacent type-1 taper carrying configured depth,
        // nonzero type-3 member frustum, and a standalone type-9 joint ball.
        let depth = m0_constants()
            .unwrap()
            .placeholders()
            .default_contact_depth_mm()
            .diagnostic()
            .get();
        let records = build_records(
            &[
                primitive_with_radii(
                    SupportSegmentKind::ContactSphere,
                    [1.0, 2.0, 3.0],
                    [1.0, 2.0, 5.0],
                    1.0,
                    1.0,
                ),
                primitive_with_radii(
                    SupportSegmentKind::ContactTaper,
                    [1.0, 2.0, 4.0],
                    [1.0, 2.0, 8.0],
                    0.8,
                    0.3,
                ),
                primitive_with_radii(
                    SupportSegmentKind::Member,
                    [1.0, 2.0, 8.0],
                    [1.0, 2.0, 11.0],
                    0.6,
                    0.4,
                ),
                primitive_with_radii(
                    SupportSegmentKind::JointSphere,
                    [1.0, 2.0, 14.0],
                    [1.0, 2.0, 10.0],
                    0.75,
                    0.75,
                ),
            ],
            [0.0; 3],
        )
        .unwrap();
        assert_eq!(records[0].segment_type, SegmentType::TIP);
        assert_eq!(records[0].start, [1.0, 2.0, 3.0]);
        assert_eq!(records[0].end, [1.0, 2.0, 5.0]);
        let tip_bounds = record_bounds(&records[0]);
        assert_eq!((tip_bounds.min[2], tip_bounds.max[2]), (3.0, 5.0));

        assert_eq!(records[1].segment_type, SegmentType::TAPER);
        assert_eq!(records[1].start, [1.0, 2.0, 4.0]);
        assert_eq!(records[1].end, [1.0, 2.0, 8.0]);
        assert_eq!(
            f32::from_le_bytes(records[1].trailing[0..4].try_into().unwrap()),
            depth as f32
        );
        let taper_bounds = record_bounds(&records[1]);
        assert_eq!((taper_bounds.min[2], taper_bounds.max[2]), (4.0, 8.0));

        assert_eq!(records[2].segment_type, SegmentType::JOINER);
        assert_eq!(records[2].start, [1.0, 2.0, 8.0]);
        assert_eq!(records[2].end, [1.0, 2.0, 11.0]);
        let joiner_bounds = record_bounds(&records[2]);
        assert_eq!((joiner_bounds.min[2], joiner_bounds.max[2]), (8.0, 11.0));

        assert_eq!(records[3].segment_type, SegmentType::SHAFT);
        // A joint ball is written at its circumscribed vertex radius, poles
        // and both record radii alike (`drawn_sphere_radius`).
        let drawn = 0.75 / *JOINT_CIRCUMSCRIBE;
        assert_eq!(
            records[3].start,
            [1.0, 2.0, point_f32([1.0, 2.0, 12.0 + drawn]).unwrap()[2]]
        );
        assert_eq!(
            records[3].end,
            [1.0, 2.0, point_f32([1.0, 2.0, 12.0 - drawn]).unwrap()[2]]
        );
        assert_eq!(records[3].r_start, f32_of(drawn).unwrap());
        assert_eq!(records[3].r_end, f32_of(drawn).unwrap());
        let shaft_bounds = record_bounds(&records[3]);
        assert!((shaft_bounds.min[2] - (12.0 - drawn)).abs() < 1e-5);
        assert!((shaft_bounds.max[2] - (12.0 + drawn)).abs() < 1e-5);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_junction_hull_is_one_closed_outward_member_record_on_the_neighbours_own_rings() {
        let center = [1.0, 2.0, 3.0];
        let radius = |value: f64| PositiveFinite::new(value).unwrap();
        let member = |start: [f64; 3], end: [f64; 3], r_start: f64, r_end: f64| {
            primitive_with_radii(SupportSegmentKind::Member, start, end, r_start, r_end)
        };
        // A landed branch leaning 41 degrees onto a host: the host above (thin),
        // the host below (wide) and the branch, each ending on a ring near K.
        let above = member([1.0, 2.0, 6.0], [1.0, 2.0, 3.0], 0.2342, 0.2342);
        let below = member([1.0, 2.0, 2.7], [1.0, 2.0, 0.0], 0.761, 0.761);
        let branch = member([3.0, 2.0, 5.5], [1.2, 2.0, 3.2], 0.761, 0.761);
        let ring = |frustum: &SupportPrimitive, at_start: bool| plinth_support::HullRing {
            frustum_start_mm: frustum.start_mm,
            frustum_end_mm: frustum.end_mm,
            radius_mm: if at_start {
                frustum.start_radius_mm
            } else {
                frustum.end_radius_mm
            },
            at_start,
            plate_z: false,
        };
        let rings = plinth_support::JunctionRings([
            Some(ring(&branch, false)),
            Some(ring(&below, true)),
            Some(ring(&above, false)),
        ]);
        let hull = SupportPrimitive {
            segment_kind: SupportSegmentKind::Member,
            start_mm: branch.end_mm,
            end_mm: below.start_mm,
            start_radius_mm: radius(0.761),
            end_radius_mm: radius(0.761),
            contact_depth_mm: None,
            ring_normal: FrustumRingNormal::Junction(rings),
            sphere_geometry: None,
            seat_trim: None,
        };
        let primitives = [above.clone(), branch.clone(), below.clone(), hull.clone()];
        // The record: a Member (type 3) with the two largest rings' centres.
        let records = build_records(&primitives, center).unwrap();
        let record = &records[3];
        assert_eq!(record.segment_type, SegmentType::JOINER);
        assert_eq!(record.start, point_f32(sub_center(finite_point(branch.end_mm), center)).unwrap());
        assert_eq!(record.end, point_f32(sub_center(finite_point(below.start_mm), center)).unwrap());
        // Closed, wound outward, positive volume.
        let triangles = wire_triangles(&record.mesh.xyz);
        assert_watertight(&triangles, "junction hull");
        assert_convex_soup_is_wound_outward(&triangles, "junction hull");
        let volume: f64 = triangles
            .iter()
            .map(|t| dot(t[0], cross(t[1], t[2])))
            .sum::<f64>()
            / 6.0;
        assert!(volume > 0.0, "volume {volume}");
        // Its rings ARE the neighbours' rings, bit for bit (before the f32 cast).
        let (_, hull_soup) = record_frame_and_soup(&hull, center, 24, 8).unwrap();
        let hull_corners: std::collections::HashSet<[u64; 3]> =
            hull_soup.iter().flatten().map(|p| point_bits(*p)).collect();
        for (frustum, at_start) in [(&branch, false), (&below, true), (&above, false)] {
            let (_, soup) = record_frame_and_soup(frustum, center, 24, 8).unwrap();
            let start = sub_center(finite_point(frustum.start_mm), center);
            let end = sub_center(finite_point(frustum.end_mm), center);
            let (ring_centre, ring_radius) = if at_start {
                (start, frustum.start_radius_mm.get())
            } else {
                (end, frustum.end_radius_mm.get())
            };
            let axis = geom_kernel::vec3::normalize(sub(end, start)).unwrap();
            let on_ring: Vec<[f64; 3]> = soup
                .iter()
                .flatten()
                .copied()
                .filter(|p| {
                    dot(sub(*p, ring_centre), axis).abs() < 1e-9
                        && (norm(sub(*p, ring_centre)) - ring_radius).abs() < 1e-9
                })
                .collect();
            assert!(!on_ring.is_empty());
            let found = on_ring
                .iter()
                .filter(|p| hull_corners.contains(&point_bits(**p)))
                .count();
            // The wide rings are the hull's boundary; the thin ring of the host
            // above is buried in it and need not be a vertex.
            if ring_radius > 0.5 {
                assert!(
                    on_ring.iter().all(|p| hull_corners.contains(&point_bits(*p))),
                    "only {found} of {} ring corners are hull vertices",
                    on_ring.len()
                );
            }
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn plate_root_joiner_uses_frozen_horizontal_ring_normal() {
        let mut root = primitive_with_radii(
            SupportSegmentKind::Member,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 2.0],
            0.6,
            0.4,
        );
        root.ring_normal = FrustumRingNormal::PlateZ;
        let record = build_record(&root, [0.0; 3]).unwrap();
        assert_eq!(record.start, [0.0, 0.0, 0.0]);
        assert_eq!(record.end, [1.0, 0.0, 2.0]);
        assert!(record.mesh.xyz.chunks_exact(3).all(|point| {
            point[2].to_bits() == 0.0_f32.to_bits() || point[2].to_bits() == 2.0_f32.to_bits()
        }));

        let axis_record = build_record(
            &primitive_with_radii(
                SupportSegmentKind::Member,
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 2.0],
                0.6,
                0.4,
            ),
            [0.0; 3],
        )
        .unwrap();
        assert!(axis_record
            .mesh
            .xyz
            .chunks_exact(3)
            .any(|point| point[2] != 0.0 && point[2] != 2.0));
    }

    #[test]
    fn model_winding_is_fixed_in_f64_before_wire_cast() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .unwrap();
        // Same tetrahedron, every face reversed.  Emitting both pins that the
        // correction happens, not merely that one lucky input needed none.
        let reversed = Mesh::new(
            mesh.verts().to_vec(),
            mesh.tris()
                .iter()
                .map(|face| [face[0], face[2], face[1]])
                .collect(),
        )
        .unwrap();
        for (source, label) in [
            (&mesh, "the outward tetrahedron"),
            (&reversed, "the reversed one"),
        ] {
            let soup = model_triangle_soup(source, mesh_bbox_center(source).unwrap()).unwrap();
            assert_convex_soup_is_wound_outward(&wire_triangles(&soup.xyz), label);
        }
    }

    #[test]
    fn record_builder_covers_current_segment_vocabulary_in_order() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 3.0],
            ),
            primitive(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 6.0],
            ),
            primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 6.0],
                [0.0, 0.0, 9.0],
            ),
            primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, 9.0],
                [0.0, 0.0, 11.0],
            ),
            primitive(
                SupportSegmentKind::RootFlare,
                [0.0, 0.0, 11.0],
                [0.0, 0.0, 12.0],
            ),
            primitive(
                SupportSegmentKind::RaftBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPad,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        let records = build_records(&primitives, [0.0; 3]).unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.segment_type)
                .collect::<Vec<_>>(),
            primitives
                .iter()
                .map(|primitive| segment_type(primitive.segment_kind))
                .collect::<Vec<_>>()
        );
        for (index, record) in records.iter().enumerate() {
            assert!(!record.mesh.xyz.is_empty());
            assert!(record.mesh.xyz.iter().all(|value| value.is_finite()));
            assert_convex_soup_is_wound_outward(
                &wire_triangles(&record.mesh.xyz),
                &format!("record {index} ({:?})", record.segment_type),
            );
        }
    }

    /// Unmatched surface of a soup, as a fraction of its total area.
    ///
    /// The total outward area vector `Σ (v1-v0)×(v2-v0)` cancels to zero over a
    /// closed surface and equals the missing patch over an open one, so its
    /// norm over the soup's total area is a scale-free openness measure.  It is
    /// also *exactly* the term that makes the tetrahedron sum frame-dependent:
    /// translating a soup by `t` shifts the sum by `t · Σ(v1-v0)×(v2-v0) / 6`.
    /// Measuring it needs no vertex welding, which §2 of 06-conventions.md
    /// forbids as a production habit for good reason.
    fn relative_closure_defect(triangles: &[SoupTriangle]) -> f64 {
        let mut total = [0.0_f64; 3];
        let mut area = 0.0;
        for triangle in triangles {
            let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
            for (axis, value) in normal.iter().enumerate() {
                total[axis] += value;
            }
            area += norm(normal);
        }
        if area == 0.0 {
            return 0.0;
        }
        norm(total) / area
    }

    /// The precondition the whole winding decision rests on.
    ///
    /// `Σ v0 · (v1 × v2) / 6` is only translation-invariant — and therefore
    /// only sign-stable — for a CLOSED soup.  Native
    /// `morel_mudskipper_supports.chitubox` proves the negative case: its
    /// merged, open records read positive or negative purely by where they sit
    /// (see `tests/native_winding_oracle.rs`).  Every soup this crate emits is
    /// one closed convex primitive, and that is what this test pins.
    ///
    /// FU-36 (2026-08-12 ruling): the south-pole seam this bound used to
    /// accommodate is repaired — `sphere_triangles` now closes both caps on
    /// an exact single apex (`center[2] ± radius`, via a shared latitude
    /// table pinned at the poles and the equator rather than raw
    /// `sin`/`cos`), and the mesh is exactly north/south symmetric. See
    /// `every_emitted_sphere_is_a_closed_two_manifold`,
    /// `both_pole_caps_are_exact_single_apex_fans`, and
    /// `the_generated_sphere_is_exactly_north_south_symmetric` below for the
    /// invariants the ruling asked for.
    ///
    /// The bound is 1e-15 rather than exactly zero because the residual
    /// after the fix is f32 wire-quantization noise, not a seam: reading
    /// every fixture record back through `wire_triangles` (f32) measured a
    /// maximum defect of 1.5200e-17 in this crate's own test suite on
    /// 2026-08-12 (record 2, the JointSphere, origin frame — every record in
    /// the displaced frame reads exactly `0`). 1e-15 leaves ~65x headroom
    /// over that measured maximum. As always: the bound is never widened —
    /// a future closure defect above it means the geometry is wrong, not
    /// the bound.
    #[test]
    fn every_emitted_soup_is_closed_which_the_winding_decision_requires() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 3.0],
            ),
            primitive_with_radii(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 6.0],
                0.35,
                0.8,
            ),
            primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 6.0],
                [0.0, 0.0, 9.0],
            ),
            primitive_with_radii(
                SupportSegmentKind::Member,
                [0.0, 0.0, 9.0],
                [1.5, -2.0, 11.0],
                0.8,
                0.6,
            ),
            primitive(
                SupportSegmentKind::RootFlare,
                [0.0, 0.0, 11.0],
                [0.0, 0.0, 12.0],
            ),
            primitive(
                SupportSegmentKind::RaftBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPad,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        // Off-origin on every axis: a closed soup's winding decision is
        // unchanged by the frame, an open one's is not, so a leak shows up
        // here as a disagreement between the two centers.
        let origin = build_records(&primitives, [0.0; 3]).unwrap();
        let displaced = build_records(&primitives, [-37.5, 91.25, -6.0]).unwrap();
        for (index, (near, far)) in origin.iter().zip(&displaced).enumerate() {
            for (record, what) in [(near, "origin frame"), (far, "displaced frame")] {
                let triangles = wire_triangles(&record.mesh.xyz);
                let defect = relative_closure_defect(&triangles);
                assert!(
                    defect <= 1e-15,
                    "record {index} ({:?}) in the {what} leaks {defect:e} of its area",
                    record.segment_type
                );
                assert_convex_soup_is_wound_outward(
                    &triangles,
                    &format!("record {index} in the {what}"),
                );
            }
        }
    }

    /// FU-36 invariant fixtures. `[0.0, 0.0, 0.0]` MUST be included: the
    /// original pre-fix seam is bit-invisible at an off-axis centre (the
    /// `r * 1.2246e-16` south-apex offset gets absorbed by f64 rounding once
    /// the centre coordinates carry real magnitude), so a test suite that
    /// only exercised off-axis centres would pass on the broken generator.
    const SPHERE_TEST_CENTERS: [[f64; 3]; 6] = [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 3.0],
        [0.0, 12.34, 0.0],
        [12.34, 0.0, -5.0],
        [1.5, -2.0, 11.0],
        [37.5, -91.25, 6.0],
    ];
    const SPHERE_TEST_RADII: [f64; 3] = [0.35, 2.2, 6.0];

    /// FU-36: every emitted sphere is a closed, consistently-oriented
    /// two-manifold. Before the fix this fails at the origin centre with 32
    /// unmatched directed edges around the 16-sliver south seam.
    #[test]
    #[expect(
        clippy::cast_possible_wrap,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn every_emitted_sphere_is_a_closed_two_manifold() {
        for center in SPHERE_TEST_CENTERS {
            for &radius in &SPHERE_TEST_RADII {
                let triangles = sphere_triangles(center, radius).expect("valid radius");
                let mut vertex_keys: std::collections::HashSet<[u64; 3]> =
                    std::collections::HashSet::new();
                let mut directed_edges: std::collections::HashMap<([u64; 3], [u64; 3]), u32> =
                    std::collections::HashMap::new();
                for triangle in &triangles {
                    let normal =
                        cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
                    assert!(
                        norm(normal) > 0.0,
                        "center {center:?} radius {radius}: degenerate triangle {triangle:?}"
                    );
                    for &corner in triangle {
                        vertex_keys.insert(point_bits(corner));
                    }
                    for i in 0..3 {
                        let a = point_bits(triangle[i]);
                        let b = point_bits(triangle[(i + 1) % 3]);
                        *directed_edges.entry((a, b)).or_insert(0) += 1;
                    }
                }
                for (&(a, b), &count) in &directed_edges {
                    assert_eq!(
                        count, 1,
                        "center {center:?} radius {radius}: directed edge {a:?}->{b:?} appears {count} times, want exactly 1"
                    );
                    let reverse_count = directed_edges.get(&(b, a)).copied().unwrap_or(0);
                    assert_eq!(
                        reverse_count, 1,
                        "center {center:?} radius {radius}: directed edge {a:?}->{b:?} has {reverse_count} matching reverses, want exactly 1"
                    );
                }
                let vertex_count = vertex_keys.len() as isize;
                let edge_count = (directed_edges.len() / 2) as isize;
                let face_count = triangles.len() as isize;
                assert_eq!(
                    vertex_count - edge_count + face_count,
                    2,
                    "center {center:?} radius {radius}: V-E+F = {vertex_count}-{edge_count}+{face_count}, want 2 (closed genus-0 surface)"
                );
            }
        }
    }

    /// FU-36, the direct assertion the ruling asked for: both poles are
    /// exact single-apex fans at `center ± [0, 0, radius]`, and no other
    /// vertex sits within `radius * 1e-12` of either pole. This is the test
    /// that would have caught the original bug (the south "pole" used to be
    /// a 16-gon of radius `r * 1.2246e-16`, not a point).
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn both_pole_caps_are_exact_single_apex_fans() {
        for center in SPHERE_TEST_CENTERS {
            for &radius in &SPHERE_TEST_RADII {
                let triangles = sphere_triangles(center, radius).expect("valid radius");
                let north_cap = &triangles[0..SPHERE_LONGITUDES];
                let south_cap = &triangles[triangles.len() - SPHERE_LONGITUDES..];
                let expected_north = [center[0], center[1], center[2] + radius];
                let expected_south = [center[0], center[1], center[2] - radius];
                for triangle in north_cap {
                    assert_eq!(
                        triangle[0], expected_north,
                        "center {center:?} radius {radius}: north cap apex is not the exact pole"
                    );
                }
                for triangle in south_cap {
                    assert_eq!(
                        triangle[1], expected_south,
                        "center {center:?} radius {radius}: south cap apex is not the exact pole"
                    );
                }
                let pole_tolerance = radius * 1e-12;
                for triangle in &triangles {
                    for &vertex in triangle {
                        if vertex == expected_north || vertex == expected_south {
                            continue;
                        }
                        assert!(
                            norm(sub(vertex, expected_north)) > pole_tolerance,
                            "center {center:?} radius {radius}: stray vertex {vertex:?} sits within pole tolerance of the north pole"
                        );
                        assert!(
                            norm(sub(vertex, expected_south)) > pole_tolerance,
                            "center {center:?} radius {radius}: stray vertex {vertex:?} sits within pole tolerance of the south pole"
                        );
                    }
                }
            }
        }
    }

    /// FU-36: the generated sphere is exactly north/south symmetric.
    ///
    /// Three independent, exact claims, each of which fails on the pre-fix
    /// generator (verified by stashing the fix and re-running):
    ///
    /// 1. The mesh's full VERTEX set, at the origin centre, is exactly
    ///    closed under negating z alone: for every emitted vertex `(x, y,
    ///    z)`, `(x, y, -z)` is also an emitted vertex, bit-for-bit. This is
    ///    the actual "north/south symmetric" claim — pre-fix, the 16
    ///    near-but-not-at south vertices mean `(0, 0, -r)` has no exact
    ///    mirror partner for the true north pole `(0, 0, r)`.
    /// 2. The two pole-cap triangle fans (16 + 16 = 32 of the 224
    ///    triangles) are exactly closed under "negate z, reverse winding" —
    ///    this is the one place the fixed diagonal-splitting convention
    ///    used by every other band does not apply (a fan has no diagonal to
    ///    choose), so the *full* 224-triangle multiset is deliberately NOT
    ///    asserted here: away from the poles, a band's quad is always split
    ///    on the same diagonal regardless of hemisphere, so mirroring a
    ///    mid-band triangle produces the *other*, un-emitted diagonal of
    ///    its mirror quad rather than an actual emitted triangle — a
    ///    property of the (unchanged, out-of-scope) tessellation choice,
    ///    not a symmetry defect in the vertex geometry itself.
    /// 3. The shared latitude table driving every sphere this crate emits,
    ///    at every centre, is itself exactly mirrored (sin unchanged, cos
    ///    exactly negated) — checked once here and, by the determinism of
    ///    floating point arithmetic (identical inputs to `sphere_point`
    ///    always produce identical outputs), implying every north/south
    ///    ring pair is congruent at every centre and radius, not only the
    ///    origin.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn the_generated_sphere_is_exactly_north_south_symmetric() {
        let radius = 2.2;
        let triangles = sphere_triangles([0.0, 0.0, 0.0], radius).expect("valid radius");

        let mut vertex_set: std::collections::HashSet<[u64; 3]> = std::collections::HashSet::new();
        for triangle in &triangles {
            for &vertex in triangle {
                vertex_set.insert(point_bits(vertex));
            }
        }
        for &key in &vertex_set {
            let mirrored = point_bits([
                f64::from_bits(key[0]),
                f64::from_bits(key[1]),
                -f64::from_bits(key[2]),
            ]);
            assert!(
                vertex_set.contains(&mirrored),
                "vertex ({}, {}, {}) has no exact z-mirror in the origin-centred sphere's vertex set",
                f64::from_bits(key[0]),
                f64::from_bits(key[1]),
                f64::from_bits(key[2])
            );
        }

        let north_cap = &triangles[0..SPHERE_LONGITUDES];
        let south_cap = &triangles[triangles.len() - SPHERE_LONGITUDES..];
        let mut north_mirrored: Vec<[[u64; 3]; 3]> = north_cap
            .iter()
            .map(|&triangle| {
                canonicalize_triangle(mirror_z_reverse_winding(triangle)).map(point_bits)
            })
            .collect();
        north_mirrored.sort();
        let mut south_canonical: Vec<[[u64; 3]; 3]> = south_cap
            .iter()
            .map(|&triangle| canonicalize_triangle(triangle).map(point_bits))
            .collect();
        south_canonical.sort();
        assert_eq!(
            north_mirrored, south_canonical,
            "the north pole cap, mirrored and winding-reversed, is not exactly the south pole cap"
        );

        let lat_table = &*LATITUDE_TABLE;
        for k in 0..=SPHERE_LATITUDES {
            let mirror_k = SPHERE_LATITUDES - k;
            let (sin_k, cos_k) = lat_table[k];
            let (sin_mirror, cos_mirror) = lat_table[mirror_k];
            assert_eq!(
                sin_k, sin_mirror,
                "latitude index {k} and its mirror {mirror_k} do not share sin bitwise"
            );
            assert_eq!(
                normalize_zero(cos_k),
                normalize_zero(-cos_mirror),
                "latitude index {k}'s cos is not the exact negation of mirror {mirror_k}'s cos"
            );
        }
    }

    // -----------------------------------------------------------------
    // The clipped tip ball (owner ruling, 2026-09-05).
    // -----------------------------------------------------------------

    /// Every directed edge of a soup, as bit keys, so "closed" can be stated
    /// as an exact count rather than a tolerance.
    fn directed_edges(triangles: &[SoupTriangle]) -> Vec<([u64; 3], [u64; 3])> {
        let key = |point: [f64; 3]| point.map(|value| normalize_zero(value).to_bits());
        triangles
            .iter()
            .flat_map(|triangle| {
                (0..3).map(move |corner| {
                    (key(triangle[corner]), key(triangle[(corner + 1) % 3]))
                })
            })
            .collect()
    }

    /// A closed manifold traverses every edge exactly once in each direction.
    fn assert_watertight(triangles: &[SoupTriangle], what: &str) {
        let mut counts: std::collections::HashMap<([u64; 3], [u64; 3]), i64> =
            std::collections::HashMap::new();
        for (from, to) in directed_edges(triangles) {
            *counts.entry((from, to)).or_default() += 1;
        }
        for ((from, to), forward) in &counts {
            let backward = counts.get(&(*to, *from)).copied().unwrap_or(0);
            assert_eq!(
                *forward, 1,
                "{what}: an edge is traversed {forward} times in one direction; a closed \
                 mesh traverses each exactly once"
            );
            assert_eq!(
                backward, 1,
                "{what}: an edge has {backward} opposite traversals; a closed mesh has \
                 exactly one, so this is an open crack"
            );
        }
    }

    /// A seated tip ball and its taper at the edge of a slab: the ball
    /// straddles the slab's bottom face AND hangs past its side, the taper's
    /// tilted start cap rises through the bottom face. Returns the model and
    /// the two primitives, both carrying the seat trim of the bottom face.
    fn trimmed_edge_tip() -> (Mesh, Vec<SupportPrimitive>) {
        // A slab 10 x 10 x 2 mm, bottom face at z = 0, side face at x = 5.
        let model = plinth_mesh::make_box([10.0, 10.0, 2.0], [0.0, 0.0, 0.0]).unwrap();
        let trim = plinth_support::SeatTrim::new([4.8, 0.0, 0.0], [0.0, 0.0, -1.0]).unwrap();
        // A leaned arm: axis 45 degrees down and outward; ball centre 0.1 mm
        // below the face and 0.05 mm outside the side face.
        let centre = [5.05, 0.0, -0.1];
        let axis = [0.5_f64.sqrt(), 0.0, -(0.5_f64.sqrt())];
        let radius = 0.3;
        let mut ball = primitive_with_radii(
            SupportSegmentKind::ContactSphere,
            std::array::from_fn(|i| centre[i] - radius * axis[i]),
            std::array::from_fn(|i| centre[i] + radius * axis[i]),
            radius,
            radius,
        );
        ball.seat_trim = Some(trim);
        let mut taper = primitive_with_radii(
            SupportSegmentKind::ContactTaper,
            centre,
            std::array::from_fn(|i| centre[i] + 2.0 * axis[i]),
            0.25,
            0.3,
        );
        taper.seat_trim = Some(trim);
        (model, vec![ball, taper])
    }

    fn inside_slab(vertex: [f64; 3]) -> bool {
        vertex[0] > -5.0 + 1e-9
            && vertex[0] < 5.0 - 1e-9
            && vertex[1].abs() < 5.0 - 1e-9
            && vertex[2] > 1e-9
            && vertex[2] < 2.0 - 1e-9
    }

    /// THE SEAT TRIM ITSELF, cut to the model mesh (Craig 2026-10-12). Every
    /// trimmed record keeps nothing inside the model and stays closed; the cut
    /// surface lies ON the model's faces; and NO PLANE cuts it: the ball at
    /// the slab's edge keeps the part that wraps up past the bottom face
    /// outside the side face, which the old seat-plane half-space removed.
    #[test]
    fn a_trimmed_tip_and_taper_keep_their_part_outside_the_model_and_no_plane_cuts_them() {
        let (model, primitives) = trimmed_edge_tip();
        let center = [0.0; 3];
        let (records, report) =
            build_records_trimmed(&primitives, center, Some(&model)).expect("records");
        assert_eq!(report.records_trimmed, 2, "{report:?}");
        // The ball reaches into the slab; the taper's start cap (a plane used
        // to cut it) does not, and is left whole.
        assert_eq!(report.records_cut_by_model, 1, "{report:?}");
        assert!(report.batch_error.is_none(), "{report:?}");
        assert!(report.far_side_pieces.is_empty(), "{report:?}");
        let mut above_the_seat_plane = 0;
        for (index, record) in records.iter().enumerate() {
            let triangles = wire_triangles(&record.mesh.xyz);
            assert!(!triangles.is_empty(), "record {index} kept nothing");
            for vertex in triangles.iter().flatten() {
                assert!(
                    !inside_slab(*vertex),
                    "record {index}: vertex {vertex:?} is inside the model"
                );
                above_the_seat_plane += usize::from(vertex[2] > 1e-3);
            }
            // Shared corners are one indexed vertex in the backend's output,
            // so they round to the same f32 and pair up by point identity.
            assert_watertight(&triangles, "a trimmed record");
        }
        assert!(
            above_the_seat_plane > 0,
            "nothing of the ball wraps past the seat plane outside the side face: a plane still \
             cuts it"
        );
    }

    /// A slab `thickness` mm thick, bottom face at z = 0, and a tip ball of
    /// radius 0.3 whose centre stands `below` mm under that face on a vertical
    /// arm: the ball straddles the slab when `below + 0.3 > thickness`.
    fn tip_under_a_slab(thickness: f64, below: f64, shaft: [f64; 3]) -> (Mesh, Vec<SupportPrimitive>) {
        let model = plinth_mesh::make_box([10.0, 10.0, thickness], [0.0, 0.0, 0.0]).unwrap();
        let trim = plinth_support::SeatTrim::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0])
            .unwrap()
            .with_shaft_unit(shaft);
        let radius = 0.3;
        let centre = [0.0, 0.0, -below];
        let mut ball = primitive_with_radii(
            SupportSegmentKind::ContactSphere,
            [centre[0], centre[1], centre[2] + radius],
            [centre[0], centre[1], centre[2] - radius],
            radius,
            radius,
        );
        ball.seat_trim = Some(trim);
        (model, vec![ball])
    }

    /// THE FAR SIDE OF A THIN WALL (Craig 2026-10-12). A ball that straddles a
    /// wall thinner than itself comes out of the model difference in two
    /// pieces: the support-side piece that holds the shaft end, and a cap that
    /// pokes through the wall's top. The cap is dropped and warned with its
    /// volume; the record keeps the shaft piece only.
    #[test]
    fn a_ball_through_a_thin_wall_drops_its_far_side_piece_and_warns() {
        // Wall z in [0, 0.1]; ball centre 0.15 below, radius 0.3: it reaches
        // z = 0.15, so a 0.05 mm cap stands above the wall.
        let (model, primitives) = tip_under_a_slab(0.1, 0.15, [0.0, 0.0, -1.0]);
        let (records, report) =
            build_records_trimmed(&primitives, [0.0; 3], Some(&model)).expect("records");
        assert!(report.batch_error.is_none(), "{report:?}");
        assert_eq!(report.records_split, 1, "{report:?}");
        assert_eq!(report.far_side_pieces.len(), 1, "{report:?}");
        let piece = &report.far_side_pieces[0];
        assert_eq!(piece.kind, "tip");
        // A spherical cap of height 0.05 on a ball of radius 0.3, faceted.
        let cap = std::f64::consts::PI * 0.05 * 0.05 * (3.0 * 0.3 - 0.05) / 3.0;
        #[expect(clippy::cast_precision_loss, reason = "a small test volume")]
        let dropped = piece.volume_micro_mm3 as f64 / 1.0e6;
        assert!(
            dropped > 0.5 * cap && dropped < 1.1 * cap,
            "the dropped piece is {dropped} mm3, the cap is {cap} mm3"
        );
        // The wall is 0.1 thick and the shaft end 0.45 below its bottom.
        assert!(piece.distance_from_shaft_um >= 500, "{piece:?}");
        let kept = wire_triangles(&records[0].mesh.xyz);
        assert!(!kept.is_empty());
        for vertex in kept.iter().flatten() {
            assert!(
                vertex[2] <= 1e-5,
                "a vertex at z {} of the far-side cap is still in the record",
                vertex[2]
            );
        }
        assert_watertight(&kept, "the kept shaft piece");
    }

    /// SABOTAGE of the far-side rule: the same ball with its shaft said to
    /// leave UPWARD keeps the cap and drops the support-side piece. So the
    /// piece kept is decided by the shaft end, not by size or order.
    #[test]
    fn the_shaft_end_decides_which_piece_of_a_split_ball_is_kept() {
        let (model, primitives) = tip_under_a_slab(0.1, 0.15, [0.0, 0.0, 1.0]);
        let (records, report) =
            build_records_trimmed(&primitives, [0.0; 3], Some(&model)).expect("records");
        assert_eq!(report.far_side_pieces.len(), 1, "{report:?}");
        let kept = wire_triangles(&records[0].mesh.xyz);
        assert!(
            kept.iter().flatten().all(|vertex| vertex[2] >= 0.1 - 1e-5),
            "an upward shaft keeps the cap above the wall"
        );
        // And a wall thicker than the ball's reach does not split it at all.
        let (model, primitives) = tip_under_a_slab(1.0, 0.15, [0.0, 0.0, -1.0]);
        let (_, report) =
            build_records_trimmed(&primitives, [0.0; 3], Some(&model)).expect("records");
        assert_eq!(report.records_split, 0, "{report:?}");
        assert!(report.far_side_pieces.is_empty(), "{report:?}");
    }

    /// A horizontal prism of `sides` faces, radius `radius`, axis along X at
    /// height `radius`, one vertex at the bottom: a convex underside.
    fn laid_prism(radius: f64, length: f64, sides: u32) -> Mesh {
        let mut verts = Vec::new();
        for end in [-0.5 * length, 0.5 * length] {
            for k in 0..sides {
                let theta = -std::f64::consts::FRAC_PI_2
                    + std::f64::consts::TAU * f64::from(k) / f64::from(sides);
                verts.push([end, radius * theta.cos(), radius + radius * theta.sin()]);
            }
        }
        let mut tris = Vec::new();
        for k in 0..sides {
            let next = (k + 1) % sides;
            tris.push([k, next, sides + next]);
            tris.push([k, sides + next, sides + k]);
        }
        for k in 1..sides - 1 {
            tris.push([0, k + 1, k]);
            tris.push([sides, sides + k, sides + k + 1]);
        }
        let mut mesh = Mesh::new(verts, tris).unwrap();
        // Outward, whichever way the fans above were written.
        let triangles: Vec<SoupTriangle> = mesh
            .tris()
            .iter()
            .map(|t| t.map(|corner| mesh.verts()[corner as usize]))
            .collect();
        let volume: f64 = triangles.iter().map(|[a, b, c]| dot(*a, cross(*b, *c))).sum();
        if volume < 0.0 {
            let flipped: Vec<[u32; 3]> = mesh.tris().iter().map(|t| [t[0], t[2], t[1]]).collect();
            mesh = Mesh::new(mesh.verts().to_vec(), flipped).unwrap();
        }
        mesh
    }

    /// THE CURVED UNDERSIDE (TIP-TRUNCATION.md, Bat_Cat_Owl tip 0). A ball
    /// seated under a convex underside: every face the cut made lies ON the
    /// model surface (nothing is cut where the model is not), and the cut
    /// surface is wider than the disc a plane through the lowest point leaves
    /// on the model.
    #[test]
    fn a_ball_under_a_curved_underside_is_cut_only_where_the_model_is() {
        let radius_model = 2.0;
        let model = laid_prism(radius_model, 6.0, 384);
        // Ball radius 0.5, its pole 0.05 deep: centre 0.45 below the lowest
        // point (z = 0), on a vertical arm.
        let ball_radius = 0.5;
        let centre = [0.0, 0.0, -0.45];
        let trim = plinth_support::SeatTrim::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0])
            .unwrap()
            .with_shaft_unit([0.0, 0.0, -1.0]);
        let mut ball = primitive_with_radii(
            SupportSegmentKind::ContactSphere,
            [centre[0], centre[1], centre[2] + ball_radius],
            [centre[0], centre[1], centre[2] - ball_radius],
            ball_radius,
            ball_radius,
        );
        ball.seat_trim = Some(trim);
        let (records, report) =
            build_records_trimmed(&[ball], [0.0; 3], Some(&model)).expect("records");
        assert!(report.batch_error.is_none(), "{report:?}");
        assert_eq!(report.records_cut_by_model, 1, "{report:?}");
        let kept = wire_triangles(&records[0].mesh.xyz);
        // Every vertex lies on the ball's sphere or ON the prism's surface
        // (axis at y = 0, z = 2; facets within the sagitta): a plane cut would
        // leave vertices on neither. A face whose centroid is on the model is
        // contact.
        let off_model = |p: [f64; 3]| (p[1].hypot(p[2] - radius_model) - radius_model).abs();
        let mut cut_area = 0.0;
        for [a, b, c] in &kept {
            for p in [a, b, c] {
                let off_sphere = (norm(sub(*p, centre)) - ball_radius).abs();
                assert!(
                    off_model(*p) < 2e-3 || off_sphere < 1e-5,
                    "vertex {p:?} lies neither on the model ({} off) nor on the ball ({off_sphere} \
                     off): a plane cut it",
                    off_model(*p)
                );
            }
            let centroid = scale(add(add(*a, *b), *c), 1.0 / 3.0);
            if off_model(centroid) < 2e-3 {
                cut_area += 0.5 * norm(cross(sub(*b, *a), sub(*c, *a)));
            }
        }
        // The model surface inside the ball, integrated on a fine grid over
        // the smooth cylinder. A plane through the lowest point touches it on
        // one line only (area 0): everything else of that disc hangs under
        // the curve. The mesh cut takes the whole patch.
        let step = 0.001;
        let mut patch = 0.0;
        for i in -300..=300 {
            for j in -300..=300 {
                let (x, y) = (f64::from(i) * step, f64::from(j) * step);
                let root = (radius_model * radius_model - y * y).sqrt();
                let z = radius_model - root;
                if x * x + y * y + (z - centre[2]).powi(2) < ball_radius * ball_radius {
                    patch += step * step * (1.0 + (y / root).powi(2)).sqrt();
                }
            }
        }
        assert!(
            // The emitted ball is a faceted polyhedron inscribed in the
            // sphere, so its patch is somewhat smaller than the smooth one's.
            cut_area > 0.7 * patch && cut_area <= 1.001 * patch,
            "the contact on the model is {cut_area} mm2; the model surface inside the ball is \
             {patch} mm2"
        );
        assert_watertight(&kept, "the cut ball");
    }

    /// SABOTAGE: the trim not run. The same two records built WITHOUT the
    /// model (the path that trims nothing) must stand on the model side of
    /// the plane and inside the model; otherwise the test above could pass
    /// because the fixture never reached the face.
    #[test]
    fn without_the_trim_the_same_records_stand_inside_the_model() {
        let (_, primitives) = trimmed_edge_tip();
        let records = build_records(&primitives, [0.0; 3]).expect("records");
        for (index, record) in records.iter().enumerate() {
            let triangles = wire_triangles(&record.mesh.xyz);
            assert!(
                triangles.iter().flatten().any(|vertex| vertex[2] > 1e-3),
                "record {index} never crossed the seat plane, so the trim test is vacuous"
            );
        }
        assert!(
            records.iter().any(|record| wire_triangles(&record.mesh.xyz)
                .iter()
                .flatten()
                .any(|vertex| inside_slab(*vertex))),
            "no record reached inside the model, so the model difference is graded on nothing"
        );
    }

    /// A record with no seat trim is never touched by the batched call: the
    /// same primitives with the trims removed come out bit-identical with and
    /// without the model.
    #[test]
    fn an_untrimmed_record_is_bit_identical_with_or_without_the_model() {
        let (model, mut primitives) = trimmed_edge_tip();
        for primitive in &mut primitives {
            primitive.seat_trim = None;
        }
        let (with_model, report) =
            build_records_trimmed(&primitives, [0.0; 3], Some(&model)).expect("records");
        let without = build_records(&primitives, [0.0; 3]).expect("records");
        assert_eq!(report, SeatTrimReport::default());
        assert_eq!(with_model, without);
    }

    /// FU-36: guards against a future "fix" that drops or adds a sliver and
    /// silently changes every emitted byte downstream. The triangle count
    /// and its formula must never move — only vertex positions do.
    #[test]
    fn the_sphere_triangle_count_and_soup_length_are_unchanged() {
        assert_eq!(2 * SPHERE_LONGITUDES * (SPHERE_LATITUDES - 1), 224);
        for center in SPHERE_TEST_CENTERS {
            for &radius in &SPHERE_TEST_RADII {
                let triangles = sphere_triangles(center, radius).expect("valid radius");
                assert_eq!(triangles.len(), 224);
            }
        }
    }

    /// Keeps the existing radius rejection covered directly against
    /// `sphere_triangles`, alongside the new FU-36 invariants.
    #[test]
    fn sphere_triangles_still_refuses_a_non_positive_or_non_finite_radius() {
        for radius in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                sphere_triangles([0.0, 0.0, 0.0], radius),
                Err(EmitError::RecordEncodingRejected),
                "radius {radius} should be rejected"
            );
        }
    }

    /// **The joint ball contains the round rims at its centre** (Craig,
    /// 2026-10-07: two peripheral lines stood out of every head joint). The
    /// 8x16 mesh's flat faces sit inside its vertex sphere, so a head's round
    /// base or a post's round top of radius `r`, centred on the joint, poked
    /// through a mesh drawn at vertex radius `r`. Every vertex of a 24-gon ring
    /// of radius `r` on the sphere centre, in three orientations, must be on
    /// the inner side of every triangle's plane, through the REAL record path.
    #[test]
    fn a_joint_sphere_contains_a_round_rim_of_its_own_radius_at_its_centre() {
        let r = 0.298_f64;
        let centre = [3.0, -2.0, 7.5];
        let primitive = primitive_with_radii(
            SupportSegmentKind::JointSphere,
            [centre[0], centre[1], centre[2] + r],
            [centre[0], centre[1], centre[2] - r],
            r,
            r,
        );
        let (frame, soup) = record_frame_and_soup(&primitive, [0.0; 3], 16, 16).unwrap();
        let k = *JOINT_CIRCUMSCRIBE;
        assert!((0.95..0.98).contains(&k), "k = {k}, expected about 0.962");
        let orientations = [
            [0.0, 0.0, 1.0],
            [40.0_f64.to_radians().sin(), 0.0, 40.0_f64.to_radians().cos()],
            [0.3, -0.5, 0.2],
        ];
        for axis in orientations {
            let axis = scale(axis, 1.0 / norm(axis));
            let seed = if axis[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            let u = cross(axis, seed);
            let u = scale(u, 1.0 / norm(u));
            let v = cross(axis, u);
            for index in 0..24 {
                let angle = std::f64::consts::TAU * f64::from(index) / 24.0;
                let vertex = add(
                    centre,
                    scale(add(scale(u, angle.cos()), scale(v, angle.sin())), r),
                );
                for [a, b, c] in &soup {
                    let mut normal = cross(sub(*b, *a), sub(*c, *a));
                    if dot(normal, sub(*a, centre)) < 0.0 {
                        normal = scale(normal, -1.0);
                    }
                    let outside = dot(normal, sub(vertex, *a)) / norm(normal);
                    assert!(
                        outside <= 1e-12,
                        "rim vertex {index} of axis {axis:?} stands {outside} mm outside a face plane"
                    );
                }
            }
        }
        // The record carries what is drawn: radius and poles.
        assert!((frame.r_start - r / k).abs() < 1e-12 && frame.r_end.to_bits() == frame.r_start.to_bits());
        assert!((frame.start[2] - (centre[2] + r / k)).abs() < 1e-12);
        // The contact sphere is NOT circumscribed: its radius is the contact's.
        assert_eq!(
            drawn_sphere_radius(SupportSegmentKind::ContactSphere, r).to_bits(),
            r.to_bits()
        );
    }

    /// The batched builder must reach the same wire bytes it reached when each
    /// record was wound on its own, and must do so whatever else shares the
    /// batch.  This is the bit-identity guard on moving the sum into the
    /// kernel and the winding call out of the per-record path.
    #[test]
    fn batched_winding_matches_each_record_built_alone() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 3.0],
            ),
            primitive_with_radii(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 6.0],
                0.35,
                0.8,
            ),
            primitive_with_radii(
                SupportSegmentKind::Member,
                [0.0, 0.0, 9.0],
                [1.5, -2.0, 11.0],
                0.8,
                0.6,
            ),
            primitive(
                SupportSegmentKind::RaftBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPad,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        let batched = build_records(&primitives, [0.25, -0.5, 1.0]).unwrap();
        for (index, primitive) in primitives.iter().enumerate() {
            let alone = build_records(std::slice::from_ref(primitive), [0.25, -0.5, 1.0]).unwrap();
            assert_eq!(alone.len(), 1);
            assert_eq!(
                differing_record_field(&alone[0], &batched[index]),
                None,
                "record {index} differs between the batched and solitary builds"
            );
        }
    }

    #[test]
    fn wire_audit_rejects_signed_zero_and_nan_payload_changes() {
        // Both values can be represented by an f32 parser, but neither is the
        // same serialized field.  This is intentionally a bit test rather
        // than `assert_ne!`: normal f32 equality would miss the first case.
        assert!(!same_f32_slice_bits(&[0.0], &[f32::from_bits(0x8000_0000)]));
        assert!(!same_f32_slice_bits(
            &[f32::from_bits(0x7fc0_0001)],
            &[f32::from_bits(0x7fc0_0002)],
        ));
    }

    #[test]
    fn malformed_prewrite_geometry_is_rejected() {
        assert_eq!(
            frustum_triangles(
                [0.0; 3],
                [0.0; 3],
                1.0,
                1.0,
                24,
                FrustumRingNormal::SegmentAxis,
            ),
            Err(EmitError::RecordEncodingRejected)
        );
        assert_eq!(
            raft_bridge_triangles([0.0; 3], [0.0; 3], 2.2, 0.2),
            Err(EmitError::RecordEncodingRejected)
        );
    }

    #[test]
    fn emitter_handoff_accepts_coincident_raft_pad_only_after_semantic_validation() {
        for kind in [
            SupportSegmentKind::ContactTaper,
            SupportSegmentKind::Member,
            SupportSegmentKind::RootFlare,
        ] {
            let primitive = primitive(kind, [0.0; 3], [0.0; 3]);
            let history = fixture_history(std::slice::from_ref(&primitive), 3.0);
            assert!(matches!(
                SupportedModel::from_parts(fixture_mesh(), vec![primitive], history),
                Err(plinth_support::SupportError::InvalidPrimitive)
            ));
        }

        let model = fixture_model(
            vec![primitive(SupportSegmentKind::RaftPad, [0.0; 3], [0.0; 3])],
            3.0,
        );
        let records = build_records(model.primitives(), [0.0; 3])
            .expect("validated coincident raft primitive emits a pad record");
        assert_eq!(records[0].segment_type, SegmentType::PAD_OR_BRIDGE);
    }

    /// THE NATIVE RAFT FRAME (fix/sweep-raft, 2026-09-29): every type-6 record
    /// in every ChituBox-authored file writes start/end `z = 0.0` while its
    /// mesh sits on the plate in the model-centred frame. A pad and a bridge
    /// on a plate at model-frame z −6.75 (centre z 0.5) must write exactly
    /// that: record z +0.0, mesh from −7.25 to −7.05, xy untouched, and the
    /// written file must pass the disk audit.
    ///
    /// Sabotage, run by hand at the change: without the two `record.*[2] =
    /// 0.0` lines in `encode_record`, the record-z assertion goes red, and so
    /// does `raft_pad_exactly_on_the_plate_survives_a_one_ulp_plate_derivation_split`,
    /// because the disk audit refuses a raft record whose z is not 0.0.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "exact binary32 pins of a format field and of an exactly representable mesh height"
    )]
    fn raft_records_write_the_native_frame_and_keep_their_mesh_on_the_plate() {
        let plate = -6.75;
        let center = [0.25, -0.5, 0.5];
        let pad = primitive(
            SupportSegmentKind::RaftPad,
            [1.0, 2.0, plate],
            [1.0, 2.0, plate],
        );
        let bridge = primitive(
            SupportSegmentKind::RaftBridge,
            [1.0, 2.0, plate],
            [4.0, 2.0, plate],
        );
        let records = build_records(&[bridge, pad], center).expect("raft records");
        for record in &records {
            assert_eq!(record.segment_type, SegmentType::PAD_OR_BRIDGE);
            assert_eq!(record.start[2].to_bits(), 0.0_f32.to_bits(), "{record:?}");
            assert_eq!(record.end[2].to_bits(), 0.0_f32.to_bits(), "{record:?}");
            let (lo, hi) = record
                .mesh
                .xyz
                .chunks_exact(3)
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(v[2]), hi.max(v[2]))
                });
            assert_eq!(lo, -7.25, "the mesh bottom is the plate, model-centred");
            assert!(
                (hi - (-7.05)).abs() < 1e-6,
                "the mesh top is plate + 0.2: {hi}"
            );
        }
        assert_eq!(records[1].start[0], 0.75);
        assert_eq!(records[1].start[1], 2.5);
    }

    #[test]
    fn semantic_segment_mapping_is_exhaustive_and_pins_native_ids() {
        let expected = [
            (SupportSegmentKind::ContactSphere, 7),
            (SupportSegmentKind::ContactTaper, 1),
            (SupportSegmentKind::JointSphere, 9),
            (SupportSegmentKind::Member, 3),
            (SupportSegmentKind::RootFlare, 4),
            (SupportSegmentKind::RaftPad, 6),
            (SupportSegmentKind::RaftBridge, 6),
        ];
        for (kind, id) in expected {
            assert_eq!(segment_type(kind), SegmentType(id));
        }
    }

    #[test]
    fn native_chitubox_fixture_pins_real_support_record_semantics() {
        const BASIC: &[u8] = include_bytes!(
            "../../../scripts/tests/fixtures/chitubox/basic-cube-supports-raft.chitubox"
        );
        const CYLINDER: &[u8] = include_bytes!(
            "../../../scripts/tests/fixtures/chitubox/cylinder-supported-bottom-connected.chitubox"
        );
        for bytes in [BASIC, CYLINDER] {
            let project = Project::from_bytes(bytes.to_vec()).expect("native required fixture");
            assert!(project
                .supports(0)
                .unwrap()
                .iter()
                .any(|record| record.segment_type == SegmentType::PAD_OR_BRIDGE));
        }
        const TWO_SUPPORTS: &[u8] =
            include_bytes!("../../../scripts/tests/fixtures/chitubox/two-supports.chitubox");
        let project = Project::from_bytes(TWO_SUPPORTS.to_vec()).expect("native two-support donor");
        let records = project.supports(0).expect("native donor supports");
        // These are independently transcribed raw binary32 values from the
        // authoritative UI-authored donor.  They deliberately do not call
        // our emitter or sphere projection code: this fixture is the native
        // semantic oracle for type 7/1/9/3/4 records.
        type NativeRecord = (u32, [u32; 3], [u32; 3], [u32; 2], usize, Option<u32>);
        const EXPECTED: &[NativeRecord] = &[
            (
                7,
                [0x3fe6_ff88, 0xc0c0_b36b, 0xc104_2b50],
                [0x3fef_1cf0, 0xc0c8_45aa, 0xc106_f0c3],
                [0x3e19_9999, 0x3e19_9999],
                4320,
                None,
            ),
            (
                1,
                [0x3feb_0e3c, 0xc0c4_7c8a, 0xc105_8e0a],
                [0x4010_93cf, 0xc0f6_f631, 0xc118_07b1],
                [0x3dcc_cccd, 0x3e99_999a],
                540,
                Some(0x3dcc_cccd),
            ),
            (
                9,
                [0x4010_93cf, 0xc0f6_f631, 0xc113_09bd],
                [0x4010_93cf, 0xc0f6_f631, 0xc11d_05a5],
                [0x3e9f_be80, 0x3e9f_be80],
                4320,
                None,
            ),
            (
                7,
                [0x40e6_b696, 0xc08f_aaea, 0xc106_a1b7],
                [0x40e6_b696, 0xc08f_aaea, 0xc126_a1b7],
                [0x3f80_0000, 0x3f80_0000],
                4320,
                None,
            ),
            (
                3,
                [0x4010_93cf, 0xc0f6_f631, 0xc118_07b1],
                [0x4010_93cf, 0xc0f6_f631, 0xc19c_fcf1],
                [0x3e99_999a, 0x3e99_999a],
                540,
                None,
            ),
            (
                4,
                [0x4010_93cf, 0xc0f6_f631, 0xc19c_fae5],
                [0x4010_93cf, 0xc0f6_f631, 0xc1a0_fae5],
                [0x3e99_999a, 0x3f40_0000],
                540,
                None,
            ),
            (
                3,
                [0x40f8_add7, 0xc0d0_6c7d, 0xc12e_a643],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc198_fefe],
                [0x3f40_0000, 0x3f40_0000],
                540,
                None,
            ),
            (
                4,
                [0x40f8_add7, 0xc0d0_6c7d, 0xc198_fae5],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc1a0_fae5],
                [0x3f40_0000, 0x3f8c_cccd],
                540,
                None,
            ),
            (
                9,
                [0x40f8_add7, 0xc0d0_6c7d, 0xc122_2b62],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc13b_2124],
                [0x3f47_ae10, 0x3f47_ae10],
                4320,
                None,
            ),
            (
                1,
                [0x40e6_b696, 0xc08f_aaea, 0xc116_a1b7],
                [0x40f8_add7, 0xc0d0_6c7d, 0xc12e_a643],
                [0x3e99_999a, 0x3f40_0000],
                540,
                Some(0x3f00_0000),
            ),
        ];
        let semantic_records: Vec<_> = records
            .iter()
            .filter(|record| record.segment_type != SegmentType::PAD_OR_BRIDGE)
            .collect();
        assert_eq!(semantic_records.len(), EXPECTED.len());
        for (record, &(segment_type, start, end, radii, mesh_len, contact_depth)) in
            semantic_records.into_iter().zip(EXPECTED)
        {
            assert_eq!(record.segment_type, SegmentType(segment_type));
            assert_eq!(record.start.map(f32::to_bits), start);
            assert_eq!(record.end.map(f32::to_bits), end);
            assert_eq!([record.r_start.to_bits(), record.r_end.to_bits()], radii);
            assert_eq!(record.mesh.xyz.len(), mesh_len);
            assert_eq!(
                contact_depth.map(
                    |_| f32::from_le_bytes(record.trailing[..4].try_into().unwrap()).to_bits()
                ),
                contact_depth
            );
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn writes_and_reopens_one_fresh_supported_artifact_in_exact_record_order() {
        let primitives = vec![
            primitive(
                SupportSegmentKind::ContactSphere,
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 3.0],
            ),
            primitive(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, 3.0],
                [0.0, 0.0, 5.0],
            ),
            primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 5.0],
                [0.0, 0.0, 8.0],
            ),
            primitive(SupportSegmentKind::Member, [0.0, 0.0, 8.0], [0.0, 0.0, 9.0]),
            primitive(
                SupportSegmentKind::RootFlare,
                [0.0, 0.0, 9.0],
                [0.0, 0.0, 10.0],
            ),
            primitive(
                SupportSegmentKind::RaftBridge,
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
            ),
            primitive(
                SupportSegmentKind::RaftPad,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ),
        ];
        let expected_types: Vec<_> = primitives
            .iter()
            .map(|primitive| segment_type(primitive.segment_kind))
            .collect();
        let model = fixture_model(primitives, 3.0);
        let target = fixture_path("all-types.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("fresh write and disk audit");
        assert_eq!(report.artifact_path, target);
        assert_eq!(report.support_record_count, expected_types.len());
        assert!(report.artifact_bytes > 0);

        let reopened = Project::open(&target).expect("reopen emitted file");
        assert_eq!(reopened.model_count(), 1);
        assert_eq!(reopened.model(0).unwrap().placement.z_lift, 3.0);
        let records = reopened.supports(0).unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.segment_type)
                .collect::<Vec<_>>(),
            expected_types
        );
        assert!(records
            .iter()
            .flat_map(|record| record.mesh.xyz.iter())
            .all(|value| value.is_finite()));
        std::fs::remove_file(target).expect("remove unique fixture artifact");
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn writes_zero_support_scene_and_applies_retained_lift_once() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("zero-supports.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("zero-support write");
        assert_eq!(report.support_record_count, 0);
        let reopened = Project::open(&target).unwrap();
        assert_eq!(reopened.supports(0).unwrap(), Vec::new());
        assert_eq!(reopened.model(0).unwrap().placement.z_lift, 3.0);
        std::fs::remove_file(target).expect("remove unique fixture artifact");
    }

    #[test]
    fn deterministic_emission_is_byte_identical() {
        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::JointSphere,
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 4.0],
            )],
            3.0,
        );
        let first = fixture_path("determinism-a.chitubox");
        let second = fixture_path("determinism-b.chitubox");
        for target_path in [&first, &second] {
            write(
                &model,
                &EmitRequest {
                    target_path: target_path.clone(),
                    printer: printer(),
                },
            )
            .expect("deterministic fixture write");
        }
        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&second).unwrap()
        );
        std::fs::remove_file(first).expect("remove first deterministic fixture");
        std::fs::remove_file(second).expect("remove second deterministic fixture");
    }

    #[test]
    fn target_directory_is_rejected_before_any_write() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("target-dir");
        std::fs::create_dir(&target).unwrap();
        let result = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        );
        assert_eq!(
            result,
            Err(EmitError::TargetUnavailable {
                path: target.clone()
            })
        );
        std::fs::remove_dir(target).expect("remove unique fixture directory");
    }

    #[test]
    fn fresh_disk_corruption_fails_without_repairing_or_rewriting() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("corruption.chitubox");
        write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .unwrap();
        let corrupt = vec![0_u8; 9];
        std::fs::write(&target, &corrupt).unwrap();
        let mesh = fixture_mesh();
        let expected_model = model_triangle_soup(&mesh, mesh_bbox_center(&mesh).unwrap()).unwrap();
        assert_eq!(
            audit_disk(
                &target,
                &printer(),
                AuditExpected {
                    lift: 3.0,
                    plate_z: -3.5,
                    model: &expected_model,
                    records: &[],
                    primitives: &[],
                    model_center: [0.0; 3],
                },
            )
            .map(|_| ())
            .map_err(|finding| finding.check),
            Err(AuditCheck::Reopen),
            "bytes the reader cannot open are the one UNREADABLE outcome, named as the reopen"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            corrupt,
            "audit must not repair or rewrite disk bytes"
        );
        std::fs::remove_file(target).expect("remove unique fixture artifact");
    }

    /// EMIT AND WARN (fix/round100-emit, 2026-09-29): support geometry below
    /// the planned plate is a FINDING on an emitted file, not a refusal. The
    /// file is at the target, and the finding names the check, the record
    /// that holds the lowest point and its value against the limit.
    #[test]
    fn support_geometry_below_the_planned_plate_is_emitted_and_named() {
        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, -4.0],
                [0.0, 0.0, -3.1],
            )],
            3.0,
        );
        let target = fixture_path("below-plate.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("a file that reads back is emitted, whatever the audit found");
        assert!(target.exists(), "the evidence must be kept at the target");
        assert!(!staging_path(&target).exists());
        assert!(!failed_audit_path(&target).exists());
        let below: Vec<_> = report
            .audit_findings
            .iter()
            .filter(|finding| finding.check == AuditCheck::BuildVolume)
            .collect();
        assert_eq!(
            below.len(),
            1,
            "one violated side: {:?}",
            report.audit_findings
        );
        assert_eq!(below[0].record, Some(0), "record 0 holds the lowest point");
        assert!(
            below[0].detail.starts_with("minimum z above the plate -")
                && below[0].detail.contains("at least 0.000000 mm"),
            "the finding names the value and the limit: {}",
            below[0].detail
        );
        std::fs::remove_file(target).unwrap();
    }

    /// A raft pad sitting *exactly* on the planned plate must emit, even when
    /// the audit's reopened plate derivation lands an ulp away from the one the
    /// pad was written at.
    ///
    /// The two derivations are `f32(plate_z_mm - center_z)` (what the pad
    /// vertices carry) and `f32(model_min_z) - f32(lift)` (what the reopened
    /// file yields). They agree whenever `lift` and `lift + center_z` share a
    /// binade, because the f32 grid is then translation invariant under the
    /// exactly-representable centre offset. This lift is chosen so they do not:
    /// `center_z` is 0.5 and `7.5 + 2^-21 + 2^-30` rounds to `7.5 + 2^-21` in
    /// binary32, whose sum with 0.5 is the exact tie `8 + 2^-21` and rounds to
    /// even (8.0), while the unrounded `8 + 2^-21 + 2^-30` is above that tie and
    /// rounds to `8 + 2^-20`. One ulp of separation, the pad one ulp *below* the
    /// reopened plate, and the `min[2] < 0.0` floor used to refuse the file.
    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn raft_pad_exactly_on_the_plate_survives_a_one_ulp_plate_derivation_split() {
        let lift = 7.5 + 2.0_f64.powi(-21) + 2.0_f64.powi(-30);
        let center_z = 0.5_f64;

        // Prove this fixture really is the ulp case before it proves the fix:
        // the two derivations of the same plate differ by exactly one ulp.
        let pad_datum = (-lift - center_z) as f32;
        let reopened_datum = ((-center_z) as f32) - (lift as f32);
        assert_ne!(
            pad_datum.to_bits(),
            reopened_datum.to_bits(),
            "fixture no longer straddles the binade boundary it was chosen for"
        );
        assert!(
            pad_datum < reopened_datum,
            "the pad datum must sit below the reopened datum for the floor to fire"
        );

        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::RaftPad,
                [0.0, 0.0, -lift],
                [0.0, 0.0, -lift],
            )],
            lift,
        );
        let target = fixture_path("plate-ulp.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("a pad exactly on the plate is a perfect layout, not a defect");
        assert_eq!(report.support_record_count, 1);
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn support_geometry_above_printer_height_is_emitted_and_named() {
        let model = fixture_model(
            vec![primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, 221.0],
                [0.0, 0.0, 222.0],
            )],
            3.0,
        );
        let target = fixture_path("above-height.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("a file that reads back is emitted, whatever the audit found");
        assert!(target.exists(), "the evidence must be kept at the target");
        let high: Vec<_> = report
            .audit_findings
            .iter()
            .filter(|finding| finding.check == AuditCheck::BuildVolume)
            .collect();
        assert_eq!(
            high.len(),
            1,
            "one violated side: {:?}",
            report.audit_findings
        );
        assert_eq!(high[0].record, Some(0));
        assert!(
            high[0].detail.starts_with("maximum z above the plate"),
            "{}",
            high[0].detail
        );
        std::fs::remove_file(target).unwrap();
    }

    /// A clean scene carries no audit finding: the findings are not noise.
    #[test]
    fn a_clean_scene_has_no_audit_findings() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("clean-audit.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("the scene emits");
        assert_eq!(report.audit_findings, Vec::new());
        std::fs::remove_file(target).unwrap();
    }

    /// A file that cannot be read back is the only audit outcome that fails
    /// the write, and even then its BYTES ARE KEPT, beside the target (never
    /// at it), and the error names the check and where the bytes are. An
    /// artifact an earlier run left at the target is removed, so it cannot
    /// pass for this run's answer.
    #[test]
    fn an_unreadable_file_fails_the_write_but_keeps_its_bytes_beside_the_target() {
        let target = fixture_path("unreadable.chitubox");
        let good = fixture_model(Vec::new(), 3.0);
        let request = EmitRequest {
            target_path: target.clone(),
            printer: printer(),
        };
        write(&good, &request).expect("the first run emits");
        assert!(target.exists());

        // The staged bytes this run "wrote" cannot be read back.
        let staging = staging_path(&target);
        let unreadable = vec![0_u8; 9];
        std::fs::write(&staging, &unreadable).unwrap();
        let mesh = fixture_mesh();
        let expected_model = model_triangle_soup(&mesh, mesh_bbox_center(&mesh).unwrap()).unwrap();
        let staged = audit_disk(
            &staging,
            &printer(),
            AuditExpected {
                lift: 3.0,
                plate_z: -3.5,
                model: &expected_model,
                records: &[],
                primitives: &[],
                model_center: [0.0; 3],
            },
        )
        .map_err(|finding| EmitError::DiskAuditUnreadable {
            finding,
            kept_at: staging.clone(),
        });

        let result = settle_staged(&staging, &target, staged);
        let Err(EmitError::DiskAuditUnreadable { finding, kept_at }) = result else {
            panic!("an unreadable file fails the write: {result:?}");
        };
        assert_eq!(finding.check, AuditCheck::Reopen);
        assert_eq!(kept_at, failed_audit_path(&target));
        assert_eq!(
            std::fs::read(&kept_at).unwrap(),
            unreadable,
            "the unreadable bytes are the evidence and must be kept"
        );
        assert!(
            !target.exists(),
            "the target only ever holds a file that reads back; the earlier run's \
             artifact must not pass for this run's answer"
        );
        assert!(!staging.exists());
        std::fs::remove_file(kept_at).unwrap();
    }

    /// A failed write (here, a scene whose coordinates do not fit binary32)
    /// leaves *no* file at the target, including one an earlier run put there.
    ///
    /// If a previous run's artifact simply stayed where it was, the next
    /// reader takes it for this run's answer. It is the same reason
    /// `judge_then_emit` clears the target on a gate refusal, which never
    /// reaches this function at all.
    #[test]
    fn a_failed_write_removes_an_earlier_runs_artifact_at_the_target() {
        let target = fixture_path("stale-then-failed.chitubox");
        let good = fixture_model(Vec::new(), 3.0);
        let request = EmitRequest {
            target_path: target.clone(),
            printer: printer(),
        };
        let report = write(&good, &request).expect("the first run emits");
        assert_eq!(report.artifact_path, target);
        assert!(target.exists());

        let unencodable = fixture_model(
            vec![primitive(
                SupportSegmentKind::Member,
                [0.0, 0.0, 1.0e39],
                [0.0, 0.0, 2.0e39],
            )],
            3.0,
        );
        assert_eq!(
            write(&unencodable, &request),
            Err(EmitError::RecordEncodingRejected)
        );
        assert!(
            !target.exists(),
            "a failed write left the earlier run's artifact at {}",
            target.display()
        );
    }

    /// The accepted path leaves the artifact and nothing else: the staging
    /// sibling is renamed, never copied, so it cannot linger next to exports.
    #[test]
    fn an_accepted_write_leaves_no_staging_file_beside_the_artifact() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("no-staging-litter.chitubox");
        let report = write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .expect("the scene emits");

        assert_eq!(report.artifact_path, target, "the report names the target");
        assert!(target.exists());
        assert!(
            !staging_path(&target).exists(),
            "the staging file survived at {}",
            staging_path(&target).display()
        );
        std::fs::remove_file(target).unwrap();
    }

    /// THIS TEST DOES NOT CALL [`write`], AND DOES NOT COVER ITS ERROR ARM.
    ///
    /// It reproduces the two steps `write` performs after `write_staged`
    /// returns -- rename, then [`sync_dir`] -- and shows that when the sync
    /// fails, the artifact the rename put in place survives. That is a fact
    /// about the filesystem sequence, not about `write`'s handling of it.
    ///
    /// The distinction is load-bearing and was measured: inserting
    /// `let _ = std::fs::remove_file(&request.target_path);` into `write`'s
    /// `DurabilitySyncFailed` arm -- the exact regression that would trade a
    /// confirmed-good artifact for certain data loss -- leaves this test, and
    /// the whole `plinth-emit` suite, fully green. **`write`'s error arm has
    /// no test.** Covering it needs a fault-injection seam that does not
    /// exist yet: the directory cannot be restricted from outside `write`
    /// at the one point between its rename and its sync, because
    /// `write_staged`'s own internal fsync runs first and fails earlier.
    ///
    /// What this test does earn: it pins the platform behaviour the design
    /// rests on, and it fails if that behaviour changes.
    ///
    /// A directory with `--wx` permission (write + search, no read) still
    /// permits the rename onto `target_path` -- POSIX only requires search
    /// and write on the containing directory for a rename, confirmed
    /// against this platform directly (a plain `mv` under `--wx` succeeds
    /// here) -- but `File::open` on the directory itself, which `sync_dir`
    /// requires, fails with `EACCES` (confirmed the same way: `ls` on a
    /// `--wx` directory is refused).  That gives a deterministic way to make
    /// the rename succeed and the durability sync fail, without touching
    /// real crash behaviour.
    ///
    /// The restriction is applied only *after* [`write_staged`] returns:
    /// `write_staged` calls `chitubox_io::Project::save`, which performs its
    /// *own* directory fsync (on its own internal rename onto `staging`,
    /// before this test ever runs) using the identical `open`-then-`sync_all`
    /// idiom -- restricting the directory any earlier would fail that
    /// earlier sync instead and never reach the rename this test targets.
    /// This test therefore drives the same two steps `write` performs after
    /// `write_staged` succeeds (rename, then [`sync_dir`]) directly, so the
    /// directory can be restricted at exactly the point between them.
    #[cfg(unix)]
    #[test]
    fn the_rename_then_sync_sequence_write_repeats_keeps_the_artifact_when_the_sync_fails() {
        use std::os::unix::fs::PermissionsExt;

        let dir = fixture_path("dir-sync-write-dir");
        std::fs::create_dir_all(&dir).expect("create isolated test directory");
        let target = dir.join("artifact.chitubox");
        let staging = staging_path(&target);
        let model = fixture_model(Vec::new(), 3.0);
        let request = EmitRequest {
            target_path: target.clone(),
            printer: printer(),
        };

        write_staged(&model, &request, &staging).expect("staging save succeeds beforehand");

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300))
            .expect("restrict directory to write+search, no read");

        let rename_result = std::fs::rename(&staging, &target);
        let sync_result = sync_dir(&target);

        // Restore permissions before any assertion can panic and skip cleanup.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok();
        let artifact_survived = target.exists();
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            rename_result.is_ok(),
            "a rename needs only write+search on its directory, and must still \
             succeed under --wx"
        );
        assert!(
            sync_result.is_err(),
            "sync_dir must fail to confirm durability when the directory cannot be \
             opened for read"
        );
        assert!(
            artifact_survived,
            "the successfully-renamed artifact must remain at target_path even though \
             its directory's durability could not be confirmed -- write()'s caller sees \
             this as EmitError::DurabilitySyncFailed without target_path being deleted"
        );
    }

    #[test]
    fn disk_audit_names_a_reopened_lift_or_plate_frame_mismatch() {
        let model = fixture_model(Vec::new(), 3.0);
        let target = fixture_path("lift-mismatch.chitubox");
        write(
            &model,
            &EmitRequest {
                target_path: target.clone(),
                printer: printer(),
            },
        )
        .unwrap();
        let mesh = fixture_mesh();
        let expected_model = model_triangle_soup(&mesh, mesh_bbox_center(&mesh).unwrap()).unwrap();
        let report = audit_disk(
            &target,
            &printer(),
            AuditExpected {
                lift: 3.0,
                plate_z: -3.4,
                model: &expected_model,
                records: &[],
                primitives: &[],
                model_center: [0.0; 3],
            },
        )
        .expect("the file reads back");
        assert!(
            report
                .audit_findings
                .iter()
                .any(|finding| finding.check == AuditCheck::PlateFrame
                    && finding.detail.contains("-3.4")),
            "the frame mismatch is named with its planned plate: {:?}",
            report.audit_findings
        );
        std::fs::remove_file(target).unwrap();
    }
}
