//! T1 native: the shape builders against every support record the owner's ChituBox
//! v1.4.0 samples hold (`test-data/chitubox-native/samples/`).
//!
//! Each record's own fields are the parameters. `spec_of` reads them back out of the
//! record (never out of the mesh, except the z of a raft part, which the file stores only
//! in the mesh). The builder then makes a record, and this file compares:
//!
//! * the 72-byte record fields, bit for bit;
//! * every mesh coordinate, in units in the last place (ULP) of f32.
//!
//! Result table (also printed by `native_shape_table`): see the format document, section 19.

use plinth_chitubox::{Project, RecordType, SupportRecord, parse};
use plinth_emit::shapes::{
    FrustumRole, MESH_TOLERANCE_ULPS, ShapeError, Spec, SphereRole, build, mesh_error_ulps,
};
use std::collections::BTreeMap;

/// (sample stem, model index) of every supported model in the v1.4.0 samples.
const SUPPORTED: [(&str, usize); 6] = [
    ("03-round25-supports", 0),
    ("04-round25-supports-raft-connected", 0),
    ("04-round25-supports-raft-hex", 0),
    ("05-two-plates", 0),
    ("05-two-plates", 1),
    ("07-round25-on-edge-braced", 0),
];

fn sample(stem: &str) -> Project {
    let path = format!(
        "{}/../../test-data/chitubox-native/samples/{stem}.chitubox",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    parse(&bytes).unwrap_or_else(|e| panic!("parse {stem}: {e}"))
}

/// Every support record of every supported model, with where it came from.
fn all_records() -> Vec<(String, usize, SupportRecord)> {
    let mut out = Vec::new();
    let mut projects: BTreeMap<&str, Project> = BTreeMap::new();
    for (stem, model) in SUPPORTED {
        let p = projects.entry(stem).or_insert_with(|| sample(stem));
        let s = p.models[model].supports.as_ref().expect("supported model");
        for (i, r) in s.records.iter().enumerate() {
            out.push((format!("{stem}#{model}"), i, r.clone()));
        }
    }
    out
}

fn top_z(r: &SupportRecord) -> f32 {
    r.mesh
        .iter()
        .flatten()
        .map(|v| v[2])
        .fold(f32::NEG_INFINITY, f32::max)
}

/// The kind label and the builder parameters of one native record. `None` means the
/// record type is not one this step knows.
fn spec_of(r: &SupportRecord) -> Option<(&'static str, Spec)> {
    let (start, end, r1, r2) = (r.start, r.end, r.r1, r.r2);
    let ball = |role, label| {
        (
            label,
            Spec::Sphere {
                role,
                start,
                end,
                radius: r1,
            },
        )
    };
    let frustum = |role, label| {
        (
            label,
            Spec::Frustum {
                role,
                start,
                end,
                r1,
                r2,
            },
        )
    };
    Some(match r.record_type {
        RecordType::TIP_BALL => ball(SphereRole::TipBall, "tip ball (7)"),
        RecordType::FOOT_BALL => ball(SphereRole::FootBall, "foot ball (8)"),
        RecordType::JOINT_BALL => ball(SphereRole::JointBall, "joint ball (9)"),
        RecordType::TIP_CONE => (
            "tip cone (1)",
            Spec::TipCone {
                start,
                end,
                r1,
                r2,
                contact: r.tail_f32(0),
            },
        ),
        RecordType::FOOT_CONE => frustum(FrustumRole::FootCone, "foot cone (2)"),
        RecordType::ROD => frustum(FrustumRole::Rod, "rod (3)"),
        RecordType::BASE_FLARE => frustum(FrustumRole::BaseFlare, "base flare (4)"),
        RecordType::BASE_PAD => frustum(FrustumRole::BasePad, "default-mat pad (5)"),
        RecordType::RAFT if start[2] < -1.0e7 => ("hex raft mat (6)", Spec::HexMat),
        RecordType::RAFT if r.tail_f32(0).to_bits() == 1.0f32.to_bits() => (
            "raft bridge (6)",
            Spec::RaftBridge {
                from: [start[0], start[1]],
                to: [end[0], end[1]],
                z_top: top_z(r),
                thickness: r2,
                width: r1,
            },
        ),
        RecordType::RAFT => (
            "raft pad (6)",
            Spec::RaftPad {
                centre: [start[0], start[1]],
                z_top: top_z(r),
                thickness: r2,
                diameter: r1,
            },
        ),
        _ => return None,
    })
}

fn fields_equal(a: &SupportRecord, b: &SupportRecord) -> bool {
    let bits = |v: [f32; 3]| v.map(f32::to_bits);
    a.record_type == b.record_type
        && bits(a.start) == bits(b.start)
        && bits(a.end) == bits(b.end)
        && a.r1.to_bits() == b.r1.to_bits()
        && a.r2.to_bits() == b.r2.to_bits()
        && a.tail == b.tail
}

/// Largest error over all coordinates in ULP of the part's largest coordinate, and whether
/// every coordinate is bit-equal. `None` when the triangle counts differ.
fn mesh_distance(built: &SupportRecord, native: &SupportRecord) -> Option<(f32, bool)> {
    let worst = mesh_error_ulps(&built.mesh, &native.mesh)?;
    let identical = built
        .mesh
        .iter()
        .flatten()
        .flatten()
        .zip(native.mesh.iter().flatten().flatten())
        .all(|(a, b)| a.to_bits() == b.to_bits());
    Some((worst, identical))
}

#[derive(Debug, Default)]
struct Stats {
    records: usize,
    fields_exact: usize,
    mesh_bit_exact: usize,
    mesh_within_tolerance: usize,
    mesh_over_tolerance: usize,
    max_ulps: f32,
    unverified: usize,
    coords: usize,
    coords_bit_exact: usize,
}

fn measure() -> BTreeMap<&'static str, Stats> {
    let mut table: BTreeMap<&'static str, Stats> = BTreeMap::new();
    for (file, idx, native) in all_records() {
        let (label, spec) = spec_of(&native).unwrap_or_else(|| {
            panic!("{file} record {idx}: unknown type {}", native.record_type.0)
        });
        let st = table.entry(label).or_default();
        st.records += 1;
        match build(&spec) {
            Err(ShapeError::Unverified { .. }) => st.unverified += 1,
            Err(e) => panic!("{file} record {idx} ({label}): {e}"),
            Ok(built) => {
                st.fields_exact += usize::from(fields_equal(&built, &native));
                match mesh_distance(&built, &native) {
                    None => st.mesh_over_tolerance += 1,
                    Some((worst, identical)) => {
                        st.max_ulps = st.max_ulps.max(worst);
                        // Mesh size is the same here: mesh_distance returned Some.
                        st.mesh_bit_exact += usize::from(identical);
                        if worst <= MESH_TOLERANCE_ULPS {
                            st.mesh_within_tolerance += 1;
                        } else {
                            st.mesh_over_tolerance += 1;
                            eprintln!("{file} record {idx} ({label}): {worst} ULP");
                        }
                        for (a, b) in built
                            .mesh
                            .iter()
                            .flatten()
                            .flatten()
                            .zip(native.mesh.iter().flatten().flatten())
                        {
                            st.coords += 1;
                            st.coords_bit_exact += usize::from(a.to_bits() == b.to_bits());
                        }
                    }
                }
            }
        }
    }
    table
}

/// Records per kind in the five samples (pinned so a silent skip cannot pass).
const EXPECTED_COUNTS: [(&str, usize); 11] = [
    ("base flare (4)", 62),
    ("default-mat pad (5)", 10),
    ("foot ball (8)", 11),
    ("foot cone (2)", 26),
    ("hex raft mat (6)", 1),
    ("joint ball (9)", 137),
    ("raft bridge (6)", 39),
    ("raft pad (6)", 43),
    ("rod (3)", 131),
    ("tip ball (7)", 97),
    ("tip cone (1)", 97),
];

#[test]
fn native_shape_table() {
    let table = measure();
    let mut total = 0;
    eprintln!(
        "kind | records | fields exact | mesh bit-exact | mesh <= {MESH_TOLERANCE_ULPS} ULP | over | max ULP | coords bit-exact | unverified"
    );
    for (label, s) in &table {
        eprintln!(
            "{label} | {} | {} | {} | {} | {} | {} | {}/{} | {}",
            s.records,
            s.fields_exact,
            s.mesh_bit_exact,
            s.mesh_within_tolerance,
            s.mesh_over_tolerance,
            s.max_ulps,
            s.coords_bit_exact,
            s.coords,
            s.unverified
        );
        total += s.records;
    }
    eprintln!("total records: {total}");
    for (label, want) in EXPECTED_COUNTS {
        let got = table.get(label).map_or(0, |s| s.records);
        assert_eq!(got, want, "record count of {label}");
    }
}

#[test]
fn native_every_verifiable_record_reproduces_within_tolerance() {
    let table = measure();
    assert!(table.len() >= 10, "kinds seen: {}", table.len());
    for (label, s) in &table {
        if s.unverified == s.records {
            continue;
        }
        assert_eq!(s.fields_exact, s.records, "{label}: record fields differ");
        assert_eq!(s.mesh_over_tolerance, 0, "{label}: mesh beyond tolerance");
        assert_eq!(s.mesh_within_tolerance, s.records, "{label}");
        assert!(
            s.max_ulps <= MESH_TOLERANCE_ULPS,
            "{label}: {} ULP",
            s.max_ulps
        );
    }
}

#[test]
fn native_unverified_hex_raft_mat_is_refused_not_guessed() {
    let hex: Vec<_> = all_records()
        .into_iter()
        .filter(|(_, _, r)| r.record_type == RecordType::RAFT && r.start[2] < -1.0e7)
        .collect();
    assert_eq!(hex.len(), 1, "one hex record in the samples");
    let (_, _, native) = &hex[0];
    assert_eq!(native.mesh.len(), 2772);
    let (_, spec) = spec_of(native).unwrap();
    match build(&spec) {
        Err(ShapeError::Unverified { kind, reason }) => {
            // The runner counts lines that start with this mark (`unverified N` on the
            // `scripts/t` verdict line); `.config/nextest.toml` keeps this test's output.
            eprintln!("PLINTH-UNVERIFIED: {kind}: {reason}");
        }
        other => panic!("hex mat must be refused as unverified, got {other:?}"),
    }
}

#[test]
fn native_comparison_catches_a_wrong_shape() {
    // Sabotage: the comparator must be able to fail. Each wrong variant of a real record
    // is built and must come out beyond tolerance or with different fields.
    let recs = all_records();
    let find = |ty: RecordType| {
        recs.iter()
            .find(|(_, _, r)| r.record_type == ty)
            .unwrap()
            .2
            .clone()
    };
    // A default-mat pad built with the radii the wrong way round.
    let pad = find(RecordType::BASE_PAD);
    let wrong = build(&Spec::Frustum {
        role: FrustumRole::Rod,
        start: pad.start,
        end: pad.end,
        r1: pad.r1,
        r2: pad.r2,
    })
    .unwrap();
    assert!(!fields_equal(&wrong, &pad), "type differs");
    let (worst, _) = mesh_distance(&wrong, &pad).unwrap();
    assert!(
        worst > MESH_TOLERANCE_ULPS,
        "radii swap must be seen: {worst}"
    );
    // A ball built with a radius off by 0.01 mm.
    let ball = find(RecordType::JOINT_BALL);
    let off = build(&Spec::Sphere {
        role: SphereRole::JointBall,
        start: ball.start,
        end: ball.end,
        radius: ball.r1 + 0.01,
    })
    .unwrap();
    let (worst, _) = mesh_distance(&off, &ball).unwrap();
    assert!(
        worst > MESH_TOLERANCE_ULPS,
        "radius error must be seen: {worst}"
    );
    // A frustum with its axis tilted by a hair (1e-4 on the end point).
    let rod = find(RecordType::ROD);
    let mut end = rod.end;
    end[0] += 1.0e-4;
    let tilted = build(&Spec::Frustum {
        role: FrustumRole::Rod,
        start: rod.start,
        end,
        r1: rod.r1,
        r2: rod.r2,
    })
    .unwrap();
    let (worst, _) = mesh_distance(&tilted, &rod).unwrap();
    assert!(
        worst > MESH_TOLERANCE_ULPS,
        "axis error must be seen: {worst}"
    );
    // A different triangle count is a failure, not a pass.
    let mut short = rod.clone();
    short.mesh.pop();
    assert!(mesh_distance(&rod, &short).is_none());
}

/// Swap the support records of a one-model sample for records built from their own
/// parameters, write the project, and compare the file with the native bytes: same length,
/// same bytes before the support-mesh blob and after it (so the index, group header, every
/// 72-byte record with its offsets and the trailer are bit-equal), and each record's
/// serialized mesh within `MESH_TOLERANCE_ULPS` of the native mesh.
#[test]
fn native_built_records_write_the_native_file_layout() {
    for stem in [
        "03-round25-supports",
        "04-round25-supports-raft-connected",
        "07-round25-on-edge-braced",
    ] {
        let path = format!(
            "{}/../../test-data/chitubox-native/samples/{stem}.chitubox",
            env!("CARGO_MANIFEST_DIR")
        );
        let native_bytes = std::fs::read(path).unwrap();
        let mut project = parse(&native_bytes).unwrap();
        assert_eq!(project.models.len(), 1, "{stem}");
        let supports = project.models[0].supports.as_mut().unwrap();
        let n = supports.records.len();
        let specs: Vec<Spec> = supports
            .records
            .iter()
            .map(|r| spec_of(r).unwrap().1)
            .collect();
        let built = plinth_emit::shapes::build_all(&specs).unwrap();
        let native_meshes: Vec<Vec<plinth_chitubox::Triangle>> =
            supports.records.iter().map(|r| r.mesh.clone()).collect();
        let blob: usize = native_meshes.iter().map(|m| m.len() * 36).sum();
        supports.records = built;
        let bytes = project.to_bytes().unwrap();
        assert_eq!(bytes.len(), native_bytes.len(), "{stem}: file size");
        // Layout: ... model mesh | blob | index 16 | group header 400 | records 72 n | trailer 20.
        let tail = 16 + 400 + 72 * n + 20;
        let blob_end = bytes.len() - tail;
        let blob_start = blob_end - blob;
        assert_eq!(
            bytes[..blob_start],
            native_bytes[..blob_start],
            "{stem}: head"
        );
        assert_eq!(
            bytes[blob_end..],
            native_bytes[blob_end..],
            "{stem}: index and records"
        );
        // Read the serialized blob back as one mesh per record (36 bytes a triangle) and
        // compare each with the native mesh by the same metric the in-memory test uses: the
        // largest error in ULP of that part's largest coordinate, at most the tolerance.
        let mut at = blob_start;
        for (i, native_mesh) in native_meshes.iter().enumerate() {
            let end = at + native_mesh.len() * 36;
            let written = triangles(&bytes[at..end]);
            let worst = mesh_error_ulps(&written, native_mesh)
                .unwrap_or_else(|| panic!("{stem} record {i}: mesh size differs or not finite"));
            assert!(
                worst <= MESH_TOLERANCE_ULPS,
                "{stem} record {i}: serialized mesh is {worst} ULP from native"
            );
            at = end;
        }
        assert_eq!(at, blob_end, "{stem}: blob length");
    }
}

/// Decode little-endian bytes (36 per triangle) into triangles.
fn triangles(b: &[u8]) -> Vec<plinth_chitubox::Triangle> {
    let f = |c: &[u8]| f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    b.chunks_exact(36)
        .map(|t| {
            let v = |k: usize| [f(&t[k * 12..]), f(&t[k * 12 + 4..]), f(&t[k * 12 + 8..])];
            [v(0), v(1), v(2)]
        })
        .collect()
}
