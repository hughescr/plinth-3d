//! The junction hull: the convex hull of the end circles of the members that meet
//! at one landed junction, and the one ring routine every frustum end ring and every
//! hull ring is made with.
//!
//! Craig's ruling (v18, 2026-10-05): the junction between two members is the CONVEX
//! HULL of the two meeting end circles. A thin member buried in a thick post is the
//! same hull hidden inside it and costs nothing. v19 drew a flat cap square to the
//! branch's own axis against a collar cone instead, and the cap's rim stood out of
//! the host by up to 0.3 mm as a down-facing lens.
//!
//! **One ring routine.** A hull ring must be bit-identical to the end ring of the
//! neighbour frustum it meets, or the hull and the neighbour disagree about where
//! the surface is by a rounding step and the seam shows. [`frustum_ring`] is
//! therefore the only code that makes a frustum end ring: `plinth-emit` draws every
//! frustum with it and builds every hull from it, with the neighbour's own start,
//! end, radius and sign.
//!
//! **The hull** is a small incremental 3-D hull for at most a few dozen points (a
//! hull ring is `chitubox_emit_n_sides` points, at most three rings). No dependency,
//! no native call. A point within [`HULL_COPLANAR_RELATIVE`] of a face plane is
//! treated as ON the plane and not as outside it; a polygon's corners are still all
//! found, because the faces next to the polygon see each of them as strictly
//! outside.

use geom_kernel::vec3::{add, cross, dot, norm, normalize, scale, sub};

/// A point is on a face's plane, not outside it, when its distance from the plane is
/// at most this fraction of the point set's extent. Float noise, not a physical
/// allowance: the rings are coplanar to about 1e-16 of their size.
pub const HULL_COPLANAR_RELATIVE: f64 = 1.0e-10;

/// A convex hull as an indexed triangle mesh, every triangle wound counter-clockwise
/// seen from outside.
#[derive(Clone, Debug, PartialEq)]
pub struct ConvexHull {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[usize; 3]>,
}

impl ConvexHull {
    /// The enclosed volume, in the units of the points cubed.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.triangles
            .iter()
            .map(|face| {
                dot(
                    self.vertices[face[0]],
                    cross(self.vertices[face[1]], self.vertices[face[2]]),
                )
            })
            .sum::<f64>()
            / 6.0
    }

    /// The triangles as corner coordinates, for a soup consumer.
    #[must_use]
    pub fn soup(&self) -> Vec<[[f64; 3]; 3]> {
        self.triangles
            .iter()
            .map(|face| {
                [
                    self.vertices[face[0]],
                    self.vertices[face[1]],
                    self.vertices[face[2]],
                ]
            })
            .collect()
    }
}

struct Face {
    corners: [usize; 3],
    normal: [f64; 3],
    offset: f64,
}

fn make_face(points: &[[f64; 3]], corners: [usize; 3], interior: [f64; 3]) -> Option<Face> {
    let [a, b, c] = corners;
    let mut normal = normalize(cross(sub(points[b], points[a]), sub(points[c], points[a])))?;
    let mut corners = corners;
    if dot(normal, sub(interior, points[a])) > 0.0 {
        normal = scale(normal, -1.0);
        corners = [a, c, b];
    }
    Some(Face {
        corners,
        normal,
        offset: dot(normal, points[a]),
    })
}

/// The convex hull of `points`, or `None` when they do not span a solid (fewer than
/// four points, or all on one plane). Deterministic: points are inserted in order.
#[must_use]
pub fn convex_hull(points: &[[f64; 3]]) -> Option<ConvexHull> {
    if points.len() < 4 || points.iter().any(|p| p.iter().any(|v| !v.is_finite())) {
        return None;
    }
    let mut lo = points[0];
    let mut hi = points[0];
    for p in points {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    let extent = norm(sub(hi, lo));
    if extent <= 0.0 {
        return None;
    }
    let eps = HULL_COPLANAR_RELATIVE * extent;
    // The starting tetrahedron: the extreme point on x, the point farthest from it,
    // the point farthest from that line, the point farthest from that plane.
    let i0 = (0..points.len()).min_by(|&i, &j| points[i][0].total_cmp(&points[j][0]))?;
    let i1 = (0..points.len()).max_by(|&i, &j| {
        norm(sub(points[i], points[i0])).total_cmp(&norm(sub(points[j], points[i0])))
    })?;
    let line = sub(points[i1], points[i0]);
    let i2 = (0..points.len()).max_by(|&i, &j| {
        norm(cross(line, sub(points[i], points[i0])))
            .total_cmp(&norm(cross(line, sub(points[j], points[i0]))))
    })?;
    let plane = normalize(cross(line, sub(points[i2], points[i0])))?;
    let i3 = (0..points.len()).max_by(|&i, &j| {
        dot(plane, sub(points[i], points[i0]))
            .abs()
            .total_cmp(&dot(plane, sub(points[j], points[i0])).abs())
    })?;
    if dot(plane, sub(points[i3], points[i0])).abs() <= eps {
        return None;
    }
    let interior = scale(
        add(add(points[i0], points[i1]), add(points[i2], points[i3])),
        0.25,
    );
    let mut faces: Vec<Face> = Vec::with_capacity(4 * points.len());
    for corners in [[i0, i1, i2], [i0, i1, i3], [i0, i2, i3], [i1, i2, i3]] {
        faces.push(make_face(points, corners, interior)?);
    }
    let mut used = vec![false; points.len()];
    for i in [i0, i1, i2, i3] {
        used[i] = true;
    }
    let mut visible: Vec<bool> = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        if used[index] {
            continue;
        }
        visible.clear();
        visible.extend(
            faces
                .iter()
                .map(|face| dot(face.normal, point) - face.offset > eps),
        );
        if !visible.iter().any(|&seen| seen) {
            continue;
        }
        // The horizon: edges of a visible face whose reverse edge is not an edge of
        // another visible face.
        let mut directed: Vec<(usize, usize)> = Vec::new();
        for (face, _) in faces.iter().zip(&visible).filter(|(_, &seen)| seen) {
            let [a, b, c] = face.corners;
            directed.extend([(a, b), (b, c), (c, a)]);
        }
        let horizon: Vec<(usize, usize)> = directed
            .iter()
            .copied()
            .filter(|&(u, v)| !directed.contains(&(v, u)))
            .collect();
        let mut kept: Vec<Face> = faces
            .drain(..)
            .zip(&visible)
            .filter(|(_, &seen)| !seen)
            .map(|(face, _)| face)
            .collect();
        for (u, v) in horizon {
            // Wound like the visible face it replaces, so it stays outward.
            let normal = normalize(cross(sub(points[v], points[u]), sub(point, points[u])));
            let Some(normal) = normal else {
                // A sliver on the horizon edge's own line: it encloses nothing.
                continue;
            };
            kept.push(Face {
                corners: [u, v, index],
                normal,
                offset: dot(normal, points[u]),
            });
        }
        faces = kept;
        used[index] = true;
    }
    // Compact the vertices actually used.
    let mut remap = vec![usize::MAX; points.len()];
    let mut vertices = Vec::new();
    let mut triangles = Vec::with_capacity(faces.len());
    for face in &faces {
        let mut corners = face.corners;
        for corner in &mut corners {
            if remap[*corner] == usize::MAX {
                remap[*corner] = vertices.len();
                vertices.push(points[*corner]);
            }
            *corner = remap[*corner];
        }
        triangles.push(corners);
    }
    Some(ConvexHull {
        vertices,
        triangles,
    })
}

/// The basis `(u, v)` of the plane perpendicular to `normal`, as the emitter has
/// always built it. `None` for a zero or non-finite normal.
#[must_use]
pub fn perpendicular_basis(normal: [f64; 3]) -> Option<([f64; 3], [f64; 3])> {
    let reference = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = cross(normal, reference);
    let length = norm(u);
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }
    let u = scale(u, 1.0 / length);
    Some((u, cross(normal, u)))
}

/// **THE RING ROUTINE**: the `sides` corners of the end ring of the frustum from
/// `start` to `end`, at `radius`, at its start (`at_start`) or its end.
///
/// The ring's plane is perpendicular to the frustum's own axis, or horizontal
/// (`plate_z`, a plate-root column). The corners are `centre + radius * radial(k)`,
/// `radial(k) = u cos(2 pi k / sides) + v sin(2 pi k / sides)`, with `(u, v)` the
/// basis of the plane from the axis `end - start` taken in that order: the same
/// axis vector and sign for the frustum's two rings, so a hull ring built here from
/// a frustum's own `start`/`end` equals that frustum's ring bit for bit.
///
/// `None` for fewer than three sides, a non-finite radius or a zero-length axis.
#[must_use]
pub fn frustum_ring(
    start: [f64; 3],
    end: [f64; 3],
    radius: f64,
    at_start: bool,
    plate_z: bool,
    sides: usize,
) -> Option<Vec<[f64; 3]>> {
    if sides < 3 || !radius.is_finite() {
        return None;
    }
    let axis = sub(end, start);
    let axis_length = norm(axis);
    if !axis_length.is_finite() || axis_length <= f64::EPSILON {
        return None;
    }
    let normal = if plate_z {
        [0.0, 0.0, 1.0]
    } else {
        scale(axis, 1.0 / axis_length)
    };
    let (u, v) = perpendicular_basis(normal)?;
    let centre = if at_start { start } else { end };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a ring corner index is below the side count, exact in f64"
    )]
    let ring = (0..sides)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / sides as f64;
            let radial = add(scale(u, angle.cos()), scale(v, angle.sin()));
            add(centre, scale(radial, radius))
        })
        .collect();
    Some(ring)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_counts_are_two(hull: &ConvexHull) -> bool {
        let mut edges: std::collections::HashMap<(usize, usize), usize> =
            std::collections::HashMap::new();
        for face in &hull.triangles {
            for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                *edges.entry((a, b)).or_default() += 1;
            }
        }
        edges
            .iter()
            .all(|(&(a, b), &count)| count == 1 && edges.get(&(b, a)) == Some(&1))
    }

    #[test]
    fn a_cube_with_interior_and_face_points_is_a_closed_unit_volume() {
        let mut points = Vec::new();
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 1.0] {
                    points.push([x, y, z]);
                }
            }
        }
        points.push([0.5, 0.5, 0.5]);
        points.push([0.5, 0.5, 0.0]);
        points.push([0.5, 1.0, 0.5]);
        let hull = convex_hull(&points).expect("a cube spans a solid");
        assert!(edge_counts_are_two(&hull), "closed 2-manifold");
        assert!((hull.volume() - 1.0).abs() < 1e-12, "{}", hull.volume());
        assert!(hull.vertices.len() >= 8);
    }

    #[test]
    fn two_parallel_polygons_keep_every_corner_and_the_volume_of_the_frustum() {
        let sides = 24;
        let bottom = frustum_ring([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, true, false, sides)
            .expect("ring");
        let top = frustum_ring([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, false, false, sides)
            .expect("ring");
        let points: Vec<[f64; 3]> = bottom.iter().chain(&top).copied().collect();
        let hull = convex_hull(&points).expect("hull");
        assert!(edge_counts_are_two(&hull));
        assert_eq!(hull.vertices.len(), 2 * sides, "every ring corner is a hull vertex");
        let polygon = |r: f64| 0.5 * sides as f64 * r * r * (std::f64::consts::TAU / sides as f64).sin();
        let expected = (polygon(1.0) + polygon(0.5) + (polygon(1.0) * polygon(0.5)).sqrt()) / 3.0;
        assert!((hull.volume() - expected).abs() < 1e-9, "{} vs {expected}", hull.volume());
    }

    #[test]
    fn a_flat_set_has_no_hull() {
        let flat: Vec<[f64; 3]> = (0..10)
            .map(|k| [f64::from(k), f64::from(k * k % 7), 1.0])
            .collect();
        assert!(convex_hull(&flat).is_none());
        assert!(convex_hull(&flat[..3]).is_none());
    }

    #[test]
    fn a_ring_made_from_a_frustums_own_ends_is_that_frustums_ring() {
        let start = [0.3, -0.2, 1.0];
        let end = [0.9, 0.4, 0.2];
        let a = frustum_ring(start, end, 0.7, false, false, 24).expect("ring");
        let b = frustum_ring(start, end, 0.7, false, false, 24).expect("ring");
        assert_eq!(a, b);
        let at_start = frustum_ring(start, end, 0.7, true, false, 24).expect("ring");
        assert_ne!(a, at_start);
        assert!(frustum_ring(start, start, 0.7, true, false, 24).is_none());
    }
}
