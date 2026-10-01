use geom_kernel::vec3::{cross, dist, dot, norm, sub};

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
    let edge = |to: usize| sub(points[to], points[0]);
    let (u, v, w) = (edge(1), edge(2), edge(3));
    // `geom_kernel::vec3` (docs/specs/geometry-canon.md §1 OP-9, plan item
    // C8a). Byte-neutral: the retired inline scalar triple product spelled its
    // middle term `-u[1] * (v[0]*w[2] - v[2]*w[0])`, which is the exact IEEE
    // negation of the kernel's `u[1] * (v[2]*w[0] - v[0]*w[2])`, and negation
    // is exact — so the sum associates identically and the sign test below
    // cannot move.
    let determinant = dot(u, cross(v, w));
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

/// **The coverage-repair witness: an overhang table whose own baseline layout
/// leaves a strip of its underside outside the registered coverage radius.**
///
/// Same family and same generator as [`make_overhang_table`] — four legs, one
/// top, no new geometry code — at 25 x 25 x 4 mm on 2 x 2 x 20 mm legs instead
/// of 30 x 30 x 4 mm on 4 x 4 x 20 mm legs. It exists for one measured
/// property, and the property is what the dimensions were chosen to produce:
///
/// * **the hole is measured, not asserted.** Attempt 0 leaves **0.553 mm²** of
///   the underside uncovered on island 8, component 4, with a worst gap of
///   **6.901 mm** to the nearest contact ROUTED to that same route — against
///   the registered coverage radius
///   `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm. That area is 3.4x
///   the registered A3 censor `VOXEL_PITCH_MM²` = 0.16 mm², which is what
///   makes the patch BLOCK the verdict rather than be forgiven as A3
///   uncertainty.
/// * **why a hole can exist here at all.** Baseline seat selection is an
///   ordered Poisson pass at `TARGET_SPACING_MM` (`plinth-support`'s
///   `select_island_contacts`), so no candidate is ever more than 4 mm from a
///   selected seat and the selection alone cannot leave a 6 mm gap. Coverage
///   counts only contacts that ROUTED, and this run's own §6 removal ledger
///   names one regular contact on the hole's own route — candidate row 959,
///   island 8, component 4, carrying 312.5 mm² before the removal — retired
///   `NoWinner` because the routing cadence found it no arm. A seat that is
///   chosen and then cannot be built is how a Poisson-covered island still
///   measures uncovered.
/// * the hole is closable from this model's own repair pool, so the run does
///   not merely refuse: the controller spends two trials and commits one
///   contact at the +X rim, and the sealing step measures zero failing
///   patches.
///
/// **Nothing is loosened to get here.** The radius, the censor and the
/// zero-area tolerance are the registered production values; the fixture
/// changes the geometry, not the rule that judges it.
///
/// Its predecessor in this role was [`make_overhang_table`] itself, until
/// `3a6526f` converged the clearance mask and seated the model anchor. That
/// fix let the model-seated anchor family route for the first time, the 30 mm
/// table's baseline layout began covering every island, and every test that
/// reads a COMMITTED repair step lost its witness. This fixture is that
/// witness, rebuilt against the fixed code rather than against the bug.
pub fn make_uncovered_rim_table() -> Result<Mesh, MeshError> {
    make_overhang_table_with([2.0, 2.0, 20.0], [25.0, 25.0, 4.0], 0.0)
}

/// The toed slab: a 30 x 30 x 4 mm slab with its underside at z = 15, standing
/// on two downward cone toes (radius 3 mm, 4 mm long at x = -8; radius
/// 1.5 mm, 6 mm long at x = +8), as ONE closed solid.
///
/// The Python `make_toed_slab()` this was ported from concatenated three
/// closed shells (a box and two cones) without a union, so each toe kept its
/// base cap inside the slab: 64 triangles, 35.12 mm2, facing up into
/// material. Every face-area term read them as surface; turned over, the
/// orientation fold charged them as down-facing area, and that decided the
/// X 18 pose by 0.015 % (field-2026-09-28 `creep-arms-diagnosis.md` Q2). The
/// union below has no buried face: the slab's underside is pierced by the two
/// toe footprints and each toe's flank meets the slab on that footprint's
/// ring.
pub fn make_toed_slab() -> Result<Mesh, MeshError> {
    make_toed_slab_with([30.0, 30.0, 4.0], 15.0)
}

/// [`make_toed_slab`] at another slab size and height, the toes unchanged.
///
/// Every vertex is exactly representable in f32, so the mesh survives the
/// binary-STL wire bit for bit and the CLI reads the very solid this builds.
///
/// # Errors
///
/// A non-positive or non-finite slab dimension or a non-finite `z_bottom`;
/// [`MeshError::InvalidRangeFixtureParameter`] when the slab is too small for
/// each toe's footprint to sit wholly inside its own half of the underside
/// (the halves are cut along the diagonal from the (-x, -y) corner to the
/// (+x, +y) corner).
#[expect(
    clippy::cast_possible_truncation,
    reason = "two deliberate narrowings: f64 -> f32 -> f64 rounds each ring coordinate onto the STL \
          wire's grid (the point of it), and a ring index below SECTIONS = 32 fits a u32"
)]
pub fn make_toed_slab_with(top: [f64; 3], z_bottom: f64) -> Result<Mesh, MeshError> {
    // (centre x, radius, length) of each toe; y = 0, attached at z_bottom.
    const TOES: [(f64, f64, f64); 2] = [(-8.0, 3.0, 4.0), (8.0, 1.5, 6.0)];
    const SECTIONS: usize = 32;
    validate_dimensions("top", top)?;
    validate_finite("z_bottom", z_bottom)?;
    let f32_exact = |value: f64| f64::from(value as f32);
    let (hx, hy) = (top[0] / 2.0, top[1] / 2.0);
    if [hx, hy, z_bottom, z_bottom + top[2]]
        .iter()
        .any(|value| f32_exact(*value).to_bits() != value.to_bits())
    {
        return Err(MeshError::InvalidFiniteFixtureParameter {
            parameter: "top/z_bottom (must be exactly representable in f32)",
            value: top[0],
        });
    }
    // The two halves of the underside, as their corners: the toe at -x lies
    // above the diagonal y = (hy / hx) x, the toe at +x below it.
    let corners = [[-hx, -hy], [hx, hy], [-hx, hy], [hx, -hy]];
    let halves: [[usize; 3]; 2] = [[0, 1, 2], [0, 3, 1]];
    let diagonal_length = norm([hx, hy, 0.0]);
    for &(centre_x, radius, _) in &TOES {
        // Distance of the toe centre (centre_x, 0) from the diagonal, and
        // from the slab's edges: the footprint must clear all of them.
        let clearance = (hy * centre_x).abs() / diagonal_length;
        if clearance <= radius || hx - centre_x.abs() <= radius || hy <= radius {
            return Err(MeshError::InvalidRangeFixtureParameter {
                parameter: "top.x/top.y (each toe footprint inside its own half)",
                value: top[0].min(top[1]),
                min: 2.0 * (centre_x.abs() + radius),
                max: f64::INFINITY,
            });
        }
    }

    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(8 + TOES.len() * (SECTIONS + 1));
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(10 + TOES.len() * (3 * SECTIONS + 3));
    let index = |count: usize| u32::try_from(count).map_err(|_| MeshError::FixtureIndexOverflow);

    // The slab: the box's own ten faces that are not its underside, in the
    // box's face order (so the first source face stays [1, 3, 0]).
    append_box(
        &mut verts,
        &mut tris,
        top,
        [0.0, 0.0, z_bottom + top[2] / 2.0],
    )?;
    // Box vertex k sits at (x, y, z) = (bit 2, bit 1, bit 0) of k: the
    // underside is {0, 2, 4, 6}; drop the two faces made only of those.
    tris.retain(|face| face.iter().any(|vertex| vertex % 2 == 1));
    let corner_vertex = [0_u32, 6, 2, 4]; // (-,-), (+,+), (-,+), (+,-)

    for (toe, &(centre_x, radius, length)) in TOES.iter().enumerate() {
        let ring_start = index(verts.len())?;
        for section in 0..SECTIONS {
            let theta = 2.0 * core::f64::consts::PI * section as f64 / SECTIONS as f64;
            verts.push([
                f32_exact(centre_x + radius * theta.cos()),
                f32_exact(radius * theta.sin()),
                z_bottom,
            ]);
        }
        let apex = index(verts.len())?;
        verts.push([centre_x, 0.0, f32_exact(z_bottom - length)]);
        let ring = |section: usize| ring_start + (section % SECTIONS) as u32;

        // The toe's flank, outward: the ring runs counter-clockwise seen from
        // +z, so (k+1, k, apex) faces away from the axis and downward.
        for section in 0..SECTIONS {
            tris.push([ring(section + 1), ring(section), apex]);
        }

        // This toe's half of the underside: the band between the half's three
        // corners (a convex triangle) and the toe's ring (a convex polygon
        // inside it), triangulated without new vertices. Walk both
        // counter-clockwise; each corner takes, in order, every ring edge it
        // SEES (lies strictly outside of), then hands over to the next
        // corner. Consecutive corners' visible arcs of the ring overlap on
        // these dimensions, so the hand-over vertex is seen by both and no
        // triangle crosses the hole. (A plain angular zip does not work: the
        // far (+x, +y) corner owns a 148-degree arc of the -x toe's ring and
        // folded a triangle over the hole.) The fixture test checks the
        // result: every underside face down-facing, their areas summing to
        // the slab minus the two footprints.
        let centre = [centre_x, 0.0];
        let angle = |point: [f64; 2]| (point[1] - centre[1]).atan2(point[0] - centre[0]);
        let mut outer: Vec<(f64, u32, [f64; 2])> = halves[toe]
            .iter()
            .map(|&corner| {
                (
                    angle(corners[corner]),
                    corner_vertex[corner],
                    corners[corner],
                )
            })
            .collect();
        outer.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Every point here lies in the plane z = z_bottom, so work in it.
        let in_plane = |point: [f64; 2]| [point[0], point[1], z_bottom];
        let ring_point = |section: usize| verts[ring(section) as usize];
        // Strictly right of the directed ring edge (k -> k+1): outside it.
        let sees = |point: [f64; 2], section: usize| {
            let a = ring_point(section);
            let turn = cross(sub(ring_point(section + 1), a), sub(in_plane(point), a));
            turn[2] < 0.0
        };
        // Start on the ring vertex nearest the first corner, the middle of
        // the arc it sees.
        let first_corner = in_plane(outer[0].2);
        let first = (0..SECTIONS)
            .min_by(|a, b| {
                dist(ring_point(*a), first_corner).total_cmp(&dist(ring_point(*b), first_corner))
            })
            .expect("a ring has sections");
        let (mut p, mut q) = (0_usize, 0_usize);
        while p < outer.len() || q < SECTIONS {
            let corner = outer[p % outer.len()].1;
            let corner_xy = outer[p % outer.len()].2;
            if q < SECTIONS && (p == outer.len() || sees(corner_xy, first + q)) {
                // Down-facing: clockwise seen from +z. The corner lies outside
                // the counter-clockwise ring edge (k, k+1), on its right, so
                // (corner, k, k+1) is clockwise.
                tris.push([corner, ring(first + q), ring(first + q + 1)]);
                q += 1;
            } else {
                // The ring vertex lies inside the counter-clockwise corner
                // edge (corner, next), on its left, so (corner, k, next) is
                // clockwise.
                let next = outer[(p + 1) % outer.len()].1;
                tris.push([corner, ring(first + q), next]);
                p += 1;
            }
        }
    }
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

// ---------------------------------------------------------------------------
// Dual-cavity pawn: the E1 hollowing fixture.
// ---------------------------------------------------------------------------

/// Radius, in mm, of the silhouette sphere whose flat-truncated base forms
/// the lower chamber and the model's foot. A whole number of 0.5 mm voxel
/// pitches, so `grid_spec_from_bounds` anchors a lattice sample exactly on
/// the model's axis (see the generator's lattice-phase section).
pub const DUAL_CAVITY_LOWER_SPHERE_R_MM: f64 = 12.5;
/// Height, in mm, of the lower silhouette sphere's centre above the foot
/// plane (`z = 0`).
pub const DUAL_CAVITY_LOWER_SPHERE_Z_MM: f64 = 9.5;
/// Radius, in mm, of the silhouette sphere that forms the upper chamber.
pub const DUAL_CAVITY_UPPER_SPHERE_R_MM: f64 = 8.0;
/// Distance, in mm, between the two silhouette spheres' centres. Exactly
/// `LOWER_SPHERE_R + UPPER_SPHERE_R`: the two silhouette spheres are
/// externally tangent, which is what makes the throat's cylindrical band
/// have strictly positive height at *every* admitted throat radius (the
/// band's height is `SPACING - sqrt(R1^2 - rt^2) - sqrt(R2^2 - rt^2)`, zero
/// only in the `rt -> 0` limit and increasing in `rt`), so the lathe profile
/// is single-valued in `z` across the whole parameter domain rather than
/// only at the three registered settings.
pub const DUAL_CAVITY_CENTRE_SPACING_MM: f64 =
    DUAL_CAVITY_LOWER_SPHERE_R_MM + DUAL_CAVITY_UPPER_SPHERE_R_MM;
/// Vertices per ring of the lathe. One value for the whole profile: the
/// throat radius only ever moves *where* two rings sit, never how many
/// vertices a ring carries or how many rings exist.
pub const DUAL_CAVITY_SLICES: usize = 64;
/// Equal-angle steps carrying the lower arc from the foot rim to the throat's
/// lower junction (so `LOWER_ARC_STEPS + 1` lower-arc rings).
pub const DUAL_CAVITY_LOWER_ARC_STEPS: usize = 48;
/// Equal-angle steps carrying the upper arc from the throat's upper junction
/// to the apex.
pub const DUAL_CAVITY_UPPER_ARC_STEPS: usize = 60;

/// The cavity-side (post-erosion) throat radius, in mm, for the SEPARATED
/// control's physical setting is not expressible as a cavity radius at all —
/// see [`DUAL_CAVITY_THROAT_SEPARATED_MM`] instead, which states the
/// physical passage directly.
///
/// AT_BORE: the cavity-side throat radius that pins the throat's axis
/// clearance to exactly `0.5*sqrt(8)` = 1.4142135623730951 mm — the widest
/// passage the FU-47 core threshold still refuses — and that a comparison
/// against the raw bore radius alone would wrongly admit.
///
/// **Re-derived 2026-08-13 from 1.45 mm, when the registered drill went from
/// 3.0 mm across to 2.5 mm.** The engine did not change and was not meant to:
/// the flow rule's minimum passage width is defined by what the chosen drill
/// can pass through, so it moves with the drill by design. The fixture is what
/// recalibrates.
///
/// **The line, and which lattice rung it lands on.** `HOLLOW_DRAIN_R_MM` is
/// now 1.25 mm and `flow::core_clearance_threshold_mm` adds half a voxel
/// pitch, so the clearance line is `1.25 + 0.25` = **1.5 mm**, where it used
/// to be 1.75 mm. On this fixture's own lattice phase a cavity-side throat
/// radius `r` reads a clearance of `0.5*sqrt(k)` mm, `k` the smallest sum of
/// two squares with `0.5*sqrt(k) > r`. The reachable rungs around the line
/// are `k = 5` (1.1180), `k = 8` (1.4142), `k = 9` (1.5) and `k = 10`
/// (1.5811); `k = 6` and `k = 7` are not sums of two squares, so 1.2247 and
/// 1.3229 are not readings any lattice offset can produce.
///
/// The new line lands **exactly on rung `k = 9`** — the offset `(3, 0)`,
/// three whole pitches. The retired 1.75 mm line landed between rungs 9 and
/// 13 and no offset could equal it, which is why the old fixture could sit on
/// rung 9 and still be refused. That is no longer available: the core test is
/// `clearance >= threshold`, so a throat that reads 1.5 mm is now **admitted**
/// and the refused side of the line begins one rung lower, at
/// `0.5*sqrt(8)` = 1.4142135623730951 mm.
///
/// **Why rung 8 and not a lower one.** Rung 8 is the only choice that keeps
/// this setting's whole claim alive. B4 asserts that a comparison against the
/// *raw* bore radius would wrongly admit AT_BORE and only the half-voxel
/// correction refuses it: 1.4142 >= 1.25, so the raw comparison does admit it.
/// Rung 5 (1.1180) is under the raw bore and would be refused with or without
/// the correction, which would prove nothing.
///
/// **Why 1.25 and not another radius on that rung.** Rung 8 is read by any
/// cavity radius in `(0.5*sqrt(5), 0.5*sqrt(8)]` =
/// `(1.1180339887498949, 1.4142135623730951]`. 1.25 sits 0.1320 mm above the
/// bottom of that bracket and 0.1642 mm below the top — near the middle,
/// farthest from either mistake, and about 3.7x the margin the retired 1.45 mm
/// had at the tight end of its own bracket (0.0358 mm). It is also exactly
/// representable in binary (`5/4`), as is the physical throat it produces
/// (`t + 1.25` = 3.75 mm = `30/8`), so the mandatory f32 snap and every
/// platform's `sin`/`cos` decide the rung identically. That it also equals
/// today's `HOLLOW_DRAIN_R_MM` makes the setting's name literal — the cavity
/// throat is exactly one bore radius wide — but the binding requirement is the
/// bracket, not that coincidence.
///
/// **Arbitrary lattice phase.** Decimation and the dense orientation sweep put
/// the throat on an arbitrary phase, where the worst-case in-plane distance
/// from a line to the nearest 0.5 mm lattice sample is `0.5/sqrt(2)` ~
/// 0.354 mm. That error costs a reading clearance; it does not manufacture it,
/// so for a setting whose job is to be **refused** it works in the safe
/// direction — the opposite of
/// [`DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM`], which had to be pushed up the
/// bracket to survive the same error. The claim is not left to that argument
/// either. It is measured twice, both times off this fixture's own phase:
/// `plinth-hollow`'s
/// `dual_cavity_doctrine::the_flow_rule_still_splits_at_bore_and_joins_above_bore_off_the_lattice`
/// shifts the fixture a quarter pitch and requires the flow rule to still
/// report **two** resin-flow interiors here against one at ABOVE_BORE; and
/// `plinth-pipeline/tests/dual_cavity_real_pipeline.rs` runs the real
/// `repair -> decimate -> dense orientation sweep` chain and requires the far
/// chamber to be handed back to solid, within 5% of its own analytic volume.
/// On the quarter-pitch pose that second claim does **not** hold today — the
/// flow rule splits the chambers and the drilling stage's reachability flood
/// then crosses the same sub-bore throat — and that gap is recorded as an open
/// finding in that test rather than hidden behind a weaker assertion. It is a
/// drilling-stage question; it does not move this number.
pub const DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM: f64 = 1.25;
/// ABOVE_BORE: the cavity-side throat radius that pins the throat's axis
/// clearance to exactly 2.5 mm — five 0.5 mm lattice steps, comfortably
/// admitted by the FU-47 core threshold (1.5 mm since the 2026-08-13 drill
/// change; 1.75 mm when this value was derived).
///
/// **Why 2.45 and not the smallest admitted value.** The clearance the flow
/// rule reads is quantized onto the lattice, so *any* cavity radius in
/// `(0.5*sqrt(20), 0.5*sqrt(25)) = (2.2360, 2.5)` reads exactly 2.5 mm on
/// the fixture's own lattice phase. The value is placed near the top of that
/// bracket because the fixture is also run through the **real** production
/// chain (repair -> decimate -> dense orientation sweep -> hollow), which
/// lands the throat on an arbitrary lattice phase: the worst-case in-plane
/// distance from an arbitrary line to the nearest 0.5 mm lattice sample is
/// `0.5/sqrt(2) ~ 0.354 mm`, so an off-phase throat still reads at least
/// `2.45 - 0.354 = 2.096 mm`, clear of the 1.5 mm line today and of the
/// 1.75 mm line it was sized against. At the pre-2026-08-12 value of 1.90 mm
/// that margin was `1.90 - 0.354 = 1.546 mm < 1.75 mm`, and the doctrine
/// measurably failed after decimation and re-orientation — which is the
/// defect this number's current value fixes. That failure was measured
/// against the retired 1.75 mm line and is left standing as the record of
/// **why** the value is 2.45; it has not been re-measured against the 1.5 mm
/// line, and 2.45 is retained on its own merits rather than re-derived down.
pub const DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM: f64 = 2.45;
/// SEPARATED: a physical throat radius under the registered erosion `t`, so
/// the throat holds no cavity voxel at all and the two chambers are not even
/// 6-connected. The control that proves the split is real.
///
/// **Re-derived 2026-08-13** from 3.0 mm, when the owner ruling zeroed
/// `HOLLOW_CAVITY_MARCH_MARGIN_MM` and the erosion `t` fell from 3.366 mm to
/// the requested wall itself, 2.5 mm. At 3.0 mm this throat would sit ABOVE
/// the new erosion, grow a real cavity throat, and turn the control silently
/// into a different experiment -- which is exactly what `plinth-hollow`'s
/// B7 guard is there to catch, and did. 2.0 mm restores the control with
/// MORE margin than it had: 0.5 mm under the erosion where it previously had
/// 0.366 mm, comfortably clear of the 0.354 mm worst-case lattice phase
/// error an off-phase run can introduce.
pub const DUAL_CAVITY_THROAT_SEPARATED_MM: f64 = 2.0;
/// ABOVE_BORE: the fixture's frozen default physical throat radius,
/// `HOLLOW_WALL_DEFAULT_MM + HOLLOW_CAVITY_MARCH_MARGIN_MM + 2.45`
/// = `2.5 + 0.0 + 2.45`. A hollow-side test asserts this literal still
/// equals that live-registry sum, so a wall or margin change fails a named
/// test instead of silently invalidating this fixture.
///
/// **Re-derived 2026-08-13** from 5.816025403784439 mm, when the owner
/// ruling zeroed `HOLLOW_CAVITY_MARCH_MARGIN_MM`. The formula did not
/// change; the margin term in it went to zero.
pub const DUAL_CAVITY_THROAT_ABOVE_BORE_MM: f64 = 4.95;
/// The smallest admitted `throat_radius_mm`.
///
/// An **added** admission bound (FU-8b permits adding one; it never permits
/// loosening one), and the bound that makes the generator's own
/// non-degeneracy claim true across its whole public domain rather than only
/// at the three registered settings. Adjacent vertices on a lathe ring of
/// radius `r` are `2 r sin(pi / DUAL_CAVITY_SLICES)` apart; at `r = 0.1` that
/// is `9.8e-3 mm`, four orders of magnitude above the f32 step at this
/// fixture's coordinate magnitudes (`2^-23 * 38 = 4.5e-6 mm`), so the
/// mandatory f32 snap provably cannot weld a ring into a point or emit a
/// zero-area face. Below this bound it eventually can, and
/// [`crate::Mesh::new`] does not reject zero-area faces.
pub const DUAL_CAVITY_MIN_THROAT_R_MM: f64 = 0.1;

/// The E1 dual-cavity hollowing fixture, at [`DUAL_CAVITY_THROAT_ABOVE_BORE_MM`].
///
/// See [`make_dual_cavity_pawn_with_throat_radius`] for the full geometry and
/// rationale. This default is sized so **both** doctrine settings the crate
/// ships hollow (ABOVE_BORE with one flow-connected interior, AT_BORE with
/// the far chamber left solid) — it is deliberately not a
/// `Solid{BelowThreshold}` witness.
pub fn make_dual_cavity_pawn() -> Result<Mesh, MeshError> {
    make_dual_cavity_pawn_with_throat_radius(DUAL_CAVITY_THROAT_ABOVE_BORE_MM)
}

/// A coaxial surface of revolution: two silhouette spheres — a 12.5 mm lower
/// sphere flat-truncated at the foot plane `z = 0`, and an 8 mm upper sphere
/// 20.5 mm above it — joined by a right-circular-cylinder throat of the given
/// physical radius. Built as one ordered lathe profile, so it is manifold and
/// watertight **by construction**: no boolean kernel, no repair pass, pure f64
/// trigonometry snapped to the STL f32 wire. The article is 25 mm across and
/// 38 mm tall.
///
/// **Why a lathe and not a union.** The shape is axially symmetric about
/// `x = y = 0`, so its silhouette is an exact 2-D profile and needs no
/// boolean at all — unlike `plinth-hollow`'s test-only pinched dumbbell,
/// which needs `ManifoldBackend::union_limited` and is therefore stranded
/// behind that crate's Manifold dependency. This fixture has zero new
/// dependencies, byte-stable output, and no dependence on Manifold's
/// floating-point behaviour.
///
/// **This fixture is an argument about one lattice phase.** The axis sits at
/// `x = y = 0` and the silhouette's half-width is
/// [`DUAL_CAVITY_LOWER_SPHERE_R_MM`] = 12.5 mm — a whole number of 0.5 mm
/// pitches — so H0's `grid_spec_from_bounds`, which anchors its origin at
/// `round_ties_even(low[axis]/pitch)` times the pitch, lands its origin at
/// exactly `-12.5` and therefore lands a voxel sample exactly on the axis.
/// The throat's cross-section falls on lattice offsets `(0.5i, 0.5j)` with no
/// phase error, which is what makes the throat clearance thresholds in
/// `plinth-hollow`'s `dual_cavity_doctrine` tests **exact** rather than
/// approximate.
///
/// **The exact clearances belong to that phase; the doctrine does not.**
/// Translating the fixture off the 0.5 mm lattice, decimating it, or running
/// it through an orientation sweep all move it to an arbitrary phase, and the
/// *exact* equalities (`clearance == 0.5*sqrt(8)` at AT_BORE, `clearance ==
/// 2.5` at ABOVE_BORE) are only
/// claimed on the fixture's own phase. The *doctrine* — AT_BORE refused,
/// ABOVE_BORE admitted as one flow-connected interior spanning both chambers
/// — is claimed on **both**, and
/// `plinth-pipeline/tests/dual_cavity_real_pipeline.rs` asserts it after the
/// real `repair -> decimate -> dense orientation sweep` chain. That is what
/// sizes [`DUAL_CAVITY_CAVITY_THROAT_ABOVE_BORE_MM`] at 2.45 mm rather than
/// at the smallest value that reads as admitted on this phase: see that
/// constant's own worst-case off-phase derivation.
///
/// **The owner's explainer numbers, and where they landed.** The approved
/// explainer described a ~35 mm pawn with a ~12 mm lower chamber, a ~10 mm
/// upper chamber, and a ~1 mm throat. The silhouette sizes here are the
/// smallest that clear the registered savings gate:
/// - The gate is the registered absolute-volume minimum
///   (`HOLLOW_SAVE_MIN_ML`, 1.5 mL / 1,500 mm3), judged, since the
///   2026-08-13 owner ruling, against **every retained chamber's cavity
///   together** — selection keeps every flow-connected interior the flood
///   reaches, not the largest alone (the FU-47 finding this fixture was
///   built to probe). With erosion
///   `t = HOLLOW_WALL_DEFAULT_MM + HOLLOW_CAVITY_MARCH_MARGIN_MM = 2.5 mm`
///   (the margin term is zero, by the same 2026-08-13 ruling), the 12.5 mm
///   lower sphere keeps a 10.0 mm cavity radius and the two chambers
///   together clear the registered minimum with margin at every throat
///   setting; `dual_cavity_pawn_analytic_volume_mm3` states the arithmetic
///   in code and `plinth-hollow`'s `dual_cavity_doctrine` measures the
///   realized figures (4,678.125 mm3 of cavity on the default ABOVE_BORE
///   setting, against the 1,500 mm3 minimum).
/// - That puts the model at 38 mm tall and 25 mm wide — the explainer's
///   ~35 mm — with cavity chambers 20.0 mm and 11.0 mm across (the
///   explainer's "~12 mm" and "~10 mm", after the 2.5 mm erosion the wall
///   costs; the march margin costs nothing now).
/// - A cavity-level throat cannot exist at all below the eroded radius `t =
///   2.5 mm`, so the explainer's "~1 mm throat" cannot become a *cavity*
///   throat at any parameter value — it survives in spirit as
///   [`DUAL_CAVITY_THROAT_SEPARATED_MM`], the control where the two
///   interiors are genuinely separate cavities.
/// - The owner's "round base" is the flat foot the truncated lower sphere
///   already produces, rather than a separate base disc: a disc adds dead
///   volume with no cavity of its own (a base's own EDT never exceeds its own
///   half-thickness), which only erodes the savings ratio for no benefit.
/// - The "3 mm drain bore that exits the base" is deliberately **not**
///   modelled here — the fixture is a closed solid; `plinth-hollow`'s own
///   `drains`/`drilling` stages drill the registered
///   `HOLLOW_DRAIN_R_MM` bore downstream, and pre-drilling it here would
///   both break watertightness and steal the thing this fixture exists to
///   let those stages exercise.
///
/// # Profile construction
///
/// One ordered lathe profile of 109 rings (independent of `throat_radius_mm`
/// — the throat only moves where two rings sit) times [`DUAL_CAVITY_SLICES`]
/// vertices per ring, plus a foot-centre vertex and an apex vertex:
/// 1. `foot_centre = (0, 0, 0)`.
/// 2. The lower arc: [`DUAL_CAVITY_LOWER_ARC_STEPS`] equal steps in the polar
///    angle from the sphere's own top, from the foot rim down to the
///    throat's lower junction — 49 rings, the first the foot rim, the last
///    exactly `(throat_radius_mm, zA)`.
/// 3. The throat's upper-junction ring, `(throat_radius_mm, zB)` — one ring;
///    the band immediately below it is the throat's cylindrical wall.
/// 4. The upper arc: [`DUAL_CAVITY_UPPER_ARC_STEPS`] equal steps from the
///    throat's upper junction to the apex, with the first point (which
///    duplicates the throat ring) and the last (the degenerate `r = 0` apex
///    point) both dropped — 59 rings.
/// 5. `apex = (0, 0, upper centre z + upper radius)`.
///
/// Every vertex is a full lathe circle: `(r cos theta_s, r sin theta_s, z)`
/// for `theta_s = 2 pi s / DUAL_CAVITY_SLICES`. Ring 0's slice 0 therefore
/// sits at the equator `(+r, 0, z)`, so `y = 0` is hit exactly on the +x
/// axis. The extreme `|x|` and `|y|` are the *inscribed* polygon's, a
/// derived hair under [`DUAL_CAVITY_LOWER_SPHERE_R_MM`] rather than bit-exact
/// at it: no lower-arc ring lands exactly on the sphere's equator, and
/// `dual_cavity_pawn_has_frozen_counts_and_bounds` computes that deficit from
/// the ring angles instead of tolerating it.
///
/// # Mandatory f32 snap
///
/// Every coordinate is stored as `(value as f32) as f64` before it enters
/// the mesh. Two reasons, both load-bearing: [`crate::write_binary_stl`]
/// refuses any coordinate that round-trips lossily through f32
/// (`MeshIoError::LossyF32Encoding`), so without the snap this fixture could
/// not be written to STL at all; and it collapses cross-platform `sin`/`cos`
/// libm differences below the STL wire resolution, hardening determinism.
/// The mesh's minimum vertex separation (the ring just below the apex) is
/// orders of magnitude above the f32 step at these magnitudes, so the snap
/// cannot weld two vertices or create a zero-area face —
/// `dual_cavity_pawn_has_no_degenerate_face_after_the_f32_snap` proves that
/// with [`Mesh::audit_strict`] rather than arguing it in prose, and
/// [`DUAL_CAVITY_MIN_THROAT_R_MM`] is the bound that keeps the claim true
/// across the whole admitted parameter domain rather than only at the three
/// registered settings.
///
/// # Errors
///
/// [`MeshError::InvalidPositiveFixtureParameter`] for a non-finite or
/// non-positive `throat_radius_mm`, and
/// [`MeshError::InvalidRangeFixtureParameter`] outside the open interval
/// ([`DUAL_CAVITY_MIN_THROAT_R_MM`], [`DUAL_CAVITY_UPPER_SPHERE_R_MM`]) —
/// at or above the upper bound the throat's upper junction cannot exist on
/// the upper sphere at all, and below the lower bound the f32 snap can weld a
/// throat ring into a point. Both are new admitted domains, not loosened
/// ones — FU-8b permits adding a bound and forbids only loosening or
/// bypassing an existing one.
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub fn make_dual_cavity_pawn_with_throat_radius(throat_radius_mm: f64) -> Result<Mesh, MeshError> {
    validate_positive("throat_radius_mm", throat_radius_mm)?;
    if !(DUAL_CAVITY_MIN_THROAT_R_MM..DUAL_CAVITY_UPPER_SPHERE_R_MM).contains(&throat_radius_mm) {
        return Err(MeshError::InvalidRangeFixtureParameter {
            parameter: "throat_radius_mm",
            value: throat_radius_mm,
            min: DUAL_CAVITY_MIN_THROAT_R_MM,
            max: DUAL_CAVITY_UPPER_SPHERE_R_MM,
        });
    }

    let r1 = DUAL_CAVITY_LOWER_SPHERE_R_MM;
    let z1 = DUAL_CAVITY_LOWER_SPHERE_Z_MM;
    let r2 = DUAL_CAVITY_UPPER_SPHERE_R_MM;
    let z2 = z1 + DUAL_CAVITY_CENTRE_SPACING_MM;
    let rt = throat_radius_mm;
    let slices = DUAL_CAVITY_SLICES;
    let lower_steps = DUAL_CAVITY_LOWER_ARC_STEPS;
    let upper_steps = DUAL_CAVITY_UPPER_ARC_STEPS;

    // Junction angles, measured from each sphere's own +z pole.
    let theta_foot = (-z1 / r1).acos();
    let (z_a, z_b) = dual_cavity_junction_heights_mm(rt);
    let theta_a = ((z_a - z1) / r1).acos();
    let theta_b = ((z_b - z2) / r2).acos();

    let lower_rings = lower_steps + 1; // 49: k = 0..=lower_steps
    let upper_rings = upper_steps - 1; // 59: j = 1..upper_steps
    let ring_count = lower_rings + 1 + upper_rings; // + the throat's upper-junction ring

    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(2 + ring_count * slices);
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(2 * slices + 2 * slices * (ring_count - 1));

    verts.push(snap_vertex([0.0, 0.0, 0.0])); // index 0: foot_centre

    // Lower arc: theta sweeps DOWN from theta_foot (foot rim) to theta_a
    // (the throat's lower junction). The two endpoints are forced to their
    // exact closed-form (r, z) rather than round-tripped through
    // acos/sin/cos, which cannot invert bit-exactly: ring 0 must sit exactly
    // on the foot plane `z = 0` (frozen by `dual_cavity_pawn_has_frozen_
    // counts_and_bounds`), and the last ring must sit exactly at the
    // throat's own radius (load-bearing for the exact-clearance hollow-side
    // doctrine tests).
    let foot_r = (r1 * r1 - z1 * z1).sqrt();
    for k in 0..lower_rings {
        let (r, z) = if k == 0 {
            (foot_r, 0.0)
        } else if k == lower_rings - 1 {
            (rt, z_a)
        } else {
            let t = k as f64 / lower_steps as f64;
            let theta = theta_foot + (theta_a - theta_foot) * t;
            (r1 * theta.sin(), z1 + r1 * theta.cos())
        };
        push_lathe_ring(&mut verts, r, z, slices);
    }

    // Throat upper-junction ring, exact by the same closed form as the lower
    // junction above.
    push_lathe_ring(&mut verts, rt, z_b, slices);

    // Upper arc: theta sweeps from theta_b (excluded: duplicates the throat
    // ring) toward 0 (excluded: the degenerate r = 0 apex point).
    for j in 1..upper_steps {
        let t = j as f64 / upper_steps as f64;
        let theta = theta_b * (1.0 - t);
        push_lathe_ring(&mut verts, r2 * theta.sin(), z2 + r2 * theta.cos(), slices);
    }

    let apex_index = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    verts.push(snap_vertex([0.0, 0.0, z2 + r2]));

    // Every ring index this builder uses is `< ring_count`, and `apex_index`
    // above already proved `1 + ring_count * slices` fits a `u32` -- so this
    // conversion cannot fail. It is still written as a checked conversion
    // rather than a saturating one: fabricating `u32::MAX` on overflow would
    // silently emit out-of-range indices from the one function whose entire
    // claim is that its output topology is trustworthy without a repair pass.
    let ring_base = |ring_idx: usize| -> Result<u32, MeshError> {
        u32::try_from(ring_idx * slices)
            .ok()
            .and_then(|offset| offset.checked_add(1))
            .ok_or(MeshError::FixtureIndexOverflow)
    };

    // Bottom fan: foot_centre to the foot-rim ring (ring 0), outward normal
    // points down (-z), which (slice angle increasing counter-clockwise
    // viewed from +z) needs `s + 1` before `s`.
    let foot_base = ring_base(0)?;
    for s in 0..slices {
        let next = (s + 1) % slices;
        tris.push([0, foot_base + next as u32, foot_base + s as u32]);
    }

    // Bands between every consecutive pair of rings, including the
    // lower-arc/throat and throat/upper-arc seams — they are ordinary bands,
    // the throat radius only changes where a ring's own (r, z) sits.
    for k in 0..ring_count - 1 {
        let base_k = ring_base(k)?;
        let base_k1 = ring_base(k + 1)?;
        for s in 0..slices {
            let next = (s + 1) % slices;
            let (s, next) = (s as u32, next as u32);
            tris.push([base_k + s, base_k1 + next, base_k1 + s]);
            tris.push([base_k + s, base_k + next, base_k1 + next]);
        }
    }

    // Apex fan: the last upper-arc ring to the apex, outward normal points
    // up (+z), which needs `s` before `s + 1`.
    let last_base = ring_base(ring_count - 1)?;
    for s in 0..slices {
        let next = (s + 1) % slices;
        tris.push([apex_index, last_base + s as u32, last_base + next as u32]);
    }

    let mesh = Mesh::new(verts, tris)?;
    let signed_volume = mesh_signed_volume(&mesh);
    if !signed_volume.is_finite() || signed_volume <= 0.0 {
        return Err(MeshError::InvalidFiniteFixtureParameter {
            parameter: "dual cavity pawn signed volume",
            value: signed_volume,
        });
    }
    Ok(mesh)
}

/// `(zA, zB)`: the throat's lower and upper junction heights, in mm, for a
/// given physical throat radius — the closed form
/// [`make_dual_cavity_pawn_with_throat_radius`] itself builds the profile
/// from, exported so every downstream assertion measures against the
/// generator's own arithmetic rather than a restated copy of it.
///
/// `zB > zA` for every `throat_radius_mm > 0`, because
/// [`DUAL_CAVITY_CENTRE_SPACING_MM`] is exactly the sum of the two silhouette
/// radii: `zB - zA = R1 + R2 - sqrt(R1^2 - rt^2) - sqrt(R2^2 - rt^2)`, which
/// is `0` at `rt = 0` and strictly increasing in `rt`.
#[must_use]
pub fn dual_cavity_junction_heights_mm(throat_radius_mm: f64) -> (f64, f64) {
    let r1 = DUAL_CAVITY_LOWER_SPHERE_R_MM;
    let z1 = DUAL_CAVITY_LOWER_SPHERE_Z_MM;
    let r2 = DUAL_CAVITY_UPPER_SPHERE_R_MM;
    let z2 = z1 + DUAL_CAVITY_CENTRE_SPACING_MM;
    let rt = throat_radius_mm;
    (
        z1 + (r1 * r1 - rt * rt).sqrt(),
        z2 - (r2 * r2 - rt * rt).sqrt(),
    )
}

/// The **exact** volume, in mm³, of the smooth solid of revolution
/// [`make_dual_cavity_pawn_with_throat_radius`] approximates — the reference
/// its mesh is measured against, computed rather than asserted.
///
/// Three disjoint pieces, split at the two junction heights
/// [`dual_cavity_junction_heights_mm`] returns, each an exact closed form of
/// the very surfaces the generator laths:
/// 1. the lower silhouette sphere between the foot plane `z = 0` and `zA`,
///    `pi * integral_{-z1}^{zA - z1} (R1^2 - u^2) du`;
/// 2. the throat's right circular cylinder, `pi * rt^2 * (zB - zA)`;
/// 3. the upper silhouette sphere's cap above `zB`, `pi h^2 (3 R2 - h) / 3`
///    for `h = z2 + R2 - zB`.
///
/// Note what this is *not*: the sum of a truncated lower sphere and a whole
/// upper sphere. That over-counts both the upper sphere's cap below `zB` and
/// the lower sphere's cap above `zA`, neither of which is in the lathe solid.
///
/// The generated mesh is strictly **inscribed** in this solid (every ring
/// vertex lies on the exact surface, every chord between rings lies inside a
/// profile that is concave in `z`, and each ring's regular
/// [`DUAL_CAVITY_SLICES`]-gon lies inside its own circle), so the mesh's
/// signed volume is always slightly *below* this figure — by the azimuthal
/// faceting factor `sinc(pi / SLICES)` plus the polar chord deficit.
#[must_use]
pub fn dual_cavity_pawn_analytic_volume_mm3(throat_radius_mm: f64) -> f64 {
    use core::f64::consts::PI;
    let r1 = DUAL_CAVITY_LOWER_SPHERE_R_MM;
    let z1 = DUAL_CAVITY_LOWER_SPHERE_Z_MM;
    let r2 = DUAL_CAVITY_UPPER_SPHERE_R_MM;
    let z2 = z1 + DUAL_CAVITY_CENTRE_SPACING_MM;
    let (z_a, z_b) = dual_cavity_junction_heights_mm(throat_radius_mm);

    let antiderivative = |u: f64| r1 * r1 * u - u * u * u / 3.0;
    let lower = PI * (antiderivative(z_a - z1) - antiderivative(-z1));
    let throat = PI * throat_radius_mm * throat_radius_mm * (z_b - z_a);
    let cap_height = z2 + r2 - z_b;
    let upper = PI * cap_height * cap_height * (3.0 * r2 - cap_height) / 3.0;
    lower + throat + upper
}

/// Push one full lathe ring of [`DUAL_CAVITY_SLICES`] vertices at radius `r`,
/// height `z`, f32-snapped as they are produced.
fn push_lathe_ring(verts: &mut Vec<[f64; 3]>, r: f64, z: f64, slices: usize) {
    for s in 0..slices {
        let theta_s = 2.0 * core::f64::consts::PI * s as f64 / slices as f64;
        let (sin, cos) = theta_s.sin_cos();
        verts.push(snap_vertex([r * cos, r * sin, z]));
    }
}

/// Snap a vertex onto the STL f32 wire: `v -> (v as f32) as f64`. See the
/// "Mandatory f32 snap" section of [`make_dual_cavity_pawn_with_throat_radius`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "deliberate narrowing to the binary32 the printer file format stores. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn snap_vertex(point: [f64; 3]) -> [f64; 3] {
    [
        f64::from(point[0] as f32),
        f64::from(point[1] as f32),
        f64::from(point[2] as f32),
    ]
}

/// The mesh's own signed volume (positive iff every face winds outward), by
/// the divergence-theorem tetrahedron sum. Used only to derive, rather than
/// assume, that this fixture's triangulation order winds outward — the
/// `append_tetra` precedent this module already follows.
fn mesh_signed_volume(mesh: &Mesh) -> f64 {
    mesh.tris()
        .iter()
        .map(|triangle| {
            let a = mesh.verts()[triangle[0] as usize];
            let b = mesh.verts()[triangle[1] as usize];
            let c = mesh.verts()[triangle[2] as usize];
            // `geom_kernel::vec3` (canon §1 OP-9, C8a). Byte-neutral: same
            // cross operand order, same left-to-right dot association.
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
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

// ---------------------------------------------------------------------------
// Slice-sweep oracle fixtures (LANE 1,
// `docs/plans/slice-sweep-core-2026-09-02.md` §10).
//
// These exist so the sweep's three oracles can be graded from a CLEAN CHECKOUT.
// The `Round_25/50/100.stl` bases that motivated the work are run outputs and
// are not in this repository; committing them is LANE 2's job (design decision
// D11). Until they are, the disc oracle grades a PARAMETRIC MEMBER OF THE SAME
// FAMILY -- drafted rim, recessed top -- and every area it grades against is
// measured by the harness from the slice stack, never hand-fed.
// ---------------------------------------------------------------------------

/// The overhang table with square legs of side `w`, for the model-section
/// overload witness.
///
/// The stock [`make_overhang_table`] does **not** fire the sweep's
/// `model_section_overload` warning and the oracle says so: at 4 mm legs the
/// leg carries `X_model = 64.334 / 16 = 4.02`, comfortably under
/// `X_ALLOWED = 12`. Thin it to 1.5 mm and the same arithmetic gives
/// `(2.25 + 12.780 + 14.254) / 2.25 = 13.02 > 12`, which is one warning on a
/// file that is still emitted. Same generator, same family, one dimension
/// changed — no new geometry code.
pub fn make_overhang_table_thin(leg_width_mm: f64) -> Result<Mesh, MeshError> {
    make_overhang_table_with(
        [leg_width_mm, leg_width_mm, 20.0],
        [30.0, 30.0, 4.0],
        0.0,
    )
}

/// A hollow box: a `outer` mm cube with `wall` mm walls and a closed cubic
/// cavity, wound so the cavity's own faces point INTO the void.
///
/// The cavity-ceiling oracle. Its ceiling underside can only be reached from
/// inside, so a placement engine either supports it from the cavity's lower
/// interior — which is what `docs/physics.md` §"supports may seat on model"
/// requires — or leaves it bare. A strut punched through the exterior wall is
/// the wrong answer and this shape is what makes the difference visible.
pub fn make_cavity_box(outer_mm: f64, wall_mm: f64) -> Result<Mesh, MeshError> {
    validate_positive("outer", outer_mm)?;
    validate_positive("wall", wall_mm)?;
    if 2.0 * wall_mm >= outer_mm {
        return Err(MeshError::InvalidFiniteFixtureParameter {
            parameter: "wall",
            value: wall_mm,
        });
    }
    let inner = outer_mm - 2.0 * wall_mm;
    let mut verts = Vec::with_capacity(16);
    let mut tris = Vec::with_capacity(24);
    append_box(
        &mut verts,
        &mut tris,
        [outer_mm, outer_mm, outer_mm],
        [0.0, 0.0, outer_mm / 2.0],
    )?;
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    let half = inner / 2.0;
    for x in [-half, half] {
        for y in [-half, half] {
            for z in [-half, half] {
                verts.push([x, y, outer_mm / 2.0 + z]);
            }
        }
    }
    // The cavity is the same box with every triangle reversed: its normals
    // point into the void, which is what makes the void a void rather than a
    // second solid.
    tris.extend(BOX_FACES.map(|face| [base + face[0], base + face[2], base + face[1]]));
    Mesh::new(verts, tris)
}

/// A right cylinder of `diameter` mm and `height` mm whose flat caps are
/// **single fans** from a centre vertex.
///
/// Half of the triangulation-invariance pair. See [`make_disc_uniform`].
pub fn make_disc_fan(diameter_mm: f64, height_mm: f64, segments: usize) -> Result<Mesh, MeshError> {
    disc(diameter_mm, height_mm, segments, 1)
}

/// The same cylinder as [`make_disc_fan`], with its caps subdivided into
/// `rings` concentric rings instead of one fan.
///
/// **The triangulation-invariance witness.** The two meshes bound the same
/// solid and differ only in how many triangles cover the flat faces, so any
/// placement engine that keys its coverage raster or its load ledger to
/// TRIANGLES rather than to area will disagree with itself between them. That
/// is exactly the defect the disc diagnosis of 2026-09-02 named RC3 and RC4,
/// and sabotage S6 re-introduces it to check that this pair can still see it.
pub fn make_disc_uniform(
    diameter_mm: f64,
    height_mm: f64,
    segments: usize,
    rings: usize,
) -> Result<Mesh, MeshError> {
    disc(diameter_mm, height_mm, segments, rings.max(2))
}

fn disc(
    diameter_mm: f64,
    height_mm: f64,
    segments: usize,
    rings: usize,
) -> Result<Mesh, MeshError> {
    validate_positive("diameter", diameter_mm)?;
    validate_positive("height", height_mm)?;
    if segments < 3 || rings == 0 {
        return Err(MeshError::InvalidFiniteFixtureParameter {
            parameter: "segments",
            value: segments as f64,
        });
    }
    let radius = diameter_mm / 2.0;
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    append_cap(&mut verts, &mut tris, radius, 0.0, segments, rings, false)?;
    append_cap(
        &mut verts, &mut tris, radius, height_mm, segments, rings, true,
    )?;
    append_skirt(
        &mut verts, &mut tris, radius, 0.0, radius, height_mm, segments, true,
    )?;
    Mesh::new(verts, tris)
}

/// A drafted, recessed disc: the parametric member of the `Round_*` base
/// family the disc oracle grades.
///
/// The measured shape of `Round_50.stl`
/// (`plinth-evidence/disc-diagnosis-2026-09-02/first-principles/FINDINGS.md`
/// §0) is a **drafted frustum**, `r 24 → 25` over `z −3.5 → 0`, with a 2.5 mm
/// recess in the top face. That draft is why the bottom layer's demand is the
/// bottom FACE and not the footprint, and the recess is why the largest solid
/// cross-section is the layer immediately below the recess and not the bottom
/// or the rim top. Both facts are what the oracle checks, so the fixture has to
/// carry them.
///
/// `z = 0` is the bottom face. The recess opens downward from the top face.
pub fn make_recessed_disc(
    bottom_radius_mm: f64,
    top_radius_mm: f64,
    height_mm: f64,
    recess_depth_mm: f64,
    recess_radius_mm: f64,
    segments: usize,
) -> Result<Mesh, MeshError> {
    validate_positive("bottom_radius", bottom_radius_mm)?;
    validate_positive("top_radius", top_radius_mm)?;
    validate_positive("height", height_mm)?;
    validate_positive("recess_depth", recess_depth_mm)?;
    validate_positive("recess_radius", recess_radius_mm)?;
    if segments < 3 || recess_depth_mm >= height_mm || recess_radius_mm >= top_radius_mm {
        return Err(MeshError::InvalidFiniteFixtureParameter {
            parameter: "recess_depth",
            value: recess_depth_mm,
        });
    }
    let recess_z = height_mm - recess_depth_mm;
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    // Bottom face, a single fan: the demand surface the oracle measures.
    append_cap(&mut verts, &mut tris, bottom_radius_mm, 0.0, segments, 1, false)?;
    // Drafted rim, outward normals.
    append_skirt(
        &mut verts,
        &mut tris,
        bottom_radius_mm,
        0.0,
        top_radius_mm,
        height_mm,
        segments,
        true,
    )?;
    // Top annulus between the recess lip and the rim.
    append_annulus(
        &mut verts,
        &mut tris,
        recess_radius_mm,
        top_radius_mm,
        height_mm,
        segments,
        true,
    )?;
    // Recess wall, normals pointing INTO the recess void.
    append_skirt(
        &mut verts,
        &mut tris,
        recess_radius_mm,
        recess_z,
        recess_radius_mm,
        height_mm,
        segments,
        false,
    )?;
    // Recess ceiling seen from above: it is the top of the remaining material,
    // so it faces up.
    append_cap(
        &mut verts,
        &mut tris,
        recess_radius_mm,
        recess_z,
        segments,
        1,
        true,
    )?;
    Mesh::new(verts, tris)
}

fn ring_point(radius: f64, z: f64, segment: usize, segments: usize) -> [f64; 3] {
    let theta = 2.0 * core::f64::consts::PI * segment as f64 / segments as f64;
    [radius * theta.cos(), radius * theta.sin(), z]
}

/// A flat circular cap at `z`, `rings` concentric rings deep, facing up when
/// `up` and down otherwise.
fn append_cap(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
    radius: f64,
    z: f64,
    segments: usize,
    rings: usize,
    up: bool,
) -> Result<(), MeshError> {
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    verts.push([0.0, 0.0, z]);
    for ring in 1..=rings {
        let r = radius * ring as f64 / rings as f64;
        for segment in 0..segments {
            verts.push(ring_point(r, z, segment, segments));
        }
    }
    let index = |ring: usize, segment: usize| -> u32 {
        base + 1 + u32::try_from((ring - 1) * segments + segment % segments).unwrap_or(u32::MAX)
    };
    let mut push = |a: u32, b: u32, c: u32| {
        // One winding rule for both faces, applied once: the caller says which
        // way the face looks and the swap happens here, so no call site has to
        // remember an orientation convention.
        if up {
            tris.push([a, b, c]);
        } else {
            tris.push([a, c, b]);
        }
    };
    for segment in 0..segments {
        push(base, index(1, segment), index(1, segment + 1));
    }
    for ring in 2..=rings {
        for segment in 0..segments {
            push(
                index(ring - 1, segment),
                index(ring, segment),
                index(ring, segment + 1),
            );
            push(
                index(ring - 1, segment),
                index(ring, segment + 1),
                index(ring - 1, segment + 1),
            );
        }
    }
    Ok(())
}

/// A flat annulus at `z` between `inner` and `outer`.
fn append_annulus(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
    inner: f64,
    outer: f64,
    z: f64,
    segments: usize,
    up: bool,
) -> Result<(), MeshError> {
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    for segment in 0..segments {
        verts.push(ring_point(inner, z, segment, segments));
    }
    for segment in 0..segments {
        verts.push(ring_point(outer, z, segment, segments));
    }
    let count = u32::try_from(segments).map_err(|_| MeshError::FixtureIndexOverflow)?;
    for segment in 0..segments {
        let s = u32::try_from(segment).map_err(|_| MeshError::FixtureIndexOverflow)?;
        let n = (s + 1) % count;
        let (i0, i1, o0, o1) = (base + s, base + n, base + count + s, base + count + n);
        if up {
            tris.push([i0, o0, o1]);
            tris.push([i0, o1, i1]);
        } else {
            tris.push([i0, o1, o0]);
            tris.push([i0, i1, o1]);
        }
    }
    Ok(())
}

/// A frustum wall from `(r0, z0)` to `(r1, z1)`; `outward` selects which side
/// its normals face.
#[allow(clippy::too_many_arguments, reason = "one revolution's six geometric parameters plus its winding; grouping them into a struct would hide the call sites rather than clarify them")]
fn append_skirt(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[u32; 3]>,
    r0: f64,
    z0: f64,
    r1: f64,
    z1: f64,
    segments: usize,
    outward: bool,
) -> Result<(), MeshError> {
    let base = u32::try_from(verts.len()).map_err(|_| MeshError::FixtureIndexOverflow)?;
    for segment in 0..segments {
        verts.push(ring_point(r0, z0, segment, segments));
    }
    for segment in 0..segments {
        verts.push(ring_point(r1, z1, segment, segments));
    }
    let count = u32::try_from(segments).map_err(|_| MeshError::FixtureIndexOverflow)?;
    for segment in 0..segments {
        let s = u32::try_from(segment).map_err(|_| MeshError::FixtureIndexOverflow)?;
        let n = (s + 1) % count;
        let (b0, b1, t0, t1) = (base + s, base + n, base + count + s, base + count + n);
        // Derived, not guessed: with the ring parameterised counter-clockwise
        // seen from +z, the cross product of (t1 - b0) and (t0 - b0) points
        // radially OUTWARD, so that is the outward winding and its swap is the
        // inward one. The unit test below checks the sign on a real cylinder
        // rather than trusting this paragraph.
        if outward {
            tris.push([b0, t1, t0]);
            tris.push([b0, b1, t1]);
        } else {
            tris.push([b0, t0, t1]);
            tris.push([b0, t1, b1]);
        }
    }
    Ok(())
}



/// **The slenderness lane's broad case (2026-09-06): a 30 x 30 x 4 mm plate
/// held 30 mm above the plate on four 2 mm legs, so that EVERY arm under it is
/// long and thin.**
///
/// Same family and same generator as [`make_overhang_table`] — four legs, one
/// top, no new geometry code — with the legs stretched from 20 mm to 30 mm and
/// narrowed from 4 mm to 2 mm. The 2 mm legs are what make it printable at all:
/// a plate floating 30 mm up on nothing is an island, and an island's arms are
/// the engine's own invention rather than a property of the model.
///
/// What it is FOR: under a 30 x 30 mm underside at the registered hex spacing
/// the up-sweep places dozens of arms, every one of them descending about 30 mm
/// to the plate, and the area law gives most of them the smallest printable
/// tip. At `S = 50` and `r = 0.05 mm` that is a limit of 2.5 mm against a 30 mm
/// run — twelve times over — so remedies 1 and 2 both fire, broadly, on one
/// small model. It is the piece the owner should eyeball first to judge whether
/// `S = 50` produces a field that looks like something he would print.
pub fn make_tall_stilt_plate() -> Result<Mesh, MeshError> {
    make_overhang_table_with([2.0, 2.0, 30.0], [30.0, 30.0, 4.0], 0.0)
}

/// **The MAST case (2026-09-06): one 6 mm square overhang 30 mm up on a 2 mm
/// stalk, with nothing else anywhere near it.**
///
/// The overhang's underside is a 6 x 6 mm square ring around a 2 x 2 mm stalk,
/// 30 mm above the plate. The arms the up-sweep puts under that ring are 30 mm
/// long, thin, and — this is the whole point of the fixture — they have NO
/// neighbouring shaft to brace to except each other and the stalk's own
/// footprint, and the model's own material stands between them. Remedy 2 has
/// nothing to reach, so remedy 3 has to fire: a new plate-rooted mast is
/// dropped beside the peak and an `|X|` ties the two.
///
/// Nothing else stands within 20 mm of it, which is what makes "no neighbour in
/// reach" a property of the model rather than of the placement.
pub fn make_lonely_peak() -> Result<Mesh, MeshError> {
    make_peaks_with([2.0, 2.0, 30.0], [6.0, 6.0, 2.0], &[0.0])
}

/// **The `|X|` case (2026-09-06): two 6 mm square overhangs 30 mm up on 2 mm
/// stalks, their undersides 4 mm apart.**
///
/// [`make_lonely_peak`] twice, 10 mm between centres, so that 6 mm of overhang
/// on each side leaves a 4 mm gap between the two undersides. The arms under
/// the two rims face each other across that gap with clear air between them, so
/// a shaft IS within brace reach and remedy 2 fires: two crossing diagonals,
/// each inside the overhang law, no horizontal span.
///
/// The 4 mm gap is chosen against the geometry and not against a target count:
/// a diagonal across 4 mm at the saturated overhang angle is 4.74 mm long, which
/// needs a 0.095 mm brace radius to satisfy its own slenderness limit — inside
/// the registered radius clamps at both ends, so the case is admissible without
/// anything being loosened for it.
pub fn make_brace_pair() -> Result<Mesh, MeshError> {
    make_peaks_with([2.0, 2.0, 30.0], [6.0, 6.0, 2.0], &[-5.0, 5.0])
}

/// **The CONGESTION case (2026-09-06): a horizontal tube, 20 mm across and
/// 40 mm long, standing on two 3 mm feet so that no orientation search can lay
/// it any flatter than it already is.**
///
/// This is the Cylinder's own defect, made into a fixture. A body lying on its
/// side routes its whole peel load down through itself to the LINE where it is
/// lowest, and the area law then puts `peak / X_ALLOWED` of arm section on that
/// line however the arms are arranged. Merging cannot thin it — a merge
/// re-routes load and never removes it, and the section a load needs is a
/// property of the load. So the roots stack, and the owner sees "chock-full of
/// overlapping thick supports" at the bottom of the piece.
///
/// The feet are what make the fixture honest. Without them the orientation
/// search stands the tube on an end and the crowded line never exists; on two
/// 3 mm feet the flat-bottomed pose is the only one that is not a much worse
/// overhang, so the case is a property of the MODEL rather than of the search.
///
/// The tube is a 32-sided prism whose section is turned half a step, so it has
/// a flat bottom strip about 1.96 mm wide for the feet to meet face to face —
/// the same face-to-face contact [`make_overhang_table`] uses between its legs
/// and its top, and never an overlap the parity fill would have to resolve.
/// Every coordinate is snapped to an exact `f32`, because `write_binary_stl`
/// refuses anything else and a fixture that cannot be exported cannot be run
/// through the production CLI.
pub fn make_laid_tube() -> Result<Mesh, MeshError> {
    make_laid_tube_with(10.0, 40.0, 3.0, 32)
}

/// The generator behind [`make_laid_tube`], so the fixture's dimensions are
/// arguments rather than literals buried in a mesh.
///
/// `sections` is the prism's side count and must be a positive multiple of 4,
/// which is what puts a flat facet at the bottom and keeps the section
/// symmetric about both axes.
pub fn make_laid_tube_with(
    radius_mm: f64,
    length_mm: f64,
    foot_height_mm: f64,
    sections: usize,
) -> Result<Mesh, MeshError> {
    validate_positive("tube.radius", radius_mm)?;
    validate_positive("tube.length", length_mm)?;
    validate_positive("tube.foot_height", foot_height_mm)?;
    if sections < 8 || !sections.is_multiple_of(4) {
        return Err(MeshError::InvalidPositiveFixtureParameter {
            parameter: "tube.sections",
            value: sections as f64,
        });
    }
    // Exactly representable in f32, or `write_binary_stl` refuses the fixture.
    // Snapping is done ONCE, here, so no coordinate downstream can be a value
    // that survives in f64 and dies at the file boundary.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the truncation IS the operation: `write_binary_stl` refuses any coordinate that \
              does not round-trip through f32, so every vertex is snapped to the nearest f32 here \
              rather than dying at the file boundary"
    )]
    let snap = |value: f64| f64::from(value as f32);

    let half_length = snap(length_mm / 2.0);
    let axis_z = snap(foot_height_mm + radius_mm * (core::f64::consts::PI / sections as f64).cos());
    let step = 2.0 * core::f64::consts::PI / sections as f64;
    // Half a step of turn, so the lowest two vertices sit at the same height
    // and the prism has a flat bottom facet rather than a bottom EDGE.
    let ring: Vec<[f64; 2]> = (0..sections)
        .map(|section| {
            let theta = -core::f64::consts::FRAC_PI_2 + (section as f64 + 0.5) * step;
            [snap(radius_mm * theta.cos()), snap(axis_z + radius_mm * theta.sin())]
        })
        .collect();
    let bottom_half_width = ring
        .iter()
        .map(|point| point[0].abs())
        .fold(0.0_f64, f64::max)
        .min(snap(radius_mm * (step / 2.0).sin()));

    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(2 * sections + 16);
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(4 * sections + 24);
    // Two rings: the far end first, then the near end, so a side quad is
    // `(far[i], far[i+1], near[i+1], near[i])`.
    for end in [-half_length, half_length] {
        for point in &ring {
            verts.push([point[0], end, point[1]]);
        }
    }
    let wrap = |section: usize| u32::try_from((section + 1) % sections).unwrap_or(0);
    for section in 0..sections {
        let here = u32::try_from(section).map_err(|_| MeshError::FixtureIndexOverflow)?;
        let next = wrap(section);
        let far_here = here;
        let far_next = next;
        let near_here = here + u32::try_from(sections).unwrap_or(0);
        let near_next = next + u32::try_from(sections).unwrap_or(0);
        tris.push([far_here, near_here, near_next]);
        tris.push([far_here, near_next, far_next]);
    }
    // The two end caps, as fans about vertex 0 of each ring. A convex section
    // makes a fan a valid triangulation without any further test.
    for section in 1..sections - 1 {
        let here = u32::try_from(section).map_err(|_| MeshError::FixtureIndexOverflow)?;
        let offset = u32::try_from(sections).unwrap_or(0);
        // The far cap faces -Y and the near cap +Y, so the two fans wind
        // OPPOSITE ways round the same ring. Getting this backwards makes a
        // mesh that closes and is inside-out, which the hollow stage refuses as
        // "not one closed, consistently oriented solid" — measured, 2026-09-06.
        tris.push([0, here, here + 1]);
        tris.push([offset, offset + here + 1, offset + here]);
    }

    // The two feet, meeting the flat bottom strip face to face.
    let foot = [
        snap(bottom_half_width),
        snap(length_mm / 8.0),
        snap(foot_height_mm),
    ];
    for centre_y in [snap(-length_mm * 0.3), snap(length_mm * 0.3)] {
        append_box(
            &mut verts,
            &mut tris,
            foot,
            [0.0, centre_y, snap(foot_height_mm / 2.0)],
        )?;
    }
    Mesh::new(verts, tris)
}

/// One stalk-and-cap peak per entry in `centres_x`, all on the same plate.
///
/// The generator behind [`make_lonely_peak`] and [`make_brace_pair`], so the
/// two fixtures differ in their layout and in nothing else.
pub fn make_peaks_with(
    stalk: [f64; 3],
    cap: [f64; 3],
    centres_x: &[f64],
) -> Result<Mesh, MeshError> {
    validate_dimensions("stalk", stalk)?;
    validate_dimensions("cap", cap)?;
    let mut verts = Vec::with_capacity(16 * centres_x.len());
    let mut tris = Vec::with_capacity(24 * centres_x.len());
    for (index, x) in centres_x.iter().enumerate() {
        validate_finite("centre_x", *x)?;
        if index > 0 && *x <= centres_x[index - 1] {
            // Strictly increasing centres, so two peaks can never be asked to
            // occupy the same place and quietly produce a self-intersecting
            // mesh the census would then have to repair.
            return Err(MeshError::InvalidPositiveFixtureParameter {
                parameter: "centres_x.spacing",
                value: *x - centres_x[index - 1],
            });
        }
        append_box(&mut verts, &mut tris, stalk, [*x, 0.0, stalk[2] / 2.0])?;
        append_box(
            &mut verts,
            &mut tris,
            cap,
            [*x, 0.0, stalk[2] + cap[2] / 2.0],
        )?;
    }
    Mesh::new(verts, tris)
}


#[cfg(test)]
mod slice_sweep_fixture_tests {
    use super::*;

    /// Signed volume by the divergence theorem. Positive means every face
    /// winds outward; a fixture that comes out inside-out slices with its
    /// windings inverted and every downstream fill rule reads the void as the
    /// solid, so this is checked rather than assumed.
    fn signed_volume(mesh: &Mesh) -> f64 {
        mesh.tris()
            .iter()
            .map(|tri| {
                let a = mesh.verts()[tri[0] as usize];
                let b = mesh.verts()[tri[1] as usize];
                let c = mesh.verts()[tri[2] as usize];
                dot(a, cross(b, c)) / 6.0
            })
            .sum()
    }

    #[test]
    fn the_disc_pair_bounds_the_same_solid_with_outward_windings() {
        let fan = make_disc_fan(30.0, 4.0, 128).unwrap();
        let uniform = make_disc_uniform(30.0, 4.0, 128, 8).unwrap();
        let exact = core::f64::consts::PI * 15.0 * 15.0 * 4.0;
        // A 128-gon inscribes 99.97 % of its circle, so the tolerance is the
        // faceting and nothing else.
        assert!((signed_volume(&fan) / exact - 1.0).abs() < 1.0e-3);
        assert!((signed_volume(&uniform) / signed_volume(&fan) - 1.0).abs() < 1.0e-12);
        // The point of the pair: same solid, very different triangle counts.
        assert!(uniform.tris().len() > 4 * fan.tris().len());
    }

    #[test]
    fn the_recessed_disc_carries_its_draft_and_its_recess() {
        let disc = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).unwrap();
        let frustum = core::f64::consts::PI * 3.5 / 3.0 * (24.0 * 24.0 + 24.0 * 25.0 + 25.0 * 25.0);
        let recess = core::f64::consts::PI * 22.5 * 22.5 * 2.5;
        assert!((signed_volume(&disc) / (frustum - recess) - 1.0).abs() < 1.0e-3);
    }

    #[test]
    fn the_cavity_box_encloses_a_void_and_not_a_second_solid() {
        let box_mesh = make_cavity_box(40.0, 3.0).unwrap();
        assert!((signed_volume(&box_mesh) - (40.0_f64.powi(3) - 34.0_f64.powi(3))).abs() < 1.0e-6);
    }

    #[test]
    fn the_thin_table_is_the_stock_table_with_one_dimension_changed() {
        let thin = make_overhang_table_thin(1.5).unwrap();
        let stock = make_overhang_table().unwrap();
        assert_eq!(thin.tris().len(), stock.tris().len());
        assert!(signed_volume(&thin) < signed_volume(&stock));
    }
}