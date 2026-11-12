//! Layer walk with a topology-only fallback when model slicing cannot be certified.
use super::*;

pub(super) fn run(scene: &Scene, patches: &PatchOut, reading: Reading) -> Result<Walked, String> {
    match attempt(scene, patches, reading, true) {
        Ok(w) => Ok(w),
        Err(why) => {
            let mut w = attempt(scene, patches, reading, false)?;
            w.relies_on_model.push(format!(
                "model slicing unavailable: {why}; model-standing feet retain reliance only"
            ));
            Ok(w)
        }
    }
}

fn support_sections(
    scene: &Scene,
    infos: &[PartInfo],
    k: usize,
    z: f64,
    reading: Reading,
) -> Vec<(usize, Sec)> {
    scene
        .parts
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            (infos[*i].k_lo..=infos[*i].k_hi).contains(&k)
                && z >= infos[*i].zmin - 1e-9
                && z <= infos[*i].zmax + 1e-9
        })
        .filter_map(|(i, p)| {
            let inner = EDGE_FACE_MM.min(0.5 * (infos[i].zmax - infos[i].zmin));
            let zc = if z < infos[i].zmin {
                infos[i].zmin + inner
            } else if z > infos[i].zmax {
                infos[i].zmax - inner
            } else {
                z
            };
            section(&p.tris, zc, reading.sliver).map(|s| (i, s))
        })
        .collect()
}

/// Support samples have priority and keep their exact height and provenance.
fn proof_planes(support: &[f64], mut extra: Vec<f64>) -> Vec<(f64, bool)> {
    extra.retain(|z| support.iter().all(|h| (z - h).abs() > ROUND_MM));
    extra.sort_by(f64::total_cmp);
    extra.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    let mut out: Vec<_> = support.iter().map(|&z| (z, true)).collect();
    out.extend(extra.into_iter().map(|z| (z, false)));
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn attempt(
    scene: &Scene,
    patches: &PatchOut,
    reading: Reading,
    material: bool,
) -> Result<Walked, String> {
    if let Some(why) = walk_refusal(scene) {
        return Err(why);
    }
    let z0 = scene.plate_z();
    let infos: Vec<PartInfo> = scene.parts.iter().map(|p| part_info(p, z0)).collect();
    if let Some((i, _)) = infos.iter().enumerate().find(|(_, i)| i.zmin < z0 - 1e-4) {
        return Err(format!(
            "record {} lies below the plate level",
            scene.parts[i].rec
        ));
    }
    let layers = infos.iter().map(|i| i.k_hi + 1).max().unwrap_or(0);
    if layers > MAX_LAYERS {
        return Err(format!(
            "the supports are {layers} layers tall, more than the {MAX_LAYERS} layers (1 m) the checker slices"
        ));
    }
    let feet = feet_of(scene, patches);
    let slice_model = material && !feet.is_empty();
    if slice_model && !scene.model_closed {
        return Err(
            "model-standing feet require a closed, oriented material mesh for slicing".to_owned(),
        );
    }
    let foot_starts = model::foot_events(scene, &feet, layers, z0, reading);
    let windows = if slice_model {
        model::windows(scene, &feet)
    } else {
        Vec::new()
    };
    let roots = Roots {
        infos: &infos,
        feet: &feet,
        reading,
    };
    let mut state = Walk {
        scene,
        z0,
        reported: BTreeSet::new(),
        located: Vec::new(),
        relies_on_model: Vec::new(),
        pieces: 0,
        unrooted_pieces: 0,
        planes_total: 0,
    };
    let mut all: Vec<Vec<Plane>> = Vec::with_capacity(layers);
    let mut stored_sections = 0usize;
    let (mut model_slice_batches, mut model_tiles) = (0usize, 0usize);
    for (k, starts) in foot_starts.iter().enumerate() {
        let mut planes: Vec<Plane> = Vec::new();
        let support_heights = layer_planes(&infos, k, z0);
        let mut extra = Vec::new();
        let bottom = z0 + k as f64 * LAYER_MM;
        let active: Vec<_> = windows
            .iter()
            .filter(|w| bottom <= w.top && bottom + LAYER_MM >= scene.model_min_z)
            .collect();
        if slice_model {
            for w in active.iter().filter(|w| w.can_root) {
                model::events(scene, bottom, bottom + LAYER_MM, w.xy, &mut extra);
            }
            extra.extend(starts.iter().map(|&(_, z)| z));
        }
        for (z, support_sample) in proof_planes(&support_heights, extra) {
            let mut secs = support_sections(scene, &infos, k, z, reading);
            if slice_model {
                let first_print = starts
                    .iter()
                    .any(|&(_, start)| (z - start).abs() <= ROUND_MM);
                for w in active.iter().filter(|w| w.can_root || first_print) {
                    let material = model::sections(scene, z, w.xy)?;
                    model_slice_batches += 1;
                    model_tiles += material.len();
                    secs.extend(material.into_iter().map(|s| (scene.parts.len(), s)));
                }
                stored_sections = stored_sections.saturating_add(secs.len());
                if stored_sections > scene.model_slice_budget.stored_sections {
                    return Err(
                        "print-order walk exceeds stored material/support section budget"
                            .to_owned(),
                    );
                }
                model::check_foot_starts(&mut state, &secs, starts, z, k, reading);
            }
            let grid = grid_of(&secs);
            let (rooted, reliant) = root_plane(&secs, &grid, all.last().map(Vec::as_slice), &roots);
            let pl = Plane {
                z,
                support_sample,
                secs,
                rooted,
                reliant,
                grid,
            };
            if pl.support_sample {
                let (batches, tiles) = state.plane(&pl, k, !feet.is_empty());
                model_slice_batches += batches;
                model_tiles += tiles;
            }
            planes.push(pl);
        }
        all.push(planes);
    }
    Ok(Walked {
        layers: all,
        z0,
        located: state.located,
        relies_on_model: state.relies_on_model,
        pieces: state.pieces,
        unrooted_pieces: state.unrooted_pieces,
        planes: state.planes_total,
        model_slice_batches,
        model_tiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{closed_box, test_limits};

    #[test]
    fn model_and_foot_events_cannot_displace_a_nearby_support_sample() {
        let h = 1.0;
        let vertex = h + PLANE_DELTA - 5e-10;
        let scene = Scene::from_parts(
            &test_limits(),
            closed_box([-1.0, -1.0, vertex], [1.0, 1.0, 2.0]),
            [0.0; 3],
            vec![],
        );
        let mut extra = Vec::new();
        model::events(&scene, 0.99, 1.01, [-1.0, -1.0, 1.0, 1.0], &mut extra);
        assert!(extra.iter().any(|z| *z < h && (z - h).abs() < ROUND_MM));
        extra.extend([h - 1e-12, h + 1e-12]);
        let planes = proof_planes(&[h], extra);
        assert_eq!(
            planes
                .iter()
                .filter(|(_, primary)| *primary)
                .copied()
                .collect::<Vec<_>>(),
            [(h, true)]
        );
        assert!(
            planes
                .iter()
                .filter(|(_, primary)| !primary)
                .all(|(z, _)| (z - h).abs() > ROUND_MM)
        );
        assert!(planes.len() > 1, "a real model event was retained");
    }
}
