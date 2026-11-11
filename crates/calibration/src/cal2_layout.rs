//! CAL-2 plate layout: three front-to-back zones, shelf packing, seeded shuffle.
//! Copies of one cell go to different zones. Positions use measured footprints (body and
//! supports), and every move is a whole owner-grid pixel step.
use crate::{
    Result,
    cal2_spec::{Item, Kind},
    goo_grid::Grid,
    plate,
    plate::Random,
};
use plinth_chitubox::Model;

/// Plate edge margin, mm: the required 8 mm plus 0.1 mm, because a part moves by whole
/// pixel steps (under 0.05 mm each way). The same 0.1 mm is added to the part-to-part gap.
const MARGIN: f64 = plate::EDGE_MARGIN + 0.1;
const SLACK: f64 = 0.1;
pub const ZONES: usize = 3;
/// Part-to-part clearances the layout may use, mm, widest first. 6 mm is the design value;
/// the plan goes down to 5 mm only if that saves a coupon (a control or a second 1.20 mm
/// R70 column).
pub const GAPS: [f64; 5] = [plate::GAP, 5.75, 5.5, 5.25, 5.0];

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
    /// Part-to-part clearance the layout was packed for, mm (at most 6, at least 5).
    pub gap: f64,
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

/// Rows of this priority and better (R1 to R6) never go below two copies per cell.
const PROTECTED: usize = 6;
/// Ratio from which a 1.20 mm column cell is the first thing cut (R70). The columns have the
/// biggest bodies and the least to say; the controls and the lower column ratio outrank them.
const COLUMN_HIGH_RATIO: f64 = 70.0;

/// Coupons that never go below two copies per cell: rows of priority 1 to 5, and the
/// 1.20 mm columns below R70.
fn keeps_two(it: &Item) -> bool {
    it.priority <= PROTECTED || (it.kind == Kind::Column && it.ratio < COLUMN_HIGH_RATIO)
}

/// Removal order for a plate that does not fit. Formation witnesses are never cut (they are
/// small and go into gaps, see `add_witnesses`). First the high-ratio column copies (R70;
/// extra copies, then the rest), then the other lower-priority rows from the bottom up (the
/// 4-head controls: extra copies, then the rest). Rows that keep two copies lose nothing.
/// Within a tier the largest footprint goes first.
#[must_use]
pub fn cut_order(items: &[Item], foot: &[Footprint]) -> Vec<usize> {
    let rows = items.iter().map(|i| i.row).max().map_or(0, |r| r + 1);
    let high_column = |it: &Item| it.kind == Kind::Column && it.ratio >= COLUMN_HIGH_RATIO;
    let mut out = Vec::new();
    // Pass 0: the high-ratio columns. Pass 1: everything else that may go.
    for pass in 0..2 {
        for row in (0..rows).rev() {
            for tier in 0..2 {
                let mut group: Vec<usize> = (0..items.len())
                    .filter(|&k| {
                        let it = &items[k];
                        let cuttable = it.kind != Kind::Witness
                            && it.row == row
                            && (pass == 0) == high_column(it);
                        let extra = if keeps_two(it) { 2 } else { 1 };
                        cuttable
                            && match tier {
                                0 => it.copy >= extra,
                                _ => it.copy < extra && !keeps_two(it),
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
    }
    out
}

/// Random-order greedy: each coupon goes to the least-loaded zone not yet holding a copy
/// of its cell (ties broken by the seeded stream).
fn assign_zones(
    active: &[usize],
    items: &[Item],
    foot: &[Footprint],
    sep: f64,
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
        load[z] += (foot[k].width + sep) * (foot[k].height + sep);
        zone[k] = z;
    }
    zone
}

struct Shelf {
    height: f64,
    used: f64,
    items: Vec<usize>,
}

fn pack_zone(members: &[usize], foot: &[Footprint], sep: f64) -> Option<Vec<Shelf>> {
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
            .find(|s| foot[k].height <= s.height && s.used + sep + foot[k].width <= width);
        if let Some(s) = fit {
            s.used += sep + foot[k].width;
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

/// Put each formation witness into a gap: the shelf with the most free width that can hold
/// it, trying a different front-to-back zone for each witness first (so they spread front,
/// middle, back). A witness that fits no shelf starts a shelf of its own (the caller then
/// checks the total height).
fn add_witnesses(
    zones: &mut [Vec<Shelf>],
    witnesses: &[usize],
    foot: &[Footprint],
    sep: f64,
    rng: &mut Random,
) {
    let width = plate::BUILD[0] - 2.0 * MARGIN;
    let first = rng.below(ZONES);
    for (n, &k) in witnesses.iter().enumerate() {
        let fit = (0..ZONES).map(|d| (first + n + d) % ZONES).find_map(|z| {
            let room = |s: &Shelf| width - s.used - sep - foot[k].width;
            zones[z]
                .iter()
                .enumerate()
                .filter(|(_, s)| foot[k].height <= s.height && room(s) >= 0.0)
                .max_by(|a, b| room(a.1).total_cmp(&room(b.1)))
                .map(|(i, _)| (z, i))
        });
        if let Some((z, i)) = fit {
            let s = &mut zones[z][i];
            s.used += sep + foot[k].width;
            s.items.push(k);
        } else {
            zones[(first + n) % ZONES].push(Shelf {
                height: foot[k].height,
                used: foot[k].width,
                items: vec![k],
            });
        }
    }
}

/// Zones with their shelves, if the total height fits. Zone heights are free; only their
/// sum, with a full separation between neighbouring shelves, must fit the plate.
fn pack_attempt(
    (active, witnesses): (&[usize], &[usize]),
    items: &[Item],
    foot: &[Footprint],
    sep: f64,
    rng: &mut Random,
) -> Option<Vec<Vec<Shelf>>> {
    let zone = assign_zones(active, items, foot, sep, rng);
    let mut zones = Vec::new();
    for z in 0..ZONES {
        let members: Vec<usize> = active.iter().copied().filter(|&k| zone[k] == z).collect();
        zones.push(pack_zone(&members, foot, sep)?);
    }
    add_witnesses(&mut zones, witnesses, foot, sep, rng);
    let shelves: Vec<&Shelf> = zones.iter().flatten().collect();
    let need: f64 = shelves.iter().map(|s| s.height).sum::<f64>()
        + sep * shelves.len().saturating_sub(1) as f64;
    (need <= plate::BUILD[1] - 2.0 * MARGIN).then_some(zones)
}

/// Pack `active` coupons and `witnesses` with a gap of `gap` mm; `None` if they do not fit.
fn try_pack(
    (active, witnesses): (&[usize], &[usize]),
    items: &[Item],
    foot: &[Footprint],
    gap: f64,
    seed: u64,
) -> Result<Option<Vec<Slot>>> {
    if seed == 0 {
        return Err("xorshift seed must not be zero".into());
    }
    let sep = gap + SLACK;
    let mut rng = Random(seed);
    let Some(zones) =
        (0..ATTEMPTS).find_map(|_| pack_attempt((active, witnesses), items, foot, sep, &mut rng))
    else {
        return Ok(None);
    };
    let width = plate::BUILD[0] - 2.0 * MARGIN;
    let count: usize = zones.iter().map(Vec::len).sum();
    let used: f64 = zones.iter().flatten().map(|s| s.height).sum();
    let usable = plate::BUILD[1] - 2.0 * MARGIN;
    // Spread any spare height evenly over the gaps between shelves.
    let gap_y = if count > 1 {
        ((usable - used) / (count - 1) as f64).max(sep)
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
                ((width - sum) / gaps as f64).max(sep)
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

/// Pack every item. Formation witnesses are always placed, into gaps between the larger
/// coupons. If the rest does not fit, coupons are cut in `cut_order`. For each clearance in
/// [`GAPS`] (6 mm first) the fewest cuts that fit is found; the plan uses the widest
/// clearance that reaches the fewest cuts overall, so 5 mm is used only if it saves a
/// coupon.
///
/// # Errors
/// A zero seed, or nothing fits even after every allowed cut.
pub fn plan(items: &[Item], foot: &[Footprint], seed: u64) -> Result<Plan> {
    let order = cut_order(items, foot);
    let witnesses: Vec<usize> = (0..items.len())
        .filter(|&k| items[k].kind == Kind::Witness)
        .collect();
    let mut best: Option<Plan> = None;
    for gap in GAPS {
        for k in 0..=order.len() {
            if best.as_ref().is_some_and(|b| k >= b.cut.len()) {
                break;
            }
            let active: Vec<usize> = (0..items.len())
                .filter(|i| !order[..k].contains(i) && !witnesses.contains(i))
                .collect();
            if let Some(slots) = try_pack((&active, &witnesses), items, foot, gap, seed)? {
                best = Some(Plan {
                    slots,
                    cut: order[..k].to_vec(),
                    gap,
                });
                break;
            }
        }
    }
    best.ok_or_else(|| "no layout fits even after every allowed cut".into())
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
    use crate::cal2_spec::Variant;

    /// Test rows are numbered from 0; the row's priority is its number plus one.
    fn kinded(kind: Kind, row: usize, ratio: f64, copy: usize, copies: usize) -> Item {
        Item {
            row,
            priority: row + 1,
            kind,
            variant: Variant::Standard,
            neck_mm: 0.4,
            heads: 4,
            ratio,
            copy,
            copies,
        }
    }
    fn item(row: usize, ratio: f64, copy: usize, copies: usize) -> Item {
        kinded(Kind::Head, row, ratio, copy, copies)
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
                assert!(dx.hypot(dy) >= p.gap, "gap {}", dx.hypot(dy));
            }
        }
    }

    #[test]
    fn overfull_plate_cuts_high_columns_then_controls_never_witnesses_or_protected_rows() {
        let mut items = Vec::new();
        for c in 0..2 {
            items.push(item(0, 30.0, c, 2));
            items.push(item(1, 30.0, c, 2));
        }
        // Row 5 is priority 6 (R6): protected. Row 6 is the column row, row 7 the control.
        for c in 0..2 {
            items.push(item(5, 30.0, c, 2));
            items.push(kinded(Kind::Column, 6, 40.0, c, 2));
            items.push(kinded(Kind::Column, 6, 70.0, c, 2));
            items.push(item(7, 40.0, c, 2));
        }
        for k in 0..3 {
            let mut w = kinded(Kind::Witness, 8, 6.0, 0, 1);
            w.neck_mm = 0.4 + 0.15 * k as f64;
            items.push(w);
        }
        // Three zones with one shelf of three each hold nine of the 12 big coupons.
        let foot: Vec<_> = items
            .iter()
            .map(|i| match i.kind {
                Kind::Witness => fp(3.0, 3.0),
                _ => fp(50.0, 28.0),
            })
            .collect();
        let p = plan(&items, &foot, 5).unwrap();
        // Witnesses are all placed (never cut), one per front-to-back zone.
        for k in (0..items.len()).filter(|&k| items[k].kind == Kind::Witness) {
            assert!(!p.cut.contains(&k));
            assert!(p.slots.iter().any(|s| s.item == k));
        }
        let mut wz: Vec<_> = p
            .slots
            .iter()
            .filter(|s| items[s.item].kind == Kind::Witness)
            .map(|s| s.zone)
            .collect();
        wz.sort_unstable();
        assert_eq!(wz, [0, 1, 2]);
        // Cut order: both R70 columns (extra copy first), then the extra control copy.
        let cut: Vec<_> = p
            .cut
            .iter()
            .map(|&k| (items[k].kind, items[k].ratio, items[k].copy))
            .collect();
        assert_eq!(
            cut,
            [
                (Kind::Column, 70.0, 1),
                (Kind::Column, 70.0, 0),
                (Kind::Head, 40.0, 1)
            ]
        );
        // R1 to R6 and the R40 columns keep two copies of every cell.
        assert!(
            p.cut
                .iter()
                .all(|&k| items[k].row >= 6 && items[k].row != 8)
        );
        assert!(
            p.cut
                .iter()
                .all(|&k| items[k].kind != Kind::Column || items[k].ratio >= 70.0)
        );
    }

    #[test]
    fn a_narrower_gap_is_used_only_when_it_saves_a_coupon() {
        // Four across need 4 x 45 + 3 x 5.1 = 195.3 mm of the 195.68 mm usable width: that
        // fits at a 5.0 mm gap but not at 6.0 mm, which would need twice the shelves.
        let items: Vec<_> = (0..12)
            .map(|i| item(0, 30.0 + (i / 3) as f64, i % 3, 3))
            .collect();
        let foot = vec![fp(45.0, 30.0); 12];
        let p = plan(&items, &foot, 5).unwrap();
        assert!(p.cut.is_empty());
        assert!((p.gap - 5.0).abs() < 1e-9, "gap {}", p.gap);
        // The same plate with room to spare keeps the 6 mm design gap.
        let items: Vec<_> = (0..3).map(|c| item(0, 30.0, c, 3)).collect();
        let p = plan(&items, &[fp(20.0, 20.0); 3], 5).unwrap();
        assert!((p.gap - plate::GAP).abs() < 1e-9);
    }

    #[test]
    fn nothing_fits_is_an_error_not_a_silent_drop_of_protected_rows() {
        let items: Vec<_> = (0..6).map(|c| item(0, 30.0, c % 2, 2)).collect();
        let foot = vec![fp(150.0, 100.0); 6];
        assert!(plan(&items, &foot, 5).is_err());
    }
}
