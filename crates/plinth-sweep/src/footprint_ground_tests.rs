//! [`super::keeps_ground`] against the check as it stood at c941e69 (fix
//! lane A), kept verbatim below as the oracle for the speed lane's two exact
//! cuts (2026-10-13): the scan for lost cells keeps to the placed ball's
//! square, and a walk stops at its first ground.

use super::{keeps_ground, Ground, GroundWindow, Slid};
use crate::grid::lay_out_at;

const PITCH: f64 = 0.05;
const H: f64 = 0.05;
const SIDE: usize = 100;
/// The layer the material's first laminae cure around.
const K: usize = 50;

/// The check as it stood at c941e69, verbatim (the oracle).
fn keeps_ground_whole_walk(ground: &Ground<'_>, placed: Slid, moved: Slid) -> bool {
    let before = placed.ungrown();
    let after = moved.ungrown();
    let grid = ground.grid;
    let pitch = grid.pitch_mm();
    if !(pitch > 0.0 && ground.height > 0.0) || grid.cells() == 0 {
        return true;
    }
    let travel = (after.0[0] - before.0[0]).hypot(after.0[1] - before.0[1]);
    let reach = before.1 + after.1 + travel + 2.0 * pitch;
    let [cx, cy] = [before.0[0], before.0[1]];
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "raster coordinates of a window a few ball radii across are small, clamped to the grid"
    )]
    let span = |coord: f64, count: usize| -> (usize, usize) {
        let lo = (coord - reach / pitch).floor().max(0.0) as usize;
        let hi = ((coord + reach / pitch).ceil().max(0.0) as usize).min(count.saturating_sub(1));
        (lo, hi)
    };
    let (c0, c1) = span(grid.col_coord(cx), grid.cols());
    let (r0, r1) = span(grid.row_coord(cy), grid.rows());
    if c1 < c0 || r1 < r0 {
        return true;
    }
    let width = c1 - c0 + 1;
    let local = |row: usize, col: usize| (row - r0) * width + (col - c0);
    let in_window = |cell: usize| {
        let [x, y] = ground.centre_of(cell);
        let (row, col) = (grid.row_of(cell), grid.col_of(cell));
        (r0..=r1).contains(&row)
            && (c0..=c1).contains(&col)
            && (x - cx).hypot(y - cy) <= reach
            && ground.in_store(cell)
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "layer indices of a ball's height over the plate are small and clamped at zero"
    )]
    let layer_of = |z: f64| ((z - ground.plate_z) / ground.height).max(0.0) as usize;
    let lowest = layer_of(before.0[2] - before.1).saturating_sub(1);
    let highest = layer_of(before.0[2] + before.1) + 1;
    // Which walk reached each cell of the layer (0: none). Every walk that
    // ended before this one found ground (an ungrounded one returns at once),
    // so a cell an EARLIER walk reached is ground for this one.
    let mut seen = vec![0_u32; width * (r1 - r0 + 1)];
    let mut walk = 0_u32;
    let mut queue: Vec<usize> = Vec::new();
    for layer in lowest..=highest {
        seen.iter_mut().for_each(|stamp| *stamp = 0);
        for row in r0..=r1 {
            for col in c0..=c1 {
                let cell = grid.index(row, col);
                if seen[local(row, col)] != 0
                    || !ground.may_be_model(cell, layer)
                    || !ground.holds(before, cell, layer)
                    || ground.holds(after, cell, layer)
                {
                    continue;
                }
                // A cell the placed ball held and the moved one does not:
                // the component of its layer it lies in must be held some
                // other way. The whole component is walked, so a later lost
                // cell in it is already decided.
                let mut grounded = false;
                walk += 1;
                queue.clear();
                queue.push(cell);
                seen[local(row, col)] = walk;
                // A cell where model only MAY stand is the uncertain edge of
                // whatever surely stands within the raster's own rounding band
                // (`cover` rounds out one cell, `carry` in one and erodes one
                // more in Y): it belongs to that material, so the walk starts
                // from it too. Without this the rim ring of a flat disc's
                // first lamina joined nothing, and every rim slide of the
                // flipped Round 25 was refused.
                let band: isize = if ground.is_model(cell, layer) { 0 } else { 2 };
                for dr in -band..=band {
                    for dc in -band..=band {
                        let (Some(nr), Some(nc)) =
                            (row.checked_add_signed(dr), col.checked_add_signed(dc))
                        else {
                            continue;
                        };
                        if !((r0..=r1).contains(&nr) && (c0..=c1).contains(&nc)) {
                            continue;
                        }
                        let near = grid.index(nr, nc);
                        if !(ground.is_model(near, layer) && in_window(near)) {
                            continue;
                        }
                        match seen[local(nr, nc)] {
                            0 => {
                                seen[local(nr, nc)] = walk;
                                queue.push(near);
                            }
                            stamp if stamp != walk => grounded = true,
                            _ => {}
                        }
                    }
                }
                while let Some(at) = queue.pop() {
                    if (layer > 0 && ground.is_model(at, layer - 1)) || ground.holds(after, at, layer) {
                        grounded = true;
                    }
                    let (ar, ac) = (grid.row_of(at), grid.col_of(at));
                    for dr in -1_isize..=1 {
                        for dc in -1_isize..=1 {
                            if dr == 0 && dc == 0 {
                                continue;
                            }
                            let (Some(nr), Some(nc)) =
                                (ar.checked_add_signed(dr), ac.checked_add_signed(dc))
                            else {
                                continue;
                            };
                            if nr >= grid.rows() || nc >= grid.cols() {
                                continue;
                            }
                            let next = grid.index(nr, nc);
                            if !ground.is_model(next, layer) {
                                continue;
                            }
                            if !in_window(next) {
                                // It runs past everything the slide touched.
                                grounded = true;
                                continue;
                            }
                            let slot = local(nr, nc);
                            match seen[slot] {
                                0 => {
                                    seen[slot] = walk;
                                    queue.push(next);
                                }
                                stamp if stamp != walk => grounded = true,
                                _ => {}
                            }
                        }
                    }
                }
                if !grounded {
                    return false;
                }
            }
        }
    }
    true
}

/// A small deterministic generator (xorshift64*), so the cases are the same
/// on every run.
struct Cases(u64);

impl Cases {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    #[expect(clippy::cast_precision_loss, reason = "a 53-bit fraction")]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// One blob of model: centre, radius, the layers it spans, and whether a
/// two-cell `cover`-only rim rings it (the raster's rounding band).
type Blob = ([f64; 2], f64, usize, usize, bool);

/// A 5 x 5 mm window over layers `K - 14 ..= K + 2`, the blobs rasterised.
fn window(blobs: &[Blob]) -> GroundWindow {
    let (layer0, layers) = (K - 14, 17);
    let mut bits = vec![0; SIDE * SIDE * layers];
    for k in 0..layers {
        let layer = layer0 + k;
        for r in 0..SIDE {
            for c in 0..SIDE {
                #[expect(clippy::cast_precision_loss, reason = "small grid indices")]
                let (x, y) = ((c as f64 + 0.5) * PITCH, (r as f64 + 0.5) * PITCH);
                let mut cell = 0;
                for &(centre, radius, from, to, ragged) in blobs {
                    if !(from..to).contains(&layer) {
                        continue;
                    }
                    let rho = (x - centre[0]).hypot(y - centre[1]);
                    if rho <= radius {
                        cell |= GroundWindow::COVER | GroundWindow::CARRY;
                    } else if ragged && rho <= radius + 2.0 * PITCH {
                        cell |= GroundWindow::COVER;
                    }
                }
                bits[(k * SIDE + r) * SIDE + c] = cell;
            }
        }
    }
    GroundWindow {
        row0: 0,
        col0: 0,
        rows: SIDE,
        cols: SIDE,
        layer0,
        layers,
        bits,
    }
}

/// A 5 x 5 mm window over layers `K - 14 ..= K + 2`, each cell's bits as
/// `bits(row, col, layer)` gives them.
fn window_of(bits: impl Fn(usize, usize, usize) -> u8) -> GroundWindow {
    let (layer0, layers) = (K - 14, 17);
    let mut cells = vec![0; SIDE * SIDE * layers];
    for k in 0..layers {
        for r in 0..SIDE {
            for c in 0..SIDE {
                cells[(k * SIDE + r) * SIDE + c] = bits(r, c, layer0 + k);
            }
        }
    }
    GroundWindow {
        row0: 0,
        col0: 0,
        rows: SIDE,
        cols: SIDE,
        layer0,
        layers,
        bits: cells,
    }
}

#[test]
fn a_stopped_walk_still_stamps_every_piece_it_started_from() {
    // Layer K under a ball at (2.5, 2.5) that slides 1.5 mm away: a 2 x 2
    // block A carried by a column of model under it (scanned first; its walk
    // is grounded at once), a lone `cover` cell X between A and an island B
    // that nothing carries, within the raster's two-cell band of both. X's
    // walk starts from A's and B's cells and is grounded by A's stamp. B is
    // X's material, so the slide keeps its ground. A walk that stopped
    // before it had stamped every cell it started from would leave B
    // unstamped, and B's own walk would then find nothing under it.
    let both = GroundWindow::COVER | GroundWindow::CARRY;
    let masks = window_of(|r, c, layer| {
        let a = (46..48).contains(&r) && (46..48).contains(&c) && layer <= K;
        let b = (50..52).contains(&r) && (50..52).contains(&c) && layer == K;
        let x = r == 48 && c == 48 && layer == K;
        if a || b {
            both
        } else if x {
            GroundWindow::COVER
        } else {
            0
        }
    });
    let grid = lay_out_at(0.0, 0.0, PITCH, SIDE, SIDE);
    let ground = Ground {
        grid: &grid,
        window: &masks,
        plate_z: 0.0,
        height: H,
    };
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    let lamina = K as f64 * H;
    let ball = |x: f64| Slid {
        ball_centre: [x, 2.5, lamina - 0.2],
        ball_radius: 0.4,
        planned_ball_radius: 0.4,
        disc_centre: [x, 2.5, lamina - 0.2],
        disc_radius: 0.3,
        normal: [0.0, 0.0, -1.0],
    };
    let (placed, moved) = (ball(2.5), ball(4.0));
    assert!(keeps_ground_whole_walk(&ground, placed, moved), "premise: the whole walk keeps it");
    assert!(keeps_ground(&ground, placed, moved));
}

#[test]
fn the_cut_check_agrees_with_the_whole_walk_on_random_ground() {
    // Random blobs of model (some ringed by `cover`-only rims, some carried
    // from below and some not) under random slides of a ball whose size,
    // height and travel vary, a grown ball among them: the cut check answers
    // as the whole-walk check does on every case, and both answers occur.
    let grid = lay_out_at(0.0, 0.0, PITCH, SIDE, SIDE);
    let mut cases = Cases(0x9E37_79B9_7F4A_7C15);
    let (mut kept, mut refused) = (0_usize, 0_usize);
    for _ in 0..1500 {
        let count = 1 + cases.next() % 5;
        let blobs: Vec<Blob> = (0..count)
            .map(|_| {
                let centre = [1.5 + 2.0 * cases.unit(), 1.5 + 2.0 * cases.unit()];
                let radius = 0.05 + 0.5 * cases.unit();
                let from = K - 8 + usize::try_from(cases.next() % 10).unwrap_or(0);
                let to = from + 1 + usize::try_from(cases.next() % 6).unwrap_or(0);
                (centre, radius, from, to, cases.next().is_multiple_of(3))
            })
            .collect();
        let masks = window(&blobs);
        let ground = Ground {
            grid: &grid,
            window: &masks,
            plate_z: 0.0,
            height: H,
        };
        #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
        let lamina = K as f64 * H;
        let radius = 0.15 + 0.35 * cases.unit();
        let grown = radius * (1.0 + 0.3 * cases.unit());
        let (x, y) = (1.5 + 2.0 * cases.unit(), 1.5 + 2.0 * cases.unit());
        let z = lamina - 0.35 + 0.4 * cases.unit();
        let angle = std::f64::consts::TAU * cases.unit();
        let travel = 0.6 * cases.unit();
        let slid = |dx: f64, dy: f64| Slid {
            ball_centre: [x + dx, y + dy, z],
            ball_radius: grown,
            planned_ball_radius: radius,
            disc_centre: [x + dx, y + dy, z],
            disc_radius: 0.8 * radius,
            normal: [0.0, 0.0, -1.0],
        };
        let (placed, moved) = (slid(0.0, 0.0), slid(travel * angle.cos(), travel * angle.sin()));
        let cut = keeps_ground(&ground, placed, moved);
        assert_eq!(
            cut,
            keeps_ground_whole_walk(&ground, placed, moved),
            "ball r {radius} at ({x}, {y}, {z}) slid {travel} at {angle} over {blobs:?}"
        );
        if cut {
            kept += 1;
        } else {
            refused += 1;
        }
    }
    assert!(
        kept > 100 && refused > 100,
        "both answers must be exercised: kept {kept}, refused {refused}"
    );
}
