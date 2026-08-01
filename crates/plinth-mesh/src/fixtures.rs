use crate::{Mesh, MeshError};

const BOX_FACES: [[u32; 3]; 12] = [
    [1, 3, 0],
    [4, 1, 0],
    [0, 3, 2],
    [2, 4, 0],
    [1, 7, 3],
    [5, 1, 4],
    [5, 7, 1],
    [3, 7, 2],
    [6, 4, 2],
    [2, 7, 6],
    [6, 5, 4],
    [7, 5, 6],
];

/// The Python `make_box()` default.
pub fn make_box_default() -> Result<Mesh, MeshError> {
    make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0])
}

/// Native counterpart of `mesh_io.make_box(size, translate)`.
pub fn make_box(size: [f64; 3], translate: [f64; 3]) -> Result<Mesh, MeshError> {
    validate_positive("size.x", size[0])?;
    validate_positive("size.y", size[1])?;
    validate_positive("size.z", size[2])?;
    validate_finite("translate.x", translate[0])?;
    validate_finite("translate.y", translate[1])?;
    validate_finite("translate.z", translate[2])?;
    let mut verts = Vec::with_capacity(8);
    let mut tris = Vec::with_capacity(12);
    append_box(
        &mut verts,
        &mut tris,
        size,
        [translate[0], translate[1], translate[2] + size[2] / 2.0],
    )?;
    Mesh::new(verts, tris)
}

/// The Python `make_overhang_table()` default.
pub fn make_overhang_table() -> Result<Mesh, MeshError> {
    make_overhang_table_with([4.0, 4.0, 20.0], [30.0, 30.0, 4.0], 0.0)
}

pub fn make_overhang_table_with(leg: [f64; 3], top: [f64; 3], z0: f64) -> Result<Mesh, MeshError> {
    validate_dimensions("leg", leg)?;
    validate_dimensions("top", top)?;
    validate_finite("z0", z0)?;
    let mut verts = Vec::with_capacity(40);
    let mut tris = Vec::with_capacity(60);
    let inset = 3.0;
    let xs = [inset + leg[0] / 2.0, top[0] - inset - leg[0] / 2.0];
    let ys = [inset + leg[1] / 2.0, top[1] - inset - leg[1] / 2.0];
    for x in xs {
        for y in ys {
            append_box(
                &mut verts,
                &mut tris,
                leg,
                [x - top[0] / 2.0, y - top[1] / 2.0, z0 + leg[2] / 2.0],
            )?;
        }
    }
    append_box(
        &mut verts,
        &mut tris,
        top,
        [0.0, 0.0, z0 + leg[2] + top[2] / 2.0],
    )?;
    Mesh::new(verts, tris)
}

/// The Python `make_toed_slab()` default.
pub fn make_toed_slab() -> Result<Mesh, MeshError> {
    make_toed_slab_with([30.0, 30.0, 4.0], 15.0)
}

pub fn make_toed_slab_with(top: [f64; 3], z_bottom: f64) -> Result<Mesh, MeshError> {
    validate_dimensions("top", top)?;
    validate_finite("z_bottom", z_bottom)?;
    let mut verts = Vec::with_capacity(76);
    let mut tris = Vec::with_capacity(140);
    append_box(
        &mut verts,
        &mut tris,
        top,
        [0.0, 0.0, z_bottom + top[2] / 2.0],
    )?;
    append_down_cone(&mut verts, &mut tris, 3.0, 4.0, [-8.0, 0.0, z_bottom])?;
    append_down_cone(&mut verts, &mut tris, 1.5, 6.0, [8.0, 0.0, z_bottom])?;
    Mesh::new(verts, tris)
}

/// The Python `make_raking_blade()` default.
pub fn make_raking_blade() -> Result<Mesh, MeshError> {
    make_raking_blade_with([20.0, 20.0, 4.0], 10.0, [0.03, 2.0, 5.0], 85.0)
}

pub fn make_raking_blade_with(
    top: [f64; 3],
    z_bottom: f64,
    blade: [f64; 3],
    rake_degrees: f64,
) -> Result<Mesh, MeshError> {
    validate_dimensions("top", top)?;
    validate_dimensions("blade", blade)?;
    validate_finite("z_bottom", z_bottom)?;
    validate_finite("rake_degrees", rake_degrees)?;
    let mut verts = Vec::with_capacity(16);
    let mut tris = Vec::with_capacity(24);
    append_box(
        &mut verts,
        &mut tris,
        top,
        [0.0, 0.0, z_bottom + top[2] / 2.0],
    )?;
    let blade_start = verts.len();
    append_box(&mut verts, &mut tris, blade, [0.0, 0.0, -blade[2] / 2.0])?;
    let radians = rake_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    for vertex in &mut verts[blade_start..] {
        let x = vertex[0];
        let z = vertex[2];
        vertex[0] = cos * x + sin * z;
        vertex[2] = -sin * x + cos * z + z_bottom;
    }
    Mesh::new(verts, tris)
}

/// The Python `make_thin_fin()` default.
pub fn make_thin_fin() -> Result<Mesh, MeshError> {
    make_thin_fin_with([20.0, 20.0, 4.0], [0.6, 10.0, 15.0], 0.0)
}

pub fn make_thin_fin_with(base: [f64; 3], fin: [f64; 3], z0: f64) -> Result<Mesh, MeshError> {
    validate_dimensions("base", base)?;
    validate_dimensions("fin", fin)?;
    validate_finite("z0", z0)?;
    let mut verts = Vec::with_capacity(16);
    let mut tris = Vec::with_capacity(24);
    append_box(&mut verts, &mut tris, base, [0.0, 0.0, z0 + base[2] / 2.0])?;
    append_box(
        &mut verts,
        &mut tris,
        fin,
        [0.0, 0.0, z0 + base[2] + fin[2] / 2.0],
    )?;
    Mesh::new(verts, tris)
}

fn append_box(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
    size: [f64; 3],
    center: [f64; 3],
) -> Result<(), MeshError> {
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    let half = [size[0] / 2.0, size[1] / 2.0, size[2] / 2.0];
    for x in [-half[0], half[0]] {
        for y in [-half[1], half[1]] {
            for z in [-half[2], half[2]] {
                verts.push([center[0] + x, center[1] + y, center[2] + z]);
            }
        }
    }
    tris.extend(BOX_FACES.map(|face| [base + face[0], base + face[1], base + face[2]]));
    Ok(())
}

fn append_down_cone(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
    radius: f64,
    height: f64,
    attachment: [f64; 3],
) -> Result<(), MeshError> {
    const SECTIONS: usize = 32;
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    // `trimesh.creation.cone` is a 32-section revolution, followed by its
    // process=True exact merge.  `unique_ordered` retains the first vertex
    // seen in every angular slice: base centre, ring zero, apex, then the
    // remaining ring.  Preserve that unusual order because fixture indices
    // are source data, not an invitation to canonicalize.
    let rotate_x_pi = |point: [f64; 3]| {
        let radians = core::f64::consts::PI;
        let (sin, cos) = radians.sin_cos();
        [
            point[0] + attachment[0],
            cos * point[1] - sin * point[2] + attachment[1],
            sin * point[1] + cos * point[2] + attachment[2],
        ]
    };
    verts.push(rotate_x_pi([0.0, 0.0, 0.0]));
    let ring = |section: usize| {
        let theta = 2.0 * core::f64::consts::PI * section as f64 / SECTIONS as f64;
        rotate_x_pi([radius * theta.cos(), radius * theta.sin(), 0.0])
    };
    verts.push(ring(0));
    verts.push(rotate_x_pi([0.0, 0.0, height]));
    for section in 1..SECTIONS {
        verts.push(ring(section));
    }
    let apex = base + 2;
    let ring_index = |section: u32| {
        if section == 0 {
            base + 1
        } else {
            base + 2 + section
        }
    };
    for section in 0..SECTIONS as u32 {
        let next = (section + 1) % SECTIONS as u32;
        tris.push([ring_index(section), base, ring_index(next)]);
        tris.push([ring_index(section), ring_index(next), apex]);
    }
    Ok(())
}

fn validate_dimensions(prefix: &'static str, values: [f64; 3]) -> Result<(), MeshError> {
    validate_positive(
        match prefix {
            "leg" => "leg.x",
            "top" => "top.x",
            "blade" => "blade.x",
            "base" => "base.x",
            "fin" => "fin.x",
            _ => "size.x",
        },
        values[0],
    )?;
    validate_positive(
        match prefix {
            "leg" => "leg.y",
            "top" => "top.y",
            "blade" => "blade.y",
            "base" => "base.y",
            "fin" => "fin.y",
            _ => "size.y",
        },
        values[1],
    )?;
    validate_positive(
        match prefix {
            "leg" => "leg.z",
            "top" => "top.z",
            "blade" => "blade.z",
            "base" => "base.z",
            "fin" => "fin.z",
            _ => "size.z",
        },
        values[2],
    )
}

fn validate_positive(parameter: &'static str, value: f64) -> Result<(), MeshError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(MeshError::InvalidPositiveFixtureParameter { parameter, value })
    }
}

fn validate_finite(parameter: &'static str, value: f64) -> Result<(), MeshError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(MeshError::InvalidFiniteFixtureParameter { parameter, value })
    }
}
