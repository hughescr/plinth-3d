//! Fixed-size column proposals. Only `construct::probe_members` can admit these
//! paths with a section, native geometry and rooted carrier certificate.

/// Free length above which the owner's Light rule calls for bracing, mm **[O]**.
pub const BRACE_FREE_LENGTH_MM: f64 = 4.0;
/// Smallest column radius, mm **[O]**.
pub const MIN_COLUMN_RADIUS_MM: f64 = 0.25;
/// Plain routing request, not an admitted member.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnRequest {
    /// Joint centre in local coordinates, mm.
    pub joint: [f32; 3],
    /// Column radius, mm.
    pub radius: f32,
}
/// Plain path data. Native local plate height must be supplied explicitly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    /// Joint centre.
    pub top: [f32; 3],
    /// Flare overlap centre.
    pub bottom: [f32; 3],
    /// Radius, mm.
    pub radius: f32,
}
/// No legal fixed-size path can be proposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    /// Invalid coordinate or radius.
    Invalid,
    /// Joint below the flare.
    TooShort,
    /// Legacy entrypoint has no plate coordinate.
    NotBuilt,
}
/// Legacy stub cannot infer a local plate height. Use `route_columns_at_plate`.
pub fn route_columns(requests: &[ColumnRequest], out: &mut Vec<Result<Column, RouteError>>) {
    out.clear();
    out.extend(requests.iter().map(|_| Err(RouteError::NotBuilt)));
}
/// Route all columns at one known local plate height. This is a proposal only; the
/// construction API still checks full-solid clearance and joints before admission.
pub fn route_columns_at_plate(
    requests: &[ColumnRequest],
    plate: f32,
    out: &mut Vec<Result<Column, RouteError>>,
) {
    out.clear();
    out.extend(requests.iter().map(|r| {
        if !plate.is_finite()
            || r.joint.iter().any(|v| !v.is_finite())
            || !r.radius.is_finite()
            || r.radius < 0.25
        {
            return Err(RouteError::Invalid);
        }
        let bottom = [r.joint[0], r.joint[1], plate + 1.196];
        if r.joint[2] <= bottom[2] + 0.05 {
            return Err(RouteError::TooShort);
        }
        Ok(Column {
            top: r.joint,
            bottom,
            radius: r.radius,
        })
    }));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_plate_is_not_inferred_from_zero() {
        let request = ColumnRequest {
            joint: [0., 0., 0.],
            radius: 0.25,
        };
        let mut out = Vec::new();
        route_columns_at_plate(&[request], -5., &mut out);
        assert!((out[0].unwrap().bottom[2] + 3.804).abs() < 1e-6);
    }
}
