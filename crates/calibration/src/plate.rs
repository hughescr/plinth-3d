//! CAL-1 assembly: translation only, measured footprints, seeded blocked layout.
//! The source meshes and native support bytes are never rewritten by placement.
use crate::{Result, goo_grid};
use plinth_chitubox::{Model, Triangle};
use serde::Serialize;

pub const SEED: u64 = 20261102;
pub const GAP: f64 = 6.0;
pub const BUILD: [f64; 2] = [211.68, 118.37];
/// CAL-2: least distance from any part's footprint (raft included) to each edge of the build
/// area, mm.
pub const EDGE_MARGIN: f64 = 8.0;
pub const IDS: [&str; 8] = ["A0", "A1", "B-100", "S-100", "S-324", "K-15", "K-60", "W"];
/// Same 100-rung waist in B, S and W; inferred from the owner's measured inflation.
pub const WAIST_100: f32 = 0.229;
/// The old S-324 sliced at 278 (14% low, outside ±10%); it too needs retuning.
pub const WAIST_324: f32 = 0.125;

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Bounds {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}
impl Bounds {
    pub fn width(self) -> f64 {
        self.hi[0] - self.lo[0]
    }
    pub fn centre(self) -> [f64; 2] {
        std::array::from_fn(|k| (self.lo[k] + self.hi[k]) / 2.0)
    }
    pub fn gap(self, b: Self) -> f64 {
        let d: [f64; 2] =
            std::array::from_fn(|k| (b.lo[k] - self.hi[k]).max(self.lo[k] - b.hi[k]).max(0.0));
        d[0].hypot(d[1])
    }
    pub fn inside(self) -> bool {
        (0..2).all(|k| self.lo[k] >= -BUILD[k] / 2.0 && self.hi[k] <= BUILD[k] / 2.0)
    }
}
pub fn triangles(m: &Model) -> Vec<Triangle> {
    m.mesh
        .iter()
        .chain(
            m.supports
                .iter()
                .flat_map(|s| s.records.iter().flat_map(|r| r.mesh.iter())),
        )
        .copied()
        .collect()
}
pub fn bounds(m: &Model) -> Bounds {
    let frame = goo_grid::offset(m);
    let mut out = Bounds {
        lo: [f64::INFINITY; 3],
        hi: [f64::NEG_INFINITY; 3],
    };
    for p in m
        .mesh
        .iter()
        .chain(
            m.supports
                .iter()
                .flat_map(|s| s.records.iter().flat_map(|r| r.mesh.iter())),
        )
        .flatten()
    {
        for k in 0..3 {
            let x = f64::from(p[k]) + frame[k];
            out.lo[k] = out.lo[k].min(x);
            out.hi[k] = out.hi[k].max(x);
        }
    }
    out
}
pub(crate) struct Random(pub(crate) u64);
impl Random {
    fn next(&mut self) -> u64 {
        // Explicit xorshift64; a version-independent seeded shuffle, not a platform RNG.
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub(crate) fn below(&mut self, n: usize) -> usize {
        let n = n as u64;
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next();
            if x < limit {
                return (x % n) as usize;
            }
        }
    }
}
pub fn orders(seed: u64) -> Result<[Vec<usize>; 2]> {
    if seed == 0 {
        return Err("xorshift seed must not be zero".into());
    }
    let mut rng = Random(seed);
    Ok(std::array::from_fn(|_| {
        let mut units = [vec![0, 1], vec![2, 3], vec![4], vec![5], vec![6], vec![7]];
        for u in &mut units {
            if u.len() == 2 && rng.below(2) == 1 {
                u.swap(0, 1);
            }
        }
        for i in (1..units.len()).rev() {
            let j = rng.below(i + 1);
            units.swap(i, j);
        }
        units.into_iter().flatten().collect()
    }))
}
/// Positions are built from the complete written body + supports footprint, not nominal sizes.
pub fn place(sources: &[Model], seed: u64) -> Result<Vec<Model>> {
    place_on_grid(sources, seed, goo_grid::Grid::cal1())
}
pub fn place_on_grid(sources: &[Model], seed: u64, grid: goo_grid::Grid) -> Result<Vec<Model>> {
    if sources.len() != 8 {
        return Err("CAL-1 needs exactly eight source pieces".into());
    }
    let mut out = Vec::new();
    let pitch = grid.pitch;
    // Preserve the source's raster phase: arbitrary translation can split A1b's
    // first ball pixels from its cone. Reserve two X pixels before quantization.
    let spacing = GAP + 2.0 * pitch[0];
    for (band, order) in orders(seed)?.iter().enumerate() {
        let width = order
            .iter()
            .map(|&i| bounds(&sources[i]).width())
            .sum::<f64>()
            + spacing * 7.0;
        if width > BUILD[0] - 2.0 {
            return Err("band does not fit with 6 mm gaps".into());
        }
        let mut x = -width / 2.0;
        for &i in order {
            let mut m = sources[i].clone();
            let b = bounds(&m);
            let c = b.centre();
            let target = [x + b.width() / 2.0, if band == 0 { -30.0 } else { 30.0 }];
            for k in 0..2 {
                let delta = ((target[k] - c[k]) / pitch[k]).round() * pitch[k];
                m.translate[k] = (f64::from(m.translate[k]) + delta) as f32;
            }
            m.name = format!(
                "{}-{}; {}",
                if band == 0 { "front" } else { "back" },
                IDS[i],
                crate::LABEL
            );
            x += b.width() + spacing;
            out.push(m);
        }
    }
    validate(&out)?;
    Ok(out)
}
#[must_use]
pub fn raster_phase_kept(source: &Model, placed: &Model, grid: goo_grid::Grid) -> bool {
    (0..2).all(|k| {
        let cells =
            (f64::from(placed.translate[k]) - f64::from(source.translate[k])) / grid.pitch[k];
        (cells - cells.round()).abs() < 0.001
    })
}
/// Conservative bounding-box gaps: never claim a mesh clearance larger than this.
pub fn validate(models: &[Model]) -> Result<f64> {
    let boxes: Vec<_> = models.iter().map(bounds).collect();
    let mut gap = f64::INFINITY;
    for (i, a) in boxes.iter().enumerate() {
        if !a.inside() || a.lo[2].abs() > 0.00001 {
            return Err(format!("{}: outside build area or not on plate", models[i].name).into());
        }
        for b in boxes.iter().skip(i + 1) {
            gap = gap.min(a.gap(*b));
        }
    }
    if gap < GAP {
        return Err(format!("piece clearance {gap} < 6 mm").into());
    }
    Ok(gap)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sources() -> Vec<Model> {
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        (0..8)
            .map(|i| {
                let mut m = crate::build::specimen(&spec.specimens[0], &spec.head)
                    .models
                    .remove(0);
                m.name = IDS[i].into();
                m
            })
            .collect()
    }
    fn paired(order: &[usize]) -> bool {
        [(0, 1), (2, 3)].iter().all(|(a, b)| {
            let a = order.iter().position(|x| x == a).unwrap();
            let b = order.iter().position(|x| x == b).unwrap();
            a.abs_diff(b) == 1
        })
    }
    #[test]
    fn seeded_shuffle_is_reproducible_and_pairs_stay_together() {
        assert_eq!(orders(SEED).unwrap(), orders(SEED).unwrap());
        assert_ne!(orders(SEED).unwrap(), orders(SEED + 1).unwrap());
        for seed in 1..1000 {
            for order in orders(seed).unwrap() {
                assert!(paired(&order));
                let mut sorted = order;
                sorted.sort_unstable();
                assert_eq!(sorted, (0..8).collect::<Vec<_>>());
            }
        }
        assert!(!paired(&[0, 2, 1, 3, 4, 5, 6, 7])); // sabotage the pair boundary
        assert!(orders(0).is_err());
    }
    #[test]
    fn placements_fit_and_clearance_and_edge_sabotages_fail() {
        let source = sources();
        let mut plate = place(&source, SEED).unwrap();
        assert!(validate(&plate).unwrap() >= 5.99999);
        for m in &plate {
            assert!(bounds(m).inside());
        }
        plate[1].translate = plate[0].translate;
        assert!(validate(&plate).is_err());
        let mut plate = place(&source, SEED).unwrap();
        plate[0].translate[0] = 106.0;
        assert!(validate(&plate).is_err());
        let mut plate = place(&source, SEED).unwrap();
        plate[0].translate[2] += 0.1;
        assert!(validate(&plate).is_err());
    }
    #[test]
    fn translation_phase_guard_rejects_a_half_pixel_shift() {
        let source = sources();
        let plate = place(&source, SEED).unwrap();
        let grid = goo_grid::Grid::cal1();
        for m in &plate {
            assert!(raster_phase_kept(&source[0], m, grid));
        }
        let mut broken = plate[0].clone();
        broken.translate[0] += 0.007;
        assert!(!raster_phase_kept(&source[0], &broken, grid));
    }
    #[test]
    fn written_retuned_waists_and_old_dimension_sabotage() {
        let spec: crate::spec::PlateSpec =
            serde_json::from_str(include_str!("../../../fixtures/calibration/cal1.json")).unwrap();
        spec.validate().unwrap();
        for s in spec
            .specimens
            .iter()
            .filter(|s| matches!(s.body, crate::spec::Body::Block | crate::spec::Body::Strip))
        {
            let p = crate::build::specimen(s, &spec.head);
            let written = plinth_chitubox::parse(&p.to_bytes().unwrap()).unwrap();
            for head in written.models[0].supports.as_ref().unwrap().records[..28].chunks_exact(7) {
                let r = &head[1];
                let want = if s.id == "S-324" {
                    WAIST_324
                } else {
                    WAIST_100
                };
                assert_eq!((r.r1 * 2.0).to_bits(), want.to_bits());
                let section = crate::section::SectionMesh::new(&r.mesh, [0.0; 3], [0.0, 0.0, 1.0]);
                let area = crate::section::union_area(&[section], -3.0);
                let target =
                    8.0 * (std::f64::consts::TAU / 16.0).sin() * (f64::from(want) / 2.0).powi(2);
                assert!((area / target - 1.0).abs() < 0.0001);
            }
        }
        let mut broken = spec;
        broken.specimens[0].waist_diameters = vec![0.242; 4];
        assert!(broken.validate().is_err());
        broken.specimens[0].waist_diameters = vec![0.099; 4];
        assert!(broken.validate().is_err());
    }
}
