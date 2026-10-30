//! Malformed input: the reader returns an error, never panics, and never allocates
//! from an unchecked count (design section 6, "reader survives malformed input").
//! Also: whenever a mutated file still parses, it must write back to the same bytes,
//! so nothing a parse accepts is lost or invented by a rewrite.

use crate::common::{native_dir, sample_bytes};
use crate::props::{config, project};
use plinth_chitubox::{ParseErrorKind, parse};
use proptest::prelude::*;
use std::sync::OnceLock;

fn two_supports() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| std::fs::read(native_dir().join("two-supports.chitubox")).unwrap())
}

fn set_u32(b: &mut [u8], at: usize, v: u32) {
    if let Some(dst) = b.get_mut(at..at + 4) {
        dst.copy_from_slice(&v.to_le_bytes());
    }
}

fn get_u32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// If `bytes` parses, it must rewrite to itself.
fn check(bytes: &[u8]) -> Result<(), TestCaseError> {
    if let Ok(p) = parse(bytes) {
        prop_assert_eq!(p.to_bytes().unwrap(), bytes);
    }
    Ok(())
}

/// Extreme values for 32-bit fields: zero, off by one, huge, sign bit.
fn extreme() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(0u32),
        Just(1),
        Just(u32::MAX),
        Just(u32::MAX - 1),
        Just(0x8000_0000),
        Just(0x7fff_ffff),
        Just(36),
        any::<u32>(),
    ]
}

/// Positions worth hitting in a one-model supported file: header, preview header,
/// model record, front block, and the support section at the end.
fn structural_position(len: usize, off8: usize) -> impl Strategy<Value = usize> {
    prop_oneof![
        0usize..24,
        420usize..452,
        off8..off8 + 300,
        off8 + 680..off8 + 800,
        len.saturating_sub(1500)..len,
        0usize..len,
    ]
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn malformed_random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..2000)) {
        check(&bytes)?;
    }

    #[test]
    fn malformed_truncated_files_are_errors(p in project(), cut in 0.0f64..1.0) {
        let bytes = p.to_bytes().unwrap();
        // Every strict prefix of a valid file is an error: the last byte is always
        // part of a region.
        let n = ((bytes.len() as f64) * cut) as usize;
        prop_assume!(n < bytes.len());
        prop_assert!(parse(&bytes[..n]).is_err());
    }

    #[test]
    fn malformed_extended_files_are_errors(p in project(), extra in prop::collection::vec(any::<u8>(), 1..20)) {
        let mut bytes = p.to_bytes().unwrap();
        bytes.extend_from_slice(&extra);
        prop_assert!(matches!(
            parse(&bytes).unwrap_err().kind,
            ParseErrorKind::TrailingBytes { .. } | ParseErrorKind::Mismatch { .. } | ParseErrorKind::NonZero { .. }
        ), "trailing bytes accepted");
    }

    #[test]
    fn malformed_mutated_generated_files(
        p in project(),
        edits in prop::collection::vec((any::<prop::sample::Index>(), extreme()), 1..4),
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..4),
    ) {
        let mut bytes = p.to_bytes().unwrap();
        for (idx, v) in &edits {
            let at = idx.index(bytes.len());
            set_u32(&mut bytes, at, *v);
        }
        for (idx, v) in &flips {
            let at = idx.index(bytes.len());
            bytes[at] ^= *v;
        }
        check(&bytes)?;
    }

    #[test]
    fn malformed_mutated_native_supported_file(
        at in structural_position(two_supports().len(), get_u32(two_supports(), 8) as usize),
        v in extreme(),
        flip in any::<u8>(),
        cut in prop::option::of(0usize..2000),
    ) {
        let mut bytes = two_supports().to_vec();
        if flip & 1 == 0 {
            set_u32(&mut bytes, at, v);
        } else {
            bytes[at] ^= flip;
        }
        if let Some(c) = cut {
            bytes.truncate(bytes.len().saturating_sub(c));
        }
        check(&bytes)?;
    }
}

#[test]
fn malformed_hostile_counts_do_not_allocate() {
    // A 420-byte header that claims 4 billion models, a gigantic preview, and so on.
    let mut base = std::fs::read(native_dir().join("empty.chitubox")).unwrap();
    set_u32(&mut base, 4, u32::MAX);
    assert!(parse(&base).is_err());

    let mut b = sample_bytes("01-round25-plain");
    set_u32(&mut b, 420, u32::MAX); // preview width
    set_u32(&mut b, 424, u32::MAX); // preview height
    assert!(parse(&b).is_err());

    let mut b = sample_bytes("01-round25-plain");
    set_u32(&mut b, 432, u32::MAX); // preview data length
    assert!(parse(&b).is_err());

    // Support index with a record count of 4 billion.
    let b = sample_bytes("04-round25-supports-raft-connected");
    let off8 = get_u32(&b, 8) as usize;
    let index_off = get_u32(&b, off8 + 268) as usize;
    for field in 0..4 {
        let mut m = b.clone();
        set_u32(&mut m, index_off + 4 * field, u32::MAX);
        assert!(parse(&m).is_err(), "index field {field}");
    }
    // Model mesh size and offset at the extremes.
    for (field, v) in [(272usize, u32::MAX), (276, u32::MAX), (268, u32::MAX)] {
        let mut m = b.clone();
        set_u32(&mut m, off8 + field, v);
        assert!(parse(&m).is_err(), "model field {field}");
    }
    // Descriptor pointers at the extremes.
    let off20 = get_u32(&b, 20) as usize;
    for field in 0..3 {
        let mut m = b.clone();
        m[off20 + 8 * field..off20 + 8 * field + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(parse(&m).is_err(), "descriptor field {field}");
    }
}

#[test]
fn malformed_every_prefix_of_a_small_file_is_an_error() {
    let b = std::fs::read(native_dir().join("empty.chitubox")).unwrap();
    for n in 0..b.len() {
        assert!(parse(&b[..n]).is_err(), "prefix {n}");
    }
    // A supported file: all of the first 2 KB and a stride over the rest.
    let b = two_supports();
    let cuts = (0..2000).chain((2000..b.len()).step_by(997));
    for n in cuts {
        assert!(parse(&b[..n]).is_err(), "prefix {n}");
    }
    assert!(parse(b).is_ok());
}

#[test]
fn malformed_non_finite_numbers_are_errors() {
    let b = sample_bytes("04-round25-supports-raft-connected");
    let off8 = get_u32(&b, 8) as usize;
    for bits in [
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        f32::NEG_INFINITY.to_bits(),
    ] {
        // Translate X.
        let mut m = b.clone();
        set_u32(&mut m, off8 + 256, bits);
        assert!(matches!(
            parse(&m).unwrap_err().kind,
            ParseErrorKind::NonFinite { .. }
        ));
        // First model vertex.
        let mesh_off = get_u32(&b, off8 + 272) as usize;
        let mut m = b.clone();
        set_u32(&mut m, mesh_off, bits);
        assert!(matches!(
            parse(&m).unwrap_err().kind,
            ParseErrorKind::NonFinite { .. }
        ));
        // First support record's start x: records_off from the support index.
        let index_off = get_u32(&b, off8 + 268) as usize;
        let records_off = get_u32(&b, index_off + 4) as usize;
        let mut m = b.clone();
        set_u32(&mut m, records_off + 8, bits);
        assert!(matches!(
            parse(&m).unwrap_err().kind,
            ParseErrorKind::NonFinite { .. }
        ));
    }
}

#[test]
fn malformed_other_container_is_bad_magic() {
    // The .ctp file in the same folder is another format.
    let b = std::fs::read(native_dir().join("cylinder-cube-sphere.ctp")).unwrap();
    assert!(matches!(
        parse(&b).unwrap_err().kind,
        ParseErrorKind::BadMagic(0xAB23_1253)
    ));
}
