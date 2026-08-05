//! The machine-provable gate over every pinned baseline orientation record.
//!
//! `corpus/manifest.json` names `scripts/tools/prep_one.py` at git rev 6eb7f6a as the sole
//! baseline tool, and that Python freeze is closed. There is therefore no sanctioned regenerator
//! that could re-derive `stages[hollow].report.orientation` on demand, which would ordinarily
//! leave those records as unfalsifiable assertions.
//!
//! This gate falsifies them instead. For every corpus model that ships a committed
//! `*.prepped.oriented-solid.stl`, it feeds the source STL through the real production path --
//! `plinth_decimate::run` then `plinth_orient::run`, reading the orientation only from the pinned
//! report -- and requires the result to reproduce the committed artifact. A wrong axis, a wrong
//! sign, a wrong angle, or a borrowed record from another model all move vertices by millimetres
//! and fail here immediately; the tolerance admits only the artifact's own `f32` quantization.
//!
//! Two of the four records (Cylinder, make_box) encode the legacy identity orientation, which the
//! Python writer expressed by omission -- `orient_search` seeded `best` with `np.eye(4)` and wrote
//! `log["orientation"]` only when a candidate beat that seed, so an identity win emitted no key at
//! all. Those two records were recovered from these very artifacts and carry
//! `"orientation_provenance": "recovered-from-oriented-solid-artifact"`. This gate is what makes
//! that recovery a checked invariant rather than a claim.

use std::path::{Path, PathBuf};

use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};
use plinth_orient::{run as run_orient, BaselineSelection, OrientRequest, PinnedBaseline};

/// Every corpus model whose freeze committed an oriented-solid artifact to compare against.
/// The four models that ship no such artifact cannot be gated this way and are not claimed to be.
const REDERIVABLE: [Rederivable; 4] = [
    Rederivable::new("Sphere", ExpectedSelection::RotatedAboutX(-20.0)),
    Rederivable::new("Cylinder", ExpectedSelection::Identity),
    Rederivable::new("make_box", ExpectedSelection::Identity),
    Rederivable::new(
        "Weeping Treant v1.2",
        ExpectedSelection::RotatedAboutX(-30.0),
    ),
];

/// The probe limits the rest of the workspace pins for real corpus geometry.
const PROBE: ProbeConfig = ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0);

#[derive(Clone, Copy)]
struct Rederivable {
    name: &'static str,
    expected: ExpectedSelection,
}

impl Rederivable {
    const fn new(name: &'static str, expected: ExpectedSelection) -> Self {
        Self { name, expected }
    }
}

/// The record each pinned report is expected to decode to, stated here independently so that a
/// silent edit to a baseline JSON cannot quietly redefine what this gate proves.
#[derive(Clone, Copy)]
enum ExpectedSelection {
    Identity,
    RotatedAboutX(f64),
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root resolves from this crate")
}

fn load(path: &Path, what: &str) -> Mesh {
    load_binary_stl(path, WeldPolicy::None).unwrap_or_else(|error| {
        panic!(
            "the required {what} fixture {} must be provisioned and loadable: {error:?}",
            path.display()
        )
    })
}

/// The committed artifacts are binary STL, so every coordinate in them was rounded to `f32` on the
/// way out. Reproducing them in `f64` can therefore never be exact; the achievable bound is that
/// rounding, which scales with magnitude. Four ULP of `f32` at the artifact's own largest
/// coordinate leaves ample room for the rounding itself while staying orders of magnitude below
/// any real orientation error -- the nearest wrong candidate in the legacy family is 1.74 mm away
/// for make_box and 5.21 mm for Cylinder.
fn quantization_tolerance_mm(artifact: &Mesh) -> f64 {
    let largest = artifact
        .verts()
        .iter()
        .flat_map(|vertex| vertex.iter())
        .fold(0.0_f64, |worst, coordinate| worst.max(coordinate.abs()));
    (4.0 * f64::from(f32::EPSILON) * largest).max(1e-9)
}

/// Worst per-coordinate disagreement. The artifacts preserve the source triangle order, so this is
/// a direct vertex-for-vertex comparison rather than a permutation-tolerant one.
fn worst_residual_mm(rederived: &Mesh, artifact: &Mesh) -> f64 {
    rederived
        .verts()
        .iter()
        .zip(artifact.verts())
        .flat_map(|(left, right)| left.iter().zip(right))
        .fold(0.0_f64, |worst, (left, right)| {
            worst.max((left - right).abs())
        })
}

#[test]
fn every_pinned_orientation_record_rederives_its_committed_oriented_solid_artifact() {
    let root = repo_root();

    for model in REDERIVABLE {
        let name = model.name;
        let source = load(
            &root.join("corpus/models").join(format!("{name}.stl")),
            "corpus source",
        );
        let artifact = load(
            &root
                .join("corpus/baseline")
                .join(format!("{name}.prepped.oriented-solid.stl")),
            "committed oriented-solid",
        );

        let source_faces = source.tris().len();
        let budget = DecimationBudget::ratified_default(PROBE).expect("registered corpus budget");
        let decimated = run_decimation(&source, budget)
            .unwrap_or_else(|error| panic!("{name}: decimation must succeed: {error:?}"))
            .into_decimated_model()
            .unwrap_or_else(|scale| panic!("{name}: corpus scale is in contract: {scale:?}"));
        assert_eq!(
            decimated.mesh().tris().len(),
            source_faces,
            "{name}: the corpus is entirely already-print-scale, so orientation must see the \
             unreduced source; a reduction here would invalidate the artifact comparison rather \
             than fail it"
        );

        let oriented = run_orient(
            decimated,
            &OrientRequest {
                pinned: PinnedBaseline {
                    pipeline_report: root
                        .join("corpus/baseline")
                        .join(format!("{name}.pipeline.json")),
                },
            },
        )
        .unwrap_or_else(|error| panic!("{name}: the pinned baseline must orient: {error}"));

        let selection = oriented.history().orientation().selection;
        match (model.expected, selection) {
            (ExpectedSelection::Identity, BaselineSelection::Identity) => {}
            (
                ExpectedSelection::RotatedAboutX(degrees),
                BaselineSelection::AxisDegrees { axis, degrees: got },
            ) => {
                assert!(
                    matches!(axis, plinth_orient::BaselineAxis::X) && got.get() == degrees,
                    "{name}: pinned report decodes to {selection:?}, expected {degrees} deg about X"
                );
            }
            _ => panic!("{name}: pinned report decodes to the wrong kind of record: {selection:?}"),
        }

        assert_eq!(
            oriented.mesh().verts().len(),
            artifact.verts().len(),
            "{name}: the committed artifact must describe the same geometry as the source"
        );

        let tolerance = quantization_tolerance_mm(&artifact);
        let residual = worst_residual_mm(oriented.mesh(), &artifact);
        // Visible under `--nocapture`: a gate that claims to prove something should let a reader
        // see how much room it actually has.
        println!("{name}: residual {residual:e} mm against a {tolerance:e} mm bound");
        assert!(
            residual <= tolerance,
            "{name}: the pinned orientation record does not rederive its own committed \
             oriented-solid artifact -- worst coordinate residual {residual:e} mm exceeds the \
             {tolerance:e} mm f32 quantization bound. The record and the artifact disagree; one \
             of them is wrong."
        );
    }
}

#[test]
fn the_recovered_identity_records_are_the_ones_this_gate_proves() {
    let root = repo_root();

    // These two reports carry a hand-added record, so they are the ones whose provenance must be
    // legible in the frozen data itself rather than only in a commit message.
    for name in ["Cylinder", "make_box"] {
        let path = root
            .join("corpus/baseline")
            .join(format!("{name}.pipeline.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            text.contains("\"orientation_provenance\": \"recovered-from-oriented-solid-artifact\""),
            "{name}: a recovered orientation record must say so in the report that carries it"
        );
    }
}
