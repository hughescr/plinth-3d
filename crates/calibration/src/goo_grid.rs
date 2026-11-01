//! CAL-1 midpoint raster witness on the owner's GOO grid, not a ChituBox replacement.
//! Uses written triangles, nonzero winding, and two boundary conventions.
use crate::{
    Result,
    goo::{self, Span},
};
use plinth_chitubox::{Model, Triangle};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub pitch: [f64; 2],
    pub resolution: [u16; 2],
    pub height: f64,
}
impl Grid {
    #[must_use]
    pub fn cal1() -> Self {
        Self {
            pitch: [0.014, 0.019],
            resolution: [15120, 6230],
            height: 0.05,
        }
    }
    #[must_use]
    pub fn from_header(h: &goo::Header) -> Self {
        Self {
            pitch: h.pitch(),
            resolution: h.resolution,
            height: f64::from(h.layer_mm),
        }
    }
    fn pixel(self, p: [f64; 3]) -> [f64; 3] {
        [
            f64::from(self.resolution[0]) / 2.0 + 0.5 - p[0] / self.pitch[0],
            f64::from(self.resolution[1]) / 2.0 + 0.5 - p[1] / self.pitch[1],
            p[2],
        ]
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Boundary {
    Centre,
    Touch,
}
#[derive(Debug, Clone, Serialize)]
pub struct Island {
    pub layer: usize,
    pub z_mid: f64,
    pub blob: goo::Blob,
}
#[derive(Debug, Clone, Copy)]
struct Segment {
    a: [f64; 2],
    b: [f64; 2],
}
#[must_use]
pub fn offset(m: &Model) -> [f64; 3] {
    let min = m
        .mesh
        .iter()
        .flatten()
        .map(|p| f64::from(p[2]))
        .fold(f64::INFINITY, f64::min);
    [
        f64::from(m.translate[0]),
        f64::from(m.translate[1]),
        f64::from(m.translate[2]) - min,
    ]
}
fn segment(t: [[f64; 3]; 3], z: f64) -> Option<Segment> {
    let mut p = Vec::with_capacity(2);
    for i in 0..3 {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        if (a[2] <= z && b[2] > z) || (b[2] <= z && a[2] > z) {
            let f = (z - a[2]) / (b[2] - a[2]);
            p.push([a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1])]);
        }
    }
    if p.len() != 2 {
        return None;
    }
    let a = std::array::from_fn::<_, 3, _>(|i| t[1][i] - t[0][i]);
    let b = std::array::from_fn::<_, 3, _>(|i| t[2][i] - t[0][i]);
    let normal = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2]];
    if (p[1][0] - p[0][0]) * (-normal[1]) + (p[1][1] - p[0][1]) * normal[0] < 0.0 {
        p.swap(0, 1);
    }
    Some(Segment { a: p[0], b: p[1] })
}
fn merge(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_unstable_by_key(|s| (s.y, s.x0, s.x1));
    let mut out: Vec<Span> = Vec::new();
    for s in spans {
        if let Some(last) = out.last_mut()
            && last.y == s.y
            && s.x0 <= last.x1
        {
            last.x1 = last.x1.max(s.x1);
            continue;
        }
        out.push(s);
    }
    out
}
fn span(y: u32, x0: f64, x1: f64, width: u16) -> Option<Span> {
    let (a, b) = (
        x0.clamp(0.0, f64::from(width)) as u32,
        x1.clamp(0.0, f64::from(width)) as u32,
    );
    (a < b).then_some(Span {
        y,
        x0: a,
        x1: b,
        value: 255,
    })
}
fn raster(segments: &[Segment], grid: Grid, boundary: Boundary) -> Vec<Span> {
    let mut cross: Vec<(u32, f64, i32)> = Vec::new();
    let mut out = Vec::new();
    for s in segments {
        let (lo, hi, sign) = if s.a[1] < s.b[1] {
            (s.a, s.b, 1)
        } else {
            (s.b, s.a, -1)
        };
        if hi[1] > lo[1] {
            let start = (lo[1] - 0.5).ceil().max(0.0) as u32;
            let end = (hi[1] - 0.5)
                .ceil()
                .min(f64::from(grid.resolution[1]))
                .max(0.0) as u32;
            for y in start..end {
                let f = (f64::from(y) + 0.5 - lo[1]) / (hi[1] - lo[1]);
                cross.push((y, lo[0] + f * (hi[0] - lo[0]), sign));
            }
        }
        if matches!(boundary, Boundary::Touch) {
            let start = lo[1].floor().max(0.0) as u32;
            let end = hi[1]
                .floor()
                .min(f64::from(grid.resolution[1]) - 1.0)
                .max(0.0) as u32;
            for y in start..=end {
                let f = |v: f64| {
                    if hi[1] > lo[1] {
                        ((v - lo[1]) / (hi[1] - lo[1])).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                };
                let x0 = lo[0] + f(f64::from(y)) * (hi[0] - lo[0]);
                let x1 = if hi[1] > lo[1] {
                    lo[0] + f(f64::from(y + 1)) * (hi[0] - lo[0])
                } else {
                    hi[0]
                };
                if let Some(s) = span(
                    y,
                    x0.min(x1).floor(),
                    x0.max(x1).floor() + 1.0,
                    grid.resolution[0],
                ) {
                    out.push(s);
                }
            }
        }
    }
    cross.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut i = 0;
    while i < cross.len() {
        let y = cross[i].0;
        let mut winding = 0;
        let mut start = 0.0;
        while i < cross.len() && cross[i].0 == y {
            let before = winding;
            winding += cross[i].2;
            if before == 0 && winding != 0 {
                start = cross[i].1;
            }
            if before != 0
                && winding == 0
                && let Some(s) = span(
                    y,
                    (start - 0.5).ceil(),
                    (cross[i].1 - 0.5).ceil(),
                    grid.resolution[0],
                )
            {
                out.push(s);
            }
            i += 1;
        }
    }
    merge(out)
}
/// Batch the triangle/plane intersections once, then raster every midpoint layer.
pub fn slice(
    mesh: &[Triangle],
    offset: [f64; 3],
    grid: Grid,
    boundary: Boundary,
) -> Result<Vec<Vec<Span>>> {
    if grid.height <= 0.0 || grid.pitch.iter().any(|x| *x <= 0.0) {
        return Err("invalid CAL-1 witness grid".into());
    }
    let max = mesh
        .iter()
        .flatten()
        .map(|p| f64::from(p[2]) + offset[2])
        .fold(0f64, f64::max);
    let n = (max / grid.height).ceil() as usize;
    let mut layers = vec![Vec::new(); n];
    for tri in mesh {
        let t = tri.map(|p| grid.pixel(std::array::from_fn(|i| f64::from(p[i]) + offset[i])));
        let lo = t.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        let hi = t.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
        let first = ((lo / grid.height - 0.5).ceil().max(0.0) as usize).min(n);
        let last = ((hi / grid.height - 0.5).floor().max(-1.0) as isize + 1).max(0) as usize;
        for (k, l) in layers.iter_mut().enumerate().take(last.min(n)).skip(first) {
            if let Some(s) = segment(t, (k as f64 + 0.5) * grid.height) {
                l.push(s);
            }
        }
    }
    Ok(layers.iter().map(|s| raster(s, grid, boundary)).collect())
}
/// For a head pair the first nonempty layer is carried by the unchanged lower support.
#[must_use]
pub fn islands(layers: &[Vec<Span>], grid: Grid) -> Vec<Island> {
    let mut out = Vec::new();
    let mut seen = false;
    for (i, s) in layers.iter().enumerate() {
        if seen {
            for b in goo::blobs(s, &layers[i - 1]) {
                if b.overlap_below_pixels == 0 {
                    out.push(Island {
                        layer: i + 1,
                        z_mid: (i as f64 + 0.5) * grid.height,
                        blob: b,
                    });
                }
            }
        }
        seen |= !s.is_empty();
    }
    out
}

/// Keep every field and endpoint except r1; select a canonical native cone whose
/// written cone/ball pair has no new midpoint island under either boundary rule.
/// Both checks are required during construction; caller still needs a ChituBox slice.
pub fn thin_cone(
    cone: &plinth_chitubox::SupportRecord,
    ball: &plinth_chitubox::SupportRecord,
    frame: [f64; 3],
    grid: Grid,
) -> Result<plinth_chitubox::SupportRecord> {
    for radius in [0.052f32, 0.07, 0.10, 0.12, 0.15, 0.18] {
        let mesh = plinth_geom::shapes::frustum(cone.start, cone.end, radius, cone.r2);
        let pair: [&[Triangle]; 2] = [&ball.mesh, &mesh];
        let triangles = pair.concat();
        let mut ok = true;
        for boundary in [Boundary::Centre, Boundary::Touch] {
            let layers = slice(&triangles, frame, grid, boundary)?;
            if !islands(&layers, grid).is_empty() {
                ok = false;
                break;
            }
        }
        if ok {
            let mut result = cone.clone();
            result.r1 = radius;
            result.mesh = mesh;
            return Ok(result);
        }
    }
    Err("CAL-1 cone cannot be thinned without a midpoint island; keep A0".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn winding_fills_union_and_midpoints() {
        let mesh = plinth_geom::shapes::frustum([0.0, 0.0, 0.0], [0.0, 0.0, 0.2], 0.1, 0.1);
        let g = Grid::cal1();
        for mode in [Boundary::Centre, Boundary::Touch] {
            let a = slice(&mesh, [0.0; 3], g, mode).unwrap();
            assert_eq!(a.len(), 5); // Written f32 top is just above .2; last midpoint is empty.
            assert_eq!(a.iter().filter(|s| !s.is_empty()).count(), 4);
            assert!(islands(&a, g).is_empty());
            let twice = [mesh.clone(), mesh.clone()].concat();
            let b = slice(&twice, [0.0; 3], g, mode).unwrap();
            assert_eq!(
                a.iter()
                    .map(|s| s.iter().map(|s| s.x1 - s.x0).sum::<u32>())
                    .collect::<Vec<_>>(),
                b.iter()
                    .map(|s| s.iter().map(|s| s.x1 - s.x0).sum::<u32>())
                    .collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn record_five_ball_onset_requires_a_wider_cone_and_sabotage_fires() {
        use plinth_chitubox::{RecordType as Ty, SupportRecord};
        let point = |p: [u32; 3]| p.map(f32::from_bits);
        let a = point([0xc0481f60, 0x4081a8cc, 0xc134aca4]);
        let b = point([0xc066a4ad, 0x40aac8ce, 0xc14bf9d8]);
        let sa = point([0xc0444eb6, 0x40790997, 0xc131c2fe]);
        let sb = point([0xc04bf009, 0x4086cccc, 0xc137964b]);
        let cone = SupportRecord {
            record_type: Ty::TIP_CONE,
            start: a,
            end: b,
            r1: 0.2,
            r2: 0.6,
            tail: [0; 6],
            mesh: plinth_geom::shapes::frustum(a, b, 0.2, 0.6),
        };
        let ball = SupportRecord {
            record_type: Ty::TIP_BALL,
            start: sa,
            end: sb,
            r1: 0.25,
            r2: 0.25,
            tail: [0; 6],
            mesh: plinth_geom::shapes::sphere(sa, sb, 0.25),
        };
        let frame = [45.64874267578125, 8.614495277404785, 14.615063667297363];
        let g = Grid::cal1();
        let broken = plinth_geom::shapes::frustum(a, b, 0.052, 0.6);
        let broken_pair = [ball.mesh.clone(), broken].concat();
        for boundary in [Boundary::Centre, Boundary::Touch] {
            let layers = slice(&broken_pair, frame, g, boundary).unwrap();
            assert!(islands(&layers, g).iter().any(|i| i.layer == 62));
        }
        let fixed = thin_cone(&cone, &ball, frame, g).unwrap();
        assert_eq!(fixed.r1.to_bits(), 0.1f32.to_bits());
        assert_eq!(fixed.start.map(f32::to_bits), cone.start.map(f32::to_bits));
        assert_eq!(fixed.end.map(f32::to_bits), cone.end.map(f32::to_bits));
        let pair = [ball.mesh, fixed.mesh].concat();
        for boundary in [Boundary::Centre, Boundary::Touch] {
            let layers = slice(&pair, frame, g, boundary).unwrap();
            assert!(islands(&layers, g).is_empty());
        }
    }
    #[test]
    fn disconnected_upper_ball_is_detected() {
        let mut mesh = plinth_geom::shapes::frustum([0.0, 0.0, 0.0], [0.0, 0.0, 0.2], 0.1, 0.1);
        mesh.extend(plinth_geom::shapes::sphere(
            [0.4, 0.0, 0.5],
            [0.4, 0.0, 0.3],
            0.1,
        ));
        let g = Grid::cal1();
        let layers = slice(&mesh, [0.0; 3], g, Boundary::Centre).unwrap();
        assert!(!islands(&layers, g).is_empty());
    }
}
