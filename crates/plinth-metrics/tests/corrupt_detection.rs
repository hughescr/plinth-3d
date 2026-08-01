//! Disk-first self-validation for the independent format metrics.
//!
//! Each mutation begins with the committed baseline Sphere artifact, is written
//! as a separate temporary ChituBox file through the typed container writer,
//! and is then reopened through `plinth_metrics::load_artifacts`.  There is no
//! producer-memory score and no repair between write and audit.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chitubox_io::{ModelGeometry, ModelSource, Placement, Project, TriangleSoup};
use plinth_core::ModelId;
use plinth_metrics::{
    analyze_artifact_topology, load_artifacts, topology_metric_rows, ArtifactDisposition,
    ArtifactLoad, ArtifactRequest, DiskArtifact, TopologyMetricError, TopologyMetricInput,
    TOPOLOGY_METRIC_ORDER,
};

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("crate lives two directories below the committed repository root")
}

fn baseline_sphere() -> PathBuf {
    let path = repo_root().join("corpus/baseline/Sphere.chitubox");
    assert!(
        path.is_file(),
        "required committed known-good baseline Sphere artifact is missing: {}",
        path.display()
    );
    path
}

fn baseline_sphere_triangles() -> Vec<f32> {
    let path = baseline_sphere();
    let project = Project::open(&path).unwrap_or_else(|error| {
        panic!(
            "required committed known-good baseline Sphere artifact cannot be opened at {}: {error}",
            path.display()
        )
    });
    project.model_triangles(0).unwrap_or_else(|error| {
        panic!(
            "required committed known-good baseline Sphere artifact has no readable first model at {}: {error}",
            path.display()
        )
    })
}

fn write_model(path: &Path, triangles: Vec<f32>) {
    let mut project = Project::new_empty();
    project
        .add_model(ModelSource {
            name: "metrics-corruption-probe".to_owned(),
            geometry: ModelGeometry::Triangles(TriangleSoup::new(triangles)),
            placement: Placement::default(),
        })
        .unwrap_or_else(|error| {
            panic!(
                "test corruption artifact could not be constructed at {}: {error}",
                path.display()
            )
        });
    project.save(path).unwrap_or_else(|error| {
        panic!(
            "test corruption artifact could not be written at {}: {error}",
            path.display()
        )
    });
}

fn reopen(path: &Path, name: &str) -> DiskArtifact {
    let loads = load_artifacts(&[ArtifactRequest::new(
        ModelId::from(name),
        ArtifactDisposition::Produced {
            path: path.to_owned(),
        },
    )])
    .unwrap_or_else(|error| {
        panic!(
            "fresh disk audit input {name} could not be reopened from {}: {error}",
            path.display()
        )
    });
    match loads.into_iter().next() {
        Some(ArtifactLoad::Artifact(artifact)) => artifact,
        Some(ArtifactLoad::ProducerFailed { .. }) => {
            panic!("fresh disk audit input {name} unexpectedly became a producer failure")
        }
        None => panic!("fresh disk audit input {name} returned no row"),
    }
}

fn append_interior_tetrahedron(mut sphere: Vec<f32>) -> Vec<f32> {
    let mut lower = [f32::INFINITY; 3];
    let mut upper = [f32::NEG_INFINITY; 3];
    for point in sphere.chunks_exact(3) {
        for component in 0..3 {
            lower[component] = lower[component].min(point[component]);
            upper[component] = upper[component].max(point[component]);
        }
    }
    let center = [
        (lower[0] + upper[0]) * 0.5,
        (lower[1] + upper[1]) * 0.5,
        (lower[2] + upper[2]) * 0.5,
    ];
    let radius = (upper[0] - lower[0])
        .min(upper[1] - lower[1])
        .min(upper[2] - lower[2])
        * 0.01;
    assert!(
        radius.is_finite() && radius > 0.0,
        "baseline Sphere must have a finite positive bounding-box extent to inject an interior cavity"
    );
    let vertices = [
        [center[0] - radius, center[1] - radius, center[2] - radius],
        [center[0] + radius, center[1] - radius, center[2] - radius],
        [center[0] - radius, center[1] + radius, center[2] - radius],
        [center[0] - radius, center[1] - radius, center[2] + radius],
    ];
    // Positive orientation, so this test exercises nesting/cavity detection
    // rather than merely relying on the negative-shell metric.
    for [a, b, c] in [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]] {
        sphere.extend_from_slice(&vertices[a]);
        sphere.extend_from_slice(&vertices[b]);
        sphere.extend_from_slice(&vertices[c]);
    }
    sphere
}

#[test]
fn disk_reopened_corruptions_raise_their_named_format_metrics() {
    let temp = TempDir::new();
    let source = baseline_sphere_triangles();
    assert!(
        source.len() > 9 && source.len().is_multiple_of(9),
        "required committed known-good baseline Sphere model must contain complete triangles"
    );

    let clean = temp.path().join("clean.chitubox");
    write_model(&clean, source.clone());
    let clean_bytes = fs::read(&clean).expect("written clean corruption-control artifact");
    let clean_metrics = analyze_artifact_topology(&reopen(&clean, "clean-control"))
        .expect("clean artifact topology analysis")
        .metrics;
    assert_eq!(
        clean_metrics.boundary_edges, 0,
        "clean artifact is watertight"
    );
    assert_eq!(
        clean_metrics.negative_shells, 0,
        "clean artifact has outward shell winding"
    );
    assert_eq!(
        clean_metrics.winding, 0,
        "clean artifact has no winding failure"
    );
    assert_eq!(
        clean_metrics.cavities, 0,
        "clean artifact has no interior shell"
    );

    let clean_artifact = reopen(&clean, "clean-topology-row");
    let rows = topology_metric_rows(
        &clean_artifact,
        &[TopologyMetricInput {
            source_index: 0,
            islands: 7,
        }],
    )
    .expect("ordered islands input is carried into the M1 topology row");
    assert_eq!(
        rows[0].entries().map(|(id, _)| id.as_str().to_owned()),
        TOPOLOGY_METRIC_ORDER.map(str::to_owned),
        "the public topology row uses the normative metric order"
    );
    assert_eq!(
        rows[0].entries()[0].1.get(),
        7.0,
        "islands is supplied, not fabricated"
    );
    assert!(matches!(
        topology_metric_rows(&clean_artifact, &[]),
        Err(TopologyMetricError::InputCount {
            expected: 1,
            actual: 0
        })
    ));
    assert!(matches!(
        topology_metric_rows(
            &clean_artifact,
            &[TopologyMetricInput {
                source_index: 1,
                islands: 7
            }]
        ),
        Err(TopologyMetricError::InputSourceIndex {
            row: 0,
            expected: 0,
            actual: 1
        })
    ));

    let mut deleted = source.clone();
    deleted.truncate(deleted.len() - 9);
    let deleted_path = temp.path().join("deleted-triangle.chitubox");
    write_model(&deleted_path, deleted);
    assert_ne!(
        fs::read(&deleted_path).expect("written deleted-triangle artifact"),
        clean_bytes,
        "deleted-triangle mutation must actually change the emitted bytes"
    );
    let deleted_metrics = analyze_artifact_topology(&reopen(&deleted_path, "deleted-triangle"))
        .expect("deleted-triangle topology analysis")
        .metrics;
    assert!(
        deleted_metrics.boundary_edges > 0,
        "deleted triangle must raise boundary_edges after fresh disk reopen"
    );

    let mut flipped = source.clone();
    for triangle in flipped.chunks_exact_mut(9) {
        for component in 0..3 {
            triangle.swap(3 + component, 6 + component);
        }
    }
    let flipped_path = temp.path().join("flipped-shell.chitubox");
    write_model(&flipped_path, flipped);
    assert_ne!(
        fs::read(&flipped_path).expect("written flipped-shell artifact"),
        clean_bytes,
        "flipped-shell mutation must actually change the emitted bytes"
    );
    let flipped_metrics = analyze_artifact_topology(&reopen(&flipped_path, "flipped-shell"))
        .expect("flipped-shell topology analysis")
        .metrics;
    assert!(
        flipped_metrics.negative_shells > 0,
        "flipped shell must raise negative_shells after fresh disk reopen"
    );
    assert!(
        flipped_metrics.winding > 0,
        "flipped shell must raise winding after fresh disk reopen"
    );

    let cavity_path = temp.path().join("interior-cavity.chitubox");
    write_model(&cavity_path, append_interior_tetrahedron(source));
    assert_ne!(
        fs::read(&cavity_path).expect("written interior-cavity artifact"),
        clean_bytes,
        "interior-cavity mutation must actually change the emitted bytes"
    );
    let cavity_metrics = analyze_artifact_topology(&reopen(&cavity_path, "interior-cavity"))
        .expect("interior-cavity topology analysis")
        .metrics;
    assert!(
        cavity_metrics.cavities > 0,
        "injected interior tetrahedron must raise cavities after fresh disk reopen"
    );
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-metrics-corrupt-detection-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create test-only corruption directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
