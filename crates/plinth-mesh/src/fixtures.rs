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

/// The default box with a 4-face dust shard hanging off one corner vertex.
///
/// One of three deliberately tiny fixtures for the repair pre-pass — they are
/// **not** corpus members and never enter a parity or frozen-digest gate. Each
/// carries exactly one census defect class so a rescue test names which repair
/// did the rescuing.
///
/// This one is the shard class. The tetrahedron meets the vertex `(5, 5, 10)`
/// exactly and lies wholly inside the box's material and bounding box, so it
/// satisfies the six triage criteria (4 faces, 0.1 mm across, 1.6e-4 mm³,
/// watertight, no shared edge, touching the main component's vertex set). It
/// is also, on the way in, a pinch: a shard that kisses the body at a single
/// vertex is precisely how the corpus's dust presents.
pub fn make_dust_shard_box() -> Result<Mesh, MeshError> {
    let mut verts = Vec::with_capacity(12);
    let mut tris = Vec::with_capacity(16);
    append_box(&mut verts, &mut tris, [10.0, 10.0, 10.0], [0.0, 0.0, 5.0])?;
    append_tetra(
        &mut verts,
        &mut tris,
        [
            [5.0, 5.0, 10.0],
            [4.9, 4.99, 9.99],
            [4.99, 4.9, 9.99],
            [4.99, 4.99, 9.9],
        ],
    )?;
    Mesh::new(verts, tris)
}

/// The default box with one of its twelve faces present twice, same winding.
///
/// The duplicate class. Every edge of the repeated face carries three
/// occurrences rather than two, so the mesh has no boundary edge and no
/// same-direction edge — exactly what the corpus census measured — and the
/// gate refuses it on `TOPOLOGY_CUT_DUPLICATE` alone.
pub fn make_duplicated_face_box() -> Result<Mesh, MeshError> {
    let box_mesh = make_box_default()?;
    let mut tris = box_mesh.tris().to_vec();
    tris.push(tris[0]);
    Mesh::new(box_mesh.verts().to_vec(), tris)
}

/// The default box with one face present three times: twice as itself and once
/// reverse-wound.
///
/// The duplicate class again, in the shape that tells the two candidate
/// removal rules apart. A rule that reduces `f` forward and `r` reverse
/// members to `|f - r|` faces deletes all three of these and takes a
/// watertight box to three boundary edges; the surviving member has to be one
/// wound with its neighbours, because the copies are sitting on load-bearing
/// surface rather than floating free.
pub fn make_triply_wound_face_box() -> Result<Mesh, MeshError> {
    let box_mesh = make_box_default()?;
    let mut tris = box_mesh.tris().to_vec();
    let face = tris[0];
    tris.push(face);
    tris.push([face[0], face[2], face[1]]);
    Mesh::new(box_mesh.verts().to_vec(), tris)
}

/// The default box beside a free-floating 8 × 2 mm **double-sided sheet** —
/// four triangles, each also present reverse-wound.
///
/// **The shape a cancellation rule deletes if it is allowed to cascade.** Every
/// one of the sheet's four duplicate groups has duplicate groups for
/// neighbours, so a keep-none disposition (reverted, FU-46) judged only by "do the local edges
/// come out clean" is clean for all four independently and the whole sheet
/// disappears — leaving a residual that looks repaired while 8 mm of the
/// owner's geometry is gone, and turning a model the gate refuses into one it
/// passes. The sheet is 8 mm across, thirty times the triage dust ceiling, so
/// the shard strip does not touch it; only the duplicate action can reach it.
/// A pass that halves the sheet to one-sided and hands the open boundary to the
/// unchanged gate is the correct behaviour, and this fixture is what proves it.
pub fn make_doubled_sheet_box() -> Result<Mesh, MeshError> {
    let box_mesh = make_box_default()?;
    let mut verts = box_mesh.verts().to_vec();
    let mut tris = box_mesh.tris().to_vec();
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    // A 8 x 2 mm strip well clear of the box, split into four triangles.
    for step in 0..3_u32 {
        let x = 20.0 + 4.0 * f64::from(step);
        verts.push([x, 0.0, 0.0]);
        verts.push([x, 2.0, 0.0]);
    }
    for quad in 0..2_u32 {
        let corner = base + 2 * quad;
        for face in [
            [corner, corner + 2, corner + 1],
            [corner + 1, corner + 2, corner + 3],
        ] {
            tris.push(face);
            tris.push([face[0], face[2], face[1]]);
        }
    }
    Mesh::new(verts, tris)
}

/// Two 10 mm boxes sharing exactly one whole edge.
///
/// **Not a census class, and that is the point.** The shared edge carries four
/// face occurrences, so it is non-manifold — a defect the census found zero of
/// and this pass deliberately does not repair. Its two endpoint vertices
/// nevertheless show up as `TOPOLOGY_CUT_POINT_ONLY` pinches, because fan
/// connectivity is computed across *accepted* edges and a non-manifold edge is
/// not one. A pinch split applied here would tear the junction into a boundary
/// crack and record it as an ordinary repair, so the pre-pass has to recognise
/// the shape and refuse it.
pub fn make_edge_shared_boxes() -> Result<Mesh, MeshError> {
    let mut verts = Vec::with_capacity(16);
    let mut tris = Vec::with_capacity(24);
    append_box(&mut verts, &mut tris, [10.0, 10.0, 10.0], [0.0, 0.0, 5.0])?;
    append_box(&mut verts, &mut tris, [10.0, 10.0, 10.0], [10.0, 10.0, 5.0])?;
    Mesh::new(verts, tris)
}

/// Two 10 mm boxes meeting at exactly one shared corner vertex.
///
/// The pinch class, and nothing else: both shells are watertight, both are
/// consistently oriented, both enclose positive volume, and neither is small
/// enough to be dust. The only defect is that six faces meet at
/// `(5, 5, 10)` in two locally disconnected fans, which is what
/// `TOPOLOGY_CUT_POINT_ONLY` names.
pub fn make_pinched_boxes() -> Result<Mesh, MeshError> {
    let mut verts = Vec::with_capacity(16);
    let mut tris = Vec::with_capacity(24);
    append_box(&mut verts, &mut tris, [10.0, 10.0, 10.0], [0.0, 0.0, 5.0])?;
    append_box(
        &mut verts,
        &mut tris,
        [10.0, 10.0, 10.0],
        [10.0, 10.0, 15.0],
    )?;
    Mesh::new(verts, tris)
}

/// Append a closed tetrahedron whose faces all wind outward.
///
/// The winding is derived from the points' own signed volume rather than
/// assumed, so a caller cannot silently produce an inside-out fixture and
/// then blame the topology for noticing.
fn append_tetra(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
    points: [[f64; 3]; 4],
) -> Result<(), MeshError> {
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    let edge = |to: usize| {
        [
            points[to][0] - points[0][0],
            points[to][1] - points[0][1],
            points[to][2] - points[0][2],
        ]
    };
    let (u, v, w) = (edge(1), edge(2), edge(3));
    let determinant = u[0] * (v[1] * w[2] - v[2] * w[1]) - u[1] * (v[0] * w[2] - v[2] * w[0])
        + u[2] * (v[0] * w[1] - v[1] * w[0]);
    if !determinant.is_finite() || determinant == 0.0 {
        return Err(MeshError::InvalidFiniteFixtureParameter {
            parameter: "tetra volume",
            value: determinant,
        });
    }
    let [a, b, c, d] = if determinant > 0.0 {
        [0_u32, 1, 2, 3]
    } else {
        [0_u32, 2, 1, 3]
    };
    verts.extend(points);
    for face in [[a, c, b], [a, b, d], [a, d, c], [b, c, d]] {
        tris.push([base + face[0], base + face[1], base + face[2]]);
    }
    Ok(())
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
