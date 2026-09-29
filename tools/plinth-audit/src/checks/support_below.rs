//! I5 clause (e), "no support island": every support primitive is held up
//! FROM BELOW. (Clause (d) on fix/no-dangling; renumbered on landing, because
//! fix/plate-gap's plate-root clause took (d) first.)
//!
//! I5's grounding (`load_path.rs`) asks whether a primitive is CONNECTED to the
//! plate or to a valid model seat. That is the right question for a load path
//! and the wrong one for printability: a support arm whose tip is seated in
//! the model and whose lower end stops in the air is connected — through the
//! model it hangs from — and I5 grounds it. It still prints as an island: its
//! lowest layer is cured on nothing, peels off with the vat film, and the
//! member above it is built on a floating stub. The owner saw these in
//! ChituBox on 2026-09-28 ("porcupine quills", 157 on the Cylinder, 28 on the
//! Sphere; `plinth-evidence/.../field-2026-09-28/quills.md`), on files I5
//! could not tell apart from good ones.
//!
//! ## The rule
//!
//! Printing runs upward, so support has to exist before what rests on it. For
//! every support primitive, its LOWEST point must rest on one of:
//!
//! * **the plate** — the lowest point is within one layer of `z_plate`; a
//!   root FLARE may instead end at the raft top, `z_plate +`
//!   [`c::RAFT_PAD_HEIGHT_MM`], because that is where the format's own flare
//!   contract puts its lower end (`reader.rs`, wire 4: "column end to raft
//!   top") and the plinth emitter writes no raft record under it. Those are
//!   counted apart (`rooted_at_raft_top`) so the unresolved raft question
//!   stays visible;
//! * **the model** — the lowest point is inside the model or within one layer
//!   of its surface. **A tip ball is never rooted this way**: a tip seated in
//!   the model holds only its own member's TOP, and a member hanging from its
//!   tip is exactly the island this clause exists to find;
//! * **another primitive that extends below it** — the lowest point lies
//!   within one layer of that primitive's solid (a ball: the two solids
//!   overlap), and that primitive's own lowest point is strictly lower.
//!
//! and the chain it rests on must end at the plate or the model. The relation
//! is closed transitively from the plate and model roots, so a member resting
//! on a quill is itself unrooted.
//!
//! ## Cost
//!
//! One plan-view bucket grid over the primitives' boxes, built once; each
//! lowest point is tested only against the solids whose boxes cover its
//! column. The model clause is one surface query per non-tip primitive. The
//! only all-pairs work is the "nearest lower solid" distance published with a
//! failure, computed for UNROOTED primitives alone, so a clean file pays
//! nothing for it.

use crate::consts as c;
use crate::mesh::Inside;
use crate::primitives::Primitive;
use crate::reader::RecordKind;
use crate::vec3::V3;

/// Where one primitive's lowest point rests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rest {
    Plate,
    RaftTop,
    Model,
    /// On at least one lower primitive (rooted or not; the closure decides).
    Member,
    Nothing,
    /// The model test could not be decided (open mesh, exhausted rays) and no
    /// other root applied. Never a pass and never a failure.
    Indeterminate,
}

/// Whether a point rests on the model, as the caller's mesh can say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnModel {
    Yes,
    No,
    Unknown,
}

/// The classification of every record.
#[derive(Clone, Debug, Default)]
pub struct BelowReport {
    /// Per record: what its lowest point rests on (`None` for a record with no
    /// solid).
    pub rest: Vec<Option<Rest>>,
    /// Per record: rooted through a chain that ends at the plate or the model.
    pub rooted: Vec<bool>,
    /// Records whose lowest point rests on nothing, or on a chain that does
    /// not end at a root, EXCLUDING tip balls (a tip always sits on its own
    /// taper, so an unrooted tip is named through its taper). Record indices.
    pub unrooted: Vec<usize>,
    /// For each entry of `unrooted`: its lowest point, and the distance from it
    /// to the nearest solid that reaches lower (`inf` when none does).
    pub unrooted_low: Vec<V3>,
    pub unrooted_nearest_below_mm: Vec<f64>,
    pub rooted_at_plate: usize,
    pub rooted_at_raft_top: usize,
    pub rooted_on_model: usize,
    pub resting_on_member: usize,
    pub indeterminate: usize,
}

/// The lowest point of a primitive's solid, on its axis where it has one.
///
/// A frustum's lower AXIS end, not the rim of its end disc: the question is
/// where the member's own centreline stops, which is where the next member
/// has to take it, and a tilted end disc's rim is lower by up to its radius
/// without being anywhere the load goes.
#[must_use]
pub fn lowest_point(p: &Primitive) -> V3 {
    match *p {
        Primitive::Ball { centre, r } => [centre[0], centre[1], centre[2] - r],
        Primitive::Frustum { a, b, .. } => {
            if a[2] <= b[2] {
                a
            } else {
                b
            }
        }
        Primitive::RaftPad { base, .. } => base,
        Primitive::RaftBridge { a, b, .. } => {
            if a[2] <= b[2] {
                a
            } else {
                b
            }
        }
    }
}

/// A uniform plan-view bucket grid over the primitives' boxes, so each lowest
/// point is tested against the handful of solids above its own column rather
/// than against every record in the file.
struct Columns {
    origin: [f64; 2],
    cell: f64,
    dims: [usize; 2],
    /// CSR: `starts[c]..starts[c + 1]` indexes into `items`.
    starts: Vec<usize>,
    items: Vec<usize>,
}

impl Columns {
    /// `boxes` are `(record, min, max)` in plan view, already expanded by the
    /// tolerance the query will use.
    fn build(boxes: &[(usize, [f64; 2], [f64; 2])], cell: f64) -> Self {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for (_, min, max) in boxes {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(min[axis]);
                hi[axis] = hi[axis].max(max[axis]);
            }
        }
        if !lo[0].is_finite() {
            return Self { origin: [0.0; 2], cell, dims: [0, 0], starts: vec![0], items: Vec::new() };
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let dims = [
            (((hi[0] - lo[0]) / cell).floor() as usize) + 1,
            (((hi[1] - lo[1]) / cell).floor() as usize) + 1,
        ];
        let mut grid = Self { origin: lo, cell, dims, starts: Vec::new(), items: Vec::new() };
        let mut counts = vec![0usize; dims[0] * dims[1] + 1];
        let span = |g: &Self, min: [f64; 2], max: [f64; 2]| (g.index(min), g.index(max));
        for &(_, min, max) in boxes {
            let ((x0, y0), (x1, y1)) = span(&grid, min, max);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    counts[y * dims[0] + x + 1] += 1;
                }
            }
        }
        for c in 1..counts.len() {
            counts[c] += counts[c - 1];
        }
        let mut fill = counts.clone();
        let mut items = vec![0usize; *counts.last().unwrap_or(&0)];
        for &(record, min, max) in boxes {
            let ((x0, y0), (x1, y1)) = span(&grid, min, max);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let c = y * dims[0] + x;
                    items[fill[c]] = record;
                    fill[c] += 1;
                }
            }
        }
        grid.starts = counts;
        grid.items = items;
        grid
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn index(&self, p: [f64; 2]) -> (usize, usize) {
        let x = ((p[0] - self.origin[0]) / self.cell).floor().max(0.0) as usize;
        let y = ((p[1] - self.origin[1]) / self.cell).floor().max(0.0) as usize;
        (x.min(self.dims[0].saturating_sub(1)), y.min(self.dims[1].saturating_sub(1)))
    }

    fn at(&self, p: V3) -> &[usize] {
        if self.dims[0] == 0
            || p[0] < self.origin[0]
            || p[1] < self.origin[1]
            || p[0] > self.origin[0] + self.cell * self.dims[0] as f64
            || p[1] > self.origin[1] + self.cell * self.dims[1] as f64
        {
            return &[];
        }
        let (x, y) = self.index([p[0], p[1]]);
        let c = y * self.dims[0] + x;
        &self.items[self.starts[c]..self.starts[c + 1]]
    }
}

/// Classify every primitive's lowest point and close the relation.
///
/// * `ok` — the records that have a solid, `(record index, primitive)`;
/// * `kinds` — every record's kind, indexed by record;
/// * `on_model` — the model clause, asked once per non-tip primitive.
///
/// The member clause is a POINT test — is this lowest point within one layer
/// of that solid — and deliberately not I5's pairwise overlap graph: two
/// solids can meet in I5's fusion-uncertain band while one's lowest point sits
/// squarely inside the other, and a clause that only trusted definite overlap
/// edges would call that member unrooted.
#[must_use]
pub fn classify(
    ok: &[(usize, Primitive)],
    kinds: &[RecordKind],
    z_plate_mm: f64,
    layer_mm: f64,
    on_model: impl Fn(V3) -> OnModel,
) -> BelowReport {
    let n = kinds.len();
    let mut report = BelowReport {
        rest: vec![None; n],
        rooted: vec![false; n],
        ..BelowReport::default()
    };
    let mut prim: Vec<Option<Primitive>> = vec![None; n];
    let mut low: Vec<V3> = vec![[0.0; 3]; n];
    let mut min_z = vec![f64::INFINITY; n];
    for &(i, p) in ok {
        prim[i] = Some(p);
        low[i] = lowest_point(&p);
        min_z[i] = p.aabb().min[2];
    }
    let boxes: Vec<(usize, [f64; 2], [f64; 2])> = ok
        .iter()
        .map(|&(i, p)| {
            let bb = p.aabb();
            (
                i,
                [bb.min[0] - layer_mm, bb.min[1] - layer_mm],
                [bb.max[0] + layer_mm, bb.max[1] + layer_mm],
            )
        })
        .collect();
    // One millimetre columns: a support member is a fraction of that across,
    // so a column holds a few solids and a query reads only those.
    let columns = Columns::build(&boxes, 1.0);

    let mut rests_on: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(i, _) in ok {
        let kind = kinds[i];
        let point = low[i];
        let tol = c::tol_geom_at(point);
        let is_ball = kind.is_ball();
        // --- the plate, and the raft top a flare ends at ---------------------
        if !is_ball {
            if point[2] <= z_plate_mm + layer_mm + tol {
                report.rest[i] = Some(Rest::Plate);
                report.rooted[i] = true;
                report.rooted_at_plate += 1;
                continue;
            }
            if kind == RecordKind::RootFlare
                && point[2] <= z_plate_mm + c::RAFT_PAD_HEIGHT_MM + layer_mm + tol
            {
                report.rest[i] = Some(Rest::RaftTop);
                report.rooted[i] = true;
                report.rooted_at_raft_top += 1;
                continue;
            }
        }
        // --- another primitive that reaches lower -----------------------------
        for &j in columns.at(point) {
            if j == i || min_z[j] >= point[2] - tol {
                continue;
            }
            let Some(q) = prim[j] else { continue };
            if q.signed_distance(point) <= layer_mm {
                rests_on[i].push(j);
            }
        }
        // --- the model, never for a tip ----------------------------------------
        let model = if is_ball { OnModel::No } else { on_model(point) };
        let rest = match model {
            OnModel::Yes => {
                report.rooted[i] = true;
                report.rooted_on_model += 1;
                Rest::Model
            }
            _ if !rests_on[i].is_empty() => {
                report.resting_on_member += 1;
                Rest::Member
            }
            OnModel::Unknown => {
                report.indeterminate += 1;
                Rest::Indeterminate
            }
            OnModel::No => Rest::Nothing,
        };
        report.rest[i] = Some(rest);
    }

    // Close the relation upward from the roots.
    loop {
        let mut changed = false;
        for &(i, _) in ok {
            if !report.rooted[i] && rests_on[i].iter().any(|&j| report.rooted[j]) {
                report.rooted[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for &(i, _) in ok {
        if report.rooted[i]
            || kinds[i].is_ball()
            || report.rest[i] == Some(Rest::Indeterminate)
        {
            continue;
        }
        let point = low[i];
        let nearest = ok
            .iter()
            .filter(|&&(j, _)| j != i && min_z[j] < point[2])
            .map(|&(_, q)| q.signed_distance(point))
            .fold(f64::INFINITY, f64::min);
        report.unrooted.push(i);
        report.unrooted_low.push(point);
        report.unrooted_nearest_below_mm.push(nearest);
    }
    report
}

/// The model clause over a real mesh: inside, or within one layer of the
/// surface.
#[must_use]
pub fn on_model_mesh(mesh: &crate::mesh::Mesh, point: V3, layer_mm: f64) -> OnModel {
    if mesh.is_empty() {
        return OnModel::No;
    }
    if let Some(hit) = mesh.nearest_surface(point) {
        if hit.distance_mm <= layer_mm {
            return OnModel::Yes;
        }
    }
    match mesh.point_inside(point) {
        Inside::Inside => OnModel::Yes,
        Inside::Outside => OnModel::No,
        Inside::Indeterminate => OnModel::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify, OnModel};
    use crate::primitives::Primitive;
    use crate::reader::RecordKind;

    const LAYER: f64 = 0.05;

    fn run(prims: &[Primitive], kinds: &[RecordKind]) -> super::BelowReport {
        let ok: Vec<(usize, Primitive)> = prims.iter().copied().enumerate().collect();
        // The "model" is everything above z = 10.
        classify(&ok, kinds, 0.0, LAYER, |p| {
            if p[2] >= 10.0 - LAYER {
                OnModel::Yes
            } else {
                OnModel::No
            }
        })
    }

    /// A tip seated in the model, a taper down to a flare, the flare to the
    /// raft top: rooted end to end, and the tip is NOT rooted by its seat.
    #[test]
    fn a_whole_arm_is_rooted_from_the_plate_up() {
        let prims = [
            Primitive::Ball { centre: [0.0, 0.0, 10.0], r: 0.2 },
            Primitive::Frustum { a: [0.0, 0.0, 10.0], ra: 0.2, b: [0.0, 0.0, 1.0], rb: 0.4 },
            Primitive::Frustum { a: [0.0, 0.0, 1.0], ra: 0.4, b: [0.0, 0.0, 0.2], rb: 0.8 },
        ];
        let kinds = [RecordKind::ContactSphere, RecordKind::ContactTaper, RecordKind::RootFlare];
        let report = run(&prims, &kinds);
        assert!(report.unrooted.is_empty(), "{report:?}");
        assert_eq!(report.rooted_at_raft_top, 1);
        assert_eq!(report.rooted_on_model, 0, "a tip must never root through its seat");
    }

    /// The quill: a tip seated in the model and a taper that stops in the air
    /// 2 mm away from the neighbouring trunk. The taper is named; a second arm
    /// that rests on the quill is named too.
    #[test]
    fn a_quill_and_everything_resting_on_it_are_named() {
        let prims = [
            // A good trunk at x = 0.
            Primitive::Frustum { a: [0.0, 0.0, 10.0], ra: 0.3, b: [0.0, 0.0, 0.0], rb: 0.3 },
            // The quill: seated at x = 3, lower end at (2.2, 0, 8.7), 2 mm off the trunk.
            Primitive::Ball { centre: [3.0, 0.0, 10.0], r: 0.1 },
            Primitive::Frustum { a: [3.0, 0.0, 10.0], ra: 0.1, b: [2.2, 0.0, 8.7], rb: 0.15 },
            // A child landed on the quill's shaft.
            Primitive::Frustum { a: [4.0, 0.0, 10.5], ra: 0.1, b: [2.6, 0.0, 9.35], rb: 0.12 },
            // A child landed on the good trunk.
            Primitive::Frustum { a: [-1.5, 0.0, 10.0], ra: 0.1, b: [0.0, 0.0, 8.0], rb: 0.12 },
        ];
        let kinds = [
            RecordKind::ContactTaper,
            RecordKind::ContactSphere,
            RecordKind::ContactTaper,
            RecordKind::ContactTaper,
            RecordKind::ContactTaper,
        ];
        let report = run(&prims, &kinds);
        assert_eq!(report.unrooted, vec![2, 3], "{report:?}");
        assert!(report.rooted[4], "a branch on a rooted trunk is rooted");
        assert!(report.unrooted_nearest_below_mm[0] > 1.0);
    }
}
