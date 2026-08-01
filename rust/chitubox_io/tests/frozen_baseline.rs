use std::path::PathBuf;

use chitubox_io::Project;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("chitubox_io must remain two directories below the repository root")
        .to_path_buf()
}

#[test]
fn sphere_baseline_is_a_disk_artifact_not_an_in_memory_claim() {
    let path = repo_root().join("corpus/baseline/Sphere.chitubox");
    assert!(
        path.is_file(),
        "required frozen baseline artifact is missing: {}",
        path.display()
    );

    let disk_bytes =
        std::fs::read(&path).expect("required frozen Sphere baseline must be readable");
    let project = Project::open(&path).expect("frozen Sphere baseline must parse fresh from disk");

    assert_eq!(
        project.model_count(),
        1,
        "Sphere baseline model count changed"
    );
    assert_eq!(
        project.support_counts(),
        vec![47],
        "Sphere baseline support-record count changed"
    );

    // `to_bytes()` independently re-parses and interval-audits the serialization.
    // The equality proof means that audit covered the original disk bytes, with no
    // repair or rewritten artifact between producer and audit.
    let audited_bytes = project
        .to_bytes()
        .expect("frozen Sphere baseline must pass fresh structural audit");
    assert_eq!(
        audited_bytes, disk_bytes,
        "fresh structural audit must preserve the artifact bytes exactly"
    );
}
