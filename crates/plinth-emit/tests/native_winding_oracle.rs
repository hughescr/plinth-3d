//! The winding algorithm validated against native ChituBox geometry.
//!
//! `docs/specs/chitubox-format.md` §2 rules that the Python trimesh
//! `mass_properties` winding check is REPLACED by a native f64
//! signed-tetrahedron-volume sum in `geom_kernel`, and that the chosen
//! algorithm must be validated against the **native** ground-truth fixtures of
//! §3 — for PAD/BRIDGE specifically against the only two natives carrying
//! type-6 geometry.  That is what this file does.
//!
//! Why this is not circular: every soup read here was authored by ChituBox
//! itself, never by this workspace's writer.  The support meshes are unwelded
//! triangle soups — loose frustum, sphere, pad and bridge triangles with no
//! shared vertex table — which is precisely the case `chitubox_emit.py`'s
//! docstring recorded trimesh disagreeing on.  The claim under test is that
//! `Σ v0 · (v1 × v2) / 6` reproduces ChituBox's own outward convention on them,
//! and it is checked against ChituBox's bytes, not against our emitter.
//!
//! Deliberately excluded, per the same ruling: the regenerated `sphere-*` /
//! `cylinder-*` set and `pad_bridge_donor.chitubox`, which were written by
//! `chitubox_io` and are therefore circular for writer bugs.
//!
//! Also deliberately excluded, and measured rather than assumed:
//! `morel_mudskipper_supports.chitubox` is native, but its 243 records are
//! *composite* soups — a whole support merged into one 544-triangle record —
//! and 210 of the 243 are OPEN (48 boundary edges each).  A tetrahedron sum
//! taken from the origin is only translation-invariant for a CLOSED soup, so
//! on an open one its sign is a function of where the support happens to sit:
//! 77 of morel's 243 read negative from the origin while only 15 do about
//! their own centroid.  That is not a defect in the algorithm and not a
//! fixture to "fix" — it is the algorithm's precondition, pinned below by
//! `only_closed_soups_are_decidable_and_the_natives_used_here_are_closed`, and
//! on the emitter's own side by
//! `every_emitted_soup_is_closed_which_the_winding_decision_requires` in
//! `src/lib.rs`.  This crate never writes a merged support record; it writes
//! one closed convex primitive per segment, exactly like the three specimens
//! used here.

use std::path::PathBuf;

use chitubox_io::{Project, SegmentType, SupportRecord};
use geom_kernel::{signed_soup_volumes, wind_soups_outward, SoupTriangle};

/// Native (never regenerated) specimens whose support records are per-segment
/// closed primitives — the shape this crate emits.
const NATIVE_SPECIMENS: [&str; 3] = [
    "two-supports.chitubox",
    "basic-cube-supports-raft.chitubox",
    "cylinder-supported-bottom-connected.chitubox",
];

/// Native, but merged and open; see the module note.
const NATIVE_MERGED_SPECIMEN: &str = "morel_mudskipper_supports.chitubox";

/// The only two natives that carry real type-6 PAD/BRIDGE geometry.
const NATIVE_PAD_BRIDGE_SPECIMENS: [&str; 2] = [
    "basic-cube-supports-raft.chitubox",
    "cylinder-supported-bottom-connected.chitubox",
];

/// Committed fixtures fail by their named path; they never skip.
fn native_records(name: &str) -> Vec<SupportRecord> {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/tests/fixtures/chitubox")
        .join(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("required native fixture {}: {error}", path.display()));
    let project = Project::from_bytes(bytes)
        .unwrap_or_else(|error| panic!("native fixture {name} did not parse: {error}"));
    project.supports(0).unwrap_or_else(|error| {
        panic!("native fixture {name} carries no support records on model 0: {error}")
    })
}

/// Widen ChituBox's binary32 wire soup to the f64 the kernel works in.
fn wire_soup(xyz: &[f32]) -> Vec<SoupTriangle> {
    xyz.chunks_exact(9)
        .map(|t| {
            [
                [f64::from(t[0]), f64::from(t[1]), f64::from(t[2])],
                [f64::from(t[3]), f64::from(t[4]), f64::from(t[5])],
                [f64::from(t[6]), f64::from(t[7]), f64::from(t[8])],
            ]
        })
        .collect()
}

fn reversed(soups: &[Vec<SoupTriangle>]) -> Vec<Vec<SoupTriangle>> {
    soups
        .iter()
        .map(|soup| {
            soup.iter()
                .map(|triangle| [triangle[0], triangle[2], triangle[1]])
                .collect()
        })
        .collect()
}

fn volumes(soups: &[Vec<SoupTriangle>]) -> Vec<f64> {
    let borrowed: Vec<&[SoupTriangle]> = soups.iter().map(Vec::as_slice).collect();
    signed_soup_volumes(&borrowed)
        .expect("native soups have a finite signed volume")
        .volumes_mm3()
        .to_vec()
}

/// Unmatched surface of a soup as a fraction of its total area.
///
/// `Σ (v1-v0)×(v2-v0)` cancels to zero over a closed surface and equals the
/// missing patch over an open one — and it is exactly the term that makes the
/// tetrahedron sum frame-dependent, since translating a soup by `t` shifts its
/// signed volume by `t · Σ(v1-v0)×(v2-v0) / 6`.  No vertex welding is involved.
fn relative_closure_defect(soup: &[SoupTriangle]) -> f64 {
    let mut total = [0.0_f64; 3];
    let mut area = 0.0;
    for triangle in soup {
        let edge0 = [
            triangle[1][0] - triangle[0][0],
            triangle[1][1] - triangle[0][1],
            triangle[1][2] - triangle[0][2],
        ];
        let edge1 = [
            triangle[2][0] - triangle[0][0],
            triangle[2][1] - triangle[0][1],
            triangle[2][2] - triangle[0][2],
        ];
        let normal = [
            edge0[1] * edge1[2] - edge0[2] * edge1[1],
            edge0[2] * edge1[0] - edge0[0] * edge1[2],
            edge0[0] * edge1[1] - edge0[1] * edge1[0],
        ];
        for (axis, value) in normal.iter().enumerate() {
            total[axis] += value;
        }
        area += (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    }
    if area == 0.0 {
        return 0.0;
    }
    (total[0] * total[0] + total[1] * total[1] + total[2] * total[2]).sqrt() / area
}

/// Second, independent orientation oracle used only as a cross-check on the
/// convex native primitives: every face of a convex body wound outward has its
/// geometric normal pointing away from the body's centroid.  It forms no
/// tetrahedron sum, so agreement between the two is real agreement.
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn faces_wound_outward(soup: &[SoupTriangle]) -> (usize, usize) {
    let mut centroid = [0.0_f64; 3];
    let mut corners = 0.0;
    for triangle in soup {
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
    for triangle in soup {
        let edge0 = [
            triangle[1][0] - triangle[0][0],
            triangle[1][1] - triangle[0][1],
            triangle[1][2] - triangle[0][2],
        ];
        let edge1 = [
            triangle[2][0] - triangle[0][0],
            triangle[2][1] - triangle[0][1],
            triangle[2][2] - triangle[0][2],
        ];
        let normal = [
            edge0[1] * edge1[2] - edge0[2] * edge1[1],
            edge0[2] * edge1[0] - edge0[0] * edge1[2],
            edge0[0] * edge1[1] - edge0[1] * edge1[0],
        ];
        if normal == [0.0; 3] {
            continue;
        }
        checked += 1;
        let radial = [
            (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0 - centroid[0],
            (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0 - centroid[1],
            (triangle[0][2] + triangle[1][2] + triangle[2][2]) / 3.0 - centroid[2],
        ];
        if normal[0] * radial[0] + normal[1] * radial[1] + normal[2] * radial[2] > 0.0 {
            outward += 1;
        }
    }
    (checked, outward)
}

#[test]
fn every_native_support_soup_reads_as_outward_under_the_kernel_sum() {
    let mut total = 0;
    for name in NATIVE_SPECIMENS {
        let records = native_records(name);
        assert!(!records.is_empty(), "{name} has no support records");
        let soups: Vec<Vec<SoupTriangle>> = records
            .iter()
            .map(|record| wire_soup(&record.mesh.xyz))
            .collect();
        for (index, (record, volume)) in records.iter().zip(volumes(&soups)).enumerate() {
            assert!(
                volume > 0.0,
                "{name} record {index} (type {}) reads as {volume:e}; ChituBox's own geometry \
                 must read strictly outward",
                record.segment_type.0
            );
        }
        total += records.len();
    }
    // A guard against this test quietly validating nothing if a fixture is
    // ever swapped for one without supports.
    assert!(
        total >= 250,
        "only {total} native support soups were checked"
    );
}

#[test]
fn the_face_normal_oracle_agrees_with_the_sum_on_every_native_face() {
    for name in NATIVE_SPECIMENS {
        for (index, record) in native_records(name).iter().enumerate() {
            let soup = wire_soup(&record.mesh.xyz);
            let (checked, outward) = faces_wound_outward(&soup);
            assert!(checked > 0, "{name} record {index} has no usable face");
            assert_eq!(
                checked,
                outward,
                "{name} record {index} (type {}): the face-normal oracle calls {} of {checked} \
                 faces inward while the tetrahedron sum calls the soup outward",
                record.segment_type.0,
                checked - outward
            );
        }
    }
}

/// The stated precondition of the algorithm, measured on both sides.
#[test]
fn only_closed_soups_are_decidable_and_the_natives_used_here_are_closed() {
    for name in NATIVE_SPECIMENS {
        for (index, record) in native_records(name).iter().enumerate() {
            let defect = relative_closure_defect(&wire_soup(&record.mesh.xyz));
            assert!(
                defect <= 1e-6,
                "{name} record {index} leaks {defect:e} of its area and is not a closed soup"
            );
        }
    }

    // The excluded specimen, pinned rather than merely described: its merged
    // records are open, which is exactly why an origin-referenced sum cannot
    // decide them and why this file does not use it as an orientation oracle.
    let merged = native_records(NATIVE_MERGED_SPECIMEN);
    let soups: Vec<Vec<SoupTriangle>> = merged
        .iter()
        .map(|record| wire_soup(&record.mesh.xyz))
        .collect();
    let open = soups
        .iter()
        .filter(|soup| relative_closure_defect(soup) > 1e-6)
        .count();
    assert!(
        open > 0,
        "{NATIVE_MERGED_SPECIMEN} is exercised here only as the open-soup case"
    );
    let negative = volumes(&soups).iter().filter(|v| **v < 0.0).count();
    assert!(
        negative > 0 && negative <= open,
        "every sign-unstable record must be one of the open ones: {negative} negative, {open} open"
    );
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn reversing_a_native_soup_exactly_negates_its_volume_and_winding_restores_it() {
    for name in NATIVE_SPECIMENS {
        let records = native_records(name);
        let native: Vec<Vec<SoupTriangle>> = records
            .iter()
            .map(|record| wire_soup(&record.mesh.xyz))
            .collect();
        let native_volumes = volumes(&native);

        // Corner-swap negation is exact in IEEE-754: fl(a - b) == -fl(b - a),
        // so each tetrahedron term and every partial sum negates bit-for-bit.
        let mut flipped = reversed(&native);
        let flipped_volumes = volumes(&flipped);
        for (index, (native_volume, flipped_volume)) in
            native_volumes.iter().zip(&flipped_volumes).enumerate()
        {
            assert_eq!(
                *flipped_volume, -*native_volume,
                "{name} record {index} did not negate exactly"
            );
            assert!(*flipped_volume < 0.0);
        }

        // The winding decision must recognize every reversed native as inward
        // and put it back exactly where ChituBox had it.
        let measured = wind_soups_outward(&mut flipped).expect("finite native geometry");
        assert_eq!(measured.volumes_mm3(), flipped_volumes.as_slice());
        assert_eq!(flipped, native, "{name} was not restored to native winding");

        // And must leave an already-outward native completely alone.
        let mut untouched = native.clone();
        wind_soups_outward(&mut untouched).expect("finite native geometry");
        assert_eq!(
            untouched, native,
            "{name} was flipped when it should not be"
        );
    }
}

#[test]
fn native_pad_and_bridge_geometry_validates_the_type_six_case() {
    for name in NATIVE_PAD_BRIDGE_SPECIMENS {
        let records = native_records(name);
        let type_six: Vec<&SupportRecord> = records
            .iter()
            .filter(|record| record.segment_type == SegmentType::PAD_OR_BRIDGE)
            .collect();
        assert!(
            !type_six.is_empty(),
            "{name} is required to carry native type-6 geometry"
        );

        let soups: Vec<Vec<SoupTriangle>> = type_six
            .iter()
            .map(|record| wire_soup(&record.mesh.xyz))
            .collect();
        let mut pads = 0;
        let mut bridges = 0;
        for (index, (soup, volume)) in soups.iter().zip(volumes(&soups)).enumerate() {
            assert!(
                volume > 0.0,
                "{name} type-6 record {index} reads as {volume:e}"
            );
            // The reverse-engineered pad/bridge contract these two natives are
            // the source of: an 80-triangle pad or a 12-triangle bridge.
            match soup.len() {
                80 => pads += 1,
                12 => bridges += 1,
                other => panic!("{name} type-6 record {index} has {other} triangles"),
            }
            // Both are convex, so the independent face-normal oracle must
            // agree with the tetrahedron sum on every single face.
            let (checked, outward) = faces_wound_outward(soup);
            assert!(checked > 0);
            assert_eq!(
                checked,
                outward,
                "{name} type-6 record {index}: the two orientation oracles disagree on \
                 {} of {checked} faces",
                checked - outward
            );
        }
        assert!(pads > 0, "{name} carries no native PAD");
        assert!(bridges > 0, "{name} carries no native BRIDGE");
    }
}

#[test]
fn the_native_donor_covers_every_segment_type_the_emitter_writes() {
    let records = native_records("two-supports.chitubox");
    let soups: Vec<Vec<SoupTriangle>> = records
        .iter()
        .map(|record| wire_soup(&record.mesh.xyz))
        .collect();
    let measured = volumes(&soups);
    for wanted in [
        SegmentType::TIP,
        SegmentType::TAPER,
        SegmentType::SHAFT,
        SegmentType::JOINER,
        SegmentType::FLARE,
        SegmentType::PAD_OR_BRIDGE,
    ] {
        let present: Vec<f64> = records
            .iter()
            .zip(&measured)
            .filter(|(record, _)| record.segment_type == wanted)
            .map(|(_, volume)| *volume)
            .collect();
        assert!(
            !present.is_empty(),
            "the native donor carries no type {} record",
            wanted.0
        );
        assert!(
            present.iter().all(|volume| *volume > 0.0),
            "type {} reads inward in the native donor: {present:?}",
            wanted.0
        );
    }
}
