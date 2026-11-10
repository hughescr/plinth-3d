//! Raft-only H9 admission on the written prism union, not on incidental pairs.
//! geo uses i_overlay's integer boolean kernel after a bounded float conversion.

use super::{NativePart, PartKind, Reject};
use geo::algorithm::bool_ops::unary_union;
use geo::algorithm::buffer::{BufferStyle, LineJoin};
use geo::{Area, BooleanOps, BoundingRect, Buffer, ConvexHull, MultiPoint, Point, Polygon};

// H9 = 0.1 mm. Reserve 0.00025 mm on each erosion radius for quantization,
// offset arithmetic and arc chords. For spans <= 1024 mm, i_overlay's 29-bit
// adapter grid is <= 2^-19 mm. Round-join chords at 0.05 rad lose < 0.000016 mm
// of radius. The reserve exceeds these errors, rather than accepting a 0.1-mm
// neck on numerical luck. This checks topology/interior survival, not all local
// boundary widths (tips/edges are excluded from H9).
const EROSION: f64 = 0.05025;
const MAX_SPAN: f64 = 1024.0;

fn erode(polygon: &Polygon) -> geo::MultiPolygon {
    polygon.buffer_with_style(BufferStyle::new(-EROSION).line_join(LineJoin::Round(0.05)))
}

fn reject(polygon: &Polygon, reason: &'static str) -> Reject {
    let bounds = polygon
        .bounding_rect()
        .map_or([0.; 4], |r| [r.min().x, r.min().y, r.max().x, r.max().y]);
    Reject::RaftRegionUnverified { bounds, reason }
}

/// Admit only common-height convex prisms. Their hull is the exact projection
/// of their written triangles, not an analytic circle or nominal rectangle.
pub(super) fn certify(parts: &[NativePart]) -> Result<(), Reject> {
    let mut polygons = Vec::with_capacity(parts.len());
    let mut heights = None;
    for part in parts {
        if !matches!(part.kind(), PartKind::RaftPad | PartKind::RaftBridge) {
            return Err(Reject::UnknownShape);
        }
        let points: Vec<_> = part
            .mesh()
            .iter()
            .flatten()
            .map(|v| Point::new(f64::from(v[0]), f64::from(v[1])))
            .collect();
        let polygon = MultiPoint::new(points).convex_hull();
        let lo = part
            .mesh()
            .iter()
            .flatten()
            .map(|v| v[2])
            .min_by(f32::total_cmp)
            .ok_or(Reject::UnknownShape)?;
        let hi = part
            .mesh()
            .iter()
            .flatten()
            .map(|v| v[2])
            .max_by(f32::total_cmp)
            .ok_or(Reject::UnknownShape)?;
        let range = [lo.to_bits(), hi.to_bits()];
        if heights.is_some_and(|z| z != range)
            || f64::from(hi) - f64::from(lo) < 0.102
            || part.mesh().iter().flatten().any(|v| {
                v.iter().any(|c| !c.is_finite() || c.abs() > 1.0e6)
                    || (v[2].to_bits() != range[0] && v[2].to_bits() != range[1])
            })
        {
            return Err(reject(&polygon, "raft is not a common-height finite prism"));
        }
        heights = Some(range);
        polygons.push(polygon);
    }
    certify_polygons(&polygons)
}

fn certify_polygons(polygons: &[Polygon]) -> Result<(), Reject> {
    if polygons.is_empty() {
        return Err(Reject::NotBuilt);
    }
    // One batched boolean union, not a growing per-part union hot path.
    let union = unary_union(polygons);
    let bounds = union.bounding_rect().ok_or(Reject::UnknownShape)?;
    if bounds.width().max(bounds.height()) > MAX_SPAN {
        return Err(reject(
            &polygons[0],
            "raft exceeds erosion precision domain",
        ));
    }
    if union.0.is_empty() {
        return Err(reject(&polygons[0], "empty raft union"));
    }
    for component in &union.0 {
        let interior = erode(component);
        if interior.0.len() != 1 || interior.unsigned_area() <= 0.0 {
            return Err(reject(
                component,
                "raft component vanishes or splits after 0.05025 mm erosion",
            ));
        }
    }
    for polygon in polygons {
        let interior = erode(polygon);
        if interior.0.is_empty()
            || interior.unsigned_area() <= 0.0
            || interior.intersection(&union).unsigned_area() <= 0.0
        {
            return Err(reject(
                polygon,
                "raft part has no surviving eroded interior",
            ));
        }
    }
    // Connected raft construction also promises exactly one material component.
    if union.0.len() != 1 {
        return Err(reject(&union.0[0], "raft union has multiple components"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::polygon;

    #[test]
    fn collinear_native_strips_connect_through_their_shared_pad() {
        let parts = [
            NativePart::pad([0., 0.], 0.2),
            NativePart::pad([2.3, 0.], 0.2),
            NativePart::pad([4.6, 0.], 0.2),
            NativePart::bridge([0., 0.], [2.3, 0.], 0.2),
            NativePart::bridge([2.3, 0.], [4.6, 0.], 0.2),
        ];
        assert!(certify(&parts).is_ok());
    }

    #[test]
    fn narrow_neck_splits_the_union_and_is_refused() {
        let lobes = [
            polygon![(x: -2., y: -1.), (x: 0., y: -1.), (x: 0., y: 1.), (x: -2., y: 1.)],
            polygon![(x: 2., y: -1.), (x: 4., y: -1.), (x: 4., y: 1.), (x: 2., y: 1.)],
        ];
        for (width, pass) in [(0.08, false), (0.1, false), (0.104, true), (2.2, true)] {
            let strip = polygon![(x: -1., y: -width / 2.), (x: 3., y: -width / 2.), (x: 3., y: width / 2.), (x: -1., y: width / 2.)];
            let result = certify_polygons(&[lobes[0].clone(), lobes[1].clone(), strip]);
            assert_eq!(result.is_ok(), pass, "width {width}: {result:?}");
        }
    }

    #[test]
    fn thin_dangling_part_cannot_disappear_silently() {
        let parts = [
            polygon![(x: -1., y: -1.), (x: 1., y: -1.), (x: 1., y: 1.), (x: -1., y: 1.)],
            polygon![(x: 0., y: -0.04), (x: 3., y: -0.04), (x: 3., y: 0.04), (x: 0., y: 0.04)],
        ];
        assert!(matches!(
            certify_polygons(&parts),
            Err(Reject::RaftRegionUnverified {
                reason: "raft part has no surviving eroded interior",
                ..
            })
        ));
    }
}
