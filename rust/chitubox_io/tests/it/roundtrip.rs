//! Gold invariant (a): `open -> to_bytes` is byte-identical to the input for
//! every corpus specimen, including the three transform probes and the large
//! supported files. `to_bytes` also runs the structural verify + interval
//! audit internally, so a green test proves gold (a) and (b) together.

use crate::common;

use chitubox_io::Project;
use common::*;

#[test]
fn corpus_roundtrip_byte_identical() {
    for name in REQUIRED_ROUNDTRIP_FILES {
        let original = read_required_fixture(name);
        let proj =
            Project::from_bytes(original.clone()).unwrap_or_else(|e| panic!("parse {name}: {e}"));
        let out = proj
            .to_bytes()
            .unwrap_or_else(|e| panic!("to_bytes {name}: {e}"));
        assert_eq!(
            out.len(),
            original.len(),
            "{name}: length differs ({} vs {})",
            out.len(),
            original.len()
        );
        assert!(
            out == original,
            "{name}: bytes differ (first diff at {:?})",
            out.iter().zip(&original).position(|(a, b)| a != b)
        );
    }

    if let Some(original) = read_optional_treant_candidate() {
        let name = OPTIONAL_TREANT_CANDIDATE;
        let proj = Project::from_bytes(original.clone())
            .unwrap_or_else(|error| panic!("parse {name}: {error}"));
        let out = proj
            .to_bytes()
            .unwrap_or_else(|error| panic!("to_bytes {name}: {error}"));
        assert_eq!(out, original, "{name}: bytes differ after round-trip");
    }
}

#[test]
fn empty_template_roundtrips() {
    // The embedded template must round-trip even without the corpus dir.
    let proj = Project::new_empty();
    assert_eq!(proj.model_count(), 0);
    let bytes = proj.to_bytes().expect("empty to_bytes");
    assert_eq!(bytes.len(), 420);
    let reparsed = Project::from_bytes(bytes.clone()).expect("reparse empty");
    assert_eq!(reparsed.to_bytes().unwrap(), bytes);
}
