//! Down-sweep reuse probe (X0, 2026-10-10): how much of each per-event
//! `down_sweep::run` a resumed sweep could have skipped.
//!
//! A WRITE-ONLY MEASUREMENT. Nothing here changes a member, a field or a
//! decision; every function returns at once when `perf` counting is off, and
//! the product never reads what it stores. It exists to answer one question
//! before any lane builds an incremental down sweep: of the cell-layers a run
//! walks, what fraction lies ABOVE the highest layer the previous run's edit
//! touched (the prefix a resume skips), and, separately, how many of the
//! (layer, band) regions below that line come out equal anyway (what a
//! regional reuse could skip).
//!
//! # What is compared
//!
//! The previous run in the SAME pose frame (one `run_sweep_with_holes`; the
//! slices are fixed inside it, so two runs differ only by the members). Three
//! figures per run, written as one `down.reuse` row:
//!
//! * `dirty_member`: the number of layers from the bottom up to and including
//!   the highest layer any CHANGED, added or removed member reaches (members
//!   matched by id; a removal renumbers ids, so this over-states the change
//!   after one and is the conservative figure). A resume point at the top of
//!   the changed members, the retained r3 S3 design's rule.
//! * `dirty_layer`: the same count for the highest layer whose live-member
//!   fingerprint changed (id-free, so it is not fooled by a renumbering). At
//!   most `dirty_member`: the permissive figure.
//! * `bands_total`, `bands_equal`: of the (layer, 32-row band) pairs below the
//!   `dirty_layer` line that carry load or an arm in either run, how many have
//!   the same hash of load, carriers and per-arm ledger slots after the
//!   member phase of pass 1. Collected only with `PLINTH_PERF_BANDS=1`,
//!   because the hashing costs real time and would distort the spans.
//!
//! `layers - dirty_*` is the prefix above the line. A first run in a frame has
//! nothing to compare with and is marked `first` (it counts no prefix).

use crate::SweepStrut;
use geom_kernel::perf;

/// Rows per band in the band comparison.
const BAND_ROWS: usize = 32;

#[inline]
fn mix(h: u64, v: u64) -> u64 {
    // The rotation keeps `mix(a, b)` and `mix(b, a)` apart; `h ^ v` alone is
    // symmetric, so two inputs with the same XOR would collide.
    let x = (h.rotate_left(23) ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^ (x >> 29)
}

/// One run's fingerprints.
#[derive(Default)]
struct Snapshot {
    /// Per member (index = id): fingerprint and the layers up to and including
    /// its top alive layer (`min(k_top, layers)`).
    members: Vec<(u64, usize)>,
    /// Per layer: the commutative sum of the fingerprints of the members
    /// alive there, id left out.
    layer: Vec<u64>,
}

/// The probe's state for one pose frame.
#[derive(Default)]
pub(crate) struct Probe {
    bands_on: Option<bool>,
    previous: Option<Snapshot>,
    /// The previous run's pass-1 band hashes, `[layer * bands + band]`, 0 for
    /// an empty band.
    prev_bands: Vec<u64>,
    /// This run's, being filled by pass 1.
    cur_bands: Vec<u64>,
    bands: usize,
    pending: Option<Pending>,
}

/// What `begin` worked out, until pass 1 ends and the row can be written.
struct Pending {
    site: u64,
    layers: usize,
    cells: usize,
    first: bool,
    dirty_member: usize,
    dirty_layer: usize,
}

fn member_print(strut: &SweepStrut) -> u64 {
    let mut h = mix(0x51ed_270b, strut.kind as u64);
    for value in [
        strut.cell as u64,
        strut.k_top as u64,
        strut.k_bottom as u64,
        strut.min_radius_mm.to_bits(),
        u64::from(strut.seat_on_strut.unwrap_or(u32::MAX)),
        u64::from(strut.inserted_by_load),
        strut.joint_hold_layer().map_or(u64::MAX, |layer| layer as u64),
    ] {
        h = mix(h, value);
    }
    h = match strut.landing {
        crate::Landing::Plate => mix(h, 1),
        crate::Landing::Model { layer } => mix(mix(h, 2), layer as u64),
        crate::Landing::Strut { id, layer } => mix(mix(mix(h, 3), u64::from(id)), layer as u64),
    };
    for cell in &strut.path {
        h = mix(h, u64::from(*cell));
    }
    for radius in &strut.thickened_mm {
        h = mix(h, radius.to_bits());
    }
    h
}

/// A member's identity for the layer fingerprints: everything but its id and
/// its cell at the layer.
fn layer_seed(strut: &SweepStrut) -> u64 {
    mix(
        mix(0x7a11_7a11, strut.kind as u64),
        strut.min_radius_mm.to_bits(),
    )
}

/// Whether the band probe is collecting for this frame.
pub(crate) fn collecting() -> bool {
    perf::with_pose_slot(|probe: &mut Probe| probe.bands_on == Some(true)).unwrap_or(false)
}

/// Fingerprint the members of a per-event run about to start, and compare
/// them with the previous run in this pose frame. `site` is the sizing site
/// (`perf::size_site_peek`).
pub(crate) fn begin(cells: usize, layers: usize, struts: &[SweepStrut], site: u64) {
    if !perf::enabled() {
        return;
    }
    let mut now = Snapshot {
        members: Vec::with_capacity(struts.len()),
        layer: vec![0; layers],
    };
    for strut in struts {
        let print = member_print(strut);
        let top = strut.k_top.min(layers);
        now.members.push((print, top));
        if strut.k_bottom == usize::MAX {
            continue;
        }
        let seed = layer_seed(strut);
        for layer in strut.k_bottom..top {
            let mut h = mix(seed, strut.cell_at(layer) as u64);
            h = mix(h, strut.thickened_mm.get(layer).map_or(0, |r| r.to_bits()));
            now.layer[layer] = now.layer[layer].wrapping_add(h);
        }
    }
    perf::with_pose_slot(|probe: &mut Probe| {
        probe.bands_on.get_or_insert_with(|| {
            std::env::var_os("PLINTH_PERF_BANDS").is_some_and(|value| value == "1")
        });
        let first = probe.previous.is_none();
        let (mut dirty_member, mut dirty_layer) = (layers, layers);
        if let Some(before) = probe.previous.as_ref() {
            dirty_member = 0;
            for index in 0..before.members.len().max(now.members.len()) {
                let (old, new) = (before.members.get(index), now.members.get(index));
                if old.map(|m| m.0) != new.map(|m| m.0) {
                    let top = old.map_or(0, |m| m.1).max(new.map_or(0, |m| m.1));
                    dirty_member = dirty_member.max(top);
                }
            }
            dirty_layer = (0..layers)
                .rev()
                .find(|&layer| before.layer.get(layer) != now.layer.get(layer))
                .map_or(0, |layer| layer + 1);
        }
        probe.previous = Some(now);
        probe.bands = 0;
        probe.cur_bands.clear();
        probe.pending = Some(Pending {
            site,
            layers,
            cells,
            first,
            dirty_member,
            dirty_layer,
        });
    });
}

/// Size the band store for a pass-1 sweep over `cells` cells of `cols`
/// columns.
pub(crate) fn begin_bands(layers: usize, cells: usize, cols: usize) {
    let rows = cells.checked_div(cols).unwrap_or(0);
    let bands = rows.div_ceil(BAND_ROWS);
    perf::with_pose_slot(|probe: &mut Probe| {
        probe.bands = bands;
        probe.cur_bands.clear();
        probe.cur_bands.resize(layers * bands, 0);
    });
}

/// One layer of pass 1 is done: hash its bands. `arms` are the non-growing
/// arms of the layer as (cell, value) with the value folded from the arm's
/// ledger slot by [`slot_value`].
pub(crate) fn layer_done(
    layer: usize,
    cols: usize,
    load_out: &[f64],
    carriers: &[bool],
    arms: &[(usize, u64)],
) {
    perf::with_pose_slot(|probe: &mut Probe| {
        let bands = probe.bands;
        if bands == 0 || probe.cur_bands.len() < (layer + 1) * bands || cols == 0 {
            return;
        }
        let cells = load_out.len();
        let mut arm_acc = vec![0_u64; bands];
        let mut arm_any = vec![false; bands];
        for &(cell, value) in arms {
            let band = (cell / cols) / BAND_ROWS;
            if band < bands {
                arm_acc[band] = arm_acc[band].wrapping_add(mix(cell as u64, value));
                arm_any[band] = true;
            }
        }
        for band in 0..bands {
            let from = band * BAND_ROWS * cols;
            let to = ((band + 1) * BAND_ROWS * cols).min(cells);
            let mut h = 0_u64;
            let mut loaded = false;
            for cell in from..to {
                let load = load_out[cell];
                if load != 0.0 {
                    h = mix(mix(h, cell as u64), load.to_bits());
                    loaded = true;
                }
                if carriers[cell] {
                    h = mix(h, cell as u64 ^ 0xC0DE);
                }
            }
            probe.cur_bands[layer * bands + band] = if loaded || arm_any[band] {
                mix(h, arm_acc[band]) | 1
            } else {
                0
            };
        }
    });
}

/// A value for an arm's ledger slot, for [`layer_done`].
pub(crate) fn slot_value(carried_mm2: f64, radius_mm: f64) -> u64 {
    mix(carried_mm2.to_bits(), radius_mm.to_bits())
}

/// Pass 1 of the run has finished: compare the bands, charge the counters and
/// write the run's row.
pub(crate) fn end_first_pass() {
    perf::with_pose_slot(|probe: &mut Probe| {
        let Some(p) = probe.pending.take() else {
            return;
        };
        let (mut total, mut equal) = (0_u64, 0_u64);
        let bands = probe.bands;
        if !p.first && bands > 0 && probe.prev_bands.len() == probe.cur_bands.len() {
            // Layers strictly under the highest changed one.
            let below = p.dirty_layer.saturating_sub(1);
            for layer in 0..below.min(p.layers) {
                for band in 0..bands {
                    let (was, is) = (
                        probe.prev_bands[layer * bands + band],
                        probe.cur_bands[layer * bands + band],
                    );
                    if was != 0 || is != 0 {
                        total += 1;
                        equal += u64::from(was == is);
                    }
                }
            }
        }
        if bands > 0 {
            probe.prev_bands = std::mem::take(&mut probe.cur_bands);
        }
        let unchanged = !p.first && p.dirty_member == 0 && p.dirty_layer == 0;
        perf::add(perf::Work::ReuseRuns, 1);
        perf::add(perf::Work::ReuseFirstRuns, u64::from(p.first));
        perf::add(perf::Work::ReuseUnchangedRuns, u64::from(unchanged));
        perf::add(perf::Work::ReuseLayers, perf::count(p.layers));
        if !p.first {
            perf::add(
                perf::Work::ReuseLayersAboveMember,
                perf::count(p.layers.saturating_sub(p.dirty_member)),
            );
            perf::add(
                perf::Work::ReuseLayersAboveLayer,
                perf::count(p.layers.saturating_sub(p.dirty_layer)),
            );
        }
        perf::add(perf::Work::ReuseBandsTotal, total);
        perf::add(perf::Work::ReuseBandsEqual, equal);
        perf::row(
            perf::Rows::DownReuse,
            &[
                p.site,
                perf::count(p.layers),
                perf::count(p.cells),
                u64::from(p.first),
                perf::count(p.dirty_member),
                perf::count(p.dirty_layer),
                total,
                equal,
            ],
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{member_print, mix};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    fn strut(id: u32, k_top: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell: 10 + id as usize,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_birth_cells: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        }
    }

    /// The fingerprint sees every input the sweep reads and none of its
    /// outputs (a sized ledger must not look like an edit).
    #[test]
    fn a_member_print_moves_with_inputs_and_not_with_outputs() {
        let base = strut(3, 20);
        let mut sized = base.clone();
        sized.radius_mm = vec![0.5; 20];
        sized.carried_mm2 = vec![1.0; 20];
        assert_eq!(member_print(&base), member_print(&sized), "outputs are not inputs");
        let mut moved = base.clone();
        moved.cell += 1;
        assert_ne!(member_print(&base), member_print(&moved));
        let mut taller = base.clone();
        taller.k_top += 1;
        assert_ne!(member_print(&base), member_print(&taller));
        let mut thick = base.clone();
        thick.thickened_mm = vec![0.2; 4];
        assert_ne!(member_print(&base), member_print(&thick));
        assert_ne!(mix(1, 2), mix(2, 1));
    }
}
