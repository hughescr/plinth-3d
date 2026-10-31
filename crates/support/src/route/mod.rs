//! Columns, bracing and bases (lane A; design `step11-construction.md` section 5.7). Step 0
//! holds the signatures only.

/// A column with more free length than this gets braces, mm (the owner's Light profile).
pub const BRACE_FREE_LENGTH_MM: f64 = 4.0;
/// The smallest column radius, mm (the pillar radius of the owner's profiles).
pub const MIN_COLUMN_RADIUS_MM: f64 = 0.25;

/// A request for one column from a joint down to the plate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnRequest {
    /// Joint ball centre, mm.
    pub joint: [f32; 3],
    /// Radius the column must have to carry its load, mm.
    pub radius: f32,
}

/// One routed column: its two end centres and radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    /// Top end (the joint centre), mm.
    pub top: [f32; 3],
    /// Bottom end (above the flare), mm.
    pub bottom: [f32; 3],
    /// Radius, mm.
    pub radius: f32,
}

/// Why a column could not be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    /// Routing is not built yet (step 0 stub).
    NotBuilt,
}

/// Route one column per request, in one batch. `out` is cleared and refilled with exactly one
/// answer per request.
pub fn route_columns(requests: &[ColumnRequest], out: &mut Vec<Result<Column, RouteError>>) {
    out.clear();
    out.extend(requests.iter().map(|_| Err(RouteError::NotBuilt)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stub_routes_nothing() {
        let r = ColumnRequest {
            joint: [0.0; 3],
            radius: 0.25,
        };
        let mut out = Vec::new();
        route_columns(&[r, r, r], &mut out);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|c| *c == Err(RouteError::NotBuilt)));
    }
}
