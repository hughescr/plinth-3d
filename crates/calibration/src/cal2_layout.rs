//! CAL-2 plate layout: three front-to-back zones, shelf packing, seeded shuffle.
//! Copies of one cell go to different zones. Positions use measured footprints (body and
//! supports), and every move is a whole owner-grid pixel step.
use crate::{Result, cal2_spec::Item, goo_grid::Grid, plate, plate::Random};
use plinth_chitubox::Model;

/// Plate edge margin and the three zones' extra separation, mm. The margin is the required
/// 8 mm plus 0.1 mm, because a part moves by whole pixel steps (under 0.05 mm each way).
const MARGIN: f64 = plate::EDGE_MARGIN + 0.1;
const SEP: f64 = plate::GAP + 0.1;
pub const ZONES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    pub width: f64,
    pub height: f64,
    pub centre: [f64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub item: usize,
    pub zone: usize,
    pub shelf: usize,
    /// Footprint centre on the plate, mm.
    pub centre: [f64; 2],
}

#[derive(Debug)]
pub struct Plan {
    pub slots: Vec<Slot>,
    /// Items removed to make the plate fit, in removal order.
    pub cut: Vec<usize>,
}

#[must_use]
pub fn footprint(m: &Model) -> Footprint {
    let b = plate::bounds(m);
    Footprint {
        width: b.hi[0] - b.lo[0],
        height: b.hi[1] - b.lo[1],
        centre: b.centre(),
    }
}

fn cell(i: &Item) -> (usize, u64, u64) {
    (i.row, i.neck_mm.to_bits(), i.ratio.to_bits())
}

/// Rows of this priority and better never go below two copies per cell.
const PROTECTED: usize = 5;

/// Removal order for a plate that does not fit: lowest-priority row first. Rows of priority
/// 1 to 5 never go below two copies per cell; later rows drop their extra copies, then the
/// rest. Within a tier the largest footprint goes first.
#[must_use]
pub fn cut_order(items: &[Item], foot: &[Footprint]) -> Vec<usize> {
    let rows = items.iter().map(|i| i.row).max().map_or(0, |r| r + 1);
    let mut out = Vec::new();
    for row in (0..rows).rev() {
        for tier in 0..2 {
            let mut group: Vec<usize> = (0..items.len())
                .filter(|&k| {
                    let it = &items[k];
                    it.row == row
                        && match (it.priority <= PROTECTED, tier) {
                            (true, 0) => it.copy >= 2,
                            (true, _) => false,
                            (false, 0) => it.copy >= 1,
                            (false, _) => it.copy == 0,
                        }
                })
                .collect();
            group.sort_by(|&a, &b| {
                let area = |k: usize| foot[k].width * foot[k].height;
                area(b).total_cmp(&area(a)).then(a.cmp(&b))
            });
            out.extend(group);
        }
    }
    out
}

/// Random-order greedy: each coupon goes to the least-loaded zone not yet holding a copy
/// of its cell (ties broken by the seeded stream).
fn assign_zones(
    active: &[usize],
    items: &[Item],
    foot: &[Footprint],
    rng: &mut Random,
) -> Vec<usize> {
    let mut order = active.to_vec();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.below(i + 1));
    }
    let mut load = [0.0_f64; ZONES];
    let mut used: std::collections::BTreeMap<_, Vec<usize>> = std::collections::BTreeMap::new();
    let mut zone = vec![0; items.len()];
    for k in order {
        let taken = used.entry(cell(&items[k])).or_default();
        let mut free: Vec<usize> = (0..ZONES).filter(|z| !taken.contains(z)).collect();
        if free.is_empty() {
            free = (0..ZONES).collect();
        }
        let low = free.iter().map(|&z| load[z]).fold(f64::INFINITY, f64::min);
        free.retain(|&z| (load[z] - low).abs() < 1e-9);
        let z = free[rng.below(free.len())];
        taken.push(z);
        load[z] += (foot[k].width + SEP) * (foot[k].height + SEP);
        zone[k] = z;
    }
    zone
}

struct Shelf {
    height: f64,
    used: f64,
    items: Vec<usize>,
}

fn pack_zone(members: &[usize], foot: &[Footprint]) -> Option<Vec<Shelf>> {
    let width = plate::BUILD[0] - 2.0 * MARGIN;
    let mut order = members.to_vec();
    order.sort_by(|&a, &b| foot[b].height.total_cmp(&foot[a].height).then(a.cmp(&b)));
    let mut shelves: Vec<Shelf> = Vec::new();
    for k in order {
        if foot[k].width > width {
            return None;
        }
        let fit = shelves
            .iter_mut()
            .find(|s| foot[k].height <= s.height && s.used + SEP + foot[k].width <= width);
        if let Some(s) = fit {
            s.used += SEP + foot[k].width;
            s.items.push(k);
        } else {
            shelves.push(Shelf {
                height: foot[k].height,
                used: foot[k].width,
                items: vec![k],
            });
        }
    }
    Some(shelves)
}

/// Draws of the seeded stream tried per plate before declaring it does not fit.
const ATTEMPTS: usize = 400;

/// Zones with their shelves, if the total height fits. Zone heights are free; only their
/// sum, with a full separation between neighbouring shelves, must fit the plate.
fn pack_attempt(
    active: &[usize],
    items: &[Item],
    foot: &[Footprint],
    rng: &mut Random,
) -> Option<Vec<Vec<Shelf>>> {
    let zone = assign_zones(active, items, foot, rng);
    let mut zones = Vec::new();
    for z in 0..ZONES {
        let members: Vec<usize> = active.iter().copied().filter(|&k| zone[k] == z).collect();
        zones.push(pack_zone(&members, foot)?);
    }
    let shelves: Vec<&Shelf> = zones.iter().flatten().collect();
    let need: f64 = shelves.iter().map(|s| s.height).sum::<f64>()
        + SEP * shelves.len().saturating_sub(1) as f64;
    (need <= plate::BUILD[1] - 2.0 * MARGIN).then_some(zones)
}

fn try_pack(
    active: &[usize],
    items: &[Item],
    foot: &[Footprint],
    seed: u64,
) -> Result<Option<Vec<Slot>>> {
    if seed == 0 {
        return Err("xorshift seed must not be zero".into());
    }
    let mut rng = Random(seed);
    let Some(zones) = (0..ATTEMPTS).find_map(|_| pack_attempt(active, items, foot, &mut rng))
    else {
        return Ok(None);
    };
    let width = plate::BUILD[0] - 2.0 * MARGIN;
    let count: usize = zones.iter().map(Vec::len).sum();
    let used: f64 = zones.iter().flatten().map(|s| s.height).sum();
    let usable = plate::BUILD[1] - 2.0 * MARGIN;
    // Spread any spare height evenly over the gaps between shelves.
    let gap_y = if count > 1 {
        ((usable - used) / (count - 1) as f64).max(SEP)
    } else {
        0.0
    };
    let mut slots = Vec::new();
    let mut y = -plate::BUILD[1] / 2.0 + MARGIN;
    for (z, shelves) in zones.into_iter().enumerate() {
        for (n, mut shelf) in shelves.into_iter().enumerate() {
            for i in (1..shelf.items.len()).rev() {
                shelf.items.swap(i, rng.below(i + 1));
            }
            let sum: f64 = shelf.items.iter().map(|&k| foot[k].width).sum();
            let gaps = shelf.items.len().saturating_sub(1);
            let gap = if gaps == 0 {
                0.0
            } else {
                ((width - sum) / gaps as f64).max(SEP)
            };
            let mut x = if gaps == 0 {
                -sum / 2.0
            } else {
                -plate::BUILD[0] / 2.0 + MARGIN
            };
            for &k in &shelf.items {
                slots.push(Slot {
                    item: k,
                    zone: z,
                    shelf: n,
                    centre: [x + foot[k].width / 2.0, y + shelf.height / 2.0],
                });
                x += foot[k].width + gap;
            }
            y += shelf.height + gap_y;
        }
    }
    Ok(Some(slots))
}

/// Pack every item, cutting copies in `cut_order` until the plate fits.
///
/// # Errors
/// A zero seed, or nothing fits even after every allowed cut.
pub fn plan(items: &[Item], foot: &[Footprint], seed: u64) -> Result<Plan> {
    let order = cut_order(items, foot);
    for k in 0..=order.len() {
        let active: Vec<usize> = (0..items.len())
            .filter(|i| !order[..k].contains(i))
            .collect();
        if let Some(slots) = try_pack(&active, items, foot, seed)? {
            return Ok(Plan {
                slots,
                cut: order[..k].to_vec(),
            });
        }
    }
    Err("no layout fits even after every allowed cut".into())
}

/// Move a model's footprint centre to `target` by whole pixel steps of the owner grid, so
/// the source raster phase is kept.
pub fn move_to(m: &mut Model, target: [f64; 2], grid: Grid) {
    let c = plate::bounds(m).centre();
    for k in 0..2 {
        let delta = ((target[k] - c[k]) / grid.pitch[k]).round() * grid.pitch[k];
        m.translate[k] = (f64::from(m.translate[k]) + delta) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cal2_spec::{Kind, Variant};

    /// Test rows are numbered from 0; the row's priority is its number plus one.
    fn item(row: usize, ratio: f64, copy: usize, copies: usize) -> Item {
        Item {
            row,
            priority: row + 1,
            kind: Kind::Head,
            variant: Variant::Standard,
            neck_mm: 0.4,
            heads: 4,
            ratio,
            copy,
            copies,
        }
    }
    fn fp(w: f64, h: f64) -> Footprint {
        Footprint {
            width: w,
            height: h,
            centre: [0.0; 2],
        }
    }

    #[test]
    fn copies_of_a_cell_land_in_different_zones_and_clear_by_the_gap() {
        let items: Vec<_> = (0..3).map(|c| item(0, 30.0, c, 3)).collect();
        let foot = vec![fp(20.0, 20.0); 3];
        let p = plan(&items, &foot, 7).unwrap();
        let mut zones: Vec<_> = p.slots.iter().map(|s| s.zone).collect();
        zones.sort_unstable();
        assert_eq!(zones, [0, 1, 2]);
        assert!(p.cut.is_empty());
    }

    #[test]
    fn layout_is_reproducible_and_seed_dependent() {
        let items: Vec<_> = (0..12).map(|i| item(0, 30.0 + i as f64, 0, 1)).collect();
        let foot: Vec<_> = (0..12).map(|i| fp(10.0 + i as f64, 12.0)).collect();
        let a = plan(&items, &foot, 11).unwrap();
        let b = plan(&items, &foot, 11).unwrap();
        let c = plan(&items, &foot, 12).unwrap();
        assert_eq!(a.slots, b.slots);
        assert_ne!(a.slots, c.slots);
        assert!(plan(&items, &foot, 0).is_err());
    }

    #[test]
    fn every_pair_keeps_the_gap_and_stays_on_the_plate() {
        let items: Vec<_> = (0..20).map(|i| item(i % 3, 30.0, i, 20)).collect();
        let foot: Vec<_> = (0..20)
            .map(|i| fp(9.0 + (i % 5) as f64 * 2.0, 10.0 + (i % 4) as f64))
            .collect();
        let p = plan(&items, &foot, 3).unwrap();
        let boxes: Vec<_> = p.slots.iter().map(|s| (s.centre, foot[s.item])).collect();
        for (i, (c, f)) in boxes.iter().enumerate() {
            let m = plate::EDGE_MARGIN;
            assert!(c[0] - f.width / 2.0 >= -plate::BUILD[0] / 2.0 + m);
            assert!(c[0] + f.width / 2.0 <= plate::BUILD[0] / 2.0 - m);
            assert!(c[1] - f.height / 2.0 >= -plate::BUILD[1] / 2.0 + m);
            assert!(c[1] + f.height / 2.0 <= plate::BUILD[1] / 2.0 - m);
            for (d, g) in &boxes[i + 1..] {
                let dx = ((c[0] - d[0]).abs() - (f.width + g.width) / 2.0).max(0.0);
                let dy = ((c[1] - d[1]).abs() - (f.height + g.height) / 2.0).max(0.0);
                assert!(dx.hypot(dy) >= plate::GAP, "gap {}", dx.hypot(dy));
            }
        }
    }

    #[test]
    fn overfull_plate_cuts_the_lowest_priority_rows_first_and_never_below_two_copies() {
        let mut items = Vec::new();
        for c in 0..3 {
            items.push(item(0, 30.0, c, 3));
            items.push(item(1, 30.0, c, 3));
        }
        for c in 0..2 {
            items.push(item(5, 30.0, c, 2));
            items.push(item(6, 30.0, c, 2));
        }
        items.push(item(8, 6.0, 0, 1));
        // Three zones with one shelf of three each hold nine of these eleven.
        let foot = vec![fp(50.0, 28.0); items.len()];
        let p = plan(&items, &foot, 5).unwrap();
        assert!(!p.cut.is_empty());
        // The witness (row 8) goes first, then row 6 before any row 5, 1 or 0 item.
        assert_eq!(items[p.cut[0]].row, 8);
        assert_eq!(p.cut.len(), 2);
        let rows: Vec<_> = p.cut.iter().map(|&k| items[k].row).collect();
        assert!(rows.windows(2).all(|w| w[0] >= w[1]), "{rows:?}");
        // Rows of priority 1 and 2 keep at least two copies of every cell.
        for row in 0..2 {
            for c in 0..2 {
                let k = items
                    .iter()
                    .position(|i| i.row == row && i.copy == c)
                    .unwrap();
                assert!(!p.cut.contains(&k), "row {row} copy {c} was cut");
            }
        }
    }

    #[test]
    fn nothing_fits_is_an_error_not_a_silent_drop_of_protected_rows() {
        let items: Vec<_> = (0..6).map(|c| item(0, 30.0, c % 2, 2)).collect();
        let foot = vec![fp(150.0, 100.0); 6];
        assert!(plan(&items, &foot, 5).is_err());
    }
}
