//! Release-only evidence gate for the independent indexed delicacy field.
//!
//! This deliberately parses binary STL locally: `plinth-metrics` must not gain
//! a producer-mesh dependency merely to time an audit.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use plinth_metrics::{DelicacyField, Mesh, Triangle};

fn main() -> Result<(), String> {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "missing binary STL path".to_owned())?;
    let mesh = read_binary_stl(&path)?;
    let started = Instant::now();
    let field =
        DelicacyField::build_diagnostic(0, &mesh, 0, 20_000).map_err(|error| error.to_string())?;
    let elapsed = started.elapsed();
    let stats = field.build_stats();
    println!("path={}", path.display());
    println!("faces={}", mesh.triangles().len());
    println!("samples={}", field.samples().len());
    println!("selected_faces={}", field.selected_face_indices().len());
    println!("duration_ms={:.3}", elapsed.as_secs_f64() * 1_000.0);
    match rss_bytes() {
        Some(bytes) => println!("rss_bytes={bytes}"),
        None => println!("rss_bytes=unavailable"),
    }
    println!("triangle_nodes_visited={}", stats.triangle_nodes_visited);
    println!("triangle_faces_tested={}", stats.triangle_faces_tested);
    println!("vertex_grid_candidates={}", stats.vertex_grid_candidates);
    println!("sample_kd_nodes_visited={}", stats.sample_kd_nodes_visited);
    println!("centre_kd_nodes_visited={}", stats.centre_kd_nodes_visited);
    println!("digest={:016x}", field_digest(&field));
    Ok(())
}

fn read_binary_stl(path: &Path) -> Result<Mesh, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("binary STL {}: {error}", path.display()))?;
    if bytes.len() < 84 {
        return Err(format!(
            "malformed binary STL {}: shorter than header",
            path.display()
        ));
    }
    let count = u32::from_le_bytes(bytes[80..84].try_into().expect("fixed-width count")) as usize;
    let expected = count
        .checked_mul(50)
        .and_then(|body| body.checked_add(84))
        .ok_or_else(|| {
            format!(
                "malformed binary STL {}: triangle count overflow",
                path.display()
            )
        })?;
    if bytes.len() != expected {
        return Err(format!(
            "malformed binary STL {}: expected {expected} bytes for {count} triangles, found {}",
            path.display(),
            bytes.len()
        ));
    }
    let mut vertices = Vec::with_capacity(count * 3);
    let mut triangles = Vec::with_capacity(count);
    for face in 0..count {
        let offset = 84 + face * 50 + 12;
        let first = vertices.len();
        for vertex in 0..3 {
            let point = offset + vertex * 12;
            vertices.push([
                f64::from(f32::from_le_bytes(
                    bytes[point..point + 4].try_into().expect("x"),
                )),
                f64::from(f32::from_le_bytes(
                    bytes[point + 4..point + 8].try_into().expect("y"),
                )),
                f64::from(f32::from_le_bytes(
                    bytes[point + 8..point + 12].try_into().expect("z"),
                )),
            ]);
        }
        triangles.push([first, first + 1, first + 2] as Triangle);
    }
    Mesh::new(vertices, triangles)
        .map_err(|error| format!("malformed binary STL {}: {error}", path.display()))
}

fn rss_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let kibibytes = String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    kibibytes.checked_mul(1_024)
}

fn field_digest(field: &DelicacyField) -> u64 {
    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
    digest_bytes(
        &mut digest,
        &(field.exterior_mask().len() as u64).to_le_bytes(),
    );
    for exterior in field.exterior_mask() {
        digest_bytes(&mut digest, &[u8::from(*exterior)]);
    }
    digest_bytes(
        &mut digest,
        &(field.selected_face_indices().len() as u64).to_le_bytes(),
    );
    for face in field.selected_face_indices() {
        digest_bytes(&mut digest, &(*face as u64).to_le_bytes());
    }
    digest_bytes(&mut digest, &(field.samples().len() as u64).to_le_bytes());
    for (sample, weight) in field.samples().iter().zip(field.weights()) {
        for value in sample.point.into_iter().chain(sample.normal) {
            digest_bytes(&mut digest, &value.to_bits().to_le_bytes());
        }
        digest_bytes(&mut digest, &(sample.face_index as u64).to_le_bytes());
        digest_bytes(&mut digest, &weight.to_bits().to_le_bytes());
    }
    digest
}

fn digest_bytes(digest: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *digest ^= u64::from(*byte);
        *digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
    }
}
