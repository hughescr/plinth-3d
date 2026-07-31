//! Gold invariant (a): `open -> to_bytes` is byte-identical to the input for
//! every corpus specimen, including the three transform probes and the large
//! supported files. `to_bytes` also runs the structural verify + interval
//! audit internally, so a green test proves gold (a) and (b) together.

mod common;
use chitubox_io::Project;
use common::*;

#[test]
fn corpus_roundtrip_byte_identical() {
    let mut ran = 0;
    for name in ROUNDTRIP_FILES {
        let Some(original) = read_corpus(name) else {
            continue;
        };
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
        ran += 1;
    }
    assert!(ran > 0, "no corpus files were available to round-trip");
    eprintln!("round-tripped {ran} corpus files byte-identically");
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
