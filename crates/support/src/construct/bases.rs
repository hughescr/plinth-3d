//! Plate bases may merge, but not through a tangent or a thin lens. This guard is
//! part of column admission (including private alternatives), before selection.

use super::native::{ENVELOPE, cross, dot, sub};
use super::{NativePart, PartFields, PartKind, Reject};
use plinth_geom::patch::plane_distance_bounds;

const JOIN_RADIUS: f64 = 0.052; // 0.104 mm diameter, above the 0.102 mm build width.

/// A ball inside every inward face half-space of a native convex part is inside
/// its solid. Test actual written faces, not just field radii. The interior axis
/// point fixes the sign independently of triangle winding; the plane-distance
/// interval includes arithmetic error, and ENVELOPE leaves room for native rounding.
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

/// Build the native flat-ended box only across the pad gap, with 0.3 mm of
/// centreline overlap into each radius-1.1 pad. The complete box still has its
/// native 2.2 mm width. Extending a box all the way to a pad centre is unnecessary
/// and its far corners can graze a third base. Certify both endpoint joins from
/// the written meshes, before this bridge enters the raft.
pub(super) fn bridge(a: [f32; 2], b: [f32; 2], top: f32) -> Result<NativePart, Reject> {
    let d = (f64::from(a[0]) - f64::from(b[0])).hypot(f64::from(a[1]) - f64::from(b[1]));
    if d <= 2.2 + 2.0 * ENVELOPE {
        return Err(Reject::NarrowJoin);
    }
    let axis: [f64; 2] = std::array::from_fn(|k| (f64::from(b[k]) - f64::from(a[k])) / d);
    let start = std::array::from_fn(|k| (f64::from(a[k]) + 0.8 * axis[k]) as f32);
    let end = std::array::from_fn(|k| (f64::from(b[k]) - 0.8 * axis[k]) as f32);
    let bridge = NativePart::bridge(start, end, top);
    for (pad, centre, sign) in [(a, start, 1.), (b, end, -1.)] {
        let witness = [
            f64::from(centre[0]) + sign * (JOIN_RADIUS + 2.0 * ENVELOPE) * axis[0],
            f64::from(centre[1]) + sign * (JOIN_RADIUS + 2.0 * ENVELOPE) * axis[1],
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
                (f64::from(start[0]) + f64::from(end[0])) * 0.5,
                (f64::from(start[1]) + f64::from(end[1])) * 0.5,
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
    fn apart(&self, other: &Self) -> bool {
        // A separating face is sufficient (not necessary). Unresolved edge-edge
        // separations are refused, never treated as permission for a thin join.
        if (0..3).any(|k| {
            self.hi[k] + 2.0 * ENVELOPE < other.lo[k] || other.hi[k] + 2.0 * ENVELOPE < self.lo[k]
        }) {
            return true;
        }
        // Clip vertices/edges to the COMMON height interval before testing side
        // faces. A flare's extended side plane below its bottom is not material;
        // un-clipped raft vertices there must not hide a real plan-view gap.
        let lo = self.lo[2].max(other.lo[2]);
        let hi = self.hi[2].min(other.hi[2]);
        let av = self.vertices_in_z(lo, hi);
        let bv = other.vertices_in_z(lo, hi);
        [(self, &bv), (other, &av)]
            .into_iter()
            .any(|(a, vertices)| {
                !vertices.is_empty()
                    && a.part.mesh().iter().any(|t| {
                        let tri = t.map(|v| v.map(f64::from));
                        let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
                        let sign = dot(sub(a.inside, tri[0]), n);
                        vertices.iter().all(|&p| {
                            sign * dot(sub(p, tri[0]), n) < 0.0
                                && plane_distance_bounds(p, tri)
                                    .is_some_and(|d| d.lo > 2.0 * ENVELOPE)
                        })
                    })
            })
    }
    fn vertices_in_z(&self, lo: f64, hi: f64) -> Vec<[f64; 3]> {
        let mut vertices = Vec::new();
        for tri in self.part.mesh() {
            let tri = tri.map(|v| v.map(f64::from));
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                if a[2] >= lo && a[2] <= hi {
                    vertices.push(a);
                }
                for z in [lo, hi] {
                    if (a[2] < z && b[2] > z) || (a[2] > z && b[2] < z) {
                        let t = (z - a[2]) / (b[2] - a[2]);
                        vertices.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), z]);
                    }
                }
            }
        }
        vertices
    }
    fn contains(&self, p: [f64; 3]) -> bool {
        contains_ball(self.part, p, self.inside)
    }
    // A flare and raft straddle one horizontal cut. Each covers its half of
    // the witness ball: ignore ONLY the flare's bottom and the raft's top, and
    // require their actual cap planes to overlap across the cut. Every other
    // face must contain the complete ball. This proves a union ball even though
    // the native vertical overlap is only 0.002 mm, not a full ball diameter.
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

/// Admit all incidental bridge joins as one batch, before changing the builder.
/// Each is separated by a plane, contains a common ball, or is a certified
/// complementary-half-ball flare/raft union. Unknown is a rejection. No repair.
pub(super) fn admit_bridges(raft: &[NativePart], members: &[NativePart]) -> Result<(), Reject> {
    let parts: Vec<_> = raft
        .iter()
        .chain(members.iter().filter(|p| p.kind() == PartKind::BaseFlare))
        .map(Convex::new)
        .collect();
    for (i, a) in parts.iter().enumerate() {
        for b in &parts[..i] {
            if a.part.kind() != PartKind::RaftBridge && b.part.kind() != PartKind::RaftBridge {
                continue; // Pad/flare pairs were already certified at column admission.
            }
            if a.apart(b) {
                continue;
            }
            let lo: [f64; 3] = std::array::from_fn(|k| a.lo[k].max(b.lo[k]));
            let hi: [f64; 3] = std::array::from_fn(|k| a.hi[k].min(b.hi[k]));
            let (flare, raft) = if a.part.kind() == PartKind::BaseFlare {
                (a, b)
            } else {
                (b, a)
            };
            let mut wide = false;
            // Search witnesses only; the face inequalities, not this resolution,
            // establish width. A missed witness fails closed.
            for x in [0.25, 0.5, 0.75] {
                for y in [0.25, 0.5, 0.75] {
                    let p = [
                        lo[0] + x * (hi[0] - lo[0]),
                        lo[1] + y * (hi[1] - lo[1]),
                        (lo[2] + hi[2]) * 0.5,
                    ];
                    wide |= (a.contains(p) && b.contains(p))
                        || (flare.part.kind() == PartKind::BaseFlare
                            && flare.covers_half(p, true)
                            && raft.covers_half(p, false));
                    if wide {
                        break;
                    }
                }
                if wide {
                    break;
                }
            }
            if !wide {
                return Err(Reject::NarrowJoin);
            }
        }
    }
    Ok(())
}

/// Either prove the complete flares AND their pads disjoint in plan view, or
/// exhibit a common 0.104 mm ball inside both flares and another inside both pads.
/// A common ball is a conservative sufficient join-width certificate. No failing
/// geometry is repaired or resized: selection must choose another alternative.
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
    // Pads have 20 sides, flares 16, so flare overlap alone does not certify
    // their pads. Use the actual pad meshes too. Z is arbitrary for this XY test.
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
    use crate::construct::{PartFields, PartKind, Section};

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
    fn redundant_bridge_corner_is_refused_and_gap_bridge_is_wide() {
        // Rounded positions measured in the rejected Round 25 file. Full-length
        // flat ends graze the third flare; construction from the pad gaps does not.
        let a = [-8.658361, 4.8202653];
        let b = [-10.810063, 4.3419685];
        let third = flare([-11.480255, 2.8054507]);
        assert!(matches!(
            admit_bridges(&[NativePart::bridge(a, b, 0.2)], &[third]),
            Err(Reject::NarrowJoin)
        ));
        let short = bridge(a, b, 0.2).unwrap();
        assert!(admit_bridges(&[short], &[flare([-11.480255, 2.8054507])]).is_ok());
        for distance in [2.21, 4., 15.] {
            let short = bridge([0., 0.], [distance, 0.], 0.2).unwrap();
            let PartFields::Bridge { from, to, .. } = short.fields() else {
                unreachable!()
            };
            assert!((from[0] - 0.8).abs() < 1e-6);
            assert!((to[0] - (distance - 0.8)).abs() < 1e-6);
            assert!(
                admit_bridges(
                    &[
                        NativePart::pad([0., 0.], 0.2),
                        NativePart::pad([distance, 0.], 0.2),
                        short
                    ],
                    &[]
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn flare_side_planes_do_not_extend_below_the_flare() {
        let third = flare([10.75, -3.7834868]);
        let box_part = NativePart::bridge([8.83109, -2.3474655], [9.476693, -2.1763792], 0.2);
        assert!(admit_bridges(&[box_part], &[third]).is_ok());
    }
}
