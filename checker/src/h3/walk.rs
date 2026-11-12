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
    let unknown_feet = BTreeSet::new();
    let roots = Roots {
        infos: &infos,
        feet: if slice_model { &unknown_feet } else { &feet },
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
    for (k, starts) in foot_starts.iter().enumerate() {
        let mut planes: Vec<Plane> = Vec::new();
        let mut heights = layer_planes(&infos, k, z0);
        if slice_model {
            let bottom = z0 + k as f64 * LAYER_MM;
            model::events(scene, bottom, bottom + LAYER_MM, &mut heights);
            heights.extend(starts.iter().map(|&(_, z)| z));
            heights.sort_by(f64::total_cmp);
            heights.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        }
        for z in heights {
            let mut secs: Vec<(usize, Sec)> = scene
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
                .collect();
            if slice_model {
                secs.extend(
                    model::sections(scene, z)?
                        .into_iter()
                        .map(|s| (scene.parts.len(), s)),
                );
                if state.pieces.saturating_add(secs.len())
                    > scene.model_slice_budget.stored_sections
                {
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
                secs,
                rooted,
                reliant,
                grid,
            };
            state.plane(&pl, k);
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
    })
}
