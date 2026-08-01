use crate::{
    add, cross, dot, norm_squared, scale, sub, validate_vec3, CoordinateKind, Mesh, MetricsError,
    TriangleSide, Vec3,
};

const EPSILON: f64 = 1.0e-9;

/// A ray accepted by the independent batch ray queries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// The nearest point on a triangle for one input point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosestPoint {
    pub point: Vec3,
    pub squared_distance: f64,
    pub triangle_index: usize,
}

/// A ray/triangle crossing, reported in sorted distance order per input ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub distance: f64,
    pub triangle_index: usize,
}

/// Computes the closest point for every query against every triangle.
///
/// This is serial O(points × triangles) by design.  It retains stable batch
/// ordering and breaks equal-distance ties by the lowest triangle index.
pub fn closest_points(mesh: &Mesh, points: &[Vec3]) -> Result<Vec<ClosestPoint>, MetricsError> {
    for (point, value) in points.iter().enumerate() {
        validate_vec3(*value, CoordinateKind::Point { point })?;
    }

    let mut result = Vec::with_capacity(points.len());
    for &point in points {
        let mut best = ClosestPoint {
            point: [0.0; 3],
            squared_distance: f64::INFINITY,
            triangle_index: 0,
        };
        for triangle_index in 0..mesh.triangles().len() {
            let (a, b, c) = mesh.triangle_points(triangle_index);
            let candidate = closest_point_on_triangle(point, a, b, c);
            let distance = norm_squared(sub(point, candidate));
            if distance < best.squared_distance {
                best = ClosestPoint {
                    point: candidate,
                    squared_distance: distance,
                    triangle_index,
                };
            }
        }
        result.push(best);
    }
    Ok(result)
}

/// Computes all positive ray crossings for every ray, preserving input order.
pub fn ray_intersections(mesh: &Mesh, rays: &[Ray]) -> Result<Vec<Vec<RayHit>>, MetricsError> {
    for (ray, value) in rays.iter().enumerate() {
        validate_vec3(value.origin, CoordinateKind::RayOrigin { ray })?;
        validate_vec3(value.direction, CoordinateKind::RayDirection { ray })?;
        // Do not use a squared norm here: a nonzero subnormal direction can
        // legitimately underflow to zero when squared.
        if value.direction.iter().all(|component| *component == 0.0) {
            return Err(MetricsError::ZeroRayDirection { ray });
        }
    }

    let mut output = Vec::with_capacity(rays.len());
    for ray in rays {
        let mut hits = Vec::new();
        for triangle_index in 0..mesh.triangles().len() {
            let (a, b, c) = mesh.triangle_points(triangle_index);
            if let Some(distance) = ray_triangle_distance(*ray, a, b, c) {
                hits.push(RayHit {
                    distance,
                    triangle_index,
                });
            }
        }
        hits.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then(left.triangle_index.cmp(&right.triangle_index))
        });
        output.push(hits);
    }
    Ok(output)
}

/// Classifies a batch with three independently oriented rays per point.
///
/// A point lying on the input surface is classified inside.  Other points use
/// a majority vote across non-axis-aligned directions, avoiding the historical
/// nearest-face-normal sign failure on thin triangle soups.
pub fn contains_points(mesh: &Mesh, points: &[Vec3]) -> Result<Vec<bool>, MetricsError> {
    let nearest = closest_points(mesh, points)?;
    let directions = [
        [1.0, 0.371, 0.193],
        [0.293, 1.0, 0.419],
        [0.157, 0.347, 1.0],
    ];
    let mut rays = Vec::with_capacity(points.len() * directions.len());
    for &point in points {
        for direction in directions {
            rays.push(Ray {
                origin: point,
                direction,
            });
        }
    }
    let crossings = ray_intersections(mesh, &rays)?;
    let mut result = Vec::with_capacity(points.len());
    for (index, closest) in nearest.iter().enumerate() {
        if closest.squared_distance <= EPSILON * EPSILON {
            result.push(true);
            continue;
        }
        let votes = crossings[index * directions.len()..(index + 1) * directions.len()]
            .iter()
            .filter(|hits| deduplicated_crossings(hits) % 2 == 1)
            .count();
        result.push(votes >= 2);
    }
    Ok(result)
}

/// Tests every requested triangle pair for intersection, including coplanar overlap.
///
/// The input and output positions correspond exactly; invalid pair indices are
/// reported as typed semantic errors rather than silently becoming a false hit.
pub fn triangle_intersections(
    mesh: &Mesh,
    pairs: &[(usize, usize)],
) -> Result<Vec<bool>, MetricsError> {
    for (query, &(left, right)) in pairs.iter().enumerate() {
        for (side, triangle) in [(TriangleSide::Left, left), (TriangleSide::Right, right)] {
            if triangle >= mesh.triangles().len() {
                return Err(MetricsError::QueryTriangleIndexOutOfBounds {
                    query,
                    side,
                    triangle,
                    triangle_count: mesh.triangles().len(),
                });
            }
        }
    }
    Ok(pairs
        .iter()
        .map(|&(left, right)| {
            triangles_intersect_points(mesh.triangle_points(left), mesh.triangle_points(right))
        })
        .collect())
}

fn closest_point_on_triangle(point: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    // Ericson's region tests.  The caller has already rejected zero-area faces.
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(point, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(point, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(point, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denominator = 1.0 / (va + vb + vc);
    add(
        a,
        add(scale(ab, vb * denominator), scale(ac, vc * denominator)),
    )
}

fn ray_triangle_distance(ray: Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let edge_one = sub(b, a);
    let edge_two = sub(c, a);
    let p = cross(ray.direction, edge_two);
    let determinant = dot(edge_one, p);
    if determinant.abs() <= EPSILON {
        return None;
    }
    let inverse = 1.0 / determinant;
    let from_vertex = sub(ray.origin, a);
    let u = dot(from_vertex, p) * inverse;
    if !(-EPSILON..=1.0 + EPSILON).contains(&u) {
        return None;
    }
    let q = cross(from_vertex, edge_one);
    let v = dot(ray.direction, q) * inverse;
    if v < -EPSILON || u + v > 1.0 + EPSILON {
        return None;
    }
    let distance = dot(edge_two, q) * inverse;
    if distance < -EPSILON {
        None
    } else {
        Some(distance.max(0.0))
    }
}

fn deduplicated_crossings(hits: &[RayHit]) -> usize {
    let mut count = 0;
    let mut previous = None;
    for hit in hits {
        if hit.distance <= EPSILON {
            continue;
        }
        if previous.is_none_or(|value: f64| (hit.distance - value).abs() > EPSILON) {
            count += 1;
            previous = Some(hit.distance);
        }
    }
    count
}

fn triangles_intersect_points(first: (Vec3, Vec3, Vec3), second: (Vec3, Vec3, Vec3)) -> bool {
    let (a, b, c) = first;
    let (d, e, f) = second;
    if !aabb_overlaps([a, b, c], [d, e, f]) {
        return false;
    }
    for (start, end) in [(a, b), (b, c), (c, a)] {
        if segment_intersects_triangle_exact(start, end, d, e, f) {
            return true;
        }
    }
    for (start, end) in [(d, e), (e, f), (f, d)] {
        if segment_intersects_triangle_exact(start, end, a, b, c) {
            return true;
        }
    }
    let first_normal = scaled_normal(a, b, c);
    let second_normal = scaled_normal(d, e, f);
    // Triangle-pair intersection has exact f64 input semantics: unlike the
    // parity ray path, it must not turn a separated near-coplanar pair into a
    // hit through an arbitrary absolute tolerance band.
    if cross(first_normal, second_normal) == [0.0; 3] && dot(first_normal, sub(d, a)) == 0.0 {
        return coplanar_overlap([a, b, c], [d, e, f], first_normal);
    }
    false
}

fn scaled_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let first = sub(b, a);
    let second = sub(c, a);
    let scale = first
        .into_iter()
        .chain(second)
        .map(f64::abs)
        .fold(0.0, f64::max);
    // Mesh construction has already rejected coincident and degenerate faces.
    cross(
        [first[0] / scale, first[1] / scale, first[2] / scale],
        [second[0] / scale, second[1] / scale, second[2] / scale],
    )
}

fn aabb_overlaps(left: [Vec3; 3], right: [Vec3; 3]) -> bool {
    (0..3).all(|axis| {
        let left_min = left
            .iter()
            .map(|point| point[axis])
            .fold(f64::INFINITY, f64::min);
        let left_max = left
            .iter()
            .map(|point| point[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let right_min = right
            .iter()
            .map(|point| point[axis])
            .fold(f64::INFINITY, f64::min);
        let right_max = right
            .iter()
            .map(|point| point[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        left_min <= right_max && right_min <= left_max
    })
}

fn coplanar_overlap(left: [Vec3; 3], right: [Vec3; 3], normal: Vec3) -> bool {
    let drop = normal
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
        .map_or(2, |(index, _)| index);
    let left_2d = left.map(|point| project(point, drop));
    let right_2d = right.map(|point| project(point, drop));
    for (start, end) in edges_2d(left_2d) {
        for (other_start, other_end) in edges_2d(right_2d) {
            if segments_intersect_2d(start, end, other_start, other_end) {
                return true;
            }
        }
    }
    point_in_triangle_2d(left_2d[0], right_2d) || point_in_triangle_2d(right_2d[0], left_2d)
}

fn project(point: Vec3, drop: usize) -> [f64; 2] {
    match drop {
        0 => [point[1], point[2]],
        1 => [point[0], point[2]],
        _ => [point[0], point[1]],
    }
}

fn edges_2d(points: [[f64; 2]; 3]) -> [([f64; 2], [f64; 2]); 3] {
    [
        (points[0], points[1]),
        (points[1], points[2]),
        (points[2], points[0]),
    ]
}

fn orientation_2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn segments_intersect_2d(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let ab_c = orientation_2d(a, b, c);
    let ab_d = orientation_2d(a, b, d);
    let cd_a = orientation_2d(c, d, a);
    let cd_b = orientation_2d(c, d, b);
    if ab_c == 0.0 && on_segment_2d(a, b, c)
        || ab_d == 0.0 && on_segment_2d(a, b, d)
        || cd_a == 0.0 && on_segment_2d(c, d, a)
        || cd_b == 0.0 && on_segment_2d(c, d, b)
    {
        return true;
    }
    (ab_c > 0.0) != (ab_d > 0.0) && (cd_a > 0.0) != (cd_b > 0.0)
}

fn on_segment_2d(a: [f64; 2], b: [f64; 2], point: [f64; 2]) -> bool {
    point[0] >= a[0].min(b[0])
        && point[0] <= a[0].max(b[0])
        && point[1] >= a[1].min(b[1])
        && point[1] <= a[1].max(b[1])
}

fn point_in_triangle_2d(point: [f64; 2], triangle: [[f64; 2]; 3]) -> bool {
    let values = [
        orientation_2d(triangle[0], triangle[1], point),
        orientation_2d(triangle[1], triangle[2], point),
        orientation_2d(triangle[2], triangle[0], point),
    ];
    values.iter().all(|value| *value >= 0.0) || values.iter().all(|value| *value <= 0.0)
}

fn segment_intersects_triangle_exact(start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let direction = sub(end, start);
    let edge_one = sub(b, a);
    let edge_two = sub(c, a);
    let p = cross(direction, edge_two);
    let determinant = dot(edge_one, p);
    if determinant == 0.0 {
        return false;
    }
    let inverse = 1.0 / determinant;
    let from_vertex = sub(start, a);
    let u = dot(from_vertex, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(from_vertex, edge_one);
    let v = dot(direction, q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let distance = dot(edge_two, q) * inverse;
    (0.0..=1.0).contains(&distance)
}
