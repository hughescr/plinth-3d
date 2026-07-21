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


def make_thin_fin(base=(20, 20, 4), fin=(0.6, 10, 15), z0=0.0):
    """Synthetic fixture: a block with a thin fin sticking up -- for
    delicacy/thinness field tests."""
    b = trimesh.creation.box(extents=base)
    b.apply_translation((0, 0, z0 + base[2] / 2))
    f = trimesh.creation.box(extents=fin)
    f.apply_translation((0, 0, z0 + base[2] + fin[2] / 2))
    merged = trimesh.util.concatenate([b, f])
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(), process=True)
