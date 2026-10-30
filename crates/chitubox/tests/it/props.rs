//! T0p: write -> read is lossless for any valid project. Fixed seed, 64 cases (design
//! section 6). The L4 checkpoint raises the case count and changes the seed.

use plinth_chitubox::{
    Model, Preset, Preview, Project, RecordType, SupportRecord, Supports, Triangle, WriteError,
    parse, preview::MAX_PIXELS,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

pub fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        rng_seed: RngSeed::Fixed(0x00c4_17b0_5e11),
        ..ProptestConfig::default()
    }
}

fn coord() -> impl Strategy<Value = f32> {
    prop_oneof![
        -1000.0f32..1000.0,
        Just(0.0),
        Just(-0.0),
        Just(f32::MIN_POSITIVE)
    ]
}

fn point() -> impl Strategy<Value = [f32; 3]> {
    prop::array::uniform3(coord())
}

fn mesh(max: usize) -> impl Strategy<Value = Vec<Triangle>> {
    prop::collection::vec(prop::array::uniform3(point()), 0..max)
}

fn record() -> impl Strategy<Value = SupportRecord> {
    (
        0u32..12,
        point(),
        point(),
        (coord(), coord()),
        prop::array::uniform6(any::<u32>()),
        mesh(4),
    )
        .prop_map(|(t, start, end, (r1, r2), tail, mesh)| SupportRecord {
            record_type: RecordType(t),
            start,
            end,
            r1,
            r2,
            tail,
            mesh,
        })
}

fn supports() -> impl Strategy<Value = Supports> {
    (
        prop::collection::vec((any::<u64>(), "[A-Za-z]{0,10}"), 0..5),
        prop::collection::vec(any::<u64>(), 0..8),
        prop::collection::vec(any::<u8>(), 0..50),
        prop::collection::vec(record(), 0..6),
    )
        .prop_map(|(presets, attrs, unreferenced_blob, records)| Supports {
            presets: presets
                .into_iter()
                .map(|(id, name)| Preset { id, name })
                .collect(),
            attrs,
            unreferenced_blob,
            records,
        })
}

fn model() -> impl Strategy<Value = Model> {
    (
        "[A-Za-z0-9 #._-]{0,40}",
        point(),
        mesh(5),
        prop::option::of(supports()),
    )
        .prop_map(|(name, translate, mesh, supports)| Model {
            name,
            translate,
            mesh,
            supports,
        })
}

fn preview() -> impl Strategy<Value = Option<Preview>> {
    let palette = prop::sample::select(vec![0x0841u16, 0x7c00, 0x03c0, 0x001f, 0x0000]);
    prop::option::of((0u32..9, 0u32..9).prop_flat_map(move |(w, h)| {
        prop::collection::vec(palette.clone(), (w * h) as usize).prop_map(move |pixels| Preview {
            width: w,
            height: h,
            pixels,
        })
    }))
}

pub fn project() -> impl Strategy<Value = Project> {
    (
        any::<u32>(),
        preview(),
        prop::collection::vec(model(), 0..4),
    )
        .prop_map(|(header_f16, preview, models)| Project {
            header_f16,
            preview,
            models,
        })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn write_then_read_is_lossless(p in project()) {
        let bytes = p.to_bytes().unwrap();
        let back = parse(&bytes).unwrap();
        prop_assert_eq!(&back, &p);
        // And the second write is byte-identical: offsets are a function of the data.
        prop_assert_eq!(back.to_bytes().unwrap(), bytes);
    }
}

proptest! {
    #![proptest_config(config())]

    /// Hostile preview sizes: whatever the dimensions, building or writing a preview
    /// returns an error or a file the reader accepts; it never panics or allocates first.
    #[test]
    fn hostile_preview_dimensions_never_panic_and_never_write_unreadable_files(
        width in prop_oneof![any::<u32>(), 0u32..70_000_000],
        height in prop_oneof![any::<u32>(), 0u32..70_000_000],
        len in 0usize..4,
    ) {
        let over = u64::from(width) * u64::from(height) > MAX_PIXELS;
        let built = Preview::solid(width, height, 0x0841);
        if over {
            prop_assert!(built.is_err());
        }
        let p = Project {
            preview: Some(Preview { width, height, pixels: vec![0x0841; len] }),
            ..Project::default()
        };
        // Four pixels at most, so only a tiny size can match the pixel count.
        match p.to_bytes() {
            Ok(bytes) => {
                prop_assert!(!over);
                prop_assert_eq!(parse(&bytes).unwrap(), p);
            }
            Err(e) => prop_assert_eq!(e, WriteError::BadPreview),
        }
    }
}

#[test]
fn writer_rebuilds_offsets_whatever_the_old_ones_were() {
    // Grow a native-shaped project and check the rebuilt layout parses: add a record,
    // an attr, a model and a longer preview, all in one edit.
    let mut p = Project {
        preview: Some(Preview::solid(3, 3, 0x0841).unwrap()),
        models: vec![Model {
            name: "a".into(),
            translate: [0.0, 0.0, 3.0],
            mesh: vec![[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
            supports: Some(Supports::default()),
        }],
        ..Project::default()
    };
    let before = p.to_bytes().unwrap();
    {
        let m = &mut p.models[0];
        let s = m.supports.as_mut().unwrap();
        s.presets.push(Preset {
            id: 1,
            name: "Ultralight".into(),
        });
        s.attrs.push(0x104);
        s.records.push(SupportRecord {
            record_type: RecordType::TIP_BALL,
            start: [0.0; 3],
            end: [0.0, 0.0, 0.2],
            r1: 0.1,
            r2: 0.1,
            tail: [0; 6],
            mesh: vec![[[0.0; 3]; 3]; 4],
        });
    }
    p.models.push(p.models[0].clone());
    p.preview = Some(Preview::solid(40, 30, 0x03c0).unwrap());
    let after = p.to_bytes().unwrap();
    assert!(after.len() > before.len());
    assert_eq!(parse(&after).unwrap(), p);
    assert!(p.deviations().is_empty());
}
