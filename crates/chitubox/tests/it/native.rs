//! T1: every ChituBox-written file parses and rewrites byte for byte.

use crate::common::{native_files, sample};
use plinth_chitubox::{Deviation, parse};

#[test]
fn native_all_files_round_trip_byte_for_byte() {
    let files = native_files();
    // 7 older files, the 8 v1.4.0 samples and the ChituBox re-save of a plinth-written
    // file. (`cylinder-cube-sphere.ctp` is another format, tested below, and the `.cfgx`
    // is a profile export. The plinth-written file is not native.)
    assert_eq!(
        files.len(),
        16,
        "expected 7 older files, 8 samples and 1 re-save"
    );
    assert!(
        files.iter().all(|p| p
            .file_name()
            .is_some_and(|n| n != crate::common::PLINTH_WRITTEN)),
        "the plinth-written file must not be in the native set"
    );
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

/// Load test 2026-10-30: the plinth-written Round 50 transport file and its ChituBox
/// v1.4.0 re-save differ only in the preview image and the offsets that follow it.
/// The plinth-written file is read here only for this comparison; it is not native.
#[test]
fn load_test_resave_changed_only_the_preview() {
    let dir = crate::common::load_test_dir();
    let ours_bytes = std::fs::read(dir.join(crate::common::PLINTH_WRITTEN)).unwrap();
    let saved_bytes = std::fs::read(dir.join("Round 50 resaved.keepup.chitubox")).unwrap();
    let ours = parse(&ours_bytes).unwrap();
    let saved = parse(&saved_bytes).unwrap();
    // Name, translate, every vertex and every triangle are unchanged, and there are no supports.
    assert_eq!(ours.models, saved.models);
    assert_eq!(saved.models.len(), 1);
    assert_eq!(saved.models[0].mesh.len(), 6014);
    assert!(saved.models[0].supports.is_none());
    assert_eq!(ours.header_f16, saved.header_f16);
    // The mesh bytes, the last 216,504 bytes of each file, are identical.
    let tail = 6014 * 36;
    assert_eq!(
        ours_bytes[ours_bytes.len() - tail..],
        saved_bytes[saved_bytes.len() - tail..]
    );
    // The preview is the only typed difference: ours is one flat colour, ChituBox's is a render.
    let (po, ps) = (ours.preview.unwrap(), saved.preview.unwrap());
    assert_eq!((po.width, po.height), (800, 600));
    assert_eq!((ps.width, ps.height), (1298, 1140));
    assert!(po.pixels.iter().all(|&p| p == po.pixels[0]));
    assert!(ps.pixels.iter().any(|&p| p != ps.pixels[0]));
    // The saved file is longer by exactly the preview data and nothing else.
    assert_eq!(saved_bytes.len() - ours_bytes.len(), 92_080 - 472);
}
