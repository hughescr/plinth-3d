"""Mesh loading helpers. Thin wrapper over trimesh so the rest of the
package works with plain (vertices, faces) + trimesh conveniences without
every module re-deriving load quirks (process=False to avoid trimesh
silently welding/reordering verts out from under a caller who wants to
correlate results back to source indices)."""
import numpy as np
import trimesh


def load_stl(path):
    """Load an STL as a trimesh.Trimesh, unprocessed (no implicit vertex
    merge/reorder) so downstream code's vertex indices stay meaningful.
    Raises on failure -- do not silently return an empty/partial mesh."""
    mesh = trimesh.load_mesh(path, file_type="stl", process=False)
    if not isinstance(mesh, trimesh.Trimesh):
        raise ValueError(f"{path} did not load as a single Trimesh (got {type(mesh)})")
    if len(mesh.faces) == 0:
        raise ValueError(f"{path} loaded with zero faces")
    return mesh


def bounds_z(mesh):
    return float(mesh.bounds[0][2]), float(mesh.bounds[1][2])


def face_normals_z(mesh):
    """Z component of each (unit) face normal."""
    n = mesh.face_normals
    return n[:, 2]


def make_box(size=(10, 10, 10), translate=(0, 0, 0)):
    """Synthetic test fixture: axis-aligned box, bottom face at translate.z."""
    b = trimesh.creation.box(extents=size)
    b.apply_translation((translate[0], translate[1], translate[2] + size[2] / 2.0))
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(), process=False)


def make_overhang_table(leg=(4, 4, 20), top=(30, 30, 4), z0=0.0):
    """Synthetic fixture: a table -- four legs holding up a flat slab with a
    large down-facing overhang (the slab underside, minus where legs meet
    it). Used to unit-test overhang/island detection and routing without
    needing a real mini."""
    parts = []
    lx, ly, lz = leg
    tx, ty, tz = top
    inset = 3.0
    xs = [inset + lx / 2, tx - inset - lx / 2]
    ys = [inset + ly / 2, ty - inset - ly / 2]
    for x in xs:
        for y in ys:
            b = trimesh.creation.box(extents=leg)
            b.apply_translation((x - tx / 2, y - ty / 2, z0 + lz / 2))
            parts.append(b)
    slab = trimesh.creation.box(extents=top)
    slab.apply_translation((0, 0, z0 + lz + tz / 2))
    parts.append(slab)
    merged = trimesh.util.concatenate(parts)
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(), process=True)


def make_toed_slab(top=(30, 30, 4), z_bottom=15.0):
    """Synthetic fixture for the v3.6 layer-island minima rule: a high
    slab with two downward cones hanging from its underside.

      - shallow toe (r=3, h=4, apex z_bottom-4): flanks ~53deg from
        horizontal, so its faces ARE down-facing overhang faces -- an
        island the area model sees, but whose apex still births a layer
        island no spacing-based contact necessarily pins;
      - steep toe (r=1.5, h=6, apex z_bottom-6): flanks ~76deg, faces
        only ~14deg below horizontal -- INVISIBLE to the overhang-angle
        island model. Exactly the missed class the minima rule exists
        for.
    """
    parts = []
    slab = trimesh.creation.box(extents=top)
    slab.apply_translation((0, 0, z_bottom + top[2] / 2))
    parts.append(slab)
    down = trimesh.transformations.rotation_matrix(np.pi, (1, 0, 0))
    shallow = trimesh.creation.cone(radius=3.0, height=4.0)
    shallow.apply_transform(down)
    shallow.apply_translation((-8, 0, z_bottom))  # apex at z_bottom - 4
    parts.append(shallow)
    steep = trimesh.creation.cone(radius=1.5, height=6.0)
    steep.apply_transform(down)
    steep.apply_translation((8, 0, z_bottom))  # apex at z_bottom - 6
    parts.append(steep)
    merged = trimesh.util.concatenate(parts)
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(), process=True)


def make_raking_blade(top=(20, 20, 4), z_bottom=10.0, blade=(0.03, 2.0, 5.0),
                       rake_deg=85.0):
    """Synthetic fixture for v4 p6's I1 slice-based island-birth detection
    (TDD regression for the class local_z_minima's vertex-graph model
    cannot cover on its own): a slab with a THIN blade hanging from its
    underside, rotated `rake_deg` from vertical about the y-axis so it
    rakes steeply sideways as it descends -- the real Weeping Treant
    corpus's "raking/migrating cluster" case (design scratchpad
    island_contact_demand_result.json: 10 such clusters, 1-4-contact
    ladders).

    Physically: the blade is topologically ONE connected minimum (a
    single lowest vertex at its free tip) -- local_z_minima correctly
    finds exactly that one apex and stops there, a single contact. But
    because the blade is thin (bx) and rakes far from vertical, its
    cross-section at each REAL PRINT LAYER (cfg.layer_height_mm) shifts
    sideways by roughly layer_height * tan(rake_deg) -- at the default
    85 degrees that is ~0.57mm per 0.05mm layer, comfortably exceeding
    both the blade's own cross-sectional width and
    perception.ISLAND_ATTACH_REACH_MM (0.2mm). Every consecutive pair of
    real-layer slices therefore fails to overlap: perception.
    island_births sees a whole LADDER of distinct birth events climbing
    the blade that a single apex contact never reaches at birth (Craig's
    criterion is about the print-time state at each layer, not mesh
    topology) -- exactly the gap I2's contact-demand pass exists to fill.

    CALIBRATED TO THE 0.05mm DEFAULT (cfg.layer_height_mm), not
    layer-height-independent: the ladder only appears because per-layer
    xy shift (layer_height * tan(rake_deg)) exceeds the blade's own
    cross-sectional width at THAT layer height. Empirically verified at a
    finer 0.02mm profile the per-layer shift drops under the blade's
    width, consecutive slices go back to overlapping, and the fixture
    genuinely stops birthing a ladder (perception.local_z_minima still
    finds 2, but perception.island_births drops from 9 to 1) -- this is
    the PHYSICALLY CORRECT behavior (a slower-migrating cross-section at
    finer layers really does re-merge), not a bug, but it means this
    fixture's ladder-proof tests are only valid at the 0.05mm default; a
    future dynamic-layer-height pass that changes that default would need
    to re-tune `blade`/`rake_deg` here (steeper rake and/or thinner blade)
    to keep exercising the same class of failure.
    """
    parts = []
    slab = trimesh.creation.box(extents=top)
    slab.apply_translation((0, 0, z_bottom + top[2] / 2))
    parts.append(slab)
    bx, by, bz = blade
    b = trimesh.creation.box(extents=(bx, by, bz))
    b.apply_translation((0, 0, -bz / 2))  # top face at local z=0 (attach); tip at z=-bz
    R = trimesh.transformations.rotation_matrix(np.radians(rake_deg), (0, 1, 0),
                                                 point=[0, 0, 0])
    b.apply_transform(R)
    b.apply_translation((0, 0, z_bottom))
    parts.append(b)
    merged = trimesh.util.concatenate(parts)
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(),
                            process=True)


def make_thin_fin(base=(20, 20, 4), fin=(0.6, 10, 15), z0=0.0):
    """Synthetic fixture: a block with a thin fin sticking up -- for
    delicacy/thinness field tests."""
    b = trimesh.creation.box(extents=base)
    b.apply_translation((0, 0, z0 + base[2] / 2))
    f = trimesh.creation.box(extents=fin)
    f.apply_translation((0, 0, z0 + base[2] + fin[2] / 2))
    merged = trimesh.util.concatenate([b, f])
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(), process=True)
