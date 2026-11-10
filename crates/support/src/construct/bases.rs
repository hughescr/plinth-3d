//! Plate flare admission and raft-only union certificates. No geometry is repaired.

use super::native::{ENVELOPE, cross, dot, sub};
use super::{NativePart, PartFields, PartKind, Reject};
use plinth_geom::patch::plane_distance_bounds;

const JOIN_RADIUS: f64 = 0.052; // 0.104 mm diameter, above the 0.102 mm build width.

/// A ball inside every inward face half-space of a native convex part is inside
/// its solid. Test actual written faces, not just field radii.
fn contains_ball(part: &NativePart, centre: [f64; 3], interior: [f64; 3]) -> bool {
    part.mesh().iter().all(|t| {
        let tri = t.map(|v| v.map(f64::from));
        let normal = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
        let inside = dot(sub(interior, tri[0]), normal);
        let point = dot(sub(centre, tri[0]), normal);
        inside * point > 0.0
            && plane_distance_bounds(centre, tri).is_some_and(|d| d.lo >= JOIN_RADIUS + ENVELOPE)
    })
}

/// Native flat-ended strip, with end planes through pad centres. Shortening used
/// to evade pairwise corner/flare checks; the actual raft union is now certified
/// instead. Endpoint overlap is still certified before admission.
pub(super) fn bridge(a: [f32; 2], b: [f32; 2], top: f32) -> Result<NativePart, Reject> {
    let d = (f64::from(a[0]) - f64::from(b[0])).hypot(f64::from(a[1]) - f64::from(b[1]));
    if d <= 0.0 {
        return Err(Reject::NarrowJoin);
    }
    let axis: [f64; 2] = std::array::from_fn(|k| (f64::from(b[k]) - f64::from(a[k])) / d);
    let bridge = NativePart::bridge(a, b, top);
    for (pad, sign) in [(a, 1.), (b, -1.)] {
        let witness = [
            f64::from(pad[0]) + sign * (JOIN_RADIUS + 2.0 * ENVELOPE) * axis[0],
            f64::from(pad[1]) + sign * (JOIN_RADIUS + 2.0 * ENVELOPE) * axis[1],
            f64::from(top) - 0.1,
        ];
        if !contains_ball(
            &NativePart::pad(pad, top),
            witness,
            [f64::from(pad[0]), f64::from(pad[1]), f64::from(top) - 0.1],
        ) || !contains_ball(
            &bridge,
            witness,
            [
                (f64::from(a[0]) + f64::from(b[0])) * 0.5,
                (f64::from(a[1]) + f64::from(b[1])) * 0.5,
                f64::from(top) - 0.1,
            ],
        ) {
            return Err(Reject::NarrowJoin);
        }
    }
    Ok(bridge)
}

struct Convex<'a> {
    part: &'a NativePart,
    lo: [f64; 3],
    hi: [f64; 3],
    inside: [f64; 3],
}
impl<'a> Convex<'a> {
    fn new(part: &'a NativePart) -> Self {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in part.mesh().iter().flatten() {
            for k in 0..3 {
                lo[k] = lo[k].min(f64::from(v[k]));
                hi[k] = hi[k].max(f64::from(v[k]));
            }
        }
        Self {
            part,
            lo,
            hi,
            inside: std::array::from_fn(|k| (lo[k] + hi[k]) * 0.5),
        }
    }
    // A flare and its pad straddle one horizontal cut. Each covers its half of
    // the witness ball: ignore ONLY the flare bottom and pad top, requiring their
    // actual cap planes to overlap across the cut. Other faces contain the ball.
    fn covers_half(&self, p: [f64; 3], upper: bool) -> bool {
        let cap = if upper { self.lo[2] } else { self.hi[2] };
        if (upper && cap > p[2]) || (!upper && cap < p[2]) {
            return false;
        }
        self.part.mesh().iter().all(|t| {
            if t.iter().all(|v| f64::from(v[2]).to_bits() == cap.to_bits()) {
                return true;
            }
            let tri = t.map(|v| v.map(f64::from));
            let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
            dot(sub(self.inside, tri[0]), n) * dot(sub(p, tri[0]), n) > 0.0
                && plane_distance_bounds(p, tri).is_some_and(|d| d.lo >= JOIN_RADIUS + ENVELOPE)
        })
    }
}

/// Certify the raft union and each flare's own pad, before changing the builder.
/// A strip touching a flare is already joined through this certified pad/union;
/// incidental strip/flare pairs are not independent load-bearing joins.
pub(super) fn admit_bridges(raft: &[NativePart], members: &[NativePart]) -> Result<(), Reject> {
    super::raft_union::certify(raft)?;
    let pads: std::collections::BTreeMap<_, _> = raft
        .iter()
        .filter_map(|p| {
            if let PartFields::Pad { centre, .. } = p.fields() {
                Some((centre.map(f32::to_bits), p))
            } else {
                None
            }
        })
        .collect();
    for flare in members.iter().filter(|p| p.kind() == PartKind::BaseFlare) {
        let PartFields::Frustum { end, .. } = flare.fields() else {
            return Err(Reject::UnknownShape);
        };
        let error = || Reject::RaftRegionUnverified {
            bounds: [
                f64::from(end[0]) - 1.1,
                f64::from(end[1]) - 1.1,
                f64::from(end[0]) + 1.1,
                f64::from(end[1]) + 1.1,
            ],
            reason: "flare has no certified half-ball join to its own pad",
        };
        let pad = pads
            .get(&[end[0].to_bits(), end[1].to_bits()])
            .ok_or_else(error)?;
        let pad = Convex::new(pad);
        let point = [f64::from(end[0]), f64::from(end[1]), pad.hi[2]];
        if !Convex::new(flare).covers_half(point, true) || !pad.covers_half(point, false) {
            return Err(error());
        }
    }
    Ok(())
}

/// Either prove the complete flares AND their pads disjoint in plan view, or
/// exhibit a common 0.104 mm ball inside both flares and another inside both pads.
/// This non-raft member admission certificate is unchanged.
pub(super) fn compatible(a: &NativePart, b: &NativePart) -> Result<(), Reject> {
    let fields = |p: &NativePart| match p.fields() {
        PartFields::Frustum { start, end, r1, r2 }
            if start[0].to_bits() == end[0].to_bits()
                && start[1].to_bits() == end[1].to_bits()
                && start[2] > end[2] =>
        {
            Ok((
                start.map(f64::from),
                end.map(f64::from),
                f64::from(r1.max(r2)),
            ))
        }
        _ => Err(Reject::UnknownShape),
    };
    let (at, ab, ar) = fields(a)?;
    let (bt, bb, br) = fields(b)?;
    if ab[2].to_bits() != bb[2].to_bits() || at[2].to_bits() != bt[2].to_bits() {
        return Err(Reject::NarrowJoin);
    }
    let distance = (at[0] - bt[0]).hypot(at[1] - bt[1]);
    if distance > ar.max(1.1) + br.max(1.1) + 2.0 * ENVELOPE {
        return Ok(());
    }
    let mid = [(at[0] + bt[0]) * 0.5, (at[1] + bt[1]) * 0.5];
    let witness = [mid[0], mid[1], ab[2] + JOIN_RADIUS + 2.0 * ENVELOPE];
    let interior = |top: [f64; 3], bottom: [f64; 3]| [top[0], top[1], (top[2] + bottom[2]) * 0.5];
    if !contains_ball(a, witness, interior(at, ab)) || !contains_ball(b, witness, interior(bt, bb))
    {
        return Err(Reject::NarrowJoin);
    }
    let pads = [
        NativePart::pad([at[0] as f32, at[1] as f32], 0.2),
        NativePart::pad([bt[0] as f32, bt[1] as f32], 0.2),
    ];
    let witness = [mid[0], mid[1], 0.1];
    if pads
        .iter()
        .zip([at, bt])
        .all(|(pad, top)| contains_ball(pad, witness, [top[0], top[1], 0.1]))
    {
        Ok(())
    } else {
        Err(Reject::NarrowJoin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::Section;

    fn flare(xy: [f32; 2]) -> NativePart {
        NativePart::frustum(
            PartKind::BaseFlare,
            [xy[0], xy[1], 1.198],
            [xy[0], xy[1], 0.198],
            Section::new(0.585).unwrap(),
            Section::new(1.1).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn native_corner_connects_through_the_certified_pad_union() {
        let a = [-8.658361, 4.8202653];
        let b = [-10.810063, 4.3419685];
        let c = [-11.480255, 2.8054507];
        assert!(
            admit_bridges(
                &[
                    NativePart::pad(a, 0.2),
                    NativePart::pad(b, 0.2),
                    NativePart::pad(c, 0.2),
                    bridge(a, b, 0.2).unwrap()
                ],
                &[flare(c)]
            )
            .is_ok()
        );
        assert!(matches!(
            admit_bridges(&[NativePart::pad(a, 0.2)], &[flare(c)]),
            Err(Reject::RaftRegionUnverified { .. })
        ));
    }

    #[test]
    fn every_bridge_end_plane_passes_through_its_pad_centre() {
        for (a, b) in [
            ([0., 0.], [0.4, 0.]),
            ([0., 0.], [1.9, 0.]),
            ([0., 0.], [2.21, 0.]),
            ([0., 0.], [4., 0.]),
            ([0., 0.], [15., 0.]),
            ([-8.658361, 4.8202653], [-10.810063, 4.3419685]),
            ([2., -3.], [8., 5.]),
        ] {
            let strip = bridge(a, b, 0.2).unwrap();
            let PartFields::Bridge { from, to, .. } = strip.fields() else {
                unreachable!()
            };
            assert_eq!(from.map(f32::to_bits), a.map(f32::to_bits));
            assert_eq!(to.map(f32::to_bits), b.map(f32::to_bits));
            let delta = [f64::from(b[0] - a[0]), f64::from(b[1] - a[1])];
            let length = delta[0].hypot(delta[1]);
            let axis = delta.map(|d| d / length);
            for (centre, expected) in [(a, 0.0), (b, -length)] {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for v in strip.mesh().iter().flatten() {
                    let p = (f64::from(v[0]) - f64::from(centre[0])) * axis[0]
                        + (f64::from(v[1]) - f64::from(centre[1])) * axis[1];
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                assert!((lo - expected).abs() < 3e-6, "start plane: {lo}");
                assert!((hi - (expected + length)).abs() < 3e-6, "end plane: {hi}");
            }
            assert!(
                admit_bridges(
                    &[NativePart::pad(a, 0.2), NativePart::pad(b, 0.2), strip],
                    &[]
                )
                .is_ok()
            );
        }
    }
}
