//! T1: every ChituBox-written file parses and rewrites byte for byte.

use crate::common::{native_files, sample};
use plinth_chitubox::{Deviation, parse};

#[test]
fn native_all_files_round_trip_byte_for_byte() {
    let files = native_files();
    // 7 older files and the 8 v1.4.0 samples. (The format document says "17 files";
    // the folder holds 15 `.chitubox` files. `cylinder-cube-sphere.ctp` is another
    // format, tested below, and the `.cfgx` is a profile export.)
    assert_eq!(files.len(), 15, "expected 7 older files and 8 samples");
    let mut supported_models = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(path).unwrap();
        let project = parse(&bytes).unwrap_or_else(|e| panic!("{name}: parse: {e}"));
        let back = project
            .to_bytes()
            .unwrap_or_else(|e| panic!("{name}: write: {e}"));
        assert_eq!(back.len(), bytes.len(), "{name}: length");
        assert!(back == bytes, "{name}: bytes differ after rewrite");
        // Parsing the rewrite gives the same typed data.
        assert_eq!(parse(&back).unwrap(), project, "{name}: reparse");
        supported_models += project
            .models
            .iter()
            .filter(|m| m.supports.is_some())
            .count();
    }
    // The supported path was exercised, not skipped.
    assert!(
        supported_models >= 12,
        "only {supported_models} supported models"
    );
}

#[test]
fn native_only_the_mudskipper_file_deviates_from_a_clean_save() {
    for path in native_files() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let project = parse(&std::fs::read(&path).unwrap()).unwrap();
        let dev = project.deviations();
        if name == "morel_mudskipper_supports.chitubox" {
            assert_eq!(dev.len(), 2, "{dev:?}");
            assert!(dev.contains(&Deviation::AttrCount {
                model: 0,
                attrs: 13,
                records: 243
            }));
            assert_eq!(
                dev[1],
                Deviation::UnreferencedBlob {
                    model: 0,
                    bytes: 88_272
                }
            );
        } else {
            assert!(dev.is_empty(), "{name}: {dev:?}");
        }
    }
}

#[test]
fn native_empty_file_and_header_constants() {
    let dir = crate::common::native_dir();
    let bytes = std::fs::read(dir.join("empty.chitubox")).unwrap();
    assert_eq!(bytes.len(), 420);
    let p = parse(&bytes).unwrap();
    assert!(p.models.is_empty() && p.preview.is_none());
    assert_eq!(p.header_f16, 412);
    assert_eq!(p.to_bytes().unwrap(), bytes);

    let p = sample("01-round25-plain");
    let pv = p.preview.as_ref().unwrap();
    assert_eq!((pv.width, pv.height), (2103, 1580));
    assert_eq!(pv.pixels.len(), 2103 * 1580);
    assert_eq!(p.models[0].mesh.len(), 2996);
    assert_eq!(p.models[0].name, "Round 25.stl");
}
